//! WP-3.11 A：`ui!` 与 Builder 各建一叶；viewBox + circle/path；layout_bounds。

use rui::core::tree::LayoutTree;
use rui::prelude::*;
use rui::primitives::path::{ATTR_PATH_D, ATTR_PATH_SCENE};

#[test]
fn svg_builder_builds_a_leaf() {
    let el: Element = svg()
        .view_box("0 0 100 100")
        .w(160.0)
        .h(160.0)
        .child(
            circle()
                .cx(50.0)
                .cy(50.0)
                .r(40.0)
                .fill("primary")
                .stroke("foreground")
                .stroke_width(2.0),
        )
        .child(
            svg_path()
                .d("M10 80 L90 80")
                .fill("none")
                .stroke("destructive")
                .stroke_width(3.0),
        )
        .into();
    assert!(el.children.is_empty());
    assert!(el.get_attr(ATTR_PATH_SCENE).is_some());
    assert!(el.get_attr(ATTR_PATH_D).is_some());
}

#[test]
fn svg_ui_tag_builds_a_leaf() {
    let el: Element = ui! {
        <svg viewBox="0 0 100 100" w="10rem" h="10rem">
            <circle cx="50" cy="50" r="40" fill="primary" stroke="foreground" stroke_width="2" />
            <path d="M10 80 L90 80" fill="none" stroke="destructive" stroke_width="3" />
        </svg>
    };
    assert!(
        el.children.is_empty(),
        "ui! <svg> shapes must flatten; children={}",
        el.children.len()
    );
    assert!(
        el.get_attr(ATTR_PATH_SCENE).is_some(),
        "ui! svg must compile to the shared path scene"
    );
    let d = el.get_attr(ATTR_PATH_D).expect("d");
    assert!(d.contains('M') && d.contains('L'), "{d}");
}

#[test]
fn svg_ui_leaf_layout_bounds() {
    let svg_el: Element = ui! {
        <svg viewBox="0 0 100 100" w={160.0} h={120.0}>
            <circle cx="50" cy="50" r="36" fill="primary" />
            <path d="M12 78 L88 78" fill="none" stroke="foreground" stroke_width="3" />
        </svg>
    };
    let mut root = Element::new().w(200.0).h(180.0).child(svg_el);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 180.0, [-1.0, -1.0]);
    let leaf = root.children.first().expect("svg leaf");
    let [_, _, w, h] = leaf.layout_bounds;
    assert!(
        (w - 160.0).abs() < 1.0 && (h - 120.0).abs() < 1.0,
        "layout_bounds={:?}",
        leaf.layout_bounds
    );
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.texture_key.starts_with("rui_path:")),
        "draws={:?}",
        result.textured_draws
    );
}
