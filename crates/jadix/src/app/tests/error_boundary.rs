use super::*;
use crate::core::reactivity::{create_effect, create_root, create_signal, is_tracking};
use crate::core::tree::Element;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn throw_error_without_boundary_is_noop() {
    reset_orphan_throw_warn();
    throw_error("orphan");
    throw_error("orphan-again");
    assert_eq!(
        orphan_throw_warn_count(),
        1,
        "debug orphan throw must warn once"
    );
}

#[test]
fn throw_error_inside_frame_does_not_count_as_orphan() {
    reset_orphan_throw_warn();
    let frame = push_error_frame();
    throw_error("inside");
    pop_error_frame();
    assert_eq!(frame.error.borrow().as_deref(), Some("inside"));
    assert_eq!(orphan_throw_warn_count(), 0);
}

#[test]
fn inner_boundary_eats_throw() {
    reset_orphan_throw_warn();
    let outer = push_error_frame();
    let inner = push_error_frame();
    throw_error("inner-only");
    pop_error_frame();
    assert_eq!(inner.error.borrow().as_deref(), Some("inner-only"));
    assert!(
        outer.error.borrow().is_none(),
        "outer must not see an inner throw"
    );
    pop_error_frame();
    assert_eq!(orphan_throw_warn_count(), 0);
}

#[test]
fn panic_in_frame_still_pops() {
    reset_orphan_throw_warn();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _frame = push_error_frame();
        let _guard = ErrorFrameGuard;
        panic!("user-render");
    }));
    assert!(panicked.is_err());
    throw_error("after-panic");
    assert_eq!(
        orphan_throw_warn_count(),
        1,
        "leaked frame would swallow the orphan throw"
    );
}

#[test]
fn error_boundary_fallback_when_child_throws() {
    // Drive the boundary render path without a full AppState: call the same
    // push/pop + children logic the reactive_view body uses.
    let frame = push_error_frame();
    let _child = {
        throw_error("boom");
        Element::new().text("ok")
    };
    let err = frame.error.borrow().clone();
    pop_error_frame();
    assert_eq!(err.as_deref(), Some("boom"));
    let fallback = err
        .map(|m| Element::new().text(format!("err:{m}")))
        .unwrap();
    assert_eq!(fallback.text.as_deref(), Some("err:boom"));
}

#[test]
fn error_boundary_with_error_signal() {
    let (_owner, saw_fallback) = create_root(|| {
        let err_sig = create_signal(None::<String>);
        let saw = Rc::new(Cell::new(false));
        // Simulate boundary body reacting to an error signal set by a child.
        let saw2 = saw.clone();
        create_effect(move || {
            let frame = push_error_frame();
            if let Some(msg) = err_sig.get() {
                throw_error(msg);
            }
            let _child = Element::new().text("child");
            let err = frame.error.borrow().clone();
            pop_error_frame();
            if err.is_some() {
                saw2.set(true);
            }
        });
        assert!(!saw.get());
        err_sig.set(Some("failed".into()));
        assert!(saw.get(), "fallback path should run after error signal");
        saw
    });
    assert!(saw_fallback.get());
}

#[test]
fn error_boundary_api_builds_element() {
    let (_o, template_id) = create_root(|| {
        let out = Rc::new(RefCell::new(None::<String>));
        let out2 = out.clone();
        create_effect(move || {
            assert!(is_tracking());
            let el = error_boundary(
                "eb-test",
                |msg| Element::new().text(format!("fb:{msg}")),
                || {
                    throw_error("x");
                    Element::new().text("child")
                },
            );
            *out2.borrow_mut() = el.template_id.clone();
        });
        let tid = out.borrow().clone().expect("template_id");
        tid
    });
    assert_eq!(template_id, "eb-test");
}

#[test]
fn error_boundary_empty_id_is_not_blank_identity() {
    let (_o, template_id) = create_root(|| {
        let out = Rc::new(RefCell::new(None::<String>));
        let out2 = out.clone();
        create_effect(move || {
            let el = error_boundary("", |_| Element::new(), || Element::new());
            *out2.borrow_mut() = el.template_id.clone();
        });
        let tid = out.borrow().clone().expect("template_id");
        tid
    });
    assert_eq!(template_id, "ErrorBoundary");
}

#[test]
fn resource_err_to_error_boundary_via_throw_resource_error() {
    use crate::core::reactivity::create_signal;
    use crate::core::resource::{Resource, ResourceState};

    let (_owner, saw) = create_root(|| {
        let state = create_signal(ResourceState::Ready(1_i32));
        let resource = Resource::from_signal(state);
        let saw = Rc::new(Cell::new(false));
        let saw2 = saw.clone();
        create_effect(move || {
            let frame = push_error_frame();
            throw_resource_error(resource);
            let _child = Element::new().text(format!("{:?}", resource.get()));
            let err = frame.error.borrow().clone();
            pop_error_frame();
            saw2.set(err.as_deref() == Some("fetch failed"));
        });
        assert!(!saw.get(), "Ready must not throw");
        resource.set_error("fetch failed");
        assert!(
            saw.get(),
            "Err + throw_resource_error must trip enclosing boundary frame"
        );
        saw
    });
    assert!(saw.get());
}
