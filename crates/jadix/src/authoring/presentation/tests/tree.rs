use super::*;
use std::cell::Cell;

#[test]
fn reparenting_preserves_nodes_and_retires_later_removed_operations() {
    let called = Rc::new(Cell::new(false));
    let mut root = Element::new();
    let mut first = Element::new();
    attach_tree(&mut first, Phase::State, |root, _, _| {
        let retained = root.children.remove(1).children.remove(0);
        root.children[0].children.push(retained);
        LayoutStatus::StructureChanged
    });
    let mut second = Element::new();
    let later = called.clone();
    attach_tree(&mut second, Phase::State, move |_, _, _| {
        later.set(true);
        LayoutStatus::Clean
    });
    second.children.push(Element::new().text("retained"));
    root.children = vec![first, second];
    let status = super::super::sync_tree(
        &mut root,
        &mut LayoutTree::new(),
        Context::new(Phase::State, [800.0, 600.0]),
    );
    assert_eq!(status, LayoutStatus::StructureChanged);
    assert!(!called.get());
    assert_eq!(
        root.children[0].children[0].text.as_deref(),
        Some("retained")
    );
}
#[test]
fn removed_and_disposed_tree_effects_restore_paint_once() {
    for dispose_owner in [false, true] {
        let calls = Rc::new(Cell::new(0));
        let cleaned = calls.clone();
        let (owner, node) = crate::core::reactivity::create_root(|| {
            let mut node = Element::new();
            attach_tree_with_cleanup(
                &mut node,
                Phase::BeforeLayout,
                |root, _, _| {
                    root.style.opacity = 0.5;
                    LayoutStatus::StyleChanged
                },
                move |root, _, _| {
                    cleaned.set(cleaned.get() + 1);
                    root.style.opacity = 1.0;
                    LayoutStatus::StyleChanged
                },
            );
            node
        });
        let mut root = Element::new().child(node);
        let mut layout = LayoutTree::new();
        let context = Context::new(Phase::BeforeLayout, [800.0, 600.0]);
        super::super::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.style.opacity, 0.5);
        if dispose_owner {
            owner.dispose();
        } else {
            root.children.clear();
        }
        super::super::sync_tree(&mut root, &mut layout, context);
        assert_eq!(root.style.opacity, 1.0);
        super::super::sync_tree(&mut root, &mut layout, context);
        assert_eq!(calls.get(), 1);
    }
}
