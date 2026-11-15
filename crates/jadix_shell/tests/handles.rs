use jadix_core::create_signal;
use jadix_shell::{schema::*, *};
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

fn session() -> Session {
    let mut session = Session::new(4);
    session
        .verify(PackageManifest {
            package: "example.handles".into(),
            protocol_version: 1,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    session
}

#[test]
fn handles_export_existing_signal_and_reject_foreign_or_wrong_kinds() {
    let mut first = session();
    let mut second = session();
    let signal = first
        .load(|| Ok(create_signal(Value::Number(0.0))))
        .unwrap();
    second.load(|| Ok(())).unwrap();
    let root = first.mount_root().unwrap();
    let state = first
        .export_state(root.id(), signal, Schema::Number)
        .unwrap();
    first.write_state(&state, Value::Number(3.0)).unwrap();
    assert_eq!(signal.get(), Value::Number(3.0));
    assert_eq!(second.read_state(&state), Err(ShellError::StaleHandle));
    assert_eq!(
        first.write_state(&state, Value::String("bad".into())),
        Err(ShellError::InvalidArguments)
    );
    assert!(matches!(
        first.event_delivery(&state, Value::Null),
        Err(ShellError::InvalidHandleKind)
    ));
    first.unmount_root(root.id()).unwrap();
    assert_eq!(first.read_state(&state), Err(ShellError::StaleHandle));
}

#[test]
fn released_callback_and_unmounted_root_revoke_already_queued_delivery() {
    let mut session = session();
    session.load(|| Ok(())).unwrap();
    let root = session.mount_root().unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let event = session
        .export_event(root.id(), Schema::Null, move |_| {
            observed.set(observed.get() + 1)
        })
        .unwrap();
    let delivery = session.event_delivery(&event, Value::Null).unwrap();
    delivery.dispatch().unwrap();
    assert_eq!(calls.get(), 1);
    let late = session.event_delivery(&event, Value::Null).unwrap();
    session.release_handle(&event).unwrap();
    assert_eq!(late.dispatch(), Err(ShellError::StaleHandle));
    let event = session
        .export_event(root.id(), Schema::Null, |_| panic!("unmounted callback"))
        .unwrap();
    let late = session.event_delivery(&event, Value::Null).unwrap();
    session.unmount_root(root.id()).unwrap();
    assert_eq!(late.dispatch(), Err(ShellError::StaleHandle));
    assert_eq!(calls.get(), 1);
}

#[test]
fn delivery_runs_after_session_borrow_ends_and_disposal_precedes_owner_cleanup() {
    let session = Rc::new(std::cell::RefCell::new(session()));
    session.borrow_mut().load(|| Ok(())).unwrap();
    let root = session.borrow_mut().mount_root().unwrap();
    let weak = Rc::downgrade(&session);
    let event = session
        .borrow_mut()
        .export_event(root.id(), Schema::Null, move |_| {
            weak.upgrade().unwrap().borrow_mut().dispose();
        })
        .unwrap();
    let delivery = session
        .borrow()
        .event_delivery(&event, Value::Null)
        .unwrap();
    delivery.dispatch().unwrap();
    assert_eq!(session.borrow().state(), SessionState::Disposed);
    assert!(root.is_disposed());
}

#[test]
fn mount_checks_event_contract_root_and_session_ownership() {
    use jadix_shell::wire::*;
    let mut session = session();
    session.load(|| Ok(())).unwrap();
    let root = session.mount_root().unwrap();
    let other = session.mount_root().unwrap();
    let mut registry = ComponentRegistry::default();
    for schema in serde_json::from_str::<Vec<jadix_core::presentation::component::ComponentSchema>>(
        include_str!("../../jadix_components/schema/portable-v1.json"),
    )
    .unwrap()
    {
        registry.register(schema).unwrap();
    }
    let event = session
        .export_event(root.id(), Schema::Null, |_| {})
        .unwrap();
    let mut envelope = Envelope {
        protocol: 1,
        session: session.token(),
        command: Command::Mount {
            root: root.id().token(),
            tree: Box::new(ElementSpec {
                component: "Button".into(),
                key: None,
                props: Value::Object([("label".into(), Value::String("Continue".into()))].into()),
                style: Default::default(),
                events: [("activate".into(), event)].into(),
                children: vec![],
            }),
        },
    };
    session.validate_mount(&envelope, &registry).unwrap();
    let Command::Mount { root: token, .. } = &mut envelope.command else {
        unreachable!()
    };
    *token = other.id().token();
    assert_eq!(
        session.validate_mount(&envelope, &registry),
        Err(ShellError::StaleHandle)
    );
    let Command::Mount { tree, root: token } = &mut envelope.command else {
        unreachable!()
    };
    *token = root.id().token();
    tree.events.insert(
        "activate".into(),
        session
            .export_event(root.id(), Schema::String { max_bytes: 10 }, |_| {})
            .unwrap(),
    );
    assert_eq!(
        session.validate_mount(&envelope, &registry),
        Err(ShellError::InvalidArguments)
    );
    session.unmount_root(root.id()).unwrap();
    assert_eq!(
        session.validate_mount(&envelope, &registry),
        Err(ShellError::UnknownMount)
    );
}

#[test]
fn mount_and_handle_admission_are_bounded_before_allocation() {
    let mut session = Session::with_limits(runtime::ResourceLimits {
        mounts: 1,
        handles: 1,
        ..Default::default()
    })
    .unwrap();
    session
        .verify(PackageManifest {
            package: "example.quota".into(),
            protocol_version: 1,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    session.load(|| Ok(())).unwrap();
    let root = session.mount_root().unwrap();
    assert!(matches!(session.mount_root(), Err(ShellError::MountLimit)));
    let handle = session
        .export_event(root.id(), Schema::Null, |_| {})
        .unwrap();
    assert_eq!(
        session.export_event(root.id(), Schema::Null, |_| {}),
        Err(ShellError::HandleLimit)
    );
    session.release_handle(&handle).unwrap();
    session
        .export_event(root.id(), Schema::Null, |_| {})
        .unwrap();
    session.unmount_root(root.id()).unwrap();
    session.mount_root().unwrap();
}
