use external_dnd::utils::drag;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use ui_runtime::core::reactivity::create_root;
use ui_runtime::core::tree::Element;
use ui_runtime::utils::drag::runtime::{self, MoveMode};

#[test]
fn copied_dnd_product_uses_the_host_protocol_and_owns_its_session() {
    let (_owner, ()) = create_root(|| {
        let tree = Rc::new(RefCell::new(Element::new()));
        let ended = Rc::new(Cell::new(false));
        let callback_tree = tree.clone();
        let callback_ended = ended.clone();
        *tree.borrow_mut() = drag::drag_scope("copied-dnd")
            .autoscroll(false)
            .on_drag_end(move |_| {
                callback_tree.borrow_mut().text = Some("external drop".into());
                callback_ended.set(true);
            })
            .child(Element::new);
        drag::begin(drag::BeginDrag {
            scope_id: "copied-dnd".into(),
            draggable_id: "card".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "card".into(),
            group: String::new(),
            capture_id: "external-card".into(),
        });
        assert!(drag::is_dragging());
        assert!(runtime::is_dragging());
        let update = runtime::prepare_move(
            Some(&tree.borrow()),
            [30.0, 10.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(update.dispatch());
        let end = runtime::prepare_end(&tree.borrow());
        assert!(end.dispatch());
        assert!(ended.get());
        assert_eq!(tree.borrow().text.as_deref(), Some("external drop"));
        runtime::unregister();
    });
}

#[test]
fn product_macro_lowers_to_the_copied_dnd_implementation() {
    create_root(|| {
        let root = external_dnd::ui! {
            <DragProvider id="external-macro" autoscroll={false}>
                <Draggable id="external-card"><Box>"external source"</Box></Draggable>
                <Droppable id="external-target"><Box>"target"</Box></Droppable>
            </DragProvider>
        };
        assert_eq!(root.attrs.get("jadix_drag_scope").map(String::as_str), Some("external-macro"));
        drag::begin(drag::BeginDrag {
            scope_id: "external-macro".into(), draggable_id: "external-card".into(),
            pointer: [10.0, 10.0], grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 40.0, 24.0],
            from_list: None, from_index: None, item_type: String::new(), group: String::new(),
            capture_id: "external-card".into(),
        });
        assert!(drag::is_dragging());
        assert!(runtime::is_dragging());
        drag::cancel();
        drop(root);
    });
}

#[test]
fn copied_dnd_owner_disposal_and_new_session_reject_stale_work() {
    let (owner, root) = create_root(|| drag::drag_scope("copied-owner").child(Element::new));
    let begin = || {
        drag::begin(drag::BeginDrag {
            scope_id: "copied-owner".into(),
            draggable_id: "card".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0; 2],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: None,
            from_index: None,
            item_type: "card".into(),
            group: String::new(),
            capture_id: "copied-card".into(),
        })
    };
    begin();
    let movement =
        runtime::prepare_move(Some(&root), [30.0, 10.0], [400.0, 300.0], MoveMode::Follow);
    let ending = runtime::prepare_end(&root);
    drag::cancel();
    begin();
    assert!(!movement.dispatch());
    assert!(!ending.dispatch());
    assert!(drag::is_dragging());
    owner.dispose();
    assert!(!drag::session::scope_exists("copied-owner"));
    assert!(!drag::is_dragging());
    runtime::unregister();
}

#[test]
fn copied_overlay_builds_and_collects_through_public_layout() {
    let (_owner, ()) = create_root(|| {
        let mut root = drag::drag_scope("external-overlay").child(|| {
            Element::new().child(drag::drag_overlay().child(|| Element::new().w(40.0).h(24.0)))
        });
        let mut layout = ui_runtime::layout::LayoutTree::new();
        layout.build_and_layout(&mut root, 400.0, 300.0, [-1.0, -1.0]);
        assert!(root.layout_bounds.iter().all(|value| value.is_finite()));
        assert!(root.layout_bounds[2] > 0.0);
        assert!(!runtime::is_dragging());
        runtime::unregister();
    });
}
