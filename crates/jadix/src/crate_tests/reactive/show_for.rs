use crate::app::reactive::{
    bind_text, for_keyed, for_reactive, reactive_component, show, show_reactive, show_scoped,
    static_template_clone_hit_count, ATTR_SOURCE_TEMPLATE_ID, REACTIVE_PLACEHOLDER_HASH,
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
    assert_eq!(
        status,
        LayoutStatus::LayoutChanged,
        "same keys in a new order must dirty flex pack (fast layout never StructureChanged)"
    );
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
    assert_eq!(status, LayoutStatus::LayoutChanged);
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
    // No `install_test_reconcile_root`: island reconcile misses. Must not
    // `schedule_effect` into the in-flight deferred flush (100k-round loop).
    reset_registered_anchors();
    let flag = use_signal("__jadix_test_show_reactive_when", || false);
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
    let items = use_signal("__jadix_test_for_reactive_items", || vec![1_i32, 2]);
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
fn for_reactive_keyed_reorder_updates_live_child_order() {
    use crate::app::reactive::drain_deferred_view_remounts;
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::core::reactivity::{create_root, isolate_reads, Signal};

    reset_registered_anchors();
    clear_test_reconcile_root();
    let items = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (_owner, _) = create_root({
        let live = Rc::clone(&live);
        move || {
            create_effect(move || {
                *live.borrow_mut() = isolate_reads(|| {
                    for_reactive("fk-reorder", move || {
                        let cur = items.get();
                        for_keyed(
                            "fk-reorder:items",
                            &cur,
                            |s: &String| s.clone(),
                            |s| {
                                let mut e = Element::new();
                                e.text = Some(s.clone());
                                e
                            },
                        )
                    })
                });
            });
        }
    });
    drain_deferred_view_remounts();

    let texts = |el: &Element| {
        el.children
            .iter()
            .filter_map(|c| c.text.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        texts(&live.borrow()),
        vec!["Alpha".to_string(), "Beta".to_string()],
        "precondition: keyed mount order"
    );

    items.set(vec!["Beta".to_string(), "Alpha".to_string()]);
    drain_deferred_view_remounts();
    assert_eq!(
        texts(&live.borrow()),
        vec!["Beta".to_string(), "Alpha".to_string()],
        "for_keyed rebuild must reorder live children, not keep Alpha/Beta"
    );

    clear_test_reconcile_root();
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
fn placeholder_promoted_when_root_gains_portal_sibling() {
    // Gallery live root is `[chrome, unkeyed toast, __jadix_portal_layer]`.
    // `app_builder()` / force_reload only emits `[chrome, toast]`. Length
    // mismatch used to skip nested placeholder promotion and blank the page.
    let island_id = "examples/gallery/App:section_body";
    let mut filled = Element::new()
        .template_id(island_id)
        .template_hash("nav-hash");
    filled.set_attr(
        ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/main.rs:4334:21",
    );
    filled.children = vec![Element::new().text("导航")];

    let mut chrome = Element::new().template_id("examples/gallery/src/main.rs:4467:21");
    chrome.children = vec![filled];

    let mut old_root = Element::new();
    old_root.children = vec![
        chrome,
        Element::new(),
        Element::new().template_id("__jadix_portal_layer"),
    ];

    let mut new_chrome = Element::new().template_id("examples/gallery/src/main.rs:4467:21");
    new_chrome.children = vec![Element::new()
        .template_id(island_id)
        .template_hash(REACTIVE_PLACEHOLDER_HASH)];
    let mut new_root = Element::new();
    new_root.children = vec![new_chrome, Element::new()];

    let status = reconcile_elements(&mut new_root, &mut old_root);
    assert_eq!(status, LayoutStatus::StructureChanged);
    assert_eq!(
        new_root.children[0].children[0].children[0].text.as_deref(),
        Some("导航"),
        "filled section island must survive a portal-sibling length mismatch"
    );
    assert_eq!(
        new_root.children[0].children[0]
            .get_attr(ATTR_SOURCE_TEMPLATE_ID)
            .map(String::as_str),
        Some("examples/gallery/src/main.rs:4334:21"),
        "source ui! id must remain so a second hot-reload patch can match"
    );
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

#[test]
fn show_for_spot_check_button_toast_have_no_reactive_view() {
    let button = include_str!("../../../../jadix_components/src/components/button.rs");
    assert!(
        !button.contains("reactive_view("),
        "button must bind leaves, not wrap chrome in reactive_view"
    );
    let toast = include_str!("../../../../jadix_components/src/components/toast/mod.rs");
    assert!(
        !toast.contains("reactive_view("),
        "toast must not use reactive_view for text/variant"
    );
}

#[test]
fn show_for_spot_check_dialog_island_is_viewport_modal() {
    let dialog = include_str!("../../../../jadix_components/src/components/dialog.rs");
    assert!(
        dialog.contains("viewport_modal_island("),
        "Dialog open/close is a structure island"
    );
    assert!(
        dialog.contains("viewport_modal_layer"),
        "Dialog island must mount through the viewport_modal shell"
    );
}

#[test]
fn show_for_spot_check_tabs_strip_not_an_island() {
    let tabs = include_str!("../../../../jadix_components/src/components/tabs.rs");
    assert!(
        !tabs.contains("reactive_view("),
        "tabs strip must not rebuild interactive chrome on selection"
    );
    assert!(
        tabs.contains("reactive_anchor("),
        "selected panel remains a structure island"
    );
}

#[test]
fn show_for_spot_check_progress_track_not_an_island() {
    let progress = include_str!("../../../../jadix_components/src/components/progress.rs");
    assert!(
        !progress.contains("reactive_view("),
        "ProgressTrack fill is paint-only — bind_prop, not reactive_view"
    );
    assert!(
        progress.contains("apply_bound_size"),
        "fill size must patch via apply_bound_size"
    );
}

#[test]
fn show_for_spot_check_menubar_trigger_not_an_island() {
    let menubar = include_str!("../../../../jadix_components/src/components/menubar.rs");
    assert!(
        !menubar.contains("reactive_view("),
        "MenubarTrigger open accent is paint-only — bind_prop, not reactive_view"
    );
    assert!(
        menubar.contains("bind_prop("),
        "open chrome must patch via bind_prop"
    );
}

#[test]
fn unkeyed_for_warn_string_names_keyed_path() {
    let src = include_str!("../../app/reactive.rs");
    assert!(
        src.contains("has no key; each list change rebuilds the whole table"),
        "unkeyed For must debug-warn that it rebuilds the whole table"
    );
    assert!(
        src.contains("for_keyed_reactive"),
        "warn must point authors at for_keyed_reactive"
    );
}

#[test]
fn clone_static_skeleton_copies_text_not_layout_node() {
    let mut src = Element::new()
        .template_id("n6:leaf")
        .template_hash("n6-hash");
    src.text = Some("hello".to_string());
    src.focus_ring = true;
    src.layout_node = Some(taffy::prelude::NodeId::from(7u64));
    src.focus_ring = true;
    src.children.push({
        let mut c = Element::new();
        c.text = Some("kid".to_string());
        c
    });
    let cloned = src.clone_static_skeleton();
    assert_eq!(cloned.template_id.as_deref(), Some("n6:leaf"));
    assert_eq!(cloned.template_hash.as_deref(), Some("n6-hash"));
    assert_eq!(cloned.text.as_deref(), Some("hello"));
    assert!(cloned.focus_ring);
    assert!(
        cloned.layout_node.is_none(),
        "clone must not reuse taffy nodes"
    );
    assert!(cloned.focus_ring);
    assert_eq!(cloned.children.len(), 1);
    assert_eq!(cloned.children[0].text.as_deref(), Some("kid"));
}

#[test]
fn show_reactive_static_remount_clones_interned_template() {
    reset_registered_anchors();
    let open = use_signal("wp_n6_static_open", || false);
    open.set_silent(false);
    let renders = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let renders_e = Rc::clone(&renders);
    let mounted_a = Rc::clone(&mounted);

    let root = create_effect(move || {
        let renders_e = Rc::clone(&renders_e);
        let view = show_reactive(
            "wp_n6:static",
            move || open.get(),
            move || {
                renders_e.set(renders_e.get() + 1);
                let mut e = Element::new().template_id("wp_n6:panel");
                e.text = Some("panel".to_string());
                e
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    open.set(true);
    assert_eq!(renders.get(), 1, "first open builds the static body");
    let hits_after_first = static_template_clone_hit_count();

    open.set(false);
    open.set(true);
    assert_eq!(
        renders.get(),
        1,
        "static Show remount must clone interned skeleton, not re-run render"
    );
    assert!(
        static_template_clone_hit_count() > hits_after_first,
        "remount must count as a clone hit"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn show_reactive_bound_child_still_reruns_render() {
    reset_registered_anchors();
    let open = use_signal("wp_n6_bound_open", || false);
    open.set_silent(false);
    let n = use_signal("wp_n6_bound_n", || 1_i32);
    n.set_silent(1);
    let renders = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let renders_e = Rc::clone(&renders);
    let mounted_a = Rc::clone(&mounted);

    let root = create_effect(move || {
        let renders_e = Rc::clone(&renders_e);
        let view = show_reactive(
            "wp_n6:bound",
            move || open.get(),
            move || {
                renders_e.set(renders_e.get() + 1);
                bind_text("wp_n6:bound:text", move || n.get().to_string())
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    open.set(true);
    assert_eq!(renders.get(), 1);
    open.set(false);
    open.set(true);
    assert_eq!(
        renders.get(),
        2,
        "Show body with bind_* must re-run render so effects re-register"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}

#[test]
fn show_reactive_handler_body_still_reruns_render() {
    reset_registered_anchors();
    let open = use_signal("wp_n6_handler_open", || false);
    open.set_silent(false);
    let renders = Rc::new(Cell::new(0_u32));
    let mounted = Rc::new(RefCell::new(None::<Element>));
    let renders_e = Rc::clone(&renders);
    let mounted_a = Rc::clone(&mounted);

    let root = create_effect(move || {
        let renders_e = Rc::clone(&renders_e);
        let view = show_reactive(
            "wp_n6:handler",
            move || open.get(),
            move || {
                renders_e.set(renders_e.get() + 1);
                Element::new()
                    .template_id("wp_n6:handler:btn")
                    .text("click")
                    .on_click(|| {})
            },
        );
        *mounted_a.borrow_mut() = Some(view);
    });

    open.set(true);
    assert_eq!(renders.get(), 1);
    let hits_after_first = static_template_clone_hit_count();
    open.set(false);
    open.set(true);
    assert_eq!(
        renders.get(),
        2,
        "Show body with on_* must re-run render; interned clones must not share handlers"
    );
    assert_eq!(
        static_template_clone_hit_count(),
        hits_after_first,
        "handler bodies must not count as static template clone hits"
    );

    drop(mounted.borrow_mut().take());
    dispose_effect(root);
    reset_registered_anchors();
}
