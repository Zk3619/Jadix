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

use crate::core::reactivity::Signal;
use crate::core::tree::{Element, parse_color, parse_text_color};
use crate::primitives::r#box::hstack;
use crate::primitives::interactive::interactive;
use crate::utils::resolve_instance_id;
use crate::utils::text_node::{TextNodeConfig, build_text_node};
use crate::utils::toggle_state::{SLOT_TOGGLE, bool_on_click, resolve_bool_signal};
use rui_macros::component;

pub struct ToggleBuilder {
    id: String,
    pressed: Option<Signal<bool>>,
    default_pressed: bool,
    disabled: bool,
    variant: String,
    size: String,
    label: String,
    children: Vec<Element>,
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
        variant: "default".into(),
        size: "default".into(),
        label: String::new(),
        children: Vec::new(),
    }
}

impl ToggleBuilder {
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
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = v.into();
        self
    }
    pub fn size(mut self, s: impl Into<String>) -> Self {
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

fn size_pad(size: &str) -> (&'static str, &'static str, f32) {
    match size {
        "sm" => ("0.5rem", "0.25rem", 12.0),
        "lg" => ("1rem", "0.5rem", 16.0),
        _ => ("0.75rem", "0.375rem", 14.0),
    }
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
        let outline = builder.variant == "outline";

        let (px, py, text_px) = size_pad(&builder.size);
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
        if disabled {
            out = out.cursor_not_allowed();
        } else {
            out = out.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        out.template_id = Some(instance);
        out
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
        let pressed = use_signal("__rui_test_toggle_ctrl", || true);
        let el: Element = toggle("t-ctrl").pressed_signal(pressed).into();
        assert_eq!(el.template_id.as_deref(), Some("t-ctrl"));
        assert_eq!(el.focus_id.as_deref(), Some("t-ctrl"));
        assert!(pressed.peek());
    }
}
