//! Shared focus-ring chrome (theme `ring` color + blur).
//!
//! Matches Input focus shadow: `ring` token, blur [`FOCUS_RING_BLUR`], offset/spread 0.
//!
//! - **Input / Textarea**: always ring while focused (text_edit paint; ignore `__rui_focus_visible`).
//! - **Other controls**: opt-in via [`Element::focus_ring`]; ring only when focused **and**
//!   `__rui_focus_visible` (keyboard Tab). Pointer down clears that flag so mouse focus has no ring.
//! - **Button**: spring-blended ring in `button.rs` (same token/blur); skipped here.

use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::parse_color;

/// Default focus-ring blur in logical px (matches Button / Input).
pub const FOCUS_RING_BLUR: f32 = 3.0;

/// Theme `ring` color as linear RGBA for [`crate::style::Style::shadow_color`].
pub fn focus_ring_color() -> [f32; 4] {
    parse_color("ring")
}

/// Whether this node should show the keyboard focus ring right now.
pub fn control_focus_ring_active(focus_id: &str, focusable: bool, disabled_skip: bool) -> bool {
    if !focusable || disabled_skip || focus_id.is_empty() {
        return false;
    }
    let focused = use_signal("__rui_button_focus_id", String::new).peek();
    let focus_visible = use_signal("__rui_focus_visible", || false).peek();
    focus_visible && focused == focus_id
}

/// Write focus-ring shadow fields onto `el` (or clear when `focused` is false and `clear_when_unfocused`).
pub fn apply_focus_ring_shadow(el: &mut Element, focused: bool, clear_when_unfocused: bool) {
    if focused {
        el.style.shadow_color = focus_ring_color();
        el.style.shadow_blur = FOCUS_RING_BLUR;
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_spread = 0.0;
        el.style.shadow_inset = false;
    } else if clear_when_unfocused {
        el.style.shadow_blur = 0.0;
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_spread = 0.0;
        el.style.shadow_inset = false;
    }
}

/// Whether `el` already matches the focused ring params (avoids redundant style dirty).
pub fn focus_ring_shadow_matches(el: &Element, focused: bool) -> bool {
    if !focused {
        return el.style.shadow_blur.abs() <= 0.01;
    }
    let c = focus_ring_color();
    (el.style.shadow_blur - FOCUS_RING_BLUR).abs() <= 0.01
        && (el.style.shadow_color[0] - c[0]).abs() <= 0.001
        && (el.style.shadow_color[1] - c[1]).abs() <= 0.001
        && (el.style.shadow_color[2] - c[2]).abs() <= 0.001
        && (el.style.shadow_color[3] - c[3]).abs() <= 0.001
}

/// Per-frame sync for opt-in [`Element::focus_ring`] controls (not Button / Input).
///
/// Clears ring chrome only when the current shadow still matches the ring, so rest / pressed
/// shadows from other sync paths are preserved.
pub fn sync_focus_ring_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    // Button owns its spring-blended ring; Input/Textarea paint their own always-on ring.
    if el.button_hover_paint.is_none() && el.input_paint_sync.is_none() && el.focus_ring {
        if let Some(fid) = el.focus_id.clone() {
            let show = control_focus_ring_active(&fid, el.focusable, false);
            if show {
                if !focus_ring_shadow_matches(el, true) {
                    apply_focus_ring_shadow(el, true, false);
                    if el.layout_node.is_some() {
                        layout_tree.update_taffy_node_style(el);
                    }
                    *impact = impact.merge(LayoutStatus::StyleChanged);
                }
            } else if focus_ring_shadow_matches(el, true) {
                apply_focus_ring_shadow(el, false, true);
                if el.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(el);
                }
                *impact = impact.merge(LayoutStatus::StyleChanged);
            }
        }
    }
    for child in &mut el.children {
        sync_focus_ring_subtree(child, layout_tree, impact);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_and_match_ring() {
        let mut el = Element::new();
        apply_focus_ring_shadow(&mut el, true, true);
        assert!(focus_ring_shadow_matches(&el, true));
        apply_focus_ring_shadow(&mut el, false, true);
        assert!(focus_ring_shadow_matches(&el, false));
    }

    #[test]
    fn ring_matches_input_token_and_blur() {
        let mut el = Element::new();
        apply_focus_ring_shadow(&mut el, true, true);
        assert!((el.style.shadow_blur - FOCUS_RING_BLUR).abs() < f32::EPSILON);
        assert_eq!(el.style.shadow_offset, [0.0, 0.0]);
        assert!((el.style.shadow_spread).abs() < f32::EPSILON);
        let c = focus_ring_color();
        assert!((el.style.shadow_color[3] - c[3]).abs() < 0.001);
    }

    #[test]
    fn clear_only_when_requested() {
        let mut el = Element::new();
        el.style.shadow_blur = 2.0;
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.08];
        apply_focus_ring_shadow(&mut el, false, false);
        assert!((el.style.shadow_blur - 2.0).abs() < f32::EPSILON);
    }
}
