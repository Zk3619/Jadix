//! Optional command/event bridge for external HTTP adapters, mirroring media playback.
//! No transport, runtime, worker or implicit global client is created here.

use crate::core::reactivity::{on_cleanup, Signal};
pub use jadix_net::*;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// One replaceable request slot. Clones share its state, token sequence and sink.
/// Create under its page/window owner, outside transient render closures.
/// The sink must enqueue promptly; marshal events onto the UI thread before publish.
#[derive(Clone)]
pub struct HttpController {
    snapshot: Signal<HttpSnapshot>,
    request_error: Signal<Option<HttpError>>,
    issued: Rc<Cell<u64>>,
    alive: Arc<AtomicBool>,
    window: crate::app::window_scope::WindowScope,
    command: Rc<dyn Fn(HttpCommand) -> Result<(), HttpError>>,
}

impl HttpController {
    pub fn new(
        snapshot: Signal<HttpSnapshot>,
        command: impl Fn(HttpCommand) -> Result<(), HttpError> + 'static,
    ) -> Self {
        let initial = snapshot.try_peek().unwrap_or_default();
        let alive = Arc::new(AtomicBool::new(true));
        let command = Rc::new(command);
        on_cleanup({
            let alive = alive.clone();
            let command = command.clone();
            move || {
                alive.store(false, Ordering::Release);
                if let Err(error) = command(HttpCommand::Shutdown) {
                    log::warn!("network adapter rejected owner shutdown: {error}");
                }
            }
        });
        Self {
            snapshot,
            request_error: Signal::new(None),
            issued: Rc::new(Cell::new(initial.request_id)),
            alive,
            window: crate::app::window_scope::active(),
            command,
        }
    }

    pub fn disconnected() -> Self {
        Self::new(Signal::new(HttpSnapshot::default()), |_| {
            Err(HttpError::new(
                HttpErrorKind::Disconnected,
                "no HTTP adapter connected",
            ))
        })
    }

    pub fn snapshot(&self) -> Signal<HttpSnapshot> {
        self.snapshot
    }
    pub fn request_error(&self) -> Signal<Option<HttpError>> {
        self.request_error
    }
    pub fn current(&self) -> HttpSnapshot {
        self.snapshot.try_peek().unwrap_or_default()
    }

    /// Cloneable worker-side callback. Posts observations to the existing UI queue;
    /// it never reads a Signal on the calling worker. Coalesce progress at the adapter.
    pub fn event_sink(&self) -> impl Fn(HttpEvent) + Clone + Send + Sync + 'static {
        let snapshot = self.snapshot;
        let alive = self.alive.clone();
        let window = self.window;
        move |event| {
            if !alive.load(Ordering::Acquire) {
                return;
            }
            let alive = alive.clone();
            crate::app::run_on_ui_thread(move || {
                if alive.load(Ordering::Acquire) {
                    let _window = crate::app::window_scope::enter(window);
                    publish_snapshot(snapshot, event);
                }
            });
        }
    }

    /// Typed push sink for HttpStarter adapters. Closed owners reject before queueing.
    /// Progress must still be coalesced by the backend; this is not a body-stream queue.
    pub fn backend_event_sink(&self) -> HttpEventSink {
        let publish = self.event_sink();
        let alive = self.alive.clone();
        HttpEventSink::new(move |event| {
            if !alive.load(Ordering::Acquire) {
                return Err(EventSendError::Closed(event));
            }
            publish(event);
            Ok(())
        })
    }

    /// Owns a single dispatcher; backend execution and runtime remain application-owned.
    pub fn with_backend(snapshot: Signal<HttpSnapshot>, backend: Arc<dyn HttpStarter>) -> Self {
        let dispatcher = Rc::new(std::cell::RefCell::new(None::<HttpDispatcher>));
        let shutdown = Rc::new(Cell::new(false));
        let command = dispatcher.clone();
        let controller = Self::new(snapshot, move |request| {
            let closing = matches!(request, HttpCommand::Shutdown);
            if closing {
                shutdown.set(true);
            }
            let Ok(mut dispatcher) = command.try_borrow_mut() else {
                return if closing {
                    Ok(())
                } else {
                    Err(HttpError::new(
                        HttpErrorKind::Backend,
                        "reentrant HTTP adapter dispatch",
                    ))
                };
            };
            let dispatcher = dispatcher.as_mut().ok_or_else(|| {
                HttpError::new(HttpErrorKind::Disconnected, "HTTP dispatcher unavailable")
            })?;
            let result = dispatcher.dispatch(request);
            if shutdown.get() {
                dispatcher.dispatch(HttpCommand::Shutdown)?;
            }
            result
        });
        *dispatcher.borrow_mut() = Some(HttpDispatcher::new(
            backend,
            controller.backend_event_sink(),
        ));
        controller
    }

    fn check_alive(&self) -> Result<(), HttpError> {
        if !self.alive.load(Ordering::Acquire) || !self.snapshot.is_alive() {
            return Err(HttpError::new(
                HttpErrorKind::Disconnected,
                "HTTP owner was disposed",
            ));
        }
        Ok(())
    }

    /// Returns an accepted token, not a response. New accepted sends replace the old job.
    /// Rejected sends restore unchanged prior state but never reuse their token.
    pub fn send(&self, request: HttpRequest) -> Result<u64, HttpError> {
        let result = (|| {
            self.check_alive()?;
            request.validate()?;
            let previous = self.current();
            let request_id = self
                .issued
                .get()
                .max(previous.request_id)
                .checked_add(1)
                .ok_or_else(|| HttpError::invalid("HTTP request token exhausted"))?;
            self.issued.set(request_id);
            let mut pending = previous.clone();
            pending.begin_request(request_id, &request)?;
            self.snapshot.try_set(pending.clone());
            self.check_alive()?;
            let current = self.current();
            if current.request_id != request_id || current.state != HttpState::Pending {
                return Err(HttpError::invalid(
                    "HTTP request was superseded before dispatch",
                ));
            }
            let result = (self.command)(HttpCommand::Send {
                request_id,
                request,
            });
            if result.is_err() && self.snapshot.try_peek().as_ref() == Some(&pending) {
                self.snapshot.try_set(previous);
            }
            crate::app::runtime::request_redraw();
            result.map(|()| request_id)
        })();
        self.request_error.try_set(result.as_ref().err().cloned());
        result
    }

    /// Fence late events immediately; a rejected cancellation restores unchanged state.
    pub fn cancel(&self) -> Result<(), HttpError> {
        let result = (|| {
            self.check_alive()?;
            let previous = self.current();
            if previous.state != HttpState::Pending {
                return Ok(());
            }
            let mut cancelled = previous.clone();
            cancelled.state = HttpState::Cancelled;
            self.snapshot.try_set(cancelled.clone());
            let result = (self.command)(HttpCommand::Cancel {
                request_id: previous.request_id,
            });
            if result.is_err() && self.snapshot.try_peek().as_ref() == Some(&cancelled) {
                self.snapshot.try_set(previous);
            }
            crate::app::runtime::request_redraw();
            result
        })();
        self.request_error.try_set(result.as_ref().err().cloned());
        result
    }

    /// UI-thread only. Ignored/duplicate observations do not notify or redraw.
    pub fn publish(&self, event: HttpEvent) -> bool {
        self.alive.load(Ordering::Acquire) && publish_snapshot(self.snapshot, event)
    }
}

fn publish_snapshot(state: Signal<HttpSnapshot>, event: HttpEvent) -> bool {
    let Some(mut snapshot) = state.try_peek() else {
        return false;
    };
    let previous = snapshot.clone();
    if !snapshot.apply(event) || snapshot == previous {
        return false;
    }
    let changed = state.try_set(snapshot);
    crate::app::runtime::request_redraw();
    changed
}

impl Default for HttpController {
    fn default() -> Self {
        Self::disconnected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK},
        core::reactivity::create_root,
    };

    #[derive(Default)]
    struct Starter(std::sync::Mutex<Option<(u64, HttpEventSink, CancellationToken)>>);

    impl HttpStarter for Starter {
        fn start(
            &self,
            id: u64,
            _request: HttpRequest,
            sink: HttpEventSink,
            cancellation: CancellationToken,
        ) -> Result<TaskHandle, HttpError> {
            *self.0.lock().unwrap() = Some((id, sink, cancellation.clone()));
            Ok(TaskHandle::cooperative(cancellation))
        }
    }

    #[test]
    fn push_backend_uses_ui_delivery_and_owner_cancels_the_native_job() {
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let backend = Arc::new(Starter::default());
        let (owner, controller) = create_root(|| {
            HttpController::with_backend(Signal::new(HttpSnapshot::default()), backend.clone())
        });
        let id = controller.send(HttpRequest::get("https://host")).unwrap();
        let (_, sink, cancel) = backend.0.lock().unwrap().take().unwrap();
        std::thread::spawn(move || {
            sink.try_send(HttpEvent::new(id, HttpEventKind::Cancelled))
                .unwrap()
        })
        .join()
        .unwrap();
        assert_eq!(controller.current().state, HttpState::Pending);
        drain_ui_tasks();
        assert_eq!(controller.current().state, HttpState::Cancelled);
        drop(owner);
        assert!(cancel.is_cancelled());
        assert!(controller.send(HttpRequest::get("https://host")).is_err());
        assert!(matches!(
            controller
                .backend_event_sink()
                .try_send(HttpEvent::new(id, HttpEventKind::Cancelled)),
            Err(EventSendError::Closed(_))
        ));
    }

    #[test]
    fn owner_disposal_during_start_defers_shutdown_without_leaking_accepted_work() {
        use crate::core::reactivity::OwnerHandle;
        thread_local! {
            static OWNER: std::cell::RefCell<Option<OwnerHandle>> = const { std::cell::RefCell::new(None) };
        }
        struct DisposeStarter(Arc<AtomicBool>);
        impl HttpStarter for DisposeStarter {
            fn start(
                &self,
                _id: u64,
                _request: HttpRequest,
                _sink: HttpEventSink,
                cancellation: CancellationToken,
            ) -> Result<TaskHandle, HttpError> {
                let owner = OWNER.with(|owner| owner.borrow_mut().take());
                drop(owner);
                let aborted = self.0.clone();
                Ok(TaskHandle::with_abort(cancellation, move || {
                    aborted.store(true, Ordering::Release);
                }))
            }
        }
        let aborted = Arc::new(AtomicBool::new(false));
        let (owner, controller) = create_root(|| {
            HttpController::with_backend(
                Signal::new(HttpSnapshot::default()),
                Arc::new(DisposeStarter(aborted.clone())),
            )
        });
        OWNER.with(|slot| *slot.borrow_mut() = Some(owner));
        controller.send(HttpRequest::get("https://host")).unwrap();
        assert!(aborted.load(Ordering::Acquire));
    }

    #[test]
    fn worker_events_only_apply_when_the_ui_queue_drains() {
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (_owner, ()) = create_root(|| {
            let controller = HttpController::new(Signal::new(HttpSnapshot::default()), |_| Ok(()));
            let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
            let sink = controller.event_sink();
            std::thread::spawn(move || sink(HttpEvent::new(request_id, HttpEventKind::Cancelled)))
                .join()
                .unwrap();
            assert_eq!(controller.current().state, HttpState::Pending);
            drain_ui_tasks();
            assert_eq!(controller.current().state, HttpState::Cancelled);
        });
    }

    #[test]
    fn queued_events_cannot_update_a_disposed_controller_with_a_live_parent_signal() {
        let _guard = UI_QUEUE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let (_parent, ()) = create_root(|| {
            let state = Signal::new(HttpSnapshot::default());
            let (owner, controller) = create_root(|| HttpController::new(state, |_| Ok(())));
            let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
            let sink = controller.event_sink();
            sink(HttpEvent::new(request_id, HttpEventKind::Cancelled));
            owner.dispose();
            drain_ui_tasks();
            assert_eq!(state.peek().state, HttpState::Pending);
            sink(HttpEvent::new(
                request_id,
                HttpEventKind::Failed(HttpError::new(HttpErrorKind::Backend, "late")),
            ));
            drain_ui_tasks();
            assert_eq!(state.peek().state, HttpState::Pending);
        });
    }
}
