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

use jadix_core::reactivity::{use_signal, Signal};

use super::element_style::TimingFunction;

pub use super::physics::{
    find_spring, spring_step_closed, spring_unit_easing, GeneratorSample, Inertia, InertiaOptions,
    Spring, SpringOptions, SpringPhysics,
};

impl From<SpringPhysics> for TimingFunction {
    fn from(physics: SpringPhysics) -> Self {
        Self::Spring {
            stiffness: physics.stiffness,
            damping: physics.damping,
            mass: physics.mass.max(1e-4),
        }
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
    if !cfg!(feature = "animation-base") {
        return GeneratorSample {
            value: to,
            velocity: 0.0,
            done: true,
        };
    }
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
        Self::Inertia(InertiaOptions {
            velocity,
            ..InertiaOptions::default()
        })
    }
}

/// Where a stagger delay is measured from (Motion `stagger({ from })`).
#[cfg(feature = "animation-ex")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaggerFrom {
    First,
    Last,
    Center,
    Index(usize),
}

/// Delay in milliseconds for child `index` of `count` (Motion `stagger`).
#[cfg(feature = "animation-ex")]
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
    if let Some(ms) = crate::parse_css_time_ms_strict(raw) {
        return Some(SpringArg::Time(ms));
    }
    raw.parse().ok().map(SpringArg::Number)
}

fn parse_time_or_number(val: &str) -> Option<f32> {
    crate::parse_css_time_ms_strict(val).or_else(|| val.parse().ok())
}

fn parse_visual_seconds(val: &str) -> Option<f32> {
    if let Some(ms) = crate::parse_css_time_ms_strict(val) {
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
pub fn cycle_duration_clock(
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
        let from = if cfg!(feature = "animation-base") {
            from
        } else {
            to
        };
        let done = (from - to).abs() < 1e-5 && !matches!(driver, ValueDriver::Inertia(_));
        let done = done || !cfg!(feature = "animation-base");
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
        Self::spring(initial, initial)
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
        if !cfg!(feature = "animation-base") {
            let state = Rc::downgrade(&self.0);
            jadix_core::reactivity::create_render_effect(move || {
                let target = source.get();
                if let Some(state) = state.upgrade() {
                    AnimatedNumber(state).jump(target);
                }
            });
            return self;
        }
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
        if !cfg!(feature = "animation-base") {
            self.jump(to);
            return self;
        }
        self.0.borrow_mut().queue.push_back((to, driver));
        self
    }

    /// Re-arm from the live value. Same cell and [`Self::signal`] — do not
    /// replace the handle (a `bind_prop` from the first mount would keep the
    /// old signal and look like a dead click).
    pub fn restart(&self, to: f32, driver: ValueDriver) {
        if !cfg!(feature = "animation-base") {
            self.jump(to);
            return;
        }
        let mut s = self.0.borrow_mut();
        s.queue.clear();
        s.pending_target = None;
        s.repeat = RepeatMode::Once;
        s.repeats_left = Some(0);
        let from = s.value;
        start_segment(&mut s, from, to, Some(driver));
    }

    pub fn restart_from(&self, from: f32, to: f32, velocity: f32, driver: ValueDriver) {
        if !cfg!(feature = "animation-base") {
            self.jump(to);
            return;
        }
        let signal = {
            let mut state = self.0.borrow_mut();
            state.queue.clear();
            state.pending_target = None;
            state.repeat = RepeatMode::Once;
            state.repeats_left = Some(0);
            state.value = from;
            state.velocity = velocity;
            start_segment(&mut state, from, to, Some(driver));
            state.signal
        };
        let _ = signal.try_set(from);
    }

    /// Coalesced retarget (Motion `FollowAnimation.setTarget`). Applied on the
    /// next tick from the live position/velocity — many `set`s in one frame
    /// cost one retarget.
    pub fn set(&self, target: f32) {
        if !cfg!(feature = "animation-base") {
            self.jump(target);
            return;
        }
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
        if !cfg!(feature = "animation-base") {
            return;
        }
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
        if !cfg!(feature = "animation-base") {
            return;
        }
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

    /// Set a presentation value immediately, cancelling playback and velocity.
    pub fn jump(&self, value: f32) {
        assert!(value.is_finite(), "animation values must be finite");
        let (signal, finished) = {
            let mut state = self.0.borrow_mut();
            state.value = value;
            state.from = value;
            state.target = value;
            state.home = value;
            state.velocity = 0.0;
            state.elapsed_ms = 0.0;
            state.delay_ms = 0.0;
            state.active = false;
            state.paused = false;
            state.pending_target = None;
            state.follow = None;
            state.queue.clear();
            state.spring = None;
            state.inertia = None;
            (state.signal, state.finished)
        };
        let _ = signal.try_set(value);
        let _ = finished.try_set(true);
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
    if !cfg!(feature = "animation-base") {
        return false;
    }
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
    if !cfg!(feature = "animation-base") {
        return false;
    }
    VALUE_ANIMS.with(|list| {
        list.borrow()
            .iter()
            .any(|w| w.upgrade().is_some_and(|s| handle_needs_tick(&s.borrow())))
    })
}

#[cfg(test)]
#[path = "tests/generator.rs"]
mod tests;
