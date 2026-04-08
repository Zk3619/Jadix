use crate::core::reactivity::use_signal;
use crate::core::tree::{
    parse_color, parse_text_color, ButtonHoverPaint, Element, IntoDimension, IntoF32, LayoutStatus,
};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::components::text_node::{build_text_node, TextNodeConfig};
use crate::style::{
    lerp_display_srgb_linear, lerp_premultiplied_display_srgba, parse_box_shadow, use_ui_transition_cubic,
    use_ui_transition_spring,
};
use rui_macros::component;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Tailwind v4 `shadow-xs`（shadcn 文档站 Button demo：`0 1px 2px 0 rgb(0 0 0 / 0.05)`）。
const SHADOW_XS: &str = "0 1px 2px 0 #0000000d";
/// 引擎坐标下正值会把下划线抬高；这里取负值让 link 下划线向下偏移。
const LINK_UNDERLINE_OFFSET_PX: f32 = -4.0;

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}

/// `shadow-xs` 与 `focus-visible:ring` / `ring-destructive/20` 的合成（单槽 `box-shadow`）。
fn button_shadow_style_for_sync(is_focused: bool, disabled: bool, variant: &str) -> (f32, [f32; 4], [f32; 2], f32) {
    let xs = matches!(variant, "default" | "secondary" | "outline" | "destructive");
    if is_focused && !disabled {
        let c = if variant == "destructive" {
            parse_color("destructive").with_alpha(0.2)
        } else {
            parse_color("ring")
        };
        return (3.0, c, [0.0, 0.0], 0.0);
    }
    if xs && !disabled {
        let s = parse_box_shadow(SHADOW_XS);
        return (s.blur, s.color, s.offset, s.spread);
    }
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

/// `hover_t`：0=静止，1=悬停；由弹簧驱动，对标浏览器 `transition` 的平滑插值（颜色在线性 sRGB 下 lerp）。
fn button_chrome_for_variant(variant: &str, hover_t: f32) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    let t = hover_t.clamp(0.0, 1.0);
    match variant {
        "default" => {
            let rest = parse_color("primary");
            let hover = parse_color("primary").with_alpha(0.9);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("primary-foreground"), None)
        }
        "secondary" => {
            let rest = parse_color("secondary");
            let hover = parse_color("secondary").with_alpha(0.8);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("secondary-foreground"), None)
        }
        "outline" => {
            let rest_bg = parse_color("background");
            let hover_bg = parse_color("accent");
            let bg = lerp_display_srgb_linear(rest_bg, hover_bg, t);
            let tc = lerp_text_rgba(
                parse_text_color("foreground"),
                parse_text_color("accent-foreground"),
                t,
            );
            // Tailwind `border` 类，与 shadcn 文档站 outline 按钮一致（浅色主题常与 `input` 同色）。
            (bg, tc, Some(parse_color("border")))
        }
        "ghost" => {
            let rest_bg = [0.0, 0.0, 0.0, 0.0];
            let hover_bg = parse_color("accent");
            let bg = lerp_premultiplied_display_srgba(rest_bg, hover_bg, t);
            let tc = lerp_text_rgba(
                parse_text_color("foreground"),
                parse_text_color("accent-foreground"),
                t,
            );
            (bg, tc, None)
        }
        "destructive" => {
            let rest = parse_color("destructive");
            let hover = parse_color("destructive").with_alpha(0.9);
            let bg = lerp_display_srgb_linear(rest, hover, t);
            (bg, parse_text_color("white"), None)
        }
        "link" => ([0.0; 4], parse_text_color("primary"), None),
        _ => {
            let rest = parse_color("primary");
            let hover = parse_color("primary").with_alpha(0.9);
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
        Self::from_str(s)
    }
}

impl From<String> for ButtonVariant {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

impl ButtonVariant {
    pub fn from_str(s: &str) -> Self {
        match s {
            "secondary" => Self::Secondary,
            "outline" => Self::Outline,
            "ghost" => Self::Ghost,
            "destructive" => Self::Destructive,
            "link" => Self::Link,
            _ => Self::Default,
        }
    }

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
        Self::from_str(s)
    }
}

impl From<String> for ButtonSize {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

impl ButtonSize {
    pub fn from_str(s: &str) -> Self {
        match s {
            "xs" => Self::Xs,
            "sm" => Self::Sm,
            "lg" => Self::Lg,
            "icon" => Self::Icon,
            "icon-xs" => Self::IconXs,
            "icon-sm" => Self::IconSm,
            "icon-lg" => Self::IconLg,
            _ => Self::Default,
        }
    }

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
        matches!(self, Self::Icon | Self::IconXs | Self::IconSm | Self::IconLg)
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
    #[prop(default = "Handler::default()")]
    mut on_click: crate::core::tree::Handler,
    #[prop(default = "KeyHandler::default()")]
    mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
) -> Element {
    let hover_easing = if hover_easing.is_empty() || (hover_easing != "spring" && hover_easing != "cubic") {
        "spring".to_string()
    } else {
        hover_easing
    };
    let mut builder = button(&id)
        .variant(&variant.as_paint_key())
        .hover_easing(&hover_easing)
        .disabled(disabled);
    
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
            if handled { return true; }
            on_key_down.call(ev)
        });
    }

    el
}

pub fn button(id: &str) -> ButtonBuilder {
    let size = ButtonSize::Default;
    let id = if id.trim().is_empty() { auto_button_id() } else { id.to_string() };
    ButtonBuilder {
        id,
        variant: ButtonVariant::Default,
        size,
        hover_easing: "spring".to_string(),
        disabled: false,
        on_click: None,
        container: hstack()
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
    }
}

impl ButtonBuilder {
    pub fn hover_easing(mut self, e: &str) -> Self {
        self.hover_easing = e.to_string();
        self
    }

    pub fn variant(mut self, v: &str) -> Self {
        self.variant = ButtonVariant::from_str(v);
        self
    }

    pub fn size(mut self, s: &str) -> Self {
        self.size = ButtonSize::from_str(s);
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
        self
    }

    pub fn text_leading_none(self) -> Self { self.leading_none() }
    pub fn text_leading_tight(self) -> Self { self.leading_tight() }
    pub fn text_leading_normal(self) -> Self { self.leading_normal() }
    pub fn text_leading_relaxed(self) -> Self { self.leading_relaxed() }
    pub fn text_line_height(self, px: impl IntoF32) -> Self { self.line_height(px) }

    pub fn on_click(mut self, cb: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(cb));
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
    pub fn w(mut self, w: impl IntoDimension) -> Self {
        self.container = self.container.w(w);
        self
    }
    pub fn h(mut self, h: impl IntoDimension) -> Self {
        self.container = self.container.h(h);
        self
    }
    pub fn p(mut self, p: impl IntoDimension) -> Self {
        self.container = self.container.p(p);
        self
    }
    pub fn gap(mut self, g: impl IntoDimension) -> Self {
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
        let focus_spring = use_ui_transition_spring(
            &btn_focus_spring_key(&builder.id),
            if is_focused { 1.0 } else { 0.0 },
        );
        let focus_t = focus_spring.peek().clamp(0.0, 1.0);

        // Apply size-based padding if not specified
        let mut container = builder.container;
        if !builder.size.is_icon() {
            container = container
                .px(builder.size.padding_h())
                .py(builder.size.padding_v());
        }

        let (mut bg, text_color, border) = button_chrome_for_variant(builder.variant.as_paint_key(), t);
        if builder.disabled {
            bg[3] *= 0.5;
        }
        container = container.bg(bg);
        if let Some(mut b_color) = border {
            if builder.disabled && builder.variant == ButtonVariant::Outline {
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

        let mut el: Element = container.into();
        el.template_id = Some(builder.id.clone());
        el.button_hover_paint = Some(ButtonHoverPaint {
            variant: builder.variant.as_paint_key().to_string(),
            disabled: builder.disabled,
            hover_easing: builder.hover_easing.clone(),
        });
        
        for mut child in builder.content {
            if let Some(text_content) = child.text.take() {
                let mut color = text_color;
                if builder.disabled { color[3] = (color[3] as f32 * 0.5) as u8; }
                let underline = builder.variant == ButtonVariant::Link && is_hovered;
                // `Icon` 等叶子在宏里设置了 `text_size`；若只用 `builder.text_style.size_px`，
                // icon-sm / icon / icon-lg 会全部被压成按钮默认字号（仅 icon-xs 碰巧一致）。
                let is_lucide_glyph = child.text_style.font_family.as_deref() == Some("lucide");
                let size_px = if is_lucide_glyph {
                    child.text_style.size
                } else {
                    builder.text_style.size_px
                };
                let text_node = build_text_node(text_content, TextNodeConfig {
                    color,
                    underline,
                    underline_offset_px: if builder.variant == ButtonVariant::Link {
                        Some(LINK_UNDERLINE_OFFSET_PX)
                    } else {
                        builder.text_style.underline_offset_px
                    },
                    font_family: child.text_style.font_family.clone(),
                    size_px,
                    ..builder.text_style
                });
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
                let v = if disabled { 0.0 } else if h { 1.0 } else { 0.0 };
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
                    if let Some(ref mut cb) = on_click { cb(); }
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
    impact: &mut LayoutStatus,
) {
    let hover_t = hover_t_for_id(focus_id, &desc.hover_easing);
    let t = if desc.disabled { 0.0 } else { hover_t };
    let (mut bg, text_color, border) = button_chrome_for_variant(&desc.variant, t);
    if desc.disabled {
        bg[3] *= 0.5;
    }
    let border = border.map(|mut c| {
        if desc.disabled && desc.variant == "outline" {
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
    let root_ok =
        el.style.bg_color == bg && el.style.border_width == target_bw && root_color_ok;
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
        if child.text_style.underline != underline
            || child.text_style.color != c
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
    let mut style_hit = !layout_hit && (el.style.bg_color != old_bg || el.style.border_color != old_bc);
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
        child.text_style.color = color;
        child.text_style.underline = underline;
        if desc.variant == "link" {
            child.text_style.underline_offset = LINK_UNDERLINE_OFFSET_PX;
        }
        if prev_u != underline {
            layout_hit = true;
        } else if child.text_style.color != old_tc {
            style_hit = true;
        } else if (prev_uo - child.text_style.underline_offset).abs() > 0.01 {
            style_hit = true;
        }
        if child.layout_node.is_some() {
            layout_tree.update_taffy_node_style(child);
        }
    }
    if layout_hit {
        *impact = impact.merge(LayoutStatus::LayoutChanged);
    } else if style_hit {
        *impact = impact.merge(LayoutStatus::StyleChanged);
    }
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
    let focus_spring = use_ui_transition_spring(
        &btn_focus_spring_key(focus_id),
        if is_focused { 1.0 } else { 0.0 },
    );
    focus_spring.set(if is_focused { 1.0 } else { 0.0 });
    let focus_t = focus_spring.peek().clamp(0.0, 1.0);
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
    if blur_ok && color_ok && off_ok && spread_ok && inset_ok {
        return;
    }
    el.style.shadow_color = want_color;
    el.style.shadow_blur = want_blur;
    el.style.shadow_offset = want_off;
    el.style.shadow_spread = want_spread;
    el.style.shadow_inset = want_inset;
    layout_tree.update_taffy_node_style(el);
    *impact = impact.merge(LayoutStatus::StyleChanged);
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
            paint_button_root_from_hover(el, layout_tree, &desc, &fid, is_hovered, impact);
            let focused = use_signal("__rui_button_focus_id", String::new);
            let focus_visible = use_signal("__rui_focus_visible", || false);
            let is_focused =
                !desc.disabled && focus_visible.peek() && focused.peek() == fid;
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
