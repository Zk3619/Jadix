//! Empty fullscreen portal layer must not steal wheel/scroll hit chain from shell scroll areas.

use crate::app::scroll_support::collect_scroll_rest_chain_at;
use crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX;
use crate::core::tree::Element;
use crate::layout::layout_tree::{
    hoist_portal_subtrees, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
};
use crate::layout::layout_tree::LayoutTree;

#[test]
fn empty_portal_layer_does_not_block_scroll_chain_underneath() {
    // Shell: sidebar scroll (left) + main scroll (right), then a fullscreen portal layer on top.
    let mut side = Element::new()
        .w(200.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"))
        .template_id("side")
        .child(Element::new().w(200.0).h(2000.0));
    side.layout_bounds = [0.0, 0.0, 200.0, 600.0];

    let mut main = Element::new()
        .flex_grow(1.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}root-scroll"))
        .template_id("main")
        .child(Element::new().w(800.0).h(3000.0));
    main.layout_bounds = [200.0, 0.0, 800.0, 600.0];

    let mut layer = Element::new()
        .absolute()
        .inset(0.0)
        .w(1000.0)
        .h(600.0)
        .template_id(PORTAL_LAYER_ID);
    // Closed portal host: full-screen absolute wrapper with no interactive children.
    let mut ghost = Element::new()
        .absolute()
        .inset(0.0)
        .w(1000.0)
        .h(600.0)
        .pointer_events_box_none()
        .template_id("closed-portal");
    ghost.set_attr(ATTR_HOISTED_PORTAL, "1");
    ghost.layout_bounds = [0.0, 0.0, 1000.0, 600.0];
    layer.children.push(ghost);
    layer.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut root = Element::new()
        .flex_row()
        .w(1000.0)
        .h(600.0)
        .children([side, main, layer]);
    root.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut chain = Vec::new();
    assert!(
        collect_scroll_rest_chain_at(&root, 80.0, 300.0, [0.0, 0.0], &mut chain),
        "pointer over sidebar must hit a scroll branch"
    );
    assert_eq!(
        chain.first().map(String::as_str),
        Some("side-nav"),
        "empty portal layer must not swallow sidebar wheel target; got {chain:?}"
    );

    chain.clear();
    assert!(collect_scroll_rest_chain_at(
        &root,
        500.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert_eq!(
        chain.first().map(String::as_str),
        Some("root-scroll"),
        "empty portal layer must not swallow main wheel target; got {chain:?}"
    );
}

#[test]
fn hoist_then_layout_keeps_scroll_chain_under_empty_portals() {
    let mut root = Element::new().flex_row().w(1000.0).h(600.0).children([
        Element::new()
            .w(200.0)
            .h("100%")
            .overflow_y("auto")
            .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"))
            .template_id("side")
            .child(Element::new().w(200.0).h(2000.0)),
        Element::new()
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .overflow_y("auto")
            .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}root-scroll"))
            .template_id("main")
            .child({
                let mut portal = Element::new()
                    .absolute()
                    .inset(0.0)
                    .w("100%")
                    .h("100%")
                    .template_id("page-portal");
                portal.set_attr("rui_portal", "1");
                portal
            }),
    ]);

    assert_eq!(hoist_portal_subtrees(&mut root), 1);
    let mut layout = LayoutTree::new();
    let _ = layout.build_and_layout(&mut root, 1000.0, 600.0, [-1.0, -1.0]);

    let mut chain = Vec::new();
    assert!(collect_scroll_rest_chain_at(
        &root,
        80.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert_eq!(
        chain.first().map(String::as_str),
        Some("side-nav"),
        "after hoist, empty portal must not block sidebar; got {chain:?}"
    );
}
