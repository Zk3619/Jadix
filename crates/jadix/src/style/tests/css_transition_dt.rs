use super::*;
use crate::style::element_style::{TransitionEntry, TransitionProperty};
use crate::style::TimingFunction;

#[test]
#[cfg(feature = "animation-base")]
fn spring_transition_retargets_from_current_value() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        opacity: 0.0,
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 0.0,
        delay_ms: 0.0,
        timing: TimingFunction::default_spring(),
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.opacity = 1.0;
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = style.opacity;
    assert!(mid > 0.05 && mid < 0.95, "spring mid {mid}");
    style.opacity = 0.0;
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.opacity - mid).abs() < 0.2,
        "retarget must start near mid {mid}, got {}",
        style.opacity
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn monotonic_timeline_ignores_dt_storm() {
    set_css_transition_virtual_timeline(false);
    let mut style = crate::style::Style {
        scale: [1.0, 1.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 150.0,
        delay_ms: 0.0,
        timing: TimingFunction::Ease,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.scale = [1.5, 1.0];
    let t0 = Instant::now();
    assert!(tick_element_transitions(&mut style, &mut snap, 1.0));
    // Invented 1s dt × many ticks must not finish the tween — progress is Instant-based.
    for _ in 0..20 {
        style.scale = [1.5, 1.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 1.0);
    }
    let mono_ms = t0.elapsed().as_secs_f32() * 1000.0;
    assert!(
        snap.has_active_tweens() || mono_ms >= 140.0,
        "monotonic tween must stay active unless real Instant covered the duration"
    );
    if mono_ms < 40.0 {
        assert!(
            style.scale[0] < 1.35,
            "invented dt must not finish a 150ms tween in {mono_ms}ms (scale={})",
            style.scale[0]
        );
    }
}

#[test]
#[cfg(feature = "animation-base")]
fn duration_authored_css_spring_finishes_at_its_declared_time() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut el = crate::core::tree::Element::new().transition("transform spring(800ms, 0.35)");
    let mut snap = TransitionSnapshot::default();
    tick_element_transitions(&mut el.style, &mut snap, 0.0);
    el.style.translate[0] = 196.0;
    tick_element_transitions(&mut el.style, &mut snap, 0.0);
    set_css_transition_timeline_ms(400.0);
    assert!(tick_element_transitions(&mut el.style, &mut snap, 0.0));
    set_css_transition_timeline_ms(800.0);
    assert!(!tick_element_transitions(&mut el.style, &mut snap, 0.0));
    assert_eq!(el.style.translate[0], 196.0);
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn virtual_timeline_advances_only_when_seeked() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        scale: [1.0, 1.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 150.0,
        delay_ms: 0.0,
        timing: TimingFunction::Ease,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.scale = [1.5, 1.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(16.0);
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    let after_one = style.scale[0];
    assert!(after_one > 1.0 && after_one < 1.5, "got {after_one}");
    for _ in 0..40 {
        style.scale = [1.5, 1.0];
        let _ = tick_element_transitions(&mut style, &mut snap, 1.0);
    }
    assert!(
        (style.scale[0] - after_one).abs() < 0.02,
        "passing dt must not invent progress; only timeline seeks move time"
    );
    advance_css_transition_timeline_ms(200.0);
    style.scale = [1.5, 1.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        !snap.has_active_tweens(),
        "virtual 150ms+ tween should settle"
    );
    assert!((style.scale[0] - 1.5).abs() < 0.01);
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn retarget_continues_from_current_computed_value() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        opacity: 0.5,
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.opacity = 1.0;
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(100.0); // mid-way → 0.75 on linear
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = style.opacity;
    assert!(
        (mid - 0.75).abs() < 0.02,
        "expected ~0.75 at 100ms, got {mid}"
    );
    // Leave: retarget to 0.5 from current (~0.75), not from 1.0.
    style.opacity = 0.5;
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    assert!((snap.tweens.get(&TransitionProperty::Opacity).unwrap().from[0] - mid).abs() < 0.02);
    set_css_transition_timeline_ms(100.0); // same now — just retargeted, elapsed≈0
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.opacity - mid).abs() < 0.05,
        "right after leave retarget, value should stay near mid ({mid}), got {}",
        style.opacity
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn transform_transition_includes_translate() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [0.0, 0.0],
        scale: [1.0, 1.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.translate = [-200.0, 0.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    style.translate = [-200.0, 0.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate[0] + 100.0).abs() < 1.0,
        "mid translate should be ~-100, got {}",
        style.translate[0]
    );
    set_css_transition_timeline_ms(100.0);
    style.translate = [-200.0, 0.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate[0] + 200.0).abs() < 0.01,
        "settled translate should be -200, got {}",
        style.translate[0]
    );
    assert!(!snap.has_active_tweens());
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn duration_tween_idle_instant_jump_keeps_a_mid_frame() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_max_step_ms(Some(1000.0 / 60.0));
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [0.0, 0.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 300.0,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.translate = [0.0, 800.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(400.0);
    style.translate = [0.0, 800.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        snap.has_active_tweens(),
        "idle Instant jump must not finish a 300ms tween in one tick"
    );
    assert!(
        style.translate[1] > 1.0 && style.translate[1] < 400.0,
        "first post-idle sample must be a mid, not the end (got {})",
        style.translate[1]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn spring_tween_idle_instant_jump_keeps_a_mid_frame() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_max_step_ms(Some(1000.0 / 60.0));
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [0.0, 0.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 0.0,
        delay_ms: 0.0,
        timing: TimingFunction::default_spring(),
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.translate = [0.0, 800.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(400.0);
    style.translate = [0.0, 800.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        snap.has_active_tweens(),
        "idle Instant jump must not finish a spring in one tick"
    );
    assert!(
        style.translate[1] > 0.01 && style.translate[1] < 400.0,
        "first post-idle spring sample must be a start/mid, not the end (got {})",
        style.translate[1]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn bind_specified_translate_commit_keeps_first_paint_at_start() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut el = crate::core::tree::Element::new();
    el.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 400.0,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    });
    let _ = el.tick_css_transitions(0.0);
    crate::style::apply_class_list(&mut el, "translate-x-40");
    let specified = el.style.translate[0];
    assert!(
        specified > 100.0,
        "translate-x-40 writes the end pose ({specified})"
    );
    crate::style::commit_style_transitions_after_specified_write(&mut el);
    assert!(
        el.style.translate[0] < specified * 0.2,
        "first painted frame must stay near start, not the class target (got {})",
        el.style.translate[0]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn translate_transition_interpolates_percent() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate_pct: [0.0, 0.5],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.translate_pct = [0.0, 0.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    style.translate_pct = [0.0, 0.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate_pct[1] - 0.25).abs() < 0.02,
        "mid translate_pct should be ~0.25, got {}",
        style.translate_pct[1]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn duration_tween_sub_idle_frame_is_realtime() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_max_step_ms(Some(1000.0 / 60.0));
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [0.0, 0.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 300.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.translate = [0.0, 300.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(33.0);
    style.translate = [0.0, 300.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate[1] - 33.0).abs() < 1.5,
        "33ms frame must not be slowed to 16ms (got {})",
        style.translate[1]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn snap_baseline_then_new_target_still_animates() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [-100.0, 0.0],
        ..Default::default()
    };
    // Simulate Carousel seamless jump: pin at -100 and seed, then write new target -300.
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    // Simulate Carousel seamless jump: pin at -100 and seed, then write new target -300.
    snap_element_transitions_to_current(&style, &mut snap);
    style.translate = [-300.0, 0.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    style.translate = [-300.0, 0.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate[0] + 200.0).abs() < 1.0,
        "should animate from -100 toward -300, mid≈-200, got {}",
        style.translate[0]
    );
    set_css_transition_virtual_timeline(false);
}

/// bugs.md: `transition: translate` after snap baseline should still tween,
/// same as the verified `transform` path above.
#[test]
#[cfg(feature = "animation-base")]
fn snap_baseline_then_new_target_still_animates_with_translate() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        translate: [-100.0, 0.0],
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    snap_element_transitions_to_current(&style, &mut snap);
    assert!(
        snap.has_any_tweens(),
        "snap must seed a Translate tween (from=to=current, inactive)"
    );
    style.translate = [-300.0, 0.0];
    assert!(
        css_transition_needs_frame(&style, &snap),
        "new translate target after snap must request a frame"
    );
    assert!(
        tick_element_transitions(&mut style, &mut snap, 0.0),
        "tick must start an active Translate tween"
    );
    set_css_transition_timeline_ms(50.0);
    style.translate = [-300.0, 0.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.translate[0] + 200.0).abs() < 1.0,
        "should animate from -100 toward -300, mid≈-200, got {}",
        style.translate[0]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
fn snap_while_transition_empty_does_not_seed() {
    let mut style = crate::style::Style {
        translate: [0.0, -100.0],
        ..Default::default()
    };
    let mut snap = TransitionSnapshot::default();
    snap_element_transitions_to_current(&style, &mut snap);
    assert!(
        !snap.has_any_tweens(),
        "snap cannot choose properties when transition is empty"
    );
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Translate,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    style.translate = [0.0, 40.0];
    assert!(
        !tick_element_transitions(&mut style, &mut snap, 0.0),
        "first tick after empty-snap seeds from=to=new target and skips the tween"
    );
    assert_eq!(style.translate[1], 40.0);
}
