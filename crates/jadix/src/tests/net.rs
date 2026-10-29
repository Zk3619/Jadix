use super::*;
use crate::{
    app::ui_bridge::{drain_ui_tasks, UI_QUEUE_TEST_LOCK},
    core::reactivity::create_root,
};

#[derive(Default)]
struct Starter(std::sync::Mutex<Option<(u64, HttpEventSink, CancellationToken)>>);

impl HttpStarter for Starter {
    fn start(
        &self,
        id: u64,
        _request: HttpRequest,
        sink: HttpEventSink,
        cancellation: CancellationToken,
    ) -> Result<TaskHandle, HttpError> {
        *self.0.lock().unwrap() = Some((id, sink, cancellation.clone()));
        Ok(TaskHandle::cooperative(cancellation))
    }
}

#[test]
fn push_backend_uses_ui_delivery_and_owner_cancels_the_native_job() {
    let _guard = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let backend = Arc::new(Starter::default());
    let (owner, controller) = create_root(|| {
        HttpController::with_backend(Signal::new(HttpSnapshot::default()), backend.clone())
    });
    let id = controller.send(HttpRequest::get("https://host")).unwrap();
    let (_, sink, cancel) = backend.0.lock().unwrap().take().unwrap();
    std::thread::spawn(move || {
        sink.try_send(HttpEvent::new(id, HttpEventKind::Cancelled))
            .unwrap()
    })
    .join()
    .unwrap();
    assert_eq!(controller.current().state, HttpState::Pending);
    drain_ui_tasks();
    assert_eq!(controller.current().state, HttpState::Cancelled);
    drop(owner);
    assert!(cancel.is_cancelled());
    assert!(controller.send(HttpRequest::get("https://host")).is_err());
    assert!(matches!(
        controller
            .backend_event_sink()
            .try_send(HttpEvent::new(id, HttpEventKind::Cancelled)),
        Err(EventSendError::Closed(_))
    ));
}

#[test]
fn owner_disposal_during_start_defers_shutdown_without_leaking_accepted_work() {
    use crate::core::reactivity::OwnerHandle;
    thread_local! {
        static OWNER: std::cell::RefCell<Option<OwnerHandle>> = const { std::cell::RefCell::new(None) };
    }
    struct DisposeStarter(Arc<AtomicBool>);
    impl HttpStarter for DisposeStarter {
        fn start(
            &self,
            _id: u64,
            _request: HttpRequest,
            _sink: HttpEventSink,
            cancellation: CancellationToken,
        ) -> Result<TaskHandle, HttpError> {
            let owner = OWNER.with(|owner| owner.borrow_mut().take());
            drop(owner);
            let aborted = self.0.clone();
            Ok(TaskHandle::with_abort(cancellation, move || {
                aborted.store(true, Ordering::Release);
            }))
        }
    }
    let aborted = Arc::new(AtomicBool::new(false));
    let (owner, controller) = create_root(|| {
        HttpController::with_backend(
            Signal::new(HttpSnapshot::default()),
            Arc::new(DisposeStarter(aborted.clone())),
        )
    });
    OWNER.with(|slot| *slot.borrow_mut() = Some(owner));
    controller.send(HttpRequest::get("https://host")).unwrap();
    assert!(aborted.load(Ordering::Acquire));
}

#[test]
fn worker_events_only_apply_when_the_ui_queue_drains() {
    let _guard = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_owner, ()) = create_root(|| {
        let controller = HttpController::new(Signal::new(HttpSnapshot::default()), |_| Ok(()));
        let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
        let sink = controller.event_sink();
        std::thread::spawn(move || sink(HttpEvent::new(request_id, HttpEventKind::Cancelled)))
            .join()
            .unwrap();
        assert_eq!(controller.current().state, HttpState::Pending);
        drain_ui_tasks();
        assert_eq!(controller.current().state, HttpState::Cancelled);
    });
}

#[test]
fn queued_events_cannot_update_a_disposed_controller_with_a_live_parent_signal() {
    let _guard = UI_QUEUE_TEST_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let (_parent, ()) = create_root(|| {
        let state = Signal::new(HttpSnapshot::default());
        let (owner, controller) = create_root(|| HttpController::new(state, |_| Ok(())));
        let request_id = controller.send(HttpRequest::get("https://host")).unwrap();
        let sink = controller.event_sink();
        sink(HttpEvent::new(request_id, HttpEventKind::Cancelled));
        owner.dispose();
        drain_ui_tasks();
        assert_eq!(state.peek().state, HttpState::Pending);
        sink(HttpEvent::new(
            request_id,
            HttpEventKind::Failed(HttpError::new(HttpErrorKind::Backend, "late")),
        ));
        drain_ui_tasks();
        assert_eq!(state.peek().state, HttpState::Pending);
    });
}
