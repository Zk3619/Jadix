//! System Media Foundation rendering, with owned callbacks and accessible controls.
use super::*;
use jadix_core::hosted::MediaFit;
use std::{path::Path, sync::{Arc, Mutex}};
use windows::{core::{implement, BSTR, Interface, PCWSTR, PWSTR}, Win32::{
    Media::MediaFoundation::*, System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    UI::Shell::{PathCreateFromUrlW, UrlCreateFromPathW},
}};

pub(super) const READY: u32 = WM_APP + 0x3a1;
pub(super) const TIMER: usize = 0x4a444d;
#[derive(Default)]
struct Events { closed: bool, overflow: bool, items: VecDeque<u32> }
type Queue = Arc<Mutex<Events>>;
#[implement(IMFMediaEngineNotify)]
struct Notify { events: Queue, window: usize }
impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, _: usize, _: u32) -> windows::core::Result<()> {
        let mut events = self.events.lock().unwrap();
        if !events.closed {
            if events.items.len() < 128 { events.items.push_back(event); }
            else { events.overflow = true; }
            // Serialize waking with shutdown; no callback retains a Leaf pointer.
            unsafe { PostMessageW(self.window as HWND, READY, 0, 0); }
        }
        Ok(())
    }
}

struct Runtime;
impl Drop for Runtime { fn drop(&mut self) { unsafe { let _ = MFShutdown(); } } }
// COM interfaces are released before balancing MFStartup.
struct Engine { engine: IMFMediaEngine, events: Queue, _runtime: Runtime }
impl Engine {
    fn new(window: HWND, notify_window: HWND, audio: bool) -> Result<Self, String> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.map_err(|e| e.to_string())?;
        let runtime = Runtime;
        let events = Queue::default();
        let result = (|| -> windows::core::Result<IMFMediaEngine> { unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 2)?;
            let attributes = attributes.unwrap();
            let notify: IMFMediaEngineNotify = Notify { events: events.clone(), window: notify_window as usize }.into();
            attributes.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &notify)?;
            if !audio { attributes.SetUINT64(&MF_MEDIA_ENGINE_PLAYBACK_HWND, window as u64)?; }
            let factory: IMFMediaEngineClassFactory = CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
            factory.CreateInstance(if audio { MF_MEDIA_ENGINE_AUDIOONLY.0 as u32 } else { 0 }, &attributes)
        } })();
        match result {
            Ok(engine) => Ok(Self { engine, events, _runtime: runtime }),
            Err(error) => { events.lock().unwrap().closed = true; Err(error.to_string()) }
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        { let mut events = self.events.lock().unwrap(); events.closed = true; events.items.clear(); }
        unsafe { let _ = self.engine.Shutdown(); }
    }
}

pub(super) struct Media {
    player: Option<Engine>,
    video: HWND,
    play: HWND,
    seek: HWND,
    time: HWND,
    parent: HWND,
    source: String,
    audio: bool,
    fit: MediaFit,
    controls: bool,
    interval: u32,
    playing: bool,
    timer: Option<u32>,
    pending_seek: Option<f64>,
}
impl Media {
    pub fn new(leaf: &Leaf) -> Result<Self, String> {
        let video = leaf.child("STATIC", "Video", 4 /* SS_BLACKRECT */)?;
        let play = leaf.child("BUTTON", "Play", BS_PUSHBUTTON as u32)?;
        let seek = leaf.child("msctls_trackbar32", "Playback position", TBS_NOTICKS)?;
        let time = leaf.child("STATIC", "0:00 / 0:00", 1 /* SS_CENTER */)?;
        unsafe {
            for child in [video, time] {
                SetWindowLongPtrW(child, GWL_STYLE, GetWindowLongPtrW(child, GWL_STYLE) & !(WS_TABSTOP as isize));
            }
            SendMessageW(seek, TBM_SETRANGEMIN, 0, 0);
            SendMessageW(seek, TBM_SETRANGEMAX, 0, 10_000);
            SendMessageW(seek, TBM_SETPAGESIZE, 0, 100);
        }
        if unsafe { SetWindowSubclass(video, Some(video_procedure), TIMER, leaf as *const Leaf as usize) } == 0 { return Err(native_error("video repaint subclass")); }
        Ok(Self { player: None, video, play, seek, time, parent: leaf.parent,
            source: String::new(), audio: false, fit: MediaFit::Contain, controls: false,
            interval: 250, playing: false, timer: None, pending_seek: None })
    }
    pub fn update(&mut self, leaf: &Leaf, old: &HostedContent, spec: &HostedContent) -> Result<(), String> {
        let HostedContent::Media { source, audio, paused, looping, controls, muted,
            progress_interval_ms, volume, rate, seek, seek_revision, fit } = spec else { return Ok(()); };
        let replace = self.player.is_none() || self.source != *source || self.audio != *audio;
        if replace {
            self.stop_timer();
            self.player.take();
            self.playing = false;
            self.pending_seek = *seek;
            let resolved = resolve_source(source, &leaf.asset_root.lock().unwrap())?;
            self.player = Some(Engine::new(self.video, leaf.parent, *audio)?);
            self.source = source.clone();
            self.audio = *audio;
            unsafe { self.player.as_ref().unwrap().engine.SetSource(&BSTR::from(resolved)) }.map_err(|e| e.to_string())?;
        } else if !matches!(old, HostedContent::Media { seek_revision: previous, .. } if previous == seek_revision) {
            self.pending_seek = *seek;
        }
        self.fit = *fit;
        self.controls = *controls;
        self.interval = *progress_interval_ms;
        let engine = &self.player.as_ref().unwrap().engine;
        (|| -> windows::core::Result<()> { unsafe {
            engine.SetLoop(*looping)?;
            engine.SetMuted(*muted)?;
            engine.SetVolume(*volume)?;
            engine.SetPlaybackRate(*rate)?;
            if replace {
                engine.SetPreload(MF_MEDIA_ENGINE_PRELOAD_AUTOMATIC)?;
                engine.SetAutoPlay(!*paused)?;
                engine.Load()?;
            } else if !matches!(old, HostedContent::Media { paused: previous, .. } if previous == paused) {
                if *paused { engine.Pause()?; } else { engine.Play()?; }
            }
            Ok(())
        } })().map_err(|e| e.to_string())?;
        self.apply_seek()?;
        self.sync_timer()?;
        self.resize();
        Ok(())
    }
    fn stop_timer(&mut self) {
        if self.timer.take().is_some() { unsafe { KillTimer(self.parent, TIMER); } }
    }
    fn sync_timer(&mut self) -> Result<(), String> {
        if self.playing && self.timer == Some(self.interval) { return Ok(()); }
        self.stop_timer();
        if self.playing {
            if unsafe { SetTimer(self.parent, TIMER, self.interval, None) } == 0 { return Err(native_error("media progress timer")); }
            self.timer = Some(self.interval);
        }
        Ok(())
    }
    fn apply_seek(&mut self) -> Result<(), String> {
        let Some(position) = self.pending_seek else { return Ok(()); };
        let Some(player) = &self.player else { return Ok(()); };
        if unsafe { player.engine.GetReadyState() } >= MF_MEDIA_ENGINE_READY_HAVE_METADATA.0 as u16 {
            let duration = unsafe { player.engine.GetDuration() };
            let position = if duration.is_finite() && duration > 0. { position.min(duration) } else { position };
            unsafe { player.engine.SetCurrentTime(position) }.map_err(|e| e.to_string())?;
            self.pending_seek = None;
        }
        Ok(())
    }
    pub fn pump(&mut self, leaf: &Leaf) {
        let Some(player) = &self.player else { return; };
        let (overflow, events) = {
            let mut queue = player.events.lock().unwrap();
            (std::mem::take(&mut queue.overflow), std::mem::take(&mut queue.items))
        };
        if overflow { leaf.context().failed.set(true); }
        for event in events {
            match MF_MEDIA_ENGINE_EVENT(event as i32) {
                MF_MEDIA_ENGINE_EVENT_LOADSTART => leaf.defer("loadStart", true.into()),
                MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA => {
                    let duration = unsafe { self.player.as_ref().unwrap().engine.GetDuration() };
                    if duration.is_finite() && duration >= 0. {
                        leaf.defer("duration", duration.into());
                        leaf.defer("load", serde_json::json!({ "duration": duration }));
                    }
                    if let Err(error) = self.apply_seek() { leaf.defer("error", error.into()); }
                    self.resize();
                }
                MF_MEDIA_ENGINE_EVENT_DURATIONCHANGE => {
                    let duration = unsafe { self.player.as_ref().unwrap().engine.GetDuration() };
                    if duration.is_finite() && duration >= 0. { leaf.defer("duration", duration.into()); }
                    self.progress(leaf);
                }
                MF_MEDIA_ENGINE_EVENT_LOADEDDATA | MF_MEDIA_ENGINE_EVENT_SEEKED => {
                    if let Err(error) = self.repaint() { leaf.defer("error", error.to_string().into()); }
                    self.progress(leaf);
                }
                MF_MEDIA_ENGINE_EVENT_PLAYING | MF_MEDIA_ENGINE_EVENT_PAUSE | MF_MEDIA_ENGINE_EVENT_ENDED => {
                    self.playing = unsafe {
                        let engine = &self.player.as_ref().unwrap().engine;
                        !engine.IsPaused().as_bool() && !engine.IsEnded().as_bool()
                    };
                    leaf.defer("playing", self.playing.into());
                    leaf.defer("buffer", false.into());
                    if event == MF_MEDIA_ENGINE_EVENT_ENDED.0 as u32 { leaf.defer("ended", true.into()); }
                    if let Err(error) = self.sync_timer() { leaf.defer("error", error.into()); }
                    self.progress(leaf);
                }
                MF_MEDIA_ENGINE_EVENT_WAITING => leaf.defer("buffer", true.into()),
                MF_MEDIA_ENGINE_EVENT_CANPLAY | MF_MEDIA_ENGINE_EVENT_CANPLAYTHROUGH => leaf.defer("buffer", false.into()),
                MF_MEDIA_ENGINE_EVENT_ERROR => {
                    self.playing = false;
                    self.stop_timer();
                    leaf.defer("playing", false.into());
                    let engine = &self.player.as_ref().unwrap().engine;
                    let error = unsafe { engine.GetError() }.map(|error| unsafe {
                        format!("Media Foundation error {}: {:?}", error.GetErrorCode(), error.GetExtendedErrorCode())
                    }).unwrap_or_else(|error| error.to_string());
                    leaf.defer("error", error.into());
                }
                _ => {}
            }
        }
    }
    pub fn progress(&self, leaf: &Leaf) {
        let Some(player) = &self.player else { return; };
        let engine = &player.engine;
        let position = unsafe { engine.GetCurrentTime() };
        let duration = unsafe { engine.GetDuration() };
        if !position.is_finite() || position < 0. { return; }
        let mut progress = serde_json::json!({ "position": position });
        if duration.is_finite() && duration > 0. {
            progress["duration"] = duration.into();
            unsafe { SendMessageW(self.seek, TBM_SETPOS, 1, ((position / duration).clamp(0., 1.) * 10_000.).round() as isize); }
        }
        if let Ok(buffered) = unsafe { engine.GetBuffered() } {
            let count = unsafe { buffered.GetLength() };
            if count > 0 {
                if let Ok(end) = unsafe { buffered.GetEnd(count - 1) } {
                    if end.is_finite() && end >= 0. { progress["buffered"] = end.into(); }
                }
            }
        }
        unsafe {
            SetWindowTextW(self.time, wide(&format!("{} / {}", clock(position), clock(duration))).as_ptr());
            SetWindowTextW(self.play, wide(if self.playing { "Pause" } else { "Play" }).as_ptr());
            EnableWindow(self.seek, i32::from(duration.is_finite() && duration > 0.));
        }
        leaf.defer("time", position.into());
        leaf.defer("progress", progress);
    }
    pub fn command(&mut self, leaf: &Leaf, child: HWND, scroll: bool) {
        let Some(player) = &self.player else { return; };
        let result = unsafe {
            if !scroll && child == self.play {
                if player.engine.IsPaused().as_bool() { player.engine.Play() } else { player.engine.Pause() }
            } else if scroll && child == self.seek {
                let duration = player.engine.GetDuration();
                if !duration.is_finite() || duration <= 0. { return; }
                let position = SendMessageW(self.seek, TBM_GETPOS, 0, 0) as f64 / 10_000. * duration;
                player.engine.SetCurrentTime(position)
            } else { return; }
        };
        if let Err(error) = result { leaf.defer("error", error.to_string().into()); }
        self.progress(leaf);
    }
    pub fn resize(&self) {
        let mut frame = RECT::default(); unsafe { GetClientRect(self.parent, &mut frame); }
        let width = frame.right.max(0);
        let height = frame.bottom.max(0);
        let scale = unsafe { GetDpiForWindow(self.parent) }.max(96) as f64 / 96.;
        let bar = if self.controls { ((36. * scale).round() as i32).min(height) } else { 0 };
        let button = ((48. * scale).round() as i32).min(width);
        let time = ((112. * scale).round() as i32).min(width - button);
        unsafe {
            ShowWindow(self.video, if self.audio { SW_HIDE } else { SW_SHOWNA });
            SetWindowPos(self.video, null_mut(), 0, 0, width, height - bar, SWP_NOZORDER | SWP_NOACTIVATE);
            for (child, x, w) in [(self.play, 0, button), (self.seek, button, width - button - time), (self.time, width - time, time)] {
                ShowWindow(child, if self.controls { SW_SHOWNA } else { SW_HIDE });
                SetWindowPos(child, null_mut(), x, height - bar, w, bar, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        let _ = self.repaint();
    }
    pub fn focus(&self) { if self.controls { unsafe { SetFocus(self.play); } } }
    pub fn repaint(&self) -> windows::core::Result<()> {
        let Some(player) = &self.player else { return Ok(()); };
        if self.audio { return Ok(()); }
        let mut frame = RECT::default(); unsafe { GetClientRect(self.video, &mut frame); }
        let mut width = 0; let mut height = 0;
        unsafe { let _ = player.engine.GetVideoAspectRatio(Some(&mut width), Some(&mut height)); }
        let Some((dest, source)) = self.fit.geometry([0., 0., frame.right as f64, frame.bottom as f64], [width as f64, height as f64]) else { return Ok(()); };
        let source = MFVideoNormalizedRect { left: source[0] as f32, top: source[1] as f32,
            right: (source[0] + source[2]) as f32, bottom: (source[1] + source[3]) as f32 };
        let dest = windows::Win32::Foundation::RECT { left: dest[0].round() as i32, top: dest[1].round() as i32,
            right: (dest[0] + dest[2]).round() as i32, bottom: (dest[1] + dest[3]).round() as i32 };
        let border = MFARGB { rgbAlpha: 255, ..Default::default() };
        unsafe { player.engine.cast::<IMFMediaEngineEx>()?.UpdateVideoStream(Some(&source), Some(&dest), Some(&border)) }
    }
}
impl Drop for Media { fn drop(&mut self) { self.stop_timer(); unsafe { RemoveWindowSubclass(self.video, Some(video_procedure), TIMER); } self.player.take(); } }

unsafe extern "system" fn video_procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, reference: usize) -> LRESULT {
    let result = unsafe { DefSubclassProc(window, message, parameter, data) };
    if message == WM_PAINT {
        let leaf = unsafe { &*(reference as *const Leaf) };
        if !leaf.updating.get() {
            if let Ok(media) = leaf.media.try_borrow() {
                if let Some(media) = media.as_ref() {
                    if let Err(error) = media.repaint() { leaf.defer("error", error.to_string().into()); }
                }
            }
        }
    }
    result
}

fn clock(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0. { return "--:--".into(); }
    let seconds = seconds as u64;
    if seconds >= 3600 { format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60) }
    else { format!("{}:{:02}", seconds / 60, seconds % 60) }
}
fn resolve_source(source: &str, root: &Path) -> Result<String, String> {
    use jadix_core::audio::AudioSource;
    if source.starts_with("https://") { return Ok(source.into()); }
    let source = if source.starts_with("file:///") {
        let mut path = vec![0u16; 32768]; let mut length = path.len() as u32;
        unsafe { PathCreateFromUrlW(PCWSTR(wide(source).as_ptr()), PWSTR(path.as_mut_ptr()), &mut length, 0) }.map_err(|e| e.to_string())?;
        let length = path.iter().position(|c| *c == 0).unwrap_or(length as usize);
        AudioSource::File { path: std::path::PathBuf::from(String::from_utf16(&path[..length]).map_err(|e| e.to_string())?) }
    } else { AudioSource::Url { url: source.into() } };
    let path = crate::audio_source::local_path(&source, root)?.ok_or("invalid local media source")?;
    let path = media_path(&path)?;
    let mut url = vec![0u16; 32768]; let mut length = url.len() as u32;
    unsafe { UrlCreateFromPathW(PCWSTR(wide(&path).as_ptr()), PWSTR(url.as_mut_ptr()), &mut length, 0) }.map_err(|e| e.to_string())?;
    let length = url.iter().position(|c| *c == 0).unwrap_or(length as usize);
    String::from_utf16(&url[..length]).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::NativeRoot;
    use std::time::{Duration, Instant};

    #[test]
    fn system_media_loads_seeks_preserves_user_pause_and_closes_callbacks() {
        let root = std::env::temp_dir().join(format!("jadix-media-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("中文 clip.wav");
        let samples = 16_000u32;
        let length = samples * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF"); bytes.extend_from_slice(&(36 + length).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&8000u32.to_le_bytes()); bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data"); bytes.extend_from_slice(&length.to_le_bytes());
        bytes.resize(44 + length as usize, 0); std::fs::write(&path, bytes).unwrap();
        let mut host = WindowsHost::new("Jadix system media probe", [640, 360]).unwrap();
        host.set_asset_root(&root).unwrap();
        let mut surface = NativeRoot::new(host);
        let id = NodeId::new(surface.model().id());
        let spec = HostedContent::Media { source: "asset:/中文 clip.wav".into(), audio: true, paused: true,
            looping: false, controls: true, muted: true, progress_interval_ms: 50, volume: 0.5, rate: 1.,
            seek: Some(0.5), seek_revision: 1, fit: MediaFit::Contain };
        let descriptor = NodeDescriptor {
            content: NodeContent::Hosted { specification: serde_json::to_string(&spec).unwrap() },
            frame: Rect { x: 12., y: 12., width: 600., height: 320. },
            style: Default::default(), accessible_role: None, accessible_selected: None,
            accessible_expanded: None, interaction: Default::default(), keyboard: Default::default(),
            navigation_keys: Vec::new(), accessibility: Default::default(),
            accessibility_relations: Default::default(), accessible_description: None,
            accessible_name: None,
        };
        surface.present(&mut [PresentationNode { id, descriptor, children: vec![] }], [640., 360.], 0).unwrap();
        surface.host().show();
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            surface.host().pump();
            let events = surface.poll_events().unwrap();
            assert!(!events.iter().any(|e| matches!(&e.payload, EventPayload::Hosted { event, .. } if event == "error")), "{events:?}");
            let ready = {
                let media = surface.host().hosted[&id].media.borrow(); let media = media.as_ref().unwrap();
                (unsafe { media.player.as_ref().unwrap().engine.GetReadyState() >= MF_MEDIA_ENGINE_READY_HAVE_METADATA.0 as u16 })
                    && media.pending_seek.is_none()
            };
            if ready { break; }
            assert!(Instant::now() < deadline, "system media did not load");
            std::thread::sleep(Duration::from_millis(10));
        }
        let leaf = &surface.host().hosted[&id];
        let mut playing = spec.clone();
        if let HostedContent::Media { paused, .. } = &mut playing { *paused = false; }
        leaf.update(playing.clone()).unwrap();
        let (engine, queue) = {
            let media = leaf.media.borrow(); let player = media.as_ref().unwrap().player.as_ref().unwrap();
            (player.engine.clone(), player.events.clone())
        };
        unsafe { engine.Pause().unwrap(); }
        let mut volume = playing;
        if let HostedContent::Media { volume, .. } = &mut volume { *volume = 0.2; }
        leaf.update(volume).unwrap();
        assert!(unsafe { engine.IsPaused().as_bool() }, "volume updates must preserve a user pause");
        assert!((unsafe { engine.GetCurrentTime() } - 0.5).abs() < 0.1);
        assert!(resolve_source("asset:/%2e%2e/escape.wav", &root).is_err());
        assert_eq!(resolve_source(&format!("file:///{}", path.to_string_lossy().replace('\\', "/")), &root).unwrap(), resolve_source("asset:/中文 clip.wav", &root).unwrap());
        drop(engine);
        drop(surface);
        assert!(queue.lock().unwrap().closed);
        let notify: IMFMediaEngineNotify = Notify { events: queue.clone(), window: 0 }.into();
        unsafe { notify.EventNotify(MF_MEDIA_ENGINE_EVENT_PLAYING.0 as u32, 0, 0).unwrap(); }
        assert!(queue.lock().unwrap().items.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
