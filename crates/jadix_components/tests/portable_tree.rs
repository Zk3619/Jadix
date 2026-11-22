#![cfg(all(not(feature = "gpu"), feature = "macros"))]

use jadix::{authoring::native::ElementProjection, portable::tree_model::*, presentation::*};
use jadix_components::prelude::*;
use jadix_native::HeadlessRoot;
use std::{cell::RefCell, rc::Rc};

fn nodes(root: &PresentationNode) -> Vec<&PresentationNode> {
    let mut result = vec![root];
    let mut index = 0;
    while index < result.len() {
        result.extend(&result[index].children);
        index += 1;
    }
    result
}

fn button(root: &PresentationNode, label: &str) -> NodeId {
    nodes(root).into_iter().find(|node| matches!(&node.descriptor.content,
        NodeContent::Button { label: value, .. } if value == label)).unwrap().id
}

#[test]
fn lazy_tree_cancels_collapsed_loads_reuses_keys_and_reveals_keyboard_target() {
    let requests = Rc::new(RefCell::new(Vec::new()));
    let requested = requests.clone();
    let (_owner, (mut element, state)) = create_root(|| {
        let state = TreeViewState::new(vec![TreeNode {
            key: "folder".into(), label: "Folder".into(), enabled: true, children: Children::Unloaded,
        }, TreeNode::leaf("folder__toggle".into(), "Sibling")], TreeLimits::default()).unwrap();
        let element = tree_view("files", state.clone()).h(128.).on_load(move |request| {
            requested.borrow_mut().push(request);
        }).build();
        (element, state)
    });
    let root = HeadlessRoot::new();
    let projection = ElementProjection::with_lifetime(root.lifetime());
    let first = projection.snapshot(&mut element).unwrap();
    let folder = button(&first, "Folder");
    let navigate = |element: &mut Element, node, key| projection.dispatch(element, &PlatformEvent {
        node, payload: EventPayload::Navigate { key, shift: false, control: false },
    }).unwrap();
    navigate(&mut element, folder, NavigationKey::Right);
    let pending = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&pending).iter().find(|node| node.id == folder).unwrap().descriptor.accessibility.busy);
    let old = requests.borrow()[0].clone();
    projection.dispatch(&mut element, &PlatformEvent {
        node: button(&pending, "▾"), payload: EventPayload::Activate,
    }).unwrap();
    assert_eq!(state.complete_load(&old, vec![TreeNode::leaf("stale".into(), "Stale")]), Err(TreeError::StaleLoad));
    navigate(&mut element, folder, NavigationKey::Right);
    let current = requests.borrow()[1].clone();
    assert_ne!(old.generation(), current.generation());
    state.complete_load(&current, (0..5000).map(|i| TreeNode::leaf(format!("child-{i}"), format!("Child {i}"))).collect()).unwrap();
    let expanded = projection.snapshot(&mut element).unwrap();
    assert_eq!(button(&expanded, "Folder"), folder);
    assert!(nodes(&expanded).len() < 80);
    navigate(&mut element, folder, NavigationKey::End);
    let last = projection.snapshot(&mut element).unwrap();
    assert!(nodes(&last).len() < 80);
    assert_eq!(projection.take_focus_request(&element), Some(button(&last, "Sibling")));
    assert!(!nodes(&last).iter().any(|node| matches!(&node.descriptor.content, NodeContent::Button {label, ..} if label == "Stale")));
}

#[test]
fn synchronous_lazy_loader_can_publish_without_borrowing_tree_twice() {
    let (_owner, (mut element, state)) = create_root(|| {
        let state = TreeViewState::new(vec![TreeNode { key: "a".into(), label: "A".into(), enabled: true, children: Children::Unloaded }], TreeLimits::default()).unwrap();
        let loaded = state.clone();
        let element = tree_view("sync", state.clone()).on_load(move |request| {
            loaded.complete_load(&request, vec![TreeNode::leaf("b".into(), "B")]).unwrap();
        }).build();
        (element, state)
    });
    let projection = ElementProjection::new(RootId::new());
    let first = projection.snapshot(&mut element).unwrap();
    projection.dispatch(&mut element, &PlatformEvent { node: button(&first, "▸"), payload: EventPayload::Activate }).unwrap();
    let second = projection.snapshot(&mut element).unwrap();
    assert_eq!(state.read(|tree| tree.visible().len()), 2);
    button(&second, "B");
}

