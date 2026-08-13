//! Roving tabindex helpers for lists / menus (Stage 7 / WP-1.2 keyboard nav).
//!
//! - Index math: [`roving_move`] / [`roving_edge`]
//! - Focus ring: [`focus_by_id`] moves `__rui_button_focus_id` + `__rui_focus_visible`
//! - Tree wiring: [`attach_focus_roving`] adds arrow-key focus movement among sibling ids

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::utils::input_hit::collect_focusable_ids;

/// Move selection within `0..len`, wrapping at ends. Returns `None` if `len == 0`.
pub fn roving_move(len: usize, current: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let n = len as i32;
    let cur = (current.min(len - 1)) as i32;
    let next = (cur + delta).rem_euclid(n);
    Some(next as usize)
}

/// Home / End targets.
pub fn roving_edge(len: usize, to_end: bool) -> Option<usize> {
    if len == 0 {
        None
    } else if to_end {
        Some(len - 1)
    } else {
        Some(0)
    }
}

/// Skip disabled indices while moving (wraps). `disabled[i] == true` means skip.
/// Returns `None` if `len == 0` or every index is disabled.
pub fn roving_move_skipping(
    len: usize,
    current: usize,
    delta: i32,
    disabled: impl Fn(usize) -> bool,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let step = if delta >= 0 { 1 } else { -1 };
    let steps = delta.unsigned_abs().max(1) as usize;
    let mut idx = current.min(len - 1);
    for _ in 0..steps {
        let mut guard = 0usize;
        loop {
            idx = roving_move(len, idx, step)?;
            guard += 1;
            if !disabled(idx) {
                break;
            }
            if guard >= len {
                return None;
            }
        }
    }
    if disabled(idx) {
        None
    } else {
        Some(idx)
    }
}

/// Move keyboard focus ring to `focus_id` (`focus-visible` on).
///
/// For pointer / programmatic open (DatePicker popover), use [`focus_id_only`] so the
/// ring only appears after Tab, matching [`crate::style::focus_ring`].
pub fn focus_by_id(focus_id: impl Into<String>) {
    let id = focus_id.into();
    if id.is_empty() {
        return;
    }
    use_signal("__rui_button_focus_id", String::new).set(id);
    use_signal("__rui_focus_visible", || false).set(true);
}

/// Move focus without enabling the keyboard focus ring.
pub fn focus_id_only(focus_id: impl Into<String>) {
    let id = focus_id.into();
    if id.is_empty() {
        return;
    }
    use_signal("__rui_button_focus_id", String::new).set(id);
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
mod tests {
    use super::*;

    #[test]
    fn wraps_and_edges() {
        assert_eq!(roving_move(3, 0, -1), Some(2));
        assert_eq!(roving_move(3, 2, 1), Some(0));
        assert_eq!(roving_edge(5, false), Some(0));
        assert_eq!(roving_edge(5, true), Some(4));
        assert_eq!(roving_move(0, 0, 1), None);
        assert_eq!(roving_edge(0, true), None);
    }

    #[test]
    fn multi_step_wrap() {
        assert_eq!(roving_move(4, 1, 2), Some(3));
        assert_eq!(roving_move(4, 1, -2), Some(3));
        assert_eq!(roving_move(1, 0, 1), Some(0));
        assert_eq!(roving_move(1, 0, -1), Some(0));
    }

    #[test]
    fn clamps_stale_current() {
        assert_eq!(roving_move(3, 99, 1), Some(0));
        assert_eq!(roving_move(3, 99, -1), Some(1));
    }

    #[test]
    fn menu_list_contract_home_end_arrows() {
        let n = 4;
        let mut i = 0;
        i = roving_move(n, i, 1).unwrap();
        assert_eq!(i, 1);
        i = roving_move(n, i, 1).unwrap();
        assert_eq!(i, 2);
        i = roving_edge(n, true).unwrap();
        assert_eq!(i, 3);
        i = roving_move(n, i, 1).unwrap();
        assert_eq!(i, 0);
        i = roving_edge(n, false).unwrap();
        assert_eq!(i, 0);
    }

    #[test]
    fn tabs_strip_wraps_like_roving_move() {
        let n = 3;
        assert_eq!(roving_move(n, 2, 1), Some(0));
        assert_eq!(roving_move(n, 0, -1), Some(2));
    }

    #[test]
    fn focus_id_only_does_not_set_focus_visible() {
        use_signal("__rui_focus_visible", || false).set(false);
        focus_id_only("cal");
        assert_eq!(
            use_signal("__rui_button_focus_id", String::new).peek(),
            "cal"
        );
        assert!(!use_signal("__rui_focus_visible", || false).peek());
        focus_by_id("cal2");
        assert!(use_signal("__rui_focus_visible", || false).peek());
    }

    #[test]
    fn skipping_disabled() {
        let disabled = |i: usize| i == 1 || i == 2;
        assert_eq!(roving_move_skipping(4, 0, 1, disabled), Some(3));
        assert_eq!(roving_move_skipping(4, 3, -1, disabled), Some(0));
        assert_eq!(roving_move_skipping(3, 0, 1, |_| true), None);
    }
}
