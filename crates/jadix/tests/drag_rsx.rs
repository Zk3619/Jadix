//! WP-DRAG.1: `<DragProvider>` RSX and Builder share the same scope suffix.

use jadix::core::reactivity::create_root;
use jadix::core::tree::Element;
use jadix::primitives::vstack;
use jadix::ui;
use jadix::utils::drag::{drag_scope, draggable, CollisionAlg};

fn collect_tids(el: &Element, out: &mut Vec<String>) {
    if let Some(id) = &el.template_id {
        out.push(id.clone());
    }
    for c in &el.children {
        collect_tids(c, out);
    }
}

#[test]
fn rsx_and_builder_share_scope_suffix() {
    let (_o, (rsx, builder)) = create_root(|| {
        let rsx = ui! {
            <DragProvider id="drag-rsx">
                <Draggable id="a">
                    { Element::new().text("a") }
                </Draggable>
                <Droppable id="z">
                    { Element::new().text("z") }
                </Droppable>
            </DragProvider>
        };
        let builder = drag_scope("drag-builder")
            .collision(CollisionAlg::ClosestCenter)
            .child(|| Element::new().text("b"));
        (rsx, builder)
    });
    let mut rsx_ids = Vec::new();
    collect_tids(&rsx, &mut rsx_ids);
    let mut b_ids = Vec::new();
    collect_tids(&builder, &mut b_ids);
    assert!(
        rsx_ids.iter().any(|t| t.ends_with("__drag_scope")),
        "RSX scope tid missing: {rsx_ids:?}"
    );
    assert!(
        b_ids.iter().any(|t| t.ends_with("__drag_scope")),
        "Builder scope tid missing: {b_ids:?}"
    );
    assert!(
        rsx_ids
            .iter()
            .any(|t| t.contains("__draggable:") || t.contains("drag-rsx__draggable")),
        "RSX draggable island missing: {rsx_ids:?}"
    );
}

#[test]
fn for_keyed_does_not_steal_draggable_island_id() {
    let (_o, per_row) = create_root(|| {
        let out = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Vec<String>>::new()));
        let out2 = out.clone();
        let _root = drag_scope("s").child(move || {
            let items = ["Mercury".to_string(), "Venus".to_string()];
            let rows = jadix::for_keyed!(
                "gal-drag-keyed",
                items.as_slice(),
                |s: &String| s.clone(),
                |name: &String| {
                    draggable(name.clone())
                        .also_droppable(true)
                        .list("planets")
                        .index(0)
                        .child({
                            let name = name.clone();
                            move || Element::new().text(name.clone())
                        })
                }
            );
            let mut collected = Vec::new();
            for row in &rows {
                let mut ids = Vec::new();
                collect_tids(row, &mut ids);
                collected.push(ids);
            }
            *out2.borrow_mut() = collected;
            Element::from(vstack().children(rows))
        });
        let x = out.borrow().clone();
        x
    });
    assert_eq!(per_row.len(), 2, "{per_row:?}");
    for ids in &per_row {
        assert!(
            ids.iter().any(|t| t.contains("__draggable:")),
            "for_keyed stole the reactive island id: {ids:?}"
        );
        assert!(
            ids.iter().any(|t| t.starts_with("gal-drag-keyed:")),
            "for_keyed row key missing: {ids:?}"
        );
    }
}
