use super::*;

#[test]
fn select_all_and_extend_selection() {
    assert_eq!(select_all_range(""), None);
    assert_eq!(select_all_range("你好"), Some((0, "你好".len())));
    // New selection: anchor at old caret
    assert_eq!(extend_selection(3, None, 1), (3, 1));
    // Extend from existing: caret was at hi → keep lo
    assert_eq!(extend_selection(5, Some((2, 5)), 7), (2, 7));
    // Extend from existing: caret was at lo → keep hi
    assert_eq!(extend_selection(2, Some((2, 5)), 0), (5, 0));
    assert_eq!(normalized_selection("abcd", Some((3, 1))), Some((1, 3)));
    assert_eq!(normalized_selection("abcd", Some((2, 2))), None);
}

#[test]
fn select_line_range_covers_hard_line_only() {
    let s = "ab\n你好\ncd";
    let on_mid = "ab\n".len() + "你".len();
    assert_eq!(
        select_line_range(s, on_mid),
        Some(("ab\n".len(), "ab\n你好".len()))
    );
    assert_eq!(select_line_range(s, 0), Some((0, 2)));
    assert_eq!(select_line_range("", 0), None);
    assert_eq!(select_line_range("\n", 0), None);
}

#[test]
fn home_end_stay_on_hard_line() {
    let s = "的\n啊\n我";
    let mid = "的\n".len() + "啊".len(); // end of line 1 "啊"
    assert_eq!(caret_line_home(s, mid), "的\n".len());
    assert_eq!(caret_line_end(s, "的\n".len()), "的\n".len() + "啊".len());
    assert_eq!(caret_line_home(s, 0), 0);
    assert_eq!(caret_line_end(s, s.len()), s.len());
}

#[test]
fn arrow_vertical_moves_between_hard_lines() {
    let s = "的\n啊\n我";
    let on_second = "的\n".len(); // start of "啊"
    let up = caret_move_vertical(s, on_second, -1);
    assert_eq!(up, 0);
    let down = caret_move_vertical(s, on_second, 1);
    assert_eq!(down, "的\n啊\n".len());
    // First line Up stays; last line Down stays.
    assert_eq!(caret_move_vertical(s, 0, -1), 0);
    assert_eq!(caret_move_vertical(s, s.len(), 1), s.len());
}

#[test]
fn arrow_vertical_preserves_char_column() {
    let s = "abc\nxy";
    let after_b = "ab".len();
    let down = caret_move_vertical(s, after_b, 1);
    assert_eq!(down, "abc\n".len() + "xy".len()); // col 2 clamps to end of "xy"
    let s2 = "ab\nxyz";
    let after_a = "a".len();
    let down2 = caret_move_vertical(s2, after_a, 1);
    assert_eq!(down2, "ab\n".len() + "x".len());
}

#[test]
fn line_aware_horizontal_skips_newline() {
    let s = "abc\ndef";
    let line2 = "abc\n".len();
    assert_eq!(caret_move_left_line_aware(s, line2), "abc".len());
    assert_eq!(caret_move_right_line_aware(s, "abc".len()), line2);
}

#[test]
fn visual_horizontal_wraps_at_line_boundaries() {
    use crate::text::{build_visual_lines, LineBreakSource};
    let text = "hello world wrap";
    let model = build_visual_lines(text, 14.0, 400, 21.0, 60.0, LineBreakSource::SoftWord);
    assert!(model.line_count() >= 2);
    let (_, end0) = model.ranges[0];
    let (start1, _) = model.ranges[1];
    assert_eq!(caret_move_right_visual(text, &model.ranges, end0), start1);
    assert_eq!(caret_move_left_visual(text, &model.ranges, start1), end0);
}
