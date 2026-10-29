use super::{
    parse_text_shadow, TextAlign, TextAlignDeclaration, TextDirection, TextOverflow, TextSpan,
    TextSpanKind, TextStyle, WhiteSpace,
};

#[test]
fn text_span_chip_builder() {
    let s = TextSpan::new("alice")
        .chip()
        .background([40, 40, 50, 255])
        .padding_x(6.0)
        .color([200, 200, 255, 255]);
    assert_eq!(s.kind, TextSpanKind::Chip);
    assert_eq!(s.padding_x, 6.0);
    assert_eq!(s.background, Some([40, 40, 50, 255]));
    assert_eq!(s.color, Some([200, 200, 255, 255]));
    let m = TextSpan::new("@bob").mention();
    assert_eq!(m.kind, TextSpanKind::Mention);
    assert_eq!(m.padding_x, 4.0);
    assert_eq!(m.background, Some([37, 99, 235, 40]));
    assert_eq!(m.color, Some([37, 99, 235, 255]));
    let m_override = TextSpan::new("@bob")
        .mention()
        .padding_x(8.0)
        .background([1, 2, 3, 4])
        .color([5, 6, 7, 8]);
    assert_eq!(m_override.padding_x, 8.0);
    assert_eq!(m_override.background, Some([1, 2, 3, 4]));
    assert_eq!(m_override.color, Some([5, 6, 7, 8]));
    let plain = TextSpan::new("hi");
    assert_eq!(plain.kind, TextSpanKind::Text);
    assert_eq!(plain.padding_x, 0.0);
    assert!(plain.background.is_none());
}

#[test]
fn text_style_overflow_and_selectable_defaults() {
    let s = TextStyle::default();
    assert_eq!(s.overflow, TextOverflow::Clip);
    assert_eq!(s.white_space, WhiteSpace::Normal);
    assert_eq!(s.wrap_mode, crate::text::WrapMode::Word);
    assert!(!s.selectable);
    assert_eq!(s.direction, super::TextDirection::Auto);
    assert_eq!(s.letter_spacing, 0.0);
}

#[test]
fn element_selectable_and_overflow_roundtrip() {
    let el = crate::core::tree::Element::new()
        .text("hi")
        .selectable(true)
        .text_overflow_ellipsis();
    assert!(el.text_style.selectable);
    assert_eq!(el.text_style.overflow, TextOverflow::Ellipsis);
    assert_eq!(el.text_style.white_space, WhiteSpace::Nowrap);
    assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
}

#[test]
fn text_align_parse_mdn_keywords() {
    assert_eq!(TextAlign::parse("start"), TextAlign::Start);
    assert_eq!(TextAlign::parse("end"), TextAlign::End);
    assert_eq!(TextAlign::parse("left"), TextAlign::Left);
    assert_eq!(TextAlign::parse("right"), TextAlign::Right);
    assert_eq!(TextAlign::parse("center"), TextAlign::Center);
    assert_eq!(TextAlign::parse("JUSTIFY"), TextAlign::Justify);
    assert_eq!(TextAlign::parse("justify-all"), TextAlign::JustifyAll);
    assert_eq!(TextAlign::parse("match-parent"), TextAlign::MatchParent);
    assert_eq!(TextAlign::parse("-moz-center"), TextAlign::Center);
    assert_eq!(TextAlign::parse("-webkit-center"), TextAlign::Center);
    assert_eq!(
        TextAlign::parse_declaration("inherit"),
        TextAlignDeclaration::Inherit
    );
    assert_eq!(
        TextAlign::parse_declaration("initial"),
        TextAlignDeclaration::Initial
    );
    assert_eq!(
        TextAlign::parse_declaration("unset"),
        TextAlignDeclaration::Inherit
    );
    assert_eq!(
        TextAlign::parse_declaration("revert"),
        TextAlignDeclaration::Invalid
    );
    assert_eq!(
        TextAlign::parse_declaration("revert-layer"),
        TextAlignDeclaration::Invalid
    );
    assert_eq!(
        TextAlign::Start.to_glyphon(TextDirection::Ltr),
        glyphon::cosmic_text::Align::Left
    );
    assert_eq!(
        TextAlign::Start.to_glyphon(TextDirection::Rtl),
        glyphon::cosmic_text::Align::Right
    );
    assert_eq!(
        TextAlign::Left.to_glyphon(TextDirection::Rtl),
        glyphon::cosmic_text::Align::Left
    );
    assert_eq!(
        TextAlign::End.to_glyphon(TextDirection::Auto),
        glyphon::cosmic_text::Align::End
    );
    assert_eq!(
        TextAlign::Justify.to_glyphon(TextDirection::Auto),
        glyphon::cosmic_text::Align::Justified
    );
    assert_eq!(
        TextAlign::resolve_match_parent(TextAlign::Start, TextDirection::Rtl),
        TextAlign::Right
    );
    assert_eq!(TextStyle::default().align, None);
}

#[test]
fn text_overflow_parse_clip_ellipsis_newline() {
    assert_eq!(TextOverflow::parse("clip"), TextOverflow::Clip);
    assert_eq!(TextOverflow::parse("hidden"), TextOverflow::Hidden);
    assert_eq!(TextOverflow::parse("ellipsis"), TextOverflow::Ellipsis);
    assert_eq!(TextOverflow::parse("truncate"), TextOverflow::Ellipsis);
    assert_eq!(TextOverflow::parse("newline"), TextOverflow::Newline);
    assert_eq!(TextOverflow::parse("wrap"), TextOverflow::Newline);
    assert!(TextOverflow::Clip.nowrap());
    assert!(TextOverflow::Hidden.nowrap());
    assert!(TextOverflow::Ellipsis.nowrap());
    assert!(!TextOverflow::Newline.nowrap());
    assert_eq!(TextOverflow::editable_single_line(), TextOverflow::Hidden);
    assert_eq!(TextOverflow::editable_placeholder(), TextOverflow::Ellipsis);
    assert!(TextOverflow::Hidden.allows_caret_scroll());
    assert!(!TextOverflow::editable_placeholder().allows_caret_scroll());
    assert!(!TextOverflow::Clip.allows_caret_scroll());
    assert!(TextOverflow::Ellipsis.start_align_when_line_overflows());
    assert!(!TextOverflow::Newline.start_align_when_line_overflows());
}

fn approx_eq(a: f32, b: f32) {
    assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
}

#[test]
fn text_shadow_unsuffixed_defaults_to_rem() {
    let (_, offset, blur) = parse_text_shadow("1 0.5 0.25");
    approx_eq(offset[0], 16.0);
    approx_eq(offset[1], 8.0);
    approx_eq(blur, 4.0);
}

#[test]
fn text_shadow_supports_px_rem_em_conversion() {
    let (_, offset, blur) = parse_text_shadow("4px 0.5rem 0.25em");
    approx_eq(offset[0], 4.0);
    approx_eq(offset[1], 8.0);
    approx_eq(blur, 4.0);
}

#[test]
fn letter_spacing_inherits_when_unspecified() {
    let parent = TextStyle {
        letter_spacing: 0.4,
        letter_spacing_specified: true,
        ..Default::default()
    };
    let child = TextStyle::default();
    let out = child.effective_ancestry(&parent);
    assert!((out.letter_spacing - 0.4).abs() < f32::EPSILON);
    assert!(!out.letter_spacing_specified);
}

#[test]
fn letter_spacing_specified_zero_does_not_inherit() {
    let parent = TextStyle {
        letter_spacing: 0.4,
        letter_spacing_specified: true,
        ..Default::default()
    };
    let child = TextStyle {
        letter_spacing: 0.0,
        letter_spacing_specified: true,
        ..Default::default()
    };
    let out = child.effective_ancestry(&parent);
    assert!(out.letter_spacing.abs() < f32::EPSILON);
}

#[test]
fn letter_spacing_cascades_to_unspecified_child() {
    let child = crate::core::tree::Element::new().text("x");
    let mut root = crate::core::tree::Element::new()
        .letter_spacing(0.4)
        .child(child);
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    assert!(
        (root.children[0].text_style.letter_spacing - 0.4).abs() < 1e-4,
        "got {}",
        root.children[0].text_style.letter_spacing
    );
    assert!(!root.children[0].text_style.letter_spacing_specified);
}
