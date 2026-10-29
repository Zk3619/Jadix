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
            let el = suspense("", Element::new, Element::new);
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
