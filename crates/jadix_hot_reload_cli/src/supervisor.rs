use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};

use super::options::Options;

static STOP: AtomicBool = AtomicBool::new(false);

pub(super) fn stop_requested() -> bool {
    STOP.load(Ordering::Relaxed)
}

#[cfg(windows)]
fn install_stop_handler() {
    unsafe extern "system" fn handler(event: u32) -> i32 {
        if event <= 1 {
            STOP.store(true, Ordering::Relaxed);
            1
        } else {
            0
        }
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> i32>,
            add: i32,
        ) -> i32;
    }
    unsafe {
        SetConsoleCtrlHandler(Some(handler), 1);
    }
}

#[cfg(unix)]
fn install_stop_handler() {
    unsafe extern "C" fn handler(_: i32) {
        STOP.store(true, Ordering::Relaxed);
    }
    unsafe extern "C" {
        fn signal(number: i32, handler: usize) -> usize;
    }
    unsafe {
        signal(2, handler as *const () as usize);
        signal(15, handler as *const () as usize);
    }
}

#[cfg(not(any(windows, unix)))]
fn install_stop_handler() {}

pub(super) struct Supervisor {
    command: Vec<String>,
    root: PathBuf,
    child: Option<Child>,
    sources: HashMap<PathBuf, Option<String>>,
}

impl Supervisor {
    pub fn new(options: &Options) -> io::Result<Self> {
        install_stop_handler();
        let mut sources = HashMap::new();
        {
            for root in &options.watch {
                for file in super::collect_recent_tracked_files(root, u64::MAX)
                    .into_iter()
                    .filter(|file| options.tracks(file))
                {
                    sources.insert(file.clone(), file_signature(&file));
                }
            }
        }
        let mut supervisor = Self {
            command: options.run.clone(),
            root: options.root.clone(),
            child: None,
            sources,
        };
        if !supervisor.command.is_empty() {
            supervisor.restart()?;
        }
        Ok(supervisor)
    }

    pub fn needs_restart(&mut self, files: &[PathBuf]) -> bool {
        let exited = !self.command.is_empty()
            && self
                .child
                .as_mut()
                .is_none_or(|child| child.try_wait().ok().flatten().is_some());
        let mut restart = exited;
        for file in files {
            let signature = file_signature(file);
            let previous = self.sources.get(file);
            if super::is_rust_source(file)
                || file
                    .file_name()
                    .is_some_and(|name| name == "Cargo.toml" || name == "Cargo.lock")
            {
                restart |= signature.is_none() || previous != Some(&signature);
            }
        }
        restart
    }

    pub fn restart_if_managed(&mut self) -> io::Result<bool> {
        if self.command.is_empty() {
            return Ok(false);
        }
        self.restart()?;
        Ok(true)
    }

    pub fn accept_sources(&mut self, files: &[PathBuf]) {
        for file in files {
            self.sources.insert(file.clone(), file_signature(file));
        }
    }

    pub fn restart(&mut self) -> io::Result<()> {
        self.stop();
        super::hmr_log::update(&format!("rebuild/run: {}", self.command.join(" ")));
        let mut command = Command::new(&self.command[0]);
        command.args(&self.command[1..]).current_dir(&self.root);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        self.child = Some(command.spawn()?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().ok().flatten().is_none() {
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    let _ = Command::new("taskkill")
                        .args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .creation_flags(0x08000000)
                        .output();
                }
                #[cfg(unix)]
                {
                    let _ = Command::new("kill")
                        .args(["-TERM", "--", &format!("-{}", child.id())])
                        .status();
                }
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn file_signature(path: &std::path::Path) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    if super::is_rust_source(path) {
        rust_signature(&source)
    } else {
        Some(source)
    }
}

fn rust_signature(source: &str) -> Option<String> {
    jadix_hot_reload::syntax::source_signature(source)
}

#[cfg(test)]
#[path = "tests/supervisor.rs"]
mod tests;
