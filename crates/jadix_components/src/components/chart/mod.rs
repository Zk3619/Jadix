//! # Chart（experimental · WP-3.2 grammar）
//!
//! Compare the **grammar mind** of [TanStack Charts](https://github.com/TanStack/charts) without moving the runtime:
//! - **Data as rows**：[`ChartDatum`]（label + value + 可选 `series` / `time`）。
//! - **Scales**（Rust compact，对位 TanStack 边界、**不是** JS D3）：linear / band / point /
//!   [`scale_log`] / [`scale_time`]。
//! - **Marks**: Descartes [`bar_y`] [`line_y`] [`area_y`] [`dot`] [`rule_*`]; polar
//!   [`pie`] [`radial_bar`] [`radial_line`] [`radial_area`] [`radial_dot`]；[`custom_mark`]。
//! - **Theme / interaction**：[`ChartTheme`] / [`ChartConfig`] / [`TooltipIndicator`] / [`MarkStyle`]；`.tooltip(true)` / `.crosshair(true)`。
//! - **Composition**: [`define_chart`] stacks in the order of declaration; the default is thin packaging (including `donut_chart` / `radial_chart`).
//! - **Scene**: mark → [`crate::primitives::path::PathCommand`] → same path core.
//!
//! Layering (see `docs/drawing.md` §8 for details):
//!
//! ```text
//! 禁区     DOM · SVG 1.1 全家桶 · JS D3 运行时 · 换渲染器
//! Deepen the polar family · Timeline · Rust-specific scale (log/time) · Chart tooltip/crosshair
//! 别的包   公开 <svg>（WP-3.11）· Canvas paint（WP-3.12）· 通用运动原语（B17）
//! ```
//!
//! Double writing: Builder [`define_chart`] / Default fn; RSX [`Chart`] / [`BarChart`] / …
//!
//! **身份（WP-2.8）**：展示型不要求用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。

mod datum;
mod define;
mod mark;
mod polar;
mod preset;
mod scale;
mod theme;

pub use datum::{ChartDatum, ChartSelect, ChartSelected, ChartTooltipItem};
pub use define::{define_chart, ChartBuilder, ChartDefinition};
pub use mark::{
    area_y, bar_x, bar_y, custom_mark, dot, line_y, pie, rule_x, rule_x_num, rule_y, ChartMark,
    CustomMarkBuilder, MarkChannels, MarkRenderContext, SceneLayer,
};
pub use polar::{radial_area, radial_bar, radial_dot, radial_line};
pub use preset::{
    area_chart, bar_chart, donut_chart, line_chart, pie_chart, radial_chart, AreaChart,
    AreaChartProps, BarChart, BarChartBuilder, BarChartProps, Chart, ChartProps, DonutChart,
    DonutChartProps, LineChart, LineChartBuilder, LineChartProps, PieChart, PieChartBuilder,
    PieChartProps, RadialChart, RadialChartBuilder, RadialChartProps,
};
pub use scale::{
    format_time_label, pie_layout, pie_layout_span, scale_band, scale_linear, scale_log,
    scale_point, scale_time, unix_ms_to_utc, AxisSpec, ScaleKind,
};
pub use theme::{
    ChartConfig, ChartPaint, ChartSeriesConfig, ChartTheme, MarkStyle, TooltipIndicator,
};

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::app::pointer_events::{dispatch_pointer, hit_path, PointerEvent, PointerKind};
    use crate::core::tree::Element;
    use crate::layout::layout_tree::LayoutTree;
    use crate::primitives::path::{
        commands_from_attrs, rasterize_path_rgba, tessellate_path, PathStyle, ATTR_PATH_D,
        ATTR_PATH_MARKERS, ATTR_PATH_SCENE,
    };
    use std::cell::Cell;
    use std::f32::consts::{FRAC_PI_2, TAU};
    use std::rc::Rc;
    use winit::event::MouseButton;

    fn assert_chart_path_draws(mut el: Element) {
        let result = LayoutTree::new().build_and_layout(&mut el, 400.0, 300.0, [-1.0, -1.0]);
        assert!(
            result
                .textured_draws
                .iter()
                .any(|d| d.texture_key.starts_with("jadix_path:")),
            "chart layout must draw its scene through the shared path kernel"
        );
    }

    #[test]
    fn bar_chart_without_id_writes_no_template_id() {
        let el: Element = bar_chart().values([1.0, 2.0]).into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn line_chart_empty_id_writes_no_template_id() {
        let el: Element = line_chart().id("  ").values([1.0, 3.0, 2.0]).into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn pie_chart_without_id_writes_no_template_id() {
        let el: Element = pie_chart().values([1.0, 2.0, 3.0]).into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn donut_chart_without_id_writes_no_template_id() {
        let el: Element = donut_chart().values([1.0, 2.0, 3.0]).into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn radial_chart_without_id_writes_no_template_id() {
        let el: Element = radial_chart()
            .values([1.0, 2.0, 3.0])
            .labels(["A", "B", "C"])
            .into();
        assert!(el.template_id.is_none());
    }

    #[test]
    fn chart_theme_tokens_match_shadcn_chrome() {
        use crate::style::parse_color;
        let t = ChartTheme::tokens();
        assert_eq!(t.background, parse_color("transparent"));
        assert_eq!(t.muted, parse_color("muted-foreground"));
        assert_eq!(t.grid, parse_color("border/50"));
    }

    #[test]
    fn unknown_tooltip_indicator_string_falls_back_to_dot() {
        assert_eq!(TooltipIndicator::from("weird"), TooltipIndicator::Dot);
        let el: Element = bar_chart()
            .values([1.0, 2.0])
            .tooltip_indicator("nope")
            .into();
        assert!(el.attrs.get("jadix_chart").is_some() || !el.children.is_empty());
    }

    #[test]
    fn pie_layout_covers_full_turn() {
        let spans = pie_layout(&[1.0, 1.0, 2.0], -FRAC_PI_2);
        assert_eq!(spans.len(), 3);
        let total: f32 = spans.iter().map(|(a, b)| b - a).sum();
        assert!((total - TAU).abs() < 1e-4, "got {total}");
        assert!((spans[0].0 + FRAC_PI_2).abs() < 1e-4);
    }

    #[test]
    fn pie_layout_span_half_turn() {
        let spans = pie_layout_span(&[1.0, 1.0], -FRAC_PI_2, FRAC_PI_2);
        assert_eq!(spans.len(), 2);
        let total: f32 = spans.iter().map(|(a, b)| b - a).sum();
        assert!((total - std::f32::consts::PI).abs() < 1e-4, "got {total}");
    }

    #[test]
    fn scale_log_maps_powers() {
        let y = scale_log((1.0, 100.0), (0.0, 100.0), 10.0);
        assert!((y(1.0) - 0.0).abs() < 1e-3);
        assert!((y(10.0) - 50.0).abs() < 1e-3);
        assert!((y(100.0) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn scale_time_uses_f64_domain() {
        let t0 = 1_700_000_000_000i64;
        let t1 = t0 + 86_400_000;
        let x = scale_time((t0, t1), (0.0, 100.0));
        assert!((x(t0) - 0.0).abs() < 1e-3);
        assert!((x(t1) - 100.0).abs() < 1e-3);
        let mid = x(t0 + 43_200_000);
        assert!((mid - 50.0).abs() < 0.5, "got {mid}");
    }

    #[test]
    fn unix_ms_epoch_is_utc() {
        let (y, m, d, hh, mm) = unix_ms_to_utc(0);
        assert_eq!((y, m, d, hh, mm), (1970, 1, 1, 0, 0));
    }

    #[test]
    fn scale_linear_maps_domain_to_range() {
        let y = scale_linear((0.0, 10.0), (100.0, 0.0));
        assert!((y(0.0) - 100.0).abs() < 1e-4);
        assert!((y(10.0) - 0.0).abs() < 1e-4);
        assert!((y(5.0) - 50.0).abs() < 1e-4);
    }

    #[test]
    fn scale_band_respects_padding() {
        let (bw, x_of) = scale_band(4, (0.0, 100.0), 0.2);
        assert!(bw > 0.0 && bw < 25.0);
        assert!(x_of(0) >= 0.0);
        assert!(x_of(3) + bw <= 100.5);
    }

    #[test]
    fn scale_point_centers_categories() {
        let x_of = scale_point(3, (0.0, 100.0), 0.0);
        assert!((x_of(0) - 0.0).abs() < 1e-3);
        assert!((x_of(2) - 100.0).abs() < 1e-3);
        assert!((x_of(1) - 50.0).abs() < 1e-3);
    }

    fn find_attr<'a>(el: &'a Element, key: &str) -> Option<&'a Element> {
        if el.get_attr(key).is_some() {
            return Some(el);
        }
        for c in &el.children {
            if let Some(hit) = find_attr(c, key) {
                return Some(hit);
            }
        }
        None
    }

    #[test]
    fn line_chart_attaches_shared_path_d() {
        let el: Element = line_chart()
            .values([0.2, 0.8, 0.4])
            .size(200.0, 80.0)
            .into();
        let path_leaf =
            find_attr(&el, ATTR_PATH_D).expect("line plot must host path d (same kernel as Svg)");
        let d = path_leaf.get_attr(ATTR_PATH_D).unwrap();
        assert!(d.contains('M') && d.contains('L'), "{d}");
        let cmds = commands_from_attrs(&path_leaf.attrs);
        let rgba = rasterize_path_rgba(
            &cmds,
            &PathStyle::fill_stroke([0.2, 0.4, 0.8, 0.3], [0.2, 0.4, 0.8, 1.0], 3.0),
            &[],
            200,
            80,
            2,
        );
        assert!(
            rgba.iter().skip(3).step_by(4).any(|&a| a > 32),
            "line path raster must cover pixels"
        );
        let quads = tessellate_path(
            &cmds,
            &PathStyle::fill_stroke([0.2, 0.4, 0.8, 0.3], [0.2, 0.4, 0.8, 1.0], 2.5),
        );
        assert!(
            quads.iter().any(|q| q.rotate_deg.abs() > 0.5),
            "fallback tessellate still emits rotated stroke quads"
        );
    }

    #[test]
    fn pie_chart_slices_use_path_kernel() {
        let el: Element = pie_chart()
            .data([
                ChartDatum::new("A", 2.0),
                ChartDatum::new("B", 1.0),
                ChartDatum::new("C", 1.0),
            ])
            .into();
        let plot = el
            .children
            .iter()
            .find(|c| c.get_attr("jadix_chart").map(|s| s.as_str()) == Some("pie"))
            .expect("pie plot");
        assert!(plot.get_attr(ATTR_PATH_SCENE).is_some());
        assert_chart_path_draws(el);
    }

    #[test]
    fn chart_datum_data_api_matches_values_labels() {
        let a: Element = bar_chart()
            .data([ChartDatum::new("Mon", 1.0), ChartDatum::new("Tue", 2.0)])
            .into();
        let b: Element = bar_chart().values([1.0, 2.0]).labels(["Mon", "Tue"]).into();
        assert_eq!(a.children.len(), b.children.len());
    }

    #[test]
    fn area_chart_mark_attr_and_no_id() {
        let el: Element = area_chart().values([0.2, 0.9, 0.4]).into();
        assert!(el.template_id.is_none());
        assert_eq!(el.get_attr("jadix_chart").map(|s| s.as_str()), Some("area"));
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
    }

    #[test]
    fn line_chart_default_kind_is_line_not_area() {
        let el: Element = line_chart().values([0.2, 0.9, 0.4]).h(80.0).into();
        assert_eq!(el.get_attr("jadix_chart").map(|s| s.as_str()), Some("line"));
        assert!(find_attr(&el, ATTR_PATH_SCENE).is_some());
    }

    #[test]
    fn bar_chart_pointer_down_selects_index() {
        let hit = Rc::new(Cell::new(None::<usize>));
        let h = hit.clone();
        let mut el: Element = bar_chart()
            .values([0.2, 0.9, 0.5])
            .labels(["A", "B", "C"])
            .h("80px")
            .on_select(move |i| h.set(Some(i)))
            .into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut el, 300.0, 140.0, [-1.0, -1.0]);
        fn find_pd(el: &Element) -> Option<&Element> {
            if el.on_mousedown.is_some() {
                return Some(el);
            }
            el.children.iter().find_map(find_pd)
        }
        let plot = find_pd(&el).expect("plot pointer handler");
        let b = plot.layout_bounds;
        assert!(b[2] > 10.0 && b[3] > 10.0, "plot bounds {b:?}");
        let x = b[0] + b[2] * 0.50;
        let y = b[1] + b[3] * 0.45;
        let path = hit_path(&el, x, y);
        assert!(!path.is_empty(), "hit path empty at ({x},{y}) bounds={b:?}");
        let mut ev = PointerEvent::new(x, y, MouseButton::Left, 1);
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        assert_eq!(hit.get(), Some(1), "expected middle bar, bounds={b:?}");
    }

    #[test]
    fn pie_pointer_down_does_not_select_sibling_chart() {
        let a = Rc::new(Cell::new(None::<usize>));
        let b = Rc::new(Cell::new(None::<usize>));
        let ha = a.clone();
        let hb = b.clone();
        let pie_a: Element = pie_chart()
            .id("iso-a")
            .values([2.0, 1.0, 1.0])
            .size(120.0)
            .on_select(move |i| ha.set(Some(i)))
            .into();
        let pie_b: Element = pie_chart()
            .id("iso-b")
            .values([2.0, 1.0, 1.0])
            .size(120.0)
            .on_select(move |i| hb.set(Some(i)))
            .into();
        let mut root: Element = crate::primitives::r#box::vstack()
            .child(pie_a)
            .child(pie_b)
            .into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
        fn find_all_pd(el: &Element, out: &mut Vec<[f32; 4]>) {
            if el.on_mousedown.is_some() && el.get_attr("jadix_chart").is_some() {
                out.push(el.layout_bounds);
            }
            for c in &el.children {
                find_all_pd(c, out);
            }
        }
        let mut plots = Vec::new();
        find_all_pd(&root, &mut plots);
        assert_eq!(plots.len(), 2, "expected two pie plots, got {plots:?}");
        let pa = plots[0];
        let x = pa[0] + pa[2] * 0.72;
        let y = pa[1] + pa[3] * 0.50;
        let path = hit_path(&root, x, y);
        let mut ev = PointerEvent::new(x, y, MouseButton::Left, 1);
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        assert!(a.get().is_some(), "first pie should select, bounds={pa:?}");
        assert_eq!(b.get(), None, "sibling pie must stay unselected");
    }

    #[test]
    fn pie_donut_inner_radius_still_rasters() {
        let el: Element = pie_chart()
            .values([2.0, 1.0, 1.0])
            .inner_radius(0.55)
            .into();
        let plot = el
            .children
            .iter()
            .find(|c| c.get_attr("jadix_chart").map(|s| s.as_str()) == Some("pie"))
            .expect("pie plot");
        assert!(plot.get_attr(ATTR_PATH_SCENE).is_some());
        assert_chart_path_draws(el);
    }

    #[test]
    fn define_chart_layers_marks_and_custom_theme() {
        let rows = [
            ChartDatum::new("A", 2.0),
            ChartDatum::new("B", 5.0),
            ChartDatum::new("C", 3.0),
        ];
        let theme = ChartTheme::tokens().palette([[0.1, 0.4, 0.9, 1.0], [0.2, 0.7, 0.3, 1.0]]);
        let el: Element = define_chart()
            .theme(theme)
            .mark(
                area_y(rows.clone())
                    .fill_opacity(0.35)
                    .stroke("#2563eb")
                    .stroke_width(2.0),
            )
            .mark(line_y(rows.clone()).stroke("#2563eb").stroke_width(2.0))
            .mark(dot(rows.clone()).r(4.0).fill("#2563eb"))
            .mark(rule_y([4.0]).stroke("#dc2626").stroke_dash([4.0, 2.0]))
            .y_linear()
            .y_nice()
            .guides(true)
            .h(140.0)
            .into();
        assert!(el.template_id.is_none());
        assert_eq!(
            el.get_attr("jadix_chart").map(|s| s.as_str()),
            Some("chart")
        );
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
        assert_chart_path_draws(el);
    }

    #[test]
    fn custom_mark_emits_scene_layers() {
        use crate::primitives::path::{polyline, PathStyle};
        let el: Element = define_chart()
            .mark(custom_mark("diag", |ctx| {
                let a = (ctx.scales.pad[0], ctx.scales.plot_h - ctx.scales.pad[3]);
                let b = (ctx.scales.plot_w - ctx.scales.pad[2], ctx.scales.pad[1]);
                vec![SceneLayer::path(
                    "diag",
                    polyline(&[a, b]),
                    PathStyle::stroke(ctx.theme.color(0), 3.0),
                )]
            }))
            .h(80.0)
            .width_px(120.0)
            .guides(false)
            .into();
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
        assert_chart_path_draws(el);
    }

    #[test]
    fn series_z_splits_line_groups() {
        let rows = [
            ChartDatum::new("Jan", 1.0).series("east"),
            ChartDatum::new("Feb", 3.0).series("east"),
            ChartDatum::new("Jan", 2.0).series("west"),
            ChartDatum::new("Feb", 2.5).series("west"),
        ];
        let mark = line_y(rows);
        let ch = ChartMark::from(mark).channels();
        assert_eq!(ch.x_cats, vec!["Jan".to_string(), "Feb".to_string()]);
        assert_eq!(ch.y_nums.len(), 4);
    }

    #[test]
    fn time_rows_infer_x_time_scale() {
        let t0 = 1_700_000_000_000i64;
        let rows = [
            ChartDatum::at(t0, 1.0),
            ChartDatum::at(t0 + 86_400_000, 3.0),
            ChartDatum::at(t0 + 2 * 86_400_000, 2.0),
        ];
        let mark = ChartMark::from(line_y(rows.clone()));
        assert!(!mark.channels().x_times.is_empty());
        let el: Element = define_chart()
            .mark(line_y(rows))
            .x_time()
            .tooltip(true)
            .crosshair(true)
            .h(80.0)
            .width_px(200.0)
            .guides(true)
            .into();
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
        fn chart_kind(el: &Element) -> Option<&str> {
            el.get_attr("jadix_chart")
                .map(|s| s.as_str())
                .or_else(|| el.children.iter().find_map(chart_kind))
        }
        assert_eq!(chart_kind(&el), Some("line"));
    }

    #[test]
    fn y_log_chart_rasters() {
        let rows = [
            ChartDatum::new("a", 1.0),
            ChartDatum::new("b", 10.0),
            ChartDatum::new("c", 100.0),
        ];
        let el: Element = define_chart()
            .mark(line_y(rows))
            .y_log()
            .h(80.0)
            .width_px(160.0)
            .guides(false)
            .into();
        assert!(
            el.image_texture
                .as_deref()
                .is_some_and(|k| k.starts_with("jadix_path:"))
                || find_attr(&el, ATTR_PATH_D).is_some()
        );
    }

    #[test]
    fn radial_bar_is_polar_and_rasters() {
        let rows = [
            ChartDatum::new("N", 4.0),
            ChartDatum::new("E", 2.0),
            ChartDatum::new("S", 3.0),
            ChartDatum::new("W", 1.5),
        ];
        let mark = ChartMark::from(radial_bar(rows.clone()));
        assert!(mark.is_polar());
        assert_eq!(mark.kind(), "radialBar");
        let el: Element = define_chart()
            .mark(radial_bar(rows.clone()))
            .mark(radial_dot(rows).r(3.0))
            .guides(true)
            .tooltip(true)
            .size(140.0)
            .into();
        let kind = el.get_attr("jadix_chart").map(|s| s.as_str());
        assert!(
            kind == Some("chart") || kind == Some("radialBar"),
            "got {kind:?}"
        );
        assert_chart_path_draws(el);
    }

    #[test]
    fn tooltip_color_uses_explicit_mark_paint() {
        use super::scale::resolve_scales;
        let fill = [0.12, 0.34, 0.56, 1.0];
        let mark = ChartMark::from(bar_y([ChartDatum::new("A", 2.0)]).fill(ChartPaint::Rgba(fill)));
        let scales = resolve_scales(
            120.0,
            80.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["A".into()],
            &[],
            &[],
            &[2.0],
            false,
        );
        let theme = ChartTheme::tokens();
        let ctx = MarkRenderContext::new(&scales, &theme, None, 80.0);
        assert_eq!(mark.tooltip_color(&ctx, 0), Some(fill));
    }

    #[test]
    fn radial_marks_share_category_anchor_points() {
        use super::scale::resolve_scales;
        let rows = [
            ChartDatum::new("N", 4.0),
            ChartDatum::new("E", 2.0),
            ChartDatum::new("S", 3.0),
            ChartDatum::new("W", 1.5),
        ];
        let scales = resolve_scales(
            180.0,
            180.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["N".into(), "E".into(), "S".into(), "W".into()],
            &[],
            &[],
            &[4.0, 2.0, 3.0, 1.5],
            true,
        );
        let theme = ChartTheme::tokens();
        let ctx = MarkRenderContext::new(&scales, &theme, None, 180.0);
        let bar = ChartMark::from(radial_bar(rows.clone()));
        let line = ChartMark::from(radial_line(rows.clone()));
        let dot = ChartMark::from(radial_dot(rows));
        for index in 0..4 {
            let a = bar.datum_point(&ctx, index).unwrap();
            let b = line.datum_point(&ctx, index).unwrap();
            let c = dot.datum_point(&ctx, index).unwrap();
            assert!((b.0 - c.0).abs() < 1e-4 && (b.1 - c.1).abs() < 1e-4);
            let center = (scales.polar_cx(), scales.polar_cy());
            let av = (a.0 - center.0, a.1 - center.1);
            let bv = (b.0 - center.0, b.1 - center.1);
            assert!((av.0 * bv.1 - av.1 * bv.0).abs() < 1e-3);
        }
    }

    #[test]
    fn format_time_label_picks_granularity() {
        let t = 1_704_067_200_000i64; // 2023-12-31 approx
        assert!(format_time_label(t, 3_600_000).contains(':'));
        let d = format_time_label(t, 30 * 86_400_000);
        assert!(d.contains('-'), "{d}");
    }

    #[test]
    fn pie_selected_grows_outer_radius_without_translating() {
        use super::scale::resolve_scales;
        use crate::primitives::path::PathCommand;
        let rows = [
            ChartDatum::new("A", 2.0),
            ChartDatum::new("B", 1.0),
            ChartDatum::new("C", 1.0),
        ];
        let mark = ChartMark::from(pie(rows));
        let scales = resolve_scales(
            180.0,
            180.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["A".into(), "B".into(), "C".into()],
            &[],
            &[],
            &[2.0, 1.0, 1.0],
            true,
        );
        let theme = ChartTheme::tokens();
        let plain = mark.render(&MarkRenderContext::new(&scales, &theme, None, 180.0));
        let selected = mark.render(&MarkRenderContext::new(&scales, &theme, Some(0), 180.0));
        assert_eq!(plain.len(), 3);
        assert_eq!(selected.len(), 3);
        let sel = selected.last().expect("selected drawn last");
        assert!(
            sel.style.stroke.is_some() && sel.style.stroke_width >= 1.5,
            "selected slice must stroke, {:?}",
            sel.style
        );
        fn pts(cmds: &[PathCommand]) -> Vec<(f32, f32)> {
            crate::primitives::path::points_from_commands(cmds)
        }
        fn centroid(pts: &[(f32, f32)]) -> (f32, f32) {
            let n = pts.len().max(1) as f32;
            (
                pts.iter().map(|p| p.0).sum::<f32>() / n,
                pts.iter().map(|p| p.1).sum::<f32>() / n,
            )
        }
        let p0 = pts(&plain[0].commands);
        let s0 = pts(&sel.commands);
        let c0 = centroid(&p0);
        let c1 = centroid(&s0);
        let dist = (c0.0 - c1.0).hypot(c0.1 - c1.1);
        assert!(
            dist < 6.0,
            "TanStack radialArc keeps the same center; centroid jump={dist}"
        );
        let r_plain = p0
            .iter()
            .map(|p| (p.0 - 90.0).hypot(p.1 - 90.0))
            .fold(0.0f32, f32::max);
        let r_sel = s0
            .iter()
            .map(|p| (p.0 - 90.0).hypot(p.1 - 90.0))
            .fold(0.0f32, f32::max);
        assert!(
            r_sel > r_plain + 3.0,
            "selected outerRadius should grow, plain={r_plain} sel={r_sel}"
        );
    }

    #[test]
    fn donut_selected_keeps_inner_radius() {
        use super::scale::resolve_scales;
        use crate::primitives::path::PathCommand;
        let rows = [
            ChartDatum::new("A", 2.0),
            ChartDatum::new("B", 1.0),
            ChartDatum::new("C", 1.0),
        ];
        let mark = ChartMark::from(pie(rows).inner_radius(0.55));
        let scales = resolve_scales(
            180.0,
            180.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["A".into(), "B".into(), "C".into()],
            &[],
            &[],
            &[2.0, 1.0, 1.0],
            true,
        );
        let theme = ChartTheme::tokens();
        let ctx_plain = MarkRenderContext::new(&scales, &theme, None, 180.0);
        let ctx_sel = MarkRenderContext::new(&scales, &theme, Some(0), 180.0);
        let plain = mark.render(&ctx_plain);
        let selected = mark.render(&ctx_sel);
        fn inner_r(cmds: &[PathCommand]) -> f32 {
            crate::primitives::path::points_from_commands(cmds)
                .into_iter()
                .map(|(x, y)| (x - 90.0).hypot(y - 90.0))
                .fold(f32::MAX, f32::min)
        }
        let sel = selected.last().expect("selected last");
        let i0 = inner_r(&plain[0].commands);
        let i1 = inner_r(&sel.commands);
        assert!(
            (i0 - i1).abs() < 1.5,
            "donut inner ring must stay put, plain={i0} sel={i1}"
        );
    }

    #[test]
    fn area_fill_covers_below_line_in_buffer() {
        use super::scale::resolve_scales;
        use crate::primitives::path::rasterize_layers_rgba;
        let mark = ChartMark::from(area_y([0.2f32, 0.9, 0.4]));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::linear(),
            &AxisSpec::linear(),
            vec![],
            &[0.0, 1.0, 2.0],
            &[],
            &[0.2, 0.9, 0.4],
            true,
        );
        let theme = ChartTheme::tokens();
        let layers = mark.render(&MarkRenderContext::new(&scales, &theme, None, 80.0));
        let path_layers: Vec<_> = layers
            .iter()
            .filter(|l| !l.commands.is_empty())
            .map(|l| (l.commands.clone(), l.style.clone()))
            .collect();
        let rgba = rasterize_layers_rgba(&path_layers, &[], 0.0, 200, 80, 1);
        let mut top = 0u32;
        let mut bot = 0u32;
        for y in 0..40u32 {
            for x in 0..200u32 {
                if rgba[((y * 200 + x) * 4 + 3) as usize] > 40 {
                    top += 1;
                }
            }
        }
        for y in 40..80u32 {
            for x in 0..200u32 {
                if rgba[((y * 200 + x) * 4 + 3) as usize] > 40 {
                    bot += 1;
                }
            }
        }
        assert!(
            bot > top * 2,
            "area should fill toward baseline (buffer bottom / origin), top={top} bot={bot}"
        );
    }

    #[test]
    fn bar_chart_axis_labels_extend_past_plot_height() {
        let mut el: Element = bar_chart()
            .values([0.2, 0.9])
            .labels(["Mon", "Tue"])
            .h("80px")
            .into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut el, 300.0, 400.0, [-1.0, -1.0]);
        assert!(
            el.layout_bounds[3] > 96.0,
            "plot 80px + axis labels must not be clamped to plot height, got {:?}",
            el.layout_bounds
        );
    }

    #[test]
    fn bar_axis_labels_center_on_band() {
        use crate::layout::{lpa_kind, DimKind};
        let labels = ["Mon", "Tue", "Wed", "Thu", "Fri"];
        let plot_w = 320.0;
        let pad = [8.0, 8.0, 8.0, 0.0];
        let n = labels.len();
        let (bw, x_of) = scale_band(n, (pad[0], plot_w - pad[2]), 0.2);
        let ticks: Vec<(f32, String)> = labels
            .iter()
            .enumerate()
            .map(|(i, lab)| (x_of(i) + bw * 0.5, (*lab).to_string()))
            .collect();
        let row = super::define::tick_label_row(&ticks, plot_w);
        assert_eq!(row.children.len(), n);
        for (i, lab) in labels.iter().enumerate() {
            let cell = &row.children[i];
            let got = match lpa_kind(cell.style.inset.left) {
                DimKind::Percent(p) => p * 100.0,
                other => panic!("{lab} inset.left not %, got {other:?}"),
            };
            let expect = ticks[i].0 / plot_w * 100.0;
            assert!(
                (got - expect).abs() < 0.05,
                "{lab} label left {got}% vs bar center {expect}%"
            );
            assert!(
                (cell.style.translate_pct[0] + 0.5).abs() < 1e-4,
                "{lab} must translate_pct -50% to sit on the tick"
            );
        }
    }

    #[test]
    fn bar_sits_on_plot_baseline() {
        use super::scale::resolve_scales_pad;
        use crate::primitives::path::points_from_commands;
        let mark = ChartMark::from(bar_y([("A", 0.8f32), ("B", 0.4)]));
        let scales = resolve_scales_pad(
            200.0,
            80.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["A".into(), "B".into()],
            &[],
            &[],
            &[0.8, 0.4],
            true,
            [8.0, 8.0, 8.0, 0.0],
        );
        let theme = ChartTheme::tokens();
        let layers = mark.render(&MarkRenderContext::new(&scales, &theme, None, 80.0));
        let mut max_y = 0.0f32;
        for l in &layers {
            for p in points_from_commands(&l.commands) {
                max_y = max_y.max(p.1);
            }
        }
        assert!(
            (max_y - 80.0).abs() < 1.5,
            "bar bottom {max_y} should sit on plot_h=80 (axis)"
        );
    }

    #[test]
    fn bar_base_radius_rounds_bottom_when_no_axis() {
        use super::scale::resolve_scales_pad;
        use crate::primitives::path::points_from_commands;
        let mark = ChartMark::from(bar_y([("A", 1.0f32)]).corner_radius(8.0));
        let scales = resolve_scales_pad(
            120.0,
            80.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["A".into()],
            &[],
            &[],
            &[1.0],
            true,
            [0.0, 8.0, 0.0, 0.0],
        );
        let theme = ChartTheme::tokens();
        let mut ctx = MarkRenderContext::new(&scales, &theme, None, 80.0);
        ctx.round_bar_base = true;
        let layers = mark.render(&ctx);
        let pts: Vec<_> = layers
            .iter()
            .flat_map(|l| points_from_commands(&l.commands))
            .collect();
        assert!(
            pts.len() > 8,
            "rounded all-four corners must emit arc vertices, got {}",
            pts.len()
        );
        let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_y = pts.iter().map(|p| p.1).fold(0.0f32, f32::max);
        // Bottom corner rounding: The bottom of the column can be slightly beyond plot_h (+1px to cover the raster for half-open cropping), with more than 4 vertices.
        assert!(max_y <= 81.5, "bottom still within plot, max_y={max_y}");
        assert!(min_y >= 7.0, "top pad leaves headroom, min_y={min_y}");
    }

    #[test]
    fn bar_chart_tooltip_tracks_mousemove() {
        let el: Element = bar_chart().values([0.2, 0.9]).labels(["Mon", "Tue"]).into();
        fn find_move(el: &Element) -> bool {
            el.on_mousemove.is_some() || el.children.iter().any(find_move)
        }
        assert!(
            find_move(&el),
            "tooltip charts must listen to mousemove for hover"
        );
    }

    #[test]
    fn line_stroke_color_does_not_dim_when_non_first_point_selected() {
        use super::scale::resolve_scales;
        let mark = ChartMark::from(line_y([0.2f32, 0.9, 0.4]));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::linear(),
            &AxisSpec::linear(),
            vec![],
            &[0.0, 1.0, 2.0],
            &[],
            &[0.2, 0.9, 0.4],
            false,
        );
        let theme = ChartTheme::tokens();
        let plain = mark.render(&MarkRenderContext::new(&scales, &theme, None, 80.0));
        let selected = mark.render(&MarkRenderContext::new(&scales, &theme, Some(1), 80.0));
        let stroke_of = |layers: &[SceneLayer]| {
            layers
                .iter()
                .rev()
                .find(|l| l.style.stroke.is_some() && l.style.fill.is_none())
                .and_then(|l| l.style.stroke)
        };
        assert_eq!(
            stroke_of(&plain),
            stroke_of(&selected),
            "selecting a non-zero vertex must not dim the whole polyline"
        );
    }

    #[test]
    fn dot_path_centers_sit_on_line_vertices_and_paint_after_line() {
        use super::scale::resolve_scales;
        use crate::primitives::path::points_from_commands;
        let rows = [0.2f32, 0.9, 0.4];
        let line = ChartMark::from(line_y(rows));
        let dots = ChartMark::from(dot(rows).r(4.0));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::linear(),
            &AxisSpec::linear(),
            vec![],
            &[0.0, 1.0, 2.0],
            &[],
            &[0.2, 0.9, 0.4],
            false,
        );
        let theme = ChartTheme::tokens();
        let ctx = MarkRenderContext::new(&scales, &theme, Some(1), 80.0);
        let mut layers = line.render(&ctx);
        let n_line = layers.len();
        layers.extend(dots.render(&ctx));
        assert!(layers.len() > n_line, "dots must emit path layers");
        assert!(
            layers[n_line..]
                .iter()
                .all(|l| !l.commands.is_empty() && l.markers.is_empty()),
            "dots are path circles, not raster markers"
        );
        let line_pts = points_from_commands(
            &layers
                .iter()
                .find(|l| l.style.stroke.is_some() && l.style.fill.is_none())
                .expect("line stroke")
                .commands,
        );
        fn centroid(cmds: &[crate::primitives::path::PathCommand]) -> (f32, f32) {
            let pts = crate::primitives::path::points_from_commands(cmds);
            let n = pts.len().max(1) as f32;
            (
                pts.iter().map(|p| p.0).sum::<f32>() / n,
                pts.iter().map(|p| p.1).sum::<f32>() / n,
            )
        }
        let last = layers.last().expect("selected dot last");
        let c = centroid(&last.commands);
        let vertex = line_pts[1];
        let dist = (c.0 - vertex.0).hypot(c.1 - vertex.1);
        assert!(
            dist < 0.35,
            "selected dot centroid {c:?} must sit on vertex {vertex:?}, dist={dist}"
        );
    }

    #[test]
    fn radial_line_hit_does_not_steal_bar_away_from_vertices() {
        use super::scale::resolve_scales;
        let rows = [
            ChartDatum::new("N", 8.0),
            ChartDatum::new("E", 2.0),
            ChartDatum::new("S", 5.0),
            ChartDatum::new("W", 3.0),
        ];
        let bar = ChartMark::from(radial_bar(rows.clone()).inner_radius(0.2));
        let line = ChartMark::from(radial_line(rows).stroke_width(2.0));
        let scales = resolve_scales(
            180.0,
            180.0,
            &AxisSpec::band(),
            &AxisSpec::linear(),
            vec!["N".into(), "E".into(), "S".into(), "W".into()],
            &[],
            &[],
            &[8.0, 2.0, 5.0, 3.0],
            true,
        );
        let theme = ChartTheme::tokens();
        let ctx = MarkRenderContext::new(&scales, &theme, None, 180.0);
        let cx = scales.polar_cx();
        let cy = scales.polar_cy();
        // Near the hole on band 0: bar should hit, line must miss (otherwise compose never selects bars).
        let a = scales.polar_angle_center(
            0,
            4,
            -std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2 + TAU,
        );
        let hole = scales.polar_r_max() * 0.2;
        let local = scales.polar_xy(a, hole + 6.0);
        assert_eq!(bar.hit_index(&ctx, local), Some(0));
        assert_eq!(
            line.hit_index(&ctx, local),
            None,
            "radial line must not unbounded-nearest-hit at {local:?} center=({cx},{cy})"
        );
        let far = (cx, cy);
        assert_eq!(line.hit_index(&ctx, far), None);
    }

    #[test]
    fn pie_pointer_down_in_scrolled_viewport() {
        let hit = Rc::new(Cell::new(None::<usize>));
        let h = hit.clone();
        let pie: Element = pie_chart()
            .values([2.0, 1.0, 1.0])
            .size(120.0)
            .on_select(move |i| h.set(Some(i)))
            .into();
        let spacer: Element = crate::primitives::r#box::r#box().w(160.0).h(90.0).into();
        let mut content: Element = crate::primitives::r#box::vstack()
            .child(spacer)
            .child(pie)
            .into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut content, 200.0, 400.0, [-1.0, -1.0]);

        let mut viewport = Element::new().overflow("scroll");
        viewport.layout_bounds = [0.0, 0.0, 200.0, 100.0];
        viewport.scroll_offset = [0.0, 90.0];
        viewport.children.push(content);

        fn find_pd(el: &Element) -> Option<&Element> {
            if el.on_mousedown.is_some() && el.get_attr("jadix_chart").is_some() {
                return Some(el);
            }
            el.children.iter().find_map(find_pd)
        }
        let plot = find_pd(&viewport).expect("pie plot");
        let b = plot.layout_bounds;
        let vx = b[0] - viewport.scroll_offset[0];
        let vy = b[1] - viewport.scroll_offset[1];
        assert!(
            vy + b[3] > 0.0 && vy < 100.0,
            "pie must be visually in the 100px viewport after scroll, layout={b:?} visual_y={vy}"
        );
        let x = vx + b[2] * 0.72;
        let y = vy + b[3] * 0.50;
        let path = hit_path(&viewport, x, y);
        assert!(
            !path.is_empty(),
            "hit path empty at visual ({x},{y}) layout={b:?}"
        );
        let plot_entry = path
            .iter()
            .find(|e| e.on_mousedown.is_some())
            .expect("plot handler on path");
        assert!(
            (plot_entry.bounds[1] - vy).abs() < 2.0,
            "handler bounds must be visual, got {:?} vy={vy}",
            plot_entry.bounds
        );
        let mut ev = PointerEvent::new(x, y, MouseButton::Left, 1);
        dispatch_pointer(&path, &mut ev, PointerKind::Down);
        assert!(
            hit.get().is_some(),
            "pie in scrolled viewport should select, bounds={b:?} visual=[{vx},{vy}]"
        );
    }

    #[test]
    fn line_selected_vertex_lives_in_path_scene_not_markers() {
        let el: Element = line_chart()
            .values([0.2, 0.8, 0.4])
            .size(200.0, 80.0)
            .selected(Some(1))
            .into();
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
        assert!(
            find_attr(&el, ATTR_PATH_MARKERS).is_none(),
            "selected vertex is a path circle in the scene, not ATTR_PATH_MARKERS / SDF overlay"
        );
        assert!(
            el.image_texture
                .as_deref()
                .is_some_and(|k| k.starts_with("jadix_path:"))
                || find_attr(&el, ATTR_PATH_SCENE).is_some()
        );
    }

    #[test]
    fn next_chart_selection_toggles_and_clears() {
        use super::define::next_chart_selection;
        assert_eq!(next_chart_selection(None, Some(2)), Some(2));
        assert_eq!(next_chart_selection(Some(2), Some(2)), None);
        assert_eq!(next_chart_selection(Some(2), Some(0)), Some(0));
        assert_eq!(next_chart_selection(Some(2), None), None);
    }

    #[test]
    fn line_hit_misses_far_from_polyline() {
        use super::scale::resolve_scales;
        let mark = ChartMark::from(line_y([0.2f32, 0.9, 0.4]));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::linear(),
            &AxisSpec::linear(),
            vec![],
            &[0.0, 1.0, 2.0],
            &[],
            &[0.2, 0.9, 0.4],
            false,
        );
        let theme = ChartTheme::tokens();
        let ctx = MarkRenderContext::new(&scales, &theme, None, 80.0);
        let pt = mark.datum_point(&ctx, 1).expect("vertex 1");
        assert_eq!(mark.hit_index(&ctx, pt), Some(1));
        let far = if pt.1 < 40.0 {
            (pt.0, pt.1 + 36.0)
        } else {
            (pt.0, pt.1 - 36.0)
        };
        assert_eq!(mark.hit_index(&ctx, far), None, "far={far:?} vertex={pt:?}");
    }

    #[test]
    fn selected_dot_radius_stays_near_base() {
        use super::scale::resolve_scales;
        use crate::primitives::path::points_from_commands;
        let rows = [0.2f32, 0.9, 0.4];
        let dots = ChartMark::from(dot(rows).r(4.0));
        let scales = resolve_scales(
            200.0,
            80.0,
            &AxisSpec::linear(),
            &AxisSpec::linear(),
            vec![],
            &[0.0, 1.0, 2.0],
            &[],
            &[0.2, 0.9, 0.4],
            false,
        );
        let theme = ChartTheme::tokens();
        let layers = dots.render(&MarkRenderContext::new(&scales, &theme, Some(1), 80.0));
        let last = layers.last().expect("selected last");
        let pts = points_from_commands(&last.commands);
        let n = pts.len().max(1) as f32;
        let cx = pts.iter().map(|p| p.0).sum::<f32>() / n;
        let cy = pts.iter().map(|p| p.1).sum::<f32>() / n;
        let rmax = pts
            .iter()
            .map(|p| (p.0 - cx).hypot(p.1 - cy))
            .fold(0.0f32, f32::max);
        assert!(
            rmax < 5.6,
            "selected dot r should be base+1 (≈5), got {rmax}"
        );
    }

    #[test]
    fn selected_chart_emits_tooltip_text() {
        let el: Element = line_chart()
            .values([0.2, 0.8, 0.4])
            .size(200.0, 80.0)
            .selected(Some(1))
            .into();
        fn has_text(el: &Element, needle: &str) -> bool {
            el.text.as_deref().is_some_and(|t| t.contains(needle))
                || el.children.iter().any(|c| has_text(c, needle))
        }
        assert!(
            has_text(&el, "0.80") || has_text(&el, "0.8"),
            "selected+tooltip must emit datum label in the tree"
        );
    }

    #[test]
    fn tooltip_label_lives_inside_styled_percent_bubble() {
        use crate::core::tree::UnitSlot;
        use crate::layout::dimension::CssDim;
        let el: Element = line_chart()
            .values([0.2, 0.8, 0.4])
            .size(200.0, 80.0)
            .selected(Some(1))
            .into();
        fn find_bubble<'a>(el: &'a Element, needle: &str) -> Option<&'a Element> {
            fn contains_text(el: &Element, needle: &str) -> bool {
                el.text.as_deref().is_some_and(|t| t.contains(needle))
                    || el.children.iter().any(|c| contains_text(c, needle))
            }
            if el.style.bg_color[3] > 0.5
                && el.style.radii.iter().any(|r| *r > 1.0)
                && contains_text(el, needle)
            {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_bubble(c, needle))
        }
        let bubble = find_bubble(&el, "0.8").expect("tooltip bubble with value text");
        assert!(
            bubble.style.radii.iter().all(|r| *r > 4.0),
            "tooltip must keep CSS corner radii (no square path backing)"
        );
        let tracks_plot = bubble.style.unit_specs.iter().any(|(slot, dim)| {
            matches!(*slot, UnitSlot::InsetL | UnitSlot::InsetR)
                && matches!(
                    dim,
                    CssDim::Percent(_) | CssDim::CalcAdd(_, _) | CssDim::CalcSub(_, _)
                )
        });
        assert!(
            tracks_plot,
            "tooltip left/right must be % / calc(% ± px) so stretch cannot split box and text"
        );
        let below_pin = bubble.style.unit_specs.iter().any(|(slot, dim)| {
            *slot == UnitSlot::InsetT
                && matches!(
                    dim,
                    CssDim::Percent(_) | CssDim::CalcAdd(_, _) | CssDim::CalcSub(_, _)
                )
        });
        assert!(
            below_pin,
            "default tooltip must sit below the pin (`top: y% + gap`)"
        );
    }

    #[test]
    fn tooltip_pin_flips_near_plot_edges() {
        use super::datum::ChartTooltipItem;
        use super::define::tooltip_pin_place;
        let item = |label: &str, v: f32| ChartTooltipItem {
            label: label.into(),
            value: v,
            series_key: None,
            header: None,
        };
        assert!(
            !tooltip_pin_place(40.0, 40.0, 200.0, 80.0, &item("Tue", 0.40)).flip_x,
            "room on the right → keep tooltip to the right of the pin"
        );
        assert!(
            tooltip_pin_place(190.0, 40.0, 200.0, 80.0, &item("Fri", 0.90)).flip_x,
            "near right edge → flip to the left so overflow:hidden cannot clip"
        );
        assert!(
            !tooltip_pin_place(100.0, 6.0, 200.0, 80.0, &item("A", 1.00)).flip_y,
            "near top → keep tooltip below the pointer"
        );
        assert!(
            !tooltip_pin_place(100.0, 20.0, 200.0, 80.0, &item("A", 1.00)).flip_y,
            "enough room below → keep tooltip below the pointer"
        );
        assert!(
            tooltip_pin_place(100.0, 70.0, 200.0, 80.0, &item("A", 1.00)).flip_y,
            "near bottom → flip above so the bubble stays in plot"
        );
    }

    #[test]
    fn tooltip_near_right_edge_anchors_from_right() {
        use crate::core::tree::UnitSlot;
        use crate::layout::dimension::CssDim;
        let el: Element = bar_chart()
            .values([0.2, 0.4, 0.6, 0.8, 0.9])
            .labels(["Mon", "Tue", "Wed", "Thu", "Fri"])
            .selected(Some(4))
            .into();
        fn find_bubble<'a>(el: &'a Element, needle: &str) -> Option<&'a Element> {
            fn contains_text(el: &Element, needle: &str) -> bool {
                el.text.as_deref().is_some_and(|t| t.contains(needle))
                    || el.children.iter().any(|c| contains_text(c, needle))
            }
            if el.style.bg_color[3] > 0.5
                && el.style.radii.iter().any(|r| *r > 1.0)
                && contains_text(el, needle)
            {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_bubble(c, needle))
        }
        let bubble = find_bubble(&el, "Fri").expect("Fri tooltip bubble");
        let from_right = bubble.style.unit_specs.iter().any(|(slot, dim)| {
            *slot == UnitSlot::InsetR
                && matches!(
                    dim,
                    CssDim::Percent(_) | CssDim::CalcAdd(_, _) | CssDim::CalcSub(_, _)
                )
        });
        let from_left = bubble
            .style
            .unit_specs
            .iter()
            .any(|(slot, _)| *slot == UnitSlot::InsetL);
        assert!(
            from_right && !from_left,
            "last category must pin with `right`, not `left`"
        );
    }

    #[test]
    fn builder_axis_style_reaches_axis_slot() {
        let el: Element = define_chart()
            .mark(bar_y([
                ChartDatum::new("Mon", 1.0),
                ChartDatum::new("Tue", 2.0),
            ]))
            .axis_labels(true)
            .slot("chart-axis", |c| c.style(|b| b.text_size(20.0)))
            .into();
        fn find_axis_size(el: &Element, want: f32) -> bool {
            let is_axis = el.attrs.get("data-slot").map(String::as_str) == Some("chart-axis");
            (is_axis
                && (el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                    || (el.text_style.size_resolved() - want).abs() < 0.01))
                || el.children.iter().any(|c| find_axis_size(c, want))
        }
        assert!(
            find_axis_size(&el, 20.0),
            "chart .slot(chart-axis) style must land on data-slot=chart-axis"
        );
    }

    #[test]
    fn builder_legend_style_reaches_legend_slot() {
        let el: Element = define_chart()
            .mark(pie([ChartDatum::new("A", 1.0), ChartDatum::new("B", 2.0)]))
            .legend(true)
            .slot("chart-legend", |c| c.style(|b| b.text_size(20.0)))
            .into();
        fn find_legend_size(el: &Element, want: f32) -> bool {
            let is_legend = el.attrs.get("data-slot").map(String::as_str) == Some("chart-legend");
            (is_legend
                && (el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                    || (el.text_style.size_resolved() - want).abs() < 0.01))
                || el.children.iter().any(|c| find_legend_size(c, want))
        }
        assert!(
            find_legend_size(&el, 20.0),
            "chart .slot(chart-legend) style must land on data-slot=chart-legend"
        );
    }
}
