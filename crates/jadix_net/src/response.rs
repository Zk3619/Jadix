use crate::{request::validate_url, HttpError, HttpErrorKind, HttpHeader};
use std::{fmt, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    /// Final URL as reported by the adapter, including any explicit redirect policy.
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: Arc<[u8]>,
}

impl HttpResponse {
    pub fn validate(&self) -> Result<(), HttpError> {
        if !(100..=599).contains(&self.status) {
            return Err(HttpError::invalid("invalid HTTP status"));
        }
        validate_url(&self.url)?;
        for header in &self.headers {
            header.validate()?;
        }
        Ok(())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    /// Strict UTF-8; no implicit charset guessing or lossy decoding.
    pub fn text(&self) -> Result<&str, HttpError> {
        std::str::from_utf8(&self.body)
            .map_err(|error| HttpError::new(HttpErrorKind::Decode, error.to_string()))
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Fetch-style completion preserves 4xx/5xx; status failure is explicit.
    pub fn error_for_status(self) -> Result<Self, HttpError> {
        if self.status >= 400 {
            return Err(HttpError::new(
                HttpErrorKind::HttpStatus,
                format!("HTTP status {}", self.status),
            ));
        }
        Ok(self)
    }

    #[cfg(feature = "json")]
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, HttpError> {
        serde_json::from_slice(&self.body)
            .map_err(|error| HttpError::new(HttpErrorKind::Decode, error.to_string()))
    }
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}
