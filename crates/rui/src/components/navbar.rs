//! # Navbar
//!
//! 顶栏布局壳：左侧 brand / 中间 slots / 右侧 actions。

use crate::core::tree::Element;
use crate::primitives::r#box::hstack;
use crate::tokens;

pub struct NavbarBuilder {
    id: String,
    brand: Option<Element>,
    center: Vec<Element>,
    actions: Vec<Element>,
}

pub fn navbar(id: impl Into<String>) -> NavbarBuilder {
    NavbarBuilder {
        id: id.into(),
        brand: None,
        center: Vec::new(),
        actions: Vec::new(),
    }
}

impl NavbarBuilder {
    pub fn brand(mut self, el: impl Into<Element>) -> Self {
        self.brand = Some(el.into());
        self
    }
    pub fn center_child(mut self, el: impl Into<Element>) -> Self {
        self.center.push(el.into());
        self
    }
    pub fn action(mut self, el: impl Into<Element>) -> Self {
        self.actions.push(el.into());
        self
    }
}

impl From<NavbarBuilder> for Element {
    fn from(builder: NavbarBuilder) -> Self {
        let NavbarBuilder {
            id,
            brand,
            center,
            actions,
        } = builder;
        let mut left = hstack().gap("0.75rem").items_center();
        if let Some(b) = brand {
            left = left.child(b);
        }
        let mut mid = hstack().gap("0.5rem").items_center().flex_grow(1.0);
        for c in center {
            mid = mid.child(c);
        }
        let mut right = hstack().gap("0.5rem").items_center();
        for a in actions {
            right = right.child(a);
        }
        let mut bar: Element = hstack()
            .w("100%")
            .h("3rem")
            .px("1rem")
            .items_center()
            .gap("1rem")
            .child(left)
            .child(mid)
            .child(right)
            .into();
        bar = bar.bg(tokens::background());
        bar.style.border_width = 1.0;
        bar.style.border_color = tokens::border();
        bar.template_id = Some(id);
        bar
    }
}
