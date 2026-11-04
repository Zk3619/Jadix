use jadix_core::on_cleanup;
use jadix_shell::*;
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

fn manifest(permissions: &[&str]) -> PackageManifest {
    PackageManifest {
        package: "example.panel".into(),
        protocol_version: presentation::PROTOCOL_VERSION,
        requested_permissions: permissions
            .iter()
            .map(|permission| permission.to_string())
            .collect(),
    }
}

fn ready(limit: usize) -> Session {
    let mut session = Session::new(limit);
    let manifest = manifest(&["documents.read"]);
    let grants = manifest.requested_permissions.clone();
    session.verify(manifest).unwrap();
    session.authorize(&grants).unwrap();
    session.load(|| Ok(())).unwrap();
    session.activate().unwrap();
    session
}

fn registry() -> HostRegistry {
    let mut registry = HostRegistry::default();
    registry
        .register("documents.get".into(), "documents.read".into())
        .unwrap();
    registry
        .register("documents.put".into(), "documents.write".into())
        .unwrap();
    registry
}

#[test]
fn lifecycle_checks_protocol_grants_and_builds_one_owner() {
    let mut session = Session::new(8);
    assert_eq!(session.activate(), Err(ShellError::InvalidState));
    let mut incompatible = manifest(&[]);
    incompatible.protocol_version += 1;
    assert_eq!(
        session.verify(incompatible),
        Err(ShellError::IncompatibleProtocol)
    );
    assert_eq!(session.state(), SessionState::Created);
    session.verify(manifest(&["documents.read"])).unwrap();
    assert_eq!(
        session.authorize(&BTreeSet::new()),
        Err(ShellError::PermissionDenied)
    );
    session
        .authorize(&["documents.read".into(), "documents.write".into()].into())
        .unwrap();
    let disposed = Rc::new(Cell::new(0));
    let observed = disposed.clone();
    session
        .load(|| {
            on_cleanup(move || observed.set(observed.get() + 1));
            Ok(())
        })
        .unwrap();
    assert_eq!(session.state(), SessionState::Loading);
    assert_eq!(session.load(|| Ok(())), Err(ShellError::InvalidState));
    session.activate().unwrap();
    assert_eq!(
        session.request(&registry(), "documents.put"),
        Err(ShellError::PermissionDenied)
    );
    session.dispose();
    session.dispose();
    assert_eq!(disposed.get(), 1);
    assert_eq!(session.state(), SessionState::Disposed);
}

#[test]
fn requests_are_session_scoped_consumed_once_and_bounded() {
    let registry = registry();
    let mut first = ready(1);
    let mut second = ready(1);
    assert_eq!(
        first.request(&registry, "unknown"),
        Err(ShellError::UnknownMethod)
    );
    let request = first.request(&registry, "documents.get").unwrap();
    let other = second.request(&registry, "documents.get").unwrap();
    assert_eq!(
        first.request(&registry, "documents.get"),
        Err(ShellError::RequestLimit)
    );
    assert_eq!(second.complete(request), Err(ShellError::StaleRequest));
    assert_eq!(second.pending_requests(), 1);
    first.complete(request).unwrap();
    assert_eq!(first.complete(request), Err(ShellError::StaleRequest));
    second.complete(other).unwrap();
    let cancelled = first.request(&registry, "documents.get").unwrap();
    first.cancel(cancelled).unwrap();
    assert_eq!(first.complete(cancelled), Err(ShellError::StaleRequest));
}

#[test]
fn fault_and_drop_release_owners_and_revoke_late_results() {
    let disposed = Rc::new(Cell::new(0));
    let mut session = Session::new(1);
    let observed = disposed.clone();
    session.verify(manifest(&["documents.read"])).unwrap();
    session
        .authorize(&["documents.read".into()].into())
        .unwrap();
    session
        .load(|| {
            on_cleanup(move || observed.set(observed.get() + 1));
            Ok(())
        })
        .unwrap();
    session.activate().unwrap();
    let request = session.request(&registry(), "documents.get").unwrap();
    session.fault().unwrap();
    assert_eq!(session.state(), SessionState::Faulted);
    assert_eq!(session.pending_requests(), 0);
    assert_eq!(session.complete(request), Err(ShellError::StaleRequest));
    assert_eq!(disposed.get(), 1);
    drop(session);
    assert_eq!(disposed.get(), 1);
}

#[test]
fn failed_load_disposes_the_partial_owner() {
    let mut session = Session::new(1);
    session.verify(manifest(&[])).unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    let disposed = Rc::new(Cell::new(false));
    let observed = disposed.clone();
    assert_eq!(
        session.load(|| {
            on_cleanup(move || observed.set(true));
            Err::<(), _>(ShellError::InvalidManifest)
        }),
        Err(ShellError::InvalidManifest)
    );
    assert_eq!(session.state(), SessionState::Faulted);
    assert!(disposed.get());
    assert_eq!(session.activate(), Err(ShellError::InvalidState));
}

#[test]
fn dropping_active_session_cleans_up_and_zero_limit_rejects_requests() {
    let mut session = ready(0);
    assert_eq!(
        session.request(&registry(), "documents.get"),
        Err(ShellError::RequestLimit)
    );
    session.dispose();
    assert_eq!(
        session.request(&registry(), "documents.get"),
        Err(ShellError::InvalidState)
    );
    assert_eq!(session.verify(manifest(&[])), Err(ShellError::InvalidState));
}

#[test]
fn dropping_active_owner_does_not_dispose_another_session() {
    let cleanup = Rc::new(Cell::new(0));
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let mut session = Session::new(1);
        session.verify(manifest(&[])).unwrap();
        session.authorize(&BTreeSet::new()).unwrap();
        let observed = cleanup.clone();
        session
            .load(|| {
                on_cleanup(move || observed.set(observed.get() + 1));
                Ok(())
            })
            .unwrap();
        session.activate().unwrap();
        sessions.push(session);
    }
    drop(sessions.pop());
    assert_eq!(cleanup.get(), 1);
    assert_eq!(sessions[0].state(), SessionState::Active);
    drop(sessions);
    assert_eq!(cleanup.get(), 2);
}
