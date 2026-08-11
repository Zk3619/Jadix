//! LayoutChanged path: subtree `mark_dirty` + compute from app root (not subtree parent).

use crate::core::tree::{Element, LayoutTree};
use taffy::style::Dimension;

#[test]
fn layout_changed_mark_dirty_updates_leaf_size() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .flex_col()
        .child(Element::new().w(100.0).h(40.0))
        .child(
            Element::new()
                .flex_col()
                .w(100.0)
                .child(Element::new().w(100.0).h(50.0).template_id("leaf")),
        );

    tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let leaf_node = root.children[1].children[0]
        .layout_node
        .expect("leaf should have a taffy node after build");
    let root_node = tree.root_node.expect("app root");
    assert!(
        !tree.is_node_dirty(leaf_node),
        "fresh compute should clear dirty"
    );
    let before_h = root.children[1].children[0].layout_bounds[3];
    assert!(
        (before_h - 50.0).abs() < 0.5,
        "expected leaf height ~50, got {before_h}"
    );

    let calls_before = tree.mark_dirty_calls;
    root.children[1].children[0].style.size.height = Dimension::Length(80.0);
    tree.mark_element_layout_dirty(&root.children[1].children[0]);
    assert!(
        tree.mark_dirty_calls > calls_before,
        "LayoutChanged path must call taffy mark_dirty"
    );
    assert!(
        tree.is_node_dirty(leaf_node),
        "leaf must be dirty after mark_element_layout_dirty"
    );

    let dirty = std::collections::HashSet::from([leaf_node]);
    // pick may succeed, but live path must still compute from app root (location safety).
    let _picked = tree.pick_subtree_compute_root(&dirty);
    tree.build_and_layout_fast_ex(
        &mut root,
        200.0,
        200.0,
        [0.0, 0.0],
        false,
        false,
        None,
    );
    assert_eq!(
        tree.last_compute_root,
        Some(root_node),
        "LayoutChanged must compute from app root so subtree location is not zeroed"
    );
    let after_h = root.children[1].children[0].layout_bounds[3];
    assert!(
        (after_h - 80.0).abs() < 0.5,
        "expected leaf height ~80 after LayoutChanged mark_dirty, got {after_h}"
    );
    assert!(
        !tree.is_node_dirty(leaf_node),
        "root compute_layout should clear dirty"
    );
}

#[test]
fn mark_all_text_leaves_dirty_touches_text_nodes() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new().flex_col().child(
        Element::new()
            .text("hello wrap me please")
            .text_size(14.0)
            .w(80.0)
            .template_id("txt"),
    );
    tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let text_node = root.children[0]
        .layout_node
        .expect("text leaf must have taffy node");
    assert!(!tree.is_node_dirty(text_node));

    let calls_before = tree.mark_dirty_calls;
    tree.mark_all_text_leaves_dirty(&root);
    assert!(
        tree.mark_dirty_calls > calls_before,
        "must mark_dirty text leaves for settle/fit remasure"
    );
    assert!(
        tree.is_node_dirty(text_node),
        "text leaf must be dirty after mark_all_text_leaves_dirty"
    );
}
