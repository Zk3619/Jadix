//! # Button
//!
//! ShadCN style clickable button: variant (default / outline / ghost / link / success /
//! warning / destructive / ...), size, disable, hover transition and focus ring.
//!
//! ## Usage
//! ```ignore
//! <Button variant="outline" size="sm" on_click={move |_| { … }}>"保存"</Button>
//! <Button id="hw-hover" variant="ghost">"hover"</Button>
//! <Button variant="success">"Complete"</Button>
//! <Button variant="warn">"注意"</Button> // warn ≡ warning
//! // Or: Element::from(button("ok").variant("default").on_click(…))
//! ```
//!
//! ## Notes
//! - **Alignment ShadCN/solid-ui**: Normal copy default `whitespace-nowrap` + `flex-shrink: 0`
//!  (Base class `inline-flex shrink-0 … whitespace-nowrap`), when the dynamic copy becomes wider, the button will be widened instead of wrapping internally.
//!  Lucide 图标叶不加 nowrap，避免方按钮内图标偏右。
//! - **`id` is optional**: RSX `<Button>` without `id` gets a compile-time
//!   `__rui_button_auto_N` (same contract as Input/Checkbox/Switch). Builder
//!   `button("")` still allocates a session `btn-N` (not stable across rebuilds).
//! - Multi-instance uncontrolled, stable focus, smoke selectors: please explicitly stabilize the `id` (see `docs/COMPONENT_LIBRARY.md` §4.2).
//! - Hover / focus using `peek` + paint sync per frame; hover color walking CSS `transition-colors` (~150ms).
//! - Child nodes can be text or icons; Lucide icons retain their own `text_size`.
//! - `ui!` multi-child packs into a `rui_fragment`; Button unpacks via
//!   [`crate::utils::take_packed`] so captions get the variant foreground
//!   (outline → `foreground`). Slotted shells such as
//!   [`crate::components::Kbd`] / `KbdGroup` keep their own chrome.

use crate::components::base::SlotChrome;
use crate::core::reactivity::use_signal;
use crate::core::tree::{
    parse_color, parse_text_color, ButtonHoverPaint, Element, IntoCssDim, IntoF32, IntoNodeBg,
    IntoNodeSize, LayoutStatus,
};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::style::{
    apply_focus_ring_shadow, apply_focus_ring_shadow_color, button_soft_semantic_chrome,
    color_mix_over_background, ensure_class_list, extend_class_list, focus_ring_border_color,
    focus_ring_color, focus_ring_destructive_color, focus_ring_shadow_matches_color, is_dark_theme,
    lerp_paint_chrome, ClassSet, PaintChrome, SemanticTone, TimingFunction, TransitionEntry,
    TransitionProperty, ATTR_SYNC_LABEL_FG,
};
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use std::sync::atomic::{AtomicUsize, Ordering};

const ATTR_SLOT: &str = "data-slot";

/// Keycap shells keep muted chrome during hover / bind paint (do not recolor captions).
fn is_preserved_button_child_slot(el: &Element) -> bool {
    matches!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some("kbd" | "kbd-group")
    )
}

/// Tailwind v4 `shadow-xs` (constants reserved for documentation/reference; current Button variants no longer have shadow by default).
#[allow(dead_code)]
const SHADOW_XS: &str = "0 1px 2px 0 #0000000d";

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}

/// Fallback ring color for Button (theme `ring`, or destructive). Authored
/// [`Style::focus_ring_color`] still wins inside [`apply_focus_ring_shadow_color`].
fn button_ring_fallback(variant: &str) -> [f32; 4] {
    if variant == "destructive" {
        focus_ring_destructive_color()
    } else {
        focus_ring_color()
    }
}

/// CSS hover chrome duration (short to `transition-colors`).
const BUTTON_HOVER_TRANSITION_MS: f32 = 150.0;

fn ensure_button_css_transition(el: &mut Element) {
    let timing = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0);
    for prop in [
        TransitionProperty::BackgroundColor,
        TransitionProperty::BorderColor,
        TransitionProperty::BoxShadow,
    ] {
        if let Some(entry) = el.style.transition.iter_mut().find(|e| e.property == prop) {
            entry.duration_ms = BUTTON_HOVER_TRANSITION_MS;
            entry.timing = timing;
        } else {
            el.style.transition.push(TransitionEntry {
                property: prop,
                duration_ms: BUTTON_HOVER_TRANSITION_MS,
                delay_ms: 0.0,
                timing,
            });
        }
    }
}

fn hover_t_from_bool(hovered: bool, disabled: bool) -> f32 {
    if disabled || !hovered {
        0.0
    } else {
        1.0
    }
}

/// `link` + explicit rest text color (such as Breadcrumb muted): hover cuts `foreground`.
fn link_caption_color(
    explicit_rest: Option<[u8; 4]>,
    variant_text: [u8; 4],
    is_hovered: bool,
) -> [u8; 4] {
    match (explicit_rest, is_hovered) {
        (Some(_rest), true) => parse_text_color("foreground"),
        (Some(rest), false) => rest,
        (None, _) => variant_text,
    }
}

fn color4_near(a: [f32; 4], b: [f32; 4]) -> bool {
    (0..4).all(|i| (a[i] - b[i]).abs() <= 1e-4)
}

/// 文档站 Aria Button 采样配方（透明度叠底 → 不透明等价色）：
/// - default: `bg-primary` → hover `primary/80`
/// - secondary: `bg-secondary` → hover `secondary/80`
/// - success / warning / destructive: soft tint via [`button_soft_semantic_chrome`] / [`SemanticTone`]
/// - outline / ghost / link: see each branch
pub(crate) fn button_chrome_for_variant(
    variant: &str,
    hover_t: f32,
) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    let t = hover_t.clamp(0.0, 1.0);
    let dark = is_dark_theme();
    if let Some(tone) = match variant {
        "success" | "warning" | "destructive" | "info" => Some(SemanticTone::parse(variant)),
        _ => None,
    } {
        if tone.is_semantic() {
            return button_soft_semantic_chrome(tone, t);
        }
    }
    match variant {
        "default" => {
            let fg = parse_text_color("primary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("primary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: color_mix_over_background(parse_color("primary"), 0.8),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
        "secondary" => {
            let fg = parse_text_color("secondary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("secondary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: parse_color("secondary/80"),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
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
            let fg = parse_text_color("foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: rest_bg,
                    fg,
                    border: Some(border),
                },
                PaintChrome {
                    bg: hover_bg,
                    fg,
                    border: Some(border),
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
        "ghost" => {
            let hover_bg = if dark {
                parse_color("accent").with_alpha(0.5)
            } else {
                parse_color("accent")
            };
            // 与 hover 同 RGB、α=0：CSS 非预乘插值只动 alpha，避免透明黑→有色脏闪。
            // Do not use `lerp_premultiplied` (it will fold into black when α≈0), the endpoints must retain RGB.
            let rest_bg = [hover_bg[0], hover_bg[1], hover_bg[2], 0.0];
            let bg = if t >= 0.5 { hover_bg } else { rest_bg };
            (bg, parse_text_color("foreground"), None)
        }
        "link" => ([0.0; 4], parse_text_color("primary"), None),
        _ => {
            let fg = parse_text_color("primary-foreground");
            let c = lerp_paint_chrome(
                PaintChrome {
                    bg: parse_color("primary"),
                    fg,
                    border: None,
                },
                PaintChrome {
                    bg: color_mix_over_background(parse_color("primary"), 0.8),
                    fg,
                    border: None,
                },
                t,
            );
            (c.bg, c.fg, c.border)
        }
    }
}

/// ShadCN Button CVA color classes. Hover/disabled go through class variants
/// (same path as Tabs/Toggle), so author `class="hover:bg-*"` wins by CSS order.
pub(crate) fn button_variant_color_classes(variant: &str) -> &'static str {
    match variant {
        "secondary" => {
            "bg-secondary text-secondary-foreground hover:bg-secondary/80 disabled:opacity-50"
        }
        "outline" => {
            "bg-background text-foreground hover:bg-accent hover:text-foreground \
             dark:bg-input/30 dark:hover:bg-input/50 dark:hover:text-foreground \
             disabled:opacity-50"
        }
        "ghost" => {
            "text-foreground hover:bg-accent hover:text-foreground \
             dark:hover:bg-accent/50 disabled:opacity-50"
        }
        "link" => {
            "text-primary underline-offset-[4px] hover:underline hover:text-primary \
             disabled:opacity-50"
        }
        "destructive" => {
            "bg-destructive/10 text-destructive hover:bg-destructive/20 \
             dark:bg-destructive/20 dark:hover:bg-destructive/30 disabled:opacity-50"
        }
        "success" => {
            "bg-success/10 text-success hover:bg-success/20 \
             dark:bg-success/20 dark:hover:bg-success/30 disabled:opacity-50"
        }
        "warning" => {
            "bg-warning/10 text-warning hover:bg-warning/20 \
             dark:bg-warning/20 dark:hover:bg-warning/30 disabled:opacity-50"
        }
        "info" => {
            "bg-info/10 text-info hover:bg-info/20 \
             dark:bg-info/20 dark:hover:bg-info/30 disabled:opacity-50"
        }
        _ => "bg-primary text-primary-foreground hover:bg-primary/80 disabled:opacity-50",
    }
}

// ─── ShadCN-UI Button Variants ───────────────────────────────────────────────
/// Button `variant`: ShadCN baseline + semantic soft tint (success/warning) + Toast homologous color.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonVariant {
    #[default]
    Default,
    Secondary,
    Outline,
    Ghost,
    Success,
    Warning,
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

impl From<&String> for ButtonVariant {
    fn from(s: &String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl std::str::FromStr for ButtonVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "secondary" => Self::Secondary,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "success" => Self::Success,
            "warning" | "warn" => Self::Warning,
            "destructive" | "error" => Self::Destructive,
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
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Destructive => "destructive",
            Self::Link => "link",
        }
    }
}

// ─── ShadCN-UI Button Sizes ─────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonSize {
    #[default]
    Default,
    /// Inline text chain (Breadcrumb / inline navigation): zero padding · `h-auto`.
    Inline,
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

impl From<&String> for ButtonSize {
    fn from(s: &String) -> Self {
        s.parse().unwrap_or(Self::Default)
    }
}

impl std::str::FromStr for ButtonSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "inline" => Self::Inline,
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Xs => "xs",
            Self::Sm => "sm",
            Self::Default => "default",
            Self::Lg => "lg",
            Self::Icon => "icon",
            Self::IconXs => "icon-xs",
            Self::IconSm => "icon-sm",
            Self::IconLg => "icon-lg",
        }
    }

    fn height(&self) -> &'static str {
        match self {
            Self::Inline => "auto",
            Self::Xs | Self::IconXs => "1.5rem",
            Self::Sm | Self::IconSm => "2rem",
            Self::Default | Self::Icon => "2.25rem",
            Self::Lg | Self::IconLg => "2.5rem",
        }
    }

    fn padding_h(&self) -> &'static str {
        match self {
            Self::Inline => "0",
            Self::Xs => "0.5rem",
            Self::Sm => "0.75rem",
            Self::Default => "1rem",
            Self::Lg => "1.5rem",
            _ => "0rem",
        }
    }

    fn padding_v(&self) -> &'static str {
        match self {
            Self::Inline => "0",
            Self::Xs => "0.125rem",
            Self::Sm => "0.25rem",
            Self::Default => "0.5rem",
            Self::Lg => "0.625rem",
            _ => "0rem",
        }
    }

    fn gap(&self) -> &'static str {
        match self {
            Self::Inline | Self::Xs | Self::IconXs => "0.25rem",
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
            Self::Inline => "0.875rem",
            Self::Xs | Self::IconXs => "0.75rem",
            _ => "0.875rem",
        }
    }

    fn is_inline(self) -> bool {
        matches!(self, Self::Inline)
    }
}

// ─── ButtonBuilder ───────────────────────────────────────────────────────────

pub struct ButtonBuilder {
    id: String,
    variant: ButtonVariant,
    size: ButtonSize,
    /// 保留字段：历史 `hover_easing`；hover 现由 CSS `transition-colors` 驱动，此值忽略。
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
    chrome: SlotChrome,
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
    let hover_easing = if hover_easing.is_empty() || hover_easing == "spring" {
        "cubic".to_string()
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
    if size.is_inline() {
        builder.container = builder
            .container
            .w("auto")
            .h("auto")
            .min_h(0.0)
            .flex_shrink(0.0);
    } else if size.is_icon() {
        builder.container = builder
            .container
            .h(size.height())
            .w(size.height())
            .flex_shrink(0.0);
    } else {
        builder.container = builder.container.min_h(size.height()).flex_shrink(0.0);
    }

    if let Some(c) = children {
        for kid in take_packed(c) {
            builder = builder.child(kid);
        }
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
        hover_easing: "cubic".to_string(),
        disabled: false,
        on_click: None,
        container: hstack()
            // ShadCN Button base: `inline-flex shrink-0 items-center justify-center gap-2`
            // (`whitespace-nowrap` falls on copywriting leaves, see From).
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
            text_align: crate::style::TextAlign::Center,
            line_height_px: None,
            leading_scale: None,
            font_family: None,
        },
        explicit_text_color: None,
        truncate: false,
        text_overflow: None,
        chrome: SlotChrome::default(),
    }
}

impl ButtonBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn hover_easing(mut self, e: &str) -> Self {
        self.hover_easing = e.to_string();
        self
    }

    pub fn variant(mut self, v: impl Into<ButtonVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn size(mut self, s: impl Into<ButtonSize>) -> Self {
        self.size = s.into();
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

    pub fn font_weight(mut self, weight: u16) -> Self {
        self.text_style.weight = weight;
        self
    }

    pub fn text_color(mut self, color: impl crate::style::IntoTextColor) -> Self {
        let color = color.into_text_color();
        self.text_style.color = color;
        self.explicit_text_color = Some(color);
        self
    }

    /// Link `underline-offset`. Logical px; CSS `text-underline-offset` (positive = down).
    /// Overrides the variant class (`underline-offset-[4px]`).
    pub fn text_underline_offset(mut self, px: impl IntoF32) -> Self {
        self.text_style.underline_offset_px = Some(px.into_f32());
        self
    }

    pub fn text_leading_none(self) -> Self {
        self.leading_none()
    }
    pub fn text_leading_tight(self) -> Self {
        self.leading_tight()
    }
    pub fn text_leading_snug(self) -> Self {
        self.leading_snug()
    }
    pub fn text_leading_normal(self) -> Self {
        self.leading_normal()
    }
    pub fn text_leading_relaxed(self) -> Self {
        self.leading_relaxed()
    }
    pub fn text_leading_loose(self) -> Self {
        self.leading_loose()
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

    pub fn cursor(mut self, value: impl crate::style::IntoCursor) -> Self {
        self.container = self.container.cursor(value);
        self
    }

    pub fn truncate(mut self, on: bool) -> Self {
        self.truncate = on;
        self
    }

    /// `clip` / `hidden` / `ellipsis` / `newline`. Overridden by [`Self::truncate`] `true`.
    pub fn text_overflow(mut self, mode: impl Into<crate::style::TextOverflow>) -> Self {
        self.text_overflow = Some(mode.into());
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        // We'll build the text element later during into() to apply the correct variant color
        self.content.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.content.extend(take_packed(child.into()));
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }

    // Pass-through some layout methods
    #[track_caller]
    pub fn w(mut self, w: impl IntoNodeSize) -> Self {
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
    #[track_caller]
    pub fn h(mut self, h: impl IntoNodeSize) -> Self {
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
    #[track_caller]
    pub fn bg(mut self, color: impl IntoNodeBg) -> Self {
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

    pub fn leading_snug(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(1.375);
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

    pub fn leading_loose(mut self) -> Self {
        self.text_style.line_height_px = None;
        self.text_style.leading_scale = Some(2.0);
        self
    }
}

fn tree_holds_morph_icon(el: &Element) -> bool {
    el.attrs.contains_key("rui_morph_icon") || el.children.iter().any(tree_holds_morph_icon)
}

impl From<ButtonBuilder> for Element {
    fn from(builder: ButtonBuilder) -> Element {
        let chrome = builder.chrome.clone();
        let caption_underline_offset = builder.text_style.underline_offset_px;

        let id_hover = format!("{}_hover", builder.id);
        let hovered = use_signal(&id_hover, || false);
        let focused = use_signal("__rui_button_focus_id", String::new);
        let focus_visible = use_signal("__rui_focus_visible", || false);

        // `try_peek`: do not subscribe hover/focus to the root `create_effect`.
        // hover 由 `sync_button_hover_paint_subtree` 写回；焦点环由同函数内 `sync_button_focus_ring_shadow` 写回。
        let is_hovered = hovered.try_peek().unwrap_or(false) && !builder.disabled;
        let is_focused = focused.try_peek().as_deref() == Some(builder.id.as_str())
            && focus_visible.try_peek().unwrap_or(false)
            && !builder.disabled;

        // Rest chrome only at build. Hover/disabled colors are class variants
        // (`button_variant_color_classes`) resolved like CSS `:hover`.
        let t = 0.0_f32;
        // Focus ring is instantaneous (same frame as Tab); applied after class list
        // so `ring-2` / `.ring_color()` override the theme the same way as Input.

        // Apply size-based padding if not specified
        let mut container = builder.container;
        if builder.size.is_inline() {
            container = container.w("auto").h("auto").min_h(0.0).flex_shrink(0.0);
        } else if !builder.size.is_icon() {
            container = container
                .px(builder.size.padding_h())
                .py(builder.size.padding_v());
        }

        let (mut bg, variant_text_color, border) =
            button_chrome_for_variant(builder.variant.as_paint_key(), t);
        let text_color = if builder.variant == ButtonVariant::Link {
            link_caption_color(builder.explicit_text_color, variant_text_color, is_hovered)
        } else {
            builder.explicit_text_color.unwrap_or(variant_text_color)
        };
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

        let overflow_mode = if builder.truncate {
            Some(crate::style::TextOverflow::Ellipsis)
        } else {
            builder.text_overflow
        };

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
            // MorphIcon seeds a Lucide glyph so layout takes the texture path. Do not
            // hoist that glyph into a caption leaf — it strips `rui_morph_icon` and
            // the morph bind can no longer find the instance.
            if tree_holds_morph_icon(&child) {
                if child
                    .attrs
                    .get(crate::core::tree::ATTR_POINTER_EVENTS)
                    .is_none()
                    && child.attrs.contains_key("rui_morph_icon")
                {
                    child = child.pointer_events("none");
                }
                el = el.child(child);
                continue;
            }
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
                let is_lucide_glyph = child.text_style.font_family.as_deref() == Some("lucide");
                let size_px = if is_lucide_glyph {
                    child.text_style.size_resolved()
                } else {
                    builder.text_style.size_px
                };
                // SelectValue 等子节点会自带 `text-overflow: ellipsis`。重建文案叶时必须
                // 保留，否则只剩 nowrap + 父级 clip，看起来像裁切而没有省略号。
                let child_overflow = match child.text_style.overflow {
                    crate::style::TextOverflow::Ellipsis
                    | crate::style::TextOverflow::Hidden
                    | crate::style::TextOverflow::Newline => Some(child.text_style.overflow),
                    crate::style::TextOverflow::Clip => None,
                };
                let leaf_overflow = overflow_mode.or(child_overflow);
                let leaf_shrink = leaf_overflow.is_some();
                let mut text_node = build_text_node(
                    text_content,
                    TextNodeConfig {
                        color,
                        underline: false,
                        underline_offset_px: None,
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
                .flex_shrink(if leaf_shrink { 1.0 } else { 0.0 });
                if child.style.flex_grow > 0.0 {
                    text_node = text_node.flex_grow(child.style.flex_grow);
                }
                // CSS `text-overflow`: overflowing nowrap is start-aligned (ellipsis at
                // inline-end). `text-align: center` on the caption would center a short
                // truncated run in the full button — large left gap. Untruncated buttons
                // still center via flex `justify-center` + shrink-wrapped caption.
                if leaf_shrink
                    && !matches!(leaf_overflow, Some(crate::style::TextOverflow::Newline))
                {
                    text_node = text_node.text_left();
                } else {
                    text_node = text_node.text_center();
                }
                if !is_lucide_glyph {
                    match leaf_overflow {
                        Some(mode) => {
                            text_node = text_node.text_overflow(mode).min_w(0.0);
                        }
                        None => {
                            text_node = text_node.whitespace_nowrap();
                        }
                    }
                    if builder.variant == ButtonVariant::Link {
                        text_node = text_node.pointer_events("none");
                    }
                }
                if is_lucide_glyph {
                    text_node = text_node.underline(false);
                } else {
                    // Inherit `hover:underline` / `underline-offset-*` from the Button root.
                    text_node.text_style.underline = None;
                    text_node.text_style.underline_offset = None;
                }
                if preserve_binding {
                    if let Some(tid) = child.template_id.take() {
                        text_node.template_id = Some(tid);
                    }
                    if let Some(hook) = child.on_unmount.take() {
                        text_node.on_unmount = Some(hook);
                    }
                }
                // Keep child hit policy (e.g. Pagination icon/caption `pointer-events: none`
                // so clicks land on the Button interactive root).
                if let Some(pe) = child.attrs.get(crate::core::tree::ATTR_POINTER_EVENTS) {
                    text_node = text_node.pointer_events(pe.as_str());
                }
                if is_lucide_glyph {
                    if let Some(name) = child.get_attr(crate::utils::ATTR_ICON_NAME) {
                        text_node.set_attr(crate::utils::ATTR_ICON_NAME, name);
                    }
                }
                el = el.child(text_node);
            } else {
                // Structured children (Kbd / KbdGroup / Icon shells, …) attach as-is.
                // Keycap slots keep muted chrome; captions were hoisted via take_packed.
                el = el.child(child);
            }
        }

        let mut on_click = builder.on_click;
        let disabled = builder.disabled;
        let focus_id = builder.id.clone();
        ensure_button_css_transition(&mut el);

        let mut el: Element = interactive(el)
            .focus_id(builder.id)
            .focusable(!disabled)
            .on_hover(move |h| {
                let want = if disabled { false } else { h };
                match hovered.try_peek() {
                    Some(cur) if cur == want => return,
                    None => return,
                    Some(_) => {}
                }
                if hovered.try_set_silent(want) {
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
            .into();
        if el.cursor.is_none() {
            el = el.cursor_kind(if disabled {
                crate::core::tree::CursorKind::NotAllowed
            } else {
                crate::core::tree::CursorKind::Pointer
            });
        }
        let variant_key = builder.variant.as_paint_key();
        let explicit_fg = builder.explicit_text_color;
        let mut el = chrome.apply(el);
        if let Some(off) = caption_underline_offset {
            el.text_style.underline_offset = Some(off);
        }
        if explicit_fg.is_none() {
            el.set_attr(ATTR_SYNC_LABEL_FG, "1");
        }
        if disabled {
            el.set_attr("disabled", "true");
        }
        let _ = ensure_class_list(&mut el, button_variant_color_classes(variant_key));
        if is_focused {
            apply_focus_ring_shadow_color(&mut el, true, false, button_ring_fallback(variant_key));
        }
        el
    }
}

/// Class variants own bg/fg/link underline. Still join ButtonGroup radii and explicit caption fg.
fn paint_button_join_and_link_underline(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    desc: &ButtonHoverPaint,
    is_hovered: bool,
    impact: &mut LayoutStatus,
) {
    if desc.text_color.is_some() {
        let t = hover_t_from_bool(is_hovered, desc.disabled);
        let (_, variant_text_color, _) = button_chrome_for_variant(&desc.variant, t);
        let text_color = if desc.variant == "link" {
            link_caption_color(desc.text_color, variant_text_color, is_hovered)
        } else {
            desc.text_color.unwrap_or(variant_text_color)
        };
        let mut style_hit = false;
        for child in &mut el.children {
            if is_preserved_button_child_slot(child) || child.text.is_none() {
                continue;
            }
            if child.text_style.font_family.as_deref() == Some("lucide") {
                continue;
            }
            let old_tc = child.text_style.color_resolved();
            let mut color = text_color;
            if desc.disabled {
                color[3] = ((color[3] as f32) * 0.5).round() as u8;
            }
            child.text_style.color = Some(color);
            if child.text_style.color_resolved() != old_tc {
                style_hit = true;
            }
        }
        if style_hit {
            *impact = impact.merge(LayoutStatus::StyleChanged);
            push_button_paint_damage(el);
            // Caption fg is not on the Button root `text_style`, so class-variant
            // `before_text` would miss it. Bust idle skip or Breadcrumb stays muted.
            layout_tree.note_collect_skip_hash_bust_if_idle(el.collect_skip_live);
        }
    }
    let before_radii = el.style.radii;
    crate::components::button_group::apply_button_group_join_chrome(el);
    if el.style.radii != before_radii {
        *impact = impact.merge(LayoutStatus::StyleChanged);
        push_button_paint_damage(el);
    }
}

fn paint_button_root_from_hover(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    desc: &ButtonHoverPaint,
    _focus_id: &str,
    is_hovered: bool,
    is_focused: bool,
    impact: &mut LayoutStatus,
) {
    ensure_button_css_transition(el);
    let class_owns_colors = el.class_set.as_ref().is_some_and(ClassSet::has_hover_color);
    if class_owns_colors {
        paint_button_join_and_link_underline(el, layout_tree, desc, is_hovered, impact);
        return;
    }
    let t = hover_t_from_bool(is_hovered, desc.disabled);
    let (mut bg, variant_text_color, border) = button_chrome_for_variant(&desc.variant, t);
    let text_color = if desc.variant == "link" {
        link_caption_color(desc.text_color, variant_text_color, is_hovered)
    } else {
        desc.text_color.unwrap_or(variant_text_color)
    };
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
    let target_bw = if border.is_some() { 1.0_f32 } else { 0.0_f32 };
    let root_color_ok = match border {
        Some(b_color) => color4_near(el.style.border_color, b_color),
        None => true,
    };
    let root_ok =
        color4_near(el.style.bg_color, bg) && el.style.border_width == target_bw && root_color_ok;
    let mut text_children_ok = true;
    for child in &el.children {
        if is_preserved_button_child_slot(child) {
            continue;
        }
        if child.text.is_none() {
            continue;
        }
        if child.text_style.font_family.as_deref() == Some("lucide") {
            continue;
        }
        let mut c = text_color;
        if desc.disabled {
            c[3] = ((c[3] as f32) * 0.5).round() as u8;
        }
        if child.text_style.color_resolved() != c {
            text_children_ok = false;
            break;
        }
    }
    if root_ok && text_children_ok {
        // 组内连体：即便色值已到位，也要重套圆角 / 绘制叠边标记（hover 的 set_uniform 会清掉连体）。
        let before_radii = el.style.radii;
        crate::components::button_group::apply_button_group_join_chrome(el);
        if el.style.radii != before_radii {
            *impact = impact.merge(LayoutStatus::StyleChanged);
            push_button_paint_damage(el);
        }
        return;
    }

    let old_bw = el.style.border_width;
    let old_bg = el.style.bg_color;
    let old_bc = el.style.border_color;

    // Write the target color every frame; `tick_element_transitions` is then inserted back into mid (same caliber as Carousel/Toast).
    el.style.bg_color = bg;
    match border {
        Some(b_color) => {
            el.style.border_color = b_color;
            el.style.set_uniform_border_width(1.0);
        }
        None => {
            el.style.border_width = 0.0;
            el.style.border_width_sides = None;
        }
    }
    // ButtonGroup conjoined: write back the border and then fillet it/draw the overlapping mark.
    crate::components::button_group::apply_button_group_join_chrome(el);
    let layout_hit = el.style.border_width != old_bw;
    let mut style_hit =
        !layout_hit && (el.style.bg_color != old_bg || el.style.border_color != old_bc);
    // 仅边框宽度影响布局；色值变化不必每帧刷 Taffy。
    if layout_hit {
        layout_tree.update_taffy_node_style(el);
    }

    for child in &mut el.children {
        if is_preserved_button_child_slot(child) {
            continue;
        }
        if child.text.is_none() {
            continue;
        }
        if child.text_style.font_family.as_deref() == Some("lucide") {
            continue;
        }
        let old_tc = child.text_style.color_resolved();
        let mut color = text_color;
        if desc.disabled {
            color[3] = ((color[3] as f32) * 0.5).round() as u8;
        }
        child.text_style.color = Some(color);
        if child.text_style.color_resolved() != old_tc {
            style_hit = true;
        }
    }
    if layout_hit {
        // Border chrome still updates Taffy above, but report StyleChanged so
        // WP-2.2 keeps a local damage rect. LayoutChanged here forced FullClear on every
        // Button hover leave (including Inspector Set/Apply).
        *impact = impact.merge(LayoutStatus::StyleChanged);
        push_button_paint_damage(el);
        layout_tree.note_collect_skip_hash_bust_if_idle(el.collect_skip_live);
    } else if style_hit {
        *impact = impact.merge(LayoutStatus::StyleChanged);
        push_button_paint_damage(el);
        // Color hover leave is paint-only. Without a skip-hash bust, ViewportsOnly
        // `reuse_idle` skip-replays the hovered bake until scroll settles.
        layout_tree.note_collect_skip_hash_bust_if_idle(el.collect_skip_live);
    }
}

/// Local dirty rect for hover/focus chrome (WP-2.2 — avoid full-window Clear).
fn push_button_paint_damage(el: &Element) {
    let b = el.layout_bounds;
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
}

fn sync_button_focus_ring_shadow(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    disabled: bool,
    is_focused: bool,
    variant: &str,
    _focus_id: &str,
    impact: &mut LayoutStatus,
) {
    // Instant focus-visible ring (same frame as Tab). Geometry/color match Input
    // via `apply_focus_ring_shadow_color` (theme + per-element `ring-*` overrides).
    let show = is_focused && !disabled;
    let ring = button_ring_fallback(variant);
    let mut dirty = false;
    if show {
        if !focus_ring_shadow_matches_color(el, true, ring) {
            apply_focus_ring_shadow_color(el, true, false, ring);
            dirty = true;
        }
    } else if focus_ring_shadow_matches_color(el, true, ring) {
        apply_focus_ring_shadow(el, false, true);
        dirty = true;
    }

    // `focus-visible:border-ring` for variants that already have a border (e.g. outline).
    let mut border_dirty = false;
    if el.style.border_width > 0.5 {
        let want_border = if show {
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

    if !dirty && !border_dirty {
        return;
    }
    layout_tree.update_taffy_node_style(el);
    *impact = impact.merge(LayoutStatus::StyleChanged);
    push_button_paint_damage(el);
}

/// Solid-style in-place bind for `<Button variant={sig.get()}>`.
///
/// Updates [`ButtonHoverPaint::variant`] and rest-state chrome without rebuilding the button tree.
/// Unknown names fall back to `default` (WP-3.8 Glossary).
pub fn apply_bound_button_variant(el: &mut Element, variant: impl AsRef<str>) -> LayoutStatus {
    let variant = ButtonVariant::from(variant.as_ref()).as_paint_key();
    {
        let inner = &mut *el.inner;
        let Some(desc) = inner.button_hover_paint.as_mut() else {
            return LayoutStatus::Clean;
        };
        if desc.variant == variant {
            return LayoutStatus::Clean;
        }
        desc.variant = variant.to_string();
    }
    let desc = el.button_hover_paint.as_ref().expect("set above");
    let explicit_fg = desc.text_color;
    let _ = extend_class_list(el, button_variant_color_classes(variant));
    if let Some(fg) = explicit_fg {
        for child in &mut el.children {
            if is_preserved_button_child_slot(child) {
                continue;
            }
            if child.text.is_some() {
                child.text_style.color = Some(fg);
            }
        }
    }
    LayoutStatus::StyleChanged
}

/// Solid-style in-place bind for `<Button disabled={sig.get()}>`.
/// Alias of [`crate::primitives::interactive::apply_disabled`].
pub fn apply_bound_button_disabled(el: &mut Element, disabled: bool) -> LayoutStatus {
    crate::primitives::interactive::apply_disabled(el, disabled)
}

/// Synchronize `{focus_id}_hover` with the global focus ring to the node with [`Element::button_hover_paint`] (without going through the root `create_effect`).
pub(crate) fn sync_button_hover_paint_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
) {
    if crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h) {
        return;
    }
    struct Frame {
        el: *mut Element,
        scroll_acc: [f32; 2],
    }
    let mut stack = vec![Frame {
        el: el as *mut Element,
        scroll_acc,
    }];
    while let Some(frame) = stack.pop() {
        // SAFETY: exclusive paint-sync walk of `el`'s subtree.
        let node = unsafe { &mut *frame.el };
        if crate::core::tree::element_is_offscreen(node, frame.scroll_acc, viewport_w, viewport_h) {
            continue;
        }
        if let Some(desc) = node.button_hover_paint.clone() {
            if let Some(fid) = node.focus_id.clone() {
                let key = format!("{}_hover", fid);
                let hover_sig = use_signal(&key, || false);
                let is_hovered = hover_sig.try_peek().unwrap_or(false) && !desc.disabled;
                let focused = use_signal("__rui_button_focus_id", String::new);
                let focus_visible = use_signal("__rui_focus_visible", || false);
                let is_focused = !desc.disabled
                    && focus_visible.try_peek().unwrap_or(false)
                    && focused.try_peek().as_deref() == Some(fid.as_str());
                paint_button_root_from_hover(
                    node,
                    layout_tree,
                    &desc,
                    &fid,
                    is_hovered,
                    is_focused,
                    impact,
                );
                sync_button_focus_ring_shadow(
                    node,
                    layout_tree,
                    desc.disabled,
                    is_focused,
                    &desc.variant,
                    &fid,
                    impact,
                );
            }
        }
        let child_acc = crate::app::tree_hit::next_hit_scroll_acc(node, frame.scroll_acc);
        let n = node.children.len();
        for i in (0..n).rev() {
            stack.push(Frame {
                el: std::ptr::addr_of_mut!(node.children[i]),
                scroll_acc: child_acc,
            });
        }
    }
}

impl crate::core::tree::IntoElementList for ButtonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::{button, button_chrome_for_variant};
    use crate::core::tree::{Element, LayoutStatus};
    use crate::style::color_mix_over_background;
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
    fn hover_does_not_toggle_border_presence() {
        // 0↔1px border is a Taffy box change. Hover must stay paint-only.
        set_theme_mode(ThemeMode::Light);
        for variant in [
            "default",
            "secondary",
            "outline",
            "ghost",
            "link",
            "destructive",
            "success",
            "warning",
        ] {
            let (_, _, rest) = button_chrome_for_variant(variant, 0.0);
            let (_, _, hover) = button_chrome_for_variant(variant, 1.0);
            assert_eq!(
                rest.is_some(),
                hover.is_some(),
                "{variant}: rest border={} hover border={}",
                rest.is_some(),
                hover.is_some()
            );
        }
    }

    #[test]
    fn secondary_hover_uses_secondary_80() {
        set_theme_mode(ThemeMode::Light);
        let (rest, _, _) = button_chrome_for_variant("secondary", 0.0);
        let (hover, fg, _) = button_chrome_for_variant("secondary", 1.0);
        assert_eq!(fg, parse_text_color("secondary-foreground"));
        assert_eq!(hex(rest), "#F5F5F5");
        assert_eq!(hex(hover), hex(parse_color("secondary/80")));
    }

    #[test]
    fn ghost_rest_keeps_hover_rgb_at_zero_alpha() {
        set_theme_mode(ThemeMode::Light);
        let (rest, rest_fg, _) = button_chrome_for_variant("ghost", 0.0);
        let (hover, hover_fg, _) = button_chrome_for_variant("ghost", 1.0);
        assert!(
            (rest[0] - hover[0]).abs() < 1e-5
                && (rest[1] - hover[1]).abs() < 1e-5
                && (rest[2] - hover[2]).abs() < 1e-5,
            "ghost rest must share hover RGB so CSS alpha-only fade stays clean"
        );
        assert!(rest[3] < 0.01);
        assert!(hover[3] > 0.9);
        assert_eq!(rest_fg, parse_text_color("foreground"));
        assert_eq!(hover_fg, rest_fg, "ghost hover keeps caption color");
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
            .find_map(|c| c.text.as_ref().map(|_| c.text_style.color_resolved()));
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

    #[test]
    fn success_and_warning_soft_tint_match_toast_accents() {
        set_theme_mode(ThemeMode::Light);
        let green = parse_color("success");
        let orange = parse_color("warning");
        let (s_rest, s_fg, _) = button_chrome_for_variant("success", 0.0);
        let (s_hover, _, _) = button_chrome_for_variant("success", 1.0);
        assert_eq!(s_fg, parse_text_color("success"));
        assert_eq!(s_fg, parse_text_color("green-600"));
        assert_eq!(hex(s_rest), hex(color_mix_over_background(green, 0.1)));
        assert_eq!(hex(s_hover), hex(color_mix_over_background(green, 0.2)));
        let (w_rest, w_fg, _) = button_chrome_for_variant("warning", 0.0);
        let (w_hover, _, _) = button_chrome_for_variant("warning", 1.0);
        assert_eq!(w_fg, parse_text_color("warning"));
        assert_eq!(w_fg, parse_text_color("orange-600"));
        assert_eq!(hex(w_rest), hex(color_mix_over_background(orange, 0.1)));
        assert_eq!(hex(w_hover), hex(color_mix_over_background(orange, 0.2)));
        assert_eq!(
            "warn".parse::<super::ButtonVariant>().unwrap(),
            super::ButtonVariant::Warning
        );
        assert_eq!(
            "error".parse::<super::ButtonVariant>().unwrap(),
            super::ButtonVariant::Destructive
        );
    }

    #[test]
    fn inline_size_has_no_padding() {
        use super::button;
        use crate::core::tree::Element;
        use crate::layout::lp_length_px;
        let el: Element = button("t-inline")
            .variant("link")
            .size("inline")
            .text("Home")
            .into();
        assert_eq!(lp_length_px(el.style.padding.left), Some(0.0));
        assert_eq!(lp_length_px(el.style.padding.right), Some(0.0));
    }

    #[test]
    fn link_default_underline_offset_is_4px() {
        let el: Element = button("link-offset").variant("link").text("Go").into();
        assert_eq!(
            el.text_style.underline_offset,
            Some(4.0),
            "shadcn link uses underline-offset-[4px] (not rem-scale 4 = 16px)"
        );
        let caption = el
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("caption");
        assert!(
            caption.text_style.underline.is_none(),
            "caption inherits hover:underline from the Button root"
        );
    }

    #[test]
    fn link_author_underline_offset_class_wins() {
        let el: Element = button("link-offset-author")
            .variant("link")
            .class("underline-offset-[8px]")
            .text("Go")
            .into();
        assert_eq!(el.text_style.underline_offset, Some(8.0));
    }

    #[test]
    fn underline_offset_bare_token_is_rem_scale() {
        let el: Element = button("link-offset-rem")
            .variant("link")
            .class("underline-offset-4")
            .text("Go")
            .into();
        let root = crate::layout::dimension::css_root_font_size_px();
        assert!(
            (el.text_style.underline_offset.unwrap_or(0.0) - root).abs() < 0.01,
            "underline-offset-4 = 1rem = {root}px, got {:?}",
            el.text_style.underline_offset
        );
    }

    #[test]
    fn link_hover_underline_is_text_style() {
        use crate::core::reactivity::create_root;
        use crate::style::{resolve_class_variants, VariantContext};

        let (_h, mut el) =
            create_root(|| -> Element { button("t-link-ul").variant("link").text("Go").into() });
        let attrs = el.attrs.clone();
        let ctx = VariantContext {
            hovered: true,
            attrs: Some(&attrs),
            ..VariantContext::default()
        };
        let _ = resolve_class_variants(&mut el, &ctx);
        assert_eq!(el.text_style.underline, Some(true));
        let caption = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Go"))
            .expect("caption");
        assert_eq!(caption.text_style.underline, Some(true));
        assert_eq!(caption.text_style.underline_offset, Some(4.0));
    }

    #[test]
    fn link_explicit_muted_hovers_to_foreground() {
        use super::link_caption_color;
        let muted = parse_text_color("muted-foreground");
        let fg = parse_text_color("foreground");
        assert_eq!(
            link_caption_color(Some(muted), parse_text_color("primary"), false),
            muted
        );
        assert_eq!(
            link_caption_color(Some(muted), parse_text_color("primary"), true),
            fg
        );
    }

    #[test]
    fn unknown_variant_and_size_fall_back_to_default() {
        assert_eq!(
            "not-a-variant".parse::<super::ButtonVariant>().unwrap(),
            super::ButtonVariant::Default
        );
        assert_eq!(
            "xxl".parse::<super::ButtonSize>().unwrap(),
            super::ButtonSize::Default
        );
        let (unknown, ..) = button_chrome_for_variant("nope", 0.0);
        let (default, ..) = button_chrome_for_variant("default", 0.0);
        assert_eq!(hex(unknown), hex(default));
    }

    #[test]
    fn outline_button_unpacks_ui_vstack_and_paints_caption_foreground() {
        use super::{Button, ButtonProps, ButtonVariant};
        use crate::components::kbd::kbd_group;
        use crate::primitives::r#box::vstack;

        // Mimic ui! multi-child pack: anonymous VStack wrapping caption + KbdGroup.
        let packed = Element::from(vstack().children(vec![
            Element::new().text("Search"),
            kbd_group().kbd("⌘").kbd("K").into(),
        ]));
        let el = Button(ButtonProps {
            variant: Some(ButtonVariant::Outline),
            children: Some(packed),
            ..Default::default()
        });
        let want_fg = parse_text_color("foreground");
        let caption = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Search"))
            .expect("Search caption must be hoisted to the button row");
        assert_eq!(
            caption.text_style.color_resolved(),
            want_fg,
            "outline caption must use foreground, not default white"
        );
        let group = el
            .children
            .iter()
            .find(|c| c.attrs.get("data-slot").map(|s| s.as_str()) == Some("kbd-group"))
            .expect("KbdGroup shell must stay intact");
        assert!(
            group.children.iter().all(|k| {
                k.attrs.get("data-slot").map(|s| s.as_str()) == Some("kbd")
                    && k.children.iter().any(|leaf| {
                        leaf.text.is_some()
                            && leaf.text_style.color == Some(parse_text_color("muted-foreground"))
                    })
            }),
            "Kbd captions must keep muted-foreground"
        );
    }

    #[test]
    fn outline_button_alone_kbd_group_is_not_dismantled() {
        use super::{Button, ButtonProps, ButtonVariant};
        use crate::components::kbd::kbd_group;

        let el = Button(ButtonProps {
            variant: Some(ButtonVariant::Outline),
            children: Some(kbd_group().kbd("⌘").kbd("K").into()),
            ..Default::default()
        });
        assert_eq!(
            el.children.len(),
            1,
            "sole KbdGroup must not be unpacked into raw keycaps"
        );
        assert_eq!(
            el.children[0].attrs.get("data-slot").map(|s| s.as_str()),
            Some("kbd-group")
        );
    }

    #[test]
    fn button_class_hover_applies_primary_80() {
        use crate::core::reactivity::create_root;
        use crate::layout::layout_tree::LayoutTree;
        use crate::style::sync_class_variants_subtree;
        let (_h, mut el) = create_root(|| -> Element {
            button("t-btn-class-hover")
                .variant("default")
                .text("Go")
                .into()
        });
        el.layout_bounds = [0.0, 0.0, 80.0, 32.0];
        for c in &mut el.children {
            if c.text.is_some() {
                c.layout_bounds = [8.0, 6.0, 64.0, 20.0];
            }
        }
        assert_eq!(hex(el.style.bg_color), hex(parse_color("primary")));
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [40.0, 16.0],
            "",
            false,
            800.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            hex(el.style.bg_color),
            hex(parse_color("primary/80")),
            "Button hover color must resolve via class hover:bg-primary/80"
        );
    }

    #[test]
    fn ghost_hover_keeps_foreground() {
        use crate::core::reactivity::create_root;
        use crate::style::{resolve_class_variants, VariantContext};
        let (_h, mut el) = create_root(|| -> Element {
            button("t-btn-ghost-fg").variant("ghost").text("Go").into()
        });
        let rest = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Go"))
            .expect("caption")
            .text_style
            .color;
        assert_eq!(rest, Some(parse_text_color("foreground")));
        let attrs = el.attrs.clone();
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        let hover = el
            .children
            .iter()
            .find(|c| c.text.as_deref() == Some("Go"))
            .expect("caption")
            .text_style
            .color;
        assert_eq!(
            hover, rest,
            "observed ShadCN ghost Button: hover changes bg only, caption stays foreground"
        );
    }

    #[test]
    fn ghost_classes_declare_rest_foreground() {
        let classes = super::button_variant_color_classes("ghost");
        assert!(
            classes.split_whitespace().any(|t| t == "text-foreground"),
            "ghost rest must set text-foreground so Lucide chrome (min/max/close) \
             is not washed to primary-foreground / white on frameless secondaries: {classes}"
        );
    }

    #[test]
    fn ghost_icon_child_keeps_foreground_after_class_resolve() {
        use super::button;
        use crate::core::reactivity::create_root;
        use crate::style::{resolve_class_variants, VariantContext};
        use crate::utils::text_node::build_lucide_icon_text_node;
        let fg = parse_text_color("foreground");
        let lucide_fg = |el: &Element| {
            el.children
                .iter()
                .find(|c| c.text_style.font_family.as_deref() == Some("lucide"))
                .map(|c| c.text_style.color_resolved())
                .expect("lucide child")
        };
        let (_h, mut el) = create_root(|| -> Element {
            button("t-ghost-icon")
                .variant("ghost")
                .size("icon-xs")
                .child(build_lucide_icon_text_node("minus"))
                .into()
        });
        assert_eq!(lucide_fg(&el), fg);
        assert_ne!(lucide_fg(&el), [255, 255, 255, 255]);
        let attrs = el.attrs.clone();
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(
            lucide_fg(&el),
            fg,
            "class restore must not paint ghost icons white"
        );
        assert_eq!(el.text_style.color, Some(fg));
    }

    #[test]
    fn ghost_lucide_title_chrome_emits_textured_draws() {
        use super::button;
        use crate::core::reactivity::create_root;
        use crate::layout::layout_tree::LayoutTree;
        use crate::utils::text_node::build_lucide_icon_text_node;
        let (_h, mut el) = create_root(|| -> Element {
            button("t-ghost-caption-x")
                .variant("ghost")
                .size("icon-xs")
                .child(build_lucide_icon_text_node("x"))
                .into()
        });
        let mut tree = LayoutTree::new();
        let result = tree.build_and_layout(&mut el, 48.0, 32.0, [0.0, 0.0]);
        assert!(
            result
                .textured_draws
                .iter()
                .any(|d| d.texture_key.starts_with("rui_lucide")),
            "frameless secondary chrome icons are Lucide textures; \
             secondary windows must upload/draw textured_draws. keys={:?}",
            result
                .textured_draws
                .iter()
                .map(|d| d.texture_key.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ghost_icon_hover_applies_accent_background() {
        use super::button;
        use crate::core::reactivity::create_root;
        use crate::layout::layout_tree::LayoutTree;
        use crate::style::sync_class_variants_subtree;
        use crate::utils::text_node::build_lucide_icon_text_node;
        set_theme_mode(ThemeMode::Light);
        let (_h, mut el) = create_root(|| -> Element {
            button("t-ghost-caption-hover")
                .variant("ghost")
                .size("icon-xs")
                .child(build_lucide_icon_text_node("x"))
                .into()
        });
        el.layout_bounds = [0.0, 0.0, 28.0, 28.0];
        assert!(
            el.style.bg_color[3] < 0.01,
            "ghost rest background is transparent"
        );
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [14.0, 14.0],
            "",
            false,
            800.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            hex(el.style.bg_color),
            hex(parse_color("accent")),
            "frameless caption ghost buttons resolve hover:bg-accent; \
             secondary windows must sync_class_variants with the pointer"
        );
        sync_class_variants_subtree(
            &mut el,
            [-1.0, -1.0],
            "",
            false,
            800.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover),
            Some(false),
            "pointer leave must drop :hover so transition-colors can fade ghost rest"
        );
    }

    #[test]
    fn author_hover_bg_overrides_button_variant_hover() {
        use crate::core::reactivity::create_root;
        use crate::layout::layout_tree::LayoutTree;
        use crate::style::sync_class_variants_subtree;
        let (_h, mut el) = create_root(|| -> Element {
            button("t-btn-author-hover")
                .variant("default")
                .class("hover:bg-muted")
                .text("Go")
                .into()
        });
        el.layout_bounds = [0.0, 0.0, 80.0, 32.0];
        let mut tree = LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [40.0, 16.0],
            "",
            false,
            800.0,
            600.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            hex(el.style.bg_color),
            hex(parse_color("muted")),
            "author hover:bg must win over variant hover:bg-primary/80"
        );
    }

    #[test]
    fn button_defaults_to_keyboard_focus_ring() {
        use crate::core::reactivity::create_root;
        let (_h, el) = create_root(|| -> Element { button("t-btn-focus-ring").text("Go").into() });
        assert!(el.focusable);
        assert!(el.focus_ring);
    }

    #[test]
    fn disabled_button_is_not_focusable() {
        use crate::core::reactivity::create_root;
        let (_h, el) = create_root(|| -> Element {
            button("t-btn-focus-ring-off")
                .disabled(true)
                .text("Go")
                .into()
        });
        assert!(!el.focusable);
        assert!(!el.focus_ring);
    }

    #[test]
    fn button_class_ring_width_overrides_theme() {
        use crate::core::reactivity::create_root;
        use crate::style::apply_focus_ring_shadow;
        let (_h, mut el) =
            create_root(|| -> Element { button("t-btn-ring-2").class("ring-2").text("Go").into() });
        assert_eq!(el.style.focus_ring_width, Some(2.0));
        apply_focus_ring_shadow(&mut el, true, true);
        assert!(
            (el.style.shadow_spread - 2.0).abs() < f32::EPSILON,
            "Button ring must honor authored ring-2 like Input"
        );
    }

    #[test]
    fn builder_style() {
        use crate::core::reactivity::create_root;
        let (_owner, el): (_, Element) = create_root(|| {
            button("btn-chrome")
                .style(|b| b.text_size(20.0))
                .child(Element::new().text("OK"))
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "button .style(|b| b.text_size(20)) must land on the button"
        );
    }
}
