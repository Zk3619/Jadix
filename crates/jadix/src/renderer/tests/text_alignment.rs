use super::set_buffer_text_alignment;
use crate::style::{TextAlign, TextAlignLast, TextDirection, TextStyle};
use glyphon::cosmic_text::{fontdb, Align, AlignLast};

fn aligned_buffer(
    text: &str,
    source: Option<&str>,
    align: TextAlign,
    last: TextAlignLast,
) -> glyphon::Buffer {
    let mut fonts =
        glyphon::FontSystem::new_with_locale_and_db("en-US".into(), fontdb::Database::new());
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../crates/jadix_cosmic_text/fonts/FiraMono-Medium.ttf")
            .to_vec(),
    );
    fonts.db_mut().load_font_data(
        include_bytes!("../../../../../crates/jadix_cosmic_text/fonts/NotoSansHebrew.ttf").to_vec(),
    );
    let mut buffer = glyphon::Buffer::new(&mut fonts, glyphon::Metrics::new(16.0, 22.0));
    buffer.set_text(
        text,
        &glyphon::Attrs::new(),
        glyphon::Shaping::Advanced,
        None,
    );
    let style = TextStyle {
        align: Some(align),
        align_last: Some(last),
        direction: TextDirection::Auto,
        ..TextStyle::default()
    };
    set_buffer_text_alignment(
        &mut buffer,
        source,
        align.to_glyphon(style.direction),
        &style,
        true,
    );
    buffer
}

#[test]
fn prewrapped_alignment_only_overrides_original_paragraph_ends() {
    let buffer = aligned_buffer(
        "שלום\nעולם\nאב\nגד",
        Some("שלום עולם\nאב גד"),
        TextAlign::Left,
        TextAlignLast::Center,
    );
    let last: Vec<_> = buffer.lines.iter().map(|line| line.align_last()).collect();
    assert_eq!(
        last,
        vec![None, Some(AlignLast::Center), None, Some(AlignLast::Center)]
    );
    let text: Vec<_> = buffer.lines.iter().map(|line| line.text()).collect();
    assert_eq!(text, vec!["שלום", "עולם", "אב", "גד"]);
}

#[test]
fn prewrapped_justification_preserves_soft_lines_and_last_override() {
    let buffer = aligned_buffer(
        "one two\nthree four\nfive six",
        Some("one two three four five six"),
        TextAlign::Justify,
        TextAlignLast::Center,
    );
    let last: Vec<_> = buffer.lines.iter().map(|line| line.align_last()).collect();
    assert_eq!(
        last,
        vec![
            Some(AlignLast::Justified),
            Some(AlignLast::Justified),
            Some(AlignLast::Center)
        ]
    );
}

#[test]
fn prewrapped_neutral_tail_uses_original_paragraph_direction() {
    for (last, expected) in [
        (TextAlignLast::Start, AlignLast::Right),
        (TextAlignLast::End, AlignLast::Left),
    ] {
        let buffer = aligned_buffer(
            "שלום\nעולם\n123",
            Some("שלום עולם 123"),
            TextAlign::Start,
            last,
        );
        assert!(buffer
            .lines
            .iter()
            .all(|line| line.align() == Some(Align::Right)));
        assert_eq!(buffer.lines.last().unwrap().align_last(), Some(expected));
    }
}

#[test]
fn prewrapped_auto_justification_uses_original_logical_start() {
    let buffer = aligned_buffer(
        "שלום\nעולם\n123",
        Some("שלום עולם 123"),
        TextAlign::Justify,
        TextAlignLast::Auto,
    );
    assert_eq!(buffer.lines[0].align_last(), Some(AlignLast::Justified));
    assert_eq!(
        buffer.lines.last().unwrap().align_last(),
        Some(AlignLast::Right)
    );
}

#[test]
fn auto_direction_distinguishes_logical_and_physical_alignment() {
    for (align, expected) in [
        (TextAlign::Start, Align::Right),
        (TextAlign::Left, Align::Left),
    ] {
        let buffer = aligned_buffer("שלום", None, align, TextAlignLast::Auto);
        assert_eq!(buffer.lines[0].align(), Some(expected));
    }
}
