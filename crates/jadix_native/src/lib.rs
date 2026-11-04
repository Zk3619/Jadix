use std::collections::BTreeMap;

pub use jadix_core::presentation;
use presentation::capabilities::{BackendCapabilities, NodeKind};
use presentation::{
    CommitBatch, DragPhase, EventPayload, NodeContent, NodeDescriptor, NodeId, Operation,
    PlatformEvent, RootId, RootLifetime, valid_utf16_range,
};

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;
#[cfg(all(feature = "dom", target_arch = "wasm32"))]
pub mod dom;
mod grid;
pub mod host;
pub mod images;
mod inspection;
pub mod layout;
#[cfg(feature = "motion")]
pub mod motion;
#[cfg(all(feature = "ohos", target_env = "ohos"))]
pub mod ohos;
mod popover;
mod reconcile;
mod resources;
mod retired;
#[cfg(feature = "code")]
pub mod syntax;
pub use resources::{ResourceKind, ResourceLimits, ResourceUsage};
#[cfg(feature = "uikit")]
pub mod uikit;
#[cfg(all(feature = "windows", windows))]
pub mod windows;
pub mod wire;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    Disposed,
    WrongRoot,
    Revision,
    UnknownNode(NodeId),
    ReusedNode(NodeId),
    InvalidParent,
    InvalidIndex,
    InvalidGeometry,
    InvalidStyle,
    InvalidContent,
    InvalidGrid(NodeId),
    InvalidAnchor(NodeId),
    InvalidEvent,
    ResourceLimit(ResourceKind),
    Unsupported(NodeKind),
    UnsupportedFocus,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for BackendError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub descriptor: NodeDescriptor,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    edit_revision: u64,
}

#[derive(Clone)]
pub struct HeadlessRoot {
    capabilities: BackendCapabilities,
    lifetime: RootLifetime,
    revision: u64,
    nodes: BTreeMap<NodeId, Node>,
    roots: Vec<NodeId>,
    retired: retired::RetiredIds,
    focus: Option<NodeId>,
    limits: ResourceLimits,
    usage: ResourceUsage,
}

impl Default for HeadlessRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadlessRoot {
    pub fn new() -> Self {
        Self::with_capabilities(BackendCapabilities::headless())
    }

    pub fn with_capabilities(capabilities: BackendCapabilities) -> Self {
        Self::with_lifetime(RootLifetime::new(RootId::new()), capabilities)
    }

    pub fn with_limits(limits: ResourceLimits) -> Self {
        Self {
            limits,
            ..Self::new()
        }
    }

    pub fn with_lifetime(lifetime: RootLifetime, capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            lifetime,
            revision: 0,
            nodes: BTreeMap::new(),
            roots: Vec::new(),
            retired: retired::RetiredIds::default(),
            focus: None,
            limits: ResourceLimits::default(),
            usage: ResourceUsage::default(),
        }
    }

    pub fn id(&self) -> RootId {
        self.lifetime.id()
    }
    pub fn resource_limits(&self) -> ResourceLimits {
        self.limits
    }
    pub fn resource_usage(&self) -> ResourceUsage {
        self.usage
    }

    pub(crate) fn check_resources(
        &self,
        roots: &[presentation::PresentationNode],
    ) -> Result<(), BackendError> {
        resources::tree_usage(roots, self.limits).map(|_| ())
    }
    pub fn lifetime(&self) -> RootLifetime {
        self.lifetime.clone()
    }
    pub fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn roots(&self) -> &[NodeId] {
        if self.lifetime.is_disposed() {
            &[]
        } else {
            &self.roots
        }
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        if self.lifetime.is_disposed() {
            None
        } else {
            self.nodes.get(&id)
        }
    }
    pub fn focused(&self) -> Option<NodeId> {
        if self.lifetime.is_disposed() {
            None
        } else {
            self.focus
        }
    }

    pub fn commit(&mut self, batch: CommitBatch) -> Result<bool, BackendError> {
        if let Some(candidate) = self.prepare_commit(batch)? {
            *self = candidate;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn prepare_commit(&self, batch: CommitBatch) -> Result<Option<Self>, BackendError> {
        if self.lifetime.is_disposed() {
            return Err(BackendError::Disposed);
        }
        if batch.root != self.id() {
            return Err(BackendError::WrongRoot);
        }
        if batch.previous != self.revision
            || batch.revision != self.revision.checked_add(1).ok_or(BackendError::Revision)?
        {
            return Err(BackendError::Revision);
        }
        if batch.operations.is_empty() {
            return Ok(None);
        }
        let mut candidate = Self {
            capabilities: self.capabilities.clone(),
            lifetime: self.lifetime.clone(),
            revision: self.revision,
            nodes: self.nodes.clone(),
            roots: self.roots.clone(),
            retired: self.retired.clone(),
            focus: self.focus,
            limits: self.limits,
            usage: self.usage,
        };
        for operation in batch.operations {
            candidate.apply(operation)?;
        }
        popover::validate_anchors(
            &candidate
                .nodes
                .iter()
                .map(|(id, node)| (*id, (node.parent, &node.descriptor.content)))
                .collect(),
        )
        .map_err(BackendError::InvalidAnchor)?;
        candidate.validate_grid()?;
        let mut usage = ResourceUsage {
            retired_nodes: candidate.retired.len(),
            retired_ranges: candidate.retired.ranges(),
            ..Default::default()
        };
        if usage.retired_nodes > candidate.limits.retired_nodes {
            return Err(BackendError::ResourceLimit(ResourceKind::RetiredNodes));
        }
        if usage.retired_ranges > candidate.limits.retired_ranges {
            return Err(BackendError::ResourceLimit(ResourceKind::RetiredRanges));
        }
        for node in candidate.nodes.values() {
            usage.add(&node.descriptor, candidate.limits)?;
        }
        candidate.usage = usage;
        if candidate.focus.is_some_and(|id| !candidate.can_focus(id)) {
            candidate.focus = None;
        }
        candidate.revision = batch.revision;
        Ok(Some(candidate))
    }

    fn children(&mut self, parent: Option<NodeId>) -> Result<&mut Vec<NodeId>, BackendError> {
        match parent {
            Some(parent) => {
                let node = self
                    .nodes
                    .get_mut(&parent)
                    .ok_or(BackendError::UnknownNode(parent))?;
                if !node.descriptor.content.accepts_children() {
                    return Err(BackendError::InvalidParent);
                }
                Ok(&mut node.children)
            }
            None => Ok(&mut self.roots),
        }
    }

    fn apply(&mut self, operation: Operation) -> Result<(), BackendError> {
        match operation {
            Operation::Create {
                node,
                parent,
                index,
                descriptor,
            } => {
                if !descriptor.content.is_valid() {
                    return Err(BackendError::InvalidContent);
                }
                if !descriptor.style.is_valid() {
                    return Err(BackendError::InvalidStyle);
                }
                if !self.capabilities.supports(&descriptor.content) {
                    return Err(BackendError::Unsupported(descriptor.content.kind()));
                }
                if node.root != self.id() {
                    return Err(BackendError::WrongRoot);
                }
                if self.nodes.contains_key(&node) || self.retired.contains(&node) {
                    return Err(BackendError::ReusedNode(node));
                }
                if !descriptor.frame.is_valid() {
                    return Err(BackendError::InvalidGeometry);
                }
                let children = self.children(parent)?;
                if index > children.len() {
                    return Err(BackendError::InvalidIndex);
                }
                children.insert(index, node);
                let edit_revision = match descriptor.content {
                    NodeContent::Input { revision, .. }
                    | NodeContent::Textarea { revision, .. }
                    | NodeContent::Checkbox { revision, .. }
                    | NodeContent::Switch { revision, .. }
                    | NodeContent::Select { revision, .. }
                    | NodeContent::Dialog { revision, .. }
                    | NodeContent::Popover { revision, .. } => revision,
                    _ => 0,
                };
                self.nodes.insert(
                    node,
                    Node {
                        descriptor,
                        parent,
                        children: Vec::new(),
                        edit_revision,
                    },
                );
            }
            Operation::Update { node, descriptor } => {
                if !descriptor.content.is_valid() {
                    return Err(BackendError::InvalidContent);
                }
                if !descriptor.style.is_valid() {
                    return Err(BackendError::InvalidStyle);
                }
                if !self.capabilities.supports(&descriptor.content) {
                    return Err(BackendError::Unsupported(descriptor.content.kind()));
                }
                if !descriptor.frame.is_valid() {
                    return Err(BackendError::InvalidGeometry);
                }
                let current = self
                    .nodes
                    .get_mut(&node)
                    .ok_or(BackendError::UnknownNode(node))?;
                if !descriptor.content.accepts_children() && !current.children.is_empty() {
                    return Err(BackendError::InvalidParent);
                }
                if std::mem::discriminant(&descriptor.content)
                    != std::mem::discriminant(&current.descriptor.content)
                {
                    return Err(BackendError::InvalidParent);
                }
                if let NodeContent::Input { revision, .. }
                | NodeContent::Textarea { revision, .. }
                | NodeContent::Checkbox { revision, .. }
                | NodeContent::Switch { revision, .. }
                | NodeContent::Select { revision, .. }
                | NodeContent::Dialog { revision, .. }
                | NodeContent::Popover { revision, .. } = descriptor.content
                {
                    if revision < current.edit_revision {
                        return Err(BackendError::Revision);
                    }
                    current.edit_revision = revision;
                }
                current.descriptor = descriptor;
                if self.focus == Some(node) && !self.can_focus(node) {
                    self.focus = None;
                }
            }
            Operation::Move {
                node,
                parent,
                index,
            } => {
                let previous = self
                    .nodes
                    .get(&node)
                    .ok_or(BackendError::UnknownNode(node))?
                    .parent;
                let mut ancestor = parent;
                while let Some(id) = ancestor {
                    if id == node {
                        return Err(BackendError::InvalidParent);
                    }
                    ancestor = self
                        .nodes
                        .get(&id)
                        .ok_or(BackendError::UnknownNode(id))?
                        .parent;
                }
                self.children(previous)?.retain(|child| *child != node);
                let children = self.children(parent)?;
                if index > children.len() {
                    return Err(BackendError::InvalidIndex);
                }
                children.insert(index, node);
                self.nodes.get_mut(&node).unwrap().parent = parent;
            }
            Operation::Remove { node } => {
                let parent = self
                    .nodes
                    .get(&node)
                    .ok_or(BackendError::UnknownNode(node))?
                    .parent;
                self.children(parent)?.retain(|child| *child != node);
                let mut pending = vec![node];
                while let Some(id) = pending.pop() {
                    let removed = self.nodes.remove(&id).unwrap();
                    pending.extend(removed.children);
                    self.retired.insert(id);
                    if self.focus == Some(id) {
                        self.focus = None;
                    }
                }
            }
            Operation::Focus { node } => {
                if !self.capabilities.focus && node.is_some() {
                    return Err(BackendError::UnsupportedFocus);
                }
                if node.is_some_and(|id| !self.can_focus(id)) {
                    return Err(BackendError::InvalidEvent);
                }
                self.focus = node;
            }
        }
        Ok(())
    }

    fn can_focus(&self, node: NodeId) -> bool {
        self.event_visible(node)
            && self.nodes.get(&node).is_some_and(|node| {
                matches!(
                    node.descriptor.content,
                    NodeContent::Button { enabled: true, .. }
                        | NodeContent::Input { enabled: true, .. }
                        | NodeContent::Textarea { enabled: true, .. }
                        | NodeContent::Checkbox { enabled: true, .. }
                        | NodeContent::Switch { enabled: true, .. }
                        | NodeContent::Select { enabled: true, .. }
                )
            })
    }

    pub fn accept_event(&mut self, event: &PlatformEvent) -> Result<(), BackendError> {
        if self.lifetime.is_disposed() {
            return Err(BackendError::Disposed);
        }
        if event.node.root != self.id() {
            return Err(BackendError::WrongRoot);
        }
        if !self.nodes.contains_key(&event.node) {
            return Err(BackendError::UnknownNode(event.node));
        }
        if !self.event_visible(event.node)
            && !matches!(
                &event.payload,
                EventPayload::Drag {
                    phase: DragPhase::Cancelled,
                    ..
                }
            )
        {
            return Err(BackendError::InvalidEvent);
        }
        if !self.capabilities.focus && matches!(event.payload, EventPayload::Focus) {
            return Err(BackendError::UnsupportedFocus);
        }
        let node = self
            .nodes
            .get_mut(&event.node)
            .ok_or(BackendError::UnknownNode(event.node))?;
        match (&node.descriptor.content, &event.payload) {
            (NodeContent::Button { enabled: true, .. }, EventPayload::Expansion { .. })
                if node.descriptor.accessible_role.is_none()
                    && node.descriptor.accessible_expanded.is_some() => {}
            (NodeContent::Button { enabled: true, .. }, EventPayload::Tree { action })
                if node.descriptor.accessible_role
                    == Some(presentation::AccessibleRole::TreeItem)
                    && match action {
                        presentation::TreeAction::Expand | presentation::TreeAction::Collapse => {
                            node.descriptor.accessible_expanded.is_some()
                        }
                        _ => node.descriptor.accessible_selected.is_some(),
                    } => {}
            (NodeContent::Button { enabled: true, .. }, EventPayload::Navigate { .. }) => {}
            (NodeContent::Input { enabled: true, .. }, EventPayload::Navigate { .. }) => {}
            (
                NodeContent::Button { .. },
                EventPayload::Drag {
                    phase: DragPhase::Cancelled,
                    screen_position,
                },
            ) if screen_position
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1_000_000.0) => {}
            (
                NodeContent::Button { enabled: true, .. },
                EventPayload::Drag {
                    screen_position, ..
                },
            ) if screen_position
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1_000_000.0) => {}
            (
                NodeContent::Popover {
                    open: true,
                    close_on_outside_click: close_on_overlay_click,
                    ..
                }
                | NodeContent::Dialog {
                    open: true,
                    close_on_overlay_click,
                    ..
                },
                EventPayload::Dismiss { backdrop, revision },
            ) if *revision > node.edit_revision && (!backdrop || *close_on_overlay_click) => {
                node.edit_revision = *revision;
            }
            (NodeContent::Button { enabled: true, .. }, EventPayload::Activate) => {}
            (
                NodeContent::Button { enabled: true, .. }
                | NodeContent::Input { enabled: true, .. }
                | NodeContent::Textarea { enabled: true, .. }
                | NodeContent::Checkbox { enabled: true, .. }
                | NodeContent::Switch { enabled: true, .. }
                | NodeContent::Select { enabled: true, .. },
                EventPayload::Focus,
            ) => {
                self.focus = Some(event.node);
            }
            (NodeContent::Scroll, EventPayload::Scroll { x, y })
                if x.is_finite() && y.is_finite() => {}
            (
                NodeContent::Input { enabled: true, .. }
                | NodeContent::Textarea { enabled: true, .. },
                EventPayload::Edit {
                    value,
                    revision,
                    selection_utf16,
                    composing_utf16,
                },
            ) if *revision > node.edit_revision
                && valid_utf16_range(value, *selection_utf16)
                && composing_utf16.is_none_or(|range| valid_utf16_range(value, range)) =>
            {
                node.edit_revision = *revision;
            }
            (
                NodeContent::Checkbox { enabled: true, .. }
                | NodeContent::Switch { enabled: true, .. },
                EventPayload::Toggle { revision, .. },
            ) if *revision > node.edit_revision => {
                node.edit_revision = *revision;
            }
            (
                NodeContent::Select {
                    options,
                    enabled: true,
                    ..
                },
                EventPayload::Select { value, revision },
            ) if *revision > node.edit_revision
                && options
                    .iter()
                    .any(|option| option.enabled && option.value == *value) =>
            {
                node.edit_revision = *revision;
            }
            _ => return Err(BackendError::InvalidEvent),
        }
        Ok(())
    }

    pub fn dispose(&mut self) {
        self.lifetime.dispose();
        self.nodes.clear();
        self.roots.clear();
        self.retired.clear();
        self.focus = None;
        self.usage = ResourceUsage::default();
    }

    fn event_visible(&self, target: NodeId) -> bool {
        let mut pending: Vec<_> = self.roots.iter().rev().copied().collect();
        let mut modal = None;
        while let Some(id) = pending.pop() {
            let node = &self.nodes[&id];
            match node.descriptor.content {
                NodeContent::Dialog { open: false, .. }
                | NodeContent::Popover { open: false, .. } => continue,
                NodeContent::Dialog { open: true, .. } => modal = Some(id),
                _ => {}
            }
            pending.extend(node.children.iter().rev().copied());
        }
        let mut current = Some(target);
        let mut in_modal = modal.is_none();
        while let Some(id) = current {
            let Some(node) = self.nodes.get(&id) else {
                return false;
            };
            if matches!(
                node.descriptor.content,
                NodeContent::Dialog { open: false, .. } | NodeContent::Popover { open: false, .. }
            ) {
                return false;
            }
            in_modal |= modal == Some(id);
            current = node.parent;
        }
        in_modal
    }
}
