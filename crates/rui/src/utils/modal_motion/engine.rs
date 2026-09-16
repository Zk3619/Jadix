//! Dialog / Sheet / Drawer 共享模态动效：presence（退场保留挂载）+ 表面动画。
//!
//! **Dialog** 对齐 shadcn [`dialog.tsx`](https://ui.shadcn.com/docs/components/dialog)：
//! overlay `fade-in-0` / `fade-out-0`，面板 `fade-in-0 zoom-in-90` /
//! `fade-out-0 zoom-out-90` (fade symmetry), use CSS `@keyframes` ([`style::keyframes`]),
//! 不经 transition flush/tick 链路。遮罩 `backdrop-blur` 保持恒定目标半径，
//! 入场/退场的模糊渐显由渲染器按 overlay opacity 乘算（见 `AppState` backdrop pass）。
//!
//! Sheet / Drawer：**入场/退场** 面板都走 `@keyframes` + CSS `translate: 100%`
//! （`rui-modal-slide-in-*` / `rui-modal-slide-out-*`），遮罩 `fade-in-0` / `fade-out-0`。
//! 退场若再写 closed 像素 `translate` / opacity=0 当 specified，会一帧卸掉壳。
//! [`flush_modal_shell_motion_subtree`] 仍负责入场 seed 与退场 keyframes。
//!
//! 种类与视口在 [`crate::utils::modal_surface`]；runtime tick/flush 走
//! [`crate::utils::modal_motion`]。本文件是 crate-private 引擎，不升作者面。

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::portal_overlay::modal_stack_metrics;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::style::{
    animation_run_active, css_transition_needs_frame, keyframes_anim_entry, AnimationFillMode,
    TimingFunction, TransitionEntry, TransitionProperty,
};
use crate::utils::motion::{
    clear_motion, motion_sync_status, prepare_motion_sync, MotionBootstrap, MotionEpsilon,
    MotionStyle, MotionTransform,
};
use crate::utils::presence::{ExitGate, PresencePhase};

pub(crate) const MODAL_ENTER_MS: f32 = 250.0;
pub(crate) const MODAL_EXIT_MS: f32 = 200.0;
/// shadcn Drawer/Sheet popup: `duration-450 ease-[cubic-bezier(0.22,1,0.36,1)]`.
pub(crate) const SLIDE_ENTER_MS: f32 = 450.0;
/// shadcn ending-style ≈ `calc(swipe-strength * 400ms)` at full strength.
pub(crate) const SLIDE_EXIT_MS: f32 = 400.0;
/// shadcn Dialog `duration-200`.
const DIALOG_ENTER_ANIM_MS: f32 = 200.0;
const DIALOG_EXIT_ANIM_MS: f32 = MODAL_EXIT_MS;
/// Cover Sheet/Drawer slide exit (Dialog uses CSS completion, not this wall clock).
const EXIT_DURATION: Duration = Duration::from_millis(SLIDE_EXIT_MS as u64 + 80);
/// shadcn `zoom-in-90` — a bit more travel than `zoom-in-95`.
const DIALOG_ENTER_SCALE: f32 = 0.90;
/// CSS `ease` — Sheet's drawer curve (0.22,1,0.36,1) finishes a 10% zoom in the
/// first tens of ms, so the enter looked like it skipped the middle.
const DIALOG_ENTER_EASE: TimingFunction = TimingFunction::Ease;
/// shadcn drawer popup easing.
const SLIDE_ENTER_EASE: TimingFunction = TimingFunction::CubicBezier(0.22, 1.0, 0.36, 1.0);
/// shadcn drawer overlay / ending easing.
const SLIDE_EXIT_EASE: TimingFunction = TimingFunction::CubicBezier(0.32, 0.72, 0.0, 1.0);

fn dialog_zoom_tid(modal_id: &str) -> String {
    format!("{modal_id}__modal_zoom")
}

/// Author kind lives in [`crate::utils::modal_surface`]; engine stays here.
pub use crate::utils::modal_surface::ModalSurfaceMotion;

impl ModalSurfaceMotion {
    fn closed_translate(self, slide_axis_px: f32) -> [f32; 2] {
        let d = slide_axis_px.max(1.0);
        match self {
            Self::DialogFade => [0.0, 0.0],
            Self::SlideFromTop => [0.0, -d],
            Self::SlideFromBottom => [0.0, d],
            Self::SlideFromLeft => [-d, 0.0],
            Self::SlideFromRight => [d, 0.0],
        }
    }

    /// CSS `translate: ±100%` of the panel box (shadcn Sheet/Drawer exit).
    fn closed_translate_pct(self) -> [f32; 2] {
        match self {
            Self::DialogFade => [0.0, 0.0],
            Self::SlideFromTop => [0.0, -1.0],
            Self::SlideFromBottom => [0.0, 1.0],
            Self::SlideFromLeft => [-1.0, 0.0],
            Self::SlideFromRight => [1.0, 0.0],
        }
    }

    fn animates_panel_transform(self) -> bool {
        true
    }

    /// Dialog 退场只淡遮罩，面板保持 scale=1（避免「缩小幽灵窗」+ 关闭钮渲染异常）。
    fn animates_panel_transform_on_exit(self) -> bool {
        !matches!(self, Self::DialogFade)
    }
}

/// Prefer the panel's own laid-out size (shadcn `translate: 100%`) so enter/exit
/// tuck into the docking edge instead of travelling a full viewport.
fn slide_axis_for_panel(motion: ModalSurfaceMotion, panel: &Element, fallback: f32) -> f32 {
    let along = match motion {
        ModalSurfaceMotion::SlideFromTop | ModalSurfaceMotion::SlideFromBottom => {
            panel.layout_bounds[3]
        }
        ModalSurfaceMotion::SlideFromLeft | ModalSurfaceMotion::SlideFromRight => {
            panel.layout_bounds[2]
        }
        ModalSurfaceMotion::DialogFade => fallback,
    };
    if along > 8.0 {
        along
    } else {
        fallback.max(1.0)
    }
}

/// Pre-layout fallback: Sheet/Drawer pass viewport; `0` collapses travel to 1px
/// (instant open, overlay fades while the panel sits until ExitGate).
#[allow(dead_code)]
pub(crate) fn default_slide_axis_px(motion: ModalSurfaceMotion) -> f32 {
    let (vw, vh) = crate::utils::modal_surface::modal_viewport_size();
    match motion {
        ModalSurfaceMotion::DialogFade => 0.0,
        ModalSurfaceMotion::SlideFromTop | ModalSurfaceMotion::SlideFromBottom => vh,
        ModalSurfaceMotion::SlideFromLeft | ModalSurfaceMotion::SlideFromRight => vw,
    }
}

#[derive(Clone)]
struct ModalShellState {
    modal_id: String,
    motion: ModalSurfaceMotion,
    phase: PresencePhase,
    extra_translate: [f32; 2],
    dragging: bool,
    slide_axis_px: f32,
    /// Live open fraction while dragging (Y-axis height drag keeps extra_translate at 0).
    live_open_frac: f32,
}

struct ModalPresenceCell {
    mounted: Signal<bool>,
    gate: ExitGate,
    overlay_motion: MotionStyle,
    panel_motion: MotionStyle,
    needs_enter: bool,
    /// 入场 CSS 过渡进行中（防同帧二次 reactive 把 opacity 打满）。
    enter_active: bool,
    /// Wall clock when Dialog enter was seeded. Recreated overlay/zoom nodes
    /// (first-open layout churn) must seek to this, not restart fill-mode 0%.
    enter_started: Option<Instant>,
    exit_armed: bool,
    /// The motion registered by the latest shell build (the only source of flush after hoist).
    shell: Option<ModalShellState>,
    /// Logical `open`; used to arm exit after the viewport-modal island is disposed.
    open: Option<Signal<bool>>,
    /// Dialog CSS exit: first frame seed `fade-out-0` / `zoom-out-95`.
    needs_exit_anim: bool,
    /// Drawer 松手后：从当前拖拽位移 CSS transition 吸附到 snap（对齐 `data-swiping:duration-0` → 恢复 duration）。
    slide_settle: bool,
    /// Optional mid translate to seed before settle (drag release).
    slide_settle_from: Option<[f32; 2]>,
}

/// 单实例 presence + motion 状态（[`use_modal_presence`] 取得）。
#[derive(Clone)]
pub(crate) struct ModalPresenceHandle {
    cell: Rc<RefCell<ModalPresenceCell>>,
}

impl ModalPresenceHandle {
    /// Enter seed / CSS settle / exit — skip page-scale while the shell is in motion.
    pub(crate) fn motion_busy(&self, phase: PresencePhase) -> bool {
        let c = self.cell.borrow();
        c.needs_enter || c.enter_active || phase == PresencePhase::Exiting
    }

    /// [`motion_busy`] using the gate's current phase (paint-sync / focus).
    pub(crate) fn is_motion_busy(&self) -> bool {
        self.motion_busy(self.phase())
    }

    /// Gate phase. Source of truth for flush/tick — `shell.phase` is only a cache.
    pub(crate) fn phase(&self) -> PresencePhase {
        self.cell.borrow().gate.phase()
    }
}

thread_local! {
    static PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<ModalPresenceCell>>>> =
        const { RefCell::new(Vec::new()) };
    /// `mounted.set` / portal stack pop deferred until AppState borrow is released.
    static PENDING_MODAL_UNMOUNTS: RefCell<Vec<(Signal<bool>, String)>> =
        const { RefCell::new(Vec::new()) };
    /// Set by [`tick_hoisted_modal_css`]. The redraw pump must see CSS keyframes
    /// (`fade-in-0` / `zoom-in-90`) — `any_ui_animation_active` only covers cubic tweens.
    static MODAL_CSS_NEEDS_FRAME: Cell<bool> = const { Cell::new(false) };
    /// One-shot: `about_to_wait` must `Poll` instead of caret/long-press `WaitUntil`.
    static MODAL_FORCE_IMMEDIATE_WAKE: Cell<bool> = const { Cell::new(false) };
    /// Drawer snap-commit after CSS settle. Engine must not import `components::drawer`.
    static SLIDE_SETTLE_COMPLETE: Cell<Option<fn(&str)>> = const { Cell::new(None) };
}

pub(crate) fn register_slide_settle_complete(f: fn(&str)) {
    SLIDE_SETTLE_COMPLETE.with(|c| c.set(Some(f)));
}

fn notify_slide_settle_complete(id: &str) {
    SLIDE_SETTLE_COMPLETE.with(|c| {
        if let Some(f) = c.get() {
            f(id);
        }
    });
}

/// True while hoisted Dialog/Sheet/Drawer CSS still has frames to paint.
pub(crate) fn any_modal_css_needs_frame() -> bool {
    MODAL_CSS_NEEDS_FRAME.with(|c| c.get())
}

/// Consume the one-shot "do not sleep on WaitUntil" armed by [`sync_modal_presence`].
pub(crate) fn take_modal_force_immediate_wake() -> bool {
    MODAL_FORCE_IMMEDIATE_WAKE.with(|c| c.replace(false))
}

fn arm_modal_event_loop_wake() {
    MODAL_CSS_NEEDS_FRAME.with(|f| f.set(true));
    MODAL_FORCE_IMMEDIATE_WAKE.with(|f| f.set(true));
    // Close no longer rebuilds the paint island, so `open.set` has no view to
    // request a redraw. Pair the Poll latch with an actual frame.
    crate::app::runtime::request_redraw();
}

#[cfg(test)]
pub(crate) fn set_modal_css_needs_frame_for_test(v: bool) {
    MODAL_CSS_NEEDS_FRAME.with(|c| c.set(v));
}

/// Skip unflushed enter so tests can assert post-enter paint (focus, layout).
#[cfg(test)]
pub(crate) fn clear_pending_modal_enter(handle: &ModalPresenceHandle) {
    let mut c = handle.cell.borrow_mut();
    c.needs_enter = false;
    c.enter_active = false;
}

/// Wave 2 canary: jump Exiting → Absent without waiting on CSS / wall clock.
#[cfg(test)]
pub(crate) fn finish_modal_exit_for_test(handle: &ModalPresenceHandle) {
    let mounted_sig = mounted_signal(handle);
    {
        let mut c = handle.cell.borrow_mut();
        c.gate.force_absent();
        c.exit_armed = false;
        c.needs_exit_anim = false;
        c.enter_active = false;
        c.shell = None;
    }
    mounted_sig.set(false);
}

pub(crate) fn use_modal_presence(inst_id: &str) -> ModalPresenceHandle {
    let key = format!("__rui_modal_presence:{inst_id}");
    let mounted_key = format!("__rui_modal_mounted:{inst_id}");
    let cell = use_signal(key.as_str(), || {
        let mounted = use_signal(mounted_key.as_str(), || false);
        let rc = Rc::new(RefCell::new(ModalPresenceCell {
            mounted,
            gate: ExitGate::new(false, EXIT_DURATION),
            overlay_motion: MotionStyle::new(),
            panel_motion: MotionStyle::new(),
            needs_enter: false,
            enter_active: false,
            enter_started: None,
            exit_armed: false,
            shell: None,
            open: None,
            needs_exit_anim: false,
            slide_settle: false,
            slide_settle_from: None,
        }));
        PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(Rc::clone(&rc)));
        rc
    });
    let handle = ModalPresenceHandle { cell: cell.peek() };
    take_over_stale_modal_presence(&handle);
    handle
}

fn take_over_stale_modal_presence(handle: &ModalPresenceHandle) {
    let stale = {
        let c = handle.cell.borrow();
        if !c.gate.is_mounted() {
            return;
        }
        match c.open {
            Some(s) => s.try_peek().is_none(),
            None => false,
        }
    };
    if !stale {
        return;
    }
    finish_modal_exit_immediate(handle);
    handle.cell.borrow_mut().open = None;
}

pub(crate) fn mounted_signal(handle: &ModalPresenceHandle) -> Signal<bool> {
    handle.cell.borrow().mounted
}

pub(crate) fn bind_modal_open(handle: &ModalPresenceHandle, open: Signal<bool>) {
    handle.cell.borrow_mut().open = Some(open);
}

fn should_arm_orphan_modal_exit(c: &ModalPresenceCell) -> bool {
    if !c.gate.is_mounted() || c.shell.is_none() {
        return false;
    }
    let Some(open) = c.open else {
        return false;
    };
    open.try_peek().is_none()
}

fn arm_orphaned_modal_exits() -> bool {
    let mut armed = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let handle = ModalPresenceHandle {
                cell: Rc::clone(cell_rc),
            };
            let has_exit = {
                let c = handle.cell.borrow();
                if !should_arm_orphan_modal_exit(&c) {
                    continue;
                }
                c.shell
                    .as_ref()
                    .is_some_and(|s| modal_motion_has_exit(s.motion))
            };
            if !has_exit {
                finish_modal_exit_immediate(&handle);
                continue;
            }
            let phase = sync_modal_presence(&handle, false);
            if let Some(s) = handle.cell.borrow_mut().shell.as_mut() {
                s.phase = phase;
            }
            if phase == PresencePhase::Exiting {
                armed = true;
            }
        }
    });
    if armed {
        arm_modal_event_loop_wake();
    }
    armed
}

fn modal_motion_has_exit(motion: ModalSurfaceMotion) -> bool {
    match motion {
        ModalSurfaceMotion::DialogFade
        | ModalSurfaceMotion::SlideFromTop
        | ModalSurfaceMotion::SlideFromBottom
        | ModalSurfaceMotion::SlideFromLeft
        | ModalSurfaceMotion::SlideFromRight => true,
    }
}

fn finish_modal_exit_immediate(handle: &ModalPresenceHandle) {
    let mounted_sig = mounted_signal(handle);
    {
        let mut c = handle.cell.borrow_mut();
        c.gate.force_absent();
        c.exit_armed = false;
        c.needs_exit_anim = false;
        c.enter_active = false;
        c.shell = None;
    }
    if mounted_sig.try_peek() == Some(true) {
        mounted_sig.set(false);
    }
}

fn prune_finished_orphaned_modals() {
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow_mut().retain(|cell| {
            let c = cell.borrow();
            if c.gate.is_mounted() {
                return true;
            }
            match c.open {
                Some(open) => open.try_peek().is_some(),
                None => true,
            }
        });
    });
}

/// 同步 `open` 期望；返回当前相位。
pub(crate) fn sync_modal_presence(handle: &ModalPresenceHandle, want_open: bool) -> PresencePhase {
    let mounted_sig = mounted_signal(handle);
    let (phase, mounted, set_mounted, wake) = {
        let mut c = handle.cell.borrow_mut();
        let was = c.gate.phase();
        let changed = c.gate.set_present(want_open, Instant::now());
        let phase = c.gate.phase();
        let mounted = c.gate.is_mounted();
        let mut wake = false;
        if want_open
            && matches!(phase, PresencePhase::Present)
            && (changed || was == PresencePhase::Absent)
        {
            c.needs_enter = true;
            c.enter_active = false;
            c.exit_armed = false;
            c.needs_exit_anim = false;
            if let Some(s) = c.shell.as_mut() {
                s.phase = PresencePhase::Present;
            }
            wake = true;
        }
        if matches!(phase, PresencePhase::Exiting) && !c.exit_armed {
            c.exit_armed = true;
            c.needs_enter = false;
            c.enter_active = false;
            c.needs_exit_anim = true;
            if let Some(s) = c.shell.as_mut() {
                s.phase = PresencePhase::Exiting;
            }
            wake = true;
        }
        let set_mounted = c.mounted.peek() != mounted;
        (phase, mounted, set_mounted, wake)
    };
    // The paint island tracks `mounted`, not `open`. Opening from a Computed
    // must `set` so the island mounts. Same-turn peek-sync *inside* that island
    // (default-open first run) must `set_silent` or `body()` runs twice.
    if set_mounted {
        if mounted && crate::core::reactivity::current_effect_is_render_phase() {
            mounted_sig.set_silent(mounted);
        } else {
            mounted_sig.set(mounted);
        }
    }
    if wake {
        arm_modal_event_loop_wake();
    }
    phase
}

/// 事件循环推进退场计时；返回是否仍需 redraw。
pub(crate) fn tick_modal_presence() -> bool {
    let orphaned = arm_orphaned_modal_exits();
    let now = Instant::now();
    let mut needs_frame = orphaned;
    let mut unmount: Vec<Signal<bool>> = Vec::new();
    PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            let maybe_unmount = {
                let mut c = cell.borrow_mut();
                if c.gate.phase() == PresencePhase::Exiting {
                    let dialog_css_exit = c
                        .shell
                        .as_ref()
                        .is_some_and(|s| s.motion == ModalSurfaceMotion::DialogFade);
                    if dialog_css_exit {
                        // Wall-clock ExitGate must not unmount a Dialog whose CSS is still
                        // painting: Bake/layout hitch makes elapsed_ms lag Instant, and the
                        // gate would tear the panel down mid-zoom (stutter + content disappears suddenly).
                        needs_frame = true;
                        None
                    } else if c.gate.tick(now) {
                        c.exit_armed = false;
                        c.enter_active = false;
                        c.overlay_motion.reset();
                        c.panel_motion.reset();
                        c.shell = None;
                        Some(c.mounted)
                    } else {
                        needs_frame = true;
                        None
                    }
                } else if c.enter_active && c.gate.phase() == PresencePhase::Present {
                    needs_frame = true;
                    None
                } else if c.gate.phase() == PresencePhase::Present
                    && c.shell
                        .as_ref()
                        .is_some_and(|s| s.motion == ModalSurfaceMotion::DialogFade)
                    && c.needs_enter
                {
                    needs_frame = true;
                    None
                } else {
                    None
                }
            };
            if let Some(sig) = maybe_unmount {
                unmount.push(sig);
            }
        }
    });
    for sig in unmount {
        sig.set(false);
        crate::app::runtime::try_hoist_portals_now();
    }
    prune_finished_orphaned_modals();
    needs_frame
}

fn motion_timing(motion: ModalSurfaceMotion, exiting: bool) -> (f32, TimingFunction) {
    if matches!(motion, ModalSurfaceMotion::DialogFade) {
        let dur = if exiting {
            MODAL_EXIT_MS
        } else {
            MODAL_ENTER_MS
        };
        let timing = if exiting {
            TimingFunction::EaseIn
        } else {
            TimingFunction::EaseOut
        };
        return (dur, timing);
    }
    if exiting {
        (SLIDE_EXIT_MS, SLIDE_EXIT_EASE)
    } else {
        (SLIDE_ENTER_MS, SLIDE_ENTER_EASE)
    }
}

fn overlay_motion_entries(motion: ModalSurfaceMotion, exiting: bool) -> [TransitionEntry; 1] {
    let (dur, timing) = motion_timing(motion, exiting);
    [TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: dur,
        delay_ms: 0.0,
        timing,
    }]
}

fn panel_motion_entries(motion: ModalSurfaceMotion, exiting: bool) -> [TransitionEntry; 1] {
    let (dur, timing) = motion_timing(motion, exiting);
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: dur,
        delay_ms: 0.0,
        timing,
    }]
}

fn nested_panel_scale(modal_id: &str) -> f32 {
    let (idx, len) = modal_stack_metrics(modal_id);
    if len <= 1 || idx + 1 >= len {
        return 1.0;
    }
    let depth = len - 1 - idx;
    (1.0 - 0.04 * depth as f32).clamp(0.88, 1.0)
}

fn dialog_closed_scale(nested_scale: f32) -> f32 {
    (nested_scale * DIALOG_ENTER_SCALE).clamp(0.88, 1.0)
}

fn extra_to_translate_pct(
    motion: ModalSurfaceMotion,
    extra: [f32; 2],
    slide_axis_px: f32,
) -> [f32; 2] {
    let axis = slide_axis_px.max(1.0);
    match motion {
        ModalSurfaceMotion::DialogFade => [0.0, 0.0],
        ModalSurfaceMotion::SlideFromBottom | ModalSurfaceMotion::SlideFromTop => {
            [0.0, extra[1] / axis]
        }
        ModalSurfaceMotion::SlideFromLeft | ModalSurfaceMotion::SlideFromRight => {
            [extra[0] / axis, 0.0]
        }
    }
}

fn panel_motion_target(
    motion: ModalSurfaceMotion,
    open_visual: bool,
    nested_scale: f32,
    extra_translate: [f32; 2],
    slide_axis_px: f32,
) -> MotionTransform {
    if open_visual {
        return MotionTransform {
            translate: [0.0, 0.0],
            translate_pct: extra_to_translate_pct(motion, extra_translate, slide_axis_px),
            opacity: 1.0,
            scale: [nested_scale, nested_scale],
        };
    }
    // Closed is fully off-screen from the docked pose — do **not** add drag/snap
    // `extra_translate` (that would overshoot when dismissing mid-swipe).
    let base = motion.closed_translate(slide_axis_px);
    let scale = match motion {
        ModalSurfaceMotion::DialogFade => dialog_closed_scale(nested_scale),
        _ => nested_scale,
    };
    let _ = extra_translate;
    MotionTransform {
        translate: base,
        translate_pct: motion.closed_translate_pct(),
        opacity: 1.0,
        scale: [scale, scale],
    }
}

/// Open fraction encoded by drag/snap extra translate (1 = fully open).
fn open_frac_from_extra(motion: ModalSurfaceMotion, extra: [f32; 2], slide_axis_px: f32) -> f32 {
    let axis = slide_axis_px.max(1.0);
    let traveled = match motion {
        ModalSurfaceMotion::SlideFromBottom => extra[1],
        ModalSurfaceMotion::SlideFromTop => -extra[1],
        ModalSurfaceMotion::SlideFromRight => extra[0],
        ModalSurfaceMotion::SlideFromLeft => -extra[0],
        ModalSurfaceMotion::DialogFade => 0.0,
    };
    (1.0 - traveled / axis).clamp(0.0, 1.0)
}

fn overlay_motion_target(open_visual: bool) -> MotionTransform {
    MotionTransform {
        translate: [0.0, 0.0],
        translate_pct: [0.0, 0.0],
        opacity: if open_visual { 1.0 } else { 0.0 },
        scale: [1.0, 1.0],
    }
}

fn find_template_id_mut<'a>(el: &'a mut Element, want: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_by_template_id_mut(el, want)
}

/// DialogFade prefers `{id}__modal_zoom` (Dialog card). Shells that only stamp
/// `{id}__modal_panel` still need overlay fade/exit, or the scrim stays at
/// opacity 0 and ExitGate never finishes.
fn dialog_fade_motion_tid(root: &Element, modal_id: &str) -> String {
    let zoom = dialog_zoom_tid(modal_id);
    if crate::core::tree::find_by_template_id(root, &zoom).is_some() {
        zoom
    } else {
        format!("{modal_id}__modal_panel")
    }
}

fn seed_modal_shell_transition_mid(el: &mut Element) {
    if el.style.transition.is_empty() {
        return;
    }
    let _ = el.tick_css_transitions_locked(0.0);
}

fn enter_visual_settled(
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
    slide_axis_px: f32,
) -> bool {
    // Same class of cutoff as Dialog zoom: 2–4% remaining snapped the last
    // ease-out tail onto the dock in one frame (Sheet/Drawer 入场末段瞬跳).
    let overlay_ok = (overlay.style.opacity - 1.0).abs() < 0.005;
    let nested = 1.0_f32;
    let panel_ok = if motion == ModalSurfaceMotion::DialogFade {
        (panel.style.scale[0] - nested).abs() < 0.002
    } else {
        let target_pct = extra_to_translate_pct(motion, target_translate, slide_axis_px);
        let pct_ok = (panel.style.translate_pct[0] - target_pct[0]).abs() < 0.002
            && (panel.style.translate_pct[1] - target_pct[1]).abs() < 0.002;
        let px_ok = panel.style.translate[0].abs() < 1.0 && panel.style.translate[1].abs() < 1.0;
        pct_ok && px_ok
    };
    overlay_ok && panel_ok
}

fn needs_hoist_enter_seed(
    overlay: &Element,
    panel: &Element,
    enter_was_active: bool,
    needs_enter: bool,
    open_visual: bool,
    exiting: bool,
) -> bool {
    // Rebuild dropped the animation *list*. Names present (even before the first tick)
    // are not "lost" — `animation_run_active` is false until tick and must not reseed.
    enter_was_active
        && !needs_enter
        && open_visual
        && !exiting
        && (!slide_overlay_has_enter_anim(overlay) || !slide_panel_has_enter_anim(panel))
}

fn enter_animation_running(
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
    slide_axis_px: f32,
) -> bool {
    if enter_visual_settled(overlay, panel, motion, target_translate, slide_axis_px) {
        return false;
    }
    true
}

fn write_modal_shell_tick_targets(
    shell: &ModalShellState,
    overlay: &mut Element,
    panel: &mut Element,
) {
    // Pixel specified targets are only for drag-release CSS transition.
    // Keyframe enter/exit own opacity / translate_pct.
    let settle = panel
        .style
        .transition
        .iter()
        .any(|e| e.property == TransitionProperty::Transform);
    if !settle || shell.phase == PresencePhase::Exiting {
        return;
    }
    let nested = nested_panel_scale(&shell.modal_id);
    let o_target = overlay_motion_target(true);
    let p_target = panel_motion_target(
        shell.motion,
        true,
        nested,
        shell.extra_translate,
        shell.slide_axis_px,
    );
    panel.style.translate = p_target.translate;
    panel.style.translate_pct = p_target.translate_pct;
    panel.style.opacity = 1.0;
    panel.style.scale = p_target.scale;
    overlay.style.opacity = o_target.opacity;
}

fn exit_visual_settled(overlay: &Element, panel: &Element, motion: ModalSurfaceMotion) -> bool {
    let overlay_done = overlay.style.opacity < 0.05;
    let closed = motion.closed_translate_pct();
    let pct_done = (panel.style.translate_pct[0] - closed[0]).abs() < 0.04
        && (panel.style.translate_pct[1] - closed[1]).abs() < 0.04;
    overlay_done && pct_done
}

fn modal_shell_enter_css_active(overlay: &Element, panel: &Element) -> bool {
    overlay.transition_snapshot.has_active_tweens()
        || panel.transition_snapshot.has_active_tweens()
        || css_transition_needs_frame(&overlay.style, &overlay.transition_snapshot)
        || css_transition_needs_frame(&panel.style, &panel.transition_snapshot)
        || animation_run_active(&overlay.style, &overlay.animation_snapshot)
        || animation_run_active(&panel.style, &panel.animation_snapshot)
}

fn settle_modal_enter_if_ready(
    presence: &ModalPresenceHandle,
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
    slide_axis_px: f32,
) -> bool {
    // Do not nail while `@keyframes` still own the pose — fill-mode already holds
    // the 100% stop, so clearing mid-tail jumps the remainder in one present.
    let settled = enter_visual_settled(overlay, panel, motion, target_translate, slide_axis_px)
        && !modal_shell_enter_css_active(overlay, panel);
    if settled {
        let modal_id = presence
            .cell
            .borrow()
            .shell
            .as_ref()
            .map(|s| s.modal_id.clone());
        {
            let mut c = presence.cell.borrow_mut();
            c.enter_active = false;
            c.slide_settle = false;
        }
        crate::utils::popup::arm_underlay_close();
        if let Some(id) = modal_id {
            notify_slide_settle_complete(&id);
        }
    }
    !settled
}

fn reset_dialog_css_anim_snapshots(overlay: &mut Element, zoom: &mut Element) {
    overlay.animation_snapshot = Default::default();
    zoom.animation_snapshot = Default::default();
}

fn dialog_enter_anim_entries() -> Vec<crate::style::AnimationEntry> {
    vec![
        keyframes_anim_entry(
            "fade-in-0",
            DIALOG_ENTER_ANIM_MS,
            DIALOG_ENTER_EASE,
            AnimationFillMode::Both,
        ),
        keyframes_anim_entry(
            "zoom-in-90",
            DIALOG_ENTER_ANIM_MS,
            DIALOG_ENTER_EASE,
            AnimationFillMode::Both,
        ),
    ]
}

fn seed_dialog_enter_anim(overlay: &mut Element, zoom: &mut Element) {
    clear_motion(overlay);
    clear_motion(zoom);
    overlay.style.opacity = 0.0;
    // shadcn content is `fade-in-0 zoom-in-90`: the panel must be **invisible** while it is
    // still scaled down. Zooming an opaque panel shows a 90% render of finished text/borders
    // that then snaps to 100% — the "shrunk state → normal state" jump.
    zoom.style.opacity = 0.0;
    zoom.style.scale = [DIALOG_ENTER_SCALE, DIALOG_ENTER_SCALE];
    overlay.style.animations = vec![keyframes_anim_entry(
        "fade-in-0",
        DIALOG_ENTER_ANIM_MS,
        DIALOG_ENTER_EASE,
        AnimationFillMode::Both,
    )];
    zoom.style.animations = dialog_enter_anim_entries();
    reset_dialog_css_anim_snapshots(overlay, zoom);
}

/// Rebuilds can drop the animation list. Restore names without resetting the mid pose
/// (full [`seed_dialog_enter_anim`] would flash closed → open).
fn ensure_dialog_enter_anim(overlay: &mut Element, zoom: &mut Element) {
    if !overlay
        .style
        .animations
        .iter()
        .any(|a| a.name == "fade-in-0")
    {
        overlay.style.animations = vec![keyframes_anim_entry(
            "fade-in-0",
            DIALOG_ENTER_ANIM_MS,
            DIALOG_ENTER_EASE,
            AnimationFillMode::Both,
        )];
    }
    if !zoom.style.animations.iter().any(|a| a.name == "zoom-in-90") {
        zoom.style.animations = dialog_enter_anim_entries();
    }
}

fn dialog_exit_overlay_anim() -> Vec<crate::style::AnimationEntry> {
    vec![keyframes_anim_entry(
        "fade-out-0",
        DIALOG_EXIT_ANIM_MS,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )]
}

fn dialog_exit_zoom_anim() -> Vec<crate::style::AnimationEntry> {
    vec![
        keyframes_anim_entry(
            "fade-out-0",
            DIALOG_EXIT_ANIM_MS,
            TimingFunction::EaseIn,
            AnimationFillMode::Both,
        ),
        keyframes_anim_entry(
            "zoom-out-90",
            DIALOG_EXIT_ANIM_MS,
            TimingFunction::EaseIn,
            AnimationFillMode::Both,
        ),
    ]
}

fn seed_dialog_exit_anim(overlay: &mut Element, zoom: &mut Element) {
    clear_motion(overlay);
    clear_motion(zoom);
    overlay.style.opacity = 1.0;
    zoom.style.opacity = 1.0;
    zoom.style.scale = [1.0, 1.0];
    overlay.style.animations = dialog_exit_overlay_anim();
    zoom.style.animations = dialog_exit_zoom_anim();
    reset_dialog_css_anim_snapshots(overlay, zoom);
}

fn dialog_css_animating(overlay: &Element, zoom: &Element) -> bool {
    crate::style::animation_run_active(&overlay.style, &overlay.animation_snapshot)
        || crate::style::animation_run_active(&zoom.style, &zoom.animation_snapshot)
}

fn slide_overlay_has_enter_anim(el: &Element) -> bool {
    el.style.animations.iter().any(|a| a.name == "fade-in-0")
}

fn slide_overlay_has_exit_anim(el: &Element) -> bool {
    el.style.animations.iter().any(|a| a.name == "fade-out-0")
}

fn slide_overlay_has_fade(el: &Element) -> bool {
    slide_overlay_has_enter_anim(el) || slide_overlay_has_exit_anim(el)
}

fn seed_slide_overlay_enter(overlay: &mut Element) {
    clear_motion(overlay);
    overlay.style.opacity = 0.0;
    overlay.style.animations = vec![keyframes_anim_entry(
        "fade-in-0",
        SLIDE_ENTER_MS,
        SLIDE_ENTER_EASE,
        AnimationFillMode::Both,
    )];
    overlay.animation_snapshot = Default::default();
}

fn seed_slide_overlay_exit(overlay: &mut Element) {
    clear_motion(overlay);
    overlay.style.opacity = 1.0;
    overlay.style.animations = vec![keyframes_anim_entry(
        "fade-out-0",
        SLIDE_EXIT_MS,
        SLIDE_EXIT_EASE,
        AnimationFillMode::Both,
    )];
    overlay.animation_snapshot = Default::default();
}

fn slide_panel_exit_kf_name(motion: ModalSurfaceMotion) -> &'static str {
    match motion {
        ModalSurfaceMotion::SlideFromTop => "rui-modal-slide-out-top",
        ModalSurfaceMotion::SlideFromBottom => "rui-modal-slide-out-bottom",
        ModalSurfaceMotion::SlideFromLeft => "rui-modal-slide-out-left",
        ModalSurfaceMotion::SlideFromRight => "rui-modal-slide-out-right",
        ModalSurfaceMotion::DialogFade => "",
    }
}

fn slide_panel_enter_kf_name(motion: ModalSurfaceMotion) -> &'static str {
    match motion {
        ModalSurfaceMotion::SlideFromTop => "rui-modal-slide-in-top",
        ModalSurfaceMotion::SlideFromBottom => "rui-modal-slide-in-bottom",
        ModalSurfaceMotion::SlideFromLeft => "rui-modal-slide-in-left",
        ModalSurfaceMotion::SlideFromRight => "rui-modal-slide-in-right",
        ModalSurfaceMotion::DialogFade => "",
    }
}

fn slide_panel_has_exit_anim(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| a.name.starts_with("rui-modal-slide-out-"))
}

fn slide_panel_has_enter_anim(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| a.name.starts_with("rui-modal-slide-in-"))
}

fn seed_slide_panel_enter(panel: &mut Element, motion: ModalSurfaceMotion) {
    let name = slide_panel_enter_kf_name(motion);
    if name.is_empty() {
        return;
    }
    let closed = motion.closed_translate_pct();
    clear_motion(panel);
    panel.style.translate = [0.0, 0.0];
    panel.style.translate_pct = closed;
    panel.style.opacity = 1.0;
    panel.style.animations = vec![keyframes_anim_entry(
        name,
        SLIDE_ENTER_MS,
        SLIDE_ENTER_EASE,
        AnimationFillMode::Both,
    )];
    panel.animation_snapshot = Default::default();
}

fn seed_slide_panel_exit(panel: &mut Element, motion: ModalSurfaceMotion) {
    let name = slide_panel_exit_kf_name(motion);
    if name.is_empty() {
        return;
    }
    clear_motion(panel);
    panel.style.translate = [0.0, 0.0];
    panel.style.translate_pct = [0.0, 0.0];
    panel.style.opacity = 1.0;
    panel.style.animations = vec![keyframes_anim_entry(
        name,
        SLIDE_EXIT_MS,
        SLIDE_EXIT_EASE,
        AnimationFillMode::Both,
    )];
    panel.animation_snapshot = Default::default();
}

fn finish_dialog_exit_presence(presence: &ModalPresenceHandle, modal_id: &str) {
    let mounted_sig = mounted_signal(presence);
    {
        let mut c = presence.cell.borrow_mut();
        c.gate.force_absent();
        c.exit_armed = false;
        c.needs_exit_anim = false;
        c.enter_active = false;
        c.shell = None;
    }
    // Cannot `mounted.set` / hoist here: called from flush while AppState is borrowed.
    PENDING_MODAL_UNMOUNTS.with(|q| {
        q.borrow_mut().push((mounted_sig, modal_id.to_string()));
    });
    crate::utils::popup::arm_underlay_close();
    crate::app::runtime::request_redraw();
}

/// Drain CSS Dialog exit unmounts. Call only when AppState is not borrowed.
pub(crate) fn drain_pending_modal_unmounts() {
    let pending = PENDING_MODAL_UNMOUNTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
    if pending.is_empty() {
        return;
    }
    use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
    for (mounted_sig, modal_id) in pending {
        mounted_sig.set(false);
        let _ = portal_sync(
            &modal_id,
            mounted_sig,
            OverlayKind::Modal,
            OverlayMode::Stacked,
        );
    }
    crate::app::runtime::try_hoist_portals_now();
}

/// shadcn 风格 Dialog：overlay `fade` + 内层 [`dialog_zoom_tid`] `zoom`（纯绘制，不改面板布局）。
fn apply_dialog_css_keyframe_motion(
    modal_id: &str,
    overlay: &mut Element,
    zoom: &mut Element,
    phase: PresencePhase,
    presence: &ModalPresenceHandle,
) -> bool {
    overlay.template_id = Some(format!("{modal_id}__modal_overlay"));
    zoom.template_id = Some(dialog_zoom_tid(modal_id));

    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;

    let (needs_enter, needs_exit_anim, enter_active) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        if needs_enter && open_visual {
            c.needs_enter = false;
            c.enter_active = true;
            c.enter_started = Some(Instant::now());
        }
        if exiting {
            if needs_exit_anim {
                c.needs_exit_anim = false;
            }
            // Do not let enter catch-up seek a leftover `enter_started` on the
            // first exit tick (rebuilds can drop snapshots → hold_seed).
            c.enter_started = None;
        }
        (needs_enter, needs_exit_anim, c.enter_active)
    };

    if exiting && needs_exit_anim {
        seed_dialog_exit_anim(overlay, zoom);
        return true;
    }

    if exiting {
        let animating = dialog_css_animating(overlay, zoom);
        if !animating {
            overlay.style.animations.clear();
            zoom.style.animations.clear();
            finish_dialog_exit_presence(presence, modal_id);
            return false;
        }
        if overlay.style.opacity < 0.08 {
            overlay.set_attr(
                crate::core::tree::ATTR_POINTER_EVENTS,
                crate::core::tree::PointerEvents::None.as_str(),
            );
        }
        return true;
    }

    if open_visual && needs_enter {
        seed_dialog_enter_anim(overlay, zoom);
        return true;
    }

    if open_visual {
        if enter_active {
            ensure_dialog_enter_anim(overlay, zoom);
        }
        let animating = dialog_css_animating(overlay, zoom);
        // Nail only when the pose is already rest. A 2% cutoff snapped the last
        // compositor blit (dest AABB + bitmap rows) onto Coverage in one frame.
        let zoom_done = (zoom.style.scale[0] - 1.0).abs() < 0.002 && overlay.style.opacity > 0.97;
        if !animating && zoom_done {
            overlay.style.opacity = 1.0;
            overlay.style.animations.clear();
            zoom.style.opacity = 1.0;
            zoom.style.scale = [1.0, 1.0];
            zoom.style.animations.clear();
            presence.cell.borrow_mut().enter_active = false;
            presence.cell.borrow_mut().enter_started = None;
            crate::utils::popup::arm_underlay_close();
            return false;
        }
        // Match Sheet `enter_in_progress`: keep flush requesting frames for the
        // whole enter, not only while `animation_run_active` is true.
        return animating || enter_active;
    }

    false
}

fn apply_modal_shell_motion_inner(
    modal_id: &str,
    overlay: &mut Element,
    panel: &mut Element,
    motion: ModalSurfaceMotion,
    phase: PresencePhase,
    presence: &ModalPresenceHandle,
    extra_translate: [f32; 2],
    dragging: bool,
    slide_axis_px: f32,
) -> bool {
    if motion == ModalSurfaceMotion::DialogFade {
        return apply_dialog_css_keyframe_motion(modal_id, overlay, panel, phase, presence);
    }

    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;
    let (needs_enter, enter_was_active, slide_settle, settle_from, needs_exit_anim) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        let slide_settle = c.slide_settle;
        let settle_from = c.slide_settle_from.take();
        // Read before settle/enter flags — `slide_settle` sets `enter_active`, and a
        // rebuilt node looks like a lost hoist mid. That re-seeded closed→open ("reappear").
        let enter_was_active = c.enter_active;
        if needs_enter && open_visual {
            c.needs_enter = false;
            c.enter_active = true;
        }
        if needs_exit_anim && exiting {
            c.needs_exit_anim = false;
        }
        if slide_settle {
            // Keep the flag until visual settle so continuation frames do not
            // look like a lost enter and re-seed `@keyframes` from 100%.
            c.enter_active = true;
        }
        (
            needs_enter,
            enter_was_active,
            slide_settle,
            settle_from,
            needs_exit_anim,
        )
    };

    overlay.template_id = Some(format!("{modal_id}__modal_overlay"));
    panel.template_id = Some(format!("{modal_id}__modal_panel"));

    let slide_axis_px = slide_axis_for_panel(motion, panel, slide_axis_px);

    let hoist_enter_seed = !slide_settle
        && needs_hoist_enter_seed(
            overlay,
            panel,
            enter_was_active,
            needs_enter,
            open_visual,
            exiting,
        );
    let hoist_exit_seed = exiting && !needs_exit_anim && !slide_panel_has_exit_anim(panel);
    let settle_snap = !dragging && open_visual && slide_settle && !exiting;
    let enter_in_progress = !dragging
        && open_visual
        && enter_was_active
        && !exiting
        && !settle_snap
        && !needs_enter
        && !hoist_enter_seed;

    if !dragging && exiting && (needs_exit_anim || !slide_overlay_has_exit_anim(overlay)) {
        seed_slide_overlay_exit(overlay);
    } else if !dragging
        && open_visual
        && !slide_settle
        && (needs_enter || (enter_was_active && !slide_overlay_has_enter_anim(overlay)))
    {
        seed_slide_overlay_enter(overlay);
    }
    if !dragging
        && open_visual
        && !slide_settle
        && motion.animates_panel_transform()
        && (needs_enter || (enter_was_active && !slide_panel_has_enter_anim(panel)))
    {
        seed_slide_panel_enter(panel, motion);
    }
    if !dragging
        && exiting
        && motion.animates_panel_transform_on_exit()
        && (needs_exit_anim || hoist_exit_seed)
    {
        seed_slide_panel_exit(panel, motion);
    }

    if enter_in_progress
        || (!dragging && exiting)
        || (!dragging && !settle_snap && open_visual && (needs_enter || hoist_enter_seed))
    {
        return true;
    }

    // Opacity / transform CSS transition is only for drag-release snap.
    let animate_overlay = settle_snap;
    let animate_panel_transform = settle_snap && motion.animates_panel_transform();

    let overlay_entries = overlay_motion_entries(motion, exiting);
    let panel_transform_entries = panel_motion_entries(motion, exiting);
    let nested = nested_panel_scale(modal_id);
    let o_target = overlay_motion_target(open_visual);
    let p_target = panel_motion_target(motion, open_visual, nested, extra_translate, slide_axis_px);
    let bootstrap = if settle_snap && settle_from.is_none() {
        Some(MotionBootstrap::EnsureOnly)
    } else {
        Some(MotionBootstrap::SnapAfterEnsure)
    };

    if dragging {
        overlay.style.animations.clear();
        overlay.animation_snapshot = Default::default();
        let mut c = presence.cell.borrow_mut();
        clear_motion(overlay);
        clear_motion(panel);
        c.overlay_motion.reset();
        c.panel_motion.reset();
        // Swipe owns transform/opacity — an in-flight enter tick would write the
        // snap target and can leave the panel fill on a stale clip.
        c.enter_active = false;
        let frac = c
            .shell
            .as_ref()
            .map(|s| s.live_open_frac)
            .unwrap_or_else(|| open_frac_from_extra(motion, extra_translate, slide_axis_px));
        overlay.style.opacity = frac.clamp(0.0, 1.0);
        panel.style.translate = p_target.translate;
        panel.style.translate_pct = p_target.translate_pct;
        panel.style.opacity = 1.0;
        panel.style.scale = p_target.scale;
        return true;
    }

    // Seed drag-release mid before prepare_motion_sync snapshots the baseline.
    if settle_snap {
        if let Some(from) = settle_from {
            panel.style.translate = [0.0, 0.0];
            panel.style.translate_pct = extra_to_translate_pct(motion, from, slide_axis_px);
            panel.style.opacity = 1.0;
            overlay.style.opacity = open_frac_from_extra(motion, from, slide_axis_px);
        }
    }

    let entering_mid = open_visual
        && enter_was_active
        && !enter_visual_settled(overlay, panel, motion, extra_translate, slide_axis_px)
        && modal_shell_enter_css_active(overlay, panel);

    if animate_overlay {
        let mut c = presence.cell.borrow_mut();
        prepare_motion_sync(
            overlay,
            &overlay_entries,
            bootstrap,
            &mut c.overlay_motion,
            false,
        );
    } else if !entering_mid && !slide_overlay_has_fade(overlay) {
        let mut c = presence.cell.borrow_mut();
        clear_motion(overlay);
        c.overlay_motion.reset();
        overlay.style.opacity = o_target.opacity;
    }

    if animate_panel_transform {
        let mut c = presence.cell.borrow_mut();
        prepare_motion_sync(
            panel,
            &panel_transform_entries,
            bootstrap,
            &mut c.panel_motion,
            false,
        );
    } else if !entering_mid {
        let mut c = presence.cell.borrow_mut();
        clear_motion(panel);
        c.panel_motion.reset();
        panel.style.translate = p_target.translate;
        panel.style.translate_pct = p_target.translate_pct;
        panel.style.opacity = 1.0;
        panel.style.scale = p_target.scale;
    }

    let mut c = presence.cell.borrow_mut();
    let o_changed = if animate_overlay {
        c.overlay_motion
            .apply(&mut overlay.style, o_target, MotionEpsilon::default())
    } else {
        false
    };
    let p_changed = if animate_panel_transform {
        c.panel_motion
            .apply(&mut panel.style, p_target, MotionEpsilon::default())
    } else {
        false
    };

    let mut needs_frame = motion_sync_status(overlay, o_changed, false) != LayoutStatus::Clean
        || motion_sync_status(panel, p_changed, false) != LayoutStatus::Clean
        || settle_snap;

    if animate_overlay {
        seed_modal_shell_transition_mid(overlay);
    }
    if animate_panel_transform {
        seed_modal_shell_transition_mid(panel);
    }

    if open_visual && enter_was_active {
        if enter_animation_running(overlay, panel, motion, extra_translate, slide_axis_px)
            || modal_shell_enter_css_active(overlay, panel)
        {
            needs_frame = true;
        } else {
            c.enter_active = false;
        }
    }

    needs_frame
}

fn flush_shell_state(
    root: &mut Element,
    shell: &ModalShellState,
    presence: &ModalPresenceHandle,
) -> bool {
    let overlay_tid = format!("{}__modal_overlay", shell.modal_id);
    let overlay_ptr = find_template_id_mut(root, &overlay_tid).map(|el| el as *mut Element);
    let motion_tid = if shell.motion == ModalSurfaceMotion::DialogFade {
        dialog_fade_motion_tid(root, &shell.modal_id)
    } else {
        format!("{}__modal_panel", shell.modal_id)
    };
    let motion_ptr = find_template_id_mut(root, &motion_tid).map(|el| el as *mut Element);
    let (Some(overlay_ptr), Some(motion_ptr)) = (overlay_ptr, motion_ptr) else {
        return true;
    };
    let (overlay, motion_el) = unsafe { (&mut *overlay_ptr, &mut *motion_ptr) };
    apply_modal_shell_motion_inner(
        &shell.modal_id,
        overlay,
        motion_el,
        shell.motion,
        presence.phase(),
        presence,
        shell.extra_translate,
        shell.dragging,
        shell.slide_axis_px,
    )
}

/// 拖拽等跟手路径：直接写面板 transform，避免 reactive 重建。
pub(crate) fn patch_modal_panel_transform(root: &mut Element, modal_id: &str, translate: [f32; 2]) {
    let panel_tid = format!("{modal_id}__modal_panel");
    let Some(panel) = find_template_id_mut(root, &panel_tid) else {
        return;
    };
    let (motion, axis) = PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            if let Some(s) = cell.borrow().shell.as_ref() {
                if s.modal_id == modal_id {
                    return (s.motion, s.slide_axis_px.max(1.0));
                }
            }
        }
        (ModalSurfaceMotion::SlideFromBottom, 1.0)
    });
    let pct = extra_to_translate_pct(motion, translate, axis);
    clear_motion(panel);
    let prev_px = panel.style.translate;
    let prev_pct = panel.style.translate_pct;
    let w = panel.layout_bounds[2].max(1.0);
    let h = panel.layout_bounds[3].max(1.0);
    panel.style.translate = [0.0, 0.0];
    panel.style.translate_pct = pct;
    panel.style.opacity = 1.0;
    crate::app::damage::push_frame_paint_damage(panel.layout_bounds);
    crate::app::damage::push_frame_paint_damage(translated_bounds(panel.layout_bounds, prev_px));
    crate::app::damage::push_frame_paint_damage(translated_bounds(
        panel.layout_bounds,
        [prev_pct[0] * w, prev_pct[1] * h],
    ));
    crate::app::damage::push_frame_paint_damage(translated_bounds(
        panel.layout_bounds,
        [pct[0] * w, pct[1] * h],
    ));
}

/// 拖拽跟手：遮罩透明度随 open_frac（对齐 shadcn `--drawer-swipe-progress`）。
pub(crate) fn patch_modal_overlay_opacity(root: &mut Element, modal_id: &str, opacity: f32) {
    let overlay_tid = format!("{modal_id}__modal_overlay");
    let Some(overlay) = find_template_id_mut(root, &overlay_tid) else {
        return;
    };
    clear_motion(overlay);
    overlay.style.animations.clear();
    overlay.animation_snapshot = Default::default();
    overlay.style.opacity = opacity.clamp(0.0, 1.0);
    crate::app::damage::push_frame_paint_damage(overlay.layout_bounds);
}

fn translated_bounds(bounds: [f32; 4], translate: [f32; 2]) -> [f32; 4] {
    [
        bounds[0] + translate[0],
        bounds[1] + translate[1],
        bounds[2],
        bounds[3],
    ]
}

/// Keep flush's dragging branch in sync with the live swipe (no reactive rebuild).
pub(crate) fn note_modal_shell_drag(
    modal_id: &str,
    extra: [f32; 2],
    dragging: bool,
    open_frac: f32,
) {
    PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            let mut c = cell.borrow_mut();
            if let Some(s) = c.shell.as_mut() {
                if s.modal_id == modal_id {
                    s.extra_translate = extra;
                    s.dragging = dragging;
                    s.live_open_frac = open_frac.clamp(0.0, 1.15);
                    s.phase = if dragging {
                        PresencePhase::Present
                    } else {
                        s.phase
                    };
                }
            }
        }
    });
}

/// Drawer 松手后吸附：下一帧从 `from` 位移 CSS transition 到 snap 目标。
pub(crate) fn arm_modal_slide_settle(modal_id: &str, from: [f32; 2]) {
    PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            let mut c = cell.borrow_mut();
            if c.shell.as_ref().is_some_and(|s| s.modal_id == modal_id) {
                c.slide_settle = true;
                c.slide_settle_from = Some(from);
            }
        }
    });
}

/// Advance CSS transition on entry (must be after hoist + [`flush_modal_shell_motion_subtree`]).
pub(crate) fn tick_modal_enter_css_subtree(root: &mut Element) -> bool {
    let mut needs_frame = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let (run_tick, shell, handle, exiting) = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                if shell.motion == ModalSurfaceMotion::DialogFade {
                    continue;
                }
                if shell.dragging {
                    continue;
                }
                let exiting = c.gate.phase() == PresencePhase::Exiting;
                if !c.enter_active && !exiting {
                    continue;
                }
                (
                    true,
                    shell,
                    ModalPresenceHandle {
                        cell: Rc::clone(cell_rc),
                    },
                    exiting,
                )
            };
            if !run_tick {
                continue;
            }
            let overlay_tid = format!("{}__modal_overlay", shell.modal_id);
            let panel_tid = format!("{}__modal_panel", shell.modal_id);
            let overlay_ptr = find_template_id_mut(root, &overlay_tid).map(|el| el as *mut Element);
            let panel_ptr = find_template_id_mut(root, &panel_tid).map(|el| el as *mut Element);
            let (Some(overlay_ptr), Some(panel_ptr)) = (overlay_ptr, panel_ptr) else {
                needs_frame = true;
                continue;
            };
            let (overlay, panel) = unsafe { (&mut *overlay_ptr, &mut *panel_ptr) };
            // Write specified targets, then sample transitions so style holds mid (not the target).
            write_modal_shell_tick_targets(&shell, overlay, panel);
            let _ = tick_element_css(overlay, 0.0);
            let _ = tick_element_css(panel, 0.0);
            needs_frame = true;
            if exiting {
                if exit_visual_settled(overlay, panel, shell.motion) {
                    panel.style.opacity = 0.0;
                    panel.style.translate = [0.0, 0.0];
                    panel.style.translate_pct = shell.motion.closed_translate_pct();
                    finish_dialog_exit_presence(&handle, &shell.modal_id);
                }
            } else if settle_modal_enter_if_ready(
                &handle,
                overlay,
                panel,
                shell.motion,
                shell.extra_translate,
                shell.slide_axis_px,
            ) {
                needs_frame = true;
            } else {
                overlay.style.animations.clear();
                overlay.animation_snapshot = Default::default();
                overlay.style.opacity = 1.0;
                panel.style.animations.clear();
                panel.animation_snapshot = Default::default();
                panel.style.translate = [0.0, 0.0];
                panel.style.translate_pct = [0.0, 0.0];
            }
        }
    });
    needs_frame
}

fn tick_element_css(el: &mut Element, dt: f32) -> bool {
    let anim = if !el.style.animations.is_empty()
        || crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot)
    {
        el.tick_css_animations(dt)
    } else {
        false
    };
    let trans = if !el.style.transition.is_empty()
        || el.transition_snapshot.has_active_tweens()
        || crate::style::css_transition_needs_frame(&el.style, &el.transition_snapshot)
    {
        el.tick_css_transitions_locked(dt)
    } else {
        false
    };
    anim || trans
}

/// Tick CSS and dest-reset Overlay occupancy when the pose actually moved.
///
/// [`crate::style::tick_element_animations`] returns false on `TimingPhase::Finished`
/// even though fill-mode just wrote the 100% sample. `fade-out-0` is `ease-in`, so
/// that last sample is most of the fade — skipping dirty freezes the previous GPU
/// stamp until [`finish_dialog_exit_presence`] unmounts.
fn tick_hoisted_shell_css(el: &mut Element, dt: f32) -> bool {
    let opacity = el.style.opacity;
    let scale = el.style.scale;
    let translate = el.style.translate;
    let translate_pct = el.style.translate_pct;
    let ticked = tick_element_css(el, dt);
    let posed = (el.style.opacity - opacity).abs() > 1e-6
        || (el.style.scale[0] - scale[0]).abs() > 1e-6
        || (el.style.scale[1] - scale[1]).abs() > 1e-6
        || (el.style.translate[0] - translate[0]).abs() > 1e-6
        || (el.style.translate[1] - translate[1]).abs() > 1e-6
        || (el.style.translate_pct[0] - translate_pct[0]).abs() > 1e-6
        || (el.style.translate_pct[1] - translate_pct[1]).abs() > 1e-6;
    if ticked || posed {
        crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
        true
    } else {
        false
    }
}

/// Advance CSS on hoisted overlay / panel / zoom. Call **after**
/// [`flush_modal_shell_motion_subtree`] so seed/targets exist and this is the only stepper.
pub(crate) fn tick_hoisted_modal_css(root: &mut Element) -> bool {
    // Overlay fade-out-0 / panel slide-out advance by `dt`, not Instant. After idle,
    // `frame_dt` can be seconds. Cap only those jumps to one display frame so close
    // still plays mids; 30 Hz frames (~33ms) must not be slowed to 16ms.
    let raw = crate::style::utility::frame_dt().max(0.0);
    // Idle jumps (>80ms) → one 60 Hz step. Other frames never skip more than ~33ms
    // so a 200ms Dialog enter still has visible mids (a 50ms dt ate 25% of the zoom).
    let dt = if raw > 0.08 {
        1.0 / 60.0
    } else {
        raw.min(1.0 / 30.0)
    };
    let mut any = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let shell = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                if shell.dragging {
                    continue;
                }
                shell
            };
            let (enter_started, exiting) = {
                let c = cell_rc.borrow();
                (c.enter_started, c.gate.phase() == PresencePhase::Exiting)
            };
            let overlay_tid = format!("{}__modal_overlay", shell.modal_id);
            let motion_tid = if shell.motion == ModalSurfaceMotion::DialogFade {
                dialog_fade_motion_tid(root, &shell.modal_id)
            } else {
                format!("{}__modal_panel", shell.modal_id)
            };
            let overlay_ptr = find_template_id_mut(root, &overlay_tid).map(|el| el as *mut Element);
            let motion_ptr = find_template_id_mut(root, &motion_tid).map(|el| el as *mut Element);
            let (Some(overlay_ptr), Some(motion_ptr)) = (overlay_ptr, motion_ptr) else {
                any = true;
                continue;
            };
            let (overlay, motion_el) = unsafe { (&mut *overlay_ptr, &mut *motion_ptr) };
            // Recreated nodes (first-open layout churn) have empty snapshots — seek to
            // `enter_started` instead of restarting at 0%.
            //
            // Same-turn seed+tick used to pass the raw idle `frame_dt` (seconds) and jump
            // a 200ms zoom-in-90 to settle. `dt` is already capped to one display frame,
            // so a 0-step "hold seed" present is no longer needed — and that blank
            // present was the wait before the popup appeared: click → expensive first
            // layout at opacity 0 → extra vsync before any pixels.
            let hold_seed = overlay.animation_snapshot.runs.is_empty()
                && motion_el.animation_snapshot.runs.is_empty();
            // Catch-up is enter-only. Exit rebuilds (AlertDialog Action click) often
            // drop snapshots; seeking `enter_started` (seconds ago) would consume the
            // whole 200ms fade/zoom in one tick and skip the exit animation.
            let step = if hold_seed {
                if shell.motion != ModalSurfaceMotion::DialogFade {
                    0.0
                } else if exiting {
                    dt
                } else {
                    match enter_started {
                        Some(t) => {
                            let elapsed = t.elapsed().as_secs_f32();
                            if elapsed < 0.0005 {
                                dt
                            } else {
                                elapsed.min(DIALOG_ENTER_ANIM_MS / 1000.0)
                            }
                        }
                        None => dt,
                    }
                }
            } else {
                dt
            };
            if tick_hoisted_shell_css(overlay, step) {
                any = true;
            }
            if tick_hoisted_shell_css(motion_el, step) {
                any = true;
            }
            if crate::style::css_animation_needs_frame(&overlay.style, &overlay.animation_snapshot)
                || crate::style::css_animation_needs_frame(
                    &motion_el.style,
                    &motion_el.animation_snapshot,
                )
            {
                any = true;
            }
        }
    });
    MODAL_CSS_NEEDS_FRAME.with(|c| c.set(any));
    any
}

/// Skip the pre-hoist CSS tick for live modal overlay / panel / zoom nodes.
///
/// `sync_class_variants_subtree` runs **before** portal hoist +
/// [`flush_modal_shell_motion_subtree`]. Ticking then either steps a stale copy or
/// advances a node that flush is about to re-seed — Dialog fade jumps to opaque on
/// the first presented frame. The post-hoist tick in `AppState::render` is the
/// single stepper.
pub(crate) fn modal_shell_defer_css_tick(tid: Option<&str>) -> bool {
    let Some(t) = tid else {
        return false;
    };
    let modal_id = match t
        .strip_suffix("__modal_overlay")
        .or_else(|| t.strip_suffix("__modal_panel"))
        .or_else(|| t.strip_suffix("__modal_zoom"))
    {
        Some(id) => id,
        None => return false,
    };
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow().iter().any(|cell| {
            let c = cell.borrow();
            c.gate.is_mounted() && c.shell.as_ref().is_some_and(|s| s.modal_id == modal_id)
        })
    })
}

/// After Portal hoist: Press presence registration + `template_id` to write motion to the hoisted tree.
pub(crate) fn flush_modal_shell_motion_subtree(root: &mut Element) -> bool {
    let mut needs_frame = arm_orphaned_modal_exits();
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let (shell, handle) = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                (
                    shell,
                    ModalPresenceHandle {
                        cell: Rc::clone(cell_rc),
                    },
                )
            };
            if flush_shell_state(root, &shell, &handle) {
                needs_frame = true;
            }
        }
    });
    if tick_modal_enter_css_subtree(root) {
        needs_frame = true;
    }
    needs_frame
}

/// Keep a hoisted modal overlay/stage after the viewport-modal island is disposed.
pub(crate) fn hoisted_should_retain(el: &Element) -> bool {
    PRESENCE_REGISTRY.with(|reg| {
        reg.borrow().iter().any(|cell| {
            let c = cell.borrow();
            if !c.gate.is_mounted() {
                return false;
            }
            let Some(id) = c.shell.as_ref().map(|s| s.modal_id.as_str()) else {
                return false;
            };
            subtree_belongs_to_modal(el, id)
        })
    })
}

fn subtree_belongs_to_modal(el: &Element, modal_id: &str) -> bool {
    let overlay = format!("{modal_id}__modal_overlay");
    let panel = format!("{modal_id}__modal_panel");
    let zoom = format!("{modal_id}__modal_zoom");
    let mut stack: Vec<&Element> = vec![el];
    while let Some(n) = stack.pop() {
        if let Some(tid) = n.template_id.as_deref() {
            if tid == overlay || tid == panel || tid == zoom {
                return true;
            }
        }
        for c in n.children.iter().rev() {
            stack.push(c);
        }
    }
    false
}

/// Register shell motion (only write presence + `template_id`; write live tree by flush after hoist).
pub(crate) fn apply_modal_shell_motion(
    modal_id: &str,
    overlay: &mut Element,
    panel: &mut Element,
    motion: ModalSurfaceMotion,
    phase: PresencePhase,
    presence: &ModalPresenceHandle,
    extra_translate: [f32; 2],
    dragging: bool,
    slide_axis_px: f32,
) -> bool {
    overlay.template_id = Some(format!("{modal_id}__modal_overlay"));
    panel.template_id = Some(format!("{modal_id}__modal_panel"));
    let slide_axis_px = slide_axis_for_panel(motion, panel, slide_axis_px);
    let needs_frame = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let (extra_translate, dragging, live_open_frac) = if dragging {
            if let Some(s) = c.shell.as_ref() {
                if s.modal_id == modal_id && s.dragging {
                    (s.extra_translate, true, s.live_open_frac)
                } else {
                    (
                        extra_translate,
                        true,
                        open_frac_from_extra(motion, extra_translate, slide_axis_px),
                    )
                }
            } else {
                (
                    extra_translate,
                    true,
                    open_frac_from_extra(motion, extra_translate, slide_axis_px),
                )
            }
        } else {
            (extra_translate, false, 1.0)
        };
        c.shell = Some(ModalShellState {
            modal_id: modal_id.to_string(),
            motion,
            phase,
            extra_translate,
            dragging,
            slide_axis_px,
            live_open_frac,
        });
        // 构建树默认 opacity=1；登记时写 shadcn 入场初态，flush 再 seed `@keyframes`。
        if needs_enter && matches!(phase, PresencePhase::Present) && !dragging {
            overlay.style.opacity = 0.0;
            if motion == ModalSurfaceMotion::DialogFade {
                // Panel fades with the overlay, so hide the zoom layer here too — otherwise a
                // frame that paints before flush shows the panel opaque at 90%.
                if let Some(zoom) = find_template_id_mut(panel, &dialog_zoom_tid(modal_id)) {
                    zoom.style.opacity = 0.0;
                    zoom.style.scale = [DIALOG_ENTER_SCALE, DIALOG_ENTER_SCALE];
                }
            } else {
                // Same for Sheet/Drawer: flush is after hoist. Without a closed pose here
                // the first paint is the docked sheet, then flush jumps it off-screen.
                panel.style.translate = [0.0, 0.0];
                panel.style.translate_pct = motion.closed_translate_pct();
            }
        }
        dragging
            || matches!(phase, PresencePhase::Present | PresencePhase::Exiting)
            || c.enter_active
            || needs_enter
            || (motion == ModalSurfaceMotion::DialogFade
                && matches!(phase, PresencePhase::Exiting | PresencePhase::Present))
    };
    needs_frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_effect, create_root};
    use crate::style::{set_css_transition_timeline_ms, set_css_transition_virtual_timeline};
    use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage};
    use std::time::{Duration, Instant};

    fn attach_dialog_zoom_shell(modal_id: &str, panel: &mut Element) {
        panel.template_id = Some(format!("{modal_id}__modal_panel"));
        let mut zoom = Element::new();
        zoom.template_id = Some(dialog_zoom_tid(modal_id));
        panel.children.push(zoom);
    }

    #[test]
    fn dialog_open_arms_event_loop_wake_before_first_tick() {
        create_root(|| {
            set_modal_css_needs_frame_for_test(false);
            let _ = take_modal_force_immediate_wake();
            let handle = use_modal_presence("wake-before-tick");
            assert!(
                !any_modal_css_needs_frame(),
                "precondition: idle CSS pump must be off"
            );
            sync_modal_presence(&handle, true);
            assert!(
                handle.cell.borrow().needs_enter,
                "open must set needs_enter"
            );
            assert!(
                any_modal_css_needs_frame(),
                "open must arm about_to_wait before flush/tick"
            );
            assert!(
                take_modal_force_immediate_wake(),
                "open must force one Poll so caret/long-press WaitUntil (~500ms) is not the next wake"
            );
            assert!(
                !take_modal_force_immediate_wake(),
                "immediate wake is one-shot; later frames Poll via any_modal_css_needs_frame"
            );
            assert!(
                any_modal_css_needs_frame(),
                "CSS pump must stay armed after the one-shot is consumed"
            );
        });
    }

    #[test]
    fn sync_modal_presence_survives_mounted_effect_reentry() {
        use std::cell::Cell;
        use std::rc::Rc;
        create_root(|| {
            let handle = use_modal_presence("reentry-test");
            let mounted = mounted_signal(&handle);
            let runs = Rc::new(Cell::new(0u32));
            let runs_eff = Rc::clone(&runs);
            create_effect(move || {
                let _ = mounted.get();
                runs_eff.set(runs_eff.get() + 1);
            });
            assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
            assert!(mounted.get());
            assert_eq!(
                runs.get(),
                2,
                "opening must notify mounted so the paint island (which tracks mounted, not open) mounts"
            );
            assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
            assert_eq!(runs.get(), 2, "already-open sync must not remount");
            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            assert!(mounted.get(), "Exiting stays mounted");
            assert_eq!(
                runs.get(),
                2,
                "close must not notify mounted; that rebuild is the exit hitch"
            );
        });
    }

    #[test]
    fn dialog_enter_seeds_keyframes_once() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("dlg-enter");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
            assert!(
                overlay
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == "fade-in-0"),
                "overlay must run fade-in keyframes"
            );
            assert!(
                handle.cell.borrow().enter_active,
                "Dialog enter must keep enter_active like Sheet so flush keeps requesting frames"
            );
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
            assert!(
                overlay
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == "fade-in-0"),
                "second flush must not drop enter keyframes mid-flight"
            );
        });
    }

    #[test]
    fn dialog_fade_flush_falls_back_to_panel_without_zoom() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.template_id = Some("dlg-fb__modal_panel".into());
            let handle = use_modal_presence("dlg-fb");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-fb",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let overlay = find_template_id_mut(&mut root, "dlg-fb__modal_overlay").unwrap();
            assert!(
                overlay
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == "fade-in-0"),
                "missing __modal_zoom must still seed overlay fade so the scrim is not stuck at 0"
            );
        });
    }

    #[test]
    fn apply_registers_shell_flush_bootstraps_enter() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("build-enter");
            sync_modal_presence(&handle, true);
            assert!(apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            ));
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let o_ptr =
                find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
            let p_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (overlay, zoom) = unsafe { (&*o_ptr, &*p_ptr) };
            assert!(
                overlay.style.opacity < 0.05,
                "flush must start overlay faded, got {}",
                overlay.style.opacity
            );
            assert!(overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"));
            assert!(zoom.style.animations.iter().any(|a| a.name == "zoom-in-90"));
            assert!(
                zoom.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "panel must fade together with the overlay (shadcn `fade-in-0 zoom-in-90`)"
            );
            assert!(
                (zoom.style.scale[0] - DIALOG_ENTER_SCALE).abs() < 0.02,
                "zoom layer starts at zoom-in-90, got {}",
                zoom.style.scale[0]
            );
            assert!(
                zoom.style.opacity < 0.05,
                "panel must start invisible, not as an opaque 95% render, got {}",
                zoom.style.opacity
            );
        });
    }

    #[test]
    fn dialog_enter_settles_and_clears_needs_enter() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("enter-settle");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(!handle.cell.borrow().needs_enter);
            for _ in 0..30 {
                let o_ptr =
                    find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
                let z_ptr =
                    find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
                let (o, z) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
                let _ = o.tick_css_animations(0.016);
                let _ = z.tick_css_animations(0.016);
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
            assert!(
                (overlay.style.opacity - 1.0).abs() < 0.05,
                "enter animation should settle to opaque, got {}",
                overlay.style.opacity
            );
            assert!(
                !handle.cell.borrow().enter_active,
                "settled Dialog must drop enter_active so the redraw pump stops"
            );
        });
    }

    #[test]
    fn dialog_second_flush_does_not_reseed_enter_keyframes() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("enter-progress");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let elapsed_before = find_template_id_mut(&mut root, "dlg__modal_overlay")
                .unwrap()
                .animation_snapshot
                .runs
                .first()
                .map(|r| r.elapsed_ms)
                .unwrap_or(0.0);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let elapsed_after = find_template_id_mut(&mut root, "dlg__modal_overlay")
                .unwrap()
                .animation_snapshot
                .runs
                .first()
                .map(|r| r.elapsed_ms)
                .unwrap_or(0.0);
            assert!(
                (elapsed_after - elapsed_before).abs() < 1.0,
                "second flush must not restart fade-in, before={elapsed_before} after={elapsed_after}"
            );
        });
    }

    #[test]
    fn dialog_enter_recreated_nodes_seek_enter_started() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("enter-seek");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            handle.cell.borrow_mut().enter_started =
                Some(Instant::now() - Duration::from_millis(80));

            let mut overlay2 = Element::new();
            let mut panel2 = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel2);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay2,
                &mut panel2,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.clear();
            root.children.push(overlay2);
            root.children.push(panel2);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let _ = tick_hoisted_modal_css(&mut root);
            let zoom = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap();
            assert!(
                zoom.style.opacity > 0.12,
                "recreated zoom must seek mid-enter, not restart at 0%; got {}",
                zoom.style.opacity
            );
            assert!(
                zoom.style.scale[0] > 0.92,
                "recreated zoom must seek scale past 0.90; got {}",
                zoom.style.scale[0]
            );
        });
    }

    #[test]
    fn flush_finds_shell_after_portal_hoist() {
        use crate::app::portal_overlay::{
            clear_overlay_stack, portal_sync, OverlayKind, OverlayMode,
        };
        use crate::layout::layout_tree::hoist_portal_subtrees;
        use crate::primitives::overlay::overlay;

        clear_overlay_stack();
        create_root(|| {
            let open = use_signal("dlg-open", || true);
            let handle = use_modal_presence("gal-dialog");
            sync_modal_presence(&handle, true);
            let Some(zs) =
                portal_sync("gal-dialog", open, OverlayKind::Modal, OverlayMode::Stacked)
            else {
                panic!("portal_sync");
            };

            let mut overlay_el = overlay(zs.underlay).color([0.0, 0.0, 0.0, 0.5]).into();
            let mut panel = Element::new().w(320.0).h(200.0).bg([1.0, 1.0, 1.0, 1.0]);
            attach_dialog_zoom_shell("gal-dialog", &mut panel);
            apply_modal_shell_motion(
                "gal-dialog",
                &mut overlay_el,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            let stage = mount_viewport_portal_stage(
                crate::primitives::r#box::r#box()
                    .inset(0.0)
                    .w("100%")
                    .h("100%")
                    .items_center()
                    .justify_center()
                    .child(panel)
                    .into(),
                zs.panel,
            );
            let shell = mount_modal_layer(overlay_el, stage);

            let mut root = Element::new().w(800.0).h(600.0).child(shell);
            assert!(hoist_portal_subtrees(&mut root) >= 2);
            assert!(flush_modal_shell_motion_subtree(&mut root));

            let panel_ptr = find_template_id_mut(&mut root, "gal-dialog__modal_panel")
                .expect("hoisted panel must keep template_id")
                as *mut Element;
            let zoom_ptr = find_template_id_mut(&mut root, "gal-dialog__modal_zoom")
                .expect("hoisted zoom layer must exist") as *mut Element;
            let (panel, zoom) = unsafe { (&*panel_ptr, &*zoom_ptr) };
            assert!(
                (panel.style.opacity - 1.0).abs() < 0.01,
                "panel shell must stay opaque, got {}",
                panel.style.opacity
            );
            assert!(
                (panel.style.scale[0] - 1.0).abs() < 0.01,
                "panel shell must not scale (layout stable), got {}",
                panel.style.scale[0]
            );
            assert!(
                zoom.style.scale[0] < 0.99,
                "zoom layer should start scaled down on enter, got {}",
                zoom.style.scale[0]
            );
        });
        clear_overlay_stack();
    }

    #[test]
    fn dialog_exit_finish_defers_mounted_until_drain() {
        use crate::app::portal_overlay::{
            clear_overlay_stack, portal_sync, OverlayKind, OverlayMode,
        };

        create_root(|| {
            clear_overlay_stack();
            let handle = use_modal_presence("defer-unmount");
            sync_modal_presence(&handle, true);
            mounted_signal(&handle).set(true);
            let mounted = mounted_signal(&handle);
            let _ = portal_sync("dlg", mounted, OverlayKind::Modal, OverlayMode::Stacked);
            assert!(mounted.peek());

            finish_dialog_exit_presence(&handle, "dlg");
            assert!(
                mounted.peek(),
                "mounted must stay true until drain (AppState may be borrowed during flush)"
            );
            drain_pending_modal_unmounts();
            assert!(!mounted.peek());
            assert_eq!(
                crate::app::portal_overlay::overlay_stack_depth_for_test(),
                0
            );
        });
    }

    /// The reported artifact: the panel used to render opaque at 95% and then snap to 100%,
    /// out of step with the overlay. Overlay and panel must fade together while the panel
    /// scales up.
    #[test]
    fn dialog_enter_fades_overlay_and_panel_together() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("enter-unified-fade");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));

            // ~40% through the 200ms enter.
            for _ in 0..5 {
                let o_ptr =
                    find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
                let z_ptr =
                    find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
                let (o, z) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
                let _ = o.tick_css_animations(0.016);
                let _ = z.tick_css_animations(0.016);
            }

            let o_ptr =
                find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (overlay, zoom) = unsafe { (&*o_ptr, &*z_ptr) };
            assert!(
                zoom.style.opacity > 0.05 && zoom.style.opacity < 0.95,
                "panel must be mid-fade, not opaque, got {}",
                zoom.style.opacity
            );
            assert!(
                (zoom.style.opacity - overlay.style.opacity).abs() < 0.1,
                "panel and overlay share one fade curve: overlay={} panel={}",
                overlay.style.opacity,
                zoom.style.opacity
            );
            assert!(
                zoom.style.scale[0] > DIALOG_ENTER_SCALE && zoom.style.scale[0] < 1.0,
                "panel must still be zooming while it fades, got {}",
                zoom.style.scale[0]
            );
        });
    }

    #[test]
    fn tick_hoisted_modal_css_exit_dirties_overlay_paint() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let _ = crate::app::damage::take_frame_paint_nodes();
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-dirty", &mut panel);
            let handle = use_modal_presence("exit-dirty");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-dirty",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            sync_modal_presence(&handle, false);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            {
                let o = find_template_id_mut(&mut root, "dlg-dirty__modal_overlay").unwrap();
                o.layout_bounds = [0.0, 0.0, 800.0, 600.0];
                let z = find_template_id_mut(&mut root, "dlg-dirty__modal_zoom").unwrap();
                z.layout_bounds = [200.0, 120.0, 400.0, 280.0];
            }
            let _ = crate::app::damage::take_frame_paint_nodes();
            assert!(tick_hoisted_modal_css(&mut root));
            let dirty = crate::app::damage::take_frame_paint_nodes();
            assert!(
                dirty.iter().any(|d| {
                    matches!(d.kind, crate::app::damage::PaintDirtyKind::Overlay)
                }),
                "exit CSS tick must dest-reset Overlay occupancy (PartialLoad otherwise freezes the last stamp), got {dirty:?}"
            );
        });
    }

    #[test]
    fn dialog_exit_opacity_keeps_moving_until_css_finishes() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-mono", &mut panel);
            let handle = use_modal_presence("exit-mono");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-mono",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            for _ in 0..20 {
                let _ = flush_modal_shell_motion_subtree(&mut root);
                let _ = tick_hoisted_modal_css(&mut root);
            }
            sync_modal_presence(&handle, false);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let mut last_opacity = 1.0_f32;
            let mut last_scale = 1.0_f32;
            let mut saw_mid = false;
            let mut plateau = 0u32;
            for i in 0..24 {
                let _ = flush_modal_shell_motion_subtree(&mut root);
                if !handle.cell.borrow().gate.is_mounted() {
                    assert!(
                        last_opacity < 0.05,
                        "unmount must wait until fade-out 100% sample, froze at {last_opacity} on tick {i}"
                    );
                    break;
                }
                let _ = tick_hoisted_modal_css(&mut root);
                let (opacity, scale) = {
                    let o_ptr = find_template_id_mut(&mut root, "dlg-mono__modal_overlay").unwrap()
                        as *mut Element;
                    let z_ptr = find_template_id_mut(&mut root, "dlg-mono__modal_zoom").unwrap()
                        as *mut Element;
                    let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                    (o.style.opacity, z.style.scale[0])
                };
                if opacity < 0.95 && opacity > 0.08 {
                    saw_mid = true;
                }
                assert!(
                    opacity <= last_opacity + 1e-4,
                    "exit overlay opacity must not rise, {last_opacity} → {opacity} at tick {i}"
                );
                assert!(
                    scale <= last_scale + 1e-4,
                    "exit zoom scale must not rise, {last_scale} → {scale} at tick {i}"
                );
                if (last_opacity - opacity).abs() < 1e-5 && opacity > 0.08 {
                    plateau += 1;
                    assert!(
                        plateau < 3,
                        "exit opacity froze at {opacity} while still mounted (tick {i})"
                    );
                } else {
                    plateau = 0;
                }
                last_opacity = opacity;
                last_scale = scale;
            }
            assert!(
                saw_mid,
                "exit must pass through mid opacities, not skip to unmount"
            );
        });
    }

    #[test]
    fn dialog_exit_finish_tick_dirties_end_pose() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let _ = crate::app::damage::take_frame_paint_nodes();
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-end", &mut panel);
            let handle = use_modal_presence("exit-end");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-end",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            for _ in 0..20 {
                let _ = flush_modal_shell_motion_subtree(&mut root);
                let _ = tick_hoisted_modal_css(&mut root);
            }
            sync_modal_presence(&handle, false);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            {
                let o = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap();
                o.layout_bounds = [0.0, 0.0, 800.0, 600.0];
                let z = find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap();
                z.layout_bounds = [200.0, 120.0, 400.0, 280.0];
            }
            let mut saw_finish = false;
            for i in 0..24 {
                let before = {
                    let o_ptr = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap()
                        as *mut Element;
                    let z_ptr = find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap()
                        as *mut Element;
                    let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                    dialog_css_animating(o, z)
                };
                let _ = crate::app::damage::take_frame_paint_nodes();
                let _ = tick_hoisted_modal_css(&mut root);
                let (after, opacity) = {
                    let o_ptr = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap()
                        as *mut Element;
                    let z_ptr = find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap()
                        as *mut Element;
                    let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                    (dialog_css_animating(o, z), o.style.opacity)
                };
                let dirty = crate::app::damage::take_frame_paint_nodes();
                if before && !after {
                    saw_finish = true;
                    assert!(
                        opacity < 0.05,
                        "Finished fill must write fade-out 100% this tick, got {opacity} at {i}"
                    );
                    assert!(
                        dirty.iter().any(|d| {
                            matches!(d.kind, crate::app::damage::PaintDirtyKind::Overlay)
                        }),
                        "Finished sample must dest-reset Overlay (ease-in tail otherwise freezes), got {dirty:?}"
                    );
                    assert!(
                        handle.cell.borrow().gate.is_mounted(),
                        "unmount is the next flush, not the finishing tick"
                    );
                    break;
                }
            }
            assert!(saw_finish, "exit keyframes must reach Finished");
            let _ = flush_modal_shell_motion_subtree(&mut root);
            assert!(
                !handle.cell.borrow().gate.is_mounted(),
                "next flush unmounts after the end pose has been dirtied"
            );
        });
    }

    #[test]
    fn tick_hoisted_modal_css_keeps_redraw_flag_while_dialog_enter_plays() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-pump", &mut panel);
            let handle = use_modal_presence("enter-pump");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-pump",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(
                tick_hoisted_modal_css(&mut root),
                "enter keyframes must still need frames after the first tick"
            );
            assert!(
                any_modal_css_needs_frame(),
                "redraw pump must see Dialog CSS as active (not only cubic tweens)"
            );
        });
    }

    #[test]
    fn tick_hoisted_modal_css_first_frame_leaves_seed_without_settling() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-hold", &mut panel);
            let handle = use_modal_presence("enter-hold");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-hold",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(tick_hoisted_modal_css(&mut root));
            let zoom = find_template_id_mut(&mut root, "dlg-hold__modal_zoom").unwrap();
            assert!(
                zoom.style.scale[0] > DIALOG_ENTER_SCALE + 0.002,
                "first present after click must leave the 0.90 seed pose, got {}",
                zoom.style.scale[0]
            );
            assert!(
                zoom.style.opacity > 0.01 && zoom.style.opacity < 0.5,
                "first present must already be fading in, not blank or settled, got {}",
                zoom.style.opacity
            );
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            assert!(tick_hoisted_modal_css(&mut root));
            let zoom = find_template_id_mut(&mut root, "dlg-hold__modal_zoom").unwrap();
            let elapsed = zoom
                .animation_snapshot
                .runs
                .iter()
                .map(|r| r.elapsed_ms)
                .fold(0.0_f32, f32::max);
            assert!(
                elapsed > 20.0,
                "second tick must consume frame dt, elapsed_ms={elapsed}"
            );
        });
    }

    #[test]
    fn tick_hoisted_modal_css_idle_jump_does_not_settle_enter() {
        create_root(|| {
            crate::style::utility::set_frame_dt(2.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-idle", &mut panel);
            let handle = use_modal_presence("enter-idle");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-idle",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(tick_hoisted_modal_css(&mut root));
            let overlay = find_template_id_mut(&mut root, "dlg-idle__modal_overlay").unwrap();
            assert!(
                overlay.style.opacity > 0.01 && overlay.style.opacity < 0.35,
                "idle click must paint one 16ms step, not a blank seed or a settled dialog, got {}",
                overlay.style.opacity
            );
        });
    }

    #[test]
    fn dialog_exit_seeds_fade_and_zoom_out() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg", &mut panel);
            let handle = use_modal_presence("exit-anim");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            sync_modal_presence(&handle, false);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let o_ptr =
                find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (overlay, zoom) = unsafe { (&*o_ptr, &*z_ptr) };
            assert!(overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"));
            assert!(
                zoom.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "panel must fade out with the overlay"
            );
            assert!(
                zoom.style
                    .animations
                    .iter()
                    .any(|a| a.name == "zoom-out-90"),
                "exit must mirror enter (shadcn `fade-out-0 zoom-out-90`)"
            );
            // Exit starts from the settled pose; the keyframes take it down from here.
            assert!((zoom.style.scale[0] - 1.0).abs() < 0.01);
            assert!((zoom.style.opacity - 1.0).abs() < 0.01);
        });
    }

    #[test]
    fn dialog_exit_hold_seed_does_not_seek_stale_enter_elapsed() {
        create_root(|| {
            crate::style::utility::set_frame_dt(1.0 / 60.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            attach_dialog_zoom_shell("dlg-exit-seek", &mut panel);
            let handle = use_modal_presence("exit-seek");
            sync_modal_presence(&handle, true);
            apply_modal_shell_motion(
                "dlg-exit-seek",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::DialogFade,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                800.0,
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));

            sync_modal_presence(&handle, false);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            // Action-click rebuild: snapshots dropped while `enter_started` still
            // points at open time (hundreds of ms ago).
            {
                let mut c = handle.cell.borrow_mut();
                c.enter_started = Some(Instant::now() - Duration::from_millis(800));
            }
            let o_ptr = find_template_id_mut(&mut root, "dlg-exit-seek__modal_overlay").unwrap()
                as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg-exit-seek__modal_zoom").unwrap()
                as *mut Element;
            {
                let (overlay, zoom) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
                overlay.animation_snapshot = Default::default();
                zoom.animation_snapshot = Default::default();
            }
            let _ = tick_hoisted_modal_css(&mut root);
            let overlay = unsafe { &*o_ptr };
            assert!(
                overlay.style.opacity > 0.7,
                "exit hold_seed must not seek enter elapsed (would finish fade in one tick), got {}",
                overlay.style.opacity
            );
            assert!(
                overlay
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == "fade-out-0"),
                "exit keyframes must still be playing"
            );
        });
    }

    #[test]
    fn slide_enter_does_not_settle_while_panel_offscreen() {
        create_root(|| {
            let mut overlay = Element::new();
            overlay.style.opacity = 1.0;
            let mut panel = Element::new();
            panel.style.opacity = 1.0;
            panel.style.translate = [0.0, 400.0];
            panel.style.translate_pct = [0.0, 1.0];
            assert!(
                !enter_visual_settled(
                    &overlay,
                    &panel,
                    ModalSurfaceMotion::SlideFromBottom,
                    [0.0, 0.0],
                    400.0,
                ),
                "opacity=1 must not settle a slide that is still translated"
            );
            panel.style.translate = [0.0, 0.0];
            panel.style.translate_pct = [0.0, 0.0];
            assert!(enter_visual_settled(
                &overlay,
                &panel,
                ModalSurfaceMotion::SlideFromBottom,
                [0.0, 0.0],
                400.0,
            ));
            // 4% of a 400px sheet is 16px. Nailing that remainder in one frame is the
            // reported end-of-enter snap (Dialog already tightened zoom to 0.002).
            panel.style.translate_pct = [0.0, 0.03];
            assert!(
                !enter_visual_settled(
                    &overlay,
                    &panel,
                    ModalSurfaceMotion::SlideFromBottom,
                    [0.0, 0.0],
                    400.0,
                ),
                "3% remaining travel must not count as settled (that nail is the end-of-enter jump)"
            );
        });
    }

    #[test]
    fn slide_closed_target_ignores_drag_extra() {
        let closed = panel_motion_target(
            ModalSurfaceMotion::SlideFromBottom,
            false,
            1.0,
            [0.0, 200.0],
            800.0,
        );
        assert!(
            (closed.translate[1] - 800.0).abs() < 0.01,
            "closed must be full off-screen, not closed+extra; got {}",
            closed.translate[1]
        );
    }

    #[test]
    fn slide_closed_translate_is_off_the_dock_edge() {
        let bottom = ModalSurfaceMotion::SlideFromBottom.closed_translate(300.0);
        assert_eq!(bottom, [0.0, 300.0], "bottom sheet hides downward");
        let top = ModalSurfaceMotion::SlideFromTop.closed_translate(300.0);
        assert_eq!(top, [0.0, -300.0], "top sheet hides upward");
        let left = ModalSurfaceMotion::SlideFromLeft.closed_translate(240.0);
        assert_eq!(left, [-240.0, 0.0], "left sheet hides leftward");
        let right = ModalSurfaceMotion::SlideFromRight.closed_translate(240.0);
        assert_eq!(right, [240.0, 0.0], "right sheet hides rightward");
    }

    #[test]
    fn slide_axis_prefers_laid_out_panel_size() {
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 500.0, 800.0, 280.0];
        let axis = slide_axis_for_panel(ModalSurfaceMotion::SlideFromBottom, &panel, 900.0);
        assert!(
            (axis - 280.0).abs() < 0.01,
            "enter/exit travel should be panel height, got {axis}"
        );
        panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
        let fallback = slide_axis_for_panel(ModalSurfaceMotion::SlideFromBottom, &panel, 900.0);
        assert!((fallback - 900.0).abs() < 0.01);
    }

    #[test]
    fn slide_enter_flush_seeds_closed_translate() {
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            let handle = use_modal_presence("slide-enter");
            sync_modal_presence(&handle, true);
            assert!(apply_modal_shell_motion(
                "slide",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            ));
            assert!(
                panel.style.translate_pct[1].abs() > 0.9,
                "apply must hide the sheet off-screen before flush, otherwise the first paint is the dock"
            );
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let o_ptr =
                find_template_id_mut(&mut root, "slide__modal_overlay").unwrap() as *mut Element;
            let p_ptr =
                find_template_id_mut(&mut root, "slide__modal_panel").unwrap() as *mut Element;
            let (overlay, panel) = unsafe { (&*o_ptr, &*p_ptr) };
            assert!(
                overlay.style.opacity < 0.05,
                "overlay enter mid should start faded, got {}",
                overlay.style.opacity
            );
            assert!(
                overlay
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == "fade-in-0"),
                "slide overlay enter must use fade-in-0 keyframes (rebuild-proof)"
            );
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-in-")),
                "slide panel enter must use translate_pct keyframes, not a pixel jump"
            );
            assert!(
                panel.style.translate_pct[1].abs() > 0.9,
                "slide enter must start off-screen at 100%, got pct={}",
                panel.style.translate_pct[1]
            );
            assert!(
                panel.style.translate[1].abs() < 8.0,
                "enter travel is translate_pct, not viewport pixels, got ty={}",
                panel.style.translate[1]
            );
            assert!(handle.cell.borrow().enter_active);
        });
    }

    #[test]
    fn slide_enter_does_not_snap_the_ease_out_tail() {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 200.0, 400.0, 400.0];
            let handle = use_modal_presence("slide-enter-tail");
            sync_modal_presence(&handle, true);
            assert!(apply_modal_shell_motion(
                "slide-tail",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                400.0,
            ));
            root.children.push(overlay);
            root.children.push(panel);
            assert!(flush_modal_shell_motion_subtree(&mut root));

            let mut nail_from = None;
            for _ in 0..40 {
                let pct = {
                    let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                    p.style.translate_pct[1]
                };
                let had_enter = {
                    let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                    p.style
                        .animations
                        .iter()
                        .any(|a| a.name.starts_with("rui-modal-slide-in-"))
                };
                let _ = flush_modal_shell_motion_subtree(&mut root);
                let _ = tick_hoisted_modal_css(&mut root);
                let has_enter = {
                    let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                    p.style
                        .animations
                        .iter()
                        .any(|a| a.name.starts_with("rui-modal-slide-in-"))
                };
                if had_enter && !has_enter {
                    nail_from = Some(pct);
                    break;
                }
            }
            let from = nail_from.expect("enter keyframes must clear after the slide finishes");
            assert!(
                from.abs() < 0.005,
                "nailing enter while {from} remaining (~{}px on a 400px panel) is the end-of-enter snap",
                from.abs() * 400.0
            );
        });
        crate::style::utility::set_frame_dt(1.0 / 60.0);
    }

    #[test]
    fn slide_exit_keeps_needs_exit_anim_until_flush() {
        create_root(|| {
            set_css_transition_virtual_timeline(true);
            set_css_transition_timeline_ms(0.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            let handle = use_modal_presence("slide-flag");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-flag",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                400.0,
            );
            overlay.style.opacity = 1.0;
            panel.style.translate = [0.0, 0.0];
            root.children.push(overlay);
            root.children.push(panel);

            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            {
                let o_ptr = find_template_id_mut(&mut root, "slide-flag__modal_overlay").unwrap()
                    as *mut Element;
                let p_ptr = find_template_id_mut(&mut root, "slide-flag__modal_panel").unwrap()
                    as *mut Element;
                let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
                apply_modal_shell_motion(
                    "slide-flag",
                    o,
                    p,
                    ModalSurfaceMotion::SlideFromRight,
                    PresencePhase::Exiting,
                    &handle,
                    [0.0, 0.0],
                    false,
                    400.0,
                );
            }
            assert!(
                handle.cell.borrow().needs_exit_anim,
                "apply only registers; consuming the flag here skips the post-hoist slide seed"
            );
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(!handle.cell.borrow().needs_exit_anim);
            let ov = find_template_id_mut(&mut root, "slide-flag__modal_overlay").unwrap();
            assert!(
                ov.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "flush after apply must seed overlay fade-out"
            );
            let p = find_template_id_mut(&mut root, "slide-flag__modal_panel").unwrap();
            assert!(
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-out-")),
                "flush after apply must seed panel slide-out keyframes"
            );
            set_css_transition_virtual_timeline(false);
        });
    }

    #[test]
    fn slide_overlay_exit_clamps_idle_frame_dt() {
        crate::style::utility::set_frame_dt(2.0);
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            let handle = use_modal_presence("slide-idle-dt");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-idle",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                400.0,
            );
            overlay.style.opacity = 1.0;
            overlay.style.animations.clear();
            panel.style.translate = [0.0, 0.0];
            root.children.push(overlay);
            root.children.push(panel);
            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            {
                let o_ptr = find_template_id_mut(&mut root, "slide-idle__modal_overlay").unwrap()
                    as *mut Element;
                let p_ptr = find_template_id_mut(&mut root, "slide-idle__modal_panel").unwrap()
                    as *mut Element;
                let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
                apply_modal_shell_motion(
                    "slide-idle",
                    o,
                    p,
                    ModalSurfaceMotion::SlideFromRight,
                    PresencePhase::Exiting,
                    &handle,
                    [0.0, 0.0],
                    false,
                    400.0,
                );
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let _ = tick_hoisted_modal_css(&mut root);
            let ov = find_template_id_mut(&mut root, "slide-idle__modal_overlay").unwrap();
            assert!(
                ov.style.opacity > 0.15 && animation_run_active(&ov.style, &ov.animation_snapshot),
                "idle frame_dt must not finish fade-out-0 in one tick, got {}",
                ov.style.opacity
            );
            let panel = find_template_id_mut(&mut root, "slide-idle__modal_panel").unwrap();
            assert!(
                panel.style.translate_pct[0] > 0.02 && panel.style.translate_pct[0] < 0.35,
                "idle frame_dt must sample the slide-out middle, not skip to 100%, got pct={}",
                panel.style.translate_pct[0]
            );
            let visual_tx = panel.layout_bounds[2] * panel.style.translate_pct[0];
            assert!(
                visual_tx < panel.layout_bounds[2] * 0.4,
                "travel must be a fraction of the panel, not the viewport fallback, got {visual_tx}"
            );
        });
        crate::style::utility::set_frame_dt(1.0 / 60.0);
    }

    #[test]
    fn slide_exit_second_flush_does_not_restart_tween() {
        create_root(|| {
            set_css_transition_virtual_timeline(true);
            set_css_transition_timeline_ms(0.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            let handle = use_modal_presence("slide-exit");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-x",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            );
            overlay.style.opacity = 1.0;
            panel.style.translate = [0.0, 0.0];
            root.children.push(overlay);
            root.children.push(panel);

            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            {
                let o_ptr = find_template_id_mut(&mut root, "slide-x__modal_overlay").unwrap()
                    as *mut Element;
                let p_ptr = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap()
                    as *mut Element;
                let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
                apply_modal_shell_motion(
                    "slide-x",
                    o,
                    p,
                    ModalSurfaceMotion::SlideFromBottom,
                    PresencePhase::Exiting,
                    &handle,
                    [0.0, 0.0],
                    false,
                    600.0,
                );
            }
            assert!(flush_modal_shell_motion_subtree(&mut root));
            assert!(
                !handle.cell.borrow().needs_exit_anim,
                "exit bootstrap must run only once"
            );
            {
                let o = find_template_id_mut(&mut root, "slide-x__modal_overlay").unwrap();
                assert!(
                    o.style.animations.iter().any(|a| a.name == "fade-out-0"),
                    "slide overlay exit must use fade-out-0 keyframes so blur fades with the panel"
                );
            }
            let seed = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
            let ty_seed = seed.style.translate[1];
            let pct_seed = seed.style.translate_pct[1];
            assert!(
                ty_seed < 80.0 && pct_seed.abs() < 0.08,
                "first exit sample must stay near the open pose, got ty={ty_seed} pct={pct_seed}"
            );
            {
                let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
                assert!(
                    p.style
                        .animations
                        .iter()
                        .any(|a| a.name.starts_with("rui-modal-slide-out-")),
                    "exit must install slide-out keyframes"
                );
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let again = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
            let ty_again = again.style.translate[1];
            let pct_again = again.style.translate_pct[1];
            assert!(
                ty_again < 80.0 && pct_again.abs() < 0.08,
                "later flushes must not SnapAfterEnsure the closed pose, got ty={ty_again} pct={pct_again}"
            );
            {
                let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
                for _ in 0..12 {
                    let _ = tick_element_css(p, 0.08);
                }
            }
            let end = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
            set_css_transition_virtual_timeline(false);
            assert!(
                end.style.translate_pct[1] > 0.9,
                "exit keyframes must be able to finish after a second flush, got pct={}",
                end.style.translate_pct[1]
            );
        });
    }

    #[test]
    fn slide_exit_reseeds_transform_when_rebuild_drops_transition() {
        create_root(|| {
            set_css_transition_virtual_timeline(true);
            set_css_transition_timeline_ms(0.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            let handle = use_modal_presence("slide-reseed");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-reseed",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            );
            overlay.style.opacity = 1.0;
            panel.style.translate = [0.0, 0.0];
            root.children.push(overlay);
            root.children.push(panel);

            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            {
                let o_ptr = find_template_id_mut(&mut root, "slide-reseed__modal_overlay").unwrap()
                    as *mut Element;
                let p_ptr = find_template_id_mut(&mut root, "slide-reseed__modal_panel").unwrap()
                    as *mut Element;
                let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
                apply_modal_shell_motion(
                    "slide-reseed",
                    o,
                    p,
                    ModalSurfaceMotion::SlideFromBottom,
                    PresencePhase::Exiting,
                    &handle,
                    [0.0, 0.0],
                    false,
                    600.0,
                );
            }
            assert!(flush_modal_shell_motion_subtree(&mut root));

            let mut fresh = Element::new();
            fresh.template_id = Some("slide-reseed__modal_panel".into());
            fresh.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            fresh.style.translate = [0.0, 0.0];
            let idx = root
                .children
                .iter()
                .position(|c| c.template_id.as_deref() == Some("slide-reseed__modal_panel"))
                .expect("panel");
            root.children[idx] = fresh;
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let p = find_template_id_mut(&mut root, "slide-reseed__modal_panel").unwrap();
            assert!(
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-out-")),
                "reactive rebuild must re-seed panel slide-out, otherwise overlay fades and the sheet sticks at the dock"
            );
            assert!(
                p.style.translate[1].abs() < 80.0 && p.style.translate_pct[1].abs() < 0.08,
                "reseed must start from the open pose, got ty={} pct={}",
                p.style.translate[1],
                p.style.translate_pct[1]
            );
            set_css_transition_virtual_timeline(false);
        });
    }

    #[test]
    fn slide_exit_plays_inbetween_frames_not_viewport_jump() {
        crate::style::utility::set_frame_dt(2.0);
        create_root(|| {
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            // Typical right Sheet: 18rem panel, full viewport fallback.
            panel.layout_bounds = [0.0, 0.0, 288.0, 600.0];
            let handle = use_modal_presence("slide-mid");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-mid",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                1920.0,
            );
            overlay.style.opacity = 1.0;
            overlay.style.animations.clear();
            panel.style.translate = [0.0, 0.0];
            root.children.push(overlay);
            root.children.push(panel);
            assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
            {
                let o_ptr = find_template_id_mut(&mut root, "slide-mid__modal_overlay").unwrap()
                    as *mut Element;
                let p_ptr = find_template_id_mut(&mut root, "slide-mid__modal_panel").unwrap()
                    as *mut Element;
                let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
                apply_modal_shell_motion(
                    "slide-mid",
                    o,
                    p,
                    ModalSurfaceMotion::SlideFromRight,
                    PresencePhase::Exiting,
                    &handle,
                    [0.0, 0.0],
                    false,
                    1920.0,
                );
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let mut mids = Vec::new();
            for _ in 0..4 {
                let _ = tick_hoisted_modal_css(&mut root);
                let p = find_template_id_mut(&mut root, "slide-mid__modal_panel").unwrap();
                let visual = p.layout_bounds[2] * p.style.translate_pct[0];
                mids.push(visual);
            }
            let ov = find_template_id_mut(&mut root, "slide-mid__modal_overlay").unwrap();
            assert!(
                ov.style.opacity > 0.25 && ov.style.opacity < 0.98,
                "scrim must still be fading after a few frames, got {}",
                ov.style.opacity
            );
            assert!(
                mids.iter().any(|&x| x > 8.0 && x < 288.0 * 0.85),
                "exit must paint an in-between slide, not dock→off-screen; frames={mids:?}"
            );
            assert!(
                mids.iter().all(|&x| x < 288.0 * 0.95),
                "idle dt must not finish a 400ms slide in 8 frames, frames={mids:?}"
            );
        });
        crate::style::utility::set_frame_dt(1.0 / 60.0);
    }

    #[test]
    fn slide_settle_starts_from_drag_pose_not_closed() {
        create_root(|| {
            set_css_transition_virtual_timeline(true);
            set_css_transition_timeline_ms(0.0);
            let mut root = Element::new();
            let mut overlay = Element::new();
            let mut panel = Element::new();
            panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
            let handle = use_modal_presence("slide-settle");
            sync_modal_presence(&handle, true);
            {
                let mut c = handle.cell.borrow_mut();
                c.needs_enter = false;
                c.enter_active = false;
            }
            apply_modal_shell_motion(
                "slide-s",
                &mut overlay,
                &mut panel,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Present,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            );
            overlay.style.opacity = 1.0;
            overlay.style.transition.clear();
            overlay.style.animations.clear();
            panel.style.translate = [0.0, 0.0];
            panel.style.translate_pct = [0.0, 0.4];
            panel.style.transition.clear();
            root.children.push(overlay);
            root.children.push(panel);
            arm_modal_slide_settle("slide-s", [0.0, 240.0]);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let p = find_template_id_mut(&mut root, "slide-s__modal_panel").unwrap();
            let pct_y = p.style.translate_pct[1];
            assert!(
                pct_y > 0.15 && pct_y < 0.55,
                "release snap must continue from the drag pose, not re-enter from off-screen, got {pct_y}"
            );
            assert!(
                !p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-in-")),
                "snap must not seed enter keyframes"
            );
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let p = find_template_id_mut(&mut root, "slide-s__modal_panel").unwrap();
            assert!(
                p.style.translate_pct[1] > 0.1 && p.style.translate_pct[1] < 0.55,
                "second settle frame must not jump to closed 100%, got {}",
                p.style.translate_pct[1]
            );
            assert!(
                !p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("rui-modal-slide-in-")),
                "continuation must not re-seed enter from 100%"
            );
            set_css_transition_virtual_timeline(false);
        });
    }
}
