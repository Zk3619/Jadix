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
    let result = tree.build_and_layout_fast(&mut root, 80.0, 100.0, [-1.0, -1.0], true, false);
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
        let _ = tree.build_and_layout_fast(&mut root, 80.0, 100.0, [-1.0, -1.0], true, false);
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
    let _ = tree.build_and_layout_fast(&mut root, 80.0, 100.0, [-1.0, -1.0], true, false);
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

#[test]
fn full_collect_after_reflow_shrink_drops_stale_clip_cull_extent() {
    // Gallery text page: scroll to bottom (clip-cull stamps tall collect_extent on upper
    // cards), then widen so wrapped labels reflow shorter. Full collect must not keep
    // max(stale_extent, own) or max_scroll leaves blank space below the last card.
    let mut col = Element::new().flex_col().flex_shrink(0.0).w("100%");
    for i in 0..8 {
        // Wrap-sensitive block: narrow → tall, wide → short.
        col = col.child(
            Element::new()
                .w("100%")
                .flex_shrink(0.0)
                .bg([0.9, 0.9, 0.9, 1.0])
                .child(
                    Element::new()
                        .w("100%")
                        .text(format!(
                            "row {i:02} hello world from rui text measure alignment soft wrap block"
                        ))
                        .text_size(14.0),
                ),
        );
    }
    let mut root = Element::new()
        .w(120.0)
        .h(160.0)
        .overflow_y("hidden")
        .focus_id("scroll:diag-reflow-blank")
        .child(col);

    let mut tree = LayoutTree::new();
    let narrow = tree.build_and_layout(&mut root, 120.0, 160.0, [-1.0, -1.0]);
    let (_id, _vp, max_n, content_n) = narrow
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "diag-reflow-blank")
        .expect("narrow scroll metrics");
    assert!(
        max_n[1] > 1.0,
        "narrow layout must scroll (max_y={})",
        max_n[1]
    );

    // Simulate scrolled deep: stamp large collect_extent on upper siblings via scroll_only.
    root.scroll_offset = [0.0, max_n[1]];
    let _ = tree.build_and_layout_fast(&mut root, 120.0, 160.0, [-1.0, -1.0], true, false);

    // Widen: same tree, full collect (resize path). Content must shrink; max must follow.
    root.style.size.width = taffy::style::Dimension::length(360.0);
    let wide = tree.build_and_layout(&mut root, 360.0, 160.0, [-1.0, -1.0]);
    let (_id, _vp, max_w, content_w) = wide
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "diag-reflow-blank")
        .expect("wide scroll metrics");

    assert!(
        content_w[1] + 1.0 < content_n[1],
        "wider reflow must shorten content (narrow_h={} wide_h={})",
        content_n[1],
        content_w[1]
    );
    assert!(
        max_w[1] + 1.0 < max_n[1],
        "max_scroll must shrink with content (narrow_max={} wide_max={}); \
         stale clip-cull extent left blank range after resize",
        max_n[1],
        max_w[1]
    );
}

/// TreeView collapsible/accordion used `max_h` + `overflow:auto` without a `scroll:` id.
/// Inner height must not leak into the outer ScrollArea `content_h`, or the thumb jumps
/// when `scroll_only` (stale inner extent) and full clip-cull (layout box) disagree.
fn nested_overflow_scroll_in_outer_viewport() -> Element {
    let nested = Element::new()
        .w("100%")
        .h(80.0)
        .max_h(80.0)
        .overflow_y("scroll")
        .flex_shrink(0.0)
        .child(
            Element::new()
                .w("100%")
                .h(400.0)
                .flex_shrink(0.0)
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    let sibling = Element::new()
        .w("100%")
        .h(50.0)
        .flex_shrink(0.0)
        .bg([0.0, 1.0, 0.0, 1.0]);
    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .child(nested)
        .child(sibling);
    Element::new()
        .w(120.0)
        .h(100.0)
        .overflow_y("hidden")
        .focus_id("scroll:diag-nested-ovf")
        .child(col)
}

fn outer_content_h(result: &crate::core::tree::RenderResult) -> f32 {
    result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "diag-nested-ovf")
        .map(|m| m.3[1])
        .expect("outer scroll metrics")
}

#[test]
fn overflow_scroll_box_does_not_inflate_parent_content_h() {
    let mut root = nested_overflow_scroll_in_outer_viewport();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 120.0, 100.0, [-1.0, -1.0]);
    let content_h = outer_content_h(&first);

    assert!(
        content_h < 80.0 + 50.0 + 16.0,
        "nested overflow:scroll must not leak inner 400px into parent content_h (got {content_h})"
    );
    assert!(
        content_h > 80.0 + 50.0 - 16.0,
        "parent content_h should still include the nested viewport box + sibling (got {content_h})"
    );
}

#[test]
fn overflow_scroll_box_content_h_stable_across_scroll_only_and_full_collect() {
    let mut root = nested_overflow_scroll_in_outer_viewport();
    let mut tree = LayoutTree::new();
    let baseline = outer_content_h(&tree.build_and_layout(&mut root, 120.0, 100.0, [-1.0, -1.0]));

    for sy in [0.0_f32, 20.0, 80.0, 160.0] {
        root.scroll_offset = [0.0, sy];
        let h = outer_content_h(&tree.build_and_layout_fast(
            &mut root,
            120.0,
            100.0,
            [-1.0, -1.0],
            true,
            false,
        ));
        assert!(
            (h - baseline).abs() < 1.0,
            "scroll_only content_h must stay stable (sy={sy} baseline={baseline} now={h})"
        );
    }

    // Full collect after the nested box leaves the visit band must not snap to sibling-only.
    root.scroll_offset = [0.0, 160.0];
    let h = outer_content_h(&tree.build_and_layout(&mut root, 120.0, 100.0, [-1.0, -1.0]));
    assert!(
        (h - baseline).abs() < 1.0,
        "full collect after clip-cull must keep content_h (baseline={baseline} now={h})"
    );
}
