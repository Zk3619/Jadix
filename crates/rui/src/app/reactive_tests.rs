use crate::app::reactive::{for_keyed, show, show_scoped};
use crate::app::state::AppState;
use crate::core::tree::{Element, LayoutStatus};

fn child(id: &str, text: &str) -> Element {
    let mut el = Element::new().template_id(id.to_string());
    el.text = Some(text.to_string());
    el
}

#[test]
fn show_false_returns_empty_element() {
    let el = show(false, || {
        let mut e = Element::new();
        e.text = Some("x".to_string());
        e
    });
    assert!(el.text.is_none());
    assert!(el.children.is_empty());
}

#[test]
fn show_true_renders_element() {
    let el = show(true, || {
        let mut e = Element::new().template_id("show:true");
        e.text = Some("visible".to_string());
        e
    });
    assert_eq!(el.template_id.as_deref(), Some("show:true"));
    assert_eq!(el.text.as_deref(), Some("visible"));
}

#[test]
fn for_keyed_assigns_stable_template_id() {
    let items = vec![1_i32, 2, 3];
    let out = for_keyed("scope:test", &items, |v| *v, |v| {
        let mut e = Element::new();
        e.text = Some(format!("item-{v}"));
        e
    });
    let ids: Vec<String> = out
        .iter()
        .map(|e| e.template_id.clone().unwrap_or_default())
        .collect();
    assert_eq!(
        ids,
        vec![
            "scope:test:1".to_string(),
            "scope:test:2".to_string(),
            "scope:test:3".to_string()
        ]
    );
}

#[test]
fn keyed_reconcile_reorder_does_not_structure_change() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("k:a", "a"), child("k:b", "b")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![child("k:b", "b"), child("k:a", "a")];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_ne!(status, LayoutStatus::StructureChanged);
}

#[test]
fn keyed_reconcile_insert_remove_is_structure_changed() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("k:a", "a"), child("k:b", "b")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![child("k:a", "a"), child("k:c", "c")];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
}

#[test]
fn show_toggle_changes_structure() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![show(true, || child("toggle:block", "shown"))];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![show(false, || child("toggle:block", "shown"))];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
}

#[test]
fn show_scoped_toggle_keeps_unkeyed_sibling_intact() {
    // Mirrors `<Reactive>` + `<HStack>` (no template_id) + `<Show>`: positional reconcile must not
    // drop the button row when the Show branch toggles.
    fn row_like() -> Element {
        let mut row = Element::new();
        row.children.push({
            let mut leaf = Element::new();
            leaf.text = Some("btn".to_string());
            leaf
        });
        row
    }

    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![
        row_like(),
        show_scoped("show:anchor", true, || child("inner", "x")),
    ];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![
        row_like(),
        show_scoped("show:anchor", false, || child("inner", "x")),
    ];

    let _ = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(new_root.children.len(), 2);
    assert_eq!(new_root.children[0].children.len(), 1);
    assert_eq!(new_root.children[0].children[0].text.as_deref(), Some("btn"));
    assert!(new_root.children[1].children.is_empty());
}

#[test]
fn show_scoped_toggle_keeps_sibling_intact() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![
        child("sibling:hstack", "buttons"),
        show_scoped("show:anchor", true, || child("toggle:block", "shown")),
    ];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![
        child("sibling:hstack", "buttons"),
        show_scoped("show:anchor", false, || child("toggle:block", "shown")),
    ];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(new_root.children[0].text.as_deref(), Some("buttons"));
    assert_ne!(status, LayoutStatus::Clean);
}

#[test]
fn reactive_placeholder_matches_anchor_template_id_contract() {
    let ph = crate::app::reactive::reactive_placeholder("contract:anchor");
    assert_eq!(ph.template_id.as_deref(), Some("contract:anchor"));
    assert_eq!(
        ph.template_hash.as_deref(),
        Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH)
    );
}

#[test]
fn reactive_placeholder_in_root_rebuild_keeps_mounted_subtree() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("reactive:anchor", "buttons+show")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![
        Element::new()
            .template_id("reactive:anchor")
            .template_hash(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH),
    ];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::Clean);
    assert_eq!(new_root.children[0].text.as_deref(), Some("buttons+show"));
}

/// Root rebuild must not strip the reactive root wrapper (only moving `children` would).
#[test]
fn reactive_placeholder_root_rebuild_keeps_wrapper_element() {
    let mut mounted = Element::new().template_id("reactive:anchor");
    mounted.template_hash = Some("inner-vstack-hash".into());
    mounted.children = vec![child("row", "buttons")];

    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![mounted];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![
        Element::new()
            .template_id("reactive:anchor")
            .template_hash(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH),
    ];

    let status = AppState::reconcile_elements_for_test(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::Clean);
    assert_eq!(
        new_root.children[0].template_hash.as_deref(),
        Some("inner-vstack-hash")
    );
    assert_eq!(new_root.children[0].children.len(), 1);
    assert_eq!(new_root.children[0].children[0].text.as_deref(), Some("buttons"));
}
