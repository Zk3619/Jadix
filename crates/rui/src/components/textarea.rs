//! # Textarea
//!
//! 多行文本输入：复用 Input 信号键族；`focus_id` 为 `textarea:{id}`；Enter 插入换行。

use crate::components::input::input_value_signal_key;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::r#box;
use crate::primitives::text::text;
use crate::tokens;
use rui_macros::component;

/// `focus_id` 前缀；与单行 `input:` 区分，供 input_controller 识别多行。
pub const TEXTAREA_FOCUS_PREFIX: &str = "textarea:";

pub fn textarea_focus_id(id: &str) -> String {
    format!("{TEXTAREA_FOCUS_PREFIX}{id}")
}

/// Signal key reuse: value/caret/selection share Input's key helpers by id.
pub fn textarea(id: &str) -> TextareaBuilder {
    TextareaBuilder {
        id: id.to_string(),
        placeholder: String::new(),
        rows: 4,
        disabled: false,
        invalid: bool::default(),
    }
}

pub struct TextareaBuilder {
    id: String,
    placeholder: String,
    rows: u32,
    disabled: bool,
    invalid: bool,
}

impl TextareaBuilder {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    pub fn rows(mut self, n: u32) -> Self {
        self.rows = n.max(1);
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn invalid(mut self, v: bool) -> Self {
        self.invalid = v;
        self
    }
}

impl From<TextareaBuilder> for Element {
    fn from(b: TextareaBuilder) -> Self {
        let TextareaBuilder {
            id,
            placeholder,
            rows,
            disabled,
            invalid,
        } = b;
        let vk = input_value_signal_key(&id);
        let value = use_signal(&vk, String::new).get();
        let display = if value.is_empty() && !placeholder.is_empty() {
            placeholder
        } else {
            value
        };
        let min_h = rows as f32 * 20.0 + 16.0;
        let border = if invalid {
            [0.8, 0.2, 0.2, 1.0]
        } else {
            tokens::border()
        };
        let fid = textarea_focus_id(&id);
        let focus_id_for_click = fid.clone();
        let mut box_el: Element = r#box()
            .w("100%")
            .min_h(min_h)
            .p("0.5rem")
            .rounded("0.375rem")
            .bg(tokens::input())
            .border_style("solid")
            .border_width(1.0)
            .border_color(border)
            .child(text(display).size(14.0))
            .into();
        box_el = box_el.focus_id(fid).focusable(true);
        if disabled {
            box_el
        } else {
            interactive(box_el)
                .on_click(move || {
                    use_signal("__rui_button_focus_id", String::new)
                        .set(focus_id_for_click.clone());
                })
                .into()
        }
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Textarea(
    id: String,
    placeholder: String,
    rows: Option<u32>,
    disabled: bool,
    invalid: bool,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = textarea(&id)
        .placeholder(placeholder)
        .disabled(disabled)
        .invalid(invalid);
    if let Some(r) = rows {
        b = b.rows(r);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::input::{input_caret_signal_key, input_selection_signal_key};

    #[test]
    fn textarea_focus_prefix() {
        assert_eq!(textarea_focus_id("bio"), "textarea:bio");
        let _el: Element = textarea("bio").rows(3).placeholder("Say hi").into();
        let _ = input_caret_signal_key("bio");
        let _ = input_selection_signal_key("bio");
    }
}
