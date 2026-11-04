#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{authoring::native::ElementProjection, presentation::*};
use jadix_components::prelude::*;

fn nodes(tree: &PresentationNode) -> Vec<&PresentationNode> {
    std::iter::once(tree)
        .chain(tree.children.iter().flat_map(nodes))
        .collect()
}

fn button(tree: &PresentationNode, title: &str) -> NodeId {
    nodes(tree).into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label, .. } if label == title)).unwrap().id
}

fn editor(tree: &PresentationNode) -> &PresentationNode {
    nodes(tree)
        .into_iter()
        .find(|node| matches!(node.descriptor.content, NodeContent::Input { .. }))
        .unwrap()
}

#[test]
fn disclosure_focus_restores_after_composition_without_stealing_outside_focus() {
    for use_accordion in [false, true] {
        let (owner, (mut element, open, selected, draft)) = create_root(|| {
            let open = create_signal(true);
            let selected = create_signal(vec!["account".to_string()]);
            let draft = create_signal("old".to_string());
            let panel: Element = if use_accordion {
                accordion("panel")
                    .value_signal(selected)
                    .item("account", "Account", move || {
                        input("draft").value_signal(draft).into()
                    })
                    .into()
            } else {
                collapsible("panel")
                    .open_signal(open)
                    .trigger(|| Element::new().text("Account"))
                    .content(move || input("draft").value_signal(draft).into())
                    .into()
            };
            let element = ui! { <div>{panel}<Button>"Outside"</Button></div> };
            (element, open, selected, draft)
        });
        let projection = ElementProjection::new(RootId::new());
        let first = projection.snapshot(&mut element).unwrap();
        let account = button(&first, "Account");
        let outside = button(&first, "Outside");
        open.set(false);
        selected.set(Vec::new());
        projection
            .snapshot_with_focus(&mut element, Some(outside))
            .unwrap();
        assert_eq!(projection.take_focus_request(&element), None);
        open.set(true);
        selected.set(vec!["account".into()]);
        let opened = projection.snapshot(&mut element).unwrap();
        let input = editor(&opened);
        let input_id = input.id;
        let NodeContent::Input { revision, .. } = input.descriptor.content else {
            unreachable!()
        };
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: input_id,
                    payload: EventPayload::Edit {
                        value: "中".into(),
                        revision: revision + 1,
                        selection_utf16: [1, 1],
                        composing_utf16: Some([0, 1]),
                    },
                },
            )
            .unwrap();
        open.set(false);
        selected.set(Vec::new());
        let pending = projection
            .snapshot_with_focus(&mut element, Some(input_id))
            .unwrap();
        assert_eq!(editor(&pending).id, input_id);
        assert_eq!(projection.take_focus_request(&element), None);
        projection
            .dispatch(
                &mut element,
                &PlatformEvent {
                    node: input_id,
                    payload: EventPayload::Edit {
                        value: "中文😀".into(),
                        revision: revision + 2,
                        selection_utf16: [4, 4],
                        composing_utf16: None,
                    },
                },
            )
            .unwrap();
        let closed = projection
            .snapshot_with_focus(&mut element, Some(input_id))
            .unwrap();
        assert!(nodes(&closed).iter().all(|node| node.id != input_id));
        assert_eq!(projection.take_focus_request(&element), Some(account));
        assert_eq!(draft.get(), "中文😀");
        projection
            .snapshot_with_focus(&mut element, Some(input_id))
            .unwrap();
        assert_eq!(projection.take_focus_request(&element), None);
        drop(owner);
    }
}

#[test]
fn nested_disclosures_restore_the_nearest_surviving_header() {
    let (owner, (mut element, outer, inner)) = create_root(|| {
        let outer = create_signal(true);
        let inner = create_signal(true);
        let draft = create_signal(String::new());
        let element = ui! {
            <Collapsible id="outer" open={outer}>
                <CollapsibleTrigger>"Outer"</CollapsibleTrigger>
                <CollapsibleContent>
                    <Collapsible id="inner" open={inner}>
                        <CollapsibleTrigger>"Inner"</CollapsibleTrigger>
                        <CollapsibleContent><Input value={draft}/></CollapsibleContent>
                    </Collapsible>
                </CollapsibleContent>
            </Collapsible>
        };
        (element, outer, inner)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    inner.set(false);
    projection
        .snapshot_with_focus(&mut element, Some(editor(&first).id))
        .unwrap();
    assert_eq!(
        projection.take_focus_request(&element),
        Some(button(&first, "Inner"))
    );
    inner.set(true);
    let reopened = projection.snapshot(&mut element).unwrap();
    inner.set(false);
    outer.set(false);
    projection
        .snapshot_with_focus(&mut element, Some(editor(&reopened).id))
        .unwrap();
    assert_eq!(
        projection.take_focus_request(&element),
        Some(button(&first, "Outer"))
    );
    drop(owner);
}

#[test]
fn disabled_or_ambiguous_restore_targets_are_not_focused() {
    for ambiguous in [false, true] {
        let (owner, (mut element, open)) = create_root(|| {
            let open = create_signal(true);
            let mut element: Element = ui! {
                <div>
                    <Collapsible id="panel" open={open} disabled={!ambiguous}>
                        <CollapsibleTrigger>"Account"</CollapsibleTrigger>
                        <CollapsibleContent><Input/></CollapsibleContent>
                    </Collapsible>
                </div>
            };
            if ambiguous {
                element
                    .children
                    .push(ui! { <Button id="panel:trigger">"Duplicate"</Button> });
            }
            (element, open)
        });
        let projection = ElementProjection::new(RootId::new());
        let first = projection.snapshot(&mut element).unwrap();
        open.set(false);
        projection
            .snapshot_with_focus(&mut element, Some(editor(&first).id))
            .unwrap();
        assert_eq!(projection.take_focus_request(&element), None);
        drop(owner);
    }
}
