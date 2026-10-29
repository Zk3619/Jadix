use super::*;
use crate::primitives::r#box::vstack;

#[test]
fn fragment_unpacks_children() {
    let mut frag = Element::new();
    frag.set_attr(ATTR_FRAGMENT, "1");
    frag = frag.children(vec![Element::new().text("a"), Element::new().text("b")]);
    let kids = take_packed(frag);
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].text.as_deref(), Some("a"));
    assert_eq!(kids[1].text.as_deref(), Some("b"));
}

#[test]
fn take_fragment_ignores_anonymous_vstack() {
    let pack =
        Element::from(vstack().children(vec![Element::new().text("a"), Element::new().text("b")]));
    let kids = take_fragment(pack);
    assert_eq!(kids.len(), 1, "take_fragment must not peel VStack");
}

#[test]
fn anonymous_vstack_unpacks() {
    let pack = Element::from(vstack().children(vec![
        Element::new().text("Search"),
        Element::new().text("⌘"),
    ]));
    let kids = take_packed(pack);
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].text.as_deref(), Some("Search"));
}

#[test]
fn opacity_group_shell_survives_packing_and_empty_child_normalization() {
    for enabled in [false, true] {
        let empty = Element::new().opacity_group(enabled);
        assert!(children_none_if_vacuous(Some(empty)).is_some());
        let shell = Element::new()
            .opacity_group(enabled)
            .child(Element::new().text("a"))
            .child(Element::new().text("b"));
        let packed = take_packed(shell);
        assert_eq!(packed.len(), 1);
        assert_eq!(packed[0].children.len(), 2);
    }
}

#[test]
fn plain_leaf_stays_singleton() {
    let el = Element::new().text("x");
    let kids = take_packed(el);
    assert_eq!(kids.len(), 1);
    assert_eq!(kids[0].text.as_deref(), Some("x"));
}

#[test]
fn data_slot_shell_is_not_dismantled() {
    let mut kbd =
        Element::from(vstack().children(vec![Element::new().text("⌘"), Element::new().text("K")]));
    kbd.set_attr(ATTR_SLOT, "kbd-group");
    let kids = take_packed(kbd);
    assert_eq!(kids.len(), 1);
    assert_eq!(
        kids[0].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some("kbd-group")
    );
    assert_eq!(kids[0].children.len(), 2);
}

#[test]
fn nested_fragment_flattens_but_keeps_slot() {
    let mut kbd = Element::new().text("⌘");
    kbd.set_attr(ATTR_SLOT, "kbd");

    let mut inner = Element::new();
    inner.set_attr(ATTR_FRAGMENT, "1");
    inner = inner.children(vec![Element::new().text("Search"), kbd]);

    let mut outer = Element::new();
    outer.set_attr(ATTR_FRAGMENT, "1");
    outer = outer.children(vec![inner]);

    let kids = take_packed(outer);
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].text.as_deref(), Some("Search"));
    assert_eq!(
        kids[1].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
        Some("kbd")
    );
}

#[test]
fn padded_anonymous_vstack_is_not_dismantled() {
    let pack = Element::from(
        vstack()
            .w("100%")
            .p("0.25rem")
            .children(vec![Element::new().text("a"), Element::new().text("b")]),
    );
    let kids = take_packed(pack);
    assert_eq!(kids.len(), 1, "padded shell must stay intact");
    assert!(
        lp_length_px(kids[0].style.padding.left).unwrap_or(0.0) > 0.0,
        "padding must survive take_packed"
    );
}

#[test]
fn interactive_shell_is_preserved() {
    let mut btn =
        Element::from(vstack().children(vec![Element::new().text("A"), Element::new().text("B")]));
    btn = btn.on_click(|| {});
    let kids = take_packed(btn);
    assert_eq!(kids.len(), 1);
    assert_eq!(kids[0].children.len(), 2);
}

#[test]
fn vacuous_leaf_and_fragment_normalize_to_none() {
    assert!(is_vacuous_element(&Element::new()));
    let mut frag = Element::new();
    frag.set_attr(ATTR_FRAGMENT, "1");
    assert!(is_vacuous_element(&frag));
    assert!(children_none_if_vacuous(None).is_none());
    assert!(children_none_if_vacuous(Some(Element::new())).is_none());
    assert!(children_none_if_vacuous(Some(frag)).is_none());
}

#[test]
fn vacuous_whitespace_text_is_absent() {
    assert!(is_text_content_leaf(&Element::new().text("Cancel")));
    assert!(!is_text_content_leaf(&Element::new().text("   ")));
    assert!(is_vacuous_element(&Element::new().text("   ")));
}

#[test]
fn non_vacuous_children_survive_normalization() {
    let el = Element::new().text("OK");
    let out = children_none_if_vacuous(Some(el));
    assert_eq!(out.unwrap().text.as_deref(), Some("OK"));
}

#[test]
fn reactive_placeholder_is_not_vacuous() {
    let ph = Element::new()
        .template_id("examples/gallery/MessageScrollerShowcase:rows")
        .template_hash("__jadix_reactive_placeholder_v1")
        .w("100%")
        .h("max-content");
    assert!(
        !is_vacuous_element(&ph),
        "empty reactive placeholder must survive ui! children_none_if_vacuous"
    );
    let kept = children_none_if_vacuous(Some(ph)).expect("placeholder dropped");
    assert_eq!(
        kept.template_id.as_deref(),
        Some("examples/gallery/MessageScrollerShowcase:rows")
    );
}
