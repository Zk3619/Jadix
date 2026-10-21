use crate::{HttpError, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_TIMEOUT};
use std::{fmt, sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// A list of headers preserves duplicates and non-UTF-8 response values.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpHeader {
    pub name: String,
    pub value: Vec<u8>,
}

impl HttpHeader {
    pub fn new(name: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub fn validate(&self) -> Result<(), HttpError> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
        {
            return Err(HttpError::invalid("invalid HTTP header name"));
        }
        if self
            .value
            .iter()
            .any(|byte| (*byte < 32 && *byte != b'\t') || *byte == 127)
        {
            return Err(HttpError::invalid("invalid HTTP header value"));
        }
        Ok(())
    }
}

impl fmt::Debug for HttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpHeader")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub body: Arc<[u8]>,
    /// Total time through receipt of the entire response, not just its headers.
    pub timeout: Duration,
    /// Bound delivered/decompressed bytes, including when Content-Length is absent.
    pub max_response_bytes: usize,
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Arc::from([]),
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Get, url)
    }
    pub fn post(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Post, url)
    }

    /// Append, rather than replace, to preserve repeated header semantics.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        self.headers.push(HttpHeader::new(name, value));
        self
    }

    pub fn body(mut self, body: impl Into<Arc<[u8]>>) -> Self {
        self.body = body.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn max_response_bytes(mut self, limit: usize) -> Self {
        self.max_response_bytes = limit;
        self
    }

    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize + ?Sized>(mut self, value: &T) -> Result<Self, HttpError> {
        self.body = serde_json::to_vec(value)
            .map_err(|error| HttpError::invalid(error.to_string()))?
            .into();
        self.headers
            .retain(|header| !header.name.eq_ignore_ascii_case("content-type"));
        self.headers.push(HttpHeader::new(
            "Content-Type",
            b"application/json".to_vec(),
        ));
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), HttpError> {
        validate_url(&self.url)?;
        if self.timeout.is_zero() {
            return Err(HttpError::invalid("HTTP timeout must be positive"));
        }
        for header in &self.headers {
            header.validate()?;
        }
        Ok(())
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish_non_exhaustive()
    }
}

/// Only admission checks, not a URL parser. The transport must perform full parsing.
pub(crate) fn validate_url(url: &str) -> Result<(), HttpError> {
    let invalid = || HttpError::invalid("an absolute http(s) URL without credentials is required");
    let (scheme, rest) = url.split_once("://").ok_or_else(invalid)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || authority.is_empty()
        || authority.contains('@')
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(invalid());
    }
    Ok(())
}
