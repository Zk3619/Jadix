use super::*;
use crate::core::reactivity::create_root;

#[test]
fn nested_child_owner_reregisters_after_parent_effect_rerun() {
    use crate::core::reactivity::{create_effect, with_child_owner};
    create_root(|| {
        let tick = Signal::new(0u32);
        create_effect(move || {
            let _ = tick.get();
            with_child_owner(|| {
                let h = use_floating_presence("rebind-fmt", FloatingMotionKind::Expand);
                sync_floating_presence(&h, true);
            });
        });
        assert!(
            any_floating_motion_needs_frame(),
            "first child-owner mount must register Expand presence"
        );
        tick.set(1);
        assert!(
                any_floating_motion_needs_frame(),
                "parent overlay rebuild disposes the Select owner; named use_signal must rebind the flush registry"
            );
    });
}

#[test]
fn idle_presence_tick_must_not_drop_closed_overlay_before_first_open() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let open = Signal::new(false);
        let handle = use_floating_presence("idle-first-open", FloatingMotionKind::Fade);
        let _ = floating_portal_phase(
            "idle-first-open",
            open,
            &handle,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            crate::app::portal_overlay::ScrollFreeze::None,
        );
        let _ = tick_floating_presence();
        drain_pending_floating_unmounts();

        open.set(true);
        let phase = sync_floating_presence(&handle, true);
        assert!(
            apply_floating_panel_motion(
                "idle-first-open",
                &mut panel,
                FloatingMotionKind::Fade,
                phase,
                &handle,
                open,
                OverlayKind::Popover,
                OverlayMode::Stacked,
                None,
            ),
            "first open after idle ticks must still register Fade enter"
        );
        root.children.push(panel);
        assert!(
            flush_floating_motion_subtree(&mut root),
            "flush must see the overlay after idle prune"
        );
        let panel = find_template_id_mut(&mut root, "idle-first-open__floating_panel").unwrap();
        assert!(
            panel.style.animations.iter().any(|a| a.name == "fade-in-0"),
            "first open after idle ticks must seed fade-in-0, not stay opacity 0"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn fade_enter_seeds_once() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let open = Signal::new(true);
        let handle = use_floating_presence("fade-enter", FloatingMotionKind::Fade);
        sync_floating_presence(&handle, true);
        assert!(apply_floating_panel_motion(
            "fade-enter",
            &mut panel,
            FloatingMotionKind::Fade,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            None,
        ));
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "fade-enter__floating_panel").unwrap();
        assert!(
            panel.style.animations.iter().any(|a| a.name == "fade-in-0"),
            "must seed fade-in-0"
        );
        assert!(panel.style.opacity < 0.05);
        let elapsed_before = panel
            .animation_snapshot
            .runs
            .first()
            .map(|r| r.elapsed_ms)
            .unwrap_or(0.0);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "fade-enter__floating_panel").unwrap();
        let elapsed_after = panel
            .animation_snapshot
            .runs
            .first()
            .map(|r| r.elapsed_ms)
            .unwrap_or(0.0);
        assert!(
            (elapsed_after - elapsed_before).abs() < 1.0,
            "second flush must not restart fade-in"
        );
    });
}

#[test]
fn expand_open_stays_busy_across_queries_unlike_one_shot_wake() {
    create_root(|| {
        FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
        let fade = use_floating_presence("fade-parent", FloatingMotionKind::Fade);
        sync_floating_presence(&fade, true);
        assert!(
            any_floating_motion_needs_frame(),
            "parent Fade open must be visible to about_to_wait before flush"
        );
        // One-shot take() would return false here and starve the nested Expand.
        assert!(
            any_floating_motion_needs_frame(),
            "must be a live query, not a consumed latch"
        );
        let nested = use_floating_presence("nested-fmt", FloatingMotionKind::Expand);
        sync_floating_presence(&nested, true);
        assert!(
            any_floating_motion_needs_frame(),
            "nested Expand open must still pump after the parent Fade already entered"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn fade_css_tick_does_not_request_second_layout() {
    create_root(|| {
        FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
        let mut root = Element::new();
        let mut panel = Element::new();
        let open = Signal::new(true);
        let handle = use_floating_presence("fade-norel", FloatingMotionKind::Fade);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "fade-norel",
            &mut panel,
            FloatingMotionKind::Fade,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(
            flush_floating_motion_subtree(&mut root),
            "Fade enter still needs paint frames"
        );
        assert!(
            !floating_motion_needs_relayout(),
            "Fade seed must not dirty Taffy / second collect"
        );
        crate::style::utility::set_frame_dt(0.016);
        assert!(tick_hoisted_floating_css(&mut root));
        crate::style::utility::set_frame_dt(0.0);
        assert!(
            !floating_motion_needs_relayout(),
            "Fade opacity tick is paint-only"
        );
    });
}

#[test]
fn expand_flush_requests_second_layout_for_measure() {
    create_root(|| {
        FLOATING_CSS_NEEDS_FRAME.with(|c| c.set(false));
        let mut root = Element::new();
        let mut panel = Element::new();
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-relayout", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-relayout",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        assert!(
            floating_motion_needs_relayout(),
            "Expand measure / max-height seed must same-frame relayout"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn fade_exit_seeds_and_defers_unmount() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let open = Signal::new(true);
        let handle = use_floating_presence("fade-exit", FloatingMotionKind::Fade);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "fade-exit",
            &mut panel,
            FloatingMotionKind::Fade,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        // Settle enter.
        for _ in 0..40 {
            let p = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
            let _ = p.tick_css_animations(0.016);
        }
        let _ = flush_floating_motion_subtree(&mut root);

        open.set(false);
        sync_floating_presence(&handle, false);
        assert_eq!(handle.cell.borrow().gate.phase(), PresencePhase::Exiting);
        assert!(floating_mounted_signal(&handle).peek());
        // Re-apply shell with exiting phase.
        let panel = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
        apply_floating_panel_motion(
            "fade-exit",
            panel,
            FloatingMotionKind::Fade,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "fade-exit__floating_panel").unwrap();
        assert!(panel
            .style
            .animations
            .iter()
            .any(|a| a.name == "fade-out-0"));
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_waits_for_measured_height() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-wait", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-wait",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-wait__floating_panel").unwrap();
        assert!(
            panel.style.animations.is_empty(),
            "must not seed before measured height"
        );
        panel.layout_bounds[3] = 120.0;
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-wait__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| is_expand_down_anim(&a.name)),
            "must seed expand-down after measure"
        );
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(999.0);
        assert!(
            mh < 2.0,
            "seed+tick(0) must start at max-height≈0, got {mh}"
        );
        assert!(
            panel.style.opacity < 0.05,
            "enter seed must reveal the shadow gradually from opacity≈0"
        );
        let _ = panel.tick_css_animations(0.1);
        assert!(panel.style.opacity > 0.05 && panel.style.opacity < 0.99);
        for _ in 0..20 {
            let _ = panel.tick_css_animations(0.016);
        }
        assert!((panel.style.opacity - 1.0).abs() < 0.001);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_enter_same_frame_css_tick_must_stay_at_zero() {
    create_root(|| {
        crate::style::utility::set_frame_dt(0.016);
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-seed-dt", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-seed-dt",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-seed-dt__floating_panel").unwrap();
        panel.layout_bounds[3] = 192.0;
        panel
            .used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
        assert!(flush_floating_motion_subtree(&mut root));
        let _ = tick_hoisted_floating_css(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-seed-dt__floating_panel").unwrap();
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(999.0);
        assert!(
            mh < 2.0,
            "same-present CSS tick must not EaseOut the seed frame off 0, got {mh}"
        );
        crate::style::utility::set_frame_dt(0.0);
    });
}

#[test]
fn expand_enter_rebuild_must_not_zero_remembered_clip() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-clip0", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-clip0",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
        panel.layout_bounds[3] = 120.0;
        let bounds = panel.layout_bounds;
        panel.used_layout.stamp_border_box(bounds, false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
            for _ in 0..4 {
                let _ = panel.tick_css_animations(0.016);
            }
        }
        let mh = {
            let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
            crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0)
        };
        assert!(mh > 16.0, "accordion-down must have grown, got {mh}");
        {
            let mut c = handle.cell.borrow_mut();
            c.clip_height = Some(0.0);
            c.needs_enter = false;
            c.measuring = false;
            c.enter_active = true;
        }
        let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-clip0",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        let panel = find_template_id_mut(&mut root, "exp-clip0__floating_panel").unwrap();
        let mh2 = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            mh2 > 16.0,
            "island rebuild must not write remembered 0 over accordion clip, got {mh2}"
        );
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| is_expand_down_anim(&a.name)),
            "enter keyframes must survive rebuild"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_with_from_h_starts_at_baseline_not_zero() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-from", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-from",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from__floating_panel").unwrap();
        assert!(
            panel.style.animations.is_empty(),
            "must not seed before measured height"
        );
        panel.layout_bounds[3] = 120.0;
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-expand-down-")),
            "offset expand must use jadix-expand-down-* keyframes"
        );
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            mh > 20.0 && mh < 40.0,
            "seed+tick(0) must start near collapsed bar height, got {mh}"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_from_h_exit_collapses_to_chip_not_zero() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-from-exit", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-from-exit",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
        panel.layout_bounds[3] = 120.0;
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
            for _ in 0..20 {
                let _ = panel.tick_css_animations(0.016);
            }
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            panel.style.max_size.height = Dimension::length(120.0);
            panel
                .used_layout
                .stamp_border_box([0.0, 0.0, 100.0, 120.0], false);
        }

        open.set(false);
        sync_floating_presence(&handle, false);
        let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-from-exit",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from-exit__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-expand-up-")),
            "from_h exit must clip to the covered trigger, not accordion-up to 0; anims={:?}",
            panel
                .style
                .animations
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            !panel
                .style
                .animations
                .iter()
                .any(|a| a.name == EXPAND_KEYS.up),
            "cover expand must not accordion-up through the pill"
        );
        for _ in 0..8 {
            let _ = panel.tick_css_animations(0.016);
        }
        let mid = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            mid + 0.5 >= 28.0,
            "visible exit must not shrink below the reaction/chip height, got {mid}"
        );
        assert!(mid < 120.0, "exit must actually be collapsing, got {mid}");
        for _ in 0..16 {
            let _ = panel.tick_css_animations(0.016);
        }
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            (mh - 28.0).abs() < 3.0,
            "from_h exit must land on the covered trigger height, got {mh}"
        );
    });
}

#[test]
fn expand_with_from_w_clips_width_on_enter() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "32");
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, "80");
        panel.set_attr(ATTR_FLOATING_EXPAND_CLIP_X, "1");
        panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-from-w", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-from-w",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from-w__floating_panel").unwrap();
        panel.layout_bounds[2] = 240.0;
        panel.layout_bounds[3] = 48.0;
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-from-w__floating_panel").unwrap();
        let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
        assert!(
            (mw - 80.0).abs() < 2.0,
            "seed+tick(0) must start near collapsed row width, got {mw}"
        );
        assert_eq!(
            panel.style.overflow.x,
            Overflow::Hidden,
            "row expand must clip overflow-x"
        );
    });
}

#[test]
fn expand_from_w_without_clip_x_pins_width_for_wrap() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_H, "28");
        panel.set_attr(ATTR_FLOATING_EXPAND_FROM_W, "90");
        panel.style.flex_wrap = FlexWrap::Wrap;
        panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-pin-w", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-pin-w",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-pin-w__floating_panel").unwrap();
        let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
        assert!(
            (mw - 90.0).abs() < 2.0,
            "wrap expand must pin max-width to collapsed bar during measure, got {mw}"
        );
        assert_eq!(
            panel.style.flex_wrap,
            FlexWrap::Wrap,
            "wrap expand must not force nowrap"
        );
        panel.layout_bounds[2] = 90.0;
        panel.layout_bounds[3] = 56.0;
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-pin-w__floating_panel").unwrap();
        let mw = crate::layout::dim_length_px(panel.style.max_size.width).unwrap_or(0.0);
        assert!(
            (mw - 90.0).abs() < 2.0,
            "wrap expand must stay pinned after seed, got {mw}"
        );
        assert_eq!(panel.style.flex_wrap, FlexWrap::Wrap);
    });
}

#[test]
fn expand_does_not_seed_from_viewport_capped_first_layout() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.style.max_size.height = Dimension::length(80.0);
        panel.layout_bounds = [0.0, 0.0, 100.0, 80.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-cap", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-cap",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-cap__floating_panel").unwrap();
        assert!(
            panel.style.animations.is_empty(),
            "must not seed accordion from the overflow-capped first layout"
        );
        assert!(
            panel
                .attrs
                .get(ATTR_FLOATING_EXPAND_MEASURE)
                .map(|s| s.as_str())
                == Some("1"),
            "measure attr must stay until unconstrained height is known"
        );
        panel.layout_bounds[3] = 200.0;
        panel.style.max_size.height = Dimension::auto();
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-cap__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| is_expand_down_anim(&a.name)),
            "must seed after unconstrained measure"
        );
        let content = panel
            .attrs
            .get(ATTR_FLOATING_CONTENT_H)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.0);
        assert!(
            (content - 200.0).abs() < 0.5,
            "content height must be unconstrained 200, got {content}"
        );
    });
}

#[test]
fn expand_idle_does_not_shrink_measured_height_to_clip() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-idle", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-idle",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-idle__floating_panel").unwrap();
        panel.layout_bounds[3] = 200.0;
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-idle__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            handle.cell.borrow_mut().clip_height = None;
            panel.layout_bounds[3] = 80.0;
        }
        let _ = flush_floating_motion_subtree(&mut root);
        let stored = find_template_id_mut(&mut root, "exp-idle__floating_panel")
            .unwrap()
            .used_layout
            .anim_height();
        assert!(
            (stored - 200.0).abs() < 0.5,
            "idle clip 80 must not overwrite content measure 200, got {stored}"
        );
    });
}

#[test]
fn expand_idle_keeps_max_height_and_relayouts_unpin() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let mut child = Element::new();
        child.style.flex_shrink = 1.0;
        child.style.min_size.height = Dimension::length(0.0);
        panel.children.push(child);
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-idle-keep", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-idle-keep",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
        panel.layout_bounds[3] = 192.0;
        panel
            .used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
            assert!(
                panel.children[0].style.flex_shrink < 0.5,
                "enter must pin overflow contents"
            );
            for _ in 0..40 {
                let _ = panel.tick_css_animations(0.016);
            }
        }
        let idle = flush_floating_motion_subtree(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
        assert!(
            crate::layout::dim_length_px(panel.style.max_size.height).is_some(),
            "Expand Idle must not snap max-height to auto"
        );
        assert!(
            (panel.children[0].style.flex_shrink - 1.0).abs() < 0.01,
            "Idle must unpin ScrollArea"
        );
        assert!(
            idle,
            "first Idle after unpin must request a same-frame layout"
        );
        let mh_before_tick = crate::layout::dim_length_px(panel.style.max_size.height);
        let _ = tick_hoisted_floating_css(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-idle-keep__floating_panel").unwrap();
        let mh_after_tick = crate::layout::dim_length_px(panel.style.max_size.height);
        assert_eq!(
            mh_before_tick, mh_after_tick,
            "tick after Idle must not restore seed max-height auto"
        );
    });
}

#[test]
fn expand_enter_last_frame_stays_at_capped_height() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let mut child = Element::new();
        child.style.flex_shrink = 1.0;
        child.style.min_size.height = Dimension::length(0.0);
        panel.children.push(child);
        panel.style.padding.top = taffy::style::LengthPercentage::length(4.0);
        panel.style.padding.bottom = taffy::style::LengthPercentage::length(4.0);
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-cap-end", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-cap-end",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            Some(256.0),
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
        panel.layout_bounds[3] = 2000.0;
        panel
            .used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 2000.0], false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
            let pinned = crate::layout::dim_length_px(panel.children[0].style.min_size.height)
                .unwrap_or(0.0);
            assert!(
                (pinned - 248.0).abs() < 1.0,
                "overflow pin must be cap minus chrome padding, got {pinned}"
            );
            for _ in 0..40 {
                let _ = panel.tick_css_animations(0.016);
            }
        }
        let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
        let last_anim = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
                (last_anim - 256.0).abs() < 1.5,
                "finishing tick must land on the painted cap, not unconstrained intrinsic, got {last_anim}"
            );
        let _ = flush_floating_motion_subtree(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-cap-end__floating_panel").unwrap();
        let idle = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            (idle - last_anim).abs() < 1.0,
            "Idle must not dip below the last expand frame, last={last_anim} idle={idle}"
        );
        assert!(
            crate::layout::dim_length_px(panel.style.size.height).is_none(),
            "must not force a definite used height as a last-frame fallback"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_exit_reseed_continues_from_mid_height() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-exit-mid", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-exit-mid",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        // Pretend mid-exit after a rebuild wiped animations but kept mid max-height.
        let panel = find_template_id_mut(&mut root, "exp-exit-mid__floating_panel").unwrap();
        panel.style.animations.clear();
        panel.animation_snapshot = Default::default();
        panel.style.max_size.height = Dimension::length(48.0);
        panel
            .used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 120.0], false);

        open.set(false);
        sync_floating_presence(&handle, false);
        apply_floating_panel_motion(
            "exp-exit-mid",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-mid__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| a.name == EXPAND_KEYS.up),
            "menu expand exit must accordion-up; anims={:?}",
            panel
                .style
                .animations
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
        );
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            mh <= 50.0,
            "reseed must not jump back to full 120px, got {mh}"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_exit_hides_sliver_on_same_frame_as_finish() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-exit-hide", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-exit-hide",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
        panel.layout_bounds[3] = 120.0;
        let bounds = panel.layout_bounds;
        panel.used_layout.stamp_border_box(bounds, false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            panel.style.max_size.height = Dimension::length(120.0);
            panel.style.opacity = 1.0;
        }

        open.set(false);
        sync_floating_presence(&handle, false);
        let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-exit-hide",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == EXPAND_KEYS.up),
                "menu exit must accordion-up"
            );
            for _ in 0..20 {
                let _ = panel.tick_css_animations(0.016);
            }
            assert!(
                !animation_run_active(&panel.style, &panel.animation_snapshot),
                "accordion-up must finish"
            );
            assert!(
                panel.style.opacity > 0.5,
                "finished keyframe must not be the paint hide; got {}",
                panel.style.opacity
            );
        }
        assert!(
            tick_hoisted_floating_css(&mut root),
            "post-tick hide must request the same-frame collect"
        );
        let panel = find_template_id_mut(&mut root, "exp-exit-hide__floating_panel").unwrap();
        assert!(
            panel.style.opacity < 0.01,
            "sliver must be hidden before collect, got {}",
            panel.style.opacity
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_exit_hides_chrome_hairline_before_animation_finishes() {
    // Inline ColorPicker format Select: the last accordion frames are a 1–2px
    // white/outline streak that PartialLoad leaves above 预设 after close.
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-exit-hairline", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-exit-hairline",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-hairline__floating_panel").unwrap();
        panel.layout_bounds[3] = 120.0;
        let bounds = panel.layout_bounds;
        panel.used_layout.stamp_border_box(bounds, false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel =
                find_template_id_mut(&mut root, "exp-exit-hairline__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            panel.style.max_size.height = Dimension::length(120.0);
            panel.style.opacity = 1.0;
        }

        open.set(false);
        sync_floating_presence(&handle, false);
        let panel = find_template_id_mut(&mut root, "exp-exit-hairline__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-exit-hairline",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel =
                find_template_id_mut(&mut root, "exp-exit-hairline__floating_panel").unwrap();
            assert!(
                panel
                    .style
                    .animations
                    .iter()
                    .any(|a| a.name == EXPAND_KEYS.up || a.name.starts_with("jadix-expand-up-")),
                "exit must be accordion-up"
            );
            // Simulate the last hairline frames without waiting for the full ease-in.
            panel.style.max_size.height = Dimension::length(2.0);
            panel
                .animation_snapshot
                .sync_written_max_height(Dimension::length(2.0));
        }
        assert!(
            tick_hoisted_floating_css(&mut root),
            "hairline hide must request collect"
        );
        let panel = find_template_id_mut(&mut root, "exp-exit-hairline__floating_panel").unwrap();
        assert!(
            panel.style.opacity < 0.01,
            "chrome hairline must hide before collect, got opacity={}",
            panel.style.opacity
        );
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            mh < 0.5,
            "hairline max-height must collapse to 0 for stamp restore, got {mh}"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_exit_keeps_opacity_until_accordion_up_finishes() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 100.0, 120.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-exit-full", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-exit-full",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
        panel.layout_bounds[3] = 120.0;
        let bounds = panel.layout_bounds;
        panel.used_layout.stamp_border_box(bounds, false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            panel.style.max_size.height = Dimension::length(120.0);
            panel.style.opacity = 1.0;
        }

        open.set(false);
        sync_floating_presence(&handle, false);
        let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-exit-full",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
            for _ in 0..4 {
                let _ = panel.tick_css_animations(0.016);
            }
            let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
            assert!(
                animation_run_active(&panel.style, &panel.animation_snapshot),
                "accordion-up must still be running"
            );
            assert!(
                mh > 16.0 && panel.style.opacity > 0.5,
                "mid-exit must stay visible so accordion-up can finish; mh={mh} opacity={}",
                panel.style.opacity
            );
        }
        let _ = tick_hoisted_floating_css(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-exit-full__floating_panel").unwrap();
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(
            panel.style.opacity > 0.5 && mh > 16.0,
            "hide-on-sliver must not snap opacity at ~16px; mh={mh} opacity={}",
            panel.style.opacity
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn expand_exit_pins_child_min_height_so_list_overflows_instead_of_relayout() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let mut child = Element::new();
        child.style.flex_shrink = 1.0;
        child.style.min_size.height = Dimension::length(0.0);
        child.layout_bounds = [0.0, 0.0, 100.0, 240.0];
        panel.children.push(child);
        panel.layout_bounds = [0.0, 0.0, 100.0, 240.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-exit-clip", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-exit-clip",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
        panel.layout_bounds[3] = 240.0;
        let bounds = panel.layout_bounds;
        panel.used_layout.stamp_border_box(bounds, false);
        assert!(flush_floating_motion_subtree(&mut root));
        {
            let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
            panel.style.animations.clear();
            panel.animation_snapshot = Default::default();
            handle.cell.borrow_mut().enter_active = false;
            handle.cell.borrow_mut().needs_enter = false;
            handle.cell.borrow_mut().measuring = false;
            panel.style.max_size.height = Dimension::length(240.0);
            panel.style.opacity = 1.0;
            if let Some(child) = panel.children.first_mut() {
                child.style.flex_shrink = 1.0;
                child.style.min_size.height = Dimension::length(0.0);
            }
        }
        open.set(false);
        sync_floating_presence(&handle, false);
        let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
        apply_floating_panel_motion(
            "exp-exit-clip",
            panel,
            FloatingMotionKind::Expand,
            PresencePhase::Exiting,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| a.name == EXPAND_KEYS.up),
            "exit must stay accordion-up, not scale"
        );
        for _ in 0..8 {
            let _ = panel.tick_css_animations(0.016);
        }
        let _ = flush_floating_motion_subtree(&mut root);
        let panel = find_template_id_mut(&mut root, "exp-exit-clip__floating_panel").unwrap();
        let mh = crate::layout::dim_length_px(panel.style.max_size.height).unwrap_or(0.0);
        assert!(mh < 200.0, "accordion-up must shrink the clip, got {mh}");
        let child = panel.children.first().expect("list child");
        assert!(
            child.style.flex_shrink < 0.01,
            "list must not flex-shrink with the clip"
        );
        let child_min = crate::layout::dim_length_px(child.style.min_size.height).unwrap_or(0.0);
        assert!(
            child_min > 200.0,
            "open list min-height must survive the clip so overflow hides rows, got {child_min}"
        );
    });
}

#[test]
fn expand_pin_targets_scroll_viewport_rows_not_the_wrapper() {
    create_root(|| {
        let mut root = Element::new();
        let mut panel = Element::new();
        let mut rows = Element::new();
        rows.style.flex_shrink = 1.0;
        rows.style.min_size.height = Dimension::length(0.0);
        let mut viewport = Element::new().focus_id(
            crate::core::scroll_viewport::focus_id_for_scroll_viewport("fmt:menu_scroll"),
        );
        viewport.style.flex_shrink = 1.0;
        viewport.style.min_size.height = Dimension::length(0.0);
        viewport.children.push(rows);
        let mut outer = Element::new();
        outer.style.flex_shrink = 1.0;
        outer.style.min_size.height = Dimension::length(0.0);
        outer.children.push(viewport);
        panel.children.push(outer);
        panel.layout_bounds = [0.0, 0.0, 100.0, 0.0];
        let open = Signal::new(true);
        let handle = use_floating_presence("exp-pin-sa", FloatingMotionKind::Expand);
        sync_floating_presence(&handle, true);
        apply_floating_panel_motion(
            "exp-pin-sa",
            &mut panel,
            FloatingMotionKind::Expand,
            PresencePhase::Present,
            &handle,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            None,
        );
        root.children.push(panel);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-pin-sa__floating_panel").unwrap();
        panel.layout_bounds[3] = 192.0;
        panel
            .used_layout
            .stamp_border_box([0.0, 0.0, 100.0, 192.0], false);
        assert!(flush_floating_motion_subtree(&mut root));
        let panel = find_template_id_mut(&mut root, "exp-pin-sa__floating_panel").unwrap();
        let outer = panel.children.first().expect("ScrollArea wrapper");
        let viewport = outer.children.first().expect("scroll viewport");
        let rows = viewport.children.first().expect("rows");
        assert!(
            (outer.style.flex_shrink - 1.0).abs() < 0.01,
            "wrapper must shrink with the accordion so overflow:auto can clip a short list"
        );
        assert!(
            (viewport.style.flex_shrink - 1.0).abs() < 0.01,
            "scroll viewport must shrink with the accordion"
        );
        assert!(
            rows.style.flex_shrink < 0.01,
            "rows must not collapse into a one-line chip"
        );
        let row_min = crate::layout::dim_length_px(rows.style.min_size.height).unwrap_or(0.0);
        assert!(
            row_min > 100.0,
            "pinned rows min-height must be the open list, got {row_min}"
        );
    });
}
