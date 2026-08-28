//! Dialog / Sheet / Drawer 共享模态动效：presence（退场保留挂载）+ 表面动画。
//!
//! **Dialog** 对齐 shadcn [`dialog.tsx`](https://ui.shadcn.com/docs/components/dialog)：
//! overlay `fade-in-0` / `fade-out-0`，面板 `fade-in-0 zoom-in-95` /
//! `fade-out-0 zoom-out-95`（退场对称），走 CSS `@keyframes`（[`style::keyframes`]），
//! 不经 transition flush/tick 链路。
//!
//! Sheet / Drawer 仍用 CSS transition + [`flush_modal_shell_motion_subtree`]。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::portal_overlay::modal_stack_metrics;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::style::{
    css_transition_needs_frame, AnimationDirection,
    AnimationEntry, AnimationFillMode, AnimationIterationCount, AnimationPlayState,
    TimingFunction, TransitionEntry, TransitionProperty, tick_element_transitions_locked,
};
use crate::utils::motion::{
    clear_motion, motion_is_animating, motion_sync_status,
    prepare_motion_sync, snap_motion_baseline, MotionBootstrap, MotionEpsilon, MotionStyle,
    MotionTransform,
};
use crate::utils::presence::{ExitGate, PresencePhase};

pub(crate) const MODAL_ENTER_MS: f32 = 250.0;
pub(crate) const MODAL_EXIT_MS: f32 = 200.0;
/// shadcn Dialog `duration-200`
const DIALOG_CSS_ANIM_MS: f32 = MODAL_EXIT_MS;
const EXIT_DURATION: Duration = Duration::from_millis(MODAL_EXIT_MS as u64 + 40);
const DIALOG_ENTER_SCALE: f32 = 0.95;

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

    fn animates_panel_opacity(self) -> bool {
        false
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
    /// 最近一次 shell 构建登记的 motion（hoist 后 flush 的唯一来源）。
    shell: Option<ModalShellState>,
    /// Dialog CSS 退场：首帧 seed `fade-out-0` / `zoom-out-95`。
    needs_exit_anim: bool,
}

/// 单实例 presence + motion 状态（[`use_modal_presence`] 取得）。
#[derive(Clone)]
pub(crate) struct ModalPresenceHandle {
    cell: Rc<RefCell<ModalPresenceCell>>,
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
        }));
        PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(Rc::clone(&rc)));
        rc
    });
    ModalPresenceHandle {
        cell: cell.peek(),
    }
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
                        // gate would tear the panel down mid-zoom (卡顿 + 内容突然消失).
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
                    && c.shell.as_ref().is_some_and(|s| s.motion == ModalSurfaceMotion::DialogFade)
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

fn motion_timing(exiting: bool) -> (f32, TimingFunction) {
    let dur = if exiting { MODAL_EXIT_MS } else { MODAL_ENTER_MS };
    let timing = if exiting {
        TimingFunction::EaseIn
    } else {
        TimingFunction::EaseOut
    };
    (dur, timing)
}

fn overlay_motion_entries(exiting: bool) -> [TransitionEntry; 1] {
    let (dur, timing) = motion_timing(exiting);
    [TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: dur,
        delay_ms: 0.0,
        timing,
    }]
}

fn panel_motion_entries(exiting: bool) -> [TransitionEntry; 1] {
    let (dur, timing) = motion_timing(exiting);
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
    let base = motion.closed_translate(slide_axis_px);
    let scale = match motion {
        ModalSurfaceMotion::DialogFade => dialog_closed_scale(nested_scale),
        _ => nested_scale,
    };
    MotionTransform {
        translate: [base[0] + extra_translate[0], base[1] + extra_translate[1]],
        opacity: 1.0,
        scale: [scale, scale],
    }
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

fn enter_visual_settled(overlay: &Element, panel: &Element, motion: ModalSurfaceMotion) -> bool {
    let overlay_ok = (overlay.style.opacity - 1.0).abs() < 0.02;
    let nested = 1.0_f32;
    let panel_ok = if motion == ModalSurfaceMotion::DialogFade {
        (panel.style.scale[0] - nested).abs() < 0.02
    } else {
        panel.style.translate == [0.0, 0.0] || panel.style.opacity > 0.99
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
) -> bool {
    enter_was_active
        && !needs_enter
        && open_visual
        && !exiting
        // hoist 后 fresh 节点默认 opacity≈1 / scale≈1，mid-flight 的 0.42 不应触发 re-seed。
        && overlay.style.opacity > 0.95
        && panel.style.scale[0] > 0.995
        && !motion_is_animating(&overlay.style, &overlay.transition_snapshot)
        && !motion_is_animating(&panel.style, &panel.transition_snapshot)
        && !enter_visual_settled(overlay, panel, motion)
}

fn enter_animation_running(overlay: &Element, panel: &Element, motion: ModalSurfaceMotion) -> bool {
    if enter_visual_settled(overlay, panel, motion) {
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
    let o_target = overlay_motion_target(true);
    let p_target = panel_motion_target(
        shell.motion,
        true,
        nested,
        shell.extra_translate,
        shell.slide_axis_px,
    );
    overlay.style.opacity = o_target.opacity;
    panel.style.translate = p_target.translate;
    panel.style.opacity = 1.0;
    panel.style.scale = p_target.scale;
}

fn modal_shell_enter_css_active(overlay: &Element, panel: &Element) -> bool {
    overlay.transition_snapshot.has_active_tweens()
        || panel.transition_snapshot.has_active_tweens()
        || css_transition_needs_frame(&overlay.style, &overlay.transition_snapshot)
        || css_transition_needs_frame(&panel.style, &panel.transition_snapshot)
}

fn settle_modal_enter_if_ready(
    presence: &ModalPresenceHandle,
    overlay: &Element,
    panel: &Element,
    motion: ModalSurfaceMotion,
) -> bool {
    let settled = enter_visual_settled(overlay, panel, motion);
    if settled {
        presence.cell.borrow_mut().enter_active = false;
        crate::utils::popup::arm_underlay_close();
    }
    !settled
}

fn dialog_css_anim(name: &str, timing: TimingFunction, fill: AnimationFillMode) -> AnimationEntry {
    AnimationEntry {
        name: name.to_string(),
        duration_ms: DIALOG_CSS_ANIM_MS,
        timing,
        delay_ms: 0.0,
        iteration_count: AnimationIterationCount::Finite(1.0),
        direction: AnimationDirection::Normal,
        fill_mode: fill,
        play_state: AnimationPlayState::Running,
        ..Default::default()
    }
}

fn dialog_has_exit_anim(el: &Element) -> bool {
    el.style
        .animations
        .iter()
        .any(|a| a.name == "fade-out-0" || a.name == "zoom-out-95")
}

fn reset_dialog_css_anim_snapshots(overlay: &mut Element, zoom: &mut Element) {
    overlay.animation_snapshot = Default::default();
    zoom.animation_snapshot = Default::default();
}

fn seed_dialog_enter_anim(overlay: &mut Element, zoom: &mut Element) {
    clear_motion(overlay);
    clear_motion(zoom);
    overlay.style.opacity = 0.0;
    // shadcn content is `fade-in-0 zoom-in-95`: the panel must be **invisible** while it is
    // still scaled down. Zooming an opaque panel shows a 95% render of finished text/borders
    // that then snaps to 100% — the "缩小态 → 正常态" jump.
    zoom.style.opacity = 0.0;
    zoom.style.scale = [DIALOG_ENTER_SCALE, DIALOG_ENTER_SCALE];
    overlay.style.animations = vec![dialog_css_anim(
        "fade-in-0",
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    zoom.style.animations = vec![
        dialog_css_anim("fade-in-0", TimingFunction::EaseOut, AnimationFillMode::Both),
        dialog_css_anim(
            "zoom-in-95",
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
    overlay.style.animations = vec![dialog_css_anim(
        "fade-out-0",
        TimingFunction::EaseIn,
        AnimationFillMode::Both,
    )];
    // Symmetric with enter (`fade-out-0 zoom-out-95`). The panel fades as it shrinks, so the
    // old "ghost window" artifact that made exit zoom unusable cannot appear.
    zoom.style.animations = vec![
        dialog_css_anim("fade-out-0", TimingFunction::EaseIn, AnimationFillMode::Both),
        dialog_css_anim(
            "zoom-out-95",
            TimingFunction::EaseIn,
            AnimationFillMode::Both,
        ),
    ];
    reset_dialog_css_anim_snapshots(overlay, zoom);
}

fn dialog_css_animating(overlay: &Element, zoom: &Element) -> bool {
    fn el_animating(el: &Element) -> bool {
        if el.style.animations.is_empty() {
            return false;
        }
        if el.animation_snapshot.runs.is_empty() {
            return true;
        }
        el.animation_snapshot.runs.iter().any(|r| !r.finished)
    }
    el_animating(overlay) || el_animating(zoom)
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
        q.borrow_mut()
            .push((mounted_sig, modal_id.to_string()));
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
        let _ = portal_sync(&modal_id, mounted_sig, OverlayKind::Modal, OverlayMode::Stacked);
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
    let (needs_enter, enter_was_active) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        if needs_enter && open_visual {
            c.needs_enter = false;
            c.enter_active = true;
        }
        (needs_enter, c.enter_active)
    };

    overlay.template_id = Some(format!("{modal_id}__modal_overlay"));
    panel.template_id = Some(format!("{modal_id}__modal_panel"));

    let hoist_enter_seed = needs_hoist_enter_seed(
        overlay,
        panel,
        motion,
        enter_was_active,
        needs_enter,
        open_visual,
        exiting,
    );
    // 入场 bootstrap 仅首帧 / hoist 后丢 mid 时跑一次；enter_active 期间勿每帧 prepare_motion_sync。
    let animate_shell = !dragging
        && (exiting || (open_visual && (needs_enter || hoist_enter_seed)));
    let enter_in_progress =
        !dragging && open_visual && enter_was_active && !exiting && !animate_shell;

    if enter_in_progress {
        // CSS 推进与 settle 在 [`tick_modal_enter_css_subtree`]（写回目标 → tick → settle）。
        return false;
    }

    let animate_overlay = animate_shell;
    let animate_panel_transform = animate_shell
        && motion.animates_panel_transform()
        && (!exiting || motion.animates_panel_transform_on_exit());

    let overlay_entries = overlay_motion_entries(exiting);
    let panel_transform_entries = panel_motion_entries(exiting);
    let nested = nested_panel_scale(modal_id);
    let o_target = overlay_motion_target(open_visual);
    let p_target = panel_motion_target(motion, open_visual, nested, extra_translate, slide_axis_px);

    let bootstrap = if (needs_enter || hoist_enter_seed) && open_visual {
        Some(MotionBootstrap::EnsureAfterSnap)
    } else if exiting {
        Some(MotionBootstrap::SnapAfterEnsure)
    } else {
        Some(MotionBootstrap::EnsureOnly)
    };

    if dragging {
        let mut c = presence.cell.borrow_mut();
        clear_motion(overlay);
        clear_motion(panel);
        c.overlay_motion.reset();
        c.panel_motion.reset();
        overlay.style.opacity = o_target.opacity;
        panel.style.translate = p_target.translate;
        panel.style.opacity = 1.0;
        panel.style.scale = p_target.scale;
        return true;
    }

    let entering_mid = open_visual
        && enter_was_active
        && !enter_visual_settled(overlay, panel, motion)
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
    } else if !entering_mid {
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

    if (needs_enter || hoist_enter_seed) && open_visual {
        if animate_overlay {
            overlay.style.opacity = overlay_motion_target(false).opacity;
        }
        if animate_panel_transform {
            let closed_p =
                panel_motion_target(motion, false, nested, extra_translate, slide_axis_px);
            panel.style.translate = closed_p.translate;
            panel.style.scale = closed_p.scale;
            panel.style.opacity = 1.0;
        }
        if animate_overlay {
            snap_motion_baseline(overlay);
        }
        if animate_panel_transform {
            snap_motion_baseline(panel);
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
        || enter_animation_running(overlay, panel, motion);

    if animate_overlay {
        seed_modal_shell_transition_mid(overlay);
    }
    if animate_panel_transform {
        seed_modal_shell_transition_mid(panel);
    }

    if open_visual && enter_was_active && animate_shell && enter_animation_running(overlay, panel, motion) {
        needs_frame = true;
    } else if open_visual && enter_was_active && !enter_animation_running(overlay, panel, motion) {
        c.enter_active = false;
    }

    needs_frame
}

fn flush_shell_state(root: &mut Element, shell: &ModalShellState, presence: &ModalPresenceHandle) -> bool {
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
    panel.style.translate = translate;
    panel.style.opacity = 1.0;
    crate::app::damage::push_frame_paint_damage(panel.layout_bounds);
}

/// 入场 CSS transition 推进（须在 hoist + [`flush_modal_shell_motion_subtree`] 之后）。
pub(crate) fn tick_modal_enter_css_subtree(root: &mut Element) -> bool {
    let mut needs_frame = false;
    PRESENCE_REGISTRY.with(|reg| {
        for cell_rc in reg.borrow().iter() {
            let (enter_active, shell, handle) = {
                let c = cell_rc.borrow();
                if !c.gate.is_mounted() || !c.enter_active {
                    continue;
                }
                let Some(shell) = c.shell.clone() else {
                    continue;
                };
                if shell.motion == ModalSurfaceMotion::DialogFade {
                    continue;
                }
                (
                    true,
                    shell,
                    ModalPresenceHandle {
                        cell: Rc::clone(cell_rc),
                    },
                )
            };
            if !enter_active {
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
            write_modal_shell_tick_targets(&shell, overlay, panel);
            needs_frame = true;
            if settle_modal_enter_if_ready(&handle, overlay, panel, shell.motion) {
                needs_frame = true;
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

/// Portal hoist 后：按 presence 登记 + `template_id` 写 motion 到 hoisted 树。
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

/// 登记 shell motion（仅写 presence + `template_id`；hoist 后由 flush 写 live 树）。
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
    let needs_frame = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        c.shell = Some(ModalShellState {
            modal_id: modal_id.to_string(),
            motion,
            phase,
            extra_translate,
            dragging,
            slide_axis_px,
        });
        // 构建树默认 opacity=1；登记时写 shadcn 入场初态，flush 再 seed `@keyframes`。
        if needs_enter
            && matches!(phase, PresencePhase::Present)
            && !dragging
            && motion == ModalSurfaceMotion::DialogFade
        {
            overlay.style.opacity = 0.0;
            // Panel fades with the overlay, so hide the zoom layer here too — otherwise a
            // frame that paints before flush shows the panel opaque at 95%.
            if let Some(zoom) = find_template_id_mut(panel, &dialog_zoom_tid(modal_id)) {
                zoom.style.opacity = 0.0;
                zoom.style.scale = [DIALOG_ENTER_SCALE, DIALOG_ENTER_SCALE];
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
                overlay.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "overlay must run fade-in keyframes"
            );
            assert!(!handle.cell.borrow().enter_active);
            assert!(flush_modal_shell_motion_subtree(&mut root));
            let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
            assert!(
                overlay.style.animations.iter().any(|a| a.name == "fade-in-0"),
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
            let p_ptr =
                find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (overlay, zoom) = unsafe { (&*o_ptr, &*p_ptr) };
            assert!(
                overlay.style.opacity < 0.05,
                "flush must start overlay faded, got {}",
                overlay.style.opacity
            );
            assert!(
                overlay.style.animations.iter().any(|a| a.name == "fade-in-0")
            );
            assert!(
                zoom.style.animations.iter().any(|a| a.name == "zoom-in-95")
            );
            assert!(
                zoom.style.animations.iter().any(|a| a.name == "fade-in-0"),
                "panel must fade together with the overlay (shadcn `fade-in-0 zoom-in-95`)"
            );
            assert!(
                (zoom.style.scale[0] - DIALOG_ENTER_SCALE).abs() < 0.02,
                "zoom layer starts at zoom-in-95, got {}",
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
        use crate::app::portal_overlay::{clear_overlay_stack, OverlayKind, OverlayMode, portal_sync};
        use crate::layout::layout_tree::hoist_portal_subtrees;
        use crate::primitives::overlay::overlay;

        clear_overlay_stack();
        create_root(|| {
            let open = use_signal("dlg-open", || true);
            let handle = use_modal_presence("gal-dialog");
            sync_modal_presence(&handle, true);
            let Some(zs) = portal_sync("gal-dialog", open, OverlayKind::Modal, OverlayMode::Stacked) else {
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
                .expect("hoisted panel must keep template_id") as *mut Element;
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
        use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};

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
            assert_eq!(crate::app::portal_overlay::overlay_stack_depth_for_test(), 0);
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
            let z_ptr =
                find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
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
                zoom.style.animations.iter().any(|a| a.name == "zoom-out-95"),
                "exit must mirror enter (shadcn `fade-out-0 zoom-out-95`)"
            );
            // Exit starts from the settled pose; the keyframes take it down from here.
            assert!((zoom.style.scale[0] - 1.0).abs() < 0.01);
            assert!((zoom.style.opacity - 1.0).abs() < 0.01);
        });
    }
}
