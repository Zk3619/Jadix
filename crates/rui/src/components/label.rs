use crate::primitives::text::TextBuilder;
use crate::core::tree::{Element, IntoDimension, IntoF32, IntoLengthPercentage};
use crate::components::text_node::apply_label_defaults;
use rui_macros::component;

pub struct LabelBuilder {
    pub(crate) id: String,
    pub(crate) inner: TextBuilder,
}

/// Label / Text component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Label(
    id: String,
    text: String,
    size: Option<f32>,
    color: Option<[u8; 4]>,
    weight: Option<f32>,
    center: bool,
    children: Option<Element>,
) -> Element {
    let mut builder = label(&id).text(text);
    if let Some(s) = size { builder = builder.size(s); }
    if let Some(c) = color { builder = builder.color(c); }
    if let Some(w) = weight { builder = builder.font_weight(w); }
    if center { builder = builder.text_center(); }
    if let Some(c) = children { builder = builder.child(c); }
    builder.into()
}

pub fn label(id: &str) -> LabelBuilder {
    LabelBuilder {
        id: id.to_string(),
        inner: TextBuilder {
            el: apply_label_defaults(Element::new()),
        },
    }
}

// Legacy alias for existing <Text> tags.
pub fn text_node(id: &str) -> LabelBuilder {
    label(id)
}

impl LabelBuilder {
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.inner = self.inner.text(content);
        self
    }

    pub fn size(mut self, size: impl IntoF32) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    pub fn text_size(mut self, size: impl IntoF32) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    pub fn font_weight(mut self, weight: impl IntoF32) -> Self {
        self.inner = self.inner.weight(weight);
        self
    }

    pub fn color(mut self, color: [u8; 4]) -> Self {
        self.inner = self.inner.color(color);
        self
    }

    pub fn text_color(mut self, color: [u8; 4]) -> Self {
        self.inner = self.inner.color(color);
        self
    }

    pub fn text_center(mut self) -> Self {
        self.inner = self.inner.align_center();
        self
    }

    pub fn line_height(mut self, line_height: impl IntoF32) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }

    pub fn leading(mut self, scale: impl IntoF32) -> Self {
        self.inner = self.inner.leading(scale);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.inner = self.inner.underline(underline);
        self
    }

    // Pass-through some layout/base methods from Element
    pub fn w(mut self, w: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.w(w);
        self
    }
    pub fn h(mut self, h: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.h(h);
        self
    }
    pub fn bg(mut self, bg: [f32; 4]) -> Self {
        self.inner.el = self.inner.el.bg(bg);
        self
    }
    pub fn p(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.p(p);
        self
    }
    pub fn px(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.px(p);
        self
    }
    pub fn py(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.py(p);
        self
    }
    pub fn pt(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.pt(p);
        self
    }
    pub fn pr(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.pr(p);
        self
    }
    pub fn pb(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.pb(p);
        self
    }
    pub fn pl(mut self, p: impl IntoLengthPercentage) -> Self {
        self.inner.el = self.inner.el.pl(p);
        self
    }
    pub fn m(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.m(m);
        self
    }
    pub fn mx(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.mx(m);
        self
    }
    pub fn my(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.my(m);
        self
    }
    pub fn mt(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.mt(m);
        self
    }
    pub fn mr(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.mr(m);
        self
    }
    pub fn mb(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.mb(m);
        self
    }
    pub fn ml(mut self, m: impl IntoDimension) -> Self {
        self.inner.el = self.inner.el.ml(m);
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.inner = self.inner.child(child);
        self
    }
    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
    pub fn text_decoration(mut self, decoration: impl Into<String>) -> Self {
        self.inner.el = self.inner.el.text_decoration(decoration);
        self
    }
    pub fn text_decoration_style(mut self, style: impl Into<String>) -> Self {
        self.inner.el = self.inner.el.text_decoration_style(style);
        self
    }
    pub fn text_decoration_color(mut self, color: [u8; 4]) -> Self {
        self.inner.el = self.inner.el.text_decoration_color(color);
        self
    }
    pub fn text_decoration_thickness(mut self, thickness: impl IntoF32) -> Self {
        self.inner.el = self.inner.el.text_decoration_thickness(thickness);
        self
    }
    pub fn text_underline_offset(mut self, offset: impl IntoF32) -> Self {
        self.inner.el = self.inner.el.text_underline_offset(offset);
        self
    }

    pub fn text_line_height(mut self, line_height: impl IntoF32) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }

    pub fn text_leading(mut self, scale: impl IntoF32) -> Self {
        self.inner = self.inner.leading(scale);
        self
    }

    pub fn text_leading_none(mut self) -> Self {
        self.inner = self.inner.leading(1.0);
        self
    }

    pub fn text_leading_tight(mut self) -> Self {
        self.inner = self.inner.leading(1.25);
        self
    }

    pub fn text_leading_normal(mut self) -> Self {
        self.inner = self.inner.leading(1.5);
        self
    }

    pub fn text_leading_relaxed(mut self) -> Self {
        self.inner = self.inner.leading(1.625);
        self
    }
}

impl From<LabelBuilder> for Element {
    fn from(builder: LabelBuilder) -> Element {
        let mut el: Element = builder.inner.into();
        el.template_id = Some(builder.id);
        el
    }
}

impl crate::core::tree::IntoElementList for LabelBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}
