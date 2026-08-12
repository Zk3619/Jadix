//! # Progress
//!
//! 进度条：`value` ∈ `0..=1`。
//!
//! **身份（WP-2.8）**：`id` 可选；空串 → [`resolve_instance_id`](crate::utils::resolve_instance_id)
//!（非受控信号 / reactive 槽需要非空键）。受控时仍可不传用户 id。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;
use crate::utils::resolve_instance_id;
use rui_macros::component;

pub struct ProgressBuilder {
    id: String,
    value: Option<Signal<f32>>,
    default_value: f32,
}

/// RSX `<Progress value={…} />` — `id` 可省略（Default 空串 → 自动实例键）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Progress(id: String, value: Option<f32>, children: Option<Element>) -> Element {
    let _ = children;
    progress()
        .id(id)
        .default_value(value.unwrap_or(0.0))
        .into()
}

/// Builder 入口：默认无用户 id（WP-2.8）。
pub fn progress() -> ProgressBuilder {
    ProgressBuilder {
        id: String::new(),
        value: None,
        default_value: 0.0,
    }
}

impl ProgressBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
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
        let instance = resolve_instance_id(id, "progress");
        let val = value.unwrap_or_else(|| {
            use_signal(
                format!("{instance}__progress").as_str(),
                move || default_value,
            )
        });
        let view_id = format!("{instance}__progress_view");
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
            track.template_id = Some(instance.clone());
            track
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_empty_id_never_writes_empty_template() {
        let el: Element = progress().default_value(0.25).into();
        let tid = el.template_id.as_deref().unwrap_or("");
        // reactive_view placeholder may carry the view id; must not be "".
        assert_ne!(tid, "");
        assert!(!tid.is_empty() || el.template_id.is_none());
    }

    #[test]
    fn progress_empty_id_resolves_nonempty_instance_attr() {
        // Mount via reactive path: placeholder from reactive_view uses view_id.
        let el: Element = progress().into();
        assert_ne!(el.template_id.as_deref(), Some(""));
    }
}
