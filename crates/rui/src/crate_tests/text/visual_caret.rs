//! Visual-line caret / wrap regression matrix.

use crate::text::{
    build_visual_lines, caret_visual_line_index, textarea_visual_content_height, LineBreakSource,
    WrapMode, VIRT_CHAR_THRESHOLD,
};
use crate::utils::keyboard::{
    caret_move_vertical_visual_pref, caret_visual_line_end, caret_visual_line_home,
};

#[test]
fn soft_visual_home_end_and_empty_trailing_newline() {
    let text = "hello world hello\n";
    let model = build_visual_lines(text, 14.0, 400, 21.0, 60.0, LineBreakSource::SoftWord);
    assert!(model.line_count() >= 2);
    let caret = text.len();
    let i = caret_visual_line_index(&model.ranges, caret);
    assert_eq!(model.ranges[i], (text.len(), text.len()));
    // Home on a mid-line caret jumps to that visual line's start (may be > 0 after soft wrap).
    let mid = "hello ".len();
    let home = caret_visual_line_home(&model.ranges, mid);
    let line_i = caret_visual_line_index(&model.ranges, mid);
    assert_eq!(home, model.ranges[line_i].0);
    let end = caret_visual_line_end(&model.ranges, home);
    assert!(end >= home);
}

#[test]
fn preferred_x_vertical_move_stays_near_pixel_column() {
    let text = "WWW WWW WWW\niii iii iii";
    let model = build_visual_lines(text, 16.0, 400, 24.0, 400.0, LineBreakSource::SoftWord);
    assert_eq!(model.line_count(), 2);
    let start_second = text.find('\n').unwrap() + 1;
    let px = model.caret_x_on_line(3); // after "WWW"
    let dest =
        caret_move_vertical_visual_pref(text, &model.ranges, Some(&model.caret_xs), 3, 1, Some(px));
    assert!(dest >= start_second);
    let landed_x = model.caret_x_on_line(dest);
    assert!(
        (landed_x - px).abs() < 16.0,
        "preferred x drift too large: from {px} to {landed_x} at {dest}"
    );
}

#[test]
fn wrap_char_breaks_long_token_latin() {
    let text = "Supercalifragilisticexpialidocious";
    let word = build_visual_lines(text, 14.0, 400, 21.0, 40.0, LineBreakSource::SoftWord);
    let ch = build_visual_lines(text, 14.0, 400, 21.0, 40.0, LineBreakSource::SoftChar);
    assert!(
        ch.line_count() >= word.line_count(),
        "char should break at least as much as word: char={} word={}",
        ch.line_count(),
        word.line_count()
    );
    assert!(ch.line_count() > 1, "narrow char wrap should multi-line");
}

#[test]
fn wrap_mode_none_single_visual_line() {
    let text = "aaaa bbbb cccc dddd";
    let model = build_visual_lines(
        text,
        14.0,
        400,
        21.0,
        20.0,
        WrapMode::None.to_line_break_source(),
    );
    assert_eq!(model.line_count(), 1);
}

#[test]
fn virt_height_estimates_for_huge_text() {
    let mut s = String::new();
    while s.len() < VIRT_CHAR_THRESHOLD + 100 {
        s.push_str("line of text that will wrap or not\n");
    }
    let est = textarea_visual_content_height(&s, 14.0, 400, 21.0, 200.0, WrapMode::Word, 4.0);
    assert!(est.estimated);
    assert!(est.content_height > 1000.0);
}
