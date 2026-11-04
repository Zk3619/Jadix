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

fn button(node: &PresentationNode, title: &str) -> NodeId {
    nodes(node).into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == title)).unwrap().id
}

fn editor(node: &PresentationNode) -> &PresentationNode {
    nodes(node)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Input { .. }))
        .unwrap()
}

#[test]
fn builder_navigation_skips_disabled_without_opening_and_remounts_lazy_content() {
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    let (owner, (mut element, selected, draft)) = create_root(|| {
        let selected = create_signal(Vec::<String>::new());
        let draft = create_signal("中文😀".to_string());
        let element = accordion("faq")
            .value_signal(selected)
            .item("account", "Account", move || {
                observed.set(observed.get() + 1);
                input("draft").value_signal(draft).into()
            })
            .item_disabled("disabled", "Unavailable", || {
                Element::new().text("Disabled")
            })
            .item("privacy", "Privacy", || {
                Element::new().text("Privacy content")
            })
            .into();
        (element, selected, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let initial = projection.snapshot(&mut element).unwrap();
    let account = button(&initial, "Account");
    let privacy = button(&initial, "Privacy");
    let disabled = button(&initial, "Unavailable");
    assert_eq!(builds.get(), 0);
    assert_eq!(projection.navigation_nodes(&element).len(), 2);
    model.reconcile(&[initial]).unwrap();
    for (source, key, target) in [
        (account, NavigationKey::Down, privacy),
        (privacy, NavigationKey::Down, account),
        (account, NavigationKey::Up, privacy),
        (privacy, NavigationKey::Home, account),
        (account, NavigationKey::End, privacy),
    ] {
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: source,
                    payload: EventPayload::Navigate {
                        key,
                        shift: false,
                        control: false,
                    },
                },
            )
            .unwrap();
        assert_eq!(projection.take_focus_request(&element), Some(target));
        assert!(selected.get().is_empty());
    }
    assert_eq!(
        projection.dispatch(
            &mut element,
            &PlatformEvent {
                node: disabled,
                payload: EventPayload::Activate,
            },
        ),
        Err(ProjectionError::InvalidEvent)
    );
    assert!(selected.get().is_empty());
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: account,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    let opened = projection.snapshot(&mut element).unwrap();
    let old_editor = editor(&opened).id;
    assert_eq!(selected.get(), vec!["account"]);
    assert_eq!(builds.get(), 1);
    assert_eq!(
        nodes(&opened)
            .into_iter()
            .find(|node| node.id == account)
            .unwrap()
            .descriptor
            .accessible_expanded,
        Some(true)
    );
    model.reconcile(&[opened]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: privacy,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    model
        .reconcile(&[projection.snapshot(&mut element).unwrap()])
        .unwrap();
    assert_eq!(selected.get(), vec!["privacy"]);
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
    selected.set(vec!["account".into()]);
    let restored = projection.snapshot(&mut element).unwrap();
    assert_ne!(editor(&restored).id, old_editor);
    assert!(
        matches!(&editor(&restored).descriptor.content, NodeContent::Input { value, .. } if value == &draft.get())
    );
    assert_eq!(builds.get(), 1);
    model.reconcile(&[restored]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: account,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert!(selected.get().is_empty());
    projection.dispose();
    drop(owner);
}

#[test]
fn rsx_multiple_selection_isolated_scopes_and_composition_commit() {
    let (owner, (mut element, selected, draft)) = create_root(|| {
        let selected = create_signal(vec!["account".to_string()]);
        let draft = create_signal("old".to_string());
        let element = ui! {
            <div>
                <Accordion id="settings" value={selected} multiple={true}>
                    <AccordionItem value="account">
                        <AccordionTrigger>"Account"</AccordionTrigger>
                        <AccordionContent><Input id="draft" value={draft} /></AccordionContent>
                    </AccordionItem>
                    <AccordionItem value="privacy">
                        <AccordionTrigger>"Privacy"</AccordionTrigger>
                        <AccordionContent>"Privacy content"</AccordionContent>
                    </AccordionItem>
                    <AccordionItem value="disabled" disabled={true}>
                        <AccordionTrigger>"Unavailable"</AccordionTrigger>
                    </AccordionItem>
                </Accordion>
                <Accordion id="sibling" disabled={true}>
                    <AccordionItem value="account"><AccordionTrigger>"Sibling"</AccordionTrigger></AccordionItem>
                </Accordion>
            </div>
        };
        (element, selected, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let initial = projection.snapshot(&mut element).unwrap();
    let privacy = button(&initial, "Privacy");
    let editor_id = editor(&initial).id;
    let NodeContent::Input { revision, .. } = editor(&initial).descriptor.content else {
        unreachable!()
    };
    for title in ["Unavailable", "Sibling"] {
        let target = button(&initial, title);
        assert_eq!(
            projection.dispatch(
                &mut element,
                &PlatformEvent {
                    node: target,
                    payload: EventPayload::Activate,
                },
            ),
            Err(ProjectionError::InvalidEvent)
        );
        assert_eq!(selected.get(), vec!["account"]);
    }
    model.reconcile(&[initial]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: privacy,
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert_eq!(selected.get(), vec!["account", "privacy"]);
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
    selected.set(vec!["privacy".into()]);
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
    assert_eq!(draft.get(), "中文😀");
    model.reconcile(&[committed]).unwrap();
    drop(owner);
}

#[test]
fn invalid_value_inventory_is_rejected_before_publication() {
    for (values, multiple, duplicate) in [
        (vec!["missing".into()], false, false),
        (vec!["account".into(), "privacy".into()], false, false),
        (vec!["account".into(), "account".into()], true, false),
        (vec![], false, true),
    ] {
        let (owner, mut element) = create_root(|| {
            let mut builder = accordion("invalid")
                .default_value(values)
                .multiple(multiple)
                .item("account", "Account", Element::new)
                .item("privacy", "Privacy", Element::new);
            if duplicate {
                builder = builder.item("account", "Duplicate", Element::new);
            }
            Element::from(builder)
        });
        let projection = ElementProjection::new(RootId::new());
        assert!(projection.snapshot(&mut element).is_err());
        drop(owner);
    }
}
