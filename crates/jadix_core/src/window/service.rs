//! Owner-scoped desktop windows, independent of the platform transport.
use super::*;
use crate::reactivity::{
    Context, Signal, create_context, create_effect, current_owner, on_cleanup, provide_context,
    use_context,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::mpsc::{self, Receiver, Sender},
};

pub type WindowResult = Result<(), String>;

pub trait WindowHost {
    fn window_declarations(&self) -> WindowDeclarations;
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WindowRequest {
    Open {
        id: String,
        kind: WindowKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        title: String,
        width: u32,
        height: u32,
        resizable: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        close_on_blur: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        modal: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        initial_mode: Option<WindowMode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        initially_visible: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attributes: Option<WindowAttributes>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<WindowContent>,
    },
    Update {
        id: String,
        attributes: WindowAttributes,
    },
    SetMode {
        id: String,
        mode: WindowMode,
    },
    Restore {
        id: String,
    },
    Focus {
        id: String,
    },
    SetVisibility {
        id: String,
        visible: bool,
    },
    Close {
        id: String,
    },
    OpenDeclared {
        id: String,
        #[serde(flatten)]
        options: DeclaredWindowOptions,
    },
    UpdateDeclared {
        id: String,
        attributes: WindowAttributes,
    },
    CloseDeclared {
        id: String,
    },
}

enum DeclarationChange {
    Open(WindowAttributes),
    Update(WindowAttributes),
    Close,
}

/// Drives one stable child window ID from a declaration on its owning root.
pub struct DeclaredWindow {
    id: String,
    presence: Rc<Cell<WindowPresence>>,
    applied: WindowAttributes,
    pending: Option<(u64, DeclarationChange, Receiver<WindowResult>)>,
}

impl DeclaredWindow {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            presence: Rc::new(Cell::new(WindowPresence::Absent)),
            applied: WindowAttributes::default(),
            pending: None,
        }
    }

    pub fn presence(&self) -> WindowPresence {
        self.presence.get()
    }

    /// Sync the latest declaration on the owning root's UI thread. Pass None
    /// to remove it, and drive again after the host replies when this returns
    /// false. Only attributes follow updates after creation. A window closed
    /// by the host can be opened again once the declaration has been removed.
    pub fn sync(
        &mut self,
        host: &mut impl WindowHost,
        desired: Option<&DeclaredWindowOptions>,
    ) -> Result<bool, String> {
        let declarations = host.window_declarations();
        let windows = declarations
            .windows
            .upgrade()
            .ok_or("window root was disposed")?;
        let result = self.sync_requests(&mut windows.borrow_mut(), desired);
        result
    }

    fn sync_requests(
        &mut self,
        windows: &mut WindowRequests,
        desired: Option<&DeclaredWindowOptions>,
    ) -> Result<bool, String> {
        if let Some((request_id, change, receiver)) = self.pending.take() {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    self.pending = Some((request_id, change, receiver));
                    return Ok(false);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err("window declaration disconnected".into())
                }
            };
            let closed_by_host = self.presence() == WindowPresence::ClosedByHost;
            match change {
                DeclarationChange::Open(attributes) => {
                    self.presence
                        .set(self.presence().finish_open(result.is_ok()));
                    if self.presence() == WindowPresence::Open {
                        self.applied = attributes;
                    } else if self.presence() == WindowPresence::Absent {
                        windows.untrack(&self.id, &self.presence);
                    }
                }
                DeclarationChange::Update(attributes) => {
                    if result.is_ok() && self.presence() == WindowPresence::Open {
                        self.applied.merge(attributes);
                    }
                }
                DeclarationChange::Close => {
                    if result.is_ok() || closed_by_host {
                        self.presence.set(WindowPresence::Absent);
                        windows.untrack(&self.id, &self.presence);
                    }
                }
            }
            if !closed_by_host {
                result?;
            }
        }

        let id = self.id.clone();
        let (request, change) = match (self.presence(), desired) {
            (WindowPresence::Absent, Some(options)) => {
                options.validate()?;
                windows.track(&id, &self.presence)?;
                self.presence.set(WindowPresence::Opening);
                (
                    WindowRequest::OpenDeclared {
                        id,
                        options: options.clone(),
                    },
                    DeclarationChange::Open(options.attributes.clone()),
                )
            }
            (WindowPresence::Open, Some(options)) => {
                self.applied.validate_update(&options.attributes)?;
                let Some(attributes) = self.applied.diff(&options.attributes) else {
                    return Ok(true);
                };
                (
                    WindowRequest::UpdateDeclared {
                        id,
                        attributes: attributes.clone(),
                    },
                    DeclarationChange::Update(attributes),
                )
            }
            (WindowPresence::Open, None) => (
                WindowRequest::CloseDeclared { id },
                DeclarationChange::Close,
            ),
            (WindowPresence::ClosedByHost, None) => {
                self.presence.set(WindowPresence::Absent);
                windows.untrack(&self.id, &self.presence);
                return Ok(true);
            }
            _ => return Ok(true),
        };
        match windows.request(&request) {
            Ok((request_id, receiver)) => {
                self.pending = Some((request_id, change, receiver));
                Ok(false)
            }
            Err(error) => {
                if matches!(change, DeclarationChange::Open(_)) {
                    self.presence.set(WindowPresence::Absent);
                    windows.untrack(&self.id, &self.presence);
                }
                Err(error)
            }
        }
    }
}

/// Access to the window declarations of one root, on its UI thread.
#[derive(Clone)]
pub struct WindowDeclarations {
    windows: Weak<RefCell<WindowRequests>>,
}

fn declarations_context() -> Context<WindowDeclarations> {
    thread_local! { static CONTEXT: Context<WindowDeclarations> = create_context(); }
    CONTEXT.with(|context| *context)
}

/// Install the root's window service before constructing its components.
pub fn provide_window_declarations(windows: WindowDeclarations) {
    provide_context(declarations_context(), windows);
}

pub fn use_window_declarations() -> Option<WindowDeclarations> {
    use_context(declarations_context())
}

/// Optional bidirectional bindings. Only explicitly controlled attributes are
/// written back; observations never replace unrelated application preferences.
#[derive(Clone, Copy, Default)]
pub struct WindowBindings {
    pub open: Option<Signal<bool>>,
    pub attributes: Option<Signal<WindowAttributes>>,
    pub state: Option<Signal<Option<WindowState>>>,
    pub error: Option<Signal<Option<String>>>,
}

impl WindowDeclarations {
    /// Imperative declaration for hosts without a Rust reactive owner (for
    /// example a budgeted script runtime). Dropping it removes the window.
    pub fn create(&self, id: impl Into<String>) -> Result<WindowDeclaration, String> {
        let id = id.into();
        if id.is_empty() {
            return Err("window declaration requires an ID".into());
        }
        let windows = self.windows.upgrade().ok_or("root was disposed")?;
        let (errors, receiver) = mpsc::channel();
        let states = Rc::new(RefCell::new(None));
        let mut requests = windows.borrow_mut();
        requests.mount(&id, errors, WindowBindings::default())?;
        requests.scoped.get_mut(&id).unwrap().observations = Some(states.clone());
        Ok(WindowDeclaration {
            id,
            service: self.clone(),
            errors: receiver,
            states,
        })
    }

    /// Bind a stable window ID to the current reactive owner. Reading signals in
    /// `options` drives attribute updates. Returning `None` removes the window,
    /// as does owner cleanup. The receiver reports failed native requests.
    pub fn mount(
        &self,
        id: impl Into<String>,
        options: impl FnMut() -> Option<DeclaredWindowOptions> + 'static,
    ) -> Result<Receiver<String>, String> {
        self.mount_with_state(id, options, None)
    }

    /// Like `mount`, with observations from the child window on this owner.
    /// The host reports child state through the owner root's window event.
    pub fn mount_with_state(
        &self,
        id: impl Into<String>,
        options: impl FnMut() -> Option<DeclaredWindowOptions> + 'static,
        state: impl Into<Option<Signal<Option<WindowState>>>>,
    ) -> Result<Receiver<String>, String> {
        self.mount_bound(
            id,
            options,
            WindowBindings {
                state: state.into(),
                ..Default::default()
            },
        )
    }

    pub fn mount_bound(
        &self,
        id: impl Into<String>,
        mut options: impl FnMut() -> Option<DeclaredWindowOptions> + 'static,
        bindings: WindowBindings,
    ) -> Result<Receiver<String>, String> {
        if current_owner().is_none() {
            return Err("window declarations require a reactive owner".into());
        }
        let id = id.into();
        if id.is_empty() {
            return Err("window declaration requires an ID".into());
        }
        let windows = self.windows.upgrade().ok_or("root was disposed")?;
        let (errors, receiver) = mpsc::channel();
        let state = bindings.state;
        windows.borrow_mut().mount(&id, errors, bindings)?;
        if let Some(state) = state {
            state.try_set(None);
        }
        if let Some(error) = bindings.error {
            error.try_set(None);
        }

        let handle = self.clone();
        let update_id = id.clone();
        create_effect(move || {
            let mut desired = options();
            if bindings.open.is_some_and(|open| !open.get()) {
                desired = None;
            }
            if let (Some(desired), Some(attributes)) = (&mut desired, bindings.attributes) {
                desired.attributes.merge(attributes.get());
            }
            if let Some(windows) = handle.windows.upgrade() {
                crate::reactivity::batch(|| {
                    windows.borrow_mut().set_desired(&update_id, desired, false)
                });
            }
        });
        let handle = self.clone();
        on_cleanup(move || {
            if let Some(windows) = handle.windows.upgrade() {
                crate::reactivity::batch(|| windows.borrow_mut().set_desired(&id, None, true));
            }
        });
        Ok(receiver)
    }
}

pub struct WindowDeclaration {
    id: String,
    service: WindowDeclarations,
    errors: Receiver<String>,
    states: Rc<RefCell<Option<Option<WindowState>>>>,
}

impl WindowDeclaration {
    pub fn update(&self, options: Option<DeclaredWindowOptions>) -> Result<(), String> {
        if let Some(options) = &options {
            options.validate()?;
        }
        let windows = self.service.windows.upgrade().ok_or("root was disposed")?;
        crate::reactivity::batch(|| windows.borrow_mut().set_desired(&self.id, options, false));
        Ok(())
    }
    pub fn try_error(&self) -> Option<String> {
        self.errors.try_recv().ok()
    }
    pub fn try_state(&self) -> Option<Option<WindowState>> {
        self.states.borrow_mut().take()
    }
}

impl Drop for WindowDeclaration {
    fn drop(&mut self) {
        if let Some(windows) = self.service.windows.upgrade() {
            crate::reactivity::batch(|| windows.borrow_mut().set_desired(&self.id, None, true));
        }
    }
}

struct ScopedDeclaration {
    window: DeclaredWindow,
    desired: Option<DeclaredWindowOptions>,
    mounted: bool,
    removing: bool,
    desired_changed: bool,
    errors: Sender<String>,
    state: Option<Signal<Option<WindowState>>>,
    bindings: WindowBindings,
    observed: Option<WindowState>,
    observations: Option<Rc<RefCell<Option<Option<WindowState>>>>>,
}

impl ScopedDeclaration {
    fn report_error(&self, error: String) {
        if let Some(signal) = self.bindings.error {
            signal.try_set(Some(error.clone()));
        }
        log::error!("window {}: {error}", self.window.id);
        let _ = self.errors.send(error);
    }

    fn observe(&mut self, state: Option<WindowState>) {
        if self.observed != state {
            if let Some(signal) = self.state {
                if !signal.try_set(state.clone()) {
                    self.state = None;
                }
            }
            self.observed = state;
            if let Some(latest) = &self.observations {
                *latest.borrow_mut() = Some(self.observed.clone());
            }
        }
    }

    fn synchronize_observation(&mut self) {
        // Native animations can publish an intermediate frame before an update
        // is acknowledged. Keep the latest user intent until that request ends.
        if self.window.pending.is_some() {
            return;
        }
        let (Some(state), Some(signal)) = (&self.observed, self.bindings.attributes) else {
            return;
        };
        let Some(mut attributes) = crate::reactivity::untrack(|| signal.try_get()) else {
            return;
        };
        let before = attributes.clone();
        if attributes.mode.is_some() {
            attributes.mode = Some(state.mode);
        }
        if attributes.visible.is_some() {
            attributes.visible = Some(state.visible);
        }
        if attributes.position.is_some() {
            attributes.position = Some(WindowPosition {
                x: state.frame.x,
                y: state.frame.y,
            });
        }
        if attributes.content_size.is_some() && state.mode == WindowMode::Normal {
            attributes.content_size = Some(WindowSize {
                width: state.content_width.round().max(1.0) as u32,
                height: state.content_height.round().max(1.0) as u32,
            });
        }
        if attributes != before {
            self.window.applied.merge(attributes.clone());
            if let Some(desired) = &mut self.desired {
                desired.attributes.merge(attributes.clone());
            }
            signal.try_set(attributes);
        }
    }

    fn sync(&mut self, windows: &mut WindowRequests) -> Result<(), String> {
        if self.removing {
            if !self.window.sync_requests(windows, None)? {
                return Ok(());
            }
            self.removing = false;
        }
        self.window.sync_requests(windows, self.desired.as_ref())?;
        Ok(())
    }
}

pub struct WindowRequests {
    callback: Option<Box<dyn FnMut(&WindowRequest) -> Result<u64, String>>>,
    pending: BTreeMap<u64, Sender<WindowResult>>,
    presence: BTreeMap<String, Weak<Cell<WindowPresence>>>,
    scoped: BTreeMap<String, ScopedDeclaration>,
}

impl Default for WindowRequests {
    fn default() -> Self {
        Self {
            callback: None,
            pending: BTreeMap::new(),
            presence: BTreeMap::new(),
            scoped: BTreeMap::new(),
        }
    }
}

impl WindowRequests {
    pub fn set_request_handler(
        &mut self,
        handler: impl FnMut(&WindowRequest) -> Result<u64, String> + 'static,
    ) {
        self.callback = Some(Box::new(handler));
    }

    pub fn declarations(windows: &Rc<RefCell<Self>>) -> WindowDeclarations {
        WindowDeclarations {
            windows: Rc::downgrade(windows),
        }
    }
    fn mount(
        &mut self,
        id: &str,
        errors: Sender<String>,
        bindings: WindowBindings,
    ) -> Result<(), String> {
        if self.callback.is_none() {
            return Err("window requests are unavailable".into());
        }
        if let Some(entry) = self.scoped.get_mut(id) {
            if entry.mounted {
                return Err(format!("window {id} already has a declaration"));
            }
            entry.mounted = true;
            entry.errors = errors;
            entry.state = bindings.state;
            entry.bindings = bindings;
            entry.observed = None;
            return Ok(());
        }
        if self.presence.get(id).and_then(Weak::upgrade).is_some() {
            return Err(format!("window {id} already has a declaration"));
        }
        self.scoped.insert(
            id.into(),
            ScopedDeclaration {
                window: DeclaredWindow::new(id),
                desired: None,
                mounted: true,
                removing: false,
                desired_changed: false,
                errors,
                state: bindings.state,
                bindings,
                observed: None,
                observations: None,
            },
        );
        Ok(())
    }

    fn set_desired(&mut self, id: &str, desired: Option<DeclaredWindowOptions>, unmount: bool) {
        if let Some(entry) = self.scoped.get_mut(id) {
            entry.mounted = !unmount;
            entry.desired_changed |= entry.desired != desired;
            entry.removing |= desired.is_none() && entry.desired.is_some();
            entry.desired = desired;
            self.drive(id);
        }
    }

    fn drive(&mut self, id: &str) {
        let Some(mut entry) = self.scoped.remove(id) else {
            return;
        };
        let previous_request = entry.window.pending.as_ref().map(|pending| pending.0);
        if let Err(error) = entry.sync(self) {
            entry.report_error(error);
            if previous_request.is_some() && entry.desired_changed {
                if let Err(error) = entry.sync(self) {
                    entry.report_error(error);
                }
            }
        }
        let next_request = entry.window.pending.as_ref().map(|pending| pending.0);
        if next_request != previous_request || next_request.is_none() {
            entry.desired_changed = false;
        }
        if entry.mounted
            || next_request.is_some()
            || entry.window.presence() == WindowPresence::Open
        {
            self.scoped.insert(id.into(), entry);
        }
    }

    fn track(&mut self, id: &str, presence: &Rc<Cell<WindowPresence>>) -> Result<(), String> {
        if self.scoped.contains_key(id) || self.presence.get(id).and_then(Weak::upgrade).is_some() {
            return Err(format!("window {id} already has a declaration"));
        }
        self.presence.insert(id.into(), Rc::downgrade(presence));
        Ok(())
    }

    fn untrack(&mut self, id: &str, presence: &Rc<Cell<WindowPresence>>) {
        if self
            .presence
            .get(id)
            .is_some_and(|entry| entry.as_ptr() == Rc::as_ptr(presence))
        {
            self.presence.remove(id);
        }
    }

    pub fn request(
        &mut self,
        request: &WindowRequest,
    ) -> Result<(u64, Receiver<WindowResult>), String> {
        match request {
            WindowRequest::Open {
                id,
                width,
                height,
                attributes,
                content,
                ..
            } => {
                if id.is_empty() || *width == 0 || *height == 0 {
                    return Err("window ID and positive dimensions are required".into());
                }
                if let Some(content) = content {
                    content.validate()?;
                }
                if let Some(attributes) = attributes {
                    attributes.validate()?;
                }
            }
            WindowRequest::OpenDeclared { id, options } => {
                if id.is_empty() {
                    return Err("window ID is required".into());
                }
                options.validate()?;
            }
            WindowRequest::Update { attributes, .. }
            | WindowRequest::UpdateDeclared { attributes, .. } => attributes.validate()?,
            _ => {}
        }
        let callback = self
            .callback
            .as_mut()
            .ok_or("window requests are unavailable")?;
        let id = callback(request)?;
        if id == 0 || self.pending.contains_key(&id) {
            return Err("window host returned an invalid or duplicate request ID".into());
        }
        let (sender, receiver) = mpsc::channel();
        self.pending.insert(id, sender);
        Ok((id, receiver))
    }

    pub fn complete(&mut self, id: u64, result: WindowResult) -> Result<(), String> {
        let sender = self.pending.remove(&id).ok_or("unknown window request")?;
        let _ = sender.send(result);
        let declaration = self.scoped.iter().find_map(|(key, entry)| {
            (entry.window.pending.as_ref().map(|pending| pending.0) == Some(id))
                .then(|| key.clone())
        });
        if let Some(declaration) = declaration {
            self.drive(&declaration);
        }
        Ok(())
    }

    pub fn child_destroyed(&mut self, id: &str) {
        if let Some(entry) = self.scoped.get_mut(id) {
            entry.observe(None);
            if let Some(latest) = &entry.observations {
                *latest.borrow_mut() = Some(None);
            }
            if let Some(open) = entry.bindings.open {
                open.try_set(false);
            }
        }
        if let Some(presence) = self.presence.get(id).and_then(Weak::upgrade) {
            presence.set(WindowPresence::ClosedByHost);
            self.drive(id);
        }
    }

    pub fn child_state(&mut self, id: &str, state: Option<WindowState>) {
        if state.as_ref().is_some_and(|state| {
            state.validate().is_err() || state.id.as_deref().is_some_and(|state_id| state_id != id)
        }) {
            return;
        }
        if let Some(entry) = self.scoped.get_mut(id) {
            if entry.mounted
                && !entry.removing
                && matches!(
                    entry.window.presence(),
                    WindowPresence::Opening | WindowPresence::Open
                )
            {
                entry.observe(state);
                entry.synchronize_observation();
            }
        }
    }

    pub fn dispose(&mut self) {
        self.callback = None;
        for (_, sender) in std::mem::take(&mut self.pending) {
            let _ = sender.send(Err("root was disposed".into()));
        }
        for entry in std::mem::take(&mut self.presence).into_values() {
            if let Some(presence) = entry.upgrade() {
                presence.set(WindowPresence::ClosedByHost);
            }
        }
        for mut entry in std::mem::take(&mut self.scoped).into_values() {
            entry.observe(None);
        }
    }
}
