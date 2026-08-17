//! WP-3.7：Toggle / Checkbox / Switch 共享布尔状态原语（受控 / 非受控 / disabled）。

use crate::components::{checkbox, switch, toggle};
use crate::core::reactivity::{create_root, use_scoped_signal, use_signal, Signal};
use crate::core::tree::Element;
use crate::utils::toggle_state::{bool_slot_key, SLOT_CHECKBOX, SLOT_SWITCH, SLOT_TOGGLE};

fn fire_click(el: &Element) {
    el.on_click
        .as_ref()
        .unwrap_or_else(|| panic!("missing on_click"))
        .borrow_mut()();
}

fn build_toggle(sig: Signal<bool>) -> Element {
    toggle("wp37-t").pressed_signal(sig).into()
}

fn build_checkbox(sig: Signal<bool>) -> Element {
    checkbox("wp37-cb").checked_signal(sig).into()
}

fn build_switch(sig: Signal<bool>) -> Element {
    switch("wp37-sw").checked_signal(sig).into()
}

#[test]
fn toggle_checkbox_switch_controlled_click_writes_caller_signal() {
    let cases: [(&str, fn(Signal<bool>) -> Element); 3] = [
        ("toggle", build_toggle),
        ("checkbox", build_checkbox),
        ("switch", build_switch),
    ];
    for (name, build) in cases {
        let sig = use_signal(&format!("__rui_wp37_ctrl_{name}"), || false);
        let el = build(sig);
        fire_click(&el);
        assert!(
            sig.peek(),
            "{name}: controlled click must write caller signal"
        );
        fire_click(&el);
        assert!(
            !sig.peek(),
            "{name}: second click must flip caller signal back"
        );
    }
}

#[test]
fn toggle_checkbox_switch_disabled_does_not_write() {
    let t = use_signal("__rui_wp37_dis_t", || false);
    let c = use_signal("__rui_wp37_dis_c", || false);
    let s = use_signal("__rui_wp37_dis_s", || false);
    let tel: Element = toggle("wp37-dt").pressed_signal(t).disabled(true).into();
    let cel: Element = checkbox("wp37-dc").checked_signal(c).disabled(true).into();
    let sel: Element = switch("wp37-ds").checked_signal(s).disabled(true).into();
    fire_click(&tel);
    fire_click(&cel);
    fire_click(&sel);
    assert!(!t.peek() && !c.peek() && !s.peek());
}

#[test]
fn toggle_checkbox_switch_uncontrolled_scoped_per_owner() {
    let specs: [(&str, &str, fn(&str) -> Element); 3] = [
        (SLOT_TOGGLE, "same_t", |id| toggle(id).into()),
        (SLOT_CHECKBOX, "same_c", |id| checkbox(id).into()),
        (SLOT_SWITCH, "same_s", |id| switch(id).into()),
    ];
    for (slot, local, build) in specs {
        let key = bool_slot_key(local, slot);
        let (h1, flipped) = create_root(|| {
            let el = build(local);
            fire_click(&el);
            use_scoped_signal(&key, || false).peek()
        });
        assert!(flipped, "{slot}: click must flip owner-scoped store");

        let (h2, fresh) = create_root(|| {
            let _el = build(local);
            use_scoped_signal(&key, || false).peek()
        });
        assert!(!fresh, "{slot}: fresh owner must start at default");

        h1.dispose();
        h2.dispose();
    }
}
