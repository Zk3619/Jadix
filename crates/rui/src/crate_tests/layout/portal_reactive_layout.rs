//! Portal-local `reactive_sync` LayoutChanged must not remasure every page text leaf.

use crate::core::tree::{
    dim_kind, DimKind, Element, LayoutStatus, LayoutTree, ReactiveSyncCollect, UnitSlot,
};
use crate::layout::dimension::CssDim;
use crate::layout::layout_tree::PORTAL_LAYER_ID;
use std::cell::Cell;
use std::rc::Rc;

#[test]
fn portal_layout_changed_collects_dirty_without_page_flag() {
    let mut tree = LayoutTree::new();
    let want_h = Rc::new(Cell::new(40.0_f32));

    let want_h_sync = want_h.clone();
    let mut pile = Element::new()
        .w(200.0)
        .h(40.0)
        .template_id("__rui_toast_stack_pile");
    pile = pile.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let h = want_h_sync.get();
        let cur = match dim_kind(el.style.size.height) {
            DimKind::Length(v) => v,
            _ => -1.0,
        };
        if (cur - h).abs() <= 0.5 {
            return LayoutStatus::Clean;
        }
        el.style.set_css_dim(UnitSlot::Height, CssDim::Px(h));
        LayoutStatus::LayoutChanged
    });

    let portal = Element::new()
        .absolute()
        .w("100%")
        .h("100%")
        .template_id(PORTAL_LAYER_ID)
        .child(pile);

    let page_text = Element::new()
        .text("gallery page label that must not remasure")
        .text_size(14.0)
        .w(200.0)
        .template_id("page-text");

    let mut root = Element::new()
        .w(800.0)
        .h(600.0)
        .child(page_text)
        .child(portal);

    tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
    let page_node = root.children[0]
        .layout_node
        .expect("page text taffy node");
    let pile_node = root.children[1].children[0]
        .layout_node
        .expect("pile taffy node");
    assert!(!tree.is_node_dirty(page_node));
    assert!(!tree.is_node_dirty(pile_node));

    want_h.set(120.0);
    let mut impact = LayoutStatus::Clean;
    let mut collect = ReactiveSyncCollect::default();
    crate::core::tree::sync_reactive_elements_subtree_ex(
        &mut root,
        &mut tree,
        &mut impact,
        800.0,
        600.0,
        [0.0, 0.0],
        false,
        &mut collect,
    );

    assert_eq!(impact, LayoutStatus::LayoutChanged);
    assert!(
        collect.portal_layout_changed,
        "pile under portal layer must set portal_layout_changed"
    );
    assert!(
        !collect.page_layout_changed,
        "page tree must stay clean for portal-only height sync"
    );
    assert!(
        collect.layout_dirty.contains(&pile_node),
        "pile node must be queued for mark_dirty, got {:?}",
        collect.layout_dirty
    );
    assert!(
        !collect.layout_dirty.contains(&page_node),
        "page text must not be in reactive dirty set"
    );

    // Simulate AppState LayoutChanged path: mark only collected dirty nodes.
    let dirty: std::collections::HashSet<_> = collect.layout_dirty.into_iter().collect();
    let calls_before = tree.mark_dirty_calls;
    tree.mark_dirty_nodes_in_tree(&root, &dirty);
    assert!(tree.mark_dirty_calls > calls_before);
    assert!(
        tree.is_node_dirty(pile_node),
        "portal pile must be dirty after targeted mark"
    );
    assert!(
        !tree.is_node_dirty(page_node),
        "page text must stay clean — mark_all_text_leaves_dirty would have dirtied it"
    );

    tree.build_and_layout_fast_ex(&mut root, 800.0, 600.0, [0.0, 0.0], false, false, None);
    let after_h = root.children[1].children[0].layout_bounds[3];
    assert!(
        (after_h - 120.0).abs() < 0.5,
        "portal pile height should update without remasuring page text, got {after_h}"
    );
}
