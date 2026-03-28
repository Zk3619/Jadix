use crate::core::reactivity::use_signal;
use crate::core::tree::{
    parse_color, parse_text_color, ButtonHoverPaint, Element, IntoDimension, IntoF32, LayoutStatus,
};
use crate::tokens;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::primitives::interactive::{interactive, mark_hover_paint_dirty};
use crate::components::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;
use std::sync::atomic::{AtomicUsize, Ordering};

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
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
    #[prop(default = "Handler::default()")]
    mut on_click: crate::core::tree::Handler,
    #[prop(default = "KeyHandler::default()")]
    mut on_key_down: crate::core::tree::KeyHandler,
    children: Option<Element>,
) -> Element {
    let mut builder = button(&id)
        .variant(&variant.as_paint_key())
        .disabled(disabled);
    
    builder.size = size;
    builder.container = builder.container.gap(size.gap());
    builder.text_style.size_px = IntoF32::into_f32(size.text_size());
    if size.is_icon() {
        builder.container = builder.container.h(size.height()).w(size.height());
    } else {
        builder.container = builder.container.min_h(size.height());
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
        disabled: false,
        on_click: None,
        container: hstack()
            .rounded(size.corner_radius())
            .gap(size.gap())
            .justify_center()
            .items_center(),
        content: Vec::new(),
        text_style: TextNodeConfig {
            size_px: IntoF32::into_f32(size.text_size()),
            weight: 500,
            color: [255, 255, 255, 255],
            underline: false,
            center: true,
            line_height_px: None,
            leading_scale: None,
            font_family: None,
        },
    }
}

impl ButtonBuilder {
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
                .w(self.size.height());
        } else {
            // Text buttons are content-sized but follow standard min-height.
            self.container = self
                .container
                .min_h(self.size.height())
                .h("auto")
                .w("auto");
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

        // Apply size-based padding if not specified
        let mut container = builder.container;
        if !builder.size.is_icon() {
            container = container
                .px(builder.size.padding_h())
                .py(builder.size.padding_v());
        }

        // Apply variant styling
        let (bg, text_color, border) = match builder.variant {
            ButtonVariant::Default => (
                if is_hovered {
                    tokens::primary().with_alpha(0.9)
                } else {
                    tokens::primary()
                },
                parse_text_color("primary-foreground"),
                None
            ),
            ButtonVariant::Secondary => (
                if is_hovered { parse_color("secondary").with_alpha(0.8) } else { parse_color("secondary") },
                parse_text_color("secondary-foreground"),
                None
            ),
            ButtonVariant::Outline => (
                if is_hovered { parse_color("accent") } else { parse_color("background") },
                if is_hovered { parse_text_color("accent-foreground") } else { parse_text_color("foreground") },
                Some(parse_color("border"))
            ),
            ButtonVariant::Ghost => (
                if is_hovered { parse_color("accent") } else { [0.0; 4] },
                if is_hovered { parse_text_color("accent-foreground") } else { parse_text_color("foreground") },
                None
            ),
            ButtonVariant::Destructive => (
                if is_hovered { parse_color("destructive").with_alpha(0.9) } else { parse_color("destructive") },
                parse_text_color("primary-foreground"),
                None
            ),
            ButtonVariant::Link => (
                [0.0; 4],
                parse_text_color("primary"),
                None
            ),
        };

        let mut bg = bg;
        if builder.disabled {
            bg[3] *= 0.5;
        }
        container = container.bg(bg);
        if let Some(b_color) = border {
            container = container.border(b_color, 1.0);
        }
        if is_focused {
            container = container.shadow(parse_color("ring"), 3.0);
        }

        let mut el: Element = container.into();
        el.template_id = Some(builder.id.clone());
        el.button_hover_paint = Some(ButtonHoverPaint {
            variant: builder.variant.as_paint_key().to_string(),
            disabled: builder.disabled,
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

        interactive(el)
            .focus_id(builder.id)
            .focusable(!disabled)
            .on_hover(move |h| {
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

fn button_chrome_for_variant(variant: &str, is_hovered: bool) -> ([f32; 4], [u8; 4], Option<[f32; 4]>) {
    match variant {
        "default" => (
            if is_hovered {
                parse_color("primary").with_alpha(0.9)
            } else {
                parse_color("primary")
            },
            parse_text_color("primary-foreground"),
            None,
        ),
        "secondary" => (
            if is_hovered {
                parse_color("secondary").with_alpha(0.8)
            } else {
                parse_color("secondary")
            },
            parse_text_color("secondary-foreground"),
            None,
        ),
        "outline" => (
            if is_hovered { parse_color("accent") } else { parse_color("background") },
            if is_hovered {
                parse_text_color("accent-foreground")
            } else {
                parse_text_color("foreground")
            },
            Some(parse_color("border")),
        ),
        "ghost" => (
            if is_hovered { parse_color("accent") } else { [0.0; 4] },
            if is_hovered {
                parse_text_color("accent-foreground")
            } else {
                parse_text_color("foreground")
            },
            None,
        ),
        "destructive" => (
            if is_hovered {
                parse_color("destructive").with_alpha(0.9)
            } else {
                parse_color("destructive")
            },
            parse_text_color("primary-foreground"),
            None,
        ),
        "link" => ([0.0; 4], parse_text_color("primary"), None),
        _ => button_chrome_for_variant("default", is_hovered),
    }
}

fn paint_button_root_from_hover(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    desc: &ButtonHoverPaint,
    is_hovered: bool,
    impact: &mut LayoutStatus,
) {
    let (mut bg, text_color, border) = button_chrome_for_variant(&desc.variant, is_hovered);
    if desc.disabled {
        bg[3] *= 0.5;
    }
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
        if child.text_style.underline != underline || child.text_style.color != c {
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
        child.text_style.color = color;
        child.text_style.underline = underline;
        if prev_u != underline {
            layout_hit = true;
        } else if child.text_style.color != old_tc {
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
    impact: &mut LayoutStatus,
) {
    let want_blur = if is_focused && !disabled { 3.0 } else { 0.0 };
    let want_color = if is_focused && !disabled {
        parse_color("ring")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let blur_ok = (el.style.shadow_blur - want_blur).abs() <= 0.01;
    let color_ok = (el.style.shadow_color[0] - want_color[0]).abs() <= 0.001
        && (el.style.shadow_color[1] - want_color[1]).abs() <= 0.001
        && (el.style.shadow_color[2] - want_color[2]).abs() <= 0.001
        && (el.style.shadow_color[3] - want_color[3]).abs() <= 0.001;
    if blur_ok && color_ok {
        return;
    }
    el.style.shadow_color = want_color;
    el.style.shadow_blur = want_blur;
    layout_tree.update_taffy_node_style(el);
    // Shadow is paint/chrome; `LayoutChanged` here was forcing full scrollbar prepass + Taffy rebuild
    // on every focus/hover frame and fighting auto scrollbar fade.
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
            paint_button_root_from_hover(el, layout_tree, &desc, is_hovered, impact);
            let focused = use_signal("__rui_button_focus_id", String::new);
            let focus_visible = use_signal("__rui_focus_visible", || false);
            let is_focused =
                !desc.disabled && focus_visible.peek() && focused.peek() == fid;
            sync_button_focus_ring_shadow(el, layout_tree, desc.disabled, is_focused, impact);
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
