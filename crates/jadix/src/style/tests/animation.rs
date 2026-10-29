use super::*;

#[test]
#[cfg(feature = "animation-base")]
fn cubic_set_same_target_does_not_restart_elapsed() {
    let tw = CubicTween::new_ui_transition(0.0);
    tw.set(1.0);
    let _ = tick_animations(0.05);
    assert!(tw.peek() > 0.0, "expected initial tween progress");
    // Chrome sync style: set the same target again each frame
    tw.set(1.0);
    tw.set(1.0);
    // Retained clock settles at 150ms; a reset clock would still be active.
    for _ in 0..6 {
        let _ = tick_animations(0.02);
    }
    assert!(!tw.is_active());
    assert!((tw.peek() - 1.0).abs() < 1e-3);
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_clamps_near_zero_dt_so_first_frames_still_advance() {
    let tw = CubicTween::new_ui_transition(0.0);
    tw.set(1.0);
    // First-screen back-to-back frames / repeated RedrawRequested often yield raw dt≈0
    let _ = tick_animations(0.0);
    assert!(tw.is_active());
    assert!(tw.peek() > 0.0, "near-zero dt must still advance");
    assert!(any_ui_animation_active());
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_clamps_huge_dt_so_idle_wake_does_not_teleport() {
    let tw = CubicTween::new_ui_transition(0.0);
    tw.set(1.0);
    // First frame after Wait for several seconds: without clamping, 150ms would finish in one step
    let _ = tick_animations(2.0);
    assert!(
        tw.is_active(),
        "huge dt must not finish a 150ms tween in one tick"
    );
    let v = tw.peek();
    assert!(
        v > 0.01 && v < 0.99,
        "expected mid-tween after clamped step, got {v}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn hot_reload_keyframes_restart_changed_definitions_but_keep_identical_clocks() {
    use crate::style::keyframes::{register_keyframes, KeyframeStop};
    let definition = vec![
        KeyframeStop::at(0.0).opacity(0.0),
        KeyframeStop::at(1.0).opacity(1.0),
    ];
    register_keyframes("hr-clock", definition.clone());
    let mut style = crate::style::Style::default();
    style.animations.push(AnimationEntry {
        name: "hr-clock".into(),
        duration_ms: 1000.0,
        ..Default::default()
    });
    let mut snapshot = AnimationSnapshot::default();
    tick_element_animations(&mut style, &mut snapshot, 0.08);
    tick_element_animations(&mut style, &mut snapshot, 0.08);
    let elapsed = snapshot.runs[0].elapsed_ms;
    register_keyframes("hr-clock", definition);
    tick_element_animations(&mut style, &mut snapshot, 0.08);
    assert!(snapshot.runs[0].elapsed_ms > elapsed);
    register_keyframes(
        "hr-clock",
        vec![
            KeyframeStop::at(0.0).opacity(0.2),
            KeyframeStop::at(1.0).opacity(0.8),
        ],
    );
    tick_element_animations(&mut style, &mut snapshot, 0.08);
    assert!(snapshot.runs[0].elapsed_ms <= 80.1);
    style.animations.clear();
    tick_element_animations(&mut style, &mut snapshot, 0.08);
    assert!(snapshot.runs.is_empty());
    assert_eq!(style.opacity, 1.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_element_animations_spin_advances_rotate() {
    use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
    let mut style = crate::style::Style::default();
    style.animations.push(AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Infinite,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    // Engine clamps dt to ≤80ms per tick.
    for _ in 0..4 {
        assert!(tick_element_animations(&mut style, &mut snap, 0.08));
    }
    assert!(
        (style.rotate_deg - 115.2).abs() < 2.0,
        "expected ~115deg after 320ms of 1s spin, got {}",
        style.rotate_deg
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_element_animations_zero_dt_storm_uses_monotonic_clock() {
    // Gallery Marker: CursorMoved over the row requests extra paints with dt≈0.
    // Summing that froze loader-circle; flooring it would fast-forward during the storm.
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.animations.push(crate::style::AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        timing: crate::style::TimingFunction::Linear,
        iteration_count: crate::style::AnimationIterationCount::Infinite,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    assert!(tick_element_animations(&mut style, &mut snap, 0.0));
    let at_origin = style.rotate_deg;
    for _ in 0..12 {
        assert!(tick_element_animations(&mut style, &mut snap, 0.0));
    }
    assert!(
        (style.rotate_deg - at_origin).abs() < 1.0,
        "virtual clock must not invent spin from zero dt, origin={at_origin} now={}",
        style.rotate_deg
    );
    set_css_transition_timeline_ms(80.0);
    assert!(tick_element_animations(&mut style, &mut snap, 0.0));
    assert!(
        (style.rotate_deg - 28.8).abs() < 2.0,
        "dt≈0 extra frame must still sample wall/virtual time (expected ~28.8deg, got {})",
        style.rotate_deg
    );
    // Page-wide CursorMoved extra paints often report 1–8ms, not dt≈0 — enough
    // to take the frame-dt branch and ignore the CSS timeline (spin crawls).
    set_css_transition_timeline_ms(160.0);
    assert!(tick_element_animations(&mut style, &mut snap, 0.002));
    assert!(
        (style.rotate_deg - 57.6).abs() < 2.0,
        "tiny extra-frame dt must not starve the CSS timeline (expected ~57.6deg, got {})",
        style.rotate_deg
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn late_frame_must_not_drop_spin_time() {
    // Pointer motion blocks the GUI thread. The next present is late by
    // hundreds of ms and still reports a tiny frame dt. Capping the gap at
    // 80ms and stamping the clock to *now* deletes the rest — spin freezes
    // for the whole slide, then the icon pops when the mouse stops.
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.animations.push(crate::style::AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        timing: crate::style::TimingFunction::Linear,
        iteration_count: crate::style::AnimationIterationCount::Infinite,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    assert!(tick_element_animations(&mut style, &mut snap, 1.0 / 60.0));
    let seeded = style.rotate_deg;
    set_css_transition_timeline_ms(240.0);
    assert!(tick_element_animations(&mut style, &mut snap, 0.002));
    let advanced = style.rotate_deg - seeded;
    assert!(
            (advanced - 86.4).abs() < 3.0,
            "a 240ms late present must rotate the full gap, not the 80ms idle cap (seed {seeded:.1} now {:.1}, delta {advanced:.1})",
            style.rotate_deg
        );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_element_animations_respects_paused() {
    use crate::style::{
        AnimationEntry, AnimationIterationCount, AnimationPlayState, TimingFunction,
    };
    let mut style = crate::style::Style::default();
    style.animations.push(AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Infinite,
        play_state: AnimationPlayState::Paused,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.25);
    assert!(
        style.rotate_deg.abs() < 1.0,
        "paused should not advance elapsed (got {})",
        style.rotate_deg
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn pause_resume_preserves_elapsed() {
    use crate::style::{
        AnimationEntry, AnimationIterationCount, AnimationPlayState, TimingFunction,
    };
    let mut style = crate::style::Style::default();
    style.animations.push(AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Infinite,
        play_state: AnimationPlayState::Running,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    // Engine clamps dt to ≤80ms per tick → 3×80ms ≈ 86.4deg.
    for _ in 0..3 {
        assert!(tick_element_animations(&mut style, &mut snap, 0.08));
    }
    let mid = style.rotate_deg;
    assert!(mid > 70.0 && mid < 100.0, "expected ~86deg, got {mid}");

    style.animations[0].play_state = AnimationPlayState::Paused;
    let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    assert!(
        (style.rotate_deg - mid).abs() < 1.0,
        "pause must not reset elapsed (mid={mid}, after={})",
        style.rotate_deg
    );

    style.animations[0].play_state = AnimationPlayState::Running;
    let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    assert!(
        style.rotate_deg > mid + 20.0,
        "resume should continue from mid={mid}, got {}",
        style.rotate_deg
    );
}

#[test]
fn tick_element_animations_forwards_keeps_end() {
    use crate::style::{
        AnimationEntry, AnimationFillMode, AnimationIterationCount, TimingFunction,
    };
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(AnimationEntry {
        name: "fade-out".into(),
        duration_ms: 100.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Finite(1.0),
        fill_mode: AnimationFillMode::Forwards,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    for _ in 0..6 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.05);
    }
    assert!(
        style.opacity < 0.05,
        "forwards fade-out should stay near 0, got {}",
        style.opacity
    );
}

/// The component assigns a target **once**; every later frame just re-renders the
/// value the engine wrote. Reading that back as a new target used to retarget the
/// tween to itself each frame, freezing it a few percent from the start.
#[test]
#[cfg(feature = "animation-base")]
fn transition_runs_to_completion_without_being_respecified() {
    use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        opacity: 0.0,
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

    style.opacity = 1.0; // the one and only assignment
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));

    set_css_transition_timeline_ms(100.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = style.opacity;
    assert!(
        (mid - 0.5).abs() < 0.05,
        "expected ~0.5 at 100ms, got {mid}"
    );

    set_css_transition_timeline_ms(200.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.opacity - 1.0).abs() < 0.01,
        "tween must reach the specified target, got {}",
        style.opacity
    );
    assert!(!snap.has_active_tweens());
    set_css_transition_virtual_timeline(false);
}

/// `transition: all` sweeps in `max-height`, which is `auto` on most boxes. Resolving
/// `auto` to `0` collapsed accordion panels the moment anything animated.
#[test]
fn transition_all_leaves_auto_max_height_alone() {
    use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.transition.push(TransitionEntry {
        property: TransitionProperty::All,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    assert!(matches!(
        crate::layout::dim_kind(style.max_size.height),
        crate::layout::DimKind::Auto
    ));
    let mut snap = TransitionSnapshot::default();
    for ms in [0.0, 50.0, 200.0] {
        set_css_transition_timeline_ms(ms);
        let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    }
    assert!(
        matches!(
            crate::layout::dim_kind(style.max_size.height),
            crate::layout::DimKind::Auto
        ),
        "auto max-height must survive `transition: all`, got {:?}",
        style.max_size.height
    );
    assert!(!snap.tweens.contains_key(&TransitionProperty::MaxHeight));
    set_css_transition_virtual_timeline(false);
}

#[test]
fn from_css_recognizes_layout_properties() {
    use crate::style::TransitionProperty as P;
    assert_eq!(P::from_css("width"), P::Width);
    assert_eq!(P::from_css("min-width"), P::MinWidth);
    assert_eq!(P::from_css("min_width"), P::MinWidth);
    assert_eq!(P::from_css("height"), P::Height);
    assert_eq!(P::from_css("padding"), P::Padding);
    assert_eq!(P::from_css("padding-left"), P::PaddingLeft);
    assert_eq!(P::from_css("margin-top"), P::MarginTop);
    assert_eq!(P::from_css("gap"), P::Gap);
    assert_eq!(P::from_css("flex-grow"), P::FlexGrow);
    assert_eq!(P::from_css("border-radius"), P::BorderRadius);
    assert_eq!(P::from_css("zoom"), P::Zoom);
    assert_eq!(P::from_css("backdrop-filter"), P::BackdropFilter);
    assert_eq!(P::from_css("grid-template-rows"), P::GridTemplateRows);
    assert_eq!(P::from_css("grid-template-columns"), P::GridTemplateColumns);
    assert_eq!(P::from_css("background-position"), P::BackgroundPosition);
    assert_eq!(P::from_css("clip-path"), P::ClipPath);
    assert_eq!(P::from_css("scale"), P::Scale);
    assert_eq!(P::from_css("rotate"), P::Rotate);
    assert_eq!(P::from_css("display"), P::Unsupported);
}

#[test]
#[cfg(feature = "animation-base")]
fn width_length_tweens_auto_stays_discrete() {
    use taffy::style::Dimension;
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.size.width = Dimension::length(48.0);
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Width,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.size.width = Dimension::length(256.0);
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(100.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
    assert!(
        mid > 100.0 && mid < 220.0,
        "width must interpolate, got {mid}"
    );

    let mut auto_style = crate::style::Style::default();
    auto_style.transition.push(TransitionEntry {
        property: TransitionProperty::All,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut auto_snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut auto_style, &mut auto_snap, 0.0);
    assert!(
        matches!(
            crate::layout::dim_kind(auto_style.size.width),
            crate::layout::DimKind::Auto
        ),
        "auto width must survive `transition: all`"
    );
    assert!(!auto_snap.tweens.contains_key(&TransitionProperty::Width));
    set_css_transition_virtual_timeline(false);
}

#[test]
fn copied_used_width_keeps_standing_target_across_paint() {
    use taffy::style::Dimension;
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.size.width = Dimension::length(48.0);
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Width,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(!snap.has_active_tweens());

    let mut painted = crate::style::Style::default();
    painted.size.width = Dimension::length(256.0);
    painted.transition = style.transition.clone();
    copy_transition_used_values(&mut painted, &style, &snap);
    assert_eq!(
        crate::layout::dim_length_px(painted.size.width),
        Some(48.0),
        "rebuild paint 16rem must not replace the used collapsed width"
    );
    let _ = tick_element_transitions(&mut painted, &mut snap, 0.0);
    assert!(
        !snap.has_active_tweens(),
        "matching used value must keep the standing rest target, not expand"
    );
    assert_eq!(crate::layout::dim_length_px(painted.size.width), Some(48.0));
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn assign_specified_does_not_clobber_mid_when_target_unchanged() {
    use taffy::style::Dimension;
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.size.width = Dimension::length(256.0);
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Width,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.size.width = Dimension::length(48.0);
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(100.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
    assign_transition_specified(
        &mut style,
        &snap,
        TransitionProperty::Width,
        [48.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );
    let after = crate::layout::dim_length_px(style.size.width).unwrap_or(-1.0);
    assert!(
        (after - mid).abs() < 0.5,
        "same-target assign must keep mid {mid}, got {after}"
    );
    set_css_transition_virtual_timeline(false);
}

/// An animation owns the properties its keyframes touch; the transition on the same
/// property must not chase the animation's output (CSS: animation wins, and an
/// animation-driven change never starts a transition).
#[test]
#[cfg(feature = "animation-base")]
fn animation_lock_parks_transition_on_owned_property() {
    use crate::style::{
        AnimationEntry, AnimationFillMode, AnimationIterationCount, TimingFunction,
        TransitionEntry, TransitionProperty,
    };
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    style.animations.push(AnimationEntry {
        name: "fade-out".into(),
        duration_ms: 200.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Finite(1.0),
        fill_mode: AnimationFillMode::Forwards,
        ..Default::default()
    });
    let mut anim = AnimationSnapshot::default();
    let mut trans = TransitionSnapshot::default();
    for _ in 0..3 {
        let _ = tick_element_animations(&mut style, &mut anim, 0.05);
        let _ = tick_element_transitions_locked(&mut style, &mut trans, anim.lock(), 0.0);
    }
    assert!(anim.lock().opacity, "fade-out must claim opacity");
    let after = style.opacity;
    assert!(
        after > 0.05 && after < 0.95,
        "animation should drive opacity mid-way, got {after}"
    );
    assert!(
        !trans
            .tweens
            .get(&TransitionProperty::Opacity)
            .is_some_and(|t| t.active),
        "transition must stay parked while the animation owns opacity"
    );
    set_css_transition_virtual_timeline(false);
}

/// Keyframes are layered over the cascaded value and re-sampled from it every frame;
/// dropping the animation hands that value back.
#[test]
#[cfg(feature = "animation-base")]
fn animation_resamples_from_base_and_restores_on_removal() {
    use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(AnimationEntry {
        name: "fade-out".into(),
        duration_ms: 400.0,
        timing: TimingFunction::Linear,
        iteration_count: AnimationIterationCount::Infinite,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    for _ in 0..2 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    }
    let mid = style.opacity;
    assert!(
        (mid - 0.6).abs() < 0.05,
        "160ms of a 400ms linear fade-out should read ~0.6, got {mid}"
    );

    style.animations.clear();
    let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    assert!(
        (style.opacity - 1.0).abs() < 1e-4,
        "removing the animation must restore the cascaded opacity, got {}",
        style.opacity
    );
}

/// shadcn composes `fade-in-0 zoom-in-95` on one element. Each entry must only write the
/// properties its own keyframes declare, or the later one snaps the earlier one back to
/// the base value.
#[test]
#[cfg(feature = "animation-base")]
fn stacked_animations_do_not_clobber_each_other() {
    use crate::style::{AnimationEntry, AnimationIterationCount, TimingFunction};
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    for name in ["fade-in-0", "zoom-in-95"] {
        style.animations.push(AnimationEntry {
            name: name.into(),
            duration_ms: 200.0,
            timing: TimingFunction::Linear,
            iteration_count: AnimationIterationCount::Finite(1.0),
            ..Default::default()
        });
    }
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.08);
    assert!(
        style.opacity > 0.05 && style.opacity < 0.95,
        "fade must survive the zoom entry, got {}",
        style.opacity
    );
    assert!(
        style.scale[0] > 0.95 && style.scale[0] < 1.0,
        "zoom must survive the fade entry, got {}",
        style.scale[0]
    );
    let lock = snap.lock();
    assert!(lock.opacity && lock.transform);
}

#[test]
fn max_height_keyframes_pin_min_height_zero_and_restore_on_clear() {
    use crate::layout::dim_kind;
    use crate::layout::DimKind;
    use crate::style::{AnimationFillMode, TimingFunction};
    let mut style = crate::style::Style::default();
    style.max_size.height = taffy::style::Dimension::length(48.0);
    style.animations.push(keyframes_anim_entry(
        "accordion-up",
        200.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut snap = AnimationSnapshot::default();
    assert!(matches!(dim_kind(style.min_size.height), DimKind::Auto));
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!(matches!(
        dim_kind(style.min_size.height),
        DimKind::Length(h) if h.abs() < f32::EPSILON
    ));
    for _ in 0..20 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    }
    assert!(
        !animation_run_active(&style, &snap),
        "finished finite run must not block presence exit"
    );
    style.animations.clear();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!(
        matches!(dim_kind(style.min_size.height), DimKind::Auto),
        "min-height must restore to cascaded auto after animation removal"
    );
}

#[test]
fn finished_animation_retains_fill_without_requesting_frames() {
    use crate::style::{AnimationFillMode, TimingFunction};
    let mut style = crate::style::Style::default();
    style.max_size.height = taffy::style::Dimension::length(48.0);
    style.animations.push(keyframes_anim_entry(
        "accordion-up",
        200.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut snap = AnimationSnapshot::default();
    for _ in 0..20 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    }
    assert!(
        !css_animation_needs_frame(&style, &snap),
        "finished animation must not keep the frame pump alive"
    );
    assert!(
        !animation_run_active(&style, &snap),
        "presence gates must use animation_run_active, not css_animation_needs_frame"
    );
}

#[test]
fn opacity_animation_preserves_auto_max_height() {
    use crate::layout::{dim_kind, DimKind};
    use crate::style::{AnimationFillMode, TimingFunction};
    let mut style = crate::style::Style::default();
    style.size.height = taffy::style::Dimension::length(32.0);
    style.animations.push(keyframes_anim_entry(
        "pulse",
        2000.0,
        TimingFunction::Linear,
        AnimationFillMode::None,
    ));
    style.animations[0].iteration_count = crate::style::AnimationIterationCount::Infinite;
    assert!(matches!(dim_kind(style.max_size.height), DimKind::Auto));
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    assert!(
        matches!(dim_kind(style.max_size.height), DimKind::Auto),
        "pulse/spin/bounce must not rewrite max-height:auto to 0px, got {:?}",
        dim_kind(style.max_size.height)
    );
}

#[test]
fn parse_animation_shorthand_spin_infinite() {
    let list = crate::core::tree::parse_animation_shorthand(
        "1s linear infinite reverse both running spin",
    );
    assert_eq!(list.len(), 1);
    let e = &list[0];
    assert_eq!(e.name, "spin");
    assert!((e.duration_ms - 1000.0).abs() < 0.1);
    assert!(matches!(
        e.iteration_count,
        crate::style::AnimationIterationCount::Infinite
    ));
    assert_eq!(e.direction, crate::style::AnimationDirection::Reverse);
    assert_eq!(e.fill_mode, crate::style::AnimationFillMode::Both);
    assert_eq!(e.play_state, crate::style::AnimationPlayState::Running);
}

#[test]
fn parse_animation_shorthand_keeps_linear_stops_token() {
    let list = crate::core::tree::parse_animation_shorthand(
        "2s linear(0, 0.05 50%, 1) infinite gal-linear-slide",
    );
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "gal-linear-slide");
    assert!(
        matches!(list[0].timing, crate::style::TimingFunction::LinearStops(_)),
        "linear() must parse as piecewise easing, not the spin keyframe, got {:?}",
        list[0].timing
    );
}

#[test]
fn parse_animation_shorthand_keeps_steps_token() {
    let list =
        crate::core::tree::parse_animation_shorthand("200ms steps(4, jump-end) both fade-in-0");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "fade-in-0");
    assert!(matches!(
        list[0].timing,
        crate::style::TimingFunction::Steps {
            n: 4,
            jump: crate::style::StepJumpTerm::JumpEnd
        }
    ));
}

#[test]
fn reduced_motion_snaps_keyframes_to_end() {
    set_prefers_reduced_motion(Some(true));
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(keyframes_anim_entry(
        "fade-out-0",
        400.0,
        TimingFunction::Linear,
        crate::style::AnimationFillMode::Forwards,
    ));
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    assert!(
        style.opacity < 0.05,
        "reduced motion must jump to the end sample, got {}",
        style.opacity
    );
    set_prefers_reduced_motion(None);
}

#[test]
#[cfg(feature = "animation-base")]
fn accordion_down_uses_used_layout_not_written_px() {
    use crate::style::UsedLayout;
    let mut style = crate::style::Style::default();
    style.max_size.height = taffy::style::Dimension::auto();
    style.animations.push(keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::Linear,
        crate::style::AnimationFillMode::Both,
    ));
    let mut text = crate::style::TextStyle::default();
    let mut used = UsedLayout::default();
    used.stamp_border_box([0.0, 0.0, 100.0, 80.0], false);
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
    let h = crate::layout::dim_length_px(style.max_size.height).unwrap_or(999.0);
    assert!(h < 2.0, "fill-mode both at t=0 must start at 0, got {h}");
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.1);
    let mid = crate::layout::dim_length_px(style.max_size.height).unwrap_or(0.0);
    assert!(
        mid > 1.0 && mid < 80.0,
        "mid accordion-down must interpolate toward used height, got {mid}"
    );
    match snap.specified.get(crate::style::PropertyId::MaxHeight) {
        Some(crate::style::AnimatableValue::Auto) | None => {}
        other => panic!("specified max-height must stay auto, got {other:?}"),
    }
}

#[test]
#[cfg(feature = "animation-base")]
fn color_transition_writes_text_style() {
    let mut style = crate::style::Style::default();
    style.transition.push(crate::style::TransitionEntry {
        property: crate::style::TransitionProperty::Color,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut text = crate::style::TextStyle {
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    let mut snap = TransitionSnapshot::default();
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let _ = tick_element_transitions_with_text(
        &mut style,
        &mut text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    text.color = Some([255, 255, 255, 255]);
    let _ = tick_element_transitions_with_text(
        &mut style,
        &mut text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    advance_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions_with_text(
        &mut style,
        &mut text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    let c = text.color.expect("color");
    assert!(
        c[0] > 10 && c[0] < 245,
        "color transition must be mid-gray, got {c:?}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn grid_template_rows_transition_lerps_fr() {
    use crate::layout::{first_track_fr, track_fr};
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        grid_template_rows: vec![track_fr(0.0)],
        ..Default::default()
    };
    style.transition.push(crate::style::TransitionEntry {
        property: crate::style::TransitionProperty::GridTemplateRows,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.grid_template_rows = vec![track_fr(1.0)];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    style.grid_template_rows = vec![track_fr(1.0)];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = first_track_fr(&style.grid_template_rows).unwrap();
    assert!(
        (mid - 0.5).abs() < 0.05,
        "expected ~0.5fr at 50ms linear, got {mid}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn grid_template_rows_transition_lerps_equal_length_list() {
    use crate::layout::{track_fr, tracks_fr_list};
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        grid_template_rows: vec![track_fr(0.0), track_fr(0.0)],
        ..Default::default()
    };
    style.transition.push(crate::style::TransitionEntry {
        property: crate::style::TransitionProperty::GridTemplateRows,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    style.grid_template_rows = vec![track_fr(1.0), track_fr(1.0)];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = tracks_fr_list(&style.grid_template_rows).expect("fr list");
    assert_eq!(mid.len(), 2);
    assert!(
        (mid[0] - 0.5).abs() < 0.05 && (mid[1] - 0.5).abs() < 0.05,
        "expected ~0.5 0.5, got {mid:?}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
fn animation_composition_add_stacks_opacity() {
    use crate::style::{
        register_keyframes, AnimationComposition, AnimationFillMode, KeyframeStop, TimingFunction,
    };
    register_keyframes(
        "anim6-hold-03",
        vec![
            KeyframeStop::at(0.0).opacity(0.3),
            KeyframeStop::at(1.0).opacity(0.3),
        ],
    );
    register_keyframes(
        "anim6-hold-02",
        vec![
            KeyframeStop::at(0.0).opacity(0.2),
            KeyframeStop::at(1.0).opacity(0.2),
        ],
    );
    let mut style = crate::style::Style {
        opacity: 0.0,
        ..Default::default()
    };
    let mut a = keyframes_anim_entry(
        "anim6-hold-03",
        1000.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    );
    a.composition = AnimationComposition::Replace;
    let mut b = keyframes_anim_entry(
        "anim6-hold-02",
        1000.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    );
    b.composition = AnimationComposition::Add;
    style.animations.push(a);
    style.animations.push(b);
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!(
        (style.opacity - 0.5).abs() < 0.02,
        "replace 0.3 + add 0.2, got {}",
        style.opacity
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn interpolate_size_allow_keywords_tweens_auto_height() {
    use crate::style::{InterpolateSize, TransitionEntry, TransitionProperty, UsedLayout};
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.size.height = taffy::style::Dimension::length(0.0);
    style.interpolate_size = InterpolateSize::AllowKeywords;
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Height,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let used = UsedLayout {
        content_height: 100.0,
        intrinsic_height: 100.0,
        border_box: [0.0, 0.0, 40.0, 100.0],
        ..Default::default()
    };
    let mut text = crate::style::TextStyle::default();
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions_with_text_used(
        &mut style,
        &mut text,
        Some(&used),
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    style.size.height = taffy::style::Dimension::auto();
    let _ = tick_element_transitions_with_text_used(
        &mut style,
        &mut text,
        Some(&used),
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    set_css_transition_timeline_ms(50.0);
    style.size.height = taffy::style::Dimension::auto();
    let _ = tick_element_transitions_with_text_used(
        &mut style,
        &mut text,
        Some(&used),
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    let h = crate::layout::dim_length_px(style.size.height).unwrap_or(0.0);
    assert!(
        (h - 50.0).abs() < 5.0,
        "interpolate-size should tween auto via used 100px, got {h}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn scroll_timeline_samples_progress_not_elapsed() {
    use crate::style::{AnimationFillMode, AnimationTimeline, TimingFunction};
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    let mut entry = keyframes_anim_entry(
        "fade-in",
        10_000.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    );
    entry.timeline = AnimationTimeline::Scroll;
    style.animations.push(entry);
    let mut snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex_progress(
        &mut style,
        &mut text,
        &used,
        &mut snap,
        1.0,
        TimelineProgress {
            scroll: 0.5,
            view: 0.0,
        },
    );
    assert!(
        (style.opacity - 0.5).abs() < 0.05,
        "scroll() progress 0.5 must sample fade-in mid, got {}",
        style.opacity
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn background_position_and_clip_path_lerp() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.bg_position = [0.0, 0.0];
    style.clip_path = crate::style::ClipPath::Inset {
        trbl: [crate::style::ClipCoord::Px(0.0); 4],
    };
    style.transition.push(crate::style::TransitionEntry {
        property: crate::style::TransitionProperty::BackgroundPosition,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    style.transition.push(crate::style::TransitionEntry {
        property: crate::style::TransitionProperty::ClipPath,
        duration_ms: 100.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.bg_position = [1.0, 1.0];
    style.clip_path = crate::style::ClipPath::Inset {
        trbl: [crate::style::ClipCoord::Px(20.0); 4],
    };
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    style.bg_position = [1.0, 1.0];
    style.clip_path = crate::style::ClipPath::Inset {
        trbl: [crate::style::ClipCoord::Px(20.0); 4],
    };
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.bg_position[0] - 0.5).abs() < 0.05,
        "bg-position mid, got {:?}",
        style.bg_position
    );
    let inset = style.clip_path.inset_trbl_px([100.0, 100.0]).expect("clip");
    assert!(
        (inset[0] - 10.0).abs() < 1.0,
        "clip-path inset mid, got {inset:?}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn named_scroll_timeline_drives_descendant_animation() {
    use crate::core::tree::Element;
    use crate::style::{push_named_timelines, AnimationFillMode, AnimationTimeline};
    use taffy::style::Overflow;

    let mut scroller = Element::new();
    scroller.style.overflow.y = Overflow::Scroll;
    scroller.style.scroll_timeline_name = Some("--page".into());
    scroller.layout_bounds = [0.0, 0.0, 100.0, 100.0];
    scroller.used_layout.content_height = 200.0;
    scroller.used_layout.intrinsic_height = 200.0;
    scroller.used_layout.border_box = [0.0, 0.0, 100.0, 200.0];
    scroller.scroll_offset = [0.0, 50.0];
    let mut named = Vec::new();
    push_named_timelines(&scroller, &mut named, [0.0, 0.0], 100.0);
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].0, "page");
    assert!(
        (named[0].1 - 0.5).abs() < 0.05,
        "scroller 50/100 → 0.5, got {}",
        named[0].1
    );

    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    let mut entry = keyframes_anim_entry(
        "fade-in",
        10_000.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    );
    entry.timeline = AnimationTimeline::Named("page".into());
    entry.range_start = Some("0%".into());
    entry.range_end = Some("100%".into());
    style.animations.push(entry);
    let mut snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex_named(
        &mut style,
        &mut text,
        &used,
        &mut snap,
        1.0,
        TimelineProgress::ZERO,
        &named,
    );
    assert!(
        (style.opacity - 0.5).abs() < 0.05,
        "named --page at 0.5 must sample fade-in mid, got {}",
        style.opacity
    );
}

#[test]
fn calc_size_keyframes_animate_to_used_plus_n() {
    use crate::layout::dimension::parse_css_dim;
    use crate::style::{
        register_keyframes, AnimatableValue, AnimationFillMode, KeyframeStop, PropertyId,
        UsedLayout,
    };
    register_keyframes(
        "calc-size-grow",
        vec![
            KeyframeStop::at(0.0).set(PropertyId::Height, AnimatableValue::Scalar(0.0)),
            KeyframeStop::at(1.0).set(
                PropertyId::Height,
                AnimatableValue::Dim(parse_css_dim("calc-size(auto, size + 20px)").expect("parse")),
            ),
        ],
    );
    let mut style = crate::style::Style::default();
    style.size.height = taffy::style::Dimension::length(0.0);
    style.animations.push(keyframes_anim_entry(
        "calc-size-grow",
        1.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let used = UsedLayout {
        content_height: 100.0,
        intrinsic_height: 100.0,
        border_box: [0.0, 0.0, 40.0, 100.0],
        ..Default::default()
    };
    let mut text = crate::style::TextStyle::default();
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 1.0);
    let h = crate::layout::dim_length_px(style.size.height).unwrap_or(-1.0);
    assert!(
        (h - 120.0).abs() < 1.0,
        "calc-size(auto, size+20px) of used 100 → 120, got {h}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn clip_path_polygon_keyframes_lerp_vertices() {
    use crate::style::{
        register_keyframes, AnimatableValue, AnimationFillMode, ClipCoord, ClipPath, KeyframeStop,
        PropertyId,
    };
    let from =
        crate::style::parse_clip_path("polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)").unwrap();
    let to = crate::style::parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap();
    register_keyframes(
        "clip-poly-lerp",
        vec![
            KeyframeStop::at(0.0).set(PropertyId::ClipPath, AnimatableValue::Clip(from)),
            KeyframeStop::at(1.0).set(PropertyId::ClipPath, AnimatableValue::Clip(to)),
        ],
    );
    let mut style = crate::style::Style::default();
    style.animations.push(keyframes_anim_entry(
        "clip-poly-lerp",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.05);
    match &style.clip_path {
        ClipPath::Polygon { points, .. } => match points[0][0] {
            ClipCoord::Percent(p) => {
                assert!((p - 0.125).abs() < 0.02, "polygon mid vertex, got {p}")
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}
