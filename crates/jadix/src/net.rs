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
#[path = "tests/net.rs"]
mod tests;
