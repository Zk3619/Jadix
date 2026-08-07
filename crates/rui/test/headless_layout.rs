//! Headless layout: build a simple Element tree and assert non-zero bounds (no window).

use crate::core::tree::{Element, LayoutTree};

#[test]
fn headless_build_and_layout_nonzero_bounds() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .flex_col()
        .w(320.0)
        .h(240.0)
        .child(Element::new().w(100.0).h(40.0).bg([1.0, 0.0, 0.0, 1.0]))
        .child(Element::new().w(200.0).h(60.0).text("hello"));

    tree.build_and_layout(&mut root, 320.0, 240.0, [0.0, 0.0]);

    assert!(
        root.layout_bounds[2] > 0.0 && root.layout_bounds[3] > 0.0,
        "root bounds should be positive, got {:?}",
        root.layout_bounds
    );
    let leaf = &root.children[0];
    assert!(
        leaf.layout_bounds[2] > 0.0 && leaf.layout_bounds[3] > 0.0,
        "leaf bounds should be positive, got {:?}",
        leaf.layout_bounds
    );
    assert!(
        (leaf.layout_bounds[2] - 100.0).abs() < 0.5,
        "expected width ~100, got {}",
        leaf.layout_bounds[2]
    );
}
