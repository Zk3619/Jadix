use super::*;

fn snapshot(children: &[u64], focus: u64) -> TreeUpdate {
    let mut root = accesskit::Node::new(accesskit::Role::Window);
    root.set_children(
        children
            .iter()
            .copied()
            .map(accesskit::NodeId)
            .collect::<Vec<_>>(),
    );
    let mut nodes = vec![(accesskit::NodeId(1), root)];
    nodes.extend(children.iter().map(|&id| {
        let mut node = accesskit::Node::new(accesskit::Role::TextInput);
        node.set_value(format!("value-{id}"));
        (accesskit::NodeId(id), node)
    }));
    TreeUpdate {
        nodes,
        tree: Some(accesskit::Tree::new(accesskit::NodeId(1))),
        tree_id: accesskit::TreeId::ROOT,
        focus: accesskit::NodeId(focus),
    }
}

#[test]
fn semantic_delta_sends_only_changed_nodes_and_keeps_focus_updates() {
    let mut cache = TreeSnapshot::default();
    assert_eq!(cache.delta(snapshot(&[2, 3], 2), false).nodes.len(), 3);
    let unchanged = cache.delta(snapshot(&[2, 3], 3), false);
    assert!(unchanged.nodes.is_empty());
    assert!(unchanged.tree.is_none());
    assert_eq!(unchanged.focus, accesskit::NodeId(3));
    let mut next = snapshot(&[2, 3], 3);
    next.nodes[2].1.set_value("中文👩‍💻");
    let delta = cache.delta(next, false);
    assert_eq!(delta.nodes.len(), 1);
    assert_eq!(delta.nodes[0].0, accesskit::NodeId(3));
}

#[test]
fn semantic_delta_handles_reorder_removal_reinsertion_and_activation_reset() {
    let mut cache = TreeSnapshot::default();
    cache.delta(snapshot(&[2, 3], 2), false);
    let reordered = cache.delta(snapshot(&[3, 2], 2), false);
    assert_eq!(reordered.nodes.len(), 1);
    assert_eq!(
        reordered.nodes[0].1.children(),
        &[accesskit::NodeId(3), accesskit::NodeId(2)]
    );
    assert_eq!(cache.delta(snapshot(&[3], 3), false).nodes.len(), 1);
    assert!(!cache.nodes.contains_key(&accesskit::NodeId(2)));
    assert_eq!(cache.delta(snapshot(&[3, 2], 2), false).nodes.len(), 2);
    let reset = cache.delta(snapshot(&[3, 2], 2), true);
    assert_eq!(reset.nodes.len(), 3);
    assert!(reset.tree.is_some());
    assert_eq!(
        TreeSnapshot::default()
            .delta(snapshot(&[2], 2), false)
            .nodes
            .len(),
        2
    );
}

#[test]
fn destroyed_surface_actions_cannot_target_a_replacement() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let old = Arc::new(AtomicBool::new(true));
    let mut event = Event {
        window_id: WindowId::from_raw(0),
        window_event: AdapterWindowEvent::InitialTreeRequested,
        active: Some(old.clone()),
    };
    assert!(event.is_current());
    old.store(false, Ordering::Release);
    assert!(!event.is_current());
    event.active = Some(Arc::new(AtomicBool::new(true)));
    assert!(event.is_current());
    assert!(!old.load(Ordering::Acquire));
    event.active = None;
    assert!(event.is_current());
}

#[test]
fn ohos_bounds_scale_once_at_root() {
    use accesskit::{Affine, Node, NodeId, Rect, Role, Tree, TreeId};
    let mut root = Node::new(Role::Window);
    root.set_transform(Affine::translate((3.0, 4.0)));
    let update = TreeUpdate {
        nodes: vec![(NodeId(1), root)],
        tree: Some(Tree::new(NodeId(1))),
        tree_id: TreeId::ROOT,
        focus: NodeId(1),
    };
    let scaled = physical_tree_update(update.clone(), 2.0);
    assert_eq!(
        scaled.nodes[0]
            .1
            .transform()
            .unwrap()
            .transform_rect_bbox(Rect::new(0.0, 0.0, 5.0, 6.0)),
        Rect::new(6.0, 8.0, 16.0, 20.0)
    );
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.0] {
        assert_eq!(
            physical_tree_update(update.clone(), scale).nodes[0]
                .1
                .transform(),
            update.nodes[0].1.transform()
        );
    }
}
