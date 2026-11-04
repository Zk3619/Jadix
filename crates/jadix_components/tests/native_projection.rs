use jadix::authoring::native::{ElementProjection, ProjectionError};
use jadix::core::reactivity::create_root;
use jadix_components::prelude::*;
use jadix_native::{
    presentation::{hybrid::HybridCoordinator, *},
    HeadlessRoot,
};
use jadix_shell::{PackageManifest, Session};
use std::{cell::Cell, collections::BTreeSet, rc::Rc};

#[test]
fn actual_builders_layout_once_and_drive_native_event_under_one_session_owner() {
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    let mut session = Session::new(4);
    session
        .verify(PackageManifest {
            package: "counter".into(),
            protocol_version: PROTOCOL_VERSION,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    let (mut element, count) = session
        .load(|| {
            observed.set(observed.get() + 1);
            let count = create_signal(0usize);
            let element: Element = vstack()
                .w(300.0)
                .h(200.0)
                .child(
                    button("native-counter")
                        .text("Increment")
                        .w(120.0)
                        .h(40.0)
                        .on_click(move || count.set(count.get() + 1)),
                )
                .into();
            Ok((element, count))
        })
        .unwrap();
    let mut layout = jadix::core::tree::LayoutTree::new();
    layout.build_and_layout(&mut element, 300.0, 200.0, [0.0, 0.0]);
    let lifetime = session.mount_root().unwrap();
    let mut root = HeadlessRoot::with_lifetime(
        lifetime.clone(),
        capabilities::BackendCapabilities::headless(),
    );
    let projection = ElementProjection::with_lifetime(lifetime.clone());
    let snapshot = projection.snapshot(&mut element).unwrap();
    assert_eq!(snapshot.children.len(), 1);
    assert_eq!(
        snapshot.children[0].descriptor.content,
        NodeContent::Button {
            label: "Increment".into(),
            enabled: true
        }
    );
    assert!(snapshot.children[0].children.is_empty());
    assert!(
        snapshot.descriptor.style.is_valid(),
        "root: {:?}",
        snapshot.descriptor.style
    );
    assert!(
        snapshot.children[0].descriptor.style.is_valid(),
        "button: {:?}",
        snapshot.children[0].descriptor.style
    );
    assert!(root.reconcile(&[snapshot.clone()]).unwrap());
    assert!(!root
        .reconcile(&[projection.snapshot(&mut element).unwrap()])
        .unwrap());
    let event = PlatformEvent {
        node: snapshot.children[0].id,
        payload: EventPayload::Activate,
    };
    session.activate().unwrap();
    root.accept_event(&event).unwrap();
    projection.dispatch(&mut element, &event).unwrap();
    assert_eq!(count.get(), 1);
    assert_eq!(builds.get(), 1);
    let mut hybrid = HybridCoordinator::with_lifetime(lifetime.clone());
    assert!(hybrid.stage(1, projection.geometry(&snapshot, 1).unwrap()));
    assert!(!hybrid.confirm(1, true, false));
    assert!(hybrid.hit(2.0, 2.0).is_none());
    assert!(hybrid.confirm(1, false, true));
    let frame = snapshot.children[0].descriptor.frame;
    assert_eq!(
        hybrid.hit(frame.x + 1.0, frame.y + 1.0).unwrap().node,
        event.node
    );
    assert!(hybrid.focus(1, event.node));
    let removed = element.children.remove(0);
    root.reconcile(&[projection.snapshot(&mut element).unwrap()])
        .unwrap();
    assert_eq!(
        projection.dispatch(&mut element, &event),
        Err(ProjectionError::StaleNode)
    );
    drop(removed);
    session.dispose();
    assert!(lifetime.is_disposed());
    assert!(root.roots().is_empty());
    assert!(hybrid.focused().is_none());
    assert!(hybrid.hit(frame.x + 1.0, frame.y + 1.0).is_none());
    assert_eq!(
        projection.dispatch(&mut element, &event),
        Err(ProjectionError::Disposed)
    );
    assert_eq!(
        projection.snapshot(&mut element),
        Err(ProjectionError::Disposed)
    );
    assert_eq!(
        projection.geometry(&snapshot, 2),
        Err(ProjectionError::Disposed)
    );
}

#[test]
fn session_shutdown_rejects_retained_real_button_callbacks_and_keeps_other_sessions_live() {
    let mut sessions = Vec::new();
    for index in 0..2 {
        let mut session = Session::new(1);
        session
            .verify(PackageManifest {
                package: format!("retained.{index}"),
                protocol_version: PROTOCOL_VERSION,
                requested_permissions: BTreeSet::new(),
            })
            .unwrap();
        session.authorize(&BTreeSet::new()).unwrap();
        let clicks = Rc::new(Cell::new(0));
        let observed = clicks.clone();
        let mut element: Element = session
            .load(|| {
                Ok(button(format!("retained-{index}"))
                    .text("Run")
                    .on_click(move || observed.set(observed.get() + 1))
                    .into())
            })
            .unwrap();
        let lifetime = session.mount_root().unwrap();
        let projection = ElementProjection::with_lifetime(lifetime.clone());
        let snapshot = projection.snapshot(&mut element).unwrap();
        let mut root = HeadlessRoot::with_lifetime(
            lifetime.clone(),
            capabilities::BackendCapabilities::headless(),
        );
        root.reconcile(std::slice::from_ref(&snapshot)).unwrap();
        session.activate().unwrap();
        sessions.push((session, element, projection, root, snapshot, clicks));
    }
    for (_, element, projection, root, snapshot, clicks) in &mut sessions {
        let event = PlatformEvent {
            node: snapshot.id,
            payload: EventPayload::Activate,
        };
        root.accept_event(&event).unwrap();
        projection.dispatch(element, &event).unwrap();
        assert_eq!(clicks.get(), 1);
    }
    sessions[0].0.fault().unwrap();
    let (_, element, projection, root, snapshot, clicks) = &mut sessions[0];
    let late = PlatformEvent {
        node: snapshot.id,
        payload: EventPayload::Activate,
    };
    assert_eq!(
        projection.dispatch(element, &late),
        Err(ProjectionError::Disposed)
    );
    assert_eq!(
        root.accept_event(&late),
        Err(jadix_native::BackendError::Disposed)
    );
    assert_eq!(clicks.get(), 1);
    let (session, element, projection, root, snapshot, clicks) = &mut sessions[1];
    let live = PlatformEvent {
        node: snapshot.id,
        payload: EventPayload::Activate,
    };
    root.accept_event(&live).unwrap();
    projection.dispatch(element, &live).unwrap();
    assert_eq!(clicks.get(), 2);
    session.dispose();
    assert_eq!(
        projection.dispatch(element, &live),
        Err(ProjectionError::Disposed)
    );
}

#[test]
fn populated_framework_portal_layer_requires_explicit_composition_support() {
    let mut element = Element::new().child(
        Element::new()
            .template_id(jadix::layout::layout_tree::PORTAL_LAYER_ID)
            .child(Element::new().text("overlay")),
    );
    let projection = ElementProjection::new(RootId::new());
    assert_eq!(
        projection.snapshot(&mut element),
        Err(ProjectionError::Unsupported("portal composition"))
    );
}

#[test]
fn input_composition_uses_existing_controlled_signal_and_monotonic_revisions() {
    let (_owner, (mut element, value)) = create_root(|| {
        let value = create_signal("initial".to_string());
        let element: Element = input("native-input")
            .value_signal(value)
            .placeholder("Name")
            .into();
        (element, value)
    });
    element.layout_bounds = [0.0, 0.0, 180.0, 40.0];
    let root = RootId::new();
    let projection = ElementProjection::new(root);
    let snapshot = projection.snapshot(&mut element).unwrap();
    let draft = PlatformEvent {
        node: snapshot.id,
        payload: EventPayload::Edit {
            value: "ni".into(),
            revision: 1,
            selection_utf16: [2, 2],
            composing_utf16: Some([0, 2]),
        },
    };
    projection.dispatch(&mut element, &draft).unwrap();
    assert_eq!(value.get(), "initial");
    value.set("external during composition".into());
    assert!(
        matches!(projection.snapshot(&mut element).unwrap().descriptor.content, NodeContent::Input { value, revision: 1, .. } if value == "ni")
    );
    let commit = PlatformEvent {
        node: snapshot.id,
        payload: EventPayload::Edit {
            value: "\u{4f60}\u{1f600}".into(),
            revision: 2,
            selection_utf16: [3, 3],
            composing_utf16: None,
        },
    };
    projection.dispatch(&mut element, &commit).unwrap();
    assert_eq!(value.get(), "\u{4f60}\u{1f600}");
    assert_eq!(
        projection.dispatch(&mut element, &draft),
        Err(ProjectionError::InvalidEvent)
    );
    let mut invalid = commit.clone();
    invalid.payload = EventPayload::Edit {
        value: "\u{1f600}".into(),
        revision: 3,
        selection_utf16: [1, 1],
        composing_utf16: None,
    };
    assert_eq!(
        projection.dispatch(&mut element, &invalid),
        Err(ProjectionError::InvalidEvent)
    );
}

#[test]
fn retained_element_reorder_keeps_ids_and_static_skeleton_gets_new_ids() {
    let (_owner, mut element) = create_root(|| {
        Element::new()
            .child(Element::from(button("first").text("First")))
            .child(Element::from(button("second").text("Second")))
    });
    let mut root = HeadlessRoot::new();
    let projection = ElementProjection::new(root.id());
    let first = projection.snapshot(&mut element).unwrap();
    root.reconcile(&[first.clone()]).unwrap();
    element.children.swap(0, 1);
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(second.children[0].id, first.children[1].id);
    assert!(root.reconcile(&[second]).unwrap());
    let mut cloned = element.clone_static_skeleton();
    let copy = projection.snapshot(&mut cloned).unwrap();
    assert_ne!(copy.children[0].id, first.children[1].id);
    assert!(matches!(
        copy.children[0].descriptor.content,
        NodeContent::Button { .. }
    ));
}

#[test]
fn unsupported_elements_and_transforms_fail_before_a_native_commit() {
    let projection = ElementProjection::new(RootId::new());
    let mut element = Element::new().on_click(|| {});
    assert!(matches!(
        projection.snapshot(&mut element),
        Err(ProjectionError::Unsupported(_))
    ));
    let mut hover = Element::new().on_hover(|_| {});
    assert!(matches!(
        projection.snapshot(&mut hover),
        Err(ProjectionError::Unsupported(_))
    ));
    let mut transparent = Element::new().pointer_events_none();
    assert!(matches!(
        projection.snapshot(&mut transparent),
        Err(ProjectionError::Unsupported(_))
    ));
    let mut drawing = Element::new();
    drawing.set_attr(jadix::primitives::canvas::ATTR_CANVAS_PAINT, "1");
    assert!(matches!(
        projection.snapshot(&mut drawing),
        Err(ProjectionError::Unsupported(_))
    ));
    let mut rotated = Element::new();
    rotated.style.rotate_deg = 45.0;
    assert!(matches!(
        projection.snapshot(&mut rotated),
        Err(ProjectionError::Unsupported(_))
    ));
    let mut plain = Element::new().text("plain");
    let first = projection.snapshot(&mut plain).unwrap();
    assert!(matches!(first.descriptor.content, NodeContent::Text(_)));
    assert!(ElementProjection::new(RootId::new())
        .snapshot(&mut plain)
        .is_err());
}

#[cfg(feature = "macros")]
#[test]
fn rsx_and_builder_keep_the_same_typed_control_semantics() {
    let (_owner, (mut builder, mut rsx)) = create_root(|| {
        let builder: Element = button("builder").text("Save").into();
        let rsx = ui! { <Button id="rsx">"Save"</Button> };
        (builder, rsx)
    });
    let projection = ElementProjection::new(RootId::new());
    let builder_node = projection.snapshot(&mut builder).unwrap();
    let rsx_node = projection.snapshot(&mut rsx).unwrap();
    assert_eq!(builder_node.descriptor.content, rsx_node.descriptor.content);
    for (element, node) in [(&mut builder, builder_node.id), (&mut rsx, rsx_node.id)] {
        projection
            .dispatch(
                element,
                &PlatformEvent {
                    node,
                    payload: EventPayload::Activate,
                },
            )
            .unwrap();
    }
}
