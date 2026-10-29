use super::*;
use crate::core::tree::Element;

#[test]
fn properties_scrollbar_stays_stable_after_mount_and_rebuild() {
    use crate::app::state::{update_scrollbar_elements_for_test, ScrollbarUpdateMode};
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;

    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_tab(DevToolsTab::Elements);
        DevToolsState::set_panel_width(900.0);
        DevToolsState::set_computed(false);
        DevToolsState::set_selected(vec![0, 0, 0], "scroll-target", vec![], (0.0, 0.0));
        DevToolsState::bump();
        let app = Element::new().w("100%").h("100%").child(
            Element::new()
                .template_id("scroll-target")
                .text("Selected text")
                .text_size(14.0),
        );
        let mut root = wrap_app(app);
        crate::app::state::recompute_scroll_viewport_flags(&mut root);
        let mut layout = crate::core::tree::LayoutTree::new();
        sync_overlay_layout(&mut root, &mut layout);
        let keys = ScrollViewportSignalKeys::new("devtools-card-properties");
        let mut expected = None;
        let mut expected_thumb = None;
        for frame in 0..16 {
            if frame == 8 {
                DevToolsState::set_computed(false);
                sync_overlay_layout(&mut root, &mut layout);
            }
            let overlay = frame % 3 == 1;
            let scroll_only = frame % 3 != 0;
            let offset = if frame < 3 { 0.0 } else { 120.0 };
            use_signal(&keys.offset_y, || 0.0_f32).set_silent(offset);
            update_scrollbar_elements_for_test(
                &mut root,
                &mut layout,
                if overlay {
                    ScrollbarUpdateMode::ViewportsOnly
                } else {
                    ScrollbarUpdateMode::Full
                },
            );
            let result = if frame == 0 {
                layout.build_and_layout(&mut root, 1400.0, 760.0, [-1.0; 2])
            } else {
                layout.build_and_layout_fast(
                    &mut root,
                    1400.0,
                    760.0,
                    [-1.0; 2],
                    scroll_only && frame != 8,
                    overlay,
                )
            };
            let (_, bounds, max_scroll, content) = result
                .scroll_metrics
                .iter()
                .find(|entry| entry.0 == "devtools-card-properties")
                .expect("properties viewport metrics");
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(bounds[2]);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(bounds[3]);
            use_signal(&keys.content_w, || 0.0_f32).set_silent(content[0]);
            use_signal(&keys.content_h, || 0.0_f32).set_silent(content[1]);
            let metrics = [bounds[2], bounds[3], content[0], content[1], max_scroll[1]];
            if frame >= 3 {
                let thumb = crate::core::tree::find_in_tree(&root, |element| {
                    element.focus_id.as_deref()
                        == Some("scrollbar-thumb:devtools-card-properties:y")
                })
                .expect("properties scrollbar thumb");
                if let Some(previous) = expected_thumb {
                    assert_eq!(
                        thumb.layout_bounds, previous,
                        "thumb jumped on frame {frame}"
                    );
                } else {
                    expected_thumb = Some(thumb.layout_bounds);
                }
            }
            if frame >= 2 {
                if let Some(previous) = expected {
                    assert_eq!(metrics, previous, "scroll metrics jumped on frame {frame}");
                } else {
                    assert!(max_scroll[1] > 120.0, "fixture must scroll: {metrics:?}");
                    expected = Some(metrics);
                }
            }
        }
        DevToolsState::set_open(false);
        sync_overlay_layout(&mut root, &mut layout);
        assert!(!root.contains_scroll_viewport);
        assert_eq!(root.scroll_viewport_subtree_count, 0);
    });
}

#[test]
fn complete_tree_starts_collapsed_without_pagination() {
    crate::core::reactivity::create_root(|| {
        if !DevToolsState::is_collapsed(&[]) {
            DevToolsState::toggle_collapsed(&[]);
        }
        let root = Element::new()
            .children((0..600).map(|index| Element::new().focus_id(format!("complete-{index}"))));
        let tree = snapshot_tree(&root);
        let outline = tree_outline(&tree, None);
        let mut row_count = 0;
        crate::core::tree::walk_elements(&outline, |element| {
            assert_ne!(element.template_id.as_deref(), Some("dt-tree-more"));
            if element
                .template_id
                .as_deref()
                .is_some_and(|id| id.starts_with("dt-row-"))
            {
                row_count += 1;
            }
        });
        assert_eq!(row_count, 1, "initial tree must show its collapsed root");
    });
}

#[test]
fn element_style_editor_does_not_repeat_framework_defaults() {
    crate::core::reactivity::create_root(|| {
        let root = Element::new().children([
            Element::new(),
            Element::new().w(120.0).bg([1.0, 0.0, 0.0, 1.0]),
        ]);
        let tree = snapshot_tree(&root);
        for path in [vec![0], vec![1]] {
            let node = tree.iter().find(|node| node.path == path).unwrap();
            let details = details_pane(Some(node), &root);
            let key = super::super::snapshot::path_to_key(&path);
            assert!(
                super::super::snapshot::find_element_by_template(
                    &details,
                    &format!("dt-decl-{key}-box-sizing-name")
                )
                .is_none(),
                "implicit defaults belong in Computed, not every element.style"
            );
            let width = super::super::snapshot::find_element_by_template(
                &details,
                &format!("dt-decl-{key}-width-name"),
            );
            assert_eq!(width.is_some(), path == [1]);
        }
    });
}

#[test]
fn expanded_large_tree_virtualizes_all_rows_and_reveals_unmounted_selection() {
    crate::core::reactivity::create_root(|| {
        let root = Element::new().children((0..5000).map(|index| {
            Element::new()
                .focus_id(format!("large-{index}"))
                .child(Element::new().text("leaf"))
        }));
        let tree = snapshot_tree(&root);
        for node in &tree {
            if node.child_count > 0 && DevToolsState::is_collapsed(&node.path) {
                DevToolsState::toggle_collapsed(&node.path);
            }
        }
        let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
        use_signal(&keys.viewport_w, || 0.0_f32).set_silent(1000.0);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(220.0);
        use_signal(&keys.offset_y, || 0.0_f32).set_silent(0.0);
        DevToolsState::finish_tree_reveal();
        let mut body = tree_body(&tree, "");
        assert!(
            body.children.len() < 40,
            "must not mount the whole expanded tree"
        );
        assert_eq!(
            crate::layout::dim_length_px(body.style.size.height),
            Some(330044.0)
        );
        let target = vec![4900, 0];
        let key = format!("dt-row-{}", super::super::snapshot::path_to_key(&target));
        assert!(super::super::snapshot::find_element_by_template(&body, &key).is_none());
        DevToolsState::set_selected_path(target.clone());
        DevToolsState::request_tree_reveal(&target);
        let mut layout = crate::core::tree::LayoutTree::new();
        let sync = body.reactive_sync.clone().unwrap();
        assert_eq!(
            sync.run(&mut body, &mut layout),
            LayoutStatus::StructureChanged
        );
        assert!(super::super::snapshot::find_element_by_template(&body, &key).is_some());
        assert!(body.children.len() < 40);
        let expected = (4900.0 * 3.0 + 2.0) * 22.0 + 11.0 - 110.0;
        assert!((use_signal(&keys.offset_y, || 0.0_f32).peek() - expected).abs() < 0.5);
        assert!(DevToolsState::pending_tree_reveal().is_none());
        use_signal(&keys.offset_y, || 0.0_f32).set_silent(330000.0);
        sync.run(&mut body, &mut layout);
        assert!(super::super::snapshot::find_element_by_template(&body, "dt-row-4999,0").is_some());
        crate::app::scroll_support::cancel_scroll_item_into_view(rest);
    });
}

#[test]
fn tree_row_selection_does_not_request_tree_scroll() {
    crate::core::reactivity::create_root(|| {
        let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
        crate::app::scroll_support::cancel_scroll_item_into_view(rest);
        let tree = snapshot_tree(&Element::new().focus_id("selection-target"));
        let mut row = tree_row(&tree, &tree[0], None, None);
        row.layout_bounds = [0.0, 400.0, 800.0, 22.0];
        let viewport = Element::new()
            .focus_id(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                rest,
            ))
            .child(row);
        for _ in 0..3 {
            (viewport.children[0].on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(DevToolsState::selected_path(), tree[0].path);
            let pending = crate::app::scroll_support::resolve_pending_descendant_bounds(&viewport);
            crate::app::scroll_support::cancel_scroll_item_into_view(rest);
            assert!(
                !pending.contains_key(rest),
                "selecting a visible tree row must not scroll it"
            );
        }
    });
}

#[test]
fn tree_scrollbars_stay_visible_only_while_scrollable() {
    crate::core::reactivity::create_root(|| {
        let card = inspector_card("Element tree", Element::new());
        let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
        let key = crate::core::scrollbar::scrollbar_visibility_signal_key(rest, "y");
        assert_eq!(use_signal(&key, String::new).peek(), "always");
        assert!(super::super::snapshot::find_element_by_template(
            &card,
            &format!("__jadix_h_scrollbar_{rest}")
        )
        .is_none());
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
        use_signal(&keys.viewport_w, || 0.0_f32).set_silent(480.0);
        let viewport_h = use_signal(&keys.viewport_h, || 0.0_f32);
        viewport_h.set_silent(100.0);
        let tree = snapshot_tree(&Element::new().children((0..20).map(|_| Element::new())));
        let rows: Vec<_> = (0..tree.len()).map(|index| (index, false)).collect();
        let mut body = Element::new();
        let mut geometry = (0.0, 0.0, Vec::new());
        sync_tree_window(&mut body, &tree, &rows, &mut geometry);
        assert_eq!(use_signal(&key, String::new).peek(), "always");
        assert!(body.style.padding.right != taffy::style::LengthPercentage::length(0.0));
        viewport_h.set_silent(1000.0);
        sync_tree_window(&mut body, &tree, &rows, &mut geometry);
        assert_eq!(use_signal(&key, String::new).peek(), "never");
        assert_eq!(
            body.style.padding.right,
            taffy::style::LengthPercentage::length(0.0)
        );
        use_signal(&key, String::new).set_silent("always".to_string());
        let mut empty = tree_body(&tree, "missing-tree-node");
        let mut layout = crate::core::tree::LayoutTree::new();
        empty
            .reactive_sync
            .clone()
            .unwrap()
            .run(&mut empty, &mut layout);
        assert_eq!(use_signal(&key, String::new).peek(), "never");
    });
}

#[test]
fn tree_double_click_toggles_branches_but_not_leaves_or_disclosures() {
    crate::core::reactivity::create_root(|| {
        let root = Element::new().child(Element::new());
        let tree = snapshot_tree(&root);
        let collapsed = DevToolsState::is_collapsed(&tree[0].path);
        let row = tree_row(&tree, &tree[0], None, None);
        let handler = row.on_double_click.as_ref().unwrap();
        let mut event = crate::app::pointer_events::MouseEvent::new(
            10.0,
            10.0,
            winit::event::MouseButton::Left,
            2,
        );
        handler.borrow_mut()(&event);
        assert_eq!(DevToolsState::is_collapsed(&tree[0].path), !collapsed);
        event.target_focus_id = Some("dt-fold-".into());
        handler.borrow_mut()(&event);
        assert_eq!(DevToolsState::is_collapsed(&tree[0].path), !collapsed);
        event.target_focus_id = None;
        handler.borrow_mut()(&event);
        assert_eq!(DevToolsState::is_collapsed(&tree[0].path), collapsed);
        let leaf = tree_row(&tree, &tree[1], None, None);
        leaf.on_double_click.as_ref().unwrap().borrow_mut()(&event);
        assert!(DevToolsState::is_collapsed(&tree[1].path));
        let path = [
            crate::app::pointer_events::HitPathEntry::from_element(&row, [0.0, 0.0, 200.0, 22.0]),
            crate::app::pointer_events::HitPathEntry::from_element(
                &row.children[1],
                [16.0, 0.0, 184.0, 22.0],
            ),
        ];
        crate::app::pointer_events::dispatch_pointer(
            &path,
            &mut event,
            crate::app::pointer_events::PointerKind::DoubleClick,
        );
        assert_eq!(
            DevToolsState::is_collapsed(&tree[0].path),
            !collapsed,
            "caption bubbling must toggle the branch only once"
        );
    });
}

#[test]
fn wrapped_tree_rows_keep_disclosure_indent_and_only_reserve_scrollable_gutter() {
    crate::core::reactivity::create_root(|| {
        let root = Element::new().children((0..20).map(|index| {
            Element::new().focus_id(format!("wrapped-tree-node-{index}-with-a-long-identifier"))
        }));
        let tree = snapshot_tree(&root);
        if DevToolsState::is_collapsed(&[]) {
            DevToolsState::toggle_collapsed(&[]);
        }
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(
            super::super::overlay::ELEMENTS_TREE_SCROLL_ID,
        );
        use_signal(&keys.viewport_w, || 0.0_f32).set_silent(220.0);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(100.0);
        let mut body = tree_body(&tree, "");
        assert_ne!(
            body.style.padding.right,
            taffy::style::LengthPercentage::length(0.0)
        );
        let mut layout = crate::core::tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut body, 220.0, 2000.0, [-1.0; 2]);
        let row = &body.children[1];
        assert!(row.layout_bounds[3] > 22.0, "long node must wrap");
        let caption = &row.children[1];
        assert!((caption.layout_bounds[0] - row.layout_bounds[0] - 26.0).abs() < 0.1);
        assert!(caption.layout_bounds[0] + caption.layout_bounds[2] <= 220.1);
        assert!(result
            .text_nodes
            .iter()
            .any(|text| text.text.contains("wrapped-tree-node")));
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(10000.0);
        body.reactive_sync
            .clone()
            .unwrap()
            .run(&mut body, &mut layout);
        assert_eq!(
            body.style.padding.right,
            taffy::style::LengthPercentage::length(0.0)
        );
    });
}

#[test]
fn narrow_header_navigation_retains_horizontal_scroll_extent() {
    crate::core::reactivity::create_root(|| {
        let mut root = panel_header(false, DevToolsTab::Elements);
        let mut tree = crate::core::tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 400.0, 34.0, [-1.0; 2]);
        let (_, bounds, max_scroll, content) = result
            .scroll_metrics
            .iter()
            .find(|(id, _, _, _)| id == "dt-nav")
            .unwrap();
        assert!(bounds[2] > 0.0);
        assert!(
            max_scroll[0] > 100.0,
            "bounds={bounds:?} max={max_scroll:?} content={content:?}"
        );
        assert!(max_scroll[1] < 0.5);
        let first = super::super::snapshot::find_element_by_template(&root, "dt-tab-el").unwrap();
        let last =
            super::super::snapshot::find_element_by_template(&root, "dt-tab-changes").unwrap();
        assert!(last.layout_bounds[0] > first.layout_bounds[0] + bounds[2]);
    });
}

#[test]
fn tree_selection_repaints_the_entire_row_including_indentation() {
    crate::core::reactivity::create_root(|| {
        let app = Element::new().child(Element::new());
        let tree = snapshot_tree(&app);
        DevToolsState::set_selected_path(tree[0].path.clone());
        let row = tree_row(&tree, &tree[1], None, None);
        let mut root = Element::new().w(220.0).h(100.0).child(row);
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 220.0, 100.0, [-1.0; 2]);
        crate::app::damage::take_frame_paint_nodes();
        DevToolsState::set_selected_path(tree[1].path.clone());
        let mut impact = LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut layout,
            &mut impact,
            220.0,
            100.0,
            [0.0; 2],
        );
        let damage = crate::app::damage::take_frame_paint_nodes();
        assert!(
            damage.iter().any(|dirty| {
                dirty.kind == crate::app::damage::PaintDirtyKind::Region
                    && dirty.bounds[0] <= 0.0
                    && dirty.bounds[0] + dirty.bounds[2] >= 220.0
            }),
            "selection damage must restore the full row: {damage:?}"
        );
    });
}

#[test]
fn replacing_tree_rows_invalidates_the_visible_viewport_not_only_old_content() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new().w(320.0).h(300.0).child(
            Element::new()
                .w(320.0)
                .h(240.0)
                .overflow("hidden")
                .child(Element::new().template_id("dt-tree-rows").w(320.0).h(22.0)),
        );
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 320.0, 300.0, [-1.0; 2]);
        crate::app::damage::take_frame_paint_nodes();
        replace_panel_region(
            &mut root,
            "dt-tree-rows",
            Element::new().template_id("dt-tree-rows").w(320.0).h(220.0),
        );
        let damage = crate::app::damage::take_frame_paint_nodes();
        assert!(
            damage.iter().any(|dirty| {
                dirty.bounds[0] <= 0.0
                    && dirty.bounds[1] <= 0.0
                    && dirty.bounds[0] + dirty.bounds[2] >= 320.0
                    && dirty.bounds[1] + dirty.bounds[3] >= 240.0
            }),
            "expanded rows below the previous body must repaint: {damage:?}"
        );
    });
}

#[test]
fn chrome_and_selection_updates_preserve_unaffected_controls() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(640.0);
        DevToolsState::set_tab(DevToolsTab::Elements);
        DevToolsState::set_pick(false);
        DevToolsState::bump();
        let app = Element::new().w("100%").h("100%").child(
            Element::new()
                .template_id("scoped-selection-target")
                .text("target"),
        );
        let mut root = wrap_app(app);
        let mut layout = crate::core::tree::LayoutTree::new();
        sync_overlay_layout(&mut root, &mut layout);
        layout.build_and_layout(&mut root, 1400.0, 900.0, [-1.0; 2]);
        let query_id = crate::utils::text_field::input_text_template_id("dt-tree-query");
        let query_node = super::super::snapshot::find_element_by_template(&root, &query_id)
            .unwrap()
            .layout_node;
        let picker = super::super::snapshot::find_element_by_template(&root, "dt-pick")
            .unwrap()
            .on_click
            .clone()
            .unwrap();
        DevToolsState::set_pick(true);
        assert_eq!(
            sync_overlay_layout(&mut root, &mut layout),
            LayoutStatus::StyleChanged
        );
        assert_eq!(
            super::super::snapshot::find_element_by_template(&root, &query_id)
                .unwrap()
                .layout_node,
            query_node
        );
        assert!(std::rc::Rc::ptr_eq(
            &picker,
            super::super::snapshot::find_element_by_template(&root, "dt-pick")
                .unwrap()
                .on_click
                .as_ref()
                .unwrap()
        ));
        DevToolsState::set_selected(vec![0, 0, 0], "scoped-selection-target", vec![], (0.0, 0.0));
        sync_overlay_layout(&mut root, &mut layout);
        assert_eq!(
            super::super::snapshot::find_element_by_template(&root, &query_id)
                .unwrap()
                .layout_node,
            query_node
        );
        layout.build_and_layout_fast(&mut root, 1400.0, 900.0, [-1.0; 2], false, false);
        let editor_id =
            crate::utils::text_field::input_text_template_id(&cssom_input_id(&[0, 0, 0]));
        let editor = super::super::snapshot::find_element_by_template(&root, &editor_id)
            .unwrap()
            .layout_node;
        DevToolsState::set_pick(false);
        sync_overlay_layout(&mut root, &mut layout);
        assert_eq!(
            super::super::snapshot::find_element_by_template(&root, &editor_id)
                .unwrap()
                .layout_node,
            editor
        );
        DevToolsState::set_open(false);
    });
}

#[test]
fn status_changes_only_rebuild_the_visible_status_panel() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::set_tab(DevToolsTab::Elements);
        let before = DevToolsState::revisions();
        DevToolsState::record_change("CSS", "updated");
        let after = DevToolsState::revisions();
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1], before[1]);
        assert_eq!(after[2], before[2]);

        DevToolsState::set_tab(DevToolsTab::Changes);
        let before = DevToolsState::revisions();
        DevToolsState::record_change("CSS", "updated");
        let after = DevToolsState::revisions();
        assert_eq!(after[0], before[0].wrapping_add(1));
        assert_eq!(after[1], before[1]);
        assert_eq!(after[2], before[2]);
        DevToolsState::set_tab(DevToolsTab::Elements);
    });
}

#[test]
fn devtools_sources_do_not_depend_on_component_library() {
    for source in [
        include_str!("../mod.rs"),
        include_str!("../controls.rs"),
        include_str!("../device.rs"),
        include_str!("../locale.rs"),
        include_str!("../overlay.rs"),
        include_str!("../panel.rs"),
        include_str!("../presentation.rs"),
        include_str!("../reactivity_panel.rs"),
        include_str!("../snapshot.rs"),
        include_str!("../source.rs"),
        include_str!("../state.rs"),
        include_str!("../style_edit.rs"),
    ] {
        assert!(
            !source.contains(concat!("crate::", "components")),
            "DevTools must use the shared foundation, not product components"
        );
    }
}

#[test]
fn closing_resizing_and_reopening_restore_app_geometry_on_fast_layout() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_panel_width(400.0);
        DevToolsState::set_open(true);
        DevToolsState::bump();
        let app = Element::new().w("100%").h("100%").children(
            (0..30)
                .map(|index| Element::new().focus_id(format!("app-row-{index}")))
                .collect::<Vec<_>>(),
        );
        let mut root = wrap_app(app);
        let mut tree = crate::core::tree::LayoutTree::new();
        assert_ne!(
            sync_overlay_layout(&mut root, &mut tree),
            LayoutStatus::Clean
        );
        tree.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);
        assert!((root.children[0].layout_bounds[2] - 800.0).abs() < 1.0);
        let hit = crate::app::pointer_events::hit_path(&root, 803.0, 300.0);
        assert!(
            hit.iter()
                .any(|entry| entry.focus_id.as_deref() == Some("__jadix_devtools_resize")),
            "resize handle must be reachable through the real hit-test path"
        );
        for (open, width, expected) in [
            (false, 400.0, 1200.0),
            (true, 520.0, 680.0),
            (false, 520.0, 1200.0),
        ] {
            DevToolsState::set_open(open);
            DevToolsState::set_panel_width(width);
            assert_ne!(
                sync_overlay_layout(&mut root, &mut tree),
                LayoutStatus::Clean
            );
            tree.build_and_layout_fast(&mut root, 1200.0, 700.0, [-1.0, -1.0], false, false);
            assert!(
                (root.children[0].layout_bounds[2] - expected).abs() < 1.0,
                "app did not reclaim width after open={open}"
            );
            if open {
                let panel = super::super::snapshot::find_element_by_template(&root, PANEL_ID)
                    .expect("open DevTools panel missing");
                assert!(
                    (panel.layout_bounds[2] - width).abs() < 1.0,
                    "DevTools panel width did not follow state: got={} want={width}",
                    panel.layout_bounds[2]
                );
            }
        }
    });
}

#[test]
fn closing_devtools_dismisses_hovered_retained_tooltips() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::bump();
        let mut root = wrap_app(Element::new());
        sync_overlay(&mut root);
        let close = super::super::snapshot::find_element_by_template(&root, "dt-close")
            .expect("close button");
        let hover = close.on_hover.clone().unwrap();
        (hover.borrow_mut())(true);
        let open = use_signal("dt-close:tooltip:open", || false);
        assert!(open.peek());
        DevToolsState::set_open(false);
        sync_overlay(&mut root);
        assert!(
            !open.peek(),
            "unmount must close the retained tooltip without mouseleave"
        );
        assert!(super::super::snapshot::find_element_by_template(&root, PANEL_ID).is_none());
    });
}

#[test]
fn status_bar_spans_the_bottom_of_the_docked_panel() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(700.0);
        DevToolsState::set_tab(DevToolsTab::Elements);
        let mut root = wrap_app(Element::new().w("100%").h("100%"));
        let mut tree = crate::core::tree::LayoutTree::new();
        sync_overlay_layout(&mut root, &mut tree);
        tree.build_and_layout(&mut root, 1400.0, 800.0, [-1.0; 2]);
        let footer =
            super::super::snapshot::find_element_by_template(&root, "__jadix_devtools_status")
                .unwrap();
        let [left, top, width, height] = footer.layout_bounds;
        assert!(
            (top + height - 800.0).abs() < 1.0,
            "footer={:?}",
            footer.layout_bounds
        );
        assert!((height - 24.0).abs() < 1.0);
        assert!(left >= 700.0 && width >= 698.0);
        DevToolsState::set_open(false);
    });
}

#[test]
fn tree_rows_include_closing_tags_and_stable_opaque_backgrounds() {
    crate::core::reactivity::create_root(|| {
        let mut leaf = Element::new().text("leaf");
        leaf.set_attr("data-slot", "text");
        let mut root = Element::new().focus_id("tree-parent").child(leaf);
        root.set_attr("data-slot", "parent");
        let tree = snapshot_tree(&root);
        if DevToolsState::is_collapsed(&[]) {
            DevToolsState::toggle_collapsed(&[]);
        }
        let row = tree_row(&tree, &tree[0], None, None);
        assert_eq!(row.style.bg_color[3], 1.0);
        let closing = closing_tree_row(&tree[0]);
        let mut captions = Vec::new();
        crate::core::tree::walk_elements(&closing, |node| {
            if let Some(value) = &node.text {
                captions.push(value.clone());
            }
        });
        assert!(captions.iter().any(|caption| caption == "</parent>"));
        DevToolsState::toggle_collapsed(&[]);
        let collapsed = tree_row(&tree, &tree[0], None, None);
        let mut closed = false;
        crate::core::tree::walk_elements(&collapsed, |node| {
            closed |= node
                .text
                .as_ref()
                .is_some_and(|value| value.contains("</parent>"));
        });
        assert!(closed);
        DevToolsState::toggle_collapsed(&[]);
    });
}

#[test]
fn breadcrumbs_reveal_selected_node_in_horizontal_only_viewport() {
    crate::core::reactivity::create_root(|| {
        let mut root = Element::new().focus_id("selected-leaf");
        for index in 0..12 {
            let mut parent = Element::new().child(root);
            parent.set_attr("data-slot", format!("ancestor-{index}"));
            root = parent;
        }
        let tree = snapshot_tree(&root);
        let selected = tree
            .iter()
            .find(|node| node.focus_id.as_deref() == Some("selected-leaf"));
        let mut crumbs = breadcrumbs(&tree, selected);
        let mut layout = crate::core::tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut crumbs, 240.0, 32.0, [-1.0; 2]);
        let (rest, viewport, max_scroll, content) = result
            .scroll_metrics
            .iter()
            .find(|(rest, _, _, _)| rest == "dt-breadcrumbs")
            .expect("breadcrumb scroll metrics missing");
        assert!(max_scroll[0] > 0.0);
        assert_eq!(max_scroll[1], 0.0);
        let targets = crate::app::scroll_support::resolve_pending_descendant_bounds(&crumbs);
        let target = targets[rest];
        assert!(crate::app::scroll_support::apply_pending_scroll_align(
            rest,
            *viewport,
            content[0],
            content[1],
            Some(target),
        ));
        let offset = crate::core::reactivity::use_signal(
            &crate::core::scroll_viewport::scroll_offset_x_signal_key(rest),
            || 0.0_f32,
        )
        .peek();
        assert!(offset > 0.0, "selected breadcrumb was not revealed");
        assert!(target[0] - offset >= viewport[0] - 0.5);
        assert!(target[0] + target[2] - offset <= viewport[0] + viewport[2] + 0.5);
    });
}

#[test]
fn breadcrumbs_keep_arrows_aligned_and_mark_the_selected_node() {
    crate::core::reactivity::create_root(|| {
        let root = Element::new().focus_id("root").child(
            Element::new()
                .focus_id("parent")
                .child(Element::new().focus_id("child")),
        );
        let tree = snapshot_tree(&root);
        let selected = tree
            .iter()
            .find(|node| node.focus_id.as_deref() == Some("child"));
        let mut breadcrumbs = breadcrumbs(&tree, selected);
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut breadcrumbs, 360.0, 32.0, [-1.0; 2]);

        let selected_key = super::super::snapshot::path_to_key(&selected.unwrap().path);
        let selected_crumb = super::super::snapshot::find_element_by_template(
            &breadcrumbs,
            &format!("dt-crumb-{selected_key}"),
        )
        .expect("selected breadcrumb missing");
        assert_eq!(
            selected_crumb.style.bg_color[3],
            parse_color("secondary")[3]
        );
        for focus_id in ["root", "parent", "child"] {
            let node = tree
                .iter()
                .find(|node| node.focus_id.as_deref() == Some(focus_id))
                .unwrap();
            let key = super::super::snapshot::path_to_key(&node.path);
            assert!(
                super::super::snapshot::find_element_by_template(
                    &breadcrumbs,
                    &format!("dt-crumb-{key}"),
                )
                .is_some(),
                "breadcrumb node {focus_id} missing"
            );
        }
    });
}

#[test]
fn closing_tag_uses_available_row_width_and_supports_wrapping() {
    crate::core::reactivity::create_root(|| {
        let mut parent = Element::new().child(Element::new());
        parent.set_attr("data-slot", "sidebar-menu-button");
        let tree = snapshot_tree(&parent);
        let mut row = closing_tree_row(&tree[0]);
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut row, 360.0, 22.0, [-1.0; 2]);
        let caption = &row.children[0];
        assert_eq!(
            caption.text_style.white_space,
            crate::style::WhiteSpace::Normal
        );
        assert!(
            caption.layout_bounds[2] >= 300.0,
            "closing tag prematurely constrained: {:?}",
            caption.layout_bounds
        );
        assert!(caption.layout_bounds[3] <= 22.0);
    });
}

#[test]
fn tree_find_preserves_the_search_draft_when_the_find_button_takes_focus() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::set_tree_filter(String::new());
        let tree = snapshot_tree(&Element::new().child(Element::new().focus_id("checkbox-target")));
        let field = use_signal(&input_value_signal_key("dt-tree-query"), String::new);
        field.set_silent("checkbox".into());
        use_signal("__jadix_button_focus_id", String::new).set_silent("dt-tree-filter".into());
        let outline = tree_outline(&tree, None);
        assert_eq!(field.peek(), "checkbox");
        let find =
            super::super::snapshot::find_element_by_template(&outline, "dt-tree-filter").unwrap();
        (find.on_click.as_ref().unwrap().borrow_mut())();
        assert_eq!(DevToolsState::selected_path(), vec![0]);
        field.set_silent(String::new());
        DevToolsState::set_tree_filter(String::new());
        DevToolsState::finish_tree_reveal();
    });
}

#[test]
fn tree_find_uses_updated_nodes_after_outline_refresh() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_tab(DevToolsTab::Elements);
        DevToolsState::bump();
        DevToolsState::set_selected_path(Vec::new());
        DevToolsState::set_tree_filter(String::new());
        let mut root = wrap_app(
            Element::new()
                .template_id("search-page")
                .child(Element::new().focus_id("old-search-target")),
        );
        sync_overlay(&mut root);
        let page = find_element_by_template_mut(&mut root, "search-page").unwrap();
        page.children = vec![Element::new().focus_id("fresh-search-target")];
        DevToolsState::refresh_outline();
        sync_overlay(&mut root);
        let expected = snapshot_tree(&root)
            .into_iter()
            .find(|node| node.focus_id.as_deref() == Some("fresh-search-target"))
            .unwrap()
            .path;
        use_signal(&input_value_signal_key("dt-tree-query"), String::new)
            .set_silent("fresh-search-target".to_string());
        let find =
            super::super::snapshot::find_element_by_template(&root, "dt-tree-filter").unwrap();
        (find.on_click.as_ref().unwrap().borrow_mut())();
        assert_eq!(DevToolsState::selected_path(), expected);
        DevToolsState::set_tree_filter(String::new());
        DevToolsState::finish_tree_reveal();
        DevToolsState::set_open(false);
    });
}

#[test]
fn tree_find_selects_collapsed_descendants_in_document_order_and_wraps() {
    let root = Element::new().children(vec![
        Element::new()
            .focus_id("parent")
            .child(Element::new().focus_id("needle")),
        Element::new().focus_id("other-needle"),
    ]);
    let tree = snapshot_tree(&root);
    if DevToolsState::is_collapsed(&[]) {
        DevToolsState::toggle_collapsed(&[]);
    }
    if DevToolsState::is_collapsed(&[0]) {
        DevToolsState::toggle_collapsed(&[0]);
    }
    DevToolsState::toggle_collapsed(&[0]);
    assert_eq!(visible_tree(&tree, "").len(), 3);
    DevToolsState::set_tree_filter(String::new());
    find_next_tree_node(&tree, "NEEDLE".into());
    assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
    assert!(!DevToolsState::is_collapsed(&[0]));
    assert_eq!(DevToolsState::pending_tree_reveal().as_deref(), Some("0,0"));
    find_next_tree_node(&tree, "needle".into());
    assert_eq!(DevToolsState::selected_path(), vec![1]);
    find_next_tree_node(&tree, "needle".into());
    assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
    find_next_tree_node(&tree, "missing".into());
    assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
    DevToolsState::finish_tree_reveal();
    DevToolsState::set_tree_filter(String::new());
}

#[test]
fn selecting_offscreen_page_node_centers_only_its_scroll_viewport() {
    crate::core::reactivity::create_root(|| {
        use crate::core::scroll_viewport::{activate_scroll_viewport, ScrollViewportSignalKeys};
        let rest = "devtools-page-reveal-test";
        let mut root = activate_scroll_viewport(
            Element::new().w(300.0).h(200.0).child(
                Element::new()
                    .w(300.0)
                    .h(1000.0)
                    .min_h(1000.0)
                    .flex_shrink(0.0)
                    .child(
                        Element::new()
                            .template_id("offscreen-target")
                            .absolute()
                            .top(600.0)
                            .w(80.0)
                            .h(40.0),
                    ),
            ),
            rest,
            "auto",
            "auto",
            "jump",
            "scrollbar",
        );
        let mut layout = crate::core::tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
        let keys = ScrollViewportSignalKeys::new(rest);
        use_signal(&keys.content_h, || 0.0_f32).set_silent(result.scroll_metrics[0].3[1]);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(200.0);
        reveal_page_node_in_root(&root, &[0, 0]);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 520.0);
        layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
        reveal_page_node_in_root(&root, &[0, 0]);
        assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 520.0);
    });
}

#[test]
fn scaled_page_reveal_keeps_the_selected_target_visible_after_scroll() {
    crate::core::reactivity::create_root(|| {
        use crate::core::scroll_viewport::{activate_scroll_viewport, ScrollViewportSignalKeys};
        let rest = "devtools-scaled-page-reveal";
        let viewport = activate_scroll_viewport(
            Element::new().w(300.0).h(200.0).child(
                Element::new()
                    .w(300.0)
                    .h(1000.0)
                    .min_h(1000.0)
                    .flex_shrink(0.0)
                    .child(
                        Element::new()
                            .template_id("scaled-offscreen-target")
                            .absolute()
                            .top(600.0)
                            .w(80.0)
                            .h(40.0),
                    ),
            ),
            rest,
            "auto",
            "auto",
            "jump",
            "scrollbar",
        );
        let mut root = Element::new().w(800.0).h(700.0).child(
            Element::new()
                .absolute()
                .left(80.0)
                .top(90.0)
                .w(390.0)
                .h(844.0)
                .scale(0.57)
                .transform_origin("left top")
                .child(viewport),
        );
        let mut layout = crate::core::tree::LayoutTree::new();
        let result = layout.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
        let keys = ScrollViewportSignalKeys::new(rest);
        use_signal(&keys.content_h, || 0.0_f32).set_silent(result.scroll_metrics[0].3[1]);
        use_signal(&keys.viewport_h, || 0.0_f32).set_silent(200.0);
        reveal_page_node_in_root(&root, &[0, 0, 0, 0]);
        layout.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
        let viewport = &root.children[0].children[0];
        let target = &viewport.children[0].children[0];
        let bounds = crate::core::tree::visual_paint_aabb(viewport, viewport.paint_scroll_acc);
        let target_box = crate::core::tree::visual_paint_aabb(target, target.paint_scroll_acc);
        assert!(
            (target_box[1] + target_box[3] * 0.5 - bounds[1] - bounds[3] * 0.5).abs() < 1.0,
            "search scrolled away from the target: viewport={bounds:?} target={target_box:?}"
        );
    });
}

#[test]
fn sources_resolves_nearest_preserved_reactive_anchor() {
    let mut parent = Element::new().template_id("gallery:section");
    parent.set_attr(
        crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
        "examples/gallery/src/app.rs:21:5",
    );
    let root = Element::new().child(parent.child(Element::new().focus_id("button")));
    let source = source_for_path(&root, &[0, 0]).unwrap();
    assert_eq!(source.path, "examples/gallery/src/app.rs");
    assert_eq!(source.line, 21);
}

#[test]
fn developer_panels_build_without_source_selection() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        let root = Element::new().child(Element::new().focus_id("app"));
        let tree = snapshot_tree(&root);
        for tab in [
            DevToolsTab::Elements,
            DevToolsTab::Layout,
            DevToolsTab::Reactivity,
            DevToolsTab::Sources,
            DevToolsTab::Changes,
        ] {
            let panel = dock_panel(tab, &tree, tree.get(1), false, 720.0, &root, &[0]);
            assert_eq!(panel.template_id.as_deref(), Some(PANEL_ID));
            assert_eq!(
                panel.children.len(),
                4,
                "header, body, footer and resize handle"
            );
            assert!(super::super::snapshot::find_element_by_template(&panel, "dt-tabs").is_none());
            let header = super::super::snapshot::find_element_by_template(&panel, "dt-header")
                .expect("header is present");
            assert!(super::super::snapshot::find_element_by_template(header, "dt-nav").is_some());
            for (id, title, candidate) in devtools_tabs() {
                let button = super::super::snapshot::find_element_by_template(&header, id)
                    .unwrap_or_else(|| panic!("missing navigation button {id}"));
                assert_eq!(
                    button.attrs.get("aria-label").map(String::as_str),
                    Some(title)
                );
                assert_eq!(button.attrs.get("role").map(String::as_str), Some("tab"));
                assert_eq!(
                    button.attrs.get("aria-selected").map(String::as_str),
                    Some(if tab == candidate { "true" } else { "false" })
                );
                assert!(button
                    .class_set
                    .as_ref()
                    .is_some_and(|set| set.has_pointer_hover_variants()));
            }
        }
    });
}

#[test]
fn inspector_tabs_hover_for_selected_and_unselected_items() {
    let selected = inspector_tab("selected", "Styles", true, || {});
    let unselected = inspector_tab("unselected", "Computed", false, || {});
    for tab in [&selected, &unselected] {
        let classes = tab.class_set.as_ref().expect("hover variants are retained");
        assert!(classes.has_pointer_hover_variants());
        assert_eq!(tab.attrs.get("role").map(String::as_str), Some("tab"));
    }
    assert_ne!(selected.style.bg_color, unselected.style.bg_color);
}

#[test]
fn inspector_seed_unbinds_stale_controlled_pairing() {
    use crate::core::reactivity::{create_root, use_signal};
    use crate::utils::text_field::{
        bind_controlled_value, input_value_signal_key, pull_controlled_into_store,
        unbind_controlled_value,
    };

    create_root(|| {
        let id = "dt-cssom-test-unbind";
        let external = use_signal("__jadix_dt_cssom_test_unbind", || "from-external".into());
        bind_controlled_value(id, external);
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "from-external"
        );

        // Simulate Inspector switching to uncontrolled: drop pairing, seed store.
        unbind_controlled_value(id);
        use_signal(&input_value_signal_key(id), String::new).set_silent("from-store".into());

        // Paint sync must NOT pull the old external back over the store.
        let pulled = pull_controlled_into_store(id, "from-store", false);
        assert!(pulled.is_none());
        assert_eq!(
            use_signal(&input_value_signal_key(id), String::new).peek(),
            "from-store"
        );
    });
}

#[test]
fn wrap_app_inserts_docked_panel_slot() {
    DevToolsState::enable_for_window();
    let root = wrap_app(Element::new().focus_id("app"));
    assert!(root
        .children
        .iter()
        .any(|c| c.template_id.as_deref() == Some(PANEL_SLOT_ID)));
    let host = root
        .children
        .iter()
        .find(|c| c.template_id.as_deref() == Some("__jadix_app_host"))
        .expect("app host");
    assert!(host
        .children
        .iter()
        .any(|c| { c.template_id.as_deref() == Some("__jadix_devtools_highlight") }));
}

#[test]
fn docked_content_breakpoints_use_the_remaining_window_width() {
    crate::core::reactivity::create_root(|| {
        DevToolsState::enable_for_window();
        DevToolsState::set_open(true);
        DevToolsState::set_panel_width(640.0);
        super::super::device::disable();
        let mut app = Element::new().focus_id("responsive-app");
        crate::style::apply_class_list(&mut app, "w-24 md:w-48 p-2 md:p-8 hover:bg-red-500");
        let mut root = wrap_app(app);
        let mut layout = crate::core::tree::LayoutTree::new();
        for (window_width, open, panel_width, expected_width) in [
            (1200.0, true, 640.0, 96.0),
            (1500.0, true, 640.0, 192.0),
            (1500.0, true, 900.0, 96.0),
            (1200.0, false, 900.0, 192.0),
            (1200.0, true, 320.0, 192.0),
            (600.0, true, 900.0, 96.0),
        ] {
            crate::utils::modal_motion::set_modal_viewport_size(window_width, 900.0);
            DevToolsState::set_open(open);
            DevToolsState::set_panel_width(panel_width);
            sync_overlay_layout(&mut root, &mut layout);
            let expected_viewport = if open {
                (window_width - panel_width).max(1.0)
            } else {
                window_width
            };
            assert_eq!(
                crate::utils::modal_motion::modal_viewport_size(),
                (expected_viewport, 900.0)
            );
            let mut inspector = Element::new();
            crate::style::apply_class_list(&mut inspector, "w-24 md:w-48");
            root.children[1].children.push(inspector);
            let mut impact = LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut root,
                [-1.0; 2],
                "",
                false,
                window_width,
                900.0,
                &mut layout,
                &mut impact,
            );
            assert_eq!(
                root.children[0].children[0].style.size.width,
                taffy::style::Dimension::length(expected_width),
                "window={window_width}, dock={panel_width}, open={open}"
            );
            assert_eq!(
                root.children[1].children.last().unwrap().style.size.width,
                taffy::style::Dimension::length(if window_width >= 768.0 { 192.0 } else { 96.0 }),
                "Inspector must keep its own breakpoint scope"
            );
        }
        DevToolsState::set_open(false);
    });
}

#[test]
fn cssom_alignment_apply_reports_errors_and_queues_supported_values() {
    crate::core::reactivity::create_root(|| {
        let _ = super::super::take_pending_inspector_edits();
        let instance_id = 123456;
        DevToolsState::set_tab(DevToolsTab::Elements);
        let field = "dt-decl-0-align-items";
        let _ = element_style_editor(&[0], instance_id, "align-items: center;");
        use_signal(&input_value_signal_key(field), String::new).set_silent("left".into());
        let textarea = cssom_input_id(&[0]);
        let _ = cssom_editor(vec![0], instance_id, "align-items: center;".into());
        use_signal(&input_value_signal_key(&textarea), String::new)
            .set_silent("align-items: left;".into());
        let before = DevToolsState::revisions();
        apply_cssom(vec![0], instance_id, "align-items: left;".into());
        assert_ne!(DevToolsState::revisions()[2], before[2]);
        let _ = element_style_editor(&[0], instance_id, "align-items: center;");
        let _ = cssom_editor(vec![0], instance_id, "align-items: center;".into());
        assert_eq!(field_store_value(field), "left");
        assert_eq!(field_store_value(&textarea), "align-items: left;");
        let error = use_signal(&cssom_error_key(instance_id), String::new).peek();
        assert!(
            error.contains("align-items") && error.contains("left"),
            "{error}"
        );
        assert!(super::super::take_pending_inspector_edits().is_empty());
        let editor = element_style_editor(&[0], instance_id, "align-items: center;");
        use_signal(&input_value_signal_key(field), String::new).set_silent("flex-start".into());
        let apply = super::super::snapshot::find_element_by_template(
            &editor,
            "dt-decl-0-align-items-apply",
        )
        .unwrap();
        let before = DevToolsState::revisions();
        (apply.on_click.as_ref().unwrap().borrow_mut())();
        assert_ne!(DevToolsState::revisions()[2], before[2]);
        let css = "align-items: flex-start;";
        assert!(use_signal(&cssom_error_key(instance_id), String::new)
            .peek()
            .is_empty());
        let pending = super::super::take_pending_inspector_edits();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].css, css);
        assert_eq!(pending[0].instance_id, instance_id);
    });
}

#[test]
fn sync_inspector_applied_cssom_keeps_applied_source() {
    use crate::core::reactivity::create_root;
    use crate::utils::text_field::input_value_signal_key;

    create_root(|| {
        let path = vec![0usize];
        let path_key = super::super::snapshot::path_to_key(&path);
        let field = format!("dt-cssom-{path_key}");
        let applied = "opacity: 0.25;\npadding: 20px;\n";
        use_signal(&input_value_signal_key(&field), String::new)
            .set_silent("opacity: 1.00;\n".into());

        // Simulate post-Apply: store must keep the applied source, not an
        // authored dump that can hide live edits (Button rest chrome, etc.).
        sync_inspector_applied_cssom(&path, applied);

        let cssom = use_signal(&input_value_signal_key(&field), String::new).peek();
        assert_eq!(cssom, applied);
    });
}

#[test]
fn cssom_editor_preserves_nested_horizontal_and_vertical_scroll() {
    use crate::core::reactivity::create_root;
    use crate::style::WhiteSpace;
    use crate::utils::text_field::input_text_template_id;
    create_root(|| {
        let path = vec![0usize];
        let field = cssom_input_id(&path);
        let tid = input_text_template_id(&field);
        let el = cssom_editor(
            path,
            0,
            "padding: 8px 16px; background: #ffffff; border-radius: 6px".into(),
        );
        fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, tid))
        }
        let text = find_tid(&el, &tid).expect("cssom text leaf");
        assert_eq!(text.text_style.white_space, WhiteSpace::Nowrap);
    });
}

#[test]
fn cssom_seed_prefers_committed_apply_text() {
    use crate::core::reactivity::create_root;
    use crate::style::parse_color;
    use crate::utils::text_field::input_value_signal_key;

    create_root(|| {
        super::super::style_edit::clear_committed_inspector_edits_for_test();
        let path = vec![0usize];
        let applied = "background: #00ff00;\nopacity: 0.5;\n";

        // Live style has the apply; authored button dump would show ghost transparent.
        let mut el = Element::new();
        el.authored_style = Some(|element| {
            let mut style = element.style.as_ref().clone();
            style.bg_color = [0.0; 4];
            style.border_width = 0.0;
            Some(style)
        });
        el.style.bg_color = parse_color("#00ff00");
        el.style.opacity = 0.5;
        let root = Element::new().child(el);
        super::super::style_edit::commit_inspector_cssom(&root, &path, applied.into());
        let authored =
            super::super::snapshot::cssom_text_from_element(root.children.first().unwrap());
        assert_ne!(
            authored, applied,
            "authored dump must differ from applied text to prove preference"
        );

        let seed = super::super::style_edit::committed_inspector_cssom(
            &path,
            root.children[0].inspector_instance_id,
        )
        .or_else(|| {
            super::super::snapshot::find_element_by_path(&root, &path)
                .map(super::super::snapshot::cssom_text_from_element)
        })
        .expect("seed");
        assert_eq!(seed, applied);

        let field = format!("dt-cssom-{}", super::super::snapshot::path_to_key(&path));
        seed_edit_field(&field, &seed, true);
        assert_eq!(
            use_signal(&input_value_signal_key(&field), String::new).peek(),
            applied
        );
        super::super::style_edit::clear_committed_inspector_edits_for_test();
    });
}
