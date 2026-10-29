use super::*;

#[test]
fn decorate_keeps_title() {
    let o = WindowOptions::new().title("iOS");
    let attrs = decorate_window_attributes(o.to_window_attributes(), &o, false);
    assert_eq!(attrs.title, "iOS");
}
