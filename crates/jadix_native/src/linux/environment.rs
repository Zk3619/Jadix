//! Optional portal appearance, independent from GNOME and GTK theme names.
use super::*;
use glib::variant::ToVariant;
use jadix_core::environment::ColorScheme;
const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";

pub(super) struct Preferences {
    pub scheme: Cell<Option<ColorScheme>>,
    pub resume: ResumeCounter,
    cancel: gio::Cancellable,
    signals: RefCell<Vec<gio::SignalSubscription>>,
    watcher: RefCell<Option<gio::WatcherId>>,
    wake: WakeHandle,
}
impl Preferences {
    pub fn new(wake: WakeHandle) -> Rc<Self> {
        let this = Rc::new(Self {
            resume: ResumeCounter::default(),
            scheme: Cell::new(None),
            cancel: gio::Cancellable::new(),
            signals: Default::default(),
            watcher: Default::default(),
            wake,
        });
        let weak = Rc::downgrade(&this);
        gio::bus_get(gio::BusType::System, Some(&this.cancel), move |result| {
            let Some(this) = weak.upgrade().filter(|this| !this.cancel.is_cancelled()) else {
                return;
            };
            let Ok(bus) = result else { return };
            let weak = Rc::downgrade(&this);
            this.signals.borrow_mut().push(bus.subscribe_to_signal(
                Some("org.freedesktop.login1"),
                Some("org.freedesktop.login1.Manager"),
                Some("PrepareForSleep"),
                Some("/org/freedesktop/login1"),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(this) = weak.upgrade() {
                        if let Some((sleeping,)) = signal.parameters.get::<(bool,)>() {
                            if this.resume.observe(sleeping) {
                                this.wake.wake();
                            }
                        }
                    }
                },
            ));
        });
        let weak = Rc::downgrade(&this);
        gio::bus_get(gio::BusType::Session, Some(&this.cancel), move |result| {
            let Some(this) = weak.upgrade().filter(|this| !this.cancel.is_cancelled()) else {
                return;
            };
            let Ok(bus) = result else { return };
            let weak = Rc::downgrade(&this);
            this.signals.borrow_mut().push(bus.subscribe_to_signal(
                Some(PORTAL),
                Some(SETTINGS),
                Some("SettingChanged"),
                Some(PATH),
                None,
                gio::DBusSignalFlags::NONE,
                move |signal| {
                    if let Some(this) = weak.upgrade() {
                        if let Some((namespace, key, value)) =
                            signal.parameters.get::<(String, String, glib::Variant)>()
                        {
                            if namespace == APPEARANCE && key == "color-scheme" {
                                this.update(&value);
                            }
                        }
                    }
                },
            ));
            let weak = Rc::downgrade(&this);
            let gone = weak.clone();
            let watcher = gio::bus_watch_name_on_connection(
                &bus,
                PORTAL,
                gio::BusNameWatcherFlags::NONE,
                move |bus, _, _| {
                    let Some(this) = weak.upgrade() else { return };
                    let weak = Rc::downgrade(&this);
                    bus.call(
                        Some(PORTAL),
                        PATH,
                        SETTINGS,
                        "Read",
                        Some(&(APPEARANCE, "color-scheme").to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        3000,
                        Some(&this.cancel),
                        move |result| {
                            if let Some(this) = weak.upgrade() {
                                if let Ok(value) = result {
                                    if let Some(value) =
                                        value.try_child_value(0).and_then(|v| v.as_variant())
                                    {
                                        this.update(&value);
                                    }
                                }
                            }
                        },
                    );
                },
                move |_, _| {
                    if let Some(this) = gone.upgrade() {
                        this.scheme.set(None);
                        this.wake.wake();
                    }
                },
            );
            *this.watcher.borrow_mut() = Some(watcher);
        });
        this
    }
    fn update(&self, value: &glib::Variant) {
        let value = value.as_variant().unwrap_or_else(|| value.clone());
        let scheme = match value.get::<u32>() {
            Some(1) => Some(ColorScheme::Dark),
            Some(2) => Some(ColorScheme::Light),
            _ => None,
        };
        if self.scheme.replace(scheme) != scheme {
            self.wake.wake();
        }
    }
    pub fn dispose(&self) {
        self.cancel.cancel();
        self.signals.borrow_mut().clear();
        if let Some(watcher) = self.watcher.borrow_mut().take() {
            gio::bus_unwatch_name(watcher);
        }
    }
}
impl Drop for Preferences {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[derive(Default)]
pub(super) struct ResumeCounter {
    sleeping: Cell<bool>,
    pub count: Cell<u64>,
}
impl ResumeCounter {
    fn observe(&self, sleeping: bool) -> bool {
        if self.sleeping.replace(sleeping) && !sleeping {
            self.count.set(self.count.get().saturating_add(1));
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn resumes_are_counted_once_per_sleep_cycle() {
        let counter = super::ResumeCounter::default();
        assert!(!counter.observe(false));
        assert!(!counter.observe(true));
        assert!(!counter.observe(true));
        assert!(counter.observe(false));
        assert!(!counter.observe(false));
        assert_eq!(counter.count.get(), 1);
        counter.observe(true);
        assert!(counter.observe(false));
        assert_eq!(counter.count.get(), 2);
    }
}
