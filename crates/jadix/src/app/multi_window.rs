//! Multi-window UI, weak control handles and window lifecycle policies.
//!
//! The primary owns [`super::state::AppState`] while it is open; independent
//! secondary windows retain only the shared GPU context and fonts after it closes.
//! Secondary windows are created via [`open_secondary_window`] →
//! [`super::JadixUserEvent::OpenWindow`] on the event-loop proxy, tracked in
//! [`WindowRegistry`], and host an independent UI tree via
//! [`super::secondary_window::SecondaryWindowState`].
//!
//! Status: secondary windows create/show/close with layout + render
//! (shared primary device/queue). Each window has a dedicated input channel
//! ([`crate::app::secondary_input`]) that hit-tests only that window's tree.
//! Cross-window state uses [`shared_slot`] / [`super::ui_bridge::run_on_ui_thread`].

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use winit::chrome::WindowExtJadix;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::core::tree::Element;

use super::secondary_window::SecondaryUiBuilder;
use super::window_chrome::WindowOptions;

/// A weak OS window handle: retaining it never keeps a closed window alive.
/// Capture this in workers or callbacks to target a particular window, independent
/// of whichever window currently owns the input/reactivity scope.
#[derive(Clone, Debug)]
pub struct WindowHandle {
    id: WindowId,
    window: std::sync::Weak<dyn Window>,
    pub(super) chrome: Arc<WindowChromeState>,
}

#[derive(Debug, Default)]
pub(super) struct WindowChromeState {
    pub inset: AtomicU64,
    pub hovered: AtomicBool,
    pub pressed: AtomicBool,
    pub fullscreen_shortcuts: AtomicBool,
    pub close_with_primary: AtomicBool,
}

impl WindowHandle {
    pub(crate) fn new(window: &Arc<dyn Window>) -> Self {
        Self {
            id: window.id(),
            window: Arc::downgrade(window),
            chrome: Arc::default(),
        }
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn is_alive(&self) -> bool {
        self.window.strong_count() > 0
    }

    pub(crate) fn upgrade(&self) -> Result<Arc<dyn Window>, String> {
        self.window
            .upgrade()
            .ok_or_else(|| "window has closed".into())
    }

    pub fn close(&self) -> Result<(), String> {
        let _window = self.upgrade()?;
        super::ui_bridge::send_user_event(super::JadixUserEvent::CloseWindow { id: self.id })
    }

    /// Ask the window manager to show, restore, and focus this window.
    pub fn focus(&self) -> Result<(), String> {
        let window = self.upgrade()?;
        window.set_visible(true);
        window.set_minimized(false);
        window.focus_window();
        Ok(())
    }

    /// Replace / clear the native window icon. Call on the UI thread.
    /// macOS/mobile/Web launcher icons are configured in the application bundle.
    pub fn set_icon(&self, icon: Option<super::AppIcon>) -> Result<(), String> {
        let window = self.upgrade()?;
        if !super::AppIcon::window_supported() {
            return Err(
                "native window icons are unsupported on this platform; configure the app bundle"
                    .into(),
            );
        }
        window.set_window_icon(icon.as_ref().map(super::AppIcon::to_winit));
        Ok(())
    }

    pub fn set_title(&self, title: &str) -> Result<(), String> {
        self.upgrade()?.set_title(title);
        Ok(())
    }

    pub fn set_visible(&self, visible: bool) -> Result<(), String> {
        self.upgrade()?.set_visible(visible);
        Ok(())
    }

    pub fn set_fullscreen(&self, enabled: bool) -> Result<(), String> {
        self.clear_resize_cursor();
        self.upgrade()?
            .set_fullscreen(enabled.then_some(winit::monitor::Fullscreen::Borderless(None)));
        Ok(())
    }

    pub fn is_fullscreen(&self) -> Result<bool, String> {
        Ok(self.upgrade()?.fullscreen().is_some())
    }

    pub fn toggle_fullscreen(&self) -> Result<(), String> {
        self.set_fullscreen(!self.is_fullscreen()?)
    }

    pub fn set_fullscreen_shortcuts(&self, enabled: bool) -> Result<(), String> {
        self.upgrade()?;
        self.chrome
            .fullscreen_shortcuts
            .store(enabled, Ordering::Relaxed);
        Ok(())
    }

    pub fn set_close_with_primary(&self, enabled: bool) -> Result<(), String> {
        self.upgrade()?;
        self.chrome
            .close_with_primary
            .store(enabled, Ordering::Relaxed);
        Ok(())
    }

    pub fn closes_with_primary(&self) -> Result<bool, String> {
        self.upgrade()?;
        Ok(self.chrome.close_with_primary.load(Ordering::Relaxed))
    }

    /// Global window-manager priority, including over other applications.
    pub fn set_always_on_top(&self, enabled: bool) -> Result<(), String> {
        self.upgrade()?.set_window_level(if enabled {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        });
        Ok(())
    }

    /// Native move; invoke directly from a left-button press on a title drag region.
    pub fn start_drag(&self) -> Result<(), String> {
        self.upgrade()?.drag_window().map_err(|e| e.to_string())
    }

    pub fn set_minimized(&self, minimized: bool) -> Result<(), String> {
        self.upgrade()?.set_minimized(minimized);
        Ok(())
    }

    pub fn set_maximized(&self, maximized: bool) -> Result<(), String> {
        self.clear_resize_cursor();
        self.upgrade()?.set_maximized(maximized);
        Ok(())
    }

    pub fn is_maximized(&self) -> Result<bool, String> {
        Ok(self.upgrade()?.is_maximized())
    }

    /// Custom title-bar maximize/double-click action. Honors the native policy.
    /// Explicit programmatic maximize remains available via `set_maximized`.
    pub fn toggle_maximized(&self) -> Result<(), String> {
        let window = self.upgrade()?;
        if !window.chrome_state().can_maximize() {
            return Err("window cannot be maximized by its title bar".into());
        }
        self.clear_resize_cursor();
        window.set_maximized(!window.is_maximized());
        Ok(())
    }

    /// Live backend capabilities, including Wayland configure/tiled edges.
    pub fn chrome_state(&self) -> Result<winit::chrome::WindowChrome, String> {
        Ok(self.upgrade()?.chrome_state())
    }

    /// macOS follows the system double-click action; desktop peers maximize/restore.
    pub fn titlebar_double_click(&self) -> Result<(), String> {
        self.clear_resize_cursor();
        self.upgrade()?
            .titlebar_double_click()
            .map_err(|e| e.to_string())
    }

    /// Publish native caption rectangles after layout (Windows, any thread). Do not add
    /// client click handlers for the same rectangles: Windows owns the action.
    #[cfg(windows)]
    pub fn set_window_control_regions(
        &self,
        regions: Vec<winit::chrome::WindowControlRegion>,
    ) -> Result<(), String> {
        winit::platform::windows::set_window_control_regions(&self.upgrade()?, regions)
            .map_err(|e| e.to_string())
    }

    pub fn set_resizable(&self, resizable: bool) -> Result<(), String> {
        if !resizable {
            self.clear_resize_cursor();
        }
        self.upgrade()?.set_resizable(resizable);
        Ok(())
    }

    /// Enable the native resize band for a frameless window (logical pixels).
    /// Zero disables it. Windows/Linux/OHOS; native macOS decorations remain available.
    pub fn set_resize_border(&self, inset: f64) -> Result<(), String> {
        self.upgrade()?;
        if !inset.is_finite() || inset < 0.0 {
            return Err("resize border must be finite and nonnegative".into());
        }
        if inset > 0.0 && !cfg!(any(windows, target_os = "linux")) {
            return Err("custom native resize borders require Windows, Linux or OHOS".into());
        }
        self.chrome.inset.store(inset.to_bits(), Ordering::Relaxed);
        if inset == 0.0 {
            self.clear_resize_cursor();
        }
        Ok(())
    }

    pub(super) fn clear_resize_cursor(&self) {
        if self.chrome.hovered.swap(false, Ordering::Relaxed) {
            if let Ok(window) = self.upgrade() {
                window.set_cursor(winit::cursor::CursorIcon::Default.into());
            }
        }
    }

    /// Native system menu at a logical client position (Windows/Wayland).
    /// Wayland requires a valid pointer/touch serial and compositor capability.
    pub fn show_system_menu(&self, x: f64, y: f64) -> Result<(), String> {
        let window = self.upgrade()?;
        if !x.is_finite() || !y.is_finite() {
            return Err("window menu position must be finite".into());
        }
        window
            .try_show_window_menu(winit::dpi::LogicalPosition::new(x, y).into())
            .map_err(|e| e.to_string())
    }

    /// Query a custom border using logical client coordinates and a logical inset.
    /// Native decorations, fixed size, maximize and fullscreen never offer resize.
    pub fn resize_direction_at(
        &self,
        x: f64,
        y: f64,
        inset: f64,
    ) -> Result<Option<winit::window::ResizeDirection>, String> {
        let window = self.upgrade()?;
        let chrome = window.chrome_state();
        if chrome.decorated || !chrome.resizable || chrome.maximized || chrome.fullscreen {
            return Ok(None);
        }
        let size = window
            .surface_size()
            .to_logical::<f64>(window.scale_factor());
        if chrome.tiled != winit::chrome::TiledEdges::default() {
            return Ok(winit::chrome::resize_direction_at_frame(
                [size.width, size.height],
                [x, y],
                [0.0; 4],
                chrome.tiled,
                inset,
            ));
        }
        Ok(super::window_chrome::resize_direction_at(
            [size.width, size.height],
            [x, y],
            inset,
        ))
    }

    /// Current native policy, including changes made after window creation.
    /// Custom borders should omit resize hit zones and cursors when false.
    pub fn is_resizable(&self) -> Result<bool, String> {
        Ok(self.upgrade()?.is_resizable())
    }

    /// Call from a pointer-down handler on a custom resize border.
    /// Rejects non-resizable windows before asking the OS to begin a resize.
    pub fn start_resize(&self, direction: winit::window::ResizeDirection) -> Result<(), String> {
        let window = self.upgrade()?;
        if !window.is_resizable() {
            return Err("window is not resizable".into());
        }
        if window.is_maximized() || window.fullscreen().is_some() {
            return Err("maximized or fullscreen windows cannot be resized by their border".into());
        }
        window
            .start_window_resize(direction)
            .map_err(|e| e.to_string())
    }
}

/// Handle for the UI tree currently being built or handling an event.
pub fn current_window() -> Option<WindowHandle> {
    super::runtime::current_window_handle()
}

/// Snapshot of live windows. Call on the UI thread; handles can then be sent to workers.
pub fn windows() -> Vec<WindowHandle> {
    super::runtime::window_handles()
}

/// Per-window handle slot. Primary owns full [`super::state::AppState`];
/// secondaries are tracked here and rendered via [`super::secondary_window::SecondaryWindowState`].
#[derive(Clone, Debug)]
pub struct WindowSlot {
    pub id: WindowId,
    pub window: Arc<dyn Window>,
}

/// Lightweight `WindowId` → slot map. Production inserts one primary + N secondary.
#[derive(Default, Debug)]
pub struct WindowRegistry {
    slots: HashMap<WindowId, WindowSlot>,
    primary: Option<WindowId>,
}

impl WindowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn primary_id(&self) -> Option<WindowId> {
        self.primary
    }

    pub fn get(&self, id: WindowId) -> Option<&WindowSlot> {
        self.slots.get(&id)
    }

    /// Insert / replace the primary window slot (idempotent on the same id).
    pub fn insert_primary(&mut self, window: Arc<dyn Window>) -> WindowId {
        let id = window.id();
        self.primary = Some(id);
        self.slots.insert(id, WindowSlot { id, window });
        id
    }

    /// Reserve a secondary slot for a live [`Window`].
    pub fn insert_secondary_slot(&mut self, window: Arc<dyn Window>) -> WindowId {
        let id = window.id();
        self.slots.insert(id, WindowSlot { id, window });
        id
    }

    pub fn contains(&self, id: WindowId) -> bool {
        self.slots.contains_key(&id)
    }

    /// Remove a secondary slot. Refuses to remove the primary window.
    pub fn remove_secondary(&mut self, id: WindowId) -> Option<WindowSlot> {
        if self.primary == Some(id) {
            return None;
        }
        self.slots.remove(&id)
    }

    pub(super) fn remove_primary(&mut self) -> Option<WindowSlot> {
        self.primary.take().and_then(|id| self.slots.remove(&id))
    }

    pub fn is_secondary(&self, id: WindowId) -> bool {
        self.primary != Some(id) && self.slots.contains_key(&id)
    }

    /// Hide and remove every non-primary window slot (call on app shutdown).
    pub fn drain_secondaries(&mut self) -> Vec<WindowSlot> {
        let primary = self.primary;
        let ids: Vec<WindowId> = self
            .slots
            .keys()
            .copied()
            .filter(|id| primary != Some(*id))
            .collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(slot) = self.slots.remove(&id) {
                slot.window.set_visible(false);
                out.push(slot);
            }
        }
        out
    }
}

/// Shared window attributes for primary bootstrap and secondary windows.
///
/// Reads `JADIX_WINDOW_TITLE` / `JADIX_WINDOW_WIDTH` / `JADIX_WINDOW_HEIGHT` the same way
/// as the primary `resumed` path.
pub fn default_window_attributes() -> WindowAttributes {
    WindowOptions::default().to_window_attributes()
}

/// Like [`default_window_attributes`], but with an explicit title (secondary windows).
pub fn default_window_attributes_with_title(title: impl Into<String>) -> WindowAttributes {
    WindowOptions::default().title(title).to_window_attributes()
}

/// Queue creation of a secondary window with its own UI tree builder.
///
/// Requires [`super::ui_bridge::install_proxy`] (done when `run` starts). Returns
/// `Ok(())` once the [`super::JadixUserEvent::OpenWindow`] event is queued; the
/// actual [`WindowId`] appears in [`WindowRegistry`] after the event is handled.
///
/// Prefer calling from the UI thread (builders typically capture non-`Send` Signals).
pub fn open_secondary_window(
    title: impl Into<String>,
    builder: impl FnMut() -> Element + 'static,
) -> Result<(), String> {
    open_secondary_window_with(WindowOptions::new().title(title), builder)
}

/// Like [`open_secondary_window`], with frameless / transparent / blur options.
pub fn open_secondary_window_with(
    options: WindowOptions,
    builder: impl FnMut() -> Element + 'static,
) -> Result<(), String> {
    super::ui_bridge::send_user_event(super::JadixUserEvent::OpenWindow {
        options: Box::new(options),
        builder: SecondaryUiBuilder(Box::new(builder)),
    })
    .map_err(|e| format!("multi-window: failed to queue OpenWindow: {e}"))
}

/// Named UI-thread slot shared across windows (and with workers via [`super::ui_bridge::run_on_ui_thread`]).
///
/// Keys are process-global. Capture the returned [`Signal`](crate::core::reactivity::Signal) in a
/// secondary builder, or call this with the same `key` from both trees. `Signal` is `Copy` but
/// not `Send` — hop threads with `run_on_ui_thread(move || slot.set(v))`.
pub fn shared_slot<T: Clone + PartialEq + 'static>(
    key: &'static str,
    init: impl FnOnce() -> T,
) -> crate::core::reactivity::Signal<T> {
    crate::core::reactivity::use_signal(key, init)
}

/// Convenience: open a secondary window with an empty full-size root (tests / placeholders).
pub fn open_secondary_window_title_only(title: impl Into<String>) -> Result<(), String> {
    open_secondary_window(title, || Element::new().w("100%").h("100%"))
}

#[cfg(test)]
#[path = "tests/multi_window.rs"]
mod tests;
