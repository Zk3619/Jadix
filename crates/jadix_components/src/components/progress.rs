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
//! `bg-primary/20` 轨 + `bg-primary` 指示条 · `w-full`。指示条尺寸走 `bind_prop`，
//! 不因 `value` 重建轨（N.4）。
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
//!         frag.set_attr("jadix_fragment", "1");
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
//!
//! // Circle · arbitrary centered content, contained inside the inner circle.
//! ui! {
//!     <ProgressCircle valueSignal={pct}>
//!         { Element::new().text(move || format!("{:.0}%", pct.get().clamp(0.0, 1.0) * 100.0)) }
//!     </ProgressCircle>
//!     <ProgressCircle value={0.6} size={ProgressCircleSize::Lg}>
//!         <Image src="assets/photo.jpg" w="100%" h="100%" />
//!     </ProgressCircle>
//! }
//! progress_circle().value_signal(pct)
//!     .child(Element::new().text(move || format!("{:.0}%", pct.get() * 100.0)))
//! // Use VStack / HStack inside the slot to compose images, text, icons, etc.
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::sync::OnceLock;

use crate::app::reactive::{apply_bound_size, bind_prop, bind_text};
use crate::components::base::{
    fill_default_text_color, fill_default_text_size, fill_default_text_weight, packed_children,
    SlotAuthor, SlotChrome,
};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{Element, IntoElementList, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::impl_slot_chrome_methods;
use crate::primitives::path::{
    decode_path_d, encode_path_d, ensure_raster_texture, register_path_scene, PathStyle,
    ATTR_PATH_D, ATTR_PATH_FIT_LEAF, ATTR_PATH_SCENE, ATTR_PATH_VIEWBOX,
};
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color};
use crate::tokens;
use crate::utils::resolve_instance_id;
use crate::utils::take_packed;

const ATTR_SLOT: &str = "data-slot";
const SLOT_TRACK: &str = "progress-track";
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

fn is_progress_track(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_TRACK)
}

fn flatten_kids(mut body: Element) -> Vec<Element> {
    if is_empty_el(&body) {
        return Vec::new();
    }
    let is_frag = body.attrs.get("jadix_fragment").map(|s| s.as_str()) == Some("1");
    if is_frag {
        std::mem::take(&mut body.children)
    } else {
        vec![body]
    }
}

fn flatten_header_kids(kids: Vec<Element>) -> Option<Element> {
    if kids.is_empty() {
        return None;
    }
    if kids.len() == 1 {
        return Some(kids.into_iter().next().unwrap());
    }
    // Label + Value 等：一行两端对齐（对位 Root flex-wrap 首行）。
    Some(
        hstack()
            .w("100%")
            .items_center()
            .justify("between")
            .gap("0.5rem")
            .children(kids)
            .into(),
    )
}

fn split_progress_body(body: Element) -> (Option<Element>, Option<Element>) {
    let mut header_kids = Vec::new();
    let mut track = None;
    for kid in flatten_kids(body) {
        if is_progress_track(&kid) {
            if track.is_none() {
                track = Some(kid);
            }
        } else if !is_empty_el(&kid) {
            header_kids.push(kid);
        }
    }
    (flatten_header_kids(header_kids), track)
}

/// 提供 Progress 上下文；子树之后自动挂 [`progress_track`]（对位 Root 注入 Track）。
pub fn progress_scope(
    id: impl Into<String>,
    value: Option<Signal<f32>>,
    default_value: f32,
    orientation: impl Into<ProgressOrientation>,
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
        let (header, user_track) = split_progress_body(body());
        let track = user_track.unwrap_or_else(|| {
            progress_track(ProgressTrackOpts {
                chrome: SlotChrome::default(),
                value: Some(value),
                id: Some(id.clone()),
                orientation: Some(orientation),
            })
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

pub struct ProgressTrackBuilder {
    chrome: SlotChrome,
}

pub fn progress_track_slot() -> ProgressTrackBuilder {
    ProgressTrackBuilder {
        chrome: SlotChrome::default(),
    }
}

impl ProgressTrackBuilder {
    impl_slot_chrome_methods!();
}

pub struct ProgressBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    orientation: ProgressOrientation,
    chrome: SlotChrome,
    track_chrome: SlotChrome,
}

/// Builder 入口：默认无用户 id（WP-2.8）。
pub fn progress() -> ProgressBuilder {
    ProgressBuilder {
        id: String::new(),
        value: None,
        default_value: 0.0,
        orientation: ProgressOrientation::Horizontal,
        chrome: SlotChrome::default(),
        track_chrome: SlotChrome::default(),
    }
}

fn finish_progress(
    builder: ProgressBuilder,
    body: Option<Box<dyn FnOnce() -> Element>>,
) -> Element {
    let ProgressBuilder {
        id,
        value,
        default_value,
        orientation,
        chrome,
        track_chrome,
    } = builder;
    chrome.apply(progress_scope(
        id,
        value,
        default_value,
        orientation,
        move || {
            let raw = body.map(|b| b()).unwrap_or_default();
            ensure_progress_track(raw, &track_chrome)
        },
    ))
}

fn ensure_progress_track(body: Element, chrome: &SlotChrome) -> Element {
    let mut kids = flatten_kids(body);
    if kids.iter().any(is_progress_track) {
        return packed_children(kids).unwrap_or_default();
    }
    kids.push(progress_track(ProgressTrackOpts {
        chrome: chrome.clone(),
        value: None,
        id: None,
        orientation: None,
    }));
    packed_children(kids).unwrap_or_default()
}

impl ProgressBuilder {
    impl_slot_chrome_methods!();

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

    /// Style the auto-injected [`ProgressTrack`] (ignored when the body already has a track).
    pub fn track_with(
        mut self,
        f: impl FnOnce(ProgressTrackBuilder) -> ProgressTrackBuilder,
    ) -> Self {
        self.track_chrome = f(progress_track_slot()).chrome;
        self
    }

    /// 复合面：在 Track 之前渲染 Label / Value 等（内部 → [`progress_scope`]）。
    pub fn child(self, body: impl FnOnce() -> Element + 'static) -> Element {
        finish_progress(self, Some(Box::new(body)))
    }
}

impl From<ProgressBuilder> for Element {
    fn from(builder: ProgressBuilder) -> Self {
        finish_progress(builder, None)
    }
}

crate::__component_contract! {
ProgressProps [];
/// RSX 备用入口（无宏特殊展开时）；复合面请走宏 → [`progress_scope`]。
#[allow(non_snake_case)]
pub fn Progress(
identity [] id: String,
plain [] value: Option<f32>,
plain [] value_signal: Option<Signal<f32>>,
plain [] orientation: Option<String>,
plain [] children: Option<Element>,
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
    if let Some(ch) = children {
        // 无宏时子节点已脱离上下文；仅作占位，复合请用 ui! 宏展开。
        let _ = ch;
    }
    b.into()
}
}

// ---------------------------------------------------------------------------
// Track / Indicator
// ---------------------------------------------------------------------------

/// `ProgressTrack` 选项。
#[derive(Clone, Default)]
pub struct ProgressTrackOpts {
    pub chrome: SlotChrome,
    /// 显式值信号；`None` 时读 [`ProgressCtx`]。
    pub value: Option<Signal<f32>>,
    pub id: Option<String>,
    pub orientation: Option<ProgressOrientation>,
}

impl ProgressTrackOpts {
    crate::impl_content_opts_chrome!(chrome);
}

/// ShadCN `cn-progress-track` + 内嵌 Indicator（响应 `value`）。
///
/// Track chrome is static. Fill size patches in place — same contract as the circle variant.
pub fn progress_track(opts: ProgressTrackOpts) -> Element {
    let value = opts.value.or_else(|| use_progress().map(|c| c.value));
    let Some(val) = value else {
        panic!("ProgressTrack must be used within <Progress> / progress_scope (or pass value)");
    };
    let instance = opts
        .id
        .or_else(|| use_progress().map(|c| c.id))
        .unwrap_or_else(|| "progress".into());
    let chrome = opts.chrome;
    let orientation = opts
        .orientation
        .or_else(|| use_progress().map(|c| c.orientation))
        .unwrap_or_default();
    let pct0 = (val.peek().clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0);
    let mut track_bg = tokens::primary();
    track_bg[3] *= 0.2;
    let vertical = orientation.is_vertical();
    let mut track = if vertical {
        let mut track: Element = vstack()
            .w(PROGRESS_TRACK_CROSS)
            .h("100%")
            .rounded_full()
            .overflow("hidden")
            .items_center()
            .justify("end")
            .into();
        track = track.bg(track_bg);
        track
    } else {
        let mut track: Element = hstack()
            .w("100%")
            .h(PROGRESS_TRACK_CROSS)
            .rounded_full()
            .overflow("hidden")
            .items_center()
            .into();
        track = track.bg(track_bg);
        track
    };
    let mut fill: Element = if vertical {
        r#box()
            .w("100%")
            .h(format!("{pct0}%"))
            .rounded_full()
            .flex_shrink(0.0)
            .into()
    } else {
        r#box()
            .h("100%")
            .w(format!("{pct0}%"))
            .rounded_full()
            .flex_shrink(0.0)
            .into()
    };
    fill = fill.bg(tokens::primary());
    fill.template_id = Some(format!("{instance}__fill"));
    let axis = if vertical { "h" } else { "w" };
    fill = bind_prop(
        fill,
        format!("{instance}__fill_dim"),
        move || {
            let pct = (val.get().clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0);
            format!("{pct}%")
        },
        move |el, pct: String| apply_bound_size(el, axis, pct),
    );
    track = track.child(fill);
    track.set_attr("jadix_progress", format!("{pct0:.1}%"));
    track.set_attr(ATTR_ORIENTATION, orientation.as_str());
    track = track.accesskit_role("progressbar");
    track.template_id = Some(format!("{instance}__track"));
    track.set_attr(ATTR_SLOT, SLOT_TRACK);
    chrome.apply(bind_prop(
        track,
        format!("{instance}__progress_attr"),
        move || {
            let pct = (val.get().clamp(0.0, 1.0) * 100.0).clamp(0.0, 100.0);
            format!("{pct:.1}%")
        },
        |el, s: String| {
            if el.attrs.get("jadix_progress").map(String::as_str) == Some(s.as_str()) {
                LayoutStatus::Clean
            } else {
                el.set_attr("jadix_progress", s);
                LayoutStatus::StyleChanged
            }
        },
    ))
}

crate::__component_contract! {
ProgressTrackProps [];
/// RSX `<ProgressTrack class=… />`（需在 Progress 内；通常由 Root 自动注入）。
#[allow(non_snake_case)]
pub fn ProgressTrack(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    progress_track(ProgressTrackOpts {
        chrome: SlotChrome::default(),
        value: None,
        id: None,
        orientation: None,
    })
}
}

crate::__component_contract! {
ProgressIndicatorProps [];
/// RSX `<ProgressIndicator />` — Track 已内嵌指示条；本标签为 no-op 占位（对位导出面）。
#[allow(non_snake_case)]
pub fn ProgressIndicator(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    Element::new().w(0.0).h(0.0)
}
}

crate::__component_contract! {
ProgressLabelProps [];
/// ShadCN `ProgressLabel`（`text-sm` / `font-medium`）。
#[allow(non_snake_case)]
pub fn ProgressLabel(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_progress("ProgressLabel");
    let fg = parse_text_color("foreground");
    let kids = children.map(take_packed).unwrap_or_default();
    if kids.len() == 1 {
        let c = kids.into_iter().next().unwrap();
        if c.children.is_empty() && c.text.as_ref().is_some_and(|t| !t.is_empty()) {
            let author = SlotAuthor::from_el(&c);
            let mut el: Element = text(c.text.clone().unwrap_or_default()).into();
            fill_default_text_size(&mut el, 14.0);
            fill_default_text_weight(&mut el, 500);
            fill_default_text_color(&mut el, fg);
            return author.apply(el);
        }
        let mut c = c;
        fill_default_text_size(&mut c, 14.0);
        fill_default_text_weight(&mut c, 500);
        fill_default_text_color(&mut c, fg);
        return c;
    }
    Element::from(
        hstack()
            .w("100%")
            .items_center()
            .justify("between")
            .gap("0.5rem")
            .children(
                kids.into_iter()
                    .map(|mut c| {
                        fill_default_text_size(&mut c, 14.0);
                        fill_default_text_weight(&mut c, 500);
                        fill_default_text_color(&mut c, fg);
                        c
                    })
                    .collect::<Vec<_>>(),
            ),
    )
}
}

crate::__component_contract! {
ProgressValueProps [];
/// ShadCN `ProgressValue`：当前百分比（`bind_text` 原地更新）。
#[allow(non_snake_case)]
pub fn ProgressValue(
plain [] children: Option<Element>,
) -> Element {
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
}

impl_simple_slot_builder!(
    ProgressLabelBuilder,
    progress_label,
    ProgressLabel,
    ProgressLabelProps
);
impl_simple_slot_builder!(
    ProgressValueBuilder,
    progress_value,
    ProgressValue,
    ProgressValueProps
);

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

fn progress_circle_overlay(children: Element, outer_r: f32, stroke_width: f32) -> Element {
    // Only expand explicit RSX fragments; preserve custom layout containers.
    let kids = children.into_element_list();
    if kids.is_empty() {
        return Element::new();
    }
    let inner_r = (outer_r - stroke_width).max(0.0);
    let content_side = inner_r * 2.0;
    let inset = outer_r - inner_r;
    let mut overlay: Element = r#box()
        .absolute(true)
        .top(inset)
        .left(inset)
        .w(content_side)
        .h(content_side)
        .min_w(0.0)
        .min_h(0.0)
        .rounded(inner_r)
        .overflow("hidden")
        .items_center()
        .justify_center()
        .children(kids)
        .into();
    overlay.set_attr(ATTR_SLOT, "progress-circle-content");
    overlay
}

/// 构建圆形进度叶（path 核；不依赖 `svg` feature / resvg）。
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

    let mut layers = Vec::new();
    if let Some(mut track_d) = progress_circle_arc_d(outer_r, outer_r, outer_r, 1.0) {
        let inner_r = (outer_r - sw).max(0.0);
        if inner_r > 0.0 {
            let inner_top = outer_r - inner_r;
            let inner_bottom = outer_r + inner_r;
            track_d.push_str(&format!(
                " L {outer_r} {inner_top} A {inner_r} {inner_r} 0 1 0 {outer_r} {inner_bottom} A {inner_r} {inner_r} 0 1 0 {outer_r} {inner_top}"
            ));
        }
        track_d.push_str(" Z");
        let mut track_style = PathStyle::fill(parse_color("border"));
        track_style.fill_rule = crate::primitives::path::FillRule::EvenOdd;
        layers.push((decode_path_d(&track_d), track_style));
    }
    if let Some(arc_d) = progress_circle_arc_d(outer_r, outer_r, nr, t) {
        layers.push((
            decode_path_d(&arc_d),
            PathStyle::stroke(parse_color("primary"), sw),
        ));
    }

    let id = register_path_scene(layers.clone(), Vec::new(), 0.0, d.max(1.0), d.max(1.0));
    let mut el = Element::new().w(d).h(d);
    el.set_attr(ATTR_PATH_SCENE, format!("{id:016x}"));
    el.set_attr(ATTR_PATH_FIT_LEAF, "1");
    el.set_attr(ATTR_PATH_VIEWBOX, format!("0 0 {d} {d}"));
    if let Some((cmds, _)) = layers.first() {
        if !cmds.is_empty() {
            el.set_attr(ATTR_PATH_D, encode_path_d(cmds));
        }
    }
    let dim = d.round().max(16.0) as u32;
    if let Some(tex) = ensure_raster_texture(&layers, &[], 0.0, dim, dim, 2) {
        el = el.image_texture(tex);
    }
    if show_animation {
        el = el
            .transition("all")
            .transition_duration("300ms")
            .transition_timing_function("ease-in-out");
    }
    el.set_attr("jadix_progress", format!("{:.0}%", t * 100.0));
    el
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
    pub chrome: SlotChrome,
    /// Centered custom content clipped by the inner circle.
    /// `100%` sizes fill its diameter without covering the ring.
    pub children: Option<Element>,
}

impl ProgressCircleOpts {
    crate::impl_content_opts_chrome!(chrome);
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
    let chrome = opts.chrome;
    let children = opts.children;
    let (outer_r, sw) = progress_circle_dims(size, radius, stroke_width);
    let diameter = outer_r * 2.0;
    let view_id = format!("{instance}__view");

    let mut core =
        progress_circle_inner(value, size, radius, stroke_width, show_animation, &view_id);
    core = core.flex_shrink(0.0);

    let mut root: Element = if let Some(ch) = children {
        let overlay = progress_circle_overlay(ch, outer_r, sw);
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
    chrome.apply(root)
}

pub struct ProgressCircleBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
    size: ProgressCircleSize,
    radius: Option<f32>,
    stroke_width: Option<f32>,
    show_animation: bool,
    chrome: SlotChrome,
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
        chrome: SlotChrome::default(),
        children: None,
    }
}

impl ProgressCircleBuilder {
    crate::impl_slot_chrome_methods!();

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

    /// Set the centered inner content (image, reactive text, or a composed subtree).
    /// `w("100%")` / `h("100%")` fill the inner circle's diameter.
    /// Overflow is clipped by the circular content box, keeping the ring visible.
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
            chrome,
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
            chrome,
            children,
        })
    }
}

crate::__component_contract! {
ProgressCircleProps [];
/// RSX `<ProgressCircle />`（对位 Solid UI [`progress-circle`](../../../../Reference/solid-ui-main/apps/docs/src/registry/ui/progress-circle.tsx)）。
/// Children are centered and clipped by the inner circle; compose images, text, or subtrees.
#[allow(non_snake_case)]
pub fn ProgressCircle(
identity [] id: String,
plain [] value: Option<f32>,
plain [] value_signal: Option<Signal<f32>>,
plain [] size: Option<ProgressCircleSize>,
plain [] radius: Option<f32>,
plain [] stroke_width: Option<f32>,
plain [] show_animation: Option<bool>,
plain [] children: Option<Element>,
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
    if let Some(ch) = children {
        b = b.child(ch);
    }
    b.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/progress.rs"]
mod tests;
