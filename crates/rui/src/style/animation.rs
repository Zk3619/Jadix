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
        if !sig.try_set(val) {
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
        let _ = sig.try_set(val);
    }

    AnimationTick {
        any: spring_any_running || cubic_running,
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
    })
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
        assert!(elapsed_mid > 0.04, "expected ~50ms progress, got {elapsed_mid}");
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
