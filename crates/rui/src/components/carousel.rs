//! # Carousel
//!
//! 简单轮播：在子节点构建器间 prev / next 切换索引。

use crate::app::reactive::reactive_view;
use crate::components::button::button;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, vstack};
use std::rc::Rc;

pub struct CarouselBuilder {
    id: String,
    slides: Vec<Rc<dyn Fn() -> Element>>,
    index: Option<Signal<usize>>,
}

pub fn carousel(id: impl Into<String>) -> CarouselBuilder {
    CarouselBuilder {
        id: id.into(),
        slides: Vec::new(),
        index: None,
    }
}

impl CarouselBuilder {
    pub fn slide(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.slides.push(Rc::new(build));
        self
    }
    pub fn index_signal(mut self, s: Signal<usize>) -> Self {
        self.index = Some(s);
        self
    }
}

impl From<CarouselBuilder> for Element {
    fn from(builder: CarouselBuilder) -> Self {
        let CarouselBuilder { id, slides, index } = builder;
        let idx = index.unwrap_or_else(|| use_signal(format!("{id}__idx").as_str(), || 0usize));
        let n = slides.len().max(1);
        let view_id = format!("{id}__carousel");
        reactive_view(view_id, move || {
            let i = idx.get() % n;
            let slide = slides.get(i).map(|f| f()).unwrap_or_else(Element::new);
            let prev_i = idx;
            let next_i = idx;
            let prev: Element = button(format!("{id}:prev"))
                .variant("outline")
                .size("sm")
                .on_click(move || {
                    let cur = prev_i.peek();
                    prev_i.set(if cur == 0 { n - 1 } else { cur - 1 });
                })
                .text("‹")
                .into();
            let next: Element = button(format!("{id}:next"))
                .variant("outline")
                .size("sm")
                .on_click(move || next_i.set((next_i.peek() + 1) % n))
                .text("›")
                .into();
            let controls: Element = hstack()
                .gap("0.5rem")
                .items_center()
                .child(prev)
                .child(next)
                .into();
            vstack()
                .gap("0.5rem")
                .w("100%")
                .child(slide)
                .child(controls)
                .into()
        })
    }
}
