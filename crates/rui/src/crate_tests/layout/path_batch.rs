//! WP-3.2：path 产品绘制走 TexturedDraw（GPU PathPipeline 或 CPU golden）；SDF 矩形不是产品核。

use crate::components::{
    area_chart, bar_chart, define_chart, line_chart, line_y, pie_chart, ChartDatum,
};
use crate::core::tree::Element;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::path::{attach_path, polyline, PathStyle, ATTR_PATH_D, ATTR_PATH_VIEWBOX};
use crate::primitives::r#box::vstack;

#[test]
fn path_leaf_emits_textured_stroke() {
    let mut root = attach_path(
        Element::new().w(80.0).h(60.0),
        &polyline(&[(4.0, 30.0), (40.0, 8.0), (76.0, 50.0)]),
        &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 80.0, 60.0, [-1.0, -1.0]);
    assert!(
        result.textured_draws.iter().any(|d| d.rect[2] > 40.0
            && d.rect[3] > 20.0
            && d.texture_key.starts_with("rui_path:")),
        "polyline stroke must raster to a texture; draws={:?}",
        result.textured_draws
    );
    assert!(
        root.get_attr(ATTR_PATH_D).is_some(),
        "shared path d attr missing"
    );
    let sdf_round_caps = result
        .instances
        .iter()
        .filter(|i| {
            i.border_width == 0.0
                && i.shadow_blur == 0.0
                && (i.rect_size[0] - i.rect_size[1]).abs() < 0.25
                && i.rect_size[0] > 1.5
                && i.rect_size[0] < 8.0
                && i.radii.iter().all(|&r| r > 0.4)
        })
        .count();
    if crate::renderer::gpu2d_enabled() {
        assert_eq!(
            sdf_round_caps,
            0,
            "GPU2D stroke must not emit SDF round-cap quads; instances={}",
            result.instances.len()
        );
        assert!(
            result
                .render_graph
                .nodes
                .iter()
                .any(|n| n.kind == crate::renderer::PassKind::Path2d),
            "collect must enqueue PassKind::Path2d; graph={:?}",
            result.render_graph.nodes
        );
    }
}

#[test]
fn chart_defers_raster_until_layout_selects_its_backend() {
    let mut el: Element = area_chart()
        .values([0.23, 0.91, 0.47, 0.72])
        .size(203.0, 83.0)
        .into();
    crate::core::tree::walk_elements(&el, |node| {
        if node
            .get_attr(crate::primitives::path::ATTR_PATH_SCENE)
            .is_some()
        {
            assert!(
                node.image_texture.is_none(),
                "chart construction must not raster a fallback before backend selection"
            );
        }
    });
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 220.0, 100.0, [-1.0, -1.0]);
    assert!(result
        .textured_draws
        .iter()
        .any(|draw| draw.texture_key.starts_with("rui_path:")));
}

#[test]
fn line_chart_layout_is_not_fake_bars_only() {
    let mut el: Element = line_chart()
        .values([0.2, 0.9, 0.4, 0.7])
        .size(200.0, 80.0)
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 220.0, 100.0, [-1.0, -1.0]);
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.rect[2] > 50.0 && d.rect[3] > 40.0),
        "line chart must emit a path texture, draws={:?} instances={}",
        result.textured_draws,
        result.instances.len()
    );
}

#[test]
fn area_chart_layout_emits_path_texture() {
    let mut el: Element = area_chart()
        .values([0.2, 0.9, 0.4, 0.7])
        .size(200.0, 80.0)
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 220.0, 100.0, [-1.0, -1.0]);
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.rect[2] > 50.0 && d.rect[3] > 40.0),
        "area chart must emit a path texture, draws={:?}",
        result.textured_draws
    );
}

#[test]
fn viewbox_pixel_height_draw_ignores_stretched_leaf() {
    let mut el = attach_path(
        Element::new().w(200.0).h(400.0),
        &polyline(&[(0.0, 40.0), (0.5, 8.0), (1.0, 70.0)]),
        &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0),
    );
    el.set_attr(ATTR_PATH_VIEWBOX, "0 0 1 80");
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 200.0, 400.0, [-1.0, -1.0]);
    let h = result
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("rui_path:"))
        .map(|d| d.rect[3])
        .fold(0.0_f32, f32::max);
    assert!(
        h > 60.0 && h <= 90.0,
        "path draw height must follow viewBox 80px, not leaf 400px; h={h} draws={:?}",
        result.textured_draws
    );
}

#[test]
fn line_chart_texture_does_not_stretch_past_plot() {
    let mut el: Element = line_chart()
        .values([0.2, 0.9, 0.4, 0.7])
        .size(200.0, 80.0)
        .into();
    let mut tree = LayoutTree::new();
    // 视口远高于 plot：若 absolute 100% 吃到视口/内容高，贴图会拉到数百 px。
    let result = tree.build_and_layout(&mut el, 220.0, 400.0, [-1.0, -1.0]);
    let h = result
        .textured_draws
        .iter()
        .map(|d| d.rect[3])
        .fold(0.0_f32, f32::max);
    assert!(
        h > 40.0 && h <= 100.0,
        "line texture height must stay near plot (80px), got {h}; draws={:?}",
        result.textured_draws
    );
}

fn find_path_d_bounds(el: &Element) -> Option<[f32; 4]> {
    if el.get_attr(ATTR_PATH_D).is_some() {
        return Some(el.layout_bounds);
    }
    el.children.iter().find_map(find_path_d_bounds)
}

fn find_chart_tex_bounds(el: &Element, kind: &str) -> Option<[f32; 4]> {
    if el.get_attr("rui_chart").map(|s| s.as_str()) == Some(kind)
        && el
            .get_attr(crate::primitives::path::ATTR_PATH_SCENE)
            .is_some()
    {
        return Some(el.layout_bounds);
    }
    el.children
        .iter()
        .find_map(|c| find_chart_tex_bounds(c, kind))
}

#[test]
fn line_path_leaf_stays_plot_height_above_pie_in_scroll_column() {
    let line: Element = line_chart()
        .values([0.1, 0.35, 0.55, 0.4, 0.85, 0.7])
        .h(140.0)
        .into();
    let pie: Element = pie_chart()
        .data([
            ChartDatum::new("A", 32.0),
            ChartDatum::new("B", 24.0),
            ChartDatum::new("C", 18.0),
        ])
        .size(180.0)
        .into();
    let body: Element = vstack()
        .w("100%")
        .gap(12.0)
        .flex_shrink(0.0)
        .child(line)
        .child(pie)
        .into();
    let mut root = Element::new()
        .w(320.0)
        .h(220.0)
        .overflow_y("auto")
        .overflow_x("hidden")
        .child(body);
    // 滚到饼图带：若折线叶被撑成内容高，viewBox 会把折线画进饼图视口。
    root.scroll_offset = [0.0, 130.0];

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 320.0, 220.0, [-1.0, -1.0]);

    let max_path_draw_h = result
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("rui_path:"))
        .map(|d| d.rect[3])
        .fold(0.0_f32, f32::max);
    assert!(
        max_path_draw_h <= 190.0,
        "path textures must not span the scrollport after pie scroll; max_h={max_path_draw_h} draws={:?}",
        result.textured_draws
    );

    let line_b = find_path_d_bounds(&root).expect("line path leaf");
    let pie_b = find_chart_tex_bounds(&root, "pie").expect("pie path scene");
    assert!(
        line_b[3] > 120.0 && line_b[3] <= 150.0,
        "line path layout height must stay ~140px, got {:?}",
        line_b
    );
    assert!(
        pie_b[3] > 160.0 && pie_b[3] <= 190.0,
        "pie texture layout height must stay ~180px, got {:?}",
        pie_b
    );
    assert!(
        line_b[1] + line_b[3] <= pie_b[1] + 2.0,
        "line path must not overlap pie (line={line_b:?} pie={pie_b:?})"
    );
}

#[test]
fn pie_chart_layout_emits_filled_wedges() {
    let mut el: Element = pie_chart()
        .data([
            ChartDatum::new("A", 3.0),
            ChartDatum::new("B", 2.0),
            ChartDatum::new("C", 1.0),
        ])
        .size(140.0)
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 320.0, 180.0, [-1.0, -1.0]);
    assert!(
        result
            .textured_draws
            .iter()
            .any(|d| d.rect[2] > 100.0 && d.rect[3] > 100.0),
        "pie must emit a raster texture ~size, draws={:?}",
        result.textured_draws
    );
}

#[test]
fn bar_chart_still_lays_out_columns() {
    let mut el: Element = bar_chart()
        .values([0.3, 0.8, 0.5])
        .labels(["A", "B", "C"])
        .h("80px")
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 240.0, 120.0, [-1.0, -1.0]);
    assert!(
        el.children.len() >= 2,
        "plot row + label row, got {}",
        el.children.len()
    );
    let tex = result
        .textured_draws
        .iter()
        .any(|d| d.rect[2] > 40.0 && d.rect[3] > 20.0 && d.texture_key.starts_with("rui_path:"));
    let tall_bars = result
        .instances
        .iter()
        .filter(|i| i.rect_size[1] > 8.0 && i.color[3] > 0.2)
        .count();
    assert!(
        tex || tall_bars >= 3,
        "bar chart must paint visible columns via path texture or SDF, tex={tex} bars={tall_bars} / {} instances",
        result.instances.len()
    );
}

#[test]
fn bar_chart_texture_follows_leaf_width_without_double_draw() {
    let mut el: Element = bar_chart()
        .values([0.3, 0.8, 0.5])
        .labels(["A", "B", "C"])
        .h("80px")
        .into();
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut el, 400.0, 140.0, [-1.0, -1.0]);
    let path_draws: Vec<_> = result
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("rui_path:"))
        .collect();
    let max_w = path_draws.iter().map(|d| d.rect[2]).fold(0.0_f32, f32::max);
    assert!(
        max_w > 280.0,
        "chart texture should fill leaf width (~400), got {max_w}; draws={path_draws:?}"
    );
    assert!(
        path_draws.len() <= 2,
        "must not stack image_texture + path_d rasters, n={} draws={path_draws:?}",
        path_draws.len()
    );
}

/// Gallery data page: several cartesian charts with guides (grid z + plot z) plus tooltip
/// overlays (`z-index: 8`). Those z-indexes must **not** opt the whole page into
/// [`OverlayPaintSplit::page_stack_bands`] — that path was built for CarouselStack cards,
/// and flattening every chart into one global band list mis-assigns labels/textures once
/// the chart region scrolls into view (and the broken paint stays after scroll stops).
#[test]
fn chart_column_does_not_enable_page_stack_bands() {
    let vals = [0.2_f32, 0.9, 0.4, 0.7];
    let mut col = vstack().w("100%").gap(12.0).flex_shrink(0.0);
    for (i, id) in ["c-a", "c-b", "c-c"].iter().enumerate() {
        col = col.child(Element::new().text(format!("label-{i}")));
        let chart: Element = line_chart().id(*id).values(vals).size(320.0, 120.0).into();
        col = col.child(chart);
    }
    let composed: Element = define_chart()
        .id("c-tip")
        .mark(line_y(vals).stroke("chart-1").stroke_width(2.0))
        .guides(true)
        .tooltip(true)
        .crosshair(true)
        .h(120.0)
        .into();
    col = col.child(Element::new().text("label-tip"));
    col = col.child(composed);
    let body: Element = col.into();
    let mut root = Element::new()
        .w(360.0)
        .h(280.0)
        .overflow_y("auto")
        .child(body);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 360.0, 280.0, [-1.0, -1.0]);
    assert!(
        result.overlay_split.page_stack_bands.is_empty(),
        "chart z-index must stay local; global page_stack_bands switch the gallery data-page paint path, got {} bands",
        result.overlay_split.page_stack_bands.len()
    );
    root.scroll_offset = [0.0, 220.0];
    let scrolled = tree.build_and_layout_fast(&mut root, 360.0, 280.0, [-1.0, -1.0], true, false);
    assert!(
        scrolled.overlay_split.page_stack_bands.is_empty(),
        "scroll_only collect must not grow page_stack_bands from charts, got {}",
        scrolled.overlay_split.page_stack_bands.len()
    );
}

#[test]
fn scroll_only_chart_column_textures_stay_separated() {
    let vals = [0.1_f32, 0.55, 0.9];
    let mut col = Element::new().flex_col().flex_shrink(0.0).w(400.0);
    for (idx, id) in ["gal-scroll-a", "gal-scroll-b", "gal-scroll-c"]
        .iter()
        .enumerate()
    {
        let chart: Element = line_chart().id(*id).values(vals).size(400.0, 120.0).into();
        col = col.child(chart);
        if idx < 2 {
            col = col.child(Element::new().h(500.0).flex_shrink(0.0));
        }
    }
    let mut root = Element::new()
        .w(400.0)
        .h(220.0)
        .overflow_y("hidden")
        .child(col);

    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 400.0, 220.0, [-1.0, -1.0]);
    root.scroll_offset = [0.0, 1100.0];
    let result = tree.build_and_layout_fast(&mut root, 400.0, 220.0, [-1.0, -1.0], true, false);
    let path_ys: Vec<f32> = result
        .textured_draws
        .iter()
        .filter(|d| d.texture_key.starts_with("rui_path:"))
        .map(|d| d.rect[1])
        .collect();
    assert!(
        !path_ys.is_empty(),
        "scrolled chart band must still paint at least one texture; draws={:?}",
        result.textured_draws
    );
    if path_ys.len() >= 2 {
        let min_y = path_ys.iter().copied().fold(f32::INFINITY, f32::min);
        let max_y = path_ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_y - min_y) > 8.0,
            "chart textures must not stack at one y during scroll_only collect; ys={path_ys:?}"
        );
    }
}
