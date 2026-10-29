use super::format_12h;

#[test]
fn format_12h_midnight_and_afternoon() {
    assert_eq!(format_12h(0, 5, 7), "12:05:07 AM");
    assert_eq!(format_12h(13, 34, 2), "1:34:02 PM");
    assert_eq!(format_12h(12, 0, 0), "12:00:00 PM");
}
