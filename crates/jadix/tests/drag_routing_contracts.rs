use jadix::core::tree::Element;
use jadix::utils::drag::routing::{self, DragHandlers, ScrollPolicy};
use std::cell::{Cell, RefCell};

thread_local! {
    static TREE: RefCell<Element> = RefCell::new(Element::new());
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static PENDING: Cell<Option<[f32; 2]>> = const { Cell::new(None) };
}

fn event(name: &'static str) {
    EVENTS.with(|events| events.borrow_mut().push(name));
}

fn moved(x: f32, y: f32) -> bool {
    PENDING.with(|pending| pending.set(None));
    TREE.with(|tree| tree.borrow_mut().text = Some(format!("{x},{y}")));
    event("move");
    true
}

fn finished() {
    routing::unregister("external-chart");
    PENDING.with(|pending| pending.set(None));
    event("finish");
}

fn sampled(_root: &Element, x: f32, y: f32) {
    PENDING.with(|pending| pending.set(Some([x, y])));
    event("sample");
}

fn drained() {
    let pending = PENDING.with(|pending| pending.take());
    if let Some([x, y]) = pending {
        moved(x, y);
    }
}

fn chart() {
    routing::register(
        "external-chart",
        DragHandlers::new(moved, finished)
            .scroll_policy(ScrollPolicy::Freeze)
            .frame_sampling(sampled, drained),
    );
}

fn reset() {
    routing::finish_all();
    EVENTS.with(|events| events.borrow_mut().clear());
    PENDING.with(|pending| pending.set(None));
    TREE.with(|tree| *tree.borrow_mut() = Element::new());
}

fn events() -> Vec<&'static str> {
    EVENTS.with(|events| events.borrow().clone())
}

#[test]
fn product_handlers_sample_under_borrow_and_move_after_release() {
    reset();
    chart();
    let movement = TREE.with(|tree| {
        let borrowed = tree.borrow_mut();
        let movement = routing::prepare_move(Some(&borrowed), 30.0, 40.0);
        assert!(borrowed.text.is_none());
        assert_eq!(events(), ["sample"]);
        movement
    });
    assert!(movement.dispatch());
    TREE.with(|tree| assert_eq!(tree.borrow().text.as_deref(), Some("30,40")));
    routing::finish_all();
    assert_eq!(events(), ["sample", "move", "finish"]);
    assert!(!routing::blocks_content_pan());
}

#[test]
fn frame_sampling_defers_writes_until_the_host_releases_its_tree() {
    reset();
    chart();
    TREE.with(|tree| {
        let borrowed = tree.borrow_mut();
        routing::queue_frame(&borrowed, 10.0, 20.0);
        assert!(borrowed.text.is_none());
    });
    routing::drain_frame();
    TREE.with(|tree| assert_eq!(tree.borrow().text.as_deref(), Some("10,20")));
    routing::drain_frame();
    assert_eq!(events(), ["sample", "move"]);
    routing::finish_all();
}

#[test]
fn replacement_or_finish_invalidates_an_already_prepared_move() {
    reset();
    chart();
    let previous = routing::prepare_move(Some(&Element::new()), 1.0, 2.0);
    chart();
    assert!(!previous.dispatch());
    let current = routing::prepare_move(Some(&Element::new()), 3.0, 4.0);
    routing::finish_all();
    assert!(!current.dispatch());
    routing::drain_frame();
    TREE.with(|tree| assert!(tree.borrow().text.is_none()));
    assert_eq!(events(), ["sample", "sample", "finish"]);
}

fn unregister_chart(_x: f32, _y: f32) -> bool {
    routing::unregister("external-chart");
    event("remove");
    false
}

#[test]
fn move_callbacks_can_remove_later_handlers_without_borrow_conflicts() {
    reset();
    routing::register("remover", DragHandlers::new(unregister_chart, || {}));
    chart();
    assert!(!routing::prepare_move(Some(&Element::new()), 1.0, 2.0).dispatch());
    assert_eq!(events(), ["sample", "remove"]);
    routing::finish_all();
}

fn finish_and_restart() {
    routing::finish_all();
    routing::register("restarted", DragHandlers::new(moved, finished));
    event("restart");
}

#[test]
fn finish_detaches_handlers_before_reentrant_cleanup_and_new_registration() {
    reset();
    routing::register("old", DragHandlers::new(moved, finish_and_restart));
    routing::finish_all();
    assert_eq!(events(), ["restart"]);
    assert!(routing::prepare_move(None, 5.0, 6.0).dispatch());
    routing::finish_all();
    routing::finish_all();
    assert_eq!(events(), ["restart", "move", "finish"]);
}

#[test]
fn products_choose_scroll_policy_and_tree_sampling_requirements() {
    reset();
    routing::register(
        "splitter",
        DragHandlers::new(moved, || {}).scroll_policy(ScrollPolicy::BlockPan),
    );
    assert!(routing::blocks_content_pan());
    assert!(!routing::freezes_scroll());
    chart();
    assert!(routing::freezes_scroll());
    assert!(routing::prepare_move(None, 8.0, 9.0).dispatch());
    assert_eq!(events(), ["move"]);
    routing::unregister("external-chart");
    assert!(!routing::freezes_scroll());
    routing::finish_all();
    assert!(!routing::blocks_content_pan());
    routing::register("swipe", DragHandlers::new(moved, || {}));
    assert!(!routing::blocks_content_pan());
    routing::finish_all();
}

#[test]
fn newest_pointer_move_supersedes_a_queued_frame_sample() {
    reset();
    chart();
    routing::queue_frame(&Element::new(), 10.0, 20.0);
    assert!(routing::prepare_move(Some(&Element::new()), 80.0, 70.0).dispatch());
    routing::drain_frame();
    TREE.with(|tree| assert_eq!(tree.borrow().text.as_deref(), Some("80,70")));
    assert_eq!(events(), ["sample", "sample", "move"]);
    routing::finish_all();
}

#[test]
fn ending_a_drag_discards_queued_frame_work() {
    reset();
    chart();
    routing::queue_frame(&Element::new(), 10.0, 20.0);
    routing::finish_all();
    routing::drain_frame();
    TREE.with(|tree| assert!(tree.borrow().text.is_none()));
    assert_eq!(events(), ["sample", "finish"]);
}

fn sampled_paint(root: &mut Element, x: f32, y: f32) -> bool {
    root.text = Some(format!("paint:{x},{y}"));
    PENDING.with(|pending| pending.set(Some([x, y])));
    true
}

#[test]
fn paint_sampling_keeps_reactive_drain_outside_tree_borrows() {
    reset();
    routing::register(
        "external-chart",
        DragHandlers::new(moved, finished).paint_sampling(sampled_paint, drained),
    );
    let movement = TREE.with(|tree| {
        let mut borrowed = tree.borrow_mut();
        assert!(routing::sample_paint(&mut borrowed, 12.0, 18.0));
        assert_eq!(borrowed.text.as_deref(), Some("paint:12,18"));
        assert!(events().is_empty());
        routing::prepare_move(Some(&borrowed), 12.0, 18.0)
    });
    routing::drain_frame();
    TREE.with(|tree| assert_eq!(tree.borrow().text.as_deref(), Some("12,18")));
    routing::unregister("external-chart");
    assert!(!movement.dispatch());
    assert!(!routing::prepare_move(None, 0.0, 0.0).dispatch());
    reset();
}

#[test]
fn native_surfaces_yield_only_to_opted_in_drag_sessions() {
    reset();
    routing::register("external-chart", DragHandlers::new(moved, finished));
    assert!(!routing::blocks_native_input());
    routing::register("external-chart", DragHandlers::new(moved, finished).block_native_input());
    assert!(routing::blocks_native_input());
    routing::finish_all();
    assert!(!routing::blocks_native_input());
    reset();
}
