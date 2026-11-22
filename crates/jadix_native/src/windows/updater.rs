//! Windows adapter for the shared update worker and installation transaction.
use super::*;
use crate::update_worker::{Archive, Work, Reply, Worker};
use jadix_core::updater::{
    self as shared,
    installation::{Installation, Staged},
    Command, Event, Phase, Updater,
};
use std::{
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub(super) struct Runtime {
    pub service: Updater,
    worker: RefCell<Worker>,
    cancel: RefCell<Option<(u32, Arc<AtomicBool>)>>,
    archive: RefCell<Option<Archive>>,
    staged: RefCell<Option<Staged>>,
    scheduled: Cell<bool>,
    disposed: Cell<bool>,
    application_id: String,
    window: usize,
}
impl Runtime {
    pub fn new(
        config: shared::Config,
        installation: Installation,
        window: usize,
        curl: String,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Rc<Self>, String> {
        let platform = format!("windows-{}", std::env::consts::ARCH);
        let service = Updater::new(Some(config.clone()), platform, installation.can_install())?;
        let wake = Arc::new(wake);
        let signal = wake.clone();
        service.set_command_waker(Some(Rc::new(move || signal())));
        let worker = Worker::new(installation, curl, move || wake())?;
        Ok(Rc::new(Self {
            service,
            worker: RefCell::new(worker),
            cancel: Default::default(),
            archive: Default::default(),
            staged: Default::default(),
            scheduled: Cell::new(false),
            disposed: Cell::new(false),
            application_id: config.id,
            window,
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
                        match installer::launch(staged, true) {
                            Ok(()) => unsafe { PostMessageW(self.window as HWND, WM_CLOSE, 0, 0); },
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
                if let Err(error) = installer::launch(staged, false) {
                    log::error!("scheduled Windows update: {error}");
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

mod installer;
pub use installer::run_update_helper;
