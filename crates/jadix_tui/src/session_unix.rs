//! One process terminal owner: bounded nonblocking output and signal intent.
use std::{
    cell::{Cell, RefCell},
    fs::File,
    io::{self, Write},
    os::fd::{AsRawFd, FromRawFd},
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
    time::{Duration, Instant},
};

static OWNED: AtomicBool = AtomicBool::new(false);
static EXIT: AtomicI32 = AtomicI32::new(0);
extern "C" fn signal(signal: libc::c_int) {
    // No allocation, locks, logging, IO or terminal changes in the handler.
    let _ = EXIT.compare_exchange(0, signal, Ordering::Relaxed, Ordering::Relaxed);
}
pub(crate) fn exit_signal() -> Option<i32> {
    if !OWNED.load(Ordering::Acquire) {
        return None;
    }
    let value = EXIT.load(Ordering::Relaxed);
    (value != 0).then_some(value)
}
struct Signals(Vec<(i32, libc::sigaction)>);
impl Signals {
    fn install() -> io::Result<Self> {
        if OWNED
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "a terminal session already owns process signals",
            ));
        }
        EXIT.store(0, Ordering::Relaxed);
        let mut owner = Self(Vec::new());
        for number in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            // SAFETY: initialized POSIX action records; the handler is an
            // allocation-free extern C function and the signal mask is empty.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                let mut previous: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = signal as *const () as usize;
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(number, &action, &mut previous) != 0 {
                    return Err(io::Error::last_os_error());
                }
                owner.0.push((number, previous));
            }
        }
        Ok(owner)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for (number, previous) in self.0.iter().rev() {
            // SAFETY: these are the actions saved by this owner. Do not
            // overwrite a handler another library installed in the meantime.
            unsafe {
                let mut current: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(*number, std::ptr::null(), &mut current) == 0
                    && current.sa_sigaction == signal as *const () as usize
                {
                    libc::sigaction(*number, previous, std::ptr::null_mut());
                }
            }
        }
        EXIT.store(0, Ordering::Relaxed);
        OWNED.store(false, Ordering::Release);
    }
}
pub(crate) struct Session {
    file: File,
    flags: i32,
    signals: RefCell<Option<Signals>>,
    closed: Cell<bool>,
    failed: Cell<bool>,
    cleaning: Cell<bool>,
    written: Cell<u64>,
    last_signal: Cell<Option<i32>>,
    timeout: Duration,
    cancellation: crate::CancellationToken,
}
impl Session {
    pub(crate) fn new(timeout: Duration) -> io::Result<Rc<Self>> {
        if timeout.is_zero() || timeout > Duration::from_secs(3600) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output timeout must be positive and at most one hour",
            ));
        }
        let signals = Signals::install()?;
        // SAFETY: duplicate stdout as a separately owned close-on-exec handle.
        // Its open-file-description flags remain shared with stdout and are
        // restored before this session releases signal ownership.
        let descriptor = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fcntl returned a unique owned descriptor.
        let file = unsafe { File::from_raw_fd(descriptor) };
        // SAFETY: file owns a live descriptor; only nonblocking status changes.
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Rc::new(Self {
            file,
            flags,
            signals: RefCell::new(Some(signals)),
            closed: Cell::new(false),
            failed: Cell::new(false),
            cleaning: Cell::new(false),
            written: Cell::new(0),
            last_signal: Cell::new(None),
            timeout,
            cancellation: crate::CancellationToken::new(),
        }))
    }
    pub(crate) fn cancellation(&self) -> crate::CancellationToken {
        self.cancellation.clone()
    }
    pub(crate) fn signal(&self) -> Option<i32> {
        self.last_signal.get().or_else(|| {
            if self.closed.get() {
                None
            } else {
                exit_signal()
            }
        })
    }
    pub(crate) fn written_bytes(&self) -> u64 {
        self.written.get()
    }
    fn failed(&self, error: io::Error) -> io::Error {
        self.failed.set(true);
        error
    }
    pub(crate) fn check_write(&self) -> io::Result<()> {
        if self.closed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.cleaning.get() {
            return Ok(());
        }
        if self.failed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if exit_signal().is_some() || self.cancellation.is_cancelled() {
            return Err(self.failed(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "terminal shutdown requested",
            )));
        }
        Ok(())
    }
    pub(crate) fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        self.check_write()?;
        if self.cleaning.get() {
            let n = (&self.file).write(bytes)?;
            self.written
                .set(self.written.get().saturating_add(n as u64));
            return Ok(n);
        }
        let deadline = Instant::now() + self.timeout;
        loop {
            if exit_signal().is_some() || self.cancellation.is_cancelled() {
                // Write::write_all retries Interrupted indefinitely. Shutdown
                // therefore uses ConnectionAborted, including partial frames.
                return Err(self.failed(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "terminal shutdown requested",
                )));
            }
            match (&self.file).write(bytes) {
                Ok(n) => {
                    self.written
                        .set(self.written.get().saturating_add(n as u64));
                    return Ok(n);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(self.failed(e)),
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(self.failed(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "terminal output stalled",
                )));
            }
            let mut fd = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: one caller-owned poll record, borrowed only for this call.
            let result = unsafe {
                libc::poll(
                    &mut fd,
                    1,
                    left.min(Duration::from_millis(16)).as_millis().max(1) as i32,
                )
            };
            if result < 0 {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(self.failed(e));
                }
            }
            if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(self.failed(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "terminal output disconnected",
                )));
            }
        }
    }
    /// Best effort only: a full output queue cannot accept escape cleanup.
    /// The caller must still restore termios and flags when this fails.
    pub(crate) fn cleanup(&self, bytes: &[u8]) -> io::Result<()> {
        let mut left = bytes;
        while !left.is_empty() {
            let n = (&self.file).write(left)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            self.written
                .set(self.written.get().saturating_add(n as u64));
            left = &left[n..];
        }
        Ok(())
    }
    pub(crate) fn is_cleaning(&self) -> bool {
        self.cleaning.get()
    }
    pub(crate) fn begin_cleanup(&self) -> io::Result<()> {
        if self.closed.get() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self.cleaning.replace(true) {
            return Ok(());
        }
        self.cleanup(b"\x18\x1b\\")
    }
    pub(crate) fn finish(&self) -> io::Result<()> {
        if self.closed.replace(true) {
            return Ok(());
        }
        self.last_signal.set(exit_signal());
        // Disable every cloned writer before restoring blocking flags, so a
        // late Ratatui/graphics Drop cannot block again after teardown.
        let result = unsafe { libc::fcntl(self.file.as_raw_fd(), libc::F_SETFL, self.flags) };
        let error = (result < 0).then(io::Error::last_os_error);
        self.signals.borrow_mut().take();
        error.map_or(Ok(()), Err)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
