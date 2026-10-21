use jadix_net::*;
use std::{collections::VecDeque, sync::Arc, time::Duration};

fn response(status: u16, bytes: &[u8]) -> HttpResponse {
    HttpResponse {
        status,
        url: "https://example.test/data".into(),
        headers: Vec::new(),
        body: bytes.into(),
    }
}

fn pending() -> HttpSnapshot {
    let mut snapshot = HttpSnapshot::default();
    snapshot
        .begin_request(1, &HttpRequest::get("https://example.test/data"))
        .unwrap();
    snapshot
}

#[test]
fn request_builders_preserve_bytes_headers_and_policy() {
    let request = HttpRequest::post("https://example.test/upload")
        .header("X-Tag", "first")
        .header("x-tag", "second")
        .body(vec![0, 128, 255])
        .timeout(Duration::from_secs(5))
        .max_response_bytes(1024);
    request.validate().unwrap();
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.headers.len(), 2);
    assert_eq!(request.body.as_ref(), &[0, 128, 255]);
    assert_eq!(request.timeout, Duration::from_secs(5));
    assert_eq!(request.max_response_bytes, 1024);
    assert!(Arc::ptr_eq(&request.body, &request.clone().body));
    for (method, text) in [
        (HttpMethod::Get, "GET"),
        (HttpMethod::Put, "PUT"),
        (HttpMethod::Patch, "PATCH"),
        (HttpMethod::Delete, "DELETE"),
        (HttpMethod::Head, "HEAD"),
        (HttpMethod::Options, "OPTIONS"),
    ] {
        assert_eq!(method.as_str(), text);
    }
}

#[test]
fn invalid_inputs_are_rejected_without_io() {
    for url in [
        "",
        "/relative",
        "file:///data",
        "https://",
        "https:///path",
        "https://host/a b",
        "https://user:password@host/path",
        "http://host\r\nx: y",
    ] {
        assert_eq!(
            HttpRequest::get(url).validate().unwrap_err().kind,
            HttpErrorKind::InvalidInput
        );
    }
    assert!(HttpRequest::get("HTTPS://example.test/data")
        .validate()
        .is_ok());
    assert!(HttpRequest::get("http://[::1]:8080/data")
        .validate()
        .is_ok());
    assert!(HttpRequest::get("https://host")
        .timeout(Duration::ZERO)
        .validate()
        .is_err());
    for header in [
        HttpHeader::new("bad name", "value"),
        HttpHeader::new("", "value"),
        HttpHeader::new("X-Test", "a\r\nb"),
        HttpHeader::new("X-Test", vec![0, 127]),
    ] {
        assert!(header.validate().is_err());
    }
    assert!(HttpHeader::new("X-Test", vec![9, 128, 255])
        .validate()
        .is_ok());
}

#[test]
fn debug_redacts_urls_header_values_and_bodies() {
    let secret = "DO_NOT_LOG_ME";
    let request = HttpRequest::post(format!("https://example.test/?token={secret}"))
        .header("Authorization", secret)
        .body(secret.as_bytes().to_vec());
    assert!(!format!("{request:?}").contains(secret));
    assert!(!format!("{:?}", request.headers).contains(secret));
    let mut response = response(200, secret.as_bytes());
    response.url.push_str(&format!("?token={secret}"));
    assert!(!format!("{response:?}").contains(secret));
}

#[test]
fn stale_future_and_terminal_events_cannot_overwrite_the_slot() {
    let mut snapshot = pending();
    let original = snapshot.clone();
    for request_id in [0, 2, u64::MAX] {
        assert!(!snapshot.apply(HttpEvent::new(
            request_id,
            HttpEventKind::Completed(response(200, b"old"))
        )));
        assert_eq!(snapshot, original);
    }
    snapshot
        .begin_request(2, &HttpRequest::get("https://example.test/new"))
        .unwrap();
    assert!(!snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Failed(HttpError::new(HttpErrorKind::Timeout, "old"))
    )));
    assert!(snapshot.apply(HttpEvent::new(
        2,
        HttpEventKind::Completed(response(200, b"new"))
    )));
    let completed = snapshot.clone();
    assert!(!snapshot.apply(HttpEvent::new(2, HttpEventKind::Cancelled)));
    assert_eq!(snapshot, completed);
    assert_eq!(snapshot.response.unwrap().bytes(), b"new");
}

#[test]
fn invalid_tokens_and_requests_do_not_change_the_snapshot() {
    assert!(HttpCommand::Cancel { request_id: 0 }.validate().is_err());
    assert!(HttpCommand::Send {
        request_id: 0,
        request: HttpRequest::get("https://host")
    }
    .validate()
    .is_err());
    assert!(HttpCommand::Shutdown.validate().is_ok());
    let mut snapshot = pending();
    let previous = snapshot.clone();
    for request_id in [0, 1] {
        assert!(snapshot
            .begin_request(request_id, &HttpRequest::get("https://host"))
            .is_err());
        assert_eq!(snapshot, previous);
    }
    assert!(snapshot
        .begin_request(2, &HttpRequest::get("invalid"))
        .is_err());
    assert_eq!(snapshot, previous);
}

#[test]
fn progress_is_monotonic_and_validates_known_totals() {
    let mut snapshot = pending();
    let progress = TransferProgress {
        transferred: 4,
        total: None,
    };
    assert!(snapshot.apply(HttpEvent::new(1, HttpEventKind::DownloadProgress(progress))));
    for progress in [
        TransferProgress {
            transferred: 3,
            total: None,
        },
        TransferProgress {
            transferred: 5,
            total: Some(4),
        },
    ] {
        assert!(!snapshot.apply(HttpEvent::new(1, HttpEventKind::DownloadProgress(progress))));
    }
    assert_eq!(snapshot.download.transferred, 4);
    assert!(snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::UploadProgress(TransferProgress {
            transferred: 8,
            total: Some(8),
        })
    )));
    assert_eq!(snapshot.upload.transferred, 8);
    snapshot
        .begin_request(2, &HttpRequest::get("https://host"))
        .unwrap();
    assert_eq!(snapshot.upload, TransferProgress::default());
    assert_eq!(snapshot.download, TransferProgress::default());
}

#[test]
fn oversized_or_invalid_responses_fail_closed() {
    let mut snapshot = pending();
    snapshot.max_response_bytes = 2;
    assert!(snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Completed(response(200, b"big"))
    )));
    assert_eq!(snapshot.state, HttpState::Failed);
    assert_eq!(snapshot.error.unwrap().kind, HttpErrorKind::BodyTooLarge);
    assert!(snapshot.response.is_none());
    let mut snapshot = pending();
    assert!(snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Completed(response(99, b""))
    )));
    assert_eq!(snapshot.state, HttpState::Failed);
    assert!(snapshot.response.is_none());
}

#[test]
fn http_status_is_not_a_transport_failure_and_text_is_strict_utf8() {
    let mut snapshot = pending();
    snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Completed(response(404, b"missing")),
    ));
    assert_eq!(snapshot.state, HttpState::Completed);
    let response = snapshot.response.unwrap();
    assert!(!response.is_success());
    assert_eq!(response.text().unwrap(), "missing");
    assert_eq!(
        response.error_for_status().unwrap_err().kind,
        HttpErrorKind::HttpStatus
    );
    assert_eq!(
        self::response(200, &[255]).text().unwrap_err().kind,
        HttpErrorKind::Decode
    );
}

#[test]
fn cancellation_is_terminal_and_new_requests_clear_errors() {
    let mut snapshot = pending();
    snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Failed(HttpError::new(HttpErrorKind::Cancelled, "cancelled")),
    ));
    assert_eq!(snapshot.state, HttpState::Cancelled);
    assert!(!snapshot.apply(HttpEvent::new(
        1,
        HttpEventKind::Completed(response(200, b"late"))
    )));
    snapshot
        .begin_request(2, &HttpRequest::get("https://host"))
        .unwrap();
    assert_eq!(snapshot.state, HttpState::Pending);
    assert!(snapshot.error.is_none());
    assert!(snapshot.response.is_none());
}

#[test]
fn external_adapters_can_implement_the_contract_with_only_std() {
    struct Adapter(VecDeque<HttpEvent>);
    impl HttpBackend for Adapter {
        fn dispatch(&mut self, command: HttpCommand) -> Result<(), HttpError> {
            let HttpCommand::Send {
                request_id,
                request,
            } = command
            else {
                return Ok(());
            };
            request.validate()?;
            self.0.push_back(HttpEvent::new(
                request_id,
                HttpEventKind::Completed(response(200, b"fixture")),
            ));
            Ok(())
        }
        fn poll_event(&mut self) -> Option<HttpEvent> {
            self.0.pop_front()
        }
    }
    let mut backend: Box<dyn HttpBackend> = Box::new(Adapter(VecDeque::new()));
    backend
        .dispatch(HttpCommand::Send {
            request_id: 1,
            request: HttpRequest::get("https://host"),
        })
        .unwrap();
    let mut snapshot = pending();
    assert!(snapshot.apply(backend.poll_event().unwrap()));
    assert_eq!(snapshot.state, HttpState::Completed);
    assert!(backend.poll_event().is_none());
}

#[cfg(feature = "json")]
#[test]
fn optional_json_uses_the_same_request_and_response_contracts() {
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Payload {
        name: String,
    }
    let payload = Payload {
        name: "Jadix".into(),
    };
    let request = HttpRequest::post("https://host")
        .header("content-type", "old")
        .json(&payload)
        .unwrap();
    assert_eq!(request.headers.len(), 1);
    assert_eq!(request.headers[0].value, b"application/json");
    assert_eq!(
        response(200, &request.body).json::<Payload>().unwrap(),
        payload
    );
    assert_eq!(
        response(200, b"not json")
            .json::<Payload>()
            .unwrap_err()
            .kind,
        HttpErrorKind::Decode
    );
}
