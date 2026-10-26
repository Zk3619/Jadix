//! `<div>` / `<Div>` 宏别名 → `Box`。

use jadix::prelude::*;

#[test]
fn div_and_div_pascal_tags_match_box() {
    let from_div = ui! {
        <div id="shell" w="2rem" h="1rem">"hi"</div>
    };
    let from_pascal = ui! {
        <Div id="shell" w="2rem" h="1rem">"hi"</Div>
    };
    let from_box = ui! {
        <Box id="shell" w="2rem" h="1rem">"hi"</Box>
    };
    assert_eq!(from_div.template_id.as_deref(), Some("shell"));
    assert_eq!(from_pascal.template_id.as_deref(), Some("shell"));
    assert_eq!(from_box.template_id.as_deref(), Some("shell"));
    assert_eq!(from_div.style.size, from_box.style.size);
    assert_eq!(from_pascal.style.size, from_box.style.size);
    assert_eq!(from_div.children.len(), 1);
    assert_eq!(from_div.children[0].text.as_deref(), Some("hi"));
    assert_eq!(from_pascal.children[0].text.as_deref(), Some("hi"));
    assert_eq!(from_box.children[0].text.as_deref(), Some("hi"));
}

#[test]
fn div_builder_and_module_match_existing_box_builders() {
    let builder: Element = div().id("shell").w("2rem").h("1rem").text("hi").into();
    let module: Element = jadix::primitives::div::div().w("2rem").h("1rem").into();
    let compatibility: Element = r#box().w("2rem").h("1rem").into();
    assert_eq!(builder.template_id.as_deref(), Some("shell"));
    assert_eq!(builder.text.as_deref(), Some("hi"));
    assert_eq!(builder.style.size, compatibility.style.size);
    assert_eq!(module.style.size, compatibility.style.size);
}
