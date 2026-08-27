//! `pointer-events`：CSS 通用 + RUI `box-none` + MDN SVG-only 关键字。

use crate::app::scroll_support::collect_scroll_rest_chain_at;
use crate::app::tree_hit::find_click;
use crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX;
use crate::core::tree::{
    pointer_events_targets_children, pointer_events_targets_self, Element, PointerEvents,
};
use crate::style::apply_class_list;

#[test]
fn pointer_events_parse_modes() {
    // CSS 通用
    assert_eq!(PointerEvents::parse("none"), PointerEvents::None);
    assert_eq!(PointerEvents::parse("auto"), PointerEvents::Auto);
    assert!(!PointerEvents::Auto.is_svg_only());
    assert!(!PointerEvents::None.is_svg_only());

    // RUI / RN
    assert_eq!(PointerEvents::parse("box-none"), PointerEvents::BoxNone);
    assert_eq!(PointerEvents::parse("box_none"), PointerEvents::BoxNone);
    assert!(!PointerEvents::BoxNone.is_svg_only());

    // MDN SVG-only — 保留独立变体，不折叠成 Auto
    assert_eq!(
        PointerEvents::parse("bounding-box"),
        PointerEvents::BoundingBox
    );
    assert_eq!(
        PointerEvents::parse("visiblePainted"),
        PointerEvents::VisiblePainted
    );
    assert_eq!(
        PointerEvents::parse("visibleFill"),
        PointerEvents::VisibleFill
    );
    assert_eq!(
        PointerEvents::parse("visibleStroke"),
        PointerEvents::VisibleStroke
    );
    assert_eq!(PointerEvents::parse("visible"), PointerEvents::Visible);
    assert_eq!(PointerEvents::parse("painted"), PointerEvents::Painted);
    assert_eq!(PointerEvents::parse("fill"), PointerEvents::Fill);
    assert_eq!(PointerEvents::parse("stroke"), PointerEvents::Stroke);
    assert_eq!(PointerEvents::parse("all"), PointerEvents::All);
    assert!(PointerEvents::BoundingBox.is_svg_only());
    assert!(PointerEvents::Fill.is_svg_only());
    assert!(PointerEvents::BoundingBox.targets_self());
    assert!(!PointerEvents::None.targets_self());
    assert!(!PointerEvents::BoxNone.targets_self());
    assert!(PointerEvents::BoxNone.targets_children());

    assert_eq!(
        Element::new().pointer_events_none().pointer_events_mode(),
        PointerEvents::None
    );
    assert_eq!(
        Element::new()
            .pointer_events_box_none()
            .pointer_events_mode(),
        PointerEvents::BoxNone
    );
    assert_eq!(
        Element::new()
            .pointer_events("bounding-box")
            .pointer_events_mode(),
        PointerEvents::BoundingBox
    );
    assert!(pointer_events_targets_self(
        &Element::new().pointer_events("bounding-box")
    ));
    assert!(pointer_events_targets_self(
        &Element::new().pointer_events("fill")
    ));
}

#[test]
fn utility_classes_set_pointer_events() {
    let mut el = Element::new();
    let _ = apply_class_list(&mut el, "pointer-events-none");
    assert_eq!(el.pointer_events_mode(), PointerEvents::None);

    let mut el = Element::new();
    let _ = apply_class_list(&mut el, "pointer-events-box-none");
    assert_eq!(el.pointer_events_mode(), PointerEvents::BoxNone);

    let mut el = Element::new().pointer_events_none();
    let _ = apply_class_list(&mut el, "pointer-events-auto");
    assert_eq!(el.pointer_events_mode(), PointerEvents::Auto);
}

#[test]
fn none_blocks_self_and_children_clicks() {
    let mut child = Element::new().w(50.0).h(50.0).on_click(|| {});
    child.layout_bounds = [10.0, 10.0, 50.0, 50.0];
    let mut root = Element::new()
        .pointer_events_none()
        .w(100.0)
        .h(100.0)
        .child(child);
    root.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    assert!(!pointer_events_targets_children(&root));
    assert!(!pointer_events_targets_self(&root));
    assert!(find_click(&root, 20.0, 20.0, [0.0, 0.0]).is_none());
}

#[test]
fn box_none_passes_self_but_keeps_child_clicks() {
    let mut child = Element::new().w(50.0).h(50.0).on_click(|| {});
    child.layout_bounds = [10.0, 10.0, 50.0, 50.0];
    let mut shell = Element::new()
        .pointer_events_box_none()
        .w(100.0)
        .h(100.0)
        .on_click(|| {}) // must not win over passthrough
        .child(child);
    shell.layout_bounds = [0.0, 0.0, 100.0, 100.0];

    assert!(pointer_events_targets_children(&shell));
    assert!(!pointer_events_targets_self(&shell));
    assert!(
        find_click(&shell, 20.0, 20.0, [0.0, 0.0]).is_some(),
        "child under box-none must remain hittable"
    );
    assert!(
        find_click(&shell, 90.0, 90.0, [0.0, 0.0]).is_none(),
        "box-none shell on_click must not capture empty area"
    );
}

#[test]
fn box_none_shell_does_not_block_scroll_underneath() {
    let mut side = Element::new()
        .w(200.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"));
    side.layout_bounds = [0.0, 0.0, 200.0, 600.0];

    let mut overlay = Element::new()
        .absolute()
        .pointer_events_box_none()
        .w(1000.0)
        .h(600.0);
    overlay.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut root = Element::new().w(1000.0).h(600.0).children([side, overlay]);
    root.layout_bounds = [0.0, 0.0, 1000.0, 600.0];

    let mut chain = Vec::new();
    assert!(collect_scroll_rest_chain_at(
        &root,
        80.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert_eq!(chain.first().map(String::as_str), Some("side-nav"));
}

#[test]
fn none_subtree_hides_scroll_targets() {
    let mut side = Element::new()
        .w(200.0)
        .h(600.0)
        .overflow_y("auto")
        .focus_id(format!("{SCROLL_AREA_FOCUS_PREFIX}side-nav"));
    side.layout_bounds = [0.0, 0.0, 200.0, 600.0];
    let mut buried = Element::new()
        .pointer_events_none()
        .w(200.0)
        .h(600.0)
        .child(side);
    buried.layout_bounds = [0.0, 0.0, 200.0, 600.0];

    let mut chain = Vec::new();
    assert!(!collect_scroll_rest_chain_at(
        &buried,
        80.0,
        300.0,
        [0.0, 0.0],
        &mut chain
    ));
    assert!(chain.is_empty());
}
