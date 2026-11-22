//! Independent native roots driven by the shared owner-scoped window service.
use super::*;
use jadix_core::window::*;
use std::rc::Rc;

pub trait WindowApplication {
    fn host(&self) -> &WindowsHost;
    fn host_mut(&mut self) -> &mut WindowsHost;
    fn tick(&mut self) -> Result<(), String>;
}

type Factory = Rc<dyn Fn(WindowsHost, Option<&str>) -> Result<Box<dyn WindowApplication>, String>>;
struct Child {
    app: Box<dyn WindowApplication>,
    parent: Option<String>,
    observed: Option<WindowState>,
}

pub(super) struct Windows {
    requests: Rc<RefCell<WindowRequests>>,
    pending: Rc<RefCell<VecDeque<(u64, WindowRequest)>>>,
    factories: BTreeMap<String, Factory>,
    children: BTreeMap<String, Child>,
}
impl Windows {
    pub fn new(window: HWND) -> Self {
        let requests = Rc::new(RefCell::new(WindowRequests::default()));
        let pending = Rc::new(RefCell::new(VecDeque::new()));
        let queue = pending.clone();
        let window = window as usize;
        let mut next = 0u64;
        requests.borrow_mut().set_request_handler(move |request| {
            let mut queue = queue.borrow_mut();
            if queue.len() >= 256 { return Err("native window request limit exceeded".into()); }
            next = next.checked_add(1).ok_or("window request IDs exhausted")?;
            queue.push_back((next, request.clone()));
            unsafe { PostMessageW(window as HWND, WM_NULL, 0, 0); }
            Ok(next)
        });
        Self { requests, pending, factories: Default::default(), children: Default::default() }
    }
    pub fn register(&mut self, name: String, factory: Factory) -> Result<(), String> {
        WindowContent { factory: name.clone(), argument: None }.validate()?;
        if self.factories.contains_key(&name) { return Err("window factory already registered".into()); }
        self.factories.insert(name, factory); Ok(())
    }
    pub fn declarations(&self) -> WindowDeclarations { WindowRequests::declarations(&self.requests) }
    pub fn request(&self, request: &WindowRequest) -> Result<std::sync::mpsc::Receiver<WindowResult>, String> {
        self.requests.borrow_mut().request(request).map(|(_, receiver)| receiver)
    }
    pub fn pump(&mut self, owner: &WindowsHost) {
        let batch: Vec<_> = self.pending.borrow_mut().drain(..).collect();
        for (id, request) in batch {
            let result = self.apply(request, owner);
            let _ = self.requests.borrow_mut().complete(id, result);
        }
        let mut closed = Vec::new();
        for (id, child) in &mut self.children {
            if child.app.host().is_closed() { closed.push(id.clone()); continue; }
            if let Err(error) = child.app.tick() {
                log::error!("native child window {id}: {error}"); closed.push(id.clone()); continue;
            }
            match child.app.host().window_state(Some(id.clone())) {
                Ok(state) if child.observed.as_ref() != Some(&state) => {
                    child.observed = Some(state.clone());
                    self.requests.borrow_mut().child_state(id, Some(state));
                }
                Err(_) => closed.push(id.clone()),
                _ => {}
            }
        }
        for id in closed { self.close(&id); }
    }
    fn apply(&mut self, request: WindowRequest, owner: &WindowsHost) -> WindowResult {
        match request {
            WindowRequest::Open { id, kind, parent, title, width, height, resizable, close_on_blur, modal, initial_mode, initially_visible, attributes, content } => {
                let mut options = DeclaredWindowOptions { kind, width, height, modal: modal.unwrap_or(false), initial_mode: initial_mode.unwrap_or(WindowMode::Normal), initially_visible: initially_visible.unwrap_or(true), content,
                    attributes: WindowAttributes { title: Some(title), resizable: Some(resizable), close_on_blur, ..Default::default() } };
                options.attributes.merge(attributes.unwrap_or_default());
                self.open(id, parent, options, owner)
            }
            WindowRequest::OpenDeclared { id, options } => self.open(id, None, options, owner),
            WindowRequest::Update { id, attributes } | WindowRequest::UpdateDeclared { id, attributes } => self.update(&id, attributes),
            WindowRequest::SetMode { id, mode } => self.update(&id, WindowAttributes { mode: Some(mode), ..Default::default() }),
            WindowRequest::Restore { id } => self.update(&id, WindowAttributes { mode: Some(WindowMode::Normal), visible: Some(true), ..Default::default() }),
            WindowRequest::SetVisibility { id, visible } => self.update(&id, WindowAttributes { visible: Some(visible), ..Default::default() }),
            WindowRequest::Focus { id } => {
                let host = self.child(&id)?.app.host();
                apply_attributes(host, WindowAttributes { visible: Some(true), ..Default::default() })?;
                unsafe { if IsIconic(host.window) != 0 { ShowWindow(host.window, SW_RESTORE); } SetForegroundWindow(host.window); }
                Ok(())
            }
            WindowRequest::Close { id } | WindowRequest::CloseDeclared { id } => {
                self.child(&id)?; self.close(&id); Ok(())
            }
        }
    }
    fn child(&mut self, id: &str) -> Result<&mut Child, String> { self.children.get_mut(id).ok_or_else(|| "unknown child window".into()) }
    fn open(&mut self, id: String, parent: Option<String>, options: DeclaredWindowOptions, owner: &WindowsHost) -> WindowResult {
        options.validate()?;
        supported(&options.attributes)?;
        if options.width > 16384 || options.height > 16384 { return Err("native window size exceeds 16384".into()); }
        if self.children.contains_key(&id) { return Err("child window ID already open".into()); }
        if self.children.len() >= 64 { return Err("native child window limit exceeded".into()); }
        let parent_window = match &parent {
            Some(id) => self.child(id)?.app.host().window,
            None => owner.window,
        };
        let content = options.content.as_ref().ok_or("child window requires registered content")?;
        let factory = self.factories.get(&content.factory).cloned().ok_or("unknown window content factory")?;
        let mut host = WindowsHost::new(options.attributes.title.as_deref().unwrap_or(""), [options.width, options.height])?;
        if let Some(web) = &owner.web { host.set_asset_root(web.asset_root().lock().map_err(|e| e.to_string())?.clone())?; }
        host.updater = owner.updater.clone(); host.owns_updater = false;
        host.windows.borrow_mut().factories = self.factories.clone();
        unsafe { SetWindowLongPtrW(host.window, GWLP_HWNDPARENT, parent_window as isize); }
        {
            let mut policy = host.context.window_policy.borrow_mut();
            policy.modal_owner = if options.modal { parent_window } else { null_mut() };
        }
        if matches!(options.kind, WindowKind::Panel | WindowKind::Popup | WindowKind::Overlay) {
            unsafe { SetWindowLongPtrW(host.window, GWL_EXSTYLE, GetWindowLongPtrW(host.window, GWL_EXSTYLE) | WS_EX_TOOLWINDOW as isize); }
        }
        let mut attributes = WindowAttributes { mode: Some(options.initial_mode), visible: Some(options.initially_visible), content_size: Some(WindowSize { width: options.width, height: options.height }), ..Default::default() };
        attributes.merge(options.attributes);
        // Mount and publish while hidden, before activation or modal disabling.
        let visible = attributes.visible.take();
        apply_attributes(&host, attributes)?;
        let mut app = factory(host, content.argument.as_deref())?;
        app.tick()?;
        apply_attributes(app.host(), WindowAttributes { visible, ..Default::default() })?;
        self.children.insert(id, Child { app, parent, observed: None }); Ok(())
    }
    fn update(&mut self, id: &str, attributes: WindowAttributes) -> WindowResult { apply_attributes(self.child(id)?.app.host(), attributes) }
    fn close(&mut self, id: &str) {
        let descendants: Vec<_> = self.children.iter().filter(|(_, c)| c.parent.as_deref() == Some(id)).map(|(id, _)| id.clone()).collect();
        for child in descendants { self.close(&child); }
        if let Some(mut child) = self.children.remove(id) {
            child.app.host_mut().dispose(); drop(child);
            self.requests.borrow_mut().child_destroyed(id);
        }
    }
    pub fn dispose(&mut self) {
        self.requests.borrow_mut().dispose(); self.pending.borrow_mut().clear();
        for mut child in std::mem::take(&mut self.children).into_values() { child.app.host_mut().dispose(); }
    }
}

#[derive(Default)]
pub(super) struct Policy {
    attributes: WindowAttributes,
    fullscreen: Option<(isize, WINDOWPLACEMENT)>,
    modal_owner: HWND,
    modal_active: bool,
    pending_mode: Option<WindowMode>,
}
thread_local! { static MODALS: RefCell<BTreeMap<usize, (usize, bool)>> = RefCell::new(BTreeMap::new()); }
impl Policy {
    pub fn release_modal(&mut self) { self.modal(false); }
    fn modal(&mut self, visible: bool) {
        if self.modal_owner.is_null() || self.modal_active == visible { return; }
        self.modal_active = visible;
        MODALS.with(|owners| {
            let mut owners = owners.borrow_mut();
            let owner = self.modal_owner;
            if visible {
                let entry = owners.entry(owner as usize).or_insert_with(|| (0, unsafe { IsWindowEnabled(owner) != 0 }));
                entry.0 += 1; unsafe { EnableWindow(owner, 0); }
            } else if let Some(entry) = owners.get_mut(&(owner as usize)) {
                entry.0 -= 1;
                if entry.0 == 0 {
                    let enabled = entry.1; owners.remove(&(owner as usize));
                    if enabled && unsafe { IsWindow(owner) } != 0 { unsafe { EnableWindow(owner, 1); } }
                }
            }
        });
    }
}

pub(super) fn message(context: &WindowContext, window: HWND, message: u32, parameter: WPARAM, data: LPARAM) -> Option<LRESULT> {
    if window != context.root.get() { return None; }
    // Win32 setters synchronously send messages. Never retain this borrow across them.
    if matches!(message, WM_SHOWWINDOW | WM_WINDOWPOSCHANGED | WM_DESTROY) {
        let visible = match message { WM_SHOWWINDOW => parameter != 0, WM_DESTROY => false, _ => unsafe { IsWindowVisible(window) != 0 } };
        if let Ok(mut policy) = context.window_policy.try_borrow_mut() { policy.modal(visible); }
    }
    if message == WM_CLOSE {
        let policy = context.window_policy.borrow();
        if policy.attributes.close_on_press == Some(false) { return Some(0); }
        if policy.attributes.dismiss_on_close == Some(true) {
            drop(policy); unsafe { ShowWindow(window, SW_HIDE); } return Some(0);
        }
    }
    if message == WM_ACTIVATE && parameter & 0xffff == WA_INACTIVE as usize {
        let close = context.window_policy.borrow().attributes.close_on_blur == Some(true);
        if close && unsafe { IsWindowVisible(window) } != 0 { unsafe { PostMessageW(window, WM_CLOSE, 0, 0); } }
    }
    if message == WM_GETMINMAXINFO && data != 0 {
        let policy = context.window_policy.borrow();
        let scale = context.scale.get();
        let info = unsafe { &mut *(data as *mut MINMAXINFO) };
        for (size, target) in [(policy.attributes.min_content_size, &mut info.ptMinTrackSize), (policy.attributes.max_content_size, &mut info.ptMaxTrackSize)] {
            if let Some(size) = size {
                let mut rect = RECT { left: 0, top: 0, right: (size.width as f64 * scale).round() as i32, bottom: (size.height as f64 * scale).round() as i32 };
                unsafe { AdjustWindowRectExForDpi(&mut rect, GetWindowLongPtrW(window, GWL_STYLE) as u32, 0, GetWindowLongPtrW(window, GWL_EXSTYLE) as u32, GetDpiForWindow(window)); }
                target.x = rect.right - rect.left; target.y = rect.bottom - rect.top;
            }
        }
        return Some(0);
    }
    None
}
pub(super) fn escape(context: &WindowContext) -> bool {
    if context.window_policy.borrow().attributes.close_on_escape != Some(true) || !context.compositions.borrow().is_empty() { return false; }
    unsafe { PostMessageW(context.root.get(), WM_CLOSE, 0, 0); } true
}

fn supported(attributes: &WindowAttributes) -> WindowResult {
    attributes.validate()?;
    if attributes.workspaces == Some(WindowWorkspaces::All) { return Err("Windows does not expose a supported API to pin a window across all virtual desktops".into()); }
    if attributes.level == Some(WindowLevel::DesktopIcon) { return Err("desktop icon window level is unavailable".into()); }
    if attributes.title_bar == Some(WindowTitleBar::Transparent) { return Err("transparent title bar requires a custom nonclient renderer".into()); }
    if attributes.appearance.is_some_and(|a| a != WindowAppearance::Inherit) { return Err("per-window appearance override unavailable".into()); }
    if attributes.screen_orientations.is_some_and(|o| o != ScreenOrientations::LandscapeAndPortrait) { return Err("desktop screen rotation policy unavailable".into()); }
    for size in [attributes.content_size, attributes.min_content_size, attributes.max_content_size].into_iter().flatten() {
        if size.width > 16384 || size.height > 16384 { return Err("native window size exceeds 16384".into()); }
    }
    if attributes.position.is_some_and(|p| p.x.abs() > 1_000_000. || p.y.abs() > 1_000_000.) { return Err("native window position exceeds bounds".into()); }
    Ok(())
}

fn apply_attributes(host: &WindowsHost, attributes: WindowAttributes) -> WindowResult {
    supported(&attributes)?;
    host.context.window_policy.borrow().attributes.validate_update(&attributes)?;
    let _dpi = DpiScope::window(host.window);
    let window = host.window;
    if let Some(title) = &attributes.title { if unsafe { SetWindowTextW(window, wide(title).as_ptr()) } == 0 { return Err(native_error("SetWindowTextW")); } }
    unsafe {
        let mut style = host.context.window_policy.borrow().fullscreen.as_ref().map_or_else(|| GetWindowLongPtrW(window, GWL_STYLE), |saved| saved.0);
        if let Some(resizable) = attributes.resizable {
            if resizable { style |= (WS_THICKFRAME | WS_MAXIMIZEBOX) as isize; } else { style &= !((WS_THICKFRAME | WS_MAXIMIZEBOX) as isize); }
        }
        if let Some(bar) = attributes.title_bar {
            if bar == WindowTitleBar::Hidden { style &= !(WS_CAPTION as isize); } else { style |= WS_CAPTION as isize; }
        }
        if attributes.resizable.is_some() || attributes.title_bar.is_some() {
            let fullscreen = {
                let mut policy = host.context.window_policy.borrow_mut();
                if let Some(saved) = policy.fullscreen.as_mut() { saved.0 = style; true } else { false }
            };
            if !fullscreen { SetWindowLongPtrW(window, GWL_STYLE, style); }
        }
        if let Some(cycle) = attributes.participates_in_cycle {
            let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, if cycle { ex & !(WS_EX_TOOLWINDOW as isize) } else { ex | WS_EX_TOOLWINDOW as isize });
        }
        let mut rect = RECT::default(); GetWindowRect(window, &mut rect);
        let scale = host.context.scale.get();
        let (mut width, mut height) = (rect.right - rect.left, rect.bottom - rect.top);
        if let Some(size) = attributes.content_size {
            let mut content = RECT { left: 0, top: 0, right: (size.width as f64 * scale).round() as i32, bottom: (size.height as f64 * scale).round() as i32 };
            if AdjustWindowRectExForDpi(&mut content, style as u32, 0, GetWindowLongPtrW(window, GWL_EXSTYLE) as u32, GetDpiForWindow(window)) == 0 { return Err(native_error("AdjustWindowRectExForDpi")); }
            width = content.right - content.left; height = content.bottom - content.top;
        }
        let position = attributes.position;
        let z = attributes.level.map(|level| if level == WindowLevel::Normal { HWND_NOTOPMOST } else { HWND_TOPMOST }).unwrap_or(null_mut());
        let flags = SWP_NOACTIVATE | SWP_FRAMECHANGED | if attributes.level.is_none() { SWP_NOZORDER } else { 0 };
        if SetWindowPos(window, z, position.map_or(rect.left, |p| (p.x * scale).round() as i32), position.map_or(rect.top, |p| (p.y * scale).round() as i32), width, height, flags) == 0 { return Err(native_error("SetWindowPos")); }
        if let Some(mode) = attributes.mode { host.context.window_policy.borrow_mut().pending_mode = Some(mode); }
        let becoming_visible = attributes.visible.unwrap_or(IsWindowVisible(window) != 0);
        if becoming_visible {
            let mode = host.context.window_policy.borrow_mut().pending_mode.take();
            if let Some(mode) = mode { set_mode(host, mode)?; }
        }
    }
    let visible = attributes.visible;
    host.context.window_policy.borrow_mut().attributes.merge(attributes);
    if let Some(visible) = visible { unsafe { ShowWindow(window, if visible { SW_SHOW } else { SW_HIDE }); } }
    // ShowWindow may omit WM_SHOWWINDOW (for example while maximizing). The
    // actual visibility after all synchronous setters is authoritative.
    host.context.window_policy.borrow_mut().modal(unsafe { IsWindowVisible(window) != 0 });
    Ok(())
}

unsafe fn set_mode(host: &WindowsHost, mode: WindowMode) -> WindowResult {
    let window = host.window;
    let restore = if mode != WindowMode::FullScreen { host.context.window_policy.borrow_mut().fullscreen.take() } else { None };
    unsafe {
        if let Some((style, placement)) = restore {
            SetWindowLongPtrW(window, GWL_STYLE, style); SetWindowPlacement(window, &placement);
            SetWindowPos(window, null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
        if mode == WindowMode::FullScreen {
            if host.context.window_policy.borrow().fullscreen.is_none() {
                let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
                let mut monitor = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                if GetWindowPlacement(window, &mut placement) == 0 || GetMonitorInfoW(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 { return Err(native_error("fullscreen placement")); }
                let style = GetWindowLongPtrW(window, GWL_STYLE);
                host.context.window_policy.borrow_mut().fullscreen = Some((style, placement));
                SetWindowLongPtrW(window, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW as isize));
                let r = monitor.rcMonitor;
                if SetWindowPos(window, null_mut(), r.left, r.top, r.right-r.left, r.bottom-r.top, SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED) == 0 { return Err(native_error("fullscreen bounds")); }
            }
        } else {
            ShowWindow(window, match mode { WindowMode::Normal => SW_RESTORE, WindowMode::Maximized => SW_MAXIMIZE, WindowMode::Minimized => SW_MINIMIZE, _ => unreachable!() });
        }
    }
    Ok(())
}

pub(super) fn state(host: &WindowsHost, id: Option<String>) -> Result<WindowState, String> {
    if host.is_closed() { return Err("native window is closed".into()); }
    let _dpi = DpiScope::window(host.window);
    let scale = host.context.scale.get();
    let frame = |r: RECT| WindowFrame { x: r.left as f64 / scale, y: r.top as f64 / scale, width: (r.right-r.left) as f64 / scale, height: (r.bottom-r.top) as f64 / scale };
    unsafe {
        let mut rect = RECT::default();
        let mut placement = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        let mut monitor = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetWindowRect(host.window, &mut rect) == 0 || GetWindowPlacement(host.window, &mut placement) == 0 || GetMonitorInfoW(MonitorFromWindow(host.window, MONITOR_DEFAULTTONEAREST), &mut monitor) == 0 { return Err(native_error("window state")); }
        // WINDOWPLACEMENT uses workspace coordinates for ordinary top-level windows.
        let mut normal = host.context.window_policy.borrow().fullscreen.as_ref().map_or(placement.rcNormalPosition, |saved| saved.1.rcNormalPosition);
        if GetWindowLongPtrW(host.window, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize == 0 {
            OffsetRect(&mut normal, monitor.rcWork.left-monitor.rcMonitor.left, monitor.rcWork.top-monitor.rcMonitor.top);
        }
        let mode = if IsIconic(host.window) != 0 { WindowMode::Minimized } else if host.context.window_policy.borrow().fullscreen.is_some() { WindowMode::FullScreen } else if IsZoomed(host.window) != 0 { WindowMode::Maximized } else { WindowMode::Normal };
        let [width, height] = host.viewport(); let visible = IsWindowVisible(host.window) != 0;
        Ok(WindowState { id, frame: frame(rect), normal_frame: Some(frame(normal)), content_width: width as f64, content_height: height as f64, backing_scale: scale,
            display: Some(WindowDisplay { id: None, frame: frame(monitor.rcMonitor), work_area: frame(monitor.rcWork) }), title_bar_insets: Default::default(), mode,
            key: GetForegroundWindow() == host.window, visible, occluded: !visible || mode == WindowMode::Minimized })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct App(WindowsHost);
    impl WindowApplication for App {
        fn host(&self) -> &WindowsHost { &self.0 }
        fn host_mut(&mut self) -> &mut WindowsHost { &mut self.0 }
        fn tick(&mut self) -> Result<(), String> { self.0.pump(); Ok(()) }
    }
    fn options(visible: bool, modal: bool) -> DeclaredWindowOptions {
        DeclaredWindowOptions { initially_visible: visible, modal, content: Some(WindowContent { factory: "test".into(), argument: Some("typed-session".into()) }), ..DeclaredWindowOptions::new(320, 240) }
    }
    fn send(host: &mut WindowsHost, request: WindowRequest) -> WindowResult {
        let reply = host.request_window(&request)?; host.pump(); reply.try_recv().map_err(|e| e.to_string())?
    }
    #[test]
    fn independent_windows_hide_modal_modes_close_and_cleanup() {
        let mut owner = WindowsHost::new("multiwindow owner", [640, 480]).unwrap();
        owner.register_window_factory("test", |host, argument| {
            assert_eq!(argument, Some("typed-session"));
            assert_eq!(unsafe { IsWindowVisible(host.window) }, 0, "factory mounts hidden");
            Ok(Box::new(App(host)))
        }).unwrap();
        let mut initial = options(false, true); initial.initial_mode = WindowMode::Maximized;
        send(&mut owner, WindowRequest::OpenDeclared { id: "modal".into(), options: initial }).unwrap();
        let window = owner.windows.borrow().children["modal"].app.host().window;
        assert_eq!(unsafe { IsWindowVisible(window) }, 0);
        assert_ne!(unsafe { IsWindowEnabled(owner.window) }, 0);
        send(&mut owner, WindowRequest::SetVisibility { id: "modal".into(), visible: true }).unwrap();
        assert_ne!(unsafe { IsZoomed(window) }, 0);
        assert_eq!(unsafe { IsWindowEnabled(owner.window) }, 0);
        send(&mut owner, WindowRequest::OpenDeclared { id: "second".into(), options: options(true, true) }).unwrap();
        send(&mut owner, WindowRequest::SetVisibility { id: "modal".into(), visible: false }).unwrap();
        assert_eq!(unsafe { IsWindowEnabled(owner.window) }, 0, "second modal still owns disable");
        send(&mut owner, WindowRequest::Close { id: "second".into() }).unwrap();
        assert_ne!(unsafe { IsWindowEnabled(owner.window) }, 0);
        send(&mut owner, WindowRequest::Restore { id: "modal".into() }).unwrap();
        assert_eq!(unsafe { IsZoomed(window) }, 0);
        send(&mut owner, WindowRequest::SetMode { id: "modal".into(), mode: WindowMode::FullScreen }).unwrap();
        let state = owner.windows.borrow().children["modal"].app.host().window_state(Some("modal".into())).unwrap();
        assert_eq!(state.mode, WindowMode::FullScreen);
        assert!(state.frame.width >= state.content_width);
        send(&mut owner, WindowRequest::Restore { id: "modal".into() }).unwrap();
        send(&mut owner, WindowRequest::Update { id: "modal".into(), attributes: WindowAttributes { dismiss_on_close: Some(true), close_on_escape: Some(true), ..Default::default() } }).unwrap();
        unsafe { SendMessageW(window, WM_CLOSE, 0, 0); }
        owner.pump();
        assert_eq!(unsafe { IsWindowVisible(window) }, 0);
        assert!(owner.windows.borrow().children.contains_key("modal"));
        assert_ne!(unsafe { IsWindowEnabled(owner.window) }, 0);
        send(&mut owner, WindowRequest::Focus { id: "modal".into() }).unwrap();
        assert_ne!(unsafe { IsWindowVisible(window) }, 0, "focus restores a hidden root");
        assert_eq!(unsafe { IsWindowEnabled(owner.window) }, 0);
        send(&mut owner, WindowRequest::SetVisibility { id: "modal".into(), visible: false }).unwrap();
        let pending = owner.request_window(&WindowRequest::SetVisibility { id: "modal".into(), visible: true }).unwrap();
        owner.dispose();
        assert!(pending.try_recv().unwrap().is_err());
        assert_eq!(unsafe { IsWindow(window) }, 0);
        assert!(owner.request_window(&WindowRequest::Close { id: "modal".into() }).is_err());
    }
    #[test]
    fn failed_factory_and_rejected_attributes_leave_owner_usable() {
        let mut owner = WindowsHost::new("multiwindow failure", [640, 480]).unwrap();
        let created = Rc::new(Cell::new(null_mut())); let capture = created.clone();
        owner.register_window_factory("test", move |host, _| { capture.set(host.window); Err("mount failed".into()) }).unwrap();
        assert!(send(&mut owner, WindowRequest::OpenDeclared { id: "bad".into(), options: options(true, true) }).is_err());
        assert!(!created.get().is_null());
        assert_eq!(unsafe { IsWindow(created.get()) }, 0);
        assert_ne!(unsafe { IsWindowEnabled(owner.window) }, 0);
        assert!(owner.windows.borrow().children.is_empty());
        let mut invalid = options(true, true); invalid.attributes.content_size = Some(WindowSize { width: 20000, height: 20 });
        created.set(null_mut());
        assert!(send(&mut owner, WindowRequest::OpenDeclared { id: "invalid".into(), options: invalid }).is_err());
        assert!(created.get().is_null());
    }
}
