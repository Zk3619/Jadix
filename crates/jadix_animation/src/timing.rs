//! Shared animation timing model (CSS `animation` / `transition` / programmatic handle).
//!
//! Pure `f(elapsed)` — no DOM timeline objects. Spring is a native driver alongside
//! duration-based easing.

use super::element_style::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, TimingFunction,
};

/// Duration / delay / fill / iterations — the engine kernel both CSS surfaces compile into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub duration_ms: f32,
    pub delay_ms: f32,
    pub end_delay_ms: f32,
    pub easing: TimingFunction,
    pub iterations: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill: AnimationFillMode,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            duration_ms: 0.0,
            delay_ms: 0.0,
            end_delay_ms: 0.0,
            easing: TimingFunction::Linear,
            iterations: AnimationIterationCount::Finite(1.0),
            direction: AnimationDirection::Normal,
            fill: AnimationFillMode::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingPhase {
    Delay,
    Active,
    EndDelay,
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimingProgress {
    pub phase: TimingPhase,
    /// 0..=1 within the current iteration after `direction`.
    pub local_t: f32,
    pub iteration: u32,
    pub finished: bool,
}

impl Timing {
    pub fn duration(mut self, ms: f32) -> Self {
        self.duration_ms = ms.max(0.0);
        self
    }

    pub fn delay(mut self, ms: f32) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn end_delay(mut self, ms: f32) -> Self {
        self.end_delay_ms = ms.max(0.0);
        self
    }

    pub fn easing(mut self, easing: TimingFunction) -> Self {
        self.easing = easing;
        self
    }

    pub fn iterations(mut self, n: f32) -> Self {
        self.iterations = if n.is_infinite() {
            AnimationIterationCount::Infinite
        } else {
            AnimationIterationCount::Finite(n.max(0.0))
        };
        self
    }

    pub fn direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn fill(mut self, fill: AnimationFillMode) -> Self {
        self.fill = fill;
        self
    }

    pub fn spring(mut self) -> Self {
        self.easing = TimingFunction::default_spring();
        self
    }

    pub fn active_duration_ms(&self) -> f32 {
        match self.iterations {
            AnimationIterationCount::Infinite => f32::INFINITY,
            AnimationIterationCount::Finite(n) => self.duration_ms.max(0.0) * n.max(0.0),
        }
    }

    /// Map elapsed time (after playbackRate) onto delay / active / endDelay / finished.
    pub fn resolve(&self, elapsed_ms: f32) -> TimingProgress {
        let after_delay = elapsed_ms - self.delay_ms;
        if after_delay < 0.0 {
            return TimingProgress {
                phase: TimingPhase::Delay,
                local_t: 0.0,
                iteration: 0,
                finished: false,
            };
        }
        let dur = self.duration_ms.max(1e-3);
        let total = match self.iterations {
            AnimationIterationCount::Infinite => None,
            AnimationIterationCount::Finite(n) => Some(n.max(0.0)),
        };
        if let Some(n) = total {
            let active_end = dur * n;
            if after_delay >= active_end {
                let last_iter = if n > 0.0 {
                    (n.ceil() as u32).saturating_sub(1)
                } else {
                    0
                };
                let local_t = direction_local_t(self.direction, last_iter, 1.0);
                if after_delay < active_end + self.end_delay_ms {
                    return TimingProgress {
                        phase: TimingPhase::EndDelay,
                        local_t,
                        iteration: last_iter,
                        finished: false,
                    };
                }
                return TimingProgress {
                    phase: TimingPhase::Finished,
                    local_t,
                    iteration: last_iter,
                    finished: true,
                };
            }
        }
        let cycle_index = (after_delay / dur).floor() as u32;
        let cycle_t = (after_delay % dur) / dur;
        TimingProgress {
            phase: TimingPhase::Active,
            local_t: direction_local_t(self.direction, cycle_index, cycle_t),
            iteration: cycle_index,
            finished: false,
        }
    }
}

pub fn direction_local_t(direction: AnimationDirection, iteration_index: u32, cycle_t: f32) -> f32 {
    let reverse = match direction {
        AnimationDirection::Normal => false,
        AnimationDirection::Reverse => true,
        AnimationDirection::Alternate => iteration_index % 2 == 1,
        AnimationDirection::AlternateReverse => iteration_index % 2 == 0,
    };
    if reverse {
        1.0 - cycle_t
    } else {
        cycle_t
    }
}

/// One spring step. Closed-form (Motion) rather than implicit Euler, so large
/// `dt` does not dump energy. Settle band stays the historical tight thresholds.
pub fn spring_step(
    pos: f32,
    vel: f32,
    target: f32,
    dt_secs: f32,
    stiffness: f32,
    damping: f32,
    mass: f32,
) -> (f32, f32, bool) {
    super::generator::spring_step_closed(pos, vel, target, dt_secs, stiffness, damping, mass)
}

/// `spring` / `spring()` / `spring(stiffness)` / `spring(k, d)` / `spring(k, d, mass)`
/// plus Motion duration form: `spring(800ms)` / `spring(800ms, 0.25)` /
/// `spring(duration 400ms, bounce 0.2)` / `spring(visual 0.3)`.
pub fn parse_spring_timing(s: &str) -> Option<TimingFunction> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    if lower == "spring" {
        return Some(TimingFunction::default_spring());
    }
    let inner = lower.strip_prefix("spring(")?.strip_suffix(')')?;
    let opts = super::generator::parse_spring_options(inner)?;
    Some(if !opts.has_physics_keys() && opts.has_duration_keys() {
        TimingFunction::SpringOptions(opts)
    } else {
        opts.resolve().to_timing_function()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_delay_active_end_delay_finish() {
        let t = Timing::default()
            .duration(100.0)
            .delay(50.0)
            .end_delay(20.0)
            .iterations(1.0);
        assert_eq!(t.resolve(10.0).phase, TimingPhase::Delay);
        assert_eq!(t.resolve(80.0).phase, TimingPhase::Active);
        assert!((t.resolve(80.0).local_t - 0.3).abs() < 1e-4);
        assert_eq!(t.resolve(160.0).phase, TimingPhase::EndDelay);
        assert!(!t.resolve(160.0).finished);
        assert!(t.resolve(180.0).finished);
    }

    #[test]
    fn spring_step_settles_at_target() {
        let mut pos = 0.0f32;
        let mut vel = 0.0f32;
        let k = TimingFunction::default_spring();
        let TimingFunction::Spring {
            stiffness,
            damping,
            mass,
        } = k
        else {
            panic!("default spring");
        };
        let mut settled = false;
        for _ in 0..240 {
            let (p, v, ok) = spring_step(pos, vel, 1.0, 1.0 / 60.0, stiffness, damping, mass);
            pos = p;
            vel = v;
            if ok {
                settled = true;
                break;
            }
        }
        assert!(settled, "spring must settle, pos={pos}");
        assert!((pos - 1.0).abs() < 1e-3);
    }

    #[test]
    fn parse_spring_variants() {
        assert!(parse_spring_timing("spring").unwrap().is_spring());
        assert!(parse_spring_timing("spring()").unwrap().is_spring());
        let TimingFunction::Spring { stiffness, .. } = parse_spring_timing("spring(90)").unwrap()
        else {
            panic!("spring(90)");
        };
        assert!((stiffness - 90.0).abs() < 1e-3);
        assert!(parse_spring_timing("ease").is_none());
        let TimingFunction::SpringOptions(opts) =
            parse_spring_timing("spring(800ms, 0.25)").unwrap()
        else {
            panic!("duration spring");
        };
        assert_eq!(opts.duration_ms, Some(800.0));
        let physics = opts.resolve();
        assert!(physics.stiffness > 10.0 && physics.damping > 1.0);
        assert!(parse_spring_timing("spring(visual 0.3)")
            .unwrap()
            .is_spring());
    }
}
