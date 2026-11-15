#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{
    authoring::native::{ElementProjection, ProjectionError},
    presentation::*,
};
use jadix_components::prelude::*;
use jadix_native::HeadlessRoot;
use std::{cell::Cell, rc::Rc};

fn descendants<'node>(node: &'node PresentationNode, output: &mut Vec<&'node PresentationNode>) {
    output.push(node);
    for child in &node.children {
        descendants(child, output);
    }
}

fn button(node: &PresentationNode, label: &str) -> NodeId {
    let mut nodes = vec![];
    descendants(node, &mut nodes);
    nodes.into_iter().find(|node| matches!(&node.descriptor.content, NodeContent::Button { label: current, .. } if current == label)).unwrap().id
}

fn input_node(node: &PresentationNode) -> NodeId {
    let mut nodes = vec![];
    descendants(node, &mut nodes);
    nodes
        .into_iter()
        .find(|node| matches!(&node.descriptor.content, NodeContent::Input { .. }))
        .unwrap()
        .id
}

#[test]
fn rust_and_rsx_tabs_select_lazily_skip_disabled_items_and_return_keyboard_focus() {
    let (owner, (mut element, selected, draft)) = create_root(|| {
        let selected = create_signal("a".to_string());
        let draft = create_signal("中文😀".to_string());
        let element = ui! {
            <Tabs id="settings" value={selected}>
                <TabsList>
                    <TabsTrigger value="a">"Account"</TabsTrigger>
                    <TabsTrigger value="disabled" disabled={true}>"Unavailable"</TabsTrigger>
                    <TabsTrigger value="b">"Password"</TabsTrigger>
                </TabsList>
                <TabsContent value="a"><Input id="account" value={draft} /></TabsContent>
                <TabsContent value="b">"Second panel"</TabsContent>
            </Tabs>
        };
        (element, selected, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let first = projection.snapshot(&mut element).unwrap();
    let account = button(&first, "Account");
    let password = button(&first, "Password");
    let mut nodes = vec![];
    descendants(&first, &mut nodes);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.descriptor.accessible_role == Some(AccessibleRole::TabList))
            .count(),
        1
    );
    assert!(nodes
        .iter()
        .any(|node| node.descriptor.accessible_role == Some(AccessibleRole::TabPanel)));
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == account)
            .unwrap()
            .descriptor
            .accessible_role,
        Some(AccessibleRole::TabItem)
    );
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == account)
            .unwrap()
            .descriptor
            .accessible_selected,
        Some(true)
    );
    let old_input = input_node(&first);
    assert_eq!(projection.navigation_nodes(&element).len(), 2);
    model.reconcile(std::slice::from_ref(&first)).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: account,
                payload: EventPayload::Navigate {
                    key: NavigationKey::Right,
                    shift: false,
                    control: false,
                },
            },
        )
        .unwrap();
    assert_eq!(selected.get(), "b");
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(projection.take_focus_request(&element), Some(password));
    assert_eq!(button(&second, "Account"), account);
    let mut nodes = vec![];
    descendants(&second, &mut nodes);
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == account)
            .unwrap()
            .descriptor
            .accessible_selected,
        Some(false)
    );
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == password)
            .unwrap()
            .descriptor
            .accessible_selected,
        Some(true)
    );
    model.reconcile(&[second]).unwrap();
    assert_eq!(
        projection.dispatch(
            &mut element,
            &PlatformEvent {
                node: old_input,
                payload: EventPayload::Edit {
                    value: "late".into(),
                    revision: 1,
                    selection_utf16: [4, 4],
                    composing_utf16: None
                }
            }
        ),
        Err(ProjectionError::StaleNode)
    );
    assert_eq!(draft.get(), "中文😀");
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: password,
                payload: EventPayload::Navigate {
                    key: NavigationKey::Home,
                    shift: false,
                    control: false,
                },
            },
        )
        .unwrap();
    let third = projection.snapshot(&mut element).unwrap();
    assert_ne!(input_node(&third), old_input);
    model.reconcile(&[third]).unwrap();
    assert_eq!(selected.get(), "a");
    assert_eq!(projection.take_focus_request(&element), Some(account));
    assert!(projection.take_focus_request(&element).is_none());
    projection.dispose();
    assert!(projection.navigation_nodes(&element).is_empty());
    drop(owner);
}

#[test]
fn portable_tabs_list_projects_accessibility_label() {
    let (_owner, mut element) = create_root(|| {
        tabs("labelled").child(|| {
            Element::new().child(TabsList(TabsListProps {
                accessibility_label: Some("Settings sections".into()),
                ..Default::default()
            }))
        })
    });
    let projection = ElementProjection::new(RootId::new());
    let snapshot = projection.snapshot(&mut element).unwrap();
    let mut nodes = vec![];
    descendants(&snapshot, &mut nodes);
    let list = nodes
        .into_iter()
        .find(|node| node.descriptor.accessible_role == Some(AccessibleRole::TabList))
        .expect("tab list");
    assert_eq!(
        list.descriptor.accessible_name.as_deref(),
        Some("Settings sections")
    );
}

#[test]
fn panels_build_once_on_first_selection_and_sibling_tabs_contexts_stay_isolated() {
    let builds = Rc::new(Cell::new(0));
    let observed = builds.clone();
    let (owner, (mut element, first, second)) = create_root(|| {
        let first = create_signal("a".to_string());
        let second = create_signal("x".to_string());
        let first_tabs = tabs("first").value_signal(first).child(|| {
            Element::new()
                .child(TabsList(TabsListProps {
                    children: Some(
                        Element::new()
                            .child(tabs_trigger("a").text("A"))
                            .child(tabs_trigger("b").text("B")),
                    ),
                    ..Default::default()
                }))
                .child(tabs_content("a", || Element::new().text("first a")))
                .child(tabs_content("b", move || {
                    observed.set(observed.get() + 1);
                    Element::new().text("first b")
                }))
        });
        let second_tabs = tabs("second")
            .value_signal(second)
            .orientation("vertical")
            .child(|| {
                Element::new()
                    .child(TabsList(TabsListProps {
                        children: Some(
                            Element::new()
                                .child(tabs_trigger("x").text("X"))
                                .child(tabs_trigger("y").text("Y")),
                        ),
                        ..Default::default()
                    }))
                    .child(tabs_content("x", || Element::new().text("second x")))
                    .child(tabs_content("y", || Element::new().text("second y")))
            });
        (
            Element::new().child(first_tabs).child(second_tabs),
            first,
            second,
        )
    });
    let projection = ElementProjection::new(RootId::new());
    let snapshot = projection.snapshot(&mut element).unwrap();
    assert_eq!(builds.get(), 0);
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: button(&snapshot, "B"),
                payload: EventPayload::Activate,
            },
        )
        .unwrap();
    assert_eq!(first.get(), "b");
    assert_eq!(second.get(), "x");
    let snapshot = projection.snapshot(&mut element).unwrap();
    assert_eq!(builds.get(), 1);
    first.set("a".into());
    projection.snapshot(&mut element).unwrap();
    first.set("b".into());
    projection.snapshot(&mut element).unwrap();
    assert_eq!(builds.get(), 1);
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: button(&snapshot, "X"),
                payload: EventPayload::Navigate {
                    key: NavigationKey::Down,
                    shift: false,
                    control: false,
                },
            },
        )
        .unwrap();
    assert_eq!(second.get(), "y");
    assert_eq!(first.get(), "b");
    drop(owner);
}

#[test]
fn conditional_panels_keep_the_mounted_editor_until_composition_finishes() {
    let (owner, (mut element, selected, draft)) = create_root(|| {
        let selected = create_signal("a".to_string());
        let draft = create_signal("old".to_string());
        let element = ui! {
            <Tabs id="composition" value={selected}>
                <TabsList><TabsTrigger value="a">"A"</TabsTrigger><TabsTrigger value="b">"B"</TabsTrigger></TabsList>
                <TabsContent value="a"><Input value={draft} /></TabsContent>
                <TabsContent value="b">"B panel"</TabsContent>
            </Tabs>
        };
        (element, selected, draft)
    });
    let mut model = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(model.lifetime());
    let initial = projection.snapshot(&mut element).unwrap();
    let editor = input_node(&initial);
    let mut nodes = Vec::new();
    descendants(&initial, &mut nodes);
    let revision = nodes
        .iter()
        .find_map(|node| match &node.descriptor.content {
            NodeContent::Input { revision, .. } if node.id == editor => Some(*revision),
            _ => None,
        })
        .unwrap();
    model.reconcile(&[initial]).unwrap();
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: editor,
                payload: EventPayload::Edit {
                    value: "中".into(),
                    revision: revision + 1,
                    selection_utf16: [1, 1],
                    composing_utf16: Some([0, 1]),
                },
            },
        )
        .unwrap();
    selected.set("b".into());
    let pending = projection.snapshot(&mut element).unwrap();
    assert_eq!(input_node(&pending), editor);
    model.reconcile(&[pending]).unwrap();
    assert_eq!(draft.get(), "old");
    projection
        .dispatch(
            &mut element,
            &PlatformEvent {
                node: editor,
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
    let mut visible = Vec::new();
    descendants(&committed, &mut visible);
    assert!(visible.iter().all(|node| node.id != editor));
    model.reconcile(&[committed]).unwrap();
    assert_eq!(draft.get(), "中文😀");
    selected.set("a".into());
    let restored = projection.snapshot(&mut element).unwrap();
    assert_ne!(input_node(&restored), editor);
    model.reconcile(&[restored]).unwrap();
    drop(owner);
}

#[test]
fn duplicate_values_and_unknown_orientation_fail_before_native_publication() {
    let (owner, mut duplicate) = create_root(|| {
        tabs("duplicate").child(|| {
            Element::new()
                .child(tabs_trigger("a"))
                .child(tabs_trigger("a"))
        })
    });
    let projection = ElementProjection::new(RootId::new());
    assert_eq!(
        projection.snapshot(&mut duplicate),
        Err(ProjectionError::Unsupported("native component options"))
    );
    drop(owner);
    let (owner, mut orientation) = create_root(|| {
        tabs("orientation")
            .orientation("diagonal")
            .child(|| Element::new().child(tabs_trigger("a")))
    });
    assert_eq!(
        projection.snapshot(&mut orientation),
        Err(ProjectionError::Unsupported("native component options"))
    );
    drop(owner);
}
