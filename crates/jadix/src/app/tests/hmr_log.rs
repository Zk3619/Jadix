#[test]
fn format_12h_midnight_and_afternoon() {
    assert_eq!(super::format_12h(0, 5, 9), "12:05:09 AM");
    assert_eq!(super::format_12h(13, 0, 0), "1:00:00 PM");
    assert_eq!(super::format_12h(12, 0, 0), "12:00:00 PM");
}
