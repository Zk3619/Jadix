//! # Textarea
//!
//! 多行文本输入的 **样式壳**：编辑能力在 [`crate::utils::text_field`]，
//! 共享 chrome 构建/绘制在 [`crate::text_edit`]（crate 私有，非产品组件）。
//! `focus_id` 为 `textarea:{id}`；Enter 插入换行（见 `input_controller`）。
//! 内容溢出时在内部滚动视口滚动；默认右下角可拖拽改高。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::components::input::InputType;
use crate::text_edit::{build_text_edit_element, TextEditBuildParams};
use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use rui_macros::component;

pub use crate::utils::text_field::{textarea_focus_id, TEXTAREA_FOCUS_PREFIX};

const DEFAULT_TEXT_SIZE_PX: f32 = 0.875 * 16.0;
const DEFAULT_FONT_WEIGHT: u16 = 400;
const DEFAULT_RESIZE_MAX_H: f32 = 480.0;

/// Signal key reuse: value/caret/selection share Input's key helpers by id.
pub fn textarea(id: &str) -> TextareaBuilder {
    TextareaBuilder {
        id: id.to_string(),
        placeholder: String::new(),
        rows: 4,
        disabled: false,
        invalid: bool::default(),
        value: None,
        resizable: true,
        resize_max_h: None,
    }
}

pub struct TextareaBuilder {
    id: String,
    placeholder: String,
    rows: u32,
    disabled: bool,
    invalid: bool,
    value: Option<Signal<String>>,
    resizable: bool,
    resize_max_h: Option<f32>,
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

    /// 右下角拖拽三角改高；默认开启。
    pub fn resizable(mut self, yes: bool) -> Self {
        self.resizable = yes;
        self
    }

    pub fn resize_max_h(mut self, px: f32) -> Self {
        self.resize_max_h = Some(px.max(32.0));
        self
    }

    /// 受控值：见 [`crate::utils::text_field::bind_controlled_value`]。
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
}

impl From<TextareaBuilder> for Element {
    fn from(b: TextareaBuilder) -> Element {
        let TextareaBuilder {
            id,
            placeholder,
            rows,
            disabled,
            invalid,
            value,
            resizable,
            resize_max_h,
        } = b;

        let field_id = if id.is_empty() {
            static RT_EMPTY_TEXTAREA_ID: AtomicU64 = AtomicU64::new(0);
            format!(
                "__rui_textarea_rt_{}",
                RT_EMPTY_TEXTAREA_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            id
        };

        let resize_max_h = resize_max_h.unwrap_or(DEFAULT_RESIZE_MAX_H);

        build_text_edit_element(TextEditBuildParams {
            field_id,
            placeholder,
            disabled,
            invalid,
            input_type: InputType::Text,
            clearable: false,
            password_toggle: false,
            text_size_px: DEFAULT_TEXT_SIZE_PX,
            font_weight: DEFAULT_FONT_WEIGHT,
            line_height_px: None,
            leading_scale: None,
            value,
            single_line_container: None,
            multiline: true,
            rows,
            resizable,
            resize_max_h,
        })
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
    resizable: Option<bool>,
    resize_max_h: Option<f32>,
    value: Option<Signal<String>>,
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
    if let Some(r) = resizable {
        b = b.resizable(r);
    }
    if let Some(h) = resize_max_h {
        b = b.resize_max_h(h);
    }
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::input::{input_caret_signal_key, input_value_signal_key};
    use crate::core::reactivity::use_signal;
    use crate::utils::text_field::{textarea_resize_focus_id, TEXTAREA_RESIZE_FOCUS_PREFIX};

    fn find_focus(el: &Element, want: &str) -> bool {
        if el.focus_id.as_deref() == Some(want) {
            return true;
        }
        el.children.iter().any(|c| find_focus(c, want))
    }

    #[test]
    fn textarea_focus_prefix() {
        assert_eq!(textarea_focus_id("bio"), "textarea:bio");
        let el: Element = textarea("bio").rows(3).placeholder("Say hi").into();
        assert_eq!(el.focus_id.as_deref(), Some("textarea:bio"));
    }

    #[test]
    fn textarea_default_resize_grip() {
        let el: Element = textarea("bio").rows(3).into();
        let grip = textarea_resize_focus_id("bio");
        assert!(grip.starts_with(TEXTAREA_RESIZE_FOCUS_PREFIX));
        assert!(find_focus(&el, &grip), "expected resize grip focus_id");
    }

    #[test]
    fn textarea_resizable_false_omits_grip() {
        let el: Element = textarea("bio").resizable(false).into();
        assert!(!find_focus(&el, &textarea_resize_focus_id("bio")));
    }

    #[test]
    fn textarea_value_signal_keys() {
        let _ = use_signal(&input_value_signal_key("t1"), String::new);
        let _ = use_signal(&input_caret_signal_key("t1"), || None::<usize>);
        let el: Element = textarea("t1").into();
        assert_eq!(el.focus_id.as_deref(), Some("textarea:t1"));
    }
}
