use super::{gdk, gio, glib, gtk, WakeHandle};
use gtk::prelude::*;
use jadix_core::{clipboard::*, file_request::*};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::mpsc::{self, Receiver},
};

struct Pending {
    cancel: gio::Cancellable,
    abort: Box<dyn FnOnce()>,
    timer: Option<glib::SourceId>,
}
pub(super) struct Io {
    pending: RefCell<BTreeMap<u64, Pending>>,
    serial: Cell<u64>,
    disposed: Cell<bool>,
    wake: WakeHandle,
}
type Completion<T> = Box<dyn FnOnce(Result<T, String>)>;
impl Io {
    pub fn new(wake: WakeHandle) -> Rc<Self> {
        Rc::new(Self {
            pending: Default::default(),
            serial: Cell::new(0),
            disposed: Cell::new(false),
            wake,
        })
    }
    fn begin<T: 'static>(
        self: &Rc<Self>,
        timeout: Option<std::time::Duration>,
    ) -> Result<(gio::Cancellable, Completion<T>, Receiver<Result<T, String>>), String> {
        if self.disposed.get() || self.pending.borrow().len() >= 32 {
            return Err("native I/O unavailable or request limit reached".into());
        }
        let id = self
            .serial
            .get()
            .checked_add(1)
            .ok_or("native I/O identity exhausted")?;
        self.serial.set(id);
        let cancel = gio::Cancellable::new();
        let (send, recv) = mpsc::channel();
        let abort = send.clone();
        let weak = Rc::downgrade(self);
        let timer = timeout.map(|duration| {
            glib::timeout_add_local_once(duration, move || {
                if let Some(io) = weak.upgrade() {
                    let pending = io.pending.borrow_mut().remove(&id);
                    if let Some(p) = pending {
                        p.cancel.cancel();
                        (p.abort)();
                        io.wake.wake();
                    }
                }
            })
        });
        self.pending.borrow_mut().insert(
            id,
            Pending {
                cancel: cancel.clone(),
                abort: Box::new(move || {
                    let _ = abort.send(Err("native I/O cancelled or timed out".into()));
                }),
                timer,
            },
        );
        let weak = Rc::downgrade(self);
        let finish = Box::new(move |result| {
            if let Some(io) = weak.upgrade() {
                let pending = io.pending.borrow_mut().remove(&id);
                if let Some(p) = pending {
                    if let Some(timer) = p.timer {
                        timer.remove();
                    }
                    let _ = send.send(result);
                    io.wake.wake();
                }
            }
        });
        Ok((cancel, finish, recv))
    }
    pub fn file(
        self: &Rc<Self>,
        parent: &gtk::Window,
        request: &FileRequest,
    ) -> Result<Receiver<FileResult>, String> {
        request.validate()?;
        // GTK's chooser has separate file and directory modes.
        if matches!(
            request,
            FileRequest::PickPaths {
                selection: FileSelection::FilesAndFolders,
                ..
            }
        ) {
            return Err("GTK requires choosing files or folders separately".into());
        }
        let (cancel, finish, recv) = self.begin::<Vec<std::path::PathBuf>>(None)?;
        match request {
            FileRequest::PickPaths {
                selection,
                multiple,
                options,
            } => {
                let dialog = panel(options);
                if *multiple {
                    let done = move |result: Result<gio::ListModel, glib::Error>| {
                        finish(picker_result(result.and_then(|model| {
                            if model.n_items() > 64 {
                                return Err(glib::Error::new(
                                    gio::IOErrorEnum::NoSpace,
                                    "too many selected files",
                                ));
                            }
                            (0..model.n_items())
                                .map(|i| {
                                    model
                                        .item(i)
                                        .and_downcast::<gio::File>()
                                        .and_then(|f| f.path())
                                        .ok_or_else(|| {
                                            glib::Error::new(
                                                gio::IOErrorEnum::NotSupported,
                                                "selection is not a local file",
                                            )
                                        })
                                })
                                .collect()
                        })));
                    };
                    if *selection == FileSelection::Folders {
                        dialog.select_multiple_folders(Some(parent), Some(&cancel), done);
                    } else {
                        dialog.open_multiple(Some(parent), Some(&cancel), done);
                    }
                } else {
                    let done = move |result: Result<gio::File, glib::Error>| {
                        finish(picker_result(result.and_then(local_path).map(|p| vec![p])))
                    };
                    if *selection == FileSelection::Folders {
                        dialog.select_folder(Some(parent), Some(&cancel), done);
                    } else {
                        dialog.open(Some(parent), Some(&cancel), done);
                    }
                }
            }
            FileRequest::PickSavePath { file_name, options } => {
                let dialog = panel(options);
                dialog.set_initial_name(file_name.as_deref());
                dialog.save(Some(parent), Some(&cancel), move |r| {
                    finish(picker_result(r.and_then(local_path).map(|p| vec![p])))
                });
            }
            FileRequest::Open { path } | FileRequest::Reveal { path } => {
                let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(path)));
                let path = path.clone();
                let done = move |r: Result<(), glib::Error>| {
                    finish(r.map(|_| vec![path]).map_err(|e| e.to_string()))
                };
                if matches!(request, FileRequest::Reveal { .. }) {
                    launcher.open_containing_folder(Some(parent), Some(&cancel), done);
                } else {
                    launcher.launch(Some(parent), Some(&cancel), done);
                }
            }
            FileRequest::Trash { path } => {
                let output = path.clone();
                gio::File::for_path(path).trash_async(
                    glib::Priority::DEFAULT,
                    Some(&cancel),
                    move |r| finish(r.map(|_| vec![output]).map_err(|e| e.to_string())),
                );
            }
        }
        Ok(recv)
    }
    pub fn clipboard(
        self: &Rc<Self>,
        display: &gdk::Display,
        request: &ClipboardRequest,
    ) -> Result<Receiver<ClipboardResult>, String> {
        request.validate()?;
        let clipboard = display.clipboard();
        let (cancel, finish, recv) =
            self.begin::<ClipboardResponse>(Some(std::time::Duration::from_secs(5)))?;
        match request {
            ClipboardRequest::Clear => finish(
                clipboard
                    .set_content(gdk::ContentProvider::NONE)
                    .map(|_| ClipboardResponse::Complete)
                    .map_err(|e| e.to_string()),
            ),
            ClipboardRequest::Formats => finish(Ok(ClipboardResponse::Formats(
                clipboard
                    .formats()
                    .mime_types()
                    .iter()
                    .take(256)
                    .map(|s| s.to_string())
                    .collect(),
            ))),
            ClipboardRequest::Write { items } => {
                let providers: Vec<_> = items
                    .iter()
                    .map(|i| {
                        gdk::ContentProvider::for_bytes(
                            &i.mime,
                            &glib::Bytes::from_owned(i.bytes.clone()),
                        )
                    })
                    .collect();
                finish(
                    clipboard
                        .set_content(Some(&gdk::ContentProvider::new_union(&providers)))
                        .map(|_| ClipboardResponse::Complete)
                        .map_err(|e| e.to_string()),
                );
            }
            ClipboardRequest::Read { mime_types } => {
                let available = clipboard.formats();
                if !mime_types.iter().any(|m| available.contain_mime_type(m)) {
                    finish(Ok(ClipboardResponse::Item(None)));
                } else {
                    let names: Vec<_> = mime_types.iter().map(String::as_str).collect();
                    let read_cancel = cancel.clone();
                    clipboard.read_async(
                        &names,
                        glib::Priority::DEFAULT,
                        Some(&cancel),
                        move |r| match r {
                            Err(e) => finish(Err(e.to_string())),
                            Ok((stream, mime)) => read_clipboard(
                                stream,
                                read_cancel,
                                ClipboardItem {
                                    mime: mime.to_string(),
                                    bytes: Vec::new(),
                                },
                                finish,
                            ),
                        },
                    );
                }
            }
        }
        Ok(recv)
    }
    pub fn dispose(&self) {
        if self.disposed.replace(true) {
            return;
        }
        let requests = std::mem::take(&mut *self.pending.borrow_mut());
        for (_, p) in requests {
            p.cancel.cancel();
            if let Some(timer) = p.timer {
                timer.remove();
            }
            (p.abort)();
        }
    }
}
impl Drop for Io {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn read_clipboard(
    stream: gio::InputStream,
    cancel: gio::Cancellable,
    mut item: ClipboardItem,
    finish: Completion<ClipboardResponse>,
) {
    let next = stream.clone();
    let next_cancel = cancel.clone();
    stream.read_bytes_async(
        (MAX_BYTES + 1 - item.bytes.len()).min(64 * 1024),
        glib::Priority::DEFAULT,
        Some(&cancel),
        move |result| match result {
            Err(e) => finish(Err(e.to_string())),
            Ok(bytes) if bytes.is_empty() => finish(Ok(ClipboardResponse::Item(Some(item)))),
            Ok(bytes) => {
                item.bytes.extend_from_slice(&bytes);
                if item.bytes.len() > MAX_BYTES {
                    finish(Err("clipboard payload too large".into()));
                } else {
                    read_clipboard(next, next_cancel, item, finish);
                }
            }
        },
    );
}
fn panel(options: &FilePanelOptions) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder().modal(true).build();
    if let Some(title) = &options.title {
        dialog.set_title(title);
    }
    dialog.set_accept_label(options.prompt.as_deref());
    if let Some(directory) = &options.directory {
        dialog.set_initial_folder(Some(&gio::File::for_path(directory)));
    }
    if !options.content_types.is_empty() {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let filter = gtk::FileFilter::new();
        for kind in &options.content_types {
            if kind.contains('/') {
                filter.add_mime_type(kind);
            } else {
                filter.add_suffix(kind.trim_start_matches('.'));
            }
        }
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
    }
    dialog
}
fn local_path(file: gio::File) -> Result<std::path::PathBuf, glib::Error> {
    file.path().ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::NotSupported,
            "selection is not a local file",
        )
    })
}
fn picker_result(result: Result<Vec<std::path::PathBuf>, glib::Error>) -> FileResult {
    match result {
        Err(e) if e.matches(gtk::DialogError::Dismissed) => Ok(Vec::new()),
        r => r.map_err(|e| e.to_string()),
    }
}
