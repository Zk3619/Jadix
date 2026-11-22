//! Retain Taffy identity and caches independently of caller-owned snapshots.
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Key {
    Root,
    Node(NodeId),
    Panel(NodeId),
}
pub(super) struct Leaf {
    pub id: NodeId,
    pub descriptor: NodeDescriptor,
}
struct Plan<'a> {
    style: Style,
    children: Vec<Key>,
    leaf: Option<(NodeId, &'a NodeDescriptor)>,
}
pub(super) struct Engine {
    pub tree: TaffyTree<Leaf>,
    pub ids: HashMap<Key, taffy::NodeId>,
    pub mapping: HashMap<taffy::NodeId, NodeId>,
    children: HashMap<Key, Vec<Key>>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            tree: TaffyTree::new(),
            ids: HashMap::new(),
            mapping: HashMap::new(),
            children: HashMap::new(),
        }
    }
}
impl Engine {
    pub fn id(&self, key: Key) -> taffy::NodeId {
        self.ids[&key]
    }
    // An accepted plain tree keeps its descriptors and children. Resizing only
    // changes the synthetic root style, without rebuilding per-node plans.
    pub fn resize(
        &mut self,
        viewport: [f32; 2],
        stats: &mut LayoutStats,
    ) -> Result<(), LayoutError> {
        let root = self.id(Key::Root);
        let mut style = self
            .tree
            .style(root)
            .map_err(|_| LayoutError::Engine)?
            .clone();
        let size = Size {
            width: Dimension::length(viewport[0]),
            height: Dimension::length(viewport[1]),
        };
        if style.size != size {
            style.size = size;
            self.tree
                .set_style(root, style)
                .map_err(|_| LayoutError::Engine)?;
            stats.engine_updated += 1;
        }
        stats.sync_nodes += 1;
        stats.engine_reused += 1;
        stats.incremental_sync = true;
        Ok(())
    }
    pub fn sync(
        &mut self,
        roots: &[PresentationNode],
        viewport: [f32; 2],
        stats: &mut LayoutStats,
    ) -> Result<(), LayoutError> {
        let mut plans = HashMap::new();
        for root in roots {
            plan(root, Key::Node(root.id), None, &mut plans);
        }
        let mut children: Vec<_> = roots.iter().map(|n| Key::Node(n.id)).collect();
        let mut pending: Vec<_> = roots.iter().rev().collect();
        while let Some(node) = pending.pop() {
            if let NodeContent::Dialog { open, .. } | NodeContent::Popover { open, .. } =
                node.descriptor.content
            {
                if !open {
                    continue;
                }
                let mut descriptor = node.descriptor.clone();
                descriptor.content = NodeContent::Container;
                let style = &mut descriptor.style;
                let popover = matches!(node.descriptor.content, NodeContent::Popover { .. });
                let available_width = (viewport[0] - if popover { 16. } else { 32. }).max(0.);
                let available_height = (viewport[1] - if popover { 16. } else { 32. }).max(0.);
                if popover
                    && (style.padding[1] + style.padding[3] > available_width
                        || style.padding[0] + style.padding[2] > available_height)
                {
                    return Err(LayoutError::InvalidStyle);
                }
                let width = style
                    .width
                    .unwrap_or(if popover { 320. } else { 480. })
                    .min(available_width);
                style.width = Some(width);
                style.min_width = style.min_width.min(width);
                style.min_height = style.min_height.min(available_height);
                style.height = style.height.map(|v| v.min(available_height));
                style.max_height = Some(
                    style
                        .max_height
                        .unwrap_or(available_height)
                        .min(available_height),
                );
                let key = Key::Panel(node.id);
                plan(node, key, Some(&descriptor), &mut plans);
                let style = &mut plans.get_mut(&key).unwrap().style;
                style.position = Position::Absolute;
                style.inset.left = LengthPercentageAuto::length(0.);
                style.inset.top = LengthPercentageAuto::length(0.);
                children.push(key);
            }
            pending.extend(node.children.iter().rev());
        }
        plans.insert(
            Key::Root,
            Plan {
                style: Style {
                    size: Size {
                        width: Dimension::length(viewport[0]),
                        height: Dimension::length(viewport[1]),
                    },
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                children,
                leaf: None,
            },
        );
        stats.sync_nodes += plans.len();
        let removed = self
            .ids
            .keys()
            .filter(|k| !plans.contains_key(k))
            .copied()
            .collect();
        self.apply_plans(plans, removed, stats)
    }
    pub fn sync_delta(
        &mut self,
        delta: &StructureDelta<'_>,
        viewport: [f32; 2],
        stats: &mut LayoutStats,
    ) -> Result<(), LayoutError> {
        let mut plans = HashMap::new();
        for node in &delta.nodes {
            plans.insert(
                Key::Node(node.id),
                Plan {
                    style: engine_style(node.descriptor),
                    children: node.children.iter().map(|id| Key::Node(*id)).collect(),
                    leaf: needs_measurement(node.descriptor, node.children.is_empty())
                        .then_some((node.id, node.descriptor)),
                },
            );
        }
        if let Some(roots) = delta.roots {
            plans.insert(
                Key::Root,
                Plan {
                    style: Style {
                        size: Size {
                            width: Dimension::length(viewport[0]),
                            height: Dimension::length(viewport[1]),
                        },
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    children: roots.iter().map(|id| Key::Node(*id)).collect(),
                    leaf: None,
                },
            );
        }
        stats.sync_nodes += plans.len();
        stats.incremental_sync = true;
        self.apply_plans(
            plans,
            delta.removed.iter().map(|id| Key::Node(*id)).collect(),
            stats,
        )
    }
    fn apply_plans(
        &mut self,
        plans: HashMap<Key, Plan<'_>>,
        removed: Vec<Key>,
        stats: &mut LayoutStats,
    ) -> Result<(), LayoutError> {
        for (&key, plan) in &plans {
            if self.ids.contains_key(&key) {
                stats.engine_reused += 1;
                continue;
            }
            let id = self
                .tree
                .new_leaf(plan.style.clone())
                .map_err(|_| LayoutError::Engine)?;
            self.ids.insert(key, id);
            if let Key::Node(node) | Key::Panel(node) = key {
                self.mapping.insert(id, node);
            }
            stats.engine_created += 1;
        }
        // Detach every changed/deleted parent before reparenting, so simultaneous
        // ancestor reversal never exposes a transient cycle to Taffy mark_dirty.
        let changed: Vec<_> = plans
            .iter()
            .filter(|(key, p)| self.children.get(key).is_some_and(|old| *old != p.children))
            .map(|(key, _)| *key)
            .chain(removed.iter().copied())
            .collect();
        for key in changed {
            if !self.ids.contains_key(&key) {
                continue;
            }
            self.tree
                .set_children(self.id(key), &[])
                .map_err(|_| LayoutError::Engine)?;
            self.children.remove(&key);
        }

        for key in removed {
            let id = self.ids.remove(&key).unwrap();
            self.mapping.remove(&id);
            self.children.remove(&key);
            self.tree.remove(id).map_err(|_| LayoutError::Engine)?;
            stats.engine_removed += 1;
        }
        for (key, plan) in plans {
            let id = self.id(key);
            let mut updated = false;
            if self.tree.style(id).map_err(|_| LayoutError::Engine)? != &plan.style {
                self.tree
                    .set_style(id, plan.style)
                    .map_err(|_| LayoutError::Engine)?;
                updated = true;
            }
            match (self.tree.get_node_context(id), plan.leaf) {
                (Some(old), Some((_, next))) if metrics_equal(&old.descriptor, next) => {
                    // Paint/accessibility/frame changes are not measurement keys.
                    if old.descriptor != *next {
                        self.tree.get_node_context_mut(id).unwrap().descriptor = next.clone();
                    }
                }
                (None, None) => {}
                (_, leaf) => {
                    self.tree
                        .set_node_context(
                            id,
                            leaf.map(|(id, d)| Leaf {
                                id,
                                descriptor: d.clone(),
                            }),
                        )
                        .map_err(|_| LayoutError::Engine)?;
                    updated = true;
                }
            }
            if self.children.get(&key) != Some(&plan.children) {
                let ids: Vec<_> = plan.children.iter().map(|k| self.id(*k)).collect();
                self.tree
                    .set_children(id, &ids)
                    .map_err(|_| LayoutError::Engine)?;
                self.children.insert(key, plan.children);
                stats.children_updated += 1;
            }
            stats.engine_updated += usize::from(updated);
        }
        Ok(())
    }
}
fn metrics_equal(old: &NodeDescriptor, new: &NodeDescriptor) -> bool {
    if old.content != new.content {
        return false;
    }
    let mut style = old.style.clone();
    style.foreground = new.style.foreground;
    style.background = new.style.background;
    style == new.style
}
fn plan<'a>(
    node: &'a PresentationNode,
    key: Key,
    panel: Option<&NodeDescriptor>,
    plans: &mut HashMap<Key, Plan<'a>>,
) {
    let descriptor = panel.unwrap_or(&node.descriptor);
    let hidden = panel.is_none()
        && matches!(
            descriptor.content,
            NodeContent::Dialog { .. } | NodeContent::Popover { .. }
        );
    let children = if hidden {
        vec![]
    } else {
        node.children.iter().map(|n| Key::Node(n.id)).collect()
    };
    let style = if hidden {
        Style {
            display: Display::None,
            ..Default::default()
        }
    } else {
        engine_style(descriptor)
    };
    let leaf = if !hidden && needs_measurement(descriptor, node.children.is_empty()) {
        Some((node.id, &node.descriptor))
    } else {
        None
    };
    plans.insert(
        key,
        Plan {
            style,
            children,
            leaf,
        },
    );
    if !hidden {
        for child in &node.children {
            plan(child, Key::Node(child.id), None, plans);
        }
    }
}

// Buttons can host authored content, but a plain label still needs the host's
// intrinsic control metrics. Child admission is not a measurement policy.
fn needs_measurement(descriptor: &NodeDescriptor, empty: bool) -> bool {
    !descriptor.content.accepts_children()
        || (empty && matches!(descriptor.content, NodeContent::Button { .. } | NodeContent::Hosted { .. }))
}
