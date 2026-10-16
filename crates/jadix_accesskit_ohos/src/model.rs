// Adapted from AccessKit PR #828; MIT OR Apache-2.0.
use accesskit_consumer::{Node, NodeId};
use std::collections::HashMap;
const ROOT_QUERY_ID: i64 = -1;
const ROOT_ELEMENT_ID: i32 = 0;

#[derive(Debug, Default)]
pub(crate) struct NodeIdMap {
    platform_to_accesskit: HashMap<i32, NodeId>,
    accesskit_to_platform: HashMap<NodeId, i32>,
    next_platform_id: i32,
}

impl NodeIdMap {
    pub(crate) fn get_or_create(&mut self, node: &Node) -> Option<i32> {
        if node.is_root() {
            self.platform_to_accesskit
                .insert(ROOT_ELEMENT_ID, node.id());
            self.accesskit_to_platform
                .insert(node.id(), ROOT_ELEMENT_ID);
            self.next_platform_id = self.next_platform_id.max(1);
            return Some(ROOT_ELEMENT_ID);
        }
        if let Some(id) = self.accesskit_to_platform.get(&node.id()) {
            return Some(*id);
        }
        let id = self.next_platform_id.max(1);
        self.next_platform_id = id.checked_add(1)?;
        self.platform_to_accesskit.insert(id, node.id());
        self.accesskit_to_platform.insert(node.id(), id);
        Some(id)
    }

    pub(crate) fn get_accesskit(&self, platform_id: i64, root: NodeId) -> Option<NodeId> {
        if platform_id == ROOT_QUERY_ID || platform_id == i64::from(ROOT_ELEMENT_ID) {
            return Some(root);
        }
        let platform_id = i32::try_from(platform_id).ok()?;
        self.platform_to_accesskit.get(&platform_id).copied()
    }
}

impl NodeIdMap {
    pub(crate) fn retain(&mut self, tree: &accesskit_consumer::Tree) {
        self.accesskit_to_platform
            .retain(|id, _| tree.state().node_by_id(*id).is_some());
        self.platform_to_accesskit
            .retain(|_, id| tree.state().node_by_id(*id).is_some());
    }
}

#[cfg(test)]
mod tests {
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
}
