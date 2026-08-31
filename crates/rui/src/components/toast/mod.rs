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
//! ## Module split (product layer, not framework primitives)
//! - [`timer`] / [`queue`] / [`swipe`] / [`motion`] / [`view`] / [`host`]：本组件私有拆层
//! - 退场时长判定吃 [`utils::presence::exit_elapsed`](crate::utils::presence::exit_elapsed)
//! - 入退场/堆叠位移吃 [`utils::motion::MotionStyle`](crate::utils::motion::MotionStyle)
//! - portal + 同 key 调和吃 [`utils::keyed_overlay`](crate::utils::keyed_overlay)
//! - 堆叠/列表 rank→位移吃 [`utils::layout_group`](crate::utils::layout_group)
//! - 拖拽会话登记见 [`utils::pointer_drag`](crate::utils::pointer_drag)
//!
//! ## Usage
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
//! ## Notes
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
//! - 堆叠悬停展开/收起：**不**重建宿主树（避免 3+ 卡时每卡 portal band + reconcile
//!   打穿帧率）；`reactive_sync` 读 `expanded` 热更新 lift/scale/阴影/命中高度。
//!   堆高度 `LayoutChanged` 由框架按 portal 局部 dirty 处理（不全窗 remasure / FullClear）。

use crate::app::pointer_events::{EventPhase, PointerEvent};
pub mod host;
mod motion;
mod queue;
mod swipe;
mod timer;
pub mod view;

use crate::core::reactivity::{batch, remove_named_signal, use_signal, Signal};
use crate::core::tree::{Element, LayoutStatus, PointerEvents, ATTR_POINTER_EVENTS};
use crate::layout::dimension::CssDim;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{
    parse_color, parse_text_color, TimingFunction, TransitionEntry, TransitionProperty, UnitSlot,
};
use crate::tokens;
use crate::utils::keyed_overlay::{item_template_id, TOAST_CARD_SCOPE};
use crate::utils::layout_group::linear_slot_offset;
use crate::utils::lucide_glyph;
use crate::utils::presence::exit_elapsed;
use queue::{max_displace_fades, non_displace_budget_full, MAX_CONCURRENT_EXITING, TOAST_EXIT_GAP};
use swipe::{clamp_swipe_offset, should_dismiss, ToastSwipeDirs};

use rui_macros::component;
use std::cell::{Cell, RefCell};
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

fn swipe_dirs_for_position(pos: ToastPosition) -> ToastSwipeDirs {
    match pos {
        ToastPosition::TopLeft => ToastSwipeDirs::for_edge(true, false, true, false),
        ToastPosition::TopCenter => ToastSwipeDirs::for_edge(true, false, false, false),
        ToastPosition::TopRight => ToastSwipeDirs::for_edge(true, false, false, true),
        ToastPosition::BottomLeft => ToastSwipeDirs::for_edge(false, true, true, false),
        ToastPosition::BottomCenter => ToastSwipeDirs::for_edge(false, true, false, false),
        ToastPosition::BottomRight => ToastSwipeDirs::for_edge(false, true, false, true),
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

/// 退场滑出动画时长（到时后真正出队）。
const TOAST_EXIT_MS: u64 = 320;
/// 收起堆叠中后方条目淡出时长（Sonner opacity ~200ms）。
const TOAST_BEHIND_EXIT_MS: u64 = 240;
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

/// 计时是否应冻结：堆叠布局下悬停展开或拖拽即冻结（Sonner 口径）。
///
/// 逐项对账与新条目上计时都以此为准，**勿**只读 [`TOAST_STACK_TIMER_PAUSED`]——
/// 那只是缓存，hover-out 掉帧时会滞后于真实展开态。
fn toast_timers_should_freeze() -> bool {
    is_stack_layout() && stack_expanded_now()
}

fn arm_toast_timer(id: u64, duration: Duration) {
    // 展开态下新到的通知同样生下来就冻结（悬停期间不该偷跑）；
    // 收起后由 `apply_stack_timer_pause` 逐项接续。
    let running = !(TOAST_STACK_TIMER_PAUSED.get() || toast_timers_should_freeze());
    timer::arm(id, duration, running);
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
    // 挤出可与入场重叠；上限 = limit+1，避免连推时瞬间堆出过多 portal band。
    let max_displace = max_displace_fades(visible_limit());
    while q
        .iter()
        .filter(|t| t.exiting && is_displace_exit(t.id))
        .count()
        >= max_displace
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

/// 非挤出退场（关闭/到期）是否已占满预算；占满时返回最晚一段的结束时刻。
///
/// [`prune_expired`] 用它把同帧其余到期项顺延，而不是 snap 掉正在播的那张。
/// 若在播的那段没有 `exit_started`（[`finalize_exits`] 永远摘不掉它），返回 `None`
/// 交回 [`ensure_exit_budget`] 走老路 snap，避免顺延变死等。
fn non_displace_exit_busy_until(q: &[ToastItem]) -> Option<Instant> {
    let mut until: Option<Instant> = None;
    let mut n = 0usize;
    for t in q.iter().filter(|t| t.exiting && !is_displace_exit(t.id)) {
        n += 1;
        let at = toast_exit_finish_at(t)?;
        until = Some(until.map_or(at, |u| u.max(at)));
    }
    if !non_displace_budget_full(n) {
        return None;
    }
    until
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
    timer::clear(id);
}

fn pause_toast_timer(id: u64) {
    timer::pause(id);
}

fn resume_toast_timer(id: u64) {
    timer::resume(id);
}

/// 到期但退场预算已满：顺延这条的到期时刻，**勿** snap 正在播的退场。
///
/// 见 [`prune_expired`]：一批同时到期时逐张顺延，才能一张接一张滑出。
fn defer_toast_timer(id: u64, delay: Duration) {
    // 冻结态下计时不会到期，理论上到不了这里；仍按冻结口径落盘，别偷跑。
    let running = !toast_timers_should_freeze();
    timer::defer(id, delay, running);
}

fn toast_timer_expired(id: u64) -> bool {
    timer::expired(id)
}

fn toast_timer_deadline(id: u64) -> Option<Instant> {
    timer::deadline(id)
}

fn begin_toast_drag(drag: ToastDrag) {
    toast_drag_xy(drag.id).set([0.0, 0.0]);
    let id = drag.id;
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::Toast,
        format!("toast:{id}"),
        drag.start_x,
        drag.start_y,
    );
    TOAST_DRAG.with(|d| *d.borrow_mut() = Some(drag));
    if is_stack_layout() {
        set_stack_expanded(true);
        set_stack_timers_paused(true);
    } else {
        set_item_timer_paused(id, true);
    }
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
    crate::utils::pointer_drag::clear();
    let finished = TOAST_DRAG.with(|d| d.borrow_mut().take());
    let Some(drag) = finished else {
        return;
    };
    crate::app::release_pointer_capture();
    let off = toast_drag_xy(drag.id).peek();
    let dismiss = should_dismiss(off);
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

/// 逐项同步冻结态。**勿**按 [`TOAST_STACK_TIMER_PAUSED`] 早退：
/// `displace` 挤出的条目在退场动画期间被 `t.exiting` 跳过、计时仍在跑，落进 pending
/// 后 `exiting` 又被清掉，只有逐项对账才能在展开期间把它一并冻住（否则它会在悬停
/// 期间悄悄到期，等收起时整批一起蒸发）。pause/resume 幂等，重复对账是廉价 no-op。
fn apply_stack_timer_pause(paused: bool, q: &[ToastItem], p: &[ToastItem]) {
    TOAST_STACK_TIMER_PAUSED.set(paused);
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

fn set_stack_timers_paused(paused: bool) {
    let q = toast_queue().peek();
    let p = toast_pending().peek();
    apply_stack_timer_pause(paused, &q, &p);
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
fn begin_toast_exit(id: u64, from_drag: Option<[f32; 2]>) {
    batch(|| {
        let mut p = toast_pending().peek();
        if p.iter().any(|t| t.id == id) {
            p.retain(|t| t.id != id);
            toast_pending().set(p);
            clear_toast_timer(id);
            dispose_toast_motion(id);
            clear_toast_enter_state(id);
            clear_toast_exit_vis(id);
            return;
        }

        let mut q = toast_queue().peek();
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
            set_stack_expanded(false);
        }
        toast_queue().set(q);
        toast_pending().set(p);
    });
}

/// 到期开始退场、退场结束出队；返回是否有队列变化（需重绘）。
///
/// 事件循环调用时必须通知队列订阅者，让 toast host 以新的 `exiting` 状态重新调和；
/// 单纯 `request_redraw` 只会重画旧 Element 树，新通知到来前不会出现退场动画。
pub(crate) fn tick_toast_lifecycle() -> bool {
    tick_toast_lifecycle_commit(true)
}

/// `notify=false` 仅供 [`paint_toast_host`] 的构建过程使用：该次构建随后会直接读取并
/// 渲染刚提交的队列，若再通知自身会造成同帧重复 reconcile。
fn tick_toast_lifecycle_commit(notify: bool) -> bool {
    let mut q = toast_queue().peek();
    let mut p = toast_pending().peek();

    // 无可见活跃项：先恢复 pending 计时并剔除到期项，避免暂停状态下永远无法 prune。
    if active_count(&q) == 0 {
        expanded_signal().set_silent(false);
        apply_stack_timer_pause(false, &q, &p);
        prune_expired_pending_into(&mut p);
    }

    // stack：以展开态为准同步冻结，避免 hover 重建闪断把计时误恢复。
    // 逐项对账（勿早退）：全局标志在 hover-out 掉帧时会滞后，且挤出→pending 的条目
    // 需要在这里补上冻结。
    if is_stack_layout() {
        apply_stack_timer_pause(toast_timers_should_freeze(), &q, &p);
    }

    let started = prune_expired(&mut q, &mut p);
    let removed = finalize_exits(&mut q, &mut p);
    let pending_pruned = prune_expired_pending_into(&mut p);
    let visible_empty_after_exit = active_count(&q) == 0;
    if started || removed || pending_pruned {
        if notify {
            batch(|| {
                toast_queue().set(q);
                toast_pending().set(p);
            });
        } else {
            let _ = toast_queue().try_set_silent(q);
            let _ = toast_pending().try_set_silent(p);
        }
    }
    if removed {
        if visible_empty_after_exit {
            sync_pending_when_visible_empty(&toast_queue().peek());
        }
        rebalance_visible_queue_commit(false);
        return true;
    }
    started || pending_pruned
}

/// 退场动画结束（应出队）的时刻：最前滑出与后方淡出时长不同。
///
/// [`finalize_exits`] 按同一口径摘树；[`toast_next_wake_at`] 与
/// [`non_displace_exit_busy_until`] 复用此处，勿各自重算时长。
fn toast_exit_finish_at(t: &ToastItem) -> Option<Instant> {
    let started = t.exit_started?;
    let behind_fade = toast_exit_vis(t.id).is_some_and(|v| !v.as_front);
    let ms = if behind_fade {
        TOAST_BEHIND_EXIT_MS
    } else {
        TOAST_EXIT_MS
    };
    Some(started + Duration::from_millis(ms))
}

/// 下一次应唤醒事件循环的时刻（到期或退场结束）。
pub(crate) fn toast_next_wake_at() -> Option<Instant> {
    let q = toast_queue().peek();
    let p = toast_pending().peek();
    if q.is_empty() && p.is_empty() {
        return None;
    }
    let mut next: Option<Instant> = None;
    for t in &q {
        if t.exiting {
            if let Some(at) = toast_exit_finish_at(t) {
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

/// 堆叠是否处于展开态（悬停或拖拽）。仅 `peek`，不订阅宿主 effect。
fn stack_expanded_now() -> bool {
    expanded_signal().peek() || is_toast_dragging()
}

/// 切换堆叠展开；宿主**不**订阅此信号（避免整树重建），故显式 `request_redraw`。
fn set_stack_expanded(expanded: bool) {
    let sig = expanded_signal();
    if sig.peek() == expanded {
        return;
    }
    sig.set(expanded);
    crate::app::runtime::request_redraw();
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
            // A layout switch can happen while the stack is hovered. Clear the
            // stack-wide pause latch and re-arm all existing timers before list
            // mode takes over (list mode does not run stack reconciliation).
            set_stack_timers_paused(false);
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
    rebalance_visible_queue_commit(true);
}

/// `notify`：是否唤醒宿主 effect。paint 路径内必须 silent，否则同帧二次 reconcile。
fn rebalance_visible_queue_commit(notify: bool) {
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
        if active_count(&q) == 0 {
            expanded_signal().set_silent(false);
            apply_stack_timer_pause(false, &q, &p);
            let _ = prune_expired_pending_into(&mut p);
        }
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

    let q_changed = pruned || finalized || demoted || promoted;
    let p_changed = demoted || promoted || finalized || pending_pruned;
    if notify {
        batch(|| {
            if q_changed {
                toast_queue().set(q);
            }
            if p_changed {
                toast_pending().set(p);
            }
            if promoted {
                use_signal(TOAST_HOST_OPEN, || true).set(true);
            }
        });
    } else {
        if q_changed {
            let _ = toast_queue().try_set_silent(q);
        }
        if p_changed {
            let _ = toast_pending().try_set_silent(p);
        }
        if promoted {
            let _ = use_signal(TOAST_HOST_OPEN, || true).try_set_silent(true);
        }
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
    let mut started_any = false;
    for (id, as_front, rank) in plans {
        // 预算已满：**勿**让 `ensure_exit_budget` snap 掉正在播的那张。一批同时到期
        // （连点推送、或悬停冻结后一起接续时必然如此）会把前 N-1 张无动画瞬间抹掉，
        // 看起来就是「原通知全部消失」。改为顺延到当前退场播完，逐张滑出。
        let busy_until = non_displace_exit_busy_until(q);
        if let Some(until) = busy_until {
            let delay = until.saturating_duration_since(now) + TOAST_EXIT_GAP;
            defer_toast_timer(id, delay);
            continue;
        }
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
        started_any = true;
    }
    // 全部顺延时队列未变：勿谎报变化，否则白跑一帧 reconcile。
    started_any
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
    expanded_signal().set_silent(false);
    let mut p = toast_pending().peek();
    apply_stack_timer_pause(false, q, &p);
    if prune_expired_pending_into(&mut p) {
        // paint / tick 路径：silent，避免同帧再跑宿主。
        let _ = toast_pending().try_set_silent(p);
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
                // 墙钟到点摘树（CSS transition 与时长对齐）；时长判定走 Presence 原语。
                if !exit_elapsed(started, dur) {
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
    let id = seq.peek() + 1;
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

    // 一次 push 会写 queue + pending + open；不 batch 时宿主 effect 连跑多次
    // reconcile（满员挤出时尤甚：3 活跃 + 1 挤出 × 每卡 portal band）。
    batch(|| {
        seq.set(id);
        let mut q = toast_queue().peek();
        let mut p = toast_pending().peek();
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
                    let mut p = toast_pending().peek();
                    arm_toast_timer(id, duration);
                    p.push(item);
                    toast_pending().set(p);
                }
            }
        }
        use_signal(TOAST_HOST_OPEN, || true).set(true);
    });
    id
}

pub fn toast_dismiss(id: u64) {
    begin_toast_exit(id, None);
}

#[cfg(test)]
mod tests {
    use super::host::{paint_toast_host, toaster, TOAST_HOST_VIEW};
    use super::motion::{
        exit_target, stack_lift_visual, toast_enter_offset_y, toast_exit_offset_y,
        toast_motion_pose, tone_shadow, ToastStackMotion,
    };
    use super::view::tone_colors;
    use super::*;
    use crate::layout::layout_tree::ATTR_PORTAL_KEY;

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
        assert_eq!(
            exit_target([0.0, 0.0], false)[1],
            toast_exit_offset_y(false)
        );
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
                live_rank: None,
            }),
            [0.0, 0.0],
        );
        let lift = stack_lift_visual(false, 14.0, 0.95);
        assert_eq!(x, 0.0);
        assert!(
            (y - lift).abs() < 0.01,
            "behind exit must not nudge, y={y} lift={lift}"
        );
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
                live_rank: None,
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
                live_rank: None,
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
    fn collapse_resumes_a_timer_frozen_past_its_deadline() {
        // 指针正好在到期瞬间压住堆叠：暂停会把剩余削到 `timer::TOAST_PAUSE_FLOOR`。收起后必须
        // 重新起表——否则 `running_since` 永远是 None、`toast_timer_expired` 恒 false，
        // 这条通知再也不会自动退场（悬停展开后移开鼠标即卡死的主症状）。
        toast_set_layout("stack");
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        expanded_signal().set(false);
        set_stack_timers_paused(false);

        let id = toast("stall").duration(500).show();
        std::thread::sleep(Duration::from_millis(560));
        assert!(
            toast_timer_expired(id),
            "500ms timer is due before the hover"
        );

        expanded_signal().set(true);
        let _ = tick_toast_lifecycle();
        assert!(
            !toast_timer_expired(id),
            "expanding must freeze even an already-due timer"
        );
        assert!(
            toast_timer_deadline(id).is_none(),
            "frozen timer has no deadline (nothing to wake the event loop for)"
        );

        expanded_signal().set(false);
        let _ = tick_toast_lifecycle();
        assert!(
            toast_timer_deadline(id).is_some(),
            "collapse must re-arm the frozen timer instead of leaving it dead"
        );
        std::thread::sleep(timer::TOAST_PAUSE_FLOOR + Duration::from_millis(60));
        assert!(
            toast_timer_expired(id),
            "resumed timer must expire so the toast leaves on its own"
        );
        toast_dismiss(id);
        settle_toast_exits();
    }

    #[test]
    fn expanded_stack_freezes_displaced_pending_timers() {
        // displace 挤出的条目在退场动画期间被 `exiting` 跳过（计时继续跑），落进 pending
        // 后 `exiting` 又被清掉。若冻结按全局标志早退，它会在悬停期间悄悄到期，收起那一
        // 刻整批一起蒸发（「重新打开新通知，原通知全部消失」的来源）。
        toast_set_layout("stack");
        toast_set_overflow("displace");
        toast_set_limit(1);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        expanded_signal().set(false);
        set_stack_timers_paused(false);

        let a = toast("A").duration(10_000).show();
        let _b = toast("B").duration(10_000).show();
        settle_toast_exits();
        assert!(
            toast_pending().get().iter().any(|t| t.id == a),
            "A demoted to pending after the displace fade"
        );
        assert!(
            toast_timer_deadline(a).is_some(),
            "pending timer runs while the stack is collapsed"
        );

        expanded_signal().set(true);
        let _ = tick_toast_lifecycle();
        assert!(
            toast_timer_deadline(a).is_none(),
            "expanding must freeze pending timers too, not only visible ones"
        );

        expanded_signal().set(false);
        let _ = tick_toast_lifecycle();
        assert!(
            toast_timer_deadline(a).is_some(),
            "collapsing resumes the pending timer"
        );

        toast_set_limit(3);
        let q = toast_queue().get();
        for t in &q {
            toast_dismiss(t.id);
        }
        let p = toast_pending().get();
        for t in &p {
            toast_dismiss(t.id);
        }
        settle_toast_exits();
    }

    #[test]
    fn same_frame_expiries_stagger_instead_of_snapping() {
        // 退场预算只有 1：一批同时到期时老路会 `snap_finalize_exit` 掉正在播的那张，
        // 前 N-1 张无动画瞬间抹掉——这就是「重新打开新通知，原通知全部消失」看到的样子。
        // 现在应只起一张退场，其余顺延到它播完。
        toast_set_overflow("displace");
        toast_set_limit(3);
        toast_queue().set(vec![]);
        toast_pending().set(vec![]);
        expanded_signal().set(false);
        set_stack_timers_paused(false);

        let a = toast("A").duration(500).show();
        let b = toast("B").duration(500).show();
        let c = toast("C").duration(500).show();
        std::thread::sleep(Duration::from_millis(560));

        let mut q = toast_queue().get();
        let mut p = toast_pending().get();
        assert_eq!(q.len(), 3, "all three visible before the batch expiry");
        assert!(prune_expired(&mut q, &mut p), "the oldest starts leaving");
        assert_eq!(
            q.len(),
            3,
            "no card may be snapped off-tree while another animates out"
        );
        assert_eq!(
            q.iter().filter(|t| t.exiting).count(),
            1,
            "exactly one exit animation at a time (MAX_CONCURRENT_EXITING)"
        );
        for id in [a, b, c] {
            if q.iter().any(|t| t.id == id && t.exiting) {
                continue;
            }
            assert!(
                !toast_timer_expired(id),
                "deferred card must stop being due, else it snaps the animating one"
            );
            assert!(
                toast_timer_deadline(id).is_some(),
                "deferred card keeps a running deadline so the loop wakes for it"
            );
        }

        toast_queue().set(q);
        toast_pending().set(p);
        for id in [a, b, c] {
            toast_dismiss(id);
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
        let tr = swipe_dirs_for_position(ToastPosition::TopRight);
        assert!(tr.up && tr.right && !tr.left && !tr.down);
        let bl = swipe_dirs_for_position(ToastPosition::BottomLeft);
        assert!(bl.down && bl.left && !bl.up && !bl.right);
        let tc = swipe_dirs_for_position(ToastPosition::TopCenter);
        assert!(tc.up && !tc.left && !tc.right && !tc.down);
        assert_eq!(clamp_swipe_offset(40.0, -30.0, tr), [40.0, -30.0]);
        assert_eq!(clamp_swipe_offset(-40.0, 30.0, tr), [0.0, 0.0]);
    }
}
