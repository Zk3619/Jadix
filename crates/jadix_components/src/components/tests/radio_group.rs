use super::*;
#[cfg(feature = "a11y")]
use crate::a11y::bridge::{node_id_for_key, tree_update_from_elements};
#[cfg(feature = "a11y")]
use accesskit::Role;

#[test]
fn options_builder_builds_circular_radios() {
    let el: Element = radio_group("tier")
        .default_value("free")
        .options(vec![("free", "Free"), ("pro", "Pro")])
        .into();
    assert_eq!(
        el.attrs.get("accesskit_role").map(|s| s.as_str()),
        Some("radiogroup")
    );
    let first = &el.children[0];
    let ctrl = first
        .children
        .iter()
        .find(|c| c.radio_selected_sync.is_some())
        .expect("control");
    assert_eq!(ctrl.focus_id.as_deref(), Some("tier:free"));
    assert_eq!(
        ctrl.attrs.get("accesskit_role").map(|s| s.as_str()),
        Some("radio")
    );
    let dot = ctrl
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_INDICATOR)
                .map(|s| s.as_str() == SLOT_INDICATOR)
                .unwrap_or(false)
        })
        .expect("indicator");
    // default free → on
    assert!(dot.style.bg_color[3] > 0.5);
}

#[test]
fn compound_item_requires_scope() {
    let el = radio_group_scope("plan", None, "a", false, || {
        RadioGroupItem(RadioGroupItemProps {
            value: Some("a".into()),
            disabled: None,
            children: Some(Element::new().text("A")),
            base: None,
        })
    });
    assert_eq!(
        el.attrs.get("accesskit_role").map(|s| s.as_str()),
        Some("radiogroup")
    );
    let item = &el.children[0];
    let ctrl = item
        .children
        .iter()
        .find(|c| c.radio_selected_sync.is_some())
        .expect("control");
    assert_eq!(ctrl.focus_id.as_deref(), Some("plan:a"));
}

#[cfg(feature = "a11y")]
#[test]
fn radio_exposes_role_in_accesskit() {
    let el: Element = radio_group("a11y_rg")
        .default_value("x")
        .options(vec![("x", "X")])
        .into();
    let root = Element::new().child(el);
    let update = tree_update_from_elements(&root, None);
    let gid = node_id_for_key("a11y_rg");
    let rid = node_id_for_key("a11y_rg:x");
    let group = update
        .nodes
        .iter()
        .find(|(i, _)| *i == gid)
        .map(|(_, n)| n)
        .expect("group");
    let radio = update
        .nodes
        .iter()
        .find(|(i, _)| *i == rid)
        .map(|(_, n)| n)
        .expect("radio");
    assert_eq!(group.role(), Role::RadioGroup);
    assert_eq!(radio.role(), Role::RadioButton);
}

#[test]
fn builder_item_style() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        radio_group_scope("rg-chrome", None, "", false, || {
            radio_group_item("a")
                .style(|b| b.text_size(20.0))
                .text("A")
                .into()
        })
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "radio_group_item .style(|b| b.text_size(20)) must land on the item"
    );
}

#[test]
fn builder_option_style_reaches_item() {
    use crate::core::reactivity::create_root;
    let (_owner, el): (_, Element) = create_root(|| {
        radio_group("rg-opt-chrome")
            .options([radio_option("a", "A").style(|b| b.text_size(20.0))])
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "radio_option .style must land on the assembled item"
    );
}
