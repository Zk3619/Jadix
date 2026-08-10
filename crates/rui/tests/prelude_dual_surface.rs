//! C-WP-0.3：`rui::prelude` 同时导出标签与 Builder，且可互嵌编译。

use rui::prelude::*;

#[test]
fn prelude_builder_surface_compiles() {
    let el: Element = button("prelude-btn")
        .variant("outline")
        .text("Builder")
        .into();
    let _ = el;
}

#[test]
fn prelude_tag_surface_compiles() {
    let el = ui! {
        <Button id="prelude-tag-btn" variant="outline">"Tag"</Button>
    };
    let _: Element = el;
}

#[test]
fn prelude_ui_embeds_builder() {
    let el = ui! {
        <VStack gap="0.5rem">
            { Element::from(button("prelude-embed-b").variant("ghost").text("内嵌 Builder")) }
            <Button id="prelude-embed-t" variant="outline">"标签"</Button>
        </VStack>
    };
    let _: Element = el;
}

#[test]
fn prelude_builder_embeds_ui() {
    let el: Element = vstack()
        .gap("0.5rem")
        .child(ui! { <Label id="prelude-embed-l">"内嵌标签"</Label> })
        .child(Element::from(
            button("prelude-embed-b2").variant("default").text("外层 Builder"),
        ))
        .into();
    assert!(!el.children.is_empty());
}
