use super::*;

#[test]
fn spin_samples_rotate() {
    let stops = lookup_keyframes("spin").expect("spin");
    let mid = sample_keyframes(&stops, 0.5);
    assert!((mid.rotate_deg.unwrap() - 180.0).abs() < 0.01);
}

#[test]
fn pulse_samples_opacity() {
    let stops = lookup_keyframes("pulse").expect("pulse");
    let mid = sample_keyframes(&stops, 0.5);
    assert!((mid.opacity.unwrap() - 0.5).abs() < 0.01);
}

#[test]
fn bounce_peak_is_percent_not_pixels() {
    // Tailwind `animate-bounce` is translateY(-25%) of the box. A 2rem gallery
    // demo with -25px jumped over the caption (dest-reset then punched it).
    let stops = lookup_keyframes("bounce").expect("bounce");
    let peak = sample_keyframes(&stops, 0.5);
    assert_eq!(peak.translate, None, "bounce must not use px translate");
    let pct = match peak.props.get(&PropertyId::TranslatePct) {
        Some(AnimatableValue::Vec2(xy)) => *xy,
        other => panic!("bounce peak must be translate_pct, got {other:?}"),
    };
    assert!(
        (pct[1] + 0.25).abs() < 0.001,
        "bounce peak must be -25% of self, got {pct:?}"
    );
}

fn base() -> KeyframeBase {
    KeyframeBase {
        max_height: 48.0,
        ..Default::default()
    }
}

#[test]
fn easing_applies_per_segment_not_to_global_progress() {
    // Two equal segments, `ease-in` on each. At the segment boundary (t=0.5) the
    // value must be exactly the authored stop — easing global t would overshoot it.
    let stops = vec![
        KeyframeStop::at(0.0).opacity(0.0),
        KeyframeStop::at(0.5).opacity(1.0),
        KeyframeStop::at(1.0).opacity(0.0),
    ];
    let at_stop = sample_keyframes_eased(&stops, 0.5, TimingFunction::EaseIn, None);
    assert!(
        (at_stop.opacity.unwrap() - 1.0).abs() < 1e-3,
        "authored stop must be hit exactly, got {:?}",
        at_stop.opacity
    );
    // Inside the first segment, ease-in lags linear.
    let quarter = sample_keyframes_eased(&stops, 0.25, TimingFunction::EaseIn, None);
    assert!(
        quarter.opacity.unwrap() < 0.5,
        "ease-in should lag linear inside the segment, got {:?}",
        quarter.opacity
    );
}

#[test]
fn per_keyframe_timing_overrides_animation_timing() {
    let stops = vec![
        KeyframeStop::at(0.0)
            .opacity(0.0)
            .timing(TimingFunction::Linear),
        KeyframeStop::at(1.0).opacity(1.0),
    ];
    let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::EaseIn, None);
    assert!(
        (mid.opacity.unwrap() - 0.5).abs() < 1e-3,
        "stop-level linear must win over entry-level ease-in, got {:?}",
        mid.opacity
    );
}

#[test]
fn missing_endpoint_falls_back_to_base_value() {
    // `from { opacity: 0 }` with no `to` → animates 0 → base (1.0), like CSS.
    let stops = vec![KeyframeStop::at(0.0).opacity(0.0)];
    let b = base();
    let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::Linear, Some(&b));
    assert!(
        (mid.opacity.unwrap() - 0.5).abs() < 1e-3,
        "expected interpolation toward base 1.0, got {:?}",
        mid.opacity
    );
    let end = sample_keyframes_eased(&stops, 1.0, TimingFunction::Linear, Some(&b));
    assert!((end.opacity.unwrap() - 1.0).abs() < 1e-3);
}

#[test]
fn without_base_single_ended_property_holds() {
    let stops = vec![KeyframeStop::at(0.0).opacity(0.0)];
    let mid = sample_keyframes_eased(&stops, 0.5, TimingFunction::Linear, None);
    assert!((mid.opacity.unwrap() - 0.0).abs() < 1e-3);
}

#[test]
fn undeclared_property_is_left_alone_without_base() {
    let stops = lookup_keyframes("fade-in").expect("fade-in");
    let mid = sample_keyframes(&stops, 0.5);
    assert!(mid.scale.is_none(), "fade-in must not touch scale");
    assert!(mid.translate.is_none());
}

#[test]
fn coverage_reports_declared_properties_only() {
    let fade = keyframe_coverage(&lookup_keyframes("fade-in").expect("fade-in"));
    assert!(fade.opacity && !fade.transform);
    let spin = keyframe_coverage(&lookup_keyframes("spin").expect("spin"));
    assert!(!spin.opacity && spin.transform);
}

#[test]
fn accordion_height_keyframes_interpolate_toward_base() {
    let down = lookup_keyframes("accordion-down").expect("accordion-down");
    let b = base();
    let mid = sample_keyframes_eased(&down, 0.5, TimingFunction::Linear, Some(&b));
    assert!((mid.max_height.unwrap() - 24.0).abs() < 1e-3);
    let up = lookup_keyframes("accordion-up").expect("accordion-up");
    let mid_up = sample_keyframes_eased(&up, 0.5, TimingFunction::Linear, Some(&b));
    assert!((mid_up.max_height.unwrap() - 24.0).abs() < 1e-3);
}

#[test]
fn collapsible_height_keyframes_match_accordion_shape() {
    let down = lookup_keyframes("collapsible-down").expect("collapsible-down");
    let b = base();
    let mid = sample_keyframes_eased(&down, 0.5, TimingFunction::Linear, Some(&b));
    assert!((mid.max_height.unwrap() - 24.0).abs() < 1e-3);
    let up = lookup_keyframes("collapsible-up").expect("collapsible-up");
    let mid_up = sample_keyframes_eased(&up, 0.5, TimingFunction::Linear, Some(&b));
    assert!((mid_up.max_height.unwrap() - 24.0).abs() < 1e-3);
}

#[test]
fn shadcn_slide_names_are_registered() {
    for name in [
        "fade-in-0",
        "fade-out-0",
        "accordion-down",
        "accordion-up",
        "collapsible-down",
        "collapsible-up",
        "slide-in-from-top-2",
        "slide-out-to-right-2",
        "zoom-in-90",
    ] {
        assert!(keyframes_exist(name), "{name} should be a builtin");
    }
    let slide = lookup_keyframes("slide-in-from-bottom-2").expect("slide");
    assert_eq!(
        slide[0].get(PropertyId::Translate),
        Some(&AnimatableValue::Vec2([0.0, SLIDE_2_PX]))
    );
    assert_eq!(
        slide[1].get(PropertyId::Translate),
        Some(&AnimatableValue::Vec2([0.0, 0.0]))
    );
}
