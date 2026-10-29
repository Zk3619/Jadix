use super::*;
use crate::core::reactivity::{create_root, use_signal};

#[test]
fn controlled_is_the_only_write_path() {
    let caller = use_signal("__jadix_wp37_ctrl", || false);
    let resolved = resolve_bool_signal(Some(caller), "same", SLOT_CHECKBOX, true);
    assert!(
        !resolved.peek(),
        "controlled must ignore default; caller starts false"
    );
    write_bool_toggle(resolved, false);
    assert!(caller.peek());
}

#[test]
fn uncontrolled_is_owner_scoped() {
    let key = bool_slot_key("same_local", SLOT_TOGGLE);
    let (h1, a) = create_root(|| {
        let sig = resolve_bool_signal(None, "same_local", SLOT_TOGGLE, false);
        sig.set(true);
        use_scoped_signal(&key, || false).peek()
    });
    assert!(a);

    let (h2, b) =
        create_root(|| resolve_bool_signal(None, "same_local", SLOT_TOGGLE, false).peek());
    assert!(!b, "fresh owner must not see the other root's store");

    h1.dispose();
    h2.dispose();
}

#[test]
fn disabled_click_does_not_write_or_focus() {
    let caller = use_signal("__jadix_wp37_dis", || false);
    use_signal("__jadix_button_focus_id", String::new).set(String::new());
    let mut click = bool_on_click(caller, true, "focus-me".into());
    click();
    assert!(!caller.peek());
    assert_eq!(
        use_signal("__jadix_button_focus_id", String::new).peek(),
        ""
    );
}

#[test]
fn enabled_click_toggles_and_sets_focus() {
    let caller = use_signal("__jadix_wp37_en", || false);
    let mut click = bool_on_click(caller, false, "focus-me".into());
    click();
    assert!(caller.peek());
    assert_eq!(
        use_signal("__jadix_button_focus_id", String::new).peek(),
        "focus-me"
    );
}

#[test]
fn slots_do_not_collide_under_same_instance_id() {
    let cb = resolve_bool_signal(None, "foo", SLOT_CHECKBOX, false);
    let sw = resolve_bool_signal(None, "foo", SLOT_SWITCH, true);
    assert!(!cb.peek());
    assert!(sw.peek());
    cb.set(true);
    assert!(
        sw.peek() && cb.peek(),
        "independent stores; switch default stays true"
    );
}
