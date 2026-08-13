//! WP-3.2：path 产品绘制走 TexturedDraw（CPU 覆盖率光栅）；tessellate 仅回退。

use crate::components::{area_chart, bar_chart, line_chart, pie_chart, ChartDatum};
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

fn find_image_tex_bounds(el: &Element) -> Option<[f32; 4]> {
    if el
        .image_texture
        .as_deref()
        .is_some_and(|k| k.starts_with("rui_path:"))
    {
        return Some(el.layout_bounds);
    }
    el.children.iter().find_map(find_image_tex_bounds)
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
    let pie_b = find_image_tex_bounds(&root).expect("pie raster texture");
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
    let tall_bars = result
        .instances
        .iter()
        .filter(|i| i.rect_size[1] > 8.0 && i.color[3] > 0.2)
        .count();
    assert!(
        tall_bars >= 3,
        "bar chart must paint visible columns, got {tall_bars} / {} instances",
        result.instances.len()
    );
}
