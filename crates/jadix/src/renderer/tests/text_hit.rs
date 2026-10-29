use super::*;
use crate::style::TextOverflow;

fn test_font_system() -> glyphon::FontSystem {
    let mut fs = crate::renderer::font_loader::create_font_system();
    crate::renderer::font_loader::apply_default_families(fs.db_mut());
    crate::text::install_shared_font_system(&mut fs);
    fs
}

#[test]
fn best_font_family_keeps_platform_ui_after_cjk_lazy_load() {
    let mut fs = test_font_system();
    let before = best_font_family(&fs);
    crate::renderer::font_loader::ensure_coverage_for_text(&mut fs, "浮层与反馈中文");
    let after = best_font_family(&fs);
    assert_eq!(
        before, after,
        "lazy CJK must not switch the global UI family (before={before:?}, after={after:?})"
    );
}

#[test]
fn unconstrained_measure_is_wider_than_wrapped_box() {
    let mut fs = test_font_system();
    let family = best_font_family(&fs);
    let text = "aaaaaaaaaaaaaaaaaaaaaaaa";
    let size = 14.0;
    let lh = 21.0;
    let box_w = 40.0;
    let wrapped = measure_prefix_width(&mut fs, text, size, lh, 400, family, box_w);
    let free = measure_prefix_width(&mut fs, text, size, lh, 400, family, UNBOUNDED_WRAP);
    assert!(
        free > wrapped + 8.0,
        "nowrap intrinsic must exceed wrapped line width (free={free} wrapped={wrapped})"
    );
    let out = truncate_with_ellipsis(&mut fs, text, size, lh, 400, family, box_w);
    assert!(out.ends_with(ELLIPSIS), "got {out:?}");
    assert!(out.chars().count() < text.chars().count());
}

#[test]
fn ellipsis_terminates_on_multibyte_scalars() {
    let mut fs = test_font_system();
    let family = best_font_family(&fs);
    // 3-byte UTF-8; the old byte-midpoint search snapped `end` back to `lo` and hung.
    let text = "€€€€€€€€€€";
    let out = truncate_with_ellipsis(&mut fs, text, 14.0, 21.0, 400, family, 12.0);
    assert!(out.ends_with(ELLIPSIS), "got {out:?}");
}

#[test]
fn ellipsis_truncates_latin_phrase() {
    let mut fs = test_font_system();
    let family = best_font_family(&fs);
    let text = "Very long button caption that must not wrap";
    let out = truncate_with_ellipsis(&mut fs, text, 14.0, 21.0, 400, family, 80.0);
    assert!(out.ends_with(ELLIPSIS), "got {out:?}");
    assert_ne!(out, text);
}

#[test]
fn hidden_hit_test_outside_box_reaches_hidden_glyphs() {
    let mut fs = test_font_system();
    let text = "abcdefghijklmnop";
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.overflow = TextOverflow::Hidden;
    style.scroll_x = 40.0;
    let bounds = [10.0, 0.0, 40.0, 21.0];
    // Pointer 20px left of the leaf: should hit earlier than the visible start.
    let left = hit_test_input_text_index(&mut fs, text, -10.0, 10.0, bounds, &style, 1.0);
    let edge = hit_test_input_text_index(&mut fs, text, 10.0, 10.0, bounds, &style, 1.0);
    assert_eq!(left, 0, "click in the left padding must go to line start");
    assert!(
        left < edge,
        "drag left of the box must move caret toward the start (left={left} edge={edge})"
    );
    let right = hit_test_input_text_index(&mut fs, text, 80.0, 10.0, bounds, &style, 1.0);
    let right_edge = hit_test_input_text_index(&mut fs, text, 50.0, 10.0, bounds, &style, 1.0);
    assert_eq!(
        right,
        text.len(),
        "click in the right padding must go to line end"
    );
    assert!(
        right >= right_edge,
        "drag right of the box must move caret toward the end (right={right} edge={right_edge})"
    );
}

#[test]
fn multiline_hard_line_hit_ignores_long_prev_line_width() {
    // Long hard line 0 must not steal Y hits for hard line 1 (devtools CSSOM “串行”).
    let mut fs = test_font_system();
    let line0 = "padding: 8px 16px 8px 16px; background: #ffffff; border-radius: 6px;";
    let line1 = "border-width: 1px;";
    let text = format!("{line0}\n{line1}");
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.overflow = TextOverflow::Clip;
    style.white_space = crate::style::WhiteSpace::Nowrap;
    let bounds = [0.0, 0.0, 80.0, 42.0];
    let ix = hit_test_input_text_index(&mut fs, &text, 1.0, 30.0, bounds, &style, 1.0);
    assert_eq!(
        ix,
        line0.len() + 1,
        "click on visual row 1 must land at start of hard line 1, got {ix}"
    );
    // Soft-wrap of line0 under width 80 would put extra visual rows before line1;
    // hard-line indexing must still map y≈30 to line1 (not mid-line0).
    assert!(
        ix > line0.len(),
        "must not land inside hard line 0 when clicking the second row"
    );
}

#[test]
fn textarea_click_padding_snaps_to_line_start_and_end() {
    let mut fs = test_font_system();
    let text = "abcdefghijklmnop";
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.white_space = crate::style::WhiteSpace::Nowrap;
    style.overflow = TextOverflow::Clip;
    let bounds = [32.0, 8.0, 200.0, 21.0];
    let left = hit_test_textarea_click_caret_index(
        &mut fs, text, 8.0, 16.0, bounds, &style, 1.0, 32.0, 16.0,
    );
    assert_eq!(left, 0, "scrollport left padding must snap to line start");
    let right = hit_test_textarea_click_caret_index(
        &mut fs, text, 250.0, 16.0, bounds, &style, 1.0, 232.0, 16.0,
    );
    assert_eq!(
        right,
        text.len(),
        "scrollport / trailing blank must snap to line end"
    );
}

#[test]
fn textarea_click_line_padding_does_not_jump_to_doc_end() {
    let mut fs = test_font_system();
    let line0 = "short";
    let text = format!("{line0}\nsecond-line-here");
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.white_space = crate::style::WhiteSpace::Nowrap;
    style.overflow = TextOverflow::Clip;
    let bounds = [32.0, 8.0, 80.0, 42.0];
    let left = hit_test_textarea_click_caret_index(
        &mut fs, &text, 8.0, 16.0, bounds, &style, 1.0, 32.0, 16.0,
    );
    assert_eq!(
        left, 0,
        "first-line left padding must be line start, not doc end"
    );
    let right = hit_test_textarea_click_caret_index(
        &mut fs, &text, 200.0, 16.0, bounds, &style, 1.0, 112.0, 16.0,
    );
    assert_eq!(
        right,
        line0.len(),
        "first-line right padding must be that line's end, got {right}"
    );
    let below = hit_test_textarea_click_caret_index(
        &mut fs, &text, 40.0, 80.0, bounds, &style, 1.0, 40.0, 80.0,
    );
    assert_eq!(
        below,
        text.len(),
        "true below-last-line blank still goes to doc end"
    );
    // Soft-wrap pointer space clamps Y onto the last line. Center empty chrome
    // must still be doc-end, not a mid-line glyph (which starts drag-select).
    let clamped_last_line_y = bounds[1] + 21.0 + 10.0;
    let below_clamped = hit_test_textarea_click_caret_index(
        &mut fs,
        &text,
        48.0,
        80.0,
        bounds,
        &style,
        1.0,
        48.0,
        clamped_last_line_y,
    );
    assert_eq!(
        below_clamped,
        text.len(),
        "Y-clamped empty chrome must not hit a glyph, got {below_clamped}"
    );
}

#[test]
fn line_side_blank_detects_padding_and_ink_gap() {
    let mut fs = test_font_system();
    let text = "hi";
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.overflow = TextOverflow::Hidden;
    // Wide leaf so the right side past "hi" is blank.
    let bounds = [10.0, 0.0, 200.0, 21.0];
    assert!(
        hit_test_input_is_line_side_blank(&mut fs, text, 0.0, 10.0, bounds, &style, 1.0),
        "left of leaf is blank"
    );
    assert!(
        hit_test_input_is_line_side_blank(&mut fs, text, 180.0, 10.0, bounds, &style, 1.0),
        "right of ink inside leaf is blank"
    );
    assert!(
        !hit_test_input_is_line_side_blank(&mut fs, text, 12.0, 10.0, bounds, &style, 1.0),
        "over the first glyphs must not count as blank"
    );
}

#[test]
fn line_side_blank_multiline_uses_hard_line() {
    let mut fs = test_font_system();
    let text = "short\nxy";
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    style.overflow = TextOverflow::Clip;
    style.white_space = crate::style::WhiteSpace::Nowrap;
    let bounds = [0.0, 0.0, 200.0, 42.0];
    // Second hard line, far to the right of "xy".
    assert!(hit_test_input_is_line_side_blank(
        &mut fs, text, 150.0, 30.0, bounds, &style, 1.0
    ));
    // Over "xy" on line 1.
    assert!(!hit_test_input_is_line_side_blank(
        &mut fs, text, 2.0, 30.0, bounds, &style, 1.0
    ));
}

#[test]
fn textarea_below_last_line_detects_blank_under_ink() {
    let text = "line1\nline2";
    let mut style = TextStyle::default();
    style.size = Some(14.0);
    style.weight = Some(400);
    style.line_height_px = Some(21.0);
    let bounds = [0.0, 0.0, 200.0, 80.0];
    assert!(
        hit_test_textarea_below_last_line(text, 50.0, bounds, &style),
        "click below last line body"
    );
    assert!(
        !hit_test_textarea_below_last_line(text, 25.0, bounds, &style),
        "click on second line must not count as below"
    );
    assert!(
        !hit_test_textarea_below_last_line(text, 43.0, bounds, &style),
        "click on last line baseline must not count as below"
    );
}
