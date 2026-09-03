//! Dialog / Sheet / Drawer 共享模态动效：presence（退场保留挂载）+ 表面动画。
//!
//! **Dialog** 对齐 shadcn [`dialog.tsx`](https://ui.shadcn.com/docs/components/dialog)：
//! overlay `fade-in-0` / `fade-out-0`，面板 `fade-in-0 zoom-in-90` /
//! `fade-out-0 zoom-out-90` (fade symmetry), use CSS `@keyframes` ([`style::keyframes`]),
//! 不经 transition flush/tick 链路。遮罩 `backdrop-blur` 保持恒定目标半径，
//! 入场/退场的模糊渐显由渲染器按 overlay opacity 乘算（见 `AppState` backdrop pass）。
//!
//! Sheet / Drawer：面板 CSS transform transition；遮罩与 Dialog 一样走
//! `fade-in-0` / `fade-out-0` keyframes（重建不会把 opacity 钉死成 1）。
//! [`flush_modal_shell_motion_subtree`] 仍负责面板入/退场。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::portal_overlay::modal_stack_metrics;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::style::{
    animation_run_active, css_transition_needs_frame, keyframes_anim_entry,
    tick_element_transitions_locked, AnimationFillMode, TimingFunction, TransitionEntry,
    TransitionProperty,
};
use crate::utils::motion::{
    clear_motion, motion_is_animating, motion_sync_status, prepare_motion_sync, MotionBootstrap,
    MotionEpsilon, MotionStyle, MotionTransform,
};
use crate::utils::presence::{ExitGate, PresencePhase};

pub(crate) const MODAL_ENTER_MS: f32 = 250.0;
pub(crate) const MODAL_EXIT_MS: f32 = 200.0;
/// shadcn Drawer/Sheet popup: `duration-450 ease-[cubic-bezier(0.22,1,0.36,1)]`.
pub(crate) const SLIDE_ENTER_MS: f32 = 450.0;
/// shadcn ending-style ≈ `calc(swipe-strength * 400ms)` at full strength.
pub(crate) const SLIDE_EXIT_MS: f32 = 400.0;
/// shadcn Dialog enter — slightly longer than exit for a clearer reveal.
const DIALOG_ENTER_ANIM_MS: f32 = 300.0;
const DIALOG_EXIT_ANIM_MS: f32 = MODAL_EXIT_MS;
/// Cover Sheet/Drawer slide exit (Dialog uses CSS completion, not this wall clock).
const EXIT_DURATION: Duration = Duration::from_millis(SLIDE_EXIT_MS as u64 + 80);
/// shadcn `zoom-in-90` — a bit more travel than `zoom-in-95`.
const DIALOG_ENTER_SCALE: f32 = 0.90;
/// shadcn drawer popup easing.
const SLIDE_ENTER_EASE: TimingFunction = TimingFunction::CubicBezier(0.22, 1.0, 0.36, 1.0);
/// shadcn drawer overlay / ending easing.
const SLIDE_EXIT_EASE: TimingFunction = TimingFunction::CubicBezier(0.32, 0.72, 0.0, 1.0);

fn dialog_zoom_tid(modal_id: &str) -> String {
    format!("{modal_id}__modal_zoom")
}

/// 面板运动曲线（Dialog 淡入缩放；Sheet/Drawer 贴边滑入）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalSurfaceMotion {
    DialogFade,
    SlideFromTop,
    SlideFromBottom,
    SlideFromLeft,
    SlideFromRight,
}

impl ModalSurfaceMotion {
    pub(crate) fn from_sheet_side(side: crate::components::sheet::SheetSide) -> Self {
        use crate::components::sheet::SheetSide;
        match side {
            SheetSide::Top => Self::SlideFromTop,
            SheetSide::Right => Self::SlideFromRight,
            SheetSide::Bottom => Self::SlideFromBottom,
            SheetSide::Left => Self::SlideFromLeft,
        }
    }

    pub(crate) fn from_drawer_direction(dir: crate::components::drawer::DrawerDirection) -> Self {
        use crate::components::drawer::DrawerDirection;
        match dir {
            DrawerDirection::Down => Self::SlideFromBottom,
            DrawerDirection::Up => Self::SlideFromTop,
            DrawerDirection::Left => Self::SlideFromLeft,
            DrawerDirection::Right => Self::SlideFromRight,
        }
    }

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
    exit_armed: bool,
    /// The motion registered by the latest shell build (the only source of flush after hoist).
    shell: Option<ModalShellState>,
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
}

thread_local! {
    static PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<ModalPresenceCell>>>> =
        const { RefCell::new(Vec::new()) };
    /// `mounted.set` / portal stack pop deferred until AppState borrow is released.
    static PENDING_MODAL_UNMOUNTS: RefCell<Vec<(Signal<bool>, String)>> =
        const { RefCell::new(Vec::new()) };
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
            exit_armed: false,
            shell: None,
            needs_exit_anim: false,
            slide_settle: false,
            slide_settle_from: None,
        }));
        PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(Rc::clone(&rc)));
        rc
    });
    ModalPresenceHandle { cell: cell.peek() }
}

pub(crate) fn mounted_signal(handle: &ModalPresenceHandle) -> Signal<bool> {
    handle.cell.borrow().mounted
}

/// 同步 `open` 期望；返回当前相位。
pub(crate) fn sync_modal_presence(handle: &ModalPresenceHandle, want_open: bool) -> PresencePhase {
    let mounted_sig = mounted_signal(handle);
    let (phase, mounted, set_mounted) = {
        let mut c = handle.cell.borrow_mut();
        let was = c.gate.phase();
        let changed = c.gate.set_present(want_open, Instant::now());
        let phase = c.gate.phase();
        let mounted = c.gate.is_mounted();
        if want_open
            && matches!(phase, PresencePhase::Present)
            && (changed || was == PresencePhase::Absent)
        {
            c.needs_enter = true;
            c.enter_active = false;
            c.exit_armed = false;
            c.needs_exit_anim = false;
        }
        if matches!(phase, PresencePhase::Exiting) && !c.exit_armed {
            c.exit_armed = true;
            c.needs_enter = false;
            c.enter_active = false;
            c.needs_exit_anim = true;
        }
        let set_mounted = c.mounted.peek() != mounted;
        (phase, mounted, set_mounted)
    };
    // `mounted.set` 会同步 flush reactive_view；必须在释放 RefCell 后再写。
    if set_mounted {
        mounted_sig.set(mounted);
    }
    phase
}

/// 事件循环推进退场计时；返回是否仍需 redraw。
pub(crate) fn tick_modal_presence() -> bool {
    let now = Instant::now();
    let mut needs_frame = false;
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

fn panel_motion_target(
    motion: ModalSurfaceMotion,
    open_visual: bool,
    nested_scale: f32,
    extra_translate: [f32; 2],
    slide_axis_px: f32,
) -> MotionTransform {
    if open_visual {
        return MotionTransform {
            translate: extra_translate,
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
        opacity: if open_visual { 1.0 } else { 0.0 },
        scale: [1.0, 1.0],
    }
}

fn find_template_id_mut<'a>(el: &'a mut Element, want: &str) -> Option<&'a mut Element> {
    let want = want.replace('\\', "/").to_ascii_lowercase();
    if el
        .template_id
        .as_deref()
        .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
    {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(hit) = find_template_id_mut(child, &want) {
            return Some(hit);
        }
    }
    None
}

fn seed_modal_shell_transition_mid(el: &mut Element) {
    if el.style.transition.is_empty() {
        return;
    }
    let _ = tick_element_transitions_locked(
        &mut el.style,
        &mut el.transition_snapshot,
        el.animation_snapshot.lock(),
        0.0,
    );
}

fn enter_visual_settled(
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
) -> bool {
    let overlay_ok = (overlay.style.opacity - 1.0).abs() < 0.02;
    let nested = 1.0_f32;
    let panel_ok = if motion == ModalSurfaceMotion::DialogFade {
        (panel.style.scale[0] - nested).abs() < 0.02
    } else {
        // Slide panels keep opacity=1 for the whole enter — never use opacity as settle.
        let dx = (panel.style.translate[0] - target_translate[0]).abs();
        let dy = (panel.style.translate[1] - target_translate[1]).abs();
        dx < 2.0 && dy < 2.0
    };
    overlay_ok && panel_ok
}

fn needs_hoist_enter_seed(
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    enter_was_active: bool,
    needs_enter: bool,
    open_visual: bool,
    exiting: bool,
    target_translate: [f32; 2],
) -> bool {
    if !(enter_was_active && !needs_enter && open_visual && !exiting) {
        return false;
    }
    // Slide: hoist wiped mid → empty transitions while enter_active; re-seed closed→open.
    if motion != ModalSurfaceMotion::DialogFade
        && overlay.style.transition.is_empty()
        && panel.style.transition.is_empty()
        && !motion_is_animating(&overlay.style, &overlay.transition_snapshot)
        && !motion_is_animating(&panel.style, &panel.transition_snapshot)
    {
        return true;
    }
    // Dialog / generic: hoist after mid lost looks fully open with no tweens.
    enter_was_active
        && !needs_enter
        && open_visual
        && !exiting
        // hoist 后 fresh 节点默认 opacity≈1 / scale≈1，mid-flight 的 0.42 不应触发 re-seed。
        && overlay.style.opacity > 0.95
        && panel.style.scale[0] > 0.995
        && !motion_is_animating(&overlay.style, &overlay.transition_snapshot)
        && !motion_is_animating(&panel.style, &panel.transition_snapshot)
        && !enter_visual_settled(overlay, panel, motion, target_translate)
}

fn enter_animation_running(
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
) -> bool {
    if enter_visual_settled(overlay, panel, motion, target_translate) {
        return false;
    }
    true
}

fn write_modal_shell_tick_targets(
    shell: &ModalShellState,
    overlay: &mut Element,
    panel: &mut Element,
) {
    let nested = nested_panel_scale(&shell.modal_id);
    let open_visual = shell.phase == PresencePhase::Present;
    let o_target = overlay_motion_target(open_visual);
    let p_target = panel_motion_target(
        shell.motion,
        open_visual,
        nested,
        shell.extra_translate,
        shell.slide_axis_px,
    );
    panel.style.translate = p_target.translate;
    panel.style.opacity = 1.0;
    panel.style.scale = p_target.scale;
    if !slide_overlay_has_fade(overlay) {
        overlay.style.opacity = o_target.opacity;
    }
}

fn exit_visual_settled(overlay: &Element, panel: &Element, closed_translate: [f32; 2]) -> bool {
    let overlay_ok = overlay.style.opacity < 0.05;
    let dx = (panel.style.translate[0] - closed_translate[0]).abs();
    let dy = (panel.style.translate[1] - closed_translate[1]).abs();
    overlay_ok && dx < 4.0 && dy < 4.0
}

fn modal_shell_enter_css_active(overlay: &Element, panel: &Element) -> bool {
    overlay.transition_snapshot.has_active_tweens()
        || panel.transition_snapshot.has_active_tweens()
        || css_transition_needs_frame(&overlay.style, &overlay.transition_snapshot)
        || css_transition_needs_frame(&panel.style, &panel.transition_snapshot)
        || animation_run_active(&overlay.style, &overlay.animation_snapshot)
}

fn settle_modal_enter_if_ready(
    presence: &ModalPresenceHandle,
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
    target_translate: [f32; 2],
) -> bool {
    let settled = enter_visual_settled(overlay, panel, motion, target_translate);
    if settled {
        let modal_id = presence
            .cell
            .borrow()
            .shell
            .as_ref()
            .map(|s| s.modal_id.clone());
        presence.cell.borrow_mut().enter_active = false;
        crate::utils::popup::arm_underlay_close();
        if let Some(id) = modal_id {
            crate::components::drawer::on_modal_slide_settle_complete(&id);
        }
    }
    !settled
}

fn dialog_has_exit_anim(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| a.name == "fade-out-0" || a.name == "zoom-out-90" || a.name == "zoom-out-95")
}

fn reset_dialog_css_anim_snapshots(overlay: &mut Element, zoom: &mut Element) {
    overlay.animation_snapshot = Default::default();
    zoom.animation_snapshot = Default::default();
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
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    zoom.style.animations = vec![
        keyframes_anim_entry(
            "fade-in-0",
            DIALOG_ENTER_ANIM_MS,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        ),
        keyframes_anim_entry(
            "zoom-in-90",
            DIALOG_ENTER_ANIM_MS,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        ),
    ];
    reset_dialog_css_anim_snapshots(overlay, zoom);
}

fn seed_dialog_exit_anim(overlay: &mut Element, zoom: &mut Element) {
    clear_motion(overlay);
    clear_motion(zoom);
    overlay.style.opacity = 1.0;
    zoom.style.opacity = 1.0;
    zoom.style.scale = [1.0, 1.0];
    overlay.style.animations = vec![keyframes_anim_entry(
        "fade-out-0",
        DIALOG_EXIT_ANIM_MS,
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    zoom.style.animations = vec![
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
    ];
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

    let (needs_enter, needs_exit_anim) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        let needs_exit_anim = c.needs_exit_anim;
        if needs_enter && open_visual {
            c.needs_enter = false;
        }
        if needs_exit_anim && exiting {
            c.needs_exit_anim = false;
        }
        (needs_enter, needs_exit_anim)
    };

    if exiting && (needs_exit_anim || !dialog_has_exit_anim(overlay)) {
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
        let animating = dialog_css_animating(overlay, zoom);
        if !animating {
            overlay.style.opacity = 1.0;
            overlay.style.animations.clear();
            zoom.style.opacity = 1.0;
            zoom.style.scale = [1.0, 1.0];
            zoom.style.animations.clear();
            crate::utils::popup::arm_underlay_close();
        }
        return animating;
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
            c.slide_settle = false;
            // Reuse enter CSS tick so drag-release snap keeps advancing after the seed frame.
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
            motion,
            enter_was_active,
            needs_enter,
            open_visual,
            exiting,
            extra_translate,
        );
    // Entry / first exit frame / hoist mid lost. Later exit frames must NOT SnapAfterEnsure
    // (that restarts the tween from the current mid every frame → creep then pop).
    let settle_snap = !dragging && open_visual && slide_settle && !exiting;
    let animate_shell = !dragging
        && !settle_snap
        && ((exiting && needs_exit_anim) || (open_visual && (needs_enter || hoist_enter_seed)));
    let enter_in_progress =
        !dragging && open_visual && enter_was_active && !exiting && !animate_shell && !settle_snap;
    let exit_in_progress = !dragging && exiting && !animate_shell;

    // Overlay fade uses Dialog-style keyframes so rebuilds cannot snap opacity to 1
    // (opacity transitions were lost on hoist, leaving a fully-blurred underlay).
    if !dragging && exiting && (needs_exit_anim || !slide_overlay_has_exit_anim(overlay)) {
        seed_slide_overlay_exit(overlay);
    } else if !dragging
        && open_visual
        && (needs_enter
            || hoist_enter_seed
            || (enter_was_active
                && overlay.style.opacity < 0.9
                && !slide_overlay_has_enter_anim(overlay)))
    {
        seed_slide_overlay_enter(overlay);
    }

    if enter_in_progress || exit_in_progress {
        // CSS advancement and settle in [`tick_modal_enter_css_subtree`] (write back target → tick → settle).
        return false;
    }

    // Slide overlay fade is keyframes; opacity CSS transition is only for drag-release snap.
    let animate_overlay = settle_snap;
    let animate_panel_transform = (animate_shell || settle_snap)
        && motion.animates_panel_transform()
        && (!exiting || motion.animates_panel_transform_on_exit());

    let overlay_entries = overlay_motion_entries(motion, exiting);
    let panel_transform_entries = panel_motion_entries(motion, exiting);
    let nested = nested_panel_scale(modal_id);
    let o_target = overlay_motion_target(open_visual);
    let p_target = panel_motion_target(motion, open_visual, nested, extra_translate, slide_axis_px);

    let bootstrap = if (needs_enter || hoist_enter_seed) && open_visual {
        // Closed pose must be in style *before* snap baseline (carousel/transition contract).
        Some(MotionBootstrap::SnapAfterEnsure)
    } else if exiting || settle_snap {
        // Exit / drag-release: lock current mid as baseline, then ease to target.
        Some(MotionBootstrap::SnapAfterEnsure)
    } else {
        Some(MotionBootstrap::EnsureOnly)
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
        panel.style.opacity = 1.0;
        panel.style.scale = p_target.scale;
        return true;
    }

    // Seed drag-release mid before prepare_motion_sync snapshots the baseline.
    if settle_snap {
        if let Some(from) = settle_from {
            panel.style.translate = from;
            panel.style.opacity = 1.0;
            overlay.style.opacity = open_frac_from_extra(motion, from, slide_axis_px);
        }
    }

    // Enter bootstrap: write closed pose before SnapAfterEnsure locks the baseline.
    if (needs_enter || hoist_enter_seed) && open_visual {
        let closed_p = panel_motion_target(motion, false, nested, extra_translate, slide_axis_px);
        panel.style.translate = closed_p.translate;
        panel.style.scale = closed_p.scale;
        panel.style.opacity = 1.0;
        if !slide_overlay_has_fade(overlay) {
            overlay.style.opacity = overlay_motion_target(false).opacity;
        }
    }

    let entering_mid = open_visual
        && enter_was_active
        && !enter_visual_settled(overlay, panel, motion, extra_translate)
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
        if !(exiting && motion == ModalSurfaceMotion::DialogFade) {
            panel.style.translate = p_target.translate;
            panel.style.opacity = 1.0;
            panel.style.scale = p_target.scale;
        }
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

    // Settle only after mid is seeded into style — targets written by `apply` look "settled"
    // and must not clear `enter_active` on the bootstrap frame (that killed slide enter).
    if open_visual && enter_was_active {
        if animate_shell
            || enter_animation_running(overlay, panel, motion, extra_translate)
            || modal_shell_enter_css_active(overlay, panel)
        {
            needs_frame = true;
        } else if !enter_animation_running(overlay, panel, motion, extra_translate) {
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
    let motion_ptr = if shell.motion == ModalSurfaceMotion::DialogFade {
        find_template_id_mut(root, &dialog_zoom_tid(&shell.modal_id)).map(|el| el as *mut Element)
    } else {
        find_template_id_mut(root, &format!("{}__modal_panel", shell.modal_id))
            .map(|el| el as *mut Element)
    };
    let (Some(overlay_ptr), Some(motion_ptr)) = (overlay_ptr, motion_ptr) else {
        return true;
    };
    let (overlay, motion_el) = unsafe { (&mut *overlay_ptr, &mut *motion_ptr) };
    apply_modal_shell_motion_inner(
        &shell.modal_id,
        overlay,
        motion_el,
        shell.motion,
        shell.phase,
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
    clear_motion(panel);
    let prev = panel.style.translate;
    panel.style.translate = translate;
    panel.style.opacity = 1.0;
    crate::app::damage::push_frame_paint_damage(panel.layout_bounds);
    crate::app::damage::push_frame_paint_damage(translated_bounds(panel.layout_bounds, prev));
    crate::app::damage::push_frame_paint_damage(translated_bounds(panel.layout_bounds, translate));
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
                let axis = slide_axis_for_panel(shell.motion, panel, shell.slide_axis_px);
                let closed = shell.motion.closed_translate(axis);
                if exit_visual_settled(overlay, panel, closed) {
                    finish_dialog_exit_presence(&handle, &shell.modal_id);
                }
            } else if settle_modal_enter_if_ready(
                &handle,
                overlay,
                panel,
                shell.motion,
                shell.extra_translate,
            ) {
                needs_frame = true;
            } else {
                overlay.style.animations.clear();
                overlay.animation_snapshot = Default::default();
                overlay.style.opacity = 1.0;
            }
        }
    });
    needs_frame
}

fn tick_element_css(el: &mut Element, dt: f32) -> bool {
    let anim = if !el.style.animations.is_empty()
        || crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot)
    {
        crate::style::tick_element_animations(&mut el.style, &mut el.animation_snapshot, dt)
    } else {
        false
    };
    let trans = if !el.style.transition.is_empty()
        || el.transition_snapshot.has_active_tweens()
        || crate::style::css_transition_needs_frame(&el.style, &el.transition_snapshot)
    {
        crate::style::tick_element_transitions_locked(
            &mut el.style,
            &mut el.transition_snapshot,
            el.animation_snapshot.lock(),
            dt,
        )
    } else {
        false
    };
    anim || trans
}

/// Advance CSS on hoisted overlay / panel / zoom. Call **after**
/// [`flush_modal_shell_motion_subtree`] so seed/targets exist and this is the only stepper.
pub(crate) fn tick_hoisted_modal_css(root: &mut Element) -> bool {
    let dt = crate::style::utility::frame_dt();
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
            let overlay_tid = format!("{}__modal_overlay", shell.modal_id);
            let motion_tid = if shell.motion == ModalSurfaceMotion::DialogFade {
                dialog_zoom_tid(&shell.modal_id)
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
            if tick_element_css(overlay, dt) {
                any = true;
            }
            if tick_element_css(motion_el, dt) {
                any = true;
            }
        }
    });
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
    let mut needs_frame = false;
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
            }
        }
        dragging
            || matches!(phase, PresencePhase::Present | PresencePhase::Exiting)
            || (c.enter_active && motion != ModalSurfaceMotion::DialogFade)
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
    use crate::style::{
        advance_css_transition_timeline_ms, set_css_transition_timeline_ms,
        set_css_transition_virtual_timeline,
    };
    use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage};

    fn attach_dialog_zoom_shell(modal_id: &str, panel: &mut Element) {
        panel.template_id = Some(format!("{modal_id}__modal_panel"));
        let mut zoom = Element::new();
        zoom.template_id = Some(dialog_zoom_tid(modal_id));
        panel.children.push(zoom);
    }

    #[test]
    fn sync_modal_presence_survives_mounted_effect_reentry() {
        create_root(|| {
            let handle = use_modal_presence("reentry-test");
            let mounted = mounted_signal(&handle);
            create_effect(move || {
                let _ = mounted.get();
            });
            assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
            assert!(mounted.get());
            assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
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
            assert!(!handle.cell.borrow().enter_active);
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
        use crate::style::tick_element_animations;

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
                let _ = tick_element_animations(&mut o.style, &mut o.animation_snapshot, 0.016);
                let _ = tick_element_animations(&mut z.style, &mut z.animation_snapshot, 0.016);
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
            assert!(
                (overlay.style.opacity - 1.0).abs() < 0.05,
                "enter animation should settle to opaque, got {}",
                overlay.style.opacity
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
        use crate::style::tick_element_animations;

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
                let _ = tick_element_animations(&mut o.style, &mut o.animation_snapshot, 0.016);
                let _ = tick_element_animations(&mut z.style, &mut z.animation_snapshot, 0.016);
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
            {
                let mut c = handle.cell.borrow_mut();
                if let Some(shell) = c.shell.as_mut() {
                    shell.phase = PresencePhase::Exiting;
                }
            }
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
    fn slide_enter_does_not_settle_while_panel_offscreen() {
        create_root(|| {
            let mut overlay = Element::new();
            overlay.style.opacity = 1.0;
            let mut panel = Element::new();
            panel.style.opacity = 1.0;
            panel.style.translate = [0.0, 400.0];
            assert!(
                !enter_visual_settled(
                    &overlay,
                    &panel,
                    ModalSurfaceMotion::SlideFromBottom,
                    [0.0, 0.0]
                ),
                "opacity=1 must not settle a slide that is still translated"
            );
            panel.style.translate = [0.0, 0.0];
            assert!(enter_visual_settled(
                &overlay,
                &panel,
                ModalSurfaceMotion::SlideFromBottom,
                [0.0, 0.0]
            ));
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
                panel.style.translate[1] > 100.0,
                "slide enter must start off-screen, got ty={}",
                panel.style.translate[1]
            );
            assert!(handle.cell.borrow().enter_active);
        });
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
            let ty_seed = find_template_id_mut(&mut root, "slide-x__modal_panel")
                .unwrap()
                .style
                .translate[1];
            assert!(
                ty_seed < 80.0,
                "first exit sample must stay near the open pose, got {ty_seed}"
            );
            {
                let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
                assert!(
                    !p.style.transition.is_empty(),
                    "exit must install a transform transition"
                );
            }
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let ty_again = find_template_id_mut(&mut root, "slide-x__modal_panel")
                .unwrap()
                .style
                .translate[1];
            assert!(
                ty_again < 80.0,
                "later flushes must not SnapAfterEnsure the closed pose, got {ty_again}"
            );
            advance_css_transition_timeline_ms(400.0);
            {
                let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
                let _ = tick_element_css(p, 0.0);
            }
            let ty_end = find_template_id_mut(&mut root, "slide-x__modal_panel")
                .unwrap()
                .style
                .translate[1];
            set_css_transition_virtual_timeline(false);
            assert!(
                ty_end > 500.0,
                "exit tween must be able to finish after a second flush, got {ty_end}"
            );
        });
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
            panel.style.translate = [0.0, 240.0];
            panel.style.transition.clear();
            root.children.push(overlay);
            root.children.push(panel);
            arm_modal_slide_settle("slide-s", [0.0, 240.0]);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let ty = find_template_id_mut(&mut root, "slide-s__modal_panel")
                .unwrap()
                .style
                .translate[1];
            set_css_transition_virtual_timeline(false);
            assert!(
                ty > 100.0 && ty < 280.0,
                "release snap must continue from the drag pose, not re-enter from off-screen, got {ty}"
            );
        });
    }
}
