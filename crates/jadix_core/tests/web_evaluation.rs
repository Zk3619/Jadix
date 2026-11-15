use jadix_core::web_evaluation::*;
use serde_json::json;
use std::{
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

fn poll(future: &mut WebEvaluation) -> Poll<Result<serde_json::Value, WebEvaluationError>> {
    Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
}
#[test]
fn promises_poll_without_reexecuting_the_expression_and_settle_json() {
    let mut session = WebEvaluationSession::default();
    let (mut future, initial) = session.start("incrementOnce()", Duration::ZERO);
    assert!(poll(&mut future).is_pending());
    initial.work[0]
        .reply
        .complete(Ok(r#"{"status":"pending"}"#.into()));
    assert!(session.advance(Duration::ZERO).work.is_empty());
    initial.work[0]
        .reply
        .complete(Ok(r#"{"status":"ok","value":"stale duplicate"}"#.into()));
    assert!(!initial.work[0].reply.is_active());
    assert!(session.advance(Duration::from_millis(24)).work.is_empty());
    let poll_work = session.advance(Duration::from_millis(25));
    assert_eq!(poll_work.work.len(), 1);
    assert!(!poll_work.work[0].script.contains("incrementOnce"));
    poll_work.work[0].reply.complete(Ok(
        r#"{"status":"ok","value":{"answer":[42,null,true]}}"#.into()
    ));
    let completed = session.advance(Duration::from_millis(26));
    assert_eq!(completed.work.len(), 1);
    assert!(completed.work[0].script.starts_with("delete "));
    completed.wake();
    assert_eq!(
        poll(&mut future),
        Poll::Ready(Ok(json!({"answer":[42,null,true]})))
    );
    assert!(!session.is_pending());
}
#[test]
fn timeout_drop_and_document_abort_discard_late_native_callbacks() {
    let mut session = WebEvaluationSession::default();
    let (mut timeout, initial) = session.start("neverSettles()", Duration::ZERO);
    session.advance(TIMEOUT).wake();
    assert_eq!(
        poll(&mut timeout),
        Poll::Ready(Err(WebEvaluationError::Timeout))
    );
    assert!(!initial.work[0].reply.is_active());
    let (mut old, old_work) = session.start("oldDocument()", TIMEOUT);
    session.abort().wake();
    let (mut new, new_work) = session.start("newDocument()", TIMEOUT);
    old_work.work[0]
        .reply
        .complete(Ok(r#"{"status":"ok","value":"stale"}"#.into()));
    assert_eq!(
        poll(&mut old),
        Poll::Ready(Err(WebEvaluationError::Aborted))
    );
    assert!(poll(&mut new).is_pending());
    new_work.work[0]
        .reply
        .complete(Ok(r#"{"status":"ok","value":"current"}"#.into()));
    session.advance(TIMEOUT).wake();
    assert_eq!(poll(&mut new), Poll::Ready(Ok(json!("current"))));
    let (dropped, work) = session.start("dropMe()", TIMEOUT);
    drop(dropped);
    assert_eq!(session.advance(TIMEOUT).work.len(), 1);
    assert!(!work.work[0].reply.is_active());
}
#[test]
fn per_document_and_request_limits_fail_before_dispatch() {
    let mut session = WebEvaluationSession::default();
    let (mut oversized, batch) = session.start(&"x".repeat(MAX_SCRIPT_BYTES + 1), Duration::ZERO);
    assert!(batch.work.is_empty());
    assert_eq!(
        poll(&mut oversized),
        Poll::Ready(Err(WebEvaluationError::ScriptTooLarge))
    );
    let futures: Vec<_> = (0..MAX_PENDING)
        .map(|_| session.start("null", Duration::ZERO).0)
        .collect();
    let (mut overflow, batch) = session.start("null", Duration::ZERO);
    assert!(batch.work.is_empty());
    assert_eq!(
        poll(&mut overflow),
        Poll::Ready(Err(WebEvaluationError::TooManyPending))
    );
    drop(futures);
    session.advance(Duration::ZERO).wake();
    assert!(!session.is_pending());
}
#[test]
fn malformed_non_json_deep_and_large_results_reject() {
    let deep = format!(
        "{{\"status\":\"ok\",\"value\":{}0{}}}",
        "[".repeat(65),
        "]".repeat(65)
    );
    for raw in [
        "null".to_string(),
        r#"{"status":"ok"}"#.into(),
        r#"{"status":"ok","value":NaN}"#.into(),
        deep,
    ] {
        let mut session = WebEvaluationSession::default();
        let (mut future, batch) = session.start("0", Duration::ZERO);
        batch.work[0].reply.complete(Ok(raw));
        session.advance(Duration::ZERO).wake();
        assert_eq!(
            poll(&mut future),
            Poll::Ready(Err(WebEvaluationError::InvalidResult))
        );
    }
    let mut session = WebEvaluationSession::default();
    let (mut future, batch) = session.start("0", Duration::ZERO);
    batch.work[0]
        .reply
        .complete(Ok("x".repeat(MAX_RESULT_BYTES + 4097)));
    session.advance(Duration::ZERO).wake();
    assert_eq!(
        poll(&mut future),
        Poll::Ready(Err(WebEvaluationError::ResultTooLarge))
    );
}
#[test]
fn exceptions_engine_errors_and_session_drop_wake_waiters() {
    for (raw, expected) in [
        (
            Ok(r#"{"status":"error","code":"exception","message":"boom"}"#.into()),
            WebEvaluationError::Exception("boom".into()),
        ),
        (
            Err("engine died".into()),
            WebEvaluationError::Engine("engine died".into()),
        ),
    ] {
        let mut session = WebEvaluationSession::default();
        let (mut future, batch) = session.start("0", Duration::ZERO);
        batch.work[0].reply.complete(raw);
        session.advance(Duration::ZERO).wake();
        assert_eq!(poll(&mut future), Poll::Ready(Err(expected)));
    }
    let mut session = WebEvaluationSession::default();
    let (mut future, _) = session.start("0", Duration::ZERO);
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    assert!(Pin::new(&mut future)
        .poll(&mut Context::from_waker(&waker))
        .is_pending());
    let aborted = session.abort();
    assert_eq!(aborted.work.len(), 1);
    assert!(aborted.work[0].script.starts_with("delete "));
    assert_eq!(
        count.0.load(Ordering::Relaxed),
        0,
        "wake only after releasing the session borrow"
    );
    aborted.wake();
    assert_eq!(count.0.load(Ordering::Relaxed), 1);
    drop(session);
    assert_eq!(
        poll(&mut future),
        Poll::Ready(Err(WebEvaluationError::Aborted))
    );
}
