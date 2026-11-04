use jadix_shell::{handles::Handle, schema::Value, wire::*};

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::default();
    let schemas =
        serde_json::from_str::<Vec<jadix_core::presentation::component::ComponentSchema>>(
            include_str!("../../jadix_components/schema/portable-v1.json"),
        )
        .unwrap();
    for schema in schemas {
        registry.register(schema).unwrap();
    }
    registry
}

#[test]
fn actual_bundled_tsx_fixture_is_accepted_by_rust_component_contract() {
    let envelope =
        Envelope::decode(include_bytes!("fixtures/tsx-panel.json"), &registry()).unwrap();
    let Command::Mount { tree, .. } = envelope.command else {
        panic!("mount fixture");
    };
    assert_eq!(tree.component, "Box");
    assert_eq!(tree.children.len(), 3);
    assert_eq!(tree.children[1].component, "Button");
}

#[test]
fn product_schema_is_single_source_for_registered_component_wire_roundtrip() {
    let registry = registry();
    let input = br#"{"protocol":1,"session":"1","command":{"type":"mount","root":"2","tree":{"component":"Button","props":{"label":"Continue"},"events":{"activate":{"session":"1","sequence":"3"}}}}}"#;
    let decoded = Envelope::decode(input, &registry).unwrap();
    assert_eq!(
        Envelope::decode(&decoded.encode(&registry).unwrap(), &registry).unwrap(),
        decoded
    );
    let changed = String::from_utf8(input.to_vec())
        .unwrap()
        .replace("Button", "Missing");
    assert_eq!(
        Envelope::decode(changed.as_bytes(), &registry),
        Err(WireError::Component)
    );
}

#[test]
fn unknown_properties_children_and_duplicate_keys_fail_closed() {
    let registry = registry();
    for input in [
        r#"{"protocol":1,"session":"1","command":{"type":"mount","root":"2","tree":{"component":"Text","props":{"text":"ok","mystery":true}}}}"#,
        r#"{"protocol":1,"session":"1","command":{"type":"mount","root":"2","tree":{"component":"Text","props":{"text":"ok","text":"other"}}}}"#,
        r#"{"protocol":1,"session":"1","command":{"type":"call","request":"1","method":"save","arguments":{"x":1,"x":2}}}"#,
        r#"{"protocol":1,"session":"1","command":{"type":"call","request":"01","method":"save","arguments":null}}"#,
        r#"{"protocol":1,"session":"1","command":{"type":"write_state","handle":{"session":"2","sequence":"1"},"value":null}}"#,
    ] {
        assert!(
            Envelope::decode(input.as_bytes(), &registry).is_err(),
            "{input}"
        );
    }
}

#[test]
fn nonfinite_values_oversize_and_protocol_mismatch_are_not_serialized() {
    let registry = registry();
    let mut envelope = Envelope {
        protocol: 1,
        session: "1".into(),
        command: Command::WriteState {
            handle: Handle {
                session: "1".into(),
                sequence: "1".into(),
            },
            value: Value::Number(f64::NAN),
        },
    };
    assert!(envelope.encode(&registry).is_err());
    envelope.protocol = 99;
    assert_eq!(envelope.encode(&registry), Err(WireError::Version));
    assert_eq!(
        Envelope::decode(&vec![b' '; MAX_MESSAGE_BYTES + 1], &registry),
        Err(WireError::Limit)
    );
    assert!(serde_json::to_vec(&Value::Number(f64::INFINITY)).is_err());
}
