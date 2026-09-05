//! RSX `class={expr}` must apply at build time (not only string literals / `.get()` binds).

use rui::prelude::*;

fn authored_class(el: &Element) -> Option<&str> {
    el.attrs.get("rui_utility_class").map(String::as_str)
}

#[test]
fn rsx_class_variable_applies_on_native_tag() {
    let cls = "p-4";
    let el = ui! {
        <Box class={cls} />
    };
    assert_eq!(authored_class(&el), Some("p-4"));
}

#[test]
fn rsx_class_owned_string_applies_on_native_tag() {
    let cls = String::from("flex gap-2");
    let el = ui! {
        <Box class={cls} />
    };
    assert_eq!(authored_class(&el), Some("flex gap-2"));
}

#[test]
fn rsx_class_variable_applies_on_component_tag() {
    let cls = "p-4";
    let el = ui! {
        <Button class={cls}>"ok"</Button>
    };
    assert_eq!(authored_class(&el), Some("p-4"));
}

#[test]
fn rsx_class_literal_still_applies() {
    let el = ui! {
        <Box class="flex p-2" />
    };
    assert_eq!(authored_class(&el), Some("flex p-2"));
}

#[test]
fn rsx_class_signal_get_still_binds() {
    let sig = use_signal("rsx-class-bind", || "p-2".to_string());
    let el = ui! {
        <Box class={sig.get()} />
    };
    assert_eq!(authored_class(&el), Some("p-2"));
}

#[test]
fn rsx_class_classes_macro_joins_like_class_list() {
    let extra = "gap-2";
    let spin = false;
    let el = ui! {
        <Box class={classes!("flex", extra, ("p-4", true), ("animate-spin", spin))} />
    };
    assert_eq!(authored_class(&el), Some("flex gap-2 p-4"));
}
