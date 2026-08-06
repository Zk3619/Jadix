use crate::core::tree::Element;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;

/// Wakes the event loop for hot reload (debug) and background tasks, and delivers
/// [`accesskit_winit`] messages.
#[derive(Debug)]
pub(super) enum RuiUserEvent {
    #[cfg(debug_assertions)]
    HotReloadWake,
    /// A worker thread queued a closure via [`ui_bridge::run_on_ui_thread`].
    UiWake,
    AccessKit(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for RuiUserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        RuiUserEvent::AccessKit(e)
    }
}

mod damage;
mod hot_reload;
mod image_registry;
mod input_controller;
mod lifecycle;
pub mod navigator;
pub mod pending_images;
pub mod pointer_events;
pub mod portal_overlay;
pub mod reactive;
pub mod runtime;
mod scroll_support;
mod state;
#[cfg(test)]
pub(crate) mod test_support;
mod tree_hit;
pub mod ui_bridge;
mod window_events;
pub mod window_keys;

pub use ui_bridge::{run_on_ui_thread, spawn_blocking};

use crate::utils::drag_reorder::DragReorderState;
use scroll_support::ScrollbarDragState;
use state::AppState;

use crate::core::scroll_viewport::ScrollViewportSignalKeys;

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
    /// Last primary-button click for double-click detection.
    last_click_at: Option<std::time::Instant>,
    last_click_pos: [f32; 2],
    click_count: u8,
    scrollbar_drag: Option<ScrollbarDragState>,
    drag_reorder: Option<DragReorderState>,
    /// ??????? input `rest`?`input:{rest}` ?? id??
    input_drag_rest: Option<String>,
    /// ?????UTF-8 ????
    input_drag_anchor: usize,
    /// 静态可选文本拖选时的稳定键（`focus_id` / `template_id`）
    label_drag_key: Option<String>,
    /// 静态文本拖选锚点（UTF-8 字节）
    label_drag_anchor: usize,
    /// ?????????????????????
    scroll_accum: std::collections::HashMap<String, [f32; 2]>,
    /// ?????????????? overlay scrollbar ? Auto ????
    hover_scroll_rest: Option<String>,
    /// ?????????????????????????
    hover_scroll_axis: Option<(String, scroll_support::ScrollAxis)>,
    /// overlay scrollbar ???????????????/hover/???????
    scrollbar_visible_until: std::collections::HashMap<String, std::time::Instant>,
    /// `scroll_behavior="smooth"` ??????px/s??????????????
    scroll_velocity: std::collections::HashMap<String, [f32; 2]>,
    /// `scroll_behavior="smooth"`????????????/????????????????????
    smooth_active_until: std::collections::HashMap<String, std::time::Instant>,
    /// ????????? `[max_x, max_y]`???????????? target ?????????
    scroll_max_lowpass: std::collections::HashMap<String, [f32; 2]>,
    /// ??? `collect_scroll_area_metrics` ??????????? `render()` ?????????
    scroll_metrics_cache: std::collections::HashMap<String, ScrollMetrics>,
    /// Duplicate scroll viewport `id` detector: warn once per `id`.
    scroll_metric_duplicate_warned: std::collections::HashSet<String>,
    /// Stable signal key strings per scroll viewport id (avoids per-frame `format!` in nested scroll).
    scroll_signal_key_cache: std::collections::HashMap<String, ScrollViewportSignalKeys>,
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

/// Set pointer capture to the element with this `focus_id` (DOM `setPointerCapture` analogue).
pub fn set_pointer_capture(focus_id: impl Into<String>) {
    POINTER_CAPTURE.with(|c| *c.borrow_mut() = Some(focus_id.into()));
}

/// Release pointer capture.
pub fn release_pointer_capture() {
    POINTER_CAPTURE.with(|c| *c.borrow_mut() = None);
}

pub fn pointer_capture_id() -> Option<String> {
    POINTER_CAPTURE.with(|c| c.borrow().clone())
}

thread_local! {
    static POINTER_CAPTURE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static FILE_DROP_HANDLER: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<dyn FnMut(Vec<std::path::PathBuf>)>>>> =
        const { std::cell::RefCell::new(None) };
    static DROPPED_FILES: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Register a process-wide file-drop handler (paths from the OS).
pub fn on_file_drop(handler: impl FnMut(Vec<std::path::PathBuf>) + 'static) {
    FILE_DROP_HANDLER.with(|h| {
        *h.borrow_mut() = Some(std::rc::Rc::new(std::cell::RefCell::new(handler)));
    });
}

pub(super) fn take_dropped_files() -> Vec<std::path::PathBuf> {
    DROPPED_FILES.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

pub(super) fn push_dropped_file(path: std::path::PathBuf) {
    DROPPED_FILES.with(|f| f.borrow_mut().push(path));
}

pub(super) fn fire_file_drop(paths: Vec<std::path::PathBuf>) {
    FILE_DROP_HANDLER.with(|h| {
        if let Some(cb) = h.borrow().as_ref() {
            (*cb.borrow_mut())(paths);
        }
    });
}
