//! Window transport for independently owned GTK roots. GTK/Wayland deliberately
//! does not expose global positioning or stacking; unsupported requests fail
//! before mutating a window instead of acknowledging attributes we cannot apply.
use super::*;
use jadix_core::window::*;

pub trait WindowApplication {
    fn host(&self) -> &GtkHost;
    fn host_mut(&mut self) -> &mut GtkHost;
    /// Poll the root, deliver events and publish reactive changes on the UI thread.
    fn tick(&mut self) -> Result<(), String>;
}

type Factory = Rc<dyn Fn(GtkHost, Option<&str>) -> Result<Box<dyn WindowApplication>, String>>;
struct Child {
    app: Box<dyn WindowApplication>,
    parent: Option<String>,
    attributes: Rc<RefCell<WindowAttributes>>,
    observed: Option<WindowState>,
    normal: Option<WindowFrame>,
}

pub(super) struct Windows {
    requests: Rc<RefCell<WindowRequests>>,
    pending: Rc<RefCell<VecDeque<(u64, WindowRequest)>>>,
    factories: BTreeMap<String, Factory>,
    children: BTreeMap<String, Child>,
    wake: WakeHandle,
}
impl Windows {
    pub fn new(wake: WakeHandle) -> Self {
        let requests = Rc::new(RefCell::new(WindowRequests::default()));
        let pending = Rc::new(RefCell::new(VecDeque::new()));
        let queue = pending.clone();
        let signal = wake.clone();
        let mut next = 0u64;
        requests.borrow_mut().set_request_handler(move |request| {
            let mut queue = queue.borrow_mut();
            if queue.len() >= 256 {
                return Err("GTK window request limit exceeded".into());
            }
            next = next.checked_add(1).ok_or("window request IDs exhausted")?;
            queue.push_back((next, request.clone()));
            signal.wake();
            Ok(next)
        });
        Self {
            requests,
            pending,
            factories: BTreeMap::new(),
            children: BTreeMap::new(),
            wake,
        }
    }
    pub fn register(&mut self, name: String, factory: Factory) -> Result<(), String> {
        WindowContent {
            factory: name.clone(),
            argument: None,
        }
        .validate()?;
        if self.factories.contains_key(&name) {
            return Err("window factory already registered".into());
        }
        self.factories.insert(name, factory);
        Ok(())
    }
    pub fn declarations(&self) -> WindowDeclarations {
        WindowRequests::declarations(&self.requests)
    }
    pub fn request(
        &self,
        request: &WindowRequest,
    ) -> Result<std::sync::mpsc::Receiver<WindowResult>, String> {
        self.requests
            .borrow_mut()
            .request(request)
            .map(|(_, receiver)| receiver)
    }
    pub fn pump(
        &mut self,
        owner: &gtk::Window,
        assets: &std::path::Path,
        fonts: &Rc<fonts::Fonts>,
        components: &components::Components,
        updater: &Option<Rc<updater::Runtime>>,
    ) {
        // Completion can enqueue another request. One finite batch per turn avoids
        // starving input when declarations change in response to acknowledgements.
        let batch: Vec<_> = self.pending.borrow_mut().drain(..).collect();
        for (request, operation) in batch {
            let result = self.apply(operation, owner, assets, fonts, components, updater);
            let _ = self.requests.borrow_mut().complete(request, result);
        }
        let mut closed = Vec::new();
        let mut observations = Vec::new();
        for (id, child) in &mut self.children {
            if !child.app.host().is_alive() {
                closed.push(id.clone());
                continue;
            }
            if let Err(error) = child.app.tick() {
                log::error!("GTK child window {id}: {error}");
                closed.push(id.clone());
                continue;
            }
            let state = state(id, child);
            if child.observed.as_ref() != Some(&state) {
                child.observed = Some(state.clone());
                observations.push((id.clone(), state));
            }
        }
        for id in closed {
            self.close(&id);
        }
        for (id, state) in observations {
            self.requests.borrow_mut().child_state(&id, Some(state));
        }
    }
    fn apply(
        &mut self,
        request: WindowRequest,
        owner: &gtk::Window,
        assets: &std::path::Path,
        fonts: &Rc<fonts::Fonts>,
        components: &components::Components,
        updater: &Option<Rc<updater::Runtime>>,
    ) -> WindowResult {
        match request {
            WindowRequest::Open {
                id,
                kind,
                parent,
                title,
                width,
                height,
                resizable,
                close_on_blur,
                modal,
                initial_mode,
                initially_visible,
                attributes,
                content,
            } => {
                let mut options = DeclaredWindowOptions {
                    kind,
                    width,
                    height,
                    modal: modal.unwrap_or(false),
                    initial_mode: initial_mode.unwrap_or(WindowMode::Normal),
                    initially_visible: initially_visible.unwrap_or(true),
                    content,
                    attributes: WindowAttributes {
                        title: Some(title),
                        resizable: Some(resizable),
                        close_on_blur,
                        ..Default::default()
                    },
                };
                options.attributes.merge(attributes.unwrap_or_default());
                self.open(
                    id, parent, options, owner, assets, fonts, components, updater,
                )
            }
            WindowRequest::OpenDeclared { id, options } => {
                self.open(id, None, options, owner, assets, fonts, components, updater)
            }
            WindowRequest::Update { id, attributes }
            | WindowRequest::UpdateDeclared { id, attributes } => self.update(&id, attributes),
            WindowRequest::SetMode { id, mode } => self.update(
                &id,
                WindowAttributes {
                    mode: Some(mode),
                    ..Default::default()
                },
            ),
            WindowRequest::Restore { id } => self.update(
                &id,
                WindowAttributes {
                    mode: Some(WindowMode::Normal),
                    visible: Some(true),
                    ..Default::default()
                },
            ),
            WindowRequest::SetVisibility { id, visible } => self.update(
                &id,
                WindowAttributes {
                    visible: Some(visible),
                    ..Default::default()
                },
            ),
            WindowRequest::Focus { id } => {
                self.child(&id)?.app.host().present();
                Ok(())
            }
            WindowRequest::Close { id } | WindowRequest::CloseDeclared { id } => {
                if !self.children.contains_key(&id) {
                    return Err("unknown child window".into());
                }
                self.close(&id);
                Ok(())
            }
        }
    }
    fn child(&mut self, id: &str) -> Result<&mut Child, String> {
        self.children
            .get_mut(id)
            .ok_or_else(|| "unknown child window".into())
    }
    fn open(
        &mut self,
        id: String,
        parent: Option<String>,
        options: DeclaredWindowOptions,
        owner: &gtk::Window,
        assets: &std::path::Path,
        fonts: &Rc<fonts::Fonts>,
        components: &components::Components,
        updater: &Option<Rc<updater::Runtime>>,
    ) -> WindowResult {
        options.validate()?;
        supported(&options.attributes)?;
        if options.width > i32::MAX as u32 || options.height > i32::MAX as u32 {
            return Err("GTK window size exceeds i32".into());
        }
        if self.children.contains_key(&id) {
            return Err("child window ID already open".into());
        }
        if self.children.len() >= 64 {
            return Err("GTK child window limit exceeded".into());
        }
        let parent_window = match &parent {
            Some(id) => self
                .children
                .get(id)
                .ok_or("unknown parent window")?
                .app
                .host()
                .window
                .clone(),
            None => owner.clone(),
        };
        let content = options
            .content
            .as_ref()
            .ok_or("child window requires registered content")?;
        let factory = self
            .factories
            .get(&content.factory)
            .cloned()
            .ok_or("unknown window content factory")?;
        let mut host = GtkHost::new(
            options.attributes.title.as_deref().unwrap_or(""),
            [options.width as i32, options.height as i32],
        )?;
        host.set_asset_root(assets)?;
        host.fonts = fonts.clone();
        host.components = components.child();
        host.updater = updater.clone();
        host.owns_updater = false;
        host.window.set_font_map(Some(&fonts.map));
        host.windows.factories = self.factories.clone();
        let signal = self.wake.clone();
        host.on_wake(move || signal.wake());
        host.window.set_application(owner.application().as_ref());
        host.window.set_transient_for(Some(&parent_window));
        host.window.set_destroy_with_parent(true);
        host.window.set_modal(options.modal);
        let attributes = Rc::new(RefCell::new(WindowAttributes {
            mode: Some(options.initial_mode),
            visible: Some(options.initially_visible),
            ..Default::default()
        }));
        attributes.borrow_mut().merge(options.attributes);
        bind_policy(&host, attributes.clone());
        let mut child = Child {
            app: factory(host, content.argument.as_deref())?,
            parent,
            attributes,
            observed: None,
            normal: None,
        };
        apply_attributes(child.app.host(), &child.attributes.borrow());
        child.app.tick()?;
        self.children.insert(id, child);
        Ok(())
    }
    fn update(&mut self, id: &str, attributes: WindowAttributes) -> WindowResult {
        supported(&attributes)?;
        let child = self.child(id)?;
        let mut applied = child.attributes.borrow_mut();
        applied.validate_update(&attributes)?;
        // Only changed attributes are applied: changing the title must not restore
        // a window the user minimized, or make a hidden window visible again.
        apply_attributes(child.app.host(), &attributes);
        applied.merge(attributes);
        Ok(())
    }
    fn close(&mut self, id: &str) {
        let descendants: Vec<_> = self
            .children
            .iter()
            .filter(|(_, child)| child.parent.as_deref() == Some(id))
            .map(|(id, _)| id.clone())
            .collect();
        for child in descendants {
            self.close(&child);
        }
        if let Some(mut child) = self.children.remove(id) {
            child.app.host_mut().dispose();
            drop(child);
            self.requests.borrow_mut().child_destroyed(id);
        }
    }
    pub fn dispose(&mut self) {
        self.requests.borrow_mut().dispose();
        self.pending.borrow_mut().clear();
        for mut child in std::mem::take(&mut self.children).into_values() {
            child.app.host_mut().dispose();
        }
    }
}

fn supported(attributes: &WindowAttributes) -> WindowResult {
    attributes.validate()?;
    if attributes.position.is_some() {
        return Err("GTK4 does not support global window positioning on Wayland".into());
    }
    if attributes.max_content_size.is_some() {
        return Err("GTK4 does not expose a portable maximum window size".into());
    }
    if attributes
        .level
        .is_some_and(|value| value != WindowLevel::Normal)
        || attributes
            .workspaces
            .is_some_and(|value| value != WindowWorkspaces::Current)
        || attributes.participates_in_cycle == Some(false)
    {
        return Err("window stacking and workspace policy require compositor support".into());
    }
    if attributes
        .appearance
        .is_some_and(|value| value != WindowAppearance::Inherit)
    {
        return Err("per-window appearance override unavailable".into());
    }
    if attributes.title_bar == Some(WindowTitleBar::Transparent) {
        return Err("transparent system title bars unavailable".into());
    }
    if attributes
        .screen_orientations
        .is_some_and(|value| value != ScreenOrientations::LandscapeAndPortrait)
    {
        return Err("desktop screen orientation policy unavailable".into());
    }
    for size in [attributes.content_size, attributes.min_content_size]
        .into_iter()
        .flatten()
    {
        if size.width > i32::MAX as u32 || size.height > i32::MAX as u32 {
            return Err("GTK window size exceeds i32".into());
        }
    }
    Ok(())
}
fn apply_attributes(host: &GtkHost, attributes: &WindowAttributes) {
    let window = &host.window;
    if let Some(title) = &attributes.title {
        window.set_title(Some(title));
    }
    if let Some(resizable) = attributes.resizable {
        window.set_resizable(resizable);
    }
    if let Some(bar) = attributes.title_bar {
        window.set_decorated(bar != WindowTitleBar::Hidden);
    }
    if let Some(size) = attributes.content_size {
        window.set_default_size(size.width as i32, size.height as i32);
    }
    if let Some(size) = attributes.min_content_size {
        host.content
            .set_size_request(size.width as i32, size.height as i32);
    }
    if let Some(mode) = attributes.mode {
        match mode {
            WindowMode::Normal => {
                window.unfullscreen();
                window.unmaximize();
                window.unminimize();
            }
            WindowMode::Maximized => {
                window.unfullscreen();
                window.maximize();
            }
            WindowMode::FullScreen => window.fullscreen(),
            WindowMode::Minimized => window.minimize(),
        }
    }
    if let Some(visible) = attributes.visible {
        if visible {
            tray::prepare_activation(window);
            window.present();
        } else {
            window.set_visible(false);
        }
    }
}
fn dismiss(window: &gtk::Window, alive: &Cell<bool>, attributes: &WindowAttributes) {
    if attributes.dismiss_on_close == Some(true) {
        window.set_visible(false);
    } else {
        alive.set(false);
        window.destroy();
    }
}
fn bind_policy(host: &GtkHost, attributes: Rc<RefCell<WindowAttributes>>) {
    // The destroy signal owns root liveness; vetoing a close leaves it mounted.
    let policy = attributes.clone();
    let alive = host.alive.clone();
    host.window.connect_close_request(move |window| {
        let policy = policy.borrow();
        if policy.close_on_press != Some(false) {
            dismiss(window, &alive, &policy);
        }
        glib::Propagation::Stop
    });
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let weak = host.window.downgrade();
    let alive = host.alive.clone();
    let policy = attributes.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape && policy.borrow().close_on_escape == Some(true) {
            if let Some(window) = weak.upgrade() {
                dismiss(&window, &alive, &policy.borrow());
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    host.window.add_controller(keys);
    let alive = host.alive.clone();
    host.window.connect_is_active_notify(move |window| {
        if !window.is_active()
            && window.is_visible()
            && attributes.borrow().close_on_blur == Some(true)
        {
            dismiss(window, &alive, &attributes.borrow());
        }
    });
}
fn state(id: &str, child: &mut Child) -> WindowState {
    let host = child.app.host();
    let window = &host.window;
    let mode = if window.is_fullscreen() {
        WindowMode::FullScreen
    } else if window.is_maximized() {
        WindowMode::Maximized
    } else if window
        .surface()
        .and_downcast::<gdk::Toplevel>()
        .is_some_and(|surface| surface.state().contains(gdk::ToplevelState::MINIMIZED))
    {
        WindowMode::Minimized
    } else {
        WindowMode::Normal
    };
    let [width, height] = host.viewport();
    // GTK4 reports surface-local size; global origin is intentionally unavailable.
    // Position updates are rejected above, so these local coordinates cannot be
    // written back into a controlled global-position attribute.
    let frame = WindowFrame {
        x: 0.,
        y: 0.,
        width: width.into(),
        height: height.into(),
    };
    if mode == WindowMode::Normal {
        child.normal = Some(frame);
    }
    WindowState {
        id: Some(id.into()),
        frame,
        normal_frame: child.normal,
        content_width: width.into(),
        content_height: height.into(),
        backing_scale: window.scale_factor().into(),
        display: None,
        title_bar_insets: Default::default(),
        mode,
        key: window.is_active(),
        visible: window.is_visible(),
        occluded: !window.is_mapped() || mode == WindowMode::Minimized,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_window_requests_fail_before_mutation() {
        assert!(supported(&WindowAttributes {
            position: Some(WindowPosition { x: 12., y: 20. }),
            ..Default::default()
        })
        .is_err());
        assert!(supported(&WindowAttributes {
            content_size: Some(WindowSize {
                width: u32::MAX,
                height: 1
            }),
            ..Default::default()
        })
        .is_err());
        assert!(supported(&WindowAttributes {
            mode: Some(WindowMode::FullScreen),
            visible: Some(false),
            ..Default::default()
        })
        .is_ok());
    }
}
