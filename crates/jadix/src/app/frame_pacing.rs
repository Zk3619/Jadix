use std::cell::Cell;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

thread_local! {
    static FRAME_RATE_LIMIT: Cell<Option<NonZeroU32>> = const { Cell::new(NonZeroU32::new(120)) };
}

/// Limit redraws per window on this UI thread (default: 120 frames/s).
///
/// Input and UI tasks still run immediately. Only painting is coalesced, and the
/// event loop sleeps until the next frame or input. Use `None` for display-paced
/// rendering, or `NonZeroU32::new(120)` for a 120 FPS limit. Call on the UI thread.
pub fn set_frame_rate_limit(fps: Option<NonZeroU32>) {
    FRAME_RATE_LIMIT.with(|limit| limit.set(fps));
    super::runtime::request_redraw_all();
}

pub(super) fn frame_interval() -> Option<Duration> {
    FRAME_RATE_LIMIT.with(|limit| {
        limit
            .get()
            .map(|fps| Duration::from_secs_f64(1.0 / f64::from(fps.get())))
    })
}

#[derive(Default)]
pub(super) struct FramePacer {
    last_frame: Option<Instant>,
    pub pending: bool,
}

impl FramePacer {
    pub fn deadline(&self, now: Instant, interval: Option<Duration>) -> Instant {
        match (self.last_frame, interval) {
            (Some(last), Some(interval)) => last + interval,
            _ => now,
        }
    }

    pub fn begin_frame(&mut self, now: Instant, interval: Option<Duration>) -> bool {
        let deadline = self.deadline(now, interval);
        if deadline > now {
            self.pending = true;
            return false;
        }
        self.pending = false;
        // Preserve the cadence through small timer delays instead of adding
        // wake-up latency to every period. Rebase after idle / a missed period
        // so slow frames never produce a burst of catch-up paints.
        self.last_frame = Some(match interval {
            Some(period) if now.duration_since(deadline) < period => deadline,
            _ => now,
        });
        true
    }
}

#[cfg(test)]
#[path = "tests/frame_pacing.rs"]
mod tests;
