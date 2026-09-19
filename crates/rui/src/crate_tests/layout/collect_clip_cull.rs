//! Off-screen descendants of an overflow clip must not be collected as instances,
//! while Taffy content height (scroll range) stays the full column.

use crate::app::ime_support::{painted_ime_field_in_overflow_clip, painted_text_edit_node};
use crate::components::input::{focus_id_for_input, input, input_value_signal_key};
use crate::components::internal::text_edit::{
    sync_text_edit_paint_subtree, sync_text_edit_paint_subtree_in_viewport,
};
use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::scroll_offset_y_signal_key;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::primitives::r#box::hstack;
use crate::renderer::font_loader;
use crate::style::{sync_class_variants_subtree, tick_active_css_transitions_subtree};
use crate::text::install_shared_font_system;
use crate::utils::text_field::input_text_template_id;
use taffy::style::FlexWrap;

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

/// End-anchored `scroll_only` must keep the live edge of an intrinsic-height
/// column. The column is `h(max-content)` with no product `data-slot` — the
/// collect skip is the CSS dim, not `message-scroller-content`.
#[test]
fn scroll_only_intrinsic_column_keeps_end_edge_visible() {
    let mut col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(80.0)
        .h("max-content");
    assert!(
        col.attrs.get("data-slot").is_none(),
        "synthetic column must not carry a product data-slot"
    );
    for _ in 0..16 {
        col = col.child(
            Element::new()
                .w(80.0)
                .h(48.0)
                .flex_shrink(0.0)
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    }
    let mut root = Element::new()
        .w(80.0)
        .h(100.0)
        .overflow_y("hidden")
        .child(col);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 80.0, 100.0, [-1.0, -1.0]);
    root.scroll_offset = [0.0, 16.0 * 48.0 - 100.0];
    let result = tree.build_and_layout_fast(&mut root, 80.0, 100.0, [-1.0, -1.0], true, false);
    let visible = result
        .instances
        .iter()
        .filter(|i| i.rect_pos[1] + i.rect_size[1] > -1.0 && i.rect_pos[1] < 101.0)
        .count();
    assert!(
        visible > 0,
        "max-content column scroll_only must keep the end-edge band (instances={}, visible={visible})",
        result.instances.len(),
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

/// Chart-like cards: overflow:visible + absolutely positioned labels that stick out of
/// the Taffy box. Paint-collect extent must not grow/shrink as cards enter/leave the
/// visit band — that is the gallery data-page thumb jitter.
fn chart_like_abspos_overflow_column() -> Element {
    let mut col = Element::new().flex_col().flex_shrink(0.0).w(200.0).gap(8.0);
    for _ in 0..10 {
        col = col.child(
            Element::new()
                .w(200.0)
                .h(100.0)
                .flex_shrink(0.0)
                .overflow_y("visible")
                .overflow_x("visible")
                .relative()
                .bg([0.2, 0.2, 0.2, 1.0])
                .child(
                    Element::new()
                        .absolute()
                        .left(-12.0)
                        .top(92.0)
                        .w(48.0)
                        .h(24.0)
                        .bg([0.0, 1.0, 0.0, 1.0]),
                ),
        );
    }
    Element::new()
        .w(200.0)
        .h(140.0)
        .overflow_y("hidden")
        .overflow_x("hidden")
        .focus_id("scroll:diag-abspos-ovf")
        .child(col)
}

fn abspos_metrics(result: &crate::core::tree::RenderResult) -> (f32, f32) {
    result
        .scroll_metrics
        .iter()
        .find(|(id, ..)| id == "diag-abspos-ovf")
        .map(|m| (m.3[0], m.3[1]))
        .expect("abspos overflow scroll metrics")
}

#[test]
fn abspos_overflow_visible_content_extent_stable_across_scroll() {
    let mut root = chart_like_abspos_overflow_column();
    let mut tree = LayoutTree::new();
    let first = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
    let (base_w, base_h) = abspos_metrics(&first);
    assert!(
        base_h > 140.0,
        "column of 10×100px cards must overflow the 140px viewport (content_h={base_h})"
    );

    for sy in [0.0_f32, 80.0, 200.0, 400.0, 700.0, 900.0] {
        root.scroll_offset = [0.0, sy];
        let (w, h) = abspos_metrics(&tree.build_and_layout_fast(
            &mut root,
            200.0,
            140.0,
            [-1.0, -1.0],
            true,
            false,
        ));
        assert!(
            (h - base_h).abs() < 1.0 && (w - base_w).abs() < 1.0,
            "scroll_only content extent must stay stable (sy={sy} base=({base_w},{base_h}) now=({w},{h}))"
        );
    }

    root.scroll_offset = [0.0, 700.0];
    let (w, h) = abspos_metrics(&tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]));
    assert!(
        (h - base_h).abs() < 1.0 && (w - base_w).abs() < 1.0,
        "full collect after clip-cull must keep content extent (base=({base_w},{base_h}) now=({w},{h}))"
    );
}

#[test]
fn accordion_max_height_paints_the_clip_not_content_height() {
    let mut panel = Element::new()
        .w(80.0)
        .max_h(40.0)
        .overflow_y("hidden")
        .bg([1.0, 1.0, 1.0, 1.0])
        .outline([1.0, 1.0, 1.0, 1.0], 1.0)
        .child(
            Element::new()
                .w(80.0)
                .h(200.0)
                .flex_shrink(0.0)
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    panel.set_attr("rui_hoisted_portal", "1");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut panel, 80.0, 400.0, [-1.0, -1.0]);
    assert!(
        panel.layout_bounds[3] <= 41.0,
        "Taffy used height must be the max-height clip, got {}",
        panel.layout_bounds[3]
    );
    let max_bottom = result
        .instances
        .iter()
        .map(|i| {
            let b = crate::app::draw_cull::instance_paint_bounds(i);
            b[1] + b[3]
        })
        .fold(0.0_f32, f32::max);
    assert!(
        max_bottom <= panel.layout_bounds[1] + 40.0 + 16.0,
        "outline/shadow must not stamp a full-content frame around the clip (bottom={max_bottom})"
    );
}

/// ColorPicker format Select is 6 rows that fit in `max_height: 16rem`. Pinning the
/// ScrollArea wrapper made viewport==content, so `overflow:auto` never clipped and
/// labels survived accordion-up. DatePicker months overflow 16rem so the same pin
/// still clipped. Shrink the scrollport, pin the rows.
#[test]
fn accordion_scroll_viewport_clips_short_list_when_wrapper_shrinks() {
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;

    let mut rows = Element::new().flex_col().flex_shrink(0.0).min_h(192.0);
    for _ in 0..6 {
        rows = rows.child(
            Element::new()
                .w(80.0)
                .h(32.0)
                .flex_shrink(0.0)
                .bg([1.0, 0.0, 0.0, 1.0]),
        );
    }
    let viewport = Element::new()
        .w("100%")
        .h("100%")
        .min_h(0.0)
        .flex_shrink(1.0)
        .overflow_y("auto")
        .focus_id(focus_id_for_scroll_viewport("fmt:menu_scroll"))
        .child(rows);
    let mut panel = Element::new()
        .w(80.0)
        .max_h(40.0)
        .overflow_y("hidden")
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(viewport);
    panel.set_attr("rui_hoisted_portal", "1");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut panel, 80.0, 400.0, [-1.0, -1.0]);
    let max_bottom = result
        .instances
        .iter()
        .map(|i| {
            let b = crate::app::draw_cull::instance_paint_bounds(i);
            b[1] + b[3]
        })
        .fold(0.0_f32, f32::max);
    assert!(
        max_bottom <= panel.layout_bounds[1] + 40.0 + 16.0,
        "short-list rows must clip to the accordion (bottom={max_bottom} clip={})",
        panel.layout_bounds[1] + 40.0
    );
}

fn select_like_format_menu(max_h: f32, pin_rows: bool) -> Element {
    use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
    use crate::utils::text_node::apply_label_defaults;

    let labels = ["HEX", "HEX8", "RGB", "RGBA", "HSL", "HSLA"];
    let mut rows = Element::new().flex_col().w(80.0);
    if pin_rows {
        rows = rows.flex_shrink(0.0).min_h(192.0);
    }
    for name in labels {
        let text = apply_label_defaults(
            Element::new()
                .text(name)
                .overflow("hidden")
                .whitespace_nowrap()
                .text_overflow_ellipsis(),
        )
        .w(80.0)
        .h(32.0)
        .min_h(32.0)
        .flex_shrink(0.0);
        rows = rows.child(text);
    }
    let viewport = Element::new()
        .w("100%")
        .h("100%")
        .min_h(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .overflow_y("auto")
        .focus_id(focus_id_for_scroll_viewport("fmt:menu_scroll"))
        .child(rows);
    let mut panel = Element::new()
        .w(80.0)
        .max_h(max_h)
        .overflow_y("hidden")
        .p(4.0)
        .bg([1.0, 1.0, 1.0, 1.0])
        .child(viewport);
    panel.set_attr("rui_hoisted_portal", "1");
    panel
}

/// Expand enter must grow from 0, not from one Select-item min-height (~32px).
#[test]
fn accordion_zero_max_height_does_not_floor_to_item_row() {
    let mut panel = select_like_format_menu(0.0, true);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut panel, 80.0, 400.0, [-1.0, -1.0]);
    assert!(
        panel.layout_bounds[3] <= 8.5,
        "max-height:0 may keep padding (8px) but must not floor to a format-item row, got {}",
        panel.layout_bounds[3]
    );
    let mut unpinned = select_like_format_menu(0.0, false);
    let _ = tree.build_and_layout(&mut unpinned, 80.0, 400.0, [-1.0, -1.0]);
    assert!(
        unpinned.layout_bounds[3] <= 8.5,
        "unpinned short list at max-height:0 must also not floor to an item row, got {}",
        unpinned.layout_bounds[3]
    );
}

/// ColorPicker format labels use ellipsis; occupancy follows max-height but glyphon
/// used the Taffy content box, so accordion-up left HEX/RGB ink after chrome shrank.
#[test]
fn accordion_max_height_clips_select_ellipsis_labels() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let mut panel = select_like_format_menu(40.0, true);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut panel, 80.0, 400.0, [-1.0, -1.0]);
    let clip_bottom = panel.layout_bounds[1]
        + crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(40.0);
    let dump = format!(
        "bounds={:?} max_h={:?} texts={}",
        panel.layout_bounds,
        crate::layout::dim_length_px(panel.style.max_size.height),
        result
            .text_nodes
            .iter()
            .map(|t| format!(
                "{} y={:.1} h={:.1} clip={:?}",
                t.text, t.y, t.height, t.clip_bounds
            ))
            .collect::<Vec<_>>()
            .join("; ")
    );
    for t in &result.text_nodes {
        let clip_b = t.clip_bounds.map(|c| c[1] + c[3]).unwrap_or(t.y + t.height);
        assert!(
            clip_b <= clip_bottom + 1.0,
            "format label {:?} must clip to accordion, clip_bottom={clip_b} cap={clip_bottom}\n{dump}",
            t.text
        );
    }
    assert!(
        result.text_nodes.iter().any(|t| t.text == "HEX"),
        "must still paint the visible first row\n{dump}"
    );
}

/// Gallery forms: focus an Input, scroll until the field is not fully visible.
/// Clip-cull used to drop the text leaf (`input_caret` gone from `text_nodes`) while
/// the chrome still peeks. `caret_wake_at` died → bare `Wait` → clicks/keys queued
/// until the next scroll/blank-click paint.
#[test]
fn clip_cull_must_keep_focused_input_caret_when_field_is_partially_clipped() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "clip-cull-live-caret";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());

    let field: Element = input(id).placeholder("Placeholder…").into();
    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(240.0)
        .child(field)
        .child(
            Element::new()
                .w(240.0)
                .h(400.0)
                .flex_shrink(0.0)
                .bg([0.9, 0.9, 0.9, 1.0]),
        );
    let mut root = Element::new()
        .w(240.0)
        .h(80.0)
        .overflow_y("hidden")
        .child(col);

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let full = tree.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);
    assert!(
        full.text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "focused Input must emit caret before scroll"
    );

    // Default Input is 2.25rem (36px) with `py` 0.25rem. Wrapper still peeks for
    // scroll in (text-leaf-bottom, 36): chrome vis > 0, text leaf vis == 0, so
    // clip-cull used to drop `input_caret` while the field still looked focused.
    root.scroll_offset = [0.0, 34.0];
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let clipped = tree.build_and_layout_fast(&mut root, 240.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        clipped
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "partially clipped focused Input must stay in collect so caret WaitUntil does not die"
    );

    // Wheel stop: collect is no longer `scroll_only` and skip is allowed. Dropping
    // the caret here is the freeze (`caret_wake_at` dies → bare Wait).
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [-1.0, -1.0],
        "",
        false,
        240.0,
        80.0,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    tree.set_collect_skip_allowed(true);
    let settled = tree.build_and_layout_fast(&mut root, 240.0, 80.0, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);
    assert!(
        settled
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "settle collect (skip on, not scroll_only) must keep the focused caret"
    );
}

fn wipe_input_carets(el: &mut Element) {
    el.text_style.input_caret = None;
    for c in &mut el.children {
        wipe_input_carets(c);
    }
}

/// `scroll_paint_only` skipped `sync_text_edit` after class-variant hover-leave restored
/// `base_text_style` (no caret). Collect then had nothing to emit; `caret_wake_at` died.
#[test]
fn class_snapshot_restore_without_text_edit_sync_drops_caret_on_partial_clip() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    let id = "clip-cull-wiped-caret";
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(id));
    use_signal(&input_value_signal_key(id), String::new).set("hello".into());

    let field: Element = input(id).placeholder("Placeholder…").into();
    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w(240.0)
        .child(field)
        .child(
            Element::new()
                .w(240.0)
                .h(400.0)
                .flex_shrink(0.0)
                .bg([0.9, 0.9, 0.9, 1.0]),
        );
    let mut root = Element::new()
        .w(240.0)
        .h(80.0)
        .overflow_y("hidden")
        .child(col);

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, 240.0, 80.0, [-1.0, -1.0]);

    wipe_input_carets(&mut root);
    root.scroll_offset = [0.0, 34.0];
    let skipped = tree.build_and_layout_fast(&mut root, 240.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        skipped
            .text_nodes
            .iter()
            .all(|n| n.style.input_caret.is_none()),
        "skipping paint-sync after snapshot restore must drop caret (the freeze)"
    );

    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let restored = tree.build_and_layout_fast(&mut root, 240.0, 80.0, [-1.0, -1.0], true, false);
    assert!(
        restored
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "paint-sync on the scroll frame must reinstall caret before collect"
    );
}

fn collect_input_bounds(el: &Element, out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = el.focus_id.as_deref() {
        if fid.starts_with("input:") {
            out.push((fid.to_string(), el.layout_bounds));
        }
    }
    for c in &el.children {
        collect_input_bounds(c, out);
    }
}

fn wrap_child_input_span(wrap: &Element) -> (usize, f32, Vec<(String, [f32; 4])>) {
    let mut kids = Vec::new();
    for c in &wrap.children {
        collect_input_bounds(c, &mut kids);
    }
    if kids.is_empty() {
        return (0, 0.0, kids);
    }
    let min_y = kids.iter().map(|(_, b)| b[1]).fold(f32::MAX, f32::min);
    let max_b = kids
        .iter()
        .map(|(_, b)| b[1] + b[3])
        .fold(0.0_f32, f32::max);
    (kids.len(), (max_b - min_y).max(0.0), kids)
}

fn collect_wrap_rows<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
    if el.style.flex_wrap != FlexWrap::NoWrap {
        out.push(el);
    }
    for c in &el.children {
        collect_wrap_rows(c, out);
    }
}

/// RSX `<Input flex_grow="1">` used to keep the builder's `w(100%)`, forcing
/// one wrap item per line (gallery forms Placeholder–Search). `Element::flex_grow`
/// now matches [`input`] builder (`w(auto)`). Re-apply `w(100%)` to lock that
/// stacked overflow-hidden wrap layout.
fn gallery_rsx_flex_grow_input(id: &str) -> Element {
    let mut b = input(id).placeholder(id).clearable(true);
    if id.ends_with("-c") {
        b = b.input_type("password").password_toggle(true);
    }
    if id.ends_with("-d") {
        b = b.input_type("number").number_steppers(true);
    }
    let el: Element = b.into();
    el.flex_grow(1.0).w("100%")
}

/// Gallery forms card: two `wrap` HStacks of RSX `flex_grow` Inputs (`w(100%)`
/// kept). After scrolling the first field half off the ScrollArea, the focused
/// sibling must keep a caret in `text_nodes` (event loop WaitUntil).
#[test]
fn wrap_flex_grow_inputs_must_keep_caret_when_first_field_is_partially_clipped() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const REST: &str = "scroll-forms";
    const VW: f32 = 280.0;
    const VH: f32 = 160.0;

    let ids = [
        "wrap-a", "wrap-b", "wrap-c", "wrap-d", "wrap-e", "wrap-f", "wrap-g", "wrap-h",
    ];
    for id in ids {
        use_signal(&input_value_signal_key(id), String::new).set(String::new());
    }
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input("wrap-c"));
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(0.0);

    fn row(ids: &[&str]) -> Element {
        let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w("100%");
        for id in ids {
            row = row.child(gallery_rsx_flex_grow_input(id));
        }
        row.into()
    }

    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .gap("0.75rem")
        .child(row(&ids[..4]))
        .child(row(&ids[4..]));
    let mut root: Element = scroll_area(REST)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .w(VW)
        .h(VH)
        .child(col)
        .into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let _ = tree.build_and_layout(&mut root, VW, VH, [-1.0, -1.0]);

    let mut bounds = Vec::new();
    collect_input_bounds(&root, &mut bounds);
    let mut wrap_els = Vec::new();
    collect_wrap_rows(&root, &mut wrap_els);
    let wraps: Vec<[f32; 4]> = wrap_els.iter().map(|e| e.layout_bounds).collect();
    assert_eq!(
        bounds.len(),
        8,
        "expected 8 inputs, wraps={wraps:?} inputs={bounds:?}"
    );
    assert_eq!(wraps.len(), 2, "expected two wrap HStacks, got {wraps:?}");
    for wrap_el in &wrap_els {
        let wrap = wrap_el.layout_bounds;
        let (n, span, kids) = wrap_child_input_span(wrap_el);
        assert!(
            n >= 4 && wrap[3] + 1.0 >= span,
            "wrap HStack height must cover its wrapped Inputs (else clip-cull/hit use a one-row box).\n\
             wrap={wrap:?} span={span} n={n} kids={kids:?}"
        );
    }

    bounds.sort_by(|a, b| {
        a.1[1]
            .partial_cmp(&b.1[1])
            .unwrap()
            .then(a.1[0].partial_cmp(&b.1[0]).unwrap())
    });
    let first = bounds[0].1;

    let scroll_y = (first[3] - 8.0).max(1.0);
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(scroll_y);
    impact = LayoutStatus::Clean;
    let fid = focus_id_for_input("wrap-c");
    crate::style::tick_active_css_transitions_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, 4.0],
        fid.as_str(),
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let clipped = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], true, false);
    tree.set_collect_skip_allowed(true);
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let idle = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);

    let mut after = Vec::new();
    collect_input_bounds(&root, &mut after);
    after.sort_by(|a, b| a.1[1].partial_cmp(&b.1[1]).unwrap());
    let focused = after
        .iter()
        .find(|(id, _)| id.contains("wrap-c"))
        .expect("password-like third field");
    let has_caret = clipped
        .text_nodes
        .iter()
        .any(|n| n.style.input_caret.is_some())
        && idle
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some());
    assert!(
        has_caret,
        "focused wrap Input must keep caret when the first sibling is partially clipped.\n\
         scroll_y={scroll_y} wraps={wraps:?}\n\
         before={bounds:?}\n\
         after={after:?}\n\
         focused={focused:?} text_nodes={}",
        clipped.text_nodes.len()
    );
}

#[test]
fn rsx_input_flex_grow_clears_default_full_width() {
    let el: Element = input("rsx-grow").into();
    assert_eq!(
        el.style.size.width,
        taffy::style::Dimension::percent(1.0),
        "default Input is w(100%)"
    );
    let grown = el.flex_grow(1.0);
    assert_eq!(
        grown.style.size.width,
        taffy::style::Dimension::auto(),
        "RSX flex_grow on Input must match builder w(auto)"
    );
    assert_eq!(
        grown.style.flex_basis,
        taffy::style::Dimension::length(0.0),
        "grow Input is flex-basis 0 so wrap ignores placeholder max-content"
    );
    assert!((grown.style.flex_grow - 1.0).abs() < 1e-5);
}

/// Gallery forms first wrap row: typing into Placeholder used to shrink max-content
/// so Invalid jumped onto the same line. Native `<input>` layout ignores value/
/// placeholder glyphs; empty and `"1"` must share the same wrap row.
#[test]
fn wrap_grow_inputs_must_not_reflow_when_placeholder_becomes_value() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const VW: f32 = 280.0;
    const A: &str = "reflow-ph";
    const B: &str = "reflow-inv";
    use_signal(&input_value_signal_key(A), String::new).set(String::new());
    use_signal(&input_value_signal_key(B), String::new).set(String::new());
    use_signal("__rui_button_focus_id", String::new).set(String::new());

    let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w(VW);
    row = row.child(
        input(A)
            .placeholder("Placeholder…")
            .clearable(true)
            .flex_grow(1.0),
    );
    row = row.child(
        input(B)
            .placeholder("Invalid")
            .clearable(true)
            .flex_grow(1.0),
    );
    let mut root: Element = row.into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, VW, 80.0, [-1.0, -1.0]);

    let mut empty = Vec::new();
    collect_input_bounds(&root, &mut empty);
    empty.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        empty.len(),
        2,
        "expected Placeholder + Invalid, got {empty:?}"
    );
    let empty_dy = (empty[0].1[1] - empty[1].1[1]).abs();
    assert!(
        empty_dy < 2.0,
        "empty Placeholder and Invalid must share a wrap row (native input ignores placeholder max-content).\n\
         empty={empty:?}"
    );

    use_signal(&input_value_signal_key(A), String::new).set("1".into());
    impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree(&mut root, &mut tree, &mut impact);
    let _ = tree.build_and_layout(&mut root, VW, 80.0, [-1.0, -1.0]);

    let mut filled = Vec::new();
    collect_input_bounds(&root, &mut filled);
    filled.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(filled.len(), 2);
    let filled_dy = (filled[0].1[1] - filled[1].1[1]).abs();
    assert!(
        filled_dy < 2.0,
        "after typing, both fields must still share a wrap row.\n filled={filled:?}"
    );
    assert!(
        (empty[0].1[1] - filled[0].1[1]).abs() < 2.0
            && (empty[1].1[1] - filled[1].1[1]).abs() < 2.0,
        "typing must not restack the wrap row.\n empty={empty:?}\n filled={filled:?}"
    );
}

fn gallery_like_wrap_input(id: &str) -> Element {
    let mut b = input(id).placeholder(id).clearable(true);
    if id.ends_with("-d") {
        b = b.input_type("number").number_steppers(true);
    }
    if id.ends_with("-h") {
        b = b.input_type("search");
    }
    let el: Element = b.into();
    el.flex_grow(1.0)
}

fn text_nodes_have_caret(nodes: &[crate::core::tree::TextLayoutInfo], rest: &str) -> bool {
    let tid = input_text_template_id(rest);
    nodes
        .iter()
        .any(|n| n.template_id.as_deref() == Some(tid.as_str()) && n.style.input_caret.is_some())
}

fn input_shadow_spread(el: &Element, rest: &str) -> Option<f32> {
    let want = focus_id_for_input(rest);
    fn walk(el: &Element, want: &str) -> Option<f32> {
        if el.focus_id.as_deref() == Some(want) {
            return Some(el.style.shadow_spread);
        }
        el.children.iter().find_map(|c| walk(c, want))
    }
    walk(el, &want)
}

/// Gallery forms card「2) Input」: focus is paint-only. After the first wrap field is a
/// chrome sliver, clicking Search must move caret + ring on the next collect — without
/// rebuilding `InputShowcase`. IME may already follow `focus_id` (candidates on Search)
/// while the last presented DL still shows the number field.
#[test]
fn wrap_flex_grow_focus_move_must_repaint_chrome_after_first_field_clip() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const REST: &str = "scroll-forms-focus-move";
    const VW: f32 = 520.0;
    const VH: f32 = 200.0;
    const NUMBER: &str = "wrap-d";
    const SEARCH: &str = "wrap-h";

    let ids = [
        "wrap-a", "wrap-b", "wrap-c", "wrap-d", "wrap-e", "wrap-f", "wrap-g", "wrap-h",
    ];
    for id in ids {
        use_signal(&input_value_signal_key(id), String::new).set(String::new());
    }
    use_signal(&input_value_signal_key(NUMBER), String::new).set("p".into());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(NUMBER));
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(0.0);

    fn row(ids: &[&str]) -> Element {
        let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w("100%");
        for id in ids {
            row = row.child(gallery_like_wrap_input(id));
        }
        row.into()
    }

    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .gap("0.75rem")
        .child(row(&ids[..4]))
        .child(row(&ids[4..]));
    let mut root: Element = scroll_area(REST)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .w(VW)
        .h(VH)
        .child(col)
        .into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let _ = tree.build_and_layout(&mut root, VW, VH, [-1.0, -1.0]);

    let mut bounds = Vec::new();
    collect_input_bounds(&root, &mut bounds);
    bounds.sort_by(|a, b| {
        a.1[1]
            .partial_cmp(&b.1[1])
            .unwrap()
            .then(a.1[0].partial_cmp(&b.1[0]).unwrap())
    });
    let first = bounds[0].1;
    let scroll_y = (first[1] + first[3] - 8.0).max(1.0);
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(scroll_y);

    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, 4.0],
        focus_id_for_input(NUMBER).as_str(),
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    tree.set_collect_skip_allowed(true);
    let clipped = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);

    assert!(
        text_nodes_have_caret(&clipped.text_nodes, NUMBER),
        "number field must keep a caret after the first sibling is a sliver"
    );
    assert!(
        !painted_ime_field_in_overflow_clip(
            &clipped.text_nodes,
            &focus_id_for_input(NUMBER),
            VW,
            VH
        ),
        "sliver caret must not count as in-clip (Windows Imm hang)"
    );

    // Paint-only focus move — do not rebuild the tree (InputShowcase isolate_reads).
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(SEARCH));
    assert!(
        !painted_ime_field_in_overflow_clip(
            &clipped.text_nodes,
            &focus_id_for_input(SEARCH),
            VW,
            VH
        ),
        "Search placeholder without a caret must not Imm; composing would freeze \
         the previous field's ring while IME candidates sit on Search"
    );

    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, 80.0],
        focus_id_for_input(SEARCH).as_str(),
        false,
        VW,
        VH,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let moved = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);

    assert!(
        text_nodes_have_caret(&moved.text_nodes, SEARCH),
        "paint-sync + collect-skip must put the caret on Search after focus_id.set"
    );
    assert!(
        !text_nodes_have_caret(&moved.text_nodes, NUMBER),
        "the sliver number field must drop its caret when focus moved to Search"
    );
    let search_spread = input_shadow_spread(&root, SEARCH).unwrap_or(0.0);
    let number_spread = input_shadow_spread(&root, NUMBER).unwrap_or(0.0);
    assert!(
        search_spread >= 2.5,
        "Search must receive the focus ring (spread={search_spread})"
    );
    assert!(
        number_spread < 1.5,
        "number field must drop the focus ring (spread={number_spread})"
    );
    assert!(
        painted_ime_field_in_overflow_clip(&moved.text_nodes, &focus_id_for_input(SEARCH), VW, VH),
        "after the chrome frame, Search's caret is in clip and may Imm"
    );
}

/// Gallery forms first wrap row, no sliver: Placeholder → number. IME already
/// follows `focus_id` (number placeholder leaf). Collect-skip must still move
/// caret + ring — `request_redraw` without a hash bust replays Placeholder chrome.
#[test]
fn wrap_first_row_placeholder_to_number_must_move_caret_without_clip() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const REST: &str = "scroll-forms-row1-focus-move";
    const VW: f32 = 520.0;
    const VH: f32 = 200.0;
    const PLACEHOLDER: &str = "row1-a";
    const NUMBER: &str = "row1-d";

    let ids = ["row1-a", "row1-b", "row1-c", "row1-d"];
    for id in ids {
        use_signal(&input_value_signal_key(id), String::new).set(String::new());
    }
    use_signal(&input_value_signal_key(NUMBER), String::new).set("0".into());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(PLACEHOLDER));
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(0.0);

    let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w("100%");
    for id in ids {
        row = row.child(gallery_like_wrap_input(id));
    }
    let mut root: Element = scroll_area(REST)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .w(VW)
        .h(VH)
        .child(row)
        .into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let _ = tree.build_and_layout(&mut root, VW, VH, [-1.0, -1.0]);

    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [80.0, 40.0],
        focus_id_for_input(PLACEHOLDER).as_str(),
        false,
        VW,
        VH,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    tree.set_collect_skip_allowed(true);
    let focused_ph = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    assert!(
        text_nodes_have_caret(&focused_ph.text_nodes, PLACEHOLDER),
        "precondition: Placeholder has the caret"
    );
    assert!(
        painted_text_edit_node(&focused_ph.text_nodes, &focus_id_for_input(NUMBER)).is_some(),
        "precondition: number placeholder leaf is already in the DL (IME can jump)"
    );

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(NUMBER));
    impact = LayoutStatus::Clean;
    sync_class_variants_subtree(
        &mut root,
        [400.0, 40.0],
        focus_id_for_input(NUMBER).as_str(),
        false,
        VW,
        VH,
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let moved = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);

    assert!(
        text_nodes_have_caret(&moved.text_nodes, NUMBER),
        "collect-skip must put the caret on number after Placeholder focus_id.set"
    );
    assert!(
        !text_nodes_have_caret(&moved.text_nodes, PLACEHOLDER),
        "Placeholder must drop its caret when focus moved to number"
    );
    let number_spread = input_shadow_spread(&root, NUMBER).unwrap_or(0.0);
    let ph_spread = input_shadow_spread(&root, PLACEHOLDER).unwrap_or(0.0);
    assert!(
        number_spread >= 2.5,
        "number must receive the focus ring (spread={number_spread})"
    );
    assert!(
        ph_spread < 1.5,
        "Placeholder must drop the focus ring (spread={ph_spread})"
    );
}

/// Live `scroll_paint_only` uses `tick_active_css_transitions_subtree`, not class
/// variants. Focus chrome must still follow `focus_id.set` on that path.
#[test]
fn wrap_flex_grow_tick_active_focus_move_must_repaint_chrome() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const REST: &str = "scroll-forms-tick-active-focus";
    const VW: f32 = 520.0;
    const VH: f32 = 200.0;
    const NUMBER: &str = "tick-d";
    const SEARCH: &str = "tick-h";

    let ids = [
        "tick-a", "tick-b", "tick-c", "tick-d", "tick-e", "tick-f", "tick-g", "tick-h",
    ];
    for id in ids {
        use_signal(&input_value_signal_key(id), String::new).set(String::new());
    }
    use_signal(&input_value_signal_key(NUMBER), String::new).set("p".into());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(NUMBER));
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(0.0);

    fn row(ids: &[&str]) -> Element {
        let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w("100%");
        for id in ids {
            row = row.child(gallery_like_wrap_input(id));
        }
        row.into()
    }

    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .gap("0.75rem")
        .child(row(&ids[..4]))
        .child(row(&ids[4..]));
    let mut root: Element = scroll_area(REST)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .w(VW)
        .h(VH)
        .child(col)
        .into();

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let _ = tree.build_and_layout(&mut root, VW, VH, [-1.0, -1.0]);

    let mut bounds = Vec::new();
    collect_input_bounds(&root, &mut bounds);
    bounds.sort_by(|a, b| {
        a.1[1]
            .partial_cmp(&b.1[1])
            .unwrap()
            .then(a.1[0].partial_cmp(&b.1[0]).unwrap())
    });
    let first = bounds[0].1;
    let scroll_y = (first[1] + first[3] - 8.0).max(1.0);
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(scroll_y);

    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, 4.0],
        focus_id_for_input(NUMBER).as_str(),
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    tree.set_collect_skip_allowed(true);
    let clipped = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);

    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(SEARCH));
    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, 80.0],
        focus_id_for_input(SEARCH).as_str(),
        false,
        VW,
        VH,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(&mut root, &mut tree, &mut impact, VW, VH, [0.0, 0.0]);
    let moved = tree.build_and_layout_fast(&mut root, VW, VH, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);

    assert!(
        text_nodes_have_caret(&moved.text_nodes, SEARCH),
        "tick_active (scroll_paint_only) + paint-sync must put the caret on Search"
    );
    assert!(
        !text_nodes_have_caret(&moved.text_nodes, NUMBER),
        "tick_active must drop the sliver number caret after focus moved"
    );
    assert!(
        clipped
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some()),
        "pre-move frame still had a caret to mismatch against"
    );
}

/// Gallery forms: `scroll-forms` sits *below* the shell header, so a sliver Input
/// still has `visual.y > 0` in window space. `painted_ime_field_in_overflow_clip`
/// must use the ScrollArea clip — not the window Y heuristic, and not the field's
/// own `overflow:hidden` box (the text leaf is always inside that).
#[test]
fn gallery_header_offset_sliver_must_not_push_imm() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);

    const REST: &str = "scroll-forms-header-offset";
    const FIRST: &str = "hdr-a";
    const WIN_W: f32 = 800.0;
    const WIN_H: f32 = 500.0;
    const HEADER_H: f32 = 96.0;

    let ids = ["hdr-a", "hdr-b", "hdr-c", "hdr-d"];
    for id in ids {
        use_signal(&input_value_signal_key(id), String::new).set(String::new());
    }
    use_signal(&input_value_signal_key(FIRST), String::new).set("p".into());
    use_signal("__rui_button_focus_id", String::new).set(focus_id_for_input(FIRST));
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(0.0);

    let mut row = hstack().gap("0.5rem").items("start").wrap("wrap").w("100%");
    for id in ids {
        row = row.child(gallery_like_wrap_input(id));
    }
    let col = Element::new()
        .flex_col()
        .flex_shrink(0.0)
        .w("100%")
        .gap("0.75rem")
        .child(Element::new().w("100%").h(24.0).flex_shrink(0.0))
        .child(row)
        .child(
            Element::new()
                .w("100%")
                .h(900.0)
                .flex_shrink(0.0)
                .bg([0.95, 0.95, 0.95, 1.0]),
        );
    let sa: Element = scroll_area(REST)
        .flex_grow(1.0)
        .min_h(0.0)
        .overflow_y("auto")
        .overflow_x("auto")
        .w(WIN_W)
        .child(col)
        .into();
    let mut root = Element::new()
        .flex_col()
        .w(WIN_W)
        .h(WIN_H)
        .child(
            Element::new()
                .w(WIN_W)
                .h(HEADER_H)
                .flex_shrink(0.0)
                .bg([0.2, 0.2, 0.2, 1.0]),
        )
        .child(sa);

    let mut tree = LayoutTree::new();
    let mut impact = LayoutStatus::Clean;
    sync_text_edit_paint_subtree_in_viewport(
        &mut root,
        &mut tree,
        &mut impact,
        WIN_W,
        WIN_H,
        [0.0, 0.0],
    );
    let _ = tree.build_and_layout(&mut root, WIN_W, WIN_H, [-1.0, -1.0]);

    let mut bounds = Vec::new();
    collect_input_bounds(&root, &mut bounds);
    bounds.sort_by(|a, b| {
        a.1[1]
            .partial_cmp(&b.1[1])
            .unwrap()
            .then(a.1[0].partial_cmp(&b.1[0]).unwrap())
    });
    assert!(!bounds.is_empty(), "expected wrap Inputs, got {bounds:?}");
    let first = bounds[0].1;
    assert!(
        first[1] + 0.5 >= HEADER_H,
        "first Input must sit in the page ScrollArea (layout y={}, header={HEADER_H})",
        first[1]
    );
    // Chrome sliver at the ScrollArea top: keep a few px of the field in clip.
    // `layout_bounds` is window-absolute; subtract the header so scroll_y is
    // content-relative (existing tests use ScrollArea as the layout root).
    let content_y = (first[1] - HEADER_H).max(0.0);
    let scroll_y = (content_y + first[3] - 8.0).max(1.0);
    use_signal(&scroll_offset_y_signal_key(REST), || 0.0_f32).set_silent(scroll_y);

    impact = LayoutStatus::Clean;
    tick_active_css_transitions_subtree(
        &mut root,
        [first[0] + first[2] * 0.5, HEADER_H + 4.0],
        focus_id_for_input(FIRST).as_str(),
        false,
        WIN_W,
        WIN_H,
        [0.0, 0.0],
        &mut tree,
        &mut impact,
    );
    sync_text_edit_paint_subtree_in_viewport(
        &mut root,
        &mut tree,
        &mut impact,
        WIN_W,
        WIN_H,
        [0.0, 0.0],
    );
    tree.set_collect_skip_allowed(true);
    let clipped = tree.build_and_layout_fast(&mut root, WIN_W, WIN_H, [-1.0, -1.0], false, false);
    tree.set_collect_skip_allowed(false);

    let fid = focus_id_for_input(FIRST);
    let node = painted_text_edit_node(&clipped.text_nodes, &fid);
    let diag = node
        .map(|n| {
            format!(
                "leaf=({:.1},{:.1} {:.1}x{:.1}) clip={:?} caret={}",
                n.x,
                n.y,
                n.width,
                n.height,
                n.clip_bounds,
                n.style.input_caret.is_some()
            )
        })
        .unwrap_or_else(|| "no painted text leaf".into());
    assert!(
        text_nodes_have_caret(&clipped.text_nodes, FIRST),
        "live clip-cull exemption must keep the caret (WaitUntil). {diag} scroll_y={scroll_y} first={first:?}"
    );
    assert!(
        node.is_some_and(|n| n.y > 8.0),
        "sliver must still be below the window top so the window-Y heuristic cannot save us. {diag}"
    );
    assert!(
        !painted_ime_field_in_overflow_clip(&clipped.text_nodes, &fid, WIN_W, WIN_H),
        "header-offset chrome sliver must not ImmSetCompositionWindow (Windows WaitUntil hang). {diag} \
         scroll_y={scroll_y} first={first:?}"
    );
}
