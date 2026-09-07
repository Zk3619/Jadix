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

use crate::app::portal_overlay::{OverlayKind, OverlayMode, portal_sync};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::isolate_reads;
use crate::core::tree::Element;
use crate::layout::{DimKind, dim_kind};
use crate::utils::popup::{ATTR_FLOATING, mount_anchored_overlay};

use super::builder::{clone_source_preview, require_drag};
use super::session::{mark_overlay_registered, with_scope};
use super::{
    ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE, ATTR_DRAG_GROUP,
    ATTR_DRAG_HANDLE, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX, ATTR_DRAG_LIST, ATTR_DRAG_OVERLAY,
    ATTR_DRAG_SCOPE, ATTR_DRAG_TYPE,
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
    hide_source: bool,
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

    /// Hide the source island completely while the overlay is open (only the ghost).
    /// Default is a dimmed source (`opacity: 0.4`).
    pub fn hide_source(mut self, yes: bool) -> Self {
        self.hide_source = yes;
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
        "rui_drag_over",
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
        with_scope(&ctx.id, |rt| rt.hide_source = builder.hide_source);
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
        hide_source: false,
    }
}

/// RSX `<DragOverlay>` with children — same implementation as the Builder.
pub fn drag_overlay_from_rsx(body: impl Fn() -> Element + 'static) -> Element {
    drag_overlay().child(body).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{Signal, create_render_effect, create_root};
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
    fn overlay_hide_source_marks_scope() {
        use crate::utils::drag::{drag_overlay, drag_scope};
        let (_o, _) = create_root(|| {
            let _el: Element =
                drag_scope("hide-src").child(|| Element::from(drag_overlay().hide_source(true)));
            assert!(
                crate::utils::drag::overlay_hides_source("hide-src"),
                "DragOverlay.hide_source must stick on the scope for assemble_draggable"
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
            BeginDrag, begin, drag_overlay, drag_scope, draggable, overlay_is_registered, tick,
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
            BeginDrag, begin, drag_overlay, drag_scope, draggable, droppable, end, tick,
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
                    let overlay: Element = drag_overlay().hide_source(true).into();
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
            let tree = live.borrow();
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
                "hide_source dim must clear after dropping on the same zone; opacity={}",
                item.style.opacity
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn activate_chip(sid: &str, pointer: [f32; 2], grab: [f32; 2], bounds: [f32; 4]) {
        use crate::utils::drag::{BeginDrag, begin, tick};
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
}
