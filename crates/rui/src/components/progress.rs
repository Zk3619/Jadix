//! # Progress
//!
//! 进度条：`value` ∈ `0..=1`。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;
use rui_macros::component;

pub struct ProgressBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
}

/// RSX `<Progress id="load" value={…} />`（value 为 0..1 的 f32 Signal 时走 builder）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Progress(id: String, value: Option<f32>, children: Option<Element>) -> Element {
    let _ = children;
    progress(&id)
        .default_value(value.unwrap_or(0.0))
        .into()
}

pub fn progress(id: impl Into<String>) -> ProgressBuilder {
    ProgressBuilder {
        id: id.into(),
        value: None,
        default_value: 0.0,
    }
}

impl ProgressBuilder {
    pub fn value_signal(mut self, s: Signal<f32>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: f32) -> Self {
        self.default_value = v.clamp(0.0, 1.0);
        self
    }
}

impl From<ProgressBuilder> for Element {
    fn from(builder: ProgressBuilder) -> Self {
        let ProgressBuilder {
            id,
            value,
            default_value,
        } = builder;
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__progress").as_str(), move || default_value)
        });
        let view_id = format!("{id}__progress_view");
        crate::app::reactive::reactive_view(view_id, move || {
            let t = val.get().clamp(0.0, 1.0);
            let pct = format!("{:.1}%", t * 100.0);
            let mut track: Element = hstack()
                .w("100%")
                .h("0.5rem")
                .rounded("9999px")
                .overflow("hidden")
                .into();
            track = track.bg(tokens::muted());
            let mut fill: Element = r#box()
                .h("100%")
                .w(format!("{}%", (t * 100.0).clamp(0.0, 100.0)))
                .rounded("9999px")
                .into();
            fill = fill.bg(tokens::primary());
            track = track.child(fill);
            track.set_attr("rui_progress", &pct);
            track.template_id = Some(id.clone());
            track
        })
    }
}
