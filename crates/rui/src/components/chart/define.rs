//! `define_chart`：marks + scales + theme → 同一 path 核场景（TanStack ChartSpec 窄化）。

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::core::reactivity::{use_scoped_signal, Signal};
use crate::core::tree::{CursorKind, Element, LayoutStatus};
use crate::primitives::path::{
    attach_path, encode_path_d, encode_points, ensure_raster_texture, filled_circle, filled_rect,
    points_from_commands, register_path_scene, PathCommand, PathStyle, Point2, ATTR_PATH_D,
    ATTR_PATH_MARKERS, ATTR_PATH_SCENE, ATTR_PATH_VIEWBOX,
};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

use super::mark::{ChartMark, MarkRenderContext, SceneLayer};
use super::polar::polar_guide_layers;
use super::scale::{format_time_label, resolve_scales, AxisSpec, ScaleKind};
use super::theme::ChartTheme;

/// RSX `<Chart definition={…} />` 入参。
#[derive(Clone)]
pub struct ChartDefinition(pub ChartBuilder);

impl From<ChartBuilder> for ChartDefinition {
    fn from(b: ChartBuilder) -> Self {
        Self(b)
    }
}

/// TanStack `defineChart({ marks, x, y, theme, guides })` 的 RUI Builder。
#[derive(Clone)]
pub struct ChartBuilder {
    id: String,
    marks: Vec<ChartMark>,
    x: Option<AxisSpec>,
    y: Option<AxisSpec>,
    theme: ChartTheme,
    guides: bool,
    legend: bool,
    axis_labels: bool,
    /// 笛卡尔图高（px）。Polar 用 [`Self::size`]。
    height: f32,
    width: Option<f32>,
    /// Polar 正方形边长。
    polar_size: f32,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
    tooltip: bool,
    crosshair: bool,
    selected: Option<usize>,
    /// 受控选中；点击时写入，且 chart 在信号变更时重建（饼图外扩等）。
    selected_signal: Option<Signal<Option<usize>>>,
    on_select: Option<Rc<dyn Fn(usize)>>,
}

/// 入口：组合 marks，而不是选用固定图表类型。
pub fn define_chart() -> ChartBuilder {
    ChartBuilder {
        id: String::new(),
        marks: Vec::new(),
        x: None,
        y: None,
        theme: ChartTheme::tokens(),
        guides: true,
        legend: false,
        axis_labels: false,
        height: 160.0,
        width: None,
        polar_size: 180.0,
        polar_start: -FRAC_PI_2,
        polar_end: -FRAC_PI_2 + TAU,
        polar_inner: 0.15,
        tooltip: false,
        crosshair: false,
        selected: None,
        selected_signal: None,
        on_select: None,
    }
}

impl ChartBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn mark(mut self, m: impl Into<ChartMark>) -> Self {
        self.marks.push(m.into());
        self
    }

    pub fn marks(mut self, marks: impl IntoIterator<Item = impl Into<ChartMark>>) -> Self {
        self.marks.extend(marks.into_iter().map(Into::into));
        self
    }

    pub fn theme(mut self, theme: ChartTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn guides(mut self, on: bool) -> Self {
        self.guides = on;
        self
    }

    pub fn legend(mut self, on: bool) -> Self {
        self.legend = on;
        self
    }

    /// 底轴类别标签（柱状图预设默认开）。
    pub fn axis_labels(mut self, on: bool) -> Self {
        self.axis_labels = on;
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        self.height = px.max(16.0);
        self
    }

    pub fn size(mut self, px: f32) -> Self {
        self.polar_size = px.max(48.0);
        self.height = px.max(48.0);
        self
    }

    pub fn width_px(mut self, px: f32) -> Self {
        self.width = Some(px.max(16.0));
        self
    }

    pub fn selected(mut self, i: Option<usize>) -> Self {
        self.selected = i;
        self
    }

    pub fn selected_signal(mut self, s: Signal<Option<usize>>) -> Self {
        self.selected_signal = Some(s);
        self
    }

    pub fn on_select(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    pub fn x_linear(mut self) -> Self {
        self.x = Some(AxisSpec::linear());
        self
    }
    pub fn x_band(mut self) -> Self {
        self.x = Some(AxisSpec::band());
        self
    }
    pub fn x_point(mut self) -> Self {
        self.x = Some(AxisSpec::point());
        self
    }
    pub fn x_padding(mut self, p: f32) -> Self {
        self.x.get_or_insert_with(AxisSpec::band).padding = p.clamp(0.0, 0.9);
        self
    }
    pub fn x_label(mut self, label: impl Into<String>) -> Self {
        self.x.get_or_insert_with(AxisSpec::linear).label = Some(label.into());
        self
    }
    pub fn x_domain(mut self, lo: f32, hi: f32) -> Self {
        self.x.get_or_insert_with(AxisSpec::linear).domain_num = Some((lo, hi));
        self
    }
    pub fn x_log(mut self) -> Self {
        self.x = Some(AxisSpec::log());
        self
    }
    pub fn x_time(mut self) -> Self {
        self.x = Some(AxisSpec::time());
        self
    }
    pub fn x_log_base(mut self, base: f32) -> Self {
        self.x.get_or_insert_with(AxisSpec::log).log_base = base.max(1.0001);
        self
    }
    pub fn x_domain_time(mut self, lo: i64, hi: i64) -> Self {
        self.x.get_or_insert_with(AxisSpec::time).domain_time = Some((lo, hi));
        self
    }

    pub fn y_linear(mut self) -> Self {
        let mut a = self.y.take().unwrap_or_else(AxisSpec::linear);
        a.kind = ScaleKind::Linear;
        self.y = Some(a);
        self
    }
    pub fn y_nice(mut self) -> Self {
        self.y.get_or_insert_with(AxisSpec::linear).nice = true;
        self
    }
    pub fn y_grid(mut self, on: bool) -> Self {
        self.y.get_or_insert_with(AxisSpec::linear).grid = on;
        self
    }
    pub fn y_label(mut self, label: impl Into<String>) -> Self {
        self.y.get_or_insert_with(AxisSpec::linear).label = Some(label.into());
        self
    }
    pub fn y_domain(mut self, lo: f32, hi: f32) -> Self {
        self.y.get_or_insert_with(AxisSpec::linear).domain_num = Some((lo, hi));
        self
    }
    pub fn y_log(mut self) -> Self {
        let mut a = self.y.take().unwrap_or_else(AxisSpec::log);
        a.kind = ScaleKind::Log;
        self.y = Some(a);
        self
    }
    pub fn y_log_base(mut self, base: f32) -> Self {
        self.y.get_or_insert_with(AxisSpec::log).log_base = base.max(1.0001);
        self
    }

    /// Polar 起始角（弧度，默认 12 点 = `-π/2`）。
    pub fn polar_start(mut self, rad: f32) -> Self {
        self.polar_start = rad;
        self
    }
    pub fn polar_end(mut self, rad: f32) -> Self {
        self.polar_end = rad;
        self
    }
    /// Polar 内半径比例（径向柱/线；pie 仍用 mark `.inner_radius`）。
    pub fn polar_inner(mut self, frac: f32) -> Self {
        self.polar_inner = frac.clamp(0.0, 0.85);
        self
    }

    /// 钉在选中 datum（再点取消）。对照 TanStack tooltip，不跟指针 move。
    pub fn tooltip(mut self, on: bool) -> Self {
        self.tooltip = on;
        self
    }
    pub fn crosshair(mut self, on: bool) -> Self {
        self.crosshair = on;
        self
    }

    fn polar(&self) -> bool {
        self.marks.iter().any(|m| m.is_polar())
    }
}

fn infer_x(marks: &[ChartMark], has_bar: bool) -> AxisSpec {
    let any_time = marks.iter().any(|m| !m.channels().x_times.is_empty());
    if any_time {
        return AxisSpec::time();
    }
    let any_cat = marks.iter().any(|m| !m.channels().x_cats.is_empty());
    if !any_cat {
        return AxisSpec::linear();
    }
    if has_bar {
        AxisSpec::band()
    } else {
        AxisSpec::point()
    }
}

fn make_ctx<'a>(
    scales: &'a super::scale::ResolvedScales,
    theme: &'a ChartTheme,
    selected: Option<usize>,
    polar_size: f32,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
) -> MarkRenderContext<'a> {
    MarkRenderContext {
        scales,
        theme,
        selected,
        polar_size,
        polar_start,
        polar_end,
        polar_inner,
    }
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

fn cartesian_frame(
    height: f32,
    theme: &ChartTheme,
    guides: bool,
    grid: bool,
    plot: Element,
) -> Element {
    let h = format!("{height}px");
    let mut frame: Element = r#box()
        .w("100%")
        .h(h.as_str())
        .min_h(h.as_str())
        .max_h(h.as_str())
        .flex_shrink(0.0)
        .overflow("hidden")
        .rounded("0.375rem")
        .relative(true)
        .into();
    frame = frame.bg(theme.background).border(theme.border, 1.0);
    if guides && grid {
        for t in [0.25f32, 0.5, 0.75] {
            let mut ln: Element = r#box()
                .w("100%")
                .h(1.0)
                .absolute(true)
                .left(0.0)
                .top(format!("{}%", t * 100.0))
                .pointer_events("none")
                .into();
            ln = ln.bg(theme.grid);
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
        base = base.bg(theme.border);
        frame = frame.child(base);
    }
    frame.child(plot)
}

fn compile_layers(layers: &[SceneLayer]) -> (Vec<(Vec<PathCommand>, PathStyle)>, Vec<Point2>, f32) {
    let mut out = Vec::new();
    let mut markers = Vec::new();
    let mut marker_r = 0.0f32;
    for layer in layers {
        if !layer.commands.is_empty() {
            out.push((layer.commands.clone(), layer.style.clone()));
        }
        if !layer.markers.is_empty() {
            markers.extend_from_slice(&layer.markers);
            marker_r = marker_r.max(layer.style.stroke_width.max(2.5) * 0.9);
        }
    }
    (out, markers, marker_r)
}

/// 无 DotMark 时把选中顶点画进 path 场景最后一层（与线同一坐标系，盖在折线上）。
fn push_selected_vertex(
    path_layers: &mut Vec<(Vec<PathCommand>, PathStyle)>,
    markers: &[Point2],
    marks: &[ChartMark],
    selected: Option<usize>,
    theme: &ChartTheme,
) {
    let Some(i) = selected else {
        return;
    };
    if marks
        .iter()
        .any(|m| m.kind() == "dot" || m.kind() == "radialDot")
    {
        return;
    }
    let pt = markers.get(i).copied().or_else(|| {
        path_layers
            .iter()
            .rev()
            .find(|(cmds, st)| st.stroke.is_some() && st.fill.is_none() && !cmds.is_empty())
            .and_then(|(cmds, _)| points_from_commands(cmds).get(i).copied())
    });
    if let Some((x, y)) = pt {
        path_layers.push((
            filled_circle(x, y, 3.5, 20),
            PathStyle::fill_stroke(theme.color(0), theme.background, 1.25),
        ));
    }
}

fn attach_chart_scene(
    mut plot: Element,
    path_layers: &[(Vec<PathCommand>, PathStyle)],
    markers: &[Point2],
    marker_r: f32,
    logic_w: f32,
    logic_h: f32,
) -> Element {
    let id = register_path_scene(
        path_layers.to_vec(),
        markers.to_vec(),
        marker_r,
        logic_w.max(1.0),
        logic_h.max(1.0),
    );
    plot.set_attr(ATTR_PATH_SCENE, format!("{id:016x}"));
    let dim_w = logic_w.round().max(16.0) as u32;
    let dim_h = logic_h.round().max(16.0) as u32;
    if let Some(tex) = ensure_raster_texture(path_layers, markers, marker_r, dim_w, dim_h, 2) {
        plot = plot.image_texture(tex);
    }
    if let Some((cmds, _)) = path_layers
        .iter()
        .find(|(c, st)| st.stroke.is_some() && !c.is_empty())
        .or_else(|| path_layers.first())
    {
        if !cmds.is_empty() {
            plot.set_attr(ATTR_PATH_D, encode_path_d(cmds));
        }
    } else if !markers.is_empty() {
        plot.set_attr(ATTR_PATH_D, "M0,0");
    }
    if !markers.is_empty() {
        plot.set_attr(ATTR_PATH_MARKERS, encode_points(markers));
    }
    plot.set_attr(ATTR_PATH_VIEWBOX, format!("0 0 {logic_w} {logic_h}"));
    plot
}

fn kind_attr(marks: &[ChartMark]) -> String {
    if marks.len() == 1 {
        return marks[0].kind().to_string();
    }
    let mut kinds: Vec<&str> = marks.iter().map(|m| m.kind()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    if kinds.len() == 1 {
        kinds[0].to_string()
    } else {
        "chart".into()
    }
}

impl From<ChartBuilder> for Element {
    fn from(mut builder: ChartBuilder) -> Self {
        // 每图独立选中信号（TanStack keyedSelection 按 chart 实例隔离）。
        // 未传 `selected_signal` 时用 runtime key 建非受控信号，避免同 Owner 下多图串状态。
        if builder.selected_signal.is_none() {
            let key = chart_runtime_key(&builder);
            let init = builder.selected;
            builder.selected_signal = Some(use_scoped_signal(&format!("{key}::sel"), move || init));
        }
        live_selected_chart(builder)
    }
}

fn chart_runtime_key(b: &ChartBuilder) -> String {
    if !b.id.trim().is_empty() {
        return format!("chart:{}", b.id.trim());
    }
    let mut h = DefaultHasher::new();
    b.polar().hash(&mut h);
    b.height.to_bits().hash(&mut h);
    b.polar_size.to_bits().hash(&mut h);
    b.tooltip.hash(&mut h);
    b.inner_fingerprint(&mut h);
    format!("chart:{:016x}", h.finish())
}

impl ChartBuilder {
    fn inner_fingerprint(&self, h: &mut DefaultHasher) {
        self.crosshair.hash(h);
        self.axis_labels.hash(h);
        self.polar_inner.to_bits().hash(h);
        for m in &self.marks {
            m.kind().hash(h);
            m.id().hash(h);
            let ch = m.channels();
            for v in ch.y_nums.iter().chain(ch.polar_values.iter()) {
                v.to_bits().hash(h);
            }
            for c in &ch.x_cats {
                c.hash(h);
            }
        }
    }
}

fn chart_shell_h(b: &ChartBuilder) -> f32 {
    if b.polar() {
        b.polar_size.max(48.0)
    } else {
        let extra = if b.axis_labels { 28.0 } else { 0.0 };
        b.height.max(16.0) + extra
    }
}

fn live_selected_chart(builder: ChartBuilder) -> Element {
    if !crate::core::reactivity::is_tracking() {
        let mut b = builder;
        if let Some(s) = b.selected_signal {
            b.selected = s.peek();
        }
        return if b.polar() {
            compile_polar(b)
        } else {
            compile_cartesian(b)
        };
    }
    let sig = builder.selected_signal.expect("selected_signal");
    let key = chart_runtime_key(&builder);
    let shell_h = chart_shell_h(&builder);
    crate::app::reactive::reactive_anchor(key.clone(), move || {
        let mut b = builder.clone();
        b.selected = sig.get();
        if b.polar() {
            compile_polar(b)
        } else {
            compile_cartesian(b)
        }
    });
    crate::app::reactive::reactive_placeholder_sized(key, "100%", format!("{shell_h:.0}px"))
}

/// 再点同一 datum → 取消；点空白 → 取消。
pub(crate) fn next_chart_selection(current: Option<usize>, hit: Option<usize>) -> Option<usize> {
    match hit {
        Some(i) if current == Some(i) => None,
        other => other,
    }
}

fn selection_pin(
    marks: &[ChartMark],
    ctx: &MarkRenderContext<'_>,
    selected: Option<usize>,
) -> ChartPin {
    let i = selected?;
    let text = marks
        .iter()
        .rev()
        .find_map(|m| m.hover_text(i))
        .filter(|t| !t.is_empty())?;
    let pt = marks.iter().rev().find_map(|m| m.datum_point(ctx, i))?;
    Some((pt.0, pt.1, text))
}

impl From<ChartDefinition> for Element {
    fn from(d: ChartDefinition) -> Self {
        d.0.into()
    }
}

type ChartPin = Option<(f32, f32, String)>;

const TOOLTIP_GAP_PX: f32 = 8.0;
const TOOLTIP_MARGIN_PX: f32 = 4.0;

fn estimate_tooltip_px(text: &str) -> (f32, f32) {
    let tw = (text.chars().count() as f32 * 7.2 + 28.0).clamp(40.0, 320.0);
    (tw, 28.0)
}

/// 画布内翻转：点靠近右/上缘时改锚到左/下方，避免 `overflow: hidden` 裁切气泡。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TooltipPinPlace {
    pub flip_x: bool,
    pub flip_y: bool,
}

pub(crate) fn tooltip_pin_place(
    lx: f32,
    ly: f32,
    logic_w: f32,
    logic_h: f32,
    text: &str,
) -> TooltipPinPlace {
    let (tw, th) = estimate_tooltip_px(text);
    let w = logic_w.max(1.0);
    let h = logic_h.max(1.0);
    let gap = TOOLTIP_GAP_PX + TOOLTIP_MARGIN_PX;
    let room_right = (w - lx - gap).max(0.0);
    let room_left = (lx - gap).max(0.0);
    let room_above = (ly - gap).max(0.0);
    let room_below = (h - ly - gap).max(0.0);
    TooltipPinPlace {
        flip_x: tw > room_right && room_left >= room_right,
        flip_y: th > room_above && room_below >= room_above,
    }
}

fn pin_overlay_layer(
    pin: ChartPin,
    theme: &ChartTheme,
    logic_w: f32,
    logic_h: f32,
    tooltip: bool,
    crosshair: bool,
    polar: bool,
) -> Element {
    let mut layer: Element = r#box()
        .w("100%")
        .h("100%")
        .absolute(true)
        .left(0.0)
        .top(0.0)
        .pointer_events("none")
        .z_index(8)
        .into();
    let Some((lx, ly, text_s)) = pin else {
        return layer;
    };
    let x_pct = (lx / logic_w.max(1.0) * 100.0).clamp(0.0, 100.0);
    let y_pct = (ly / logic_h.max(1.0) * 100.0).clamp(0.0, 100.0);
    if crosshair {
        let mut v: Element = r#box()
            .w(1.0)
            .h("100%")
            .absolute(true)
            .left(format!("{x_pct:.2}%"))
            .top(0.0)
            .pointer_events("none")
            .into();
        v = v.bg(theme.grid);
        let mut hline: Element = r#box()
            .h(1.0)
            .w("100%")
            .absolute(true)
            .left(0.0)
            .top(format!("{y_pct:.2}%"))
            .pointer_events("none")
            .into();
        hline = hline.bg(theme.grid);
        layer = layer.child(v).child(hline);
        if polar {
            let mut dot: Element = r#box()
                .w(6.0)
                .h(6.0)
                .rounded("9999px")
                .absolute(true)
                .left(format!("calc({x_pct:.2}% - 3px)"))
                .top(format!("calc({y_pct:.2}% - 3px)"))
                .pointer_events("none")
                .into();
            dot = dot.bg(theme.color(0));
            layer = layer.child(dot);
        }
    }
    if tooltip && !text_s.is_empty() {
        let place = tooltip_pin_place(lx, ly, logic_w, logic_h, &text_s);
        let gap = TOOLTIP_GAP_PX;
        let mut tip = crate::components::tooltip::tooltip_bubble(text(text_s.clone()).into());
        tip = tip.absolute().pointer_events("none");
        if place.flip_x {
            let from_right = (100.0 - x_pct).clamp(0.0, 100.0);
            tip = tip.right(format!("calc({from_right:.2}% + {gap}px)"));
        } else {
            tip = tip.left(format!("calc({x_pct:.2}% + {gap}px)"));
        }
        if place.flip_y {
            tip = tip.top(format!("calc({y_pct:.2}% + {gap}px)"));
        } else {
            let from_bottom = (100.0 - y_pct).clamp(0.0, 100.0);
            tip = tip.bottom(format!("calc({from_bottom:.2}% + {gap}px)"));
        }
        let room_w = if place.flip_x {
            (lx - gap - TOOLTIP_MARGIN_PX).max(48.0)
        } else {
            (logic_w - lx - gap - TOOLTIP_MARGIN_PX).max(48.0)
        };
        tip = tip.max_w(format!("{room_w}px"));
        // 子贴图排在图 scene 贴图之后，避免面积填充盖住 SDF 气泡底。
        tip = attach_path(
            tip,
            &filled_rect(0.0, 0.0, 1.0, 1.0),
            &PathStyle::fill(tokens::popover()),
        );
        tip.set_attr(ATTR_PATH_VIEWBOX, "0 0 1 1");
        layer = layer.child(tip);
    }
    layer
}

fn attach_pin_overlay(
    plot: Element,
    pin: ChartPin,
    theme: &ChartTheme,
    logic_w: f32,
    logic_h: f32,
    tooltip: bool,
    crosshair: bool,
    polar: bool,
) -> Element {
    if pin.is_none() && !crosshair {
        return plot;
    }
    plot.child(pin_overlay_layer(
        pin, theme, logic_w, logic_h, tooltip, crosshair, polar,
    ))
}

fn attach_chart_pointer(
    mut plot: Element,
    marks: Vec<ChartMark>,
    scales: super::scale::ResolvedScales,
    theme: ChartTheme,
    selected: Option<usize>,
    polar_size: f32,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
    logic_w: f32,
    logic_h: f32,
    on_select: Option<Rc<dyn Fn(usize)>>,
    selected_signal: Option<Signal<Option<usize>>>,
) -> Element {
    let bounds = Rc::new(Cell::new([0.0_f32; 4]));
    let interactive = on_select.is_some() || selected_signal.is_some();
    if interactive {
        plot = plot.cursor_kind(CursorKind::Pointer);
        let marks_hit = marks;
        let scales_hit = scales;
        let theme_hit = theme;
        let bounds_pd = bounds.clone();
        plot = plot.on_pointer_down(move |e| {
            if e.phase == EventPhase::Capture || marks_hit.is_empty() {
                return;
            }
            let b = if e.current_bounds[2] > 0.0 && e.current_bounds[3] > 0.0 {
                e.current_bounds
            } else {
                bounds_pd.get()
            };
            if b[2] <= 0.0 || b[3] <= 0.0 {
                return;
            }
            let lx = (e.x - b[0]) / b[2] * logic_w;
            let ly = (e.y - b[1]) / b[3] * logic_h;
            let ctx = make_ctx(
                &scales_hit,
                &theme_hit,
                selected,
                polar_size,
                polar_start,
                polar_end,
                polar_inner,
            );
            let mut hit = None;
            for m in marks_hit.iter().rev() {
                if let Some(i) = m.hit_index(&ctx, (lx, ly)) {
                    hit = Some(i);
                    break;
                }
            }
            let next = next_chart_selection(selected, hit);
            if let Some(s) = selected_signal {
                s.set(next);
            }
            if let (Some(cb), Some(i)) = (&on_select, next) {
                cb(i);
            }
            e.stop_propagation();
        });
    }
    let bounds_sync = bounds;
    plot.reactive_sync(move |root, _| {
        bounds_sync.set(root.layout_bounds);
        LayoutStatus::Clean
    })
}

fn compile_cartesian(builder: ChartBuilder) -> Element {
    let ChartBuilder {
        id,
        marks,
        x,
        y,
        theme,
        guides,
        legend: _,
        axis_labels,
        height,
        width,
        polar_start,
        polar_end,
        polar_inner,
        tooltip,
        crosshair,
        selected,
        selected_signal,
        on_select,
        ..
    } = builder;
    let h = height.max(16.0);
    let w_logic = width.unwrap_or(320.0);
    let has_bar = marks
        .iter()
        .any(|m| m.kind() == "barY" || m.kind() == "barX");
    let x_spec = x.unwrap_or_else(|| infer_x(&marks, has_bar));
    let mut y_spec = y.unwrap_or_else(AxisSpec::linear);
    if guides && !y_spec.grid && y_spec.domain_num.is_none() {
        y_spec.grid = true;
    }

    let mut x_cats = Vec::new();
    let mut x_nums = Vec::new();
    let mut x_times = Vec::new();
    let mut y_nums = Vec::new();
    let mut y_zero = false;
    for m in &marks {
        let ch = m.channels();
        for c in ch.x_cats {
            if !x_cats.iter().any(|e| e == &c) {
                x_cats.push(c);
            }
        }
        x_nums.extend(ch.x_nums);
        x_times.extend(ch.x_times);
        y_nums.extend(ch.y_nums);
        y_zero |= ch.y_include_zero;
    }
    if let Some(forced) = x_spec.domain_cats.clone() {
        x_cats = forced;
    }

    let scales = resolve_scales(
        w_logic,
        h,
        &x_spec,
        &y_spec,
        x_cats.clone(),
        &x_nums,
        &x_times,
        &y_nums,
        y_zero || has_bar,
    );
    let ctx = make_ctx(
        &scales,
        &theme,
        selected,
        h,
        polar_start,
        polar_end,
        polar_inner,
    );
    let mut layers = Vec::new();
    for m in &marks {
        layers.extend(m.render(&ctx));
    }
    let (mut path_layers, markers, marker_r) = compile_layers(&layers);
    push_selected_vertex(&mut path_layers, &markers, &marks, selected, &theme);
    let pin = if tooltip || crosshair {
        selection_pin(&marks, &ctx, selected)
    } else {
        None
    };
    let mut hit: Element = r#box().w("100%").h("100%").relative(true).into();
    hit = attach_chart_pointer(
        hit,
        marks.clone(),
        scales.clone(),
        theme.clone(),
        selected,
        h,
        polar_start,
        polar_end,
        polar_inner,
        w_logic,
        h,
        on_select,
        selected_signal,
    );

    let mut plot = cartesian_frame(h, &theme, guides, y_spec.grid || guides, hit);
    plot = attach_chart_scene(plot, &path_layers, &markers, marker_r, w_logic, h);
    plot.set_attr("rui_chart", kind_attr(&marks));
    plot = attach_pin_overlay(plot, pin, &theme, w_logic, h, tooltip, crosshair, false);

    if let Some(w) = width {
        plot = r#box().w(format!("{}px", w.max(16.0))).child(plot).into();
    }
    plot = plot.flex_shrink(0.0);

    let mut root = plot;
    let time_labels =
        scales.x_kind == ScaleKind::Time && (axis_labels || scales.x_time_domain.is_some());
    if axis_labels && !x_cats.is_empty() {
        let mut labels: Element = hstack().w("100%").gap("0.5rem").into();
        for lab in &x_cats {
            labels = labels.child(vstack().flex_grow(1.0).items_center().child(caption(lab)));
        }
        root = vstack()
            .w("100%")
            .flex_shrink(0.0)
            .gap("0.5rem")
            .child(root)
            .child(labels)
            .into();
    } else if time_labels {
        if let Some((t0, t1)) = scales.x_time_domain {
            let span = (t1 - t0).max(1);
            let n = 3usize;
            let mut labels: Element = hstack().w("100%").gap("0.25rem").into();
            for k in 0..=n {
                let t = t0 + span * k as i64 / n as i64;
                labels = labels.child(
                    vstack()
                        .flex_grow(1.0)
                        .items_center()
                        .child(caption(&format_time_label(t, span))),
                );
            }
            root = vstack()
                .w("100%")
                .flex_shrink(0.0)
                .gap("0.5rem")
                .child(root)
                .child(labels)
                .into();
        }
    }
    apply_optional_id(root, &id)
}

fn compile_polar(builder: ChartBuilder) -> Element {
    let ChartBuilder {
        id,
        marks,
        y,
        theme,
        guides,
        legend,
        polar_size,
        polar_start,
        polar_end,
        polar_inner,
        tooltip,
        crosshair,
        selected,
        selected_signal,
        on_select,
        ..
    } = builder;
    let s = polar_size.max(48.0);
    let mut x_cats = Vec::new();
    let mut y_nums = Vec::new();
    for m in &marks {
        let ch = m.channels();
        y_nums.extend(ch.y_nums);
        y_nums.extend(ch.polar_values.iter().copied());
        for l in ch.polar_labels.into_iter().chain(ch.x_cats.into_iter()) {
            if !x_cats.iter().any(|e| e == &l) {
                x_cats.push(l);
            }
        }
    }
    let x_spec = AxisSpec::band();
    let y_spec = y.unwrap_or_else(AxisSpec::linear);
    let scales = resolve_scales(s, s, &x_spec, &y_spec, x_cats, &[], &[], &y_nums, true);
    let ctx = make_ctx(
        &scales,
        &theme,
        selected,
        s,
        polar_start,
        polar_end,
        polar_inner,
    );
    let mut layers = Vec::new();
    let pie_only = marks.iter().all(|m| m.kind() == "pie");
    if guides && !pie_only {
        layers.extend(polar_guide_layers(&ctx));
    }
    for m in &marks {
        layers.extend(m.render(&ctx));
    }
    let (mut path_layers, markers, marker_r) = compile_layers(&layers);
    push_selected_vertex(&mut path_layers, &markers, &marks, selected, &theme);
    let pin = if tooltip || crosshair {
        selection_pin(&marks, &ctx, selected)
    } else {
        None
    };

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
    if path_layers.is_empty() && markers.is_empty() {
        for layer in &layers {
            let mut slice = Element::new()
                .w("100%")
                .h("100%")
                .absolute()
                .left(0.0)
                .top(0.0)
                .pointer_events_none();
            slice.set_attr(ATTR_PATH_D, encode_path_d(&layer.commands));
            plot = plot.child(slice);
        }
    } else {
        plot = attach_chart_scene(plot, &path_layers, &markers, marker_r, s, s);
    }
    plot.set_attr("rui_chart", kind_attr(&marks));
    plot = attach_chart_pointer(
        plot,
        marks.clone(),
        scales,
        theme.clone(),
        selected,
        s,
        polar_start,
        polar_end,
        polar_inner,
        s,
        s,
        on_select,
        selected_signal,
    );
    plot = attach_pin_overlay(plot, pin, &theme, s, s, tooltip, crosshair, true);

    if !legend {
        return apply_optional_id(plot, &id);
    }

    let ch = marks.iter().find(|m| m.is_polar()).map(|m| m.channels());
    let pairs: Vec<(String, f32)> = ch
        .map(|c| {
            c.polar_labels
                .into_iter()
                .zip(c.polar_values.into_iter())
                .collect()
        })
        .unwrap_or_default();
    let legend_el = pie_legend_rows(&pairs, &theme, selected);
    let root: Element = hstack()
        .gap("0.75rem")
        .items_center()
        .flex_shrink(0.0)
        .min_h(s)
        .child(plot)
        .child(legend_el)
        .into();
    apply_optional_id(root, &id)
}

pub(crate) fn pie_legend_rows(
    labels: &[(String, f32)],
    theme: &ChartTheme,
    selected: Option<usize>,
) -> Element {
    let mut legend = vstack().gap("0.25rem").flex_shrink(0.0);
    for (i, (lab, val)) in labels.iter().enumerate() {
        let mut swatch: Element = r#box()
            .w(10.0)
            .h(10.0)
            .min_w(10.0)
            .min_h(10.0)
            .rounded("0.125rem")
            .into();
        swatch = swatch
            .bg(theme.dim_if_unselected(theme.color(i), i, selected))
            .flex_shrink(0.0);
        let label = caption(&format!("{}  {:.0}", lab, val));
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
    legend.into()
}
