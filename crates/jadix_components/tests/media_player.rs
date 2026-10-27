use jadix_components::{core::reactivity::create_root, prelude::*};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn ready() -> MediaSnapshot {
    MediaSnapshot {
        state: PlaybackState::Paused,
        duration: Some(Duration::from_secs(180)),
        position: Duration::from_secs(30),
        capabilities: MediaCapabilities {
            audio: true,
            video: true,
            play: true,
            pause: true,
            stop: true,
            seek: true,
            volume: true,
            mute: true,
            playback_rate: true,
            looping: true,
        },
        metadata: MediaMetadata {
            title: "A track".into(),
            artist: "An artist".into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn slot<'a>(root: &'a Element, name: &str) -> Option<&'a Element> {
    if root.attrs.get("data-slot").map(String::as_str) == Some(name) {
        return Some(root);
    }
    root.children.iter().find_map(|child| slot(child, name))
}

fn click(element: &Element) {
    element
        .on_click
        .as_ref()
        .expect("click handler")
        .borrow_mut()();
}

#[test]
fn media_requests_never_invent_playback_state() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = commands.clone();
        let controller = MediaController::new(Signal::new(ready()), move |command| {
            captured.borrow_mut().push(command);
            Ok(())
        });
        controller.toggle_playback().unwrap();
        assert_eq!(commands.borrow().as_slice(), &[MediaCommand::Play]);
        assert_eq!(controller.current().state, PlaybackState::Paused);
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::StateChanged {
                state: PlaybackState::Playing,
                play_when_ready: true,
            },
        ));
        controller.toggle_playback().unwrap();
        assert_eq!(commands.borrow().last(), Some(&MediaCommand::Pause));
        assert_eq!(controller.current().state, PlaybackState::Playing);
    });
}

#[test]
fn primitives_support_upper_lower_and_builder_surfaces() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                video_texture: Some("media-frame".into()),
                ..ready()
            }),
            |_| Ok(()),
        );
        let upper = ui! { <Video controller={controller.clone()} w="320px" h="180px" /> };
        let lower = ui! { <video controller={controller.clone()} fit="cover" /> };
        let built: Element = video().controller(controller.clone()).fit("cover").into();
        assert_eq!(upper.image_texture.as_deref(), Some("media-frame"));
        assert_eq!(lower.image_texture, built.image_texture);
        assert_eq!(
            lower.attrs.get("jadix_video_fit"),
            built.attrs.get("jadix_video_fit")
        );
        let upper_audio = ui! { <Audio controller={controller.clone()} /> };
        let lower_audio = ui! { <audio controller={controller.clone()} /> };
        let built_audio: Element = audio().controller(controller).into();
        for endpoint in [&upper_audio, &lower_audio, &built_audio] {
            assert!(endpoint.image_texture.is_none());
            assert!(endpoint.children.is_empty());
            assert_eq!(
                endpoint.attrs.get("data-slot").map(String::as_str),
                Some("audio")
            );
        }
        assert_ne!(upper_audio.template_id, lower_audio.template_id);
    });
}

#[test]
fn compound_rsx_children_are_built_inside_the_controller_scope() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = commands.clone();
        let controller = MediaController::new(Signal::new(ready()), move |command| {
            captured.borrow_mut().push(command);
            Ok(())
        });
        let from_rsx = ui! {
            <MediaPlayer id="rsx-player" controller={controller.clone()} class="p-2">
                <MediaTitle />
                <MediaControls><MediaPlayButton /><MediaTimeDisplay /><MediaVolumeSlider /></MediaControls>
            </MediaPlayer>
        };
        let from_builder: Element = media_player()
            .id("builder-player")
            .controller(controller)
            .child(|| {
                hstack()
                    .child(media_title())
                    .child(media_play_button())
                    .child(media_time_display())
                    .into()
            })
            .into();
        click(slot(&from_rsx, "media-play").unwrap());
        click(slot(&from_builder, "media-play").unwrap());
        assert_eq!(
            commands.borrow().as_slice(),
            &[MediaCommand::Play, MediaCommand::Play]
        );
        assert_eq!(
            slot(&from_rsx, "media-title").unwrap().text.as_deref(),
            Some("A track")
        );
        assert_eq!(
            slot(&from_rsx, "media-time").unwrap().text.as_deref(),
            Some("0:30 / 3:00")
        );
    });
}

#[test]
fn default_players_and_custom_audio_video_roots_share_parts() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(Signal::new(ready()), |_| Ok(()));
        let default_video = ui! { <VideoPlayer controller={controller.clone()} /> };
        let default_audio: Element = audio_player().controller(controller.clone()).into();
        let custom_video = ui! { <VideoPlayer controller={controller.clone()}><MediaViewport /><MediaPlayButton /></VideoPlayer> };
        let custom_audio =
            ui! { <AudioPlayer controller={controller}><MediaPlayButton /></AudioPlayer> };
        assert!(slot(&default_video, "media-viewport").is_some());
        assert!(slot(&default_audio, "audio").is_some());
        assert!(slot(&custom_video, "media-play").is_some());
        assert!(slot(&custom_audio, "media-play").is_some());
        for player in [&default_audio, &default_video] {
            for name in [
                "media-seek-slider",
                "media-volume",
                "media-mute",
                "media-rate",
                "media-loop",
                "media-status",
                "media-error",
            ] {
                assert!(slot(player, name).is_some(), "missing {name}");
            }
        }
    });
}

#[test]
fn default_speed_choices_can_restore_normal_rate_without_assuming_success() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = commands.clone();
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                playback_rate: 1.5,
                ..ready()
            }),
            move |command| {
                captured.borrow_mut().push(command);
                Ok(())
            },
        );
        let root: Element = audio_player().controller(controller.clone()).into();
        fn rates<'a>(element: &'a Element, found: &mut Vec<&'a Element>) {
            if element.attrs.get("data-slot").map(String::as_str) == Some("media-rate") {
                found.push(element);
            }
            for child in &element.children {
                rates(child, found);
            }
        }
        let mut choices = Vec::new();
        rates(&root, &mut choices);
        assert_eq!(choices.len(), 2);
        assert_eq!(
            choices[0].attrs.get("aria-pressed").map(String::as_str),
            Some("false")
        );
        assert_eq!(
            choices[1].attrs.get("aria-pressed").map(String::as_str),
            Some("true")
        );
        click(choices[0]);
        assert_eq!(
            commands.borrow().as_slice(),
            &[MediaCommand::SetPlaybackRate(1.0)]
        );
        assert_eq!(controller.current().playback_rate, 1.5);
    });
}

#[test]
fn sibling_and_nested_players_keep_independent_scopes() {
    let (_owner, ()) = create_root(|| {
        let first_commands = Rc::new(RefCell::new(Vec::new()));
        let second_commands = Rc::new(RefCell::new(Vec::new()));
        let first_sink = first_commands.clone();
        let second_sink = second_commands.clone();
        let first = MediaController::new(Signal::new(ready()), move |command| {
            first_sink.borrow_mut().push(command);
            Ok(())
        });
        let second = MediaController::new(Signal::new(ready()), move |command| {
            second_sink.borrow_mut().push(command);
            Ok(())
        });
        let nested: Element = media_player()
            .controller(first.clone())
            .child(move || {
                vstack()
                    .child(
                        media_player()
                            .controller(second)
                            .child(|| media_play_button().into()),
                    )
                    .child(media_play_button())
                    .into()
            })
            .into();
        fn collect_play_buttons<'a>(element: &'a Element, buttons: &mut Vec<&'a Element>) {
            if element.attrs.get("data-slot").map(String::as_str) == Some("media-play") {
                buttons.push(element);
            }
            for child in &element.children {
                collect_play_buttons(child, buttons);
            }
        }
        let mut buttons = Vec::new();
        collect_play_buttons(&nested, &mut buttons);
        assert_eq!(buttons.len(), 2);
        for button in buttons {
            click(button);
        }
        assert_eq!(first_commands.borrow().as_slice(), &[MediaCommand::Play]);
        assert_eq!(second_commands.borrow().as_slice(), &[MediaCommand::Play]);
    });
}

#[test]
fn unsupported_disabled_and_rejected_commands_do_not_mutate_state() {
    let (_owner, ()) = create_root(|| {
        let calls = Rc::new(RefCell::new(0));
        let captured = calls.clone();
        let controller = MediaController::new(Signal::new(MediaSnapshot::default()), move |_| {
            *captured.borrow_mut() += 1;
            Ok(())
        });
        let tree: Element = audio_player().controller(controller.clone()).into();
        let play = slot(&tree, "media-play").unwrap();
        assert_eq!(play.attrs.get("disabled").map(String::as_str), Some("true"));
        click(play);
        assert_eq!(*calls.borrow(), 0);
        controller.publish(MediaEvent::new(0, MediaEventKind::Snapshot(ready())));
        let disabled: Element = audio_player().controller(controller).disabled(true).into();
        click(slot(&disabled, "media-play").unwrap());
        assert_eq!(*calls.borrow(), 0);
        let rejected = MediaController::new(Signal::new(ready()), |_| {
            Err(MediaError::new(MediaErrorKind::Backend, "device busy"))
        });
        assert!(rejected.toggle_playback().is_err());
        assert_eq!(rejected.current().state, PlaybackState::Paused);
        let tree: Element = audio_player().controller(rejected).into();
        assert_eq!(
            slot(&tree, "media-error").unwrap().text.as_deref(),
            Some("device busy")
        );
    });
}

#[test]
fn unknown_duration_disables_seek_and_large_relative_seeks_clamp() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = commands.clone();
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                duration: None,
                ..ready()
            }),
            move |command| {
                captured.borrow_mut().push(command);
                Ok(())
            },
        );
        let tree: Element = audio_player().controller(controller.clone()).into();
        assert_eq!(
            slot(&tree, "media-seek-slider")
                .unwrap()
                .attrs
                .get("disabled")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            slot(&tree, "media-time").unwrap().text.as_deref(),
            Some("0:30 / --:--")
        );
        controller.publish(MediaEvent::new(
            0,
            MediaEventKind::SeekableChanged(Some(
                MediaTimeRange::new(Duration::from_secs(20), Duration::from_secs(60)).unwrap(),
            )),
        ));
        controller.seek_by(f64::MAX).unwrap();
        controller.seek_by(-f64::MAX).unwrap();
        assert_eq!(
            commands.borrow().as_slice(),
            &[
                MediaCommand::Seek(Duration::from_secs(60)),
                MediaCommand::Seek(Duration::from_secs(20))
            ]
        );
        assert!(controller.seek_by(f64::NAN).is_err());
    });
}

#[test]
fn disposed_controllers_stop_dispatching() {
    let (owner, controller) = create_root(|| {
        MediaController::new(Signal::new(ready()), |_| {
            panic!("disposed sink must not run")
        })
    });
    drop(owner);
    assert_eq!(
        controller.request(MediaCommand::Play).unwrap_err().kind,
        MediaErrorKind::Disconnected
    );
    assert!(!controller.publish(MediaEvent::new(
        0,
        MediaEventKind::PositionChanged(Duration::ZERO)
    )));
}

#[test]
fn time_format_supports_long_media() {
    assert_eq!(format_media_time(Duration::ZERO), "0:00");
    assert_eq!(format_media_time(Duration::from_secs(3661)), "1:01:01");
    assert_eq!(format_media_time(Duration::from_secs(360000)), "100:00:00");
}

#[test]
fn player_controls_wrap_inside_narrow_layouts() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(Signal::new(ready()), |_| Ok(()));
        for width in [280.0, 480.0, 960.0] {
            let mut root: Element = video_player().controller(controller.clone()).into();
            let mut layout = jadix_components::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut root, width, 1000.0, [-1.0, -1.0]);
            fn check_bounds(element: &Element, width: f32) {
                for value in element.layout_bounds {
                    assert!(value.is_finite());
                }
                let [left, _, child_width, _] = element.layout_bounds;
                assert!(
                    left + child_width <= width + 1.0,
                    "overflow at {:?}: {:?}",
                    element.attrs.get("data-slot"),
                    element.layout_bounds
                );
                for child in &element.children {
                    check_bounds(child, width);
                }
            }
            check_bounds(&root, width);
        }
    });
}

#[test]
fn video_layout_preserves_aspect_and_cover_uv() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                video_texture: Some("fit-frame".into()),
                video_size: Some(MediaVideoSize {
                    width: 160,
                    height: 90,
                }),
                ..ready()
            }),
            |_| Ok(()),
        );
        let mut contain: Element = video()
            .controller(controller.clone())
            .fit("contain")
            .style(|base| base.w(200.0).h(200.0))
            .into();
        let mut tree = jadix_components::core::tree::LayoutTree::new();
        let result = tree.build_and_layout(&mut contain, 200.0, 200.0, [-1.0, -1.0]);
        let draw = result
            .textured_draws
            .iter()
            .find(|draw| draw.texture_key == "fit-frame")
            .unwrap();
        assert!((draw.rect[2] - 200.0).abs() < 0.01);
        assert!((draw.rect[3] - 112.5).abs() < 0.01);
        assert!((draw.rect[1] - 43.75).abs() < 0.01);
        let mut cover: Element = video()
            .controller(controller)
            .fit("cover")
            .style(|base| base.w(200.0).h(200.0))
            .into();
        let result = tree.build_and_layout(&mut cover, 200.0, 200.0, [-1.0, -1.0]);
        let draw = result
            .textured_draws
            .iter()
            .find(|draw| draw.texture_key == "fit-frame")
            .unwrap();
        assert!((draw.rect[3] - 200.0).abs() < 0.01);
        assert!((draw.uv_scale[0] - 0.5625).abs() < 0.01);
        assert!((draw.uv_offset[0] - 0.21875).abs() < 0.01);
    });
}

#[cfg(feature = "a11y")]
#[test]
fn media_accessibility_exports_slider_values_and_toggle_state() {
    let (_owner, ()) = create_root(|| {
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                volume: 0.7,
                muted: true,
                ..ready()
            }),
            |_| Ok(()),
        );
        let tree: Element = audio_player().controller(controller).into();
        let update = jadix_components::a11y::tree_update_from_elements(&tree, None);
        let volume = update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| node.value() == Some("70%"))
            .expect("volume slider value");
        assert_eq!(volume.numeric_value(), Some(70.0));
        assert_eq!(volume.min_numeric_value(), Some(0.0));
        assert_eq!(volume.max_numeric_value(), Some(100.0));
        assert!(update
            .nodes
            .iter()
            .any(|(_, node)| node.toggled().is_some()));
    });
}
