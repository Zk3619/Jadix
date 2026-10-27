//! Closed-form spring and inertia generators, independent of UI owners and rendering.
//! Time uses milliseconds and velocity uses units per second.

use std::cell::RefCell;

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

    pub fn to_timing_function(self) -> super::element_style::TimingFunction {
        self.into()
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
        // visualDuration controls the main motion, not an arbitrary 800ms
        // completion deadline. Its tail settles using the physical thresholds.
        s.duration_resolved = duration_resolved && opts.visual_duration_s.is_none();
        if s.duration_resolved {
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
    pub fn with_rest(mut self, rest_speed: f32, rest_delta: f32) -> Self {
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
        if !cfg!(feature = "animation-base") {
            return GeneratorSample {
                value: self.target as f32,
                velocity: 0.0,
                done: true,
            };
        }
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
        if !cfg!(feature = "animation-base") {
            return 0.0;
        }
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

/// Closed-form replacement for implicit-Euler the implicit-Euler step.
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
    if !cfg!(feature = "animation-base") {
        return (target, 0.0, true);
    }
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
        if !cfg!(feature = "animation-base") {
            let value = self
                .target
                .max(self.min.unwrap_or(f32::NEG_INFINITY))
                .min(self.max.unwrap_or(f32::INFINITY));
            return GeneratorSample {
                value,
                velocity: 0.0,
                done: true,
            };
        }
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
