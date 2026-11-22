//! Bounded audio ownership and commands shared by native and script hosts.
//! Loading is asynchronous. A sound voice only plays an already decoded buffer.
use std::{cell::RefCell, collections::{BTreeMap, VecDeque}, rc::Rc};
use serde::{Deserialize, Serialize};
use base64::Engine;

pub const MAX_SOUND_BYTES: usize = 8 << 20;
pub const MAX_SOUND_SECONDS: f64 = 5.;
pub const MAX_SOUNDS: usize = 64;
pub const MAX_PLAYERS: usize = 64;
pub const MAX_DECODED_BYTES: u64 = 16 << 20;
pub const MAX_VOICES: usize = 32;
const MAX_PENDING: usize = 256;
const MAX_ENCODED_TOTAL: usize = 32 << 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioSource {
    Url { url: String },
    /// Trusted native paths. Script hosts must replace granted handles first.
    File { path: std::path::PathBuf },
    Bytes { base64: String },
}
impl AudioSource {
    pub fn bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_SOUND_BYTES { return Err("sound encoded quota exceeded".into()); }
        Ok(Self::Bytes { base64: base64::engine::general_purpose::STANDARD.encode(bytes) })
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Url { url } => {
                if crate::web_resource::bundle_path(url).is_some() { return Ok(()); }
                if url.bytes().any(|c| c <= 32 || c == 127 || c == b'\\') { return Err("invalid media URL".into()); }
                let parsed = url::Url::parse(url).map_err(|_| "invalid media URL")?;
                if url.len() > 16 * 1024 || parsed.username() != "" || parsed.password().is_some()
                    || parsed.scheme() != "https" || parsed.host_str().is_none() {
                    return Err("media URL is not allowed".into());
                }
            }
            Self::File { path } => if !path.is_absolute() || path.as_os_str().len() > 32768 { return Err("invalid media path".into()); },
            Self::Bytes { base64 } => {
                if base64.len() > MAX_SOUND_BYTES.div_ceil(3) * 4 { return Err("sound encoded quota exceeded".into()); }
                let bytes = base64::engine::general_purpose::STANDARD.decode(base64).map_err(|_| "invalid media bytes")?;
                if bytes.is_empty() || bytes.len() > MAX_SOUND_BYTES { return Err("sound encoded quota exceeded".into()); }
            }
        }
        Ok(())
    }
    fn encoded_size(&self) -> usize { if let Self::Bytes { base64 } = self { base64.len() } else { 0 } }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSettings { pub volume: f32, pub rate: f32, pub muted: bool, pub looping: bool }
impl Default for PlayerSettings { fn default() -> Self { Self { volume: 1., rate: 1., muted: false, looping: false } } }
impl PlayerSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) || !self.rate.is_finite() || !(0.25..=4.).contains(&self.rate) { Err("invalid audio settings".into()) } else { Ok(()) }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AudioCommand {
    Load { id: u32, source: AudioSource }, Play { id: u32, volume: f32, rate: f32 }, Stop { id: u32 }, Unload { id: u32 },
    Open { id: u32, source: AudioSource, settings: PlayerSettings }, Preload { id: u32 }, Resume { id: u32 }, Pause { id: u32 },
    Seek { id: u32, position: f64 }, Set { id: u32, settings: PlayerSettings }, Close { id: u32 },
}
impl AudioCommand {
    pub fn id(&self) -> u32 { match self {
        Self::Load { id, .. } | Self::Play { id, .. } | Self::Stop { id } | Self::Unload { id } | Self::Open { id, .. } | Self::Preload { id } | Self::Resume { id } | Self::Pause { id } | Self::Seek { id, .. } | Self::Set { id, .. } | Self::Close { id } => *id,
    } }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioEvent {
    Loaded { duration: f64, decoded_bytes: u64 }, Load { duration: Option<f64> },
    Progress { position: f64, duration: Option<f64>, buffered: Option<f64> },
    Playing, Paused, Buffer { buffering: bool }, Ended, Failed { message: String },
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AudioSnapshot { pub ready: bool, pub playing: bool, pub buffering: bool, pub ended: bool,
    pub position: f64, pub duration: Option<f64>, pub buffered: Option<f64>, pub error: Option<String> }
struct Entry { dispatched: bool, source: AudioSource, sound: bool, references: usize, decoded: u64, snapshot: AudioSnapshot, events: VecDeque<AudioEvent> }
#[derive(Default)]
struct State { next: u32, entries: BTreeMap<u32, Entry>, commands: VecDeque<AudioCommand>, disposed: bool }
#[derive(Clone, Default)]
pub struct AudioService(Rc<RefCell<State>>, Rc<RefCell<Option<Rc<dyn Fn()>>>>);
struct Lease { id: u32, service: AudioService }
#[derive(Clone)]
pub struct Sound(Rc<Lease>);
#[derive(Clone)]
pub struct AudioPlayer(Rc<Lease>);
impl AudioService {
    /// Schedule a host turn when commands become available. The callback must
    /// enqueue work, not synchronously enter application code.
    pub fn set_command_waker(&self, wake: Option<Rc<dyn Fn()>>) { *self.1.borrow_mut() = wake; }
    fn wake_commands(&self) { let wake = self.1.borrow().clone(); if let Some(wake) = wake { wake(); } }
    pub fn new() -> Self { Self::default() }
    pub fn load_sound(&self, source: AudioSource) -> Result<Sound, String> { self.open(source, true, PlayerSettings::default()).map(|v| Sound(Rc::new(v))) }
    pub fn open_player(&self, source: AudioSource, settings: PlayerSettings) -> Result<AudioPlayer, String> { self.open(source, false, settings).map(|v| AudioPlayer(Rc::new(v))) }
    fn open(&self, source: AudioSource, sound: bool, settings: PlayerSettings) -> Result<Lease, String> {
        source.validate()?; settings.validate()?;
        let mut state = self.0.borrow_mut();
        if state.disposed { return Err("audio service disposed".into()); }
        if sound {
            if let Some((&id, entry)) = state.entries.iter_mut().find(|(_, e)| e.sound && e.source == source) {
                entry.references += 1; return Ok(Lease { id, service: self.clone() });
            }
        }
        if state.entries.values().filter(|e| e.sound == sound).count() >= if sound { MAX_SOUNDS } else { MAX_PLAYERS }
            || state.commands.len() >= MAX_PENDING
            || state.entries.values().map(|e| e.source.encoded_size()).sum::<usize>() + source.encoded_size() > MAX_ENCODED_TOTAL { return Err("audio resource quota exceeded".into()); }
        let id = state.next.checked_add(1).ok_or("audio identifiers exhausted")?; state.next = id;
        state.commands.push_back(if sound { AudioCommand::Load { id, source: source.clone() } } else { AudioCommand::Open { id, source: source.clone(), settings } });
        state.entries.insert(id, Entry { dispatched: false, source, sound, references: 1, decoded: 0, snapshot: AudioSnapshot::default(), events: VecDeque::new() });
        drop(state); self.wake_commands();
        Ok(Lease { id, service: self.clone() })
    }
    /// Drain on the platform's UI thread; decoders and network fetches run elsewhere.
    pub fn take_commands(&self) -> Vec<AudioCommand> {
        let mut state = self.0.borrow_mut();
        let commands: Vec<_> = state.commands.drain(..).collect();
        for command in &commands {
            if matches!(command, AudioCommand::Load { .. } | AudioCommand::Open { .. }) {
                if let Some(entry) = state.entries.get_mut(&command.id()) { entry.dispatched = true; }
            }
        }
        commands
    }
    /// Reject stale object ids, malformed timelines and decoded-budget violations.
    pub fn event(&self, id: u32, mut event: AudioEvent) -> bool {
        let finite = |v: f64| v.is_finite() && v >= 0. && v <= 1e9;
        let valid = match &event {
            AudioEvent::Loaded { duration, decoded_bytes } => finite(*duration) && *duration <= MAX_SOUND_SECONDS && *decoded_bytes > 0,
            AudioEvent::Load { duration } => duration.is_none_or(finite),
            AudioEvent::Progress { position, duration, buffered } => finite(*position) && duration.is_none_or(finite) && buffered.is_none_or(finite),
            AudioEvent::Failed { message } => message.len() <= 4096,
            _ => true,
        };
        let mut state = self.0.borrow_mut();
        if state.disposed || !state.entries.contains_key(&id) { return false; }
        let total = state.entries.values().map(|e| e.decoded).sum::<u64>();
        let entry = state.entries.get(&id).unwrap();
        if entry.snapshot.error.is_some() { return false; }
        let wrong_family = entry.sound && !matches!(event, AudioEvent::Loaded { .. } | AudioEvent::Playing | AudioEvent::Paused | AudioEvent::Ended | AudioEvent::Failed { .. });
        let sound = entry.sound;
        let overflow = if let AudioEvent::Loaded { decoded_bytes, .. } = &event { !entry.sound || total.saturating_sub(entry.decoded).saturating_add(*decoded_bytes) > MAX_DECODED_BYTES } else { false };
        let cancelled = !valid || overflow || wrong_family;
        if cancelled {
            state.commands.retain(|command| command.id() != id);
            state.commands.push_back(if sound { AudioCommand::Unload { id } } else { AudioCommand::Close { id } });
            event = AudioEvent::Failed { message: "audio decoder quota or timeline violation".into() };
        }
        let entry = state.entries.get_mut(&id).unwrap();
        if entry.snapshot.error.is_some() { return false; }
        match &event {
            AudioEvent::Loaded { duration, decoded_bytes } => { entry.decoded = *decoded_bytes; entry.snapshot.ready = true; entry.snapshot.duration = Some(*duration); }
            AudioEvent::Load { duration } => { entry.snapshot.ready = true; entry.snapshot.duration = *duration; }
            AudioEvent::Progress { position, duration, buffered } => { entry.snapshot.position = *position; entry.snapshot.duration = *duration; entry.snapshot.buffered = *buffered; }
            AudioEvent::Playing => { entry.snapshot.playing = true; entry.snapshot.ended = false; }
            AudioEvent::Paused => entry.snapshot.playing = false,
            AudioEvent::Buffer { buffering } => entry.snapshot.buffering = *buffering,
            AudioEvent::Ended => { entry.snapshot.playing = false; entry.snapshot.ended = true; }
            AudioEvent::Failed { message } => { entry.decoded = 0; entry.snapshot.ready = false; entry.snapshot.playing = false; entry.snapshot.error = Some(message.clone()); }
        }
        // Coalesce progress without dropping terminal state or unbounding memory.
        if matches!(event, AudioEvent::Progress { .. }) && matches!(entry.events.back(), Some(AudioEvent::Progress { .. })) { entry.events.pop_back(); }
        if entry.events.len() == 64 { entry.events.pop_front(); }
        entry.events.push_back(event);
        drop(state); if cancelled { self.wake_commands(); } true
    }
    pub fn dispose(&self) { self.set_command_waker(None); let mut state = self.0.borrow_mut(); state.disposed = true; state.commands.clear(); state.entries.clear(); }
}
impl Lease {
    fn command(&self, command: AudioCommand, require_ready: bool) -> Result<(), String> {
        let mut state = self.service.0.borrow_mut();
        let entry = state.entries.get(&self.id).ok_or("audio object disposed")?;
        if let Some(error) = &entry.snapshot.error { return Err(error.clone()); }
        if require_ready && !entry.snapshot.ready { return Err("sound is not decoded yet".into()); }
        if state.commands.len() >= MAX_PENDING { return Err("audio command quota exceeded".into()); }
        state.commands.push_back(command); drop(state); self.service.wake_commands(); Ok(())
    }
    fn snapshot(&self) -> Result<AudioSnapshot, String> { self.service.0.borrow().entries.get(&self.id).map(|e| e.snapshot.clone()).ok_or("audio object disposed".into()) }
    fn events(&self) -> Vec<AudioEvent> { self.service.0.borrow_mut().entries.get_mut(&self.id).map(|e| e.events.drain(..).collect()).unwrap_or_default() }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.service.0.borrow_mut();
        let Some(entry) = state.entries.get_mut(&self.id) else { return }; entry.references -= 1;
        if entry.references != 0 { return; }
        let sound = entry.sound; let dispatched = entry.dispatched; state.entries.remove(&self.id);
        // Disposal must always reach the platform, even when app commands filled the queue.
        state.commands.retain(|command| command.id() != self.id);
        if dispatched { state.commands.push_back(if sound { AudioCommand::Unload { id: self.id } } else { AudioCommand::Close { id: self.id } }); }
        drop(state); self.service.wake_commands();
    }
}
impl Sound {
    pub fn id(&self) -> u32 { self.0.id }
    pub fn snapshot(&self) -> Result<AudioSnapshot, String> { self.0.snapshot() }
    pub fn events(&self) -> Vec<AudioEvent> { self.0.events() }
    pub fn play(&self, volume: f32, rate: f32) -> Result<(), String> {
        PlayerSettings { volume, rate, ..Default::default() }.validate()?;
        self.0.command(AudioCommand::Play { id: self.id(), volume, rate }, true)
    }
    pub fn stop(&self) -> Result<(), String> { self.0.command(AudioCommand::Stop { id: self.id() }, false) }
}
impl AudioPlayer {
    pub fn id(&self) -> u32 { self.0.id }
    pub fn snapshot(&self) -> Result<AudioSnapshot, String> { self.0.snapshot() }
    pub fn events(&self) -> Vec<AudioEvent> { self.0.events() }
    pub fn preload(&self) -> Result<(), String> { self.0.command(AudioCommand::Preload { id: self.id() }, false) }
    pub fn play(&self) -> Result<(), String> { self.0.command(AudioCommand::Resume { id: self.id() }, false) }
    pub fn pause(&self) -> Result<(), String> { self.0.command(AudioCommand::Pause { id: self.id() }, false) }
    pub fn seek(&self, position: f64) -> Result<(), String> {
        if !position.is_finite() || !(0.0..=1e9).contains(&position) { return Err("invalid seek position".into()); }
        self.0.command(AudioCommand::Seek { id: self.id(), position }, false)
    }
    pub fn set(&self, settings: PlayerSettings) -> Result<(), String> { settings.validate()?; self.0.command(AudioCommand::Set { id: self.id(), settings }, false) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_bundle_urls_share_decoding_and_containment_rules() {
        for url in ["asset:/中文 clip.wav", "asset://bundle/中文%20clip.wav", "app://bundle/中文 clip.wav"] {
            assert!(AudioSource::Url { url: url.into() }.validate().is_ok(), "{url}");
            assert_eq!(crate::web_resource::bundle_path(url).unwrap(), std::path::PathBuf::from("中文 clip.wav"));
        }
        for url in ["asset:/%2e%2e/a.wav", "app://bundle/a/%2f../b.wav", "asset:/a%00.wav", "asset:/a%0a.wav", "asset:/a%5cb.wav", "asset:/a%q1.wav", "asset:/a%.wav", "asset:/C:/a.wav", "asset://remote/a.wav", "app://remote/a.wav"] {
            assert!(AudioSource::Url { url: url.into() }.validate().is_err(), "{url}");
        }
    }
    #[test]
    fn deduplicated_sound_requires_decode_and_last_owner_unloads() {
        let service = AudioService::new(); let source = AudioSource::bytes(b"RIFF").unwrap();
        let a = service.load_sound(source.clone()).unwrap(); let b = service.load_sound(source).unwrap();
        assert_eq!(a.id(), b.id()); assert_eq!(service.take_commands().len(), 1); assert!(a.play(1., 1.).is_err());
        assert!(service.event(a.id(), AudioEvent::Loaded { duration: 1., decoded_bytes: 8 }));
        a.play(1., 1.).unwrap(); service.take_commands(); drop(a); assert!(service.take_commands().is_empty());
        let id = b.id(); drop(b); assert!(matches!(service.take_commands().as_slice(), [AudioCommand::Unload { .. }]));
        assert!(!service.event(id, AudioEvent::Playing));
    }
    #[test]
    fn cancelled_undispatched_loads_do_not_fill_the_queue() {
        let service = AudioService::new();
        for _ in 0..1000 { drop(service.load_sound(AudioSource::bytes(b"x").unwrap()).unwrap()); }
        assert!(service.take_commands().is_empty());
        assert!(AudioSource::Url { url: "app://bundle/a/%2e%2e/b".into() }.validate().is_err());
    }
    #[test]
    fn decoder_limits_and_disposal_fail_closed() {
        let service = AudioService::new(); let sound = service.load_sound(AudioSource::bytes(b"x").unwrap()).unwrap();
        service.take_commands(); service.event(sound.id(), AudioEvent::Loaded { duration: 5., decoded_bytes: MAX_DECODED_BYTES + 1 });
        assert!(sound.snapshot().unwrap().error.is_some()); assert!(sound.play(1., 1.).is_err());
        assert!(matches!(service.take_commands().as_slice(), [AudioCommand::Unload { .. }]));
        service.dispose(); assert!(sound.snapshot().is_err()); assert!(service.take_commands().is_empty());
    }
}
