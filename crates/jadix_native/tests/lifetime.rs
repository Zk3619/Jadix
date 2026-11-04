use jadix_native::{
    presentation::{capabilities::BackendCapabilities, *},
    BackendError, HeadlessRoot,
};

fn tree(id: NodeId) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: "shared".into(),
                enabled: true,
            },
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 40.0,
            },
            accessible_name: None,
            style: Default::default(),
        },
        children: vec![],
    }
}

#[test]
fn external_revocation_hides_live_nodes_and_blocks_all_mutations() {
    let lifetime = RootLifetime::new(RootId::new());
    let mut root = HeadlessRoot::with_lifetime(lifetime.clone(), BackendCapabilities::headless());
    let node = NodeId::new(root.id());
    root.reconcile(&[tree(node)]).unwrap();
    let event = PlatformEvent {
        node,
        payload: EventPayload::Focus,
    };
    root.accept_event(&event).unwrap();
    root.lifetime().dispose();
    assert!(lifetime.is_disposed());
    assert!(root.node(node).is_none());
    assert!(root.roots().is_empty());
    assert!(root.focused().is_none());
    assert_eq!(root.accept_event(&event), Err(BackendError::Disposed));
    assert_eq!(root.reconcile(&[]), Err(BackendError::Disposed));
    assert_eq!(
        root.commit(CommitBatch {
            root: root.id(),
            previous: 1,
            revision: 2,
            operations: vec![]
        }),
        Err(BackendError::Disposed)
    );
    root.dispose();
    let mut replacement = HeadlessRoot::with_lifetime(lifetime, BackendCapabilities::headless());
    assert_eq!(
        replacement.reconcile(&[tree(node)]),
        Err(BackendError::Disposed)
    );
}

#[test]
fn candidate_transaction_success_and_failure_never_revoke_a_shared_root() {
    let mut root = HeadlessRoot::new();
    let lifetime = root.lifetime();
    let node = NodeId::new(root.id());
    root.reconcile(&[tree(node)]).unwrap();
    assert!(!lifetime.is_disposed());
    let invalid = tree(NodeId::new(RootId::new()));
    assert_eq!(root.reconcile(&[invalid]), Err(BackendError::WrongRoot));
    let mut changed = tree(node);
    changed.descriptor.frame.width = -1.0;
    assert_eq!(
        root.reconcile(&[changed]),
        Err(BackendError::InvalidGeometry)
    );
    assert!(!lifetime.is_disposed());
    assert!(root.node(node).is_some());
    root.dispose();
    assert!(lifetime.is_disposed());
}
