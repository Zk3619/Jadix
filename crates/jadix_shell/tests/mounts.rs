use jadix_core::{
    create_signal, on_cleanup,
    presentation::{hybrid::*, *},
};
use jadix_shell::{schema::*, *};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn authorized(limit: usize) -> Session {
    let mut session = Session::new(limit);
    session
        .verify(PackageManifest {
            package: "mounted.panel".into(),
            protocol_version: PROTOCOL_VERSION,
            requested_permissions: ["save".into()].into(),
        })
        .unwrap();
    session.authorize(&["save".into()].into()).unwrap();
    session
}

fn registry() -> HostRegistry {
    let mut registry = HostRegistry::default();
    registry.register("save".into(), "save".into()).unwrap();
    registry
}

#[test]
fn mounts_require_a_loaded_session_and_cannot_be_adopted_by_another_session() {
    let mut session = Session::new(2);
    assert!(matches!(
        session.mount_root(),
        Err(ShellError::InvalidState)
    ));
    session = authorized(2);
    assert!(matches!(
        session.mount_root(),
        Err(ShellError::InvalidState)
    ));
    session.load(|| Ok(())).unwrap();
    let lifetime = session.mount_root().unwrap();
    session.activate().unwrap();
    let mut other = authorized(2);
    other.load(|| Ok(())).unwrap();
    other.activate().unwrap();
    assert_eq!(
        other.request_for_root(&registry(), lifetime.id(), "save", &Value::Null),
        Err(ShellError::UnknownMount)
    );
    assert_eq!(
        other.unmount_root(lifetime.id()),
        Err(ShellError::UnknownMount)
    );
    assert!(!lifetime.is_disposed());
    session.unmount_root(lifetime.id()).unwrap();
    assert!(lifetime.is_disposed());
    assert_eq!(
        session.unmount_root(lifetime.id()),
        Err(ShellError::UnknownMount)
    );
    session.dispose();
    assert!(matches!(
        session.mount_root(),
        Err(ShellError::InvalidState)
    ));
}

#[test]
fn closing_one_mount_revokes_only_its_requests_and_releases_request_capacity() {
    let mut session = authorized(3);
    session.load(|| Ok(())).unwrap();
    let first = session.mount_root().unwrap();
    let second = session.mount_root().unwrap();
    session.activate().unwrap();
    let registry = registry();
    let scoped = session
        .request_for_root(&registry, first.id(), "save", &Value::Null)
        .unwrap();
    let sibling = session
        .request_for_root(&registry, second.id(), "save", &Value::Null)
        .unwrap();
    let global = session.request(&registry, "save").unwrap();
    assert_eq!(
        session.request(&registry, "save"),
        Err(ShellError::RequestLimit)
    );
    session.unmount_root(first.id()).unwrap();
    assert_eq!(session.pending_requests(), 2);
    assert_eq!(session.complete(scoped), Err(ShellError::StaleRequest));
    assert_eq!(
        session.request_for_root(&registry, first.id(), "save", &Value::Null),
        Err(ShellError::UnknownMount)
    );
    let replacement = session
        .request_for_root(&registry, second.id(), "save", &Value::Null)
        .unwrap();
    session.complete(sibling).unwrap();
    session.complete(global).unwrap();
    session.complete(replacement).unwrap();
    assert!(!second.is_disposed());
}

#[test]
fn backend_initiated_disposal_cannot_deliver_or_hold_a_scoped_request_slot() {
    let mut session = authorized(1);
    session.load(|| Ok(())).unwrap();
    let lifetime = session.mount_root().unwrap();
    session.activate().unwrap();
    let registry = registry();
    let pending = session
        .request_for_root(&registry, lifetime.id(), "save", &Value::Null)
        .unwrap();
    let mut backend = jadix_native::HeadlessRoot::with_lifetime(
        lifetime.clone(),
        presentation::capabilities::BackendCapabilities::headless(),
    );
    backend.dispose();
    assert_eq!(session.pending_requests(), 0);
    let global = session.request(&registry, "save").unwrap();
    assert_eq!(session.complete(pending), Err(ShellError::StaleRequest));
    assert_eq!(session.cancel(pending), Err(ShellError::StaleRequest));
    session.complete(global).unwrap();
}

#[test]
fn disposal_fault_and_drop_revoke_presenters_before_owner_cleanup() {
    for action in 0..3 {
        let mut session = authorized(1);
        let observed = Rc::new(Cell::new(false));
        let cleaned = observed.clone();
        let roots = Rc::new(RefCell::new(Vec::<RootLifetime>::new()));
        let checked = roots.clone();
        let signal = session
            .load(|| {
                on_cleanup(move || {
                    cleaned.set(checked.borrow().iter().all(RootLifetime::is_disposed))
                });
                Ok(create_signal(0usize))
            })
            .unwrap();
        let lifetime = session.mount_root().unwrap();
        roots.borrow_mut().push(lifetime.clone());
        let mut hybrid = HybridCoordinator::with_lifetime(lifetime.clone());
        let node = NodeId::new(lifetime.id());
        assert!(hybrid.stage(
            1,
            vec![HybridGeometry {
                node,
                revision: 1,
                presentation: Presentation::Native,
                frame: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0
                },
                clips: vec![],
                visible: true,
                interactive: true,
                z_index: 0,
            }]
        ));
        assert!(hybrid.confirm(1, false, true));
        assert!(hybrid.focus(1, node));
        session.activate().unwrap();
        match action {
            0 => session.dispose(),
            1 => session.fault().unwrap(),
            _ => drop(session),
        }
        assert!(observed.get());
        assert!(lifetime.is_disposed());
        assert!(signal.try_peek().is_none());
        assert!(hybrid.focused().is_none());
        assert!(hybrid.hit(1.0, 1.0).is_none());
    }
}

#[test]
fn mount_revocation_takes_precedence_over_result_schema_and_does_not_grant_permissions() {
    let mut session = authorized(2);
    session.load(|| Ok(())).unwrap();
    let lifetime = session.mount_root().unwrap();
    session.activate().unwrap();
    let mut registry = HostRegistry::default();
    registry
        .register_contract(
            "save".into(),
            MethodContract {
                permission: "save".into(),
                arguments: Schema::Bool,
                result: Schema::String { max_bytes: 8 },
            },
        )
        .unwrap();
    registry.register("admin".into(), "admin".into()).unwrap();
    assert_eq!(
        session.request_for_root(&registry, lifetime.id(), "admin", &Value::Null),
        Err(ShellError::PermissionDenied)
    );
    assert_eq!(
        session.request_for_root(&registry, lifetime.id(), "save", &Value::Null),
        Err(ShellError::InvalidArguments)
    );
    let request = session
        .request_for_root(&registry, lifetime.id(), "save", &Value::Bool(true))
        .unwrap();
    assert_eq!(
        session.complete_with(request, &Value::Number(1.0)),
        Err(ShellError::InvalidResult)
    );
    assert_eq!(session.pending_requests(), 1);
    lifetime.dispose();
    assert_eq!(
        session.complete_with(request, &Value::Number(1.0)),
        Err(ShellError::StaleRequest)
    );
    assert_eq!(session.pending_requests(), 0);
}
