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
