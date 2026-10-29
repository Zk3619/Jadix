use super::*;

#[test]
fn kde_wheel_scroll_lines_from_section() {
    let text = "[KDE]\nWheelScrollLines=5\n";
    assert_eq!(parse_kde_wheel_scroll_lines(text), Some(5.0));
}

#[test]
fn kde_wheel_scroll_lines_ignores_comments_and_zero() {
    let text = "# WheelScrollLines=9\nWheelScrollLines=0\nWheelScrollLines=3\n";
    assert_eq!(parse_kde_wheel_scroll_lines(text), Some(3.0));
}

#[test]
fn default_is_toolkit_convention() {
    assert_eq!(DEFAULT_SCROLL_LINES, 3.0);
}
