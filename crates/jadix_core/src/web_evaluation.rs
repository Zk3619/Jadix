//! Engine-independent, bounded JSON evaluation with explicit document ownership.
//! Hosts execute returned scripts and feed their native completion into `reply`.

use serde_json::Value;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    task::{Context, Poll, Waker},
    time::Duration,
};

pub const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 1024 * 1024;
pub const MAX_PENDING: usize = 64;
pub const TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebEvaluationError {
    Unavailable,
    NotReady,
    TooManyPending,
    ScriptTooLarge,
    ResultTooLarge,
    InvalidResult,
    Exception(String),
    Engine(String),
    Timeout,
    Aborted,
}
impl std::fmt::Display for WebEvaluationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WebEvaluationError {}

type Answer = Result<Value, WebEvaluationError>;
#[derive(Default)]
struct FutureState {
    answer: Option<Answer>,
    waker: Option<Waker>,
}

/// Awaitable result. Dropping it cancels the request at the next host pump.
pub struct WebEvaluation {
    state: Arc<Mutex<FutureState>>,
}
impl WebEvaluation {
    pub fn failed(error: WebEvaluationError) -> Self {
        Self {
            state: Arc::new(Mutex::new(FutureState {
                answer: Some(Err(error)),
                waker: None,
            })),
        }
    }
}
impl Future for WebEvaluation {
    type Output = Answer;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Answer> {
        let mut state = self.state.lock().unwrap();
        if let Some(answer) = state.answer.take() {
            Poll::Ready(answer)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

type NativeAnswer = Result<String, String>;
type ReplyState = Mutex<Option<NativeAnswer>>;
/// A native callback capability, separate from page IPC. Late replies are ignored.
#[derive(Clone, Default)]
pub struct EvaluationReply {
    state: Weak<ReplyState>,
}
impl EvaluationReply {
    pub fn is_active(&self) -> bool {
        self.state.strong_count() != 0
    }
    pub fn complete(&self, answer: NativeAnswer) {
        if let Some(state) = self.state.upgrade() {
            // The wrapper returns an object, not a doubly encoded JSON string.
            let answer = match answer {
                Ok(text) if text.len() > MAX_RESULT_BYTES + 4096 => {
                    Ok(r#"{"status":"error","code":"too-large"}"#.into())
                }
                other => other,
            };
            let mut slot = state.lock().unwrap();
            if slot.is_none() {
                *slot = Some(answer);
            }
        }
    }
}
pub struct EvaluationWork {
    pub script: String,
    pub reply: EvaluationReply,
}
#[derive(Default)]
pub struct EvaluationBatch {
    pub work: Vec<EvaluationWork>,
    wakers: Vec<Waker>,
}
impl EvaluationBatch {
    /// Call after releasing the session/host borrow; custom wakers may reenter.
    pub fn wake(self) {
        for waker in self.wakers {
            waker.wake();
        }
    }
}
struct Pending {
    key: String,
    future: Weak<Mutex<FutureState>>,
    reply: Arc<ReplyState>,
    deadline: Duration,
    next_poll: Option<Duration>,
}
/// One document lifetime. Call `abort` before navigation or destruction.
#[derive(Default)]
pub struct WebEvaluationSession {
    pending: BTreeMap<u64, Pending>,
}
impl WebEvaluationSession {
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn start(&mut self, script: &str, now: Duration) -> (WebEvaluation, EvaluationBatch) {
        let error = if script.len() > MAX_SCRIPT_BYTES {
            Some(WebEvaluationError::ScriptTooLarge)
        } else if self.pending.len() >= MAX_PENDING {
            Some(WebEvaluationError::TooManyPending)
        } else {
            None
        };
        if let Some(error) = error {
            return (WebEvaluation::failed(error), EvaluationBatch::default());
        }
        let Ok(serial) =
            NEXT_REQUEST.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
        else {
            return (
                WebEvaluation::failed(WebEvaluationError::Unavailable),
                EvaluationBatch::default(),
            );
        };
        let key = format!("__jadix_evaluation_{serial}");
        let state = Arc::new(Mutex::new(FutureState::default()));
        let reply = Arc::new(Mutex::new(None));
        let work = EvaluationWork {
            script: include_str!("web_evaluation.js")
                .replace("__JADIX_TOKEN__", &serde_json::to_string(&key).unwrap())
                .replace("__JADIX_SOURCE__", &serde_json::to_string(script).unwrap()),
            reply: EvaluationReply {
                state: Arc::downgrade(&reply),
            },
        };
        self.pending.insert(
            serial,
            Pending {
                key,
                future: Arc::downgrade(&state),
                reply,
                deadline: now.saturating_add(TIMEOUT),
                next_poll: None,
            },
        );
        (
            WebEvaluation { state },
            EvaluationBatch {
                work: vec![work],
                wakers: Vec::new(),
            },
        )
    }
    pub fn advance(&mut self, now: Duration) -> EvaluationBatch {
        let mut batch = EvaluationBatch::default();
        self.pending.retain(|_, pending| {
            let response = pending.reply.lock().unwrap().take();
            let answer = if pending.future.strong_count() == 0 { Some(Err(WebEvaluationError::Aborted)) }
                else if now >= pending.deadline { Some(Err(WebEvaluationError::Timeout)) }
                else if let Some(answer) = response {
                    match answer {
                        Err(error) => Some(Err(WebEvaluationError::Engine(error))),
                        Ok(raw) => match decode(&raw) {
                            Ok(None) => {
                                // Each native execution owns one reply slot. A late
                                // callback from an earlier poll cannot settle this one.
                                pending.reply = Arc::new(Mutex::new(None));
                                pending.next_poll = Some(now.saturating_add(POLL_INTERVAL));
                                None
                            }
                            Ok(Some(value)) => Some(Ok(value)),
                            Err(error) => Some(Err(error)),
                        }
                    }
                } else { None };
            if let Some(answer) = answer {
                settle(pending, answer, &mut batch);
                batch.work.push(EvaluationWork { script: format!("delete globalThis[{}]", serde_json::to_string(&pending.key).unwrap()), reply: EvaluationReply::default() });
                false
            } else {
                if pending.next_poll.is_some_and(|deadline| deadline <= now) {
                    pending.next_poll = None;
                    let key = serde_json::to_string(&pending.key).unwrap();
                    batch.work.push(EvaluationWork { script: format!("(()=>{{const c=globalThis[{key}];return c?c.result:{{status:'error',code:'aborted'}};}})()"), reply: EvaluationReply { state: Arc::downgrade(&pending.reply) } });
                }
                true
            }
        });
        batch
    }
    pub fn abort(&mut self) -> EvaluationBatch {
        let mut batch = EvaluationBatch::default();
        for (_, pending) in std::mem::take(&mut self.pending) {
            settle(&pending, Err(WebEvaluationError::Aborted), &mut batch);
            batch.work.push(EvaluationWork {
                script: format!(
                    "delete globalThis[{}]",
                    serde_json::to_string(&pending.key).unwrap()
                ),
                reply: EvaluationReply::default(),
            });
        }
        batch
    }
}
impl Drop for WebEvaluationSession {
    fn drop(&mut self) {
        self.abort().wake();
    }
}
fn settle(pending: &Pending, answer: Answer, batch: &mut EvaluationBatch) {
    if let Some(state) = pending.future.upgrade() {
        let mut state = state.lock().unwrap();
        state.answer = Some(answer);
        if let Some(waker) = state.waker.take() {
            batch.wakers.push(waker);
        }
    }
}
fn decode(raw: &str) -> Result<Option<Value>, WebEvaluationError> {
    if raw.len() > MAX_RESULT_BYTES + 4096 {
        return Err(WebEvaluationError::ResultTooLarge);
    }
    let value: Value = serde_json::from_str(raw).map_err(|_| WebEvaluationError::InvalidResult)?;
    match value.get("status").and_then(Value::as_str) {
        Some("pending") => Ok(None),
        Some("ok") => {
            let value = value
                .get("value")
                .ok_or(WebEvaluationError::InvalidResult)?;
            fn depth(value: &Value, level: usize) -> bool {
                level <= 64
                    && match value {
                        Value::Array(items) => items.iter().all(|v| depth(v, level + 1)),
                        Value::Object(items) => items.values().all(|v| depth(v, level + 1)),
                        _ => true,
                    }
            }
            if !depth(value, 0) {
                return Err(WebEvaluationError::InvalidResult);
            }
            if serde_json::to_vec(value).unwrap().len() > MAX_RESULT_BYTES {
                return Err(WebEvaluationError::ResultTooLarge);
            }
            Ok(Some(value.clone()))
        }
        Some("error") => Err(match value.get("code").and_then(Value::as_str) {
            Some("aborted") => WebEvaluationError::Aborted,
            Some("bridge-error") => WebEvaluationError::Engine(
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("WebView bridge failed")
                    .chars()
                    .take(512)
                    .collect(),
            ),
            Some("too-large") => WebEvaluationError::ResultTooLarge,
            Some("invalid-result") => WebEvaluationError::InvalidResult,
            Some("exception") => WebEvaluationError::Exception(
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(512)
                    .collect(),
            ),
            _ => WebEvaluationError::InvalidResult,
        }),
        _ => Err(WebEvaluationError::InvalidResult),
    }
}
