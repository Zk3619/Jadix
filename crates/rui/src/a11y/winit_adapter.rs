//! AccessKit ↔ winit 0.31.
//!
//! Upstream `accesskit_winit` still depends on winit 0.30 (`EventLoopProxy<T>`).
//! winit 0.31 only wakes the loop; actions travel on the app user-event channel.
//! Windows uses `accesskit_windows::SubclassingAdapter`. Other targets keep a
//! no-op adapter so `a11y` still builds.

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

#[derive(Debug)]
pub struct Event {
    pub window_id: WindowId,
    pub window_event: AdapterWindowEvent,
}

#[derive(Debug)]
pub enum AdapterWindowEvent {
    ActionRequested(accesskit::ActionRequest),
    InitialTreeRequested,
}

struct ChannelActionHandler {
    window_id: WindowId,
}

impl ActionHandler for ChannelActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = crate::app::ui_bridge::send_user_event(crate::app::RuiUserEvent::AccessKit(Event {
            window_id: self.window_id,
            window_event: AdapterWindowEvent::ActionRequested(request),
        }));
    }
}

#[cfg(windows)]
mod platform {
    use super::{ActivationHandler, ChannelActionHandler, TreeUpdate, Window};
    use accesskit_windows::{HWND, SubclassingAdapter};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    pub struct PlatformAdapter {
        adapter: SubclassingAdapter,
    }

    impl PlatformAdapter {
        pub fn new(
            window: &dyn Window,
            activation: impl 'static + ActivationHandler,
            action: ChannelActionHandler,
        ) -> Self {
            let hwnd = hwnd_of(window).unwrap_or_else(|| {
                panic!("AccessKit requires a Win32 window handle");
            });
            Self {
                adapter: SubclassingAdapter::new(hwnd, activation, action),
            }
        }

        pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(update_factory) {
                events.raise();
            }
        }
    }

    fn hwnd_of(window: &dyn Window) -> Option<HWND> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return None;
        };
        Some(HWND(h.hwnd.get() as *mut core::ffi::c_void))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{ActivationHandler, ChannelActionHandler, TreeUpdate, Window};

    pub struct PlatformAdapter;

    impl PlatformAdapter {
        pub fn new(
            _window: &dyn Window,
            _activation: impl 'static + ActivationHandler,
            _action: ChannelActionHandler,
        ) -> Self {
            Self
        }

        pub fn update_if_active(&mut self, _update_factory: impl FnOnce() -> TreeUpdate) {}
    }
}

pub struct Adapter {
    inner: platform::PlatformAdapter,
}

impl Adapter {
    pub fn with_mixed_handlers(
        _event_loop: &dyn ActiveEventLoop,
        window: &dyn Window,
        activation: impl 'static + ActivationHandler,
        _proxy: EventLoopProxy,
    ) -> Self {
        let action = ChannelActionHandler {
            window_id: window.id(),
        };
        Self {
            inner: platform::PlatformAdapter::new(window, activation, action),
        }
    }

    pub fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}

    /// Push a tree only when a platform adapter is subscribed (a screen reader
    /// asked for the tree). The factory is not called otherwise.
    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        self.inner.update_if_active(update_factory);
    }
}
