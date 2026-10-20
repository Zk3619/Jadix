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
mod tests {
    use super::*;

    #[test]
    fn default_is_120_fps_and_can_be_overridden() {
        assert_eq!(frame_interval(), Some(Duration::from_secs_f64(1.0 / 120.0)));
        set_frame_rate_limit(NonZeroU32::new(90));
        assert_eq!(frame_interval(), Some(Duration::from_secs_f64(1.0 / 90.0)));
        set_frame_rate_limit(None);
        assert_eq!(frame_interval(), None);
        set_frame_rate_limit(NonZeroU32::new(120));
    }

    #[test]
    fn late_wakes_do_not_accumulate_into_a_lower_frame_rate() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        let period = Duration::from_secs_f64(1.0 / 60.0);
        let interval = Some(period);
        assert!(pacer.begin_frame(start, interval));
        for frame in 1..=120 {
            let wake = pacer.deadline(start, interval) + Duration::from_micros(500);
            assert!(pacer.begin_frame(wake, interval));
            assert_eq!(pacer.deadline(wake, interval), start + period * (frame + 1));
        }
        // After idle or a full missed frame, resume without a burst of catch-up paints.
        let resume = start + Duration::from_secs(10);
        assert!(pacer.begin_frame(resume, interval));
        assert_eq!(pacer.deadline(resume, interval), resume + period);
        assert!(!pacer.begin_frame(resume, interval));
    }

    #[test]
    fn coalesces_until_deadline_without_postponing_it_on_input() {
        let mut pacer = FramePacer::default();
        let start = Instant::now();
        let interval = Some(Duration::from_secs_f64(1.0 / 60.0));
        assert!(pacer.begin_frame(start, interval));
        for ms in 1..16 {
            let now = start + Duration::from_millis(ms);
            assert!(!pacer.begin_frame(now, interval));
            assert_eq!(pacer.deadline(now, interval), start + interval.unwrap());
            assert!(pacer.pending);
        }
        assert!(pacer.begin_frame(start + interval.unwrap(), interval));
        assert!(!pacer.pending);
    }

    #[test]
    fn idle_has_no_wake_and_windows_have_independent_budgets() {
        let now = Instant::now();
        let mut a = FramePacer::default();
        let mut b = FramePacer::default();
        let interval = Some(Duration::from_millis(16));
        assert!(!a.pending);
        assert!(a.begin_frame(now, interval));
        assert!(b.begin_frame(now, interval));
        assert!(!a.pending && !b.pending);
        assert!(a.begin_frame(now, None));
        assert!(!a.begin_frame(now, interval));
        assert!(
            a.begin_frame(now, None),
            "removing the limit releases pending paint"
        );
    }
}
