//! `define_chart`: marks + scales + theme → same path kernel scenario (TanStack ChartSpec narrowing).

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, TAU};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::app::pointer_events::EventPhase;
use crate::components::base::{paint_slot, SlotChrome, SlotMap};
use crate::core::reactivity::{create_memo, use_scoped_signal, use_signal, Signal, SignalId};
use crate::core::tree::{CursorKind, Element, LayoutStatus};
use crate::primitives::path::{
    encode_path_d, encode_points, ensure_raster_texture, filled_circle, points_from_commands,
    register_path_scene, PathCommand, PathStyle, Point2, ATTR_PATH_D, ATTR_PATH_MARKERS,
    ATTR_PATH_SCENE, ATTR_PATH_VIEWBOX,
};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

use super::datum::ChartTooltipItem;
use super::mark::{ChartMark, MarkRenderContext, SceneLayer};
use super::polar::polar_guide_layers;
use super::scale::{format_time_label, resolve_scales, resolve_scales_pad, AxisSpec, ScaleKind};
use super::theme::{ChartConfig, ChartTheme, TooltipIndicator};

/// RSX `<Chart definition={…} />` input parameter.
#[derive(Clone)]
pub struct ChartDefinition(pub ChartBuilder);

impl From<ChartBuilder> for ChartDefinition {
    fn from(b: ChartBuilder) -> Self {
        Self(b)
    }
}

/// RUI Builder for TanStack `defineChart({ marks, x, y, theme, guides })`.
#[derive(Clone)]
pub struct ChartBuilder {
    id: String,
    marks: Vec<ChartMark>,
    x: Option<AxisSpec>,
    y: Option<AxisSpec>,
    theme: ChartTheme,
    /// shadcn `ChartConfig`: Series label/color.
    config: ChartConfig,
    guides: bool,
    legend: bool,
    axis_labels: bool,
    /// Cartesian figure height (px). Polar uses [`Self::size`].
    height: f32,
    width: Option<f32>,
    /// Polar side length of the square.
    polar_size: f32,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
    tooltip: bool,
    /// shadcn tooltip indicator：`dot` | `line` | `dashed`。
    tooltip_indicator: TooltipIndicator,
    crosshair: bool,
    selected: Option<usize>,
    /// 受控选中；点击时写入，且 chart 在信号变更时重建（饼图外扩等）。
    selected_signal: Option<Signal<Option<usize>>>,
    /// 悬停 datum；与 selected 独立。live 路径用 [`ChartHover`] 信号，不整图重建。
    hovered: Option<usize>,
    hover_signal: Option<Signal<ChartHover>>,
    /// 布局测得的绘图区尺寸；有值时按此重算 scale，避免 320×h 贴图被拉伸。
    layout_w: Option<f32>,
    layout_h: Option<f32>,
    plot_size_signal: Option<Signal<(f32, f32)>>,
    tooltip_scene: Option<Rc<RefCell<Option<TooltipScene>>>>,
    /// Live tooltip island id (`{key}__tip__view`); when there is a value, compile only inserts placeholder.
    overlay_key: Option<String>,
    /// `h="full"` / `100%`：占位与绘图区吃满父级高度。
    height_fluid: bool,
    on_select: Option<Rc<dyn Fn(usize)>>,
    slots: SlotMap,
}

/// Entry: combine marks instead of selecting a fixed chart type.
pub fn define_chart() -> ChartBuilder {
    ChartBuilder {
        id: String::new(),
        marks: Vec::new(),
        x: None,
        y: None,
        theme: ChartTheme::tokens(),
        config: ChartConfig::new(),
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
        tooltip_indicator: TooltipIndicator::Dot,
        crosshair: false,
        selected: None,
        selected_signal: None,
        hovered: None,
        hover_signal: None,
        layout_w: None,
        layout_h: None,
        plot_size_signal: None,
        tooltip_scene: None,
        overlay_key: None,
        height_fluid: false,
        on_select: None,
        slots: SlotMap::default(),
    }
}

impl ChartBuilder {
    crate::impl_slot_map_methods!();

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

    /// shadcn `ChartConfig`：系列显示名与色（覆盖 palette 槽）。
    pub fn config(mut self, config: ChartConfig) -> Self {
        self.config = config;
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

    pub fn plot_h(mut self, px: f32) -> Self {
        self.height = px.max(16.0);
        self.height_fluid = false;
        self
    }
    /// Alias of [`Self::plot_h`] (plot viewport, not layout `h`).
    pub fn h(self, px: f32) -> Self {
        self.plot_h(px)
    }

    /// 绘图区高度跟随父级（`h="full"` / `100%`）。
    pub fn h_fluid(mut self) -> Self {
        self.height_fluid = true;
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
    /// Polar inner radius scale (radial cylinder/line; pie still uses mark `.inner_radius`).
    pub fn polar_inner(mut self, frac: f32) -> Self {
        self.polar_inner = frac.clamp(0.0, 0.85);
        self
    }

    /// 钉在悬停 datum；点击仍走 selected。对照 shadcn ChartTooltip。
    pub fn tooltip(mut self, on: bool) -> Self {
        self.tooltip = on;
        self
    }

    /// shadcn `ChartTooltipContent` indicator: `dot` | `line` | `dashed` (unknown → dot).
    pub fn tooltip_indicator(mut self, indicator: impl Into<TooltipIndicator>) -> Self {
        self.tooltip_indicator = indicator.into();
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
    round_bar_base: bool,
) -> MarkRenderContext<'a> {
    MarkRenderContext {
        scales,
        theme,
        selected,
        polar_size,
        polar_start,
        polar_end,
        polar_inner,
        round_bar_base,
    }
}

fn caption(text: &str) -> Element {
    build_text_node(
        text,
        TextNodeConfig {
            size_px: 12.0,
            weight: 400,
            color: parse_text_color("muted-foreground"),
            ..Default::default()
        },
    )
}

/// 轴刻度与 plot 同一套 x（band 中心 / time 点），避免 flex+gap 和柱错位。
pub(crate) fn tick_label_row(ticks: &[(f32, String)], plot_w: f32) -> Element {
    let w = plot_w.max(1.0);
    let mut row: Element = r#box()
        .w("100%")
        .relative(true)
        .min_h(16.0)
        .overflow("visible")
        .into();
    for (x, text) in ticks {
        let pct = (*x / w * 100.0).clamp(0.0, 100.0);
        let mut cell = caption(text);
        cell = cell
            .absolute()
            .left(format!("{pct:.3}%"))
            .top(0.0)
            .pointer_events_none()
            .translate_pct(-0.5, 0.0);
        row = row.child(cell);
    }
    row
}

fn apply_optional_id(mut el: Element, id: &str) -> Element {
    if !id.trim().is_empty() {
        el.template_id = Some(id.to_string());
    }
    el
}

fn cartesian_frame(
    height: f32,
    height_fluid: bool,
    theme: &ChartTheme,
    guides: bool,
    grid: bool,
    plot: Element,
) -> Element {
    let mut frame: Element = r#box()
        .w("100%")
        .flex_grow(if height_fluid { 1.0 } else { 0.0 })
        .flex_shrink(0.0)
        .overflow("visible")
        .relative(true)
        .into();
    if height_fluid {
        frame = frame.h("100%").min_h(0.0);
    } else {
        let h = format!("{height}px");
        frame = frame.h(h.as_str()).min_h(h.as_str());
    }
    // shadcn ChartContainer: transparent bottom, grid border/50; shell Card is borne by the application.
    // 网格先挂、plot 后挂：树序即「网格在贴图下」。不要用 z-index——任意非零 z 会让父级
    // 启用全局 page_stack_bands（本为 CarouselStack），数据页多图一进视口整页改绘制路径。
    // overflow visible：tooltip 靠翻转留在图内，不用 hidden 裁切。
    frame = frame.bg(theme.background);
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
    // The column/line texture is clipped in the drawing area; tooltip overlay is the follow-up brother and will not be clipped.
    frame.child(plot.overflow("hidden"))
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

/// 指针局部坐标 + 命中 datum。Clone+PartialEq 供 Signal。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChartHover {
    pub x: f32,
    pub y: f32,
    pub index: Option<usize>,
}

thread_local! {
    static CHART_HOVER_FROZEN: Cell<bool> = const { Cell::new(false) };
    static PENDING_CHART_HOVER: RefCell<HashMap<SignalId, (Signal<ChartHover>, ChartHover)>> =
        RefCell::new(HashMap::new());
    /// Leave during scroll: hide the live overlay in place (`StyleChanged`) instead of
    /// remounting the tip island (`StructureChanged` / FullClear).
    static OVERLAY_SCROLL_HIDDEN: RefCell<HashSet<SignalId>> = RefCell::new(HashSet::new());
    /// Skip `hit_index` when pointer + plot visual box have not moved.
    static LAST_HOVER_GEOM: RefCell<HashMap<SignalId, [f32; 6]>> = RefCell::new(HashMap::new());
}

/// Smooth-scroll / scrollbar-fade frames: defer tooltip overlay remounts.
/// Rebuilding `{chart}__tip__view` is StructureChanged → FullClear, which drops a scroll
/// frame and ghosts the overlay scrollbar thumb.
pub(crate) fn set_chart_hover_frozen(frozen: bool) {
    let was = CHART_HOVER_FROZEN.with(|c| c.replace(frozen));
    if was && !frozen {
        flush_pending_chart_hovers();
    }
}

fn overlay_scroll_hidden(id: SignalId) -> bool {
    OVERLAY_SCROLL_HIDDEN.with(|s| s.borrow().contains(&id))
}

fn set_overlay_scroll_hidden(id: SignalId, hidden: bool) {
    OVERLAY_SCROLL_HIDDEN.with(|s| {
        if hidden {
            s.borrow_mut().insert(id);
        } else {
            s.borrow_mut().remove(&id);
        }
    });
}

fn assign_chart_hover(sig: Signal<ChartHover>, next: ChartHover) {
    if CHART_HOVER_FROZEN.with(|c| c.get()) {
        PENDING_CHART_HOVER.with(|m| {
            m.borrow_mut().insert(sig.id(), (sig, next));
        });
        if next == ChartHover::default() {
            set_overlay_scroll_hidden(sig.id(), true);
            LAST_HOVER_GEOM.with(|m| {
                m.borrow_mut().remove(&sig.id());
            });
            crate::primitives::interactive::mark_hover_paint_dirty();
        }
        return;
    }
    set_overlay_scroll_hidden(sig.id(), false);
    if next == ChartHover::default() {
        LAST_HOVER_GEOM.with(|m| {
            m.borrow_mut().remove(&sig.id());
        });
    }
    if sig.peek() != next {
        sig.set(next);
        crate::primitives::interactive::mark_hover_paint_dirty();
    }
}

fn flush_pending_chart_hovers() {
    let pending = PENDING_CHART_HOVER.with(|m| std::mem::take(&mut *m.borrow_mut()));
    for (_, (sig, next)) in pending {
        set_overlay_scroll_hidden(sig.id(), false);
        if sig.is_alive() && sig.peek() != next {
            sig.set(next);
            crate::primitives::interactive::mark_hover_paint_dirty();
        }
    }
}

fn attach_overlay_scroll_hide(layer: Element, sig: Signal<ChartHover>) -> Element {
    layer.reactive_sync(move |el, _| {
        let target = if overlay_scroll_hidden(sig.id()) {
            0.0
        } else {
            1.0
        };
        if (el.style.opacity - target).abs() > 0.001 {
            el.style.opacity = target;
            LayoutStatus::StyleChanged
        } else {
            LayoutStatus::Clean
        }
    })
}

/// Keep pointer movement in the paint prepass; only datum changes rebuild content.
fn attach_overlay_pointer_sync(
    mut layer: Element,
    sig: Signal<ChartHover>,
    selected: Option<usize>,
    sc: &TooltipScene,
) -> Element {
    let bounds = Rc::new(Cell::new([0.0; 4]));
    let parent_bounds = bounds.clone();
    layer = attach_overlay_scroll_hide(layer, sig);
    let hide = layer.reactive_sync.take().expect("overlay scroll sync");
    layer = layer.reactive_sync(move |el, tree| {
        parent_bounds.set(el.layout_bounds);
        hide.run(el, tree)
    });
    let (logic_w, logic_h) = (sc.logic_w.max(1.0), sc.logic_h.max(1.0));
    for (i, child) in layer.children.iter_mut().enumerate() {
        let bounds = bounds.clone();
        let tooltip = child
            .get_attr("data-slot")
            .is_some_and(|s| s == "chart-tooltip");
        let crosshair = sc.crosshair;
        let previous = child.reactive_sync.take();
        *child = std::mem::take(child).reactive_sync(move |el, tree| {
            let status = previous
                .as_ref()
                .map_or(LayoutStatus::Clean, |sync| sync.run(el, tree));
            let hov = sig.peek();
            if hov == ChartHover::default() || (hov.index.is_none() && selected.is_some()) {
                return status;
            }
            let [bx, by, bw, bh] = bounds.get();
            if bw <= 0.0 || bh <= 0.0 {
                return status;
            }
            let x = (hov.x / logic_w).clamp(0.0, 1.0) * bw;
            let y = (hov.y / logic_h).clamp(0.0, 1.0) * bh;
            let [ex, ey, ew, eh] = el.layout_bounds;
            let target = if tooltip {
                let gap = TOOLTIP_GAP_PX;
                let margin = TOOLTIP_MARGIN_PX;
                let flip_x = ew > bw - x - gap - margin && x >= bw - x;
                let flip_y = eh > bh - y - gap - margin && y >= bh - y;
                [
                    bx + x + if flip_x { -ew - gap } else { gap },
                    by + y + if flip_y { -eh - gap } else { gap },
                ]
            } else if crosshair && i == 0 {
                [bx + x, ey]
            } else if crosshair && i == 1 {
                [ex, by + y]
            } else if crosshair && i == 2 {
                [bx + x - ew / 2.0, by + y - eh / 2.0]
            } else {
                return status;
            };
            let translate = [target[0] - ex, target[1] - ey];
            if el.style.translate == translate {
                return status;
            }
            el.style.translate = translate;
            status.merge(LayoutStatus::StyleChanged)
        });
    }
    layer
}

#[derive(Clone)]
struct TooltipScene {
    marks: Vec<ChartMark>,
    theme: ChartTheme,
    config: ChartConfig,
    scales: super::scale::ResolvedScales,
    logic_w: f32,
    logic_h: f32,
    tooltip: bool,
    tooltip_indicator: TooltipIndicator,
    crosshair: bool,
    polar: bool,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
    round_bar_base: bool,
    tooltip_chrome: SlotChrome,
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
        if builder.hover_signal.is_none() {
            let key = chart_runtime_key(&builder);
            builder.hover_signal = Some(use_scoped_signal(
                &format!("{key}::hov"),
                ChartHover::default,
            ));
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
        if let Some(s) = b.hover_signal {
            b.hovered = s.peek().index;
        }
        return if b.polar() {
            compile_polar(b)
        } else {
            compile_cartesian(b)
        };
    }
    let sig = builder.selected_signal.expect("selected_signal");
    let hov = builder.hover_signal.expect("hover_signal");
    let key = chart_runtime_key(&builder);
    // `__view` 让 layout 期 reconcile miss 能走 deferred remount（否则缩放后图卡死直到点击）。
    let view_id = format!("{key}__view");
    let tip_id = format!("{key}__tip__view");
    let remount_key = format!("{key}__remount");
    let tip_remount_key = format!("{key}__tip__remount");
    let size_sig = use_scoped_signal(&format!("{key}::sz"), || (0.0f32, 0.0f32));
    let plot_gen = use_scoped_signal(&format!("{key}::gen"), || 0u32);
    let scene: Rc<RefCell<Option<TooltipScene>>> = Rc::new(RefCell::new(None));
    let fluid = builder.height_fluid;
    let shell_h = chart_shell_h(&builder);
    let wants_overlay = builder.tooltip || builder.crosshair;

    let scene_plot = scene.clone();
    let tip_for_plot = tip_id.clone();
    crate::app::reactive::reactive_anchor(view_id.clone(), move || {
        let _ = use_signal(remount_key.as_str(), || 0u32).get();
        let mut b = builder.clone();
        b.selected = sig.get();
        // hover 不进此 effect：只重建 overlay，避免整图 raster。
        let (lw, lh) = size_sig.get();
        if lw >= 16.0 {
            b.layout_w = Some(lw);
        }
        if lh >= 16.0 {
            b.layout_h = Some(lh);
        }
        b.plot_size_signal = Some(size_sig);
        b.hover_signal = Some(hov);
        b.tooltip_scene = Some(scene_plot.clone());
        if wants_overlay {
            b.overlay_key = Some(tip_for_plot.clone());
        }
        // A fluid cartesian chart is first compiled before its real plot size
        // is known. Keep that bootstrap raster hidden so it cannot flash at
        // the default width for one frame when the chart enters the viewport.
        let polar = b.polar();
        let measured = polar || (lw >= 16.0 && lh >= 16.0);
        let mut el = if polar {
            compile_polar(b)
        } else {
            compile_cartesian(b)
        };
        if !measured {
            el.style.opacity = 0.0;
        }
        // The image reconcile will be replaced with a new overlay placeholder; the bump will allow the tooltip island to fill in the content afterwards.
        plot_gen.set(plot_gen.peek().wrapping_add(1));
        el
    });

    if wants_overlay {
        let content = create_memo(move || {
            let hover = hov.get();
            (hover.index, hover == ChartHover::default())
        });
        crate::app::reactive::reactive_anchor(tip_id, move || {
            let _ = use_signal(tip_remount_key.as_str(), || 0u32).get();
            let _ = plot_gen.get();
            let _ = content.get();
            let hov_now = hov.peek();
            let selected = sig.peek();
            let sc = scene.borrow();
            let Some(sc) = sc.as_ref() else {
                return attach_overlay_scroll_hide(empty_overlay_layer(), hov);
            };
            attach_overlay_pointer_sync(
                overlay_from_pointer(hov_now, selected, sc),
                hov,
                selected,
                sc,
            )
        });
    }

    let ph_h: String = if fluid {
        "100%".into()
    } else {
        format!("{shell_h:.0}px")
    };
    crate::app::reactive::reactive_placeholder_sized(view_id, "100%", ph_h)
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
    config: &ChartConfig,
) -> ChartPin {
    let i = selected?;
    let mark = marks.iter().rev().find(|m| m.hover_item(i).is_some())?;
    let mut item = mark.hover_item(i)?;
    // config 只改行标签（系列名）；顶栏 header（类目）保留。
    if let Some(key) = item.series_key.as_deref() {
        if let Some(lbl) = config.label_for(key) {
            item.label = lbl.to_string();
        }
    } else if let Some(lbl) = config.label_for(&item.label) {
        item.label = lbl.to_string();
    }
    let color = mark
        .tooltip_color(ctx, i)
        .or_else(|| {
            item.series_key
                .as_deref()
                .and_then(|k| config.color_for(k))
                .or_else(|| config.color_for(&item.label))
        })
        .unwrap_or_else(|| ctx.theme.color(i));
    let pt = mark
        .datum_point(ctx, i)
        .or_else(|| marks.iter().rev().find_map(|m| m.datum_point(ctx, i)))?;
    Some(ChartPinData {
        x: pt.0,
        y: pt.1,
        item,
        color,
    })
}

impl From<ChartDefinition> for Element {
    fn from(d: ChartDefinition) -> Self {
        d.0.into()
    }
}

struct ChartPinData {
    x: f32,
    y: f32,
    item: ChartTooltipItem,
    color: [f32; 4],
}

type ChartPin = Option<ChartPinData>;

const TOOLTIP_GAP_PX: f32 = 6.0;
const TOOLTIP_MARGIN_PX: f32 = 4.0;

fn estimate_tooltip_px(item: &ChartTooltipItem) -> (f32, f32) {
    let label_w = item.label.chars().count() as f32 * 6.5;
    let value_w = item.display_value().chars().count() as f32 * 7.0;
    let header_w = item
        .header
        .as_ref()
        .map(|h| h.chars().count() as f32 * 6.5)
        .unwrap_or(0.0);
    let tw = (label_w + value_w + 36.0)
        .max(header_w + 16.0)
        .clamp(96.0, 280.0);
    let th = if item.header.is_some() { 44.0 } else { 32.0 };
    (tw, th)
}

/// 画布内翻转：默认指针右下；不足则翻到左/上。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TooltipPinPlace {
    pub flip_x: bool,
    pub flip_y: bool,
}

/// 默认锚在指针右下方；空间不足时翻到左/上方。
pub(crate) fn tooltip_pin_place(
    lx: f32,
    ly: f32,
    logic_w: f32,
    logic_h: f32,
    item: &ChartTooltipItem,
) -> TooltipPinPlace {
    let (tw, th) = estimate_tooltip_px(item);
    let w = logic_w.max(1.0);
    let h = logic_h.max(1.0);
    let gap = TOOLTIP_GAP_PX + TOOLTIP_MARGIN_PX;
    let room_right = (w - lx - gap).max(0.0);
    let room_left = (lx - gap).max(0.0);
    let room_above = (ly - gap).max(0.0);
    let room_below = (h - ly - gap).max(0.0);
    TooltipPinPlace {
        flip_x: tw > room_right && room_left >= room_right,
        flip_y: th > room_below && room_above >= room_below,
    }
}

fn tooltip_indicator_el(indicator: TooltipIndicator, color: [f32; 4]) -> Element {
    // shadcn: dot `h-2.5 w-2.5`, line `w-1`, dashed border; `rounded-[2px]`.
    match indicator {
        TooltipIndicator::Dot => {
            let el: Element = r#box()
                .w(10.0)
                .h(10.0)
                .min_w(10.0)
                .min_h(10.0)
                .rounded("2px")
                .flex_shrink(0.0)
                .into();
            el.bg(color)
        }
        TooltipIndicator::Line => {
            let el: Element = r#box()
                .w(4.0)
                .h(12.0)
                .min_w(4.0)
                .min_h(12.0)
                .rounded("2px")
                .flex_shrink(0.0)
                .into();
            el.bg(color)
        }
        TooltipIndicator::Dashed => {
            let el: Element = r#box()
                .w(0.0)
                .h(12.0)
                .min_w(0.0)
                .min_h(12.0)
                .border_style("dashed")
                .border_width(1.5)
                .border_color(color)
                .flex_shrink(0.0)
                .into();
            el
        }
    }
}

/// shadcn `ChartTooltipContent` 外壳：紧凑圆角矩形。不用 path 直角贴图盖住 radii。
fn chart_tooltip_bubble(body: Element, chrome: &SlotChrome) -> Element {
    let mut panel: Element = r#box()
        .px("0.5rem")
        .py("0.25rem")
        .rounded("0.5rem")
        .overflow("hidden")
        .min_w(96.0)
        .max_w("18rem")
        .into();
    panel = panel
        .bg(tokens::background())
        .border_style("solid")
        .border_width(1.0)
        .border_color(parse_color("border/50"))
        .popup_shadow();
    paint_slot(panel.child(body), "chart-tooltip", chrome)
}

/// shadcn `ChartTooltipContent`：可选顶栏 + indicator + muted label + mono value。
fn chart_tooltip_content(
    item: &ChartTooltipItem,
    color: [f32; 4],
    indicator: TooltipIndicator,
) -> Element {
    let indicator_el = tooltip_indicator_el(indicator, color);
    let label = build_text_node(
        item.label.clone(),
        TextNodeConfig {
            size_px: 12.0,
            weight: 400,
            color: parse_text_color("muted-foreground"),
            ..Default::default()
        },
    );
    let value = build_text_node(
        item.display_value(),
        TextNodeConfig {
            size_px: 12.0,
            weight: 500,
            color: parse_text_color("foreground"),
            font_family: Some("ui-monospace".into()),
            ..Default::default()
        },
    )
    // Tooltip values are atomic readouts.  Letting the flex row shrink this leaf
    // makes glyphon wrap the last digit into a second visual row.
    .whitespace_nowrap()
    .flex_shrink(0.0);
    let row: Element = hstack()
        .gap("0.5rem")
        .items_center()
        .w("100%")
        .child(indicator_el)
        .child(
            hstack()
                .gap("0.75rem")
                .items_center()
                .flex_grow(1.0)
                .justify("between")
                .child(label)
                .child(value),
        )
        .into();
    if let Some(header) = item.header.as_ref().filter(|h| !h.is_empty()) {
        let title = build_text_node(
            header.clone(),
            TextNodeConfig {
                size_px: 12.0,
                weight: 500,
                color: parse_text_color("foreground"),
                ..Default::default()
            },
        );
        vstack()
            .gap("0.375rem")
            .items("start")
            .w("100%")
            .child(title)
            .child(row)
            .into()
    } else {
        row
    }
}

fn pin_overlay_layer(
    pin: ChartPin,
    theme: &ChartTheme,
    logic_w: f32,
    logic_h: f32,
    tooltip: bool,
    tooltip_indicator: TooltipIndicator,
    crosshair: bool,
    polar: bool,
    tooltip_chrome: &SlotChrome,
) -> Element {
    let mut layer: Element = r#box()
        .w("100%")
        .h("100%")
        .absolute(true)
        .left(0.0)
        .top(0.0)
        .pointer_events("none")
        .into();
    // Local chart overlays move every pointer frame. Mark the whole retained
    // overlay subtree so damage can replay the chart page band underneath the
    // old and new positions instead of doing a white node dest-reset.
    layer.set_attr("rui_chart_overlay", "1");
    let Some(pin) = pin else {
        return layer;
    };
    let lx = pin.x;
    let ly = pin.y;
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
        v.set_attr("rui_chart_overlay", "1");
        v = v.bg(theme.grid);
        let mut hline: Element = r#box()
            .h(1.0)
            .w("100%")
            .absolute(true)
            .left(0.0)
            .top(format!("{y_pct:.2}%"))
            .pointer_events("none")
            .into();
        hline.set_attr("rui_chart_overlay", "1");
        hline = hline.bg(theme.grid);
        layer = layer.child(v).child(hline);
        if polar {
            let mut dot: Element = r#box()
                .w(6.0)
                .h(6.0)
                .rounded_full()
                .absolute(true)
                .left(format!("calc({x_pct:.2}% - 3px)"))
                .top(format!("calc({y_pct:.2}% - 3px)"))
                .pointer_events("none")
                .into();
            dot.set_attr("rui_chart_overlay", "1");
            dot = dot.bg(pin.color);
            layer = layer.child(dot);
        }
    }
    if tooltip && !pin.item.label.is_empty() {
        let place = tooltip_pin_place(lx, ly, logic_w, logic_h, &pin.item);
        let gap = TOOLTIP_GAP_PX;
        let body = chart_tooltip_content(&pin.item, pin.color, tooltip_indicator);
        let mut tip = chart_tooltip_bubble(body, tooltip_chrome);
        tip.set_attr("rui_chart_overlay", "1");
        tip = tip.absolute().pointer_events("none");
        if place.flip_x {
            let from_right = (100.0 - x_pct).clamp(0.0, 100.0);
            tip = tip.right(format!("calc({from_right:.2}% + {gap}px)"));
        } else {
            tip = tip.left(format!("calc({x_pct:.2}% + {gap}px)"));
        }
        if place.flip_y {
            // 下方空间不足：翻到指针上方。
            let from_bottom = (100.0 - y_pct).clamp(0.0, 100.0);
            tip = tip.bottom(format!("calc({from_bottom:.2}% + {gap}px)"));
        } else {
            // 默认：指针右下方。
            tip = tip.top(format!("calc({y_pct:.2}% + {gap}px)"));
        }
        let room_w = if place.flip_x {
            (lx - gap - TOOLTIP_MARGIN_PX).max(96.0)
        } else {
            (logic_w - lx - gap - TOOLTIP_MARGIN_PX).max(96.0)
        };
        tip = tip.max_w(format!("{room_w}px"));
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
    tooltip_indicator: TooltipIndicator,
    crosshair: bool,
    polar: bool,
    tooltip_chrome: &SlotChrome,
) -> Element {
    if pin.is_none() && !crosshair {
        return plot;
    }
    plot.child(pin_overlay_layer(
        pin,
        theme,
        logic_w,
        logic_h,
        tooltip,
        tooltip_indicator,
        crosshair,
        polar,
        tooltip_chrome,
    ))
}

fn attach_overlay_slot(plot: Element, overlay_key: String) -> Element {
    let mut ph = crate::app::reactive::reactive_placeholder_sized(overlay_key, "100%", "100%");
    ph = ph.absolute().left(0.0).top(0.0).pointer_events("none");
    plot.child(ph)
}

fn empty_overlay_layer() -> Element {
    r#box()
        .w("100%")
        .h("100%")
        .absolute(true)
        .left(0.0)
        .top(0.0)
        .pointer_events("none")
        .into()
}

fn overlay_from_pointer(hov: ChartHover, selected: Option<usize>, sc: &TooltipScene) -> Element {
    let ctx = make_ctx(
        &sc.scales,
        &sc.theme,
        None,
        if sc.polar { sc.logic_w } else { sc.logic_h },
        sc.polar_start,
        sc.polar_end,
        sc.polar_inner,
        sc.round_bar_base,
    );
    let pin = if let Some(i) = hov.index {
        let mut pin = selection_pin(&sc.marks, &ctx, Some(i), &sc.config);
        if let Some(ref mut d) = pin {
            d.x = hov.x;
            d.y = hov.y;
        }
        pin
    } else if let Some(i) = selected {
        selection_pin(&sc.marks, &ctx, Some(i), &sc.config)
    } else if sc.crosshair && hov != ChartHover::default() {
        Some(ChartPinData {
            x: hov.x,
            y: hov.y,
            item: ChartTooltipItem {
                label: String::new(),
                value: 0.0,
                series_key: None,
                header: None,
            },
            color: sc.theme.grid,
        })
    } else {
        None
    };
    pin_overlay_layer(
        pin,
        &sc.theme,
        sc.logic_w,
        sc.logic_h,
        sc.tooltip,
        sc.tooltip_indicator,
        sc.crosshair,
        sc.polar,
        &sc.tooltip_chrome,
    )
}

fn store_tooltip_scene(cell: &Option<Rc<RefCell<Option<TooltipScene>>>>, scene: TooltipScene) {
    if let Some(cell) = cell {
        *cell.borrow_mut() = Some(scene);
    }
}

fn snap_plot_px(v: f32) -> f32 {
    (v / 4.0).round() * 4.0
}

fn apply_chart_hover_from_client(
    marks: &[ChartMark],
    scales: &super::scale::ResolvedScales,
    theme: &ChartTheme,
    selected: Option<usize>,
    polar_size: f32,
    polar_start: f32,
    polar_end: f32,
    polar_inner: f32,
    logic_w: f32,
    logic_h: f32,
    round_bar_base: bool,
    hover_signal: Signal<ChartHover>,
    client_x: f32,
    client_y: f32,
    visual_bounds: [f32; 4],
) {
    if visual_bounds[2] <= 0.5 || visual_bounds[3] <= 0.5 || marks.is_empty() {
        return;
    }
    let geom = [
        client_x,
        client_y,
        visual_bounds[0],
        visual_bounds[1],
        visual_bounds[2],
        visual_bounds[3],
    ];
    let unchanged = LAST_HOVER_GEOM.with(|m| {
        m.borrow().get(&hover_signal.id()).is_some_and(|prev| {
            prev.iter()
                .zip(geom.iter())
                .all(|(a, b)| (a - b).abs() < 0.5)
        })
    });
    if unchanged {
        return;
    }
    LAST_HOVER_GEOM.with(|m| {
        m.borrow_mut().insert(hover_signal.id(), geom);
    });
    let lx = (client_x - visual_bounds[0]) / visual_bounds[2] * logic_w;
    let ly = (client_y - visual_bounds[1]) / visual_bounds[3] * logic_h;
    let ctx = make_ctx(
        scales,
        theme,
        selected,
        polar_size,
        polar_start,
        polar_end,
        polar_inner,
        round_bar_base,
    );
    let mut hit = None;
    for m in marks.iter().rev() {
        if let Some(i) = m.hit_index(&ctx, (lx, ly)) {
            hit = Some(i);
            break;
        }
    }
    assign_chart_hover(
        hover_signal,
        ChartHover {
            x: lx.round(),
            y: ly.round(),
            index: hit,
        },
    );
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
    hover_signal: Option<Signal<ChartHover>>,
    plot_size_signal: Option<Signal<(f32, f32)>>,
    track_hover: bool,
    round_bar_base: bool,
) -> Element {
    let bounds = Rc::new(Cell::new([0.0_f32; 4]));
    let interactive = on_select.is_some() || selected_signal.is_some();
    let hit_enabled = interactive || track_hover;
    if hit_enabled {
        if interactive {
            plot = plot.cursor_kind(CursorKind::Pointer);
        }
        let marks_hit = marks.clone();
        let scales_hit = scales.clone();
        let theme_hit = theme.clone();
        let bounds_pd = bounds.clone();
        if interactive {
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
                    round_bar_base,
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
        if track_hover {
            let marks_mv = marks.clone();
            let scales_mv = scales.clone();
            let theme_mv = theme.clone();
            let hover_inside = Rc::new(Cell::new(false));
            plot = plot.on_mousemove({
                let marks_mv = marks_mv.clone();
                let scales_mv = scales_mv.clone();
                let theme_mv = theme_mv.clone();
                let hover_inside = hover_inside.clone();
                move |e| {
                    if e.phase == EventPhase::Capture || marks_mv.is_empty() {
                        return;
                    }
                    let b = if e.current_bounds[2] > 0.0 && e.current_bounds[3] > 0.0 {
                        e.current_bounds
                    } else {
                        return;
                    };
                    hover_inside.set(true);
                    if let Some(s) = hover_signal {
                        apply_chart_hover_from_client(
                            &marks_mv,
                            &scales_mv,
                            &theme_mv,
                            selected,
                            polar_size,
                            polar_start,
                            polar_end,
                            polar_inner,
                            logic_w,
                            logic_h,
                            round_bar_base,
                            s,
                            e.x,
                            e.y,
                            b,
                        );
                    }
                }
            });
            if let Some(s) = hover_signal {
                let marks_h = marks_mv.clone();
                let scales_h = scales_mv.clone();
                let theme_h = theme_mv.clone();
                let hover_inside_h = hover_inside.clone();
                plot = plot.on_hover(move |inside| {
                    hover_inside_h.set(inside);
                    if inside {
                        if let Some((pos, b)) = crate::app::pointer_events::last_hover_sample() {
                            apply_chart_hover_from_client(
                                &marks_h,
                                &scales_h,
                                &theme_h,
                                selected,
                                polar_size,
                                polar_start,
                                polar_end,
                                polar_inner,
                                logic_w,
                                logic_h,
                                round_bar_base,
                                s,
                                pos[0],
                                pos[1],
                                b,
                            );
                        }
                    } else {
                        assign_chart_hover(s, ChartHover::default());
                    }
                });
            }
            let bounds_sync = bounds.clone();
            let hover_inside_sync = hover_inside;
            let marks_sync = marks_mv;
            let scales_sync = scales_mv;
            let theme_sync = theme_mv;
            plot = plot.reactive_sync(move |root, _| {
                bounds_sync.set(root.layout_bounds);
                if let Some(sz) = plot_size_signal {
                    let w = snap_plot_px(root.layout_bounds[2]);
                    let h = snap_plot_px(root.layout_bounds[3]);
                    if w >= 16.0 && h >= 16.0 {
                        let prev = sz.try_peek().unwrap_or((0.0, 0.0));
                        if (prev.0 - w).abs() >= 0.5 || (prev.1 - h).abs() >= 0.5 {
                            let _ = sz.try_set((w, h));
                        }
                    }
                }
                if hover_inside_sync.get() {
                    if let (Some(s), Some((pos, b))) = (
                        hover_signal,
                        crate::app::pointer_events::last_hover_sample(),
                    ) {
                        apply_chart_hover_from_client(
                            &marks_sync,
                            &scales_sync,
                            &theme_sync,
                            selected,
                            polar_size,
                            polar_start,
                            polar_end,
                            polar_inner,
                            logic_w,
                            logic_h,
                            round_bar_base,
                            s,
                            pos[0],
                            pos[1],
                            b,
                        );
                    }
                }
                LayoutStatus::Clean
            });
            return plot;
        }
    }
    let bounds_sync = bounds;
    plot.reactive_sync(move |root, _| {
        bounds_sync.set(root.layout_bounds);
        if let Some(sz) = plot_size_signal {
            let w = snap_plot_px(root.layout_bounds[2]);
            let h = snap_plot_px(root.layout_bounds[3]);
            if w >= 16.0 && h >= 16.0 {
                let prev = sz.try_peek().unwrap_or((0.0, 0.0));
                if (prev.0 - w).abs() >= 0.5 || (prev.1 - h).abs() >= 0.5 {
                    let _ = sz.try_set((w, h));
                }
            }
        }
        LayoutStatus::Clean
    })
}

fn compile_cartesian(builder: ChartBuilder) -> Element {
    let hover_signal = builder.hover_signal;
    let tooltip_scene = builder.tooltip_scene.clone();
    let overlay_key = builder.overlay_key.clone();
    let tooltip_chrome = builder.slots.get("chart-tooltip");
    let axis_chrome = builder.slots.get("chart-axis");
    let ChartBuilder {
        id,
        marks,
        x,
        y,
        mut theme,
        config,
        guides,
        legend: _,
        axis_labels,
        height,
        width,
        polar_start,
        polar_end,
        polar_inner,
        tooltip,
        tooltip_indicator,
        crosshair,
        selected,
        selected_signal,
        hovered,
        layout_w,
        layout_h,
        plot_size_signal,
        height_fluid,
        on_select,
        ..
    } = builder;
    config.apply_palette_overrides(&mut theme);
    let has_bar_y = marks.iter().any(|m| m.kind() == "barY");
    let has_bar = has_bar_y || marks.iter().any(|m| m.kind() == "barX");
    let h = layout_h.unwrap_or(height).max(16.0);
    let w_logic = layout_w.or(width).unwrap_or(320.0);
    let round_bar_base = !guides;
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

    let pad = [8.0, 8.0, 8.0, if has_bar_y { 0.0 } else { 8.0 }];
    let scales = resolve_scales_pad(
        w_logic,
        h,
        &x_spec,
        &y_spec,
        x_cats.clone(),
        &x_nums,
        &x_times,
        &y_nums,
        y_zero || has_bar,
        pad,
    );
    let ctx = make_ctx(
        &scales,
        &theme,
        selected,
        h,
        polar_start,
        polar_end,
        polar_inner,
        round_bar_base,
    );
    let mut layers = Vec::new();
    for m in &marks {
        layers.extend(m.render(&ctx));
    }
    let (mut path_layers, markers, marker_r) = compile_layers(&layers);
    push_selected_vertex(&mut path_layers, &markers, &marks, selected, &theme);
    store_tooltip_scene(
        &tooltip_scene,
        TooltipScene {
            marks: marks.clone(),
            theme: theme.clone(),
            config: config.clone(),
            scales: scales.clone(),
            logic_w: w_logic,
            logic_h: h,
            tooltip,
            tooltip_indicator,
            crosshair,
            polar: false,
            polar_start,
            polar_end,
            polar_inner,
            round_bar_base,
            tooltip_chrome: tooltip_chrome.clone(),
        },
    );
    let pin_index = if overlay_key.is_some() {
        None
    } else if tooltip || crosshair {
        hovered.or(selected)
    } else {
        None
    };
    let pin = if pin_index.is_some() {
        selection_pin(&marks, &ctx, pin_index, &config)
    } else {
        None
    };
    let mut hit: Element = r#box().w("100%").h("100%").relative(true).into();
    // path 贴图挂在 hit 上（而非 frame）：否则绝对定位网格会盖住 bar。
    hit = attach_chart_scene(hit, &path_layers, &markers, marker_r, w_logic, h);
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
        hover_signal,
        plot_size_signal,
        tooltip || crosshair,
        round_bar_base,
    );

    let mut plot = cartesian_frame(h, height_fluid, &theme, guides, y_spec.grid || guides, hit);
    plot.set_attr("rui_chart", kind_attr(&marks));
    if let Some(tip_key) = overlay_key {
        plot = attach_overlay_slot(plot, tip_key);
    } else {
        plot = attach_pin_overlay(
            plot,
            pin,
            &theme,
            w_logic,
            h,
            tooltip,
            tooltip_indicator,
            crosshair,
            false,
            &tooltip_chrome,
        );
    }

    // 仅用户显式 width_px 时锁宽；默认 100% 由 layout_w 重算，避免贴图横向拉伸。
    if let Some(w) = width {
        if layout_w.is_none() {
            plot = r#box().w(format!("{}px", w.max(16.0))).child(plot).into();
        }
    }
    plot = plot.flex_shrink(0.0);

    let mut root = plot;
    let time_labels =
        scales.x_kind == ScaleKind::Time && (axis_labels || scales.x_time_domain.is_some());
    if axis_labels && !x_cats.is_empty() {
        let ticks: Vec<(f32, String)> = x_cats
            .iter()
            .enumerate()
            .map(|(i, lab)| (scales.x_center(i), lab.clone()))
            .collect();
        let mut labels = tick_label_row(&ticks, scales.plot_w);
        labels.set_attr("data-slot", "chart-axis");
        let labels = axis_chrome.apply(labels);
        root = vstack()
            .w("100%")
            .h(if height_fluid { "100%" } else { "auto" })
            .flex_shrink(0.0)
            .gap("0.5rem")
            .child(root)
            .child(labels)
            .into();
    } else if time_labels {
        if let Some((t0, t1)) = scales.x_time_domain {
            let span = (t1 - t0).max(1);
            let n = 3usize;
            let ticks: Vec<(f32, String)> = (0..=n)
                .map(|k| {
                    let t = t0 + span * k as i64 / n as i64;
                    (scales.x_time(t), format_time_label(t, span))
                })
                .collect();
            let mut labels = tick_label_row(&ticks, scales.plot_w);
            labels.set_attr("data-slot", "chart-axis");
            let labels = axis_chrome.apply(labels);
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

/// Polar marks share one raster scene, so their semantic z-order must not
/// depend on the order in which a caller happened to provide the builders.
/// Filled bars/areas stay underneath strokes, and dots are the final ink.
fn polar_mark_order(marks: &[ChartMark]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by_key(|&i| match marks[i].kind() {
        "radialBar" | "pie" => 0u8,
        "radialArea" => 1,
        "radialLine" => 2,
        "radialDot" => 3,
        _ => 2,
    });
    order
}

fn compile_polar(builder: ChartBuilder) -> Element {
    let hover_signal = builder.hover_signal;
    let tooltip_scene = builder.tooltip_scene.clone();
    let overlay_key = builder.overlay_key.clone();
    let tooltip_chrome = builder.slots.get("chart-tooltip");
    let legend_chrome = builder.slots.get("chart-legend");
    let ChartBuilder {
        id,
        marks,
        y,
        mut theme,
        config,
        guides,
        legend,
        polar_size,
        polar_start,
        polar_end,
        polar_inner,
        tooltip,
        tooltip_indicator,
        crosshair,
        selected,
        selected_signal,
        hovered,
        plot_size_signal,
        on_select,
        ..
    } = builder;
    config.apply_palette_overrides(&mut theme);
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
        false,
    );
    let mut layers = Vec::new();
    let pie_only = marks.iter().all(|m| m.kind() == "pie");
    if guides && !pie_only {
        layers.extend(polar_guide_layers(&ctx));
    }
    for i in polar_mark_order(&marks) {
        layers.extend(marks[i].render(&ctx));
    }
    let (mut path_layers, markers, marker_r) = compile_layers(&layers);
    push_selected_vertex(&mut path_layers, &markers, &marks, selected, &theme);
    store_tooltip_scene(
        &tooltip_scene,
        TooltipScene {
            marks: marks.clone(),
            theme: theme.clone(),
            config: config.clone(),
            scales: scales.clone(),
            logic_w: s,
            logic_h: s,
            tooltip,
            tooltip_indicator,
            crosshair,
            polar: true,
            polar_start,
            polar_end,
            polar_inner,
            round_bar_base: false,
            tooltip_chrome: tooltip_chrome.clone(),
        },
    );
    let pin_index = if overlay_key.is_some() {
        None
    } else if tooltip || crosshair {
        hovered.or(selected)
    } else {
        None
    };
    let pin = if pin_index.is_some() {
        selection_pin(&marks, &ctx, pin_index, &config)
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
        .overflow("visible")
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
        hover_signal,
        plot_size_signal,
        tooltip || crosshair,
        false,
    );
    if let Some(tip_key) = overlay_key {
        plot = attach_overlay_slot(plot, tip_key);
    } else {
        plot = attach_pin_overlay(
            plot,
            pin,
            &theme,
            s,
            s,
            tooltip,
            tooltip_indicator,
            crosshair,
            true,
            &tooltip_chrome,
        );
    }

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
    let mut legend_el = pie_legend_rows(&pairs, &theme, &config, selected);
    legend_el.set_attr("data-slot", "chart-legend");
    let legend_el = legend_chrome.apply(legend_el);
    // shadcn ChartLegendContent: Horizontal legend below the chart (not sidebar).
    let root: Element = vstack()
        .gap("0.75rem")
        .items_center()
        .flex_shrink(0.0)
        .child(plot)
        .child(legend_el)
        .into();
    apply_optional_id(root, &id)
}

pub(crate) fn pie_legend_rows(
    labels: &[(String, f32)],
    theme: &ChartTheme,
    config: &ChartConfig,
    selected: Option<usize>,
) -> Element {
    // shadcn ChartLegendContent: horizontal flex + 8×8 rounded-[2px] color block.
    let mut legend = hstack().gap("1rem").wrap("wrap").flex_shrink(0.0);
    for (i, (lab, _val)) in labels.iter().enumerate() {
        let display = config.label_for(lab).unwrap_or(lab.as_str());
        let color = config.color_for(lab).unwrap_or_else(|| theme.color(i));
        let mut swatch: Element = r#box()
            .w(8.0)
            .h(8.0)
            .min_w(8.0)
            .min_h(8.0)
            .rounded("0.125rem")
            .into();
        swatch = swatch
            .bg(theme.dim_if_unselected(color, i, selected))
            .flex_shrink(0.0);
        let label = caption(display);
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

#[cfg(test)]
mod hover_freeze_tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_signal};

    #[test]
    fn tooltip_regression_decimal_stays_on_one_line() {
        let item = ChartTooltipItem {
            label: "Tuesday".into(),
            value: 0.52,
            series_key: None,
            header: None,
        };
        let body = chart_tooltip_content(&item, [0.0, 0.0, 1.0, 1.0], TooltipIndicator::Dot);
        let mut chart = chart_tooltip_bubble(body, &SlotChrome::default())
            .w(96.0)
            .max_w(96.0);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut chart, 1360.0, 200.0, [-1.0, -1.0]);
        for text in ["0.52"] {
            let value = result.text_nodes.iter().find(|t| t.text == text).unwrap();
            assert_eq!(
                value.style.white_space,
                crate::style::WhiteSpace::Nowrap,
                "tooltip {text} must stay on one visual row"
            );
            let prepared = value.prepared.as_ref().expect("measured tooltip text");
            let metrics = crate::text::layout(prepared, value.width);
            let intrinsic = crate::text::layout(prepared, f32::MAX).width;
            assert!(
                value.width + 0.01 >= intrinsic,
                "tooltip {text} is squeezed: {} < {intrinsic}",
                value.width
            );
            assert_eq!(
                metrics.line_count, 1,
                "tooltip {text} wraps at width {}",
                value.width
            );
        }
    }

    #[test]
    fn tooltip_regression_pointer_moves_do_not_rebuild() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, take_flushed_effects, EffectPhase};

        reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let (owner, hov) = create_root({
            let live = live.clone();
            move || {
                let hov = use_signal("tooltip-regression-move", || ChartHover {
                    x: 30.0,
                    y: 20.0,
                    index: Some(0),
                });
                create_effect(move || {
                    let mut builder = define_chart()
                        .id("tooltip-regression")
                        .mark(super::super::mark::bar_y([("Tue", 0.52), ("Wed", 0.8)]))
                        .tooltip(true);
                    builder.hover_signal = Some(hov);
                    *live.borrow_mut() = builder.into();
                });
                hov
            }
        });
        drain_deferred_view_remounts();
        let _ = take_flushed_effects();
        assign_chart_hover(
            hov,
            ChartHover {
                x: 50.0,
                y: 30.0,
                index: Some(0),
            },
        );
        let rebuilds = take_flushed_effects()
            .into_iter()
            .filter(|(_, phase)| *phase == EffectPhase::Render)
            .count();
        assign_chart_hover(
            hov,
            ChartHover {
                x: 70.0,
                y: 30.0,
                index: Some(1),
            },
        );
        let datum_rebuilds = take_flushed_effects()
            .into_iter()
            .filter(|(_, phase)| *phase == EffectPhase::Render)
            .count();
        let mut updated_value = false;
        crate::core::tree::walk_elements(&live.borrow(), |el| {
            updated_value |= el.text.as_deref() == Some("0.80");
        });
        assign_chart_hover(hov, ChartHover::default());
        let mut tooltip_visible = false;
        crate::core::tree::walk_elements(&live.borrow(), |el| {
            tooltip_visible |= el
                .get_attr("data-slot")
                .is_some_and(|s| s == "chart-tooltip");
        });
        clear_test_reconcile_root();
        owner.dispose();
        reset_registered_anchors();
        assert_eq!(
            rebuilds, 0,
            "moving within a datum must only update overlay position"
        );
        assert!(
            datum_rebuilds > 0 && updated_value,
            "changing datum must refresh the content"
        );
        assert!(
            !tooltip_visible,
            "leaving must remove an unselected tooltip"
        );
    }

    #[test]
    fn tooltip_regression_pointer_sync_follows_and_flips_without_layout() {
        create_root(|| {
            let scene = Rc::new(RefCell::new(None));
            let mut builder = define_chart()
                .mark(super::super::mark::bar_y([("Tue", 0.52)]))
                .width_px(200.0)
                .plot_h(80.0)
                .tooltip(true)
                .crosshair(true);
            builder.tooltip_scene = Some(scene.clone());
            let _ = compile_cartesian(builder);
            let scene = scene.borrow();
            let sc = scene.as_ref().unwrap();
            let sig = use_signal("tooltip-sync-regression", || ChartHover {
                x: 30.0,
                y: 20.0,
                index: Some(0),
            });
            let layer = attach_overlay_pointer_sync(
                overlay_from_pointer(sig.peek(), None, sc),
                sig,
                None,
                sc,
            );
            let mut root: Element = r#box().w(200.0).h(80.0).relative(true).child(layer).into();
            let mut tree = crate::layout::layout_tree::LayoutTree::new();
            tree.build_and_layout(&mut root, 200.0, 80.0, [-1.0, -1.0]);
            let original = root.children[0].children[2].layout_bounds;
            for (x, y, flip) in [
                (50.0, 25.0, false),
                (190.0, 75.0, true),
                (40.0, 15.0, false),
            ] {
                assign_chart_hover(
                    sig,
                    ChartHover {
                        x,
                        y,
                        index: Some(0),
                    },
                );
                let mut impact = LayoutStatus::Clean;
                crate::core::tree::sync_reactive_elements_subtree(
                    &mut root,
                    &mut tree,
                    &mut impact,
                    200.0,
                    80.0,
                    [0.0, 0.0],
                );
                assert_eq!(impact, LayoutStatus::StyleChanged);
                let layer = &root.children[0];
                let tip = &layer.children[2];
                assert_eq!(
                    tip.layout_bounds, original,
                    "movement must retain the layout box"
                );
                let expected_x = x + if flip {
                    -original[2] - TOOLTIP_GAP_PX
                } else {
                    TOOLTIP_GAP_PX
                };
                let expected_y = y + if flip {
                    -original[3] - TOOLTIP_GAP_PX
                } else {
                    TOOLTIP_GAP_PX
                };
                assert!((tip.layout_bounds[0] + tip.style.translate[0] - expected_x).abs() < 0.01);
                assert!((tip.layout_bounds[1] + tip.style.translate[1] - expected_y).abs() < 0.01);
                assert!(
                    (layer.children[0].layout_bounds[0] + layer.children[0].style.translate[0] - x)
                        .abs()
                        < 0.01
                );
                assert!(
                    (layer.children[1].layout_bounds[1] + layer.children[1].style.translate[1] - y)
                        .abs()
                        < 0.01
                );
            }
        });
    }

    #[test]
    fn polar_marks_keep_fill_stroke_and_dot_z_order() {
        let rows = || {
            vec![
                super::super::datum::ChartDatum::new("N", 4.0),
                super::super::datum::ChartDatum::new("E", 2.0),
            ]
        };
        let marks = vec![
            ChartMark::from(super::super::polar::radial_dot(rows())),
            ChartMark::from(super::super::polar::radial_line(rows())),
            ChartMark::from(super::super::polar::radial_bar(rows())),
        ];
        let kinds: Vec<_> = polar_mark_order(&marks)
            .into_iter()
            .map(|i| marks[i].kind())
            .collect();
        assert_eq!(kinds, vec!["radialBar", "radialLine", "radialDot"]);
    }

    #[test]
    fn frozen_hover_does_not_write_until_unfreeze() {
        create_root(|| {
            set_chart_hover_frozen(false);
            let sig = use_signal("t-chart-hov-freeze", ChartHover::default);
            set_chart_hover_frozen(true);
            assign_chart_hover(
                sig,
                ChartHover {
                    x: 12.0,
                    y: 8.0,
                    index: Some(1),
                },
            );
            assert_eq!(
                sig.peek(),
                ChartHover::default(),
                "scroll-frozen hover must not remount the tooltip overlay"
            );
            set_chart_hover_frozen(false);
            assert_eq!(
                sig.peek().index,
                Some(1),
                "pending hover applies after scroll settles"
            );
        });
    }

    #[test]
    fn frozen_leave_clears_after_unfreeze() {
        create_root(|| {
            set_chart_hover_frozen(false);
            let sig = use_signal("t-chart-hov-leave", || ChartHover {
                x: 4.0,
                y: 4.0,
                index: Some(0),
            });
            set_chart_hover_frozen(true);
            assign_chart_hover(sig, ChartHover::default());
            assert_eq!(sig.peek().index, Some(0));
            assert!(
                overlay_scroll_hidden(sig.id()),
                "leave during scroll must hide the live overlay immediately"
            );
            set_chart_hover_frozen(false);
            assert!(!overlay_scroll_hidden(sig.id()));
            assert_eq!(sig.peek(), ChartHover::default());
        });
    }

    #[test]
    fn frozen_leave_hides_overlay_via_opacity_not_structure() {
        use crate::core::tree::{sync_reactive_elements_subtree, LayoutTree};
        create_root(|| {
            set_chart_hover_frozen(false);
            let sig = use_signal("t-chart-hov-hide-style", || ChartHover {
                x: 4.0,
                y: 4.0,
                index: Some(0),
            });
            let mut layer = attach_overlay_scroll_hide(r#box().w(40.0).h(40.0).into(), sig);
            layer.layout_bounds = [0.0, 0.0, 40.0, 40.0];
            set_chart_hover_frozen(true);
            assign_chart_hover(sig, ChartHover::default());
            let mut tree = LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            sync_reactive_elements_subtree(
                &mut layer,
                &mut tree,
                &mut impact,
                200.0,
                200.0,
                [0.0, 0.0],
            );
            assert!(
                (layer.style.opacity).abs() < 0.001,
                "leave during scroll must hide tooltip without remount"
            );
            assert_eq!(impact, LayoutStatus::StyleChanged);
            assert_eq!(sig.peek().index, Some(0));
            set_chart_hover_frozen(false);
        });
    }

    #[test]
    fn hover_enter_samples_datum_without_mousemove() {
        use super::super::mark::bar_y;
        use super::super::scale::{resolve_scales, AxisSpec};
        create_root(|| {
            set_chart_hover_frozen(false);
            let sig = use_signal("t-chart-hov-enter", ChartHover::default);
            let mark = ChartMark::from(bar_y([0.2f32, 0.9]));
            let scales = resolve_scales(
                200.0,
                80.0,
                &AxisSpec::band(),
                &AxisSpec::linear(),
                vec!["Mon".into(), "Tue".into()],
                &[],
                &[],
                &[0.2, 0.9],
                true,
            );
            let theme = ChartTheme::tokens();
            crate::app::pointer_events::note_pointer_client(150.0, 40.0);
            crate::app::pointer_events::note_hover_target_bounds([0.0, 0.0, 200.0, 80.0]);
            apply_chart_hover_from_client(
                &[mark],
                &scales,
                &theme,
                None,
                80.0,
                0.0,
                0.0,
                0.0,
                200.0,
                80.0,
                true,
                sig,
                150.0,
                40.0,
                [0.0, 0.0, 200.0, 80.0],
            );
            assert!(
                sig.peek().index.is_some(),
                "scroll-induced hover (no mousemove) must still pin a datum"
            );
        });
    }
}
