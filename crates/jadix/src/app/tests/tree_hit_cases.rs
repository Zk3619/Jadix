#[test]
fn body_portal_hit_order_matches_paint_order_above_max_page_z() {
    let portal = Element::new().template_id(crate::layout::layout_tree::PORTAL_LAYER_ID);
    let page = Element::new().z_index(i32::MAX);
    let root = Element::new().child(portal).child(page);
    assert_eq!(hit_test_child_indices(&root), [1, 0]);
    assert!(!children_share_same_z_index(&root));
}
