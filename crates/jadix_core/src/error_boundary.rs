//! Owner-scoped error recovery, shared by renderers and native event adapters.
use std::{any::Any, cell::{Cell, RefCell}, rc::Rc};
use crate::reactivity::{self::*, Context};

thread_local! {
    static ERROR_REVISION: Cell<u64> = const { Cell::new(0) };
    static ERROR_CONTEXT: Context<Rc<ErrorState>> = create_context();
}

struct ErrorState { error: Signal<Option<String>> }

/// A boundary keeps its status outside the disposable content scope. Resetting
/// removes failed effects, timers and resources before constructing new content.
#[derive(Clone)]
pub struct ErrorBoundary(Rc<Boundary>);
struct Boundary {
    parent: OwnerId,
    state: Rc<ErrorState>,
    generation: Signal<u64>,
    content: RefCell<Option<OwnerHandle>>,
}

impl ErrorBoundary {
    pub fn new() -> Self {
        let error = create_signal(None);
        Self(Rc::new(Boundary {
            parent: current_owner().expect("error boundary requires an owner"),
            state: Rc::new(ErrorState { error }),
            generation: create_signal(0), content: RefCell::new(None),
        }))
    }
    pub fn error(&self) -> Option<String> { try_with_owner(self.0.parent, || self.0.state.error.get()).flatten() }
    pub fn generation(&self) -> u64 { try_with_owner(self.0.parent, || self.0.generation.get()).unwrap_or(0) }
    pub fn reset(&self) {
        let old = self.0.content.borrow_mut().take();
        drop(old);
        let _ = try_with_owner(self.0.parent, || batch(|| {
            self.0.state.error.set(None);
            self.0.generation.update(|value| *value = value.wrapping_add(1));
        }));
    }
    /// Build content, or return `None` after the first reported error. Call this
    /// from a renderer's reactive region; the fallback runs outside this scope.
    pub fn run<R>(&self, body: impl FnOnce() -> R) -> Option<R> {
        try_with_owner(self.0.parent, || {
            let old = self.0.content.borrow_mut().take();
            drop(old);
            if self.0.state.error.peek().is_some() { return None; }
            let (scope, result) = create_scope(|| {
                ERROR_CONTEXT.with(|context| provide_context(*context, self.0.state.clone()));
                run_owned_callback(current_owner().unwrap(), body)
            });
            self.0.content.replace(Some(scope));
            if self.0.state.error.peek().is_some() { None } else { result }
        }).flatten()
    }
}
impl Default for ErrorBoundary { fn default() -> Self { Self::new() } }

fn state() -> Option<Rc<ErrorState>> { ERROR_CONTEXT.with(|context| use_context(*context)) }

/// Report an expected resource/application error to the nearest live boundary.
/// Returns false when no boundary exists; callers retain their own error policy.
pub fn throw_error(error: impl Into<String>) -> bool {
    let Some(state) = state() else { return false; };
    if state.error.peek().is_none() {
        let message = error.into();
        let end = message.char_indices().map(|(index, _)| index).take_while(|index| *index <= 16_384).last().unwrap_or(0);
        let message = if message.len() > 16_384 { message[..end].to_owned() } else { message };
        // Projection adapters discard any partial snapshot when this changes.
        ERROR_REVISION.with(|revision| revision.set(revision.get().wrapping_add(1)));
        batch(|| state.error.set(Some(message)));
    }
    true
}

pub(crate) fn owner_failed(owner: OwnerId) -> bool {
    try_with_owner(owner, || state().is_some_and(|state| state.error.peek().is_some())).unwrap_or(true)
}

pub(crate) fn report_panic(owner: OwnerId, payload: Box<dyn Any + Send>) {
    let message = payload.downcast_ref::<String>().cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| (*text).to_owned()))
        .unwrap_or_else(|| "application callback panicked".into());
    if !try_with_owner(owner, || throw_error(message)).unwrap_or(false) {
        std::panic::resume_unwind(payload);
    }
}

/// Execute an event/timer under its source owner. Disposed or failed content is
/// inert. Panics reach its boundary; without one, ordinary Rust unwinding applies.
pub fn run_owned_callback<R>(owner: OwnerId, callback: impl FnOnce() -> R) -> Option<R> {
    try_with_owner(owner, || {
        if owner_failed(owner) { return None; }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(callback)) {
            Ok(value) => Some(value),
            Err(payload) => { report_panic(owner, payload); None }
        }
    }).flatten()
}

/// Changes when a boundary first fails. Projection adapters use this to discard
/// a partially refreshed tree and resolve the fallback in the same transaction.
#[doc(hidden)]
pub fn error_revision() -> u64 { ERROR_REVISION.with(Cell::get) }
