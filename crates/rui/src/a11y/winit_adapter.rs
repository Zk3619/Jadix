//! Local AccessKit ↔ winit 0.31 hook.
//!
//! Upstream `accesskit_winit` still depends on winit 0.30 (`EventLoopProxy<T>`).
//! This stub keeps feature `a11y` compiling: the tree bridge still runs;
//! OS platform adapters land when AccessKit publishes a 0.31 crate.

use accesskit::{ActivationHandler, TreeUpdate};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

pub struct Event {
    pub window_id: WindowId,
    pub window_event: AdapterWindowEvent,
}

#[derive(Debug)]
pub enum AdapterWindowEvent {
    ActionRequested(accesskit::ActionRequest),
    InitialTreeRequested,
}

pub struct Adapter;

impl Adapter {
    pub fn with_mixed_handlers(
        _event_loop: &dyn ActiveEventLoop,
        _window: &dyn Window,
        _activation: impl ActivationHandler,
        _proxy: EventLoopProxy,
    ) -> Self {
        Self
    }

    pub fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        let _ = update_factory();
    }
}
