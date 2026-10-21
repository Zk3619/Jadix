use jadix_net::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Default)]
struct Starter {
    requests: Mutex<Vec<(u64, CancellationToken, HttpEventSink)>>,
    rejected: AtomicBool,
    aborts: Arc<AtomicUsize>,
}

impl HttpStarter for Starter {
    fn start(
        &self,
        id: u64,
        _request: HttpRequest,
        sink: HttpEventSink,
        cancel: CancellationToken,
    ) -> Result<TaskHandle, HttpError> {
        if self.rejected.load(Ordering::SeqCst) {
            return Err(HttpError::new(HttpErrorKind::Backend, "queue full"));
        }
        self.requests
            .lock()
            .unwrap()
            .push((id, cancel.clone(), sink));
        let aborts = self.aborts.clone();
        Ok(TaskHandle::with_abort(cancel, move || {
            aborts.fetch_add(1, Ordering::SeqCst);
        }))
    }
}

fn send(id: u64) -> HttpCommand {
    HttpCommand::Send {
        request_id: id,
        request: HttpRequest::get("https://host"),
    }
}

#[test]
fn accepted_replacement_cancel_and_shutdown_abort_only_their_jobs() {
    let starter = Arc::new(Starter::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let mut dispatcher = HttpDispatcher::new(
        starter.clone(),
        HttpEventSink::new(move |event| {
            observed.lock().unwrap().push(event);
            Ok(())
        }),
    );
    dispatcher.dispatch(send(1)).unwrap();
    dispatcher.dispatch(send(2)).unwrap();
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 1);
    {
        let requests = starter.requests.lock().unwrap();
        assert!(requests[0].1.is_cancelled());
        assert!(!requests[1].1.is_cancelled());
        assert!(matches!(
            requests[0]
                .2
                .try_send(HttpEvent::new(1, HttpEventKind::Cancelled)),
            Err(EventSendError::Closed(_))
        ));
        assert!(matches!(
            requests[1]
                .2
                .try_send(HttpEvent::new(99, HttpEventKind::Cancelled)),
            Err(EventSendError::Closed(_))
        ));
    }
    dispatcher
        .dispatch(HttpCommand::Cancel { request_id: 1 })
        .unwrap();
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 1);
    dispatcher
        .dispatch(HttpCommand::Cancel { request_id: 2 })
        .unwrap();
    dispatcher
        .dispatch(HttpCommand::Cancel { request_id: 2 })
        .unwrap();
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 2);
    assert!(dispatcher.dispatch(send(2)).is_err());
    dispatcher.dispatch(send(3)).unwrap();
    dispatcher.dispatch(HttpCommand::Shutdown).unwrap();
    dispatcher.dispatch(HttpCommand::Shutdown).unwrap();
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 3);
    assert_eq!(
        dispatcher.dispatch(send(4)).unwrap_err().kind,
        HttpErrorKind::Disconnected
    );
}

#[test]
fn rejection_keeps_previous_work_and_drop_releases_the_active_job() {
    let starter = Arc::new(Starter::default());
    let mut dispatcher = HttpDispatcher::new(starter.clone(), HttpEventSink::new(|_| Ok(())));
    dispatcher.dispatch(send(1)).unwrap();
    starter.rejected.store(true, Ordering::SeqCst);
    assert!(dispatcher.dispatch(send(2)).is_err());
    assert!(!starter.requests.lock().unwrap()[0].1.is_cancelled());
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 0);
    drop(dispatcher);
    assert_eq!(starter.aborts.load(Ordering::SeqCst), 1);
}

#[test]
fn backpressure_is_not_swallowed() {
    let starter = Arc::new(Starter::default());
    let mut dispatcher = HttpDispatcher::new(
        starter.clone(),
        HttpEventSink::new(|event| Err(EventSendError::Full(event))),
    );
    dispatcher.dispatch(send(1)).unwrap();
    assert!(matches!(
        starter.requests.lock().unwrap()[0]
            .2
            .try_send(HttpEvent::new(1, HttpEventKind::Cancelled)),
        Err(EventSendError::Full(_))
    ));
}
