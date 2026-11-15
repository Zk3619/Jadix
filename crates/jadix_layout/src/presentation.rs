//! Shared CPU presentation layout; callers provide host-specific leaf measurement.
use std::collections::{BTreeMap, BTreeSet};

use jadix_core::presentation::{
    style::{Axis, NodeStyle},
    MeasureKey, MeasureResult, NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect, RootId,
    RootLifetime,
};
use taffy::prelude::*;
mod retained;
mod structural;
pub use structural::StructuralLayout;
// Transient synchronization borrows the validated candidate. Engine contexts
// own only the descriptors they retain after this synchronous call.
struct DeltaNode<'a> {
    id: NodeId,
    descriptor: &'a NodeDescriptor,
    children: &'a [NodeId],
}
struct StructureDelta<'a> {
    node_count: usize,
    nodes: Vec<DeltaNode<'a>>,
    removed: Vec<NodeId>,
    roots: Option<&'a [NodeId]>,
}
use retained::{Engine, Key};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureMode {
    Definite,
    MinContent,
    MaxContent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasureRequest {
    pub key: MeasureKey,
    pub width_mode: MeasureMode,
    pub height_mode: MeasureMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    Disposed,
    InvalidTree,
    InvalidStyle,
    InvalidViewport,
    InvalidMeasurement(NodeId),
    MeasurementUnavailable(NodeId),
    RevisionExhausted,
    Engine,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    /// Taffy synchronization visits, including synthetic roots; excludes O(N) validation/frames.
    pub sync_nodes: usize,
    pub incremental_sync: bool,
    /// Every published frame and topology is unchanged from the accepted commit.
    /// True only when complete prior measurement requests prove reuse safe.
    pub geometry_reused: bool,
    pub measurements: usize,
    pub cache_hits: usize,
    pub nodes: usize,
    /// Retained Taffy nodes, including synthetic root and open panel roots.
    pub engine_created: usize,
    pub engine_reused: usize,
    pub engine_removed: usize,
    pub engine_updated: usize,
    pub children_updated: usize,
}

#[derive(Clone)]
struct CachedNode {
    content: NodeContent,
    style: NodeStyle,
    content_revision: u64,
    style_revision: u64,
    measures: Vec<(MeasureRequest, MeasureResult)>,
    // False after any request eviction; retained queries then cannot prove that
    // all inputs used by accepted Taffy geometry have been remeasured.
    complete: bool,
}

pub struct PresentationLayout {
    lifetime: RootLifetime,
    nodes: BTreeMap<NodeId, CachedNode>,
    environment: u64,
    epoch: u64,
    engine: Option<Engine>,
    viewport: Option<[f32; 2]>,
}

impl PresentationLayout {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            nodes: BTreeMap::new(),
            environment: 0,
            epoch: 0,
            engine: None,
            viewport: None,
        }
    }

    pub fn cached_nodes(&self) -> usize {
        if self.lifetime.is_disposed() {
            0
        } else {
            self.nodes.len()
        }
    }

    /// Drop only measurement/layout caches; presentation IDs and edit state live
    /// in the host/model and are not recreated when preferences change.
    pub fn invalidate_measurements(&mut self) {
        self.nodes.clear();
        self.engine = None;
    }
    /// Release retained Taffy storage while keeping bounded host measurements.
    /// The next layout reconstructs structure and recomputes geometry.
    pub fn release_layout_cache(&mut self) {
        self.engine = None;
    }

    pub fn layout(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment_revision: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
    ) -> Result<LayoutStats, LayoutError> {
        let mut engine = self.engine.take().unwrap_or_default();
        let result = self.layout_inner(
            roots,
            viewport,
            environment_revision,
            measure,
            &mut engine,
            None,
        );
        // A failed measure may leave Taffy with partial computed state. Rebuild
        // on retry; only successful geometry is ever written into the input.
        if result.is_ok() {
            self.engine = Some(engine);
            self.viewport = Some(viewport);
        }
        result
    }
    fn layout_inner(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment_revision: u64,
        measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
        engine: &mut Engine,
        delta: Option<&StructureDelta<'_>>,
    ) -> Result<LayoutStats, LayoutError> {
        if self.lifetime.is_disposed() {
            return Err(LayoutError::Disposed);
        }
        if viewport
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0)
        {
            return Err(LayoutError::InvalidViewport);
        }
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or(LayoutError::RevisionExhausted)?;
        if self.environment != environment_revision {
            self.nodes.clear();
            *engine = Engine::default();
            self.environment = environment_revision;
        }
        let mut seen = BTreeSet::new();
        let mut metadata = BTreeMap::new();
        let (node_count, overlay_order) = if let Some(delta) = delta {
            // StructuralLayout has already validated candidate topology. With
            // no overlays, unchanged descriptors remain valid and only changed
            // records need content/style admission again.
            for node in &delta.nodes {
                if node.id.root != self.lifetime.id()
                    || (!node.descriptor.content.accepts_children() && !node.children.is_empty())
                    || matches!(
                        node.descriptor.content,
                        NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                    )
                {
                    return Err(LayoutError::InvalidTree);
                }
                validate_descriptor(node.descriptor, node.id.root)?;
            }
            for id in &delta.removed {
                self.nodes.remove(id);
            }
            (delta.node_count, Vec::new())
        } else {
            let mut pending: Vec<_> = roots.iter().map(|node| (node, None, 0)).collect();
            while let Some((node, parent, depth)) = pending.pop() {
                if depth > 256
                    || seen.len() >= 65536
                    || node.id.root != self.lifetime.id()
                    || !seen.insert(node.id)
                    || (!node.descriptor.content.accepts_children() && !node.children.is_empty())
                {
                    return Err(LayoutError::InvalidTree);
                }
                validate_descriptor(&node.descriptor, node.id.root)?;
                metadata.insert(node.id, (parent, &node.descriptor.content));
                pending.extend(
                    node.children
                        .iter()
                        .map(|child| (child, Some(node.id), depth + 1)),
                );
            }
            let overlay_order = crate::presentation_overlay::overlay_order(&metadata)
                .map_err(|_| LayoutError::InvalidTree)?;
            self.nodes.retain(|node, _| seen.contains(node));
            (seen.len(), overlay_order)
        };
        let mut stats = LayoutStats {
            nodes: node_count,
            ..Default::default()
        };
        if let Some(delta) =
            delta.filter(|_| self.viewport == Some(viewport) && engine.ids.contains_key(&Key::Root))
        {
            engine.sync_delta(delta, viewport, &mut stats)?;
        } else {
            engine.sync(roots, viewport, &mut stats)?;
        }
        self.compute(viewport, environment_revision, measure, engine, &mut stats)?;
        let tree = &engine.tree;
        let mut frames: BTreeMap<_, _> = seen.into_iter().map(|id| (id, Rect::default())).collect();
        for node in roots.iter() {
            collect_frames(
                tree,
                engine.ids[&Key::Node(node.id)],
                [0.0, 0.0],
                &engine.mapping,
                &mut |id, frame| {
                    frames.insert(id, frame);
                },
            )?;
        }
        for node in overlay_order {
            let Some(child) = engine.ids.get(&Key::Panel(node)) else {
                continue;
            };
            let size = tree.layout(*child).map_err(|_| LayoutError::Engine)?.size;
            let offset = match metadata[&node].1 {
                NodeContent::Popover {
                    anchor,
                    placement,
                    gap,
                    ..
                } => crate::presentation_overlay::position(
                    frames[anchor],
                    [f64::from(size.width), f64::from(size.height)],
                    viewport,
                    *placement,
                    *gap,
                ),
                _ => [
                    f64::from((viewport[0] - size.width).max(0.0) / 2.0),
                    f64::from((viewport[1] - size.height).max(0.0) / 2.0),
                ],
            };
            collect_frames(tree, *child, offset, &engine.mapping, &mut |id, frame| {
                frames.insert(id, frame);
            })?;
        }
        if frames.len() != node_count {
            return Err(LayoutError::Engine);
        }
        for node in roots {
            apply_frames(node, &frames);
        }
        Ok(stats)
    }

    // Computes retained engine geometry without constructing a presentation
    // snapshot. Callers publish frames only after this entire operation succeeds.
    fn compute(
        &mut self,
        viewport: [f32; 2],
        environment_revision: u64,
        mut measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
        engine: &mut Engine,
        stats: &mut LayoutStats,
    ) -> Result<(), LayoutError> {
        let root = engine.id(Key::Root);
        let tree = &mut engine.tree;
        let mut failure = None;
        let caches = &mut self.nodes;
        let epoch = self.epoch;
        tree.compute_layout_with_measure(
            root,
            Size {
                width: AvailableSpace::Definite(viewport[0]),
                height: AvailableSpace::Definite(viewport[1]),
            },
            |inputs, _, context, style| {
                let mut baseline = None;
                let mut output = taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, space| {
                        let Some(node) = context else {
                            return Size::ZERO;
                        };
                        if failure.is_some() {
                            return Size::ZERO;
                        }
                        let cached = caches.entry(node.id).or_insert_with(|| CachedNode {
                            content: node.descriptor.content.clone(),
                            style: node.descriptor.style.clone(),
                            content_revision: epoch,
                            style_revision: epoch,
                            measures: Vec::new(),
                            complete: true,
                        });
                        if cached.content != node.descriptor.content {
                            cached.content = node.descriptor.content.clone();
                            cached.content_revision = epoch;
                            cached.measures.clear();
                            cached.complete = true;
                        }
                        if cached.style != node.descriptor.style {
                            let mut metrics = cached.style.clone();
                            metrics.foreground = node.descriptor.style.foreground;
                            metrics.background = node.descriptor.style.background;
                            if metrics != node.descriptor.style {
                                cached.style_revision = epoch;
                                cached.measures.clear();
                                cached.complete = true;
                            }
                            cached.style = node.descriptor.style.clone();
                        }
                        let (max_width, width_mode) =
                            constraint(known.width, space.width, viewport[0]);
                        let (max_height, height_mode) =
                            constraint(known.height, space.height, viewport[1]);
                        let request = MeasureRequest {
                            key: MeasureKey {
                                node: node.id,
                                content_revision: cached.content_revision,
                                style_revision: cached.style_revision,
                                environment_revision,
                                max_width: max_width.into(),
                                max_height: max_height.into(),
                            },
                            width_mode,
                            height_mode,
                        };
                        let result = if let Some((_, result)) =
                            cached.measures.iter().find(|(key, _)| *key == request)
                        {
                            stats.cache_hits += 1;
                            Ok(*result)
                        } else {
                            stats.measurements += 1;
                            if let Some([width, height]) =
                                node.descriptor.content.image_dimensions()
                            {
                                Ok(MeasureResult {
                                    key: request.key,
                                    width: f64::from(width).min(request.key.max_width),
                                    height: f64::from(height).min(request.key.max_height),
                                    baseline: None,
                                })
                            } else {
                                measure(request, &node.descriptor)
                            }
                        };
                        match result {
                            Ok(result) if result.matches(request.key) => {
                                baseline = result.baseline.map(|value| value as f32);
                                if !cached.measures.iter().any(|(key, _)| *key == request) {
                                    if cached.measures.len() >= 16 {
                                        cached.measures.remove(0);
                                        cached.complete = false;
                                    }
                                    cached.measures.push((request, result));
                                }
                                Size {
                                    width: result.width as f32,
                                    height: result.height as f32,
                                }
                            }
                            Ok(_) => {
                                failure = Some(LayoutError::InvalidMeasurement(node.id));
                                Size::ZERO
                            }
                            Err(error) => {
                                failure = Some(error);
                                Size::ZERO
                            }
                        }
                    },
                );
                output.baselines.first = baseline;
                output.baselines.last = baseline;
                output
            },
        )
        .map_err(|_| LayoutError::Engine)?;
        if let Some(error) = failure {
            return Err(error);
        }
        if self.lifetime.is_disposed() {
            return Err(LayoutError::Disposed);
        }
        Ok(())
    }

    pub fn dispose(&mut self) {
        self.lifetime.dispose();
        self.nodes.clear();
        self.engine = None;
    }
}

fn validate_descriptor(descriptor: &NodeDescriptor, root: RootId) -> Result<(), LayoutError> {
    if !descriptor.style.is_valid()
        || (!descriptor.style.inline_styles.is_empty()
            && !matches!(&descriptor.content,NodeContent::Text(text) if descriptor.style.inline_valid(text)))
    {
        return Err(LayoutError::InvalidStyle);
    }
    if !descriptor.content.is_valid() {
        return Err(LayoutError::InvalidTree);
    }
    if !descriptor.accessibility_relations.is_valid_for(root) {
        return Err(LayoutError::InvalidTree);
    }
    Ok(())
}

fn constraint(known: Option<f32>, available: AvailableSpace, viewport: f32) -> (f32, MeasureMode) {
    if let Some(value) = known {
        return (value.max(0.0), MeasureMode::Definite);
    }
    match available {
        AvailableSpace::Definite(value) => (value.max(0.0), MeasureMode::Definite),
        AvailableSpace::MinContent => (viewport, MeasureMode::MinContent),
        AvailableSpace::MaxContent => (viewport, MeasureMode::MaxContent),
    }
}

fn engine_style(node: &NodeDescriptor) -> Style {
    let style = &node.style;
    let [top, right, bottom, left] = style.padding;
    Style {
        aspect_ratio: node
            .content
            .image_dimensions()
            .map(|[width, height]| width as f32 / height as f32),
        size: Size {
            width: style.width.map_or(Dimension::auto(), Dimension::length),
            height: style.height.map_or(Dimension::auto(), Dimension::length),
        },
        min_size: Size {
            width: LengthPercentageAuto::length(style.min_width),
            height: LengthPercentageAuto::length(style.min_height),
        },
        max_size: Size {
            width: style
                .max_width
                .map_or(LengthPercentageAuto::auto(), LengthPercentageAuto::length),
            height: style
                .max_height
                .map_or(LengthPercentageAuto::auto(), LengthPercentageAuto::length),
        },
        flex_direction: match style.axis {
            Axis::Row => FlexDirection::Row,
            Axis::Column => FlexDirection::Column,
        },
        direction: match style.direction {
            Some(jadix_core::environment::LayoutDirection::RightToLeft) => {
                taffy::style::Direction::Rtl
            }
            Some(jadix_core::environment::LayoutDirection::LeftToRight) | None => {
                taffy::style::Direction::Ltr
            }
        },
        gap: Size {
            width: LengthPercentage::length(style.gap),
            height: LengthPercentage::length(style.gap),
        },
        padding: taffy::geometry::Rect {
            top: LengthPercentage::length(top),
            right: LengthPercentage::length(right),
            bottom: LengthPercentage::length(bottom),
            left: LengthPercentage::length(left),
        },
        flex_grow: style.grow,
        flex_shrink: style.shrink,
        ..Default::default()
    }
}

fn collect_frames(
    tree: &TaffyTree<retained::Leaf>,
    id: taffy::NodeId,
    parent: [f64; 2],
    mapping: &std::collections::HashMap<taffy::NodeId, NodeId>,
    emit: &mut impl FnMut(NodeId, Rect),
) -> Result<(), LayoutError> {
    let layout = tree.layout(id).map_err(|_| LayoutError::Engine)?;
    let frame = Rect {
        x: parent[0] + f64::from(layout.location.x),
        y: parent[1] + f64::from(layout.location.y),
        width: layout.size.width.into(),
        height: layout.size.height.into(),
    };
    if !frame.is_valid() {
        return Err(LayoutError::Engine);
    }
    emit(mapping[&id], frame);
    for child in taffy::TraversePartialTree::child_ids(tree, id) {
        collect_frames(tree, child, [frame.x, frame.y], mapping, emit)?;
    }
    Ok(())
}

fn apply_frames(node: &mut PresentationNode, frames: &BTreeMap<NodeId, Rect>) {
    node.descriptor.frame = frames[&node.id];
    for child in &mut node.children {
        apply_frames(child, frames);
    }
}
