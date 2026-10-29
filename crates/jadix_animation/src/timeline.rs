//! Unified animation timeline (duration / easing / tracks). Complements [`Spring`] / [`CubicTween`].

use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::time::Instant;

use super::animation::{cubic_bezier_y_at_x, CubicTween};
use super::generator::cycle_duration_clock;

thread_local! {
    static CLOCK: RefCell<Option<Instant>> = const { RefCell::new(None) };
    #[cfg(all(any(test, feature = "test-hooks"), target_os = "windows"))]
    static TEST_TIME: std::cell::Cell<Option<f32>> = const { std::cell::Cell::new(None) };
    static TIMELINES: RefCell<Vec<Weak<RefCell<TimelineInner>>>> = const { RefCell::new(Vec::new()) };
}

fn now() -> Instant {
    CLOCK.with(|c| *c.borrow_mut().get_or_insert_with(Instant::now))
}

/// Same variants as [`RepeatMode`]; timeline clock and `AnimatedNumber` share one enum.
pub use super::generator::RepeatMode as Repeat;

#[derive(Clone, Debug)]
pub struct TimelineInner {
    pub duration_s: f32,
    pub elapsed_s: f32,
    pub playing: bool,
    pub repeat: Repeat,
    pub reverse: bool,
    /// cubic-bezier control points (CSS `ease` default).
    pub bezier: [f32; 4],
}

#[derive(Clone)]
pub struct Timeline(Rc<RefCell<TimelineInner>>);

impl Timeline {
    pub fn new(duration_s: f32) -> Self {
        let duration_s = duration_s.max(1e-4);
        let inner = Rc::new(RefCell::new(TimelineInner {
            duration_s,
            elapsed_s: if cfg!(feature = "animation-base") {
                0.0
            } else {
                duration_s
            },
            playing: cfg!(feature = "animation-base"),
            repeat: Repeat::Once,
            reverse: false,
            bezier: [0.25, 0.1, 0.25, 1.0],
        }));
        TIMELINES.with(|t| t.borrow_mut().push(Rc::downgrade(&inner)));
        Self(inner)
    }

    pub fn ease(self, x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        self.0.borrow_mut().bezier = [x1, y1, x2, y2];
        self
    }

    pub fn repeat(self, repeat: Repeat) -> Self {
        self.0.borrow_mut().repeat = repeat;
        self
    }

    pub fn play(&self) {
        let mut state = self.0.borrow_mut();
        state.playing = cfg!(feature = "animation-base");
        if !state.playing {
            state.elapsed_s = state.duration_s;
        }
    }

    pub fn pause(&self) {
        self.0.borrow_mut().playing = false;
    }

    pub fn seek(&self, t: f32) {
        let mut g = self.0.borrow_mut();
        g.elapsed_s = (t * g.duration_s).clamp(0.0, g.duration_s);
    }

    /// Linear 0..=1 time (before easing).
    pub fn linear(&self) -> f32 {
        let g = self.0.borrow();
        (g.elapsed_s / g.duration_s).clamp(0.0, 1.0)
    }

    /// Eased 0..=1 progress.
    pub fn progress(&self) -> f32 {
        let g = self.0.borrow();
        let x = (g.elapsed_s / g.duration_s).clamp(0.0, 1.0);
        let x = if g.reverse { 1.0 - x } else { x };
        cubic_bezier_y_at_x(x, g.bezier[0], g.bezier[1], g.bezier[2], g.bezier[3])
    }
}

/// Tick all live timelines. Returns true if any is still playing.
pub fn tick_timelines(dt: f32) -> bool {
    if !cfg!(feature = "animation-base") {
        return false;
    }
    let _ = now();
    let mut any = false;
    TIMELINES.with(|slot| {
        let mut v = slot.borrow_mut();
        v.retain(|w| w.upgrade().is_some());
        for w in v.iter() {
            let Some(rc) = w.upgrade() else {
                continue;
            };
            let mut g = rc.borrow_mut();
            if !g.playing {
                continue;
            }
            any = true;
            g.elapsed_s += dt.max(0.0);
            let (elapsed, reverse, playing) =
                cycle_duration_clock(g.elapsed_s, g.duration_s, g.repeat, g.reverse);
            g.elapsed_s = elapsed;
            g.reverse = reverse;
            g.playing = playing;
        }
    });
    any
}

/// Whether any timeline is still playing (does not advance time).
pub fn any_timeline_playing() -> bool {
    if !cfg!(feature = "animation-base") {
        return false;
    }
    TIMELINES.with(|slot| {
        slot.borrow()
            .iter()
            .any(|w| w.upgrade().is_some_and(|rc| rc.borrow().playing))
    })
}
pub fn global_time_s() -> f32 {
    #[cfg(all(any(test, feature = "test-hooks"), target_os = "windows"))]
    if let Some(time) = TEST_TIME.with(|clock| clock.get()) {
        return time;
    }
    CLOCK.with(|c| {
        c.borrow()
            .map(|t0| t0.elapsed().as_secs_f32())
            .unwrap_or(0.0)
    })
}

#[cfg(all(any(test, feature = "test-hooks"), target_os = "windows"))]
pub fn set_global_time_for_test(time: Option<f32>) {
    TEST_TIME.with(|clock| clock.set(time));
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn keyframe(times: &[f32], values: &[f32], t: f32) -> f32 {
    if times.is_empty() || values.is_empty() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    if t <= times[0] {
        return values[0];
    }
    for i in 1..times.len().min(values.len()) {
        if t <= times[i] {
            let span = (times[i] - times[i - 1]).max(1e-6);
            let u = (t - times[i - 1]) / span;
            return lerp(values[i - 1], values[i], u);
        }
    }
    values[values.len() - 1]
}

// Keep CubicTween visible so callers can mix spring / cubic / timeline.
pub fn cubic_from_css(duration_s: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> CubicTween {
    CubicTween::new_with_curve(0.0, duration_s, (x1, y1, x2, y2))
}

#[cfg(test)]
#[path = "tests/timeline.rs"]
mod tests;
