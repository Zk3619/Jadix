//! # RadioGroup
//!
//! 水平 pill 单选组：选项为 `(value, label)`。外观是胶囊分段，不是圆形 radio。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!     radio_group("tier")
//!         .selected_signal(tier)
//!         .default_value("free")
//!         .options(vec![
//!             ("free".into(), "Free".into()),
//!             ("pro".into(), "Pro".into()),
//!         ]),
//! )
//! ```
//!
//! ## 注意
//! - 受控 `selected` 使用 `get()`（会订阅）。
//! - 每项 `focus_id = "{group_id}:{value}"`，需保证 group `id` 稳定唯一。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::primitives::text::text;
use crate::tokens;
use rui_macros::component;

pub struct RadioGroupBuilder {
    group_id: String,
    selected: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    children: Vec<Element>,
}

/// Horizontal radio group: `(value, label)` pairs; `selected` holds the current value signal.
/// RadioGroup component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn RadioGroup(
    id: String,
    selected: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    children: Option<Element>,
) -> Element {
    let mut builder = radio_group(&id)
        .default_value(default_value)
        .options(options);
    
    if let Some(sig) = selected {
        builder = builder.selected_signal(sig);
    }

    if let Some(c) = children {
        builder = builder.child(c);
    }

    builder.into()
}

pub fn radio_group(group_id: impl Into<String>) -> RadioGroupBuilder {
    RadioGroupBuilder {
        group_id: group_id.into(),
        selected: None,
        default_value: String::new(),
        options: Vec::new(),
        children: Vec::new(),
    }
}

impl RadioGroupBuilder {
    pub fn selected_signal(mut self, s: Signal<String>) -> Self {
        self.selected = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    pub fn options(mut self, pairs: Vec<(String, String)>) -> Self {
        self.options = pairs;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
}

impl From<RadioGroupBuilder> for Element {
    fn from(builder: RadioGroupBuilder) -> Self {
        let scope = format!("{}__radio", builder.group_id);
        let sel = builder
            .selected
            .unwrap_or_else(|| use_signal(scope.as_str(), || builder.default_value.clone()));

        let mut row: Element = hstack().gap("0.5rem").items_center().into();

        for (value, label) in builder.options {
            let v = value.clone();
            let is_on = sel.get() == value;
            let bg = if is_on {
                tokens::primary()
            } else {
                tokens::muted()
            };
            let fg = if is_on {
                tokens::primary_foreground_text()
            } else {
                tokens::foreground_text()
            };
            let pill_label: Element = text(label.as_str())
                .color(fg)
                .size(0.875 * 16.0)
                .into();
            let pill: Element = r#box()
                .px("0.75rem")
                .py("0.375rem")
                .rounded("9999px")
                .bg(bg)
                .child(pill_label)
                .into();
            let sel_c = sel;
            let v_clone = v.clone();
            let opt: Element = interactive(pill)
                .focus_id(format!("{}:{}", builder.group_id, v_clone))
                .focusable(true)
                .on_click(move || sel_c.set(v.clone()))
                .into();
            row = row.child(opt);
        }
        row = row.children(builder.children);

        row.template_id = Some(builder.group_id);
        row
    }
}
