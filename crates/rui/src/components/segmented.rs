//! # Segmented
//!
//! 分段控件（单选 ToggleGroup 风格壳）。

use crate::components::toggle_group::toggle_group;
use crate::core::reactivity::Signal;
use crate::core::tree::Element;

pub struct SegmentedBuilder {
    id: String,
    options: Vec<(String, String)>,
    value: Option<Signal<String>>,
    default_value: String,
}

pub fn segmented(id: impl Into<String>) -> SegmentedBuilder {
    SegmentedBuilder {
        id: id.into(),
        options: Vec::new(),
        value: None,
        default_value: String::new(),
    }
}

impl SegmentedBuilder {
    pub fn options(mut self, opts: Vec<(String, String)>) -> Self {
        self.options = opts;
        self
    }
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
}

impl From<SegmentedBuilder> for Element {
    fn from(builder: SegmentedBuilder) -> Self {
        let SegmentedBuilder {
            id,
            options,
            value,
            default_value,
        } = builder;
        let mut b = toggle_group(&id)
            .multiple(false)
            .default_value(default_value)
            .options(options)
            .variant("outline")
            .joined(true);
        if let Some(sig) = value {
            b = b.value_signal(sig);
        }
        let mut el: Element = b.into();
        el.set_attr("rui_segmented", "1");
        el
    }
}
