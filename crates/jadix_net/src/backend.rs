use std::sync::Arc;

use jadix_tasks::EventSink;

use crate::{
    CancellationToken, HttpCommand, HttpError, HttpErrorKind, HttpEvent, HttpRequest, TaskHandle,
};

pub type HttpEventSink = EventSink<HttpEvent>;

/// Push-based alternative to HttpBackend's command/poll seam.
/// Start only validates/enqueues and returns promptly; the adapter owns execution.
/// Err admits no work and emits no events; Ok must eventually emit exactly one terminal
/// event unless cancelled. Events retain request_id, including progress and failures.
/// Cancellation is logical; the returned handle bridges best-effort native abort.
/// Full/Closed from the sink must be handled without blocking the caller/UI thread.
pub trait HttpStarter: Send + Sync {
    fn start(
        &self,
        request_id: u64,
        request: HttpRequest,
        sink: HttpEventSink,
        cancellation: CancellationToken,
    ) -> Result<TaskHandle, HttpError>;
}

/// Reuses the media-style single-slot command contract for push-based adapters.
/// No worker, transport, event pump or runtime is created. Use one dispatcher per slot.
pub struct HttpDispatcher {
    backend: Arc<dyn HttpStarter>,
    sink: HttpEventSink,
    active: Option<(u64, CancellationToken)>,
    issued: u64,
    closed: bool,
}

impl HttpDispatcher {
    pub fn new(backend: Arc<dyn HttpStarter>, sink: HttpEventSink) -> Self {
        Self {
            backend,
            sink,
            active: None,
            issued: 0,
            closed: false,
        }
    }

    pub fn dispatch(&mut self, command: HttpCommand) -> Result<(), HttpError> {
        command.validate()?;
        match command {
            HttpCommand::Send {
                request_id,
                request,
            } => {
                if self.closed {
                    return Err(HttpError::new(
                        HttpErrorKind::Disconnected,
                        "HTTP channel closed",
                    ));
                }
                if request_id <= self.issued {
                    return Err(HttpError::invalid("HTTP request token must increase"));
                }
                self.issued = request_id;
                let cancellation = CancellationToken::new();
                let observed = cancellation.clone();
                let sink = self.sink.clone();
                let events = HttpEventSink::new(move |event| {
                    if observed.is_cancelled() || event.request_id != request_id {
                        return Err(crate::EventSendError::Closed(event));
                    }
                    sink.try_send(event)
                });
                match self
                    .backend
                    .start(request_id, request, events, cancellation.clone())
                {
                    Ok(handle) => {
                        cancellation.link_handle(handle);
                        if let Some((_, previous)) = self.active.replace((request_id, cancellation))
                        {
                            previous.cancel();
                        }
                        Ok(())
                    }
                    Err(error) => {
                        cancellation.cancel();
                        Err(error)
                    }
                }
            }
            HttpCommand::Cancel { request_id } => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|(active, _)| *active == request_id)
                {
                    if let Some((_, cancellation)) = self.active.take() {
                        cancellation.cancel();
                    }
                }
                Ok(())
            }
            HttpCommand::Shutdown => {
                self.closed = true;
                if let Some((_, cancellation)) = self.active.take() {
                    cancellation.cancel();
                }
                Ok(())
            }
        }
    }
}

impl Drop for HttpDispatcher {
    fn drop(&mut self) {
        if let Some((_, cancellation)) = self.active.take() {
            cancellation.cancel();
        }
    }
}
