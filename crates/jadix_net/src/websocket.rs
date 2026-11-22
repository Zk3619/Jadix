//! Bounded WebSocket contracts. A handle owns its connection; dropping it cancels it.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
pub const MAX_CONNECTIONS: usize = 8;
pub const MAX_MESSAGES: usize = 8;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
fn timeout() -> u64 {
    10_000
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SocketRequest {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub protocols: Vec<String>,
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}
impl SocketRequest {
    pub fn validate(&self) -> Result<(), String> {
        let url = url::Url::parse(&self.url).map_err(|_| "invalid WebSocket URL")?;
        if self.url.len() > 8192
            || !matches!(url.scheme(), "ws" | "wss")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || self.url.chars().any(char::is_control)
        {
            return Err("WebSocket requires a ws/wss URL without credentials or fragment".into());
        }
        if !(100..=120_000).contains(&self.timeout_ms)
            || self.headers.len() > 128
            || self
                .headers
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>()
                > 65536
        {
            return Err("WebSocket request budget exceeded".into());
        }
        for (name, value) in &self.headers {
            let lower = name.to_ascii_lowercase();
            if !token(name)
                || value.bytes().any(|b| b < 32 || b == 127)
                || lower.starts_with("sec-websocket-")
                || matches!(
                    lower.as_str(),
                    "host"
                        | "connection"
                        | "upgrade"
                        | "content-length"
                        | "transfer-encoding"
                        | "proxy-authorization"
                )
            {
                return Err("invalid or reserved WebSocket header".into());
            }
        }
        let mut protocols = std::collections::BTreeSet::new();
        if self.protocols.len() > 16
            || self
                .protocols
                .iter()
                .any(|p| p.len() > 128 || !token(p) || !protocols.insert(p))
        {
            return Err("invalid WebSocket protocols".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SocketMessage {
    Text(String),
    Binary(Vec<u8>),
}
impl SocketMessage {
    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Binary(b) => b.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SocketEvent {
    Connected { protocol: Option<String> },
    Message { message: SocketMessage },
    Closed { code: Option<u16>, reason: String },
    Failed { message: String },
}
impl SocketEvent {
    fn bytes(&self) -> usize {
        match self {
            Self::Message { message } => message.len(),
            _ => 0,
        }
    }
    fn terminal(&self) -> bool {
        matches!(self, Self::Closed { .. } | Self::Failed { .. })
    }
}
pub fn validate_close(code: u16, reason: &str) -> Result<(), String> {
    if !(matches!(code,1000..=1003|1007..=1014|3000..=4999)) || reason.len() > 123 {
        Err("invalid WebSocket close code or reason".into())
    } else {
        Ok(())
    }
}
pub(crate) enum Command {
    Connect {
        id: u32,
        request: SocketRequest,
        cancelled: Arc<AtomicBool>,
    },
    Send {
        id: u32,
        message: SocketMessage,
    },
    Close {
        id: u32,
        code: u16,
        reason: String,
    },
}
impl Command {
    fn id(&self) -> u32 {
        match self {
            Self::Connect { id, .. } | Self::Send { id, .. } | Self::Close { id, .. } => *id,
        }
    }
}
struct Slot {
    events: VecDeque<SocketEvent>,
    bytes: usize,
    outgoing: usize,
    outgoing_bytes: usize,
    terminal: bool,
    closing: bool,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct State {
    next: u32,
    slots: BTreeMap<u32, Slot>,
    commands: VecDeque<Command>,
    disposed: bool,
}
struct Inner {
    state: Mutex<State>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}
#[derive(Clone)]
pub struct SocketService(Arc<Inner>);
pub struct SocketHandle {
    id: u32,
    service: SocketService,
}
impl Default for SocketService {
    fn default() -> Self {
        Self::new()
    }
}
impl SocketService {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            state: Mutex::new(State::default()),
            wake: Mutex::new(None),
        }))
    }
    pub fn set_command_waker(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        *self.0.wake.lock().unwrap() = wake;
    }
    fn wake(&self) {
        let wake = self.0.wake.lock().unwrap().clone();
        if let Some(w) = wake {
            w();
        }
    }
    pub fn connect(&self, request: SocketRequest) -> Result<SocketHandle, String> {
        request.validate()?;
        let mut state = self.0.state.lock().unwrap();
        if state.disposed || state.slots.len() >= MAX_CONNECTIONS || state.commands.len() >= 64 {
            return Err("WebSocket service is closed or full".into());
        }
        let id = state.next.checked_add(1).ok_or("WebSocket ID exhausted")?;
        state.next = id;
        let cancelled = Arc::new(AtomicBool::new(false));
        state.slots.insert(
            id,
            Slot {
                events: VecDeque::new(),
                bytes: 0,
                outgoing: 0,
                outgoing_bytes: 0,
                terminal: false,
                closing: false,
                cancelled: cancelled.clone(),
            },
        );
        state.commands.push_back(Command::Connect {
            id,
            request,
            cancelled,
        });
        drop(state);
        self.wake();
        Ok(SocketHandle {
            id,
            service: self.clone(),
        })
    }
    pub fn dispose(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.disposed = true;
        state.commands.clear();
        for slot in state.slots.values_mut() {
            slot.cancelled.store(true, Ordering::Release);
            slot.events.clear();
            slot.bytes = 0;
            slot.events.push_back(SocketEvent::Closed {
                code: None,
                reason: "service disposed".into(),
            });
            slot.terminal = true;
        }
        drop(state);
        self.wake();
    }
    pub(crate) fn disposed(&self) -> bool {
        self.0.state.lock().unwrap().disposed
    }
    pub(crate) fn take_commands(&self) -> Vec<Command> {
        self.0.state.lock().unwrap().commands.drain(..).collect()
    }
    pub(crate) fn acknowledge(&self, id: u32, bytes: usize) {
        if let Some(s) = self.0.state.lock().unwrap().slots.get_mut(&id) {
            s.outgoing = s.outgoing.saturating_sub(1);
            s.outgoing_bytes = s.outgoing_bytes.saturating_sub(bytes);
        }
    }
    /// Full message queues exert backpressure; terminal state is always retained.
    pub fn event(&self, id: u32, event: SocketEvent) -> bool {
        let mut state = self.0.state.lock().unwrap();
        let Some(s) = state.slots.get_mut(&id) else {
            return false;
        };
        if s.terminal {
            return false;
        }
        let bytes = event.bytes();
        if !event.terminal() && (s.events.len() >= MAX_MESSAGES || s.bytes + bytes > MAX_BYTES) {
            return false;
        }
        s.terminal = event.terminal();
        s.bytes += bytes;
        s.events.push_back(event);
        true
    }
}
impl SocketHandle {
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn events(&self) -> Vec<SocketEvent> {
        let mut state = self.service.0.state.lock().unwrap();
        let Some(s) = state.slots.get_mut(&self.id) else {
            return vec![];
        };
        s.bytes = 0;
        let events = s.events.drain(..).collect();
        drop(state);
        self.service.wake();
        events
    }
    pub fn send(&self, message: SocketMessage) -> Result<(), String> {
        let bytes = message.len();
        let mut state = self.service.0.state.lock().unwrap();
        if state.commands.len() >= 64 {
            return Err("WebSocket command queue full".into());
        }
        let s = state.slots.get_mut(&self.id).ok_or("stale WebSocket")?;
        if s.terminal
            || s.closing
            || s.outgoing >= MAX_MESSAGES
            || s.outgoing_bytes + bytes > MAX_BYTES
        {
            return Err("WebSocket closed or send budget exceeded".into());
        }
        s.outgoing += 1;
        s.outgoing_bytes += bytes;
        state.commands.push_back(Command::Send {
            id: self.id,
            message,
        });
        drop(state);
        self.service.wake();
        Ok(())
    }
    pub fn close(&self, code: u16, reason: String) -> Result<(), String> {
        validate_close(code, &reason)?;
        let mut state = self.service.0.state.lock().unwrap();
        if state.commands.len() >= 64 {
            return Err("WebSocket command queue full".into());
        }
        let s = state.slots.get_mut(&self.id).ok_or("stale WebSocket")?;
        if s.terminal || s.closing {
            return Ok(());
        }
        s.closing = true;
        state.commands.push_back(Command::Close {
            id: self.id,
            code,
            reason,
        });
        drop(state);
        self.service.wake();
        Ok(())
    }
}
impl Drop for SocketHandle {
    fn drop(&mut self) {
        let mut state = self.service.0.state.lock().unwrap();
        if let Some(s) = state.slots.remove(&self.id) {
            s.cancelled.store(true, Ordering::Release);
        }
        state.commands.retain(|c| c.id() != self.id);
        drop(state);
        self.service.wake();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> SocketRequest {
        SocketRequest {
            url: "ws://127.0.0.1/".into(),
            headers: vec![],
            protocols: vec![],
            timeout_ms: 1000,
        }
    }
    #[test]
    fn validation_and_queue_lifetimes() {
        let s = SocketService::new();
        let h = s.connect(request()).unwrap();
        for _ in 0..8 {
            h.send(SocketMessage::Text("a".into())).unwrap();
        }
        assert!(h.send(SocketMessage::Text("b".into())).is_err());
        s.acknowledge(h.id(), 1);
        h.send(SocketMessage::Text("c".into())).unwrap();
        for _ in 0..8 {
            assert!(s.event(
                h.id(),
                SocketEvent::Message {
                    message: SocketMessage::Binary(vec![0; MAX_BYTES / 8])
                }
            ));
        }
        assert!(!s.event(
            h.id(),
            SocketEvent::Message {
                message: SocketMessage::Text("x".into())
            }
        ));
        assert!(s.event(
            h.id(),
            SocketEvent::Closed {
                code: Some(1000),
                reason: String::new()
            }
        ));
        assert_eq!(h.events().len(), 9);
        let id = h.id();
        drop(h);
        assert!(!s.event(
            id,
            SocketEvent::Closed {
                code: None,
                reason: String::new()
            }
        ));
        assert!(s.take_commands().is_empty());
    }
    #[test]
    fn reject_credentials_reserved_headers_and_bad_close() {
        for url in [
            "https://example.org",
            "ws://u:p@example.org",
            "ws://example.org/#x",
            "file:///tmp/a",
        ] {
            let mut r = request();
            r.url = url.into();
            assert!(r.validate().is_err());
        }
        for header in [
            "Sec-WebSocket-Key",
            "Host",
            "Connection",
            "Proxy-Authorization",
        ] {
            let mut r = request();
            r.headers.push((header.into(), "x".into()));
            assert!(r.validate().is_err());
        }
        assert!(validate_close(1006, "").is_err());
        assert!(validate_close(1000, &"a".repeat(124)).is_err());
        let mut r = request();
        r.headers
            .push(("Authorization".into(), "Bearer test".into()));
        r.protocols = vec!["chat".into()];
        assert!(r.validate().is_ok());
    }
}
