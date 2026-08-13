//! WP-2.3: size-overflow promote-to-parent/root + nested flex reflow (no wrong clip).

use crate::core::tree::{Element, LayoutTree};
use taffy::style::Dimension;

fn find_tid<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_tid(c, id))
}

fn find_tid_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
    if el.template_id.as_deref() == Some(id) {
        return Some(el);
    }
    for c in &mut el.children {
        if let Some(h) = find_tid_mut(c, id) {
            return Some(h);
        }
    }
    None
}

/// Nested flex: auto-height mid row wraps when leaf widens; footer must move down.
#[test]
fn nested_flex_child_width_growth_reflows_footer() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new()
        .flex_col()
        .w(200.0)
        .child(Element::new().w(200.0).h(20.0).template_id("header"))
        .child(
            Element::new()
                .flex_row()
                .w(200.0)
                // height Auto → content-sized; leaf width growth can wrap → mid grows.
                .flex_wrap_kw("wrap")
                .gap(4.0)
                .template_id("mid")
                .child(Element::new().w(60.0).h(24.0).template_id("leaf"))
                .child(Element::new().w(60.0).h(24.0).template_id("sib")),
        )
        .child(Element::new().w(200.0).h(20.0).template_id("footer"));

    tree.build_and_layout(&mut root, 220.0, 400.0, [0.0, 0.0]);
    let leaf_node = find_tid(&root, "leaf")
        .and_then(|e| e.layout_node)
        .expect("leaf node");
    let footer_before = find_tid(&root, "footer")
        .map(|e| e.layout_bounds[1])
        .expect("footer y");
    let mid_h_before = find_tid(&root, "mid")
        .map(|e| e.layout_bounds[3])
        .expect("mid h");
    assert!(
        mid_h_before < 40.0,
        "expected single-line mid before growth, h={mid_h_before}"
    );

    // Widen leaf so mid wraps (60+60+gap < 200; 180+60+gap > 200).
    {
        let leaf = find_tid_mut(&mut root, "leaf").expect("leaf");
        leaf.style.size.width = Dimension::Length(180.0);
    }
    let dirty = std::collections::HashSet::from([leaf_node]);
    tree.mark_dirty_nodes_in_tree(&root, &dirty);

    // Overflow rule: common parent `mid` is content-sized → promote to app root.
    assert!(
        tree.resolve_layout_compute_root(&dirty).is_none(),
        "auto-height mid must promote compute root to app root"
    );

    // Live path: mark_dirty + compute from app root.
    tree.build_and_layout_fast_ex(&mut root, 220.0, 400.0, [0.0, 0.0], false, false, None);

    let mid_h_after = find_tid(&root, "mid")
        .map(|e| e.layout_bounds[3])
        .expect("mid h after");
    let footer_after = find_tid(&root, "footer")
        .map(|e| e.layout_bounds[1])
        .expect("footer y after");
    assert!(
        mid_h_after > mid_h_before + 10.0,
        "mid should grow after wrap: before={mid_h_before} after={mid_h_after}"
    );
    assert!(
        footer_after > footer_before + 10.0,
        "footer must reflow down (no wrong clip): before={footer_before} after={footer_after}"
    );
    let leaf_w = find_tid(&root, "leaf")
        .map(|e| e.layout_bounds[2])
        .expect("leaf w");
    assert!(
        (leaf_w - 180.0).abs() < 1.0,
        "leaf width applied, got {leaf_w}"
    );
}

#[test]
fn promote_lifts_past_auto_sized_parent() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new().flex_col().w(300.0).h(300.0).child(
        Element::new()
            .flex_col()
            .w(300.0)
            // Fixed outer panel: promotion should stop here once past auto mid.
            .h(200.0)
            .template_id("panel")
            .child(
                Element::new()
                    .flex_col()
                    .w(300.0)
                    .template_id("mid")
                    .child(Element::new().w(40.0).h(40.0).template_id("leaf")),
            ),
    );

    tree.build_and_layout(&mut root, 300.0, 300.0, [0.0, 0.0]);
    let leaf_node = find_tid(&root, "leaf")
        .and_then(|e| e.layout_node)
        .expect("leaf");
    let mid_node = find_tid(&root, "mid")
        .and_then(|e| e.layout_node)
        .expect("mid");
    let panel_node = find_tid(&root, "panel")
        .and_then(|e| e.layout_node)
        .expect("panel");

    let dirty = std::collections::HashSet::from([leaf_node]);
    let picked = tree
        .pick_subtree_compute_root(&dirty)
        .expect("siblings under mid → pick mid");
    assert_eq!(picked, mid_node);
    assert!(
        tree.node_size_depends_on_descendants(mid_node),
        "mid height Auto → content-sized"
    );
    assert!(
        !tree.node_size_depends_on_descendants(panel_node),
        "panel has definite w/h"
    );

    let resolved = tree
        .resolve_layout_compute_root(&dirty)
        .expect("promote to fixed panel, not app root");
    assert_eq!(
        resolved, panel_node,
        "overflow promote should stop at definite-sized panel"
    );
}

#[test]
fn fixed_parent_keeps_shallow_pick_without_promote() {
    let mut tree = LayoutTree::new();
    let mut root = Element::new().flex_col().w(200.0).h(200.0).child(
        Element::new()
            .flex_col()
            .w(200.0)
            .h(100.0)
            .template_id("panel")
            .child(Element::new().w(50.0).h(20.0).template_id("leaf")),
    );

    tree.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let leaf_node = find_tid(&root, "leaf")
        .and_then(|e| e.layout_node)
        .expect("leaf");
    let panel_node = find_tid(&root, "panel")
        .and_then(|e| e.layout_node)
        .expect("panel");

    let dirty = std::collections::HashSet::from([leaf_node]);
    assert_eq!(tree.pick_subtree_compute_root(&dirty), Some(panel_node));
    assert_eq!(
        tree.resolve_layout_compute_root(&dirty),
        Some(panel_node),
        "definite panel is already a safe shallow root"
    );
}
