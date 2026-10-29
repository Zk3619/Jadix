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
#[path = "tests/model.rs"]
mod tests;
