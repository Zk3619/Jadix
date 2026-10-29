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
#[path = "tests/winit_adapter.rs"]
mod tests;

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
    #[cfg(windows)]
    snapshot: TreeSnapshot,
    #[cfg(windows)]
    reset_snapshot: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(any(windows, test))]
#[derive(Default)]
struct TreeSnapshot {
    nodes: std::collections::HashMap<accesskit::NodeId, accesskit::Node>,
    tree: Option<accesskit::Tree>,
    tree_id: Option<accesskit::TreeId>,
}

#[cfg(any(windows, test))]
impl TreeSnapshot {
    fn delta(&mut self, mut update: TreeUpdate, reset: bool) -> TreeUpdate {
        if reset || self.tree_id != Some(update.tree_id) {
            self.nodes.clear();
            self.tree = None;
        }
        let mut next = std::collections::HashMap::with_capacity(update.nodes.len());
        update.nodes.retain_mut(|(id, node)| {
            if let Some(previous) = self.nodes.remove(id).filter(|previous| previous == node) {
                next.insert(*id, previous);
                false
            } else {
                next.insert(*id, node.clone());
                true
            }
        });
        self.nodes = next;
        self.tree_id = Some(update.tree_id);
        if update.tree == self.tree {
            update.tree = None;
        } else {
            self.tree = update.tree.clone();
        }
        update
    }
}

#[cfg(windows)]
struct ResettingActivation<Handler> {
    handler: Handler,
    reset: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(windows)]
impl<Handler: ActivationHandler> ActivationHandler for ResettingActivation<Handler> {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.reset.store(true, std::sync::atomic::Ordering::Release);
        self.handler.request_initial_tree()
    }
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
        #[cfg(windows)]
        let reset_snapshot = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        #[cfg(windows)]
        let activation = ResettingActivation {
            handler: activation,
            reset: reset_snapshot.clone(),
        };
        Self {
            inner: platform::PlatformAdapter::new(window, activation, action),
            #[cfg(windows)]
            snapshot: TreeSnapshot::default(),
            #[cfg(windows)]
            reset_snapshot,
        }
    }

    pub fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {
        #[cfg(target_env = "ohos")]
        self.inner.process_event(_window, _event);
    }

    /// Push a tree only when a platform adapter is subscribed (a screen reader
    /// asked for the tree). The factory is not called otherwise.
    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        self.inner.update_if_active(|| {
            let _work =
                crate::app::frame_budget::begin_work(crate::app::frame_budget::WorkKind::Semantics);
            let update = update_factory();
            #[cfg(windows)]
            let update = self.snapshot.delta(
                update,
                self.reset_snapshot
                    .swap(false, std::sync::atomic::Ordering::AcqRel),
            );
            update
        });
    }
}
