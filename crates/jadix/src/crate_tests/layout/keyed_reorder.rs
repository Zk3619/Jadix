//! Keyed child reorder is `LayoutChanged` (same Taffy nodes). The live frame uses
//! `build_and_layout_fast`, which must `set_children` so flex pack follows the new
//! `element.children` order — otherwise sortable lists snap back visually.

use crate::core::tree::{Element, LayoutTree};

#[test]
fn keyed_reorder_fast_layout_updates_pack_order() {
    let mut tree = LayoutTree::new();
    let mut a = Element::new().w(80.0).h(32.0).template_id("k:a");
    a.text = Some("Alpha".into());
    let mut b = Element::new().w(80.0).h(32.0).template_id("k:b");
    b.text = Some("Beta".into());
    let mut root = Element::new()
        .flex_col()
        .w(80.0)
        .gap(8.0)
        .child(a)
        .child(b)
        .template_id("list");

    tree.build_and_layout(&mut root, 80.0, 200.0, [0.0, 0.0]);
    let ia = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some("k:a"))
        .expect("Alpha row");
    let ib = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some("k:b"))
        .expect("Beta row");
    let y_a0 = root.children[ia].layout_bounds[1];
    let y_b0 = root.children[ib].layout_bounds[1];
    assert!(
        y_a0 < y_b0,
        "precondition: Alpha above Beta; ya={y_a0} yb={y_b0}"
    );

    root.children.swap(ia, ib);
    assert_eq!(root.children[ib].template_id.as_deref(), Some("k:a"));
    assert_eq!(root.children[ia].template_id.as_deref(), Some("k:b"));

    tree.build_and_layout_fast_ex(&mut root, 80.0, 200.0, [0.0, 0.0], false, false, None);

    let ia = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some("k:a"))
        .expect("Alpha row after swap");
    let ib = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some("k:b"))
        .expect("Beta row after swap");
    let y_a = root.children[ia].layout_bounds[1];
    let y_b = root.children[ib].layout_bounds[1];
    assert!(
        y_b < y_a,
        "fast layout must pack the new child order; Beta y={y_b} Alpha y={y_a} (before ya={y_a0} yb={y_b0})"
    );
}

/// Cross-list keyed move (kanban / sortable). The fast path used to
/// `remove_taffy_subtree` as soon as a parent no longer listed the child, then
/// `set_children` on the destination panicked: `invalid SlotMap key used`.
#[test]
fn keyed_cross_parent_move_fast_layout_keeps_taffy_node() {
    let mut tree = LayoutTree::new();
    let mut item = Element::new().w(80.0).h(32.0).template_id("k:item");
    item.text = Some("Item".into());
    let list_a = Element::new()
        .flex_col()
        .w(80.0)
        .h(80.0)
        .child(item)
        .template_id("list-a");
    let list_b = Element::new()
        .flex_col()
        .w(80.0)
        .h(80.0)
        .template_id("list-b");
    let mut root = Element::new()
        .flex_row()
        .w(160.0)
        .h(80.0)
        .child(list_a)
        .child(list_b)
        .template_id("row");

    tree.build_and_layout(&mut root, 160.0, 80.0, [0.0, 0.0]);
    let item_id = root.children[0].children[0]
        .layout_node
        .expect("item taffy id");

    let moved = root.children[0].children.remove(0);
    root.children[1].children.push(moved);

    tree.build_and_layout_fast_ex(&mut root, 160.0, 80.0, [0.0, 0.0], false, false, None);

    assert!(
        root.children[0].children.is_empty(),
        "source list must be empty after the move"
    );
    assert_eq!(root.children[1].children.len(), 1);
    assert_eq!(
        root.children[1].children[0].layout_node,
        Some(item_id),
        "moved item must keep the same Taffy node"
    );
    let bounds = root.children[1].children[0].layout_bounds;
    assert!(
        bounds[2] > 0.0 && bounds[3] > 0.0,
        "moved item must still have a laid-out box; got {bounds:?}"
    );
}

/// Nodes that left the element tree still have to be dropped from Taffy after
/// the reparent pass (the old parent-local remove is gone).
#[test]
fn keyed_drop_child_fast_layout_removes_orphan() {
    let mut tree = LayoutTree::new();
    let mut a = Element::new().w(80.0).h(32.0).template_id("k:a");
    a.text = Some("Alpha".into());
    let mut b = Element::new().w(80.0).h(32.0).template_id("k:b");
    b.text = Some("Beta".into());
    let mut root = Element::new()
        .flex_col()
        .w(80.0)
        .gap(8.0)
        .child(a)
        .child(b)
        .template_id("list");

    tree.build_and_layout(&mut root, 80.0, 200.0, [0.0, 0.0]);
    let ib = root
        .children
        .iter()
        .position(|c| c.template_id.as_deref() == Some("k:b"))
        .expect("Beta row");
    root.children.remove(ib);
    let nodes_before = tree.stats().taffy_nodes;

    tree.build_and_layout_fast_ex(&mut root, 80.0, 200.0, [0.0, 0.0], false, false, None);

    assert!(
        root.children
            .iter()
            .all(|c| c.template_id.as_deref() != Some("k:b")),
        "Beta must leave the element tree"
    );
    assert!(
        tree.stats().taffy_nodes < nodes_before,
        "dropped child's Taffy node must be removed"
    );
}
