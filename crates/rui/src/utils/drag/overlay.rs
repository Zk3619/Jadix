//! Drag overlay (solid-dnd `DragOverlay`): a Pinned portal whose author inset
//! follows the pointer. Uses [`portal_sync`] + [`OverlayMode::Pinned`] so the
//! ghost is not on the Esc stack (overlay_shells.md §9.3).
//!
//! Geometry is **Unanchored** (`ATTR_FLOATING` only). Point placement runs
//! [`compute_cursor_menu_placement`](crate::utils::popup::compute_cursor_menu_placement)
//! and clamps Y to the viewport floor; seeding `apply_stored_floating_place`
//! with `match_trigger=0` + `strategy=fixed` writes overflow-hidden at (0,0)
//! until measure and the ghost vanishes.

use std::rc::Rc;

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::isolate_reads;
use crate::core::tree::{Element, IntoOpacity};
use crate::layout::{dim_kind, DimKind};
use crate::utils::popup::{mount_anchored_overlay, ATTR_FLOATING};

use super::builder::{clone_source_preview, require_drag};
use super::session::{mark_overlay_registered, with_scope};
use super::{
    ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE, ATTR_DRAG_GROUP,
    ATTR_DRAG_HANDLE, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX, ATTR_DRAG_ITEM_SHELL, ATTR_DRAG_LIST,
    ATTR_DRAG_OVER, ATTR_DRAG_OVERLAY, ATTR_DRAG_SCOPE, ATTR_DRAG_TYPE,
};

/// Pin the ghost at viewport `origin` (item top-left, already grab-offset).
///
/// `fixed` so left/top resolve against the window even before hoist — Gallery
/// hosts the overlay placeholder inside a ScrollArea. No POINT / PLACEMENT, so
/// `sync_floating_placement` does not rewrite the inset.
///
/// Placement only (`fixed` left/top). Size is decided by the caller: follow-source
/// clones pin `drag_extent` so `w=100%` does not resolve against the portal CB;
/// a custom overlay child keeps its author size (stretching a chip to the Handle
/// row is the huge empty slab).
fn place_drag_ghost(mut panel: Element, ghost_id: &str, origin: [f32; 2]) -> Element {
    panel.set_attr(ATTR_FLOATING, ghost_id);
    panel.set_attr(ATTR_DRAG_OVERLAY, "1");
    panel.set_attr(crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    panel
        .pointer_events_none()
        .fixed()
        .left(origin[0])
        .top(origin[1])
}

/// Follow-source clones of `w=100%` / `h=100%` must pin the used `drag_extent`
/// (portal CB would resolve the percentage into a slab). Auto-sized chips must
/// keep author size — `start_bounds` is the stretched island, not the chip.
fn pin_percent_size_to_extent(mut panel: Element, extent: [f32; 2]) -> Element {
    if matches!(dim_kind(panel.style.size.width), DimKind::Percent(_)) && extent[0] > 0.0 {
        panel = panel.w(extent[0]);
    }
    if matches!(dim_kind(panel.style.size.height), DimKind::Percent(_)) && extent[1] > 0.0 {
        panel = panel.h(extent[1]);
    }
    panel
}

/// Builder: register an overlay slot inside the current [`super::drag_scope`].
///
/// Default (no [`Self::child`]): the ghost is the active draggable's author body.
/// [`Self::style`] is applied on top of that clone (or on a custom child).
pub struct DragOverlayBuilder {
    body: Option<Box<dyn Fn() -> Element>>,
    decorate: Option<Rc<dyn Fn(Element) -> Element>>,
    /// Source-island opacity while this overlay is open. Applied to the **body**,
    /// not the ghost. `None` → [`super::OVERLAY_SOURCE_OPACITY`].
    source_opacity: Option<f32>,
}

impl DragOverlayBuilder {
    pub fn child(mut self, body: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(body));
        self
    }

    /// Overlay the clone / custom child. Later calls compose after earlier ones.
    pub fn style(mut self, f: impl Fn(Element) -> Element + 'static) -> Self {
        let prev = self.decorate.take();
        self.decorate = Some(Rc::new(move |el| {
            let el = match &prev {
                Some(p) => p(el),
                None => el,
            };
            f(el)
        }));
        self
    }

    /// Opacity of the in-list **source** while this overlay is open.
    /// `0` leaves the insert hole empty; default is [`super::OVERLAY_SOURCE_OPACITY`].
    /// Does not paint the ghost — RSX `opacity` / [`Self::style`] is resting/ghost paint.
    pub fn source_opacity(mut self, o: impl IntoOpacity) -> Self {
        self.source_opacity = Some(o.into_opacity());
        self
    }
}

fn strip_source_session_attrs(el: &mut Element) {
    for key in [
        ATTR_DRAG_DRAGGABLE,
        ATTR_DRAG_DROPPABLE,
        ATTR_DRAG_HANDLE,
        ATTR_DRAG_IGNORE,
        ATTR_DRAG_SCOPE,
        ATTR_DRAG_TYPE,
        ATTR_DRAG_LIST,
        ATTR_DRAG_INDEX,
        ATTR_DRAG_GROUP,
        ATTR_DRAG_DISABLED,
        ATTR_DRAG_OVER,
        ATTR_DRAG_ITEM_SHELL,
    ] {
        el.attrs.remove(key);
    }
    el.on_mousedown = None;
    el.on_mouseup = None;
    el.on_mousemove = None;
    el.on_click = None;
    for child in &mut el.children {
        strip_source_session_attrs(child);
    }
}

fn suffix_ghost_ids(el: &mut Element, suffix: &str) {
    if let Some(tid) = &mut el.template_id {
        tid.push_str("::");
        tid.push_str(suffix);
    }
    if let Some(fid) = &mut el.focus_id {
        fid.push_str("::");
        fid.push_str(suffix);
    }
    for child in &mut el.children {
        suffix_ghost_ids(child, suffix);
    }
}

impl From<DragOverlayBuilder> for Element {
    fn from(builder: DragOverlayBuilder) -> Self {
        let ctx = require_drag("DragOverlay");
        mark_overlay_registered(&ctx.id);
        with_scope(&ctx.id, |rt| {
            rt.source_opacity = builder
                .source_opacity
                .unwrap_or(super::OVERLAY_SOURCE_OPACITY);
        });
        let custom = builder.body;
        let decorate = builder.decorate;
        let view_id = format!("{}__drag_overlay", ctx.id);
        let open = ctx.overlay_open;
        let pos = ctx.overlay_pos;
        let active = ctx.active_draggable;
        let extent = ctx.drag_extent;
        let scope_id = ctx.id.clone();
        let ghost_id = format!("{}__ghost", ctx.id);

        reactive_anchor(view_id.clone(), move || {
            // Subscribe before portal_sync: Pinned + closed returns None via peek,
            // so a skipped `.get()` would never re-run when the drag activates
            // (date_picker panel island, same contract).
            let is_open = open.get();
            let p = pos.get();
            let source_id = active.get();
            if !is_open {
                let _ = portal_sync(
                    ghost_id.as_str(),
                    open,
                    OverlayKind::Popover,
                    OverlayMode::Pinned,
                );
                return Element::new().w(0.0).h(0.0);
            }
            let Some(oz) = portal_sync(
                ghost_id.as_str(),
                open,
                OverlayKind::Popover,
                OverlayMode::Pinned,
            ) else {
                return Element::new().w(0.0).h(0.0);
            };
            let following_source = custom.is_none();
            let mut panel = if let Some(body) = &custom {
                isolate_reads(|| body())
            } else if let Some(id) = source_id.as_deref() {
                isolate_reads(|| clone_source_preview(&scope_id, id).unwrap_or_else(Element::new))
            } else {
                Element::new().w(0.0).h(0.0)
            };
            strip_source_session_attrs(&mut panel);
            suffix_ghost_ids(&mut panel, ghost_id.as_str());
            if following_source {
                panel = pin_percent_size_to_extent(panel, extent.get());
            }
            if let Some(dec) = &decorate {
                panel = dec(panel);
            }
            panel = place_drag_ghost(panel, ghost_id.as_str(), p);
            mount_anchored_overlay(panel, oz.panel)
        });
        reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
    }
}

pub fn drag_overlay() -> DragOverlayBuilder {
    DragOverlayBuilder {
        body: None,
        decorate: None,
        source_opacity: None,
    }
}

/// RSX `<DragOverlay>` with children — same implementation as the Builder.
pub fn drag_overlay_from_rsx(body: impl Fn() -> Element + 'static) -> Element {
    drag_overlay().child(body).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_render_effect, create_root, Signal};
    use crate::utils::popup::{ATTR_FLOATING_PLACEMENT, ATTR_FLOATING_POINT};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn overlay_island_reruns_when_open_flips() {
        let (_o, _) = create_root(|| {
            let open = Signal::new(false);
            let runs = Rc::new(Cell::new(0));
            let runs2 = runs.clone();
            create_render_effect(move || {
                runs2.set(runs2.get() + 1);
                let is_open = open.get();
                if !is_open {
                    let _ = portal_sync(
                        "drag-ov-test",
                        open,
                        OverlayKind::Popover,
                        OverlayMode::Pinned,
                    );
                    return;
                }
                let _ = portal_sync(
                    "drag-ov-test",
                    open,
                    OverlayKind::Popover,
                    OverlayMode::Pinned,
                );
            });
            assert_eq!(runs.get(), 1);
            open.set(true);
            assert!(runs.get() >= 2);
        });
    }

    #[test]
    fn overlay_default_source_opacity_is_dimmed() {
        use crate::utils::drag::{
            drag_overlay, drag_scope, overlay_hides_source, overlay_source_opacity,
        };
        let (_o, _) = create_root(|| {
            let _el: Element = drag_scope("item-op").child(|| Element::from(drag_overlay()));
            assert!(
                (overlay_source_opacity("item-op") - 0.4).abs() < 0.001,
                "default overlay session opacity is 0.4"
            );
            assert!(!overlay_hides_source("item-op"));
        });
    }

    #[test]
    fn overlay_opacity_zero_marks_scope() {
        use crate::utils::drag::{drag_overlay, drag_scope};
        let (_o, _) = create_root(|| {
            let _el: Element =
                drag_scope("hide-src").child(|| Element::from(drag_overlay().source_opacity(0.0)));
            assert!(
                crate::utils::drag::overlay_hides_source("hide-src"),
                "DragOverlay.source_opacity(0) must stick on the scope for assemble_draggable"
            );
        });
    }

    #[test]
    fn ghost_pins_viewport_origin_without_point_attr() {
        let el = place_drag_ghost(Element::new().text("card"), "g1", [120.0, 80.0]);
        assert_eq!(el.attrs.get(ATTR_FLOATING).map(String::as_str), Some("g1"));
        assert_eq!(
            el.attrs.get(ATTR_DRAG_OVERLAY).map(String::as_str),
            Some("1")
        );
        assert!(
            el.attrs.get(ATTR_FLOATING_POINT).is_none(),
            "POINT would let cursor-menu clamp Y to the viewport floor"
        );
        assert!(
            el.attrs.get(ATTR_FLOATING_PLACEMENT).is_none(),
            "PLACEMENT would let sync_floating_placement rewrite inset"
        );
        assert!(
            el.style.position_fixed,
            "fixed: viewport containing block before hoist (ScrollArea host)"
        );
        assert_eq!(
            crate::layout::lpa_length_px(el.style.inset.left),
            Some(120.0)
        );
        assert_eq!(crate::layout::lpa_length_px(el.style.inset.top), Some(80.0));
        assert!(
            crate::layout::dim_length_px(el.style.size.width).is_none(),
            "source drag_extent must not become the overlay width"
        );
        assert!(
            crate::layout::dim_length_px(el.style.size.height).is_none(),
            "source drag_extent must not become the overlay height"
        );
    }

    fn find_ghost(el: &Element) -> Option<&Element> {
        if el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
            return Some(el);
        }
        el.children.iter().find_map(find_ghost)
    }

    fn collect_tids(el: &Element, out: &mut Vec<String>) {
        if let Some(id) = &el.template_id {
            out.push(id.clone());
        }
        for c in &el.children {
            collect_tids(c, out);
        }
    }

    fn collect_texts(el: &Element, out: &mut Vec<String>) {
        if let Some(t) = &el.text {
            if !t.is_empty() {
                out.push(t.clone());
            }
        }
        for c in &el.children {
            collect_texts(c, out);
        }
    }

    fn find_scope_id(el: &Element) -> Option<String> {
        el.attrs
            .get(crate::utils::drag::ATTR_DRAG_SCOPE)
            .cloned()
            .or_else(|| el.children.iter().find_map(find_scope_id))
    }

    fn ghost_is_under_portal_layer(el: &Element) -> bool {
        fn walk(el: &Element, under_layer: bool) -> bool {
            let here = under_layer
                || el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            if here && el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
                return true;
            }
            el.children.iter().any(|c| walk(c, here))
        }
        walk(el, false)
    }

    /// Gallery path: section `isolate_reads` → DragProvider + DragOverlay island →
    /// `begin`/`tick` → reconcile + hoist. Asserts the user-visible symptom (ghost on
    /// the portal layer at `overlay_pos`), not a helper in isolation.
    #[test]
    fn gallery_path_ghost_is_on_portal_layer_after_activate() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, isolate_reads};
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, overlay_is_registered, tick, BeginDrag,
        };
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (_o, _) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let el = isolate_reads(|| {
                    drag_scope("ov-loop").threshold(0.0).child(|| {
                        let chip = draggable("chip").child(|| {
                            Element::new()
                                .text("card")
                                .w(80.0)
                                .h(32.0)
                                .bg([0.9, 0.9, 0.9, 1.0])
                        });
                        let overlay = drag_overlay()
                            .child(|| Element::new().text("ghost").bg([1.0, 0.0, 0.0, 1.0]));
                        Element::from(vstack().children([chip, overlay.into()]))
                    })
                });
                *live_e.borrow_mut() = el;
            });
        });
        drain_deferred_view_remounts();

        let sid = find_scope_id(&live.borrow()).expect("drag scope attr");
        assert!(
            overlay_is_registered(&sid),
            "DragOverlay From must set overlay_registered before activate; sid={sid}"
        );

        begin(BeginDrag {
            scope_id: sid.clone(),
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        // origin = start + delta - grab = [40+10-10, 16+4-8] = [40, 12]
        assert!(
            tick(50.0, 20.0, &[], [800.0, 600.0]),
            "threshold 0 must activate"
        );
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let mut lt = crate::layout::LayoutTree::new();
            let _ = lt.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
            let mut tids = Vec::new();
            collect_tids(&tree, &mut tids);
            let ghost = find_ghost(&tree);
            assert!(
                ghost.is_some(),
                "ghost missing from tree after activate (overlay island did not mount). tids={tids:?}"
            );
            assert!(
                ghost_is_under_portal_layer(&tree),
                "ghost is in the tree but was not hoisted out of the 0×0 mount (ScrollArea would clip it). tids={tids:?}"
            );
            let g = ghost.unwrap();
            assert!(
                g.layout_bounds[2] > 10.0 && g.layout_bounds[3] > 10.0,
                "ghost hoisted but 0-sized: {:?}",
                g.layout_bounds
            );
            assert!(
                (g.layout_bounds[0] - 40.0).abs() < 2.0,
                "ghost x not following overlay_pos; bounds={:?}",
                g.layout_bounds
            );
            assert!(
                (g.layout_bounds[1] - 12.0).abs() < 2.0,
                "ghost y not following overlay_pos; bounds={:?}",
                g.layout_bounds
            );
        }

        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn mount_scope_tree(
        build: impl Fn() -> Element + 'static,
    ) -> (
        crate::core::reactivity::OwnerHandle,
        std::rc::Rc<std::cell::RefCell<Element>>,
    ) {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, isolate_reads};
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, _) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                *live_e.borrow_mut() = isolate_reads(|| build());
            });
        });
        drain_deferred_view_remounts();
        (owner, live)
    }

    fn find_droppable<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DROPPABLE)
            .map(String::as_str)
            == Some(id)
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_droppable(c, id))
    }

    fn find_draggable<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
            .map(String::as_str)
            == Some(id)
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_draggable(c, id))
    }

    fn find_draggable_in_scope<'a>(el: &'a Element, scope: &str, id: &str) -> Option<&'a Element> {
        let id_ok = el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
            .map(String::as_str)
            == Some(id);
        let scope_ok = el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_SCOPE)
            .map(String::as_str)
            == Some(scope);
        if id_ok && scope_ok {
            return Some(el);
        }
        el.children
            .iter()
            .find_map(|c| find_draggable_in_scope(c, scope, id))
    }

    /// Gallery park: source `<Show>` unmounts `chip` and the droppable mounts
    /// `chip:inbox`. Kanban uses the same location-scoped id so the parked
    /// island is new and actually paints inside the zone.
    #[test]
    fn park_location_scoped_id_paints_inside_droppable() {
        use crate::app::reactive::{reset_registered_anchors, show_reactive};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::hstack;
        use crate::utils::drag::{drag_scope, draggable, droppable};

        let parked = Signal::new(false);
        let (_o, live) = mount_scope_tree({
            let parked = parked;
            move || {
                drag_scope("ov-park-scoped").child(move || {
                    let src = show_reactive(
                        "ov-park-scoped-src",
                        move || !parked.get(),
                        || {
                            draggable("chip")
                                .item_type("chip")
                                .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0))
                        },
                    );
                    let zone = droppable("inbox").accept("chip").child({
                        let parked = parked;
                        move || {
                            if parked.get() {
                                draggable("chip:inbox")
                                    .item_type("chip")
                                    .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0))
                            } else {
                                Element::new().text("放到这里")
                            }
                        }
                    });
                    Element::from(hstack().children([src, zone]))
                })
            }
        });
        parked.set(true);
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "location-scoped parked id must paint inside the droppable, got {texts:?}"
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery park row: auto-width chip + Droppable in a wrap HStack.
    /// A `100%`×`100%` droppable shell wraps and collapses; the zone must keep
    /// the author min-size so the dashed container stays visible.
    #[test]
    fn droppable_zone_keeps_min_size_beside_auto_chip() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::{boxed, hstack};
        use crate::utils::drag::{drag_scope, draggable, droppable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-zone-size").child(|| {
                let src = draggable("chip")
                    .item_type("chip")
                    .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0));
                let zone = droppable("inbox").accept("chip").child(|| {
                    Element::from(
                        boxed()
                            .min_h(72.0)
                            .min_w(128.0)
                            .flex_grow(1.0)
                            .child(Element::new().text("放到这里")),
                    )
                });
                Element::from(
                    hstack()
                        .w("100%")
                        .items("stretch")
                        .wrap("wrap")
                        .gap(12.0)
                        .children([src, zone]),
                )
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox zone");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("放到这里")),
                "droppable body must mount without a hover rebuild, got {texts:?}"
            );
            assert!(
                zone.layout_bounds[2] >= 120.0,
                "droppable must not shrink to a sliver beside the chip; w={}",
                zone.layout_bounds[2]
            );
            assert!(
                zone.layout_bounds[3] >= 70.0,
                "droppable must keep author min-height; h={}",
                zone.layout_bounds[3]
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery: park into a Droppable, then drag that parked island and drop
    /// on the same zone. Overlay dim / empty placeholder must not stick.
    ///
    /// Do **not** hoist+layout repeatedly here — Windows debug stacks overflow
    /// (same class as the deleted scrolled-host ghost test).
    #[test]
    fn parked_item_restores_style_after_redrop_in_same_zone() {
        use crate::app::reactive::{reset_registered_anchors, show_reactive};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::hstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, end, tick, BeginDrag,
        };

        let parked = Signal::new(false);
        let (_o, live) = mount_scope_tree({
            let parked = parked;
            move || {
                drag_scope("ov-park-redrop").threshold(0.0).child(move || {
                    let src = show_reactive(
                        "ov-park-redrop-src",
                        move || !parked.get(),
                        || {
                            draggable("chip").item_type("chip").child(|| {
                                Element::new()
                                    .text("可拖卡片")
                                    .w(80.0)
                                    .h(32.0)
                                    .bg([0.2, 0.3, 0.8, 1.0])
                            })
                        },
                    );
                    let zone = droppable("inbox").accept("chip").child({
                        let parked = parked;
                        move || {
                            if parked.get() {
                                draggable("chip:inbox").item_type("chip").child(|| {
                                    Element::new()
                                        .text("可拖卡片")
                                        .w(80.0)
                                        .h(32.0)
                                        .bg([0.2, 0.3, 0.8, 1.0])
                                })
                            } else {
                                Element::new().text("放到这里")
                            }
                        }
                    });
                    let overlay: Element = drag_overlay().source_opacity(0.0).into();
                    Element::from(hstack().children([src, zone, overlay]))
                })
            }
        });
        parked.set(true);
        crate::app::reactive::drain_deferred_view_remounts();
        begin(BeginDrag {
            scope_id: "ov-park-redrop".into(),
            draggable_id: "chip:inbox".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let inbox = crate::utils::drag::DragHit {
            id: "inbox".into(),
            scope_id: "ov-park-redrop".into(),
            is_draggable: false,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, 0.0, 200.0, 80.0]),
            item_type: String::new(),
            accept: "chip".into(),
            list_id: Some("inbox".into()),
            index: None,
            group: String::new(),
            disabled: false,
        };
        let hits = [inbox];
        assert!(tick(50.0, 20.0, &hits, [800.0, 600.0]));
        crate::app::reactive::drain_deferred_view_remounts();
        assert!(end(&hits));
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let zone = find_droppable(&tree, "inbox").expect("inbox after redrop");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "parked chip must still paint inside the droppable after redrop, got {texts:?}"
            );
            let item = find_draggable(&tree, "chip:inbox").expect("parked chip after redrop");
            assert!(
                (item.style.opacity - 1.0).abs() < 0.05,
                "opacity-0 dim must clear after dropping on the same zone; opacity={}",
                item.style.opacity
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn activate_chip(sid: &str, pointer: [f32; 2], grab: [f32; 2], bounds: [f32; 4]) {
        use crate::utils::drag::{begin, tick, BeginDrag};
        begin(BeginDrag {
            scope_id: sid.into(),
            draggable_id: "chip".into(),
            pointer,
            grab_offset: grab,
            start_bounds: bounds,
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(
            pointer[0] + 10.0,
            pointer[1] + 4.0,
            &[],
            [800.0, 600.0]
        ));
        crate::app::reactive::drain_deferred_view_remounts();
    }

    fn hoist_and_layout(tree: &mut Element) {
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(tree);
        let mut lt = crate::layout::LayoutTree::new();
        let _ = lt.build_and_layout(tree, 800.0, 600.0, [0.0, 0.0]);
    }

    /// Empty `<DragOverlay>` must render the source item's text, not a dummy
    /// placeholder like Gallery's old `card_chip("handled")`.
    #[test]
    fn ghost_follows_source_text_by_default() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-follow").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("拖左侧手柄；点文字不会启动")
                        .w(200.0)
                        .h(40.0)
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [30.0, 8.0, 200.0, 40.0]);
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("拖左侧手柄")),
                "ghost must follow source text, got {texts:?}"
            );
            assert!(
                texts.iter().all(|t| t != "handled"),
                "ghost leaked placeholder text: {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Nested SortableItem body re-runs inside the overlay clone. Inner
    /// `assemble_draggable` must not reuse the live `reactive_view` tid
    /// (Gallery folder ghosts otherwise collapse to the header chip).
    #[test]
    fn overlay_clone_of_nested_sortable_keeps_inner_live_island() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, sortable_item, sortable_list, tick, BeginDrag,
        };

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-nest-clone").threshold(0.0).child(|| {
                let folder = sortable_item("folder:Later")
                    .item_type("folder")
                    .list("tree-root")
                    .index(0)
                    .child(|| {
                        let inner = sortable_list("tree-later").accept("file").child(|| {
                            sortable_item("tree-later:Gamma")
                                .item_type("file")
                                .list("tree-later")
                                .index(0)
                                .child(|| Element::new().text("Gamma").w(80.0).h(24.0))
                        });
                        Element::from(
                            vstack()
                                .w("100%")
                                .child(Element::new().text("Later"))
                                .child(inner),
                        )
                    });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([folder, overlay]))
            })
        });
        {
            let tree = live.borrow();
            let gamma = find_draggable(&tree, "tree-later:Gamma").expect("live Gamma before drag");
            let mut texts = Vec::new();
            collect_texts(gamma, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("Gamma")),
                "live inner chip must paint before clone, got {texts:?}"
            );
        }
        begin(BeginDrag {
            scope_id: "ov-nest-clone".into(),
            draggable_id: "folder:Later".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 200.0, 80.0],
            from_list: Some("tree-root".into()),
            from_index: Some(0),
            item_type: "folder".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(50.0, 20.0, &[], [800.0, 600.0]));
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let gamma = find_draggable(&tree, "tree-later:Gamma")
                .expect("clone must not steal the live inner island");
            let mut live_texts = Vec::new();
            collect_texts(gamma, &mut live_texts);
            assert!(
                live_texts.iter().any(|t| t.contains("Gamma")),
                "live Gamma text missing after clone, got {live_texts:?}"
            );
            let ghost = find_ghost(&tree).expect("ghost");
            let mut ghost_texts = Vec::new();
            collect_texts(ghost, &mut ghost_texts);
            assert!(
                ghost_texts.iter().any(|t| t.contains("Later")),
                "ghost must include the folder title, got {ghost_texts:?}"
            );
            assert!(
                ghost_texts.iter().any(|t| t.contains("Gamma")),
                "ghost must clone nested list body, not a header-only chip; got {ghost_texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_default_clone_is_undimmed_author_body() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-undimmed").threshold(0.0).child(|| {
                let chip = draggable("chip")
                    .child(|| Element::new().text("本体文案").w(80.0).h(32.0).opacity(1.0));
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [30.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.style.opacity - 1.0).abs() < 0.01,
                "default overlay must clone the undimmed author body; got {}",
                ghost.style.opacity
            );
            let source = find_draggable(&tree, "chip").expect("source");
            assert!(
                (source.style.opacity - 0.4).abs() < 0.05,
                "in-list source is what the kit dims; opacity={}",
                source.style.opacity
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_author_style_overrides_source_opacity() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-style").threshold(0.0).child(|| {
                let chip = draggable("chip")
                    .child(|| Element::new().text("本体文案").w(80.0).h(32.0).opacity(1.0));
                let overlay: Element = drag_overlay().style(|el| el.opacity(0.5)).into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [30.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.style.opacity - 0.5).abs() < 0.01,
                "DragOverlay style must override clone opacity; got {}",
                ghost.style.opacity
            );
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "本体文案"),
                "style overlay must keep source text, got {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Follow-source clone of a `w=100%` row must pin `drag_extent`, otherwise
    /// the percentage resolves against the portal / window and the ghost is a slab.
    #[test]
    fn ghost_follow_source_pins_used_extent() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-extent").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("拖左侧手柄；点文字不会启动")
                        .w("100%")
                        .h(48.0)
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [24.0, 16.0, 420.0, 48.0]);
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.layout_bounds[2] - 420.0).abs() < 8.0,
                "follow-source ghost must pin used width, not 100% of portal; bounds={:?}",
                ghost.layout_bounds
            );
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("拖左侧手柄")),
                "extent pin must not replace source text, got {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Auto-sized follow-source clone must not inherit the stretched island
    /// `start_bounds` (Gallery chip next to a tall Droppable).
    #[test]
    fn ghost_auto_sized_clone_ignores_stretched_extent() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-hug").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("源卡片")
                        .w(80.0)
                        .h(32.0)
                        .bg([0.2, 0.4, 0.9, 1.0])
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [24.0, 16.0, 420.0, 72.0]);
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                ghost.layout_bounds[2] < 160.0,
                "auto chip ghost must not pin stretched row width; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                ghost.layout_bounds[3] < 48.0,
                "auto chip ghost must not pin stretched row height; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                (ghost.style.bg_color[2] - 0.9).abs() < 0.05,
                "ghost must keep author fill; bg={:?}",
                ghost.style.bg_color
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_custom_child_keeps_its_own_text() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-custom").threshold(0.0).child(|| {
                let chip =
                    draggable("chip").child(|| Element::new().text("本体文案").w(80.0).h(32.0));
                let overlay: Element = drag_overlay()
                    .child(|| Element::new().text("custom-ghost").w(80.0).h(32.0))
                    .into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(&sid, [40.0, 16.0], [10.0, 8.0], [30.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "custom-ghost"),
                "custom overlay child must replace source, got {texts:?}"
            );
            assert!(
                texts.iter().all(|t| t != "本体文案"),
                "custom child leaked source text: {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_layout_keeps_extent_at_origin() {
        let panel = place_drag_ghost(
            Element::new()
                .w(96.0)
                .h(36.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .text("task"),
            "g1",
            [120.0, 80.0],
        );
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 901));
        assert!(
            crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1,
            "ghost must hoist out of the 0×0 mount"
        );
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let ghost = find_ghost(&root).expect("hoisted ghost");
        assert!(
            ghost.layout_bounds[2] > 50.0,
            "ghost width vanished (0×0 portal CB); w={}",
            ghost.layout_bounds[2]
        );
        assert!(
            ghost.layout_bounds[3] > 20.0,
            "ghost height vanished (0×0 portal CB); h={}",
            ghost.layout_bounds[3]
        );
        assert!(
            (ghost.layout_bounds[0] - 120.0).abs() < 2.0,
            "ghost x should follow origin; x={}",
            ghost.layout_bounds[0]
        );
        assert!(
            (ghost.layout_bounds[1] - 80.0).abs() < 2.0,
            "ghost y should follow origin; y={}",
            ghost.layout_bounds[1]
        );
    }

    /// Custom overlay child (a small chip) must keep its author size even when
    /// the source item is a `w=100%` row. Pinning `drag_extent` belongs only to
    /// the default follow-source clone.
    #[test]
    fn ghost_keeps_author_size_when_source_extent_is_wider() {
        let inner = Element::new()
            .w(80.0)
            .h(32.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("handled");
        let panel = place_drag_ghost(inner, "g1", [24.0, 16.0]);
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 901));
        assert!(crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let ghost = find_ghost(&root).expect("hoisted ghost");
        assert!(
            ghost.layout_bounds[2] > 40.0 && ghost.layout_bounds[2] < 120.0,
            "ghost must keep overlay-body size, not stretch to source row; bounds={:?}",
            ghost.layout_bounds
        );
        assert!(
            ghost.layout_bounds[3] > 16.0 && ghost.layout_bounds[3] < 40.0,
            "ghost height stretched to source row; bounds={:?}",
            ghost.layout_bounds
        );
        assert!(
            (ghost.layout_bounds[0] - 24.0).abs() < 2.0
                && (ghost.layout_bounds[1] - 16.0).abs() < 2.0,
            "origin lost; bounds={:?}",
            ghost.layout_bounds
        );
    }

    fn contains_portal(el: &Element) -> bool {
        crate::core::tree::find_by_attr(el, "rui_portal", "1").is_some()
    }

    /// Live `AppState::reconcile_subtree`: `reconcile_elements` then assign, then hoist.
    /// `install_test_reconcile_root` skips this and wholesale-replaces the slot.
    fn apply_like_appstate(root: &mut Element, anchor: &str, mut fresh: Element) {
        let want = anchor.replace('\\', "/").to_ascii_lowercase();
        let needs = {
            let Some(old) = crate::core::tree::find_in_tree_mut(root, |n| {
                n.template_id
                    .as_deref()
                    .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
            }) else {
                panic!("anchor {anchor} not in tree");
            };
            let _ = crate::app::test_support::reconcile_elements(&mut fresh, old);
            let needs = contains_portal(&fresh);
            *old = fresh;
            needs
        };
        if needs {
            let n = crate::layout::layout_tree::hoist_portal_subtrees(root);
            assert!(
                n >= 1,
                "open fragment had rui_portal but hoist moved 0 nodes"
            );
        }
    }

    fn open_ghost_at(tid: &str, origin: [f32; 2], inner_tid: Option<&str>) -> Element {
        let mut inner = Element::new()
            .w(96.0)
            .h(36.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("ghost");
        if let Some(t) = inner_tid {
            inner.template_id = Some(t.into());
        }
        let panel = place_drag_ghost(inner, "g1", origin);
        let mut el = mount_anchored_overlay(panel, 901);
        el.template_id = Some(tid.into());
        el
    }

    fn red_paint_at(
        result: &crate::layout::RenderResult,
        origin: [f32; 2],
        min_size: [f32; 2],
    ) -> bool {
        result.instances.iter().any(|i| {
            i.color[0] > 0.85
                && i.color[1] < 0.2
                && i.color[2] < 0.2
                && i.color[3] > 0.85
                && i.rect_size[0] >= min_size[0]
                && i.rect_size[1] >= min_size[1]
                && (i.rect_pos[0] - origin[0]).abs() < 3.0
                && (i.rect_pos[1] - origin[1]).abs() < 3.0
                && (i.clip_size[0] < 0.0
                    || (i.clip_size[0] >= min_size[0] && i.clip_size[1] >= min_size[1]))
        })
    }

    /// Gallery idle frame has already laid out the 0×0 placeholder. The first
    /// overlay open then a pointer move both go through `reconcile_elements`
    /// (slot vs new portal, equal child len). Wholesale test reconcile never
    /// hits this.
    #[test]
    fn appstate_reconcile_after_idle_layout_paints_ghost_and_follows() {
        use crate::app::reactive::reactive_placeholder_sized;

        const TID: &str = "ov-loop__drag_overlay";
        let placeholder = reactive_placeholder_sized(TID, 0.0, 0.0).pointer_events_box_none();
        let scroll = Element::new()
            .w(400.0)
            .h(120.0)
            .overflow("hidden")
            .child(placeholder);
        let later = Element::new().w(400.0).h(400.0).bg([0.1, 0.1, 0.1, 1.0]);
        let mut root = Element::new().w(800.0).h(600.0).child(scroll).child(later);

        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

        apply_like_appstate(
            &mut root,
            TID,
            open_ghost_at(TID, [40.0, 200.0], Some("card_chip")),
        );
        let painted = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g = find_ghost(&root).expect("ghost missing after first AppState reconcile");
        assert!(
            ghost_is_under_portal_layer(&root),
            "ghost not on portal layer after AppState open"
        );
        assert!(
            g.layout_bounds[2] > 50.0 && g.layout_bounds[3] > 20.0,
            "ghost 0-sized after AppState open: {:?}",
            g.layout_bounds
        );
        assert!(
            (g.layout_bounds[0] - 40.0).abs() < 2.0 && (g.layout_bounds[1] - 200.0).abs() < 2.0,
            "ghost not at overlay_pos after AppState open: {:?}",
            g.layout_bounds
        );
        assert!(
            red_paint_at(&painted, [40.0, 200.0], [50.0, 20.0]),
            "ghost has layout but paint missed it (clip/z). overlay_split.start={} instances={}",
            painted.overlay_split.instance_start,
            painted.instances.len()
        );

        let _ = crate::utils::popup::sync_floating_placement(&mut root, 800.0, 600.0);
        apply_like_appstate(
            &mut root,
            TID,
            open_ghost_at(TID, [80.0, 220.0], Some("card_chip")),
        );
        let painted2 = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let _ = crate::utils::popup::sync_floating_placement(&mut root, 800.0, 600.0);
        let painted3 = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g2 = find_ghost(&root).expect("ghost missing after pos tick reconcile");
        assert!(
            (g2.layout_bounds[0] - 80.0).abs() < 2.0 && (g2.layout_bounds[1] - 220.0).abs() < 2.0,
            "ghost not following after second AppState reconcile: {:?}",
            g2.layout_bounds
        );
        assert!(
            red_paint_at(&painted3, [80.0, 220.0], [50.0, 20.0])
                || red_paint_at(&painted2, [80.0, 220.0], [50.0, 20.0]),
            "ghost layout followed but paint did not. bounds={:?} n0={} n1={}",
            g2.layout_bounds,
            painted2.instances.len(),
            painted3.instances.len()
        );
    }

    #[test]
    fn appstate_reconcile_plain_inner_without_template_id_still_paints() {
        use crate::app::reactive::reactive_placeholder_sized;

        const TID: &str = "ov-plain__drag_overlay";
        let placeholder = reactive_placeholder_sized(TID, 0.0, 0.0);
        let mut root = Element::new().w(800.0).h(600.0).child(
            Element::new()
                .w(400.0)
                .h(80.0)
                .overflow("hidden")
                .child(placeholder),
        );

        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        apply_like_appstate(&mut root, TID, open_ghost_at(TID, [16.0, 24.0], None));
        apply_like_appstate(&mut root, TID, open_ghost_at(TID, [64.0, 48.0], None));
        let painted = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g = find_ghost(&root).expect("plain inner ghost missing");
        assert!(
            g.layout_bounds[2] > 50.0 && (g.layout_bounds[0] - 64.0).abs() < 2.0,
            "plain inner (no tid, pairs with portal slot) vanished: {:?}",
            g.layout_bounds
        );
        assert!(
            red_paint_at(&painted, [64.0, 48.0], [50.0, 20.0]),
            "plain inner ghost not in paint at overlay_pos"
        );
    }

    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }

    /// Fast sortable drag used to remount every item island on `hover_index` /
    /// `overlay_pos` (h=auto placeholders collapsed → list height jitter).
    #[test]
    fn sortable_preview_does_not_remount_or_jitter_height() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_scope, draggable, end, tick, Axis, BeginDrag, DragHit,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let runs = Rc::new(Cell::new(0));
        let (_o, live) = mount_scope_tree({
            let runs = runs.clone();
            move || {
                let runs = runs.clone();
                drag_scope("ov-sort-h")
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .child(move || {
                        let mk = |id: &'static str, idx: usize| {
                            let runs = runs.clone();
                            draggable(id)
                                .also_droppable(true)
                                .item_type("planet")
                                .list("planets")
                                .index(idx)
                                .child(move || {
                                    runs.set(runs.get() + 1);
                                    Element::new().text(id).w(80.0).h(32.0)
                                })
                        };
                        let mut list = Element::from(vstack().gap(8.0).children([
                            mk("mercury", 0),
                            mk("venus", 1),
                            mk("earth", 2),
                        ]));
                        list.template_id = Some("ov-sort-h-list".into());
                        list
                    })
            }
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }
        let h0 = find_tid(&live.borrow(), "ov-sort-h-list")
            .expect("list")
            .layout_bounds[3];
        assert!(
            h0 > 80.0,
            "precondition: three slotted rows must lay out; h={h0}"
        );
        let n0 = runs.get();
        assert!(n0 >= 3, "three author bodies should mount; runs={n0}");

        begin(BeginDrag {
            scope_id: "ov-sort-h".into(),
            draggable_id: "mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, y: f32, index: usize| DragHit {
            id: id.into(),
            scope_id: "ov-sort-h".into(),
            is_draggable: true,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, y, 80.0, 32.0]),
            item_type: "planet".into(),
            accept: String::new(),
            list_id: Some("planets".into()),
            index: Some(index),
            group: String::new(),
            disabled: false,
        };
        let hits = [
            hit("mercury", 0.0, 0),
            hit("venus", 40.0, 1),
            hit("earth", 80.0, 2),
        ];
        assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(
            tick(40.0, 88.0, &hits, [800.0, 600.0]),
            "pointer past last midpoint must keep the session active"
        );
        drain_deferred_view_remounts();
        assert_eq!(
            runs.get(),
            n0,
            "hover / overlay_pos must not remount author bodies; before={n0} after={}",
            runs.get()
        );

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let h1 = find_tid(&tree, "ov-sort-h-list")
                .expect("list after hover")
                .layout_bounds[3];
            assert!(
                (h1 - h0).abs() < 1.0,
                "sortable preview must not change list layout height; before={h0} after={h1}"
            );
            let venus = find_draggable(&tree, "venus").expect("venus");
            assert!(
                venus.style.translate[1].abs() > 1.0,
                "siblings must still shift via reactive_sync; translate={:?}",
                venus.style.translate
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Dimmed source occupies the insert hole (opacity 0.4). opacity 0 leaves
    /// that hole empty (opacity 0 at origin). Siblings still FLIP the gap.
    #[test]
    fn dimmed_source_occupies_insert_hole() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, end, tick, Axis, BeginDrag, DragHit,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-sort-src")
                .axis(Axis::Y)
                .threshold(0.0)
                .child(move || {
                    // Register overlay so chrome dims the source. Do not parent
                    // the ghost island: portal + sync_reactive overflowed the stack.
                    let _ = Element::from(drag_overlay());
                    let mk = |id: &'static str, idx: usize| {
                        draggable(id)
                            .also_droppable(true)
                            .item_type("planet")
                            .list("planets")
                            .index(idx)
                            .child(move || Element::new().text(id).w(80.0).h(32.0))
                    };
                    Element::from(vstack().gap(8.0).children([
                        mk("mercury", 0),
                        mk("venus", 1),
                        mk("earth", 2),
                    ]))
                })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        begin(BeginDrag {
            scope_id: "ov-sort-src".into(),
            draggable_id: "mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, y: f32, index: usize| DragHit {
            id: id.into(),
            scope_id: "ov-sort-src".into(),
            is_draggable: true,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, y, 80.0, 32.0]),
            item_type: "planet".into(),
            accept: String::new(),
            list_id: Some("planets".into()),
            index: Some(index),
            group: String::new(),
            disabled: false,
        };
        let hits = [
            hit("mercury", 0.0, 0),
            hit("venus", 40.0, 1),
            hit("earth", 80.0, 2),
        ];
        assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "mercury").expect("mercury at grab");
            let venus = find_draggable(&tree, "venus").expect("venus at grab");
            assert!(
                mercury.style.translate[1].abs() < 1.0,
                "grab at source centre must not swap with the next row; translate={:?}",
                mercury.style.translate
            );
            assert!(
                venus.style.translate[1].abs() < 1.0,
                "neighbour must not slide under the dimmed body at grab; translate={:?}",
                venus.style.translate
            );
        }
        assert!(tick(40.0, 88.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let mercury = find_draggable(&tree, "mercury").expect("mercury");
            let venus = find_draggable(&tree, "venus").expect("venus");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "source stays visible and dimmed; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1] > 40.0,
                "dimmed body must sit in the insert hole, not under siblings at origin; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source must paint above overlapping siblings; z={}",
                mercury.style.z_index
            );
            assert!(
                venus.style.translate[1] < -1.0,
                "neighbour must still vacate the hole; translate={:?}",
                venus.style.translate
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Default overlay (opacity 0.4): the dimmed source must translate onto the target
    /// list hole (not opacity 0 in the origin slot).
    #[test]
    fn dimmed_source_cross_list_appears_in_target_hole() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, end, tick, BeginDrag, DragHit,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-cross-src").threshold(0.0).child(move || {
                let _ = Element::from(drag_overlay());
                let mk = |id: &'static str, list: &'static str, idx: usize| {
                    draggable(id)
                        .also_droppable(true)
                        .item_type("task")
                        .group("board")
                        .list(list)
                        .index(idx)
                        .child(move || Element::new().text(id).w(80.0).h(32.0))
                };
                let left = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(move || Element::from(vstack().w(80.0).child(mk("src", "todo", 0))));
                let right = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(move || Element::from(vstack().w(80.0).child(mk("dst", "doing", 0))));
                Element::from(hstack().gap(40.0).children([left, right]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        begin(BeginDrag {
            scope_id: "ov-cross-src".into(),
            draggable_id: "src".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, list: &str, x: f32, y: f32, index: Option<usize>| DragHit {
            id: id.into(),
            scope_id: "ov-cross-src".into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([x, y, 80.0, 32.0]),
            item_type: "task".into(),
            accept: "task".into(),
            list_id: Some(list.into()),
            index,
            group: "board".into(),
            disabled: false,
        };
        let hits = [
            hit("src", "todo", 0.0, 0.0, Some(0)),
            hit("todo", "todo", 0.0, 0.0, None),
            hit("dst", "doing", 120.0, 0.0, Some(0)),
            hit("doing", "doing", 120.0, 0.0, None),
        ];
        assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(tick(140.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let src = find_draggable(&tree, "src").expect("src");
            assert!(
                (src.style.opacity - 0.4).abs() < 0.05,
                "cross-list with default overlay opacity must keep a dimmed body; opacity={}",
                src.style.opacity
            );
            assert!(
                src.style.translate[0] > 50.0,
                "dimmed source must translate into the target column; translate={:?}",
                src.style.translate
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Auto-height sortable lists: cross-list hover must shrink the source list
    /// (vacate the row in the parent flow), not only pad the target. After
    /// `apply_transfer` the origin list height must drop by about one row.
    #[test]
    fn nested_cross_list_transfer_shrinks_source_folder() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let later = Signal::new(vec!["Gamma".to_string()]);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            move || {
                drag_scope("ov-nest-shrink")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let mut dst = if to == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "tree-inbox" {
                                inbox.set(src);
                                later.set(dst);
                            } else {
                                later.set(src);
                                inbox.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let mk = |id: &'static str, files: Signal<Vec<String>>| {
                            sortable_list(id).accept("file").group("nest").child(move || {
                                let items = files.get();
                                let rows: Vec<Element> = items
                                    .iter()
                                    .enumerate()
                                    .map(|(i, name)| chip(id, name.clone(), i))
                                    .collect();
                                Element::from(
                                    vstack().w("100%").min_h(0.0).gap(6.0).children(rows),
                                )
                            })
                        };
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(200.0).min_h(0.0).gap(8.0).children([
                            mk("tree-inbox", inbox),
                            mk("tree-later", later),
                            overlay,
                        ]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let list_h = |tree: &Element, id: &str| find_droppable(tree, id).expect(id).layout_bounds[3];
        let pad_b = |tree: &Element, id: &str| {
            crate::layout::lp_length_px(find_droppable(tree, id).expect(id).style.padding.bottom)
                .unwrap_or(0.0)
        };
        let mar_b = |tree: &Element, id: &str| {
            crate::layout::lpa_length_px(find_droppable(tree, id).expect(id).style.margin.bottom)
                .unwrap_or(0.0)
        };

        let h0_inbox = list_h(&live.borrow(), "tree-inbox");
        let h0_later = list_h(&live.borrow(), "tree-later");
        assert!(
            h0_inbox > h0_later + 20.0,
            "precondition: Inbox (2 files) taller than Later (1); inbox={h0_inbox} later={h0_later}"
        );

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-shrink",
            "tree-inbox:Alpha",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-shrink".into(),
            draggable_id: "tree-inbox:Alpha".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0], pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let later_hit = hits0
            .iter()
            .find(|h| h.id == "tree-later")
            .expect("later list");
        let dest = [
            later_hit.rect.x + 20.0,
            later_hit.rect.y + later_hit.rect.height * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
        }

        let later_pad = pad_b(&live.borrow(), "tree-later");
        let inbox_margin = mar_b(&live.borrow(), "tree-inbox");
        assert!(
            later_pad > 16.0,
            "target list must pad by dragged extent; pad={later_pad}"
        );
        assert!(
            inbox_margin < -16.0,
            "source list must pull the vacated row out of flow; margin={inbox_margin}"
        );

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            hoist_and_layout(&mut tree);
        }

        assert_eq!(inbox.peek(), vec!["Beta".to_string()]);
        assert!(later.peek().iter().any(|s| s == "Alpha"));
        let h1_inbox = list_h(&live.borrow(), "tree-inbox");
        let h1_later = list_h(&live.borrow(), "tree-later");
        assert!(
            h1_later > h0_later + 16.0,
            "Later must grow after taking Alpha; before={h0_later} after={h1_later}"
        );
        assert!(
            h1_inbox < h0_inbox - 16.0,
            "Inbox must shrink after Alpha leaves; before={h0_inbox} after={h1_inbox}"
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery `kanban_column`: paint `reactive_sync` must see `hover_list`
    /// after Todo→Doing. Origin stays at rest. `use_drag()` walks `current_owner`;
    /// the frame prepass does not restore the provider owner — capture `DragCtx`
    /// at column build (same as `apply_live_droppable_chrome`).
    #[test]
    fn kanban_column_highlight_todo_to_doing() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::hstack;
        use crate::style::parse_color;
        use crate::utils::drag::{
            begin, drag_scope, droppable, end, tick, use_drag, BeginDrag, DragHit, ATTR_DRAG_OVER,
        };

        fn column(list: &'static str) -> Element {
            let ctx = use_drag();
            let mut el = Element::new().w(80.0).min_h(128.0);
            el.style.bg_color = parse_color("muted");
            el.style.border_color = parse_color("border");
            el.reactive_sync(move |el, _| {
                let over = ctx.as_ref().and_then(|c| c.hover_list.peek()).as_deref() == Some(list);
                el.style.bg_color = parse_color(if over { "accent" } else { "muted" });
                el.style.border_color = parse_color(if over { "primary" } else { "border" });
                LayoutStatus::StyleChanged
            })
        }

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("gal-drag-board").threshold(0.0).child(move || {
                let left = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(|| column("todo"));
                let right = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(|| column("doing"));
                Element::from(hstack().gap(8.0).children([left, right]))
            })
        });

        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "task-a".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, list: &str, x: f32, index: Option<usize>| DragHit {
            id: id.into(),
            scope_id: "gal-drag-board".into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([x, 0.0, 80.0, 128.0]),
            item_type: "task".into(),
            accept: "task".into(),
            list_id: Some(list.into()),
            index,
            group: "board".into(),
            disabled: false,
        };
        let hits = [
            hit("task-a", "todo", 0.0, Some(0)),
            hit("todo", "todo", 0.0, None),
            hit("doing", "doing", 88.0, None),
        ];
        assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(tick(128.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            assert!(
                crate::core::reactivity::current_owner().is_none(),
                "paint prepass has no component owner — same as AppState::render"
            );
            let doing = find_droppable(&tree, "doing").expect("doing");
            assert_eq!(
                doing.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "engine chrome must mark the hover column; collision is not the bug"
            );
            assert_eq!(
                doing.style.bg_color,
                parse_color("accent"),
                "Doing column must highlight from hover_list at paint; got {:?}",
                doing.style.bg_color
            );
            assert_eq!(doing.style.border_color, parse_color("primary"));
            let todo = find_droppable(&tree, "todo").expect("todo");
            assert_ne!(
                todo.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "origin column is not the drop target"
            );
            assert_eq!(
                todo.style.border_color,
                parse_color("border"),
                "Todo origin column stays at rest; only the hover target highlights"
            );
            assert_eq!(todo.style.bg_color, parse_color("muted"));
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery Kanban Todo: `Droppable` + `for_keyed` + `w=100%` also_droppable
    /// islands. Lift Review PR must keep hover=from and the dimmed body must not
    /// translate (no yield to the overlay clone).
    #[test]
    fn kanban_todo_lift_review_pr_keeps_source_slot() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, droppable, end, insert_index_along_axis,
            sortable_item, start_geometry, tick, Axis, BeginDrag, CollisionAlg,
        };

        const NAMES: [&str; 2] = ["Draft RFC", "Review PR"];
        let (_o, live) = mount_scope_tree(move || {
            drag_scope("gal-drag-board")
                .collision(CollisionAlg::MostIntersecting)
                .axis(Axis::Y)
                .threshold(0.0)
                .child(move || {
                    let todo = droppable("todo")
                        .list("todo")
                        .group("board")
                        .accept("task")
                        .child(|| {
                            let todo_rows = for_keyed(
                                "gal-drag-todo-rows",
                                &NAMES,
                                |s: &&str| (*s).to_string(),
                                |name: &&str| {
                                    let id = format!("todo:{}", *name);
                                    let i = NAMES.iter().position(|n| n == name).unwrap_or(0);
                                    sortable_item(id)
                                        .item_type("task")
                                        .group("board")
                                        .list("todo")
                                        .index(i)
                                        .child({
                                            let label = (*name).to_string();
                                            move || {
                                                Element::new()
                                                    .text(label.clone())
                                                    .w("100%")
                                                    .h(32.0)
                                                    .min_h(32.0)
                                            }
                                        })
                                },
                            );
                            Element::from(vstack().w("100%").gap(6.0).p(8.0).children(todo_rows))
                        });
                    let doing = droppable("doing")
                        .list("doing")
                        .group("board")
                        .accept("task")
                        .child(|| Element::from(vstack().w("100%").p(8.0).min_h(128.0)));
                    let overlay: Element = drag_overlay().into();
                    Element::from(hstack().gap(8.0).w(400.0).children([todo, doing, overlay]))
                })
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits = collect_hits(&live.borrow());
        let todo_rows: Vec<_> = hits
            .iter()
            .filter(|h| h.list_id.as_deref() == Some("todo") && h.index.is_some())
            .collect();
        assert_eq!(
            todo_rows.len(),
            2,
            "todo column must expose two indexed rows; hits={:?}",
            hits.iter()
                .map(|h| (h.id.clone(), h.index, h.rect.y, h.rect.height))
                .collect::<Vec<_>>()
        );
        let y0 = todo_rows[0].rect.y;
        let y1 = todo_rows[1].rect.y;
        assert!(
            (y1 - y0) >= 8.0,
            "row origins must be distinct (not clustered); y0={y0} y1={y1}"
        );
        for h in &todo_rows {
            assert!(
                h.rect.height > 8.0 && h.rect.height < 80.0,
                "row slot must be chip-sized, not the column shell; {} h={}",
                h.id,
                h.rect.height
            );
        }

        let review = todo_rows
            .iter()
            .find(|h| h.index == Some(1))
            .expect("Review PR row");
        let pointer = [
            review.rect.x + review.rect.width * 0.5,
            review.rect.y + review.rect.height * 0.5,
        ];
        let (grab, start_bounds) = start_geometry(
            "gal-drag-board",
            "todo:Review PR",
            pointer,
            [
                review.rect.x,
                review.rect.y,
                review.rect.width,
                review.rect.height,
            ],
        );

        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            pointer,
            grab_offset: grab,
            start_bounds,
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hits = collect_hits(&live.borrow());
        assert!(tick(pointer[0], pointer[1], &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        assert_eq!(
            insert_index_along_axis(&hits, "todo", pointer, Axis::Y),
            Some(1),
            "insert index must match the lifted row; hits={:?}",
            hits.iter()
                .map(|h| (h.id.clone(), h.index, h.rect.y, h.rect.height))
                .collect::<Vec<_>>()
        );

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let review_el =
                find_draggable_in_scope(&tree, "gal-drag-board", "todo:Review PR").expect("review");
            let draft_el =
                find_draggable_in_scope(&tree, "gal-drag-board", "todo:Draft RFC").expect("draft");
            assert!(
                review_el.style.translate[0].abs() < 1.0
                    && review_el.style.translate[1].abs() < 1.0,
                "dimmed source must not cross-translate or yield at lift; translate={:?}",
                review_el.style.translate
            );
            assert!(
                draft_el.style.translate[1].abs() < 1.0,
                "sibling must not shift at lift; translate={:?}",
                draft_el.style.translate
            );
            assert!(
                (review_el.style.opacity - 0.4).abs() < 0.05,
                "source stays dimmed; opacity={}",
                review_el.style.opacity
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery「Sortable 列表 · 半透明本体」: `SortableList` + `for_keyed` +
    /// `DragOverlay` (default source opacity 0.4). Drag must not collapse the source row,
    /// the dimmed body stays in-list, and commit follows the hovered slot —
    /// not always `len` (tail insert).
    #[test]
    fn gallery_sortable_without_hide_source_keeps_dimmed_body() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_reorder_vec, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child(move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets")
                                            .index(i)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let item_hits: Vec<_> = hits0
            .iter()
            .filter(|h| h.index.is_some() && h.list_id.as_deref() == Some("planets"))
            .cloned()
            .collect();
        assert_eq!(
            item_hits.len(),
            8,
            "eight SortableItems must stay in the hit list; hits={:?}",
            hits0
                .iter()
                .map(|h| (h.id.clone(), h.index, h.rect.height))
                .collect::<Vec<_>>()
        );
        for h in &item_hits {
            assert!(
                h.rect.height > 8.0 && h.rect.height < 80.0,
                "row slot must be a chip, not the list shell; {} h={}",
                h.id,
                h.rect.height
            );
        }

        let src = item_hits
            .iter()
            .find(|h| h.id == "Mercury")
            .expect("Mercury");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0], pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "Mercury").expect("Mercury must stay in the list");
            assert!(
                mercury.layout_bounds[3] > 8.0,
                "source row must not collapse to 0; bounds={:?}",
                mercury.layout_bounds
            );
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "in-list source is dimmed, not opacity 0; opacity={}",
                mercury.style.opacity
            );
        }

        let at_lift = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_lift,
            Some(0),
            "lift on row 0 must keep hover=from so the dimmed body stays; got {at_lift:?}"
        );

        let earth = item_hits.iter().find(|h| h.id == "Earth").expect("Earth");
        let earth_p = [earth.rect.x + 20.0, earth.rect.y + earth.rect.height * 0.5];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(earth_p[0], earth_p[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "Mercury").expect("Mercury after move");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "dimmed body must remain while hovering another row; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1] > 8.0,
                "dimmed body must occupy the Earth hole, not stay hidden at origin; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source must paint above siblings; z={}",
                mercury.style.z_index
            );
        }
        let at_earth = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_earth,
            Some(2),
            "hovering Earth must open slot 2, not append at len; got {at_earth:?}"
        );

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            committed.get(),
            Some(Some(2)),
            "commit must use the hovered slot, not the tail; got {:?}",
            committed.get()
        );
        let mut order: Vec<&str> = names.to_vec();
        apply_reorder_vec(&mut order, 0, 2);
        assert_eq!(order[2], "Mercury");

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Same Gallery path, but the list sits below a 400px spacer (page
    /// ScrollArea offset). Insert-index must use the visual slots, not fall
    /// through to `len` because the pointer Y is large.
    #[test]
    fn gallery_sortable_below_page_offset_does_not_append() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                let spacer = Element::new().w(240.0).h(400.0).min_h(400.0);
                let list = drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child(move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-offset-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets")
                                            .index(i)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-offset-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    });
                Element::from(vstack().w(240.0).children([spacer, list]))
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "Mercury" && h.index == Some(0))
            .expect("Mercury");
        assert!(
            src.rect.y > 200.0,
            "precondition: list is below the spacer; mercury.y={}",
            src.rect.y
        );
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0], pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let at_lift = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_lift,
            Some(0),
            "page-offset lift must keep hover=from, not len; got {at_lift:?}"
        );
        let hits1 = collect_hits(&live.borrow());
        assert!(end(&hits1));
        assert_eq!(
            committed.get(),
            Some(Some(0)),
            "commit on the source slot must not append; got {:?}",
            committed.get()
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery stacks opacity-0 above the dimmed list (same planet names,
    /// outer `scroll-overlays`). Dimmed chrome must match opacity-0 except
    /// opacity 0.4, and moving to Earth must not append.
    #[test]
    fn gallery_stacked_dimmed_matches_hide_source() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                let hide = drag_scope("gal-drag-sort-hide")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .child(|| {
                        let list = sortable_list("planets-hide").accept("planet").child({
                            let names = names;
                            move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-hide-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets-hide")
                                            .index(i)
                                            .source_opacity(0.0)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            }
                        });
                        let overlay: Element = drag_overlay().into();
                        let scroll: Element = scroll_area("gal-drag-sort-hide-scroll")
                            .h(176.0)
                            .w("100%")
                            .overflow_y("auto")
                            .child(list)
                            .into();
                        Element::from(vstack().w(240.0).children([scroll, overlay]))
                    });
                let dimmed = drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child({
                                let names = names;
                                move || {
                                    let rows = for_keyed(
                                        "gal-drag-sort-rows",
                                        &names,
                                        |s: &&str| (*s).to_string(),
                                        |name: &&str| {
                                            let id = (*name).to_string();
                                            let i =
                                                names.iter().position(|n| n == name).unwrap_or(0);
                                            sortable_item(id.clone())
                                                .item_type("planet")
                                                .list("planets")
                                                .index(i)
                                                .child({
                                                    let id = id.clone();
                                                    move || {
                                                        Element::new()
                                                            .text(id.clone())
                                                            .w(200.0)
                                                            .h(32.0)
                                                    }
                                                })
                                        },
                                    );
                                    Element::from(vstack().w("100%").gap(6.0).children(rows))
                                }
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    });
                let page: Element = scroll_area("scroll-overlays")
                    .h(600.0)
                    .w(240.0)
                    .overflow_y("auto")
                    .child(Element::from(vstack().w(240.0).children([hide, dimmed])))
                    .into();
                page
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let item_hits: Vec<_> = hits0
            .iter()
            .filter(|h| h.index.is_some() && h.list_id.as_deref() == Some("planets"))
            .cloned()
            .collect();
        assert_eq!(item_hits.len(), 8, "dimmed list must keep eight rows");
        let src = item_hits
            .iter()
            .find(|h| h.id == "Mercury")
            .expect("Mercury");
        let earth = item_hits.iter().find(|h| h.id == "Earth").expect("Earth");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height.max(8.0) * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0], pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury =
                find_draggable_in_scope(&tree, "gal-drag-sort", "Mercury").expect("dimmed Mercury");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "stacked dimmed source must stay at 0.4, not hide; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1].abs() < 1.0,
                "lift on from=hover keeps the dimmed body in slot; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source is stacked above siblings; z={}",
                mercury.style.z_index
            );
            assert!(
                mercury.layout_bounds[3] > 4.0,
                "dimmed body must not collapse; bounds={:?}",
                mercury.layout_bounds
            );
        }
        let earth_p = [
            earth.rect.x + 20.0,
            earth.rect.y + earth.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(earth_p[0], earth_p[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury =
                find_draggable_in_scope(&tree, "gal-drag-sort", "Mercury").expect("dimmed Mercury");
            assert!(
                mercury.style.translate[1] > 8.0,
                "stacked dimmed body must occupy the Earth hole; translate={:?}",
                mercury.style.translate
            );
        }
        let at_earth = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_earth,
            Some(2),
            "stacked dimmed list must open Earth, not append; got {at_earth:?}"
        );
        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            committed.get(),
            Some(Some(2)),
            "commit must use Earth, not the tail; got {:?}",
            committed.get()
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn activate_named(sid: &str, id: &str, pointer: [f32; 2], bounds: [f32; 4]) {
        use crate::utils::drag::{begin, tick, BeginDrag};
        begin(BeginDrag {
            scope_id: sid.into(),
            draggable_id: id.into(),
            pointer,
            grab_offset: [10.0, 8.0],
            start_bounds: bounds,
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(
            pointer[0] + 16.0,
            pointer[1] + 8.0,
            &[],
            [800.0, 600.0]
        ));
        crate::app::reactive::drain_deferred_view_remounts();
    }

    fn sync_drag_chrome(tree: &mut Element) {
        let mut lt = crate::layout::LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            tree,
            &mut lt,
            &mut impact,
            800.0,
            600.0,
            [0.0, 0.0],
        );
    }

    fn two_free_draggables(
        scope: &'static str,
        stack_a: crate::utils::drag::DragStack,
        stack_b: crate::utils::drag::DragStack,
        z_a: i32,
        z_b: i32,
    ) -> (
        crate::core::reactivity::OwnerHandle,
        std::rc::Rc<std::cell::RefCell<Element>>,
    ) {
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_scope, draggable};
        mount_scope_tree(move || {
            drag_scope(scope).threshold(0.0).child(move || {
                let a = draggable("a").stack(stack_a).child(move || {
                    Element::new()
                        .text("Draggable 1")
                        .w(80.0)
                        .h(32.0)
                        .z_index(z_a)
                });
                let b = draggable("b").stack(stack_b).child(move || {
                    Element::new()
                        .text("Draggable 2")
                        .w(80.0)
                        .h(32.0)
                        .z_index(z_b)
                });
                Element::from(vstack().children([a, b]))
            })
        })
    }

    /// Default `stack=auto`: dragging raises the item above its sibling and the
    /// new order survives `end` (solid-dnd arbitrary move).
    #[test]
    fn auto_stack_raises_on_drag_and_keeps_after_end() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) = two_free_draggables("stack-auto", DragStack::Auto, DragStack::Auto, 0, 0);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a rest");
            let b = find_draggable(&tree, "b").expect("b rest");
            assert_eq!(a.style.z_index, 0);
            assert_eq!(b.style.z_index, 0);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "a", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a drag");
            let b = find_draggable(&tree, "b").expect("b drag");
            assert!(
                a.style.z_index > b.style.z_index,
                "dragged auto must sit above sibling; a={} b={}",
                a.style.z_index,
                b.style.z_index
            );
            assert!(a.style.z_index >= 1);
        }
        assert!(end(&[]));
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a end");
            let b = find_draggable(&tree, "b").expect("b end");
            assert!(
                a.style.z_index > b.style.z_index,
                "auto rank must persist after drop; a={} b={}",
                a.style.z_index,
                b.style.z_index
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// `stack=keep`: author z-index is unchanged while dragging, even when the
    /// lower item is the active drag.
    #[test]
    fn keep_stack_preserves_author_z_while_dragging() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) = two_free_draggables("stack-keep", DragStack::Keep, DragStack::Keep, 1, 4);
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "a", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a");
            let b = find_draggable(&tree, "b").expect("b");
            assert_eq!(a.style.z_index, 1, "keep must not raise the dragged item");
            assert_eq!(b.style.z_index, 4);
            assert!(a.style.z_index < b.style.z_index);
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Same author z + `keep`: both stay equal (tree order paints).
    #[test]
    fn keep_stack_same_author_z_stays_equal() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) =
            two_free_draggables("stack-keep-eq", DragStack::Keep, DragStack::Keep, 2, 2);
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "b", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a");
            let b = find_draggable(&tree, "b").expect("b");
            assert_eq!(a.style.z_index, 2);
            assert_eq!(b.style.z_index, 2);
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// `top` stays above a dragged `auto`; `bottom` stays below even when dragged.
    #[test]
    fn top_and_bottom_bands_ignore_drag_raise() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_scope, draggable, end, DragStack};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("stack-bands").threshold(0.0).child(|| {
                let top = draggable("top")
                    .stack(DragStack::Top)
                    .child(|| Element::new().text("top").w(80.0).h(32.0).z_index(0));
                let auto = draggable("auto")
                    .stack(DragStack::Auto)
                    .child(|| Element::new().text("auto").w(80.0).h(32.0).z_index(0));
                let bottom = draggable("bottom")
                    .stack(DragStack::Bottom)
                    .child(|| Element::new().text("bottom").w(80.0).h(32.0).z_index(0));
                Element::from(vstack().children([top, auto, bottom]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let top = find_draggable(&tree, "top").expect("top rest");
            let bottom = find_draggable(&tree, "bottom").expect("bottom rest");
            assert!(top.style.z_index >= DragStack::TOP_BAND);
            assert!(bottom.style.z_index <= DragStack::BOTTOM_BAND + 1);
            assert!(top.style.z_index > bottom.style.z_index);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "auto", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let top = find_draggable(&tree, "top").expect("top");
            let auto = find_draggable(&tree, "auto").expect("auto");
            let bottom = find_draggable(&tree, "bottom").expect("bottom");
            assert!(
                auto.style.z_index > bottom.style.z_index,
                "auto drag must still sit above always-bottom"
            );
            assert!(
                top.style.z_index > auto.style.z_index,
                "always-top must stay above dragged auto; top={} auto={}",
                top.style.z_index,
                auto.style.z_index
            );
        }
        let _ = end(&[]);
        activate_named(&sid, "bottom", [20.0, 48.0], [8.0, 40.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let auto = find_draggable(&tree, "auto").expect("auto rest rank");
            let bottom = find_draggable(&tree, "bottom").expect("bottom drag");
            assert!(
                bottom.style.z_index < auto.style.z_index,
                "dragging always-bottom must not raise it above auto; bottom={} auto={}",
                bottom.style.z_index,
                auto.style.z_index
            );
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn find_item_shell<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.children.iter().any(|c| {
            c.attrs
                .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
                .map(String::as_str)
                == Some(id)
        }) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_item_shell(c, id))
    }

    /// Gallery wraps overlapping pills in a relative arena under DragProvider,
    /// and the page ScrollArea already owns `page_stack_depth`. The item shell
    /// must be a stacking-context so each pill paints `rects → text` (otherwise
    /// the lower glyph sits on the upper fill).
    #[test]
    fn wrapped_arena_inside_page_stack_keeps_glyphs_with_shell() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::layout::LayoutTree;
        use crate::utils::drag::{drag_scope, draggable};
        let (_owner, live) = mount_scope_tree(move || {
            let drag = drag_scope("gal-wrap-stack").threshold(0.0).child(move || {
                let a = draggable("a").child(move || {
                    Element::new()
                        .bg([0.1, 0.2, 0.9, 1.0])
                        .w(176.0)
                        .h(92.0)
                        .absolute()
                        .left(16.0)
                        .top(20.0)
                        .child(Element::new().text("A"))
                });
                let b = draggable("b").child(move || {
                    Element::new()
                        .bg([0.9, 0.1, 0.1, 1.0])
                        .w(176.0)
                        .h(92.0)
                        .absolute()
                        .left(88.0)
                        .top(52.0)
                        .child(Element::new().text("B"))
                });
                Element::new().w(400.0).h(200.0).relative().children([a, b])
            });
            let bar = Element::new()
                .w(8.0)
                .h(80.0)
                .absolute()
                .right(0.0)
                .z_index(1000)
                .bg([0.0, 0.0, 0.0, 0.4]);
            Element::new()
                .w(400.0)
                .h(240.0)
                .relative()
                .child(drag)
                .child(bar)
        });
        {
            let tree = live.borrow();
            let shell_a = find_item_shell(&tree, "a").expect("shell a");
            let shell_b = find_item_shell(&tree, "b").expect("shell b");
            assert_eq!(
                shell_a
                    .attrs
                    .get("data-stacking-context")
                    .map(String::as_str),
                Some("1"),
                "item shell must opt into nested page_stack splits"
            );
            assert_eq!(
                shell_b
                    .attrs
                    .get("data-stacking-context")
                    .map(String::as_str),
                Some("1")
            );
        }
        let mut tree = live.borrow_mut();
        sync_drag_chrome(&mut tree);
        let mut lt = LayoutTree::new();
        let painted = lt.build_and_layout(&mut tree, 400.0, 240.0, [0.0, 0.0]);
        let ranges = painted.overlay_split.page_stack_band_ranges();
        let band_of = |label: &str| {
            ranges.iter().position(|r| {
                painted.text_nodes[r.text.0..r.text.1]
                    .iter()
                    .any(|t| t.text == label)
            })
        };
        let a = band_of("A");
        let b = band_of("B");
        assert!(
            a.is_some() && b.is_some() && a != b,
            "overlapping pills must not share a paint band (A={a:?} B={b:?} bands={})",
            ranges.len()
        );
        assert!(
            b > a,
            "later sibling B must paint after A so its fill occludes A's glyph (A={a:?} B={b:?})"
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }
}
