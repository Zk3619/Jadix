//! wrap=char caret chrome + emoji ink/advance layout regressions.

use crate::renderer::emoji_layout::{
    custom_emoji_left_centered_top, plan_emoji_layout_text, set_buffer_with_emoji_slots,
    EMOJI_ADVANCE_EM, EMOJI_INK_EM, EMOJI_LAYOUT_SLOT,
};
use crate::renderer::font_loader;
use crate::renderer::prepare_text::emoji_paint_pixel_size;
use crate::text::{
    build_visual_lines, caret_visual_line_index, measure_prefix_advance, soft_visual_line_byte_ranges,
    LineBreakSource,
};

/// Gallery-shaped sample (wrap=char demo).
const SAMPLE: &str =
    "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😃 —\nchar may mid-break.";

#[test]
fn wrap_char_visual_lines_differ_from_soft_word() {
    let size = 14.0;
    let lh = 21.0;
    let avail = 280.0;
    let word = build_visual_lines(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftWord);
    let ch = build_visual_lines(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftChar);
    assert!(
        ch.ranges != word.ranges,
        "SoftChar mid-breaks must differ from SoftWord for the gallery long-token sample"
    );
}

#[test]
fn soft_visual_line_byte_ranges_honors_soft_char() {
    let size = 14.0;
    let lh = 21.0;
    let avail = 280.0;
    let soft_word =
        soft_visual_line_byte_ranges(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftWord);
    let soft_char =
        soft_visual_line_byte_ranges(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftChar);
    assert_ne!(
        soft_word, soft_char,
        "chrome/hit must pass SoftChar for wrap=char — SoftWord ranges diverge"
    );
    let expected = build_visual_lines(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftChar)
        .ranges
        .clone();
    assert_eq!(soft_char, expected);
}

#[test]
fn wrap_char_caret_prefix_before_emoji_matches_soft_char_line() {
    let size = 14.0;
    let lh = 21.0;
    let avail = 280.0;
    let emoji_byte = SAMPLE.find('😃').expect("emoji");
    let model = build_visual_lines(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftChar);
    let line_i = caret_visual_line_index(&model.ranges, emoji_byte);
    let (s, e) = model.ranges[line_i];
    let line_body = &SAMPLE[s..e];
    let local = emoji_byte - s;
    let prefix = &line_body[..local];
    assert!(
        prefix.ends_with("emoji") || prefix.ends_with("文emoji"),
        "SoftChar line prefix before emoji should end near 'emoji', got {prefix:?}"
    );
    let soft_word =
        soft_visual_line_byte_ranges(SAMPLE, size, 400, lh, avail, LineBreakSource::SoftWord);
    let wi = caret_visual_line_index(&soft_word, emoji_byte);
    let (ws, _) = soft_word[wi];
    // When SoftWord starts the visual line earlier (keeps more of the long token),
    // chrome that used SoftWord would over-measure the caret X.
    if ws < s {
        let wrong_prefix = &SAMPLE[ws..emoji_byte];
        let wrong_x = measure_prefix_advance(wrong_prefix, size, 400, lh);
        let right_x = measure_prefix_advance(prefix, size, 400, lh);
        assert!(
            wrong_x > right_x + 2.0,
            "SoftWord chrome over-measure: wrong={wrong_x} right={right_x}"
        );
    }
}

#[test]
fn emoji_paint_pixel_size_matches_ink_em() {
    // CustomGlyph.width must equal cached raster px (= round(INK_EM×fs)).
    let size = 14.0_f32;
    let px = emoji_paint_pixel_size(size);
    let expected = (size * EMOJI_INK_EM).round() as u16;
    assert_eq!(px, expected);
    assert!(px as f32 > size, "ink must be slightly larger than 1em");
}

#[test]
fn emoji_ink_fits_inside_advance_slot() {
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &EMOJI_LAYOUT_SLOT.to_string());

    let text = "emoji😃—";
    let layout = plan_emoji_layout_text(text);
    let slot_start = layout.find(EMOJI_LAYOUT_SLOT).expect("slot");
    let size = 14.0_f32;
    let lh = size * 1.5;
    let attrs = glyphon::Attrs::new().family(glyphon::Family::SansSerif);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(Some(400.0), Some(lh + 2.0));
    set_buffer_with_emoji_slots(
        &mut buffer,
        &layout,
        &attrs,
        glyphon::Shaping::Advanced,
        size,
        lh,
    );
    buffer.shape_until_scroll(&mut fs, false);

    let (line_i, local) =
        crate::renderer::emoji_layout::global_byte_to_line_local(&layout, slot_start).unwrap();
    let (_run, g) =
        crate::renderer::emoji_layout::find_glyph_for_line_byte(&buffer, line_i, local).unwrap();
    let ink = emoji_paint_pixel_size(size) as f32;
    let (left, _top) =
        custom_emoji_left_centered_top(&buffer, &layout, slot_start, ink, ink).unwrap();

    assert!(
        (g.w / size - EMOJI_ADVANCE_EM).abs() < 0.25,
        "slot advance should track ADVANCE_EM, g.w/size={}",
        g.w / size
    );
    assert!(
        ink <= g.w + 0.5,
        "ink must fit inside advance (ink={ink} adv={})",
        g.w
    );
    assert!(
        left + 0.25 >= g.x,
        "emoji left={left} must not start left of slot advance origin g.x={gx} (ink={ink} g.w={gw})",
        gx = g.x,
        gw = g.w
    );
    assert!(
        left + ink.min(g.w) <= g.x + g.w + 0.25,
        "emoji right must stay within slot advance (left={left} ink={ink} g.x={gx} g.w={gw})",
        gx = g.x,
        gw = g.w
    );
}
