use super::*;
use crate::core::reactivity::{create_root, use_scoped_signal, use_signal};

#[test]
fn string_controlled_ignores_default() {
    let caller = use_signal("__jadix_si2_str", || "a".to_string());
    let resolved = resolve_string_signal(Some(caller), "same", "tabs", "b");
    assert_eq!(resolved.peek(), "a");
    resolved.set("c".into());
    assert_eq!(caller.peek(), "c");
}

#[test]
fn string_uncontrolled_is_owner_scoped() {
    let (_o, a) = create_root(|| resolve_string_signal(None, "inst", "tabs", "x").peek());
    assert_eq!(a, "x");
    let (_o2, b) = create_root(|| {
        let key = bool_slot_key("inst", "tabs");
        use_scoped_signal(&key, || "y".to_string()).peek()
    });
    // Separate roots → separate Owner stores; both may init independently.
    assert_eq!(b, "y");
}

#[test]
fn f32_and_open_resolve() {
    let v = resolve_f32_signal(None, "sl", "value", 0.5);
    assert!((v.peek() - 0.5).abs() < f32::EPSILON);
    let open = resolve_open_signal(None, "dp", false);
    assert!(!open.peek());
    open.set(true);
    assert!(open.peek());
}
