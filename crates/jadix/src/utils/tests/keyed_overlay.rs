use super::*;
use crate::style::{TransitionEntry, TransitionProperty, TransitionSnapshot};

#[test]
fn item_template_id_format() {
    assert_eq!(item_template_id("toast-card", 42), "toast-card-42");
    assert_eq!(
        item_template_id(&format!("{KEYED_OVERLAY_PREFIX}notify"), "x"),
        "ko:notify-x"
    );
}

#[test]
fn is_keyed_motion_item_scopes() {
    assert!(is_keyed_motion_item(Some("toast-card-1")));
    assert!(is_keyed_motion_item(Some("ko:tooltip-abc")));
    assert!(!is_keyed_motion_item(Some("random")));
}

#[test]
fn adopt_keyed_overlay_preserves_mid_on_same_template_id() {
    let mut prev = Element::new().template_id("toast-card-1");
    prev.style.translate = [10.0, 20.0];
    prev.style.opacity = 0.5;
    prev.transition_snapshot = TransitionSnapshot::default();

    let mut fresh = Element::new().template_id("toast-card-1");
    fresh.style.translate = [0.0, 100.0];
    fresh.style.opacity = 1.0;
    fresh.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 300.0,
        delay_ms: 0.0,
        timing: crate::style::TimingFunction::Ease,
    });

    adopt_keyed_overlay(&mut fresh, &mut prev);
    assert!((fresh.style.translate[0] - 10.0).abs() < f32::EPSILON);
    assert!(
        (fresh.style.opacity - 1.0).abs() < f32::EPSILON,
        "opacity is not in the declared transition list and must not be whitelist-copied"
    );
}

#[test]
fn viewport_portal_stage_sets_portal_key() {
    let stage = viewport_portal_stage("notify-host", Element::new(), 9000);
    assert_eq!(
        stage.attrs.get(ATTR_PORTAL_KEY).map(String::as_str),
        Some("notify-host")
    );
    assert_eq!(
        stage.attrs.get("jadix_portal").map(String::as_str),
        Some("1")
    );
}

#[test]
fn preserve_keyed_motion_on_reconcile_copies_mid() {
    let mut prev = Element::new().template_id("toast-card-9");
    prev.style.scale = [0.9, 0.9];
    prev.style.shadow_color = [0.0, 0.0, 0.0, 0.4];
    prev.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: crate::style::TimingFunction::Ease,
    });

    let mut fresh = Element::new().template_id("toast-card-9");
    fresh.style.scale = [1.0, 1.0];
    fresh.style.transition = prev.style.transition.clone();

    assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
    assert!((fresh.style.scale[0] - 0.9).abs() < f32::EPSILON);
    assert!(
        (fresh.style.shadow_color[3] - 0.0).abs() < f32::EPSILON,
        "box-shadow is not an active tween property here"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn presence_wrapper_reconcile_keeps_exit_opacity_and_seeds_tween() {
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);

    let mut prev = Element::new().template_id("gal-pres-exit-rsx__presence");
    prev.style.opacity = 1.0;
    prev.style.transition.push(TransitionEntry {
        property: TransitionProperty::Opacity,
        duration_ms: 320.0,
        delay_ms: 0.0,
        timing: crate::style::TimingFunction::Ease,
    });

    let mut fresh = Element::new().template_id("gal-pres-exit-rsx__presence");
    fresh.style.opacity = 0.0;
    fresh.style.transition = prev.style.transition.clone();

    assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
    assert!(
        (fresh.style.opacity - 0.0).abs() < f32::EPSILON,
        "exit target must not be copied back to the visible used value"
    );
    assert!(
        fresh.transition_snapshot.has_any_tweens(),
        "first exit must seed a snapshot so tick can retarget 1→0"
    );
    let mut style = fresh.style.clone();
    let mut snap = fresh.transition_snapshot.clone();
    assert!(crate::style::tick_element_transitions(
        &mut style, &mut snap, 0.0
    ));
    crate::style::set_css_transition_timeline_ms(160.0);
    let _ = crate::style::tick_element_transitions(&mut style, &mut snap, 0.0);
    assert!(
        style.opacity > 0.05 && style.opacity < 0.95,
        "opacity should be mid-fade, got {}",
        style.opacity
    );
    crate::style::set_css_transition_virtual_timeline(false);
}

#[test]
fn modal_shell_motion_skips_keyed_preserve_on_reconcile_when_idle() {
    let mut prev = Element::new().template_id("dlg__modal_panel");
    prev.style.opacity = 1.0;
    prev.style.translate = [0.0, 0.0];

    let mut fresh = Element::new().template_id("dlg__modal_panel");
    fresh.style.opacity = 1.0;
    fresh.style.translate = [0.0, 520.0];

    assert!(!preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
    assert!((fresh.style.translate[1] - 520.0).abs() < f32::EPSILON);
}

#[test]
fn floating_panel_motion_preserves_expand_mid_on_reconcile() {
    use crate::style::{
        keyframes_anim_entry, AnimationFillMode, AnimationSnapshot, TimingFunction,
    };
    use taffy::style::Dimension;

    let mut prev = Element::new().template_id("menu__floating_panel");
    prev.style.max_size.height = Dimension::length(40.0);
    prev.style.opacity = 1.0;
    prev.style.animations = vec![keyframes_anim_entry(
        "accordion-down",
        200.0,
        TimingFunction::EaseOut,
        AnimationFillMode::Both,
    )];
    prev.animation_snapshot = AnimationSnapshot::default();
    prev.layout_bounds = [10.0, 20.0, 100.0, 80.0];
    prev.set_attr(ATTR_FLOATING_CONTENT_H, "200");
    prev.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");

    let mut fresh = Element::new().template_id("menu__floating_panel");
    fresh.style.max_size.height = Dimension::auto();
    fresh.style.opacity = 1.0;

    assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
    assert!(
        !fresh.style.animations.is_empty(),
        "accordion-down must survive reconcile"
    );
    assert!(
        matches!(
            crate::layout::dim_kind(fresh.style.max_size.height),
            crate::layout::DimKind::Length(_)
        ),
        "expand mid max-height must survive"
    );
    assert!(
        (fresh.layout_bounds[3] - 80.0).abs() < f32::EPSILON,
        "measured height bounds must survive for next flush"
    );
    assert_eq!(
        fresh.attrs.get(ATTR_FLOATING_CONTENT_H).map(String::as_str),
        Some("200"),
        "content height must survive reconcile so exit flip stays stable"
    );
    assert_eq!(
        fresh
            .attrs
            .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
            .map(String::as_str),
        Some("492")
    );
}

#[test]
fn floating_panel_measure_does_not_copy_clipped_max_height() {
    use taffy::style::Dimension;

    let mut prev = Element::new().template_id("menu__floating_panel");
    prev.style.max_size.height = Dimension::length(80.0);
    prev.layout_bounds = [10.0, 20.0, 100.0, 80.0];

    let mut fresh = Element::new().template_id("menu__floating_panel");
    fresh.style.max_size.height = Dimension::auto();
    fresh.set_attr(ATTR_FLOATING_EXPAND_MEASURE, "1");
    fresh.layout_bounds = [0.0, 0.0, 100.0, 0.0];

    let _ = preserve_keyed_motion_on_reconcile(&mut fresh, &prev);
    assert!(
        fresh.style.max_size.height.is_auto(),
        "measure frame must not inherit the overflow-capped max-height"
    );
    assert!(
        fresh.layout_bounds[3] < 0.5,
        "measure frame must not inherit clipped layout_bounds"
    );
}

#[test]
fn floating_panel_motion_preserves_fade_mid_on_hoist() {
    let mut prev = Element::new().template_id("tip__floating_panel");
    prev.style.opacity = 0.4;
    prev.style.animations = vec![crate::style::keyframes_anim_entry(
        "fade-in-0",
        300.0,
        crate::style::TimingFunction::EaseOut,
        crate::style::AnimationFillMode::Both,
    )];

    let mut fresh = Element::new().template_id("tip__floating_panel");
    fresh.style.opacity = 1.0;

    adopt_keyed_overlay(&mut fresh, &mut prev);
    assert!(
        (fresh.style.opacity - 0.4).abs() < f32::EPSILON,
        "fade mid opacity must survive hoist adopt"
    );
    assert!(!fresh.style.animations.is_empty());
}

#[test]
#[cfg(feature = "animation-base")]
fn modal_shell_motion_preserves_enter_mid_on_hoist() {
    use crate::style::{
        set_css_transition_timeline_ms, set_css_transition_virtual_timeline, TimingFunction,
    };

    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);

    let mut prev = Element::new().template_id("dlg__modal_panel");
    prev.style.scale = [0.96, 0.96];
    prev.style.opacity = 1.0;
    prev.style.transition.push(TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: 250.0,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    });
    prev.snap_css_transition_baseline();
    prev.style.scale = [1.0, 1.0];
    let _ = prev.tick_css_transitions(0.0);
    set_css_transition_timeline_ms(120.0);
    let _ = prev.tick_css_transitions(0.0);
    assert!(
        prev.style.scale[0] > 0.97 && prev.style.scale[0] < 0.995,
        "sanity: prev should be mid-enter scale, got {}",
        prev.style.scale[0]
    );

    let mut fresh = Element::new().template_id("dlg__modal_panel");
    fresh.style.scale = [1.0, 1.0];
    fresh.style.opacity = 1.0;
    fresh.style.transition = prev.style.transition.clone();

    adopt_keyed_overlay(&mut fresh, &mut prev);
    assert!(
        (fresh.style.scale[0] - prev.style.scale[0]).abs() < f32::EPSILON,
        "enter scale mid must survive hoist adopt, fresh={} prev={}",
        fresh.style.scale[0],
        prev.style.scale[0]
    );
    assert!(
        (fresh.style.opacity - 1.0).abs() < f32::EPSILON,
        "panel must stay opaque during enter"
    );

    set_css_transition_virtual_timeline(false);
}

#[test]
fn modal_shell_preserves_exit_keyframes_before_first_tick() {
    let mut prev = Element::new().template_id("dlg__modal_overlay");
    prev.style.opacity = 1.0;
    prev.style.animations = vec![crate::style::keyframes_anim_entry(
        "fade-out-0",
        200.0,
        crate::style::TimingFunction::EaseIn,
        crate::style::AnimationFillMode::Both,
    )];
    prev.animation_snapshot = Default::default();

    let mut fresh = Element::new().template_id("dlg__modal_overlay");
    fresh.style.opacity = 1.0;

    adopt_keyed_overlay(&mut fresh, &mut prev);
    assert!(
        fresh
            .style
            .animations
            .iter()
            .any(|a| a.name == "fade-out-0"),
        "exit fade-out-0 must survive hoist adopt even before the first CSS tick"
    );
    assert!(
        (fresh.style.opacity - 1.0).abs() < f32::EPSILON,
        "exit adopt must start from the open pose, got {}",
        fresh.style.opacity
    );
}

#[test]
fn modal_shell_exit_seed_survives_adopt_over_settled_enter() {
    let mut prev = Element::new().template_id("sheet__modal_overlay");
    prev.style.opacity = 1.0;
    prev.style.animations = vec![crate::style::keyframes_anim_entry(
        "fade-in-0",
        450.0,
        crate::style::TimingFunction::EaseOut,
        crate::style::AnimationFillMode::Both,
    )];

    let mut fresh = Element::new().template_id("sheet__modal_overlay");
    fresh.style.opacity = 1.0;
    fresh.style.animations = vec![crate::style::keyframes_anim_entry(
        "fade-out-0",
        400.0,
        crate::style::TimingFunction::EaseIn,
        crate::style::AnimationFillMode::Both,
    )];

    adopt_keyed_overlay(&mut fresh, &mut prev);
    assert!(
        fresh
            .style
            .animations
            .iter()
            .any(|a| a.name == "fade-out-0"),
        "exit fade-out-0 must not be replaced by leftover fade-in-0"
    );
}

#[test]
#[cfg(feature = "animation-base")]
fn preserve_copies_active_width_tween_on_non_keyed_node() {
    use crate::style::{
        set_css_transition_timeline_ms, set_css_transition_virtual_timeline, TimingFunction,
    };
    use taffy::style::Dimension;

    set_css_transition_virtual_timeline(true);
    set_css_transition_timeline_ms(0.0);

    let mut prev = Element::new().template_id("prov__rail_open");
    prev.style.size.width = Dimension::length(256.0);
    prev.style.min_size.width = Dimension::length(256.0);
    prev.style.transition.push(TransitionEntry {
        property: TransitionProperty::Width,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    prev.style.transition.push(TransitionEntry {
        property: TransitionProperty::MinWidth,
        duration_ms: 200.0,
        delay_ms: 0.0,
        timing: TimingFunction::Linear,
    });
    let _ = prev.tick_css_transitions(0.0);
    prev.style.size.width = Dimension::length(48.0);
    prev.style.min_size.width = Dimension::length(48.0);
    let _ = prev.tick_css_transitions(0.0);
    set_css_transition_timeline_ms(80.0);
    let _ = prev.tick_css_transitions(0.0);
    let mid = crate::layout::dim_length_px(prev.style.size.width).unwrap_or(-1.0);
    assert!(
        mid > 48.0 && mid < 256.0,
        "sanity: prev should be mid-collapse, got {mid}"
    );

    let mut fresh = Element::new().template_id("prov__rail_open");
    fresh.style.size.width = Dimension::length(256.0);
    fresh.style.min_size.width = Dimension::length(256.0);
    fresh.style.transition = prev.style.transition.clone();

    assert!(preserve_keyed_motion_on_reconcile(&mut fresh, &prev));
    let kept = crate::layout::dim_length_px(fresh.style.size.width).unwrap_or(-1.0);
    assert!(
        (kept - mid).abs() < 0.5,
        "used width must survive rebuild paint 16rem, mid={mid} got {kept}"
    );
    assert!(fresh.transition_snapshot.has_active_tweens());

    set_css_transition_virtual_timeline(false);
}

#[test]
#[cfg(feature = "product-tests")]
fn hoisted_exit_keeps_close_button_hover_before_relayout() {
    crate::core::reactivity::create_root(|| {
        crate::style::set_css_transition_virtual_timeline(true);
        let build = || {
            Element::new()
                .template_id("close-hover__floating_panel")
                .w(220.0)
                .h(100.0)
                .p(8.0)
                .flex_col()
                .child(
                    Element::from(crate::components::button::button("hoisted-close").text("Close"))
                        .w(160.0)
                        .h(32.0),
                )
        };
        let mut tree = crate::layout::LayoutTree::new();
        let mut previous = build();
        tree.build_and_layout(&mut previous, 240.0, 160.0, [-1.0; 2]);
        for time in [0.0, 200.0, 400.0] {
            crate::style::set_css_transition_timeline_ms(time);
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::style::sync_class_variants_subtree(
                &mut previous,
                [20.0, 20.0],
                "",
                false,
                240.0,
                160.0,
                &mut tree,
                &mut impact,
            );
        }
        let hovered = previous.children[0].style.bg_color;
        let bounds = previous.children[0].layout_bounds;
        let mut fresh = build();
        fresh.set_attr(crate::utils::popup::ATTR_FLOATING_EXITING, "1");
        adopt_keyed_overlay(&mut fresh, &mut previous);
        assert!(
            fresh.children[0].layout_node.is_none(),
            "hoist must allocate fresh Taffy nodes"
        );
        crate::style::set_css_transition_timeline_ms(416.0);
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut fresh,
            [20.0, 20.0],
            "",
            false,
            240.0,
            160.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            fresh.children[0].style.bg_color, hovered,
            "exit must not reset hovered chrome"
        );
        assert_eq!(fresh.children[0].layout_bounds, bounds);
        crate::style::set_css_transition_virtual_timeline(false);
    });
}
