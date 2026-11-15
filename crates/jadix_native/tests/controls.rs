use base64::Engine;
use jadix_native::{presentation::*, BackendError, HeadlessRoot};

fn node(root: RootId, content: NodeContent) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content,
            frame: Rect::default(),
            style: Default::default(),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
        },
        children: vec![],
    }
}

#[test]
fn delayed_blur_does_not_release_the_new_focus_owner() {
    let mut root = HeadlessRoot::new();
    let first = node(
        root.id(),
        NodeContent::Button {
            label: "First".into(),
            enabled: true,
        },
    );
    let second = node(
        root.id(),
        NodeContent::Button {
            label: "Second".into(),
            enabled: true,
        },
    );
    let text = node(root.id(), NodeContent::Text("Not focusable".into()));
    root.reconcile(&[first.clone(), second.clone(), text.clone()])
        .unwrap();
    let event = |node, payload| PlatformEvent { node, payload };
    root.accept_event(&event(first.id, EventPayload::Focus))
        .unwrap();
    root.accept_event(&event(second.id, EventPayload::Focus))
        .unwrap();
    root.accept_event(&event(first.id, EventPayload::Blur))
        .unwrap();
    assert_eq!(root.focused(), Some(second.id));
    assert_eq!(
        root.accept_event(&event(text.id, EventPayload::Blur)),
        Err(BackendError::InvalidEvent)
    );
    let json = serde_json::json!({"root": root.id().token(), "node": second.id.token(), "payload": {"kind": "blur"}}).to_string();
    let blur = jadix_native::wire::decode_event(&json, &root).unwrap();
    root.accept_event(&blur).unwrap();
    assert_eq!(root.focused(), None);
}

#[test]
fn keyboard_focus_visibility_tracks_the_focus_owner_and_wire_event() {
    let mut root = HeadlessRoot::new();
    let first = node(
        root.id(),
        NodeContent::Input {
            value: String::new(),
            placeholder: String::new(),
            enabled: true,
            password: false,
            max_length: None,
            revision: 0,
        },
    );
    let second = node(
        root.id(),
        NodeContent::Button {
            label: "Second".into(),
            enabled: true,
        },
    );
    root.reconcile(&[first.clone(), second.clone()]).unwrap();

    let json = serde_json::json!({
        "root": root.id().token(),
        "node": first.id.token(),
        "payload": {"kind": "focusVisible"}
    });
    let visible = jadix_native::wire::decode_event(&json.to_string(), &root).unwrap();
    root.accept_event(&visible).unwrap();
    assert_eq!(root.focused(), Some(first.id));
    assert_eq!(root.focus_visible(), Some(first.id));

    root.accept_event(&PlatformEvent {
        node: second.id,
        payload: EventPayload::Focus,
    })
    .unwrap();
    assert_eq!(root.focused(), Some(second.id));
    assert_eq!(root.focus_visible(), None);

    root.accept_event(&PlatformEvent {
        node: first.id,
        payload: EventPayload::Blur,
    })
    .unwrap();
    assert_eq!(root.focused(), Some(second.id));
    assert_eq!(root.focus_visible(), None);
    root.dispose();
    assert_eq!(root.focus_visible(), None);
}

#[test]
fn multiline_edits_keep_revision_and_utf16_rules() {
    let mut root = HeadlessRoot::new();
    let node = node(
        root.id(),
        NodeContent::Textarea {
            max_length: None,
            value: String::new(),
            placeholder: "Note".into(),
            enabled: true,
            revision: 0,
        },
    );
    root.reconcile(std::slice::from_ref(&node)).unwrap();
    let event = |revision, selection_utf16| PlatformEvent {
        node: node.id,
        payload: EventPayload::Edit {
            value: "中😀\n文".into(),
            revision,
            selection_utf16,
            composing_utf16: None,
        },
    };
    assert_eq!(
        root.accept_event(&event(1, [2, 2])),
        Err(BackendError::InvalidEvent)
    );
    root.accept_event(&event(1, [5, 5])).unwrap();
    assert_eq!(
        root.accept_event(&event(1, [5, 5])),
        Err(BackendError::InvalidEvent)
    );
    root.dispose();
    assert_eq!(
        root.accept_event(&event(2, [5, 5])),
        Err(BackendError::Disposed)
    );
}

#[test]
fn textarea_internal_scroll_is_not_an_outer_scroll_event() {
    let mut root = HeadlessRoot::new();
    let editor = node(
        root.id(),
        NodeContent::Textarea {
            max_length: None,
            value: "中😀\n第二行".into(),
            placeholder: String::new(),
            enabled: true,
            revision: 0,
        },
    );
    let scroll = node(root.id(), NodeContent::Scroll);
    root.reconcile(&[editor.clone(), scroll.clone()]).unwrap();
    let before = root.node(editor.id).unwrap().clone();
    let event = |node| PlatformEvent {
        node,
        payload: EventPayload::Scroll { x: 0.0, y: 120.0 },
    };
    assert_eq!(
        root.accept_event(&event(editor.id)),
        Err(BackendError::InvalidEvent)
    );
    assert_eq!(root.node(editor.id), Some(&before));
    root.accept_event(&event(scroll.id)).unwrap();
    assert_eq!(root.node(editor.id), Some(&before));
}

#[test]
fn wheel_events_are_bounded_and_only_target_scroll_nodes() {
    let mut root = HeadlessRoot::new();
    let button = node(
        root.id(),
        NodeContent::Button {
            label: "Wheel target".into(),
            enabled: true,
        },
    );
    let scroll = node(root.id(), NodeContent::Scroll);
    root.reconcile(&[button.clone(), scroll.clone()]).unwrap();
    let event = |node, delta_x, delta_y| PlatformEvent {
        node,
        payload: EventPayload::Wheel { delta_x, delta_y },
    };
    assert_eq!(
        root.accept_event(&event(button.id, 0.0, 12.0)),
        Err(BackendError::InvalidEvent)
    );
    assert_eq!(
        root.accept_event(&event(scroll.id, f64::NAN, 12.0)),
        Err(BackendError::InvalidEvent)
    );
    assert_eq!(
        root.accept_event(&event(scroll.id, 0.0, 1_000_001.0)),
        Err(BackendError::InvalidEvent)
    );
    root.accept_event(&event(scroll.id, -2.5, 120.0)).unwrap();
}

#[test]
fn wheel_wire_payload_uses_camel_case_deltas() {
    let mut root = HeadlessRoot::new();
    let scroll = node(root.id(), NodeContent::Scroll);
    root.reconcile(std::slice::from_ref(&scroll)).unwrap();
    let json = serde_json::json!({
        "root": root.id().token(),
        "node": scroll.id.token(),
        "payload": {"kind": "wheel", "deltaX": -3.5, "deltaY": 48.0}
    });
    let decoded = jadix_native::wire::decode_event(&json.to_string(), &root).unwrap();
    assert_eq!(
        decoded.payload,
        EventPayload::Wheel {
            delta_x: -3.5,
            delta_y: 48.0
        }
    );
    root.accept_event(&decoded).unwrap();
}

#[test]
fn checkbox_toggle_revisions_disabled_state_and_root_are_enforced() {
    let mut root = HeadlessRoot::new();
    let mut node = node(
        root.id(),
        NodeContent::Checkbox {
            label: "Accept".into(),
            checked: false,
            enabled: true,
            revision: 0,
        },
    );
    root.reconcile(std::slice::from_ref(&node)).unwrap();
    let event = PlatformEvent {
        node: node.id,
        payload: EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    };
    root.accept_event(&event).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    node.descriptor.content = NodeContent::Checkbox {
        label: "Accept".into(),
        checked: true,
        enabled: false,
        revision: 1,
    };
    root.reconcile(std::slice::from_ref(&node)).unwrap();
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: node.id,
            payload: EventPayload::Toggle {
                checked: false,
                revision: 2
            }
        }),
        Err(BackendError::InvalidEvent)
    );
    assert_eq!(
        root.accept_event(&PlatformEvent {
            node: NodeId::new(RootId::new()),
            payload: EventPayload::Toggle {
                checked: false,
                revision: 2
            }
        }),
        Err(BackendError::WrongRoot)
    );
}

#[test]
fn image_resources_have_bounded_local_png_headers() {
    let source = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGPQqPjwHwAE7AKQVV27NwAAAABJRU5ErkJggg==";
    let content = NodeContent::Image {
        source: source.into(),
        alt: "Pixel".into(),
    };
    assert_eq!(content.image_dimensions(), Some([1, 1]));
    let mut root = HeadlessRoot::new();
    root.reconcile(&[node(root.id(), content)]).unwrap();
    for source in [
        "https://example.invalid/image.png".into(),
        "data:image/png;base64,AAAA".into(),
        "x".repeat(1024 * 1024 + 1),
    ] {
        let invalid = node(
            root.id(),
            NodeContent::Image {
                source,
                alt: String::new(),
            },
        );
        assert_eq!(
            root.reconcile(&[invalid]),
            Err(BackendError::InvalidContent)
        );
    }
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(source.split_once(',').unwrap().1)
        .unwrap();
    bytes[16..20].copy_from_slice(&4097u32.to_be_bytes());
    let invalid = NodeContent::Image {
        source: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
        alt: String::new(),
    };
    assert!(!invalid.is_valid());
}
