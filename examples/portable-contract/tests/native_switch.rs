use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_native::{
    BackendError, HeadlessRoot, ResourceKind, ResourceLimits, presentation::*, wire,
};

#[test]
fn switch_builder_and_rsx_share_native_semantics() {
    let (_owner, (mut builder, mut rsx)) = create_root(|| {
        let checked = create_signal(true);
        let builder: Element = switch("builder")
            .checked_signal(checked)
            .child(text("Airplane mode"))
            .into();
        let rsx = ui! { <Switch id="rsx" checked={checked}>"Airplane mode"</Switch> };
        (builder, rsx)
    });
    let projection = ElementProjection::new(RootId::new());
    for element in [&mut builder, &mut rsx] {
        let node = projection.snapshot(element).unwrap();
        assert!(
            matches!(node.descriptor.content, NodeContent::Switch { ref label, checked: true, enabled: true, .. } if label == "Airplane mode")
        );
        assert!(node.children.is_empty());
        assert_eq!(wire::descriptor(&node.descriptor)["kind"], "switch");
    }
}

#[test]
fn switch_toggle_updates_one_signal_and_rejects_stale_revisions() {
    let (_owner, (mut element, checked)) = create_root(|| {
        let checked = create_signal(false);
        (
            Element::from(switch("toggle").checked_signal(checked).child(text("Mode"))),
            checked,
        )
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let first = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&first)).unwrap();
    let event = PlatformEvent {
        node: first.id,
        payload: EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    };
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert!(checked.get());
    assert!(root.accept_event(&event).is_err());
    assert!(projection.dispatch(&mut element, &event).is_err());
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(first.id, second.id);
    assert!(matches!(
        second.descriptor.content,
        NodeContent::Switch {
            checked: true,
            revision: 1,
            ..
        }
    ));
    root.reconcile(&[second]).unwrap();
    checked.set(false);
    let third = projection.snapshot(&mut element).unwrap();
    assert!(matches!(
        third.descriptor.content,
        NodeContent::Switch {
            checked: false,
            revision: 2,
            ..
        }
    ));
    root.reconcile(&[third]).unwrap();
    root.reconcile(&[]).unwrap();
    assert_eq!(
        root.accept_event(&event),
        Err(BackendError::UnknownNode(first.id))
    );
}

#[test]
fn disabled_switch_rejects_toggle_and_focus() {
    let (_owner, mut element) =
        create_root(|| Element::from(switch("disabled").disabled(true).child(text("Disabled"))));
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let node = projection.snapshot(&mut element).unwrap();
    root.reconcile(std::slice::from_ref(&node)).unwrap();
    for payload in [
        EventPayload::Focus,
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
    }
    assert!(
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: node.id,
                    payload: EventPayload::Toggle {
                        checked: true,
                        revision: 1
                    }
                }
            )
            .is_err()
    );
}

#[test]
fn switch_capability_and_label_budget_are_explicit() {
    let (_owner, mut element) =
        create_root(|| Element::from(switch("limited").child(text("Mode"))));
    let mut capabilities = capabilities::BackendCapabilities::headless();
    capabilities
        .node_kinds
        .retain(|kind| *kind != capabilities::NodeKind::Switch);
    let mut root = HeadlessRoot::with_capabilities(capabilities);
    let projection = ElementProjection::new(root.id());
    let node = projection.snapshot(&mut element).unwrap();
    assert_eq!(
        root.reconcile(&[node]),
        Err(BackendError::Unsupported(capabilities::NodeKind::Switch))
    );
    let mut limited = HeadlessRoot::with_limits(ResourceLimits {
        text_bytes: 3,
        ..Default::default()
    });
    let mut descriptor = NodeDescriptor {
        content: NodeContent::Switch {
            label: "Mode".into(),
            checked: false,
            enabled: true,
            revision: 0,
        },
        frame: Rect::default(),
        accessible_role: None,
        accessible_selected: None,
        accessible_expanded: None,
        accessible_description: None,
        accessible_name: None,
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
        style: Default::default(),
    };
    let node = PresentationNode {
        id: NodeId::new(limited.id()),
        descriptor: descriptor.clone(),
        children: vec![],
    };
    assert_eq!(
        limited.reconcile(std::slice::from_ref(&node)),
        Err(BackendError::ResourceLimit(ResourceKind::TextBytes))
    );
    if let NodeContent::Switch { label, .. } = &mut descriptor.content {
        *label = "On".into();
    }
    assert!(
        limited
            .reconcile(&[PresentationNode { descriptor, ..node }])
            .unwrap()
    );
}

#[test]
fn switch_schema_validates_controlled_values_and_versioned_toggles() {
    use jadix_shell::schema::Value;
    let schemas = jadix_components::schema::extended_components().unwrap();
    let schema = schemas
        .iter()
        .find(|schema| schema.name == "Switch")
        .unwrap();
    assert!(schema.is_valid());
    assert_eq!(schema.kind, capabilities::NodeKind::Switch);
    assert!(!schema.children);
    let properties = Value::Object(
        [
            ("label".into(), Value::String("Mode".into())),
            ("checked".into(), Value::Bool(true)),
            ("disabled".into(), Value::Bool(false)),
        ]
        .into(),
    );
    assert!(schema.properties.accepts(&properties));
    let event = Value::Object(
        [
            ("checked".into(), Value::Bool(true)),
            ("revision".into(), Value::String("1".into())),
        ]
        .into(),
    );
    assert!(schema.events["toggle"].accepts(&event));
    for fields in [
        vec![("checked", Value::Bool(true))],
        vec![
            ("checked", Value::String("true".into())),
            ("revision", Value::String("1".into())),
        ],
        vec![
            ("checked", Value::Bool(true)),
            ("revision", Value::Number(1.0)),
        ],
    ] {
        let event = Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
        );
        assert!(!schema.events["toggle"].accepts(&event));
    }
    assert!(
        !jadix_components::schema::portable_components()
            .unwrap()
            .iter()
            .any(|schema| schema.name == "Switch")
    );
}
