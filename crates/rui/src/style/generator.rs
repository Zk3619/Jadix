//! Motion-aligned value generators (engine kernel, not a CSS property expansion).
//!
//! Port of `Reference/motion-main/packages/motion-dom/src/animation/generators`:
//! closed-form spring (under / critical / over-damped), duration/bounce /
//! `visualDuration` resolution, inertia (friction + optional min/max bounce).
//!
//! Time is milliseconds. Velocity is **units/second** (same convention as
//! [`super::timing::spring_step`]). Callers and tests cross this seam;
//! CSS `transition` / `@keyframes` keep compiling into resolved
//! `{stiffness, damping, mass}`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::core::reactivity::{use_signal, Signal};

use super::element_style::TimingFunction;

const MAX_GENERATOR_MS: f32 = 20_000.0;
const DURATION_STEP_MS: f32 = 50.0;

const SPRING_MIN_DURATION_S: f64 = 0.01;
const SPRING_MAX_DURATION_S: f64 = 10.0;
const SPRING_MIN_DAMPING_RATIO: f64 = 0.05;
const SPRING_MAX_DAMPING_RATIO: f64 = 1.0;
const FIND_SPRING_SAFE_MIN: f64 = 0.001;
const FIND_SPRING_ITERS: u32 = 12;

const REST_SPEED_GRANULAR: f32 = 0.01;
const REST_SPEED_DEFAULT: f32 = 2.0;
const REST_DELTA_GRANULAR: f32 = 0.005;
const REST_DELTA_DEFAULT: f32 = 0.5;
const GRANULAR_DELTA: f32 = 5.0;

/// One sample from a time-driven generator (Motion `KeyframeGenerator.next`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneratorSample {
    pub value: f32,
    /// Units / second.
    pub velocity: f32,
    pub done: bool,
}

/// Resolved spring physics (what `TimingFunction::Spring` stores).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringPhysics {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl SpringPhysics {
    pub fn default_css() -> Self {
        Self {
            stiffness: 170.0,
            damping: 26.0,
            mass: 1.0,
        }
    }

    pub fn to_timing_function(self) -> TimingFunction {
        TimingFunction::Spring {
            stiffness: self.stiffness,
            damping: self.damping,
            mass: self.mass.max(1e-4),
        }
    }
}

/// Authoring options. Physics keys (`stiffness` / `damping` / `mass`) win over
/// duration / bounce / visual-duration, matching Motion `getSpringOptions`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringOptions {
    pub stiffness: Option<f32>,
    pub damping: Option<f32>,
    pub mass: Option<f32>,
    pub duration_ms: Option<f32>,
    pub bounce: Option<f32>,
    pub visual_duration_s: Option<f32>,
    pub rest_speed: Option<f32>,
    pub rest_delta: Option<f32>,
}

impl SpringOptions {
    pub fn physics(stiffness: f32, damping: f32, mass: f32) -> Self {
        Self {
            stiffness: Some(stiffness),
            damping: Some(damping),
            mass: Some(mass),
            ..Self::default()
        }
    }

    pub fn duration_bounce(duration_ms: f32, bounce: f32) -> Self {
        Self {
            duration_ms: Some(duration_ms),
            bounce: Some(bounce),
            ..Self::default()
        }
    }

    pub fn visual(visual_duration_s: f32, bounce: f32) -> Self {
        Self {
            visual_duration_s: Some(visual_duration_s),
            bounce: Some(bounce),
            ..Self::default()
        }
    }

    pub fn has_physics_keys(self) -> bool {
        self.stiffness.is_some() || self.damping.is_some() || self.mass.is_some()
    }

    pub fn has_duration_keys(self) -> bool {
        self.duration_ms.is_some() || self.bounce.is_some() || self.visual_duration_s.is_some()
    }

    /// Resolve to stiffness / damping / mass. Duration-defined springs ignore
    /// inherited velocity (Motion: interrupted velocity must not retune `findSpring`).
    pub fn resolve(self) -> SpringPhysics {
        if !self.has_physics_keys() && self.has_duration_keys() {
            if let Some(visual) = self.visual_duration_s {
                return resolve_visual_duration(visual, self.bounce.unwrap_or(0.3));
            }
            return find_spring(
                self.duration_ms.unwrap_or(800.0),
                self.bounce.unwrap_or(0.3),
                self.mass.unwrap_or(1.0),
            );
        }
        let fallback = SpringPhysics::default_css();
        SpringPhysics {
            stiffness: self.stiffness.unwrap_or(fallback.stiffness),
            damping: self.damping.unwrap_or(fallback.damping),
            mass: self.mass.unwrap_or(fallback.mass).max(1e-4),
        }
    }
}

fn resolve_visual_duration(visual_s: f32, bounce: f32) -> SpringPhysics {
    let visual = (visual_s as f64).max(1e-4);
    let root = (2.0 * std::f64::consts::PI) / (visual * 1.2);
    let stiffness = root * root;
    let ratio = (1.0 - bounce as f64).clamp(SPRING_MIN_DAMPING_RATIO, SPRING_MAX_DAMPING_RATIO);
    let damping = 2.0 * ratio * stiffness.sqrt();
    SpringPhysics {
        stiffness: stiffness as f32,
        damping: damping as f32,
        mass: 1.0,
    }
}

/// Framer / Motion `findSpring`: map duration + bounce → stiffness / damping.
pub fn find_spring(duration_ms: f32, bounce: f32, mass: f32) -> SpringPhysics {
    let mass = mass.max(1e-4) as f64;
    let mut damping_ratio =
        (1.0 - bounce as f64).clamp(SPRING_MIN_DAMPING_RATIO, SPRING_MAX_DAMPING_RATIO);
    let duration_s =
        ((duration_ms as f64) / 1000.0).clamp(SPRING_MIN_DURATION_S, SPRING_MAX_DURATION_S);
    let velocity = 0.0_f64;

    let undamped = if damping_ratio < 1.0 {
        approximate_root(
            |w| {
                let exponential_decay = w * damping_ratio;
                let delta = exponential_decay * duration_s;
                let a = exponential_decay - velocity;
                let b = calc_angular_freq(w, damping_ratio);
                let c = (-delta).exp();
                FIND_SPRING_SAFE_MIN - (a / b) * c
            },
            |w| {
                let exponential_decay = w * damping_ratio;
                let delta = exponential_decay * duration_s;
                let d = delta * velocity + velocity;
                let e = damping_ratio * damping_ratio * w * w * duration_s;
                let f = (-delta).exp();
                let g = calc_angular_freq(w * w, damping_ratio);
                let env = {
                    let exponential_decay = w * damping_ratio;
                    let delta = exponential_decay * duration_s;
                    let a = exponential_decay - velocity;
                    let b = calc_angular_freq(w, damping_ratio);
                    let c = (-delta).exp();
                    FIND_SPRING_SAFE_MIN - (a / b) * c
                };
                let factor = if -env + FIND_SPRING_SAFE_MIN > 0.0 {
                    -1.0
                } else {
                    1.0
                };
                (factor * ((d - e) * f)) / g
            },
            5.0 / duration_s,
        )
    } else {
        damping_ratio = 1.0;
        approximate_root(
            |w| {
                let a = (-w * duration_s).exp();
                let b = (w - velocity) * duration_s + 1.0;
                -FIND_SPRING_SAFE_MIN + a * b
            },
            |w| {
                let a = (-w * duration_s).exp();
                let b = (velocity - w) * (duration_s * duration_s);
                a * b
            },
            5.0 / duration_s,
        )
    };

    if !undamped.is_finite() || undamped <= 0.0 {
        return SpringPhysics {
            stiffness: 100.0,
            damping: 10.0,
            mass: mass as f32,
        };
    }
    let stiffness = undamped * undamped * mass;
    let damping = damping_ratio * 2.0 * (mass * stiffness).sqrt();
    SpringPhysics {
        stiffness: stiffness as f32,
        damping: damping as f32,
        mass: mass as f32,
    }
}

fn calc_angular_freq(undamped: f64, damping_ratio: f64) -> f64 {
    undamped * (1.0 - damping_ratio * damping_ratio).sqrt()
}

fn approximate_root(
    envelope: impl Fn(f64) -> f64,
    derivative: impl Fn(f64) -> f64,
    initial: f64,
) -> f64 {
    let mut result = initial;
    for _ in 1..FIND_SPRING_ITERS {
        let d = derivative(result);
        if d.abs() < 1e-12 {
            break;
        }
        result -= envelope(result) / d;
        if !result.is_finite() {
            return f64::NAN;
        }
    }
    result
}

#[derive(Clone, Copy, Debug)]
enum SpringRegime {
    Under { angular: f64, decay: f64 },
    Critical { omega: f64 },
    Over { damped_angular: f64, decay: f64 },
}

/// Closed-form spring. `next(t_ms)` is absolute time from the last
/// [`Self::retarget`] / construction (Motion generator, not Euler).
#[derive(Clone, Debug)]
pub struct Spring {
    physics: SpringPhysics,
    origin: f64,
    target: f64,
    /// Stored as formula parameter (`-dpos/dt`), see Motion sign flip.
    velocity_param: f64,
    rest_speed: f32,
    rest_delta: f32,
    duration_resolved: bool,
    resolved_duration_ms: Option<f32>,
    regime: SpringRegime,
}

impl Spring {
    pub fn new(origin: f32, target: f32, velocity: f32, opts: SpringOptions) -> Self {
        let physics = opts.resolve();
        let duration_resolved = !opts.has_physics_keys() && opts.has_duration_keys();
        let ignore_velocity = duration_resolved;
        let mut s = Self::from_physics(
            origin,
            target,
            if ignore_velocity { 0.0 } else { velocity },
            physics,
        );
        if let Some(v) = opts.rest_speed {
            s.rest_speed = v;
        }
        if let Some(v) = opts.rest_delta {
            s.rest_delta = v;
        }
        s.duration_resolved = duration_resolved;
        if duration_resolved {
            s.resolved_duration_ms = opts.duration_ms.or(Some(800.0));
        }
        s
    }

    pub fn from_physics(origin: f32, target: f32, velocity: f32, physics: SpringPhysics) -> Self {
        let mass = physics.mass.max(1e-4) as f64;
        let stiffness = physics.stiffness.max(1e-6) as f64;
        let damping = physics.damping.max(0.0) as f64;
        let omega = (stiffness / mass).sqrt();
        let damping_ratio = damping / (2.0 * (stiffness * mass).sqrt());
        let regime = if damping_ratio < 1.0 - 1e-4 {
            let angular = omega * (1.0 - damping_ratio * damping_ratio).sqrt();
            SpringRegime::Under {
                angular,
                decay: damping_ratio * omega,
            }
        } else if damping_ratio > 1.0 + 1e-4 {
            let damped = omega * (damping_ratio * damping_ratio - 1.0).sqrt();
            SpringRegime::Over {
                damped_angular: damped,
                decay: damping_ratio * omega,
            }
        } else {
            SpringRegime::Critical { omega }
        };
        let delta = target as f64 - origin as f64;
        let (rest_speed, rest_delta) = rest_thresholds(delta.abs() as f32);
        Self {
            physics: SpringPhysics {
                mass: mass as f32,
                ..physics
            },
            origin: origin as f64,
            target: target as f64,
            velocity_param: -(velocity as f64),
            rest_speed,
            rest_delta,
            duration_resolved: false,
            resolved_duration_ms: None,
            regime,
        }
    }

    pub fn physics(&self) -> SpringPhysics {
        self.physics
    }

    /// CSS transition settle band (tighter than Motion rest, keeps mid-frames).
    pub(crate) fn with_rest(mut self, rest_speed: f32, rest_delta: f32) -> Self {
        self.rest_speed = rest_speed;
        self.rest_delta = rest_delta;
        self
    }

    pub fn retarget(&mut self, origin: f32, target: f32, velocity: f32) {
        self.origin = origin as f64;
        self.target = target as f64;
        self.velocity_param = if self.duration_resolved {
            0.0
        } else {
            -(velocity as f64)
        };
        let (rs, rd) = rest_thresholds((self.target - self.origin).abs() as f32);
        self.rest_speed = rs;
        self.rest_delta = rd;
    }

    pub fn next(&self, t_ms: f32) -> GeneratorSample {
        let t = (t_ms.max(0.0) as f64) / 1000.0;
        let (value, vel) = self.sample_secs(t);
        let done = if self.duration_resolved {
            t_ms >= self.resolved_duration_ms.unwrap_or(800.0)
        } else {
            vel.abs() <= self.rest_speed && (self.target as f32 - value).abs() <= self.rest_delta
        };
        GeneratorSample {
            value: if done { self.target as f32 } else { value },
            velocity: if done { 0.0 } else { vel },
            done,
        }
    }

    /// Walk the generator until rest (Motion `calcGeneratorDuration`).
    pub fn calculated_duration_ms(&self) -> f32 {
        if let Some(d) = self.resolved_duration_ms {
            return d;
        }
        let mut t = 0.0_f32;
        let mut state = self.next(t);
        while !state.done && t < MAX_GENERATOR_MS {
            t += DURATION_STEP_MS;
            state = self.next(t);
        }
        if t >= MAX_GENERATOR_MS {
            MAX_GENERATOR_MS
        } else {
            t
        }
    }

    fn sample_secs(&self, t: f64) -> (f32, f32) {
        let delta = self.target - self.origin;
        let v = self.velocity_param;
        match self.regime {
            SpringRegime::Under { angular, decay } => {
                let env = (-decay * t).exp();
                let s = (angular * t).sin();
                let c = (angular * t).cos();
                let a = (v + decay * delta) / angular;
                let pos = self.target - env * (a * s + delta * c);
                let sin_c = decay * a + delta * angular;
                let cos_c = decay * delta - a * angular;
                let vel = env * (sin_c * s + cos_c * c);
                (pos as f32, vel as f32)
            }
            SpringRegime::Critical { omega } => {
                let env = (-omega * t).exp();
                let coeff = v + omega * delta;
                let pos = self.target - env * (delta + coeff * t);
                let vel = env * (omega * coeff * t - v);
                (pos as f32, vel as f32)
            }
            SpringRegime::Over {
                damped_angular,
                decay,
            } => {
                let env = (-decay * t).exp();
                let freq_t = (damped_angular * t).min(20.0);
                let sh = freq_t.sinh();
                let ch = freq_t.cosh();
                let pos = self.target
                    - (env * ((v + decay * delta) * sh + damped_angular * delta * ch))
                        / damped_angular;
                let p = (v + decay * delta) / damped_angular;
                let sinh_c = decay * p - delta * damped_angular;
                let cosh_c = decay * delta - p * damped_angular;
                let vel = env * (sinh_c * sh + cosh_c * ch);
                (pos as f32, vel as f32)
            }
        }
    }
}

fn rest_thresholds(abs_delta: f32) -> (f32, f32) {
    if abs_delta < GRANULAR_DELTA {
        (REST_SPEED_GRANULAR, REST_DELTA_GRANULAR)
    } else {
        (REST_SPEED_DEFAULT, REST_DELTA_DEFAULT)
    }
}

/// Closed-form replacement for implicit-Euler [`super::timing::spring_step`].
/// Settle thresholds stay on the historical tight band so existing CSS
/// transition mid-frames do not suddenly jump to rest.
pub fn spring_step_closed(
    pos: f32,
    vel: f32,
    target: f32,
    dt_secs: f32,
    stiffness: f32,
    damping: f32,
    mass: f32,
) -> (f32, f32, bool) {
    let dt = dt_secs.clamp(0.0, 0.064);
    if dt <= 0.0 {
        let settled = (pos - target).abs() < 8e-4 && vel.abs() < 8e-3;
        return if settled {
            (target, 0.0, true)
        } else {
            (pos, vel, false)
        };
    }
    let spring = Spring::from_physics(
        pos,
        target,
        vel,
        SpringPhysics {
            stiffness,
            damping,
            mass,
        },
    );
    let sample = spring.next(dt * 1000.0);
    let settled = (sample.value - target).abs() < 8e-4 && sample.velocity.abs() < 8e-3;
    if settled {
        (target, 0.0, true)
    } else {
        (sample.value, sample.velocity, false)
    }
}

thread_local! {
    static UNIT_SPRING_DUR: RefCell<std::collections::HashMap<(u32, u32, u32), f32>> =
        RefCell::new(std::collections::HashMap::new());
}

fn unit_spring_duration_ms(stiffness: f32, damping: f32, mass: f32) -> f32 {
    let key = (stiffness.to_bits(), damping.to_bits(), mass.to_bits());
    UNIT_SPRING_DUR.with(|map| {
        if let Some(&d) = map.borrow().get(&key) {
            return d;
        }
        let spring = Spring::from_physics(
            0.0,
            1.0,
            0.0,
            SpringPhysics {
                stiffness,
                damping,
                mass,
            },
        );
        let d = spring.calculated_duration_ms().max(1.0);
        map.borrow_mut().insert(key, d);
        d
    })
}

/// Unit spring as a 0..=1 easing (for `TimingFunction::eval` on a duration track).
/// `t` is normalized duration progress; the spring is sampled across its
/// calculated rest time so `@keyframes` + `spring()` actually overshoots.
pub fn spring_unit_easing(t: f32, stiffness: f32, damping: f32, mass: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let spring = Spring::from_physics(
        0.0,
        1.0,
        0.0,
        SpringPhysics {
            stiffness,
            damping,
            mass,
        },
    );
    let dur = unit_spring_duration_ms(stiffness, damping, mass);
    spring.next(t * dur).value
}

/// Inertia / decay after a flick. Velocity is units/second; `time_constant_ms`
/// matches Motion (`325`). Optional `min` / `max` catch the coast with a spring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InertiaOptions {
    pub velocity: f32,
    pub power: f32,
    pub time_constant_ms: f32,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub bounce_stiffness: f32,
    pub bounce_damping: f32,
    pub rest_delta: f32,
    pub rest_speed: Option<f32>,
}

impl Default for InertiaOptions {
    fn default() -> Self {
        Self {
            velocity: 0.0,
            power: 0.8,
            time_constant_ms: 325.0,
            min: None,
            max: None,
            bounce_stiffness: 500.0,
            bounce_damping: 10.0,
            rest_delta: 0.5,
            rest_speed: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Inertia {
    target: f32,
    amplitude: f32,
    time_constant_ms: f32,
    rest_delta: f32,
    min: Option<f32>,
    max: Option<f32>,
    bounce_stiffness: f32,
    bounce_damping: f32,
    rest_speed: Option<f32>,
    boundary_ms: Option<f32>,
    bounce: Option<Spring>,
}

impl Inertia {
    pub fn new(origin: f32, opts: InertiaOptions) -> Self {
        let amplitude = opts.power * opts.velocity;
        let ideal = origin + amplitude;
        let target = ideal;
        let mut s = Self {
            target,
            amplitude,
            time_constant_ms: opts.time_constant_ms.max(1.0),
            rest_delta: opts.rest_delta,
            min: opts.min,
            max: opts.max,
            bounce_stiffness: opts.bounce_stiffness,
            bounce_damping: opts.bounce_damping,
            rest_speed: opts.rest_speed,
            boundary_ms: None,
            bounce: None,
        };
        s.catch_boundary(0.0);
        s
    }

    fn out_of_bounds(&self, v: f32) -> bool {
        self.min.is_some_and(|m| v < m) || self.max.is_some_and(|m| v > m)
    }

    fn nearest_boundary(&self, v: f32) -> Option<f32> {
        match (self.min, self.max) {
            (Some(lo), Some(hi)) => {
                if (lo - v).abs() < (hi - v).abs() {
                    Some(lo)
                } else {
                    Some(hi)
                }
            }
            (Some(lo), None) => Some(lo),
            (None, Some(hi)) => Some(hi),
            (None, None) => None,
        }
    }

    fn friction(&self, t_ms: f32) -> GeneratorSample {
        let delta = -self.amplitude * (-t_ms / self.time_constant_ms).exp();
        let done = delta.abs() <= self.rest_delta;
        GeneratorSample {
            value: if done {
                self.target
            } else {
                self.target + delta
            },
            velocity: if done {
                0.0
            } else {
                // d/dt_ms of (target + delta) = -delta / time_constant, in units/ms
                // → units/s
                (-delta / self.time_constant_ms) * 1000.0
            },
            done,
        }
    }

    fn catch_boundary(&mut self, t_ms: f32) {
        if self.bounce.is_some() || self.boundary_ms.is_some() {
            return;
        }
        let sample = self.friction(t_ms);
        if !self.out_of_bounds(sample.value) {
            return;
        }
        let Some(bound) = self.nearest_boundary(sample.value) else {
            return;
        };
        self.boundary_ms = Some(t_ms);
        let mut opts = SpringOptions::physics(self.bounce_stiffness, self.bounce_damping, 1.0);
        opts.rest_delta = Some(self.rest_delta);
        opts.rest_speed = self.rest_speed;
        self.bounce = Some(Spring::new(sample.value, bound, sample.velocity, opts));
    }

    pub fn next(&mut self, t_ms: f32) -> GeneratorSample {
        let t_ms = t_ms.max(0.0);
        if self.bounce.is_none() && self.boundary_ms.is_none() {
            let sample = self.friction(t_ms);
            self.catch_boundary(t_ms);
            if let (Some(t0), Some(spring)) = (self.boundary_ms, self.bounce.as_ref()) {
                if t_ms >= t0 {
                    return spring.next(t_ms - t0);
                }
            }
            return sample;
        }
        if let (Some(t0), Some(spring)) = (self.boundary_ms, self.bounce.as_ref()) {
            if t_ms >= t0 {
                return spring.next(t_ms - t0);
            }
        }
        self.friction(t_ms)
    }
}

/// Duration tween (Motion `keyframes` generator, two stops).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TweenOptions {
    pub duration_ms: f32,
    pub easing: TimingFunction,
}

impl Default for TweenOptions {
    fn default() -> Self {
        Self {
            duration_ms: 300.0,
            easing: TimingFunction::EaseInOut,
        }
    }
}

pub fn tween_sample(from: f32, to: f32, t_ms: f32, opts: TweenOptions) -> GeneratorSample {
    let duration = opts.duration_ms.max(0.0);
    let done = duration <= 0.0 || t_ms >= duration;
    let t = if duration <= 0.0 {
        1.0
    } else {
        (t_ms / duration).clamp(0.0, 1.0)
    };
    let eased = opts.easing.eval(t);
    let value = if done { to } else { from + (to - from) * eased };
    GeneratorSample {
        value,
        velocity: 0.0,
        done,
    }
}

/// Unified value driver. One interface; spring / inertia / tween sit behind it.
#[derive(Clone, Debug)]
pub enum ValueDriver {
    Tween(TweenOptions),
    Spring(SpringOptions),
    Inertia(InertiaOptions),
}

impl ValueDriver {
    pub fn spring() -> Self {
        Self::Spring(SpringOptions::default())
    }

    pub fn tween(duration_ms: f32, easing: TimingFunction) -> Self {
        Self::Tween(TweenOptions {
            duration_ms,
            easing,
        })
    }

    pub fn inertia(velocity: f32) -> Self {
        let mut opts = InertiaOptions::default();
        opts.velocity = velocity;
        Self::Inertia(opts)
    }
}

/// Where a stagger delay is measured from (Motion `stagger({ from })`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaggerFrom {
    First,
    Last,
    Center,
    Index(usize),
}

/// Delay in milliseconds for child `index` of `count` (Motion `stagger`).
pub fn stagger_ms(index: usize, count: usize, interval_ms: f32, from: StaggerFrom) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let last = count.saturating_sub(1);
    let origin = match from {
        StaggerFrom::First => 0,
        StaggerFrom::Last => last,
        StaggerFrom::Center => last / 2,
        StaggerFrom::Index(i) => i.min(last),
    };
    let dist = (index as i32 - origin as i32).unsigned_abs() as f32;
    dist * interval_ms
}

/// Parse CSS-ish `spring(...)` arguments into options (physics or duration).
///
/// Positional: `spring(k)` / `spring(k, d)` / `spring(k, d, mass)` stay stiffness.
/// A first token with a time unit (`800ms`, `0.4s`) is duration + optional bounce.
/// Named: `duration` / `bounce` / `visual` / `stiffness`|`k` / `damping`|`d` / `mass`.
pub fn parse_spring_options(inner: &str) -> Option<SpringOptions> {
    let inner = inner.trim();
    if inner.is_empty() {
        return Some(SpringOptions::default());
    }
    let mut opts = SpringOptions::default();
    let mut positional: Vec<SpringArg> = Vec::new();
    let mut named = false;
    for raw in inner.split(',') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some((key, val)) = split_named_arg(raw) {
            named = true;
            apply_named(&mut opts, key, val)?;
        } else {
            positional.push(parse_arg_token(raw)?);
        }
    }
    if named {
        for (i, arg) in positional.into_iter().enumerate() {
            match (i, arg) {
                (0, SpringArg::Time(ms)) => opts.duration_ms = Some(ms),
                (0, SpringArg::Number(n)) => opts.stiffness = Some(n),
                (1, SpringArg::Number(n)) => {
                    if opts.duration_ms.is_some() {
                        opts.bounce = Some(n);
                    } else {
                        opts.damping = Some(n);
                    }
                }
                (2, SpringArg::Number(n)) => opts.mass = Some(n),
                _ => {}
            }
        }
        return Some(opts);
    }
    match positional.as_slice() {
        [] => Some(opts),
        [SpringArg::Time(ms)] => {
            opts.duration_ms = Some(*ms);
            Some(opts)
        }
        [SpringArg::Time(ms), SpringArg::Number(b)] => {
            opts.duration_ms = Some(*ms);
            opts.bounce = Some(*b);
            Some(opts)
        }
        [SpringArg::Number(k)] => {
            opts.stiffness = Some(*k);
            Some(opts)
        }
        [SpringArg::Number(k), SpringArg::Number(d)] => {
            opts.stiffness = Some(*k);
            opts.damping = Some(*d);
            Some(opts)
        }
        [SpringArg::Number(k), SpringArg::Number(d), SpringArg::Number(m), ..] => {
            opts.stiffness = Some(*k);
            opts.damping = Some(*d);
            opts.mass = Some(*m);
            Some(opts)
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum SpringArg {
    Number(f32),
    Time(f32),
}

fn split_named_arg(raw: &str) -> Option<(&str, &str)> {
    let raw = raw.trim();
    if let Some(i) = raw.find(':') {
        let (k, v) = raw.split_at(i);
        return Some((k.trim(), v[1..].trim()));
    }
    let mut parts = raw.split_whitespace();
    let key = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return None;
    }
    if matches!(
        key,
        "duration" | "bounce" | "visual" | "stiffness" | "damping" | "mass" | "k" | "d"
    ) {
        Some((key, rest[0]))
    } else {
        None
    }
}

fn apply_named(opts: &mut SpringOptions, key: &str, val: &str) -> Option<()> {
    match key {
        "duration" => {
            opts.duration_ms = Some(parse_time_or_number(val)?);
        }
        "bounce" => opts.bounce = Some(val.parse().ok()?),
        "visual" => opts.visual_duration_s = Some(parse_visual_seconds(val)?),
        "stiffness" | "k" => opts.stiffness = Some(val.parse().ok()?),
        "damping" | "d" => opts.damping = Some(val.parse().ok()?),
        "mass" => opts.mass = Some(val.parse().ok()?),
        _ => return None,
    }
    Some(())
}

fn parse_arg_token(raw: &str) -> Option<SpringArg> {
    if let Some(ms) = crate::core::tree::parse_css_time_ms_strict(raw) {
        return Some(SpringArg::Time(ms));
    }
    raw.parse().ok().map(SpringArg::Number)
}

fn parse_time_or_number(val: &str) -> Option<f32> {
    crate::core::tree::parse_css_time_ms_strict(val).or_else(|| val.parse().ok())
}

fn parse_visual_seconds(val: &str) -> Option<f32> {
    if let Some(ms) = crate::core::tree::parse_css_time_ms_strict(val) {
        return Some(ms / 1000.0);
    }
    val.parse().ok()
}

// ---------------------------------------------------------------------------
// Programmatic value handle (Motion `animate` + FollowAnimation + JSAnimation)
// ---------------------------------------------------------------------------

/// How a finished segment continues (Motion `repeat` / `repeatType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Once,
    /// Restart from the segment origin (snap, like CSS iteration).
    Loop,
    /// Reverse toward the origin, keeping velocity (Motion `reverse` / `mirror`).
    PingPong,
}

/// Advance a duration clock past `duration` under [`RepeatMode`].
/// Returns `(elapsed, reverse, still_playing)`.
pub(crate) fn cycle_duration_clock(
    elapsed: f32,
    duration: f32,
    mode: RepeatMode,
    reverse: bool,
) -> (f32, bool, bool) {
    let duration = duration.max(1e-6);
    if elapsed < duration {
        return (elapsed, reverse, true);
    }
    match mode {
        RepeatMode::Once => (duration, reverse, false),
        RepeatMode::Loop => (elapsed % duration, reverse, true),
        RepeatMode::PingPong => ((elapsed - duration).min(duration), !reverse, true),
    }
}

thread_local! {
    static VALUE_ANIMS: RefCell<Vec<Weak<RefCell<AnimatedNumberInner>>>> =
        const { RefCell::new(Vec::new()) };
}

struct AnimatedNumberInner {
    value: f32,
    from: f32,
    target: f32,
    home: f32,
    velocity: f32,
    elapsed_ms: f32,
    delay_ms: f32,
    playback_rate: f32,
    paused: bool,
    driver: ValueDriver,
    spring: Option<Spring>,
    inertia: Option<Inertia>,
    active: bool,
    signal: Signal<f32>,
    finished: Signal<bool>,
    repeat: RepeatMode,
    repeats_left: Option<u32>,
    pending_target: Option<f32>,
    follow: Option<Signal<f32>>,
    queue: VecDeque<(f32, ValueDriver)>,
}

/// Signal-backed number driven by [`ValueDriver`]. Clone shares the cell.
///
/// One handle: oneshot, follow-retarget, play/pause/speed, repeat, sequence.
#[derive(Clone)]
pub struct AnimatedNumber(Rc<RefCell<AnimatedNumberInner>>);

impl PartialEq for AnimatedNumber {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl AnimatedNumber {
    pub fn run(from: f32, to: f32, driver: ValueDriver) -> Self {
        let done = (from - to).abs() < 1e-5 && !matches!(driver, ValueDriver::Inertia(_));
        let mut inner = AnimatedNumberInner {
            value: from,
            from,
            target: to,
            home: from,
            velocity: 0.0,
            elapsed_ms: 0.0,
            delay_ms: 0.0,
            playback_rate: 1.0,
            paused: false,
            driver,
            spring: None,
            inertia: None,
            active: !done,
            signal: Signal::new_unowned(from),
            finished: Signal::new_unowned(done),
            repeat: RepeatMode::Once,
            repeats_left: Some(0),
            pending_target: None,
            follow: None,
            queue: VecDeque::new(),
        };
        if inner.active {
            arm_driver(&mut inner);
        }
        let state = Rc::new(RefCell::new(inner));
        VALUE_ANIMS.with(|list| list.borrow_mut().push(Rc::downgrade(&state)));
        Self(state)
    }

    pub fn spring(from: f32, to: f32) -> Self {
        Self::run(from, to, ValueDriver::spring())
    }

    pub fn tween(from: f32, to: f32, duration_ms: f32, easing: TimingFunction) -> Self {
        Self::run(from, to, ValueDriver::tween(duration_ms, easing))
    }

    pub fn inertia(from: f32, velocity: f32) -> Self {
        Self::run(from, from, ValueDriver::inertia(velocity))
    }

    /// Idle follower (Motion `springValue` / `followValue`). Aim with [`Self::set`].
    pub fn follow(initial: f32) -> Self {
        let n = Self::spring(initial, initial);
        n
    }

    pub fn delay(self, ms: f32) -> Self {
        self.0.borrow_mut().delay_ms = ms.max(0.0);
        self
    }

    pub fn repeat(self, mode: RepeatMode) -> Self {
        {
            let mut s = self.0.borrow_mut();
            s.repeat = mode;
            s.repeats_left = match mode {
                RepeatMode::Once => Some(0),
                RepeatMode::Loop | RepeatMode::PingPong => None,
            };
        }
        self
    }

    /// Extra iterations after the first play (Motion `repeat: n`). `0` = oneshot.
    pub fn repeat_times(self, n: u32) -> Self {
        {
            let mut s = self.0.borrow_mut();
            if s.repeat == RepeatMode::Once && n > 0 {
                s.repeat = RepeatMode::Loop;
            }
            s.repeats_left = Some(n);
        }
        self
    }

    pub fn speed(self, rate: f32) -> Self {
        self.0.borrow_mut().playback_rate = rate;
        self
    }

    pub fn set_speed(&self, rate: f32) {
        self.0.borrow_mut().playback_rate = rate;
    }

    /// Chase `source` every tick (Motion follow). Coalesced with [`Self::set`].
    pub fn follow_signal(self, source: Signal<f32>) -> Self {
        {
            let mut s = self.0.borrow_mut();
            s.follow = Some(source);
            if let Some(v) = source.try_peek() {
                if (v - s.target).abs() > 1e-5 {
                    s.pending_target = Some(v);
                    s.active = true;
                    s.paused = false;
                    let _ = s.finished.try_set(false);
                }
            }
        }
        self
    }

    /// Queue another segment after the current one finishes (and after its repeats).
    pub fn then_to(self, to: f32, driver: ValueDriver) -> Self {
        self.0.borrow_mut().queue.push_back((to, driver));
        self
    }

    /// Re-arm from the live value. Same cell and [`Self::signal`] — do not
    /// replace the handle (a `bind_prop` from the first mount would keep the
    /// old signal and look like a dead click).
    pub fn restart(&self, to: f32, driver: ValueDriver) {
        let mut s = self.0.borrow_mut();
        s.queue.clear();
        s.pending_target = None;
        s.repeat = RepeatMode::Once;
        s.repeats_left = Some(0);
        let from = s.value;
        start_segment(&mut s, from, to, Some(driver));
    }

    /// Coalesced retarget (Motion `FollowAnimation.setTarget`). Applied on the
    /// next tick from the live position/velocity — many `set`s in one frame
    /// cost one retarget.
    pub fn set(&self, target: f32) {
        let mut s = self.0.borrow_mut();
        if (s.target - target).abs() < 1e-5 && s.pending_target.is_none() && s.active {
            return;
        }
        s.pending_target = Some(target);
        s.paused = false;
        s.active = true;
        let _ = s.finished.try_set(false);
    }

    pub fn play(&self) {
        let mut s = self.0.borrow_mut();
        s.paused = false;
        if !s.active {
            if (s.value - s.target).abs() < 1e-5 && s.queue.is_empty() {
                return;
            }
            s.active = true;
            s.from = s.value;
            s.elapsed_ms = 0.0;
            arm_driver(&mut s);
        }
        let _ = s.finished.try_set(false);
    }

    pub fn pause(&self) {
        self.0.borrow_mut().paused = true;
    }

    pub fn stop(&self) {
        let mut s = self.0.borrow_mut();
        s.active = false;
        s.paused = false;
        s.pending_target = None;
        s.velocity = 0.0;
        s.queue.clear();
        let _ = s.finished.try_set(true);
    }

    pub fn get(&self) -> f32 {
        self.0.borrow().signal.get()
    }

    pub fn peek(&self) -> f32 {
        let s = self.0.borrow();
        s.signal.try_peek().unwrap_or(s.value)
    }

    pub fn is_active(&self) -> bool {
        let s = self.0.borrow();
        s.active && !s.paused
    }

    pub fn is_paused(&self) -> bool {
        self.0.borrow().paused
    }

    pub fn velocity(&self) -> f32 {
        self.0.borrow().velocity
    }

    pub fn finished(&self) -> Signal<bool> {
        self.0.borrow().finished
    }

    /// Live value cell. Bind to `w` / `h` / text via this Signal (do not `get()`
    /// it in a `#[component]` body — that would rebuild the card every frame).
    pub fn signal(&self) -> Signal<f32> {
        self.0.borrow().signal
    }
}

/// Motion-style `animate(from, to)`. Defaults to a CSS spring (`170 / 26 / 1`).
pub fn animate(from: f32, to: f32) -> AnimatedNumber {
    AnimatedNumber::spring(from, to)
}

/// Idle spring follower. Same as [`AnimatedNumber::follow`].
pub fn follow(initial: f32) -> AnimatedNumber {
    AnimatedNumber::follow(initial)
}

pub fn use_animated(id_str: &str, initial: f32) -> AnimatedNumber {
    let sig = use_signal(id_str, move || AnimatedNumber::spring(initial, initial));
    sig.peek()
}

fn arm_driver(s: &mut AnimatedNumberInner) {
    s.spring = None;
    s.inertia = None;
    match s.driver {
        ValueDriver::Spring(opts) => {
            s.spring = Some(Spring::new(s.from, s.target, s.velocity, opts));
        }
        ValueDriver::Inertia(mut opts) => {
            if opts.velocity.abs() < 1e-6 {
                opts.velocity = s.velocity;
            }
            s.inertia = Some(Inertia::new(s.from, opts));
            s.active = true;
        }
        ValueDriver::Tween(_) => {}
    }
}

fn retarget_in_place(s: &mut AnimatedNumberInner, origin: f32, target: f32, velocity: f32) {
    s.from = origin;
    s.target = target;
    s.home = origin;
    s.velocity = velocity;
    s.elapsed_ms = 0.0;
    s.delay_ms = 0.0;
    match &mut s.driver {
        ValueDriver::Spring(_) => {
            if let Some(spring) = s.spring.as_mut() {
                spring.retarget(origin, target, velocity);
            } else {
                arm_driver(s);
            }
        }
        _ => arm_driver(s),
    }
    s.active = true;
}

fn sample_driver(s: &mut AnimatedNumberInner, t: f32) -> GeneratorSample {
    match &mut s.driver {
        ValueDriver::Tween(opts) => tween_sample(s.from, s.target, t, *opts),
        ValueDriver::Spring(_) => {
            s.spring
                .as_ref()
                .map(|sp| sp.next(t))
                .unwrap_or(GeneratorSample {
                    value: s.target,
                    velocity: 0.0,
                    done: true,
                })
        }
        ValueDriver::Inertia(_) => {
            s.inertia
                .as_mut()
                .map(|n| n.next(t))
                .unwrap_or(GeneratorSample {
                    value: s.value,
                    velocity: 0.0,
                    done: true,
                })
        }
    }
}

fn consume_repeat(s: &mut AnimatedNumberInner) -> bool {
    match s.repeats_left {
        None => true,
        Some(0) => false,
        Some(n) => {
            s.repeats_left = Some(n - 1);
            true
        }
    }
}

fn start_segment(s: &mut AnimatedNumberInner, from: f32, to: f32, driver: Option<ValueDriver>) {
    s.from = from;
    s.target = to;
    s.home = from;
    s.elapsed_ms = 0.0;
    s.delay_ms = 0.0;
    if let Some(d) = driver {
        s.driver = d;
    }
    s.active = true;
    s.paused = false;
    arm_driver(s);
    let _ = s.finished.try_set(false);
}

fn finish_or_continue(s: &mut AnimatedNumberInner) {
    if let Some((to, driver)) = s.queue.pop_front() {
        start_segment(s, s.value, to, Some(driver));
        return;
    }
    if s.repeat != RepeatMode::Once && consume_repeat(s) {
        match s.repeat {
            RepeatMode::Once => {}
            RepeatMode::Loop => {
                s.value = s.home;
                s.velocity = 0.0;
                start_segment(s, s.home, s.target, None);
            }
            RepeatMode::PingPong => {
                let dest = s.home;
                start_segment(s, s.value, dest, None);
            }
        }
        return;
    }
    if !matches!(s.driver, ValueDriver::Inertia(_)) {
        s.value = s.target;
    }
    s.velocity = 0.0;
    s.active = false;
    let _ = s.finished.try_set(true);
}

fn step_inner(s: &mut AnimatedNumberInner, dt_secs: f32) -> bool {
    if let Some(src) = s.follow {
        if let Some(v) = src.try_peek() {
            if (v - s.target).abs() > 1e-5 {
                s.pending_target = Some(v);
            }
        }
    }

    if s.paused {
        return false;
    }

    let has_pending = s.pending_target.is_some();
    if !s.active && !has_pending {
        return false;
    }

    if s.active {
        let rate = s.playback_rate;
        s.elapsed_ms += dt_secs * 1000.0 * rate;
        if rate < 0.0 && s.elapsed_ms < 0.0 {
            s.elapsed_ms = 0.0;
        }
    }

    if s.active && s.elapsed_ms < s.delay_ms && !has_pending {
        return true;
    }

    let t = if s.active {
        (s.elapsed_ms - s.delay_ms).max(0.0)
    } else {
        0.0
    };
    let sample = if s.active {
        sample_driver(s, t)
    } else {
        GeneratorSample {
            value: s.value,
            velocity: s.velocity,
            done: true,
        }
    };
    s.value = sample.value;
    s.velocity = sample.velocity;

    let retargeted = if let Some(next) = s.pending_target.take() {
        if (next - s.value).abs() > 1e-5 || sample.velocity.abs() > 1e-3 {
            retarget_in_place(s, s.value, next, s.velocity);
            true
        } else {
            s.target = next;
            false
        }
    } else {
        false
    };

    if sample.done && !retargeted {
        finish_or_continue(s);
    }
    true
}

fn handle_needs_tick(s: &AnimatedNumberInner) -> bool {
    if s.paused {
        return false;
    }
    if s.active || s.pending_target.is_some() {
        return true;
    }
    s.follow
        .is_some_and(|src| src.try_peek().is_some_and(|v| (v - s.value).abs() > 1e-5))
}

/// Advance programmatic [`AnimatedNumber`] handles. Returns whether any is live.
pub fn tick_value_generators(dt_secs: f32) -> bool {
    let mut any = false;
    let mut updates = Vec::new();
    VALUE_ANIMS.with(|list| {
        let mut list = list.borrow_mut();
        let mut i = 0;
        while i < list.len() {
            if let Some(cell) = list[i].upgrade() {
                let mut s = cell.borrow_mut();
                if handle_needs_tick(&s) && step_inner(&mut s, dt_secs) {
                    any = true;
                    updates.push((s.signal, s.value));
                }
                i += 1;
            } else {
                list.remove(i);
            }
        }
    });
    for (sig, val) in updates {
        let _ = sig.try_set(val);
    }
    any
}

pub fn any_value_generator_active() -> bool {
    VALUE_ANIMS.with(|list| {
        list.borrow()
            .iter()
            .any(|w| w.upgrade().is_some_and(|s| handle_needs_tick(&s.borrow())))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underdamped_overshoots_then_settles() {
        let spring = Spring::from_physics(
            0.0,
            1.0,
            0.0,
            SpringPhysics {
                stiffness: 100.0,
                damping: 10.0,
                mass: 1.0,
            },
        );
        let mut peak = 0.0_f32;
        for i in 0..40 {
            peak = peak.max(spring.next(i as f32 * 20.0).value);
        }
        assert!(peak > 1.0, "underdamped must overshoot, peak={peak}");
        let mut settled = false;
        for i in 0..80 {
            if spring.next(i as f32 * 50.0).done {
                settled = true;
                break;
            }
        }
        assert!(settled);
        assert!((spring.next(20_000.0).value - 1.0).abs() < 1e-3);
    }

    #[test]
    fn critically_damped_does_not_overshoot() {
        // ζ = 1 ⇒ d = 2√(k m). k=100, m=1 ⇒ d=20.
        let spring = Spring::from_physics(
            0.0,
            1.0,
            0.0,
            SpringPhysics {
                stiffness: 100.0,
                damping: 20.0,
                mass: 1.0,
            },
        );
        let mut max_v = 0.0_f32;
        for i in 0..40 {
            max_v = max_v.max(spring.next(i as f32 * 20.0).value);
        }
        assert!(max_v <= 1.0 + 0.02, "critical must not bounce, max={max_v}");
    }

    #[test]
    fn overdamped_is_slower_than_critical() {
        let critical = Spring::from_physics(
            0.0,
            1.0,
            0.0,
            SpringPhysics {
                stiffness: 100.0,
                damping: 20.0,
                mass: 1.0,
            },
        );
        let over = Spring::from_physics(
            0.0,
            1.0,
            0.0,
            SpringPhysics {
                stiffness: 100.0,
                damping: 40.0,
                mass: 1.0,
            },
        );
        let tc = critical.next(200.0).value;
        let to = over.next(200.0).value;
        assert!(to < tc, "overdamped {to} should lag critical {tc}");
    }

    #[test]
    fn duration_bounce_resolves_to_physics() {
        let p = find_spring(800.0, 0.3, 1.0);
        assert!(p.stiffness > 10.0 && p.stiffness < 500.0, "{p:?}");
        assert!(p.damping > 1.0, "{p:?}");
        let spring = Spring::new(0.0, 100.0, 0.0, SpringOptions::duration_bounce(800.0, 0.3));
        let end = spring.next(800.0);
        assert!(end.done, "duration-resolved spring finishes at duration");
        assert!((end.value - 100.0).abs() < 1e-3);
    }

    #[test]
    fn visual_duration_is_snappy() {
        let p = SpringOptions::visual(0.3, 0.25).resolve();
        assert!(p.stiffness > 50.0, "{p:?}");
        let s = Spring::new(0.0, 1.0, 0.0, SpringOptions::visual(0.3, 0.25));
        let mid = s.next(180.0);
        assert!(
            mid.value > 0.4,
            "visual 0.3s should be well along, {}",
            mid.value
        );
    }

    #[test]
    fn retarget_keeps_velocity() {
        let mut s = Spring::from_physics(
            0.0,
            100.0,
            0.0,
            SpringPhysics {
                stiffness: 100.0,
                damping: 10.0,
                mass: 1.0,
            },
        );
        let a = s.next(80.0);
        s.retarget(a.value, 40.0, a.velocity);
        let b = s.next(16.0);
        assert!(
            (b.value - a.value).abs() > 0.5,
            "retargeted spring must keep moving"
        );
    }

    #[test]
    fn inertia_coasts_toward_amplitude() {
        let mut n = Inertia::new(
            0.0,
            InertiaOptions {
                velocity: 400.0,
                power: 0.8,
                ..InertiaOptions::default()
            },
        );
        let far = n.next(800.0);
        assert!(far.value > 200.0, "got {}", far.value);
        assert!(far.done || n.next(4000.0).done);
    }

    #[test]
    fn inertia_bounces_at_max() {
        let mut n = Inertia::new(
            0.0,
            InertiaOptions {
                velocity: 800.0,
                power: 1.0,
                max: Some(50.0),
                ..InertiaOptions::default()
            },
        );
        let mut hit_bound = false;
        for i in 0..40 {
            let s = n.next(i as f32 * 16.0);
            if s.value <= 55.0 && s.value >= 40.0 {
                hit_bound = true;
            }
            if s.done {
                assert!((s.value - 50.0).abs() < 1.0, "settle at max, {}", s.value);
                break;
            }
        }
        assert!(hit_bound);
    }

    #[test]
    fn stagger_from_ends() {
        assert!((stagger_ms(0, 4, 50.0, StaggerFrom::First) - 0.0).abs() < 1e-3);
        assert!((stagger_ms(3, 4, 50.0, StaggerFrom::First) - 150.0).abs() < 1e-3);
        assert!((stagger_ms(3, 4, 50.0, StaggerFrom::Last) - 0.0).abs() < 1e-3);
        assert!((stagger_ms(0, 4, 50.0, StaggerFrom::Last) - 150.0).abs() < 1e-3);
        assert!((stagger_ms(1, 3, 40.0, StaggerFrom::Center) - 0.0).abs() < 1e-3);
        assert!((stagger_ms(0, 3, 40.0, StaggerFrom::Center) - 40.0).abs() < 1e-3);
    }

    #[test]
    fn parse_positional_and_duration() {
        let k = parse_spring_options("90").unwrap();
        assert!((k.stiffness.unwrap() - 90.0).abs() < 1e-3);
        let dur = parse_spring_options("800ms, 0.25").unwrap();
        assert!((dur.duration_ms.unwrap() - 800.0).abs() < 1e-3);
        assert!((dur.bounce.unwrap() - 0.25).abs() < 1e-3);
        let named = parse_spring_options("duration 400ms, bounce 0.2").unwrap();
        assert!((named.duration_ms.unwrap() - 400.0).abs() < 1e-3);
        let visual = parse_spring_options("visual 0.3").unwrap();
        assert!((visual.visual_duration_s.unwrap() - 0.3).abs() < 1e-3);
    }

    #[test]
    fn animate_handle_ticks_to_target() {
        let n = animate(0.0, 1.0);
        assert!(n.is_active());
        let mut done = false;
        for _ in 0..120 {
            let _ = tick_value_generators(1.0 / 60.0);
            if !n.is_active() {
                done = true;
                break;
            }
        }
        assert!(done, "animate() must settle");
        assert!((n.peek() - 1.0).abs() < 1e-2);
    }

    #[test]
    fn tween_driver_respects_duration() {
        let n = AnimatedNumber::tween(0.0, 10.0, 100.0, TimingFunction::Linear);
        let _ = tick_value_generators(0.05);
        let mid = n.peek();
        assert!(mid > 4.0 && mid < 6.5, "linear 50ms of 100ms, got {mid}");
        let _ = tick_value_generators(0.06);
        assert!(!n.is_active());
        assert!((n.peek() - 10.0).abs() < 1e-3);
    }

    #[test]
    fn set_retargets_from_live_value() {
        let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear);
        let _ = tick_value_generators(0.05);
        let mid = n.peek();
        assert!(mid > 1.5 && mid < 3.5, "got {mid}");
        n.set(0.0);
        let _ = tick_value_generators(1.0 / 60.0);
        let after = n.peek();
        assert!(
            (after - mid).abs() < 1.5,
            "retarget must start near live mid {mid}, got {after}"
        );
        assert!(n.is_active());
    }

    #[test]
    fn pause_freezes_play_resumes() {
        let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear);
        let _ = tick_value_generators(0.05);
        let mid = n.peek();
        n.pause();
        assert!(n.is_paused());
        let _ = tick_value_generators(0.05);
        assert!((n.peek() - mid).abs() < 1e-4, "pause must freeze");
        n.play();
        let _ = tick_value_generators(0.05);
        assert!(n.peek() > mid + 1.0, "play must resume, {}", n.peek());
    }

    #[test]
    fn speed_scales_elapsed() {
        let n = AnimatedNumber::tween(0.0, 10.0, 200.0, TimingFunction::Linear).speed(2.0);
        let _ = tick_value_generators(0.05);
        let v = n.peek();
        assert!(v > 4.0 && v < 6.5, "2x of 50ms on 200ms → ~5, got {v}");
    }

    #[test]
    fn pingpong_reverses() {
        let n = AnimatedNumber::tween(0.0, 10.0, 100.0, TimingFunction::Linear)
            .repeat(RepeatMode::PingPong);
        let _ = tick_value_generators(0.10);
        assert!((n.peek() - 10.0).abs() < 0.6, "end of first, {}", n.peek());
        let _ = tick_value_generators(0.05);
        let back = n.peek();
        assert!(back < 8.0, "pingpong should reverse, got {back}");
        assert!(n.is_active());
    }

    #[test]
    fn then_to_plays_next_segment() {
        let n = AnimatedNumber::tween(0.0, 10.0, 50.0, TimingFunction::Linear)
            .then_to(0.0, ValueDriver::tween(50.0, TimingFunction::Linear));
        let _ = tick_value_generators(0.05);
        assert!(n.peek() > 8.0, "first segment {}", n.peek());
        let _ = tick_value_generators(0.05);
        assert!(
            n.peek() < 2.0,
            "second segment should return toward 0, {}",
            n.peek()
        );
        assert!(n.finished().peek() || !n.is_active() || n.peek() < 2.0);
    }

    #[test]
    fn follow_signal_chases() {
        let src = Signal::new_unowned(0.0_f32);
        let n = follow(0.0).follow_signal(src);
        src.set(8.0);
        let mut moved = false;
        for _ in 0..20 {
            let _ = tick_value_generators(1.0 / 60.0);
            if n.peek() > 1.0 {
                moved = true;
                break;
            }
        }
        assert!(moved, "follower must chase the signal, {}", n.peek());
    }

    #[test]
    fn finished_signal_flips() {
        let n = AnimatedNumber::tween(0.0, 1.0, 50.0, TimingFunction::Linear);
        assert!(!n.finished().peek());
        let _ = tick_value_generators(0.06);
        assert!(n.finished().peek());
        n.set(0.0);
        assert!(!n.finished().peek());
    }

    #[test]
    fn spring_step_closed_settles() {
        let mut pos = 0.0;
        let mut vel = 0.0;
        let p = SpringPhysics::default_css();
        let mut ok = false;
        for _ in 0..240 {
            let (np, nv, settled) =
                spring_step_closed(pos, vel, 1.0, 1.0 / 60.0, p.stiffness, p.damping, p.mass);
            pos = np;
            vel = nv;
            if settled {
                ok = true;
                break;
            }
        }
        assert!(ok);
        assert!((pos - 1.0).abs() < 1e-3);
    }

    #[test]
    fn css_mid_is_spring_next_at_elapsed() {
        let p = SpringPhysics::default_css();
        let s = Spring::from_physics(0.0, 1.0, 0.0, p).with_rest(8e-3, 8e-4);
        let mid = s.next(50.0);
        assert!(mid.value > 0.05 && mid.value < 0.95, "{}", mid.value);
        let again = Spring::from_physics(0.0, 1.0, 0.0, p)
            .with_rest(8e-3, 8e-4)
            .next(50.0);
        assert!((again.value - mid.value).abs() < 1e-6);
    }

    #[test]
    fn cubic_ui_curve_is_tween_sample() {
        let opts = TweenOptions {
            duration_ms: 150.0,
            easing: TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0),
        };
        let a = tween_sample(0.0, 10.0, 75.0, opts);
        let t = 75.0 / 150.0;
        let eased = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0).eval(t);
        assert!((a.value - 10.0 * eased).abs() < 1e-4);
        assert!(!a.done);
        assert!(tween_sample(0.0, 10.0, 150.0, opts).done);
    }

    #[test]
    fn replaced_handle_does_not_move_previous_signal() {
        let first = AnimatedNumber::follow(8.0);
        let bound = first.signal();
        let _second = AnimatedNumber::tween(8.0, 200.0, 40.0, TimingFunction::Linear);
        let _ = tick_value_generators(0.05);
        assert!(
            (bound.peek() - 8.0).abs() < 1e-3,
            "replacing the handle leaves the bound cell idle, {}",
            bound.peek()
        );
    }

    #[test]
    fn restart_drives_original_signal() {
        let n = AnimatedNumber::follow(8.0);
        let sig = n.signal();
        n.restart(200.0, ValueDriver::tween(40.0, TimingFunction::Linear));
        let _ = tick_value_generators(0.05);
        assert!(
            sig.peek() > 50.0,
            "restart must tick the same signal, {}",
            sig.peek()
        );
    }

    #[test]
    fn duration_clock_repeat_modes() {
        let (e, _, playing) = cycle_duration_clock(0.4, 0.5, RepeatMode::Once, false);
        assert!((e - 0.4).abs() < 1e-6 && playing);
        let (e, _, playing) = cycle_duration_clock(0.6, 0.5, RepeatMode::Once, false);
        assert!((e - 0.5).abs() < 1e-6 && !playing);
        let (e, _, playing) = cycle_duration_clock(0.7, 0.5, RepeatMode::Loop, false);
        assert!((e - 0.2).abs() < 1e-6 && playing);
        let (e, rev, playing) = cycle_duration_clock(0.6, 0.5, RepeatMode::PingPong, false);
        assert!((e - 0.1).abs() < 1e-6 && rev && playing);
    }
}
