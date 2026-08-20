//! Off-screen descendants of an overflow clip must not be collected as instances,
//! while Taffy content height (scroll range) stays the full column.

use crate::core::tree::{Element, LayoutTree};

fn tall_scroll_content_column() -> Element {
    let mut col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(80.0)
        .h(2000.0)
        .min_h(2000.0);
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
    let mut root = tall_scroll_content_column();
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
fn scroll_only_sibling_band_cull_skips_far_offscreen_rows() {
    let mut root = tall_scroll_content_column();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    root.scroll_offset = [0.0, 1500.0];
    let result = tree.build_and_layout_fast(
        &mut root,
        80.0,
        100.0,
        [-1.0, -1.0],
        true,
        false,
    );
    assert!(
        result.instances.len() <= 4,
        "scroll_only band cull must skip rows far outside the clip (got {} instances)",
        result.instances.len()
    );
    assert!(
        !result.instances.is_empty(),
        "rows in the visible band must still paint"
    );
}

#[test]
fn scroll_only_band_cull_keeps_stable_content_extent_with_overflowing_children() {
    // Cards with overflow:visible children taller than the card's Taffy box. Band cull must
    // not drop that overflow from content height when the card leaves the visit band —
    // otherwise ScrollArea content_h jitters and the thumb oscillates.
    let mut col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(80.0)
        .h(2000.0)
        .min_h(2000.0);
    for _ in 0..12 {
        col = col.child(
            Element::new()
                .w(80.0)
                .h(100.0)
                .flex_shrink(0.0)
                .overflow_y("visible")
                .child(
                    Element::new()
                        .w(80.0)
                        .h(140.0)
                        .flex_shrink(0.0)
                        .bg([0.0, 1.0, 0.0, 1.0]),
                ),
        );
    }
    let mut root = Element::new()
        .w(80.0)
        .h(100.0)
        .overflow_y("hidden")
        .child(col);

    let mut tree = LayoutTree::new();
    let baseline = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    let baseline_h = baseline
        .instances
        .iter()
        .map(|i| i.rect_pos[1] + i.rect_size[1])
        .fold(0.0_f32, f32::max);
    // Prefer subtree extent from the content column.
    let content_extent_h = root.children[0].collect_extent[1];
    assert!(
        root.children[0].collect_extent_valid,
        "full collect must stamp subtree extent"
    );

    for sy in [0.0_f32, 200.0, 600.0, 1100.0, 1500.0] {
        root.scroll_offset = [0.0, sy];
        let _ = tree.build_and_layout_fast(
            &mut root,
            80.0,
            100.0,
            [-1.0, -1.0],
            true,
            false,
        );
        let h = root.children[0].collect_extent[1];
        assert!(
            (h - content_extent_h).abs() < 0.5,
            "scroll_only content extent must stay stable (sy={sy}, was {content_extent_h}, now {h}); \
             baseline_paint_bottom={baseline_h}"
        );
    }
}

#[test]
fn scroll_only_band_cull_keeps_full_column_extent() {
    let mut root = tall_scroll_content_column();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    root.scroll_offset = [0.0, 1500.0];
    let _ = tree.build_and_layout_fast(
        &mut root,
        80.0,
        100.0,
        [-1.0, -1.0],
        true,
        false,
    );
    let last = &root.children[0].children[19];
    let bottom = last.layout_bounds[1] + last.layout_bounds[3];
    assert!(
        bottom >= 1900.0,
        "band cull must still assign Taffy bounds for off-screen rows (bottom={bottom})"
    );
}

#[test]
fn overflow_clip_cull_keeps_full_column_extent() {
    let mut root = tall_scroll_content_column();
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    let last = &root.children[0].children[19];
    let bottom = last.layout_bounds[1] + last.layout_bounds[3];
    assert!(
        bottom >= 1900.0,
        "cull must still assign Taffy bounds for off-screen rows (bottom={bottom})"
    );
}
