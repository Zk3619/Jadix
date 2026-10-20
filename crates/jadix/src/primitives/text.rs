use crate::core::tree::{Element, IntoF32, IntoNodeFg, IntoNodeText};
use crate::style::TextAlignDeclaration;

pub struct TextBuilder {
    pub(crate) el: Element,
}

impl TextBuilder {
    /// Static text, a Signal, or a live closure. Passing `get()`'s result creates no
    /// leaf binding; the read still subscribes its enclosing effect/memo.
    #[track_caller]
    pub fn new(text: impl IntoNodeText) -> Self {
        Self {
            el: Element::new().text(text),
        }
    }

    pub fn size(mut self, size: impl IntoF32) -> Self {
        self.el = self.el.text_size(size.into_f32());
        self
    }

    pub fn weight(mut self, weight: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.font_weight(weight);
        self
    }

    #[track_caller]
    pub fn color(mut self, color: impl IntoNodeFg) -> Self {
        self.el = color.apply_node_fg(self.el);
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

    pub fn text_align(mut self, align: impl Into<TextAlignDeclaration>) -> Self {
        self.el = self.el.text_align(align);
        self
    }

    pub fn align_center(self) -> Self {
        self.text_align("center")
    }

    pub fn line_height(mut self, px: impl IntoF32) -> Self {
        self.el = self.el.text_line_height(px);
        self
    }

    pub fn leading(mut self, scale: impl crate::core::tree::IntoF32Raw) -> Self {
        self.el = self.el.text_leading(scale);
        self
    }

    /// B17 glyph/cluster motion (`blur` / `split` / `scramble` / `countup` / `dissolve`).
    pub fn glyph_fx(mut self, kind: impl Into<String>) -> Self {
        self.el
            .set_attr(crate::renderer::ATTR_GLYPH_FX, kind.into());
        self
    }

    #[track_caller]
    pub fn text(mut self, text: impl IntoNodeText) -> Self {
        self.el = text.apply_node_text(self.el);
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        let mut child = child.into();
        // If the child is a simple text node, we can just adopt its text
        // and let our own TextStyle govern how it's rendered.
        // Bound leaves (`bind_text` / `bind_prop`) must keep `template_id` + unmount hook —
        // otherwise the effect is disposed on Drop and in-place updates never find this node
        // (gallery: `Selected variant: {selected.get()}` stuck on the initial value).
        if child.children.is_empty() && child.text.is_some() {
            let preserve_binding = child.template_id.is_some() || child.on_unmount.is_some();
            self.el.text = child.text.take();
            if let Some(family) = child.text_style.font_family.clone() {
                self.el = self.el.font_family(family);
            }
            if preserve_binding {
                if let Some(tid) = child.template_id.take() {
                    self.el.template_id = Some(tid);
                }
                if let Some(hook) = child.on_unmount.take() {
                    self.el.on_unmount = Some(hook);
                }
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

/// Text leaf with the same live binding support as [`Element::text`].
#[track_caller]
pub fn text(content: impl IntoNodeText) -> TextBuilder {
    TextBuilder::new(content)
}

pub fn icon(glyph: impl Into<String>) -> TextBuilder {
    let mut b = TextBuilder::new(Into::<String>::into(glyph))
        .family("lucide")
        .color(crate::style::parse_text_color("foreground"));
    b.el = b.el.pointer_events("bounding-box");
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{GlyphFxKind, ATTR_GLYPH_FX};

    #[test]
    fn text_constructor_keeps_owned_and_borrowed_static_inputs() {
        let owned = "caption".to_string();
        let leaves: [Element; 6] = [
            text("caption").into(),
            text(owned.clone()).into(),
            text(&owned).into(),
            text(std::borrow::Cow::Borrowed("caption")).into(),
            text("caption".to_owned().into_boxed_str()).into(),
            text('字').into(),
        ];
        for el in &leaves[..5] {
            assert_eq!(el.text.as_deref(), Some("caption"));
            assert!(el.template_id.is_none());
        }
        assert_eq!(leaves[5].text.as_deref(), Some("字"));
    }

    #[test]
    fn glyph_fx_attr_roundtrip() {
        let el: crate::core::tree::Element = text("BlurText").glyph_fx("split").into();
        assert_eq!(
            el.get_attr(ATTR_GLYPH_FX).map(String::as_str),
            Some("split")
        );
        assert_eq!(GlyphFxKind::parse("split"), GlyphFxKind::Split);
    }
}
