use crate::app::reactive::{
    for_keyed, for_reactive, reactive_component, show, show_reactive, show_scoped,
    REACTIVE_PLACEHOLDER_HASH,
};
use crate::app::reactive::{reactive_placeholder, reactive_view, reset_registered_anchors};
use crate::app::test_support::{is_bootstrap_root, reconcile_elements, BOOTSTRAP_ROOT_ATTR};
use crate::core::reactivity::{create_effect, dispose_effect, use_signal};
use crate::core::tree::{Element, LayoutStatus};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
    let out = for_keyed(
        "scope:test",
        &items,
        |v| *v,
        |v| {
            let mut e = Element::new();
            e.text = Some(format!("item-{v}"));
            e
        },
    );
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

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_ne!(status, LayoutStatus::StructureChanged);
}

#[test]
fn keyed_reconcile_insert_remove_is_structure_changed() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("k:a", "a"), child("k:b", "b")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![child("k:a", "a"), child("k:c", "c")];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
}

#[test]
fn keyed_reconcile_insert_head_middle_tail() {
    let cases: [(&str, Vec<(&str, &str)>); 3] = [
        ("head", vec![("k:a", "a"), ("k:b", "b")]),
        ("tail", vec![("k:b", "b"), ("k:c", "c")]),
        ("middle", vec![("k:a", "a"), ("k:b", "b"), ("k:c", "c")]),
    ];

    for (label, new_children) in cases {
        let mut old_root = Element::new().template_id("root");
        old_root.children = vec![child("k:b", "b")];

        let mut new_root = Element::new().template_id("root");
        new_root.children = new_children
            .into_iter()
            .map(|(id, text)| child(id, text))
            .collect();

        let status = reconcile_elements(&mut new_root, &mut old_root);
        assert_eq!(
            status,
            LayoutStatus::StructureChanged,
            "insert at {label} must structure-change"
        );
    }
}

#[test]
fn keyed_reconcile_delete_reports_structure_changed() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("k:a", "a"), child("k:b", "b"), child("k:c", "c")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![child("k:a", "a"), child("k:c", "c")];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
}

#[test]
fn keyed_reconcile_three_item_reorder_is_not_structure_changed() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("k:a", "a"), child("k:b", "b"), child("k:c", "c")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![child("k:c", "c"), child("k:a", "a"), child("k:b", "b")];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_ne!(status, LayoutStatus::StructureChanged);
    assert_eq!(new_root.children[0].text.as_deref(), Some("c"));
    assert_eq!(new_root.children[2].text.as_deref(), Some("b"));
}

#[test]
fn show_toggle_changes_structure() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![show(true, || child("toggle:block", "shown"))];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![show(false, || child("toggle:block", "shown"))];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
}

#[test]
fn show_scoped_toggle_keeps_unkeyed_sibling_intact() {
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

    let _ = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(new_root.children.len(), 2);
    assert_eq!(new_root.children[0].children.len(), 1);
    assert_eq!(
        new_root.children[0].children[0].text.as_deref(),
        Some("btn")
    );
    assert!(new_root.children[1].children.is_empty());
}

#[test]
fn show_reactive_when_does_not_subscribe_parent_effect() {
    reset_registered_anchors();
    let flag = use_signal("__rui_test_show_reactive_when", || false);
    flag.set_silent(false);
    let when_runs = Rc::new(Cell::new(0_u32));
    let root_runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let when_a = Rc::clone(&when_runs);
    let root_a = Rc::clone(&root_runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        root_a.set(root_a.get() + 1);
        let when_a = Rc::clone(&when_a);
        let view = show_reactive(
            "show:reactive:when",
            move || {
                when_a.set(when_a.get() + 1);
                flag.get()
            },
            || {
                let mut e = Element::new();
                e.text = Some("shown".to_string());
                e
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    assert_eq!(root_runs.get(), 1);
    assert!(when_runs.get() >= 1);

    let when_before = when_runs.get();
    flag.set(true);
    assert_eq!(
        root_runs.get(),
        1,
        "parent effect must not re-run on when signal"
    );
    assert!(
        when_runs.get() > when_before,
        "show_reactive when effect should re-run"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn for_reactive_list_does_not_subscribe_parent_effect() {
    reset_registered_anchors();
    let items = use_signal("__rui_test_for_reactive_items", || vec![1_i32, 2]);
    items.set_silent(vec![1, 2]);
    let list_runs = Rc::new(Cell::new(0_u32));
    let root_runs = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let list_a = Rc::clone(&list_runs);
    let root_a = Rc::clone(&root_runs);
    let mounted_a = Rc::clone(&mounted);
    let root = create_effect(move || {
        root_a.set(root_a.get() + 1);
        let list_a = Rc::clone(&list_a);
        let view = for_reactive("for:reactive:list", move || {
            list_a.set(list_a.get() + 1);
            let cur = items.get();
            for_keyed(
                "for:reactive:list:items",
                &cur,
                |v| *v,
                |v| {
                    let mut e = Element::new();
                    e.text = Some(format!("n={v}"));
                    e
                },
            )
        });
        *mounted_a.borrow_mut() = Some(view);
    });

    assert_eq!(root_runs.get(), 1);
    assert!(list_runs.get() >= 1);
    let before = list_runs.get();
    items.set(vec![1, 2, 3]);
    assert_eq!(root_runs.get(), 1, "parent must not re-run on list signal");
    assert!(list_runs.get() > before);

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
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

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(new_root.children[0].text.as_deref(), Some("buttons"));
    assert_ne!(status, LayoutStatus::Clean);
}

#[test]
fn reactive_placeholder_matches_anchor_template_id_contract() {
    let ph = reactive_placeholder("contract:anchor");
    assert_eq!(ph.template_id.as_deref(), Some("contract:anchor"));
    assert_eq!(ph.template_hash.as_deref(), Some(REACTIVE_PLACEHOLDER_HASH));
}

#[test]
fn reactive_view_unmount_allows_same_id_to_register_fresh_effect() {
    reset_registered_anchors();
    let renders = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let first_renders = Rc::clone(&renders);
    let first_mounted = Rc::clone(&mounted);
    let first_root_effect = create_effect(move || {
        let renders = Rc::clone(&first_renders);
        let view = reactive_view("test:remount", move || {
            renders.set(renders.get() + 1);
            Element::new()
        });
        *first_mounted.borrow_mut() = Some(view);
    });
    assert_eq!(renders.get(), 1);

    drop(mounted.borrow_mut().take());

    let second_renders = Rc::clone(&renders);
    let second_mounted = Rc::clone(&mounted);
    let second_root_effect = create_effect(move || {
        let renders = Rc::clone(&second_renders);
        let view = reactive_view("test:remount", move || {
            renders.set(renders.get() + 1);
            Element::new()
        });
        *second_mounted.borrow_mut() = Some(view);
    });
    assert_eq!(renders.get(), 2);

    dispose_effect(first_root_effect);
    dispose_effect(second_root_effect);
    reset_registered_anchors();
}

#[test]
fn bootstrap_root_attr_marks_private_startup_shell() {
    let mut bootstrap = Element::new().w("100%").h("100%");
    bootstrap.set_attr(BOOTSTRAP_ROOT_ATTR, "1");
    assert!(is_bootstrap_root(&bootstrap));

    let user_root = Element::new().w("100%").h("100%");
    assert!(!is_bootstrap_root(&user_root));
}

#[test]
fn reactive_placeholder_in_root_rebuild_keeps_mounted_subtree() {
    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![child("reactive:anchor", "buttons+show")];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![Element::new()
        .template_id("reactive:anchor")
        .template_hash(REACTIVE_PLACEHOLDER_HASH)];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::Clean);
    assert_eq!(new_root.children[0].text.as_deref(), Some("buttons+show"));
}

#[test]
fn reactive_placeholder_root_rebuild_keeps_wrapper_element() {
    let mut mounted = Element::new().template_id("reactive:anchor");
    mounted.template_hash = Some("inner-vstack-hash".into());
    mounted.children = vec![child("row", "buttons")];

    let mut old_root = Element::new().template_id("root");
    old_root.children = vec![mounted];

    let mut new_root = Element::new().template_id("root");
    new_root.children = vec![Element::new()
        .template_id("reactive:anchor")
        .template_hash(REACTIVE_PLACEHOLDER_HASH)];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::Clean);
    assert_eq!(
        new_root.children[0].template_hash.as_deref(),
        Some("inner-vstack-hash")
    );
    assert_eq!(new_root.children[0].children.len(), 1);
    assert_eq!(
        new_root.children[0].children[0].text.as_deref(),
        Some("buttons")
    );
}

#[test]
fn reactive_component_props_refresh_reruns_effect() {
    reset_registered_anchors();
    let runs = Rc::new(Cell::new(0_u32));
    let last = Rc::new(RefCell::new(String::new()));
    let mounted = Rc::new(RefCell::new(None::<Element>));

    let runs_a = Rc::clone(&runs);
    let last_a = Rc::clone(&last);
    let mounted_a = Rc::clone(&mounted);
    let _root_effect = create_effect(move || {
        let runs_a = Rc::clone(&runs_a);
        let last_a = Rc::clone(&last_a);
        let view = reactive_component("test:props", "alpha".to_string(), move |text| {
            runs_a.set(runs_a.get() + 1);
            *last_a.borrow_mut() = text;
            Element::new()
        });
        *mounted_a.borrow_mut() = Some(view);
    });
    assert_eq!(runs.get(), 1);
    assert_eq!(last.borrow().as_str(), "alpha");

    reactive_component("test:props", "beta".to_string(), move |_text| {
        Element::new()
    });
    assert_eq!(runs.get(), 2);
    assert_eq!(last.borrow().as_str(), "beta");

    drop(mounted.borrow_mut().take());
    dispose_effect(_root_effect);
    reset_registered_anchors();
}
