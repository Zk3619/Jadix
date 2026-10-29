use super::*;

#[test]
fn empty_root_slot_and_layout() {
    let el = Empty(EmptyProps {
        id: String::new(),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert!(el.template_id.is_none());
}

#[test]
fn empty_media_icon_variant_chrome() {
    let el = EmptyMedia(EmptyMediaProps {
        variant: Some(EmptyMediaVariant::Icon),
        children: Some(Element::new().text("inbox")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_MEDIA)
    );
    assert_eq!(el.attrs.get(ATTR_VARIANT).map(String::as_str), Some("icon"));
    assert_eq!(el.style.bg_color, parse_color("muted"));
}

#[test]
fn empty_title_and_description_slots() {
    let title = EmptyTitle(EmptyTitleProps {
        children: Some(Element::new().text("Title")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let desc = EmptyDescription(EmptyDescriptionProps {
        children: Some(Element::new().text("Desc")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(
        title.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_TITLE)
    );
    assert_eq!(
        desc.attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_DESCRIPTION)
    );
}

#[test]
fn builder_assembles_header_and_content() {
    let el: Element = empty()
        .header(
            empty_header()
                .title("No data")
                .description("Try creating one."),
        )
        .content_child(Element::new().text("Action"))
        .into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(el.children.len(), 2);
    assert_eq!(
        el.children[0].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_HEADER)
    );
    assert_eq!(
        el.children[1].attrs.get(ATTR_SLOT).map(String::as_str),
        Some(SLOT_CONTENT)
    );
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = empty().header(empty_header().title("X")).into();
    let el = Empty(EmptyProps {
        id: String::new(),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    let _: Element = el;
}
