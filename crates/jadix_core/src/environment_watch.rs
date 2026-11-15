//! OS notifications deliver the latest bounded snapshot, never UI state.
use super::EnvironmentSnapshot;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

/// One owner, one pending snapshot, one OS worker. The wake callback must be
/// nonblocking and only notify the host (e.g. its existing bounded mailbox).
/// Drain `take` before the host blocks; changes coalesce to the newest snapshot.
pub struct SystemEnvironmentWatcher {
    pending: Arc<Mutex<Option<Result<EnvironmentSnapshot, String>>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    token: usize,
}
impl SystemEnvironmentWatcher {
    pub fn start(notify: impl Fn() + Send + 'static) -> io::Result<Self> {
        #[cfg(not(all(
            feature = "system-environment",
            any(windows, all(target_os = "linux", not(target_env = "ohos")))
        )))]
        {
            let _ = notify;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "system environment notifications unavailable",
            ))
        }
        #[cfg(all(
            feature = "system-environment",
            any(windows, all(target_os = "linux", not(target_env = "ohos")))
        ))]
        {
            let pending = Arc::new(Mutex::new(None));
            let stop = Arc::new(AtomicBool::new(false));
            let slot = pending.clone();
            let shutdown = stop.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let thread = std::thread::Builder::new()
                .name("jadix-environment".into())
                .spawn(move || {
                    let mut previous = None;
                    let changed = || {
                        let next = super::system_snapshot().map_err(|e| e.to_string());
                        if previous.as_ref() == Some(&next) {
                            return;
                        }
                        previous = Some(next.clone());
                        *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(next);
                        notify();
                    };
                    #[cfg(windows)]
                    super::windows::run_watch(shutdown.clone(), tx, changed);
                    #[cfg(all(target_os = "linux", not(target_env = "ohos")))]
                    super::linux::run_watch(shutdown.clone(), tx, changed);
                    if !shutdown.load(Ordering::Acquire) {
                        *slot.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(Err("system environment listener stopped".into()));
                        notify();
                    }
                })?;
            match rx.recv() {
                Ok(Ok(token)) => Ok(Self {
                    pending,
                    stop,
                    thread: Some(thread),
                    token,
                }),
                Ok(Err(e)) => {
                    let _ = thread.join();
                    Err(e)
                }
                Err(e) => {
                    let _ = thread.join();
                    Err(io::Error::other(e))
                }
            }
        }
    }
    pub fn take(&self) -> Option<io::Result<EnvironmentSnapshot>> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .map(|v| v.map_err(io::Error::other))
    }
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self.thread.is_none() {
            return;
        }
        #[cfg(all(windows, feature = "system-environment"))]
        super::windows::stop_watch(self.token);
        #[cfg(not(all(windows, feature = "system-environment")))]
        let _ = self.token;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}
impl Drop for SystemEnvironmentWatcher {
    fn drop(&mut self) {
        self.close();
    }
}
