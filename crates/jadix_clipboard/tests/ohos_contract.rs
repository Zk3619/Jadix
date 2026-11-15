//! Executes the production OHOS backend against an in-process SDK double.
#![cfg(not(target_env = "ohos"))]
#![allow(non_snake_case)]

#[cfg(feature = "image")]
pub use jadix_clipboard::RustImageData;
pub use jadix_clipboard::{Clipboard, ClipboardContent, ClipboardHandler, ContentFormat, Result};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::{
    atomic::{AtomicIsize, Ordering},
    mpsc, Mutex,
};
use std::time::Duration;

#[path = "../src/platform/ohos.rs"]
mod ohos;
pub use ohos::WatcherShutdown;

// Match the platform-specific shutdown type; the OHOS cross check also compiles
// this implementation against the actual public trait in lib.rs.
pub trait ClipboardWatcher<T: ClipboardHandler>: Send {
    fn add_handler(&mut self, handler: T) -> &mut Self;
    fn start_watch(&mut self);
    fn get_shutdown_channel(&self) -> WatcherShutdown;
}

#[derive(Clone, Default)]
struct Object {
    text: Option<CString>,
    html: Option<CString>,
    context: usize,
    notify: Option<unsafe extern "C" fn(*mut c_void, c_int)>,
    finalize: Option<unsafe extern "C" fn(*mut c_void)>,
}
#[derive(Default)]
struct State {
    text: Option<CString>,
    html: Option<CString>,
    read_error: i32,
    write_error: i32,
    subscribe_error: i32,
    writes: usize,
    subscriptions: i32,
}
static STATE: Mutex<State> = Mutex::new(State {
    text: None,
    html: None,
    read_error: 0,
    write_error: 0,
    subscribe_error: 0,
    writes: 0,
    subscriptions: 0,
});
static LIVE: AtomicIsize = AtomicIsize::new(0);
fn allocate(value: Object) -> *mut c_void {
    LIVE.fetch_add(1, Ordering::SeqCst);
    Box::into_raw(Box::new(value)).cast()
}
unsafe fn object<'a>(p: *mut c_void) -> &'a mut Object {
    &mut *p.cast::<Object>()
}
unsafe fn release(p: *mut c_void) {
    let value = Box::from_raw(p.cast::<Object>());
    if let Some(finalize) = value.finalize {
        finalize(value.context as *mut c_void);
    }
    LIVE.fetch_sub(1, Ordering::SeqCst);
}
macro_rules! handles {
    ($($create:ident, $destroy:ident;)*) => {$ (
        #[no_mangle] extern "C" fn $create() -> *mut c_void { allocate(Object::default()) }
        #[no_mangle] unsafe extern "C" fn $destroy(p: *mut c_void) { release(p); }
    )*};
}
handles! {
    OH_Pasteboard_Create, OH_Pasteboard_Destroy;
    OH_PasteboardObserver_Create, OH_PasteboardObserver_Destroy;
    OH_UdmfData_Create, OH_UdmfData_Destroy;
    OH_UdmfRecord_Create, OH_UdmfRecord_Destroy;
    OH_UdsPlainText_Create, OH_UdsPlainText_Destroy;
    OH_UdsHtml_Create, OH_UdsHtml_Destroy;
}
#[no_mangle]
unsafe extern "C" fn OH_Pasteboard_HasType(_: *mut c_void, mime: *const c_char) -> bool {
    let state = STATE.lock().unwrap();
    match CStr::from_ptr(mime).to_bytes() {
        b"text/plain" => state.text.is_some(),
        b"text/html" => state.html.is_some(),
        _ => false,
    }
}
#[no_mangle]
unsafe extern "C" fn OH_Pasteboard_GetData(_: *mut c_void, status: *mut c_int) -> *mut c_void {
    let state = STATE.lock().unwrap();
    *status = state.read_error;
    allocate(Object {
        text: state.text.clone(),
        html: state.html.clone(),
        ..Default::default()
    })
}
#[no_mangle]
unsafe extern "C" fn OH_Pasteboard_SetData(_: *mut c_void, data: *mut c_void) -> c_int {
    let mut state = STATE.lock().unwrap();
    state.writes += 1;
    if state.write_error != 0 {
        return state.write_error;
    }
    state.text = object(data).text.clone();
    state.html = object(data).html.clone();
    0
}
#[no_mangle]
extern "C" fn OH_Pasteboard_ClearData(_: *mut c_void) -> c_int {
    let mut state = STATE.lock().unwrap();
    state.text = None;
    state.html = None;
    0
}
macro_rules! copy_value {
    ($name:ident, $field:ident) => {
        #[no_mangle]
        unsafe extern "C" fn $name(dest: *mut c_void, source: *mut c_void) -> c_int {
            object(dest).$field = object(source).$field.clone();
            0
        }
    };
}
copy_value!(OH_UdmfRecord_AddPlainText, text);
copy_value!(OH_UdmfRecord_AddHtml, html);
#[no_mangle]
unsafe extern "C" fn OH_UdmfData_AddRecord(data: *mut c_void, record: *mut c_void) -> c_int {
    *object(data) = object(record).clone();
    0
}
#[no_mangle]
unsafe extern "C" fn OH_UdmfData_GetPrimaryPlainText(
    data: *mut c_void,
    text: *mut c_void,
) -> c_int {
    object(text).text = object(data).text.clone();
    if object(text).text.is_some() {
        0
    } else {
        1
    }
}
#[no_mangle]
unsafe extern "C" fn OH_UdmfData_GetPrimaryHtml(data: *mut c_void, html: *mut c_void) -> c_int {
    object(html).html = object(data).html.clone();
    if object(html).html.is_some() {
        0
    } else {
        1
    }
}
macro_rules! content {
    ($set:ident, $get:ident, $field:ident) => {
        #[no_mangle]
        unsafe extern "C" fn $set(p: *mut c_void, value: *const c_char) -> c_int {
            object(p).$field = Some(CStr::from_ptr(value).to_owned());
            0
        }
        #[no_mangle]
        unsafe extern "C" fn $get(p: *mut c_void) -> *const c_char {
            object(p)
                .$field
                .as_ref()
                .map_or(std::ptr::null(), |s| s.as_ptr())
        }
    };
}
content!(OH_UdsPlainText_SetContent, OH_UdsPlainText_GetContent, text);
content!(OH_UdsHtml_SetContent, OH_UdsHtml_GetContent, html);
#[no_mangle]
unsafe extern "C" fn OH_UdsHtml_SetPlainContent(p: *mut c_void, text: *const c_char) -> c_int {
    object(p).text = Some(CStr::from_ptr(text).to_owned());
    0
}
#[no_mangle]
unsafe extern "C" fn OH_PasteboardObserver_SetData(
    p: *mut c_void,
    context: *mut c_void,
    notify: unsafe extern "C" fn(*mut c_void, c_int),
    finalize: unsafe extern "C" fn(*mut c_void),
) -> c_int {
    let value = object(p);
    value.context = context as usize;
    value.notify = Some(notify);
    value.finalize = Some(finalize);
    0
}
#[no_mangle]
unsafe extern "C" fn OH_Pasteboard_Subscribe(
    _: *mut c_void,
    kind: c_int,
    observer: *const c_void,
) -> c_int {
    assert_eq!(kind, 1);
    let mut state = STATE.lock().unwrap();
    if state.subscribe_error != 0 {
        return state.subscribe_error;
    }
    state.subscriptions += 1;
    drop(state);
    let value = object(observer.cast_mut());
    value.notify.unwrap()(value.context as *mut c_void, kind);
    0
}
#[no_mangle]
extern "C" fn OH_Pasteboard_Unsubscribe(_: *mut c_void, kind: c_int, _: *const c_void) -> c_int {
    assert_eq!(kind, 1);
    STATE.lock().unwrap().subscriptions -= 1;
    0
}

struct Handler(mpsc::Sender<()>);
impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) {
        self.0.send(()).unwrap();
    }
}

#[test]
fn text_html_errors_batch_atomicity_and_subscription_cleanup() {
    let clipboard = ohos::ClipboardContext::new().unwrap();
    clipboard
        .set(vec![
            ClipboardContent::Text("中文 🦀".into()),
            ClipboardContent::Html("<b>中文 🦀</b>".into()),
        ])
        .unwrap();
    assert_eq!(clipboard.get_text().unwrap(), "中文 🦀");
    assert_eq!(clipboard.get_html().unwrap(), "<b>中文 🦀</b>");
    assert_eq!(
        clipboard.available_formats().unwrap(),
        ["text/plain", "text/html"]
    );
    assert!(clipboard.has(ContentFormat::Text));
    assert!(!clipboard.has(ContentFormat::Files));
    assert_eq!(
        clipboard
            .get(&[ContentFormat::Text, ContentFormat::Html])
            .unwrap()
            .len(),
        2
    );
    let writes = STATE.lock().unwrap().writes;
    assert!(clipboard.set_text("bad\0value".into()).is_err());
    assert!(clipboard
        .set(vec![
            ClipboardContent::Text("replacement".into()),
            ClipboardContent::Files(vec![])
        ])
        .is_err());
    assert!(clipboard.set_buffer("text/plain", vec![255]).is_err());
    assert_eq!(STATE.lock().unwrap().writes, writes);
    assert_eq!(clipboard.get_text().unwrap(), "中文 🦀");
    STATE.lock().unwrap().read_error = 201;
    assert!(clipboard
        .get_text()
        .unwrap_err()
        .to_string()
        .contains("201"));
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    STATE.lock().unwrap().read_error = 0;
    STATE.lock().unwrap().write_error = 129000;
    assert!(clipboard
        .set_text("denied".into())
        .unwrap_err()
        .to_string()
        .contains("129000"));
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    STATE.lock().unwrap().write_error = 0;
    clipboard
        .set_buffer("text/plain", "再次复制".as_bytes().into())
        .unwrap();
    assert_eq!(
        clipboard.get_buffer("text/plain").unwrap(),
        "再次复制".as_bytes()
    );
    clipboard.clear().unwrap();
    assert!(clipboard.available_formats().unwrap().is_empty());
    assert!(clipboard.get_text().is_err());

    STATE.lock().unwrap().subscribe_error = 201;
    let (sender, _) = mpsc::channel();
    let mut denied_watcher = ohos::ClipboardWatcherContext::new().unwrap();
    denied_watcher.add_handler(Handler(sender));
    assert!(denied_watcher
        .try_start_watch()
        .unwrap_err()
        .to_string()
        .contains("201"));
    assert_eq!(STATE.lock().unwrap().subscriptions, 0);
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    STATE.lock().unwrap().subscribe_error = 0;

    let (sender, receiver) = mpsc::channel();
    let mut watcher =
        ohos::ClipboardWatcherContext::new_with_interval(Duration::from_millis(1)).unwrap();
    watcher.add_handler(Handler(sender));
    let stop = watcher.get_shutdown_channel();
    let thread = std::thread::spawn(move || watcher.try_start_watch().unwrap());
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    stop.stop();
    thread.join().unwrap();
    assert_eq!(STATE.lock().unwrap().subscriptions, 0);
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}
