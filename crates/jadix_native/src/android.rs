use jni::{
    objects::{GlobalRef, JFloatArray, JObject, JString, JValue},
    JNIEnv, JavaVM,
};

use crate::host::{ApplyReceipt, PlatformHost};
use crate::layout::{LayoutError, MeasureRequest};
use crate::presentation::{CommitBatch, MeasureResult, NodeDescriptor, NodeId, PresentationNode, RootId};

pub struct AndroidHost {
    vm: JavaVM,
    peer: GlobalRef,
    web: std::rc::Rc<std::cell::RefCell<crate::native_web::NativeWeb>>,
    web_id: String,
    resources: std::sync::Arc<std::sync::Mutex<jadix_core::web_resource::WebResources>>,
    services: std::rc::Rc<crate::native_services::NativeServices>,
    windows: std::rc::Rc<std::cell::RefCell<jadix_core::window::WindowRequests>>,
    window_commands: std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>,
    window_factory: std::rc::Rc<std::cell::RefCell<Option<AndroidWindowFactory>>>,
}

impl AndroidHost {
    pub fn new(env: &JNIEnv<'_>, peer: &JObject<'_>) -> jni::errors::Result<Self> {
        // Register by function pointer so every Android application using this
        // host retains the bridge, including independently generated projects.
        let mut registration = unsafe { env.unsafe_clone() };
        registration.register_native_methods("dev/jadix/nativeui/NativeRuntime", &[jni::NativeMethod {
            name: "policy".into(), sig: "(Ljava/lang/String;)Ljava/lang/String;".into(),
            fn_ptr: native_policy as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "performance".into(), sig: "(Ljava/lang/String;DDZZ)Ljava/lang/String;".into(),
            fn_ptr: native_performance as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "quit".into(), sig: "(Ljava/lang/String;Z)I".into(),
            fn_ptr: native_quit as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "terminal".into(), sig: "(Ljava/lang/String;)Ljava/lang/String;".into(),
            fn_ptr: native_terminal as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "rasterizeSvg".into(), sig: "([B)[I".into(),
            fn_ptr: native_rasterize_svg as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "pumpWeb".into(), sig: "(Ldev/jadix/nativeui/NativeView;Ljava/lang/String;)Z".into(),
            fn_ptr: native_pump_web as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "serviceEvents".into(), sig: "(Ljava/lang/String;Ljava/lang/String;)V".into(),
            fn_ptr: native_service_events as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "pumpServices".into(), sig: "(Ldev/jadix/nativeui/NativeView;Ljava/lang/String;)V".into(),
            fn_ptr: native_pump_services as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "requestWebResource".into(), sig: "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)J".into(),
            fn_ptr: native_request_web_resource as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "awaitWebResource".into(), sig: "(Ljava/lang/String;J)[Ljava/lang/Object;".into(),
            fn_ptr: native_await_web_resource as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "cancelWebResources".into(), sig: "(Ljava/lang/String;Ljava/lang/String;)V".into(),
            fn_ptr: native_cancel_web_resources as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "windowWork".into(), sig: "(Ljava/lang/String;)Ljava/lang/String;".into(),
            fn_ptr: native_window_work as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "windowEvent".into(), sig: "(Ljava/lang/String;Ljava/lang/String;)V".into(),
            fn_ptr: native_window_event as *mut std::ffi::c_void,
        }, jni::NativeMethod {
            name: "createWindowRoot".into(), sig: "(Ljava/lang/String;Ldev/jadix/nativeui/NativeView;FFLjava/lang/String;)Ljava/lang/String;".into(),
            fn_ptr: native_create_window_root as *mut std::ffi::c_void,
        }])?;
        let host = Self {
            vm: env.get_java_vm()?, peer: env.new_global_ref(peer)?, web: Default::default(),
            web_id: RootId::new().token(),
            services: Default::default(), resources: Default::default(),
            windows: Default::default(), window_commands: Default::default(), window_factory: Default::default(),
        };
        resource_sessions().lock().unwrap().insert(host.web_id.clone(), std::sync::Arc::downgrade(&host.resources));
        WEB_SESSIONS.with(|sessions| sessions.borrow_mut().insert(host.web_id.clone(), std::rc::Rc::downgrade(&host.web)));
        SERVICE_SESSIONS.with(|sessions| sessions.borrow_mut().insert(host.web_id.clone(), std::rc::Rc::downgrade(&host.services)));
        if let Err(error) = host.call("bindWebSession", "(Ljava/lang/String;)V", &host.web_id) {
            let _ = registration.throw_new("java/lang/IllegalStateException", error);
            return Err(jni::errors::Error::JavaException);
        }
        let vm = env.get_java_vm()?;
        let peer = host.peer.clone();
        let session = host.web_id.clone();
        host.services.set_command_waker(Some(std::rc::Rc::new(move || {
            if let Ok(mut env) = vm.get_env() {
                if let Ok(session) = env.new_string(&session) {
                    let _ = env.call_method(peer.as_obj(), "scheduleServicePump", "(Ljava/lang/String;)V", &[JValue::Object(session.as_ref())]);
                    if env.exception_check().unwrap_or(true) { let _ = env.exception_clear(); }
                }
            }
        })));
        WINDOW_SESSIONS.with(|sessions| sessions.borrow_mut().insert(host.web_id.clone(), AndroidWindows {
            requests: std::rc::Rc::downgrade(&host.windows), commands: std::rc::Rc::downgrade(&host.window_commands),
            factory: std::rc::Rc::downgrade(&host.window_factory),
        }));
        let commands = host.window_commands.clone();
        let vm = env.get_java_vm()?;
        let peer = host.peer.clone();
        let session = host.web_id.clone();
        let mut next = 0_u64;
        host.windows.borrow_mut().set_request_handler(move |request| {
            let mut commands = commands.borrow_mut();
            if commands.len() >= 128 { return Err("Android window queue is full".into()); }
            next = next.checked_add(1).ok_or("window request identity exhausted")?;
            let input = serde_json::json!({"request": next.to_string(), "command": request}).to_string();
            if input.len() > 128 * 1024 { return Err("Android window request is too large".into()); }
            let mut env = vm.get_env().map_err(|error| error.to_string())?;
            let session = env.new_string(&session).map_err(|error| error.to_string())?;
            let result = env.call_method(peer.as_obj(), "scheduleWindowPump", "(Ljava/lang/String;)V", &[JValue::Object(session.as_ref())]);
            if env.exception_check().unwrap_or(true) { let _ = env.exception_clear(); return Err("Android window pump unavailable".into()); }
            result.map_err(|error| error.to_string())?;
            commands.push_back(input);
            Ok(next)
        });
        Ok(host)
    }

    /// The application owns each returned root token, just as for its initial root.
    /// Factories run on the UI thread after the requesting root borrow is released.
    pub fn set_window_factory(&mut self, factory: impl Fn(AndroidHost, [f32; 2], jadix_core::window::WindowContent) -> Result<String, String> + 'static) {
        *self.window_factory.borrow_mut() = Some(std::rc::Rc::new(factory));
    }


    /// Register shared application resources served on WebView worker threads.
    pub fn register_web_resources(&mut self, authority: &str, provider: impl Fn(jadix_core::web_resource::ResourceRequest, jadix_core::web_resource::ResourceReply) + Send + Sync + 'static) -> Result<(), &'static str> {
        self.resources.lock().unwrap().register(authority, provider)
    }

    /// Apply the portable image loader's budgets to this Android root.
    pub fn configure_image_cache(&mut self, limits: &crate::images::ImageLimits) -> Result<(), String> {
        if !limits.is_valid() { return Err("invalid image cache configuration".into()); }
        self.call("configureImages", "(Ljava/lang/String;)V", &serde_json::json!({
            "bindings": limits.bindings, "pending": limits.pending, "sourceBytes": limits.source_bytes,
            "cacheBytes": limits.cache_bytes, "cacheEntries": limits.cache_entries,
            "cacheAgeMs": limits.cache_age.as_millis() as u64,
        }).to_string()).map(|_| ())
    }

    fn call(&self, method: &str, signature: &str, json: &str) -> Result<Option<[f32; 2]>, String> {
        let mut env = self.vm.get_env().map_err(|error| error.to_string())?;
        let value = env.new_string(json).map_err(|error| error.to_string())?;
        let result = env.call_method(
            self.peer.as_obj(),
            method,
            signature,
            &[JValue::Object(value.as_ref())],
        );
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return Err(format!("Android host {method} failed"));
        }
        let result = result.map_err(|error| error.to_string())?;
        if method == "measure" {
            let array = JFloatArray::from(result.l().map_err(|error| error.to_string())?);
            if env
                .get_array_length(&array)
                .map_err(|error| error.to_string())?
                != 2
            {
                return Err(String::from("invalid Android measurement"));
            }
            let mut size = [0.0; 2];
            env.get_float_array_region(&array, 0, &mut size)
                .map_err(|error| error.to_string())?;
            Ok(Some(size))
        } else {
            Ok(None)
        }
    }


}

type ResourceSessions = std::collections::BTreeMap<String, std::sync::Weak<std::sync::Mutex<jadix_core::web_resource::WebResources>>>;
fn resource_sessions() -> &'static std::sync::Mutex<ResourceSessions> {
    static SESSIONS: std::sync::OnceLock<std::sync::Mutex<ResourceSessions>> = std::sync::OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}
fn resource_session(env: &mut JNIEnv<'_>, session: &JString<'_>) -> Option<std::sync::Arc<std::sync::Mutex<jadix_core::web_resource::WebResources>>> {
    let session = env.get_string(session).ok()?.to_string_lossy().into_owned();
    resource_sessions().lock().ok()?.get(&session)?.upgrade()
}
extern "system" fn native_request_web_resource(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, owner: JString<'_>, url: JString<'_>, method: JString<'_>) -> jni::sys::jlong {
    let request = || -> Option<u64> {
        let resources = resource_session(&mut env, &session)?;
        let owner: String = env.get_string(&owner).ok()?.into();
        let url: String = env.get_string(&url).ok()?.into();
        let method: String = env.get_string(&method).ok()?.into();
        let mut resources = resources.lock().ok()?;
        resources.request(&owner, jadix_core::web_resource::ResourceRequest { url, method })
    };
    let mut request = request;
    request().and_then(|id| i64::try_from(id).ok()).unwrap_or(0)
}
extern "system" fn native_cancel_web_resources(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, owner: JString<'_>) {
    let Some(resources) = resource_session(&mut env, &session) else { return; };
    let Ok(owner) = env.get_string(&owner).map(String::from) else { return; };
    if let Ok(mut resources) = resources.lock() { resources.cancel_owner(&owner); };
}
extern "system" fn native_await_web_resource(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, id: jni::sys::jlong) -> jni::sys::jobjectArray {
    let mut await_reply = || -> Option<jni::sys::jobjectArray> {
        let resources = resource_session(&mut env, &session)?;
        let (ticket, work) = {
            let mut resources = resources.lock().ok()?;
            (resources.ticket(u64::try_from(id).ok()?)?, resources.take_work())
        };
        // Neither registry nor host resource state is held while calling providers
        // or waiting for asynchronous completion; UI cancellation remains available.
        for work in work { work.run(); }
        let response = ticket.wait().ok()?;
        // WebResourceResponse does not admit 3xx. A provider must resolve redirects.
        if (300..400).contains(&response.status) { return None; }
        let output = env.new_object_array(3, "java/lang/Object", JObject::null()).ok()?;
        let status = env.new_string(response.status.to_string()).ok()?;
        let mime = env.new_string(response.mime).ok()?;
        let bytes = env.byte_array_from_slice(&response.bytes).ok()?;
        env.set_object_array_element(&output, 0, &status).ok()?;
        env.set_object_array_element(&output, 1, &mime).ok()?;
        env.set_object_array_element(&output, 2, &bytes).ok()?;
        Some(output.into_raw())
    };
    await_reply().unwrap_or(std::ptr::null_mut())
}

extern "system" fn native_pump_services(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, peer: JObject<'_>, session: JString<'_>) {
    let Ok(session) = env.get_string(&session).map(String::from) else { return; };
    let services = SERVICE_SESSIONS.with(|sessions| sessions.borrow().get(&session).and_then(std::rc::Weak::upgrade));
    let Some(services) = services else { return; };
    for command in services.take_commands() {
        let accepted = serde_json::to_string(&command).ok().and_then(|input| env.new_string(input).ok()).is_some_and(|input| {
            let result = env.call_method(&peer, "serviceCommand", "(Ljava/lang/String;)V", &[JValue::Object(input.as_ref())]);
            if env.exception_check().unwrap_or(true) { let _ = env.exception_clear(); false } else { result.is_ok() }
        });
        if !accepted { services.rejected(&command); }
    }
}

extern "system" fn native_policy(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, input: JString<'_>) -> jni::sys::jstring {
    let result = env.get_string(&input).map(String::from).map_err(|error| error.to_string())
        .and_then(|input| crate::native_policy::dispatch(&input));
    match result {
        Ok(value) => env.new_string(value).map_or(std::ptr::null_mut(), |value| value.into_raw()),
        Err(error) => { let _ = env.throw_new("java/lang/IllegalArgumentException", error); std::ptr::null_mut() }
    }
}

extern "system" fn native_terminal(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, input: JString<'_>) -> jni::sys::jstring {
    static SESSIONS: std::sync::OnceLock<std::sync::Mutex<crate::native_policy::terminal::Sessions>> = std::sync::OnceLock::new();
    let result = env.get_string(&input).map(String::from).map_err(|e| e.to_string()).and_then(|input| {
        SESSIONS.get_or_init(Default::default).lock().map_err(|_| "terminal session lock failed".to_owned())?.dispatch(&input)
    });
    match result {
        Ok(value) => env.new_string(value).map_or(std::ptr::null_mut(), |value| value.into_raw()),
        Err(error) => { let _ = env.throw_new("java/lang/IllegalArgumentException", error); std::ptr::null_mut() }
    }
}

impl PlatformHost for AndroidHost {
    fn bind_performance(&mut self, monitor: std::rc::Weak<std::cell::RefCell<crate::host::NativePerformance>>) {
        PERFORMANCE_SESSIONS.with(|sessions| sessions.borrow_mut().insert(self.web_id.clone(), monitor));
    }
    fn read_environment(&self) -> Result<Option<crate::environment::EnvironmentSnapshot>, String> {
        let mut env = self.vm.get_env().map_err(|error| error.to_string())?;
        let json = call_string(&mut env, self.peer.as_obj(), "environmentSnapshot", "")?;
        if json.len() > crate::environment::EnvironmentSnapshot::MAX_JSON_BYTES { return Err("Android environment snapshot exceeds budget".into()); }
        let snapshot: crate::environment::EnvironmentSnapshot = serde_json::from_str(&json).map_err(|error| error.to_string())?;
        snapshot.validate().map_err(|error| error.to_string())?;
        Ok(Some(snapshot))
    }
    fn show_share(&mut self, share: &jadix_core::desktop::Share) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>, String> { self.services.share(share) }
    fn open_url(&mut self, url: &str) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        jadix_core::desktop::check_open_url(url)?;
        let mut env = self.vm.get_env().map_err(|e| e.to_string())?;
        let result = call_string(&mut env, self.peer.as_obj(), "openUrl", &serde_json::json!({"url":url}).to_string()).and_then(|value| serde_json::from_str::<bool>(&value).map_err(|e| e.to_string()));
        let (sender, receiver) = std::sync::mpsc::channel(); let _ = sender.send(result); Ok(receiver)
    }
    fn configure_deep_links(&mut self, config: jadix_core::deep_link::DeepLinkConfig) -> Result<(), String> {
        let mut links = self.services.deep_links.borrow_mut();
        if let Some(router) = links.as_mut() { router.configure(config).map_err(String::from) }
        else { *links = Some(jadix_core::deep_link::DeepLinkRouter::new(config).map_err(String::from)?); Ok(()) }
    }
    fn take_deep_links(&mut self) -> Vec<jadix_core::deep_link::DeepLink> {
        let Some(mut router) = self.services.deep_links.borrow_mut().take() else { return Vec::new(); };
        if let Ok(mut env) = self.vm.get_env() {
            if let Ok(value) = call_string(&mut env, self.peer.as_obj(), "deepLinkExchange", "[]") {
                if value.len() <= 3 * 1024 * 1024 {
                    if let Ok(links) = serde_json::from_str::<Vec<jadix_core::deep_link::DeepLink>>(&value) {
                        for link in links.into_iter().take(jadix_core::deep_link::MAX_DEEP_LINK_QUEUE) { let _ = router.accept(link); }
                    }
                }
            }
        }
        let result = router.drain(); *self.services.deep_links.borrow_mut() = Some(router); result
    }
    fn audio(&self) -> Option<jadix_core::audio::AudioService> { Some(self.services.audio.clone()) }
    fn speech(&self) -> Option<jadix_core::speech::SpeechService> { Some(self.services.speech.clone()) }
    fn fetch(&self) -> Option<jadix_net::service::FetchService> { Some(self.services.fetch.clone()) }
    fn desktop_services(&self) -> Option<jadix_core::desktop_services::DesktopServices> { Some(self.services.desktop.clone()) }
    fn pump_audio(&mut self, _: RootId) {
        for command in self.services.take_commands() {
            let accepted = serde_json::to_string(&command).ok().is_some_and(|input|
                self.call("serviceCommand", "(Ljava/lang/String;)V", &input).is_ok());
            if !accepted { self.services.rejected(&command); }
        }
    }
    fn window_declarations(&self) -> Option<jadix_core::window::WindowDeclarations> {
        Some(jadix_core::window::WindowRequests::declarations(&self.windows))
    }
    fn request_window(&mut self, request: &jadix_core::window::WindowRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::window::WindowResult>, String> {
        self.windows.borrow_mut().request(request).map(|(_, receiver)| receiver)
    }
    fn capabilities(&self) -> crate::presentation::capabilities::BackendCapabilities {
        let mut capabilities = crate::presentation::capabilities::BackendCapabilities::headless();
        capabilities.inline_raster_sources = true;
        capabilities.inline_svg_sources = true;
        capabilities.local_image_sources = true;
        capabilities.remote_image_sources = true;
        capabilities
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let [width, height] = self
            .call(
                "measure",
                "(Ljava/lang/String;)[F",
                &crate::wire::measurement(request, descriptor),
            )
            .ok()
            .flatten()
            .ok_or(LayoutError::MeasurementUnavailable(request.key.node))?;
        if !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0 {
            return Err(LayoutError::InvalidMeasurement(request.key.node));
        }
        Ok(MeasureResult {
            key: request.key,
            width: f64::from(width).min(request.key.max_width),
            height: f64::from(height).min(request.key.max_height),
            baseline: None,
        })
    }

    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String> {
        self.call(
            "publish",
            "(Ljava/lang/String;)V",
            &crate::wire::snapshot(root, revision, roots),
        )
        ?;
        self.web.borrow_mut().sync(roots);
        Ok(())
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        let mut env = self.vm.get_env().map_err(|error| error.to_string())?;
        let value = env
            .new_string(crate::wire::batch(batch))
            .map_err(|error| error.to_string())?;
        let result = env.call_method(
            self.peer.as_obj(),
            "publishBatch",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(value.as_ref())],
        );
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return Err(String::from("Android native batch failed"));
        }
        let receipt = JString::from(
            result
                .map_err(|error| error.to_string())?
                .l()
                .map_err(|error| error.to_string())?,
        );
        let receipt: String = env
            .get_string(&receipt)
            .map_err(|error| error.to_string())?
            .into();
        if receipt
            != format!(
                "{}:{}:{}",
                batch.root.token(),
                batch.previous,
                batch.revision
            )
        {
            return Err(String::from(
                "Android native batch acknowledgement mismatch",
            ));
        }
        self.web.borrow_mut().sync(roots);
        Ok(ApplyReceipt::for_batch(batch))
    }

    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        self.call("scrollCommand", "(Ljava/lang/String;)V", &serde_json::json!({"id":node.token(),"operation":"scroll","offset":offset}).to_string()).map(|_| ())
    }

    fn stop_scroll(&mut self, node: NodeId) -> Result<(), String> {
        self.call("scrollCommand", "(Ljava/lang/String;)V", &serde_json::json!({"id":node.token(),"operation":"stop"}).to_string()).map(|_| ())
    }

    fn zoom_to(&mut self, node: NodeId, scale: f64, anchor: [f64; 2]) -> Result<(), String> {
        self.call("scrollCommand", "(Ljava/lang/String;)V", &serde_json::json!({"id":node.token(),"operation":"zoom","scale":scale,"anchor":anchor}).to_string()).map(|_| ())
    }

    fn evaluate_web(&mut self, node: NodeId, script: &str) -> jadix_core::web_evaluation::WebEvaluation {
        let result = self.web.borrow_mut().evaluate(node, script);
        let _ = self.call("scheduleWebPump", "(Ljava/lang/String;)V", "");
        result
    }

    fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        let Ok(mut env) = self.vm.get_env() else { return Vec::new(); };
        pump_web(&mut self.web.borrow_mut(), &mut env, self.peer.as_obj())
    }

    fn schedule_reactive_wake(&mut self, delay: Option<std::time::Duration>) {
        let millis = delay.map(|delay| delay.as_millis().saturating_add(u128::from(delay.subsec_nanos() % 1_000_000 != 0)).min(i64::MAX as u128) as i64);
        let _ = self.call("scheduleReactiveWake", "(Ljava/lang/String;)V", &serde_json::to_string(&millis).unwrap());
    }

    fn dispose(&mut self) {
        self.windows.borrow_mut().dispose();
        self.window_commands.borrow_mut().clear();
        self.window_factory.borrow_mut().take();
        self.resources.lock().unwrap().dispose();
        self.services.dispose();
        self.web.borrow_mut().dispose();
        let _ = self.call("clear", "(Ljava/lang/String;)V", "{}");
    }
}

thread_local! {
    static QUIT_SESSIONS: std::cell::RefCell<std::collections::BTreeMap<String, jadix_core::reactivity::AppQuitCleanup>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static PERFORMANCE_SESSIONS: std::cell::RefCell<std::collections::BTreeMap<String, std::rc::Weak<std::cell::RefCell<crate::host::NativePerformance>>>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static SERVICE_SESSIONS: std::cell::RefCell<std::collections::BTreeMap<String, std::rc::Weak<crate::native_services::NativeServices>>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    static WEB_SESSIONS: std::cell::RefCell<std::collections::BTreeMap<String, std::rc::Weak<std::cell::RefCell<crate::native_web::NativeWeb>>>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}
impl Drop for AndroidHost {
    fn drop(&mut self) {
        QUIT_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&self.web_id));
        PERFORMANCE_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&self.web_id));
        self.resources.lock().unwrap().dispose();
        self.services.dispose();
        self.windows.borrow_mut().dispose();
        WINDOW_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&self.web_id));
        resource_sessions().lock().unwrap().remove(&self.web_id);
        SERVICE_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&self.web_id));
        WEB_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&self.web_id));
    }
}
extern "system" fn native_quit(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, begin: jni::sys::jboolean) -> jni::sys::jint {
    let Ok(session) = env.get_string(&session).map(String::from) else { return -1; };
    if !WEB_SESSIONS.with(|sessions| sessions.borrow().get(&session).is_some_and(|host| host.strong_count() != 0)) { return -1; }
    // Temporarily remove the controller before invoking application code. Reentrant
    // close requests fail closed instead of borrowing the TLS registry recursively.
    thread_local! { static ASKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if ASKING.with(|asking| asking.replace(true)) { return -1; }
    let mut cleanup = QUIT_SESSIONS.with(|sessions| sessions.borrow_mut().remove(&session)).unwrap_or_default();
    let now = std::time::Instant::now();
    let result = if begin != 0 && !cleanup.begin(now) { -1 }
        else if cleanup.poll(now, std::task::Waker::noop()) { 1 } else { 0 };
    if result >= 0 { QUIT_SESSIONS.with(|sessions| sessions.borrow_mut().insert(session, cleanup)); }
    ASKING.with(|asking| asking.set(false));
    result
}
extern "system" fn native_performance(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, id: JString<'_>, refresh: f64, work: f64, changed: jni::sys::jboolean, sample: jni::sys::jboolean) -> jni::sys::jstring {
    jadix_core::reactivity::collect_reactive_garbage();
    let Ok(id) = env.get_string(&id).map(String::from) else { return std::ptr::null_mut(); };
    let Some(monitor) = PERFORMANCE_SESSIONS.with(|sessions| sessions.borrow().get(&id).and_then(std::rc::Weak::upgrade)) else { return std::ptr::null_mut(); };
    let Ok(mut monitor) = monitor.try_borrow_mut() else { return std::ptr::null_mut(); };
    let cost = (changed != 0 && work.is_finite() && (0.0..=60.0).contains(&work)).then(|| std::time::Duration::from_secs_f64(work));
    monitor.display_tick(Some(refresh), cost);
    if sample == 0 { return std::ptr::null_mut(); }
    serde_json::to_string(&monitor.snapshot()).ok().and_then(|json| env.new_string(json).ok()).map_or(std::ptr::null_mut(), |value| value.into_raw())
}
extern "system" fn native_service_events(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, id: JString<'_>, input: JString<'_>) {
    let Ok(id) = env.get_string(&id).map(String::from) else { return; };
    let Some(services) = SERVICE_SESSIONS.with(|sessions| sessions.borrow().get(&id).and_then(std::rc::Weak::upgrade)) else { return; };
    let result = env.get_string(&input).map(String::from).map_err(|e| e.to_string()).and_then(|input| services.events(&input));
    if let Err(error) = result { let _ = env.throw_new("java/lang/IllegalArgumentException", error); }
}
fn call_string(env: &mut JNIEnv<'_>, peer: &JObject<'_>, method: &str, json: &str) -> Result<String, String> {
    let input = env.new_string(json).map_err(|e| e.to_string())?;
    let result = env.call_method(peer, method, "(Ljava/lang/String;)Ljava/lang/String;", &[JValue::Object(input.as_ref())]);
    if env.exception_check().unwrap_or(true) { let _ = env.exception_clear(); return Err(format!("Android host {method} failed")); }
    let output = JString::from(result.map_err(|e| e.to_string())?.l().map_err(|e| e.to_string())?);
    let output = env.get_string(&output).map_err(|e| e.to_string())?;
    Ok(output.into())
}
fn pump_web(web: &mut crate::native_web::NativeWeb, env: &mut JNIEnv<'_>, peer: &JObject<'_>) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        let nodes: Vec<_> = web.nodes().collect();
        let exchange = call_string(env, peer, "webExchange", "[]")
            .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).map_err(|e| e.to_string()));
        match exchange {
            Ok(value) => {
                for node in nodes {
                    web.ready(node, value["ready"][node.token()].as_bool().unwrap_or(false));
                }
                if let Some(results) = value["results"].as_array() {
                    for result in results {
                        if let (Some(request), Some(value)) = (result["request"].as_str().and_then(|id| id.parse().ok()), result["value"].as_str()) {
                            web.complete(request, if result["ok"].as_bool() == Some(true) { Ok(value.into()) } else { Err(value.into()) });
                        }
                    }
                }
            }
            Err(_) => for node in nodes { web.ready(node, false); },
        }
        let (work, wake) = web.pump();
        if !work.is_empty() {
            let input: Vec<_> = work.iter().map(|(node, request, script)| serde_json::json!({"node":node.token(), "request":request.to_string(), "script":script})).collect();
            if let Err(error) = call_string(env, peer, "webExchange", &serde_json::to_string(&input).unwrap()) {
                for (_, request, _) in work { web.complete(request, Err(error.clone())); }
            }
        }
        wake
}
extern "system" fn native_pump_web(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, peer: JObject<'_>, id: JString<'_>) -> jni::sys::jboolean {
    let Ok(id) = env.get_string(&id).map(String::from) else { return 0; };
    let Some(session) = WEB_SESSIONS.with(|sessions| sessions.borrow().get(&id).and_then(std::rc::Weak::upgrade)) else { return 0; };
    let Ok(mut web) = session.try_borrow_mut() else { return 1; };
    let wake = pump_web(&mut web, &mut env, &peer);
    let pending = web.has_pending();
    drop(web);
    // Wakers can reenter application state; neither registry nor session is borrowed.
    for batch in wake { batch.wake(); }
    u8::from(pending)
}

extern "system" fn native_rasterize_svg(env: JNIEnv<'_>, _: jni::objects::JClass<'_>, input: jni::objects::JByteArray<'_>) -> jni::sys::jintArray {
    let render = || -> Option<jni::sys::jintArray> {
        if env.get_array_length(&input).ok()? > 1024 * 1024 { return None; }
        let bytes = env.convert_byte_array(&input).ok()?;
        let (image, _) = crate::images::svg::rasterize(&bytes, 4096, 4096, 2)?;
        let mut pixels = Vec::with_capacity(image.pixels().len() + 2);
        pixels.extend([image.width() as i32, image.height() as i32]);
        pixels.extend(image.pixels().iter().map(|pixel| {
            let c = pixel.demultiply();
            ((u32::from(c.alpha()) << 24) | (u32::from(c.red()) << 16) | (u32::from(c.green()) << 8) | u32::from(c.blue())) as i32
        }));
        let output = env.new_int_array(pixels.len().try_into().ok()?).ok()?;
        env.set_int_array_region(&output, 0, &pixels).ok()?;
        Some(output.into_raw())
    };
    render().unwrap_or(std::ptr::null_mut())
}


type AndroidWindowFactory = std::rc::Rc<dyn Fn(AndroidHost, [f32; 2], jadix_core::window::WindowContent) -> Result<String, String>>;
struct AndroidWindows {
    requests: std::rc::Weak<std::cell::RefCell<jadix_core::window::WindowRequests>>,
    commands: std::rc::Weak<std::cell::RefCell<std::collections::VecDeque<String>>>,
    factory: std::rc::Weak<std::cell::RefCell<Option<AndroidWindowFactory>>>,
}
thread_local! {
    static WINDOW_SESSIONS: std::cell::RefCell<std::collections::BTreeMap<String, AndroidWindows>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}
extern "system" fn native_window_work(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>) -> jni::sys::jstring {
    let work = env.get_string(&session).ok().and_then(|session| WINDOW_SESSIONS.with(|sessions|
        sessions.borrow().get(session.to_str().ok()?).and_then(|entry| entry.commands.upgrade())))
        .map(|queue| queue.borrow_mut().drain(..).collect::<Vec<_>>()).unwrap_or_default();
    let output = format!("[{}]", work.join(","));
    env.new_string(output).map_or(std::ptr::null_mut(), |value| value.into_raw())
}
extern "system" fn native_window_event(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, input: JString<'_>) {
    let Some(windows) = env.get_string(&session).ok().and_then(|session| WINDOW_SESSIONS.with(|sessions|
        sessions.borrow().get(session.to_str().ok()?).and_then(|entry| entry.requests.upgrade()))) else { return; };
    let Ok(input) = env.get_string(&input).map(String::from) else { return; };
    if input.len() > 64 * 1024 { return; }
    let Ok(event) = serde_json::from_str::<serde_json::Value>(&input) else { return; };
    jadix_core::reactivity::batch(|| {
        let mut windows = windows.borrow_mut();
        if let Some(id) = event.get("request").and_then(|id| id.as_str()).and_then(|id| id.parse().ok()) {
            let result = event.get("error").and_then(|error| error.as_str()).map_or(Ok(()), |error| Err(error.to_owned()));
            let _ = windows.complete(id, result);
        } else if let Some(id) = event.get("destroyed").and_then(|id| id.as_str()) {
            windows.child_destroyed(id);
        } else if let Some(id) = event.get("id").and_then(|id| id.as_str()) {
            if let Some(state) = event.get("state").and_then(|state| serde_json::from_value(state.clone()).ok()) { windows.child_state(id, state); }
        }
    });
}
extern "system" fn native_create_window_root(mut env: JNIEnv<'_>, _: jni::objects::JClass<'_>, session: JString<'_>, peer: JObject<'_>, width: f32, height: f32, input: JString<'_>) -> jni::sys::jstring {
    let mut create = || -> Result<String, String> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 { return Err("invalid child root viewport".into()); }
        let session: String = env.get_string(&session).map_err(|error| error.to_string())?.into();
        let factory = WINDOW_SESSIONS.with(|sessions| sessions.borrow().get(&session).and_then(|entry| entry.factory.upgrade()))
            .and_then(|factory| factory.borrow().clone()).ok_or("window content factory is unavailable")?;
        let input: String = env.get_string(&input).map_err(|error| error.to_string())?.into();
        if input.len() > 128 * 1024 { return Err("window content exceeds transport limit".into()); }
        let content: jadix_core::window::WindowContent = serde_json::from_str(&input).map_err(|error| error.to_string())?;
        content.validate()?;
        let host = AndroidHost::new(&env, &peer).map_err(|error| error.to_string())?;
        *host.window_factory.borrow_mut() = Some(factory.clone());
        factory(host, [width, height], content)
    };
    match create() {
        Ok(token) => env.new_string(token).map_or(std::ptr::null_mut(), |value| value.into_raw()),
        Err(error) => { let _ = env.throw_new("java/lang/IllegalStateException", error); std::ptr::null_mut() }
    }
}
