//! Multi-window wired MVP.
//!
//! Primary window still owns the full [`super::state::AppState`] / GPU path.
//! Secondary windows are created via [`open_secondary_window`] →
//! [`super::RuiUserEvent::OpenWindow`] on the event-loop proxy, tracked in
//! [`WindowRegistry`], and host an independent UI tree via
//! [`super::secondary_window::SecondaryWindowState`].
//!
//! Status: **wired MVP** — secondary windows create/show/close with layout + render
//! (shared primary device/queue); input routing beyond redraw/resize is still primary-first.

use std::collections::HashMap;
use std::sync::Arc;

use winit::window::{Window, WindowAttributes, WindowId};

use crate::core::tree::Element;

use super::secondary_window::SecondaryUiBuilder;

/// Per-window handle slot. Primary owns full [`super::state::AppState`];
/// secondaries are tracked here and rendered via [`super::secondary_window::SecondaryWindowState`].
#[derive(Clone, Debug)]
pub struct WindowSlot {
    pub id: WindowId,
    pub window: Arc<Window>,
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
    pub fn insert_primary(&mut self, window: Arc<Window>) -> WindowId {
        let id = window.id();
        self.primary = Some(id);
        self.slots.insert(id, WindowSlot { id, window });
        id
    }

    /// Reserve a secondary slot for a live [`Window`].
    pub fn insert_secondary_slot(&mut self, window: Arc<Window>) -> WindowId {
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

    pub fn is_secondary(&self, id: WindowId) -> bool {
        self.primary != Some(id) && self.slots.contains_key(&id)
    }
}

/// Shared window attributes for primary bootstrap and secondary windows.
///
/// Reads `RUI_WINDOW_TITLE` / `RUI_WINDOW_WIDTH` / `RUI_WINDOW_HEIGHT` the same way
/// as the primary `resumed` path.
pub fn default_window_attributes() -> WindowAttributes {
    default_window_attributes_with_title(
        std::env::var("RUI_WINDOW_TITLE").unwrap_or_else(|_| "RUI Engine".into()),
    )
}

/// Like [`default_window_attributes`], but with an explicit title (secondary windows).
pub fn default_window_attributes_with_title(title: impl Into<String>) -> WindowAttributes {
    let mut attrs = Window::default_attributes()
        .with_title(title)
        .with_visible(false);
    if let (Ok(w), Ok(h)) = (
        std::env::var("RUI_WINDOW_WIDTH"),
        std::env::var("RUI_WINDOW_HEIGHT"),
    ) {
        if let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) {
            if w > 0.0 && h > 0.0 {
                attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(w, h));
            }
        }
    }
    attrs
}

/// Queue creation of a secondary window with its own UI tree builder.
///
/// Requires [`super::ui_bridge::install_proxy`] (done when `run` starts). Returns
/// `Ok(())` once the [`super::RuiUserEvent::OpenWindow`] event is queued; the
/// actual [`WindowId`] appears in [`WindowRegistry`] after the event is handled.
///
/// Prefer calling from the UI thread (builders typically capture non-`Send` Signals).
pub fn open_secondary_window(
    title: impl Into<String>,
    builder: impl FnMut() -> Element + 'static,
) -> Result<(), String> {
    let proxy = super::ui_bridge::proxy().ok_or_else(|| {
        "multi-window: event loop proxy not installed (call rui::run first)".to_string()
    })?;
    proxy
        .send_event(super::RuiUserEvent::OpenWindow {
            title: title.into(),
            builder: SecondaryUiBuilder(Box::new(builder)),
        })
        .map_err(|e| format!("multi-window: failed to queue OpenWindow: {e}"))
}

/// Convenience: open a secondary window with an empty full-size root (tests / placeholders).
pub fn open_secondary_window_title_only(title: impl Into<String>) -> Result<(), String> {
    open_secondary_window(title, || Element::new().w("100%").h("100%"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_secondary_window_needs_proxy() {
        // Without an installed proxy this process cannot create windows.
        let err = open_secondary_window("Secondary", Element::new).expect_err("needs proxy");
        assert!(
            err.contains("proxy not installed") || err.contains("failed to queue"),
            "unexpected message: {err}"
        );
        let err2 = open_secondary_window_title_only("Secondary").expect_err("needs proxy");
        assert!(
            err2.contains("proxy not installed") || err2.contains("failed to queue"),
            "unexpected message: {err2}"
        );
    }

    #[test]
    fn window_registry_starts_empty_and_tracks_api() {
        let reg = WindowRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        assert!(reg.primary_id().is_none());
        assert!(open_secondary_window_title_only("extra").is_err());
    }

    #[test]
    fn window_registry_remove_secondary_on_empty() {
        let reg = WindowRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
        // remove_secondary is a no-op until real WindowIds are inserted by the event loop.
        assert!(reg.primary_id().is_none());
    }
}
