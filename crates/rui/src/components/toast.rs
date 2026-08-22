//! # Toast / Toaster
//!
//! 对齐 ShadCN **Sonner** 宿主与 solid-ui **Toast**（Kobalte）列表方案：
//!
//! - **宿主布局变体**（[`Toaster`] / [`toast_host`] 的 `variant`，**不要**用 `sonner`/`toast` 作名）：
//!   - **`stack`（默认）**：可收缩堆叠；收起时后方仅窥视条，悬停展开；新项整卡滑入。
//!   - **`list`**：逐条展开、带间距的固定列表（solid-ui `toast.tsx` / Kobalte List）。
//! - **上限策略**（[`ToastOverflow`] / `overflow`；**`limit` 仅控制同时展示数**）：
//!   - **`displace`（默认）**：新通知立即入场；最旧可见项同步淡出后进等待队列，计时继续；到期自 pending 直接退出；有空位后归堆叠最底。
//!   - **`wait`**：新通知进入等待队列（计时仍启动），待有空位后再入场。
//! - **条目语气变体**（[`Toast`] / [`ToastBuilder::variant`]）：
//!   `default` / `info` / `success` / `warning` / `destructive`（=`error`）。
//!   语义色落在边框、阴影与文案/图标上；容器底保持 popover。
//!   非 `default` 默认在左侧居中显示语气图标，可用 [`.icon(false)`](ToastBuilder::icon) 关闭。
//!
//! ## 用法
//! ```ignore
//! ui! { <Toaster variant="stack" overflow="displace" limit={3} /> }
//! ui! { <Toaster variant="list" overflow="wait" limit={5} /> }
//! toast_set_overflow("wait");
//! toast_set_limit(3);
//!
//! toast("Profile updated")
//!     .description("Your changes are live.")
//!     .variant("success")
//!     .show();
//! ```
//!
//! ## 注意
//! - 宿主锚点 0×0（不进 flex 分栏）；舞台提升到窗口根 portal 层（与 Dialog / Command 同层）。
//!   `ui! { <Toaster/> }` 会盖掉根 `template_id`，因此锚点必须是子节点。
//! - 队列项 id 仅内部递增；关闭：关闭钮，或朝靠近边缘方向拖拽松手（四角两边、中位垂直反向）。
//! - 点击通知本体**不会**关闭；悬停时暂停自动消失计时。
//! - `position`：`top-left` / `top-center` / `top-right` / `bottom-left` / `bottom-center` / `bottom-right`。
//! - 入场/退场/堆叠位移用壳上的 **CSS `transition-transform` + `transition-opacity`**
//!  （与 CarouselStack 同口径）：只写目标；mid 由 `transition_snapshot` 推进。
//!   稳定靠 `toast-card-{id}` reconcile 保留 + portal 同 key 调和，勿再挂每卡 cubic。
//! - 堆叠父级带 `data-stacking-context`；框架对带 `z-index` 的子树按卡 **rects→text**
//!   绘制，避免下层字形透到上层壳上。
//! - 达 `limit` 后再连推时，并发退场最多 1 条（多余立刻 snap 进 pending），避免 portal
//!   band / `prepare_text` 堆积把帧率打穿。

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::{remove_named_signal, use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::{LayoutTree, ATTR_PORTAL_KEY};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{
    parse_color, parse_text_color, snap_element_transitions_to_current, TimingFunction,
    TransitionEntry, TransitionProperty,
};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::popup::portal_panel;
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;
use std::cell::RefCell;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// 宿主布局：`stack`（默认，可收缩堆叠）| `list`（逐条展开）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastLayout {
    #[default]
    Stack,
    List,
}

impl ToastLayout {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "list" | "expanded" | "queue" => Self::List,
            _ => Self::Stack,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stack => "stack",
            Self::List => "list",
        }
    }
}

/// 条目语气（solid-ui `toastVariants`）。
///
/// `Destructive` 与 `Error` 为同一视觉变体（浅红色边框/阴影/文案）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastTone {
    #[default]
    Default,
    Info,
    Success,
    Warning,
    Destructive,
    Error,
}

impl ToastTone {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "info" => Self::Info,
            "success" => Self::Success,
            "warning" | "warn" => Self::Warning,
            "destructive" | "error" => Self::Destructive,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Destructive => "destructive",
            Self::Error => "error",
        }
    }

    /// 非 default 语气默认带左侧图标。
    fn default_show_icon(self) -> bool {
        !matches!(self, Self::Default)
    }
}

/// 可见队列已满时，新通知的处理策略。
///
/// **`limit` 只限制同时展示条数**；被挤出的项进入 [`toast_pending`]，计时继续，有空位后从**堆叠最底（队列最旧侧）**归队（仅堆叠重排，不顶栏滑入）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastOverflow {
    /// 新通知立即入场；最旧可见项移入等待队列（不销毁）（默认）。
    #[default]
    Displace,
    /// 新通知进入等待队列，待有空位后再入场。
    Wait,
}

impl ToastOverflow {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "wait" | "queue" | "hold" | "pending" => Self::Wait,
            // displace / replace / drop / evict / push
            _ => Self::Displace,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Displace => "displace",
            Self::Wait => "wait",
        }
    }
}

/// 宿主锚点：六向（上/下 × 左/中/右）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastPosition {
    TopLeft,
    #[default]
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl ToastPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "top-left" | "tl" => Self::TopLeft,
            "top-right" | "tr" => Self::TopRight,
            "bottom-left" | "bl" => Self::BottomLeft,
            "bottom-center" | "bottom" | "bc" => Self::BottomCenter,
            "bottom-right" | "br" => Self::BottomRight,
            // top-center / top / tc / default（Sonner 默认顶中）
            _ => Self::TopCenter,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCenter => "top-center",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomCenter => "bottom-center",
            Self::BottomRight => "bottom-right",
        }
    }

    pub fn is_bottom(self) -> bool {
        matches!(
            self,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight
        )
    }
}

/// 拖拽关闭允许的轴向（四角可沿靠近的两边；中上/中下仅垂直反向）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ToastSwipeDirs {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

impl ToastSwipeDirs {
    fn for_position(pos: ToastPosition) -> Self {
        match pos {
            ToastPosition::TopLeft => Self {
                up: true,
                left: true,
                ..Self::default()
            },
            ToastPosition::TopCenter => Self {
                up: true,
                ..Self::default()
            },
            ToastPosition::TopRight => Self {
                up: true,
                right: true,
                ..Self::default()
            },
            ToastPosition::BottomLeft => Self {
                down: true,
                left: true,
                ..Self::default()
            },
            ToastPosition::BottomCenter => Self {
                down: true,
                ..Self::default()
            },
            ToastPosition::BottomRight => Self {
                down: true,
                right: true,
                ..Self::default()
            },
        }
    }
}

#[derive(Clone, Debug)]
struct ToastDrag {
    id: u64,
    start_x: f32,
    start_y: f32,
    dirs: ToastSwipeDirs,
}

thread_local! {
    static TOAST_DRAG: RefCell<Option<ToastDrag>> = const { RefCell::new(None) };
    /// 独立于 `toast_queue` 的剩余时间计时（暂停勿写回 queue，避免重建导致 hover 闪断）。
    static TOAST_TIMERS: RefCell<std::collections::HashMap<u64, ToastTimer>> =
        RefCell::new(std::collections::HashMap::new());
    static TOAST_STACK_TIMER_PAUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// 可见入场标记：仅「新挂上可见队列」播入场滑入；堆叠晋升为 front 不重播。
    static TOAST_NEEDS_ENTER: RefCell<std::collections::HashSet<u64>> =
        RefCell::new(std::collections::HashSet::new());
    /// 退场首帧：在当前 mid 上 seed CSS 基线，再写滑出/淡出目标。
    static TOAST_EXIT_SEED: RefCell<std::collections::HashSet<u64>> =
        RefCell::new(std::collections::HashSet::new());
    /// 退场视觉：front 滑出 / 收起堆叠的后方淡出（对标 Sonner `data-front=false`）。
    static TOAST_EXIT_VIS: RefCell<std::collections::HashMap<u64, ToastExitVis>> =
        RefCell::new(std::collections::HashMap::new());
}

#[derive(Clone, Copy, Debug)]
struct ToastExitVis {
    /// 退场开始时是否为视觉最前。
    as_front: bool,
    /// 退场开始时在非 exiting 堆叠中的 rank（0=最前）。
    rank: usize,
    /// 淡出结束后移入 pending（displace 挤出），而非销毁。
    to_pending: bool,
}

#[derive(Clone, Debug)]
struct ToastTimer {
    remaining: Duration,
    running_since: Option<Instant>,
}

const SWIPE_DISMISS_PX: f32 = 56.0;
/// 退场滑出动画时长（到时后真正出队）。
const TOAST_EXIT_MS: u64 = 320;
/// 收起堆叠中后方条目淡出时长（Sonner opacity ~200ms）。
const TOAST_BEHIND_EXIT_MS: u64 = 240;
/// 同时做完整退场动画的上限（非挤出：手动关闭/到期）。挤出淡出另计，见
/// [`ensure_exit_budget`]，避免连环 displace 把上一段退场 snap 掉。
const MAX_CONCURRENT_EXITING: usize = 1;
/// 入场 CSS transition 时长。
const TOAST_ENTER_MS: f32 = 550.0;
/// 堆叠 lift/scale CSS transition 时长。
const TOAST_STACK_MS: f32 = 480.0;
/// 堆叠 lift/scale：略减速末段，位移小不必强调起步。
const TOAST_STACK_EASE: TimingFunction = TimingFunction::CubicBezier(0.22, 1.0, 0.36, 1.0);
/// 入场滑入：零初速起步（勿用强 ease-out，开头会像瞬移弹出）。
const TOAST_ENTER_EASE: TimingFunction = TimingFunction::CubicBezier(0.4, 0.0, 0.2, 1.0);
/// 入场透明度。
const TOAST_OPACITY_EASE: TimingFunction = TimingFunction::EaseInOut;
/// 最前退场：先慢后快（ease-in），避免 ease-out 开头瞬隐。
const TOAST_FRONT_EXIT_EASE: TimingFunction = TimingFunction::EaseIn;
/// 后方淡出 / 挤出靠拢。
const TOAST_BEHIND_EXIT_EASE: TimingFunction = TimingFunction::EaseInOut;

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

fn ensure_toast_css_transition(el: &mut Element, duration_ms: f32, phase: ToastCssPhase) {
    let ms = duration_ms.max(1.0);
    let (transform_ease, opacity_ease) = match phase {
        ToastCssPhase::Enter => (TOAST_ENTER_EASE, TOAST_OPACITY_EASE),
        ToastCssPhase::Stack => (TOAST_STACK_EASE, TOAST_OPACITY_EASE),
        ToastCssPhase::FrontExit => (TOAST_FRONT_EXIT_EASE, TOAST_FRONT_EXIT_EASE),
        ToastCssPhase::BehindExit => (TOAST_BEHIND_EXIT_EASE, TOAST_BEHIND_EXIT_EASE),
    };
    // 堆叠阴影淡出与 lift 同拍；BoxShadow tween 只插值 color（含 alpha）。
    let shadow_ease = TOAST_STACK_EASE;
    let shadow_ms = TOAST_STACK_MS;
    let transform_ok = el.style.transition.iter().any(|e| {
        e.property == TransitionProperty::Transform
            && (e.duration_ms - ms).abs() < 0.5
            && e.delay_ms == 0.0
            && e.timing == transform_ease
    });
    let opacity_ok = el.style.transition.iter().any(|e| {
        e.property == TransitionProperty::Opacity
            && (e.duration_ms - ms).abs() < 0.5
            && e.delay_ms == 0.0
            && e.timing == opacity_ease
    });
    let shadow_ok = el.style.transition.iter().any(|e| {
        e.property == TransitionProperty::BoxShadow
            && (e.duration_ms - shadow_ms).abs() < 0.5
            && e.delay_ms == 0.0
            && e.timing == shadow_ease
    });
    if el.style.transition.len() == 3 && transform_ok && opacity_ok && shadow_ok {
        return;
    }
    el.style.transition.clear();
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: ms,
        delay_ms: 0.0,
        timing: transform_ease,
    });
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: ms,
        delay_ms: 0.0,
        timing: opacity_ease,
    });
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::BoxShadow,
        duration_ms: shadow_ms,
        delay_ms: 0.0,
        timing: shadow_ease,
    });
}

fn toast_drag_xy(id: u64) -> Signal<[f32; 2]> {
    use_signal(&format!("__rui_toast_drag_xy:{id}"), || [0.0_f32, 0.0])
}

fn toast_exit_from_xy(id: u64) -> Signal<[f32; 2]> {
    use_signal(&format!("__rui_toast_exit_from:{id}"), || [0.0_f32, 0.0])
}

fn is_stack_layout() -> bool {
    ToastLayout::parse(&layout_signal().peek()) == ToastLayout::Stack
}

pub(crate) fn is_toast_dragging() -> bool {
    TOAST_DRAG.with(|d| d.borrow().is_some())
}

fn arm_toast_timer(id: u64, duration: Duration) {
    let running = !TOAST_STACK_TIMER_PAUSED.get();
    TOAST_TIMERS.with(|m| {
        m.borrow_mut().insert(
            id,
            ToastTimer {
                remaining: duration,
                running_since: running.then(Instant::now),
            },
        );
    });
    mark_toast_enter(id);
}

fn mark_toast_enter(id: u64) {
    TOAST_NEEDS_ENTER.with(|s| {
        s.borrow_mut().insert(id);
    });
}

fn take_toast_needs_enter(id: u64) -> bool {
    TOAST_NEEDS_ENTER.with(|s| s.borrow_mut().remove(&id))
}

fn clear_toast_enter_state(id: u64) {
    TOAST_NEEDS_ENTER.with(|s| {
        s.borrow_mut().remove(&id);
    });
}

fn mark_toast_exit_seed(id: u64) {
    TOAST_EXIT_SEED.with(|s| {
        s.borrow_mut().insert(id);
    });
}

fn take_toast_exit_seed(id: u64) -> bool {
    TOAST_EXIT_SEED.with(|s| s.borrow_mut().remove(&id))
}

fn clear_toast_exit_seed(id: u64) {
    TOAST_EXIT_SEED.with(|s| {
        s.borrow_mut().remove(&id);
    });
}

fn toast_exit_vis(id: u64) -> Option<ToastExitVis> {
    TOAST_EXIT_VIS.with(|m| m.borrow().get(&id).copied())
}

fn clear_toast_exit_vis(id: u64) {
    TOAST_EXIT_VIS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

fn exiting_count(q: &[ToastItem]) -> usize {
    q.iter().filter(|t| t.exiting).count()
}

/// 卸掉该 toast 的拖拽/退场起点信号与入场/退场 seed。
fn dispose_toast_motion(id: u64) {
    remove_named_signal(&format!("__rui_toast_drag_xy:{id}"));
    remove_named_signal(&format!("__rui_toast_exit_from:{id}"));
    clear_toast_enter_state(id);
    clear_toast_exit_seed(id);
}

/// 立刻结束一条退场：进 pending 或销毁，不播完剩余动画。
fn snap_finalize_exit(q: &mut Vec<ToastItem>, pending_out: &mut Vec<ToastItem>, id: u64) -> bool {
    let Some(idx) = q.iter().position(|t| t.id == id && t.exiting) else {
        return false;
    };
    let vis = toast_exit_vis(id);
    let mut item = q.remove(idx);
    if vis.is_some_and(|v| v.to_pending) {
        item.exiting = false;
        item.exit_started = None;
        clear_toast_enter_state(item.id);
        clear_toast_exit_vis(item.id);
        pending_out.push(item);
    } else {
        clear_toast_timer(item.id);
        dispose_toast_motion(item.id);
        clear_toast_enter_state(item.id);
        clear_toast_exit_vis(item.id);
    }
    true
}

fn is_displace_exit(id: u64) -> bool {
    toast_exit_vis(id).is_some_and(|v| v.to_pending)
}

/// 并发退场预算：挤出淡出与新通知入场同步，勿中途 snap；仅在挤出淡出过多时
/// 才收最旧一段。非挤出（关闭/到期）仍受 [`MAX_CONCURRENT_EXITING`] 限制。
fn ensure_exit_budget(q: &mut Vec<ToastItem>, pending_out: &mut Vec<ToastItem>) {
    let max_displace_fades = visible_limit().saturating_add(1).max(2);
    while q
        .iter()
        .filter(|t| t.exiting && is_displace_exit(t.id))
        .count()
        >= max_displace_fades
    {
        let Some(id) = q
            .iter()
            .find(|t| t.exiting && is_displace_exit(t.id))
            .map(|t| t.id)
        else {
            break;
        };
        if !snap_finalize_exit(q, pending_out, id) {
            break;
        }
    }
    while q
        .iter()
        .filter(|t| t.exiting && !is_displace_exit(t.id))
        .count()
        >= MAX_CONCURRENT_EXITING
    {
        let Some(id) = q
            .iter()
            .find(|t| t.exiting && !is_displace_exit(t.id))
            .map(|t| t.id)
        else {
            break;
        };
        if !snap_finalize_exit(q, pending_out, id) {
            break;
        }
    }
}

/// 可见队列中视觉最前 = 最新的非退场项（队列末尾方向）。
fn visible_front_id(q: &[ToastItem]) -> Option<u64> {
    q.iter().rev().find(|t| !t.exiting).map(|t| t.id)
}

/// 在非退场 newest-first 序列中的 rank（0=最前）。
fn visible_stack_rank(q: &[ToastItem], id: u64) -> usize {
    q.iter()
        .rev()
        .filter(|t| !t.exiting)
        .position(|t| t.id == id)
        .unwrap_or(0)
}

fn clear_toast_timer(id: u64) {
    TOAST_TIMERS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

fn pause_toast_timer(id: u64) {
    TOAST_TIMERS.with(|m| {
        if let Some(t) = m.borrow_mut().get_mut(&id) {
            if let Some(since) = t.running_since.take() {
                t.remaining = t.remaining.saturating_sub(since.elapsed());
            }
        }
    });
}

fn resume_toast_timer(id: u64) {
    TOAST_TIMERS.with(|m| {
        if let Some(t) = m.borrow_mut().get_mut(&id) {
            if t.running_since.is_none() && !t.remaining.is_zero() {
                t.running_since = Some(Instant::now());
            }
        }
    });
}

fn toast_timer_expired(id: u64) -> bool {
    TOAST_TIMERS.with(|m| {
        let map = m.borrow();
        let Some(t) = map.get(&id) else {
            return false;
        };
        match t.running_since {
            Some(since) => since.elapsed() >= t.remaining,
            None => false,
        }
    })
}

fn toast_timer_deadline(id: u64) -> Option<Instant> {
    TOAST_TIMERS.with(|m| {
        let map = m.borrow();
        let t = map.get(&id)?;
        let since = t.running_since?;
        Some(since + t.remaining)
    })
}

fn begin_toast_drag(drag: ToastDrag) {
    toast_drag_xy(drag.id).set([0.0, 0.0]);
    let id = drag.id;
    TOAST_DRAG.with(|d| *d.borrow_mut() = Some(drag));
    if is_stack_layout() {
        expanded_signal().set(true);
        set_stack_timers_paused(true);
    } else {
        set_item_timer_paused(id, true);
    }
}

fn clamp_swipe_offset(dx: f32, dy: f32, dirs: ToastSwipeDirs) -> [f32; 2] {
    let ox = if dirs.left && dx < 0.0 {
        dx
    } else if dirs.right && dx > 0.0 {
        dx
    } else {
        0.0
    };
    let oy = if dirs.up && dy < 0.0 {
        dy
    } else if dirs.down && dy > 0.0 {
        dy
    } else {
        0.0
    };
    [ox, oy]
}

/// CursorMoved：若正在拖拽 toast，按指针更新偏移（仅允许的关闭轴向）。
pub(crate) fn tick_toast_drag(x: f32, y: f32) -> bool {
    TOAST_DRAG.with(|d| {
        let Some(drag) = d.borrow().clone() else {
            return false;
        };
        let offset = clamp_swipe_offset(x - drag.start_x, y - drag.start_y, drag.dirs);
        let sig = toast_drag_xy(drag.id);
        let cur = sig.peek();
        if (cur[0] - offset[0]).abs() > 0.5 || (cur[1] - offset[1]).abs() > 0.5 {
            sig.set(offset);
        }
        true
    })
}

/// 指针抬起：任一侧向超过阈值则退场，否则回弹。
pub(crate) fn clear_toast_drag_if_any() {
    let finished = TOAST_DRAG.with(|d| d.borrow_mut().take());
    let Some(drag) = finished else {
        return;
    };
    crate::app::release_pointer_capture();
    let off = toast_drag_xy(drag.id).peek();
    let dismiss = off[0].abs() >= SWIPE_DISMISS_PX || off[1].abs() >= SWIPE_DISMISS_PX;
    if dismiss {
        begin_toast_exit(drag.id, Some(off));
    } else {
        toast_drag_xy(drag.id).set([0.0, 0.0]);
        if is_stack_layout() {
            // 拖拽期间强制暂停；结束时按当前展开态恢复（展开=仍悬停意图）。
            set_stack_timers_paused(expanded_signal().peek());
        } else {
            // list：悬停态由卡片 on_hover 继续维护；此处先解除拖拽强制暂停。
            set_item_timer_paused(drag.id, false);
        }
    }
}

fn set_stack_timers_paused(paused: bool) {
    if TOAST_STACK_TIMER_PAUSED.get() == paused {
        return;
    }
    TOAST_STACK_TIMER_PAUSED.set(paused);
    let q = toast_queue().peek();
    let p = toast_pending().peek();
    for t in q.iter().chain(p.iter()) {
        if t.exiting {
            continue;
        }
        if paused {
            pause_toast_timer(t.id);
        } else {
            resume_toast_timer(t.id);
        }
    }
}

fn set_item_timer_paused(id: u64, paused: bool) {
    if paused {
        pause_toast_timer(id);
    } else if !TOAST_STACK_TIMER_PAUSED.get() {
        resume_toast_timer(id);
    }
}

fn active_count(q: &[ToastItem]) -> usize {
    q.iter().filter(|t| !t.exiting).count()
}

/// 入场起点：顶栏从上方（负 Y），底栏从下方（正 Y）。
fn toast_enter_offset_y(from_bottom: bool) -> f32 {
    if from_bottom {
        TOAST_ENTER_SLIDE
    } else {
        -TOAST_ENTER_SLIDE
    }
}

/// 默认退场终点：退回入场边（运动方向与入场相反）。
/// 顶栏从上滑入 → 向上滑出；底栏从下滑入 → 向下滑出。
fn toast_exit_offset_y(from_bottom: bool) -> f32 {
    toast_enter_offset_y(from_bottom).signum() * TOAST_EXIT_SLIDE
}

fn exit_target(from: [f32; 2], from_bottom: bool) -> [f32; 2] {
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

fn begin_toast_exit(id: u64, from_drag: Option<[f32; 2]>) {
    let mut p = toast_pending().get();
    if p.iter().any(|t| t.id == id) {
        p.retain(|t| t.id != id);
        toast_pending().set(p);
        clear_toast_timer(id);
        dispose_toast_motion(id);
        clear_toast_enter_state(id);
        clear_toast_exit_vis(id);
        return;
    }

    let mut q = toast_queue().get();
    ensure_exit_budget(&mut q, &mut p);
    let as_front = visible_front_id(&q) == Some(id);
    let rank = visible_stack_rank(&q, id);
    let Some(t) = q.iter_mut().find(|t| t.id == id) else {
        toast_pending().set(p);
        return;
    };
    if t.exiting {
        toast_queue().set(q);
        toast_pending().set(p);
        return;
    }
    t.exiting = true;
    t.exit_started = Some(Instant::now());
    mark_toast_exit_seed(id);
    TOAST_EXIT_VIS.with(|m| {
        m.borrow_mut().insert(
            id,
            ToastExitVis {
                as_front,
                rank,
                to_pending: false,
            },
        );
    });
    let from = from_drag.unwrap_or([0.0, 0.0]);
    toast_exit_from_xy(id).set(from);
    toast_drag_xy(id).set([0.0, 0.0]);
    pause_toast_timer(id);
    if as_front && is_stack_layout() {
        // 关闭最前：收起展开堆叠，余下条目仅堆叠弹簧重排，不从顶栏重新滑入。
        expanded_signal().set(false);
    }
    toast_queue().set(q);
    toast_pending().set(p);
}

/// 到期开始退场、退场结束出队；返回是否有队列变化（需重绘）。
pub(crate) fn tick_toast_lifecycle() -> bool {
    let mut q = toast_queue().peek();
    let mut p = toast_pending().peek();

    // 无可见活跃项：先恢复 pending 计时并剔除到期项，避免暂停状态下永远无法 prune。
    if active_count(&q) == 0 {
        expanded_signal().set(false);
        set_stack_timers_paused(false);
        prune_expired_pending_into(&mut p);
    }

    // stack：以展开态为准同步冻结，避免 hover 重建闪断把计时误恢复。
    if is_stack_layout() {
        set_stack_timers_paused(expanded_signal().peek() || is_toast_dragging());
    }

    let started = prune_expired(&mut q, &mut p);
    let removed = finalize_exits(&mut q, &mut p);
    let pending_pruned = prune_expired_pending_into(&mut p);
    let visible_empty_after_exit = active_count(&q) == 0;
    if started || removed || pending_pruned {
        toast_queue().set(q);
        toast_pending().set(p);
    }
    if removed {
        if visible_empty_after_exit {
            sync_pending_when_visible_empty(&toast_queue().peek());
        }
        promote_pending();
        return true;
    }
    started || pending_pruned
}

/// 下一次应唤醒事件循环的时刻（到期或退场结束）。
pub(crate) fn toast_next_wake_at() -> Option<Instant> {
    let q = toast_queue().peek();
    let p = toast_pending().peek();
    if q.is_empty() && p.is_empty() {
        return None;
    }
    let exit_dur_front = Duration::from_millis(TOAST_EXIT_MS);
    let exit_dur_behind = Duration::from_millis(TOAST_BEHIND_EXIT_MS);
    let mut next: Option<Instant> = None;
    for t in &q {
        if t.exiting {
            if let Some(started) = t.exit_started {
                let behind_fade = toast_exit_vis(t.id).is_some_and(|v| !v.as_front);
                let dur = if behind_fade {
                    exit_dur_behind
                } else {
                    exit_dur_front
                };
                let at = started + dur;
                next = Some(next.map_or(at, |n| n.min(at)));
            }
            continue;
        }
        if let Some(at) = toast_timer_deadline(t.id) {
            next = Some(next.map_or(at, |n| n.min(at)));
        }
    }
    for t in &p {
        if let Some(at) = toast_timer_deadline(t.id) {
            next = Some(next.map_or(at, |n| n.min(at)));
        }
    }
    next
}

#[derive(Clone, Debug)]
pub struct ToastItem {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub tone: ToastTone,
    /// 是否在左侧居中显示语气图标（`None` = 跟语气默认：非 default 为 true）。
    pub show_icon: Option<bool>,
    pub created: Instant,
    pub duration: Duration,
    /// 退场动画中（仍绘制，不计可见上限）。
    pub exiting: bool,
    exit_started: Option<Instant>,
}

impl PartialEq for ToastItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.title == other.title
            && self.description == other.description
            && self.tone == other.tone
            && self.show_icon == other.show_icon
            && self.exiting == other.exiting
    }
}

impl ToastItem {
    /// 兼容旧字段名：标题即 message。
    pub fn message(&self) -> &str {
        &self.title
    }
}

const TOAST_QUEUE_KEY: &str = "__rui_toast_queue";
const TOAST_PENDING_KEY: &str = "__rui_toast_pending";
const TOAST_SEQ_KEY: &str = "__rui_toast_seq";
const TOAST_HOST_OPEN: &str = "__rui_toast_host_open";
const TOAST_LAYOUT_KEY: &str = "__rui_toast_layout";
const TOAST_POSITION_KEY: &str = "__rui_toast_position";
const TOAST_OVERFLOW_KEY: &str = "__rui_toast_overflow";
const TOAST_LIMIT_KEY: &str = "__rui_toast_limit";

const DEFAULT_LIMIT: usize = 3;
/// 高于 portal 栈里的 Modal（Command / Dialog 从 1000 起）。
const TOAST_PORTAL_Z: i32 = 50_000;
const TOAST_WIDTH: &str = "22rem";
/// 卡片估计高度（标题+描述）；堆叠绝对定位用。
const CARD_H: f32 = 68.0;
/// 入场滑入距离（对标 Sonner 近乎整卡高度的 translateY）。
const TOAST_ENTER_SLIDE: f32 = CARD_H;
/// 退场滑出距离（与入场同量级）。
const TOAST_EXIT_SLIDE: f32 = CARD_H;
/// 收起时后方条目露出的窥视高度（Sonner 观感）。
const STACK_PEEK: f32 = 14.0;
/// 展开时间距：仅卡片之间的空隙（对标 Sonner `--gap`）；步长 = CARD_H + gap。
/// 取 8px：过宽会像「容器被撑开」；过窄则展开后几乎贴死。
const STACK_EXPAND_GAP: f32 = 8.0;
/// list 布局行距（原 `vstack` gap 0.5rem）。
const LIST_GAP: f32 = 8.0;

fn list_step() -> f32 {
    CARD_H + LIST_GAP
}

/// 挤出退场起步档位步长：stack 用窥视，list 用整卡行高。
fn displace_slot_step() -> f32 {
    if is_stack_layout() {
        STACK_PEEK
    } else {
        list_step()
    }
}

/// 当前可见（已入场）队列。
pub fn toast_queue() -> Signal<Vec<ToastItem>> {
    use_signal(TOAST_QUEUE_KEY, Vec::<ToastItem>::new)
}

/// `overflow=wait` 时尚未入场的等待队列。
pub fn toast_pending() -> Signal<Vec<ToastItem>> {
    use_signal(TOAST_PENDING_KEY, Vec::<ToastItem>::new)
}

fn layout_signal() -> Signal<String> {
    use_signal(TOAST_LAYOUT_KEY, || ToastLayout::Stack.as_str().to_string())
}

fn position_signal() -> Signal<String> {
    use_signal(TOAST_POSITION_KEY, || "top-center".to_string())
}

fn overflow_signal() -> Signal<String> {
    use_signal(TOAST_OVERFLOW_KEY, || {
        ToastOverflow::Displace.as_str().to_string()
    })
}

fn limit_signal() -> Signal<usize> {
    use_signal(TOAST_LIMIT_KEY, || DEFAULT_LIMIT)
}

fn expanded_signal() -> Signal<bool> {
    use_signal("__rui_toast_stack_expanded", || false)
}

fn visible_limit() -> usize {
    limit_signal().peek().max(1)
}

/// 运行时切换宿主布局（`stack` / `list`），供 Gallery / 命令式切换。
/// 顺带套用布局默认锚点：stack → 顶中，list → 右下。
pub fn toast_set_layout(variant: impl Into<String>) {
    let layout = ToastLayout::parse(&variant.into());
    layout_signal().set(layout.as_str().to_string());
    match layout {
        ToastLayout::List => {
            expanded_signal().set(false);
            position_signal().set(ToastPosition::BottomRight.as_str().to_string());
        }
        ToastLayout::Stack => {
            position_signal().set(ToastPosition::TopCenter.as_str().to_string());
        }
    }
}

/// 运行时切换锚点位置（六向，见 [`ToastPosition`]）。
pub fn toast_set_position(position: impl Into<String>) {
    let p = ToastPosition::parse(&position.into());
    position_signal().set(p.as_str().to_string());
}

/// 运行时切换上限策略：`displace` | `wait`。
pub fn toast_set_overflow(overflow: impl Into<String>) {
    let policy = ToastOverflow::parse(&overflow.into());
    overflow_signal().set(policy.as_str().to_string());
    match policy {
        ToastOverflow::Displace => flush_pending_by_displace(),
        ToastOverflow::Wait => promote_pending(),
    }
}

/// 运行时设置同时可见上限（至少 1）。超出部分进入等待队列，有空位再入场。
pub fn toast_set_limit(limit: usize) {
    limit_signal().set(limit.max(1));
    rebalance_visible_queue();
}

/// 将最旧的非退场可见项移入 pending 末尾（先挤出的先回来）。
/// 计时器继续走；重新入场不重置剩余时间。无退场动画（limit 重平衡等）。
fn demote_oldest_active(q: &mut Vec<ToastItem>, p: &mut Vec<ToastItem>) -> bool {
    let Some(idx) = q.iter().position(|t| !t.exiting) else {
        return false;
    };
    let mut old = q.remove(idx);
    old.exiting = false;
    old.exit_started = None;
    clear_toast_enter_state(old.id);
    clear_toast_exit_vis(old.id);
    p.push(old);
    true
}

/// displace 挤出：最旧项向堆叠「上限+1」档位靠拢并淡出，与新通知入场同步；结束后进 pending。
fn begin_displace_exit(q: &mut Vec<ToastItem>, pending_out: &mut Vec<ToastItem>) -> bool {
    ensure_exit_budget(q, pending_out);
    let Some(idx) = q.iter().position(|t| !t.exiting) else {
        return false;
    };
    let id = q[idx].id;
    // 0-based：可见为 0..limit-1；挤出目标为 limit（上限的下一档）。
    let rank = visible_limit();
    let now = Instant::now();
    let t = &mut q[idx];
    t.exiting = true;
    t.exit_started = Some(now);
    mark_toast_exit_seed(id);
    TOAST_EXIT_VIS.with(|m| {
        m.borrow_mut().insert(
            id,
            ToastExitVis {
                as_front: false,
                rank,
                to_pending: true,
            },
        );
    });
    toast_exit_from_xy(id).set([0.0, 0.0]);
    true
}

/// 按 `limit` 重平衡：多余可见 → pending；有空位则 pending 晋级到可见末尾。
fn rebalance_visible_queue() {
    let lim = visible_limit();
    let mut q = toast_queue().peek();
    let mut p = toast_pending().peek();
    let pruned = prune_expired(&mut q, &mut p);
    let _ = prune_expired_pending_into(&mut p);
    let finalized = finalize_exits(&mut q, &mut p);
    let pending_pruned = prune_expired_pending_into(&mut p);

    let mut demoted = false;
    let policy = ToastOverflow::parse(&overflow_signal().peek());
    while active_count(&q) > lim {
        let ok = match policy {
            // displace：播挤出淡出，勿 demote 瞬丢。
            ToastOverflow::Displace => begin_displace_exit(&mut q, &mut p),
            ToastOverflow::Wait => demote_oldest_active(&mut q, &mut p),
        };
        if ok {
            demoted = true;
        } else {
            break;
        }
    }

    let mut promoted = false;
    if active_count(&q) < lim && !p.is_empty() {
        sync_pending_when_visible_empty(&q);
        p = toast_pending().peek();
    }
    while active_count(&q) < lim {
        if p.is_empty() {
            break;
        }
        let mut item = p.remove(0);
        // 到期项直接丢弃，不归队、不晋级。
        if toast_timer_expired(item.id) {
            clear_toast_timer(item.id);
            clear_toast_enter_state(item.id);
            clear_toast_exit_vis(item.id);
            continue;
        }
        item.exiting = false;
        item.exit_started = None;
        // pending 归队：插入可见队列最旧侧（堆叠最底），仅 lift/scale 弹簧重排，不 mark 顶栏滑入。
        let insert_at = q.iter().position(|t| !t.exiting).unwrap_or(q.len());
        q.insert(insert_at, item);
        promoted = true;
    }

    if pruned || finalized || demoted || promoted {
        toast_queue().set(q);
    }
    if demoted || promoted || finalized || pending_pruned {
        toast_pending().set(p);
    }
    if promoted {
        use_signal(TOAST_HOST_OPEN, || true).set(true);
    }
}

fn prune_expired(q: &mut Vec<ToastItem>, pending_out: &mut Vec<ToastItem>) -> bool {
    // 先快照再改：同一帧多个到期时 front/rank 不得被前序 exiting 污染。
    let plans: Vec<(u64, bool, usize)> = q
        .iter()
        .filter(|t| !t.exiting && toast_timer_expired(t.id))
        .map(|t| {
            (
                t.id,
                visible_front_id(q) == Some(t.id),
                visible_stack_rank(q, t.id),
            )
        })
        .collect();
    if plans.is_empty() {
        return false;
    }
    let now = Instant::now();
    for (id, as_front, rank) in plans {
        ensure_exit_budget(q, pending_out);
        let Some(t) = q.iter_mut().find(|t| t.id == id) else {
            continue;
        };
        if t.exiting {
            continue;
        }
        t.exiting = true;
        t.exit_started = Some(now);
        mark_toast_exit_seed(id);
        TOAST_EXIT_VIS.with(|m| {
            m.borrow_mut().insert(
                id,
                ToastExitVis {
                    as_front,
                    rank,
                    to_pending: false,
                },
            );
        });
        toast_exit_from_xy(id).set([0.0, 0.0]);
        pause_toast_timer(id);
    }
    true
}

fn prune_expired_pending_into(p: &mut Vec<ToastItem>) -> bool {
    let before = p.len();
    p.retain(|t| {
        if toast_timer_expired(t.id) {
            clear_toast_timer(t.id);
            dispose_toast_motion(t.id);
            clear_toast_enter_state(t.id);
            clear_toast_exit_vis(t.id);
            false
        } else {
            true
        }
    });
    p.len() != before
}

/// 可见队列清空后：恢复 pending 计时，并剔除已到期项（避免空窗后整批重挂）。
fn sync_pending_when_visible_empty(q: &[ToastItem]) {
    if active_count(q) != 0 {
        return;
    }
    expanded_signal().set(false);
    set_stack_timers_paused(false);
    let mut p = toast_pending().peek();
    if prune_expired_pending_into(&mut p) {
        toast_pending().set(p);
    }
}

fn finalize_exits(q: &mut Vec<ToastItem>, pending_out: &mut Vec<ToastItem>) -> bool {
    let exit_dur_front = Duration::from_millis(TOAST_EXIT_MS);
    let exit_dur_behind = Duration::from_millis(TOAST_BEHIND_EXIT_MS);
    let before = q.len();
    let pending_before = pending_out.len();
    let mut displaced: Vec<ToastItem> = Vec::new();
    q.retain(|t| {
        if !t.exiting {
            return true;
        }
        match t.exit_started {
            Some(started) => {
                let vis = toast_exit_vis(t.id);
                let behind_fade = vis.is_some_and(|v| !v.as_front);
                let dur = if behind_fade {
                    exit_dur_behind
                } else {
                    exit_dur_front
                };
                // 墙钟到点摘树（CSS transition 与时长对齐）。
                if started.elapsed() < dur {
                    return true;
                }
                if vis.is_some_and(|v| v.to_pending) {
                    let mut item = t.clone();
                    item.exiting = false;
                    item.exit_started = None;
                    clear_toast_enter_state(item.id);
                    clear_toast_exit_vis(item.id);
                    displaced.push(item);
                } else {
                    clear_toast_timer(t.id);
                    dispose_toast_motion(t.id);
                    clear_toast_enter_state(t.id);
                    clear_toast_exit_vis(t.id);
                }
                false
            }
            None => true,
        }
    });
    pending_out.extend(displaced);
    q.len() != before || pending_out.len() != pending_before
}

/// 从等待队列晋级到可见队列末尾；计时在 pending 期间持续，晋级不重置。
fn promote_pending() {
    rebalance_visible_queue();
}

/// 切换到 `displace` 时：按 limit 重平衡（等待项有空位则入场，多余可见回等待）。
fn flush_pending_by_displace() {
    rebalance_visible_queue();
}

// ---------------------------------------------------------------------------
// Imperative API
// ---------------------------------------------------------------------------

/// 命令式推送构建器（Sonner 心智：`toast("…").variant("success").show()`）。
pub struct ToastBuilder {
    title: String,
    description: String,
    tone: ToastTone,
    duration_ms: u64,
    show_icon: Option<bool>,
}

/// 以标题开始一条 toast（返回构建器；调用 [`ToastBuilder::show`] 入队）。
pub fn toast(message: impl Into<String>) -> ToastBuilder {
    ToastBuilder {
        title: message.into(),
        description: String::new(),
        tone: ToastTone::Default,
        duration_ms: 3000,
        show_icon: None,
    }
}

impl ToastBuilder {
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }
    /// 条目语气：`default` / `info` / `success` / `warning` / `destructive`（=`error`）。
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.tone = ToastTone::parse(&v.into());
        self
    }
    pub fn tone(mut self, t: ToastTone) -> Self {
        self.tone = t;
        self
    }
    /// 是否在左侧居中显示语气图标。默认：非 `default` 为 true。
    pub fn icon(mut self, show: bool) -> Self {
        self.show_icon = Some(show);
        self
    }
    pub fn duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms.max(500);
        self
    }
    /// 入队并返回 id。
    pub fn show(self) -> u64 {
        toast_push_item(
            self.title,
            self.description,
            self.tone,
            self.duration_ms,
            self.show_icon,
        )
    }
}

/// 兼容旧 API：`toast_push("msg", 2500)`。
pub fn toast_push(message: impl Into<String>, duration_ms: u64) -> u64 {
    toast(message).duration(duration_ms).show()
}

fn toast_push_item(
    title: impl Into<String>,
    description: impl Into<String>,
    tone: ToastTone,
    duration_ms: u64,
    show_icon: Option<bool>,
) -> u64 {
    let seq = use_signal(TOAST_SEQ_KEY, || 0_u64);
    let id = seq.get() + 1;
    seq.set(id);
    let duration = Duration::from_millis(duration_ms.max(500));
    let item = ToastItem {
        id,
        title: title.into(),
        description: description.into(),
        tone,
        show_icon,
        created: Instant::now(),
        duration,
        exiting: false,
        exit_started: None,
    };

    let lim = visible_limit();
    let policy = ToastOverflow::parse(&overflow_signal().peek());
    let mut q = toast_queue().get();
    let mut p = toast_pending().get();
    prune_expired(&mut q, &mut p);
    finalize_exits(&mut q, &mut p);

    if active_count(&q) < lim {
        arm_toast_timer(id, duration);
        q.push(item);
        toast_queue().set(q);
        toast_pending().set(p);
    } else {
        match policy {
            ToastOverflow::Displace => {
                // 最旧可见原地淡出 → pending；新项同步入场（勿 demote 瞬丢动画）。
                while active_count(&q) >= lim {
                    if !begin_displace_exit(&mut q, &mut p) {
                        break;
                    }
                }
                if active_count(&q) >= lim {
                    // 仍满：本条改走等待，保留已在播的挤出退场。
                    arm_toast_timer(id, duration);
                    p.push(item);
                    toast_queue().set(q);
                    toast_pending().set(p);
                } else {
                    arm_toast_timer(id, duration);
                    q.push(item);
                    toast_queue().set(q);
                    toast_pending().set(p);
                }
            }
            ToastOverflow::Wait => {
                toast_queue().set(q);
                let mut p = toast_pending().get();
                arm_toast_timer(id, duration);
                p.push(item);
                toast_pending().set(p);
            }
        }
    }
    use_signal(TOAST_HOST_OPEN, || true).set(true);
    id
}

pub fn toast_dismiss(id: u64) {
    begin_toast_exit(id, None);
}

// ---------------------------------------------------------------------------
// Visual: card + parts
// ---------------------------------------------------------------------------

fn tone_colors(tone: ToastTone) -> ([f32; 4], [u8; 4], [f32; 4], Option<&'static str>) {
    // 底色始终 popover；语义色只落在边框与文案/图标上。
    let bg = tokens::popover();
    match tone {
        ToastTone::Default => (bg, tokens::foreground_text(), tokens::border(), None),
        ToastTone::Info => (
            bg,
            parse_text_color("blue-600"),
            parse_color("blue-300"),
            Some("info"),
        ),
        ToastTone::Success => (
            bg,
            parse_text_color("green-600"),
            parse_color("green-300"),
            Some("circle-check"),
        ),
        ToastTone::Warning => (
            bg,
            parse_text_color("orange-600"),
            parse_color("orange-300"),
            Some("circle-alert"),
        ),
        ToastTone::Destructive | ToastTone::Error => (
            bg,
            parse_text_color("red-600"),
            parse_color("red-300"),
            Some("circle-x"),
        ),
    }
}

/// 四边均匀阴影（offset 0,0）；语义变体仅换色相，强度与 default 一致。
fn tone_shadow(tone: ToastTone) -> ([f32; 4], f32, f32) {
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
fn tone_shadow_stacked(tone: ToastTone, behind: bool) -> ([f32; 4], f32, f32) {
    let (mut c, blur, spread) = tone_shadow(tone);
    if behind {
        c[3] = 0.0;
    }
    (c, blur, spread)
}

/// `dirs`：关闭拖拽允许方向（四角两边，中位仅垂直反向）。
/// `collapse_h`：堆叠模式锁定与 front 同高，窥视条步长稳定（Sonner `--front-toast-height`）。
/// 后方层**不**再单独淡出文案——运动只加在整卡壳上，避免「空壳 + 文字晚到」。
fn build_toast_card(
    item: &ToastItem,
    dirs: ToastSwipeDirs,
    collapse_h: Option<f32>,
    behind: bool,
) -> Element {
    let id = item.id;
    let (mut bg, fg, mut border, icon_name) = tone_colors(item.tone);
    // 必须不透明，否则堆叠时后方文案会透出。
    bg[3] = 1.0;
    border[3] = 1.0;
    let muted_fg = if matches!(item.tone, ToastTone::Default) {
        parse_text_color("muted-foreground")
    } else {
        let mut c = fg;
        c[3] = (c[3] as f32 * 0.78) as u8;
        c
    };
    let show_icon = item
        .show_icon
        .unwrap_or_else(|| item.tone.default_show_icon());

    let mut body = vstack().gap("0.125rem").flex_grow(1.0).min_w(0.0);
    if !item.title.is_empty() {
        let title_el: Element = text(item.title.as_str())
            .size(14.0)
            .weight(600.0)
            .color(fg)
            .into();
        body = body.child(title_el);
    }
    if !item.description.is_empty() {
        let desc: Element = text(item.description.as_str())
            .size(14.0)
            .color(muted_fg)
            .into();
        body = body.child(desc);
    }

    let mut row = hstack()
        .w("100%")
        .items("center")
        .gap("0.75rem")
        .justify("between");

    if show_icon {
        if let Some(name) = icon_name {
            if let Some(g) = lucide_glyph(name) {
                // 独立左侧图标列，纵向居中（与文案区分离）。
                let icon = build_icon_text_node(g)
                    .text_size(18.0)
                    .text_color(fg)
                    .w(18.0)
                    .h(18.0)
                    .flex_shrink(0.0);
                row = row.child(icon);
            }
        }
    }

    row = row.child(Element::from(body));

    let close_glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
    let close_icon = build_icon_text_node(close_glyph)
        .text_size(14.0)
        .text_color({
            let mut c = fg;
            c[3] = (c[3] as f32 * 0.55) as u8;
            c
        })
        .w(16.0)
        .h(16.0)
        .pointer_events_none();
    let mut close_btn: Element = interactive(
        r#box()
            .p("0.25rem")
            .rounded("0.375rem")
            .child(close_icon)
            .into(),
    )
    .on_click(move || toast_dismiss(id))
    .into();
    close_btn = close_btn.on_pointer_down(move |e: &PointerEvent| {
        // 避免关闭钮按下启动整卡拖拽。
        e.stop_propagation();
    });
    row = row.child(close_btn.flex_shrink(0.0));

    let content: Element = Element::from(row);

    let capture_id = format!("toast-card-{id}");
    let capture_for_down = capture_id.clone();

    let mut card: Element = r#box()
        .w("100%")
        .min_h(CARD_H)
        .px("1rem")
        .py("0.875rem")
        .pr("0.75rem")
        .rounded("0.5rem")
        .child(content)
        .into();
    if let Some(h) = collapse_h {
        // 与 front 同高 + 裁切，窥视条步长才稳定（Sonner `--front-toast-height`）。
        card = card.h(h).overflow("hidden");
    }
    let (shadow_color, shadow_blur, shadow_spread) = tone_shadow_stacked(item.tone, behind);
    card = card
        .bg(bg)
        .border(border, 1.0)
        .shadow(shadow_color, shadow_blur)
        .box_shadow_spread(shadow_spread)
        .opacity(1.0)
        .focus_id(capture_id)
        .on_pointer_down(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            begin_toast_drag(ToastDrag {
                id,
                start_x: e.x,
                start_y: e.y,
                dirs,
            });
            crate::app::set_pointer_capture(capture_for_down.clone());
            e.stop_propagation();
        })
        .on_pointer_up(move |e: &PointerEvent| {
            if e.phase == EventPhase::Capture {
                return;
            }
            clear_toast_drag_if_any();
        });
    card
}

/// 堆叠位移动画参数；`None` = list 布局仅入场/退场。
#[derive(Clone)]
struct ToastStackMotion {
    target_lift: f32,
    target_scale: f32,
    behind_fade: bool,
    /// 收起时后方卡：阴影目标 alpha=0（与 lift 同步淡出）。
    dim_shadow: bool,
}

/// 缩放绕壳顶/底时，translate 补偿（与 CarouselStack / 原 spring 同口径）。
fn stack_lift_visual(from_bottom: bool, lift_raw: f32, scale: f32) -> f32 {
    let origin_fix = CARD_H * (1.0 - scale) * 0.5;
    if from_bottom {
        lift_raw - origin_fix
    } else {
        lift_raw + origin_fix
    }
}

/// 当前目标姿态（只写目标；mid 由 Element CSS transition 推进）。
fn toast_motion_pose(
    item_id: u64,
    exiting: bool,
    from_bottom: bool,
    behind_fade: bool,
    stack: Option<&ToastStackMotion>,
    drag: [f32; 2],
) -> (f32, f32, f32, f32) {
    let scale = stack.map(|s| s.target_scale).unwrap_or(1.0);
    let lift = stack
        .map(|s| stack_lift_visual(from_bottom, s.target_lift, scale))
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
fn apply_enter_motion(
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
            .map(|s| stack_lift_visual(from_bottom, s.target_lift, scale))
            .unwrap_or(0.0);
        let seed_y = toast_enter_offset_y(from_bottom) + seed_lift;
        // 自透明 + 边缘起步，避免强 ease-out 下「已完全可见却瞬移到位」。
        card = card
            .translate(0.0, seed_y)
            .opacity(0.0)
            .scale(scale);
        ensure_toast_css_transition(&mut card, duration_ms, css_phase);
        snap_element_transitions_to_current(&card.style, &mut card.transition_snapshot);
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
                    .map(|s| stack_lift_visual(from_bottom, s.target_lift, sc))
                    .unwrap_or(0.0);
                (exit_from[0], lift + exit_from[1], 1.0, sc)
            }
        } else {
            (tx, ty, opacity, scale)
        };
        card = card.translate(ix, iy).opacity(iop).scale(isc);
        ensure_toast_css_transition(&mut card, duration_ms, css_phase);
        if exit_seed {
            snap_element_transitions_to_current(&card.style, &mut card.transition_snapshot);
        }
    }

    reset_toast_descendant_scales(&mut card);
    card.template_id = Some(format!("toast-card-{item_id}"));

    let stack_for_sync = stack;
    let exiting = item.exiting;
    let exit_seed_pending = std::cell::Cell::new(exit_seed);
    // 入场缓动持续整段 ENTER_MS；避免 sync 立刻切到 Stack ease 打断起步。
    let enter_until = std::cell::Cell::new(if needs_enter {
        Some(Instant::now() + Duration::from_millis(TOAST_ENTER_MS as u64))
    } else {
        None
    });
    // 上一帧已断言的目标；tick 会把 style 写成 mid，不能靠 style≈target 判断「目标未变」。
    let last_target = std::cell::Cell::new(None::<(f32, f32, f32, f32)>);
    let last_shadow_a = std::cell::Cell::new(None::<f32>);
    card = card.reactive_sync(move |el: &mut Element, _lt: &mut LayoutTree| {
        let dragging =
            TOAST_DRAG.with(|d| d.borrow().as_ref().is_some_and(|dg| dg.id == item_id));
        let behind = stack_for_sync.as_ref().is_some_and(|s| s.behind_fade);
        let dim_shadow = stack_for_sync.as_ref().is_some_and(|s| s.dim_shadow);
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

        if dragging {
            el.style.transition.clear();
            el.transition_snapshot = Default::default();
            last_target.set(None);
            last_shadow_a.set(None);
        } else if exit_seed_pending.get() {
            // reconcile/hoist 已把 mid 写回 style；钉基线后再写退场目标。
            ensure_toast_css_transition(el, duration_ms, css_phase);
            snap_element_transitions_to_current(&el.style, &mut el.transition_snapshot);
            exit_seed_pending.set(false);
            last_target.set(None);
            last_shadow_a.set(None);
        } else if el.style.transition.is_empty() {
            snap_element_transitions_to_current(&el.style, &mut el.transition_snapshot);
            ensure_toast_css_transition(el, duration_ms, css_phase);
        } else {
            ensure_toast_css_transition(el, duration_ms, css_phase);
        }

        let mut impact = LayoutStatus::Clean;
        let target = (x, y, opacity, scale);
        let target_changed = match last_target.get() {
            Some((lx, ly, lo, ls)) => {
                (lx - x).abs() > 0.5
                    || (ly - y).abs() > 0.5
                    || (lo - opacity).abs() > 0.001
                    || (ls - scale).abs() > 0.001
            }
            None => true,
        };

        // 每帧把目标写回 style（tick 会覆盖为 mid）；仅目标变化或拖拽时报 StyleChanged，
        // 避免 3 卡堆叠动画期间每帧 3× StyleChanged 打满同步路径。
        el.style.translate = [x, y];
        el.style.opacity = opacity;
        el.style.scale = [scale, scale];

        let (want_shadow, want_blur, want_spread) = tone_shadow_stacked(tone, dim_shadow);
        let shadow_changed = match last_shadow_a.get() {
            Some(a) => (a - want_shadow[3]).abs() > 0.001,
            None => true,
        };
        el.style.shadow_color = want_shadow;
        el.style.shadow_blur = want_blur;
        el.style.shadow_spread = want_spread;

        if dragging || target_changed || shadow_changed {
            last_target.set(Some(target));
            last_shadow_a.set(Some(want_shadow[3]));
            impact = impact.merge(LayoutStatus::StyleChanged);
        }
        reset_toast_descendant_scales(el);

        // 只按本卡 CSS 是否在跑来要帧；勿用 any_ui_animation_active，否则一张在动
        // 会让堆里每张每帧都走重路径。
        let self_animating = !dragging
            && (el.transition_snapshot.has_active_tweens()
                || crate::style::css_transition_needs_frame(
                    &el.style,
                    &el.transition_snapshot,
                ));
        if self_animating || impact != LayoutStatus::Clean {
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        }
        impact
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
        if (child.style.scale[0] - 1.0).abs() > 0.001
            || (child.style.scale[1] - 1.0).abs() > 0.001
        {
            child.style.scale = [1.0, 1.0];
        }
        reset_toast_descendant_scales(child);
    }
}

/// 声明式条目壳（语气变体）；一般由宿主渲染队列，亦可自定义。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Toast(
    variant: Option<String>,
    title: Option<String>,
    description: Option<String>,
    icon: Option<bool>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let tone = ToastTone::parse(variant.as_deref().unwrap_or("default"));
    let item = ToastItem {
        id: 0,
        title: title.unwrap_or_default(),
        description: description.unwrap_or_default(),
        tone,
        show_icon: icon,
        created: Instant::now(),
        duration: Duration::from_secs(3),
        exiting: false,
        exit_started: None,
    };
    build_toast_card(
        &item,
        ToastSwipeDirs::for_position(ToastPosition::TopRight),
        None,
        false,
    )
}

/// 标题叶（文档/自定义组合用）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToastTitle(children: Option<Element>) -> Element {
    let label = children
        .as_ref()
        .and_then(|c| c.text.clone())
        .unwrap_or_default();
    text(label.as_str())
        .size(14.0)
        .weight(600.0)
        .color(tokens::foreground_text())
        .into()
}

/// 描述叶。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToastDescription(children: Option<Element>) -> Element {
    let label = children
        .as_ref()
        .and_then(|c| c.text.clone())
        .unwrap_or_default();
    text(label.as_str())
        .size(14.0)
        .color(parse_text_color("muted-foreground"))
        .into()
}

/// 关闭按钮（需自行 `on_click` dismiss；队列卡片已内置关闭）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToastClose(children: Option<Element>) -> Element {
    let _ = children;
    let g = lucide_glyph("x").unwrap_or_else(|| "×".into());
    let icon = build_icon_text_node(g)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"))
        .w(16.0)
        .h(16.0);
    interactive(r#box().p("0.25rem").rounded("0.375rem").child(icon).into()).into()
}

// ---------------------------------------------------------------------------
// Host layout
// ---------------------------------------------------------------------------

fn place_panel(panel: Element, position: &str) -> Element {
    let pad = 16.0;
    let pos = ToastPosition::parse(position);
    match pos {
        ToastPosition::TopLeft => panel.absolute().left(pad).top(pad),
        ToastPosition::TopRight => panel.absolute().right(pad).top(pad),
        ToastPosition::BottomLeft => panel.absolute().left(pad).bottom(pad),
        ToastPosition::BottomRight => panel.absolute().right(pad).bottom(pad),
        ToastPosition::TopCenter => Element::from(
            hstack()
                .absolute(true)
                .left(0.0)
                .right(0.0)
                .top(pad)
                .w("100%")
                .justify("center")
                .pointer_events("box-none")
                .child(panel),
        ),
        ToastPosition::BottomCenter => Element::from(
            hstack()
                .absolute(true)
                .left(0.0)
                .right(0.0)
                .bottom(pad)
                .w("100%")
                .justify("center")
                .pointer_events("box-none")
                .child(panel),
        ),
    }
}

/// Sonner 式堆叠：收起时仅窥视后方条目；悬停展开为完整间距列表。
/// `items` 新→旧（index 0 = 最前）。
/// 布局：同锚点 + `translateY` 堆叠（避免中心缩放 + 不同 `top` 导致窥视条不匀）。
/// 后方到期：整卡淡出；最前到期：滑出淡出。
/// 悬停命中区始终按**展开高度**计算，避免折叠高度与卡片动画不同步导致 expand 抖动。
fn render_stack(
    items: &[&ToastItem],
    expanded: bool,
    from_bottom: bool,
    swipe: ToastSwipeDirs,
) -> Element {
    let n = items.len();
    if n == 0 {
        return Element::new();
    }

    // 仅非退场项参与堆叠位次与堆高度（退场后方不占位，对标 Sonner 立即收高度）。
    let mut rank_of = vec![None; n];
    let mut n_active = 0usize;
    for i in 0..n {
        if !items[i].exiting {
            rank_of[i] = Some(n_active);
            n_active += 1;
        }
    }
    let has_front_exit = items
        .iter()
        .any(|t| t.exiting && toast_exit_vis(t.id).is_some_and(|v| v.as_front));
    let pile_n = n_active.max(usize::from(has_front_exit));

    let pile_h_expanded =
        pile_n as f32 * CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_EXPAND_GAP;
    // 收起时命中区跟窥视堆高度走，避免展开幽灵高度拖住 hover 导致「移开却不收」。
    // 展开后用满高，保证卡间空隙仍可命中。
    let pile_h_collapsed =
        CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_PEEK;
    let pile_h = if expanded {
        pile_h_expanded
    } else {
        pile_h_collapsed
    };
    let anchor_top = if from_bottom { pile_h - CARD_H } else { 0.0 };

    let mut pile: Element = r#box()
        .relative(true)
        .w(TOAST_WIDTH)
        .h(pile_h)
        .overflow("visible")
        .into();
    // 与 CarouselStack 相同：逐卡 rects→text band，避免下层字形画在上层壳之上。
    // （框架也会在子节点有 z-index 时自动建 band；此处显式标注语义。）
    pile.set_attr("data-stacking-context", "1");

    // 从后往前挂：先画最旧（底），再画最新（顶），保证命中/绘制顺序。
    for draw_i in (0..n).rev() {
        let item = items[draw_i];
        let exit_vis = toast_exit_vis(item.id);
        let behind_fade = item.exiting && !expanded && exit_vis.is_some_and(|v| !v.as_front);

        let (rank, target_scale, behind, pointer_none, z) = if item.exiting {
            if let Some(v) = exit_vis {
                if v.as_front || expanded {
                    (0usize, 1.0, false, true, 2000)
                } else {
                    let rank = v.rank;
                    let scale = (1.0 - rank as f32 * 0.05).max(0.90);
                    (rank, scale, true, true, 900 - rank as i32)
                }
            } else {
                (0usize, 1.0, false, true, 2000)
            }
        } else {
            let rank = rank_of[draw_i].unwrap_or(0);
            let scale = if expanded {
                1.0
            } else {
                (1.0 - rank as f32 * 0.05).max(0.90)
            };
            let is_behind = !expanded && rank > 0;
            (rank, scale, is_behind, is_behind, 1000 - rank as i32)
        };

        let base_lift = if expanded {
            let step = CARD_H + STACK_EXPAND_GAP;
            if from_bottom {
                -(rank as f32 * step)
            } else {
                rank as f32 * step
            }
        } else if from_bottom {
            -(rank as f32 * STACK_PEEK)
        } else {
            rank as f32 * STACK_PEEK
        };

        // 后方窥视条不重播入场滑入（晋升 front 时再播）。
        if behind {
            let _ = take_toast_needs_enter(item.id);
        }
        // 整壳裁切到 front 同高；文案始终绘制，靠 overflow 裁切窥视。
        let collapse_h = Some(CARD_H);
        let mut card = build_toast_card(item, swipe, collapse_h, behind);
        card = apply_enter_motion(
            card,
            item,
            from_bottom,
            behind_fade,
            Some(ToastStackMotion {
                target_lift: base_lift,
                target_scale,
                behind_fade,
                dim_shadow: behind,
            }),
        );
        card = card
            .absolute()
            .left(0.0)
            .w("100%")
            .top(anchor_top)
            .z_index(z);
        if pointer_none {
            card = card.pointer_events_none();
        }
        pile = pile.child(card);
    }

    let expanded_sig = expanded_signal();
    interactive(Element::from(pile))
        .on_hover(move |h| {
            if is_toast_dragging() {
                if h {
                    expanded_sig.set(true);
                    set_stack_timers_paused(true);
                }
                return;
            }
            expanded_sig.set(h);
            set_stack_timers_paused(h);
        })
        .into()
}

/// list：绝对定位 + translateY 占位（与 stack 同口径），index 变化才有过渡。
/// `items`：顶→底视觉序。底栏传入 oldest→newest（最上最旧）；顶栏 newest→oldest。
/// 挤出最旧：向入场推移方向再移一档（底栏向上 / 顶栏向下）并淡出。
fn render_list(items: &[&ToastItem], from_bottom: bool, swipe: ToastSwipeDirs) -> Element {
    let n = items.len();
    if n == 0 {
        return Element::new();
    }

    let step = list_step();
    let mut rank_of = vec![None; n];
    let actives: Vec<usize> = (0..n).filter(|&i| !items[i].exiting).collect();
    let n_active = actives.len();
    if from_bottom {
        // oldest→newest：末项最新 → rank 0（贴底边）。
        for (k, &i) in actives.iter().rev().enumerate() {
            rank_of[i] = Some(k);
        }
    } else {
        // newest→oldest：首项最新 → rank 0（贴顶边）。
        for (k, &i) in actives.iter().enumerate() {
            rank_of[i] = Some(k);
        }
    }

    let has_front_exit = items
        .iter()
        .any(|t| t.exiting && toast_exit_vis(t.id).is_some_and(|v| v.as_front));
    let pile_n = n_active.max(usize::from(has_front_exit));
    let pile_h = pile_n as f32 * CARD_H + (pile_n.saturating_sub(1) as f32) * LIST_GAP;
    let anchor_top = if from_bottom { pile_h - CARD_H } else { 0.0 };

    let mut pile: Element = r#box()
        .relative(true)
        .w(TOAST_WIDTH)
        .max_w("420px")
        .h(pile_h)
        .overflow("visible")
        .into();

    // 先画高 rank（更远），再画 rank0，保证命中/绘制。
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| {
        let item = items[i];
        let r = if item.exiting {
            toast_exit_vis(item.id).map(|v| v.rank).unwrap_or(0)
        } else {
            rank_of[i].unwrap_or(0)
        };
        std::cmp::Reverse(r)
    });

    for draw_i in order {
        let item = items[draw_i];
        let exit_vis = toast_exit_vis(item.id);
        let as_front = exit_vis.is_some_and(|v| v.as_front);
        // 挤出 / 非最前关闭：按档位平移淡出；最前关闭：滑向入场边。
        let list_slot_fade = item.exiting && !as_front;

        let rank = if item.exiting {
            if as_front {
                0usize
            } else {
                exit_vis.map(|v| v.rank).unwrap_or_else(visible_limit)
            }
        } else {
            rank_of[draw_i].unwrap_or(0)
        };

        let base_lift = if from_bottom {
            -(rank as f32 * step)
        } else {
            rank as f32 * step
        };

        let mut card = build_toast_card(item, swipe, None, false);
        card = apply_enter_motion(
            card,
            item,
            from_bottom,
            list_slot_fade,
            Some(ToastStackMotion {
                target_lift: base_lift,
                target_scale: 1.0,
                behind_fade: list_slot_fade,
                dim_shadow: false,
            }),
        );
        let z = 1000i32 - rank as i32;
        card = card
            .absolute()
            .left(0.0)
            .w("100%")
            .top(anchor_top)
            .z_index(z);

        let id = item.id;
        let card: Element = if item.exiting {
            card.pointer_events_none()
        } else {
            interactive(card)
                .on_hover(move |h| {
                    set_item_timer_paused(id, h);
                })
                .into()
        };
        pile = pile.child(card);
    }

    Element::from(pile)
}

fn render_queue(layout: ToastLayout, position: &str, q: &[ToastItem], expanded: bool) -> Element {
    let limit = visible_limit();
    let active_ids: std::collections::HashSet<u64> = q
        .iter()
        .rev()
        .filter(|t| !t.exiting)
        .take(limit)
        .map(|t| t.id)
        .collect();
    let newest_first: Vec<&ToastItem> = q
        .iter()
        .rev()
        .filter(|t| t.exiting || active_ids.contains(&t.id))
        .collect();
    if newest_first.is_empty() {
        return Element::new();
    }

    let pos = ToastPosition::parse(position);
    let bottom = pos.is_bottom();
    let swipe = ToastSwipeDirs::for_position(pos);
    let expanded = expanded || is_toast_dragging();
    let body = match layout {
        ToastLayout::Stack => render_stack(&newest_first, expanded, bottom, swipe),
        ToastLayout::List => {
            if bottom {
                let oldest_first: Vec<&ToastItem> = newest_first.into_iter().rev().collect();
                render_list(&oldest_first, bottom, swipe)
            } else {
                render_list(&newest_first, bottom, swipe)
            }
        }
    };

    place_panel(r#box().child(body).into(), position)
}

/// Builder：配置宿主布局/锚点/上限策略。
pub struct ToasterBuilder {
    variant: ToastLayout,
    position: String,
    overflow: ToastOverflow,
    limit: usize,
}

pub fn toaster() -> ToasterBuilder {
    ToasterBuilder {
        variant: ToastLayout::Stack,
        position: "top-center".into(),
        overflow: ToastOverflow::Displace,
        limit: DEFAULT_LIMIT,
    }
}

/// 兼容旧名：默认 `stack` 宿主。
pub fn toast_host() -> Element {
    toaster().into()
}

impl ToasterBuilder {
    /// `stack`（默认）| `list`。切换时套用布局默认锚点（stack→顶中，list→右下）。
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = ToastLayout::parse(&v.into());
        self.position = match self.variant {
            ToastLayout::List => ToastPosition::BottomRight.as_str().to_string(),
            ToastLayout::Stack => ToastPosition::TopCenter.as_str().to_string(),
        };
        self
    }
    pub fn layout(mut self, layout: ToastLayout) -> Self {
        self.variant = layout;
        self.position = match layout {
            ToastLayout::List => ToastPosition::BottomRight.as_str().to_string(),
            ToastLayout::Stack => ToastPosition::TopCenter.as_str().to_string(),
        };
        self
    }
    /// `top-center`（stack 默认）| `top-right` | `top-left` | `bottom-left` | `bottom-center` | `bottom-right`（list 默认）。
    pub fn position(mut self, p: impl Into<String>) -> Self {
        self.position = ToastPosition::parse(&p.into()).as_str().to_string();
        self
    }
    /// 上限策略：`displace`（挤出最旧）| `wait`（等待空位）。
    pub fn overflow(mut self, v: impl Into<String>) -> Self {
        self.overflow = ToastOverflow::parse(&v.into());
        self
    }
    pub fn overflow_policy(mut self, policy: ToastOverflow) -> Self {
        self.overflow = policy;
        self
    }
    /// 同时可见上限（默认 3，至少 1）。
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = n.max(1);
        self
    }
}

impl crate::core::tree::IntoElementList for ToasterBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ToasterBuilder> for Element {
    fn from(builder: ToasterBuilder) -> Self {
        layout_signal().set(builder.variant.as_str().to_string());
        position_signal().set(ToastPosition::parse(&builder.position).as_str().to_string());
        overflow_signal().set(builder.overflow.as_str().to_string());
        limit_signal().set(builder.limit.max(1));
        match builder.overflow {
            ToastOverflow::Displace => flush_pending_by_displace(),
            ToastOverflow::Wait => promote_pending(),
        }
        mount_toast_host()
    }
}

const TOAST_HOST_VIEW: &str = "__rui_toast_host_view";

/// 反应根必须是**非 portal** 外壳。若把 `rui_portal` 当 `__rui_toast_host_view` 根，
/// hoist 后身份落到 portal 层；Command Modal 关闭再调和会找不到壳，后续通知全部不上屏。
fn toast_host_idle() -> Element {
    Element::new().w(0.0).h(0.0).pointer_events_none()
}

/// 铺满视口的舞台（提升到窗口根 portal 层）；卡片在其内按 `position` 锚点摆放。
fn toast_viewport_stage(panel: Element) -> Element {
    let mut stage: Element = Element::new()
        .absolute()
        .inset(0.0)
        .w("100%")
        .h("100%")
        .pointer_events_box_none()
        .child(panel);
    stage.set_attr(ATTR_PORTAL_KEY, "toast-host");
    portal_panel(stage, TOAST_PORTAL_Z)
}

pub(crate) fn paint_toast_host() -> Element {
    let open = use_signal(TOAST_HOST_OPEN, || true);
    let _ = tick_toast_lifecycle();
    let q = toast_queue().get();
    let _ = open.get();
    let _ = toast_pending().get(); // 等待队列变化时重绘（角标/状态）
    let layout = ToastLayout::parse(&layout_signal().get());
    let position = position_signal().get();
    let expanded = expanded_signal().get() || is_toast_dragging();
    let _ = overflow_signal().get();
    let _ = limit_signal().get();
    // 与 Tooltip 相同：Pinned 登记，不进 Modal 栈。关 Command 不得把队列藏起来。
    let _ = portal_sync(
        "toast-host",
        open,
        OverlayKind::Tooltip,
        OverlayMode::Pinned,
    );
    if q.is_empty() && toast_pending().peek().is_empty() {
        if expanded_signal().peek() {
            expanded_signal().set(false);
        }
        set_stack_timers_paused(false);
        return toast_host_idle();
    }
    if q.is_empty() {
        sync_pending_when_visible_empty(&q);
        return toast_host_idle();
    }
    let panel = render_queue(layout, position.as_str(), &q, expanded);
    toast_host_idle().child(toast_viewport_stage(panel))
}

fn mount_toast_host() -> Element {
    reactive_anchor(TOAST_HOST_VIEW, paint_toast_host);
    // 锚点必须是子节点：`ui! { <Toaster/> }` 会用 file:line:column 盖掉根 template_id。
    // 0×0 外壳不占 flex；真正的舞台 hoist 到窗口根 `__rui_portal_layer`。
    Element::new()
        .w(0.0)
        .h(0.0)
        .pointer_events_box_none()
        .child(reactive_placeholder_sized(TOAST_HOST_VIEW, 0.0, 0.0).pointer_events_box_none())
}

/// RSX `<Toaster variant="stack" overflow="displace" limit={3} position="top-right" />`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Toaster(
    variant: Option<String>,
    position: Option<String>,
    overflow: Option<String>,
    limit: Option<i32>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let mut b = toaster();
    if let Some(v) = variant {
        b = b.variant(v);
    }
    if let Some(p) = position {
        b = b.position(p);
    }
    if let Some(o) = overflow {
        b = b.overflow(o);
    }
    if let Some(n) = limit {
        b = b.limit(n.max(1) as usize);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settle_toast_exits() {
        let mut q = toast_queue().get();
        let mut p = toast_pending().get();
        let past = Instant::now() - Duration::from_secs(1);
        for t in &mut q {
            if t.exiting {
                t.exit_started = Some(past);
            }
        }
        let _ = finalize_exits(&mut q, &mut p);
        toast_queue().set(q);
        toast_pending().set(p);
        promote_pending();
    }

    #[test]
    fn stack_limit_three_keeps_three_active() {
        toast_set_overflow("displace");
        toast_set_limit(3);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let _a = toast("A").duration(10_000).show();
        let _b = toast("B").duration(10_000).show();
        let _c = toast("C").duration(10_000).show();
        assert_eq!(active_count(&toast_queue().get()), 3);
        let _d = toast("D").duration(10_000).show();
        let q = toast_queue().get();
        assert_eq!(active_count(&q), 3, "displace must free a slot for D");
        assert_eq!(
            q.iter().filter(|t| t.exiting).count(),
            1,
            "oldest should be exiting"
        );
        assert!(q.iter().any(|t| t.id == _d && !t.exiting));
        settle_toast_exits();
        toast_set_limit(3);
    }

    #[test]
    fn toast_portal_sits_above_stacked_modals() {
        assert!(TOAST_PORTAL_Z > 1000 + 10 * 20);
    }

    #[test]
    fn exit_moves_back_toward_enter_edge() {
        // 顶栏：入场从上方（负 Y）滑入 → 退场继续向负 Y（回入场边）。
        assert!(toast_enter_offset_y(false) < 0.0);
        assert!(toast_exit_offset_y(false) < 0.0);
        // 底栏：入场从下方（正 Y）滑入 → 退场继续向正 Y。
        assert!(toast_enter_offset_y(true) > 0.0);
        assert!(toast_exit_offset_y(true) > 0.0);
        // 无拖拽时 front 终点。
        assert_eq!(exit_target([0.0, 0.0], false)[1], toast_exit_offset_y(false));
        assert_eq!(exit_target([0.0, 0.0], true)[1], toast_exit_offset_y(true));
    }

    #[test]
    fn behind_exit_stays_in_place() {
        let (x, y, op, _) = toast_motion_pose(
            1,
            true,
            false,
            true,
            Some(&ToastStackMotion {
                target_lift: 14.0,
                target_scale: 0.95,
                behind_fade: true,
                dim_shadow: true,
            }),
            [0.0, 0.0],
        );
        let lift = stack_lift_visual(false, 14.0, 0.95);
        assert_eq!(x, 0.0);
        assert!((y - lift).abs() < 0.01, "behind exit must not nudge, y={y} lift={lift}");
        assert_eq!(op, 0.0);
    }

    #[test]
    fn list_displace_exit_moves_one_slot_along_enter_axis() {
        toast_set_layout("list");
        toast_set_overflow("displace");
        toast_set_limit(2);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let _a = toast("A").duration(10_000).show();
        let _b = toast("B").duration(10_000).show();
        let _c = toast("C").duration(10_000).show();
        let q = toast_queue().get();
        let a = q.iter().find(|t| t.title == "A").expect("A");
        assert!(a.exiting);
        let vis = toast_exit_vis(a.id).expect("vis");
        assert!(vis.to_pending);
        assert_eq!(vis.rank, 2);
        // 底栏 list：rank↑ → lift 更负（向上 +1 档）。
        let step = list_step();
        let lift = -(vis.rank as f32 * step);
        let (x, y, op, sc) = toast_motion_pose(
            a.id,
            true,
            true,
            true,
            Some(&ToastStackMotion {
                target_lift: lift,
                target_scale: 1.0,
                behind_fade: true,
                dim_shadow: false,
            }),
            [0.0, 0.0],
        );
        assert_eq!(x, 0.0);
        assert!((y - lift).abs() < 0.01, "y={y} lift={lift}");
        assert_eq!(op, 0.0);
        assert!((sc - 1.0).abs() < 0.001);
        toast_set_layout("stack");
    }

    #[test]
    fn displace_exit_targets_limit_plus_one_slot() {
        toast_set_overflow("displace");
        toast_set_limit(3);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let _a = toast("A").duration(10_000).show();
        let _b = toast("B").duration(10_000).show();
        let _c = toast("C").duration(10_000).show();
        let _d = toast("D").duration(10_000).show();
        let q = toast_queue().get();
        let a = q.iter().find(|t| t.title == "A").expect("A");
        assert!(a.exiting, "oldest should be displace-exiting");
        let vis = toast_exit_vis(a.id).expect("exit vis");
        assert!(vis.to_pending);
        assert_eq!(
            vis.rank, 3,
            "displace rank must be visible_limit (= stack cap + 1 slot, 0-based)"
        );
        let scale = (1.0 - 3.0_f32 * 0.05).max(0.90);
        let lift_raw = 3.0 * STACK_PEEK;
        let (x, y, op, sc) = toast_motion_pose(
            a.id,
            true,
            false,
            true,
            Some(&ToastStackMotion {
                target_lift: lift_raw,
                target_scale: scale,
                behind_fade: true,
                dim_shadow: true,
            }),
            [0.0, 0.0],
        );
        let lift = stack_lift_visual(false, lift_raw, scale);
        assert_eq!(x, 0.0);
        assert!((y - lift).abs() < 0.01);
        assert_eq!(op, 0.0);
        assert!((sc - scale).abs() < 0.001);
    }

    fn host_has_toast_portal(el: &Element) -> bool {
        fn walk(el: &Element) -> bool {
            if el.attrs.get("rui_portal").map(String::as_str) == Some("1")
                && el.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
            {
                return true;
            }
            el.children.iter().any(walk)
        }
        el.attrs.get("rui_portal").map(String::as_str) != Some("1") && walk(el)
    }

    fn find_tid(el: &Element, id: &str) -> bool {
        el.template_id.as_deref() == Some(id) || el.children.iter().any(|c| find_tid(c, id))
    }

    #[test]
    fn toast_host_root_is_a_box_none_shell_not_the_portal() {
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, el) = crate::core::reactivity::create_root(paint_toast_host);
        assert_ne!(
            el.attrs.get("rui_portal").map(String::as_str),
            Some("1"),
            "reactive toast root must stay in-tree so Command close can still find it"
        );
    }

    #[test]
    fn toaster_anchor_survives_ui_root_template_stamp() {
        crate::app::reactive::reset_registered_anchors();
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, stamped) = crate::core::reactivity::create_root(|| {
            let host: Element = toaster().into();
            host.template_id("examples/gallery/src/main.rs:3729:9")
        });
        assert_eq!(
            stamped.template_id.as_deref(),
            Some("examples/gallery/src/main.rs:3729:9"),
            "ui! stamps the wrapper"
        );
        assert!(
            find_tid(&stamped, TOAST_HOST_VIEW),
            "toast reactive placeholder must remain a descendant after ui! root stamp"
        );
        crate::app::reactive::reset_registered_anchors();
    }

    #[test]
    fn toast_stage_hoists_to_window_portal_layer() {
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, LayoutTree, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
        };
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, mut root) = crate::core::reactivity::create_root(|| {
            let _ = toast("Toast from Gallery")
                .description("Hover the stack to expand.")
                .duration(5000)
                .show();
            Element::new().w(800.0).h(600.0).child(paint_toast_host())
        });
        let _ = hoist_portal_subtrees(&mut root);
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
        let layer = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("window portal layer");
        let stage = layer
            .children
            .iter()
            .find(|c| {
                c.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
                    && c.attrs.get(ATTR_PORTAL_KEY).map(String::as_str) == Some("toast-host")
            })
            .expect("toast stage on window root");
        assert!(
            stage.layout_bounds[2] > 700.0 && stage.layout_bounds[3] > 500.0,
            "toast stage must fill the window portal layer, got {:?}",
            stage.layout_bounds
        );
    }

    #[test]
    fn toast_builder_and_push_share_the_same_host_portal() {
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, first) = crate::core::reactivity::create_root(|| {
            let _ = toast("Toast from Gallery")
                .description("Hover the stack to expand.")
                .duration(5000)
                .show();
            paint_toast_host()
        });
        assert!(host_has_toast_portal(&first));
        let (_owner, from_command) = crate::core::reactivity::create_root(|| {
            let _ = toast("Hello from Command")
                .description("Hover the stack to expand.")
                .duration(5000)
                .show();
            paint_toast_host()
        });
        assert!(
            host_has_toast_portal(&from_command),
            "command-triggered toasts must reuse the Toaster portal, not a second root"
        );
        let q = toast_queue().get();
        assert!(q.iter().any(|t| t.title == "Toast from Gallery"));
        assert!(q.iter().any(|t| t.title == "Hello from Command"));
    }

    #[test]
    fn toast_host_lifts_cards_as_child_portal() {
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, el) = crate::core::reactivity::create_root(|| {
            let _ = toast_push("Hello from Command", 2500);
            paint_toast_host()
        });
        assert!(
            host_has_toast_portal(&el),
            "toast cards must hoist from a child portal, not replace the host root"
        );
    }

    #[test]
    fn toast_still_paints_after_command_modal_closes() {
        use crate::app::portal_overlay::{
            clear_overlay_stack, portal_sync, OverlayKind, OverlayMode,
        };
        clear_overlay_stack();
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        let (_owner, _) = crate::core::reactivity::create_root(|| {
            let cmd = Signal::new(true);
            assert!(
                portal_sync("gal-cmd", cmd, OverlayKind::Modal, OverlayMode::Stacked).is_some()
            );
            let first = toast_push("Hello from Command", 2500);
            cmd.set(false);
            let _ = portal_sync("gal-cmd", cmd, OverlayKind::Modal, OverlayMode::Stacked);
            let painted = paint_toast_host();
            assert!(
                host_has_toast_portal(&painted),
                "first command toast must still paint after the modal leaves the overlay stack"
            );
            let second = toast_push("Copy", 1500);
            let painted = paint_toast_host();
            assert!(
                host_has_toast_portal(&painted),
                "follow-up toasts must keep painting"
            );
            assert!(toast_queue().get().iter().any(|t| t.id == first));
            assert!(toast_queue().get().iter().any(|t| t.id == second));
            clear_overlay_stack();
        });
    }

    #[test]
    fn toast_push_and_dismiss() {
        let id = toast_push("hi", 5000);
        assert!(toast_queue().get().iter().any(|t| t.id == id));
        toast_dismiss(id);
        assert!(
            toast_queue().get().iter().any(|t| t.id == id && t.exiting),
            "dismiss starts exit animation"
        );
        settle_toast_exits();
        assert!(!toast_queue().get().iter().any(|t| t.id == id));
    }

    #[test]
    fn toast_builder_tone_and_description() {
        let id = toast("Saved")
            .description("ok")
            .variant("success")
            .duration(2000)
            .show();
        let item = toast_queue()
            .get()
            .into_iter()
            .find(|t| t.id == id)
            .expect("queued");
        assert_eq!(item.title, "Saved");
        assert_eq!(item.description, "ok");
        assert_eq!(item.tone, ToastTone::Success);
        toast_dismiss(id);
        settle_toast_exits();
    }

    #[test]
    fn layout_parse_defaults_to_stack() {
        assert_eq!(ToastLayout::parse("stack"), ToastLayout::Stack);
        assert_eq!(ToastLayout::parse("list"), ToastLayout::List);
        assert_eq!(ToastLayout::parse("sonner"), ToastLayout::Stack);
        assert_eq!(ToastLayout::parse(""), ToastLayout::Stack);
    }

    #[test]
    fn toast_tone_unknown_falls_back_and_success_uses_soft_accent() {
        assert_eq!(ToastTone::parse("nope"), ToastTone::Default);
        assert_eq!(ToastTone::parse("info"), ToastTone::Info);
        assert_eq!(ToastTone::parse("error"), ToastTone::Destructive);
        assert_eq!(ToastTone::parse("warn"), ToastTone::Warning);
        let (bg, fg, border, icon) = tone_colors(ToastTone::Success);
        assert_eq!(bg, tokens::popover());
        assert_eq!(border, parse_color("green-300"));
        assert_eq!(fg, parse_text_color("green-600"));
        assert_eq!(icon, Some("circle-check"));
        let (_, _, _, info_icon) = tone_colors(ToastTone::Info);
        assert_eq!(info_icon, Some("info"));
        let (_, _, wborder, wicon) = tone_colors(ToastTone::Warning);
        assert_eq!(wborder, parse_color("orange-300"));
        assert_eq!(wicon, Some("circle-alert"));
        let (_, _, _, xicon) = tone_colors(ToastTone::Destructive);
        assert_eq!(xicon, Some("circle-x"));
        let (sc, _, _) = tone_shadow(ToastTone::Success);
        assert!(
            sc[1] > sc[0] && sc[1] > sc[2] && (sc[3] - 0.12).abs() < 0.001,
            "success shadow must be green-tinted at default strength, got {sc:?}"
        );
        let (ic, _, _) = tone_shadow(ToastTone::Info);
        assert!(
            ic[2] > ic[0] && ic[2] > ic[1] && (ic[3] - 0.12).abs() < 0.001,
            "info shadow must be blue-tinted at default strength, got {ic:?}"
        );
    }

    #[test]
    fn toast_set_layout_switches_signal() {
        toast_set_layout("list");
        assert_eq!(
            ToastLayout::parse(&layout_signal().peek()),
            ToastLayout::List
        );
        toast_set_layout("stack");
        assert_eq!(
            ToastLayout::parse(&layout_signal().peek()),
            ToastLayout::Stack
        );
    }

    #[test]
    fn overflow_displace_caps_concurrent_exiting() {
        toast_set_overflow("displace");
        toast_set_limit(2);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);

        let a = toast("A").duration(10_000).show();
        let b = toast("B").duration(10_000).show();
        let c = toast("C").duration(10_000).show();
        assert_eq!(
            toast_queue().get().iter().filter(|t| t.exiting).count(),
            1,
            "first displace starts one exit"
        );
        assert!(
            toast_queue().get().iter().any(|t| t.id == a && t.exiting),
            "A should be exiting"
        );

        let d = toast("D").duration(10_000).show();
        let q = toast_queue().get();
        let p = toast_pending().get();
        // 挤出淡出与入场同步：允许并发两段 to_pending 退场，勿 snap 掉 A 的动画。
        assert_eq!(
            q.iter().filter(|t| t.exiting).count(),
            2,
            "A and B should both be fading (displace exits are not snapped early)"
        );
        assert!(
            p.iter().all(|t| t.id != a),
            "A must still be on-queue exiting, not snapped to pending"
        );
        assert!(q.iter().any(|t| t.id == a && t.exiting));
        assert!(q.iter().any(|t| t.id == b && t.exiting));
        assert!(q.iter().any(|t| t.id == c && !t.exiting));
        assert!(q.iter().any(|t| t.id == d && !t.exiting));

        settle_toast_exits();
        toast_set_limit(3);
    }

    #[test]
    fn overflow_displace_requeues_oldest_at_end() {
        toast_set_overflow("displace");
        toast_set_limit(2);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);

        let a = toast("A").duration(10_000).show();
        let b = toast("B").duration(10_000).show();
        let c = toast("C").duration(10_000).show();
        let q = toast_queue().get();
        assert_eq!(active_count(&q), 2);
        assert!(
            q.iter().any(|t| t.id == a && t.exiting),
            "A fades out synchronously with C entering"
        );
        assert!(q.iter().any(|t| t.id == b && !t.exiting));
        assert!(q.iter().any(|t| t.id == c && !t.exiting));
        assert!(
            toast_pending().get().is_empty(),
            "A moves to pending only after fade completes"
        );

        settle_toast_exits();
        assert!(!toast_queue().get().iter().any(|t| t.id == a));
        let p = toast_pending().get();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id, a, "A held in pending after fade");

        toast_dismiss(b);
        settle_toast_exits();
        let q = toast_queue().get();
        assert_eq!(active_count(&q), 2);
        assert!(q.iter().any(|t| t.id == c));
        assert_eq!(
            q.iter().filter(|t| !t.exiting).next().map(|t| t.id),
            Some(a),
            "A reappears at stack back (queue oldest) after a slot frees"
        );
        assert_eq!(
            q.iter().filter(|t| !t.exiting).last().map(|t| t.id),
            Some(c),
            "C stays front after pending promote"
        );
        assert!(toast_pending().get().is_empty());
        for id in [c, a] {
            toast_dismiss(id);
        }
        settle_toast_exits();
    }

    #[test]
    fn overflow_wait_queues_then_promotes() {
        toast_set_overflow("wait");
        toast_set_limit(1);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);

        let a = toast("A").duration(10_000).show();
        let b = toast("B").duration(10_000).show();
        assert_eq!(toast_queue().get().len(), 1);
        assert_eq!(toast_queue().get()[0].id, a);
        assert_eq!(toast_pending().get().len(), 1);
        assert_eq!(toast_pending().get()[0].id, b);

        toast_dismiss(a);
        settle_toast_exits();
        let q = toast_queue().get();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, b, "B promoted after A exit finishes");
        assert!(toast_pending().get().is_empty());
        toast_dismiss(b);
        settle_toast_exits();
        toast_set_overflow("displace");
        toast_set_limit(3);
    }

    #[test]
    fn timer_pauses_without_expiring() {
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        set_stack_timers_paused(false);

        let id = toast("hold").duration(500).show();
        assert!(!toast_timer_expired(id));
        set_item_timer_paused(id, true);
        std::thread::sleep(Duration::from_millis(600));
        assert!(
            !toast_timer_expired(id),
            "paused toast must not expire while frozen"
        );
        set_item_timer_paused(id, false);
        std::thread::sleep(Duration::from_millis(600));
        assert!(toast_timer_expired(id), "resumed toast eventually expires");
        toast_dismiss(id);
        settle_toast_exits();
    }

    #[test]
    fn demoted_pending_keeps_running_timer() {
        toast_set_overflow("displace");
        toast_set_limit(1);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        expanded_signal().set(false);
        set_stack_timers_paused(false);

        let a = toast("A").duration(800).show();
        let _b = toast("B").duration(10_000).show();
        assert!(
            toast_queue().get().iter().any(|t| t.id == a && t.exiting),
            "A displace-fading before pending"
        );
        assert!(
            toast_timer_deadline(a).is_some(),
            "displaced toast must keep its timer armed during fade"
        );
        settle_toast_exits();
        assert!(
            toast_pending().get().iter().any(|t| t.id == a),
            "A demoted to pending after fade"
        );
        set_item_timer_paused(a, false);
        std::thread::sleep(Duration::from_millis(900));
        let mut p = toast_pending().peek();
        assert!(
            prune_expired_pending_into(&mut p),
            "pending toast expires without waiting to re-enter"
        );
        toast_set_limit(3);
        let q = toast_queue().get();
        for t in &q {
            toast_dismiss(t.id);
        }
        settle_toast_exits();
    }

    #[test]
    fn overflow_parse() {
        assert_eq!(ToastOverflow::parse("displace"), ToastOverflow::Displace);
        assert_eq!(ToastOverflow::parse("wait"), ToastOverflow::Wait);
        assert_eq!(ToastOverflow::parse("queue"), ToastOverflow::Wait);
        assert_eq!(ToastOverflow::parse(""), ToastOverflow::Displace);
    }

    #[test]
    fn position_parse_six_corners() {
        assert_eq!(ToastPosition::parse("top-left"), ToastPosition::TopLeft);
        assert_eq!(ToastPosition::parse("top_center"), ToastPosition::TopCenter);
        assert_eq!(ToastPosition::parse("top-right"), ToastPosition::TopRight);
        assert_eq!(
            ToastPosition::parse("bottom-left"),
            ToastPosition::BottomLeft
        );
        assert_eq!(
            ToastPosition::parse("bottom-center"),
            ToastPosition::BottomCenter
        );
        assert_eq!(
            ToastPosition::parse("bottom_right"),
            ToastPosition::BottomRight
        );
        assert_eq!(ToastPosition::parse("nope"), ToastPosition::TopCenter);
        assert_eq!(ToastPosition::default(), ToastPosition::TopCenter);
        assert!(ToastPosition::BottomCenter.is_bottom());
        assert!(!ToastPosition::TopLeft.is_bottom());
    }

    #[test]
    fn corner_swipe_allows_two_edges() {
        let tr = ToastSwipeDirs::for_position(ToastPosition::TopRight);
        assert!(tr.up && tr.right && !tr.left && !tr.down);
        let bl = ToastSwipeDirs::for_position(ToastPosition::BottomLeft);
        assert!(bl.down && bl.left && !bl.up && !bl.right);
        let tc = ToastSwipeDirs::for_position(ToastPosition::TopCenter);
        assert!(tc.up && !tc.left && !tc.right && !tc.down);
        assert_eq!(clamp_swipe_offset(40.0, -30.0, tr), [40.0, -30.0]);
        assert_eq!(clamp_swipe_offset(-40.0, 30.0, tr), [0.0, 0.0]);
    }
}
