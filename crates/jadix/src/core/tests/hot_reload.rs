use super::*;

fn node(name: &str, key: Option<&str>, children: Vec<TemplateNode>) -> TemplateNode {
    TemplateNode::Element {
        name: name.into(),
        key: key.map(String::from),
        attributes: Vec::new(),
        children,
    }
}
fn leaf(key: &str) -> Element {
    Element::new()
        .hot_reload_node_type("Box")
        .hot_reload_declaration_base(&[], false)
        .template_id(key)
}
fn registered(tree: &TemplateNode, children: Vec<Element>) -> Element {
    let mut root = Element::new()
        .hot_reload_node_type("Box")
        .hot_reload_declaration_base(&[], false)
        .children(children);
    assert!(bind(&mut root, tree));
    root
}

#[test]
fn structural_reorder_nesting_add_remove_preserves_keys_slots_and_root() {
    let slot = TemplateNode::Slot {
        fingerprint: "{ captured_handler }".into(),
    };
    let before = node(
        "Box",
        None,
        vec![
            node("Box", Some("first"), vec![]),
            slot.clone(),
            node("Box", Some("last"), vec![]),
        ],
    );
    let mut captured = Element::new().text("state");
    captured.set_attr("capture-owner", "original");
    let mut root = registered(&before, vec![leaf("first"), captured, leaf("last")]);
    let identities = instances(&root);
    let root_id = root.inspector_instance_id;
    let first_id = root.children[0].inspector_instance_id;
    let slot_id = root.children[1].inspector_instance_id;
    let removed_id = root.children[2].inspector_instance_id;
    let after = node(
        "Box",
        None,
        vec![
            node("HStack", Some("wrapper"), vec![slot]),
            node("Box", Some("first"), vec![]),
            TemplateNode::Text {
                value: "added".into(),
            },
        ],
    );
    let prepared = prepare(&root, &after).unwrap();
    apply_prepared(&mut root, &after, prepared, |_, _| {});
    assert_eq!(root.inspector_instance_id, root_id);
    assert_eq!(root.children[0].children[0].inspector_instance_id, slot_id);
    assert_eq!(
        root.children[0].children[0]
            .attrs
            .get("capture-owner")
            .map(String::as_str),
        Some("original")
    );
    assert_eq!(root.children[1].inspector_instance_id, first_id);
    assert_eq!(root.children[2].text.as_deref(), Some("added"));
    let changes = changes(&identities, &root);
    assert_eq!(
        changes
            .iter()
            .find(|change| change.instance == slot_id)
            .unwrap()
            .after,
        Some(vec![0, 0])
    );
    assert!(changes
        .iter()
        .find(|change| change.instance == removed_id)
        .unwrap()
        .after
        .is_none());
}

#[test]
fn data_only_changes_keep_runtime_identity_and_classify_paint_layout_and_structure() {
    let before = node(
        "Box",
        None,
        vec![TemplateNode::Text {
            value: "same".into(),
        }],
    );
    let mut root = registered(&before, vec![Element::new().text("same")]);
    let child_id = root.children[0].inspector_instance_id;
    let mut after = before.clone();
    let TemplateNode::Element { attributes, .. } = &mut after else {
        unreachable!()
    };
    attributes.push(("bg".into(), "red".into()));
    assert_eq!(
        impact(&root, &after),
        crate::core::tree::LayoutStatus::StyleChanged
    );
    let prepared = prepare(&root, &after).unwrap();
    apply_prepared(&mut root, &after, prepared, |_, _| {});
    assert_eq!(root.children[0].inspector_instance_id, child_id);
    let TemplateNode::Element { children, .. } = &mut after else {
        unreachable!()
    };
    children[0] = TemplateNode::Text {
        value: "longer".into(),
    };
    assert_eq!(
        impact(&root, &after),
        crate::core::tree::LayoutStatus::LayoutChanged
    );
    let TemplateNode::Element { children, .. } = &mut after else {
        unreachable!()
    };
    children.push(node("Box", Some("new"), vec![]));
    assert_eq!(
        impact(&root, &after),
        crate::core::tree::LayoutStatus::StructureChanged
    );
}

#[test]
fn structural_preflight_rejects_unknown_factory_duplicates_and_changed_capture() {
    let before = node(
        "Box",
        None,
        vec![TemplateNode::Slot {
            fingerprint: "{ captured }".into(),
        }],
    );
    let root = registered(&before, vec![Element::new().text("unchanged")]);
    let changed = node(
        "Box",
        None,
        vec![TemplateNode::Slot {
            fingerprint: "{ different }".into(),
        }],
    );
    assert!(preflight(&root, &changed).is_err());
    let unknown = node(
        "Box",
        None,
        vec![
            node("UnregisteredComponent", Some("new"), vec![]),
            before.clone(),
        ],
    );
    assert!(preflight(&root, &unknown).is_err());
    let duplicate = node("Box", None, vec![before.clone(), before]);
    assert!(preflight(&root, &duplicate).is_err());
    assert_eq!(root.children[0].text.as_deref(), Some("unchanged"));
}

#[test]
fn registered_factory_is_not_called_in_preflight_or_for_reused_instances() {
    thread_local! { static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    fn component() -> Element {
        CALLS.with(|calls| calls.set(calls.get() + 1));
        Element::new()
    }
    register_factory("HrTestCard", 1, component).unwrap();
    let before = node("Box", None, vec![]);
    let mut root = registered(&before, vec![]);
    let after = node("Box", None, vec![node("HrTestCard", Some("card"), vec![])]);
    preflight(&root, &after).unwrap();
    CALLS.with(|calls| assert_eq!(calls.get(), 0));
    let prepared = prepare(&root, &after).unwrap();
    apply_prepared(&mut root, &after, prepared, |_, _| {});
    CALLS.with(|calls| assert_eq!(calls.get(), 1));
    let prepared = prepare(&root, &after).unwrap();
    apply_prepared(&mut root, &after, prepared, |_, _| {});
    CALLS.with(|calls| assert_eq!(calls.get(), 1));
    assert!(register_factory("HrTestCard", 2, component).is_err());
}

#[test]
fn hot_reload_opaque_capture_survives_moves_and_drops_once_on_unmount() {
    struct Capture(std::rc::Rc<std::cell::Cell<usize>>);
    impl Drop for Capture {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = std::rc::Rc::new(std::cell::Cell::new(0));
    let capture = Capture(drops.clone());
    let slot = TemplateNode::Slot {
        fingerprint: "compiled-event-capture".into(),
    };
    let original = node("Box", None, vec![slot.clone()]);
    let callback = Element::new().on_click(move || {
        let _ = &capture;
    });
    let mut root = registered(&original, vec![callback]);
    let identity = root.children[0].inspector_instance_id;
    for index in 0..24 {
        let next = node(
            "Box",
            None,
            vec![node(
                if index % 2 == 0 { "HStack" } else { "VStack" },
                Some("wrapper"),
                vec![slot.clone()],
            )],
        );
        let prepared = prepare(&root, &next).unwrap();
        apply_prepared(&mut root, &next, prepared, |_, _| {});
        assert_eq!(root.children[0].children[0].inspector_instance_id, identity);
        assert_eq!(drops.get(), 0);
    }
    drop(root);
    assert_eq!(drops.get(), 1);
}

#[test]
fn failed_factory_leaves_the_existing_tree_and_owner_untouched() {
    register_factory("HrTestBroken", 1, || {
        Element::new().children(vec![Element::new()])
    })
    .unwrap();
    let before = node("Box", None, vec![node("Box", Some("kept"), vec![])]);
    let root = registered(&before, vec![leaf("kept")]);
    let identity = root.children[0].inspector_instance_id;
    let after = node(
        "Box",
        None,
        vec![node("HrTestBroken", Some("broken"), vec![])],
    );
    assert!(prepare(&root, &after).is_err());
    assert_eq!(root.children[0].inspector_instance_id, identity);
}
