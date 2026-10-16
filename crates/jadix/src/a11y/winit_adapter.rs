//! AccessKit ↔ winit 0.31.
//!
//! Upstream `accesskit_winit` still depends on winit 0.30 (`EventLoopProxy<T>`).
//! winit 0.31 only wakes the loop; actions travel on the app user-event channel.
//! Windows subclasses the HWND; OHOS registers a per-surface ArkUI provider.
//! Other targets currently keep a no-op adapter.

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

/// ArkUI provider bounds use physical pixels relative to the XComponent.
#[cfg(any(target_env = "ohos", test))]
pub(super) fn physical_tree_update(mut update: TreeUpdate, scale: f64) -> TreeUpdate {
    if scale.is_finite() && scale > 0.0 && scale != 1.0 {
        if let Some(tree) = &update.tree {
            if let Some((_, root)) = update.nodes.iter_mut().find(|(id, _)| *id == tree.root) {
                root.set_transform(
                    accesskit::Affine::scale(scale)
                        * root
                            .transform()
                            .copied()
                            .unwrap_or(accesskit::Affine::IDENTITY),
                );
            }
        }
    }
    update
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destroyed_surface_actions_cannot_target_a_replacement() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let old = Arc::new(AtomicBool::new(true));
        let mut event = Event {
            window_id: WindowId::from_raw(0),
            window_event: AdapterWindowEvent::InitialTreeRequested,
            active: Some(old.clone()),
        };
        assert!(event.is_current());
        old.store(false, Ordering::Release);
        assert!(!event.is_current());
        event.active = Some(Arc::new(AtomicBool::new(true)));
        assert!(event.is_current());
        assert!(!old.load(Ordering::Acquire));
        event.active = None;
        assert!(event.is_current());
    }

    #[test]
    fn ohos_bounds_scale_once_at_root() {
        use accesskit::{Affine, Node, NodeId, Rect, Role, Tree, TreeId};
        let mut root = Node::new(Role::Window);
        root.set_transform(Affine::translate((3.0, 4.0)));
        let update = TreeUpdate {
            nodes: vec![(NodeId(1), root)],
            tree: Some(Tree::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: NodeId(1),
        };
        let scaled = physical_tree_update(update.clone(), 2.0);
        assert_eq!(
            scaled.nodes[0]
                .1
                .transform()
                .unwrap()
                .transform_rect_bbox(Rect::new(0.0, 0.0, 5.0, 6.0)),
            Rect::new(6.0, 8.0, 16.0, 20.0)
        );
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.0] {
            assert_eq!(
                physical_tree_update(update.clone(), scale).nodes[0]
                    .1
                    .transform(),
                update.nodes[0].1.transform()
            );
        }
    }
}

#[derive(Debug)]
pub struct Event {
    pub window_id: WindowId,
    pub window_event: AdapterWindowEvent,
    pub active: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Event {
    pub(crate) fn is_current(&self) -> bool {
        self.active
            .as_ref()
            .is_none_or(|active| active.load(std::sync::atomic::Ordering::Acquire))
    }
}

#[derive(Debug)]
pub enum AdapterWindowEvent {
    ActionRequested(accesskit::ActionRequest),
    InitialTreeRequested,
}

struct ChannelActionHandler {
    window_id: WindowId,
    active: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl ActionHandler for ChannelActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let _ =
            crate::app::ui_bridge::send_user_event(crate::app::JadixUserEvent::AccessKit(Event {
                window_id: self.window_id,
                window_event: AdapterWindowEvent::ActionRequested(request),
                active: self.active.clone(),
            }));
    }
}

#[cfg(windows)]
mod platform {
    use super::{ActivationHandler, ChannelActionHandler, TreeUpdate, Window};
    use accesskit_windows::{SubclassingAdapter, HWND};
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

#[cfg(target_env = "ohos")]
#[path = "ohos_adapter.rs"]
mod platform;

#[cfg(not(any(windows, target_env = "ohos")))]
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
            active: None,
        };
        Self {
            inner: platform::PlatformAdapter::new(window, activation, action),
        }
    }

    pub fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {
        #[cfg(target_env = "ohos")]
        self.inner.process_event(_window, _event);
    }

    /// Push a tree only when a platform adapter is subscribed (a screen reader
    /// asked for the tree). The factory is not called otherwise.
    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        self.inner.update_if_active(update_factory);
    }
}
