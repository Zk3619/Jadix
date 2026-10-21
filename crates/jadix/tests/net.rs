#![cfg(feature = "net")]

use jadix::{
    core::reactivity::{create_effect, create_root, Signal},
    net::*,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn response(bytes: &[u8]) -> HttpResponse {
    HttpResponse {
        status: 200,
        url: "https://example.test/data".into(),
        headers: Vec::new(),
        body: bytes.into(),
    }
}

#[test]
fn injected_push_backend_shares_cancellation_and_shuts_down_with_the_owner() {
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Starter(Mutex<Vec<CancellationToken>>);
    impl HttpStarter for Starter {
        fn start(
            &self,
            _id: u64,
            _request: HttpRequest,
            _sink: HttpEventSink,
            cancel: CancellationToken,
        ) -> Result<TaskHandle, HttpError> {
            self.0.lock().unwrap().push(cancel.clone());
            Ok(TaskHandle::cooperative(cancel))
        }
    }
    let backend = Arc::new(Starter::default());
    let (owner, controller) = create_root(|| {
        HttpController::with_backend(Signal::new(HttpSnapshot::default()), backend.clone())
    });
    controller
        .send(HttpRequest::get("https://host/first"))
        .unwrap();
    controller
        .send(HttpRequest::get("https://host/second"))
        .unwrap();
    {
        let tokens = backend.0.lock().unwrap();
        assert!(tokens[0].is_cancelled());
        assert!(!tokens[1].is_cancelled());
    }
    controller.cancel().unwrap();
    assert!(backend.0.lock().unwrap()[1].is_cancelled());
    controller
        .send(HttpRequest::get("https://host/third"))
        .unwrap();
    drop(owner);
    assert!(backend.0.lock().unwrap()[2].is_cancelled());
    assert!(controller
        .send(HttpRequest::get("https://host/closed"))
        .is_err());
}

#[test]
fn replace_fences_old_events_and_clones_share_tokens() {
    let (_owner, ()) = create_root(|| {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), {
            let commands = commands.clone();
            move |command| {
                commands.borrow_mut().push(command);
                Ok(())
            }
        });
        let first = controller
            .send(HttpRequest::get("https://example.test/first"))
            .unwrap();
        let second = controller
            .clone()
            .send(HttpRequest::get("https://example.test/second"))
            .unwrap();
        assert_eq!((first, second), (1, 2));
        assert!(!controller.publish(HttpEvent::new(
            first,
            HttpEventKind::Completed(response(b"old"))
        )));
        assert!(!controller.publish(HttpEvent::new(second + 1, HttpEventKind::Cancelled)));
        assert!(controller.publish(HttpEvent::new(
            second,
            HttpEventKind::Completed(response(b"new"))
        )));
        assert_eq!(controller.current().response.unwrap().bytes(), b"new");
        assert_eq!(commands.borrow().len(), 2);
    });
}

#[test]
fn rejected_dispatch_restores_prior_state_without_reusing_tokens() {
    let (_owner, ()) = create_root(|| {
        let reject = Rc::new(Cell::new(false));
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), {
            let reject = reject.clone();
            move |_| {
                if reject.get() {
                    Err(HttpError::new(HttpErrorKind::Backend, "queue full"))
                } else {
                    Ok(())
                }
            }
        });
        controller
            .send(HttpRequest::get("https://host/first"))
            .unwrap();
        let previous = controller.current();
        reject.set(true);
        assert!(controller
            .send(HttpRequest::get("https://host/rejected"))
            .is_err());
        assert_eq!(controller.current(), previous);
        assert!(controller.request_error().peek().is_some());
        reject.set(false);
        assert_eq!(
            controller
                .send(HttpRequest::get("https://host/third"))
                .unwrap(),
            3
        );
        assert!(controller.request_error().peek().is_none());
        assert!(!controller.publish(HttpEvent::new(
            2,
            HttpEventKind::Completed(response(b"rejected"))
        )));
    });
}

#[test]
fn cancellation_is_idempotent_fences_completion_and_can_be_rejected() {
    let (_owner, ()) = create_root(|| {
        let reject = Rc::new(Cell::new(false));
        let commands = Rc::new(RefCell::new(Vec::new()));
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), {
            let reject = reject.clone();
            let commands = commands.clone();
            move |command| {
                if matches!(command, HttpCommand::Cancel { .. }) && reject.get() {
                    return Err(HttpError::new(HttpErrorKind::Backend, "queue full"));
                }
                commands.borrow_mut().push(command);
                Ok(())
            }
        });
        let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
        reject.set(true);
        assert!(controller.cancel().is_err());
        assert_eq!(controller.current().state, HttpState::Pending);
        reject.set(false);
        controller.cancel().unwrap();
        controller.clone().cancel().unwrap();
        assert_eq!(controller.current().state, HttpState::Cancelled);
        assert!(!controller.publish(HttpEvent::new(
            request_id,
            HttpEventKind::Completed(response(b"late"))
        )));
        assert_eq!(
            commands
                .borrow()
                .iter()
                .filter(|command| matches!(command, HttpCommand::Cancel { .. }))
                .count(),
            1
        );
    });
}

#[test]
fn owner_disposal_shuts_down_adapter_and_fences_retained_clones() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let (owner, controller) = create_root({
        let commands = commands.clone();
        move || {
            let controller =
                HttpController::new(Signal::new(HttpSnapshot::default()), move |command| {
                    commands.borrow_mut().push(command);
                    Ok(())
                });
            controller.send(HttpRequest::get("https://host")).unwrap();
            controller
        }
    });
    owner.dispose();
    assert!(matches!(
        commands.borrow().last(),
        Some(HttpCommand::Shutdown)
    ));
    assert_eq!(commands.borrow().len(), 2);
    assert_eq!(
        controller
            .send(HttpRequest::get("https://host"))
            .unwrap_err()
            .kind,
        HttpErrorKind::Disconnected
    );
    assert!(!controller.publish(HttpEvent::new(
        1,
        HttpEventKind::Completed(response(b"late"))
    )));
    assert!(!controller.snapshot().is_alive());
}

#[test]
fn duplicate_progress_does_not_notify_and_terminal_events_are_ignored() {
    let (_owner, ()) = create_root(|| {
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), |_| Ok(()));
        let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
        let runs = Rc::new(Cell::new(0));
        let state = controller.snapshot();
        create_effect({
            let runs = runs.clone();
            move || {
                state.get();
                runs.set(runs.get() + 1);
            }
        });
        let progress = HttpEvent::new(
            request_id,
            HttpEventKind::DownloadProgress(TransferProgress {
                transferred: 1,
                total: Some(2),
            }),
        );
        assert!(controller.publish(progress.clone()));
        let count = runs.get();
        assert!(!controller.publish(progress));
        assert_eq!(runs.get(), count);
        assert!(controller.publish(HttpEvent::new(request_id, HttpEventKind::Cancelled)));
        assert!(!controller.publish(HttpEvent::new(
            request_id,
            HttpEventKind::Completed(response(b"late"))
        )));
    });
}

#[test]
fn synchronous_observations_and_newer_reentrant_requests_are_not_rolled_back() {
    let (_owner, ()) = create_root(|| {
        let holder = Rc::new(RefCell::new(None::<HttpController>));
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), {
            let holder = holder.clone();
            move |command| {
                if let HttpCommand::Send { request_id, .. } = command {
                    let controller = holder.borrow().as_ref().unwrap().clone();
                    if request_id == 1 {
                        controller
                            .send(HttpRequest::get("https://host/newer"))
                            .unwrap();
                        return Err(HttpError::new(HttpErrorKind::Backend, "old rejected"));
                    }
                    controller.publish(HttpEvent::new(
                        request_id,
                        HttpEventKind::Completed(response(b"new")),
                    ));
                }
                Ok(())
            }
        });
        *holder.borrow_mut() = Some(controller.clone());
        assert!(controller
            .send(HttpRequest::get("https://host/old"))
            .is_err());
        assert_eq!(controller.current().request_id, 2);
        assert_eq!(controller.current().state, HttpState::Completed);
        holder.borrow_mut().take();
    });
}

#[test]
fn invalid_requests_and_token_exhaustion_never_dispatch() {
    let (_owner, ()) = create_root(|| {
        let sent = Rc::new(Cell::new(0));
        let controller = HttpController::new(
            Signal::new(HttpSnapshot {
                request_id: u64::MAX,
                ..HttpSnapshot::default()
            }),
            {
                let sent = sent.clone();
                move |_| {
                    sent.set(sent.get() + 1);
                    Ok(())
                }
            },
        );
        assert!(controller.send(HttpRequest::get("relative")).is_err());
        assert!(controller.send(HttpRequest::get("https://host")).is_err());
        assert_eq!(sent.get(), 0);
        assert_eq!(controller.current().request_id, u64::MAX);
    });
}
