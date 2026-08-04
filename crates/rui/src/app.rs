use crate::core::tree::Element;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;

/// Wakes the event loop for hot reload (debug) and delivers [`accesskit_winit`] messages.
#[derive(Debug)]
pub(super) enum RuiUserEvent {
    #[cfg(debug_assertions)]
    HotReloadWake,
    AccessKit(#[allow(dead_code)] accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for RuiUserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        RuiUserEvent::AccessKit(e)
    }
}

mod input_controller;
mod hot_reload;
mod lifecycle;
pub mod runtime;
pub mod reactive;
pub mod window_keys;
mod scroll_support;
mod state;
mod tree_hit;
mod window_events;
pub mod pending_images;
pub mod portal_overlay;
pub mod navigator;
mod image_registry;
#[cfg(test)]
mod reactive_tests;

use scroll_support::ScrollbarDragState;
use state::AppState;
use crate::utils::drag_reorder::DragReorderState;

use crate::components::scroll_area::ScrollAreaSignalKeys;

type ScrollMetrics = ([f32; 4], [f32; 2], [f32; 2]);

struct App {
    state: std::rc::Rc<std::cell::RefCell<Option<AppState>>>,
    is_suspended: bool,
    force_redraw_count: u32,
    app_builder: std::rc::Rc<std::cell::RefCell<dyn FnMut() -> Element>>,
    cursor_pos: winit::dpi::PhysicalPosition<f64>,
    last_frame_time: std::time::Instant,
    modifiers: ModifiersState,
    left_button_down: bool,
    scrollbar_drag: Option<ScrollbarDragState>,
    drag_reorder: Option<DragReorderState>,
    /// 正在拖选文本的 input `rest`（`input:{rest}` 中的 id）。
    input_drag_rest: Option<String>,
    /// 拖选锚点（UTF-8 字节）。
    input_drag_anchor: usize,
    /// 累积的滚动增量，在每一帧开始时应用到信号。
    scroll_accum: std::collections::HashMap<String, [f32; 2]>,
    /// 当前指针悬停的滚动区域（用于 overlay scrollbar 的 Auto 显示）。
    hover_scroll_rest: Option<String>,
    /// 当前指针悬停的滚动条轴（用于横竖滚动条独立显隐）。
    hover_scroll_axis: Option<(String, scroll_support::ScrollAxis)>,
    /// overlay scrollbar 的自动隐藏：显示截止时间（滚动/hover/拖拽会延长）。
    scrollbar_visible_until: std::collections::HashMap<String, std::time::Instant>,
    /// `scroll_behavior="smooth"` 的滚轮动量（px/s），用于产生“快速滚动感”。
    scroll_velocity: std::collections::HashMap<String, [f32; 2]>,
    /// `scroll_behavior="smooth"`：滚轮输入后至少保持动画/重绘到此时刻，避免连续滚轮出现断续停顿。
    smooth_active_until: std::collections::HashMap<String, std::time::Instant>,
    /// 平滑后的最大滚动量 `[max_x, max_y]`，减轻布局度量抖动导致的 target 来回跳、中部振荡。
    scroll_max_lowpass: std::collections::HashMap<String, [f32; 2]>,
    /// 上一帧 `collect_scroll_area_metrics` 的哈希快照，用于本帧在 `render()` 之前积分平滑滚动。
    scroll_metrics_cache: std::collections::HashMap<String, ScrollMetrics>,
    /// Duplicate `ScrollArea id` detector: warn once per `id`.
    scroll_metric_duplicate_warned: std::collections::HashSet<String>,
    /// Stable signal key strings per scroll-area id (avoids per-frame `format!` in nested scroll).
    scroll_signal_key_cache: std::collections::HashMap<String, ScrollAreaSignalKeys>,
    /// Throttle for optional scrollbar diagnostics (`RUI_SCROLLBAR_DIAG`).
    scroll_diag_last_log: std::collections::HashMap<String, std::time::Instant>,
    event_loop_proxy: EventLoopProxy<RuiUserEvent>,
    accesskit_adapter: Option<accesskit_winit::Adapter>,
    #[cfg(debug_assertions)]
    hot_reload: hot_reload::HotReloadState,
}

pub fn run<F: FnMut() -> Element + 'static>(app_builder: F) {
    lifecycle::run(app_builder);
}
