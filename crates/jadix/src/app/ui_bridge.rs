//! Background thread -> UI thread bridge via [`EventLoopProxy::wake_up`].
//!
//! Worker threads cannot touch Signals / Elements. Queue a `Send` closure with
//! [`run_on_ui_thread`]; each proxy wake drains a bounded FIFO batch.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

use winit::event_loop::EventLoopProxy;

use super::JadixUserEvent;

static PROXY: OnceLock<EventLoopProxy> = OnceLock::new();
static USER_TX: OnceLock<Sender<JadixUserEvent>> = OnceLock::new();
static PENDING: Mutex<VecDeque<Box<dyn FnOnce() + Send>>> = Mutex::new(VecDeque::new());
pub(super) const UI_TASK_BATCH: usize = 64;

pub(super) fn install_proxy(proxy: EventLoopProxy, tx: Sender<JadixUserEvent>) {
    let _ = PROXY.set(proxy);
    let _ = USER_TX.set(tx);
    // Work queued before startup must not wait for an unrelated OS event.
    if !PENDING.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        wake();
    }
}

pub(super) fn wake() {
    if let Some(proxy) = PROXY.get() {
        proxy.wake_up();
    }
}

pub(crate) fn send_user_event(event: JadixUserEvent) -> Result<(), String> {
    let tx = USER_TX
        .get()
        .ok_or_else(|| "event loop proxy not installed (call jadix::run first)".to_string())?;
    tx.send(event)
        .map_err(|_| "event loop user-event channel closed".to_string())?;
    wake();
    Ok(())
}

/// Schedule `f` on the UI thread and wake the event loop.
///
/// If the app has not started yet, the closure is still queued and will run after the first
/// [`install_proxy`]. Only an empty-to-nonempty transition needs a proxy wake;
/// subsequent callbacks join that batch without allocating extra user events.
/// Long-running callbacks still block the UI thread; use [`spawn_blocking`] for work.
pub fn run_on_ui_thread(f: impl FnOnce() + Send + 'static) {
    let needs_wake = {
        let mut q = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        let empty = q.is_empty();
        q.push_back(Box::new(f));
        empty
    };
    if needs_wake {
        wake();
    }
}

/// Ask the event loop to shut down (same path as closing the primary window).
///
/// Tray **Quit** should call this from `on_click` after the native hop via
/// [`run_on_ui_thread`]. No-ops with a warn if [`install_proxy`] has not run yet.
pub fn request_exit() {
    if send_user_event(JadixUserEvent::Exit).is_err() {
        log::warn!("jadix: request_exit ignored (event loop not started)");
    }
}

/// Run one FIFO batch, outside the mutex. Nested posts run on a later turn.
/// Re-arm a wake while work remains, giving OS input/paint a turn between batches.
pub(crate) fn drain_ui_tasks() -> usize {
    let window_tasks = super::window_session::drain_tasks();
    let tasks: Vec<Box<dyn FnOnce() + Send>> = {
        let mut q = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        let count = q.len().min(UI_TASK_BATCH);
        q.drain(..count).collect()
    };
    let count = tasks.len();
    for task in tasks {
        crate::core::reactivity::with_ui_build_stack(task);
    }
    if !PENDING.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        wake();
    }
    count + window_tasks
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

    #[test]
    fn ui_tasks_are_bounded_fifo_and_nested_posts_wait_for_next_turn() {
        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        while drain_ui_tasks() != 0 {}
        let seen = Arc::new(Mutex::new(Vec::new()));
        let total = UI_TASK_BATCH * 3 + 7;
        for i in 0..total {
            let seen = seen.clone();
            run_on_ui_thread(move || seen.lock().unwrap().push(i));
        }
        assert_eq!(drain_ui_tasks(), UI_TASK_BATCH);
        assert_eq!(seen.lock().unwrap().len(), UI_TASK_BATCH);
        while drain_ui_tasks() != 0 {}
        assert_eq!(*seen.lock().unwrap(), (0..total).collect::<Vec<_>>());

        let seen2 = seen.clone();
        run_on_ui_thread(move || {
            run_on_ui_thread(move || seen2.lock().unwrap().push(total));
        });
        assert_eq!(drain_ui_tasks(), 1);
        assert_eq!(seen.lock().unwrap().len(), total);
        assert_eq!(drain_ui_tasks(), 1);
        assert_eq!(seen.lock().unwrap().last(), Some(&total));
    }
}
