//! Window-local input geometry state for deferred product updates.

use crate::app::window_scope::{self, WindowScope};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

thread_local! {
    static SCROLL_REVISIONS: RefCell<HashMap<WindowScope, u64>> = RefCell::new(HashMap::new());
    static CHANGING_GEOMETRY: RefCell<HashSet<WindowScope>> = RefCell::new(HashSet::new());
}

/// Whether scroll animation or scrollbar dragging is changing the active window's
/// hit geometry. Product hover UI may defer structural updates until it settles.
pub fn geometry_changing() -> bool {
    CHANGING_GEOMETRY.with(|scopes| scopes.borrow().contains(&window_scope::active()))
}

/// Scroll a viewport by logical pixels, clamped to measured content bounds.
/// Positive deltas move toward the end and honor the viewport's smooth/jump policy.
pub fn scroll_by(viewport: &str, dx: f32, dy: f32) {
    use crate::core::{reactivity::use_signal, scroll_viewport::ScrollViewportSignalKeys};
    if !dx.is_finite() || !dy.is_finite() {
        return;
    }
    let keys = ScrollViewportSignalKeys::new(viewport);
    let max_x = (use_signal(keys.content_w.as_str(), || 0.0_f32).peek()
        - use_signal(keys.viewport_w.as_str(), || 0.0_f32).peek())
    .max(0.0);
    let max_y = (use_signal(keys.content_h.as_str(), || 0.0_f32).peek()
        - use_signal(keys.viewport_h.as_str(), || 0.0_f32).peek())
    .max(0.0);
    crate::app::scroll_support::apply_scroll_navigation_delta(viewport, -dx, -dy, max_x, max_y);
    crate::app::runtime::request_redraw_all();
}

/// Monotonic revision of actual scroll movement in this window. Product overlays
/// can dismiss without a framework dependency on their component or ID conventions.
pub fn scroll_revision() -> u64 {
    SCROLL_REVISIONS.with(|revisions| {
        revisions
            .borrow()
            .get(&window_scope::active())
            .copied()
            .unwrap_or(0)
    })
}

pub(crate) fn note_scroll() {
    SCROLL_REVISIONS.with(|revisions| {
        let mut revisions = revisions.borrow_mut();
        let revision = revisions.entry(window_scope::active()).or_default();
        *revision = revision.wrapping_add(1);
    });
}

pub(crate) fn set_geometry_changing(changing: bool) {
    let scope = window_scope::active();
    CHANGING_GEOMETRY.with(|scopes| {
        if changing {
            scopes.borrow_mut().insert(scope);
        } else {
            scopes.borrow_mut().remove(&scope);
        }
    });
}

pub(crate) fn forget(scope: WindowScope) {
    SCROLL_REVISIONS.with(|revisions| revisions.borrow_mut().remove(&scope));
    CHANGING_GEOMETRY.with(|scopes| {
        scopes.borrow_mut().remove(&scope);
    });
}

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
