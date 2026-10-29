use super::{
    apply_letter_spacing_approx, node_shape_hash, node_shape_identity_hash,
    node_shape_key_from_identity,
};
use crate::style::TextDirection;
use std::borrow::Cow;
use std::time::Instant;

#[test]
fn document_rich_text_preserves_crlf_lines_and_attributes() {
    let mut fonts = glyphon::FontSystem::new();
    let mut buffer = glyphon::Buffer::new(&mut fonts, glyphon::Metrics::new(14.0, 21.0));
    let attrs = glyphon::Attrs::new();
    let accent = attrs.clone().color(glyphon::Color::rgb(90, 120, 210));
    let spans = vec![
        ("fn\r".into(), accent),
        ("\nmain\r\n".into(), attrs.clone()),
    ];
    super::set_document_rich_text(&mut buffer, &spans, &attrs, glyphon::Shaping::Advanced);
    assert_eq!(buffer.lines.len(), 3);
    assert_eq!(buffer.lines[0].text(), "fn");
    assert_eq!(buffer.lines[1].text(), "main");
    assert_eq!(buffer.lines[0].ending().as_str(), "\r\n");
    assert_eq!(buffer.lines[1].ending().as_str(), "\r\n");
    assert_ne!(buffer.lines[0].attrs_list(), buffer.lines[1].attrs_list());
}

#[test]
fn document_span_colors_invalidate_shape_identity() {
    let original = vec![crate::style::TextSpan::new("fn").color([90, 120, 210, 255])];
    let changed = vec![crate::style::TextSpan::new("fn").color([190, 90, 110, 255])];
    assert_ne!(
        super::rich_shape_identity_hash(1, Some(&original)),
        super::rich_shape_identity_hash(1, Some(&changed))
    );
    assert_ne!(
        super::rich_shape_identity_hash(1, None),
        super::rich_shape_identity_hash(1, Some(&original))
    );
}

#[test]
fn ltr_and_auto_do_not_inject_lrm() {
    assert_eq!(super::direction_mark(TextDirection::Auto), "");
    assert_eq!(super::direction_mark(TextDirection::Ltr), "");
    assert_eq!(super::direction_mark(TextDirection::Rtl), "\u{200F}");
}

fn glyph_rgba(c: glyphon::Color) -> [u8; 4] {
    let v = c.0;
    [
        ((v >> 16) & 0xff) as u8,
        ((v >> 8) & 0xff) as u8,
        (v & 0xff) as u8,
        ((v >> 24) & 0xff) as u8,
    ]
}

#[test]
fn selection_rich_text_sets_color_opt_on_selected_range() {
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    buffer.set_size(Some(400.0), Some(40.0));
    let attrs = glyphon::Attrs::new().color(glyphon::Color::rgba(1, 2, 3, 255));
    super::set_selection_colored_buffer_text(
        &mut buffer,
        "abc",
        1,
        2,
        [9, 8, 7, 255],
        &attrs,
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let mut found_sel = false;
    let mut found_other = false;
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            let rgba = g.color_opt.map(glyph_rgba);
            if g.start >= 1 && g.start < 2 {
                assert_eq!(rgba, Some([9, 8, 7, 255]), "selected glyph {:?}", g.start);
                found_sel = true;
            } else {
                assert_eq!(rgba, Some([1, 2, 3, 255]), "unselected glyph {:?}", g.start);
                found_other = true;
            }
        }
    }
    assert!(found_sel, "expected a glyph in the selected byte range");
    assert!(found_other, "expected a glyph outside the selected range");
}

#[test]
fn selection_rich_text_recolors_disjoint_ranges() {
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    buffer.set_size(Some(400.0), Some(80.0));
    let attrs = glyphon::Attrs::new().color(glyphon::Color::rgba(1, 2, 3, 255));
    let text = "abcd\nefgh";
    super::set_selection_colored_buffer_ranges(
        &mut buffer,
        text,
        &[((0, 2), [9, 8, 7, 255]), ((5, 7), [9, 8, 7, 255])],
        &attrs,
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(&mut fs, false);
    let mut found_sel = 0usize;
    let mut found_other = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            let rgba = g.color_opt.map(glyph_rgba);
            let selected = g.start < 2 || (g.start >= 5 && g.start < 7);
            if selected {
                assert_eq!(rgba, Some([9, 8, 7, 255]), "selected glyph {:?}", g.start);
                found_sel += 1;
            } else {
                assert_eq!(rgba, Some([1, 2, 3, 255]), "unselected glyph {:?}", g.start);
                found_other += 1;
            }
        }
    }
    assert!(
        found_sel >= 2,
        "expected glyphs in both extra selection ranges"
    );
    assert!(
        found_other >= 1,
        "expected a glyph outside the selected ranges"
    );
}

#[test]
fn slotted_layout_stays_invisible_even_when_hide_flag_is_false() {
    use crate::renderer::emoji_layout::{
        glyph_is_emoji_layout_slot, plan_emoji_layout_text, EMOJI_LAYOUT_SLOT,
    };
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    font_loader::ensure_coverage_for_text(&mut fs, &EMOJI_LAYOUT_SLOT.to_string());
    install_shared_font_system(&mut fs);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(12.0, 12.0));
    buffer.set_size(Some(64.0), Some(24.0));
    let attrs = glyphon::Attrs::new().color(glyphon::Color::rgba(17, 17, 17, 255));
    let layout = plan_emoji_layout_text("👍");
    assert!(
        layout.contains(EMOJI_LAYOUT_SLOT),
        "chip layout is the object slot, not the original emoji"
    );
    super::set_shaped_buffer_text(
        &mut buffer,
        &mut fs,
        &layout,
        attrs,
        glyphon::Shaping::Advanced,
        false,
        12.0,
        12.0,
        &[],
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
                "already-slotted paint_text must hide `一` even when hide_emoji=false \
                     (got color_opt={:?})",
                g.color_opt
            );
            found += 1;
        }
    }
    assert!(found >= 1, "expected an invisible slot glyph");
}

#[test]
fn selection_recolor_applies_to_text_around_emoji() {
    use crate::renderer::emoji_layout::plan_emoji_layout_text;
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    buffer.set_size(Some(400.0), Some(40.0));
    let attrs = glyphon::Attrs::new().color(glyphon::Color::rgba(1, 2, 3, 255));
    let text = "ab😀cd";
    let fg = [9_u8, 8, 7, 255];
    super::set_shaped_buffer_text(
        &mut buffer,
        &mut fs,
        text,
        attrs,
        glyphon::Shaping::Advanced,
        true,
        14.0,
        21.0,
        &[((0, text.len()), fg)],
    );
    buffer.shape_until_scroll(&mut fs, false);
    let layout = plan_emoji_layout_text(text);
    let mut found_plain = false;
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            let slice = layout.get(g.start..g.end.min(layout.len())).unwrap_or("");
            if slice.chars().all(|c| c.is_ascii_alphabetic()) && !slice.is_empty() {
                let rgba = g.color_opt.map(glyph_rgba);
                assert_eq!(
                    rgba,
                    Some(fg),
                    "plain glyph {slice:?} should use selection fg"
                );
                found_plain = true;
            }
        }
    }
    assert!(
        found_plain,
        "expected recolored Latin glyphs around the emoji"
    );
}

#[test]
fn bidi_wj_selection_keeps_all_glyphs() {
    use crate::renderer::font_loader;
    use crate::text::install_shared_font_system;

    let mut fs = font_loader::create_font_system();
    font_loader::apply_default_families(fs.db_mut());
    install_shared_font_system(&mut fs);
    let text = "Hello עברית\u{2060}789 and العربية — soft wrap keeps WJ pairs.";
    let attrs = glyphon::Attrs::new().color(glyphon::Color::rgba(20, 20, 20, 255));
    let mut plain = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    plain.set_size(Some(256.0), Some(120.0));
    plain.set_text(text, &attrs, glyphon::Shaping::Advanced, None);
    plain.shape_until_scroll(&mut fs, false);
    let plain_keys: Vec<(usize, usize)> = plain
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(|g| (g.start, g.end)))
        .collect();

    let mut selected = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(14.0, 21.0));
    selected.set_size(Some(256.0), Some(120.0));
    super::set_selection_colored_buffer_text(
        &mut selected,
        text,
        0,
        text.len(),
        [240, 240, 240, 255],
        &attrs,
        glyphon::Shaping::Advanced,
    );
    selected.shape_until_scroll(&mut fs, false);
    let sel_keys: Vec<(usize, usize)> = selected
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(|g| (g.start, g.end)))
        .collect();
    assert_eq!(
        plain_keys, sel_keys,
        "selection recolor must not drop or split BiDi/WJ glyphs"
    );
    assert!(
        !sel_keys.is_empty(),
        "gallery BiDi+WJ string must produce glyphs"
    );
}

#[test]
fn emoji_wrap_width_q_buckets_one_px_jitter() {
    assert_eq!(
        super::emoji_wrap_width_q(400.0),
        super::emoji_wrap_width_q(407.0)
    );
    assert_ne!(
        super::emoji_wrap_width_q(400.0),
        super::emoji_wrap_width_q(408.0)
    );
}

#[test]
fn emoji_sequence_hash_is_content_only() {
    let a = super::emoji_sequence_content_hash("Family: 👨‍👩‍👧‍👦");
    let b = super::emoji_sequence_content_hash("Family: 👨‍👩‍👧‍👦");
    let c = super::emoji_sequence_content_hash("Skin tone: 👍🏽");
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn letter_spacing_zero_borrows_without_alloc() {
    let s = "The quick brown fox 中文 عربي 😀";
    match apply_letter_spacing_approx(s, 0.0, 14.0) {
        Cow::Borrowed(b) => assert_eq!(b, s),
        Cow::Owned(_) => panic!("zero letter-spacing must not allocate on scroll hot path"),
    }
}

#[test]
fn shape_key_from_identity_avoids_rehashing_text() {
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let text = "日本語テスト：こんにちは世界。汉字・かな";
    let identity = node_shape_identity_hash(
        text,
        14.0,
        21.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        [0, 0, 0, 255],
        400,
        false,
    );
    let from_id = node_shape_key_from_identity(identity, 320.0, 40.0, false);
    let full = node_shape_hash(
        text,
        14.0,
        21.0,
        320.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        [0, 0, 0, 255],
        false,
    );
    assert_eq!(from_id, full);
}

/// Feedback loop: scroll-like CPU for dense multilingual labels must stay cheap.
/// Pre-fix: every node did text.to_string() + double full-text hash each frame.
#[test]
fn scroll_like_label_cpu_stays_under_budget() {
    let samples = [
        "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.",
        "中文测试：输入法、标点符号、全角半角、换行和字距表现。",
        "日本語テスト：こんにちは世界。漢字・かな・カタカナ・句読点「」『』の表示確認。",
        "Arabic: مرحبا بالعالم، هذا اختبار لاتجاه الكتابة والمحاذاة وعرض الحروف.",
        "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦",
        "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789",
    ];
    // Simulate ~80 visible labels (text page band) × 60 scroll frames.
    let labels: Vec<&str> = samples.iter().cycle().take(80).copied().collect();
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let t0 = Instant::now();
    for _ in 0..60 {
        for (i, text) in labels.iter().enumerate() {
            let spaced = apply_letter_spacing_approx(text, 0.0, 14.0);
            assert!(matches!(spaced, Cow::Borrowed(_)));
            let identity = node_shape_identity_hash(
                spaced.as_ref(),
                14.0,
                21.0,
                400,
                family,
                align,
                shaping,
                0.0,
                TextDirection::Auto,
                [0, 0, 0, 255],
                400,
                false,
            );
            let _key = node_shape_key_from_identity(identity, 400.0 + (i % 7) as f32, 24.0, false);
        }
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    // Debug + CI headroom: 80×60 identity hashes must stay well under a frame budget.
    // (Was allocating+double-hashing full strings — multi× slower.)
    assert!(
        ms < 50.0,
        "scroll-like shape-key CPU too slow: {ms:.2}ms for 80 labels × 60 frames"
    );
}

#[test]
fn shape_hash_changes_when_text_alpha_flips() {
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let hidden = node_shape_hash(
        "x",
        14.0,
        14.0,
        20.0,
        20.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        [100, 100, 100, 0],
        false,
    );
    let shown = node_shape_hash(
        "x",
        14.0,
        14.0,
        20.0,
        20.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        [100, 100, 100, 200],
        false,
    );
    assert_ne!(
        hidden, shown,
        "opacity/color alpha must invalidate shaped cache (Input adornments)"
    );
}

#[test]
fn bounds_miss_keeps_shape_identity_when_only_width_changes() {
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let text = "Bidirectional sample: English 123 - العربية";
    let color = [30u8, 30, 30, 255];
    let shape_a = node_shape_identity_hash(
        text,
        14.0,
        21.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        400,
        false,
    );
    let shape_b = node_shape_identity_hash(
        text,
        14.0,
        21.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        400,
        false,
    );
    assert_eq!(shape_a, shape_b, "identity must ignore leaf bounds");

    let key_narrow = node_shape_hash(
        text,
        14.0,
        21.0,
        160.0,
        42.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        false,
    );
    let key_wide = node_shape_hash(
        text,
        14.0,
        21.0,
        260.0,
        42.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        false,
    );
    assert_ne!(key_narrow, key_wide, "bounds change must miss Exact key");
    // Same shape_hash + different key_hash ⇒ Bounds path (iced Difference::Bounds).
    assert_eq!(
        node_shape_identity_hash(
            text,
            14.0,
            21.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            400,
            false,
        ),
        shape_a
    );
}

#[test]
fn paint_key_hash_ignores_force_fast_flag() {
    // prepare_text_frame always passes force_fast=false into node_shape_hash so
    // scroll fast↔settle does not mass-miss (text page white flash).
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let color = [0u8, 0, 0, 255];
    let settle = node_shape_hash(
        "Typography",
        14.0,
        21.0,
        201.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Ltr,
        color,
        false,
    );
    let scroll = node_shape_hash(
        "Typography",
        14.0,
        21.0,
        201.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Ltr,
        color,
        false,
    );
    assert_eq!(settle, scroll);
}

#[test]
fn fast_mode_quantizes_bounds_to_cut_exact_churn() {
    let family = glyphon::Family::SansSerif;
    let align = glyphon::cosmic_text::Align::Left;
    let shaping = glyphon::Shaping::Advanced;
    let color = [0u8, 0, 0, 255];
    let a = node_shape_hash(
        "hi",
        14.0,
        21.0,
        201.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        true,
    );
    let b = node_shape_hash(
        "hi",
        14.0,
        21.0,
        202.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        true,
    );
    assert_eq!(
        a, b,
        "2px quantize during fast should collapse nearby widths"
    );
    let c = node_shape_hash(
        "hi",
        14.0,
        21.0,
        201.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        false,
    );
    let d = node_shape_hash(
        "hi",
        14.0,
        21.0,
        202.0,
        40.0,
        400,
        family,
        align,
        shaping,
        0.0,
        TextDirection::Auto,
        color,
        false,
    );
    assert_ne!(c, d, "rest path keeps 1px Exact sensitivity");
}
