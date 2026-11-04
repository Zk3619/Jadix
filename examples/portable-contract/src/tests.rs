use super::*;
use jadix_components::authoring::native::ProjectionError;
use jadix_shell::{PackageManifest, Session};
use std::collections::BTreeSet;

#[test]
fn password_authoring_and_wire_keep_masking_separate_from_edit_values() {
    let (owner, (mut builder, mut rsx)) = create_root(|| {
        let value = create_signal(String::from("test-only"));
        let builder: Element = input("secret-builder").password(true).value_signal(value).into();
        let rsx = ui! { <Input id="secret-rsx" password=true value={value} /> };
        (builder, rsx)
    });
    let projection = ElementProjection::new(RootId::new());
    for element in [&mut builder, &mut rsx] {
        let node = projection.snapshot(element).unwrap();
        assert!(matches!(node.descriptor.content, NodeContent::Input { password: true, .. }));
        let wire = jadix_native::wire::descriptor(&node.descriptor);
        assert_eq!(wire["password"], true);
        assert_eq!(wire["text"], "test-only");
        assert_eq!(node.descriptor.content, NodeContent::Input {
            value: "test-only".into(), placeholder: String::new(), enabled: true,
            password: true, revision: 1,
        });
    }
    drop(owner);
}

#[test]
fn dialog_rsx_lazily_builds_once_restores_context_and_reuses_real_controls() {
    use jadix_components::dialog_close;
    let (owner, (mut element, open, value)) = create_root(|| {
        let open = create_signal(false);
        let value = create_signal(String::new());
        let element = ui! {
            <Dialog id="edit" open={open}>
                <DialogTrigger><Button id="show">Open editor</Button></DialogTrigger>
                <DialogContent>
                    <DialogHeader><DialogTitle>Edit details</DialogTitle></DialogHeader>
                    <Input id="modal-name" value={value} />
                    <Button id="done" on_click={dialog_close}>Done</Button>
                </DialogContent>
            </Dialog>
        };
        (element, open, value)
    });
    fn flatten(node: &PresentationNode) -> Vec<&PresentationNode> {
        let mut nodes = vec![node];
        for child in &node.children {
            nodes.extend(flatten(child));
        }
        nodes
    }
    let projection = ElementProjection::new(RootId::new());
    let closed = projection.snapshot(&mut element).unwrap();
    assert!(!flatten(&closed)
        .iter()
        .any(|node| matches!(node.descriptor.content, NodeContent::Input { .. })));
    let trigger = flatten(&closed).into_iter().find(|node| matches!(&node.descriptor.content,NodeContent::Button{label,..} if label=="Open editor")).unwrap().id;
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: trigger,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert!(open.get());
    let opened = projection.snapshot(&mut element).unwrap();
    assert!(flatten(&opened)
        .iter()
        .any(|node| matches!(node.descriptor.content, NodeContent::Input { .. })));
    let done = flatten(&opened).into_iter().find(|node| matches!(&node.descriptor.content,NodeContent::Button{label,..} if label=="Done")).unwrap().id;
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: done,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert!(!open.get());
    projection.snapshot(&mut element).unwrap();
    open.set(true);
    let reopened = projection.snapshot(&mut element).unwrap();
    let original_ids: Vec<_> = flatten(&opened).into_iter().map(|node| node.id).collect();
    assert_eq!(
        original_ids,
        flatten(&reopened)
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>()
    );
    let modal = flatten(&reopened)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Dialog { .. }))
        .unwrap();
    let revision = match modal.descriptor.content {
        NodeContent::Dialog { revision, .. } => revision,
        _ => unreachable!(),
    };
    let dismiss = PlatformEvent {
        node: modal.id,
        payload: EventPayload::Dismiss {
            backdrop: false,
            revision: revision + 1,
        },
    };
    projection.dispatch(&mut element, &dismiss).unwrap();
    assert!(!open.get());
    assert_eq!(
        projection.dispatch(&mut element, &dismiss),
        Err(ProjectionError::InvalidEvent)
    );
    assert_eq!(value.get(), "");
    projection.dispose();
    drop(owner);
}

#[test]
fn native_dialog_rejects_unimplemented_chrome_instead_of_ignoring_it() {
    let (owner, mut element) = create_root(|| {
        ui! { <Dialog id="styled"><DialogContent overlay_color="red">Unsupported chrome</DialogContent></Dialog> }
    });
    let projection = ElementProjection::new(RootId::new());
    assert_eq!(
        projection.snapshot(&mut element),
        Err(ProjectionError::Unsupported("native component options"))
    );
    projection.dispose();
    drop(owner);
}

#[test]
fn same_source_builders_and_rsx_keep_component_contracts_without_gpu() {
    let (owner, (mut builder, mut rsx)) = create_root(|| {
        let count = create_signal(0usize);
        let value = create_signal(String::new());
        (page::builder_page(count, value), page::page(count, value))
    });
    let root = RootId::new();
    let projection = ElementProjection::new(root);
    let builder = projection.snapshot(&mut builder).unwrap();
    let rsx = projection.snapshot(&mut rsx).unwrap();
    assert_eq!(builder.descriptor, rsx.descriptor);
    assert_eq!(builder.children.len(), rsx.children.len());
    for (builder, rsx) in builder.children.iter().zip(rsx.children.iter()) {
        assert_eq!(builder.descriptor, rsx.descriptor);
    }
    drop(owner);
}

#[test]
fn session_owner_callbacks_composition_reset_and_late_events_share_one_runtime() {
    let mut session = Session::new(4);
    session
        .verify(PackageManifest {
            package: "portable.counter".into(),
            protocol_version: PROTOCOL_VERSION,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    let (mut element, count, value) = session
        .load(|| {
            let count = create_signal(0usize);
            let value = create_signal(String::new());
            Ok((page::page(count, value), count, value))
        })
        .unwrap();
    let lifetime = session.mount_root().unwrap();
    let projection = ElementProjection::with_lifetime(lifetime.clone());
    let mut backend = HeadlessRoot::with_lifetime(
        lifetime.clone(),
        capabilities::BackendCapabilities::headless(),
    );
    let node = projection.snapshot(&mut element).unwrap();
    backend.reconcile(std::slice::from_ref(&node)).unwrap();
    session.activate().unwrap();
    let click = PlatformEvent {
        node: node.children[1].id,
        payload: EventPayload::Activate,
    };
    backend.accept_event(&click).unwrap();
    projection.dispatch(&mut element, &click).unwrap();
    assert_eq!(count.get(), 1);
    let input = node.children[2].id;
    let edit = |revision, composing| PlatformEvent {
        node: input,
        payload: EventPayload::Edit {
            value: "中😀".into(),
            revision,
            selection_utf16: [3, 3],
            composing_utf16: composing,
        },
    };
    projection
        .dispatch(&mut element, &edit(1, Some([0, 3])))
        .unwrap();
    assert_eq!(value.get(), "");
    let draft = projection.snapshot(&mut element).unwrap();
    assert!(
        matches!(&draft.children[2].descriptor.content, NodeContent::Input { value, .. } if value == "中😀")
    );
    value.set("reset".into());
    let reset = projection.snapshot(&mut element).unwrap();
    assert!(
        matches!(&reset.children[2].descriptor.content, NodeContent::Input { value, .. } if value == "reset")
    );
    assert_eq!(
        projection.dispatch(&mut element, &edit(1, None)),
        Err(ProjectionError::InvalidEvent)
    );
    projection.dispatch(&mut element, &edit(3, None)).unwrap();
    assert_eq!(value.get(), "中😀");
    assert_eq!(
        projection.dispatch(&mut element, &edit(3, None)),
        Err(ProjectionError::InvalidEvent)
    );
    session.dispose();
    assert!(backend.accept_event(&click).is_err());
    assert_eq!(
        projection.dispatch(&mut element, &click),
        Err(ProjectionError::Disposed)
    );
    assert_eq!(
        projection.snapshot(&mut element),
        Err(ProjectionError::Disposed)
    );
}

#[test]
fn enabled_button_without_callback_accepts_activation_as_a_noop() {
    let (owner, mut element) = create_root(|| button("idle").child(text("Idle")).into());
    let projection = ElementProjection::new(RootId::new());
    let node = projection.snapshot(&mut element).unwrap().id;
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    drop(owner);
}

#[test]
fn dynamic_rsx_text_refreshes_without_rebuilding_component_or_owner() {
    let (owner, (count, mut element)) = create_root(|| {
        let count = create_signal(0usize);
        let element = ui! { <VStack><Text>{count.get()}</Text></VStack> };
        (count, element)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    count.set(7);
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(first.children[0].id, second.children[0].id);
    assert_eq!(
        second.children[0].descriptor.content,
        NodeContent::Text("7".into())
    );
    drop(owner);
}

#[test]
fn extended_rsx_controls_preserve_drafts_and_controlled_state() {
    let (owner, (checked, value, mut element)) = create_root(|| {
        let checked = create_signal(false);
        let value = create_signal(String::new());
        let element = ui! { <VStack><Checkbox id="accept" checked={checked}>"Accept"</Checkbox><Textarea id="note" value={value} placeholder="Note" /></VStack> };
        (checked, value, element)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert!(
        matches!(&first.children[0].descriptor.content, NodeContent::Checkbox { label, checked: false, .. } if label == "Accept")
    );
    let toggle = PlatformEvent {
        node: first.children[0].id,
        payload: EventPayload::Toggle {
            checked: true,
            revision: 1,
        },
    };
    projection.dispatch(&mut element, &toggle).unwrap();
    assert!(checked.get());
    assert!(projection.dispatch(&mut element, &toggle).is_err());
    let draft = PlatformEvent {
        node: first.children[1].id,
        payload: EventPayload::Edit {
            value: "中文😀\n草稿".into(),
            revision: 1,
            selection_utf16: [7, 7],
            composing_utf16: Some([5, 7]),
        },
    };
    projection.dispatch(&mut element, &draft).unwrap();
    assert_eq!(value.get(), "");
    value.set("external reset".into());
    checked.set(false);
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(first.children[0].id, second.children[0].id);
    assert!(
        matches!(&second.children[1].descriptor.content, NodeContent::Textarea { value, .. } if value == "external reset")
    );
    assert!(projection.dispatch(&mut element, &draft).is_err());
    assert!(jadix_components::schema::extended_components()
        .unwrap()
        .iter()
        .all(|schema| schema.is_valid()));
    projection.dispose();
    drop(owner);
}

#[test]
fn embedded_image_rsx_keeps_identity_and_alt_text() {
    let (owner, mut element) = create_root(|| {
        ui! { <Image id="pixel" src={include_str!("../../native-controls/assets/pixel.txt").trim().to_string()} alt="Native image" /> }
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    assert_eq!(first.descriptor.content.image_dimensions(), Some([1, 1]));
    assert!(
        matches!(first.descriptor.content, NodeContent::Image { ref alt, .. } if alt == "Native image")
    );
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(first.id, second.id);
    projection.dispose();
    drop(owner);
}

#[test]
fn portable_callbacks_restore_component_owner_and_context() {
    use jadix_components::core::reactivity::{
        component_root, create_context, provide_context, use_context,
    };
    let context = create_context::<String>();
    let resources = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Signal<usize>>::new()));
    let (owner, mut element) = create_root(|| {
        let mut root = vstack();
        for name in ["first", "second"] {
            let resources = resources.clone();
            root = root.child(component_root(|| {
                provide_context(context, name.to_string());
                button(name)
                    .on_click(move || {
                        assert_eq!(use_context(context).as_deref(), Some(name));
                        resources.borrow_mut().push(create_signal(7));
                    })
                    .child(jadix_components::app::reactive::bind_text(
                        name,
                        move || use_context(context).unwrap(),
                    ))
            }));
        }
        root
    });
    let projection = ElementProjection::new(RootId::new());
    let snapshot = projection.snapshot(&mut element).unwrap();
    for (index, name) in ["first", "second"].into_iter().enumerate() {
        assert!(
            matches!(&snapshot.children[index].descriptor.content, NodeContent::Button { label, .. } if label == name)
        );
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: snapshot.children[index].id,
                    payload: EventPayload::Activate,
                },
            )
            .unwrap();
    }
    assert_eq!(resources.borrow().len(), 2);
    projection.dispose();
    drop(owner);
    assert!(resources
        .borrow()
        .iter()
        .all(|signal| signal.try_get().is_none()));
}
