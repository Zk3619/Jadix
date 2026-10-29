use super::*;
use crate::{
    app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK},
    core::reactivity::{create_root, current_owner, signal_key_scope, Signal},
};
use std::{
    sync::atomic::AtomicUsize,
    task::{Context, Poll, Wake, Waker},
};

#[cfg(feature = "runtime-tokio")]
#[test]
fn tokio_completion_returns_to_its_owner_and_disposal_cancels_pending_work() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    while drain_ui_tasks() != 0 {}
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 2).unwrap();
    let (done_send, done_receive) = tokio::sync::oneshot::channel();
    let (owner, (scope, value)) = create_root(|| {
        let scope = TaskScope::new();
        let value = Signal::new(0);
        let expected_owner = current_owner();
        scope
            .spawn(
                &executor,
                move |_| async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    done_send.send(()).unwrap();
                    Ok(42)
                },
                move |result| {
                    assert_eq!(current_owner(), expected_owner);
                    value.set(result.unwrap());
                },
            )
            .unwrap();
        (scope, value)
    });
    runtime.block_on(done_receive).unwrap();
    assert_eq!(value.peek(), 0);
    drain_ui_tasks();
    assert_eq!(value.peek(), 42);
    let pending = scope
        .spawn(
            &executor,
            |_| std::future::pending::<Result<(), TaskError>>(),
            |_| panic!("disposed task must not update the UI"),
        )
        .unwrap();
    owner.dispose();
    assert!(pending.cancellation().is_cancelled());
    runtime.block_on(async { tokio::task::yield_now().await });
    drain_ui_tasks();
    assert!(scope.is_closed());
}

#[derive(Default)]
struct QueueExecutor {
    work: Mutex<VecDeque<BlockingTask>>,
    aborts: Arc<AtomicUsize>,
}

impl BlockingExecutor for QueueExecutor {
    fn spawn_blocking(&self, task: BlockingTask) -> Result<TaskHandle, TaskError> {
        self.work.lock().unwrap().push_back(task);
        let aborts = self.aborts.clone();
        Ok(TaskHandle::with_abort(
            CancellationToken::new(),
            move || {
                aborts.fetch_add(1, Ordering::SeqCst);
            },
        ))
    }
}

impl QueueExecutor {
    fn run_one(&self) {
        let work = self.work.lock().unwrap().pop_front().unwrap();
        std::thread::spawn(work).join().unwrap();
    }
}

#[test]
fn blocking_completion_restores_window_and_owner_and_can_create_ui_work() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    while drain_ui_tasks() != 0 {}
    let executor = QueueExecutor::default();
    let observed = Arc::new(Mutex::new(None));
    let (owner, scope) = create_root(|| {
        let _window = window_scope::enter(window_scope::WindowScope::Secondary(789));
        let scope = TaskScope::new();
        let expected_owner = current_owner();
        let observed = observed.clone();
        scope
            .spawn_blocking_on(
                &executor,
                |_| Ok(42),
                move |result| {
                    assert_eq!(current_owner(), expected_owner);
                    assert_eq!(signal_key_scope(), 789);
                    *observed.lock().unwrap() = Some(Signal::new(result.unwrap()));
                },
            )
            .unwrap();
        scope
    });
    executor.run_one();
    assert!(observed.lock().unwrap().is_none());
    drain_ui_tasks();
    let signal = observed.lock().unwrap().unwrap();
    assert_eq!(signal.get(), 42);
    assert!(scope.state.jobs.lock().unwrap().is_empty());
    drop(owner);
    assert!(!signal.is_alive());
    assert!(scope.is_closed());
}

#[test]
fn cancellation_fences_queued_completion_and_owner_close_aborts_unfinished_work() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let executor = QueueExecutor::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let (owner, scope) = create_root(TaskScope::new);
    let observed = calls.clone();
    let handle = scope
        .spawn_blocking_on(
            &executor,
            |_| Ok(()),
            move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
    executor.run_one();
    handle.cancel();
    drain_ui_tasks();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    scope
        .spawn_blocking_on(&executor, |_| Ok(()), |_| panic!("disposed completion"))
        .unwrap();
    drop(owner);
    assert_eq!(executor.aborts.load(Ordering::SeqCst), 2);
    executor.run_one();
    drain_ui_tasks();
    assert!(scope.spawn_blocking(|_| Ok(()), |_| {}).is_err());
}

#[test]
fn rejection_and_abandoned_jobs_do_not_leave_pending_entries() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_owner, scope) = create_root(TaskScope::new);
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let reject = |_: BlockingTask| Err(TaskError::new(TaskErrorKind::Rejected, "queue full"));
    assert!(scope
        .spawn_blocking_on(
            &reject,
            |_| Ok(()),
            move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
            }
        )
        .is_err());
    drain_ui_tasks();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(scope.state.jobs.lock().unwrap().is_empty());
    let dropped = |_: BlockingTask| Ok(TaskHandle::default());
    let error_kind = Arc::new(Mutex::new(None));
    let observed = error_kind.clone();
    scope
        .spawn_blocking_on(
            &dropped,
            |_| Ok(()),
            move |result| {
                *observed.lock().unwrap() = Some(result.unwrap_err().kind);
            },
        )
        .unwrap();
    drain_ui_tasks();
    assert_eq!(*error_kind.lock().unwrap(), Some(TaskErrorKind::Abandoned));
}

#[test]
fn blocking_panics_become_errors_instead_of_leaving_loading_forever() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let executor = QueueExecutor::default();
    let result = Arc::new(Mutex::new(None));
    let observed = result.clone();
    let (_owner, scope) = create_root(TaskScope::new);
    scope
        .spawn_blocking_on::<(), _, _>(
            &executor,
            |_| panic!("fixture"),
            move |error| {
                *observed.lock().unwrap() = Some(error.unwrap_err().kind);
            },
        )
        .unwrap();
    executor.run_one();
    drain_ui_tasks();
    assert_eq!(*result.lock().unwrap(), Some(TaskErrorKind::Panicked));
}

struct NoopWake;
impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

#[test]
fn future_factory_is_deferred_and_external_spawner_owns_polling() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let futures = Arc::new(Mutex::new(VecDeque::<BoxTask>::new()));
    let queued = futures.clone();
    let spawner = move |task| {
        queued.lock().unwrap().push_back(task);
        Ok(TaskHandle::default())
    };
    let called = Arc::new(AtomicBool::new(false));
    let started = called.clone();
    let value = Arc::new(Mutex::new(None));
    let observed = value.clone();
    let (_owner, scope) = create_root(TaskScope::new);
    scope
        .spawn(
            &spawner,
            move |_| {
                started.store(true, Ordering::Release);
                async { Ok(42) }
            },
            move |result| {
                *observed.lock().unwrap() = Some(result.unwrap());
            },
        )
        .unwrap();
    assert!(!called.load(Ordering::Acquire));
    let mut future = futures.lock().unwrap().pop_front().unwrap();
    std::thread::spawn(move || {
        let waker = Waker::from(Arc::new(NoopWake));
        assert_eq!(
            future.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Ready(())
        );
    })
    .join()
    .unwrap();
    assert!(value.lock().unwrap().is_none());
    drain_ui_tasks();
    assert_eq!(*value.lock().unwrap(), Some(42));
}

#[test]
fn async_resource_reports_runtime_abandonment_and_admission_rejection() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let drop_future: Arc<dyn FutureSpawner> = Arc::new(|_: BoxTask| Ok(TaskHandle::default()));
    let (_owner, resource) = create_root(|| {
        crate::app::create_async_resource(|| (), drop_future, |_, _| async { Ok::<_, String>(42) })
    });
    assert!(resource.loading());
    drain_ui_tasks();
    assert!(resource.error().unwrap().contains("unfinished"));
    let rejected: Arc<dyn FutureSpawner> =
        Arc::new(|_: BoxTask| Err(TaskError::new(TaskErrorKind::Rejected, "runtime closed")));
    let (_owner, resource) = create_root(|| {
        crate::app::create_async_resource(|| (), rejected, |_, _| async { Ok::<_, String>(42) })
    });
    assert_eq!(resource.error().as_deref(), Some("runtime closed"));
    drain_ui_tasks();
    assert_eq!(resource.error().as_deref(), Some("runtime closed"));
}

#[test]
fn subscriptions_apply_bounded_fifo_batches_and_report_backpressure() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    while drain_ui_tasks() != 0 {}
    let sink = Arc::new(Mutex::new(None));
    let output = sink.clone();
    let received = Arc::new(Mutex::new(Vec::new()));
    let observed = received.clone();
    let (_owner, scope) = create_root(TaskScope::new);
    let handle = scope
        .subscribe(
            100,
            move |events, cancel| {
                *output.lock().unwrap() = Some(events);
                Ok(TaskHandle::cooperative(cancel))
            },
            move |event| observed.lock().unwrap().push(event),
        )
        .unwrap();
    let sink = sink.lock().unwrap().take().unwrap();
    for value in 0..100 {
        sink.try_send(value).unwrap();
    }
    assert_eq!(sink.try_send(100), Err(EventSendError::Full(100)));
    assert_eq!(drain_ui_tasks(), 1);
    assert_eq!(received.lock().unwrap().len(), 64);
    assert_eq!(drain_ui_tasks(), 1);
    assert_eq!(*received.lock().unwrap(), (0..100).collect::<Vec<_>>());
    sink.try_send(101).unwrap();
    handle.cancel();
    drain_ui_tasks();
    assert_eq!(received.lock().unwrap().len(), 100);
    assert_eq!(sink.try_send(102), Err(EventSendError::Closed(102)));
    assert!(scope.state.jobs.lock().unwrap().is_empty());
}

#[test]
fn a_subscription_can_cancel_itself_without_a_callback_mutex_deadlock() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let sink = Arc::new(Mutex::new(None));
    let output = sink.clone();
    let token = Arc::new(Mutex::new(None::<CancellationToken>));
    let cancellation = token.clone();
    let received = Arc::new(AtomicUsize::new(0));
    let observed = received.clone();
    let (_owner, scope) = create_root(TaskScope::new);
    scope
        .subscribe(
            4,
            move |events: EventSink<()>, cancel| {
                *output.lock().unwrap() = Some(events);
                *token.lock().unwrap() = Some(cancel.clone());
                Ok(TaskHandle::cooperative(cancel))
            },
            move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                cancellation.lock().unwrap().as_ref().unwrap().cancel();
            },
        )
        .unwrap();
    let sink = sink.lock().unwrap().take().unwrap();
    sink.try_send(()).unwrap();
    sink.try_send(()).unwrap();
    drain_ui_tasks();
    assert_eq!(received.load(Ordering::SeqCst), 1);
}

#[test]
fn releasing_the_last_source_sink_removes_its_scope_registration() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let output = Arc::new(Mutex::new(None::<EventSink<()>>));
    let sink = output.clone();
    let (_owner, scope) = create_root(TaskScope::new);
    scope
        .subscribe(
            4,
            move |events, cancel| {
                *sink.lock().unwrap() = Some(events);
                Ok(TaskHandle::cooperative(cancel))
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(scope.state.jobs.lock().unwrap().len(), 1);
    output.lock().unwrap().take();
    assert!(scope.state.jobs.lock().unwrap().is_empty());
}

#[test]
fn resource_refetch_cancels_old_work_and_keeps_the_latest_generation() {
    let _queue = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let executor = Arc::new(QueueExecutor::default());
    let (owner, (source, resource)) = create_root(|| {
        let source = Signal::new(1);
        let resource = crate::app::create_resource_on(
            move || source.get(),
            executor.clone(),
            |value, cancel| {
                cancel.checkpoint()?;
                Ok::<_, TaskError>(value * 10)
            },
        );
        (source, resource)
    });
    source.set(2);
    assert_eq!(executor.aborts.load(Ordering::SeqCst), 1);
    executor.run_one();
    drain_ui_tasks();
    assert!(resource.loading());
    executor.run_one();
    drain_ui_tasks();
    assert_eq!(resource.get(), Some(20));
    source.set(3);
    assert_eq!(resource.latest(), Some(20));
    drop(owner);
    executor.run_one();
    drain_ui_tasks();
}
