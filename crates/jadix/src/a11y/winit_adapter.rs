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

    fn snapshot(children: &[u64], focus: u64) -> TreeUpdate {
        let mut root = accesskit::Node::new(accesskit::Role::Window);
        root.set_children(
            children
                .iter()
                .copied()
                .map(accesskit::NodeId)
                .collect::<Vec<_>>(),
        );
        let mut nodes = vec![(accesskit::NodeId(1), root)];
        nodes.extend(children.iter().map(|&id| {
            let mut node = accesskit::Node::new(accesskit::Role::TextInput);
            node.set_value(format!("value-{id}"));
            (accesskit::NodeId(id), node)
        }));
        TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(accesskit::NodeId(1))),
            tree_id: accesskit::TreeId::ROOT,
            focus: accesskit::NodeId(focus),
        }
    }

    #[test]
    fn semantic_delta_sends_only_changed_nodes_and_keeps_focus_updates() {
        let mut cache = TreeSnapshot::default();
        assert_eq!(cache.delta(snapshot(&[2, 3], 2), false).nodes.len(), 3);
        let unchanged = cache.delta(snapshot(&[2, 3], 3), false);
        assert!(unchanged.nodes.is_empty());
        assert!(unchanged.tree.is_none());
        assert_eq!(unchanged.focus, accesskit::NodeId(3));
        let mut next = snapshot(&[2, 3], 3);
        next.nodes[2].1.set_value("中文👩‍💻");
        let delta = cache.delta(next, false);
        assert_eq!(delta.nodes.len(), 1);
        assert_eq!(delta.nodes[0].0, accesskit::NodeId(3));
    }

    #[test]
    fn semantic_delta_handles_reorder_removal_reinsertion_and_activation_reset() {
        let mut cache = TreeSnapshot::default();
        cache.delta(snapshot(&[2, 3], 2), false);
        let reordered = cache.delta(snapshot(&[3, 2], 2), false);
        assert_eq!(reordered.nodes.len(), 1);
        assert_eq!(
            reordered.nodes[0].1.children(),
            &[accesskit::NodeId(3), accesskit::NodeId(2)]
        );
        assert_eq!(cache.delta(snapshot(&[3], 3), false).nodes.len(), 1);
        assert!(!cache.nodes.contains_key(&accesskit::NodeId(2)));
        assert_eq!(cache.delta(snapshot(&[3, 2], 2), false).nodes.len(), 2);
        let reset = cache.delta(snapshot(&[3, 2], 2), true);
        assert_eq!(reset.nodes.len(), 3);
        assert!(reset.tree.is_some());
        assert_eq!(
            TreeSnapshot::default()
                .delta(snapshot(&[2], 2), false)
                .nodes
                .len(),
            2
        );
    }

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
