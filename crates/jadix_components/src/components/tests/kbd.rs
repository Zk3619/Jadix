use super::*;

#[test]
fn kbd_sets_slot_and_muted_chrome() {
    let el: Element = kbd().text("⌘").into();
    assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_KBD));
    assert_eq!(el.style.bg_color, parse_color("muted"));
    assert_eq!(
        el.attrs
            .get(crate::core::tree::ATTR_POINTER_EVENTS)
            .map(|s| s.as_str()),
        Some("none")
    );
    let leaf = el
        .children
        .iter()
        .find(|c| c.text.as_deref() == Some("⌘"))
        .expect("caption leaf");
    assert_eq!(
        leaf.text_style.color_resolved(),
        parse_text_color("muted-foreground")
    );
    assert_eq!(leaf.text_style.weight_resolved(), 500);
}

#[test]
fn kbd_empty_id_does_not_write_empty_template() {
    let el: Element = kbd().text("K").into();
    assert!(el.template_id.is_none() || el.template_id.as_deref() != Some(""));
}

#[test]
fn kbd_group_unpacks_and_sets_slot() {
    let el: Element = KbdGroup(KbdGroupProps {
        children: Some(Element::from(
            crate::primitives::r#box::vstack()
                .children(vec![kbd().text("Ctrl").into(), kbd().text("K").into()]),
        )),
        ..Default::default()
    });
    assert_eq!(
        el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some(SLOT_KBD_GROUP)
    );
    assert_eq!(el.children.len(), 2);
    assert!(el
        .children
        .iter()
        .all(|c| c.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_KBD)));
}

#[test]
fn kbd_group_builder_kbd_helper() {
    let el: Element = kbd_group().kbd("Ctrl").kbd("B").into();
    assert_eq!(el.children.len(), 2);
}

#[test]
fn kbd_group_builder_style() {
    let el: Element = kbd_group().style(|b| b.text_size(20.0)).kbd("Ctrl").into();
    assert!(
        el.text_style.size.is_some_and(|s| (s - 20.0).abs() < 0.01),
        "kbd_group .style must land on the cluster root"
    );
}
