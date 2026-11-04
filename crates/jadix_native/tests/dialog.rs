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
