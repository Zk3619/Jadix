//! Off-screen descendants of an overflow clip must not be collected as instances,
//! while Taffy content height (scroll range) stays the full column.

use crate::core::tree::{Element, LayoutTree};

fn tall_clipped_column() -> Element {
    let mut col = Element::new().flex_col().flex_shrink(0.0);
    for _ in 0..20 {
        col = col.child(
            Element::new()
                .w(80.0)
                .h(100.0)
                .flex_shrink(0.0)
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    }
    Element::new()
        .w(80.0)
        .h(100.0)
        .overflow_y("hidden")
        .child(col)
}

#[test]
fn overflow_clip_skips_offscreen_child_instances() {
    let mut root = tall_clipped_column();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    // Visible strip is 100px; each child is 100px. Allow the on-screen row plus a pad row.
    assert!(
        result.instances.len() <= 4,
        "off-screen rows must be clip-culled (got {} instances)",
        result.instances.len()
    );
    assert!(
        !result.instances.is_empty(),
        "the on-screen row must still paint"
    );
}

#[test]
fn overflow_clip_cull_keeps_full_column_extent() {
    let mut root = tall_clipped_column();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    let last = &root.children[0].children[19];
    let bottom = last.layout_bounds[1] + last.layout_bounds[3];
    assert!(
        bottom >= 1900.0,
        "cull must still assign Taffy bounds for off-screen rows (bottom={bottom})"
    );
}
