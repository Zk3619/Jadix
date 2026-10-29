use super::*;
#[cfg(feature = "a11y")]
use crate::a11y::bridge::{node_id_for_key, tree_update_from_elements};
use crate::components::label::{label, ATTR_LABEL_FOR};
#[cfg(feature = "a11y")]
use accesskit::Role;

fn click_at(root: &Element, point: [f32; 2]) -> bool {
    let path = crate::app::pointer_events::hit_path(root, point[0], point[1]);
    let (callback, _) = crate::app::pointer_events::resolve_click_from_path(root, &path);
    if let Some(callback) = callback {
        callback.borrow_mut()();
        true
    } else {
        false
    }
}

#[test]
fn checkbox_sibling_title_does_not_stretch_to_description_width() {
    let checked = crate::core::reactivity::use_signal("cb-title-hit", || false);
    checked.set(false);
    let mut root: Element = hstack()
        .gap(8.0)
        .items("start")
        .child(checkbox("cb-title-hit").checked_signal(checked))
        .child(
            Element::new()
                .flex_col()
                .gap(4.0)
                .child(
                    label("")
                        .text("Enable notifications")
                        .for_target("cb-title-hit"),
                )
                .child(label("").text("You can enable or disable notifications at any time.")),
        )
        .into();
    LayoutTree::new().build_and_layout(&mut root, 640.0, 160.0, [-1.0, -1.0]);
    let title = root.children[1].children[0].layout_bounds;
    let description = root.children[1].children[1].layout_bounds;
    assert!(
        description[2] > title[2] + 40.0,
        "title stretched: {title:?}, description: {description:?}"
    );
    assert!(!click_at(
        &root,
        [
            description[0] + description[2] - 4.0,
            title[1] + title[3] * 0.5
        ]
    ));
    assert!(!checked.peek());
    assert!(click_at(&root, [title[0] + 4.0, title[1] + title[3] * 0.5]));
    assert!(checked.peek());
    assert!(!click_at(
        &root,
        [description[0] + 4.0, description[1] + description[3] * 0.5]
    ));
}

#[test]
fn checkbox_wrapped_option_clicks_include_gap_description_and_padding() {
    for disabled in [false, true] {
        let checked = crate::core::reactivity::use_signal("cb-option-hit", || false);
        checked.set(false);
        let option: Element = checkbox("cb-option-hit")
            .checked_signal(checked)
            .disabled(disabled)
            .child(
                Element::new()
                    .flex_col()
                    .gap(4.0)
                    .child(
                        label("")
                            .text("Enable notifications")
                            .for_target("cb-option-hit"),
                    )
                    .child(label("").text("You can enable or disable notifications at any time.")),
            )
            .into();
        let mut root = Element::new()
            .w(640.0)
            .h(180.0)
            .flex_col()
            .gap(24.0)
            .child(option.p(12.0).flex_items("start"))
            .child(checkbox("cb-next-option").child(Element::new().text("Next option")));
        LayoutTree::new().build_and_layout(&mut root, 640.0, 180.0, [-1.0, -1.0]);
        let option = &root.children[0];
        let control = &option.children[0];
        assert!(control.focusable == !disabled);
        let box_bounds = control.layout_bounds;
        let title = option.children[1].children[0].layout_bounds;
        let description = option.children[1].children[1].layout_bounds;
        let bounds = option.layout_bounds;
        assert!(
            bounds[2] < 640.0 - 40.0,
            "default option must hug its content: {bounds:?}"
        );
        let points = [
            [box_bounds[0] + 8.0, box_bounds[1] + 8.0],
            [box_bounds[0] + box_bounds[2] + 4.0, box_bounds[1] + 8.0],
            [title[0] + 4.0, title[1] + title[3] * 0.5],
            [description[0] + 4.0, description[1] + description[3] * 0.5],
            [bounds[0] + bounds[2] - 4.0, title[1] + title[3] * 0.5],
            [bounds[0] + 4.0, bounds[1] + 4.0],
        ];
        for point in points {
            checked.set(false);
            assert!(click_at(&root, point), "option must own {point:?}");
            assert_eq!(
                checked.peek(),
                !disabled,
                "click must toggle exactly once at {point:?}"
            );
        }
        assert!(!click_at(
            &root,
            [bounds[0] + 4.0, bounds[1] + bounds[3] + 12.0]
        ));
    }
}

#[test]
fn checkbox_string_child_builds_labelled_row() {
    let el: Element = checkbox("")
        .child(Element::new().text("Accept terms"))
        .into();
    let ctrl = el
        .children
        .iter()
        .find(|c| c.checkbox_sync.is_some())
        .expect("control");
    let lbl = el
        .children
        .iter()
        .find(|c| c.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label"))
        .expect("caption label");
    assert_eq!(
        lbl.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
        ctrl.focus_id.as_deref()
    );
    let aid = lbl
        .attrs
        .get("accesskit_id")
        .cloned()
        .expect("accesskit_id");
    assert_eq!(
        ctrl.attrs.get("aria-labelledby").map(|s| s.as_str()),
        Some(aid.as_str())
    );
    assert_eq!(lbl.text.as_deref(), Some("Accept terms"));
}

#[test]
fn checkbox_nested_label_wires_for() {
    let el: Element = checkbox("terms-nested")
        .child(Element::from(label("").text("Nested label")))
        .into();
    let ctrl = el
        .children
        .iter()
        .find(|c| c.checkbox_sync.is_some())
        .expect("control");
    let lbl = el
        .children
        .iter()
        .find(|c| is_label_element(c))
        .expect("label");
    assert_eq!(
        lbl.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
        Some("terms-nested")
    );
    assert_eq!(ctrl.focus_id.as_deref(), Some("terms-nested"));
}

#[cfg(feature = "a11y")]
#[test]
fn checkbox_caption_exposes_labelled_by_in_accesskit() {
    let el: Element = checkbox("a11y-cb-caption")
        .child(Element::new().text("Read me"))
        .into();
    let root = Element::new().child(el);
    let update = tree_update_from_elements(&root, None);
    let lid = node_id_for_key("lbl:for:a11y-cb-caption");
    let cid = node_id_for_key("a11y-cb-caption");
    let label = update
        .nodes
        .iter()
        .find(|(i, _)| *i == lid)
        .map(|(_, n)| n)
        .expect("label node");
    let box_node = update
        .nodes
        .iter()
        .find(|(i, _)| *i == cid)
        .map(|(_, n)| n)
        .expect("checkbox node");
    assert_eq!(label.role(), Role::Label);
    assert_eq!(label.value(), Some("Read me"));
    assert_eq!(box_node.role(), Role::CheckBox);
    assert_eq!(box_node.labelled_by(), &[lid]);
}
