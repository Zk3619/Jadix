//! Read-only application URL resources. Replies may arrive on any thread; each
//! request belongs to one document and expires on cancellation or timeout.
use std::{collections::BTreeMap, sync::{Arc, Condvar, Mutex, Weak}, time::{Duration, Instant}};

pub const MAX_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PENDING: usize = 64;
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// Decode a bundle URL before URL normalization can erase traversal segments.
/// The native adapter must additionally canonicalize and contain the file under
/// its asset directory, since a symbolic link can escape a syntactically safe path.
pub fn bundle_path(value: &str) -> Option<std::path::PathBuf> {
    if value.len() > 4096 || value.chars().any(char::is_control) { return None; }
    let raw = value.strip_prefix("asset://bundle/").or_else(|| value.strip_prefix("app://bundle/"))
        .or_else(|| value.strip_prefix("asset:/"))?;
    if raw.contains(['?', '#']) { return None; }
    for (index, byte) in raw.bytes().enumerate() {
        if byte == b'%' && !raw.as_bytes().get(index + 1..index + 3)
            .is_some_and(|escape| escape.iter().all(u8::is_ascii_hexdigit)) { return None; }
    }
    let decoded = percent_encoding::percent_decode_str(raw).decode_utf8().ok()?;
    if decoded.is_empty() || decoded.chars().any(char::is_control) || decoded.contains('\\') || decoded.split('/').any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains(':')) { return None; }
    Some(std::path::PathBuf::from(decoded.as_ref()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRequest { pub url: String, pub method: String }
impl ResourceRequest {
    pub fn authority(&self) -> Option<String> {
        if self.url.len() > 4096 || !matches!(self.method.as_str(), "GET" | "HEAD") { return None; }
        // Check the original path before URL normalization removes dot segments.
        let (_, rest) = self.url.split_once("://")?;
        let path = rest.find('/').map_or("/", |i| &rest[i..]);
        let path = path.split(['?', '#']).next()?;
        let decoded = percent_encoding::percent_decode_str(path).decode_utf8().ok()?;
        if decoded.contains(['\\', '\0']) || decoded.split('/').any(|part| matches!(part, "." | "..")) { return None; }
        let url = url::Url::parse(&self.url).ok()?;
        if url.scheme() != "app" || !url.username().is_empty() || url.password().is_some()
            || url.port().is_some() || url.fragment().is_some() { return None; }
        let host = url.host_str()?;
        valid_authority(host).then(|| host.to_ascii_lowercase())
    }
}
fn valid_authority(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value != "bundle"
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.'))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceResponse { pub status: u16, pub mime: String, pub bytes: Vec<u8> }
impl ResourceResponse {
    pub fn validate(&self) -> bool {
        (200..=599).contains(&self.status) && self.bytes.len() <= MAX_RESOURCE_BYTES
            && !self.mime.is_empty() && self.mime.len() <= 256 && self.mime.contains('/')
            && self.mime.bytes().all(|c| c.is_ascii_graphic() || c == b' ')
    }
}
pub type ResourceResult = Result<ResourceResponse, String>;
type Provider = Arc<dyn Fn(ResourceRequest, ResourceReply) + Send + Sync>;
struct Pending { owner: String, deadline: Instant, answer: Option<ResourceResult> }
#[derive(Default)]
struct State { serial: u64, bytes: usize, pending: BTreeMap<u64, Pending> }
#[derive(Default)]
struct Shared { state: Mutex<State>, changed: Condvar, wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> }
#[derive(Clone)]
pub struct ResourceReply { id: u64, state: Weak<Shared> }
impl ResourceReply {
    pub fn is_active(&self) -> bool {
        self.state.upgrade().is_some_and(|state| state.state.lock().unwrap().pending.get(&self.id)
            .is_some_and(|pending| pending.answer.is_none() && pending.deadline > Instant::now()))
    }
    /// First response wins. Oversized/malformed replies become bounded errors.
    pub fn complete(&self, answer: ResourceResult) -> bool {
        let Some(state) = self.state.upgrade() else { return false; };
        let shared = state;
        let mut state = shared.state.lock().unwrap();
        let Some(pending) = state.pending.get(&self.id) else { return false; };
        if pending.answer.is_some() || pending.deadline <= Instant::now() { return false; }
        let answer = match answer {
            Ok(response) if !response.validate() || state.bytes.saturating_add(response.bytes.len()) > MAX_RESOURCE_BYTES => Err("resource response exceeds budget or is malformed".into()),
            Err(message) => Err(message.chars().take(1024).collect()),
            answer => answer,
        };
        if let Ok(response) = &answer { state.bytes += response.bytes.len(); }
        state.pending.get_mut(&self.id).unwrap().answer = Some(answer);
        shared.changed.notify_all();
        drop(state);
        let wake = shared.wake.lock().unwrap().clone();
        if let Some(wake) = wake { wake(); }
        true
    }
}
/// Blocking transport adapter for platforms whose resource callback runs on a
/// worker thread. Cancellation and disposal wake the waiter immediately.
pub struct ResourceTicket { id: u64, state: Weak<Shared> }
impl ResourceTicket {
    pub fn wait(self) -> ResourceResult {
        let shared = self.state.upgrade().ok_or("resource service disposed")?;
        let mut state = shared.state.lock().unwrap();
        loop {
            let pending = state.pending.get(&self.id).ok_or("resource request cancelled")?;
            let deadline = pending.deadline;
            if pending.answer.is_some() || deadline <= Instant::now() {
                let pending = state.pending.remove(&self.id).unwrap();
                let answer = pending.answer.unwrap_or_else(|| Err("resource request timed out".into()));
                if let Ok(response) = &answer { state.bytes -= response.bytes.len(); }
                return answer;
            }
            state = shared.changed.wait_timeout(state, deadline.saturating_duration_since(Instant::now())).unwrap().0;
        }
    }
}
impl Drop for ResourceTicket {
    fn drop(&mut self) {
        if let Some(shared) = self.state.upgrade() {
            let mut state = shared.state.lock().unwrap();
            if let Some(Pending { answer: Some(Ok(response)), .. }) = state.pending.remove(&self.id) { state.bytes -= response.bytes.len(); }
            shared.changed.notify_all();
        }
    }
}
pub struct ResourceWork { request: ResourceRequest, provider: Provider, reply: ResourceReply }
impl ResourceWork {
    /// Run after releasing the host borrow. Providers should start asynchronous
    /// work and check reply.is_active() before expensive reads or completion.
    pub fn run(self) {
        if !self.reply.is_active() { return; }
        let reply = self.reply.clone();
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.provider)(self.request, self.reply))).is_err() {
            reply.complete(Err("resource provider panicked".into()));
        }
    }
}
#[derive(Default)]
pub struct WebResources { providers: BTreeMap<String, Provider>, state: Arc<Shared>, work: Vec<ResourceWork> }
impl WebResources {
    /// Notify a native main context when a worker completes a response. The
    /// callback runs outside service locks and may inspect the service again.
    pub fn set_completion_waker(&mut self, wake: Option<Arc<dyn Fn() + Send + Sync>>) { *self.state.wake.lock().unwrap() = wake; }
    pub fn has_pending(&self) -> bool { !self.state.state.lock().unwrap().pending.is_empty() }
    pub fn register(&mut self, authority: &str, provider: impl Fn(ResourceRequest, ResourceReply) + Send + Sync + 'static) -> Result<(), &'static str> {
        let authority = authority.to_ascii_lowercase();
        if !valid_authority(&authority) || self.providers.contains_key(&authority) || self.providers.len() >= 64 { return Err("invalid or duplicate resource authority"); }
        self.providers.insert(authority, Arc::new(provider)); Ok(())
    }
    pub fn allows(&self, request: &ResourceRequest) -> bool { request.authority().is_some_and(|host| self.providers.contains_key(&host)) }
    pub fn request(&mut self, owner: &str, request: ResourceRequest) -> Option<u64> {
        if owner.is_empty() || owner.len() > 256 { return None; }
        let provider = self.providers.get(&request.authority()?)?.clone();
        self.work.retain(|work| work.reply.is_active());
        let mut state = self.state.state.lock().unwrap();
        if state.pending.len() >= MAX_PENDING { return None; }
        let id = state.serial.checked_add(1)?; state.serial = id;
        state.pending.insert(id, Pending { owner: owner.into(), deadline: Instant::now() + TIMEOUT, answer: None });
        self.work.push(ResourceWork { request, provider, reply: ResourceReply { id, state: Arc::downgrade(&self.state) } }); Some(id)
    }
    pub fn cancel(&mut self, id: u64) {
        let mut state = self.state.state.lock().unwrap();
        if let Some(Pending { answer: Some(Ok(response)), .. }) = state.pending.remove(&id) { state.bytes -= response.bytes.len(); }
        self.work.retain(|work| work.reply.id != id);
        self.state.changed.notify_all();
    }
    pub fn cancel_owner(&mut self, owner: &str) {
        let ids: Vec<_> = self.state.state.lock().unwrap().pending.iter().filter_map(|(id, pending)| (pending.owner == owner).then_some(*id)).collect();
        for id in ids { self.cancel(id); }
    }
    /// Obtain a worker-side waiter. Consume a request through either this ticket
    /// or take_completed, never both. Waiting does not hold the host lock.
    pub fn ticket(&self, id: u64) -> Option<ResourceTicket> {
        self.state.state.lock().unwrap().pending.contains_key(&id)
            .then(|| ResourceTicket { id, state: Arc::downgrade(&self.state) })
    }
    pub fn take_work(&mut self) -> Vec<ResourceWork> { std::mem::take(&mut self.work) }
    pub fn take_completed(&mut self) -> Vec<(u64, ResourceResult)> {
        let mut state = self.state.state.lock().unwrap(); let now = Instant::now();
        let ids: Vec<_> = state.pending.iter().filter_map(|(id, pending)| (pending.answer.is_some() || pending.deadline <= now).then_some(*id)).collect();
        ids.into_iter().map(|id| {
            let pending = state.pending.remove(&id).unwrap();
            let answer = pending.answer.unwrap_or_else(|| Err("resource request timed out".into()));
            if let Ok(response) = &answer { state.bytes -= response.bytes.len(); }
            (id, answer)
        }).collect()
    }
    pub fn dispose(&mut self) { self.work.clear(); self.providers.clear(); let mut state = self.state.state.lock().unwrap(); state.pending.clear(); state.bytes = 0; self.state.changed.notify_all(); self.state.wake.lock().unwrap().take(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundle_paths_reject_encoded_traversal_and_host_path_syntax() {
        assert_eq!(bundle_path("asset://bundle/images/icon.svg").unwrap(), std::path::PathBuf::from("images/icon.svg"));
        for value in ["app://bundle/%2e%2e/key", "app://bundle/a/%2fsecret", "asset://bundle/a%5cb", "asset://bundle/C:/a", "asset://bundle/a?x=1", "app://other/a"] { assert!(bundle_path(value).is_none(), "{value}"); }
    }
    #[test]
    fn completion_waker_can_reenter_and_disposal_releases_it() {
        let mut resources = WebResources::default();
        let state = Arc::downgrade(&resources.state);
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = count.clone();
        resources.set_completion_waker(Some(Arc::new(move || {
            assert_eq!(state.upgrade().unwrap().state.lock().unwrap().pending.len(), 1);
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        resources.register("local", |_, reply| { assert!(reply.complete(Err("missing".into()))); }).unwrap();
        resources.request("page", ResourceRequest { url: "app://local/a".into(), method: "GET".into() }).unwrap();
        assert!(resources.has_pending());
        for work in resources.take_work() { work.run(); }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(resources.take_completed().len(), 1);
        resources.dispose();
        assert!(!resources.has_pending());
        assert!(resources.state.wake.lock().unwrap().is_none());
    }
    #[test]
    fn resource_reply_cancellation_and_replacement_are_isolated() {
        let replies = Arc::new(Mutex::new(Vec::new())); let capture = replies.clone();
        let mut resources = WebResources::default();
        resources.register("local", move |_, reply| capture.lock().unwrap().push(reply)).unwrap();
        assert!(!resources.allows(&ResourceRequest { url: "app://local/%2e%2e/private".into(), method: "GET".into() }));
        let request = || ResourceRequest { url: "app://local/page?version=2".into(), method: "GET".into() };
        let first = resources.request("document", request()).unwrap();
        for work in resources.take_work() { work.run(); }
        resources.cancel_owner("document");
        let second = resources.request("document", request()).unwrap(); assert_ne!(first, second);
        for work in resources.take_work() { work.run(); }
        let replies = replies.lock().unwrap(); assert!(!replies[0].is_active());
        let response = || Ok(ResourceResponse { status: 200, mime: "text/html; charset=utf-8".into(), bytes: b"hello".to_vec() });
        assert!(!replies[0].complete(response())); assert!(replies[1].complete(response())); assert!(!replies[1].complete(response()));
        assert_eq!(resources.take_completed(), vec![(second, response())]);
        assert!(!replies[1].is_active());
    }
    #[test]
    fn worker_tickets_complete_cancel_and_release_response_budget() {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let capture = replies.clone();
        let mut resources = WebResources::default();
        resources.register("local", move |_, reply| capture.lock().unwrap().push(reply)).unwrap();
        let request = || ResourceRequest { url: "app://local/data".into(), method: "GET".into() };
        let id = resources.request("page", request()).unwrap();
        let ticket = resources.ticket(id).unwrap();
        for work in resources.take_work() { work.run(); }
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || sender.send(ticket.wait()).unwrap());
        assert!(replies.lock().unwrap()[0].complete(Ok(ResourceResponse { status: 200, mime: "text/plain".into(), bytes: vec![1, 2, 3] })));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap().unwrap().bytes, vec![1, 2, 3]);
        worker.join().unwrap();
        assert_eq!(resources.state.state.lock().unwrap().bytes, 0);
        assert!(resources.take_completed().is_empty());

        for dispose in [false, true] {
            let id = resources.request("page", request()).unwrap();
            let ticket = resources.ticket(id).unwrap();
            let (sender, receiver) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || sender.send(ticket.wait()).unwrap());
            if dispose { resources.dispose(); } else { resources.cancel_owner("page"); }
            assert!(receiver.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
            worker.join().unwrap();
            assert!(resources.take_work().is_empty());
        }
    }

    #[test]
    fn abandoned_and_expired_tickets_free_capacity() {
        let mut resources = WebResources::default();
        resources.register("local", |_, _| {}).unwrap();
        let request = || ResourceRequest { url: "app://local/data".into(), method: "GET".into() };
        for _ in 0..MAX_PENDING * 2 {
            let id = resources.request("page", request()).unwrap();
            drop(resources.ticket(id).unwrap());
        }
        assert_eq!(resources.work.len(), 1);
        let id = resources.request("page", request()).unwrap();
        let ticket = resources.ticket(id).unwrap();
        resources.state.state.lock().unwrap().pending.get_mut(&id).unwrap().deadline = Instant::now();
        assert_eq!(ticket.wait().unwrap_err(), "resource request timed out");
        assert!(resources.state.state.lock().unwrap().pending.is_empty());
    }

}
