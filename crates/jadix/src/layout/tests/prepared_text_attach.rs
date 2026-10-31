use super::*;
use crate::style::{InputCaretDraw, TextOverflow, TextStyle};

#[test]
fn height_animation_reuses_unchanged_text_measures() {
    let mut rows = vec![Element::new().h(20.0)];
    rows.extend((0..80).map(|i| {
        Element::new()
            .text(format!("row {i}: unchanged wrapped content"))
            .w("100%")
    }));
    let mut root = Element::from(crate::primitives::vstack().children(rows));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 360.0, 600.0, [0.0, 0.0]);
    // Warm both flex probe variants before checking subsequent animation frames.
    for h in [22.0, 24.0, 26.0] {
        root.children[0].style.size.height = Dimension::length(h);
        tree.mark_element_layout_dirty(&root.children[0]);
        for row in &root.children[1..] {
            tree.mark_element_layout_dirty(row);
        }
        tree.build_and_layout_fast(&mut root, 360.0, 600.0, [0.0, 0.0], false, false);
    }
    assert!(
        tree.text_measure_calls.get() > 0,
        "fixture must exercise Taffy's text probes during relayout"
    );
    assert_eq!(
        tree.text_measure_misses.get(),
        0,
        "changing a preceding row's height must reuse text sizes at unchanged inline widths"
    );
    root.children[1].text = Some("changed content that must be measured again".into());
    tree.mark_element_layout_dirty(&root.children[1]);
    tree.build_and_layout_fast(&mut root, 360.0, 600.0, [0.0, 0.0], false, false);
    assert!(
        tree.text_measure_misses.get() > 0,
        "text edits must invalidate measurement"
    );
    tree.build_and_layout_fast(&mut root, 200.0, 600.0, [0.0, 0.0], false, false);
    assert!(
        tree.text_measure_misses.get() > 0,
        "inline width changes must be measured"
    );
    tree.mark_all_text_leaves_dirty(&root);
    tree.build_and_layout_fast(&mut root, 200.0, 600.0, [0.0, 0.0], false, false);
    assert!(
        tree.text_measure_misses.get() > 0,
        "paint-time font fitting must force new measures"
    );
}

#[test]
fn paint_style_refresh_preserves_taffy_cache_but_geometry_still_invalidates() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .flex_col()
        .child(Element::new().w(100.0).h(40.0));
    tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let node = root.children[0].layout_node.unwrap();
    root.children[0].style.bg_color = [0.2, 0.4, 0.6, 1.0];
    tree.update_taffy_node_style(&root.children[0]);
    assert!(
        !tree.is_node_dirty(node),
        "paint-only refresh must preserve layout"
    );
    root.children[0].style.size.height = Dimension::length(80.0);
    tree.update_taffy_node_style(&root.children[0]);
    assert!(
        tree.is_node_dirty(node),
        "real geometry must invalidate the cache"
    );
    tree.build_and_layout_fast(&mut root, 200.0, 200.0, [0.0, 0.0], false, false);
    assert_eq!(root.children[0].layout_bounds[3], 80.0);
}

#[test]
fn text_baselines_skip_zero_metrics() {
    let mut meta = text_measure_meta(&Element::new().text_size(0.0), "zero".into(), false);
    assert!(measured_text_baselines(&meta, 100.0).first.is_none());
    meta.size = 16.0;
    meta.line_height_px = Some(0.0);
    assert!(measured_text_baselines(&meta, 100.0).first.is_none());
}

#[test]
fn plain_text_skips_unused_baseline_shaping_and_alignment_changes_remeasure() {
    crate::text::measure::with_layout_font_system(|fonts| {
        fonts.db_mut().load_font_data(
            include_bytes!("../../../../../crates/jadix_cosmic_text/fonts/FiraMono-Medium.ttf")
                .to_vec(),
        );
    });
    let mut root = Element::new()
        .flex_row()
        .child(Element::new().text("small baseline").text_size(16.0))
        .child(Element::new().text("large baseline").text_size(32.0));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 160.0, [0.0, 0.0]);
    assert!(!tree.measure_layout_cache.borrow().is_empty());
    assert!(
        tree.measure_layout_cache
            .borrow()
            .values()
            .all(|value| { value.baselines.first.is_none() && value.baselines.last.is_none() }),
        "ordinary text must not shape an unused font baseline"
    );
    root.style.align_items = Some(taffy::AlignItems::BASELINE);
    tree.mark_element_layout_dirty(&root);
    tree.build_and_layout_fast(&mut root, 400.0, 160.0, [0.0, 0.0], false, false);
    assert!(
        tree.measure_layout_cache
            .borrow()
            .values()
            .any(|value| { value.baselines.first.is_some() && value.baselines.last.is_some() }),
        "adding baseline alignment must invalidate cached leaf layouts"
    );
}

#[test]
fn unused_text_baselines_do_not_resolve_box_edges() {
    let resolutions = std::cell::Cell::new(0);
    let empty = offset_text_baselines(taffy::tree::Baselines::NONE, || {
        resolutions.set(resolutions.get() + 1);
        3.0
    });
    assert_eq!(empty, taffy::tree::Baselines::NONE);
    assert_eq!(
        resolutions.get(),
        0,
        "unused baselines must not resolve box edges"
    );
    let shifted = offset_text_baselines(
        taffy::tree::Baselines {
            first: Some(10.0),
            last: Some(20.0),
        },
        || {
            resolutions.set(resolutions.get() + 1);
            3.0
        },
    );
    assert_eq!(shifted.first, Some(13.0));
    assert_eq!(shifted.last, Some(23.0));
    let last_only = offset_text_baselines(
        taffy::tree::Baselines {
            first: None,
            last: Some(4.0),
        },
        || {
            resolutions.set(resolutions.get() + 1);
            3.0
        },
    );
    assert_eq!(last_only.first, None);
    assert_eq!(last_only.last, Some(7.0));
    assert_eq!(resolutions.get(), 2);
}

#[test]
fn text_measure_cache_ignores_available_width_when_inline_size_is_known() {
    let mut tree = LayoutTree::new();
    let mut leaf = Element::new().text("known inline width wraps consistently");
    let node = tree.taffy.new_leaf(taffy::Style::default()).unwrap();
    leaf.layout_node = Some(node);
    tree.sync_text_measure(&leaf);
    let known = Size {
        width: Some(96.0),
        height: None,
    };
    let mut first = None;
    for width in [
        AvailableSpace::MinContent,
        AvailableSpace::MaxContent,
        AvailableSpace::Definite(240.0),
    ] {
        let result = measure_leaf_cached(
            &tree.measure_layout_cache,
            &tree.text_measurements,
            node,
            known,
            Size {
                width,
                height: AvailableSpace::MaxContent,
            },
            None,
        );
        assert_eq!(*first.get_or_insert(result), result);
    }
    assert_eq!(
        tree.measure_layout_cache.borrow().len(),
        1,
        "the same known inline size must not reshape for each flex sizing phase"
    );
}

#[test]
fn text_measure_cache_keeps_icon_and_vertical_constraints() {
    let mut tree = LayoutTree::new();
    let mut icon = Element::new().text("icon");
    icon.text_style.font_family = Some("lucide".into());
    let node = tree.taffy.new_leaf(taffy::Style::default()).unwrap();
    icon.layout_node = Some(node);
    tree.sync_text_measure(&icon);
    let avail = Size {
        width: AvailableSpace::MaxContent,
        height: AvailableSpace::MaxContent,
    };
    for h in [18.0, 32.0] {
        let size = measure_leaf_cached(
            &tree.measure_layout_cache,
            &tree.text_measurements,
            node,
            Size {
                width: None,
                height: Some(h),
            },
            avail,
            None,
        );
        assert_eq!(
            size.height, h,
            "icon dimensions cannot alias text cache entries"
        );
    }
    icon.text_style.font_family = None;
    icon.style.writing_mode = crate::style::WritingMode::VerticalRl;
    tree.sync_text_measure(&icon);
    let wide = measure_leaf_cached(
        &tree.measure_layout_cache,
        &tree.text_measurements,
        node,
        Size {
            width: None,
            height: Some(100.0),
        },
        avail,
        None,
    );
    let narrow = measure_leaf_cached(
        &tree.measure_layout_cache,
        &tree.text_measurements,
        node,
        Size {
            width: None,
            height: Some(20.0),
        },
        avail,
        None,
    );
    assert_ne!(
        wide, narrow,
        "vertical text uses height as its inline constraint"
    );
}

#[test]
fn prepared_text_on_text_layout_info_roundtrips() {
    let prepared = crate::text::prepare("hello prepared attach", 14.0, 400, 21.0);
    let info = TextLayoutInfo {
        text: "hello prepared attach".into(),
        shape_suffix: None,
        spans: None,
        template_id: Some("t-prepared".into()),
        x: 0.0,
        y: 0.0,
        width: 120.0,
        height: 42.0,
        clip_bounds: None,
        style: TextStyle::default(),
        prepared: Some(prepared.clone()),
        glyph_fx: None,
        shimmer: None,
        paint_scale: 1.0,
        paint_transform: crate::style::MAT2D_IDENTITY,
        paint_origin: [0.0, 0.0],
        glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
    };
    let attached = info.prepared.as_ref().expect("prepared attached");
    assert_eq!(attached.segment_count(), prepared.segment_count());
    assert!((attached.max_segment_width - prepared.max_segment_width).abs() < f32::EPSILON);
    let metrics = crate::text::layout(attached, info.width);
    assert!(metrics.line_count >= 1);
    let lines = crate::text::layout_lines(attached, info.width);
    assert_eq!(lines.len() as u32, metrics.line_count);
}

#[test]
fn hard_line_slices_keeps_trailing_empty_line() {
    let lines = hard_line_slices("a\n");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1], (2, ""));
}

#[test]
fn latin_cjk_prefix_width_sums_segments_not_max() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);

    let size = 14.0;
    let lh = 21.0;
    let weight = 400u16;
    let mixed = "1是";
    let after_cjk = mixed.len();
    let w_mixed = prefix_width_logical(mixed, after_cjk, size, weight, lh);
    let w_latin = prefix_width_logical("1", 1, size, weight, lh);
    let w_cjk = prefix_width_logical("是", "是".len(), size, weight, lh);
    // Bug used max(segment) ≈ w_cjk, placing caret mid-"是".
    assert!(
        w_mixed > w_cjk + w_latin * 0.5,
        "prefix of \"1是\" must be ~sum, got mixed={w_mixed} latin={w_latin} cjk={w_cjk}"
    );
    assert!(
        (w_mixed - (w_latin + w_cjk)).abs() < 1.0,
        "expected ~latin+cjk, got mixed={w_mixed} vs {w_latin}+{w_cjk}"
    );
}

/// Multi-line selection highlight is glyph-tight per line (no EOL/box fill).
#[test]
fn multiline_selection_rects_are_glyph_tight_per_line() {
    let text = "啊\n啊\n啊";
    let lh = 21.0_f32;
    let box_w = 280.0_f32;
    let glyph_w = prefix_width_logical("啊", "啊".len(), 14.0, 400, lh);
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(lh),
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: Some((0, text.len())),
        selection_bg: [80, 120, 200, 90],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut selection = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        10.0,
        20.0,
        box_w,
        lh * 3.0 + 4.0,
        None,
        1.0,
    );
    assert_eq!(selection.len(), 3);
    for r in &selection {
        assert!(
            (r.rect_size[0] - glyph_w).abs() < 2.0,
            "selected line must stay glyph-tight (no EOL fill), got w={} glyph≈{glyph_w}",
            r.rect_size[0]
        );
    }
}

/// A one-line textarea leaf is `line_height + ink_pad`. Glyphs start at the
/// leaf top; the highlight must too, or the ideograph sits on the bar's top edge.
#[test]
fn single_line_textarea_selection_tracks_glyph_top() {
    let text = "啊啊啊啊啊啊aaaa";
    let lh = 21.0_f32;
    let size = 14.0_f32;
    let y = 8.0_f32;
    let box_h = lh + crate::utils::text_field::text_edit_ink_pad_px(size);
    let mut style = TextStyle::default();
    style.size = Some(size);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.color = Some([0, 0, 0, 255]);
    style.white_space = crate::style::WhiteSpace::Normal;
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: Some((0, text.len())),
        selection_bg: [0, 0, 0, 255],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut selection = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        4.0,
        y,
        280.0,
        box_h,
        None,
        1.0,
    );
    assert!(!selection.is_empty(), "selection chrome missing");
    let sel_y = selection[0].rect_pos[1];
    let band = crate::utils::text_field::text_edit_cjk_chrome_band_px(lh, size);
    let expected = y + band.map(|(dy, _)| dy).unwrap_or(0.0);
    let centered = y + (box_h - lh) * 0.5;
    assert!(
            (sel_y - expected).abs() < 0.51,
            "selection must share the glyph line top (y={sel_y}, expected={expected}, centered={centered})"
        );
    assert!(
        (expected - centered).abs() > 0.5,
        "fixture must be taller than one line so centering would be visible"
    );
}

/// RTL paragraph: selecting the trailing LTR island must highlight the visual
/// left cluster, not a logical-prefix box that covers the Hebrew run.
#[test]
fn bidi_selection_rects_follow_visual_runs() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);

    let text = "עברית Hello";
    let Some(lo) = text.find("Hello") else {
        return;
    };
    let hi = lo + "Hello".len();
    let hebrew_prefix = prefix_width_logical(&text[..lo], lo, 14.0, 400, 21.0);
    let lh = 21.0_f32;
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.color = Some([0, 0, 0, 255]);
    style.white_space = crate::style::WhiteSpace::Normal;
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: hi,
        selection: Some((lo, hi)),
        selection_bg: [0, 0, 0, 255],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: Some([255, 255, 255, 255]),
    });

    let mut selection = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        0.0,
        0.0,
        280.0,
        lh + 4.0,
        None,
        1.0,
    );
    assert!(
        !selection.is_empty(),
        "BiDi selection must emit at least one visual slab"
    );
    let x = selection[0].rect_pos[0];
    assert!(
            x > hebrew_prefix + 1.0,
            "Hello glyphs are past the logical Hebrew prefix (LTR box would cover Hebrew); x={x} hebrew_prefix={hebrew_prefix}"
        );
}

#[test]
fn preedit_underline_sits_below_glyphs_and_caret_still_drawn() {
    let text = "你好hao";
    let lh = 21.0_f32;
    let y = 20.0_f32;
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(lh),
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: None,
        selection_bg: [0; 4],
        overwrite_highlight: None,
        preedit_underline: Some((6, 9)),
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });
    let mut underline = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut underline,
        &mut chrome,
        text,
        &style,
        10.0,
        y,
        280.0,
        lh,
        None,
        1.0,
    );
    assert_eq!(underline.len(), 1, "one underline band for the preedit run");
    let ul = &underline[0];
    assert!(
        ul.rect_pos[1] >= y + 14.0 + 2.5,
        "underline must sit below glyphs with a gap (y={}, size=14, got {})",
        y,
        ul.rect_pos[1]
    );
    assert!(
        ul.rect_size[1] < 2.5,
        "preedit underline is a hairline, got h={}",
        ul.rect_size[1]
    );
    assert!(
        !chrome.is_empty(),
        "caret hairline must still draw while composing"
    );
}

#[test]
fn extra_carets_emit_additional_hairlines() {
    let text = "hello";
    let lh = 21.0_f32;
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(lh),
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: 1,
        selection: None,
        extra_carets: vec![3, 5],
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        selection_bg: [0; 4],
        overwrite_highlight: None,
        preedit_underline: None,
        drop_caret: true,
        caret_color: None,
        selection_foreground: None,
    });
    let mut selection = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        10.0,
        20.0,
        280.0,
        lh,
        None,
        1.0,
    );
    assert_eq!(
        chrome.len(),
        3,
        "primary + two extra carets, got {}",
        chrome.len()
    );
}

#[test]
fn soft_wrap_caret_sits_on_visual_line_not_box_center() {
    // One hard line soft-wrapped into several visual rows: caret must track the
    // visual line (not vertically center in the tall leaf — gallery symptom).
    let text = "padding: 8px 16px; background: #ffffff; border-width: 1px; border-radius: 6px";
    let lh = 21.0_f32;
    let box_w = 160.0_f32;
    let ranges = crate::text::soft_visual_line_byte_ranges(
        text,
        14.0,
        400,
        lh,
        box_w,
        crate::text::LineBreakSource::SoftWord,
    );
    assert!(
        ranges.len() >= 2,
        "fixture must soft-wrap under {box_w}px, got {} lines",
        ranges.len()
    );
    let caret_byte = text.len();
    let visual_i = crate::text::caret_visual_line_index(&ranges, caret_byte);
    let expected_y = visual_i as f32 * lh; // multi-line origin (no half-box center)

    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.white_space = crate::style::WhiteSpace::Normal;
    style.color = Some([0, 0, 0, 255]);
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: caret_byte,
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let tall_h = lh * ranges.len() as f32 + 8.0;
    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    let mut saw = false;
    for _ in 0..8 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            0.0,
            0.0,
            box_w,
            tall_h,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
    }
    assert!(saw, "caret must draw within blink window");
    let caret_y = chrome[0].rect_pos[1];
    let caret_x = chrome[0].rect_pos[0];
    let centered = (tall_h - lh).max(0.0) * 0.5;
    assert!(
        (caret_y - expected_y).abs() < lh * 0.6,
        "soft-wrap caret y={caret_y} should sit near visual line {visual_i} (≈{expected_y}), \
             not box-center {centered}"
    );
    assert!(
        (caret_y - centered).abs() > lh * 0.4 || visual_i == ranges.len() / 2,
        "must not use single-line box centering for soft-wrapped tall leaf \
             (y={caret_y} center={centered})"
    );
    let (s, e) = ranges[visual_i];
    let body = &text[s..e];
    let full_w = prefix_width_logical(body, body.len(), 14.0, 400, lh);
    assert!(
        (caret_x - full_w).abs() < 3.0,
        "caret x={caret_x} should sit at end of visual line (prefix≈{full_w}), not mid-glyph"
    );
}

#[test]
fn wrap_char_caret_x_uses_soft_char_not_soft_word() {
    // Gallery wrap=char sample: SoftChar mid-breaks the long token; SoftWord does not.
    // Chrome must use wrap_mode so the caret before 😃 lands on the SoftChar visual line.
    let text = "Supercalifragilisticexpialidocious_unbreakable_token_plus_中文emoji😃 —";
    let lh = 21.0_f32;
    let box_w = 280.0_f32;
    let size = 14.0_f32;
    let emoji = text.find('😃').expect("emoji");
    let soft_char = crate::text::soft_visual_line_byte_ranges(
        text,
        size,
        400,
        lh,
        box_w,
        crate::text::LineBreakSource::SoftChar,
    );
    let soft_word = crate::text::soft_visual_line_byte_ranges(
        text,
        size,
        400,
        lh,
        box_w,
        crate::text::LineBreakSource::SoftWord,
    );
    assert_ne!(
        soft_char, soft_word,
        "fixture requires SoftChar ≠ SoftWord ranges"
    );
    let ci = crate::text::caret_visual_line_index(&soft_char, emoji);
    let (cs, _) = soft_char[ci];
    let expected_x = prefix_width_logical(&text[cs..emoji], emoji - cs, size, 400, lh);

    let mut style = TextStyle::default();
    style.size = Some(size);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.white_space = crate::style::WhiteSpace::Normal;
    style.wrap_mode = crate::text::WrapMode::Char;
    style.color = Some([0, 0, 0, 255]);
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: emoji,
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    let mut saw = false;
    for _ in 0..8 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            0.0,
            0.0,
            box_w,
            lh * soft_char.len() as f32 + 8.0,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
    }
    assert!(saw, "wrap=char caret must draw within blink window");
    let caret_x = chrome[0].rect_pos[0];
    let caret_y = chrome[0].rect_pos[1];
    let expected_y = ci as f32 * lh + (lh - chrome[0].rect_size[1]) * 0.5;
    assert!(
        (caret_y - expected_y).abs() < lh * 0.6,
        "wrap=char caret y={caret_y} should sit on SoftChar visual line {ci} (≈{expected_y})"
    );
    assert!(
        (caret_x - expected_x).abs() < 3.0,
        "wrap=char caret x={caret_x} must match SoftChar prefix≈{expected_x} (not SoftWord)"
    );
}

#[test]
fn multiline_caret_slightly_taller_than_line_and_centered() {
    let text = "asd\nadj";
    let lh = 21.0_f32;
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(lh),
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    let mut saw = false;
    for _ in 0..6 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            0.0,
            0.0,
            200.0,
            lh * 2.0,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
    }
    assert!(saw, "multiline caret chrome missing during blink ON");
    let caret_h = chrome[0].rect_size[1];
    let caret_y = chrome[0].rect_pos[1];
    assert!(
        caret_h > lh && caret_h <= lh + 2.0,
        "caret should be slightly taller than line height (h={caret_h}, lh={lh})"
    );
    let expected_y = lh + (lh - caret_h) * 0.5;
    assert!(
        (caret_y - expected_y).abs() < 0.5,
        "caret should be vertically centered on line 1 (y={caret_y}, expected={expected_y})"
    );
}

#[test]
fn nowrap_textarea_selection_highlight_follows_viewport_scroll() {
    let text = "abcdefghijklmnop";
    let lh = 21.0_f32;
    let prefix_mid = prefix_width_logical(text, 4, 14.0, 400, lh);
    let prefix_end = prefix_width_logical(text, 8, 14.0, 400, lh);
    let scroll_x = 80.0_f32;
    let layout_x = 20.0_f32;
    let text_x = layout_x - scroll_x;

    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.color = Some([0, 0, 0, 255]);
    style.white_space = crate::style::WhiteSpace::Nowrap;
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: 8,
        selection: Some((0, 8)),
        selection_bg: [80, 120, 200, 90],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut selection = Vec::new();
    let mut chrome = Vec::new();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        text_x,
        0.0,
        800.0,
        lh,
        None,
        1.0,
    );
    assert_eq!(selection.len(), 1);
    assert!(
        (selection[0].rect_pos[0] - text_x).abs() < 1.5,
        "selection start must track panned text origin"
    );
    assert!(
        (selection[0].rect_size[0] - prefix_end).abs() < 2.0,
        "selection width must stay glyph-tight"
    );

    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: 8,
        selection: Some((4, 8)),
        selection_bg: [80, 120, 200, 90],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });
    selection.clear();
    push_input_chrome_instances(
        &mut selection,
        &mut chrome,
        text,
        &style,
        text_x,
        0.0,
        800.0,
        lh,
        None,
        1.0,
    );
    assert_eq!(selection.len(), 1);
    assert!(
        (selection[0].rect_pos[0] - (text_x + prefix_mid)).abs() < 1.5,
        "mid-line selection must follow sx (x={} expected={})",
        selection[0].rect_pos[0],
        text_x + prefix_mid
    );
}

#[test]
fn hidden_overflow_caret_follows_scroll_not_box_clamp() {
    let text = "abcdefghijklmnop";
    let lh = 21.0_f32;
    let prefix = prefix_width_logical(text, text.len(), 14.0, 400, lh);
    let box_w = 40.0_f32;
    assert!(
        prefix > box_w + 8.0,
        "fixture must overflow the box (prefix={prefix})"
    );
    let scroll_x = (prefix - box_w + 2.0).max(0.0);
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(lh),
        color: Some([0, 0, 0, 255]),
        overflow: TextOverflow::Hidden,
        scroll_x,
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let origin = 10.0 - scroll_x;
    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    let mut saw = false;
    for _ in 0..8 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            origin,
            0.0,
            box_w,
            lh,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
    }
    assert!(saw, "hidden-overflow caret missing during blink ON");
    let caret_x = chrome[0].rect_pos[0];
    assert!(
        (caret_x - (origin + prefix)).abs() < 1.5,
        "caret must sit at content end minus scroll (x={caret_x} expected={})",
        origin + prefix
    );
    assert!(
        (caret_x - (10.0 + box_w - 2.0)).abs() < 2.0,
        "scrolled caret should land near the visible right edge, got {caret_x}"
    );
}

#[test]
fn center_align_caret_offsets_into_box() {
    let text = "42";
    let lh = 21.0_f32;
    let size = 14.0_f32;
    let line_w = prefix_width_logical(text, text.len(), size, 400, lh);
    let box_w = 80.0_f32;
    assert!(
        line_w + 8.0 < box_w,
        "fixture must fit so center offset applies (line_w={line_w})"
    );
    let mut style = TextStyle::default();
    style.size = Some(size);
    style.weight = Some(400);
    style.line_height_px = Some(lh);
    style.color = Some([0, 0, 0, 255]);
    style.overflow = TextOverflow::Hidden;
    style.align = Some(crate::style::TextAlign::Center);
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: text.len(),
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });
    let origin = 10.0_f32;
    let want_dx = style.align_inline_offset(line_w, box_w);
    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    let mut saw = false;
    for _ in 0..8 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            origin,
            0.0,
            box_w,
            lh,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
    assert!(saw, "center-align caret missing during blink ON");
    let caret_x = chrome[0].rect_pos[0];
    let expected = origin + want_dx + line_w;
    assert!(
        (caret_x - expected).abs() < 1.5,
        "centered caret should sit after ink (x={caret_x}, expected={expected}, dx={want_dx})"
    );
    assert!(
        want_dx > 4.0,
        "center offset should be material for this fixture (dx={want_dx})"
    );
}

#[test]
fn clip_to_box_nowrap_does_not_grow_past_known_width() {
    let text = "abcdefghijklmnopabcdefghijklmnop";
    let size = 14.0;
    let lh = 21.0;
    let prepared = crate::text::prepare(text, size, 400, lh);
    let intrinsic = crate::text::intrinsic_width(&prepared);
    assert!(intrinsic > 80.0, "fixture intrinsic={intrinsic}");
    let m = TextMeasureMeta {
        size,
        font_family: None,
        text: text.to_string(),
        shape_suffix: None,
        is_icon: false,
        line_height_px: Some(lh),
        line_height_scale: None,
        weight: 400,
        letter_spacing: 0.0,
        spans: None,
        nowrap: true,
        clip_to_box: true,
        caret_scroll: false,
        writing_mode: crate::style::WritingMode::HorizontalTb,
    };
    let known = taffy::geometry::Size {
        width: Some(64.0),
        height: None,
    };
    let avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::Definite(64.0),
        height: taffy::style::AvailableSpace::MaxContent,
    };
    let out = measured_text_leaf_size(&m, known, avail);
    assert!(
        (out.width - 64.0).abs() < 0.5,
        "clip-to-box leaf must stay at known width, got {}",
        out.width
    );
    assert!(
        (out.height - lh).abs() < 1.0,
        "must stay one line h={}",
        out.height
    );
}

#[test]
fn caret_scroll_max_content_ignores_placeholder_intrinsic() {
    let text = "Placeholder…";
    let size = 14.0;
    let lh = 21.0;
    let prepared = crate::text::prepare(text, size, 400, lh);
    let intrinsic = crate::text::intrinsic_width(&prepared);
    assert!(intrinsic > 40.0, "fixture intrinsic={intrinsic}");
    let m = TextMeasureMeta {
        size,
        font_family: None,
        text: text.to_string(),
        shape_suffix: None,
        is_icon: false,
        line_height_px: Some(lh),
        line_height_scale: None,
        weight: 400,
        letter_spacing: 0.0,
        spans: None,
        nowrap: true,
        clip_to_box: true,
        caret_scroll: true,
        writing_mode: crate::style::WritingMode::HorizontalTb,
    };
    let known = taffy::geometry::Size {
        width: None,
        height: None,
    };
    let avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::MaxContent,
        height: taffy::style::AvailableSpace::MaxContent,
    };
    let out = measured_text_leaf_size(&m, known, avail);
    assert!(
        out.width < 1.0,
        "native <input> max-content must not follow placeholder glyphs ({intrinsic}), got {}",
        out.width
    );
    let min_avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::MinContent,
        height: taffy::style::AvailableSpace::MinContent,
    };
    let min_out = measured_text_leaf_size(&m, known, min_avail);
    assert!(
        min_out.width < 1.0,
        "native <input> min-content must not follow placeholder glyphs, got {}",
        min_out.width
    );
}

#[test]
fn nowrap_without_clip_to_box_still_grows_to_intrinsic() {
    let text = "abcdefghijklmnopabcdefghijklmnop";
    let size = 14.0;
    let lh = 21.0;
    let prepared = crate::text::prepare(text, size, 400, lh);
    let intrinsic = crate::text::intrinsic_width(&prepared);
    let m = TextMeasureMeta {
        size,
        font_family: None,
        text: text.to_string(),
        shape_suffix: None,
        is_icon: false,
        line_height_px: Some(lh),
        line_height_scale: None,
        weight: 400,
        letter_spacing: 0.0,
        spans: None,
        nowrap: true,
        clip_to_box: false,
        caret_scroll: false,
        writing_mode: crate::style::WritingMode::HorizontalTb,
    };
    let known = taffy::geometry::Size {
        width: Some(64.0),
        height: None,
    };
    let avail = taffy::geometry::Size {
        width: taffy::style::AvailableSpace::Definite(64.0),
        height: taffy::style::AvailableSpace::MaxContent,
    };
    let out = measured_text_leaf_size(&m, known, avail);
    assert!(
        (out.width - intrinsic).abs() < 0.5,
        "button nowrap should still grow to intrinsic {intrinsic}, got {}",
        out.width
    );
}

#[test]
fn hidden_input_clip_inflates_into_padding_inside_chrome() {
    let leaf = [20.0, 10.0, 80.0, 20.0];
    let chrome = Some([10.0, 0.0, 120.0, 40.0]);
    let c = hidden_input_text_clip(chrome, leaf).unwrap();
    assert!(
        (c[0] - 18.0).abs() < 0.01 && (c[2] - 84.0).abs() < 0.01,
        "content box + 2px ink, capped to chrome, got {c:?}"
    );
    let tight = Some([20.0, 10.0, 80.0, 20.0]);
    let inner = hidden_input_text_clip(tight, leaf).unwrap();
    assert!(
        (inner[2] - 80.0).abs() < 0.01,
        "cannot inflate past a content-sized parent clip, got {inner:?}"
    );
}

#[test]
fn caret_after_cjk_in_latin_cjk_latin_is_past_glyph() {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);

    let text = "1是1";
    // Caret after "是" (IME replace middle of "1111").
    let caret_byte = "1是".len();
    let mut style = TextStyle {
        size: Some(14.0),
        weight: Some(400),
        line_height_px: Some(21.0),
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    style.input_caret = Some(InputCaretDraw {
        caret_byte_index: caret_byte,
        selection: None,
        selection_bg: [0, 0, 0, 0],
        overwrite_highlight: None,
        preedit_underline: None,
        extra_carets: Vec::new(),
        extra_selections: Vec::new(),
        extra_preedit_underlines: Vec::new(),
        preedit_ul: Vec::new(),
        drop_caret: false,
        caret_color: None,
        selection_foreground: None,
    });

    let mut chrome = Vec::new();
    let mut unused = Vec::new();
    // Force blink-ON by retrying a few phases if needed.
    let mut saw = false;
    for _ in 0..6 {
        chrome.clear();
        push_input_chrome_instances(
            &mut unused,
            &mut chrome,
            text,
            &style,
            0.0,
            0.0,
            200.0,
            32.0,
            None,
            1.0,
        );
        if !chrome.is_empty() {
            saw = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(280));
    }
    assert!(saw, "caret chrome missing during blink ON");
    let caret_x = chrome[0].rect_pos[0];
    let after_cjk = prefix_width_logical(text, caret_byte, 14.0, 400, 21.0);
    let only_cjk = prefix_width_logical("是", "是".len(), 14.0, 400, 21.0);
    assert!(
        (caret_x - after_cjk).abs() < 0.5,
        "caret x={caret_x} should match intrinsic prefix={after_cjk}"
    );
    assert!(
        caret_x > only_cjk + 1.0,
        "caret must sit past mid-CJK (only_cjk={only_cjk}, caret_x={caret_x})"
    );
}
