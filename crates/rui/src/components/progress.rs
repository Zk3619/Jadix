//! # Progress
//!
//! 对照 ShadCN **base** [`progress.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/progress.tsx)：
//!
//! ```text
//! Progress                 // Root：value ∈ 0..=1；自动挂 Track + Indicator
//! ├── ProgressLabel        // 可选标题
//! ├── ProgressValue        // 可选百分比（bind_text）
//! └── ProgressTrack        // cn-progress-track（也可单独使用）
//!     └── ProgressIndicator
//! ProgressCircle           // 圆形变体（Solid UI progress-circle 对位）
//! ```
//!
//! 轨道样式对位 `cn-progress-track` / `cn-progress-indicator`：`h-2` · `rounded-full` ·
//! `bg-primary/20` 轨 + `bg-primary` 指示条 · `w-full`。
//!
//! `ui!` 中 `<Progress>` 由宏展开为 [`progress_scope`]（子树先拿到上下文）。
//! Builder [`progress`] 简单条与 `.child(|| …)` 复合面共用同一套实现。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX（camelCase 宏别名：valueSignal）
//! ui! {
//!     <Progress id="upload" valueSignal={pct}>
//!         <ProgressLabel>"Uploading"</ProgressLabel>
//!         <ProgressValue />
//!     </Progress>
//!     <Progress value={0.4} />
//! }
//!
//! // B · Builder (snake_case only)
//! progress()
//!     .id("upload")
//!     .value_signal(pct)
//!     .child(|| {
//!         let mut frag = Element::new();
//!         frag.set_attr("rui_fragment", "1");
//!         frag.children(vec![
//!             ProgressLabel(ProgressLabelProps {
//!                 children: Some(Element::new().text("Uploading")),
//!                 base: None,
//!             }),
//!             ProgressValue(ProgressValueProps {
//!                 children: None,
//!                 base: None,
//!             }),
//!         ])
//!     })
//! // 简单条：
//! progress().default_value(0.4)
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::sync::OnceLock;

use crate::app::reactive::{bind_prop, bind_text, reactive_view};
use crate::core::reactivity::{
    Context, Signal, create_context, provide_context, use_context, use_signal, with_child_owner,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::r#box::{r#box, hstack, vstack};
use crate::primitives::svg::{circle, path, svg};
use crate::primitives::text::text;
use crate::style::{parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::resolve_instance_id;
use crate::utils::take_packed;
use rui_macros::component;

const ATTR_ORIENTATION: &str = "data-orientation";
/// 纵向条默认高度（横向条仍 `w-full` + 固定轨高）。
const PROGRESS_VERTICAL_DEFAULT_H: &str = "8rem";
const PROGRESS_TRACK_CROSS: &str = "0.5rem";

// ---------------------------------------------------------------------------
// Orientation
// ---------------------------------------------------------------------------

/// 线条型 Progress 主轴（对位 Tabs / ButtonGroup `orientation`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgressOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl ProgressOrientation {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vertical" | "col" | "column" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl From<&str> for ProgressOrientation {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ProgressOrientation {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

// ---------------------------------------------------------------------------
// Context / scope
// ---------------------------------------------------------------------------

/// Root 上下文：进度 `value` ∈ `0..=1` + 稳定实例 `id`。
#[derive(Clone)]
pub struct ProgressCtx {
    pub value: Signal<f32>,
    pub id: String,
    pub orientation: ProgressOrientation,
}

fn progress_context() -> Context<ProgressCtx> {
    static CTX: OnceLock<Context<ProgressCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ProgressCtx>)
}

/// 读取最近的 [`Progress`] 上下文。
pub fn use_progress() -> Option<ProgressCtx> {
    use_context(progress_context())
}

fn require_progress(part: &str) -> ProgressCtx {
    use_progress().unwrap_or_else(|| {
        panic!("{part} must be used within <Progress> / progress_scope");
    })
}

fn is_empty_el(el: &Element) -> bool {
    el.children.is_empty()
        && el.text.as_ref().is_none_or(|t| t.is_empty())
        && el.template_id.is_none()
}

fn flatten_header(mut header: Element) -> Option<Element> {
    if is_empty_el(&header) {
        return None;
    }
    let is_frag = header.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1");
    if is_frag {
        let kids = std::mem::take(&mut header.children);
        if kids.is_empty() {
            return None;
        }
        if kids.len() == 1 {
            return Some(kids.into_iter().next().unwrap());
        }
        // Label + Value 等：一行两端对齐（对位 Root flex-wrap 首行）。
        return Some(
            hstack()
                .w("100%")
                .items_center()
                .justify("between")
                .gap("0.5rem")
                .children(kids)
                .into(),
        );
    }
    Some(header)
}

/// 提供 Progress 上下文；子树之后自动挂 [`progress_track`]（对位 Root 注入 Track）。
pub fn progress_scope(
    id: impl Into<String>,
    value: Option<Signal<f32>>,
    default_value: f32,
    orientation: impl Into<ProgressOrientation>,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "progress");
    let orientation = orientation.into();
    let default_value = default_value.clamp(0.0, 1.0);
    let value = value
        .unwrap_or_else(|| use_signal(format!("{id}__progress").as_str(), move || default_value));
    let mut root = with_child_owner(|| {
        provide_context(
            progress_context(),
            ProgressCtx {
                value,
                id: id.clone(),
                orientation,
            },
        );
        let header = flatten_header(body());
        let track = progress_track(ProgressTrackOpts {
            class: None,
            value: Some(value),
            id: Some(id.clone()),
            orientation: Some(orientation),
        });
        let mut col = vstack().gap("0.75rem");
        if orientation.is_vertical() {
            col = col.h(PROGRESS_VERTICAL_DEFAULT_H).items_center();
        } else {
            col = col.w("100%");
        }
        if let Some(h) = header {
            col = col.child(h);
        }
        col = col.child(track);
        let mut el: Element = col.into();
        el.set_attr(ATTR_ORIENTATION, orientation.as_str());
        if let Some(ref c) = class {
            if !c.trim().is_empty() {
                el = with_class_list(el, c);
            }
        }
        el
    });
    if root.template_id.is_none() {
        root.template_id = Some(format!("{id}__progress"));
    }
    root
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct ProgressBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    orientation: ProgressOrientation,
    class: String,
}

/// Builder 入口：默认无用户 id（WP-2.8）。
pub fn progress() -> ProgressBuilder {
    ProgressBuilder {
        id: String::new(),
        value: None,
        default_value: 0.0,
        orientation: ProgressOrientation::Horizontal,
        class: String::new(),
    }
}

impl ProgressBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v.clamp(0.0, 1.0);
        self
    }

    pub fn orientation(mut self, o: impl Into<ProgressOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    /// Tailwind / utility class（根）。
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    /// 复合面：在 Track 之前渲染 Label / Value 等（内部 → [`progress_scope`]）。
    pub fn child(self, body: impl FnOnce() -> Element + 'static) -> Element {
        let class = if self.class.trim().is_empty() {
            None
        } else {
            Some(self.class)
        };
        progress_scope(
            self.id,
            self.value,
            self.default_value,
            self.orientation,
            class,
            body,
        )
    }
}

impl From<ProgressBuilder> for Element {
    fn from(builder: ProgressBuilder) -> Self {
        let ProgressBuilder {
            id,
            value,
            default_value,
            orientation,
            class,
        } = builder;
        let class = if class.trim().is_empty() {
            None
        } else {
            Some(class)
        };
        progress_scope(id, value, default_value, orientation, class, Element::new)
    }
}

/// RSX 备用入口（无宏特殊展开时）；复合面请走宏 → [`progress_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Progress(
    id: String,
    value: Option<f32>,
    value_signal: Option<Signal<f32>>,
    orientation: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut b = progress().id(id);
    if let Some(s) = value_signal {
        b = b.value_signal(s);
    } else if let Some(v) = value {
        b = b.default_value(v);
    }
    if let Some(o) = orientation {
        b = b.orientation(o);
    }
    if let Some(c) = class {
        b = b.class(c);
    }
    if let Some(ch) = children {
        // 无宏时子节点已脱离上下文；仅作占位，复合请用 ui! 宏展开。
        let _ = ch;
    }
    b.into()
}

// ---------------------------------------------------------------------------
// Track / Indicator
// ---------------------------------------------------------------------------

/// `ProgressTrack` 选项。
#[derive(Clone, Default)]
pub struct ProgressTrackOpts {
    pub class: Option<String>,
    /// 显式值信号；`None` 时读 [`ProgressCtx`]。
    pub value: Option<Signal<f32>>,
    pub id: Option<String>,
    pub orientation: Option<ProgressOrientation>,
}

/// ShadCN `cn-progress-track` + 内嵌 Indicator（响应 `value`）。
pub fn progress_track(opts: ProgressTrackOpts) -> Element {
    let value = opts.value.or_else(|| use_progress().map(|c| c.value));
    let Some(val) = value else {
        panic!("ProgressTrack must be used within <Progress> / progress_scope (or pass value)");
    };
    let instance = opts
        .id
        .or_else(|| use_progress().map(|c| c.id))
        .unwrap_or_else(|| "progress".into());
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let orientation = opts
        .orientation
        .or_else(|| use_progress().map(|c| c.orientation))
        .unwrap_or_default();
    let view_id = format!("{instance}__progress_track_view");

    reactive_view(view_id, move || {
        let t = val.get().clamp(0.0, 1.0);
        let pct = (t * 100.0).clamp(0.0, 100.0);
        let mut track_bg = tokens::primary();
        track_bg[3] *= 0.2;
        let mut track = if orientation.is_vertical() {
            // 纵向：自底向上填充（justify-end + 高度百分比）。
            let mut track: Element = vstack()
                .w(PROGRESS_TRACK_CROSS)
                .h("100%")
                .rounded_full()
                .overflow("hidden")
                .items_center()
                .justify("end")
                .into();
            track = track.bg(track_bg);
            let mut fill: Element = r#box()
                .w("100%")
                .h(format!("{pct}%"))
                .rounded_full()
                .flex_shrink(0.0)
                .into();
            fill = fill.bg(tokens::primary());
            track.child(fill)
        } else {
            // cn-progress-track: relative flex w-full items-center overflow-x-hidden; h-2 rounded-full
            let mut track: Element = hstack()
                .w("100%")
                .h(PROGRESS_TRACK_CROSS)
                .rounded_full()
                .overflow("hidden")
                .items_center()
                .into();
            track = track.bg(track_bg);
            let mut fill: Element = r#box()
                .h("100%")
                .w(format!("{pct}%"))
                .rounded_full()
                .flex_shrink(0.0)
                .into();
            fill = fill.bg(tokens::primary());
            track.child(fill)
        };
        track.set_attr("rui_progress", format!("{pct:.1}%"));
        track.set_attr(ATTR_ORIENTATION, orientation.as_str());
        track = track.accesskit_role("progressbar");
        if let Some(ref c) = class {
            track = with_class_list(track, c);
        }
        track.template_id = Some(format!("{instance}__track"));
        track
    })
}

/// RSX `<ProgressTrack class=… />`（需在 Progress 内；通常由 Root 自动注入）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressTrack(class: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    progress_track(ProgressTrackOpts {
        class,
        value: None,
        id: None,
        orientation: None,
    })
}

/// RSX `<ProgressIndicator />` — Track 已内嵌指示条；本标签为 no-op 占位（对位导出面）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressIndicator(class: Option<String>, children: Option<Element>) -> Element {
    let _ = (class, children);
    Element::new().w(0.0).h(0.0)
}

/// ShadCN `ProgressLabel`（`text-sm` / `font-medium`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressLabel(children: Option<Element>) -> Element {
    let _ = require_progress("ProgressLabel");
    let fg = parse_text_color("foreground");
    let kids = children.map(take_packed).unwrap_or_default();
    if kids.len() == 1 {
        let c = kids.into_iter().next().unwrap();
        if c.children.is_empty() && c.text.as_ref().is_some_and(|t| !t.is_empty()) {
            return text(c.text.clone().unwrap_or_default())
                .size(14.0)
                .weight(500.0)
                .color(fg)
                .into();
        }
        return c.text_size(14.0).font_weight(500.0).text_color(fg);
    }
    Element::from(
        hstack()
            .w("100%")
            .items_center()
            .justify("between")
            .gap("0.5rem")
            .children(
                kids.into_iter()
                    .map(|c| c.text_size(14.0).font_weight(500.0).text_color(fg))
                    .collect::<Vec<_>>(),
            ),
    )
}

/// ShadCN `ProgressValue`：当前百分比（`bind_text` 原地更新）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressValue(children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_progress("ProgressValue");
    let value = ctx.value;
    let bind_id = format!("{}:progress_value", ctx.id);
    let muted = parse_text_color("muted-foreground");
    bind_text(bind_id, move || {
        let t = value.get().clamp(0.0, 1.0);
        format!("{:.0}%", t * 100.0)
    })
    .text_size(14.0)
    .font_weight(400.0)
    .text_color(muted)
}

// ---------------------------------------------------------------------------
// ProgressCircle（Solid UI progress-circle 对位）
// ---------------------------------------------------------------------------

/// 圆形进度尺寸（对位 Solid UI `xs` … `xl`）。
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ProgressCircleSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl ProgressCircleSize {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "xs" | "extra-small" | "extrasmall" => Self::Xs,
            "sm" | "small" => Self::Sm,
            "lg" | "large" => Self::Lg,
            "xl" | "extra-large" | "extralarge" => Self::Xl,
            _ => Self::Md,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xl => "xl",
        }
    }

    /// 外接圆半径（px），对位 Solid UI `sizes` 表。
    pub fn radius(self) -> f32 {
        match self {
            Self::Xs => 15.0,
            Self::Sm => 19.0,
            Self::Md => 32.0,
            Self::Lg => 52.0,
            Self::Xl => 80.0,
        }
    }

    pub fn stroke_width(self) -> f32 {
        match self {
            Self::Xs => 3.0,
            Self::Sm => 4.0,
            Self::Md => 6.0,
            Self::Lg => 8.0,
            Self::Xl => 10.0,
        }
    }
}

fn progress_circle_dims(
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
) -> (f32, f32) {
    let outer_r = radius.unwrap_or_else(|| size.radius()).max(1.0);
    let sw = stroke_width.unwrap_or_else(|| size.stroke_width()).max(0.5);
    (outer_r, sw)
}

/// 自 12 点顺时针扫过的圆弧 `d`（`value` ∈ `0..=1`）；`None` 表示无可见弧。
pub(crate) fn progress_circle_arc_d(cx: f32, cy: f32, r: f32, value: f32) -> Option<String> {
    let t = value.clamp(0.0, 1.0);
    if t <= 0.0 {
        return None;
    }
    if t >= 1.0 {
        return Some(format!(
            "M {cx} {y0} A {r} {r} 0 1 1 {cx} {y1} A {r} {r} 0 1 1 {cx} {y0}",
            y0 = cy - r,
            y1 = cy + r,
        ));
    }
    let angle = t * std::f32::consts::TAU;
    let x1 = cx + r * angle.sin();
    let y1 = cy - r * angle.cos();
    let large = u8::from(t > 0.5);
    Some(format!(
        "M {cx} {y0} A {r} {r} 0 {large} 1 {x1} {y1}",
        y0 = cy - r,
    ))
}

fn progress_circle_overlay(children: Element) -> Element {
    let kids = take_packed(children);
    if kids.is_empty() {
        return Element::new();
    }
    r#box()
        .absolute(true)
        .top(0.0)
        .left(0.0)
        .right(0.0)
        .bottom(0.0)
        .items_center()
        .justify_center()
        .children(kids)
        .into()
}

/// 构建圆形进度 SVG 叶（无 reactive 壳）。
pub fn progress_circle_svg(
    value: f32,
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: bool,
) -> Element {
    let t = value.clamp(0.0, 1.0);
    let (outer_r, sw) = progress_circle_dims(size, radius, stroke_width);
    let nr = (outer_r - sw / 2.0).max(0.0);
    let d = outer_r * 2.0;

    let track: Element = circle()
        .cx(outer_r)
        .cy(outer_r)
        .r(nr)
        .fill("none")
        .stroke("border")
        .stroke_width(sw)
        .stroke_linecap("round")
        .into();

    let mut svg_children = vec![track];
    if let Some(path_d) = progress_circle_arc_d(outer_r, outer_r, nr, t) {
        let mut indicator: Element = path()
            .d(path_d)
            .fill("none")
            .stroke("primary")
            .stroke_width(sw)
            .stroke_linecap("round")
            .into();
        if show_animation {
            indicator = indicator
                .transition("all")
                .transition_duration("300ms")
                .transition_timing_function("ease-in-out");
        }
        svg_children.push(indicator);
    }

    let mut svg_el: Element = svg()
        .w(d)
        .h(d)
        .view_box(format!("0 0 {d} {d}"))
        .children(svg_children)
        .into();
    svg_el.set_attr("rui_progress", format!("{:.0}%", t * 100.0));
    svg_el
}

fn apply_progress_circle_svg(
    el: &mut Element,
    t: f32,
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: bool,
) -> LayoutStatus {
    let fresh = progress_circle_svg(t, size, radius, stroke_width, show_animation);
    el.attrs = fresh.attrs.clone();
    el.image_texture = fresh.image_texture.clone();
    el.style.size = fresh.style.size;
    el.style.min_size = fresh.style.min_size;
    el.style.max_size = fresh.style.max_size;
    el.children.clear();
    el.text = None;
    el.text_spans = None;
    LayoutStatus::LayoutChanged
}

/// 响应式圆环：`bind_prop` 首帧同步写入 SVG 场景，后续 `value` 变化原位 patch（无 reconcile 占位符）。
fn progress_circle_inner(
    value: Signal<f32>,
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: bool,
    view_id: &str,
) -> Element {
    let view_id = view_id.to_string();
    bind_prop(
        Element::new().flex_shrink(0.0),
        view_id,
        move || value.get().clamp(0.0, 1.0),
        move |el, t| apply_progress_circle_svg(el, t, size, radius, stroke_width, show_animation),
    )
}

/// 圆形进度条选项。
#[derive(Default)]
pub struct ProgressCircleOpts {
    pub id: Option<String>,
    pub value: Option<Signal<f32>>,
    pub default_value: f32,
    pub size: ProgressCircleSize,
    pub radius: Option<f32>,
    pub stroke_width: Option<f32>,
    pub show_animation: bool,
    pub class: Option<String>,
    pub children: Option<Element>,
}

/// 渲染圆形进度（响应 `value` ∈ `0..=1`）。
pub fn progress_circle_view(opts: ProgressCircleOpts) -> Element {
    let instance = resolve_instance_id(opts.id.unwrap_or_default(), "progress_circle");
    let default_value = opts.default_value.clamp(0.0, 1.0);
    let value = opts
        .value
        .or_else(|| use_progress().map(|c| c.value))
        .unwrap_or_else(|| {
            use_signal(format!("{instance}__progress_circle").as_str(), move || {
                default_value
            })
        });
    let size = opts.size;
    let radius = opts.radius;
    let stroke_width = opts.stroke_width;
    let show_animation = opts.show_animation;
    let class = opts.class.filter(|s| !s.trim().is_empty());
    let children = opts.children;
    let (outer_r, _) = progress_circle_dims(size, radius, stroke_width);
    let diameter = outer_r * 2.0;
    let view_id = format!("{instance}__view");

    let mut core =
        progress_circle_inner(value, size, radius, stroke_width, show_animation, &view_id);
    core = core.flex_shrink(0.0);

    let mut root: Element = if let Some(ch) = children {
        let overlay = progress_circle_overlay(ch);
        if overlay.children.is_empty() {
            core
        } else {
            r#box()
                .relative(true)
                .w(diameter)
                .h(diameter)
                .flex_shrink(0.0)
                .items_center()
                .justify_center()
                .child(core)
                .child(overlay)
                .into()
        }
    } else {
        core
    };

    root = root.accesskit_role("progressbar");
    if let Some(ref c) = class {
        root = with_class_list(root, c);
    }
    root
}

pub struct ProgressCircleBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: bool,
    class: String,
    children: Option<Element>,
}

/// Builder 入口：默认 `size = md`，`show_animation = true`。
pub fn progress_circle() -> ProgressCircleBuilder {
    ProgressCircleBuilder {
        id: String::new(),
        value: None,
        default_value: 0.0,
        size: ProgressCircleSize::Md,
        radius: None,
        stroke_width: None,
        show_animation: true,
        class: String::new(),
        children: None,
    }
}

impl ProgressCircleBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v.clamp(0.0, 1.0);
        self
    }

    pub fn size(mut self, size: ProgressCircleSize) -> Self {
        self.size = size;
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = Some(r.max(1.0));
        self
    }

    pub fn stroke_width(mut self, w: f32) -> Self {
        self.stroke_width = Some(w.max(0.5));
        self
    }

    pub fn show_animation(mut self, on: bool) -> Self {
        self.show_animation = on;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children = Some(child.into());
        self
    }
}

impl From<ProgressCircleBuilder> for Element {
    fn from(builder: ProgressCircleBuilder) -> Self {
        let ProgressCircleBuilder {
            id,
            value,
            default_value,
            size,
            radius,
            stroke_width,
            show_animation,
            class,
            children,
        } = builder;
        progress_circle_view(ProgressCircleOpts {
            id: Some(id),
            value,
            default_value,
            size,
            radius,
            stroke_width,
            show_animation,
            class: if class.trim().is_empty() {
                None
            } else {
                Some(class)
            },
            children,
        })
    }
}

/// RSX `<ProgressCircle />`（对位 Solid UI [`progress-circle`](../../../../Reference/solid-ui-main/apps/docs/src/registry/ui/progress-circle.tsx)）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ProgressCircle(
    id: String,
    value: Option<f32>,
    value_signal: Option<Signal<f32>>,
    size: Option<ProgressCircleSize>,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: Option<bool>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut b = progress_circle().id(id);
    if let Some(s) = size {
        b = b.size(s);
    }
    if let Some(on) = show_animation {
        b = b.show_animation(on);
    }
    if let Some(s) = value_signal {
        b = b.value_signal(s);
    } else if let Some(v) = value {
        b = b.default_value(v);
    }
    if let Some(r) = radius {
        b = b.radius(r);
    }
    if let Some(w) = stroke_width {
        b = b.stroke_width(w);
    }
    if let Some(c) = class {
        b = b.class(c);
    }
    if let Some(ch) = children {
        b = b.child(ch);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn progress_empty_id_never_writes_empty_template() {
        let el: Element = progress().default_value(0.25).into();
        let tid = el.template_id.as_deref().unwrap_or("");
        assert_ne!(tid, "");
        assert!(!tid.is_empty() || el.template_id.is_none());
    }

    #[test]
    fn progress_empty_id_resolves_nonempty_instance_attr() {
        let el: Element = progress().into();
        assert_ne!(el.template_id.as_deref(), Some(""));
    }

    #[test]
    fn scope_exposes_template() {
        let (_owner, el): (_, Element) = create_root(|| {
            progress()
                .id("prog-scope")
                .value_signal(use_signal("prog-scope-val", || 0.5f32))
                .into()
        });
        assert_eq!(el.template_id.as_deref(), Some("prog-scope__progress"));
    }

    #[test]
    fn labeled_scope_keeps_context() {
        let (_owner, el): (_, Element) = create_root(|| {
            progress().id("prog-lbl").default_value(0.3).child(|| {
                ProgressLabel(ProgressLabelProps {
                    children: Some(Element::new().text("Load")),
                    base: None,
                })
            })
        });
        assert_eq!(el.template_id.as_deref(), Some("prog-lbl__progress"));
    }

    #[test]
    fn circle_size_table_matches_solid_ui() {
        assert_eq!(ProgressCircleSize::Xs.radius(), 15.0);
        assert_eq!(ProgressCircleSize::Md.stroke_width(), 6.0);
        assert_eq!(ProgressCircleSize::Xl.radius(), 80.0);
        assert_eq!(ProgressCircleSize::parse("LG"), ProgressCircleSize::Lg);
    }

    #[test]
    fn circle_arc_d_quarter_and_full() {
        let q = progress_circle_arc_d(32.0, 32.0, 29.0, 0.25).unwrap();
        assert!(q.starts_with("M 32 "));
        assert!(q.contains(" A 29 29 "));
        let full = progress_circle_arc_d(10.0, 10.0, 8.0, 1.0).unwrap();
        assert_eq!(full.matches(" A ").count(), 2);
        assert!(progress_circle_arc_d(10.0, 10.0, 8.0, 0.0).is_none());
    }

    #[test]
    fn circle_builder_sync_svg_when_untracked() {
        let el: Element = progress_circle().id("pc").default_value(0.6).into();
        assert_eq!(
            el.attrs.get("rui_progress").map(String::as_str),
            Some("60%")
        );
        assert!(
            el.get_attr(crate::primitives::svg::ATTR_SVG_SCENE)
                .is_some()
                || el
                    .get_attr(crate::primitives::path::ATTR_PATH_SCENE)
                    .is_some(),
            "untracked circle must compile to a raster scene"
        );
    }

    #[cfg(feature = "svg")]
    #[test]
    fn circle_svg_registers_scene() {
        use crate::primitives::svg::ATTR_SVG_SCENE;
        let el = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, true);
        assert!(
            el.get_attr(ATTR_SVG_SCENE).is_some(),
            "progress circle must compile to resvg scene"
        );
    }

    fn rgba_opaque_count(px: &[u8]) -> usize {
        px.chunks(4)
            .filter(|p| p.get(3).copied().unwrap_or(0) > 16)
            .count()
    }

    /// Gallery paints `<svg>` via resvg. Theme tokens in `stroke=` are not CSS colors.
    /// Emit must rewrite them to rgb(); otherwise the ring occupies layout but is invisible.
    #[cfg(feature = "svg")]
    #[test]
    fn circle_emitted_svg_has_opaque_pixels() {
        let el = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, false);
        let raw_token_doc = concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" fill="none">"#,
            r#"<circle cx="32" cy="32" r="29" fill="none" stroke="border" stroke-width="6"/>"#,
            r#"</svg>"#,
        );
        let raw_n = rgba_opaque_count(
            &crate::media::svg_raster::rasterize_svg_str(raw_token_doc, 64, 64).unwrap(),
        );
        assert_eq!(
            raw_n, 0,
            "control: resvg must ignore CSS theme token names (got opaque={raw_n})"
        );

        let doc = crate::primitives::svg::emit_svg_document_from_element_attrs(&el.attrs)
            .expect("progress circle must emit svg markup");
        assert!(
            doc.contains("rgb(") || doc.contains("rgba("),
            "emitted SVG must resolve theme tokens to CSS rgb; doc={doc}"
        );
        let px = crate::media::svg_raster::rasterize_svg_str(&doc, 64, 64)
            .expect("emitted progress circle must rasterize");
        assert!(
            rgba_opaque_count(&px) > 80,
            "emitted progress-circle SVG must have visible coverage; opaque={} doc={doc}",
            rgba_opaque_count(&px)
        );
    }

    #[test]
    fn circle_mounts_after_effect() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::create_effect;
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();
        let tree: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&tree));
        let tree_for_assert = Rc::clone(&tree);

        let (_owner, _): (_, ()) = create_root(|| {
            create_effect(move || {
                let pc = progress_circle()
                    .id("pc-mount")
                    .default_value(0.45)
                    .size(ProgressCircleSize::Md)
                    .into();
                tree.borrow_mut().children = vec![pc];
            });
        });

        fn has_progress_attr(el: &Element) -> bool {
            el.attrs.contains_key("rui_progress") || el.children.iter().any(has_progress_attr)
        }
        assert!(
            has_progress_attr(&tree_for_assert.borrow()),
            "bind_prop must ship SVG scene on first mount"
        );
        clear_test_reconcile_root();
    }

    #[test]
    fn circle_bind_prop_ships_scene_immediately() {
        let (_owner, el): (_, Element) =
            create_root(|| progress_circle().id("pc").default_value(0.5).into());
        assert_eq!(el.template_id.as_deref(), Some("pc__view"));
        assert!(
            el.get_attr(crate::primitives::svg::ATTR_SVG_SCENE)
                .is_some()
                || el
                    .get_attr(crate::primitives::path::ATTR_PATH_SCENE)
                    .is_some(),
            "bind_prop initial apply must register svg/path scene"
        );
    }

    #[test]
    fn circle_value_signal_clamps() {
        let (_owner, el): (_, Element) = create_root(|| {
            let v = use_signal("pc-clamp", || 1.5f32);
            progress_circle().id("pc-clamp").value_signal(v).into()
        });
        fn progress_pct(el: &Element) -> Option<&str> {
            el.attrs
                .get("rui_progress")
                .map(String::as_str)
                .or_else(|| {
                    el.children
                        .first()
                        .and_then(|c| c.attrs.get("rui_progress").map(String::as_str))
                })
        }
        assert_eq!(progress_pct(&el), Some("100%"));
    }

    #[cfg(feature = "svg")]
    #[test]
    fn circle_svg_layout_paints() {
        use crate::layout::layout_tree::LayoutTree;

        let svg_el = progress_circle_svg(0.45, ProgressCircleSize::Md, None, None, true);
        let mut root = Element::new().w(200.0).h(200.0).child(svg_el);
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
        assert!(
            result
                .textured_draws
                .iter()
                .any(|d| d.rect[2] > 30.0 && d.rect[3] > 30.0),
            "progress circle must raster with non-zero bounds; draws={:?}",
            result.textured_draws
        );
        crate::style::set_gradient_raster_dpi(1.0);
        let leaf = &root.children[0];
        let (tex, dw, dh) = crate::primitives::svg::svg_raster_from_element_attrs(
            &leaf.attrs,
            leaf.layout_bounds[2],
            leaf.layout_bounds[3],
        )
        .expect("svg raster key");
        let doc =
            crate::primitives::svg::emit_svg_document_from_element_attrs(&leaf.attrs).unwrap();
        let lo = crate::media::svg_raster::ensure_svg_document_texture(&doc, 64, 64).unwrap();
        assert_ne!(
            tex, lo,
            "paint raster must not use 1× viewBox texture (blurry on HiDPI)"
        );
        assert!(
            (dw - 64.0).abs() < 1.0 && (dh - 64.0).abs() < 1.0,
            "draw size stays logical, got {dw}×{dh}"
        );
    }

    #[test]
    fn vertical_progress_sets_orientation_attr() {
        let el: Element = progress().orientation("vertical").default_value(0.5).into();
        assert_eq!(
            el.attrs.get(ATTR_ORIENTATION).map(String::as_str),
            Some("vertical")
        );
    }

    #[test]
    fn vertical_scope_uses_default_height() {
        let (_owner, el): (_, Element) = create_root(|| {
            progress()
                .id("prog-vert")
                .orientation(ProgressOrientation::Vertical)
                .default_value(0.4)
                .into()
        });
        assert_eq!(
            el.attrs.get(ATTR_ORIENTATION).map(String::as_str),
            Some("vertical")
        );
        assert!(
            el.children.len() >= 1,
            "vertical progress should include track"
        );
    }
}
