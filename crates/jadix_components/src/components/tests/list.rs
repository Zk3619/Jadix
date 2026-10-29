use super::*;

#[test]
fn list_marker_parse() {
    assert_eq!(ListMarker::from("disc"), ListMarker::Disc);
    assert_eq!(ListMarker::from("decimal"), ListMarker::Decimal);
    assert_eq!(ListMarker::from("none"), ListMarker::None);
}

#[test]
fn list_sets_role_and_slot() {
    let el = List(ListProps {
        marker: Some(ListMarker::Disc),
        children: None,
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
    assert_eq!(el.attrs.get("role").map(String::as_str), Some("list"));
    assert_eq!(el.attrs.get(ATTR_MARKER).map(String::as_str), Some("disc"));
}

#[test]
fn list_item_sets_listitem_role() {
    let el = ListItem(ListItemProps {
        marker: Some(ListMarker::Disc),
        children: Some(Element::new().text("Row")),
        base: None,
        base_builder: None,
        ..Default::default()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ITEM));
    assert_eq!(el.attrs.get("role").map(String::as_str), Some("listitem"));
    assert!(el
        .children
        .iter()
        .any(|c| c.text.as_deref() == Some("\u{2022}")));
}

#[test]
fn list_decimal_marker_renders_index() {
    let el = build_list_item_shell(
        ListMarker::Decimal,
        2,
        vec![Element::new().text("Third")],
        None,
    );
    assert!(el.children.iter().any(|c| c.text.as_deref() == Some("3.")));
}

#[test]
fn list_decimal_reindex_strips_existing_marker() {
    let first = build_list_item_shell(
        ListMarker::Decimal,
        0,
        vec![Element::new().text("One")],
        None,
    );
    let kids = apply_list_decimal_indices(
        ListMarker::Decimal,
        vec![
            first,
            build_list_item_shell(
                ListMarker::Decimal,
                0,
                vec![Element::new().text("Two")],
                None,
            ),
        ],
    );
    assert_eq!(kids.len(), 2);
    let count_ones = kids[0]
        .children
        .iter()
        .filter(|c| c.text.as_deref() == Some("1."))
        .count();
    assert_eq!(count_ones, 1, "must not double-wrap decimal marker");
    let count_twos = kids[1]
        .children
        .iter()
        .filter(|c| c.text.as_deref() == Some("2."))
        .count();
    assert_eq!(count_twos, 1);
}

#[test]
fn list_builder_dual_surface() {
    let _el: Element = list()
        .marker("disc")
        .header("Title")
        .description("Subtitle")
        .text("One")
        .text_row("Two", "Second line")
        .into();
}

#[test]
fn list_header_with_chrome_applies_text_size() {
    let el: Element = list()
        .class("w-full")
        .header_with(|h| h.class("font-bold").style(|b| b.text_size(18.0)).text("Hi"))
        .into();
    let header = el
        .children
        .iter()
        .find(|c| c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_HEADER))
        .expect("list header slot");
    assert_eq!(header.text_style.size, Some(18.0));
}
