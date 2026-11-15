use crate::presentation::{NodeId, PresentationNode, Rect, RootLifetime};
use jadix_animation::{
    GeneratorSample, Inertia, Spring, SpringOptions, TimingFunction, TweenOptions,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

pub use jadix_animation::InertiaOptions as DecayOptions;

const MAX_VALUE: f32 = 1_000_000.0;
const MAX_DURATION: Duration = Duration::from_secs(20);
const MAX_TRACKS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    Linear,
    In,
    Out,
    InOut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transition {
    Timing {
        duration: Duration,
        easing: Ease,
    },
    Spring {
        stiffness: f32,
        damping: f32,
        mass: f32,
    },
}

impl Default for Transition {
    fn default() -> Self {
        Self::Timing {
            duration: Duration::from_millis(200),
            easing: Ease::InOut,
        }
    }
}

impl Transition {
    fn validate(self) -> Result<(), MotionError> {
        let valid = match self {
            Self::Timing { duration, .. } => duration <= MAX_DURATION,
            Self::Spring {
                stiffness,
                damping,
                mass,
            } => {
                (0.01..=100_000.0).contains(&stiffness)
                    && (0.01..=10_000.0).contains(&damping)
                    && (0.001..=1_000.0).contains(&mass)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(MotionError::InvalidOptions)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionError {
    Disposed,
    ForeignRoot,
    InvalidOptions,
    InvalidValue,
    TimeWentBackwards,
    TrackLimit,
}

impl std::fmt::Display for MotionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MotionError {}

#[derive(Clone, Debug)]
enum Driver {
    Timing {
        from: f32,
        target: f32,
        options: TweenOptions,
    },
    Spring(Spring),
    Decay(Inertia),
}

#[derive(Clone, Debug)]
pub struct MotionValue {
    sample: GeneratorSample,
    target: f32,
    driver: Option<Driver>,
    started: Duration,
    observed: Duration,
}

impl MotionValue {
    pub fn new(value: f32) -> Result<Self, MotionError> {
        valid_value(value)?;
        Ok(Self {
            sample: GeneratorSample {
                value,
                velocity: 0.0,
                done: true,
            },
            target: value,
            driver: None,
            started: Duration::ZERO,
            observed: Duration::ZERO,
        })
    }

    pub fn current(&self) -> GeneratorSample {
        self.sample
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn jump(&mut self, value: f32) -> Result<(), MotionError> {
        valid_value(value)?;
        self.target = value;
        self.sample = GeneratorSample {
            value,
            velocity: 0.0,
            done: true,
        };
        self.driver = None;
        Ok(())
    }

    pub fn animate_to(
        &mut self,
        target: f32,
        transition: Transition,
        now: Duration,
    ) -> Result<(), MotionError> {
        valid_value(target)?;
        transition.validate()?;
        self.check_time(now)?;
        let previous = self.sample(now)?;
        self.target = target;
        if previous.done && previous.value == target {
            return self.jump(target);
        }
        self.driver = Some(match transition {
            Transition::Timing { duration, easing } => Driver::Timing {
                from: previous.value,
                target,
                options: TweenOptions {
                    duration_ms: duration.as_secs_f32() * 1000.0,
                    easing: match easing {
                        Ease::Linear => TimingFunction::Linear,
                        Ease::In => TimingFunction::EaseIn,
                        Ease::Out => TimingFunction::EaseOut,
                        Ease::InOut => TimingFunction::EaseInOut,
                    },
                },
            },
            Transition::Spring {
                stiffness,
                damping,
                mass,
            } => Driver::Spring(Spring::new(
                previous.value,
                target,
                previous.velocity,
                SpringOptions::physics(stiffness, damping, mass),
            )),
        });
        self.started = now;
        self.sample.done = false;
        self.sample(now)?;
        Ok(())
    }

    pub fn decay(&mut self, options: DecayOptions, now: Duration) -> Result<(), MotionError> {
        self.check_time(now)?;
        let valid = options.velocity.is_finite()
            && options.velocity.abs() <= MAX_VALUE
            && (0.0..=1.0).contains(&options.power)
            && (1.0..=20_000.0).contains(&options.time_constant_ms)
            && (0.01..=100_000.0).contains(&options.bounce_stiffness)
            && (0.01..=10_000.0).contains(&options.bounce_damping)
            && (0.0001..=MAX_VALUE).contains(&options.rest_delta)
            && options
                .rest_speed
                .is_none_or(|value| (0.0001..=MAX_VALUE).contains(&value))
            && options
                .min
                .into_iter()
                .chain(options.max)
                .all(|value| valid_value(value).is_ok())
            && options
                .min
                .zip(options.max)
                .is_none_or(|(min, max)| min <= max);
        if !valid {
            return Err(MotionError::InvalidOptions);
        }
        let mut candidate = self.clone();
        let origin = candidate.sample(now)?.value;
        let mut target = origin + options.power * options.velocity;
        valid_value(target)?;
        if let Some(min) = options.min {
            target = target.max(min);
        }
        if let Some(max) = options.max {
            target = target.min(max);
        }
        candidate.target = target;
        candidate.driver = Some(Driver::Decay(Inertia::new(origin, options)));
        candidate.started = now;
        candidate.sample.done = false;
        candidate.sample(now)?;
        *self = candidate;
        Ok(())
    }

    pub fn sample(&mut self, now: Duration) -> Result<GeneratorSample, MotionError> {
        self.check_time(now)?;
        self.observed = now;
        let elapsed = now.saturating_sub(self.started);
        if elapsed >= MAX_DURATION {
            self.jump(self.target)?;
        } else if let Some(driver) = &mut self.driver {
            let milliseconds = elapsed.as_secs_f32() * 1000.0;
            self.sample = match driver {
                Driver::Timing {
                    from,
                    target,
                    options,
                } => jadix_animation::tween_sample(*from, *target, milliseconds, *options),
                Driver::Spring(spring) => spring.next(milliseconds),
                Driver::Decay(decay) => decay.next(milliseconds),
            };
            if !self.sample.value.is_finite() || !self.sample.velocity.is_finite() {
                self.jump(self.target)?;
            } else {
                self.sample.value = self.sample.value.clamp(-MAX_VALUE, MAX_VALUE);
                self.sample.velocity = self.sample.velocity.clamp(-MAX_VALUE, MAX_VALUE);
                if self.sample.done {
                    self.jump(self.target)?;
                }
            }
        }
        Ok(self.sample)
    }

    fn check_time(&self, now: Duration) -> Result<(), MotionError> {
        if now < self.observed {
            Err(MotionError::TimeWentBackwards)
        } else {
            Ok(())
        }
    }
}

fn valid_value(value: f32) -> Result<(), MotionError> {
    if value.is_finite() && value.abs() <= MAX_VALUE {
        Ok(())
    } else {
        Err(MotionError::InvalidValue)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MotionPolicy {
    #[default]
    Full,
    Reduced,
    Paused,
}

impl MotionPolicy {
    /// Background/inactive roots stop advancing their animation clock. An
    /// unavailable reduced-motion preference retains the normal policy.
    pub fn from_environment(environment: &crate::environment::EnvironmentSnapshot) -> Self {
        use crate::environment::Lifecycle;
        if environment.lifecycle != Lifecycle::Active {
            Self::Paused
        } else if environment.reduced_motion == Some(true) {
            Self::Reduced
        } else {
            Self::Full
        }
    }
}

#[derive(Clone, Debug)]
struct LayoutTrack {
    parent: Option<NodeId>,
    channels: [MotionValue; 4],
}

#[derive(Clone, Debug)]
pub struct LayoutMotion {
    lifetime: RootLifetime,
    bindings: BTreeMap<NodeId, Transition>,
    tracks: BTreeMap<NodeId, LayoutTrack>,
    wall: Option<Duration>,
    clock: Duration,
    policy: MotionPolicy,
    disposed: bool,
}

impl LayoutMotion {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            bindings: BTreeMap::new(),
            tracks: BTreeMap::new(),
            wall: None,
            clock: Duration::ZERO,
            policy: MotionPolicy::Full,
            disposed: false,
        }
    }

    pub fn transition(
        &mut self,
        node: NodeId,
        transition: Option<Transition>,
    ) -> Result<(), MotionError> {
        self.alive()?;
        if node.root != self.lifetime.id() {
            return Err(MotionError::ForeignRoot);
        }
        if let Some(transition) = transition {
            transition.validate()?;
            if !self.bindings.contains_key(&node) && self.bindings.len() >= MAX_TRACKS {
                return Err(MotionError::TrackLimit);
            }
            self.bindings.insert(node, transition);
        } else {
            self.bindings.remove(&node);
            self.tracks.remove(&node);
        }
        Ok(())
    }

    pub fn tracked_nodes(&self) -> usize {
        if self.alive().is_ok() {
            self.tracks.len()
        } else {
            0
        }
    }

    pub fn is_active(&self) -> bool {
        self.alive().is_ok()
            && self.policy == MotionPolicy::Full
            && self
                .tracks
                .values()
                .any(|track| track.channels.iter().any(|channel| !channel.current().done))
    }

    pub fn next_frame_delay(&self) -> Option<Duration> {
        self.is_active().then_some(Duration::from_millis(16))
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
        self.bindings.clear();
        self.tracks.clear();
    }

    pub(crate) fn apply(
        &mut self,
        lifetime: &RootLifetime,
        roots: &mut [PresentationNode],
        now: Duration,
        policy: MotionPolicy,
    ) -> Result<(), MotionError> {
        self.alive()?;
        if lifetime.id() != self.lifetime.id() {
            return Err(MotionError::ForeignRoot);
        }
        if self.wall.is_some_and(|wall| now < wall) {
            return Err(MotionError::TimeWentBackwards);
        }
        let mut candidate = self.clone();
        if let Some(wall) = candidate.wall {
            if candidate.policy != MotionPolicy::Paused && policy != MotionPolicy::Paused {
                candidate.clock += now - wall;
            }
        }
        candidate.wall = Some(now);
        candidate.policy = policy;
        let mut seen = BTreeSet::new();
        let mut frames = BTreeMap::new();
        for root in roots.iter() {
            candidate.project(root, None, [0.0; 2], [0.0; 2], &mut seen, &mut frames)?;
        }
        candidate.tracks.retain(|node, _| seen.contains(node));
        candidate.bindings.retain(|node, _| seen.contains(node));
        fn apply_frames(node: &mut PresentationNode, frames: &BTreeMap<NodeId, Rect>) {
            node.descriptor.frame = frames[&node.id];
            for child in &mut node.children {
                apply_frames(child, frames);
            }
        }
        for root in roots {
            apply_frames(root, &frames);
        }
        *self = candidate;
        Ok(())
    }

    fn alive(&self) -> Result<(), MotionError> {
        if self.disposed || self.lifetime.is_disposed() {
            Err(MotionError::Disposed)
        } else {
            Ok(())
        }
    }

    fn project(
        &mut self,
        node: &PresentationNode,
        parent: Option<NodeId>,
        target_parent: [f64; 2],
        visual_parent: [f64; 2],
        seen: &mut BTreeSet<NodeId>,
        frames: &mut BTreeMap<NodeId, Rect>,
    ) -> Result<(), MotionError> {
        if node.id.root != self.lifetime.id() {
            return Err(MotionError::ForeignRoot);
        }
        let target = node.descriptor.frame;
        let values = [
            target.x - target_parent[0],
            target.y - target_parent[1],
            target.width,
            target.height,
        ];
        if !target.is_valid()
            || values
                .iter()
                .any(|value| !value.is_finite() || value.abs() > f64::from(MAX_VALUE))
        {
            return Err(MotionError::InvalidValue);
        }
        let mut visual = values;
        if let Some(transition) = self.bindings.get(&node.id).copied() {
            seen.insert(node.id);
            let initial = values.map(|value| MotionValue::new(value as f32).unwrap());
            let track = self.tracks.entry(node.id).or_insert_with(|| LayoutTrack {
                parent,
                channels: initial,
            });
            let reset = track.parent != parent
                || policy_snap(self.policy)
                || target.width == 0.0
                || target.height == 0.0;
            for (index, channel) in track.channels.iter_mut().enumerate() {
                if reset {
                    channel.jump(values[index] as f32)?;
                } else if channel.target() != values[index] as f32 {
                    channel.animate_to(values[index] as f32, transition, self.clock)?;
                }
                visual[index] = f64::from(channel.sample(self.clock)?.value);
            }
            track.parent = parent;
        }
        let frame = Rect {
            x: visual_parent[0] + visual[0],
            y: visual_parent[1] + visual[1],
            width: visual[2].max(0.0),
            height: visual[3].max(0.0),
        };
        frames.insert(node.id, frame);
        for child in &node.children {
            self.project(
                child,
                Some(node.id),
                [target.x, target.y],
                [frame.x, frame.y],
                seen,
                frames,
            )?;
        }
        Ok(())
    }
}

fn policy_snap(policy: MotionPolicy) -> bool {
    policy == MotionPolicy::Reduced
}
