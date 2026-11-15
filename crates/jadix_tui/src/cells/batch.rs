//! Transactional ingestion of the shared incremental presentation protocol.
use super::*;
use jadix_core::presentation::{ApplyReceipt, CommitBatch, Operation};

struct Pending<'a> {
    descriptor: &'a NodeDescriptor,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}
impl CellSurface {
    /// Apply Create/Update/Move/Remove/Focus atomically. Payload is incremental;
    /// Geometry-preserving leaf updates validate/stage only changed nodes.
    /// Structural or layout changes stage the whole tree; sibling moves have
    /// an explicit work budget. No incremental Taffy guarantee is implied.
    pub fn apply_batch(&mut self, batch: &CommitBatch) -> io::Result<ApplyReceipt> {
        self.apply_batch_inner(batch, None)
    }
    pub fn apply_batch_auto(
        &mut self,
        batch: &CommitBatch,
        area: Rect,
        environment: u64,
    ) -> io::Result<ApplyReceipt> {
        self.apply_batch_inner(batch, Some((area, environment)))
    }
    fn apply_batch_inner(
        &mut self,
        batch: &CommitBatch,
        auto: Option<(Rect, u64)>,
    ) -> io::Result<ApplyReceipt> {
        self.live()?;
        if self.structural_layout.is_some() {
            return Err(invalid("use commit_auto for this structural publisher"));
        }
        if batch.root != self.lifetime.id()
            || batch.previous != self.revision
            || batch.revision <= batch.previous
            || batch.operations.len() > 65_536
        {
            return Err(invalid("invalid or stale cell batch"));
        }
        if auto.is_none() && self.try_patch(batch)? {
            return Ok(ApplyReceipt::for_batch(batch));
        }
        let mut roots = Vec::new();
        let mut nodes = HashMap::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let parent = node.parent.map(|index| self.nodes[index].id);
            nodes.insert(
                node.id,
                Pending {
                    descriptor: &node.descriptor,
                    parent,
                    children: vec![],
                },
            );
            siblings(&mut nodes, &mut roots, parent)?.push(node.id);
        }
        let mut retired = self.retired.clone();
        let mut focus = None;
        let mut created_bytes = 0usize;
        // ponytail: structural staging still walks the tree; borrowed descriptors
        // avoid duplicating unchanged text before final candidate construction.
        let mut work = nodes.len();
        for operation in &batch.operations {
            let id = match operation {
                Operation::Create { node, .. }
                | Operation::Update { node, .. }
                | Operation::Move { node, .. }
                | Operation::Remove { node } => Some(*node),
                Operation::Focus { node } => *node,
            };
            if id.is_some_and(|id| id.root != batch.root) {
                return Err(invalid("foreign batch node"));
            }
            match operation {
                Operation::Create {
                    node,
                    parent,
                    index,
                    descriptor,
                } => {
                    if nodes.contains_key(node) || retired.contains(node) || nodes.len() >= 65_536 {
                        return Err(invalid("reused or over-budget batch node"));
                    }
                    budget(descriptor, &mut created_bytes)?;
                    insert(&mut nodes, &mut roots, *parent, *index, *node, &mut work)?;
                    nodes.insert(
                        *node,
                        Pending {
                            descriptor,
                            parent: *parent,
                            children: vec![],
                        },
                    );
                }
                Operation::Update { node, descriptor } => {
                    budget(descriptor, &mut created_bytes)?;
                    let current = nodes
                        .get_mut(node)
                        .ok_or_else(|| invalid("unknown batch update"))?;
                    if !descriptor.content.accepts_children() && !current.children.is_empty() {
                        return Err(invalid("batch leaf has children"));
                    }
                    current.descriptor = descriptor;
                }
                Operation::Move {
                    node,
                    parent,
                    index,
                } => {
                    let old = nodes
                        .get(node)
                        .ok_or_else(|| invalid("unknown batch move"))?
                        .parent;
                    let mut at = *parent;
                    let mut depth = 0;
                    while let Some(ancestor) = at {
                        charge(&mut work, 1)?;
                        depth += 1;
                        if ancestor == *node || depth > 128 {
                            return Err(invalid("cyclic batch move"));
                        }
                        at = nodes
                            .get(&ancestor)
                            .ok_or_else(|| invalid("unknown batch parent"))?
                            .parent;
                    }
                    let list = siblings(&mut nodes, &mut roots, old)?;
                    charge(&mut work, list.len().saturating_mul(2))?;
                    let from = list
                        .iter()
                        .position(|id| id == node)
                        .ok_or_else(|| invalid("detached batch node"))?;
                    list.remove(from);
                    insert(&mut nodes, &mut roots, *parent, *index, *node, &mut work)?;
                    nodes.get_mut(node).unwrap().parent = *parent;
                }
                Operation::Remove { node } => {
                    let current = nodes
                        .get(node)
                        .ok_or_else(|| invalid("unknown batch removal"))?;
                    let parent = current.parent;
                    let list = siblings(&mut nodes, &mut roots, parent)?;
                    charge(&mut work, list.len())?;
                    list.retain(|id| id != node);
                    let mut pending = vec![*node];
                    while let Some(id) = pending.pop() {
                        charge(&mut work, 1)?;
                        let removed = nodes
                            .remove(&id)
                            .ok_or_else(|| invalid("broken batch subtree"))?;
                        pending.extend(removed.children);
                        retired.insert(id);
                    }
                }
                Operation::Focus { node } => focus = Some(*node),
            }
        }
        let mut built = HashMap::with_capacity(nodes.len());
        let mut pending: Vec<_> = roots.iter().rev().map(|id| (*id, false, 0usize)).collect();
        while let Some((id, visited, depth)) = pending.pop() {
            if depth > 128 {
                return Err(invalid("batch tree depth"));
            }
            let node = nodes.get(&id).ok_or_else(|| invalid("broken batch tree"))?;
            if !visited {
                pending.push((id, true, depth));
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|child| (*child, false, depth + 1)),
                );
            } else {
                let children = node
                    .children
                    .iter()
                    .map(|id| built.remove(id).unwrap())
                    .collect();
                built.insert(
                    id,
                    PresentationNode {
                        id,
                        descriptor: node.descriptor.clone(),
                        children,
                    },
                );
            }
        }
        let mut trees: Vec<_> = roots.iter().map(|id| built.remove(id).unwrap()).collect();
        drop(nodes);
        self.validate_candidate(&trees)?;
        if let Some((area, environment)) = auto {
            self.layout_auto(&mut trees, area, environment)?;
        }
        // Validate focus against candidate modal/visibility state before publishing.
        if let Some(Some(id)) = focus {
            let mut nodes = HashMap::new();
            let mut pending: Vec<_> = trees.iter().map(|node| (node, None)).collect();
            while let Some((node, parent)) = pending.pop() {
                nodes.insert(
                    node.id,
                    Pending {
                        descriptor: &node.descriptor,
                        parent,
                        children: vec![],
                    },
                );
                pending.extend(node.children.iter().map(|child| (child, Some(node.id))));
            }
            let metadata = nodes
                .iter()
                .map(|(id, n)| (*id, (n.parent, &n.descriptor.content)))
                .collect();
            let priority = trees_order(&trees);
            let overlays = jadix_layout::presentation_overlay::overlay_order_with_priority(
                &metadata, &priority,
            )
            .map_err(|_| invalid("invalid batch overlays"))?;
            let visible = |id: NodeId| {
                let mut at = Some(id);
                while let Some(id) = at {
                    let Some(node) = nodes.get(&id) else {
                        return false;
                    };
                    if !shown(node.descriptor) {
                        return false;
                    }
                    at = node.parent;
                }
                true
            };
            let modal = overlays.iter().rev().find(|id| visible(**id)).copied();
            let mut at = Some(id);
            let mut in_modal = modal.is_none();
            while let Some(id) = at {
                in_modal |= Some(id) == modal;
                at = nodes.get(&id).and_then(|n| n.parent);
            }
            if !visible(id)
                || !in_modal
                || !nodes
                    .get(&id)
                    .is_some_and(|n| enabled(&n.descriptor.content))
            {
                return Err(invalid("ineligible batch focus"));
            }
        }
        self.present(self.revision, trees)?;
        if let Some((area, _)) = auto {
            self.automatic = true;
            self.layout(area);
        }
        self.revision = batch.revision;
        self.retired = retired;
        if let Some(node) = focus {
            self.focus = node;
            self.capture = None;
            self.dirty = true;
            if self.area.is_some() {
                self.reveal_focus();
            }
        }
        Ok(ApplyReceipt::for_batch(batch))
    }
}

impl CellSurface {
    // Reuse published structure/geometry when neither can change. Every other
    // transaction takes the general validator; never silently skip layout.
    fn try_patch(&mut self, batch: &CommitBatch) -> io::Result<bool> {
        if self.automatic {
            return Ok(false);
        }
        let mut updates = BTreeMap::new();
        let mut focus = None;
        let mut payload_bytes = 0;
        for operation in &batch.operations {
            match operation {
                Operation::Update { node, descriptor } => {
                    let index = self
                        .node_indices
                        .get(node)
                        .copied()
                        .ok_or_else(|| invalid("unknown batch update"))?;
                    let old = &self.nodes[index].descriptor;
                    if descriptor.frame != old.frame
                        || descriptor.style != old.style
                        || std::mem::discriminant(&descriptor.content)
                            != std::mem::discriminant(&old.content)
                        || descriptor.content.accepts_children()
                    {
                        return Ok(false);
                    }
                    budget(descriptor, &mut payload_bytes)?;
                    if content_revision(&descriptor.content) == Some(u64::MAX) {
                        return Err(invalid("cell event revision exhausted"));
                    }
                    updates.insert(index, descriptor);
                }
                Operation::Focus { node } => focus = Some(*node),
                _ => return Ok(false),
            }
        }
        // Eligibility requires the published projection. A pre-paint Focus
        // uses the general path, whose candidate validation needs no Buffer.
        if focus.is_some() && self.geometry.len() != self.nodes.len() {
            return Ok(false);
        }
        let mut bytes = self.candidate_bytes;
        let mut retained = self.editor_bytes;
        let mut fields = Vec::new();
        let mut geometry_changed = focus.is_some();
        for (&index, &descriptor) in &updates {
            let old_node = &self.nodes[index];
            bytes = bytes - descriptor_bytes(&old_node.descriptor) + descriptor_bytes(descriptor);
            let stable_editor = match (&old_node.descriptor.content, &descriptor.content) {
                (
                    NodeContent::Input {
                        enabled: a,
                        password: p,
                        ..
                    },
                    NodeContent::Input {
                        enabled: b,
                        password: q,
                        ..
                    },
                ) => a == b && p == q,
                (
                    NodeContent::Textarea { enabled: a, .. },
                    NodeContent::Textarea { enabled: b, .. },
                ) => a == b,
                _ => false,
            } && self.focus.is_some()
                && old_node.descriptor.accessible_role == descriptor.accessible_role
                && old_node.descriptor.accessible_selected == descriptor.accessible_selected;
            geometry_changed |= !matches!(
                descriptor.content,
                NodeContent::Text(_) | NodeContent::Image { .. }
            ) && !stable_editor;
            if let Some((value, revision)) = field_value(&descriptor.content) {
                let old = &self.fields[&old_node.id];
                if revision < old.revision || (revision == old.revision && value != old.controlled)
                {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                if value.len() > TerminalEditor::MAX_BYTES {
                    return Err(invalid("cell editor budget"));
                }
                if revision > old.revision
                    || old.editor.max_length() != descriptor.content.input_max_length()
                {
                    let mut next = old.editor.clone();
                    if revision > old.revision {
                        next.apply_external(next.document().revision(), value)?;
                    }
                    next.set_max_length(descriptor.content.input_max_length())?;
                    retained = retained - old.editor.retained_bytes() + next.retained_bytes();
                    fields.push((old_node.id, next, value.to_owned(), revision));
                }
            }
        }
        if bytes > 16 * 1024 * 1024 || retained > Self::EDITOR_BYTES {
            return Err(invalid("cell patch text budget"));
        }
        if let Some(Some(id)) = focus {
            let index = self
                .node_indices
                .get(&id)
                .copied()
                .ok_or_else(|| invalid("unknown batch focus"))?;
            let descriptor = updates
                .get(&index)
                .copied()
                .unwrap_or(&self.nodes[index].descriptor);
            if !self.geometry[index].live || !self.in_scope(index) || !enabled(&descriptor.content)
            {
                return Err(invalid("ineligible batch focus"));
            }
        }
        // All fallible work is complete, including editor replacements.
        for (id, editor, controlled, revision) in fields {
            let old = self.fields.get_mut(&id).unwrap();
            if editor.document() != old.editor.document() {
                old.preferred_column = None;
                old.visual_caret = None;
            }
            old.editor = editor;
            old.controlled = controlled;
            old.revision = revision;
            if !geometry_changed {
                let index = self.node_indices[&id];
                if let Some(geometry) = self.geometry.get(index) {
                    if self
                        .search
                        .active
                        .and_then(|at| self.search.matches.get(at))
                        .is_none_or(|found| found.node != id)
                    {
                        reveal_caret(
                            old,
                            geometry.frame,
                            matches!(
                                self.nodes[index].descriptor.content,
                                NodeContent::Input { password: true, .. }
                            ),
                        );
                    }
                }
            }
        }
        for (index, descriptor) in updates {
            self.search.touch(self.nodes[index].id);
            self.nodes[index].descriptor = descriptor.clone();
        }
        self.editor_bytes = retained;
        self.candidate_bytes = bytes;
        self.revision = batch.revision;
        self.automatic = false;
        self.dirty = true;
        if geometry_changed {
            if let Some(area) = self.area {
                self.layout(area);
            }
        }
        if let Some(node) = focus {
            self.focus = node;
            self.capture = None;
            if node.is_some() {
                self.reveal_focus();
            }
        }
        Ok(true)
    }
}
fn siblings<'a>(
    nodes: &'a mut HashMap<NodeId, Pending<'_>>,
    roots: &'a mut Vec<NodeId>,
    parent: Option<NodeId>,
) -> io::Result<&'a mut Vec<NodeId>> {
    match parent {
        None => Ok(roots),
        Some(id) => {
            let parent = nodes
                .get_mut(&id)
                .ok_or_else(|| invalid("unknown batch parent"))?;
            if !parent.descriptor.content.accepts_children() {
                return Err(invalid("batch parent is a leaf"));
            }
            Ok(&mut parent.children)
        }
    }
}
fn insert(
    nodes: &mut HashMap<NodeId, Pending<'_>>,
    roots: &mut Vec<NodeId>,
    parent: Option<NodeId>,
    index: usize,
    id: NodeId,
    work: &mut usize,
) -> io::Result<()> {
    let list = siblings(nodes, roots, parent)?;
    if index > list.len() {
        return Err(invalid("batch child index"));
    }
    charge(work, list.len() - index + 1)?;
    list.insert(index, id);
    Ok(())
}
fn charge(work: &mut usize, amount: usize) -> io::Result<()> {
    *work = work.saturating_add(amount);
    if *work > 4 * 1024 * 1024 {
        return Err(invalid("cell batch structural work budget"));
    }
    Ok(())
}
fn budget(node: &NodeDescriptor, bytes: &mut usize) -> io::Result<()> {
    *bytes = bytes
        .saturating_add(content_bytes(&node.content))
        .saturating_add(node.style.font_family.len())
        .saturating_add(
            node.style.inline_styles.len()
                * std::mem::size_of::<jadix_core::presentation::style::InlineStyle>(),
        )
        .saturating_add(node.accessible_name.as_ref().map_or(0, |name| name.len()));
    if *bytes > 16 * 1024 * 1024
        || !node.style.is_valid()
        || !inline_valid(node)
        || !node.frame.is_valid()
        || !node.content.is_valid()
    {
        return Err(invalid("invalid or over-budget batch payload"));
    }
    Ok(())
}
fn trees_order(trees: &[PresentationNode]) -> BTreeMap<NodeId, usize> {
    let mut result = BTreeMap::new();
    let mut pending: Vec<_> = trees.iter().rev().collect();
    while let Some(node) = pending.pop() {
        result.insert(node.id, result.len());
        pending.extend(node.children.iter().rev());
    }
    result
}
