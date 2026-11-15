//! Operation-driven Taffy synchronization. Candidates stage only touched records.
//! Warm leaf commits reuse the existing snapshot; full candidate callbacks,
//! topology changes and unproven geometry still visit the full tree. Complete
//! measurement equivalence can retain accepted geometry and update only leaves.
use super::*;
use jadix_core::presentation::{retired::RetiredIds, CommitBatch, Operation};
use std::borrow::Cow;
#[derive(Clone)]
struct Node {
    descriptor: NodeDescriptor,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    // Position in its accepted parent children (or root list). Rebuilt only
    // after a successful structural publication, never by rejected candidates.
    slot: usize,
}
// A transactional overlay borrows unchanged records and root order. Each edited
// record is copied once; live state is never mutated before validation/layout.
struct Candidate<'a> {
    before: &'a BTreeMap<NodeId, Node>,
    changes: BTreeMap<NodeId, Option<Node>>,
    roots: Cow<'a, [NodeId]>,
    len: usize,
}
impl<'a> Candidate<'a> {
    fn new(nodes: &'a BTreeMap<NodeId, Node>, roots: &'a [NodeId]) -> Self {
        Self {
            before: nodes,
            changes: BTreeMap::new(),
            roots: Cow::Borrowed(roots),
            len: nodes.len(),
        }
    }
    fn get(&self, id: &NodeId) -> Option<&Node> {
        self.changes
            .get(id)
            .map_or_else(|| self.before.get(id), Option::as_ref)
    }
    fn get_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.changes
            .entry(*id)
            .or_insert_with(|| self.before.get(id).cloned())
            .as_mut()
    }
    fn insert(&mut self, id: NodeId, node: Node) {
        self.len += usize::from(self.get(&id).is_none());
        self.changes.insert(id, Some(node));
    }
    fn remove(&mut self, id: &NodeId) -> Option<Node> {
        let node = self.get(id)?.clone();
        self.changes.insert(*id, None);
        self.len -= 1;
        Some(node)
    }
}
pub struct StructuralLayout {
    layout: PresentationLayout,
    nodes: BTreeMap<NodeId, Node>,
    roots: Vec<NodeId>,
    retired: RetiredIds,
    revision: u64,
    snapshot: Vec<PresentationNode>,
    overlays: bool,
}
impl StructuralLayout {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            layout: PresentationLayout::new(lifetime),
            nodes: Default::default(),
            roots: vec![],
            retired: Default::default(),
            revision: 0,
            snapshot: vec![],
            overlays: false,
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn roots(&self) -> &[PresentationNode] {
        if self.layout.lifetime.is_disposed() {
            &[]
        } else {
            &self.snapshot
        }
    }
    pub fn release_layout_cache(&mut self) {
        self.layout.release_layout_cache();
    }
    /// Relayout does not consume a structural revision. Warm plain trees reuse
    /// accepted descriptors/topology; compute, rounding and frames still cost N.
    /// Environment changes, overlays and released engines use full layout.
    pub fn relayout(
        &mut self,
        viewport: [f32; 2],
        environment: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
    ) -> Result<LayoutStats, LayoutError> {
        if self.layout.lifetime.is_disposed() {
            return Err(LayoutError::Disposed);
        }
        if viewport.into_iter().any(|v| !v.is_finite() || v < 0.) {
            return Err(LayoutError::InvalidViewport);
        }
        if self.overlays || self.layout.environment != environment || self.layout.engine.is_none() {
            // layout() stages all fallible geometry before writing frames. The
            // accepted snapshot can be borrowed directly, including on failure.
            return self
                .layout
                .layout(&mut self.snapshot, viewport, environment, measure);
        }
        self.layout.epoch = self
            .layout
            .epoch
            .checked_add(1)
            .ok_or(LayoutError::RevisionExhausted)?;
        let mut stats = LayoutStats {
            nodes: self.nodes.len(),
            ..Default::default()
        };
        let mut engine = self.layout.engine.take().expect("warm relayout engine");
        let result = (|| {
            engine.resize(viewport, &mut stats)?;
            self.layout
                .compute(viewport, environment, measure, &mut engine, &mut stats)?;
            self.plain_frames(&engine)
        })();
        match result {
            Ok(frames) => {
                self.publish_plain_frames(frames, &BTreeMap::new());
                self.layout.engine = Some(engine);
                self.layout.viewport = Some(viewport);
                Ok(stats)
            }
            Err(error) => {
                self.layout.nodes.clear();
                self.layout.release_layout_cache();
                Err(error)
            }
        }
    }
    // Same topology without overlays gives engine and snapshot authored preorder.
    // Retain fallible collection before publishing any accepted frame/content.
    fn plain_frames(&self, engine: &Engine) -> Result<Vec<(NodeId, Rect)>, LayoutError> {
        let mut frames = Vec::with_capacity(self.nodes.len());
        for id in &self.roots {
            collect_frames(
                &engine.tree,
                engine.id(Key::Node(*id)),
                [0., 0.],
                &engine.mapping,
                &mut |id, frame| frames.push((id, frame)),
            )?;
        }
        if frames.len() != self.nodes.len() {
            return Err(LayoutError::Engine);
        }
        Ok(frames)
    }
    fn publish_plain_frames(
        &mut self,
        frames: Vec<(NodeId, Rect)>,
        updates: &BTreeMap<NodeId, &NodeDescriptor>,
    ) {
        fn publish(
            node: &mut PresentationNode,
            updates: &BTreeMap<NodeId, &NodeDescriptor>,
            frames: &mut std::vec::IntoIter<(NodeId, Rect)>,
        ) {
            if let Some(descriptor) = updates.get(&node.id) {
                node.descriptor = (*descriptor).clone();
            }
            let (id, frame) = frames.next().expect("complete accepted geometry");
            debug_assert_eq!(id, node.id, "stable engine/snapshot preorder");
            node.descriptor.frame = frame;
            for child in &mut node.children {
                publish(child, updates, frames);
            }
        }
        let mut frames = frames.into_iter();
        for node in &mut self.snapshot {
            publish(node, updates, &mut frames);
        }
        debug_assert!(frames.next().is_none());
    }
    pub fn commit(
        &mut self,
        batch: &CommitBatch,
        viewport: [f32; 2],
        environment: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
    ) -> Result<LayoutStats, LayoutError> {
        // A warm, plain leaf batch cannot change accepted topology. It can
        // validate addressed descriptors and reuse the published snapshot. The
        // checked API still supplies a complete authored candidate to its host.
        if !self.overlays
            && self.layout.environment == environment
            && self.layout.viewport == Some(viewport)
            && self.layout.engine.is_some()
            && batch.operations.iter().all(|operation| {
                matches!(operation, Operation::Update { node, descriptor }
                    if self.nodes.get(node).is_some_and(|old| old.children.is_empty())
                    && !matches!(descriptor.content, NodeContent::Dialog { .. } | NodeContent::Popover { .. }))
            })
        {
            self.commit_leaf_updates(batch, viewport, environment, measure)
        } else {
            self.commit_checked(batch, viewport, environment, measure, |_| Ok(()))
        }
    }
    fn commit_leaf_updates(
        &mut self,
        batch: &CommitBatch,
        viewport: [f32; 2],
        environment: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
    ) -> Result<LayoutStats, LayoutError> {
        if self.layout.lifetime.is_disposed() {
            return Err(LayoutError::Disposed);
        }
        if batch.root != self.layout.lifetime.id()
            || batch.previous != self.revision
            || batch.revision
                != self
                    .revision
                    .checked_add(1)
                    .ok_or(LayoutError::RevisionExhausted)?
            || batch.operations.len() > 65536
        {
            return Err(LayoutError::InvalidTree);
        }
        let updates: BTreeMap<_, _> = batch
            .operations
            .iter()
            .map(|operation| {
                let Operation::Update { node, descriptor } = operation else {
                    unreachable!()
                };
                (*node, descriptor)
            })
            .collect();
        // Match the complete candidate's final-descriptor semantics, including
        // duplicate updates. Failed admission never modifies caches or live data.
        for (node, descriptor) in &updates {
            validate_descriptor(descriptor, node.root)?;
        }
        self.layout.epoch = self
            .layout
            .epoch
            .checked_add(1)
            .ok_or(LayoutError::RevisionExhausted)?;
        let mut stats = LayoutStats {
            nodes: self.nodes.len(),
            ..Default::default()
        };
        let mut measure = measure;
        let proof = match self.measure_unchanged(&updates, &mut measure, &mut stats) {
            Ok(proof) => proof,
            Err(error) => {
                self.layout.nodes.clear();
                self.layout.release_layout_cache();
                return Err(error);
            }
        };
        if let Some(caches) = proof {
            // All fallible work is done. The accepted engine layout remains
            // valid: update its context without dirtying ancestors or rounding.
            let engine = self.layout.engine.as_mut().expect("warm leaf engine");
            for (&id, &descriptor) in &updates {
                let taffy = engine.id(Key::Node(id));
                let context = engine
                    .tree
                    .get_node_context_mut(taffy)
                    .expect("proven text leaf");
                if context.descriptor != *descriptor {
                    context.descriptor = descriptor.clone();
                    stats.engine_updated += 1;
                }
            }
            for (id, cache) in caches {
                self.layout.nodes.insert(id, cache);
            }
            for (&id, &descriptor) in &updates {
                let mut path = Vec::new();
                let mut at = Some(id);
                while let Some(id) = at {
                    let node = &self.nodes[&id];
                    path.push(node.slot);
                    at = node.parent;
                }
                let mut nodes = &mut self.snapshot;
                let mut slots = path.into_iter().rev().peekable();
                while let Some(slot) = slots.next() {
                    let node = &mut nodes[slot];
                    if slots.peek().is_none() {
                        debug_assert_eq!(node.id, id);
                        let frame = node.descriptor.frame;
                        node.descriptor = descriptor.clone();
                        node.descriptor.frame = frame;
                        break;
                    }
                    nodes = &mut node.children;
                }
                self.nodes.get_mut(&id).expect("accepted leaf").descriptor = descriptor.clone();
            }
            self.revision = batch.revision;
            stats.sync_nodes = updates.len();
            stats.engine_reused = updates.len();
            stats.incremental_sync = true;
            stats.geometry_reused = true;
            return Ok(stats);
        }
        let delta = StructureDelta {
            node_count: self.nodes.len(),
            nodes: updates
                .iter()
                .map(|(id, descriptor)| DeltaNode {
                    id: *id,
                    descriptor,
                    children: &[],
                })
                .collect(),
            removed: vec![],
            roots: None,
        };
        let mut engine = self.layout.engine.take().expect("warm leaf engine");
        let result = (|| {
            engine.sync_delta(&delta, viewport, &mut stats)?;
            self.layout
                .compute(viewport, environment, measure, &mut engine, &mut stats)?;
            self.plain_frames(&engine)
        })();
        let frames = match result {
            Ok(frames) => frames,
            Err(error) => {
                self.layout.nodes.clear();
                self.layout.release_layout_cache();
                return Err(error);
            }
        };
        self.publish_plain_frames(frames, &updates);
        for (id, descriptor) in updates {
            self.nodes.get_mut(&id).expect("accepted leaf").descriptor = descriptor.clone();
        }
        self.layout.engine = Some(engine);
        self.revision = batch.revision;
        Ok(stats)
    }
    fn measure_unchanged(
        &self,
        updates: &BTreeMap<NodeId, &NodeDescriptor>,
        measure: &mut impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
        stats: &mut LayoutStats,
    ) -> Result<Option<BTreeMap<NodeId, CachedNode>>, LayoutError> {
        let mut caches = BTreeMap::new();
        let engine = self.layout.engine.as_ref().expect("warm leaf engine");
        // Gate every record before invoking any host measurement. Style and
        // metadata stay exact; label length alone never proves geometry equal.
        for (&id, &next) in updates {
            let old = &self.nodes[&id].descriptor;
            let compatible = match (&old.content, &next.content) {
                (NodeContent::Text(_), NodeContent::Text(_)) => true,
                (
                    NodeContent::Button { enabled: a, .. },
                    NodeContent::Button { enabled: b, .. },
                ) => a == b,
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
                (
                    NodeContent::Checkbox { enabled: a, .. },
                    NodeContent::Checkbox { enabled: b, .. },
                )
                | (
                    NodeContent::Switch { enabled: a, .. },
                    NodeContent::Switch { enabled: b, .. },
                )
                | (
                    NodeContent::Select { enabled: a, .. },
                    NodeContent::Select { enabled: b, .. },
                ) => a == b,
                _ => false,
            };
            if !compatible
                || old.style != next.style
                || old.frame != next.frame
                || old.accessible_name != next.accessible_name
                || old.accessible_role != next.accessible_role
                || old.accessible_selected != next.accessible_selected
                || old.accessible_expanded != next.accessible_expanded
                || engine
                    .tree
                    .get_node_context(engine.id(Key::Node(id)))
                    .is_none_or(|context| {
                        context.descriptor.content != old.content
                            || context.descriptor.style != old.style
                    })
            {
                return Ok(None);
            }
            if old.content == next.content {
                continue;
            }
            let Some(cache) = self.layout.nodes.get(&id).filter(|cache| {
                cache.complete
                    && !cache.measures.is_empty()
                    && cache.content == old.content
                    && cache.style == old.style
            }) else {
                return Ok(None);
            };
            let mut cache = cache.clone();
            cache.content = next.content.clone();
            cache.content_revision = self.layout.epoch;
            caches.insert(id, cache);
        }
        for (&id, cache) in &mut caches {
            for (request, previous) in &mut cache.measures {
                request.key.content_revision = cache.content_revision;
                stats.measurements += 1;
                let measured = measure(*request, updates[&id]);
                if self.layout.lifetime.is_disposed() {
                    return Err(LayoutError::Disposed);
                }
                // A retained query may belong to an earlier layout. Failure
                // only disqualifies the proof; normal layout validates the
                // queries that are active now before accepting the candidate.
                let Ok(next) = measured else {
                    return Ok(None);
                };
                if !next.matches(request.key) {
                    return Ok(None);
                }
                if next.width != previous.width
                    || next.height != previous.height
                    || next.baseline != previous.baseline
                {
                    return Ok(None);
                }
                *previous = next;
            }
        }
        Ok(Some(caches))
    }
    pub fn commit_checked(
        &mut self,
        batch: &CommitBatch,
        viewport: [f32; 2],
        environment: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
        validate: impl FnOnce(&[PresentationNode]) -> Result<(), LayoutError>,
    ) -> Result<LayoutStats, LayoutError> {
        if self.layout.lifetime.is_disposed() {
            return Err(LayoutError::Disposed);
        }
        if batch.root != self.layout.lifetime.id()
            || batch.previous != self.revision
            || batch.revision
                != self
                    .revision
                    .checked_add(1)
                    .ok_or(LayoutError::RevisionExhausted)?
            || batch.operations.len() > 65536
        {
            return Err(LayoutError::InvalidTree);
        }
        let mut nodes = Candidate::new(&self.nodes, &self.roots);
        let mut dirty = BTreeSet::new();
        let mut work = 0usize;
        let mut removed = BTreeSet::new();
        let mut roots_dirty = false;
        for operation in &batch.operations {
            match operation {
                Operation::Create {
                    node,
                    parent,
                    index,
                    descriptor,
                } => {
                    if node.root != batch.root
                        || nodes.get(node).is_some()
                        || self.retired.contains(node)
                        || removed.contains(node)
                        || nodes.len >= 65536
                    {
                        return Err(LayoutError::InvalidTree);
                    }
                    validate_descriptor(descriptor, node.root)?;
                    insert(&mut nodes, *node, *parent, *index, &mut work)?;
                    nodes.insert(
                        *node,
                        Node {
                            descriptor: descriptor.clone(),
                            parent: *parent,
                            children: vec![],
                            slot: 0,
                        },
                    );
                    dirty.insert(*node);
                    if let Some(parent) = parent {
                        dirty.insert(*parent);
                    } else {
                        roots_dirty = true;
                    }
                }
                Operation::Update { node, descriptor } => {
                    validate_descriptor(descriptor, node.root)?;
                    nodes
                        .get_mut(node)
                        .ok_or(LayoutError::InvalidTree)?
                        .descriptor = descriptor.clone();
                    dirty.insert(*node);
                }
                Operation::Move {
                    node,
                    parent,
                    index,
                } => {
                    let previous = nodes.get(node).ok_or(LayoutError::InvalidTree)?.parent;
                    detach(&mut nodes, *node, previous, &mut work)?;
                    insert(&mut nodes, *node, *parent, *index, &mut work)?;
                    nodes.get_mut(node).unwrap().parent = *parent;
                    if let Some(p) = previous {
                        dirty.insert(p);
                    } else {
                        roots_dirty = true;
                    }
                    if let Some(p) = parent {
                        dirty.insert(*p);
                    } else {
                        roots_dirty = true;
                    }
                }
                Operation::Remove { node } => {
                    let previous = nodes.get(node).ok_or(LayoutError::InvalidTree)?.parent;
                    detach(&mut nodes, *node, previous, &mut work)?;
                    if let Some(p) = previous {
                        dirty.insert(p);
                    } else {
                        roots_dirty = true;
                    }
                    let mut pending = vec![*node];
                    while let Some(id) = pending.pop() {
                        charge(&mut work, 1)?;
                        let n = nodes.remove(&id).ok_or(LayoutError::InvalidTree)?;
                        pending.extend(n.children);
                        removed.insert(id);
                    }
                }
                Operation::Focus { .. } => return Err(LayoutError::InvalidTree),
            }
        }
        // Check final topology before touching Taffy (including disconnected cycles).
        let mut seen = BTreeSet::new();
        let mut overlays = false;
        let mut pending: Vec<_> = nodes.roots.iter().map(|id| (*id, None, 0)).collect();
        while let Some((id, parent, depth)) = pending.pop() {
            let n = nodes.get(&id).ok_or(LayoutError::InvalidTree)?;
            if depth > 256 || !seen.insert(id) || n.parent != parent {
                return Err(LayoutError::InvalidTree);
            }
            if matches!(
                n.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            ) {
                overlays = true;
            }
            pending.extend(n.children.iter().map(|child| (*child, Some(id), depth + 1)));
        }
        if seen.len() != nodes.len || self.retired.len().saturating_add(removed.len()) > 65536 {
            return Err(LayoutError::InvalidTree);
        }
        fn tree(id: NodeId, nodes: &Candidate<'_>) -> PresentationNode {
            let n = nodes.get(&id).expect("validated reachable node");
            PresentationNode {
                id,
                descriptor: n.descriptor.clone(),
                children: n.children.iter().map(|id| tree(*id, nodes)).collect(),
            }
        }
        let mut snapshot: Vec<_> = nodes.roots.iter().map(|id| tree(*id, &nodes)).collect();
        validate(&snapshot)?;
        let delta = StructureDelta {
            node_count: nodes.len,
            nodes: dirty
                .into_iter()
                .filter_map(|id| {
                    nodes.get(&id).map(|n| DeltaNode {
                        id,
                        descriptor: &n.descriptor,
                        children: &n.children,
                    })
                })
                .collect(),
            removed: removed
                .iter()
                .filter(|id| self.nodes.contains_key(id))
                .copied()
                .collect(),
            roots: roots_dirty.then_some(nodes.roots.as_ref()),
        };
        let mut engine = self.layout.engine.take().unwrap_or_default();
        let result = self.layout.layout_inner(
            &mut snapshot,
            viewport,
            environment,
            measure,
            &mut engine,
            if overlays || self.overlays {
                None
            } else {
                Some(&delta)
            },
        );
        match result {
            Ok(stats) => {
                self.layout.engine = Some(engine);
                self.layout.viewport = Some(viewport);
                self.overlays = overlays;
                let Candidate { changes, roots, .. } = nodes;
                let roots = match roots {
                    Cow::Owned(roots) => Some(roots),
                    Cow::Borrowed(_) => None,
                };
                for (id, node) in changes {
                    if let Some(node) = node {
                        self.nodes.insert(id, node);
                    } else {
                        self.nodes.remove(&id);
                    }
                }
                if let Some(roots) = roots {
                    self.roots = roots;
                }
                self.snapshot = snapshot;
                fn slots(nodes: &mut BTreeMap<NodeId, Node>, roots: &[PresentationNode]) {
                    for (slot, node) in roots.iter().enumerate() {
                        nodes.get_mut(&node.id).expect("accepted topology").slot = slot;
                        slots(nodes, &node.children);
                    }
                }
                slots(&mut self.nodes, &self.snapshot);
                self.revision = batch.revision;
                for id in removed {
                    self.retired.insert(id);
                }
                Ok(stats)
            }
            Err(error) => {
                // Failed measurement can cache records from an unpublished
                // candidate. Do not retain those through later sparse retries.
                self.layout.nodes.clear();
                self.layout.release_layout_cache();
                Err(error)
            }
        }
    }
}
fn detach(
    nodes: &mut Candidate<'_>,
    node: NodeId,
    parent: Option<NodeId>,
    work: &mut usize,
) -> Result<(), LayoutError> {
    let list = if let Some(parent) = parent {
        &mut nodes
            .get_mut(&parent)
            .ok_or(LayoutError::InvalidTree)?
            .children
    } else {
        nodes.roots.to_mut()
    };
    let index = list
        .iter()
        .position(|id| *id == node)
        .ok_or(LayoutError::InvalidTree)?;
    charge(work, list.len() - index)?;
    list.remove(index);
    Ok(())
}
fn insert(
    nodes: &mut Candidate<'_>,
    node: NodeId,
    parent: Option<NodeId>,
    index: usize,
    work: &mut usize,
) -> Result<(), LayoutError> {
    let list = if let Some(parent) = parent {
        let n = nodes.get_mut(&parent).ok_or(LayoutError::InvalidTree)?;
        if parent == node || !n.descriptor.content.accepts_children() {
            return Err(LayoutError::InvalidTree);
        }
        &mut n.children
    } else {
        nodes.roots.to_mut()
    };
    if index > list.len() {
        return Err(LayoutError::InvalidTree);
    }
    charge(work, list.len() - index + 1)?;
    list.insert(index, node);
    Ok(())
}

fn charge(work: &mut usize, amount: usize) -> Result<(), LayoutError> {
    *work = work.saturating_add(amount);
    if *work > 4 * 1024 * 1024 {
        Err(LayoutError::InvalidTree)
    } else {
        Ok(())
    }
}
