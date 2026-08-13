//! # Stepper
//!
//! 编号步骤条：高亮当前步。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::tokens;
use crate::utils::text_node::{build_text_node, TextNodeConfig};

pub struct StepperBuilder {
    id: String,
    steps: Vec<String>,
    current: Option<Signal<usize>>,
    default_step: usize,
}

pub fn stepper(id: impl Into<String>) -> StepperBuilder {
    StepperBuilder {
        id: id.into(),
        steps: Vec::new(),
        current: None,
        default_step: 0,
    }
}

impl StepperBuilder {
    pub fn steps(mut self, steps: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.steps = steps.into_iter().map(|s| s.into()).collect();
        self
    }
    pub fn current_signal(mut self, s: Signal<usize>) -> Self {
        self.current = Some(s);
        self
    }
    pub fn default_step(mut self, i: usize) -> Self {
        self.default_step = i;
        self
    }
}

impl From<StepperBuilder> for Element {
    fn from(builder: StepperBuilder) -> Self {
        let StepperBuilder {
            id,
            steps,
            current,
            default_step,
        } = builder;
        let cur = current
            .unwrap_or_else(|| use_signal(format!("{id}__step").as_str(), move || default_step));
        let active = cur.get();
        let mut row: Element = hstack().w("100%").gap("0.75rem").items_center().into();
        for (i, label) in steps.iter().enumerate() {
            let done = i < active;
            let here = i == active;
            let mut badge: Element = r#box().w("1.5rem").h("1.5rem").rounded("9999px").into();
            badge = badge.bg(if done || here {
                tokens::primary()
            } else {
                tokens::muted()
            });
            badge = badge.child(build_text_node(
                format!("{}", i + 1),
                TextNodeConfig {
                    size_px: 12.0,
                    weight: 600,
                    ..Default::default()
                },
            ));
            let title = build_text_node(
                label,
                TextNodeConfig {
                    size_px: 13.0,
                    weight: if here { 600 } else { 400 },
                    ..Default::default()
                },
            );
            let cell: Element = vstack()
                .gap("0.25rem")
                .items_center()
                .child(badge)
                .child(title)
                .into();
            row = row.child(cell);
            if i + 1 < steps.len() {
                let mut line: Element = r#box().h("2px").flex_grow(1.0).into();
                line = line.bg(if done {
                    tokens::primary()
                } else {
                    tokens::border()
                });
                row = row.child(line);
            }
        }
        row.template_id = Some(id);
        row
    }
}
