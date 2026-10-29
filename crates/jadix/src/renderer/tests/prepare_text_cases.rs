#[test]
fn physical_text_y_removes_scroll_noise_without_rounding_fractional_layout() {
    for (actual, expected) in [
        (201.99998, 202.0),
        (224.00002, 224.0),
        (-201.99998, -202.0),
        (-224.00002, -224.0),
        (201.25, 201.25),
        (201.75, 201.75),
        (-201.75, -201.75),
    ] {
        assert_eq!(stable_physical_text_y(actual), expected);
    }
}
