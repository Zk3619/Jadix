use crate::core::tree::{Element, IntoF32};

pub struct TextBuilder {
    pub(crate) el: Element,
}

impl TextBuilder {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            el: Element::new().text(text),
        }
    }

    pub fn size(mut self, size: impl IntoF32) -> Self {
        self.el = self.el.text_size(size.into_f32());
        self
    }

    pub fn weight(mut self, weight: impl IntoF32) -> Self {
        self.el = self.el.font_weight(weight);
        self
    }

    pub fn color(mut self, color: [u8; 4]) -> Self {
        self.el = self.el.text_color(color);
        self
    }

    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.el = self.el.font_family(family);
        self
    }

    pub fn underline(mut self, underline: bool) -> Self {
        self.el = self.el.underline(underline);
        self
    }

    pub fn align_center(mut self) -> Self {
        self.el = self.el.text_center();
        self
    }

    pub fn line_height(mut self, px: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(px);
        self
    }

    pub fn leading(mut self, scale: impl IntoF32) -> Self {
        self.el = self.el.text_leading(scale);
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.el = self.el.text(text);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        let child = child.into();
        // If the child is a simple text node, we can just adopt its text
        // and let our own TextStyle govern how it's rendered.
        if child.children.is_empty() && child.text.is_some() {
            self.el.text = child.text;
            if let Some(family) = child.text_style.font_family.clone() {
                self.el = self.el.font_family(family);
            }
        } else {
            self.el = self.el.child(child);
        }
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        for c in children {
            self = self.child(c);
        }
        self
    }
}

impl From<TextBuilder> for Element {
    fn from(builder: TextBuilder) -> Self {
        builder.el
    }
}

impl crate::core::tree::IntoElementList for TextBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub fn text(content: impl Into<String>) -> TextBuilder {
    TextBuilder::new(content)
}

pub fn icon(glyph: impl Into<String>) -> TextBuilder {
    TextBuilder::new(glyph).family("lucide")
}
