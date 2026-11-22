use jadix_native::{presentation::*, wire};

#[test]
fn shared_selection_roles_preserve_native_semantics_and_tab_stops() {
    for (role, name, tab_stop) in [
        (AccessibleRole::ListBox, "listbox", None),
        (AccessibleRole::ListBoxOption { tab_stop: false }, "option", Some(false)),
        (AccessibleRole::Heading, "heading", None),
    ] {
        let encoded = wire::descriptor(&NodeDescriptor {
            accessible_role: Some(role),
            accessible_name: Some("Emoji".into()),
            content: NodeContent::Container,
            frame: Rect::default(),
            style: Default::default(),
            accessible_selected: None,
            accessible_expanded: None,
            accessible_description: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
        });
        assert_eq!(encoded["role"], name);
        assert_eq!(encoded["tabStop"], serde_json::json!(tab_stop));
        assert_eq!(encoded["name"], "Emoji");
    }
}

#[test]
fn delta_wire_preserves_full_decimal_identity_revision_and_native_operations() {
    let root = RootId::new();
    let parent = NodeId::new(root);
    let child = NodeId::new(root);
    let descriptor = NodeDescriptor {
        content: NodeContent::Text("中文 😀".into()),
        frame: Rect::default(),
        style: Default::default(),
        accessible_role: Some(AccessibleRole::TreeItem),
        accessible_selected: Some(true),
        accessible_expanded: Some(false),
        interaction: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(),
        accessibility: Default::default(),
        accessibility_relations: Default::default(),
        accessible_description: Some("Supplemental 中文 help".into()),
        accessible_name: Some("caption".into()),
    };
    let batch = CommitBatch {
        root,
        previous: u64::MAX - 1,
        revision: u64::MAX,
        operations: vec![
            Operation::Create {
                node: child,
                parent: Some(parent),
                index: 0,
                descriptor: descriptor.clone(),
            },
            Operation::Update {
                node: child,
                descriptor: NodeDescriptor {
                    accessible_description: None,
                    ..descriptor
                },
            },
            Operation::Move {
                node: child,
                parent: None,
                index: 0,
            },
            Operation::Remove { node: parent },
            Operation::Focus { node: None },
        ],
    };
    let encoded: serde_json::Value = serde_json::from_str(&wire::batch(&batch)).unwrap();
    assert_eq!(encoded["protocol"], 2);
    assert_eq!(encoded["root"], root.token());
    assert_eq!(encoded["revision"], u64::MAX.to_string());
    assert_eq!(encoded["previous"], (u64::MAX - 1).to_string());
    assert!(encoded.get("nodes").is_none());
    let operations = encoded["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 5);
    assert_eq!(operations[0]["id"], child.token());
    assert_eq!(operations[0]["parent"], parent.token());
    assert_eq!(operations[0]["descriptor"]["text"], "中文 😀");
    assert_eq!(operations[0]["descriptor"]["role"], "treeitem");
    assert_eq!(operations[0]["descriptor"]["selected"], true);
    assert_eq!(operations[0]["descriptor"]["expanded"], false);
    assert_eq!(operations[0]["descriptor"]["name"], "caption");
    assert_eq!(
        operations[0]["descriptor"]["description"],
        "Supplemental 中文 help"
    );
    assert!(operations[1]["descriptor"]["description"].is_null());
    assert_eq!(operations[2]["op"], "move");
    assert!(operations[2]["parent"].is_null());
    assert!(operations[4]["id"].is_null());
}

#[test]
fn scroll_indicator_insets_round_trip_without_changing_content_padding() {
    let style: style::NodeStyle = serde_json::from_value(serde_json::json!({
        "scrollbar_inset": [12.0, -2.0, 48.0, 6.0], "padding": [3.0, 4.0, 5.0, 6.0]
    })).unwrap();
    assert!(style.is_valid());
    let encoded = wire::descriptor(&NodeDescriptor {
        content: NodeContent::Scroll, style: style.clone(), frame: Rect::default(),
        accessible_role: None, accessible_name: None, accessible_description: None,
        accessible_selected: None, accessible_expanded: None,
        interaction: Default::default(),
        accessibility: Default::default(),
        keyboard: Default::default(),
        navigation_keys: Vec::new(), accessibility_relations: Default::default(),
    });
    assert_eq!(encoded["style"]["scrollbar_inset"], serde_json::json!([12.0, -2.0, 48.0, 6.0]));
    assert_eq!(style.padding, [3.0, 4.0, 5.0, 6.0]);
    for value in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        assert!(!style::NodeStyle { scrollbar_inset: Some([value; 4]), ..style.clone() }.is_valid());
    }
    let reset: style::NodeStyle = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(reset.scrollbar_inset.is_none());
}
