//! Tokio execution with nonblocking admission and an application-owned runtime.
#![doc = include_str!("../README.md")]

use std::sync::Arc;

use jadix_tasks::{
    BlockingExecutor, BlockingTask, BoxTask, CancellationToken, FutureSpawner, TaskError,
    TaskErrorKind, TaskHandle,
};
use tokio::{runtime::Handle, sync::Semaphore};

/// Clones share a capacity limit across asynchronous and blocking tasks.
///
/// This adapter never creates or shuts down a runtime. The application must keep
/// its runtime running, with the drivers needed by its futures enabled.
#[derive(Clone)]
pub struct TokioExecutor {
    runtime: Handle,
    permits: Arc<Semaphore>,
}

impl TokioExecutor {
    /// Attach to the entered Tokio runtime, rejecting calls outside its context.
    pub fn try_current(capacity: usize) -> Result<Self, TaskError> {
        let runtime = Handle::try_current().map_err(|_| {
            TaskError::new(
                TaskErrorKind::Rejected,
                "TokioExecutor requires an entered Tokio runtime",
            )
        })?;
        Self::new(runtime, capacity)
    }

    /// Reject invalid capacity instead of creating an unlimited execution queue.
    pub fn new(runtime: Handle, capacity: usize) -> Result<Self, TaskError> {
        if capacity == 0 || capacity > Semaphore::MAX_PERMITS {
            return Err(TaskError::new(
                TaskErrorKind::Rejected,
                "Tokio executor capacity is out of range",
            ));
        }
        Ok(Self {
            runtime,
            permits: Arc::new(Semaphore::new(capacity)),
        })
    }

    /// Permanently reject new tasks through every clone. Existing scopes still
    /// own cancellation of their accepted work.
    pub fn close(&self) {
        self.permits.close();
    }

    fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit, TaskError> {
        self.permits.clone().try_acquire_owned().map_err(|_| {
            TaskError::new(TaskErrorKind::Rejected, "Tokio executor is full or closed")
        })
    }
}

impl FutureSpawner for TokioExecutor {
    fn spawn(&self, task: BoxTask) -> Result<TaskHandle, TaskError> {
        let permit = self.acquire()?;
        let join = self.runtime.spawn(async move {
            let _permit = permit;
            task.await;
        });
        let abort = join.abort_handle();
        Ok(TaskHandle::with_abort(
            CancellationToken::new(),
            move || abort.abort(),
        ))
    }
}

impl BlockingExecutor for TokioExecutor {
    fn spawn_blocking(&self, task: BlockingTask) -> Result<TaskHandle, TaskError> {
        let permit = self.acquire()?;
        let join = self.runtime.spawn_blocking(move || {
            let _permit = permit;
            task();
        });
        let abort = join.abort_handle();
        Ok(TaskHandle::with_abort(
            CancellationToken::new(),
            move || abort.abort(),
        ))
    }
}
