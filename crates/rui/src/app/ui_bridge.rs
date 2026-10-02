//! Background thread -> UI thread bridge via [`EventLoopProxy::wake_up`].
//!
//! Worker threads cannot touch Signals / Elements. Queue a `Send` closure with
//! [`run_on_ui_thread`]; the event loop drains it on [`crate::app::RuiUserEvent::UiWake`].

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use winit::event_loop::EventLoopProxy;

use super::RuiUserEvent;

static PROXY: OnceLock<EventLoopProxy> = OnceLock::new();
static USER_TX: OnceLock<Sender<RuiUserEvent>> = OnceLock::new();
static PENDING: Mutex<VecDeque<Box<dyn FnOnce() + Send>>> = Mutex::new(VecDeque::new());

pub(super) fn install_proxy(proxy: EventLoopProxy, tx: Sender<RuiUserEvent>) {
    let _ = PROXY.set(proxy);
    let _ = USER_TX.set(tx);
}

pub(crate) fn send_user_event(event: RuiUserEvent) -> Result<(), String> {
    let tx = USER_TX
        .get()
        .ok_or_else(|| "event loop proxy not installed (call rui::run first)".to_string())?;
    tx.send(event)
        .map_err(|_| "event loop user-event channel closed".to_string())?;
    if let Some(proxy) = PROXY.get() {
        proxy.wake_up();
    }
    Ok(())
}

/// Schedule `f` on the UI thread and wake the event loop.
///
/// If the app has not started yet, the closure is still queued and will run after the first
/// [`install_proxy`] + wake (callers that need immediate delivery should ensure `run` has started).
pub fn run_on_ui_thread(f: impl FnOnce() + Send + 'static) {
    {
        let mut q = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        q.push_back(Box::new(f));
    }
    let _ = send_user_event(RuiUserEvent::UiWake);
}

/// Ask the event loop to shut down (same path as closing the primary window).
///
/// Tray **Quit** should call this from `on_click` after the native hop via
/// [`run_on_ui_thread`]. No-ops with a warn if [`install_proxy`] has not run yet.
pub fn request_exit() {
    if send_user_event(RuiUserEvent::Exit).is_err() {
        log::warn!("rui: request_exit ignored (event loop not started)");
    }
}

/// Drain and run all pending UI callbacks (called from the event loop).
pub(crate) fn drain_ui_tasks() {
    let tasks: Vec<Box<dyn FnOnce() + Send>> = {
        let mut q = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    };
    for task in tasks {
        crate::core::reactivity::with_ui_build_stack(task);
    }
}

/// Serialize tests that drain the global UI task queue (resource / ui_bridge).
#[cfg(test)]
pub(crate) static UI_QUEUE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Spawn a background thread that eventually posts results back with [`run_on_ui_thread`].
pub fn spawn_blocking<T, F, C>(work: F, on_ui: C)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    C: FnOnce(T) + Send + 'static,
{
    std::thread::spawn(move || {
        let result = work();
        run_on_ui_thread(move || on_ui(result));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn queue_drains_without_proxy() {
        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let flag = Arc::new(Mutex::new(false));
        let flag2 = Arc::clone(&flag);
        run_on_ui_thread(move || {
            *flag2.lock().unwrap() = true;
        });
        drain_ui_tasks();
        assert!(*flag.lock().unwrap());
    }

    #[test]
    fn request_exit_without_proxy_does_not_panic() {
        request_exit();
    }
}
