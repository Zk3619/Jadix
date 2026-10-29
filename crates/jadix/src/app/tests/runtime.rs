use super::*;

#[test]
fn request_redraw_while_state_mutably_borrowed_does_not_panic() {
    call_while_app_state_mutably_borrowed(|| {
        request_redraw();
        request_redraw_all();
    });
}

#[test]
fn secondary_updates_defer_while_window_tree_is_mutably_borrowed() {
    let state = std::rc::Rc::new(RefCell::new(None));
    let secondaries = std::rc::Rc::new(RefCell::new(HashMap::new()));
    let runtime = AppRuntime::new(
        std::rc::Rc::downgrade(&state),
        std::rc::Rc::downgrade(&secondaries),
    );
    let _scope = window_scope::enter_secondary_id(WindowId::from_raw(987654));
    let _borrow = secondaries.borrow_mut();

    assert!(runtime
        .try_reconcile_subtree("tooltip", crate::primitives::r#box().into())
        .is_none());
    assert!(runtime
        .try_update_element_by_template_id("tooltip", |_| panic!("tree remains borrowed"))
        .is_none());
}
