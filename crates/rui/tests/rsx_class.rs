//! RSX `class={expr}` must apply at build time (not only string literals / `.get()` binds).

use rui::components::{InputProps, SidebarProps};
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

#[test]
fn sidebar_rsx_w_is_rail_own_prop() {
    let el = ui! {
        <SidebarProvider>
            <Sidebar id="rail-w" rail_w="13rem" />
        </SidebarProvider>
    };
    assert_eq!(
        el.attrs.get("data-sidebar-width").map(String::as_str),
        Some("13rem"),
        "RSX rail_w must bind the collapsible track, not Element chrome width"
    );
}

#[test]
fn sidebar_rsx_w_is_layout_chrome_not_rail() {
    let el = ui! {
        <SidebarProvider>
            <Sidebar id="rail-chrome-w" w="13rem" />
        </SidebarProvider>
    };
    assert_eq!(
        el.attrs.get("data-sidebar-width").map(String::as_str),
        Some("16rem"),
        "RSX w is layout chrome; rail stays the 16rem default"
    );
}

#[test]
fn table_rsx_col_w_is_own_prop() {
    let el = ui! {
        <TableHead col_w="100px">"Invoice"</TableHead>
    };
    assert_eq!(
        el.attrs.get("data-table-col-w").map(String::as_str),
        Some("100px"),
        "RSX col_w must stamp the shared column track"
    );
}

#[test]
fn table_rsx_w_is_layout_chrome_not_col() {
    let el = ui! {
        <TableHead w="100px">"Invoice"</TableHead>
    };
    assert!(
        el.attrs.get("data-table-col-w").is_none(),
        "RSX w is layout chrome, not a column track"
    );
}

#[test]
fn interactive_rsx_disabled_is_primitive() {
    let el = ui! {
        <Interactive disabled={true} />
    };
    assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
}

#[test]
fn button_rsx_disabled_stamps_attr() {
    let el = ui! {
        <Button disabled={true}>"ok"</Button>
    };
    assert_eq!(el.attrs.get("disabled").map(String::as_str), Some("true"));
    assert!(!el.focusable);
}

#[test]
fn input_rsx_text_size_and_caret_forward_into_paint_sync() {
    let el = ui! {
        <Input id="sz-rsx" text_size={20.0} caret_color="#111111" ring_width={5.0} ring_color="primary" />
    };
    let sync = el.input_paint_sync.as_ref().expect("paint sync");
    assert!(
        (sync.text_size_px - 20.0).abs() < 0.01,
        "RSX text_size must update inner glyphs"
    );
    let want_caret = rui::core::tree::parse_text_color("#111111");
    assert_eq!(sync.caret_color, Some(want_caret));
    assert_eq!(el.style.focus_ring_width, Some(5.0));
    let want_ring = rui::core::tree::parse_color("primary");
    let got = el.style.focus_ring_color.expect("ring color");
    assert!((got[0] - want_ring[0]).abs() < 0.001);
}
