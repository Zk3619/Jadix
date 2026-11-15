//! Mounted CPU portable Element with the existing reactive owner/projection.
use crate::{
    crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    CellSurface,
};
use jadix::portable::{authoring::native::ElementProjection, Element};
use jadix_core::{
    presentation::{EventPayload, PlatformEvent, RootId, RootLifetime},
    reactivity::{create_root, with_owner, OwnerHandle},
};
use jadix_layout::presentation::LayoutStats;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use std::io;

mod checkpoint;

/// Owns a retained portable tree and its reactive scope. No GPU, OS-view host,
/// VM, or alternate signal implementation. External changes call `present` on
/// the UI thread; this type does not add a polling timer or executor.
pub struct CellApplication {
    owner: Option<OwnerHandle>,
    lifetime: RootLifetime,
    element: Element,
    projection: ElementProjection,
    surface: CellSurface,
    viewport: Option<(Rect, u64)>,
    shortcuts: Vec<Shortcut>,
    layout_stats: LayoutStats,
    menus: crate::TerminalMenu,
    commands: Option<crate::CommandRouter>,
    command_scope: Option<String>,
    checkpoint: Option<checkpoint::Recovery>,
    exit_requested: bool,
}
/// A named shared Button command. Names are resolved after each projection, so
/// a hidden page/remount cannot dispatch into a retired presentation identity.
#[derive(Clone, Debug)]
pub struct Shortcut {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub target: String,
    /// Template name containing both focused node and target. None is root scope.
    pub scope: Option<String>,
    /// Explicit opt-in to overriding a focused Input/Textarea's own shortcuts.
    pub in_editors: bool,
}
impl CellApplication {
    pub fn new(cell_size: [f64; 2], build: impl FnOnce() -> Element) -> io::Result<Self> {
        let lifetime = RootLifetime::new(RootId::new());
        let surface = CellSurface::new(lifetime.clone(), cell_size)?;
        let (owner, element) = create_root(build);
        Ok(Self {
            projection: ElementProjection::with_lifetime(lifetime.clone()),
            lifetime,
            owner: Some(owner),
            element,
            surface,
            viewport: None,
            shortcuts: Vec::new(),
            layout_stats: LayoutStats::default(),
            menus: crate::TerminalMenu::default(),
            commands: None,
            command_scope: None,
            checkpoint: None,
            exit_requested: false,
        })
    }
    pub fn surface(&self) -> &CellSurface {
        &self.surface
    }
    /// Use for search/color policy. Tree publication belongs to this application.
    pub fn surface_mut(&mut self) -> &mut CellSurface {
        &mut self.surface
    }
    pub fn element_mut(&mut self) -> &mut Element {
        &mut self.element
    }
    /// Last successful shared layout, including retained engine work counts.
    pub fn layout_stats(&self) -> &LayoutStats {
        &self.layout_stats
    }
    pub fn needs_frame(&self) -> bool {
        self.surface.needs_frame() || self.menus.needs_frame()
    }
    /// Request that an embedding host stop its run loop before the tree is
    /// disposed. Pending command prefixes and pointer captures are cancelled.
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
    /// Whether the host should stop dispatching work for this application.
    pub fn exit_requested(&self) -> bool {
        self.exit_requested || self.lifetime.is_disposed()
    }
    pub fn menus(&self) -> &crate::TerminalMenu {
        &self.menus
    }
    pub fn command_registry(&self) -> jadix_core::commands::CommandRegistry {
        jadix_core::commands::CommandRegistry::new(self.lifetime.clone())
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
    pub fn set_menubar(&mut self, items: Vec<crate::MenuItem>) -> io::Result<()> {
        if self.lifetime.is_disposed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.menus.set_bar(items)?;
        if let Some((area, environment)) = self.viewport {
            self.present(area, environment)?;
        }
        Ok(())
    }
    pub fn set_context_menu(&mut self, items: Vec<crate::MenuItem>) -> io::Result<()> {
        if self.lifetime.is_disposed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.menus.set_context(items)
    }
    /// Atomically replace at most 128 distinct (scope,chord) bindings. Only
    /// modified printable characters or F1..F24 are accepted; navigation and
    /// plain text remain owned by the focused component. Matching is exact.
    pub fn set_shortcuts(&mut self, shortcuts: Vec<Shortcut>) -> io::Result<()> {
        if self.owner.is_none() || self.lifetime.is_disposed() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let mut keys = std::collections::HashSet::new();
        if shortcuts.len() > 128
            || shortcuts.iter().any(|s| {
                let name =
                    |s: &str| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control);
                name(&s.target)
                    || s.scope.as_deref().is_some_and(name)
                    || !(matches!(s.code, KeyCode::F(1..=24))
                        || matches!(s.code,KeyCode::Char(c) if !c.is_control())
                            && s.modifiers.intersects(
                                KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                            ))
                    || !keys.insert((normalized(s.code), s.modifiers, s.scope.as_deref()))
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid or duplicate shortcut binding",
            ));
        }
        self.shortcuts = shortcuts;
        Ok(())
    }
    pub fn present(&mut self, area: Rect, environment_revision: u64) -> io::Result<()> {
        if self.exit_requested() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if self
            .viewport
            .is_some_and(|old| old != (area, environment_revision))
        {
            self.menus.close();
        }
        let content_area = self.menus.content_area(area);
        let owner = self
            .owner
            .as_ref()
            .filter(|_| !self.lifetime.is_disposed())
            .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?
            .id();
        let result = with_owner(owner, || {
            let mut tree = self
                .projection
                .snapshot_with_focus(&mut self.element, self.surface.focused())
                .map_err(|e| io::Error::other(format!("portable projection: {e:?}")))?;
            let cell = self.surface.cell_size();
            tree.descriptor
                .style
                .width
                .get_or_insert(f32::from(content_area.width) * cell[0] as f32);
            tree.descriptor
                .style
                .height
                .get_or_insert(f32::from(content_area.height) * cell[1] as f32);
            self.layout_stats = self.surface.present_auto(
                self.surface.revision(),
                vec![tree],
                content_area,
                environment_revision,
            )?;
            if self.surface.has_modal() || !self.surface.input_active() {
                self.menus.close();
            }
            self.surface
                .bind_navigation(&self.projection.navigation_nodes(&self.element))?;
            if let Some(target) = self.projection.take_focus_request(&self.element) {
                self.surface.focus_node(target)?;
            }
            Ok(())
        });
        if result.is_err() {
            // A projection may refresh bindings before rejecting a tree. Close
            // the owner rather than dispatching against two divergent trees.
            self.dispose();
        } else {
            self.viewport = Some((area, environment_revision));
        }
        result
    }
    /// Routes input through the surface, the shared callbacks, then refreshes
    /// controlled values/focus. Returned Focus/Scroll are host observations.
    pub fn handle(&mut self, event: Event) -> io::Result<Option<PlatformEvent>> {
        if self.exit_requested() {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let result = self.handle_inner(event)?;
        self.save_checkpoint()?;
        Ok(result)
    }
    fn handle_inner(&mut self, event: Event) -> io::Result<Option<PlatformEvent>> {
        match self.menus.handle(
            &event,
            !self.surface.has_modal() && self.surface.input_active(),
            self.surface.editor_focused(),
        ) {
            crate::MenuInput::Handled => {
                self.surface.cancel_capture();
                return Ok(None);
            }
            crate::MenuInput::Command { run, .. } => {
                self.surface.cancel_capture();
                let owner = self.owner.as_ref().ok_or(io::ErrorKind::BrokenPipe)?.id();
                with_owner(owner, || run());
                let (area, environment) = self
                    .viewport
                    .ok_or_else(|| io::Error::other("present before menu input"))?;
                self.present(area, environment)?;
                return Ok(None);
            }
            crate::MenuInput::Pass => {}
        }
        if let Some(router) = &mut self.commands {
            let context = crate::CommandContext {
                allowed: !self.menus.is_open()
                    && !self.surface.has_modal()
                    && self.surface.input_active(),
                in_editor: self.surface.editor_focused(),
                scope: self.command_scope.clone(),
                focus: self.surface.focused(),
            };
            let owner = self.owner.as_ref().ok_or(io::ErrorKind::BrokenPipe)?.id();
            let outcome = with_owner(owner, || router.handle(&event, context))?;
            if outcome != crate::CommandInput::Pass {
                self.surface.cancel_capture();
                if matches!(outcome, crate::CommandInput::Invoked { .. }) {
                    if let Some((area, environment)) = self.viewport {
                        self.present(area, environment)?;
                    }
                }
                return Ok(None);
            }
        }
        if let Event::Key(key) = event {
            let mut candidate = None;
            for shortcut in &self.shortcuts {
                if normalized(key.code) != normalized(shortcut.code)
                    || key.modifiers != shortcut.modifiers
                {
                    continue;
                }
                let Some(target) = self
                    .projection
                    .node_by_template(&self.element, &shortcut.target)
                else {
                    continue;
                };
                let scope = match &shortcut.scope {
                    Some(name) => {
                        let Some(id) = self.projection.node_by_template(&self.element, name) else {
                            continue;
                        };
                        Some(id)
                    }
                    None => None,
                };
                if let Some(rank) = self
                    .surface
                    .shortcut_rank(target, scope, shortcut.in_editors)
                {
                    if candidate.is_none_or(|(_, old)| rank < old) {
                        candidate = Some((target, rank));
                    }
                }
            }
            if let Some((node, _)) = candidate {
                if key.kind != KeyEventKind::Press {
                    return Ok(None);
                }
                let event = PlatformEvent {
                    node,
                    payload: EventPayload::Activate,
                };
                self.surface.cancel_capture();
                self.dispatch(&event)?;
                return Ok(Some(event));
            }
        }
        let event = self.surface.handle(event)?;
        if let Some(event) = &event {
            self.dispatch(event)?;
        }
        Ok(event)
    }
    pub fn dispatch(&mut self, event: &PlatformEvent) -> io::Result<()> {
        let owner = self
            .owner
            .as_ref()
            .filter(|_| !self.lifetime.is_disposed())
            .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?
            .id();
        if !self.surface.event_target(event.node) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inactive application event target",
            ));
        }
        match event.payload {
            EventPayload::Focus => return self.surface.focus_node(event.node),
            EventPayload::Scroll { x, y } => {
                self.surface.scroll_to(event.node, [x, y])?;
                return Ok(());
            }
            _ => {}
        }
        with_owner(owner, || self.projection.dispatch(&mut self.element, event))
            .map_err(|e| io::Error::other(format!("portable event: {e:?}")))?;
        let (area, environment) = self
            .viewport
            .ok_or_else(|| io::Error::other("present before input"))?;
        self.present(area, environment)
    }
    pub fn dispose(&mut self) {
        self.request_exit();
        #[cfg(feature = "native-clipboard")]
        let _ = self.surface.set_clipboard(None);
        self.lifetime.dispose();
        self.viewport = None;
        self.owner.take();
        self.shortcuts.clear();
        self.menus = crate::TerminalMenu::default();
        self.checkpoint = None;
    }
}
fn normalized(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(c) if c.is_ascii() => KeyCode::Char(c.to_ascii_lowercase()),
        _ => code,
    }
}
impl Drop for CellApplication {
    fn drop(&mut self) {
        self.dispose();
    }
}
impl Widget for &mut CellApplication {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        (&mut self.surface).render(self.menus.content_area(area), buffer);
        if !self.lifetime.is_disposed() {
            self.menus.set_colors(self.surface.colors());
            self.menus.render(area, buffer);
        }
    }
}
