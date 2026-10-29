use super::*;
use crate::core::reactivity::create_root;
fn tab(key: &str) -> DockTab {
    DockTab::new(key, key, Element::new)
}
fn mount_dock(dock: DockState) -> Rc<std::cell::RefCell<Element>> {
    use crate::app::{reactive, runtime};
    use crate::core::reactivity::{create_effect, isolate_reads};
    reactive::reset_registered_anchors();
    runtime::clear_test_reconcile_root();
    let live = Rc::new(std::cell::RefCell::new(Element::new()));
    runtime::install_test_reconcile_root(live.clone());
    let tree = live.clone();
    create_effect(move || {
        *tree.borrow_mut() = isolate_reads(|| dock.clone().into_element());
    });
    reactive::drain_deferred_view_remounts();
    live
}
#[test]
fn central_three_quarters_merges_and_only_outer_edges_split() {
    let bounds = [100.0, 200.0, 800.0, 400.0];
    assert_eq!(
        DockPlacement::at(bounds, [220.0, 400.0], 0.75),
        DockPlacement::Center
    );
    assert_eq!(
        DockPlacement::at(bounds, [500.0, 260.0], 0.75),
        DockPlacement::Center
    );
    assert_eq!(
        DockPlacement::at(bounds, [180.0, 400.0], 0.75),
        DockPlacement::Left
    );
    assert_eq!(
        DockPlacement::at(bounds, [500.0, 560.0], 0.75),
        DockPlacement::Bottom
    );
    assert_eq!(
        DockPlacement::at(bounds, [220.0, 400.0], 0.5),
        DockPlacement::Left
    );
}
#[test]
fn dragging_over_a_panel_body_does_not_reserve_a_sortable_row() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "body-drop-regression",
            DockLayout::split(
                "columns",
                DockOrientation::Horizontal,
                [50.0, 50.0],
                [
                    DockLayout::tabs("source", [tab("a"), tab("b")]),
                    DockLayout::tabs("target", [tab("c")]),
                ],
            ),
        );
        let live = mount_dock(dock);
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        layout.build_and_layout(&mut live.borrow_mut(), 1000.0, 700.0, [0.0; 2]);
        let hits = drag::collect_hits(&live.borrow());
        let source = hits
            .iter()
            .find(|hit| hit.id == "source__dock_tab_a")
            .unwrap();
        let target = hits
            .iter()
            .find(|hit| hit.id == "target__dock_drop")
            .unwrap();
        let before = target.rect.to_bounds();
        drag::begin(drag::BeginDrag {
            scope_id: "body-drop-regression__dock_drag".into(),
            draggable_id: source.id.clone(),
            pointer: [source.rect.x + 5.0, source.rect.y + 5.0],
            grab_offset: [5.0; 2],
            start_bounds: source.rect.to_bounds(),
            from_list: Some("source__dock_tabs".into()),
            from_index: Some(0),
            item_type: "dock-tab".into(),
            group: String::new(),
            capture_id: "capture".into(),
        });
        drag::tick(
            before[0] + before[2] / 2.0,
            before[1] + before[3] / 2.0,
            &hits,
            [1000.0, 700.0],
        );
        crate::app::reactive::drain_deferred_view_remounts();
        let mut root = live.borrow_mut();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut root,
            &mut layout,
            &mut impact,
            1000.0,
            700.0,
            [0.0; 2],
        );
        fn body(element: &Element) -> Option<&Element> {
            if element
                .get_attr(drag::ATTR_DRAG_DROPPABLE)
                .map(String::as_str)
                == Some("target__dock_drop")
            {
                return Some(element);
            }
            element.children.iter().find_map(body)
        }
        let target = body(&root).unwrap();
        assert_eq!(
            crate::layout::lp_length_px(target.style.padding.bottom).unwrap_or(0.0),
            0.0
        );
        assert_eq!(
            crate::layout::lp_length_px(target.style.padding.right).unwrap_or(0.0),
            0.0
        );
        assert!(target.get_attr(drag::ATTR_DRAG_LIST).is_none());
        drop(root);
        let _ = drag::end(&hits);
        crate::app::runtime::clear_test_reconcile_root();
        crate::app::reactive::reset_registered_anchors();
    });
}
#[test]
fn docking_keeps_nested_groups_filling_their_panels() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "fill-regression",
            DockLayout::split(
                "columns",
                DockOrientation::Horizontal,
                [20.0, 80.0],
                [
                    DockLayout::tabs("files", [tab("files")]),
                    DockLayout::split(
                        "rows",
                        DockOrientation::Vertical,
                        [75.0, 25.0],
                        [
                            DockLayout::tabs("editor", [tab("web"), tab("notes")]),
                            DockLayout::tabs("tools", [tab("terminal"), tab("problems")]),
                        ],
                    ),
                ],
            ),
        );
        dock.move_tab("tools", "problems", "editor", 2, DockPlacement::Center);
        dock.move_tab("editor", "notes", "tools", 1, DockPlacement::Right);
        let live = mount_dock(dock);
        let mut root = live.borrow_mut();
        let mut layout = crate::layout::layout_tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 1000.0, 700.0, [0.0; 2]);
        fn check(element: &Element) -> usize {
            let mut panels = 0;
            if element.get_attr("data-slot").map(String::as_str) == Some("resizable-panel") {
                panels += 1;
                assert!(element.layout_bounds[3] > 0.0);
                let content = &element.children[0];
                assert!(
                    (element.layout_bounds[3] - content.layout_bounds[3]).abs() < 1.0,
                    "panel {:?} content {:?}",
                    element.layout_bounds,
                    content.layout_bounds
                );
            }
            for child in &element.children {
                panels += check(child);
            }
            panels
        }
        assert!(check(&root) >= 4);
        drop(root);
        crate::app::runtime::clear_test_reconcile_root();
        crate::app::reactive::reset_registered_anchors();
    });
}
#[test]
fn docking_moves_splits_collapses_restores_and_resets() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "test",
            DockLayout::split(
                "root",
                DockOrientation::Horizontal,
                [20.0, 80.0],
                [
                    DockLayout::tabs("left", [tab("files").closable(false)]),
                    DockLayout::tabs("editor", [tab("web"), tab("notes")]),
                ],
            ),
        );
        assert!(dock.move_tab("editor", "notes", "left", 0, DockPlacement::Center));
        assert_eq!(
            dock.data.peek().layout.group("left").unwrap()[0].key,
            "notes"
        );
        assert!(dock.move_tab("left", "notes", "editor", 0, DockPlacement::Bottom));
        let data = dock.data.peek();
        let DockLayout::Split { children, .. } = &data.layout else {
            panic!()
        };
        let DockLayout::Split { children, .. } = &children[1] else {
            panic!()
        };
        let group = children[1].id().to_string();
        dock.close(&group, "notes");
        assert_eq!(dock.data.peek().layout.group("editor").unwrap().len(), 1);
        dock.restore_closed();
        assert!(dock.data.peek().closed.is_empty());
        dock.close("left", "files");
        assert!(dock
            .data
            .peek()
            .layout
            .group("left")
            .unwrap()
            .iter()
            .any(|t| t.key == "files"));
        assert!(!dock.move_tab("editor", "web", "missing", 0, DockPlacement::Right));
        dock.reset();
        assert_eq!(dock.data.peek().layout.group("editor").unwrap().len(), 2);
    });
}
#[test]
fn moving_within_one_target_keeps_the_preview_pointer_live() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "live-preview",
            DockLayout::tabs("editor", [tab("a"), tab("b")]),
        );
        let _workspace = dock.clone().into_element();
        let scope = "live-preview__dock_drag";
        let mut body = Element::new();
        body.layout_bounds = [0.0, 100.0, 800.0, 600.0];
        body.set_attr(drag::ATTR_DRAG_SCOPE, scope);
        body.set_attr(drag::ATTR_DRAG_DROPPABLE, "editor__dock_drop");
        body.set_attr(drag::ATTR_DRAG_ACCEPT, "dock-tab");
        let hits = drag::collect_hits(&body);
        drag::begin(drag::BeginDrag {
            scope_id: scope.into(),
            draggable_id: "a".into(),
            pointer: [30.0, 20.0],
            grab_offset: [10.0, 10.0],
            start_bounds: [20.0, 10.0, 100.0, 30.0],
            from_list: None,
            from_index: None,
            item_type: "dock-tab".into(),
            group: String::new(),
            capture_id: "capture".into(),
        });
        assert!(drag::tick(400.0, 300.0, &hits, [1000.0, 800.0]));
        assert_eq!(dock.pointer.peek(), [400.0, 300.0]);
        assert_eq!(
            DockPlacement::at(
                hits[0].rect.to_bounds(),
                dock.pointer.peek(),
                dock.center_drop_ratio
            ),
            DockPlacement::Center
        );
        assert!(drag::tick(780.0, 300.0, &hits, [1000.0, 800.0]));
        assert_eq!(dock.pointer.peek(), [780.0, 300.0]);
        assert_eq!(
            DockPlacement::at(
                hits[0].rect.to_bounds(),
                dock.pointer.peek(),
                dock.center_drop_ratio
            ),
            DockPlacement::Right
        );
        let _ = drag::end(&hits);
    });
}
#[test]
fn forward_reorder_commits_the_same_slot_as_the_drag_preview() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "reorder-forward",
            DockLayout::tabs("editor", [tab("a"), tab("b"), tab("c"), tab("d")]),
        );
        assert!(dock.move_tab("editor", "a", "editor", 1, DockPlacement::Center));
        assert_eq!(
            dock.data
                .peek()
                .layout
                .group("editor")
                .unwrap()
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            ["b", "a", "c", "d"]
        );
        assert!(dock.move_tab("editor", "b", "editor", 3, DockPlacement::Center));
        assert_eq!(
            dock.data
                .peek()
                .layout
                .group("editor")
                .unwrap()
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            ["a", "c", "d", "b"]
        );
    });
}
#[test]
fn reordering_and_closing_last_tab_keep_a_restorable_group() {
    let (_owner, ()) = create_root(|| {
        let dock = DockState::new(
            "reorder",
            DockLayout::tabs("editor", [tab("a"), tab("b"), tab("c")]),
        );
        assert!(dock.move_tab("editor", "a", "editor", 3, DockPlacement::Center));
        assert_eq!(
            dock.data
                .peek()
                .layout
                .group("editor")
                .unwrap()
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        for key in ["a", "b", "c"] {
            dock.close("editor", key);
        }
        dock.restore_closed();
        assert_eq!(dock.data.peek().layout.group("editor").unwrap()[0].key, "c");
        assert!(!dock.move_tab("editor", "c", "editor", 0, DockPlacement::Left));
    });
}
