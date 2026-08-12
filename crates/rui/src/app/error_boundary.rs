//! Solid-style [`error_boundary`] MVP — catch child-reported errors and render fallback.

use std::cell::RefCell;
use std::rc::Rc;

use crate::core::tree::Element;

use super::reactive::reactive_view;

pub(crate) struct ErrorFrame {
    pub(crate) error: RefCell<Option<String>>,
}

thread_local! {
    static ERROR_STACK: RefCell<Vec<Rc<ErrorFrame>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn push_error_frame() -> Rc<ErrorFrame> {
    let frame = Rc::new(ErrorFrame {
        error: RefCell::new(None),
    });
    ERROR_STACK.with(|stack| stack.borrow_mut().push(Rc::clone(&frame)));
    frame
}

pub(crate) fn pop_error_frame() {
    ERROR_STACK.with(|stack| {
        let _ = stack.borrow_mut().pop();
    });
}

/// Report an error to the nearest enclosing [`error_boundary`] (no-op if none).
pub fn throw_error(message: impl Into<String>) {
    let message = message.into();
    ERROR_STACK.with(|stack| {
        if let Some(frame) = stack.borrow().last() {
            *frame.error.borrow_mut() = Some(message);
        }
    });
}

/// WP-2.5 contract: if `resource` is [`ResourceState::Err`](crate::core::resource::ResourceState::Err),
/// [`throw_error`] so an enclosing [`error_boundary`] can render fallback.
///
/// Consolidation policy: **explicit** bridge (call this inside boundary children). Automatic
/// Err→EB on every Resource read is deferred (see `docs/REACTIVITY.md` / R-WP-4).
pub fn throw_resource_error<T: Clone + PartialEq + 'static>(
    resource: crate::core::resource::Resource<T>,
) {
    if let Some(msg) = resource.error() {
        throw_error(msg);
    }
}

/// Solid-style `<ErrorBoundary>`: run `children`; if they [`throw_error`] (or set the frame),
/// render `fallback(message)` instead.
///
/// Both arms run inside a [`reactive_view`] so signal reads stay local to the boundary.
pub fn error_boundary<F, C>(
    anchor_id: impl Into<String>,
    fallback: F,
    children: C,
) -> Element
where
    F: Fn(String) -> Element + 'static,
    C: Fn() -> Element + 'static,
{
    let id = anchor_id.into();
    reactive_view(id, move || {
        let frame = push_error_frame();
        let child = children();
        let err = frame.error.borrow().clone();
        pop_error_frame();
        match err {
            Some(msg) => fallback(msg),
            None => child,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_effect, create_root, create_signal, is_tracking};
    use crate::core::tree::Element;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn throw_error_without_boundary_is_noop() {
        throw_error("orphan");
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
    fn resource_err_to_error_boundary_via_throw_resource_error() {
        use crate::core::resource::{Resource, ResourceState};
        use crate::core::reactivity::create_signal;

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
}
