//! # Button
//!
//! ShadCN 风格可点击按钮：变体（default / outline / ghost / link / …）、尺寸、禁用、
//! hover 过渡与焦点环。
//!
//! ## 用法
//! ```ignore
//! <Button variant="outline" size="sm" on_click={move |_| { … }}>"保存"</Button>
//! <Button id="hw-hover" variant="ghost" hover_easing="cubic">"悬停"</Button>
//! // 或：Element::from(button("ok").variant("default").on_click(…))
//! ```
//!
//! ## 注意
//! - **对齐 ShadCN / solid-ui**：普通文案默认 `whitespace-nowrap` + `flex-shrink: 0`
//!  （基类 `inline-flex shrink-0 … whitespace-nowrap`），动态文案变宽时拉宽按钮而非内部换行。
//!  Lucide 图标叶不加 nowrap，避免方按钮内图标偏右。
//! - **`id` 非必填心智**：无稳定焦点/测试需求时可省略；空 `id` → 自动 `btn-N`（会话内递增，**不**保证跨 rebuild 同一逻辑实例）。
//! - 多实例非受控、稳定焦点、冒烟选择器：请显式稳定 `id`（见 `docs/COMPONENT_LIBRARY.md` §4.2）。
//! - hover / focus 用 `peek` + 每帧 paint sync，避免整树 rebuild。
//! - 子节点可为文字或图标；Lucide 图标会保留自身 `text_size`。

use crate::core::reactivity::use_signal;
use crate::core::tree::{
    parse_color, parse_text_color, ButtonHoverPaint, Element, IntoCssDim, IntoF32, LayoutStatus,
};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::{
    focus_ring_border_color, focus_ring_color, focus_ring_destructive_color, focus_ring_params,
    focus_ring_style, is_dark_theme, lerp_display_srgb_linear, lerp_premultiplied_display_srgba,
    mix_display_srgb_in_oklab, use_ui_transition_cubic, use_ui_transition_spring,
};
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Tailwind v4 `shadow-xs`（保留常量供文档/对照；当前 Button 变体不再默认挂 shadow）。
#[allow(dead_code)]
const SHADOW_XS: &str = "0 1px 2px 0 #0000000d";
/// Tailwind `underline-offset-4` 观感折中：正值向上抬向字形；略小于 3 以免贴字。
const LINK_UNDERLINE_OFFSET_PX: f32 = 2.0;

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}

/// Focus ring（Hard/Soft via [`focus_ring_params`]）的单槽 `box-shadow`。
/// 当前 shadcn Base/Aria Button 文档站变体不再挂 `shadow-xs`。
fn button_shadow_style_for_sync(
    is_focused: bool,
    disabled: bool,
    variant: &str,
) -> (f32, [f32; 4], [f32; 2], f32) {
    if is_focused && !disabled {
        let (blur, spread) = focus_ring_params(focus_ring_style());
        let c = if variant == "destructive" {
            focus_ring_destructive_color()
        } else {
            focus_ring_color()
        };
        return (blur, c, [0.0, 0.0], spread);
    }
    let _ = disabled;
    (0.0, [0.0, 0.0, 0.0, 0.0], [0.0, 0.0], 0.0)
}

/// 与 `From` / `paint` 共用，保证与 [`use_ui_transition_spring`] 同一 key。
fn btn_hover_spring_key(id: &str) -> String {
    format!("rui:btn_hover_blend:{id}")
}

fn btn_hover_cubic_key(id: &str) -> String {
    format!("rui:btn_hover_cubic:{id}")
}

fn btn_focus_spring_key(id: &str) -> String {
    format!("rui:btn_focus_blend:{id}")
}

fn lerp_shadow_params(
    a: (f32, [f32; 4], [f32; 2], f32),
    b: (f32, [f32; 4], [f32; 2], f32),
    t: f32,
) -> (f32, [f32; 4], [f32; 2], f32) {
    let t = t.clamp(0.0, 1.0);
    let col = lerp_display_srgb_linear(a.1, b.1, t);
    let blur = a.0 * (1.0 - t) + b.0 * t;
    let off = [
        a.2[0] * (1.0 - t) + b.2[0] * t,
        a.2[1] * (1.0 - t) + b.2[1] * t,
    ];
    let spread = a.3 * (1.0 - t) + b.3 * t;
    (blur, col, off, spread)
}

fn hover_t_for_id(id: &str, easing: &str) -> f32 {
    let t = if easing == "cubic" {
        use_ui_transition_cubic(&btn_hover_cubic_key(id), 0.0).peek()
    } else {
        use_ui_transition_spring(&btn_hover_spring_key(id), 0.0).peek()
    };
    t.clamp(0.0, 1.0)
}

fn lerp_text_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let af = [
        a[0] as f32 / 255.0,
        a[1] as f32 / 255.0,
        a[2] as f32 / 255.0,
        a[3] as f32 / 255.0,
    ];
    let bf = [
        b[0] as f32 / 255.0,
        b[1] as f32 / 255.0,
        b[2] as f32 / 255.0,
        b[3] as f32 / 255.0,
    ];
    let o = lerp_display_srgb_linear(af, bf, t);
    [
        (o[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// Tailwind / CSS `color-mix(..., fg alpha%, transparent)` 叠在 `background` 上的不透明等价色。
///
/// 使用 **直通道 sRGB alpha 合成**（与文档站 canvas 采样一致：`primary/80`→`#333`、
/// `destructive/10`→`#fde6e7`）。勿用 Oklab 混合替代——会把结果抬亮。
fn color_mix_over_background(fg: [f32; 4], alpha: f32) -> [f32; 4] {
    let bg = parse_color("background");
    let a = alpha.clamp(0.0, 1.0);
    [
        fg[0] * a + bg[0] * (1.0 - a),
        fg[1] * a + bg[1] * (1.0 - a),
        fg[2] * a + bg[2] * (1.0 - a),
        1.0,
    ]
}

/// `color-mix(in oklch, base, other pct%)` — pct 为 other 占比（0–100）。
fn color_mix_oklch_pct(base: [f32; 4], other: [f32; 4], other_pct: f32) -> [f32; 4] {
    let t = (other_pct / 100.0).clamp(0.0, 1.0);
    let mut out = mix_display_srgb_in_oklab(base, other, t);
    out[3] = 1.0;
    out
}

/// 文档站 Aria Button 采样配方（透明度叠底 → 不透明等价色）：
/// - default: `bg-primary` → hover `primary/80`
/// - secondary: `bg-secondary` → hover `color-mix(secondary, foreground 5%)`
/// - destructive: `destructive/10` → hover `destructive/20`；字色为不透明 `destructive`
/// - outline / ghost / link：见各分支
pub(crate) fn button_chrome_for_variant(
    variant: &str,
    hover_t: f32,
) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    let t = hover_t.clamp(0.0, 1.0);
    let dark = is_dark_theme();
    match variant {
        "default" => {
            let rest = parse_color("primary");
            let hover = color_mix_over_background(parse_color("primary"), 0.8);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("primary-foreground"), None)
        }
        "secondary" => {
            let rest = parse_color("secondary");
            let hover =
                color_mix_oklch_pct(parse_color("secondary"), parse_color("foreground"), 5.0);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("secondary-foreground"), None)
        }
        "outline" => {
            let (rest_bg, hover_bg, border) = if dark {
                (
                    color_mix_over_background(parse_color("input"), 0.3),
                    color_mix_over_background(parse_color("input"), 0.5),
                    parse_color("input"),
                )
            } else {
                (
                    parse_color("background"),
                    parse_color("accent"),
                    parse_color("border"),
                )
            };
            let bg = lerp_display_srgb_linear(rest_bg, hover_bg, t);
            let tc = lerp_text_rgba(
                parse_text_color("foreground"),
                parse_text_color("accent-foreground"),
                t,
            );
            (bg, tc, Some(border))
        }
        "ghost" => {
            let rest_bg = [0.0, 0.0, 0.0, 0.0];
            let hover_bg = if dark {
                parse_color("accent").with_alpha(0.5)
            } else {
                parse_color("accent")
            };
            let bg = lerp_premultiplied_display_srgba(rest_bg, hover_bg, t);
            let tc = lerp_text_rgba(
                parse_text_color("foreground"),
                parse_text_color("accent-foreground"),
                t,
            );
            (bg, tc, None)
        }
        "destructive" => {
            let destructive = parse_color("destructive");
            let (rest_a, hover_a) = if dark { (0.2, 0.3) } else { (0.1, 0.2) };
            let rest = color_mix_over_background(destructive, rest_a);
            let hover = color_mix_over_background(destructive, hover_a);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("destructive"), None)
        }
        "link" => ([0.0; 4], parse_text_color("primary"), None),
        _ => {
            let rest = parse_color("primary");
            let hover = color_mix_over_background(parse_color("primary"), 0.8);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("primary-foreground"), None)
        }
    }
}

// ─── ShadCN-UI Button Variants ───────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

impl From<&str> for ButtonVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for ButtonVariant {
    fn from(s: String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl std::str::FromStr for ButtonVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "secondary" => Self::Secondary,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "destructive" => Self::Destructive,
            "link" => Self::Link,
            _ => Self::Default,
        })
    }
}

impl ButtonVariant {
    pub(crate) fn as_paint_key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Secondary => "secondary",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Destructive => "destructive",
            Self::Link => "link",
        }
    }
}

// ─── ShadCN-UI Button Sizes ─────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    #[default]
    Default,
    Xs,
    Sm,
    Lg,
    Icon,
    IconXs,
    IconSm,
    IconLg,
}

impl From<&str> for ButtonSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl From<String> for ButtonSize {
    fn from(s: String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl std::str::FromStr for ButtonSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "xs" => Self::Xs,
            "sm" => Self::Sm,
            "lg" => Self::Lg,
            "icon" => Self::Icon,
            "icon-xs" => Self::IconXs,
            "icon-sm" => Self::IconSm,
            "icon-lg" => Self::IconLg,
            _ => Self::Default,
        })
    }
}

impl ButtonSize {
    fn height(&self) -> &'static str {
        match self {
            Self::Xs | Self::IconXs => "1.5rem",
            Self::Sm | Self::IconSm => "2rem",
            Self::Default | Self::Icon => "2.25rem",
            Self::Lg | Self::IconLg => "2.5rem",
        }
    }

    fn padding_h(&self) -> &'static str {
        match self {
            Self::Xs => "0.5rem",
            Self::Sm => "0.75rem",
            Self::Default => "1rem",
            Self::Lg => "1.5rem",
            _ => "0rem",
        }
    }

    fn padding_v(&self) -> &'static str {
        match self {
            Self::Xs => "0.125rem",
            Self::Sm => "0.25rem",
            Self::Default => "0.5rem",
            Self::Lg => "0.625rem",
            _ => "0rem",
        }
    }

    fn gap(&self) -> &'static str {
        match self {
            Self::Xs | Self::IconXs => "0.25rem",
            Self::Sm | Self::IconSm => "0.375rem",
            _ => "0.5rem",
        }
    }

    fn corner_radius(&self) -> &'static str {
        "0.375rem"
    }

    fn is_icon(&self) -> bool {
        matches!(
            self,
            Self::Icon | Self::IconXs | Self::IconSm | Self::IconLg
        )
    }

    /// 对齐 shadcn/ui `buttonVariants`：基底 `text-sm`，仅 `xs` / `icon-xs` 叠加 `text-xs`。
    fn text_size(&self) -> &'static str {
        match self {
            Self::Xs | Self::IconXs => "0.75rem",
            _ => "0.875rem",
        }
    }
}

// ─── ButtonBuilder ───────────────────────────────────────────────────────────

pub struct ButtonBuilder {
    id: String,
    variant: ButtonVariant,
    size: ButtonSize,
    /// `"spring"` 或 `"cubic"`（`cubic-bezier` 时长过渡）。
    hover_easing: String,
    disabled: bool,
    on_click: Option<Box<dyn FnMut()>>,
    container: BoxBuilder,
    content: Vec<Element>,
    text_style: TextNodeConfig,
    /// `Some` → 覆盖变体字色（含 hover 同步）；Calendar outside days 用。
    explicit_text_color: Option<[u8; 4]>,
    truncate: bool,
    text_overflow: Option<crate::style::TextOverflow>,
}

static NEXT_BUTTON_ID: AtomicUsize = AtomicUsize::new(1);

fn auto_button_id() -> String {
    let n = NEXT_BUTTON_ID.fetch_add(1, Ordering::Relaxed);
    format!("btn-{}", n)
}

/// ShadCN-style Button component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Button(
    id: String,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    hover_easing: String,
    #[prop(default = "Handler::default()")] mut on_click: crate::core::tree::Handler,
    #[prop(default = "KeyHandler::default()")] mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
    truncate: bool,
    text_overflow: Option<String>,
) -> Element {
    let hover_easing =
        if hover_easing.is_empty() || (hover_easing != "spring" && hover_easing != "cubic") {
            "spring".to_string()
        } else {
            hover_easing
        };
    let mut builder = button(&id)
        .variant(variant.as_paint_key())
        .hover_easing(&hover_easing)
        .disabled(disabled)
        .truncate(truncate);
    if !truncate {
        if let Some(ref mode) = text_overflow {
            builder = builder.text_overflow(mode.as_str());
        }
    }

    builder.size = size;
    builder.container = builder.container.gap(size.gap());
    builder.text_style.size_px = IntoF32::into_f32(size.text_size());
    if size.is_icon() {
        builder.container = builder
            .container
            .h(size.height())
            .w(size.height())
            .flex_shrink(0.0);
    } else {
        builder.container = builder.container.min_h(size.height()).flex_shrink(0.0);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    let mut el: Element = builder.into();

    if !disabled {
        let mut old_click = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old_click {
                rc.borrow_mut()();
            }
            on_click.call(());
        });

        let mut old_key = el.on_key_down.clone();
        el = el.on_key_down(move |ev| {
            let mut handled = false;
            if let Some(ref mut rc) = old_key {
                handled = rc.borrow_mut()(ev.clone());
            }
            if handled {
                return true;
            }
            on_key_down.call(ev)
        });
    }

    el
}

pub fn button(id: impl AsRef<str>) -> ButtonBuilder {
    let id = id.as_ref();
    let size = ButtonSize::Default;
    let id = if id.trim().is_empty() {
        auto_button_id()
    } else {
        id.to_string()
    };
    ButtonBuilder {
        id,
        variant: ButtonVariant::Default,
        size,
        hover_easing: "spring".to_string(),
        disabled: false,
        on_click: None,
        container: hstack()
            // ShadCN Button base: `inline-flex shrink-0 items-center justify-center gap-2`
            // （`whitespace-nowrap` 落在文案叶子上，见 From）。
            .rounded(size.corner_radius())
            .gap(size.gap())
            .justify_center()
            .items_center()
            .flex_shrink(0.0),
        content: Vec::new(),
        text_style: TextNodeConfig {
            size_px: IntoF32::into_f32(size.text_size()),
            weight: 500,
            color: [255, 255, 255, 255],
            underline: false,
            underline_offset_px: None,
            center: true,
            line_height_px: None,
            leading_scale: None,
            font_family: None,
        },
        explicit_text_color: None,
        truncate: false,
        text_overflow: None,
    }
}

impl ButtonBuilder {
    pub fn hover_easing(mut self, e: &str) -> Self {
        self.hover_easing = e.to_string();
        self
    }

    pub fn variant(mut self, v: &str) -> Self {
        self.variant = v.parse().unwrap_or(ButtonVariant::Default);
        self
    }

    pub fn size(mut self, s: &str) -> Self {
        self.size = s.parse().unwrap_or(ButtonSize::Default);
        self.container = self.container.gap(self.size.gap());
        self.text_style.size_px = IntoF32::into_f32(self.size.text_size());
        if self.size.is_icon() {
            // Icon buttons keep a fixed square frame.
            self.container = self
                .container
                .min_h("auto")
                .h(self.size.height())
                .w(self.size.height())
                .flex_shrink(0.0);
        } else {
            // Text buttons are content-sized but follow standard min-height.
            self.container = self
                .container
                .min_h(self.size.height())
                .h("auto")
                .w("auto")
                .flex_shrink(0.0);
        }
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.text_style.size_px = size.into_f32();
        self
    }

    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.text_style.color = color;
        self.explicit_text_color = Some(color);
        self
    }

    pub fn text_leading_none(self) -> Self {
        self.leading_none()
    }
    pub fn text_leading_tight(self) -> Self {
        self.leading_tight()
    }
    pub fn text_leading_normal(self) -> Self {
        self.leading_normal()
    }
    pub fn text_leading_relaxed(self) -> Self {
        self.leading_relaxed()
    }
    pub fn text_line_height(self, px: impl IntoF32) -> Self {
        self.line_height(px)
    }

    pub fn on_click(mut self, cb: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(cb));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn truncate(mut self, on: bool) -> Self {
        self.truncate = on;
        self
    }

    /// `clip` / `ellipsis` / `newline`. Overridden by [`Self::truncate`] `true`.
    pub fn text_overflow(mut self, mode: impl Into<crate::style::TextOverflow>) -> Self {
        self.text_overflow = Some(mode.into());
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        // We'll build the text element later during into() to apply the correct variant color
        self.content.push(Element::new().text(content));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.content.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    // Pass-through some layout methods
    pub fn w(mut self, w: impl IntoCssDim) -> Self {
        self.container = self.container.w(w);
        self
    }
    /// Stretch to parent width while keeping ShadCN `shrink-0`.
    ///
    /// Sets `width: 100%` + `min-width: 0` + `max-width: 100%` so flex `min-width: auto`
    /// does not inflate past the parent (Dialog / form rows).
    pub fn w_full(mut self) -> Self {
        self.container = self.container.w("100%").min_w(0.0).max_w("100%");
        self
    }
    pub fn h(mut self, h: impl IntoCssDim) -> Self {
        self.container = self.container.h(h);
        self
    }
    pub fn p(mut self, p: impl IntoCssDim) -> Self {
        self.container = self.container.p(p);
        self
    }
    pub fn gap(mut self, g: impl IntoCssDim) -> Self {
        self.container = self.container.gap(g);
        self
    }
    pub fn bg(mut self, color: [f32; 4]) -> Self {
        self.container = self.container.bg(color);
        self
    }
    pub fn rounded(mut self, r: impl IntoF32) -> Self {
        self.container = self.container.rounded(r);
        self
    }

    pub fn flex_grow(mut self, grow: impl crate::core::tree::IntoF32Raw) -> Self {
        self.container = self.container.flex_grow(grow);
        self
    }

    pub fn flex_shrink(mut self, shrink: impl crate::core::tree::IntoF32Raw) -> Self {
        self.container = self.container.flex_shrink(shrink);
        self
    }

    pub fn line_height(mut self, lh: impl IntoF32) -> Self {
        self.text_style.line_height_px = Some(lh.into_f32());
        self.text_style.leading_scale = None;
        self
    }

    pub fn leading_none(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(1.0);
        self
    }

    pub fn leading_tight(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(1.25);
        self
    }

    pub fn leading_normal(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(1.5);
        self
    }

    pub fn leading_relaxed(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(1.625);
        self
    }
}

impl From<ButtonBuilder> for Element {
    fn from(builder: ButtonBuilder) -> Element {
        let id_hover = format!("{}_hover", builder.id);
        let hovered = use_signal(&id_hover, || false);
        let focused = use_signal("__rui_button_focus_id", String::new);
        let focus_visible = use_signal("__rui_focus_visible", || false);

        // `peek`：不把 hover / 焦点订阅到根 `create_effect`，避免悬停、点输入框、Tab 等触发整树 `rebuild_ui`。
        // hover 由 `sync_button_hover_paint_subtree` 写回；焦点环由同函数内 `sync_button_focus_ring_shadow` 写回。
        let is_hovered = hovered.peek() && !builder.disabled;
        let is_focused = focused.peek() == builder.id && focus_visible.peek() && !builder.disabled;

        let hover_t = if builder.disabled {
            0.0
        } else {
            hover_t_for_id(&builder.id, &builder.hover_easing)
        };
        let t = hover_t;
        // Focus ring is snapped (not sprung): keyboard focus must paint the same frame as Tab.
        let focus_spring = use_ui_transition_spring(
            &btn_focus_spring_key(&builder.id),
            if is_focused { 1.0 } else { 0.0 },
        );
        focus_spring.snap(if is_focused { 1.0 } else { 0.0 });
        let focus_t = if is_focused { 1.0 } else { 0.0 };

        // Apply size-based padding if not specified
        let mut container = builder.container;
        if !builder.size.is_icon() {
            container = container
                .px(builder.size.padding_h())
                .py(builder.size.padding_v());
        }

        let (mut bg, variant_text_color, border) =
            button_chrome_for_variant(builder.variant.as_paint_key(), t);
        let text_color = builder.explicit_text_color.unwrap_or(variant_text_color);
        if builder.disabled {
            bg[3] *= 0.5;
        }
        container = container.bg(bg);
        if let Some(mut b_color) = border {
            if is_focused && !builder.disabled {
                b_color = if builder.variant == ButtonVariant::Destructive {
                    focus_ring_destructive_color()
                } else {
                    focus_ring_border_color()
                };
            } else if builder.disabled && builder.variant == ButtonVariant::Outline {
                b_color[3] *= 0.5;
            }
            container = container.border(b_color, 1.0);
        }
        let vkey = builder.variant.as_paint_key();
        let rest_sh = button_shadow_style_for_sync(false, builder.disabled, vkey);
        let ring_sh = button_shadow_style_for_sync(true, builder.disabled, vkey);
        let (sh_blur, sh_col, sh_off, sh_spread) = lerp_shadow_params(rest_sh, ring_sh, focus_t);
        if sh_blur > 0.0 || sh_col[3] > 0.0 {
            container = container
                .box_shadow_color(sh_col)
                .box_shadow_blur(sh_blur)
                .box_shadow_offset_x(sh_off[0])
                .box_shadow_offset_y(sh_off[1])
                .box_shadow_spread(sh_spread)
                .box_shadow_inset(false);
        }

        let overflow_mode = if builder.truncate {
            Some(crate::style::TextOverflow::Ellipsis)
        } else {
            builder.text_overflow
        };
        let caption_shrink = overflow_mode.is_some();

        let mut el: Element = container.into();
        match overflow_mode {
            Some(crate::style::TextOverflow::Newline) => {
                el = el.min_w(0.0).flex_shrink(1.0);
            }
            Some(_) => {
                el = el.min_w(0.0).overflow("hidden").flex_shrink(1.0);
            }
            None => {}
        }
        el.template_id = Some(builder.id.clone());
        el.button_hover_paint = Some(ButtonHoverPaint {
            variant: builder.variant.as_paint_key().to_string(),
            disabled: builder.disabled,
            hover_easing: builder.hover_easing.clone(),
            text_color: builder.explicit_text_color,
        });

        for mut child in builder.content {
            // Always build caption through `build_text_node` so measure/paint match static
            // `"…"` children. Re-attach bind_text identity (template_id + unmount) so Solid-style
            // updates keep targeting this leaf — preserving the raw bind shell used to leave a
            // zero-content outline button in gallery (Mark invalid).
            if let Some(text_content) = child.text.take() {
                let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
                let mut color = text_color;
                if builder.disabled {
                    color[3] = (color[3] as f32 * 0.5) as u8;
                }
                let underline = builder.variant == ButtonVariant::Link && is_hovered;
                let is_lucide_glyph = child.text_style.font_family.as_deref() == Some("lucide");
                let size_px = if is_lucide_glyph {
                    child.text_style.size
                } else {
                    builder.text_style.size_px
                };
                let mut text_node = build_text_node(
                    text_content,
                    TextNodeConfig {
                        color,
                        underline,
                        underline_offset_px: if builder.variant == ButtonVariant::Link {
                            Some(LINK_UNDERLINE_OFFSET_PX)
                        } else {
                            builder.text_style.underline_offset_px
                        },
                        font_family: child.text_style.font_family.clone(),
                        size_px,
                        weight: if is_lucide_glyph {
                            400
                        } else {
                            builder.text_style.weight
                        },
                        ..builder.text_style.clone()
                    },
                )
                .text_center()
                .flex_shrink(if caption_shrink { 1.0 } else { 0.0 });
                if !is_lucide_glyph {
                    match overflow_mode {
                        Some(mode) => {
                            text_node = text_node.text_overflow(mode).min_w(0.0);
                        }
                        None => {
                            text_node = text_node.whitespace_nowrap();
                        }
                    }
                }
                if preserve_binding {
                    if let Some(tid) = child.template_id.take() {
                        text_node.template_id = Some(tid);
                    }
                    if let Some(hook) = child.on_unmount.take() {
                        text_node.on_unmount = Some(hook);
                    }
                }
                el = el.child(text_node);
            } else {
                el = el.child(child);
            }
        }

        let mut on_click = builder.on_click;
        let disabled = builder.disabled;
        let focus_id = builder.id.clone();
        let hover_spring_cb = use_ui_transition_spring(&btn_hover_spring_key(&builder.id), 0.0);
        let cubic_cb = use_ui_transition_cubic(&btn_hover_cubic_key(&builder.id), 0.0);
        let hover_easing = builder.hover_easing.clone();

        interactive(el)
            .focus_id(builder.id)
            .focusable(!disabled)
            .on_hover(move |h| {
                let v = if disabled {
                    0.0
                } else if h {
                    1.0
                } else {
                    0.0
                };
                if hover_easing == "cubic" {
                    cubic_cb.set(v);
                } else {
                    hover_spring_cb.set(v);
                }
                let v = if disabled { false } else { h };
                if hovered.peek() != v {
                    hovered.set_silent(v);
                    mark_hover_paint_dirty();
                }
            })
            .on_click(move || {
                if !disabled {
                    focused.set(focus_id.clone());
                    if let Some(ref mut cb) = on_click {
                        cb();
                    }
                }
            })
            .into()
    }
}

fn paint_button_root_from_hover(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    desc: &ButtonHoverPaint,
    focus_id: &str,
    is_hovered: bool,
    is_focused: bool,
    impact: &mut LayoutStatus,
) {
    let hover_t = hover_t_for_id(focus_id, &desc.hover_easing);
    let t = if desc.disabled { 0.0 } else { hover_t };
    let (mut bg, variant_text_color, border) = button_chrome_for_variant(&desc.variant, t);
    let text_color = desc.text_color.unwrap_or(variant_text_color);
    if desc.disabled {
        bg[3] *= 0.5;
    }
    let border = border.map(|mut c| {
        if is_focused && !desc.disabled {
            c = if desc.variant == "destructive" {
                focus_ring_destructive_color()
            } else {
                focus_ring_border_color()
            };
        } else if desc.disabled && desc.variant == "outline" {
            c[3] *= 0.5;
        }
        c
    });
    let underline = desc.variant == "link" && is_hovered;
    let target_bw = if border.is_some() { 1.0_f32 } else { 0.0_f32 };
    let root_color_ok = match border {
        Some(b_color) => el.style.border_color == b_color,
        None => true,
    };
    let root_ok = el.style.bg_color == bg && el.style.border_width == target_bw && root_color_ok;
    let mut text_children_ok = true;
    for child in &el.children {
        if child.text.is_none() {
            continue;
        }
        let mut c = text_color;
        if desc.disabled {
            c[3] = ((c[3] as f32) * 0.5).round() as u8;
        }
        let want_uo = if desc.variant == "link" {
            LINK_UNDERLINE_OFFSET_PX
        } else {
            child.text_style.underline_offset
        };
        // link：保持 `None`（currentColor），勿钉死 Some(rgb)。
        if child.text_style.underline != underline
            || child.text_style.color != c
            || (desc.variant == "link" && child.text_style.decoration_color.is_some())
            || (child.text_style.underline_offset - want_uo).abs() > 0.01
        {
            text_children_ok = false;
            break;
        }
    }
    if root_ok && text_children_ok {
        return;
    }

    let old_bw = el.style.border_width;
    let old_bg = el.style.bg_color;
    let old_bc = el.style.border_color;

    el.style.bg_color = bg;
    match border {
        Some(b_color) => {
            el.style.border_color = b_color;
            el.style.border_width = 1.0;
        }
        None => {
            el.style.border_width = 0.0;
        }
    }
    let mut layout_hit = el.style.border_width != old_bw;
    let mut style_hit =
        !layout_hit && (el.style.bg_color != old_bg || el.style.border_color != old_bc);
    layout_tree.update_taffy_node_style(el);

    for child in &mut el.children {
        if child.text.is_none() {
            continue;
        }
        let prev_u = child.text_style.underline;
        let old_tc = child.text_style.color;
        let mut color = text_color;
        if desc.disabled {
            color[3] = ((color[3] as f32) * 0.5).round() as u8;
        }
        let prev_uo = child.text_style.underline_offset;
        let prev_dc = child.text_style.decoration_color;
        child.text_style.color = color;
        child.text_style.underline = underline;
        // Protect against cascade wipe on LayoutChanged (same as `Element::underline`).
        child
            .attrs
            .insert("text_decoration_line".to_string(), "1".to_string());
        child
            .attrs
            .insert("text_color".to_string(), "1".to_string());
        if desc.variant == "link" {
            child.text_style.underline_offset = LINK_UNDERLINE_OFFSET_PX;
            // currentColor：勿写 Some(rgb)/attr，让 cascade 与 prepare_text 跟字色走。
            child.text_style.decoration_color = None;
            child.attrs.remove("text_decoration_color");
            child
                .attrs
                .insert("text_underline_offset".to_string(), "1".to_string());
        }
        if prev_u != underline {
            layout_hit = true;
        } else if child.text_style.color != old_tc
            || prev_dc != child.text_style.decoration_color
            || (prev_uo - child.text_style.underline_offset).abs() > 0.01
        {
            style_hit = true;
        }
        if child.layout_node.is_some() {
            layout_tree.update_taffy_node_style(child);
        }
    }
    if layout_hit {
        *impact = impact.merge(LayoutStatus::LayoutChanged);
        push_button_paint_damage(el);
    } else if style_hit {
        *impact = impact.merge(LayoutStatus::StyleChanged);
        push_button_paint_damage(el);
    }
}

/// Local dirty rect for hover/focus chrome (WP-2.2 — avoid full-window Clear).
fn push_button_paint_damage(el: &Element) {
    let b = el.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    let pad = el.style.shadow_blur + el.style.shadow_spread.abs() + 8.0;
    crate::app::damage::push_frame_paint_damage([
        b[0] - pad,
        b[1] - pad,
        b[2] + pad * 2.0,
        b[3] + pad * 2.0,
    ]);
}

fn sync_button_focus_ring_shadow(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    disabled: bool,
    is_focused: bool,
    variant: &str,
    focus_id: &str,
    impact: &mut LayoutStatus,
) {
    // Instant focus-visible ring. Spring blending lagged behind Tab when `tick_animations`
    // ran before this sync: after idle, target flipped but no further redraw → missed move,
    // then dual rings when springs finally caught up on the next key.
    let focus_spring = use_ui_transition_spring(
        &btn_focus_spring_key(focus_id),
        if is_focused { 1.0 } else { 0.0 },
    );
    focus_spring.snap(if is_focused { 1.0 } else { 0.0 });
    let focus_t = if is_focused { 1.0 } else { 0.0 };
    let rest_sh = button_shadow_style_for_sync(false, disabled, variant);
    let ring_sh = button_shadow_style_for_sync(true, disabled, variant);
    let (want_blur, want_color, want_off, want_spread) =
        lerp_shadow_params(rest_sh, ring_sh, focus_t);
    let want_inset = false;
    let blur_ok = (el.style.shadow_blur - want_blur).abs() <= 0.01;
    let color_ok = (el.style.shadow_color[0] - want_color[0]).abs() <= 0.001
        && (el.style.shadow_color[1] - want_color[1]).abs() <= 0.001
        && (el.style.shadow_color[2] - want_color[2]).abs() <= 0.001
        && (el.style.shadow_color[3] - want_color[3]).abs() <= 0.001;
    let off_ok = (el.style.shadow_offset[0] - want_off[0]).abs() <= 0.01
        && (el.style.shadow_offset[1] - want_off[1]).abs() <= 0.01;
    let spread_ok = (el.style.shadow_spread - want_spread).abs() <= 0.01;
    let inset_ok = el.style.shadow_inset == want_inset;

    // `focus-visible:border-ring` for variants that already have a border (e.g. outline).
    let mut border_dirty = false;
    if el.style.border_width > 0.5 {
        let want_border = if is_focused && !disabled {
            if variant == "destructive" {
                focus_ring_destructive_color()
            } else {
                focus_ring_border_color()
            }
        } else {
            let (_, _, border) = button_chrome_for_variant(variant, 0.0);
            let mut c = border.unwrap_or(parse_color("border"));
            if disabled && variant == "outline" {
                c[3] *= 0.5;
            }
            c
        };
        if el.style.border_color != want_border {
            el.style.border_color = want_border;
            border_dirty = true;
        }
    }

    if blur_ok && color_ok && off_ok && spread_ok && inset_ok && !border_dirty {
        return;
    }
    el.style.shadow_color = want_color;
    el.style.shadow_blur = want_blur;
    el.style.shadow_offset = want_off;
    el.style.shadow_spread = want_spread;
    el.style.shadow_inset = want_inset;
    layout_tree.update_taffy_node_style(el);
    *impact = impact.merge(LayoutStatus::StyleChanged);
    push_button_paint_damage(el);
}

/// Solid-style in-place bind for `<Button variant={sig.get()}>`.
///
/// Updates [`ButtonHoverPaint::variant`] and rest-state chrome without rebuilding the button tree.
pub fn apply_bound_button_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let variant = variant.as_ref();
    let Some(desc) = el.button_hover_paint.as_mut() else {
        return LayoutStatus::Clean;
    };
    if desc.variant == variant {
        return LayoutStatus::Clean;
    }
    desc.variant = variant.to_string();
    let (bg, variant_text_color, border) = button_chrome_for_variant(variant, 0.0);
    let text_color = desc.text_color.unwrap_or(variant_text_color);
    el.style.bg_color = bg;
    match border {
        Some(b) => {
            el.style.border_color = b;
            el.style.border_width = 1.0;
        }
        None => {
            el.style.border_width = 0.0;
        }
    }
    for child in &mut el.children {
        if child.text.is_some() {
            child.text_style.color = text_color;
        }
    }
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Button disabled={sig.get()}>`.
pub fn apply_bound_button_disabled(el: &mut Element, disabled: bool) -> LayoutStatus {
    let Some(desc) = el.button_hover_paint.as_mut() else {
        return LayoutStatus::Clean;
    };
    if desc.disabled == disabled {
        return LayoutStatus::Clean;
    }
    desc.disabled = disabled;
    el.focusable = !disabled;
    LayoutStatus::StyleChanged
}

/// 把 `{focus_id}_hover` 与全局焦点环同步到带 [`Element::button_hover_paint`] 的节点（不经过根 `create_effect`）。
pub(crate) fn sync_button_hover_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    if let Some(desc) = el.button_hover_paint.clone() {
        if let Some(fid) = el.focus_id.clone() {
            let key = format!("{}_hover", fid);
            let is_hovered = use_signal(&key, || false).peek() && !desc.disabled;
            let focused = use_signal("__rui_button_focus_id", String::new);
            let focus_visible = use_signal("__rui_focus_visible", || false);
            let is_focused = !desc.disabled && focus_visible.peek() && focused.peek() == fid;
            paint_button_root_from_hover(
                el,
                layout_tree,
                &desc,
                &fid,
                is_hovered,
                is_focused,
                impact,
            );
            sync_button_focus_ring_shadow(
                el,
                layout_tree,
                desc.disabled,
                is_focused,
                &desc.variant,
                &fid,
                impact,
            );
        }
    }
    for child in &mut el.children {
        sync_button_hover_paint_subtree(child, layout_tree, impact);
    }
}

impl crate::core::tree::IntoElementList for ButtonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::{button_chrome_for_variant, color_mix_over_background};
    use crate::style::{parse_color, parse_text_color, set_theme_mode, ThemeMode};

    fn hex(c: [f32; 4]) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (c[2].clamp(0.0, 1.0) * 255.0).round() as u8
        )
    }

    fn luminance(c: [f32; 4]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    #[test]
    fn primary_matches_docs_black_and_hover_80() {
        set_theme_mode(ThemeMode::Light);
        let (rest, _, _) = button_chrome_for_variant("default", 0.0);
        let (hover, _, _) = button_chrome_for_variant("default", 1.0);
        assert_eq!(hex(rest), "#000000");
        assert_eq!(hex(hover), "#333333");
    }

    #[test]
    fn secondary_hover_darkens_via_foreground_mix() {
        set_theme_mode(ThemeMode::Light);
        let (rest, _, _) = button_chrome_for_variant("secondary", 0.0);
        let (hover, fg, _) = button_chrome_for_variant("secondary", 1.0);
        assert_eq!(fg, parse_text_color("secondary-foreground"));
        assert_eq!(hex(rest), "#F5F5F5");
        assert_eq!(hex(hover), "#E5E5E5");
        assert!(luminance(hover) < luminance(rest));
    }

    #[test]
    fn explicit_text_color_overrides_ghost_chrome() {
        use super::button;
        use crate::core::reactivity::create_root;
        use crate::core::tree::Element;
        let muted = parse_text_color("muted-foreground");
        let (_h, el) = create_root(|| -> Element {
            button("t-btn-muted")
                .variant("ghost")
                .text("26")
                .text_color(muted)
                .into()
        });
        let fg = el
            .children
            .iter()
            .find_map(|c| c.text.as_ref().map(|_| c.text_style.color));
        assert_eq!(fg, Some(muted));
        assert_eq!(
            el.button_hover_paint.as_ref().and_then(|p| p.text_color),
            Some(muted)
        );
    }

    #[test]
    fn destructive_soft_tint_with_opaque_destructive_text() {
        set_theme_mode(ThemeMode::Light);
        let destructive = parse_color("destructive");
        let (rest, fg, _) = button_chrome_for_variant("destructive", 0.0);
        let (hover, hover_fg, _) = button_chrome_for_variant("destructive", 1.0);
        assert_eq!(fg, parse_text_color("destructive"));
        assert_eq!(hover_fg, parse_text_color("destructive"));
        assert_eq!(fg[3], 255);
        assert_eq!(hex(rest), hex(color_mix_over_background(destructive, 0.1)));
        assert_eq!(hex(hover), hex(color_mix_over_background(destructive, 0.2)));
        assert!(luminance(hover) < luminance(rest));
    }
}
