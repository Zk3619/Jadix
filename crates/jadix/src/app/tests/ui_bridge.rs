use super::*;
use std::sync::{Arc, Mutex};

#[test]
fn queue_drains_without_proxy() {
    let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let flag = Arc::new(Mutex::new(false));
    let flag2 = Arc::clone(&flag);
    run_on_ui_thread(move || {
        *flag2.lock().unwrap() = true;
    });
    drain_ui_tasks();
    assert!(*flag.lock().unwrap());
}

#[test]
fn request_exit_without_proxy_does_not_panic() {
    request_exit();
}

#[test]
fn ui_tasks_are_bounded_fifo_and_nested_posts_wait_for_next_turn() {
    let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    while drain_ui_tasks() != 0 {}
    let seen = Arc::new(Mutex::new(Vec::new()));
    let total = UI_TASK_BATCH * 3 + 7;
    for i in 0..total {
        let seen = seen.clone();
        run_on_ui_thread(move || seen.lock().unwrap().push(i));
    }
    assert_eq!(drain_ui_tasks(), UI_TASK_BATCH);
    assert_eq!(seen.lock().unwrap().len(), UI_TASK_BATCH);
    while drain_ui_tasks() != 0 {}
    assert_eq!(*seen.lock().unwrap(), (0..total).collect::<Vec<_>>());

    let seen2 = seen.clone();
    run_on_ui_thread(move || {
        run_on_ui_thread(move || seen2.lock().unwrap().push(total));
    });
    assert_eq!(drain_ui_tasks(), 1);
    assert_eq!(seen.lock().unwrap().len(), total);
    assert_eq!(drain_ui_tasks(), 1);
    assert_eq!(seen.lock().unwrap().last(), Some(&total));
}

#[test]
fn cancelled_blocking_work_does_not_post_its_result_to_the_ui_thread() {
    let _guard = UI_QUEUE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    while drain_ui_tasks() != 0 {}

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let callback_ran = Arc::new(Mutex::new(false));
    let callback_ran2 = callback_ran.clone();

    let handle = spawn_blocking_cancellable(
        move |cancellation| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let cancelled = cancellation.is_cancelled();
            finished_tx.send(()).unwrap();
            cancelled
        },
        move |_| *callback_ran2.lock().unwrap() = true,
    );

    started_rx.recv().unwrap();
    assert!(handle.cancel());
    release_tx.send(()).unwrap();
    finished_rx.recv().unwrap();
    assert_eq!(drain_ui_tasks(), 0);
    assert!(!*callback_ran.lock().unwrap());
}
