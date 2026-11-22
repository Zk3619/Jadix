//! Media Foundation decodes bounded short sounds off-thread; XAudio2 voices
//! retain their cached PCM. WinRT MediaPlayer streams longer sources.
use super::*;
use std::rc::Rc;
use jadix_core::audio::*;
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use windows::{core::{HSTRING, PCWSTR}, Foundation::{TimeSpan, TypedEventHandler, Uri}, Media::{Core::MediaSource, Playback::{MediaPlayer, MediaPlaybackState, MediaPlaybackSession}}, Storage::{StorageFile, Streams::{DataWriter, InMemoryRandomAccessStream}}, Win32::{Media::{MediaFoundation::*, Audio::{WAVEFORMATEX, AudioCategory_Media, XAudio2::*}}, System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED}, UI::Shell::SHCreateMemStream}};

type Wake = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct Callbacks { closed: bool, active: std::collections::BTreeSet<u32>, changed: std::collections::BTreeSet<u32>, overflow: std::collections::BTreeSet<u32>, events: VecDeque<(u32, AudioEvent)> }
type Queue = Arc<Mutex<Callbacks>>;
fn queue(queue: &Queue, wake: &Wake, id: u32, event: AudioEvent) {
    let mut q = queue.lock().unwrap();
    if !q.closed && q.active.contains(&id) {
        if q.events.len() < 512 { q.events.push_back((id, event)); } else { q.overflow.insert(id); }
        wake();
    }
}

fn changed(queue: &Queue, wake: &Wake, id: u32) {
    let mut q = queue.lock().unwrap();
    if !q.closed && q.active.contains(&id) { q.changed.insert(id); wake(); }
}

struct Clip { pcm: Arc<[u8]>, format: WAVEFORMATEX, duration: f64 }
enum Loaded { Clip(Clip), Stream(MediaSource) }
struct Job { id: u32, source: AudioSource, sound: bool, root: PathBuf, cancel: Arc<AtomicBool> }
struct Readers { send: Option<mpsc::SyncSender<Job>>, receive: Option<mpsc::Receiver<(u32, Result<Loaded, String>)>>, threads: Vec<std::thread::JoinHandle<()>> }
impl Readers {
    fn new(curl: PathBuf, callbacks: Queue, wake: Wake) -> Result<Self, String> {
        let (send, jobs) = mpsc::sync_channel::<Job>(MAX_SOUNDS + MAX_PLAYERS);
        let jobs = Arc::new(Mutex::new(jobs));
        let (reply, receive) = mpsc::sync_channel(2);
        let mut readers = Self { send: Some(send), receive: Some(receive), threads: Vec::new() };
        for _ in 0..2 {
            let (jobs, reply, curl, callbacks, wake) = (jobs.clone(), reply.clone(), curl.clone(), callbacks.clone(), wake.clone());
            readers.threads.push(std::thread::Builder::new().name("jadix-win-audio".into()).spawn(move || {
                let runtime = MediaRuntime::new();
                loop {
                    let Ok(job) = jobs.lock().unwrap().recv() else { break };
                    if job.cancel.load(Ordering::Acquire) { continue; }
                    let result = match &runtime {
                        Err(error) => Err(error.clone()),
                        Ok(_) if job.sound => crate::audio_source::read(job.source, &job.root, &curl, job.cancel.clone())
                            .and_then(|bytes| decode(&bytes, &job.cancel)).map(Loaded::Clip),
                        Ok(_) => stream_source(job.source, &job.root).map(Loaded::Stream),
                    };
                    if !job.cancel.load(Ordering::Acquire) {
                        if reply.send((job.id, result)).is_err() { break; }
                        let q = callbacks.lock().unwrap();
                        if !q.closed && q.active.contains(&job.id) { wake(); }
                    }
                }
            }).map_err(|e| e.to_string())?);
        }
        Ok(readers)
    }
    fn dispose(&mut self) {
        self.send.take(); self.receive.take();
        for thread in self.threads.drain(..) { let _ = thread.join(); }
    }
}
impl Drop for Readers { fn drop(&mut self) { self.dispose(); } }
struct MediaRuntime;
impl MediaRuntime {
    fn new() -> Result<Self, String> { unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| e.to_string())?;
        if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) { CoUninitialize(); return Err(e.to_string()); }
        Ok(Self)
    } }
}
impl Drop for MediaRuntime { fn drop(&mut self) { unsafe { let _ = MFShutdown(); CoUninitialize(); } } }

fn decode(bytes: &[u8], cancel: &AtomicBool) -> Result<Clip, String> {
    (|| -> windows::core::Result<Clip> { unsafe {
        let input = SHCreateMemStream(Some(bytes)).ok_or_else(windows::core::Error::from_thread)?;
        let stream = MFCreateMFByteStreamOnStream(&input)?;
        let reader = MFCreateSourceReaderFromByteStream(&stream, None)?;
        let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(audio, true)?;
        let requested = MFCreateMediaType()?;
        requested.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        requested.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        requested.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        reader.SetCurrentMediaType(audio, None, &requested)?;
        let media = reader.GetCurrentMediaType(audio)?;
        let channels = media.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?;
        let rate = media.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
        if !(1..=8).contains(&channels) || !(8000..=192000).contains(&rate) || media.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)? != 16 {
            return Err(windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), "unsupported decoded PCM format"));
        }
        let alignment = channels * 2;
        let bytes_per_second = rate * alignment;
        let limit = (f64::from(bytes_per_second) * MAX_SOUND_SECONDS) as usize;
        let mut pcm = Vec::new();
        let started = Instant::now();
        loop {
            if cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(10) {
                return Err(windows::core::Error::new(windows::core::HRESULT(0x80004004u32 as i32), "audio decode cancelled or deadline exceeded"));
            }
            let (mut flags, mut sample) = (0, None);
            reader.ReadSample(audio, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            if flags & (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0) as u32 != 0 {
                return Err(windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), "audio format changed during decode"));
            }
            if let Some(sample) = sample {
                let buffer = sample.ConvertToContiguousBuffer()?;
                let (mut pointer, mut length) = (null_mut(), 0);
                buffer.Lock(&mut pointer, None, Some(&mut length))?;
                let fits = length as usize <= limit.saturating_sub(pcm.len());
                if fits && length > 0 { pcm.extend_from_slice(std::slice::from_raw_parts(pointer, length as usize)); }
                buffer.Unlock()?;
                if !fits { return Err(windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), "short sound exceeds five seconds")); }
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 { break; }
        }
        if pcm.is_empty() || pcm.len() % alignment as usize != 0 {
            return Err(windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), "empty or incomplete decoded PCM"));
        }
        Ok(Clip { duration: pcm.len() as f64 / f64::from(bytes_per_second), pcm: pcm.into(), format: WAVEFORMATEX {
            wFormatTag: 1, nChannels: channels as u16, nSamplesPerSec: rate, nAvgBytesPerSec: bytes_per_second,
            nBlockAlign: alignment as u16, wBitsPerSample: 16, cbSize: 0,
        } })
    } })().map_err(|e| e.to_string())
}

fn stream_source(source: AudioSource, root: &Path) -> Result<MediaSource, String> {
    source.validate()?;
    let local = crate::audio_source::local_path(&source, root)?;
    (|| -> windows::core::Result<MediaSource> {
        if let Some(path) = local {
            let path = media_path(&path).map_err(|message| windows::core::Error::new(windows::core::HRESULT(0x80070057u32 as i32), message))?;
            let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))?.join()?;
            MediaSource::CreateFromStorageFile(&file)
        } else { match source {
            AudioSource::Url { url } => MediaSource::CreateFromUri(&Uri::CreateUri(&HSTRING::from(url))?),
            AudioSource::Bytes { base64 } => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(base64).expect("validated media bytes");
                let stream = InMemoryRandomAccessStream::new()?;
                let writer = DataWriter::CreateDataWriter(&stream)?;
                writer.WriteBytes(&bytes)?; writer.StoreAsync()?.join()?; writer.DetachStream()?; stream.Seek(0)?;
                MediaSource::CreateFromStream(&stream, &HSTRING::new())
            }
            _ => unreachable!(),
        } }
    })().map_err(|e| e.to_string())
}

struct Engine { engine: IXAudio2, master: IXAudio2MasteringVoice }
impl Engine {
    fn new() -> windows::core::Result<Self> { unsafe {
        let mut engine = None;
        XAudio2CreateWithVersionInfo(&mut engine, 0, XAUDIO2_DEFAULT_PROCESSOR, 0x0a000000)?;
        let engine = engine.unwrap(); let mut master = None;
        engine.CreateMasteringVoice(&mut master, 0, 0, 0, PCWSTR::null(), None, AudioCategory_Media)?;
        Ok(Self { engine, master: master.unwrap() })
    } }
    fn play(&self, id: u32, clip: &Clip, volume: f32, rate: f32) -> windows::core::Result<Voice> { unsafe {
        let mut handle = None;
        self.engine.CreateSourceVoice(&mut handle, &clip.format, 0, 4., None, None, None)?;
        let voice = Voice { id, handle: handle.unwrap(), _pcm: clip.pcm.clone() };
        voice.handle.SetVolume(volume, 0)?; voice.handle.SetFrequencyRatio(rate, 0)?;
        voice.handle.SubmitSourceBuffer(&XAUDIO2_BUFFER { Flags: XAUDIO2_END_OF_STREAM, AudioBytes: clip.pcm.len() as u32, pAudioData: clip.pcm.as_ptr(), ..Default::default() }, None)?;
        voice.handle.Start(0, 0)?;
        Ok(voice)
    } }
}
impl Drop for Engine { fn drop(&mut self) { unsafe { self.master.DestroyVoice(); self.engine.StopEngine(); } } }
struct Voice { id: u32, handle: IXAudio2SourceVoice, _pcm: Arc<[u8]> }
impl Voice { fn active(&self) -> bool { unsafe { let mut state = XAUDIO2_VOICE_STATE::default(); self.handle.GetState(&mut state, 0); state.BuffersQueued != 0 } } }
impl Drop for Voice { fn drop(&mut self) { unsafe { self.handle.DestroyVoice(); } } }

struct Player {
    handle: MediaPlayer, state_token: Option<i64>, tokens: [Option<i64>; 3], settings: PlayerSettings,
    ready: bool, resume: bool, seek: Option<f64>, state: Option<MediaPlaybackState>,
}
impl Player {
    fn new(id: u32, settings: PlayerSettings, callbacks: &Queue, wake: &Wake) -> windows::core::Result<Self> {
        let handle = MediaPlayer::new()?; handle.SetAutoPlay(false)?;
        let mut player = Self { handle, state_token: None, tokens: [None; 3], settings, ready: false, resume: false, seek: None, state: None };
        player.set(settings)?;
        let (q, w) = (callbacks.clone(), wake.clone());
        player.tokens[0] = Some(player.handle.MediaOpened(&TypedEventHandler::new(move |sender: windows::core::Ref<MediaPlayer>, _| {
            let duration = sender.as_ref().and_then(|p| p.PlaybackSession().ok()).and_then(|p| p.NaturalDuration().ok()).and_then(|t| (t.Duration > 0).then_some(t.Duration as f64 / 1e7));
            queue(&q, &w, id, AudioEvent::Load { duration }); Ok(())
        }))?);
        let (q, w) = (callbacks.clone(), wake.clone());
        player.tokens[1] = Some(player.handle.MediaEnded(&TypedEventHandler::new(move |_, _| { queue(&q, &w, id, AudioEvent::Ended); Ok(()) }))?);
        let (q, w) = (callbacks.clone(), wake.clone());
        player.tokens[2] = Some(player.handle.MediaFailed(&TypedEventHandler::new(move |_, args: windows::core::Ref<windows::Media::Playback::MediaPlayerFailedEventArgs>| {
            let message = args.as_ref().and_then(|a| a.ErrorMessage().ok()).map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Windows media playback failed".into());
            queue(&q, &w, id, AudioEvent::Failed { message: bounded_error(message) }); Ok(())
        }))?);
        let (q, w) = (callbacks.clone(), wake.clone());
        player.state_token = Some(player.handle.PlaybackSession()?.PlaybackStateChanged(&TypedEventHandler::new(move |_: windows::core::Ref<MediaPlaybackSession>, _| {
            changed(&q, &w, id); Ok(())
        }))?);
        Ok(player)
    }
    fn set(&mut self, settings: PlayerSettings) -> windows::core::Result<()> {
        self.handle.SetVolume(f64::from(settings.volume))?; self.handle.SetIsMuted(settings.muted)?;
        self.handle.SetIsLoopingEnabled(settings.looping)?; self.handle.PlaybackSession()?.SetPlaybackRate(f64::from(settings.rate))?;
        self.settings = settings; Ok(())
    }
    fn seek(&mut self, position: f64) -> windows::core::Result<()> {
        self.seek = Some(position);
        if self.ready {
            let session = self.handle.PlaybackSession()?;
            if !session.CanSeek()? { return Err(windows::core::Error::new(windows::core::HRESULT(0x80004001u32 as i32), "media source cannot seek")); }
            let duration = session.NaturalDuration()?.Duration as f64 / 1e7;
            let position = if duration > 0. { position.min(duration) } else { position };
            session.SetPosition(TimeSpan { Duration: (position * 1e7) as i64 })?;
            self.seek = None;
        }
        Ok(())
    }
}
impl Drop for Player { fn drop(&mut self) {
    if let Some(token) = self.tokens[0] { let _ = self.handle.RemoveMediaOpened(token); }
    if let Some(token) = self.tokens[1] { let _ = self.handle.RemoveMediaEnded(token); }
    if let Some(token) = self.tokens[2] { let _ = self.handle.RemoveMediaFailed(token); }
    if let Some(token) = self.state_token { if let Ok(session) = self.handle.PlaybackSession() { let _ = session.RemovePlaybackStateChanged(token); } }
    let _ = self.handle.Close();
} }
fn bounded_error(mut error: String) -> String { let mut end = error.len().min(4096); while !error.is_char_boundary(end) { end -= 1; } error.truncate(end); error }

pub(super) struct Audio {
    pub service: AudioService, root: PathBuf, readers: Readers, pending: BTreeMap<u32, Arc<AtomicBool>>,
    clips: BTreeMap<u32, Clip>, voices: Vec<Voice>, players: BTreeMap<u32, Player>, engine: Option<Engine>,
    callbacks: Queue, wake: Wake, polled: Instant,
}
impl Audio {
    pub fn new(curl: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Result<Self, String> {
        let callbacks = Queue::default(); let wake: Wake = Arc::new(wake);
        let readers = Readers::new(curl, callbacks.clone(), wake.clone())?;
        let service = AudioService::new(); let command_wake = wake.clone(); service.set_command_waker(Some(Rc::new(move || command_wake())));
        Ok(Self { service, root: std::env::current_dir().map_err(|e| e.to_string())?, readers, pending: BTreeMap::new(), clips: BTreeMap::new(), voices: Vec::new(), players: BTreeMap::new(), engine: None, callbacks, wake, polled: Instant::now() })
    }
    pub fn set_root(&mut self, root: PathBuf) { self.root = root; }
    fn cancel(&mut self, id: u32) {
        { let mut q = self.callbacks.lock().unwrap(); q.active.remove(&id); q.changed.remove(&id); q.overflow.remove(&id); q.events.retain(|(owner, _)| *owner != id); }
        if let Some(cancel) = self.pending.remove(&id) { cancel.store(true, Ordering::Release); }
        self.voices.retain(|voice| voice.id != id); self.clips.remove(&id); self.players.remove(&id);
    }
    fn fail(&mut self, id: u32, error: String) { self.cancel(id); self.service.event(id, AudioEvent::Failed { message: bounded_error(error) }); }
    fn load(&mut self, id: u32, source: AudioSource, sound: bool) -> Result<(), String> {
        self.callbacks.lock().unwrap().active.insert(id);
        let cancel = Arc::new(AtomicBool::new(false)); self.pending.insert(id, cancel.clone());
        self.readers.send.as_ref().ok_or("audio host disposed")?.try_send(Job { id, source, sound, root: self.root.clone(), cancel }).map_err(|_| "audio loader busy".into())
    }
    fn command(&mut self, command: AudioCommand) -> Result<(), String> {
        let id = command.id();
        let result: windows::core::Result<()> = match command {
            AudioCommand::Load { source, .. } => return self.load(id, source, true),
            AudioCommand::Open { source, settings, .. } => {
                match Player::new(id, settings, &self.callbacks, &self.wake) {
                    Ok(player) => { self.players.insert(id, player); return self.load(id, source, false); }
                    Err(e) => Err(e),
                }
            }
            AudioCommand::Unload { .. } | AudioCommand::Close { .. } => { self.cancel(id); return Ok(()); }
            AudioCommand::Stop { .. } => { self.voices.retain(|v| v.id != id); self.service.event(id, AudioEvent::Paused); return Ok(()); }
            AudioCommand::Play { volume, rate, .. } => {
                if self.voices.len() >= MAX_VOICES { return Err("sound voice quota exceeded".into()); }
                let clip = self.clips.get(&id).ok_or("sound is not decoded")?;
                if self.engine.is_none() { self.engine = Some(Engine::new().map_err(|e| e.to_string())?); }
                let voice = self.engine.as_ref().unwrap().play(id, clip, volume, rate).map_err(|e| e.to_string())?;
                self.voices.push(voice); self.service.event(id, AudioEvent::Playing); return Ok(());
            }
            AudioCommand::Preload { .. } => return Ok(()), // Open already prepares the source asynchronously.
            command => {
                let player = self.players.get_mut(&id).ok_or("audio player is closed")?;
                match command {
                    AudioCommand::Resume { .. } => { player.resume = true; if player.ready { player.handle.Play() } else { Ok(()) } }
                    AudioCommand::Pause { .. } => { player.resume = false; player.handle.Pause() }
                    AudioCommand::Seek { position, .. } => { let result = player.seek(position); changed(&self.callbacks, &self.wake, id); result },
                    AudioCommand::Set { settings, .. } => player.set(settings),
                    _ => unreachable!(),
                }
            }
        };
        result.map_err(|e| e.to_string())
    }
    pub fn pump(&mut self) {
        let finished: std::collections::BTreeSet<_> = self.voices.iter().filter(|v| !v.active()).map(|v| v.id).collect();
        self.voices.retain(Voice::active);
        for id in finished { if !self.voices.iter().any(|v| v.id == id) { self.service.event(id, AudioEvent::Ended); } }
        // Cleanup commands win over queued decoder/media callbacks.
        for command in self.service.take_commands() { let id = command.id(); if let Err(e) = self.command(command) { self.fail(id, e); } }
        while let Some((id, result)) = self.readers.receive.as_ref().and_then(|r| r.try_recv().ok()) {
            if self.pending.remove(&id).is_none() { continue; }
            match result {
                Err(e) => self.fail(id, e),
                Ok(Loaded::Clip(clip)) => {
                    let total = self.clips.values().map(|v| v.pcm.len() as u64).sum::<u64>();
                    if total + clip.pcm.len() as u64 > MAX_DECODED_BYTES { self.fail(id, "decoded sound quota exceeded".into()); continue; }
                    if self.service.event(id, AudioEvent::Loaded { duration: clip.duration, decoded_bytes: clip.pcm.len() as u64 }) { self.clips.insert(id, clip); }
                }
                Ok(Loaded::Stream(source)) => if let Some(player) = self.players.get(&id) { if let Err(e) = player.handle.SetSource(&source) { self.fail(id, e.to_string()); } },
            }
        }
        let (events, mut dirty, overflow) = {
            let mut q = self.callbacks.lock().unwrap();
            (std::mem::take(&mut q.events), std::mem::take(&mut q.changed), std::mem::take(&mut q.overflow))
        };
        for id in overflow { self.fail(id, "Windows audio callback queue exceeded its limit".into()); }
        for (id, event) in events {
            if !self.players.contains_key(&id) { continue; }
            if let AudioEvent::Failed { message } = event { self.fail(id, message); continue; }
            if matches!(event, AudioEvent::Ended) && self.players[&id].settings.looping { continue; }
            dirty.insert(id);
            if matches!(event, AudioEvent::Load { .. }) {
                let player = self.players.get_mut(&id).unwrap(); player.ready = true;
                let result = (|| { if let Some(position) = player.seek { player.seek(position)?; } if player.resume { player.handle.Play()?; } Ok::<_, windows::core::Error>(()) })();
                if let Err(e) = result { self.fail(id, e.to_string()); continue; }
            }
            self.service.event(id, event);
        }
        let poll = self.polled.elapsed() >= Duration::from_millis(100);
        if poll { self.polled = Instant::now(); }
        let mut errors = Vec::new();
        for (&id, player) in &mut self.players {
            if !dirty.contains(&id) && !(poll && player.state == Some(MediaPlaybackState::Playing)) { continue; }
            let result = (|| -> windows::core::Result<()> {
                let session = player.handle.PlaybackSession()?; let state = session.PlaybackState()?;
                if player.state != Some(state) {
                    self.service.event(id, AudioEvent::Buffer { buffering: state == MediaPlaybackState::Buffering || state == MediaPlaybackState::Opening });
                    if state == MediaPlaybackState::Playing { self.service.event(id, AudioEvent::Playing); }
                    else if state == MediaPlaybackState::Paused { self.service.event(id, AudioEvent::Paused); }
                    player.state = Some(state);
                }
                if player.ready {
                    let duration = session.NaturalDuration()?.Duration as f64 / 1e7;
                    let duration = (duration > 0.).then_some(duration);
                    let position = (session.Position()?.Duration as f64 / 1e7).max(0.);
                    let buffered = duration.map(|d| d * session.DownloadProgress().unwrap_or(0.).clamp(0., 1.));
                    self.service.event(id, AudioEvent::Progress { position, duration, buffered });
                }
                Ok(())
            })();
            if let Err(e) = result { errors.push((id, e.to_string())); }
        }
        for (id, error) in errors { self.fail(id, error); }
    }
    pub fn active(&self) -> bool { self.players.values().any(|player| player.state == Some(MediaPlaybackState::Playing)) || !self.voices.is_empty() }
    pub fn dispose(&mut self) {
        { let mut q = self.callbacks.lock().unwrap(); q.closed = true; q.active.clear(); q.changed.clear(); q.overflow.clear(); q.events.clear(); }
        self.service.dispose();
        for cancel in self.pending.values() { cancel.store(true, Ordering::Release); } self.pending.clear();
        self.players.clear(); self.voices.clear(); self.clips.clear(); self.engine.take(); self.readers.dispose();
    }
}
impl Drop for Audio { fn drop(&mut self) { self.dispose(); } }

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(samples: u32) -> Vec<u8> {
        let length = samples * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF"); bytes.extend_from_slice(&(36 + length).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&8000u32.to_le_bytes()); bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data"); bytes.extend_from_slice(&length.to_le_bytes());
        bytes.resize(44 + length as usize, 0); bytes
    }

    #[test]
    fn media_foundation_decodes_pcm_and_rejects_cancelled_or_oversized_sounds() {
        std::thread::spawn(|| {
            let _runtime = MediaRuntime::new().unwrap();
            let cancelled = AtomicBool::new(false);
            let clip = decode(&wav(800), &cancelled).unwrap();
            assert_eq!(clip.pcm.len(), 1600);
            assert!((clip.duration - 0.1).abs() < 1e-6);
            assert!(decode(&wav(40001), &cancelled).is_err());
            cancelled.store(true, Ordering::Release);
            assert!(decode(&wav(800), &cancelled).is_err());
        }).join().unwrap();
    }

    #[test]
    fn system_streams_load_seek_play_pause_and_sleep_until_woken() {
        std::thread::spawn(|| {
            let _runtime = MediaRuntime::new().unwrap();
            let root = std::env::temp_dir().join(format!("jadix-stream-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("中文 clip.wav");
            std::fs::write(&path, wav(16_000)).unwrap();
            let mut audio = Audio::new("curl.exe".into(), || {}).unwrap();
            audio.set_root(root.clone());
            for source in [AudioSource::File { path }, AudioSource::Url { url: "asset:/中文 clip.wav".into() }, AudioSource::bytes(&wav(16_000)).unwrap()] {
                let player = audio.service.open_player(source, PlayerSettings { muted: true, ..Default::default() }).unwrap();
                player.seek(0.4).unwrap();
                let deadline = Instant::now() + Duration::from_secs(12);
                let wait = |audio: &mut Audio, playing: Option<bool>| {
                    loop {
                        audio.pump();
                        let snapshot = player.snapshot().unwrap();
                        assert!(snapshot.error.is_none(), "{snapshot:?}");
                        if snapshot.ready && playing.is_none_or(|playing| snapshot.playing == playing) { break; }
                        assert!(Instant::now() < deadline, "{snapshot:?}");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                };
                wait(&mut audio, None);
                player.play().unwrap(); wait(&mut audio, Some(true)); assert!(audio.active());
                player.pause().unwrap(); wait(&mut audio, Some(false)); assert!(!audio.active());
                assert!((player.snapshot().unwrap().position - 0.4).abs() < 0.3);
                player.events();
                std::thread::sleep(Duration::from_millis(120)); audio.pump(); player.events();
                std::thread::sleep(Duration::from_millis(120)); audio.pump();
                assert!(player.events().is_empty(), "paused players must not generate polling events");
                drop(player); audio.pump(); assert!(audio.players.is_empty());
            }
            audio.dispose(); std::fs::remove_dir_all(root).unwrap();
        }).join().unwrap();
    }

    #[test]
    fn asynchronous_decode_cancellation_and_disposal_release_resources() {
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = wakes.clone();
        let mut audio = Audio::new("curl.exe".into(), move || { count.fetch_add(1, Ordering::Relaxed); }).unwrap();
        let sound = audio.service.load_sound(AudioSource::bytes(&wav(800)).unwrap()).unwrap();
        let cancelled = audio.service.load_sound(AudioSource::bytes(&wav(801)).unwrap()).unwrap();
        audio.pump();
        drop(cancelled);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !sound.snapshot().unwrap().ready {
            audio.pump();
            assert!(sound.snapshot().unwrap().error.is_none(), "{:?}", sound.snapshot());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(audio.clips.len(), 1);
        let callbacks = audio.callbacks.clone(); let wake = audio.wake.clone(); let id = sound.id();
        audio.dispose();
        let before = wakes.load(Ordering::Relaxed);
        queue(&callbacks, &wake, id, AudioEvent::Playing);
        assert_eq!(before, wakes.load(Ordering::Relaxed));
        assert!(sound.snapshot().is_err());
        assert!(audio.readers.threads.is_empty());
    }
}
