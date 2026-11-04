use jadix_core::presentation::{
    hybrid::{snapshot::*, *},
    *,
};

#[test]
fn capture_requires_policy_budget_and_noninteractive_current_geometry() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut store = SnapshotStore::new(lifetime.clone(), 1024).unwrap();
    store.invalidate(1).unwrap();
    let mut geometry = HybridGeometry {
        node: NodeId::new(lifetime.id()),
        revision: 1,
        presentation: Presentation::Native,
        frame: Rect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        clips: vec![],
        visible: true,
        interactive: false,
        z_index: 0,
    };
    assert_eq!(
        store.request(&geometry, [16, 16], false),
        Err(SnapshotError::PolicyDenied)
    );
    assert_eq!(
        store.request(&geometry, [17, 17], true),
        Err(SnapshotError::Budget)
    );
    geometry.interactive = true;
    assert_eq!(
        store.request(&geometry, [16, 16], true),
        Err(SnapshotError::Interactive)
    );
    geometry.interactive = false;
    let ticket = store.request(&geometry, [16, 16], true).unwrap();
    let mut replacement = SnapshotStore::new(lifetime.clone(), 1024).unwrap();
    replacement.invalidate(1).unwrap();
    let replacement_ticket = replacement.request(&geometry, [16, 16], true).unwrap();
    assert_eq!(
        replacement.complete(ticket, vec![0; 1024]),
        Err(SnapshotError::Stale)
    );
    replacement
        .complete(replacement_ticket, vec![0; 1024])
        .unwrap();
    let other_lifetime = RootLifetime::new(RootId::new());
    let mut other = SnapshotStore::new(other_lifetime.clone(), 1024).unwrap();
    other.invalidate(1).unwrap();
    let other_geometry = HybridGeometry {
        node: NodeId::new(other_lifetime.id()),
        ..geometry.clone()
    };
    let other_ticket = other.request(&other_geometry, [16, 16], true).unwrap();
    assert_eq!(
        other.complete(ticket, vec![0; 1024]),
        Err(SnapshotError::Stale)
    );
    other.complete(other_ticket, vec![0; 1024]).unwrap();
    assert_eq!(
        store.request(&geometry, [16, 16], true),
        Err(SnapshotError::Busy)
    );
    assert_eq!(
        store.complete(ticket, vec![0; 4]),
        Err(SnapshotError::Pixels)
    );
    store.complete(ticket, vec![255; 1024]).unwrap();
    assert_eq!(store.pixels(geometry.node, 1).unwrap().len(), 1024);
    assert!(store.pixels(NodeId::new(lifetime.id()), 1).is_none());
    store.invalidate(2).unwrap();
    assert!(store.pixels(geometry.node, 1).is_none());
    assert_eq!(
        store.complete(ticket, vec![0; 1024]),
        Err(SnapshotError::Stale)
    );
    assert_eq!(
        store.request(&geometry, [16, 16], true),
        Err(SnapshotError::Geometry)
    );
    geometry.revision = 2;
    let late = store.request(&geometry, [16, 16], true).unwrap();
    lifetime.dispose();
    assert_eq!(
        store.complete(late, vec![0; 1024]),
        Err(SnapshotError::Disposed)
    );
    assert!(store.pixels(geometry.node, 2).is_none());
}
