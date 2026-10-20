//! UI-thread bridge between external media adapters and retained media controls.

use crate::core::reactivity::Signal;
pub use jadix_media::*;
use std::{cell::Cell, rc::Rc};

/// A controlled UI binding. Clones share state and the command sink.
///
/// Create it under the application owner, outside transient views. The sink must
/// enqueue work promptly. Marshal backend events onto the UI thread before publish.
#[derive(Clone)]
pub struct MediaController {
    snapshot: Signal<MediaSnapshot>,
    request_error: Signal<Option<MediaError>>,
    issued_session: Rc<Cell<u64>>,
    command: Rc<dyn Fn(MediaCommand) -> Result<(), MediaError>>,
}

impl MediaController {
    pub fn new(
        snapshot: Signal<MediaSnapshot>,
        command: impl Fn(MediaCommand) -> Result<(), MediaError> + 'static,
    ) -> Self {
        let issued_session = snapshot
            .try_peek()
            .map_or(0, |snapshot| snapshot.load_session);
        Self {
            snapshot,
            request_error: Signal::new(None),
            issued_session: Rc::new(Cell::new(issued_session)),
            command: Rc::new(command),
        }
    }

    /// An inert binding: controls remain disabled until connected to an adapter.
    pub fn disconnected() -> Self {
        Self::new(Signal::new(MediaSnapshot::default()), |_| {
            Err(MediaError::new(
                MediaErrorKind::Disconnected,
                "no media adapter connected",
            ))
        })
    }

    pub fn snapshot(&self) -> Signal<MediaSnapshot> {
        self.snapshot
    }
    pub fn request_error(&self) -> Signal<Option<MediaError>> {
        self.request_error
    }

    pub fn current(&self) -> MediaSnapshot {
        self.snapshot.try_peek().unwrap_or_default().normalized()
    }

    /// Allocate a fresh Load token shared by controller clones and enqueue the request.
    pub fn load(
        &self,
        source: impl Into<MediaSource>,
        options: MediaLoadOptions,
    ) -> Result<u64, MediaError> {
        let load_session = self
            .issued_session
            .get()
            .max(self.current().load_session)
            .checked_add(1)
            .ok_or_else(|| MediaError::invalid("media load session exhausted"))?;
        self.request(MediaCommand::Load {
            load_session,
            source: source.into(),
            options,
        })?;
        Ok(load_session)
    }

    /// Playback controls stay authoritative. Load fences and detaches the previous source
    /// before dispatch; a rejected, still-unobserved load restores the previous snapshot.
    pub fn request(&self, command: MediaCommand) -> Result<(), MediaError> {
        let result = if !self.snapshot.is_alive() {
            Err(MediaError::new(
                MediaErrorKind::Disconnected,
                "media owner was disposed",
            ))
        } else {
            command
                .validate(&self.current())
                .and_then(|()| self.dispatch_validated(command))
        };
        self.request_error.try_set(result.as_ref().err().cloned());
        result
    }

    fn dispatch_validated(&self, command: MediaCommand) -> Result<(), MediaError> {
        let MediaCommand::Load {
            load_session,
            source,
            ..
        } = &command
        else {
            return (self.command)(command);
        };
        let load_session = *load_session;
        if load_session <= self.issued_session.get() {
            return Err(MediaError::invalid("media load token was already issued"));
        }
        self.issued_session.set(load_session);
        let previous = self.current();
        let mut pending = previous.clone();
        pending.begin_load(load_session, source.clone());
        self.snapshot.try_set(pending.clone());
        if self.current().load_session != load_session {
            return Err(MediaError::invalid("load was superseded before dispatch"));
        }
        let result = (self.command)(command);
        if result.is_err() && self.snapshot.try_peek().as_ref() == Some(&pending) {
            self.snapshot.try_set(previous);
        }
        crate::app::runtime::request_redraw();
        result
    }

    /// Publish on the UI thread. Stale, future and unchanged observations return false
    /// without writing the Signal or requesting another redraw.
    pub fn publish(&self, event: MediaEvent) -> bool {
        let Some(mut snapshot) = self.snapshot.try_peek() else {
            return false;
        };
        let previous = snapshot.clone();
        if !snapshot.apply(event) || snapshot == previous {
            return false;
        }
        let changed = self.snapshot.try_set(snapshot);
        crate::app::runtime::request_redraw();
        changed
    }

    pub fn toggle_playback(&self) -> Result<(), MediaError> {
        self.request(if self.current().wants_playback() {
            MediaCommand::Pause
        } else {
            MediaCommand::Play
        })
    }

    pub fn seek_by(&self, seconds: f64) -> Result<(), MediaError> {
        if !seconds.is_finite() {
            return Err(MediaError::invalid("seek delta must be finite"));
        }
        let snapshot = self.current();
        let range = snapshot
            .seek_window()
            .ok_or_else(|| MediaError::unsupported("seek"))?;
        let seconds = (snapshot.position.as_secs_f64() + seconds)
            .clamp(range.start().as_secs_f64(), range.end().as_secs_f64());
        let position = std::time::Duration::try_from_secs_f64(seconds)
            .map_err(|_| MediaError::invalid("seek position is out of range"))?;
        self.request(MediaCommand::Seek(range.clamp(position)))
    }
}

impl Default for MediaController {
    fn default() -> Self {
        Self::disconnected()
    }
}
