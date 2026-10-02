use crate::core::tree::Element;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::ModifiersState;
use winit::window::WindowId;

use secondary_window::SecondaryUiBuilder;

/// Wakes the event loop for hot reload (debug) and background tasks, and delivers
/// AccessKit messages when feature `a11y` is enabled.
pub(super) enum RuiUserEvent {
    #[cfg(all(debug_assertions, feature = "hot-reload"))]
    HotReloadWake,
    /// A worker thread queued a closure via [`ui_bridge::run_on_ui_thread`].
    UiWake,
    /// Create and show a secondary window (see [`multi_window::open_secondary_window`]).
    OpenWindow {
        options: WindowOptions,
        builder: SecondaryUiBuilder,
    },
    /// Drop a secondary window (custom chrome Close).
    CloseWindow { id: WindowId },
    #[cfg(feature = "a11y")]
    AccessKit(crate::a11y::winit_adapter::Event),
    /// Request a clean event-loop exit (tray Quit / [`ui_bridge::request_exit`]).
    Exit,
}

impl std::fmt::Debug for RuiUserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(all(debug_assertions, feature = "hot-reload"))]
            Self::HotReloadWake => f.write_str("HotReloadWake"),
            Self::UiWake => f.write_str("UiWake"),
            Self::OpenWindow { options, .. } => f
                .debug_struct("OpenWindow")
                .field("title", &options.title)
                .field("builder", &"SecondaryUiBuilder(..)")
                .finish(),
            Self::CloseWindow { id } => f.debug_struct("CloseWindow").field("id", id).finish(),
            #[cfg(feature = "a11y")]
            Self::AccessKit(ev) => f.debug_tuple("AccessKit").field(ev).finish(),
            Self::Exit => f.write_str("Exit"),
        }
    }
}

#[cfg(feature = "a11y")]
impl From<crate::a11y::winit_adapter::Event> for RuiUserEvent {
    fn from(e: crate::a11y::winit_adapter::Event) -> Self {
        RuiUserEvent::AccessKit(e)
    }
}

pub mod damage;
pub(crate) mod draw_cull;
pub mod frame_stats;
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
pub mod entry;
mod error_boundary;
mod glass_gpu;
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
#[cfg(feature = "devtools")]
pub mod devtools;
pub(crate) mod glass_params;
#[cfg(test)]
pub(crate) mod test_support;
pub mod tray;
pub(crate) mod tree_hit;
pub mod ui_bridge;
pub(crate) mod window_chrome;
mod window_events;
pub mod window_keys;
pub(crate) mod window_lifecycle;
pub(crate) mod window_scope;

pub use entry::{app, launch, AppBuilder, AppConfig};
pub use error_boundary::{error_boundary, throw_error, throw_resource_error};
pub use multi_window::{
    open_secondary_window, open_secondary_window_title_only, open_secondary_window_with,
    shared_slot,
};
pub use portal_overlay::{
    page_scroll_locked, parse_scroll_freeze, set_page_scroll_locked, set_scroll_freeze,
    ScrollFreeze,
};
pub use resource::{create_resource, suspense, Resource, ResourceState};
pub use router::Router;
pub use tray::{system_tray_native, tray_icon, TrayIcon, TrayIconBuilder, TrayMenuItem};
pub use ui_bridge::{request_exit, run_on_ui_thread, spawn_blocking};
pub use window_chrome::{
    close_window, set_window_maximized, set_window_minimized, start_window_drag,
    toggle_window_maximized, window_is_maximized, WindowBackdrop, WindowOptions,
    PRIMARY_WINDOW_LABEL,
};
pub use window_lifecycle::{set_window_lifecycle_hook, WindowLifecycleHook};

use crate::utils::drag_reorder::DragReorderState;
use scroll_support::ScrollbarDragState;
use state::AppState;

use crate::core::scroll_viewport::ScrollViewportSignalKeys;

type ScrollMetrics = ([f32; 4], [f32; 2], [f32; 2]);

pub use crate::core::reactivity::with_ui_build_stack;

/// Textarea bottom-right resize-grip drag state.
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
    /// Latched in `RedrawRequested` when a vsync clock is still sampling (CSS / SMIL /
    /// gif / lucide). `about_to_wait` must Poll on it: `request_redraw` inside
    /// `RedrawRequested` is coalesced on Windows, and a scrollbar/caret `WaitUntil`
    /// planted in that same handler would otherwise sleep every CSS clock.
    paint_clock_active: bool,
    /// Previous frame's `scroll_paint_only`. Falling edge arms `force_redraw_count`
    /// so caret `WaitUntil` cannot swallow the first idle frame after the wheel stops.
    prev_scroll_paint_only: bool,
    app_builder: std::rc::Rc<std::cell::RefCell<dyn FnMut() -> Element>>,
    cursor_pos: winit::dpi::PhysicalPosition<f64>,
    /// `WM_MOUSEMOVE` only stores the latest point. Hit-testing runs once in
    /// [`Self::flush_pending_cursor_moved`] at the start of the next present.
    /// Walking the gallery on every move shares the GUI thread with
    /// `RedrawRequested` and is what freezes Marker `animate-spin`.
    cursor_move_pending: bool,
    /// False after `CursorLeft` until the next `CursorMoved` / enter.
    /// Render then samples class-variant `:hover` at an offscreen point so leave clears.
    cursor_inside_window: bool,
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
    /// User-initiated scroll (wheel / pan / keyboard) after this press; cancels `click`.
    click_press_scrolled: bool,
    /// Outside-press already dismissed a stacked layer on pointer down; skip the
    /// mouseup dismiss so nested Select inside DatePicker closes one layer per click.
    outside_dismiss_on_down: bool,
    /// Last primary-button click for double-click detection.
    last_click_at: Option<std::time::Instant>,
    last_click_pos: [f32; 2],
    click_count: u8,
    scrollbar_drag: Option<ScrollbarDragState>,
    /// Content pan over a scroll viewport (feeds `scroll_velocity` on release for inertia).
    content_scroll_drag: Option<scroll_support::ContentScrollDrag>,
    /// Textarea bottom-right drag-to-resize height.
    textarea_resize_drag: Option<TextareaResizeDrag>,
    /// Primary touch finger id (winit `Touch.id`); secondary fingers feed pinch.
    primary_touch_id: Option<winit::event::FingerId>,
    /// All active touch points for pinch (finger id → physical position).
    active_touches: std::collections::HashMap<
        winit::event::FingerId,
        winit::dpi::PhysicalPosition<f64>,
    >,
    /// Previous two-finger distance; `None` when not pinching.
    pinch_prev_dist: Option<f32>,
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
    /// Input `rest` currently being drag-selected (`id` inside `input:{rest}`).
    input_drag_rest: Option<String>,
    /// Drag-selection anchor (UTF-8 byte index).
    input_drag_anchor: usize,
    /// Alt+click extra-caret drag (do not move the primary caret).
    input_drag_multi: bool,
    /// Extra carets present when the Alt gesture started (excluding this drag).
    input_drag_multi_base: Vec<(usize, usize)>,
    /// Stable key for static selectable-text drag (`focus_id` / `template_id`).
    label_drag_key: Option<String>,
    /// Static text drag-selection anchor (UTF-8 bytes).
    label_drag_anchor: usize,
    /// Jump-mode wheel deltas merged until the next redraw.
    scroll_accum: std::collections::HashMap<String, [f32; 2]>,
    /// Last wheel viewport; reused while the pointer stays put and the rest can still move.
    wheel_sticky_rest: Option<String>,
    wheel_sticky_cursor: [f32; 2],
    /// Scroll area under the pointer (for overlay scrollbar Auto visibility).
    hover_scroll_rest: Option<String>,
    /// Scrollbar axis under the pointer (independent H/V show-hide).
    hover_scroll_axis: Option<(String, scroll_support::ScrollAxis)>,
    /// Overlay scrollbar auto-hide: visible-until deadline (scroll/hover/drag extends it).
    scrollbar_visible_until: std::collections::HashMap<String, std::time::Instant>,
    /// Prune in `about_to_wait` must Poll until a real `RedrawRequested`. Windows often
    /// delivers a second `about_to_wait` before paint; a local one-shot then `Wait`s and
    /// coalesces the fade-start frame (bar stuck at cubic 1).
    scrollbar_hide_wake: bool,
    /// Scroll fling velocity (px/s of offset) seeded by content-pan release; decayed each frame.
    /// Wheel must not write here — it only moves `target_*` (see `handle_mouse_wheel`).
    scroll_velocity: std::collections::HashMap<String, [f32; 2]>,
    /// `scroll_behavior="smooth"`: keep animating/redrawing at least until this instant after wheel input,
    /// so consecutive ticks do not stutter.
    smooth_active_until: std::collections::HashMap<String, std::time::Instant>,
    /// Low-passed max scroll `[max_x, max_y]` to damp layout-metric jitter that makes targets bounce mid-range.
    scroll_max_lowpass: std::collections::HashMap<String, [f32; 2]>,
    /// Hash snapshot from the last `collect_scroll_area_metrics`, used to integrate smooth scroll before `render()`.
    scroll_metrics_cache: std::collections::HashMap<String, ScrollMetrics>,
    /// Duplicate scroll viewport `id` detector: warn once per `id`.
    scroll_metric_duplicate_warned: std::collections::HashSet<String>,
    /// Stable signal key strings per scroll viewport id (avoids per-frame `format!` in nested scroll).
    scroll_signal_key_cache: std::collections::HashMap<String, ScrollViewportSignalKeys>,
    /// Throttle for optional scrollbar diagnostics (`RUI_SCROLLBAR_DIAG`).
    scroll_diag_last_log: std::collections::HashMap<String, std::time::Instant>,
    /// Wake the event loop for caret blink without continuous `request_redraw`.
    caret_wake_at: Option<std::time::Instant>,
    /// Last IME enablement sent via `Window::request_ime_update` (avoid per-frame ImmAssociateContextEx).
    ime_allowed_latched: bool,
    /// Latest `WindowEvent::Focused` for the primary window. Used to stop IME HWND
    /// drive / re-enable while caret blink still redraws in the background.
    os_window_focused: bool,
    /// Soft-blur: wait before cancelling IME so clicking the candidate chrome
    /// (which can briefly steal winit focus) does not abort composition.
    ime_os_blur_at: Option<std::time::Instant>,
    /// AccessKit adapters clone this; unused when feature `a11y` is off.
    #[cfg_attr(not(feature = "a11y"), allow(dead_code))]
    event_loop_proxy: EventLoopProxy,
    #[cfg(feature = "a11y")]
    accesskit_adapter: Option<crate::a11y::winit_adapter::Adapter>,
    /// Per-secondary-window AccessKit adapters (created before `set_visible`).
    #[cfg(feature = "a11y")]
    secondary_accesskit:
        std::collections::HashMap<winit::window::WindowId, crate::a11y::winit_adapter::Adapter>,
    user_events: std::sync::mpsc::Receiver<RuiUserEvent>,
    /// Window whose HWND currently owns OS IME drive (`LAST_IME_FOCUS_ID` / Imm).
    ime_owner_window: Option<winit::window::WindowId>,
    custom_cursors: crate::style::CustomCursorCache,
    window_options: WindowOptions,
    #[cfg(debug_assertions)]
    hot_reload: hot_reload::HotReloadState,
}

/// 兼容：默认 [`WindowOptions`] 启动事件循环。
///
/// 新代码请用 [`launch`]（RSX + [`crate::components::App`]）或 [`app()`]`.mount`（Builder）。
pub fn run<F: FnMut() -> Element + 'static>(app_builder: F) {
    lifecycle::run(WindowOptions::default(), app_builder);
}

/// 兼容：带创建期窗口选项启动。新代码请用 [`launch`] / [`app()`]`.mount`。
pub fn run_with<F: FnMut() -> Element + 'static>(options: WindowOptions, app_builder: F) {
    lifecycle::run(options, app_builder);
}

/// HarmonyOS / OpenHarmony entry (gpui-template `openharmony_app`).
///
/// ```ignore
/// use openharmony_ability::OpenHarmonyApp;
///
/// #[openharmony_ability_derive::ability]
/// fn openharmony_app(app: OpenHarmonyApp) {
///     rui::run_ohos(app, rui::WindowOptions::default(), || ui! { <App/> });
/// }
/// ```
///
/// ArkTS side: `class EntryAbility extends NativeAbility { moduleName = "your_cdylib" }`.
#[cfg(target_env = "ohos")]
pub fn run_ohos<F: FnMut() -> Element + 'static>(
    app: openharmony_ability::OpenHarmonyApp,
    options: WindowOptions,
    app_builder: F,
) {
    crate::platform::install_ohos_app(app);
    lifecycle::run(options, app_builder);
}

/// Android `android_main` helper: install the activity, then start the winit loop.
///
/// ```ignore
/// #[cfg(target_os = "android")]
/// #[no_mangle]
/// fn android_main(app: winit::platform::android::activity::AndroidApp) {
///     rui::run_android(app, rui::WindowOptions::default(), || ui! { <App/> });
/// }
/// ```
#[cfg(target_os = "android")]
pub fn run_android<F: FnMut() -> Element + 'static>(
    android_app: winit::platform::android::activity::AndroidApp,
    options: WindowOptions,
    app_builder: F,
) {
    crate::platform::install_android_app(android_app);
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

pub(super) fn fire_file_drop(paths: Vec<std::path::PathBuf>) {
    let cb = FILE_DROP_HANDLER.with(|h| h.borrow().clone());
    if let Some(cb) = cb {
        crate::core::reactivity::with_ui_build_stack(|| (*cb.borrow_mut())(paths));
    }
}
