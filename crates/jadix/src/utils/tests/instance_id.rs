use super::*;

#[test]
fn nonempty_passthrough() {
    assert_eq!(resolve_instance_id("gal-tabs", "tabs"), "gal-tabs");
}

#[test]
fn empty_gets_kind_prefixed_unique() {
    let a = resolve_instance_id("", "tabs");
    let b = resolve_instance_id("   ", "tabs");
    assert!(a.starts_with("__jadix_tabs_rt_"), "{a}");
    assert!(b.starts_with("__jadix_tabs_rt_"), "{b}");
    assert_ne!(a, b);
}
