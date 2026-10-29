use super::*;

#[test]
fn advance_equals_ink_plus_bearings() {
    assert!((EMOJI_ADVANCE_EM - (EMOJI_INK_EM + 2.0 * EMOJI_BEARING_EM)).abs() < 1e-6);
    assert!(EMOJI_ADVANCE_EM > EMOJI_INK_EM);
    assert!(EMOJI_BEARING_EM > 0.0);
}

#[test]
fn newline_byte_maps_to_second_line() {
    let s = "a\n😀";
    let idx = s.find('😀').unwrap();
    let (li, loc) = global_byte_to_line_local(s, idx).unwrap();
    assert_eq!(li, 1);
    assert_eq!(loc, 0);
}

#[test]
fn plan_collapses_family_zwj_to_one_slot() {
    let raw = "A👨‍👩‍👧‍👦B";
    let layout = plan_emoji_layout_text(raw);
    assert_eq!(layout.chars().count(), 3);
    assert!(layout.contains(EMOJI_LAYOUT_SLOT));
    assert!(!layout.contains('\u{200d}'));
}

#[test]
fn left_cursor_resets_on_soft_wrap_line() {
    let slot = 16.0_f32;
    let mut cursor = f32::NEG_INFINITY;
    let mut line_top = None;
    let l0 = advance_emoji_left_cursor(100.0, 0.0, slot, &mut cursor, &mut line_top);
    assert!((l0 - 100.0).abs() < f32::EPSILON);
    let l1 = advance_emoji_left_cursor(10.0, 0.0, slot, &mut cursor, &mut line_top);
    assert!(
        l1 + 0.1 >= 100.0,
        "same-line backstop should keep monotonic left, got {l1}"
    );
    let l2 = advance_emoji_left_cursor(4.0, 24.0, slot, &mut cursor, &mut line_top);
    assert!(
        (l2 - 4.0).abs() < 0.01,
        "wrapped emoji must keep line-start left, got {l2} (stair-step regression)"
    );
}

#[test]
fn left_cursor_keeps_first_glyph_nudge_left_of_origin() {
    let slot = 16.0_f32;
    let mut cursor = f32::NEG_INFINITY;
    let mut line_top = None;
    let left = advance_emoji_left_cursor(-1.25, 0.0, slot, &mut cursor, &mut line_top);
    assert!(
        (left + 1.25).abs() < 0.01,
        "first-on-line centering overflow must not be packed to 0, got {left}"
    );
}

#[test]
fn visible_paint_size_covers_ink_that_leading_none_strut_cannot() {
    let font = 12.0_f32;
    let strut_h = font;
    let scale = 1.25_f32;
    let ink = font * EMOJI_INK_EM;
    let (pw, ph) = emoji_visible_paint_size(16.0, strut_h, font);
    assert!((ph - ink).abs() < 1e-6, "paint h={ph} ink={ink}");
    assert!((pw - 16.0).abs() < 1e-6);
    assert!(
        strut_h * scale + 1.0 < ink * scale,
        "strut*hover:scale-125 is the screenshot crop ({:.2} < {:.2})",
        strut_h * scale,
        ink * scale
    );
    assert!(ph * scale + 0.01 >= ink * scale);
    let bounds = layout_bounds_with_visible_emoji_ink(
        [10.0, 20.0, 16.0, 12.0],
        Some("😮"),
        font,
        true,
        true,
    );
    assert!((bounds[3] - ink).abs() < 1e-6);
    assert!((bounds[1] - (20.0 - (ink - 12.0) * 0.5)).abs() < 1e-6);
}

#[test]
fn slot_advance_near_one_em_with_coverage() {
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;

    let slot = EMOJI_SHAPING_GLYPH.to_string();
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &slot);
    let ui = best_font_family(&fs);
    let size = 14.0_f32;
    let lh = size * 1.5;
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(Some(64.0), Some(lh + 2.0));
    buffer.set_text(
        &slot,
        &glyphon::Attrs::new().family(ui),
        glyphon::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let w = buffer
        .layout_runs()
        .map(|r| r.line_w)
        .fold(0.0_f32, f32::max);
    let ratio = w / size;
    println!(
        "slot U+{:04X} whitespace={} advance={w:.2} ratio={ratio:.2}",
        EMOJI_LAYOUT_SLOT as u32,
        EMOJI_LAYOUT_SLOT.is_whitespace()
    );
    assert!(
        ratio > 0.85 && ratio < 1.35,
        "bare object slot (no ADVANCE metrics) must be ~1em after coverage, got ratio={ratio}"
    );
}

#[test]
fn slot_attrs_advance_near_advance_em() {
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;

    let layout = plan_emoji_layout_text("a😀b");
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &layout);
    let ui = best_font_family(&fs);
    let size = 14.0_f32;
    let lh = size * 1.5;
    let attrs = glyphon::Attrs::new().family(ui);
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
    let slot_start = layout.find(EMOJI_LAYOUT_SLOT).unwrap();
    let (line_i, local) = global_byte_to_line_local(&layout, slot_start).unwrap();
    let (_run, g) = find_glyph_for_line_byte(&buffer, line_i, local).unwrap();
    let ratio = g.w / size;
    assert!(
        (ratio - EMOJI_ADVANCE_EM).abs() < 0.2,
        "slot advance should track ADVANCE_EM≈{EMOJI_ADVANCE_EM}, got ratio={ratio} g.w={}",
        g.w
    );
    let ink = size * EMOJI_INK_EM;
    assert!(
        ink <= g.w + 0.5,
        "ink must fit inside advance (ink={ink} adv={})",
        g.w
    );
}

#[test]
fn shape_text_slots_all_emoji_forms() {
    // Mixed prose and chip leaves alike — every emoji becomes a layout slot.
    let mixed = shape_text_for_layout("hi 👍 there");
    assert!(
        mixed.contains(EMOJI_LAYOUT_SLOT),
        "mixed text must slot emoji for optical centering"
    );
    assert!(mixed.contains("hi"), "non-emoji text must survive");
    let chip = shape_text_for_layout("😮");
    assert_eq!(chip.as_ref(), EMOJI_LAYOUT_SLOT.to_string());
    let plain = shape_text_for_layout("abc");
    assert_eq!(plain.as_ref(), "abc");
}

#[test]
fn emoji_layout_slot_is_not_whitespace() {
    assert!(
        !EMOJI_LAYOUT_SLOT.is_whitespace(),
        "slot must survive soft-wrap leading-whitespace strip (was EM SPACE)"
    );
}

#[test]
fn invisible_slot_attrs_still_emit_layout_glyphs() {
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;

    let text = "Hi 😀 there";
    let layout = plan_emoji_layout_text(text);
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &layout);
    let ui = best_font_family(&fs);
    let size = 14.0_f32;
    let lh = size * 1.5;
    let visible = glyphon::Attrs::new()
        .family(ui)
        .color(glyphon::Color::rgba(0, 0, 0, 255));
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(Some(400.0), Some(lh * 2.0));
    set_buffer_with_emoji_slots(
        &mut buffer,
        &layout,
        &visible,
        glyphon::Shaping::Advanced,
        size,
        lh,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let slot_start = layout.find(EMOJI_LAYOUT_SLOT).unwrap();
    let ink = size * EMOJI_INK_EM;
    let (left, _top) = custom_emoji_left_centered_top(&buffer, &layout, slot_start, ink, ink)
        .expect("alpha=0 slot must still produce a layout glyph for Twemoji alignment");
    assert!(left.is_finite() && left >= 0.0, "left={left}");
}

#[test]
fn pagination_captions_keep_literal_ideograph_with_and_without_emoji() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    for text in ["上一页", "下一页", "上一页 😀 下一页"] {
        let layout = shape_text_for_layout(text);
        crate::renderer::font_loader::ensure_coverage_for_text(&mut fs, &layout);
        let attrs = glyphon::Attrs::new()
            .family(crate::renderer::text_hit::best_font_family(&fs))
            .color(glyphon::Color::rgba(17, 17, 17, 255));
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 20.0));
        buffer.set_size(Some(400.0), Some(40.0));
        set_buffer_with_emoji_slots(
            &mut buffer,
            &layout,
            &attrs,
            glyphon::Shaping::Advanced,
            14.0,
            20.0,
        );
        buffer.shape_until_scroll(&mut fs, false);
        let mut visible_ones = 0;
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                if run.text.get(glyph.start..glyph.end) == Some("一")
                    && !glyph_is_emoji_layout_slot(&run, glyph)
                {
                    assert!(
                        glyph.color_opt.map(|c| c.a()).unwrap_or(255) > 0,
                        "{text}: literal 一 must retain ink"
                    );
                    visible_ones += 1;
                }
            }
        }
        assert_eq!(visible_ones, text.matches('一').count());
    }
}

#[test]
fn slot_glyphs_are_fully_transparent() {
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;

    let layout = plan_emoji_layout_text("👍");
    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &layout);
    let ui = best_font_family(&fs);
    let size = 12.0_f32;
    let lh = size;
    let visible = glyphon::Attrs::new()
        .family(ui)
        .color(glyphon::Color::rgba(17, 17, 17, 255));
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
    buffer.set_size(Some(64.0), Some(lh * 2.0));
    set_buffer_with_emoji_slots(
        &mut buffer,
        &layout,
        &visible,
        glyphon::Shaping::Advanced,
        size,
        lh,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let mut found = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            if !glyph_is_emoji_layout_slot(&run, g) {
                continue;
            }
            let alpha = g.color_opt.map(|c| c.a()).unwrap_or(255);
            assert_eq!(
                alpha, 0,
                "CJK slot `一` must stay alpha=0 (got color_opt={:?}); \
                     otherwise the ideograph paints as a horizontal line through Twemoji",
                g.color_opt
            );
            found += 1;
        }
    }
    assert!(
        found >= 1,
        "emoji-only chip must emit an invisible slot glyph"
    );
}

#[test]
fn map_original_byte_compresses_emoji_grapheme() {
    let s = "ab😀c";
    assert_eq!(map_original_byte_to_emoji_layout(s, 0), 0);
    assert_eq!(map_original_byte_to_emoji_layout(s, 2), 2);
    let after = s.find('😀').unwrap() + '😀'.len_utf8();
    assert_eq!(
        map_original_byte_to_emoji_layout(s, after),
        2 + EMOJI_LAYOUT_SLOT.len_utf8()
    );
    assert_eq!(
        map_original_byte_to_emoji_layout(s, s.len()),
        plan_emoji_layout_text(s).len()
    );
}
