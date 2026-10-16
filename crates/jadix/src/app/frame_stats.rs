//! Per-frame collect / present counters (GPUI `Entity::cached` observability analog).
//!
//! Paint-only frames can skip clean siblings by replaying a fingerprint display list
//! ([`crate::layout::collect_cache`]). Structure / layout still FullClear GPU present.
//! Scroll invalidates the viewport layer (scissor Clear), not the whole window.
//!
//! Enable a `log::info` line with `JADIX_FRAME_STATS=1` (`--features runtime-logging`).

use std::sync::atomic::{AtomicU64, Ordering};

static SKIPPED_SIBLINGS: AtomicU64 = AtomicU64::new(0);
static PATCHED_DL: AtomicU64 = AtomicU64::new(0);
static LAST_SKIPPED: AtomicU64 = AtomicU64::new(0);
static LAST_PATCHED: AtomicU64 = AtomicU64::new(0);
static PRESENTED_FRAMES: AtomicU64 = AtomicU64::new(0);

/// Cumulative surface frames actually submitted for presentation (all windows).
/// Redraw requests that skip presentation do not increment this counter.
pub fn presented_frames_count() -> u64 {
    PRESENTED_FRAMES.load(Ordering::Relaxed)
}

pub(crate) fn note_presented_frame() {
    PRESENTED_FRAMES.fetch_add(1, Ordering::Relaxed);
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
mod tests {
    use super::*;

    #[test]
    fn note_collect_skip_counts() {
        reset_collect_skip_counters();
        note_collect_skip(2, 1);
        note_collect_skip(3, 0);
        assert_eq!(skipped_siblings_count(), 5);
        assert_eq!(patched_dl_count(), 1);
        assert_eq!(last_skipped_siblings(), 3);
        assert_eq!(last_patched_dl(), 0);
    }
}
