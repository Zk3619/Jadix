use super::*;

#[test]
fn wrap_mode_parse() {
    assert_eq!(WrapMode::parse("soft"), WrapMode::Word);
    assert_eq!(WrapMode::parse("word"), WrapMode::Word);
    assert_eq!(WrapMode::parse("off"), WrapMode::None);
    assert_eq!(WrapMode::parse("char"), WrapMode::Char);
}

#[test]
fn soft_word_wraps_latin() {
    let text = "hello world hello world hello world";
    let model = build_visual_lines(text, 14.0, 400, 21.0, 80.0, LineBreakSource::SoftWord);
    assert!(
        model.line_count() > 1,
        "expected wrap, got {:?}",
        model.ranges
    );
}

#[test]
fn none_is_hard_lines_only() {
    let text = "aaaa bbbb cccc";
    let model = build_visual_lines(text, 14.0, 400, 21.0, 40.0, LineBreakSource::None);
    assert_eq!(model.line_count(), 1);
}

#[test]
fn preferred_x_roundtrip() {
    let text = "abc";
    let model = build_visual_lines(text, 14.0, 400, 21.0, 400.0, LineBreakSource::SoftWord);
    let x = model.caret_x_on_line(text.len());
    let b = model.byte_at_preferred_x(0, x);
    assert_eq!(b, text.len());
}

#[test]
fn char_falls_back_on_bidi() {
    let text = "עברית\u{2060}789";
    let src = LineBreakSource::SoftChar.resolve_for_text(text);
    assert_eq!(src, LineBreakSource::SoftWord);
}

#[test]
fn visual_selection_ltr_island_in_rtl_paragraph() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);

    let text = "עברית Hello";
    let lo = text.find("Hello").expect("Hello");
    let hi = lo + "Hello".len();
    let slabs = visual_selection_highlights(
        text,
        14.0,
        400,
        21.0,
        280.0,
        LineBreakSource::SoftWord,
        lo,
        hi,
    );
    assert!(!slabs.is_empty(), "expected a highlight over Hello");
    let hebrew_w = crate::text::measure_prefix_advance(&text[..lo], 14.0, 400, 21.0);
    let x = slabs[0].1;
    assert!(
            x > hebrew_w + 1.0,
            "Hello glyphs are past the logical Hebrew prefix (LTR box would cover Hebrew); x={x} hebrew_w={hebrew_w}"
        );
}
