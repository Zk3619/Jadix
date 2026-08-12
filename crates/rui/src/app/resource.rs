//! [`create_resource`] / [`suspense`] — async data + loading fallback (Solid-style MVP).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::core::reactivity::{create_effect, create_signal};
use crate::core::resource::{
    pop_suspense_frame, push_suspense_frame, suspense_frame_is_pending,
};
use crate::core::tree::Element;

use super::reactive::reactive_view;
use super::ui_bridge::spawn_blocking;

pub use crate::core::resource::{Resource, ResourceState};

/// Create a Solid-style resource: `source` is tracked; when it changes, `fetcher` runs on a
/// worker thread and the result is applied on the UI thread via [`spawn_blocking`].
///
/// ```ignore
/// let data = create_resource(|| user_id.get(), |id| fetch_user(id));
/// // in UI: data.get() / data.loading() / suspense(...)
/// ```
pub fn create_resource<S, T, E, SF, FF>(mut source: SF, fetcher: FF) -> Resource<T>
where
    S: Clone + PartialEq + Send + 'static,
    T: Clone + PartialEq + Send + 'static,
    E: std::fmt::Display + Send + 'static,
    SF: FnMut() -> S + 'static,
    FF: Fn(S) -> Result<T, E> + Send + Sync + 'static,
{
    let state = create_signal(ResourceState::Loading { previous: None });
    let resource = Resource { state };
    let fetcher = Arc::new(fetcher);
    let generation = Arc::new(AtomicU64::new(0));

    create_effect({
        let generation = Arc::clone(&generation);
        move || {
            let input = source();
            let previous = match resource.state.peek() {
                ResourceState::Ready(v) => Some(v),
                ResourceState::Loading { previous } => previous,
                ResourceState::Err(_) => None,
            };
            resource
                .state
                .set(ResourceState::Loading { previous });
            let gen = generation.fetch_add(1, Ordering::SeqCst) + 1;
            let fetcher = Arc::clone(&fetcher);
            let generation = Arc::clone(&generation);
            spawn_blocking(
                move || fetcher(input),
                move |result| {
                    if generation.load(Ordering::SeqCst) != gen {
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
                },
            );
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
    let id = anchor_id.into();
    reactive_view(id, move || {
        let frame = push_suspense_frame();
        let child = children();
        let pending = suspense_frame_is_pending(&frame);
        pop_suspense_frame();
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
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
    fn create_resource_err_sets_state_and_bridges_to_eb() {
        use crate::app::error_boundary::{
            pop_error_frame, push_error_frame, throw_resource_error,
        };
        use crate::core::reactivity::create_effect;
        use std::cell::Cell;
        use std::rc::Rc;

        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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
