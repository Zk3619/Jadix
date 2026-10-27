//! Owner-scoped background work and bounded subscriptions, without a built-in runtime.

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    marker::PhantomData,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crate::core::reactivity::{current_owner, on_cleanup, with_owner, OwnerId};
pub use jadix_tasks::{
    BlockingExecutor, BlockingTask, BoxTask, CancellationToken, EventSendError, EventSink,
    FutureSpawner, TaskError, TaskErrorKind, TaskHandle,
};

use super::{ui_bridge::run_on_ui_thread, window_scope};

#[cfg(feature = "runtime-tokio")]
pub use jadix_tokio::TokioExecutor;

#[derive(Default)]
struct ScopeState {
    closed: AtomicBool,
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, TaskHandle>>,
}

impl ScopeState {
    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let jobs =
            std::mem::take(&mut *self.jobs.lock().unwrap_or_else(|error| error.into_inner()));
        for handle in jobs.into_values() {
            handle.cancel();
        }
    }

    fn remove(&self, id: u64) {
        self.jobs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&id);
    }
}

/// Create once under a page/window owner, not in a transient render callback.
/// Clones share a lifetime; dropping one clone does not cancel other users.
/// This UI-thread handle is deliberately !Send; only tokens, sinks and jobs cross threads.
#[derive(Clone)]
pub struct TaskScope {
    state: Arc<ScopeState>,
    window: window_scope::WindowScope,
    owner: Option<OwnerId>,
    ui_thread: PhantomData<Rc<()>>,
}

impl TaskScope {
    pub fn new() -> Self {
        let state = Arc::new(ScopeState::default());
        let cleanup = state.clone();
        on_cleanup(move || cleanup.close());
        Self {
            state,
            window: window_scope::active(),
            owner: current_owner(),
            ui_thread: PhantomData,
        }
    }

    /// Permanently close admission and fence callbacks before asking backends to abort.
    pub fn close(&self) {
        self.state.close();
    }

    pub fn is_closed(&self) -> bool {
        self.state.closed.load(Ordering::Acquire)
    }

    fn register(&self) -> Result<(u64, TaskHandle), TaskError> {
        if self.is_closed() {
            return Err(TaskError::new(TaskErrorKind::Rejected, "task scope closed"));
        }
        let id = self
            .state
            .next
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| TaskError::new(TaskErrorKind::Rejected, "task IDs exhausted"))?
            + 1;
        let handle = TaskHandle::default();
        let weak = Arc::downgrade(&self.state);
        handle.cancellation().on_cancel(move || {
            if let Some(state) = weak.upgrade() {
                state.remove(id);
            }
        });
        self.state
            .jobs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id, handle.clone());
        Ok((id, handle))
    }

    fn attach(
        handle: TaskHandle,
        accepted: Result<TaskHandle, TaskError>,
    ) -> Result<TaskHandle, TaskError> {
        match accepted {
            Ok(backend) => {
                handle.cancellation().link_handle(backend);
                Ok(handle)
            }
            Err(error) => {
                handle.cancel();
                Err(error)
            }
        }
    }

    /// Convenience for occasional blocking work: one native thread per call, not a pool.
    /// Long-lived IO and high concurrency should use an injected executor or subscription.
    pub fn spawn_blocking<T, Work, Callback>(
        &self,
        work: Work,
        on_ui: Callback,
    ) -> Result<TaskHandle, TaskError>
    where
        T: Send + 'static,
        Work: FnOnce(CancellationToken) -> Result<T, TaskError> + Send + 'static,
        Callback: FnOnce(Result<T, TaskError>) + Send + 'static,
    {
        self.spawn_blocking_on(
            &|job: BlockingTask| {
                std::thread::Builder::new()
                    .name("jadix-task".into())
                    .spawn(job)
                    .map_err(|error| TaskError::new(TaskErrorKind::Rejected, error.to_string()))?;
                Ok(TaskHandle::default())
            },
            work,
            on_ui,
        )
    }

    pub fn spawn_blocking_on<T, Work, Callback>(
        &self,
        executor: &dyn BlockingExecutor,
        work: Work,
        on_ui: Callback,
    ) -> Result<TaskHandle, TaskError>
    where
        T: Send + 'static,
        Work: FnOnce(CancellationToken) -> Result<T, TaskError> + Send + 'static,
        Callback: FnOnce(Result<T, TaskError>) + Send + 'static,
    {
        let (id, handle) = self.register()?;
        let cancel = handle.cancellation();
        let mut completion = self.completion(id, cancel.clone(), on_ui);
        let accepted = executor.spawn_blocking(Box::new(move || {
            let result = cancel.checkpoint().and_then(|()| {
                catch_unwind(AssertUnwindSafe(|| work(cancel.clone()))).unwrap_or_else(|_| {
                    Err(TaskError::new(
                        TaskErrorKind::Panicked,
                        "background task panicked",
                    ))
                })
            });
            completion.deliver(result);
        }));
        Self::attach(handle, accepted)
    }

    /// The factory is invoked inside the supplied runtime, not on the UI thread.
    /// Its IO/timer dependencies must be compatible with that runtime.
    pub fn spawn<T, Factory, Work, Callback>(
        &self,
        spawner: &dyn FutureSpawner,
        factory: Factory,
        on_ui: Callback,
    ) -> Result<TaskHandle, TaskError>
    where
        T: Send + 'static,
        Factory: FnOnce(CancellationToken) -> Work + Send + 'static,
        Work: Future<Output = Result<T, TaskError>> + Send + 'static,
        Callback: FnOnce(Result<T, TaskError>) + Send + 'static,
    {
        let (id, handle) = self.register()?;
        let cancel = handle.cancellation();
        let mut completion = self.completion(id, cancel.clone(), on_ui);
        let accepted = spawner.spawn(Box::pin(async move {
            let result = match cancel.checkpoint() {
                Ok(()) => factory(cancel.clone()).await,
                Err(error) => Err(error),
            };
            completion.deliver(result);
        }));
        Self::attach(handle, accepted)
    }

    fn completion<T, Callback>(
        &self,
        id: u64,
        cancel: CancellationToken,
        callback: Callback,
    ) -> Completion<T, Callback>
    where
        T: Send + 'static,
        Callback: FnOnce(Result<T, TaskError>) + Send + 'static,
    {
        Completion {
            state: self.state.clone(),
            id,
            cancel,
            window: self.window,
            owner: self.owner,
            callback: Some(callback),
            output: PhantomData,
        }
    }

    /// Bounded, FIFO, nonblocking stream. Full returns ownership to the producer;
    /// it must pause/retry or explicitly choose a domain-specific lossy strategy.
    /// The backend's start callback only enqueues/starts work and must not block.
    pub fn subscribe<Event, Start, Callback>(
        &self,
        capacity: usize,
        start: Start,
        on_ui: Callback,
    ) -> Result<TaskHandle, TaskError>
    where
        Event: Send + 'static,
        Start: FnOnce(EventSink<Event>, CancellationToken) -> Result<TaskHandle, TaskError>,
        Callback: FnMut(Event) + Send + 'static,
    {
        if capacity == 0 {
            return Err(TaskError::new(
                TaskErrorKind::Rejected,
                "subscription capacity must be positive",
            ));
        }
        let (id, handle) = self.register()?;
        let cancel = handle.cancellation();
        let mailbox = Arc::new(Mailbox {
            id,
            queue: Mutex::new(MailboxQueue {
                events: VecDeque::new(),
                scheduled: false,
            }),
            callback: Mutex::new(Some(on_ui)),
            state: self.state.clone(),
            cancel: cancel.clone(),
            capacity,
            window: self.window,
            owner: self.owner,
        });
        let weak = Arc::downgrade(&mailbox);
        cancel.on_cancel(move || {
            if let Some(mailbox) = weak.upgrade() {
                mailbox
                    .queue
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .events
                    .clear();
                mailbox
                    .callback
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take();
            }
        });
        let sink = EventSink::new(move |event| mailbox.send(event));
        Self::attach(handle, start(sink, cancel))
    }
}

impl Default for TaskScope {
    fn default() -> Self {
        Self::new()
    }
}

struct Completion<T: Send + 'static, Callback: FnOnce(Result<T, TaskError>) + Send + 'static> {
    state: Arc<ScopeState>,
    id: u64,
    cancel: CancellationToken,
    window: window_scope::WindowScope,
    owner: Option<OwnerId>,
    callback: Option<Callback>,
    output: PhantomData<T>,
}

fn in_ui_scope(window: window_scope::WindowScope, owner: Option<OwnerId>, callback: impl FnOnce()) {
    let _window = window_scope::enter(window);
    match owner {
        Some(owner) => with_owner(owner, callback),
        None => callback(),
    }
    super::runtime::request_redraw();
}

impl<T: Send + 'static, Callback: FnOnce(Result<T, TaskError>) + Send + 'static>
    Completion<T, Callback>
{
    fn deliver(&mut self, result: Result<T, TaskError>) {
        let Some(callback) = self.callback.take() else {
            return;
        };
        self.state.remove(self.id);
        if self.state.closed.load(Ordering::Acquire) || self.cancel.is_cancelled() {
            return;
        }
        let state = self.state.clone();
        let cancel = self.cancel.clone();
        let window = self.window;
        let owner = self.owner;
        run_on_ui_thread(move || {
            if !state.closed.load(Ordering::Acquire) && !cancel.is_cancelled() {
                in_ui_scope(window, owner, || callback(result));
            }
        });
    }
}

impl<T: Send + 'static, Callback: FnOnce(Result<T, TaskError>) + Send + 'static> Drop
    for Completion<T, Callback>
{
    fn drop(&mut self) {
        self.deliver(Err(TaskError::new(
            TaskErrorKind::Abandoned,
            "executor dropped unfinished work",
        )));
    }
}

struct MailboxQueue<Event> {
    events: VecDeque<Event>,
    scheduled: bool,
}

struct Mailbox<Event, Callback> {
    id: u64,
    queue: Mutex<MailboxQueue<Event>>,
    callback: Mutex<Option<Callback>>,
    state: Arc<ScopeState>,
    cancel: CancellationToken,
    capacity: usize,
    window: window_scope::WindowScope,
    owner: Option<OwnerId>,
}

impl<Event, Callback> Drop for Mailbox<Event, Callback> {
    fn drop(&mut self) {
        self.state.remove(self.id);
    }
}

impl<Event: Send + 'static, Callback: FnMut(Event) + Send + 'static> Mailbox<Event, Callback> {
    fn closed(&self) -> bool {
        self.state.closed.load(Ordering::Acquire) || self.cancel.is_cancelled()
    }

    fn schedule(self: &Arc<Self>) {
        let mailbox = self.clone();
        run_on_ui_thread(move || mailbox.drain());
    }

    fn send(self: &Arc<Self>, event: Event) -> Result<(), EventSendError<Event>> {
        let schedule = {
            let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
            if self.closed() {
                return Err(EventSendError::Closed(event));
            }
            if queue.events.len() >= self.capacity {
                return Err(EventSendError::Full(event));
            }
            queue.events.push_back(event);
            let schedule = !queue.scheduled;
            queue.scheduled = true;
            schedule
        };
        if schedule {
            self.schedule();
        }
        Ok(())
    }

    fn drain(self: Arc<Self>) {
        let events: Vec<_> = {
            let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
            let count = queue.events.len().min(64);
            queue.events.drain(..count).collect()
        };
        let callback = self
            .callback
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(mut callback) = callback {
            for event in events {
                if self.closed() {
                    break;
                }
                in_ui_scope(self.window, self.owner, || callback(event));
            }
            if !self.closed() {
                *self
                    .callback
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(callback);
            }
        }
        let schedule = {
            let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
            queue.scheduled = !queue.events.is_empty() && !self.closed();
            queue.scheduled
        };
        if schedule {
            self.schedule();
        }
    }
}
