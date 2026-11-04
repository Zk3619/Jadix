use std::collections::{BTreeMap, BTreeSet};

use super::{BackendError, HeadlessRoot};
use crate::presentation::{CommitBatch, NodeId, Operation, PresentationNode};

impl HeadlessRoot {
    pub fn reconcile(&mut self, roots: &[PresentationNode]) -> Result<bool, BackendError> {
        self.reconcile_batch(roots).map(|batch| batch.is_some())
    }

    pub fn reconcile_batch(
        &mut self,
        roots: &[PresentationNode],
    ) -> Result<Option<CommitBatch>, BackendError> {
        let batch = self.diff_batch(roots)?;
        if let Some(batch) = &batch {
            self.commit(batch.clone())?;
        }
        Ok(batch)
    }

    pub(crate) fn diff_batch(
        &self,
        roots: &[PresentationNode],
    ) -> Result<Option<CommitBatch>, BackendError> {
        if self.lifetime.is_disposed() {
            return Err(BackendError::Disposed);
        }
        self.check_resources(roots)?;
        let mut desired = BTreeMap::new();
        let mut ordered = Vec::new();
        let mut pending: Vec<_> = roots
            .iter()
            .enumerate()
            .rev()
            .map(|(index, node)| (node, None, index))
            .collect();
        while let Some((node, parent, index)) = pending.pop() {
            if node.id.root != self.id() {
                return Err(BackendError::WrongRoot);
            }
            if desired.insert(node.id, node).is_some() {
                return Err(BackendError::ReusedNode(node.id));
            }
            if !node.children.is_empty() && !node.descriptor.content.accepts_children() {
                return Err(BackendError::InvalidParent);
            }
            ordered.push((node, parent, index));
            pending.extend(
                node.children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, child)| (child, Some(node.id), index)),
            );
        }
        let mut operations = Vec::new();
        let mut parents: BTreeMap<_, _> = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.parent))
            .collect();
        let mut children: BTreeMap<Option<NodeId>, Vec<NodeId>> = self
            .nodes
            .iter()
            .map(|(id, node)| (Some(*id), node.children.clone()))
            .collect();
        children.insert(None, self.roots.clone());
        for (node, parent, index) in ordered {
            if let Some(current) = self.nodes.get(&node.id) {
                if current.descriptor != node.descriptor {
                    operations.push(Operation::Update {
                        node: node.id,
                        descriptor: node.descriptor.clone(),
                    });
                }
                let previous = parents[&node.id];
                let previous_index = children[&previous]
                    .iter()
                    .position(|id| *id == node.id)
                    .unwrap();
                if previous != parent || previous_index != index {
                    children.get_mut(&previous).unwrap().remove(previous_index);
                    children
                        .get_mut(&parent)
                        .ok_or(BackendError::InvalidParent)?
                        .insert(index, node.id);
                    parents.insert(node.id, parent);
                    operations.push(Operation::Move {
                        node: node.id,
                        parent,
                        index,
                    });
                }
            } else {
                children
                    .get_mut(&parent)
                    .ok_or(BackendError::InvalidParent)?
                    .insert(index, node.id);
                children.insert(Some(node.id), Vec::new());
                parents.insert(node.id, parent);
                operations.push(Operation::Create {
                    node: node.id,
                    parent,
                    index,
                    descriptor: node.descriptor.clone(),
                });
            }
        }
        let removed: BTreeSet<_> = self
            .nodes
            .keys()
            .filter(|id| !desired.contains_key(id))
            .copied()
            .collect();
        for id in &removed {
            if parents[id].is_none_or(|parent| !removed.contains(&parent)) {
                operations.push(Operation::Remove { node: *id });
            }
        }
        let batch = CommitBatch {
            root: self.id(),
            previous: self.revision,
            revision: self.revision.checked_add(1).ok_or(BackendError::Revision)?,
            operations,
        };
        Ok((!batch.operations.is_empty()).then_some(batch))
    }
}
