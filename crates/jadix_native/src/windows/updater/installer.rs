//! The helper runs from an exclusively created temporary directory so the
//! application's executable and DLLs are unlocked before the directory swap.
use jadix_core::updater::installation::Staged;
use std::{
    fs,
    io::{Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};

const FLAG: &str = "--jadix-apply-update";
const PREFIX: &str = ".jadix-update-helper-";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    plan: PathBuf,
    parent: u32,
    created: u64,
    restart: bool,
    arguments: Vec<Vec<u16>>,
}
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn creation(process: HANDLE) -> Result<u64, String> {
    let mut times = [FILETIME::default(); 4];
    let [created, exited, kernel, user] = &mut times;
    if unsafe { GetProcessTimes(process, created, exited, kernel, user) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok((u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime))
}
fn wait_parent(parent: u32, created: u64) -> Result<(), String> {
    if parent == std::process::id() || parent == 0 {
        return Err("invalid updater parent".into());
    }
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            parent,
        )
    };
    if raw.is_null() {
        // ERROR_INVALID_PARAMETER means this PID has already exited. Access
        // denial is not evidence of exit and must leave the installation alone.
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        };
    }
    let process = Process(raw);
    if creation(process.0)? != created {
        return Ok(());
    } // Reused PID.
    if unsafe { WaitForSingleObject(process.0, INFINITE) } != WAIT_OBJECT_0 {
        return Err("waiting for updater parent failed".into());
    }
    Ok(())
}
fn reserve() -> Result<PathBuf, String> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    for _ in 0..32 {
        let directory = std::env::temp_dir().join(format!(
            "{PREFIX}{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::create_dir(&directory) {
            Ok(()) => return directory.canonicalize().map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("cannot reserve update helper directory".into())
}
fn remove_helper(directory: &Path) {
    // Never recursively remove a directory supplied in command-line arguments.
    // A running helper's image remains locked; a later application startup
    // retries cleanup. Unknown files are preserved.
    if fs::remove_file(directory.join("helper.exe")).is_ok()
        || !directory.join("helper.exe").exists()
    {
        for name in ["bootstrap.json", "ready", "status.json"] {
            let _ = fs::remove_file(directory.join(name));
        }
        let _ = fs::remove_dir(directory);
    }
}
pub(super) fn cleanup_helpers() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with(PREFIX))
        .take(64)
    {
        // Another root (or application process) may still be preparing this
        // helper. Only reap directories whose reserving process has exited.
        let name = entry.file_name();
        let Some(owner) = name
            .to_str()
            .and_then(|s| s.strip_prefix(PREFIX))
            .and_then(|s| s.split_once('-'))
            .and_then(|(pid, _)| pid.parse::<u32>().ok())
            .filter(|pid| *pid != 0)
        else {
            continue;
        };
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, owner) };
        let exited = if process.is_null() {
            unsafe { GetLastError() == ERROR_INVALID_PARAMETER }
        } else {
            let process = Process(process);
            unsafe { WaitForSingleObject(process.0, 0) == WAIT_OBJECT_0 }
        };
        if exited
            && entry
                .file_type()
                .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink())
        {
            remove_helper(&entry.path());
        }
    }
}
pub(super) fn launch(staged: Staged, restart: bool) -> Result<(), String> {
    let helper = reserve()?;
    let result = (|| {
        let executable = helper.join("helper.exe");
        let current = std::env::current_exe().map_err(|e| e.to_string())?;
        // reserve() exclusively owns this directory; do not copy into an
        // arbitrary existing path. fs::copy's destination is therefore new.
        fs::copy(current, &executable).map_err(|e| e.to_string())?;
        let parent = std::process::id();
        let created = creation(unsafe { GetCurrentProcess() })?;
        let arguments: Vec<Vec<u16>> = std::env::args_os()
            .skip(1)
            .map(|arg| arg.encode_wide().collect())
            .collect();
        staged.handoff(|plan| {
            let bytes = serde_json::to_vec(&Bootstrap {
                plan: plan.into(),
                parent,
                created,
                restart,
                arguments,
            })
            .map_err(|e| e.to_string())?;
            if bytes.len() > 1024 * 1024 {
                return Err("update restart arguments exceed limit".into());
            }
            let bootstrap = helper.join("bootstrap.json");
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&bootstrap)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            let mut child = Command::new(executable)
                .arg(FLAG)
                .arg(bootstrap)
                .current_dir(&helper)
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| e.to_string())?;
            await_ready(&mut child, &helper)
        })
    })();
    if result.is_err() {
        remove_helper(&helper);
    }
    result
}
fn await_ready(child: &mut std::process::Child, directory: &Path) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!(
                "update helper exited before accepting the transaction: {status}"
            ));
        }
        if directory.join("ready").is_file() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("update helper did not accept the transaction within 5 seconds".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
/// Call at the start of `main`, before parsing application arguments or creating
/// windows. Return from `main` when this returns true. `WindowsHost::new` also
/// handles this entry for applications without their own argument parser.
/// Updates of applications with custom startup code must call this explicitly.
pub fn run_update_helper() -> Result<bool, String> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new(FLAG)) {
        cleanup_helpers();
        return Ok(false);
    }
    let bootstrap = arguments
        .next()
        .ok_or("missing update helper transaction")?;
    if arguments.next().is_some() {
        return Err("unexpected update helper arguments".into());
    }
    execute(Path::new(&bootstrap))?;
    Ok(true)
}
fn execute(bootstrap: &Path) -> Result<(), String> {
    let current = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    let directory = current.parent().ok_or("missing helper directory")?;
    if current.file_name().is_none_or(|s| s != "helper.exe")
        || !directory
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with(PREFIX))
        || bootstrap != directory.join("bootstrap.json")
        || !bootstrap.is_absolute()
    {
        return Err("invalid update helper location".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(bootstrap)
        .map_err(|e| e.to_string())?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("update helper transaction exceeds limit".into());
    }
    let boot: Bootstrap = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    // A successful CreateProcess does not prove this image could load its DLLs
    // or reach our entry point. Acknowledge before the old application exits.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("ready"))
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    let result = (|| {
        // Wait before acquiring the shared transaction: the old process may
        // still be disposing its runtime after successful handoff.
        wait_parent(boot.parent, boot.created)?;
        let staged = Staged::resume_handoff(&boot.plan)?;
        staged
            .commit(|executable| {
                if boot.restart {
                    Command::new(executable)
                        .args(
                            boot.arguments
                                .iter()
                                .map(|arg| std::ffi::OsString::from_wide(arg)),
                        )
                        .current_dir(executable.parent().ok_or("missing application directory")?)
                        .spawn()
                        .map_err(|e| format!("restart updated application: {e}"))?;
                }
                Ok(())
            })
            .map(|_| ())
    })();
    let status = serde_json::json!({"installed":result.is_ok(), "error":result.as_ref().err()});
    let _ = fs::write(directory.join("status.json"), status.to_string());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_identity_and_invalid_helper_never_touch_installation() {
        let created = creation(unsafe { GetCurrentProcess() }).unwrap();
        assert!(created > 0);
        assert!(wait_parent(std::process::id(), created).is_err());
        assert!(wait_parent(0, created).is_err());
        assert!(execute(Path::new("transaction.json")).is_err());
        let reserved = reserve().unwrap();
        fs::write(reserved.join("helper.exe"), b"pending copy").unwrap();
        cleanup_helpers();
        assert!(
            reserved.join("helper.exe").exists(),
            "a live process still owns its pending helper"
        );
        fs::remove_file(reserved.join("helper.exe")).unwrap();
        fs::remove_dir(reserved).unwrap();
        let mut child = Command::new("cmd.exe")
            .args(["/c", "exit", "0"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        child.wait().unwrap();
        assert!(wait_parent(child.id(), created).is_ok());
        let pending = reserve().unwrap();
        assert!(
            await_ready(&mut child, &pending)
                .unwrap_err()
                .contains("before accepting")
        );
        fs::remove_dir(pending).unwrap();
    }
    #[test]
    #[ignore = "entry point invoked only by the isolated helper process test"]
    fn helper_process_entry() {
        execute(Path::new(
            &std::env::var_os("JADIX_TEST_UPDATE_BOOTSTRAP").expect("helper bootstrap"),
        ))
        .unwrap_or_else(|error| {
            // A restart failure is an expected rollback scenario; execute writes
            // the transaction's actual outcome outside the staging directory.
            eprintln!("helper result: {error}");
        });
    }
    #[test]
    fn external_helper_waits_for_exit_and_rolls_back_failed_restart() {
        use jadix_core::updater::{Config, installation::Installation};
        use std::os::windows::io::AsRawHandle;
        for restart in [false, true] {
            let helper = reserve().unwrap();
            fs::copy(std::env::current_exe().unwrap(), helper.join("helper.exe")).unwrap();
            let root = helper.join("application");
            fs::create_dir(&root).unwrap();
            fs::write(root.join("app.exe"), b"MZold").unwrap();
            fs::write(
                root.join("app.json"),
                br#"{"id":"test.app","version":"1.0.0"}"#,
            )
            .unwrap();
            let config = Config {
                id: "test.app".into(),
                version: "1.0.0".into(),
                github: Some("test/app".into()),
                url: None,
                channels: Default::default(),
                public_key: None,
            };
            let installation =
                Installation::new(&root, Path::new("app.exe"), Path::new("app.json"), &config)
                    .unwrap();
            let archive = helper.join("update.zip");
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            for (name, bytes) in [
                ("app.exe", &b"MZnew"[..]),
                ("app.json", &br#"{"id":"test.app","version":"2.0.0"}"#[..]),
            ] {
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(bytes).unwrap();
            }
            zip.finish().unwrap();
            let staged = installation.stage(&archive, "2.0.0").unwrap();
            let mut parent = Command::new("cmd.exe")
                .args(["/c", "ping -n 3 127.0.0.1 > nul"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .unwrap();
            let boot = Bootstrap {
                plan: PathBuf::new(),
                parent: parent.id(),
                created: creation(parent.as_raw_handle()).unwrap(),
                restart,
                arguments: Vec::new(),
            };
            let bootstrap = helper.join("bootstrap.json");
            let mut child = None;
            staged
                .handoff(|plan| {
                    let boot = Bootstrap {
                        plan: plan.into(),
                        ..boot
                    };
                    fs::write(&bootstrap, serde_json::to_vec(&boot).unwrap()).unwrap();
                    child = Some(
                        Command::new(helper.join("helper.exe"))
                            .args([
                                "--exact",
                                "windows::updater::installer::tests::helper_process_entry",
                                "--ignored",
                                "--nocapture",
                            ])
                            .env("JADIX_TEST_UPDATE_BOOTSTRAP", &bootstrap)
                            .creation_flags(CREATE_NO_WINDOW)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                            .map_err(|e| e.to_string())?,
                    );
                    Ok(())
                })
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(200));
            assert_eq!(fs::read(root.join("app.exe")).unwrap(), b"MZold");
            assert!(
                child.as_mut().unwrap().try_wait().unwrap().is_none(),
                "helper must wait for the parent"
            );
            parent.wait().unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                if child.as_mut().unwrap().try_wait().unwrap().is_some() {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    child.as_mut().unwrap().kill().unwrap();
                    panic!("update helper timed out");
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let status: serde_json::Value =
                serde_json::from_slice(&fs::read(helper.join("status.json")).unwrap()).unwrap();
            assert_eq!(status["installed"], !restart);
            assert_eq!(
                fs::read(root.join("app.exe")).unwrap(),
                if restart {
                    &b"MZold"[..]
                } else {
                    &b"MZnew"[..]
                }
            );
            assert!(!fs::read_dir(&helper).unwrap().flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".jadix-update-")
            }));
            fs::remove_dir_all(helper).unwrap();
        }
    }

    #[test]
    #[ignore = "desktop acceptance: set JADIX_ACCEPTANCE_APPLICATION to the freshly built Windows template"]
    fn template_helper_restarts_real_executable_after_parent_exit() {
        use jadix_core::updater::{Config, installation::Installation};
        use std::os::windows::io::AsRawHandle;
        let source = PathBuf::from(
            std::env::var_os("JADIX_ACCEPTANCE_APPLICATION").expect("template executable"),
        );
        let helper = reserve().unwrap();
        fs::copy(&source, helper.join("helper.exe")).unwrap();
        let root = helper.join("application");
        fs::create_dir(&root).unwrap();
        fs::copy(&source, root.join("app.exe")).unwrap();
        fs::write(
            root.join("app.json"),
            br#"{"id":"test.app","version":"1.0.0"}"#,
        )
        .unwrap();
        let config = Config {
            id: "test.app".into(),
            version: "1.0.0".into(),
            github: Some("test/app".into()),
            url: None,
            channels: Default::default(),
            public_key: None,
        };
        let installation =
            Installation::new(&root, Path::new("app.exe"), Path::new("app.json"), &config).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(helper.join("update.zip")).unwrap());
        for (name, bytes) in [
            ("app.exe", fs::read(&source).unwrap()),
            (
                "app.json",
                br#"{"id":"test.app","version":"2.0.0"}"#.to_vec(),
            ),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        let staged = installation
            .stage(&helper.join("update.zip"), "2.0.0")
            .unwrap();
        let mut parent = Command::new(root.join("app.exe")).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert!(
            parent.try_wait().unwrap().is_none(),
            "real template must be running"
        );
        let probe = helper.join("restarted.json");
        let boot = Bootstrap {
            plan: PathBuf::new(),
            parent: parent.id(),
            created: creation(parent.as_raw_handle()).unwrap(),
            restart: true,
            arguments: [
                std::ffi::OsString::from("--probe"),
                std::ffi::OsString::from("更新重启-😀"),
                probe.as_os_str().to_owned(),
            ]
            .iter()
            .map(|a| a.encode_wide().collect())
            .collect(),
        };
        let mut child = None;
        staged
            .handoff(|plan| {
                fs::write(
                    helper.join("bootstrap.json"),
                    serde_json::to_vec(&Bootstrap {
                        plan: plan.into(),
                        ..boot
                    })
                    .unwrap(),
                )
                .unwrap();
                let mut process = Command::new(helper.join("helper.exe"))
                    .arg(FLAG)
                    .arg(helper.join("bootstrap.json"))
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map_err(|e| e.to_string())?;
                await_ready(&mut process, &helper)?;
                child = Some(process);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(root.join("app.json")).unwrap())
                .unwrap()["version"],
            "1.0.0"
        );
        assert!(child.as_mut().unwrap().try_wait().unwrap().is_none());
        parent.kill().unwrap();
        parent.wait().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while child.as_mut().unwrap().try_wait().unwrap().is_none() || !probe.exists() {
            if std::time::Instant::now() >= deadline {
                let _ = child.as_mut().unwrap().kill();
                panic!(
                    "real updated application did not restart: {}",
                    helper.display()
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let status: serde_json::Value =
            serde_json::from_slice(&fs::read(helper.join("status.json")).unwrap()).unwrap();
        assert_eq!(status["installed"], true, "{status}");
        let probe: serde_json::Value = serde_json::from_slice(&fs::read(probe).unwrap()).unwrap();
        assert_eq!(probe["nonce"], "更新重启-😀");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(root.join("app.json")).unwrap())
                .unwrap()["version"],
            "2.0.0"
        );
        eprintln!(
            "real template updater accepted: {}",
            serde_json::json!({"helper":status,"restarted_probe":probe})
        );
        // The probe receipt is written just before the restarted process exits;
        // Windows may still have its executable mapped at this point.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match fs::remove_dir_all(&helper) {
                Ok(()) => break,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::PermissionDenied
                            | std::io::ErrorKind::DirectoryNotEmpty
                    ) && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(error) => panic!("remove completed update acceptance directory: {error}"),
            }
        }
    }
}
