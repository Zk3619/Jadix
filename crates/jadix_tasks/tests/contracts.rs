use jadix_tasks::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};
use std::task::{Context, Poll, Wake, Waker};

struct PendingDrop(Arc<AtomicUsize>);

impl Future for PendingDrop {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Pending
    }
}

impl Drop for PendingDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn cancellation_is_idempotent_and_late_registration_is_not_lost() {
    let cancellation = CancellationToken::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let nested = cancellation.clone();
    let observed = calls.clone();
    cancellation.on_cancel(move || {
        observed.fetch_add(1, Ordering::SeqCst);
        nested.on_cancel(|| {});
        assert!(!nested.cancel());
    });
    assert!(cancellation.cancel());
    assert!(!cancellation.cancel());
    let observed = calls.clone();
    cancellation.on_cancel(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        cancellation.checkpoint().unwrap_err().kind,
        TaskErrorKind::Cancelled
    );
}

#[test]
fn cancellation_wakes_waiting_task_and_drops_its_pending_future() {
    let cancellation = CancellationToken::new();
    let drops = Arc::new(AtomicUsize::new(0));
    let dropped = drops.clone();
    let mut future = Box::pin(cancellation.run_until_cancelled(PendingDrop(dropped)));
    let wake = Arc::new(CountWake(AtomicUsize::new(0)));
    let waker = Waker::from(wake.clone());
    let mut context = Context::from_waker(&waker);

    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(cancellation.cancel());
    assert_eq!(wake.0.load(Ordering::SeqCst), 1);
    assert!(matches!(
        future.as_mut().poll(&mut context),
        Poll::Ready(Err(TaskError {
            kind: TaskErrorKind::Cancelled,
            ..
        }))
    ));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn registration_and_cancellation_can_race_without_losing_abort() {
    for _ in 0..100 {
        let cancellation = CancellationToken::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let thread = {
            let cancellation = cancellation.clone();
            let calls = calls.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                cancellation.on_cancel(move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                });
            })
        };
        barrier.wait();
        cancellation.cancel();
        thread.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn dropping_a_clone_does_not_cancel_but_shared_cancel_aborts_once() {
    let cancellation = CancellationToken::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let handle = TaskHandle::with_abort(cancellation.clone(), move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    drop(handle.clone());
    assert!(!cancellation.is_cancelled());
    handle.clone().cancel();
    handle.cancel();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn sink_preserves_rejected_payload_and_supports_non_clone_events() {
    struct Payload(u64);
    let sink = EventSink::new(|payload: Payload| Err(EventSendError::Full(payload)));
    match sink.clone().try_send(Payload(42)) {
        Err(EventSendError::Full(payload)) => assert_eq!(payload.0, 42),
        _ => panic!("expected backpressure"),
    }
}

#[test]
fn execution_contracts_are_object_safe_without_a_runtime() {
    let executor: Box<dyn BlockingExecutor> =
        Box::new(|_task: BlockingTask| Err(TaskError::new(TaskErrorKind::Rejected, "pool full")));
    assert_eq!(
        executor.spawn_blocking(Box::new(|| {})).unwrap_err().kind,
        TaskErrorKind::Rejected
    );
    let spawner: Box<dyn FutureSpawner> =
        Box::new(|_task: BoxTask| Err(TaskError::new(TaskErrorKind::Rejected, "runtime stopped")));
    assert!(spawner.spawn(Box::pin(async {})).is_err());
}
