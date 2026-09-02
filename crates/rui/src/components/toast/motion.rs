//! Toast 入退场 / 堆叠档位运动（产品层）。
//!
//! 只写目标；mid 由 `utils::motion` + reconcile/hoist 保留。

use super::*;
use crate::utils::layout_group::{SlotListConfig, slot_list_pose, slot_visual_offset};
use crate::utils::motion::{
    MotionBootstrap, MotionEpsilon, MotionStyle, MotionTransform, ensure_motion_transitions,
    motion_sync_status, prepare_motion_sync, snap_motion_baseline,
};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToastCssPhase {
    /// 新通知自边缘滑入。
    Enter,
    /// 堆叠 lift/scale。
    Stack,
    /// 最前整卡滑出 + 淡出。
    FrontExit,
    /// 后方淡出；挤出则向 limit 档位靠拢并淡出。
    BehindExit,
}

fn toast_css_phase(exiting: bool, behind_fade: bool, entering: bool) -> ToastCssPhase {
    if exiting {
        if behind_fade {
            ToastCssPhase::BehindExit
        } else {
            ToastCssPhase::FrontExit
        }
    } else if entering {
        ToastCssPhase::Enter
    } else {
        ToastCssPhase::Stack
    }
}

fn toast_motion_entries(duration_ms: f32, phase: ToastCssPhase) -> [TransitionEntry; 3] {
    let ms = duration_ms.max(1.0);
    let (transform_ease, opacity_ease) = match phase {
        ToastCssPhase::Enter => (TOAST_ENTER_EASE, TOAST_OPACITY_EASE),
        ToastCssPhase::Stack => (TOAST_STACK_EASE, TOAST_OPACITY_EASE),
        ToastCssPhase::FrontExit => (TOAST_FRONT_EXIT_EASE, TOAST_FRONT_EXIT_EASE),
        ToastCssPhase::BehindExit => (TOAST_BEHIND_EXIT_EASE, TOAST_BEHIND_EXIT_EASE),
    };
    let shadow_ease = TOAST_STACK_EASE;
    let shadow_ms = TOAST_STACK_MS;
    [
        TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: ms,
            delay_ms: 0.0,
            timing: transform_ease,
        },
        TransitionEntry {
            property: TransitionProperty::Opacity,
            duration_ms: ms,
            delay_ms: 0.0,
            timing: opacity_ease,
        },
        TransitionEntry {
            property: TransitionProperty::BoxShadow,
            duration_ms: shadow_ms,
            delay_ms: 0.0,
            timing: shadow_ease,
        },
    ]
}

fn ensure_toast_css_transition(el: &mut Element, duration_ms: f32, phase: ToastCssPhase) {
    let entries = toast_motion_entries(duration_ms, phase);
    ensure_motion_transitions(&mut el.style, &entries);
}

pub(super) fn toast_enter_offset_y(from_bottom: bool) -> f32 {
    if from_bottom {
        TOAST_ENTER_SLIDE
    } else {
        -TOAST_ENTER_SLIDE
    }
}

/// 默认退场终点：退回入场边（运动方向与入场相反）。
/// 顶栏从上滑入 → 向上滑出；底栏从下滑入 → 向下滑出。
pub(super) fn toast_exit_offset_y(from_bottom: bool) -> f32 {
    toast_enter_offset_y(from_bottom).signum() * TOAST_EXIT_SLIDE
}

pub(super) fn exit_target(from: [f32; 2], from_bottom: bool) -> [f32; 2] {
    let mag = from[0].abs() + from[1].abs();
    if mag > 1.0 {
        let sx = if from[0].abs() < 1.0 {
            0.0
        } else {
            from[0].signum()
        };
        let sy = if from[1].abs() < 1.0 {
            0.0
        } else {
            from[1].signum()
        };
        [
            from[0] + sx * TOAST_EXIT_SLIDE,
            from[1] + sy * TOAST_EXIT_SLIDE,
        ]
    } else {
        [0.0, toast_exit_offset_y(from_bottom)]
    }
}

/// 四边均匀阴影（offset 0,0）；语义变体仅换色相，强度与 default 一致。
pub(super) fn tone_shadow(tone: ToastTone) -> ([f32; 4], f32, f32) {
    const BLUR: f32 = 10.0;
    const SPREAD: f32 = -2.0;
    const ALPHA: f32 = 0.12;
    match tone {
        ToastTone::Default => ([0.0, 0.0, 0.0, ALPHA], BLUR, SPREAD),
        ToastTone::Info => {
            let mut c = parse_color("blue-500");
            c[3] = ALPHA;
            (c, BLUR, SPREAD)
        }
        ToastTone::Success => {
            let mut c = parse_color("green-500");
            c[3] = ALPHA;
            (c, BLUR, SPREAD)
        }
        ToastTone::Warning => {
            let mut c = parse_color("orange-500");
            c[3] = ALPHA;
            (c, BLUR, SPREAD)
        }
        ToastTone::Destructive | ToastTone::Error => {
            let mut c = parse_color("red-500");
            c[3] = ALPHA;
            (c, BLUR, SPREAD)
        }
    }
}

/// 收起堆叠后方层：阴影 alpha 淡出至 0（blur/spread 不变，便于只 tween color）。
pub(super) fn tone_shadow_stacked(tone: ToastTone, behind: bool) -> ([f32; 4], f32, f32) {
    let (mut c, blur, spread) = tone_shadow(tone);
    if behind {
        c[3] = 0.0;
    }
    (c, blur, spread)
}

/// 堆叠位移动画参数；`None` = list 布局仅入场/退场。
#[derive(Clone)]
pub(super) struct ToastStackMotion {
    pub(super) target_lift: f32,
    pub(super) target_scale: f32,
    pub(super) behind_fade: bool,
    /// 收起时后方卡：阴影目标 alpha=0（与 lift 同步淡出）。
    pub(super) dim_shadow: bool,
    /// `Some(rank)`：每帧按 [`stack_expanded_now`] 重算 lift/scale/dim（悬停展开热路径）。
    /// `None`：用上面烘焙目标（list / 单测）。
    pub(super) live_rank: Option<usize>,
}

/// 堆叠 rank → 原始 lift / scale / 是否压暗阴影（LayoutGroup / slot_list）。
pub(super) fn stack_pose_for_rank(
    rank: usize,
    from_bottom: bool,
    expanded: bool,
) -> (f32, f32, bool) {
    let pose = slot_list_pose(
        rank,
        expanded,
        &SlotListConfig::toast_stack(CARD_H, STACK_PEEK, STACK_EXPAND_GAP, !from_bottom),
    );
    (pose.offset, pose.scale, pose.dim)
}

fn resolve_stack_pose(stack: &ToastStackMotion, from_bottom: bool) -> (f32, f32, bool) {
    if let Some(rank) = stack.live_rank {
        let expanded = stack_expanded_now();
        // 展开时退场后方卡提到 front 档，避免仍停在窥视位。
        let effective = if stack.behind_fade && expanded {
            0
        } else {
            rank
        };
        stack_pose_for_rank(effective, from_bottom, expanded)
    } else {
        (stack.target_lift, stack.target_scale, stack.dim_shadow)
    }
}

/// 缩放绕壳顶/底时，translate 补偿（LayoutGroup `slot_visual_offset`）。
pub(super) fn stack_lift_visual(from_bottom: bool, lift_raw: f32, scale: f32) -> f32 {
    slot_visual_offset(lift_raw, scale, CARD_H, !from_bottom)
}

/// 当前目标姿态（只写目标；mid 由 Element CSS transition 推进）。
pub(super) fn toast_motion_pose(
    item_id: u64,
    exiting: bool,
    from_bottom: bool,
    behind_fade: bool,
    stack: Option<&ToastStackMotion>,
    drag: [f32; 2],
) -> (f32, f32, f32, f32) {
    let (lift_raw, scale, _) = stack
        .map(|s| resolve_stack_pose(s, from_bottom))
        .unwrap_or((0.0, 1.0, false));
    let lift = stack
        .map(|_| stack_lift_visual(from_bottom, lift_raw, scale))
        .unwrap_or(0.0);

    if exiting {
        let exit_from = toast_exit_from_xy(item_id).peek();
        let [to_x, to_y] = exit_target(exit_from, from_bottom);
        if behind_fade {
            // 后方手动关：lift 为当前档（原地淡出）。
            // 挤出：lift/scale 已指向 limit 档（向上限+1 靠拢）再淡出。
            (0.0, lift, 0.0, scale)
        } else {
            (to_x, lift + to_y, 0.0, scale)
        }
    } else {
        (drag[0], lift + drag[1], 1.0, scale)
    }
}

fn toast_motion_duration_ms(exiting: bool, behind_fade: bool, entering: bool) -> f32 {
    if exiting {
        if behind_fade {
            TOAST_BEHIND_EXIT_MS as f32
        } else {
            TOAST_EXIT_MS as f32
        }
    } else if entering {
        TOAST_ENTER_MS
    } else {
        TOAST_STACK_MS
    }
}

/// 入场：仅「新挂上可见队列」时自边缘整卡滑入（Sonner）；堆叠晋升 front 不重播。
/// 退场：最前滑向边缘并淡出（ease-in）；后方原地淡出；挤出向 limit 档靠拢并淡出。
///
/// 运动挂在稳定 `toast-card-{id}` 的 CSS `transition-*` 上：重建时 reconcile /
/// portal 同 key 调和保留 mid；`reactive_sync` 只写目标。
pub(super) fn apply_enter_motion(
    mut card: Element,
    item: &ToastItem,
    from_bottom: bool,
    behind_fade: bool,
    stack: Option<ToastStackMotion>,
) -> Element {
    let item_id = item.id;
    let needs_enter = take_toast_needs_enter(item_id);
    let exit_seed = item.exiting && take_toast_exit_seed(item_id);
    let drag = toast_drag_xy(item_id).peek();
    let (tx, ty, opacity, scale) = toast_motion_pose(
        item_id,
        item.exiting,
        from_bottom,
        behind_fade,
        stack.as_ref(),
        drag,
    );
    let duration_ms = toast_motion_duration_ms(item.exiting, behind_fade, needs_enter);
    let css_phase = toast_css_phase(item.exiting, behind_fade, needs_enter);
    let tone = item.tone;

    if needs_enter {
        let seed_lift = stack
            .as_ref()
            .map(|s| {
                let (lift_raw, sc, _) = resolve_stack_pose(s, from_bottom);
                stack_lift_visual(from_bottom, lift_raw, sc)
            })
            .unwrap_or(0.0);
        let seed_y = toast_enter_offset_y(from_bottom) + seed_lift;
        // 自透明 + 边缘起步，避免强 ease-out 下「已完全可见却瞬移到位」。
        card = card.translate(0.0, seed_y).opacity(0.0).scale(scale);
        ensure_toast_css_transition(&mut card, duration_ms, css_phase);
        snap_motion_baseline(&mut card);
        card.style.translate = [tx, ty];
        card.style.opacity = opacity;
        card.style.scale = [scale, scale];
    } else {
        // 退场：先落到 rest/mid；reconcile/hoist 再盖真实 mid；sync seed 后写出口目标。
        // 挤出仅在 exit_seed 帧从「当前最深可见档」起步，后续重建用目标档以免打断 mid。
        let (ix, iy, iop, isc) = if item.exiting {
            let exit_from = toast_exit_from_xy(item_id).peek();
            if exit_seed && is_displace_exit(item_id) {
                let from_rank = visible_limit().saturating_sub(1);
                let step = displace_slot_step();
                let sc = if is_stack_layout() {
                    (1.0 - from_rank as f32 * 0.05).max(0.90)
                } else {
                    1.0
                };
                let from_lift_raw = if from_bottom {
                    -(from_rank as f32 * step)
                } else {
                    from_rank as f32 * step
                };
                let lift = stack_lift_visual(from_bottom, from_lift_raw, sc);
                (exit_from[0], lift + exit_from[1], 1.0, sc)
            } else {
                let sc = scale;
                let lift = stack
                    .as_ref()
                    .map(|s| {
                        let (lift_raw, _, _) = resolve_stack_pose(s, from_bottom);
                        stack_lift_visual(from_bottom, lift_raw, sc)
                    })
                    .unwrap_or(0.0);
                (exit_from[0], lift + exit_from[1], 1.0, sc)
            }
        } else {
            (tx, ty, opacity, scale)
        };
        card = card.translate(ix, iy).opacity(iop).scale(isc);
        ensure_toast_css_transition(&mut card, duration_ms, css_phase);
        if exit_seed {
            snap_motion_baseline(&mut card);
        }
    }

    reset_toast_descendant_scales(&mut card);
    card.template_id = Some(item_template_id(TOAST_CARD_SCOPE, item_id));

    let stack_for_sync = stack;
    let exiting = item.exiting;
    let exit_seed_pending = Cell::new(exit_seed);
    // 入场缓动持续整段 ENTER_MS；避免 sync 立刻切到 Stack ease 打断起步。
    let enter_until = Cell::new(if needs_enter {
        Some(Instant::now() + Duration::from_millis(TOAST_ENTER_MS as u64))
    } else {
        None
    });
    // 上一帧已断言的目标；tick 会把 style 写成 mid，不能靠 style≈target 判断「目标未变」。
    let motion = RefCell::new(MotionStyle::new());
    let last_shadow_a = Cell::new(None::<f32>);
    let last_pe_none = Cell::new(None::<bool>);
    card = card.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let dragging = TOAST_DRAG.with(|d| d.borrow().as_ref().is_some_and(|dg| dg.id == item_id));
        let behind = stack_for_sync.as_ref().is_some_and(|s| s.behind_fade);
        let dim_shadow = stack_for_sync
            .as_ref()
            .map(|s| resolve_stack_pose(s, from_bottom).2)
            .unwrap_or(false);
        let drag = toast_drag_xy(item_id).peek();
        let (x, y, opacity, scale) = toast_motion_pose(
            item_id,
            exiting,
            from_bottom,
            behind,
            stack_for_sync.as_ref(),
            drag,
        );
        let entering = enter_until
            .get()
            .is_some_and(|until| Instant::now() < until);
        if !entering {
            enter_until.set(None);
        }
        let duration_ms = toast_motion_duration_ms(exiting, behind, entering);
        let css_phase = toast_css_phase(exiting, behind, entering);
        let entries = toast_motion_entries(duration_ms, css_phase);

        let bootstrap = if !dragging {
            if exit_seed_pending.get() {
                exit_seed_pending.set(false);
                last_shadow_a.set(None);
                Some(MotionBootstrap::SnapAfterEnsure)
            } else if el.style.transition.is_empty() {
                Some(MotionBootstrap::EnsureAfterSnap)
            } else {
                Some(MotionBootstrap::EnsureOnly)
            }
        } else {
            last_shadow_a.set(None);
            None
        };
        prepare_motion_sync(el, &entries, bootstrap, &mut motion.borrow_mut(), dragging);

        let target = MotionTransform::uniform_scale([x, y], opacity, scale);
        let target_changed =
            motion
                .borrow_mut()
                .apply(&mut el.style, target, MotionEpsilon::default());

        let (want_shadow, want_blur, want_spread) = tone_shadow_stacked(tone, dim_shadow);
        let shadow_changed = match last_shadow_a.get() {
            Some(a) => (a - want_shadow[3]).abs() > 0.001,
            None => true,
        };
        el.style.shadow_color = want_shadow;
        el.style.shadow_blur = want_blur;
        el.style.shadow_spread = want_spread;

        // 收起时后方卡不命中；展开后恢复（不重建树）。
        if !exiting {
            if let Some(s) = stack_for_sync.as_ref() {
                if s.live_rank.is_some_and(|r| r > 0) {
                    let want_none = !stack_expanded_now();
                    if last_pe_none.get() != Some(want_none) {
                        last_pe_none.set(Some(want_none));
                        if want_none {
                            el.set_attr(ATTR_POINTER_EVENTS, PointerEvents::None.as_str());
                        } else {
                            el.attrs.remove(ATTR_POINTER_EVENTS);
                        }
                    }
                }
            }
        }

        if dragging || target_changed || shadow_changed {
            last_shadow_a.set(Some(want_shadow[3]));
        }
        reset_toast_descendant_scales(el);

        motion_sync_status(el, target_changed || shadow_changed, dragging)
    });

    if item.exiting {
        card = card.pointer_events_none();
    }
    card
}

/// 与 CarouselStack 相同：缩放只写在壳上，子树保持 scale=1，由 collect 的
/// `ancestor_scale` 继承，避免子节点各自绕中心缩放导致文字脱壳。
fn reset_toast_descendant_scales(el: &mut Element) {
    for child in &mut el.children {
        if (child.style.scale[0] - 1.0).abs() > 0.001 || (child.style.scale[1] - 1.0).abs() > 0.001
        {
            child.style.scale = [1.0, 1.0];
        }
        reset_toast_descendant_scales(child);
    }
}
