use jadix::style::*;

#[test]
fn layout_projection_obeys_the_enhanced_feature() {
    use jadix::core::tree::{Element, LayoutTree};
    use jadix::motion::{LayoutAnimation, LayoutMotion};
    set_prefers_reduced_motion(Some(false));
    let motion = LayoutMotion::new(LayoutAnimation::Both);
    let mut root = Element::new()
        .w(400.0)
        .h(200.0)
        .child(Element::new().w(100.0).h(60.0).layout_motion(&motion));
    let mut tree = LayoutTree::new();
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    root.children[0].style.size.width = taffy::style::Dimension::length(200.0);
    tree.build_and_layout(&mut root, 400.0, 200.0, [-1.0, -1.0]);
    let bounds = motion.bounds().unwrap();
    if cfg!(feature = "animation-ex") {
        assert!(motion.is_active());
        assert!((bounds[2] - 100.0).abs() < 0.01);
    } else {
        assert!(!motion.is_active());
        assert!((bounds[2] - 200.0).abs() < 0.01);
    }
    set_prefers_reduced_motion(None);
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_generators_finish_without_a_frame() {
    let spring = Spring::new(0.0, 10.0, 4.0, SpringOptions::default());
    let sample = spring.next(0.0);
    assert_eq!(sample.value, 10.0);
    assert_eq!(sample.velocity, 0.0);
    assert!(sample.done);
    assert_eq!(spring.calculated_duration_ms(), 0.0);

    let tween = AnimatedNumber::tween(0.0, 10.0, 1000.0, TimingFunction::Linear);
    assert_eq!(tween.peek(), 10.0);
    assert!(tween.finished().get());
    let sample = Inertia::new(
        0.0,
        InertiaOptions {
            velocity: 100.0,
            min: Some(0.0),
            max: Some(10.0),
            ..Default::default()
        },
    )
    .next(0.0);
    assert_eq!(sample.value, 10.0);
    assert!(sample.done);
    let inertia = AnimatedNumber::inertia(3.0, 100.0);
    assert_eq!(inertia.peek(), 3.0);
    assert!(inertia.finished().get());
    assert!(!inertia.is_active());

    let value = animate(0.0, 10.0).delay(500.0).repeat(RepeatMode::Loop);
    assert_eq!(value.peek(), 10.0);
    assert!(value.finished().get());
    assert!(!value.is_active());
    value.set(20.0);
    assert_eq!(value.peek(), 20.0);
    value.restart(30.0, ValueDriver::spring());
    assert_eq!(value.peek(), 30.0);
    let value = value.then_to(40.0, ValueDriver::spring());
    assert_eq!(value.peek(), 40.0);
    value.play();
    assert!(value.finished().get());
    assert!(!value.is_active());

    let cubic = CubicTween::new_ui_transition(0.0);
    cubic.set(1.0);
    assert_eq!(cubic.peek(), 1.0);
    assert!(!cubic.is_active());
    assert!(!tick_value_generators(0.016));
    assert!(!tick_animations(0.016).any);
    assert!(!any_ui_animation_active());
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_follower_keeps_signal_updates_without_animation_ticks() {
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let source = jadix::core::reactivity::Signal::new(1.0);
        let follower = follow(0.0).follow_signal(source);
        assert_eq!(follower.get(), 1.0);
        source.set(5.0);
        assert_eq!(follower.get(), 5.0);
        assert!(!follower.is_active());
        assert!(follower.finished().get());
        assert!(!any_value_generator_active());
    });
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_css_transitions_keep_authored_targets() {
    set_prefers_reduced_motion(Some(false));
    assert!(prefers_reduced_motion());
    let mut style = Style::default();
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 1000.0,
        delay_ms: 500.0,
        timing: TimingFunction::Linear,
    });
    let mut snapshot = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snapshot, 0.0));
    style.opacity = 0.25;
    assert!(!tick_element_transitions(&mut style, &mut snapshot, 0.0));
    assert_eq!(style.opacity, 0.25);
    assert!(!css_transition_needs_frame(&style, &snapshot));
    set_prefers_reduced_motion(None);
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_keyframes_finish_even_when_paused_delayed_or_infinite() {
    register_keyframes(
        "feature-static-target",
        vec![
            KeyframeStop::at(0.0).set(PropertyId::Opacity, AnimatableValue::Scalar(0.0)),
            KeyframeStop::at(1.0).set(PropertyId::Opacity, AnimatableValue::Scalar(1.0)),
        ],
    );
    for iterations in [
        AnimationIterationCount::Finite(1.0),
        AnimationIterationCount::Infinite,
    ] {
        let mut style = Style::default();
        style.opacity = 0.0;
        let mut entry = keyframes_anim_entry(
            "feature-static-target",
            1000.0,
            TimingFunction::Linear,
            AnimationFillMode::Forwards,
        );
        entry.delay_ms = 500.0;
        entry.play_state = AnimationPlayState::Paused;
        entry.iteration_count = iterations;
        style.animations.push(entry);
        let mut snapshot = AnimationSnapshot::default();
        assert!(!tick_element_animations(&mut style, &mut snapshot, 0.0));
        assert_eq!(style.opacity, 1.0);
        assert!(!animation_run_active(&style, &snapshot));
        assert!(!css_animation_needs_frame(&style, &snapshot));
        assert!(!tick_element_animations(&mut style, &mut snapshot, 0.016));
        assert_eq!(style.opacity, 1.0);
    }
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_timelines_finish_but_explicit_seeking_still_works() {
    let timeline = jadix::style::timeline::Timeline::new(1.0).repeat(Repeat::Loop);
    assert_eq!(timeline.linear(), 1.0);
    assert!(!any_timeline_playing());
    timeline.seek(0.25);
    assert_eq!(timeline.linear(), 0.25);
    timeline.play();
    assert_eq!(timeline.linear(), 1.0);
    assert!(!tick_timelines(0.016));
    assert!(!any_timeline_playing());
}

#[cfg(not(feature = "animation-base"))]
#[test]
fn disabled_exit_presence_does_not_wait_for_an_animation() {
    use jadix::utils::presence::{exit_elapsed_at, ExitGate, PresencePhase};
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut gate = ExitGate::new(true, Duration::from_secs(1));
    assert!(gate.set_present(false, now));
    assert_eq!(gate.phase(), PresencePhase::Absent);
    assert!(!gate.tick(now));
    assert!(!gate.is_mounted());
    assert!(exit_elapsed_at(now, Duration::from_secs(1), now));
    jadix::core::reactivity::create_root(|| {
        use jadix::utils::presence::{sync_generic_presence, use_generic_presence};
        let handle = use_generic_presence("static-presence", 1000);
        assert_eq!(sync_generic_presence(&handle, true), PresencePhase::Present);
        assert_eq!(sync_generic_presence(&handle, false), PresencePhase::Absent);
        assert!(!handle.mounted().get());
    });
}

#[cfg(feature = "animation-base")]
#[test]
fn base_spring_and_css_work_without_the_enhanced_layer() {
    let spring = Spring::new(0.0, 1.0, 0.0, SpringOptions::default());
    let sample = spring.next(16.0);
    assert!(sample.value > 0.0 && sample.value < 1.0);
    assert!(!sample.done);
    let value = animate(0.0, 1.0);
    assert_eq!(value.peek(), 0.0);
    assert!(value.is_active());
    assert!(tick_value_generators(0.016));
    assert!(value.peek() > 0.0);

    set_prefers_reduced_motion(Some(false));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = Style {
        opacity: 0.0,
        ..Default::default()
    };
    style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 1000.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let mut snapshot = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snapshot, 0.0));
    style.opacity = 1.0;
    assert!(tick_element_transitions(&mut style, &mut snapshot, 0.0));
    set_css_transition_timeline_ms(500.0);
    assert!(tick_element_transitions(&mut style, &mut snapshot, 0.0));
    assert!((style.opacity - 0.5).abs() < 0.01);
    assert!(css_transition_needs_frame(&style, &snapshot));
    set_css_transition_virtual_timeline(false);
    set_prefers_reduced_motion(None);
}

#[cfg(feature = "animation-path")]
#[test]
fn animation_path_morph_is_renderable() {
    use jadix::utils::morph_icon::{
        ensure_morph_icon_texture, morph_icon, morph_progress, morph_to, tick_morph_icons_subtree,
    };
    let mut icon = morph_icon("M4 12L20 12").into();
    morph_to(&mut icon, "M12 4L12 20");
    assert!(morph_progress(&icon) < 1.0);
    assert!(tick_morph_icons_subtree(&mut icon, 0.016));
    assert!(ensure_morph_icon_texture(&icon, 24.0, [0, 0, 0, 255], 1.0).is_some());
}

#[cfg(feature = "animation-ex")]
#[test]
fn enhanced_motion_implies_base_and_exposes_the_existing_api() {
    use jadix::prelude::*;
    const {
        assert!(cfg!(feature = "animation-base"));
    }
    let (_owner, ()) = jadix::core::reactivity::create_root(|| {
        let state = use_motion("feature-enhanced-motion");
        let element = Element::new().motion(&state, Motion::new(MotionTarget::default()));
        assert!(element.children.is_empty());
    });
}

#[cfg(feature = "animation-path")]
#[test]
fn animation_path_exposes_the_shared_path_kernel() {
    use jadix::motion::PathMorph;
    use jadix::primitives::path::PathCommand;
    let morph = PathMorph::new(
        &[PathCommand::MoveTo((0.0, 0.0))],
        &[PathCommand::MoveTo((10.0, 20.0))],
    )
    .unwrap();
    assert_eq!(morph.sample(0.5), vec![PathCommand::MoveTo((5.0, 10.0))]);
}
