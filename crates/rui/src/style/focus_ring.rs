//! Shared focus-ring chrome (theme `ring` color + blur).
//!
//! Button uses spring-blended rings; Input and other controls can call [`apply_focus_ring_shadow`]
//! for an instant ring when focused.

use crate::core::tree::Element;
use crate::style::parse_color;

/// Default focus-ring blur in logical px (matches Button / Input legacy).
pub const FOCUS_RING_BLUR: f32 = 3.0;

/// Theme `ring` color as linear RGBA for [`crate::style::Style::shadow_color`].
pub fn focus_ring_color() -> [f32; 4] {
    parse_color("ring")
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
}
