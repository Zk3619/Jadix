use super::*;

#[test]
fn interpolates_names() {
    let s = format_message("Hello, {name}!", &[Arg::new("name", "Ada")], "en");
    assert_eq!(s, "Hello, Ada!");
}

#[test]
fn plural_english() {
    let t = "{n, plural, one {# item} other {# items}}";
    assert_eq!(format_message(t, &[Arg::new("n", "1")], "en"), "1 item");
    assert_eq!(format_message(t, &[Arg::new("n", "3")], "en"), "3 items");
    assert_eq!(format_message(t, &[Arg::new("n", "0")], "en"), "0 items");
}

#[test]
fn plural_exact_zero() {
    let t = "{n, plural, =0 {none} one {one} other {#}}";
    assert_eq!(format_message(t, &[Arg::new("n", "0")], "en"), "none");
}

#[test]
fn select_gender() {
    let t = "{g, select, male {he} female {she} other {they}}";
    assert_eq!(format_message(t, &[Arg::new("g", "female")], "en"), "she");
    assert_eq!(format_message(t, &[Arg::new("g", "x")], "en"), "they");
}

#[test]
fn quoted_braces() {
    let s = format_message("Use '{'name'}' for {name}", &[Arg::new("name", "x")], "en");
    assert_eq!(s, "Use {name} for x");
}
