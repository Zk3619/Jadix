//! Shared focus-ring chrome (theme `ring` + [`FocusRingStyle`]).
//!
//! Geometry (theme-configurable; per-element [`Style::focus_ring_width`] / [`Style::focus_ring_color`]
//! override the theme):
//! - [`FocusRingStyle::Hard`] (default): ShadCN v4 `ring-[3px]` → blur 0, spread = width
//! - [`FocusRingStyle::Soft`]: soft glow → blur = width, spread 0
//!
//! Defaults are [`FOCUS_RING_HARD_SPREAD`] / [`FOCUS_RING_SOFT_BLUR`] (both 3 logical px).
//! Width and color are **not** hardcoded at the apply site — they come from the active
//! [`Theme`] (`theme.toml` `focus_ring_width` / token `ring`) or the element's authored style
//! (`ring-2`, `.ring_width(4.0)`, `.ring_color("primary")`).
//!
//! Both are **outset** box-shadows (drawn outside the control box), independent of whether the
//! control has a CSS border — same as Tailwind `ring-*` / solid-ui / shadcn. Optional
//! `border-ring` is an extra cue for bordered controls only.
//!
//! Destructive/invalid callers pass a fallback color; an authored [`Style::focus_ring_color`]
//! wins. Single primary shadow slot only — no ring-offset / `extra_shadows`.
//!
//! - **Input / Textarea**: always ring while focused (text_edit paint; ignore `__jadix_focus_visible`).
//! - **Other controls**: [`Element::focusable`]`(true)` enables [`Element::focus_ring`] by default.
//!   Ring only when focused **and** `__jadix_focus_visible` (keyboard Tab). Pointer down clears
//!   that flag so mouse focus has no ring. Opt out with `.focus_ring(false)` after `.focusable(true)`.
//! - **Button**: snaps the same helpers in `button.rs` (skipped here to avoid double-apply).

use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::style::element_style::Style;
use crate::style::parse_color;
use crate::style::theme::{
    focus_ring_style, focus_ring_width, FocusRingStyle, DEFAULT_FOCUS_RING_WIDTH,
};

/// Soft focus-ring blur in logical px when the theme width is left at default
/// (`FocusRingStyle::Soft`). Runtime value is [`focus_ring_width`].
pub const FOCUS_RING_SOFT_BLUR: f32 = DEFAULT_FOCUS_RING_WIDTH;

/// Hard focus-ring spread in logical px when the theme width is left at default
/// (`FocusRingStyle::Hard`, ShadCN `ring-[3px]`). Runtime value is [`focus_ring_width`].
pub const FOCUS_RING_HARD_SPREAD: f32 = DEFAULT_FOCUS_RING_WIDTH;

/// Legacy alias for soft blur (Soft style). Prefer [`FOCUS_RING_SOFT_BLUR`] / [`focus_ring_params`].
pub const FOCUS_RING_BLUR: f32 = FOCUS_RING_SOFT_BLUR;

/// `(blur, spread)` for the given style using the **theme** width.
pub fn focus_ring_params(style: FocusRingStyle) -> (f32, f32) {
    focus_ring_params_width(style, focus_ring_width())
}

/// `(blur, spread)` honoring `style.focus_ring_width` then the theme width.
pub fn focus_ring_params_for_style(style: &Style) -> (f32, f32) {
    let w = style.focus_ring_width.unwrap_or_else(focus_ring_width);
    focus_ring_params_width(focus_ring_style(), w)
}

/// [`focus_ring_params_for_style`] for an element.
pub fn focus_ring_params_for(el: &Element) -> (f32, f32) {
    focus_ring_params_for_style(&el.style)
}

fn focus_ring_params_width(style: FocusRingStyle, width: f32) -> (f32, f32) {
    let w = width.max(0.0);
    match style {
        FocusRingStyle::Hard => (0.0, w),
        FocusRingStyle::Soft => (w, 0.0),
    }
}

/// Authored ring color, or `fallback` (theme `ring` / invalid destructive).
pub fn focus_ring_resolved_color(style: &Style, fallback: [f32; 4]) -> [f32; 4] {
    style.focus_ring_color.unwrap_or(fallback)
}

/// Theme `ring` color as linear RGBA (`ring-ring/50`).
pub fn focus_ring_color() -> [f32; 4] {
    parse_color("ring")
}

/// Opaque border color for `focus-visible:border-ring` (CSS `--ring` without `/50`).
pub fn focus_ring_border_color() -> [f32; 4] {
    let mut c = focus_ring_color();
    c[3] = 1.0;
    c
}

/// Destructive invalid / destructive-button ring (`ring-destructive/20`).
pub fn focus_ring_destructive_color() -> [f32; 4] {
    let mut c = parse_color("destructive");
    c[3] = 0.2;
    c
}

/// Whether this node should show the keyboard focus ring right now.
pub fn control_focus_ring_active(focus_id: &str, focusable: bool, disabled_skip: bool) -> bool {
    if !focusable || disabled_skip || focus_id.is_empty() {
        return false;
    }
    let focused = use_signal("__jadix_button_focus_id", String::new).peek();
    let focus_visible = use_signal("__jadix_focus_visible", || false).peek();
    focus_visible && focused == focus_id
}

/// Write focus-ring shadow fields onto `el` (or clear when unfocused).
pub fn apply_focus_ring_shadow(el: &mut Element, focused: bool, clear_when_unfocused: bool) {
    apply_focus_ring_shadow_color(el, focused, clear_when_unfocused, focus_ring_color());
}

/// Like [`apply_focus_ring_shadow`] with an explicit outer ring color.
///
/// `ring_color` is the fallback when the element has no authored [`Style::focus_ring_color`].
pub fn apply_focus_ring_shadow_color(
    el: &mut Element,
    focused: bool,
    clear_when_unfocused: bool,
    ring_color: [f32; 4],
) {
    if focused {
        let (blur, spread) = focus_ring_params_for(el);
        el.style.shadow_color = focus_ring_resolved_color(&el.style, ring_color);
        el.style.shadow_blur = blur;
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_spread = spread;
        el.style.shadow_inset = false;
    } else if clear_when_unfocused {
        el.style.shadow_blur = 0.0;
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.0];
        el.style.shadow_offset = [0.0, 0.0];
        el.style.shadow_spread = 0.0;
        el.style.shadow_inset = false;
    }
}

/// Whether `el` already matches the focused ring for the **current** [`FocusRingStyle`].
pub fn focus_ring_shadow_matches(el: &Element, focused: bool) -> bool {
    focus_ring_shadow_matches_color(el, focused, focus_ring_color())
}

/// Like [`focus_ring_shadow_matches`] with an explicit outer ring color.
pub fn focus_ring_shadow_matches_color(el: &Element, focused: bool, ring_color: [f32; 4]) -> bool {
    if !focused {
        return el.style.shadow_blur.abs() <= 0.01 && el.style.shadow_spread.abs() <= 0.01;
    }
    let (blur, spread) = focus_ring_params_for(el);
    let ring_color = focus_ring_resolved_color(&el.style, ring_color);
    (el.style.shadow_blur - blur).abs() <= 0.01
        && (el.style.shadow_spread - spread).abs() <= 0.01
        && el.style.shadow_offset == [0.0, 0.0]
        && !el.style.shadow_inset
        && (el.style.shadow_color[0] - ring_color[0]).abs() <= 0.001
        && (el.style.shadow_color[1] - ring_color[1]).abs() <= 0.001
        && (el.style.shadow_color[2] - ring_color[2]).abs() <= 0.001
        && (el.style.shadow_color[3] - ring_color[3]).abs() <= 0.001
}

/// Per-frame sync for [`Element::focus_ring`] controls (not Button / Input).
///
/// Clears ring chrome only when the current shadow still matches the ring, so rest / pressed
/// shadows from other sync paths are preserved.
pub fn sync_focus_ring_subtree(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    impact: &mut LayoutStatus,
) {
    // Button owns its focus ring; Input/Textarea paint their own always-on ring.
    if !el.owns_focus_paint && el.input_paint_sync.is_none() && el.focus_ring {
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
    use crate::style::theme::{
        focus_ring_width, set_focus_ring_style, set_focus_ring_width, FocusRingStyle,
    };
    use crate::style::{apply_class_list, parse_color};

    fn with_style<R>(style: FocusRingStyle, f: impl FnOnce() -> R) -> R {
        let prev = focus_ring_style();
        set_focus_ring_style(style);
        let out = f();
        set_focus_ring_style(prev);
        out
    }

    fn with_width<R>(width: f32, f: impl FnOnce() -> R) -> R {
        let prev = focus_ring_width();
        set_focus_ring_width(width);
        let out = f();
        set_focus_ring_width(prev);
        out
    }

    #[test]
    fn apply_and_match_ring_hard_default() {
        with_style(FocusRingStyle::Hard, || {
            let mut el = Element::new();
            apply_focus_ring_shadow(&mut el, true, true);
            assert!(focus_ring_shadow_matches(&el, true));
            assert!((el.style.shadow_spread - FOCUS_RING_HARD_SPREAD).abs() < f32::EPSILON);
            assert!(el.style.shadow_blur.abs() < f32::EPSILON);
            apply_focus_ring_shadow(&mut el, false, true);
            assert!(focus_ring_shadow_matches(&el, false));
        });
    }

    #[test]
    fn soft_ring_uses_blur_not_spread() {
        with_style(FocusRingStyle::Soft, || {
            let mut el = Element::new();
            apply_focus_ring_shadow(&mut el, true, true);
            assert!(focus_ring_shadow_matches(&el, true));
            assert!((el.style.shadow_blur - FOCUS_RING_SOFT_BLUR).abs() < f32::EPSILON);
            assert!(el.style.shadow_spread.abs() < f32::EPSILON);
        });
    }

    #[test]
    fn style_switch_invalidates_match() {
        let mut el = Element::new();
        with_style(FocusRingStyle::Hard, || {
            apply_focus_ring_shadow(&mut el, true, true);
            assert!(focus_ring_shadow_matches(&el, true));
        });
        with_style(FocusRingStyle::Soft, || {
            assert!(!focus_ring_shadow_matches(&el, true));
            apply_focus_ring_shadow(&mut el, true, true);
            assert!(focus_ring_shadow_matches(&el, true));
        });
    }

    #[test]
    fn clear_only_when_requested() {
        let mut el = Element::new();
        el.style.shadow_blur = 2.0;
        el.style.shadow_color = [0.0, 0.0, 0.0, 0.08];
        apply_focus_ring_shadow(&mut el, false, false);
        assert!((el.style.shadow_blur - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn params_table() {
        with_width(FOCUS_RING_HARD_SPREAD, || {
            assert_eq!(
                focus_ring_params(FocusRingStyle::Hard),
                (0.0, FOCUS_RING_HARD_SPREAD)
            );
            assert_eq!(
                focus_ring_params(FocusRingStyle::Soft),
                (FOCUS_RING_SOFT_BLUR, 0.0)
            );
        });
    }

    #[test]
    fn theme_width_drives_hard_and_soft_params() {
        with_width(8.0, || {
            with_style(FocusRingStyle::Hard, || {
                assert_eq!(focus_ring_params(FocusRingStyle::Hard), (0.0, 8.0));
                let mut el = Element::new();
                apply_focus_ring_shadow(&mut el, true, true);
                assert!((el.style.shadow_spread - 8.0).abs() < f32::EPSILON);
            });
            with_style(FocusRingStyle::Soft, || {
                assert_eq!(focus_ring_params(FocusRingStyle::Soft), (8.0, 0.0));
            });
        });
    }

    #[test]
    fn element_ring_width_and_color_override_theme() {
        with_style(FocusRingStyle::Hard, || {
            with_width(3.0, || {
                let mut el = Element::new().ring_width(5.0).ring_color("primary");
                apply_focus_ring_shadow(&mut el, true, true);
                assert!((el.style.shadow_spread - 5.0).abs() < f32::EPSILON);
                let want = parse_color("primary");
                assert!((el.style.shadow_color[0] - want[0]).abs() < 0.001);
                assert!((el.style.shadow_color[1] - want[1]).abs() < 0.001);
                assert!((el.style.shadow_color[2] - want[2]).abs() < 0.001);
                assert!(focus_ring_shadow_matches(&el, true));
            });
        });
    }

    #[test]
    fn ring_utility_classes_set_specified_width_and_color() {
        with_style(FocusRingStyle::Hard, || {
            let mut el = Element::new();
            apply_class_list(&mut el, "ring-2 ring-primary");
            assert_eq!(el.style.focus_ring_width, Some(2.0));
            let want = parse_color("primary");
            let got = el.style.focus_ring_color.expect("ring color");
            assert!((got[0] - want[0]).abs() < 0.001);
            apply_focus_ring_shadow(&mut el, true, true);
            assert!((el.style.shadow_spread - 2.0).abs() < f32::EPSILON);
            assert!((el.style.shadow_color[0] - want[0]).abs() < 0.001);
        });
    }

    #[test]
    fn focusable_enables_keyboard_ring_by_default() {
        let el = Element::new().focusable(true);
        assert!(el.focusable);
        assert!(el.focus_ring);
        let opted = Element::new().focusable(true).focus_ring(false);
        assert!(opted.focusable);
        assert!(!opted.focus_ring);
        let off = Element::new().focusable(true).focusable(false);
        assert!(!off.focusable);
        assert!(!off.focus_ring);
    }
}
