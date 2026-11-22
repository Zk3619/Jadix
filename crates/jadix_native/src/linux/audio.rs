//! Root-owned audio. Two bounded readers feed asynchronous GStreamer decoders;
//! short-sound playback never fetches or decodes the source again.
use super::*;
use base64::Engine;
use gstreamer::{self as gst, prelude::*};
use jadix_core::audio::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const PCM_RATE: i32 = 48_000;
const PCM_BYTES: usize = PCM_RATE as usize * 2 * 2;
const CLIP_BYTES: usize = PCM_BYTES * 5;
struct ReadJob {
    id: u32,
    source: AudioSource,
    root: std::path::PathBuf,
    cancel: Arc<AtomicBool>,
}
struct Readers {
    sender: Option<mpsc::SyncSender<ReadJob>>,
    receiver: Option<mpsc::Receiver<(u32, Result<Vec<u8>, String>)>>,
    workers: Vec<std::thread::JoinHandle<()>>,
}
impl Readers {
    fn new(wake: WakeHandle) -> Result<Self, String> {
        let (sender, jobs) = mpsc::sync_channel::<ReadJob>(MAX_SOUNDS);
        let jobs = Arc::new(Mutex::new(jobs));
        let (replies, receiver) = mpsc::sync_channel(2);
        let mut workers = Vec::new();
        for _ in 0..2 {
            let (jobs, replies, wake) = (jobs.clone(), replies.clone(), wake.clone());
            workers.push(
                std::thread::Builder::new()
                    .name("jadix-audio-read".into())
                    .spawn(move || loop {
                        let job = { jobs.lock().unwrap().recv() };
                        let Ok(job) = job else {
                            break;
                        };
                        if job.cancel.load(Ordering::Acquire) {
                            continue;
                        }
                        let result = read_source(job.source, &job.root, job.cancel.clone());
                        if !job.cancel.load(Ordering::Acquire) {
                            if replies.send((job.id, result)).is_err() {
                                break;
                            }
                            wake.wake();
                        }
                    })
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(Self {
            sender: Some(sender),
            receiver: Some(receiver),
            workers,
        })
    }
    fn dispose(&mut self) {
        self.sender.take();
        self.receiver.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
impl Drop for Readers {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn read_source(
    source: AudioSource,
    root: &std::path::Path,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<u8>, String> {
    crate::audio_source::read(source, root, std::path::Path::new("/usr/bin/curl"), cancel)
}

struct Decoder {
    pipeline: gst::Pipeline,
    pcm: Arc<Mutex<Vec<u8>>>,
    started: Instant,
}
impl Decoder {
    fn new(bytes: Vec<u8>) -> Result<Self, String> {
        let sink = gstreamer_app::AppSink::builder()
            .sync(false)
            .max_buffers(2)
            .wait_on_eos(false)
            .build();
        sink.set_caps(Some(&raw_caps(PCM_RATE)));
        let pcm = Arc::new(Mutex::new(Vec::new()));
        let output = pcm.clone();
        sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let bytes = sample
                        .buffer()
                        .ok_or(gst::FlowError::Error)?
                        .map_readable()
                        .map_err(|_| gst::FlowError::Error)?;
                    let mut output = output.lock().map_err(|_| gst::FlowError::Error)?;
                    if output.len().saturating_add(bytes.len()) > CLIP_BYTES {
                        return Err(gst::FlowError::Error);
                    }
                    output.extend_from_slice(&bytes);
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        let pipeline = decoded_pipeline(Input::Bytes(bytes), &sink.upcast())?;
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pipeline,
            pcm,
            started: Instant::now(),
        })
    }
    fn poll(&self) -> Option<Result<Arc<[u8]>, String>> {
        if self.started.elapsed() > Duration::from_secs(10) {
            return Some(Err("audio decode deadline exceeded".into()));
        }
        for _ in 0..128 {
            let message = self.pipeline.bus()?.pop()?;
            match message.view() {
                gst::MessageView::Error(error) => return Some(Err(error.error().to_string())),
                gst::MessageView::Eos(_) => {
                    let bytes = std::mem::take(&mut *self.pcm.lock().unwrap());
                    return Some(if bytes.is_empty() {
                        Err("audio source has no samples".into())
                    } else {
                        Ok(bytes.into())
                    });
                }
                _ => (),
            }
        }
        None
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

enum Input {
    Uri(String),
    Bytes(Vec<u8>),
}
fn element(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory)
        .build()
        .map_err(|e| e.to_string())
}
fn raw_caps(rate: i32) -> gst::Caps {
    gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("layout", "interleaved")
        .field("rate", rate)
        .field("channels", 2i32)
        .build()
}
fn decoded_pipeline(input: Input, sink: &gst::Element) -> Result<gst::Pipeline, String> {
    let pipeline = gst::Pipeline::new();
    let convert = element("audioconvert")?;
    let resample = element("audioresample")?;
    pipeline
        .add_many([&convert, &resample, sink])
        .map_err(|e| e.to_string())?;
    gst::Element::link_many([&convert, &resample, sink]).map_err(|e| e.to_string())?;
    let decode = match input {
        Input::Uri(uri) => {
            let decode = element("uridecodebin")?;
            decode.set_property("uri", uri);
            pipeline.add(&decode).map_err(|e| e.to_string())?;
            decode
        }
        Input::Bytes(bytes) => {
            let source = element("giostreamsrc")?;
            let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
            source.set_property("stream", stream);
            let decode = element("decodebin")?;
            pipeline
                .add_many([&source, &decode])
                .map_err(|e| e.to_string())?;
            source.link(&decode).map_err(|e| e.to_string())?;
            decode
        }
    };
    let weak = convert.downgrade();
    decode.connect_pad_added(move |_, pad| {
        if !pad.current_caps().is_some_and(|c| {
            c.structure(0)
                .is_some_and(|s| s.name().starts_with("audio/"))
        }) {
            return;
        }
        if let Some(convert) = weak.upgrade() {
            if let Some(sink) = convert.static_pad("sink") {
                if !sink.is_linked() {
                    let _ = pad.link(&sink);
                }
            }
        }
    });
    Ok(pipeline)
}
struct Player {
    pipeline: gst::Pipeline,
    volume: gst::Element,
    settings: PlayerSettings,
    playing: bool,
    pending_seek: Option<f64>,
    last_progress: Instant,
}
impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
impl Player {
    fn new(input: Input, settings: PlayerSettings) -> Result<Self, String> {
        let volume = element("volume")?;
        let sink = element("autoaudiosink")?;
        let output = gst::Bin::new();
        output
            .add_many([&volume, &sink])
            .map_err(|e| e.to_string())?;
        volume.link(&sink).map_err(|e| e.to_string())?;
        let pad = gst::GhostPad::with_target(
            &volume
                .static_pad("sink")
                .ok_or("audio volume has no sink")?,
        )
        .map_err(|e| e.to_string())?;
        output.add_pad(&pad).map_err(|e| e.to_string())?;
        let pipeline = decoded_pipeline(input, &output.upcast())?;
        let this = Self {
            pipeline,
            volume,
            settings,
            playing: false,
            pending_seek: (settings.rate != 1.).then_some(0.),
            last_progress: Instant::now(),
        };
        this.volume.set_property("volume", settings.volume as f64);
        this.volume.set_property("mute", settings.muted);
        Ok(this)
    }
    fn seek(&self, position: f64) -> Result<(), String> {
        self.pipeline
            .seek(
                self.settings.rate as f64,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::from_nseconds((position * 1e9) as u64),
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
            .map_err(|e| e.to_string())
    }
}
struct Voice {
    owner: u32,
    pipeline: gst::Pipeline,
}
impl Drop for Voice {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
fn voice(owner: u32, pcm: Arc<[u8]>, volume: f32, rate: f32) -> Result<Voice, String> {
    let pipeline = gst::Pipeline::new();
    let source = gstreamer_app::AppSrc::builder()
        .format(gst::Format::Time)
        .build();
    source.set_caps(Some(&raw_caps((PCM_RATE as f32 * rate).round() as i32)));
    let convert = element("audioconvert")?;
    let resample = element("audioresample")?;
    let gain = element("volume")?;
    gain.set_property("volume", volume as f64);
    let sink = element("autoaudiosink")?;
    pipeline
        .add_many([source.upcast_ref(), &convert, &resample, &gain, &sink])
        .map_err(|e| e.to_string())?;
    gst::Element::link_many([source.upcast_ref(), &convert, &resample, &gain, &sink])
        .map_err(|e| e.to_string())?;
    let mut buffer = gst::Buffer::from_slice(pcm);
    {
        let buffer = buffer.get_mut().unwrap();
        buffer.set_pts(gst::ClockTime::ZERO);
    }
    source.push_buffer(buffer).map_err(|e| e.to_string())?;
    source.end_of_stream().map_err(|e| e.to_string())?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| e.to_string())?;
    Ok(Voice { owner, pipeline })
}

pub(super) struct Runtime {
    pub service: AudioService,
    readers: Readers,
    pending: BTreeMap<u32, Arc<AtomicBool>>,
    decoding: BTreeMap<u32, Decoder>,
    sounds: BTreeMap<u32, Arc<[u8]>>,
    players: BTreeMap<u32, Player>,
    voices: Vec<Voice>,
    timer: Rc<RefCell<Option<glib::SourceId>>>,
    wake: WakeHandle,
}
impl Runtime {
    pub fn new(wake: WakeHandle) -> Result<Self, String> {
        gst::init().map_err(|e| e.to_string())?;
        let service = AudioService::new();
        let signal = wake.clone();
        service.set_command_waker(Some(Rc::new(move || signal.wake())));
        Ok(Self {
            service,
            readers: Readers::new(wake.clone())?,
            pending: Default::default(),
            decoding: Default::default(),
            sounds: Default::default(),
            players: Default::default(),
            voices: Vec::new(),
            timer: Default::default(),
            wake,
        })
    }
    fn event(&self, id: u32, event: AudioEvent) {
        if self.service.event(id, event) {
            self.wake.wake();
        }
    }
    fn failed(&self, id: u32, error: String) {
        self.event(
            id,
            AudioEvent::Failed {
                message: error.chars().take(4096).collect(),
            },
        );
    }
    fn cancel(&mut self, id: u32) {
        if let Some(cancel) = self.pending.remove(&id) {
            cancel.store(true, Ordering::Release);
        }
        self.decoding.remove(&id);
        self.sounds.remove(&id);
        self.voices.retain(|voice| voice.owner != id);
        self.players.remove(&id);
    }
    fn command(&mut self, command: AudioCommand, root: &std::path::Path) -> Result<(), String> {
        let id = command.id();
        match command {
            AudioCommand::Load { source, .. } => {
                let cancel = Arc::new(AtomicBool::new(false));
                self.readers
                    .sender
                    .as_ref()
                    .ok_or("audio disposed")?
                    .try_send(ReadJob {
                        id,
                        source,
                        root: root.into(),
                        cancel: cancel.clone(),
                    })
                    .map_err(|_| "audio reader queue exhausted")?;
                self.pending.insert(id, cancel);
            }
            AudioCommand::Play { volume, rate, .. } => {
                if self.voices.len() >= MAX_VOICES {
                    return Err("audio voice quota exhausted".into());
                }
                let pcm = self
                    .sounds
                    .get(&id)
                    .ok_or("sound has not completed decoding")?
                    .clone();
                self.voices.push(voice(id, pcm, volume, rate)?);
                self.event(id, AudioEvent::Playing);
            }
            AudioCommand::Stop { .. } => {
                self.voices.retain(|voice| voice.owner != id);
                self.event(id, AudioEvent::Paused);
            }
            AudioCommand::Unload { .. } | AudioCommand::Close { .. } => self.cancel(id),
            AudioCommand::Open {
                source, settings, ..
            } => {
                source.validate()?;
                settings.validate()?;
                let input = match source {
                    AudioSource::Bytes { base64 } => Input::Bytes(
                        base64::engine::general_purpose::STANDARD
                            .decode(base64)
                            .map_err(|e| e.to_string())?,
                    ),
                    AudioSource::File { path } => {
                        Input::Uri(gio::File::for_path(path).uri().into())
                    }
                    AudioSource::Url { url } => Input::Uri(media::source_uri(&url, root)?),
                };
                self.players.insert(id, Player::new(input, settings)?);
            }
            AudioCommand::Preload { .. }
            | AudioCommand::Resume { .. }
            | AudioCommand::Pause { .. } => {
                let player = self
                    .players
                    .get_mut(&id)
                    .ok_or("audio player unavailable")?;
                if matches!(command, AudioCommand::Resume { .. }) {
                    player.playing = true;
                } else if matches!(command, AudioCommand::Pause { .. }) {
                    player.playing = false;
                }
                player
                    .pipeline
                    .set_state(if player.playing {
                        gst::State::Playing
                    } else {
                        gst::State::Paused
                    })
                    .map_err(|e| e.to_string())?;
            }
            AudioCommand::Seek { position, .. } => {
                let player = self
                    .players
                    .get_mut(&id)
                    .ok_or("audio player unavailable")?;
                if player.pipeline.current_state() < gst::State::Paused {
                    player.pending_seek = Some(position);
                } else {
                    player.seek(position)?;
                }
            }
            AudioCommand::Set { settings, .. } => {
                settings.validate()?;
                let player = self
                    .players
                    .get_mut(&id)
                    .ok_or("audio player unavailable")?;
                let rate_changed = player.settings.rate != settings.rate;
                player.settings = settings;
                player.volume.set_property("volume", settings.volume as f64);
                player.volume.set_property("mute", settings.muted);
                if rate_changed {
                    let position = player
                        .pipeline
                        .query_position::<gst::ClockTime>()
                        .map_or(0., |v| v.nseconds() as f64 / 1e9);
                    if player.pipeline.current_state() < gst::State::Paused {
                        player.pending_seek = Some(position);
                    } else {
                        player.seek(position)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn pump(&mut self, root: &std::path::Path) {
        self.timer.borrow_mut().take().map(|timer| timer.remove());
        for command in self.service.take_commands() {
            let id = command.id();
            if let Err(error) = self.command(command, root) {
                self.cancel(id);
                self.failed(id, error);
            }
        }
        while self.decoding.len() < 2 {
            let Some(Ok((id, result))) = self
                .readers
                .receiver
                .as_ref()
                .map(|receiver| receiver.try_recv())
            else {
                break;
            };
            if !self.pending.contains_key(&id) {
                continue;
            }
            match result.and_then(Decoder::new) {
                Ok(decoder) => {
                    self.decoding.insert(id, decoder);
                }
                Err(error) => {
                    self.pending.remove(&id);
                    self.failed(id, error);
                }
            }
        }
        let completed: Vec<_> = self
            .decoding
            .iter()
            .filter_map(|(id, decoder)| decoder.poll().map(|result| (*id, result)))
            .collect();
        for (id, result) in completed {
            self.decoding.remove(&id);
            self.pending.remove(&id);
            match result {
                Ok(pcm)
                    if self
                        .sounds
                        .values()
                        .map(|v| v.len())
                        .sum::<usize>()
                        .saturating_add(pcm.len())
                        <= MAX_DECODED_BYTES as usize =>
                {
                    self.event(
                        id,
                        AudioEvent::Loaded {
                            duration: pcm.len() as f64 / PCM_BYTES as f64,
                            decoded_bytes: pcm.len() as u64,
                        },
                    );
                    self.sounds.insert(id, pcm);
                }
                Ok(_) => self.failed(id, "decoded audio quota exhausted".into()),
                Err(error) => self.failed(id, error),
            }
        }
        let mut events = Vec::new();
        let mut finished = Vec::new();
        for (index, voice) in self.voices.iter().enumerate() {
            if let Some(bus) = voice.pipeline.bus() {
                for _ in 0..128 {
                    let Some(message) = bus.pop() else { break };
                    match message.view() {
                        gst::MessageView::Eos(_) => {
                            finished.push(index);
                            break;
                        }
                        gst::MessageView::Error(error) => {
                            events.push((
                                voice.owner,
                                AudioEvent::Failed {
                                    message: error.error().to_string().chars().take(4096).collect(),
                                },
                            ));
                            finished.push(index);
                            break;
                        }
                        _ => (),
                    }
                }
            }
        }
        let mut ended = Vec::new();
        for index in finished.into_iter().rev() {
            ended.push(self.voices.remove(index).owner);
        }
        for id in ended {
            if !self.voices.iter().any(|v| v.owner == id) {
                events.push((id, AudioEvent::Ended));
            }
        }
        for (id, player) in &mut self.players {
            if let Some(bus) = player.pipeline.bus() {
                for _ in 0..128 {
                    let Some(message) = bus.pop() else { break };
                    match message.view() {
                        gst::MessageView::Error(error) => {
                            events.push((
                                *id,
                                AudioEvent::Failed {
                                    message: error.error().to_string().chars().take(4096).collect(),
                                },
                            ));
                            let _ = player.pipeline.set_state(gst::State::Null);
                        }
                        gst::MessageView::AsyncDone(_) => {
                            if let Some(position) = player.pending_seek.take() {
                                if let Err(error) = player.seek(position) {
                                    let _ = player.pipeline.set_state(gst::State::Null);
                                    events.push((*id, AudioEvent::Failed { message: error }));
                                    continue;
                                }
                            }
                            events.push((
                                *id,
                                AudioEvent::Load {
                                    duration: player
                                        .pipeline
                                        .query_duration::<gst::ClockTime>()
                                        .map(|v| v.nseconds() as f64 / 1e9),
                                },
                            ));
                        }
                        gst::MessageView::Eos(_) => {
                            events.push((*id, AudioEvent::Ended));
                            if player.settings.looping {
                                let _ = player.seek(0.);
                                let _ = player.pipeline.set_state(gst::State::Playing);
                            } else {
                                player.playing = false;
                                let _ = player.pipeline.set_state(gst::State::Paused);
                            }
                        }
                        gst::MessageView::StateChanged(state)
                            if message
                                .src()
                                .is_some_and(|source| source == &player.pipeline) =>
                        {
                            if state.current() == gst::State::Playing {
                                events.push((*id, AudioEvent::Playing));
                            } else if state.current() == gst::State::Paused {
                                events.push((*id, AudioEvent::Paused));
                            }
                        }
                        gst::MessageView::Buffering(buffer) => {
                            let buffering = buffer.percent() < 100;
                            events.push((*id, AudioEvent::Buffer { buffering }));
                            let _ = player.pipeline.set_state(if buffering || !player.playing {
                                gst::State::Paused
                            } else {
                                gst::State::Playing
                            });
                        }
                        _ => (),
                    }
                }
            }
            if player.pipeline.current_state() >= gst::State::Paused
                && player.last_progress.elapsed() >= Duration::from_millis(250)
            {
                player.last_progress = Instant::now();
                if let Some(position) = player.pipeline.query_position::<gst::ClockTime>() {
                    events.push((
                        *id,
                        AudioEvent::Progress {
                            position: position.nseconds() as f64 / 1e9,
                            duration: player
                                .pipeline
                                .query_duration::<gst::ClockTime>()
                                .map(|v| v.nseconds() as f64 / 1e9),
                            buffered: None,
                        },
                    ));
                }
            }
        }
        for (id, event) in events {
            self.event(id, event);
        }
        if !self.pending.is_empty()
            || !self.voices.is_empty()
            || self
                .players
                .values()
                .any(|p| p.pipeline.current_state() != gst::State::Null)
        {
            let wake = self.wake.clone();
            let timer = self.timer.clone();
            *self.timer.borrow_mut() = Some(glib::timeout_add_local_once(
                Duration::from_millis(25),
                move || {
                    timer.borrow_mut().take();
                    wake.wake();
                },
            ));
        }
    }
    pub fn dispose(&mut self) {
        for cancel in self.pending.values() {
            cancel.store(true, Ordering::Release);
        }
        self.pending.clear();
        self.readers.dispose();
        self.decoding.clear();
        self.voices.clear();
        self.players.clear();
        self.sounds.clear();
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.service.dispose();
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(samples: u32) -> Vec<u8> {
        let mut out = b"RIFF".to_vec();
        out.extend((36 + samples * 2).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(48_000u32.to_le_bytes());
        out.extend(96_000u32.to_le_bytes());
        out.extend(2u16.to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend((samples * 2).to_le_bytes());
        for _ in 0..samples {
            out.extend(8_192i16.to_le_bytes());
        }
        out
    }
    fn decode(bytes: Vec<u8>) -> Result<Arc<[u8]>, String> {
        let decoder = Decoder::new(bytes)?;
        loop {
            if let Some(result) = decoder.poll() {
                return result;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn decodes_real_wav_to_stereo_pcm_and_bounds_long_or_invalid_sources() {
        gst::init().unwrap();
        let pcm = decode(wav(4_800)).unwrap();
        assert_eq!(pcm.len(), 4_800 * 4);
        assert!(pcm.chunks_exact(4).all(|sample| sample[..2] == sample[2..]));
        assert!(decode(wav(48_000 * 6)).is_err());
        assert!(decode(b"not audio".to_vec()).is_err());
        // Cancellation owns the pipeline and releases it before returning.
        let decoder = Decoder::new(wav(48_000 * 5)).unwrap();
        let pipeline = decoder.pipeline.clone();
        drop(decoder);
        assert_eq!(pipeline.current_state(), gst::State::Null);
    }
}
