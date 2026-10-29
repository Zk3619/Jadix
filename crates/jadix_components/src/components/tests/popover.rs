use super::*;
use crate::core::reactivity::create_root;

#[test]
fn side_align_defaults_to_bottom_center() {
    assert_eq!(
        popover_side_align_to_placement("bottom", "center"),
        PopupPlacement::BottomCenter
    );
    assert_eq!(
        popover_side_align_to_placement("bottom", "start"),
        PopupPlacement::BottomStart
    );
    assert_eq!(
        popover_side_align_to_placement("top-end", ""),
        PopupPlacement::TopEnd
    );
}

#[test]
fn popover_surface_uses_popup_shadow() {
    let el = popover_surface(Element::new().text("hi"), None);
    let (c, b) = crate::tokens::elevation_shadow(crate::tokens::Elevation::Md);
    assert_eq!(el.style.shadow_color, c);
    assert!((el.style.shadow_blur - b).abs() < 0.01);
    assert_eq!(el.style.shadow_offset, [0.0, 0.0]);
}

#[test]
fn content_anchor_is_zero_sized() {
    let (_owner, el) = create_root(|| {
        popover_scope("pop-anchor", None, || {
            popover_content(PopoverContentOpts::default(), || Element::new().text("hi"))
        })
    });
    fn find_zero(el: &Element) -> bool {
        let is_view = el.template_id.as_deref() == Some("pop-anchor__view");
        let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
        (is_view && dim0) || el.children.iter().any(find_zero)
    }
    fn find_abs_zero(el: &Element) -> bool {
        let dim0 = crate::layout::dim_length_px(el.style.size.width).is_some_and(|w| w == 0.0)
            && crate::layout::dim_length_px(el.style.size.height).is_some_and(|h| h == 0.0);
        (dim0 && el.style.position == taffy::style::Position::Absolute)
            || el.children.iter().any(find_abs_zero)
    }
    assert!(
        find_zero(&el),
        "PopoverContent must use a 0×0 portal anchor"
    );
    assert!(
        find_abs_zero(&el),
        "PopoverContent leftover host must be out of flow"
    );
}

#[test]
fn popover_open_in_wrap_row_keeps_neighbor_gap() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_effect, use_signal};
    use crate::core::tree::IntoElementList;
    use crate::layout::layout_tree::{hoist_portal_subtrees, LayoutTree};
    use crate::primitives::r#box::hstack;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn pack_popover(id: &str, open: Signal<bool>, btn_tid: &str) -> Element {
        popover_scope(id, Some(open), || {
            let trigger = PopoverTrigger(PopoverTriggerProps {
                id: String::new(),
                children: Some(
                    Element::new()
                        .w(40.0)
                        .h(20.0)
                        .flex_shrink(0.0)
                        .template_id(btn_tid),
                ),
                base: None,
            });
            let content = popover_content(PopoverContentOpts::default(), || {
                Element::new().text("hi").w(80.0).h(40.0)
            });
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![trigger, content])
        })
    }

    fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(id) {
            Some(el)
        } else {
            el.children.iter().find_map(|c| find_tid(c, id))
        }
    }

    fn neighbor_gap(el: &Element, a: &str, b: &str) -> f32 {
        let aa = find_tid(el, a)
            .unwrap_or_else(|| panic!("missing `{a}`"))
            .layout_bounds;
        let bb = find_tid(el, b)
            .unwrap_or_else(|| panic!("missing `{b}`"))
            .layout_bounds;
        bb[0] - (aa[0] + aa[2])
    }

    clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open_a) = create_root(|| {
        let open_a = use_signal("pop_gap_a__open", || false);
        let open_b = use_signal("pop_gap_b__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let mut row = hstack()
                .gap("0.5rem")
                .items("center")
                .wrap("wrap")
                .w("100%");
            for child in pack_popover("pop-gap-a", open_a, "pop-gap-a-btn").into_element_list() {
                row = row.child(child);
            }
            for child in pack_popover("pop-gap-b", open_b, "pop-gap-b-btn").into_element_list() {
                row = row.child(child);
            }
            *live_e.borrow_mut() = row.into();
        });
        open_a
    });

    let layout_once = |live: &RefCell<Element>, layout: &mut LayoutTree| {
        drain_deferred_view_remounts();
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        layout.build_and_layout(&mut tree, 800.0, 400.0, [0.0, 0.0])
    };

    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    let _ = layout_once(&live, &mut layout);
    let gap_closed = neighbor_gap(&live.borrow(), "pop-gap-a-btn", "pop-gap-b-btn");
    assert!(
        gap_closed > 4.0 && gap_closed < 16.0,
        "closed wrap-row gap should be one `0.5rem`, got {gap_closed}"
    );

    open_a.set(true);
    drain_deferred_view_remounts();
    let _ = layout_once(&live, &mut layout);
    let gap_open = neighbor_gap(&live.borrow(), "pop-gap-a-btn", "pop-gap-b-btn");
    assert!(
        (gap_open - gap_closed).abs() < 1.0,
        "opening Popover must not eat/double neighbor gap; closed={gap_closed} open={gap_open}"
    );

    drop(owner);
    clear_test_reconcile_root();
    clear_overlay_stack();
    reset_registered_anchors();
}

#[test]
fn scope_exposes_template() {
    let (_owner, el) = create_root(|| {
        popover("pop-scope")
            .open_signal(use_signal("pop-scope-open", || false))
            .child(|| Element::new().text("x"))
    });
    assert_eq!(el.template_id.as_deref(), Some("pop-scope__popover"));
}

#[test]
#[cfg(feature = "animation-base")]
fn content_keeps_panel_tid_while_exiting() {
    use crate::app::portal_overlay::clear_overlay_stack;
    use crate::core::reactivity::Signal;
    use crate::utils::anchored_overlay::{
        anchored_overlay_is_mounted, anchored_overlay_phase, apply_anchored_overlay_panel_motion,
        AnchoredOverlayContentOpts,
    };
    use crate::utils::presence::PresencePhase;

    clear_overlay_stack();
    create_root(|| {
        let open = Signal::new(true);
        let id = "pop-exit-smoke";
        let opts = AnchoredOverlayContentOpts {
            kind: OverlayKind::Popover,
            motion: FloatingMotionKind::Fade,
            mode: OverlayMode::Stacked,
            ..Default::default()
        };
        assert_eq!(
            anchored_overlay_phase(id, open, &opts),
            Some(PresencePhase::Present)
        );

        open.set(false);
        assert_eq!(
            anchored_overlay_phase(id, open, &opts),
            Some(PresencePhase::Exiting),
            "must stay mounted while exiting"
        );
        assert!(anchored_overlay_is_mounted(id, &opts));

        let mut panel = Element::new();
        apply_anchored_overlay_panel_motion(id, &mut panel, &opts, open);
        assert_eq!(
            panel.template_id.as_deref(),
            Some("pop-exit-smoke__floating_panel")
        );
    });
    clear_overlay_stack();
}
