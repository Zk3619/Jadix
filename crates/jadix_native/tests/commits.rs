use jadix_native::{presentation::*, BackendError, HeadlessRoot};

fn descriptor(content: NodeContent) -> NodeDescriptor {
    NodeDescriptor {
        content,
        frame: Rect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 40.0,
        },
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        accessible_name: None,
        style: Default::default(),
    }
}

fn create(node: NodeId, parent: Option<NodeId>, index: usize, content: NodeContent) -> Operation {
    Operation::Create {
        node,
        parent,
        index,
        descriptor: descriptor(content),
    }
}

fn commit(root: &mut HeadlessRoot, operations: Vec<Operation>) -> Result<bool, BackendError> {
    root.commit(CommitBatch {
        root: root.id(),
        previous: root.revision(),
        revision: root.revision() + 1,
        operations,
    })
}

fn button() -> NodeContent {
    NodeContent::Button {
        label: "Continue".into(),
        enabled: true,
    }
}

#[test]
fn failed_batch_does_not_publish_partial_tree_or_revision() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let child = NodeId::new(root.id());
    assert_eq!(
        commit(
            &mut root,
            vec![
                create(parent, None, 0, button()),
                create(child, Some(parent), 0, button())
            ]
        ),
        Err(BackendError::InvalidParent)
    );
    assert!(root.roots().is_empty());
    assert_eq!(root.revision(), 0);
    assert_eq!(
        commit(
            &mut root,
            vec![
                create(parent, None, 0, NodeContent::Container),
                create(child, Some(parent), 0, button())
            ]
        ),
        Ok(true)
    );
}

#[test]
fn move_preserves_identity_focus_and_rejects_cycles() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let child = NodeId::new(root.id());
    let button = NodeId::new(root.id());
    commit(
        &mut root,
        vec![
            create(parent, None, 0, NodeContent::Container),
            create(child, Some(parent), 0, NodeContent::Container),
            create(button, Some(child), 0, self::button()),
            Operation::Focus { node: Some(button) },
        ],
    )
    .unwrap();
    assert_eq!(
        commit(
            &mut root,
            vec![Operation::Move {
                node: parent,
                parent: Some(child),
                index: 0
            }]
        ),
        Err(BackendError::InvalidParent)
    );
    commit(
        &mut root,
        vec![Operation::Move {
            node: button,
            parent: Some(parent),
            index: 1,
        }],
    )
    .unwrap();
    assert_eq!(root.focused(), Some(button));
    assert_eq!(root.node(child).unwrap().children, Vec::new());
    assert_eq!(root.node(parent).unwrap().children, vec![child, button]);
}

#[test]
fn removed_subtree_cannot_be_resurrected_and_late_events_fail() {
    let mut root = HeadlessRoot::new();
    let parent = NodeId::new(root.id());
    let child = NodeId::new(root.id());
    commit(
        &mut root,
        vec![
            create(parent, None, 0, NodeContent::Container),
            create(child, Some(parent), 0, button()),
            Operation::Focus { node: Some(child) },
        ],
    )
    .unwrap();
    commit(&mut root, vec![Operation::Remove { node: parent }]).unwrap();
    assert_eq!(root.focused(), None);
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: child,
            payload: EventPayload::Activate
        }),
        Err(BackendError::UnknownNode(child))
    );
    assert_eq!(
        commit(&mut root, vec![create(child, None, 0, button())]),
        Err(BackendError::ReusedNode(child))
    );
    assert_eq!(root.revision(), 2);
}

#[test]
fn unrelated_commits_do_not_invalidate_live_node_events() {
    let mut root = HeadlessRoot::new();
    let node = NodeId::new(root.id());
    commit(&mut root, vec![create(node, None, 0, button())]).unwrap();
    let other = NodeId::new(root.id());
    commit(
        &mut root,
        vec![create(other, None, 1, NodeContent::Text("other".into()))],
    )
    .unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node,
            payload: EventPayload::Activate
        }),
        Ok(())
    );
}

#[test]
fn empty_batches_do_not_publish_but_stale_and_foreign_batches_fail() {
    let mut root = HeadlessRoot::new();
    assert_eq!(commit(&mut root, vec![]), Ok(false));
    assert_eq!(root.revision(), 0);
    assert_eq!(
        root.commit(CommitBatch {
            root: RootId::new(),
            previous: 0,
            revision: 1,
            operations: vec![]
        }),
        Err(BackendError::WrongRoot)
    );
    assert_eq!(
        root.commit(CommitBatch {
            root: root.id(),
            previous: 1,
            revision: 2,
            operations: vec![]
        }),
        Err(BackendError::Revision)
    );
}

#[test]
fn disabling_focused_control_releases_focus_and_rejects_input() {
    let mut root = HeadlessRoot::new();
    let node = NodeId::new(root.id());
    commit(
        &mut root,
        vec![
            create(node, None, 0, button()),
            Operation::Focus { node: Some(node) },
        ],
    )
    .unwrap();
    commit(
        &mut root,
        vec![Operation::Update {
            node,
            descriptor: descriptor(NodeContent::Button {
                label: "disabled".into(),
                enabled: false,
            }),
        }],
    )
    .unwrap();
    assert_eq!(root.focused(), None);
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node,
            payload: EventPayload::Activate
        }),
        Err(BackendError::InvalidEvent)
    );
    root.dispose();
    assert_eq!(commit(&mut root, vec![]), Err(BackendError::Disposed));
}

#[test]
fn edit_revisions_and_utf16_surrogate_boundaries_are_checked() {
    let mut root = HeadlessRoot::new();
    let node = NodeId::new(root.id());
    commit(
        &mut root,
        vec![create(
            node,
            None,
            0,
            NodeContent::Input {
                value: String::new(),
                placeholder: String::new(),
                enabled: true,
                password: false,
                revision: 0,
            },
        )],
    )
    .unwrap();
    let mut event = PlatformEvent {
        node,
        payload: EventPayload::Edit {
            value: "\u{4e2d}\u{1f600}".into(),
            revision: 1,
            selection_utf16: [2, 2],
            composing_utf16: None,
        },
    };
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    event.payload = EventPayload::Edit {
        value: "\u{4e2d}\u{1f600}".into(),
        revision: 1,
        selection_utf16: [3, 3],
        composing_utf16: Some([0, 1]),
    };
    assert_eq!(root.accept_event(&event), Ok(()));
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    assert_eq!(
        commit(
            &mut root,
            vec![Operation::Update {
                node,
                descriptor: descriptor(NodeContent::Input {
                    value: "old".into(),
                    placeholder: String::new(),
                    enabled: true,
                    password: false,
                    revision: 0
                })
            }]
        ),
        Err(BackendError::Revision)
    );
}

#[test]
fn foreign_nodes_and_invalid_frames_are_rejected() {
    let mut root = HeadlessRoot::new();
    let foreign = NodeId::new(RootId::new());
    assert_eq!(
        commit(&mut root, vec![create(foreign, None, 0, button())]),
        Err(BackendError::WrongRoot)
    );
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: foreign,
            payload: EventPayload::Activate
        }),
        Err(BackendError::WrongRoot)
    );
    let node = NodeId::new(root.id());
    let mut invalid = descriptor(button());
    invalid.frame.width = f64::NAN;
    assert_eq!(
        commit(
            &mut root,
            vec![Operation::Create {
                node,
                parent: None,
                index: 0,
                descriptor: invalid
            }]
        ),
        Err(BackendError::InvalidGeometry)
    );
    assert!(root.roots().is_empty());
}
