//! Solid-style [`Resource`] state + Suspense tracking (TLS).
//!
//! Fetch scheduling lives in [`crate::app::resource`] (needs the UI-thread bridge).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::reactivity::Signal;

/// Lifecycle of a [`Resource`] value.
#[derive(Clone, Debug, PartialEq)]
pub enum ResourceState<T> {
    /// In-flight (or not yet resolved). `previous` is stale-while-revalidate data.
    Loading {
        previous: Option<T>,
    },
    Ready(T),
    Err(String),
}

impl<T> ResourceState<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(v) => Some(v),
            _ => None,
        }
    }

    pub fn latest(&self) -> Option<&T> {
        match self {
            Self::Ready(v) => Some(v),
            Self::Loading { previous } => previous.as_ref(),
            Self::Err(_) => None,
        }
    }
}

/// Reactive async value (Solid `createResource` handle).
pub struct Resource<T> {
    pub(crate) state: Signal<ResourceState<T>>,
}

impl<T> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Resource<T> {}

impl<T: Clone + PartialEq + 'static> Resource<T> {
    /// Wrap an existing state signal (tests / sync demos; prefer [`crate::app::resource::create_resource`] for fetch).
    pub fn from_signal(state: Signal<ResourceState<T>>) -> Self {
        Self { state }
    }

    pub fn state(&self) -> ResourceState<T> {
        self.state.get()
    }

    pub fn loading(&self) -> bool {
        self.state.get().is_loading()
    }

    /// Latest successful value without Suspense tracking (stale OK while loading).
    pub fn latest(&self) -> Option<T> {
        self.state.get().latest().cloned()
    }

    /// Solid-style read: tracks the current effect and marks an enclosing Suspense pending
    /// while [`ResourceState::Loading`] with no previous value (or always while loading when
    /// `suspend_while_loading` is the default — we suspend whenever loading so fallback shows
    /// on refetch too unless `previous` is preferred via [`Self::latest`]).
    ///
    /// Returns `Ready` data, or `previous` while loading (still marks Suspense so fallback can
    /// choose). Callers that want fallback-only-on-first-load should check [`Self::loading`]
    /// separately.
    pub fn get(&self) -> Option<T> {
        let s = self.state.get();
        if s.is_loading() {
            mark_suspense_pending();
        }
        s.latest().cloned()
    }

    /// Force-set the ready value (Solid `mutate`).
    pub fn mutate(&self, value: T) {
        self.state.set(ResourceState::Ready(value));
    }

    pub fn error(&self) -> Option<String> {
        match self.state.get() {
            ResourceState::Err(e) => Some(e),
            _ => None,
        }
    }

    /// Test / demo helper: force [`ResourceState::Err`] without a fetcher.
    pub fn set_error(&self, message: impl Into<String>) {
        self.state.set(ResourceState::Err(message.into()));
    }
}

pub(crate) struct SuspenseFrame {
    pending: Cell<bool>,
}

thread_local! {
    static SUSPENSE_STACK: RefCell<Vec<Rc<SuspenseFrame>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn mark_suspense_pending() {
    SUSPENSE_STACK.with(|stack| {
        if let Some(frame) = stack.borrow().last() {
            frame.pending.set(true);
        }
    });
}

pub(crate) fn push_suspense_frame() -> Rc<SuspenseFrame> {
    let frame = Rc::new(SuspenseFrame {
        pending: Cell::new(false),
    });
    SUSPENSE_STACK.with(|stack| stack.borrow_mut().push(Rc::clone(&frame)));
    frame
}

pub(crate) fn pop_suspense_frame() {
    SUSPENSE_STACK.with(|stack| {
        let _ = stack.borrow_mut().pop();
    });
}

pub(crate) fn suspense_frame_is_pending(frame: &SuspenseFrame) -> bool {
    frame.pending.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, create_signal};

    #[test]
    fn resource_state_latest_prefers_ready() {
        let s = ResourceState::Loading {
            previous: Some(1_i32),
        };
        assert_eq!(s.latest(), Some(&1));
        assert_eq!(ResourceState::Ready(2).latest(), Some(&2));
    }

    #[test]
    fn suspense_stack_marks_pending() {
        let (_o, _) = create_root(|| {
            let frame = push_suspense_frame();
            assert!(!suspense_frame_is_pending(&frame));
            mark_suspense_pending();
            assert!(suspense_frame_is_pending(&frame));
            pop_suspense_frame();
        });
    }

    #[test]
    fn resource_get_marks_suspense_when_loading() {
        let (_o, _) = create_root(|| {
            let state = create_signal(ResourceState::<i32>::Loading { previous: None });
            let r = Resource { state };
            let frame = push_suspense_frame();
            assert!(r.get().is_none());
            assert!(suspense_frame_is_pending(&frame));
            pop_suspense_frame();
        });
    }
}
