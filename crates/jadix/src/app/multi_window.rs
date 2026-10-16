//! Multi-window wired MVP.
//!
//! Primary window still owns the full [`super::state::AppState`] / GPU path.
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
use std::sync::Arc;

use winit::window::{Window, WindowAttributes, WindowId};

use crate::core::tree::Element;

use super::secondary_window::SecondaryUiBuilder;
use super::window_chrome::WindowOptions;

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
        options,
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
        let err3 = open_secondary_window_with(
            WindowOptions::new().title("Glass").frameless().blur(true),
            Element::new,
        )
        .expect_err("needs proxy");
        assert!(
            err3.contains("proxy not installed") || err3.contains("failed to queue"),
            "unexpected message: {err3}"
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
    fn window_registry_drain_secondaries_on_empty() {
        let mut reg = WindowRegistry::new();
        assert!(reg.drain_secondaries().is_empty());
    }
}
