use std::cell::{Cell, RefCell};
use std::time::Instant;

thread_local! {
    static PREFERS_REDUCED_MOTION: Cell<Option<bool>> = const { Cell::new(None) };
}

/// After `ControlFlow::Wait` or a hitch, Instant can jump past `duration` /
/// a spring's start. Cap **only** those idle jumps (see
/// the host duration-tween adapter); normal frames keep wall-clock progress so
/// 30/60 Hz does not crawl.
const CSS_TRANSITION_DEFAULT_MAX_STEP_MS: f32 = 1000.0 / 60.0;

thread_local! {
    /// Monotonic origin for the document CSS-transition timeline (`performance.now()`-like).
    static CSS_TIMELINE_ORIGIN: RefCell<Option<Instant>> = const { RefCell::new(None) };
    /// When `Some`, overrides [`css_timeline_now_ms`] (unit tests / virtual clock).
    static CSS_TIMELINE_VIRTUAL_MS: Cell<Option<f64>> = const { Cell::new(None) };
    /// `None` = uncapped (virtual-timeline tests that seek exactly). `Some` = max ms/tick.
    static CSS_TRANSITION_MAX_STEP_MS: Cell<Option<f32>> =
        const { Cell::new(Some(CSS_TRANSITION_DEFAULT_MAX_STEP_MS)) };
}

pub fn css_timeline_now_ms() -> f64 {
    if let Some(ms) = CSS_TIMELINE_VIRTUAL_MS.get() {
        return ms.max(0.0);
    }
    CSS_TIMELINE_ORIGIN.with(|slot| {
        let mut g = slot.borrow_mut();
        let origin = *g.get_or_insert_with(Instant::now);
        origin.elapsed().as_secs_f64() * 1000.0
    })
}

/// Drive CSS transitions from a virtual monotonic timeline (unit tests).
///
/// When enabled, time only moves via [`set_css_transition_timeline_ms`] /
/// [`advance_css_transition_timeline_ms`] — not via wall `SystemTime` and not by
/// inventing progress from frame `dt` storms.
pub fn set_css_transition_virtual_timeline(on: bool) {
    if on {
        CSS_TIMELINE_VIRTUAL_MS.set(Some(CSS_TIMELINE_VIRTUAL_MS.get().unwrap_or(0.0).max(0.0)));
        // Exact seeks in tests; production Instant idle uses the default cap.
        CSS_TRANSITION_MAX_STEP_MS.set(None);
    } else {
        CSS_TIMELINE_VIRTUAL_MS.set(None);
        CSS_TRANSITION_MAX_STEP_MS.set(Some(CSS_TRANSITION_DEFAULT_MAX_STEP_MS));
    }
}

/// Cap duration-tween Instant delta per tick (`None` = uncapped).
pub fn set_css_transition_max_step_ms(ms: Option<f32>) {
    CSS_TRANSITION_MAX_STEP_MS.set(ms.map(|v| v.max(0.0)));
}

/// Seek the virtual CSS timeline to an absolute monotonic time (ms since origin).
pub fn set_css_transition_timeline_ms(ms: f64) {
    CSS_TIMELINE_VIRTUAL_MS.set(Some(ms.max(0.0)));
}

/// Advance the virtual CSS timeline by `delta_ms` (tests / scripted frames).
pub fn advance_css_transition_timeline_ms(delta_ms: f64) {
    let cur = CSS_TIMELINE_VIRTUAL_MS.get().unwrap_or(0.0);
    CSS_TIMELINE_VIRTUAL_MS.set(Some((cur + delta_ms).max(0.0)));
}

/// Compatibility alias: enable virtual timeline at `t = 0` (or disable).
pub fn set_css_transition_synthetic_clock(on: bool) {
    set_css_transition_virtual_timeline(on);
    if on {
        set_css_transition_timeline_ms(0.0);
    }
}

/// Override `prefers-reduced-motion` (tests / settings). `None` restores auto-detect.
pub fn set_prefers_reduced_motion(on: Option<bool>) {
    PREFERS_REDUCED_MOTION.set(on);
}

/// CSS `prefers-reduced-motion: reduce` — durations snap to the end of the interval.
pub fn prefers_reduced_motion() -> bool {
    if !cfg!(feature = "animation-base") {
        return true;
    }
    if let Some(v) = PREFERS_REDUCED_MOTION.get() {
        return v;
    }
    match std::env::var("JADIX_REDUCED_MOTION") {
        Ok(s) if s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("reduce") => {
            true
        }
        _ => false,
    }
}

pub fn css_transition_max_step_ms() -> Option<f32> {
    CSS_TRANSITION_MAX_STEP_MS.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_clock_seek_advance_and_cap_are_shared() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(12.0);
        advance_css_transition_timeline_ms(8.0);
        assert_eq!(css_timeline_now_ms(), 20.0);
        assert_eq!(css_transition_max_step_ms(), None);
        set_css_transition_max_step_ms(Some(-4.0));
        assert_eq!(css_transition_max_step_ms(), Some(0.0));
        set_css_transition_synthetic_clock(true);
        assert_eq!(css_timeline_now_ms(), 0.0);
        set_css_transition_virtual_timeline(false);
        assert_eq!(css_transition_max_step_ms(), Some(1000.0 / 60.0));
    }

    #[test]
    fn reduced_motion_override_respects_disabled_animation() {
        set_prefers_reduced_motion(Some(false));
        assert_eq!(prefers_reduced_motion(), !cfg!(feature = "animation-base"));
        set_prefers_reduced_motion(Some(true));
        assert!(prefers_reduced_motion());
        set_prefers_reduced_motion(None);
    }
}
