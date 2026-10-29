use super::*;
use accesskit::{Node as Data, NodeId as LocalId, Role, Tree as DataTree, TreeId, TreeUpdate};
use accesskit_consumer::Tree;

fn tree(root_id: u64, child_id: u64) -> Tree {
    let mut root = Data::new(Role::Window);
    root.set_children(vec![LocalId(child_id)]);
    Tree::new(
        TreeUpdate {
            nodes: vec![
                (LocalId(root_id), root),
                (LocalId(child_id), Data::new(Role::Button)),
            ],
            tree: Some(DataTree::new(LocalId(root_id))),
            tree_id: TreeId::ROOT,
            focus: LocalId(child_id),
        },
        true,
    )
}

#[test]
fn ids_are_stable_pruned_and_do_not_alias_on_overflow() {
    let tree = tree(42, u64::MAX);
    let root = tree.state().root();
    let child = root.children().next().unwrap();
    let mut ids = NodeIdMap::default();
    assert_eq!(ids.get_or_create(&root), Some(0));
    assert_eq!(ids.get_or_create(&child), Some(1));
    assert_eq!(ids.get_or_create(&child), Some(1));
    assert_eq!(ids.get_accesskit(-1, root.id()), Some(root.id()));
    assert_eq!(ids.get_accesskit(0, root.id()), Some(root.id()));
    assert_eq!(ids.get_accesskit(1, root.id()), Some(child.id()));
    for bad in [-2, i64::MAX, i64::from(i32::MAX) + 1] {
        assert_eq!(ids.get_accesskit(bad, root.id()), None);
    }
    let next = super::tests::tree(42, 10);
    ids.retain(&next);
    assert_eq!(ids.get_accesskit(1, next.state().root_id()), None);
    assert_eq!(
        ids.get_or_create(&next.state().root().children().next().unwrap()),
        Some(2)
    );
    let fresh = super::tests::tree(42, 11);
    ids.next_platform_id = i32::MAX;
    assert_eq!(
        ids.get_or_create(&fresh.state().root().children().next().unwrap()),
        None
    );
    assert_eq!(
        ids.get_accesskit(2, next.state().root_id()),
        Some(next.state().root().children().next().unwrap().id())
    );
}
