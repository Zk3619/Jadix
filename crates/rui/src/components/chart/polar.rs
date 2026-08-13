//! Polar 族 marks + guides（TanStack polar bar/line/area/dot 窄化）。
//! 值 → 半径；角按类别均分（pie 仍走 [`super::pie`]：值 → 角跨）。

use std::f32::consts::TAU;

use crate::primitives::path::{pie_ring, polyline, PathStyle, Point2};

use super::datum::{collect_rows, datum_hover_text, ChartDatum};
use super::mark::{
    dashed_segment, filled_dot_layers, paint_fill, paint_stroke, unique_labels, unique_series,
    ChartMark, MarkChannels, MarkRender, MarkRenderContext, SceneLayer,
};
use super::theme::{ChartPaint, MarkStyle};

macro_rules! style_setters {
    () => {
        pub fn id(mut self, id: impl Into<String>) -> Self {
            self.id = id.into();
            self
        }
        pub fn fill(mut self, paint: impl Into<ChartPaint>) -> Self {
            self.style.fill = Some(paint.into());
            self
        }
        pub fn stroke(mut self, paint: impl Into<ChartPaint>) -> Self {
            self.style.stroke = Some(paint.into());
            self
        }
        pub fn stroke_width(mut self, px: f32) -> Self {
            self.style.stroke_width = Some(px.max(0.5));
            self
        }
        pub fn fill_opacity(mut self, o: f32) -> Self {
            self.style.fill_opacity = Some(o.clamp(0.0, 1.0));
            self
        }
        pub fn stroke_opacity(mut self, o: f32) -> Self {
            self.style.stroke_opacity = Some(o.clamp(0.0, 1.0));
            self
        }
        pub fn stroke_dash(mut self, dash: impl IntoIterator<Item = f32>) -> Self {
            self.style = self.style.stroke_dash(dash);
            self
        }
    };
}

fn polar_labels(rows: &[ChartDatum]) -> Vec<String> {
    let labs = unique_labels(rows);
    if !labs.is_empty() {
        return labs;
    }
    (0..rows.len()).map(|i| format!("{}", i + 1)).collect()
}

fn inner_frac(mark: f32, ctx: &MarkRenderContext<'_>) -> f32 {
    mark.max(ctx.polar_inner).clamp(0.0, 0.85)
}

fn angle_in(ang: f32, a0: f32, a1: f32) -> bool {
    let span = a1 - a0;
    if span <= 0.0 {
        return false;
    }
    let mut rel = (ang - a0) % TAU;
    if rel < 0.0 {
        rel += TAU;
    }
    rel < span
}

fn polar_series_points(
    rows: &[ChartDatum],
    ctx: &MarkRenderContext<'_>,
    ser: &Option<String>,
    inner: f32,
) -> Vec<(usize, Point2)> {
    let series_rows: Vec<(usize, &ChartDatum)> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| &r.series == ser)
        .collect();
    let n = series_rows.len().max(1);
    series_rows
        .iter()
        .enumerate()
        .map(|(k, (di, r))| {
            let a = ctx
                .scales
                .polar_angle_center(k, n, ctx.polar_start, ctx.polar_end);
            let rad = ctx.scales.polar_radius(r.value.max(0.0), inner);
            (*di, ctx.scales.polar_xy(a, rad))
        })
        .collect()
}

/// Polar 同心环 + 辐条（非 pie 专用）。
pub(crate) fn polar_guide_layers(ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
    let mut layers = Vec::new();
    let cx = ctx.scales.polar_cx();
    let cy = ctx.scales.polar_cy();
    let rmax = ctx.scales.polar_r_max();
    let stroke = PathStyle::stroke(ctx.theme.grid, 1.0);
    let segs = 48usize;
    for (i, t) in [0.33f32, 0.66, 1.0].iter().enumerate() {
        let r = rmax * *t;
        let mut pts = Vec::with_capacity(segs + 1);
        for k in 0..=segs {
            let a = ctx.polar_start + (ctx.polar_end - ctx.polar_start) * (k as f32 / segs as f32);
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        layers.push(SceneLayer::polyline(
            format!("pring-{i}"),
            &pts,
            stroke.clone(),
        ));
    }
    let n = ctx.scales.x_cats.len().max(1);
    let hole = rmax * ctx.polar_inner.clamp(0.0, 0.85);
    for i in 0..n {
        let a = ctx
            .scales
            .polar_angle_center(i, n, ctx.polar_start, ctx.polar_end);
        let p0 = ctx.scales.polar_xy(a, hole);
        let p1 = ctx.scales.polar_xy(a, rmax);
        layers.push(SceneLayer::polyline(
            format!("spoke-{i}"),
            &[p0, p1],
            stroke.clone(),
        ));
    }
    layers
}

// ---------------------------------------------------------------------------
// radial_bar
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RadialBarMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    inner_radius: f32,
}

/// Polar 柱：角均分，值 → 半径（TanStack polar `bar`）。
pub fn radial_bar<I, T>(data: I) -> RadialBarMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    RadialBarMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        inner_radius: 0.15,
    }
}

impl RadialBarMark {
    style_setters!();
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
}

impl MarkRender for RadialBarMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "radialBar"
    }
    fn is_polar(&self) -> bool {
        true
    }
    fn channels(&self) -> MarkChannels {
        let labels = polar_labels(&self.rows);
        MarkChannels {
            x_cats: labels.clone(),
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            y_include_zero: true,
            polar_values: self.rows.iter().map(|r| r.value).collect(),
            polar_labels: labels,
            ..MarkChannels::default()
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let n = self.rows.len().max(1);
        let inner_f = inner_frac(self.inner_radius, ctx);
        let hole = ctx.scales.polar_r_max() * inner_f;
        let cx = ctx.scales.polar_cx();
        let cy = ctx.scales.polar_cy();
        let series = unique_series(&self.rows);
        let n_s = series.len().max(1);
        let mut layers = Vec::new();
        let mut selected_layer: Option<SceneLayer> = None;
        for (i, r) in self.rows.iter().enumerate() {
            let si = series.iter().position(|s| s == &r.series).unwrap_or(0);
            let (a0, a1) = ctx
                .scales
                .polar_angle_band(i, n, ctx.polar_start, ctx.polar_end, 0.04);
            let outer0 = ctx
                .scales
                .polar_radius(r.value.max(0.0), inner_f)
                .max(hole + 1.0);
            let selected = ctx.selected == Some(i);
            // 与 pie 相同：中心/内半径不动，选中只加大 outerRadius。
            let outer = if selected { outer0 + 4.0 } else { outer0 };
            let segs = ((a1 - a0).abs() * outer / 1.25).ceil().max(16.0) as usize;
            let fill = paint_fill(&self.style, ctx.theme, si, i, ctx.selected, 1.0);
            let cmds = pie_ring(cx, cy, outer, hole, a0, a1, segs);
            let style = if selected {
                PathStyle::fill_stroke(
                    fill,
                    super::theme::lerp_rgb(fill, ctx.theme.foreground, 0.32),
                    1.5,
                )
            } else {
                PathStyle::fill(fill)
            };
            let layer = SceneLayer::path(format!("radialBar-{i}-{n_s}"), cmds, style);
            if selected {
                selected_layer = Some(layer);
            } else {
                layers.push(layer);
            }
        }
        if let Some(layer) = selected_layer {
            layers.push(layer);
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let n = self.rows.len().max(1);
        let inner_f = inner_frac(self.inner_radius, ctx);
        let hole = ctx.scales.polar_r_max() * inner_f;
        let cx = ctx.scales.polar_cx();
        let cy = ctx.scales.polar_cy();
        let dx = local.0 - cx;
        let dy = local.1 - cy;
        let dist = dx.hypot(dy);
        let ang = dy.atan2(dx);
        for (i, r) in self.rows.iter().enumerate() {
            let (a0, a1) = ctx
                .scales
                .polar_angle_band(i, n, ctx.polar_start, ctx.polar_end, 0.04);
            let outer = ctx
                .scales
                .polar_radius(r.value.max(0.0), inner_f)
                .max(hole + 1.0);
            if dist + 4.0 >= hole && dist <= outer + 4.0 && angle_in(ang, a0, a1) {
                return Some(i);
            }
        }
        None
    }
    fn hover_text(&self, index: usize) -> Option<String> {
        datum_hover_text(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let n = self.rows.len().max(1);
        let r = self.rows.get(index)?;
        let inner_f = inner_frac(self.inner_radius, ctx);
        let hole = ctx.scales.polar_r_max() * inner_f;
        let a = ctx
            .scales
            .polar_angle_center(index, n, ctx.polar_start, ctx.polar_end);
        let outer = ctx
            .scales
            .polar_radius(r.value.max(0.0), inner_f)
            .max(hole + 1.0);
        Some(ctx.scales.polar_xy(a, (hole + outer) * 0.5))
    }
}

impl From<RadialBarMark> for ChartMark {
    fn from(m: RadialBarMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// radial_line / radial_area
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RadialLineMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    area: bool,
    inner_radius: f32,
    default_stroke: f32,
}

/// Polar 折线：角均分，值 → 半径（TanStack polar `line`）。
pub fn radial_line<I, T>(data: I) -> RadialLineMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    RadialLineMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        area: false,
        inner_radius: 0.15,
        default_stroke: 2.5,
    }
}

/// Polar 面积：折线 + 填到内半径（TanStack polar `area`）。
pub fn radial_area<I, T>(data: I) -> RadialLineMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    RadialLineMark {
        area: true,
        default_stroke: 2.0,
        ..radial_line(data)
    }
}

impl RadialLineMark {
    style_setters!();
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
}

impl MarkRender for RadialLineMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        if self.area {
            "radialArea"
        } else {
            "radialLine"
        }
    }
    fn is_polar(&self) -> bool {
        true
    }
    fn channels(&self) -> MarkChannels {
        let labels = polar_labels(&self.rows);
        MarkChannels {
            x_cats: labels.clone(),
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            y_include_zero: self.area,
            polar_values: self.rows.iter().map(|r| r.value).collect(),
            polar_labels: labels,
            ..MarkChannels::default()
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let hole = ctx.scales.polar_r_max() * inner_f;
        let series = unique_series(&self.rows);
        let sw = self.style.stroke_width.unwrap_or(self.default_stroke);
        let mut layers = Vec::new();
        for (si, ser) in series.iter().enumerate() {
            let indexed = polar_series_points(&self.rows, ctx, ser, inner_f);
            if indexed.is_empty() {
                continue;
            }
            let pts: Vec<Point2> = indexed.iter().map(|(_, p)| *p).collect();
            if self.area || self.style.fill.is_some() || self.style.fill_opacity.is_some() {
                let n = pts.len().max(1);
                let mut area_pts = pts.clone();
                for k in (0..n).rev() {
                    let a = ctx
                        .scales
                        .polar_angle_center(k, n, ctx.polar_start, ctx.polar_end);
                    area_pts.push(ctx.scales.polar_xy(a, hole));
                }
                let fill = paint_fill(&self.style, ctx.theme, si, 0, None, 0.4);
                layers.push(SceneLayer::polygon(
                    format!("radialArea-{si}"),
                    &area_pts,
                    PathStyle::fill(fill),
                ));
            }
            let stroke = paint_stroke(&self.style, ctx.theme, si, 0, None, 1.0);
            let cmds = if let Some(dash) = self.style.stroke_dash.as_deref() {
                let mut c = Vec::new();
                for w in pts.windows(2) {
                    c.extend(dashed_segment(w[0], w[1], dash));
                }
                c
            } else {
                polyline(&pts)
            };
            layers.push(SceneLayer::path(
                format!("radialLine-{si}"),
                cmds,
                PathStyle::stroke(stroke, sw),
            ));
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let series = unique_series(&self.rows);
        let mut best: Option<(f32, usize)> = None;
        // 无上限会抢走 radial_bar 的扇区命中（compose 逆序：dot → line → bar）。
        let max_d = (self.style.stroke_width.unwrap_or(self.default_stroke) * 4.0).max(14.0);
        for ser in &series {
            for (di, p) in polar_series_points(&self.rows, ctx, ser, inner_f) {
                let d = (p.0 - local.0).hypot(p.1 - local.1);
                if d <= max_d && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, di));
                }
            }
        }
        best.map(|(_, i)| i)
    }
    fn hover_text(&self, index: usize) -> Option<String> {
        datum_hover_text(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let series = unique_series(&self.rows);
        for ser in &series {
            for (di, p) in polar_series_points(&self.rows, ctx, ser, inner_f) {
                if di == index {
                    return Some(p);
                }
            }
        }
        None
    }
}

impl From<RadialLineMark> for ChartMark {
    fn from(m: RadialLineMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// radial_dot
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RadialDotMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    radius: f32,
    inner_radius: f32,
}

/// Polar 散点（TanStack polar `dot`）。
pub fn radial_dot<I, T>(data: I) -> RadialDotMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    RadialDotMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        radius: 4.0,
        inner_radius: 0.15,
    }
}

impl RadialDotMark {
    style_setters!();
    pub fn r(mut self, px: f32) -> Self {
        self.radius = px.max(1.0);
        self
    }
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
}

impl MarkRender for RadialDotMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "radialDot"
    }
    fn is_polar(&self) -> bool {
        true
    }
    fn channels(&self) -> MarkChannels {
        let labels = polar_labels(&self.rows);
        MarkChannels {
            x_cats: labels.clone(),
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            polar_values: self.rows.iter().map(|r| r.value).collect(),
            polar_labels: labels,
            ..MarkChannels::default()
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let series = unique_series(&self.rows);
        let mut layers = Vec::new();
        for (si, ser) in series.iter().enumerate() {
            let indexed = polar_series_points(&self.rows, ctx, ser, inner_f);
            if indexed.is_empty() {
                continue;
            }
            layers.extend(filled_dot_layers(
                &indexed,
                si,
                self.radius,
                &self.style,
                ctx,
                "radialDot",
            ));
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let series = unique_series(&self.rows);
        let mut best: Option<(f32, usize)> = None;
        let max_d = (self.radius + 4.0).max(8.0);
        for ser in &series {
            for (di, p) in polar_series_points(&self.rows, ctx, ser, inner_f) {
                let d = (p.0 - local.0).hypot(p.1 - local.1);
                if d <= max_d && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, di));
                }
            }
        }
        best.map(|(_, i)| i)
    }
    fn hover_text(&self, index: usize) -> Option<String> {
        datum_hover_text(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let inner_f = inner_frac(self.inner_radius, ctx);
        let series = unique_series(&self.rows);
        for ser in &series {
            for (di, p) in polar_series_points(&self.rows, ctx, ser, inner_f) {
                if di == index {
                    return Some(p);
                }
            }
        }
        None
    }
}

impl From<RadialDotMark> for ChartMark {
    fn from(m: RadialDotMark) -> Self {
        ChartMark::wrap(m)
    }
}
