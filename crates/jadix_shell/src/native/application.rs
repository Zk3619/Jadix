//! Reuse the budgeted SDK runtime with any existing native PlatformHost.
use crate::application::{ApplicationError, DesktopRequest, ScriptApplication};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    layout::LayoutStats,
    ResourceLimits,
};

#[cfg(all(feature = "native-linux", target_os = "linux", not(target_env = "ohos")))]
impl jadix_native::linux::WindowApplication for NativeApplication<jadix_native::linux::GtkHost> {
    fn host(&self) -> &jadix_native::linux::GtkHost { self.root.host() }
    fn host_mut(&mut self) -> &mut jadix_native::linux::GtkHost { self.root.host_mut() }
    fn tick(&mut self) -> Result<(), String> {
        self.poll().map_err(|e| e.to_string())?;
        self.root.refresh_environment().map_err(|e| e.to_string())?;
        self.publish(self.root.host().viewport(), 0).map_err(|e| e.to_string())?;
        for wake in self.take_web_wakes() { wake.wake(); }
        Ok(())
    }
}

#[cfg(all(feature = "native-windows", target_os = "windows"))]
impl jadix_native::windows::WindowApplication for NativeApplication<jadix_native::windows::WindowsHost> {
    fn host(&self) -> &jadix_native::windows::WindowsHost { self.root.host() }
    fn host_mut(&mut self) -> &mut jadix_native::windows::WindowsHost { self.root.host_mut() }
    fn tick(&mut self) -> Result<(), String> {
        self.poll().map_err(|e| e.to_string())?;
        self.root.refresh_environment().map_err(|e| e.to_string())?;
        self.publish(self.root.host().viewport(), 0).map_err(|e| e.to_string())?;
        for wake in self.take_web_wakes() { wake.wake(); }
        Ok(())
    }
}

pub struct NativeApplication<Host: PlatformHost> {
    app: ScriptApplication,
    root: NativeRoot<Host>,
    viewport: Option<([f32; 2], u64)>,
    shares: Vec<(
        String,
        std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>,
    )>,
    urls: Vec<(
        String,
        std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>,
    )>,
    files: Vec<(String, bool, std::sync::mpsc::Receiver<jadix_core::file_request::FileResult>)>,
    clipboards: Vec<(String, std::sync::mpsc::Receiver<jadix_core::clipboard::ClipboardResult>)>,
    speech: std::collections::BTreeMap<u32, jadix_core::speech::SpeechSession>,
    fetches: std::collections::BTreeMap<u32, jadix_net::service::FetchHandle>,
    sockets: std::collections::BTreeMap<u32, jadix_net::websocket::SocketHandle>,
    service_replies: Vec<(String, jadix_core::desktop_services::ServiceHandle)>,
    notifications: std::collections::BTreeMap<u32, jadix_core::desktop_services::ServiceHandle>,
    shortcuts: std::collections::BTreeMap<u32, jadix_core::desktop_services::ServiceHandle>,
    next_audio: u32,
    sounds: std::collections::BTreeMap<u32, jadix_core::audio::Sound>,
    players: std::collections::BTreeMap<u32, jadix_core::audio::AudioPlayer>,
    evaluations: Vec<(String, jadix_core::web_evaluation::WebEvaluation)>,
    web_wakes: Vec<jadix_core::web_evaluation::EvaluationBatch>,
    layout_stats: LayoutStats,
    windows: std::collections::BTreeMap<String, (String, jadix_core::window::WindowDeclaration)>,
}

impl<Host: PlatformHost> NativeApplication<Host> {
    pub fn new(app: ScriptApplication, host: Host, limits: ResourceLimits) -> Self {
        let mut root = NativeRoot::with_lifetime(host, app.lifetime(), limits);
        #[cfg(feature = "i18n")]
        root.set_locale_context(app.locale_context()).expect("new root is alive");
        Self {
            app,
            root,
            viewport: None,
            shares: Vec::new(),
            urls: Vec::new(),
            files: Vec::new(),
            clipboards: Vec::new(),
            speech: Default::default(),
            fetches: Default::default(),
            sockets: Default::default(),
            service_replies: Vec::new(),
            notifications: Default::default(),
            shortcuts: Default::default(),
            next_audio: 0,
            sounds: Default::default(),
            players: Default::default(),
            evaluations: Vec::new(),
            web_wakes: Vec::new(),
            layout_stats: Default::default(),
            windows: Default::default(),
        }
    }
    pub fn application(&self) -> &ScriptApplication {
        &self.app
    }
    pub fn application_mut(&mut self) -> &mut ScriptApplication {
        &mut self.app
    }
    pub fn root(&self) -> &NativeRoot<Host> {
        &self.root
    }
    pub fn root_mut(&mut self) -> &mut NativeRoot<Host> {
        &mut self.root
    }
    pub fn layout_stats(&self) -> &LayoutStats {
        &self.layout_stats
    }
    /// Deliver outside the UI application's registry borrow, like native completions.
    pub fn take_web_wakes(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        std::mem::take(&mut self.web_wakes)
    }

    /// NativeRoot performs keyed structural publication and validates receipts.
    /// Native layout and SDK projection still scan snapshots; no O(k) claim.
    pub fn publish(
        &mut self,
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<usize, ApplicationError> {
        self.poll_windows()?;
        self.poll_desktop()?;
        let (busy, heap) = self.app.performance_metrics();
        self.root.script_metrics(busy, heap);
        let root = &mut self.root;
        let stats = &mut self.layout_stats;
        let count = self.app.publish(|nodes| {
            *stats = root
                .present(&mut nodes.to_vec(), viewport, environment)
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        if count == 0
            && self
                .viewport
                .is_some_and(|old| old != (viewport, environment))
        {
            self.layout_stats = root
                .present(&mut self.app.roots().to_vec(), viewport, environment)
                .map_err(|e| ApplicationError::Host(e.to_string()))?;
        }
        if let Some(node) = self.app.take_focus_request() { root.focus(Some(node)).map_err(|e| ApplicationError::Host(e.to_string()))?; }
        self.viewport = Some((viewport, environment));
        Ok(count)
    }
    /// Deliver a synchronous platform callback through the same validation used
    /// by queued host events, before allowing it to enter the script runtime.
    pub fn event(&mut self, event: jadix_core::presentation::PlatformEvent) -> Result<(), ApplicationError> {
        self.root.event(&event).map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.app.events([event])?;
        self.poll_windows()?;
        self.poll_desktop()
    }

    pub fn event_json(&mut self, json: &str) -> Result<(), ApplicationError> {
        let event = jadix_native::wire::decode_event(json, self.root.model())
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.event(event)
    }

    /// Poll only validated events belonging to this native root, then run JS jobs.
    pub fn poll(&mut self) -> Result<(), ApplicationError> {
        let events = self
            .root
            .poll_events()
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.app.events(events)?;
        self.poll_windows()?;
        self.poll_desktop()?;
        let (busy, heap) = self.app.performance_metrics();
        self.root.script_metrics(busy, heap);
        Ok(())
    }
    fn poll_desktop(&mut self) -> Result<(), ApplicationError> {
        if self.app.lifetime().is_disposed() {
            self.service_replies.clear(); self.notifications.clear(); self.shortcuts.clear();
            self.speech.clear(); self.sounds.clear(); self.players.clear(); self.fetches.clear(); self.sockets.clear();
            self.evaluations.clear();
            self.shares.clear();
            self.urls.clear();
            self.files.clear(); self.clipboards.clear();
            return Ok(());
        }
        for request in self.app.take_desktop_requests() {
            let (key, error) = match request {
                DesktopRequest::Clipboard { key, request } => match self.root.host_mut().clipboard(&request) {
                    Ok(receiver) => { self.clipboards.push((key, receiver)); continue; }
                    Err(error) => (key, error),
                },
                DesktopRequest::Files { key, request } => {
                    let grant = matches!(&request, jadix_core::file_request::FileRequest::PickPaths { .. } | jadix_core::file_request::FileRequest::PickSavePath { .. });
                    match self.root.host_mut().request_file(&request) {
                        Ok(receiver) => { self.files.push((key, grant, receiver)); continue; }
                        Err(error) => (key, error),
                    }
                }
                DesktopRequest::ReleaseFile { key, handle } => {
                    self.app.revoke_share_file(&handle);
                    self.app.desktop_result(&key, serde_json::json!({"value":null}))?;
                    continue;
                }
                DesktopRequest::AuthSession { key, request } => {
                    let result = self.root.host().desktop_services().ok_or_else(|| "authentication sessions unavailable".to_owned()).and_then(|service| {
                        match request {
                            crate::application::ScriptAuthSession::Open { request } => service.open_auth_session(request).map(Some),
                            crate::application::ScriptAuthSession::Close => { service.close_auth_session(); Ok(None) }
                        }
                    });
                    match result {
                        Ok(Some(handle)) => self.service_replies.push((key, handle)),
                        Ok(None) => self.app.desktop_result(&key, serde_json::json!({"value":null}))?,
                        Err(error) => self.app.desktop_result(&key, serde_json::json!({"error":error}))?,
                    }
                    continue;
                }
                DesktopRequest::Haptic { key, feedback } => {
                    match self.root.host().desktop_services().ok_or_else(|| "haptics unavailable".to_owned()).and_then(|service| service.haptic(feedback)) {
                        Ok(handle) => self.service_replies.push((key, handle)),
                        Err(error) => self.app.desktop_result(&key, serde_json::json!({"error":error}))?,
                    }
                    continue;
                }
                DesktopRequest::Notifications { key, request } => {
                    let result = self.notification_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::GlobalShortcut { key, request } => {
                    let result = self.shortcut_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::DeepLinks { key, request } => {
                    let result = match request {
                        crate::application::ScriptDeepLinks::Configure { config } => self.root.host_mut().configure_deep_links(config).map(|_| serde_json::Value::Null),
                        crate::application::ScriptDeepLinks::Activation { config } => self.root.host_mut().configure_activation(config).map(serde_json::Value::Bool),
                        crate::application::ScriptDeepLinks::Launches => Ok(serde_json::json!(self.root.host_mut().take_application_launches())),
                        crate::application::ScriptDeepLinks::Events => Ok(serde_json::json!(self.root.host_mut().take_deep_links())),
                        crate::application::ScriptDeepLinks::Address { address } => self.root.set_app_address(address.as_deref()).map(|_| serde_json::Value::Null).map_err(|error| error.to_string()),
                    };
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::Performance { key } => {
                    self.app.desktop_result(&key, serde_json::json!({"value":self.root.performance()}))?;
                    continue;
                }
                DesktopRequest::Updater { key, request } => {
                    let result = self.updater_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::WebSocket { key, request } => {
                    let result = self.websocket_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::Fetch { key, request } => {
                    let result = self.fetch_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::Speech { key, request } => {
                    let result = self.speech_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::Audio { key, request } => {
                    let result = self.audio_request(request);
                    self.app.desktop_result(&key, match result { Ok(value) => serde_json::json!({"value": value}), Err(error) => serde_json::json!({"error": error}) })?;
                    continue;
                }
                DesktopRequest::SetTray { key, tray } => {
                    let root = self.root.model().id();
                    let result = self.root.host_mut().set_tray(root, tray.as_ref());
                    self.app.desktop_result(&key, match result { Ok(()) => serde_json::json!({"value":null}), Err(error) => serde_json::json!({"error":error}) })?;
                    continue;
                }
                DesktopRequest::TrayEvents { key } => {
                    let events = self.root.host_mut().take_tray_events();
                    self.app.desktop_result(&key, serde_json::json!({"value":events}))?; continue;
                }
                DesktopRequest::EvaluateWeb { key, node, script } => {
                    self.evaluations.push((key, self.root.evaluate_web(node, &script)));
                    continue;
                }
                DesktopRequest::StopScroll { key, node } => {
                    let result = self.root.stop_scroll(node);
                    self.app.desktop_result(&key, match result {
                        Ok(()) => serde_json::json!({"value":null}),
                        Err(error) => serde_json::json!({"error":error.to_string()}),
                    })?;
                    continue;
                }
                DesktopRequest::ZoomTo { key, node, scale, anchor } => {
                    let result = self.root.zoom_to(node, scale, anchor);
                    self.app.desktop_result(&key, match result {
                        Ok(()) => serde_json::json!({"value":null}),
                        Err(error) => serde_json::json!({"error":error.to_string()}),
                    })?;
                    continue;
                }
                DesktopRequest::Share { key, share } => {
                    match self.root.host_mut().show_share(&share) {
                        Ok(receiver) => {
                            self.shares.push((key, receiver));
                            continue;
                        }
                        Err(error) => (key, error),
                    }
                }
                DesktopRequest::OpenUrl { key, url } => match self.root.host_mut().open_url(&url) {
                    Ok(receiver) => {
                        self.urls.push((key, receiver));
                        continue;
                    }
                    Err(error) => (key, error),
                },
            };
            self.app
                .desktop_result(&key, serde_json::json!({"error":error}))?;
        }
        let root = self.root.model().id(); self.root.host_mut().pump_audio(root);
        self.web_wakes.extend(self.root.pump_web().into_iter().filter(|batch| batch.needs_wake()));
        let mut results = Vec::new();
        {
            use std::{future::Future, pin::Pin, task::{Context, Poll, Waker}};
            let mut cx = Context::from_waker(Waker::noop());
            self.evaluations.retain_mut(|(key, future)| {
                match Pin::new(future).poll(&mut cx) {
                    Poll::Pending => true,
                    Poll::Ready(answer) => {
                        results.push((key.clone(), match answer {
                            Ok(value) => serde_json::json!({"value":value}),
                            Err(error) => serde_json::json!({"error":error.to_string()}),
                        })); false
                    }
                }
            });
        }
        self.service_replies.retain(|(key, handle)| {
            let Some(reply) = handle.reply() else { return true; };
            results.push((key.clone(), serde_json::json!({"value":reply}))); false
        });
        collect_desktop_results(&mut self.shares, &mut results);
        collect_desktop_results(&mut self.urls, &mut results);
        collect_desktop_results(&mut self.clipboards, &mut results);
        let mut index = 0;
        while index < self.files.len() {
            let result = match self.files[index].2.try_recv() {
                Ok(value) => value,
                Err(std::sync::mpsc::TryRecvError::Empty) => { index += 1; continue; }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("file request disconnected".into()),
            };
            let (key, grant, _) = self.files.remove(index);
            let result = result.and_then(|paths| if grant { self.app.grant_selected_paths(paths) } else { Ok(serde_json::Value::Null) });
            results.push((key, match result { Ok(value) => serde_json::json!({"value":value}), Err(error) => serde_json::json!({"error":error}) }));
        }
        for (key, value) in results {
            self.app.desktop_result(&key, value)?;
        }
        Ok(())
    }
    fn notification_request(&mut self, request: crate::application::ScriptNotifications) -> Result<serde_json::Value, String> {
        use crate::application::ScriptNotifications as R;
        let service = self.root.host().desktop_services().ok_or("desktop notifications unavailable")?;
        let handle = match request {
            R::Permission { request } => service.permission(request)?,
            R::Notify { notification } => service.notify(notification)?,
            R::Badge { label } => service.badge(label)?,
            R::Reply { id } => return serde_json::to_value(self.notifications.get(&id).ok_or("stale notification handle")?.reply()).map_err(|e| e.to_string()),
            R::Events { id } => return serde_json::to_value(self.notifications.get(&id).ok_or("stale notification handle")?.events()).map_err(|e| e.to_string()),
            R::Release { id } => { self.notifications.remove(&id).ok_or("stale notification handle")?; return Ok(serde_json::Value::Null); }
            R::Activations => return serde_json::to_value(service.activations()).map_err(|e| e.to_string()),
        };
        let id = handle.id(); self.notifications.insert(id, handle); Ok(serde_json::json!(id))
    }
    fn shortcut_request(&mut self, request: crate::application::ScriptGlobalShortcut) -> Result<serde_json::Value, String> {
        use crate::application::ScriptGlobalShortcut as R;
        match request {
            R::Register { shortcut } => {
                let handle = self.root.host().desktop_services().ok_or("global shortcuts unavailable")?.register_shortcut(shortcut)?;
                let id = handle.id(); self.shortcuts.insert(id, handle); Ok(serde_json::json!(id))
            }
            R::Reply { id } => serde_json::to_value(self.shortcuts.get(&id).ok_or("stale shortcut handle")?.reply()).map_err(|e| e.to_string()),
            R::Events { id } => serde_json::to_value(self.shortcuts.get(&id).ok_or("stale shortcut handle")?.events()).map_err(|e| e.to_string()),
            R::Release { id } => { self.shortcuts.remove(&id).ok_or("stale shortcut handle")?; Ok(serde_json::Value::Null) }
        }
    }
    fn updater_request(&mut self, request: crate::application::ScriptUpdater) -> Result<serde_json::Value, String> {
        use crate::application::ScriptUpdater as R;
        let Some(service) = self.root.host().updater() else {
            return match request {
                R::Snapshot | R::Check { .. } => Ok(serde_json::json!(jadix_core::updater::Updater::default().snapshot())),
                _ => Err("application updates are unavailable on this host".into()),
            };
        };
        match request {
            R::Check { channel, locale } => service.check(channel.as_deref(), locale.as_deref())?,
            R::Download => service.download()?, R::Install { restart } => service.install(restart)?,
            R::Cancel => service.cancel()?, R::Snapshot => {},
        }
        Ok(serde_json::json!(service.snapshot()))
    }

    fn websocket_request(&mut self, request: crate::application::ScriptWebSocket) -> Result<serde_json::Value, String> {
        use crate::application::ScriptWebSocket as R;
        match request {
            R::Connect { request } => { let handle = self.root.host().websocket().ok_or("WebSocket is unavailable on this host")?.connect(request)?; let id=handle.id(); self.sockets.insert(id,handle); Ok(serde_json::json!(id)) }
            R::Send { id, message } => { self.sockets.get(&id).ok_or("stale WebSocket")?.send(message)?; Ok(serde_json::Value::Null) }
            R::Events { id } => serde_json::to_value(self.sockets.get(&id).ok_or("stale WebSocket")?.events()).map_err(|e|e.to_string()),
            R::Close { id, code, reason } => { self.sockets.get(&id).ok_or("stale WebSocket")?.close(code,reason)?; Ok(serde_json::Value::Null) }
            R::Release { id } => { self.sockets.remove(&id).ok_or("stale WebSocket")?; Ok(serde_json::Value::Null) }
        }
    }
    fn fetch_request(&mut self, request: crate::application::ScriptFetch) -> Result<serde_json::Value, String> {
        use crate::application::ScriptFetch as R;
        match request {
            R::Send { request } => {
                let handle = self.root.host().fetch().ok_or("HTTP is unavailable on this host")?.send(request).map_err(|e| e.to_string())?;
                let id = handle.id(); self.fetches.insert(id, handle); Ok(serde_json::json!(id))
            }
            R::Events { id } => serde_json::to_value(self.fetches.get(&id).ok_or("stale HTTP request")?.events()).map_err(|e| e.to_string()),
            R::Cancel { id } => { self.fetches.get(&id).ok_or("stale HTTP request")?.cancel(); Ok(serde_json::Value::Null) }
            R::Release { id } => { self.fetches.remove(&id).ok_or("stale HTTP request")?; Ok(serde_json::Value::Null) }
        }
    }
    fn speech_request(&mut self, request: crate::application::ScriptSpeech) -> Result<serde_json::Value, String> {
        use crate::application::ScriptSpeech as R;
        match request {
            R::Start { request } => {
                let session = self.root.host().speech().ok_or("speech recognition is unavailable on this host")?.start(request)?;
                let id = session.id(); self.speech.insert(id, session); Ok(serde_json::json!(id))
            }
            R::Stop { id } => { self.speech.get(&id).ok_or("stale speech session")?.stop()?; Ok(serde_json::Value::Null) }
            R::Abort { id } => { self.speech.get(&id).ok_or("stale speech session")?.abort(); Ok(serde_json::Value::Null) }
            R::Events { id } => serde_json::to_value(self.speech.get(&id).ok_or("stale speech session")?.events()).map_err(|e| e.to_string()),
            R::Release { id } => { self.speech.remove(&id).ok_or("stale speech session")?; Ok(serde_json::Value::Null) }
        }
    }
    fn audio_request(&mut self, request: crate::application::ScriptAudio) -> Result<serde_json::Value, String> {
        use crate::application::ScriptAudio as R;
        let value = match request {
            R::Load { source } => {
                let service = self.root.host().audio().ok_or("audio is unavailable on this host")?;
                // A separate script handle keeps explicit release independent of source deduplication.
                let sound = service.load_sound(source)?;
                if self.sounds.len() >= 64 { return Err("sound handle quota exceeded".into()); }
                let handle = self.next_audio.checked_add(1).ok_or("audio handles exhausted")?; self.next_audio = handle;
                self.sounds.insert(handle, sound); return Ok(serde_json::json!(handle));
            }
            R::Open { source, settings } => {
                let service = self.root.host().audio().ok_or("audio is unavailable on this host")?;
                let player = service.open_player(source, settings)?;
                if self.players.len() >= 64 { return Err("player handle quota exceeded".into()); }
                let handle = self.next_audio.checked_add(1).ok_or("audio handles exhausted")?; self.next_audio = handle;
                self.players.insert(handle, player); return Ok(serde_json::json!(handle));
            }
            R::Play { id, volume, rate } => { self.sounds.get(&id).ok_or("stale sound handle")?.play(volume, rate)?; serde_json::Value::Null }
            R::Stop { id } => { self.sounds.get(&id).ok_or("stale sound handle")?.stop()?; serde_json::Value::Null }
            R::Preload { id } => { self.players.get(&id).ok_or("stale player handle")?.preload()?; serde_json::Value::Null }
            R::Resume { id } => { self.players.get(&id).ok_or("stale player handle")?.play()?; serde_json::Value::Null }
            R::Pause { id } => { self.players.get(&id).ok_or("stale player handle")?.pause()?; serde_json::Value::Null }
            R::Seek { id, position } => { self.players.get(&id).ok_or("stale player handle")?.seek(position)?; serde_json::Value::Null }
            R::Set { id, settings } => { self.players.get(&id).ok_or("stale player handle")?.set(settings)?; serde_json::Value::Null }
            R::Snapshot { id } => serde_json::to_value(if let Some(sound) = self.sounds.get(&id) { sound.snapshot()? } else { self.players.get(&id).ok_or("stale audio handle")?.snapshot()? }).map_err(|e| e.to_string())?,
            R::Events { id } => serde_json::to_value(if let Some(sound) = self.sounds.get(&id) { sound.events() } else { self.players.get(&id).ok_or("stale audio handle")?.events() }).map_err(|e| e.to_string())?,
            R::Release { id } => { if self.sounds.remove(&id).is_none() && self.players.remove(&id).is_none() { return Err("stale audio handle".into()); }; serde_json::Value::Null }
        };
        Ok(value)
    }
    fn poll_windows(&mut self) -> Result<(), ApplicationError> {
        let updates = self.app.take_window_updates();
        for update in updates {
            let result = (|| {
                if !self.windows.contains_key(&update.id) {
                    let service = self
                        .root
                        .host()
                        .window_declarations()
                        .ok_or("window declarations are unavailable on this host")?;
                    let declaration = service.create(update.id.clone())?;
                    self.windows
                        .insert(update.id.clone(), (update.key.clone(), declaration));
                }
                let (key, declaration) = self.windows.get_mut(&update.id).unwrap();
                *key = update.key.clone();
                declaration.update(update.options)
            })();
            if let Err(error) = result {
                self.app.window_event(
                    &update.key,
                    serde_json::json!({"id":update.id,"error":error}),
                )?;
            }
        }
        let mut events = Vec::new();
        for (id, (key, declaration)) in &self.windows {
            while let Some(error) = declaration.try_error() {
                events.push((key.clone(), serde_json::json!({"id":id,"error":error})));
            }
            let mut latest = None;
            while let Some(state) = declaration.try_state() {
                latest = Some(state);
            }
            if let Some(state) = latest {
                events.push((
                    key.clone(),
                    serde_json::json!({"id":id,"state":state,"closed":state.is_none()}),
                ));
            }
        }
        for (key, event) in events {
            self.app.window_event(&key, event)?;
        }
        Ok(())
    }
    pub fn dispose(&mut self) {
        self.service_replies.clear();
        self.windows.clear();
        self.shares.clear();
        self.urls.clear();
        self.files.clear(); self.clipboards.clear();
        self.evaluations.clear();
        self.root.dispose();
        let root = self.root.model().id(); self.root.host_mut().pump_audio(root);
        self.web_wakes.extend(self.root.pump_web().into_iter().filter(|batch| batch.needs_wake()));
        self.app.dispose();
    }
}
impl<Host: PlatformHost> Drop for NativeApplication<Host> {
    fn drop(&mut self) {
        self.dispose();
        for batch in self.take_web_wakes() { batch.wake(); }
    }
}

fn collect_desktop_results<T: serde::Serialize>(
    pending: &mut Vec<(String, std::sync::mpsc::Receiver<Result<T, String>>)>,
    results: &mut Vec<(String, serde_json::Value)>,
) {
    let mut index = 0;
    while index < pending.len() {
        let result = match pending[index].1.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                index += 1;
                continue;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("desktop request disconnected".into())
            }
        };
        let (key, _) = pending.remove(index);
        results.push((
            key,
            match result {
                Ok(value) => serde_json::json!({"value":value}),
                Err(error) => serde_json::json!({"error":error}),
            },
        ));
    }
}
