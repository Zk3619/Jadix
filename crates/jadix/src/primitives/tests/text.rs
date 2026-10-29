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
