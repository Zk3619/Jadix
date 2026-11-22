//! GTK 4.14 native presentation host. Layout and event validation remain in
//! `NativeRoot`; GTK owns IME, clipboard, editing history, AT-SPI and rendering.
//! All objects are confined to the GTK main thread. No display is opened by
//! importing this module or running its policy tests.
mod accessibility;
mod audio;
mod components;
mod controls;
mod environment;
mod fonts;
mod hosted;
mod interaction;
mod io;
mod keyboard;
mod media;
mod rich_editor;
mod services;
mod shader;
mod shortcuts;
mod style;
mod terminal;
mod tray;
mod updater;
mod visual;
mod wake;
mod web;
mod windows;
pub use components::{ComponentContext, NativeComponent};
pub use updater::wait_for_update_parent;
pub use wake::WakeHandle;
pub use windows::WindowApplication;

use crate::{
    host::PlatformHost,
    layout::{LayoutError, MeasureRequest},
    presentation::*,
    HeadlessRoot,
};
use gtk4::{self as gtk, gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

pub use gtk4;

struct Events {
    pending: RefCell<VecDeque<PlatformEvent>>,
    publishing: Cell<bool>,
    disposed: Cell<bool>,
    overflow: Cell<bool>,
    wake: WakeHandle,
    touch_ids: RefCell<BTreeMap<gdk::EventSequence, u32>>,
    touch_serial: Cell<u32>,
}
impl Events {
    fn push(&self, node: &Control, payload: EventPayload) {
        if self.publishing.get() {
            return;
        }
        self.enqueue(node, payload);
    }
    fn enqueue(&self, node: &Control, payload: EventPayload) {
        if self.disposed.get() || !node.alive.get() {
            return;
        }
        let mut pending = self.pending.borrow_mut();
        // Geometry and pointer samples can replace only their immediately preceding
        // sample. Edits and activation ordering are never silently dropped.
        if let Some(last) = pending.back_mut() {
            if last.node == node.id
                && matches!(
                    (&last.payload, &payload),
                    (
                        EventPayload::ScrollViewport { .. },
                        EventPayload::ScrollViewport { .. }
                    ) | (
                        EventPayload::PointerMove { .. },
                        EventPayload::PointerMove { .. }
                    )
                )
            {
                last.payload = payload;
                return;
            }
        }
        if pending.len() >= 4096 {
            self.overflow.set(true);
            return;
        }
        pending.push_back(PlatformEvent {
            node: node.id,
            payload,
        });
        self.wake.wake();
    }
}

struct Control {
    id: NodeId,
    descriptor: RefCell<NodeDescriptor>,
    revision: Cell<u64>,
    alive: Cell<bool>,
    intrinsic: Cell<Option<[f64; 2]>>,
}
impl Control {
    fn next_revision(&self) -> Option<u64> {
        let next = self.revision.get().checked_add(1)?;
        self.revision.set(next);
        Some(next)
    }
}
struct Item {
    surface: visual::Surface,
    widget: gtk::Widget,
    children: Option<gtk::Fixed>,
    panel: Option<gtk::Window>,
    popover: Option<gtk::Popover>,
    fallback_label: Option<gtk::Label>,
    control: Rc<Control>,
    css: gtk::CssProvider,
    parent: Option<NodeId>,
    image_source: RefCell<Option<String>>,
    hosted: Option<hosted::State>,
}

/// One independent native root, embeddable through `content()` or shown in its
/// own window. Create on the application's GTK thread, after `gtk4::init()`.
pub struct GtkHost {
    window: gtk::Window,
    content: gtk::Fixed,
    items: BTreeMap<NodeId, Item>,
    model: Option<HeadlessRoot>,
    events: Rc<Events>,
    alive: Rc<Cell<bool>>,
    environment: RefCell<crate::environment::EnvironmentSnapshot>,
    environment_watch: Option<crate::environment::SystemEnvironmentWatcher>,
    services: jadix_core::desktop_services::DesktopServices,
    fetch: jadix_net::native_fetch::NativeFetch,
    websocket: jadix_net::native_websocket::NativeWebSocket,
    images: Option<crate::images::ImageLoader>,
    image_disk_cache: Option<crate::images::ImageDiskCache>,
    asset_root: std::path::PathBuf,
    windows: windows::Windows,
    wake: wake::Wake,
    web: Rc<web::Runtime>,
    audio: audio::Runtime,
    system_services: Rc<services::Services>,
    io: Rc<io::Io>,
    tray: Rc<tray::StatusItem>,
    fonts: Rc<fonts::Fonts>,
    components: components::Components,
    settings_signal: Option<(gtk::Settings, glib::SignalHandlerId)>,
    preferences: Rc<environment::Preferences>,
    updater: Option<Rc<updater::Runtime>>,
    owns_updater: bool,
}

impl GtkHost {
    pub fn new(title: &str, size: [i32; 2]) -> Result<Self, String> {
        wait_for_update_parent()?;
        gtk::init().map_err(|e| format!("GTK display initialization failed: {e}"))?;
        if size.iter().any(|v| *v <= 0) {
            return Err("window size must be positive".into());
        }
        let wake = wake::Wake::new()?;
        let content = wake::Content::new(wake.handle.clone());
        let window = gtk::Window::builder()
            .title(title)
            .default_width(size[0])
            .default_height(size[1])
            .child(&content)
            .build();
        let fonts = fonts::Fonts::new()?;
        window.set_font_map(Some(&fonts.map));
        let alive = Rc::new(Cell::new(true));
        let closed = alive.clone();
        window.connect_destroy(move |_| {
            closed.set(false);
        });
        let close_wake = wake.handle.clone();
        window.connect_destroy(move |_| close_wake.wake());
        let environment_wake = wake.handle.clone();
        window.connect_is_active_notify(move |_| environment_wake.wake());
        let visibility_wake = wake.handle.clone();
        window.connect_visible_notify(move |_| visibility_wake.wake());
        let surface_wake = wake.handle.clone();
        window.connect_realize(move |window| {
            if let Some(surface) = window.surface().and_downcast::<gdk::Toplevel>() {
                let wake = surface_wake.clone();
                surface.connect_state_notify(move |_| wake.wake());
            }
        });
        let preferences = environment::Preferences::new(wake.handle.clone());
        let settings_signal = gtk::Settings::default().map(|settings| {
            let signal = wake.handle.clone();
            let signal = settings.connect_notify_local(None, move |_, _| signal.wake());
            (settings, signal)
        });
        let fetch_wake = wake.handle.clone();
        let windows = windows::Windows::new(wake.handle.clone());
        let services = jadix_core::desktop_services::DesktopServices::default();
        let service_wake = wake.handle.clone();
        services.set_command_waker(Some(Rc::new(move || service_wake.wake())));
        let system_services = services::Services::new(services.clone(), wake.handle.clone());
        let io = io::Io::new(wake.handle.clone());
        let tray = tray::StatusItem::new(wake.handle.clone());
        let weak_services = Rc::downgrade(&system_services);
        window.connect_application_notify(move |window| {
            if let Some(services) = weak_services.upgrade() {
                services.bind(window.application().as_ref());
            }
        });
        let asset_root = std::env::current_dir()
            .and_then(|p| p.canonicalize())
            .map_err(|e| e.to_string())?;
        let fetch =
            jadix_net::native_fetch::NativeFetch::new("/usr/bin/curl", move || fetch_wake.wake());
        let command_wake = wake.handle.clone();
        fetch
            .service()
            .set_command_waker(Some(Rc::new(move || command_wake.wake())));
        let socket_wake = wake.handle.clone();
        let websocket = jadix_net::native_websocket::NativeWebSocket::new(move || socket_wake.wake())?;
        let web = web::Runtime::new(wake.handle.clone(), asset_root.clone(), fetch.service());
        let audio = audio::Runtime::new(wake.handle.clone())?;
        let environment_wake = wake.handle.clone();
        let environment_watch =
            crate::environment::SystemEnvironmentWatcher::start(move || environment_wake.wake())
                .map_err(|error| log::warn!("Linux environment notifications: {error}"))
                .ok();
        Ok(Self {
            window,
            content,
            items: BTreeMap::new(),
            model: None,
            events: Rc::new(Events {
                pending: Default::default(),
                publishing: Cell::new(false),
                disposed: Cell::new(false),
                overflow: Cell::new(false),
                wake: wake.handle.clone(),
                touch_ids: Default::default(),
                touch_serial: Cell::new(0),
            }),
            alive,
            environment: RefCell::new(crate::environment::EnvironmentSnapshot::default()),
            environment_watch,
            services,
            images: None,
            image_disk_cache: None,
            fetch,
            websocket,
            asset_root,
            windows,
            wake,
            web,
            audio,
            system_services,
            io,
            tray,
            fonts,
            components: Default::default(),
            settings_signal,
            preferences,
            updater: None,
            owns_updater: true,
        })
    }

    pub fn window(&self) -> &gtk::Window {
        &self.window
    }
    /// Configure a packaged directory installation before constructing its root.
    /// Paths identify this executable and packaged JSON containing id/version.
    pub fn configure_updater(
        &mut self,
        config: jadix_core::updater::Config,
        root: &std::path::Path,
        executable: &std::path::Path,
        metadata: &std::path::Path,
    ) -> Result<(), String> {
        if !self.is_alive() || self.updater.is_some() {
            return Err("updater already configured or host disposed".into());
        }
        let installation = jadix_core::updater::installation::Installation::new(
            root, executable, metadata, &config,
        )?;
        if installation
            .executable()
            .canonicalize()
            .map_err(|e| e.to_string())?
            != std::env::current_exe()
                .map_err(|e| e.to_string())?
                .canonicalize()
                .map_err(|e| e.to_string())?
        {
            return Err("updater installation must contain the running executable".into());
        }
        self.updater = Some(updater::Runtime::new(
            config,
            installation,
            &self.window,
            self.wake.handle.clone(),
        )?);
        Ok(())
    }
    pub fn register_component(
        &mut self,
        kind: impl Into<String>,
        factory: impl Fn(ComponentContext) -> Result<Box<dyn NativeComponent>, String> + 'static,
    ) -> Result<(), String> {
        if !self.is_alive() {
            return Err("GTK host disposed".into());
        }
        self.components.register(kind.into(), Rc::new(factory))
    }
    pub fn content(&self) -> &gtk::Fixed {
        &self.content
    }
    pub fn present(&self) {
        tray::prepare_activation(&self.window);
        self.window.present();
    }
    /// Bind once to the application's main-thread pump. The handle may also be
    /// sent to workers; callbacks run in the owning GLib main context.
    pub fn on_wake(&self, callback: impl Fn() + 'static) {
        self.wake.bind(callback);
    }
    pub fn wake_handle(&self) -> WakeHandle {
        self.wake.handle.clone()
    }
    /// Deliver a cold-start or embedding handoff URL through the same validated
    /// queue used by GApplication's cross-process `open` signal.
    pub fn deliver_deep_link(&self, link: jadix_core::deep_link::DeepLink) -> Result<(), String> {
        self.system_services.deliver(link)
    }
    /// Register independent root content before submitting window declarations.
    /// Children inherit the registry and asset directory. The returned application
    /// owns its root and reactive owner and is dropped when the window closes.
    pub fn register_window_factory(
        &mut self,
        name: impl Into<String>,
        factory: impl Fn(GtkHost, Option<&str>) -> Result<Box<dyn WindowApplication>, String> + 'static,
    ) -> Result<(), String> {
        self.windows.register(name.into(), Rc::new(factory))
    }
    pub fn viewport(&self) -> [f32; 2] {
        [
            self.content.width().max(1) as f32,
            self.content.height().max(1) as f32,
        ]
    }
    /// Asset URLs are resolved under this directory, never the process cwd at
    /// the time an asynchronous image job eventually executes.
    pub fn set_asset_root(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref().canonicalize().map_err(|e| e.to_string())?;
        if !path.is_dir() {
            return Err("asset root is not a directory".into());
        }
        self.web.set_asset_root(path.clone());
        self.asset_root = path;
        Ok(())
    }
    pub fn register_web_resources(
        &mut self,
        authority: &str,
        provider: impl Fn(jadix_core::web_resource::ResourceRequest, jadix_core::web_resource::ResourceReply)
            + Send
            + Sync
            + 'static,
    ) -> Result<(), &'static str> {
        self.web
            .resources
            .borrow_mut()
            .register(authority, provider)
    }

    /// Configure an application-owned cache before presenting the first root.
    /// Cache I/O stays on the image worker; unavailable storage is reported as a
    /// cache error and falls back to the source without blocking GTK.
    pub fn set_image_disk_cache(&mut self, cache: crate::images::ImageDiskCache) -> Result<(), String> {
        if !self.is_alive() || self.model.is_some() {
            return Err("configure image disk cache before first presentation".into());
        }
        cache.validate().map_err(|error| format!("image disk cache: {error:?}"))?;
        self.image_disk_cache = Some(cache);
        Ok(())
    }

    fn ensure_model(&mut self, root: RootId) -> Result<(), String> {
        if let Some(model) = &self.model {
            if model.id() != root {
                return Err("GTK host belongs to another root".into());
            }
        } else {
            let lifetime = RootLifetime::new(root);
            let wake = self.wake.handle.clone();
            let wake = move || wake.wake();
            self.images = Some(match self.image_disk_cache.clone() {
                Some(cache) => crate::images::ImageLoader::with_disk_cache(lifetime.clone(), Default::default(), cache, wake),
                None => crate::images::ImageLoader::new(lifetime.clone(), Default::default(), wake),
            }.map_err(|e| format!("image worker: {e:?}"))?);
            self.model = Some(HeadlessRoot::with_lifetime(lifetime, self.capabilities()));
        }
        Ok(())
    }

    fn remove(&mut self, id: NodeId) {
        if let Some(item) = self.items.remove(&id) {
            item.control.alive.set(false);
            if let Some(images) = &mut self.images {
                images.cancel(id);
            }
            if let Some(panel) = item.panel {
                panel.close();
            }
            if let Some(popover) = item.popover {
                popover.popdown();
                if popover.parent().is_some() {
                    popover.unparent();
                }
            }
            if let Some(parent) = item.surface.parent().and_downcast::<gtk::Fixed>() {
                parent.remove(&item.surface);
            }
            self.events
                .pending
                .borrow_mut()
                .retain(|event| event.node != id);
        }
    }

    fn place(
        &mut self,
        nodes: &[PresentationNode],
        parent: Option<NodeId>,
        origin: [f64; 2],
    ) -> Result<(), String> {
        for node in nodes {
            let id = node.id;
            let item = &self.items[&id];
            if item.control.descriptor.borrow().content.kind() != node.descriptor.content.kind() {
                return Err("GTK control kind changed without a fresh node identity".into());
            }
            let frame = node.descriptor.frame;
            if item.panel.is_none() && item.popover.is_none() {
                let container = parent
                    .and_then(|p| self.items[&p].children.as_ref())
                    .unwrap_or(&self.content);
                if item.surface.parent().as_ref() != Some(container.upcast_ref()) {
                    if let Some(old) = item.surface.parent().and_downcast::<gtk::Fixed>() {
                        old.remove(&item.surface);
                    }
                    container.put(&item.surface, frame.x - origin[0], frame.y - origin[1]);
                } else {
                    container.move_(&item.surface, frame.x - origin[0], frame.y - origin[1]);
                }
            }
            item.widget
                .set_size_request(pixel(frame.width), pixel(frame.height));
            controls::update(item, &node.descriptor, !node.children.is_empty());
            if let NodeContent::Hosted { specification } = &node.descriptor.content {
                hosted::update(
                    item,
                    &jadix_core::hosted::HostedContent::from_json(specification)
                        .map_err(str::to_owned)?,
                    self.events.clone(),
                );
            }
            style::apply(item, &node.descriptor);
            item.surface.update(&node.descriptor);
            accessibility::actions(item, &node.descriptor, self.events.clone());
            if let NodeContent::Hosted { specification } = &node.descriptor.content {
                if matches!(
                    jadix_core::hosted::HostedContent::from_json(specification),
                    Ok(jadix_core::hosted::HostedContent::TabBar { hidden: true, .. })
                ) {
                    item.widget.set_visible(false);
                }
            }
            item.control.revision.set(
                item.control
                    .revision
                    .get()
                    .max(revision(&node.descriptor.content)),
            );
            *item.control.descriptor.borrow_mut() = node.descriptor.clone();
            self.items.get_mut(&id).unwrap().parent = parent;
            if let NodeContent::Image { source, .. } = &node.descriptor.content {
                if self.items[&id].image_source.borrow().as_ref() != Some(source) {
                    *self.items[&id].image_source.borrow_mut() = Some(source.clone());
                    self.load_image(id, source);
                }
            }
            self.place(&node.children, Some(id), [frame.x, frame.y])?;
            if let Some(fixed) = &self.items[&id].children {
                // Fixed's natural size includes children; explicitly retain document
                // extent so scrolled children use shared logical coordinates.
                if matches!(node.descriptor.content, NodeContent::Scroll) {
                    let width = node
                        .children
                        .iter()
                        .map(|c| c.descriptor.frame.x + c.descriptor.frame.width - frame.x)
                        .fold(frame.width, f64::max);
                    let height = node
                        .children
                        .iter()
                        .map(|c| c.descriptor.frame.y + c.descriptor.frame.height - frame.y)
                        .fold(frame.height, f64::max);
                    fixed.set_size_request(pixel(width), pixel(height));
                }
            }
        }
        Ok(())
    }

    fn load_image(&mut self, id: NodeId, source: &str) {
        self.events.enqueue(
            &self.items[&id].control,
            EventPayload::ImageLoad(ImageLoadEvent::Started),
        );
        let image = self.items[&id]
            .widget
            .clone()
            .downcast::<gtk::Picture>()
            .unwrap();
        image.set_paintable(gdk::Paintable::NONE);
        if source.starts_with("data:") {
            let result = controls::texture(source);
            if let Ok(texture) = &result {
                image.set_paintable(Some(texture));
            }
            controls::image_result(&self.events, &self.items[&id].control, result);
            return;
        }
        let source = if source.starts_with("https://") {
            Some(crate::images::ImageSource::Url(source.to_owned()))
        } else if let Some(path) = jadix_core::web_resource::bundle_path(source) {
            self.asset_root
                .join(path)
                .canonicalize()
                .ok()
                .filter(|p| p.starts_with(&self.asset_root))
                .map(crate::images::ImageSource::File)
        } else if source.starts_with("file:///") {
            gio::File::for_uri(source)
                .path()
                .map(crate::images::ImageSource::File)
        } else {
            None
        };
        let result = source
            .ok_or(crate::images::ImageError::Source)
            .and_then(|source| {
                self.images
                    .as_mut()
                    .ok_or(crate::images::ImageError::Disposed)?
                    .request(self.model.as_ref().unwrap(), id, source)
            });
        if let Err(error) = result {
            controls::image_result(
                &self.events,
                &self.items[&id].control,
                Err(format!("{error:?}")),
            );
        }
    }

    fn panels(&self) {
        for item in self.items.values() {
            let descriptor = item.control.descriptor.borrow();
            match &descriptor.content {
                NodeContent::Dialog { open, .. } => {
                    if let Some(panel) = &item.panel {
                        panel.set_default_size(
                            pixel(descriptor.frame.width).max(1),
                            pixel(descriptor.frame.height).max(1),
                        );
                        if *open && !panel.is_visible() {
                            panel.present();
                        } else if !open {
                            panel.set_visible(false);
                        }
                    }
                }
                NodeContent::Popover {
                    anchor,
                    open,
                    close_on_outside_click,
                    ..
                } => {
                    let Some(popover) = &item.popover else {
                        continue;
                    };
                    let Some(anchor) = self.items.get(anchor) else {
                        popover.popdown();
                        continue;
                    };
                    if popover.parent().as_ref() != Some(&anchor.widget) {
                        if popover.parent().is_some() {
                            popover.unparent();
                        }
                        popover.set_parent(&anchor.widget);
                    }
                    popover.set_autohide(*close_on_outside_click);
                    if *open {
                        popover.popup();
                    } else {
                        popover.popdown();
                    }
                }
                _ => (),
            }
        }
    }
}

impl PlatformHost for GtkHost {
    fn register_font(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        if !self.is_alive() {
            return Err("GTK host disposed".into());
        }
        let families = self.fonts.register(bytes)?;
        self.content.pango_context().changed();
        self.window.queue_resize();
        self.wake.handle.wake();
        Ok(families)
    }
    fn measurement_revision(&self) -> u64 {
        self.fonts
            .revision()
            .wrapping_add(self.components.revision.get())
    }
    fn request_file(
        &mut self,
        request: &jadix_core::file_request::FileRequest,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::file_request::FileResult>, String> {
        self.io.file(&self.window, request)
    }
    fn clipboard(
        &mut self,
        request: &jadix_core::clipboard::ClipboardRequest,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::clipboard::ClipboardResult>, String> {
        self.io
            .clipboard(&WidgetExt::display(&self.window), request)
    }
    fn audio(&self) -> Option<jadix_core::audio::AudioService> {
        Some(self.audio.service.clone())
    }
    fn pump_audio(&mut self, _: RootId) {
        self.audio.pump(&self.asset_root);
    }
    fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        self.web.pump(&self.items)
    }
    fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> {
        self.web.resources.borrow_mut().take_work()
    }
    fn evaluate_web(
        &mut self,
        node: NodeId,
        script: &str,
    ) -> jadix_core::web_evaluation::WebEvaluation {
        let result = self.web.evaluations.borrow_mut().evaluate(node, script);
        self.wake.handle.wake();
        result
    }
    fn schedule_reactive_wake(&mut self, delay: Option<std::time::Duration>) {
        self.wake.schedule(delay);
    }
    fn capabilities(&self) -> capabilities::BackendCapabilities {
        let mut caps = capabilities::BackendCapabilities::headless();
        caps.inline_raster_sources = true;
        caps.inline_svg_sources = true;
        caps.local_image_sources = true;
        caps.remote_image_sources = true;
        caps
    }
    fn is_alive(&self) -> bool {
        self.alive.get() && !self.events.disposed.get()
    }
    fn desktop_services(&self) -> Option<jadix_core::desktop_services::DesktopServices> {
        Some(self.services.clone())
    }
    fn updater(&self) -> Option<jadix_core::updater::Updater> {
        self.updater.as_ref().map(|runtime| runtime.service.clone())
    }
    fn websocket(&self) -> Option<jadix_net::websocket::SocketService> { Some(self.websocket.service()) }
    fn fetch(&self) -> Option<jadix_net::service::FetchService> {
        Some(self.fetch.service())
    }
    fn window_declarations(&self) -> Option<jadix_core::window::WindowDeclarations> {
        Some(self.windows.declarations())
    }
    fn request_window(
        &mut self,
        request: &jadix_core::window::WindowRequest,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::window::WindowResult>, String> {
        self.windows.request(request)
    }
    fn read_environment(&self) -> Result<Option<crate::environment::EnvironmentSnapshot>, String> {
        use crate::environment::{ColorScheme, LayoutDirection, Lifecycle};
        if let Some(Ok(snapshot)) = self
            .environment_watch
            .as_ref()
            .and_then(|watch| watch.take())
        {
            *self.environment.borrow_mut() = snapshot;
        }
        let mut snapshot = self.environment.borrow().clone();
        snapshot.wake_count = self.preferences.resume.count.get();
        if let Some(settings) = gtk::Settings::default() {
            snapshot.color_scheme = self
                .preferences
                .scheme
                .get()
                .or(snapshot.color_scheme)
                .or_else(|| {
                    Some(
                        if settings.is_gtk_application_prefer_dark_theme()
                            || settings
                                .gtk_theme_name()
                                .is_some_and(|v| v.to_lowercase().contains("dark"))
                        {
                            ColorScheme::Dark
                        } else {
                            ColorScheme::Light
                        },
                    )
                });
            snapshot.reduced_motion = Some(!settings.is_gtk_enable_animations());
            snapshot.high_contrast = Some(
                settings
                    .gtk_theme_name()
                    .is_some_and(|v| v.to_ascii_lowercase().contains("highcontrast")),
            );
            let dpi: i32 = settings.property("gtk-xft-dpi");
            if dpi > 0 {
                snapshot.font_scale = (dpi as f32 / (96. * 1024.)).clamp(0.5, 4.);
            }
        }
        snapshot.layout_direction = Some(if self.content.direction() == gtk::TextDirection::Rtl {
            LayoutDirection::RightToLeft
        } else {
            LayoutDirection::LeftToRight
        });
        snapshot.lifecycle = if !self.window.is_visible()
            || self
                .window
                .surface()
                .and_downcast::<gdk::Toplevel>()
                .is_some_and(|s| s.state().contains(gdk::ToplevelState::MINIMIZED))
        {
            Lifecycle::Background
        } else if self.window.is_active() {
            Lifecycle::Active
        } else {
            Lifecycle::Inactive
        };
        Ok(Some(snapshot))
    }
    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        if let Some(size) = self
            .items
            .get(&request.key.node)
            .and_then(|item| item.control.intrinsic.get())
        {
            return Ok(MeasureResult {
                key: request.key,
                width: size[0].min(request.key.max_width),
                height: size[1].min(request.key.max_height),
                baseline: None,
            });
        }
        style::measure(&self.content, request, descriptor)
    }
    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String> {
        self.ensure_model(root)?;
        let Some(batch) = self
            .model
            .as_ref()
            .unwrap()
            .diff_batch(roots)
            .map_err(|e| format!("{e:?}"))?
        else {
            return Ok(());
        };
        if batch.revision != revision {
            return Err("GTK publication revision mismatch".into());
        }
        self.publish_batch(&batch, roots).map(|_| ())
    }
    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        if self.events.disposed.get() {
            return Err("GTK host disposed".into());
        }
        self.ensure_model(batch.root)?;
        let mut candidate = self.model.as_ref().unwrap().clone();
        candidate
            .commit(batch.clone())
            .map_err(|e| format!("GTK batch rejected: {e:?}"))?;
        // The batch and its tree projection must describe the same accepted state.
        // Allocate every fallible surface before removing or changing live widgets.
        fn prepare(
            host: &GtkHost,
            model: &HeadlessRoot,
            nodes: &[PresentationNode],
            expected: &[NodeId],
            staged: &mut BTreeMap<NodeId, Item>,
        ) -> Result<(), String> {
            if !nodes.iter().map(|n| n.id).eq(expected.iter().copied()) {
                return Err("GTK tree differs from accepted batch".into());
            }
            for node in nodes {
                let accepted = model
                    .node(node.id)
                    .ok_or("GTK tree contains an uncommitted node")?;
                if accepted.descriptor != node.descriptor {
                    return Err("GTK descriptor differs from accepted batch".into());
                }
                visual::validate(&node.descriptor)?;
                let replace = host.items.get(&node.id).is_none_or(|item| {
                    let old = item.control.descriptor.borrow();
                    match (&old.content, &node.descriptor.content) {
                        (
                            NodeContent::Hosted { specification: a },
                            NodeContent::Hosted { specification: b },
                        ) => {
                            match (
                                jadix_core::hosted::HostedContent::from_json(a),
                                jadix_core::hosted::HostedContent::from_json(b),
                            ) {
                                (
                                    Ok(jadix_core::hosted::HostedContent::Custom {
                                        kind: a, ..
                                    }),
                                    Ok(jadix_core::hosted::HostedContent::Custom {
                                        kind: b, ..
                                    }),
                                ) => a != b,
                                (Ok(a), Ok(b)) => {
                                    std::mem::discriminant(&a) != std::mem::discriminant(&b)
                                }
                                _ => true,
                            }
                        }
                        _ => false,
                    }
                });
                if replace {
                    staged.insert(
                        node.id,
                        controls::create(
                            node.id,
                            &node.descriptor,
                            host.events.clone(),
                            &host.window,
                            host.web.clone(),
                            &host.components,
                        )?,
                    );
                }
                prepare(host, model, &node.children, &accepted.children, staged)?;
            }
            Ok(())
        }
        let mut staged = BTreeMap::new();
        if let Err(error) = prepare(self, &candidate, roots, candidate.roots(), &mut staged) {
            for item in staged.values() {
                item.control.alive.set(false);
                if let Some(panel) = &item.panel {
                    panel.destroy();
                }
            }
            return Err(error);
        }
        self.events.publishing.set(true);
        let removed: Vec<_> = self
            .items
            .keys()
            .copied()
            .filter(|id| candidate.node(*id).is_none())
            .collect();
        for id in removed {
            self.remove(id);
        }
        for (id, item) in staged {
            self.remove(id);
            self.items.insert(id, item);
        }
        // Image jobs are bound to the accepted candidate, including newly created nodes.
        self.model = Some(candidate);
        self.web.evaluations.borrow_mut().sync(roots);
        let result = self.place(roots, None, [0., 0.]);
        if result.is_ok() {
            accessibility::relations(&self.items);
            self.panels();
            for op in &batch.operations {
                if let Operation::Focus { node } = op {
                    if let Some(item) = node.and_then(|id| self.items.get(&id)) {
                        item.widget.grab_focus();
                    } else {
                        GtkWindowExt::set_focus(&self.window, gtk::Widget::NONE);
                    }
                }
            }
        }
        self.events.publishing.set(false);
        result?;
        Ok(ApplyReceipt::for_batch(batch))
    }
    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        if let Some(error) = self.tray.take_error() {
            return Err(error);
        }
        if let Some(updater) = &self.updater {
            updater.pump();
        }
        self.windows.pump(
            &self.window,
            &self.asset_root,
            &self.fonts,
            &self.components,
            &self.updater,
        );
        self.fetch.pump();
        for item in self.items.values() {
            match &item.hosted {
                Some(hosted::State::RichEditor(editor)) => editor.pump(),
                Some(hosted::State::Shader(shader)) => shader.pump(),
                _ => (),
            }
        }
        self.system_services.pump();
        if let (Some(images), Some(model)) = (&mut self.images, &self.model) {
            for result in images.poll(model) {
                if let Some(item) = self.items.get(&result.node) {
                    let result = result
                        .result
                        .map_err(|e| format!("{e:?}"))
                        .and_then(|source| controls::texture(&source));
                    if let Ok(texture) = &result {
                        item.widget
                            .clone()
                            .downcast::<gtk::Picture>()
                            .unwrap()
                            .set_paintable(Some(texture));
                    }
                    controls::image_result(&self.events, &item.control, result);
                }
            }
        }
        if self.events.overflow.replace(false) {
            return Err("GTK event queue exhausted; root must be disposed".into());
        }
        let mut events: Vec<_> = self.events.pending.borrow_mut().drain(..).collect();
        events.retain(|event| {
            self.items
                .get(&event.node)
                .is_some_and(|item| item.control.alive.get())
        });
        Ok(events)
    }
    fn bind_navigation(&mut self, _: &[NodeId]) -> Result<bool, String> {
        Ok(true)
    }
    fn bind_grid_navigation(&mut self, _: &[NodeId]) -> Result<bool, String> {
        Ok(true)
    }
    fn configure_deep_links(
        &mut self,
        config: jadix_core::deep_link::DeepLinkConfig,
    ) -> Result<(), String> {
        self.system_services.configure(config)
    }
    fn take_deep_links(&mut self) -> Vec<jadix_core::deep_link::DeepLink> {
        self.system_services.drain()
    }
    fn set_tray(&mut self, _: RootId, tray: Option<&jadix_core::tray::Tray>) -> Result<(), String> {
        self.tray.set(tray)
    }
    fn take_tray_events(&mut self) -> Vec<String> {
        self.tray.take_events()
    }
    fn scroll_to(&mut self, id: NodeId, offset: [f64; 2]) -> Result<(), String> {
        if !offset.iter().all(|v| v.is_finite() && *v >= 0.) {
            return Err("invalid scroll offset".into());
        }
        let scroll = self
            .items
            .get(&id)
            .ok_or("unknown scroll node")?
            .widget
            .clone()
            .downcast::<gtk::ScrolledWindow>()
            .map_err(|_| "node is not a scroll viewport")?;
        for (adjustment, value) in [
            (scroll.hadjustment(), offset[0]),
            (scroll.vadjustment(), offset[1]),
        ] {
            adjustment.set_value(value.clamp(
                adjustment.lower(),
                (adjustment.upper() - adjustment.page_size()).max(adjustment.lower()),
            ));
        }
        Ok(())
    }
    fn open_url(
        &mut self,
        url: &str,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        jadix_core::desktop::check_open_url(url)?;
        let (send, recv) = std::sync::mpsc::channel();
        let wake = self.wake.handle.clone();
        gio::AppInfo::launch_default_for_uri_async(
            url,
            None::<&gio::AppLaunchContext>,
            gio::Cancellable::NONE,
            move |result| {
                let _ = send.send(result.map(|_| true).map_err(|e| e.to_string()));
                wake.wake();
            },
        );
        Ok(recv)
    }
    fn dispose(&mut self) {
        if self.events.disposed.replace(true) {
            return;
        }
        self.events.publishing.set(true);
        let ids: Vec<_> = self.items.keys().copied().collect();
        for id in ids {
            self.remove(id);
        }
        if let Some(images) = &mut self.images {
            images.dispose();
        }
        if let Some(model) = &self.model {
            model.lifetime().dispose();
        }
        self.system_services.dispose();
        self.io.dispose();
        self.tray.dispose();
        self.preferences.dispose();
        if let Some(mut watcher) = self.environment_watch.take() {
            watcher.close();
        }
        if let Some((settings, signal)) = self.settings_signal.take() {
            settings.disconnect(signal);
        }
        self.fetch.dispose();
        self.websocket.dispose();
        self.web.dispose();
        self.audio.dispose();
        self.windows.dispose();
        if self.owns_updater {
            if let Some(updater) = self.updater.take() {
                updater.dispose();
            }
        }
        self.wake.dispose();
        self.window.close();
        self.alive.set(false);
        self.events.pending.borrow_mut().clear();
    }
}
impl Drop for GtkHost {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn pixel(value: f64) -> i32 {
    value.ceil().clamp(0., i32::MAX as f64) as i32
}
fn revision(content: &NodeContent) -> u64 {
    match content {
        NodeContent::Input { revision, .. }
        | NodeContent::Textarea { revision, .. }
        | NodeContent::Checkbox { revision, .. }
        | NodeContent::Switch { revision, .. }
        | NodeContent::Select { revision, .. }
        | NodeContent::Dialog { revision, .. }
        | NodeContent::Popover { revision, .. } => *revision,
        _ => 0,
    }
}
