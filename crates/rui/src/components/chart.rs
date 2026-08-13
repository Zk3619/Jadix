//! # Chart（experimental · WP-3.2）
//!
//! 对照 [TanStack Charts](https://github.com/TanStack/charts) 的 **grammar 心智**，不搬运行时：
//! - **Data as rows**：[`ChartDatum`]（label + value），各 mark 直接吃行，不强制 series 包一层。
//! - **Scales**：band（柱）/ linear（折线 Y）/ polar pie 角分配；像素 range 由叶尺寸决定。
//! - **Marks**：Bar · Line · Area · Pie/Donut（`inner_radius`）。
//! - **Guides**（可选）：笛卡尔网格 + 底轴，默认开，`.guides(false)` 关闭。
//! - **Renderer-neutral scene**：mark → [`PathCommand`] → 同一 path 核（CPU 覆盖率光栅贴图；tessellate 仅回退）。
//!
//! 双书写：Builder [`bar_chart`] / [`line_chart`] / [`area_chart`] / [`pie_chart`]；
//! RSX [`BarChart`] / [`LineChart`] / [`AreaChart`] / [`PieChart`]。
//!
//! 不做：`defineChart` / D3 / SVG DOM / 完整 polar 包 / 时间轴全家桶。
//!
//! **身份（WP-2.8）**：展示型不要求用户 `id`；空 / 省略 → 不写 `template_id`（I-1）。

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::core::tree::{CursorKind, Element, LayoutStatus};
use crate::primitives::path::{
    attach_path, encode_points, ensure_raster_texture, pie_ring, pie_sector, polygon, polyline,
    PathCommand, PathStyle, Point2, ATTR_PATH_MARKERS, ATTR_PATH_VIEWBOX,
};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

/// 一行观测（TanStack mark-local datum 的 RUI 窄化）。
#[derive(Clone, Debug, PartialEq)]
pub struct ChartDatum {
    pub label: String,
    pub value: f32,
}

impl ChartDatum {
    pub fn new(label: impl Into<String>, value: f32) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

/// RSX `selected={…}`：可传 `usize` 或 `Option<usize>`。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChartSelected(pub Option<usize>);

impl From<usize> for ChartSelected {
    fn from(i: usize) -> Self {
        Self(Some(i))
    }
}

impl From<Option<usize>> for ChartSelected {
    fn from(i: Option<usize>) -> Self {
        Self(i)
    }
}

/// RSX / Builder 选中回调。
#[derive(Clone)]
pub struct ChartSelect(pub Rc<dyn Fn(usize)>);

impl<F: Fn(usize) + 'static> From<F> for ChartSelect {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CartesianMark {
    Line,
    Area,
}

fn rows_from_values_labels(values: &[f32], labels: &[String]) -> Vec<ChartDatum> {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let label = labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("{}", i + 1));
            ChartDatum::new(label, v)
        })
        .collect()
}

fn max_value(rows: &[ChartDatum]) -> f32 {
    rows.iter()
        .map(|r| r.value)
        .fold(1.0f32, f32::max)
        .max(1e-6)
}

/// Linear scale：domain → range（TanStack compact `scaleLinear` 窄化）。
pub fn scale_linear(domain: (f32, f32), range: (f32, f32)) -> impl Fn(f32) -> f32 {
    let (d0, d1) = domain;
    let (r0, r1) = range;
    let span = (d1 - d0).abs().max(1e-6);
    let sign = if d1 >= d0 { 1.0 } else { -1.0 };
    move |v| r0 + (v - d0) * sign * (r1 - r0) / span
}

/// Band scale：n 个类别均分 range，返回 (bandwidth, x_for_index)。
/// `padding` ∈ [0, 1) 为类间距占 bandwidth 的比例（≈ TanStack `scaleBand().padding`）。
pub fn scale_band(n: usize, range: (f32, f32), padding: f32) -> (f32, impl Fn(usize) -> f32) {
    let n = n.max(1);
    let (r0, r1) = range;
    let total = (r1 - r0).max(0.0);
    let pad = padding.clamp(0.0, 0.9);
    let step = total / n as f32;
    let bw = (step * (1.0 - pad)).max(1.0);
    let gap = step - bw;
    (bw, move |i| r0 + i as f32 * step + gap * 0.5)
}

/// Polar pie 角分配：从 `start` 起按正值占比切区间（TanStack `@tanstack/charts/polar` `pie` 窄化）。
/// 返回 `(start_rad, end_rad)`，约定 0 = 正右、屏幕 y 向下时增大角顺时针。
pub fn pie_layout(values: &[f32], start: f32) -> Vec<(f32, f32)> {
    let sum = values.iter().map(|v| v.max(0.0)).sum::<f32>().max(1e-6);
    let mut a = start;
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        let span = (v.max(0.0) / sum) * TAU;
        out.push((a, a + span));
        a += span;
    }
    out
}

fn lerp_rgb(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        1.0,
    ]
}

/// 色板保持不透明（扫描线填充若降 alpha，SDF 接缝会再次变成竖纹）。
fn chart_color(i: usize, selected: Option<usize>) -> [f32; 4] {
    let c = tokens::chart(((i % 5) + 1) as u8);
    match selected {
        Some(sel) if sel != i => lerp_rgb(c, tokens::muted(), 0.55),
        _ => [c[0], c[1], c[2], 1.0],
    }
}

fn grid_line_color() -> [f32; 4] {
    lerp_rgb(tokens::muted(), tokens::border(), 0.5)
}

/// TanStack guides：笛卡尔网格 + 底轴线。默认开；`.guides(false)` 可关。
fn cartesian_frame(height: &str, guides: bool, plot: Element) -> Element {
    let mut frame: Element = r#box()
        .w("100%")
        .h(height)
        .min_h(height)
        .max_h(height)
        .flex_shrink(0.0)
        .overflow("hidden")
        .rounded("0.375rem")
        .relative(true)
        .into();
    frame = frame.bg(tokens::muted()).border(tokens::border(), 1.0);
    if guides {
        for t in [0.25f32, 0.5, 0.75] {
            let mut ln: Element = r#box()
                .w("100%")
                .h(1.0)
                .absolute(true)
                .left(0.0)
                .top(format!("{}%", t * 100.0))
                .pointer_events("none")
                .into();
            ln = ln.bg(grid_line_color());
            frame = frame.child(ln);
        }
        let mut base: Element = r#box()
            .w("100%")
            .h(1.0)
            .absolute(true)
            .left(0.0)
            .bottom(0.0)
            .pointer_events("none")
            .into();
        base = base.bg(tokens::border());
        frame = frame.child(base);
    }
    frame.child(plot)
}

fn caption(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 11.0,
            weight: 400,
            ..Default::default()
        },
    )
}

fn apply_optional_id(mut el: Element, id: &str) -> Element {
    if !id.trim().is_empty() {
        el.template_id = Some(id.to_string());
    }
    el
}

// ---------------------------------------------------------------------------
// Bar
// ---------------------------------------------------------------------------

pub struct BarChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    height: String,
    selected: Option<usize>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    /// 笛卡尔网格 / 底轴（TanStack guides；默认开）。
    guides: bool,
}

/// 展示型入口：默认无用户 id（WP-2.8）。
pub fn bar_chart() -> BarChartBuilder {
    BarChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        height: "8rem".into(),
        selected: None,
        on_select: None,
        guides: true,
    }
}

impl BarChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(|s| s.into()).collect();
        let values: Vec<f32> = self.rows.iter().map(|r| r.value).collect();
        self.rows = rows_from_values_labels(&values, &labels);
        self
    }
    pub fn h(mut self, h: impl Into<String>) -> Self {
        self.height = h.into();
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }
}

impl From<BarChartBuilder> for Element {
    fn from(builder: BarChartBuilder) -> Self {
        let BarChartBuilder {
            id,
            rows,
            height,
            selected,
            on_select,
            guides,
        } = builder;
        let max_v = max_value(&rows);
        // 图区与标签分行：图区有确定高度，柱高用 flex_grow 而不是 %。
        let mut plot: Element = hstack().w("100%").h("100%").gap("0.5rem").into();
        let mut labels: Element = hstack().w("100%").gap("0.5rem").into();
        for (i, datum) in rows.iter().enumerate() {
            let ratio = (datum.value / max_v).clamp(0.0, 1.0);
            let spacer: Element = r#box().w("100%").flex_grow((1.0 - ratio).max(0.001)).into();
            let mut bar: Element = r#box()
                .w("100%")
                .flex_grow(ratio.max(0.04))
                .min_h(4.0)
                .rounded("0.25rem")
                .into();
            bar = bar.bg(chart_color(i, selected));
            if on_select.is_some() {
                bar = bar.cursor_kind(CursorKind::Pointer);
            }
            if let Some(cb) = on_select.clone() {
                bar = bar.on_click(move || cb(i));
            }
            let col: Element = vstack()
                .flex_grow(1.0)
                .h("100%")
                .child(spacer)
                .child(bar)
                .into();
            plot = plot.child(col);
            labels = labels.child(
                vstack()
                    .flex_grow(1.0)
                    .items_center()
                    .child(caption(&datum.label)),
            );
        }
        let framed = cartesian_frame(height.as_str(), guides, plot);
        let root: Element = vstack()
            .w("100%")
            .flex_shrink(0.0)
            .gap("0.25rem")
            .child(framed)
            .child(labels)
            .into();
        apply_optional_id(root, &id)
    }
}

// ---------------------------------------------------------------------------
// Line
// ---------------------------------------------------------------------------

pub struct LineChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    /// `None` = 宽度 100%（viewBox 缩放）；`Some` = 固定逻辑 px。
    width: Option<f32>,
    height: f32,
    selected: Option<usize>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    guides: bool,
    /// 折线下面积填充（TanStack Line 默认可关；Area mark 强制开）。
    fill: bool,
    stroke_width: f32,
    mark: CartesianMark,
}

/// 展示型入口：默认无用户 id（WP-2.8）。默认铺满宽度。
pub fn line_chart() -> LineChartBuilder {
    LineChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        width: None,
        height: 140.0,
        selected: None,
        on_select: None,
        guides: true,
        fill: true,
        stroke_width: 3.0,
        mark: CartesianMark::Line,
    }
}

/// TanStack Area mark：更实的填充 + 描边。
pub fn area_chart() -> LineChartBuilder {
    LineChartBuilder {
        mark: CartesianMark::Area,
        fill: true,
        stroke_width: 2.0,
        ..line_chart()
    }
}

impl LineChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    /// 固定像素宽高。要铺满容器请用 [`Self::h`]。
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(w);
        self.height = h;
        self
    }
    /// 高度（px），宽度 100%。
    pub fn h(mut self, px: f32) -> Self {
        self.width = None;
        self.height = px;
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }
    /// 折线下是否铺面积（Area mark 忽略此开关，始终填充）。
    pub fn fill(mut self, on: bool) -> Self {
        self.fill = on;
        self
    }
    pub fn stroke_width(mut self, px: f32) -> Self {
        self.stroke_width = px.max(0.5);
        self
    }
}

fn line_points_norm(rows: &[ChartDatum], height: f32) -> Vec<Point2> {
    let max_v = max_value(rows);
    let y_of = scale_linear((0.0, max_v), (height, 0.0));
    let n = rows.len().max(1);
    rows.iter()
        .enumerate()
        .map(|(i, d)| {
            let x = if n == 1 {
                0.5
            } else {
                i as f32 / (n - 1) as f32
            };
            (x, y_of(d.value.max(0.0)))
        })
        .collect()
}

impl From<LineChartBuilder> for Element {
    fn from(builder: LineChartBuilder) -> Self {
        let LineChartBuilder {
            id,
            rows,
            width,
            height,
            selected,
            on_select,
            guides,
            fill: fill_on,
            stroke_width,
            mark,
        } = builder;
        let h = height.max(16.0);
        let pts = line_points_norm(&rows, h);
        let want_fill = fill_on || mark == CartesianMark::Area;
        let cmds = if want_fill {
            let mut area_pts = pts.clone();
            if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                area_pts.push((last.0, h));
                area_pts.push((first.0, h));
            }
            let mut cmds = polygon(&area_pts);
            cmds.extend(polyline(&pts));
            cmds
        } else {
            polyline(&pts)
        };

        let muted = tokens::muted();
        let stroke = tokens::chart(1);
        let fill_mix = if mark == CartesianMark::Area {
            0.42
        } else {
            0.72
        };
        let fill = lerp_rgb(stroke, muted, fill_mix);
        let style = if want_fill {
            PathStyle::fill_stroke(fill, stroke, stroke_width)
        } else {
            PathStyle::stroke(stroke, stroke_width)
        };
        let kind_attr = match mark {
            CartesianMark::Line => "line",
            CartesianMark::Area => "area",
        };

        // 选中点 / 命中层可以是 100% 子叶；**路径必须挂在确定高度的 frame 上**。
        // 滚动列里 h(100%)（含 absolute inset）会吃到内容高，viewBox 把折线拉到饼图视口带。
        let mut hit: Element = r#box().w("100%").h("100%").relative(true).into();
        if let Some(i) = selected {
            if let Some(&(t, y)) = pts.get(i) {
                let mut dot: Element = r#box()
                    .w(8.0)
                    .h(8.0)
                    .rounded("9999px")
                    .absolute(true)
                    .left(format!("calc({:.2}% - 4px)", t * 100.0))
                    .top(y - 4.0)
                    .into();
                dot = dot.bg(stroke).pointer_events_none();
                hit = hit.child(dot);
            }
        }

        let frame_h = format!("{h}px");
        let mut plot = cartesian_frame(frame_h.as_str(), guides, hit);
        plot = attach_path(plot, &cmds, &style);
        plot.set_attr(ATTR_PATH_VIEWBOX, format!("0 0 1 {h}"));
        if mark == CartesianMark::Line {
            plot.set_attr(ATTR_PATH_MARKERS, encode_points(&pts));
        }

        if on_select.is_some() {
            plot = plot.cursor_kind(CursorKind::Pointer);
        }
        let bounds = Rc::new(Cell::new([0.0_f32; 4]));
        if let Some(cb) = on_select.clone() {
            let pts_hit = pts.clone();
            let n = pts_hit.len();
            let bounds_pd = bounds.clone();
            plot = plot.on_pointer_down(move |e| {
                if e.phase == EventPhase::Capture || n == 0 {
                    return;
                }
                let b = bounds_pd.get();
                if b[2] <= 0.0 {
                    return;
                }
                let t = ((e.x - b[0]) / b[2]).clamp(0.0, 1.0);
                let mut best = 0usize;
                let mut best_d = f32::MAX;
                for (i, &(px, _)) in pts_hit.iter().enumerate() {
                    let d = (px - t).abs();
                    if d < best_d {
                        best_d = d;
                        best = i;
                    }
                }
                cb(best);
            });
        }
        let bounds_sync = bounds;
        plot = plot.reactive_sync(move |root, _| {
            bounds_sync.set(root.layout_bounds);
            LayoutStatus::Clean
        });

        if let Some(w) = width {
            plot = r#box().w(format!("{}px", w.max(16.0))).child(plot).into();
        }
        plot = plot.flex_shrink(0.0);
        plot.set_attr("rui_chart", kind_attr);
        apply_optional_id(plot, &id)
    }
}

// ---------------------------------------------------------------------------
// Pie
// ---------------------------------------------------------------------------

pub struct PieChartBuilder {
    id: String,
    rows: Vec<ChartDatum>,
    size: f32,
    selected: Option<usize>,
    on_select: Option<Rc<dyn Fn(usize)>>,
    /// 内径占外径比例，0 = 实心饼，>0 = donut（TanStack polar innerRadius）。
    inner_radius: f32,
}

/// 展示型入口：默认无用户 id（WP-2.8）。
pub fn pie_chart() -> PieChartBuilder {
    PieChartBuilder {
        id: String::new(),
        rows: Vec::new(),
        size: 180.0,
        selected: None,
        on_select: None,
        inner_radius: 0.0,
    }
}

impl PieChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn data(mut self, rows: impl IntoIterator<Item = ChartDatum>) -> Self {
        self.rows = rows.into_iter().collect();
        self
    }
    pub fn values(mut self, v: impl IntoIterator<Item = f32>) -> Self {
        let values: Vec<f32> = v.into_iter().collect();
        self.rows = rows_from_values_labels(&values, &[]);
        self
    }
    pub fn labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(|s| s.into()).collect();
        let values: Vec<f32> = self.rows.iter().map(|r| r.value).collect();
        self.rows = rows_from_values_labels(&values, &labels);
        self
    }
    pub fn size(mut self, px: f32) -> Self {
        self.size = px;
        self
    }
    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }
    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    /// `0.0` = 实心；`0.55` 左右为常见 donut。
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
}

impl From<PieChartBuilder> for Element {
    fn from(builder: PieChartBuilder) -> Self {
        let PieChartBuilder {
            id,
            rows,
            size,
            selected,
            on_select,
            inner_radius,
        } = builder;
        let s = size.max(48.0);
        let cx = s * 0.5;
        let cy = s * 0.5;
        let r = s * 0.42;
        let inner = r * inner_radius.clamp(0.0, 0.85);
        let values: Vec<f32> = rows.iter().map(|d| d.value).collect();
        // 12 o'clock start（常见 pie）；path 核 0 = 正右。
        let angles = pie_layout(&values, -FRAC_PI_2);

        let mut layers: Vec<(Vec<PathCommand>, PathStyle)> = Vec::with_capacity(angles.len());
        for (i, &(a0, a1)) in angles.iter().enumerate() {
            if (a1 - a0).abs() < 1e-5 {
                continue;
            }
            let arc_px = (a1 - a0).abs() * r;
            let segs = (arc_px / 1.25).ceil().max(24.0) as usize;
            let cmds = if inner >= 1.0 {
                pie_ring(cx, cy, r, inner, a0, a1, segs)
            } else {
                pie_sector(cx, cy, r, a0, a1, segs)
            };
            layers.push((cmds, PathStyle::fill(chart_color(i, selected))));
        }
        let dim = s.round().max(48.0) as u32;
        let mut plot: Element = r#box()
            .w(format!("{s}px"))
            .h(format!("{s}px"))
            .min_w(format!("{s}px"))
            .min_h(format!("{s}px"))
            .max_w(format!("{s}px"))
            .max_h(format!("{s}px"))
            .flex_shrink(0.0)
            .overflow("hidden")
            .relative(true)
            .into();
        if let Some(tex) = ensure_raster_texture(&layers, &[], 0.0, dim, dim, 2) {
            plot = plot.image_texture(tex);
        } else {
            for (cmds, style) in &layers {
                let mut slice = Element::new()
                    .w("100%")
                    .h("100%")
                    .absolute()
                    .left(0.0)
                    .top(0.0)
                    .pointer_events_none();
                slice = attach_path(slice, cmds, style);
                plot = plot.child(slice);
            }
        }

        if on_select.is_some() {
            plot = plot.cursor_kind(CursorKind::Pointer);
        }
        let bounds = Rc::new(Cell::new([0.0_f32; 4]));
        if let Some(cb) = on_select.clone() {
            let angles_hit = angles.clone();
            let bounds_pd = bounds.clone();
            let inner_frac = inner_radius.clamp(0.0, 0.85);
            plot = plot.on_pointer_down(move |e| {
                if e.phase == EventPhase::Capture || angles_hit.is_empty() {
                    return;
                }
                let b = bounds_pd.get();
                if b[2] <= 0.0 || b[3] <= 0.0 {
                    return;
                }
                let dx = e.x - (b[0] + b[2] * 0.5);
                let dy = e.y - (b[1] + b[3] * 0.5);
                let dist = dx.hypot(dy);
                let radius = b[2].min(b[3]) * 0.42;
                let hole = (radius * inner_frac).max(1.0);
                if dist > radius + 4.0 || dist < hole {
                    return;
                }
                let ang = dy.atan2(dx);
                for (i, &(a0, a1)) in angles_hit.iter().enumerate() {
                    let span = a1 - a0;
                    if span <= 0.0 {
                        continue;
                    }
                    let mut rel = (ang - a0) % TAU;
                    if rel < 0.0 {
                        rel += TAU;
                    }
                    if rel < span {
                        cb(i);
                        return;
                    }
                }
            });
        }
        let bounds_sync = bounds;
        plot = plot.reactive_sync(move |root, _| {
            bounds_sync.set(root.layout_bounds);
            LayoutStatus::Clean
        });
        plot.set_attr("rui_chart", "pie");

        let mut legend = vstack().gap("0.25rem").flex_shrink(0.0);
        for (i, datum) in rows.iter().enumerate() {
            let mut swatch: Element = r#box()
                .w(10.0)
                .h(10.0)
                .min_w(10.0)
                .min_h(10.0)
                .rounded("0.125rem")
                .into();
            swatch = swatch.bg(chart_color(i, selected)).flex_shrink(0.0);
            let label = caption(&format!("{}  {:.0}", datum.label, datum.value));
            let row: Element = hstack()
                .gap("0.375rem")
                .items_center()
                .flex_shrink(0.0)
                .min_h(14.0)
                .child(swatch)
                .child(label)
                .into();
            legend = legend.child(row);
        }

        let root: Element = hstack()
            .gap("0.75rem")
            .items_center()
            .flex_shrink(0.0)
            .min_h(s)
            .child(plot)
            .child(legend)
            .into();
        apply_optional_id(root, &id)
    }
}

// ---------------------------------------------------------------------------
// RSX 标签（ADR-9 双书写；展示型 id 可省略）
// ---------------------------------------------------------------------------

fn chart_from_rows(
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
) -> Vec<ChartDatum> {
    if let Some(rows) = data {
        return rows;
    }
    let values = values.unwrap_or_default();
    let labels = labels.unwrap_or_default();
    rows_from_values_labels(&values, &labels)
}

/// RSX `<BarChart data={…} h="6rem" />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn BarChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    h: Option<String>,
    guides: Option<bool>,
    selected: Option<ChartSelected>,
    on_select: Option<ChartSelect>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = bar_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    b.into()
}

/// RSX `<LineChart values={…} h={140.0} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn LineChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    h: Option<f32>,
    guides: Option<bool>,
    fill: Option<bool>,
    stroke_width: Option<f32>,
    selected: Option<ChartSelected>,
    on_select: Option<ChartSelect>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = line_chart()
        .id(id)
        .data(chart_from_rows(data, values, None));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(f) = fill {
        b = b.fill(f);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    b.into()
}

/// RSX `<AreaChart values={…} h={140.0} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AreaChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    h: Option<f32>,
    guides: Option<bool>,
    stroke_width: Option<f32>,
    selected: Option<ChartSelected>,
    on_select: Option<ChartSelect>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = area_chart()
        .id(id)
        .data(chart_from_rows(data, values, None));
    if let Some(h) = h {
        b = b.h(h);
    }
    if let Some(g) = guides {
        b = b.guides(g);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    b.into()
}

/// RSX `<PieChart data={…} inner_radius={0.55} />`
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PieChart(
    id: String,
    data: Option<Vec<ChartDatum>>,
    values: Option<Vec<f32>>,
    labels: Option<Vec<String>>,
    size: Option<f32>,
    inner_radius: Option<f32>,
    selected: Option<ChartSelected>,
    on_select: Option<ChartSelect>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = pie_chart()
        .id(id)
        .data(chart_from_rows(data, values, labels));
    if let Some(s) = size {
        b = b.size(s);
    }
    if let Some(r) = inner_radius {
        b = b.inner_radius(r);
    }
    if let Some(ChartSelected(i)) = selected {
        b = b.selected(i);
    }
    if let Some(ChartSelect(f)) = on_select {
        b = b.on_select(move |i| f(i));
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::path::{
        commands_from_attrs, rasterize_path_rgba, tessellate_path, ATTR_PATH_D, ATTR_PATH_MARKERS,
    };

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
    fn pie_layout_covers_full_turn() {
        let spans = pie_layout(&[1.0, 1.0, 2.0], -FRAC_PI_2);
        assert_eq!(spans.len(), 3);
        let total: f32 = spans.iter().map(|(a, b)| b - a).sum();
        assert!((total - TAU).abs() < 1e-4, "got {total}");
        assert!((spans[0].0 + FRAC_PI_2).abs() < 1e-4);
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
        assert!(path_leaf.get_attr(ATTR_PATH_MARKERS).is_some());
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
            .find(|c| c.get_attr("rui_chart").map(|s| s.as_str()) == Some("pie"))
            .expect("pie plot");
        let self_tex = plot
            .image_texture
            .as_deref()
            .is_some_and(|k| k.starts_with("rui_path:"));
        let child_tex = plot.children.iter().any(|c| {
            c.image_texture
                .as_deref()
                .is_some_and(|k| k.starts_with("rui_path:"))
        });
        let slices_with_d = plot
            .children
            .iter()
            .filter(|c| c.get_attr(ATTR_PATH_D).is_some())
            .count();
        assert!(
            self_tex || child_tex || slices_with_d >= 3,
            "pie must raster via path kernel (texture) or keep slice d attrs; self_tex={self_tex} d={slices_with_d}"
        );
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
        assert_eq!(el.get_attr("rui_chart").map(|s| s.as_str()), Some("area"));
        assert!(find_attr(&el, ATTR_PATH_D).is_some());
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
            .find(|c| c.get_attr("rui_chart").map(|s| s.as_str()) == Some("pie"))
            .expect("pie plot");
        assert!(
            plot.image_texture
                .as_deref()
                .is_some_and(|k| k.starts_with("rui_path:")),
            "donut must still raster via path kernel"
        );
    }
}
