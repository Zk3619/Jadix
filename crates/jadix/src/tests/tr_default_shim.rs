#[test]
fn yields_source_string() {
    assert_eq!(
        crate::tr_default("jadix.pagination.previous", "Previous"),
        "Previous"
    );
}

#[test]
fn interpolates_placeholders() {
    assert_eq!(
        crate::tr_with_default(
            "jadix.date_picker.week_of",
            "Week of {date}",
            &[("date", "August 14, 2026")]
        ),
        "Week of August 14, 2026"
    );
}

#[test]
fn english_source_is_used_by_default() {
    assert_eq!(crate::tr_default("jadix.pagination.next", "Next"), "Next");
    assert_eq!(
        crate::tr_default_zh("jadix.pagination.next", "Next", "下一页"),
        "Next"
    );
    assert_eq!(
        crate::tr_default!("jadix.pagination.previous", "Previous", "上一页"),
        "Previous"
    );
}
