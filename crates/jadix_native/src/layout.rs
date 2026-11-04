use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::presentation::{
    MeasureKey, MeasureResult, NodeContent, NodeDescriptor, NodeId, PresentationNode, Rect,
    RootLifetime,
    style::{Axis, NodeStyle},
};
use taffy::prelude::*;

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
    pub measurements: usize,
    pub cache_hits: usize,
    pub nodes: usize,
}

#[derive(Clone)]
struct CachedNode {
    content: NodeContent,
    style: NodeStyle,
    content_revision: u64,
    style_revision: u64,
    measures: Vec<(MeasureRequest, MeasureResult)>,
}

pub struct NativeLayout {
    lifetime: RootLifetime,
    nodes: BTreeMap<NodeId, CachedNode>,
    environment: u64,
    epoch: u64,
}

impl NativeLayout {
    pub fn new(lifetime: RootLifetime) -> Self {
        Self {
            lifetime,
            nodes: BTreeMap::new(),
            environment: 0,
            epoch: 0,
        }
    }

    pub fn cached_nodes(&self) -> usize {
        if self.lifetime.is_disposed() {
            0
        } else {
            self.nodes.len()
        }
    }

    pub fn layout(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment_revision: u64,
        mut measure: impl FnMut(MeasureRequest, &NodeDescriptor) -> Result<MeasureResult, LayoutError>,
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
            self.environment = environment_revision;
        }
        let mut seen = BTreeSet::new();
        let mut metadata = BTreeMap::new();
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
            if !node.descriptor.style.is_valid() {
                return Err(LayoutError::InvalidStyle);
            }
            if !node.descriptor.content.is_valid() {
                return Err(LayoutError::InvalidTree);
            }
            metadata.insert(node.id, (parent, &node.descriptor.content));
            pending.extend(
                node.children
                    .iter()
                    .map(|child| (child, Some(node.id), depth + 1)),
            );
        }
        let overlay_order =
            crate::popover::overlay_order(&metadata).map_err(|_| LayoutError::InvalidTree)?;
        self.nodes.retain(|node, _| seen.contains(node));
        let mut tree: TaffyTree<PresentationNode> = TaffyTree::new();
        let mut mapping = HashMap::new();
        let mut children = Vec::new();
        for root in roots.iter() {
            children.push(build(&mut tree, &mut mapping, root)?);
        }
        let mut portals = HashSet::new();
        let mut overlays = BTreeMap::new();
        let mut pending: Vec<_> = roots.iter().rev().collect();
        while let Some(node) = pending.pop() {
            if let NodeContent::Dialog { open, .. } | NodeContent::Popover { open, .. } =
                node.descriptor.content
            {
                if !open {
                    continue;
                }
                let mut panel = node.clone();
                panel.descriptor.content = NodeContent::Container;
                let style = &mut panel.descriptor.style;
                let is_popover = matches!(node.descriptor.content, NodeContent::Popover { .. });
                let inset = if is_popover { 16.0 } else { 32.0 };
                let available_width = (viewport[0] - inset).max(0.0);
                let available_height = (viewport[1] - inset).max(0.0);
                if is_popover
                    && (style.padding[1] + style.padding[3] > available_width
                        || style.padding[0] + style.padding[2] > available_height)
                {
                    return Err(LayoutError::InvalidStyle);
                }
                let width = style
                    .width
                    .unwrap_or(if is_popover { 320.0 } else { 480.0 })
                    .min(available_width);
                style.width = Some(width);
                style.min_width = style.min_width.min(width);
                style.min_height = style.min_height.min(available_height);
                style.height = style.height.map(|height| height.min(available_height));
                style.max_height = Some(
                    style
                        .max_height
                        .unwrap_or(available_height)
                        .min(available_height),
                );
                let id = build(&mut tree, &mut mapping, &panel)?;
                let mut engine_style = tree.style(id).map_err(|_| LayoutError::Engine)?.clone();
                engine_style.position = Position::Absolute;
                engine_style.inset.left = LengthPercentageAuto::length(0.0);
                engine_style.inset.top = LengthPercentageAuto::length(0.0);
                tree.set_style(id, engine_style)
                    .map_err(|_| LayoutError::Engine)?;
                portals.insert(id);
                overlays.insert(node.id, id);
                children.push(id);
            }
            pending.extend(node.children.iter().rev());
        }
        let root = tree
            .new_with_children(
                Style {
                    size: Size {
                        width: Dimension::length(viewport[0]),
                        height: Dimension::length(viewport[1]),
                    },
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                },
                &children,
            )
            .map_err(|_| LayoutError::Engine)?;
        let mut stats = LayoutStats {
            nodes: seen.len(),
            ..Default::default()
        };
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
                        });
                        if cached.content != node.descriptor.content {
                            cached.content = node.descriptor.content.clone();
                            cached.content_revision = epoch;
                            cached.measures.clear();
                        }
                        if cached.style != node.descriptor.style {
                            let mut metrics = cached.style.clone();
                            metrics.foreground = node.descriptor.style.foreground;
                            metrics.background = node.descriptor.style.background;
                            if metrics != node.descriptor.style {
                                cached.style_revision = epoch;
                                cached.measures.clear();
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
                                if cached.measures.len() >= 16 {
                                    cached.measures.remove(0);
                                }
                                if !cached.measures.iter().any(|(key, _)| *key == request) {
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
        let mut frames: BTreeMap<_, _> = seen.into_iter().map(|id| (id, Rect::default())).collect();
        for child in children {
            if !portals.contains(&child) {
                collect_frames(&tree, child, [0.0, 0.0], &mapping, &mut frames)?;
            }
        }
        for node in overlay_order {
            let Some(child) = overlays.get(&node) else {
                continue;
            };
            let size = tree.layout(*child).map_err(|_| LayoutError::Engine)?.size;
            let offset = match metadata[&node].1 {
                NodeContent::Popover {
                    anchor,
                    placement,
                    gap,
                    ..
                } => crate::popover::position(
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
            collect_frames(&tree, *child, offset, &mapping, &mut frames)?;
        }
        for node in roots {
            apply_frames(node, &frames);
        }
        Ok(stats)
    }

    pub fn dispose(&mut self) {
        self.lifetime.dispose();
        self.nodes.clear();
    }
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

fn build(
    tree: &mut TaffyTree<PresentationNode>,
    mapping: &mut HashMap<taffy::NodeId, NodeId>,
    node: &PresentationNode,
) -> Result<taffy::NodeId, LayoutError> {
    if matches!(
        node.descriptor.content,
        NodeContent::Dialog { .. } | NodeContent::Popover { .. }
    ) {
        let id = tree
            .new_leaf(Style {
                display: Display::None,
                ..Default::default()
            })
            .map_err(|_| LayoutError::Engine)?;
        mapping.insert(id, node.id);
        return Ok(id);
    }
    let style = &node.descriptor.style;
    let [top, right, bottom, left] = style.padding;
    let engine_style = Style {
        aspect_ratio: node
            .descriptor
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
    };
    let id = if node.descriptor.content.accepts_children() {
        let children = node
            .children
            .iter()
            .map(|child| build(tree, mapping, child))
            .collect::<Result<Vec<_>, _>>()?;
        tree.new_with_children(engine_style, &children)
    } else {
        tree.new_leaf_with_context(engine_style, node.clone())
    }
    .map_err(|_| LayoutError::Engine)?;
    mapping.insert(id, node.id);
    Ok(id)
}

fn collect_frames(
    tree: &TaffyTree<PresentationNode>,
    id: taffy::NodeId,
    parent: [f64; 2],
    mapping: &HashMap<taffy::NodeId, NodeId>,
    frames: &mut BTreeMap<NodeId, Rect>,
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
    frames.insert(mapping[&id], frame);
    for child in tree.children(id).map_err(|_| LayoutError::Engine)? {
        collect_frames(tree, child, [frame.x, frame.y], mapping, frames)?;
    }
    Ok(())
}

fn apply_frames(node: &mut PresentationNode, frames: &BTreeMap<NodeId, Rect>) {
    node.descriptor.frame = frames[&node.id];
    for child in &mut node.children {
        apply_frames(child, frames);
    }
}
