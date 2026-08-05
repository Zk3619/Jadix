//! # Resizable
//!
//! 水平两栏可拖拽分割；左宽由 `Signal<f32>` 控制（MVP：点击 handle 步进调整）。

use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box};
use crate::tokens;

pub struct ResizableBuilder {
    id: String,
    left_width: Option<Signal<f32>>,
    default_left: f32,
    min_left: f32,
    max_left: f32,
    left: Option<Box<dyn Fn() -> Element>>,
    right: Option<Box<dyn Fn() -> Element>>,
    height: f32,
}

pub fn resizable(id: impl Into<String>) -> ResizableBuilder {
    ResizableBuilder {
        id: id.into(),
        left_width: None,
        default_left: 200.0,
        min_left: 80.0,
        max_left: 600.0,
        left: None,
        right: None,
        height: 240.0,
    }
}

impl ResizableBuilder {
    pub fn left_width_signal(mut self, s: Signal<f32>) -> Self {
        self.left_width = Some(s);
        self
    }

    pub fn default_left(mut self, w: f32) -> Self {
        self.default_left = w;
        self
    }

    pub fn min_left(mut self, w: f32) -> Self {
        self.min_left = w;
        self
    }

    pub fn max_left(mut self, w: f32) -> Self {
        self.max_left = w;
        self
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn left(mut self, f: impl Fn() -> Element + 'static) -> Self {
        self.left = Some(Box::new(f));
        self
    }

    pub fn right(mut self, f: impl Fn() -> Element + 'static) -> Self {
        self.right = Some(Box::new(f));
        self
    }
}

impl From<ResizableBuilder> for Element {
    fn from(builder: ResizableBuilder) -> Self {
        let ResizableBuilder {
            id,
            left_width,
            default_left,
            min_left,
            max_left,
            left,
            right,
            height,
        } = builder;
        let width_key = format!("{id}__resize_w");
        let width =
            left_width.unwrap_or_else(|| use_signal(width_key.as_str(), move || default_left));
        let view_id = format!("{id}__resize_view");

        reactive_view(view_id, move || {
            let w = width.get().clamp(min_left, max_left);
            let left_el = left
                .as_ref()
                .map(|f| f())
                .unwrap_or_else(|| r#box().w("100%").h("100%").bg(tokens::muted()).into());
            let right_el = right
                .as_ref()
                .map(|f| f())
                .unwrap_or_else(|| r#box().w("100%").h("100%").bg(tokens::background()).into());

            let left_pane: Element = r#box()
                .w(w)
                .h(height)
                .overflow_x("hidden")
                .overflow_y("hidden")
                .child(left_el)
                .into();

            let w3 = width;
            let min3 = min_left;
            let max3 = max_left;
            let handle: Element = interactive(r#box().w(6.0).h(height).bg(tokens::border()).into())
                .on_click(move || {
                    let cur = w3.get();
                    let next = (cur + 40.0).clamp(min3, max3);
                    w3.set(if (next - max3).abs() < 1.0 {
                        min3
                    } else {
                        next
                    });
                })
                .into();

            let right_pane: Element = r#box()
                .flex_grow(1.0)
                .h(height)
                .overflow_x("hidden")
                .overflow_y("hidden")
                .child(right_el)
                .into();

            hstack()
                .w("100%")
                .h(height)
                .child(left_pane)
                .child(handle)
                .child(right_pane)
                .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizable_builds() {
        let _el: Element = resizable("rz1")
            .default_left(160.0)
            .left(|| {
                let t: Element = crate::primitives::text::text("L").into();
                r#box().child(t).into()
            })
            .right(|| {
                let t: Element = crate::primitives::text::text("R").into();
                r#box().child(t).into()
            })
            .into();
    }
}
