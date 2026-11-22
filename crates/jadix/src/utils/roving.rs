//! Roving tabindex helpers for lists / menus (Stage 7 / WP-1.2 keyboard nav).
//!
//! - Index math: [`roving_move`] / [`roving_edge`]
//! - Focus ring: [`focus_by_id`] moves `__jadix_button_focus_id` + `__jadix_focus_visible`
//! - Tree wiring: [`attach_focus_roving`] adds arrow-key focus movement among sibling ids

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::core::reactivity::{batch, use_signal};
use crate::core::tree::Element;
use crate::utils::input_hit::collect_focusable_ids;

/// Legacy focus namespace recognized as a tab list by the accessibility bridge.
pub const TABSTRIP_FOCUS_PREFIX: &str = "tabstrip:";

pub use jadix_core::roving::{roving_move, roving_edge, roving_move_skipping};

/// Move keyboard focus ring to `focus_id` (`focus-visible` on).
///
/// For pointer / programmatic open (DatePicker popover), use [`focus_id_only`] so the
/// ring only appears after Tab, matching [`crate::style::focus_ring`].
pub fn focus_by_id(focus_id: impl Into<String>) {
    let id = focus_id.into();
    if id.is_empty() {
        return;
    }
    batch(|| {
        resume_keyboard_focus();
        use_signal("__jadix_button_focus_id", String::new).set(id);
        use_signal("__jadix_focus_visible", || false).set(true);
    });
}

/// Move focus without enabling the keyboard focus ring.
pub fn focus_id_only(focus_id: impl Into<String>) {
    let id = focus_id.into();
    if id.is_empty() {
        return;
    }
    batch(|| {
        resume_keyboard_focus();
        use_signal("__jadix_button_focus_id", String::new).set(id);
    });
}

/// Restore focus after closing a surface, preserving the existing focus ring.
/// Restoration does not count as new keyboard input for focus-triggered hints.
pub fn restore_focus_by_id(focus_id: impl Into<String>) {
    let id = focus_id.into();
    if id.is_empty() {
        return;
    }
    batch(|| {
        use_signal("__jadix_restored_focus_id", String::new).set(id.clone());
        use_signal("__jadix_button_focus_id", String::new).set(id);
    });
}

/// Whether the focus target was restored rather than newly reached by input.
pub fn is_restored_focus(focus_id: &str) -> bool {
    use_signal("__jadix_restored_focus_id", String::new).get() == focus_id
}

pub(crate) fn resume_keyboard_focus() {
    let restored = use_signal("__jadix_restored_focus_id", String::new);
    if !restored.peek().is_empty() {
        restored.set(String::new());
    }
}

/// Return focus on close only while it still belongs to the closing subtree.
/// Capture before removing the panel: application callbacks, outside presses,
/// and Tab may already have moved focus elsewhere by the time close is rendered.
#[derive(Clone, PartialEq)]
pub(crate) struct PanelFocusReturn {
    target: String,
    members: Vec<String>,
}

impl PanelFocusReturn {
    pub(crate) fn capture(panel: &Element, target: String) -> Self {
        let mut members = Vec::new();
        collect_focusable_ids(panel, &mut members);
        Self { target, members }
    }

    pub(crate) fn refresh(&mut self, panel: &Element) {
        self.members.clear();
        collect_focusable_ids(panel, &mut self.members);
    }

    pub(crate) fn restore(self) {
        if self.target.is_empty() {
            return;
        }
        let focus = use_signal("__jadix_button_focus_id", String::new);
        if self.members.contains(&focus.peek()) {
            restore_focus_by_id(self.target);
        }
    }
}

/// Axis for [`attach_focus_roving`] / [`focus_roving_key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusRovingAxis {
    /// ← → (+ Home/End)
    Horizontal,
    /// ↑ ↓ (+ Home/End)
    Vertical,
    /// Both arrow pairs
    Both,
}

/// Handle arrow / Home / End: move focus among `ids` starting at `index`. Returns whether handled.
pub fn focus_roving_key(
    ev: &KeyEvent,
    ids: &[String],
    index: usize,
    axis: FocusRovingAxis,
) -> bool {
    if ev.state != ElementState::Pressed || ev.repeat || ids.is_empty() {
        return false;
    }
    let cur = index.min(ids.len() - 1);
    let next = match (&ev.logical_key, axis) {
        (Key::Named(NamedKey::ArrowRight), FocusRovingAxis::Horizontal | FocusRovingAxis::Both)
        | (Key::Named(NamedKey::ArrowDown), FocusRovingAxis::Vertical | FocusRovingAxis::Both) => {
            roving_move(ids.len(), cur, 1)
        }
        (Key::Named(NamedKey::ArrowLeft), FocusRovingAxis::Horizontal | FocusRovingAxis::Both)
        | (Key::Named(NamedKey::ArrowUp), FocusRovingAxis::Vertical | FocusRovingAxis::Both) => {
            roving_move(ids.len(), cur, -1)
        }
        (Key::Named(NamedKey::Home), _) => roving_edge(ids.len(), false),
        (Key::Named(NamedKey::End), _) => roving_edge(ids.len(), true),
        _ => return false,
    };
    if let Some(next) = next {
        focus_by_id(ids[next].clone());
    }
    true
}

/// Walk `root`, collect focusable ids, and attach arrow-key handlers that only move the focus ring.
///
/// Chains after any existing `on_key_down` (existing handler wins if it returns `true`).
pub fn attach_focus_roving(root: &mut Element, axis: FocusRovingAxis) {
    let mut ids = Vec::new();
    collect_focusable_ids(root, &mut ids);
    if ids.len() < 2 {
        return;
    }
    let ids = Rc::new(ids);
    attach_focus_roving_rec(root, &ids, axis, &mut 0);
}

fn attach_focus_roving_rec(
    el: &mut Element,
    ids: &Rc<Vec<String>>,
    axis: FocusRovingAxis,
    next_ix: &mut usize,
) {
    if el.focusable {
        if let Some(fid) = el.focus_id.clone() {
            if ids.get(*next_ix).map(|s| s.as_str()) == Some(fid.as_str()) {
                let index = *next_ix;
                *next_ix += 1;
                let ids_c = Rc::clone(ids);
                let prev = el.on_key_down.clone();
                el.on_key_down = Some(Rc::new(RefCell::new(move |ev: KeyEvent| {
                    if let Some(ref p) = prev {
                        if p.borrow_mut()(ev.clone()) {
                            return true;
                        }
                    }
                    focus_roving_key(&ev, ids_c.as_slice(), index, axis)
                })));
            }
        }
    }
    for child in &mut el.children {
        attach_focus_roving_rec(child, ids, axis, next_ix);
    }
}

#[cfg(test)]
#[path = "tests/roving.rs"]
mod tests;
