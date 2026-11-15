//! Runtime-neutral contracts. No threads, executor, reactor or global runtime are installed.

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll, Waker},
};

type CancelAction = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    actions: Mutex<Vec<CancelAction>>,
    waiters: Mutex<Vec<Weak<CancelWaiter>>>,
}

#[derive(Default)]
struct CancelWaiter {
    waker: Mutex<Option<Waker>>,
}

struct Cancelled {
    token: CancellationToken,
    waiter: Arc<CancelWaiter>,
}

impl Future for Cancelled {
    type Output = ();

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let old_waker = {
            let mut waiters = self
                .token
                .0
                .waiters
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            waiters.retain(|waiter| waiter.strong_count() > 0);
            if self.token.is_cancelled() {
                return Poll::Ready(());
            }
            let old_waker = {
                let mut waker = self
                    .waiter
                    .waker
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                match waker.as_mut() {
                    Some(current) if current.will_wake(cx.waker()) => None,
                    Some(current) => Some(std::mem::replace(current, cx.waker().clone())),
                    None => {
                        *waker = Some(cx.waker().clone());
                        None
                    }
                }
            };
            if !waiters.iter().any(|waiter| {
                waiter
                    .upgrade()
                    .is_some_and(|waiter| Arc::ptr_eq(&waiter, &self.waiter))
            }) {
                waiters.push(Arc::downgrade(&self.waiter));
            }
            old_waker
        };
        drop(old_waker);
        Poll::Pending
    }
}

impl Drop for Cancelled {
    fn drop(&mut self) {
        self.token
            .0
            .waiters
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|waiter| {
                waiter
                    .upgrade()
                    .is_some_and(|waiter| !Arc::ptr_eq(&waiter, &self.waiter))
            });
    }
}

/// Shared logical cancellation. Callbacks bridge to a backend's native abort mechanism.
/// Callbacks must be prompt, nonblocking and non-panicking; they run on the cancelling thread.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<CancellationState>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    /// Returns true only for the first cancellation. No callback runs under the mutex.
    pub fn cancel(&self) -> bool {
        if self.0.cancelled.swap(true, Ordering::AcqRel) {
            return false;
        }
        let actions = std::mem::take(
            &mut *self
                .0
                .actions
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        let waiters = std::mem::take(
            &mut *self
                .0
                .waiters
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        for action in actions {
            action();
        }
        for waiter in waiters.into_iter().filter_map(|waiter| waiter.upgrade()) {
            let waker = waiter
                .waker
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
        true
    }

    /// Runs a future until it completes or this token is cancelled.
    /// Cancellation wakes the task and drops the future, releasing timers and other resources.
    pub async fn run_until_cancelled<F>(&self, future: F) -> Result<F::Output, TaskError>
    where
        F: Future,
    {
        let mut future = Box::pin(future);
        let mut cancelled = Cancelled {
            token: self.clone(),
            waiter: Arc::new(CancelWaiter::default()),
        };
        std::future::poll_fn(|cx| {
            if std::pin::Pin::new(&mut cancelled).poll(cx).is_ready() {
                return Poll::Ready(Err(TaskError::new(
                    TaskErrorKind::Cancelled,
                    "task cancelled",
                )));
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(_) if self.is_cancelled() => Poll::Ready(Err(TaskError::new(
                    TaskErrorKind::Cancelled,
                    "task cancelled",
                ))),
                Poll::Ready(output) => Poll::Ready(Ok(output)),
                Poll::Pending => Poll::Pending,
            }
        })
        .await
    }

    /// Registers before or after cancellation without losing the cancellation race.
    pub fn on_cancel(&self, action: impl FnOnce() + Send + 'static) {
        let action = {
            let mut actions = self
                .0
                .actions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if self.is_cancelled() {
                Some(action)
            } else {
                actions.push(Box::new(action));
                None
            }
        };
        if let Some(action) = action {
            action();
        }
    }

    pub fn checkpoint(&self) -> Result<(), TaskError> {
        if self.is_cancelled() {
            Err(TaskError::new(TaskErrorKind::Cancelled, "task cancelled"))
        } else {
            Ok(())
        }
    }

    /// Link an independently owned native handle without self-capturing a shared token.
    pub fn link_handle(&self, handle: TaskHandle) {
        if !Arc::ptr_eq(&self.0, &handle.cancellation.0) {
            self.on_cancel(move || {
                handle.cancel();
            });
        }
    }
}

impl fmt::Debug for CancellationToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

/// Cloneable cancellation control, not a join handle. Dropping a clone does not cancel.
/// A GUI task scope, rather than arbitrary handle drops, owns cancellation on teardown.
#[derive(Clone, Debug, Default)]
pub struct TaskHandle {
    cancellation: CancellationToken,
}

impl TaskHandle {
    pub fn cooperative(cancellation: CancellationToken) -> Self {
        Self { cancellation }
    }

    pub fn with_abort(
        cancellation: CancellationToken,
        abort: impl FnOnce() + Send + 'static,
    ) -> Self {
        cancellation.on_cancel(abort);
        Self { cancellation }
    }

    pub fn cancel(&self) -> bool {
        self.cancellation.cancel()
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskErrorKind {
    Rejected,
    Failed,
    Cancelled,
    Panicked,
    Abandoned,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskError {
    pub kind: TaskErrorKind,
    pub message: String,
}

impl TaskError {
    pub fn new(kind: TaskErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for TaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for TaskError {}

pub type BlockingTask = Box<dyn FnOnce() + Send + 'static>;
pub type BoxTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Nonblocking admission to an application-owned pool. Err means no execution was accepted.
/// Accepted work must eventually run or be dropped. Never join workers from this method.
pub trait BlockingExecutor: Send + Sync {
    fn spawn_blocking(&self, task: BlockingTask) -> Result<TaskHandle, TaskError>;
}

impl<Executor> BlockingExecutor for Executor
where
    Executor: Fn(BlockingTask) -> Result<TaskHandle, TaskError> + Send + Sync,
{
    fn spawn_blocking(&self, task: BlockingTask) -> Result<TaskHandle, TaskError> {
        self(task)
    }
}

/// Nonblocking admission to a compatible external runtime. The runtime owns polling,
/// wakeups and IO/timer drivers. Err means no execution was accepted.
pub trait FutureSpawner: Send + Sync {
    fn spawn(&self, task: BoxTask) -> Result<TaskHandle, TaskError>;
}

impl<Spawner> FutureSpawner for Spawner
where
    Spawner: Fn(BoxTask) -> Result<TaskHandle, TaskError> + Send + Sync,
{
    fn spawn(&self, task: BoxTask) -> Result<TaskHandle, TaskError> {
        self(task)
    }
}

/// Rejected payloads are returned to the producer; reliable streams must retry or pause.
#[derive(Debug, PartialEq, Eq)]
pub enum EventSendError<Event> {
    Full(Event),
    Closed(Event),
}

/// Typed, thread-safe producer seam; implementations must return promptly, never block.
pub struct EventSink<Event> {
    send: Arc<dyn Fn(Event) -> Result<(), EventSendError<Event>> + Send + Sync>,
}

impl<Event> Clone for EventSink<Event> {
    fn clone(&self) -> Self {
        Self {
            send: self.send.clone(),
        }
    }
}

impl<Event> EventSink<Event> {
    pub fn new(
        send: impl Fn(Event) -> Result<(), EventSendError<Event>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            send: Arc::new(send),
        }
    }

    pub fn try_send(&self, event: Event) -> Result<(), EventSendError<Event>> {
        (self.send)(event)
    }
}
