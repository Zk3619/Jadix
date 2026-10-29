use super::*;
use crate::core::reactivity::create_root;

#[test]
fn flatten_respects_expanded() {
    let roots = vec![TreeNode {
        id: "a".into(),
        label: "A".into(),
        children: vec![TreeNode {
            id: "b".into(),
            label: "B".into(),
            children: vec![],
            chrome: Default::default(),
        }],
        chrome: Default::default(),
    }];
    let mut out = Vec::new();
    flatten(&roots, &HashSet::new(), 0, &mut out);
    assert_eq!(out.len(), 1);
    let mut exp = HashSet::new();
    exp.insert("a".into());
    out.clear();
    flatten(&roots, &exp, 0, &mut out);
    assert_eq!(out.len(), 2);
}

#[test]
fn sibling_group_ids_do_not_collide_with_node_named_root() {
    assert_ne!(
        collapsible_sibling_group("tv", ""),
        collapsible_sibling_group("tv", "root"),
        "forest exclusive group must not share id with children of a node id=root"
    );
}

fn sample_workspace_tree() -> Vec<TreeNode> {
    vec![TreeNode {
        id: "root".into(),
        label: "Workspace".into(),
        children: vec![
            TreeNode {
                id: "src".into(),
                label: "src".into(),
                children: vec![
                    TreeNode {
                        id: "main".into(),
                        label: "main.rs".into(),
                        children: vec![],
                        chrome: Default::default(),
                    },
                    TreeNode {
                        id: "lib".into(),
                        label: "lib.rs".into(),
                        children: vec![],
                        chrome: Default::default(),
                    },
                ],
                chrome: Default::default(),
            },
            TreeNode {
                id: "docs".into(),
                label: "Docs".into(),
                children: vec![TreeNode {
                    id: "readme".into(),
                    label: "README.md".into(),
                    children: vec![],
                    chrome: Default::default(),
                }],
                chrome: Default::default(),
            },
        ],
        chrome: Default::default(),
    }]
}

fn subtree_has_text(el: &Element, needle: &str) -> bool {
    el.text.as_deref() == Some(needle) || el.children.iter().any(|c| subtree_has_text(c, needle))
}

fn has_slot_containing_text(el: &Element, slot: &str, needle: &str) -> bool {
    let here = el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot);
    if here && subtree_has_text(el, needle) {
        return true;
    }
    el.children
        .iter()
        .any(|c| has_slot_containing_text(c, slot, needle))
}

#[test]
fn collapsible_tree_leaves_are_static_rows() {
    let (_o, el) = create_root(|| {
        let expanded_key = "tv_col_leaf::expanded";
        use_signal(expanded_key, HashSet::<String>::new);
        tree_view("tv_col_leaf")
            .variant("collapsible")
            .height(160.0)
            .expanded_storage_key(expanded_key)
            .roots(vec![
                TreeNode {
                    id: "readme".into(),
                    label: "README.md".into(),
                    children: vec![],
                    chrome: Default::default(),
                },
                TreeNode {
                    id: "src".into(),
                    label: "src".into(),
                    children: vec![TreeNode {
                        id: "main".into(),
                        label: "main.rs".into(),
                        children: vec![],
                        chrome: Default::default(),
                    }],
                    chrome: Default::default(),
                },
            ])
            .into()
    });
    assert!(
        has_slot_containing_text(&el, SLOT_ROW, "README.md"),
        "root-level files must use tree-view-row, not CollapsibleTrigger"
    );
    assert!(
        !has_slot_containing_text(&el, "collapsible-trigger", "README.md"),
        "leaf files must not be expandable collapsibles"
    );
}

#[test]
fn collapsible_tree_sibling_expand_is_exclusive() {
    let (_o, ()) = create_root(|| {
        let expanded_key = "tv_col_excl::expanded";
        use_signal(expanded_key, || {
            let mut s = HashSet::<String>::new();
            s.insert("root".into());
            s.insert("src".into());
            s.insert("docs".into());
            s
        });
        let _: Element = tree_view("tv_col_excl")
            .variant("collapsible")
            .exclusive(true)
            .height(160.0)
            .expanded_storage_key(expanded_key)
            .roots(sample_workspace_tree())
            .into();
        let set = expanded_signal_for_key(expanded_key)
            .try_peek()
            .unwrap_or_default();
        let sibling_open = ["src", "docs"]
            .iter()
            .filter(|id| set.contains(**id))
            .count();
        assert_eq!(
            sibling_open, 1,
            "same-level collapsible must keep only one sibling expanded, set={set:?}"
        );
    });
}

#[test]
fn collapsible_tree_exclusive_false_keeps_sibling_open() {
    let (_o, ()) = create_root(|| {
        let expanded_key = "tv_col_multi::expanded";
        use_signal(expanded_key, || {
            let mut s = HashSet::<String>::new();
            s.insert("root".into());
            s.insert("src".into());
            s.insert("docs".into());
            s
        });
        let _: Element = tree_view("tv_col_multi")
            .variant("collapsible")
            .height(160.0)
            .expanded_storage_key(expanded_key)
            .roots(sample_workspace_tree())
            .into();
        let set = expanded_signal_for_key(expanded_key)
            .try_peek()
            .unwrap_or_default();
        let sibling_open = ["src", "docs"]
            .iter()
            .filter(|id| set.contains(**id))
            .count();
        assert_eq!(
            sibling_open, 2,
            "default collapsible must keep sibling folders open, set={set:?}"
        );
    });
}

#[test]
fn variant_parse_falls_back_to_flat() {
    assert_eq!(
        TreeViewVariant::parse("collapsible"),
        TreeViewVariant::Collapsible
    );
    assert_eq!(
        TreeViewVariant::parse("accordion"),
        TreeViewVariant::Collapsible
    );
    assert_eq!(TreeViewVariant::parse("unknown"), TreeViewVariant::Flat);
}

#[test]
fn distinct_tree_ids_do_not_share_expanded() {
    let roots = vec![TreeNode {
        id: "root".into(),
        label: "Workspace".into(),
        children: vec![TreeNode {
            id: "src".into(),
            label: "src".into(),
            children: vec![],
            chrome: Default::default(),
        }],
        chrome: Default::default(),
    }];
    let (_o, ()) = create_root(|| {
        let a_key = "tv-isol-a__tree_exp";
        let b_key = "tv-isol-b__tree_exp";
        let init = || {
            let mut s = HashSet::<String>::new();
            s.insert("root".into());
            s
        };
        let _: Element = tree_view("tv-isol-a")
            .variant("collapsible")
            .height(80.0)
            .expanded_signal(use_signal(a_key, init))
            .roots(roots.clone())
            .into();
        let _: Element = tree_view("tv-isol-b")
            .variant("flat")
            .height(80.0)
            .expanded_signal(use_signal(b_key, init))
            .roots(roots)
            .into();
        toggle_node_expanded_key(a_key, "root");
        assert!(!expanded_signal_for_key(a_key)
            .try_peek()
            .is_some_and(|s| s.contains("root")));
        assert!(expanded_signal_for_key(b_key)
            .try_peek()
            .is_some_and(|s| s.contains("root")));
    });
}

#[test]
fn shared_trees_collapse_root_no_disposed_peek() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let roots = vec![TreeNode {
        id: "root".into(),
        label: "Workspace".into(),
        children: vec![TreeNode {
            id: "src".into(),
            label: "src".into(),
            children: vec![TreeNode {
                id: "main".into(),
                label: "main.rs".into(),
                children: vec![],
                chrome: Default::default(),
            }],
            chrome: Default::default(),
        }],
        chrome: Default::default(),
    }];
    let (_o, ()) = create_root(|| {
        let expanded_key = "tv_shared_collapse::expanded";
        use_signal(expanded_key, || {
            let mut s = HashSet::<String>::new();
            s.insert("root".into());
            s.insert("src".into());
            s
        });
        let holder: Rc<RefCell<Option<Element>>> = Rc::new(RefCell::new(None));
        let h = Rc::clone(&holder);
        let roots_c = roots.clone();
        let roots_m = roots.clone();
        create_effect(move || {
            if h.borrow().is_some() {
                return;
            }
            let collapsible: Element = tree_view("tv-shared-collapsible")
                .variant("collapsible")
                .height(160.0)
                .expanded_storage_key(expanded_key)
                .roots(roots_c.clone())
                .into();
            let multi: Element = tree_view("tv-shared-multi")
                .variant("collapsible")
                .exclusive(false)
                .height(160.0)
                .expanded_storage_key(expanded_key)
                .roots(roots_m.clone())
                .into();
            let flat: Element = tree_view("tv-shared-flat")
                .variant("flat")
                .height(140.0)
                .expanded_storage_key(expanded_key)
                .roots(roots_c.clone())
                .into();
            *h.borrow_mut() = Some(vstack().child(collapsible).child(multi).child(flat).into());
        });
        toggle_node_expanded_key(expanded_key, "root");
        let _keep = holder.borrow().is_some();
    });
}

#[test]
fn flat_tree_collapse_root_no_disposed_peek() {
    let roots = vec![TreeNode {
        id: "root".into(),
        label: "Root".into(),
        children: vec![TreeNode {
            id: "a".into(),
            label: "A".into(),
            children: vec![],
            chrome: Default::default(),
        }],
        chrome: Default::default(),
    }];
    let (_o, ()) = create_root(|| {
        let expanded_key = "tv_flat_collapse::expanded";
        use_signal(expanded_key, || {
            let mut s = HashSet::<String>::new();
            s.insert("root".into());
            s
        });
        let _: Element = tree_view("tv_flat_collapse")
            .variant("flat")
            .height(80.0)
            .expanded_storage_key(expanded_key)
            .roots(roots)
            .into();
        toggle_node_expanded_key(expanded_key, "root");
        assert!(expanded_signal_for_key(expanded_key).try_peek().is_some());
    });
}

#[test]
fn node_open_binding_survives_parent_content_remount() {
    let (_owner, ()) = create_root(|| {
        let expanded_key = "tv_remount_binding::expanded";
        let expanded = use_signal(expanded_key, HashSet::<String>::new);
        let revision = crate::core::reactivity::Signal::new(0_u32);
        create_effect(move || {
            let _ = revision.get();
            node_open_signal("tv_remount_binding", "child", expanded_key);
        });
        // CollapsibleContent rebuilds descendants under a fresh effect owner.
        revision.set(1);
        expanded.set(HashSet::from(["child".into()]));
        let open = use_signal("tv_remount_binding::open::child", || false);
        assert_eq!(open.peek(), true, "remounted child must follow expanded");
        open.set(false);
        assert!(
            !expanded.peek().contains("child"),
            "click must update expanded after remount"
        );
    });
}

#[test]
fn node_open_signal_peek_after_expanded_toggle() {
    use crate::core::reactivity::create_effect;
    let (_o, ()) = create_root(|| {
        let expanded_key = "tv_node_open_regression::expanded";
        let expanded = use_signal(expanded_key, HashSet::<String>::new);
        expanded.set_silent({
            let mut s = HashSet::new();
            s.insert("node_a".into());
            s
        });
        let _sig = node_open_signal("tv_node_open_regression", "node_a", expanded_key);
        create_effect(move || {
            let _ = expanded.try_get();
            let _ = node_open_signal("tv_node_open_regression", "node_a", expanded_key);
            let sig = use_signal("tv_node_open_regression::open::node_a", || false);
            assert!(sig.try_peek().is_some());
        });
        expanded.set(HashSet::new());
        let sig = use_signal("tv_node_open_regression::open::node_a", || false);
        assert!(sig.try_peek().is_some());
    });
}

#[test]
fn builder_sets_slot() {
    let (_o, el): (_, Element) = create_root(|| {
        tree_view("tv-test")
            .variant("collapsible")
            .roots(vec![TreeNode {
                id: "a".into(),
                label: "A".into(),
                children: vec![],
                chrome: Default::default(),
            }])
            .into()
    });
    assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
    assert_eq!(el.template_id.as_deref(), Some("tv-test"));
}

fn find_tid(el: &Element, id: &str) -> bool {
    el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
}

fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot(c, slot))
}

fn first_text_color(el: &Element) -> Option<[u8; 4]> {
    if el.text.is_some() {
        return Some(el.text_style.color_resolved());
    }
    el.children.iter().find_map(first_text_color)
}

#[test]
fn sidebar_row_chrome_hover_and_selected() {
    let mut row = crate::primitives::r#box::hstack().w("100%").into();
    sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, false, true);
    assert_eq!(row.style.bg_color, parse_color("sidebar-accent"));
    sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, true, false);
    assert_eq!(row.style.bg_color, parse_color("sidebar-accent"));
    sync_tree_row_chrome(&mut row, TreeViewChrome::Sidebar, false, false);
    assert_eq!(row.style.bg_color, [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn sidebar_chrome_skips_inner_scroll_and_uses_sidebar_fg() {
    let (_o, el): (_, Element) = create_root(|| {
        tree_view("tv-sidebar-chrome")
            .variant("collapsible")
            .chrome("sidebar")
            .roots(vec![TreeNode {
                id: "a".into(),
                label: "A".into(),
                children: vec![],
                chrome: Default::default(),
            }])
            .into()
    });
    assert_eq!(
        el.attrs.get(ATTR_CHROME).map(String::as_str),
        Some("sidebar")
    );
    assert!(!find_tid(&el, "tv-sidebar-chrome__scroll"));
    let row = find_slot(&el, SLOT_ROW).expect("tree row");
    assert_eq!(
        first_text_color(row),
        Some(parse_text_color("sidebar-foreground"))
    );
}

#[test]
fn builder_node_style_reaches_row() {
    let (_o, el): (_, Element) = create_root(|| {
        tree_view("tv-node-chrome")
            .variant("collapsible")
            .roots(vec![tree_node("a", "A").style(|b| b.text_size(20.0))])
            .into()
    });
    fn find_size(el: &Element, want: f32) -> bool {
        el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
            || (el.text_style.size_resolved() - want).abs() < 0.01
            || el.children.iter().any(|c| find_size(c, want))
    }
    assert!(
        find_size(&el, 20.0),
        "tree_node .style must land on the painted row"
    );
}

#[test]
fn emit_select_writes_selected_signal() {
    let (_o, ()) = create_root(|| {
        let sel = use_signal("tv_emit_sel", String::new);
        let ctx = TreeViewCtx {
            id: "tv-emit".into(),
            variant: TreeViewVariant::Collapsible,
            expanded: use_signal("tv_emit_exp", HashSet::<String>::new),
            expanded_storage_key: "tv_emit_exp".into(),
            exclusive: false,
            chrome: TreeViewChrome::Default,
            selected: Some(sel),
        };
        emit_select(&ctx, "leaf");
        assert_eq!(sel.try_peek().as_deref(), Some("leaf"));
    });
}

#[test]
fn dual_surface_compiles_shape() {
    let _b: Element = tree_view("x").variant(TreeViewVariant::Flat).into();
    let el = TreeView(TreeViewProps {
        id: "y".into(),
        variant: Some("collapsible".into()),
        expanded: None,
        expanded_storage_key: None,
        row_height: None,
        height: None,
        exclusive: None,
        chrome: None,
        selected: None,
        children: None,
        base: None,
    });
    let _: Element = el;
}
