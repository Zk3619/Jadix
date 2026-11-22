//! WinRT callbacks use an owner-filtered bounded queue. Notification identity
//! belongs to the installed application, never to a borrowed system app.
use super::*;
use jadix_core::desktop_services::{DesktopServices, Notification, NotificationActivation, NotificationAuthorization, ServiceError, ServiceEvent};
use std::sync::{Arc, Mutex};
use windows::{core::{HSTRING, IInspectable}, Data::Xml::Dom::XmlDocument, Foundation::TypedEventHandler, UI::Notifications::*};
mod activation;

#[derive(Default)]
struct Queue {
    closed: bool, active: BTreeSet<u32>, events: VecDeque<(u32, ServiceEvent)>,
    activations: VecDeque<activation::Payload>,
    app_id: Option<String>,
}
type Pending = Arc<Mutex<Queue>>;

pub(super) struct Notifications {
    app_id: RefCell<Option<String>>,
    entries: RefCell<BTreeMap<u32, Registration>>,
    pending: Pending,
    wake: Arc<dyn Fn() + Send + Sync>,
    activation: RefCell<Option<activation::Registration>>,
    seen: RefCell<VecDeque<String>>,
    arguments: RefCell<Vec<String>>,
}
struct Registration {
    notifier: ToastNotifier,
    toast: ToastNotification,
    tokens: [Option<i64>; 3],
    data: String,
    token: String,
}
impl Notifications {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self { app_id: RefCell::new(None), entries: Default::default(), pending: Default::default(), wake: Arc::new(wake), activation: Default::default(), seen: Default::default(), arguments: Default::default() }
    }
    pub fn set_identity(&self, identity: Option<&str>) -> Result<(), String> {
        self.set_identity_arguments(identity, &[])
    }
    pub fn set_identity_arguments(&self, identity: Option<&str>, arguments: &[String]) -> Result<(), String> {
        if identity.is_some_and(|s| s.is_empty() || s.encode_utf16().count() > 128 || s.chars().any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '/'))) {
            return Err("invalid notification AppUserModelID".into());
        }
        if self.app_id.borrow().as_deref() == identity && self.arguments.borrow().as_slice() == arguments { return Ok(()); }
        let registration = identity.map(|identity| activation::Registration::with_arguments(identity, arguments, self.pending.clone(), self.wake.clone())).transpose()?;
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).app_id = identity.map(str::to_owned);
        self.clear();
        *self.activation.borrow_mut() = registration;
        *self.app_id.borrow_mut() = identity.map(str::to_owned);
        *self.arguments.borrow_mut() = arguments.to_vec();
        Ok(())
    }
    fn notifier(&self) -> Result<ToastNotifier, ServiceError> {
        let identity = self.app_id.borrow().clone().or_else(|| unsafe {
            let pointer = windows::Win32::UI::Shell::GetCurrentProcessExplicitAppUserModelID().ok()?;
            let value = pointer.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(pointer.0.cast()));
            value
        });
        match identity {
            Some(identity) => ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(identity)),
            None => ToastNotificationManager::CreateToastNotifier(),
        }.map_err(|_| ServiceError::Unavailable)
    }
    pub fn permission(&self) -> (NotificationAuthorization, Option<ServiceError>) {
        match self.notifier().and_then(|notifier| setting(&notifier).map_err(|_| ServiceError::Failed)) {
            Ok(NotificationSetting::Enabled) => (NotificationAuthorization::Authorized, None),
            Ok(NotificationSetting::DisabledForApplication | NotificationSetting::DisabledForUser | NotificationSetting::DisabledByGroupPolicy | NotificationSetting::DisabledByManifest) => (NotificationAuthorization::Denied, None),
            Ok(_) => (NotificationAuthorization::Unknown, None),
            Err(error) => (NotificationAuthorization::Unknown, Some(error)),
        }
    }
    pub fn notify(&self, id: u32, notification: Notification) -> Result<(), ServiceError> {
        if self.pending.lock().unwrap_or_else(|e| e.into_inner()).closed || !notification.validate() || self.entries.borrow().len() >= 128 { return Err(ServiceError::Failed); }
        let notifier = self.notifier()?;
        if setting(&notifier).map_err(|_| ServiceError::Failed)? != NotificationSetting::Enabled { return Err(ServiceError::Denied); }
        let token = activation::token(id);
        let document = content(&notification).map_err(|_| ServiceError::Failed)?;
        document.DocumentElement().and_then(|root| root.SetAttribute(&HSTRING::from("launch"), &HSTRING::from(activation::encode(&token, id, &notification.data)))).map_err(|_| ServiceError::Failed)?;
        let toast = ToastNotification::CreateToastNotification(&document).map_err(|_| ServiceError::Failed)?;
        let mut registration = Registration { notifier, toast, tokens: [None; 3], data: notification.data, token };
        let subscribe = (|| -> windows::core::Result<()> {
            registration.toast.SetTag(&HSTRING::from(id.to_string()))?;
            registration.toast.SetGroup(&HSTRING::from(format!("jadix-{}", std::process::id())))?;
            let pending = self.pending.clone(); let wake = self.wake.clone();
            registration.tokens[0] = Some(registration.toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
                enqueue(&pending, &wake, id, ServiceEvent::Click); Ok(())
            }))?);
            let pending = self.pending.clone(); let wake = self.wake.clone();
            registration.tokens[1] = Some(registration.toast.Dismissed(&TypedEventHandler::<ToastNotification, ToastDismissedEventArgs>::new(move |_, args| {
                // A timed-out banner is still actionable in Notification Center.
                if args.as_ref().is_some_and(|args| args.Reason().ok() == Some(ToastDismissalReason::TimedOut)) { return Ok(()); }
                enqueue(&pending, &wake, id, ServiceEvent::Dismiss); Ok(())
            }))?);
            let pending = self.pending.clone(); let wake = self.wake.clone();
            registration.tokens[2] = Some(registration.toast.Failed(&TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(move |_, _| {
                enqueue(&pending, &wake, id, ServiceEvent::Failed { error: ServiceError::Failed }); Ok(())
            }))?);
            self.pending.lock().unwrap_or_else(|e| e.into_inner()).active.insert(id);
            registration.notifier.Show(&registration.toast)
        })();
        if subscribe.is_err() { self.cancel(id); return Err(ServiceError::Failed); }
        self.entries.borrow_mut().insert(id, registration);
        Ok(())
    }
    pub fn pump(&self, service: &DesktopServices, window: HWND) {
        let pending: Vec<_> = self.pending.lock().unwrap_or_else(|e| e.into_inner()).events.drain(..).collect();
        for (id, event) in pending {
            let data = self.entries.borrow().get(&id).map(|entry| (entry.data.clone(), entry.token.clone()));
            let Some((data, token)) = data else { continue; };
            let clicked = matches!(event, ServiceEvent::Click);
            if service.event(id, event) && clicked {
                self.remember(token);
                service.activate(NotificationActivation { identifier: id.to_string(), data });
                unsafe { if IsIconic(window) != 0 { ShowWindow(window, SW_RESTORE); } SetForegroundWindow(window); }
            }
            self.cancel(id);
        }
        let activations: Vec<_> = self.pending.lock().unwrap_or_else(|e| e.into_inner()).activations.drain(..).collect();
        for activation in activations {
            if self.seen.borrow().contains(&activation.token) { continue; }
            self.remember(activation.token.clone());
            if self.entries.borrow().get(&activation.id).is_some_and(|entry| entry.token == activation.token) {
                service.event(activation.id, ServiceEvent::Click);
                self.cancel(activation.id);
            }
            if service.activate(NotificationActivation { identifier: activation.id.to_string(), data: activation.data }) {
                unsafe { if IsIconic(window) != 0 { ShowWindow(window, SW_RESTORE); } SetForegroundWindow(window); }
            }
        }
    }
    fn remember(&self, token: String) {
        let mut seen = self.seen.borrow_mut();
        if seen.len() >= 256 { seen.pop_front(); }
        seen.push_back(token);
    }
    pub fn cancel(&self, id: u32) {
        {
            let mut queue = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            queue.active.remove(&id);
            queue.events.retain(|(owner, _)| *owner != id);
        }
        let entry = self.entries.borrow_mut().remove(&id);
        if let Some(entry) = entry { self.remember(entry.token.clone()); }
    }
    fn clear(&self) {
        {
            let mut queue = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            queue.active.clear(); queue.events.clear(); queue.activations.clear();
        }
        // Removing WinRT handlers may wait for a callback; release the queue first.
        self.entries.borrow_mut().clear();
    }
    pub fn dispose(&self) {
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        self.clear();
        self.activation.borrow_mut().take();
    }
}
fn setting(notifier: &ToastNotifier) -> windows::core::Result<NotificationSetting> {
    match notifier.Setting() {
        Err(error) if error.code() == windows::Win32::Foundation::ERROR_NOT_FOUND.to_hresult() => Ok(NotificationSetting::Enabled),
        result => result,
    }
}
impl Drop for Notifications { fn drop(&mut self) { self.dispose(); } }
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(token) = self.tokens[0] { let _ = self.toast.RemoveActivated(token); }
        if let Some(token) = self.tokens[1] { let _ = self.toast.RemoveDismissed(token); }
        if let Some(token) = self.tokens[2] { let _ = self.toast.RemoveFailed(token); }
        let _ = self.notifier.Hide(&self.toast);
    }
}
fn enqueue(queue: &Pending, wake: &Arc<dyn Fn() + Send + Sync>, id: u32, event: ServiceEvent) {
    let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
    if queue.closed || !queue.active.contains(&id) || queue.events.len() >= 256 { return; }
    queue.events.push_back((id, event));
    // This only posts a message. Disposal waits here before HWND destruction.
    wake();
}
fn content(notification: &Notification) -> windows::core::Result<XmlDocument> {
    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from("<toast><visual><binding template=\"ToastGeneric\"/></visual></toast>"))?;
    let binding = document.GetElementsByTagName(&HSTRING::from("binding"))?.Item(0)?;
    for value in [&notification.title, &notification.subtitle, &notification.body] {
        if value.is_empty() { continue; }
        let text = document.CreateElement(&HSTRING::from("text"))?;
        text.AppendChild(&document.CreateTextNode(&HSTRING::from(value))?)?;
        binding.AppendChild(&text)?;
    }
    if !notification.sound {
        let audio = document.CreateElement(&HSTRING::from("audio"))?;
        audio.SetAttribute(&HSTRING::from("silent"), &HSTRING::from("true"))?;
        document.DocumentElement()?.AppendChild(&audio)?;
    }
    Ok(document)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_callbacks_never_wake_a_destroyed_window() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0)); let observed = count.clone();
        let notifications = Notifications::new(move || { observed.fetch_add(1, Ordering::Relaxed); });
        notifications.pending.lock().unwrap().active.insert(7);
        enqueue(&notifications.pending, &notifications.wake, 7, ServiceEvent::Click);
        assert_eq!(count.load(Ordering::Relaxed), 1);
        notifications.cancel(7);
        enqueue(&notifications.pending, &notifications.wake, 7, ServiceEvent::Click);
        assert!(notifications.pending.lock().unwrap().events.is_empty());
        notifications.pending.lock().unwrap().active.insert(8);
        notifications.dispose();
        enqueue(&notifications.pending, &notifications.wake, 8, ServiceEvent::Dismiss);
        assert_eq!(count.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn toast_xml_preserves_unicode_and_escapes_markup() {
        let _apartment = super::super::accessibility::Accessibility::new().unwrap();
        let document = content(&Notification { title: "<中文 & \u{1f642}>".into(), subtitle: String::new(), body: "第二行".into(), sound: false, data: "private".into() }).unwrap();
        let xml = document.GetXml().unwrap().to_string();
        assert!(xml.contains("&lt;中文 &amp; \u{1f642}&gt;"), "{xml}");
        assert!(xml.contains("silent=\"true\""), "{xml}");
        assert!(!xml.contains("private"));
    }
}
