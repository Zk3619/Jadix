use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

use jadix_tasks::{BlockingExecutor, FutureSpawner, TaskErrorKind};
use jadix_tokio::TokioExecutor;
use tokio::runtime::Builder;

#[test]
fn asynchronous_work_uses_the_supplied_timer_driver() {
    let runtime = Builder::new_current_thread().enable_time().build().unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 1).unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    executor
        .spawn(Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(1)).await;
            send.send(42).unwrap();
        }))
        .unwrap();
    assert_eq!(runtime.block_on(receive).unwrap(), 42);
}

#[test]
fn current_runtime_admission_is_fallible_and_preserves_capacity_validation() {
    assert!(matches!(
        TokioExecutor::try_current(1),
        Err(error) if error.kind == TaskErrorKind::Rejected
    ));
    let runtime = Builder::new_current_thread().enable_time().build().unwrap();
    let executor = {
        let _entered = runtime.enter();
        assert!(TokioExecutor::try_current(0).is_err());
        TokioExecutor::try_current(1).unwrap()
    };
    let (send, receive) = tokio::sync::oneshot::channel();
    executor
        .spawn(Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(1)).await;
            send.send(42).unwrap();
        }))
        .unwrap();
    assert_eq!(runtime.block_on(receive).unwrap(), 42);
}

#[test]
fn dropping_a_task_handle_does_not_cancel_accepted_work() {
    let runtime = Builder::new_current_thread().build().unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 1).unwrap();
    let (send, receive) = tokio::sync::oneshot::channel();
    drop(
        executor
            .spawn(Box::pin(async move { send.send(42).unwrap() }))
            .unwrap(),
    );
    assert_eq!(runtime.block_on(receive).unwrap(), 42);
    assert!(executor.spawn(Box::pin(async {})).is_ok());
}

#[test]
fn saturation_rejects_without_running_and_abort_releases_capacity() {
    let runtime = Builder::new_current_thread().build().unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 1).unwrap();
    let first = executor.spawn(Box::pin(std::future::pending())).unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    let observed = ran.clone();
    let rejected = executor.spawn(Box::pin(async move {
        observed.store(true, Ordering::Release);
    }));
    assert_eq!(rejected.unwrap_err().kind, TaskErrorKind::Rejected);
    assert!(first.cancel());
    runtime.block_on(async { tokio::task::yield_now().await });
    let (send, receive) = tokio::sync::oneshot::channel();
    executor
        .spawn(Box::pin(async move { send.send(()).unwrap() }))
        .unwrap();
    runtime.block_on(receive).unwrap();
    assert!(!ran.load(Ordering::Acquire));
}

#[test]
fn running_blocking_work_keeps_capacity_even_after_cancellation() {
    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 1).unwrap();
    let (started_send, started_receive) = mpsc::channel();
    let (release_send, release_receive) = mpsc::channel();
    let (finished_send, finished_receive) = tokio::sync::oneshot::channel();
    let task = executor
        .spawn_blocking(Box::new(move || {
            started_send.send(()).unwrap();
            release_receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            finished_send.send(()).unwrap();
        }))
        .unwrap();
    started_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    task.cancel();
    let rejected = executor.spawn(Box::pin(async {}));
    release_send.send(()).unwrap();
    runtime.block_on(finished_receive).unwrap();
    assert_eq!(rejected.unwrap_err().kind, TaskErrorKind::Rejected);
}

#[test]
fn closing_a_clone_rejects_both_execution_paths() {
    let runtime = Builder::new_current_thread().build().unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 2).unwrap();
    executor.clone().close();
    assert_eq!(
        executor.spawn(Box::pin(async {})).unwrap_err().kind,
        TaskErrorKind::Rejected
    );
    assert_eq!(
        executor.spawn_blocking(Box::new(|| {})).unwrap_err().kind,
        TaskErrorKind::Rejected
    );
    assert!(TokioExecutor::new(runtime.handle().clone(), 0).is_err());
    assert!(TokioExecutor::new(runtime.handle().clone(), usize::MAX).is_err());
}

#[test]
fn stopping_the_runtime_drops_accepted_futures() {
    struct ObserveDrop(Arc<AtomicBool>);
    impl Drop for ObserveDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let runtime = Builder::new_current_thread().build().unwrap();
    let executor = TokioExecutor::new(runtime.handle().clone(), 1).unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let witness = ObserveDrop(dropped.clone());
    executor
        .spawn(Box::pin(async move {
            let _witness = witness;
            std::future::pending::<()>().await;
        }))
        .unwrap();
    drop(runtime);
    assert!(dropped.load(Ordering::Acquire));
}
