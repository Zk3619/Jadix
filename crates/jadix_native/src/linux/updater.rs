//! One application update worker. No HTTP, hashing or archive extraction runs
//! on GTK's thread; only the final same-filesystem rename transaction does.
use super::*;
use crate::update_worker::{Archive, Work, Reply, Worker};
use jadix_core::updater::{
    self as shared,
    installation::{Installation, Staged},
    Command, Event, Phase, Updater,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

const RESTART_PARENT: &str = "JADIX_LINUX_UPDATE_PARENT";
fn process_identity(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm may contain spaces and parentheses; starttime is field 22.
    Some(
        stat.rsplit_once(") ")?
            .1
            .split_whitespace()
            .nth(19)?
            .to_owned(),
    )
}
/// Call before registering GApplication when it is created before GtkHost.
/// An update replacement waits for the previous process to release its bus name.
pub fn wait_for_update_parent() -> Result<(), String> {
    let Some(parent) = std::env::var_os(RESTART_PARENT) else {
        return Ok(());
    };
    let parent = parent.to_str().ok_or("invalid updater parent identity")?;
    let (pid, start) = parent
        .split_once(':')
        .ok_or("invalid updater parent identity")?;
    let pid = pid
        .parse::<u32>()
        .map_err(|_| "invalid updater parent PID")?;
    if pid == std::process::id() {
        return Err("updater cannot wait for itself".into());
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while process_identity(pid).as_deref() == Some(start) {
        if std::time::Instant::now() >= deadline {
            return Err("previous application did not exit after update".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

pub(super) struct Runtime {
    pub service: Updater,
    worker: RefCell<Worker>,
    cancel: RefCell<Option<(u32, Arc<AtomicBool>)>>,
    archive: RefCell<Option<Archive>>,
    staged: RefCell<Option<Staged>>,
    scheduled: Cell<bool>,
    disposed: Cell<bool>,
    application_id: String,
    window: glib::WeakRef<gtk::Window>,
}
impl Runtime {
    pub fn new(
        config: shared::Config,
        installation: Installation,
        window: &gtk::Window,
        wake: WakeHandle,
    ) -> Result<Rc<Self>, String> {
        let platform = format!("linux-{}", std::env::consts::ARCH);
        let service = Updater::new(Some(config.clone()), platform, installation.can_install())?;
        let signal = wake.clone();
        service.set_command_waker(Some(Rc::new(move || signal.wake())));
        let worker = Worker::new(installation, "curl", move || wake.wake())?;
        Ok(Rc::new(Self {
            service,
            worker: RefCell::new(worker),
            cancel: Default::default(),
            archive: Default::default(),
            staged: Default::default(),
            scheduled: Cell::new(false),
            disposed: Cell::new(false),
            application_id: config.id,
            window: window.downgrade(),
        }))
    }
    fn fail(&self, id: u32, message: impl Into<String>) {
        self.service.event(
            id,
            Event::Failed {
                message: message.into(),
            },
        );
    }
    fn cancel(&self, id: u32) {
        if self
            .cancel
            .borrow()
            .as_ref()
            .is_some_and(|(active, _)| *active == id)
        {
            if let Some((_, flag)) = self.cancel.borrow_mut().take() {
                flag.store(true, Ordering::Release);
            }
        }
        self.archive.borrow_mut().take();
        self.staged.borrow_mut().take();
        self.scheduled.set(false);
    }
    fn enqueue(&self, id: u32, work: Work) {
        let cancel = {
            let mut active = self.cancel.borrow_mut();
            if active.as_ref().is_none_or(|(previous, _)| *previous != id) {
                if let Some((_, old)) = active.take() {
                    old.store(true, Ordering::Release);
                }
                *active = Some((id, Arc::new(AtomicBool::new(false))));
            }
            active.as_ref().unwrap().1.clone()
        };
        if let Err(error) = self.worker.borrow().enqueue(id, work, cancel) { self.fail(id, error); }
    }
    pub fn pump(&self) {
        if self.disposed.get() {
            return;
        }
        let replies = self.worker.borrow().replies();
        for reply in replies {
            match reply {
                Reply::Event(id, event) => {
                    self.service.event(id, event);
                }
                Reply::Chunk(id, bytes) => {
                    self.service.chunk(id, &bytes);
                }
                Reply::Archive(id, archive) => {
                    if self.service.snapshot().generation == id
                        && self.service.snapshot().phase == Phase::Downloading
                    {
                        *self.archive.borrow_mut() = Some(archive);
                        self.service.event(id, Event::Downloaded);
                    }
                }
                Reply::Staged(id, staged) => {
                    if self.service.snapshot().generation == id
                        && self.service.snapshot().phase == Phase::Staging
                    {
                        *self.staged.borrow_mut() = Some(staged);
                        self.service.event(id, Event::Staged);
                    }
                }
            }
        }
        for command in self.service.take_commands() {
            match command {
                Command::Cancel { id } => self.cancel(id),
                Command::Manifest {
                    id,
                    url,
                    signature_url,
                } => self.enqueue(id, Work::Manifest { url, signature_url }),
                Command::Notes { id, file } => self.enqueue(id, Work::Notes(file)),
                Command::Download { id, url, max_bytes } => {
                    self.enqueue(id, Work::Download { url, max_bytes })
                }
                Command::Stage {
                    id,
                    application_id,
                    version,
                } => {
                    let archive = self.archive.borrow_mut().take();
                    if application_id != self.application_id {
                        self.fail(id, "update application identity mismatch");
                    } else if let Some(archive) = archive {
                        self.enqueue(id, Work::Stage { archive, version });
                    } else {
                        self.fail(id, "verified update archive missing");
                    }
                }
                Command::Install { id, restart: false } => {
                    self.scheduled.set(true);
                    self.service.event(id, Event::Scheduled);
                }
                Command::Install { id, restart: true } => {
                    self.scheduled.set(false);
                    if let Some(staged) = self.staged.borrow_mut().take() {
                        match staged.commit(|executable| {
                            let pid = std::process::id();
                            let start =
                                process_identity(pid).ok_or("cannot identify updater process")?;
                            std::process::Command::new(executable)
                                .args(std::env::args_os().skip(1))
                                .env(RESTART_PARENT, format!("{pid}:{start}"))
                                .spawn()
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }) {
                            Ok(_) => {
                                if let Some(window) = self.window.upgrade() {
                                    if let Some(app) = window.application() {
                                        app.quit();
                                    }
                                    window.destroy();
                                }
                            }
                            Err(error) => self.fail(id, error),
                        }
                    } else {
                        self.fail(id, "staged update missing");
                    }
                }
            }
        }
    }
    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        self.service.set_command_waker(None);
        if let Some((_, cancel)) = self.cancel.borrow_mut().take() {
            cancel.store(true, Ordering::Release);
        }
        self.worker.borrow_mut().dispose();
        if self.scheduled.replace(false) {
            if let Some(staged) = self.staged.borrow_mut().take() {
                if let Err(error) = staged.commit(|_| Ok(())) {
                    log::error!("scheduled Linux update: {error}");
                }
            }
        }
        self.archive.borrow_mut().take();
        self.staged.borrow_mut().take();
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.dispose();
    }
}
