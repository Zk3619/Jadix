use jadix_native::{presentation::*, wire};

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
                descriptor,
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
    assert_eq!(operations[2]["op"], "move");
    assert!(operations[2]["parent"].is_null());
    assert!(operations[4]["id"].is_null());
}
