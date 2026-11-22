//! Application registrations are leases: removing a root removes its actions,
//! notification IDs and pending browser callbacks from the shared GApplication.
use super::*;
use jadix_core::{deep_link::*, desktop_services::*};

pub(super) struct Services {
    pub service: DesktopServices,
    links: RefCell<DeepLinkRouter>,
    application: RefCell<Option<(gtk::Application, glib::SignalHandlerId)>>,
    notifications: RefCell<BTreeMap<u32, (String, String)>>,
    auth: RefCell<BTreeMap<u32, (String, gio::Cancellable)>>,
    prefix: String,
    wake: WakeHandle,
    disposed: Cell<bool>,
    shortcuts: Rc<shortcuts::Shortcuts>,
}
impl Services {
    pub fn new(service: DesktopServices, wake: WakeHandle) -> Rc<Self> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let shortcuts = shortcuts::Shortcuts::new(service.clone(), wake.clone());
        Rc::new(Self {
            service,
            shortcuts,
            links: RefCell::new(DeepLinkRouter::new(Default::default()).unwrap()),
            application: Default::default(),
            notifications: Default::default(),
            auth: Default::default(),
            prefix: format!(
                "jadix-root-{}",
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            wake,
            disposed: Cell::new(false),
        })
    }
    pub fn bind(self: &Rc<Self>, application: Option<&gtk::Application>) {
        if self.application.borrow().as_ref().map(|(app, _)| app) == application {
            return;
        }
        self.detach();
        if let Some(application) = application {
            let weak = Rc::downgrade(self);
            let signal = application.connect_open(move |_, files, _| {
                let Some(this) = weak.upgrade() else { return };
                for file in files.iter().take(MAX_DEEP_LINK_QUEUE) {
                    let _ = this.deliver(DeepLink {
                        url: file.uri().into(),
                        source: DeepLinkSource::HotStart,
                    });
                }
            });
            *self.application.borrow_mut() = Some((application.clone(), signal));
        }
    }
    pub fn configure(&self, config: DeepLinkConfig) -> Result<(), String> {
        config.validate().map_err(str::to_owned)?;
        if !config.schemes.is_empty() {
            let binding = self.application.borrow();
            let Some((app, _)) = binding.as_ref() else {
                return Err("deep links require a GApplication".into());
            };
            if !app.flags().contains(gio::ApplicationFlags::HANDLES_OPEN) {
                return Err("create GApplication with HANDLES_OPEN before registration".into());
            }
            if config.single_instance == app.flags().contains(gio::ApplicationFlags::NON_UNIQUE) {
                return Err(
                    "deep link single-instance policy differs from GApplication registration"
                        .into(),
                );
            }
        }
        self.links
            .borrow_mut()
            .configure(config)
            .map_err(str::to_owned)
    }
    pub fn deliver(&self, link: DeepLink) -> Result<(), String> {
        if self.disposed.get() {
            return Err("GTK service root is disposed".into());
        }
        // Authentication callbacks may use a scheme independently of app routing.
        let session = self.auth.borrow().iter().find_map(|(&id, (scheme, _))| {
            link.validate(&std::collections::BTreeSet::from([scheme.clone()]))
                .is_ok()
                .then_some(id)
        });
        if let Some(id) = session {
            if let Some((_, cancel)) = self.auth.borrow_mut().remove(&id) {
                cancel.cancel();
            }
            self.service.reply(
                id,
                ServiceReply::AuthSession {
                    outcome: Some(AuthSessionOutcome::Completed { url: link.url }),
                    error: None,
                },
            );
        } else {
            self.links
                .borrow_mut()
                .accept(link)
                .map_err(str::to_owned)?;
        }
        self.wake.wake();
        Ok(())
    }
    pub fn drain(&self) -> Vec<DeepLink> {
        self.links.borrow_mut().drain()
    }
    pub fn pump(self: &Rc<Self>) {
        if self.disposed.get() {
            return;
        }
        for command in self.service.take_commands() {
            let id = command.id();
            match command {
                ServiceCommand::Notify { notification, .. } => {
                    let application = self
                        .application
                        .borrow()
                        .as_ref()
                        .map(|(app, _)| app.clone());
                    let Some(app) =
                        application.filter(|app| app.is_registered() && !app.is_remote())
                    else {
                        self.service.reply(
                            id,
                            ServiceReply::Complete {
                                error: Some(ServiceError::Unavailable),
                            },
                        );
                        continue;
                    };
                    let name = format!("{}-{id}", self.prefix);
                    let action = gio::SimpleAction::new(&name, None);
                    let weak = Rc::downgrade(self);
                    let data = notification.data;
                    let identifier = name.clone();
                    action.connect_activate(move |_, _| {
                        if let Some(this) = weak.upgrade() {
                            if this.service.event(id, ServiceEvent::Click) {
                                this.service.activate(NotificationActivation {
                                    identifier: identifier.clone(),
                                    data: data.clone(),
                                });
                                this.wake.wake();
                            }
                        }
                    });
                    app.add_action(&action);
                    let message = gio::Notification::new(&notification.title);
                    message.set_body(Some(&format!(
                        "{}{}{}",
                        notification.subtitle,
                        if notification.subtitle.is_empty() {
                            ""
                        } else {
                            "\n"
                        },
                        notification.body
                    )));
                    message.set_default_action(&format!("app.{name}"));
                    app.send_notification(Some(&name), &message);
                    self.notifications
                        .borrow_mut()
                        .insert(id, (name.clone(), name));
                    self.service
                        .reply(id, ServiceReply::Complete { error: None });
                }
                ServiceCommand::CancelNotification { .. } => self.cancel_notification(id),
                ServiceCommand::OpenAuthSession { request, .. } => {
                    let binding = self.application.borrow();
                    let available = binding.as_ref().is_some_and(|(app, _)| {
                        app.flags().contains(gio::ApplicationFlags::HANDLES_OPEN)
                    });
                    if request.ephemeral || !available {
                        self.service.reply(
                            id,
                            ServiceReply::AuthSession {
                                outcome: None,
                                error: Some(ServiceError::Unavailable),
                            },
                        );
                        continue;
                    }
                    drop(binding);
                    let cancel = gio::Cancellable::new();
                    self.auth.borrow_mut().insert(
                        id,
                        (request.callback_scheme.to_ascii_lowercase(), cancel.clone()),
                    );
                    let weak = Rc::downgrade(self);
                    gio::AppInfo::launch_default_for_uri_async(
                        &request.url,
                        None::<&gio::AppLaunchContext>,
                        Some(&cancel),
                        move |result| {
                            let Some(this) = weak.upgrade() else { return };
                            if result.is_err() && this.auth.borrow_mut().remove(&id).is_some() {
                                this.service.reply(
                                    id,
                                    ServiceReply::AuthSession {
                                        outcome: None,
                                        error: Some(ServiceError::Failed),
                                    },
                                );
                                this.wake.wake();
                            }
                        },
                    );
                }
                ServiceCommand::CloseAuthSession { .. } => {
                    if let Some((_, cancel)) = self.auth.borrow_mut().remove(&id) {
                        cancel.cancel();
                    }
                }
                ServiceCommand::Permission { .. } => {
                    self.service.reply(
                        id,
                        ServiceReply::Permission {
                            authorization: NotificationAuthorization::Unknown,
                            error: Some(ServiceError::Unavailable),
                        },
                    );
                }
                ServiceCommand::RegisterShortcut { shortcut, .. } => {
                    let app_id = self
                        .application
                        .borrow()
                        .as_ref()
                        .and_then(|(app, _)| app.application_id())
                        .map(|v| v.to_string());
                    self.shortcuts.register(id, shortcut, app_id);
                }
                ServiceCommand::UnregisterShortcut { .. } => self.shortcuts.unregister(id),
                _ => {
                    self.service.reply(
                        id,
                        ServiceReply::Complete {
                            error: Some(ServiceError::Unavailable),
                        },
                    );
                }
            }
        }
    }
    fn cancel_notification(&self, id: u32) {
        if let Some((identifier, action)) = self.notifications.borrow_mut().remove(&id) {
            if let Some((app, _)) = self.application.borrow().as_ref() {
                app.withdraw_notification(&identifier);
                app.remove_action(&action);
            }
        }
    }
    fn detach(&self) {
        let ids: Vec<_> = self.notifications.borrow().keys().copied().collect();
        for id in ids {
            self.cancel_notification(id);
        }
        for (id, (_, cancel)) in std::mem::take(&mut *self.auth.borrow_mut()) {
            cancel.cancel();
            self.service.reply(
                id,
                ServiceReply::AuthSession {
                    outcome: None,
                    error: Some(ServiceError::Unavailable),
                },
            );
        }
        if let Some((app, signal)) = self.application.borrow_mut().take() {
            app.disconnect(signal);
        }
    }
    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.shortcuts.dispose();
        self.detach();
        self.links.borrow_mut().drain();
        self.service.dispose();
    }
}
impl Drop for Services {
    fn drop(&mut self) {
        self.dispose();
    }
}
