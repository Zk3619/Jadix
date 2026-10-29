use super::*;

#[test]
fn slide_exit_keeps_the_last_visible_pixels_until_completion() {
    for motion in [
        ModalSurfaceMotion::SlideFromRight,
        ModalSurfaceMotion::SlideFromBottom,
    ] {
        let mut overlay = Element::new().opacity(0.03);
        let mut panel = Element::new();
        let closed = motion.closed_translate_pct();
        panel.style.translate_pct = [closed[0] * 0.97, closed[1] * 0.97];
        assert!(!exit_visual_settled(&overlay, &panel, motion));
        overlay.style.opacity = 0.0;
        panel.style.translate_pct = closed;
        assert!(exit_visual_settled(&overlay, &panel, motion));
    }
}
use crate::core::reactivity::{create_effect, create_root};
#[cfg(feature = "animation-base")]
use crate::style::{set_css_transition_timeline_ms, set_css_transition_virtual_timeline};
use crate::utils::popup::{mount_modal_layer, mount_viewport_portal_stage};
#[cfg(feature = "animation-base")]
use std::time::{Duration, Instant};

fn attach_dialog_zoom_shell(modal_id: &str, panel: &mut Element) {
    panel.template_id = Some(format!("{modal_id}__modal_panel"));
    let mut zoom = Element::new();
    zoom.template_id = Some(dialog_zoom_tid(modal_id));
    panel.children.push(zoom);
}

#[test]
fn dialog_open_arms_event_loop_wake_before_first_tick() {
    create_root(|| {
        set_modal_css_needs_frame_for_test(false);
        let _ = take_modal_force_immediate_wake();
        let handle = use_modal_presence("wake-before-tick");
        assert!(
            !any_modal_css_needs_frame(),
            "precondition: idle CSS pump must be off"
        );
        sync_modal_presence(&handle, true);
        assert!(
            handle.cell.borrow().needs_enter,
            "open must set needs_enter"
        );
        assert!(
            any_modal_css_needs_frame(),
            "open must arm about_to_wait before flush/tick"
        );
        assert!(
            take_modal_force_immediate_wake(),
            "open must force one Poll so caret/long-press WaitUntil (~500ms) is not the next wake"
        );
        assert!(
            !take_modal_force_immediate_wake(),
            "immediate wake is one-shot; later frames Poll via any_modal_css_needs_frame"
        );
        assert!(
            any_modal_css_needs_frame(),
            "CSS pump must stay armed after the one-shot is consumed"
        );
    });
}

#[test]
fn sync_modal_presence_survives_mounted_effect_reentry() {
    use std::cell::Cell;
    use std::rc::Rc;
    create_root(|| {
        let handle = use_modal_presence("reentry-test");
        let mounted = mounted_signal(&handle);
        let runs = Rc::new(Cell::new(0u32));
        let runs_eff = Rc::clone(&runs);
        create_effect(move || {
            let _ = mounted.get();
            runs_eff.set(runs_eff.get() + 1);
        });
        assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
        assert!(mounted.get());
        assert_eq!(
                runs.get(),
                2,
                "opening must notify mounted so the paint island (which tracks mounted, not open) mounts"
            );
        assert_eq!(sync_modal_presence(&handle, true), PresencePhase::Present);
        assert_eq!(runs.get(), 2, "already-open sync must not remount");
        let animated = cfg!(feature = "animation-base");
        assert_eq!(
            sync_modal_presence(&handle, false),
            if animated {
                PresencePhase::Exiting
            } else {
                PresencePhase::Absent
            }
        );
        assert_eq!(mounted.get(), animated);
        assert_eq!(runs.get(), if animated { 2 } else { 3 });
    });
}

#[test]
fn dialog_enter_reseeds_when_rebuild_lands_at_rest() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-enter-reseed", &mut panel);
        let handle = use_modal_presence("enter-reseed");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-enter-reseed",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        let mut fresh_overlay = Element::new();
        fresh_overlay.template_id = Some("dlg-enter-reseed__modal_overlay".into());
        fresh_overlay.style.opacity = 1.0;
        let mut fresh_panel = Element::new();
        attach_dialog_zoom_shell("dlg-enter-reseed", &mut fresh_panel);
        fresh_panel.template_id = Some("dlg-enter-reseed__modal_panel".into());
        root.children.clear();
        root.children.push(fresh_overlay);
        root.children.push(fresh_panel);

        assert!(flush_modal_shell_motion_subtree(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg-enter-reseed__modal_overlay").unwrap();
        assert!(
            overlay.style.opacity < 0.05,
            "rebuild at rest must re-seed fade-in, not present opaque, got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "enter keyframes must be restored after rebuild"
        );
    });
}

#[test]
fn dialog_enter_seeds_keyframes_once() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("dlg-enter");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "overlay must run fade-in keyframes"
        );
        assert!(
            handle.cell.borrow().enter_active,
            "Dialog enter must keep enter_active like Sheet so flush keeps requesting frames"
        );
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "second flush must not drop enter keyframes mid-flight"
        );
    });
}

#[test]
fn dialog_fade_flush_falls_back_to_panel_without_zoom() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.template_id = Some("dlg-fb__modal_panel".into());
        let handle = use_modal_presence("dlg-fb");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-fb",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg-fb__modal_overlay").unwrap();
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "missing __modal_zoom must still seed overlay fade so the scrim is not stuck at 0"
        );
    });
}

#[test]
fn apply_registers_shell_flush_bootstraps_enter() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("build-enter");
        sync_modal_presence(&handle, true);
        assert!(apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        ));
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let o_ptr = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
        let p_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
        let (overlay, zoom) = unsafe { (&*o_ptr, &*p_ptr) };
        assert!(
            overlay.style.opacity < 0.05,
            "flush must start overlay faded, got {}",
            overlay.style.opacity
        );
        assert!(overlay
            .style
            .animations
            .iter()
            .any(|a| a.name == "fade-in-0"));
        assert!(zoom.style.animations.iter().any(|a| a.name == "zoom-in-90"));
        assert!(
            zoom.style.animations.iter().any(|a| a.name == "fade-in-0"),
            "panel must fade together with the overlay (shadcn `fade-in-0 zoom-in-90`)"
        );
        assert!(
            (zoom.style.scale[0] - DIALOG_ENTER_SCALE).abs() < 0.02,
            "zoom layer starts at zoom-in-90, got {}",
            zoom.style.scale[0]
        );
        assert!(
            zoom.style.opacity < 0.05,
            "panel must start invisible, not as an opaque 95% render, got {}",
            zoom.style.opacity
        );
    });
}

#[test]
fn dialog_enter_settles_and_clears_needs_enter() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("enter-settle");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(!handle.cell.borrow().needs_enter);
        for _ in 0..30 {
            let o_ptr =
                find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (o, z) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
            let _ = o.tick_css_animations(0.016);
            let _ = z.tick_css_animations(0.016);
        }
        let _ = flush_modal_shell_motion_subtree(&mut root);
        let overlay = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap();
        assert!(
            (overlay.style.opacity - 1.0).abs() < 0.05,
            "enter animation should settle to opaque, got {}",
            overlay.style.opacity
        );
        assert!(
            !handle.cell.borrow().enter_active,
            "settled Dialog must drop enter_active so the redraw pump stops"
        );
    });
}

#[test]
fn dialog_second_flush_does_not_reseed_enter_keyframes() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("enter-progress");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let elapsed_before = find_template_id_mut(&mut root, "dlg__modal_overlay")
            .unwrap()
            .animation_snapshot
            .runs
            .first()
            .map(|r| r.elapsed_ms)
            .unwrap_or(0.0);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let elapsed_after = find_template_id_mut(&mut root, "dlg__modal_overlay")
            .unwrap()
            .animation_snapshot
            .runs
            .first()
            .map(|r| r.elapsed_ms)
            .unwrap_or(0.0);
        assert!(
            (elapsed_after - elapsed_before).abs() < 1.0,
            "second flush must not restart fade-in, before={elapsed_before} after={elapsed_after}"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_enter_layout_delay_must_not_skip_to_settled() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-delay", &mut panel);
        let handle = use_modal_presence("enter-delay");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-delay",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        // Gallery first-open layout often lands 80–150ms after `enter_started`.
        handle.cell.borrow_mut().enter_started = Some(Instant::now() - Duration::from_millis(150));
        {
            let o = find_template_id_mut(&mut root, "dlg-delay__modal_overlay").unwrap();
            o.animation_snapshot = Default::default();
            let z = find_template_id_mut(&mut root, "dlg-delay__modal_zoom").unwrap();
            z.animation_snapshot = Default::default();
        }
        let _ = tick_hoisted_modal_css(&mut root);
        let overlay = find_template_id_mut(&mut root, "dlg-delay__modal_overlay").unwrap();
        assert!(
            overlay.style.opacity < 0.35,
            "150ms layout delay must not seek the 200ms fade to opaque; got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "enter keyframes must still be playing after first present"
        );
    });
}

#[test]
fn flush_finds_shell_after_portal_hoist() {
    use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};
    use crate::layout::layout_tree::hoist_portal_subtrees;
    use crate::primitives::overlay::overlay;

    clear_overlay_stack();
    create_root(|| {
        let open = use_signal("dlg-open", || true);
        let handle = use_modal_presence("gal-dialog");
        sync_modal_presence(&handle, true);
        let Some(zs) = portal_sync("gal-dialog", open, OverlayKind::Modal, OverlayMode::Stacked)
        else {
            panic!("portal_sync");
        };

        let mut overlay_el = overlay(zs.underlay).color([0.0, 0.0, 0.0, 0.5]).into();
        let mut panel = Element::new().w(320.0).h(200.0).bg([1.0, 1.0, 1.0, 1.0]);
        attach_dialog_zoom_shell("gal-dialog", &mut panel);
        apply_modal_shell_motion(
            "gal-dialog",
            &mut overlay_el,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        let stage = mount_viewport_portal_stage(
            crate::primitives::r#box::r#box()
                .inset(0.0)
                .w("100%")
                .h("100%")
                .items_center()
                .justify_center()
                .child(panel)
                .into(),
            zs.panel,
        );
        let shell = mount_modal_layer(overlay_el, stage);

        let mut root = Element::new().w(800.0).h(600.0).child(shell);
        assert!(hoist_portal_subtrees(&mut root) >= 2);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        let panel_ptr = find_template_id_mut(&mut root, "gal-dialog__modal_panel")
            .expect("hoisted panel must keep template_id") as *mut Element;
        let zoom_ptr = find_template_id_mut(&mut root, "gal-dialog__modal_zoom")
            .expect("hoisted zoom layer must exist") as *mut Element;
        let (panel, zoom) = unsafe { (&*panel_ptr, &*zoom_ptr) };
        assert!(
            (panel.style.opacity - 1.0).abs() < 0.01,
            "panel shell must stay opaque, got {}",
            panel.style.opacity
        );
        assert!(
            (panel.style.scale[0] - 1.0).abs() < 0.01,
            "panel shell must not scale (layout stable), got {}",
            panel.style.scale[0]
        );
        assert!(
            zoom.style.scale[0] < 0.99,
            "zoom layer should start scaled down on enter, got {}",
            zoom.style.scale[0]
        );
    });
    clear_overlay_stack();
}

#[test]
fn dialog_exit_finish_defers_mounted_until_drain() {
    use crate::app::portal_overlay::{clear_overlay_stack, portal_sync, OverlayKind, OverlayMode};

    create_root(|| {
        clear_overlay_stack();
        let handle = use_modal_presence("defer-unmount");
        sync_modal_presence(&handle, true);
        mounted_signal(&handle).set(true);
        let mounted = mounted_signal(&handle);
        let _ = portal_sync("dlg", mounted, OverlayKind::Modal, OverlayMode::Stacked);
        assert!(mounted.peek());

        finish_dialog_exit_presence(&handle, "dlg");
        assert!(
            mounted.peek(),
            "mounted must stay true until drain (AppState may be borrowed during flush)"
        );
        drain_pending_modal_unmounts();
        assert!(!mounted.peek());
        assert_eq!(
            crate::app::portal_overlay::overlay_stack_depth_for_test(),
            0
        );
    });
}

/// The reported artifact: the panel used to render opaque at 95% and then snap to 100%,
/// out of step with the overlay. Overlay and panel must fade together while the panel
/// scales up.
#[test]
#[cfg(feature = "animation-base")]
fn dialog_enter_fades_overlay_and_panel_together() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("enter-unified-fade");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        // ~40% through the 200ms enter.
        for _ in 0..5 {
            let o_ptr =
                find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
            let (o, z) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
            let _ = o.tick_css_animations(0.016);
            let _ = z.tick_css_animations(0.016);
        }

        let o_ptr = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
        let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
        let (overlay, zoom) = unsafe { (&*o_ptr, &*z_ptr) };
        assert!(
            zoom.style.opacity > 0.05 && zoom.style.opacity < 0.95,
            "panel must be mid-fade, not opaque, got {}",
            zoom.style.opacity
        );
        assert!(
            (zoom.style.opacity - overlay.style.opacity).abs() < 0.1,
            "panel and overlay share one fade curve: overlay={} panel={}",
            overlay.style.opacity,
            zoom.style.opacity
        );
        assert!(
            zoom.style.scale[0] > DIALOG_ENTER_SCALE && zoom.style.scale[0] < 1.0,
            "panel must still be zooming while it fades, got {}",
            zoom.style.scale[0]
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_hoisted_modal_css_exit_dirties_overlay_paint() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let _ = crate::app::damage::take_frame_paint_nodes();
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-dirty", &mut panel);
        let handle = use_modal_presence("exit-dirty");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-dirty",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        {
            let o = find_template_id_mut(&mut root, "dlg-dirty__modal_overlay").unwrap();
            o.layout_bounds = [0.0, 0.0, 800.0, 600.0];
            let z = find_template_id_mut(&mut root, "dlg-dirty__modal_zoom").unwrap();
            z.layout_bounds = [200.0, 120.0, 400.0, 280.0];
        }
        let _ = crate::app::damage::take_frame_paint_nodes();
        assert!(tick_hoisted_modal_css(&mut root));
        let dirty = crate::app::damage::take_frame_paint_nodes();
        assert!(
                dirty.iter().any(|d| {
                    matches!(d.kind, crate::app::damage::PaintDirtyKind::Overlay)
                }),
                "exit CSS tick must dest-reset Overlay occupancy (PartialLoad otherwise freezes the last stamp), got {dirty:?}"
            );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_opacity_keeps_moving_until_css_finishes() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-mono", &mut panel);
        let handle = use_modal_presence("exit-mono");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-mono",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..20 {
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let _ = tick_hoisted_modal_css(&mut root);
        }
        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let mut last_opacity = 1.0_f32;
        let mut last_scale = 1.0_f32;
        let mut saw_mid = false;
        let mut plateau = 0u32;
        for i in 0..24 {
            let _ = flush_modal_shell_motion_subtree(&mut root);
            if !handle.cell.borrow().gate.is_mounted() {
                assert!(
                        last_opacity < 0.05,
                        "unmount must wait until fade-out 100% sample, froze at {last_opacity} on tick {i}"
                    );
                break;
            }
            let _ = tick_hoisted_modal_css(&mut root);
            let (opacity, scale) = {
                let o_ptr = find_template_id_mut(&mut root, "dlg-mono__modal_overlay").unwrap()
                    as *mut Element;
                let z_ptr = find_template_id_mut(&mut root, "dlg-mono__modal_zoom").unwrap()
                    as *mut Element;
                let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                (o.style.opacity, z.style.scale[0])
            };
            if opacity < 0.95 && opacity > 0.08 {
                saw_mid = true;
            }
            assert!(
                opacity <= last_opacity + 1e-4,
                "exit overlay opacity must not rise, {last_opacity} → {opacity} at tick {i}"
            );
            assert!(
                scale <= last_scale + 1e-4,
                "exit zoom scale must not rise, {last_scale} → {scale} at tick {i}"
            );
            if (last_opacity - opacity).abs() < 1e-5 && opacity > 0.08 {
                plateau += 1;
                assert!(
                    plateau < 3,
                    "exit opacity froze at {opacity} while still mounted (tick {i})"
                );
            } else {
                plateau = 0;
            }
            last_opacity = opacity;
            last_scale = scale;
        }
        assert!(
            saw_mid,
            "exit must pass through mid opacities, not skip to unmount"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_finish_tick_dirties_end_pose() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let _ = crate::app::damage::take_frame_paint_nodes();
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-end", &mut panel);
        let handle = use_modal_presence("exit-end");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-end",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..20 {
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let _ = tick_hoisted_modal_css(&mut root);
        }
        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        {
            let o = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap();
            o.layout_bounds = [0.0, 0.0, 800.0, 600.0];
            let z = find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap();
            z.layout_bounds = [200.0, 120.0, 400.0, 280.0];
        }
        let mut saw_finish = false;
        for i in 0..24 {
            let before = {
                let o_ptr = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap()
                    as *mut Element;
                let z_ptr =
                    find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap() as *mut Element;
                let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                dialog_css_animating(o, z)
            };
            let _ = crate::app::damage::take_frame_paint_nodes();
            let _ = tick_hoisted_modal_css(&mut root);
            let (after, opacity) = {
                let o_ptr = find_template_id_mut(&mut root, "dlg-end__modal_overlay").unwrap()
                    as *mut Element;
                let z_ptr =
                    find_template_id_mut(&mut root, "dlg-end__modal_zoom").unwrap() as *mut Element;
                let (o, z) = unsafe { (&*o_ptr, &*z_ptr) };
                (dialog_css_animating(o, z), o.style.opacity)
            };
            let dirty = crate::app::damage::take_frame_paint_nodes();
            if before && !after {
                saw_finish = true;
                assert!(
                    opacity < 0.05,
                    "Finished fill must write fade-out 100% this tick, got {opacity} at {i}"
                );
                assert!(
                        dirty.iter().any(|d| {
                            matches!(d.kind, crate::app::damage::PaintDirtyKind::Overlay)
                        }),
                        "Finished sample must dest-reset Overlay (ease-in tail otherwise freezes), got {dirty:?}"
                    );
                assert!(
                    handle.cell.borrow().gate.is_mounted(),
                    "unmount is the next flush, not the finishing tick"
                );
                break;
            }
        }
        assert!(saw_finish, "exit keyframes must reach Finished");
        let _ = flush_modal_shell_motion_subtree(&mut root);
        assert!(
            !handle.cell.borrow().gate.is_mounted(),
            "next flush unmounts after the end pose has been dirtied"
        );
    });
}

#[test]
fn tick_hoisted_modal_css_keeps_redraw_flag_while_dialog_enter_plays() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-pump", &mut panel);
        let handle = use_modal_presence("enter-pump");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-pump",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(
            tick_hoisted_modal_css(&mut root),
            "enter keyframes must still need frames after the first tick"
        );
        assert!(
            any_modal_css_needs_frame(),
            "redraw pump must see Dialog CSS as active (not only cubic tweens)"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_hoisted_modal_css_first_frame_leaves_seed_without_settling() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-hold", &mut panel);
        let handle = use_modal_presence("enter-hold");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-hold",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(tick_hoisted_modal_css(&mut root));
        let zoom = find_template_id_mut(&mut root, "dlg-hold__modal_zoom").unwrap();
        assert!(
            zoom.style.scale[0] > DIALOG_ENTER_SCALE + 0.002,
            "first present after click must leave the 0.90 seed pose, got {}",
            zoom.style.scale[0]
        );
        assert!(
            zoom.style.opacity > 0.01 && zoom.style.opacity < 0.5,
            "first present must already be fading in, not blank or settled, got {}",
            zoom.style.opacity
        );
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        assert!(tick_hoisted_modal_css(&mut root));
        let zoom = find_template_id_mut(&mut root, "dlg-hold__modal_zoom").unwrap();
        let elapsed = zoom
            .animation_snapshot
            .runs
            .iter()
            .map(|r| r.elapsed_ms)
            .fold(0.0_f32, f32::max);
        assert!(
            elapsed > 20.0,
            "second tick must consume frame dt, elapsed_ms={elapsed}"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn tick_hoisted_modal_css_idle_jump_does_not_settle_enter() {
    create_root(|| {
        crate::style::utility::set_frame_dt(2.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-idle", &mut panel);
        let handle = use_modal_presence("enter-idle");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-idle",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(tick_hoisted_modal_css(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg-idle__modal_overlay").unwrap();
        assert!(
            overlay.style.opacity > 0.01 && overlay.style.opacity < 0.35,
            "idle click must paint one 16ms step, not a blank seed or a settled dialog, got {}",
            overlay.style.opacity
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_enter_timeline_hitch_must_not_skip_fade() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-hitch", &mut panel);
        let handle = use_modal_presence("enter-hitch");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-hitch",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(tick_hoisted_modal_css(&mut root));
        let opacity_before = find_template_id_mut(&mut root, "dlg-hitch__modal_overlay")
            .unwrap()
            .style
            .opacity;
        // First-open layout / caret WaitUntil: CSS timeline jumps 150ms while
        // the modal stepper still reports one 16ms frame.
        set_css_transition_timeline_ms(150.0);
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        assert!(tick_hoisted_modal_css(&mut root));
        let overlay = find_template_id_mut(&mut root, "dlg-hitch__modal_overlay").unwrap();
        let elapsed = overlay
            .animation_snapshot
            .runs
            .iter()
            .map(|r| r.elapsed_ms)
            .fold(0.0_f32, f32::max);
        assert!(
            overlay.style.opacity < 0.45,
            "150ms hitch must not seek the 200ms fade; before={opacity_before} after={}",
            overlay.style.opacity
        );
        assert!(
            elapsed < 50.0,
            "modal tick must consume capped dt, not last_tick_ms wall; elapsed_ms={elapsed}"
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "enter keyframes must still be playing"
        );
        set_css_transition_virtual_timeline(false);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_timeline_hitch_must_not_skip_fade() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-exit-hitch", &mut panel);
        let handle = use_modal_presence("exit-hitch");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-exit-hitch",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for i in 0..40 {
            set_css_transition_timeline_ms(i as f64 * (1000.0 / 60.0));
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let _ = tick_hoisted_modal_css(&mut root);
        }
        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let t0 = 40.0 * (1000.0 / 60.0);
        set_css_transition_timeline_ms(t0);
        assert!(tick_hoisted_modal_css(&mut root));
        let opacity_before = find_template_id_mut(&mut root, "dlg-exit-hitch__modal_overlay")
            .unwrap()
            .style
            .opacity;
        set_css_transition_timeline_ms(t0 + 150.0);
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        assert!(tick_hoisted_modal_css(&mut root));
        assert!(
            handle.cell.borrow().gate.is_mounted(),
            "exit hitch must not finish the 200ms fade in one tick"
        );
        let overlay = find_template_id_mut(&mut root, "dlg-exit-hitch__modal_overlay").unwrap();
        let elapsed = overlay
            .animation_snapshot
            .runs
            .iter()
            .map(|r| r.elapsed_ms)
            .fold(0.0_f32, f32::max);
        assert!(
            overlay.style.opacity > 0.55,
            "150ms hitch must not skip fade-out; before={opacity_before} after={}",
            overlay.style.opacity
        );
        assert!(
            elapsed < 50.0,
            "exit tick must consume capped dt, not last_tick_ms wall; elapsed_ms={elapsed}"
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"),
            "exit keyframes must still be playing"
        );
        set_css_transition_virtual_timeline(false);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_seeds_fade_and_zoom_out() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg", &mut panel);
        let handle = use_modal_presence("exit-anim");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let o_ptr = find_template_id_mut(&mut root, "dlg__modal_overlay").unwrap() as *mut Element;
        let z_ptr = find_template_id_mut(&mut root, "dlg__modal_zoom").unwrap() as *mut Element;
        let (overlay, zoom) = unsafe { (&*o_ptr, &*z_ptr) };
        assert!(overlay
            .style
            .animations
            .iter()
            .any(|a| a.name == "fade-out-0"));
        assert!(
            zoom.style.animations.iter().any(|a| a.name == "fade-out-0"),
            "panel must fade out with the overlay"
        );
        assert!(
            zoom.style
                .animations
                .iter()
                .any(|a| a.name == "zoom-out-90"),
            "exit must mirror enter (shadcn `fade-out-0 zoom-out-90`)"
        );
        // Exit starts from the settled pose; the keyframes take it down from here.
        assert!((zoom.style.scale[0] - 1.0).abs() < 0.01);
        assert!((zoom.style.opacity - 1.0).abs() < 0.01);
    });
}

/// Gallery AlertDialog Action/Cancel rebuilds overlay+zoom after the first
/// exit flush consumed `needs_exit_anim`. Without a reseed, flush treats
/// empty keyframes as "done" and unmounts — overlay dest-resets first,
/// then the card pops off (no fade / zoom-out).
#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_reseeds_when_rebuild_drops_keyframes() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-exit-reseed", &mut panel);
        let handle = use_modal_presence("exit-reseed");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-exit-reseed",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..30 {
            let _ = tick_hoisted_modal_css(&mut root);
            let _ = flush_modal_shell_motion_subtree(&mut root);
        }

        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(
            handle.cell.borrow().gate.is_mounted(),
            "first exit flush must keep the shell mounted"
        );

        let mut fresh_overlay = Element::new();
        fresh_overlay.template_id = Some("dlg-exit-reseed__modal_overlay".into());
        fresh_overlay.style.opacity = 1.0;
        let mut fresh_panel = Element::new();
        attach_dialog_zoom_shell("dlg-exit-reseed", &mut fresh_panel);
        fresh_panel.template_id = Some("dlg-exit-reseed__modal_panel".into());
        root.children.clear();
        root.children.push(fresh_overlay);
        root.children.push(fresh_panel);

        assert!(
            flush_modal_shell_motion_subtree(&mut root),
            "rebuild after exit seed must re-seed, not finish"
        );
        assert!(
            handle.cell.borrow().gate.is_mounted(),
            "rebuild must not force-absent; that pops the scrim then the card"
        );
        let o_ptr = find_template_id_mut(&mut root, "dlg-exit-reseed__modal_overlay").unwrap()
            as *mut Element;
        let z_ptr =
            find_template_id_mut(&mut root, "dlg-exit-reseed__modal_zoom").unwrap() as *mut Element;
        let (overlay, zoom) = unsafe { (&*o_ptr, &*z_ptr) };
        assert!(
            overlay.style.opacity > 0.7,
            "reseeded overlay must start from the open pose, got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"),
            "rebuild must re-seed overlay fade-out-0"
        );
        assert!(
            zoom.style.animations.iter().any(|a| a.name == "fade-out-0"),
            "rebuild must re-seed panel fade-out-0"
        );
        assert!(
            zoom.style
                .animations
                .iter()
                .any(|a| a.name == "zoom-out-90"),
            "rebuild must re-seed zoom-out-90 so the card does not pop"
        );
    });
}

#[cfg(feature = "animation-base")]
fn mark_dialog_css_runs_finished(el: &mut Element) {
    let n = el.style.animations.len().max(1);
    let baseline = crate::style::AnimationBaseline::from_style(&el.style);
    el.animation_snapshot.runs = (0..n)
        .map(|_| crate::style::AnimationRunState {
            elapsed_ms: 10_000.0,
            baseline,
            finished: true,
            entry_fp: 0,
        })
        .collect();
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_must_not_finish_while_open_pose_has_stale_snapshot() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-exit-stale", &mut panel);
        let handle = use_modal_presence("exit-stale");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-exit-stale",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        for _ in 0..30 {
            let _ = tick_hoisted_modal_css(&mut root);
            let _ = flush_modal_shell_motion_subtree(&mut root);
        }

        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        {
            let o_ptr = find_template_id_mut(&mut root, "dlg-exit-stale__modal_overlay").unwrap()
                as *mut Element;
            let z_ptr = find_template_id_mut(&mut root, "dlg-exit-stale__modal_zoom").unwrap()
                as *mut Element;
            let (overlay, zoom) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
            overlay.style.opacity = 1.0;
            zoom.style.opacity = 1.0;
            zoom.style.scale = [1.0, 1.0];
            mark_dialog_css_runs_finished(overlay);
            mark_dialog_css_runs_finished(zoom);
        }

        assert!(
            flush_modal_shell_motion_subtree(&mut root),
            "stale finished snapshot at the open pose must revive exit, not finish"
        );
        assert!(
            handle.cell.borrow().gate.is_mounted(),
            "finished enter snapshot copied onto exit names must not force-absent"
        );
        let overlay = find_template_id_mut(&mut root, "dlg-exit-stale__modal_overlay").unwrap();
        assert!(
            overlay.style.opacity > 0.7,
            "revived overlay must stay on the open pose, got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"),
            "revived exit must keep fade-out-0"
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn dialog_exit_hold_seed_does_not_seek_stale_enter_elapsed() {
    create_root(|| {
        crate::style::utility::set_frame_dt(1.0 / 60.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        attach_dialog_zoom_shell("dlg-exit-seek", &mut panel);
        let handle = use_modal_presence("exit-seek");
        sync_modal_presence(&handle, true);
        apply_modal_shell_motion(
            "dlg-exit-seek",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::DialogFade,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            800.0,
        );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        sync_modal_presence(&handle, false);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        // Action-click rebuild: snapshots dropped while `enter_started` still
        // points at open time (hundreds of ms ago).
        {
            let mut c = handle.cell.borrow_mut();
            c.enter_started = Some(Instant::now() - Duration::from_millis(800));
        }
        let o_ptr = find_template_id_mut(&mut root, "dlg-exit-seek__modal_overlay").unwrap()
            as *mut Element;
        let z_ptr =
            find_template_id_mut(&mut root, "dlg-exit-seek__modal_zoom").unwrap() as *mut Element;
        {
            let (overlay, zoom) = unsafe { (&mut *o_ptr, &mut *z_ptr) };
            overlay.animation_snapshot = Default::default();
            zoom.animation_snapshot = Default::default();
        }
        let _ = tick_hoisted_modal_css(&mut root);
        let overlay = unsafe { &*o_ptr };
        assert!(
            overlay.style.opacity > 0.7,
            "exit hold_seed must not seek enter elapsed (would finish fade in one tick), got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-out-0"),
            "exit keyframes must still be playing"
        );
    });
}

#[test]
fn slide_enter_does_not_settle_while_panel_offscreen() {
    create_root(|| {
        let mut overlay = Element::new();
        overlay.style.opacity = 1.0;
        let mut panel = Element::new();
        panel.style.opacity = 1.0;
        panel.style.translate = [0.0, 400.0];
        panel.style.translate_pct = [0.0, 1.0];
        assert!(
            !enter_visual_settled(
                &overlay,
                &panel,
                ModalSurfaceMotion::SlideFromBottom,
                [0.0, 0.0],
                400.0,
            ),
            "opacity=1 must not settle a slide that is still translated"
        );
        panel.style.translate = [0.0, 0.0];
        panel.style.translate_pct = [0.0, 0.0];
        assert!(enter_visual_settled(
            &overlay,
            &panel,
            ModalSurfaceMotion::SlideFromBottom,
            [0.0, 0.0],
            400.0,
        ));
        // 4% of a 400px sheet is 16px. Nailing that remainder in one frame is the
        // reported end-of-enter snap (Dialog already tightened zoom to 0.002).
        panel.style.translate_pct = [0.0, 0.03];
        assert!(
            !enter_visual_settled(
                &overlay,
                &panel,
                ModalSurfaceMotion::SlideFromBottom,
                [0.0, 0.0],
                400.0,
            ),
            "3% remaining travel must not count as settled (that nail is the end-of-enter jump)"
        );
    });
}

#[test]
fn slide_closed_target_ignores_drag_extra() {
    let closed = panel_motion_target(
        ModalSurfaceMotion::SlideFromBottom,
        false,
        1.0,
        [0.0, 200.0],
        800.0,
    );
    assert!(
        (closed.translate[1] - 800.0).abs() < 0.01,
        "closed must be full off-screen, not closed+extra; got {}",
        closed.translate[1]
    );
}

#[test]
fn slide_closed_translate_is_off_the_dock_edge() {
    let bottom = ModalSurfaceMotion::SlideFromBottom.closed_translate(300.0);
    assert_eq!(bottom, [0.0, 300.0], "bottom sheet hides downward");
    let top = ModalSurfaceMotion::SlideFromTop.closed_translate(300.0);
    assert_eq!(top, [0.0, -300.0], "top sheet hides upward");
    let left = ModalSurfaceMotion::SlideFromLeft.closed_translate(240.0);
    assert_eq!(left, [-240.0, 0.0], "left sheet hides leftward");
    let right = ModalSurfaceMotion::SlideFromRight.closed_translate(240.0);
    assert_eq!(right, [240.0, 0.0], "right sheet hides rightward");
}

#[test]
fn slide_axis_prefers_laid_out_panel_size() {
    let mut panel = Element::new();
    panel.layout_bounds = [0.0, 500.0, 800.0, 280.0];
    let axis = slide_axis_for_panel(ModalSurfaceMotion::SlideFromBottom, &panel, 900.0);
    assert!(
        (axis - 280.0).abs() < 0.01,
        "enter/exit travel should be panel height, got {axis}"
    );
    panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];
    let fallback = slide_axis_for_panel(ModalSurfaceMotion::SlideFromBottom, &panel, 900.0);
    assert!((fallback - 900.0).abs() < 0.01);
}

#[test]
fn slide_enter_flush_seeds_closed_translate() {
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        let handle = use_modal_presence("slide-enter");
        sync_modal_presence(&handle, true);
        assert!(apply_modal_shell_motion(
            "slide",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromBottom,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            600.0,
        ));
        assert!(
                panel.style.translate_pct[1].abs() > 0.9,
                "apply must hide the sheet off-screen before flush, otherwise the first paint is the dock"
            );
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let o_ptr =
            find_template_id_mut(&mut root, "slide__modal_overlay").unwrap() as *mut Element;
        let p_ptr = find_template_id_mut(&mut root, "slide__modal_panel").unwrap() as *mut Element;
        let (overlay, panel) = unsafe { (&*o_ptr, &*p_ptr) };
        assert!(
            overlay.style.opacity < 0.05,
            "overlay enter mid should start faded, got {}",
            overlay.style.opacity
        );
        assert!(
            overlay
                .style
                .animations
                .iter()
                .any(|a| a.name == "fade-in-0"),
            "slide overlay enter must use fade-in-0 keyframes (rebuild-proof)"
        );
        assert!(
            panel
                .style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-modal-slide-in-")),
            "slide panel enter must use translate_pct keyframes, not a pixel jump"
        );
        assert!(
            panel.style.translate_pct[1].abs() > 0.9,
            "slide enter must start off-screen at 100%, got pct={}",
            panel.style.translate_pct[1]
        );
        assert!(
            panel.style.translate[1].abs() < 8.0,
            "enter travel is translate_pct, not viewport pixels, got ty={}",
            panel.style.translate[1]
        );
        assert!(handle.cell.borrow().enter_active);
    });
}

#[test]
fn slide_enter_does_not_snap_the_ease_out_tail() {
    crate::style::utility::set_frame_dt(1.0 / 60.0);
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 200.0, 400.0, 400.0];
        let handle = use_modal_presence("slide-enter-tail");
        sync_modal_presence(&handle, true);
        assert!(apply_modal_shell_motion(
            "slide-tail",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromBottom,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            400.0,
        ));
        root.children.push(overlay);
        root.children.push(panel);
        assert!(flush_modal_shell_motion_subtree(&mut root));

        let mut nail_from = None;
        for _ in 0..40 {
            let pct = {
                let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                p.style.translate_pct[1]
            };
            let had_enter = {
                let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("jadix-modal-slide-in-"))
            };
            let _ = flush_modal_shell_motion_subtree(&mut root);
            let _ = tick_hoisted_modal_css(&mut root);
            let has_enter = {
                let p = find_template_id_mut(&mut root, "slide-tail__modal_panel").unwrap();
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("jadix-modal-slide-in-"))
            };
            if had_enter && !has_enter {
                nail_from = Some(pct);
                break;
            }
        }
        let from = nail_from.expect("enter keyframes must clear after the slide finishes");
        assert!(
                from.abs() < 0.005,
                "nailing enter while {from} remaining (~{}px on a 400px panel) is the end-of-enter snap",
                from.abs() * 400.0
            );
    });
    crate::style::utility::set_frame_dt(1.0 / 60.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_exit_keeps_needs_exit_anim_until_flush() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        let handle = use_modal_presence("slide-flag");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-flag",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromRight,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            400.0,
        );
        overlay.style.opacity = 1.0;
        panel.style.translate = [0.0, 0.0];
        root.children.push(overlay);
        root.children.push(panel);

        assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
        {
            let o_ptr = find_template_id_mut(&mut root, "slide-flag__modal_overlay").unwrap()
                as *mut Element;
            let p_ptr =
                find_template_id_mut(&mut root, "slide-flag__modal_panel").unwrap() as *mut Element;
            let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
            apply_modal_shell_motion(
                "slide-flag",
                o,
                p,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Exiting,
                &handle,
                [0.0, 0.0],
                false,
                400.0,
            );
        }
        assert!(
            handle.cell.borrow().needs_exit_anim,
            "apply only registers; consuming the flag here skips the post-hoist slide seed"
        );
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(!handle.cell.borrow().needs_exit_anim);
        let ov = find_template_id_mut(&mut root, "slide-flag__modal_overlay").unwrap();
        assert!(
            ov.style.animations.iter().any(|a| a.name == "fade-out-0"),
            "flush after apply must seed overlay fade-out"
        );
        let p = find_template_id_mut(&mut root, "slide-flag__modal_panel").unwrap();
        assert!(
            p.style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-modal-slide-out-")),
            "flush after apply must seed panel slide-out keyframes"
        );
        set_css_transition_virtual_timeline(false);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_overlay_exit_clamps_idle_frame_dt() {
    crate::style::utility::set_frame_dt(2.0);
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        let handle = use_modal_presence("slide-idle-dt");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-idle",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromRight,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            400.0,
        );
        overlay.style.opacity = 1.0;
        overlay.style.animations.clear();
        panel.style.translate = [0.0, 0.0];
        root.children.push(overlay);
        root.children.push(panel);
        assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
        {
            let o_ptr = find_template_id_mut(&mut root, "slide-idle__modal_overlay").unwrap()
                as *mut Element;
            let p_ptr =
                find_template_id_mut(&mut root, "slide-idle__modal_panel").unwrap() as *mut Element;
            let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
            apply_modal_shell_motion(
                "slide-idle",
                o,
                p,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Exiting,
                &handle,
                [0.0, 0.0],
                false,
                400.0,
            );
        }
        let _ = flush_modal_shell_motion_subtree(&mut root);
        let _ = tick_hoisted_modal_css(&mut root);
        let ov = find_template_id_mut(&mut root, "slide-idle__modal_overlay").unwrap();
        assert!(
            ov.style.opacity > 0.15 && animation_run_active(&ov.style, &ov.animation_snapshot),
            "idle frame_dt must not finish fade-out-0 in one tick, got {}",
            ov.style.opacity
        );
        let panel = find_template_id_mut(&mut root, "slide-idle__modal_panel").unwrap();
        assert!(
            panel.style.translate_pct[0] > 0.02 && panel.style.translate_pct[0] < 0.35,
            "idle frame_dt must sample the slide-out middle, not skip to 100%, got pct={}",
            panel.style.translate_pct[0]
        );
        let visual_tx = panel.layout_bounds[2] * panel.style.translate_pct[0];
        assert!(
            visual_tx < panel.layout_bounds[2] * 0.4,
            "travel must be a fraction of the panel, not the viewport fallback, got {visual_tx}"
        );
    });
    crate::style::utility::set_frame_dt(1.0 / 60.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_exit_second_flush_does_not_restart_tween() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        let handle = use_modal_presence("slide-exit");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-x",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromBottom,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            600.0,
        );
        overlay.style.opacity = 1.0;
        panel.style.translate = [0.0, 0.0];
        root.children.push(overlay);
        root.children.push(panel);

        assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
        {
            let o_ptr =
                find_template_id_mut(&mut root, "slide-x__modal_overlay").unwrap() as *mut Element;
            let p_ptr =
                find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap() as *mut Element;
            let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
            apply_modal_shell_motion(
                "slide-x",
                o,
                p,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Exiting,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            );
        }
        assert!(flush_modal_shell_motion_subtree(&mut root));
        assert!(
            !handle.cell.borrow().needs_exit_anim,
            "exit bootstrap must run only once"
        );
        {
            let o = find_template_id_mut(&mut root, "slide-x__modal_overlay").unwrap();
            assert!(
                o.style.animations.iter().any(|a| a.name == "fade-out-0"),
                "slide overlay exit must use fade-out-0 keyframes so blur fades with the panel"
            );
        }
        let seed = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
        let ty_seed = seed.style.translate[1];
        let pct_seed = seed.style.translate_pct[1];
        assert!(
            ty_seed < 80.0 && pct_seed.abs() < 0.08,
            "first exit sample must stay near the open pose, got ty={ty_seed} pct={pct_seed}"
        );
        {
            let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
            assert!(
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("jadix-modal-slide-out-")),
                "exit must install slide-out keyframes"
            );
        }
        let _ = flush_modal_shell_motion_subtree(&mut root);
        let again = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
        let ty_again = again.style.translate[1];
        let pct_again = again.style.translate_pct[1];
        assert!(
                ty_again < 80.0 && pct_again.abs() < 0.08,
                "later flushes must not SnapAfterEnsure the closed pose, got ty={ty_again} pct={pct_again}"
            );
        {
            let p = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
            for _ in 0..12 {
                let _ = tick_element_css(p, 0.08);
            }
        }
        let end = find_template_id_mut(&mut root, "slide-x__modal_panel").unwrap();
        set_css_transition_virtual_timeline(false);
        assert!(
            end.style.translate_pct[1] > 0.9,
            "exit keyframes must be able to finish after a second flush, got pct={}",
            end.style.translate_pct[1]
        );
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_exit_reseeds_transform_when_rebuild_drops_transition() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        let handle = use_modal_presence("slide-reseed");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-reseed",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromBottom,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            600.0,
        );
        overlay.style.opacity = 1.0;
        panel.style.translate = [0.0, 0.0];
        root.children.push(overlay);
        root.children.push(panel);

        assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
        {
            let o_ptr = find_template_id_mut(&mut root, "slide-reseed__modal_overlay").unwrap()
                as *mut Element;
            let p_ptr = find_template_id_mut(&mut root, "slide-reseed__modal_panel").unwrap()
                as *mut Element;
            let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
            apply_modal_shell_motion(
                "slide-reseed",
                o,
                p,
                ModalSurfaceMotion::SlideFromBottom,
                PresencePhase::Exiting,
                &handle,
                [0.0, 0.0],
                false,
                600.0,
            );
        }
        assert!(flush_modal_shell_motion_subtree(&mut root));

        let mut fresh = Element::new();
        fresh.template_id = Some("slide-reseed__modal_panel".into());
        fresh.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        fresh.style.translate = [0.0, 0.0];
        let idx = root
            .children
            .iter()
            .position(|c| c.template_id.as_deref() == Some("slide-reseed__modal_panel"))
            .expect("panel");
        root.children[idx] = fresh;
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let p = find_template_id_mut(&mut root, "slide-reseed__modal_panel").unwrap();
        assert!(
                p.style
                    .animations
                    .iter()
                    .any(|a| a.name.starts_with("jadix-modal-slide-out-")),
                "reactive rebuild must re-seed panel slide-out, otherwise overlay fades and the sheet sticks at the dock"
            );
        assert!(
            p.style.translate[1].abs() < 80.0 && p.style.translate_pct[1].abs() < 0.08,
            "reseed must start from the open pose, got ty={} pct={}",
            p.style.translate[1],
            p.style.translate_pct[1]
        );
        set_css_transition_virtual_timeline(false);
    });
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_exit_plays_inbetween_frames_not_viewport_jump() {
    crate::style::utility::set_frame_dt(2.0);
    create_root(|| {
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        // Typical right Sheet: 18rem panel, full viewport fallback.
        panel.layout_bounds = [0.0, 0.0, 288.0, 600.0];
        let handle = use_modal_presence("slide-mid");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-mid",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromRight,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            1920.0,
        );
        overlay.style.opacity = 1.0;
        overlay.style.animations.clear();
        panel.style.translate = [0.0, 0.0];
        root.children.push(overlay);
        root.children.push(panel);
        assert_eq!(sync_modal_presence(&handle, false), PresencePhase::Exiting);
        {
            let o_ptr = find_template_id_mut(&mut root, "slide-mid__modal_overlay").unwrap()
                as *mut Element;
            let p_ptr =
                find_template_id_mut(&mut root, "slide-mid__modal_panel").unwrap() as *mut Element;
            let (o, p) = unsafe { (&mut *o_ptr, &mut *p_ptr) };
            apply_modal_shell_motion(
                "slide-mid",
                o,
                p,
                ModalSurfaceMotion::SlideFromRight,
                PresencePhase::Exiting,
                &handle,
                [0.0, 0.0],
                false,
                1920.0,
            );
        }
        let _ = flush_modal_shell_motion_subtree(&mut root);
        let mut mids = Vec::new();
        for _ in 0..4 {
            let _ = tick_hoisted_modal_css(&mut root);
            let p = find_template_id_mut(&mut root, "slide-mid__modal_panel").unwrap();
            let visual = p.layout_bounds[2] * p.style.translate_pct[0];
            mids.push(visual);
        }
        let ov = find_template_id_mut(&mut root, "slide-mid__modal_overlay").unwrap();
        assert!(
            ov.style.opacity > 0.25 && ov.style.opacity < 0.98,
            "scrim must still be fading after a few frames, got {}",
            ov.style.opacity
        );
        assert!(
            mids.iter().any(|&x| x > 8.0 && x < 288.0 * 0.85),
            "exit must paint an in-between slide, not dock→off-screen; frames={mids:?}"
        );
        assert!(
            mids.iter().all(|&x| x < 288.0 * 0.95),
            "idle dt must not finish a 400ms slide in 8 frames, frames={mids:?}"
        );
    });
    crate::style::utility::set_frame_dt(1.0 / 60.0);
}

#[test]
#[cfg(feature = "animation-base")]
fn slide_settle_starts_from_drag_pose_not_closed() {
    create_root(|| {
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut root = Element::new();
        let mut overlay = Element::new();
        let mut panel = Element::new();
        panel.layout_bounds = [0.0, 0.0, 400.0, 600.0];
        let handle = use_modal_presence("slide-settle");
        sync_modal_presence(&handle, true);
        {
            let mut c = handle.cell.borrow_mut();
            c.needs_enter = false;
            c.enter_active = false;
        }
        apply_modal_shell_motion(
            "slide-s",
            &mut overlay,
            &mut panel,
            ModalSurfaceMotion::SlideFromBottom,
            PresencePhase::Present,
            &handle,
            [0.0, 0.0],
            false,
            600.0,
        );
        overlay.style.opacity = 1.0;
        overlay.style.transition.clear();
        overlay.style.animations.clear();
        panel.style.translate = [0.0, 0.0];
        panel.style.translate_pct = [0.0, 0.4];
        panel.style.transition.clear();
        root.children.push(overlay);
        root.children.push(panel);
        arm_modal_slide_settle("slide-s", [0.0, 240.0]);
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let p = find_template_id_mut(&mut root, "slide-s__modal_panel").unwrap();
        let pct_y = p.style.translate_pct[1];
        assert!(
                pct_y > 0.15 && pct_y < 0.55,
                "release snap must continue from the drag pose, not re-enter from off-screen, got {pct_y}"
            );
        assert!(
            !p.style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-modal-slide-in-")),
            "snap must not seed enter keyframes"
        );
        assert!(flush_modal_shell_motion_subtree(&mut root));
        let p = find_template_id_mut(&mut root, "slide-s__modal_panel").unwrap();
        assert!(
            p.style.translate_pct[1] > 0.1 && p.style.translate_pct[1] < 0.55,
            "second settle frame must not jump to closed 100%, got {}",
            p.style.translate_pct[1]
        );
        assert!(
            !p.style
                .animations
                .iter()
                .any(|a| a.name.starts_with("jadix-modal-slide-in-")),
            "continuation must not re-seed enter from 100%"
        );
        set_css_transition_virtual_timeline(false);
    });
}
