use super::*;

#[test]
fn ascii_measure_keeps_existing_zero_line_height_contract() {
    invalidate_prepare_cache();
    let prepared = prepare("numeric 42", 16.0, 400, 0.0);
    assert!(prepared.max_segment_width.is_finite());
    assert!(prepared.max_segment_width > 0.0);
    assert!(
        ASCII_ADVANCES.with(|c| c.borrow().is_empty()),
        "invalid shaping metrics must not enter the character-advance cache"
    );
}

#[test]
fn ascii_advance_cache_matches_basic_shaping_and_separates_fonts() {
    invalidate_prepare_cache();
    with_layout_font_system(|fs| {
        for family in [Family::SansSerif, Family::Monospace, Family::Serif] {
            let family = if family == Family::SansSerif {
                crate::renderer::text_hit::best_font_family(fs)
            } else {
                family
            };
            for (size, weight) in [(12.0, 400), (16.0, 600)] {
                let weight =
                    crate::utils::font::normalize_weight_for_family(fs.db(), family, weight);
                for text in ["AVATAR", "111.7", "589.2", "   padded 42", "()!@#$%&*+-=/"] {
                    let actual = measure_run_width(fs, text, size, size * 1.5, weight, family);
                    let mut buffer = Buffer::new(fs, Metrics::new(size, size * 1.5));
                    buffer.set_size(Some(16_384.0), Some(size * 3.0));
                    buffer.set_text(
                        text,
                        &Attrs::new()
                            .family(family)
                            .weight(glyphon::fontdb::Weight(weight)),
                        Shaping::Basic,
                        None,
                    );
                    buffer.shape_until_scroll(fs, false);
                    let expected = buffer
                        .layout_runs()
                        .map(|r| r.line_w)
                        .fold(0.0_f32, f32::max);
                    assert!((actual - expected).abs() < 0.01,
                            "{text:?} family={family:?} size={size}: cached={actual}, shaped={expected}");
                }
            }
        }
    });
    assert_ne!(
        prepare_key("WWW", 12.0, 400, 18.0, Family::Name("Segoe UI")).family_hash,
        prepare_key("WWW", 12.0, 400, 18.0, Family::Name("Consolas")).family_hash
    );
    invalidate_prepare_cache();
    assert!(
        ASCII_ADVANCES.with(|c| c.borrow().is_empty()),
        "font changes invalidate character metrics too"
    );
}

#[test]
fn measure_leaf_nowrap_ignores_narrow_avail() {
    let text = "Default (68)";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let intrinsic = snap_content_text_width(intrinsic_width(&prepared));
    let avail = (intrinsic * 0.5).max(1.0);
    let (w, h) = measure_leaf(text, size, 500, lh, None, Some(avail), 0.0, true);
    assert!(
        (w - intrinsic).abs() < 0.5,
        "nowrap width {w} should be intrinsic {intrinsic}"
    );
    assert!(
        (h - lh).abs() < 0.5,
        "nowrap must stay one line, h={h} lh={lh}"
    );
}

#[test]
fn measure_leaf_does_not_shrink_below_unbreakable_word() {
    let text = "Secondary";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let intrinsic = intrinsic_width(&prepared);
    assert!(intrinsic > 2.0, "sanity intrinsic={intrinsic}");
    // Slightly narrower than intrinsic — old path wrapped mid-word and grew height.
    let avail = (intrinsic - 1.0).max(1.0);
    let (w, h) = measure_leaf(text, size, 500, lh, None, Some(avail), 0.0, false);
    assert!(
        w + 0.01 >= prepared.max_segment_width.min(intrinsic),
        "width {w} should respect min-content {}",
        prepared.max_segment_width
    );
    assert!(
        (h - lh).abs() < 0.5,
        "single unbreakable word should stay one line, got h={h} lh={lh}"
    );
}

#[test]
fn measure_leaf_min_content_probe_returns_max_segment() {
    // `AvailableSpace::MinContent` is forwarded as avail_width=Some(0) from layout_tree;
    // that must yield max_segment_width, not the full intrinsic line.
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let intrinsic = intrinsic_width(&prepared);
    let (w, _) = measure_leaf(text, size, 500, lh, None, Some(0.0), 0.0, false);
    assert!(
        (w - prepared.max_segment_width).abs() < 1.0,
        "MinContent probe width {w} should be max_seg {}, intrinsic={intrinsic}",
        prepared.max_segment_width
    );
    assert!(
        w + 1.0 < intrinsic,
        "min-content {w} must be narrower than intrinsic {intrinsic}"
    );
}

#[test]
fn prepare_layout_wraps_latin() {
    let p = prepare("hello world from jadix text measure", 14.0, 400, 14.0 * 1.5);
    let wide = layout(&p, 10_000.0);
    assert_eq!(wide.line_count, 1);
    let narrow = layout(&p, 80.0);
    assert!(
        narrow.line_count >= 2,
        "expected wrap, got {} lines (max_line={})",
        narrow.line_count,
        narrow.max_line_width
    );
}

#[test]
fn gallery_bidi_and_rtl_emoji_soft_wrap_under_narrow_avail() {
    let size = 14.0_f32;
    let lh = size * 1.5;
    let samples = [
        "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789",
        "RTL+emoji+Latin: عربي 😀 English مرحبا 👍🏽 — 固定回归样例",
    ];
    for text in samples {
        let p = prepare(text, size, 500, lh);
        let intrinsic = intrinsic_width(&p);
        let avail = (intrinsic * 0.45)
            .max(p.max_segment_width + 1.0)
            .min(intrinsic - 1.0);
        let m = layout(&p, avail);
        let (w, h) = measure_leaf(text, size, 500, lh, Some(avail), None, 0.0, false);
        let glyphon_n = glyphon_soft_wrap_line_count(text, size, 500, lh, avail);
        println!(
            "text={:?}\nintrinsic={intrinsic:.1} max_seg={:.1} avail={avail:.1}\n\
                 layout lines={} max_line={:.1} measure=({w:.1},{h:.1}) glyphon_runs={glyphon_n}",
            &text[..text
                .chars()
                .take(24)
                .collect::<String>()
                .len()
                .min(text.len())],
            p.max_segment_width,
            m.line_count,
            m.max_line_width,
        );
        assert!(
            m.line_count >= 2,
            "measure layout should soft-wrap under avail={avail}, got {} lines",
            m.line_count
        );
        assert!(
            w <= avail + 1.0,
            "leaf width {w} must not exceed avail {avail}"
        );
        assert!(
            h + 0.5 >= lh * 2.0,
            "wrapped height should be >= 2 lines, h={h} lh={lh}"
        );
        assert!(
            glyphon_n >= 2,
            "glyphon should soft-wrap too, runs={glyphon_n}"
        );
    }
}

#[test]
fn fast_mode_skips_glyphon_fit_compute() {
    invalidate_prepare_cache();
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    set_layout_text_fast_mode(true);
    let miss = try_glyphon_fit_soft_wrap(text, size, 500, lh, 240.0);
    assert!(miss.is_none(), "empty cache must not trigger fit shaping");
    let fit = glyphon_fit_soft_wrap(text, size, 500, lh, 240.0);
    let hit = try_glyphon_fit_soft_wrap(text, size, 500, lh, layout_wrap_target(240.0, true));
    assert!(
        hit.is_some(),
        "identical widths should reuse the exact-width fit cache"
    );
    assert!(
        fit.wrap_width <= 240.0 + 1.0 || fit.max_ink <= 240.0 * 1.08 + 2.0,
        "fit must tighten wrap for bidi"
    );
    set_layout_text_fast_mode(false);
    shutdown_layout_text();
}

#[test]
fn fast_mode_layout_height_skips_fit_compute_on_miss() {
    invalidate_prepare_cache();
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let avail = 240.0f32;
    let metrics = layout(&prepared, avail);
    set_layout_text_fast_mode(true);
    let h = bidi_soft_wrap_height(
        text,
        size,
        500,
        lh,
        avail,
        prepared.max_segment_width,
        &metrics,
        false,
    );
    assert!(
        (h - metrics.height.max(lh)).abs() < 0.01,
        "resize fast must not compute fit on miss"
    );
    assert!(
        try_glyphon_fit_soft_wrap(text, size, 500, lh, layout_wrap_target(avail, true)).is_none()
    );
    set_layout_text_fast_mode(false);
    shutdown_layout_text();
}

#[test]
fn fast_mode_layout_height_ignores_warm_fit_cache() {
    invalidate_prepare_cache();
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let avail = 240.0f32;
    let metrics = layout(&prepared, avail);
    let target = layout_wrap_target(avail, true);
    let fit = glyphon_fit_soft_wrap(text, size, 500, lh, target);
    assert!(fit.line_count > 0);
    set_layout_text_fast_mode(true);
    let h = bidi_soft_wrap_height(
        text,
        size,
        500,
        lh,
        avail,
        prepared.max_segment_width,
        &metrics,
        false,
    );
    assert_eq!(h, metrics.height.max(lh));
    set_layout_text_fast_mode(false);
    shutdown_layout_text();
}

#[test]
fn fast_word_wrap_uses_available_width_without_eight_pixel_steps() {
    let text = "Measured word wrapping stays stable across small width changes during resize";
    let size = 16.0;
    let line_height = 24.0;
    set_layout_text_fast_mode(true);
    let (_, narrow) = measure_leaf(text, size, 400, line_height, Some(120.1), None, 0.0, false);
    let (_, wider) = measure_leaf(text, size, 400, line_height, Some(127.9), None, 0.0, false);
    let (_, next_bucket) =
        measure_leaf(text, size, 400, line_height, Some(128.1), None, 0.0, false);
    set_layout_text_fast_mode(false);

    assert!(narrow > line_height, "fixture must exercise word wrapping");
    assert_eq!(
        narrow, wider,
        "these widths should retain the same word breaks"
    );
    assert!(
        next_bucket <= wider,
        "increasing width must not add wrapped lines"
    );
    assert_eq!(layout_wrap_target(127.9, true), 127.9);
    assert_eq!(layout_wrap_target(128.1, true), 128.1);
}

#[test]
fn gallery_titles_keep_paint_height_during_one_way_resize() {
    set_layout_text_fast_mode(false);
    for text in [
        "ViewportModal 无头壳（居中 Dialog / 贴边 Sheet）",
        "局部遮罩关键场景（Portal mount，非整窗 overlay）",
    ] {
        let mut ascending = Vec::new();
        invalidate_prepare_cache();
        for quarter in 1200..2160 {
            let width = quarter as f32 / 4.0;
            let (_, height) = measure_leaf(text, 20.0, 700, 30.0, Some(width), None, 0.0, false);
            let fit = glyphon_fit_soft_wrap(text, 20.0, 700, 30.0, width);
            assert!(
                height >= fit.line_count as f32 * 30.0,
                "undermeasured at {width}: {text}"
            );
            if let Some(&previous) = ascending.last() {
                assert!(
                    height <= previous,
                    "widening added a line at {width}: {text}"
                );
            }
            ascending.push(height);
        }
        invalidate_prepare_cache();
        for quarter in (1200..2160).rev() {
            let width = quarter as f32 / 4.0;
            let (_, height) = measure_leaf(text, 20.0, 700, 30.0, Some(width), None, 0.0, false);
            assert_eq!(
                height,
                ascending[quarter - 1200],
                "resize direction changed height at {width}: {text}"
            );
        }
    }
    shutdown_layout_text();
}

#[test]
fn fit_needs_relayout_flag_can_be_rearmed_after_take() {
    // scroll_paint_only defers LayoutChanged by take()+note(); flag must survive that.
    let _ = take_fit_needs_relayout();
    note_fit_needs_relayout();
    assert!(take_fit_needs_relayout());
    assert!(!take_fit_needs_relayout());
    note_fit_needs_relayout();
    assert!(take_fit_needs_relayout());
}

#[test]
fn p1_layout_height_uses_arithmetic_until_fit_cache_hit() {
    invalidate_prepare_cache();
    let _ = take_fit_needs_relayout();
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let avail = 240.0f32;
    let metrics = layout(&prepared, avail);
    // Cache miss still fits once so leaf height matches glyphon line count (avoids
    // gallery label overlap). Subsequent calls must hit FIT_CACHE.
    let h1 = bidi_soft_wrap_height(
        text,
        size,
        500,
        lh,
        avail,
        prepared.max_segment_width,
        &metrics,
        false,
    );
    assert!(
        try_glyphon_fit_soft_wrap(text, size, 500, lh, avail).is_some(),
        "first layout miss must warm FIT_CACHE"
    );
    let fit = try_glyphon_fit_soft_wrap(text, size, 500, lh, avail).unwrap();
    let expect = (lh * fit.line_count.max(1) as f32)
        .max(metrics.height)
        .max(lh);
    assert!(
        (h1 - expect).abs() < 0.01,
        "height must track fit line_count, h={h1} expect={expect}"
    );
    let h2 = bidi_soft_wrap_height(
        text,
        size,
        500,
        lh,
        avail,
        prepared.max_segment_width,
        &metrics,
        false,
    );
    assert!((h2 - h1).abs() < 0.01, "FIT_CACHE hit must be stable");
    shutdown_layout_text();
}

#[test]
fn p1_min_content_width_skips_fit_path() {
    invalidate_prepare_cache();
    let text = "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789";
    let size = 14.0;
    let lh = size * 1.5;
    let prepared = prepare(text, size, 500, lh);
    let metrics = layout(&prepared, prepared.max_segment_width.max(1.0));
    let h = bidi_soft_wrap_height(
        text,
        size,
        500,
        lh,
        prepared.max_segment_width,
        prepared.max_segment_width,
        &metrics,
        false,
    );
    assert!(
        (h - metrics.height.max(lh)).abs() < 0.01,
        "MinContent / max_seg width must stay on arithmetic height"
    );
    assert!(try_glyphon_fit_soft_wrap(text, size, 500, lh, prepared.max_segment_width).is_none());
    shutdown_layout_text();
}

#[test]
fn p2_fit_cache_frame_tick_keeps_touched_entries() {
    invalidate_prepare_cache();
    let _ = glyphon_fit_soft_wrap("p2-gen-keep", 12.0, 400, 18.0, 180.0);
    for _ in 0..3 {
        fit_cache_frame_tick();
        assert!(
            try_glyphon_fit_soft_wrap("p2-gen-keep", 12.0, 400, 18.0, 180.0).is_some(),
            "touch on lookup must refresh generation across ticks"
        );
    }
    shutdown_layout_text();
}

#[test]
fn isolated_emoji_load_preserves_ui_text_layout() {
    let text = "unrelated UI paragraph 123";
    prepare(text, 14.0, 400, 21.0);
    glyphon_fit_soft_wrap(text, 14.0, 400, 21.0, 180.0);
    let prepared = PREPARE_CACHE.with(|c| c.borrow().len());
    let advances = ASCII_ADVANCES.with(|c| c.borrow().len());
    assert!(prepared > 0);
    assert!(try_glyphon_fit_soft_wrap(text, 14.0, 400, 21.0, 180.0).is_some());

    let mut isolated =
        FontSystem::new_with_locale_and_db("en-US".into(), glyphon::fontdb::Database::new());
    font_loader::ensure_emoji_font_fallback(&mut isolated);
    assert_eq!(
        PREPARE_CACHE.with(|c| c.borrow().len()),
        prepared,
        "an isolated emoji database must not invalidate UI segment widths"
    );
    assert_eq!(ASCII_ADVANCES.with(|c| c.borrow().len()), advances);
    assert!(
        try_glyphon_fit_soft_wrap(text, 14.0, 400, 21.0, 180.0).is_some(),
        "an isolated emoji database must not invalidate UI wrapping"
    );
}

#[test]
fn prepare_is_cached() {
    let a = prepare("cache-me", 12.0, 400, 18.0);
    let b = prepare("cache-me", 12.0, 400, 18.0);
    assert_eq!(a.segments.len(), b.segments.len());
    assert!((a.max_segment_width - b.max_segment_width).abs() < f32::EPSILON);
}

#[test]
fn hard_newline_increments_lines() {
    let p = prepare("a\nb\nc", 12.0, 400, 16.0);
    let m = layout(&p, 10_000.0);
    assert_eq!(m.line_count, 3);
}

#[test]
fn layout_caps_pathological_segment_width() {
    let prepared = PreparedText {
        segments: vec![PreparedSegment {
            text: "爆炸".into(),
            width: 16_384.0,
            break_after: false,
        }]
        .into(),
        size: 14.0,
        weight: 400,
        line_height: 21.0,
        max_segment_width: 16_384.0,
    };
    let m = layout(&prepared, 40.0);
    assert!(m.line_count <= 64, "line_count blew up: {}", m.line_count);
    assert!(m.height < 21.0 * 65.0);
}

#[test]
fn cjk_measure_stays_bounded() {
    let text = "中文日文한국어 हिन्दी ไทย";
    let (w, h) = measure_leaf(text, 16.0, 400, 24.0, None, Some(320.0), 0.0, false);
    assert!(w.is_finite() && h.is_finite(), "non-finite {w}x{h}");
    assert!(h < 24.0 * 64.0, "height too large: {h}");
}

#[test]
fn layout_lines_matches_line_count_for_latin_wrap() {
    let p = prepare("hello world from jadix text measure", 14.0, 400, 14.0 * 1.5);
    let narrow = layout(&p, 80.0);
    let lines = layout_lines(&p, 80.0);
    assert_eq!(lines.len() as u32, narrow.line_count);
    assert!(lines.iter().all(|l| !l.text.is_empty() || l.width == 0.0));
}

#[test]
fn layout_lines_hard_newline() {
    let p = prepare("a\nb\nc", 12.0, 400, 16.0);
    let lines = layout_lines(&p, 10_000.0);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].text, "a");
    assert_eq!(lines[1].text, "b");
    assert_eq!(lines[2].text, "c");
}

#[test]
fn prepare_rich_chip_wider_than_plain() {
    let text = "HelloChip";
    let pad = 6.0;
    let plain = prepare(text, 14.0, 400, 21.0);
    let rich = prepare_rich(
        &[crate::style::TextSpan::new(text).chip().padding_x(pad)],
        14.0,
        400,
        21.0,
    );
    assert_eq!(rich.segment_count(), 1);
    assert!(
        rich.max_segment_width > plain.max_segment_width + pad,
        "chip width {} should exceed plain {} by ~2*pad",
        rich.max_segment_width,
        plain.max_segment_width
    );
    let expected_extra = 2.0 * pad;
    assert!(
        (rich.max_segment_width - plain.max_segment_width - expected_extra).abs() < 0.5
            || rich.max_segment_width >= plain.max_segment_width + expected_extra - 0.5,
        "expected +{expected_extra} padding, plain={} chip={}",
        plain.max_segment_width,
        rich.max_segment_width
    );
}

#[test]
fn prepare_rich_chip_is_atomic_under_narrow_layout() {
    let text = "Hello World Chip";
    let plain = prepare(text, 14.0, 400, 21.0);
    assert!(
        plain.segment_count() > 1,
        "plain should split on spaces, got {}",
        plain.segment_count()
    );
    let rich = prepare_rich(
        &[crate::style::TextSpan::new(text).chip().padding_x(8.0)],
        14.0,
        400,
        21.0,
    );
    assert_eq!(rich.segment_count(), 1);
    let lines = layout_lines(&rich, 40.0);
    assert_eq!(lines.len(), 1, "atomic chip must stay on one logical line");
    assert_eq!(lines[0].text, text);
    // Mid-segment split would produce a line whose text is a proper prefix of the chip.
    assert!(
        !lines
            .iter()
            .any(|l| text.starts_with(&l.text) && l.text != text && !l.text.is_empty()),
        "chip must not split mid-segment"
    );
}

#[test]
fn prepare_rich_text_then_chip_wraps_after_break_opportunity() {
    // "Hi " (word + space break_after) + chip — narrow width forces wrap after the space.
    let spans = [
        crate::style::TextSpan::new("Hi "),
        crate::style::TextSpan::new("ChipLabel")
            .chip()
            .padding_x(4.0),
    ];
    let rich = prepare_rich(&spans, 14.0, 400, 21.0);
    assert!(
        rich.segments.len() >= 2,
        "expected text segments + chip, got {}",
        rich.segments.len()
    );
    assert!(
        rich.segments.last().is_some_and(|s| s.break_after),
        "chip must advertise break_after"
    );
    let chip_w = rich.segments.last().unwrap().width;
    // Width just under "Hi "+chip so wrap must occur at the space opportunity.
    let avail = chip_w + 1.0;
    let lines = layout_lines(&rich, avail);
    assert!(
        lines.len() >= 2,
        "expected wrap before chip, got {} lines (avail={avail})",
        lines.len()
    );
    assert!(
        lines.last().is_some_and(|l| l.text.contains("ChipLabel")),
        "chip should sit on its own line after wrap"
    );
}

#[test]
fn shared_font_system_measure_works() {
    let mut render_fs = font_loader::create_font_system();
    font_loader::apply_default_families(render_fs.db_mut());
    install_shared_font_system(&mut render_fs);
    let p = prepare("shared-fs-probe", 14.0, 400, 21.0);
    assert!(p.max_segment_width > 0.0);
    let m = layout(&p, 10_000.0);
    assert_eq!(m.line_count, 1);
    clear_shared_font_system();
    // After clear, measure falls back to owned FontSystem.
    let p2 = prepare("owned-fs-probe", 14.0, 400, 21.0);
    assert!(p2.max_segment_width > 0.0);
    shutdown_layout_text();
}

#[test]
fn diagnose_gallery_caption_last_letter_spill() {
    let size = 0.65 * 16.0; // gallery text_size="0.65rem"
    let lh = size; // Label leading-none
    let weight = 500u16;
    for text in [
        "sun", "smile", "activity", "download", "heart", "bell", "check", "settings",
    ] {
        let p = prepare(text, size, weight, lh);
        let raw = intrinsic_width(&p);
        let (mw, mh) = measure_leaf(text, size, weight, lh, None, None, 0.0, false);
        assert!(
            mw >= raw - 1e-3,
            "{text}: measured width {mw} must cover raw intrinsic {raw}"
        );
        assert!(
            (mh - lh).abs() < 0.5,
            "{text}: single-word caption must stay one line, h={mh} lh={lh}"
        );
        // Painted at the snapped measure width — and at floor(raw) after snap — must
        // not last-letter wrap (gallery "su"/"n" / "activit"/"y").
        let n_meas = glyphon_soft_wrap_line_count(text, size, weight, lh, mw);
        let n_floor_raw = glyphon_soft_wrap_line_count(text, size, weight, lh, raw.floor());
        assert_eq!(
            n_meas, 1,
            "{text}: glyphon wraps at measured {mw} (raw={raw})"
        );
        // After snap, callers size to ceil(raw); floor(raw) may still wrap — that's OK
        // as long as measure reports the ceiled width.
        assert!(
            mw >= snap_content_text_width(raw) - 1e-3,
            "{text}: measure {mw} should be snap(raw)={}",
            snap_content_text_width(raw)
        );
        let _ = n_floor_raw;
    }
}

#[test]
fn snap_content_text_width_ceils_fractional_intrinsic() {
    assert_eq!(snap_content_text_width(16.621), 17.0);
    assert_eq!(snap_content_text_width(33.0), 33.0);
    assert_eq!(snap_content_text_width(0.2), 1.0);
}

#[test]
fn leading_none_cjk_stays_one_line_at_measured_width() {
    // Gallery sidebar Labels: `text-sm font-medium leading-none` ("布局").
    let size = 14.0;
    let lh = size;
    let text = "布局";
    let p = prepare(text, size, 500, lh);
    let mw = snap_content_text_width(intrinsic_width(&p));
    assert_eq!(
        glyphon_soft_wrap_line_count(text, size, 500, lh, mw),
        1,
        "CJK caption must not wrap at measured width {mw}"
    );
}

#[test]
fn gallery_sidebar_cjk_labels_stay_one_line_under_fast_wrap() {
    // Scroll chase force_fast snap-to-8px must not wrap a 2-glyph caption
    // (user report: only 「数据」 strobes on downward chase).
    let size = 14.0;
    let lh = size;
    let labels = [
        "基础",
        "指针事件",
        "表单",
        "浮层 / 反馈",
        "数据",
        "绘制",
        "导航",
        "布局",
        "文本",
        "关于",
    ];
    for text in labels {
        let p = prepare(text, size, 500, lh);
        let mw = snap_content_text_width(intrinsic_width(&p));
        let fast = layout_wrap_target(mw, true);
        let settle_lines = glyphon_soft_wrap_line_count(text, size, 500, lh, mw);
        let fast_lines = glyphon_soft_wrap_line_count(text, size, 500, lh, fast);
        assert_eq!(
            settle_lines, 1,
            "{text:?} must stay one line at measured width {mw}"
        );
        assert_eq!(
            fast_lines, settle_lines,
            "{text:?} fast wrap {fast} must not add lines vs settle {mw}"
        );
    }
}
#[test]
fn cloned_prepared_text_shares_immutable_segments() {
    let prepared = super::prepare(
        "retained text copies share each measured word",
        16.0,
        400,
        24.0,
    );
    let copy = prepared.clone();
    assert!(std::sync::Arc::ptr_eq(&prepared.segments, &copy.segments));
    assert_eq!(
        super::intrinsic_width(&prepared),
        super::intrinsic_width(&copy)
    );
    assert_eq!(super::layout(&prepared, 150.0), super::layout(&copy, 150.0));
}
