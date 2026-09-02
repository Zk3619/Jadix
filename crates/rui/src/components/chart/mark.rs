//! Marks: Geometry layer. Built-in Bar/Line/Area/Pie/Dot/Rule + [`custom_mark`] (TanStack `createMark` narrowed).

use std::f32::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use crate::primitives::path::{
    PathCommand, PathStyle, Point2, filled_circle, pie_ring, pie_sector, polygon, polyline,
    rounded_rect,
};

use super::datum::{
    ChartDatum, ChartTooltipItem, collect_rows, datum_hover_item, datum_hover_text,
};
use super::scale::{ResolvedScales, pie_layout_span};
use super::theme::{ChartPaint, ChartTheme, MarkStyle};

/// The channel value contributed by Mark to scale (before compilation).
#[derive(Clone, Debug, Default)]
pub struct MarkChannels {
    pub x_cats: Vec<String>,
    pub x_nums: Vec<f32>,
    pub x_times: Vec<i64>,
    pub y_nums: Vec<f32>,
    pub y_include_zero: bool,
    pub polar_values: Vec<f32>,
    pub polar_labels: Vec<String>,
}

/// 渲染器无关场景层 → 同一 path 核。
#[derive(Clone, Debug)]
pub struct SceneLayer {
    pub key: String,
    pub commands: Vec<PathCommand>,
    pub style: PathStyle,
    pub markers: Vec<Point2>,
}

impl SceneLayer {
    pub fn path(key: impl Into<String>, commands: Vec<PathCommand>, style: PathStyle) -> Self {
        Self {
            key: key.into(),
            commands,
            style,
            markers: Vec::new(),
        }
    }

    pub fn polyline(key: impl Into<String>, pts: &[Point2], style: PathStyle) -> Self {
        Self::path(key, polyline(pts), style)
    }

    pub fn polygon(key: impl Into<String>, pts: &[Point2], style: PathStyle) -> Self {
        Self::path(key, polygon(pts), style)
    }

    pub fn markers(key: impl Into<String>, pts: Vec<Point2>, style: PathStyle) -> Self {
        Self {
            key: key.into(),
            commands: Vec::new(),
            style,
            markers: pts,
        }
    }
}

/// Mark `render` / `hit` context (compare TanStack `MarkRenderContext`).
pub struct MarkRenderContext<'a> {
    pub scales: &'a ResolvedScales,
    pub theme: &'a ChartTheme,
    pub selected: Option<usize>,
    /// Polar circumscribes the side length of the square (Cartesian plot equals plot height).
    pub polar_size: f32,
    pub polar_start: f32,
    pub polar_end: f32,
    pub polar_inner: f32,
    /// 无底轴时 bar 基线端默认同值端圆角。
    pub round_bar_base: bool,
}

impl<'a> MarkRenderContext<'a> {
    pub fn new(
        scales: &'a ResolvedScales,
        theme: &'a ChartTheme,
        selected: Option<usize>,
        polar_size: f32,
    ) -> Self {
        Self {
            scales,
            theme,
            selected,
            polar_size,
            polar_start: -FRAC_PI_2,
            polar_end: -FRAC_PI_2 + TAU,
            polar_inner: 0.15,
            round_bar_base: false,
        }
    }
}

pub(crate) trait MarkRender {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;
    fn channels(&self) -> MarkChannels;
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer>;
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize>;
    fn is_polar(&self) -> bool {
        false
    }
    fn hover_text(&self, _index: usize) -> Option<String> {
        None
    }
    fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        self.hover_text(index).map(|t| {
            if let Some((label, value)) = t.rsplit_once("  ") {
                let v = value.trim().parse::<f32>().unwrap_or(0.0);
                ChartTooltipItem {
                    label: label.to_string(),
                    value: v,
                    series_key: None,
                    header: None,
                }
            } else {
                ChartTooltipItem {
                    label: t,
                    value: 0.0,
                    series_key: None,
                    header: None,
                }
            }
        })
    }
    fn datum_point(&self, _ctx: &MarkRenderContext<'_>, _index: usize) -> Option<Point2> {
        None
    }
}

/// A layer of mark for type erasure (can enter [`super::define_chart`]).
#[derive(Clone)]
pub struct ChartMark {
    inner: Rc<dyn MarkRender>,
}

impl ChartMark {
    pub(crate) fn wrap(inner: impl MarkRender + 'static) -> Self {
        Self {
            inner: Rc::new(inner),
        }
    }
    pub fn id(&self) -> &str {
        self.inner.id()
    }
    pub fn kind(&self) -> &'static str {
        self.inner.kind()
    }
    pub fn channels(&self) -> MarkChannels {
        self.inner.channels()
    }
    pub fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        self.inner.render(ctx)
    }
    pub fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        self.inner.hit_index(ctx, local)
    }
    pub fn is_polar(&self) -> bool {
        self.inner.is_polar()
    }
    pub fn hover_text(&self, index: usize) -> Option<String> {
        self.inner.hover_text(index)
    }
    pub fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        self.inner.hover_item(index)
    }
    pub fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        self.inner.datum_point(ctx, index)
    }
}

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
        pub fn corner_radius(mut self, px: f32) -> Self {
            self.style.corner_radius = Some(px.max(0.0));
            self
        }
        pub fn corner_radius_base(mut self, px: f32) -> Self {
            self.style.corner_radius_base = Some(px.max(0.0));
            self
        }
    };
}

pub(crate) fn labels_are_indexish(rows: &[ChartDatum]) -> bool {
    rows.iter()
        .all(|r| r.label.is_empty() || r.label.chars().all(|c| c.is_ascii_digit()))
        && rows.iter().any(|r| r.label.is_empty())
}

pub(crate) fn unique_labels(rows: &[ChartDatum]) -> Vec<String> {
    let mut out = Vec::new();
    for r in rows {
        let l = if r.label.is_empty() {
            continue;
        } else {
            r.label.clone()
        };
        if !out.iter().any(|x| x == &l) {
            out.push(l);
        }
    }
    out
}

pub(crate) fn unique_series(rows: &[ChartDatum]) -> Vec<Option<String>> {
    let mut out = Vec::new();
    for r in rows {
        if !out.iter().any(|s| s == &r.series) {
            out.push(r.series.clone());
        }
    }
    if out.is_empty() {
        out.push(None);
    }
    out
}

fn value_for(rows: &[ChartDatum], label: &str, series: &Option<String>) -> Option<(usize, f32)> {
    rows.iter().enumerate().find_map(|(i, r)| {
        if r.label == label && &r.series == series {
            Some((i, r.value))
        } else {
            None
        }
    })
}

pub(crate) fn paint_fill(
    style: &MarkStyle,
    theme: &ChartTheme,
    series_i: usize,
    datum_i: usize,
    selected: Option<usize>,
    default_opacity: f32,
) -> [f32; 4] {
    let paint = style.fill.clone().unwrap_or(ChartPaint::Series);
    let mut c = theme.resolve(&paint, series_i);
    let op = style.fill_opacity.unwrap_or(default_opacity);
    c = theme.mix_opacity(c, op, theme.muted);
    theme.dim_if_unselected(c, datum_i, selected)
}

pub(crate) fn paint_stroke(
    style: &MarkStyle,
    theme: &ChartTheme,
    series_i: usize,
    datum_i: usize,
    selected: Option<usize>,
    default_opacity: f32,
) -> [f32; 4] {
    let paint = style.stroke.clone().unwrap_or(ChartPaint::Series);
    let mut c = theme.resolve(&paint, series_i);
    let op = style.stroke_opacity.unwrap_or(default_opacity);
    c = theme.mix_opacity(c, op, theme.muted);
    theme.dim_if_unselected(c, datum_i, selected)
}

/// Each datum is a circle, drawn on the polyline. Select Enlarge + Light Stroke; **Do not** press `di0` to make the whole group lighter.
pub(crate) fn filled_dot_layers(
    indexed: &[(usize, Point2)],
    series_i: usize,
    radius: f32,
    style: &MarkStyle,
    ctx: &MarkRenderContext<'_>,
    key_prefix: &str,
) -> Vec<SceneLayer> {
    let mut unsel = Vec::new();
    let mut sel = None;
    for &(di, p) in indexed {
        let selected = ctx.selected == Some(di);
        let r = if selected { radius + 1.0 } else { radius };
        let fill = paint_fill(style, ctx.theme, series_i, di, None, 1.0);
        let ring_w = if selected { 1.25 } else { 1.0 };
        let layer = SceneLayer::path(
            format!("{key_prefix}-{series_i}-{di}"),
            filled_circle(p.0, p.1, r, 20),
            PathStyle::fill_stroke(fill, ctx.theme.background, ring_w),
        );
        if selected {
            sel = Some(layer);
        } else {
            unsel.push(layer);
        }
    }
    unsel.extend(sel);
    unsel
}

pub(crate) fn dashed_segment(a: Point2, b: Point2, dash: &[f32]) -> Vec<PathCommand> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    if len < 1e-4 || dash.is_empty() {
        return polyline(&[a, b]);
    }
    let ux = dx / len;
    let uy = dy / len;
    let mut dist = 0.0;
    let mut on = true;
    let mut di = 0usize;
    let mut cmds = Vec::new();
    while dist < len - 1e-4 {
        let seg = dash[di % dash.len()].max(0.5);
        let next = (dist + seg).min(len);
        if on {
            cmds.push(PathCommand::MoveTo((a.0 + ux * dist, a.1 + uy * dist)));
            cmds.push(PathCommand::LineTo((a.0 + ux * next, a.1 + uy * next)));
        }
        dist = next;
        on = !on;
        di += 1;
    }
    cmds
}

fn stroke_path(a: Point2, b: Point2, style: &MarkStyle) -> Vec<PathCommand> {
    if let Some(dash) = style.stroke_dash.as_deref() {
        dashed_segment(a, b, dash)
    } else {
        polyline(&[a, b])
    }
}

// ---------------------------------------------------------------------------
// BarY / BarX
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct BarYMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    horizontal: bool,
}

/// Vertical column (TanStack `barY`). `series` → Group side by side within a class.
pub fn bar_y<I, T>(data: I) -> BarYMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    BarYMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        horizontal: false,
    }
}

/// Horizontal bar (TanStack `barX`).
pub fn bar_x<I, T>(data: I) -> BarYMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    BarYMark {
        horizontal: true,
        ..bar_y(data)
    }
}

impl BarYMark {
    style_setters!();
}

impl MarkRender for BarYMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        if self.horizontal { "barX" } else { "barY" }
    }
    fn channels(&self) -> MarkChannels {
        MarkChannels {
            x_cats: unique_labels(&self.rows),
            x_nums: Vec::new(),
            x_times: Vec::new(),
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            y_include_zero: true,
            polar_values: Vec::new(),
            polar_labels: Vec::new(),
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let labels = if ctx.scales.x_cats.is_empty() {
            unique_labels(&self.rows)
        } else {
            ctx.scales.x_cats.clone()
        };
        let series = unique_series(&self.rows);
        let n_s = series.len().max(1);
        let gap = 2.0;
        let mut layers = Vec::new();
        for (xi, lab) in labels.iter().enumerate() {
            let band = if self.horizontal {
                ctx.scales.bandwidth.max(1.0)
            } else {
                ctx.scales.bandwidth.max(1.0)
            };
            let slot = ((band - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
            for (si, ser) in series.iter().enumerate() {
                let Some((di, val)) = value_for(&self.rows, lab, ser).or_else(|| {
                    if n_s == 1 {
                        self.rows.get(xi).map(|r| (xi, r.value))
                    } else {
                        None
                    }
                }) else {
                    continue;
                };
                let fill = paint_fill(&self.style, ctx.theme, si, di, ctx.selected, 1.0);
                let r_value = self.style.corner_radius.unwrap_or(4.0);
                let r_base = self
                    .style
                    .corner_radius_base
                    .unwrap_or(if ctx.round_bar_base { r_value } else { 0.0 });
                if self.horizontal {
                    let band_y = ctx.scales.bandwidth_y().max(1.0);
                    let slot_y =
                        ((band_y - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
                    let y0 = ctx.scales.y_index(xi) + si as f32 * (slot_y + gap);
                    let x0 = ctx.scales.x_value0();
                    let x1 = ctx.scales.x_value(val.max(0.0));
                    let (l, r) = if x1 >= x0 { (x0, x1) } else { (x1, x0) };
                    let radii = if x1 >= x0 {
                        [r_base, r_value, r_value, r_base]
                    } else {
                        [r_value, r_base, r_base, r_value]
                    };
                    let cmds = rounded_rect(l, y0, (r - l).max(1.0), slot_y, radii);
                    layers.push(SceneLayer::path(
                        format!("barX-{xi}-{si}"),
                        cmds,
                        PathStyle::fill(fill),
                    ));
                } else {
                    let x0 = ctx.scales.x_index(xi) + si as f32 * (slot + gap);
                    let y1 = ctx.scales.y(val.max(0.0));
                    let y0 = ctx.scales.y0();
                    let (top, bot) = if y1 <= y0 { (y1, y0) } else { (y0, y1) };
                    // +1px: The half-open range of the raster will cut off the last line of y==plot_h, and the bottom of the column will be a gap away from the axis.
                    let cmds = rounded_rect(
                        x0,
                        top,
                        slot,
                        (bot - top + 1.0).max(0.5),
                        [r_value, r_value, r_base, r_base],
                    );
                    layers.push(SceneLayer::path(
                        format!("barY-{xi}-{si}"),
                        cmds,
                        PathStyle::fill(fill),
                    ));
                }
            }
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let labels = if ctx.scales.x_cats.is_empty() {
            unique_labels(&self.rows)
        } else {
            ctx.scales.x_cats.clone()
        };
        let series = unique_series(&self.rows);
        let n_s = series.len().max(1);
        let gap = 2.0;
        let band = ctx.scales.bandwidth.max(1.0);
        let slot = ((band - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
        for (xi, lab) in labels.iter().enumerate() {
            for (si, ser) in series.iter().enumerate() {
                let Some((di, val)) = value_for(&self.rows, lab, ser).or_else(|| {
                    if n_s == 1 {
                        self.rows.get(xi).map(|r| (xi, r.value))
                    } else {
                        None
                    }
                }) else {
                    continue;
                };
                if self.horizontal {
                    let band_y = ctx.scales.bandwidth_y().max(1.0);
                    let slot_y =
                        ((band_y - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
                    let y0 = ctx.scales.y_index(xi) + si as f32 * (slot_y + gap);
                    let x0 = ctx.scales.x_value0();
                    let x1 = ctx.scales.x_value(val.max(0.0));
                    let (l, r) = if x1 >= x0 { (x0, x1) } else { (x1, x0) };
                    if local.0 >= l && local.0 <= r && local.1 >= y0 && local.1 <= y0 + slot_y {
                        return Some(di);
                    }
                } else {
                    let x0 = ctx.scales.x_index(xi) + si as f32 * (slot + gap);
                    let y1 = ctx.scales.y(val.max(0.0));
                    let y0 = ctx.scales.y0();
                    let (top, bot) = if y1 <= y0 { (y1, y0) } else { (y0, y1) };
                    if local.0 + 3.0 >= x0
                        && local.0 <= x0 + slot + 3.0
                        && local.1 + 3.0 >= top
                        && local.1 <= bot + 3.0
                    {
                        return Some(di);
                    }
                }
            }
        }
        None
    }
    fn hover_text(&self, index: usize) -> Option<String> {
        datum_hover_text(&self.rows, index)
    }
    fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        datum_hover_item(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let r = self.rows.get(index)?;
        let labels = if ctx.scales.x_cats.is_empty() {
            unique_labels(&self.rows)
        } else {
            ctx.scales.x_cats.clone()
        };
        let series = unique_series(&self.rows);
        let n_s = series.len().max(1);
        let gap = 2.0;
        let si = series.iter().position(|s| s == &r.series).unwrap_or(0);
        let xi = labels.iter().position(|l| l == &r.label).unwrap_or(index);
        if self.horizontal {
            let band_y = ctx.scales.bandwidth_y().max(1.0);
            let slot_y = ((band_y - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
            let y0 = ctx.scales.y_index(xi) + si as f32 * (slot_y + gap);
            let x0 = ctx.scales.x_value0();
            let x1 = ctx.scales.x_value(r.value.max(0.0));
            Some(((x0 + x1) * 0.5, y0 + slot_y * 0.5))
        } else {
            let band = ctx.scales.bandwidth.max(1.0);
            let slot = ((band - gap * (n_s.saturating_sub(1)) as f32) / n_s as f32).max(1.0);
            let x0 = ctx.scales.x_index(xi) + si as f32 * (slot + gap);
            let y1 = ctx.scales.y(r.value.max(0.0));
            let y0 = ctx.scales.y0();
            Some((x0 + slot * 0.5, (y0 + y1) * 0.5))
        }
    }
}

impl From<BarYMark> for ChartMark {
    fn from(m: BarYMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// LineY / AreaY
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LineYMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    area: bool,
    /// The paving area under the polyline, but kind is still `line` (different from Area mark).
    fill_under: bool,
    default_stroke: f32,
}

/// Polyline (TanStack `lineY`). `series` → multiple independent polylines.
pub fn line_y<I, T>(data: I) -> LineYMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    LineYMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        area: false,
        fill_under: false,
        default_stroke: 3.0,
    }
}

/// Area (TanStack `areaY`): Baseline 0 + Stroke.
pub fn area_y<I, T>(data: I) -> LineYMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    LineYMark {
        area: true,
        default_stroke: 2.0,
        ..line_y(data)
    }
}

impl LineYMark {
    style_setters!();
    /// 折线下是否铺面积（`area_y` 忽略此开关，始终填充）。
    pub fn fill_under(mut self, on: bool) -> Self {
        self.fill_under = on;
        self
    }
}

fn line_points(
    rows: &[ChartDatum],
    ctx: &MarkRenderContext<'_>,
    ser: &Option<String>,
) -> Vec<(usize, Point2)> {
    let mut pts = Vec::new();
    let use_time =
        ctx.scales.x_kind == super::scale::ScaleKind::Time && rows.iter().any(|r| r.time.is_some());
    let use_cat = !use_time && !ctx.scales.x_cats.is_empty() && !labels_are_indexish(rows);
    if use_cat {
        for (xi, lab) in ctx.scales.x_cats.iter().enumerate() {
            if let Some((di, val)) = value_for(rows, lab, ser) {
                pts.push((di, (ctx.scales.x_center(xi), ctx.scales.y(val))));
            }
        }
    } else {
        let series_rows: Vec<(usize, &ChartDatum)> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| &r.series == ser)
            .collect();
        let n = series_rows.len().max(1);
        for (k, (di, r)) in series_rows.iter().enumerate() {
            let x = if use_time {
                ctx.scales.x_time(r.time.unwrap_or(0))
            } else if matches!(
                ctx.scales.x_kind,
                super::scale::ScaleKind::Linear | super::scale::ScaleKind::Log
            ) && ctx.scales.x_cats.is_empty()
            {
                ctx.scales.x_num(k as f32)
            } else if n == 1 {
                ctx.scales.x_center(0)
            } else {
                let t = k as f32 / (n - 1) as f32;
                ctx.scales.pad[0] + t * ctx.scales.inner_w()
            };
            pts.push((*di, (x, ctx.scales.y(r.value.max(0.0)))));
        }
    }
    pts
}

impl MarkRender for LineYMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        if self.area { "area" } else { "line" }
    }
    fn channels(&self) -> MarkChannels {
        let times: Vec<i64> = self.rows.iter().filter_map(|r| r.time).collect();
        let cats = if !times.is_empty() {
            Vec::new()
        } else if labels_are_indexish(&self.rows) {
            Vec::new()
        } else {
            unique_labels(&self.rows)
        };
        let x_nums = if cats.is_empty() && times.is_empty() {
            (0..self.rows.len()).map(|i| i as f32).collect()
        } else {
            Vec::new()
        };
        MarkChannels {
            x_cats: cats,
            x_nums,
            x_times: times,
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            y_include_zero: self.area,
            polar_values: Vec::new(),
            polar_labels: Vec::new(),
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let series = unique_series(&self.rows);
        let want_fill = self.area
            || self.fill_under
            || self.style.fill.is_some()
            || self.style.fill_opacity.is_some();
        let sw = self.style.stroke_width.unwrap_or(self.default_stroke);
        let fill_op = if self.area { 0.58 } else { 0.28 };
        let mut layers = Vec::new();
        for (si, ser) in series.iter().enumerate() {
            let indexed = line_points(&self.rows, ctx, ser);
            if indexed.is_empty() {
                continue;
            }
            let pts: Vec<Point2> = indexed.iter().map(|(_, p)| *p).collect();
            // The polyline/area is the entire series. You cannot use datum[0] as the selected key, otherwise the entire image will fade when non-first points are selected.
            if want_fill {
                let mut area_pts = pts.clone();
                if let (Some(first), Some(last)) = (pts.first(), pts.last()) {
                    let y0 = ctx.scales.y0();
                    area_pts.push((last.0, y0));
                    area_pts.push((first.0, y0));
                }
                let fill = paint_fill(&self.style, ctx.theme, si, 0, None, fill_op);
                layers.push(SceneLayer::polygon(
                    format!("area-{si}"),
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
                format!("line-{si}"),
                cmds,
                PathStyle::stroke(stroke, sw),
            ));
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let series = unique_series(&self.rows);
        let n = self.rows.len().max(1);
        let max_x = (ctx.scales.inner_w() / n as f32 * 0.55).max(18.0);
        let max_d = if self.area || self.fill_under {
            max_x.max(28.0)
        } else {
            16.0
        };
        let mut best: Option<(f32, usize)> = None;
        for ser in &series {
            for (di, p) in line_points(&self.rows, ctx, ser) {
                let d = if self.area || self.fill_under {
                    (p.0 - local.0).abs()
                } else {
                    (p.0 - local.0).hypot(p.1 - local.1)
                };
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
    fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        datum_hover_item(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let series = unique_series(&self.rows);
        for ser in &series {
            for (di, p) in line_points(&self.rows, ctx, ser) {
                if di == index {
                    return Some(p);
                }
            }
        }
        None
    }
}

impl From<LineYMark> for ChartMark {
    fn from(m: LineYMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// Pie
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct PieMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    inner_radius: f32,
}

/// Polar pie / donut (TanStack polar `pie`, `inner_radius` > 0 is a ring).
pub fn pie<I, T>(data: I) -> PieMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    PieMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        inner_radius: 0.0,
    }
}

impl PieMark {
    style_setters!();
    pub fn inner_radius(mut self, frac: f32) -> Self {
        self.inner_radius = frac.clamp(0.0, 0.85);
        self
    }
}

impl MarkRender for PieMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "pie"
    }
    fn is_polar(&self) -> bool {
        true
    }
    fn channels(&self) -> MarkChannels {
        MarkChannels {
            polar_values: self.rows.iter().map(|r| r.value).collect(),
            polar_labels: self
                .rows
                .iter()
                .map(|r| {
                    if r.label.is_empty() {
                        format!("{:.0}", r.value)
                    } else {
                        r.label.clone()
                    }
                })
                .collect(),
            ..MarkChannels::default()
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let s = ctx.polar_size.max(48.0);
        let cx = s * 0.5;
        let cy = s * 0.5;
        // Leave a margin for the selected outer expansion (TanStack radialArc: the center/inner radius remains unchanged, only the outerRadius is increased).
        let r0 = s * 0.40;
        let inner0 = r0 * self.inner_radius;
        let values: Vec<f32> = self.rows.iter().map(|d| d.value).collect();
        let angles = pie_layout_span(&values, ctx.polar_start, ctx.polar_end);
        let mut layers = Vec::new();
        let mut selected_layer: Option<SceneLayer> = None;
        // The outer edge overlaps at about 1.5px; the same color stroke seals AA (the raster equivalent of lineJoin round).
        let eps = (1.5 / r0.max(1.0)).clamp(0.008, 0.04);
        for (i, &(a0_raw, a1_raw)) in angles.iter().enumerate() {
            if (a1_raw - a0_raw).abs() < 1e-5 {
                continue;
            }
            let a0 = a0_raw - eps;
            let a1 = a1_raw + eps;
            let selected = ctx.selected == Some(i);
            let r = if selected { r0 + s * 0.035 } else { r0 };
            let inner = inner0;
            let segs = ((a1 - a0).abs() * r / 1.25).ceil().max(28.0) as usize;
            let cmds = if inner >= 1.0 {
                pie_ring(cx, cy, r, inner, a0, a1, segs)
            } else {
                pie_sector(cx, cy, r, a0, a1, segs)
            };
            let fill = paint_fill(&self.style, ctx.theme, i, i, ctx.selected, 1.0);
            let style = if selected {
                PathStyle::fill_stroke(
                    fill,
                    super::theme::lerp_rgb(fill, ctx.theme.foreground, 0.32),
                    1.75,
                )
            } else {
                PathStyle::fill_stroke(fill, fill, 1.25)
            };
            let layer = SceneLayer::path(format!("pie-{i}"), cmds, style);
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
        let s = ctx.polar_size.max(48.0);
        let cx = s * 0.5;
        let cy = s * 0.5;
        let r0 = s * 0.40;
        let inner0 = r0 * self.inner_radius;
        let values: Vec<f32> = self.rows.iter().map(|d| d.value).collect();
        let angles = pie_layout_span(&values, ctx.polar_start, ctx.polar_end);
        let dx = local.0 - cx;
        let dy = local.1 - cy;
        let dist = dx.hypot(dy);
        let r_hit = if ctx.selected.is_some() {
            r0 + s * 0.035 + 4.0
        } else {
            r0 + 4.0
        };
        if dist > r_hit || (inner0 >= 1.0 && dist < inner0 - 2.0) {
            return None;
        }
        let ang = dy.atan2(dx);
        for (i, &(a0, a1)) in angles.iter().enumerate() {
            let span = a1 - a0;
            if span <= 0.0 {
                continue;
            }
            let mut rel = (ang - a0) % TAU;
            if rel < 0.0 {
                rel += TAU;
            }
            if rel < span {
                return Some(i);
            }
        }
        None
    }
    fn hover_text(&self, index: usize) -> Option<String> {
        datum_hover_text(&self.rows, index)
    }
    fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        datum_hover_item(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let s = ctx.polar_size.max(48.0);
        let values: Vec<f32> = self.rows.iter().map(|d| d.value).collect();
        let angles = pie_layout_span(&values, ctx.polar_start, ctx.polar_end);
        let (a0, a1) = *angles.get(index)?;
        let mid = (a0 + a1) * 0.5;
        let r = s * 0.40 * 0.62;
        Some((s * 0.5 + r * mid.cos(), s * 0.5 + r * mid.sin()))
    }
}

impl From<PieMark> for ChartMark {
    fn from(m: PieMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// Dot
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DotMark {
    id: String,
    rows: Vec<ChartDatum>,
    style: MarkStyle,
    radius: f32,
}

/// 散点（TanStack `dot`）。默认 r=4。
pub fn dot<I, T>(data: I) -> DotMark
where
    I: IntoIterator<Item = T>,
    T: Into<ChartDatum>,
{
    DotMark {
        id: String::new(),
        rows: collect_rows(data),
        style: MarkStyle::default(),
        radius: 4.0,
    }
}

impl DotMark {
    style_setters!();
    pub fn r(mut self, px: f32) -> Self {
        self.radius = px.max(1.0);
        self
    }
}

impl MarkRender for DotMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "dot"
    }
    fn channels(&self) -> MarkChannels {
        let times: Vec<i64> = self.rows.iter().filter_map(|r| r.time).collect();
        let cats = if !times.is_empty() {
            Vec::new()
        } else if labels_are_indexish(&self.rows) {
            Vec::new()
        } else {
            unique_labels(&self.rows)
        };
        let x_nums = if cats.is_empty() && times.is_empty() {
            (0..self.rows.len()).map(|i| i as f32).collect()
        } else {
            Vec::new()
        };
        MarkChannels {
            x_cats: cats,
            x_nums,
            x_times: times,
            y_nums: self.rows.iter().map(|r| r.value).collect(),
            y_include_zero: false,
            polar_values: Vec::new(),
            polar_labels: Vec::new(),
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let series = unique_series(&self.rows);
        let mut layers = Vec::new();
        for (si, ser) in series.iter().enumerate() {
            let indexed = line_points(&self.rows, ctx, ser);
            if indexed.is_empty() {
                continue;
            }
            layers.extend(filled_dot_layers(
                &indexed,
                si,
                self.radius,
                &self.style,
                ctx,
                "dot",
            ));
        }
        layers
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        let series = unique_series(&self.rows);
        let mut best: Option<(f32, usize)> = None;
        let max_d = (self.radius + 4.0).max(8.0);
        for ser in &series {
            for (di, p) in line_points(&self.rows, ctx, ser) {
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
    fn hover_item(&self, index: usize) -> Option<ChartTooltipItem> {
        datum_hover_item(&self.rows, index)
    }
    fn datum_point(&self, ctx: &MarkRenderContext<'_>, index: usize) -> Option<Point2> {
        let series = unique_series(&self.rows);
        for ser in &series {
            for (di, p) in line_points(&self.rows, ctx, ser) {
                if di == index {
                    return Some(p);
                }
            }
        }
        None
    }
}

impl From<DotMark> for ChartMark {
    fn from(m: DotMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// RuleX / RuleY
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RuleMark {
    id: String,
    values: Vec<f32>,
    cats: Vec<String>,
    style: MarkStyle,
    vertical: bool,
}

/// Horizontal reference line (TanStack `ruleY`).
pub fn rule_y(values: impl IntoIterator<Item = f32>) -> RuleMark {
    RuleMark {
        id: String::new(),
        values: values.into_iter().collect(),
        cats: Vec::new(),
        style: MarkStyle {
            stroke: Some(ChartPaint::Rgba(crate::style::parse_color("destructive"))),
            stroke_width: Some(1.5),
            ..MarkStyle::default()
        },
        vertical: false,
    }
}

/// Vertical guide line (TanStack `ruleX`). Category name or numerical subscript.
pub fn rule_x_num(values: impl IntoIterator<Item = f32>) -> RuleMark {
    RuleMark {
        id: String::new(),
        values: values.into_iter().collect(),
        cats: Vec::new(),
        style: MarkStyle {
            stroke: Some(ChartPaint::Rgba(crate::style::parse_color("destructive"))),
            stroke_width: Some(1.5),
            ..MarkStyle::default()
        },
        vertical: true,
    }
}

pub fn rule_x(cats: impl IntoIterator<Item = impl Into<String>>) -> RuleMark {
    RuleMark {
        id: String::new(),
        values: Vec::new(),
        cats: cats.into_iter().map(|s| s.into()).collect(),
        style: MarkStyle {
            stroke: Some(ChartPaint::Rgba(crate::style::parse_color("destructive"))),
            stroke_width: Some(1.5),
            ..MarkStyle::default()
        },
        vertical: true,
    }
}

impl RuleMark {
    style_setters!();
}

impl MarkRender for RuleMark {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        if self.vertical { "ruleX" } else { "ruleY" }
    }
    fn channels(&self) -> MarkChannels {
        if self.vertical {
            MarkChannels {
                x_cats: self.cats.clone(),
                x_nums: self.values.clone(),
                y_include_zero: false,
                ..MarkChannels::default()
            }
        } else {
            MarkChannels {
                y_nums: self.values.clone(),
                y_include_zero: false,
                ..MarkChannels::default()
            }
        }
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        let sw = self.style.stroke_width.unwrap_or(1.5);
        let stroke = paint_stroke(&self.style, ctx.theme, 0, 0, None, 1.0);
        let style = PathStyle::stroke(stroke, sw);
        let mut layers = Vec::new();
        if self.vertical {
            let mut xs = Vec::new();
            for c in &self.cats {
                if let Some(i) = ctx.scales.cat_index(c) {
                    xs.push(ctx.scales.x_center(i));
                }
            }
            for v in &self.values {
                xs.push(ctx.scales.x_num(*v));
            }
            let y0 = ctx.scales.pad[1];
            let y1 = ctx.scales.plot_h - ctx.scales.pad[3];
            for (i, x) in xs.iter().enumerate() {
                let cmds = stroke_path((*x, y0), (*x, y1), &self.style);
                layers.push(SceneLayer::path(format!("ruleX-{i}"), cmds, style.clone()));
            }
        } else {
            let x0 = ctx.scales.pad[0];
            let x1 = ctx.scales.plot_w - ctx.scales.pad[2];
            for (i, v) in self.values.iter().enumerate() {
                let y = ctx.scales.y(*v);
                let cmds = stroke_path((x0, y), (x1, y), &self.style);
                layers.push(SceneLayer::path(format!("ruleY-{i}"), cmds, style.clone()));
            }
        }
        layers
    }
    fn hit_index(&self, _ctx: &MarkRenderContext<'_>, _local: Point2) -> Option<usize> {
        None
    }
}

impl From<RuleMark> for ChartMark {
    fn from(m: RuleMark) -> Self {
        ChartMark::wrap(m)
    }
}

// ---------------------------------------------------------------------------
// custom_mark (TanStack createMark narrowing)
// ---------------------------------------------------------------------------

struct CustomMarkInner {
    id: String,
    channels: MarkChannels,
    render: Rc<dyn Fn(&MarkRenderContext<'_>) -> Vec<SceneLayer>>,
    hit: Option<Rc<dyn Fn(&MarkRenderContext<'_>, Point2) -> Option<usize>>>,
    polar: bool,
}

impl MarkRender for CustomMarkInner {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "custom"
    }
    fn is_polar(&self) -> bool {
        self.polar
    }
    fn channels(&self) -> MarkChannels {
        self.channels.clone()
    }
    fn render(&self, ctx: &MarkRenderContext<'_>) -> Vec<SceneLayer> {
        (self.render)(ctx)
    }
    fn hit_index(&self, ctx: &MarkRenderContext<'_>, local: Point2) -> Option<usize> {
        self.hit.as_ref().and_then(|f| f(ctx, local))
    }
}

/// 自定义几何：用户返回 [`SceneLayer`]（仍走同一 path 核）。
///
/// ```ignore
/// custom_mark("lollipop", |ctx| {
///     let p = (ctx.scales.x_center(0), ctx.scales.y(42.0));
///     vec![SceneLayer::polyline("stem", &[(p.0, ctx.scales.y0()), p], PathStyle::stroke(ctx.theme.color(0), 2.0))]
/// })
/// ```
pub fn custom_mark(
    id: impl Into<String>,
    render: impl Fn(&MarkRenderContext<'_>) -> Vec<SceneLayer> + 'static,
) -> CustomMarkBuilder {
    CustomMarkBuilder {
        id: id.into(),
        channels: MarkChannels::default(),
        render: Rc::new(render),
        hit: None,
        polar: false,
    }
}

pub struct CustomMarkBuilder {
    id: String,
    channels: MarkChannels,
    render: Rc<dyn Fn(&MarkRenderContext<'_>) -> Vec<SceneLayer>>,
    hit: Option<Rc<dyn Fn(&MarkRenderContext<'_>, Point2) -> Option<usize>>>,
    polar: bool,
}

impl CustomMarkBuilder {
    /// Contribute domain to the shared scale (otherwise the custom layer does not participate in axis inference).
    pub fn channels(mut self, ch: MarkChannels) -> Self {
        self.channels = ch;
        self
    }
    pub fn hit(
        mut self,
        f: impl Fn(&MarkRenderContext<'_>, Point2) -> Option<usize> + 'static,
    ) -> Self {
        self.hit = Some(Rc::new(f));
        self
    }
    pub fn polar(mut self, on: bool) -> Self {
        self.polar = on;
        self
    }
}

impl From<CustomMarkBuilder> for ChartMark {
    fn from(b: CustomMarkBuilder) -> Self {
        ChartMark::wrap(CustomMarkInner {
            id: b.id,
            channels: b.channels,
            render: b.render,
            hit: b.hit,
            polar: b.polar,
        })
    }
}
