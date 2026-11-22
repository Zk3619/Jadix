use jadix_native::{presentation::*, wire, BackendError, HeadlessRoot};

#[test]
fn submission_requires_a_fresh_revision_enabled_input_and_committed_composition() {
    let mut model = HeadlessRoot::new();
    let id = NodeId::new(model.id());
    let descriptor = NodeDescriptor {
        content: NodeContent::Input {
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            password: false,
            revision: 0,
            max_length: None,
        },
        frame: Rect::default(),
        style: Default::default(),
        accessible_name: None,
        accessible_description: None,
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
    };
    model
        .commit(CommitBatch {
            root: model.id(),
            previous: 0,
            revision: 1,
            operations: vec![Operation::Create {
                node: id,
                parent: None,
                index: 0,
                descriptor,
            }],
        })
        .unwrap();
    let submit = |revision| PlatformEvent {
        node: id,
        payload: EventPayload::Submit { revision },
    };
    model.accept_event(&submit(1)).unwrap();
    assert_eq!(
        model.accept_event(&submit(1)),
        Err(BackendError::InvalidEvent)
    );
    model
        .accept_event(&PlatformEvent {
            node: id,
            payload: EventPayload::Edit {
                value: "中😀".into(),
                revision: 2,
                selection_utf16: [3, 3],
                composing_utf16: Some([0, 3]),
            },
        })
        .unwrap();
    assert_eq!(
        model.accept_event(&submit(3)),
        Err(BackendError::InvalidEvent)
    );
    model
        .accept_event(&PlatformEvent {
            node: id,
            payload: EventPayload::Edit {
                value: "中😀".into(),
                revision: 3,
                selection_utf16: [3, 3],
                composing_utf16: None,
            },
        })
        .unwrap();
    model.accept_event(&submit(4)).unwrap();
    for revision in ["04", "-1", "18446744073709551616"] {
        let source = serde_json::json!({"root": model.id().token(), "node": id.token(),
            "payload": {"kind": "submit", "revision": revision}})
        .to_string();
        assert_eq!(
            wire::decode_event(&source, &model),
            Err(BackendError::InvalidEvent)
        );
    }
    let mut descriptor = model.node(id).unwrap().descriptor.clone();
    if let NodeContent::Input {
        enabled, revision, ..
    } = &mut descriptor.content
    {
        *enabled = false;
        *revision = 4;
    }
    model
        .commit(CommitBatch {
            root: model.id(),
            previous: 1,
            revision: 2,
            operations: vec![Operation::Update {
                node: id,
                descriptor,
            }],
        })
        .unwrap();
    assert_eq!(
        model.accept_event(&submit(5)),
        Err(BackendError::InvalidEvent)
    );
    model.dispose();
    assert_eq!(model.accept_event(&submit(5)), Err(BackendError::Disposed));
}
