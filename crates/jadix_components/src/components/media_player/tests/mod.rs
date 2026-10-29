use super::*;
use crate::{
    app::pointer_events::PointerEvent, app::test_support::sync_reactive_elements_subtree,
    core::reactivity::create_root,
};
use std::cell::RefCell;

#[test]
fn mounted_choices_follow_external_confirmation_and_disable_after_source_changes() {
    use crate::app::{
        reactive::{drain_deferred_view_remounts, reset_registered_anchors},
        runtime::{clear_test_reconcile_root, install_test_reconcile_root},
    };
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let (_owner, (controller, state)) = create_root(|| {
        let controller = MediaController::new(Signal::new(MediaSnapshot::default()), |_| Ok(()));
        let state = Signal::new(MediaChoiceState::new(0));
        let mounted = tree.clone();
        let player_controller = controller.clone();
        create_effect(move || {
            *mounted.borrow_mut() = media_player_scope(
                "confirmed-choice",
                player_controller.clone(),
                MediaKind::Video,
                false,
                || media_choice("1080p", state).on_select(|_| {}).into(),
            );
        });
        (controller, state)
    });
    drain_deferred_view_remounts();
    assert_eq!(
        find_slot(&tree.borrow(), "media-choice").unwrap().attrs["aria-pressed"],
        "false"
    );
    state.update(|state| state.selected = Some("1080p".into()));
    drain_deferred_view_remounts();
    assert_eq!(
        find_slot(&tree.borrow(), "media-choice").unwrap().attrs["aria-pressed"],
        "true"
    );
    let session = controller
        .load("next", crate::media::MediaLoadOptions::default())
        .unwrap();
    drain_deferred_view_remounts();
    {
        let root = tree.borrow();
        let choice = find_slot(&root, "media-choice").unwrap();
        assert_eq!(choice.attrs["aria-pressed"], "false");
        assert!(choice.attrs.contains_key("disabled"));
        assert!(!choice.focusable);
    }
    state.set(MediaChoiceState {
        selected: Some("1080p".into()),
        ..MediaChoiceState::new(session)
    });
    drain_deferred_view_remounts();
    assert!(find_slot(&tree.borrow(), "media-choice").unwrap().focusable);
    assert_eq!(
        find_slot(&tree.borrow(), "media-choice").unwrap().attrs["aria-pressed"],
        "true"
    );
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn media_mounted_capabilities_and_video_detach_follow_session_events() {
    use crate::{
        app::{
            reactive::{drain_deferred_view_remounts, reset_registered_anchors},
            runtime::{clear_test_reconcile_root, install_test_reconcile_root},
        },
        media::{
            MediaCapabilities, MediaEvent, MediaEventKind, MediaLoadOptions, MediaTimeRange,
            MediaVideoSize,
        },
    };
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let (_owner, controller) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                state: PlaybackState::Playing,
                video_texture: Some("initial-frame".into()),
                video_size: Some(MediaVideoSize {
                    width: 160,
                    height: 90,
                }),
                ..Default::default()
            }),
            |_| Ok(()),
        );
        let mounted = tree.clone();
        let player_controller = controller.clone();
        create_effect(move || {
            let endpoint_controller = player_controller.clone();
            *mounted.borrow_mut() = media_player_scope(
                "media-live-update-test",
                player_controller.clone(),
                MediaKind::Video,
                false,
                || {
                    vstack()
                        .child(media_seek_slider())
                        .child(video().controller(endpoint_controller))
                        .into()
                },
            );
        });
        controller
    });
    drain_deferred_view_remounts();
    assert_eq!(
        find_slot(&tree.borrow(), "video")
            .unwrap()
            .image_texture
            .as_deref(),
        Some("initial-frame")
    );
    assert!(
        !find_slot(&tree.borrow(), "media-seek-slider")
            .unwrap()
            .focusable
    );
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::SeekableChanged(Some(
            MediaTimeRange::new(Duration::from_secs(300), Duration::from_secs(360)).unwrap(),
        )),
    ));
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::PositionChanged(Duration::from_secs(330)),
    ));
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::CapabilitiesChanged(MediaCapabilities {
            seek: true,
            video: true,
            ..Default::default()
        }),
    ));
    drain_deferred_view_remounts();
    {
        let root = tree.borrow();
        let seek = find_slot(&root, "media-seek-slider").unwrap();
        assert!(seek.focusable);
        assert!(!seek.attrs.contains_key("disabled"));
        assert_eq!(
            seek.attrs.get("aria-valuenow").map(String::as_str),
            Some("0.5")
        );
    }
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::CapabilitiesChanged(MediaCapabilities::default()),
    ));
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(None),
    ));
    drain_deferred_view_remounts();
    assert!(
        !find_slot(&tree.borrow(), "media-seek-slider")
            .unwrap()
            .focusable
    );
    assert!(find_slot(&tree.borrow(), "video")
        .unwrap()
        .image_texture
        .is_none());
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(Some("second-frame".into())),
    ));
    drain_deferred_view_remounts();
    assert_eq!(
        find_slot(&tree.borrow(), "video")
            .unwrap()
            .image_texture
            .as_deref(),
        Some("second-frame")
    );
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::StateChanged {
            state: PlaybackState::Idle,
            play_when_ready: false,
        },
    ));
    drain_deferred_view_remounts();
    assert!(find_slot(&tree.borrow(), "video")
        .unwrap()
        .image_texture
        .is_none());
    assert!(!find_slot(&tree.borrow(), "video")
        .unwrap()
        .attrs
        .contains_key("jadix_video_size"));
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::StateChanged {
            state: PlaybackState::Ready,
            play_when_ready: false,
        },
    ));
    controller.publish(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(Some("before-load".into())),
    ));
    let session = controller
        .load("next-source", MediaLoadOptions::default())
        .unwrap();
    assert!(!controller.publish(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(Some("obsolete-frame".into()))
    )));
    drain_deferred_view_remounts();
    assert!(find_slot(&tree.borrow(), "video")
        .unwrap()
        .image_texture
        .is_none());
    controller.publish(MediaEvent::new(
        session,
        MediaEventKind::StateChanged {
            state: PlaybackState::Ready,
            play_when_ready: false,
        },
    ));
    controller.publish(MediaEvent::new(
        session,
        MediaEventKind::VideoTextureChanged(Some("new-session-frame".into())),
    ));
    drain_deferred_view_remounts();
    assert_eq!(
        find_slot(&tree.borrow(), "video")
            .unwrap()
            .image_texture
            .as_deref(),
        Some("new-session-frame")
    );
    clear_test_reconcile_root();
    reset_registered_anchors();
}

fn find_slot<'a>(element: &'a Element, slot: &str) -> Option<&'a Element> {
    if element.attrs.get("data-slot").map(String::as_str) == Some(slot) {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_slot(child, slot))
}

fn prepare_bounds(element: &mut Element, bounds: [f32; 4]) {
    element.layout_bounds = bounds;
    for child in &mut element.children {
        prepare_bounds(child, bounds);
    }
}

#[test]
fn confirmed_rate_and_loop_updates_refresh_pressed_accessibility_state() {
    use crate::app::{
        reactive::{drain_deferred_view_remounts, reset_registered_anchors},
        runtime::{clear_test_reconcile_root, install_test_reconcile_root},
    };
    reset_registered_anchors();
    let tree = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(tree.clone());
    let (_owner, controller) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                capabilities: crate::media::MediaCapabilities {
                    playback_rate: true,
                    looping: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            |_| Ok(()),
        );
        let mounted = tree.clone();
        let player_controller = controller.clone();
        create_effect(move || {
            *mounted.borrow_mut() = media_player_scope(
                "media-toggles-test",
                player_controller.clone(),
                MediaKind::Audio,
                false,
                || {
                    hstack()
                        .child(media_rate_button(1.0))
                        .child(media_rate_button(1.5))
                        .child(media_loop_button())
                        .into()
                },
            );
        });
        controller
    });
    drain_deferred_view_remounts();
    controller.publish(crate::media::MediaEvent::new(
        0,
        crate::media::MediaEventKind::PlaybackRateChanged(1.5),
    ));
    controller.publish(crate::media::MediaEvent::new(
        0,
        crate::media::MediaEventKind::LoopChanged(true),
    ));
    drain_deferred_view_remounts();
    fn pressed_rates(element: &Element, values: &mut Vec<String>) {
        if element.attrs.get("data-slot").map(String::as_str) == Some("media-rate") {
            values.push(element.attrs["aria-pressed"].clone());
        }
        for child in &element.children {
            pressed_rates(child, values);
        }
    }
    let root = tree.borrow();
    let mut rates = Vec::new();
    pressed_rates(&root, &mut rates);
    assert_eq!(rates, ["false", "true"]);
    assert_eq!(
        find_slot(&root, "media-loop")
            .unwrap()
            .attrs
            .get("aria-pressed")
            .map(String::as_str),
        Some("true")
    );
    drop(root);
    clear_test_reconcile_root();
    reset_registered_anchors();
}

#[test]
fn media_slider_backend_updates_do_not_echo_seek_or_volume_commands() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let sink = commands.clone();
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                duration: Some(Duration::from_secs(100)),
                capabilities: crate::media::MediaCapabilities {
                    seek: true,
                    volume: true,
                    ..Default::default()
                },
                ..Default::default()
            }),
            move |command| {
                sink.borrow_mut().push(command);
                Ok(())
            },
        );
        let mut root = media_player_scope(
            "media-sync-test",
            controller.clone(),
            MediaKind::Audio,
            false,
            || {
                vstack()
                    .child(media_seek_slider())
                    .child(media_volume_slider())
                    .into()
            },
        );
        controller.publish(crate::media::MediaEvent::new(
            0,
            crate::media::MediaEventKind::PositionChanged(Duration::from_secs(75)),
        ));
        controller.publish(crate::media::MediaEvent::new(
            0,
            crate::media::MediaEventKind::VolumeChanged(0.25),
        ));
        let mut layout = crate::core::tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
        let seek = find_slot(&root, "media-seek-slider").unwrap();
        let volume = find_slot(&root, "media-volume").unwrap();
        assert!((find_slot(seek, "slider-before").unwrap().style.flex_grow - 0.75).abs() < 0.001);
        assert!((find_slot(volume, "slider-before").unwrap().style.flex_grow - 0.25).abs() < 0.001);
        assert!(commands.borrow().is_empty());
    });
}

#[test]
fn media_slider_pointer_uses_latest_capabilities_and_sends_one_command() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let sink = commands.clone();
        let mut capabilities = crate::media::MediaCapabilities::default();
        capabilities.seek = true;
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                duration: Some(Duration::from_secs(100)),
                capabilities,
                ..Default::default()
            }),
            move |command| {
                sink.borrow_mut().push(command);
                Ok(())
            },
        );
        let mut root = media_player_scope(
            "media-pointer-test",
            controller.clone(),
            MediaKind::Audio,
            false,
            media_seek_slider,
        );
        prepare_bounds(&mut root, [0.0, 0.0, 120.0, 20.0]);
        let mut layout = crate::core::tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_reactive_elements_subtree(&mut root, &mut layout, &mut impact);
        let slider = find_slot(&root, "media-seek-slider").unwrap();
        let event = PointerEvent::new(100.0, 10.0, winit::event::MouseButton::Left, 1);
        slider.on_mousedown.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(commands.borrow().len(), 1);
        assert!(
            matches!(commands.borrow().first(), Some(MediaCommand::Seek(position)) if position.as_secs() > 50)
        );
        capabilities.seek = false;
        controller.publish(crate::media::MediaEvent::new(
            0,
            crate::media::MediaEventKind::CapabilitiesChanged(capabilities),
        ));
        slider.on_mousedown.as_ref().unwrap().borrow_mut()(&event);
        assert_eq!(commands.borrow().len(), 1);
        if let Some(up) = &slider.on_mouseup {
            up.borrow_mut()(&event);
        }
        crate::app::release_pointer_capture();
    });
}
