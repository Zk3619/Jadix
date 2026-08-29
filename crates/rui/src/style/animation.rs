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
        self.0.borrow().signal.peek()
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
                    let linear_t = (s.elapsed / s.duration).min(1.0);
                    let eased = cubic_bezier_y_at_x(linear_t, s.x1, s.y1, s.x2, s.y2);
                    s.value = s.from_value + (s.target - s.from_value) * eased;
                    if linear_t >= 1.0 - 1e-6 {
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

    AnimationTick {
        any: cubic_running || timeline_any,
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
}

// ─── CSS Transition engine ───────────────────────────────────────────────
//
// CSS-like model: progress = (timeline_now − start) / duration.
// Timeline source is a **monotonic** clock (`std::time::Instant`), never
// `SystemTime` / wall clock (NTP jumps must not seek animations).
// Each frame *samples* the timeline; it does not invent time by summing clamped `dt`.

use super::element_style::{
    AnimationDirection, AnimationEntry, AnimationFillMode, AnimationIterationCount,
    AnimationPlayState, TimingFunction, TransitionEntry, TransitionProperty,
};
use super::filter::{interpolate_filter_lists, FilterList};
use std::cell::Cell;
use std::collections::HashMap;
use std::time::Instant;

thread_local! {
    /// Monotonic origin for the document CSS-transition timeline (`performance.now()`-like).
    static CSS_TIMELINE_ORIGIN: RefCell<Option<Instant>> = const { RefCell::new(None) };
    /// When `Some`, overrides [`css_timeline_now_ms`] (unit tests / virtual clock).
    static CSS_TIMELINE_VIRTUAL_MS: Cell<Option<f64>> = const { Cell::new(None) };
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
    } else {
        CSS_TIMELINE_VIRTUAL_MS.set(None);
    }
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

/// Per-property in-flight tween state (CSS transition segment).
#[derive(Clone, Debug)]
struct PropertyTween {
    /// Packed components (opacity=1, color=4, transform=5: tx/ty/sx/sy/rotate).
    from: [f32; 8],
    to: [f32; 8],
    /// Number of meaningful components (1 for scalar, 4 for color, 5 for transform).
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
    /// unless its owner rewrote the target on every single frame.
    written: Option<[f32; 8]>,
}

/// Filter list tween (cannot fit in `[f32; 4]`).
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

/// Per-element transition snapshot stored alongside the element.
#[derive(Clone, Debug, Default)]
pub struct TransitionSnapshot {
    tweens: HashMap<TransitionProperty, PropertyTween>,
    filter_tween: Option<FilterTween>,
}

impl TransitionSnapshot {
    /// True while at least one property tween is still interpolating.
    pub(crate) fn has_active_tweens(&self) -> bool {
        self.tweens.values().any(|t| t.active)
            || self.filter_tween.as_ref().is_some_and(|t| t.active)
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
}

impl AnimationLock {
    pub fn any(self) -> bool {
        self.opacity || self.transform || self.max_height
    }
}

fn animation_owns_prop(prop: TransitionProperty, lock: AnimationLock) -> bool {
    match prop {
        TransitionProperty::Opacity => lock.opacity,
        TransitionProperty::Transform | TransitionProperty::Translate => lock.transform,
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
}

impl AnimationSnapshot {
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
    h.finish()
}

/// Write a sample back, but only for the properties the rule actually declares.
///
/// The sampler fills undeclared properties from the base so single-ended keyframes can
/// interpolate out of the cascade. Writing those back would let one entry of an
/// `animation` list clobber another: shadcn's `fade-in-0, zoom-in-95` lost its fade
/// because `zoom-in-95` reported the *base* opacity and ran last.
fn apply_keyframe_sample(
    style: &mut super::Style,
    stops: &[super::keyframes::KeyframeStop],
    sample: &super::keyframes::KeyframeSample,
) {
    if let Some(o) = sample
        .opacity
        .filter(|_| stops.iter().any(|s| s.opacity.is_some()))
    {
        style.opacity = o;
    }
    if let Some(r) = sample
        .rotate_deg
        .filter(|_| stops.iter().any(|s| s.rotate_deg.is_some()))
    {
        style.rotate_deg = r;
    }
    if let Some(s) = sample
        .scale
        .filter(|_| stops.iter().any(|s| s.scale.is_some()))
    {
        style.scale = s;
    }
    if let Some(t) = sample
        .translate
        .filter(|_| stops.iter().any(|s| s.translate.is_some()))
    {
        style.translate = t;
    }
    if let Some(h) = sample
        .max_height
        .filter(|_| stops.iter().any(|s| s.max_height.is_some()))
    {
        style.max_size.height = taffy::style::Dimension::length(h.max(0.0));
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
    stops: &[super::keyframes::KeyframeStop],
    local_t: f32,
    timing: TimingFunction,
    base: &super::keyframes::KeyframeBase,
    lock: &mut AnimationLock,
) {
    let sample = super::keyframes::sample_keyframes_eased(stops, local_t, timing, Some(base));
    apply_keyframe_sample(style, stops, &sample);
    let cov = super::keyframes::keyframe_coverage(stops);
    lock.opacity |= cov.opacity;
    lock.transform |= cov.transform;
    lock.max_height |= cov.max_height;
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
            eprintln!(
                "[rui] animation `{name}` has no @keyframes; the element will stay at its \
                 cascaded value. Register it with `register_keyframes(\"{name}\", ..)`."
            );
        }
    });
}

#[cfg(not(debug_assertions))]
fn warn_unknown_keyframes(_name: &str) {}

/// Map local cycle progress through `animation-direction`.
fn direction_local_t(
    direction: super::element_style::AnimationDirection,
    iteration_index: u32,
    cycle_t: f32,
) -> f32 {
    use super::element_style::AnimationDirection;
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

/// Advance CSS `@keyframes` animations on one element. Returns true if still active.
pub fn tick_element_animations(
    style: &mut super::Style,
    snap: &mut AnimationSnapshot,
    dt_secs: f32,
) -> bool {
    use super::element_style::{AnimationIterationCount, AnimationPlayState};
    use super::keyframes::{lookup_keyframes, KeyframeBase};

    if style.animations.is_empty() {
        // Animation removed: hand the cascaded value back, unless the component has
        // meanwhile specified something of its own (CSS drops to the base style here).
        if let (Some(base), Some(written)) = (snap.base, snap.written) {
            if baseline_matches(style, &written) {
                restore_baseline(style, &base);
            }
        }
        *snap = AnimationSnapshot::default();
        return false;
    }

    // Resolve the base value the animation is layered over. Keyframes are *not*
    // cumulative: every frame must sample from the cascaded style, otherwise our own
    // output from last frame becomes the new base and the animation drifts/freezes.
    let base = match (snap.base, snap.written) {
        (Some(b), Some(w)) if baseline_matches(style, &w) => b,
        _ => AnimationBaseline::from_style(style),
    };
    snap.base = Some(base);
    restore_baseline(style, &base);

    let kf_base = KeyframeBase {
        opacity: base.opacity,
        rotate_deg: base.rotate_deg,
        scale: base.scale,
        translate: base.translate,
        // Accordion interpolates toward the cascaded px; `auto` has no length → 0.
        max_height: crate::layout::dim_length_px(base.max_height).unwrap_or(0.0),
    };

    // Sync run slots with style list length / fingerprints.
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

    let dt_ms = (dt_secs.max(0.0) * 1000.0).min(80.0);
    let mut any_active = false;
    let mut lock = AnimationLock::default();
    let animations = style.animations.clone();

    for (i, entry) in animations.iter().enumerate() {
        if !entry.is_named() || !entry.timeline.is_auto() {
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

        let Some(stops) = lookup_keyframes(&entry.name) else {
            warn_unknown_keyframes(&entry.name);
            continue;
        };

        let (elapsed_ms, finished_before, paused) = {
            let run = &snap.runs[i];
            (
                run.elapsed_ms,
                run.finished,
                entry.play_state == AnimationPlayState::Paused,
            )
        };

        if paused {
            // Still paint the current fill/frame without advancing.
            if let Some(local) = animation_sample_t(entry, elapsed_ms) {
                apply_animation_sample(style, &stops, local, entry.timing, &kf_base, &mut lock);
            } else if entry.fill_mode.apply_backwards() && elapsed_ms < entry.delay_ms {
                let local = direction_local_t(entry.direction, 0, 0.0);
                apply_animation_sample(style, &stops, local, entry.timing, &kf_base, &mut lock);
            }
            if entry.needs_frame() || !finished_before {
                any_active = true;
            }
            continue;
        }

        let elapsed_ms = if finished_before {
            elapsed_ms
        } else {
            let next = elapsed_ms + dt_ms;
            snap.runs[i].elapsed_ms = next;
            next
        };

        let after_delay = elapsed_ms - entry.delay_ms;
        if after_delay < 0.0 {
            if entry.fill_mode.apply_backwards() {
                let local = direction_local_t(entry.direction, 0, 0.0);
                apply_animation_sample(style, &stops, local, entry.timing, &kf_base, &mut lock);
            }
            any_active = true;
            continue;
        }

        let dur = entry.duration_ms.max(1e-3);
        let total_iters = match entry.iteration_count {
            AnimationIterationCount::Infinite => None,
            AnimationIterationCount::Finite(n) => Some(n.max(0.0)),
        };

        let finished = matches!(total_iters, Some(n) if after_delay >= dur * n);

        if finished {
            snap.runs[i].finished = true;
            if entry.fill_mode.apply_forwards() {
                let last_iter = match total_iters {
                    Some(n) if n > 0.0 => (n.ceil() as u32).saturating_sub(1),
                    _ => 0,
                };
                // End of last iteration: local t = 1.0 before direction.
                let local = direction_local_t(entry.direction, last_iter, 1.0);
                apply_animation_sample(style, &stops, local, entry.timing, &kf_base, &mut lock);
            }
            // `fill-mode: none` needs no work: the base was already restored above.
            continue;
        }

        snap.runs[i].finished = false;
        any_active = true;

        let cycle_index = (after_delay / dur).floor() as u32;
        let cycle_t = (after_delay % dur) / dur;
        let local = direction_local_t(entry.direction, cycle_index, cycle_t);
        apply_animation_sample(style, &stops, local, entry.timing, &kf_base, &mut lock);
    }

    snap.lock = lock;
    pin_min_height_for_max_height_keyframes(style, lock);
    snap.written = Some(AnimationBaseline::from_style(style));
    any_active
}

/// True while a finite `@keyframes` run is still playing.
///
/// Prefer over [`css_animation_needs_frame`] for presence exit gates: the latter stays
/// true whenever `style.animations` is non-empty, even after all runs have `finished`.
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
    use super::element_style::AnimationIterationCount;
    let after_delay = elapsed_ms - entry.delay_ms;
    if after_delay < 0.0 {
        return None;
    }
    let dur = entry.duration_ms.max(1e-3);
    let total_iters = match entry.iteration_count {
        AnimationIterationCount::Infinite => None,
        AnimationIterationCount::Finite(n) => Some(n.max(0.0)),
    };
    if let Some(n) = total_iters {
        if after_delay >= dur * n {
            return Some(1.0);
        }
    }
    let cycle_index = (after_delay / dur).floor() as u32;
    let cycle_t = (after_delay % dur) / dur;
    Some(direction_local_t(entry.direction, cycle_index, cycle_t))
}

/// True when this element should keep the animation frame pump alive.
pub fn css_animation_needs_frame(style: &super::Style, snap: &AnimationSnapshot) -> bool {
    style.animations.iter().any(|e| e.needs_frame()) || snap.has_active(style)
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

/// CSS keywords like `auto` have no numeric value to interpolate.
///
/// Web needs `interpolate-size: allow-keywords` / `calc-size()` to animate to `auto`;
/// without this guard `max-height: auto` reads back as `0`, and the tween then pins the
/// box to zero height — turning a decorative `transition: all` into an invisible element.
fn prop_is_interpolable(style: &super::Style, prop: TransitionProperty) -> bool {
    match prop {
        TransitionProperty::MaxHeight => matches!(
            crate::layout::dim_kind(style.max_size.height),
            crate::layout::DimKind::Length(_)
        ),
        _ => true,
    }
}

fn extract_prop(style: &super::Style, prop: TransitionProperty) -> ([f32; 8], u8) {
    match prop {
        TransitionProperty::Opacity => ([style.opacity, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1),
        TransitionProperty::BackgroundColor => {
            let c = style.bg_color;
            ([c[0], c[1], c[2], c[3], 0.0, 0.0, 0.0, 0.0], 4)
        }
        TransitionProperty::BorderColor => {
            let c = style.border_color;
            ([c[0], c[1], c[2], c[3], 0.0, 0.0, 0.0, 0.0], 4)
        }
        TransitionProperty::OutlineColor => {
            let c = style.outline_color;
            ([c[0], c[1], c[2], c[3], 0.0, 0.0, 0.0, 0.0], 4)
        }
        TransitionProperty::Color => ([0.0; 8], 0), // text color is on TextStyle, not Style
        TransitionProperty::OutlineWidth => {
            ([style.outline_width, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1)
        }
        TransitionProperty::OutlineOffset => {
            ([style.outline_offset, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1)
        }
        TransitionProperty::BoxShadow => {
            let c = style.shadow_color;
            ([c[0], c[1], c[2], c[3], 0.0, 0.0, 0.0, 0.0], 4)
        }
        TransitionProperty::MaxHeight => {
            let h = crate::layout::dim_length_px(style.max_size.height).unwrap_or(0.0);
            ([h, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1)
        }
        // Independent transform props: translate → rotate → scale（与 resolve 顺序一致）。
        TransitionProperty::Transform | TransitionProperty::Translate => (
            [
                style.translate[0],
                style.translate[1],
                style.scale[0],
                style.scale[1],
                style.rotate_deg,
                0.0,
                0.0,
                0.0,
            ],
            if matches!(prop, TransitionProperty::Translate) {
                2
            } else {
                5
            },
        ),
        // Handled via [`FilterTween`] + [`interpolate_filter_lists`].
        TransitionProperty::Filter => ([0.0; 8], 0),
        TransitionProperty::All | TransitionProperty::Unsupported => ([0.0; 8], 0),
    }
}

fn write_prop(style: &mut super::Style, prop: TransitionProperty, val: &[f32; 8]) {
    match prop {
        TransitionProperty::Opacity => style.opacity = val[0],
        TransitionProperty::BackgroundColor => style.bg_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::BorderColor => style.border_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::OutlineColor => style.outline_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::Color => {} // text color is on TextStyle
        TransitionProperty::OutlineWidth => style.outline_width = val[0],
        TransitionProperty::OutlineOffset => style.outline_offset = val[0],
        TransitionProperty::BoxShadow => style.shadow_color = [val[0], val[1], val[2], val[3]],
        TransitionProperty::MaxHeight => {
            style.max_size.height = taffy::style::Dimension::length(val[0].max(0.0));
        }
        TransitionProperty::Transform => {
            style.translate = [val[0], val[1]];
            style.scale = [val[2], val[3]];
            style.rotate_deg = val[4];
        }
        TransitionProperty::Translate => {
            style.translate = [val[0], val[1]];
        }
        TransitionProperty::Filter => {}
        TransitionProperty::All | TransitionProperty::Unsupported => {}
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

/// All concrete properties that `All` expands into.
const ALL_PROPS: &[TransitionProperty] = &[
    TransitionProperty::Opacity,
    TransitionProperty::BackgroundColor,
    TransitionProperty::BorderColor,
    TransitionProperty::OutlineColor,
    TransitionProperty::OutlineWidth,
    TransitionProperty::OutlineOffset,
    TransitionProperty::BoxShadow,
    TransitionProperty::Color,
    // Transform 已含 translate；勿再单独扩 Translate，避免同帧双写。
    TransitionProperty::Transform,
    TransitionProperty::MaxHeight,
    TransitionProperty::Filter,
];

/// 无 class 变体的节点也要能推进 CSS transition（如 Carousel 轨道）。
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

fn tween_elapsed_ms(tween: &PropertyTween, now_ms: f64) -> f32 {
    (now_ms - tween.start_ms).max(0.0) as f32
}

fn tween_sample(tween: &PropertyTween, now_ms: f64) -> ([f32; 8], f32) {
    let elapsed_ms = tween_elapsed_ms(tween, now_ms);
    let eff_elapsed = (elapsed_ms - tween.delay_ms).max(0.0);
    let linear_t = (eff_elapsed / tween.duration_ms.max(0.001)).min(1.0);
    let eased = tween.timing.eval(linear_t);
    (
        lerp_arr(&tween.from, &tween.to, eased, tween.components),
        linear_t,
    )
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

/// 把当前 style 值记为已 settle 的 transition 基线（`from=to=current`，inactive）。
/// 用于 Carousel 无缝瞬移等：清 snapshot 后若直接写入新目标，首帧 seed 会 `from=to=新目标` 而跳过动画。
pub fn snap_element_transitions_to_current(
    style: &super::Style,
    snapshot: &mut TransitionSnapshot,
) {
    if style.transition.is_empty() {
        snapshot.tweens.clear();
        snapshot.filter_tween = None;
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
            },
        );
    }
}

/// Tick CSS transitions for a single element. Returns `true` if any tween is still active.
///
/// Call this **after** class-variant resolution has written the target style.
/// `snapshot` is mutable storage that persists across frames on the element.
///
/// Progress is sampled from the **monotonic CSS timeline** (`Instant`, or a virtual
/// clock in tests). `dt_secs` is ignored for progress (kept for call-site compatibility);
/// dense CursorMoved redraws therefore cannot accelerate `transition-*`.
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
    _dt_secs: f32,
) -> bool {
    if style.transition.is_empty() {
        if !snapshot.tweens.is_empty() {
            snapshot.tweens.clear();
        }
        snapshot.filter_tween = None;
        return false;
    }

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
        let current = style.filter.clone();
        let tween = snapshot.filter_tween.get_or_insert_with(|| FilterTween {
            from: current.clone(),
            to: current.clone(),
            start_ms: now_ms,
            duration_ms: cfg.duration_ms,
            delay_ms: cfg.delay_ms,
            timing: cfg.timing,
            active: false,
            written: None,
        });

        let target = specified_filter_value(tween, &current);
        let target_changed = tween.to != target;
        if target_changed {
            if tween.active {
                let (mid, _) = filter_tween_sample(tween, now_ms);
                tween.from = mid;
            }
            tween.to = target;
            tween.duration_ms = cfg.duration_ms;
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
    } else {
        snapshot.filter_tween = None;
    }

    for (&prop, &cfg_idx) in &config_map {
        if prop == TransitionProperty::Filter {
            continue;
        }
        let cfg = &transition_entries[cfg_idx];
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
                    duration_ms: cfg.duration_ms,
                    delay_ms: cfg.delay_ms,
                    timing: cfg.timing,
                    active: false,
                    written: Some(current),
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
                duration_ms: cfg.duration_ms,
                delay_ms: cfg.delay_ms,
                timing: cfg.timing,
                active: false,
                written: None,
            });

        let target = specified_prop_value(tween, current, comps);
        // Target changed → retarget from the **current computed value** (CSS transition).
        let target_changed = (0..comps as usize).any(|i| (tween.to[i] - target[i]).abs() > 1e-5);
        if target_changed {
            tween.components = comps;
            if tween.active {
                let (mid, _) = tween_sample(tween, now_ms);
                tween.from = mid;
            }
            // Inactive: `from` already equals last settled `to`.
            tween.to = target;
            tween.duration_ms = cfg.duration_ms;
            tween.delay_ms = cfg.delay_ms;
            tween.timing = cfg.timing;
            tween.active = true;
            tween.start_ms = now_ms;
        }

        if !tween.active {
            continue;
        }

        let (interpolated, linear_t) = tween_sample(tween, now_ms);
        let written = if linear_t >= 1.0 - 1e-6 {
            tween.active = false;
            tween.from = tween.to;
            tween.to
        } else {
            any_active = true;
            interpolated
        };
        write_prop(style, prop, &written);
        tween.written = Some(written);
    }

    snapshot.tweens.retain(|p, _| config_map.contains_key(p));

    any_active
}

#[cfg(test)]
mod css_transition_dt_tests {
    use super::*;
    use crate::style::element_style::{Style, TransitionEntry, TransitionProperty};
    use crate::style::TimingFunction;

    #[test]
    fn monotonic_timeline_ignores_dt_storm() {
        set_css_transition_virtual_timeline(false);
        let mut style = Style::default();
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
    fn virtual_timeline_advances_only_when_seeked() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = Style::default();
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
        let mut style = Style::default();
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
        let mut style = Style::default();
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
    fn snap_baseline_then_new_target_still_animates() {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut style = Style::default();
        style.translate = [-100.0, 0.0];
        style.scale = [1.0, 1.0];
        style.transition.push(TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        });
        let mut snap = TransitionSnapshot::default();
        // 模拟 Carousel 无缝瞬移：钉在 -100 并 seed，再写新目标 -300。
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
        // Chrome sync 风格：每帧再 set 同一目标
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
        // 首屏连帧 / 重复 RedrawRequested 常见 raw dt≈0
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
        // Wait 数秒后首帧：未夹紧会一次走完 150ms
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
    fn animation_run_active_false_while_css_animation_needs_frame_still_true() {
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
            css_animation_needs_frame(&style, &snap),
            "list still non-empty → css_animation_needs_frame stays true"
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
}
