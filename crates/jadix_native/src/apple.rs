//! Shared Apple host: validated requests and root-owned service lifetimes.
use crate::{
    host::PlatformHost,
    layout::{LayoutError, MeasureRequest},
    presentation::*,
};
pub use jadix_core::window::{
    DeclaredWindow as AppleDeclaredWindow, DeclaredWindowOptions as AppleDeclaredWindowOptions,
    WindowAppearance as AppleWindowAppearance, WindowAttributes as AppleWindowAttributes,
    WindowContent as AppleWindowContent, WindowDeclarations as AppleWindowDeclarations,
    WindowDisplay as AppleWindowDisplay, WindowFrame as AppleWindowFrame,
    WindowKind as AppleWindowKind, WindowLevel as AppleWindowLevel,
    WindowMode as AppleWindowMode, WindowPosition as AppleWindowPosition,
    WindowRequest as AppleWindowRequest, WindowResult as AppleWindowResult,
    WindowSize as AppleWindowSize, WindowState as AppleWindowState,
    WindowTitleBar as AppleTitleBarStyle, WindowTitleBarInsets as AppleTitleBarInsets,
    WindowWorkspaces as AppleWindowWorkspaces,
};
use jadix_core::{
    menu::{self, MenuItem, MenuRole, MenuState},
    reactivity::{
        batch, poll_app_quit_futures, take_app_quit_futures, AppQuitFuture, Signal,
        APP_QUIT_TIMEOUT,
    },
    shortcut::{KeyChord, ShortcutKey, ShortcutModifier},
    window::{WindowHost, WindowRequests},
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_void, CStr, CString},
    marker::PhantomData,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, Receiver, Sender},
    task::Waker,
    time::Instant,
};

pub type AppleRequestCallback = unsafe extern "C" fn(*mut c_void, u64, *const c_char) -> bool;
pub type AppleFileRequestCallback = AppleRequestCallback;
pub type AppleFileResult = jadix_core::file_request::FileResult;
pub type AppleAlertResult = Result<AppleAlertResponse, String>;
pub type AppleShareResult = Result<AppleShareOutcome, String>;
pub type AppleContextMenuResult = Result<Option<String>, String>;

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AppleMenuItem {
    Command {
        id: String,
        label: String,
        enabled: bool,
        checked: bool,
        role: Option<&'static str>,
        icon: Option<AppleMenuIcon>,
        key: String,
        modifiers: Vec<&'static str>,
    },
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<Self>,
        icon: Option<AppleMenuIcon>,
    },
    Separator,
}

#[derive(serde::Serialize)]
struct AppleMenuIcon {
    source: String,
    template: bool,
}

type MenuCommands = BTreeMap<String, Rc<dyn Fn()>>;

struct PendingContextMenu {
    sender: Sender<AppleContextMenuResult>,
    commands: MenuCommands,
}

#[derive(Default)]
struct AppleMenuBar {
    revision: u64,
    commands: MenuCommands,
}

fn appkit_menu_items(
    items: Vec<MenuItem>,
    parent_enabled: bool,
    native_roles: bool,
    commands: &mut MenuCommands,
) -> Vec<AppleMenuItem> {
    items
        .into_iter()
        .map(|item| match item {
            MenuItem::Command {
                id,
                label,
                enabled,
                accelerator,
                state,
                icon,
                on_select,
            } => {
                let enabled = parent_enabled && enabled;
                let role = match state {
                    MenuState::Role(role) | MenuState::LocalizedRole(role) if native_roles => {
                        Some(match role {
                            MenuRole::Copy => "copy",
                            MenuRole::Cut => "cut",
                            MenuRole::Paste => "paste",
                            MenuRole::Undo => "undo",
                            MenuRole::Redo => "redo",
                            MenuRole::SelectAll => "selectAll",
                            MenuRole::Quit => "terminate",
                        })
                    }
                    _ => None,
                };
                if enabled && role.is_none() {
                    commands.insert(id.clone(), on_select);
                }
                let (key, modifiers) = accelerator
                    .and_then(|value| value.parse::<KeyChord>().ok())
                    .map(|chord| {
                        let key = match chord.key() {
                            ShortcutKey::Character(value) => value.to_lowercase().collect(),
                            ShortcutKey::Function(index) => {
                                char::from_u32(0xf703 + u32::from(index))
                                    .map(|value| value.to_string())
                                    .unwrap_or_default()
                            }
                        };
                        let modifiers = chord
                            .modifiers()
                            .iter()
                            .map(|modifier| match modifier {
                                ShortcutModifier::Control => "control",
                                ShortcutModifier::Alt => "option",
                                ShortcutModifier::Shift => "shift",
                                ShortcutModifier::Super | ShortcutModifier::CommandOrControl => {
                                    "command"
                                }
                            })
                            .collect();
                        (key, modifiers)
                    })
                    .unwrap_or_default();
                AppleMenuItem::Command {
                    id,
                    label,
                    enabled,
                    checked: matches!(
                        state,
                        MenuState::Checked(true) | MenuState::Radio { selected: true, .. }
                    ),
                    role,
                    icon: icon.map(|icon| AppleMenuIcon {
                        source: icon.source,
                        template: icon.template,
                    }),
                    key,
                    modifiers,
                }
            }
            MenuItem::Submenu {
                label,
                enabled,
                items,
                icon,
            } => {
                let enabled = parent_enabled && enabled;
                AppleMenuItem::Submenu {
                    label,
                    enabled,
                    icon: icon.map(|icon| AppleMenuIcon {
                        source: icon.source,
                        template: icon.template,
                    }),
                    items: appkit_menu_items(items, enabled, native_roles, commands),
                }
            }
            MenuItem::Separator => AppleMenuItem::Separator,
        })
        .collect()
}

/// Application-owned cleanup, driven by the Apple main run loop after exit is approved.
#[derive(Default)]
pub struct AppleQuitCleanup {
    futures: Vec<AppQuitFuture>,
    deadline: Option<Instant>,
}

impl AppleQuitCleanup {
    /// Capture owner-scoped handlers before the final root is disposed.
    pub fn collect(&mut self) {
        self.futures.extend(take_app_quit_futures());
    }

    /// Return true when cleanup finishes or reaches the shared deadline.
    pub fn poll(&mut self) -> bool {
        if self.futures.is_empty() {
            return true;
        }
        let now = Instant::now();
        let deadline = *self.deadline.get_or_insert(now + APP_QUIT_TIMEOUT);
        if now < deadline && !poll_app_quit_futures(&mut self.futures, Waker::noop()) {
            return false;
        }
        self.futures.clear();
        self.deadline = None;
        true
    }
}

pub use jadix_core::desktop::{Share as AppleShare, ShareOutcome as AppleShareOutcome};

#[derive(serde::Serialize)]
struct ShareRequest<'a> {
    subject: Option<&'a str>,
    text: Option<&'a str>,
    url: Option<&'a str>,
    files: &'a [PathBuf],
    anchor: Option<String>,
    anchor_root: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppleAlertStyle {
    #[default]
    Informational,
    Warning,
    Critical,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct AppleAlertInput {
    pub value: String,
    pub placeholder: Option<String>,
    pub accessible_name: Option<String>,
    /// Select the initial text when the alert first receives focus.
    pub select_text_on_focus: bool,
    /// Require a nonempty value for actions other than Cancel.
    pub required: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct AppleAlert {
    pub title: String,
    pub message: String,
    pub style: AppleAlertStyle,
    /// An empty list uses Apple's OK button.
    pub buttons: Vec<String>,
    /// Zero-based button index to invoke on Return. Defaults to the first button.
    pub default_button: Option<usize>,
    /// Zero-based button index to invoke on Escape.
    pub cancel_button: Option<usize>,
    pub input: Option<AppleAlertInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct AppleAlertResponse {
    /// Zero-based pressed button. Programmatic cancellation has no button.
    pub button: Option<usize>,
    pub input: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum AlertRequest<'a> {
    Show { alert: &'a AppleAlert },
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppleWindowCloseReason {
    CloseButton,
    Shortcut,
    Menu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppleWindowCloseCause {
    ClosePress,
    Escape,
    Blur,
    ParentClose,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AppleWindowEvent {
    ClosePress { reason: AppleWindowCloseReason },
    Closed { cause: AppleWindowCloseCause },
    QuitRequest,
}

#[derive(Default)]
struct RequestIds {
    next: Cell<u64>,
    pending: RefCell<BTreeSet<u64>>,
}

impl RequestIds {
    fn submit(
        &self,
        context: *mut c_void,
        callback: AppleRequestCallback,
        request: &impl serde::Serialize,
    ) -> Result<u64, String> {
        if self.pending.borrow().len() >= 64 {
            return Err("too many pending Apple requests".into());
        }
        let id = self
            .next
            .get()
            .checked_add(1)
            .ok_or("Apple request ID exhausted")?;
        self.next.set(id);
        send_request(context, callback, id, request)?;
        self.pending.borrow_mut().insert(id);
        Ok(id)
    }
}

// The wire includes a child teardown event for the owning root, independent
// of the window's own public events (which also include dismissal).
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WindowEventMessage {
    Owner(DeclaredWindowEvent),
    Window(AppleWindowEvent),
}

#[derive(serde::Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum DeclaredWindowEvent {
    ChildDestroyed {
        id: String,
    },
    ChildState {
        id: String,
        state: Option<AppleWindowState>,
    },
}

/// Synchronizes desired attributes for one open window. Keep one binding for
/// the window's lifetime and use it consistently for the fields it manages.
pub struct AppleWindowBinding {
    id: String,
    applied: AppleWindowAttributes,
    pending: Option<(AppleWindowAttributes, Receiver<AppleWindowResult>)>,
}

impl AppleWindowBinding {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            applied: AppleWindowAttributes::default(),
            pending: None,
        }
    }

    /// Drive the binding on the UI thread, supplying the latest desired attributes
    /// on each call. Returns true when synchronized. Call again after Apple has
    /// had a chance to process requests when this returns false.
    pub fn sync_attributes(
        &mut self,
        host: &mut AppleHost,
        desired: &AppleWindowAttributes,
    ) -> Result<bool, String> {
        if let Some((attributes, result)) = self.pending.take() {
            match result.try_recv() {
                Ok(result) => {
                    result?;
                    self.applied.merge(attributes);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.pending = Some((attributes, result));
                    return Ok(false);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("Apple window update disconnected".into());
                }
            }
        }
        self.applied.validate_update(desired)?;
        let Some(attributes) = self.applied.diff(desired) else {
            return Ok(true);
        };
        let receipt = host.request_window(&AppleWindowRequest::Update {
            id: self.id.clone(),
            attributes: attributes.clone(),
        })?;
        self.pending = Some((attributes, receipt));
        Ok(false)
    }

    /// Submit a mode request. The native window reports the result separately
    /// through its root host's window state.
    pub fn set_mode(
        &self,
        host: &mut AppleHost,
        mode: AppleWindowMode,
    ) -> Result<Receiver<AppleWindowResult>, String> {
        host.request_window(&AppleWindowRequest::SetMode {
            id: self.id.clone(),
            mode,
        })
    }

    /// Restore a minimized window to its prior mode.
    pub fn restore(&self, host: &mut AppleHost) -> Result<Receiver<AppleWindowResult>, String> {
        host.request_window(&AppleWindowRequest::Restore {
            id: self.id.clone(),
        })
    }

    /// Submit a visibility request. Showing a minimized window restores it.
    pub fn set_visible(
        &self,
        host: &mut AppleHost,
        visible: bool,
    ) -> Result<Receiver<AppleWindowResult>, String> {
        host.request_window(&AppleWindowRequest::SetVisibility {
            id: self.id.clone(),
            visible,
        })
    }

    /// Publish observations for this ID to a signal on the target window's root
    /// host. Replaces that host's previous signal binding. If the host is
    /// borrowed from a RefCell, batch around the borrow when binding, updating
    /// window state, and disposing so effects can run after the borrow ends.
    pub fn bind_state(&self, host: &mut AppleHost, state: Signal<Option<AppleWindowState>>) {
        let current = self.observed_state(host).cloned();
        host.window_state_signal = state.try_set(current).then(|| (self.id.clone(), state));
    }

    /// Read the actual state from the target window's root host.
    pub fn observed_state<'a>(&self, host: &'a AppleHost) -> Option<&'a AppleWindowState> {
        host.window_state()
            .filter(|state| state.id.as_deref() == Some(self.id.as_str()))
    }
}

pub use jadix_core::file_request::{FileSelection as AppleFileSelection,
    FilePanelOptions as AppleFilePanelOptions, FileRequest as AppleFileRequest};

#[derive(Clone, Copy)]
pub struct AppleCallbacks {
    pub measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    pub publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    pub clear: unsafe extern "C" fn(*mut c_void),
}

thread_local! {
    static APPLICATION_UPDATER: std::cell::OnceCell<jadix_core::updater::Updater> = const { std::cell::OnceCell::new() };
}
/// Configure once from the packaged application before mounting any roots.
pub fn configure_application_updater(config: Option<jadix_core::updater::Config>, platform: &str, can_install: bool) -> Result<(), String> {
    let can_install = can_install && !cfg!(target_os = "ios");
    let updater = jadix_core::updater::Updater::new(config, platform, can_install)?;
    APPLICATION_UPDATER.with(|slot| slot.set(updater).map_err(|_| "application updater already configured".into()))
}
/// Application lifetime, independent of individual window roots.
pub fn application_updater() -> jadix_core::updater::Updater {
    APPLICATION_UPDATER.with(|slot| slot.get_or_init(Default::default).clone())
}

pub struct AppleHost {
    fetch: jadix_net::service::FetchService,
    context: *mut c_void,
    callbacks: AppleCallbacks,
    focused: Option<NodeId>,
    web: crate::native_web::NativeWeb,
    web_resources: jadix_core::web_resource::WebResources,
    resource_nodes: BTreeMap<u64, NodeId>,
    audio: jadix_core::audio::AudioService,
    speech: jadix_core::speech::SpeechService,
    desktop_services: jadix_core::desktop_services::DesktopServices,
    tray: Option<jadix_core::tray::Tray>,
    tray_revision: u64,
    tray_events: Vec<String>,
    file_request: Option<AppleFileRequestCallback>,
    alert_request: Option<AppleRequestCallback>,
    share_request: Option<AppleRequestCallback>,
    context_menu_request: Option<AppleRequestCallback>,
    window_state: Option<AppleWindowState>,
    window_state_signal: Option<(String, Signal<Option<AppleWindowState>>)>,
    on_window_state_change: Option<Box<dyn FnMut(Option<&AppleWindowState>)>>,
    on_window_event: Option<Box<dyn FnMut(AppleWindowEvent)>>,
    request_ids: Rc<RequestIds>,
    windows: Rc<RefCell<WindowRequests>>,
    pending_file_requests: BTreeMap<u64, Sender<AppleFileResult>>,
    pending_url_requests: BTreeMap<u64, Sender<jadix_core::desktop::OpenUrlResult>>,
    url_router: Option<Box<dyn FnMut(&str) -> Result<bool, String>>>,
    deep_links: jadix_core::deep_link::DeepLinkRouter,
    pending_alerts: BTreeMap<u64, Sender<AppleAlertResult>>,
    pending_shares: BTreeMap<u64, Sender<AppleShareResult>>,
    pending_context_menus: BTreeMap<u64, PendingContextMenu>,
    menu_bar: AppleMenuBar,
    _thread: PhantomData<Rc<()>>,
}

impl AppleHost {
    /// # Safety
    /// The context and callbacks must remain valid on the current UI thread until
    /// this host is dropped. Callbacks must not unwind or retain borrowed buffers.
    /// Measurement writes finite nonnegative width and height to its output.
    /// Its third double is an optional first baseline offset or NaN if absent.
    pub unsafe fn new(context: *mut c_void, callbacks: AppleCallbacks) -> Self {
        let request_ids = Rc::new(RequestIds::default());
        Self {
            fetch: Default::default(),
            context,
            callbacks,
            focused: None,
            web: Default::default(),
            web_resources: Default::default(),
            resource_nodes: BTreeMap::new(),
            audio: Default::default(),
            speech: Default::default(),
            desktop_services: Default::default(),
            tray: None, tray_revision: 0, tray_events: Vec::new(),
            file_request: None,
            alert_request: None,
            share_request: None,
            context_menu_request: None,
            window_state: None,
            window_state_signal: None,
            on_window_state_change: None,
            on_window_event: None,
            windows: Rc::new(RefCell::new(WindowRequests::default())),
            request_ids,
            pending_file_requests: BTreeMap::new(),
            pending_url_requests: BTreeMap::new(),
            url_router: None,
            deep_links: jadix_core::deep_link::DeepLinkRouter::new(Default::default()).expect("default deep link config"),
            pending_alerts: BTreeMap::new(),
            pending_shares: BTreeMap::new(),
            pending_context_menus: BTreeMap::new(),
            menu_bar: AppleMenuBar::default(),
            _thread: PhantomData,
        }
    }

    pub fn speech(&self) -> jadix_core::speech::SpeechService { self.speech.clone() }
    pub fn fetch(&self) -> jadix_net::service::FetchService { self.fetch.clone() }
    /// Configure the root's encoded image cache using the portable loader budget.
    /// Zero bytes or entries disables retention; downloads remain bounded separately.
    pub fn configure_image_cache(&mut self, root: RootId, limits: &crate::images::ImageLimits) -> Result<(), String> {
        if !self.is_alive() || !limits.is_valid() { return Err("invalid image cache configuration".into()); }
        let json = serde_json::json!({"protocol":1,"root":root.token(),"imageCache":{
            "bytes":limits.cache_bytes,"entries":limits.cache_entries,"age":limits.cache_age.as_secs_f64()}});
        let json = CString::new(json.to_string()).map_err(|e| e.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } { Ok(()) }
        else { Err("Apple rejected image cache configuration".into()) }
    }

    pub fn desktop_services(&self) -> jadix_core::desktop_services::DesktopServices { self.desktop_services.clone() }
    pub fn speech_event(&mut self, id: u32, event: jadix_core::speech::SpeechEvent) -> bool { self.speech.event(id, event) }
    pub fn audio(&self) -> jadix_core::audio::AudioService { self.audio.clone() }
    pub fn audio_event(&mut self, id: u32, event: jadix_core::audio::AudioEvent) -> bool { self.audio.event(id, event) }
    pub fn pump_audio(&mut self, root: RootId) {
        for command in self.fetch.take_commands() {
            let id = match &command { jadix_net::service::FetchCommand::Send { id, .. } | jadix_net::service::FetchCommand::Cancel { id } => *id };
            let json = serde_json::json!({"protocol":1,"root":root.token(),"fetch":command});
            let accepted = CString::new(json.to_string()).ok().is_some_and(|json| unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) });
            if !accepted { self.fetch.event(id, jadix_net::service::FetchEvent::Failed { message:"Apple rejected HTTP command".into() }); }
        }
        for command in self.desktop_services.take_commands() {
            use jadix_core::desktop_services::{ServiceCommand, ServiceReply, ServiceError, NotificationAuthorization};
            let id = command.id();
            let permission = matches!(command, ServiceCommand::Permission { .. });
            let auth = matches!(command, ServiceCommand::OpenAuthSession { .. });
            let json = serde_json::json!({"protocol": 1, "root": root.token(), "desktopService": command});
            let accepted = CString::new(json.to_string()).ok().is_some_and(|json| unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) });
            if !accepted { self.desktop_services.reply(id, if permission {
                ServiceReply::Permission { authorization: NotificationAuthorization::Unknown, error: Some(ServiceError::Unavailable) }
            } else if auth { ServiceReply::AuthSession { outcome: None, error: Some(ServiceError::Unavailable) }
            } else { ServiceReply::Complete { error: Some(ServiceError::Unavailable) } }); }
        }
        for command in self.speech.take_commands() {
            let id = match command { jadix_core::speech::SpeechCommand::Start { id, .. } | jadix_core::speech::SpeechCommand::Stop { id } | jadix_core::speech::SpeechCommand::Abort { id } => id };
            let json = serde_json::json!({"protocol": 1, "root": root.token(), "speech": command});
            let accepted = CString::new(json.to_string()).ok().is_some_and(|json| unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) });
            if !accepted { self.speech.event(id, jadix_core::speech::SpeechEvent::End { error: Some(jadix_core::speech::SpeechError::Unavailable) }); }
        }

        for command in self.audio.take_commands() {
            let json = serde_json::json!({"protocol": 1, "root": root.token(), "audio": [command]});
            let accepted = CString::new(json.to_string()).ok().is_some_and(|json| unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) });
            if !accepted { self.audio.event(command.id(), jadix_core::audio::AudioEvent::Failed { message: "Apple rejected audio command".into() }); }
        }
    }
    pub fn tray_command(&mut self, revision: u64, id: &str) -> bool {
        if revision != self.tray_revision || self.tray_events.len() >= 64 || !self.tray.as_ref().is_some_and(|tray| tray.accepts_command(id)) { return false; }
        self.tray_events.push(id.into()); true
    }
    /// Register one application-owned app://authority resource provider. The
    /// built-in app://bundle namespace is reserved for packaged resources.
    pub fn register_web_resources(&mut self, authority: &str, provider: impl Fn(jadix_core::web_resource::ResourceRequest, jadix_core::web_resource::ResourceReply) + Send + Sync + 'static) -> Result<(), &'static str> {
        self.web_resources.register(authority, provider)
    }
    pub fn allows_web_resource(&self, request: &jadix_core::web_resource::ResourceRequest) -> bool { self.web_resources.allows(request) }
    pub fn request_web_resource(&mut self, node: NodeId, request: jadix_core::web_resource::ResourceRequest) -> Option<u64> {
        if !self.web.contains(node) { return None; }
        let id = self.web_resources.request(&node.token(), request)?;
        self.resource_nodes.insert(id, node); Some(id)
    }
    pub fn cancel_web_resource(&mut self, id: u64) { self.resource_nodes.remove(&id); self.web_resources.cancel(id); }
    pub fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> { self.web_resources.take_work() }
    pub fn web_document_ready(&mut self, node: NodeId, ready: bool) {
        self.web.ready(node, ready);
    }
    pub fn complete_web_evaluation(&mut self, request: u64, result: Result<String, String>) -> bool { self.web.complete(request, result) }
    /// The caller must wake returned batches after releasing its application borrow.
    pub fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        use base64::Engine;
        let removed: Vec<_> = self.resource_nodes.iter().filter_map(|(id, node)| (!self.web.contains(*node)).then_some(*id)).collect();
        for id in removed { self.cancel_web_resource(id); }
        for (id, answer) in self.web_resources.take_completed() {
            let Some(node) = self.resource_nodes.remove(&id) else { continue; };
            let answer = match answer {
                Ok(response) => serde_json::json!({"status":response.status,"mime":response.mime,"bytes":base64::engine::general_purpose::STANDARD.encode(response.bytes)}),
                Err(error) => serde_json::json!({"error":error}),
            };
            let command = serde_json::json!({"protocol":1,"root":node.root.token(),"webResource":{"request":id.to_string(),"answer":answer}});
            if let Ok(json) = CString::new(command.to_string()) { unsafe { (self.callbacks.publish)(self.context, json.as_ptr()); } }
        }
        let (work, wake) = self.web.pump();
        for (node, request, script) in work {
            let command = serde_json::json!({"protocol":1, "root":node.root.token(), "webEvaluate":{"node":node.token(), "request":request.to_string(), "script":script}});
            if let Ok(json) = CString::new(command.to_string()) {
                if !unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
                    self.web.complete(request, Err("Apple rejected evaluation".into()));
                }
            }
        }
        wake
    }

    /// # Safety
    /// The callback shares the host constructor's context, thread, and buffer rules.
    /// Deliver results after the callback returns.
    pub unsafe fn with_file_requests(mut self, callback: AppleFileRequestCallback) -> Self {
        self.file_request = Some(callback);
        self
    }

    /// # Safety
    /// The callback shares the host constructor's context, thread, and buffer rules.
    /// Deliver results after the callback returns.
    pub unsafe fn with_window_requests(self, callback: AppleRequestCallback) -> Self {
        let ids = Rc::clone(&self.request_ids);
        let context = self.context;
        self.windows
            .borrow_mut()
            .set_request_handler(move |request| ids.submit(context, callback, request));
        self
    }

    pub fn window_declarations(&self) -> AppleWindowDeclarations {
        WindowRequests::declarations(&self.windows)
    }

    /// # Safety
    /// The callback shares the host constructor's context, thread, and buffer rules.
    /// Deliver results after the callback returns.
    pub unsafe fn with_alert_requests(mut self, callback: AppleRequestCallback) -> Self {
        self.alert_request = Some(callback);
        self
    }

    /// # Safety
    /// The callback shares the host constructor's context, thread, and buffer rules.
    /// Deliver results after the callback returns.
    pub unsafe fn with_share_requests(mut self, callback: AppleRequestCallback) -> Self {
        self.share_request = Some(callback);
        self
    }

    pub fn window_state(&self) -> Option<&AppleWindowState> {
        self.window_state.as_ref()
    }

    /// Subscribe on the UI thread. The current state is available via `window_state`.
    /// A `None` observation indicates the root detached from its window.
    pub fn set_window_state_handler(
        &mut self,
        handler: impl FnMut(Option<&AppleWindowState>) + 'static,
    ) {
        self.on_window_state_change = Some(Box::new(handler));
    }

    /// Apply an observation supplied by the Swift window host.
    pub fn update_window_state(&mut self, json: &CStr) -> Result<(), String> {
        let state: Option<AppleWindowState> =
            serde_json::from_slice(json.to_bytes()).map_err(|error| error.to_string())?;
        if let Some(state) = &state {
            state.validate()?;
        }
        self.publish_window_state(state);
        Ok(())
    }

    /// Subscribe to lifecycle events for the window containing this root.
    pub fn set_window_event_handler(&mut self, handler: impl FnMut(AppleWindowEvent) + 'static) {
        self.on_window_event = Some(Box::new(handler));
    }

    pub fn dispatch_window_event(&mut self, json: &CStr) -> Result<(), String> {
        let event = serde_json::from_slice(json.to_bytes()).map_err(|error| error.to_string())?;
        match event {
            WindowEventMessage::Owner(DeclaredWindowEvent::ChildDestroyed { id }) => {
                batch(|| self.windows.borrow_mut().child_destroyed(&id));
            }
            WindowEventMessage::Owner(DeclaredWindowEvent::ChildState { id, state }) => {
                if let Some(state) = &state {
                    state.validate()?;
                    if state.id.as_deref() != Some(&id) {
                        return Err("Apple child state ID does not match the declaration".into());
                    }
                }
                batch(|| self.windows.borrow_mut().child_state(&id, state));
            }
            WindowEventMessage::Window(event) => {
                if let Some(handler) = &mut self.on_window_event {
                    handler(event);
                }
            }
        }
        Ok(())
    }

    fn publish_window_state(&mut self, state: Option<AppleWindowState>) {
        if self.window_state != state {
            self.window_state = state;
            if let Some((id, signal)) = &self.window_state_signal {
                let observed = self
                    .window_state
                    .as_ref()
                    .filter(|state| state.id.as_deref() == Some(id.as_str()));
                if !signal.try_set(observed.cloned()) {
                    self.window_state_signal = None;
                }
            }
            if let Some(handler) = &mut self.on_window_state_change {
                handler(self.window_state.as_ref());
            }
        }
    }

    /// Submit a file request on the UI thread. Read the receiver with `try_recv`
    /// or wait on a worker thread so Apple can deliver the response.
    pub fn request_file(
        &mut self,
        request: &AppleFileRequest,
    ) -> Result<Receiver<AppleFileResult>, String> {
        request.validate()?;
        let callback = self
            .file_request
            .ok_or("Apple file requests are unavailable")?;
        let id = self.submit_request(callback, request)?;
        let (sender, receiver) = mpsc::channel();
        self.pending_file_requests.insert(id, sender);
        Ok(receiver)
    }

    pub fn complete_file_request(&mut self, id: u64, result: &CStr) -> Result<(), String> {
        if let Some(sender) = self.pending_url_requests.remove(&id) {
            self.request_ids.pending.borrow_mut().remove(&id);
            let result = serde_json::from_slice::<AppleFileResult>(result.to_bytes())
                .map_err(|error| error.to_string())
                .and_then(|result| result)
                .map(|_| true);
            let _ = sender.send(result);
            return Ok(());
        }
        let sender = self
            .pending_file_requests
            .remove(&id)
            .ok_or("unknown Apple file request")?;
        self.request_ids.pending.borrow_mut().remove(&id);
        let result = serde_json::from_slice::<AppleFileResult>(result.to_bytes())
            .unwrap_or_else(|error| Err(format!("invalid Apple file result: {error}")));
        let _ = sender.send(result);
        Ok(())
    }

    pub fn receive_deep_link(&mut self, link: jadix_core::deep_link::DeepLink) -> Result<(), String> {
        link.validate(&self.deep_links.config().schemes)?;
        if let Some(router) = &mut self.url_router { if router(&link.url)? { return Ok(()); } }
        self.deep_links.accept(link).map_err(String::from)
    }

    /// Handle validated external links in the application's router first.
    /// Returning false falls back to NSWorkspace. Errors never open the URL.
    pub fn set_url_router(&mut self, router: impl FnMut(&str) -> Result<bool, String> + 'static) {
        self.url_router = Some(Box::new(router));
    }

    pub fn open_url(
        &mut self,
        url: &str,
    ) -> Result<Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        let parsed = jadix_core::desktop::check_open_url(url)?;
        if let Some(router) = &mut self.url_router {
            if router(url)? {
                let (sender, receiver) = mpsc::channel();
                let _ = sender.send(Ok(true));
                return Ok(receiver);
            }
        }
        let callback = self
            .file_request
            .ok_or("Apple URL opening is unavailable")?;
        let id = self.submit_request(
            callback,
            &serde_json::json!({"operation":"open_url", "url":parsed.as_str()}),
        )?;
        let (sender, receiver) = mpsc::channel();
        self.pending_url_requests.insert(id, sender);
        Ok(receiver)
    }

    /// Submit a window request on the UI thread. Read the receiver with
    /// `try_recv` or move it to a worker thread.
    pub fn request_window(
        &mut self,
        request: &AppleWindowRequest,
    ) -> Result<Receiver<AppleWindowResult>, String> {
        self.windows
            .borrow_mut()
            .request(request)
            .map(|(_, receiver)| receiver)
    }

    pub fn complete_window_request(&mut self, id: u64, result: &CStr) -> Result<(), String> {
        self.request_ids.pending.borrow_mut().remove(&id);
        let result = serde_json::from_slice::<AppleWindowResult>(result.to_bytes())
            .unwrap_or_else(|error| Err(format!("invalid Apple window result: {error}")));
        self.windows.borrow_mut().complete(id, result)
    }

    /// Show a system alert as a sheet or as a standalone modal alert.
    /// Read the response with
    /// `try_recv` on the UI thread or move the receiver to a worker thread.
    pub fn show_alert(
        &mut self,
        alert: &AppleAlert,
    ) -> Result<(u64, Receiver<AppleAlertResult>), String> {
        let button_count = alert.buttons.len().max(1);
        if alert
            .cancel_button
            .is_some_and(|index| index >= button_count)
        {
            return Err("invalid Apple alert cancel button".into());
        }
        if alert
            .default_button
            .is_some_and(|index| index >= button_count || Some(index) == alert.cancel_button)
        {
            return Err("invalid Apple alert default button".into());
        }
        let callback = self.alert_request.ok_or("Apple alerts are unavailable")?;
        let id = self.submit_request(callback, &AlertRequest::Show { alert })?;
        let (sender, receiver) = mpsc::channel();
        self.pending_alerts.insert(id, sender);
        Ok((id, receiver))
    }

    /// Ask the host to dismiss the alert identified by `show_alert`.
    pub fn close_alert(&mut self, id: u64) -> Result<(), String> {
        if !self.pending_alerts.contains_key(&id) {
            return Err("unknown Apple alert".into());
        }
        let callback = self.alert_request.ok_or("Apple alerts are unavailable")?;
        send_request(self.context, callback, id, &AlertRequest::Close)
    }

    pub fn complete_alert(&mut self, id: u64, result: &CStr) -> Result<(), String> {
        let sender = self
            .pending_alerts
            .remove(&id)
            .ok_or("unknown Apple alert")?;
        self.request_ids.pending.borrow_mut().remove(&id);
        let result = serde_json::from_slice::<AppleAlertResult>(result.to_bytes())
            .unwrap_or_else(|error| Err(format!("invalid Apple alert result: {error}")));
        let _ = sender.send(result);
        Ok(())
    }

    /// Open the system share picker. Use `try_recv` on the UI thread.
    pub fn show_share(
        &mut self,
        share: &AppleShare,
    ) -> Result<Receiver<AppleShareResult>, String> {
        share.validate()?;
        let text = share.text.as_deref().filter(|text| !text.is_empty());
        let url = share.url.as_deref();
        let callback = self.share_request.ok_or("Apple sharing is unavailable")?;
        let id = self.submit_request(
            callback,
            &ShareRequest {
                subject: share.subject.as_deref(),
                text,
                url,
                files: &share.files,
                anchor: share.anchor.map(NodeId::token),
                anchor_root: share.anchor.map(|node| node.root.token()),
            },
        )?;
        let (sender, receiver) = mpsc::channel();
        self.pending_shares.insert(id, sender);
        Ok(receiver)
    }

    pub fn complete_share(&mut self, id: u64, result: &CStr) -> Result<(), String> {
        let sender = self
            .pending_shares
            .remove(&id)
            .ok_or("unknown Apple share request")?;
        self.request_ids.pending.borrow_mut().remove(&id);
        let result = serde_json::from_slice::<AppleShareResult>(result.to_bytes())
            .unwrap_or_else(|error| Err(format!("invalid Apple share result: {error}")));
        let _ = sender.send(result);
        Ok(())
    }

    /// # Safety
    /// The callback uses the host context and must deliver its result after returning.
    pub unsafe fn set_context_menu_request(&mut self, callback: AppleRequestCallback) {
        self.context_menu_request = Some(callback);
    }

    /// Show a native menu at a mounted node. `Ok(None)` means it was dismissed.
    pub fn show_context_menu(
        &mut self,
        anchor: NodeId,
        mut items: Vec<MenuItem>,
        locale: &str,
    ) -> Result<Receiver<AppleContextMenuResult>, String> {
        let callback = self
            .context_menu_request
            .ok_or("Apple context menus are unavailable")?;
        if items.is_empty() {
            return Err("Apple context menu is empty".into());
        }
        menu::localize(&mut items, locale)?;
        let mut commands = BTreeMap::new();
        let items = appkit_menu_items(items, true, false, &mut commands);
        let id = self.submit_request(
            callback,
            &serde_json::json!({
                "anchor": anchor.token(),
                "anchor_root": anchor.root.token(),
                "items": items,
            }),
        )?;
        let (sender, receiver) = mpsc::channel();
        self.pending_context_menus
            .insert(id, PendingContextMenu { sender, commands });
        Ok(receiver)
    }

    pub fn complete_context_menu(&mut self, id: u64, result: &CStr) -> Result<(), String> {
        let pending = self
            .pending_context_menus
            .remove(&id)
            .ok_or("unknown Apple context menu request")?;
        self.request_ids.pending.borrow_mut().remove(&id);
        let result = serde_json::from_slice::<AppleContextMenuResult>(result.to_bytes())
            .unwrap_or_else(|error| Err(format!("invalid Apple context menu result: {error}")))
            .and_then(|selection| {
                if let Some(id) = &selection {
                    let select = pending
                        .commands
                        .get(id)
                        .ok_or("unknown or disabled Apple menu command")?;
                    select();
                }
                Ok(selection)
            });
        let _ = pending.sender.send(result);
        Ok(())
    }

    /// Replace this root's menu bar. The key window supplies the application menu.
    /// `window_menu` identifies a top-level submenu by its zero-based index.
    pub fn set_window_menu(
        &mut self,
        root: RootId,
        mut items: Vec<MenuItem>,
        locale: &str,
        window_menu: Option<usize>,
    ) -> Result<(), String> {
        menu::localize(&mut items, locale)?;
        if window_menu
            .is_some_and(|index| !matches!(items.get(index), Some(MenuItem::Submenu { .. })))
        {
            return Err("Apple Window menu must name a top-level submenu".into());
        }
        let revision = self
            .menu_bar
            .revision
            .checked_add(1)
            .ok_or("Apple menu revision exhausted")?;
        let mut commands = BTreeMap::new();
        let items = appkit_menu_items(items, true, true, &mut commands);
        let json = serde_json::json!({
            "protocol": PROTOCOL_VERSION,
            "root": root.token(),
            "menuBar": { "revision": revision, "windowMenu": window_menu, "items": items },
        });
        let json = CString::new(json.to_string()).map_err(|error| error.to_string())?;
        if !unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            return Err("Apple rejected the window menu".into());
        }
        self.menu_bar = AppleMenuBar { revision, commands };
        Ok(())
    }

    /// Clear this root's menu bar and use the host menu.
    pub fn clear_window_menu(&mut self, root: RootId) -> Result<(), String> {
        self.set_window_menu(root, Vec::new(), "", None)
    }

    /// Resolve a command for the installed revision. Invoke it after releasing
    /// the root borrow so the command can call back into the host.
    pub fn menu_command(&self, revision: u64, id: &str) -> Option<Rc<dyn Fn()>> {
        if self.menu_bar.revision != revision {
            return None;
        }
        self.menu_bar.commands.get(id).cloned()
    }

    fn submit_request(
        &mut self,
        callback: AppleRequestCallback,
        request: &impl serde::Serialize,
    ) -> Result<u64, String> {
        self.request_ids.submit(self.context, callback, request)
    }
}

fn send_request(
    context: *mut c_void,
    callback: AppleRequestCallback,
    id: u64,
    request: &impl serde::Serialize,
) -> Result<(), String> {
    let json = serde_json::to_string(request).map_err(|error| error.to_string())?;
    let json = CString::new(json).map_err(|error| error.to_string())?;
    if !unsafe { callback(context, id, json.as_ptr()) } {
        return Err("Apple refused the request".into());
    }
    Ok(())
}

impl PlatformHost for AppleHost {
    fn request_file(&mut self, request: &jadix_core::file_request::FileRequest) -> Result<Receiver<jadix_core::file_request::FileResult>, String> { AppleHost::request_file(self, request) }
    fn fetch(&self) -> Option<jadix_net::service::FetchService> { Some(self.fetch.clone()) }
    fn set_tray(&mut self, root: RootId, tray: Option<&jadix_core::tray::Tray>) -> Result<(), String> {
        if let Some(tray) = tray { tray.validate()?; }
        let revision = self.tray_revision.checked_add(1).ok_or("tray revision exhausted")?;
        let json = CString::new(serde_json::json!({"protocol":1,"root":root.token(),"tray":{"revision":revision,"content":tray}}).to_string()).map_err(|e| e.to_string())?;
        if !unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } { return Err("Apple rejected status item".into()); }
        self.tray = tray.cloned(); self.tray_revision = revision; self.tray_events.clear(); Ok(())
    }
    fn take_tray_events(&mut self) -> Vec<String> { std::mem::take(&mut self.tray_events) }

    fn show_share(&mut self, share: &AppleShare) -> Result<Receiver<AppleShareResult>, String> {
        AppleHost::show_share(self, share)
    }

    fn open_url(
        &mut self,
        url: &str,
    ) -> Result<Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        AppleHost::open_url(self, url)
    }

    fn window_declarations(&self) -> Option<AppleWindowDeclarations> {
        Some(AppleHost::window_declarations(self))
    }

    fn request_window(
        &mut self,
        request: &AppleWindowRequest,
    ) -> Result<Receiver<AppleWindowResult>, String> {
        AppleHost::request_window(self, request)
    }

    fn capabilities(&self) -> capabilities::BackendCapabilities {
        let supported = capabilities::BackendCapabilities::headless();
        capabilities::BackendCapabilities {
            inline_raster_sources: true,
            inline_svg_sources: true,
            local_image_sources: true,
            remote_image_sources: true,
            ..supported
        }
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let json = CString::new(crate::wire::measurement(request, descriptor))
            .map_err(|_| LayoutError::MeasurementUnavailable(request.key.node))?;
        let mut output = [f64::NAN; 3];
        if !unsafe { (self.callbacks.measure)(self.context, json.as_ptr(), output.as_mut_ptr()) } {
            return Err(LayoutError::MeasurementUnavailable(request.key.node));
        }
        let [width, height, baseline] = output;
        if [width, height]
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
            || (!baseline.is_nan()
                && (!baseline.is_finite() || baseline < 0.0 || baseline > height))
        {
            return Err(LayoutError::InvalidMeasurement(request.key.node));
        }
        let height = height.min(request.key.max_height);
        Ok(MeasureResult {
            key: request.key,
            width: width.min(request.key.max_width),
            height,
            baseline: (baseline <= height).then_some(baseline),
        })
    }

    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String> {
        let json = CString::new(crate::wire::snapshot(root, revision, roots))
            .map_err(|error| error.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            self.web.sync(roots);
            Ok(())
        } else {
            Err("Apple rejected native commit".into())
        }
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        let focus = batch.operations.iter().rev().find_map(|operation| {
            if let Operation::Focus { node } = operation {
                Some(*node)
            } else {
                None
            }
        });
        let next_focus = focus.unwrap_or(self.focused);
        let mut snapshot = serde_json::from_str::<serde_json::Value>(&crate::wire::snapshot(
            batch.root,
            batch.revision,
            roots,
        ))
        .map_err(|error| error.to_string())?;
        snapshot["focused"] = next_focus
            .map(|node| serde_json::Value::String(node.token()))
            .unwrap_or(serde_json::Value::Null);
        snapshot["focusChanged"] = serde_json::Value::Bool(focus.is_some());
        let json = CString::new(snapshot.to_string()).map_err(|error| error.to_string())?;
        if !unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            return Err("Apple rejected native commit".into());
        }
        self.web.sync(roots);
        self.focused = next_focus;
        Ok(ApplyReceipt::for_batch(batch))
    }

    fn stop_scroll(&mut self, node: NodeId) -> Result<(), String> {
        let json = CString::new(serde_json::json!({"protocol":1,"root":node.root.token(),"stopScroll":{"node":node.token()}}).to_string()).map_err(|e| e.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } { Ok(()) }
        else { Err("Apple rejected scroll stop".into()) }
    }

    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        if offset
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("invalid Apple scroll offset".into());
        }
        let command = serde_json::json!({
            "protocol": 1,
            "root": node.root.token(),
            "scrollTo": { "node": node.token(), "offset": offset },
        });
        let json = CString::new(command.to_string()).map_err(|error| error.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            Ok(())
        } else {
            Err("Apple rejected the scroll target".into())
        }
    }

    fn zoom_to(&mut self, node: NodeId, scale: f64, anchor: [f64; 2]) -> Result<(), String> {
        if !scale.is_finite() || !(0.01..=100.0).contains(&scale) || anchor.into_iter().any(|v| !v.is_finite()) {
            return Err("invalid Apple viewport magnification".into());
        }
        let command = serde_json::json!({"protocol": 1, "root": node.root.token(),
            "zoomTo": {"node": node.token(), "scale": scale, "anchor": anchor}});
        let json = CString::new(command.to_string()).map_err(|e| e.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } { Ok(()) }
        else { Err("Apple rejected the zoom target".into()) }
    }

    fn evaluate_web(&mut self, node: NodeId, script: &str) -> jadix_core::web_evaluation::WebEvaluation { self.web.evaluate(node, script) }

    fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> { AppleHost::pump_web(self) }
    fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> { AppleHost::take_resource_work(self) }
    fn speech(&self) -> Option<jadix_core::speech::SpeechService> { Some(self.speech.clone()) }
    fn updater(&self) -> Option<jadix_core::updater::Updater> { Some(application_updater()) }
    fn desktop_services(&self) -> Option<jadix_core::desktop_services::DesktopServices> { Some(self.desktop_services.clone()) }
    fn audio(&self) -> Option<jadix_core::audio::AudioService> { Some(self.audio.clone()) }
    fn pump_audio(&mut self, root: RootId) { AppleHost::pump_audio(self, root); }

    fn dispose(&mut self) {
        self.desktop_services.dispose();
        self.speech.dispose();
        self.audio.dispose();
        self.web.dispose();
        self.web_resources.dispose(); self.resource_nodes.clear();
        self.file_request = None;
        self.url_router = None;
        self.deep_links.drain();
        for (_, sender) in std::mem::take(&mut self.pending_url_requests) {
            let _ = sender.send(Err("Apple root was disposed".into()));
        }
        batch(|| self.windows.borrow_mut().dispose());
        self.request_ids.pending.borrow_mut().clear();
        self.alert_request = None;
        self.share_request = None;
        self.context_menu_request = None;
        self.menu_bar = AppleMenuBar::default();
        for (_, sender) in std::mem::take(&mut self.pending_file_requests) {
            let _ = sender.send(Err("Apple root was disposed".into()));
        }
        for (_, sender) in std::mem::take(&mut self.pending_alerts) {
            let _ = sender.send(Err("Apple root was disposed".into()));
        }
        for (_, sender) in std::mem::take(&mut self.pending_shares) {
            let _ = sender.send(Err("Apple root was disposed".into()));
        }
        for (_, pending) in std::mem::take(&mut self.pending_context_menus) {
            let _ = pending.sender.send(Err("Apple root was disposed".into()));
        }
        self.publish_window_state(None);
        self.window_state_signal = None;
        self.on_window_state_change = None;
        self.on_window_event = None;
        unsafe { (self.callbacks.clear)(self.context) };
    }

    fn configure_deep_links(&mut self, config: jadix_core::deep_link::DeepLinkConfig) -> Result<(), String> { self.deep_links.configure(config).map_err(String::from) }
    fn take_deep_links(&mut self) -> Vec<jadix_core::deep_link::DeepLink> { self.deep_links.drain() }

    fn set_app_address(&mut self, root: RootId, address: Option<&str>) -> Result<(), String> {
        let command = serde_json::json!({
            "protocol": PROTOCOL_VERSION,
            "root": root.token(),
            "appAddress": address,
        });
        let json = CString::new(command.to_string()).map_err(|error| error.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            Ok(())
        } else {
            Err("Apple rejected the app address".into())
        }
    }
}

impl WindowHost for AppleHost {
    fn window_declarations(&self) -> AppleWindowDeclarations {
        AppleHost::window_declarations(self)
    }
}
/// Sanitize resource-bearing HTML before invoking an OS text importer.
pub fn sanitize_rich_html(html: &str) -> Result<String, String> {
    if html.len() > jadix_core::rich_editor::RichEditorSpec::MAX_HTML || html.contains('\0') {
        return Err("invalid rich editor HTML".into());
    }
    let result = jadix_document::native_html::sanitize_native_html(html).map_err(str::to_owned)?;
    if result.len() > jadix_core::rich_editor::RichEditorSpec::MAX_HTML {
        return Err("rich editor HTML exceeds budget".into());
    }
    Ok(result)
}

/// Separate resources before handing rich text to the system HTML importer.
pub fn rich_html_plan(html: &str) -> Result<String, String> {
    let plan = jadix_document::native_html::prepare_native_html(html).map_err(str::to_owned)?;
    serde_json::to_string(&plan).map_err(|error| error.to_string())
}
pub fn rich_image_dimensions(width: Option<f64>, height: Option<f64>, natural: Option<[f64; 2]>) -> [f64; 2] {
    jadix_document::native_html::image_dimensions(width, height, natural)
}
