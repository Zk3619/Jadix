//! Desktop process-instance ownership and second-launch delivery.

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InstancePolicy {
    #[default]
    Multiple,
    /// One process per user and stable application/profile id.
    Single { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(unix, windows), derive(serde::Serialize, serde::Deserialize))]
pub struct InstanceLaunch {
    pub args: Vec<OsString>,
    pub working_directory: PathBuf,
}

type LaunchHandler = Rc<RefCell<dyn FnMut(InstanceLaunch)>>;

thread_local! {
    static HANDLER: RefCell<Option<LaunchHandler>> = const { RefCell::new(None) };
}

/// Called on the UI thread after showing/restoring a remaining application window.
pub fn set_second_instance_handler(handler: impl FnMut(InstanceLaunch) + 'static) {
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Rc::new(RefCell::new(handler))));
}

pub(super) fn dispatch(launch: InstanceLaunch) {
    if let Some(handler) = HANDLER.with(|slot| slot.borrow().clone()) {
        handler.borrow_mut()(launch);
    }
}

#[cfg(any(
    windows,
    target_os = "macos",
    all(target_os = "linux", not(target_env = "ohos"))
))]
mod desktop {
    use super::*;
    use std::fs::{File, OpenOptions, TryLockError};
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};

    const MAX_MESSAGE: usize = 64 * 1024;

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Endpoint {
        port: u16,
        token: String,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Message {
        token: String,
        launch: InstanceLaunch,
    }

    pub(in crate::app) struct InstanceGuard {
        _lock: File,
        endpoint: PathBuf,
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for InstanceGuard {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            let _ = std::fs::remove_file(&self.endpoint);
        }
    }

    fn instance_directory() -> io::Result<PathBuf> {
        let home = std::env::var_os(if cfg!(windows) {
            "LOCALAPPDATA"
        } else {
            "HOME"
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "user data directory is unavailable",
            )
        })?;
        let directory = PathBuf::from(home).join(if cfg!(windows) {
            "jadix/instances"
        } else {
            ".cache/jadix/instances"
        });
        std::fs::create_dir_all(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(directory)
    }

    fn identity(id: &str) -> io::Result<String> {
        if id.trim().is_empty() || id.len() > 512 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "instance id must contain 1..512 bytes",
            ));
        }
        let hash = id.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        Ok(format!("{hash:016x}"))
    }

    fn random_token() -> io::Result<String> {
        let mut bytes = [0u8; 32];
        #[cfg(windows)]
        {
            use windows_sys::Win32::Security::Cryptography::{
                BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            };
            let status = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    bytes.as_mut_ptr(),
                    bytes.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if status < 0 {
                return Err(io::Error::other(format!(
                    "system random generator failed: {status}"
                )));
            }
        }
        #[cfg(unix)]
        File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    fn read_before(
        stream: &mut TcpStream,
        mut bytes: &mut [u8],
        deadline: Instant,
    ) -> io::Result<()> {
        while !bytes.is_empty() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "instance message deadline expired")
                })?;
            stream.set_read_timeout(Some(remaining))?;
            match stream.read(bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "incomplete instance message",
                    ))
                }
                Ok(length) => bytes = &mut bytes[length..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn receive(stream: &mut TcpStream, token: &str) -> io::Result<InstanceLaunch> {
        let deadline = Instant::now() + Duration::from_millis(500);
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;
        let mut length = [0u8; 4];
        read_before(stream, &mut length, deadline)?;
        let length = u32::from_be_bytes(length) as usize;
        if length > MAX_MESSAGE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "instance message is too large",
            ));
        }
        let mut bytes = vec![0; length];
        read_before(stream, &mut bytes, deadline)?;
        let message: Message = serde_json::from_slice(&bytes)?;
        if message.token != token {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "invalid instance token",
            ));
        }
        Ok(message.launch)
    }

    pub(in crate::app) fn claim(
        id: &str,
        launch: InstanceLaunch,
        deliver: impl Fn(InstanceLaunch) -> bool + Send + 'static,
    ) -> io::Result<Option<InstanceGuard>> {
        let directory = instance_directory()?;
        let name = identity(id)?;
        let endpoint_path = directory.join(format!("{name}.json"));
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(directory.join(format!("{name}.lock")))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::Error(error)) => return Err(error),
                Err(TryLockError::WouldBlock) => {}
            }
            if let Ok(bytes) = std::fs::read(&endpoint_path) {
                if let Ok(endpoint) = serde_json::from_slice::<Endpoint>(&bytes) {
                    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port);
                    if let Ok(mut stream) =
                        TcpStream::connect_timeout(&address.into(), Duration::from_millis(100))
                    {
                        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                        let bytes = serde_json::to_vec(&Message {
                            token: endpoint.token,
                            launch,
                        })?;
                        if bytes.len() > MAX_MESSAGE {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "instance arguments exceed 64 KiB",
                            ));
                        }
                        stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
                        stream.write_all(&bytes)?;
                        let mut ack = [0];
                        stream.read_exact(&mut ack)?;
                        return if ack == [1] {
                            Ok(None)
                        } else {
                            Err(io::Error::other("existing instance rejected launch"))
                        };
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "existing instance did not publish a usable endpoint",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let token = random_token()?;
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: token.clone(),
        };
        let mut metadata = options.open(&endpoint_path)?;
        metadata.set_len(0)?;
        metadata.write_all(&serde_json::to_vec(&endpoint)?)?;
        metadata.sync_data()?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::Builder::new()
            .name("jadix-instance".into())
            .spawn(move || {
                while !worker_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Ok(launch) = receive(&mut stream, &token) {
                                let accepted = deliver(launch);
                                let _ = stream.write_all(&[u8::from(accepted)]);
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(25))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Some(InstanceGuard {
            _lock: lock,
            endpoint: endpoint_path,
            stop,
            worker: Some(worker),
        }))
    }

    #[cfg(test)]
    #[path = "tests/mod.rs"]
    mod tests;
}

#[cfg(any(
    windows,
    target_os = "macos",
    all(target_os = "linux", not(target_env = "ohos"))
))]
pub(super) use desktop::InstanceGuard;

#[cfg(not(any(
    windows,
    target_os = "macos",
    all(target_os = "linux", not(target_env = "ohos"))
)))]
pub(super) struct InstanceGuard;

pub(super) fn start(
    policy: &InstancePolicy,
    deliver: impl Fn(InstanceLaunch) -> bool + Send + 'static,
) -> io::Result<Option<InstanceGuard>> {
    match policy {
        InstancePolicy::Multiple => Ok(None),
        InstancePolicy::Single { id } => {
            #[cfg(any(
                windows,
                target_os = "macos",
                all(target_os = "linux", not(target_env = "ohos"))
            ))]
            {
                desktop::claim(
                    id,
                    InstanceLaunch {
                        args: std::env::args_os().collect(),
                        working_directory: std::env::current_dir()?,
                    },
                    deliver,
                )
            }
            #[cfg(not(any(
                windows,
                target_os = "macos",
                all(target_os = "linux", not(target_env = "ohos"))
            )))]
            {
                let _ = (id, deliver);
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "single-instance policy is desktop-only",
                ))
            }
        }
    }
}
