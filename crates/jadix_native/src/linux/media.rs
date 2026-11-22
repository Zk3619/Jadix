//! GStreamer decoding with a single bounded frame handed to GTK's main thread.
use super::*;
use gstreamer::{self as gst, prelude::*};
use jadix_core::hosted::{HostedContent, MediaFit};
use std::sync::{Arc, Mutex};

struct Frame {
    width: i32,
    height: i32,
    bytes: Vec<u8>,
}
pub(super) struct Media {
    pub widget: gtk::Box,
    picture: gtk::Picture,
    controls: gtk::Box,
    play: gtk::Button,
    position: gtk::Scale,
    player: gst::Element,
    frame: Arc<Mutex<Option<Frame>>>,
    bus: RefCell<Option<gst::bus::BusWatchGuard>>,
    timer: RefCell<Option<glib::SourceId>>,
    specification: RefCell<Option<HostedContent>>,
    seek_pending: Cell<bool>,
    updating: Cell<bool>,
    buffering: Cell<bool>,
    last_progress: Cell<std::time::Instant>,
    events: Rc<Events>,
    control: Rc<Control>,
    resources: Rc<web::Runtime>,
}
impl Media {
    pub fn new(
        events: Rc<Events>,
        control: Rc<Control>,
        resources: Rc<web::Runtime>,
    ) -> Result<Rc<Self>, String> {
        gst::init().map_err(|e| e.to_string())?;
        let player = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|e| e.to_string())?;
        let sink = gstreamer_app::AppSink::builder()
            .max_buffers(1)
            .drop(true)
            .wait_on_eos(false)
            .build();
        sink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", gst::IntRange::<i32>::new(1, 4096))
                .field("height", gst::IntRange::<i32>::new(1, 4096))
                .build(),
        ));
        let frame = Arc::new(Mutex::new(None));
        let sample = |preroll: bool| {
            let (frame, wake) = (frame.clone(), events.wake.clone());
            move |sink: &gstreamer_app::AppSink| {
                let sample = if preroll {
                    sink.pull_preroll()
                } else {
                    sink.pull_sample()
                }
                .map_err(|_| gst::FlowError::Eos)?;
                let caps = sample
                    .caps()
                    .and_then(|v| v.structure(0))
                    .ok_or(gst::FlowError::NotNegotiated)?;
                let width = caps
                    .get::<i32>("width")
                    .map_err(|_| gst::FlowError::NotNegotiated)?;
                let height = caps
                    .get::<i32>("height")
                    .map_err(|_| gst::FlowError::NotNegotiated)?;
                if !(1..=4096).contains(&width) || !(1..=4096).contains(&height) {
                    return Err(gst::FlowError::NotNegotiated);
                }
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let bytes = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                if bytes.len() != width as usize * height as usize * 4 {
                    return Err(gst::FlowError::NotNegotiated);
                }
                *frame.lock().map_err(|_| gst::FlowError::Error)? = Some(Frame {
                    width,
                    height,
                    bytes: bytes.to_vec(),
                });
                wake.wake();
                Ok(gst::FlowSuccess::Ok)
            }
        };
        sink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(sample(false))
                .new_preroll(sample(true))
                .build(),
        );
        player.set_property("video-sink", &sink);
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let picture = gtk::Picture::new();
        picture.set_can_shrink(true);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let play = gtk::Button::with_label("Play");
        let position = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0., 1., 0.1);
        position.set_draw_value(false);
        position.set_hexpand(true);
        controls.append(&play);
        controls.append(&position);
        widget.append(&picture);
        widget.append(&controls);
        let this = Rc::new(Self {
            widget,
            picture,
            controls,
            play,
            position,
            player,
            frame,
            bus: Default::default(),
            timer: Default::default(),
            specification: Default::default(),
            seek_pending: Cell::new(false),
            updating: Cell::new(false),
            buffering: Cell::new(false),
            last_progress: Cell::new(std::time::Instant::now()),
            events,
            control,
            resources,
        });
        let weak = Rc::downgrade(&this);
        this.play.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                let state = if this.player.current_state() == gst::State::Playing {
                    gst::State::Paused
                } else {
                    gst::State::Playing
                };
                if let Err(error) = this.player.set_state(state) {
                    this.error(error.to_string());
                }
            }
        });
        let weak = Rc::downgrade(&this);
        this.position.connect_change_value(move |_, _, position| {
            if let Some(this) = weak.upgrade() {
                if !this.updating.get() {
                    this.seek(position);
                }
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&this);
        *this.bus.borrow_mut() = Some(
            this.player
                .bus()
                .ok_or("media pipeline has no bus")?
                .add_watch_local(move |_, message| {
                    let Some(this) = weak.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    match message.view() {
                        gst::MessageView::Error(error) => {
                            this.error(error.error().to_string());
                            let _ = this.player.set_state(gst::State::Null);
                        }
                        gst::MessageView::Eos(_) => {
                            this.event("ended", true.into());
                            if matches!(
                                &*this.specification.borrow(),
                                Some(HostedContent::Media { looping: true, .. })
                            ) {
                                this.seek(0.);
                                let _ = this.player.set_state(gst::State::Playing);
                            } else {
                                let _ = this.player.set_state(gst::State::Paused);
                            }
                        }
                        gst::MessageView::AsyncDone(_) => {
                            if this.seek_pending.replace(false) {
                                this.seek(this.requested_position());
                            }
                            this.progress("load");
                            this.event("loadStart", false.into());
                        }
                        gst::MessageView::Buffering(buffer) => {
                            let buffering = buffer.percent() < 100;
                            this.buffering.set(buffering);
                            this.event("buffer", buffering.into());
                            let paused = matches!(
                                &*this.specification.borrow(),
                                Some(HostedContent::Media { paused: true, .. })
                            );
                            let _ = this.player.set_state(if paused || buffering {
                                gst::State::Paused
                            } else {
                                gst::State::Playing
                            });
                        }
                        gst::MessageView::StateChanged(state)
                            if message.src().is_some_and(|source| source == &this.player) =>
                        {
                            let playing = state.current() == gst::State::Playing;
                            this.play.set_label(if playing { "Pause" } else { "Play" });
                            this.event("playing", playing.into());
                            this.sync_timer();
                        }
                        _ => (),
                    }
                    glib::ControlFlow::Continue
                })
                .map_err(|e| e.to_string())?,
        );
        Ok(this)
    }
    fn event(&self, event: &str, data: serde_json::Value) {
        if self
            .specification
            .borrow()
            .as_ref()
            .is_some_and(|s| s.accepts_event(event, &data))
        {
            self.events.enqueue(
                &self.control,
                EventPayload::Hosted {
                    event: event.into(),
                    data,
                },
            );
        }
    }
    fn error(&self, error: String) {
        self.event("error", error.chars().take(4096).collect::<String>().into());
    }
    fn requested_position(&self) -> f64 {
        match &*self.specification.borrow() {
            Some(HostedContent::Media {
                seek: Some(seek), ..
            }) => *seek,
            _ => self
                .player
                .query_position::<gst::ClockTime>()
                .map_or(0., |v| v.nseconds() as f64 / 1e9),
        }
    }
    fn seek(&self, seconds: f64) {
        let rate = match &*self.specification.borrow() {
            Some(HostedContent::Media { rate, .. }) => *rate,
            _ => 1.,
        };
        if self
            .player
            .seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::from_nseconds((seconds * 1e9) as u64),
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
            .is_err()
        {
            self.error(
                "media source does not support seeking or the requested playback rate".into(),
            );
        }
    }
    fn progress(&self, event: &str) {
        let position = self
            .player
            .query_position::<gst::ClockTime>()
            .map(|v| v.nseconds() as f64 / 1e9);
        let duration = self
            .player
            .query_duration::<gst::ClockTime>()
            .map(|v| v.nseconds() as f64 / 1e9);
        let mut data = serde_json::Map::new();
        if let Some(value) = position {
            data.insert("position".into(), value.into());
        }
        if let Some(value) = duration {
            data.insert("duration".into(), value.into());
        }
        if !data.is_empty() {
            self.event(event, data.into());
        }
        self.updating.set(true);
        if let Some(duration) = duration {
            self.position.set_range(0., duration.max(0.001));
        }
        if let Some(position) = position {
            self.position.set_value(position);
        }
        self.updating.set(false);
    }
    pub fn pump(&self) {
        if let Some(frame) = self.frame.lock().ok().and_then(|mut frame| frame.take()) {
            let texture = gdk::MemoryTexture::new(
                frame.width,
                frame.height,
                gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from_owned(frame.bytes),
                frame.width as usize * 4,
            );
            self.picture.set_paintable(Some(&texture));
        }
        let interval = match &*self.specification.borrow() {
            Some(HostedContent::Media {
                progress_interval_ms,
                ..
            }) => *progress_interval_ms,
            _ => 250,
        };
        if self.player.current_state() == gst::State::Playing
            && self.last_progress.get().elapsed().as_millis() >= interval as u128
        {
            self.last_progress.set(std::time::Instant::now());
            self.progress("progress");
        }
    }
    pub fn update(self: &Rc<Self>, spec: &HostedContent) {
        let HostedContent::Media {
            source,
            audio,
            paused,
            controls,
            muted,
            volume,
            rate,
            seek_revision,
            fit,
            ..
        } = spec
        else {
            return;
        };
        if self.specification.borrow().as_ref() == Some(spec) {
            return;
        }
        let old = self.specification.replace(Some(spec.clone()));
        if !matches!(&old, Some(HostedContent::Media { progress_interval_ms, .. }) if matches!(spec, HostedContent::Media { progress_interval_ms: current, .. } if current == progress_interval_ms))
        {
            if let Some(timer) = self.timer.borrow_mut().take() {
                timer.remove();
            }
        }
        let source_changed =
            !matches!(&old,Some(HostedContent::Media {source:old,..}) if old==source);
        self.picture.set_visible(!audio);
        self.controls.set_visible(*controls);
        self.picture.set_content_fit(match fit {
            MediaFit::Contain => gtk::ContentFit::Contain,
            MediaFit::Cover => gtk::ContentFit::Cover,
            MediaFit::Fill => gtk::ContentFit::Fill,
        });
        self.player.set_property("volume", volume);
        self.player.set_property("mute", muted);
        if source_changed {
            let _ = self.player.set_state(gst::State::Null);
            self.buffering.set(false);
            self.frame.lock().unwrap().take();
            self.picture.set_paintable(gdk::Paintable::NONE);
            let uri = match source_uri(source, &self.resources.asset_root()) {
                Ok(uri) => uri,
                Err(error) => {
                    self.error(error);
                    return;
                }
            };
            self.player.set_property("uri", &uri);
            self.event("loadStart", true.into());
            self.seek_pending.set(true);
        } else if !matches!(&old,Some(HostedContent::Media {rate:old_rate,seek_revision:old_revision,..}) if old_rate==rate && old_revision==seek_revision)
        {
            let position = if matches!(&old,Some(HostedContent::Media {seek_revision:old_revision,..}) if old_revision==seek_revision)
            {
                self.player
                    .query_position::<gst::ClockTime>()
                    .map_or(0., |v| v.nseconds() as f64 / 1e9)
            } else {
                self.requested_position()
            };
            self.seek(position);
        }
        if let Err(error) = self.player.set_state(if *paused || self.buffering.get() {
            gst::State::Paused
        } else {
            gst::State::Playing
        }) {
            self.error(error.to_string());
        }
        self.sync_timer();
    }
    fn sync_timer(self: &Rc<Self>) {
        // Decoded frames already wake the host. Only playing audio/video needs a
        // position clock; a paused or ended player has no recurring source.
        if self.player.current_state() != gst::State::Playing {
            if let Some(timer) = self.timer.borrow_mut().take() {
                timer.remove();
            }
            return;
        }
        if self.timer.borrow().is_some() {
            return;
        }
        let interval = match &*self.specification.borrow() {
            Some(HostedContent::Media {
                progress_interval_ms,
                ..
            }) => *progress_interval_ms,
            _ => 250,
        };
        let weak = Rc::downgrade(self);
        *self.timer.borrow_mut() = Some(glib::timeout_add_local(
            std::time::Duration::from_millis(interval.max(16) as u64),
            move || {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                this.pump();
                glib::ControlFlow::Continue
            },
        ));
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.get_mut().take() {
            timer.remove();
        }
        self.bus.get_mut().take();
        let _ = self.player.set_state(gst::State::Null);
    }
}

pub(super) fn source_uri(source: &str, root: &std::path::Path) -> Result<String, String> {
    if NodeContent::is_remote_image_source(source) {
        return Ok(source.into());
    }
    let path = if let Some(path) = jadix_core::web_resource::bundle_path(source) {
        let path = root.join(path).canonicalize().map_err(|e| e.to_string())?;
        if !path.starts_with(root) {
            return Err("media asset escapes the bundle".into());
        }
        path
    } else if source.starts_with("file://") {
        gio::File::for_uri(source)
            .path()
            .ok_or("media file URL is not local")?
            .canonicalize()
            .map_err(|e| e.to_string())?
    } else {
        return Err("media requires an HTTPS, local file, or bundled asset URL".into());
    };
    if !path.is_file() {
        return Err("media source is not a file".into());
    }
    Ok(gio::File::for_path(path).uri().to_string())
}
