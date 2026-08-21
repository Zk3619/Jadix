use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::reactivity::{use_signal, Signal};

thread_local! {
    static ANIMATIONS: RefCell<Vec<Weak<RefCell<SpringState>>>> = const { RefCell::new(Vec::new()) };
    static CUBIC_TWEENS: RefCell<Vec<Weak<RefCell<CubicTweenState>>>> = const { RefCell::new(Vec::new()) };
}

pub struct SpringState {
    pub target: f32,
    pub current: f32,
    pub velocity: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub signal: Signal<f32>,
    /// 为真时，未 settle 的弹簧会拉高 [`AnimationTick::scrollbar_overlay`]，用于滚动条 Full 预遍历门控。
    pub affects_scrollbar_prepass: bool,
}

#[derive(Clone)]
pub struct Spring(Rc<RefCell<SpringState>>);

impl Spring {
    /// 默认参数偏「跟手」；UI 短过渡（对标 CSS `transition` ~150ms）见 [`Self::new_ui_transition`].
    pub fn new(initial: f32) -> Self {
        Self::new_with_params(initial, 170.0, 20.0)
    }

    /// 近似 **欠阻尼临界** 附近，避免明显过冲，时长接近浏览器 `transition-all` + `ease` 的常见观感。
    pub fn new_ui_transition(initial: f32) -> Self {
        Self::new_with_params(initial, 280.0, 38.0)
    }

    /// 滚动条 `auto` 模式显示/隐藏弹簧（与默认 [`Self::new`] 相同力学参数，但参与 [`AnimationTick::scrollbar_overlay`]）。
    pub fn new_scrollbar_visibility(initial: f32) -> Self {
        Self::new_with_params_flag(initial, 170.0, 20.0, true)
    }

    pub fn new_with_params(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self::new_with_params_flag(initial, stiffness, damping, false)
    }

    fn new_with_params_flag(
        initial: f32,
        stiffness: f32,
        damping: f32,
        affects_scrollbar_prepass: bool,
    ) -> Self {
        let state = Rc::new(RefCell::new(SpringState {
            target: initial,
            current: initial,
            velocity: 0.0,
            stiffness,
            damping,
            signal: Signal::new_unowned(initial),
            affects_scrollbar_prepass,
        }));

        ANIMATIONS.with(|anim| {
            anim.borrow_mut().push(Rc::downgrade(&state));
        });

        Self(state)
    }

    pub fn set(&self, target: f32) {
        if !target.is_finite() {
            return;
        }
        let mut state = self.0.borrow_mut();
        if (state.target - target).abs() < 0.0001 {
            return;
        }
        state.target = target;
        // Keep target updates from fan-out rebuilding effects synchronously.
        let _ = state.signal.try_set_silent(state.current);
    }

    /// Jump to `value` with zero velocity (no animation). Use for focus rings so a frame that
    /// retargets after idle is not stuck waiting for a redraw that never comes (tick runs
    /// before chrome sync; see button focus-ring sync).
    pub fn snap(&self, value: f32) {
        if !value.is_finite() {
            return;
        }
        let mut state = self.0.borrow_mut();
        state.target = value;
        state.current = value;
        state.velocity = 0.0;
        let _ = state.signal.try_set_silent(value);
    }

    pub fn get(&self) -> f32 {
        self.0.borrow().signal.get()
    }

    pub fn peek(&self) -> f32 {
        self.0.borrow().signal.peek()
    }

    /// 当前弹簧目标（不订阅）。用于判断是否仍在过渡、避免误重启动画。
    pub fn peek_target(&self) -> f32 {
        self.0.borrow().target
    }

    /// 是否仍在向目标积分（含过冲回摆）。
    pub fn is_moving(&self) -> bool {
        let s = self.0.borrow();
        (s.current - s.target).abs() > 0.5 || s.velocity.abs() > 1.0
    }
}

pub fn use_spring(id_str: &str, initial: f32) -> Spring {
    let spring_sig = use_signal(id_str, move || Spring::new(initial));
    // Peek: the stored `Spring` is stable; subscribing the root `create_effect` here would be
    // pointless and risks accidental coupling if the wrapper signal were ever updated.
    spring_sig.peek()
}

/// 与 [`use_spring`] 相同，但使用 [`Spring::new_ui_transition`]（适合按钮 hover 等短过渡）。
pub fn use_ui_transition_spring(id_str: &str, initial: f32) -> Spring {
    let spring_sig = use_signal(id_str, move || Spring::new_ui_transition(initial));
    spring_sig.peek()
}

/// 与 [`use_spring`] 相同存储方式，但弹簧标记为影响滚动条 overlay 预遍历（`scrollbar_spring_*` key）。
pub fn use_scrollbar_visibility_spring(id_str: &str, initial: f32) -> Spring {
    let spring_sig = use_signal(id_str, move || Spring::new_scrollbar_visibility(initial));
    spring_sig.peek()
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

/// [`tick_animations`] 的返回值：`any` 决定是否继续 `request_redraw`；`scrollbar_overlay` 决定是否对滚动条做 Full 预遍历。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationTick {
    pub any: bool,
    /// 有未 settle 且标记为 `affects_scrollbar_prepass` 的弹簧（滚动条 auto 淡入淡出等）。
    pub scrollbar_overlay: bool,
}

pub fn tick_animations(dt: f32) -> AnimationTick {
    // 与滚动同口径：首屏连帧 / Wait 后首帧的 raw `dt` 常为 ≈0 或数秒。
    // 不夹紧时：≈0 → 动画看似失效；过大 → 一帧瞬跳到目标。
    const DT_MIN: f32 = 1.0 / 240.0;
    const DT_MAX: f32 = 0.08;
    let dt = dt.clamp(DT_MIN, DT_MAX);
    const SPRING_SETTLE_POS_EPS: f32 = 0.01;
    const SPRING_SETTLE_VEL_EPS: f32 = 0.01;
    let mut spring_any_running = false;
    let mut scrollbar_overlay = false;
    let mut signals_to_update = Vec::new();

    ANIMATIONS.with(|anim| {
        let mut list = anim.borrow_mut();
        let mut i = 0;
        while i < list.len() {
            if let Some(spring) = list[i].upgrade() {
                let mut s = spring.borrow_mut();
                if !s.current.is_finite() || !s.target.is_finite() || !s.velocity.is_finite() {
                    // Corrupted spring state (NaN/Inf) must not keep the redraw pump alive.
                    s.target = 0.0;
                    s.current = 0.0;
                    s.velocity = 0.0;
                    let _ = s.signal.try_set_silent(0.0);
                    i += 1;
                    continue;
                }

                // Already at rest: skip integrate + signal write (avoids idle try_set noise).
                if (s.current - s.target).abs() < SPRING_SETTLE_POS_EPS
                    && s.velocity.abs() < SPRING_SETTLE_VEL_EPS
                {
                    s.current = s.target;
                    s.velocity = 0.0;
                    i += 1;
                    continue;
                }

                let displacement = s.current - s.target;
                let spring_force = -s.stiffness * displacement;
                let damping_force = -s.damping * s.velocity;
                let acceleration = spring_force + damping_force;

                s.velocity += acceleration * dt;
                s.current += s.velocity * dt;

                // 用积分后的状态判断 settle，避免旧位移 + 新速度混用导致长期微振、scrollbar_overlay 误报。
                let disp_after = s.current - s.target;
                if disp_after.abs() < SPRING_SETTLE_POS_EPS
                    && s.velocity.abs() < SPRING_SETTLE_VEL_EPS
                {
                    s.current = s.target;
                    s.velocity = 0.0;
                } else {
                    spring_any_running = true;
                    // 尾段几乎不可见时仍 tick/redraw，但不再强迫滚动条 Full 预遍历 → 更多帧可走 scroll_only、跳过 taffy compute。
                    const SCROLLBAR_OVERLAY_POS: f32 = 0.022;
                    const SCROLLBAR_OVERLAY_VEL: f32 = 0.028;
                    if s.affects_scrollbar_prepass
                        && (disp_after.abs() > SCROLLBAR_OVERLAY_POS
                            || s.velocity.abs() > SCROLLBAR_OVERLAY_VEL)
                    {
                        scrollbar_overlay = true;
                    }
                }

                // Only write while (or just as) the spring is moving / snapping to target.
                signals_to_update.push((s.signal, s.current));
                i += 1;
            } else {
                list.remove(i);
            }
        }
    });

    for (sig, val) in signals_to_update {
        // Silent: UI chrome reads springs via `peek` + per-frame sync. Notifying here
        // flushed effects / risked root rebuild and wiped live Inspector style edits
        // whenever a Button hover spring settled (e.g. leaving Set/Apply).
        if !sig.try_set_silent(val) {
            // Creating effect disposed a legacy owned signal; drop stale Weak next pass.
            continue;
        }
    }

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
        let _ = sig.try_set_silent(val);
    }

    let timeline_any = super::timeline::tick_timelines(dt);

    AnimationTick {
        any: spring_any_running || cubic_running || timeline_any,
        scrollbar_overlay,
    }
}

/// 是否仍有未 settle 的 spring / cubic（不推进时间）。
/// chrome sync（Switch 等）常在本帧 `tick_animations` **之后**才 `set` 目标；
/// render 结束后用此查询补 `request_redraw`，避免动画停在第 0 帧。
pub fn any_ui_animation_active() -> bool {
    const SPRING_SETTLE_POS_EPS: f32 = 0.01;
    const SPRING_SETTLE_VEL_EPS: f32 = 0.01;
    let spring_busy = ANIMATIONS.with(|anim| {
        anim.borrow().iter().any(|w| {
            w.upgrade().is_some_and(|s| {
                let s = s.borrow();
                if !s.current.is_finite() || !s.target.is_finite() || !s.velocity.is_finite() {
                    return false;
                }
                (s.current - s.target).abs() >= SPRING_SETTLE_POS_EPS
                    || s.velocity.abs() >= SPRING_SETTLE_VEL_EPS
            })
        })
    });
    if spring_busy {
        return true;
    }
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

use super::element_style::{TimingFunction, TransitionEntry, TransitionProperty};
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
        CSS_TIMELINE_VIRTUAL_MS.set(Some(
            CSS_TIMELINE_VIRTUAL_MS.get().unwrap_or(0.0).max(0.0),
        ));
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
    from: [f32; 4],
    to: [f32; 4],
    /// Number of meaningful components (1 for scalar, 4 for color).
    components: u8,
    /// Monotonic timeline time (ms) when this segment started.
    start_ms: f64,
    duration_ms: f32,
    delay_ms: f32,
    timing: TimingFunction,
    active: bool,
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

fn extract_prop(style: &super::Style, prop: TransitionProperty) -> ([f32; 4], u8) {
    match prop {
        TransitionProperty::Opacity => ([style.opacity, 0.0, 0.0, 0.0], 1),
        TransitionProperty::BackgroundColor => (style.bg_color, 4),
        TransitionProperty::BorderColor => (style.border_color, 4),
        TransitionProperty::OutlineColor => (style.outline_color, 4),
        TransitionProperty::Color => ([0.0; 4], 0), // text color is on TextStyle, not Style
        TransitionProperty::OutlineWidth => ([style.outline_width, 0.0, 0.0, 0.0], 1),
        TransitionProperty::OutlineOffset => ([style.outline_offset, 0.0, 0.0, 0.0], 1),
        TransitionProperty::BoxShadow => {
            ([style.shadow_color[0], style.shadow_color[1], style.shadow_color[2], style.shadow_color[3]], 4)
        }
        TransitionProperty::Transform => {
            ([style.scale[0], style.scale[1], style.rotate_deg, 0.0], 3)
        }
        // Handled via [`FilterTween`] + [`interpolate_filter_lists`].
        TransitionProperty::Filter => ([0.0; 4], 0),
        TransitionProperty::All => ([0.0; 4], 0),
    }
}

fn write_prop(style: &mut super::Style, prop: TransitionProperty, val: &[f32; 4]) {
    match prop {
        TransitionProperty::Opacity => style.opacity = val[0],
        TransitionProperty::BackgroundColor => style.bg_color = *val,
        TransitionProperty::BorderColor => style.border_color = *val,
        TransitionProperty::OutlineColor => style.outline_color = *val,
        TransitionProperty::Color => {} // text color is on TextStyle
        TransitionProperty::OutlineWidth => style.outline_width = val[0],
        TransitionProperty::OutlineOffset => style.outline_offset = val[0],
        TransitionProperty::BoxShadow => style.shadow_color = *val,
        TransitionProperty::Transform => {
            style.scale = [val[0], val[1]];
            style.rotate_deg = val[2];
        }
        TransitionProperty::Filter => {}
        TransitionProperty::All => {}
    }
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_arr(a: &[f32; 4], b: &[f32; 4], t: f32, n: u8) -> [f32; 4] {
    let mut out = [0.0; 4];
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
    TransitionProperty::Transform,
    TransitionProperty::Filter,
];

fn tween_elapsed_ms(tween: &PropertyTween, now_ms: f64) -> f32 {
    (now_ms - tween.start_ms).max(0.0) as f32
}

fn tween_sample(tween: &PropertyTween, now_ms: f64) -> ([f32; 4], f32) {
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
        let target = style.filter.clone();
        let tween = snapshot.filter_tween.get_or_insert_with(|| FilterTween {
            from: target.clone(),
            to: target.clone(),
            start_ms: now_ms,
            duration_ms: cfg.duration_ms,
            delay_ms: cfg.delay_ms,
            timing: cfg.timing,
            active: false,
        });

        let target_changed = tween.to != target;
        if target_changed {
            if tween.active {
                let (current, _) = filter_tween_sample(tween, now_ms);
                tween.from = current;
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
            style.filter = interpolated;
            if linear_t >= 1.0 - 1e-6 {
                tween.active = false;
                tween.from = tween.to.clone();
                style.filter = tween.to.clone();
            } else {
                any_active = true;
            }
        }
    } else {
        snapshot.filter_tween = None;
    }

    for (&prop, &cfg_idx) in &config_map {
        if prop == TransitionProperty::Filter {
            continue;
        }
        let cfg = &transition_entries[cfg_idx];
        let (target, comps) = extract_prop(style, prop);
        if comps == 0 {
            continue;
        }

        let tween = snapshot.tweens.entry(prop).or_insert_with(|| PropertyTween {
            from: target,
            to: target,
            components: comps,
            start_ms: now_ms,
            duration_ms: cfg.duration_ms,
            delay_ms: cfg.delay_ms,
            timing: cfg.timing,
            active: false,
        });

        // Target changed → retarget from the **current computed value** (CSS transition).
        let target_changed = (0..comps as usize).any(|i| (tween.to[i] - target[i]).abs() > 1e-5);
        if target_changed {
            tween.components = comps;
            if tween.active {
                let (current, _) = tween_sample(tween, now_ms);
                tween.from = current;
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
        write_prop(style, prop, &interpolated);

        if linear_t >= 1.0 - 1e-6 {
            tween.active = false;
            tween.from = tween.to;
            write_prop(style, prop, &tween.to);
        } else {
            any_active = true;
        }
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
        assert!(!snap.has_active_tweens(), "virtual 150ms+ tween should settle");
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
        assert!((mid - 0.75).abs() < 0.02, "expected ~0.75 at 100ms, got {mid}");
        // Leave: retarget to 0.5 from current (~0.75), not from 1.0.
        style.opacity = 0.5;
        assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
        assert!((snap.tweens.get(&TransitionProperty::Opacity).unwrap().from[0] - mid).abs() < 0.02);
        set_css_transition_timeline_ms(100.0); // same now — just retargeted, elapsed≈0
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
        assert!(
            (style.opacity - mid).abs() < 0.05,
            "right after leave retarget, value should stay near mid ({mid}), got {}",
            style.opacity
        );
        set_css_transition_virtual_timeline(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_effect, dispose_effect};

    #[test]
    fn named_spring_survives_creating_effect_dispose() {
        let effect = create_effect(|| {
            let _ = use_spring("test-spring-unowned", 0.0);
        });
        let spring = use_spring("test-spring-unowned", 0.0);
        spring.set(1.0);
        dispose_effect(effect);
        // Tick must not panic after the creating effect's owner is gone.
        let _ = tick_animations(1.0 / 60.0);
        assert!(spring.0.borrow().signal.is_alive());
        let _ = spring.peek();
    }

    #[test]
    fn scrollbar_fade_in_matches_exit_after_idle_wake() {
        // 退场：deadline 帧只改 target，下一帧 16ms tick 才积分。
        // 入场若在「空闲大 dt」那一拍已经 set(1)，会被积成瞬跳。
        let spring = Spring::new_scrollbar_visibility(0.0);
        let _ = tick_animations(2.0);
        assert!((spring.peek()).abs() < 0.001);
        spring.set(1.0);
        let _ = tick_animations(1.0 / 60.0);
        let v = spring.peek();
        assert!(v > 0.01, "fade-in must start (got {v})");
        assert!(v < 0.45, "one vsync tick must not pop to opaque (got {v})");
    }

    #[test]
    fn snap_jumps_without_leaving_spring_running() {
        let spring = Spring::new_ui_transition(0.0);
        spring.set(1.0);
        assert!((spring.peek() - 0.0).abs() < 0.001);
        spring.snap(1.0);
        assert!((spring.peek() - 1.0).abs() < 0.001);
        assert!((spring.0.borrow().target - 1.0).abs() < 0.001);
        assert!(spring.0.borrow().velocity.abs() < 0.001);
        let tick = tick_animations(1.0 / 60.0);
        assert!(!tick.any, "snapped spring must not keep requesting redraw");
    }

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
    fn spring_huge_dt_does_not_settle_in_one_tick() {
        let spring = Spring::new_ui_transition(0.0);
        spring.set(1.0);
        let _ = tick_animations(2.0);
        let cur = spring.peek();
        assert!(
            (cur - 1.0).abs() > 0.05,
            "hover spring must not teleport on huge dt (got {cur})"
        );
        assert!(any_ui_animation_active());
    }
}
