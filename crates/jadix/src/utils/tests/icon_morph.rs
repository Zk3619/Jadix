use super::*;

#[test]
fn resolves_raw_path_data_without_an_icon_provider() {
    let input = icon_input_from_src("M4 6h16").expect("path data");
    assert!(matches!(input, IconInput::D(_)));
    assert!(icon_input_from_src("jadix-test-nonexistent-icon-3c8b").is_none());
}
