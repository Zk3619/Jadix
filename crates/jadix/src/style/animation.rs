pub use jadix_animation::animation::*;
use std::cell::RefCell;
thread_local! {
    /// Set during chrome when `@keyframes` / `animate-spin` / shimmer still need vsync.
    /// `any_ui_animation_active` is cubic tweens only; without this, `about_to_wait`
    /// `WaitUntil` (scrollbar hide after a wheel) sleeps and every CSS clock stops.
    static PAGE_CSS_NEEDS_FRAME: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn begin_page_css_frame() {
    PAGE_CSS_NEEDS_FRAME.with(|c| c.set(false));
}

pub fn note_page_css_needs_frame() {
    PAGE_CSS_NEEDS_FRAME.with(|c| c.set(cfg!(feature = "animation-base")));
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
// per tick through the shared clock policy so ease-out cannot skip mids.

use super::clip_path::ClipPath;
use super::element_style::{
    AnimationComposition, AnimationDirection, AnimationEntry, AnimationFillMode,
    AnimationIterationCount, AnimationPlayState, AnimationTimeline, BoxShadow, TimingFunction,
    TransitionEntry, TransitionProperty, UnitSlot,
};
use super::filter::{interpolate_filter_lists, FilterList};
use super::generator::{Spring, SpringPhysics};
use std::cell::Cell;
use std::collections::HashMap;
#[cfg(test)]
use std::time::Instant;

pub(crate) use jadix_animation::clock::css_timeline_now_ms;
use jadix_animation::clock::css_transition_max_step_ms;
pub use jadix_animation::clock::{
    advance_css_transition_timeline_ms, prefers_reduced_motion, set_css_transition_max_step_ms,
    set_css_transition_synthetic_clock, set_css_transition_timeline_ms,
    set_css_transition_virtual_timeline, set_prefers_reduced_motion,
};

thread_local! {
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
    pub(crate) fn cascade_assigned(&mut self) {
        for tween in self.tweens.values_mut() {
            tween.written = None;
        }
        if let Some(tween) = &mut self.filter_tween {
            tween.written = None;
        }
        if let Some(tween) = &mut self.clip_tween {
            tween.written = None;
        }
        if let Some(tween) = &mut self.shadow_tween {
            tween.written = None;
        }
    }
    /// True while at least one property tween is still interpolating.
    pub fn has_active_tweens(&self) -> bool {
        self.tweens.values().any(|t| t.active)
            || self.filter_tween.as_ref().is_some_and(|t| t.active)
            || self.clip_tween.as_ref().is_some_and(|t| t.active)
            || self.shadow_tween.as_ref().is_some_and(|t| t.active)
    }

    /// True if this node has ever seeded a transition snapshot (active or settled).
    pub fn has_any_tweens(&self) -> bool {
        !self.tweens.is_empty()
            || self.filter_tween.is_some()
            || self.clip_tween.is_some()
            || self.shadow_tween.is_some()
    }

    pub fn prop_tween_active(&self, prop: TransitionProperty) -> bool {
        self.tweens.get(&prop).is_some_and(|t| t.active)
    }

    /// Peek the hide-settle latch (`push_frame_paint_element` hide dest-reset).
    pub fn settled_this_tick(&self) -> bool {
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
    defer_first_tick: bool,
}

impl AnimationSnapshot {
    pub(crate) fn start_on_next_sample(&mut self) {
        self.defer_first_tick = true;
    }
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
    super::keyframes::keyframe_revision(&entry.name).hash(&mut h);
    format!("{:?}", entry.timing).hash(&mut h);
    format!("{:?}", entry.composition).hash(&mut h);
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
                "[jadix] animation `{name}` has no @keyframes; the element will stay at its \
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
/// 80 ms and stamping `last_tick_ms` to *now* drops the
/// rest — spin crawls for the whole slide. Transitions keep that cap in
/// [`duration_tween_elapsed_ms`]. Keyframes apply the full gap. `frame` is only
/// the fallback when the timeline has not moved (unit tests).
fn animation_tick_dt_ms(snap: &mut AnimationSnapshot, dt_secs: f32, reduced: bool) -> f32 {
    if reduced {
        return 1.0e9;
    }
    let now_ms = css_timeline_now_ms();
    if std::mem::take(&mut snap.defer_first_tick) {
        snap.last_tick_ms = Some(now_ms);
        return 0.0;
    }
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

        if !cfg!(feature = "animation-base") {
            let local = match entry.iteration_count {
                AnimationIterationCount::Infinite => direction_local_t(entry.direction, 0, 1.0),
                AnimationIterationCount::Finite(_) => {
                    entry_timing_model(entry).resolve(f32::MAX).local_t
                }
            };
            apply_animation_sample(
                style,
                text,
                &stops,
                local,
                TimingFunction::Linear,
                &kf_base,
                &mut lock,
                entry.composition,
            );
            snap.runs[i].finished = true;
            observe_playback(playback_id, &snap.runs[i]);
            continue;
        }

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
    if !cfg!(feature = "animation-base") || style.animations.is_empty() {
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
    if !cfg!(feature = "animation-base") {
        return false;
    }
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
    if !cfg!(feature = "animation-base") {
        return false;
    }
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
    let Some(max_step) = css_transition_max_step_ms() else {
        tween.last_ms = now_ms;
        return raw as f32;
    };
    let progressed = (tween.last_ms - tween.start_ms).max(0.0);
    let since_last = (now_ms - tween.last_ms).max(0.0);
    let step = if since_last > 80.0 {
        since_last.min(max_step as f64)
    } else {
        since_last
    };
    let elapsed = (progressed + step).min(raw);
    tween.last_ms = tween.start_ms + elapsed;
    elapsed as f32
}

fn tween_sample_for_property(
    tween: &mut PropertyTween,
    property: TransitionProperty,
    now_ms: f64,
    dt_secs: f32,
) -> ([f32; 8], f32) {
    let (value, progress) = tween_sample(tween, now_ms, dt_secs);
    let value = if progress < 1.0 && is_color_transition_prop(property) && !tween.timing.is_spring()
    {
        lerp_packed_color(&tween.from, &tween.to, tween.timing.eval(progress))
    } else {
        value
    };
    (value, progress)
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
    for (i, output_component) in out[..tween.components as usize].iter_mut().enumerate() {
        let spring = tween.springs[i]
            .get_or_insert_with(|| Spring::new(tween.from[i], tween.to[i], tween.vel[i], opts));
        let sample = spring.next(elapsed - tween.delay_ms);
        *output_component = sample.value;
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
    for (i, output_component) in out[..n].iter_mut().enumerate() {
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
        *output_component = sample.value;
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
    if !cfg!(feature = "animation-base") {
        *snapshot = TransitionSnapshot::default();
        return false;
    }
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
                let (mid, _) = tween_sample_for_property(tween, prop, now_ms, dt_secs);
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

        let (interpolated, linear_t) = tween_sample_for_property(tween, prop, now_ms, dt_secs);
        let written = if linear_t >= 1.0 - 1e-6 {
            tween.active = false;
            tween.from = tween.to;
            tween.vel = [0.0; 8];
            tween.springs = [const { None }; 8];
            tween.to
        } else {
            any_active = true;
            interpolated
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
#[path = "tests/css_transition_dt.rs"]
mod css_transition_dt_tests;

#[cfg(test)]
#[path = "tests/animation.rs"]
mod tests;
