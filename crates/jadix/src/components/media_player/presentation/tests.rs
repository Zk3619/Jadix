use super::*;
use crate::{
    core::reactivity::{create_effect, create_root},
    prelude::*,
};
use std::cell::RefCell;

#[test]
fn picture_changes_invalidate_retained_paint_fingerprints() {
    let (_owner, ()) = create_root(|| {
        let mut picture: Element = video().into();
        for (key, value) in [
            ("jadix_video_rotation", "90"),
            ("jadix_video_content_ratio", "4:3"),
            ("jadix_video_mirror_x", "true"),
            ("jadix_video_mirror_y", "true"),
        ] {
            crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut picture, false);
            let before = picture.collect_paint_hash;
            picture.set_attr(key, value);
            crate::layout::collect_cache::stamp_subtree_paint_hashes(&mut picture, false);
            assert_ne!(picture.collect_paint_hash, before, "{key}");
        }
    });
}

#[test]
fn sibling_presentations_keep_distinct_bindings_after_live_updates() {
    use crate::app::{
        reactive::{drain_deferred_view_remounts, reset_registered_anchors},
        runtime::{clear_test_reconcile_root, install_test_reconcile_root},
    };
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let (_owner, first) = create_root(|| {
        let first = MediaPresentationController::default();
        let second = MediaPresentationController::default();
        let view = first.clone();
        let mounted = tree.clone();
        create_effect(move || {
            let left: Element = video_player()
                .id("left-video")
                .presentation(view.clone())
                .slots(
                    MediaPlayerSlots::new()
                        .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Left")),
                )
                .into();
            let right: Element = video_player()
                .id("right-video")
                .presentation(second.clone())
                .slots(
                    MediaPlayerSlots::new()
                        .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Right")),
                )
                .into();
            *mounted.borrow_mut() = vstack().child(left).child(right).into();
        });
        first
    });
    drain_deferred_view_remounts();
    first.set_layout(MediaLayoutMode::Wide);
    first.set_rotation(MediaRotation::Clockwise90);
    first.set_viewport_ratio(MediaAspectRatio::Square);
    drain_deferred_view_remounts();
    let mounted = tree.borrow();
    let left = &mounted.children[0];
    let right = &mounted.children[1];
    assert_eq!(
        slot(left, "media-sidebar").unwrap().style.display,
        taffy::style::Display::None
    );
    assert_ne!(
        slot(right, "media-sidebar").unwrap().style.display,
        taffy::style::Display::None
    );
    let left_view = slot(left, "media-viewport").unwrap();
    let right_view = slot(right, "media-viewport").unwrap();
    assert_ne!(left_view.template_id, right_view.template_id);
    assert_eq!(left_view.style.aspect_ratio, Some(1.0));
    assert_eq!(right_view.style.aspect_ratio, Some(16.0 / 9.0));
    assert_eq!(left_view.children[0].attrs["jadix_video_rotation"], "90");
    assert_eq!(right_view.children[0].attrs["jadix_video_rotation"], "0");
    let mut ids = std::collections::HashSet::new();
    crate::core::tree::walk_elements(&mounted, |element| {
        if let Some(id) = &element.template_id {
            if id.starts_with("__jadix_media-layout_") {
                assert!(ids.insert(id.clone()), "{id}");
            }
        }
    });
    drop(mounted);
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn primitive_rotation_and_picture_options_share_rsx_and_builder_contracts() {
    let (_owner, ()) = create_root(|| {
        let upper = ui! { <Video content_ratio="4:3" rotation="90" mirror_horizontal=true mirror_vertical=true fit="cover" /> };
        let lower = ui! { <video content_ratio="4:3" rotation="90" mirror_horizontal=true mirror_vertical=true fit="cover" /> };
        let builder: Element = video()
            .content_ratio("4:3")
            .rotation("90")
            .mirror_horizontal(true)
            .mirror_vertical(true)
            .fit("cover")
            .into();
        for key in [
            "jadix_video_content_ratio",
            "jadix_video_rotation",
            "jadix_video_mirror_x",
            "jadix_video_mirror_y",
            "jadix_video_fit",
        ] {
            assert_eq!(upper.attrs.get(key), builder.attrs.get(key));
            assert_eq!(lower.attrs.get(key), builder.attrs.get(key));
        }
        let (rect, uv) =
            crate::primitives::media::video_geometry(&upper.attrs, [0.0, 0.0, 320.0, 180.0]);
        assert_eq!(rect, [70.0, -70.0, 180.0, 320.0]);
        assert!((uv[2] + 0.421875).abs() < 0.001);
        assert_eq!(uv[3], -1.0);
    });
}

#[test]
fn synchronous_host_confirmation_and_failure_recovery_remain_authoritative() {
    let (_owner, ()) = create_root(|| {
        let binding: Rc<RefCell<Option<MediaPresentationController>>> = Rc::new(RefCell::new(None));
        let callback = binding.clone();
        let controller = MediaPresentationController::new(
            MediaPresentationCapabilities {
                fullscreen: true,
                picture_in_picture: false,
            },
            move |request| {
                let MediaHostAction::Fullscreen(fullscreen) = request.action else {
                    unreachable!()
                };
                callback.borrow().as_ref().unwrap().publish(
                    request.id,
                    Ok(MediaHostState {
                        fullscreen,
                        picture_in_picture: false,
                    }),
                );
                Ok(())
            },
        );
        *binding.borrow_mut() = Some(controller.clone());
        controller
            .request(MediaHostAction::Fullscreen(true))
            .unwrap();
        assert!(controller.current().host.fullscreen);
        assert!(controller.pending().peek().is_none());
        controller
            .request(MediaHostAction::Fullscreen(false))
            .unwrap();
        assert!(!controller.current().host.fullscreen);
        binding.borrow_mut().take();
    });
}

fn slot<'a>(root: &'a Element, name: &str) -> Option<&'a Element> {
    if root.attrs.get("data-slot").map(String::as_str) == Some(name) {
        return Some(root);
    }
    root.children.iter().find_map(|child| slot(child, name))
}

fn click(element: &Element) {
    element.on_click.as_ref().unwrap().borrow_mut()();
}

#[test]
fn host_requests_wait_for_confirmation_and_fence_late_replies() {
    let (_owner, ()) = create_root(|| {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let captured = requests.clone();
        let controller = MediaPresentationController::new(
            MediaPresentationCapabilities {
                fullscreen: true,
                picture_in_picture: true,
            },
            move |request| {
                captured.borrow_mut().push(request);
                Ok(())
            },
        );
        controller.set_layout(MediaLayoutMode::Wide);
        let id = controller
            .request(MediaHostAction::Fullscreen(true))
            .unwrap();
        assert!(!controller.current().host.fullscreen);
        assert!(controller.pending().peek().is_some());
        assert!(controller
            .request(MediaHostAction::PictureInPicture(true))
            .is_err());
        assert!(!controller.publish(
            id + 1,
            Ok(MediaHostState {
                fullscreen: true,
                picture_in_picture: false
            })
        ));
        assert!(controller.publish(
            id,
            Ok(MediaHostState {
                fullscreen: true,
                picture_in_picture: false
            })
        ));
        assert!(controller.current().expanded());
        assert!(!controller.can_request(MediaHostAction::PictureInPicture(true)));
        controller.set_capabilities(MediaPresentationCapabilities::default());
        let exit = controller
            .request(MediaHostAction::Fullscreen(false))
            .unwrap();
        assert!(controller.current().host.fullscreen);
        assert!(controller.observe_host(MediaHostState::default()));
        assert!(!controller.publish(
            exit,
            Ok(MediaHostState {
                fullscreen: true,
                picture_in_picture: false
            })
        ));
        assert_eq!(controller.current().layout, MediaLayoutMode::Wide);
        assert_eq!(requests.borrow().len(), 2);
    });
}

#[test]
fn rejection_mismatch_and_os_exit_do_not_invent_host_state() {
    let (_owner, ()) = create_root(|| {
        let caps = MediaPresentationCapabilities {
            fullscreen: true,
            picture_in_picture: true,
        };
        let rejected = MediaPresentationController::new(caps, |_| {
            Err(MediaError::unsupported("window unavailable"))
        });
        assert!(rejected.request(MediaHostAction::Fullscreen(true)).is_err());
        assert!(rejected.pending().peek().is_none());
        assert!(!rejected.current().host.fullscreen);
        let controller = MediaPresentationController::new(caps, |_| Ok(()));
        let id = controller
            .request(MediaHostAction::Fullscreen(true))
            .unwrap();
        assert!(controller.publish(id, Ok(MediaHostState::default())));
        assert!(!controller.current().host.fullscreen);
        assert!(controller.request_error().peek().is_some());
        let pip = controller
            .request(MediaHostAction::PictureInPicture(true))
            .unwrap();
        assert!(controller.publish(
            pip,
            Ok(MediaHostState {
                fullscreen: false,
                picture_in_picture: true
            })
        ));
        assert!(!controller.can_request(MediaHostAction::Fullscreen(true)));
        assert!(!controller.observe_host(MediaHostState {
            fullscreen: true,
            picture_in_picture: true
        }));
        assert!(controller.observe_host(MediaHostState::default()));
        assert!(controller.can_request(MediaHostAction::Fullscreen(true)));
    });
}

#[test]
fn presentation_controls_support_rsx_builder_and_audio_disabling() {
    let (_owner, ()) = create_root(|| {
        let presentation = MediaPresentationController::default();
        let player: Element = video_player().presentation(presentation.clone()).child(|| ui! {
            <MediaControls><MediaWideButton /><MediaWindowFullscreenButton /><MediaFullscreenButton /><MediaPictureInPictureButton /><MediaMirrorButton /><MediaRotateButton /><MediaAspectRatioButton content=true /><MediaFitButton /></MediaControls>
        }).into();
        for name in ["media-fullscreen", "media-pip"] {
            assert!(slot(&player, name).unwrap().attrs.contains_key("disabled"));
        }
        click(slot(&player, "media-wide").unwrap());
        assert_eq!(presentation.current().layout, MediaLayoutMode::Wide);
        click(slot(&player, "media-window-fullscreen").unwrap());
        assert!(presentation.current().expanded());
        click(slot(&player, "media-window-fullscreen").unwrap());
        assert_eq!(presentation.current().layout, MediaLayoutMode::Wide);
        click(slot(&player, "media-mirror").unwrap());
        click(slot(&player, "media-rotate").unwrap());
        click(slot(&player, "media-aspect-ratio").unwrap());
        assert!(presentation.current().mirror_horizontal);
        assert_eq!(presentation.current().rotation, MediaRotation::Clockwise90);
        assert_eq!(
            presentation.current().content_ratio,
            MediaAspectRatio::Widescreen
        );
        let audio: Element = audio_player()
            .presentation(presentation.clone())
            .child(|| media_rotate_button().into())
            .into();
        assert!(slot(&audio, "media-rotate")
            .unwrap()
            .attrs
            .contains_key("disabled"));
        click(slot(&audio, "media-rotate").unwrap());
        assert_eq!(presentation.current().rotation, MediaRotation::Clockwise90);
    });
}

#[test]
fn picture_geometry_changes_only_video_and_accounts_for_rotation() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                state: PlaybackState::Ready,
                video_texture: Some("rotated-frame".into()),
                video_size: Some(MediaVideoSize {
                    width: 160,
                    height: 90,
                }),
                ..Default::default()
            }),
            |_| Ok(()),
        );
        let presentation = MediaPresentationController::default();
        presentation.set_rotation(MediaRotation::Clockwise90);
        presentation.set_mirror(true, false);
        presentation.set_viewport_ratio(MediaAspectRatio::Native);
        presentation.set_content_ratio(MediaAspectRatio::Square);
        let mut player = ui! { <VideoPlayer controller={controller} presentation={presentation}><MediaViewport><Label>"Overlay"</Label></MediaViewport></VideoPlayer> };
        let mut layout = crate::core::tree::LayoutTree::new();
        let draw = layout.build_and_layout(&mut player, 360.0, 640.0, [-1.0; 2]);
        let picture = draw
            .textured_draws
            .iter()
            .find(|draw| draw.texture_key == "rotated-frame")
            .unwrap();
        assert!((picture.rect[2] - picture.rect[3]).abs() < 0.01);
        assert_eq!(picture.uv_scale[0], -1.0);
        assert!((picture.transform[1] - 1.0).abs() < 0.001);
        let viewport = slot(&player, "media-viewport").unwrap();
        assert!((viewport.style.aspect_ratio.unwrap() - 9.0 / 16.0).abs() < 0.001);
        assert_eq!(viewport.style.rotate_deg, 0.0);
        assert_eq!(viewport.children[1].style.rotate_deg, 0.0);
    });
}

#[test]
fn window_fullscreen_moves_original_content_and_restores_layout_and_focus() {
    let (_owner, ()) = create_root(|| {
        let presentation = MediaPresentationController::default();
        let factories = Rc::new(Cell::new(0));
        let count = factories.clone();
        let slots = MediaPlayerSlots::new().replace(MediaPlayerSlot::Header, move || {
            count.set(count.get() + 1);
            Element::new().text("Retained header")
        });
        let player: Element = video_player()
            .id("retained-video")
            .presentation(presentation.clone())
            .slots(slots)
            .style(|base| base.w(320.0))
            .into();
        let original = slot(&player, "media-play")
            .unwrap()
            .on_click
            .as_ref()
            .unwrap()
            .clone();
        let original_focus = slot(&player, "media-window-fullscreen")
            .unwrap()
            .focus_id
            .clone()
            .unwrap();
        crate::utils::roving::focus_id_only(original_focus.clone());
        let mut root: Element = vstack().child(player).into();
        let mut layout = crate::core::tree::LayoutTree::new();
        layout.build_and_layout(&mut root, 900.0, 600.0, [-1.0; 2]);
        presentation.set_layout(MediaLayoutMode::WindowFullscreen);
        assert!(sync_media_presentation_subtree(&mut root));
        layout.build_and_layout(&mut root, 900.0, 600.0, [-1.0; 2]);
        let mut panel_bounds = None;
        crate::core::tree::walk_elements(&root, |element| {
            if element.attrs.contains_key("data-media-expanded") {
                panel_bounds = Some(element.layout_bounds);
            }
        });
        let bounds = panel_bounds.unwrap();
        assert!((bounds[2] - 900.0).abs() < 0.1, "{bounds:?}");
        assert!((bounds[3] - 600.0).abs() < 0.1, "{bounds:?}");
        assert!(crate::app::portal_overlay::try_close_top());
        assert!(sync_media_presentation_subtree(&mut root));
        layout.build_and_layout(&mut root, 900.0, 600.0, [-1.0; 2]);
        assert_eq!(presentation.current().layout, MediaLayoutMode::Inline);
        assert_eq!(factories.get(), 1);
        let restored = slot(&root, "media-play")
            .unwrap()
            .on_click
            .as_ref()
            .unwrap();
        assert!(Rc::ptr_eq(&original, restored));
        assert_eq!(
            crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).peek(),
            original_focus
        );
        assert!((slot(&root, "media-player").unwrap().layout_bounds[2] - 320.0).abs() < 0.1);
    });
}

#[test]
fn mounted_presentation_updates_sidebar_picture_and_accessibility_without_commands() {
    use crate::app::{
        reactive::{drain_deferred_view_remounts, reset_registered_anchors},
        runtime::{clear_test_reconcile_root, install_test_reconcile_root},
    };
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let (_owner, (presentation, commands)) = create_root(|| {
        let presentation =
            MediaPresentationController::new(MediaPresentationCapabilities::default(), |_| Ok(()));
        let commands = Rc::new(Cell::new(0));
        let count = commands.clone();
        let controller = MediaController::new(Signal::new(MediaSnapshot::default()), move |_| {
            count.set(count.get() + 1);
            Ok(())
        });
        let mounted = tree.clone();
        let view = presentation.clone();
        create_effect(move || {
            *mounted.borrow_mut() = video_player()
                .presentation(view.clone())
                .controller(controller.clone())
                .slots(
                    MediaPlayerSlots::new()
                        .replace(MediaPlayerSlot::Sidebar, || Element::new().text("Sidebar"))
                        .style(MediaPlayerSlot::Viewport, |slot| {
                            slot.class("hover:bg-card")
                        })
                        .style(MediaPlayerSlot::Sidebar, |slot| slot.class("hover:bg-card")),
                )
                .into();
        });
        (presentation, commands)
    });
    drain_deferred_view_remounts();
    presentation.set_layout(MediaLayoutMode::Wide);
    presentation.set_mirror(true, true);
    presentation.set_rotation(MediaRotation::Clockwise270);
    presentation.set_fit(MediaFit::Cover);
    drain_deferred_view_remounts();
    let mounted = tree.borrow();
    assert_eq!(
        slot(&mounted, "media-sidebar").unwrap().style.display,
        taffy::style::Display::None
    );
    assert_eq!(
        slot(&mounted, "media-wide").unwrap().attrs["aria-pressed"],
        "true"
    );
    let viewport = slot(&mounted, "media-viewport").unwrap();
    assert_eq!(viewport.children[0].attrs["jadix_video_mirror_x"], "true");
    assert_eq!(viewport.children[0].attrs["jadix_video_rotation"], "270");
    assert_eq!(viewport.children[0].attrs["jadix_video_fit"], "cover");
    assert_eq!(commands.get(), 0);
    drop(mounted);
    presentation.set_capabilities(MediaPresentationCapabilities {
        fullscreen: true,
        picture_in_picture: true,
    });
    drain_deferred_view_remounts();
    let fullscreen_click = slot(&tree.borrow(), "media-fullscreen")
        .unwrap()
        .on_click
        .as_ref()
        .unwrap()
        .clone();
    fullscreen_click.borrow_mut()();
    drain_deferred_view_remounts();
    assert_eq!(
        slot(&tree.borrow(), "media-fullscreen").unwrap().attrs["aria-pressed"],
        "false"
    );
    assert_eq!(
        slot(&tree.borrow(), "media-fullscreen").unwrap().attrs["aria-busy"],
        "true"
    );
    let request = presentation.pending().peek().unwrap();
    presentation.publish(
        request.id,
        Ok(MediaHostState {
            fullscreen: true,
            picture_in_picture: false,
        }),
    );
    drain_deferred_view_remounts();
    let mut layout = crate::core::tree::LayoutTree::new();
    {
        let mut mounted = tree.borrow_mut();
        assert!(sync_media_presentation_subtree(&mut mounted));
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut mounted,
            [-1.0; 2],
            "",
            false,
            900.0,
            600.0,
            &mut layout,
            &mut impact,
        );
        layout.build_and_layout(&mut mounted, 900.0, 600.0, [-1.0; 2]);
        let viewport = slot(&mounted, "media-viewport").unwrap();
        assert!(viewport.style.aspect_ratio.is_none());
        assert!(
            viewport.layout_bounds[2] > 800.0,
            "{:?}",
            viewport.layout_bounds
        );
        assert!(
            viewport.layout_bounds[3] > 100.0 && viewport.layout_bounds[3] < 600.0,
            "{:?}",
            viewport.layout_bounds
        );
        assert_eq!(
            slot(&mounted, "media-fullscreen").unwrap().attrs["aria-pressed"],
            "true"
        );
    }
    assert!(crate::app::portal_overlay::try_close_top());
    sync_media_presentation_subtree(&mut tree.borrow_mut());
    assert!(presentation.current().host.fullscreen);
    let exit = presentation.pending().peek().unwrap();
    assert_eq!(exit.action, MediaHostAction::Fullscreen(false));
    presentation.publish(exit.id, Ok(MediaHostState::default()));
    drain_deferred_view_remounts();
    assert!(sync_media_presentation_subtree(&mut tree.borrow_mut()));
    assert_eq!(presentation.current().layout, MediaLayoutMode::Wide);
    assert_eq!(commands.get(), 0);
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn disposed_presentation_owner_rejects_callbacks() {
    let (owner, controller) = create_root(|| {
        MediaPresentationController::new(
            MediaPresentationCapabilities {
                fullscreen: true,
                picture_in_picture: true,
            },
            |_| Ok(()),
        )
    });
    let id = controller
        .request(MediaHostAction::Fullscreen(true))
        .unwrap();
    drop(owner);
    assert!(!controller.publish(
        id,
        Ok(MediaHostState {
            fullscreen: true,
            picture_in_picture: false
        })
    ));
    assert!(!controller.observe_host(MediaHostState::default()));
    assert!(controller
        .request(MediaHostAction::Fullscreen(true))
        .is_err());
}
