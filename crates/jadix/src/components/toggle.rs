//! # Toggle
//!
//! 对齐 solid-ui / Kobalte **ToggleButton**：两态按下按钮（pressed on/off），
//! 不是 Switch 轨道。变体 `default` / `outline`；尺寸 `sm` / `default` / `lg`。
//!
//! ## Usage
//! ```ignore
//! <Toggle variant="outline">"B"</Toggle>
//! { Element::from(toggle("italic").pressed_signal(on).variant("outline").text("I")) }
//! ```
//!
//! ## Notes
//! Space/Enter 经焦点路径触发 `on_click` 切换；受控用 `pressed` / `pressed_signal`。
//! **身份（WP-2.8 / 3.7）**：`id` 可空 → [`resolve_instance_id`]；布尔状态走
//! [`crate::utils::toggle_state::resolve_bool_signal`]（受控 XOR Owner 作用域非受控）。

use crate::components::base::SlotChrome;
use crate::core::reactivity::Signal;
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::hstack;
use crate::style::{ensure_class_list, ATTR_SYNC_LABEL_FG};
use crate::utils::resolve_instance_id;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use crate::utils::toggle_state::{bool_on_click, resolve_bool_signal, SLOT_TOGGLE};
use jadix_macros::component;

/// Toggle chrome: ShadCN `default` (borderless) / `outline`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleVariant {
    #[default]
    Default,
    Outline,
}

impl ToggleVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
        }
    }

    pub fn is_outline(self) -> bool {
        matches!(self, Self::Outline)
    }
}

impl From<&str> for ToggleVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ToggleVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&String> for ToggleVariant {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ToggleVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "outline" => Self::Outline,
            _ => Self::Default,
        })
    }
}

/// Toggle size ladder: `sm` / `default` / `lg`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleSize {
    Sm,
    #[default]
    Default,
    Lg,
}

impl ToggleSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Default => "default",
            Self::Lg => "lg",
        }
    }

    pub fn pad(self) -> (&'static str, &'static str, f32) {
        match self {
            Self::Sm => ("0.5rem", "0.25rem", 12.0),
            Self::Lg => ("1rem", "0.5rem", 16.0),
            Self::Default => ("0.75rem", "0.375rem", 14.0),
        }
    }

    pub fn min_px(self) -> f32 {
        match self {
            Self::Sm => 32.0,
            Self::Default => 36.0,
            Self::Lg => 40.0,
        }
    }
}

/// Hover matches observed ShadCN ghost Button: background shifts, caption stays
/// `foreground` (not `muted-foreground` / `accent-foreground`).
/// `hover:` then `data-[state=on]` so pressed accent still wins.
pub(crate) fn toggle_hover_classes(outline: bool, segmented: bool) -> &'static str {
    if segmented {
        "hover:bg-muted hover:text-foreground data-[state=on]:bg-background data-[state=on]:text-foreground"
    } else if outline {
        "hover:bg-accent hover:text-foreground data-[state=on]:bg-accent data-[state=on]:text-accent-foreground"
    } else {
        "hover:bg-muted hover:text-foreground data-[state=on]:bg-accent data-[state=on]:text-accent-foreground"
    }
}

impl From<&str> for ToggleSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ToggleSize {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&String> for ToggleSize {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ToggleSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "lg" | "large" => Self::Lg,
            _ => Self::Default,
        })
    }
}

pub struct ToggleBuilder {
    id: String,
    pressed: Option<Signal<bool>>,
    default_pressed: bool,
    disabled: bool,
    variant: ToggleVariant,
    size: ToggleSize,
    label: String,
    children: Vec<Element>,
    chrome: SlotChrome,
}

/// RSX `<Toggle>` — `id` 可省略。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Toggle(
    id: String,
    pressed: Option<Signal<bool>>,
    default_pressed: bool,
    disabled: bool,
    variant: Option<String>,
    size: Option<String>,
    children: Option<Element>,
) -> Element {
    let mut b = toggle(id)
        .default_pressed(default_pressed)
        .disabled(disabled)
        .variant(variant.unwrap_or_else(|| "default".into()))
        .size(size.unwrap_or_else(|| "default".into()));
    if let Some(sig) = pressed {
        b = b.pressed_signal(sig);
    }
    if let Some(c) = children {
        b = b.child(c);
    }
    b.into()
}

/// `id` 可空（WP-2.8）；空串 → 会话内自动实例键。
pub fn toggle(id: impl Into<String>) -> ToggleBuilder {
    ToggleBuilder {
        id: id.into(),
        pressed: None,
        default_pressed: false,
        disabled: false,
        variant: ToggleVariant::Default,
        size: ToggleSize::Default,
        label: String::new(),
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ToggleBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn pressed_signal(mut self, s: Signal<bool>) -> Self {
        self.pressed = Some(s);
        self
    }
    pub fn default_pressed(mut self, v: bool) -> Self {
        self.default_pressed = v;
        self
    }
    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn variant(mut self, v: impl Into<ToggleVariant>) -> Self {
        self.variant = v.into();
        self
    }
    pub fn size(mut self, s: impl Into<ToggleSize>) -> Self {
        self.size = s.into();
        self
    }
    pub fn text(mut self, t: impl Into<String>) -> Self {
        self.label = t.into();
        self
    }
    pub fn child(mut self, c: impl Into<Element>) -> Self {
        self.children.push(c.into());
        self
    }
}

pub(crate) fn toggle_size_pad(size: ToggleSize) -> (&'static str, &'static str, f32) {
    size.pad()
}

impl From<ToggleBuilder> for Element {
    fn from(builder: ToggleBuilder) -> Self {
        let instance = resolve_instance_id(builder.id, "toggle");
        let sig = resolve_bool_signal(
            builder.pressed,
            &instance,
            SLOT_TOGGLE,
            builder.default_pressed,
        );
        let pressed = sig.peek();
        let disabled = builder.disabled;
        let outline = builder.variant.is_outline();

        let (px, py, text_px) = toggle_size_pad(builder.size);
        let mut bg = if pressed {
            parse_color("accent")
        } else {
            parse_color("transparent")
        };
        let mut border = if outline {
            parse_color("input")
        } else {
            parse_color("transparent")
        };
        let mut fg = if pressed {
            parse_text_color("accent-foreground")
        } else {
            parse_text_color("foreground")
        };
        if disabled {
            bg[3] *= 0.5;
            border[3] *= 0.5;
            fg[3] = (fg[3] as f32 * 0.5) as u8;
        }

        let mut row: Element = hstack()
            .px(px)
            .py(py)
            .rounded("0.375rem")
            .items_center()
            .justify_center()
            .gap("0.25rem")
            .bg(bg)
            .border(border, if outline { 1.0 } else { 0.0 })
            .into();

        if !builder.label.is_empty() {
            row = row.child(build_text_node(
                builder.label,
                TextNodeConfig {
                    size_px: text_px,
                    weight: 500,
                    color: fg,
                    ..Default::default()
                },
            ));
        }
        for c in builder.children {
            row = row.child(c);
        }

        let mut out: Element = interactive(row)
            .focus_id(instance.clone())
            .focusable(!disabled)
            .focus_ring(!disabled)
            .on_click(bool_on_click(sig, disabled, instance.clone()))
            .into();
        out.set_attr("data-slot", "toggle");
        out.set_attr("data-state", if pressed { "on" } else { "off" });
        out.set_attr("data-variant", builder.variant.as_str());
        out.set_attr("data-size", builder.size.as_str());
        out.set_attr(ATTR_SYNC_LABEL_FG, "1");
        let _ = ensure_class_list(&mut out, toggle_hover_classes(outline, false));
        if disabled {
            out = out.cursor_not_allowed().pointer_events_none();
            out.set_attr("disabled", "true");
        } else {
            out = out.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        out.template_id = Some(instance);
        let mut out = builder.chrome.apply(out);
        let _ = ensure_class_list(&mut out, toggle_hover_classes(outline, false));
        let sync_sig = sig;
        out.reactive_sync(move |root, _layout_tree| {
            let _ = ensure_class_list(root, toggle_hover_classes(outline, false));
            // Uncontrolled store is owner-scoped; paint may still run for a frame after dispose.
            let Some(pressed) = sync_sig.try_peek() else {
                return LayoutStatus::Clean;
            };
            let want = if pressed { "on" } else { "off" };
            if root.attrs.get("data-state").map(String::as_str) != Some(want) {
                root.set_attr("data-state", want);
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_scoped_signal, use_signal};
    use crate::utils::toggle_state::bool_slot_key;

    #[test]
    fn toggle_empty_id_never_writes_empty_focus_or_template() {
        let el: Element = toggle("").text("B").into();
        assert_ne!(el.focus_id.as_deref(), Some(""));
        assert_ne!(el.template_id.as_deref(), Some(""));
        assert!(el.focus_id.as_ref().is_some_and(|s| !s.is_empty()));
        assert!(el.template_id.as_ref().is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn toggle_uncontrolled_uses_scoped_signal_per_owner() {
        let key = bool_slot_key("same_local", SLOT_TOGGLE);
        let (h1, a) = create_root(|| {
            let _el: Element = toggle("same_local").default_pressed(false).into();
            let sig = use_scoped_signal(&key, || false);
            sig.set(true);
            sig.peek()
        });
        assert!(a);

        let (h2, b) = create_root(|| {
            let _el: Element = toggle("same_local").default_pressed(false).into();
            let sig = use_scoped_signal(&key, || false);
            // Fresh owner → independent slot (still default false).
            sig.peek()
        });
        assert!(!b);

        h1.dispose();
        h2.dispose();
    }

    #[test]
    fn toggle_controlled_keeps_explicit_id() {
        let pressed = use_signal("__jadix_test_toggle_ctrl", || true);
        let el: Element = toggle("t-ctrl").pressed_signal(pressed).into();
        assert_eq!(el.template_id.as_deref(), Some("t-ctrl"));
        assert_eq!(el.focus_id.as_deref(), Some("t-ctrl"));
        assert!(pressed.peek());
    }

    #[test]
    fn builder_style() {
        let (_owner, el): (_, Element) = create_root(|| {
            toggle("t-chrome")
                .style(|b| b.text_size(20.0))
                .text("B")
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "toggle .style(|b| b.text_size(20)) must land on the toggle"
        );
    }

    #[test]
    fn toggle_hover_keeps_foreground() {
        let mut el: Element = toggle("t-hover").text("B").into();
        assert!(el
            .class_set
            .as_ref()
            .is_some_and(crate::style::ClassSet::has_hover_color));
        let rest_bg = el.style.bg_color;
        let rest_fg = parse_text_color("foreground");
        let attrs = el.attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el,
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(el.style.bg_color, parse_color("muted"));
        assert_ne!(el.style.bg_color, rest_bg);
        let mut caption_fg = None;
        fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
            if el.text.is_some() {
                *acc = el.text_style.color;
            }
            for c in &el.children {
                child_fg(c, acc);
            }
        }
        child_fg(&el, &mut caption_fg);
        assert_eq!(
            caption_fg,
            Some(rest_fg),
            "hover matches observed ShadCN ghost Button: caption stays foreground"
        );
        assert_ne!(
            caption_fg,
            Some(parse_text_color("muted-foreground")),
            "hover must not dim the caption to muted-foreground"
        );
        assert_ne!(
            caption_fg,
            Some(parse_text_color("accent-foreground")),
            "hover must not shift caption to accent-foreground"
        );
    }

    #[test]
    fn toggle_pressed_hover_keeps_accent() {
        let mut el: Element = toggle("t-on").default_pressed(true).text("B").into();
        assert_eq!(el.attrs.get("data-state").map(String::as_str), Some("on"));
        let attrs = el.attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el,
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(el.style.bg_color, parse_color("accent"));
        let mut caption_fg = None;
        fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
            if el.text.is_some() {
                *acc = el.text_style.color;
            }
            for c in &el.children {
                child_fg(c, acc);
            }
        }
        child_fg(&el, &mut caption_fg);
        assert_eq!(
            caption_fg,
            Some(parse_text_color("accent-foreground")),
            "pressed hover must keep data-[state=on] accent-foreground"
        );
    }

    #[test]
    fn toggle_outline_hover_uses_accent() {
        let mut el: Element = toggle("t-out").variant("outline").text("B").into();
        let attrs = el.attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el,
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(el.style.bg_color, parse_color("accent"));
        assert_eq!(
            el.text_style.color,
            Some(parse_text_color("foreground")),
            "outline hover keeps foreground, matching ghost Button"
        );
    }
}
