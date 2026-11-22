//! System WebView2 leaves. Evaluation and resource ownership live in the core.
use super::*;
use jadix_core::{hosted::HostedContent, web_evaluation::{EvaluationBatch, MAX_RESULT_BYTES}, web_resource::*};
use std::{io::Read, num::NonZeroIsize, path::PathBuf, rc::Rc, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender}}};
use wry::{WebView, WebViewBuilder, PageLoadEvent, RequestAsyncResponder};

pub(super) const TIMER: usize = 0x4a4459;
struct Parent(HWND);
impl rwh_06::HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<rwh_06::WindowHandle<'_>, rwh_06::HandleError> {
        let handle = rwh_06::Win32WindowHandle::new(NonZeroIsize::new(self.0 as isize).ok_or(rwh_06::HandleError::Unavailable)?);
        // The leaf is dropped before its parent HWND is destroyed.
        Ok(unsafe { rwh_06::WindowHandle::borrow_raw(rwh_06::RawWindowHandle::Win32(handle)) })
    }
}
struct Request {
    owner: String,
    alive: Arc<AtomicBool>,
    value: ResourceRequest,
    responder: RequestAsyncResponder,
}
struct Event { node: NodeId, alive: Arc<AtomicBool>, name: String, data: serde_json::Value }
pub(super) struct Runtime {
    pub evaluations: RefCell<crate::native_web::NativeWeb>,
    pub resources: RefCell<WebResources>,
    events: RefCell<VecDeque<Event>>,
    event_bytes: Cell<usize>,
    overflow: Cell<bool>,
    completed: Receiver<(u64, Result<String, String>)>,
    completion: SyncSender<(u64, Result<String, String>)>,
    incoming: Arc<Mutex<VecDeque<Request>>>,
    pending: RefCell<BTreeMap<u64, Request>>,
    asset_root: Arc<Mutex<PathBuf>>,
    files: Option<SyncSender<Request>>,
    worker: Option<std::thread::JoinHandle<()>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    window: usize,
}
impl Runtime {
    pub fn new(host: &WindowsHost) -> Result<Rc<Self>, String> {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(host.waker());
        let mut resources = WebResources::default();
        resources.set_completion_waker(Some(wake.clone()));
        let (completion, completed) = mpsc::sync_channel(128);
        let (files, reads) = mpsc::sync_channel::<Request>(MAX_PENDING);
        let asset_root = Arc::new(Mutex::new(std::env::current_dir().and_then(|p| p.canonicalize()).map_err(|e| e.to_string())?));
        let root = asset_root.clone();
        let worker = std::thread::Builder::new().name("jadix-web-assets".into()).spawn(move || {
            while let Ok(request) = reads.recv() {
                let path = root.lock().unwrap().clone();
                let result = read_bundle(&path, &request);
                finish(request, result);
            }
        }).map_err(|e| e.to_string())?;
        Ok(Rc::new(Self { evaluations: Default::default(), resources: RefCell::new(resources), events: Default::default(), event_bytes: Cell::new(0), overflow: Cell::new(false), completed, completion, incoming: Default::default(), pending: Default::default(), asset_root, files: Some(files), worker: Some(worker), wake, window: host.window as usize }))
    }
    pub fn set_asset_root(&self, path: PathBuf) { *self.asset_root.lock().unwrap() = path; }
    pub fn asset_root(&self) -> Arc<Mutex<PathBuf>> { self.asset_root.clone() }
    fn allowed(&self, spec: &HostedContent, url: &str) -> bool {
        let url = canonical_url(url);
        spec.allows_web_navigation(&url, self.resources.borrow().allows(&ResourceRequest { url: url.clone(), method: "GET".into() }))
    }
    fn event(&self, node: NodeId, alive: &Arc<AtomicBool>, name: &str, data: serde_json::Value) {
        if !alive.load(Ordering::Acquire) { return; }
        let bytes = name.len() + data.to_string().len() + 64;
        if bytes > 16 * 1024 + 128 || self.events.borrow().len() >= MAX_EVENTS || bytes > MAX_EVENT_BYTES.saturating_sub(self.event_bytes.get()) {
            self.overflow.set(true); (self.wake)(); return;
        }
        self.event_bytes.set(self.event_bytes.get() + bytes);
        self.events.borrow_mut().push_back(Event { node, alive: alive.clone(), name: name.into(), data });
        (self.wake)();
    }
    pub fn drain_events(&self, context: &WindowContext) {
        if self.overflow.replace(false) { context.failed.set(true); }
        self.event_bytes.set(0);
        for event in self.events.borrow_mut().drain(..) {
            if event.alive.load(Ordering::Acquire) {
                context.push(RawEvent::Hosted { control: event.node, event: event.name, data: event.data });
            }
        }
    }
    fn cancel(&self, owner: &str) {
        self.resources.borrow_mut().cancel_owner(owner);
        let ids: Vec<_> = self.pending.borrow().iter().filter_map(|(id, r)| (r.owner == owner).then_some(*id)).collect();
        for id in ids { if let Some(request) = self.pending.borrow_mut().remove(&id) { finish(request, Err("document cancelled".into())); } }
    }
    pub fn pump(&self, views: &BTreeMap<NodeId, Leaf>) -> Vec<EvaluationBatch> {
        while let Ok((serial, result)) = self.completed.try_recv() { self.evaluations.borrow_mut().complete(serial, result); }
        let incoming = std::mem::take(&mut *self.incoming.lock().unwrap());
        for request in incoming {
            if !request.alive.load(Ordering::Acquire) { finish(request, Err("document cancelled".into())); }
            else if bundle_path(&request.value.url).is_some() {
                if let Some(files) = &self.files { if let Err(error) = files.try_send(request) { let (mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request)) = error; finish(request, Err("asset worker busy".into())); } }
                else { finish(request, Err("host disposed".into())); }
            } else {
                let id = self.resources.borrow_mut().request(&request.owner, request.value.clone());
                if let Some(id) = id { self.pending.borrow_mut().insert(id, request); }
                else { finish(request, Err("resource unavailable".into())); }
            }
        }
        for (id, result) in self.resources.borrow_mut().take_completed() {
            if let Some(request) = self.pending.borrow_mut().remove(&id) { finish(request, result); }
        }
        let (work, wake) = self.evaluations.borrow_mut().pump();
        for (node, serial, script) in work {
            let result = if let Some(view) = views.get(&node) {
                let sender = self.completion.clone(); let signal = self.wake.clone(); let alive = view.alive.clone();
                view.view.evaluate_script_with_callback(&script, move |value| {
                    if !alive.load(Ordering::Acquire) { return; }
                    let result = if value.len() <= MAX_RESULT_BYTES + 4096 { Ok(value) } else { Err("web result exceeds budget".into()) };
                    let _ = sender.try_send((serial, result)); signal();
                }).map_err(|e| e.to_string())
            } else { Err("document unavailable".into()) };
            if let Err(error) = result { self.evaluations.borrow_mut().complete(serial, Err(error)); }
        }
        if self.evaluations.borrow().has_pending() || self.resources.borrow().has_pending() {
            unsafe { SetTimer(self.window as HWND, TIMER, 25, None); }
        } else { unsafe { KillTimer(self.window as HWND, TIMER); } }
        wake
    }
    pub fn dispose(&self) {
        self.evaluations.borrow_mut().dispose(); self.resources.borrow_mut().dispose();
        for (_, request) in std::mem::take(&mut *self.pending.borrow_mut()) { finish(request, Err("host disposed".into())); }
        for request in self.incoming.lock().unwrap().drain(..) { finish(request, Err("host disposed".into())); }
        self.events.borrow_mut().clear();
        unsafe { KillTimer(self.window as HWND, TIMER); }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) { self.dispose(); self.files.take(); if let Some(worker) = self.worker.take() { let _ = worker.join(); } }
}
pub(super) struct Leaf { view: WebView, alive: Arc<AtomicBool>, owner: String, runtime: Rc<Runtime> }
impl Leaf {
    pub fn new(node: NodeId, parent: HWND, spec: HostedContent, runtime: Rc<Runtime>) -> Result<Self, String> {
        let HostedContent::Web { url, html, base_url, .. } = &spec else { return Err("unsupported Windows hosted content".into()); };
        let alive = Arc::new(AtomicBool::new(true));
        let owner = format!("{:?}-{}", node, parent as usize);
        let weak = Rc::downgrade(&runtime); let live = alive.clone(); let policy = spec.clone();
        let mut builder = WebViewBuilder::new().with_devtools(false).with_clipboard(false).with_autoplay(false).with_focused(false)
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_navigation_handler(move |url| live.load(Ordering::Acquire) && weak.upgrade().is_some_and(|rt| rt.allowed(&policy, &url)));
        for scheme in ["app", "asset"] {
            let incoming = runtime.incoming.clone(); let signal = runtime.wake.clone(); let live = alive.clone(); let owner = owner.clone();
            builder = builder.with_asynchronous_custom_protocol(scheme.into(), move |_, request, responder| {
                let value = ResourceRequest { url: request.uri().to_string(), method: request.method().to_string() };
                let request = Request { owner: owner.clone(), alive: live.clone(), value, responder };
                let mut queue = incoming.lock().unwrap();
                if !live.load(Ordering::Acquire) || queue.len() >= MAX_PENDING { drop(queue); finish(request, Err("resource queue unavailable".into())); return; }
                queue.push_back(request); drop(queue); signal();
            });
        }
        let weak = Rc::downgrade(&runtime); let live = alive.clone(); let policy = spec.clone();
        builder = builder.with_ipc_handler(move |request| {
            if request.body().len() > 16 * 1024 { return; }
            if let Some(rt) = weak.upgrade() {
                if !rt.allowed(&policy, &request.uri().to_string()) { return; }
                if let Ok(value) = serde_json::from_str(request.body()) { rt.event(node, &live, "message", value); }
            }
        });
        let weak = Rc::downgrade(&runtime); let live = alive.clone();
        builder = builder.with_on_page_load_handler(move |event, url| {
            if !live.load(Ordering::Acquire) { return; }
            if let Some(rt) = weak.upgrade() {
                let ready = matches!(event, PageLoadEvent::Finished);
                rt.evaluations.borrow_mut().ready(node, ready);
                rt.event(node, &live, "loading", serde_json::json!(!ready));
                if ready { rt.event(node, &live, "navigation", serde_json::json!(canonical_url(&url))); }
            }
        });
        if let Some(url) = url { if !runtime.allowed(&spec, url) { return Err("web navigation denied".into()); } builder = builder.with_url(url); }
        else if let Some(html) = html {
            let html = if let Some(base) = base_url {
                if !runtime.allowed(&spec, base) { return Err("web base URL denied".into()); }
                format!("<base href=\"{}\">{}", display_url(base).replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;"), html)
            } else { html.clone() };
            builder = builder.with_html(html);
        }
        let view = builder.build_as_child(&Parent(parent)).map_err(|e| e.to_string())?;
        Ok(Self { view, alive, owner, runtime })
    }
    pub fn resize(&self, width: u32, height: u32) -> Result<(), String> {
        self.view.set_bounds(wry::Rect { position: wry::dpi::PhysicalPosition::new(0, 0).into(), size: wry::dpi::PhysicalSize::new(width, height).into() }).map_err(|e| e.to_string())
    }
}
impl Drop for Leaf { fn drop(&mut self) { self.alive.store(false, Ordering::Release); self.runtime.cancel(&self.owner); } }

// Relative URLs in with_html must use the same WebView2 origin as Wry's
// custom-protocol navigation. Only rewrite the leading scheme, never query data.
fn display_url(value: &str) -> String {
    for scheme in ["app", "asset"] {
        if let Some(rest) = value.strip_prefix(&format!("{scheme}://")) { return format!("http://{scheme}.{rest}"); }
    }
    value.to_owned()
}
fn canonical_url(value: &str) -> String {
    for scheme in ["app", "asset"] {
        if let Some(rest) = value.strip_prefix(&format!("http://{scheme}.")) { return format!("{scheme}://{rest}"); }
    }
    value.to_owned()
}
fn finish(request: Request, result: Result<ResourceResponse, String>) {
    let result = result.and_then(|response| { if !response.validate() { return Err("invalid resource response".into()); } if request.alive.load(Ordering::Acquire) { Ok(response) } else { Err("document cancelled".into()) } });
    let response = match result {
        Ok(value) => wry::http::Response::builder().status(value.status).header("Content-Type", value.mime).header("X-Content-Type-Options", "nosniff").body(value.bytes),
        Err(_) => wry::http::Response::builder().status(503).header("Content-Type", "text/plain").body(b"Resource unavailable".to_vec()),
    };
    if let Ok(response) = response { request.responder.respond(response); }
}
fn read_bundle(root: &std::path::Path, request: &Request) -> Result<ResourceResponse, String> {
    if !request.alive.load(Ordering::Acquire) || !matches!(request.value.method.as_str(), "GET" | "HEAD") { return Err("resource cancelled or method denied".into()); }
    let relative = bundle_path(&request.value.url).ok_or("invalid bundle URL")?;
    let path = root.join(relative).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(root) || !path.is_file() { return Err("bundle path escapes root".into()); }
    let mut bytes = Vec::new();
    if request.value.method == "GET" { std::fs::File::open(&path).map_err(|e| e.to_string())?.take(MAX_RESOURCE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?; }
    let mime = match path.extension().and_then(|v| v.to_str()).unwrap_or("") { "html" | "htm" => "text/html; charset=utf-8", "js" | "mjs" => "text/javascript", "css" => "text/css", "json" => "application/json", "svg" => "image/svg+xml", "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "woff2" => "font/woff2", _ => "application/octet-stream" };
    Ok(ResourceResponse { status: 200, mime: mime.into(), bytes })
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn translated_origins_keep_the_authority_and_path() {
        assert_eq!(display_url("app://local/path?q=app://other/"), "http://app.local/path?q=app://other/");
        assert_eq!(display_url("https://example.com/?q=app://local/"), "https://example.com/?q=app://local/");
        assert_eq!(canonical_url("http://app.local/path?q=1#x"), "app://local/path?q=1#x");
        assert_eq!(canonical_url("http://asset.bundle/a.png"), "asset://bundle/a.png");
        assert_eq!(canonical_url("https://app.local/a"), "https://app.local/a");
        assert_eq!(canonical_url("https://example.com/?q=http://app.local/"), "https://example.com/?q=http://app.local/");
    }
}
