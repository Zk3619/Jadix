use crate::{HttpError, HttpErrorKind, HttpRequest, HttpResponse, DEFAULT_MAX_RESPONSE_BYTES};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HttpState {
    #[default]
    Idle,
    Pending,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferProgress {
    pub transferred: u64,
    pub total: Option<u64>,
}

impl TransferProgress {
    pub fn is_valid(self) -> bool {
        self.total.is_none_or(|total| self.transferred <= total)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpEvent {
    pub request_id: u64,
    pub kind: HttpEventKind,
}

impl HttpEvent {
    pub fn new(request_id: u64, kind: HttpEventKind) -> Self {
        Self { request_id, kind }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpEventKind {
    UploadProgress(TransferProgress),
    DownloadProgress(TransferProgress),
    Completed(HttpResponse),
    Failed(HttpError),
    Cancelled,
}

/// A single request slot. Separate controllers/channels enable concurrent slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpSnapshot {
    pub request_id: u64,
    pub state: HttpState,
    pub upload: TransferProgress,
    pub download: TransferProgress,
    pub response: Option<HttpResponse>,
    pub error: Option<HttpError>,
    pub max_response_bytes: usize,
}

impl Default for HttpSnapshot {
    fn default() -> Self {
        Self {
            request_id: 0,
            state: HttpState::Idle,
            upload: TransferProgress::default(),
            download: TransferProgress::default(),
            response: None,
            error: None,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }
}

impl HttpSnapshot {
    /// Fence the old job before dispatching the new request.
    pub fn begin_request(
        &mut self,
        request_id: u64,
        request: &HttpRequest,
    ) -> Result<(), HttpError> {
        request.validate()?;
        if request_id <= self.request_id {
            return Err(HttpError::invalid("HTTP request token must increase"));
        }
        *self = Self {
            request_id,
            state: HttpState::Pending,
            max_response_bytes: request.max_response_bytes,
            ..Self::default()
        };
        Ok(())
    }

    /// Reject stale/future/terminal events. True means accepted, not necessarily changed.
    pub fn apply(&mut self, event: HttpEvent) -> bool {
        if event.request_id == 0
            || event.request_id != self.request_id
            || self.state != HttpState::Pending
        {
            return false;
        }
        match event.kind {
            HttpEventKind::UploadProgress(progress) => {
                if !progress.is_valid() || progress.transferred < self.upload.transferred {
                    return false;
                }
                self.upload = progress;
            }
            HttpEventKind::DownloadProgress(progress) => {
                if !progress.is_valid() || progress.transferred < self.download.transferred {
                    return false;
                }
                self.download = progress;
            }
            HttpEventKind::Completed(response) => {
                let validation = response.validate().and_then(|()| {
                    if response.body.len() > self.max_response_bytes {
                        Err(HttpError::new(
                            HttpErrorKind::BodyTooLarge,
                            "HTTP response exceeded body limit",
                        ))
                    } else {
                        Ok(())
                    }
                });
                match validation {
                    Ok(()) => {
                        self.state = HttpState::Completed;
                        self.response = Some(response);
                    }
                    Err(error) => {
                        self.state = HttpState::Failed;
                        self.error = Some(error);
                    }
                }
            }
            HttpEventKind::Failed(error) => {
                self.state = if error.kind == HttpErrorKind::Cancelled {
                    HttpState::Cancelled
                } else {
                    HttpState::Failed
                };
                self.error = Some(error);
            }
            HttpEventKind::Cancelled => self.state = HttpState::Cancelled,
        }
        true
    }
}
