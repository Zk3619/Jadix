//! StatusNotifierItem and dbusmenu, with independent object/name leases per root.
use super::*;
use glib::variant::{ObjectPath, ToVariant};
use jadix_core::{hosted::HostedMenuItem, tray::Tray};
const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
thread_local! {static ACTIVATION: RefCell<Option<(std::time::Instant,String)>>=const { RefCell::new(None) };}
pub(super) fn prepare_activation(window: &gtk::Window) {
    if let Some((at, token)) = ACTIVATION.with(|slot| slot.borrow_mut().take()) {
        if at.elapsed() < std::time::Duration::from_secs(10) {
            window.set_startup_id(&token);
        }
    }
}
const XML: &str = r#"<node><interface name="org.kde.StatusNotifierItem">
<property name="Category" type="s" access="read"/><property name="Id" type="s" access="read"/>
<property name="Title" type="s" access="read"/><property name="Status" type="s" access="read"/>
<property name="IconName" type="s" access="read"/><property name="IconPixmap" type="a(iiay)" access="read"/>
<property name="ToolTip" type="(sa(iiay)ss)" access="read"/><property name="ItemIsMenu" type="b" access="read"/>
<property name="XAyatanaLabel" type="s" access="read"/><property name="XAyatanaLabelGuide" type="s" access="read"/>
<property name="Menu" type="o" access="read"/>
<method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
<method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
<method name="ProvideXdgActivationToken"><arg type="s" direction="in"/></method>
<signal name="NewTitle"/><signal name="NewIcon"/><signal name="NewToolTip"/>
<signal name="XAyatanaNewLabel"><arg type="s"/><arg type="s"/></signal>
</interface><interface name="com.canonical.dbusmenu">
<property name="Version" type="u" access="read"/><property name="TextDirection" type="s" access="read"/>
<property name="Status" type="s" access="read"/><property name="IconThemePath" type="as" access="read"/>
<method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
<method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
<method name="GetProperty"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
<method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
<method name="EventGroup"><arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/></method>
<method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
<method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
<signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
</interface></node>"#;

struct Entry {
    parent: i32,
    action: Option<String>,
    properties: BTreeMap<String, glib::Variant>,
}
#[derive(Default)]
struct Model {
    tray: Option<Tray>,
    entries: BTreeMap<i32, Entry>,
    serial: i32,
    revision: u32,
}
impl Model {
    fn replace(&mut self, tray: Option<&Tray>) -> Result<(), String> {
        if self.tray.as_ref() == tray {
            return Ok(());
        }
        if let Some(tray) = tray {
            tray.validate()?;
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("tray revisions exhausted")?;
        let mut serial = self.serial;
        let mut entries = BTreeMap::new();
        fn walk(
            items: &[HostedMenuItem],
            parent: i32,
            enabled: bool,
            serial: &mut i32,
            out: &mut BTreeMap<i32, Entry>,
        ) -> Result<(), String> {
            for item in items {
                *serial = serial
                    .checked_add(1)
                    .ok_or("tray menu identities exhausted")?;
                let id = *serial;
                let mut properties = BTreeMap::from([
                    ("label".into(), item.label.replace('_', "__").to_variant()),
                    ("enabled".into(), (enabled && item.enabled).to_variant()),
                    ("visible".into(), true.to_variant()),
                ]);
                if item.checked {
                    properties.insert("toggle-type".into(), "checkmark".to_variant());
                    properties.insert("toggle-state".into(), 1i32.to_variant());
                }
                if let Some(symbol) = &item.symbol {
                    properties.insert("icon-name".into(), symbol.to_variant());
                }
                if !item.children.is_empty() {
                    properties.insert("children-display".into(), "submenu".to_variant());
                }
                let action =
                    (enabled && item.enabled && item.children.is_empty()).then(|| item.id.clone());
                out.insert(
                    id,
                    Entry {
                        parent,
                        action,
                        properties,
                    },
                );
                walk(&item.children, id, enabled && item.enabled, serial, out)?;
            }
            Ok(())
        }
        if let Some(tray) = tray {
            walk(&tray.menu, 0, true, &mut serial, &mut entries)?;
        }
        self.tray = tray.cloned();
        self.entries = entries;
        self.serial = serial;
        self.revision = revision;
        Ok(())
    }
    fn properties(&self, id: i32, names: &[String]) -> Option<BTreeMap<String, glib::Variant>> {
        let values = if id == 0 {
            BTreeMap::from([("children-display".into(), "submenu".to_variant())])
        } else {
            self.entries.get(&id)?.properties.clone()
        };
        Some(
            values
                .into_iter()
                .filter(|(name, _)| names.is_empty() || names.contains(name))
                .collect(),
        )
    }
    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Option<glib::Variant> {
        let properties = self.properties(id, names)?;
        let children: Vec<_> = if depth == 0 {
            Vec::new()
        } else {
            self.entries
                .iter()
                .filter(|(_, entry)| entry.parent == id)
                .filter_map(|(&id, _)| self.layout(id, depth.saturating_sub(1), names))
                .collect()
        };
        Some((id, properties, children).to_variant())
    }
    fn action(&self, id: i32) -> Option<String> {
        let action = self.entries.get(&id)?.action.as_ref()?;
        self.tray
            .as_ref()?
            .accepts_command(action)
            .then(|| action.clone())
    }
}
struct Connection {
    bus: gio::DBusConnection,
    objects: Vec<gio::RegistrationId>,
    watcher: gio::WatcherId,
    owner: gio::OwnerId,
}
pub(super) struct StatusItem {
    icons: RefCell<Vec<(i32, i32, Vec<u8>)>>,
    model: RefCell<Model>,
    connection: RefCell<Option<Connection>>,
    connecting: RefCell<Option<gio::Cancellable>>,
    events: RefCell<VecDeque<String>>,
    wake: WakeHandle,
    name: String,
    disposed: Cell<bool>,
    error: RefCell<Option<String>>,
    generation: Cell<u64>,
}
impl StatusItem {
    pub fn new(wake: WakeHandle) -> Rc<Self> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Rc::new(Self {
            icons: Default::default(),
            model: Default::default(),
            connection: Default::default(),
            connecting: Default::default(),
            events: Default::default(),
            wake,
            name: format!(
                "org.kde.StatusNotifierItem-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            disposed: Cell::new(false),
            error: Default::default(),
            generation: Cell::new(0),
        })
    }
    pub fn set(self: &Rc<Self>, tray: Option<&Tray>) -> Result<(), String> {
        if self.disposed.get() {
            return Err("tray root is disposed".into());
        }
        if self.model.borrow().tray.as_ref() == tray
            && (tray.is_none()
                || self.connection.borrow().is_some()
                || self.connecting.borrow().is_some())
        {
            return Ok(());
        }
        let address = if tray.is_some() && self.connection.borrow().is_none() {
            Some(
                std::env::var("DBUS_SESSION_BUS_ADDRESS")
                    .map_err(|_| "system tray requires a session bus")?,
            )
        } else {
            None
        };
        self.model.borrow_mut().replace(tray)?;
        *self.icons.borrow_mut() = tray.map_or_else(Vec::new, |tray| {
            icon_pixmaps(tray.symbol.as_deref().unwrap_or("application-x-executable"))
        });
        if tray.is_none() {
            self.disconnect();
            self.events.borrow_mut().clear();
            return Ok(());
        }
        if self.connection.borrow().is_some() {
            self.changed();
            return Ok(());
        }
        if self.connecting.borrow().is_some() {
            return Ok(());
        }
        let address = address.ok_or("missing session bus address")?;
        let generation = self
            .generation
            .get()
            .checked_add(1)
            .ok_or("tray connection identities exhausted")?;
        self.generation.set(generation);
        let cancel = gio::Cancellable::new();
        *self.connecting.borrow_mut() = Some(cancel.clone());
        let weak = Rc::downgrade(self);
        gio::DBusConnection::for_address(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            Some(&cancel),
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                if this.generation.get() != generation {
                    if let Ok(bus) = result {
                        bus.close(gio::Cancellable::NONE, |_| {});
                    }
                    return;
                }
                this.connecting.borrow_mut().take();
                if this.disposed.get() || this.model.borrow().tray.is_none() {
                    return;
                }
                let result = result
                    .map_err(|e| e.to_string())
                    .and_then(|bus| this.publish(bus));
                if let Err(error) = result {
                    *this.error.borrow_mut() = Some(error);
                    this.wake.wake();
                }
            },
        );
        Ok(())
    }
    fn publish(self: &Rc<Self>, bus: gio::DBusConnection) -> Result<(), String> {
        bus.set_exit_on_close(false);
        let info = gio::DBusNodeInfo::for_xml(XML).map_err(|e| e.to_string())?;
        let mut objects = Vec::new();
        for (path, interface) in [(ITEM_PATH, ITEM), (MENU_PATH, MENU)] {
            let weak = Rc::downgrade(self);
            let properties = weak.clone();
            let object = bus
                .register_object(
                    path,
                    &info
                        .lookup_interface(interface)
                        .ok_or("missing tray interface")?,
                )
                .method_call(move |_, _, _, interface, method, args, invocation| {
                    let Some(this) = weak.upgrade() else {
                        invocation.return_dbus_error(
                            "org.freedesktop.DBus.Error.Failed",
                            "tray disposed",
                        );
                        return;
                    };
                    if interface == Some(MENU) {
                        match this.menu_call(method, &args) {
                            Ok(reply) => invocation.return_value(Some(&reply)),
                            Err(error) => invocation
                                .return_dbus_error("org.freedesktop.DBus.Error.InvalidArgs", error),
                        }
                    } else if method == "ProvideXdgActivationToken" {
                        if let Some((token,)) = args.get::<(String,)>().filter(|(token,)| {
                            !token.is_empty()
                                && token.len() <= 4096
                                && !token.chars().any(char::is_control)
                        }) {
                            ACTIVATION.with(|slot| {
                                *slot.borrow_mut() = Some((std::time::Instant::now(), token))
                            });
                            invocation.return_value(Some(&().to_variant()));
                        } else {
                            invocation.return_dbus_error(
                                "org.freedesktop.DBus.Error.InvalidArgs",
                                "invalid activation token",
                            );
                        }
                    } else {
                        invocation.return_value(Some(&().to_variant()));
                    }
                })
                .property(move |_, _, _, interface, name| {
                    properties
                        .upgrade()
                        .map(|this| this.property(interface, name))
                        .unwrap_or_else(|| "".to_variant())
                })
                .build();
            match object {
                Ok(object) => objects.push(object),
                Err(error) => {
                    for object in objects {
                        let _ = bus.unregister_object(object);
                    }
                    return Err(error.to_string());
                }
            }
        }
        let weak = Rc::downgrade(self);
        let watcher = gio::bus_watch_name_on_connection(
            &bus,
            WATCHER,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| {
                if let Some(this) = weak.upgrade() {
                    this.register();
                }
            },
            |_, _| {},
        );
        let weak = Rc::downgrade(self);
        let owner = gio::bus_own_name_on_connection(
            &bus,
            &self.name,
            gio::BusNameOwnerFlags::NONE,
            move |_, _| {
                if let Some(this) = weak.upgrade() {
                    this.register();
                }
            },
            |_, _| {},
        );
        *self.connection.borrow_mut() = Some(Connection {
            bus,
            objects,
            watcher,
            owner,
        });
        Ok(())
    }
    fn register(&self) {
        if let Some(connection) = self.connection.borrow().as_ref() {
            connection.bus.call(
                Some(WATCHER),
                "/StatusNotifierWatcher",
                WATCHER,
                "RegisterStatusNotifierItem",
                Some(&(self.name.as_str(),).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                5000,
                gio::Cancellable::NONE,
                |_| {},
            );
        }
    }
    fn changed(&self) {
        if let Some(connection) = self.connection.borrow().as_ref() {
            for signal in ["NewTitle", "NewIcon", "NewToolTip"] {
                let _ = connection
                    .bus
                    .emit_signal(None, ITEM_PATH, ITEM, signal, None);
            }
            let model = self.model.borrow();
            let label = model
                .tray
                .as_ref()
                .map(Tray::plain_title)
                .unwrap_or_default();
            let _ = connection.bus.emit_signal(
                None,
                ITEM_PATH,
                ITEM,
                "XAyatanaNewLabel",
                Some(&(label, "").to_variant()),
            );
            let _ = connection.bus.emit_signal(
                None,
                MENU_PATH,
                MENU,
                "LayoutUpdated",
                Some(&(model.revision, 0i32).to_variant()),
            );
        }
    }
    fn property(&self, interface: &str, name: &str) -> glib::Variant {
        if interface == MENU {
            return match name {
                "Version" => 3u32.to_variant(),
                "Status" => "normal".to_variant(),
                "IconThemePath" => Vec::<String>::new().to_variant(),
                _ => "ltr".to_variant(),
            };
        }
        let model = self.model.borrow();
        let title = model
            .tray
            .as_ref()
            .map(Tray::plain_title)
            .unwrap_or_default();
        let icon = model
            .tray
            .as_ref()
            .and_then(|t| t.symbol.as_deref())
            .unwrap_or("application-x-executable");
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" => self.name.to_variant(),
            "Title" | "XAyatanaLabel" => title.to_variant(),
            "Status" => "Active".to_variant(),
            "IconName" => icon.to_variant(),
            "IconPixmap" => self.icons.borrow().to_variant(),
            "ItemIsMenu" => true.to_variant(),
            "Menu" => ObjectPath::try_from(MENU_PATH).unwrap().to_variant(),
            "ToolTip" => (
                icon,
                self.icons.borrow().clone(),
                title,
                model
                    .tray
                    .as_ref()
                    .map(|t| t.tooltip.as_str())
                    .unwrap_or(""),
            )
                .to_variant(),
            _ => "".to_variant(),
        }
    }
    fn click(&self, id: i32) -> bool {
        let Some(action) = self.model.borrow().action(id) else {
            return false;
        };
        let mut events = self.events.borrow_mut();
        if events.len() >= 64 {
            events.pop_front();
        }
        events.push_back(action);
        self.wake.wake();
        true
    }
    fn menu_call(&self, method: &str, args: &glib::Variant) -> Result<glib::Variant, &'static str> {
        match method {
            "GetLayout" => {
                let (id, depth, names) = args
                    .get::<(i32, i32, Vec<String>)>()
                    .ok_or("invalid layout request")?;
                if names.len() > 64 || depth < -1 {
                    return Err("invalid layout bounds");
                }
                let model = self.model.borrow();
                let layout = model
                    .layout(id, depth.min(8), &names)
                    .ok_or("unknown menu item")?;
                Ok(glib::Variant::tuple_from_iter([
                    model.revision.to_variant(),
                    layout,
                ]))
            }
            "GetGroupProperties" => {
                let (ids, names) = args
                    .get::<(Vec<i32>, Vec<String>)>()
                    .ok_or("invalid property request")?;
                if ids.len() > 256 || names.len() > 64 {
                    return Err("property request exceeds quota");
                }
                let model = self.model.borrow();
                let values: Vec<_> = ids
                    .into_iter()
                    .filter_map(|id| model.properties(id, &names).map(|p| (id, p)))
                    .collect();
                Ok((values,).to_variant())
            }
            "GetProperty" => {
                let (id, name) = args
                    .get::<(i32, String)>()
                    .ok_or("invalid property request")?;
                let value = self
                    .model
                    .borrow()
                    .properties(id, &[name.clone()])
                    .and_then(|mut p| p.remove(&name))
                    .ok_or("unknown property")?;
                Ok((value,).to_variant())
            }
            "Event" => {
                let (id, event, _, _) = args
                    .get::<(i32, String, glib::Variant, u32)>()
                    .ok_or("invalid menu event")?;
                if event == "clicked" {
                    self.click(id);
                }
                Ok(().to_variant())
            }
            "EventGroup" => {
                let (events,) = args
                    .get::<(Vec<(i32, String, glib::Variant, u32)>,)>()
                    .ok_or("invalid menu events")?;
                if events.len() > 256 {
                    return Err("menu events exceed quota");
                }
                let errors: Vec<i32> = events
                    .into_iter()
                    .filter_map(|(id, event, _, _)| {
                        (event == "clicked" && !self.click(id)).then_some(id)
                    })
                    .collect();
                Ok((errors,).to_variant())
            }
            "AboutToShow" => Ok((false,).to_variant()),
            "AboutToShowGroup" => Ok((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
            _ => Err("unknown menu method"),
        }
    }
    pub fn take_events(&self) -> Vec<String> {
        self.events.borrow_mut().drain(..).collect()
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.borrow_mut().take()
    }
    fn disconnect(&self) {
        self.generation.set(self.generation.get().saturating_add(1));
        if let Some(cancel) = self.connecting.borrow_mut().take() {
            cancel.cancel();
        }
        if let Some(mut connection) = self.connection.borrow_mut().take() {
            for object in connection.objects.drain(..) {
                let _ = connection.bus.unregister_object(object);
            }
            gio::bus_unwatch_name(connection.watcher);
            gio::bus_unown_name(connection.owner);
            connection.bus.close(gio::Cancellable::NONE, |_| {});
        }
    }
    pub fn dispose(&self) {
        if !self.disposed.replace(true) {
            self.disconnect();
            self.events.borrow_mut().clear();
        }
    }
}
impl Drop for StatusItem {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn icon_pixmaps(name: &str) -> Vec<(i32, i32, Vec<u8>)> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let theme = gtk::IconTheme::for_display(&display);
    [16, 24, 32]
        .into_iter()
        .filter_map(|size| {
            let icon = theme.lookup_icon(
                name,
                &["application-x-executable"],
                size,
                1,
                gtk::TextDirection::None,
                gtk::IconLookupFlags::PRELOAD,
            );
            let path = icon.file()?.path()?;
            if std::fs::metadata(&path).ok()?.len() > 768 * 1024 {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            let texture: gdk::Texture = if let Some((pixels, _)) =
                crate::images::svg::rasterize(&bytes, size as u32, size as u32, 0)
            {
                gdk::MemoryTexture::new(
                    pixels.width() as i32,
                    pixels.height() as i32,
                    gdk::MemoryFormat::R8g8b8a8Premultiplied,
                    &glib::Bytes::from_owned(pixels.data().to_vec()),
                    pixels.width() as usize * 4,
                )
                .upcast()
            } else {
                let dimensions = imagesize::blob_size(&bytes).ok()?;
                if dimensions.width > 4096
                    || dimensions.height > 4096
                    || dimensions.width.saturating_mul(dimensions.height) > 4 * 1024 * 1024
                {
                    return None;
                }
                gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)).ok()?
            };
            let mut downloader = gdk::TextureDownloader::new(&texture);
            downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
            let (pixels, stride) = downloader.download_bytes();
            Some((
                size,
                size,
                argb_icon(
                    &pixels,
                    stride,
                    texture.width() as usize,
                    texture.height() as usize,
                    size as usize,
                ),
            ))
        })
        .collect()
}
// SNI uses network-order, unpremultiplied ARGB, independently of host endian.
fn argb_icon(pixels: &[u8], stride: usize, width: usize, height: usize, size: usize) -> Vec<u8> {
    let mut out = vec![0; size * size * 4];
    let scale = (size as f64 / width as f64).min(size as f64 / height as f64);
    let w = (width as f64 * scale).round().max(1.) as usize;
    let h = (height as f64 * scale).round().max(1.) as usize;
    for y in 0..h {
        for x in 0..w {
            let source = (y * height / h) * stride + (x * width / w) * 4;
            let target = ((y + (size - h) / 2) * size + x + (size - w) / 2) * 4;
            let rgba = &pixels[source..source + 4];
            out[target..target + 4].copy_from_slice(&[rgba[3], rgba[0], rgba[1], rgba[2]]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_icons_use_argb_and_transparent_aspect_padding() {
        let out = argb_icon(&[255, 0, 0, 128, 0, 255, 0, 255], 8, 2, 1, 4);
        assert_eq!(&out[..16], &[0; 16]);
        assert_eq!(&out[16..24], &[128, 255, 0, 0, 128, 255, 0, 0]);
        assert_eq!(&out[24..32], &[255, 0, 255, 0, 255, 0, 255, 0]);
        assert_eq!(&out[48..], &[0; 16]);
    }
}
