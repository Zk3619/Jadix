use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::reactivity::{use_signal, Signal};

thread_local! {
    static CUBIC_TWEENS: RefCell<Vec<Weak<RefCell<CubicTweenState>>>> = const { RefCell::new(Vec::new()) };
}

/// CSS `cubic-bezier(p1x, p1y, p2x, p2y)`：给定归一化时间 `x∈[0,1]`，求曲线上对应 `y`（用于对标 `transition-timing-function`）。
#[inline]
pub fn cubic_bezier_y_at_x(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    fn bezier_x(u: f32, x1: f32, x2: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        let inv = 1.0 - u;
        3.0 * inv * inv * u * x1 + 3.0 * inv * u * u * x2 + u * u * u
    }
    fn bezier_y(u: f32, y1: f32, y2: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        let inv = 1.0 - u;
        3.0 * inv * inv * u * y1 + 3.0 * inv * u * u * y2 + u * u * u
    }
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    for _ in 0..28 {
        let mid = (lo + hi) * 0.5;
        if bezier_x(mid, x1, x2) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let u = (lo + hi) * 0.5;
    bezier_y(u, y1, y2)
}

pub struct CubicTweenState {
    pub target: f32,
    pub value: f32,
    pub from_value: f32,
    pub elapsed: f32,
    pub duration: f32,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub active: bool,
    pub signal: Signal<f32>,
}

#[derive(Clone)]
pub struct CubicTween(Rc<RefCell<CubicTweenState>>);

impl CubicTween {
    /// 默认 `cubic-bezier(0.4, 0, 0.2, 1)`，时长约 150ms，对标常见 `transition`。
    pub fn new_ui_transition(initial: f32) -> Self {
        Self::new_with_curve(initial, 0.15, (0.4, 0.0, 0.2, 1.0))
    }

    pub fn new_with_curve(initial: f32, duration_secs: f32, ctrl: (f32, f32, f32, f32)) -> Self {
        let (x1, y1, x2, y2) = ctrl;
        let state = Rc::new(RefCell::new(CubicTweenState {
            target: initial,
            value: initial,
            from_value: initial,
            elapsed: 0.0,
            duration: duration_secs.max(1e-6),
            x1,
            y1,
            x2,
            y2,
            active: false,
            signal: Signal::new_unowned(initial),
        }));
        CUBIC_TWEENS.with(|anim| {
            anim.borrow_mut().push(Rc::downgrade(&state));
        });
        Self(state)
    }

    pub fn set(&self, target: f32) {
        let mut s = self.0.borrow_mut();
        // 目标未变时不要重置 `elapsed`：chrome sync 每帧会再 `set` 同一目标，
        // 若此处重启，有效时长会远长于 `duration`（asymptotic crawl）。
        if (s.target - target).abs() < 1e-5 {
            return;
        }
        s.from_value = s.value;
        s.target = target;
        s.elapsed = 0.0;
        s.active = (s.value - target).abs() >= 1e-5;
        let _ = s.signal.try_set_silent(s.value);
    }

    pub fn get(&self) -> f32 {
        self.0.borrow().signal.get()
    }

    pub fn peek(&self) -> f32 {
        let s = self.0.borrow();
        s.signal.try_peek().unwrap_or(s.value)
    }

    pub fn is_active(&self) -> bool {
        self.0.borrow().active
    }
}

pub fn use_ui_transition_cubic(id_str: &str, initial: f32) -> CubicTween {
    let sig = use_signal(id_str, move || CubicTween::new_ui_transition(initial));
    sig.peek()
}

/// [`tick_animations`] 的返回值：`any` 决定是否继续 `request_redraw`；`scrollbar_overlay` 保留字段兼容（现恒为 `false`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationTick {
    pub any: bool,
    /// 曾用于未 settle 的滚动条可见性弹簧；Spring 移除后恒为 `false`。
    pub scrollbar_overlay: bool,
}

pub fn tick_animations(dt: f32) -> AnimationTick {
    // 与滚动同口径：首屏连帧 / Wait 后首帧的 raw `dt` 常为 ≈0 或数秒。
    // 不夹紧时：≈0 → 动画看似失效；过大 → 一帧瞬跳到目标。
    const DT_MIN: f32 = 1.0 / 240.0;
    const DT_MAX: f32 = 0.08;
    let dt = dt.clamp(DT_MIN, DT_MAX);

    let mut cubic_running = false;
    let mut cubic_updates = Vec::new();
    CUBIC_TWEENS.with(|anim| {
        let mut list = anim.borrow_mut();
        let mut i = 0;
        while i < list.len() {
            if let Some(tw) = list[i].upgrade() {
                let mut s = tw.borrow_mut();
                if s.active {
                    s.elapsed += dt;
                    let sample = generator_tween_sample(
                        s.from_value,
                        s.target,
                        s.elapsed * 1000.0,
                        TweenOptions {
                            duration_ms: s.duration * 1000.0,
                            easing: TimingFunction::CubicBezier(s.x1, s.y1, s.x2, s.y2),
                        },
                    );
                    s.value = sample.value;
                    if sample.done {
                        s.value = s.target;
                        s.active = false;
                    } else {
                        cubic_running = true;
                    }
                    cubic_updates.push((s.signal, s.value));
                }
                i += 1;
            } else {
                list.remove(i);
            }
        }
    });
    for (sig, val) in cubic_updates {
        // Silent: UI chrome reads tweens via `peek` + per-frame sync. Notifying here
        // flushed effects / risked root rebuild and wiped live Inspector style edits.
        let _ = sig.try_set_silent(val);
    }

    let timeline_any = super::timeline::tick_timelines(dt);
    let generator_any = super::generator::tick_value_generators(dt);

    AnimationTick {
        any: cubic_running || timeline_any || generator_any,
        scrollbar_overlay: false,
    }
}

/// 是否仍有未 settle 的 cubic tween / timeline（不推进时间）。
/// chrome sync（Switch 等）常在本帧 `tick_animations` **之后**才 `set` 目标；
/// render 结束后用此查询补 `request_redraw`，避免动画停在第 0 帧。
pub fn any_ui_animation_active() -> bool {
    CUBIC_TWEENS.with(|anim| {
        anim.borrow()
            .iter()
            .any(|w| w.upgrade().is_some_and(|s| s.borrow().active))
    }) || super::timeline::any_timeline_playing()
        || super::generator::any_value_generator_active()
}

thread_local! {
    /// Set during chrome when `@keyframes` / `animate-spin` / shimmer still need vsync.
    /// `any_ui_animation_active` is cubic tweens only; without this, `about_to_wait`
    /// `WaitUntil` (scrollbar hide after a wheel) sleeps and every CSS clock stops.
    static PAGE_CSS_NEEDS_FRAME: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn begin_page_css_frame() {
    PAGE_CSS_NEEDS_FRAME.with(|c| c.set(false));
}

pub(crate) fn note_page_css_needs_frame() {
    PAGE_CSS_NEEDS_FRAME.with(|c| c.set(true));
}

/// Infinite CSS / spin / shimmer still sampling. Same role as [`crate::utils::modal_motion::any_modal_css_needs_frame`].
pub fn any_page_css_needs_frame() -> bool {
    PAGE_CSS_NEEDS_FRAME.with(|c| c.get())
}

// ─── CSS Transition engine ───────────────────────────────────────────────
//
// CSS-like model: progress = (timeline_now − start) / duration.
// Timeline source is a **monotonic** clock (`std::time::Instant`), never
// `SystemTime` / wall clock (NTP jumps must not seek animations).
// Each frame *samples* the timeline; it does not invent time by summing clamped `dt`.
// After `Wait`, Instant can jump past `duration` — duration tweens cap that delta
// per tick (`CSS_TRANSITION_DEFAULT_MAX_STEP_MS`) so ease-out cannot skip mids.

use super::clip_path::ClipPath;
use super::element_style::{
    AnimationComposition, AnimationDirection, AnimationEntry, AnimationFillMode,
    AnimationIterationCount, AnimationPlayState, AnimationTimeline, BoxShadow, TimingFunction,
    TransitionEntry, TransitionProperty, UnitSlot,
};
use super::filter::{interpolate_filter_lists, FilterList};
use super::generator::{
    tween_sample as generator_tween_sample, Spring, SpringPhysics, TweenOptions,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::time::Instant;

/// After `ControlFlow::Wait` or a hitch, Instant can jump past `duration` /
/// a spring's start. Cap **only** those idle jumps (see
/// [`duration_tween_elapsed_ms`]); normal frames keep wall-clock progress so
/// 30/60 Hz does not crawl.
const CSS_TRANSITION_IDLE_JUMP_MS: f32 = 80.0;
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

fn css_timeline_now_ms() -> f64 {
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

thread_local! {
    static PREFERS_REDUCED_MOTION: Cell<Option<bool>> = const { Cell::new(None) };
    static TRANSITION_TEXT: RefCell<Option<*mut super::text_style::TextStyle>> =
        const { RefCell::new(None) };
    static TRANSITION_USED: Cell<Option<*const super::used_layout::UsedLayout>> =
        const { Cell::new(None) };
}

/// Bind `TextStyle` for color / font-size transition ticks (Element path).
pub fn tick_element_transitions_with_text(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    snapshot: &mut TransitionSnapshot,
    lock: AnimationLock,
    dt_secs: f32,
) -> bool {
    tick_element_transitions_with_text_used(style, text, None, snapshot, lock, dt_secs)
}

/// Same as [`tick_element_transitions_with_text`], with `used_layout` for `interpolate-size`.
pub fn tick_element_transitions_with_text_used(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    used: Option<&super::used_layout::UsedLayout>,
    snapshot: &mut TransitionSnapshot,
    lock: AnimationLock,
    dt_secs: f32,
) -> bool {
    TRANSITION_TEXT.with(|c| *c.borrow_mut() = Some(text as *mut _));
    TRANSITION_USED.set(used.map(|u| u as *const _));
    let r = tick_element_transitions_locked(style, snapshot, lock, dt_secs);
    TRANSITION_TEXT.with(|c| *c.borrow_mut() = None);
    TRANSITION_USED.set(None);
    r
}

fn transition_tick_used() -> Option<&'static super::used_layout::UsedLayout> {
    TRANSITION_USED.get().map(|ptr| {
        // SAFETY: pointer is set only for the duration of `tick_element_transitions_with_text_used`
        // on the same thread; no re-entrant tick.
        unsafe { &*ptr }
    })
}

fn transition_tick_text() -> Option<&'static mut super::text_style::TextStyle> {
    TRANSITION_TEXT.with(|c| {
        let p = *c.borrow();
        p.map(|ptr| {
            // SAFETY: pointer is set only for the duration of `tick_element_transitions_with_text`
            // on the same thread; no re-entrant tick.
            unsafe { &mut *ptr }
        })
    })
}

/// Override `prefers-reduced-motion` (tests / settings). `None` restores auto-detect.
pub fn set_prefers_reduced_motion(on: Option<bool>) {
    PREFERS_REDUCED_MOTION.set(on);
}

/// CSS `prefers-reduced-motion: reduce` — durations snap to the end of the interval.
pub fn prefers_reduced_motion() -> bool {
    if let Some(v) = PREFERS_REDUCED_MOTION.get() {
        return v;
    }
    match std::env::var("RUI_REDUCED_MOTION") {
        Ok(s) if s == "1" || s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("reduce") => {
            true
        }
        _ => false,
    }
}

/// Per-property in-flight tween state (CSS transition segment).
#[derive(Clone, Debug)]
struct PropertyTween {
    /// Packed components (opacity=1, color=4, transform=7: tx/ty/sx/sy/rotate/tpx/tpy).
    from: [f32; 8],
    to: [f32; 8],
    /// Number of meaningful components (1 scalar, 4 color/translate+pct, 7 transform).
    components: u8,
    /// Monotonic timeline time (ms) when this segment started.
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    timing: TimingFunction,
    active: bool,
    /// What this engine last wrote into `Style` (the CSS *used value*).
    ///
    /// A browser keeps the cascaded *specified* value and the animated *used* value in
    /// separate places. Here there is a single `Style`, so without this receipt the next
    /// frame would read back our own mid-tween value, mistake it for a new target, and
    /// retarget the tween onto itself — every transition would die after one frame
    ///     unless its owner rewrote the target on every single frame.
    written: Option<[f32; 8]>,
    vel: [f32; 8],
    last_ms: f64,
    /// Held closed-form springs; `next(now − start)` — not a second Euler loop.
    springs: [Option<Spring>; 8],
}

/// Filter list tween (cannot fit in `[f32; 8]`).
#[derive(Clone, Debug)]
struct FilterTween {
    from: Option<FilterList>,
    to: Option<FilterList>,
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    timing: TimingFunction,
    active: bool,
    /// Last value written into `Style`; see [`PropertyTween::written`].
    written: Option<Option<FilterList>>,
}

/// `clip-path` tween — same interpolator as keyframes (`ClipPath::interpolate`).
#[derive(Clone, Debug)]
struct ClipPathTween {
    from: ClipPath,
    to: ClipPath,
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    timing: TimingFunction,
    active: bool,
    written: Option<ClipPath>,
}

/// `box-shadow` list tween (primary `shadow_*` + `extra_shadows`).
#[derive(Clone, Debug)]
struct ShadowListTween {
    from: Vec<BoxShadow>,
    to: Vec<BoxShadow>,
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    timing: TimingFunction,
    active: bool,
    written: Option<Vec<BoxShadow>>,
}

/// Per-element transition snapshot stored alongside the element.
#[derive(Clone, Debug, Default)]
pub struct TransitionSnapshot {
    tweens: HashMap<TransitionProperty, PropertyTween>,
    filter_tween: Option<FilterTween>,
    clip_tween: Option<ClipPathTween>,
    shadow_tween: Option<ShadowListTween>,
    /// Last tick ended the final active tween. Dest-reset hide ghosts once.
    settled_this_tick: bool,
}

impl TransitionSnapshot {
    /// True while at least one property tween is still interpolating.
    pub(crate) fn has_active_tweens(&self) -> bool {
        self.tweens.values().any(|t| t.active)
            || self.filter_tween.as_ref().is_some_and(|t| t.active)
            || self.clip_tween.as_ref().is_some_and(|t| t.active)
            || self.shadow_tween.as_ref().is_some_and(|t| t.active)
    }

    /// True if this node has ever seeded a transition snapshot (active or settled).
    pub(crate) fn has_any_tweens(&self) -> bool {
        !self.tweens.is_empty()
            || self.filter_tween.is_some()
            || self.clip_tween.is_some()
            || self.shadow_tween.is_some()
    }

    pub(crate) fn prop_tween_active(&self, prop: TransitionProperty) -> bool {
        self.tweens.get(&prop).is_some_and(|t| t.active)
    }

    /// Peek the hide-settle latch (`push_frame_paint_element` hide dest-reset).
    pub(crate) fn settled_this_tick(&self) -> bool {
        self.settled_this_tick
    }

    #[cfg(test)]
    pub(crate) fn mark_settled_this_tick(&mut self) {
        self.settled_this_tick = true;
    }

    /// Write last-used (computed) values from this snapshot into `style`.
    fn write_used_values_into(&self, style: &mut super::Style) {
        for (prop, tween) in &self.tweens {
            if let Some(written) = tween.written {
                write_prop(style, *prop, &written, tween.components);
            } else if tween.active {
                write_prop(style, *prop, &tween.from, tween.components);
            }
        }
        if let Some(ft) = &self.filter_tween {
            if let Some(written) = &ft.written {
                style.filter = written.clone();
            }
        }
        if let Some(ct) = &self.clip_tween {
            if let Some(written) = &ct.written {
                style.clip_path = written.clone();
            }
        }
        if let Some(st) = &self.shadow_tween {
            if let Some(written) = &st.written {
                write_shadow_layers(style, written);
            }
        }
    }
}

/// Properties a running `@keyframes` currently drives.
///
/// CSS resolves `animation` **after** `transition` and never lets an animation-driven
/// change start a transition. Both engines write the same `Style` here, so the animation
/// publishes what it owns and [`tick_element_transitions_locked`] steps aside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationLock {
    pub opacity: bool,
    /// Covers `translate` / `scale` / `rotate` (this engine's transform bundle).
    pub transform: bool,
    pub max_height: bool,
    pub ids: super::animatable::PropertySet,
}

impl AnimationLock {
    pub fn any(self) -> bool {
        self.opacity || self.transform || self.max_height || !self.ids.is_empty()
    }
}

fn animation_owns_prop(prop: TransitionProperty, lock: AnimationLock) -> bool {
    if let Some(id) = super::animatable::PropertyId::from_transition(prop) {
        if lock.ids.contains(id) {
            return true;
        }
    }
    match prop {
        TransitionProperty::Opacity => lock.opacity,
        TransitionProperty::Transform
        | TransitionProperty::Translate
        | TransitionProperty::Scale
        | TransitionProperty::Rotate => lock.transform,
        TransitionProperty::MaxHeight => lock.max_height,
        _ => false,
    }
}

/// Baseline visual props captured when an animation starts (fill-mode / restore).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationBaseline {
    pub opacity: f32,
    pub rotate_deg: f32,
    pub scale: [f32; 2],
    pub translate: [f32; 2],
    /// Cascaded `max-height` (`auto` must stay `auto` — treating it as 0px collapses boxes).
    pub max_height: taffy::style::Dimension,
    /// Cascaded `min-height` before any `@keyframes` max-height pin.
    pub min_height: taffy::style::Dimension,
}

impl AnimationBaseline {
    pub fn from_style(style: &super::Style) -> Self {
        Self {
            opacity: style.opacity,
            rotate_deg: style.rotate_deg,
            scale: style.scale,
            translate: style.translate,
            max_height: style.max_size.height,
            min_height: style.min_size.height,
        }
    }
}

/// Per-list-item runtime for CSS `animation`.
#[derive(Clone, Debug)]
pub struct AnimationRunState {
    pub elapsed_ms: f32,
    pub baseline: AnimationBaseline,
    pub finished: bool,
    /// Fingerprint of the driving [`AnimationEntry`] so style edits restart cleanly.
    pub entry_fp: u64,
}

/// Per-element CSS animation runtime (persists across frames).
#[derive(Clone, Debug, Default)]
pub struct AnimationSnapshot {
    pub runs: Vec<AnimationRunState>,
    /// Cascaded value the keyframes are layered over, re-applied before every sample.
    base: Option<AnimationBaseline>,
    /// What the sampler wrote last frame, used to tell our own output apart from a
    /// fresh assignment by the component (same trick as [`PropertyTween::written`]).
    written: Option<AnimationBaseline>,
    lock: AnimationLock,
    /// Specified slot for style-only ticks (Element also has `specified_present`).
    pub specified: super::specified::SpecifiedPresent,
    written_props: HashMap<super::animatable::PropertyId, super::animatable::AnimatableValue>,
    /// Last CSS-timeline sample (ms). CursorMoved extra paints often arrive with
    /// `dt≈0`; without this, summing frame delta freezes `animate-spin`.
    last_tick_ms: Option<f64>,
}

impl AnimationSnapshot {
    /// Drop the CSS-timeline stamp so the next tick advances by the caller-supplied
    /// `dt` only. Modal shell already caps that `dt`; keeping `last_tick_ms` would
    /// apply an 80–150ms first-open layout hitch as keyframe wall time and skip a
    /// 200ms Dialog / AlertDialog fade.
    pub(crate) fn clear_timeline_stamp(&mut self) {
        self.last_tick_ms = None;
    }

    /// True while some entry still has time left to play.
    ///
    /// A finished finite animation is deliberately *not* active: `fill-mode: forwards`
    /// holds its end value without a frame pump, and treating it as active kept every
    /// element that ever animated redrawing forever.
    pub fn has_active(&self, style: &super::Style) -> bool {
        style
            .animations
            .iter()
            .enumerate()
            .any(|(i, e)| e.needs_frame() && !self.runs.get(i).is_some_and(|r| r.finished))
    }

    /// Properties the last tick painted; transitions must not fight over these.
    pub fn lock(&self) -> AnimationLock {
        self.lock
    }

    /// After an external clamp of animated `max-height` (viewport cap), keep `written`
    /// matching `Style` so the next tick does not treat the cap as a new cascade baseline.
    pub(crate) fn sync_written_max_height(&mut self, max_height: taffy::style::Dimension) {
        if let Some(w) = self.written.as_mut() {
            w.max_height = max_height;
        }
    }
}

/// Build one CSS `animation` list item that references a registered [`super::keyframes`] name.
pub fn keyframes_anim_entry(
    name: impl Into<String>,
    duration_ms: f32,
    timing: TimingFunction,
    fill_mode: AnimationFillMode,
) -> AnimationEntry {
    AnimationEntry {
        name: name.into(),
        duration_ms,
        timing,
        delay_ms: 0.0,
        iteration_count: AnimationIterationCount::Finite(1.0),
        direction: AnimationDirection::Normal,
        fill_mode,
        play_state: AnimationPlayState::Running,
        ..Default::default()
    }
}

fn observe_playback(id: Option<u64>, run: &AnimationRunState) {
    if let Some(id) = id {
        super::waapi::observe_time(id, run.elapsed_ms, run.finished);
    }
}

fn animation_entry_fp(entry: &AnimationEntry) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    // Intentionally omit `play_state`: pause/resume must keep elapsed time.
    entry.name.hash(&mut h);
    entry.duration_ms.to_bits().hash(&mut h);
    entry.delay_ms.to_bits().hash(&mut h);
    format!("{:?}", entry.iteration_count).hash(&mut h);
    entry.direction.as_str().hash(&mut h);
    entry.fill_mode.as_str().hash(&mut h);
    entry.end_delay_ms.to_bits().hash(&mut h);
    h.finish()
}

/// Progress for CSS scroll/view `animation-timeline` (0–1). Document time uses [`Self::ZERO`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimelineProgress {
    /// Nearest scroller (`scroll()`): `scroll_offset / max_scroll`.
    pub scroll: f32,
    /// Element in the viewport (`view()`): 0 just entering, 1 just leaving.
    pub view: f32,
}

impl TimelineProgress {
    pub const ZERO: Self = Self {
        scroll: 0.0,
        view: 0.0,
    };
}

/// Write a sample back, but only for the properties the rule actually declares.
///
/// The sampler fills undeclared properties from the base so single-ended keyframes can
/// interpolate out of the cascade. Writing those back would let one entry of an
/// `animation` list clobber another: shadcn's `fade-in-0, zoom-in-95` lost its fade
/// because `zoom-in-95` reported the *base* opacity and ran last.
fn apply_keyframe_sample(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    stops: &[super::keyframes::KeyframeStop],
    sample: &super::keyframes::KeyframeSample,
    composition: AnimationComposition,
) {
    let cov = super::keyframes::keyframe_coverage(stops);
    for id in cov.ids.iter() {
        if let Some(effect) = sample.props.get(&id) {
            let written = if matches!(composition, AnimationComposition::Replace) {
                effect.clone()
            } else {
                let underlying = super::animatable::read_value(style, text, id);
                super::animatable::compose(composition, &underlying, effect)
            };
            super::animatable::write_value(style, text, id, &written);
        }
    }
}

fn restore_baseline(style: &mut super::Style, base: &AnimationBaseline) {
    style.opacity = base.opacity;
    style.rotate_deg = base.rotate_deg;
    style.scale = base.scale;
    style.translate = base.translate;
    style.max_size.height = base.max_height;
    style.min_size.height = base.min_height;
}

/// Taffy `min-height: auto` resolves to min-content; padding on the same box then blocks
/// `max-height` keyframes from collapsing to 0. Pin only while a rule owns `max-height`.
fn pin_min_height_for_max_height_keyframes(style: &mut super::Style, lock: AnimationLock) {
    if lock.max_height {
        style.min_size.height = taffy::style::Dimension::length(0.0);
    }
}

fn baseline_matches(style: &super::Style, base: &AnimationBaseline) -> bool {
    (style.opacity - base.opacity).abs() <= 1e-5
        && (style.rotate_deg - base.rotate_deg).abs() <= 1e-4
        && (style.scale[0] - base.scale[0]).abs() <= 1e-5
        && (style.scale[1] - base.scale[1]).abs() <= 1e-5
        && (style.translate[0] - base.translate[0]).abs() <= 1e-4
        && (style.translate[1] - base.translate[1]).abs() <= 1e-4
        && max_height_dim_matches(style.max_size.height, base.max_height)
}

fn max_height_dim_matches(style: taffy::style::Dimension, base: taffy::style::Dimension) -> bool {
    use crate::layout::{dim_kind, DimKind};
    match (dim_kind(style), dim_kind(base)) {
        (DimKind::Auto, DimKind::Auto) => true,
        (DimKind::Length(a), DimKind::Length(b)) => (a - b).abs() <= 1e-3,
        (DimKind::Percent(a), DimKind::Percent(b)) => (a - b).abs() <= 1e-5,
        _ => false,
    }
}

/// Paint one keyframe sample and record which properties it claimed.
fn apply_animation_sample(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    stops: &[super::keyframes::KeyframeStop],
    local_t: f32,
    timing: TimingFunction,
    base: &super::keyframes::KeyframeBase,
    lock: &mut AnimationLock,
    composition: AnimationComposition,
) {
    let sample = super::keyframes::sample_keyframes_eased(stops, local_t, timing, Some(base));
    apply_keyframe_sample(style, text, stops, &sample, composition);
    let cov = super::keyframes::keyframe_coverage(stops);
    lock.opacity |= cov.opacity;
    lock.transform |= cov.transform;
    lock.max_height |= cov.max_height;
    lock.ids.union_with(cov.ids);
}

#[cfg(debug_assertions)]
fn warn_unknown_keyframes(name: &str) {
    use std::cell::RefCell;
    use std::collections::HashSet;
    thread_local! {
        static SEEN: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }
    SEEN.with(|s| {
        if s.borrow_mut().insert(name.to_string()) {
            log::warn!(
                "[rui] animation `{name}` has no @keyframes; the element will stay at its \
                 cascaded value. Register it with `register_keyframes(\"{name}\", ..)`."
            );
        }
    });
}

#[cfg(not(debug_assertions))]
fn warn_unknown_keyframes(_name: &str) {}

/// Map local cycle progress through `animation-direction`.
pub(crate) fn direction_local_t(
    direction: super::element_style::AnimationDirection,
    iteration_index: u32,
    cycle_t: f32,
) -> f32 {
    super::timing::direction_local_t(direction, iteration_index, cycle_t)
}

fn entry_timing_model(entry: &super::element_style::AnimationEntry) -> super::timing::Timing {
    super::timing::Timing {
        duration_ms: entry.duration_ms,
        delay_ms: entry.delay_ms,
        end_delay_ms: entry.end_delay_ms,
        easing: entry.timing,
        iterations: entry.iteration_count,
        direction: entry.direction,
        fill: entry.fill_mode,
    }
}

/// Frame delta for `@keyframes` / `animate-spin`.
///
/// A pointer storm blocks this thread, so the next present can be hundreds of
/// milliseconds late while `dt_secs` stays tiny. Clamping that gap to
/// [`CSS_TRANSITION_IDLE_JUMP_MS`] and stamping `last_tick_ms` to *now* drops the
/// rest — spin crawls for the whole slide. Transitions keep that cap in
/// [`duration_tween_elapsed_ms`]. Keyframes apply the full gap. `frame` is only
/// the fallback when the timeline has not moved (unit tests).
fn animation_tick_dt_ms(snap: &mut AnimationSnapshot, dt_secs: f32, reduced: bool) -> f32 {
    if reduced {
        return 1.0e9;
    }
    let now_ms = css_timeline_now_ms();
    let frame = if dt_secs > 1.0e-4 {
        (dt_secs * 1000.0).min(80.0)
    } else {
        0.0
    };
    let wall = snap
        .last_tick_ms
        .map(|last| (now_ms - last).max(0.0) as f32)
        .unwrap_or(0.0);
    let dt_ms = wall.max(frame);
    snap.last_tick_ms = Some(now_ms);
    dt_ms
}

/// Advance CSS `@keyframes` animations on one element. Returns true if still active.
pub fn tick_element_animations(
    style: &mut super::Style,
    snap: &mut AnimationSnapshot,
    dt_secs: f32,
) -> bool {
    let mut text = super::text_style::TextStyle::default();
    let used = super::used_layout::UsedLayout::default();
    tick_element_animations_ex(style, &mut text, &used, snap, dt_secs)
}

/// Full tick: specified slot on `snap.specified`, used layout for `auto` sizes, text props.
pub fn tick_element_animations_ex(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    used: &super::used_layout::UsedLayout,
    snap: &mut AnimationSnapshot,
    dt_secs: f32,
) -> bool {
    tick_element_animations_ex_named(
        style,
        text,
        used,
        snap,
        dt_secs,
        TimelineProgress::ZERO,
        &[],
    )
}

/// Full tick with scroll/view timeline progress (ANIM A6.1).
pub fn tick_element_animations_ex_progress(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    used: &super::used_layout::UsedLayout,
    snap: &mut AnimationSnapshot,
    dt_secs: f32,
    progress: TimelineProgress,
) -> bool {
    tick_element_animations_ex_named(style, text, used, snap, dt_secs, progress, &[])
}

/// Full tick with named `scroll-timeline` / `view-timeline` progress (later = nearer).
pub fn tick_element_animations_ex_named(
    style: &mut super::Style,
    text: &mut super::text_style::TextStyle,
    used: &super::used_layout::UsedLayout,
    snap: &mut AnimationSnapshot,
    dt_secs: f32,
    progress: TimelineProgress,
    named: &[(String, f32)],
) -> bool {
    use super::element_style::AnimationPlayState;
    use super::keyframes::{keyframe_coverage, lookup_keyframes, KeyframeBase};

    if style.animations.is_empty() {
        if let (Some(base), Some(written)) = (snap.base, snap.written) {
            if baseline_matches(style, &written) {
                restore_baseline(style, &base);
            }
        }
        *snap = AnimationSnapshot::default();
        return false;
    }

    let mut cov_ids = super::animatable::PropertySet::default();
    for entry in &style.animations {
        if let Some(stops) = lookup_keyframes(&entry.name) {
            cov_ids.union_with(keyframe_coverage(&stops).ids);
        }
    }
    snap.specified
        .capture_author(style, text, &snap.written_props, cov_ids.iter());

    let base = match (snap.base, snap.written) {
        (Some(b), Some(w)) if baseline_matches(style, &w) => b,
        _ => AnimationBaseline::from_style(style),
    };
    snap.base = Some(base);
    restore_baseline(style, &base);

    let kf_base = KeyframeBase::from_specified(&snap.specified, style, text, used, || {
        KeyframeBase::layered(
            base.opacity,
            base.rotate_deg,
            base.scale,
            base.translate,
            crate::layout::dim_length_px(base.max_height).unwrap_or(used.anim_height()),
        )
    });

    while snap.runs.len() < style.animations.len() {
        let i = snap.runs.len();
        snap.runs.push(AnimationRunState {
            elapsed_ms: 0.0,
            baseline: base,
            finished: false,
            entry_fp: animation_entry_fp(&style.animations[i]),
        });
    }
    snap.runs.truncate(style.animations.len());

    let reduced = prefers_reduced_motion();
    let dt_ms = animation_tick_dt_ms(snap, dt_secs, reduced);
    let mut any_active = false;
    let mut lock = AnimationLock::default();
    let mut animations = style.animations.clone();
    let skip_max_h =
        cov_ids.contains(super::animatable::PropertyId::MaxHeight) && kf_base.max_height < 1.0;

    for (i, entry) in animations.iter_mut().enumerate() {
        if !entry.is_named() {
            continue;
        }
        let fp = animation_entry_fp(entry);
        {
            let run = &mut snap.runs[i];
            run.baseline = base;
            if run.entry_fp != fp {
                *run = AnimationRunState {
                    elapsed_ms: 0.0,
                    baseline: base,
                    finished: false,
                    entry_fp: fp,
                };
            }
        }
        super::waapi::prepare_entry(entry, &mut snap.runs[i]);
        let playback_id = entry.playback_id;
        if playback_id.is_some_and(super::waapi::is_cancelled) {
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        }

        let Some(stops) = lookup_keyframes(&entry.name) else {
            warn_unknown_keyframes(&entry.name);
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        };

        let (elapsed_ms, finished_before, paused) = {
            let run = &snap.runs[i];
            (
                run.elapsed_ms,
                run.finished,
                entry.play_state == AnimationPlayState::Paused && !reduced,
            )
        };

        if skip_max_h && keyframe_coverage(&stops).max_height {
            any_active = true;
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        }

        if entry.timeline.is_progress_driven() {
            let raw = match &entry.timeline {
                AnimationTimeline::Scroll => Some(progress.scroll),
                AnimationTimeline::View => Some(progress.view),
                AnimationTimeline::Named(name) => super::scroll_timeline::lookup_named(named, name),
                AnimationTimeline::Auto => None,
            };
            let Some(raw) = raw else {
                observe_playback(playback_id, &snap.runs[i]);
                continue;
            };
            let raw = super::scroll_timeline::map_animation_range(
                raw,
                entry.range_start.as_deref(),
                entry.range_end.as_deref(),
            );
            let t = if paused {
                (elapsed_ms / 1000.0).clamp(0.0, 1.0)
            } else {
                let t = raw.clamp(0.0, 1.0);
                snap.runs[i].elapsed_ms = t * 1000.0;
                t
            };
            let local = direction_local_t(entry.direction, 0, t);
            apply_animation_sample(
                style,
                text,
                &stops,
                local,
                entry.timing,
                &kf_base,
                &mut lock,
                entry.composition,
            );
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        }

        if paused {
            if let Some(local) = animation_sample_t(entry, elapsed_ms) {
                apply_animation_sample(
                    style,
                    text,
                    &stops,
                    local,
                    entry.timing,
                    &kf_base,
                    &mut lock,
                    entry.composition,
                );
            } else if entry.fill_mode.apply_backwards() && elapsed_ms < entry.delay_ms {
                let local = direction_local_t(entry.direction, 0, 0.0);
                apply_animation_sample(
                    style,
                    text,
                    &stops,
                    local,
                    entry.timing,
                    &kf_base,
                    &mut lock,
                    entry.composition,
                );
            }
            if entry.needs_frame() || !finished_before {
                any_active = true;
            }
            if entry_timing_model(entry).resolve(elapsed_ms).finished {
                snap.runs[i].finished = true;
            }
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        }

        let elapsed_ms = if finished_before {
            elapsed_ms
        } else {
            let rate = playback_id.map(super::waapi::playback_rate).unwrap_or(1.0);
            let next = elapsed_ms + dt_ms * rate;
            if next < 0.0 {
                snap.runs[i].elapsed_ms = 0.0;
                snap.runs[i].finished = true;
                if entry.fill_mode.apply_backwards() {
                    let local = direction_local_t(entry.direction, 0, 0.0);
                    apply_animation_sample(
                        style,
                        text,
                        &stops,
                        local,
                        entry.timing,
                        &kf_base,
                        &mut lock,
                        entry.composition,
                    );
                }
                observe_playback(playback_id, &snap.runs[i]);
                continue;
            }
            snap.runs[i].elapsed_ms = next;
            next
        };

        let progress = entry_timing_model(entry).resolve(elapsed_ms);
        match progress.phase {
            super::timing::TimingPhase::Delay => {
                if entry.fill_mode.apply_backwards() {
                    apply_animation_sample(
                        style,
                        text,
                        &stops,
                        progress.local_t,
                        entry.timing,
                        &kf_base,
                        &mut lock,
                        entry.composition,
                    );
                }
                any_active = true;
                observe_playback(playback_id, &snap.runs[i]);
                continue;
            }
            super::timing::TimingPhase::EndDelay => {
                if entry.fill_mode.apply_forwards() {
                    apply_animation_sample(
                        style,
                        text,
                        &stops,
                        progress.local_t,
                        entry.timing,
                        &kf_base,
                        &mut lock,
                        entry.composition,
                    );
                }
                any_active = true;
                observe_playback(playback_id, &snap.runs[i]);
                continue;
            }
            super::timing::TimingPhase::Finished => {
                snap.runs[i].finished = true;
                if entry.fill_mode.apply_forwards() {
                    apply_animation_sample(
                        style,
                        text,
                        &stops,
                        progress.local_t,
                        entry.timing,
                        &kf_base,
                        &mut lock,
                        entry.composition,
                    );
                }
                observe_playback(playback_id, &snap.runs[i]);
                continue;
            }
            super::timing::TimingPhase::Active => {
                snap.runs[i].finished = false;
                any_active = true;
                apply_animation_sample(
                    style,
                    text,
                    &stops,
                    progress.local_t,
                    entry.timing,
                    &kf_base,
                    &mut lock,
                    entry.composition,
                );
                observe_playback(playback_id, &snap.runs[i]);
            }
        }
    }

    snap.lock = lock;
    pin_min_height_for_max_height_keyframes(style, lock);
    snap.written = Some(AnimationBaseline::from_style(style));
    snap.written_props.clear();
    for id in lock.ids.iter() {
        snap.written_props
            .insert(id, super::animatable::read_value(style, text, id));
    }
    any_active
}

/// True while a finite `@keyframes` run is still playing.
///
/// Presence exit gates depend on active runs, not on a retained forwards fill.
pub fn animation_run_active(style: &super::Style, snap: &AnimationSnapshot) -> bool {
    if style.animations.is_empty() {
        return false;
    }
    snap.has_active(style)
}

fn animation_sample_t(
    entry: &super::element_style::AnimationEntry,
    elapsed_ms: f32,
) -> Option<f32> {
    let p = entry_timing_model(entry).resolve(elapsed_ms);
    match p.phase {
        super::timing::TimingPhase::Delay => None,
        _ => Some(p.local_t),
    }
}

/// True when this element should keep the animation frame pump alive.
pub fn css_animation_needs_frame(style: &super::Style, snap: &AnimationSnapshot) -> bool {
    style.animations.iter().enumerate().any(|(i, entry)| {
        if let Some(id) = entry.playback_id {
            return super::waapi::needs_frame(id, snap.runs.get(i));
        }
        entry.needs_frame()
            && !snap
                .runs
                .get(i)
                .is_some_and(|run| run.finished && run.entry_fp == animation_entry_fp(entry))
    })
}

/// Recover the *specified* (cascade) value for `prop` from the single `Style` field.
///
/// If `style` still holds exactly what we wrote last frame, nobody re-specified the
/// property, so the standing target stands. Otherwise the owner assigned a new value
/// and that value is the new target.
fn specified_prop_value(tween: &PropertyTween, current: [f32; 8], comps: u8) -> [f32; 8] {
    match tween.written {
        Some(w) if (0..comps as usize).all(|i| (w[i] - current[i]).abs() <= 1e-5) => tween.to,
        _ => current,
    }
}

fn specified_filter_value(tween: &FilterTween, current: &Option<FilterList>) -> Option<FilterList> {
    match tween.written.as_ref() {
        Some(w) if w == current => tween.to.clone(),
        _ => current.clone(),
    }
}

fn specified_clip_value(tween: &ClipPathTween, current: &ClipPath) -> ClipPath {
    match tween.written.as_ref() {
        Some(w) if w == current => tween.to.clone(),
        _ => current.clone(),
    }
}

/// CSS keywords like `auto` have no numeric value to interpolate.
///
/// Web needs `interpolate-size: allow-keywords` / `calc-size()` to animate to `auto`;
/// without this guard `max-height: auto` reads back as `0`, and the tween then pins the
/// box to zero height — turning a decorative `transition: all` into an invisible element.
fn prop_is_interpolable(style: &super::Style, prop: TransitionProperty) -> bool {
    use crate::layout::{dim_kind, lp_kind, lpa_kind, DimKind};
    let keywords = style.interpolate_size.allows_keywords();
    let calc = |slot: UnitSlot| {
        style
            .unit_specs
            .iter()
            .any(|(s, d)| *s == slot && (d.is_calc_size() || d.contains_size_keyword()))
    };
    let dim_len = |d, slot| matches!(dim_kind(d), DimKind::Length(_)) || keywords || calc(slot);
    let lp_len = |d| matches!(lp_kind(d), DimKind::Length(_));
    let lpa_len = |d| matches!(lpa_kind(d), DimKind::Length(_));
    match prop {
        TransitionProperty::Width => dim_len(style.size.width, UnitSlot::Width),
        TransitionProperty::Height => dim_len(style.size.height, UnitSlot::Height),
        TransitionProperty::MinWidth => dim_len(style.min_size.width, UnitSlot::MinWidth),
        TransitionProperty::MinHeight => dim_len(style.min_size.height, UnitSlot::MinHeight),
        TransitionProperty::MaxWidth => dim_len(style.max_size.width, UnitSlot::MaxWidth),
        TransitionProperty::MaxHeight => dim_len(style.max_size.height, UnitSlot::MaxHeight),
        TransitionProperty::FlexBasis => dim_len(style.flex_basis, UnitSlot::FlexBasis),
        TransitionProperty::Padding => {
            lp_len(style.padding.top)
                && lp_len(style.padding.right)
                && lp_len(style.padding.bottom)
                && lp_len(style.padding.left)
        }
        TransitionProperty::PaddingTop => lp_len(style.padding.top),
        TransitionProperty::PaddingRight => lp_len(style.padding.right),
        TransitionProperty::PaddingBottom => lp_len(style.padding.bottom),
        TransitionProperty::PaddingLeft => lp_len(style.padding.left),
        TransitionProperty::Margin => {
            lpa_len(style.margin.top)
                && lpa_len(style.margin.right)
                && lpa_len(style.margin.bottom)
                && lpa_len(style.margin.left)
        }
        TransitionProperty::MarginTop => lpa_len(style.margin.top),
        TransitionProperty::MarginRight => lpa_len(style.margin.right),
        TransitionProperty::MarginBottom => lpa_len(style.margin.bottom),
        TransitionProperty::MarginLeft => lpa_len(style.margin.left),
        TransitionProperty::Inset => {
            lpa_len(style.inset.top)
                && lpa_len(style.inset.right)
                && lpa_len(style.inset.bottom)
                && lpa_len(style.inset.left)
        }
        TransitionProperty::Top => lpa_len(style.inset.top),
        TransitionProperty::Right => lpa_len(style.inset.right),
        TransitionProperty::Bottom => lpa_len(style.inset.bottom),
        TransitionProperty::Left => lpa_len(style.inset.left),
        TransitionProperty::Gap => lp_len(style.gap.width) && lp_len(style.gap.height),
        TransitionProperty::ColumnGap => lp_len(style.gap.width),
        TransitionProperty::RowGap => lp_len(style.gap.height),
        TransitionProperty::GridTemplateRows => {
            crate::layout::tracks_fr_list(&style.grid_template_rows).is_some()
                || crate::layout::first_track_fr(&style.grid_template_rows).is_some()
        }
        TransitionProperty::GridTemplateColumns => {
            crate::layout::tracks_fr_list(&style.grid_template_columns).is_some()
                || crate::layout::first_track_fr(&style.grid_template_columns).is_some()
        }
        _ => true,
    }
}

fn pack1(v: f32) -> ([f32; 8], u8) {
    ([v, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1)
}

fn pack2(a: f32, b: f32) -> ([f32; 8], u8) {
    ([a, b, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 2)
}

fn pack4(a: f32, b: f32, c: f32, d: f32) -> ([f32; 8], u8) {
    ([a, b, c, d, 0.0, 0.0, 0.0, 0.0], 4)
}

fn dim_px_transition(style: &super::Style, d: taffy::style::Dimension, slot: UnitSlot) -> f32 {
    use crate::layout::dimension::{PercentAxis, UnitCtx};
    use crate::layout::{dim_kind, DimKind};
    let height = matches!(
        slot,
        UnitSlot::Height | UnitSlot::MinHeight | UnitSlot::MaxHeight
    );
    if let Some((_, dim)) = style.unit_specs.iter().find(|(s, _)| *s == slot) {
        if dim.is_calc_size() || dim.contains_size_keyword() {
            let used_px = transition_tick_used()
                .map(|u| {
                    if height {
                        u.anim_height()
                    } else {
                        u.anim_width()
                    }
                })
                .unwrap_or(0.0);
            let ctx = UnitCtx::new(0.0, 0.0, 16.0);
            let axis = if height {
                PercentAxis::Block
            } else {
                PercentAxis::Inline
            };
            if let Some(px) = dim.eval_calc_size(used_px, &ctx, axis) {
                return px;
            }
        }
    }
    match dim_kind(d) {
        DimKind::Length(px) => px,
        _ if style.interpolate_size.allows_keywords() => transition_tick_used()
            .map(|u| {
                if height {
                    u.anim_height()
                } else {
                    u.anim_width()
                }
            })
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn lp_px(lp: taffy::style::LengthPercentage) -> f32 {
    crate::layout::lp_length_px(lp).unwrap_or(0.0)
}

fn lpa_px(lpa: taffy::style::LengthPercentageAuto) -> f32 {
    crate::layout::lpa_length_px(lpa).unwrap_or(0.0)
}

fn write_dim_slot(style: &mut super::Style, slot: UnitSlot, px: f32, nonneg: bool) {
    let v = if nonneg { px.max(0.0) } else { px };
    let slot = super::element_style::physical_unit_slot(slot, style.writing_mode, style.direction);
    style.unit_specs.retain(|(s, _)| *s != slot);
    match slot {
        UnitSlot::Width => style.size.width = taffy::style::Dimension::length(v),
        UnitSlot::Height => style.size.height = taffy::style::Dimension::length(v),
        UnitSlot::MinWidth => style.min_size.width = taffy::style::Dimension::length(v),
        UnitSlot::MinHeight => style.min_size.height = taffy::style::Dimension::length(v),
        UnitSlot::MaxWidth => style.max_size.width = taffy::style::Dimension::length(v),
        UnitSlot::MaxHeight => style.max_size.height = taffy::style::Dimension::length(v),
        UnitSlot::FlexBasis => style.flex_basis = taffy::style::Dimension::length(v),
        UnitSlot::PadT => style.padding.top = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadR => style.padding.right = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadB => style.padding.bottom = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::PadL => style.padding.left = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::MarginT => style.margin.top = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginR => style.margin.right = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginB => style.margin.bottom = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::MarginL => style.margin.left = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::GapX => style.gap.width = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::GapY => style.gap.height = taffy::style::LengthPercentage::length(v.max(0.0)),
        UnitSlot::InsetT => style.inset.top = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetR => style.inset.right = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetB => style.inset.bottom = taffy::style::LengthPercentageAuto::length(v),
        UnitSlot::InsetL => style.inset.left = taffy::style::LengthPercentageAuto::length(v),
        _ => {}
    }
}

fn extract_prop(style: &super::Style, prop: TransitionProperty) -> ([f32; 8], u8) {
    match prop {
        TransitionProperty::Opacity => pack1(style.opacity),
        TransitionProperty::BackgroundColor => {
            let c = style.bg_color;
            pack4(c[0], c[1], c[2], c[3])
        }
        TransitionProperty::BorderColor => {
            let c = style.border_color;
            pack4(c[0], c[1], c[2], c[3])
        }
        TransitionProperty::OutlineColor => {
            let c = style.outline_color;
            pack4(c[0], c[1], c[2], c[3])
        }
        TransitionProperty::Color => match transition_tick_text().and_then(|t| t.color) {
            Some(c) => pack4(
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                c[3] as f32 / 255.0,
            ),
            None => ([0.0; 8], 0),
        },
        TransitionProperty::FontSize => match transition_tick_text().and_then(|t| t.size) {
            Some(s) => pack1(s),
            None => ([0.0; 8], 0),
        },
        TransitionProperty::LineHeight => {
            match transition_tick_text().and_then(|t| t.line_height_px) {
                Some(h) => pack1(h),
                None => ([0.0; 8], 0),
            }
        }
        TransitionProperty::LetterSpacing => match transition_tick_text() {
            Some(t) => pack1(t.letter_spacing),
            None => ([0.0; 8], 0),
        },
        TransitionProperty::TextDecorationColor => {
            match transition_tick_text().and_then(|t| t.decoration_color) {
                Some(c) => pack4(
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    c[3] as f32 / 255.0,
                ),
                None => ([0.0; 8], 0),
            }
        }
        TransitionProperty::OutlineWidth => pack1(style.outline_width),
        TransitionProperty::OutlineOffset => pack1(style.outline_offset),
        TransitionProperty::BoxShadow => (
            [
                style.shadow_color[0],
                style.shadow_color[1],
                style.shadow_color[2],
                style.shadow_color[3],
                style.shadow_offset[0],
                style.shadow_offset[1],
                style.shadow_blur,
                style.shadow_spread,
            ],
            8,
        ),
        TransitionProperty::Width => {
            pack1(dim_px_transition(style, style.size.width, UnitSlot::Width))
        }
        TransitionProperty::Height => pack1(dim_px_transition(
            style,
            style.size.height,
            UnitSlot::Height,
        )),
        TransitionProperty::MinWidth => pack1(dim_px_transition(
            style,
            style.min_size.width,
            UnitSlot::MinWidth,
        )),
        TransitionProperty::MinHeight => pack1(dim_px_transition(
            style,
            style.min_size.height,
            UnitSlot::MinHeight,
        )),
        TransitionProperty::MaxWidth => pack1(dim_px_transition(
            style,
            style.max_size.width,
            UnitSlot::MaxWidth,
        )),
        TransitionProperty::MaxHeight => pack1(dim_px_transition(
            style,
            style.max_size.height,
            UnitSlot::MaxHeight,
        )),
        TransitionProperty::Padding => pack4(
            lp_px(style.padding.top),
            lp_px(style.padding.right),
            lp_px(style.padding.bottom),
            lp_px(style.padding.left),
        ),
        TransitionProperty::PaddingTop => pack1(lp_px(style.padding.top)),
        TransitionProperty::PaddingRight => pack1(lp_px(style.padding.right)),
        TransitionProperty::PaddingBottom => pack1(lp_px(style.padding.bottom)),
        TransitionProperty::PaddingLeft => pack1(lp_px(style.padding.left)),
        TransitionProperty::Margin => pack4(
            lpa_px(style.margin.top),
            lpa_px(style.margin.right),
            lpa_px(style.margin.bottom),
            lpa_px(style.margin.left),
        ),
        TransitionProperty::MarginTop => pack1(lpa_px(style.margin.top)),
        TransitionProperty::MarginRight => pack1(lpa_px(style.margin.right)),
        TransitionProperty::MarginBottom => pack1(lpa_px(style.margin.bottom)),
        TransitionProperty::MarginLeft => pack1(lpa_px(style.margin.left)),
        TransitionProperty::Inset => pack4(
            lpa_px(style.inset.top),
            lpa_px(style.inset.right),
            lpa_px(style.inset.bottom),
            lpa_px(style.inset.left),
        ),
        TransitionProperty::Top => pack1(lpa_px(style.inset.top)),
        TransitionProperty::Right => pack1(lpa_px(style.inset.right)),
        TransitionProperty::Bottom => pack1(lpa_px(style.inset.bottom)),
        TransitionProperty::Left => pack1(lpa_px(style.inset.left)),
        TransitionProperty::Gap => pack2(lp_px(style.gap.width), lp_px(style.gap.height)),
        TransitionProperty::ColumnGap => pack1(lp_px(style.gap.width)),
        TransitionProperty::RowGap => pack1(lp_px(style.gap.height)),
        TransitionProperty::BorderWidth => {
            let w = style.border_width_sides.unwrap_or([style.border_width; 4]);
            pack4(w[0], w[1], w[2], w[3])
        }
        TransitionProperty::BorderTopWidth => {
            pack1(style.border_width_sides.unwrap_or([style.border_width; 4])[0])
        }
        TransitionProperty::BorderRightWidth => {
            pack1(style.border_width_sides.unwrap_or([style.border_width; 4])[1])
        }
        TransitionProperty::BorderBottomWidth => {
            pack1(style.border_width_sides.unwrap_or([style.border_width; 4])[2])
        }
        TransitionProperty::BorderLeftWidth => {
            pack1(style.border_width_sides.unwrap_or([style.border_width; 4])[3])
        }
        TransitionProperty::BorderRadius => pack4(
            style.radii[0],
            style.radii[1],
            style.radii[2],
            style.radii[3],
        ),
        TransitionProperty::BorderTopLeftRadius => pack1(style.radii[0]),
        TransitionProperty::BorderTopRightRadius => pack1(style.radii[1]),
        TransitionProperty::BorderBottomRightRadius => pack1(style.radii[2]),
        TransitionProperty::BorderBottomLeftRadius => pack1(style.radii[3]),
        TransitionProperty::FlexGrow => pack1(style.flex_grow),
        TransitionProperty::FlexShrink => pack1(style.flex_shrink),
        TransitionProperty::FlexBasis => pack1(dim_px_transition(
            style,
            style.flex_basis,
            UnitSlot::FlexBasis,
        )),
        TransitionProperty::GridTemplateRows => {
            crate::layout::pack_tracks_fr(&style.grid_template_rows)
        }
        TransitionProperty::GridTemplateColumns => {
            crate::layout::pack_tracks_fr(&style.grid_template_columns)
        }
        TransitionProperty::BackgroundPosition => pack2(style.bg_position[0], style.bg_position[1]),
        // Handled via [`ClipPathTween`] + [`ClipPath::interpolate`].
        TransitionProperty::ClipPath => ([0.0; 8], 0),
        TransitionProperty::Zoom => pack1(style.zoom),
        TransitionProperty::BackdropFilter => pack1(style.backdrop_blur),
        TransitionProperty::Scale => pack2(style.scale[0], style.scale[1]),
        TransitionProperty::Rotate => pack1(style.rotate_deg),
        TransitionProperty::TransformOrigin => {
            let f = super::transform::origin_frac(&style.transform_origin);
            pack2(f[0], f[1])
        }
        TransitionProperty::OffsetDistance => pack1(style.offset_distance),
        TransitionProperty::OffsetRotate => pack1(style.offset_rotate_deg.unwrap_or(0.0)),
        TransitionProperty::Translate => (
            [
                style.translate[0],
                style.translate[1],
                style.translate_pct[0],
                style.translate_pct[1],
                0.0,
                0.0,
                0.0,
                0.0,
            ],
            4,
        ),
        // Independent transform props: translate → rotate → scale (matches resolve order).
        TransitionProperty::Transform => (
            [
                style.translate[0],
                style.translate[1],
                style.scale[0],
                style.scale[1],
                style.rotate_deg,
                style.translate_pct[0],
                style.translate_pct[1],
                0.0,
            ],
            7,
        ),
        // Handled via [`FilterTween`] + [`interpolate_filter_lists`].
        TransitionProperty::Filter => ([0.0; 8], 0),
        TransitionProperty::All | TransitionProperty::Unsupported => ([0.0; 8], 0),
    }
}

fn write_prop(style: &mut super::Style, prop: TransitionProperty, val: &[f32; 8], comps: u8) {
    match prop {
        TransitionProperty::Opacity => style.opacity = val[0],
        TransitionProperty::BackgroundColor => style.bg_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::BorderColor => style.border_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::OutlineColor => style.outline_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::Color => {
            if let Some(t) = transition_tick_text() {
                t.color = Some([
                    (val[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                ]);
            }
        }
        TransitionProperty::FontSize => {
            if let Some(t) = transition_tick_text() {
                t.size = Some(val[0].max(0.0));
            }
        }
        TransitionProperty::LineHeight => {
            if let Some(t) = transition_tick_text() {
                t.line_height_px = Some(val[0].max(0.0));
            }
        }
        TransitionProperty::LetterSpacing => {
            if let Some(t) = transition_tick_text() {
                t.letter_spacing = val[0];
                t.letter_spacing_specified = true;
            }
        }
        TransitionProperty::TextDecorationColor => {
            if let Some(t) = transition_tick_text() {
                t.decoration_color = Some([
                    (val[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (val[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                ]);
                t.decoration_color_specified = true;
            }
        }
        TransitionProperty::OutlineWidth => style.outline_width = val[0],
        TransitionProperty::OutlineOffset => style.outline_offset = val[0],
        TransitionProperty::BoxShadow => {
            style.shadow_color = [val[0], val[1], val[2], val[3]];
            style.shadow_offset = [val[4], val[5]];
            style.shadow_blur = val[6].max(0.0);
            style.shadow_spread = val[7];
        }
        TransitionProperty::Width => write_dim_slot(style, UnitSlot::Width, val[0], true),
        TransitionProperty::Height => write_dim_slot(style, UnitSlot::Height, val[0], true),
        TransitionProperty::MinWidth => write_dim_slot(style, UnitSlot::MinWidth, val[0], true),
        TransitionProperty::MinHeight => write_dim_slot(style, UnitSlot::MinHeight, val[0], true),
        TransitionProperty::MaxWidth => write_dim_slot(style, UnitSlot::MaxWidth, val[0], true),
        TransitionProperty::MaxHeight => write_dim_slot(style, UnitSlot::MaxHeight, val[0], true),
        TransitionProperty::Padding => {
            write_dim_slot(style, UnitSlot::PadT, val[0], true);
            write_dim_slot(style, UnitSlot::PadR, val[1], true);
            write_dim_slot(style, UnitSlot::PadB, val[2], true);
            write_dim_slot(style, UnitSlot::PadL, val[3], true);
        }
        TransitionProperty::PaddingTop => write_dim_slot(style, UnitSlot::PadT, val[0], true),
        TransitionProperty::PaddingRight => write_dim_slot(style, UnitSlot::PadR, val[0], true),
        TransitionProperty::PaddingBottom => write_dim_slot(style, UnitSlot::PadB, val[0], true),
        TransitionProperty::PaddingLeft => write_dim_slot(style, UnitSlot::PadL, val[0], true),
        TransitionProperty::Margin => {
            write_dim_slot(style, UnitSlot::MarginT, val[0], false);
            write_dim_slot(style, UnitSlot::MarginR, val[1], false);
            write_dim_slot(style, UnitSlot::MarginB, val[2], false);
            write_dim_slot(style, UnitSlot::MarginL, val[3], false);
        }
        TransitionProperty::MarginTop => write_dim_slot(style, UnitSlot::MarginT, val[0], false),
        TransitionProperty::MarginRight => write_dim_slot(style, UnitSlot::MarginR, val[0], false),
        TransitionProperty::MarginBottom => write_dim_slot(style, UnitSlot::MarginB, val[0], false),
        TransitionProperty::MarginLeft => write_dim_slot(style, UnitSlot::MarginL, val[0], false),
        TransitionProperty::Inset => {
            write_dim_slot(style, UnitSlot::InsetT, val[0], false);
            write_dim_slot(style, UnitSlot::InsetR, val[1], false);
            write_dim_slot(style, UnitSlot::InsetB, val[2], false);
            write_dim_slot(style, UnitSlot::InsetL, val[3], false);
        }
        TransitionProperty::Top => write_dim_slot(style, UnitSlot::InsetT, val[0], false),
        TransitionProperty::Right => write_dim_slot(style, UnitSlot::InsetR, val[0], false),
        TransitionProperty::Bottom => write_dim_slot(style, UnitSlot::InsetB, val[0], false),
        TransitionProperty::Left => write_dim_slot(style, UnitSlot::InsetL, val[0], false),
        TransitionProperty::Gap => {
            write_dim_slot(style, UnitSlot::GapX, val[0], true);
            write_dim_slot(style, UnitSlot::GapY, val[1], true);
        }
        TransitionProperty::ColumnGap => write_dim_slot(style, UnitSlot::GapX, val[0], true),
        TransitionProperty::RowGap => write_dim_slot(style, UnitSlot::GapY, val[0], true),
        TransitionProperty::BorderWidth => {
            style.set_border_widths_trbl([val[0], val[1], val[2], val[3]])
        }
        TransitionProperty::BorderTopWidth => style.set_border_side_width(0, val[0]),
        TransitionProperty::BorderRightWidth => style.set_border_side_width(1, val[0]),
        TransitionProperty::BorderBottomWidth => style.set_border_side_width(2, val[0]),
        TransitionProperty::BorderLeftWidth => style.set_border_side_width(3, val[0]),
        TransitionProperty::BorderRadius => {
            style.radii = [
                val[0].max(0.0),
                val[1].max(0.0),
                val[2].max(0.0),
                val[3].max(0.0),
            ];
        }
        TransitionProperty::BorderTopLeftRadius => style.radii[0] = val[0].max(0.0),
        TransitionProperty::BorderTopRightRadius => style.radii[1] = val[0].max(0.0),
        TransitionProperty::BorderBottomRightRadius => style.radii[2] = val[0].max(0.0),
        TransitionProperty::BorderBottomLeftRadius => style.radii[3] = val[0].max(0.0),
        TransitionProperty::FlexGrow => style.flex_grow = val[0].max(0.0),
        TransitionProperty::FlexShrink => style.flex_shrink = val[0].max(0.0),
        TransitionProperty::FlexBasis => write_dim_slot(style, UnitSlot::FlexBasis, val[0], true),
        TransitionProperty::GridTemplateRows => {
            write_grid_tracks_packed(&mut style.grid_template_rows, val, comps)
        }
        TransitionProperty::GridTemplateColumns => {
            write_grid_tracks_packed(&mut style.grid_template_columns, val, comps)
        }
        TransitionProperty::BackgroundPosition => {
            style.bg_position = [val[0], val[1]];
        }
        TransitionProperty::ClipPath => {}
        TransitionProperty::Zoom => {
            style.zoom = if val[0].is_finite() && val[0] > 0.0 {
                val[0]
            } else {
                1.0
            };
        }
        TransitionProperty::BackdropFilter => style.backdrop_blur = val[0].max(0.0),
        TransitionProperty::Transform => {
            style.translate = [val[0], val[1]];
            style.scale = [val[2], val[3]];
            style.rotate_deg = val[4];
            if comps >= 7 {
                style.translate_pct = [val[5], val[6]];
            }
        }
        TransitionProperty::Translate => {
            style.translate = [val[0], val[1]];
            if comps >= 4 {
                style.translate_pct = [val[2], val[3]];
            }
        }
        TransitionProperty::Scale => {
            style.scale = [val[0], val[1]];
        }
        TransitionProperty::Rotate => {
            style.rotate_deg = val[0];
        }
        TransitionProperty::TransformOrigin => {
            super::transform::set_origin_frac(&mut style.transform_origin, [val[0], val[1]]);
        }
        TransitionProperty::OffsetDistance => {
            style.offset_distance = val[0].clamp(0.0, 1.0);
        }
        TransitionProperty::OffsetRotate => {
            style.offset_rotate_deg = Some(val[0]);
        }
        TransitionProperty::Filter => {}
        TransitionProperty::All | TransitionProperty::Unsupported => {}
    }
}

fn write_grid_tracks_packed(tracks: &mut Vec<crate::layout::GridTrack>, val: &[f32; 8], comps: u8) {
    let n = (comps as usize).min(8);
    if n <= 1 {
        crate::layout::set_first_track_fr(tracks, val[0].max(0.0));
    } else {
        crate::layout::set_tracks_fr_list(tracks, &val[..n]);
    }
}

/// Write a specified (cascade) value without clobbering an in-flight used value.
///
/// When `style` still holds exactly what the engine last wrote and `specified`
/// equals the standing tween target, this is a no-op so every-frame owners
/// (sidebar rail sync) do not wipe the mid-tween width before tick.
pub fn assign_transition_specified(
    style: &mut super::Style,
    snapshot: &TransitionSnapshot,
    prop: TransitionProperty,
    specified: [f32; 8],
) {
    let (current, comps) = extract_prop(style, prop);
    if comps == 0 {
        write_prop(style, prop, &specified, comps.max(1));
        return;
    }
    if let Some(tween) = snapshot.tweens.get(&prop) {
        if let Some(w) = tween.written {
            let holding_used = (0..comps as usize).all(|i| (w[i] - current[i]).abs() <= 1e-5);
            let same_target =
                (0..comps as usize).all(|i| (tween.to[i] - specified[i]).abs() <= 1e-5);
            if holding_used && same_target {
                return;
            }
        }
    }
    write_prop(style, prop, &specified, comps);
}

fn for_each_declared_prop(entries: &[TransitionEntry], mut f: impl FnMut(TransitionProperty)) {
    for entry in entries {
        match entry.property {
            TransitionProperty::All => {
                for &p in ALL_PROPS {
                    f(p);
                }
            }
            TransitionProperty::Unsupported => {}
            p => f(p),
        }
    }
}

/// Copy computed values of every transitioned property from `src` onto `dst`.
///
/// Snapshot **used** values (mid-tween) win over `src`'s current fields so a
/// rebuild fragment cannot clobber an in-flight width with paint's end state.
/// Non-interpolable keywords (`auto`, `%`) are left on `dst` (the rebuild paint).
pub fn copy_transition_used_values(
    dst: &mut super::Style,
    src: &super::Style,
    snapshot: &TransitionSnapshot,
) {
    let mut props = Vec::new();
    for_each_declared_prop(&src.transition, |prop| props.push(prop));
    for_each_declared_prop(&dst.transition, |prop| props.push(prop));
    for prop in props {
        copy_declared_prop(dst, src, prop);
    }
    snapshot.write_used_values_into(dst);
}

fn copy_declared_prop(dst: &mut super::Style, src: &super::Style, prop: TransitionProperty) {
    if prop == TransitionProperty::Filter {
        dst.filter = src.filter.clone();
        return;
    }
    if prop == TransitionProperty::ClipPath {
        dst.clip_path = src.clip_path.clone();
        return;
    }
    if prop == TransitionProperty::BoxShadow {
        write_shadow_layers(dst, &shadow_layers(src));
        return;
    }
    if !prop_is_interpolable(src, prop) {
        return;
    }
    let (val, comps) = extract_prop(src, prop);
    if comps > 0 {
        write_prop(dst, prop, &val, comps);
    }
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_arr(a: &[f32; 8], b: &[f32; 8], t: f32, n: u8) -> [f32; 8] {
    let mut out = [0.0; 8];
    for i in 0..n as usize {
        out[i] = lerp_f32(a[i], b[i], t);
    }
    out
}

fn is_color_transition_prop(prop: TransitionProperty) -> bool {
    matches!(
        prop,
        TransitionProperty::BackgroundColor
            | TransitionProperty::BorderColor
            | TransitionProperty::OutlineColor
            | TransitionProperty::Color
            | TransitionProperty::TextDecorationColor
    )
}

/// Same interpolator as keyframes (`AnimatableValue::Color` → `lerp_text_rgba`).
fn lerp_packed_color(from: &[f32; 8], to: &[f32; 8], t: f32) -> [f32; 8] {
    use super::animatable::{interpolate, AnimatableValue};
    let a = AnimatableValue::Color([from[0], from[1], from[2], from[3]]);
    let b = AnimatableValue::Color([to[0], to[1], to[2], to[3]]);
    match interpolate(&a, &b, t) {
        Some(AnimatableValue::Color(c)) => [c[0], c[1], c[2], c[3], 0.0, 0.0, 0.0, 0.0],
        _ => lerp_arr(from, to, t, 4),
    }
}

/// All concrete properties that `All` expands into.
///
/// Shorthands only (not longhands) so `transition: all` does not double-write
/// `padding` and `padding-left` in an undefined HashMap order.
const ALL_PROPS: &[TransitionProperty] = &[
    TransitionProperty::Opacity,
    TransitionProperty::BackgroundColor,
    TransitionProperty::BorderColor,
    TransitionProperty::OutlineColor,
    TransitionProperty::OutlineWidth,
    TransitionProperty::OutlineOffset,
    TransitionProperty::BoxShadow,
    TransitionProperty::Color,
    TransitionProperty::FontSize,
    TransitionProperty::LineHeight,
    TransitionProperty::LetterSpacing,
    TransitionProperty::TextDecorationColor,
    // Transform 已含 translate；勿再单独扩 Translate，避免同帧双写。
    TransitionProperty::Transform,
    TransitionProperty::Width,
    TransitionProperty::Height,
    TransitionProperty::MinWidth,
    TransitionProperty::MinHeight,
    TransitionProperty::MaxWidth,
    TransitionProperty::MaxHeight,
    TransitionProperty::Padding,
    TransitionProperty::Margin,
    TransitionProperty::Inset,
    TransitionProperty::Gap,
    TransitionProperty::BorderWidth,
    TransitionProperty::BorderRadius,
    TransitionProperty::FlexGrow,
    TransitionProperty::FlexShrink,
    TransitionProperty::FlexBasis,
    TransitionProperty::GridTemplateRows,
    TransitionProperty::GridTemplateColumns,
    TransitionProperty::BackgroundPosition,
    TransitionProperty::ClipPath,
    TransitionProperty::TransformOrigin,
    TransitionProperty::OffsetDistance,
    TransitionProperty::Zoom,
    TransitionProperty::BackdropFilter,
    TransitionProperty::Filter,
];

/// Nodes without class variants must still advance CSS transitions (e.g. Carousel track).
/// 有活跃 tween，或目标相对已记录的 `to` 已变，或尚未建档需要 seed。
pub fn css_transition_needs_frame(style: &super::Style, snapshot: &TransitionSnapshot) -> bool {
    if style.transition.is_empty() {
        return false;
    }
    if snapshot.has_active_tweens() {
        return true;
    }
    let mut config_map: HashMap<TransitionProperty, usize> = HashMap::new();
    for (idx, entry) in style.transition.iter().enumerate() {
        if entry.property == TransitionProperty::All {
            for &p in ALL_PROPS {
                config_map.entry(p).or_insert(idx);
            }
        } else {
            config_map.insert(entry.property, idx);
        }
    }
    for &prop in config_map.keys() {
        if prop == TransitionProperty::Filter {
            let current = style.filter.clone();
            match &snapshot.filter_tween {
                Some(t) => {
                    if t.to != specified_filter_value(t, &current) {
                        return true;
                    }
                }
                None => return true,
            }
            continue;
        }
        if prop == TransitionProperty::ClipPath {
            let current = style.clip_path.clone();
            match &snapshot.clip_tween {
                Some(t) => {
                    if t.to != specified_clip_value(t, &current) {
                        return true;
                    }
                }
                None => return true,
            }
            continue;
        }
        if prop == TransitionProperty::BoxShadow {
            let current = shadow_layers(style);
            match &snapshot.shadow_tween {
                Some(t) => {
                    if t.to != specified_shadow_value(t, &current) {
                        return true;
                    }
                }
                None => return true,
            }
            continue;
        }
        let (current, comps) = extract_prop(style, prop);
        if comps == 0 || !prop_is_interpolable(style, prop) {
            continue;
        }
        match snapshot.tweens.get(&prop) {
            Some(t) => {
                let target = specified_prop_value(t, current, comps);
                if (0..comps as usize).any(|i| (t.to[i] - target[i]).abs() > 1e-5) {
                    return true;
                }
            }
            None => return true,
        }
    }
    false
}

fn duration_tween_elapsed_ms(tween: &mut PropertyTween, now_ms: f64) -> f32 {
    let raw = (now_ms - tween.start_ms).max(0.0);
    let Some(max_step) = CSS_TRANSITION_MAX_STEP_MS.get() else {
        tween.last_ms = now_ms;
        return raw as f32;
    };
    let progressed = (tween.last_ms - tween.start_ms).max(0.0);
    let since_last = (now_ms - tween.last_ms).max(0.0);
    let step = if since_last > CSS_TRANSITION_IDLE_JUMP_MS as f64 {
        since_last.min(max_step as f64)
    } else {
        since_last
    };
    let elapsed = (progressed + step).min(raw);
    tween.last_ms = tween.start_ms + elapsed;
    elapsed as f32
}

fn tween_sample(tween: &mut PropertyTween, now_ms: f64, dt_secs: f32) -> ([f32; 8], f32) {
    if prefers_reduced_motion() {
        return (tween.to, 1.0);
    }
    if let TimingFunction::SpringOptions(opts) = tween.timing {
        return tween_sample_configured_spring(tween, now_ms, opts);
    }
    if let TimingFunction::Spring {
        stiffness,
        damping,
        mass,
    } = tween.timing
    {
        return tween_sample_spring(tween, now_ms, dt_secs, stiffness, damping, mass);
    }
    let elapsed_ms = duration_tween_elapsed_ms(tween, now_ms);
    let eff_elapsed = (elapsed_ms - tween.delay_ms).max(0.0);
    let linear_t = (eff_elapsed / tween.duration_ms.max(0.001)).min(1.0);
    let eased = tween.timing.eval(linear_t);
    (
        lerp_arr(&tween.from, &tween.to, eased, tween.components),
        linear_t,
    )
}

fn tween_sample_configured_spring(
    tween: &mut PropertyTween,
    now_ms: f64,
    opts: super::generator::SpringOptions,
) -> ([f32; 8], f32) {
    let elapsed = duration_tween_elapsed_ms(tween, now_ms);
    if elapsed < tween.delay_ms {
        return (tween.from, 0.0);
    }
    let mut out = tween.from;
    let mut done = true;
    for i in 0..tween.components as usize {
        let spring = tween.springs[i]
            .get_or_insert_with(|| Spring::new(tween.from[i], tween.to[i], tween.vel[i], opts));
        let sample = spring.next(elapsed - tween.delay_ms);
        out[i] = sample.value;
        tween.vel[i] = sample.velocity;
        done &= sample.done;
    }
    (out, if done { 1.0 } else { 0.0 })
}

fn tween_sample_spring(
    tween: &mut PropertyTween,
    now_ms: f64,
    _dt_secs: f32,
    stiffness: f32,
    damping: f32,
    mass: f32,
) -> ([f32; 8], f32) {
    // Same idle-jump policy as duration tweens. `Spring::next(t)` is absolute:
    // a hitch / `ControlFlow::Wait` would otherwise sample mid-flight and skip
    // the start (gallery Go/Reset).
    let elapsed_ms = duration_tween_elapsed_ms(tween, now_ms);
    if elapsed_ms < tween.delay_ms {
        return (tween.from, 0.0);
    }
    let t_ms = elapsed_ms - tween.delay_ms;
    let physics = SpringPhysics {
        stiffness,
        damping,
        mass,
    };
    let mut out = tween.from;
    let mut all_settled = true;
    let n = tween.components as usize;
    for i in 0..n {
        if tween.springs[i].is_none() {
            tween.springs[i] = Some(
                Spring::from_physics(tween.from[i], tween.to[i], tween.vel[i], physics)
                    .with_rest(8e-3, 8e-4),
            );
        }
        let sample = tween.springs[i].as_ref().map(|s| s.next(t_ms)).unwrap_or(
            super::generator::GeneratorSample {
                value: tween.to[i],
                velocity: 0.0,
                done: true,
            },
        );
        out[i] = sample.value;
        tween.vel[i] = sample.velocity;
        all_settled &= sample.done;
    }
    (out, if all_settled { 1.0 } else { 0.0 })
}

fn filter_tween_sample(tween: &FilterTween, now_ms: f64) -> (Option<FilterList>, f32) {
    let elapsed_ms = (now_ms - tween.start_ms).max(0.0) as f32;
    let eff_elapsed = (elapsed_ms - tween.delay_ms).max(0.0);
    let linear_t = (eff_elapsed / tween.duration_ms.max(0.001)).min(1.0);
    let eased = tween.timing.eval(linear_t);
    let list = match interpolate_filter_lists(tween.from.as_ref(), tween.to.as_ref(), eased) {
        Some(list) => {
            if list.is_empty() {
                None
            } else {
                Some(list)
            }
        }
        // Discrete: hold start until finished, then snap to end.
        None => {
            if linear_t >= 1.0 - 1e-6 {
                tween.to.clone()
            } else {
                tween.from.clone()
            }
        }
    };
    (list, linear_t)
}

fn clip_tween_sample(tween: &ClipPathTween, now_ms: f64) -> (ClipPath, f32) {
    let elapsed_ms = (now_ms - tween.start_ms).max(0.0) as f32;
    let eff_elapsed = (elapsed_ms - tween.delay_ms).max(0.0);
    let linear_t = (eff_elapsed / tween.duration_ms.max(0.001)).min(1.0);
    let eased = tween.timing.eval(linear_t);
    (tween.from.interpolate(&tween.to, eased), linear_t)
}

fn shadow_layers(style: &super::Style) -> Vec<BoxShadow> {
    let mut layers = vec![BoxShadow {
        color: style.shadow_color,
        offset: style.shadow_offset,
        blur: style.shadow_blur,
        spread: style.shadow_spread,
        inset: style.shadow_inset,
    }];
    layers.extend(style.extra_shadows.iter().cloned());
    layers
}

fn write_shadow_layers(style: &mut super::Style, layers: &[BoxShadow]) {
    if let Some(first) = layers.first() {
        style.shadow_color = first.color;
        style.shadow_offset = first.offset;
        style.shadow_blur = first.blur.max(0.0);
        style.shadow_spread = first.spread;
        style.shadow_inset = first.inset;
        style.extra_shadows = layers.iter().skip(1).cloned().collect();
    } else {
        style.shadow_color = [0.0; 4];
        style.shadow_offset = [0.0; 2];
        style.shadow_blur = 0.0;
        style.shadow_spread = 0.0;
        style.shadow_inset = false;
        style.extra_shadows.clear();
    }
}

fn interpolate_shadow_layers(
    from: &[BoxShadow],
    to: &[BoxShadow],
    t: f32,
) -> Option<Vec<BoxShadow>> {
    if from.len() != to.len() {
        return None;
    }
    let t = t.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(from.len());
    for (a, b) in from.iter().zip(to.iter()) {
        if a.inset != b.inset {
            return None;
        }
        out.push(BoxShadow {
            color: [
                a.color[0] + (b.color[0] - a.color[0]) * t,
                a.color[1] + (b.color[1] - a.color[1]) * t,
                a.color[2] + (b.color[2] - a.color[2]) * t,
                a.color[3] + (b.color[3] - a.color[3]) * t,
            ],
            offset: [
                a.offset[0] + (b.offset[0] - a.offset[0]) * t,
                a.offset[1] + (b.offset[1] - a.offset[1]) * t,
            ],
            blur: a.blur + (b.blur - a.blur) * t,
            spread: a.spread + (b.spread - a.spread) * t,
            inset: a.inset,
        });
    }
    Some(out)
}

fn shadow_tween_sample(tween: &ShadowListTween, now_ms: f64) -> (Vec<BoxShadow>, f32) {
    let elapsed_ms = (now_ms - tween.start_ms).max(0.0) as f32;
    let eff_elapsed = (elapsed_ms - tween.delay_ms).max(0.0);
    let linear_t = (eff_elapsed / tween.duration_ms.max(0.001)).min(1.0);
    let eased = tween.timing.eval(linear_t);
    let list = match interpolate_shadow_layers(&tween.from, &tween.to, eased) {
        Some(list) => list,
        None => {
            if linear_t >= 1.0 - 1e-6 {
                tween.to.clone()
            } else {
                tween.from.clone()
            }
        }
    };
    (list, linear_t)
}

fn specified_shadow_value(tween: &ShadowListTween, current: &[BoxShadow]) -> Vec<BoxShadow> {
    if tween
        .written
        .as_ref()
        .is_some_and(|w| shadow_layers_close(w, current))
    {
        tween.to.clone()
    } else {
        current.to_vec()
    }
}

fn shadow_layers_close(a: &[BoxShadow], b: &[BoxShadow]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            x.inset == y.inset
                && (0..4).all(|i| (x.color[i] - y.color[i]).abs() < 1e-4)
                && (x.offset[0] - y.offset[0]).abs() < 1e-4
                && (x.offset[1] - y.offset[1]).abs() < 1e-4
                && (x.blur - y.blur).abs() < 1e-4
                && (x.spread - y.spread).abs() < 1e-4
        })
}

/// 把当前 style 值记为已 settle 的 transition 基线（`from=to=current`，inactive）。
/// 用于 Carousel 无缝瞬移等：清 snapshot 后若直接写入新目标，首帧 seed 会 `from=to=新目标` 而跳过动画。
///
/// `style.transition` 必须已经挂上：为空时无法选择要建档的属性，只会清空 snapshot。
/// After a bind writes a new specified value, sample standing transitions so the
/// used style (and dest-reset AABB) stay at the current pose. Leaving `style` at
/// the destination until the next render tick dest-resets a hole at the end and
/// paints one flash frame (gallery Go/Reset).
pub fn commit_style_transitions_after_specified_write(el: &mut crate::core::tree::Element) {
    if el.style.transition.is_empty() {
        return;
    }
    let _ = el.tick_css_transitions(0.0);
}

pub fn snap_element_transitions_to_current(
    style: &super::Style,
    snapshot: &mut TransitionSnapshot,
) {
    if style.transition.is_empty() {
        snapshot.tweens.clear();
        snapshot.filter_tween = None;
        snapshot.clip_tween = None;
        snapshot.shadow_tween = None;
        return;
    }
    let now_ms = css_timeline_now_ms();
    let mut config_map: HashMap<TransitionProperty, usize> = HashMap::new();
    for (idx, entry) in style.transition.iter().enumerate() {
        if entry.property == TransitionProperty::All {
            for &p in ALL_PROPS {
                config_map.entry(p).or_insert(idx);
            }
        } else {
            config_map.insert(entry.property, idx);
        }
    }
    snapshot.tweens.retain(|p, _| config_map.contains_key(p));
    for (&prop, &cfg_idx) in &config_map {
        if prop == TransitionProperty::Filter {
            let cur = style.filter.clone();
            snapshot.filter_tween = Some(FilterTween {
                from: cur.clone(),
                to: cur.clone(),
                start_ms: now_ms,
                duration_ms: style.transition[cfg_idx].duration_ms,
                delay_ms: style.transition[cfg_idx].delay_ms,
                timing: style.transition[cfg_idx].timing,
                active: false,
                written: Some(cur),
            });
            continue;
        }
        if prop == TransitionProperty::ClipPath {
            let cur = style.clip_path.clone();
            snapshot.clip_tween = Some(ClipPathTween {
                from: cur.clone(),
                to: cur.clone(),
                start_ms: now_ms,
                duration_ms: style.transition[cfg_idx].duration_ms,
                delay_ms: style.transition[cfg_idx].delay_ms,
                timing: style.transition[cfg_idx].timing,
                active: false,
                written: Some(cur),
            });
            continue;
        }
        if prop == TransitionProperty::BoxShadow {
            let cur = shadow_layers(style);
            snapshot.shadow_tween = Some(ShadowListTween {
                from: cur.clone(),
                to: cur.clone(),
                start_ms: now_ms,
                duration_ms: style.transition[cfg_idx].duration_ms,
                delay_ms: style.transition[cfg_idx].delay_ms,
                timing: style.transition[cfg_idx].timing,
                active: false,
                written: Some(cur),
            });
            continue;
        }
        let (val, comps) = extract_prop(style, prop);
        if comps == 0 || !prop_is_interpolable(style, prop) {
            snapshot.tweens.remove(&prop);
            continue;
        }
        let cfg = &style.transition[cfg_idx];
        snapshot.tweens.insert(
            prop,
            PropertyTween {
                from: val,
                to: val,
                components: comps,
                start_ms: now_ms,
                duration_ms: cfg.duration_ms,
                delay_ms: cfg.delay_ms,
                timing: cfg.timing,
                active: false,
                written: Some(val),
                vel: [0.0; 8],
                last_ms: now_ms,
                springs: [const { None }; 8],
            },
        );
    }
}

/// Freeze-path `:hover` color is geometric: overlay / leave writes the specified fill,
/// then the following transition tick must not put the previous used color back.
///
/// Button `transition-colors` (~150ms) seeds `from=rest` before overlay. Without this
/// settle, enter never shows hover paint and leave keeps hover ink until scroll settles.
pub(crate) fn settle_hover_paint_tweens(
    style: &super::Style,
    text: &mut super::text_style::TextStyle,
    snapshot: &mut TransitionSnapshot,
) {
    TRANSITION_TEXT.with(|c| *c.borrow_mut() = Some(text as *mut _));
    for (prop, tween) in snapshot.tweens.iter_mut() {
        if !is_color_transition_prop(*prop) {
            continue;
        }
        let (cur, n) = extract_prop(style, *prop);
        if n == 0 {
            continue;
        }
        tween.from = cur;
        tween.to = cur;
        tween.written = Some(cur);
        tween.active = false;
        tween.vel = [0.0; 8];
        tween.springs = [const { None }; 8];
    }
    if let Some(st) = snapshot.shadow_tween.as_mut() {
        let cur = shadow_layers(style);
        st.from = cur.clone();
        st.to = cur.clone();
        st.written = Some(cur);
        st.active = false;
    }
    TRANSITION_TEXT.with(|c| *c.borrow_mut() = None);
}

/// Tick CSS transitions for a single element. Returns `true` if any tween is still active.
///
/// Call this **after** class-variant resolution has written the target style.
/// `snapshot` is mutable storage that persists across frames on the element.
///
/// Progress is sampled from the **monotonic CSS timeline** (`Instant`, or a virtual
/// clock in tests). Duration tweens ignore `dt_secs` so CursorMoved storms cannot
/// accelerate `transition-*`. Spring uses `dt_secs` only when the clock did not move.
pub fn tick_element_transitions(
    style: &mut super::Style,
    snapshot: &mut TransitionSnapshot,
    dt_secs: f32,
) -> bool {
    tick_element_transitions_locked(style, snapshot, AnimationLock::default(), dt_secs)
}

/// [`tick_element_transitions`], with the properties a running `@keyframes` owns.
///
/// CSS gives animations priority over transitions, and a change *caused by* an animation
/// never starts a transition. Sharing one `Style` here means both engines write the same
/// fields, so the locked properties are dropped from the transition snapshot: when the
/// animation ends the tween re-seeds from wherever it left the value, with no snap-back.
pub fn tick_element_transitions_locked(
    style: &mut super::Style,
    snapshot: &mut TransitionSnapshot,
    lock: AnimationLock,
    dt_secs: f32,
) -> bool {
    if style.transition.is_empty() {
        let was_active = snapshot.has_active_tweens();
        if !snapshot.tweens.is_empty() {
            snapshot.tweens.clear();
        }
        snapshot.filter_tween = None;
        snapshot.clip_tween = None;
        snapshot.shadow_tween = None;
        snapshot.settled_this_tick = was_active;
        return false;
    }

    snapshot.settled_this_tick = false;
    let was_active = snapshot.has_active_tweens();
    let now_ms = css_timeline_now_ms();

    // Clone transition config to avoid borrow conflict with style mutation.
    let transition_entries: Vec<TransitionEntry> = style.transition.clone();

    // Build a lookup: for each concrete property, find the best matching entry index.
    let mut config_map: HashMap<TransitionProperty, usize> = HashMap::new();
    for (idx, entry) in transition_entries.iter().enumerate() {
        if entry.property == TransitionProperty::All {
            for &p in ALL_PROPS {
                config_map.entry(p).or_insert(idx);
            }
        } else {
            config_map.insert(entry.property, idx);
        }
    }

    let mut any_active = false;

    // Filter uses list interpolation (or discrete when kinds don't align).
    if let Some(&cfg_idx) = config_map.get(&TransitionProperty::Filter) {
        let cfg = &transition_entries[cfg_idx];
        let duration_ms = if prefers_reduced_motion() {
            0.0
        } else {
            cfg.duration_ms
        };
        let current = style.filter.clone();
        let tween = snapshot.filter_tween.get_or_insert_with(|| FilterTween {
            from: current.clone(),
            to: current.clone(),
            start_ms: now_ms,
            duration_ms,
            delay_ms: cfg.delay_ms,
            timing: cfg.timing,
            active: false,
            written: None,
        });

        if animation_owns_prop(TransitionProperty::Filter, lock) {
            tween.from = current.clone();
            tween.to = current.clone();
            tween.active = false;
            tween.written = Some(current);
        } else {
            let target = specified_filter_value(tween, &current);
            let target_changed = tween.to != target;
            if target_changed {
                if tween.active {
                    let (mid, _) = filter_tween_sample(tween, now_ms);
                    tween.from = mid;
                }
                tween.to = target;
                tween.duration_ms = duration_ms;
                tween.delay_ms = cfg.delay_ms;
                tween.timing = cfg.timing;
                tween.active = true;
                tween.start_ms = now_ms;
            }

            if tween.active {
                let (interpolated, linear_t) = filter_tween_sample(tween, now_ms);
                if linear_t >= 1.0 - 1e-6 {
                    tween.active = false;
                    tween.from = tween.to.clone();
                    style.filter = tween.to.clone();
                } else {
                    style.filter = interpolated;
                    any_active = true;
                }
                tween.written = Some(style.filter.clone());
            }
        }
    } else {
        snapshot.filter_tween = None;
    }

    // clip-path uses the same interpolator as keyframes (`ClipPath::interpolate`).
    if let Some(&cfg_idx) = config_map.get(&TransitionProperty::ClipPath) {
        let cfg = &transition_entries[cfg_idx];
        let duration_ms = if prefers_reduced_motion() {
            0.0
        } else {
            cfg.duration_ms
        };
        let current = style.clip_path.clone();
        let tween = snapshot.clip_tween.get_or_insert_with(|| ClipPathTween {
            from: current.clone(),
            to: current.clone(),
            start_ms: now_ms,
            duration_ms,
            delay_ms: cfg.delay_ms,
            timing: cfg.timing,
            active: false,
            written: None,
        });

        if animation_owns_prop(TransitionProperty::ClipPath, lock) {
            tween.from = current.clone();
            tween.to = current.clone();
            tween.active = false;
            tween.written = Some(current);
        } else {
            let target = specified_clip_value(tween, &current);
            let target_changed = tween.to != target;
            if target_changed {
                if tween.active {
                    let (mid, _) = clip_tween_sample(tween, now_ms);
                    tween.from = mid;
                }
                tween.to = target;
                tween.duration_ms = duration_ms;
                tween.delay_ms = cfg.delay_ms;
                tween.timing = cfg.timing;
                tween.active = true;
                tween.start_ms = now_ms;
            }

            if tween.active {
                let (interpolated, linear_t) = clip_tween_sample(tween, now_ms);
                if linear_t >= 1.0 - 1e-6 {
                    tween.active = false;
                    tween.from = tween.to.clone();
                    style.clip_path = tween.to.clone();
                } else {
                    style.clip_path = interpolated;
                    any_active = true;
                }
                tween.written = Some(style.clip_path.clone());
            }
        }
    } else {
        snapshot.clip_tween = None;
    }

    if let Some(&cfg_idx) = config_map.get(&TransitionProperty::BoxShadow) {
        let cfg = &transition_entries[cfg_idx];
        let duration_ms = if prefers_reduced_motion() {
            0.0
        } else {
            cfg.duration_ms
        };
        let current = shadow_layers(style);
        let tween = snapshot
            .shadow_tween
            .get_or_insert_with(|| ShadowListTween {
                from: current.clone(),
                to: current.clone(),
                start_ms: now_ms,
                duration_ms,
                delay_ms: cfg.delay_ms,
                timing: cfg.timing,
                active: false,
                written: None,
            });

        if animation_owns_prop(TransitionProperty::BoxShadow, lock) {
            tween.from = current.clone();
            tween.to = current.clone();
            tween.active = false;
            tween.written = Some(current);
        } else {
            let target = specified_shadow_value(tween, &current);
            let target_changed = tween.to != target;
            if target_changed {
                if tween.active {
                    let (mid, _) = shadow_tween_sample(tween, now_ms);
                    tween.from = mid;
                }
                tween.to = target;
                tween.duration_ms = duration_ms;
                tween.delay_ms = cfg.delay_ms;
                tween.timing = cfg.timing;
                tween.active = true;
                tween.start_ms = now_ms;
            }

            if tween.active {
                let (interpolated, linear_t) = shadow_tween_sample(tween, now_ms);
                if linear_t >= 1.0 - 1e-6 {
                    tween.active = false;
                    tween.from = tween.to.clone();
                    write_shadow_layers(style, &tween.to);
                } else {
                    write_shadow_layers(style, &interpolated);
                    any_active = true;
                }
                tween.written = Some(shadow_layers(style));
            }
        }
    } else {
        snapshot.shadow_tween = None;
    }

    for (&prop, &cfg_idx) in &config_map {
        if matches!(
            prop,
            TransitionProperty::Filter
                | TransitionProperty::ClipPath
                | TransitionProperty::BoxShadow
        ) {
            continue;
        }
        let cfg = &transition_entries[cfg_idx];
        let duration_ms = if prefers_reduced_motion() {
            0.0
        } else {
            cfg.duration_ms
        };
        let (current, comps) = extract_prop(style, prop);
        if comps == 0 {
            continue;
        }
        // A running animation owns this property. Park the tween on whatever the
        // keyframes painted so the animation-driven change starts no transition, and so
        // the handover on animation end has no value to snap back to.
        if animation_owns_prop(prop, lock) {
            snapshot.tweens.insert(
                prop,
                PropertyTween {
                    from: current,
                    to: current,
                    components: comps,
                    start_ms: now_ms,
                    duration_ms,
                    delay_ms: cfg.delay_ms,
                    timing: cfg.timing,
                    active: false,
                    written: Some(current),
                    vel: [0.0; 8],
                    last_ms: now_ms,
                    springs: [const { None }; 8],
                },
            );
            continue;
        }
        if !prop_is_interpolable(style, prop) {
            // Discrete: the owner's assignment stands as-is, and any in-flight tween is
            // abandoned rather than resolving a keyword to a bogus number.
            snapshot.tweens.remove(&prop);
            continue;
        }

        let tween = snapshot
            .tweens
            .entry(prop)
            .or_insert_with(|| PropertyTween {
                from: current,
                to: current,
                components: comps,
                start_ms: now_ms,
                duration_ms,
                delay_ms: cfg.delay_ms,
                timing: cfg.timing,
                active: false,
                written: None,
                vel: [0.0; 8],
                last_ms: now_ms,
                springs: [const { None }; 8],
            });

        let target = specified_prop_value(tween, current, comps);
        // Target changed → retarget from the **current computed value** (CSS transition).
        let target_changed = (0..comps as usize).any(|i| (tween.to[i] - target[i]).abs() > 1e-5);
        if target_changed {
            tween.components = comps;
            if tween.active {
                let (mid, _) = tween_sample(tween, now_ms, dt_secs);
                tween.from = mid;
            }
            // Inactive: `from` already equals last settled `to`.
            tween.to = target;
            tween.duration_ms = duration_ms;
            tween.delay_ms = cfg.delay_ms;
            tween.timing = cfg.timing;
            tween.active = true;
            tween.start_ms = now_ms;
            tween.last_ms = now_ms;
            tween.springs = [const { None }; 8];
            // Keep `vel` so a mid-flight retarget carries speed into the new rest pose.
        }

        if !tween.active {
            continue;
        }

        let (interpolated, linear_t) = tween_sample(tween, now_ms, dt_secs);
        let written = if linear_t >= 1.0 - 1e-6 {
            tween.active = false;
            tween.from = tween.to;
            tween.vel = [0.0; 8];
            tween.springs = [const { None }; 8];
            tween.to
        } else {
            any_active = true;
            if is_color_transition_prop(prop) && !tween.timing.is_spring() {
                let eased = tween.timing.eval(linear_t);
                lerp_packed_color(&tween.from, &tween.to, eased)
            } else {
                interpolated
            }
        };
        write_prop(style, prop, &written, tween.components);
        tween.written = Some(written);
    }

    snapshot.tweens.retain(|p, _| config_map.contains_key(p));

    if was_active && !any_active {
        snapshot.settled_this_tick = true;
    }
    any_active
}

#[cfg(test)]
mod css_transition_dt_tests {
    use super::*;
    use crate::style::element_style::{TransitionEntry, TransitionProperty};
    use crate::style::TimingFunction;

    #[test]
    fn spring_transition_retargets_from_current_value() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.opacity = 0.0;
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Opacity,
            duration_ms: 0.0,
            delay_ms: 0.0,
            timing: TimingFunction::default_spring(),
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.opacity = 1.0;
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(50.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = style.opacity;
        assert!(mid > 0.05 && mid < 0.95, "spring mid {mid}");
        style.opacity = 0.0;
        set_css_transition_timeline_ms(50.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.opacity - mid).abs() < 0.2,
            "retarget must start near mid {mid}, got {}",
            style.opacity
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn monotonic_timeline_ignores_dt_storm() {
        set_css_transition_virtual_timeline(false);
        let mut style = crate::style::Style::default();
        style.scale = [1.0, 1.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 150.0,
            delay_ms: 0.0,
            timing: TimingFunction::Ease,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.scale = [1.5, 1.0];
        let t0 = Instant::now();
        assert!(tick_element_transitions(&mut style, &mut snap, 1.0));
        // Invented 1s dt × many ticks must not finish the tween — progress is Instant-based.
        for _ in 0..20 {
            style.scale = [1.5, 1.0];
            let _ = tick_element_transitions(&mut style, &mut snap, 1.0);
        }
        let mono_ms = t0.elapsed().as_secs_f32() * 1000.0;
        assert!(
            snap.has_active_tweens() || mono_ms >= 140.0,
            "monotonic tween must stay active unless real Instant covered the duration"
        );
        if mono_ms < 40.0 {
            assert!(
                style.scale[0] < 1.35,
                "invented dt must not finish a 150ms tween in {mono_ms}ms (scale={})",
                style.scale[0]
            );
        }
    }

    #[test]
    fn duration_authored_css_spring_finishes_at_its_declared_time() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut el = crate::core::tree::Element::new().transition("transform spring(800ms, 0.35)");
        let mut snap = TransitionSnapshot::default();
        tick_element_transitions(&mut el.style, &mut snap, 0.0);
        el.style.translate[0] = 196.0;
        tick_element_transitions(&mut el.style, &mut snap, 0.0);
        set_css_transition_timeline_ms(400.0);
        assert!(tick_element_transitions(&mut el.style, &mut snap, 0.0));
        set_css_transition_timeline_ms(800.0);
        assert!(!tick_element_transitions(&mut el.style, &mut snap, 0.0));
        assert_eq!(el.style.translate[0], 196.0);
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn virtual_timeline_advances_only_when_seeked() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.scale = [1.0, 1.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 150.0,
            delay_ms: 0.0,
            timing: TimingFunction::Ease,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.scale = [1.5, 1.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(16.0);
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        let after_one = style.scale[0];
        assert!(after_one > 1.0 && after_one < 1.5, "got {after_one}");
        for _ in 0..40 {
            style.scale = [1.5, 1.0];
            let _ = tick_element_transitions(&mut style, &mut snap, 1.0);
        }
        assert!(
            (style.scale[0] - after_one).abs() < 0.02,
            "passing dt must not invent progress; only timeline seeks move time"
        );
        advance_css_transition_timeline_ms(200.0);
        style.scale = [1.5, 1.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            !snap.has_active_tweens(),
            "virtual 150ms+ tween should settle"
        );
        assert!((style.scale[0] - 1.5).abs() < 0.01);
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn retarget_continues_from_current_computed_value() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.opacity = 0.5;
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Opacity,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        style.opacity = 1.0;
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(100.0); // mid-way → 0.75 on linear
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = style.opacity;
        assert!(
            (mid - 0.75).abs() < 0.02,
            "expected ~0.75 at 100ms, got {mid}"
        );
        // Leave: retarget to 0.5 from current (~0.75), not from 1.0.
        style.opacity = 0.5;
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        assert!(
            (snap.tweens.get(&TransitionProperty::Opacity).unwrap().from[0] - mid).abs() < 0.02
        );
        set_css_transition_timeline_ms(100.0); // same now — just retargeted, elapsed≈0
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.opacity - mid).abs() < 0.05,
            "right after leave retarget, value should stay near mid ({mid}), got {}",
            style.opacity
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn transform_transition_includes_translate() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [0.0, 0.0];
        style.scale = [1.0, 1.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.translate = [-200.0, 0.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(50.0);
        style.translate = [-200.0, 0.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate[0] + 100.0).abs() < 1.0,
            "mid translate should be ~-100, got {}",
            style.translate[0]
        );
        set_css_transition_timeline_ms(100.0);
        style.translate = [-200.0, 0.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate[0] + 200.0).abs() < 0.01,
            "settled translate should be -200, got {}",
            style.translate[0]
        );
        assert!(!snap.has_active_tweens());
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn duration_tween_idle_instant_jump_keeps_a_mid_frame() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_max_step_ms(Some(1000.0 / 60.0));
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [0.0, 0.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 300.0,
            delay_ms: 0.0,
            timing: TimingFunction::EaseOut,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.translate = [0.0, 800.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(400.0);
        style.translate = [0.0, 800.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            snap.has_active_tweens(),
            "idle Instant jump must not finish a 300ms tween in one tick"
        );
        assert!(
            style.translate[1] > 1.0 && style.translate[1] < 400.0,
            "first post-idle sample must be a mid, not the end (got {})",
            style.translate[1]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn spring_tween_idle_instant_jump_keeps_a_mid_frame() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_max_step_ms(Some(1000.0 / 60.0));
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [0.0, 0.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 0.0,
            delay_ms: 0.0,
            timing: TimingFunction::default_spring(),
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.translate = [0.0, 800.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(400.0);
        style.translate = [0.0, 800.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            snap.has_active_tweens(),
            "idle Instant jump must not finish a spring in one tick"
        );
        assert!(
            style.translate[1] > 0.01 && style.translate[1] < 400.0,
            "first post-idle spring sample must be a start/mid, not the end (got {})",
            style.translate[1]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn bind_specified_translate_commit_keeps_first_paint_at_start() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut el = crate::core::tree::Element::new();
        el.style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 400.0,
            delay_ms: 0.0,
            timing: TimingFunction::EaseOut,
        });
        let _ = el.tick_css_transitions(0.0);
        crate::style::apply_class_list(&mut el, "translate-x-40");
        let specified = el.style.translate[0];
        assert!(
            specified > 100.0,
            "translate-x-40 writes the end pose ({specified})"
        );
        crate::style::commit_style_transitions_after_specified_write(&mut el);
        assert!(
            el.style.translate[0] < specified * 0.2,
            "first painted frame must stay near start, not the class target (got {})",
            el.style.translate[0]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn translate_transition_interpolates_percent() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate_pct = [0.0, 0.5];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.translate_pct = [0.0, 0.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(50.0);
        style.translate_pct = [0.0, 0.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate_pct[1] - 0.25).abs() < 0.02,
            "mid translate_pct should be ~0.25, got {}",
            style.translate_pct[1]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn duration_tween_sub_idle_frame_is_realtime() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_max_step_ms(Some(1000.0 / 60.0));
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [0.0, 0.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 300.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.translate = [0.0, 300.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(33.0);
        style.translate = [0.0, 300.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate[1] - 33.0).abs() < 1.5,
            "33ms frame must not be slowed to 16ms (got {})",
            style.translate[1]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn snap_baseline_then_new_target_still_animates() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [-100.0, 0.0];
        // Simulate Carousel seamless jump: pin at -100 and seed, then write new target -300.
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        // Simulate Carousel seamless jump: pin at -100 and seed, then write new target -300.
        snap_element_transitions_to_current(&style, &mut snap);
        style.translate = [-300.0, 0.0];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(50.0);
        style.translate = [-300.0, 0.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate[0] + 200.0).abs() < 1.0,
            "should animate from -100 toward -300, mid≈-200, got {}",
            style.translate[0]
        );
        set_css_transition_virtual_timeline(false);
    }

    /// bugs.md: `transition: translate` after snap baseline should still tween,
    /// same as the verified `transform` path above.
    #[test]
    fn snap_baseline_then_new_target_still_animates_with_translate() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.translate = [-100.0, 0.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        snap_element_transitions_to_current(&style, &mut snap);
        assert!(
            snap.has_any_tweens(),
            "snap must seed a Translate tween (from=to=current, inactive)"
        );
        style.translate = [-300.0, 0.0];
        assert!(
            css_transition_needs_frame(&style, &snap),
            "new translate target after snap must request a frame"
        );
        assert!(
            tick_element_transitions(&mut style, &mut snap, 0.0),
            "tick must start an active Translate tween"
        );
        set_css_transition_timeline_ms(50.0);
        style.translate = [-300.0, 0.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.translate[0] + 200.0).abs() < 1.0,
            "should animate from -100 toward -300, mid≈-200, got {}",
            style.translate[0]
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn snap_while_transition_empty_does_not_seed() {
        let mut style = crate::style::Style::default();
        style.translate = [0.0, -100.0];
        let mut snap = TransitionSnapshot::default();
        snap_element_transitions_to_current(&style, &mut snap);
        assert!(
            !snap.has_any_tweens(),
            "snap cannot choose properties when transition is empty"
        );
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Translate,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        style.translate = [0.0, 40.0];
        assert!(
            !tick_element_transitions(&mut style, &mut snap, 0.0),
            "first tick after empty-snap seeds from=to=new target and skips the tween"
        );
        assert_eq!(style.translate[1], 40.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_set_same_target_does_not_restart_elapsed() {
        let tw = CubicTween::new_ui_transition(0.0);
        tw.set(1.0);
        let _ = tick_animations(0.05);
        let elapsed_mid = tw.0.borrow().elapsed;
        assert!(
            elapsed_mid > 0.04,
            "expected ~50ms progress, got {elapsed_mid}"
        );
        // Chrome sync style: set the same target again each frame
        tw.set(1.0);
        tw.set(1.0);
        assert!(
            (tw.0.borrow().elapsed - elapsed_mid).abs() < 1e-6,
            "same-target set must not reset elapsed"
        );
        // 再走完剩余 ≈100ms，应在 150ms 窗口内 settle
        for _ in 0..8 {
            let _ = tick_animations(0.02);
        }
        assert!(!tw.0.borrow().active);
        assert!((tw.peek() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn tick_clamps_near_zero_dt_so_first_frames_still_advance() {
        let tw = CubicTween::new_ui_transition(0.0);
        tw.set(1.0);
        // First-screen back-to-back frames / repeated RedrawRequested often yield raw dt≈0
        let _ = tick_animations(0.0);
        let elapsed = tw.0.borrow().elapsed;
        assert!(
            elapsed >= 1.0 / 240.0 - 1e-6,
            "near-zero dt must still advance (got {elapsed})"
        );
        assert!(tw.0.borrow().active);
        assert!(any_ui_animation_active());
    }

    #[test]
    fn tick_clamps_huge_dt_so_idle_wake_does_not_teleport() {
        let tw = CubicTween::new_ui_transition(0.0);
        tw.set(1.0);
        // First frame after Wait for several seconds: without clamping, 150ms would finish in one step
        let _ = tick_animations(2.0);
        assert!(
            tw.0.borrow().active,
            "huge dt must not finish a 150ms tween in one tick"
        );
        let v = tw.peek();
        assert!(
            v > 0.01 && v < 0.99,
            "expected mid-tween after clamped step, got {v}"
        );
    }

    #[test]
    fn tick_element_animations_spin_advances_rotate() {
        use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
        let mut style = crate::style::Style::default();
        style.animations.push(AnimationEntry {
            name: "spin".into(),
            duration_ms: 1000.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Infinite,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        // Engine clamps dt to ≤80ms per tick.
        for _ in 0..4 {
            assert!(tick_element_animations(&mut style, &mut snap, 0.08));
        }
        assert!(
            (style.rotate_deg - 115.2).abs() < 2.0,
            "expected ~115deg after 320ms of 1s spin, got {}",
            style.rotate_deg
        );
    }

    #[test]
    fn tick_element_animations_zero_dt_storm_uses_monotonic_clock() {
        // Gallery Marker: CursorMoved over the row requests extra paints with dt≈0.
        // Summing that froze loader-circle; flooring it would fast-forward during the storm.
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.animations.push(crate::style::AnimationEntry {
            name: "spin".into(),
            duration_ms: 1000.0,
            timing: crate::style::TimingFunction::Linear,
            iteration_count: crate::style::AnimationIterationCount::Infinite,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        assert!(tick_element_animations(&mut style, &mut snap, 0.0));
        let at_origin = style.rotate_deg;
        for _ in 0..12 {
            assert!(tick_element_animations(&mut style, &mut snap, 0.0));
        }
        assert!(
            (style.rotate_deg - at_origin).abs() < 1.0,
            "virtual clock must not invent spin from zero dt, origin={at_origin} now={}",
            style.rotate_deg
        );
        set_css_transition_timeline_ms(80.0);
        assert!(tick_element_animations(&mut style, &mut snap, 0.0));
        assert!(
            (style.rotate_deg - 28.8).abs() < 2.0,
            "dt≈0 extra frame must still sample wall/virtual time (expected ~28.8deg, got {})",
            style.rotate_deg
        );
        // Page-wide CursorMoved extra paints often report 1–8ms, not dt≈0 — enough
        // to take the frame-dt branch and ignore the CSS timeline (spin crawls).
        set_css_transition_timeline_ms(160.0);
        assert!(tick_element_animations(&mut style, &mut snap, 0.002));
        assert!(
            (style.rotate_deg - 57.6).abs() < 2.0,
            "tiny extra-frame dt must not starve the CSS timeline (expected ~57.6deg, got {})",
            style.rotate_deg
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn late_frame_must_not_drop_spin_time() {
        // Pointer motion blocks the GUI thread. The next present is late by
        // hundreds of ms and still reports a tiny frame dt. Capping the gap at
        // 80ms and stamping the clock to *now* deletes the rest — spin freezes
        // for the whole slide, then the icon pops when the mouse stops.
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.animations.push(crate::style::AnimationEntry {
            name: "spin".into(),
            duration_ms: 1000.0,
            timing: crate::style::TimingFunction::Linear,
            iteration_count: crate::style::AnimationIterationCount::Infinite,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        assert!(tick_element_animations(&mut style, &mut snap, 1.0 / 60.0));
        let seeded = style.rotate_deg;
        set_css_transition_timeline_ms(240.0);
        assert!(tick_element_animations(&mut style, &mut snap, 0.002));
        let advanced = style.rotate_deg - seeded;
        assert!(
            (advanced - 86.4).abs() < 3.0,
            "a 240ms late present must rotate the full gap, not the 80ms idle cap (seed {seeded:.1} now {:.1}, delta {advanced:.1})",
            style.rotate_deg
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn tick_element_animations_respects_paused() {
        use crate::style::{
            AnimationEntry, AnimationIterationCount, AnimationPlayState, TimingFunction,
        };
        let mut style = crate::style::Style::default();
        style.animations.push(AnimationEntry {
            name: "spin".into(),
            duration_ms: 1000.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Infinite,
            play_state: AnimationPlayState::Paused,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.25);
        assert!(
            style.rotate_deg.abs() < 1.0,
            "paused should not advance elapsed (got {})",
            style.rotate_deg
        );
    }

    #[test]
    fn pause_resume_preserves_elapsed() {
        use crate::style::{
            AnimationEntry, AnimationIterationCount, AnimationPlayState, TimingFunction,
        };
        let mut style = crate::style::Style::default();
        style.animations.push(AnimationEntry {
            name: "spin".into(),
            duration_ms: 1000.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Infinite,
            play_state: AnimationPlayState::Running,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        // Engine clamps dt to ≤80ms per tick → 3×80ms ≈ 86.4deg.
        for _ in 0..3 {
            assert!(tick_element_animations(&mut style, &mut snap, 0.08));
        }
        let mid = style.rotate_deg;
        assert!(mid > 70.0 && mid < 100.0, "expected ~86deg, got {mid}");

        style.animations[0].play_state = AnimationPlayState::Paused;
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        assert!(
            (style.rotate_deg - mid).abs() < 1.0,
            "pause must not reset elapsed (mid={mid}, after={})",
            style.rotate_deg
        );

        style.animations[0].play_state = AnimationPlayState::Running;
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        assert!(
            style.rotate_deg > mid + 20.0,
            "resume should continue from mid={mid}, got {}",
            style.rotate_deg
        );
    }

    #[test]
    fn tick_element_animations_forwards_keeps_end() {
        use crate::style::{
            AnimationEntry, AnimationFillMode, AnimationIterationCount, TimingFunction,
        };
        let mut style = crate::style::Style::default();
        style.opacity = 1.0;
        style.animations.push(AnimationEntry {
            name: "fade-out".into(),
            duration_ms: 100.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Finite(1.0),
            fill_mode: AnimationFillMode::Forwards,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        for _ in 0..5 {
            let _ = tick_element_animations(&mut style, &mut snap, 0.05);
        }
        assert!(
            style.opacity < 0.05,
            "forwards fade-out should stay near 0, got {}",
            style.opacity
        );
    }

    /// The component assigns a target **once**; every later frame just re-renders the
    /// value the engine wrote. Reading that back as a new target used to retarget the
    /// tween to itself each frame, freezing it a few percent from the start.
    #[test]
    fn transition_runs_to_completion_without_being_respecified() {
        use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style {
            opacity: 0.0,
            ..Default::default()
        };
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Opacity,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);

        style.opacity = 1.0; // the one and only assignment
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));

        set_css_transition_timeline_ms(100.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = style.opacity;
        assert!(
            (mid - 0.5).abs() < 0.05,
            "expected ~0.5 at 100ms, got {mid}"
        );

        set_css_transition_timeline_ms(200.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.opacity - 1.0).abs() < 0.01,
            "tween must reach the specified target, got {}",
            style.opacity
        );
        assert!(!snap.has_active_tweens());
        set_css_transition_virtual_timeline(false);
    }

    /// `transition: all` sweeps in `max-height`, which is `auto` on most boxes. Resolving
    /// `auto` to `0` collapsed accordion panels the moment anything animated.
    #[test]
    fn transition_all_leaves_auto_max_height_alone() {
        use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.transition.push(TransitionEntry {
            property: TransitionProperty::All,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        assert!(matches!(
            crate::layout::dim_kind(style.max_size.height),
            crate::layout::DimKind::Auto
        ));
        let mut snap = TransitionSnapshot::default();
        for ms in [0.0, 50.0, 200.0] {
            set_css_transition_timeline_ms(ms);
            let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        }
        assert!(
            matches!(
                crate::layout::dim_kind(style.max_size.height),
                crate::layout::DimKind::Auto
            ),
            "auto max-height must survive `transition: all`, got {:?}",
            style.max_size.height
        );
        assert!(!snap.tweens.contains_key(&TransitionProperty::MaxHeight));
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn from_css_recognizes_layout_properties() {
        use crate::style::TransitionProperty as P;
        assert_eq!(P::from_css("width"), P::Width);
        assert_eq!(P::from_css("min-width"), P::MinWidth);
        assert_eq!(P::from_css("min_width"), P::MinWidth);
        assert_eq!(P::from_css("height"), P::Height);
        assert_eq!(P::from_css("padding"), P::Padding);
        assert_eq!(P::from_css("padding-left"), P::PaddingLeft);
        assert_eq!(P::from_css("margin-top"), P::MarginTop);
        assert_eq!(P::from_css("gap"), P::Gap);
        assert_eq!(P::from_css("flex-grow"), P::FlexGrow);
        assert_eq!(P::from_css("border-radius"), P::BorderRadius);
        assert_eq!(P::from_css("zoom"), P::Zoom);
        assert_eq!(P::from_css("backdrop-filter"), P::BackdropFilter);
        assert_eq!(P::from_css("grid-template-rows"), P::GridTemplateRows);
        assert_eq!(P::from_css("grid-template-columns"), P::GridTemplateColumns);
        assert_eq!(P::from_css("background-position"), P::BackgroundPosition);
        assert_eq!(P::from_css("clip-path"), P::ClipPath);
        assert_eq!(P::from_css("scale"), P::Scale);
        assert_eq!(P::from_css("rotate"), P::Rotate);
        assert_eq!(P::from_css("display"), P::Unsupported);
    }

    #[test]
    fn width_length_tweens_auto_stays_discrete() {
        use taffy::style::Dimension;
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.size.width = Dimension::length(48.0);
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Width,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        style.size.width = Dimension::length(256.0);
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(100.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
        assert!(
            mid > 100.0 && mid < 220.0,
            "width must interpolate, got {mid}"
        );

        let mut auto_style = crate::style::Style::default();
        auto_style.transition.push(TransitionEntry {
            property: TransitionProperty::All,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut auto_snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut auto_style, &mut auto_snap, 0.0);
        assert!(
            matches!(
                crate::layout::dim_kind(auto_style.size.width),
                crate::layout::DimKind::Auto
            ),
            "auto width must survive `transition: all`"
        );
        assert!(!auto_snap.tweens.contains_key(&TransitionProperty::Width));
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn copied_used_width_keeps_standing_target_across_paint() {
        use taffy::style::Dimension;
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.size.width = Dimension::length(48.0);
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Width,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(!snap.has_active_tweens());

        let mut painted = crate::style::Style::default();
        painted.size.width = Dimension::length(256.0);
        painted.transition = style.transition.clone();
        copy_transition_used_values(&mut painted, &style, &snap);
        assert_eq!(
            crate::layout::dim_length_px(painted.size.width),
            Some(48.0),
            "rebuild paint 16rem must not replace the used collapsed width"
        );
        let _ = tick_element_transitions(&mut painted, &mut snap, 0.0);
        assert!(
            !snap.has_active_tweens(),
            "matching used value must keep the standing rest target, not expand"
        );
        assert_eq!(crate::layout::dim_length_px(painted.size.width), Some(48.0));
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn assign_specified_does_not_clobber_mid_when_target_unchanged() {
        use taffy::style::Dimension;
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.size.width = Dimension::length(256.0);
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Width,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        style.size.width = Dimension::length(48.0);
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(100.0);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
        assign_transition_specified(
            &mut style,
            &snap,
            TransitionProperty::Width,
            [48.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        );
        let after = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
        assert!(
            (after - mid).abs() < 0.5,
            "same-target assign must keep mid {mid}, got {after}"
        );
        set_css_transition_virtual_timeline(false);
    }

    /// An animation owns the properties its keyframes touch; the transition on the same
    /// property must not chase the animation's output (CSS: animation wins, and an
    /// animation-driven change never starts a transition).
    #[test]
    fn animation_lock_parks_transition_on_owned_property() {
        use crate::style::{
            AnimationEntry, AnimationFillMode, AnimationIterationCount, TimingFunction,
            TransitionEntry, TransitionProperty,
        };
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style {
            opacity: 1.0,
            ..Default::default()
        };
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Opacity,
            duration_ms: 200.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        style.animations.push(AnimationEntry {
            name: "fade-out".into(),
            duration_ms: 200.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Finite(1.0),
            fill_mode: AnimationFillMode::Forwards,
            ..Default::default()
        });
        let mut anim = AnimationSnapshot::default();
        let mut trans = TransitionSnapshot::default();
        for _ in 0..3 {
            let _ = tick_element_animations(&mut style, &mut anim, 0.05);
            let _ = tick_element_transitions_locked(&mut style, &mut trans, anim.lock(), 0.0);
        }
        assert!(anim.lock().opacity, "fade-out must claim opacity");
        let after = style.opacity;
        assert!(
            after > 0.05 && after < 0.95,
            "animation should drive opacity mid-way, got {after}"
        );
        assert!(
            !trans
                .tweens
                .get(&TransitionProperty::Opacity)
                .is_some_and(|t| t.active),
            "transition must stay parked while the animation owns opacity"
        );
        set_css_transition_virtual_timeline(false);
    }

    /// Keyframes are layered over the cascaded value and re-sampled from it every frame;
    /// dropping the animation hands that value back.
    #[test]
    fn animation_resamples_from_base_and_restores_on_removal() {
        use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
        let mut style = crate::style::Style {
            opacity: 1.0,
            ..Default::default()
        };
        style.animations.push(AnimationEntry {
            name: "fade-out".into(),
            duration_ms: 400.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Infinite,
            ..Default::default()
        });
        let mut snap = AnimationSnapshot::default();
        for _ in 0..2 {
            let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        }
        let mid = style.opacity;
        assert!(
            (mid - 0.6).abs() < 0.05,
            "160ms of a 400ms linear fade-out should read ~0.6, got {mid}"
        );

        style.animations.clear();
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
        assert!(
            (style.opacity - 1.0).abs() < 1e-4,
            "removing the animation must restore the cascaded opacity, got {}",
            style.opacity
        );
    }

    /// shadcn composes `fade-in-0 zoom-in-95` on one element. Each entry must only write the
    /// properties its own keyframes declare, or the later one snaps the earlier one back to
    /// the base value.
    #[test]
    fn stacked_animations_do_not_clobber_each_other() {
        use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
        let mut style = crate::style::Style {
            opacity: 1.0,
            ..Default::default()
        };
        for name in ["fade-in-0", "zoom-in-95"] {
            style.animations.push(AnimationEntry {
                name: name.into(),
                duration_ms: 200.0,
                timing: TimingFunction::Linear,
                iteration_count: AnimationIterationCount::Finite(1.0),
                ..Default::default()
            });
        }
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
        assert!(
            style.opacity > 0.05 && style.opacity < 0.95,
            "fade must survive the zoom entry, got {}",
            style.opacity
        );
        assert!(
            style.scale[0] > 0.95 && style.scale[0] < 1.0,
            "zoom must survive the fade entry, got {}",
            style.scale[0]
        );
        let lock = snap.lock();
        assert!(lock.opacity && lock.transform);
    }

    #[test]
    fn max_height_keyframes_pin_min_height_zero_and_restore_on_clear() {
        use crate::layout::dim_kind;
        use crate::layout::DimKind;
        use crate::style::{AnimationFillMode, TimingFunction};
        let mut style = crate::style::Style::default();
        style.max_size.height = taffy::style::Dimension::length(48.0);
        style.animations.push(keyframes_anim_entry(
            "accordion-up",
            200.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        ));
        let mut snap = AnimationSnapshot::default();
        assert!(matches!(dim_kind(style.min_size.height), DimKind::Auto));
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!(matches!(
            dim_kind(style.min_size.height),
            DimKind::Length(h) if h.abs() < f32::EPSILON
        ));
        for _ in 0..20 {
            let _ = tick_element_animations(&mut style, &mut snap, 0.016);
        }
        assert!(
            !animation_run_active(&style, &snap),
            "finished finite run must not block presence exit"
        );
        style.animations.clear();
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!(
            matches!(dim_kind(style.min_size.height), DimKind::Auto),
            "min-height must restore to cascaded auto after animation removal"
        );
    }

    #[test]
    fn finished_animation_retains_fill_without_requesting_frames() {
        use crate::style::{AnimationFillMode, TimingFunction};
        let mut style = crate::style::Style::default();
        style.max_size.height = taffy::style::Dimension::length(48.0);
        style.animations.push(keyframes_anim_entry(
            "accordion-up",
            200.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        ));
        let mut snap = AnimationSnapshot::default();
        for _ in 0..20 {
            let _ = tick_element_animations(&mut style, &mut snap, 0.016);
        }
        assert!(
            !css_animation_needs_frame(&style, &snap),
            "finished animation must not keep the frame pump alive"
        );
        assert!(
            !animation_run_active(&style, &snap),
            "presence gates must use animation_run_active, not css_animation_needs_frame"
        );
    }

    #[test]
    fn opacity_animation_preserves_auto_max_height() {
        use crate::layout::{dim_kind, DimKind};
        use crate::style::{AnimationFillMode, TimingFunction};
        let mut style = crate::style::Style::default();
        style.size.height = taffy::style::Dimension::length(32.0);
        style.animations.push(keyframes_anim_entry(
            "pulse",
            2000.0,
            TimingFunction::Linear,
            AnimationFillMode::None,
        ));
        style.animations[0].iteration_count = crate::style::AnimationIterationCount::Infinite;
        assert!(matches!(dim_kind(style.max_size.height), DimKind::Auto));
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
        assert!(
            matches!(dim_kind(style.max_size.height), DimKind::Auto),
            "pulse/spin/bounce must not rewrite max-height:auto to 0px, got {:?}",
            dim_kind(style.max_size.height)
        );
    }

    #[test]
    fn parse_animation_shorthand_spin_infinite() {
        let list = crate::core::tree::parse_animation_shorthand(
            "1s linear infinite reverse both running spin",
        );
        assert_eq!(list.len(), 1);
        let e = &list[0];
        assert_eq!(e.name, "spin");
        assert!((e.duration_ms - 1000.0).abs() < 0.1);
        assert!(matches!(
            e.iteration_count,
            crate::style::AnimationIterationCount::Infinite
        ));
        assert_eq!(e.direction, crate::style::AnimationDirection::Reverse);
        assert_eq!(e.fill_mode, crate::style::AnimationFillMode::Both);
        assert_eq!(e.play_state, crate::style::AnimationPlayState::Running);
    }

    #[test]
    fn parse_animation_shorthand_keeps_linear_stops_token() {
        let list = crate::core::tree::parse_animation_shorthand(
            "2s linear(0, 0.05 50%, 1) infinite gal-linear-slide",
        );
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "gal-linear-slide");
        assert!(
            matches!(list[0].timing, crate::style::TimingFunction::LinearStops(_)),
            "linear() must parse as piecewise easing, not the spin keyframe, got {:?}",
            list[0].timing
        );
    }

    #[test]
    fn parse_animation_shorthand_keeps_steps_token() {
        let list =
            crate::core::tree::parse_animation_shorthand("200ms steps(4, jump-end) both fade-in-0");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "fade-in-0");
        assert!(matches!(
            list[0].timing,
            crate::style::TimingFunction::Steps {
                n: 4,
                jump: crate::style::StepJumpTerm::JumpEnd
            }
        ));
    }

    #[test]
    fn reduced_motion_snaps_keyframes_to_end() {
        set_prefers_reduced_motion(Some(true));
        let mut style = crate::style::Style::default();
        style.opacity = 1.0;
        style.animations.push(keyframes_anim_entry(
            "fade-out-0",
            400.0,
            TimingFunction::Linear,
            crate::style::AnimationFillMode::Forwards,
        ));
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
        assert!(
            style.opacity < 0.05,
            "reduced motion must jump to the end sample, got {}",
            style.opacity
        );
        set_prefers_reduced_motion(None);
    }

    #[test]
    fn accordion_down_uses_used_layout_not_written_px() {
        use crate::style::UsedLayout;
        let mut style = crate::style::Style::default();
        style.max_size.height = taffy::style::Dimension::auto();
        style.animations.push(keyframes_anim_entry(
            "accordion-down",
            200.0,
            TimingFunction::Linear,
            crate::style::AnimationFillMode::Both,
        ));
        let mut text = crate::style::TextStyle::default();
        let mut used = UsedLayout::default();
        used.stamp_border_box([0.0, 0.0, 100.0, 80.0], false);
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
        let h = crate::layout::dim_length_px(style.max_size.height).unwrap_or(999.0);
        assert!(h < 2.0, "fill-mode both at t=0 must start at 0, got {h}");
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.1);
        let mid = crate::layout::dim_length_px(style.max_size.height).unwrap_or(0.0);
        assert!(
            mid > 1.0 && mid < 80.0,
            "mid accordion-down must interpolate toward used height, got {mid}"
        );
        match snap.specified.get(crate::style::PropertyId::MaxHeight) {
            Some(crate::style::AnimatableValue::Auto) | None => {}
            other => panic!("specified max-height must stay auto, got {other:?}"),
        }
    }

    #[test]
    fn color_transition_writes_text_style() {
        let mut style = crate::style::Style::default();
        style.transition.push(crate::style::TransitionEntry {
            property: crate::style::TransitionProperty::Color,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut text = crate::style::TextStyle::default();
        text.color = Some([0, 0, 0, 255]);
        let mut snap = TransitionSnapshot::default();
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let _ = tick_element_transitions_with_text(
            &mut style,
            &mut text,
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        text.color = Some([255, 255, 255, 255]);
        let _ = tick_element_transitions_with_text(
            &mut style,
            &mut text,
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        advance_css_transition_timeline_ms(50.0);
        let _ = tick_element_transitions_with_text(
            &mut style,
            &mut text,
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        let c = text.color.expect("color");
        assert!(
            c[0] > 10 && c[0] < 245,
            "color transition must be mid-gray, got {c:?}"
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn grid_template_rows_transition_lerps_fr() {
        use crate::layout::{first_track_fr, track_fr};
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.grid_template_rows = vec![track_fr(0.0)];
        style.transition.push(crate::style::TransitionEntry {
            property: crate::style::TransitionProperty::GridTemplateRows,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
        style.grid_template_rows = vec![track_fr(1.0)];
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        set_css_transition_timeline_ms(50.0);
        style.grid_template_rows = vec![track_fr(1.0)];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = first_track_fr(&style.grid_template_rows).unwrap();
        assert!(
            (mid - 0.5).abs() < 0.05,
            "expected ~0.5fr at 50ms linear, got {mid}"
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn grid_template_rows_transition_lerps_equal_length_list() {
        use crate::layout::{track_fr, tracks_fr_list};
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.grid_template_rows = vec![track_fr(0.0), track_fr(0.0)];
        style.transition.push(crate::style::TransitionEntry {
            property: crate::style::TransitionProperty::GridTemplateRows,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        set_css_transition_timeline_ms(50.0);
        style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        let mid = tracks_fr_list(&style.grid_template_rows).expect("fr list");
        assert_eq!(mid.len(), 2);
        assert!(
            (mid[0] - 0.5).abs() < 0.05 && (mid[1] - 0.5).abs() < 0.05,
            "expected ~0.5 0.5, got {mid:?}"
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn animation_composition_add_stacks_opacity() {
        use crate::style::{
            register_keyframes, AnimationComposition, AnimationFillMode, KeyframeStop,
            TimingFunction,
        };
        register_keyframes(
            "anim6-hold-03",
            vec![
                KeyframeStop::at(0.0).opacity(0.3),
                KeyframeStop::at(1.0).opacity(0.3),
            ],
        );
        register_keyframes(
            "anim6-hold-02",
            vec![
                KeyframeStop::at(0.0).opacity(0.2),
                KeyframeStop::at(1.0).opacity(0.2),
            ],
        );
        let mut style = crate::style::Style::default();
        style.opacity = 0.0;
        let mut a = keyframes_anim_entry(
            "anim6-hold-03",
            1000.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        );
        a.composition = AnimationComposition::Replace;
        let mut b = keyframes_anim_entry(
            "anim6-hold-02",
            1000.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        );
        b.composition = AnimationComposition::Add;
        style.animations.push(a);
        style.animations.push(b);
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations(&mut style, &mut snap, 0.0);
        assert!(
            (style.opacity - 0.5).abs() < 0.02,
            "replace 0.3 + add 0.2, got {}",
            style.opacity
        );
    }

    #[test]
    fn interpolate_size_allow_keywords_tweens_auto_height() {
        use crate::style::{InterpolateSize, TransitionEntry, TransitionProperty, UsedLayout};
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.size.height = taffy::style::Dimension::length(0.0);
        style.interpolate_size = InterpolateSize::AllowKeywords;
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Height,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut used = UsedLayout::default();
        used.content_height = 100.0;
        used.intrinsic_height = 100.0;
        used.border_box = [0.0, 0.0, 40.0, 100.0];
        let mut text = crate::style::TextStyle::default();
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions_with_text_used(
            &mut style,
            &mut text,
            Some(&used),
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        style.size.height = taffy::style::Dimension::auto();
        let _ = tick_element_transitions_with_text_used(
            &mut style,
            &mut text,
            Some(&used),
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        set_css_transition_timeline_ms(50.0);
        style.size.height = taffy::style::Dimension::auto();
        let _ = tick_element_transitions_with_text_used(
            &mut style,
            &mut text,
            Some(&used),
            &mut snap,
            AnimationLock::default(),
            0.0,
        );
        let h = crate::layout::dim_length_px(style.size.height).unwrap_or(0.0);
        assert!(
            (h - 50.0).abs() < 5.0,
            "interpolate-size should tween auto via used 100px, got {h}"
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn scroll_timeline_samples_progress_not_elapsed() {
        use crate::style::{AnimationFillMode, AnimationTimeline, TimingFunction};
        let mut style = crate::style::Style::default();
        style.opacity = 1.0;
        let mut entry = keyframes_anim_entry(
            "fade-in",
            10_000.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        );
        entry.timeline = AnimationTimeline::Scroll;
        style.animations.push(entry);
        let mut snap = AnimationSnapshot::default();
        let mut text = crate::style::TextStyle::default();
        let used = crate::style::UsedLayout::default();
        let _ = tick_element_animations_ex_progress(
            &mut style,
            &mut text,
            &used,
            &mut snap,
            1.0,
            TimelineProgress {
                scroll: 0.5,
                view: 0.0,
            },
        );
        assert!(
            (style.opacity - 0.5).abs() < 0.05,
            "scroll() progress 0.5 must sample fade-in mid, got {}",
            style.opacity
        );
    }

    #[test]
    fn background_position_and_clip_path_lerp() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = crate::style::Style::default();
        style.bg_position = [0.0, 0.0];
        style.clip_path = crate::style::ClipPath::Inset {
            trbl: [crate::style::ClipCoord::Px(0.0); 4],
        };
        style.transition.push(crate::style::TransitionEntry {
            property: crate::style::TransitionProperty::BackgroundPosition,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        style.transition.push(crate::style::TransitionEntry {
            property: crate::style::TransitionProperty::ClipPath,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        style.bg_position = [1.0, 1.0];
        style.clip_path = crate::style::ClipPath::Inset {
            trbl: [crate::style::ClipCoord::Px(20.0); 4],
        };
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        set_css_transition_timeline_ms(50.0);
        style.bg_position = [1.0, 1.0];
        style.clip_path = crate::style::ClipPath::Inset {
            trbl: [crate::style::ClipCoord::Px(20.0); 4],
        };
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.bg_position[0] - 0.5).abs() < 0.05,
            "bg-position mid, got {:?}",
            style.bg_position
        );
        let inset = style.clip_path.inset_trbl_px([100.0, 100.0]).expect("clip");
        assert!(
            (inset[0] - 10.0).abs() < 1.0,
            "clip-path inset mid, got {inset:?}"
        );
        set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn named_scroll_timeline_drives_descendant_animation() {
        use crate::core::tree::Element;
        use crate::style::{push_named_timelines, AnimationFillMode, AnimationTimeline};
        use taffy::style::Overflow;

        let mut scroller = Element::new();
        scroller.style.overflow.y = Overflow::Scroll;
        scroller.style.scroll_timeline_name = Some("--page".into());
        scroller.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        scroller.used_layout.content_height = 200.0;
        scroller.used_layout.intrinsic_height = 200.0;
        scroller.used_layout.border_box = [0.0, 0.0, 100.0, 200.0];
        scroller.scroll_offset = [0.0, 50.0];
        let mut named = Vec::new();
        push_named_timelines(&scroller, &mut named, [0.0, 0.0], 100.0);
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].0, "page");
        assert!(
            (named[0].1 - 0.5).abs() < 0.05,
            "scroller 50/100 → 0.5, got {}",
            named[0].1
        );

        let mut style = crate::style::Style::default();
        style.opacity = 1.0;
        let mut entry = keyframes_anim_entry(
            "fade-in",
            10_000.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        );
        entry.timeline = AnimationTimeline::Named("page".into());
        entry.range_start = Some("0%".into());
        entry.range_end = Some("100%".into());
        style.animations.push(entry);
        let mut snap = AnimationSnapshot::default();
        let mut text = crate::style::TextStyle::default();
        let used = crate::style::UsedLayout::default();
        let _ = tick_element_animations_ex_named(
            &mut style,
            &mut text,
            &used,
            &mut snap,
            1.0,
            TimelineProgress::ZERO,
            &named,
        );
        assert!(
            (style.opacity - 0.5).abs() < 0.05,
            "named --page at 0.5 must sample fade-in mid, got {}",
            style.opacity
        );
    }

    #[test]
    fn calc_size_keyframes_animate_to_used_plus_n() {
        use crate::layout::dimension::parse_css_dim;
        use crate::style::{
            register_keyframes, AnimatableValue, AnimationFillMode, KeyframeStop, PropertyId,
            UsedLayout,
        };
        register_keyframes(
            "calc-size-grow",
            vec![
                KeyframeStop::at(0.0).set(PropertyId::Height, AnimatableValue::Scalar(0.0)),
                KeyframeStop::at(1.0).set(
                    PropertyId::Height,
                    AnimatableValue::Dim(
                        parse_css_dim("calc-size(auto, size + 20px)").expect("parse"),
                    ),
                ),
            ],
        );
        let mut style = crate::style::Style::default();
        style.size.height = taffy::style::Dimension::length(0.0);
        style.animations.push(keyframes_anim_entry(
            "calc-size-grow",
            1.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        ));
        let mut used = UsedLayout::default();
        used.content_height = 100.0;
        used.intrinsic_height = 100.0;
        used.border_box = [0.0, 0.0, 40.0, 100.0];
        let mut text = crate::style::TextStyle::default();
        let mut snap = AnimationSnapshot::default();
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 1.0);
        let h = crate::layout::dim_length_px(style.size.height).unwrap_or(-1.0);
        assert!(
            (h - 120.0).abs() < 1.0,
            "calc-size(auto, size+20px) of used 100 → 120, got {h}"
        );
    }

    #[test]
    fn clip_path_polygon_keyframes_lerp_vertices() {
        use crate::style::{
            register_keyframes, AnimatableValue, AnimationFillMode, ClipCoord, ClipPath,
            KeyframeStop, PropertyId,
        };
        let from =
            crate::style::parse_clip_path("polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)").unwrap();
        let to =
            crate::style::parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap();
        register_keyframes(
            "clip-poly-lerp",
            vec![
                KeyframeStop::at(0.0).set(PropertyId::ClipPath, AnimatableValue::Clip(from)),
                KeyframeStop::at(1.0).set(PropertyId::ClipPath, AnimatableValue::Clip(to)),
            ],
        );
        let mut style = crate::style::Style::default();
        style.animations.push(keyframes_anim_entry(
            "clip-poly-lerp",
            100.0,
            TimingFunction::Linear,
            AnimationFillMode::Both,
        ));
        let mut snap = AnimationSnapshot::default();
        let mut text = crate::style::TextStyle::default();
        let used = crate::style::UsedLayout::default();
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
        let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.05);
        match &style.clip_path {
            ClipPath::Polygon { points, .. } => match points[0][0] {
                ClipCoord::Percent(p) => {
                    assert!((p - 0.125).abs() < 0.02, "polygon mid vertex, got {p}")
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }
}
