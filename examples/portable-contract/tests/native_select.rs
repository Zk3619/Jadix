use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_native::{
    presentation::*, wire, BackendError, HeadlessRoot, ResourceKind, ResourceLimits,
};

fn option(value: &str, enabled: bool) -> SelectOption {
    SelectOption {
        value: value.into(),
        label: format!("Choice {value}"),
        enabled,
    }
}
fn descriptor(value: &str) -> NodeDescriptor {
    NodeDescriptor {
        content: NodeContent::Select {
            options: vec![option("apple", true), option("blocked", false)],
            value: value.into(),
            placeholder: "Choose".into(),
            enabled: true,
            revision: 0,
        },
        frame: Rect::default(),
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        accessible_name: None,
        style: Default::default(),
    }
}

#[test]
fn builder_and_options_rsx_use_the_same_controlled_selection() {
    let (_owner, (mut builder, mut rsx)) = create_root(|| {
        let value = create_signal("apple".to_string());
        let pairs = vec![("apple".into(), "Apple".into())];
        (
            Element::from(select("builder").options(pairs.clone()).value_signal(value)),
            ui! { <Select id="rsx" options={pairs} value={value} placeholder="Choose" /> },
        )
    });
    let projection = ElementProjection::new(RootId::new());
    for element in [&mut builder, &mut rsx] {
        let node = projection.snapshot(element).unwrap();
        assert!(
            matches!(&node.descriptor.content, NodeContent::Select { value, options, .. } if value == "apple" && options.len() == 1)
        );
        assert!(node.children.is_empty());
        let json = wire::descriptor(&node.descriptor);
        assert_eq!(json["kind"], "select");
        assert_eq!(json["options"][0]["value"], "apple");
    }
}

#[test]
fn native_selection_keeps_identity_and_rejects_stale_disabled_unknown_and_wrong_kind_events() {
    let (_owner, (mut element, value)) = create_root(|| {
        let value = create_signal(String::new());
        (
            Element::from(
                select("fruit")
                    .value_signal(value)
                    .item(SelectItemData::new("apple", "Apple"))
                    .item(SelectItemData::new("blocked", "Blocked").disabled(true)),
            ),
            value,
        )
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let node = projection.snapshot(&mut element).unwrap();
    root.reconcile(&[node.clone()]).unwrap();
    for payload in [
        EventPayload::Select {
            value: "blocked".into(),
            revision: 1,
        },
        EventPayload::Select {
            value: "missing".into(),
            revision: 1,
        },
        EventPayload::Select {
            value: String::new(),
            revision: 1,
        },
        EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    ] {
        let event = PlatformEvent {
            node: node.id,
            payload,
        };
        assert_eq!(root.accept_event(&event), Err(BackendError::InvalidEvent));
        assert!(projection.dispatch(&mut element, &event).is_err());
    }
    let event = PlatformEvent {
        node: node.id,
        payload: EventPayload::Select {
            value: "apple".into(),
            revision: 1,
        },
    };
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert_eq!(value.get(), "apple");
    assert!(root.accept_event(&event).is_err());
    assert!(projection.dispatch(&mut element, &event).is_err());
    let next = projection.snapshot(&mut element).unwrap();
    assert_eq!(next.id, node.id);
    root.reconcile(&[next]).unwrap();
    value.set(String::new());
    let reset = projection.snapshot(&mut element).unwrap();
    assert!(
        matches!(reset.descriptor.content, NodeContent::Select { ref value, revision: 2, .. } if value.is_empty())
    );
    root.reconcile(&[reset]).unwrap();
    root.reconcile(&[]).unwrap();
    assert_eq!(
        root.accept_event(&event),
        Err(BackendError::UnknownNode(node.id))
    );
}

#[test]
fn invalid_options_and_unknown_controlled_values_fail_before_commit() {
    let mut content = descriptor("apple").content;
    assert!(content.is_valid());
    for variant in [
        "unknown",
        "duplicate",
        "empty-value",
        "too-many",
        "too-long",
    ] {
        let mut candidate = content.clone();
        if let NodeContent::Select {
            options,
            value,
            placeholder,
            ..
        } = &mut candidate
        {
            match variant {
                "unknown" => *value = "unknown".into(),
                "duplicate" => options.push(option("apple", true)),
                "empty-value" => options.push(option("", true)),
                "too-many" => {
                    *options = (0..257)
                        .map(|index| option(&index.to_string(), true))
                        .collect()
                }
                _ => *placeholder = "x".repeat(4097),
            }
        }
        assert!(!candidate.is_valid(), "{variant}");
        let mut root = HeadlessRoot::new();
        let node = PresentationNode {
            id: NodeId::new(root.id()),
            descriptor: NodeDescriptor {
                content: candidate,
                ..descriptor("")
            },
            children: vec![],
        };
        assert_eq!(root.reconcile(&[node]), Err(BackendError::InvalidContent));
        assert_eq!(root.revision(), 0);
    }
    if let NodeContent::Select { options, value, .. } = &mut content {
        options.clear();
        value.clear();
    }
    assert!(content.is_valid());
}

#[test]
fn disabled_capability_and_aggregate_label_budget_are_enforced() {
    let mut root = HeadlessRoot::new();
    let mut data = descriptor("apple");
    if let NodeContent::Select { enabled, .. } = &mut data.content {
        *enabled = false;
    }
    let node = PresentationNode {
        id: NodeId::new(root.id()),
        descriptor: data.clone(),
        children: vec![],
    };
    root.reconcile(&[node.clone()]).unwrap();
    for payload in [
        EventPayload::Focus,
        EventPayload::Select {
            value: "apple".into(),
            revision: 1,
        },
    ] {
        assert_eq!(
            root.accept_event(&PlatformEvent {
                node: node.id,
                payload
            }),
            Err(BackendError::InvalidEvent)
        );
    }
    let mut caps = capabilities::BackendCapabilities::headless();
    caps.node_kinds
        .retain(|kind| *kind != capabilities::NodeKind::Select);
    let mut limited = HeadlessRoot::with_capabilities(caps);
    let node = PresentationNode {
        id: NodeId::new(limited.id()),
        descriptor: data.clone(),
        children: vec![],
    };
    assert_eq!(
        limited.reconcile(&[node]),
        Err(BackendError::Unsupported(capabilities::NodeKind::Select))
    );
    let mut limited = HeadlessRoot::with_limits(ResourceLimits {
        text_bytes: 8,
        ..Default::default()
    });
    let node = PresentationNode {
        id: NodeId::new(limited.id()),
        descriptor: data,
        children: vec![],
    };
    assert_eq!(
        limited.reconcile(&[node]),
        Err(BackendError::ResourceLimit(ResourceKind::TextBytes))
    );
}

#[test]
fn select_wire_uses_canonical_string_revisions_and_cannot_be_a_toggle() {
    let mut root = HeadlessRoot::new();
    let node = PresentationNode {
        id: NodeId::new(root.id()),
        descriptor: descriptor(""),
        children: vec![],
    };
    root.reconcile(&[node.clone()]).unwrap();
    for revision in ["01", "-1", "18446744073709551616"] {
        let event = format!(
            r#"{{"root":"{}","node":"{}","payload":{{"kind":"select","value":"apple","revision":"{}"}}}}"#,
            root.id().token(),
            node.id.token(),
            revision
        );
        assert!(wire::decode_event(&event, &root).is_err());
    }
    let event = format!(
        r#"{{"root":"{}","node":"{}","payload":{{"kind":"select","value":"apple","revision":"1"}}}}"#,
        root.id().token(),
        node.id.token()
    );
    root.accept_event(&wire::decode_event(&event, &root).unwrap())
        .unwrap();
}

#[test]
fn uncontrolled_selection_is_isolated_by_owner() {
    let (_first_owner, mut first) = create_root(|| {
        Element::from(
            select("same")
                .default_value("apple")
                .item(SelectItemData::new("apple", "Apple")),
        )
    });
    let (_second_owner, mut second) =
        create_root(|| Element::from(select("same").item(SelectItemData::new("apple", "Apple"))));
    let projection = ElementProjection::new(RootId::new());
    assert!(
        matches!(projection.snapshot(&mut first).unwrap().descriptor.content, NodeContent::Select { value, .. } if value == "apple")
    );
    assert!(
        matches!(projection.snapshot(&mut second).unwrap().descriptor.content, NodeContent::Select { value, .. } if value.is_empty())
    );
}

#[test]
fn select_schema_requires_bounded_options_and_string_event_revisions() {
    use jadix_shell::schema::Value;
    let schemas = jadix_components::schema::extended_components().unwrap();
    let schema = schemas
        .iter()
        .find(|schema| schema.name == "Select")
        .unwrap();
    assert!(schema.is_valid());
    assert_eq!(schema.kind, capabilities::NodeKind::Select);
    assert!(!schema.children);
    let option = Value::Object(
        [
            ("value".into(), Value::String("apple".into())),
            ("label".into(), Value::String("Apple".into())),
            ("enabled".into(), Value::Bool(true)),
        ]
        .into(),
    );
    let properties = Value::Object(
        [
            ("value".into(), Value::String(String::new())),
            ("options".into(), Value::Array(vec![option.clone()])),
        ]
        .into(),
    );
    assert!(schema.properties.accepts(&properties));
    let oversized = Value::Object(
        [
            ("value".into(), Value::String(String::new())),
            ("options".into(), Value::Array(vec![option; 257])),
        ]
        .into(),
    );
    assert!(!schema.properties.accepts(&oversized));
    for revision in [
        Some(Value::String("1".into())),
        Some(Value::Number(1.0)),
        None,
    ] {
        let valid = matches!(revision, Some(Value::String(_)));
        let mut fields =
            std::collections::BTreeMap::from([("value".into(), Value::String("apple".into()))]);
        if let Some(revision) = revision {
            fields.insert("revision".into(), revision);
        }
        assert_eq!(
            schema.events["select"].accepts(&Value::Object(fields)),
            valid
        );
    }
    assert!(!jadix_components::schema::portable_components()
        .unwrap()
        .iter()
        .any(|schema| schema.name == "Select"));
}
