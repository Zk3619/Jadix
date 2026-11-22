//! WebKitGTK documents over the shared evaluation and application-resource services.
use super::*;
use jadix_core::{hosted::HostedContent, web_resource::*};
use std::{path::PathBuf, sync::Arc};
use webkit6::{prelude::*, WebView};

pub(super) struct Runtime {
    pub fetch: jadix_net::service::FetchService,
    pub evaluations: RefCell<crate::native_web::NativeWeb>,
    pub resources: RefCell<WebResources>,
    requests: RefCell<BTreeMap<u64, (String, webkit6::URISchemeRequest)>>,
    reads: RefCell<BTreeMap<u64, (String, gio::Cancellable, webkit6::URISchemeRequest)>>,
    read_serial: Cell<u64>,
    asset_root: RefCell<PathBuf>,
    timer: RefCell<Option<glib::SourceId>>,
    wake: WakeHandle,
}
impl Runtime {
    pub(super) fn asset_root(&self) -> std::path::PathBuf {
        self.asset_root.borrow().clone()
    }
    pub fn new(
        wake: WakeHandle,
        asset_root: PathBuf,
        fetch: jadix_net::service::FetchService,
    ) -> Rc<Self> {
        let mut resources = WebResources::default();
        let signal = wake.clone();
        resources.set_completion_waker(Some(Arc::new(move || signal.wake())));
        Rc::new(Self {
            fetch,
            evaluations: Default::default(),
            resources: RefCell::new(resources),
            requests: Default::default(),
            reads: Default::default(),
            read_serial: Cell::new(0),
            asset_root: RefCell::new(asset_root),
            timer: Default::default(),
            wake,
        })
    }
    pub fn set_asset_root(&self, path: PathBuf) {
        *self.asset_root.borrow_mut() = path;
    }
    fn arm(self: &Rc<Self>) {
        if self.timer.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        *self.timer.borrow_mut() = Some(glib::timeout_add_local_once(
            std::time::Duration::from_millis(25),
            move || {
                if let Some(this) = weak.upgrade() {
                    this.timer.borrow_mut().take();
                    this.wake.wake();
                }
            },
        ));
    }
    fn cancel_owner(&self, owner: &str) {
        self.resources.borrow_mut().cancel_owner(owner);
        let ids: Vec<_> = self
            .requests
            .borrow()
            .iter()
            .filter_map(|(id, (value, _))| (value == owner).then_some(*id))
            .collect();
        for id in ids {
            if let Some((_, request)) = self.requests.borrow_mut().remove(&id) {
                fail(&request, "document cancelled");
            }
        }
        let ids: Vec<_> = self
            .reads
            .borrow()
            .iter()
            .filter_map(|(id, (value, _, _))| (value == owner).then_some(*id))
            .collect();
        for id in ids {
            if let Some((_, cancel, request)) = self.reads.borrow_mut().remove(&id) {
                cancel.cancel();
                fail(&request, "document cancelled");
            }
        }
    }
    fn request(self: &Rc<Self>, owner: &str, request: &webkit6::URISchemeRequest) {
        let value = ResourceRequest {
            url: request.uri().map(|v| v.to_string()).unwrap_or_default(),
            method: request
                .http_method()
                .map(|v| v.to_string())
                .unwrap_or_default(),
        };
        if let Some(relative) = bundle_path(&value.url) {
            if !matches!(value.method.as_str(), "GET" | "HEAD") {
                fail(request, "unsupported resource method");
                return;
            }
            let path = self.asset_root.borrow().join(relative).canonicalize();
            let Ok(path) = path else {
                fail(request, "bundle resource not found");
                return;
            };
            if !path.starts_with(&*self.asset_root.borrow())
                || !path.is_file()
                || !path
                    .metadata()
                    .is_ok_and(|m| m.len() <= MAX_RESOURCE_BYTES as u64)
            {
                fail(request, "invalid bundle resource");
                return;
            }
            let mime =
                gio::content_type_get_mime_type(&gio::content_type_guess(Some(&path), None).0)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "application/octet-stream".into());
            if value.method == "HEAD" {
                finish(
                    request,
                    ResourceResponse {
                        status: 200,
                        mime,
                        bytes: vec![],
                    },
                );
                return;
            }
            if self.reads.borrow().len() >= 64 {
                fail(request, "bundle resource capacity exhausted");
                return;
            }
            let Some(id) = self.read_serial.get().checked_add(1) else {
                fail(request, "resource serial exhausted");
                return;
            };
            self.read_serial.set(id);
            let cancel = gio::Cancellable::new();
            self.reads
                .borrow_mut()
                .insert(id, (owner.into(), cancel.clone(), request.clone()));
            // Bounded chunks also cover a file growing after its metadata check.
            let weak = Rc::downgrade(self);
            gio::File::for_path(path).read_async(
                glib::Priority::DEFAULT,
                Some(&cancel),
                move |result| {
                    if let Some(this) = weak.upgrade() {
                        match result {
                            Ok(stream) => this.read_chunk(id, stream.upcast(), mime, vec![]),
                            Err(_) => {
                                this.finish_read(id, Err("bundle resource open failed".into()))
                            }
                        }
                    }
                },
            );
            return;
        }
        let id = self.resources.borrow_mut().request(owner, value);
        if let Some(id) = id {
            self.requests
                .borrow_mut()
                .insert(id, (owner.into(), request.clone()));
            self.wake.wake();
            self.arm();
        } else {
            fail(request, "unregistered or invalid application resource");
        }
    }
    fn finish_read(&self, id: u64, result: Result<ResourceResponse, String>) {
        if let Some((_, _, request)) = self.reads.borrow_mut().remove(&id) {
            match result {
                Ok(response) => finish(&request, response),
                Err(message) => fail(&request, &message),
            }
        }
    }
    fn read_chunk(
        self: &Rc<Self>,
        id: u64,
        stream: gio::InputStream,
        mime: String,
        mut bytes: Vec<u8>,
    ) {
        let Some(cancel) = self
            .reads
            .borrow()
            .get(&id)
            .map(|(_, cancel, _)| cancel.clone())
        else {
            return;
        };
        let weak = Rc::downgrade(self);
        let next = stream.clone();
        stream.read_bytes_async(
            (MAX_RESOURCE_BYTES + 1 - bytes.len()).min(64 * 1024),
            glib::Priority::DEFAULT,
            Some(&cancel),
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                if !this.reads.borrow().contains_key(&id) {
                    return;
                }
                match result {
                    Ok(chunk) if chunk.is_empty() => this.finish_read(
                        id,
                        Ok(ResourceResponse {
                            status: 200,
                            mime,
                            bytes,
                        }),
                    ),
                    Ok(chunk) if bytes.len() + chunk.len() <= MAX_RESOURCE_BYTES => {
                        bytes.extend_from_slice(&chunk);
                        this.read_chunk(id, next, mime, bytes);
                    }
                    _ => this.finish_read(
                        id,
                        Err("bundle resource read failed or exceeded budget".into()),
                    ),
                }
            },
        );
    }
    pub fn pump(
        self: &Rc<Self>,
        items: &BTreeMap<NodeId, Item>,
    ) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        for (id, result) in self.resources.borrow_mut().take_completed() {
            if let Some((_, request)) = self.requests.borrow_mut().remove(&id) {
                match result {
                    Ok(response) => finish(&request, response),
                    Err(message) => fail(&request, &message),
                }
            }
        }
        let (work, batches) = self.evaluations.borrow_mut().pump();
        for (id, serial, script) in work {
            if let Some(hosted::State::Web(web)) =
                items.get(&id).and_then(|item| item.hosted.as_ref())
            {
                web.evaluate(serial, &script);
            } else {
                self.evaluations
                    .borrow_mut()
                    .complete(serial, Err("document removed".into()));
            }
        }
        if self.evaluations.borrow().has_pending() || self.resources.borrow().has_pending() {
            self.arm();
        }
        batches
    }
    pub fn dispose(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.evaluations.borrow_mut().dispose();
        self.resources.borrow_mut().dispose();
        for (_, (_, request)) in std::mem::take(&mut *self.requests.borrow_mut()) {
            fail(&request, "host disposed");
        }
        for (_, (_, cancel, request)) in std::mem::take(&mut *self.reads.borrow_mut()) {
            cancel.cancel();
            fail(&request, "host disposed");
        }
    }
}

pub(super) struct Web {
    pub view: WebView,
    runtime: Rc<Runtime>,
    spec: RefCell<HostedContent>,
    control: Rc<Control>,
    events: Rc<Events>,
    owner: RefCell<String>,
    generation: Cell<u64>,
    failed: Cell<bool>,
    calls: RefCell<BTreeMap<u64, gio::Cancellable>>,
}
impl Web {
    pub fn new(
        spec: &HostedContent,
        runtime: Rc<Runtime>,
        events: Rc<Events>,
        control: Rc<Control>,
    ) -> Result<Rc<Self>, String> {
        let manager = webkit6::UserContentManager::new();
        if !manager.register_script_message_handler("jadix", None) {
            return Err("WebKit message registration failed".into());
        }
        let context = webkit6::WebContext::new();
        let settings = webkit6::Settings::new();
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_javascript_can_access_clipboard(false);
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_allow_universal_access_from_file_urls(false);
        let view = WebView::builder()
            .web_context(&context)
            .user_content_manager(&manager)
            .settings(&settings)
            .network_session(&webkit6::NetworkSession::new_ephemeral())
            .build();
        let this = Rc::new(Self {
            view,
            runtime,
            spec: RefCell::new(spec.clone()),
            control,
            events,
            owner: Default::default(),
            generation: Cell::new(0),
            failed: Cell::new(false),
            calls: Default::default(),
        });
        for scheme in ["app", "asset"] {
            let weak = Rc::downgrade(&this);
            context.register_uri_scheme(scheme, move |request| {
                if let Some(this) = weak.upgrade() {
                    this.runtime.request(&this.owner.borrow(), request);
                } else {
                    fail(request, "document removed");
                }
            });
        }
        let weak = Rc::downgrade(&this);
        this.view.connect_decide_policy(move |_, decision, kind| {
            let Some(this) = weak.upgrade() else {
                decision.ignore();
                return true;
            };
            if kind == webkit6::PolicyDecisionType::NewWindowAction {
                decision.ignore();
                return true;
            }
            if let Some(navigation) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                let uri = navigation
                    .navigation_action()
                    .and_then(|a| a.request())
                    .and_then(|r| r.uri());
                let allowed = uri.as_ref().is_some_and(|uri| this.allowed(uri));
                if allowed {
                    decision.use_();
                } else {
                    decision.ignore();
                    this.event("error", "web navigation denied".into());
                }
                return true;
            }
            false
        });
        this.view.connect_create(|_, _| None);
        this.view.connect_permission_request(|_, request| {
            request.deny();
            true
        });
        let weak = Rc::downgrade(&this);
        this.view.connect_load_changed(move |view, event| {
            let Some(this) = weak.upgrade() else { return };
            match event {
                webkit6::LoadEvent::Started => {
                    this.failed.set(false);
                    this.cancel();
                    this.event("loading", true.into());
                }
                webkit6::LoadEvent::Committed => {
                    if let Some(uri) = view.uri() {
                        this.event("navigation", uri.to_string().into());
                    }
                }
                webkit6::LoadEvent::Finished => {
                    this.runtime
                        .evaluations
                        .borrow_mut()
                        .ready(this.control.id, !this.failed.get());
                    this.event("loading", false.into());
                }
                _ => {}
            }
            this.runtime.wake.wake();
        });
        let weak = Rc::downgrade(&this);
        this.view.connect_load_failed(move |_, _, _, error| {
            if let Some(this) = weak.upgrade() {
                this.failed.set(true);
                this.cancel();
                this.event(
                    "error",
                    error
                        .to_string()
                        .chars()
                        .take(4096)
                        .collect::<String>()
                        .into(),
                );
            }
            false
        });
        let weak = Rc::downgrade(&this);
        this.view.connect_web_process_terminated(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.cancel();
                this.event("error", "web process terminated".into());
            }
        });
        let weak = Rc::downgrade(&this);
        manager.connect_script_message_received(Some("jadix"), move |_, value| {
            if let Some(this) = weak.upgrade() {
                if !this.view.uri().is_some_and(|uri| this.allowed(&uri)) {
                    return;
                }
                if let Some(json) = value.to_json(0).filter(|v| v.len() <= 1024 * 1024) {
                    if let Ok(value) = serde_json::from_str(&json) {
                        this.event("message", value);
                    }
                }
            }
        });
        Ok(this)
    }
    fn allowed(&self, uri: &str) -> bool {
        let registered = self.runtime.resources.borrow().allows(&ResourceRequest {
            url: uri.into(),
            method: "GET".into(),
        });
        self.spec.borrow().allows_web_navigation(uri, registered)
    }
    fn event(&self, event: &str, data: serde_json::Value) {
        hosted::emit(&self.events, &self.control, event, data);
    }
    fn cancel(&self) {
        self.runtime
            .evaluations
            .borrow_mut()
            .ready(self.control.id, false);
        self.runtime.cancel_owner(&self.owner.borrow());
        for (_, call) in std::mem::take(&mut *self.calls.borrow_mut()) {
            call.cancel();
        }
    }
    pub fn update(&self, spec: &HostedContent) {
        if self.generation.get() != 0 && *self.spec.borrow() == *spec {
            return;
        }
        self.cancel();
        self.view.stop_loading();
        self.generation.set(self.generation.get().saturating_add(1));
        *self.owner.borrow_mut() = format!("{:?}:{}", self.control.id, self.generation.get());
        *self.spec.borrow_mut() = spec.clone();
        if let HostedContent::Web {
            url,
            html,
            base_url,
            ..
        } = spec
        {
            if let Some(url) = url {
                if self.allowed(url) {
                    self.view.load_uri(url)
                } else {
                    self.event("error", "web source denied".into());
                }
            } else if let Some(html) = html {
                if base_url.as_ref().is_none_or(|uri| self.allowed(uri)) {
                    self.view.load_html(html, base_url.as_deref());
                } else {
                    self.event("error", "web base URL denied".into());
                }
            }
        }
    }
    fn evaluate(self: &Rc<Self>, serial: u64, script: &str) {
        let call = gio::Cancellable::new();
        self.calls.borrow_mut().insert(serial, call.clone());
        let weak = Rc::downgrade(self);
        self.view
            .evaluate_javascript(script, None, None, Some(&call), move |result| {
                if let Some(this) = weak.upgrade() {
                    this.calls.borrow_mut().remove(&serial);
                    let result = result.map_err(|e| e.to_string()).and_then(|value| {
                        let text = value.to_str().to_string();
                        if text.len() <= jadix_core::web_evaluation::MAX_RESULT_BYTES {
                            Ok(text)
                        } else {
                            Err("evaluation result exceeds budget".into())
                        }
                    });
                    this.runtime
                        .evaluations
                        .borrow_mut()
                        .complete(serial, result);
                    this.runtime.wake.wake();
                }
            });
    }
    pub fn dispose(&self) {
        self.cancel();
        self.view.stop_loading();
        if let Some(manager) = self.view.user_content_manager() {
            manager.unregister_script_message_handler("jadix", None);
        }
    }
}
impl Drop for Web {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn fail(request: &webkit6::URISchemeRequest, message: &str) {
    request.finish_error(&mut glib::Error::new(gio::IOErrorEnum::Failed, message));
}
fn finish(request: &webkit6::URISchemeRequest, response: ResourceResponse) {
    let bytes = glib::Bytes::from_owned(response.bytes);
    let length = bytes.len() as i64;
    let stream = gio::MemoryInputStream::from_bytes(&bytes);
    let result = webkit6::URISchemeResponse::new(&stream, length);
    result.set_content_type(&response.mime);
    result.set_status(response.status as u32, None);
    request.finish_with_response(&result);
}
