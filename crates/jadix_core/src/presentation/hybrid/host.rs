use super::{policy::*, HybridCoordinator, HybridGeometry, Presentation, Rect, RootLifetime};

pub trait NativeAdapter {
    fn capabilities(&self) -> EmbeddingCapabilities;
    fn prepare(
        &mut self,
        revision: u64,
        geometry: &[HybridGeometry],
    ) -> Result<(), CompositionError>;
    fn publish(&mut self, revision: u64) -> Result<(), CompositionError>;
    fn cancel(&mut self, revision: u64) -> Result<(), CompositionError>;
    fn dispose(&mut self);
}

pub struct HybridHost<Adapter: NativeAdapter> {
    coordinator: HybridCoordinator,
    adapter: Adapter,
    lifetime: RootLifetime,
    pending: Option<(u64, Vec<HybridGeometry>)>,
    visible: Vec<HybridGeometry>,
    damage: Vec<Rect>,
    disposed: bool,
}

impl<Adapter: NativeAdapter> HybridHost<Adapter> {
    pub fn new(lifetime: RootLifetime, adapter: Adapter) -> Self {
        Self {
            coordinator: HybridCoordinator::with_lifetime(lifetime.clone()),
            adapter,
            lifetime,
            pending: None,
            visible: Vec::new(),
            damage: Vec::new(),
            disposed: false,
        }
    }

    pub fn coordinator(&self) -> &HybridCoordinator {
        &self.coordinator
    }

    pub fn focus(&mut self, revision: u64, node: super::super::NodeId) -> bool {
        self.coordinator.focus(revision, node)
    }
    pub fn blur(&mut self, revision: u64, node: super::super::NodeId) -> bool {
        self.coordinator.blur(revision, node)
    }
    pub fn set_composing(
        &mut self,
        revision: u64,
        node: super::super::NodeId,
        composing: bool,
    ) -> bool {
        self.coordinator.set_composing(revision, node, composing)
    }
    pub fn capture_pointer(&mut self, revision: u64, node: super::super::NodeId) -> bool {
        self.coordinator.capture_pointer(revision, node)
    }
    pub fn release_pointer(&mut self, revision: u64, node: super::super::NodeId) -> bool {
        self.coordinator.release_pointer(revision, node)
    }

    pub fn stage(
        &mut self,
        revision: u64,
        mode: CompositionMode,
        geometry: Vec<HybridGeometry>,
    ) -> Result<(), CompositionError> {
        self.ensure_live()?;
        self.adapter.capabilities().validate(mode, &geometry)?;
        if !self.coordinator.stage(revision, geometry.clone()) {
            return Err(CompositionError::Revision);
        }
        let native: Vec<_> = geometry
            .iter()
            .filter(|entry| entry.presentation == Presentation::Native)
            .cloned()
            .collect();
        if let Err(error) = self.adapter.prepare(revision, &native) {
            self.dispose();
            return Err(error);
        }
        self.pending = Some((revision, geometry));
        Ok(())
    }

    pub fn present(&mut self, revision: u64, gpu_ready: bool) -> Result<bool, CompositionError> {
        self.ensure_live()?;
        let Some((current, geometry)) = &self.pending else {
            return Err(CompositionError::Revision);
        };
        if *current != revision {
            return Err(CompositionError::Revision);
        }
        if !gpu_ready
            && geometry
                .iter()
                .any(|entry| entry.presentation == Presentation::Gpu)
        {
            return Ok(false);
        }
        if !self.coordinator.can_confirm(revision, gpu_ready, true) {
            return Err(CompositionError::Revision);
        }
        if let Err(error) = self.adapter.publish(revision) {
            self.dispose();
            return Err(error);
        }
        if !self.coordinator.confirm(revision, gpu_ready, true) {
            self.dispose();
            return Err(CompositionError::Revision);
        }
        let (_, geometry) = self.pending.take().unwrap();
        self.damage.clear();
        for entry in self.visible.iter().chain(geometry.iter()) {
            if entry.presentation == Presentation::Native
                && entry.visible
                && !self
                    .visible
                    .iter()
                    .find(|old| old.node == entry.node)
                    .zip(geometry.iter().find(|new| new.node == entry.node))
                    .is_some_and(|(old, new)| {
                        old.frame == new.frame
                            && old.clips == new.clips
                            && old.visible == new.visible
                            && old.presentation == new.presentation
                    })
            {
                self.damage.push(entry.frame);
            }
        }
        self.visible = geometry;
        Ok(true)
    }

    pub fn damage(&self) -> &[Rect] {
        if self.lifetime.is_disposed() {
            &[]
        } else {
            &self.damage
        }
    }

    pub fn cancel(&mut self, revision: u64) -> Result<(), CompositionError> {
        self.ensure_live()?;
        if self
            .pending
            .as_ref()
            .is_none_or(|(current, _)| *current != revision)
        {
            return Err(CompositionError::Revision);
        }
        if let Err(error) = self.adapter.cancel(revision) {
            self.dispose();
            return Err(error);
        }
        self.coordinator.cancel(revision);
        self.pending = None;
        Ok(())
    }

    fn ensure_live(&mut self) -> Result<(), CompositionError> {
        if self.lifetime.is_disposed() {
            self.dispose();
            Err(CompositionError::Disposed)
        } else {
            Ok(())
        }
    }

    pub fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.coordinator.dispose();
        self.adapter.dispose();
        self.pending = None;
        self.visible.clear();
        self.damage.clear();
    }
}

impl<Adapter: NativeAdapter> Drop for HybridHost<Adapter> {
    fn drop(&mut self) {
        self.dispose();
    }
}
