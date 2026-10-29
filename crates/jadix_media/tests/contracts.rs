use jadix_media::*;
use std::time::Duration;

fn seconds(value: u64) -> Duration {
    Duration::from_secs(value)
}

#[test]
fn commands_require_capabilities_and_finite_values() {
    let mut snapshot = MediaSnapshot::default();
    assert_eq!(
        MediaCommand::Play.validate(&snapshot).unwrap_err().kind,
        MediaErrorKind::Unsupported
    );
    snapshot.capabilities.play = true;
    snapshot.capabilities.volume = true;
    snapshot.capabilities.playback_rate = true;
    assert!(MediaCommand::Play.validate(&snapshot).is_ok());
    for invalid in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert_eq!(
            MediaCommand::SetVolume(invalid)
                .validate(&snapshot)
                .unwrap_err()
                .kind,
            MediaErrorKind::InvalidInput
        );
    }
    for invalid in [f32::NAN, f32::INFINITY, 0.0, -1.0] {
        assert!(MediaCommand::SetPlaybackRate(invalid)
            .validate(&snapshot)
            .is_err());
    }
    assert!(MediaCommand::SetVolume(0.0).validate(&snapshot).is_ok());
    assert!(MediaCommand::SetVolume(1.0).validate(&snapshot).is_ok());
}

#[test]
fn vod_live_and_empty_seek_windows_have_explicit_semantics() {
    let mut snapshot = MediaSnapshot::default();
    snapshot.capabilities.seek = true;
    assert!(snapshot.seek_window().is_none());
    snapshot.duration = Some(seconds(120));
    assert!(MediaCommand::Seek(seconds(120)).validate(&snapshot).is_ok());
    assert_eq!(
        MediaCommand::Seek(seconds(121))
            .validate(&snapshot)
            .unwrap_err()
            .kind,
        MediaErrorKind::InvalidInput
    );
    snapshot.duration = None;
    snapshot.seekable = Some(MediaTimeRange::new(seconds(300), seconds(360)).unwrap());
    assert!(MediaCommand::Seek(seconds(330)).validate(&snapshot).is_ok());
    assert!(MediaCommand::Seek(seconds(299))
        .validate(&snapshot)
        .is_err());
    snapshot.seekable = Some(MediaTimeRange::new(seconds(300), seconds(300)).unwrap());
    assert!(snapshot.seek_window().is_none());
    assert!(MediaTimeRange::new(seconds(2), seconds(1)).is_err());
}

#[test]
fn observations_normalize_data_and_recover_from_errors() {
    let mut snapshot = MediaSnapshot::default();
    snapshot.apply(MediaEvent::new(0, MediaEventKind::VolumeChanged(f32::NAN)));
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::PlaybackRateChanged(f32::INFINITY),
    ));
    assert_eq!(snapshot.volume, 1.0);
    assert_eq!(snapshot.playback_rate, 1.0);
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::PositionChanged(seconds(90)),
    ));
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::DurationChanged(Some(seconds(30))),
    ));
    assert_eq!(snapshot.position, seconds(30));
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::Error(MediaError::new(
            MediaErrorKind::Backend,
            "device unavailable",
        )),
    ));
    assert_eq!(snapshot.state, PlaybackState::Error);
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::StateChanged {
            state: PlaybackState::Ready,
            play_when_ready: false,
        },
    ));
    assert!(snapshot.error.is_none());
}

#[test]
fn buffering_does_not_invent_playback_intent() {
    let mut snapshot = MediaSnapshot {
        state: PlaybackState::Buffering,
        ..Default::default()
    };
    assert!(!snapshot.wants_playback());
    snapshot.play_when_ready = true;
    assert!(snapshot.wants_playback());
    snapshot.state = PlaybackState::Paused;
    assert!(!snapshot.wants_playback());
}

#[test]
fn load_is_validated_without_opening_any_resource() {
    let snapshot = MediaSnapshot::default();
    let load = |source: &str, options| MediaCommand::Load {
        load_session: 1,
        source: source.into(),
        options,
    };
    assert!(load("file:///music.ogg", MediaLoadOptions::default())
        .validate(&snapshot)
        .is_ok());
    assert!(load(" ", MediaLoadOptions::default())
        .validate(&snapshot)
        .is_err());
    assert!(load(
        "asset:track",
        MediaLoadOptions {
            volume: f32::NAN,
            ..Default::default()
        }
    )
    .validate(&snapshot)
    .is_err());
}

#[test]
fn new_load_resets_source_observations_and_preserves_confirmed_audio_settings() {
    let mut snapshot = MediaSnapshot {
        state: PlaybackState::Playing,
        duration: Some(seconds(120)),
        position: seconds(40),
        buffered: vec![MediaTimeRange::new(seconds(0), seconds(60)).unwrap()],
        seekable: Some(MediaTimeRange::new(seconds(0), seconds(120)).unwrap()),
        metadata: MediaMetadata {
            title: "old title".into(),
            ..Default::default()
        },
        video_texture: Some("old-frame".into()),
        video_size: Some(MediaVideoSize {
            width: 1920,
            height: 1080,
        }),
        error: Some(MediaError::new(MediaErrorKind::Backend, "old failure")),
        volume: 0.25,
        muted: true,
        playback_rate: 1.5,
        ..Default::default()
    };
    snapshot.capabilities.seek = true;
    assert!(snapshot.begin_load(1, "https://media/b".into()));
    assert_eq!(snapshot.load_session, 1);
    assert_eq!(snapshot.source, Some("https://media/b".into()));
    assert_eq!(snapshot.state, PlaybackState::Idle);
    assert_eq!(snapshot.position, Duration::ZERO);
    assert!(snapshot.duration.is_none());
    assert!(snapshot.buffered.is_empty());
    assert!(snapshot.seekable.is_none());
    assert_eq!(snapshot.metadata, MediaMetadata::default());
    assert_eq!(snapshot.capabilities, MediaCapabilities::default());
    assert!(snapshot.video_texture.is_none());
    assert!(snapshot.video_size.is_none());
    assert!(snapshot.error.is_none());
    assert_eq!(
        (snapshot.volume, snapshot.muted, snapshot.playback_rate),
        (0.25, true, 1.5)
    );
    let expected = snapshot.clone();
    assert!(!snapshot.begin_load(1, "reused".into()));
    assert!(!snapshot.begin_load(0, "older".into()));
    assert_eq!(snapshot, expected);
}

#[test]
fn stale_future_and_mismatched_snapshot_events_cannot_mutate_the_active_load() {
    let mut snapshot = MediaSnapshot {
        load_session: 2,
        state: PlaybackState::Playing,
        duration: Some(seconds(30)),
        video_texture: Some("b-frame".into()),
        ..Default::default()
    };
    let expected = snapshot.clone();
    let events = vec![
        MediaEventKind::DurationChanged(Some(seconds(999))),
        MediaEventKind::PositionChanged(seconds(99)),
        MediaEventKind::MetadataChanged(MediaMetadata {
            title: "old".into(),
            ..Default::default()
        }),
        MediaEventKind::StateChanged {
            state: PlaybackState::Idle,
            play_when_ready: false,
        },
        MediaEventKind::CapabilitiesChanged(MediaCapabilities::default()),
        MediaEventKind::VideoTextureChanged(Some("a-frame".into())),
        MediaEventKind::VideoSizeChanged(Some(MediaVideoSize {
            width: 1,
            height: 1,
        })),
        MediaEventKind::Error(MediaError::new(MediaErrorKind::Backend, "late failure")),
        MediaEventKind::Snapshot(MediaSnapshot {
            load_session: 1,
            ..Default::default()
        }),
    ];
    for event in events {
        for load_session in [0, 1, 3] {
            assert!(!snapshot.apply(MediaEvent::new(load_session, event.clone())));
            assert_eq!(snapshot, expected);
        }
    }
    assert!(!snapshot.apply(MediaEvent::new(
        2,
        MediaEventKind::Snapshot(MediaSnapshot {
            load_session: 1,
            ..Default::default()
        })
    )));
    assert_eq!(snapshot, expected);
    assert!(snapshot.apply(MediaEvent::new(
        2,
        MediaEventKind::DurationChanged(Some(seconds(45)))
    )));
    assert_eq!(snapshot.duration, Some(seconds(45)));
}

#[test]
fn frame_detach_and_idle_release_video_references_while_ended_keeps_the_last_frame() {
    let mut snapshot = MediaSnapshot {
        state: PlaybackState::Playing,
        video_texture: Some("frame".into()),
        video_size: Some(MediaVideoSize {
            width: 160,
            height: 90,
        }),
        ..Default::default()
    };
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::StateChanged {
            state: PlaybackState::Ended,
            play_when_ready: false,
        },
    ));
    assert_eq!(snapshot.video_texture.as_deref(), Some("frame"));
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(None),
    ));
    assert!(snapshot.video_texture.is_none());
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(Some("next-frame".into())),
    ));
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::StateChanged {
            state: PlaybackState::Idle,
            play_when_ready: false,
        },
    ));
    assert!(snapshot.video_texture.is_none());
    assert!(snapshot.video_size.is_none());
    snapshot.apply(MediaEvent::new(
        0,
        MediaEventKind::VideoTextureChanged(Some("late-idle-frame".into())),
    ));
    assert!(snapshot.video_texture.is_none());
}

#[test]
fn memory_sources_share_encoded_bytes_and_validate_without_dumping_payloads() {
    use std::sync::Arc;
    let bytes: Arc<[u8]> = Arc::from([11, 22, 33, 44]);
    let source = MediaSource::from(bytes.clone());
    let MediaSource::Bytes(cloned) = source.clone() else {
        panic!("bytes source")
    };
    assert!(Arc::ptr_eq(&bytes, &cloned));
    assert_eq!(format!("{source:?}"), "Bytes { len: 4 }");
    let command = |source| MediaCommand::Load {
        load_session: 1,
        source,
        options: MediaLoadOptions::default(),
    };
    assert!(command(source).validate(&MediaSnapshot::default()).is_ok());
    assert!(command(Vec::<u8>::new().into())
        .validate(&MediaSnapshot::default())
        .is_err());
    let snapshot = MediaSnapshot {
        load_session: 1,
        ..Default::default()
    };
    assert!(command("reused".into()).validate(&snapshot).is_err());
}
