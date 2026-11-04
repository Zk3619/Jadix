use jadix_native::{
    presentation::{capabilities::*, *},
    BackendError, HeadlessRoot,
};

#[test]
fn capability_admission_preserves_lifecycle_and_focus_contracts() {
    let mut capabilities = BackendCapabilities::headless();
    capabilities.focus = false;
    capabilities.text_editing = false;
    capabilities.scrolling = false;
    let mut root = HeadlessRoot::with_capabilities(capabilities);
    let button = NodeId::new(root.id());
    root.reconcile(&[node(
        button,
        NodeContent::Button {
            label: "ok".into(),
            enabled: true,
        },
        vec![],
    )])
    .unwrap();
    let event = PlatformEvent {
        node: button,
        payload: EventPayload::Focus,
    };
    assert_eq!(
        root.accept_event(&event),
        Err(BackendError::UnsupportedFocus)
    );
    assert_eq!(root.focused(), None);
    for content in [
        NodeContent::Scroll,
        NodeContent::Input {
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            password: false,
            revision: 0,
        },
    ] {
        let kind = content.kind();
        assert_eq!(
            root.reconcile(&[node(NodeId::new(root.id()), content, vec![])]),
            Err(BackendError::Unsupported(kind))
        );
        assert_eq!(root.revision(), 1);
        assert!(root.node(button).is_some());
    }
    root.dispose();
    assert_eq!(root.accept_event(&event), Err(BackendError::Disposed));
    assert_eq!(root.reconcile(&[]), Err(BackendError::Disposed));
}

fn node(id: NodeId, content: NodeContent, children: Vec<PresentationNode>) -> PresentationNode {
    PresentationNode {
        id,
        descriptor: NodeDescriptor {
            content,
            frame: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 40.0,
            },
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessible_name: None,
            style: Default::default(),
        },
        children,
    }
}

#[test]
fn reconcile_suppresses_idle_updates_and_preserves_focus_on_reorder() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let first = NodeId::new(root.id());
    let second = NodeId::new(root.id());
    let button = |id, label: &str| {
        node(
            id,
            NodeContent::Button {
                label: label.into(),
                enabled: true,
            },
            vec![],
        )
    };
    let mut tree = node(
        parent,
        NodeContent::Container,
        vec![button(first, "first"), button(second, "second")],
    );
    assert!(root.reconcile(&[tree.clone()]).unwrap());
    assert!(!root.reconcile(&[tree.clone()]).unwrap());
    assert_eq!(root.revision(), 1);
    root.accept_event(&PlatformEvent {
        node: first,
        payload: EventPayload::Focus,
    })
    .unwrap();
    tree.children.swap(0, 1);
    assert!(root.reconcile(&[tree.clone()]).unwrap());
    assert_eq!(root.node(parent).unwrap().children, vec![second, first]);
    assert_eq!(root.focused(), Some(first));
    tree.children[1].descriptor.content = NodeContent::Button {
        label: "changed".into(),
        enabled: true,
    };
    root.reconcile(&[tree]).unwrap();
    assert_eq!(root.revision(), 3);
    assert!(
        matches!(&root.node(first).unwrap().descriptor.content, NodeContent::Button { label, .. } if label == "changed")
    );
}

#[test]
fn moving_live_descendant_out_of_removed_parent_does_not_retire_it() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let child = NodeId::new(root.id());
    let leaf = node(child, NodeContent::Text("retained".into()), vec![]);
    root.reconcile(&[node(parent, NodeContent::Container, vec![leaf.clone()])])
        .unwrap();
    root.reconcile(&[leaf]).unwrap();
    assert_eq!(root.roots(), &[child]);
    assert!(root.node(parent).is_none());
    assert!(root.node(child).is_some());
    root.reconcile(&[]).unwrap();
    assert_eq!(
        root.reconcile(&[node(child, NodeContent::Container, vec![])]),
        Err(BackendError::ReusedNode(child))
    );
}

#[test]
fn ancestry_can_reverse_without_a_transient_cycle() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let child = NodeId::new(root.id());
    root.reconcile(&[node(
        parent,
        NodeContent::Container,
        vec![node(child, NodeContent::Container, vec![])],
    )])
    .unwrap();
    root.reconcile(&[node(
        child,
        NodeContent::Container,
        vec![node(parent, NodeContent::Container, vec![])],
    )])
    .unwrap();
    assert_eq!(root.node(parent).unwrap().parent, Some(child));
    assert_eq!(root.node(child).unwrap().parent, None);
}

#[test]
fn duplicate_foreign_or_unsupported_snapshots_do_not_change_the_tree() {
    let capabilities = BackendCapabilities {
        node_kinds: vec![NodeKind::Container, NodeKind::Text],
        text_editing: false,
        focus: false,
        scrolling: false,
    };
    let mut root = HeadlessRoot::with_capabilities(capabilities);
    let id = NodeId::new(root.id());
    let text = node(id, NodeContent::Text("original".into()), vec![]);
    root.reconcile(std::slice::from_ref(&text)).unwrap();
    assert_eq!(
        root.reconcile(&[text.clone(), text.clone()]),
        Err(BackendError::ReusedNode(id))
    );
    assert_eq!(
        root.reconcile(&[node(
            NodeId::new(RootId::new()),
            NodeContent::Container,
            vec![]
        )]),
        Err(BackendError::WrongRoot)
    );
    assert_eq!(
        root.reconcile(&[node(
            NodeId::new(root.id()),
            NodeContent::Input {
                value: String::new(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0
            },
            vec![]
        )]),
        Err(BackendError::Unsupported(NodeKind::Input))
    );
    assert_eq!(root.node(id).unwrap().descriptor, text.descriptor);
    assert_eq!(root.revision(), 1);
}
