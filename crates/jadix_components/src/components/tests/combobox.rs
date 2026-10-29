use super::*;
use crate::core::reactivity::create_root;
use crate::utils::popup::{
    mark_floating, ATTR_FLOATING, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_TRIGGER,
};

#[test]
fn opened_menu_matches_trigger_width_in_portal() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use std::{cell::RefCell, rc::Rc};
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));
    let (owner, _) = create_root(|| {
        let open = use_signal("cb-width-open", || true);
        let live = Rc::clone(&live);
        crate::core::reactivity::create_effect(move || {
            let field = combobox_scope(
                "cb-width-regression",
                Some(open),
                None,
                "Search",
                false,
                "",
                "",
                "",
                || {
                    Element::new()
                        .child(crate::style::with_class_list(
                            ComboboxInput(ComboboxInputProps::default()),
                            "w-full",
                        ))
                        .child(combobox_content(ComboboxContentOpts::default(), || {
                            Element::new().text("Alpha")
                        }))
                },
            );
            *live.borrow_mut() = Element::new().w(600.0).h(400.0).child(field);
        });
    });
    drain_deferred_view_remounts();
    {
        let mut root = live.borrow_mut();
        crate::layout::layout_tree::hoist_portal_subtrees(&mut root);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        tree.build_and_layout(&mut root, 600.0, 400.0, [-1.0; 2]);
        tree.build_and_layout_fast_ex(&mut root, 600.0, 400.0, [-1.0; 2], false, false, None);
        let panel = crate::core::tree::find_in_tree(&root, |el| {
            el.attrs.contains_key(ATTR_FLOATING_MATCH_TRIGGER)
        })
        .expect("open floating panel");
        assert!(
            panel.layout_bounds[2] >= 250.0,
            "menu squeezed: {:?}",
            panel.layout_bounds
        );
    }
    clear_test_reconcile_root();
    drop(owner);
    reset_registered_anchors();
}

#[test]
fn combobox_class_overrides_default_16rem_shell() {
    let plain: Element = combobox("cb-plain").options([("a", "A")]).into();
    let sized: Element = combobox("cb-w")
        .options([("a", "A")])
        .class("w-[4.75rem] min-w-[4.75rem]")
        .into();
    match crate::layout::dim_kind(plain.style.size.width) {
        crate::layout::DimKind::Length(px) => {
            assert!(
                (px - 16.0 * 16.0).abs() < 0.5,
                "default shell 16rem, got {px}"
            )
        }
        other => panic!("default combobox shell should be 16rem, got {other:?}"),
    }
    match crate::layout::dim_kind(sized.style.size.width) {
        crate::layout::DimKind::Length(px) => assert!(
            (px - 4.75 * 16.0).abs() < 0.5,
            "class should set 4.75rem, got {px}"
        ),
        other => panic!("expected 4.75rem width, got {other:?}"),
    }
}

#[test]
fn combobox_input_class_overrides_default_16rem_shell() {
    let (_owner, el) = create_root(|| {
        combobox_scope(
            "cb-input-w",
            None,
            None,
            "Search…",
            false,
            String::new(),
            String::new(),
            String::new(),
            || {
                let trigger = ComboboxInput(ComboboxInputProps {
                    placeholder: None,
                    clearable: None,
                    children: None,
                    base: None,
                });
                crate::style::with_class_list(trigger, "w-[4.75rem] min-w-[4.75rem]")
            },
        )
    });
    match crate::layout::dim_kind(el.style.size.width) {
        crate::layout::DimKind::Length(px) => assert!(
            (px - 4.75 * 16.0).abs() < 0.5,
            "ComboboxInput class should set 4.75rem shell, got {px}"
        ),
        other => panic!("expected 4.75rem width from Input class, got {other:?}"),
    }
}

#[test]
fn selected_value_mount_does_not_reenter_input_sync() {
    use crate::core::reactivity::create_effect;
    create_root(|| {
        create_effect(|| {
            let val = use_signal("cb-sync-val", || "sh".to_string());
            let _el: Element = combobox("cb-sync")
                .value_signal(val)
                .options([("sh", "Shanghai"), ("bj", "Beijing")])
                .into();
        });
    });
}

#[test]
fn sibling_combobox_scopes_keep_distinct_context() {
    use crate::core::reactivity::create_effect;
    use std::cell::RefCell;
    use std::rc::Rc;
    let (_owner, (id_h, id_s)) = create_root(|| {
        let id_h = Rc::new(RefCell::new(String::new()));
        let id_s = Rc::new(RefCell::new(String::new()));
        let _h = combobox_scope("cb-hms-h", None, None, "HH", false, "", "", "", {
            let id_h = id_h.clone();
            move || {
                create_effect(move || {
                    *id_h.borrow_mut() = require_combobox("sibling-h").id.clone();
                });
                Element::new()
            }
        });
        let _s = combobox_scope("cb-hms-s", None, None, "SS", false, "", "", "", {
            let id_s = id_s.clone();
            move || {
                create_effect(move || {
                    *id_s.borrow_mut() = require_combobox("sibling-s").id.clone();
                });
                Element::new()
            }
        });
        (id_h, id_s)
    });
    assert_eq!(id_h.borrow().as_str(), "cb-hms-h");
    assert_eq!(id_s.borrow().as_str(), "cb-hms-s");
}

#[test]
fn active_index_follows_zero_padded_value() {
    let items: Vec<ComboboxVisibleItem> = (0..=23)
        .map(|i| ComboboxVisibleItem {
            value: format!("{i:02}"),
            label: format!("{i:02}"),
            disabled: false,
        })
        .collect();
    assert_eq!(active_index_for_value(&items, "09"), 9);
    assert_eq!(active_index_for_value(&items, "00"), 0);
    assert_eq!(active_index_for_value(&[], "09"), 9);
    assert_eq!(active_index_for_value(&items, "xx"), 0);
}

#[test]
fn combobox_input_is_not_a_structure_island() {
    let (_owner, el) = create_root(|| {
        combobox("cb_n4")
            .options([("a", "Alpha")])
            .clearable(true)
            .into()
    });
    fn find_tid_suffix(el: &Element, suffix: &str) -> bool {
        el.template_id
            .as_deref()
            .is_some_and(|t| t.ends_with(suffix))
            || el.children.iter().any(|c| find_tid_suffix(c, suffix))
    }
    assert!(
        !find_tid_suffix(&el, "__combo_input"),
        "ComboboxInput must mount the field once, not a reactive_anchor island"
    );
    assert!(
        find_tid_suffix(&el, "__view"),
        "ComboboxContent must still use the anchored overlay island"
    );
}

#[test]
fn combobox_builds() {
    let _el: Element = combobox("cb1")
        .options([("a", "Alpha"), ("b", "Beta")])
        .into();
}

#[test]
fn combobox_select_writes_label_to_input_and_value_id() {
    let (_owner, _) = create_root(|| {
        let sig = use_signal("cb_sel_val", || "a".to_string());
        let _el: Element = combobox("cb_sel")
            .value_signal(sig)
            .item(ComboboxItemData::new("a", "Alpha"))
            .item(ComboboxItemData::new("b", "Beta Long Name"))
            .into();
        let store = use_signal(input_value_signal_key("cb_sel").as_str(), String::new);
        assert_eq!(sig.peek(), "a");
        assert_eq!(
            store.peek(),
            "Alpha",
            "input must show full label, not value id"
        );
    });
}

#[test]
fn combobox_group_entries_collect_labels() {
    let entries = vec![
        ComboboxEntry::Group(
            ComboboxGroupData::new("g1")
                .label("G")
                .item(ComboboxItemData::new("x", "Xray")),
        ),
        ComboboxEntry::Separator,
        ComboboxEntry::Item(ComboboxItemData::new("y", "Yankee")),
    ];
    let map = collect_labels(&entries);
    assert_eq!(map.get("x").map(String::as_str), Some("Xray"));
    assert_eq!(map.get("y").map(String::as_str), Some("Yankee"));
}

#[test]
fn combobox_menu_chrome_marks_floating() {
    let mut menu = Element::new().absolute();
    mark_floating(&mut menu, "cb_c", "input:cb_c", PopupPlacement::BottomStart);
    menu.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
    assert_eq!(
        menu.attrs.get(ATTR_FLOATING).map(|s| s.as_str()),
        Some("cb_c")
    );
    assert_eq!(
        menu.attrs.get(ATTR_FLOATING_TRIGGER).map(|s| s.as_str()),
        Some("input:cb_c")
    );
    assert_eq!(
        menu.attrs
            .get(ATTR_FLOATING_MATCH_TRIGGER)
            .map(|s| s.as_str()),
        Some("1")
    );
}
