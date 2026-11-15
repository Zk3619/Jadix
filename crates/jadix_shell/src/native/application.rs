//! Reuse the budgeted SDK runtime with any existing native PlatformHost.
use crate::application::{ApplicationError, ScriptApplication};
use jadix_native::{
    host::{NativeRoot, PlatformHost},
    layout::LayoutStats,
    ResourceLimits,
};

pub struct NativeApplication<Host: PlatformHost> {
    app: ScriptApplication,
    root: NativeRoot<Host>,
    viewport: Option<([f32; 2], u64)>,
    layout_stats: LayoutStats,
}

impl<Host: PlatformHost> NativeApplication<Host> {
    pub fn new(app: ScriptApplication, host: Host, limits: ResourceLimits) -> Self {
        let root = NativeRoot::with_lifetime(host, app.lifetime(), limits);
        Self {
            app,
            root,
            viewport: None,
            layout_stats: Default::default(),
        }
    }
    pub fn application(&self) -> &ScriptApplication {
        &self.app
    }
    pub fn root(&self) -> &NativeRoot<Host> {
        &self.root
    }
    pub fn root_mut(&mut self) -> &mut NativeRoot<Host> {
        &mut self.root
    }
    pub fn layout_stats(&self) -> &LayoutStats {
        &self.layout_stats
    }

    /// NativeRoot performs keyed structural publication and validates receipts.
    /// Native layout and SDK projection still scan snapshots; no O(k) claim.
    pub fn publish(
        &mut self,
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<usize, ApplicationError> {
        let root = &mut self.root;
        let stats = &mut self.layout_stats;
        let count = self.app.publish(|nodes| {
            *stats = root
                .present(&mut nodes.to_vec(), viewport, environment)
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        if count == 0
            && self
                .viewport
                .is_some_and(|old| old != (viewport, environment))
        {
            self.layout_stats = root
                .present(&mut self.app.roots().to_vec(), viewport, environment)
                .map_err(|e| ApplicationError::Host(e.to_string()))?;
        }
        self.viewport = Some((viewport, environment));
        Ok(count)
    }
    /// Poll only validated events belonging to this native root, then run JS jobs.
    pub fn poll(&mut self) -> Result<(), ApplicationError> {
        let events = self
            .root
            .poll_events()
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.app.events(events)
    }
    pub fn dispose(&mut self) {
        self.root.dispose();
        self.app.dispose();
    }
}
impl<Host: PlatformHost> Drop for NativeApplication<Host> {
    fn drop(&mut self) {
        self.dispose();
    }
}
