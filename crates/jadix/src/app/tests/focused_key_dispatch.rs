use super::*;
use crate::core::reactivity::use_signal;
use crate::primitives::interactive::interactive;
use std::cell::RefCell;
use std::rc::Rc;

/// Regression for `RefCell already borrowed`: extract handlers under an outer borrow,
/// release it, then run — handlers may `signal.set` / nest `borrow_mut`.
#[test]
fn key_handlers_run_after_outer_borrow_released() {
    let state = Rc::new(RefCell::new(0u32));
    let state_cb = state.clone();
    let open = use_signal("tree_hit_key_reentry_open", || false);
    let root: Element = interactive(Element::new())
        .focus_id("k")
        .focusable(true)
        .on_click(move || {
            *state_cb.borrow_mut() += 1;
            open.set(true);
        })
        .into();

    // Outer immutable borrow (like input_controller before the fix).
    let outer = state.borrow();
    let handlers = focused_key_handlers(&root, "k").expect("handlers");
    assert!(handlers.focusable);
    let click = handlers.on_click.clone().expect("click");
    drop(outer); // must release before running
    click.borrow_mut()();
    assert_eq!(*state.borrow(), 1);
    assert!(open.peek());
}
