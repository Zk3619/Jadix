//! Product-owned pointer handlers and host dispatch phases.
//!
//! Register at drag start and unregister at drag end. Tree sampling runs while
//! the host borrows its tree; move, finish and deferred callbacks run after that
//! borrow is released. Registrations belong to the active window, like pointer sessions.

use crate::core::tree::Element;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollPolicy {
    #[default]
    Allow,
    BlockPan,
    Freeze,
}

/// Callbacks for one active drag. The product retains ownership of its state.
pub struct DragHandlers {
    on_move: fn(f32, f32) -> bool,
    on_finish: fn(),
    sample: Option<fn(&Element, f32, f32)>,
    paint_sample: Option<fn(&mut Element, f32, f32) -> bool>,
    drain: Option<fn()>,
    scroll_policy: ScrollPolicy,
    blocks_native_input: bool,
}

impl DragHandlers {
    pub fn new(on_move: fn(f32, f32) -> bool, on_finish: fn()) -> Self {
        Self {
            on_move,
            on_finish,
            sample: None,
            paint_sample: None,
            drain: None,
            scroll_policy: ScrollPolicy::Allow,
            blocks_native_input: false,
        }
    }

    pub fn scroll_policy(mut self, policy: ScrollPolicy) -> Self {
        self.scroll_policy = policy;
        self
    }

    /// Native child surfaces must yield input while this drag owns the pointer.
    pub fn block_native_input(mut self) -> Self {
        self.blocks_native_input = true;
        self
    }

    /// Sample current tree geometry without writing Signals. Queue work in the
    /// product and apply it in `drain`, after the host releases its state borrow.
    /// A newer `on_move` must supersede any older queued frame sample.
    pub fn frame_sampling(mut self, sample: fn(&Element, f32, f32), drain: fn()) -> Self {
        self.sample = Some(sample);
        self.drain = Some(drain);
        self
    }
    /// Update retained paint under the host borrow. Signal writes and remounts
    /// must be queued for `drain` or `on_move`, after that borrow is released.
    pub fn paint_sampling(
        mut self,
        sample: fn(&mut Element, f32, f32) -> bool,
        drain: fn(),
    ) -> Self {
        self.paint_sample = Some(sample);
        self.drain = Some(drain);
        self
    }
}

#[derive(Clone)]
struct Registration {
    key: &'static str,
    handlers: Rc<DragHandlers>,
}

crate::authoring::window::window_local! {
    static ROUTES: RefCell<Vec<Registration>> = const { RefCell::new(Vec::new()) };
}

/// Keys must distinguish product modules. Replacing a key invalidates moves
/// prepared for its previous registration, including reentrant replacements.
#[cfg(feature = "drag")]
pub fn register(key: &'static str, handlers: DragHandlers) {
    let registration = Registration {
        key,
        handlers: Rc::new(handlers),
    };
    ROUTES.with(|routes| {
        let mut routes = routes.borrow_mut();
        if let Some(previous) = routes.iter_mut().find(|route| route.key == key) {
            *previous = registration;
        } else {
            routes.push(registration);
        }
    });
}

pub fn unregister(key: &'static str) {
    ROUTES.with(|routes| routes.borrow_mut().retain(|route| route.key != key));
}

fn registrations() -> Vec<Registration> {
    ROUTES.with(|routes| routes.borrow().clone())
}

fn is_current(registration: &Registration) -> bool {
    ROUTES.with(|routes| {
        routes.borrow().iter().any(|route| {
            route.key == registration.key && Rc::ptr_eq(&route.handlers, &registration.handlers)
        })
    })
}

/// A move sampled while the tree is borrowed, for dispatch after that borrow.
#[must_use]
pub struct PreparedMove {
    registrations: Vec<Registration>,
    window: crate::app::window_scope::WindowScope,
    x: f32,
    y: f32,
}

impl PreparedMove {
    pub fn dispatch(self) -> bool {
        let _scope = crate::app::window_scope::enter(self.window);
        let mut handled = false;
        for route in self.registrations {
            if is_current(&route) {
                handled |= (route.handlers.on_move)(self.x, self.y);
            }
        }
        handled
    }
}

/// `None` skips handlers requiring tree samples, e.g. while an overlay owns the
/// drag. Handlers without tree sampling continue to receive pointer movement.
pub fn prepare_move(root: Option<&Element>, x: f32, y: f32) -> PreparedMove {
    let mut prepared = Vec::new();
    for route in registrations() {
        if !is_current(&route) {
            continue;
        }
        if route.handlers.paint_sample.is_some() && root.is_none() {
            continue;
        }
        if let Some(sample) = route.handlers.sample {
            let Some(root) = root else { continue };
            sample(root, x, y);
        }
        prepared.push(route);
    }
    PreparedMove {
        window: crate::app::window_scope::active(),
        registrations: prepared,
        x,
        y,
    }
}

/// Sample retained paint during a pointer move, before preparing reactive work.
/// The host skips this phase when another overlay owns the pointer.
pub fn sample_paint(root: &mut Element, x: f32, y: f32) -> bool {
    let mut changed = false;
    for route in registrations() {
        if is_current(&route) {
            if let Some(sample) = route.handlers.paint_sample {
                changed |= sample(root, x, y);
            }
        }
    }
    changed
}

/// Refresh geometry after layout without applying reactive writes under the
/// host borrow. Only handlers that explicitly request frame sampling run here.
pub fn queue_frame(root: &Element, x: f32, y: f32) {
    for route in registrations() {
        if is_current(&route) {
            if let Some(sample) = route.handlers.sample {
                sample(root, x, y);
            }
        }
    }
}

pub fn drain_frame() {
    for route in registrations() {
        if is_current(&route) {
            if let Some(drain) = route.handlers.drain {
                drain();
            }
        }
    }
}

/// Detach before invoking callbacks so finish can unregister or start a new drag.
/// Products retain their own settle, pending-work and pointer-capture cleanup.
pub fn finish_all() {
    let routes = ROUTES.with(|routes| std::mem::take(&mut *routes.borrow_mut()));
    for route in routes {
        (route.handlers.on_finish)();
    }
}

pub fn blocks_content_pan() -> bool {
    ROUTES.with(|routes| {
        routes
            .borrow()
            .iter()
            .any(|route| route.handlers.scroll_policy != ScrollPolicy::Allow)
    })
}

pub fn freezes_scroll() -> bool {
    ROUTES.with(|routes| {
        routes
            .borrow()
            .iter()
            .any(|route| route.handlers.scroll_policy == ScrollPolicy::Freeze)
    })
}

/// Whether a product-owned drag requires native child surfaces to yield input.
pub fn blocks_native_input() -> bool {
    ROUTES.with(|routes| {
        routes
            .borrow()
            .iter()
            .any(|route| route.handlers.blocks_native_input)
    })
}

#[cfg(not(feature = "drag"))]
pub fn register(_key: &'static str, _handlers: DragHandlers) {}
