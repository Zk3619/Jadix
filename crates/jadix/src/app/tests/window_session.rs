use super::*;

#[test]
fn arguments_and_creation_failure_close_once() {
    let session = WindowSession::<_, (), String>::new("launch argument".to_owned());
    assert_eq!(session.clone().args(), "launch argument");
    let results = Rc::new(RefCell::new(Vec::new()));
    let output = results.clone();
    session.on_closed(move |event| output.borrow_mut().push(event.result));
    assert!(session
        .open(WindowOptions::new(), |_| Element::new())
        .is_err());
    assert!(session.is_closed());
    assert!(results.borrow().is_empty());
    drain_tasks();
    assert_eq!(*results.borrow(), vec![None]);
    session.finish();
    drain_tasks();
    assert_eq!(results.borrow().len(), 1);
    assert!(session.send_to_parent(()).is_err());
}

#[test]
fn messages_are_fifo_deferred_scoped_and_cancelled_on_close() {
    let _scope = window_scope::enter_primary();
    let session = WindowSession::<_, i32, i32>::new(());
    session.inner.status.set(Status::Open);
    let received = Rc::new(RefCell::new(Vec::new()));
    let output = received.clone();
    session.on_child_message(move |message| {
        assert_eq!(window_scope::active(), WindowScope::Primary);
        output.borrow_mut().push(message);
    });
    session.send_to_parent(1).unwrap();
    session.send_to_parent(2).unwrap();
    assert!(received.borrow().is_empty());
    drain_tasks();
    assert_eq!(*received.borrow(), vec![1, 2]);
    session.send_to_parent(3).unwrap();
    session.finish();
    drain_tasks();
    assert_eq!(*received.borrow(), vec![1, 2]);
}

#[test]
fn result_is_delivered_once_after_close() {
    let session = WindowSession::<_, (), String>::new(());
    let output = Rc::new(RefCell::new(None));
    let received = output.clone();
    session.on_closed(move |event| *received.borrow_mut() = event.result);
    *session.inner.result.borrow_mut() = Some("saved".into());
    session.finish();
    assert!(output.borrow().is_none());
    drain_tasks();
    assert_eq!(output.borrow().as_deref(), Some("saved"));
}
