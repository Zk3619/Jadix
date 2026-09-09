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
