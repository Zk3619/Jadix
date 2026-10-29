//! [`create_resource`] / [`suspense`] — async data + loading fallback (Solid-style MVP).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::core::reactivity::{create_effect, create_signal};
use crate::core::resource::{push_suspense_frame, suspense_frame_is_pending, SuspenseFrameGuard};
use crate::core::tree::Element;

use super::reactive::reactive_view;
use super::tasks::{
    BlockingExecutor, CancellationToken, FutureSpawner, TaskError, TaskErrorKind, TaskHandle,
    TaskScope,
};

pub use crate::core::resource::{Resource, ResourceState};

/// Create a Solid-style resource: `source` is tracked; when it changes, `fetcher` runs on a
/// worker thread and the result is applied on the UI thread via [`TaskScope`].
/// Refetch cancels the preceding handle; owner teardown fences all late results.
///
/// ```ignore
/// let data = create_resource(|| user_id.get(), |id| fetch_user(id));
/// // in UI: data.get() / data.loading() / suspense(...)
/// ```
pub fn create_resource<S, T, E, SF, FF>(source: SF, fetcher: FF) -> Resource<T>
where
    S: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
    E: std::fmt::Display + Send + 'static,
    SF: FnMut() -> S + 'static,
    FF: Fn(S) -> Result<T, E> + Send + Sync + 'static,
{
    let fetcher = Arc::new(fetcher);
    resource_with_loader(source, move |scope, input, on_ui| {
        let fetcher = fetcher.clone();
        scope.spawn_blocking(move |_| fetcher(input).map_err(resource_error), on_ui)
    })
}

/// Cancellable blocking resource on an explicitly supplied application pool.
pub fn create_resource_on<S, T, E, SF, FF>(
    source: SF,
    executor: Arc<dyn BlockingExecutor>,
    fetcher: FF,
) -> Resource<T>
where
    S: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
    E: std::fmt::Display + Send + 'static,
    SF: FnMut() -> S + 'static,
    FF: Fn(S, CancellationToken) -> Result<T, E> + Send + Sync + 'static,
{
    let fetcher = Arc::new(fetcher);
    resource_with_loader(source, move |scope, input, on_ui| {
        let fetcher = fetcher.clone();
        scope.spawn_blocking_on(
            executor.as_ref(),
            move |cancel| fetcher(input, cancel).map_err(resource_error),
            on_ui,
        )
    })
}

/// Runtime-neutral Future resource. Construct the Future inside the injected runtime.
pub fn create_async_resource<S, T, E, SF, FF, Work>(
    source: SF,
    spawner: Arc<dyn FutureSpawner>,
    fetcher: FF,
) -> Resource<T>
where
    S: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
    E: std::fmt::Display + Send + 'static,
    SF: FnMut() -> S + 'static,
    FF: Fn(S, CancellationToken) -> Work + Send + Sync + 'static,
    Work: std::future::Future<Output = Result<T, E>> + Send + 'static,
{
    let fetcher = Arc::new(fetcher);
    resource_with_loader(source, move |scope, input, on_ui| {
        let fetcher = fetcher.clone();
        scope.spawn(
            spawner.as_ref(),
            move |cancel| async move { fetcher(input, cancel).await.map_err(resource_error) },
            on_ui,
        )
    })
}

fn resource_error(error: impl std::fmt::Display) -> TaskError {
    TaskError::new(TaskErrorKind::Failed, error.to_string())
}

type ResourceCallback<T> = Box<dyn FnOnce(Result<T, TaskError>) + Send>;

fn resource_with_loader<S, T, Source, Start>(mut source: Source, start: Start) -> Resource<T>
where
    S: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
    Source: FnMut() -> S + 'static,
    Start: Fn(&TaskScope, S, ResourceCallback<T>) -> Result<TaskHandle, TaskError> + 'static,
{
    let state = create_signal(ResourceState::Loading { previous: None });
    let resource = Resource { state };
    let scope = TaskScope::new();
    let current = Rc::new(RefCell::new(None::<TaskHandle>));
    let generation = Arc::new(AtomicU64::new(0));

    create_effect({
        let generation = Arc::clone(&generation);
        move || {
            let input = source();
            let Ok(previous_generation) =
                generation.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                    value.checked_add(1)
                })
            else {
                resource
                    .state
                    .set(ResourceState::Err("resource generations exhausted".into()));
                return;
            };
            let request_generation = previous_generation + 1;
            let preceding = current.borrow_mut().take();
            if let Some(previous) = preceding {
                previous.cancel();
            }
            let previous = match resource.state.peek() {
                ResourceState::Ready(v) => Some(v),
                ResourceState::Loading { previous } => previous,
                ResourceState::Err(_) => None,
            };
            resource.state.set(ResourceState::Loading { previous });
            if generation.load(Ordering::Acquire) != request_generation || scope.is_closed() {
                return;
            }
            let observed_generation = Arc::clone(&generation);
            let result = start(
                &scope,
                input,
                Box::new(move |result| {
                    if observed_generation.load(Ordering::Acquire) != request_generation {
                        return;
                    }
                    match result {
                        Ok(v) => {
                            let _ = resource.state.try_set(ResourceState::Ready(v));
                        }
                        Err(e) => {
                            let _ = resource.state.try_set(ResourceState::Err(e.to_string()));
                        }
                    }
                }),
            );
            if generation.load(Ordering::Acquire) == request_generation {
                match result {
                    Ok(handle) => *current.borrow_mut() = Some(handle),
                    Err(error) => {
                        resource
                            .state
                            .try_set(ResourceState::Err(error.to_string()));
                    }
                }
            } else if let Ok(handle) = result {
                handle.cancel();
            }
        }
    });

    resource
}

/// Solid-style `<Suspense>`: while a nested [`Resource::get`] sees loading, render `fallback`.
///
/// `children` and `fallback` run inside a [`reactive_view`] so resource signal reads stay local.
pub fn suspense<F, C>(anchor_id: impl Into<String>, fallback: F, children: C) -> Element
where
    F: Fn() -> Element + 'static,
    C: Fn() -> Element + 'static,
{
    let id = {
        let s = anchor_id.into();
        if s.trim().is_empty() {
            String::from("Suspense")
        } else {
            s
        }
    };
    reactive_view(id, move || {
        let frame = push_suspense_frame();
        let _guard = SuspenseFrameGuard;
        let child = children();
        let pending = suspense_frame_is_pending(&frame);
        if pending {
            fallback()
        } else {
            child
        }
    })
}

#[cfg(test)]
#[path = "tests/resource.rs"]
mod tests;
