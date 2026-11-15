//! HarmonyOS API 13 pasteboard. Reading remains subject to system authorization.
//! Only UTF-8 plain text and HTML are supported; writes replace the clipboard.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::NonNull;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::{Clipboard, ClipboardContent, ClipboardHandler, ContentFormat, Result};

const UNSUPPORTED: &str = "OHOS clipboard supports plain text and HTML only";

#[cfg_attr(target_env = "ohos", link(name = "pasteboard"))]
extern "C" {
    fn OH_Pasteboard_Create() -> *mut c_void;
    fn OH_Pasteboard_Destroy(p: *mut c_void);
    fn OH_Pasteboard_HasType(p: *mut c_void, mime: *const c_char) -> bool;
    fn OH_Pasteboard_GetData(p: *mut c_void, status: *mut c_int) -> *mut c_void;
    fn OH_Pasteboard_SetData(p: *mut c_void, data: *mut c_void) -> c_int;
    fn OH_Pasteboard_ClearData(p: *mut c_void) -> c_int;
    fn OH_PasteboardObserver_Create() -> *mut c_void;
    fn OH_PasteboardObserver_Destroy(p: *mut c_void);
    fn OH_PasteboardObserver_SetData(
        p: *mut c_void,
        context: *mut c_void,
        notify: unsafe extern "C" fn(*mut c_void, c_int),
        finalize: unsafe extern "C" fn(*mut c_void),
    ) -> c_int;
    fn OH_Pasteboard_Subscribe(p: *mut c_void, kind: c_int, observer: *const c_void) -> c_int;
    fn OH_Pasteboard_Unsubscribe(p: *mut c_void, kind: c_int, observer: *const c_void) -> c_int;
}

#[cfg_attr(target_env = "ohos", link(name = "udmf"))]
extern "C" {
    fn OH_UdmfData_Create() -> *mut c_void;
    fn OH_UdmfData_Destroy(p: *mut c_void);
    fn OH_UdmfData_AddRecord(data: *mut c_void, record: *mut c_void) -> c_int;
    fn OH_UdmfData_GetPrimaryPlainText(data: *mut c_void, text: *mut c_void) -> c_int;
    fn OH_UdmfData_GetPrimaryHtml(data: *mut c_void, html: *mut c_void) -> c_int;
    fn OH_UdmfRecord_Create() -> *mut c_void;
    fn OH_UdmfRecord_Destroy(p: *mut c_void);
    fn OH_UdmfRecord_AddPlainText(record: *mut c_void, text: *mut c_void) -> c_int;
    fn OH_UdmfRecord_AddHtml(record: *mut c_void, html: *mut c_void) -> c_int;
    fn OH_UdsPlainText_Create() -> *mut c_void;
    fn OH_UdsPlainText_Destroy(p: *mut c_void);
    fn OH_UdsPlainText_SetContent(p: *mut c_void, text: *const c_char) -> c_int;
    fn OH_UdsPlainText_GetContent(p: *mut c_void) -> *const c_char;
    fn OH_UdsHtml_Create() -> *mut c_void;
    fn OH_UdsHtml_Destroy(p: *mut c_void);
    fn OH_UdsHtml_SetContent(p: *mut c_void, html: *const c_char) -> c_int;
    fn OH_UdsHtml_SetPlainContent(p: *mut c_void, text: *const c_char) -> c_int;
    fn OH_UdsHtml_GetContent(p: *mut c_void) -> *const c_char;
}

struct Owned {
    ptr: NonNull<c_void>,
    destroy: unsafe extern "C" fn(*mut c_void),
}

impl Owned {
    // All constructors below return independently owned SDK handles. None escape
    // the calling thread; getters' borrowed strings are copied before destruction.
    fn new(ptr: *mut c_void, destroy: unsafe extern "C" fn(*mut c_void)) -> Result<Self> {
        Ok(Self {
            ptr: NonNull::new(ptr).ok_or("OHOS clipboard returned a null handle")?,
            destroy,
        })
    }
    fn ptr(&self) -> *mut c_void {
        self.ptr.as_ptr()
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.ptr()) }
    }
}

fn check(status: c_int) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("OHOS clipboard operation failed (status {status})").into())
    }
}
fn pasteboard() -> Result<Owned> {
    unsafe { Owned::new(OH_Pasteboard_Create(), OH_Pasteboard_Destroy) }
}
fn mime(format: &ContentFormat) -> Result<&'static CStr> {
    match format {
        ContentFormat::Text => Ok(c"text/plain"),
        ContentFormat::Html => Ok(c"text/html"),
        ContentFormat::Other(s) if s == "text/plain" => Ok(c"text/plain"),
        ContentFormat::Other(s) if s == "text/html" => Ok(c"text/html"),
        _ => Err(UNSUPPORTED.into()),
    }
}

/// Stateless provider: system handles are opened and released per operation.
pub struct ClipboardContext;
impl ClipboardContext {
    pub fn new() -> Result<Self> {
        pasteboard()?;
        Ok(Self)
    }

    fn read(data: &Owned, html: bool) -> Result<String> {
        unsafe {
            let text = if html {
                Owned::new(OH_UdsHtml_Create(), OH_UdsHtml_Destroy)?
            } else {
                Owned::new(OH_UdsPlainText_Create(), OH_UdsPlainText_Destroy)?
            };
            check(if html {
                OH_UdmfData_GetPrimaryHtml(data.ptr(), text.ptr())
            } else {
                OH_UdmfData_GetPrimaryPlainText(data.ptr(), text.ptr())
            })?;
            let content = if html {
                OH_UdsHtml_GetContent(text.ptr())
            } else {
                OH_UdsPlainText_GetContent(text.ptr())
            };
            if content.is_null() {
                return Err("OHOS clipboard content is absent".into());
            }
            Ok(CStr::from_ptr(content).to_str()?.to_owned())
        }
    }
}

impl Clipboard for ClipboardContext {
    fn available_formats(&self) -> Result<Vec<String>> {
        let p = pasteboard()?;
        Ok([c"text/plain", c"text/html"]
            .into_iter()
            .filter(|s| unsafe { OH_Pasteboard_HasType(p.ptr(), s.as_ptr()) })
            .map(|s| s.to_string_lossy().into_owned())
            .collect())
    }
    fn has(&self, format: ContentFormat) -> bool {
        pasteboard()
            .and_then(|p| Ok(unsafe { OH_Pasteboard_HasType(p.ptr(), mime(&format)?.as_ptr()) }))
            .unwrap_or(false)
    }
    fn clear(&self) -> Result<()> {
        unsafe { check(OH_Pasteboard_ClearData(pasteboard()?.ptr())) }
    }
    fn get_buffer(&self, format: &str) -> Result<Vec<u8>> {
        match format {
            "text/plain" => self.get_text(),
            "text/html" => self.get_html(),
            _ => Err(UNSUPPORTED.into()),
        }
        .map(String::into_bytes)
    }
    fn get_text(&self) -> Result<String> {
        match self.get(&[ContentFormat::Text])?.pop() {
            Some(ClipboardContent::Text(s)) => Ok(s),
            _ => Err("OHOS clipboard has no plain text".into()),
        }
    }
    fn get_html(&self) -> Result<String> {
        match self.get(&[ContentFormat::Html])?.pop() {
            Some(ClipboardContent::Html(s)) => Ok(s),
            _ => Err("OHOS clipboard has no HTML".into()),
        }
    }
    fn get_rich_text(&self) -> Result<String> {
        Err(UNSUPPORTED.into())
    }
    fn get_files(&self) -> Result<Vec<String>> {
        Err(UNSUPPORTED.into())
    }
    #[cfg(feature = "image")]
    fn get_image(&self) -> Result<crate::RustImageData> {
        Err(UNSUPPORTED.into())
    }

    fn get(&self, formats: &[ContentFormat]) -> Result<Vec<ClipboardContent>> {
        for format in formats {
            mime(format)?;
        }
        if formats.is_empty() {
            return Ok(Vec::new());
        }
        let p = pasteboard()?;
        unsafe {
            let mut status = 0;
            let ptr = OH_Pasteboard_GetData(p.ptr(), &mut status);
            // An error may still return a handle; release it on every path.
            let data = NonNull::new(ptr).map(|ptr| Owned {
                ptr,
                destroy: OH_UdmfData_Destroy,
            });
            check(status)?;
            let data = data.ok_or("OHOS clipboard is empty")?;
            formats
                .iter()
                .map(|f| {
                    let html = mime(f)? == c"text/html";
                    Self::read(&data, html).map(|s| {
                        if html {
                            ClipboardContent::Html(s)
                        } else {
                            ClipboardContent::Text(s)
                        }
                    })
                })
                .collect()
        }
    }
    fn set_buffer(&self, format: &str, buffer: Vec<u8>) -> Result<()> {
        match format {
            "text/plain" => self.set_text(String::from_utf8(buffer)?),
            "text/html" => self.set_html(String::from_utf8(buffer)?),
            _ => Err(UNSUPPORTED.into()),
        }
    }
    fn set_text(&self, text: String) -> Result<()> {
        self.set(vec![ClipboardContent::Text(text)])
    }
    fn set_html(&self, html: String) -> Result<()> {
        self.set(vec![ClipboardContent::Html(html)])
    }
    fn set_rich_text(&self, _: String) -> Result<()> {
        Err(UNSUPPORTED.into())
    }
    fn set_files(&self, _: Vec<String>) -> Result<()> {
        Err(UNSUPPORTED.into())
    }
    #[cfg(feature = "image")]
    fn set_image(&self, _: crate::RustImageData) -> Result<()> {
        Err(UNSUPPORTED.into())
    }

    fn set(&self, contents: Vec<ClipboardContent>) -> Result<()> {
        let (mut text, mut html) = (None, None);
        // Validate the entire batch before touching the system pasteboard.
        for content in contents {
            match content {
                ClipboardContent::Text(s) if text.is_none() => text = Some(CString::new(s)?),
                ClipboardContent::Html(s) if html.is_none() => html = Some(CString::new(s)?),
                _ => {
                    return Err(
                        "OHOS clipboard requires at most one plain text and one HTML value".into(),
                    )
                }
            }
        }
        if text.is_none() && html.is_none() {
            return self.clear();
        }
        unsafe {
            let p = pasteboard()?;
            let data = Owned::new(OH_UdmfData_Create(), OH_UdmfData_Destroy)?;
            let record = Owned::new(OH_UdmfRecord_Create(), OH_UdmfRecord_Destroy)?;
            let mut plain_handle = None;
            let mut html_handle = None;
            if let Some(value) = &text {
                let plain = Owned::new(OH_UdsPlainText_Create(), OH_UdsPlainText_Destroy)?;
                check(OH_UdsPlainText_SetContent(plain.ptr(), value.as_ptr()))?;
                check(OH_UdmfRecord_AddPlainText(record.ptr(), plain.ptr()))?;
                plain_handle = Some(plain);
            }
            if let Some(value) = &html {
                let rich = Owned::new(OH_UdsHtml_Create(), OH_UdsHtml_Destroy)?;
                check(OH_UdsHtml_SetContent(rich.ptr(), value.as_ptr()))?;
                if let Some(plain) = &text {
                    check(OH_UdsHtml_SetPlainContent(rich.ptr(), plain.as_ptr()))?;
                }
                check(OH_UdmfRecord_AddHtml(record.ptr(), rich.ptr()))?;
                html_handle = Some(rich);
            }
            check(OH_UdmfData_AddRecord(data.ptr(), record.ptr()))?;
            let result = check(OH_Pasteboard_SetData(p.ptr(), data.ptr()));
            drop((plain_handle, html_handle));
            result
        }
    }
}

enum Event {
    Changed,
    Stop,
}

unsafe extern "C" fn notify(context: *mut c_void, _: c_int) {
    let sender = &*context.cast::<Sender<Event>>();
    let _ = sender.send(Event::Changed);
}
unsafe extern "C" fn finalize(context: *mut c_void) {
    drop(Box::from_raw(context.cast::<Sender<Event>>()));
}

struct Subscription {
    pasteboard: Owned,
    observer: Owned,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        unsafe {
            OH_Pasteboard_Unsubscribe(self.pasteboard.ptr(), 1, self.observer.ptr());
        }
    }
}

pub struct ClipboardWatcherContext<T: ClipboardHandler> {
    handlers: Vec<T>,
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}
impl<T: ClipboardHandler> ClipboardWatcherContext<T> {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        Ok(Self {
            handlers: Vec::new(),
            sender,
            receiver,
        })
    }
    /// API compatibility with polling platforms. OHOS uses native notifications.
    pub fn new_with_interval(_: Duration) -> Result<Self> {
        Self::new()
    }

    /// Unlike the compatibility trait, this reports native subscription failures.
    pub fn try_start_watch(&mut self) -> Result<()> {
        if self.handlers.is_empty() {
            return Ok(());
        }
        unsafe {
            let p = pasteboard()?;
            let observer = Owned::new(
                OH_PasteboardObserver_Create(),
                OH_PasteboardObserver_Destroy,
            )?;
            let context = Box::into_raw(Box::new(self.sender.clone())).cast();
            let status = OH_PasteboardObserver_SetData(observer.ptr(), context, notify, finalize);
            if status != 0 {
                finalize(context);
                check(status)?;
            }
            check(OH_Pasteboard_Subscribe(p.ptr(), 1, observer.ptr()))?;
            let _subscription = Subscription {
                pasteboard: p,
                observer,
            };
            while let Ok(Event::Changed) = self.receiver.recv() {
                for handler in &mut self.handlers {
                    handler.on_clipboard_change();
                }
            }
        }
        Ok(())
    }
}
impl<T: ClipboardHandler + Send> crate::ClipboardWatcher<T> for ClipboardWatcherContext<T> {
    fn add_handler(&mut self, handler: T) -> &mut Self {
        self.handlers.push(handler);
        self
    }
    fn start_watch(&mut self) {
        if let Err(error) = self.try_start_watch() {
            eprintln!("{error}");
        }
    }
    fn get_shutdown_channel(&self) -> crate::WatcherShutdown {
        // This module is also compiled directly by the host-side SDK contract test.
        crate::WatcherShutdown::from_ohos_sender(self.sender.clone())
    }
}
pub struct WatcherShutdown {
    sender: Sender<Event>,
}
impl WatcherShutdown {
    fn from_ohos_sender(sender: Sender<Event>) -> Self {
        Self { sender }
    }
    pub fn stop(self) {
        drop(self);
    }
}
impl Drop for WatcherShutdown {
    fn drop(&mut self) {
        let _ = self.sender.send(Event::Stop);
    }
}
