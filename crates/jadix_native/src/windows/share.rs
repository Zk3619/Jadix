//! HWND-bound sharing. File resolution is asynchronous; callbacks never retain
//! the host or touch its UI-thread state.
use super::*;
use jadix_core::desktop::{Share, ShareOutcome, ShareResult};
use std::sync::{Arc, Mutex, mpsc::{self, Receiver, SyncSender}};
use windows::{core::{AgileReference, HSTRING, Interface, IInspectable}, ApplicationModel::DataTransfer::*,
    Foundation::{TypedEventHandler, Uri}, Storage::{StorageFile, IStorageItem},
    Win32::UI::Shell::IDataTransferManagerInterop};

#[derive(Default)]
struct Queue {
    generation: u64,
    active: bool,
    files: Vec<Option<Result<AgileReference<StorageFile>, String>>>,
    result: Option<ShareResult>,
    closed: bool,
}
type Shared = Arc<Mutex<Queue>>;

pub(super) struct Sharing {
    queue: Shared,
    pending: Option<Pending>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
struct Pending {
    generation: u64,
    payload: Share,
    send: SyncSender<ShareResult>,
    cancel: Vec<Box<dyn FnOnce()>>,
    registration: Option<Registration>,
}
struct Registration {
    manager: DataTransferManager,
    package: DataPackage,
    requested: Option<i64>,
    completed: Option<i64>,
    canceled: Option<i64>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(token) = self.requested.take() { let _ = self.manager.RemoveDataRequested(token); }
        if let Some(token) = self.completed.take() { let _ = self.package.RemoveShareCompleted(token); }
        if let Some(token) = self.canceled.take() { let _ = self.package.RemoveShareCanceled(token); }
    }
}
impl Drop for Pending {
    fn drop(&mut self) { for cancel in self.cancel.drain(..) { cancel(); } }
}
impl Sharing {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self { queue: Default::default(), pending: None, wake: Arc::new(wake) }
    }
    pub fn request(&mut self, share: &Share) -> Result<Receiver<ShareResult>, String> {
        share.validate()?;
        let generation = {
            let mut queue = self.queue.lock().unwrap();
            if queue.closed { return Err("native sharing is disposed".into()); }
            let generation = queue.generation.checked_add(1).ok_or("share identity exhausted")?;
            *queue = Queue { generation, active: true, files: vec![None; share.files.len()], ..Default::default() };
            generation
        };
        self.finish(Ok(ShareOutcome::Replaced));
        let (send, receive) = mpsc::sync_channel(1);
        self.pending = Some(Pending { generation, payload: share.clone(), send, cancel: Vec::new(), registration: None });
        for (index, path) in share.files.iter().enumerate() {
            let start = (|| -> Result<(), String> {
                let operation = StorageFile::GetFileFromPathAsync(&HSTRING::from(media_path(path)?)).map_err(|e| e.to_string())?;
                let cancel = operation.clone();
                self.pending.as_mut().unwrap().cancel.push(Box::new(move || { let _ = cancel.Cancel(); }));
                let queue = self.queue.clone();
                let wake = self.wake.clone();
                operation.when(move |result| {
                    let mut queue = queue.lock().unwrap();
                    if !queue.closed && queue.active && queue.generation == generation {
                        queue.files[index] = Some(result.and_then(|file| AgileReference::new(&file)).map_err(|e| e.to_string()));
                        wake();
                    }
                }).map_err(|e| e.to_string())
            })();
            if let Err(error) = start { self.finish(Err(error)); break; }
        }
        (self.wake)();
        Ok(receive)
    }
    pub fn pump(&mut self, owner: HWND) {
        let result = self.queue.lock().unwrap().result.take();
        if let Some(result) = result { self.finish(result); }
        if self.pending.as_ref().is_none_or(|pending| pending.registration.is_some()) { return; }
        let files = {
            let mut queue = self.queue.lock().unwrap();
            if queue.files.iter().any(Option::is_none) { return; }
            std::mem::take(&mut queue.files).into_iter().map(|file| file.unwrap().and_then(|file| file.resolve().map_err(|e| e.to_string()))).collect::<Result<Vec<_>, _>>()
        };
        let result = files.and_then(|files| self.show(owner, files).map_err(|e| e.to_string()));
        if let Err(error) = result { self.finish(Err(error)); }
    }
    fn show(&mut self, owner: HWND, files: Vec<StorageFile>) -> windows::core::Result<()> {
        let pending = self.pending.as_mut().unwrap();
        let package = make_package(&pending.payload, files)?;
        let interop: IDataTransferManagerInterop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
        let manager: DataTransferManager = unsafe { interop.GetForWindow(windows::Win32::Foundation::HWND(owner))? };
        let mut registration = Registration { manager, package, requested: None, completed: None, canceled: None };
        let generation = self.queue.lock().unwrap().generation;
        let queue = self.queue.clone(); let wake = self.wake.clone();
        registration.completed = Some(registration.package.ShareCompleted(&TypedEventHandler::<DataPackage, ShareCompletedEventArgs>::new(move |_, args| {
            let service = args.as_ref().and_then(|args| args.ShareTarget().ok()).and_then(|target| {
                target.AppUserModelId().ok().filter(|id| !id.is_empty()).or_else(|| target.ShareProvider().ok()?.Title().ok())
            }).map(|name| name.to_string()).unwrap_or_default();
            complete(&queue, generation, Ok(ShareOutcome::Shared { service }), &wake);
            Ok(())
        }))?);
        let queue = self.queue.clone(); let wake = self.wake.clone();
        registration.canceled = Some(registration.package.ShareCanceled(&TypedEventHandler::<DataPackage, IInspectable>::new(move |_, _| {
            complete(&queue, generation, Ok(ShareOutcome::Dismissed), &wake); Ok(())
        }))?);
        let package = registration.package.clone();
        let queue = self.queue.clone(); let wake = self.wake.clone();
        registration.requested = Some(registration.manager.DataRequested(&TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| {
            // Old callbacks may already be in flight when a request is replaced.
            { let queue = queue.lock().unwrap();
                if queue.closed || !queue.active || queue.generation != generation { return Ok(()); }
            }
            let result = args.as_ref().ok_or_else(windows::core::Error::empty).and_then(|args| args.Request()?.SetData(&package));
            if let Err(error) = result { complete(&queue, generation, Err(error.to_string()), &wake); }
            Ok(())
        }))?);
        // Keep every token alive before the OS can synchronously request data.
        pending.registration = Some(registration);
        unsafe { interop.ShowShareUIForWindow(windows::Win32::Foundation::HWND(owner)) }
    }
    fn finish(&mut self, result: ShareResult) {
        if let Some(pending) = self.pending.take() {
            { let mut queue = self.queue.lock().unwrap();
                if queue.generation == pending.generation {
                    queue.active = false; queue.files.clear(); queue.result = None;
                }
            }
            let _ = pending.send.send(result);
        }
    }
    pub fn dispose(&mut self) {
        { let mut queue = self.queue.lock().unwrap(); queue.closed = true; queue.files.clear(); queue.result = None; }
        self.finish(Ok(ShareOutcome::Dismissed));
    }
}
impl Drop for Sharing { fn drop(&mut self) { self.dispose(); } }

fn complete(queue: &Shared, generation: u64, result: ShareResult, wake: &Arc<dyn Fn() + Send + Sync>) {
    let mut queue = queue.lock().unwrap();
    if !queue.closed && queue.active && queue.generation == generation && queue.result.is_none() { queue.result = Some(result); wake(); }
}

fn make_package(share: &Share, files: Vec<StorageFile>) -> windows::core::Result<DataPackage> {
    let package = DataPackage::new()?;
    package.Properties()?.SetTitle(&HSTRING::from(share.subject.as_deref().filter(|s| !s.is_empty()).unwrap_or("Share")))?;
    package.SetRequestedOperation(DataPackageOperation::Copy)?;
    if let Some(text) = &share.text { package.SetText(&HSTRING::from(text))?; }
    if let Some(url) = &share.url {
        let uri = Uri::CreateUri(&HSTRING::from(url))?;
        if uri.SchemeName()?.to_string().eq_ignore_ascii_case("https") || uri.SchemeName()?.to_string().eq_ignore_ascii_case("http") { package.SetWebLink(&uri)?; }
        else { package.SetApplicationLink(&uri)?; }
    }
    if !files.is_empty() {
        let items = files.into_iter().map(|file| file.cast::<IStorageItem>().map(Some)).collect::<windows::core::Result<Vec<_>>>()?;
        package.SetStorageItems(&windows_collections::IIterable::<IStorageItem>::from(items), true)?;
    }
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn replacement_disposal_and_late_callbacks_complete_once() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let mut sharing = Sharing::new(move || { count.fetch_add(1, Ordering::Relaxed); });
        let payload = Share { text: Some("中文 🦀".into()), ..Default::default() };
        let old = sharing.request(&payload).unwrap();
        let old_generation = sharing.queue.lock().unwrap().generation;
        let current = sharing.request(&payload).unwrap();
        assert_eq!(old.recv().unwrap(), Ok(ShareOutcome::Replaced));
        complete(&sharing.queue, old_generation, Ok(ShareOutcome::Dismissed), &sharing.wake);
        assert!(sharing.queue.lock().unwrap().result.is_none());
        assert!(sharing.request(&Share::default()).is_err());
        let generation = sharing.queue.lock().unwrap().generation;
        complete(&sharing.queue, generation, Ok(ShareOutcome::Shared { service: "fixture".into() }), &sharing.wake);
        complete(&sharing.queue, generation, Ok(ShareOutcome::Dismissed), &sharing.wake);
        // A queued completion is consumed before any attempt to show system UI.
        sharing.pump(std::ptr::null_mut());
        assert_eq!(current.recv().unwrap(), Ok(ShareOutcome::Shared { service: "fixture".into() }));
        let before = wakes.load(Ordering::Relaxed);
        complete(&sharing.queue, generation, Ok(ShareOutcome::Dismissed), &sharing.wake);
        assert_eq!(before, wakes.load(Ordering::Relaxed));
        let pending = sharing.request(&payload).unwrap();
        sharing.dispose();
        assert_eq!(pending.recv().unwrap(), Ok(ShareOutcome::Dismissed));
        assert!(sharing.request(&payload).is_err());
        assert!(sharing.queue.lock().unwrap().files.is_empty());
    }

    #[test]
    fn winrt_package_preserves_unicode_links_and_file_payloads() {
        std::thread::spawn(|| {
            unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).ok().unwrap(); }
            struct Com;
            impl Drop for Com { fn drop(&mut self) { unsafe { windows::Win32::System::Com::CoUninitialize(); } } }
            let _com = Com;
            let root = std::env::temp_dir().join(format!("jadix-share-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join("中文 🦀.txt");
            std::fs::write(&path, "分享内容").unwrap();
            let share = Share { subject: Some("中文标题".into()), text: Some("Unicode 🦀\n第二行".into()), url: Some("HTTPS://example.com/document?q=%20".into()), files: vec![path.clone()], ..Default::default() };
            let mut sharing = Sharing::new(|| {});
            let response = sharing.request(&share).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let files = loop {
                let mut queue = sharing.queue.lock().unwrap();
                if queue.files.iter().all(Option::is_some) {
                    break std::mem::take(&mut queue.files).into_iter().map(|file| file.unwrap().unwrap().resolve().unwrap()).collect();
                }
                drop(queue);
                assert!(std::time::Instant::now() < deadline, "StorageFile resolution timed out");
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let package = make_package(&share, files).unwrap();
            assert_eq!(package.Properties().unwrap().Title().unwrap().to_string(), "中文标题");
            let view = package.GetView().unwrap();
            assert_eq!(view.GetTextAsync().unwrap().join().unwrap().to_string(), share.text.unwrap());
            assert_eq!(view.GetWebLinkAsync().unwrap().join().unwrap().Host().unwrap().to_string(), "example.com");
            let items = view.GetStorageItemsAsync().unwrap().join().unwrap();
            assert_eq!(items.Size().unwrap(), 1);
            assert_eq!(items.GetAt(0).unwrap().Name().unwrap().to_string(), "中文 🦀.txt");
            sharing.dispose();
            assert_eq!(response.recv().unwrap(), Ok(ShareOutcome::Dismissed));
            drop(items); drop(view); drop(package);
            std::fs::remove_dir_all(root).unwrap();
        }).join().unwrap();
    }
}
