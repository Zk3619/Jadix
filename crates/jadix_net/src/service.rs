//! Bounded UI-thread command channel for system HTTP adapters.
use std::{cell::RefCell, collections::{BTreeMap, VecDeque}, rc::Rc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FetchRequest {
    pub url: String,
    #[serde(default = "method")] pub method: String,
    #[serde(default)] pub headers: Vec<(String, String)>,
    #[serde(default)] pub body: Vec<u8>,
    #[serde(default = "timeout")] pub timeout_ms: u64,
    #[serde(default = "limit")] pub max_response_bytes: usize,
}
fn method() -> String { "GET".into() }
fn timeout() -> u64 { 30_000 }
fn limit() -> usize { crate::DEFAULT_MAX_RESPONSE_BYTES }
impl FetchRequest {
    pub fn validate(&self) -> Result<(), crate::HttpError> {
        let method = match self.method.as_str() { "GET" => crate::HttpMethod::Get, "POST" => crate::HttpMethod::Post, "PUT" => crate::HttpMethod::Put, "PATCH" => crate::HttpMethod::Patch, "DELETE" => crate::HttpMethod::Delete, "HEAD" => crate::HttpMethod::Head, "OPTIONS" => crate::HttpMethod::Options, _ => return Err(crate::HttpError::invalid("unsupported HTTP method")) };
        if self.url.len() > 32 * 1024 || self.headers.len() > 128 || self.headers.iter().any(|(k,v)| k.len() + v.len() > 16 * 1024) || self.body.len() > 8 * 1024 * 1024 || !(1..=300_000).contains(&self.timeout_ms) || self.max_response_bytes > 32 * 1024 * 1024 { return Err(crate::HttpError::invalid("HTTP request limit exceeded")); }
        let mut request = crate::HttpRequest::new(method, &self.url).timeout(std::time::Duration::from_millis(self.timeout_ms));
        for (key, value) in &self.headers { request = request.header(key, value.as_bytes()); }
        request.validate()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag="type", rename_all="snake_case", deny_unknown_fields)]
pub enum FetchEvent {
    Headers { status: u16, url: String, headers: Vec<(String,String)> },
    Chunk { bytes: Vec<u8> },
    Complete,
    Failed { message: String },
    Cancelled,
}
fn event_bytes(event: &FetchEvent) -> usize { match event { FetchEvent::Chunk { bytes } => bytes.len(), FetchEvent::Headers { url, headers, .. } => url.len() + headers.iter().map(|(k,v)| k.len()+v.len()).sum::<usize>(), FetchEvent::Failed { message } => message.len(), _ => 0 } }
#[derive(Clone, Debug, Serialize)]
#[serde(tag="action", rename_all="snake_case")]
pub enum FetchCommand { Send { id: u32, request: FetchRequest }, Cancel { id: u32 } }
#[derive(Default)]
struct State { next: u32, requests: BTreeMap<u32, Slot>, commands: VecDeque<FetchCommand>, disposed: bool }
struct Slot { limit: usize, received: usize, events: VecDeque<FetchEvent>, terminal: bool, headers: bool }
#[derive(Clone, Default)]
pub struct FetchService(Rc<RefCell<State>>, Rc<RefCell<Option<Rc<dyn Fn()>>>>);
pub struct FetchHandle { id: u32, service: FetchService }
impl FetchService {
    /// Enqueue a host turn without synchronously reentering application code.
    pub fn set_command_waker(&self, wake: Option<Rc<dyn Fn()>>) { *self.1.borrow_mut() = wake; }
    fn wake_commands(&self) { let wake = self.1.borrow().clone(); if let Some(wake) = wake { wake(); } }
    pub fn send(&self, request: FetchRequest) -> Result<FetchHandle, crate::HttpError> {
        request.validate()?; let mut state = self.0.borrow_mut();
        if state.disposed || state.requests.len() >= 64 || state.commands.len() >= 128 || state.commands.iter().map(|c| match c { FetchCommand::Send { request, .. } => request.body.len(), _ => 0 }).sum::<usize>() + request.body.len() > 32 * 1024 * 1024 { return Err(crate::HttpError::invalid("HTTP service disposed or request quota exceeded")); }
        state.next = state.next.checked_add(1).ok_or_else(|| crate::HttpError::invalid("HTTP request identity exhausted"))?;
        let id = state.next;
        state.requests.insert(id, Slot { limit: request.max_response_bytes, received: 0, events: VecDeque::new(), terminal: false, headers: false });
        state.commands.push_back(FetchCommand::Send { id, request }); drop(state); self.wake_commands(); Ok(FetchHandle { id, service: self.clone() })
    }
    pub fn take_commands(&self) -> Vec<FetchCommand> { self.0.borrow_mut().commands.drain(..).collect() }
    /// End a root-owned transport without leaving handles waiting for callbacks.
    pub fn dispose(&self) {
        self.set_command_waker(None);
        let mut state = self.0.borrow_mut();
        state.disposed = true;
        state.commands.clear();
        for slot in state.requests.values_mut() {
            if !slot.terminal {
                slot.terminal = true;
                slot.events.clear();
                slot.events.push_back(FetchEvent::Cancelled);
            }
        }
    }
    pub fn event(&self, id: u32, event: FetchEvent) -> bool {
        let mut state = self.0.borrow_mut();
        let buffered: usize = state.requests.values().flat_map(|s| &s.events).map(event_bytes).sum();
        let over_budget = buffered.saturating_add(event_bytes(&event)) > 16 * 1024 * 1024;
        let Some(slot) = state.requests.get_mut(&id) else { return false; };
        if slot.terminal { return false; }
        let invalid = match &event {
            FetchEvent::Headers { status, url, headers } => slot.headers || !(100..=599).contains(status) || crate::request::validate_url(url).is_err() || headers.len() > 128 || headers.iter().any(|(k,v)| k.len()+v.len()>16*1024 || crate::HttpHeader::new(k, v.as_bytes()).validate().is_err()),
            FetchEvent::Chunk { bytes } => !slot.headers || bytes.len() > 64*1024 || slot.received.saturating_add(bytes.len()) > slot.limit,
            FetchEvent::Failed { message } => message.len() > 4096,
            FetchEvent::Complete => !slot.headers,
            FetchEvent::Cancelled => false,
        };
        if invalid || over_budget || slot.events.len() >= 128 { slot.terminal = true; slot.events.clear(); slot.events.push_back(FetchEvent::Failed { message: "HTTP response exceeded contract or queue limit".into() }); state.commands.push_back(FetchCommand::Cancel { id }); drop(state); self.wake_commands(); return false; }
        if matches!(event, FetchEvent::Headers { .. }) { slot.headers = true; }
        if let FetchEvent::Chunk { bytes } = &event { slot.received += bytes.len(); }
        slot.terminal = matches!(event, FetchEvent::Complete | FetchEvent::Failed { .. } | FetchEvent::Cancelled);
        slot.events.push_back(event); true
    }
}
impl FetchHandle {
    pub fn id(&self) -> u32 { self.id }
    pub fn events(&self) -> Vec<FetchEvent> { self.service.0.borrow_mut().requests.get_mut(&self.id).map(|s| s.events.drain(..).collect()).unwrap_or_default() }
    pub fn cancel(&self) { let mut state = self.service.0.borrow_mut(); let mut changed = false; if let Some(slot) = state.requests.get_mut(&self.id) { if !slot.terminal { slot.terminal = true; slot.events.clear(); slot.events.push_back(FetchEvent::Cancelled); state.commands.push_back(FetchCommand::Cancel { id: self.id }); changed = true; } } drop(state); if changed { self.service.wake_commands(); } }
}
impl Drop for FetchHandle { fn drop(&mut self) { self.cancel(); self.service.0.borrow_mut().requests.remove(&self.id); } }

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn stream_budget_and_stale_events() {
        let service = FetchService::default(); let request: FetchRequest = serde_json::from_str(r#"{"url":"https://example.test","max_response_bytes":2}"#).unwrap();
        let handle = service.send(request).unwrap(); let id = handle.id();
        assert!(service.event(id, FetchEvent::Headers { status:200, url:"https://example.test".into(), headers:vec![] }));
        assert!(!service.event(id, FetchEvent::Chunk { bytes:vec![1,2,3] }));
        assert!(matches!(handle.events()[0], FetchEvent::Failed { .. })); drop(handle);
        assert!(!service.event(id, FetchEvent::Complete));
    }
}
