//! # Toggle
//!
//! 对齐 solid-ui / Kobalte **ToggleButton**：两态按下按钮（pressed on/off），
//! 不是 Switch 轨道。变体 `default` / `outline`；尺寸 `sm` / `default` / `lg`。
//!
//! ## 用法
//! ```ignore
//! <Toggle id="bold" variant="outline">"B"</Toggle>
//! { Element::from(toggle("italic").pressed_signal(on).variant("outline").text("I")) }
//! ```
//!
//! ## 注意
//! Space/Enter 经焦点路径触发 `on_click` 切换；受控用 `pressed` / `pressed_signal`。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::hstack;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
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

/// RSX `<Toggle>`。
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
    let mut b = toggle(&id)
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
        let scope = format!("{}__toggle", builder.id);
        let sig = builder
            .pressed
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_pressed));
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

        let sig_click = sig;
        let id = builder.id.clone();
        let mut out: Element = interactive(row)
            .focus_id(id.clone())
            .focusable(!disabled)
            .on_click(move || {
                if disabled {
                    return;
                }
                sig_click.set(!sig_click.get());
            })
            .into();
        if disabled {
            out = out.cursor_not_allowed();
        } else {
            out = out.cursor_kind(crate::core::tree::CursorKind::Pointer);
        }
        out.template_id = Some(builder.id);
        out
    }
}
