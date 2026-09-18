//! Optional window lifecycle observer (create / move / resize / close / exit).
//!
//! Persistence is **not** in this crate. `jadix_window_state` (or any app) implements
//! [`WindowLifecycleHook`] and registers it with [`set_window_lifecycle_hook`] before
//! [`super::run`]. Same layering as a future `IconProvider`: core keeps a thin slot.

use std::sync::{Arc, Mutex};

use winit::event::WindowEvent;
use winit::window::Window;

/// Observer invoked from the winit event loop.
///
/// All methods run on the UI thread. Implementors must be `Send + Sync` so an
/// app can install the hook from `main` before `run`.
pub trait WindowLifecycleHook: Send + Sync {
    /// Window exists, is still hidden (`WindowOptions` uses `with_visible(false)`).
    /// Restore geometry here so the first layout/swapchain sees the saved size.
    fn on_created(&self, window: &Window, label: &str);
    fn on_event(&self, window: &Window, event: &WindowEvent);
    /// Last chance to snapshot before the `Window` is dropped.
    fn on_closing(&self, window: &Window);
    /// Process is leaving the event loop; flush to disk.
    fn on_exit(&self);
}

fn slot() -> &'static Mutex<Option<Arc<dyn WindowLifecycleHook>>> {
    static SLOT: Mutex<Option<Arc<dyn WindowLifecycleHook>>> = Mutex::new(None);
    &SLOT
}

fn current() -> Option<Arc<dyn WindowLifecycleHook>> {
    slot().lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Install or clear the process-wide hook. Call from `main` before [`super::run`].
pub fn set_window_lifecycle_hook(hook: Option<Arc<dyn WindowLifecycleHook>>) {
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = hook;
}

pub(crate) fn emit_created(window: &Window, label: Option<&str>) {
    let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if let Some(h) = current() {
        h.on_created(window, label);
    }
}

pub(crate) fn emit_event(window: &Window, event: &WindowEvent) {
    if let Some(h) = current() {
        h.on_event(window, event);
    }
}

pub(crate) fn emit_closing(window: &Window) {
    if let Some(h) = current() {
        h.on_closing(window);
    }
}

pub(crate) fn emit_exit() {
    if let Some(h) = current() {
        h.on_exit();
    }
}
