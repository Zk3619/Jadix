use crate::core::reactivity::use_signal;
use crate::core::tree::{parse_color, parse_text_color, Element, IntoDimension, IntoF32};
use crate::primitives::r#box::{hstack, BoxBuilder};
use crate::primitives::interactive::interactive;
use crate::components::text_node::{build_text_node, TextNodeConfig};
use std::sync::atomic::{AtomicUsize, Ordering};

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
        
        let is_hovered = hovered.get() && !builder.disabled;
        let is_focused = focused.get() == builder.id && focus_visible.get() && !builder.disabled;

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
                if is_hovered { parse_color("primary").with_alpha(0.9) } else { parse_color("primary") },
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
            .on_hover(move |h| hovered.set(if disabled { false } else { h }))
            .on_click(move || {
                if !disabled {
                    focused.set(focus_id.clone());
                    if let Some(ref mut cb) = on_click { cb(); }
                }
            })
            .into()
    }
}

trait ColorExt {
    fn with_alpha(self, alpha: f32) -> [f32; 4];
}
impl ColorExt for [f32; 4] {
    fn with_alpha(mut self, alpha: f32) -> [f32; 4] {
        self[3] = alpha;
        self
    }
}

impl crate::core::tree::IntoElementList for ButtonBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
