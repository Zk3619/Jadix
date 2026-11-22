use super::*;
use crate::core::reactivity::create_root;
use crate::utils::popup::{
    ATTR_FLOATING, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_TRIGGER, mark_floating,
};

#[test]
fn combobox_cached_rows_preserve_selection_and_dynamic_unicode_labels() {
    let (_owner, ()) = create_root(|| {
        let open = Signal::new(true);
        let value = Signal::new("a".to_string());
        let _ = combobox_scope(
            "cb-cached",
            Some(open),
            Some(value),
            "Search",
            false,
            "",
            "",
            "",
            || {
                let ctx = require_combobox("cached-test");
                let index = Rc::new(RefCell::new(TextFilter::default()));
                let rows = RefCell::new(vec![("a", "ÉCOLE"), ("b", "Beta")]);
                let body = || {
                    Element::new().children(
                        rows.borrow()
                            .iter()
                            .map(|(value, label)| {
                                ComboboxItem(ComboboxItemProps {
                                    value: Some((*value).into()),
                                    children: Some(Element::new().text(*label)),
                                    ..Default::default()
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                };
                let paint = |query: &str| {
                    ctx.visible.set(Vec::new());
                    let root = filtered_rows(&ctx.id, query.into(), &index, &body);
                    if query == "éC" {
                        let label = crate::core::tree::find_in_tree(&root, |el| {
                            el.text.as_deref() == Some("ÉCOLE")
                        })
                        .expect("matching Unicode label");
                        assert_eq!(
                            label
                                .text_spans
                                .as_ref()
                                .unwrap()
                                .iter()
                                .filter(|span| span.color.is_some())
                                .map(|span| span.text.as_str())
                                .collect::<Vec<_>>(),
                            ["ÉC"]
                        );
                    }
                    ctx.visible.with_untracked(|list| {
                        list.iter()
                            .map(|item| item.value.clone())
                            .collect::<Vec<_>>()
                    })
                };
                assert_eq!(
                    paint("ÉCOLE"),
                    ["a", "b"],
                    "selected label keeps all choices"
                );
                assert_eq!(paint("éC"), ["a"]);
                assert_eq!(
                    paint("éC"),
                    ["a"],
                    "second active-row pass reuses the filter"
                );
                rows.borrow_mut()[1].1 = "Éclair";
                rows.borrow_mut().swap(0, 1);
                assert_eq!(paint("éCl"), ["b"]);
                rows.borrow_mut().truncate(1);
                assert!(paint("éCo").is_empty());
                open.set(false);
                assert_eq!(
                    paint("missing"),
                    ["b"],
                    "closed controls don't filter choices"
                );
                Element::new()
            },
        );
    });
}

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
fn combobox_navigation_survives_repaint_and_filtering() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use std::{cell::RefCell, rc::Rc};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    for all_disabled in [false, true] {
        reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let (owner, (open, value, active)) = create_root(|| {
            let open = use_signal("cb-nav__combo_open", || false);
            let value = use_signal("cb-nav__combo_value", || "a".to_string());
            let active = use_signal("cb-nav__combo_active", || 0usize);
            open.set_silent(false);
            value.set_silent("a".into());
            active.set_silent(0);
            let live = live.clone();
            crate::core::reactivity::create_effect(move || {
                *live.borrow_mut() = combobox("cb-nav")
                    .value_signal(value)
                    .item(ComboboxItemData::new("a", "Alpha").disabled(all_disabled))
                    .item(ComboboxItemData::new("b", "Beta").disabled(true))
                    .item(ComboboxItemData::new("c", "Charlie").disabled(true))
                    .item(ComboboxItemData::new("d", "Delta").disabled(all_disabled))
                    .into();
            });
            (open, value, active)
        });
        let press = |key: NamedKey| {
            let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.focus_id.as_deref() == Some("input:cb-nav")
            })
            .unwrap()
            .on_key_down
            .clone()
            .unwrap();
            assert!((handler.borrow_mut())(KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::Enter),
                logical_key: Key::Named(key),
                text: None,
                location: KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: None,
                key_without_modifiers: Key::Named(key),
            }));
            drain_deferred_view_remounts();
        };
        press(NamedKey::ArrowDown);
        assert!(open.peek());
        if all_disabled {
            assert_eq!(active.peek(), usize::MAX);
            press(NamedKey::ArrowDown);
            press(NamedKey::Enter);
            assert_eq!(value.peek(), "a");
            assert!(open.peek());
        } else {
            let inactive_background = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.get_attr("jadix_combobox_item").map(String::as_str) == Some("d")
            })
            .unwrap()
            .style
            .bg_color
            .clone();
            for (key, expected) in [
                (NamedKey::ArrowDown, 3),
                (NamedKey::ArrowDown, 0),
                (NamedKey::ArrowUp, 3),
                (NamedKey::Home, 0),
                (NamedKey::End, 3),
            ] {
                press(key);
                assert_eq!(active.peek(), expected);
                let background = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                    el.get_attr("jadix_combobox_item").map(String::as_str) == Some("d")
                })
                .unwrap()
                .style
                .bg_color
                .clone();
                assert_eq!(
                    background == inactive_background,
                    expected != 3,
                    "keyboard navigation updates the mounted row highlight"
                );
            }
            commit_value("cb-nav", "Del".into());
            drain_deferred_view_remounts();
            assert_eq!(
                use_signal(input_value_signal_key("cb-nav").as_str(), String::new).peek(),
                "Del"
            );
            assert!(open.peek(), "typing keeps the menu open");
            assert_eq!(active.peek(), 0, "retain Delta by value after filtering");
            assert!(
                crate::core::tree::find_in_tree(&live.borrow(), |el| {
                    el.get_attr("jadix_combobox_item").map(String::as_str) == Some("a")
                })
                .is_none(),
                "filtered-out rows leave the mounted tree"
            );
            assert!(
                crate::core::tree::find_in_tree(&live.borrow(), |el| {
                    el.get_attr("jadix_combobox_item").map(String::as_str) == Some("d")
                })
                .is_some(),
                "matching row remains mounted"
            );
            for (query, expected) in [("Del", "Del"), ("LT", "lt")] {
                commit_value("cb-nav", query.into());
                drain_deferred_view_remounts();
                let tree = live.borrow();
                let text = crate::core::tree::find_in_tree(&tree, |el| {
                    el.template_id.as_deref() == Some("cb-nav:opt:d:txt")
                })
                .expect("filtered label");
                assert_eq!(text.text.as_deref(), Some("Delta"));
                let spans = text.text_spans.as_ref().expect("query match spans");
                assert_eq!(
                    spans
                        .iter()
                        .filter(|span| span.color.is_some())
                        .map(|span| span.text.as_str())
                        .collect::<String>(),
                    expected
                );
                assert!(spans.iter().all(|span| span.color.is_none()
                    || span.color == Some(parse_text_color("match-foreground"))));
                assert!(
                    spans.iter().any(|span| span.color.is_none()),
                    "unmatched text inherits the row color"
                );
            }
            press(NamedKey::Enter);
            assert_eq!(value.peek(), "d");
            assert!(!open.peek());
        }
        clear_test_reconcile_root();
        drop(live);
        drop(owner);
        reset_registered_anchors();
    }
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

#[test]
fn builder_filters_five_thousand_options_before_constructing_rows() {
    fn count_nodes(element: &Element) -> usize {
        1 + element.children.iter().map(count_nodes).sum::<usize>()
    }
    let (_owner, ()) = create_root(|| {
        let open = Signal::new(true);
        let value = Signal::new(String::new());
        let _ = combobox_scope(
            "cb-prefilter",
            Some(open),
            Some(value),
            "Search",
            false,
            "",
            "",
            "",
            || {
                let ctx = require_combobox("prefilter-test");
                let index = Rc::new(RefCell::new(TextFilter::default()));
                let mut entries = vec![
                    ComboboxEntry::Separator,
                    ComboboxEntry::Group(ComboboxGroupData {
                        id: "large".into(),
                        label: Some("Large".into()),
                        items: (0..5000)
                            .map(|i| {
                                ComboboxItemData::new(format!("id-{i}"), format!("Entry {i:04}"))
                            })
                            .collect(),
                    }),
                    ComboboxEntry::Separator,
                    ComboboxEntry::Group(ComboboxGroupData::new("empty").label("Hidden group")),
                    ComboboxEntry::Separator,
                    ComboboxEntry::Item(ComboboxItemData::new("unicode", "École")),
                    ComboboxEntry::Separator,
                    ComboboxEntry::Item(ComboboxItemData::new("disabled", "Éclair").disabled(true)),
                    ComboboxEntry::Separator,
                ];
                let paint = |entries: &[ComboboxEntry], query: &str, expected: &[&str]| {
                    ctx.visible.set(Vec::new());
                    let tree = filtered_rows(&ctx.id, query.into(), &index, &|| {
                        build_builder_entries(Rc::new(entries.to_vec()))
                    });
                    assert_eq!(
                        ctx.visible.with_untracked(|items| items
                            .iter()
                            .map(|item| item.value.clone())
                            .collect::<Vec<_>>()),
                        expected
                    );
                    assert!(
                        count_nodes(&tree) < 100,
                        "rejected metadata must not leave zero-size Elements"
                    );
                    assert!(
                        crate::core::tree::find_in_tree(&tree, |el| el.text.as_deref()
                            == Some("Hidden group"))
                        .is_none()
                    );
                    tree
                };
                for query in ["Entry 4999", "Entry 4999", "4999"] {
                    paint(&entries, query, &["id-4999"]);
                }
                let tree = paint(&entries, "éC", &["unicode", "disabled"]);
                assert_eq!(active_index_for_value(&ctx.visible.peek(), "disabled"), 0);
                // Two surviving ungrouped options need exactly one separator, with
                // no orphaned heading, leading separator or trailing separator.
                assert_eq!(tree.children.len(), 4);
                assert!(
                    crate::core::tree::find_in_tree(&tree, |el| el.text.as_deref()
                        == Some("Large"))
                    .is_none()
                );
                if let ComboboxEntry::Group(group) = &mut entries[1] {
                    group.items[0].label = "Échange".into();
                    group.items.swap(0, 4999);
                    group.items.truncate(1);
                }
                paint(&entries, "Entry 4999", &["id-4999"]);
                if let ComboboxEntry::Group(group) = &mut entries[1] {
                    group.items[0].label = "Échange".into();
                }
                paint(&entries, "éCh", &["id-4999"]);
                paint(&entries, "missing", &[]);
                entries.drain(0..3);
                value.set("unicode".into());
                paint(&entries, "École", &["unicode", "disabled"]);
                open.set(false);
                paint(&entries, "missing", &["unicode", "disabled"]);
                Element::new()
            },
        );
    });
}

#[test]
fn builder_virtualizes_grouped_options_and_reveals_offscreen_keyboard_targets() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::app::test_support::sync_reactive_elements_subtree;
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key, scroll_target_y_signal_key};
    use crate::layout::layout_tree::{LayoutTree, hoist_portal_subtrees};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    fn mounted(root: &Element) -> Vec<String> {
        let mut values = Vec::new();
        if let Some(value) = root.attrs.get("jadix_combobox_item") {
            values.push(value.clone());
        }
        values.extend(root.children.iter().flat_map(mounted));
        values
    }
    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, (open, value, active, visible)) = create_root(|| {
        let open = use_signal("cb-virtual__combo_open", || false);
        let value = use_signal("cb-virtual__combo_value", String::new);
        let active = use_signal("cb-virtual__combo_active", || 0usize);
        let visible = use_signal("cb-virtual__combo_visible", Vec::<ComboboxVisibleItem>::new);
        let live = live.clone();
        create_effect(move || {
            let group = |range: std::ops::Range<usize>| ComboboxGroupData {
                id: format!("group-{}", range.start),
                label: Some(format!("Group {}", range.start)),
                items: range
                    .map(|i| {
                        ComboboxItemData::new(format!("id-{i}"), format!("Entry {i:04}"))
                            .disabled(i % 7 == 0)
                    })
                    .collect(),
            };
            let combo: Element = combobox("cb-virtual")
                .value_signal(value)
                .separator()
                .group(group(0..2500))
                .separator()
                .separator()
                .group(ComboboxGroupData::new("empty").label("Hidden group"))
                .group(group(2500..5000))
                .separator()
                .into();
            *live.borrow_mut() = Element::new().w(800.0).h(600.0).child(combo);
        });
        (open, value, active, visible)
    });
    let press = |key| {
        let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
            el.focus_id.as_deref() == Some("input:cb-virtual")
        })
        .unwrap()
        .on_key_down
        .clone()
        .unwrap();
        assert!((handler.borrow_mut())(KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::ArrowDown),
            logical_key: Key::Named(key),
            text: None,
            location: KeyLocation::Standard,
            state: ElementState::Pressed,
            repeat: false,
            text_with_all_modifiers: None,
            key_without_modifiers: Key::Named(key),
        }));
        drain_deferred_view_remounts();
    };
    press(NamedKey::ArrowDown);
    assert!(open.peek());
    assert_eq!(visible.peek().len(), 5000);
    assert_eq!(active.peek(), 1, "opening skips the disabled first option");
    assert!((1..30).contains(&mounted(&live.borrow()).len()));
    assert!(!mounted(&live.borrow()).contains(&"id-4999".into()));
    let mut layout = LayoutTree::new();
    let settle = |layout: &mut LayoutTree| {
        for _ in 0..3 {
            let mut host = live.borrow_mut();
            hoist_portal_subtrees(&mut host);
            let result = layout.build_and_layout(&mut host, 800.0, 600.0, [-1.0; 2]);
            // Match AppState's frame boundary: deferred navigation needs the
            // actual viewport and content metrics collected by the layout pass.
            for (rest, bounds, _, content) in result.scroll_metrics {
                use crate::core::scroll_viewport::ScrollViewportSignalKeys;
                let keys = ScrollViewportSignalKeys::new(&rest);
                for (key, value) in [
                    (&keys.viewport_w, bounds[2]),
                    (&keys.viewport_h, bounds[3]),
                    (&keys.content_w, content[0]),
                    (&keys.content_h, content[1]),
                ] {
                    use_signal(key, || 0.0_f32).set_silent(value);
                }
            }
            sync_reactive_elements_subtree(&mut host, layout, &mut LayoutStatus::Clean);
        }
    };
    settle(&mut layout);
    press(NamedKey::End);
    assert_eq!(active.peek(), 4999);
    settle(&mut layout);
    assert!(mounted(&live.borrow()).contains(&"id-4999".into()));
    assert!(mounted(&live.borrow()).len() < 30);
    crate::core::tree::walk_elements(&live.borrow(), |el| {
        if el.get_attr("id").map(String::as_str) == Some("cb-virtual:listbox") {
            assert_eq!(el.attrs["accesskit_size_of_set"], "5000");
        }
        if let Some(value) = el.get_attr("jadix_combobox_item") {
            let index = value.strip_prefix("id-").unwrap().parse::<usize>().unwrap();
            assert_eq!(el.attrs["aria-posinset"], (index + 1).to_string());
            assert_eq!(el.attrs["aria-setsize"], "5000");
            assert_eq!(el.attrs["aria-disabled"], (index % 7 == 0).to_string());
        }
    });
    #[cfg(feature = "a11y")]
    {
        let update = crate::a11y::bridge::tree_update_from_elements(&live.borrow(), None);
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.role() == accesskit::Role::ListBox
                    && node.size_of_set() == Some(5000))
        );
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.role() == accesskit::Role::ListBoxOption
                    && node.position_in_set() == Some(4999))
        );
    }
    let scroll_id = "cb-virtual:menu_scroll";
    let result = layout.build_and_layout(&mut live.borrow_mut(), 800.0, 600.0, [-1.0; 2]);
    assert_eq!(
        result
            .scroll_metrics
            .iter()
            .filter(|(id, ..)| id == scroll_id)
            .count(),
        1,
        "a virtual menu must have one scroll owner"
    );
    assert!(use_signal(&scroll_target_y_signal_key(scroll_id), || 0.0_f32).peek() > 1000.0);
    // A consumed keyboard reveal must not pull a wheel/drag back to the highlight.
    use_signal(&scroll_offset_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    use_signal(&scroll_target_y_signal_key(scroll_id), || 0.0_f32).set_silent(0.0);
    settle(&mut layout);
    assert!(!mounted(&live.borrow()).contains(&"id-4999".into()));
    press(NamedKey::ArrowUp);
    assert_eq!(active.peek(), 4997, "skip disabled offscreen option 4998");
    press(NamedKey::Enter);
    assert_eq!(value.peek(), "id-4997");
    assert!(!open.peek());
    // Filter across the virtualization threshold without retaining stale rows or headings.
    press(NamedKey::ArrowDown);
    commit_value("cb-virtual", "4999".into());
    drain_deferred_view_remounts();
    settle(&mut layout);
    assert_eq!(
        visible
            .peek()
            .iter()
            .map(|x| x.value.as_str())
            .collect::<Vec<_>>(),
        ["id-4999"]
    );
    assert_eq!(mounted(&live.borrow()), ["id-4999"]);
    let filtered = live.borrow();
    let option = crate::core::tree::find_in_tree(&filtered, |el| {
        el.get_attr("jadix_combobox_item").map(String::as_str) == Some("id-4999")
    })
    .unwrap();
    assert_eq!(option.attrs["aria-posinset"], "1");
    assert_eq!(option.attrs["aria-setsize"], "1");
    assert_eq!(
        crate::core::tree::find_in_tree(&filtered, |el| el.get_attr("id").map(String::as_str)
            == Some("cb-virtual:listbox"))
        .unwrap()
        .attrs["accesskit_size_of_set"],
        "1"
    );
    drop(filtered);
    assert!(
        crate::core::tree::find_in_tree(&live.borrow(), |el| el
            .attrs
            .contains_key("jadix_combobox_virtual"))
        .is_none()
    );
    press(NamedKey::Enter);
    assert_eq!(value.peek(), "id-4999");
    assert!(!open.peek());
    clear_test_reconcile_root();
    drop(live);
    drop(owner);
    reset_registered_anchors();
}

#[test]
fn combobox_keyboard_dismissal_keeps_input_and_accessibility_relations_live() {
    use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};

    reset_registered_anchors();
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, (open, value, active)) = create_root(|| {
        let open = use_signal("cb-semantics__combo_open", || false);
        let value = use_signal("cb-semantics__combo_value", String::new);
        let active = use_signal("cb-semantics__combo_active", || 0usize);
        let tree = live.clone();
        create_effect(move || {
            *tree.borrow_mut() = combobox("cb-semantics")
                .value_signal(value)
                .item(ComboboxItemData::new("a", "Alpha"))
                .item(ComboboxItemData::new("b", "Beta"))
                .item(ComboboxItemData::new("c", "Disabled").disabled(true))
                .into();
        });
        (open, value, active)
    });
    drain_deferred_view_remounts();
    let input_handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
        el.focus_id.as_deref() == Some("input:cb-semantics")
    })
    .unwrap()
    .on_key_down
    .clone()
    .unwrap();
    let press = |key: NamedKey| {
        let consumed = (input_handler.borrow_mut())(KeyEvent {
            physical_key: PhysicalKey::Code(KeyCode::Enter),
            logical_key: Key::Named(key),
            text: None,
            location: KeyLocation::Standard,
            state: ElementState::Pressed,
            repeat: false,
            text_with_all_modifiers: None,
            key_without_modifiers: Key::Named(key),
        });
        drain_deferred_view_remounts();
        consumed
    };
    let check = |expanded: bool, expected: Option<&str>, selected: Option<&str>| {
        let root = live.borrow();
        let input = crate::core::tree::find_in_tree(&root, |el| {
            el.focus_id.as_deref() == Some("input:cb-semantics")
        })
        .unwrap();
        assert!(Rc::ptr_eq(
            input.on_key_down.as_ref().unwrap(),
            &input_handler
        ));
        assert_eq!(
            input.get_attr("aria-expanded").map(String::as_str),
            Some(if expanded { "true" } else { "false" })
        );
        assert_eq!(
            input.get_attr("aria-controls").map(String::as_str),
            expanded.then_some("cb-semantics:listbox")
        );
        let descendant = input.get_attr("aria-activedescendant");
        assert_eq!(descendant.map(String::as_str), expected);
        if expanded {
            let list = crate::core::tree::find_in_tree(&root, |el| {
                el.get_attr("id").map(String::as_str) == Some("cb-semantics:listbox")
            })
            .unwrap();
            assert_eq!(
                list.get_attr("accesskit_role").map(String::as_str),
                Some("listbox")
            );
            crate::core::tree::walk_elements(list, |el| {
                if let Some(value) = el.get_attr("jadix_combobox_item") {
                    assert!(!el.focusable, "input owns keyboard focus");
                    assert_eq!(
                        el.get_attr("aria-selected").map(String::as_str),
                        Some(if Some(value.as_str()) == selected {
                            "true"
                        } else {
                            "false"
                        })
                    );
                }
            });
        }
    };
    check(false, None, None);
    assert!(
        !press(NamedKey::Escape),
        "closed Escape bubbles to the enclosing overlay"
    );
    assert!(press(NamedKey::ArrowUp));
    assert_eq!(active.peek(), 1, "reverse open skips the disabled last row");
    check(true, Some("cb-semantics:opt:b"), None);
    commit_value("cb-semantics", "Al".into());
    drain_deferred_view_remounts();
    check(true, Some("cb-semantics:opt:a"), None);
    assert!(
        !press(NamedKey::Tab),
        "Tab closes without swallowing focus traversal"
    );
    assert!(!open.peek());
    assert!(
        value.peek().is_empty(),
        "Tab does not commit the highlighted result"
    );
    check(false, None, None);
    assert!(press(NamedKey::ArrowDown));
    assert!(press(NamedKey::Enter));
    assert_eq!(value.peek(), "a");
    assert!(press(NamedKey::ArrowUp));
    check(true, Some("cb-semantics:opt:a"), Some("a"));
    assert!(press(NamedKey::Escape));
    check(false, None, None);
    assert!(!press(NamedKey::Escape));
    clear_test_reconcile_root();
    drop(live);
    drop(owner);
    reset_registered_anchors();
}

fn settle_controlled_combobox(
    root: &Rc<RefCell<Element>>,
    layout: &mut crate::layout::layout_tree::LayoutTree,
) {
    crate::app::reactive::drain_deferred_view_remounts();
    for _ in 0..3 {
        let mut root = root.borrow_mut();
        crate::layout::layout_tree::hoist_portal_subtrees(&mut root);
        layout.build_and_layout(&mut root, 800.0, 600.0, [-1.0; 2]);
        crate::app::test_support::sync_reactive_elements_subtree(
            &mut root,
            layout,
            &mut LayoutStatus::Clean,
        );
    }
    crate::authoring::presentation::sync_tree(
        &mut root.borrow_mut(),
        layout,
        crate::authoring::presentation::Context::new(
            crate::authoring::presentation::Phase::State,
            [800.0, 600.0],
        ),
    );
}

fn combobox_test_key(key: NamedKey) -> winit::event::KeyEvent {
    use winit::event::{ElementState, KeyEvent};
    use winit::keyboard::{KeyCode, KeyLocation, PhysicalKey};
    KeyEvent {
        physical_key: PhysicalKey::Code(KeyCode::Enter),
        logical_key: Key::Named(key),
        text: None,
        location: KeyLocation::Standard,
        state: ElementState::Pressed,
        repeat: false,
        text_with_all_modifiers: None,
        key_without_modifiers: Key::Named(key),
    }
}

fn click_combobox_test_id(root: &Rc<RefCell<Element>>, id: &str) {
    let click = crate::core::tree::find_in_tree(&root.borrow(), |el| {
        el.get_attr("id").map(String::as_str) == Some(id)
            || el.focus_id.as_deref() == Some(id)
            || el.template_id.as_deref() == Some(id)
    })
    .unwrap_or_else(|| panic!("missing {id}"))
    .on_click
    .clone()
    .expect("click handler");
    click.borrow_mut()();
}

#[test]
fn controlled_multi_selection_retains_query_and_bounds_virtual_rows() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::layout::layout_tree::LayoutTree;
    for count in [4, 100] {
        crate::app::reactive::reset_registered_anchors();
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let id = format!("cb-multi-{count}");
        let (owner, (selection, open, calls, builds)) = create_root(|| {
            let selection = Signal::new(vec!["absent".to_owned(), "id-0".to_owned()]);
            let open = Signal::new(true);
            let calls = Rc::new(RefCell::new(Vec::new()));
            let builds = Rc::new(std::cell::Cell::new(0));
            create_effect({
                let live = live.clone();
                let id = id.clone();
                let calls = calls.clone();
                let builds = builds.clone();
                move || {
                    builds.set(builds.get() + 1);
                    let calls = calls.clone();
                    *live.borrow_mut() = combobox(&id)
                        .open_signal(open)
                        .selection_signal(selection)
                        .clearable(true)
                        .options(
                            (0..count)
                                .skip(1)
                                .map(|i| (format!("id-{i}"), format!("Entry {i:04}"))),
                        )
                        .item(ComboboxItemData::new("id-0", "Disabled").disabled(true))
                        .on_change(move |event| {
                            calls
                                .borrow_mut()
                                .push((event, selection.peek(), open.peek()))
                        })
                        .into();
                }
            });
            (selection, open, calls, builds)
        });
        let mut layout = LayoutTree::new();
        settle_controlled_combobox(&live, &mut layout);
        let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
            el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
        })
        .unwrap()
        .on_key_down
        .clone()
        .unwrap();
        let selected = |value: &str| {
            crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.get_attr("id") == Some(&format!("{id}:opt:{value}"))
            })
            .unwrap()
            .attrs["aria-selected"]
                .clone()
        };
        if count == 4 {
            assert_eq!(selected("id-0"), "true");
            click_combobox_test_id(&live, &format!("{id}:opt:id-0"));
            assert!(calls.borrow().is_empty());
        }
        combobox_query(&id).set("Entry 0001");
        settle_controlled_combobox(&live, &mut layout);
        click_combobox_test_id(&live, &format!("{id}:opt:id-1"));
        assert_eq!(selection.peek(), ["absent", "id-0", "id-1"]);
        assert!(open.peek());
        assert_eq!(combobox_query(&id).get(), "Entry 0001");
        assert_eq!(
            calls.borrow()[0],
            (
                ComboboxChange {
                    value: "id-1".into(),
                    selected: true,
                    reason: ComboboxChangeReason::Pointer
                },
                selection.peek(),
                true
            )
        );
        settle_controlled_combobox(&live, &mut layout);
        assert_eq!(selected("id-1"), "true");
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert_eq!(selection.peek(), ["absent", "id-0"]);
        assert_eq!(calls.borrow()[1].0.reason, ComboboxChangeReason::Keyboard);
        assert!(!calls.borrow()[1].0.selected);
        combobox_query(&id).set("");
        selection.set(vec!["absent".into(), "id-2".into()]);
        settle_controlled_combobox(&live, &mut layout);
        assert_eq!(selected("id-2"), "true");
        assert_eq!(calls.borrow().len(), 2, "external writes do not notify");
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::End)));
        settle_controlled_combobox(&live, &mut layout);
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        settle_controlled_combobox(&live, &mut layout);
        assert_eq!(
            selection.peek(),
            ["absent", "id-2", format!("id-{}", count - 1).as_str()]
        );
        let mut mounted = 0;
        crate::core::tree::walk_elements(&live.borrow(), |el| {
            mounted += usize::from(el.attrs.contains_key("jadix_combobox_item"))
        });
        assert!(mounted <= count.min(30));
        let root = live.borrow();
        let list = crate::core::tree::find_in_tree(&root, |el| {
            el.get_attr("id") == Some(&format!("{id}:listbox"))
        })
        .unwrap();
        assert_eq!(list.attrs["aria-multiselectable"], "true");
        let input = crate::core::tree::find_in_tree(&root, |el| {
            el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
        })
        .unwrap();
        assert!(Rc::ptr_eq(input.on_key_down.as_ref().unwrap(), &handler));
        drop(root);
        click_combobox_test_id(&live, &format!("{id}__combo_clear"));
        assert!(selection.peek().is_empty());
        assert!(!open.peek());
        let calls = calls.borrow();
        assert_eq!(calls.last().unwrap().0.reason, ComboboxChangeReason::Clear);
        assert!(calls.last().unwrap().1.is_empty());
        assert!(calls.last().unwrap().2, "clear callback precedes closing");
        assert_eq!(builds.get(), 1, "selection/query update only the menu");
        clear_test_reconcile_root();
        drop(live);
        drop(owner);
        crate::app::reactive::reset_registered_anchors();
    }
}

#[test]
fn external_results_loading_and_creation_share_the_mounted_input() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::layout::layout_tree::LayoutTree;
    for filterable in [false, true] {
        crate::app::reactive::reset_registered_anchors();
        let id = format!("cb-results-{filterable}");
        let live = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(live.clone());
        let (owner, (items, loading, open, value, changes, created, builds)) = create_root(|| {
            let items = Signal::new(vec![ComboboxItemData::new("initial", "Initial label")]);
            let loading = Signal::new(false);
            let open = Signal::new(false);
            let value = Signal::new("initial".to_owned());
            let changes = Rc::new(RefCell::new(Vec::new()));
            let created = Rc::new(RefCell::new(Vec::new()));
            let builds = Rc::new(std::cell::Cell::new(0));
            create_effect({
                let live = live.clone();
                let id = id.clone();
                let changes = changes.clone();
                let created = created.clone();
                let builds = builds.clone();
                move || {
                    builds.set(builds.get() + 1);
                    let changes = changes.clone();
                    let created = created.clone();
                    *live.borrow_mut() = combobox(&id)
                        .open_signal(open)
                        .value_signal(value)
                        .items_signal(items)
                        .loading_signal(loading)
                        .filterable(filterable)
                        .on_change(move |event| {
                            changes
                                .borrow_mut()
                                .push((event, value.peek(), open.peek()))
                        })
                        .on_create(move |query| created.borrow_mut().push((query, open.peek())))
                        .into();
                }
            });
            (items, loading, open, value, changes, created, builds)
        });
        let mut layout = LayoutTree::new();
        settle_controlled_combobox(&live, &mut layout);
        assert_eq!(combobox_query(&id).get(), "Initial label");
        items.set(vec![ComboboxItemData::new("initial", "Renamed label")]);
        assert_eq!(
            combobox_query(&id).get(),
            "Renamed label",
            "closed labels follow external results"
        );
        let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
            el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
        })
        .unwrap()
        .on_key_down
        .clone()
        .unwrap();
        open.set(true);
        combobox_query(&id).set("custom query");
        loading.set(true);
        items.set(Vec::new());
        open.set(false);
        assert_eq!(combobox_query(&id).get(), "Renamed label");
        open.set(true);
        combobox_query(&id).set("custom query");
        settle_controlled_combobox(&live, &mut layout);
        let busy = crate::core::tree::find_in_tree(&live.borrow(), |el| {
            el.get_attr("id") == Some(&format!("{id}:listbox"))
        })
        .unwrap()
        .attrs["aria-busy"]
            .clone();
        assert_eq!(busy, "true");
        #[cfg(feature = "a11y")]
        {
            let update = crate::a11y::bridge::tree_update_from_elements(&live.borrow(), None);
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.role() == accesskit::Role::ListBox
                        && node.is_busy()
                        && node.size_of_set() == Some(0))
            );
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.role() == accesskit::Role::ComboBox
                        && node.auto_complete() == Some(accesskit::AutoComplete::List))
            );
        }
        assert!(
            crate::core::tree::find_in_tree(&live.borrow(), |el| el
                .get_attr("accesskit_role")
                .map(String::as_str)
                == Some("status"))
            .is_some()
        );
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert!(created.borrow().is_empty());
        assert!(open.peek());
        items.set(vec![
            ComboboxItemData::new("a", "Alpha"),
            ComboboxItemData::new("b", "Beta"),
        ]);
        loading.set(false);
        settle_controlled_combobox(&live, &mut layout);
        #[cfg(feature = "a11y")]
        {
            let update = crate::a11y::bridge::tree_update_from_elements(&live.borrow(), None);
            assert!(
                update
                    .nodes
                    .iter()
                    .any(|(_, node)| node.role() == accesskit::Role::ListBox
                        && !node.is_busy()
                        && node.size_of_set() == Some(if filterable { 0 } else { 2 }))
            );
        }
        assert_eq!(combobox_query(&id).get(), "custom query");
        let visible = use_signal(
            &format!("{id}__combo_visible"),
            Vec::<ComboboxVisibleItem>::new,
        );
        if filterable {
            assert!(visible.peek().is_empty());
            combobox_query(&id).set("Alpha");
            settle_controlled_combobox(&live, &mut layout);
        } else {
            assert_eq!(
                visible
                    .peek()
                    .iter()
                    .map(|x| x.value.as_str())
                    .collect::<Vec<_>>(),
                ["a", "b"]
            );
        }
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert_eq!(value.peek(), "a");
        assert!(!open.peek());
        assert_eq!(
            changes.borrow()[0],
            (
                ComboboxChange {
                    value: "a".into(),
                    selected: true,
                    reason: ComboboxChangeReason::Keyboard
                },
                "a".into(),
                true
            )
        );
        open.set(true);
        items.set(Vec::new());
        combobox_query(&id).set("  New entry  ");
        settle_controlled_combobox(&live, &mut layout);
        assert!(
            !crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.template_id.as_deref() == Some(format!("{id}:create").as_str())
            })
            .unwrap()
            .focusable
        );
        click_combobox_test_id(&live, &format!("{id}:create"));
        assert_eq!(&*created.borrow(), &[("New entry".to_owned(), true)]);
        assert!(!open.peek());
        assert_eq!(value.peek(), "a", "caller owns insertion and assignment");
        open.set(true);
        combobox_query(&id).set(" \t ");
        settle_controlled_combobox(&live, &mut layout);
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert_eq!(created.borrow().len(), 1);
        assert!(open.peek());
        items.set(vec![
            ComboboxItemData::new("id-x", "Different").disabled(true),
        ]);
        combobox_query(&id).set("ID-X");
        settle_controlled_combobox(&live, &mut layout);
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert_eq!(
            created.borrow().len(),
            1,
            "case-insensitive existing values cannot be created"
        );
        items.set(Vec::new());
        settle_controlled_combobox(&live, &mut layout);
        assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
        assert_eq!(created.borrow().last().unwrap(), &("ID-X".to_owned(), true));
        settle_controlled_combobox(&live, &mut layout);
        let root = live.borrow();
        let input = crate::core::tree::find_in_tree(&root, |el| {
            el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
        })
        .unwrap();
        assert!(Rc::ptr_eq(input.on_key_down.as_ref().unwrap(), &handler));
        drop(root);
        assert_eq!(builds.get(), 1);
        clear_test_reconcile_root();
        drop(live);
        drop(owner);
        crate::app::reactive::reset_registered_anchors();
    }
}

#[test]
fn compound_multi_confirmation_handles_filtered_and_empty_selection() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::layout::layout_tree::LayoutTree;
    crate::app::reactive::reset_registered_anchors();
    let id = "cb-compound-confirm";
    let live = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(live.clone());
    let (owner, (selection, open, confirmed, builds)) = create_root(|| {
        let selection = Signal::new(vec!["outside-results".to_owned()]);
        let open = Signal::new(true);
        let confirmed = Rc::new(RefCell::new(Vec::new()));
        let builds = Rc::new(std::cell::Cell::new(0));
        create_effect({
            let live = live.clone();
            let confirmed = confirmed.clone();
            let builds = builds.clone();
            move || {
                builds.set(builds.get() + 1);
                let confirmed = confirmed.clone();
                *live.borrow_mut() = combobox(id)
                    .open_signal(open)
                    .selection_signal(selection)
                    .on_confirm(move |values| confirmed.borrow_mut().push((values, open.peek())))
                    .child(|| {
                        let input = ComboboxInput(ComboboxInputProps::default());
                        let content = combobox_content(ComboboxContentOpts::default(), || {
                            let mut children = ["Alpha", "Beta"]
                                .into_iter()
                                .map(|name| {
                                    ComboboxItem(ComboboxItemProps {
                                        value: Some(name.into()),
                                        children: Some(Element::new().text(name)),
                                        ..Default::default()
                                    })
                                })
                                .collect::<Vec<_>>();
                            children.push(ComboboxEmpty(ComboboxEmptyProps::default()));
                            Element::new().children(children)
                        });
                        Element::new().children(vec![input, content])
                    })
                    .into();
            }
        });
        (selection, open, confirmed, builds)
    });
    let mut layout = LayoutTree::new();
    settle_controlled_combobox(&live, &mut layout);
    click_combobox_test_id(&live, &format!("{id}:opt:Alpha"));
    combobox_query(id).set("Beta");
    settle_controlled_combobox(&live, &mut layout);
    click_combobox_test_id(&live, &format!("{id}:opt:Beta"));
    assert_eq!(selection.peek(), ["outside-results", "Alpha", "Beta"]);
    combobox_query(id).set("missing");
    settle_controlled_combobox(&live, &mut layout);
    let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
        el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
    })
    .unwrap()
    .on_key_down
    .clone()
    .unwrap();
    assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
    assert_eq!(confirmed.borrow()[0], (selection.peek(), true));
    assert!(!open.peek());
    open.set(true);
    selection.set(Vec::new());
    combobox_query(id).set("missing");
    settle_controlled_combobox(&live, &mut layout);
    assert!(handler.borrow_mut()(combobox_test_key(NamedKey::Enter)));
    assert_eq!(confirmed.borrow()[1], (Vec::<String>::new(), true));
    assert!(!open.peek());
    assert_eq!(builds.get(), 1);
    clear_test_reconcile_root();
    drop(live);
    drop(owner);
    crate::app::reactive::reset_registered_anchors();
}

#[test]
fn query_signal_keeps_combobox_selection_separate_and_survives_reopen() {
    use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
    use crate::layout::layout_tree::LayoutTree;
    for count in [0, 4, 100] {
        for multi in [false, true] {
            crate::app::reactive::reset_registered_anchors();
            let id = format!("cb-query-signal-{count}-{multi}");
            let first = if count == 0 {
                "Beta".into()
            } else {
                format!("Entry {:04}", count - 1)
            };
            let next: String = if count == 0 {
                "Alpha".into()
            } else {
                "Entry 0000".into()
            };
            let selected: String = if count == 0 {
                "Alpha".into()
            } else {
                "id-0".into()
            };
            let live = Rc::new(RefCell::new(Element::new()));
            install_test_reconcile_root(live.clone());
            let (owner, (query, value, selection, open, builds, visible, focus)) =
                create_root(|| {
                    let query = Signal::new(first.clone());
                    let value = Signal::new(String::new());
                    let selection = Signal::new(Vec::<String>::new());
                    let open = Signal::new(true);
                    let builds = Rc::new(std::cell::Cell::new(0));
                    create_effect({
                        let id = id.clone();
                        let live = live.clone();
                        let builds = builds.clone();
                        move || {
                            builds.set(builds.get() + 1);
                            let mut builder = combobox(&id)
                                .open_signal(open)
                                .value_signal(value)
                                .query_signal(query)
                                .clearable(true);
                            if multi {
                                builder = builder.selection_signal(selection);
                            }
                            *live.borrow_mut() = if count == 0 {
                                builder
                                    .child(|| {
                                        let input = ComboboxInput(ComboboxInputProps::default());
                                        let content = combobox_content(
                                            ComboboxContentOpts::default(),
                                            || {
                                                Element::new().children(
                                                    ["Alpha", "Beta"]
                                                        .into_iter()
                                                        .map(|name| {
                                                            ComboboxItem(ComboboxItemProps {
                                                                value: Some(name.into()),
                                                                children: Some(
                                                                    Element::new().text(name),
                                                                ),
                                                                ..Default::default()
                                                            })
                                                        })
                                                        .collect::<Vec<_>>(),
                                                )
                                            },
                                        );
                                        Element::new().children(vec![input, content])
                                    })
                                    .into()
                            } else {
                                builder
                                    .options(
                                        (0..count)
                                            .map(|i| (format!("id-{i}"), format!("Entry {i:04}"))),
                                    )
                                    .into()
                            };
                        }
                    });
                    (
                        query,
                        value,
                        selection,
                        open,
                        builds,
                        use_signal(
                            &format!("{id}__combo_visible"),
                            Vec::<ComboboxVisibleItem>::new,
                        ),
                        use_signal("__jadix_button_focus_id", String::new),
                    )
                });
            let mut layout = LayoutTree::new();
            settle_controlled_combobox(&live, &mut layout);
            assert_eq!(combobox_query(&id).get(), first, "initial query {id}");
            assert_eq!(visible.peek().len(), 1);
            let handler = crate::core::tree::find_in_tree(&live.borrow(), |el| {
                el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
            })
            .unwrap()
            .on_key_down
            .clone()
            .unwrap();
            focus.set(String::new());
            query.set(next.clone());
            settle_controlled_combobox(&live, &mut layout);
            assert_eq!(visible.peek().len(), 1);
            assert_eq!(visible.peek()[0].value, selected);
            assert!(
                value.peek().is_empty() && selection.peek().is_empty(),
                "query cannot select {id}"
            );
            focus.set(format!("input:{id}"));
            query.set("pending remote".into());
            settle_controlled_combobox(&live, &mut layout);
            assert_eq!(combobox_query(&id).get(), next);
            combobox_query(&id).set(first.clone());
            assert_eq!(query.peek(), first, "local wins {id}");
            combobox_query(&id).set(next.clone());
            settle_controlled_combobox(&live, &mut layout);
            click_combobox_test_id(&live, &format!("{id}:opt:{selected}"));
            if multi {
                assert_eq!(selection.peek(), [selected.clone()]);
                assert!(value.peek().is_empty());
                assert!(open.peek());
            } else {
                assert_eq!(value.peek(), selected);
                assert!(selection.peek().is_empty());
                assert!(!open.peek());
            }
            assert_eq!(query.peek(), next);
            open.set(true);
            combobox_query(&id).set(first.clone());
            assert_eq!(query.peek(), first);
            open.set(false);
            settle_controlled_combobox(&live, &mut layout);
            assert_eq!(
                query.peek(),
                if multi { "" } else { next.as_str() },
                "closed display {id}"
            );
            open.set(true);
            settle_controlled_combobox(&live, &mut layout);
            focus.set(String::new());
            query.set(String::new());
            settle_controlled_combobox(&live, &mut layout);
            assert_eq!(visible.peek().len(), if count == 0 { 2 } else { count });
            let mounted = {
                let mut mounted = 0;
                fn visit(el: &Element, count: &mut usize) {
                    *count +=
                        usize::from(el.attrs.get("role").map(String::as_str) == Some("option"));
                    for child in &el.children {
                        visit(child, count);
                    }
                }
                visit(&live.borrow(), &mut mounted);
                mounted
            };
            assert!(mounted <= 30, "bounded virtual results {id}: {mounted}");
            click_combobox_test_id(&live, &format!("{id}__combo_clear"));
            assert!(
                query.peek().is_empty() && value.peek().is_empty() && selection.peek().is_empty()
            );
            let root = live.borrow();
            let input = crate::core::tree::find_in_tree(&root, |el| {
                el.focus_id.as_deref() == Some(format!("input:{id}").as_str())
            })
            .unwrap();
            assert!(
                Rc::ptr_eq(input.on_key_down.as_ref().unwrap(), &handler),
                "stable input {id}"
            );
            drop(root);
            assert_eq!(builds.get(), 1, "query cannot rebuild owner {id}");
            clear_test_reconcile_root();
            drop(live);
            drop(owner);
            crate::app::reactive::reset_registered_anchors();
        }
    }
}
