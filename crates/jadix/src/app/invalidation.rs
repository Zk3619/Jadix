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
        super::runtime::apply_frame_invalidation(false, true, &HashMap::new());
    }
    if requests.rebuild {
        let root = ROOTS.with(|roots| roots.borrow().get(&window_scope::active()).copied());
        if let Some(root) = root.filter(|id| effect_is_alive(*id)) {
            schedule_effect(root);
        }
        super::reactive::rebuild_registered_in_scope();
    }
    super::reactive::tick_immediate_regions();
    super::runtime::apply_frame_invalidation(requests.present, false, &requests.nodes);
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
mod tests {
    use super::*;
    use crate::app::reactive::{immediate_region, reactive_view, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, create_root, OwnerHandle};
    use crate::core::tree::{Element, LayoutTree};
    use std::rc::Rc;

    fn harness(
        mut render: impl FnMut() -> Element + 'static,
    ) -> (OwnerHandle, Rc<RefCell<Element>>) {
        reset();
        reset_registered_anchors();
        let tree = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(tree.clone());
        let root = tree.clone();
        let (owner, _) = create_root(move || {
            create_effect(move || {
                register_root(crate::core::reactivity::current_effect_id().unwrap());
                let mut next = render();
                crate::app::test_support::reconcile_elements(&mut next, &mut root.borrow_mut());
                *root.borrow_mut() = next;
            });
        });
        (owner, tree)
    }

    #[test]
    fn hybrid_immediate_ticks_only_its_region_and_can_stop_requesting_frames() {
        let retained = Rc::new(Cell::new(0));
        let ticks = Rc::new(Cell::new(0));
        let active = Rc::new(Cell::new(true));
        let (owner, tree) = harness({
            let retained = retained.clone();
            let ticks = ticks.clone();
            let active = active.clone();
            move || {
                retained.set(retained.get() + 1);
                let ticks = ticks.clone();
                let active = active.clone();
                Element::new().child(immediate_region("hud", move || {
                    ticks.set(ticks.get() + 1);
                    if active.get() {
                        request_frame();
                    }
                    Element::new()
                        .children((0..ticks.get()).map(|i| Element::new().text(i.to_string())))
                }))
            }
        });
        assert_eq!(ticks.get(), 1);
        for expected in 2..5 {
            assert!(pending(WindowScope::Primary));
            begin_frame();
            assert_eq!(ticks.get(), expected);
            assert_eq!(tree.borrow().children[0].children.len(), expected);
        }
        active.set(false);
        begin_frame();
        assert!(
            !pending(WindowScope::Primary),
            "no automatic permanent pump"
        );
        assert_eq!(
            retained.get(),
            1,
            "retained parent must not rebuild with HUD"
        );
        let before = ticks.get();
        request_frame();
        begin_frame();
        assert_eq!(ticks.get(), before + 1);
        assert!(!pending(WindowScope::Primary));
        drop(owner);
        assert_eq!(crate::app::reactive::registered_anchor_count(), 0);
        clear_test_reconcile_root();
        reset();
    }

    #[test]
    fn hybrid_nested_regions_run_once_per_frame_and_rebuild_keeps_layout_hot() {
        let roots = Rc::new(Cell::new(0));
        let parent = Rc::new(Cell::new(0));
        let child = Rc::new(Cell::new(0));
        let (owner, tree) = harness({
            let roots = roots.clone();
            let parent = parent.clone();
            let child = child.clone();
            move || {
                roots.set(roots.get() + 1);
                let parent = parent.clone();
                let child = child.clone();
                Element::new()
                    .w(200.0)
                    .h(100.0)
                    .child(immediate_region("outer", move || {
                        parent.set(parent.get() + 1);
                        let child = child.clone();
                        Element::new().w(100.0).h(40.0).child(immediate_region(
                            "inner",
                            move || {
                                child.set(child.get() + 1);
                                Element::new().w(40.0).h(20.0).text("warm")
                            },
                        ))
                    }))
            }
        });
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut tree.borrow_mut(), 200.0, 100.0, [-1.0; 2]);
        let ids = {
            let t = tree.borrow();
            (
                t.layout_node,
                t.children[0].layout_node,
                t.children[0].children[0].layout_node,
            )
        };
        let stats = invalidation_stats();
        for _ in 0..4 {
            begin_frame();
        }
        assert_eq!(parent.get(), 5);
        assert_eq!(
            child.get(),
            5,
            "nested old effect must not execute before its parent replaces it"
        );
        rebuild_all();
        rebuild_all();
        begin_frame();
        assert_eq!(roots.get(), 2, "L3 coalesces");
        assert_eq!(parent.get(), 6);
        assert_eq!(child.get(), 6);
        let t = tree.borrow();
        assert_eq!(
            ids,
            (
                t.layout_node,
                t.children[0].layout_node,
                t.children[0].children[0].layout_node
            )
        );
        assert!(
            !layout.is_node_dirty(ids.0.unwrap()),
            "L3 cannot invalidate unchanged Taffy inputs"
        );
        assert_eq!(invalidation_stats().cache_busts, stats.cache_busts);
        assert_eq!(invalidation_stats().rebuilds, stats.rebuilds + 1);
        drop(t);
        drop(owner);
        clear_test_reconcile_root();
        reset();
    }

    #[test]
    fn hybrid_ladder_coalesces_without_promoting_l3_to_l4_and_is_window_scoped() {
        reset();
        let _scope = window_scope::enter_primary();
        let (owner, ()) = create_root(|| {
            create_effect(|| register_root(crate::core::reactivity::current_effect_id().unwrap()));
        });
        request_frame();
        node("leaf").mark_paint();
        node("leaf").mark_layout();
        node("leaf").mark_paint();
        rebuild_all();
        REQUESTS.with(|q| {
            let q = q.borrow();
            let r = &q[&WindowScope::Primary];
            assert!(r.present && r.rebuild && !r.caches);
            assert_eq!(r.nodes.len(), 1);
            assert!(r.nodes["leaf"]);
        });
        let _secondary = window_scope::enter(WindowScope::Secondary(991));
        assert!(!pending(window_scope::active()));
        bust_caches();
        assert!(pending(WindowScope::Secondary(991)));
        REQUESTS.with(|q| {
            let q = q.borrow();
            for scope in [WindowScope::Primary, WindowScope::Secondary(991)] {
                assert!(
                    q[&scope].caches && q[&scope].rebuild,
                    "L4 fans out to font-sharing windows"
                );
            }
        });
        forget(WindowScope::Secondary(991));
        assert!(!pending(WindowScope::Secondary(991)));
        assert!(pending(WindowScope::Primary));
        drop(owner);
        reset();
    }

    #[test]
    fn hybrid_l3_refreshes_retained_islands_without_frame_subscriptions() {
        let text = Rc::new(Cell::new("first"));
        let (owner, tree) = harness({
            let text = text.clone();
            move || {
                let text = text.clone();
                Element::new().child(reactive_view("locale", move || {
                    Element::new().text(text.get())
                }))
            }
        });
        text.set("second");
        assert_eq!(tree.borrow().children[0].text.as_deref(), Some("first"));
        rebuild_all();
        begin_frame();
        assert_eq!(tree.borrow().children[0].text.as_deref(), Some("second"));
        drop(owner);
        clear_test_reconcile_root();
        reset();
    }
}
