use super::*;

#[test]
fn navbar_without_id_writes_no_template_id() {
    let el: Element = navbar().into();
    assert!(el.template_id.is_none());
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_NAVBAR)
    );
}

#[test]
fn navbar_empty_id_writes_no_template_id() {
    let el: Element = navbar().id("").into();
    assert!(el.template_id.is_none());
    assert_ne!(el.template_id.as_deref(), Some(""));
}

#[test]
fn navbar_explicit_id_sets_template_id() {
    let el: Element = navbar().id("nav-main").into();
    assert_eq!(el.template_id.as_deref(), Some("nav-main"));
}

#[test]
fn navbar_rsx_parts_expose_slots() {
    let el = Navbar(NavbarProps {
        id: String::new(),
        children: Some({
            let mut frag = Element::new();
            frag.set_attr("jadix_fragment", "1");
            frag.children(vec![
                NavbarBrand(NavbarBrandProps {
                    children: Some(Element::new().text("Brand")),
                    base: None,
                }),
                NavbarCenter(NavbarCenterProps {
                    justify: None,
                    children: Some(Element::new().text("Center")),
                    base: None,
                }),
                NavbarActions(NavbarActionsProps {
                    children: Some(Element::new().text("Go")),
                    base: None,
                }),
            ])
        }),
        base: None,
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_NAVBAR)
    );
    assert_eq!(el.children.len(), 3);
    assert_eq!(
        el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_BRAND)
    );
    assert_eq!(
        el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_CENTER)
    );
    assert_eq!(
        el.children[2].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_ACTIONS)
    );
}

#[test]
fn navbar_builder_defaults_mid_slot_to_start() {
    let el: Element = navbar()
        .brand(Element::new().text("B"))
        .center_child(Element::new().text("C"))
        .action(Element::new().text("A"))
        .into();
    let center = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_CENTER))
        .expect("center slot");
    assert_eq!(
        center.style.justify_content,
        Some(taffy::style::JustifyContent::START)
    );
}

#[test]
fn navbar_builder_center_justify_override() {
    let el: Element = navbar()
        .center_child(Element::new().text("C"))
        .center_justify("center")
        .into();
    let center = &el.children[0];
    assert_eq!(
        center.style.justify_content,
        Some(taffy::style::JustifyContent::CENTER)
    );
}

#[test]
fn navbar_rsx_center_defaults_to_start() {
    let el = NavbarCenter(NavbarCenterProps {
        justify: None,
        children: Some(Element::new().text("C")),
        base: None,
    });
    assert_eq!(
        el.style.justify_content,
        Some(taffy::style::JustifyContent::START)
    );
}

#[test]
fn navbar_brand_with_class_and_style_text_size() {
    let el: Element = navbar()
        .brand_with(|b| {
            b.class("custom-brand")
                .style(|s| s.text_size(18.0))
                .child(Element::new().text("Jadix"))
        })
        .into();
    let brand = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_BRAND))
        .expect("brand slot");
    assert!(brand
        .attrs
        .get("jadix_utility_class")
        .map(String::as_str)
        .is_some_and(|c| c.contains("custom-brand")));
    assert_eq!(brand.text_style.size_resolved(), 18.0);
}

#[test]
fn navbar_brand_preserves_existing_slot() {
    let existing: Element = navbar_brand().child(Element::new().text("Logo")).into();
    let el: Element = navbar().brand(existing).into();
    assert_eq!(el.children.len(), 1);
    assert_eq!(
        el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_BRAND)
    );
}
