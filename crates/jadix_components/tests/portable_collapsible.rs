#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{
    authoring::native::{ElementProjection, ProjectionError},
    presentation::*,
};
use jadix_components::prelude::*;
use jadix_native::HeadlessRoot;
use std::{cell::Cell, rc::Rc};

fn nodes(node: &PresentationNode) -> Vec<&PresentationNode> {
    std::iter::once(node)
        .chain(node.children.iter().flat_map(nodes))
        .collect()
}

fn trigger(node: &PresentationNode) -> &PresentationNode {
    nodes(node)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Button { .. }))
        .unwrap()
}

fn editor(node: &PresentationNode) -> Option<&PresentationNode> {
    nodes(node)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Input { .. }))
}

#[test]
fn builder_content_builds_once_and_remounts_with_fresh_identity_and_preserved_text() {
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    let (owner, (mut element, open, draft)) = create_root(|| {
        let open = create_signal(false);
        let draft = create_signal("中文😀".to_string());
        let element = collapsible("details")
            .open(open)
            .trigger(|| Element::new().text("Details"))
            .content(move || {
                observed.set(observed.get() + 1);
                input("draft").value_signal(draft).into()
            })
            .into();
        (element, open, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let closed = projection.snapshot(&mut element).unwrap();
    let trigger_id = trigger(&closed).id;
    assert_eq!(trigger(&closed).descriptor.accessible_expanded, Some(false));
    assert!(editor(&closed).is_none());
    assert_eq!(builds.get(), 0);
    model.reconcile(&[closed]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: trigger_id,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert!(open.get());
    let opened = projection.snapshot(&mut element).unwrap();
    assert_eq!(trigger(&opened).descriptor.accessible_expanded, Some(true));
    let old_editor = editor(&opened).unwrap().id;
    assert_eq!(builds.get(), 1);
    model.reconcile(&[opened]).unwrap();
    open.set(false);
    model
        .reconcile(&[projection.snapshot(&mut element).unwrap()])
        .unwrap();
    assert_eq!(
        projection.dispatch(
            &mut element,
            &PlatformEvent {
                node: old_editor,
                payload: EventPayload::Activate
            }
        ),
        Err(ProjectionError::StaleNode)
    );
    open.set(true);
    let restored = projection.snapshot(&mut element).unwrap();
    let restored_editor = editor(&restored).unwrap();
    assert_ne!(restored_editor.id, old_editor);
    assert!(
        matches!(&restored_editor.descriptor.content, NodeContent::Input { value, .. } if value == &draft.get())
    );
    assert_eq!(trigger(&restored).id, trigger_id);
    assert_eq!(builds.get(), 1);
    model.reconcile(&[restored]).unwrap();
    projection.dispose();
    drop(owner);
}

#[test]
fn rsx_group_switch_waits_for_projected_composition_commit() {
    let (owner, (mut element, other_open, draft)) = create_root(|| {
        let open = create_signal(true);
        let other_open = create_signal(false);
        let draft = create_signal("old".to_string());
        let element = ui! {
            <div>
                <Collapsible id="compose" open={open} exclusive={true} group="editor">
                    <CollapsibleTrigger>"Details"</CollapsibleTrigger>
                    <CollapsibleContent><Input id="draft" value={draft} /></CollapsibleContent>
                </Collapsible>
                <Collapsible id="other" open={other_open} exclusive={true} group="editor">
                    <CollapsibleTrigger>"Other"</CollapsibleTrigger>
                    <CollapsibleContent>"Other content"</CollapsibleContent>
                </Collapsible>
            </div>
        };
        (element, other_open, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let initial = projection.snapshot(&mut element).unwrap();
    let editor = editor(&initial).unwrap();
    let editor_id = editor.id;
    let NodeContent::Input { revision, .. } = editor.descriptor.content else {
        unreachable!()
    };
    model.reconcile(&[initial]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: editor_id,
                payload: EventPayload::Edit {
                    value: "中".into(),
                    revision: revision + 1,
                    selection_utf16: [1, 1],
                    composing_utf16: Some([0, 1]),
                },
            },
        )
        .unwrap();
    other_open.set(true);
    let pending = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&pending).iter().any(|node| node.id == editor_id));
    model.reconcile(&[pending]).unwrap();
    assert_eq!(draft.get(), "old");
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: editor_id,
                payload: EventPayload::Edit {
                    value: "中文😀".into(),
                    revision: revision + 2,
                    selection_utf16: [4, 4],
                    composing_utf16: None,
                },
            },
        )
        .unwrap();
    let committed = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&committed).iter().all(|node| node.id != editor_id));
    model.reconcile(&[committed]).unwrap();
    assert_eq!(draft.get(), "中文😀");
    drop(owner);
}

#[test]
fn disabled_and_sibling_scopes_are_independent_and_incomplete_groups_fail_closed() {
    let (owner, (mut element, first_open, second_open)) = create_root(|| {
        let first_open = create_signal(false);
        let second_open = create_signal(true);
        let element = ui! {
            <div>
                <Collapsible id="first" open={first_open}><CollapsibleTrigger>"First"</CollapsibleTrigger></Collapsible>
                <Collapsible id="second" open={second_open} disabled={true}>
                    <CollapsibleTrigger>"Second"</CollapsibleTrigger>
                    <CollapsibleContent>"Retained content"</CollapsibleContent>
                </Collapsible>
            </div>
        };
        (element, first_open, second_open)
    });
    let projection = ElementProjection::new(RootId::new());
    let initial = projection.snapshot(&mut element).unwrap();
    let buttons: Vec<_> = nodes(&initial)
        .into_iter()
        .filter(|node| matches!(node.descriptor.content, NodeContent::Button { .. }))
        .collect();
    assert_eq!(buttons.len(), 2);
    assert!(matches!(
        buttons[1].descriptor.content,
        NodeContent::Button { enabled: false, .. }
    ));
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: buttons[0].id,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert_eq!(
        projection.dispatch(
            &mut element,
            &PlatformEvent {
                node: buttons[1].id,
                payload: EventPayload::Activate,
            },
        ),
        Err(ProjectionError::InvalidEvent)
    );
    assert!(first_open.get());
    assert!(second_open.get());
    drop(owner);
    for (exclusive, group) in [(true, None), (false, Some("faq".to_string()))] {
        let (owner, mut element) = create_root(|| {
            collapsible_scope(
                "unsupported",
                None,
                false,
                false,
                exclusive,
                group,
                Element::new,
            )
        });
        assert_eq!(
            projection.snapshot(&mut element),
            Err(ProjectionError::Unsupported("native component options"))
        );
        drop(owner);
    }
    let (owner, mut element) = create_root(|| {
        ui! {
            <Collapsible id="default" defaultOpen={true}>
                <CollapsibleTrigger disabled={true}>"Disabled trigger"</CollapsibleTrigger>
                <CollapsibleContent>"Open"</CollapsibleContent>
            </Collapsible>
        }
    });
    let initial = projection.snapshot(&mut element).unwrap();
    assert_eq!(trigger(&initial).descriptor.accessible_expanded, Some(true));
    assert!(matches!(
        trigger(&initial).descriptor.content,
        NodeContent::Button { enabled: false, .. }
    ));
    drop(owner);
}

#[test]
fn exclusive_groups_follow_controlled_state_and_are_isolated_by_owner() {
    let (owner, (mut element, first_open, second_open, independent_open)) = create_root(|| {
        let first_open = create_signal(true);
        let second_open = create_signal(true);
        let independent_open = create_signal(true);
        let first: Element = collapsible("first")
            .open(first_open)
            .exclusive(true)
            .group("faq")
            .trigger(|| Element::new().text("First"))
            .content(|| Element::new().text("One"))
            .into();
        let second = ui! {
            <Collapsible id="second" open={second_open} exclusive={true} group="faq">
                <CollapsibleTrigger>"Second"</CollapsibleTrigger>
                <CollapsibleContent>"Two"</CollapsibleContent>
            </Collapsible>
        };
        let independent: Element = component_root(|| {
            collapsible("independent")
                .open(independent_open)
                .exclusive(true)
                .group("faq")
                .trigger(|| Element::new().text("Independent"))
                .into()
        });
        (
            Element::new().child(first).child(second).child(independent),
            first_open,
            second_open,
            independent_open,
        )
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let initial = projection.snapshot(&mut element).unwrap();
    model.reconcile(&[initial.clone()]).unwrap();
    assert!(first_open.get());
    assert!(!second_open.get());
    assert!(independent_open.get());
    let second_trigger = nodes(&initial).into_iter().find(|node| {
        matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == "Second")
    }).unwrap().id;
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: second_trigger,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    let changed = projection.snapshot(&mut element).unwrap();
    model.reconcile(&[changed]).unwrap();
    assert!(!first_open.get());
    assert!(second_open.get());
    assert!(independent_open.get());
    first_open.set(true);
    assert!(first_open.get());
    assert!(!second_open.get());
    first_open.set(false);
    second_open.set(true);
    assert!(!first_open.get());
    assert!(second_open.get());
    projection.dispose();
    drop(owner);
    assert_eq!(first_open.try_get(), None);
    assert_eq!(second_open.try_get(), None);
    let (owner, mut element) = create_root(|| {
        collapsible("remounted")
            .default_open(true)
            .exclusive(true)
            .group("faq")
            .trigger(|| Element::new().text("Remounted"))
            .into()
    });
    let projection = ElementProjection::new(RootId::new());
    assert_eq!(
        trigger(&projection.snapshot(&mut element).unwrap())
            .descriptor
            .accessible_expanded,
        Some(true)
    );
    drop(owner);
}
