//! Regression: Textarea RSX `wrap` / `rows` must be component props, not flex-wrap / grid-rows.

#![allow(non_snake_case)]

use jadix_components::components::{Textarea, TextareaProps};
use jadix_components::style::WhiteSpace;
use jadix_components::ui;

fn find_tid<'a>(
    el: &'a jadix_components::core::tree::Element,
    tid: &str,
) -> Option<&'a jadix_components::core::tree::Element> {
    if el.template_id.as_deref() == Some(tid) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, tid))
}

#[test]
fn textarea_rsx_wrap_off_reaches_leaf_nowrap() {
    let id = "ta-rsx-wrap-off";
    let el = ui! {
        <Textarea id={id} rows={3} wrap="off" resizable={false} />
    };
    let tid = format!("__jadix_input:{id}:text");
    let leaf = find_tid(&el, &tid).expect("text leaf");
    assert_eq!(
        leaf.text_style.white_space,
        WhiteSpace::Nowrap,
        "RSX wrap=\"off\" must set Textarea wrap prop (not flex-wrap)"
    );
}

#[test]
fn textarea_rsx_wrap_char_stays_soft_whitespace() {
    let id = "ta-rsx-wrap-char";
    let el = ui! {
        <Textarea id={id} rows={3} wrap="char" resizable={false} />
    };
    let tid = format!("__jadix_input:{id}:text");
    let leaf = find_tid(&el, &tid).expect("text leaf");
    assert_eq!(
        leaf.text_style.white_space,
        WhiteSpace::Normal,
        "RSX wrap=\"char\" must soft-wrap (normal white_space)"
    );
}

#[test]
fn textarea_rsx_rows_reaches_builder() {
    // Smoke: integer literal rows={5} must type-check as Textarea prop (i32 → u32).
    let el = ui! {
        <Textarea id="ta-rsx-rows" rows={5} wrap="soft" resizable={false} />
    };
    assert_eq!(el.focus_id.as_deref(), Some("textarea:ta-rsx-rows"));
}
