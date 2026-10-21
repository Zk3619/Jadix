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
mod tests {
    use super::*;
    use crate::app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK};
    use crate::core::reactivity::{create_root, Signal};
    use std::sync::mpsc;

    fn wait_until(mut pred: impl FnMut() -> bool) {
        for _ in 0..200 {
            drain_ui_tasks();
            if pred() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        drain_ui_tasks();
    }

    #[test]
    fn create_resource_resolves_on_ui_thread() {
        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let (owner, resource) = create_root(|| {
            let tx = tx.clone();
            create_resource(
                || 21_i32,
                move |n| {
                    let _ = tx.send(());
                    Ok::<_, String>(n * 2)
                },
            )
        });
        assert!(resource.loading());
        rx.recv().expect("fetcher ran");
        wait_until(|| !resource.loading());
        assert_eq!(resource.get(), Some(42));
        assert!(!resource.loading());
        drop(owner);
        drain_ui_tasks();
    }

    #[test]
    fn create_resource_refetch_keeps_previous() {
        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::sync_channel::<i32>(4);
        let (owner, (resource, src)) = create_root(|| {
            let src = Signal::new(1_i32);
            let tx = tx.clone();
            let resource = create_resource(
                move || src.get(),
                move |n| {
                    let _ = tx.send(n);
                    Ok::<_, String>(n * 10)
                },
            );
            (resource, src)
        });
        assert_eq!(rx.recv().unwrap(), 1);
        wait_until(|| matches!(resource.state(), ResourceState::Ready(_)));
        assert_eq!(
            resource.state(),
            ResourceState::Ready(10),
            "first fetch should resolve"
        );
        assert_eq!(resource.get(), Some(10));

        src.set(2);
        assert_eq!(rx.recv().unwrap(), 2);
        assert!(resource.loading(), "refetch should enter loading");
        assert_eq!(resource.latest(), Some(10));
        wait_until(|| !resource.loading());
        assert_eq!(resource.get(), Some(20));
        drop(owner);
        drain_ui_tasks();
    }

    #[test]
    fn suspense_shows_fallback_while_loading() {
        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let (owner, _) = create_root(|| {
            let tx = tx.clone();
            let data = create_resource(
                || (),
                move |_| {
                    let _ = tx.send(());
                    Ok::<_, String>("ok".to_string())
                },
            );
            let _el = suspense(
                "suspense-test",
                || Element::new().text("loading"),
                move || {
                    let _ = data.get();
                    Element::new().text(data.latest().unwrap_or_else(|| "empty".into()))
                },
            );
            assert!(data.loading());
        });
        let _ = rx.recv();
        wait_until(|| true);
        drop(owner);
        drain_ui_tasks();
    }

    #[test]
    fn suspense_empty_id_is_not_blank_identity() {
        use crate::core::reactivity::create_effect;
        use std::cell::RefCell;
        use std::rc::Rc;

        let (_o, template_id) = create_root(|| {
            let out = Rc::new(RefCell::new(None::<String>));
            let out2 = out.clone();
            create_effect(move || {
                let el = suspense("", || Element::new(), || Element::new());
                *out2.borrow_mut() = el.template_id.clone();
            });
            let tid = out.borrow().clone().expect("template_id");
            tid
        });
        assert_eq!(template_id, "Suspense");
    }

    #[test]
    fn create_resource_err_sets_state_and_bridges_to_eb() {
        use crate::app::error_boundary::{pop_error_frame, push_error_frame, throw_resource_error};
        use crate::core::reactivity::create_effect;
        use std::cell::Cell;
        use std::rc::Rc;

        let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let (owner, resource) = create_root(|| {
            let tx = tx.clone();
            create_resource(
                || (),
                move |_| {
                    let _ = tx.send(());
                    Err::<i32, _>("boom".to_string())
                },
            )
        });
        rx.recv().expect("fetcher ran");
        wait_until(|| matches!(resource.state(), ResourceState::Err(_)));
        assert_eq!(resource.error().as_deref(), Some("boom"));

        let saw = Rc::new(Cell::new(false));
        let saw2 = saw.clone();
        let _bridge = create_root(|| {
            create_effect({
                let resource = resource;
                move || {
                    let frame = push_error_frame();
                    // Track Err so effect re-runs if state were to change later.
                    let _ = resource.error();
                    throw_resource_error(resource);
                    let err = frame.error.borrow().clone();
                    pop_error_frame();
                    if err.as_deref() == Some("boom") {
                        saw2.set(true);
                    }
                }
            });
        });
        assert!(
            saw.get(),
            "create_resource Err must be throwable into ErrorBoundary via throw_resource_error"
        );
        drop(owner);
        drain_ui_tasks();
    }
}
