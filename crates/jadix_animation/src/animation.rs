use crate::generator::{tween_sample as generator_tween_sample, TweenOptions};
use crate::TimingFunction;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use jadix_core::reactivity::{use_signal, Signal};

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
    // A target chosen at the current frame boundary has no share of the
    // preceding frame interval (which may include an idle WaitUntil).
    skip_frame_dt: bool,
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
            skip_frame_dt: false,
        }));
        CUBIC_TWEENS.with(|anim| {
            anim.borrow_mut().push(Rc::downgrade(&state));
        });
        Self(state)
    }

    pub fn set(&self, target: f32) {
        self.retarget(target, false);
    }

    /// Start at this frame boundary without consuming the preceding interval.
    /// Repeating the same target still preserves the running animation clock.
    pub fn set_at_frame_start(&self, target: f32) {
        self.retarget(target, true);
    }

    fn retarget(&self, target: f32, at_frame_start: bool) {
        let mut s = self.0.borrow_mut();
        if !cfg!(feature = "animation-base") {
            s.target = target;
            s.value = target;
            s.from_value = target;
            s.elapsed = 0.0;
            s.active = false;
            s.skip_frame_dt = false;
            let _ = s.signal.try_set_silent(target);
            return;
        }
        // 目标未变时不要重置 `elapsed`：chrome sync 每帧会再 `set` 同一目标，
        // 若此处重启，有效时长会远长于 `duration`（asymptotic crawl）。
        if (s.target - target).abs() < 1e-5 {
            return;
        }
        s.from_value = s.value;
        s.target = target;
        s.elapsed = 0.0;
        s.active = (s.value - target).abs() >= 1e-5;
        s.skip_frame_dt = at_frame_start && s.active;
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
    if !cfg!(feature = "animation-base") {
        return AnimationTick {
            any: false,
            scrollbar_overlay: false,
        };
    }
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
                    if !std::mem::take(&mut s.skip_frame_dt) {
                        s.elapsed += dt;
                    }
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
    if !cfg!(feature = "animation-base") {
        return false;
    }
    CUBIC_TWEENS.with(|anim| {
        anim.borrow()
            .iter()
            .any(|w| w.upgrade().is_some_and(|s| s.borrow().active))
    }) || super::timeline::any_timeline_playing()
        || super::generator::any_value_generator_active()
}
