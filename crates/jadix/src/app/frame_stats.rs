//! Per-frame collect / present counters (GPUI `Entity::cached` observability analog).
//!
//! Paint-only frames can skip clean siblings by replaying a fingerprint display list
//! ([`crate::layout::collect_cache`]). Structure / layout still FullClear GPU present.
//! Scroll invalidates the viewport layer (scissor Clear), not the whole window.
//!
//! Timing and per-window presentation APIs require the optional `performance` feature.
//!
//! Enable a `log::info` line with `JADIX_FRAME_STATS=1` (`--features runtime-logging`).

use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "performance")]
pub use super::frame_budget::{
    enable_application_frame_timings, take_application_frame_timings, ApplicationFrameSample,
    ApplicationFrameTimings,
};

/// Upload work for retained rect, textured-instance and polygon-clip streams
/// on the most recent render attempt on this UI thread (including skipped presents).
/// Excludes glyph atlases, images, uniforms and temporary effect/capture buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneUploads {
    pub source_bytes: u64,
    pub uploaded_bytes: u64,
    pub write_calls: u64,
    pub reallocations: u64,
}

thread_local! {
    static UPLOADS: std::cell::Cell<SceneUploads> = const { std::cell::Cell::new(SceneUploads {
        source_bytes: 0, uploaded_bytes: 0, write_calls: 0, reallocations: 0,
    }) };
}

pub fn last_scene_uploads() -> SceneUploads {
    UPLOADS.with(|s| s.get())
}

pub(crate) fn begin_scene_uploads() {
    UPLOADS.with(|s| s.set(SceneUploads::default()));
}

pub(crate) fn note_scene_upload(stats: SceneUploads) {
    UPLOADS.with(|s| {
        let mut total = s.get();
        total.source_bytes += stats.source_bytes;
        total.uploaded_bytes += stats.uploaded_bytes;
        total.write_calls += stats.write_calls;
        total.reallocations += stats.reallocations;
        s.set(total);
    });
}

pub(crate) fn log_scene_uploads(presented: bool) {
    if frame_stats_log_enabled() {
        let s = last_scene_uploads();
        log::info!(
            "[jadix] scene_uploads presented={presented} source_bytes={} uploaded_bytes={} writes={} reallocations={}",
            s.source_bytes,
            s.uploaded_bytes,
            s.write_calls,
            s.reallocations
        );
    }
}

#[cfg(feature = "performance")]
/// Render-call work, including surface acquire/submit/present waits.
/// Excludes event dispatch and post-render IME/semantics; use application timings
/// for the complete event-loop CPU budget. Neither API measures GPU execution.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    pub frames: u64,
    pub total_ms: f64,
    pub worst_ms: f64,
    pub layout_ms: f64,
    pub text_ms: f64,
}

#[cfg(feature = "performance")]
thread_local! {
    static TIMINGS: std::cell::RefCell<Option<std::collections::HashMap<winit::window::WindowId, FrameTimings>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(feature = "performance")]
/// Enable inexpensive per-window timing accumulation on this UI thread.
pub fn enable_frame_timings(enabled: bool) {
    TIMINGS.with(|t| *t.borrow_mut() = enabled.then(std::collections::HashMap::new));
}

#[cfg(feature = "performance")]
/// Drain all windows' render-call work. Reading never requests a frame.
pub fn take_frame_timings() -> FrameTimings {
    TIMINGS.with(|t| {
        let mut total = FrameTimings::default();
        if let Some(windows) = t.borrow_mut().as_mut() {
            for (_, sample) in windows.drain() {
                total.frames += sample.frames;
                total.total_ms += sample.total_ms;
                total.worst_ms = total.worst_ms.max(sample.worst_ms);
                total.layout_ms += sample.layout_ms;
                total.text_ms += sample.text_ms;
            }
        }
        total
    })
}

#[cfg(feature = "performance")]
/// Drain only this window. Other windows' samples remain available.
pub fn take_window_frame_timings(window: winit::window::WindowId) -> FrameTimings {
    TIMINGS.with(|t| {
        t.borrow_mut()
            .as_mut()
            .and_then(|windows| windows.remove(&window))
            .unwrap_or_default()
    })
}

#[cfg(feature = "performance")]
pub(crate) fn note_frame_timings(
    window: winit::window::WindowId,
    total_ms: f64,
    layout_ms: f64,
    text_ms: f64,
) {
    TIMINGS.with(|t| {
        if let Some(windows) = t.borrow_mut().as_mut() {
            let sample = windows.entry(window).or_default();
            sample.frames += 1;
            sample.total_ms += total_ms;
            sample.worst_ms = sample.worst_ms.max(total_ms);
            sample.layout_ms += layout_ms;
            sample.text_ms += text_ms;
        }
    });
}

static SKIPPED_SIBLINGS: AtomicU64 = AtomicU64::new(0);
static PATCHED_DL: AtomicU64 = AtomicU64::new(0);
static LAST_SKIPPED: AtomicU64 = AtomicU64::new(0);
static LAST_PATCHED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "performance")]
static PRESENTED_FRAMES: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "performance")]
/// Cumulative surface frames actually submitted for presentation (all windows).
/// Redraw requests that skip presentation do not increment this counter.
pub fn presented_frames_count() -> u64 {
    PRESENTED_FRAMES.load(Ordering::Relaxed)
}

#[cfg(feature = "performance")]
/// Actual surface-presentation requests from one window over the last second.
/// Includes partial paints and HUD paints. This does not measure display scanout.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowPresentationStats {
    pub frames: u64,
    pub animation_active: bool,
    pub sample_frames: u64,
    pub content_sample_frames: u64,
    pub sample_seconds: f64,
    /// Actual presents / elapsed wall time; zero when no frames were presented.
    pub fps: Option<f64>,
    /// Presents excluding a presentation caused solely by the performance HUD update.
    pub content_fps: Option<f64>,
}

#[cfg(feature = "performance")]
#[derive(Clone, Copy)]
struct Presentation {
    at: std::time::Instant,
    content: bool,
}

#[cfg(feature = "performance")]
#[derive(Default)]
struct WindowPresentationClock {
    frames: u64,
    started: Option<std::time::Instant>,
    continuous: bool,
    overlay_update_pending: bool,
    presents: std::collections::VecDeque<Presentation>,
}

#[cfg(feature = "performance")]
impl WindowPresentationClock {
    const SAMPLE_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);

    fn note(&mut self, now: std::time::Instant) {
        self.frames += 1;
        self.started.get_or_insert(now);
        self.presents.push_back(Presentation {
            at: now,
            content: !self.overlay_update_pending,
        });
        self.overlay_update_pending = false;
        while self
            .presents
            .front()
            .is_some_and(|present| now.saturating_duration_since(present.at) >= Self::SAMPLE_WINDOW)
        {
            self.presents.pop_front();
        }
    }

    fn sample(&self, now: std::time::Instant) -> WindowPresentationStats {
        let elapsed = self
            .started
            .map(|start| now.saturating_duration_since(start))
            .unwrap_or_default()
            .min(Self::SAMPLE_WINDOW);
        let seconds = elapsed.as_secs_f64();
        // The first presentation is the initial baseline, matching a counter delta.
        let sample_frames = self
            .presents
            .iter()
            .filter(|present| {
                self.started.is_some_and(|start| present.at > start)
                    && now.saturating_duration_since(present.at) < elapsed
            })
            .count() as u64;
        let content_sample_frames = self
            .presents
            .iter()
            .filter(|present| {
                present.content
                    && self.started.is_some_and(|start| present.at > start)
                    && now.saturating_duration_since(present.at) < elapsed
            })
            .count() as u64;
        WindowPresentationStats {
            frames: self.frames,
            animation_active: self.continuous,
            sample_frames,
            content_sample_frames,
            sample_seconds: seconds,
            fps: (seconds > 0.0).then(|| sample_frames as f64 / seconds),
            content_fps: (seconds > 0.0).then(|| content_sample_frames as f64 / seconds),
        }
    }
}

#[cfg(feature = "performance")]
thread_local! {
    static WINDOW_PRESENTS: std::cell::RefCell<std::collections::HashMap<winit::window::WindowId, WindowPresentationClock>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

#[cfg(feature = "performance")]
/// Per-window presentation cadence. Reading does not request a redraw.
pub fn window_presentation_stats(window: winit::window::WindowId) -> WindowPresentationStats {
    WINDOW_PRESENTS.with(|counts| {
        counts
            .borrow()
            .get(&window)
            .map(|clock| clock.sample(std::time::Instant::now()))
            .unwrap_or_default()
    })
}

#[cfg(feature = "performance")]
/// Record scheduling metadata. Frame rate never depends on animation flags.
pub(crate) fn note_window_animation_activity(window: winit::window::WindowId, active: bool) {
    WINDOW_PRESENTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        let clock = counts.entry(window).or_default();
        clock.continuous = active;
    });
}

#[cfg(not(feature = "performance"))]
pub(crate) fn note_window_animation_activity(_window: winit::window::WindowId, _active: bool) {}

#[cfg(feature = "performance")]
/// Exclude the next presentation caused by refreshing the diagnostic HUD from the
/// content cadence. The total surface-present count remains unchanged.
pub fn note_performance_overlay_update(window: winit::window::WindowId) {
    WINDOW_PRESENTS.with(|counts| {
        counts
            .borrow_mut()
            .entry(window)
            .or_default()
            .overlay_update_pending = true;
    });
}

#[cfg(feature = "performance")]
/// Cumulative frames submitted for presentation by this window, including overlays.
/// Skipped redraws and frames from other windows are excluded.
pub fn window_presented_frames_count(window: winit::window::WindowId) -> u64 {
    WINDOW_PRESENTS.with(|counts| {
        counts
            .borrow()
            .get(&window)
            .map(|clock| clock.frames)
            .unwrap_or(0)
    })
}

pub(crate) fn forget_window_presentations(_window: winit::window::WindowId) {
    #[cfg(feature = "performance")]
    {
        let _ = take_window_frame_timings(_window);
        WINDOW_PRESENTS.with(|counts| counts.borrow_mut().remove(&_window));
    }
}

pub(crate) fn note_presented_frame(_window: winit::window::WindowId) {
    #[cfg(feature = "performance")]
    {
        let window = _window;
        WINDOW_PRESENTS.with(|counts| {
            counts
                .borrow_mut()
                .entry(window)
                .or_default()
                .note(std::time::Instant::now())
        });
        super::frame_budget::note_presented();
        PRESENTED_FRAMES.fetch_add(1, Ordering::Relaxed);
    }
}

/// Cumulative clean-subtree collect skips (replayed previous-frame ops).
pub fn skipped_siblings_count() -> u64 {
    SKIPPED_SIBLINGS.load(Ordering::Relaxed)
}

/// Cumulative dirty-subtree display-list patches (hash miss while skip was allowed).
pub fn patched_dl_count() -> u64 {
    PATCHED_DL.load(Ordering::Relaxed)
}

/// Skips recorded on the most recent collect that called [`note_collect_skip`].
pub fn last_skipped_siblings() -> u64 {
    LAST_SKIPPED.load(Ordering::Relaxed)
}

/// Patches recorded on the most recent collect that called [`note_collect_skip`].
pub fn last_patched_dl() -> u64 {
    LAST_PATCHED.load(Ordering::Relaxed)
}

/// Reset Gallery / test counters (does not clear last-frame snapshot).
pub fn reset_collect_skip_counters() {
    SKIPPED_SIBLINGS.store(0, Ordering::Relaxed);
    PATCHED_DL.store(0, Ordering::Relaxed);
    LAST_SKIPPED.store(0, Ordering::Relaxed);
    LAST_PATCHED.store(0, Ordering::Relaxed);
}

/// Record one collect pass. `skipped` = replayed sibling subtrees; `patched` = recollected
/// subtrees that had a stale cache (skip allowed).
pub fn note_collect_skip(skipped: u32, patched: u32) {
    LAST_SKIPPED.store(skipped as u64, Ordering::Relaxed);
    LAST_PATCHED.store(patched as u64, Ordering::Relaxed);
    if skipped > 0 {
        SKIPPED_SIBLINGS.fetch_add(skipped as u64, Ordering::Relaxed);
    }
    if patched > 0 {
        PATCHED_DL.fetch_add(patched as u64, Ordering::Relaxed);
    }
    if frame_stats_log_enabled() && (skipped > 0 || patched > 0) {
        log::info!(
            "[jadix] frame_stats skipped_siblings={skipped} patched_dl={patched} \
             total_skip={} total_patch={}",
            skipped_siblings_count(),
            patched_dl_count()
        );
    }
}

fn frame_stats_log_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("JADIX_FRAME_STATS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// GPUI-like present buckets (pipeline kind, not z-reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneKind {
    Quad,
    Surface,
    Underline,
    Text,
    ShadowTex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneBatches {
    pub quad: u64,
    pub surface: u64,
    pub underline: u64,
    pub text: u64,
    pub shadow_tex: u64,
}

static SCENE_QUAD: AtomicU64 = AtomicU64::new(0);
static SCENE_SURFACE: AtomicU64 = AtomicU64::new(0);
static SCENE_UNDERLINE: AtomicU64 = AtomicU64::new(0);
static SCENE_TEXT: AtomicU64 = AtomicU64::new(0);
static SCENE_SHADOW: AtomicU64 = AtomicU64::new(0);

pub fn begin_scene_batches() {
    SCENE_QUAD.store(0, Ordering::Relaxed);
    SCENE_SURFACE.store(0, Ordering::Relaxed);
    SCENE_UNDERLINE.store(0, Ordering::Relaxed);
    SCENE_TEXT.store(0, Ordering::Relaxed);
    SCENE_SHADOW.store(0, Ordering::Relaxed);
}

pub fn note_scene_batch(kind: SceneKind, count: u32) {
    if count == 0 {
        return;
    }
    let slot = match kind {
        SceneKind::Quad => &SCENE_QUAD,
        SceneKind::Surface => &SCENE_SURFACE,
        SceneKind::Underline => &SCENE_UNDERLINE,
        SceneKind::Text => &SCENE_TEXT,
        SceneKind::ShadowTex => &SCENE_SHADOW,
    };
    slot.fetch_add(1, Ordering::Relaxed);
}

pub fn last_scene_batches() -> SceneBatches {
    SceneBatches {
        quad: SCENE_QUAD.load(Ordering::Relaxed),
        surface: SCENE_SURFACE.load(Ordering::Relaxed),
        underline: SCENE_UNDERLINE.load(Ordering::Relaxed),
        text: SCENE_TEXT.load(Ordering::Relaxed),
        shadow_tex: SCENE_SHADOW.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
#[path = "tests/frame_stats.rs"]
mod tests;
