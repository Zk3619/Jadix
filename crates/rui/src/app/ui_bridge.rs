//! Background thread -> UI thread bridge via [`EventLoopProxy`].
//!
//! Worker threads cannot touch Signals / Elements. Queue a `Send` closure with
//! [`run_on_ui_thread`]; the event loop drains it on [`crate::app::RuiUserEvent::UiWake`].

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use winit::event_loop::EventLoopProxy;

use super::RuiUserEvent;

static PROXY: OnceLock<EventLoopProxy<RuiUserEvent>> = OnceLock::new();
static PENDING: Mutex<VecDeque<Box<dyn FnOnce() + Send>>> = Mutex::new(VecDeque::new());

pub(super) fn install_proxy(proxy: EventLoopProxy<RuiUserEvent>) {
    let _ = PROXY.set(proxy);
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
    if let Some(proxy) = PROXY.get() {
        let _ = proxy.send_event(RuiUserEvent::UiWake);
    }
}

/// Drain and run all pending UI callbacks (called from the event loop).
pub(super) fn drain_ui_tasks() {
    let tasks: Vec<Box<dyn FnOnce() + Send>> = {
        let mut q = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        q.drain(..).collect()
    };
    for task in tasks {
        task();
    }
}

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
        let flag = Arc::new(Mutex::new(false));
        let flag2 = Arc::clone(&flag);
        run_on_ui_thread(move || {
            *flag2.lock().unwrap() = true;
        });
        drain_ui_tasks();
        assert!(*flag.lock().unwrap());
    }
}
