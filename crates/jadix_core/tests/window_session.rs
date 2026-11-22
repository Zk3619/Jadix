use jadix_core::{create_root, window::WindowSession};

type Session = WindowSession<String, u32, String>;

#[test]
fn independent_roots_exchange_ordered_messages_and_one_result() {
    let (_parent, session) = create_root(|| Session::new("document".into()));
    let argument = session.content("editor").argument.unwrap();
    session.send(1).unwrap();
    let (child_owner, child) = create_root(|| Session::attach(&argument).unwrap());
    assert_eq!(child.args(), "document");
    assert_eq!(child.try_recv(), Some(1));
    child.send(2).unwrap();
    child.send(3).unwrap();
    assert_eq!(session.try_recv(), Some(2));
    assert_eq!(session.try_recv(), Some(3));
    assert_eq!(session.try_recv(), None);
    child.finish("saved".into()).unwrap();
    assert!(session.is_closed());
    assert!(session.send(4).is_err());
    assert!(child.finish("duplicate".into()).is_err());
    drop(child_owner);
    assert_eq!(session.try_result().as_deref(), Some("saved"));
    assert_eq!(session.try_result(), None);
}

#[test]
fn attachment_checks_types_single_child_and_owner_lifetime() {
    let (parent, session) = create_root(|| Session::new("document".into()));
    let argument = session.content("editor").argument.unwrap();
    let (_wrong_owner, rejected) =
        create_root(|| WindowSession::<u32, u32, String>::attach(&argument).is_err());
    assert!(rejected);
    let (_child_owner, child) = create_root(|| Session::attach(&argument).unwrap());
    let (_duplicate_owner, rejected) = create_root(|| Session::attach(&argument).is_err());
    assert!(rejected);
    session.send(1).unwrap();
    child.send(2).unwrap();
    drop(parent);
    assert!(child.is_closed());
    assert!(child.send(3).is_err());
    assert_eq!(child.try_recv(), None);
    assert_eq!(session.try_recv(), None);
    let (_late_owner, rejected) = create_root(|| Session::attach(&argument).is_err());
    assert!(rejected);
}

#[test]
fn queues_are_bounded_and_child_unmount_closes_delivery() {
    let (_parent, session) = create_root(|| Session::new("document".into()));
    let argument = session.content("editor").argument.unwrap();
    let (child_owner, child) = create_root(|| Session::attach(&argument).unwrap());
    for message in 0..1024 {
        session.send(message).unwrap();
        child.send(message).unwrap();
    }
    assert!(session.send(1024).is_err());
    assert!(child.send(1024).is_err());
    assert_eq!(child.try_recv(), Some(0));
    session.send(1024).unwrap();
    drop(child_owner);
    assert!(session.is_closed());
    assert!(session.send(1025).is_err());
}
