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
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            accessible_description: None,
            accessible_name: None,
        },
        children: vec![],
    }
}

#[test]
fn shared_interactions_validate_wire_policy_geometry_and_node_lifetime() {
    use interaction::*;
    let mut root = HeadlessRoot::new();
    let mut target = node(root.id(), NodeContent::Container);
    target.descriptor.interaction = InteractionPolicy { pan: Some(PanAxis::Horizontal), press: true, ..Default::default() };
    root.reconcile(std::slice::from_ref(&target)).unwrap();
    let mut event = InteractionEvent { kind: InteractionKind::Pan, phase: InteractionPhase::Started,
        position: [20.0, 30.0], local_position: None, timestamp: 1.0, modifiers: Default::default(), translation: Some([0.0; 2]),
        velocity: Some([15.0, 0.0]), value: None, rate: None, touches: vec![], active_touches: vec![] };
    let wire = serde_json::json!({"root": root.id().token(), "node": target.id.token(),
        "payload": {"kind": "interaction", "event": event}});
    let decoded = jadix_native::wire::decode_event(&wire.to_string(), &root).unwrap();
    assert_eq!(decoded.payload, EventPayload::Interaction(event.clone()));
    root.accept_event(&decoded).unwrap();
    event.velocity = Some([f64::INFINITY, 0.0]);
    assert_eq!(root.accept_event(&PlatformEvent { node: target.id, payload: EventPayload::Interaction(event.clone()) }), Err(BackendError::InvalidEvent));
    event.velocity = Some([0.0; 2]); event.kind = InteractionKind::Pinch; event.value = Some(2.0); event.rate = Some(0.0);
    event.translation = None; event.velocity = None;
    assert_eq!(root.accept_event(&PlatformEvent { node: target.id, payload: EventPayload::Interaction(event) }), Err(BackendError::InvalidEvent));
    let committed = target.clone();
    target.descriptor.interaction.long_press_millis = Some(0);
    assert_eq!(root.reconcile(&[target]), Err(BackendError::InvalidContent));
    assert_eq!(root.node(committed.id).unwrap().descriptor, committed.descriptor);
    root.reconcile(&[]).unwrap();
    assert_eq!(root.accept_event(&decoded), Err(BackendError::UnknownNode(committed.id)));
}

#[test]
fn generic_keyboard_events_require_the_current_focus_owner_and_declared_policy() {
    use keyboard::{KeyEvent, KeyModifiers, KeyPhase, KeyShortcut, KeyboardPolicy};
    let mut root = HeadlessRoot::new();
    let mut container = node(root.id(), NodeContent::Container);
    container.descriptor.keyboard = KeyboardPolicy {
        focusable: true, key_down: true, key_up: false,
        shortcuts: vec![KeyShortcut { id: "save".into(), key: "s".into(),
            modifiers: KeyModifiers { meta: true, ..Default::default() } }],
    };
    let other = node(root.id(), NodeContent::Button { label: "Other".into(), enabled: true });
    root.reconcile(&[container.clone(), other.clone()]).unwrap();
    let key = KeyEvent { phase: KeyPhase::Down, key: "Ж".into(), code: 4,
        modifiers: Default::default(), repeat: false };
    let event = |node, payload| PlatformEvent { node, payload };
    assert_eq!(root.accept_event(&event(container.id, EventPayload::Key(key.clone()))), Err(BackendError::InvalidEvent));
    root.accept_event(&event(container.id, EventPayload::FocusVisible)).unwrap();
    assert_eq!(root.focus_visible(), Some(container.id));
    let json = serde_json::json!({"root":root.id().token(), "node":container.id.token(),
        "payload":{"kind":"key","event":key}});
    let decoded = jadix_native::wire::decode_event(&json.to_string(), &root).unwrap();
    root.accept_event(&decoded).unwrap();
    root.accept_event(&event(container.id, EventPayload::Shortcut { id: "save".into() })).unwrap();
    assert_eq!(root.accept_event(&event(container.id, EventPayload::Shortcut { id: "undeclared".into() })), Err(BackendError::InvalidEvent));
    assert_eq!(root.accept_event(&event(container.id, EventPayload::Key(KeyEvent { phase: KeyPhase::Up, ..key.clone() }))), Err(BackendError::InvalidEvent));
    assert_eq!(root.accept_event(&event(container.id, EventPayload::Key(KeyEvent { code: 0, ..key.clone() }))), Err(BackendError::InvalidEvent));
    root.accept_event(&event(other.id, EventPayload::Focus)).unwrap();
    assert_eq!(root.accept_event(&decoded), Err(BackendError::InvalidEvent));
    root.accept_event(&event(container.id, EventPayload::Blur)).unwrap();
    assert_eq!(root.focused(), Some(other.id));
    root.accept_event(&event(container.id, EventPayload::Focus)).unwrap();
    container.descriptor.keyboard.focusable = false;
    root.reconcile(&[container, other]).unwrap();
    assert_eq!(root.focused(), None);
    assert_eq!(root.accept_event(&decoded), Err(BackendError::InvalidEvent));
}

#[test]
fn ambiguous_keyboard_policy_does_not_replace_the_committed_tree() {
    let mut root = HeadlessRoot::new();
    let mut container = node(root.id(), NodeContent::Container);
    root.reconcile(std::slice::from_ref(&container)).unwrap();
    let committed = root.node(container.id).unwrap().clone();
    let id = container.id;
    let shortcut = keyboard::KeyShortcut { id: "same".into(), key: "s".into(), modifiers: Default::default() };
    container.descriptor.keyboard.shortcuts = vec![shortcut.clone(), shortcut];
    assert!(root.reconcile(&[container]).is_err());
    assert_eq!(root.node(id), Some(&committed));
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
    let image = node(root.id(), content);
    let load = serde_json::json!({
        "root": root.id().token(),
        "node": image.id.token(),
        "payload": {"kind": "imageLoad", "width": 1, "height": 1}
    });
    root.reconcile(&[image]).unwrap();
    let event = jadix_native::wire::decode_event(&load.to_string(), &root).unwrap();
    assert_eq!(
        event.payload,
        EventPayload::ImageLoad(jadix_native::presentation::ImageLoadEvent::Loaded {
            width: 1,
            height: 1,
        })
    );
    root.accept_event(&event).unwrap();
    let mut progress = load.clone();
    progress["payload"] = serde_json::json!({"kind": "imageProgress", "loaded": 8, "total": 0});
    let event = jadix_native::wire::decode_event(&progress.to_string(), &root).unwrap();
    assert_eq!(
        event.payload,
        EventPayload::ImageLoad(jadix_native::presentation::ImageLoadEvent::Progress {
            loaded: 8,
            total: 0,
        })
    );
    root.accept_event(&event).unwrap();
    progress["payload"]["total"] = serde_json::json!(7);
    let event = jadix_native::wire::decode_event(&progress.to_string(), &root).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    for source in [
        "http://example.invalid/image.png".into(),
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

#[test]
fn local_image_sources_require_an_explicit_host_capability() {
    let asset = NodeContent::Image {
        source: "asset:/images/photo.tiff".into(),
        alt: "Photo".into(),
    };
    let file = NodeContent::Image {
        source: "file:///Users/example/photo.heic".into(),
        alt: String::new(),
    };
    assert!(asset.is_valid());
    assert!(file.is_valid());
    assert!(!NodeContent::Image {
        source: "asset:/images/../private.png".into(),
        alt: String::new(),
    }
    .is_valid());
    assert!(!NodeContent::Image {
        source: "file:///tmp/photo.png?download=1".into(),
        alt: String::new(),
    }
    .is_valid());

    let mut capabilities = jadix_native::presentation::capabilities::BackendCapabilities::headless();
    assert!(!capabilities.supports(&asset));
    capabilities.local_image_sources = true;
    assert!(capabilities.supports(&asset));
    assert!(capabilities.supports(&file));

    let gif = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    let inline = NodeContent::Image {
        source: gif.into(),
        alt: "Pixel".into(),
    };
    assert!(inline.is_valid());
    assert!(!NodeContent::is_inline_raster_source(&gif.replace("image/gif", "image/jpeg")));
    assert!(!capabilities.supports(&inline));
    capabilities.inline_raster_sources = true;
    assert!(capabilities.supports(&inline));
}

#[test]
fn inline_svg_sources_require_the_svg_capability() {
    let image = NodeContent::Image {
        source: "data:image/svg+xml;base64,PHN2Zy8+".into(),
        alt: "Vector image".into(),
    };
    assert!(image.is_valid());

    let mut capabilities = jadix_native::presentation::capabilities::BackendCapabilities::headless();
    capabilities.inline_raster_sources = true;
    assert!(!capabilities.supports(&image));
    capabilities.inline_svg_sources = true;
    assert!(capabilities.supports(&image));
}

#[test]
fn remote_image_sources_require_https_and_an_explicit_host_capability() {
    let image = |source: &str| NodeContent::Image {
        source: source.into(),
        alt: "Photo".into(),
    };
    let remote = image("https://example.com/images/photo.png?w=320");
    assert!(remote.is_valid());
    for source in [
        "http://example.com/photo.png",
        "https:///photo.png",
        "https://user@example.com/photo.png",
        "https://example.com/photo.png#fragment",
    ] {
        assert!(!image(source).is_valid(), "{source}");
    }

    let mut capabilities = jadix_native::presentation::capabilities::BackendCapabilities::headless();
    let mut root = HeadlessRoot::new();
    assert_eq!(
        root.reconcile(&[node(root.id(), remote.clone())]),
        Err(BackendError::Unsupported(capabilities::NodeKind::Image))
    );
    assert!(!capabilities.supports(&remote));
    capabilities.local_image_sources = true;
    assert!(!capabilities.supports(&remote));
    capabilities.remote_image_sources = true;
    assert!(capabilities.supports(&remote));
}

#[test]
fn keyboard_scope_accepts_focused_descendants_and_enabled_editors_only() {
    use keyboard::*;
    let mut root = HeadlessRoot::new();
    let mut scope = node(root.id(), NodeContent::Container);
    scope.descriptor.keyboard.key_down = true;
    scope.descriptor.keyboard.shortcuts = vec![KeyShortcut { id: "save".into(), key: "s".into(), modifiers: Default::default() }];
    let mut input = node(root.id(), NodeContent::Input { value: String::new(), placeholder: String::new(), password: false, enabled: true, max_length: None, revision: 0 });
    input.descriptor.keyboard.key_down = true;
    scope.children.push(input.clone());
    let sibling = node(root.id(), NodeContent::Button { label: "Other".into(), enabled: true });
    root.reconcile(&[scope.clone(), sibling.clone()]).unwrap();
    root.accept_event(&PlatformEvent { node: input.id, payload: EventPayload::FocusVisible }).unwrap();
    let key = KeyEvent { phase: KeyPhase::Down, key: "s".into(), code: 22, modifiers: Default::default(), repeat: false };
    for target in [scope.id, input.id] { root.accept_event(&PlatformEvent { node: target, payload: EventPayload::Key(key.clone()) }).unwrap(); }
    root.accept_event(&PlatformEvent { node: scope.id, payload: EventPayload::Shortcut { id: "save".into() } }).unwrap();
    assert_eq!(root.accept_event(&PlatformEvent { node: sibling.id, payload: EventPayload::Key(key) }), Err(BackendError::InvalidEvent));
    input.descriptor.content = NodeContent::Input { value: String::new(), placeholder: String::new(), password: false, enabled: false, max_length: None, revision: 0 };
    scope.children = vec![input.clone()];
    root.reconcile(&[scope, sibling]).unwrap();
    assert_eq!(root.focused(), None);
    root.accept_event(&PlatformEvent { node: input.id, payload: EventPayload::Blur }).unwrap();
}

#[test]
fn file_drop_requires_opt_in_live_target_and_pointer_ancestors() {
    use interaction::{FileDropEvent, FileDropPhase, PointerEvents};
    let mut root = HeadlessRoot::new();
    let mut parent = node(root.id(), NodeContent::Container);
    let mut target = node(root.id(), NodeContent::Container);
    let event = PlatformEvent { node: target.id, payload: EventPayload::FileDrop(FileDropEvent {
        phase: FileDropPhase::Dropped, position: [20.0, 10.0], uris: vec!["file:///tmp/a.txt".into()],
    }) };
    parent.children.push(target.clone()); root.reconcile(&[parent.clone()]).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    target.descriptor.interaction.file_drop = true;
    parent.children = vec![target.clone()]; root.reconcile(&[parent.clone()]).unwrap();
    assert!(root.accept_event(&event).is_ok());
    parent.descriptor.interaction.pointer_events = PointerEvents::None;
    root.reconcile(&[parent]).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
    root.reconcile(&[]).unwrap();
    assert_eq!(root.accept_event(&event), Err(BackendError::UnknownNode(target.id)));
}
