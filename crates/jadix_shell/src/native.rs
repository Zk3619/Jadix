//! Native OS views use the same shell projection as other presentation hosts.
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
mod application;
use crate::{
    handles::EventDelivery,
    mount,
    wire::{ComponentRegistry, Envelope},
    Session, ShellError,
};
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
pub use application::NativeApplication;
use jadix_native::{
    host::{HostError, NativeRoot, PlatformHost},
    layout::LayoutStats,
    presentation::*,
    ResourceLimits,
};
#[derive(Debug)]
pub enum MountError {
    Session(ShellError),
    Host(HostError),
    Properties,
    Unsupported(String),
    Revision,
}
impl From<mount::MountError> for MountError {
    fn from(e: mount::MountError) -> Self {
        match e {
            mount::MountError::Session(e) => Self::Session(e),
            mount::MountError::Properties => Self::Properties,
            mount::MountError::Unsupported(e) => Self::Unsupported(e),
            mount::MountError::Revision => Self::Revision,
        }
    }
}
impl std::fmt::Display for MountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MountError {}
pub struct NativeMount<Host: PlatformHost> {
    root: NativeRoot<Host>,
    projection: mount::Mount,
}
impl<Host: PlatformHost> NativeMount<Host> {
    pub fn new(
        session: &mut Session,
        mut host: Host,
        limits: ResourceLimits,
    ) -> Result<Self, MountError> {
        let lifetime = match session.mount_root() {
            Ok(l) => l,
            Err(e) => {
                host.dispose();
                return Err(MountError::Session(e));
            }
        };
        Ok(Self {
            projection: mount::Mount::new(lifetime.clone()),
            root: NativeRoot::with_lifetime(host, lifetime, limits),
        })
    }
    pub fn root(&self) -> &NativeRoot<Host> {
        &self.root
    }
    pub fn root_mut(&mut self) -> &mut NativeRoot<Host> {
        &mut self.root
    }
    pub fn node_at(&self, path: &[usize]) -> Option<NodeId> {
        self.projection
            .node_at(path)
            .filter(|n| self.root.model().node(*n).is_some())
    }
    pub fn present(
        &mut self,
        session: &Session,
        registry: &ComponentRegistry,
        envelope: &Envelope,
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, MountError> {
        let mut candidate = self.projection.prepare(session, registry, envelope)?;
        let stats = self
            .root
            .present(&mut candidate.roots, viewport, environment)
            .map_err(MountError::Host)?;
        self.projection.accept(candidate)?;
        Ok(stats)
    }
    pub fn poll(&mut self, session: &Session) -> Result<Vec<EventDelivery>, MountError> {
        let events = self.root.poll_events().map_err(MountError::Host)?;
        self.projection.events(session, events).map_err(Into::into)
    }
    pub fn dispose(&mut self) {
        self.root.dispose();
        self.projection.clear();
    }
}
