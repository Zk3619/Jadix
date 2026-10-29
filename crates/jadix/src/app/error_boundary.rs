//! Solid-style [`error_boundary`] MVP — catch child-reported errors and render fallback.

#[cfg(any(debug_assertions, test))]
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::tree::Element;

use super::reactive::reactive_view;

pub(crate) struct ErrorFrame {
    pub(crate) error: RefCell<Option<String>>,
}

thread_local! {
    static ERROR_STACK: RefCell<Vec<Rc<ErrorFrame>>> = const { RefCell::new(Vec::new()) };
    #[cfg(any(debug_assertions, test))]
    static ORPHAN_THROW_WARNED: Cell<bool> = const { Cell::new(false) };
    #[cfg(test)]
    static ORPHAN_THROW_WARN_COUNT: Cell<u32> = const { Cell::new(0) };
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

/// Pops the TLS frame even if `children()` panics (does **not** catch the panic).
struct ErrorFrameGuard;

impl Drop for ErrorFrameGuard {
    fn drop(&mut self) {
        pop_error_frame();
    }
}

/// Report an error to the nearest enclosing [`error_boundary`] (no-op if none).
///
/// Debug builds log a single warning the first time this is called with no boundary
/// when the owning error boundary has already been removed.
pub fn throw_error(message: impl Into<String>) {
    let message = message.into();
    ERROR_STACK.with(|stack| {
        if let Some(frame) = stack.borrow().last() {
            *frame.error.borrow_mut() = Some(message);
        } else {
            #[cfg(any(debug_assertions, test))]
            warn_orphan_throw(&message);
        }
    });
}

#[cfg(any(debug_assertions, test))]
fn warn_orphan_throw(message: &str) {
    let first = ORPHAN_THROW_WARNED.with(|w| {
        if w.get() {
            false
        } else {
            w.set(true);
            true
        }
    });
    if !first {
        return;
    }
    #[cfg(test)]
    ORPHAN_THROW_WARN_COUNT.with(|c| c.set(c.get().saturating_add(1)));
    log::warn!("throw_error without error_boundary (orphan): {message}");
}

#[cfg(test)]
pub(crate) fn reset_orphan_throw_warn() {
    ORPHAN_THROW_WARNED.with(|w| w.set(false));
    ORPHAN_THROW_WARN_COUNT.with(|c| c.set(0));
}

#[cfg(test)]
pub(crate) fn orphan_throw_warn_count() -> u32 {
    ORPHAN_THROW_WARN_COUNT.with(|c| c.get())
}

/// WP-2.5 contract: if `resource` is [`ResourceState::Err`](crate::core::resource::ResourceState::Err),
/// [`throw_error`] so an enclosing [`error_boundary`] can render fallback.
///
/// Consolidation policy: **explicit** bridge (call this inside boundary children). Automatic
/// Resource reads do not automatically dispatch errors to an error boundary.
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
pub fn error_boundary<F, C>(anchor_id: impl Into<String>, fallback: F, children: C) -> Element
where
    F: Fn(String) -> Element + 'static,
    C: Fn() -> Element + 'static,
{
    let id = {
        let s = anchor_id.into();
        if s.trim().is_empty() {
            String::from("ErrorBoundary")
        } else {
            s
        }
    };
    reactive_view(id, move || {
        let frame = push_error_frame();
        let _guard = ErrorFrameGuard;
        let child = children();
        let err = frame.error.borrow().clone();
        match err {
            Some(msg) => fallback(msg),
            None => child,
        }
    })
}

#[cfg(test)]
#[path = "tests/error_boundary.rs"]
mod tests;
