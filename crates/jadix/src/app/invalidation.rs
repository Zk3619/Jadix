//! Window-scoped invalidation. Declarations and expensive resource caches are
//! independent axes; requests are coalesced until the next frame prepass.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::window_scope::{self, WindowScope};
use crate::core::reactivity::{effect_is_alive, schedule_effect, EffectId};
use crate::core::tree::LayoutStatus;

#[derive(Default)]
struct Requests {
    present: bool,
    repaint: bool,
    rebuild: bool,
    caches: bool,
    nodes: HashMap<String, bool>, // true = subtree layout, false = paint
}

thread_local! {
    static REQUESTS: RefCell<HashMap<WindowScope, Requests>> = RefCell::new(HashMap::new());
    static ROOTS: RefCell<HashMap<WindowScope, EffectId>> = RefCell::new(HashMap::new());
    static FRAME: Cell<u64> = const { Cell::new(0) };
    static PREPASS: Cell<bool> = const { Cell::new(false) };
    static STATS: Cell<InvalidationStats> = const { Cell::new(InvalidationStats {
        frames: 0, rebuilds: 0, cache_busts: 0, paint_nodes: 0, layout_subtrees: 0,
    }) };
}

/// Cumulative work performed on this UI thread, across windows. L3 and L4 are
/// counted separately so accidental resource invalidation is observable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvalidationStats {
    pub frames: u64,
    pub rebuilds: u64,
    pub cache_busts: u64,
    pub paint_nodes: u64,
    pub layout_subtrees: u64,
}

pub fn invalidation_stats() -> InvalidationStats {
    STATS.with(Cell::get)
}

fn request(f: impl FnOnce(&mut Requests)) {
    REQUESTS.with(|q| f(q.borrow_mut().entry(window_scope::active()).or_default()));
    // Window handles don't borrow AppState: callbacks can request a frame while
    // input dispatch, layout or a user builder holds the tree.
    super::runtime::with_active_winit_window(|window| window.request_redraw());
}

/// L0: wake and present one frame, preserving layout, declarations and caches.
/// Call again from an immediate region to keep its animation running.
pub fn request_frame() {
    request(|r| r.present = true);
}

pub(crate) fn repaint_all() {
    let scopes = ROOTS.with(|roots| roots.borrow().keys().copied().collect::<Vec<_>>());
    REQUESTS.with(|queue| {
        let mut queue = queue.borrow_mut();
        for scope in scopes
            .into_iter()
            .chain(std::iter::once(window_scope::active()))
        {
            let request = queue.entry(scope).or_default();
            request.present = true;
            request.repaint = true;
        }
    });
    super::runtime::request_redraw_all();
}

/// L3: rerun/diff the active window's declarations and reactive islands/bindings.
/// Identity, layout caches, shaped text and glyph atlases stay warm.
pub fn rebuild_all() {
    request(|r| r.rebuild = true);
}

/// L4: explicitly invalidate text measurement, shaping, glyph atlas and text
/// texture resources in all windows sharing the font system. Also requests L3.
/// Use for font database/DPI/scale changes; ordinary theme changes use L3.
pub fn bust_caches() {
    let scopes = ROOTS.with(|roots| roots.borrow().keys().copied().collect::<Vec<_>>());
    REQUESTS.with(|q| {
        let mut q = q.borrow_mut();
        for scope in scopes
            .into_iter()
            .chain(std::iter::once(window_scope::active()))
        {
            let r = q.entry(scope).or_default();
            r.caches = true;
            r.rebuild = true;
        }
    });
    super::runtime::request_redraw_all();
}

/// Stable template-id target. Create/use it in its owning window's UI scope.
#[derive(Clone, Debug)]
pub struct UiNode {
    id: String,
    scope: WindowScope,
}

pub fn node(template_id: impl Into<String>) -> UiNode {
    UiNode {
        id: template_id.into(),
        scope: window_scope::active(),
    }
}

impl UiNode {
    /// L1: recollect paint without running declarations or Taffy layout.
    pub fn mark_paint(&self) {
        self.mark(false);
    }

    /// L2: invalidate style/layout for this subtree, retaining other subtrees.
    pub fn mark_layout(&self) {
        self.mark(true);
    }

    fn mark(&self, layout: bool) {
        let _scope = window_scope::enter(self.scope);
        request(|r| {
            *r.nodes.entry(self.id.clone()).or_default() |= layout;
        });
    }
}

pub(super) fn register_root(effect: EffectId) {
    ROOTS.with(|roots| roots.borrow_mut().insert(window_scope::active(), effect));
}

pub(super) fn forget(scope: WindowScope) {
    ROOTS.with(|roots| roots.borrow_mut().remove(&scope));
    REQUESTS.with(|q| q.borrow_mut().remove(&scope));
}

pub(super) fn reset() {
    ROOTS.with(|roots| roots.borrow_mut().clear());
    REQUESTS.with(|q| q.borrow_mut().clear());
}

pub(super) fn pending(scope: WindowScope) -> bool {
    REQUESTS.with(|q| q.borrow().contains_key(&scope))
}

pub(super) fn frame_serial() -> u64 {
    FRAME.with(Cell::get)
}

pub(super) fn in_prepass() -> bool {
    PREPASS.with(Cell::get)
}

struct PrepassGuard(bool);
impl Drop for PrepassGuard {
    fn drop(&mut self) {
        PREPASS.with(|p| p.set(self.0));
    }
}

/// Must run before borrowing either window tree. Requests made by declarations
/// during this prepass remain queued for the *next* frame.
pub(super) fn begin_frame() {
    let _guard = PrepassGuard(PREPASS.with(|p| p.replace(true)));
    FRAME.with(|frame| frame.set(frame.get().wrapping_add(1)));
    let requests = REQUESTS
        .with(|q| q.borrow_mut().remove(&window_scope::active()))
        .unwrap_or_default();
    if requests.caches {
        // Builders may measure text too. Invalidate before declarations, rather
        // than letting L3 bake stale font metrics into explicit dimensions.
        crate::text::invalidate_prepare_cache();
        crate::utils::text_texture::invalidate_text_texture_cache();
        super::runtime::apply_frame_invalidation(false, true, false, &HashMap::new());
    }
    if requests.rebuild {
        let root = ROOTS.with(|roots| roots.borrow().get(&window_scope::active()).copied());
        if let Some(root) = root.filter(|id| effect_is_alive(*id)) {
            schedule_effect(root);
        }
        super::reactive::rebuild_registered_in_scope();
    }
    super::reactive::tick_immediate_regions();
    super::runtime::apply_frame_invalidation(
        requests.present,
        false,
        requests.repaint,
        &requests.nodes,
    );
    STATS.with(|stats| {
        let mut s = stats.get();
        s.frames += 1;
        s.rebuilds += u64::from(requests.rebuild);
        s.cache_busts += u64::from(requests.caches);
        s.paint_nodes += requests.nodes.values().filter(|layout| !**layout).count() as u64;
        s.layout_subtrees += requests.nodes.values().filter(|layout| **layout).count() as u64;
        stats.set(s);
    });
}

pub(super) fn patch_node(el: &mut crate::core::tree::Element, layout: bool) -> LayoutStatus {
    // Rehash the requested subtree as well as its ancestor path. A parent paint
    // mark may follow an in-place change to a descendant's visual style.
    crate::core::tree::walk_elements_mut(el, |n| {
        n.collect_paint_hash = 0;
        if !layout {
            super::damage::push_frame_paint_element(n, n.paint_scroll_acc);
        }
    });
    if layout {
        LayoutStatus::LayoutChanged
    } else {
        LayoutStatus::StyleChanged
    }
}

#[cfg(test)]
#[path = "tests/invalidation.rs"]
mod tests;
