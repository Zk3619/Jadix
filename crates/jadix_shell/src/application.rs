//! A single existing budgeted QuickJS runtime with host-independent mount/event IO.
//! Bundle JS/TSX with the SDK's esbuild toolchain; no second interpreter is used.
use crate::{
    mount::{Mount, MountError},
    runtime::{
        native::{BudgetedRuntime, VmError},
        ResourceLimits,
    },
    wire::{Command, ComponentRegistry, Envelope},
    PackageManifest, Session, ShellError,
};
use jadix_core::presentation::{
    CommitBatch, NodeContent, Operation, PlatformEvent, PresentationNode, RootLifetime,
};
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Debug)]
pub enum ApplicationError {
    Session(ShellError),
    Vm(VmError),
    Mount(MountError),
    Host(String),
    Protocol,
}
impl std::fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ApplicationError {}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum BridgeRequest {
    Files { key: String, request: ScriptFiles },
    Clipboard { key: String, request: jadix_core::clipboard::ClipboardRequest },
    DeepLinks { key: String, request: ScriptDeepLinks },
    Identity,
    VirtualRange { count: usize, extent: f64, #[serde(default)] gap: f64, offset: f64, viewport: f64, #[serde(default)] overscan: usize, focus: Option<usize>, align: Option<String> },
    EmojiSearch { query: jadix_core::emoji::EmojiQuery },
    EmojiEntry { query: jadix_core::emoji::EmojiQuery, emoji: String },
    EmojiLayout { query: jadix_core::emoji::EmojiQuery, columns: usize, headers: Option<Vec<u8>> },
    EmojiLocate { query: jadix_core::emoji::EmojiQuery, columns: usize, emoji: String, headers: Option<Vec<u8>> },
    EmojiNavigate { query: jadix_core::emoji::EmojiQuery, columns: usize, current: Option<usize>, key: jadix_core::presentation::NavigationKey, #[serde(default)] rtl: bool, #[serde(default = "emoji_page_rows")] page_rows: usize, headers: Option<Vec<u8>> },
    Fetch { key: String, request: ScriptFetch },
    WebSocket { key: String, request: ScriptWebSocket },
    Updater { key: String, request: ScriptUpdater },
    #[cfg(feature = "i18n")]
    I18n { request: jadix_core::i18n::request::LocaleRequest },
    Share {
        key: String,
        options: ScriptShare,
    },
    OpenUrl {
        key: String,
        url: String,
    },
    AuthSession { key: String, request: ScriptAuthSession },
    Haptic { key: String, feedback: jadix_core::desktop_services::Haptic },
    Notifications { key: String, request: ScriptNotifications },
    GlobalShortcut { key: String, request: ScriptGlobalShortcut },
    Performance { key: String },
    Speech { key: String, request: ScriptSpeech },
    Audio { key: String, request: ScriptAudio },
    SetTray { key: String, tray: Option<jadix_core::tray::Tray> },
    TrayEvents { key: String },
    EvaluateWeb { key: String, path: Vec<usize>, script: String },
    StopScroll { key: String, path: Vec<usize> },
    ZoomTo { key: String, path: Vec<usize>, scale: f64, anchor: [f64; 2] },
    Register {
        component: String,
        event: String,
        key: String,
    },
    Release {
        key: String,
    },
    Mount {
        tree: crate::wire::ElementSpec,
    },
    Patch {
        patches: Vec<crate::mount::LeafPatch>,
    },
    Window {
        id: String,
        key: String,
        options: Option<jadix_core::window::DeclaredWindowOptions>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptShare {
    subject: Option<String>,
    text: Option<String>,
    url: Option<String>,
    #[serde(default)]
    files: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum ScriptFiles {
    PickPaths { #[serde(default)] selection: jadix_core::file_request::FileSelection, #[serde(default)] multiple: bool, #[serde(default)] options: jadix_core::file_request::FilePanelOptions },
    PickSavePath { file_name: Option<String>, #[serde(default)] options: jadix_core::file_request::FilePanelOptions },
    Open { handle: String }, Reveal { handle: String }, Trash { handle: String }, Release { handle: String },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptDeepLinks {
    Configure { config: jadix_core::deep_link::DeepLinkConfig }, Events,
    Activation { config: jadix_core::deep_link::ApplicationActivation }, Launches,
    Address { address: Option<String> },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptUpdater {
    Check { channel: Option<String>, locale: Option<String> }, Snapshot, Download,
    Install { #[serde(default)] restart: bool }, Cancel,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptWebSocket {
    Connect { request: jadix_net::websocket::SocketRequest },
    Send { id: u32, message: jadix_net::websocket::SocketMessage },
    Events { id: u32 }, Close { id: u32, code: u16, reason: String }, Release { id: u32 },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptFetch {
    Send { request: jadix_net::service::FetchRequest },
    Events { id: u32 }, Cancel { id: u32 }, Release { id: u32 },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptAudio {
    Load { source: jadix_core::audio::AudioSource },
    Open { source: jadix_core::audio::AudioSource, #[serde(default)] settings: jadix_core::audio::PlayerSettings },
    Play { id: u32, #[serde(default = "audio_one")] volume: f32, #[serde(default = "audio_one")] rate: f32 },
    Stop { id: u32 }, Preload { id: u32 }, Resume { id: u32 }, Pause { id: u32 },
    Seek { id: u32, position: f64 }, Set { id: u32, settings: jadix_core::audio::PlayerSettings },
    Snapshot { id: u32 }, Events { id: u32 }, Release { id: u32 },
}
fn audio_one() -> f32 { 1. }
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptSpeech {
    Start { #[serde(default)] request: jadix_core::speech::SpeechRequest },
    Stop { id: u32 }, Abort { id: u32 }, Events { id: u32 }, Release { id: u32 },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptAuthSession {
    Open { request: jadix_core::desktop_services::AuthSessionRequest }, Close,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptNotifications {
    Permission { #[serde(default)] request: bool },
    Notify { notification: jadix_core::desktop_services::Notification },
    Badge { label: Option<String> },
    Reply { id: u32 }, Events { id: u32 }, Release { id: u32 }, Activations,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScriptGlobalShortcut {
    Register { shortcut: jadix_core::desktop_services::GlobalShortcut },
    Reply { id: u32 }, Events { id: u32 }, Release { id: u32 },
}

pub(crate) enum DesktopRequest {
    Files { key: String, request: jadix_core::file_request::FileRequest },
    ReleaseFile { key: String, handle: String },
    Clipboard { key: String, request: jadix_core::clipboard::ClipboardRequest },
    DeepLinks { key: String, request: ScriptDeepLinks },
    Fetch { key: String, request: ScriptFetch },
    WebSocket { key: String, request: ScriptWebSocket },
    Updater { key: String, request: ScriptUpdater },
    AuthSession { key: String, request: ScriptAuthSession },
    Haptic { key: String, feedback: jadix_core::desktop_services::Haptic },
    Notifications { key: String, request: ScriptNotifications },
    GlobalShortcut { key: String, request: ScriptGlobalShortcut },
    Performance { key: String },
    Speech { key: String, request: ScriptSpeech },
    Audio { key: String, request: ScriptAudio },
    SetTray { key: String, tray: Option<jadix_core::tray::Tray> },
    TrayEvents { key: String },
    EvaluateWeb { key: String, node: jadix_core::presentation::NodeId, script: String },
    StopScroll { key: String, node: jadix_core::presentation::NodeId },
    ZoomTo { key: String, node: jadix_core::presentation::NodeId, scale: f64, anchor: [f64; 2] },
    Share {
        key: String,
        share: jadix_core::desktop::Share,
    },
    OpenUrl {
        key: String,
        url: String,
    },
}

pub(crate) struct ScriptWindowUpdate {
    pub id: String,
    pub key: String,
    pub options: Option<jadix_core::window::DeclaredWindowOptions>,
}
enum Pending {
    Mount(Envelope),
    Patch(Vec<crate::mount::LeafPatch>),
}
// Borrow snapshots only for structural mounts. Leaf publication carries its
// existing rollback journal, so no untouched descriptor is cloned or compared.
enum Publication<'a> {
    Mount {
        before: &'a [PresentationNode],
        after: &'a [PresentationNode],
    },
    Leaves {
        before: &'a [(Vec<usize>, NodeContent)],
        after: &'a [PresentationNode],
    },
}
impl Publication<'_> {
    fn roots(&self) -> &[PresentationNode] {
        match self {
            Self::Mount { after, .. } | Self::Leaves { after, .. } => after,
        }
    }
}
type Deliveries = Rc<RefCell<VecDeque<(String, crate::schema::Value)>>>;
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct ApplicationWork {
    pub message_bytes: u64,
    pub full_mounts: u64,
    pub leaf_patches: u64,
    pub event_lookups: u64,
}
struct State {
    emoji_search: jadix_core::emoji::EmojiSearch,
    #[cfg(feature = "i18n")]
    locale: Rc<jadix_core::i18n::LocaleContext>,
    session: Session,
    registry: ComponentRegistry,
    mount: Mount,
    pending: VecDeque<Pending>,
    handles: BTreeMap<String, crate::handles::Handle>,
    deliveries: Deliveries,
    failed: bool,
    work: ApplicationWork,
    window_permission: bool,
    permissions: BTreeSet<String>,
    desktop_requests: VecDeque<DesktopRequest>,
    desktop_keys: BTreeSet<String>,
    share_files: BTreeMap<String, std::path::PathBuf>,
    next_file: u64,
    window_updates: VecDeque<ScriptWindowUpdate>,
    window_ids: BTreeSet<String>,
}
impl State {
    fn request(&mut self, request: BridgeRequest) -> Result<serde_json::Value, ShellError> {
        if self.failed || self.mount.lifetime().is_disposed() {
            return Err(ShellError::InvalidState);
        }
        match request {
            BridgeRequest::Clipboard { key, request } => {
                use jadix_core::clipboard::ClipboardRequest as R;
                let permission = match &request { R::Read { .. } | R::Formats => "desktop.clipboard.read", R::Write { .. } | R::Clear => "desktop.clipboard.write" };
                if !self.permissions.contains(permission) { return Err(ShellError::PermissionDenied); }
                request.validate().map_err(|_| ShellError::InvalidArguments)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Clipboard { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Files { key, request } => {
                use jadix_core::file_request::FileRequest as R;
                let permission = match &request { ScriptFiles::PickPaths { .. } | ScriptFiles::PickSavePath { .. } => "desktop.files.pick", ScriptFiles::Open { .. } => "desktop.files.open", ScriptFiles::Reveal { .. } => "desktop.files.reveal", ScriptFiles::Trash { .. } => "desktop.files.trash", ScriptFiles::Release { .. } => "desktop.files.pick" };
                if !self.permissions.contains(permission) { return Err(ShellError::PermissionDenied); }
                let path = |handle: &str| self.share_files.get(handle).cloned().ok_or(ShellError::StaleHandle);
                let options = |mut options: jadix_core::file_request::FilePanelOptions| -> Result<_, ShellError> {
                    if let Some(directory) = &options.directory { options.directory = Some(path(directory.to_str().ok_or(ShellError::InvalidArguments)?)?); }
                    Ok(options)
                };
                let request = match request {
                    ScriptFiles::PickPaths { selection, multiple, options: panel } => R::PickPaths { selection, multiple, options: options(panel)? },
                    ScriptFiles::PickSavePath { file_name, options: panel } => R::PickSavePath { file_name, options: options(panel)? },
                    ScriptFiles::Open { handle } => R::Open { path: path(&handle)? },
                    ScriptFiles::Reveal { handle } => R::Reveal { path: path(&handle)? },
                    ScriptFiles::Trash { handle } => R::Trash { path: path(&handle)? },
                    ScriptFiles::Release { handle } => {
                        path(&handle)?;
                        self.admit_desktop_key(&key)?;
                        self.desktop_requests.push_back(DesktopRequest::ReleaseFile { key, handle });
                        return Ok(serde_json::Value::Null);
                    }
                };
                request.validate().map_err(|_| ShellError::InvalidArguments)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Files { key, request });
                Ok(serde_json::Value::Null)
            }
            #[cfg(feature = "i18n")]
            BridgeRequest::I18n { request } => request.execute(&self.locale).map_err(|_| ShellError::InvalidArguments),
            BridgeRequest::AuthSession { key, request } => {
                let scheme = match &request {
                    ScriptAuthSession::Open { request } => {
                        if !request.validate() { return Err(ShellError::InvalidArguments); }
                        jadix_core::desktop::check_open_url(&request.url).map_err(|_| ShellError::InvalidArguments)?.scheme().to_owned()
                    }
                    ScriptAuthSession::Close => String::new(),
                };
                if !self.permissions.contains("desktop.open_url") && if scheme.is_empty() {
                    !self.permissions.contains("desktop.open_url.http") && !self.permissions.contains("desktop.open_url.https")
                } else { !self.permissions.contains(&format!("desktop.open_url.{scheme}")) } { return Err(ShellError::PermissionDenied); }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::AuthSession { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Haptic { key, feedback } => {
                if !self.permissions.contains("device.haptics") { return Err(ShellError::PermissionDenied); }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Haptic { key, feedback });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Notifications { key, request } => {
                if !self.permissions.contains("desktop.notifications") { return Err(ShellError::PermissionDenied); }
                match &request {
                    ScriptNotifications::Notify { notification } if !notification.validate() => return Err(ShellError::InvalidArguments),
                    ScriptNotifications::Badge { label: Some(label) } if label.len() > 64 || label.contains('\0') => return Err(ShellError::InvalidArguments),
                    _ => {}
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Notifications { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::GlobalShortcut { key, request } => {
                if !self.permissions.contains("desktop.global_shortcuts") { return Err(ShellError::PermissionDenied); }
                if let ScriptGlobalShortcut::Register { shortcut } = &request {
                    if !shortcut.validate() { return Err(ShellError::InvalidArguments); }
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::GlobalShortcut { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::DeepLinks { key, request } => {
                if !self.permissions.contains("desktop.deep_link") { return Err(ShellError::PermissionDenied); }
                if let ScriptDeepLinks::Configure { config } = &request { config.validate().map_err(|_| ShellError::InvalidArguments)?; }
                if let ScriptDeepLinks::Address { address: Some(address) } = &request { jadix_core::deep_link::validate_app_address(address).map_err(|_| ShellError::InvalidArguments)?; }
                if let ScriptDeepLinks::Activation { config } = &request { config.validate().map_err(|_| ShellError::InvalidArguments)?; }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::DeepLinks { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Performance { key } => {
                if !self.permissions.contains("desktop.performance") { return Err(ShellError::PermissionDenied); }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Performance { key });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Updater { key, request } => {
                if !self.permissions.contains("desktop.updater") { return Err(ShellError::PermissionDenied); }
                if let ScriptUpdater::Check { channel, locale } = &request {
                    if channel.as_ref().is_some_and(|s| s.len() > 256) || locale.as_ref().is_some_and(|s| s.len() > 64) { return Err(ShellError::InvalidArguments); }
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Updater { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::WebSocket { key, request } => {
                if !self.permissions.contains("network.websocket") { return Err(ShellError::PermissionDenied); }
                match &request {
                    ScriptWebSocket::Connect { request } => request.validate().map_err(|_| ShellError::InvalidArguments)?,
                    ScriptWebSocket::Send { message, .. } if message.len() > jadix_net::websocket::MAX_BYTES => return Err(ShellError::InvalidArguments),
                    ScriptWebSocket::Close { code, reason, .. } => jadix_net::websocket::validate_close(*code, reason).map_err(|_| ShellError::InvalidArguments)?,
                    _ => {}
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::WebSocket { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Fetch { key, request } => {
                if !self.permissions.contains("network.fetch") { return Err(ShellError::PermissionDenied); }
                if let ScriptFetch::Send { request } = &request { request.validate().map_err(|_| ShellError::InvalidArguments)?; }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Fetch { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Speech { key, request } => {
                if !self.permissions.contains("speech.recognition") { return Err(ShellError::PermissionDenied); }
                if let ScriptSpeech::Start { request } = &request { request.validate().map_err(|_| ShellError::InvalidArguments)?; }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Speech { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Audio { key, mut request } => {
                if !self.permissions.contains("audio.playback") { return Err(ShellError::PermissionDenied); }
                match &mut request {
                    ScriptAudio::Load { source } | ScriptAudio::Open { source, .. } => {
                        if let jadix_core::audio::AudioSource::File { path } = source {
                            let handle = path.to_str().ok_or(ShellError::InvalidArguments)?;
                            *path = self.share_files.get(handle).cloned().ok_or(ShellError::StaleHandle)?;
                        }
                        source.validate().map_err(|_| ShellError::InvalidArguments)?;
                        if let jadix_core::audio::AudioSource::Url { url } = source {
                            if url.get(..6).is_some_and(|scheme| scheme.eq_ignore_ascii_case("https:")) && !self.permissions.contains("audio.remote") { return Err(ShellError::PermissionDenied); }
                        }
                    }
                    _ => {}
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::Audio { key, request });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Share { key, options } => {
                if !self.permissions.contains("desktop.share") {
                    return Err(ShellError::PermissionDenied);
                }
                let files = options
                    .files
                    .iter()
                    .map(|handle| {
                        self.share_files
                            .get(handle)
                            .cloned()
                            .ok_or(ShellError::StaleHandle)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let share = jadix_core::desktop::Share {
                    subject: options.subject,
                    text: options.text,
                    url: options.url,
                    files,
                    anchor: None,
                };
                share.validate().map_err(|_| ShellError::InvalidArguments)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests
                    .push_back(DesktopRequest::Share { key, share });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::OpenUrl { key, url } => {
                let parsed = jadix_core::desktop::check_open_url(&url)
                    .map_err(|_| ShellError::InvalidArguments)?;
                if !self.permissions.contains("desktop.open_url")
                    && !self
                        .permissions
                        .contains(&format!("desktop.open_url.{}", parsed.scheme()))
                {
                    return Err(ShellError::PermissionDenied);
                }
                self.admit_desktop_key(&key)?;
                self.desktop_requests
                    .push_back(DesktopRequest::OpenUrl { key, url });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::SetTray { key, tray } => {
                if let Some(tray) = &tray { tray.validate().map_err(|_| ShellError::InvalidArguments)?; }
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::SetTray { key, tray });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::TrayEvents { key } => {
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::TrayEvents { key });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::EvaluateWeb { key, path, script } => {
                if path.len() > 32 || script.len() > jadix_core::web_evaluation::MAX_SCRIPT_BYTES {
                    return Err(ShellError::InvalidArguments);
                }
                let node = self.mount.node_at(&path).ok_or(ShellError::StaleHandle)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::EvaluateWeb { key, node, script });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::StopScroll { key, path } => {
                if path.len() > 32 { return Err(ShellError::InvalidArguments); }
                let node = self.mount.node_at(&path).ok_or(ShellError::StaleHandle)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::StopScroll { key, node });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::ZoomTo { key, path, scale, anchor } => {
                if path.len() > 32 || !scale.is_finite() || scale <= 0.0 || !anchor.iter().all(|v| v.is_finite()) {
                    return Err(ShellError::InvalidArguments);
                }
                let node = self.mount.node_at(&path).ok_or(ShellError::StaleHandle)?;
                self.admit_desktop_key(&key)?;
                self.desktop_requests.push_back(DesktopRequest::ZoomTo { key, node, scale, anchor });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Window { id, key, options } => {
                if !self.window_permission {
                    return Err(ShellError::PermissionDenied);
                }
                if id.is_empty()
                    || id.len() > 128
                    || key.is_empty()
                    || key.len() > 128
                    || self.window_updates.len() >= 64
                    || (!self.window_ids.contains(&id) && self.window_ids.len() >= 64)
                    || options
                        .as_ref()
                        .is_some_and(|options| options.validate().is_err())
                {
                    return Err(ShellError::InvalidArguments);
                }
                self.window_ids.insert(id.clone());
                self.window_updates
                    .push_back(ScriptWindowUpdate { id, key, options });
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::VirtualRange { count, extent, gap, offset, viewport, overscan, focus, align } => {
                use jadix_core::virtualization::{ListScrollAlign, UniformList};
                if count > 1_000_000 || overscan > 1024 { return Err(ShellError::InvalidArguments); }
                let list = UniformList::new(count, extent, gap).map_err(|_| ShellError::InvalidArguments)?;
                if list.total() > f32::MAX as f64 { return Err(ShellError::InvalidArguments); }
                let align = match align.as_deref().unwrap_or("nearest") {
                    "nearest" => ListScrollAlign::Nearest, "start" => ListScrollAlign::Start,
                    "center" => ListScrollAlign::Center, "end" => ListScrollAlign::End,
                    _ => return Err(ShellError::InvalidArguments),
                };
                let offset = match focus {
                    Some(index) => list.scroll_to_index(index, offset, viewport, align, 0.),
                    None => list.viewport(offset, viewport).map(|v| v.offset),
                }.map_err(|_| ShellError::InvalidArguments)?;
                let view = list.viewport(offset, viewport).map_err(|_| ShellError::InvalidArguments)?;
                let start = view.visible.start.saturating_sub(overscan);
                let end = view.visible.end.saturating_add(overscan).min(count);
                if end - start > 4096 { return Err(ShellError::InvalidArguments); }
                Ok(serde_json::json!({"start":start,"end":end,"visible_start":view.visible.start,"visible_end":view.visible.end,"offset":offset,"total":list.total(),
                    "before":list.position(start),"after":list.total()-list.position(end).unwrap()}))
            }
            BridgeRequest::EmojiEntry { query, emoji } => {
                serde_json::to_value(query.entry(&emoji).map_err(|_| ShellError::InvalidArguments)?)
                    .map_err(|_| ShellError::InvalidArguments)
            }
            BridgeRequest::EmojiSearch { query } => {
                let page = self.emoji_search.search(&query).map_err(|_| ShellError::InvalidArguments)?;
                serde_json::to_value(page).map_err(|_| ShellError::InvalidArguments)
            }
            BridgeRequest::EmojiLayout { query, columns, headers } => {
                if !(1..=64).contains(&columns) { return Err(ShellError::InvalidArguments); }
                if headers.as_ref().is_some_and(|ids| ids.len() > 11 || ids.iter().any(|id| *id > 10)) { return Err(ShellError::InvalidArguments); }
                self.emoji_search.search(&query).map_err(|_| ShellError::InvalidArguments)?;
                let grid = jadix_core::emoji::EmojiGrid::with_headers(self.emoji_search.matches().cloned().collect(), columns, &query.query, |id| headers.as_ref().is_none_or(|ids| ids.contains(&id)));
                let sections: Vec<_> = (0..grid.rows.len()).map(|row| grid.section_for_row(row)).collect();
                Ok(serde_json::json!({"rows":grid.rows,"total":grid.items.len(),"sections":sections}))
            }
            BridgeRequest::EmojiLocate { query, columns, emoji, headers } => {
                if !(1..=64).contains(&columns) || emoji.len() > 256 { return Err(ShellError::InvalidArguments); }
                if headers.as_ref().is_some_and(|ids| ids.len() > 11 || ids.iter().any(|id| *id > 10)) { return Err(ShellError::InvalidArguments); }
                self.emoji_search.search(&query).map_err(|_| ShellError::InvalidArguments)?;
                let grid = jadix_core::emoji::EmojiGrid::with_headers(self.emoji_search.matches().cloned().collect(), columns, &query.query, |id| headers.as_ref().is_none_or(|ids| ids.contains(&id)));
                let index = grid.index_of(&emoji);
                Ok(serde_json::json!({"index":index,"row":index.and_then(|i|grid.row_for(i)),"entry":index.and_then(|i|grid.items.get(i))}))
            }
            BridgeRequest::EmojiNavigate { query, columns, current, key, rtl, page_rows, headers } => {
                if !(1..=64).contains(&columns) || !(1..=256).contains(&page_rows) { return Err(ShellError::InvalidArguments); }
                if headers.as_ref().is_some_and(|ids| ids.len() > 11 || ids.iter().any(|id| *id > 10)) { return Err(ShellError::InvalidArguments); }
                self.emoji_search.search(&query).map_err(|_| ShellError::InvalidArguments)?;
                let grid = jadix_core::emoji::EmojiGrid::with_headers(self.emoji_search.matches().cloned().collect(), columns, &query.query, |id| headers.as_ref().is_none_or(|ids| ids.contains(&id)));
                let index = grid.navigate(current, key, rtl, page_rows);
                Ok(serde_json::json!({"index":index,"row":index.and_then(|i|grid.row_for(i)),"entry":index.and_then(|i|grid.items.get(i))}))
            }
            BridgeRequest::Identity => Ok(
                serde_json::json!({"session":self.session.token(),"root":self.mount.lifetime().id().token()}),
            ),
            BridgeRequest::Register {
                component,
                event,
                key,
            } => {
                if key.is_empty() || key.len() > 128 || self.handles.contains_key(&key) {
                    return Err(ShellError::InvalidArguments);
                }
                let schema = self
                    .registry
                    .get(&component)
                    .and_then(|s| s.events.get(&event))
                    .cloned()
                    .ok_or(ShellError::InvalidArguments)?;
                let queue = self.deliveries.clone();
                let callback_key = key.clone();
                let handle = self.session.export_event(
                    self.mount.lifetime().id(),
                    schema,
                    move |value| {
                        queue.borrow_mut().push_back((callback_key.clone(), value));
                    },
                )?;
                self.handles.insert(key, handle.clone());
                Ok(serde_json::to_value(handle).expect("handle is JSON"))
            }
            BridgeRequest::Release { key } => {
                let handle = self.handles.remove(&key).ok_or(ShellError::StaleHandle)?;
                self.session.release_handle(&handle)?;
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Mount { tree } => {
                if self.pending.len() >= 16 {
                    return Err(ShellError::RequestLimit);
                }
                let envelope = Envelope {
                    protocol: 1,
                    session: self.session.token(),
                    command: Command::Mount {
                        root: self.mount.lifetime().id().token(),
                        tree: Box::new(tree),
                    },
                };
                self.session.validate_mount(&envelope, &self.registry)?;
                self.pending.push_back(Pending::Mount(envelope));
                Ok(serde_json::Value::Null)
            }
            BridgeRequest::Patch { patches } => {
                if self.pending.len() >= 16 {
                    return Err(ShellError::RequestLimit);
                }
                // The current binding is checked at ordered publication, after
                // any queued mounts. Admission bounds the untrusted message.
                if patches.is_empty()
                    || patches.len() > 1024
                    || patches.iter().any(|p| p.path.len() > 32)
                {
                    return Err(ShellError::InvalidArguments);
                }
                self.pending.push_back(Pending::Patch(patches));
                Ok(serde_json::Value::Null)
            }
        }
    }
    fn admit_desktop_key(&mut self, key: &str) -> Result<(), ShellError> {
        if key.is_empty() || key.len() > 128 || self.desktop_keys.contains(key) {
            return Err(ShellError::InvalidArguments);
        }
        if self.desktop_keys.len() >= 64 {
            return Err(ShellError::RequestLimit);
        }
        self.desktop_keys.insert(key.into());
        Ok(())
    }
    fn fault(&mut self) {
        self.desktop_requests.clear();
        self.desktop_keys.clear();
        self.share_files.clear();
        self.window_updates.clear();
        self.window_ids.clear();
        self.failed = true;
        self.pending.clear();
        self.deliveries.borrow_mut().clear();
        let _ = self.session.fault();
        self.mount.clear();
    }
}
pub struct ScriptApplication {
    #[cfg(feature = "i18n")]
    locale_generation: std::cell::Cell<u64>,
    vm: BudgetedRuntime,
    state: Rc<RefCell<State>>,
    roots: Vec<PresentationNode>,
    structural: Option<bool>,
    commit_revision: u64,
    changes: Option<BTreeSet<Vec<usize>>>,
}
impl ScriptApplication {
    /// Poll outside host state borrows: QuickJS finalizers may call the host.
    pub fn performance_metrics(&self) -> (std::time::Duration, Option<jadix_core::performance::HeapSample>) {
        let heap = self.vm.poll_idle();
        (self.vm.take_busy_time(), heap)
    }
    /// Load a trusted development bundle with the complete shared component registry.
    /// Restricted hosts can continue supplying an explicit registry to `development`.
    pub fn development_standard(package: &str, source: &str, limits: ResourceLimits) -> Result<Self, ApplicationError> {
        Self::development(package, source, ComponentRegistry::standard(), limits)
    }
    /// Verify a package, then expose the complete shared component registry.
    #[cfg(feature = "native-store")]
    pub fn from_package_standard(package: &crate::store::StoredPackage, policy: &crate::package::PackagePolicy, limits: ResourceLimits) -> Result<Self, ApplicationError> {
        Self::from_package(package, policy, ComponentRegistry::standard(), limits)
    }
    /// Trusted development bundle. Production callers must use `from_package`.
    pub fn development(
        package: &str,
        source: &str,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        Self::load(
            PackageManifest {
                package: package.into(),
                protocol_version: 1,
                requested_permissions: BTreeSet::from([
                    "desktop.windows".into(),
                    "speech.recognition".into(),
                    "network.fetch".into(), "network.websocket".into(), "desktop.notifications".into(), "device.haptics".into(),
                    "desktop.performance".into(), "desktop.updater".into(),
                    "desktop.files.pick".into(), "desktop.files.open".into(), "desktop.files.reveal".into(), "desktop.files.trash".into(),
                    "desktop.clipboard.read".into(), "desktop.clipboard.write".into(),
                    "desktop.global_shortcuts".into(),
                    "audio.playback".into(),
                    "audio.remote".into(),
                    "desktop.share".into(),
                    "desktop.open_url".into(),
                ]),
            },
            &BTreeSet::from([
                "desktop.windows".into(),
                    "speech.recognition".into(),
                    "network.fetch".into(), "network.websocket".into(), "desktop.notifications".into(), "device.haptics".into(),
                    "desktop.performance".into(), "desktop.updater".into(),
                    "desktop.files.pick".into(), "desktop.files.open".into(), "desktop.files.reveal".into(), "desktop.files.trash".into(),
                    "desktop.clipboard.read".into(), "desktop.clipboard.write".into(),
                    "desktop.global_shortcuts".into(),
                    "audio.playback".into(),
                    "audio.remote".into(),
                "desktop.share".into(),
                "desktop.open_url".into(),
            ]),
            source,
            None,
            registry,
            limits,
        )
    }
    /// Reverify the signed entry and every resource before any JS executes.
    #[cfg(feature = "native-store")]
    pub fn from_package(
        package: &crate::store::StoredPackage,
        policy: &crate::package::PackagePolicy,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        package
            .signed
            .verify(policy, &package.resources)
            .map_err(|_| ApplicationError::Protocol)?;
        let manifest = &package.signed.manifest;
        let source = std::str::from_utf8(
            package
                .resources
                .get(&manifest.entry)
                .ok_or(ApplicationError::Protocol)?,
        )
        .map_err(|_| ApplicationError::Protocol)?;
        Self::load(
            PackageManifest {
                package: manifest.package.clone(),
                protocol_version: manifest.protocol,
                requested_permissions: manifest.permissions.keys().cloned().collect(),
            },
            &policy.permissions,
            source,
            Some(&package.resources),
            registry,
            limits,
        )
    }
    fn load(
        manifest: PackageManifest,
        grants: &BTreeSet<String>,
        source: &str,
        resources: Option<&BTreeMap<String, Vec<u8>>>,
        registry: ComponentRegistry,
        limits: ResourceLimits,
    ) -> Result<Self, ApplicationError> {
        let window_permission = manifest.requested_permissions.contains("desktop.windows");
        let permissions = manifest.requested_permissions.clone();
        let mut session = Session::with_limits(limits).map_err(ApplicationError::Session)?;
        session
            .verify(manifest)
            .map_err(ApplicationError::Session)?;
        session
            .authorize(grants)
            .map_err(ApplicationError::Session)?;
        session.load(|| Ok(())).map_err(ApplicationError::Session)?;
        let lifetime = session.mount_root().map_err(ApplicationError::Session)?;
        #[cfg(feature = "i18n")]
        let locale = {
            let locale = Rc::new(jadix_core::i18n::LocaleContext::new());
            if let Ok(snapshot) = jadix_core::environment::system_snapshot() {
                locale.update_environment(&snapshot.locale.locale, snapshot.locale.time_zone.as_ref().map(|tz| tz.id.as_str())).map_err(ApplicationError::Host)?;
            }
            if let Some(resources) = resources {
                jadix_core::i18n::request::load_packaged(&locale, resources).map_err(ApplicationError::Host)?;
            }
            locale
        };
        #[cfg(not(feature = "i18n"))]
        let _ = resources;
        let state = Rc::new(RefCell::new(State {
            emoji_search: Default::default(),
            #[cfg(feature = "i18n")]
            locale,
            session,
            registry,
            mount: Mount::new(lifetime),
            pending: VecDeque::new(),
            handles: BTreeMap::new(),
            deliveries: Rc::new(RefCell::new(VecDeque::new())),
            failed: false,
            work: Default::default(),
            window_permission,
            permissions,
            desktop_requests: Default::default(),
            desktop_keys: Default::default(),
            share_files: Default::default(),
            next_file: 0,
            window_updates: VecDeque::new(),
            window_ids: BTreeSet::new(),
        }));
        let vm = BudgetedRuntime::new(limits).map_err(ApplicationError::Vm)?;
        let host = state.clone();
        vm.bind_json("__jadixHost", move |input| {
            host.borrow_mut().work.message_bytes += input.len() as u64;
            let result = if input.len() > crate::wire::MAX_MESSAGE_BYTES {
                Err(ShellError::InvalidArguments)
            } else {
                serde_json::from_str::<BridgeRequest>(&input)
                    .map_err(|_| ShellError::InvalidArguments)
                    .and_then(|request| host.borrow_mut().request(request))
            };
            match result {
                Ok(value) => serde_json::json!({"ok":true,"value":value}),
                Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
            }
            .to_string()
        })
        .map_err(ApplicationError::Vm)?;
        let app = Self {
            #[cfg(feature = "i18n")]
            locale_generation: std::cell::Cell::new(jadix_core::i18n::generation()),
            vm,
            state,
            roots: Vec::new(),
            structural: None,
            commit_revision: 0,
            changes: None,
        };
        app.turn(source, true)?;
        app.state
            .borrow_mut()
            .session
            .activate()
            .map_err(ApplicationError::Session)?;
        Ok(app)
    }
    fn turn(&self, source: &str, startup: bool) -> Result<(), ApplicationError> {
        let source = format!("{source}\n;''");
        let result = if startup {
            self.vm.eval_startup(&source)
        } else {
            self.vm.eval_string(&source)
        }
        .and_then(|_| self.vm.drain_jobs());
        if let Err(error) = result {
            self.state.borrow_mut().fault();
            return Err(ApplicationError::Vm(error));
        }
        Ok(())
    }
    /// Serialize application-owned business state, excluding native handles and
    /// VM objects. The SDK hook and the host both enforce the 64 KiB boundary.
    pub fn checkpoint(&self) -> Result<serde_json::Value, ApplicationError> {
        self.poll_locale()?;
        let result = self.vm.eval_string("globalThis.__jadixCheckpoint?.() ?? 'null'")
            .map_err(|error| { self.state.borrow_mut().fault(); ApplicationError::Vm(error) })?;
        if result.len() > 65_536 { return Err(ApplicationError::Host("checkpoint exceeds 64 KiB".into())); }
        serde_json::from_str(&result).map_err(|e| ApplicationError::Host(e.to_string()))
    }

    /// Restore JSON through the application callback on the existing budgeted
    /// runtime; never evaluate storage bytes as JavaScript source.
    pub fn restore_checkpoint(&self, json: &str) -> Result<(), ApplicationError> {
        self.poll_locale()?;
        if json.len() > 65_536 { return Err(ApplicationError::Host("checkpoint exceeds 64 KiB".into())); }
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.turn(&format!("if (!globalThis.__jadixRestore) throw new Error('checkpoint restore unavailable'); globalThis.__jadixRestore({value});"), false)
    }

    pub fn lifetime(&self) -> RootLifetime {
        self.state.borrow().mount.lifetime().clone()
    }
    #[cfg(feature = "i18n")]
    pub(crate) fn locale_context(&self) -> Rc<jadix_core::i18n::LocaleContext> {
        self.state.borrow().locale.clone()
    }
    fn poll_locale(&self) -> Result<(), ApplicationError> {
        {
            let state = self.state.borrow();
            if state.failed || state.mount.lifetime().is_disposed() {
                return Err(ApplicationError::Session(ShellError::InvalidState));
            }
        }
        #[cfg(feature = "i18n")]
        {
            let generation = jadix_core::i18n::generation();
            if generation != self.locale_generation.replace(generation) {
                let snapshot = jadix_core::i18n::request::LocaleRequest::Snapshot
                    .execute(&self.locale_context()).map_err(ApplicationError::Host)?;
                self.turn(&format!("globalThis.__jadixLocaleChanged?.({snapshot})"), false)?;
            }
        }
        Ok(())
    }
    pub fn node_at(&self, path: &[usize]) -> Option<jadix_core::presentation::NodeId> {
        self.state.borrow().mount.node_at(path)
    }
    pub fn roots(&self) -> &[PresentationNode] {
        &self.roots
    }
    pub fn work(&self) -> ApplicationWork {
        self.state.borrow().work
    }
    pub fn take_focus_request(&mut self) -> Option<jadix_core::presentation::NodeId> {
        self.state.borrow_mut().mount.take_focus_request()
    }
    pub(crate) fn take_changes(&mut self) -> Option<BTreeSet<Vec<usize>>> {
        self.changes.replace(BTreeSet::new())
    }
    /// Host publication precedes binding acceptance. Full snapshots remain O(N).
    /// A rejecting host must leave its tree unchanged. The failed candidate and
    /// later mounts stay queued for retry; already accepted mounts are not replayed.
    pub fn publish(
        &mut self,
        mut host: impl FnMut(&[PresentationNode]) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        self.publish_inner(false, &mut |publication| host(publication.roots()))
    }
    /// Full mounts use a keyed snapshot diff. Validated leaf packets generate
    /// only addressed updates, without cloning or comparing the whole tree.
    /// Host validation, layout and output costs are separate from publication.
    /// Returns accepted SDK mounts, including unchanged trees that skip the
    /// host callback and do not consume a host revision.
    pub fn publish_commits(
        &mut self,
        mut host: impl FnMut(&jadix_core::presentation::CommitBatch) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        let mut revision = self.commit_revision;
        let root = self.lifetime().id();
        let result = self.publish_inner(true, &mut |publication| {
            let batch = match publication {
                Publication::Mount { before, after } => {
                    crate::mount::commits(root, revision, before, after)
                        .map_err(|e| e.to_string())?
                }
                Publication::Leaves { before, after } => CommitBatch {
                    root,
                    previous: revision,
                    revision: revision
                        .checked_add(1)
                        .ok_or_else(|| MountError::Revision.to_string())?,
                    operations: before
                        .iter()
                        .filter_map(|(path, content)| {
                            let node = presentation_at(after, path).expect("validated leaf path");
                            (node.descriptor.content != *content).then(|| Operation::Update {
                                node: node.id,
                                descriptor: node.descriptor.clone(),
                            })
                        })
                        .collect(),
                },
            };
            if !batch.operations.is_empty() {
                host(&batch)?;
                revision = batch.revision;
            }
            Ok(())
        });
        self.commit_revision = revision;
        result
    }
    fn publish_inner(
        &mut self,
        structural: bool,
        host: &mut impl FnMut(Publication<'_>) -> Result<(), String>,
    ) -> Result<usize, ApplicationError> {
        {
            let state = self.state.borrow();
            if state.failed || state.mount.lifetime().is_disposed() {
                return Err(ApplicationError::Session(ShellError::InvalidState));
            }
        }
        if self.structural.is_some_and(|mode| mode != structural) {
            return Err(ApplicationError::Protocol);
        }
        self.structural = Some(structural);
        self.poll_locale()?;
        let mut count = 0;
        loop {
            let mut state = self.state.borrow_mut();
            let Some(pending) = state.pending.front() else {
                break;
            };
            if let Pending::Patch(patches) = pending {
                let patches = state
                    .mount
                    .prepare_leaves(&state.session, &state.registry, patches)
                    .map_err(ApplicationError::Mount)?;
                // Stage and validate all paths before any mutation. A rejecting
                // host restores only changed contents, keeping retry order.
                let before = patches
                    .iter()
                    .map(|p| {
                        presentation_at(&self.roots, &p.path)
                            .filter(|n| n.children.is_empty())
                            .map(|n| (p.path.clone(), n.descriptor.content.clone()))
                            .ok_or(ApplicationError::Protocol)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for p in &patches {
                    presentation_at_mut(&mut self.roots, &p.path)
                        .unwrap()
                        .descriptor
                        .content = p.content.clone();
                }
                if let Err(error) = host(Publication::Leaves {
                    before: &before,
                    after: &self.roots,
                }) {
                    for (path, content) in before {
                        presentation_at_mut(&mut self.roots, &path)
                            .unwrap()
                            .descriptor
                            .content = content;
                    }
                    return Err(ApplicationError::Host(error));
                }
                if let Some(changes) = &mut self.changes {
                    changes.extend(patches.iter().map(|p| p.path.clone()));
                }
                state.mount.accept_leaves(patches);
                state.work.leaf_patches += before.len() as u64;
                state.pending.pop_front();
                count += 1;
                continue;
            }
            let Pending::Mount(envelope) = pending else {
                unreachable!()
            };
            let mut candidate = state
                .mount
                .prepare(&state.session, &state.registry, envelope)
                .map_err(ApplicationError::Mount)?;
            if let Err(error) = host(Publication::Mount {
                before: &self.roots,
                after: &candidate.roots,
            }) {
                return Err(ApplicationError::Host(error));
            }
            let roots = std::mem::take(&mut candidate.roots);
            state
                .mount
                .accept(candidate)
                .map_err(ApplicationError::Mount)?;
            state.pending.pop_front();
            self.roots = roots;
            state.work.full_mounts += 1;
            self.changes = None;
            count += 1;
        }
        Ok(count)
    }
    pub fn events(
        &mut self,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<(), ApplicationError> {
        self.poll_locale()?;
        let events: Vec<_> = events.into_iter().take(1025).collect();
        if events.len() > 1024 {
            return Err(ApplicationError::Protocol);
        }
        let deliveries = {
            let mut state = self.state.borrow_mut();
            state.work.event_lookups += events.len() as u64;
            let State { mount, session, .. } = &mut *state;
            mount
                .events(session, events)
                .map_err(ApplicationError::Mount)?
        };
        for delivery in deliveries {
            delivery.dispatch().map_err(ApplicationError::Session)?;
        }
        loop {
            let next = { self.state.borrow().deliveries.borrow_mut().pop_front() };
            let Some((key, value)) = next else {
                break;
            };
            self.turn(
                &format!(
                    "globalThis.__jadixDispatch({},{})",
                    serde_json::to_string(&key).unwrap(),
                    serde_json::to_string(&value).unwrap()
                ),
                false,
            )?;
        }
        Ok(())
    }
    pub fn dispose(&mut self) {
        self.state.borrow_mut().fault();
        self.roots.clear();
    }

    /// Grant one existing file selected by trusted host code. Scripts only see
    /// the returned root/session-scoped token, never a filesystem path.
    pub fn grant_share_file(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> Result<String, ApplicationError> {
        let path = path
            .as_ref()
            .canonicalize()
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        let mut state = self.state.borrow_mut();
        if state.failed
            || state.mount.lifetime().is_disposed()
            || !path.is_file()
            || state.share_files.len() >= 64
        {
            return Err(ApplicationError::Host(
                "share file grant unavailable".into(),
            ));
        }
        state.next_file = state
            .next_file
            .checked_add(1)
            .ok_or(ApplicationError::Protocol)?;
        let handle = format!("{}:file:{}", state.session.token(), state.next_file);
        state.share_files.insert(handle.clone(), path);
        Ok(handle)
    }

    pub fn revoke_share_file(&mut self, handle: &str) -> bool {
        self.state.borrow_mut().share_files.remove(handle).is_some()
    }

    /// Atomically grant the results of an explicit native chooser, including a
    /// save destination whose parent exists. No local paths cross the bridge.
    pub(crate) fn grant_selected_paths(&mut self, paths: Vec<std::path::PathBuf>) -> Result<serde_json::Value, String> {
        let mut selected = Vec::new();
        if paths.len() > 64 { return Err("file selection limit reached".into()); }
        for path in paths {
            let path = if path.exists() { path.canonicalize() } else {
                path.parent().ok_or("missing destination parent")?.canonicalize().map(|parent| parent.join(path.file_name().unwrap_or_default()))
            }.map_err(|e| e.to_string())?;
            selected.push(path);
        }
        let mut state = self.state.borrow_mut();
        if state.failed || state.mount.lifetime().is_disposed() || state.share_files.len() + selected.len() > 64 { return Err("file grants unavailable or limit reached".into()); }
        state.next_file.checked_add(selected.len() as u64).ok_or("file identity exhausted")?;
        let mut values = Vec::new();
        for path in selected {
            state.next_file += 1;
            let handle = format!("{}:file:{}", state.session.token(), state.next_file);
            values.push(serde_json::json!({"handle":handle,"name":path.file_name().map(|s|s.to_string_lossy()),"directory":path.is_dir()}));
            state.share_files.insert(handle, path);
        }
        Ok(serde_json::Value::Array(values))
    }

    pub(crate) fn take_desktop_requests(&mut self) -> VecDeque<DesktopRequest> {
        std::mem::take(&mut self.state.borrow_mut().desktop_requests)
    }

    pub(crate) fn desktop_result(
        &mut self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ApplicationError> {
        if !self.state.borrow_mut().desktop_keys.remove(key) {
            return Ok(());
        }
        self.window_event(key, value)
    }

    pub(crate) fn take_window_updates(&mut self) -> VecDeque<ScriptWindowUpdate> {
        std::mem::take(&mut self.state.borrow_mut().window_updates)
    }

    pub(crate) fn window_event(
        &mut self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), ApplicationError> {
        self.turn(
            &format!(
                "globalThis.__jadixDispatch({},{})",
                serde_json::to_string(key).unwrap(),
                value
            ),
            false,
        )
    }
}

pub(crate) fn presentation_at<'a>(
    roots: &'a [PresentationNode],
    path: &[usize],
) -> Option<&'a PresentationNode> {
    let mut node = roots.first()?;
    for i in path {
        node = node.children.get(*i)?;
    }
    Some(node)
}
fn presentation_at_mut<'a>(
    roots: &'a mut [PresentationNode],
    path: &[usize],
) -> Option<&'a mut PresentationNode> {
    let mut node = roots.first_mut()?;
    for i in path {
        node = node.children.get_mut(*i)?;
    }
    Some(node)
}

#[cfg(test)]
mod desktop_tests {
    use super::*;

    fn app(permissions: &[&str]) -> ScriptApplication {
        let permissions = permissions
            .iter()
            .map(|value| value.to_string())
            .collect::<BTreeSet<_>>();
        ScriptApplication::load(PackageManifest { package: "tests.desktop".into(), protocol_version: 1,
            requested_permissions: permissions.clone() }, &permissions,
            "globalThis.received=[]; globalThis.__jadixDispatch=(key,value)=>received.push([key,value]);",
            None, ComponentRegistry::default(), ResourceLimits::default()).unwrap()
    }

    fn request(
        app: &ScriptApplication,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, ShellError> {
        app.state
            .borrow_mut()
            .request(serde_json::from_value(value).unwrap())
    }

    #[test]
    fn websocket_authority_and_validation_precede_enqueue() {
        let connect=serde_json::json!({"type":"web_socket","key":"ws","request":{"action":"connect","request":{"url":"ws://127.0.0.1/"}}});
        assert!(matches!(request(&app(&["network.fetch"]),connect.clone()),Err(ShellError::PermissionDenied)));
        let mut allowed=app(&["network.websocket"]);
        let mut invalid=connect.clone();invalid["request"]["request"]["url"]="ws://user:secret@example.org/".into();
        assert!(matches!(request(&allowed,invalid),Err(ShellError::InvalidArguments)));
        assert!(request(&allowed,connect).is_ok());
        assert!(matches!(allowed.take_desktop_requests().make_contiguous(),[DesktopRequest::WebSocket{request:ScriptWebSocket::Connect{..},..}]));
    }

    #[test]
    fn app_addresses_share_deep_link_authority_and_validate_before_enqueue() {
        let address = serde_json::json!({"type":"deep_links","key":"address","request":{"action":"address","address":"demo://inbox/item"}});
        assert!(matches!(request(&app(&[]), address.clone()), Err(ShellError::PermissionDenied)));
        let mut allowed = app(&["desktop.deep_link"]);
        let mut invalid = address.clone(); invalid["request"]["address"] = "javascript:alert(1)".into();
        assert!(matches!(request(&allowed, invalid), Err(ShellError::InvalidArguments)));
        assert!(request(&allowed, address).is_ok());
        assert!(matches!(allowed.take_desktop_requests().make_contiguous(), [DesktopRequest::DeepLinks { request: ScriptDeepLinks::Address { address: Some(value) }, .. }] if value == "demo://inbox/item"));
        assert!(request(&allowed, serde_json::json!({"type":"deep_links","key":"clear","request":{"action":"address","address":null}})).is_ok());
        allowed.dispose();
    }
    #[test]
    fn auth_sessions_use_validated_scheme_scoped_url_authority() {
        let open = serde_json::json!({"type":"auth_session","key":"login","request":{"action":"open","request":{"url":"https://example.com/login","callback_scheme":"demo","ephemeral":true}}});
        assert!(matches!(request(&app(&[]), open.clone()), Err(ShellError::PermissionDenied)));
        assert!(matches!(request(&app(&["desktop.open_url.http"]), open.clone()), Err(ShellError::PermissionDenied)));
        let mut allowed = app(&["desktop.open_url.https"]);
        assert!(request(&allowed, open.clone()).is_ok());
        assert!(matches!(allowed.take_desktop_requests().make_contiguous(), [DesktopRequest::AuthSession { request: ScriptAuthSession::Open { request }, .. }] if request.ephemeral && request.callback_scheme == "demo"));
        let mut invalid = open; invalid["key"] = "invalid".into(); invalid["request"]["request"]["callback_scheme"] = "https".into();
        assert!(matches!(request(&allowed, invalid), Err(ShellError::InvalidArguments)));
        let close = serde_json::json!({"type":"auth_session","key":"close","request":{"action":"close"}});
        assert!(request(&allowed, close.clone()).is_ok());
        assert!(matches!(request(&app(&[]), close), Err(ShellError::PermissionDenied)));
        allowed.dispose();
    }
    #[test]
    fn haptics_require_permission_and_enter_the_shared_service_queue() {
        let denied = app(&[]);
        let value = serde_json::json!({"type":"haptic","key":"feedback","feedback":{"type":"selection"}});
        assert!(matches!(request(&denied, value.clone()), Err(ShellError::PermissionDenied)));
        let mut allowed = app(&["device.haptics"]);
        assert!(request(&allowed, value).is_ok());
        assert!(matches!(allowed.take_desktop_requests().make_contiguous(), [DesktopRequest::Haptic { key, feedback: jadix_core::desktop_services::Haptic::Selection }] if key == "feedback"));
        assert!(serde_json::from_value::<BridgeRequest>(serde_json::json!({"type":"haptic","key":"invalid","feedback":{"type":"impact","style":"unknown"}})).is_err());
        allowed.dispose();
    }
    #[test]
    fn emoji_bridge_reuses_query_layout_navigation_and_visible_geometry() {
        let app = app(&[]);
        let query = serde_json::json!({"query":"face"});
        let layout = request(&app, serde_json::json!({"type":"emoji_layout","query":query,"columns":3})).unwrap();
        let rows = layout["rows"].as_array().unwrap();
        assert!(layout["total"].as_u64().unwrap() > 3);
        assert!(rows.iter().all(|row| row["kind"] == "items"));
        assert_eq!(rows[0]["value"], serde_json::json!({"start":0,"end":3}));
        let first = request(&app, serde_json::json!({"type":"emoji_navigate","query":query,"columns":3,"current":null,"key":"down"})).unwrap();
        assert_eq!(first["index"], 0);
        assert_eq!(first["row"], 0);
        let locate = request(&app, serde_json::json!({"type":"emoji_locate","query":query,"columns":3,"emoji":first["entry"]["text"]})).unwrap();
        assert_eq!(locate, first);
        let next = request(&app, serde_json::json!({"type":"emoji_navigate","query":query,"columns":3,"current":0,"key":"down"})).unwrap();
        assert_eq!(next["index"], 3);
        assert_eq!(next["row"], 1);
        let rtl = request(&app, serde_json::json!({"type":"emoji_navigate","query":query,"columns":3,"current":3,"key":"right","rtl":true})).unwrap();
        assert_eq!(rtl["index"], 2);
        let plain = request(&app, serde_json::json!({"type":"emoji_layout","query":{"groups":[1]},"columns":3,"headers":[]})).unwrap();
        assert!(plain["rows"].as_array().unwrap().iter().all(|row| row["kind"] == "items"));
        assert_eq!(plain["sections"][0], 1);
        let first_plain = request(&app, serde_json::json!({"type":"emoji_navigate","query":{"groups":[1],"tone":5},"columns":3,"headers":[],"current":null,"key":"down"})).unwrap();
        let located_plain = request(&app, serde_json::json!({"type":"emoji_locate","query":{"groups":[1],"tone":5},"columns":3,"headers":[],"emoji":first_plain["entry"]["text"]})).unwrap();
        assert_eq!(first_plain, located_plain);
        assert_eq!(first_plain["row"], 0);
        assert!(request(&app, serde_json::json!({"type":"emoji_layout","query":{},"columns":3,"headers":[11]})).is_err());
        let range = request(&app, serde_json::json!({"type":"virtual_range","count":1000,"extent":20.,"offset":60.,"viewport":100.,"overscan":2})).unwrap();
        assert_eq!(range["start"], 1); assert_eq!(range["end"], 10);
        assert_eq!(range["visible_start"], 3); assert_eq!(range["visible_end"], 8);
        let focused = request(&app, serde_json::json!({"type":"virtual_range","count":1000,"extent":20.,"offset":0.,"viewport":100.,"overscan":2,"focus":50})).unwrap();
        assert_eq!(focused["offset"], 920.);
        let aligned = request(&app, serde_json::json!({"type":"virtual_range","count":1000,"extent":20.,"offset":0.,"viewport":100.,"focus":50,"align":"start"})).unwrap();
        assert_eq!(aligned["offset"], 1000.);
        assert!(request(&app, serde_json::json!({"type":"virtual_range","count":1000,"extent":20.,"offset":0.,"viewport":100.,"focus":50,"align":"invalid"})).is_err());
        assert!(matches!(request(&app, serde_json::json!({"type":"emoji_layout","query":{},"columns":0})), Err(ShellError::InvalidArguments)));
    }

    #[cfg(feature = "i18n")]
    #[test]
    fn locale_notifications_run_outside_state_borrows_and_stop_with_the_application() {
        let mut app = app(&[]);
        app.turn("globalThis.locales=[]; globalThis.__jadixLocaleChanged=s=>{locales.push(s.locale); __jadixHost(JSON.stringify({type:'i18n',request:{action:'snapshot'}}));}", false).unwrap();
        request(&app, serde_json::json!({"type":"i18n","request":{"action":"configure","locale":"fr"}})).unwrap();
        app.publish(|_| Ok(())).unwrap();
        assert_eq!(app.vm.eval_string("JSON.stringify(locales)").unwrap(), "[\"fr\"]");
        app.publish(|_| Ok(())).unwrap();
        assert_eq!(app.vm.eval_string("String(locales.length)").unwrap(), "1");
        app.dispose();
        assert!(app.poll_locale().is_err());
    }

    #[test]
    fn authorization_precedes_native_io_and_url_grants_are_scheme_scoped() {
        let app = app(&["desktop.open_url.https"]);
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"share","key":"s","options":{"text":"hello"}})
            ),
            Err(ShellError::PermissionDenied)
        ));
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"open_url","key":"u","url":"mailto:a@example.com"})
            ),
            Err(ShellError::PermissionDenied)
        ));
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"open_url","key":"u","url":"javascript:alert(1)"})
            ),
            Err(ShellError::InvalidArguments)
        ));
        request(
            &app,
            serde_json::json!({"type":"open_url","key":"u","url":"HTTPS://example.com/"}),
        )
        .unwrap();
        assert_eq!(app.state.borrow().desktop_requests.len(), 1);
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"open_url","key":"u","url":"https://example.com/"})
            ),
            Err(ShellError::InvalidArguments)
        ));
    }

    #[test]
    fn sharing_accepts_only_live_session_file_grants() {
        let mut app = app(&["desktop.share"]);
        let path = std::env::current_exe().unwrap();
        let handle = app.grant_share_file(&path).unwrap();
        let mut other = self::app(&["desktop.share"]);
        assert!(matches!(
            request(
                &other,
                serde_json::json!({"type":"share","key":"s","options":{"files":[handle]}})
            ),
            Err(ShellError::StaleHandle)
        ));
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"share","key":"s","options":{"files":[path]}})
            ),
            Err(ShellError::StaleHandle)
        ));
        request(
            &app,
            serde_json::json!({"type":"share","key":"s","options":{"files":[handle]}}),
        )
        .unwrap();
        let DesktopRequest::Share { share, .. } = app.take_desktop_requests().pop_front().unwrap()
        else {
            panic!("expected share");
        };
        assert_eq!(share.files, [path.canonicalize().unwrap()]);
        assert!(app.revoke_share_file(&handle));
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"share","key":"s2","options":{"files":[handle]}})
            ),
            Err(ShellError::StaleHandle)
        ));
        app.dispose();
        assert!(app.grant_share_file(&path).is_err());
        assert!(app.state.borrow().share_files.is_empty());
        other.dispose();
    }

    #[test]
    fn outstanding_requests_remain_bounded_after_drain_and_results_are_once_only() {
        let mut app = app(&["desktop.open_url"]);
        for index in 0..64 {
            request(&app, serde_json::json!({"type":"open_url","key":format!("u{index}"),"url":"https://example.com"})).unwrap();
        }
        assert_eq!(app.take_desktop_requests().len(), 64);
        assert!(matches!(
            request(
                &app,
                serde_json::json!({"type":"open_url","key":"overflow","url":"https://example.com"})
            ),
            Err(ShellError::RequestLimit)
        ));
        app.desktop_result("u0", serde_json::json!({"value":true}))
            .unwrap();
        app.desktop_result("u0", serde_json::json!({"error":"duplicate"}))
            .unwrap();
        assert_eq!(app.vm.eval_string("String(received.length)").unwrap(), "1");
        request(
            &app,
            serde_json::json!({"type":"open_url","key":"overflow","url":"https://example.com"}),
        )
        .unwrap();
        app.dispose();
        assert!(app.take_desktop_requests().is_empty());
        assert!(app.state.borrow().desktop_keys.is_empty());
    }
}

fn emoji_page_rows() -> usize { 6 }
