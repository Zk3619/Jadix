//! WP-3.11：公开 `<svg>` 叶 — 布局尺寸 + path 核光栅（Builder）。

use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::path::{ATTR_PATH_D, ATTR_PATH_SCENE};
use crate::primitives::svg::{circle, path, svg};

#[test]
fn svg_builder_leaf_layout_bounds_and_path_texture() {
    let svg_el: Element = svg()
        .view_box("0 0 100 100")
        .w(160.0)
        .h(120.0)
        .child(
            circle()
                .cx(50.0)
                .cy(50.0)
                .r(36.0)
                .fill("primary")
                .stroke("foreground")
                .stroke_width(3.0),
        )
        .child(
            path()
                .d("M12 78 L50 88 L88 78")
                .fill("none")
                .stroke("destructive")
                .stroke_width(4.0),
        )
        .into();
    assert!(
        svg_el.children.is_empty(),
        "svg children must not occupy taffy nodes"
    );
    assert!(
        svg_el.get_attr(ATTR_PATH_SCENE).is_some(),
        "must register shared path scene, not an attr stub"
    );
    let d = svg_el.get_attr(ATTR_PATH_D).expect("shared path d");
    assert!(
        d.contains('M') && (d.contains('L') || d.contains('A')),
        "same PathCommand d: {d}"
    );

    let mut root = Element::new().w(200.0).h(180.0).child(svg_el);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 180.0, [-1.0, -1.0]);
    let leaf = root.children.first().expect("svg leaf");
    let [x, y, w, h] = leaf.layout_bounds;
    assert!(
        w > 150.0 && w < 170.0 && h > 110.0 && h < 130.0,
        "leaf layout_bounds should follow w/h, got [{x}, {y}, {w}, {h}]"
    );
    assert!(
        result.textured_draws.iter().any(|d| d.rect[2] > 80.0
            && d.rect[3] > 80.0
            && d.texture_key.starts_with("jadix_path:")),
        "svg must raster via the path kernel; draws={:?}",
        result.textured_draws
    );
}
