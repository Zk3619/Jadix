//! Host integration for a product-owned drag runtime.
//!
//! Install on the UI thread before starting a drag. Tree sampling and reactive
//! dispatch are separate phases: prepared updates own their data and run after
//! the host releases its tree borrow. Replacing or removing a runtime discards
//! its outstanding updates. Registration belongs to the active window on the UI thread.

use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;
use web_time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct RuntimeState {
    pub dragging: bool,
    pub active: bool,
    pub coasting: bool,
    pub activation_deadline: Option<Instant>,
    pub pointer: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveMode {
    Follow,
    Reconcile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKey {
    Cancel,
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollectGeometry {
    pub size: [f32; 2],
    pub origin: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugRect {
    pub rect: [f32; 4],
    pub fill: [f32; 4],
    pub border: [f32; 4],
    pub width: f32,
    pub dashed: bool,
}

pub type Update = Box<dyn FnOnce() -> bool>;

/// Sampling must not invoke reactive callbacks or synchronously remount the tree.
/// Autoscroll sampling may silently adjust scroll offsets before collecting live
/// geometry. Execute all returned updates after releasing the host tree borrow.
pub trait DragRuntime {
    fn state(&self) -> RuntimeState;
    fn prepare_move(
        &self,
        root: Option<&Element>,
        pointer: [f32; 2],
        viewport: [f32; 2],
        mode: MoveMode,
    ) -> Update;
    fn prepare_end(&self, root: &Element) -> Update;
    fn refresh_geometry(&self, root: &Element);
    fn prepare_autoscroll(&self, root: &Element, viewport: [f32; 2]) -> Update;
    fn prepare_inertia(&self, root: &Element, viewport: [f32; 2]) -> Update;

    fn pump_autoscroll(&self, root: &Element, viewport: [f32; 2]) -> bool {
        self.prepare_autoscroll(root, viewport)()
    }

    fn pump_inertia(&self, root: &Element, viewport: [f32; 2]) -> bool {
        self.prepare_inertia(root, viewport)()
    }
    fn key(&self, key: DragKey) -> bool;

    fn sync_tree(&self, _root: &mut Element) {}

    fn after_layout(&self, _root: &mut Element) {}

    fn keeps_collect_live(&self, _element: &Element) -> bool {
        false
    }

    fn collect_geometry(
        &self,
        _element: &mut Element,
        size: [f32; 2],
        _scale: [f32; 2],
    ) -> CollectGeometry {
        CollectGeometry { size, origin: None }
    }

    fn ancestor_clip(&self, _element: &Element) -> Option<[f32; 4]> {
        None
    }

    fn debug_rects(&self, _root: &Element) -> Vec<DebugRect> {
        Vec::new()
    }

    fn debug_rects_with_clips(&self, root: &Element) -> Vec<(DebugRect, Option<[f32; 4]>)> {
        self.debug_rects(root)
            .into_iter()
            .map(|rect| (rect, None))
            .collect()
    }
}

struct Registration {
    runtime: Rc<dyn DragRuntime>,
}

crate::authoring::window::window_local! {
    static RUNTIME: RefCell<Option<Rc<Registration>>> = const { RefCell::new(None) };
}

/// Reinstalling the same instance keeps prepared work valid. Removal followed
/// by registration starts a new lifetime, even for the same runtime instance.
#[cfg(feature = "drag")]
pub fn register(runtime: Rc<dyn DragRuntime>) {
    let previous = RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(&current.runtime, &runtime))
        {
            return None;
        }
        slot.replace(Rc::new(Registration { runtime }))
    });
    drop(previous);
}

pub fn unregister() {
    let previous = RUNTIME.with(|slot| slot.borrow_mut().take());
    drop(previous);
}

fn current() -> Option<Rc<Registration>> {
    RUNTIME.with(|slot| slot.borrow().clone())
}

pub fn state() -> RuntimeState {
    current().map_or_else(RuntimeState::default, |registration| {
        registration.runtime.state()
    })
}

pub fn is_dragging() -> bool {
    state().dragging
}

pub fn is_active() -> bool {
    state().active
}

pub fn is_coasting() -> bool {
    state().coasting
}

#[must_use]
pub struct PreparedUpdate {
    registration: Option<Rc<Registration>>,
    window: crate::app::window_scope::WindowScope,
    update: Option<Update>,
}

impl PreparedUpdate {
    pub fn dispatch(self) -> bool {
        let _scope = crate::app::window_scope::enter(self.window);
        let (Some(registration), Some(update)) = (self.registration, self.update) else {
            return false;
        };
        let valid = current().is_some_and(|current| Rc::ptr_eq(&current, &registration));
        valid && update()
    }
}

pub fn prepare_move(
    root: Option<&Element>,
    pointer: [f32; 2],
    viewport: [f32; 2],
    mode: MoveMode,
) -> PreparedUpdate {
    let registration = current();
    let update = registration.as_ref().map(|registration| {
        registration
            .runtime
            .prepare_move(root, pointer, viewport, mode)
    });
    PreparedUpdate {
        window: crate::app::window_scope::active(),
        registration,
        update,
    }
}

pub fn prepare_end(root: &Element) -> PreparedUpdate {
    let registration = current();
    let update = registration
        .as_ref()
        .map(|registration| registration.runtime.prepare_end(root));
    PreparedUpdate {
        window: crate::app::window_scope::active(),
        registration,
        update,
    }
}

pub fn refresh_geometry(root: &Element) {
    if let Some(registration) = current() {
        registration.runtime.refresh_geometry(root);
    }
}

pub fn prepare_autoscroll(root: &Element, viewport: [f32; 2]) -> PreparedUpdate {
    let registration = current();
    let update = registration
        .as_ref()
        .map(|registration| registration.runtime.prepare_autoscroll(root, viewport));
    PreparedUpdate {
        window: crate::app::window_scope::active(),
        registration,
        update,
    }
}

pub fn prepare_inertia(root: &Element, viewport: [f32; 2]) -> PreparedUpdate {
    let registration = current();
    let update = registration
        .as_ref()
        .map(|registration| registration.runtime.prepare_inertia(root, viewport));
    PreparedUpdate {
        window: crate::app::window_scope::active(),
        registration,
        update,
    }
}

/// Synchronous convenience entry. Release host tree borrows before dispatching
/// prepared updates when callbacks can remount the tree.
pub fn pump_autoscroll(root: &Element, viewport: [f32; 2]) -> bool {
    prepare_autoscroll(root, viewport).dispatch()
}

/// Synchronous convenience entry; hosts should use [`prepare_inertia`].
pub fn pump_inertia(root: &Element, viewport: [f32; 2]) -> bool {
    prepare_inertia(root, viewport).dispatch()
}

/// Cancel the current pointer session and retire all of its prepared updates.
pub fn cancel() -> bool {
    let previous = RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();
        let previous = slot.take()?;
        *slot = Some(Rc::new(Registration {
            runtime: previous.runtime.clone(),
        }));
        Some(previous)
    });
    previous.is_some_and(|registration| registration.runtime.key(DragKey::Cancel))
}

pub fn key(key: DragKey) -> bool {
    if key == DragKey::Cancel {
        return cancel();
    }
    current().is_some_and(|registration| registration.runtime.key(key))
}

pub fn sync_tree(root: &mut Element) {
    if let Some(registration) = current() {
        registration.runtime.sync_tree(root);
    }
}

pub fn after_layout(root: &mut Element) {
    if let Some(registration) = current() {
        registration.runtime.after_layout(root);
    }
}

pub fn keeps_collect_live(element: &Element) -> bool {
    current().is_some_and(|registration| registration.runtime.keeps_collect_live(element))
}

pub fn collect_geometry(element: &mut Element, size: [f32; 2], scale: [f32; 2]) -> CollectGeometry {
    current().map_or(CollectGeometry { size, origin: None }, |registration| {
        registration.runtime.collect_geometry(element, size, scale)
    })
}

pub fn ancestor_clip(element: &Element) -> Option<[f32; 4]> {
    current().and_then(|registration| registration.runtime.ancestor_clip(element))
}

pub fn debug_rects(root: &Element) -> Vec<DebugRect> {
    current().map_or_else(Vec::new, |registration| {
        registration.runtime.debug_rects(root)
    })
}

pub fn debug_rects_with_clips(root: &Element) -> Vec<(DebugRect, Option<[f32; 4]>)> {
    current().map_or_else(Vec::new, |registration| {
        registration.runtime.debug_rects_with_clips(root)
    })
}

#[cfg(not(feature = "drag"))]
pub fn register(_runtime: Rc<dyn DragRuntime>) {}
