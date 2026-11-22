use std::{cell::RefCell, rc::Rc};

use jadix_native::{
    host::NativeRoot, miniprogram::MiniProgramHost, presentation::*, wire, BackendError,
};

fn button(root: RootId) -> PresentationNode {
    PresentationNode {
        id: NodeId::new(root),
        descriptor: NodeDescriptor {
            content: NodeContent::Button {
                label: "中😀".into(),
                enabled: true,
            },
            frame: Rect::default(),
            accessible_description: None,
            accessible_name: Some("Count".into()),
            accessible_role: None,
            accessible_selected: None,
            accessible_expanded: None,
            interaction: Default::default(),
            keyboard: Default::default(),
            navigation_keys: Vec::new(),
            accessibility: Default::default(),
            accessibility_relations: Default::default(),
            style: Default::default(),
        },
        children: vec![],
    }
}

#[test]
fn miniprogram_measurement_sends_shared_grapheme_boundaries() {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let captured = requests.clone();
    let host = MiniProgramHost::new(
        move |request| {
            captured
                .borrow_mut()
                .push(serde_json::from_str::<serde_json::Value>(request).unwrap());
            Ok([40.0, 20.0])
        },
        |_| Ok(()),
        || {},
    );
    let mut root = NativeRoot::new(host);
    let mut node = button(root.model().id());
    let text = "e\u{301}👩‍💻🇨🇳\n\n";
    node.descriptor.content = NodeContent::Text(text.into());
    root.present(&mut [node.clone()], [100.0, 100.0], 0)
        .unwrap();
    node.descriptor.content = NodeContent::Textarea {
        value: String::new(),
        placeholder: text.into(),
        enabled: true,
        revision: 0,
        max_length: None,
    };
    node.id = NodeId::new(root.model().id());
    root.present(&mut [node], [100.0, 100.0], 0).unwrap();
    let requests = requests.borrow();
    for kind in ["text", "textarea"] {
        let request = requests
            .iter()
            .find(|request| request["node"]["kind"] == kind)
            .unwrap();
        assert_eq!(
            request["wrapParagraphs"],
            serde_json::json!([["e\u{301}", "👩‍💻", "🇨🇳"], [], []])
        );
    }
}

#[test]
fn miniprogram_uses_shared_layout_revisions_and_revokes_late_events() {
    let batches = Rc::new(RefCell::new(Vec::new()));
    let output = batches.clone();
    let disposals = Rc::new(RefCell::new(0));
    let closed = disposals.clone();
    let host = MiniProgramHost::new(
        |request| {
            let request: serde_json::Value = serde_json::from_str(request).unwrap();
            assert_eq!(request["node"]["text"], "中😀");
            Ok([80.0, 32.0])
        },
        move |batch| {
            output.borrow_mut().push(batch.to_owned());
            Ok(())
        },
        move || *closed.borrow_mut() += 1,
    );
    let mut root = NativeRoot::new(host);
    let mut nodes = vec![button(root.model().id())];
    root.present(&mut nodes, [320.0, 480.0], 0).unwrap();
    let batch: serde_json::Value = serde_json::from_str(&batches.borrow()[0]).unwrap();
    assert_eq!(batch["protocol"], 2);
    assert_eq!(batch["revision"], "1");
    assert_eq!(
        batch["operations"][0]["descriptor"]["frame"][2],
        nodes[0].descriptor.frame.width
    );
    root.present(&mut nodes, [320.0, 480.0], 0).unwrap();
    assert_eq!(batches.borrow().len(), 1);
    let event = serde_json::json!({"root": root.model().id().token(), "node": nodes[0].id.token(), "payload": {"kind": "activate"}}).to_string();
    root.event(&wire::decode_event(&event, root.model()).unwrap())
        .unwrap();
    let removed = nodes[0].id;
    root.present(&mut [], [320.0, 480.0], 0).unwrap();
    assert!(wire::decode_event(&event, root.model()).is_err());
    assert_eq!(
        root.event(&PlatformEvent {
            node: removed,
            payload: EventPayload::Activate
        }),
        Err(BackendError::UnknownNode(removed))
    );
    root.dispose();
    root.dispose();
    assert_eq!(*disposals.borrow(), 1);
    assert_eq!(
        wire::decode_event(&event, root.model()),
        Err(BackendError::Disposed)
    );
}

#[test]
fn miniprogram_wire_preserves_shared_layout_direction() {
    let batches = Rc::new(RefCell::new(Vec::new()));
    let output = batches.clone();
    let host = MiniProgramHost::new(
        |_| Ok([40.0, 20.0]),
        move |batch| {
            output.borrow_mut().push(batch.to_owned());
            Ok(())
        },
        || {},
    );
    let mut root = NativeRoot::new(host);
    let mut node = button(root.model().id());
    node.descriptor.style.direction = Some(jadix_native::environment::LayoutDirection::RightToLeft);
    root.present(&mut [node], [100.0, 40.0], 0).unwrap();
    let batch: serde_json::Value = serde_json::from_str(&batches.borrow()[0]).unwrap();
    assert_eq!(
        batch["operations"][0]["descriptor"]["style"]["direction"],
        "right-to-left"
    );
}

#[test]
fn rejected_publication_disposes_the_surface_and_canonical_tokens_never_alias() {
    let root_id = RootId::new();
    let node = NodeId::new(root_id);
    assert_eq!(NodeId::from_token(root_id, &node.token()), Some(node));
    for token in ["0", "01", "+1", " 1", "1 ", "-1", "18446744073709551616"] {
        assert!(NodeId::from_token(root_id, token).is_none());
    }
    let disposed = Rc::new(RefCell::new(false));
    let closed = disposed.clone();
    let host = MiniProgramHost::new(
        |_| Ok([80.0, 32.0]),
        |_| Err("setData rejected".into()),
        move || *closed.borrow_mut() = true,
    );
    let mut root = NativeRoot::new(host);
    let mut nodes = vec![button(root.model().id())];
    assert!(root.present(&mut nodes, [320.0, 480.0], 0).is_err());
    assert!(*disposed.borrow());
    assert!(root.model().lifetime().is_disposed());
}

#[test]
fn scroll_commands_bind_the_accepted_root_and_stop_after_disposal() {
    let batches = Rc::new(RefCell::new(Vec::new()));
    let output = batches.clone();
    let host = MiniProgramHost::new(
        |_| Ok([80.0, 32.0]),
        move |batch| {
            output.borrow_mut().push(batch.to_owned());
            Ok(())
        },
        || {},
    );
    let mut root = NativeRoot::new(host);
    let mut scroll = button(root.model().id());
    scroll.descriptor.content = NodeContent::Scroll;
    let mut nodes = vec![scroll];
    root.present(&mut nodes, [320.0, 480.0], 0).unwrap();
    let node = nodes[0].id;
    root.scroll_to(node, [12.0, 96.0]).unwrap();
    let command: serde_json::Value = serde_json::from_str(&batches.borrow()[1]).unwrap();
    assert_eq!(command["command"], "scroll");
    assert_eq!(command["root"], root.model().id().token());
    assert_eq!(command["revision"], "1");
    assert_eq!(command["node"], node.token());
    assert_eq!(command["offset"], serde_json::json!([12.0, 96.0]));
    assert!(root
        .scroll_to(NodeId::new(RootId::new()), [0.0, 0.0])
        .is_err());
    assert!(root.scroll_to(node, [f64::NAN, 0.0]).is_err());
    assert!(root.scroll_to(node, [0.0, -1.0]).is_err());
    root.dispose();
    assert!(root.scroll_to(node, [0.0, 0.0]).is_err());
    assert_eq!(batches.borrow().len(), 2);
}

#[test]
fn invalid_canvas_measurements_never_reach_publication() {
    for width in [f64::NAN, f64::INFINITY, -1.0] {
        let published = Rc::new(RefCell::new(false));
        let output = published.clone();
        let host = MiniProgramHost::new(
            move |_| Ok([width, 32.0]),
            move |_| {
                *output.borrow_mut() = true;
                Ok(())
            },
            || {},
        );
        let mut root = NativeRoot::new(host);
        let mut nodes = vec![button(root.model().id())];
        assert!(root.present(&mut nodes, [320.0, 480.0], 0).is_err());
        assert!(!*published.borrow());
    }
}
