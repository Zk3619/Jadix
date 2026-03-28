use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::tokens;
use rui_macros::component;

pub struct SeparatorBuilder {
    vertical: bool,
    children: Vec<Element>,
}

pub fn separator() -> SeparatorBuilder {
    SeparatorBuilder { vertical: false, children: Vec::new() }
}

/// Separator component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Separator(vertical: bool, children: Option<Element>) -> Element {
    let mut builder = separator().vertical(vertical);
    if let Some(c) = children { builder = builder.child(c); }
    builder.into()
}

impl SeparatorBuilder {
    pub fn vertical(mut self, v: bool) -> Self {
        self.vertical = v;
        self
    }

    pub fn child(mut self, c: Element) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<Element>) -> Self {
        self.children.extend(c);
        self
    }
}

impl crate::core::tree::IntoElementList for SeparatorBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<SeparatorBuilder> for Element {
    fn from(builder: SeparatorBuilder) -> Self {
        let mut el: Element = if builder.vertical {
            r#box().w("1px").h("100%").min_h("1rem").into()
        } else {
            r#box().h("1px").w("100%").min_w("1rem").into()
        };
        el = el.bg(tokens::border());
        el
    }
}
