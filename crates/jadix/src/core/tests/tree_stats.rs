use super::*;
use crate::core::tree::Element;

#[test]
fn count_small_tree() {
    let mut root = Element::new().w("100%").h("100%");
    root.children.push(Element::new().text("hi"));
    let mut leaf = Element::new();
    leaf.set_attr("role", "button");
    leaf.on_click = Some(std::rc::Rc::new(std::cell::RefCell::new(|| {})));
    root.children.push(leaf);

    let s = count_element_tree(&root);
    assert_eq!(s.nodes, 3);
    assert_eq!(s.max_depth, 1);
    assert_eq!(s.text_bytes, 2);
    assert_eq!(s.attr_entries, 1);
    assert_eq!(s.handler_nodes, 1);
}

#[test]
fn layout_stats_tracks_taffy_nodes() {
    let mut root = Element::new().w("200px").h("200px");
    root.children.push(Element::new().w("50px").h("50px"));
    let mut layout = LayoutTree::new();
    layout.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
    let s = layout.stats();
    let e = count_element_tree(&root);
    assert!(s.has_root);
    assert!(s.taffy_nodes >= 2);
    assert_eq!(s.taffy_nodes, e.nodes);
    assert!(s.text_measurements <= s.taffy_nodes);
}
