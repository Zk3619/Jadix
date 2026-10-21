//! Runtime-neutral contracts. No threads, executor, reactor or global runtime are installed.

use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

type CancelAction = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    actions: Mutex<Vec<CancelAction>>,
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
        for action in actions {
            action();
        }
        true
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
