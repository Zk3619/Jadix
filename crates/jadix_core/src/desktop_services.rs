//! Root-owned OS notifications and global shortcuts. Dropping a handle cancels
//! its platform registration; monotonically allocated IDs reject stale replies.
use std::{cell::RefCell, collections::{BTreeMap, VecDeque}, rc::Rc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HapticImpact { Light, Medium, Heavy, Soft, Rigid }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HapticNotification { Success, Warning, Error }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Haptic {
    Selection,
    Impact { style: HapticImpact },
    Notification { kind: HapticNotification },
}
/// A system browser authentication session. Applications validate OAuth state/nonce.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthSessionRequest {
    pub url: String,
    pub callback_scheme: String,
    #[serde(default)] pub ephemeral: bool,
}
impl AuthSessionRequest {
    pub fn validate(&self) -> bool {
        let scheme = self.callback_scheme.as_bytes();
        !scheme.is_empty() && scheme.len() <= 64 && scheme[0].is_ascii_alphabetic()
            && scheme.iter().all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(b))
            && !matches!(self.callback_scheme.to_ascii_lowercase().as_str(),
                "http" | "https" | "file" | "javascript" | "vbscript" | "data" | "blob" | "about" | "asset")
            && crate::desktop::check_open_url(&self.url).is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.username().is_empty() && url.password().is_none())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthSessionOutcome { Completed { url: String }, Cancelled }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceError { Unavailable, Denied, Conflict, Failed }
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationAuthorization { NotDetermined, Denied, Authorized, Provisional, Ephemeral, Unknown }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub title: String,
    #[serde(default)] pub subtitle: String,
    #[serde(default)] pub body: String,
    #[serde(default)] pub sound: bool,
    #[serde(default)] pub data: String,
}
impl Notification {
    pub fn validate(&self) -> bool {
        [&self.title, &self.subtitle].into_iter().all(|v| v.len() <= 256 && !v.contains('\0'))
            && self.body.len() <= 4096 && !self.body.contains('\0') && self.data.len() <= 4096 && !self.data.contains('\0')
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalShortcut {
    pub key: String,
    #[serde(default)] pub command: bool,
    #[serde(default)] pub control: bool,
    #[serde(default)] pub option: bool,
    #[serde(default)] pub shift: bool,
}
impl GlobalShortcut {
    pub fn validate(&self) -> bool {
        let function = self.key.strip_prefix('F').and_then(|v| v.parse::<u8>().ok()).is_some_and(|n| (1..=20).contains(&n) && self.key == format!("F{n}"));
        let key = self.key.len() == 1 && self.key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            || function || matches!(self.key.as_str(), "Space" | "Tab" | "Enter" | "Escape" | "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End" | "PageUp" | "PageDown" | "Backspace" | "Delete");
        key && (function || self.command || self.control || self.option)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceReply {
    AuthSession { outcome: Option<AuthSessionOutcome>, error: Option<ServiceError> },
    Complete { error: Option<ServiceError> },
    Permission { authorization: NotificationAuthorization, error: Option<ServiceError> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServiceEvent { Shortcut, Click, Dismiss, Failed { error: ServiceError } }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationActivation { pub identifier: String, pub data: String }
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ServiceCommand {
    OpenAuthSession { id: u32, request: AuthSessionRequest },
    CloseAuthSession { id: u32 },
    Haptic { id: u32, feedback: Haptic },
    Permission { id: u32, request: bool },
    Notify { id: u32, notification: Notification },
    CancelNotification { id: u32 },
    Badge { id: u32, label: Option<String> },
    RegisterShortcut { id: u32, shortcut: GlobalShortcut },
    UnregisterShortcut { id: u32 },
}
impl ServiceCommand {
    pub fn id(&self) -> u32 { match self { Self::OpenAuthSession { id, .. } | Self::CloseAuthSession { id } | Self::Haptic { id, .. } | Self::Permission { id, .. } | Self::Notify { id, .. } | Self::CancelNotification { id } | Self::Badge { id, .. } | Self::RegisterShortcut { id, .. } | Self::UnregisterShortcut { id } => *id } }
}
enum Kind { AuthSession(String), Haptic, Permission, Badge, Notification, Shortcut(GlobalShortcut) }
struct Entry { kind: Kind, dispatched: bool, reply: Option<ServiceReply>, events: VecDeque<ServiceEvent> }
#[derive(Default)]
struct State { serial: u32, entries: BTreeMap<u32, Entry>, commands: VecDeque<ServiceCommand>, activations: VecDeque<NotificationActivation>, disposed: bool }
#[derive(Clone, Default)]
pub struct DesktopServices(Rc<RefCell<State>>, Rc<RefCell<Option<Rc<dyn Fn()>>>>);
struct Lease { id: u32, service: DesktopServices }
#[derive(Clone)]
pub struct ServiceHandle(Rc<Lease>);
impl DesktopServices {
    /// The host callback schedules a later turn; it must not reenter the app.
    pub fn set_command_waker(&self, wake: Option<Rc<dyn Fn()>>) { *self.1.borrow_mut() = wake; }
    fn wake_commands(&self) { let wake = self.1.borrow().clone(); if let Some(wake) = wake { wake(); } }
    fn allocate(&self, kind: Kind, command: impl FnOnce(u32) -> ServiceCommand) -> Result<ServiceHandle, String> {
        let mut state = self.0.borrow_mut();
        if state.disposed || state.entries.len() >= 128 || state.commands.len() >= 256 { return Err("desktop service unavailable or quota exceeded".into()); }
        if let Kind::Shortcut(shortcut) = &kind {
            if state.entries.values().any(|entry| matches!(&entry.kind, Kind::Shortcut(other) if other == shortcut)) { return Err("global shortcut already registered".into()); }
        }
        let id = state.serial.checked_add(1).ok_or("desktop service identities exhausted")?;
        state.serial = id;
        state.entries.insert(id, Entry { kind, dispatched: false, reply: None, events: VecDeque::new() });
        state.commands.push_back(command(id)); drop(state); self.wake_commands(); Ok(ServiceHandle(Rc::new(Lease { id, service: self.clone() })))
    }
    /// A valid replacement cancels the previous session; invalid requests leave it running.
    pub fn open_auth_session(&self, request: AuthSessionRequest) -> Result<ServiceHandle, String> {
        if !request.validate() { return Err("invalid authentication session".into()); }
        let handle = self.allocate(Kind::AuthSession(request.callback_scheme.to_ascii_lowercase()),
            |id| ServiceCommand::OpenAuthSession { id, request })?;
        self.cancel_auth_sessions(Some(handle.id()));
        Ok(handle)
    }
    pub fn close_auth_session(&self) { self.cancel_auth_sessions(None); }
    /// Route a browser callback to the dispatched, live session. OAuth state and
    /// nonce validation remain the application's responsibility.
    pub fn complete_auth_callback(&self, url: &str) -> bool {
        if url.len() > 16 * 1024 || url.chars().any(|c| c.is_control() || c.is_whitespace()) { return false; }
        let Ok(parsed) = url::Url::parse(url) else { return false; };
        let id = self.0.borrow().entries.iter().find_map(|(&id, entry)|
            (entry.dispatched && entry.reply.is_none()
                && matches!(&entry.kind, Kind::AuthSession(scheme) if scheme == parsed.scheme())).then_some(id));
        id.is_some_and(|id| self.reply(id, ServiceReply::AuthSession {
            outcome: Some(AuthSessionOutcome::Completed { url: url.to_owned() }), error: None,
        }))
    }
    fn cancel_auth_sessions(&self, keep: Option<u32>) {
        let mut state = self.0.borrow_mut();
        let mut cancelled = Vec::new();
        for (&id, entry) in &mut state.entries {
            if Some(id) != keep && matches!(entry.kind, Kind::AuthSession(_)) && entry.reply.is_none() {
                entry.reply = Some(ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Cancelled), error: None });
                cancelled.push((id, entry.dispatched));
            }
        }
        for (id, dispatched) in cancelled {
            state.commands.retain(|command| command.id() != id);
            if dispatched { state.commands.push_back(ServiceCommand::CloseAuthSession { id }); }
        }
        drop(state); self.wake_commands();
    }
    /// Unsupported platforms and disabled hardware may complete without feedback.
    pub fn haptic(&self, feedback: Haptic) -> Result<ServiceHandle, String> { self.allocate(Kind::Haptic, |id| ServiceCommand::Haptic { id, feedback }) }
    pub fn permission(&self, request: bool) -> Result<ServiceHandle, String> { self.allocate(Kind::Permission, |id| ServiceCommand::Permission { id, request }) }
    pub fn notify(&self, notification: Notification) -> Result<ServiceHandle, String> {
        if !notification.validate() { return Err("invalid notification".into()); }
        self.allocate(Kind::Notification, |id| ServiceCommand::Notify { id, notification })
    }
    pub fn badge(&self, label: Option<String>) -> Result<ServiceHandle, String> {
        if label.as_ref().is_some_and(|v| v.len() > 64 || v.contains('\0')) { return Err("invalid badge".into()); }
        self.allocate(Kind::Badge, |id| ServiceCommand::Badge { id, label })
    }
    pub fn register_shortcut(&self, shortcut: GlobalShortcut) -> Result<ServiceHandle, String> {
        if !shortcut.validate() { return Err("invalid global shortcut".into()); }
        self.allocate(Kind::Shortcut(shortcut.clone()), |id| ServiceCommand::RegisterShortcut { id, shortcut })
    }
    pub fn take_commands(&self) -> Vec<ServiceCommand> {
        let mut state = self.0.borrow_mut(); let commands: Vec<_> = state.commands.drain(..).collect();
        for command in &commands { if let Some(entry) = state.entries.get_mut(&command.id()) { entry.dispatched = true; } } commands
    }
    pub fn reply(&self, id: u32, reply: ServiceReply) -> bool {
        let mut state = self.0.borrow_mut(); let Some(entry) = state.entries.get_mut(&id) else { return false; };
        if !entry.dispatched || entry.reply.is_some() { return false; }
        let valid = match (&entry.kind, &reply) {
            (Kind::Permission, ServiceReply::Permission { .. }) => true,
            (Kind::AuthSession(scheme), ServiceReply::AuthSession { outcome, error }) => match (outcome, error) {
                (Some(AuthSessionOutcome::Cancelled), None) | (None, Some(_)) => true,
                (Some(AuthSessionOutcome::Completed { url }), None) => url.len() <= 16 * 1024
                    && !url.chars().any(|c| c.is_control() || c.is_whitespace())
                    && url::Url::parse(url).is_ok_and(|url| url.scheme() == scheme),
                _ => false,
            },
            (Kind::Haptic | Kind::Badge | Kind::Notification | Kind::Shortcut(_), ServiceReply::Complete { .. }) => true,
            _ => false,
        };
        if !valid { return false; }
        entry.reply = Some(reply); true
    }
    pub fn event(&self, id: u32, event: ServiceEvent) -> bool {
        let mut state = self.0.borrow_mut(); let Some(entry) = state.entries.get_mut(&id) else { return false; };
        if !entry.dispatched || !matches!(entry.reply, Some(ServiceReply::Complete { error: None })) { return false; }
        if !matches!((&entry.kind, &event), (Kind::Shortcut(_), ServiceEvent::Shortcut) | (Kind::Notification, ServiceEvent::Click | ServiceEvent::Dismiss | ServiceEvent::Failed { .. })) { return false; }
        if entry.events.len() >= 64 { entry.events.pop_front(); } entry.events.push_back(event); true
    }
    pub fn activate(&self, activation: NotificationActivation) -> bool {
        if activation.identifier.len() > 256 || activation.data.len() > 4096 || activation.identifier.contains('\0') || activation.data.contains('\0') { return false; }
        let mut state = self.0.borrow_mut(); if state.disposed { return false; }
        if state.activations.len() >= 64 { state.activations.pop_front(); } state.activations.push_back(activation); true
    }
    pub fn activations(&self) -> Vec<NotificationActivation> { self.0.borrow_mut().activations.drain(..).collect() }
    pub fn dispose(&self) { self.set_command_waker(None); let mut state = self.0.borrow_mut(); state.disposed = true; state.entries.clear(); state.commands.clear(); state.activations.clear(); }
}
impl ServiceHandle {
    pub fn id(&self) -> u32 { self.0.id }
    pub fn reply(&self) -> Option<ServiceReply> { self.0.service.0.borrow().entries.get(&self.0.id)?.reply.clone() }
    pub fn events(&self) -> Vec<ServiceEvent> { self.0.service.0.borrow_mut().entries.get_mut(&self.0.id).map(|e| e.events.drain(..).collect()).unwrap_or_default() }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.service.0.borrow_mut(); let Some(entry) = state.entries.remove(&self.id) else { return; };
        let close_pending = state.commands.iter().any(|command| matches!(command,
            ServiceCommand::CloseAuthSession { id } if *id == self.id));
        state.commands.retain(|command| command.id() != self.id);
        if entry.dispatched { match entry.kind {
            Kind::AuthSession(_) if entry.reply.is_none() || close_pending => state.commands.push_back(ServiceCommand::CloseAuthSession { id: self.id }),
            Kind::Notification => state.commands.push_back(ServiceCommand::CancelNotification { id: self.id }),
            Kind::Shortcut(_) => state.commands.push_back(ServiceCommand::UnregisterShortcut { id: self.id }),
            _ => {}
        } }
        drop(state); self.service.wake_commands();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asynchronous_notification_failure_is_typed_and_owner_scoped() {
        let service = DesktopServices::default();
        let handle = service.notify(Notification { title: "demo".into(), subtitle: String::new(), body: String::new(), sound: false, data: String::new() }).unwrap();
        let shortcut = service.register_shortcut(GlobalShortcut { key: "F12".into(), command: false, control: false, option: false, shift: false }).unwrap();
        let failed = ServiceEvent::Failed { error: ServiceError::Denied };
        assert!(!service.event(handle.id(), failed.clone()));
        service.take_commands();
        service.reply(handle.id(), ServiceReply::Complete { error: None });
        service.reply(shortcut.id(), ServiceReply::Complete { error: None });
        assert!(!service.event(shortcut.id(), failed.clone()));
        assert!(service.event(handle.id(), failed.clone()));
        assert_eq!(handle.events(), [failed.clone()]);
        assert_eq!(serde_json::to_string(&failed).unwrap(), r#"{"type":"failed","error":"denied"}"#);
        let id = handle.id(); drop(handle); assert!(!service.event(id, failed));
    }
    fn auth() -> AuthSessionRequest { AuthSessionRequest { url: "https://example.com/login".into(), callback_scheme: "Demo.Auth".into(), ephemeral: true } }
    #[test]
    fn permission_request_error_preserves_the_observed_authorization() {
        let service = DesktopServices::default();
        let handle = service.permission(true).unwrap();
        service.take_commands();
        let reply = ServiceReply::Permission { authorization: NotificationAuthorization::Denied, error: Some(ServiceError::Failed) };
        assert!(service.reply(handle.id(), reply.clone()));
        assert_eq!(handle.reply(), Some(reply));
        assert!(!service.reply(handle.id(), ServiceReply::Permission { authorization: NotificationAuthorization::Authorized, error: None }));
    }
    #[test]
    fn auth_replacement_cancellation_and_stale_completion() {
        let service = DesktopServices::default();
        let first = service.open_auth_session(auth()).unwrap();
        let mut bad = auth(); bad.callback_scheme = "https".into();
        assert!(service.open_auth_session(bad).is_err()); assert!(first.reply().is_none());
        service.take_commands();
        let second = service.open_auth_session(auth()).unwrap();
        assert_eq!(first.reply(), Some(ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Cancelled), error: None }));
        assert!(matches!(service.take_commands().as_slice(), [ServiceCommand::OpenAuthSession { .. }, ServiceCommand::CloseAuthSession { id }] if *id == first.id()));
        let completed = |url: &str| ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Completed { url: url.into() }), error: None };
        assert!(!service.reply(first.id(), completed("demo.auth://callback")));
        assert!(!service.reply(second.id(), ServiceReply::Complete { error: None }));
        assert!(!service.reply(second.id(), completed("other://callback")));
        assert!(service.reply(second.id(), completed("demo.auth://callback?state=123")));
        assert!(!service.reply(second.id(), completed("demo.auth://callback")));
        drop(second); assert!(service.take_commands().is_empty());
        let queued = service.open_auth_session(auth()).unwrap();
        service.close_auth_session(); assert!(service.take_commands().is_empty());
        assert!(queued.reply().is_some());
        let running = service.open_auth_session(auth()).unwrap(); service.take_commands();
        let id = running.id(); drop(running);
        assert!(matches!(service.take_commands().as_slice(), [ServiceCommand::CloseAuthSession { id: closed }] if *closed == id));
        assert!(!service.reply(id, completed("demo.auth://callback")));
        let running = service.open_auth_session(auth()).unwrap(); service.take_commands();
        let id = running.id(); service.close_auth_session(); drop(running);
        // Reading cancellation and releasing its handle must not discard the native close.
        assert!(matches!(service.take_commands().as_slice(), [ServiceCommand::CloseAuthSession { id: closed }] if *closed == id));
    }
    #[test]
    fn auth_rejects_unsafe_schemes_and_non_web_start_urls() {
        for scheme in ["", "1demo", "https", "FILE", "javascript", "a b", "a:b"] {
            let mut request = auth(); request.callback_scheme = scheme.into(); assert!(!request.validate());
        }
        for url in ["file:///tmp/x", "demo://callback", "https://user:pass@example.com", "javascript:alert(1)"] {
            let mut request = auth(); request.url = url.into(); assert!(!request.validate());
        }
        assert!(auth().validate());
    }
    #[test]
    fn browser_callbacks_only_complete_dispatched_live_sessions_once() {
        let service = DesktopServices::default();
        let first = service.open_auth_session(auth()).unwrap();
        assert!(!service.complete_auth_callback("demo.auth://callback?state=1"));
        service.take_commands();
        for invalid in ["other://callback", "demo.auth://call back", "demo.auth://callback\n", ""] {
            assert!(!service.complete_auth_callback(invalid));
        }
        let second = service.open_auth_session(auth()).unwrap();
        assert!(!service.complete_auth_callback("demo.auth://callback?state=old"));
        service.take_commands();
        assert!(service.complete_auth_callback("DEMO.AUTH://callback?state=2"));
        assert_eq!(second.reply(), Some(ServiceReply::AuthSession {
            outcome: Some(AuthSessionOutcome::Completed { url: "DEMO.AUTH://callback?state=2".into() }), error: None,
        }));
        assert!(!service.complete_auth_callback("demo.auth://callback?state=2"));
        assert!(matches!(first.reply(), Some(ServiceReply::AuthSession { outcome: Some(AuthSessionOutcome::Cancelled), .. })));
        drop(second);
        let third = service.open_auth_session(auth()).unwrap(); service.take_commands(); drop(third);
        assert!(!service.complete_auth_callback("demo.auth://callback"));
        service.dispose();
        assert!(!service.complete_auth_callback("demo.auth://callback"));
    }
    #[test]
    fn haptics_are_typed_bounded_and_complete_once() {
        let service = DesktopServices::default();
        let handle = service.haptic(Haptic::Impact { style: HapticImpact::Soft }).unwrap();
        assert!(!service.reply(handle.id(), ServiceReply::Complete { error: None }));
        let commands = service.take_commands();
        assert!(matches!(commands.as_slice(), [ServiceCommand::Haptic { feedback: Haptic::Impact { style: HapticImpact::Soft }, .. }]));
        assert_eq!(serde_json::to_value(&commands[0]).unwrap()["feedback"], serde_json::json!({"type":"impact","style":"soft"}));
        assert!(service.reply(handle.id(), ServiceReply::Complete { error: None }));
        assert!(!service.reply(handle.id(), ServiceReply::Complete { error: None }));
        assert!(!service.event(handle.id(), ServiceEvent::Shortcut));
        let handles: Vec<_> = (0..127).map(|_| service.haptic(Haptic::Selection).unwrap()).collect();
        assert!(service.haptic(Haptic::Selection).is_err());
        drop(handles); drop(handle);
        assert!(service.take_commands().is_empty());
        service.dispose(); assert!(service.haptic(Haptic::Selection).is_err());
    }
    #[test]
    fn released_registrations_reject_late_events_and_preserve_other_owners() {
        let service = DesktopServices::default();
        let shortcut = GlobalShortcut { key: "K".into(), command: true, control: false, option: false, shift: true };
        let handle = service.register_shortcut(shortcut.clone()).unwrap(); let id = handle.id();
        assert!(service.register_shortcut(shortcut.clone()).is_err());
        assert!(!service.event(id, ServiceEvent::Shortcut)); service.take_commands();
        assert!(service.reply(id, ServiceReply::Complete { error: None }));
        assert!(service.event(id, ServiceEvent::Shortcut)); assert_eq!(handle.events(), [ServiceEvent::Shortcut]);
        let clone = handle.clone(); drop(handle); assert!(service.take_commands().is_empty()); drop(clone);
        assert!(matches!(service.take_commands().as_slice(), [ServiceCommand::UnregisterShortcut { id: removed }] if *removed == id));
        let next = service.register_shortcut(shortcut).unwrap(); assert_ne!(id, next.id());
        assert!(!service.event(id, ServiceEvent::Shortcut)); service.dispose(); assert!(service.take_commands().is_empty());
    }
}
