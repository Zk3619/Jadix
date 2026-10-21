//! Backend-neutral HTTP contracts, following the same command/event seam as jadix_media.
//!
//! Default builds use only std. Applications supply and drive adapters; Jadix does
//! not implement HTTP, start threads, install an executor, or select a TLS stack.

mod backend;
mod request;
mod response;
mod state;

pub use backend::{HttpDispatcher, HttpEventSink, HttpStarter};
pub use jadix_tasks::{CancellationToken, EventSendError, TaskHandle};
pub use request::{HttpHeader, HttpMethod, HttpRequest};
pub use response::HttpResponse;
pub use state::{HttpEvent, HttpEventKind, HttpSnapshot, HttpState, TransferProgress};
use std::{error::Error, fmt, time::Duration};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const RECOMMENDED_PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpErrorKind {
    Unsupported,
    InvalidInput,
    Backend,
    Disconnected,
    Timeout,
    Cancelled,
    HttpStatus,
    BodyTooLarge,
    Decode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError {
    pub kind: HttpErrorKind,
    pub message: String,
}

impl HttpError {
    pub fn new(kind: HttpErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(HttpErrorKind::InvalidInput, message)
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for HttpError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpCommand {
    /// One logical request slot per command/event channel. Accepting a new Send
    /// supersedes and cancels its preceding job. Err means no work was accepted.
    Send {
        request_id: u64,
        request: HttpRequest,
    },
    /// Idempotent; cancelling an old or already completed job must be harmless.
    Cancel { request_id: u64 },
    /// Permanently close this channel and release all its jobs, even during dispatch.
    Shutdown,
}

impl HttpCommand {
    pub fn validate(&self) -> Result<(), HttpError> {
        match self {
            Self::Send { request_id, .. } | Self::Cancel { request_id } if *request_id == 0 => {
                Err(HttpError::invalid("HTTP request token must be nonzero"))
            }
            Self::Send { request, .. } => request.validate(),
            _ => Ok(()),
        }
    }
}

/// Implemented and driven by application-owned third-party adapters.
///
/// Dispatch only validates/enqueues; polling never blocks. Adapters own HTTP/TLS,
/// runtimes, full-body deadlines, body-size limits, cancellation and shutdown.
/// Capture request_id when creating each job and preserve it on every event.
/// Coalesce progress before posting to the UI; completion/errors are immediate.
/// Honor the request policy or reject with Unsupported; never silently ignore it.
pub trait HttpBackend {
    fn dispatch(&mut self, command: HttpCommand) -> Result<(), HttpError>;
    fn poll_event(&mut self) -> Option<HttpEvent>;
}
