use super::*;
#[cfg(feature = "animation-base")]
use crate::style::{set_css_transition_timeline_ms, set_css_transition_virtual_timeline};
use crate::style::{TimingFunction, TransitionProperty};

#[test]
fn motion_style_dedupes_target_unchanged() {
    let mut m = MotionStyle::new();
    let mut style = crate::style::Style::default();
    let t = MotionTransform::uniform_scale([1.0, 2.0], 0.5, 1.0);
    assert!(m.apply(&mut style, t, MotionEpsilon::default()));
    assert!(!m.apply(&mut style, t, MotionEpsilon::default()));
    m.reset();
    assert!(m.apply(&mut style, t, MotionEpsilon::default()));
}

#[test]
fn ensure_motion_transitions_idempotent() {
    let entries = [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 300.0,
        delay_ms: 0.0,
        timing: TimingFunction::Ease,
    }];
    let mut style = crate::style::Style::default();
    ensure_motion_transitions(&mut style, &entries);
    let ptr = style.transition.as_ptr();
    ensure_motion_transitions(&mut style, &entries);
    assert_eq!(style.transition.as_ptr(), ptr);
    assert_eq!(style.transition.len(), 1);
}

#[test]
#[cfg(feature = "animation-base")]
fn snap_baseline_then_new_target_animates_via_motion_helpers() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    el.style.translate = [-100.0, 0.0];
    ensure_motion_transitions(
        &mut el.style,
        &[TransitionEntry {
            property: TransitionProperty::Transform,
            duration_ms: 100.0,
            delay_ms: 0.0,
            timing: TimingFunction::Linear,
        }],
    );
    snap_motion_baseline(&mut el);
    el.style.translate = [-300.0, 0.0];
    assert!(motion_is_animating(&el.style, &el.transition_snapshot));
    let _ = el.tick_css_transitions(0.0);
    set_css_transition_timeline_ms(50.0);
    el.style.translate = [-300.0, 0.0];
    let _ = el.tick_css_transitions(0.0);
    assert!(
        (el.style.translate[0] + 200.0).abs() < 1.0,
        "mid should be between -100 and -300, got {}",
        el.style.translate[0]
    );
    set_css_transition_virtual_timeline(false);
}

#[cfg(feature = "animation-base")]
fn flip_entry(prop: TransitionProperty) -> TransitionEntry {
    TransitionEntry {
        property: prop,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    }
}

#[cfg(feature = "animation-base")]
fn ensure_after_snap_mid(prop: TransitionProperty) -> f32 {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut el = Element::new();
    el.style.translate = [0.0, 0.0];
    let mut tracker = MotionStyle::new();
    let entries = [flip_entry(prop)];
    prepare_motion_sync(
        &mut el,
        &entries,
        Some(MotionBootstrap::EnsureAfterSnap),
        &mut tracker,
        false,
    );
    let target = MotionTransform {
        translate: [0.0, 40.0],
        translate_pct: el.style.translate_pct,
        opacity: 1.0,
        scale: el.style.scale,
    };
    tracker.apply(&mut el.style, target, MotionEpsilon::default());
    assert!(
        motion_is_animating(&el.style, &el.transition_snapshot),
        "{prop:?} EnsureAfterSnap must seed a baseline tween"
    );
    let _ = el.tick_css_transitions(0.0);
    set_css_transition_timeline_ms(50.0);
    el.style.translate = [0.0, 40.0];
    let _ = el.tick_css_transitions(0.0);
    let mid = el.style.translate[1];
    set_css_transition_virtual_timeline(false);
    mid
}

#[test]
#[cfg(feature = "animation-base")]
fn ensure_after_snap_transform_tweens_from_old_translate() {
    let mid = ensure_after_snap_mid(TransitionProperty::Transform);
    assert!(
        mid > 8.0 && mid < 36.0,
        "Transform mid should be between 0 and 40, got {mid}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn ensure_after_snap_translate_tweens_from_old_translate() {
    let mid = ensure_after_snap_mid(TransitionProperty::Translate);
    assert!(
        mid > 8.0 && mid < 36.0,
        "Translate mid should be between 0 and 40, got {mid}"
    );
}
