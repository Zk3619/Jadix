use crate::core::tree::Element;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;
use winit::window::WindowId;

use secondary_window::SecondaryUiBuilder;

/// Wakes the event loop for hot reload (debug) and background tasks, and delivers
/// [`accesskit_winit`] messages.
pub(super) enum RuiUserEvent {
    #[cfg(debug_assertions)]
    HotReloadWake,
    /// A worker thread queued a closure via [`ui_bridge::run_on_ui_thread`].
    UiWake,
    /// Create and show a secondary window (see [`multi_window::open_secondary_window`]).
    OpenWindow {
        options: WindowOptions,
        builder: SecondaryUiBuilder,
    },
    /// Drop a secondary window (custom chrome Close).
    CloseWindow {
        id: WindowId,
    },
    AccessKit(accesskit_winit::Event),
    /// Request a clean event-loop exit (tray Quit / [`ui_bridge::request_exit`]).
    Exit,
}

impl std::fmt::Debug for RuiUserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(debug_assertions)]
            Self::HotReloadWake => f.write_str("HotReloadWake"),
            Self::UiWake => f.write_str("UiWake"),
            Self::OpenWindow { options, .. } => f
                .debug_struct("OpenWindow")
                .field("title", &options.title)
                .field("builder", &"SecondaryUiBuilder(..)")
                .finish(),
            Self::CloseWindow { id } => f.debug_struct("CloseWindow").field("id", id).finish(),
            Self::AccessKit(ev) => f.debug_tuple("AccessKit").field(ev).finish(),
            Self::Exit => f.write_str("Exit"),
        }
    }
}

impl From<accesskit_winit::Event> for RuiUserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        RuiUserEvent::AccessKit(e)
    }
}

pub mod damage;
#[cfg(debug_assertions)]
mod hmr_log;
#[cfg(any(debug_assertions, test))]
mod hot_reload;
pub(crate) mod image_registry;
pub(crate) mod ime_support;
pub use ime_support::{
    apply_ime_commit, peek_last_ime_cursor_snapshot, physical_ime_cursor_area,
    prepare_compose_anchor, take_last_ime_commit, take_last_ime_cursor_snapshot, ImeCursorSnapshot,
};
pub mod error_boundary;
mod glass_gpu;
mod wgpu_backend;
mod input_controller;
pub mod keyed_list;
mod lifecycle;
pub mod multi_window;
pub mod navigator;
pub mod pending_images;
pub mod pointer_events;
pub mod portal_overlay;
pub mod props;
pub mod reactive;
pub mod resource;
pub mod router;
pub mod runtime;
pub(crate) mod scroll_support;
mod secondary_input;
mod secondary_window;
mod state;
#[cfg(test)]
pub(crate) use state::{
    hoist_portal_subtrees_refreshing_scroll_flags, recompute_scroll_viewport_flags,
};
#[cfg(test)]
pub(crate) mod test_support;
pub mod tray;
pub(crate) mod tree_hit;
pub mod ui_bridge;
mod window_chrome;
mod window_events;
pub mod window_keys;
pub(crate) mod window_scope;
#[cfg(feature = "devtools")]
pub mod devtools;

pub use error_boundary::{error_boundary, throw_error, throw_resource_error};
pub use multi_window::{
    open_secondary_window, open_secondary_window_title_only, open_secondary_window_with,
    shared_slot,
};
pub use resource::{create_resource, suspense, Resource, ResourceState};
pub use router::Router;
pub use tray::{system_tray_native, tray_icon, TrayIcon, TrayIconBuilder, TrayMenuItem};
pub use ui_bridge::{request_exit, run_on_ui_thread, spawn_blocking};
pub use window_chrome::{
    close_window, set_window_maximized, set_window_minimized, start_window_drag,
    toggle_window_maximized, window_is_maximized, WindowBackdrop, WindowOptions,
};

use crate::utils::drag_reorder::DragReorderState;
use scroll_support::ScrollbarDragState;
use state::AppState;

use crate::core::scroll_viewport::ScrollViewportSignalKeys;

type ScrollMetrics = ([f32; 4], [f32; 2], [f32; 2]);

/// Textarea 右下角 resize grip 拖拽状态。
struct TextareaResizeDrag {
    field_id: String,
    start_cursor_y: f32,
    start_h: f32,
    min_h: f32,
    max_h: f32,
}

/// Primary-pointer long-press tracker (500ms / 8px slop → synthesize right-click).
struct LongPressTrack {
    window_id: winit::window::WindowId,
    start: std::time::Instant,
    /// Physical cursor position at press.
    pos: [f32; 2],
    fired: bool,
}

struct App {
    state: std::rc::Rc<std::cell::RefCell<Option<AppState>>>,
    is_suspended: bool,
    force_redraw_count: u32,
    app_builder: std::rc::Rc<std::cell::RefCell<dyn FnMut() -> Element>>,
    cursor_pos: winit::dpi::PhysicalPosition<f64>,
    last_frame_time: std::time::Instant,
    modifiers: ModifiersState,
    left_button_down: bool,
    /// DOM `MouseEvent.buttons` bitmask while buttons are held.
    mouse_buttons: u16,
    last_client_pos: Option<[f32; 2]>,
    last_mouse_target: Option<String>,
    last_mouse_path: Option<Vec<pointer_events::HitPathEntry>>,
    /// Left-button `mousedown` hit path. `click` / `dblclick` use the LCA with mouseup.
    click_press_path: Option<Vec<pointer_events::HitPathEntry>>,
    /// Last primary-button click for double-click detection.
    last_click_at: Option<std::time::Instant>,
    last_click_pos: [f32; 2],
    click_count: u8,
    scrollbar_drag: Option<ScrollbarDragState>,
    /// Content pan over a scroll viewport (feeds `scroll_velocity` on release for inertia).
    content_scroll_drag: Option<scroll_support::ContentScrollDrag>,
    /// Textarea 右下角拖拽改高。
    textarea_resize_drag: Option<TextareaResizeDrag>,
    /// Primary touch finger id (winit `Touch.id`); secondary fingers feed pinch.
    primary_touch_id: Option<u64>,
    /// All active touch points for pinch (finger id → physical position).
    active_touches: std::collections::HashMap<u64, winit::dpi::PhysicalPosition<f64>>,
    /// Previous two-finger distance; `None` when not pinching.
    pinch_prev_dist: Option<f32>,
    /// Latest pinch scale factor (also mirrored to signal `__rui_pinch_scale`).
    pinch_scale: f32,
    /// Long-press → ContextMenu path while primary button/touch is held.
    long_press: Option<LongPressTrack>,
    /// Last window that received pointer events (reset hover/click path on switch).
    pointer_window: Option<winit::window::WindowId>,
    /// Primary + secondary window slots (wired MVP).
    window_registry: multi_window::WindowRegistry,
    /// Secondary windows with independent UI trees / swapchains.
    secondaries: std::rc::Rc<
        std::cell::RefCell<
            std::collections::HashMap<
                winit::window::WindowId,
                secondary_window::SecondaryWindowState,
            >,
        >,
    >,
    drag_reorder: Option<DragReorderState>,
    /// ??????? input `rest`?`input:{rest}` ?? id??
    input_drag_rest: Option<String>,
    /// ?????UTF-8 ????
    input_drag_anchor: usize,
    /// 静态可选文本拖选时的稳定键（`focus_id` / `template_id`）
    label_drag_key: Option<String>,
    /// 静态文本拖选锚点（UTF-8 字节）
    label_drag_anchor: usize,
    /// Jump-mode wheel deltas merged until the next redraw.
    scroll_accum: std::collections::HashMap<String, [f32; 2]>,
    /// Last wheel viewport; reused while the pointer stays put and the rest can still move.
    wheel_sticky_rest: Option<String>,
    wheel_sticky_cursor: [f32; 2],
    /// ?????????????? overlay scrollbar ? Auto ????
    hover_scroll_rest: Option<String>,
    /// ?????????????????????????
    hover_scroll_axis: Option<(String, scroll_support::ScrollAxis)>,
    /// overlay scrollbar ???????????????/hover/???????
    scrollbar_visible_until: std::collections::HashMap<String, std::time::Instant>,
    /// Scroll fling velocity (px/s of offset) seeded by content-pan release; decayed each frame.
    /// Wheel must not write here — it only moves `target_*` (see `handle_mouse_wheel`).
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
    /// Wake the event loop for caret blink without continuous `request_redraw`.
    caret_wake_at: Option<std::time::Instant>,
    /// Last value passed to `Window::set_ime_allowed` (avoid per-frame ImmAssociateContextEx).
    ime_allowed_latched: bool,
    /// Latest `WindowEvent::Focused` for the primary window. Used to stop IME HWND
    /// drive / re-enable while caret blink still redraws in the background.
    os_window_focused: bool,
    /// Soft-blur: wait before cancelling IME so clicking the candidate chrome
    /// (which can briefly steal winit focus) does not abort composition.
    ime_os_blur_at: Option<std::time::Instant>,
    event_loop_proxy: EventLoopProxy<RuiUserEvent>,
    accesskit_adapter: Option<accesskit_winit::Adapter>,
    /// Per-secondary-window AccessKit adapters (created before `set_visible`).
    secondary_accesskit:
        std::collections::HashMap<winit::window::WindowId, accesskit_winit::Adapter>,
    /// Window whose HWND currently owns OS IME drive (`LAST_IME_FOCUS_ID` / Imm).
    ime_owner_window: Option<winit::window::WindowId>,
    custom_cursors: crate::style::CustomCursorCache,
    window_options: WindowOptions,
    #[cfg(debug_assertions)]
    hot_reload: hot_reload::HotReloadState,
}

pub fn run<F: FnMut() -> Element + 'static>(app_builder: F) {
    lifecycle::run(WindowOptions::default(), app_builder);
}

/// Like [`run`], with creation-time chrome (frameless / transparent / blur).
pub fn run_with<F: FnMut() -> Element + 'static>(options: WindowOptions, app_builder: F) {
    lifecycle::run(options, app_builder);
}

/// Enable mouse primary-button drag-to-scroll (content pan) on scroll viewports.
///
/// Default is **`false`**: desktop expects wheel / scrollbar; content pan is for touch.
/// Touch primary-finger pan is always enabled regardless of this flag.
///
/// Call before or after [`run`]; takes effect on the next pointer-down.
pub fn set_mouse_content_pan_enabled(enabled: bool) {
    MOUSE_CONTENT_PAN.with(|c| c.set(enabled));
}

/// Whether mouse drag pans scroll content (see [`set_mouse_content_pan_enabled`]).
pub fn mouse_content_pan_enabled() -> bool {
    MOUSE_CONTENT_PAN.with(|c| c.get())
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
    /// Mouse left-drag content pan; off by default (desktop). Touch still pans.
    static MOUSE_CONTENT_PAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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
