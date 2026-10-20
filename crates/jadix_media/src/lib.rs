//! Backend-neutral playback contracts using only the standard library.
//!
//! Adapters own decoding, devices, threads, clocks, transport, and resource lifetime.
//! A successful command means accepted; backend events confirm playback state.

use std::{error::Error, fmt, path::PathBuf, sync::Arc, time::Duration};

/// Recommended steady-state position cadence. Adapters coalesce intermediate positions.
/// Seek completion, discontinuities, pause and end observations bypass this interval.
pub const RECOMMENDED_POSITION_EVENT_INTERVAL: Duration = Duration::from_millis(200);

/// A location interpreted by the adapter. Construct paths explicitly to avoid URI ambiguity.
#[derive(Clone, PartialEq, Eq)]
pub enum MediaSource {
    Uri(String),
    File(PathBuf),
    /// Immutable encoded media, shared across command queues and snapshots.
    Bytes(Arc<[u8]>),
}

impl fmt::Debug for MediaSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(uri) => formatter.debug_tuple("Uri").field(uri).finish(),
            Self::File(path) => formatter.debug_tuple("File").field(path).finish(),
            Self::Bytes(bytes) => formatter
                .debug_struct("Bytes")
                .field("len", &bytes.len())
                .finish(),
        }
    }
}

impl From<Arc<[u8]>> for MediaSource {
    fn from(value: Arc<[u8]>) -> Self {
        Self::Bytes(value)
    }
}

impl From<Vec<u8>> for MediaSource {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value.into())
    }
}

impl From<&str> for MediaSource {
    fn from(value: &str) -> Self {
        Self::Uri(value.to_owned())
    }
}

impl From<String> for MediaSource {
    fn from(value: String) -> Self {
        Self::Uri(value)
    }
}

impl From<PathBuf> for MediaSource {
    fn from(value: PathBuf) -> Self {
        Self::File(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MediaPreload {
    None,
    #[default]
    Metadata,
    Auto,
}

/// Hints passed to an adapter on load; the UI never performs them implicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaLoadOptions {
    pub preload: MediaPreload,
    pub autoplay: bool,
    pub looping: bool,
    pub muted: bool,
    pub volume: f32,
    pub playback_rate: f32,
}

impl Default for MediaLoadOptions {
    fn default() -> Self {
        Self {
            preload: MediaPreload::Metadata,
            autoplay: false,
            looping: false,
            muted: false,
            volume: 1.0,
            playback_rate: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Ready,
    Playing,
    Paused,
    Buffering,
    Ended,
    Error,
}

impl PlaybackState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Buffering => "buffering",
            Self::Ended => "ended",
            Self::Error => "error",
        }
    }
}

/// False means unavailable, including before an adapter reports its capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaCapabilities {
    pub audio: bool,
    pub video: bool,
    pub play: bool,
    pub pause: bool,
    pub stop: bool,
    pub seek: bool,
    pub volume: bool,
    pub mute: bool,
    pub playback_rate: bool,
    pub looping: bool,
}

/// A finite inclusive interval on the timeline, also usable for live DVR windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaTimeRange {
    start: Duration,
    end: Duration,
}

impl MediaTimeRange {
    pub fn new(start: Duration, end: Duration) -> Result<Self, MediaError> {
        if start > end {
            return Err(MediaError::invalid("range start must not exceed its end"));
        }
        Ok(Self { start, end })
    }

    pub fn start(self) -> Duration {
        self.start
    }
    pub fn end(self) -> Duration {
        self.end
    }
    pub fn duration(self) -> Duration {
        self.end - self.start
    }
    pub fn clamp(self, position: Duration) -> Duration {
        position.clamp(self.start, self.end)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// An image source for the Jadix image loader; never a media URL.
    pub artwork: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaVideoSize {
    pub width: u32,
    pub height: u32,
}

/// Authoritative backend state. An absent duration covers unknown and live media.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSnapshot {
    /// Monotonic identity of this load. Zero is the initial, unloaded session.
    pub load_session: u64,
    pub source: Option<MediaSource>,
    pub state: PlaybackState,
    /// Whether playback should resume after buffering; supplied by the adapter.
    pub play_when_ready: bool,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub buffered: Vec<MediaTimeRange>,
    /// Explicit seek window. If absent, a known duration supplies a VOD window.
    pub seekable: Option<MediaTimeRange>,
    pub volume: f32,
    pub muted: bool,
    pub playback_rate: f32,
    pub looping: bool,
    pub capabilities: MediaCapabilities,
    pub metadata: MediaMetadata,
    /// An already queued Jadix texture key. Frame upload and timing belong to the adapter.
    pub video_texture: Option<String>,
    pub video_size: Option<MediaVideoSize>,
    pub error: Option<MediaError>,
}

impl Default for MediaSnapshot {
    fn default() -> Self {
        Self {
            load_session: 0,
            source: None,
            state: PlaybackState::Idle,
            play_when_ready: false,
            position: Duration::ZERO,
            duration: None,
            buffered: Vec::new(),
            seekable: None,
            volume: 1.0,
            muted: false,
            playback_rate: 1.0,
            looping: false,
            capabilities: MediaCapabilities::default(),
            metadata: MediaMetadata::default(),
            video_texture: None,
            video_size: None,
            error: None,
        }
    }
}

impl MediaSnapshot {
    /// Fence a newly submitted load before dispatch. Reset all source-specific observations.
    /// Playback and load options remain unconfirmed until the adapter reports them.
    pub fn begin_load(&mut self, load_session: u64, source: MediaSource) -> bool {
        if load_session <= self.load_session {
            return false;
        }
        *self = Self {
            load_session,
            source: Some(source),
            volume: self.volume,
            muted: self.muted,
            playback_rate: self.playback_rate,
            looping: self.looping,
            ..Self::default()
        };
        true
    }

    pub fn seek_window(&self) -> Option<MediaTimeRange> {
        if !self.capabilities.seek {
            return None;
        }
        self.seekable
            .or_else(|| {
                self.duration.map(|end| MediaTimeRange {
                    start: Duration::ZERO,
                    end,
                })
            })
            .filter(|range| !range.duration().is_zero())
    }

    pub fn wants_playback(&self) -> bool {
        self.state == PlaybackState::Playing
            || (self.state == PlaybackState::Buffering && self.play_when_ready)
    }

    /// Sanitize adapter numeric data before exposing it to layout and controls.
    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    fn normalize(&mut self) {
        if self.state == PlaybackState::Idle {
            self.video_texture = None;
            self.video_size = None;
            self.play_when_ready = false;
        }
        self.volume = if self.volume.is_finite() {
            self.volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if !self.playback_rate.is_finite() || self.playback_rate <= 0.0 {
            self.playback_rate = 1.0;
        }
        if let Some(duration) = self.duration {
            self.position = self.position.min(duration);
        }
    }

    /// Apply only the active load's observations. Older and unstarted future sessions
    /// are rejected without mutation; a full snapshot must also match the envelope.
    /// Returns whether the event was accepted, including an unchanged observation.
    pub fn apply(&mut self, event: MediaEvent) -> bool {
        if event.load_session != self.load_session {
            return false;
        }
        match event.kind {
            MediaEventKind::Snapshot(snapshot) => {
                if snapshot.load_session != self.load_session {
                    return false;
                }
                *self = snapshot;
            }
            MediaEventKind::StateChanged {
                state,
                play_when_ready,
            } => {
                self.state = state;
                self.play_when_ready = play_when_ready;
                if state != PlaybackState::Error {
                    self.error = None;
                }
            }
            MediaEventKind::PositionChanged(position) => self.position = position,
            MediaEventKind::DurationChanged(duration) => self.duration = duration,
            MediaEventKind::BufferedChanged(buffered) => self.buffered = buffered,
            MediaEventKind::SeekableChanged(seekable) => self.seekable = seekable,
            MediaEventKind::VolumeChanged(volume) => self.volume = volume,
            MediaEventKind::MutedChanged(muted) => self.muted = muted,
            MediaEventKind::PlaybackRateChanged(rate) => self.playback_rate = rate,
            MediaEventKind::LoopChanged(looping) => self.looping = looping,
            MediaEventKind::MetadataChanged(metadata) => self.metadata = metadata,
            MediaEventKind::CapabilitiesChanged(capabilities) => self.capabilities = capabilities,
            MediaEventKind::VideoTextureChanged(texture) => self.video_texture = texture,
            MediaEventKind::VideoSizeChanged(size) => self.video_size = size,
            MediaEventKind::Error(error) => {
                self.error = Some(error);
                self.state = PlaybackState::Error;
                self.play_when_ready = false;
            }
        }
        self.normalize();
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaCommand {
    Load {
        /// Assigned once by the controller; never reuse a token after rejection.
        load_session: u64,
        source: MediaSource,
        options: MediaLoadOptions,
    },
    Play,
    Pause,
    Stop,
    Seek(Duration),
    SetVolume(f32),
    SetMuted(bool),
    SetPlaybackRate(f32),
    SetLoop(bool),
}

impl MediaCommand {
    /// Validate input and advertised capabilities without executing playback.
    pub fn validate(&self, snapshot: &MediaSnapshot) -> Result<(), MediaError> {
        let capabilities = snapshot.capabilities;
        let (supported, operation) = match self {
            Self::Load {
                load_session,
                source,
                options,
            } => {
                if *load_session <= snapshot.load_session {
                    return Err(MediaError::invalid(
                        "load session must advance monotonically",
                    ));
                }
                let empty = match source {
                    MediaSource::Uri(uri) => uri.trim().is_empty(),
                    MediaSource::File(path) => path.as_os_str().is_empty(),
                    MediaSource::Bytes(bytes) => bytes.is_empty(),
                };
                if empty || !valid_volume(options.volume) || !valid_rate(options.playback_rate) {
                    return Err(MediaError::invalid("invalid media source or load options"));
                }
                (true, "load")
            }
            Self::Play => (capabilities.play, "play"),
            Self::Pause => (capabilities.pause, "pause"),
            Self::Stop => (capabilities.stop, "stop"),
            Self::Seek(position) => {
                if let Some(range) = snapshot.seek_window() {
                    if range.clamp(*position) != *position {
                        return Err(MediaError::invalid("seek outside available window"));
                    }
                    (true, "seek")
                } else {
                    (false, "seek")
                }
            }
            Self::SetVolume(volume) => {
                if !valid_volume(*volume) {
                    return Err(MediaError::invalid("volume must be finite and in 0..=1"));
                }
                (capabilities.volume, "volume")
            }
            Self::SetMuted(_) => (capabilities.mute, "mute"),
            Self::SetPlaybackRate(rate) => {
                if !valid_rate(*rate) {
                    return Err(MediaError::invalid(
                        "playback rate must be finite and positive",
                    ));
                }
                (capabilities.playback_rate, "playback rate")
            }
            Self::SetLoop(_) => (capabilities.looping, "loop"),
        };
        if supported {
            Ok(())
        } else {
            Err(MediaError::unsupported(operation))
        }
    }
}

fn valid_volume(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
fn valid_rate(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaEvent {
    pub load_session: u64,
    pub kind: MediaEventKind,
}

impl MediaEvent {
    pub fn new(load_session: u64, kind: MediaEventKind) -> Self {
        Self { load_session, kind }
    }
}

/// Payload of a session-tagged observation. Untagged events cannot be applied.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaEventKind {
    Snapshot(MediaSnapshot),
    StateChanged {
        state: PlaybackState,
        play_when_ready: bool,
    },
    /// Coalesce steady playback positions; see RECOMMENDED_POSITION_EVENT_INTERVAL.
    PositionChanged(Duration),
    DurationChanged(Option<Duration>),
    BufferedChanged(Vec<MediaTimeRange>),
    SeekableChanged(Option<MediaTimeRange>),
    VolumeChanged(f32),
    MutedChanged(bool),
    PlaybackRateChanged(f32),
    LoopChanged(bool),
    MetadataChanged(MediaMetadata),
    CapabilitiesChanged(MediaCapabilities),
    /// None detaches the frame key on the next UI event/render pass. Idle also detaches it.
    VideoTextureChanged(Option<String>),
    VideoSizeChanged(Option<MediaVideoSize>),
    Error(MediaError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaErrorKind {
    Unsupported,
    InvalidInput,
    Backend,
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaError {
    pub kind: MediaErrorKind,
    pub message: String,
}

impl MediaError {
    pub fn new(kind: MediaErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(MediaErrorKind::InvalidInput, message)
    }
    pub fn unsupported(operation: &str) -> Self {
        Self::new(
            MediaErrorKind::Unsupported,
            format!("media operation unavailable: {operation}"),
        )
    }
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for MediaError {}

/// Implemented and driven by external libraries. Jadix never polls or owns an adapter.
///
/// Dispatch enqueues work without blocking a UI thread. Polling is nonblocking.
/// Copy the originating Load token into every event, including asynchronous errors,
/// frames and full snapshots. Capture it per job, never read the current token at completion.
/// The adapter owns cancellation, shutdown, and coalescing steady PositionChanged
/// events to RECOMMENDED_POSITION_EVENT_INTERVAL (200 ms; typical range 100–250 ms).
/// State changes and seek completion are immediate; frame cadence is independent.
pub trait MediaBackend {
    fn dispatch(&mut self, command: MediaCommand) -> Result<(), MediaError>;
    fn poll_event(&mut self) -> Option<MediaEvent>;
}
