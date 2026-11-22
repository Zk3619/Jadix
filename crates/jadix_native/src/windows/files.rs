//! A single bounded STA worker owns Shell COM objects and modal file panels.
//! Its cancellation timer runs only inside Show's message loop; root disposal
//! closes the panel on its own apartment before joining and destroying HWNDs.
use super::*;
use jadix_core::file_request::{FilePanelOptions, FileRequest, FileResult, FileSelection};
use std::{os::windows::ffi::{OsStrExt, OsStringExt}, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver, SyncSender}}, thread::JoinHandle};
use windows::{core::{Interface, PCWSTR, HRESULT}, Win32::{Foundation::HWND as ComHwnd, System::Com::*, UI::Shell::*}};
const CANCELLED: HRESULT = HRESULT(0x800704c7u32 as i32);
type Job = (FileRequest, SyncSender<FileResult>);
thread_local! { static PANEL: RefCell<Option<(IFileDialog, Arc<AtomicBool>)>> = const { RefCell::new(None) }; }
unsafe extern "system" fn cancel_panel(_: HWND, _: u32, _: usize, _: u32) {
    let dialog = PANEL.with(|slot| slot.borrow().as_ref().filter(|(_, cancelled)| cancelled.load(Ordering::Acquire)).map(|(dialog, _)| dialog.clone()));
    if let Some(dialog) = dialog { let _ = unsafe { dialog.Close(CANCELLED) }; }
}
struct Apartment;
impl Drop for Apartment { fn drop(&mut self) { unsafe { CoUninitialize(); } } }
pub(super) struct Files {
    jobs: Option<SyncSender<Job>>,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Files {
    pub fn new(owner: usize, wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let (send, receive) = mpsc::sync_channel::<Job>(8);
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let worker = std::thread::Builder::new().name("jadix-windows-files".into()).spawn(move || {
            let apartment = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok().map(|_| Apartment);
            while let Ok((request, reply)) = receive.recv() {
                let result = if stop.load(Ordering::Acquire) { Err("file request cancelled: host disposed".into()) }
                    else if apartment.is_err() { Err("file worker could not initialize COM".into()) }
                    else { execute(owner, &request, stop.clone()) };
                let _ = reply.send(result);
                if !stop.load(Ordering::Acquire) { wake(); }
            }
        }).map_err(|e| e.to_string())?;
        Ok(Self { jobs: Some(send), cancelled, worker: Some(worker) })
    }
    pub fn request(&self, request: &FileRequest) -> Result<Receiver<FileResult>, String> {
        request.validate()?;
        if self.cancelled.load(Ordering::Acquire) { return Err("file service disposed".into()); }
        let (reply, result) = mpsc::sync_channel(1);
        self.jobs.as_ref().ok_or("file service disposed")?.try_send((request.clone(), reply)).map_err(|_| "file request queue is full or closed")?;
        Ok(result)
    }
    pub fn dispose(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.jobs.take();
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}
impl Drop for Files { fn drop(&mut self) { self.dispose(); } }
fn path_wide(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain(Some(0)).collect() }
fn item_path(item: &IShellItem) -> Result<PathBuf, String> {
    let path = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|e| e.to_string())?;
    let result = unsafe { path.as_wide() }.to_vec();
    unsafe { CoTaskMemFree(Some(path.0.cast())); }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(&result)))
}
fn configure(dialog: &IFileDialog, options: &FilePanelOptions) -> windows::core::Result<()> {
    unsafe {
        if let Some(title) = &options.title { dialog.SetTitle(PCWSTR(wide(title).as_ptr()))?; }
        if let Some(prompt) = &options.prompt { dialog.SetOkButtonLabel(PCWSTR(wide(prompt).as_ptr()))?; }
        if let Some(directory) = &options.directory {
            let folder: IShellItem = SHCreateItemFromParsingName(PCWSTR(path_wide(directory).as_ptr()), None)?;
            dialog.SetFolder(&folder)?;
        }
        let patterns: Vec<Vec<u16>> = options.content_types.iter().map(|kind| {
            let extension = match kind.as_str() {
                "text/plain" => "txt", "text/html" => "html;*.htm", "application/json" => "json",
                "application/pdf" => "pdf", "image/png" => "png", "image/jpeg" => "jpg;*.jpeg", "image/svg+xml" => "svg",
                "image/*" => "png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.svg", "audio/*" => "wav;*.mp3;*.m4a;*.ogg;*.flac", "video/*" => "mp4;*.webm;*.mov;*.mkv",
                _ => kind.trim_start_matches("*.").trim_start_matches('.'),
            };
            wide(&format!("*.{extension}"))
        }).collect();
        if patterns.iter().any(|p| p.contains(&(b'/' as u16)) || p.contains(&(b'\\' as u16))) { return Err(windows::core::Error::from_hresult(HRESULT(0x80070057u32 as i32))); }
        let names: Vec<Vec<u16>> = options.content_types.iter().map(|name| wide(name)).collect();
        let filters: Vec<Common::COMDLG_FILTERSPEC> = names.iter().zip(patterns.iter()).map(|(name, pattern)| Common::COMDLG_FILTERSPEC { pszName: PCWSTR(name.as_ptr()), pszSpec: PCWSTR(pattern.as_ptr()) }).collect();
        if !filters.is_empty() { dialog.SetFileTypes(&filters)?; }
    }
    Ok(())
}
fn show(dialog: &IFileDialog, owner: usize, cancelled: Arc<AtomicBool>) -> Result<bool, String> {
    if cancelled.load(Ordering::Acquire) { return Err("file request cancelled: host disposed".into()); }
    PANEL.with(|slot| *slot.borrow_mut() = Some((dialog.clone(), cancelled)));
    let timer = unsafe { SetTimer(null_mut(), 0, 50, Some(cancel_panel)) };
    if timer == 0 { PANEL.with(|slot| slot.borrow_mut().take()); return Err(native_error("file cancellation timer")); }
    let result = unsafe { dialog.Show(Some(ComHwnd(owner as *mut c_void))) };
    unsafe { KillTimer(null_mut(), timer); }
    PANEL.with(|slot| slot.borrow_mut().take());
    match result { Ok(()) => Ok(true), Err(error) if error.code() == CANCELLED => Ok(false), Err(error) => Err(error.to_string()) }
}
fn execute(owner: usize, request: &FileRequest, cancelled: Arc<AtomicBool>) -> FileResult {
    unsafe {
        match request {
            FileRequest::PickPaths { selection, multiple, options } => {
                if *selection == FileSelection::FilesAndFolders { return Err("Windows common dialog requires a files or folders selection mode".into()); }
                let open: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
                let dialog: IFileDialog = open.cast().map_err(|e| e.to_string())?;
                let mut flags = FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST | FOS_NOCHANGEDIR;
                if *multiple { flags |= FOS_ALLOWMULTISELECT; }
                if *selection == FileSelection::Folders { flags |= FOS_PICKFOLDERS; }
                dialog.SetOptions(flags).map_err(|e| e.to_string())?;
                configure(&dialog, options).map_err(|e| e.to_string())?;
                if !show(&dialog, owner, cancelled)? { return Ok(Vec::new()); }
                let items = open.GetResults().map_err(|e| e.to_string())?;
                let count = items.GetCount().map_err(|e| e.to_string())?;
                if count > 4096 { return Err("too many selected paths".into()); }
                (0..count).map(|i| item_path(&items.GetItemAt(i).map_err(|e| e.to_string())?)).collect()
            }
            FileRequest::PickSavePath { file_name, options } => {
                let dialog: IFileDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
                dialog.SetOptions(FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_OVERWRITEPROMPT | FOS_NOCHANGEDIR).map_err(|e| e.to_string())?;
                configure(&dialog, options).map_err(|e| e.to_string())?;
                if let Some(name) = file_name { dialog.SetFileName(PCWSTR(wide(name).as_ptr())).map_err(|e| e.to_string())?; }
                if !show(&dialog, owner, cancelled)? { return Ok(Vec::new()); }
                Ok(vec![item_path(&dialog.GetResult().map_err(|e| e.to_string())?)?])
            }
            FileRequest::Open { path } => { open(owner, path_wide(path).as_ptr())?; Ok(vec![path.clone()]) }
            FileRequest::Reveal { path } => {
                let pidl = ILCreateFromPathW(PCWSTR(path_wide(path).as_ptr()));
                if pidl.is_null() { return Err("cannot resolve shell item".into()); }
                let result = SHOpenFolderAndSelectItems(pidl, None, 0).map_err(|e| e.to_string());
                CoTaskMemFree(Some(pidl.cast()));
                result?; Ok(vec![path.clone()])
            }
            FileRequest::Trash { path } => {
                let operation: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
                operation.SetOwnerWindow(ComHwnd(owner as *mut c_void)).map_err(|e| e.to_string())?;
                operation.SetOperationFlags(FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE | FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).map_err(|e| e.to_string())?;
                let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(path_wide(path).as_ptr()), None).map_err(|e| e.to_string())?;
                operation.DeleteItem(&item, None).map_err(|e| e.to_string())?;
                operation.PerformOperations().map_err(|e| e.to_string())?;
                if operation.GetAnyOperationsAborted().map_err(|e| e.to_string())?.as_bool() { return Err("trash operation cancelled".into()); }
                Ok(vec![path.clone()])
            }
        }
    }
}
pub(super) fn open(owner: usize, target: *const u16) -> Result<(), String> {
    let result = unsafe { windows_sys::Win32::UI::Shell::ShellExecuteW(owner as HWND, wide("open").as_ptr(), target, std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
    if result as isize <= 32 { Err(format!("ShellExecuteW failed: {}", result as isize)) } else { Ok(()) }
}
