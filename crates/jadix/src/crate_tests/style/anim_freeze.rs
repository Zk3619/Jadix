//! CSS animation / transition / keyframes freeze contract.
//!
//! Anchors [`docs/engine/animation_and_motion.md`] (full freeze): dual-engine interpolator,
//! Specified/Used, fill-mode, reduced-motion, `PropertyId` 64-slot bitmap, presence
//! `animation_run_active` vs frame pump.

#[cfg(feature = "animation-base")]
use crate::layout::dim_length_px;
#[cfg(feature = "animation-base")]
use crate::layout::dimension::CssDim;
use crate::style::{
    animation_run_active, css_animation_needs_frame, keyframes_anim_entry, register_keyframes,
    set_prefers_reduced_motion, tick_element_animations, AnimationFillMode, AnimationSnapshot,
    KeyframeStop, PropertyId, TimingFunction, TransitionProperty,
};
#[cfg(feature = "animation-base")]
use crate::style::{
    parse_filter, set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
    tick_element_animations_ex, tick_element_transitions, tick_element_transitions_with_text,
    AnimatableValue, AnimationEntry, AnimationLock, ClipCoord, ClipPath, FilterFunction,
    FilterList, TransitionEntry, TransitionSnapshot, UsedLayout,
};

#[cfg(feature = "animation-base")]
fn linear_transition(prop: TransitionProperty, duration_ms: f32) -> TransitionEntry {
    TransitionEntry {
        property: prop,
        duration_ms,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    }
}

#[test]
fn from_css_recognizes_independent_scale_rotate() {
    assert_eq!(
        TransitionProperty::from_css("scale"),
        TransitionProperty::Scale
    );
    assert_eq!(
        TransitionProperty::from_css("rotate"),
        TransitionProperty::Rotate
    );
    assert_eq!(
        TransitionProperty::from_css("clip-path"),
        TransitionProperty::ClipPath
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn independent_scale_transition_lerps_without_transform_shorthand() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        scale: [1.0, 1.0],
        ..Default::default()
    };
    style
        .transition
        .push(linear_transition(TransitionProperty::Scale, 100.0));
    let mut snap = TransitionSnapshot::default();
    assert!(!tick_element_transitions(&mut style, &mut snap, 0.0));
    style.scale = [2.0, 1.0];
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    style.scale = [2.0, 1.0];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.scale[0] - 1.5).abs() < 0.02,
        "independent scale mid, got {}",
        style.scale[0]
    );
    assert!((style.scale[1] - 1.0).abs() < 0.02);
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn independent_rotate_transition_lerps() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        rotate_deg: 0.0,
        ..Default::default()
    };
    style
        .transition
        .push(linear_transition(TransitionProperty::Rotate, 100.0));
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.rotate_deg = 90.0;
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    style.rotate_deg = 90.0;
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.rotate_deg - 45.0).abs() < 1.0,
        "independent rotate mid, got {}",
        style.rotate_deg
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn clip_path_polygon_transition_lerps_vertices() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let from =
        crate::style::parse_clip_path("polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)").unwrap();
    let to = crate::style::parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap();
    let mut style = crate::style::Style {
        clip_path: from,
        ..Default::default()
    };
    style
        .transition
        .push(linear_transition(TransitionProperty::ClipPath, 100.0));
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.clip_path = to;
    assert!(tick_element_transitions(&mut style, &mut snap, 0.0));
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    match &style.clip_path {
        ClipPath::Polygon { points, .. } => match points[0][0] {
            ClipCoord::Percent(p) => {
                assert!(
                    (p - 0.125).abs() < 0.02,
                    "polygon transition mid vertex, got {p}"
                )
            }
            other => panic!("{other:?}"),
        },
        other => panic!("expected polygon, got {other:?}"),
    }
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn clip_path_inset_transition_uses_same_interpolator() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    style.clip_path = crate::style::parse_clip_path("inset(0px)").unwrap();
    style
        .transition
        .push(linear_transition(TransitionProperty::All, 100.0));
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.clip_path = crate::style::parse_clip_path("inset(40px)").unwrap();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    match &style.clip_path {
        ClipPath::Inset { trbl } => match trbl[0] {
            ClipCoord::Px(v) => assert!((v - 20.0).abs() < 0.5, "inset mid, got {v}"),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn negative_animation_delay_seeks_into_cycle() {
    let mut style = crate::style::Style::default();
    style.animations.push(AnimationEntry {
        name: "spin".into(),
        duration_ms: 1000.0,
        delay_ms: -500.0,
        timing: TimingFunction::Linear,
        ..Default::default()
    });
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    assert!(
        (style.rotate_deg - 180.0).abs() < 2.0,
        "delay=-500ms of 1000ms spin must sample ~180deg, got {}",
        style.rotate_deg
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn dual_engine_background_color_mid() {
    register_keyframes(
        "anim-freeze-bg",
        vec![
            KeyframeStop::at(0.0).background_color([0.0, 0.0, 0.0, 1.0]),
            KeyframeStop::at(1.0).background_color([1.0, 1.0, 1.0, 1.0]),
        ],
    );
    let mut kf = crate::style::Style::default();
    kf.animations.push(keyframes_anim_entry(
        "anim-freeze-bg",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut kf_snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.0);
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.05);

    let mut tr = crate::style::Style {
        bg_color: [0.0, 0.0, 0.0, 1.0],
        ..Default::default()
    };
    tr.transition.push(linear_transition(
        TransitionProperty::BackgroundColor,
        100.0,
    ));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    tr.bg_color = [1.0, 1.0, 1.0, 1.0];
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_virtual_timeline(false);

    assert!(
        (kf.bg_color[0] - tr.bg_color[0]).abs() < 0.02,
        "dual-engine bg must share interpolator, kf={:?} tr={:?}",
        kf.bg_color,
        tr.bg_color
    );
    assert!(
        kf.bg_color[0] > 0.2 && kf.bg_color[0] < 0.9,
        "bg mid expected between endpoints, got {:?}",
        kf.bg_color
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn dual_engine_width_mid() {
    register_keyframes(
        "anim-freeze-width",
        vec![
            KeyframeStop::at(0.0).set(PropertyId::Width, AnimatableValue::Scalar(0.0)),
            KeyframeStop::at(1.0).set(PropertyId::Width, AnimatableValue::Scalar(100.0)),
        ],
    );
    let mut kf = crate::style::Style::default();
    kf.size.width = taffy::style::Dimension::length(0.0);
    kf.animations.push(keyframes_anim_entry(
        "anim-freeze-width",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut kf_snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.0);
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.05);
    let kf_w = dim_length_px(kf.size.width).unwrap_or(-1.0);

    let mut tr = crate::style::Style::default();
    tr.size.width = taffy::style::Dimension::length(0.0);
    tr.transition
        .push(linear_transition(TransitionProperty::Width, 100.0));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    tr.size.width = taffy::style::Dimension::length(100.0);
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_virtual_timeline(false);
    let tr_w = dim_length_px(tr.size.width).unwrap_or(-1.0);

    assert!((kf_w - 50.0).abs() < 2.0, "keyframes width mid, got {kf_w}");
    assert!(
        (tr_w - 50.0).abs() < 2.0,
        "transition width mid, got {tr_w}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn dual_engine_border_radius_mid() {
    register_keyframes(
        "anim-freeze-radius",
        vec![
            KeyframeStop::at(0.0).set(
                PropertyId::BorderRadius,
                AnimatableValue::Vec4([0.0, 0.0, 0.0, 0.0]),
            ),
            KeyframeStop::at(1.0).set(
                PropertyId::BorderRadius,
                AnimatableValue::Vec4([16.0, 16.0, 16.0, 16.0]),
            ),
        ],
    );
    let mut kf = crate::style::Style::default();
    kf.animations.push(keyframes_anim_entry(
        "anim-freeze-radius",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut kf_snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.0);
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.05);

    let mut tr = crate::style::Style {
        radii: [0.0; 4],
        ..Default::default()
    };
    tr.transition
        .push(linear_transition(TransitionProperty::BorderRadius, 100.0));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    tr.radii = [16.0; 4];
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_virtual_timeline(false);

    assert!(
        (kf.radii[0] - 8.0).abs() < 0.5,
        "keyframes radius mid, got {}",
        kf.radii[0]
    );
    assert!(
        (tr.radii[0] - 8.0).abs() < 0.5,
        "transition radius mid, got {}",
        tr.radii[0]
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn dual_engine_filter_blur_mid() {
    let from = parse_filter("blur(0px)");
    let to = parse_filter("blur(10px)");
    register_keyframes(
        "anim-freeze-filter",
        vec![
            KeyframeStop::at(0.0).filter(from.clone()),
            KeyframeStop::at(1.0).filter(to.clone()),
        ],
    );
    let mut kf = crate::style::Style {
        filter: from.clone(),
        ..Default::default()
    };
    kf.animations.push(keyframes_anim_entry(
        "anim-freeze-filter",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut kf_snap = AnimationSnapshot::default();
    let mut text = crate::style::TextStyle::default();
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.0);
    let _ = tick_element_animations_ex(&mut kf, &mut text, &used, &mut kf_snap, 0.05);

    let mut tr = crate::style::Style {
        filter: from,
        ..Default::default()
    };
    tr.transition
        .push(linear_transition(TransitionProperty::Filter, 100.0));
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    tr.filter = to;
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut tr, &mut snap, 0.0);
    set_css_transition_virtual_timeline(false);

    fn blur_px(list: &Option<FilterList>) -> f32 {
        match list.as_ref().and_then(|l| l.first()) {
            Some(FilterFunction::Blur(Some(CssDim::Px(v)))) => *v,
            other => panic!("expected blur px, got {other:?}"),
        }
    }
    let kf_b = blur_px(&kf.filter);
    let tr_b = blur_px(&tr.filter);
    assert!((kf_b - 5.0).abs() < 0.2, "keyframes blur mid, got {kf_b}");
    assert!((tr_b - 5.0).abs() < 0.2, "transition blur mid, got {tr_b}");
}

#[test]
#[cfg(feature = "animation-base")]
fn dual_engine_text_color_mid() {
    register_keyframes(
        "anim-freeze-color",
        vec![
            KeyframeStop::at(0.0).color([0.0, 0.0, 0.0, 1.0]),
            KeyframeStop::at(1.0).color([1.0, 1.0, 1.0, 1.0]),
        ],
    );
    let mut kf = crate::style::Style::default();
    kf.animations.push(keyframes_anim_entry(
        "anim-freeze-color",
        100.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut kf_snap = AnimationSnapshot::default();
    let mut kf_text = crate::style::TextStyle {
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    let used = crate::style::UsedLayout::default();
    let _ = tick_element_animations_ex(&mut kf, &mut kf_text, &used, &mut kf_snap, 0.0);
    let _ = tick_element_animations_ex(&mut kf, &mut kf_text, &used, &mut kf_snap, 0.05);

    let mut tr = crate::style::Style::default();
    tr.transition
        .push(linear_transition(TransitionProperty::Color, 100.0));
    let mut tr_text = crate::style::TextStyle {
        color: Some([0, 0, 0, 255]),
        ..Default::default()
    };
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions_with_text(
        &mut tr,
        &mut tr_text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    tr_text.color = Some([255, 255, 255, 255]);
    let _ = tick_element_transitions_with_text(
        &mut tr,
        &mut tr_text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions_with_text(
        &mut tr,
        &mut tr_text,
        &mut snap,
        AnimationLock::default(),
        0.0,
    );
    set_css_transition_virtual_timeline(false);

    let kc = kf_text.color.expect("kf color");
    let tc = tr_text.color.expect("tr color");
    assert!(kc[0] > 10 && kc[0] < 245, "keyframes color mid, got {kc:?}");
    assert!(
        tc[0] > 10 && tc[0] < 245,
        "transition color mid, got {tc:?}"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn transform_origin_transition_lerps_fractions() {
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style::default();
    crate::style::set_origin_frac(&mut style.transform_origin, [0.0, 0.0]);
    style.transition.push(linear_transition(
        TransitionProperty::TransformOrigin,
        100.0,
    ));
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    crate::style::set_origin_frac(&mut style.transform_origin, [1.0, 1.0]);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    let mid = crate::style::origin_frac(&style.transform_origin);
    assert!(
        (mid[0] - 0.5).abs() < 0.02 && (mid[1] - 0.5).abs() < 0.02,
        "origin mid {mid:?}"
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn extra_shadows_list_lerps_equal_length() {
    use crate::style::BoxShadow;
    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);
    let mut style = crate::style::Style {
        shadow_offset: [0.0, 0.0],
        ..Default::default()
    };
    style.extra_shadows = vec![BoxShadow {
        offset: [0.0, 0.0],
        ..Default::default()
    }];
    style
        .transition
        .push(linear_transition(TransitionProperty::BoxShadow, 100.0));
    let mut snap = TransitionSnapshot::default();
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    style.shadow_offset = [10.0, 0.0];
    style.extra_shadows = vec![BoxShadow {
        offset: [20.0, 0.0],
        ..Default::default()
    }];
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    set_css_transition_timeline_ms(50.0);
    let _ = tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        (style.shadow_offset[0] - 5.0).abs() < 0.2,
        "primary shadow mid {}",
        style.shadow_offset[0]
    );
    assert!(
        (style.extra_shadows[0].offset[0] - 10.0).abs() < 0.2,
        "extra shadow mid {}",
        style.extra_shadows[0].offset[0]
    );
    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "animation-base")]
fn waapi_keyframe_objects_share_css_tick() {
    use crate::style::{Animation, Keyframe, KeyframeEffect};
    let anim = Animation::animate(
        vec![Keyframe::new().opacity(0.0), Keyframe::new().opacity(1.0)],
        KeyframeEffect::new().duration(100.0),
    );
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(anim.to_entry());
    let mut snap = crate::style::AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.05);
    assert!(
        (style.opacity - 0.5).abs() < 0.08,
        "programmatic frames must ride CSS tick, got {}",
        style.opacity
    );
    assert!(crate::style::get_animations()
        .iter()
        .any(|a| a.id() == anim.id()));
    let t = anim.effect().model();
    assert!((t.duration_ms - 100.0).abs() < 1e-3);
    assert!((t.active_duration_ms() - 100.0).abs() < 1e-3);
}

#[test]
fn spring_timing_parses_and_retargets() {
    assert!(TimingFunction::from_css("spring").is_spring());
    assert!(TimingFunction::from_css("spring()").is_spring());
    let TimingFunction::Spring {
        stiffness,
        damping,
        mass,
    } = TimingFunction::from_css("spring(200, 20, 2)")
    else {
        panic!("expected Spring");
    };
    assert!((stiffness - 200.0).abs() < 1e-3);
    assert!((damping - 20.0).abs() < 1e-3);
    assert!((mass - 2.0).abs() < 1e-3);

    let el = crate::core::tree::Element::new().transition("transform spring()");
    assert!(
        el.style.transition[0].timing.is_spring(),
        "shorthand must keep spring()"
    );
}

#[test]
fn property_id_table_is_full_u8_bitmap() {
    assert_eq!(PropertyId::TransformOrigin as u8, 60);
    assert_eq!(PropertyId::OffsetDistance as u8, 61);
    assert_eq!(PropertyId::OffsetRotate as u8, 62);
    assert_eq!(PropertyId::TranslatePct as u8, 63);
    assert_eq!(
        PropertyId::ALL.len(),
        64,
        "repr(u8) bitmap is full; new PropertyId requires expanding the freeze"
    );
    assert_eq!(PropertyId::ALL[0] as u8, 0);
    assert_eq!(PropertyId::ALL[63] as u8, 63);
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
        AnimationFillMode::Forwards,
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
fn fill_mode_none_restores_specified_opacity() {
    register_keyframes(
        "anim-freeze-fade-out",
        vec![
            KeyframeStop::at(0.0).opacity(1.0),
            KeyframeStop::at(1.0).opacity(0.0),
        ],
    );
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(keyframes_anim_entry(
        "anim-freeze-fade-out",
        80.0,
        TimingFunction::Linear,
        AnimationFillMode::None,
    ));
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations(&mut style, &mut snap, 0.0);
    let _ = tick_element_animations(&mut style, &mut snap, 0.04);
    assert!(
        style.opacity > 0.05 && style.opacity < 0.95,
        "mid fade-out must leave specified 1.0, got {}",
        style.opacity
    );
    for _ in 0..20 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    }
    assert!(
        !animation_run_active(&style, &snap),
        "finite fill:none run must finish"
    );
    assert!(
        (style.opacity - 1.0).abs() < 0.02,
        "fill-mode:none must restore specified opacity, got {}",
        style.opacity
    );
}

#[test]
fn fill_mode_forwards_holds_end_opacity() {
    register_keyframes(
        "anim-freeze-fade-out-fwd",
        vec![
            KeyframeStop::at(0.0).opacity(1.0),
            KeyframeStop::at(1.0).opacity(0.0),
        ],
    );
    let mut style = crate::style::Style {
        opacity: 1.0,
        ..Default::default()
    };
    style.animations.push(keyframes_anim_entry(
        "anim-freeze-fade-out-fwd",
        80.0,
        TimingFunction::Linear,
        AnimationFillMode::Forwards,
    ));
    let mut snap = AnimationSnapshot::default();
    for _ in 0..20 {
        let _ = tick_element_animations(&mut style, &mut snap, 0.016);
    }
    assert!(!animation_run_active(&style, &snap));
    assert!(
        !css_animation_needs_frame(&style, &snap),
        "forwards retains the end value without keeping the frame pump alive"
    );
    assert!(
        style.opacity < 0.05,
        "fill-mode:forwards must hold the last sample, got {}",
        style.opacity
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn specified_max_height_stays_auto_during_accordion_down() {
    let mut style = crate::style::Style::default();
    style.max_size.height = taffy::style::Dimension::auto();
    style.animations.push(keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::Linear,
        AnimationFillMode::Both,
    ));
    let mut text = crate::style::TextStyle::default();
    let mut used = UsedLayout::default();
    used.stamp_border_box([0.0, 0.0, 100.0, 80.0], false);
    let mut snap = AnimationSnapshot::default();
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.0);
    let h0 = dim_length_px(style.max_size.height).unwrap_or(999.0);
    assert!(h0 < 2.0, "fill-mode both at t=0 must start at 0, got {h0}");
    let _ = tick_element_animations_ex(&mut style, &mut text, &used, &mut snap, 0.1);
    let mid = dim_length_px(style.max_size.height).unwrap_or(0.0);
    assert!(
        mid > 1.0 && mid < 80.0,
        "mid accordion-down interpolates toward used height, got {mid}"
    );
    match snap.specified.get(PropertyId::MaxHeight) {
        Some(AnimatableValue::Auto) | None => {}
        other => panic!("specified max-height must stay auto, got {other:?}"),
    }
}
