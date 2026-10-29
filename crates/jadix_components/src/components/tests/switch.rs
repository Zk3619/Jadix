use super::*;
#[test]
fn checked_sync_preserves_track_damage_identity() {
    crate::core::reactivity::create_root(|| {
        let checked = crate::core::reactivity::use_signal("switch-damage-regression", || false);
        let mut root: Element = switch("damage-switch").checked_signal(checked).into();
        let mut tree = LayoutTree::new();
        tree.build_and_layout(&mut root, 240.0, 80.0, [-1.0; 2]);
        let bounds = root.layout_bounds;
        crate::app::damage::take_frame_paint_nodes();
        checked.set(true);
        let mut impact = LayoutStatus::Clean;
        sync_switch_checked_subtree(&mut root, &mut tree, &mut impact);
        let dirties = crate::app::damage::take_frame_paint_nodes();
        assert!(dirties
            .iter()
            .any(|dirty| dirty.key == crate::app::damage::paint_element_key(&root)));
        assert_eq!(root.layout_bounds, bounds);
        assert_eq!(impact, LayoutStatus::StyleChanged);
    });
}

#[cfg(feature = "a11y")]
use crate::a11y::bridge::{node_id_for_key, tree_update_from_elements};
use crate::components::label::{label, ATTR_LABEL_FOR};
#[cfg(feature = "a11y")]
use accesskit::Role;

#[test]
fn switch_string_child_builds_labelled_row() {
    let el: Element = switch("")
        .child(Element::new().text("Airplane Mode"))
        .into();
    let ctrl = el
        .children
        .iter()
        .find(|c| c.switch_checked_sync.is_some())
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
    assert_eq!(lbl.text.as_deref(), Some("Airplane Mode"));
}

#[test]
fn switch_nested_label_wires_for() {
    let el: Element = switch("air")
        .child(Element::from(label("").text("Airplane Mode")))
        .into();
    let ctrl = el
        .children
        .iter()
        .find(|c| c.switch_checked_sync.is_some())
        .expect("control");
    let lbl = el
        .children
        .iter()
        .find(|c| c.attrs.get("accesskit_role").map(|s| s.as_str()) == Some("label"))
        .expect("label");
    assert_eq!(
        lbl.attrs.get(ATTR_LABEL_FOR).map(|s| s.as_str()),
        Some("air")
    );
    assert_eq!(ctrl.focus_id.as_deref(), Some("air"));
}

#[test]
fn switch_thumb_starts_off_at_origin() {
    let el: Element = switch("off").into();
    let thumb = el
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_THUMB)
                .map(|s| s.as_str() == SLOT_THUMB)
                .unwrap_or(false)
        })
        .expect("thumb");
    assert!((thumb.style.translate[0]).abs() < 0.01);
}

#[test]
fn switch_thumb_starts_on_at_travel() {
    let el: Element = switch("on").default_checked(true).into();
    let thumb = el
        .children
        .iter()
        .find(|c| {
            c.attrs
                .get(ATTR_THUMB)
                .map(|s| s.as_str() == SLOT_THUMB)
                .unwrap_or(false)
        })
        .expect("thumb");
    assert!((thumb.style.translate[0] - THUMB_TRAVEL).abs() < 0.01);
}

#[cfg(feature = "a11y")]
#[test]
fn switch_caption_exposes_labelled_by_in_accesskit() {
    let el: Element = switch("a11y_sw")
        .child(Element::new().text("Airplane Mode"))
        .into();
    let root = Element::new().child(el);
    let update = tree_update_from_elements(&root, None);
    let lid = node_id_for_key("lbl:for:a11y_sw");
    let cid = node_id_for_key("a11y_sw");
    let label = update
        .nodes
        .iter()
        .find(|(i, _)| *i == lid)
        .map(|(_, n)| n)
        .expect("label node");
    let sw = update
        .nodes
        .iter()
        .find(|(i, _)| *i == cid)
        .map(|(_, n)| n)
        .expect("switch");
    assert_eq!(label.role(), Role::Label);
    assert_eq!(sw.role(), Role::Switch);
    assert_eq!(sw.labelled_by(), &[lid]);
}
