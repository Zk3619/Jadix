use crate::{crossterm::event::Event, ratatui::layout::Rect, CellSurface};
use jadix_core::presentation::PlatformEvent;
use jadix_shell::application::{ApplicationError, ScriptApplication};
/// Character presentation for the same shell VM/SDK used by other hosts.
pub struct ShellApplication {
    app: ScriptApplication,
    surface: CellSurface,
    commands: Option<crate::CommandRouter>,
    command_scope: Option<String>,
    viewport: Option<(Rect, u64)>,
    layout_stats: jadix_layout::presentation::LayoutStats,
    exit_requested: bool,
}
impl ShellApplication {
    pub fn new(app: ScriptApplication, cell_size: [f64; 2]) -> Result<Self, ApplicationError> {
        let surface = CellSurface::new(app.lifetime(), cell_size)
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        Ok(Self {
            app,
            surface,
            commands: None,
            command_scope: None,
            viewport: None,
            layout_stats: Default::default(),
            exit_requested: false,
        })
    }
    pub fn application(&self) -> &ScriptApplication {
        &self.app
    }
    pub fn surface(&self) -> &CellSurface {
        &self.surface
    }
    pub fn surface_mut(&mut self) -> &mut CellSurface {
        &mut self.surface
    }
    /// Ask the embedding host to stop dispatching before the VM is disposed.
    pub fn request_exit(&mut self) {
        if self.exit_requested {
            return;
        }
        self.exit_requested = true;
        if let Some(router) = &mut self.commands {
            router.cancel();
        }
        self.surface.cancel_capture();
    }
    /// Whether the host should stop processing this application.
    pub fn exit_requested(&self) -> bool {
        self.exit_requested || self.app.lifetime().is_disposed()
    }
    pub fn command_registry(&self) -> jadix_core::commands::CommandRegistry {
        jadix_core::commands::CommandRegistry::new(self.app.lifetime())
    }
    pub fn set_command_router(&mut self, router: crate::CommandRouter) {
        self.commands = Some(router);
    }
    pub fn set_command_scope(&mut self, scope: Option<String>) {
        self.command_scope = scope;
        if let Some(router) = &mut self.commands {
            router.cancel();
        }
    }
    pub fn publish(&mut self, area: Rect, environment: u64) -> Result<usize, ApplicationError> {
        if self.exit_requested() {
            return Err(ApplicationError::Host("application is exiting".into()));
        }
        let surface = &mut self.surface;
        let stats = &mut self.layout_stats;
        let mut published = false;
        let count = self.app.publish_commits(|batch| {
            *stats = surface
                .commit_auto(batch, area, environment)
                .map_err(|e| e.to_string())?;
            published = true;
            Ok(())
        })?;
        if !published && self.viewport.is_some_and(|old| old != (area, environment)) {
            self.layout_stats = surface
                .relayout_structural(area, environment)
                .map_err(|e| ApplicationError::Host(e.to_string()))?;
        }
        self.viewport = Some((area, environment));
        Ok(count)
    }
    pub fn layout_stats(&self) -> &jadix_layout::presentation::LayoutStats {
        &self.layout_stats
    }
    pub fn events(
        &mut self,
        events: impl IntoIterator<Item = PlatformEvent>,
    ) -> Result<(), ApplicationError> {
        if self.exit_requested() {
            return Err(ApplicationError::Host("application is exiting".into()));
        }
        self.app.events(events)
    }
    pub fn handle(&mut self, event: Event) -> Result<(), ApplicationError> {
        if self.exit_requested() {
            return Err(ApplicationError::Host("application is exiting".into()));
        }
        if let Some(router) = &mut self.commands {
            let context = crate::CommandContext {
                allowed: !self.surface.has_modal() && self.surface.input_active(),
                in_editor: self.surface.editor_focused(),
                scope: self.command_scope.clone(),
                focus: self.surface.focused(),
            };
            if router
                .handle(&event, context)
                .map_err(|e| ApplicationError::Host(e.to_string()))?
                != crate::CommandInput::Pass
            {
                self.surface.cancel_capture();
                return Ok(());
            }
        }
        let events = self
            .surface
            .handle(event)
            .map_err(|e| ApplicationError::Host(e.to_string()))?;
        self.app.events(events)
    }
    pub fn dispose(&mut self) {
        self.request_exit();
        #[cfg(feature = "native-clipboard")]
        let _ = self.surface.set_clipboard(None);
        self.app.dispose();
    }
}
