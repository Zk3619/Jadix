//! Root-scoped dictation sessions. Replacement and abort invalidate old callbacks.
use std::{cell::RefCell, collections::{BTreeMap, VecDeque}, rc::Rc};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeechRequest { pub lang: Option<String>, #[serde(default = "yes")] pub interim: bool }
fn yes() -> bool { true }
impl Default for SpeechRequest { fn default() -> Self { Self { lang: None, interim: true } } }
impl SpeechRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.lang.as_ref().is_some_and(|v| v.is_empty() || v.len() > 128 || v.split('-').any(|s| s.is_empty() || s.len() > 8 || !s.bytes().all(|c| c.is_ascii_alphanumeric()))) { return Err("invalid speech language".into()); } Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechError { NotAllowed, NoSpeech, AudioCapture, Network, LanguageNotSupported, Unavailable, Failed }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpeechEvent { Start, Result { text: String, is_final: bool }, End { error: Option<SpeechError> } }
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SpeechCommand { Start { id: u32, request: SpeechRequest }, Stop { id: u32 }, Abort { id: u32 } }
struct Entry { interim: bool, dispatched: bool, started: bool, ended: bool, stopping: bool, events: VecDeque<SpeechEvent> }
#[derive(Default)]
struct State { next: u32, active: Option<u32>, entries: BTreeMap<u32, Entry>, commands: VecDeque<SpeechCommand>, disposed: bool }
#[derive(Clone, Default)]
pub struct SpeechService(Rc<RefCell<State>>, Rc<RefCell<Option<Rc<dyn Fn()>>>>);
struct Lease { id: u32, service: SpeechService }
#[derive(Clone)]
pub struct SpeechSession(Rc<Lease>);
fn abort(state: &mut State, id: u32) {
    let Some(entry) = state.entries.get_mut(&id) else { return };
    if entry.ended { return; }
    entry.ended = true; if entry.events.len() >= 64 { entry.events.remove(usize::from(matches!(entry.events.front(), Some(SpeechEvent::Start)))); } entry.events.push_back(SpeechEvent::End { error: None });
    let dispatched = entry.dispatched;
    state.commands.retain(|c| match c { SpeechCommand::Start { id: other, .. } | SpeechCommand::Stop { id: other } | SpeechCommand::Abort { id: other } => *other != id });
    if dispatched { state.commands.push_back(SpeechCommand::Abort { id }); }
    if state.active == Some(id) { state.active = None; }
}
impl SpeechService {
    /// The host callback schedules a later turn; it must not reenter the app.
    pub fn set_command_waker(&self, wake: Option<Rc<dyn Fn()>>) { *self.1.borrow_mut() = wake; }
    fn wake_commands(&self) { let wake = self.1.borrow().clone(); if let Some(wake) = wake { wake(); } }
    pub fn new() -> Self { Self::default() }
    pub fn start(&self, request: SpeechRequest) -> Result<SpeechSession, String> {
        request.validate()?; let mut state = self.0.borrow_mut();
        if state.disposed { return Err("speech service disposed".into()); }
        if state.entries.len() >= 64 { return Err("speech session quota exceeded".into()); }
        if let Some(id) = state.active { abort(&mut state, id); }
        let id = state.next.checked_add(1).ok_or("speech identities exhausted")?; state.next = id;
        state.entries.insert(id, Entry { interim: request.interim, dispatched: false, started: false, ended: false, stopping: false, events: VecDeque::new() });
        state.active = Some(id); state.commands.push_back(SpeechCommand::Start { id, request });
        drop(state); self.wake_commands();
        Ok(SpeechSession(Rc::new(Lease { id, service: self.clone() })))
    }
    pub fn take_commands(&self) -> Vec<SpeechCommand> {
        let mut state = self.0.borrow_mut(); let commands: Vec<_> = state.commands.drain(..).collect();
        for c in &commands { if let SpeechCommand::Start { id, .. } = c { if let Some(e) = state.entries.get_mut(id) { e.dispatched = true; } } }; commands
    }
    pub fn event(&self, id: u32, event: SpeechEvent) -> bool {
        let mut state = self.0.borrow_mut(); if state.disposed || state.active != Some(id) { return false; }
        let Some(entry) = state.entries.get_mut(&id) else { return false }; if entry.ended { return false; }
        match &event {
            SpeechEvent::Start => { if entry.started { return false }; entry.started = true; }
            SpeechEvent::Result { text, is_final } => {
                if !entry.started || text.len() > 64 * 1024 || (!entry.interim && !is_final) { return false; }
                if matches!(entry.events.back(), Some(SpeechEvent::Result { is_final: false, .. })) { entry.events.pop_back(); }
            }
            SpeechEvent::End { .. } => {}
        }
        if entry.events.len() >= 64 {
            // A final transcript cannot be discarded as if delivery succeeded.
            // Reserve room for an explicit failure and release the platform input.
            entry.events.truncate(63);
            abort(&mut state, id);
            *state.entries.get_mut(&id).unwrap().events.back_mut().unwrap() = SpeechEvent::End { error: Some(SpeechError::Failed) };
            drop(state); self.wake_commands(); return true;
        }
        let ended = matches!(event, SpeechEvent::End { .. }); entry.ended = ended;
        entry.events.push_back(event); if ended { state.active = None; } true
    }
    pub fn dispose(&self) { self.set_command_waker(None); let mut state = self.0.borrow_mut(); state.disposed = true; state.active = None; state.entries.clear(); state.commands.clear(); }
}
impl SpeechSession {
    pub fn id(&self) -> u32 { self.0.id }
    pub fn stop(&self) -> Result<(), String> {
        let mut state = self.0.service.0.borrow_mut(); let entry = state.entries.get_mut(&self.id()).ok_or("speech session disposed")?;
        if !entry.ended && !entry.stopping { entry.stopping = true; state.commands.push_back(SpeechCommand::Stop { id: self.id() }); }
        drop(state); self.0.service.wake_commands(); Ok(())
    }
    pub fn abort(&self) { abort(&mut self.0.service.0.borrow_mut(), self.id()); self.0.service.wake_commands(); }
    pub fn events(&self) -> Vec<SpeechEvent> { self.0.service.0.borrow_mut().entries.get_mut(&self.id()).map(|e| e.events.drain(..).collect()).unwrap_or_default() }
}
impl Drop for Lease { fn drop(&mut self) { let mut state = self.service.0.borrow_mut(); abort(&mut state, self.id); state.entries.remove(&self.id); drop(state); self.service.wake_commands(); } }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_ends_previous_session_and_ignores_late_callbacks() {
        let service = SpeechService::new(); let first = service.start(Default::default()).unwrap(); service.take_commands();
        service.event(first.id(), SpeechEvent::Start);
        let second = service.start(Default::default()).unwrap();
        assert_eq!(first.events(), vec![SpeechEvent::Start, SpeechEvent::End { error: None }]);
        assert!(!service.event(first.id(), SpeechEvent::Result { text: "late".into(), is_final: true }));
        service.event(second.id(), SpeechEvent::Start); second.stop().unwrap(); second.stop().unwrap();
        assert_eq!(service.take_commands().len(), 3);
        second.abort(); assert!(!service.event(second.id(), SpeechEvent::Start));
    }
    #[test]
    fn cancellation_and_interim_queues_are_bounded() {
        let service = SpeechService::new(); for _ in 0..1000 { drop(service.start(Default::default()).unwrap()); }
        assert!(service.take_commands().is_empty());
        let session = service.start(Default::default()).unwrap(); service.event(session.id(), SpeechEvent::Start);
        for n in 0..1000 { service.event(session.id(), SpeechEvent::Result { text: n.to_string(), is_final: false }); }
        assert_eq!(session.events().len(), 2);
    }
    #[test]
    fn final_result_overflow_reports_failure_and_cancels_platform_once() {
        let service = SpeechService::new(); let session = service.start(Default::default()).unwrap(); service.take_commands();
        service.event(session.id(), SpeechEvent::Start);
        for n in 0..1000 { service.event(session.id(), SpeechEvent::Result { text: n.to_string(), is_final: true }); }
        let events = session.events(); assert_eq!(events.len(), 64);
        assert_eq!(events.first(), Some(&SpeechEvent::Start));
        assert_eq!(events.last(), Some(&SpeechEvent::End { error: Some(SpeechError::Failed) }));
        assert!(matches!(service.take_commands().as_slice(), [SpeechCommand::Abort { id }] if *id == session.id()));
        assert!(!service.event(session.id(), SpeechEvent::End { error: None }));
        session.abort(); session.stop().unwrap(); assert!(service.take_commands().is_empty());
        assert!(service.start(Default::default()).is_ok());
    }
}
