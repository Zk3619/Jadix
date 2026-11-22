//! Shared typed boundary for root-owned asynchronous system adapters.
use jadix_core::{audio::*, desktop_services::*, speech::*};
use jadix_net::service::*;
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub(crate) struct NativeServices {
    pub audio: AudioService,
    pub speech: SpeechService,
    pub fetch: FetchService,
    pub desktop: DesktopServices,
    pub deep_links: std::cell::RefCell<Option<jadix_core::deep_link::DeepLinkRouter>>,
    share_serial: std::cell::Cell<u64>,
    shares: std::cell::RefCell<std::collections::BTreeMap<u64, std::sync::mpsc::Sender<jadix_core::desktop::ShareResult>>>,
    share_commands: std::cell::RefCell<Vec<Command>>,
    disposed: std::cell::Cell<bool>,
    wake: std::cell::RefCell<Option<std::rc::Rc<dyn Fn()>>>,
}

#[derive(Serialize)]
#[serde(tag = "service", content = "command", rename_all = "snake_case")]
pub(crate) enum Command { Audio(AudioCommand), Speech(SpeechCommand), Fetch(FetchCommand), Desktop(ServiceCommand), Share(ShareCommand) }

#[derive(Serialize)]
pub(crate) struct ShareCommand { id: String, subject: Option<String>, text: Option<String>, url: Option<String>, files: Vec<String>, anchor: Option<String> }

#[derive(Deserialize)]
#[serde(tag = "service", rename_all = "snake_case", deny_unknown_fields)]
enum Event {
    Audio { id: u32, event: AudioEvent },
    Speech { id: u32, event: SpeechEvent },
    Fetch { id: u32, event: FetchEvent },
    Reply { id: u32, event: ServiceReply },
    Desktop { id: u32, event: ServiceEvent },
    Activation { event: NotificationActivation },
    Share { id: String, outcome: Option<jadix_core::desktop::ShareOutcome>, error: Option<String> },
}
impl NativeServices {
    pub fn set_command_waker(&self, wake: Option<std::rc::Rc<dyn Fn()>>) {
        self.audio.set_command_waker(wake.clone()); self.speech.set_command_waker(wake.clone());
        self.fetch.set_command_waker(wake.clone()); self.desktop.set_command_waker(wake.clone());
        *self.wake.borrow_mut() = wake;
    }
    pub fn take_commands(&self) -> Vec<Command> {
        self.audio.take_commands().into_iter().map(Command::Audio)
            .chain(self.speech.take_commands().into_iter().map(Command::Speech))
            .chain(self.fetch.take_commands().into_iter().map(Command::Fetch))
            .chain(self.desktop.take_commands().into_iter().map(Command::Desktop))
            .chain(self.share_commands.borrow_mut().drain(..)).collect()
    }
    pub fn share(&self, share: &jadix_core::desktop::Share) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>, String> {
        share.validate()?;
        if self.disposed.get() { return Err("native services disposed".into()); }
        let id = self.share_serial.get().checked_add(1).ok_or("share identity exhausted")?;
        self.share_serial.set(id);
        for (_, sender) in std::mem::take(&mut *self.shares.borrow_mut()) { let _ = sender.send(Ok(jadix_core::desktop::ShareOutcome::Replaced)); }
        self.share_commands.borrow_mut().clear();
        let (sender, receiver) = std::sync::mpsc::channel(); self.shares.borrow_mut().insert(id, sender);
        self.share_commands.borrow_mut().push(Command::Share(ShareCommand { id: id.to_string(), subject: share.subject.clone(), text: share.text.clone(), url: share.url.clone(), files: share.files.iter().map(|file| file.to_string_lossy().into_owned()).collect(), anchor: share.anchor.map(|node| node.token()) }));
        let wake = self.wake.borrow().clone(); if let Some(wake) = wake { wake(); }
        Ok(receiver)
    }
    pub fn events(&self, input: &str) -> Result<(), String> {
        if self.disposed.get() { return Err("native services disposed".into()); }
        if input.len() > 2 * 1024 * 1024 { return Err("native service event budget exceeded".into()); }
        let events: Vec<Event> = serde_json::from_str(input).map_err(|e| e.to_string())?;
        if events.len() > 128 { return Err("native service event queue exceeded".into()); }
        for event in events { match event {
            Event::Audio { id, event } => { self.audio.event(id, event); }
            Event::Speech { id, event } => { self.speech.event(id, event); }
            Event::Fetch { id, event } => { self.fetch.event(id, event); }
            Event::Reply { id, event } => { self.desktop.reply(id, event); }
            Event::Desktop { id, event } => { self.desktop.event(id, event); }
            Event::Activation { event } => { self.desktop.activate(event); }
            Event::Share { id, outcome, error } => {
                let id = id.parse::<u64>().map_err(|_| "invalid share identity")?;
                if let Some(sender) = self.shares.borrow_mut().remove(&id) {
                    let result = match (outcome, error) { (Some(outcome), None) => Ok(outcome), (_, Some(error)) if error.len() <= 4096 => Err(error), _ => Err("invalid native share result".into()) };
                    let _ = sender.send(result);
                }
            }
        } }
        Ok(())
    }
    pub fn rejected(&self, command: &Command) {
        match command {
            Command::Audio(command) => { self.audio.event(command.id(), AudioEvent::Failed { message: "native audio adapter unavailable".into() }); }
            Command::Speech(command) => {
                let (SpeechCommand::Start { id, .. } | SpeechCommand::Stop { id } | SpeechCommand::Abort { id }) = command;
                self.speech.event(*id, SpeechEvent::End { error: Some(SpeechError::Unavailable) });
            }
            Command::Fetch(command) => {
                let (FetchCommand::Send { id, .. } | FetchCommand::Cancel { id }) = command;
                self.fetch.event(*id, FetchEvent::Failed { message: "native HTTP adapter unavailable".into() });
            }
            Command::Desktop(command) => {
                let error = Some(ServiceError::Unavailable);
                self.desktop.reply(command.id(), match command {
                    ServiceCommand::Permission { .. } => ServiceReply::Permission { authorization: NotificationAuthorization::Unknown, error },
                    ServiceCommand::OpenAuthSession { .. } => ServiceReply::AuthSession { outcome: None, error },
                    _ => ServiceReply::Complete { error },
                });
            }
            Command::Share(command) => {
                if let Ok(id) = command.id.parse::<u64>() { if let Some(sender) = self.shares.borrow_mut().remove(&id) { let _ = sender.send(Err("native share adapter unavailable".into())); } }
            }
        }
    }
    pub fn dispose(&self) {
        self.set_command_waker(None);
        self.disposed.set(true);
        self.audio.dispose(); self.speech.dispose(); self.fetch.dispose(); self.desktop.dispose();
        self.share_commands.borrow_mut().clear();
        for (_, sender) in std::mem::take(&mut *self.shares.borrow_mut()) { let _ = sender.send(Ok(jadix_core::desktop::ShareOutcome::Dismissed)); }
        self.deep_links.borrow_mut().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_commands_and_last_owner_cancellation_wake_the_host() {
        let services = NativeServices::default();
        let turns = std::rc::Rc::new(std::cell::Cell::new(0));
        let observed = turns.clone();
        services.set_command_waker(Some(std::rc::Rc::new(move || observed.set(observed.get() + 1))));
        let sound = services.audio.load_sound(AudioSource::bytes(b"RIFF").unwrap()).unwrap();
        let speech = services.speech.start(Default::default()).unwrap();
        let fetch = services.fetch.send(serde_json::from_str(r#"{"url":"https://example.test"}"#).unwrap()).unwrap();
        let notification = services.desktop.permission(false).unwrap();
        assert_eq!(turns.get(), 4);
        assert_eq!(services.take_commands().len(), 4);
        drop(sound); drop(speech); drop(fetch); drop(notification);
        assert_eq!(services.take_commands().len(), 3);
        assert_eq!(turns.get(), 8);
        let share = services.share(&jadix_core::desktop::Share { text: Some("hello".into()), ..Default::default() }).unwrap();
        assert_eq!(turns.get(), 9);
        services.dispose();
        assert!(share.try_recv().is_ok());
        assert_eq!(turns.get(), 9);
    }
    #[test]
    fn replacement_and_late_share_callbacks_preserve_current_owner() {
        use jadix_core::desktop::{Share, ShareOutcome};
        let services = NativeServices::default();
        let payload = Share { text: Some("hello".into()), ..Default::default() };
        let first = services.share(&payload).unwrap();
        let second = services.share(&payload).unwrap();
        assert_eq!(first.try_recv().unwrap(), Ok(ShareOutcome::Replaced));
        assert_eq!(services.take_commands().len(), 1);
        services.events(r#"[{"service":"share","id":"1","outcome":{"status":"dismissed"}}]"#).unwrap();
        assert!(matches!(second.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
        services.events(r#"[{"service":"share","id":"2","outcome":{"status":"shared","service":"example/Target"}}]"#).unwrap();
        assert_eq!(second.try_recv().unwrap(), Ok(ShareOutcome::Shared { service: "example/Target".into() }));
        let pending = services.share(&payload).unwrap();
        services.dispose();
        assert_eq!(pending.try_recv().unwrap(), Ok(ShareOutcome::Dismissed));
        assert!(services.share(&payload).is_err());
    }
    #[test]
    fn dispatch_failure_and_disposal_complete_pending_handles() {
        let services = NativeServices::default();
        let speech = services.speech.start(Default::default()).unwrap();
        let permission = services.desktop.permission(true).unwrap();
        let fetch = services.fetch.send(serde_json::from_str(r#"{"url":"https://example.test"}"#).unwrap()).unwrap();
        for command in services.take_commands() { services.rejected(&command); }
        assert!(matches!(speech.events().as_slice(), [SpeechEvent::End { error: Some(SpeechError::Unavailable) }]));
        assert!(matches!(permission.reply(), Some(ServiceReply::Permission { error: Some(ServiceError::Unavailable), .. })));
        assert!(matches!(fetch.events().as_slice(), [FetchEvent::Failed { .. }]));
        services.dispose();
        assert!(services.events("[]").is_err());
        assert!(services.fetch.send(serde_json::from_str(r#"{"url":"https://example.test"}"#).unwrap()).is_err());
    }
}
