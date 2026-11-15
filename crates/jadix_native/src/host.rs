use crate::layout::{LayoutError, LayoutStats, MeasureRequest, NativeLayout};
use crate::presentation::{
    CommitBatch, MeasureResult, NodeDescriptor, PlatformEvent, PresentationNode, RootId,
};
use crate::{BackendError, HeadlessRoot, ResourceLimits};

pub use jadix_core::presentation::ApplyReceipt;

pub trait PlatformHost {
    fn is_alive(&self) -> bool {
        true
    }

    fn capabilities(&self) -> crate::presentation::capabilities::BackendCapabilities {
        let mut capabilities = crate::presentation::capabilities::BackendCapabilities::headless();
        capabilities
            .node_kinds
            .retain(|kind| *kind != crate::presentation::capabilities::NodeKind::Popover);
        capabilities
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError>;
    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String>;

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        self.publish(batch.root, batch.revision, roots)?;
        Ok(ApplyReceipt::for_batch(batch))
    }

    fn dispose(&mut self);

    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        Ok(Vec::new())
    }

    fn bind_navigation(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn bind_grid_navigation(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn bind_drag(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn scroll_to(&mut self, _: crate::presentation::NodeId, _: [f64; 2]) -> Result<(), String> {
        Err(String::from(
            "programmatic scrolling is unavailable on this host",
        ))
    }
}

#[derive(Debug)]
pub enum HostError {
    Backend(BackendError),
    Layout(LayoutError),
    Platform(String),
    #[cfg(feature = "motion")]
    Motion(crate::motion::MotionError),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for HostError {}

pub struct NativeRoot<Host: PlatformHost> {
    model: HeadlessRoot,
    layout: NativeLayout,
    host: Host,
    disposed: bool,
    last_layout: LayoutStats,
    last_dirty: Vec<crate::presentation::NodeId>,
    last_operations: [usize; 5],
    environment: Option<(u64, u64)>,
}

impl<Host: PlatformHost> NativeRoot<Host> {
    pub fn new(host: Host) -> Self {
        Self::with_limits(host, ResourceLimits::default())
    }

    pub fn with_limits(host: Host, limits: ResourceLimits) -> Self {
        Self::with_lifetime(
            host,
            crate::presentation::RootLifetime::new(RootId::new()),
            limits,
        )
    }

    pub fn with_lifetime(
        host: Host,
        lifetime: crate::presentation::RootLifetime,
        limits: ResourceLimits,
    ) -> Self {
        let model = HeadlessRoot {
            limits,
            ..HeadlessRoot::with_lifetime(lifetime, host.capabilities())
        };
        let layout = NativeLayout::new(model.lifetime());
        Self {
            model,
            layout,
            host,
            disposed: false,
            last_layout: LayoutStats::default(),
            last_dirty: Vec::new(),
            last_operations: [0; 5],
            environment: None,
        }
    }

    pub fn model(&self) -> &HeadlessRoot {
        &self.model
    }
    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut Host {
        &mut self.host
    }

    pub fn inspection(&self, include_text: bool) -> serde_json::Value {
        let mut result = self.model.inspection(include_text);
        result["layout"] = serde_json::json!({"nodes": self.last_layout.nodes, "measurements": self.last_layout.measurements,
            "cache_hits": self.last_layout.cache_hits, "cached_nodes": self.layout.cached_nodes()});
        result["last_commit"] = serde_json::json!({"creates": self.last_operations[0], "updates": self.last_operations[1],
            "moves": self.last_operations[2], "removes": self.last_operations[3], "focus": self.last_operations[4],
            "dirty_nodes": self.last_dirty.iter().map(|node| node.token()).collect::<Vec<_>>()});
        if let Some((identity, revision)) = self.environment {
            result["environment"] = serde_json::json!({"identity":identity.to_string(),"revision":revision.to_string()});
        }
        result
    }

    pub fn scroll_to(
        &mut self,
        node: crate::presentation::NodeId,
        offset: [f64; 2],
    ) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        if offset
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !self.model.node(node).is_some_and(|node| {
                matches!(
                    node.descriptor.content,
                    crate::presentation::NodeContent::Scroll
                )
            })
        {
            return Err(HostError::Backend(BackendError::InvalidEvent));
        }
        self.host
            .scroll_to(node, offset)
            .map_err(HostError::Platform)
    }

    pub fn present(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, HostError> {
        if self.environment.take().is_some() {
            self.layout.invalidate_measurements();
        }
        let stats = self.prepare_layout(roots, viewport, environment)?;
        self.publish_layout(roots, stats)
    }

    #[cfg(feature = "consistent-ui")]
    pub fn present_consistent(
        &mut self,
        roots: &[PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        theme: &crate::ConsistentTheme,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut projected = theme
            .project(roots, &environment.snapshot())
            .map_err(|error| HostError::Platform(error.into()))?;
        self.present_environment(&mut projected, viewport, environment)
    }

    /// Bind a validated root-local environment to measurement invalidation.
    /// Applications map preferences to content/style; this does not fabricate
    /// missing platform mappings or reset node/edit identities.
    pub fn present_environment(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        let key = (environment.identity(), environment.revision());
        if self.environment != Some(key) {
            self.layout.invalidate_measurements();
        }
        let stats = self.prepare_layout(roots, viewport, key.1)?;
        let result = self.publish_layout(roots, stats)?;
        self.environment = Some(key);
        Ok(result)
    }

    #[cfg(feature = "motion")]
    pub fn present_environment_with_motion(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
    ) -> Result<LayoutStats, HostError> {
        let result = (|| {
            if !environment.is_alive() {
                return Err(HostError::Platform("environment owner disposed".into()));
            }
            let key = (environment.identity(), environment.revision());
            if self.environment != Some(key) {
                self.layout.invalidate_measurements();
            }
            let stats = self.prepare_layout(roots, viewport, key.1)?;
            motion
                .apply(
                    &self.model.lifetime(),
                    roots,
                    now,
                    crate::motion::MotionPolicy::from_environment(&environment.snapshot()),
                )
                .map_err(HostError::Motion)?;
            let result = self.publish_layout(roots, stats)?;
            self.environment = Some(key);
            Ok(result)
        })();
        if self.model.lifetime().is_disposed() {
            motion.dispose();
        }
        result
    }

    #[cfg(all(feature = "motion", feature = "consistent-ui"))]
    pub fn present_consistent_with_motion(
        &mut self,
        roots: &[PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        theme: &crate::ConsistentTheme,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut projected = theme
            .project(roots, &environment.snapshot())
            .map_err(|error| HostError::Platform(error.into()))?;
        self.present_environment_with_motion(&mut projected, viewport, environment, motion, now)
    }

    #[cfg(feature = "motion")]
    pub fn present_with_motion(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
        policy: crate::motion::MotionPolicy,
    ) -> Result<LayoutStats, HostError> {
        let result = (|| {
            if self.environment.take().is_some() {
                self.layout.invalidate_measurements();
            }
            let stats = self.prepare_layout(roots, viewport, environment)?;
            motion
                .apply(&self.model.lifetime(), roots, now, policy)
                .map_err(HostError::Motion)?;
            self.publish_layout(roots, stats)
        })();
        if self.model.lifetime().is_disposed() {
            motion.dispose();
        }
        result
    }

    fn prepare_layout(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut pending: Vec<_> = roots.iter().collect();
        while let Some(node) = pending.pop() {
            if !self.model.capabilities().supports(&node.descriptor.content) {
                return Err(HostError::Backend(BackendError::Unsupported(
                    node.descriptor.content.kind(),
                )));
            }
            pending.extend(&node.children);
        }
        let host = &mut self.host;
        self.layout
            .layout(roots, viewport, environment, |request, descriptor| {
                host.measure(request, descriptor)
            })
            .map_err(HostError::Layout)
    }

    fn publish_layout(
        &mut self,
        roots: &[PresentationNode],
        stats: LayoutStats,
    ) -> Result<LayoutStats, HostError> {
        let mut dirty = std::collections::BTreeSet::new();
        let mut operations = [0usize; 5];
        if let Some(batch) = self.model.diff_batch(roots).map_err(HostError::Backend)? {
            let candidate = self
                .model
                .prepare_commit(batch.clone())
                .map_err(HostError::Backend)?
                .ok_or(HostError::Backend(BackendError::Revision))?;
            let applied = self.host.publish_batch(&batch, roots);
            if applied.as_ref() != Ok(&ApplyReceipt::for_batch(&batch)) {
                let error = applied.err().unwrap_or_else(|| {
                    String::from("platform returned a stale or foreign apply receipt")
                });
                self.dispose();
                return Err(HostError::Platform(error));
            }
            if candidate.lifetime().is_disposed() {
                self.dispose();
                return Err(HostError::Backend(BackendError::Disposed));
            }
            for operation in &batch.operations {
                use crate::presentation::Operation;
                let (index, node) = match operation {
                    Operation::Create { node, .. } => (0, Some(*node)),
                    Operation::Update { node, .. } => (1, Some(*node)),
                    Operation::Move { node, .. } => (2, Some(*node)),
                    Operation::Remove { node } => (3, Some(*node)),
                    Operation::Focus { node } => (4, *node),
                };
                operations[index] += 1;
                if let Some(node) = node {
                    dirty.insert(node);
                }
            }
            self.model = candidate;
        }
        self.last_layout = stats.clone();
        self.last_dirty = dirty.into_iter().collect();
        self.last_operations = operations;
        Ok(stats)
    }

    pub fn event(&mut self, event: &PlatformEvent) -> Result<(), BackendError> {
        if self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(BackendError::Disposed);
        }
        self.model.accept_event(event)
    }

    pub fn focus(&mut self, node: Option<crate::presentation::NodeId>) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        if self.model.focused() == node {
            return Ok(());
        }
        let batch = CommitBatch {
            root: self.model.id(),
            previous: self.model.revision(),
            revision: self
                .model
                .revision()
                .checked_add(1)
                .ok_or(HostError::Backend(BackendError::Revision))?,
            operations: vec![crate::presentation::Operation::Focus { node }],
        };
        let candidate = self
            .model
            .prepare_commit(batch.clone())
            .map_err(HostError::Backend)?
            .ok_or(HostError::Backend(BackendError::Revision))?;
        let mut built = std::collections::BTreeMap::new();
        let mut pending: Vec<_> = self
            .model
            .roots()
            .iter()
            .rev()
            .map(|identifier| (*identifier, false))
            .collect();
        while let Some((identifier, visited)) = pending.pop() {
            let current = self
                .model
                .node(identifier)
                .ok_or(HostError::Backend(BackendError::UnknownNode(identifier)))?;
            if !visited {
                pending.push((identifier, true));
                pending.extend(
                    current
                        .children
                        .iter()
                        .rev()
                        .map(|identifier| (*identifier, false)),
                );
            } else {
                let children = current
                    .children
                    .iter()
                    .map(|identifier| built.remove(identifier).unwrap())
                    .collect();
                built.insert(
                    identifier,
                    PresentationNode {
                        id: identifier,
                        descriptor: current.descriptor.clone(),
                        children,
                    },
                );
            }
        }
        let roots: Vec<_> = self
            .model
            .roots()
            .iter()
            .map(|identifier| built.remove(identifier).unwrap())
            .collect();
        let applied = self.host.publish_batch(&batch, &roots);
        if applied.as_ref() != Ok(&ApplyReceipt::for_batch(&batch))
            || candidate.lifetime().is_disposed()
        {
            let error = applied.err().unwrap_or_else(|| {
                String::from("platform focus returned a stale receipt or disposed root")
            });
            self.dispose();
            return Err(HostError::Platform(error));
        }
        self.model = candidate;
        self.last_operations = [0, 0, 0, 0, 1];
        self.last_dirty = node.into_iter().collect();
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlatformEvent>, HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        let events = match self.host.take_events() {
            Ok(events) => events,
            Err(error) => {
                self.dispose();
                return Err(HostError::Platform(error));
            }
        };
        for event in &events {
            self.event(event).map_err(HostError::Backend)?;
        }
        Ok(events)
    }

    pub fn dispose(&mut self) {
        if !self.disposed {
            self.disposed = true;
            self.model.dispose();
            self.layout.dispose();
            self.last_layout = LayoutStats::default();
            self.last_dirty.clear();
            self.last_operations = [0; 5];
            self.host.dispose();
        }
    }
}

impl<Host: PlatformHost> Drop for NativeRoot<Host> {
    fn drop(&mut self) {
        self.dispose();
    }
}
