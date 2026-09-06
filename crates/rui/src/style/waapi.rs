//! Programmatic animation handle (id + registry).
//!
//! CSS `transition` / `animation` / `@keyframes` and this handle compile into the
//! same [`crate::style::Timing`] + keyframe sample kernel. Not a WAAPI spec clone:
//! no DocumentTimeline, no composite add/accumulate on this surface, no Promises.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::reactivity::Signal;

use super::animatable::PropertyId;
use super::element_style::{
    AnimationComposition, AnimationDirection, AnimationEntry, AnimationFillMode,
    AnimationIterationCount, AnimationPlayState, TimingFunction,
};
use super::keyframes::{register_keyframes, KeyframeStop};
use super::keyframes_parse::parse_declared_value;
use super::timing::Timing;

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
    static PLAYBACK: RefCell<HashMap<u64, Rc<RefCell<Playback>>>> =
        RefCell::new(HashMap::new());
}

struct Playback {
    play_state: AnimationPlayState,
    current_time_ms: f32,
    seek_ms: Option<f32>,
    playback_rate: f32,
    cancelled: bool,
    finished: Signal<bool>,
    cancelled_sig: Signal<bool>,
    finish_fired: bool,
    cancel_fired: bool,
    on_finish: Option<Rc<dyn Fn()>>,
    on_cancel: Option<Rc<dyn Fn()>>,
    effect: KeyframeEffect,
}

/// One keyframe object (`offset` + per-frame easing + CSS decls).
#[derive(Clone, Debug, Default)]
pub struct Keyframe {
    offset: Option<f32>,
    easing: Option<TimingFunction>,
    decls: Vec<(String, String)>,
}

impl Keyframe {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = Some(offset.clamp(0.0, 1.0));
        self
    }

    pub fn easing(mut self, easing: TimingFunction) -> Self {
        self.easing = Some(easing);
        self
    }

    /// CSS property bag entry (`"transform"`, `"color"`, …).
    pub fn css(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.decls.push((name.into(), value.into()));
        self
    }

    pub fn transform(self, value: impl Into<String>) -> Self {
        self.css("transform", value)
    }

    pub fn color(self, value: impl Into<String>) -> Self {
        self.css("color", value)
    }

    pub fn opacity(self, v: f32) -> Self {
        self.css("opacity", format!("{v}"))
    }

    pub fn scale(self, value: impl Into<String>) -> Self {
        self.css("scale", value)
    }

    pub fn background_color(self, value: impl Into<String>) -> Self {
        self.css("background-color", value)
    }
}

/// Timing snapshot in milliseconds (infinite iterations → `f32::INFINITY`).
/// Prefer [`KeyframeEffect::model`] / [`Timing`] at call sites.
#[derive(Clone, Copy, Debug)]
pub struct ComputedTiming {
    pub delay: f32,
    pub end_delay: f32,
    pub fill: AnimationFillMode,
    pub iterations: f32,
    pub duration: f32,
    pub direction: AnimationDirection,
    pub easing: TimingFunction,
    pub active_duration: f32,
}

/// CSS-like keyframe effect (named `@keyframes` or anonymous frames).
#[derive(Clone, Debug)]
pub struct KeyframeEffect {
    pub name: String,
    pub duration_ms: f32,
    pub timing: TimingFunction,
    pub delay_ms: f32,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub composition: AnimationComposition,
    pub end_delay_ms: f32,
}

impl KeyframeEffect {
    fn blank() -> Self {
        Self {
            name: String::new(),
            duration_ms: 0.0,
            timing: TimingFunction::Linear,
            delay_ms: 0.0,
            iteration_count: AnimationIterationCount::Finite(1.0),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            composition: AnimationComposition::Replace,
            end_delay_ms: 0.0,
        }
    }

    /// Timing-only effect (default `duration` is 0). Attach frames via [`Self::with_keyframes`].
    pub fn new() -> Self {
        Self::blank()
    }

    pub fn named(name: impl Into<String>) -> Self {
        let mut s = Self::blank();
        s.name = name.into();
        s
    }

    /// Register anonymous `@keyframes` from keyframe objects (need ≥ 2 frames).
    pub fn from_keyframes(frames: impl Into<Vec<Keyframe>>) -> Self {
        Self::blank().with_keyframes(frames)
    }

    pub fn with_keyframes(mut self, frames: impl Into<Vec<Keyframe>>) -> Self {
        let frames = frames.into();
        if frames.len() >= 2 {
            self.name = intern_keyframes(frames);
        }
        self
    }

    pub fn from_timing(timing: Timing) -> Self {
        let mut s = Self::blank();
        s.duration_ms = timing.duration_ms;
        s.timing = timing.easing;
        s.delay_ms = timing.delay_ms;
        s.end_delay_ms = timing.end_delay_ms;
        s.iteration_count = timing.iterations;
        s.direction = timing.direction;
        s.fill_mode = timing.fill;
        s
    }

    pub fn model(&self) -> Timing {
        Timing {
            duration_ms: self.duration_ms,
            delay_ms: self.delay_ms,
            end_delay_ms: self.end_delay_ms,
            easing: self.timing,
            iterations: self.iteration_count,
            direction: self.direction,
            fill: self.fill_mode,
        }
    }

    pub fn duration(mut self, ms: f32) -> Self {
        self.duration_ms = ms.max(0.0);
        self
    }

    pub fn timing(mut self, timing: TimingFunction) -> Self {
        self.timing = timing;
        self
    }

    /// Same as [`Self::timing`].
    pub fn easing(self, timing: TimingFunction) -> Self {
        self.timing(timing)
    }

    pub fn delay(mut self, ms: f32) -> Self {
        self.delay_ms = ms;
        self
    }

    pub fn end_delay(mut self, ms: f32) -> Self {
        self.end_delay_ms = ms.max(0.0);
        self
    }

    pub fn iterations(mut self, n: f32) -> Self {
        self.iteration_count = if n.is_infinite() {
            AnimationIterationCount::Infinite
        } else {
            AnimationIterationCount::Finite(n.max(0.0))
        };
        self
    }

    pub fn fill(mut self, fill: AnimationFillMode) -> Self {
        self.fill_mode = fill;
        self
    }

    pub fn direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn get_computed_timing(&self) -> ComputedTiming {
        let t = self.model();
        let iterations = match t.iterations {
            AnimationIterationCount::Infinite => f32::INFINITY,
            AnimationIterationCount::Finite(n) => n.max(0.0),
        };
        ComputedTiming {
            delay: t.delay_ms,
            end_delay: t.end_delay_ms,
            fill: t.fill,
            iterations,
            duration: t.duration_ms,
            direction: t.direction,
            easing: t.easing,
            active_duration: t.active_duration_ms(),
        }
    }

    pub fn active_duration_ms(&self) -> f32 {
        self.model().active_duration_ms()
    }
}

impl Default for KeyframeEffect {
    fn default() -> Self {
        Self::blank()
    }
}

/// Playback handle. Clone shares the same registry cell.
#[derive(Clone, Copy, Debug)]
pub struct Animation {
    id: u64,
}

impl Animation {
    pub fn from_effect(effect: KeyframeEffect) -> Self {
        let id = NEXT_ID.with(|c| {
            let n = c.get();
            c.set(n.saturating_add(1).max(1));
            n
        });
        let inner = Rc::new(RefCell::new(Playback {
            play_state: AnimationPlayState::Running,
            current_time_ms: 0.0,
            seek_ms: None,
            playback_rate: 1.0,
            cancelled: false,
            finished: Signal::new_unowned(false),
            cancelled_sig: Signal::new_unowned(false),
            finish_fired: false,
            cancel_fired: false,
            on_finish: None,
            on_cancel: None,
            effect,
        }));
        PLAYBACK.with(|m| m.borrow_mut().insert(id, inner));
        Self { id }
    }

    /// Programmatic start: frames + [`Timing`] (Rust subset).
    pub fn run(frames: impl Into<Vec<Keyframe>>, timing: Timing) -> Self {
        Self::from_effect(KeyframeEffect::from_timing(timing).with_keyframes(frames))
    }

    /// Same as [`Self::run`] with a pre-built effect (named `@keyframes` or frames).
    pub fn animate(frames: impl Into<Vec<Keyframe>>, timing: KeyframeEffect) -> Self {
        Self::from_effect(timing.with_keyframes(frames))
    }

    pub fn from_id(id: u64) -> Option<Self> {
        lookup(id).map(|_| Self { id })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn play(&self) {
        if let Some(p) = lookup(self.id) {
            let mut g = p.borrow_mut();
            let restart = g.finish_fired || g.cancelled;
            g.cancelled = false;
            g.cancel_fired = false;
            let _ = g.cancelled_sig.try_set(false);
            g.play_state = AnimationPlayState::Running;
            if g.finish_fired {
                g.finish_fired = false;
                let _ = g.finished.try_set(false);
            }
            if restart {
                if g.playback_rate >= 0.0 {
                    g.seek_ms = Some(0.0);
                } else {
                    let end = g.effect.active_duration_ms();
                    g.seek_ms = Some(if end.is_finite() { end } else { 0.0 });
                }
            }
        }
    }

    pub fn pause(&self) {
        if let Some(p) = lookup(self.id) {
            p.borrow_mut().play_state = AnimationPlayState::Paused;
        }
    }

    /// Flip `playbackRate` and play.
    pub fn reverse(&self) {
        if let Some(p) = lookup(self.id) {
            let mut g = p.borrow_mut();
            g.playback_rate = -g.playback_rate;
        }
        self.play();
    }

    pub fn set_rate(&self, rate: f32) {
        self.update_playback_rate(rate);
    }

    pub fn playback_rate(&self) -> f32 {
        lookup(self.id)
            .map(|p| p.borrow().playback_rate)
            .unwrap_or(1.0)
    }

    /// Set `playbackRate` (negative rewinds).
    pub fn update_playback_rate(&self, rate: f32) {
        if let Some(p) = lookup(self.id) {
            p.borrow_mut().playback_rate = rate;
        }
    }

    pub fn seek(&self, ms: f32) {
        self.set_current_time(ms);
    }

    pub fn cancel(&self) {
        if let Some(p) = lookup(self.id) {
            let mut g = p.borrow_mut();
            g.cancelled = true;
            g.play_state = AnimationPlayState::Paused;
            g.seek_ms = Some(0.0);
            fire_cancel(&mut g);
        }
    }

    /// Jump to the end of a finite effect and mark finished.
    pub fn finish(&self) {
        let end = lookup(self.id)
            .map(|p| {
                let e = &p.borrow().effect;
                match e.iteration_count {
                    AnimationIterationCount::Infinite => e.duration_ms,
                    AnimationIterationCount::Finite(n) => e.duration_ms * n.max(0.0),
                }
                .max(0.0)
                    + e.delay_ms
            })
            .unwrap_or(0.0);
        self.set_current_time(end);
        if let Some(p) = lookup(self.id) {
            let mut g = p.borrow_mut();
            g.play_state = AnimationPlayState::Paused;
            fire_finish(&mut g);
        }
    }

    pub fn current_time(&self) -> f32 {
        lookup(self.id)
            .map(|p| p.borrow().current_time_ms)
            .unwrap_or(0.0)
    }

    pub fn set_current_time(&self, ms: f32) {
        if let Some(p) = lookup(self.id) {
            let mut g = p.borrow_mut();
            g.seek_ms = Some(ms.max(0.0));
            g.cancelled = false;
            g.cancel_fired = false;
            g.finish_fired = false;
            let _ = g.finished.try_set(false);
            let _ = g.cancelled_sig.try_set(false);
        }
    }

    pub fn play_state(&self) -> AnimationPlayState {
        lookup(self.id)
            .map(|p| p.borrow().play_state)
            .unwrap_or(AnimationPlayState::Running)
    }

    pub fn effect(&self) -> KeyframeEffect {
        lookup(self.id)
            .map(|p| p.borrow().effect.clone())
            .unwrap_or_else(KeyframeEffect::blank)
    }

    pub fn get_computed_timing(&self) -> ComputedTiming {
        self.effect().get_computed_timing()
    }

    pub fn finished(&self) -> Signal<bool> {
        lookup(self.id)
            .map(|p| p.borrow().finished)
            .unwrap_or_else(|| Signal::new_unowned(true))
    }

    pub fn cancelled(&self) -> Signal<bool> {
        lookup(self.id)
            .map(|p| p.borrow().cancelled_sig)
            .unwrap_or_else(|| Signal::new_unowned(true))
    }

    pub fn on_finish(&self, f: impl Fn() + 'static) {
        if let Some(p) = lookup(self.id) {
            p.borrow_mut().on_finish = Some(Rc::new(f));
        }
    }

    pub fn on_cancel(&self, f: impl Fn() + 'static) {
        if let Some(p) = lookup(self.id) {
            p.borrow_mut().on_cancel = Some(Rc::new(f));
        }
    }

    /// CSS `animation` list item wired to this player.
    pub fn to_entry(&self) -> AnimationEntry {
        let effect = self.effect();
        AnimationEntry {
            name: effect.name,
            duration_ms: effect.duration_ms,
            timing: effect.timing,
            delay_ms: effect.delay_ms,
            iteration_count: effect.iteration_count,
            direction: effect.direction,
            fill_mode: effect.fill_mode,
            play_state: self.play_state(),
            composition: effect.composition,
            playback_id: Some(self.id),
            end_delay_ms: effect.end_delay_ms,
            ..Default::default()
        }
    }
}

/// Live, non-cancelled handles in this thread's registry.
pub fn get_animations() -> Vec<Animation> {
    PLAYBACK.with(|m| {
        m.borrow()
            .iter()
            .filter(|(_, p)| !p.borrow().cancelled)
            .map(|(id, _)| Animation { id: *id })
            .collect()
    })
}

fn intern_keyframes(frames: Vec<Keyframe>) -> String {
    let n = frames.len();
    let specified: Vec<Option<f32>> = frames.iter().map(|k| k.offset).collect();
    let offsets = computed_offsets(n, &specified);
    let mut stops = Vec::with_capacity(n);
    for (frame, off) in frames.into_iter().zip(offsets) {
        let mut props = std::collections::BTreeMap::new();
        for (k, v) in &frame.decls {
            if let Some(id) = PropertyId::from_css(k) {
                if let Some(av) = parse_declared_value(id, v) {
                    props.insert(id, av);
                }
            }
        }
        let mut stop = KeyframeStop::at(off).with_props(props);
        if let Some(t) = frame.easing {
            stop = stop.timing(t);
        }
        stops.push(stop);
    }
    let id = NEXT_ID.with(|c| {
        let n = c.get();
        c.set(n.saturating_add(1).max(1));
        n
    });
    let name = format!("__waapi_{id}");
    register_keyframes(&name, stops);
    name
}

fn computed_offsets(n: usize, specified: &[Option<f32>]) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let mut offs = specified.to_vec();
    if offs[0].is_none() {
        offs[0] = Some(0.0);
    }
    if offs[n - 1].is_none() {
        offs[n - 1] = Some(1.0);
    }
    let mut i = 1usize;
    while i < n {
        if offs[i].is_some() {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && offs[i].is_none() {
            i += 1;
        }
        let left = offs[start - 1].unwrap();
        let right = offs[i].unwrap();
        let holes = i - start;
        let denom = (holes + 1) as f32;
        for (k, slot) in offs[start..i].iter_mut().enumerate() {
            *slot = Some(left + (right - left) * ((k + 1) as f32) / denom);
        }
        i += 1;
    }
    offs.into_iter()
        .map(|o| o.unwrap().clamp(0.0, 1.0))
        .collect()
}

fn lookup(id: u64) -> Option<Rc<RefCell<Playback>>> {
    PLAYBACK.with(|m| m.borrow().get(&id).cloned())
}

fn fire_finish(g: &mut Playback) {
    if g.finish_fired || g.cancelled {
        return;
    }
    g.finish_fired = true;
    let _ = g.finished.try_set(true);
    if let Some(cb) = g.on_finish.clone() {
        cb();
    }
}

fn fire_cancel(g: &mut Playback) {
    if g.cancel_fired {
        return;
    }
    g.cancel_fired = true;
    let _ = g.cancelled_sig.try_set(true);
    if let Some(cb) = g.on_cancel.clone() {
        cb();
    }
}

/// Copy play-state / pending seek into the CSS entry + run clock. Tick calls this.
pub(crate) fn prepare_entry(
    entry: &mut AnimationEntry,
    run: &mut super::animation::AnimationRunState,
) {
    let Some(id) = entry.playback_id else {
        return;
    };
    let Some(p) = lookup(id) else {
        return;
    };
    let mut g = p.borrow_mut();
    entry.play_state = g.play_state;
    if let Some(seek) = g.seek_ms.take() {
        run.elapsed_ms = seek;
        run.finished = false;
        g.current_time_ms = seek;
    }
    if g.cancelled {
        entry.play_state = AnimationPlayState::Paused;
    }
}

pub(crate) fn playback_rate(id: u64) -> f32 {
    lookup(id).map(|p| p.borrow().playback_rate).unwrap_or(1.0)
}

pub(crate) fn is_cancelled(id: u64) -> bool {
    lookup(id).is_some_and(|p| p.borrow().cancelled)
}

pub(crate) fn observe_time(id: u64, elapsed_ms: f32, run_finished: bool) {
    let Some(p) = lookup(id) else {
        return;
    };
    let mut g = p.borrow_mut();
    g.current_time_ms = elapsed_ms;
    if run_finished && !g.cancelled {
        fire_finish(&mut g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::animation::{tick_element_animations, AnimationSnapshot};

    #[test]
    fn play_pause_current_time_and_finished_signal() {
        let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
        let mut style = crate::style::Style::default();
        style.animations.push(anim.to_entry());
        let mut snap = AnimationSnapshot::default();
        assert!(tick_element_animations(&mut style, &mut snap, 0.08));
        let t0 = anim.current_time();
        assert!(t0 > 0.0, "got {t0}");
        anim.pause();
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        let paused = anim.current_time();
        assert!(
            (paused - t0).abs() < 1.0,
            "pause must freeze, {paused} vs {t0}"
        );
        anim.set_current_time(500.0);
        anim.play();
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!((anim.current_time() - 500.0).abs() < 1.0);
        anim.finish();
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!(anim.finished().get());
    }

    #[test]
    fn cancel_does_not_finish_and_fires_on_cancel() {
        let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
        let fired = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = fired.clone();
        anim.on_cancel(move || flag.set(true));
        anim.on_finish(|| panic!("cancel must not fire onfinish"));
        anim.cancel();
        assert!(anim.cancelled().get());
        assert!(!anim.finished().get());
        assert!(fired.get());
        assert!(get_animations().iter().all(|a| a.id() != anim.id()));
    }

    #[test]
    fn reverse_flips_playback_rate() {
        let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
        assert!((anim.playback_rate() - 1.0).abs() < 1e-6);
        anim.reverse();
        assert!((anim.playback_rate() + 1.0).abs() < 1e-6);
        anim.update_playback_rate(2.0);
        assert!((anim.playback_rate() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn computed_timing_active_duration() {
        let e = KeyframeEffect::named("spin")
            .duration(8000.0)
            .iterations(2.0)
            .end_delay(100.0);
        let t = e.model();
        assert!((t.duration_ms - 8000.0).abs() < 1e-3);
        assert!((t.active_duration_ms() - 16000.0).abs() < 1e-3);
        assert!((t.end_delay_ms - 100.0).abs() < 1e-3);
        let inf = KeyframeEffect::named("spin")
            .duration(3000.0)
            .iterations(f32::INFINITY)
            .model();
        assert!(matches!(
            inf.iterations,
            crate::style::AnimationIterationCount::Infinite
        ));
        assert!(inf.active_duration_ms().is_infinite());
    }

    #[test]
    fn run_from_timing_handle() {
        let anim = Animation::run(
            vec![Keyframe::new().opacity(0.0), Keyframe::new().opacity(1.0)],
            crate::style::Timing::default().duration(200.0),
        );
        anim.set_rate(0.5);
        anim.seek(20.0);
        let mut style = crate::style::Style::default();
        style.animations.push(anim.to_entry());
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!((anim.current_time() - 20.0).abs() < 1.0);
    }

    #[test]
    fn keyframe_objects_offset_and_default_duration_zero() {
        let anim = Animation::animate(
            vec![
                Keyframe::new().opacity(0.0),
                Keyframe::new().opacity(1.0).offset(0.3),
                Keyframe::new().opacity(0.0),
            ],
            KeyframeEffect::new().duration(1000.0),
        );
        assert!(anim.effect().name.starts_with("__waapi_"));
        assert!((KeyframeEffect::new().duration_ms).abs() < 1e-6);
        let mut style = crate::style::Style::default();
        style.opacity = 1.0;
        style.animations.push(anim.to_entry());
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.15);
        assert!(
            style.opacity > 0.05 && style.opacity < 0.95,
            "offset 0.3 midpoint should be mid opacity, got {}",
            style.opacity
        );
        assert!(get_animations().iter().any(|a| a.id() == anim.id()));
    }

    #[test]
    fn negative_rate_rewinds_current_time() {
        let anim = Animation::from_effect(KeyframeEffect::named("spin").duration(1000.0));
        let mut style = crate::style::Style::default();
        style.animations.push(anim.to_entry());
        let mut snap = AnimationSnapshot::default();
        for _ in 0..8 {
            let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        }
        let t0 = anim.current_time();
        assert!(t0 > 400.0, "got {t0}");
        anim.update_playback_rate(-1.0);
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        let t1 = anim.current_time();
        assert!(t1 < t0, "rate -1 must rewind, {t1} vs {t0}");
    }
}
