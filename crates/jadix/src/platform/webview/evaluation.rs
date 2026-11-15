use super::{WebEvaluation, WebEvaluationError};
use crate::app::window_scope::{self, WindowScope};
use jadix_core::web_evaluation::{EvaluationBatch, EvaluationReply, WebEvaluationSession};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

type Execute = dyn Fn(&str, EvaluationReply) -> Result<(), String>;
struct Host {
    ready: Cell<bool>,
    session: RefCell<WebEvaluationSession>,
    execute: Box<Execute>,
    origin: web_time::Instant,
}
thread_local! {
    static HOSTS: RefCell<HashMap<(WindowScope, String), Weak<Host>>> = RefCell::new(HashMap::new());
}
pub(crate) struct EvaluationHost {
    key: (WindowScope, String),
    host: Rc<Host>,
}
impl EvaluationHost {
    pub(crate) fn new(
        id: &str,
        execute: impl Fn(&str, EvaluationReply) -> Result<(), String> + 'static,
    ) -> Self {
        let key = (window_scope::active(), id.to_string());
        let host = Rc::new(Host {
            ready: Cell::new(false),
            session: RefCell::default(),
            execute: Box::new(execute),
            origin: web_time::Instant::now(),
        });
        let old = HOSTS.with(|hosts| hosts.borrow_mut().insert(key.clone(), Rc::downgrade(&host)));
        if let Some(old) = old.and_then(|old| old.upgrade()) {
            cancel(&old);
        }
        Self { key, host }
    }
}
impl Drop for EvaluationHost {
    fn drop(&mut self) {
        HOSTS.with(|hosts| {
            let mut hosts = hosts.borrow_mut();
            if hosts
                .get(&self.key)
                .is_some_and(|old| old.ptr_eq(&Rc::downgrade(&self.host)))
            {
                hosts.remove(&self.key);
            }
        });
        cancel(&self.host);
    }
}
fn find(scope: WindowScope, id: &str) -> Option<Rc<Host>> {
    HOSTS.with(|hosts| {
        hosts
            .borrow()
            .get(&(scope, id.to_string()))
            .and_then(Weak::upgrade)
    })
}
fn dispatch(host: &Host, batch: EvaluationBatch) {
    for work in &batch.work {
        if let Err(error) = (host.execute)(&work.script, work.reply.clone()) {
            work.reply.complete(Err(error));
        }
    }
    batch.wake();
}
fn cancel(host: &Host) {
    host.ready.set(false);
    let batch = host.session.borrow_mut().abort();
    dispatch(host, batch);
}
pub(crate) fn document_changed(scope: WindowScope, id: &str) {
    if let Some(host) = find(scope, id) {
        cancel(&host);
    }
}
pub(crate) fn evaluation_ready(scope: WindowScope, id: &str, ready: bool) {
    if let Some(host) = find(scope, id) {
        if ready {
            host.ready.set(true);
        } else {
            cancel(&host);
        }
    }
}
/// Evaluate an expression or script in the current window's named native WebView.
/// Resolves JSON values and Promises; navigation, teardown, or a 30 s timeout
/// rejects the request. Call on the Jadix UI thread after `WebViewStatus::Ready`.
/// The normal application event loop must keep running while awaiting.
pub fn webview_evaluate(id: &str, script: impl AsRef<str>) -> WebEvaluation {
    let Some(host) = find(window_scope::active(), id) else {
        return WebEvaluation::failed(WebEvaluationError::Unavailable);
    };
    if !host.ready.get() {
        return WebEvaluation::failed(WebEvaluationError::NotReady);
    }
    let (future, batch) = host
        .session
        .borrow_mut()
        .start(script.as_ref(), host.origin.elapsed());
    dispatch(&host, batch);
    crate::app::invalidation::request_frame();
    future
}
pub(crate) fn pump_evaluations() -> bool {
    let hosts: Vec<_> =
        HOSTS.with(|hosts| hosts.borrow().values().filter_map(Weak::upgrade).collect());
    let mut pending = false;
    for host in hosts {
        let batch = host.session.borrow_mut().advance(host.origin.elapsed());
        dispatch(&host, batch);
        pending |= host.session.borrow().is_pending();
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    fn poll(future: &mut WebEvaluation) -> Poll<Result<serde_json::Value, WebEvaluationError>> {
        Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
    }
    fn host(id: &str) -> (EvaluationHost, Rc<RefCell<Vec<EvaluationReply>>>) {
        let replies = Rc::new(RefCell::new(Vec::new()));
        let output = replies.clone();
        let host = EvaluationHost::new(id, move |_, reply| {
            output.borrow_mut().push(reply);
            Ok(())
        });
        (host, replies)
    }

    #[test]
    fn evaluation_hosts_replacement_navigation_and_disposal_reject_stale_replies() {
        let _scope = window_scope::enter(WindowScope::Primary);
        let id = "evaluation-ownership-test";
        assert_eq!(
            poll(&mut webview_evaluate(id, "0")),
            Poll::Ready(Err(WebEvaluationError::Unavailable))
        );
        let (old, replies) = host(id);
        assert_eq!(
            poll(&mut webview_evaluate(id, "0")),
            Poll::Ready(Err(WebEvaluationError::NotReady))
        );
        evaluation_ready(WindowScope::Primary, id, true);
        let mut pending = webview_evaluate(id, "old()");
        let (current, current_replies) = host(id);
        assert_eq!(
            poll(&mut pending),
            Poll::Ready(Err(WebEvaluationError::Aborted))
        );
        drop(old); // Must not unregister its replacement.
        evaluation_ready(WindowScope::Primary, id, true);
        let mut pending = webview_evaluate(id, "current()");
        replies.borrow()[0].complete(Ok(r#"{"status":"ok","value":"stale"}"#.into()));
        assert!(pump_evaluations());
        assert!(poll(&mut pending).is_pending());
        current_replies.borrow()[0].complete(Ok(r#"{"status":"ok","value":42}"#.into()));
        assert!(!pump_evaluations());
        assert_eq!(poll(&mut pending), Poll::Ready(Ok(serde_json::json!(42))));
        let mut navigation = webview_evaluate(id, "never()");
        document_changed(WindowScope::Primary, id);
        assert_eq!(
            poll(&mut navigation),
            Poll::Ready(Err(WebEvaluationError::Aborted))
        );
        assert_eq!(
            poll(&mut webview_evaluate(id, "0")),
            Poll::Ready(Err(WebEvaluationError::NotReady))
        );
        evaluation_ready(WindowScope::Primary, id, true);
        let mut disposal = webview_evaluate(id, "never()");
        drop(current);
        assert_eq!(
            poll(&mut disposal),
            Poll::Ready(Err(WebEvaluationError::Aborted))
        );
        assert_eq!(
            poll(&mut webview_evaluate(id, "0")),
            Poll::Ready(Err(WebEvaluationError::Unavailable))
        );
    }

    #[test]
    fn evaluation_hosts_same_id_in_two_windows_is_isolated() {
        let id = "evaluation-two-window-test";
        let (first, first_replies) = {
            let _scope = window_scope::enter(WindowScope::Primary);
            host(id)
        };
        let (second, second_replies) = {
            let _scope = window_scope::enter(WindowScope::Secondary(991));
            host(id)
        };
        evaluation_ready(WindowScope::Primary, id, true);
        evaluation_ready(WindowScope::Secondary(991), id, true);
        let mut a = {
            let _scope = window_scope::enter(WindowScope::Primary);
            webview_evaluate(id, "primary()")
        };
        let mut b = {
            let _scope = window_scope::enter(WindowScope::Secondary(991));
            webview_evaluate(id, "secondary()")
        };
        first_replies.borrow()[0].complete(Ok(r#"{"status":"ok","value":"first"}"#.into()));
        assert!(pump_evaluations());
        assert_eq!(poll(&mut a), Poll::Ready(Ok(serde_json::json!("first"))));
        assert!(poll(&mut b).is_pending());
        document_changed(WindowScope::Primary, id);
        second_replies.borrow()[0].complete(Ok(r#"{"status":"ok","value":"second"}"#.into()));
        assert!(!pump_evaluations());
        assert_eq!(poll(&mut b), Poll::Ready(Ok(serde_json::json!("second"))));
        drop((first, second));
    }
}
