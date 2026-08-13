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
//!   `default` / `destructive` / `success` / `warning` / `error`。
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
//! - 宿主必须是 **绝对铺满** 的 overlay，勿参与 flex 分栏（见 gallery 回归）。
//! - 队列项 id 仅内部递增；关闭：关闭钮，或朝靠近边缘方向拖拽松手（四角两边、中位垂直反向）。
//! - 点击通知本体**不会**关闭；悬停时暂停自动消失计时。
//! - `position`：`top-left` / `top-center` / `top-right` / `bottom-left` / `bottom-center` / `bottom-right`。

use crate::app::pointer_events::{EventPhase, PointerEvent};
use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, r#box, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color, use_spring, use_ui_transition_spring};
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastTone {
    #[default]
    Default,
    Destructive,
    Success,
    Warning,
    Error,
}

impl ToastTone {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "destructive" => Self::Destructive,
            "success" => Self::Success,
            "warning" => Self::Warning,
            "error" => Self::Error,
            _ => Self::Default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Destructive => "destructive",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
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
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl ToastPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "top-left" | "tl" => Self::TopLeft,
            "top-center" | "top" | "tc" => Self::TopCenter,
            "bottom-left" | "bl" => Self::BottomLeft,
            "bottom-center" | "bottom" | "bc" => Self::BottomCenter,
            "bottom-right" | "br" => Self::BottomRight,
            // top-right / tr / default
            _ => Self::TopRight,
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
    /// 每次可见挂载递增，避免 demote→promote 复用旧入场弹簧。
    static TOAST_ENTER_MOUNT: RefCell<std::collections::HashMap<u64, u64>> =
        RefCell::new(std::collections::HashMap::new());
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
const TOAST_EXIT_MS: u64 = 280;
/// 收起堆叠中后方条目淡出时长（Sonner opacity ~200ms）。
const TOAST_BEHIND_EXIT_MS: u64 = 200;
const TOAST_EXIT_SLIDE: f32 = 72.0;

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
    TOAST_ENTER_MOUNT.with(|m| {
        let mut m = m.borrow_mut();
        let gen = m.entry(id).or_insert(0);
        *gen = gen.saturating_add(1);
    });
    TOAST_NEEDS_ENTER.with(|s| {
        s.borrow_mut().insert(id);
    });
}

fn toast_enter_mount(id: u64) -> u64 {
    TOAST_ENTER_MOUNT.with(|m| m.borrow().get(&id).copied().unwrap_or(0))
}

fn take_toast_needs_enter(id: u64) -> bool {
    TOAST_NEEDS_ENTER.with(|s| s.borrow_mut().remove(&id))
}

fn clear_toast_enter_state(id: u64) {
    TOAST_NEEDS_ENTER.with(|s| {
        s.borrow_mut().remove(&id);
    });
    TOAST_ENTER_MOUNT.with(|m| {
        m.borrow_mut().remove(&id);
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
        [
            0.0,
            if from_bottom {
                TOAST_EXIT_SLIDE
            } else {
                -TOAST_EXIT_SLIDE
            },
        ]
    }
}

fn begin_toast_exit(id: u64, from_drag: Option<[f32; 2]>) {
    let mut p = toast_pending().get();
    if p.iter().any(|t| t.id == id) {
        p.retain(|t| t.id != id);
        toast_pending().set(p);
        clear_toast_timer(id);
        clear_toast_enter_state(id);
        clear_toast_exit_vis(id);
        return;
    }

    let mut q = toast_queue().get();
    let as_front = visible_front_id(&q) == Some(id);
    let rank = visible_stack_rank(&q, id);
    let Some(t) = q.iter_mut().find(|t| t.id == id) else {
        return;
    };
    if t.exiting {
        return;
    }
    t.exiting = true;
    t.exit_started = Some(Instant::now());
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

    let started = prune_expired(&mut q);
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
const TOAST_WIDTH: &str = "22rem";
/// 卡片估计高度（标题+描述）；堆叠绝对定位用。
const CARD_H: f32 = 68.0;
/// 入场滑入距离（对标 Sonner 近乎整卡高度的 translateY）。
const TOAST_ENTER_SLIDE: f32 = CARD_H;
/// 后方淡出时的轻微下移（Sonner `translateY(40%)`）。
const TOAST_BEHIND_EXIT_NUDGE: f32 = CARD_H * 0.4;
/// 收起时后方条目露出的窥视高度（Sonner 观感）。
const STACK_PEEK: f32 = 14.0;
/// 悬停展开后条目间距。
const STACK_EXPAND_GAP: f32 = 10.0;

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
    use_signal(TOAST_POSITION_KEY, || "top-right".to_string())
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
pub fn toast_set_layout(variant: impl Into<String>) {
    let v = variant.into();
    layout_signal().set(ToastLayout::parse(&v).as_str().to_string());
    if ToastLayout::parse(&v) == ToastLayout::List {
        expanded_signal().set(false);
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

/// displace 挤出：最旧项留在可见队列淡出，与新通知入场同步；结束后再进 pending。
fn begin_displace_exit(q: &mut Vec<ToastItem>) -> bool {
    let Some(idx) = q.iter().position(|t| !t.exiting) else {
        return false;
    };
    let id = q[idx].id;
    let rank = visible_stack_rank(q, id);
    let now = Instant::now();
    let t = &mut q[idx];
    t.exiting = true;
    t.exit_started = Some(now);
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
    let pruned = prune_expired(&mut q);
    let mut p = toast_pending().peek();
    let _ = prune_expired_pending_into(&mut p);
    let finalized = finalize_exits(&mut q, &mut p);
    let pending_pruned = prune_expired_pending_into(&mut p);

    let mut demoted = false;
    while active_count(&q) > lim {
        if demote_oldest_active(&mut q, &mut p) {
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

fn prune_expired(q: &mut Vec<ToastItem>) -> bool {
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
        let Some(t) = q.iter_mut().find(|t| t.id == id) else {
            continue;
        };
        t.exiting = true;
        t.exit_started = Some(now);
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
                if started.elapsed() >= dur {
                    if vis.is_some_and(|v| v.to_pending) {
                        let mut item = t.clone();
                        item.exiting = false;
                        item.exit_started = None;
                        clear_toast_enter_state(item.id);
                        displaced.push(item);
                    } else {
                        clear_toast_timer(t.id);
                        clear_toast_enter_state(t.id);
                    }
                    clear_toast_exit_vis(t.id);
                    false
                } else {
                    true
                }
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
}

/// 以标题开始一条 toast（返回构建器；调用 [`ToastBuilder::show`] 入队）。
pub fn toast(message: impl Into<String>) -> ToastBuilder {
    ToastBuilder {
        title: message.into(),
        description: String::new(),
        tone: ToastTone::Default,
        duration_ms: 3000,
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
    /// 条目语气：`default` / `destructive` / `success` / `warning` / `error`。
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.tone = ToastTone::parse(&v.into());
        self
    }
    pub fn tone(mut self, t: ToastTone) -> Self {
        self.tone = t;
        self
    }
    pub fn duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms.max(500);
        self
    }
    /// 入队并返回 id。
    pub fn show(self) -> u64 {
        toast_push_item(self.title, self.description, self.tone, self.duration_ms)
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
        created: Instant::now(),
        duration,
        exiting: false,
        exit_started: None,
    };

    let lim = visible_limit();
    let policy = ToastOverflow::parse(&overflow_signal().peek());
    let mut q = toast_queue().get();
    let mut p = toast_pending().get();
    prune_expired(&mut q);
    finalize_exits(&mut q, &mut p);

    if active_count(&q) < lim {
        arm_toast_timer(id, duration);
        q.push(item);
        toast_queue().set(q);
        toast_pending().set(p);
    } else {
        match policy {
            ToastOverflow::Displace => {
                // 最旧可见淡出 → pending；新项同步入场。
                while active_count(&q) >= lim {
                    if !begin_displace_exit(&mut q) {
                        break;
                    }
                }
                arm_toast_timer(id, duration);
                q.push(item);
                toast_queue().set(q);
                toast_pending().set(p);
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
    match tone {
        ToastTone::Default => (
            tokens::popover(),
            tokens::foreground_text(),
            tokens::border(),
            None,
        ),
        ToastTone::Destructive | ToastTone::Error => (
            tokens::destructive(),
            tokens::destructive_foreground_text(),
            tokens::destructive(),
            Some("circle-x"),
        ),
        ToastTone::Success => (
            parse_color("green-600"),
            [255, 255, 255, 255],
            parse_color("green-700"),
            Some("circle-check"),
        ),
        ToastTone::Warning => (
            parse_color("amber-500"),
            [23, 23, 23, 255],
            parse_color("amber-600"),
            Some("triangle-alert"),
        ),
    }
}

/// 四边均匀阴影（offset 0,0）；blur 略收，避免堆叠时下方偏重。
fn tone_box_shadow(tone: ToastTone) -> &'static str {
    match tone {
        ToastTone::Default => "0 0 8px -1px #00000024",
        ToastTone::Destructive | ToastTone::Error => "0 0 8px -1px #ef444440",
        ToastTone::Success => "0 0 8px -1px #16a34a45",
        ToastTone::Warning => "0 0 8px -1px #f59e0b45",
    }
}

/// 收起堆叠后方层：更轻、更窄的均匀阴影，减轻窥视条视觉干扰。
fn tone_box_shadow_stacked(tone: ToastTone, behind: bool) -> &'static str {
    if !behind {
        return tone_box_shadow(tone);
    }
    match tone {
        ToastTone::Default => "0 0 5px -1px #00000014",
        ToastTone::Destructive | ToastTone::Error => "0 0 5px -1px #ef444428",
        ToastTone::Success => "0 0 5px -1px #16a34a28",
        ToastTone::Warning => "0 0 5px -1px #f59e0b28",
    }
}

/// `dirs`：关闭拖拽允许方向（四角两边，中位仅垂直反向）。
/// `content_opacity`：收起堆叠后方层把文案/按钮淡到 0（对标 Sonner），底壳仍不透明。
/// `collapse_h`：堆叠模式锁定与 front 同高，窥视条步长稳定（Sonner `--front-toast-height`）。
fn build_toast_card(
    item: &ToastItem,
    dirs: ToastSwipeDirs,
    content_opacity: f32,
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
        c[3] = (c[3] as f32 * 0.9) as u8;
        c
    };

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
        .items("start")
        .gap("0.75rem")
        .justify("between");

    if let Some(name) = icon_name {
        if let Some(g) = lucide_glyph(name) {
            let icon = build_icon_text_node(g)
                .text_size(16.0)
                .text_color(fg)
                .w(16.0)
                .h(16.0)
                .flex_shrink(0.0)
                .mt("0.125rem");
            row = row.child(icon);
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

    // 后方层：内容透明但节点仍在，晋升 front / 展开时弹簧淡回，避免换成空壳「空白」。
    let mut content: Element = Element::from(row);
    if content_opacity < 0.999 {
        content = content.opacity(content_opacity.clamp(0.0, 1.0));
    }

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
    card = card
        .bg(bg)
        .border(border, 1.0)
        .box_shadow(tone_box_shadow_stacked(item.tone, behind))
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

/// 入场：仅「新挂上可见队列」时自边缘整卡滑入（Sonner）；堆叠晋升 front 不重播。
/// 退场：最前滑向边缘并淡出；收起堆叠的后方条目仅淡出+轻移（不对顶层滑穿）。
fn apply_enter_motion(
    mut card: Element,
    item: &ToastItem,
    from_bottom: bool,
    behind_fade: bool,
) -> Element {
    let item_id = item.id;
    let mount = toast_enter_mount(item_id);
    let key = format!("rui:toast:enter:{item_id}:{mount}");
    let needs_enter = take_toast_needs_enter(item_id);
    let initial = if needs_enter {
        if from_bottom {
            TOAST_ENTER_SLIDE
        } else {
            -TOAST_ENTER_SLIDE
        }
    } else {
        0.0
    };
    // 入场用略弹的弹簧（对标 Sonner）；短 UI transition 偏「粘」不够滑入感。
    let ty = use_spring(&format!("{key}:ty"), initial);
    if !item.exiting {
        ty.set(0.0);
    }
    let enter_y = ty.get();

    let exit_p = use_ui_transition_spring(&format!("rui:toast:exit:{item_id}"), 0.0);
    if item.exiting {
        exit_p.set(1.0);
    } else {
        exit_p.set(0.0);
    }
    let et = exit_p.get();

    let drag = toast_drag_xy(item_id).get();
    let exit_from = toast_exit_from_xy(item_id).get();
    let [to_x, to_y] = exit_target(exit_from, from_bottom);

    let (x, y, opacity) = if item.exiting || et > 0.001 {
        if behind_fade {
            // Sonner collapsed behind: translateY(40%) + opacity 0，不整卡滑出。
            (
                0.0,
                enter_y + TOAST_BEHIND_EXIT_NUDGE * et,
                (1.0 - et).clamp(0.0, 1.0),
            )
        } else {
            (
                exit_from[0] + (to_x - exit_from[0]) * et,
                enter_y * (1.0 - et) + exit_from[1] + (to_y - exit_from[1]) * et,
                (1.0 - et).clamp(0.0, 1.0),
            )
        }
    } else {
        (drag[0], enter_y + drag[1], 1.0)
    };
    card = card.translate(x, y).opacity(opacity);
    card.template_id = Some(format!("toast-card-{item_id}"));
    // 退场中不再响应拖拽/点击关闭（关闭钮仍可点无妨；禁拖）。
    if item.exiting {
        card = card.pointer_events_none();
    }
    card
}

/// 声明式条目壳（语气变体）；一般由宿主渲染队列，亦可自定义。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Toast(
    variant: Option<String>,
    title: Option<String>,
    description: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let tone = ToastTone::parse(variant.as_deref().unwrap_or("default"));
    let item = ToastItem {
        id: 0,
        title: title.unwrap_or_default(),
        description: description.unwrap_or_default(),
        tone,
        created: Instant::now(),
        duration: Duration::from_secs(3),
        exiting: false,
        exit_started: None,
    };
    build_toast_card(
        &item,
        ToastSwipeDirs::for_position(ToastPosition::TopRight),
        1.0,
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
/// 后方到期：就地淡出；最前到期：滑出淡出。
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

    let pile_h = if expanded {
        pile_n as f32 * CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_EXPAND_GAP
    } else {
        CARD_H + (pile_n.saturating_sub(1) as f32) * STACK_PEEK
    };
    // 同锚点：顶栏贴顶、底栏贴底；堆叠位移走 translate（缩放绕中心时窥视条仍均匀）。
    let anchor_top = if from_bottom { pile_h - CARD_H } else { 0.0 };

    let mut pile = r#box()
        .relative(true)
        .w(TOAST_WIDTH)
        .h(pile_h)
        .overflow("visible");

    // 从后往前挂：先画最旧（底），再画最新（顶），保证命中/绘制顺序。
    for draw_i in (0..n).rev() {
        let item = items[draw_i];
        let exit_vis = toast_exit_vis(item.id);
        let behind_fade = item.exiting && !expanded && exit_vis.is_some_and(|v| !v.as_front);

        let (rank, target_scale, hide_content, pointer_none, z) = if item.exiting {
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
            // Sonner：收起时后方按 rank 缩小（约 5%/层），不要去掉。
            let scale = if expanded {
                1.0
            } else {
                (1.0 - rank as f32 * 0.05).max(0.90)
            };
            let hide = !expanded && rank > 0;
            let no_ptr = !expanded && rank > 0;
            (rank, scale, hide, no_ptr, 1000 - rank as i32)
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

        let spring_key = format!(
            "rui:toast:stack:{}:{}",
            item.id,
            if from_bottom { "b" } else { "t" }
        );
        let lift_s = use_ui_transition_spring(&format!("{spring_key}:lift"), base_lift);
        // 收起时后方缩小是 Sonner 特性，保留 scale 弹簧。
        let sc_s = use_ui_transition_spring(&format!("{spring_key}:sc"), target_scale);
        let content_s = use_ui_transition_spring(&format!("{spring_key}:content"), 1.0);
        if !behind_fade {
            lift_s.set(base_lift);
            sc_s.set(target_scale);
        }
        let scale = sc_s.get();
        let lift_raw = lift_s.get();
        // 中心缩放补偿：近侧边钉在 base_lift 上，使窥视步长≈STACK_PEEK（保留缩小）。
        let origin_fix = CARD_H * (1.0 - scale) * 0.5;
        let stack_lift = if from_bottom {
            lift_raw - origin_fix
        } else {
            lift_raw + origin_fix
        };
        // 仍接近 front 位时保留文案，避免「整卡空白」；到位后再藏字（Sonner 藏 children）。
        let content_target = if !hide_content {
            1.0
        } else if scale > 0.985 && lift_raw.abs() < STACK_PEEK * 0.45 {
            1.0
        } else {
            0.0
        };
        content_s.set(content_target);
        let content_opacity = content_s.get();
        // 堆叠始终锁 front 同高（对标 Sonner `--front-toast-height`）；悬停展开只拉开间距，不撑高单卡。
        let collapse_h = Some(CARD_H);

        if hide_content {
            let _ = take_toast_needs_enter(item.id);
        }
        let mut card = build_toast_card(item, swipe, content_opacity, collapse_h, hide_content);
        card = apply_enter_motion(card, item, from_bottom, behind_fade);
        let motion_t = card.style.translate;
        card = card
            .absolute()
            .left(0.0)
            .w("100%")
            .top(anchor_top)
            .translate(motion_t[0], motion_t[1] + stack_lift)
            .scale(scale)
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

fn render_list(items: &[&ToastItem], from_bottom: bool, swipe: ToastSwipeDirs) -> Element {
    let mut col = vstack().gap("0.5rem").w(TOAST_WIDTH).max_w("420px");
    for item in items {
        let id = item.id;
        let card = apply_enter_motion(
            build_toast_card(item, swipe, 1.0, None, false),
            item,
            from_bottom,
            false,
        )
        .w("100%");
        let card: Element = interactive(card)
            .on_hover(move |h| {
                set_item_timer_paused(id, h);
            })
            .into();
        col = col.child(card);
    }
    Element::from(col)
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
        position: "top-right".into(),
        overflow: ToastOverflow::Displace,
        limit: DEFAULT_LIMIT,
    }
}

/// 兼容旧名：默认 `stack` 宿主。
pub fn toast_host() -> Element {
    toaster().into()
}

impl ToasterBuilder {
    /// `stack`（默认）| `list`。
    pub fn variant(mut self, v: impl Into<String>) -> Self {
        self.variant = ToastLayout::parse(&v.into());
        self
    }
    pub fn layout(mut self, layout: ToastLayout) -> Self {
        self.variant = layout;
        self
    }
    /// `top-right`（默认）| `bottom-right` | `top-left` | `bottom-left` | `top-center` | `bottom-center`。
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

fn mount_toast_host() -> Element {
    let open = use_signal(TOAST_HOST_OPEN, || true);
    let host = reactive_view("__rui_toast_host_view", move || {
        let _ = tick_toast_lifecycle();
        let q = toast_queue().get();
        let _ = open.get();
        let _ = toast_pending().get(); // 等待队列变化时重绘（角标/状态）
        let layout = ToastLayout::parse(&layout_signal().get());
        let position = position_signal().get();
        let expanded = expanded_signal().get() || is_toast_dragging();
        let _ = overflow_signal().get();
        let _ = limit_signal().get();
        let Some(oz) = portal_sync(
            "toast-host",
            open,
            OverlayKind::Tooltip,
            OverlayMode::Pinned,
        ) else {
            return Element::new();
        };
        if q.is_empty() && toast_pending().peek().is_empty() {
            if expanded_signal().peek() {
                expanded_signal().set(false);
            }
            set_stack_timers_paused(false);
            return Element::new();
        }
        if q.is_empty() {
            sync_pending_when_visible_empty(&q);
            return Element::new();
        }
        let panel = render_queue(layout, position.as_str(), &q, expanded);
        portal_panel(panel, oz.panel)
    });
    host.absolute().inset(0.0).w("100%").h("100%")
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
        assert_eq!(ToastPosition::parse("nope"), ToastPosition::TopRight);
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
