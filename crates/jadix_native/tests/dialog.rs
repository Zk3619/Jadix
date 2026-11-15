use jadix_native::{presentation::*, wire, BackendError, HeadlessRoot};

fn node(root: RootId, content: NodeContent, children: Vec<PresentationNode>) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
            style: Default::default(),
        },
        children,
    }
}

#[test]
fn modal_blocks_background_and_hidden_controls_and_checks_dismiss_revision() {
    let mut root = HeadlessRoot::new();
    let button = || NodeContent::Button {
        label: "Action".into(),
        enabled: true,
    };
    let background = node(root.id(), button(), vec![]);
    let child = node(root.id(), button(), vec![]);
    let mut panel = node(
        root.id(),
        NodeContent::Dialog {
            open: false,
            close_on_overlay_click: false,
            revision: 0,
        },
        vec![child.clone()],
    );
    let mut nodes = vec![background.clone(), panel.clone()];
    let event = |node, payload| PlatformEvent { node, payload };
    root.reconcile(&nodes).unwrap();
    root.accept_event(&event(background.id, EventPayload::Focus))
        .unwrap();
    assert_eq!(
        root.accept_event(&event(child.id, EventPayload::Activate)),
        Err(BackendError::InvalidEvent)
    );
    panel.descriptor.content = NodeContent::Dialog {
        open: true,
        close_on_overlay_click: false,
        revision: 1,
    };
    nodes[1] = panel.clone();
    root.reconcile(&nodes).unwrap();
    assert_eq!(root.focused(), None);
    assert_eq!(
        root.accept_event(&event(background.id, EventPayload::Activate)),
        Err(BackendError::InvalidEvent)
    );
    root.accept_event(&event(child.id, EventPayload::Focus))
        .unwrap();
    assert_eq!(
        root.accept_event(&event(
            panel.id,
            EventPayload::Dismiss {
                backdrop: true,
                revision: 2
            }
        )),
        Err(BackendError::InvalidEvent)
    );
    let dismiss = wire::decode_event(&format!(r#"{{"root":"{}","node":"{}","payload":{{"kind":"dismiss","backdrop":false,"revision":"2"}}}}"#,root.id().token(),panel.id.token()), &root).unwrap();
    root.accept_event(&dismiss).unwrap();
    assert_eq!(root.accept_event(&dismiss), Err(BackendError::InvalidEvent));
    panel.descriptor.content = NodeContent::Dialog {
        open: false,
        close_on_overlay_click: false,
        revision: 2,
    };
    nodes[1] = panel;
    root.reconcile(&nodes).unwrap();
    assert_eq!(root.focused(), None);
    root.accept_event(&event(background.id, EventPayload::Activate))
        .unwrap();
    assert_eq!(
        root.accept_event(&event(child.id, EventPayload::Activate)),
        Err(BackendError::InvalidEvent)
    );
    root.dispose();
    assert_eq!(root.accept_event(&dismiss), Err(BackendError::Disposed));
}

#[test]
fn top_modal_alone_receives_events_and_closed_ancestors_hide_nested_modals() {
    let mut root = HeadlessRoot::new();
    let child = node(
        root.id(),
        NodeContent::Button {
            label: "Nested".into(),
            enabled: true,
        },
        vec![],
    );
    let nested = node(
        root.id(),
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 1,
        },
        vec![child.clone()],
    );
    let first = node(
        root.id(),
        NodeContent::Dialog {
            open: true,
            close_on_overlay_click: true,
            revision: 1,
        },
        vec![nested.clone()],
    );
    root.reconcile(std::slice::from_ref(&first)).unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: first.id,
            payload: EventPayload::Dismiss {
                backdrop: false,
                revision: 2
            }
        }),
        Err(BackendError::InvalidEvent)
    );
    root.accept_event(&PlatformEvent {
        node: child.id,
        payload: EventPayload::Activate,
    })
    .unwrap();
    let mut closed = first;
    closed.descriptor.content = NodeContent::Dialog {
        open: false,
        close_on_overlay_click: true,
        revision: 2,
    };
    root.reconcile(&[closed]).unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: nested.id,
            payload: EventPayload::Dismiss {
                backdrop: false,
                revision: 2
            }
        }),
        Err(BackendError::InvalidEvent)
    );
}

#[test]
fn cached_modal_order_survives_text_updates_moves_rejection_and_removal() {
    let mut root = HeadlessRoot::new();
    let button = |root| {
        node(
            root,
            NodeContent::Button {
                label: "action".into(),
                enabled: true,
            },
            vec![],
        )
    };
    let background = button(root.id());
    let a_child = button(root.id());
    let b_child = button(root.id());
    let dialog = |child: PresentationNode| {
        node(
            root.id(),
            NodeContent::Dialog {
                open: true,
                close_on_overlay_click: true,
                revision: 1,
            },
            vec![child],
        )
    };
    let a = dialog(a_child.clone());
    let b = dialog(b_child.clone());
    root.reconcile(&[background.clone(), a.clone(), b.clone()])
        .unwrap();
    let activate = |root: &mut HeadlessRoot, id| {
        root.accept_event(&PlatformEvent {
            node: id,
            payload: EventPayload::Activate,
        })
    };
    assert_eq!(
        activate(&mut root, a_child.id),
        Err(BackendError::InvalidEvent)
    );
    activate(&mut root, b_child.id).unwrap();
    let commit = |root: &mut HeadlessRoot, operations| {
        root.commit(CommitBatch {
            root: root.id(),
            previous: root.revision(),
            revision: root.revision() + 1,
            operations,
        })
    };
    commit(
        &mut root,
        vec![Operation::Move {
            node: a.id,
            parent: None,
            index: 2,
        }],
    )
    .unwrap();
    activate(&mut root, a_child.id).unwrap();
    assert_eq!(
        activate(&mut root, b_child.id),
        Err(BackendError::InvalidEvent)
    );
    let mut label = a_child.descriptor.clone();
    label.content = NodeContent::Button {
        label: "changed".into(),
        enabled: true,
    };
    commit(
        &mut root,
        vec![Operation::Update {
            node: a_child.id,
            descriptor: label,
        }],
    )
    .unwrap();
    assert_eq!(
        activate(&mut root, b_child.id),
        Err(BackendError::InvalidEvent)
    );
    let mut closed = a.descriptor.clone();
    closed.content = NodeContent::Dialog {
        open: false,
        close_on_overlay_click: true,
        revision: 2,
    };
    let missing = NodeId::new(root.id());
    let revision = root.revision();
    assert_eq!(
        commit(
            &mut root,
            vec![
                Operation::Update {
                    node: a.id,
                    descriptor: closed.clone()
                },
                Operation::Remove { node: missing }
            ]
        ),
        Err(BackendError::UnknownNode(missing))
    );
    assert_eq!(root.revision(), revision);
    activate(&mut root, a_child.id).unwrap();
    assert_eq!(
        activate(&mut root, b_child.id),
        Err(BackendError::InvalidEvent)
    );
    commit(&mut root, vec![Operation::Remove { node: a.id }]).unwrap();
    activate(&mut root, b_child.id).unwrap();
    assert_eq!(
        activate(&mut root, background.id),
        Err(BackendError::InvalidEvent)
    );
    commit(
        &mut root,
        vec![Operation::Update {
            node: b.id,
            descriptor: closed,
        }],
    )
    .unwrap();
    activate(&mut root, background.id).unwrap();
    assert_eq!(
        activate(&mut root, b_child.id),
        Err(BackendError::InvalidEvent)
    );
}
