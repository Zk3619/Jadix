//! Portal sessions and request subscriptions belong to the requesting root.
use super::*;
use glib::variant::{ObjectPath, ToVariant};
use jadix_core::desktop_services::*;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
type Reply = Result<glib::VariantDict, ServiceError>;
type Completion = Box<dyn FnOnce(Reply)>;

struct Request {
    bus: gio::DBusConnection,
    path: String,
    cancel: gio::Cancellable,
    signal: RefCell<Option<gio::SignalSubscription>>,
    timer: RefCell<Option<glib::SourceId>>,
    done: RefCell<Option<Completion>>,
}
impl Request {
    fn finish(&self, result: Reply, close: bool) {
        let done = self.done.borrow_mut().take();
        self.signal.borrow_mut().take();
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.cancel.cancel();
        if close {
            close_object(&self.bus, &self.path, "org.freedesktop.portal.Request");
        }
        if let Some(done) = done {
            done(result);
        }
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.finish(Err(ServiceError::Failed), true);
    }
}
fn token() -> String {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "jadix{}_{}",
        std::process::id(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn close_object(bus: &gio::DBusConnection, path: &str, interface: &str) {
    bus.call(
        Some(PORTAL),
        path,
        interface,
        "Close",
        None,
        None,
        gio::DBusCallFlags::NONE,
        3000,
        gio::Cancellable::NONE,
        |_| {},
    );
}
fn request(
    bus: &gio::DBusConnection,
    method: &str,
    args: impl FnOnce(glib::Variant) -> glib::Variant,
    options: glib::VariantDict,
    done: impl FnOnce(Reply) + 'static,
) -> Rc<Request> {
    let token = token();
    options.insert("handle_token", token.as_str());
    let sender = bus
        .unique_name()
        .map(|name| name.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    let request = Rc::new(Request {
        bus: bus.clone(),
        path: format!("{PATH}/request/{sender}/{token}"),
        cancel: gio::Cancellable::new(),
        signal: Default::default(),
        timer: Default::default(),
        done: RefCell::new(Some(Box::new(done))),
    });
    let weak = Rc::downgrade(&request);
    *request.signal.borrow_mut() = Some(bus.subscribe_to_signal(
        Some(PORTAL),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&request.path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(request) = weak.upgrade() else {
                return;
            };
            let result = match signal
                .parameters
                .try_child_value(0)
                .and_then(|v| v.get::<u32>())
            {
                Some(0) => signal
                    .parameters
                    .try_child_value(1)
                    .map(|v| glib::VariantDict::new(Some(&v)))
                    .ok_or(ServiceError::Failed),
                Some(1) => Err(ServiceError::Denied),
                _ => Err(ServiceError::Failed),
            };
            request.finish(result, false);
        },
    ));
    let weak = Rc::downgrade(&request);
    *request.timer.borrow_mut() = Some(glib::timeout_add_local_once(
        std::time::Duration::from_secs(120),
        move || {
            if let Some(request) = weak.upgrade() {
                request.timer.borrow_mut().take();
                request.finish(Err(ServiceError::Failed), true);
            }
        },
    ));
    let weak = Rc::downgrade(&request);
    bus.call(
        Some(PORTAL),
        PATH,
        INTERFACE,
        method,
        Some(&args(options.end())),
        None,
        gio::DBusCallFlags::NONE,
        10000,
        Some(&request.cancel),
        move |result| {
            if result.is_err() {
                if let Some(request) = weak.upgrade() {
                    request.finish(Err(ServiceError::Unavailable), true);
                }
            }
        },
    );
    request
}

struct Registration {
    shortcut: GlobalShortcut,
    session: Option<String>,
    request: Option<Rc<Request>>,
}
pub(super) struct Shortcuts {
    service: DesktopServices,
    wake: WakeHandle,
    registrations: RefCell<BTreeMap<u32, Registration>>,
    bus: RefCell<Option<gio::DBusConnection>>,
    signal: RefCell<Option<gio::SignalSubscription>>,
    connecting: Cell<bool>,
    cancel: gio::Cancellable,
    disposed: Cell<bool>,
}
impl Shortcuts {
    pub fn new(service: DesktopServices, wake: WakeHandle) -> Rc<Self> {
        Rc::new(Self {
            service,
            wake,
            registrations: Default::default(),
            bus: Default::default(),
            signal: Default::default(),
            connecting: Cell::new(false),
            cancel: gio::Cancellable::new(),
            disposed: Cell::new(false),
        })
    }
    pub fn register(self: &Rc<Self>, id: u32, shortcut: GlobalShortcut, app_id: Option<String>) {
        if self.disposed.get() {
            return;
        }
        self.registrations.borrow_mut().insert(
            id,
            Registration {
                shortcut,
                session: None,
                request: None,
            },
        );
        if self.connecting.get() {
            return;
        }
        if let Some(bus) = self.bus.borrow().clone() {
            if self.signal.borrow().is_none() {
                self.subscribe(&bus);
            }
            self.create(id);
            return;
        }
        if self.connecting.replace(true) {
            return;
        }
        // A dedicated connection keeps portal identity independent of other roots.
        let Some(address) = std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .ok()
            .filter(|s| !s.is_empty())
        else {
            self.failed();
            return;
        };
        let weak = Rc::downgrade(self);
        gio::DBusConnection::for_address(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            Some(&self.cancel),
            move |result| {
                let Some(this) = weak.upgrade().filter(|this| !this.disposed.get()) else {
                    return;
                };
                let Ok(bus) = result else {
                    this.failed();
                    return;
                };
                bus.set_exit_on_close(false);
                *this.bus.borrow_mut() = Some(bus.clone());
                this.subscribe(&bus);
                if let Some(app_id) = app_id {
                    let options = glib::VariantDict::new(None);
                    let weak = Rc::downgrade(&this);
                    bus.call(
                        Some(PORTAL),
                        PATH,
                        "org.freedesktop.host.portal.Registry",
                        "Register",
                        Some(&glib::Variant::tuple_from_iter([
                            app_id.to_variant(),
                            options.end(),
                        ])),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                        Some(&this.cancel),
                        move |_| {
                            if let Some(this) = weak.upgrade() {
                                this.connected();
                            }
                        },
                    );
                } else {
                    this.connected();
                }
            },
        );
    }
    fn subscribe(self: &Rc<Self>, bus: &gio::DBusConnection) {
        let weak = Rc::downgrade(self);
        *self.signal.borrow_mut() = Some(bus.subscribe_to_signal(
            Some(PORTAL),
            Some(INTERFACE),
            Some("Activated"),
            Some(PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some(this) = weak.upgrade() else { return };
                let session = signal.parameters.try_child_value(0);
                let id = signal
                    .parameters
                    .try_child_value(1)
                    .and_then(|v| v.str().and_then(|s| s.parse::<u32>().ok()));
                if let (Some(session), Some(id)) = (session, id) {
                    let matches = this
                        .registrations
                        .borrow()
                        .get(&id)
                        .is_some_and(|r| r.session.as_deref() == session.str());
                    if matches && this.service.event(id, ServiceEvent::Shortcut) {
                        this.wake.wake();
                    }
                }
            },
        ));
    }
    fn connected(self: &Rc<Self>) {
        if self.disposed.get() {
            return;
        }
        self.connecting.set(false);
        let ids: Vec<_> = self.registrations.borrow().keys().copied().collect();
        for id in ids {
            self.create(id);
        }
    }
    fn failed(&self) {
        self.connecting.set(false);
        let registrations = std::mem::take(&mut *self.registrations.borrow_mut());
        for id in registrations.keys() {
            self.reply(*id, Some(ServiceError::Unavailable));
        }
    }
    fn reply(&self, id: u32, error: Option<ServiceError>) {
        self.service.reply(id, ServiceReply::Complete { error });
        self.wake.wake();
    }
    fn create(self: &Rc<Self>, id: u32) {
        let Some(bus) = self.bus.borrow().clone() else {
            return;
        };
        if !self.registrations.borrow().contains_key(&id) {
            return;
        }
        let options = glib::VariantDict::new(None);
        options.insert("session_handle_token", token().as_str());
        let weak = Rc::downgrade(self);
        let pending = request(
            &bus,
            "CreateSession",
            |options| glib::Variant::tuple_from_iter([options]),
            options,
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                let session = result.and_then(|v| {
                    v.lookup::<String>("session_handle")
                        .ok()
                        .flatten()
                        .ok_or(ServiceError::Failed)
                });
                match session {
                    Ok(session) => {
                        if let Some(registration) = this.registrations.borrow_mut().get_mut(&id) {
                            registration.session = Some(session.clone());
                        } else {
                            if let Some(bus) = this.bus.borrow().as_ref() {
                                close_object(bus, &session, "org.freedesktop.portal.Session");
                            }
                            return;
                        }
                        this.bind(id, &session);
                    }
                    Err(error) => {
                        this.reply(id, Some(error));
                        this.unregister(id);
                    }
                }
            },
        );
        if let Some(registration) = self.registrations.borrow_mut().get_mut(&id) {
            registration.request = Some(pending);
        }
    }
    fn bind(self: &Rc<Self>, id: u32, session: &str) {
        let Some(bus) = self.bus.borrow().clone() else {
            return;
        };
        let Some(trigger) = self
            .registrations
            .borrow()
            .get(&id)
            .map(|r| trigger(&r.shortcut))
        else {
            return;
        };
        let Ok(session) = ObjectPath::try_from(session) else {
            self.reply(id, Some(ServiceError::Failed));
            self.unregister(id);
            return;
        };
        let properties = BTreeMap::from([
            ("description", trigger.to_variant()),
            ("preferred_trigger", trigger.to_variant()),
        ]);
        let shortcuts = vec![(id.to_string(), properties)];
        let weak = Rc::downgrade(self);
        let pending = request(
            &bus,
            "BindShortcuts",
            move |options| {
                glib::Variant::tuple_from_iter([
                    session.to_variant(),
                    shortcuts.to_variant(),
                    "".to_variant(),
                    options,
                ])
            },
            glib::VariantDict::new(None),
            move |result| {
                let Some(this) = weak.upgrade() else { return };
                let result = result.and_then(|v| {
                    let shortcuts = v
                        .lookup::<Vec<(String, BTreeMap<String, glib::Variant>)>>("shortcuts")
                        .ok()
                        .flatten()
                        .ok_or(ServiceError::Failed)?;
                    if shortcuts.iter().any(|(key, _)| key == &id.to_string()) {
                        Ok(())
                    } else {
                        Err(ServiceError::Conflict)
                    }
                });
                this.reply(id, result.err());
                if result.is_err() {
                    this.unregister(id);
                }
            },
        );
        if let Some(registration) = self.registrations.borrow_mut().get_mut(&id) {
            registration.request = Some(pending);
        }
    }
    pub fn unregister(&self, id: u32) {
        let removed = self.registrations.borrow_mut().remove(&id);
        if let Some(mut removed) = removed {
            if let Some(request) = removed.request.take() {
                request.finish(Err(ServiceError::Failed), true);
            }
            if let (Some(bus), Some(session)) = (self.bus.borrow().as_ref(), removed.session) {
                close_object(bus, &session, "org.freedesktop.portal.Session");
            }
        }
    }
    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.cancel.cancel();
        self.signal.borrow_mut().take();
        let ids: Vec<_> = self.registrations.borrow().keys().copied().collect();
        for id in ids {
            self.unregister(id);
        }
        if let Some(bus) = self.bus.borrow_mut().take() {
            bus.close(gio::Cancellable::NONE, |_| {});
        }
    }
}
impl Drop for Shortcuts {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn trigger(shortcut: &GlobalShortcut) -> String {
    let mut parts = Vec::new();
    if shortcut.control {
        parts.push("CTRL");
    }
    if shortcut.option {
        parts.push("ALT");
    }
    if shortcut.shift {
        parts.push("SHIFT");
    }
    if shortcut.command {
        parts.push("LOGO");
    }
    parts.push(match shortcut.key.as_str() {
        "Enter" => "Return",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "Backspace" => "BackSpace",
        "Space" => "space",
        key => key,
    });
    parts.join("+")
}

#[cfg(test)]
mod tests;
