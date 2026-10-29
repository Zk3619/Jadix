use jadix::{
    core::reactivity::{create_effect, create_root},
    prelude::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

#[test]
fn consecutive_loads_fence_late_events_before_the_new_backend_responds() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let captured = commands.clone();
        let controller =
            MediaController::new(Signal::new(MediaSnapshot::default()), move |command| {
                captured.borrow_mut().push(command);
                Ok(())
            });
        let first = controller
            .load("https://media/a", MediaLoadOptions::default())
            .unwrap();
        controller.publish(MediaEvent::new(
            first,
            MediaEventKind::Snapshot(MediaSnapshot {
                load_session: first,
                state: PlaybackState::Playing,
                duration: Some(Duration::from_secs(100)),
                video_texture: Some("a-frame".into()),
                ..Default::default()
            }),
        ));
        let second = controller
            .clone()
            .load("https://media/b", MediaLoadOptions::default())
            .unwrap();
        assert_eq!((first, second), (1, 2));
        assert_eq!(controller.current().load_session, second);
        assert!(controller.current().duration.is_none());
        assert!(controller.current().video_texture.is_none());
        assert!(!controller.publish(MediaEvent::new(
            first,
            MediaEventKind::DurationChanged(Some(Duration::from_secs(999)))
        )));
        assert!(!controller.publish(MediaEvent::new(
            first,
            MediaEventKind::Error(MediaError::new(MediaErrorKind::Backend, "late a failure"))
        )));
        assert!(!controller.publish(MediaEvent::new(
            second + 1,
            MediaEventKind::DurationChanged(Some(Duration::from_secs(888)))
        )));
        assert!(controller.publish(MediaEvent::new(
            second,
            MediaEventKind::DurationChanged(Some(Duration::from_secs(30)))
        )));
        assert_eq!(controller.current().duration, Some(Duration::from_secs(30)));
        let commands = commands.borrow();
        assert!(matches!(
            &commands[0],
            MediaCommand::Load {
                load_session: 1,
                ..
            }
        ));
        assert!(matches!(
            &commands[1],
            MediaCommand::Load {
                load_session: 2,
                ..
            }
        ));
    });
}

#[test]
fn rejected_load_restores_active_observations_and_never_reuses_its_token() {
    let (_owner, ()) = create_root(|| {
        let reject = Rc::new(Cell::new(false));
        let sink_reject = reject.clone();
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                load_session: 5,
                source: Some("current".into()),
                state: PlaybackState::Playing,
                duration: Some(Duration::from_secs(100)),
                video_texture: Some("current-frame".into()),
                ..Default::default()
            }),
            move |_| {
                if sink_reject.get() {
                    Err(MediaError::new(MediaErrorKind::Backend, "queue full"))
                } else {
                    Ok(())
                }
            },
        );
        let previous = controller.current();
        reject.set(true);
        assert!(controller
            .load("rejected", MediaLoadOptions::default())
            .is_err());
        assert_eq!(controller.current(), previous);
        assert!(controller.request_error().peek().is_some());
        assert!(!controller.publish(MediaEvent::new(
            6,
            MediaEventKind::Error(MediaError::new(
                MediaErrorKind::Backend,
                "late rejected event"
            ))
        )));
        assert!(controller
            .request(MediaCommand::Load {
                load_session: 6,
                source: "reused".into(),
                options: MediaLoadOptions::default()
            })
            .is_err());
        reject.set(false);
        assert_eq!(
            controller
                .load("accepted", MediaLoadOptions::default())
                .unwrap(),
            7
        );
        assert!(controller.request_error().peek().is_none());
    });
}

#[test]
fn synchronous_backend_observations_see_the_new_fence_and_survive_sink_rejection() {
    let (_owner, ()) = create_root(|| {
        let handle = Rc::new(RefCell::new(None::<MediaController>));
        let sink_handle = handle.clone();
        let controller =
            MediaController::new(Signal::new(MediaSnapshot::default()), move |command| {
                if let MediaCommand::Load { load_session, .. } = command {
                    let controller = sink_handle.borrow().as_ref().unwrap().clone();
                    assert!(controller.publish(MediaEvent::new(
                        load_session,
                        MediaEventKind::StateChanged {
                            state: PlaybackState::Ready,
                            play_when_ready: false,
                        }
                    )));
                    return Err(MediaError::new(
                        MediaErrorKind::Backend,
                        "reported after observation",
                    ));
                }
                Ok(())
            });
        *handle.borrow_mut() = Some(controller.clone());
        assert!(controller
            .load("source", MediaLoadOptions::default())
            .is_err());
        assert_eq!(controller.current().load_session, 1);
        assert_eq!(controller.current().state, PlaybackState::Ready);
        handle.borrow_mut().take();
    });
}

#[test]
fn outer_rejection_cannot_roll_back_a_reentrant_newer_load() {
    let (_owner, ()) = create_root(|| {
        let handle = Rc::new(RefCell::new(None::<MediaController>));
        let sink_handle = handle.clone();
        let controller =
            MediaController::new(Signal::new(MediaSnapshot::default()), move |command| {
                if let MediaCommand::Load {
                    load_session: 1, ..
                } = command
                {
                    let controller = sink_handle.borrow().as_ref().unwrap().clone();
                    assert_eq!(
                        controller
                            .load("newer", MediaLoadOptions::default())
                            .unwrap(),
                        2
                    );
                    return Err(MediaError::new(
                        MediaErrorKind::Backend,
                        "old load rejected",
                    ));
                }
                Ok(())
            });
        *handle.borrow_mut() = Some(controller.clone());
        assert!(controller
            .load("older", MediaLoadOptions::default())
            .is_err());
        assert_eq!(controller.current().load_session, 2);
        assert_eq!(controller.current().source, Some("newer".into()));
        handle.borrow_mut().take();
    });
}

#[test]
fn stale_and_duplicate_observations_do_not_notify_signal_subscribers() {
    let (_owner, ()) = create_root(|| {
        let snapshot = Signal::new(MediaSnapshot::default());
        let controller = MediaController::new(snapshot, |_| Ok(()));
        let notifications = Rc::new(Cell::new(0));
        let observed = notifications.clone();
        create_effect(move || {
            snapshot.get();
            observed.set(observed.get() + 1);
        });
        assert!(!controller.publish(MediaEvent::new(
            0,
            MediaEventKind::PositionChanged(Duration::ZERO)
        )));
        assert!(!controller.publish(MediaEvent::new(
            1,
            MediaEventKind::PositionChanged(Duration::from_secs(1))
        )));
        assert_eq!(notifications.get(), 1);
        assert!(controller.publish(MediaEvent::new(
            0,
            MediaEventKind::PositionChanged(Duration::from_secs(1))
        )));
        assert_eq!(notifications.get(), 2);
    });
}

#[test]
fn exhausted_load_counter_never_wraps_or_dispatches() {
    let (_owner, ()) = create_root(|| {
        let dispatched = Rc::new(Cell::new(0));
        let captured = dispatched.clone();
        let controller = MediaController::new(
            Signal::new(MediaSnapshot {
                load_session: u64::MAX,
                ..Default::default()
            }),
            move |_| {
                captured.set(captured.get() + 1);
                Ok(())
            },
        );
        assert_eq!(
            controller
                .load("next", MediaLoadOptions::default())
                .unwrap_err()
                .kind,
            MediaErrorKind::InvalidInput
        );
        assert_eq!(controller.current().load_session, u64::MAX);
        assert_eq!(dispatched.get(), 0);
    });
}
