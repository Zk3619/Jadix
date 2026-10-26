#![allow(non_snake_case)]

use jadix::authoring::{
    author_class, data_slot, ensure_data_slot, packed_children, paint_slot, BaseComponentBuilder,
    SlotChrome, SlotMap,
};
use jadix::core::tree::Element;

mod runtime {
    pub use jadix::{app, authoring, core, style};
}

#[jadix::component(crate_path = "runtime")]
fn FrameworkOnlyLeaf(value: String, children: Option<Element>) -> Element {
    Element::new()
        .text(value)
        .children(Element::unpack_fragment_children(children))
}

#[test]
fn component_expansion_uses_the_framework_authoring_surface() {
    let element = FrameworkOnlyLeaf(FrameworkOnlyLeafProps {
        value: Some("framework leaf".into()),
        children: Some(Element::new().text("child")),
        base: Some(Box::new(|builder| builder.text_size(23.0).opacity(0.4))),
    });
    assert_eq!(element.text.as_deref(), Some("framework leaf"));
    assert_eq!(element.text_style.size, Some(23.0));
    assert_eq!(element.children[0].text.as_deref(), Some("child"));
    assert_eq!(element.style.opacity, 0.4);
}

#[test]
fn legacy_paths_share_types_and_preserve_base_merge_order() {
    let earlier = Some(
        Box::new(|builder: jadix::components::base::BaseComponentBuilder| {
            builder.opacity(0.2).text_size(16.0)
        }) as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>,
    );
    let later = Some(
        Box::new(|builder: BaseComponentBuilder| builder.opacity(0.7))
            as Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>,
    );
    let merged = jadix::app::props::merge_base_prop(earlier, later).expect("merged style");
    let element = merged(BaseComponentBuilder::new(Element::new().text("kept"))).finish();
    assert_eq!(element.text.as_deref(), Some("kept"));
    assert_eq!(element.style.opacity, 0.7);
    assert_eq!(element.text_style.size, Some(16.0));
    let chrome: jadix::components::SlotChrome = SlotChrome::default();
    let chrome: jadix::prelude::SlotChrome = chrome;
    assert!(chrome.is_empty());
    let slots: jadix::components::base::SlotMap = SlotMap::default();
    let slots: jadix::prelude::SlotMap = slots;
    assert!(slots.get("missing").is_empty());
}

#[test]
fn slot_chrome_overlays_classes_and_styles_in_author_order() {
    let inner = SlotChrome::default()
        .class("p-2")
        .style(|builder| builder.opacity(0.2));
    let outer = SlotChrome::default()
        .class("rounded")
        .style(|builder| builder.opacity(0.8).text_size(21.0));
    let chrome = inner.overlay(outer);
    let element = paint_slot(Element::new().text("kept"), "content", &chrome);
    assert_eq!(data_slot(&element), Some("content"));
    assert_eq!(author_class(&element), "p-2 rounded");
    assert_eq!(element.text.as_deref(), Some("kept"));
    assert_eq!(element.style.opacity, 0.8);
    assert_eq!(element.text_style.size, Some(21.0));
}

#[test]
fn slot_map_and_fragment_helpers_preserve_authored_nodes() {
    let slots = SlotMap::default().slot("content", |chrome| {
        chrome.style(|builder| builder.text_size(19.0))
    });
    let element = slots.paint("content", Element::new().text("kept"));
    let element = ensure_data_slot(element, "content", |_| panic!("already stamped"));
    assert_eq!(element.text.as_deref(), Some("kept"));
    assert_eq!(element.text_style.size, Some(19.0));
    let packed = packed_children(vec![element, Element::new().text("sibling")]);
    let children = Element::unpack_fragment_children(packed);
    assert_eq!(children.len(), 2);
    assert_eq!(data_slot(&children[0]), Some("content"));
    assert!(packed_children(Vec::new()).is_none());
}

#[test]
fn product_item_chrome_stays_in_the_product_compatibility_module() {
    use jadix::components::base::{push_pending_item_chrome, take_pending_item_chrome};
    let _ = take_pending_item_chrome();
    push_pending_item_chrome(SlotChrome::default().class("p-4"));
    assert_eq!(take_pending_item_chrome().class, "p-4");
    assert!(take_pending_item_chrome().is_empty());
}
