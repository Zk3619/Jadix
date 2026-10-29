use super::*;

#[test]
fn item_variant_and_size_parse() {
    assert_eq!(ItemVariant::from("outline"), ItemVariant::Outline);
    assert_eq!(ItemSize::from("xs"), ItemSize::Xs);
    assert_eq!(ItemMediaVariant::from("image"), ItemMediaVariant::Image);
}

#[test]
fn item_paint_sets_slots() {
    let mut el = Element::new();
    assert_eq!(
        apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Sm),
        LayoutStatus::LayoutChanged
    );
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ITEM));
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("outline")
    );
    assert_eq!(
        apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Sm),
        LayoutStatus::Clean
    );
    assert_eq!(
        apply_item_paint(&mut el, ItemVariant::Muted, ItemSize::Sm),
        LayoutStatus::StyleChanged
    );
}

#[test]
fn item_group_and_title_slots() {
    let group = ItemGroup(ItemGroupProps {
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        group.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_GROUP)
    );
    let title = ItemTitle(ItemTitleProps {
        children: Some(Element::new().text("Hi")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        title.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_TITLE)
    );
}

#[test]
fn apply_bound_item_variant_preserves_template_id() {
    let mut el = Element::new();
    el.template_id = Some("item-bind-test".into());
    assert_eq!(
        apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Default),
        LayoutStatus::StyleChanged
    );
    assert_eq!(el.template_id.as_deref(), Some("item-bind-test"));
    assert_eq!(
        apply_bound_item_variant(&mut el, "muted"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(
        el.attrs.get(ATTR_VARIANT).map(String::as_str),
        Some("muted")
    );
    assert_eq!(el.template_id.as_deref(), Some("item-bind-test"));
}

#[test]
fn apply_item_paint_preserves_layout_node() {
    let node = taffy::prelude::NodeId::new(42);
    let mut el = Element::new();
    el.set_layout_node_id(Some(node));
    el.template_id = Some("item-layout-node".into());
    assert_eq!(
        apply_item_paint(&mut el, ItemVariant::Outline, ItemSize::Default),
        LayoutStatus::StyleChanged
    );
    assert_eq!(el.layout_node_id(), Some(node));
    assert_eq!(
        apply_bound_item_variant(&mut el, "default"),
        LayoutStatus::StyleChanged
    );
    assert_eq!(el.layout_node_id(), Some(node));
}

#[test]
fn dual_surface_compiles_shape() {
    let _g: Element = item_group()
        .child(
            item()
                .variant("muted")
                .size("sm")
                .title_description("T", "D"),
        )
        .into();
}

#[test]
fn item_title_with_chrome_applies_text_size() {
    let el: Element = item()
        .class("w-full")
        .title_with(|t| t.style(|b| b.text_size(18.0)).text("Hi"))
        .into();
    let title = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CONTENT))
        .and_then(|content| {
            content
                .children
                .iter()
                .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_TITLE))
        })
        .expect("item title slot");
    assert_eq!(title.text_style.size, Some(18.0));
}
