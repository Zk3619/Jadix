//! Optional window lifecycle observer (create / move / resize / close / exit).
//!
//! Persistence is **not** in this crate. `jadix_window_state` (or any app) implements
//! [`WindowLifecycleHook`] and registers it with [`set_window_lifecycle_hook`] before
//! [`super::run`]. Same layering as a future `IconProvider`: core keeps a thin slot.

use std::sync::{Arc, Mutex};

use winit::event::WindowEvent;
use winit::window::{Window, WindowAttributes};

/// Observer invoked from the winit event loop.
///
/// All methods run on the UI thread. Implementors must be `Send + Sync` so an
/// app can install the hook from `main` before `run`.
pub trait WindowLifecycleHook: Send + Sync {
    /// Adjust initial geometry before the native window and its first surface exist.
    /// Keep `visible` false; the renderer presents the first frame before showing it.
    fn on_creating(&self, _label: &str, _attributes: &mut WindowAttributes) {}
    /// Window exists, is still hidden (`WindowOptions` uses `with_visible(false)`).
    /// Restore geometry here so the first layout/swapchain sees the saved size.
    fn on_created(&self, window: &dyn Window, label: &str);
    fn on_event(&self, window: &dyn Window, event: &WindowEvent);
    /// Actual selected adapter and configured surface, including backend fallback.
    /// Invoked once per window initialization; never on the measured frame path.
    fn on_gpu_ready(
        &self,
        _window: &dyn Window,
        _adapter: &wgpu::Adapter,
        _surface: &wgpu::SurfaceConfiguration,
    ) {
    }
    /// Return false to cancel an OS/custom-chrome close or application Quit.
    /// Runs in the target window's scope, with UI/GPU state unborrowed. To ask
    /// asynchronously, return false, show a dialog, then retry after saving.
    /// Closing the primary checks it and its dependent windows before any teardown.
    /// Quitting checks ALL live windows, including independent secondary windows.
    /// `Destroyed` / forced OS termination cannot be cancelled.
    fn on_close_requested(&self, _window: &dyn Window) -> bool {
        true
    }
    /// Last chance to snapshot before the `Window` is dropped.
    fn on_closing(&self, window: &dyn Window);
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

pub(crate) fn prepare_attributes(
    mut attrs: WindowAttributes,
    label: Option<&str>,
) -> WindowAttributes {
    if let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(hook) = current() {
            hook.on_creating(label, &mut attrs);
        }
    }
    attrs.visible = false;
    attrs
}

pub(crate) fn emit_created(window: &dyn Window, label: Option<&str>) {
    let Some(label) = label.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if let Some(h) = current() {
        h.on_created(window, label);
    }
}

pub(crate) fn emit_event(window: &dyn Window, event: &WindowEvent) {
    if let Some(h) = current() {
        h.on_event(window, event);
    }
}

pub(crate) fn emit_gpu_ready(
    window: &dyn Window,
    adapter: &wgpu::Adapter,
    surface: &wgpu::SurfaceConfiguration,
) {
    if let Some(h) = current() {
        h.on_gpu_ready(window, adapter, surface);
    }
}

pub(crate) fn emit_closing(window: &dyn Window) {
    if let Some(h) = current() {
        h.on_closing(window);
    }
}

pub(crate) fn close_requested(window: &dyn Window) -> bool {
    current().is_none_or(|h| h.on_close_requested(window))
}

pub(crate) fn emit_exit() {
    if let Some(h) = current() {
        h.on_exit();
    }
}
