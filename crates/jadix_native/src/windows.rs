use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, SetFocus, VK_ESCAPE,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::host::{ApplyReceipt, PlatformHost};
use crate::layout::{LayoutError, MeasureRequest};
use crate::presentation::capabilities::{BackendCapabilities, NodeKind};
use crate::presentation::*;
use crate::presentation::style::{ImageFit, ImagePosition};

const REACTIVE_TIMER: usize = 0x4a4458;
const MAX_EVENTS: usize = 1024;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_EVENT_BYTES: usize = 2 * 1024 * 1024;
const INVALID_NATIVE_UTF16: &str = "native text contains invalid UTF-16";

mod accessibility;
mod drag;
mod fonts;
pub use fonts::{FontId, FontRegistration};
#[cfg(feature = "windows-gpu")]
mod gpu;
mod image;
mod clipboard;
mod files;
mod keyboard;
mod services;
mod updater;
pub use updater::run_update_helper;
mod share;
mod deep_links;
mod activation;
mod notifications;
mod audio;
mod speech;
mod badge;
mod tray;
mod web;
mod hosted;
#[path = "windows/windows.rs"]
mod window_manager;
pub use window_manager::WindowApplication;
mod overlay;
mod paint;

use overlay::ActivePopover;

struct NativeImage {
    bitmap: Option<image::Bitmap>,
    alt: String,
    failed: bool,
    fit: ImageFit,
    position: ImagePosition,
}

struct DpiScope(DPI_AWARENESS_CONTEXT);

impl DpiScope {
    fn window(window: HWND) -> Self {
        Self(unsafe { SetThreadDpiAwarenessContext(GetWindowDpiAwarenessContext(window)) })
    }
}

impl Drop for DpiScope {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

// Shell and WinRT media APIs consume ordinary DOS/UNC paths, including paths
// returned by Rust's canonicalize with an extended-length prefix.
fn media_path(path: &std::path::Path) -> Result<String, String> {
    let path = path.to_str().ok_or("media path is not Unicode")?;
    Ok(if let Some(rest) = path.strip_prefix(r"\\?\UNC\") { format!(r"\\{rest}") }
        else { path.strip_prefix(r"\\?\").unwrap_or(path).to_owned() })
}

fn native_error(operation: &str) -> String {
    format!("{operation}: Win32 error {}", unsafe { GetLastError() })
}

struct Font(HFONT);

impl Font {
    fn new(descriptor: &NodeDescriptor, scale: f64) -> Result<Self, String> {
        let name = wide(&fonts::resolve_family(&descriptor.style.font_family));
        let handle = unsafe {
            CreateFontW(
                -(f64::from(descriptor.style.font_size) * scale)
                    .round()
                    .max(1.0) as i32,
                0,
                0,
                0,
                i32::from(descriptor.style.font_weight),
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                name.as_ptr(),
            )
        };
        if handle.is_null() {
            Err(native_error("CreateFontW"))
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

enum RawEvent {
    ImageLoad { control: NodeId, generation: u64, event: ImageLoadEvent },
    Hosted { control: NodeId, event: String, data: serde_json::Value },
    Key { control: NodeId, event: crate::presentation::keyboard::KeyEvent },
    Shortcut { control: NodeId, id: String },
    Tree {
        control: NodeId,
        action: TreeAction,
    },
    Drag {
        control: NodeId,
        phase: DragPhase,
        screen_position: [f64; 2],
    },
    Navigate {
        control: NodeId,
        key: NavigationKey,
        shift: bool,
        command: bool,
    },
    Dismiss {
        control: NodeId,
        backdrop: bool,
    },
    Select {
        control: NodeId,
        index: isize,
    },
    Scroll {
        control: NodeId,
        offset: [i32; 2],
    },
    Activate {
        control: NodeId,
        checked: bool,
    },
    Focus {
        control: NodeId,
    },
    FocusVisible {
        control: NodeId,
    },
    Blur {
        control: NodeId,
    },
    Submit {
        control: NodeId,
    },
    Edit {
        control: NodeId,
        value: String,
        selection: [usize; 2],
        composing: Option<[usize; 2]>,
    },
}

fn grid_navigation_key(
    key: usize,
    shift: bool,
    control: bool,
    alt: bool,
    composing: bool,
) -> Option<NavigationKey> {
    if alt || composing || control && shift {
        return None;
    }
    match (key, control) {
        (0x09, false) => Some(NavigationKey::Tab),
        (0x0d, false) => Some(NavigationKey::Enter),
        (0x21, false) if !shift => Some(NavigationKey::PageUp),
        (0x22, false) if !shift => Some(NavigationKey::PageDown),
        (0x26, true) => Some(NavigationKey::Up),
        (0x28, true) => Some(NavigationKey::Down),
        (0x25, true) => Some(NavigationKey::Left),
        (0x27, true) => Some(NavigationKey::Right),
        (0x24, true) => Some(NavigationKey::Home),
        (0x23, true) => Some(NavigationKey::End),
        _ => None,
    }
}

#[derive(Clone, Copy, Default)]
struct ScrollState {
    offset: [i32; 2],
    maximum: [i32; 2],
    page: [i32; 2],
    remainder: jadix_core::presentation::scroll::ScrollRemainder,
}

struct Composition {
    original: String,
    range: [usize; 2],
}

impl Composition {
    fn snapshot(&self, marked: &[u16], cursor: usize) -> Option<(String, [usize; 2], [usize; 2])> {
        let mut units: Vec<_> = self.original.encode_utf16().collect();
        if !valid_utf16_range(&self.original, self.range)
            || units.len() - (self.range[1] - self.range[0]) + marked.len() > MAX_TEXT
        {
            return None;
        }
        units.splice(self.range[0]..self.range[1], marked.iter().copied());
        let value = String::from_utf16(&units).ok()?;
        let range = [self.range[0], self.range[0] + marked.len()];
        let caret = [self.range[0] + cursor.min(marked.len()); 2];
        (valid_utf16_range(&value, caret) && valid_utf16_range(&value, range))
            .then_some((value, caret, range))
    }
}

struct WindowContext {
    window_policy: RefCell<window_manager::Policy>,
    badge: badge::Badge,
    tray: tray::StatusItem,
    desktop: jadix_core::desktop_services::DesktopServices,
    deep_links: RefCell<deep_links::Links>,
    hotkeys: RefCell<BTreeMap<i32, u32>>,
    wake_count: Cell<u64>,
    drag: drag::DragTracker,
    #[cfg(feature = "windows-gpu")]
    gpu_windows: RefCell<BTreeSet<usize>>,
    #[cfg(feature = "windows-gpu")]
    gpu_dirty: RefCell<BTreeSet<usize>>,
    #[cfg(feature = "windows-gpu")]
    shader_windows: RefCell<BTreeSet<usize>>,
    navigation: RefCell<BTreeSet<NodeId>>,
    grid_navigation: RefCell<BTreeSet<NodeId>>,
    keyboard: RefCell<BTreeMap<NodeId, crate::presentation::keyboard::KeyboardPolicy>>,
    paints: RefCell<BTreeMap<usize, paint::Paint>>,
    palette_dirty: Cell<bool>,
    system_fonts_dirty: Cell<bool>,
    high_contrast: Cell<bool>,
    popovers: RefCell<Vec<ActivePopover>>,
    modal: Cell<Option<NodeId>>,
    backdrops: RefCell<BTreeMap<usize, NodeId>>,
    images: RefCell<BTreeMap<usize, NativeImage>>,
    targets: RefCell<BTreeMap<usize, NodeId>>,
    root: Cell<HWND>,
    events: RefCell<VecDeque<RawEvent>>,
    event_bytes: Cell<usize>,
    suppress: Cell<bool>,
    failed: Cell<bool>,
    closed: Cell<bool>,
    defer_close: Cell<bool>,
    close_requested: Cell<bool>,
    environment: Cell<u64>,
    scrolls: RefCell<BTreeMap<usize, ScrollState>>,
    compositions: RefCell<BTreeMap<usize, Composition>>,
    scale: Cell<f64>,
    scroll_dirty: Cell<bool>,
}

unsafe fn edit_snapshot(context: &WindowContext, window: HWND) {
    let Some(control) = context.target(window) else {
        return;
    };
    match unsafe { read_text(window) } {
        Ok(value) => context.push(RawEvent::Edit {
            control,
            value,
            selection: unsafe { selection(window) },
            composing: None,
        }),
        // EDIT emits EN_CHANGE between the two WM_CHAR messages of a
        // supplementary character. Publish only complete Unicode snapshots.
        Err(error) if error == INVALID_NATIVE_UTF16 => {}
        Err(_) => context.failed.set(true),
    }
}

// Descendants provide button content; pointer input belongs to the control.
unsafe extern "system" fn button_content_proc(
    window: HWND, message: u32, parameter: WPARAM, data: LPARAM,
    _identifier: usize, _reference: usize,
) -> LRESULT {
    if message == WM_NCHITTEST {
        let mut parent = unsafe { GetParent(window) };
        for _ in 0..256 {
            if parent.is_null() { break; }
            let mut class = [0u16; 16];
            let length = unsafe { GetClassNameW(parent, class.as_mut_ptr(), class.len() as i32) };
            if length == 6 && class[..6].iter().zip("button".encode_utf16())
                .all(|(actual, expected)| *actual == expected || *actual == expected - 32) {
                return HTTRANSPARENT as LRESULT;
            }
            parent = unsafe { GetParent(parent) };
        }
    } else if message == WM_NCDESTROY {
        unsafe { RemoveWindowSubclass(window, Some(button_content_proc), 2); }
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}

unsafe extern "system" fn edit_proc(
    window: HWND,
    message: u32,
    parameter: WPARAM,
    data: LPARAM,
    _: usize,
    reference: usize,
) -> LRESULT {
    let context = unsafe { &*(reference as *const WindowContext) };
    // The classic EDIT control implements cut/copy/paste, but not Ctrl+A.
    // WM_CHAR carries the translated control character for both input kinds.
    if message == WM_CHAR && parameter == 1
        && unsafe { IsWindowEnabled(window) } != 0
        && !context.compositions.borrow().contains_key(&(window as usize))
    {
        unsafe { SendMessageW(window, EM_SETSEL, 0, -1); }
        return 0;
    }
    let single_line =
        unsafe { GetWindowLongPtrW(window, GWL_STYLE) } as u32 & ES_MULTILINE as u32 == 0;
    if single_line && parameter == 0x0d {
        if message == WM_GETDLGCODE {
            return unsafe { DefSubclassProc(window, message, parameter, data) }
                | DLGC_WANTMESSAGE as isize;
        }
        if message == WM_KEYDOWN {
            let modified = [0x11, 0x12, 0x5b, 0x5c].into_iter().any(|key| unsafe {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(key) < 0
            });
            if unsafe { IsWindowEnabled(window) } != 0
                && !modified
                && data as usize & (1 << 30) == 0
                && !context
                    .compositions
                    .borrow()
                    .contains_key(&(window as usize))
            {
                if let Some(control) = context.target(window) {
                    context.push(RawEvent::Submit { control });
                }
            }
            return 0;
        }
        if message == WM_CHAR {
            return 0;
        }
    }
    if message == WM_IME_STARTCOMPOSITION {
        match unsafe { read_text(window) } {
            Ok(original) => {
                context.compositions.borrow_mut().insert(
                    window as usize,
                    Composition {
                        original,
                        range: unsafe { selection(window) },
                    },
                );
            }
            Err(_) => context.failed.set(true),
        }
    }
    let result = unsafe { DefSubclassProc(window, message, parameter, data) };
    if message == WM_IME_COMPOSITION {
        if data as u32 & GCS_RESULTSTR != 0 {
            unsafe {
                edit_snapshot(context, window);
            }
            if let Ok(original) = unsafe { read_text(window) } {
                context.compositions.borrow_mut().insert(
                    window as usize,
                    Composition {
                        original,
                        range: unsafe { selection(window) },
                    },
                );
            }
        }
        if data as u32 & GCS_COMPSTR != 0 {
            let input = unsafe { ImmGetContext(window) };
            if !input.is_null() {
                let bytes = unsafe { ImmGetCompositionStringW(input, GCS_COMPSTR, null_mut(), 0) };
                let cursor =
                    unsafe { ImmGetCompositionStringW(input, GCS_CURSORPOS, null_mut(), 0) };
                let mut marked = vec![0u16; (bytes.max(0) as usize / 2).min(MAX_TEXT)];
                let copied = unsafe {
                    ImmGetCompositionStringW(
                        input,
                        GCS_COMPSTR,
                        marked.as_mut_ptr().cast(),
                        (marked.len() * 2) as u32,
                    )
                };
                unsafe {
                    ImmReleaseContext(window, input);
                }
                if bytes >= 0
                    && bytes % 2 == 0
                    && copied == bytes
                    && bytes as usize <= MAX_TEXT * 2
                    && cursor >= 0
                {
                    if let Some((value, selection, range)) = context
                        .compositions
                        .borrow()
                        .get(&(window as usize))
                        .and_then(|composition| composition.snapshot(&marked, cursor as usize))
                    {
                        if let Some(control) = context.target(window) {
                            context.push(RawEvent::Edit {
                                control,
                                value,
                                selection,
                                composing: Some(range),
                            });
                        }
                    } else {
                        context.failed.set(true);
                    }
                } else {
                    context.failed.set(true);
                }
            }
        }
    } else if message == WM_IME_ENDCOMPOSITION {
        context.compositions.borrow_mut().remove(&(window as usize));
        unsafe {
            edit_snapshot(context, window);
        }
    } else if message == WM_NCDESTROY {
        context.compositions.borrow_mut().remove(&(window as usize));
        unsafe {
            RemoveWindowSubclass(window, Some(edit_proc), 1);
        }
    }
    result
}

unsafe fn scroll_window(context: &WindowContext, window: HWND, offset: [i32; 2]) {
    let Some(previous) = context.scrolls.borrow().get(&(window as usize)).copied() else {
        return;
    };
    let next = std::array::from_fn(|axis| offset[axis].clamp(0, previous.maximum[axis]));
    if previous.offset == next {
        return;
    }
    context
        .scrolls
        .borrow_mut()
        .get_mut(&(window as usize))
        .unwrap()
        .offset = next;
    context.scroll_dirty.set(true);
    unsafe {
        SetScrollPos(window, SB_HORZ, next[0], 1);
        SetScrollPos(window, SB_VERT, next[1], 1);
    }
    if let Some(control) = context.target(window) {
        context.push(RawEvent::Scroll {
            control,
            offset: next,
        });
    }
}

impl WindowContext {
    fn target(&self, window: HWND) -> Option<NodeId> {
        self.targets.borrow().get(&(window as usize)).copied()
    }

    #[cfg(feature = "windows-gpu")]
    fn invalidate_shaders(&self) {
        for window in self.shader_windows.borrow().iter() {
            unsafe { InvalidateRect(*window as HWND, null_mut(), 0); }
        }
    }
    fn push(&self, event: RawEvent) {
        if self.suppress.get()
            && !matches!(
                &event,
                RawEvent::ImageLoad { .. } |
                RawEvent::Drag {
                    phase: DragPhase::Cancelled,
                    ..
                }
            )
        {
            return;
        }
        let bytes = match &event {
            RawEvent::Hosted { event, data, .. } => event.len() + data.to_string().len() + 64,
            RawEvent::Edit { value, .. } => value.len() + 64,
            RawEvent::Key { event, .. } => event.key.len() + 64,
            RawEvent::Shortcut { id, .. } => id.len() + 64,
            _ => 64,
        };
        #[cfg(feature = "windows-gpu")]
        self.invalidate_shaders();
        let mut events = self.events.borrow_mut();
        if let RawEvent::Drag {
            control,
            phase: DragPhase::Moved,
            screen_position,
        } = &event
        {
            if let Some(RawEvent::Drag {
                control: previous,
                phase: DragPhase::Moved,
                screen_position: position,
            }) = events.back_mut()
            {
                if control == previous {
                    *position = *screen_position;
                    return;
                }
            }
        }
        if self.failed.get()
            || events.len() == MAX_EVENTS
            || bytes > MAX_EVENT_BYTES.saturating_sub(self.event_bytes.get())
        {
            self.failed.set(true);
        } else {
            self.event_bytes.set(self.event_bytes.get() + bytes);
            events.push_back(event);
        }
    }
}

unsafe fn read_text(window: HWND) -> Result<String, String> {
    let length = unsafe { GetWindowTextLengthW(window) }.max(0) as usize;
    if length > MAX_TEXT {
        return Err(String::from("native editor exceeded the text budget"));
    }
    let mut buffer = vec![0u16; length + 1];
    let count = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16(&buffer[..count.max(0) as usize])
        .map_err(|_| String::from(INVALID_NATIVE_UTF16))
}

unsafe fn selection(window: HWND) -> [usize; 2] {
    let mut start = 0u32;
    let mut end = 0u32;
    unsafe {
        SendMessageW(
            window,
            EM_GETSEL,
            (&mut start as *mut u32) as usize,
            (&mut end as *mut u32) as isize,
        );
    }
    [start as usize, end as usize]
}

unsafe fn message_context(mut window: HWND) -> *const WindowContext {
    while !window.is_null() {
        if unsafe { GetWindowLongPtrW(window, GWLP_WNDPROC) } == window_proc as *const () as isize {
            let context =
                unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const WindowContext;
            if !context.is_null() {
                return context;
            }
        }
        window = unsafe { GetParent(window) };
    }
    std::ptr::null()
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    parameter: WPARAM,
    data: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let creation = unsafe { &*(data as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, creation.lpCreateParams as isize);
        }
    }
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const WindowContext;
    if !pointer.is_null() {
        let context = unsafe { &*pointer };
        if let Some(result) = window_manager::message(context, window, message, parameter, data) { return result; }
        if window == context.root.get() {
            if message == WM_COPYDATA { return unsafe { deep_links::receive(context, data) }; }
            if message == tray::MESSAGE { context.tray.message(window, data); return 0; }
            if message != 0 && message == tray::taskbar_created() {
                if context.tray.restart(window).is_err() { context.failed.set(true); }
                if context.badge.restore(window).is_err() { context.failed.set(true); }
                return 0;
            }
            if message != 0 && message == badge::taskbar_button_created() {
                if context.badge.restore(window).is_err() { context.failed.set(true); }
                return 0;
            }
        }
        match message {
            #[cfg(feature = "windows-gpu")]
            WM_PAINT if context.gpu_windows.borrow().contains(&(window as usize)) => {
                let mut paint = PAINTSTRUCT::default();
                unsafe {
                    BeginPaint(window, &mut paint);
                    EndPaint(window, &paint);
                }
                context.gpu_dirty.borrow_mut().insert(window as usize);
                return 0;
            }
            #[cfg(feature = "windows-gpu")]
            WM_ERASEBKGND if context.gpu_windows.borrow().contains(&(window as usize)) => return 1,
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => {
                if let Some(paint) = context.paints.borrow().get(&(data as usize)) {
                    let device = parameter as HDC;
                    unsafe {
                        SetTextColor(
                            device,
                            if IsWindowEnabled(data as HWND) == 0 {
                                GetSysColor(COLOR_GRAYTEXT)
                            } else {
                                paint.foreground
                            },
                        );
                        SetBkColor(device, paint.background);
                        SetBkMode(device, OPAQUE as i32);
                    }
                    return paint.brush as LRESULT;
                }
            }
            WM_ERASEBKGND => {
                if let Some(paint) = context.paints.borrow().get(&(window as usize)) {
                    let mut frame = RECT::default();
                    unsafe {
                        GetClientRect(window, &mut frame);
                        FillRect(parameter as HDC, &frame, paint.brush);
                    }
                    return 1;
                }
            }
            WM_LBUTTONDOWN if context.backdrops.borrow().contains_key(&(window as usize)) => {
                if let Some(control) = context.modal.get() {
                    context.push(RawEvent::Dismiss {
                        control,
                        backdrop: true,
                    });
                }
                return 0;
            }
            WM_PAINT | WM_PRINTCLIENT if context.images.borrow().contains_key(&(window as usize)) => {
                let images = context.images.borrow();
                let image = &images[&(window as usize)];
                let mut paint = PAINTSTRUCT::default();
                let device = if message == WM_PRINTCLIENT { parameter as HDC } else { unsafe { BeginPaint(window, &mut paint) } };
                let mut frame = RECT::default();
                unsafe {
                    GetClientRect(window, &mut frame);
                    let paints = context.paints.borrow();
                    let palette = paints.get(&(window as usize));
                    FillRect(
                        device,
                        &frame,
                        palette.map_or_else(|| GetSysColorBrush(COLOR_WINDOW), |paint| paint.brush),
                    );
                    SetTextColor(
                        device,
                        palette.map_or_else(
                            || GetSysColor(COLOR_WINDOWTEXT),
                            |paint| paint.foreground,
                        ),
                    );
                    if let Some(bitmap) = &image.bitmap {
                        bitmap.paint(device, frame.right, frame.bottom, image.fit, image.position, context.scale.get());
                    } else {
                        let alt = wide(&image.alt);
                        SetBkMode(device, TRANSPARENT as i32);
                        DrawTextW(
                            device,
                            alt.as_ptr(),
                            (alt.len() - 1) as i32,
                            &mut frame,
                            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                        );
                    }
                    if message == WM_PAINT { EndPaint(window, &paint); }
                }
                return 0;
            }
            WM_COMMAND if data != 0 && !context.suppress.get() => {
                let notification = ((parameter >> 16) & 0xffff) as u32;
                let child = data as HWND;
                let Some(control) = context.target(child) else {
                    return 0;
                };
                match notification {
                    CBN_SELCHANGE => context.push(RawEvent::Select {
                        control,
                        index: unsafe { SendMessageW(child, CB_GETCURSEL, 0, 0) },
                    }),
                    BN_CLICKED => context.push(RawEvent::Activate {
                        control,
                        checked: unsafe { SendMessageW(child, BM_GETCHECK, 0, 0) } != 0,
                    }),
                    EN_SETFOCUS | BN_SETFOCUS | CBN_SETFOCUS => {
                        let payload = if unsafe {
                            SendMessageW(window, WM_QUERYUISTATE, 0, 0) as u32 & UISF_HIDEFOCUS == 0
                        } {
                            RawEvent::FocusVisible { control }
                        } else {
                            RawEvent::Focus { control }
                        };
                        context.push(payload)
                    }
                    EN_KILLFOCUS | BN_KILLFOCUS | CBN_KILLFOCUS => {
                        context.push(RawEvent::Blur { control })
                    }
                    EN_CHANGE
                        if !context
                            .compositions
                            .borrow()
                            .contains_key(&(child as usize)) =>
                    unsafe {
                        edit_snapshot(context, child);
                    },
                    _ => {}
                }
                return 0;
            }
            WM_VSCROLL | WM_HSCROLL | WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let state = context.scrolls.borrow().get(&(window as usize)).copied();
                if let Some(state) = state {
                    let axis = usize::from(matches!(message, WM_VSCROLL | WM_MOUSEWHEEL));
                    let mut offset = state.offset;
                    if matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
                        let delta = (parameter >> 16) as u16 as i16 as i32;
                        let mut units = 3u32;
                        unsafe { SystemParametersInfoW(if axis == 1 { SPI_GETWHEELSCROLLLINES } else { SPI_GETWHEELSCROLLCHARS }, 0, (&mut units as *mut u32).cast(), 0); }
                        let distance = if units == u32::MAX { state.page[axis] as f64 }
                            else { units as f64 * 16.0 * context.scale.get() };
                        let delta = (if axis == 1 { -delta } else { delta }) as f64 / 120.0 * distance;
                        let pixels = context.scrolls.borrow_mut().get_mut(&(window as usize)).unwrap().remainder.pixels(axis, delta);
                        offset[axis] = offset[axis].saturating_add(pixels);
                    } else {
                        let mut info = SCROLLINFO {
                            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                            fMask: SIF_TRACKPOS,
                            ..unsafe { std::mem::zeroed() }
                        };
                        unsafe {
                            GetScrollInfo(
                                window,
                                if axis == 1 { SB_VERT } else { SB_HORZ },
                                &mut info,
                            );
                        }
                        offset[axis] = match (parameter & 0xffff) as i32 {
                            SB_LINEUP => offset[axis].saturating_sub(24),
                            SB_LINEDOWN => offset[axis].saturating_add(24),
                            SB_PAGEUP => offset[axis].saturating_sub(state.page[axis]),
                            SB_PAGEDOWN => offset[axis].saturating_add(state.page[axis]),
                            SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
                            SB_TOP => 0,
                            SB_BOTTOM => state.maximum[axis],
                            _ => offset[axis],
                        };
                    }
                    unsafe {
                        scroll_window(context, window, offset);
                    }
                    return 0;
                }
            }
            WM_DPICHANGED if window == context.root.get() => {
                context.scale.set((parameter & 0xffff) as f64 / 96.0);
                context
                    .environment
                    .set(context.environment.get().saturating_add(1));
                if data != 0 {
                    let frame = unsafe { &*(data as *const RECT) };
                    unsafe {
                        SetWindowPos(
                            window,
                            null_mut(),
                            frame.left,
                            frame.top,
                            frame.right - frame.left,
                            frame.bottom - frame.top,
                            SWP_NOACTIVATE | SWP_NOZORDER,
                        );
                    }
                }
                return 0;
            }
            WM_SIZE | WM_SETTINGCHANGE | WM_THEMECHANGED | WM_FONTCHANGE
                if window == context.root.get() =>
            {
                if message != WM_SIZE {
                    context.high_contrast.set(paint::high_contrast());
                    context.palette_dirty.set(true);
                    context.system_fonts_dirty.set(true);
                }
                context
                    .environment
                    .set(context.environment.get().saturating_add(1));
            }
            WM_ACTIVATE if window == context.root.get() => {
                context.environment.set(context.environment.get().saturating_add(1));
            }
            WM_HOTKEY if window == context.root.get() => {
                services::hotkey(context, parameter as i32);
                return 0;
            }
            WM_TIMER if window == context.root.get() && matches!(parameter, REACTIVE_TIMER | web::TIMER) => {
                unsafe { KillTimer(window, parameter); }
                return 0;
            }
            WM_POWERBROADCAST if window == context.root.get() && parameter == 0x12 => {
                context.wake_count.set(context.wake_count.get().saturating_add(1));
                context.environment.set(context.environment.get().saturating_add(1));
            }
            WM_SETFOCUS | WM_KILLFOCUS if window != context.root.get() => {
                if let Some(control) = context.target(window) {
                    context.push(if message == WM_SETFOCUS { RawEvent::FocusVisible { control } } else { RawEvent::Blur { control } });
                }
            }
            WM_CLOSE if window == context.root.get() => {
                if context.defer_close.get() {
                    context.close_requested.set(true);
                    return 0;
                }
                context.closed.set(true);
                unsafe {
                    ShowWindow(window, SW_HIDE);
                }
                return 0;
            }
            WM_DESTROY if window == context.root.get() => {
                context.closed.set(true);
                context.root.set(null_mut());
            }
            WM_NCDESTROY => unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            },
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, parameter, data) }
}

fn register_class() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            let class = wide("JadixNativeHost");
            let definition = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: unsafe { GetModuleHandleW(null_mut()) },
                lpszClassName: class.as_ptr(),
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
                ..unsafe { std::mem::zeroed() }
            };
            if unsafe { RegisterClassW(&definition) } == 0 {
                Err(native_error("RegisterClassW"))
            } else {
                Ok(())
            }
        })
        .clone()
}

struct Control {
    tree_accessibility: Option<accessibility::TreeAccessibility>,
    grid_accessibility: Option<accessibility::GridAccessibility>,
    window: HWND,
    identifier: u16,
    parent: Option<NodeId>,
    descriptor: NodeDescriptor,
    revision: u64,
    font: Font,
    initialized: bool,
    accessible_role: bool,
    image_generation: u64,
    shown: bool,
    effective_enabled: bool,
}

pub struct WindowsHost {
    windows: RefCell<window_manager::Windows>,
    owns_updater: bool,
    web: Option<std::rc::Rc<web::Runtime>>,
    webviews: BTreeMap<NodeId, web::Leaf>,
    hosted: BTreeMap<NodeId, Box<hosted::Leaf>>,
    services: Option<services::Services>,
    updater: Option<std::rc::Rc<updater::Runtime>>,
    #[cfg(feature = "windows-gpu")]
    plots: BTreeMap<NodeId, gpu::Plot>,
    popovers: BTreeMap<NodeId, HWND>,
    backdrops: BTreeMap<NodeId, HWND>,
    modal_returns: BTreeMap<NodeId, HWND>,
    images: Option<image::Loader>,
    accessibility: accessibility::Accessibility,
    window: HWND,
    context: Box<WindowContext>,
    controls: BTreeMap<NodeId, Control>,
    fonts: fonts::FontPool,
    root: Option<RootId>,
    revision: u64,
    next_control: u16,
    applied_scale: f64,
    fonts_dirty: bool,
    stats: WindowsStats,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowsStats {
    pub commits: u64,
    pub creates: u64,
    pub updates: u64,
    pub moves: u64,
    pub removes: u64,
    pub positions: u64,
}

impl WindowsHost {
    /// Configure an installed application's activation before showing its root.
    /// False means the launch was handed to the primary; dispose this hidden root
    /// and return from the executable. Registration is explicit and per-user.
    pub fn configure_activation(&mut self, config: jadix_core::deep_link::ApplicationActivation) -> Result<bool, String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        config.validate().map_err(String::from)?;
        {
            let links = self.context.deep_links.borrow();
            if let Some(existing) = &links.activation {
                if existing.config == config { return Ok(true); }
                return Err("application activation is already configured".into());
            }
        }
        let Some(registration) = activation::Registration::start(self.window, config.clone())? else { return Ok(false); };
        let launch = jadix_core::deep_link::ApplicationLaunch::current().map_err(String::from)?;
        self.services.as_ref().ok_or("native services disposed")?.notifications.set_identity_arguments(Some(&config.identifier), &config.arguments)?;
        registration.attach()?;
        let mut links = self.context.deep_links.borrow_mut();
        // Activation derives both protocol and file links from this same bounded
        // launch, so configure must not independently replay process arguments.
        links.configure_activation(&config, &launch, &self.context.desktop)?;
        links.launches.push_back(launch);
        links.activation = Some(registration);
        Ok(true)
    }
    pub fn register_activation_protocols(config: &jadix_core::deep_link::ApplicationActivation) -> Result<(), String> { activation::protocols(config, true) }
    pub fn unregister_activation_protocols(config: &jadix_core::deep_link::ApplicationActivation) -> Result<(), String> { activation::protocols(config, false) }
    pub fn register_activation_files(config: &jadix_core::deep_link::ApplicationActivation) -> Result<(), String> { activation::files(config, true) }
    pub fn unregister_activation_files(config: &jadix_core::deep_link::ApplicationActivation) -> Result<(), String> { activation::files(config, false) }
    pub fn take_application_launches(&mut self) -> Vec<jadix_core::deep_link::ApplicationLaunch> { self.context.deep_links.borrow_mut().launches.drain(..).collect() }
    pub fn register_window_factory(&mut self, name: impl Into<String>, factory: impl Fn(WindowsHost, Option<&str>) -> Result<Box<dyn WindowApplication>, String> + 'static) -> Result<(), String> {
        self.windows.get_mut().register(name.into(), std::rc::Rc::new(factory))
    }

    pub fn window_state(&self, id: Option<String>) -> Result<jadix_core::window::WindowState, String> {
        window_manager::state(self, id)
    }

    /// Forward a protocol launch to an existing window of this executable before
    /// creating a new root. A false result means the caller should start normally.
    pub fn forward_deep_link(config: &jadix_core::deep_link::DeepLinkConfig, url: &str) -> Result<bool, String> {
        deep_links::forward(config, url, null_mut())
    }
    /// Deliver an OS activation supplied by an embedding application's event loop.
    pub fn deliver_deep_link(&mut self, link: jadix_core::deep_link::DeepLink) -> Result<(), String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        self.context.deep_links.borrow_mut().deliver(link, &self.context.desktop)?;
        (self.waker())(); Ok(())
    }
    pub fn new(title: &str, size: [u32; 2]) -> Result<Self, String> {
        if run_update_helper()? { std::process::exit(0); }
        Self::create_window(title, size, null_mut())
    }

    #[doc = include_str!("windows/embedding.md")]
    pub unsafe fn embedded(parent: usize, size: [u32; 2]) -> Result<Self, String> {
        let parent = parent as HWND;
        if parent.is_null()
            || unsafe { IsWindow(parent) } == 0
            || unsafe { GetWindowThreadProcessId(parent, null_mut()) }
                != unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
        {
            return Err(String::from(
                "native embedding requires a live parent owned by this UI thread",
            ));
        }
        Self::create_window("Jadix embedded native root", size, parent)
    }

    pub fn register_web_resources(&mut self, authority: &str, provider: impl Fn(jadix_core::web_resource::ResourceRequest, jadix_core::web_resource::ResourceReply) + Send + Sync + 'static) -> Result<(), &'static str> {
        self.web.as_ref().ok_or("web host disposed")?.resources.borrow_mut().register(authority, provider)
    }
    pub fn set_asset_root(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref().canonicalize().map_err(|e| e.to_string())?;
        if !path.is_dir() { return Err("asset root is not a directory".into()); }
        self.web.as_ref().ok_or("web host disposed")?.set_asset_root(path.clone());
        if let Some(services) = &self.services { services.audio.borrow_mut().set_root(path); }
        Ok(())
    }
    /// Enable signed application updates for a directory installation. The
    /// installation must name this running executable. Call before mounting the
    /// native root so the script bridge receives the configured service.
    pub fn configure_updater(&mut self, config: jadix_core::updater::Config, installation: jadix_core::updater::installation::Installation) -> Result<(), String> {
        if self.is_closed() || self.updater.is_some() { return Err("updater host closed or already configured".into()); }
        if installation.executable().canonicalize().map_err(|e| e.to_string())? != std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| e.to_string())? {
            return Err("update installation must contain the running executable".into());
        }
        let curl = services::system_curl()?.to_str().ok_or("system curl path is not Unicode")?.to_owned();
        self.updater = Some(updater::Runtime::new(config, installation, self.native_window(), curl, self.waker())?);
        Ok(())
    }

    pub fn native_window(&self) -> usize {
        self.context.root.get() as usize
    }

    pub fn window_frame(&self) -> Result<Rect, String> {
        let _dpi = DpiScope::window(self.window);
        let mut bounds: RECT = unsafe { std::mem::zeroed() };
        let result = unsafe { GetWindowRect(self.window, &mut bounds) };
        if result == 0 {
            return Err(native_error("GetWindowRect"));
        }
        Ok(Rect {
            x: f64::from(bounds.left),
            y: f64::from(bounds.top),
            width: f64::from(bounds.right - bounds.left),
            height: f64::from(bounds.bottom - bounds.top),
        })
    }

    pub fn set_window_frame(&self, bounds: Rect) -> Result<(), String> {
        if self.is_closed()
            || unsafe { GetWindowLongPtrW(self.window, GWL_STYLE) } as u32 & WS_CHILD != 0
        {
            return Err(String::from(
                "window placement requires a live standalone native root",
            ));
        }
        if [bounds.x, bounds.y]
            .into_iter()
            .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
            || [bounds.width, bounds.height]
                .into_iter()
                .any(|value| !value.is_finite() || !(160.0..=16_384.0).contains(&value))
        {
            return Err(String::from("invalid native window bounds"));
        }
        let _dpi = DpiScope::window(self.window);
        let result = unsafe {
            SetWindowPos(
                self.window,
                null_mut(),
                bounds.x.round() as i32,
                bounds.y.round() as i32,
                bounds.width.round() as i32,
                bounds.height.round() as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        if result == 0 {
            return Err(native_error("SetWindowPos"));
        }
        Ok(())
    }

    pub fn defer_close(&self) -> Result<(), String> {
        if self.is_closed()
            || unsafe { GetWindowLongPtrW(self.window, GWL_STYLE) } as u32 & WS_CHILD != 0
        {
            return Err(String::from(
                "deferred close requires a live standalone native root",
            ));
        }
        self.context.defer_close.set(true);
        Ok(())
    }

    pub fn close_requested(&self) -> bool {
        self.context.close_requested.get()
    }

    pub fn resolve_close(&self, allow: bool) -> Result<(), String> {
        if self.is_closed() || !self.context.close_requested.replace(false) {
            return Err(String::from("no pending native close request"));
        }
        if allow {
            self.context.closed.set(true);
            unsafe {
                ShowWindow(self.window, SW_HIDE);
            }
        }
        Ok(())
    }

    pub fn request_close(&self) -> Result<(), String> {
        if self.is_closed() || unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) } == 0 {
            return Err(String::from("native close request could not be posted"));
        }
        Ok(())
    }

    pub fn waker(&self) -> impl Fn() + Send + Sync + 'static {
        let window = self.native_window();
        move || unsafe {
            PostMessageW(window as HWND, WM_NULL, 0, 0);
        }
    }

    fn create_window(title: &str, size: [u32; 2], parent: HWND) -> Result<Self, String> {
        register_class()?;
        let accessibility = accessibility::Accessibility::new()?;
        if size.contains(&0) || size.iter().any(|value| *value > i32::MAX as u32) {
            return Err(String::from("native window size is invalid"));
        }
        let mut context = Box::new(WindowContext {
            window_policy: Default::default(),
            badge: Default::default(),
            tray: Default::default(),
            desktop: Default::default(),
            deep_links: Default::default(),
            hotkeys: RefCell::new(BTreeMap::new()),
            wake_count: Cell::new(0),
            drag: drag::DragTracker::default(),
            #[cfg(feature = "windows-gpu")]
            gpu_windows: RefCell::new(BTreeSet::new()),
            #[cfg(feature = "windows-gpu")]
            gpu_dirty: RefCell::new(BTreeSet::new()),
            #[cfg(feature = "windows-gpu")]
            shader_windows: RefCell::new(BTreeSet::new()),
            navigation: RefCell::new(BTreeSet::new()),
            grid_navigation: RefCell::new(BTreeSet::new()),
            keyboard: RefCell::new(BTreeMap::new()),
            paints: RefCell::new(BTreeMap::new()),
            palette_dirty: Cell::new(true),
            system_fonts_dirty: Cell::new(false),
            high_contrast: Cell::new(paint::high_contrast()),
            popovers: RefCell::new(Vec::new()),
            modal: Cell::new(None),
            backdrops: RefCell::new(BTreeMap::new()),
            images: RefCell::new(BTreeMap::new()),
            targets: RefCell::new(BTreeMap::new()),
            root: Cell::new(null_mut()),
            events: RefCell::new(VecDeque::new()),
            event_bytes: Cell::new(0),
            suppress: Cell::new(false),
            failed: Cell::new(false),
            closed: Cell::new(false),
            defer_close: Cell::new(false),
            close_requested: Cell::new(false),
            environment: Cell::new(1),
            scrolls: RefCell::new(BTreeMap::new()),
            compositions: RefCell::new(BTreeMap::new()),
            scale: Cell::new(1.0),
            scroll_dirty: Cell::new(false),
        });
        let class = wide("JadixNativeHost");
        let title = wide(title);
        let previous_dpi =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous_dpi.is_null() {
            return Err(native_error("SetThreadDpiAwarenessContext"));
        }
        let window = unsafe {
            CreateWindowExW(
                WS_EX_CONTROLPARENT,
                class.as_ptr(),
                title.as_ptr(),
                if parent.is_null() {
                    WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN
                } else {
                    WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS
                },
                if parent.is_null() { CW_USEDEFAULT } else { 0 },
                if parent.is_null() { CW_USEDEFAULT } else { 0 },
                size[0] as i32,
                size[1] as i32,
                parent,
                null_mut(),
                GetModuleHandleW(null_mut()),
                (&mut *context as *mut WindowContext).cast::<c_void>(),
            )
        };
        unsafe {
            SetThreadDpiAwarenessContext(previous_dpi);
        }
        if window.is_null() {
            return Err(native_error("CreateWindowExW"));
        }
        context.root.set(window);
        let scale = unsafe { GetDpiForWindow(window) }.max(96) as f64 / 96.0;
        context.scale.set(scale);
        let mut host = Self {
            windows: RefCell::new(window_manager::Windows::new(window)),
            owns_updater: true,
            web: None,
            webviews: BTreeMap::new(),
            hosted: BTreeMap::new(),
            services: None,
            updater: None,
            #[cfg(feature = "windows-gpu")]
            plots: BTreeMap::new(),
            popovers: BTreeMap::new(),
            backdrops: BTreeMap::new(),
            modal_returns: BTreeMap::new(),
            images: None,
            accessibility,
            window,
            context,
            controls: BTreeMap::new(),
            fonts: fonts::FontPool::default(),
            root: None,
            revision: 0,
            next_control: 1,
            applied_scale: scale,
            fonts_dirty: false,
            stats: WindowsStats::default(),
        };
        host.services = Some(services::Services::new(&host)?);
        host.web = Some(web::Runtime::new(&host)?);
        Ok(host)
    }

    pub fn show(&self) {
        let _dpi = DpiScope::window(self.window);
        unsafe {
            ShowWindow(self.window, SW_SHOWNOACTIVATE);
        }
    }

    /// Register this application's AUMID and its COM notification activator.
    /// None restores packaged/process identity lookup.
    pub fn set_notification_identity(&mut self, identity: Option<&str>) -> Result<(), String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        self.services.as_ref().ok_or("native services disposed")?.notifications.set_identity(identity)
    }

    pub fn viewport(&self) -> [f32; 2] {
        let _dpi = DpiScope::window(self.window);
        let mut frame = RECT::default();
        unsafe {
            GetClientRect(self.window, &mut frame);
        }
        [
            ((frame.right - frame.left).max(0) as f64 / self.context.scale.get()) as f32,
            ((frame.bottom - frame.top).max(0) as f64 / self.context.scale.get()) as f32,
        ]
    }

    pub fn environment(&self) -> u64 {
        self.context.environment.get()
    }

    #[cfg(feature = "motion")]
    pub fn motion_policy(&self) -> Result<crate::motion::MotionPolicy, String> {
        use crate::motion::MotionPolicy;
        if self.is_closed() || unsafe { IsWindow(self.window) } == 0 {
            return Err(String::from("motion policy requires a live native window"));
        }
        if unsafe { IsIconic(self.window) } != 0 || unsafe { IsWindowVisible(self.window) } == 0 {
            return Ok(MotionPolicy::Paused);
        }
        let mut enabled: i32 = 0;
        if unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                std::ptr::addr_of_mut!(enabled).cast(),
                0,
            )
        } == 0
        {
            return Err(native_error("read client area animation preference"));
        }
        Ok(if enabled != 0 {
            MotionPolicy::Full
        } else {
            MotionPolicy::Reduced
        })
    }

    pub fn stats(&self) -> WindowsStats {
        self.stats
    }

    pub fn diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "platform": "windows", "closed": self.is_closed(),
            "root": self.root.map(RootId::token), "revision": self.revision,
            "scale": self.context.scale.get(), "environment": self.environment(),
            "controls": self.controls.len(), "queued_events": self.context.events.borrow().len(),
            "queued_event_bytes": self.context.event_bytes.get(), "queue_failed": self.context.failed.get(),
            "active_compositions": self.context.compositions.borrow().len(),
            "images_ready": self.context.images.borrow().values().filter(|image| image.bitmap.is_some()).count(),
            "images_failed": self.context.images.borrow().values().filter(|image| image.failed).count(),
            "images_loading": self.context.images.borrow().values().filter(|image| image.bitmap.is_none() && !image.failed).count(),
            "font_resources": self.font_resources(),
            "commits": self.stats.commits, "creates": self.stats.creates,
            "updates": self.stats.updates, "moves": self.stats.moves,
            "removes": self.stats.removes, "positions": self.stats.positions,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.context.closed.get()
    }

    pub fn image_diagnostics(&self, node: NodeId) -> Option<serde_json::Value> {
        let control = self.controls.get(&node)?;
        let images = self.context.images.borrow();
        let image = images.get(&(control.window as usize))?;
        Some(
            serde_json::json!({"decoded_size":image.bitmap.as_ref().map(image::Bitmap::size),"failed":image.failed,"ready":image.bitmap.is_some()}),
        )
    }

    pub fn pump(&self) {
        if let Some(updater) = &self.updater { updater.pump(); }
        let _dpi = DpiScope::window(self.window);
        let mut message = MSG::default();
        while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
            unsafe {
                let context = message_context(message.hwnd);
                if !context.is_null() {
                    let context = &*context;
                    let popup = context.popovers.borrow().last().copied();
                    if keyboard::dispatch(context, &message) { continue; }
                    if message.message == WM_KEYDOWN {
                        if let Some(control) = context
                            .target(message.hwnd)
                            .filter(|node| context.grid_navigation.borrow().contains(node))
                        {
                            let shift =
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10)
                                    < 0;
                            let command =
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11)
                                    < 0;
                            let alt =
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x12)
                                    < 0;
                            let composing = context
                                .compositions
                                .borrow()
                                .contains_key(&(message.hwnd as usize));
                            if let Some(key) =
                                grid_navigation_key(message.wParam, shift, command, alt, composing)
                            {
                                context.push(RawEvent::Navigate {
                                    control,
                                    key,
                                    shift,
                                    command,
                                });
                                continue;
                            }
                        }
                    }
                    if message.message == WM_KEYDOWN {
                        if let Some(control) = context
                            .target(message.hwnd)
                            .filter(|node| context.navigation.borrow().contains(node))
                        {
                            let key = match message.wParam {
                                0x26 => Some(NavigationKey::Up),
                                0x28 => Some(NavigationKey::Down),
                                0x25 => Some(NavigationKey::Left),
                                0x27 => Some(NavigationKey::Right),
                                0x24 => Some(NavigationKey::Home),
                                0x23 => Some(NavigationKey::End),
                                _ => None,
                            };
                            if let Some(key) = key {
                                context.push(RawEvent::Navigate { control, key,
                                    shift: windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) < 0,
                                    command: windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) < 0 });
                                continue;
                            }
                        }
                    }
                    if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize {
                        if context.drag.cancel(context, true) {
                            continue;
                        }
                        if let Some(popup) = popup {
                            context.push(RawEvent::Dismiss {
                                control: popup.node,
                                backdrop: false,
                            });
                            continue;
                        }
                        if let Some(control) = context.modal.get() {
                            context.push(RawEvent::Dismiss {
                                control,
                                backdrop: false,
                            });
                            continue;
                        }
                    }
                    if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize && window_manager::escape(context) { continue; }
                    if matches!(
                        message.message,
                        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_NCLBUTTONDOWN
                    ) {
                        if let Some(popup) = popup {
                            if message.hwnd != popup.window
                                && IsChild(popup.window, message.hwnd) == 0
                                && message.hwnd != popup.anchor
                                && IsChild(popup.anchor, message.hwnd) == 0
                            {
                                context.push(RawEvent::Dismiss {
                                    control: popup.node,
                                    backdrop: true,
                                });
                            }
                        }
                    }
                    let target = context
                        .modal
                        .get()
                        .and_then(|modal| {
                            context
                                .backdrops
                                .borrow()
                                .iter()
                                .find(|(_, identifier)| **identifier == modal)
                                .map(|(window, _)| *window as HWND)
                        })
                        .unwrap_or(context.root.get());
                    if IsDialogMessageW(target, &message) != 0 {
                        continue;
                    }
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if let Some(services) = &self.services { services.pump(&self.context); }
        if let Ok(mut windows) = self.windows.try_borrow_mut() {
            if self.is_closed() { windows.dispose(); } else { windows.pump(self); }
        }
    }

    pub fn wait_for_events(&self, timeout_ms: u32) -> Result<(), String> {
        let timeout_ms = if self.services.as_ref().is_some_and(|s| s.audio.borrow().active()) { timeout_ms.min(50) } else { timeout_ms };
        if unsafe {
            MsgWaitForMultipleObjectsEx(0, null_mut(), timeout_ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
        } == WAIT_FAILED
        {
            return Err(native_error("wait for native input"));
        }
        Ok(())
    }

    pub fn text(&self, node: NodeId) -> Result<String, String> {
        if self.is_closed() {
            return Err(String::from("native window is closed"));
        }
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        unsafe { read_text(control.window) }
    }

    pub fn control_identity(&self, node: NodeId) -> Option<usize> {
        self.controls
            .get(&node)
            .map(|control| control.window as usize)
    }

    pub fn inspect_control(&self, node: NodeId) -> Result<serde_json::Value, String> {
        if self.is_closed() {
            return Err("native window is closed".into());
        }
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        let mut class_name = [0u16; 256];
        let length = unsafe {
            GetClassNameW(
                control.window,
                class_name.as_mut_ptr(),
                class_name.len() as i32,
            )
        };
        if length == 0 {
            return Err(native_error("GetClassNameW"));
        }
        let frame = self.control_frame(node)?;
        Ok(serde_json::json!({
            "node": node.token(), "root": node.root.token(),
            "hwnd": format!("0x{:x}", control.window as usize),
            "class_name": String::from_utf16_lossy(&class_name[..length as usize]),
            "frame": [frame.x, frame.y, frame.width, frame.height],
            "enabled": unsafe { IsWindowEnabled(control.window) != 0 },
            "visible": unsafe { IsWindowVisible(control.window) != 0 },
            "focused": unsafe { GetFocus() == control.window },
            "authority": "observed Win32 HWND state; UIA and assistive technology require separate observation"
        }))
    }

    pub fn control_frame(&self, node: NodeId) -> Result<Rect, String> {
        let _dpi = DpiScope::window(self.window);
        if self.is_closed() {
            return Err(String::from("native window is closed"));
        }
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        let mut frame = RECT::default();
        if unsafe { GetWindowRect(control.window, &mut frame) } == 0 {
            return Err(native_error("GetWindowRect"));
        }
        let mut origin = POINT {
            x: frame.left,
            y: frame.top,
        };
        if unsafe { ScreenToClient(self.window, &mut origin) } == 0 {
            return Err(native_error("ScreenToClient"));
        }
        let scale = self.context.scale.get();
        Ok(Rect {
            x: f64::from(origin.x) / scale,
            y: f64::from(origin.y) / scale,
            width: f64::from(frame.right - frame.left) / scale,
            height: f64::from(frame.bottom - frame.top) / scale,
        })
    }

    pub fn selection(&self, node: NodeId) -> Result<[usize; 2], String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        if !matches!(
            control.descriptor.content,
            NodeContent::Input { .. } | NodeContent::Textarea { .. }
        ) {
            return Err(String::from("native control is not an editor"));
        }
        Ok(unsafe { selection(control.window) })
    }

    pub fn undo(&self, node: NodeId) -> Result<bool, String> {
        self.selection(node)?;
        let control = &self.controls[&node];
        Ok(unsafe { SendMessageW(control.window, EM_UNDO, 0, 0) } != 0)
    }

    pub fn scroll_offset(&self, node: NodeId) -> Result<[f64; 2], String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        self.context
            .scrolls
            .borrow()
            .get(&(control.window as usize))
            .map(|state| {
                state
                    .offset
                    .map(|value| f64::from(value) / self.context.scale.get())
            })
            .ok_or_else(|| String::from("native control is not a scroll view"))
    }

    pub fn scroll_to(&self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        self.scroll_offset(node)?;
        if offset
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > f64::from(i32::MAX))
        {
            return Err(String::from("invalid native scroll offset"));
        }
        unsafe {
            scroll_window(
                &self.context,
                self.controls[&node].window,
                offset.map(|value| (value * self.context.scale.get()).round() as i32),
            );
        }
        Ok(())
    }

    pub fn select_value(&self, node: NodeId, value: &str) -> Result<(), String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        let NodeContent::Select {
            options,
            enabled: true,
            ..
        } = &control.descriptor.content
        else {
            return Err(String::from("native control is not an enabled selector"));
        };
        let index = options
            .iter()
            .position(|option| option.enabled && option.value == value)
            .ok_or("unknown or disabled native option")?;
        unsafe {
            SendMessageW(control.window, CB_SETCURSEL, index, 0);
            SendMessageW(
                GetParent(control.window),
                WM_COMMAND,
                usize::from(control.identifier) | ((CBN_SELCHANGE as usize) << 16),
                control.window as isize,
            );
        }
        Ok(())
    }

    pub fn select_range(&self, node: NodeId, range: [usize; 2]) -> Result<(), String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        if !matches!(
            control.descriptor.content,
            NodeContent::Input { .. } | NodeContent::Textarea { .. }
        ) || !valid_utf16_range(&self.text(node)?, range)
        {
            return Err(String::from("invalid native edit selection"));
        }
        unsafe {
            SendMessageW(control.window, EM_SETSEL, range[0], range[1] as isize);
        }
        Ok(())
    }

    pub fn replace_selection(&self, node: NodeId, value: &str) -> Result<(), String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        if !matches!(
            control.descriptor.content,
            NodeContent::Input { enabled: true, .. } | NodeContent::Textarea { enabled: true, .. }
        ) {
            return Err(String::from("native control is not an enabled editor"));
        }
        if value.contains('\0') || value.encode_utf16().count() > MAX_TEXT {
            return Err(String::from("invalid native edit value"));
        }
        let value = wide(value);
        unsafe {
            SendMessageW(control.window, EM_REPLACESEL, 1, value.as_ptr() as isize);
        }
        Ok(())
    }

    pub fn activate(&self, node: NodeId) -> Result<(), String> {
        let control = self.controls.get(&node).ok_or("unknown native control")?;
        if !control.effective_enabled || !control.shown {
            return Err(String::from("native control is blocked or hidden"));
        }
        if !matches!(
            control.descriptor.content,
            NodeContent::Button { enabled: true, .. }
                | NodeContent::Checkbox { enabled: true, .. }
                | NodeContent::Switch { enabled: true, .. }
        ) {
            return Err(String::from("native control cannot activate"));
        }
        unsafe {
            SendMessageW(control.window, BM_CLICK, 0, 0);
        }
        Ok(())
    }

    pub fn dismiss(&self, node: NodeId, backdrop: bool) -> Result<(), String> {
        let active = self
            .context
            .popovers
            .borrow()
            .last()
            .map(|panel| panel.node)
            .or(self.context.modal.get());
        if self.is_closed() || active != Some(node) {
            return Err(String::from("native dialog is not the active modal"));
        }
        self.context.push(RawEvent::Dismiss {
            control: node,
            backdrop,
        });
        Ok(())
    }

    fn sync_modals(&mut self, roots: &[PresentationNode]) -> Result<(), String> {
        let mut ordered = Vec::new();
        let mut visible = BTreeMap::new();
        let mut modals = Vec::new();
        let mut pending: Vec<_> = roots.iter().rev().map(|node| (node, true)).collect();
        while let Some((node, ancestor_visible)) = pending.pop() {
            let shown = ancestor_visible
                && !matches!(
                    node.descriptor.content,
                    NodeContent::Dialog { open: false, .. }
                        | NodeContent::Popover { open: false, .. }
                );
            visible.insert(node.id, shown);
            ordered.push(node.id);
            if shown
                && matches!(
                    node.descriptor.content,
                    NodeContent::Dialog { open: true, .. }
                )
            {
                modals.push(node.id);
            }
            pending.extend(node.children.iter().rev().map(|child| (child, shown)));
        }
        let modal = modals.last().copied();
        let previous = self.context.modal.replace(modal);
        let focused = unsafe { GetFocus() };
        for node in &modals {
            self.modal_returns.entry(*node).or_insert_with(|| {
                if !focused.is_null() && unsafe { IsChild(self.window, focused) } != 0 {
                    focused
                } else {
                    null_mut()
                }
            });
        }
        // Hide parents before their children, then reveal children before their parents. This
        // keeps a newly committed subtree hidden until its descendants are ready to paint.
        for identifier in &ordered {
            let control = self.controls.get_mut(identifier).ok_or("unknown native modal target")?;
            if control.shown && !visible[identifier] {
                unsafe { ShowWindow(control.window, SW_HIDE) };
                control.shown = false;
            }
        }
        for identifier in ordered.iter().rev() {
            let control = &self.controls[identifier];
            let mut parent = Some(*identifier);
            let mut allowed = modal.is_none();
            while let Some(node) = parent {
                allowed |= modal == Some(node);
                parent = self.controls[&node].parent;
            }
            let enabled = match control.descriptor.content {
                NodeContent::Button { enabled, .. }
                | NodeContent::Input { enabled, .. }
                | NodeContent::Textarea { enabled, .. }
                | NodeContent::Checkbox { enabled, .. }
                | NodeContent::Switch { enabled, .. }
                | NodeContent::Select { enabled, .. } => enabled,
                _ => true,
            };
            let control = self
                .controls
                .get_mut(identifier)
                .ok_or("unknown native modal target")?;
            if !control.shown && visible[identifier] {
                unsafe {
                    ShowWindow(
                        control.window,
                        if visible[identifier] {
                            SW_SHOWNOACTIVATE
                        } else {
                            SW_HIDE
                        },
                    )
                };
                control.shown = visible[identifier];
            }
            if control.effective_enabled != (allowed && enabled) {
                unsafe { EnableWindow(control.window, i32::from(allowed && enabled)) };
                control.effective_enabled = allowed && enabled;
            }
        }
        let mut client = RECT::default();
        unsafe { GetClientRect(self.window, &mut client) };
        for (identifier, backdrop) in &self.backdrops {
            let shown = modals.contains(identifier);
            unsafe {
                EnableWindow(*backdrop, i32::from(modal == Some(*identifier)));
                ShowWindow(*backdrop, if shown { SW_SHOWNOACTIVATE } else { SW_HIDE });
            }
        }
        for identifier in &modals {
            let backdrop = self.backdrops[identifier];
            if unsafe {
                SetWindowPos(
                    backdrop,
                    HWND_TOP,
                    0,
                    0,
                    client.right,
                    client.bottom,
                    SWP_NOACTIVATE,
                )
            } == 0
            {
                return Err(native_error("position native modal backdrop"));
            }
        }
        if previous != modal {
            let returned = previous.and_then(|node| {
                (!modals.contains(&node))
                    .then(|| self.modal_returns.remove(&node))
                    .flatten()
            });
            if let Some(modal) = modal {
                let panel = self.controls[&modal].window;
                if focused != panel && unsafe { IsChild(panel, focused) } == 0 {
                    let restored = returned.filter(|window| {
                        !window.is_null()
                            && unsafe { IsWindow(*window) } != 0
                            && unsafe { IsChild(panel, *window) } != 0
                            && unsafe { IsWindowEnabled(*window) } != 0
                    });
                    let next = restored.unwrap_or_else(|| unsafe {
                        GetNextDlgTabItem(self.backdrops[&modal], null_mut(), 0)
                    });
                    unsafe { SetFocus(if next.is_null() { panel } else { next }) };
                }
            } else if let Some(returned) = returned.filter(|window| {
                !window.is_null()
                    && unsafe { IsWindow(*window) } != 0
                    && unsafe { IsWindowVisible(*window) } != 0
                    && unsafe { IsWindowEnabled(*window) } != 0
            }) {
                unsafe { SetFocus(returned) };
            }
        }
        self.modal_returns
            .retain(|identifier, _| modals.contains(identifier));
        Ok(())
    }

    pub fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        if let Some(web) = &self.web { web.drain_events(&self.context); }
        for leaf in self.hosted.values() { leaf.drain(); }
        let _dpi = DpiScope::window(self.window);
        self.sync_paints()?;
        if let Some(loader) = &mut self.images {
            for completion in loader.drain()? {
                let Some(control) = self
                    .controls
                    .get(&completion.node)
                    .filter(|control| control.image_generation == completion.generation)
                else {
                    continue;
                };
                let mut images = self.context.images.borrow_mut();
                let Some(image) = images.get_mut(&(control.window as usize)) else {
                    continue;
                };
                match completion.result {
                    Ok(pixels) => {
                        let (width, height) = (pixels.width, pixels.height);
                        image.bitmap = Some(image::Bitmap::new(pixels)?);
                        image.failed = false;
                        self.context.push(RawEvent::ImageLoad { control: completion.node, generation: completion.generation, event: ImageLoadEvent::Loaded { width, height } });
                    }
                    Err(message) => {
                        image.bitmap = None;
                        image.failed = true;
                        self.context.push(RawEvent::ImageLoad { control: completion.node, generation: completion.generation, event: ImageLoadEvent::Failed { message } });
                    }
                }
                self.context.push(RawEvent::ImageLoad { control: completion.node, generation: completion.generation, event: ImageLoadEvent::Ended });
                self.context.environment.set(self.context.environment.get().wrapping_add(1));
                unsafe { InvalidateRect(control.window, null_mut(), 1) };
            }
        }
        self.refresh_scroll_positions()?;
        if self.fonts_dirty
            || self.context.system_fonts_dirty.get()
            || self.applied_scale != self.context.scale.get()
        {
            for control in self.controls.values_mut() {
                let font = Font::new(&control.descriptor, self.context.scale.get())?;
                unsafe {
                    SendMessageW(control.window, WM_SETFONT, font.0 as usize, 1);
                }
                control.font = font;
            }
            self.applied_scale = self.context.scale.get();
            self.fonts_dirty = false;
            self.context.system_fonts_dirty.set(false);
        }
        if self.context.failed.get() {
            return Err(String::from(
                "native event queue overflow or invalid native text",
            ));
        }
        let mut pending = std::mem::take(&mut *self.context.events.borrow_mut());
        self.context.event_bytes.set(0);
        for event in &pending {
            if let RawEvent::Focus { control } | RawEvent::FocusVisible { control } = event {
                if self
                    .controls
                    .get(control)
                    .is_some_and(|control| control.effective_enabled && control.shown)
                {
                    self.reveal(*control)?;
                }
            }
        }
        pending.append(&mut self.context.events.borrow_mut());
        self.context.event_bytes.set(0);
        if self.context.failed.get() {
            return Err(String::from(
                "native focus scrolling exceeded the event budget",
            ));
        }
        self.refresh_scroll_positions()?;
        let mut events = Vec::new();
        #[cfg(feature = "windows-gpu")]
        self.redraw_plots()?;
        for event in pending {
            let identifier = match &event {
                RawEvent::ImageLoad { control, .. }
                | RawEvent::Hosted { control, .. }
                | RawEvent::Key { control, .. }
                | RawEvent::Shortcut { control, .. }
                | RawEvent::Tree { control, .. }
                | RawEvent::Drag { control, .. }
                | RawEvent::Navigate { control, .. }
                | RawEvent::Dismiss { control, .. }
                | RawEvent::Activate { control, .. }
                | RawEvent::Focus { control }
                | RawEvent::FocusVisible { control }
                | RawEvent::Blur { control }
                | RawEvent::Submit { control }
                | RawEvent::Edit { control, .. }
                | RawEvent::Select { control, .. }
                | RawEvent::Scroll { control, .. } => *control,
            };
            let Some(control) = self.controls.get_mut(&identifier) else {
                continue;
            };
            if (!control.effective_enabled || !control.shown)
                && !matches!(
                    &event,
                    RawEvent::ImageLoad { .. } | RawEvent::Blur { .. }
                        | RawEvent::Drag {
                            phase: DragPhase::Cancelled,
                            ..
                        }
                )
            {
                continue;
            }
            let payload = match event {
                RawEvent::ImageLoad { generation, event, .. } => {
                    if control.image_generation != generation { continue; }
                    EventPayload::ImageLoad(event)
                }
                RawEvent::Hosted { event, data, .. } => {
                    let NodeContent::Hosted { specification } = &control.descriptor.content else { continue; };
                    if !jadix_core::hosted::HostedContent::from_json(specification).is_ok_and(|spec| spec.accepts_event(&event, &data)) { continue; }
                    EventPayload::Hosted { event, data }
                },
                RawEvent::Key { event, .. } => EventPayload::Key(event),
                RawEvent::Shortcut { id, .. } => EventPayload::Shortcut { id },
                RawEvent::Tree { action, .. } => {
                    if control.descriptor.accessible_role.is_none()
                        && control.descriptor.accessible_expanded.is_some()
                        && matches!(control.descriptor.content, NodeContent::Button { .. })
                        && matches!(action, TreeAction::Expand | TreeAction::Collapse)
                    {
                        EventPayload::Expansion {
                            expanded: action == TreeAction::Expand,
                        }
                    } else {
                        let supported = control.descriptor.accessible_role
                            == Some(AccessibleRole::TreeItem)
                            && matches!(control.descriptor.content, NodeContent::Button { .. })
                            && match action {
                                TreeAction::Expand | TreeAction::Collapse => {
                                    control.descriptor.accessible_expanded.is_some()
                                }
                                _ => control.descriptor.accessible_selected.is_some(),
                            };
                        if !supported {
                            continue;
                        }
                        EventPayload::Tree { action }
                    }
                }
                RawEvent::Drag {
                    phase,
                    screen_position,
                    ..
                } => EventPayload::Drag {
                    phase,
                    screen_position,
                },
                RawEvent::Navigate {
                    key,
                    shift,
                    command,
                    ..
                } => EventPayload::Navigate {
                    key,
                    shift,
                    control: command,
                },
                RawEvent::Dismiss { backdrop, .. } => {
                    let (active, close_on_overlay_click) = match control.descriptor.content {
                        NodeContent::Dialog {
                            open: true,
                            close_on_overlay_click,
                            ..
                        } => (
                            self.context.popovers.borrow().is_empty()
                                && self.context.modal.get() == Some(identifier),
                            close_on_overlay_click,
                        ),
                        NodeContent::Popover {
                            open: true,
                            close_on_outside_click,
                            ..
                        } => (
                            self.context
                                .popovers
                                .borrow()
                                .last()
                                .is_some_and(|panel| panel.node == identifier),
                            close_on_outside_click,
                        ),
                        _ => continue,
                    };
                    if !active || (backdrop && !close_on_overlay_click) {
                        continue;
                    }
                    control.revision = control
                        .revision
                        .checked_add(1)
                        .ok_or("native modal revision exhausted")?;
                    EventPayload::Dismiss {
                        backdrop,
                        revision: control.revision,
                    }
                }
                RawEvent::Scroll { offset, .. } => EventPayload::Scroll {
                    x: f64::from(offset[0]) / self.context.scale.get(),
                    y: f64::from(offset[1]) / self.context.scale.get(),
                },
                RawEvent::Select { index, .. } => {
                    let NodeContent::Select {
                        options,
                        value,
                        enabled: true,
                        ..
                    } = &control.descriptor.content
                    else {
                        continue;
                    };
                    let Some(option) = usize::try_from(index)
                        .ok()
                        .and_then(|index| options.get(index))
                        .filter(|option| option.enabled)
                    else {
                        let selected = options
                            .iter()
                            .position(|option| option.value == *value)
                            .map_or(usize::MAX, |index| index);
                        unsafe {
                            SendMessageW(control.window, CB_SETCURSEL, selected, 0);
                        }
                        continue;
                    };
                    control.revision = control
                        .revision
                        .checked_add(1)
                        .ok_or("native edit revision exhausted")?;
                    EventPayload::Select {
                        value: option.value.clone(),
                        revision: control.revision,
                    }
                }
                RawEvent::Focus { .. } | RawEvent::FocusVisible { .. }
                    if !control.descriptor.can_focus() => continue,
                RawEvent::Focus { .. } => EventPayload::Focus,
                RawEvent::FocusVisible { .. } => EventPayload::FocusVisible,
                RawEvent::Blur { .. } => {
                    // Destroying a focused virtual row can make Win32 temporarily
                    // focus its scroll parent. That HWND is not a portable focus target.
                    if !control.descriptor.can_focus()
                        && !matches!(control.descriptor.content,
                            NodeContent::Button { .. } | NodeContent::Input { .. }
                            | NodeContent::Textarea { .. } | NodeContent::Checkbox { .. }
                            | NodeContent::Switch { .. } | NodeContent::Select { .. })
                    {
                        continue;
                    }
                    EventPayload::Blur
                },
                RawEvent::Submit { .. } => {
                    if !matches!(
                        control.descriptor.content,
                        NodeContent::Input { enabled: true, .. }
                    ) {
                        continue;
                    }
                    control.revision = control
                        .revision
                        .checked_add(1)
                        .ok_or("native edit revision exhausted")?;
                    EventPayload::Submit {
                        revision: control.revision,
                    }
                }
                RawEvent::Activate { checked, .. } => match control.descriptor.content {
                    NodeContent::Button { enabled: true, .. } => EventPayload::Activate,
                    NodeContent::Checkbox { enabled: true, .. }
                    | NodeContent::Switch { enabled: true, .. } => {
                        control.revision = control
                            .revision
                            .checked_add(1)
                            .ok_or("native edit revision exhausted")?;
                        EventPayload::Toggle {
                            checked,
                            revision: control.revision,
                        }
                    }
                    _ => continue,
                },
                RawEvent::Edit {
                    value,
                    selection,
                    composing,
                    ..
                } => {
                    if !matches!(
                        control.descriptor.content,
                        NodeContent::Input { enabled: true, .. }
                            | NodeContent::Textarea { enabled: true, .. }
                    ) {
                        continue;
                    }
                    control.revision = control
                        .revision
                        .checked_add(1)
                        .ok_or("native edit revision exhausted")?;
                    EventPayload::Edit {
                        value,
                        revision: control.revision,
                        selection_utf16: selection,
                        composing_utf16: composing,
                    }
                }
            };
            events.push(PlatformEvent {
                node: identifier,
                payload,
            });
        }
        Ok(events)
    }

    pub fn reveal(&self, node: NodeId) -> Result<(), String> {
        let target = self
            .controls
            .get(&node)
            .ok_or("unknown native focus target")?;
        let mut parent = if matches!(
            target.descriptor.content,
            NodeContent::Dialog { .. } | NodeContent::Popover { .. }
        ) {
            None
        } else {
            target.parent
        };
        while let Some(identifier) = parent {
            let control = self
                .controls
                .get(&identifier)
                .ok_or("unknown native focus ancestor")?;
            if matches!(
                control.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            ) {
                break;
            }
            let state = self
                .context
                .scrolls
                .borrow()
                .get(&(control.window as usize))
                .copied();
            if let Some(state) = state {
                let target = self.display_frame(node);
                let viewport = self.display_frame(identifier);
                let start = [target.x - viewport.x, target.y - viewport.y];
                let extent = [target.width, target.height];
                let scale = self.context.scale.get();
                let next = std::array::from_fn(|axis| {
                    let page = f64::from(state.page[axis]) / scale;
                    let delta = crate::presentation::input::focus_reveal_delta(
                        start[axis],
                        start[axis] + extent[axis],
                        page,
                    );
                    state.offset[axis].saturating_add((delta * scale).round() as i32)
                });
                unsafe { scroll_window(&self.context, control.window, next) };
            }
            parent = control.parent;
        }
        Ok(())
    }

    fn parent_window(&self, parent: Option<NodeId>) -> Result<HWND, String> {
        match parent {
            Some(node) => self
                .controls
                .get(&node)
                .map(|control| control.window)
                .ok_or_else(|| String::from("unknown native parent")),
            None => Ok(self.window),
        }
    }

    fn create(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        descriptor: &NodeDescriptor,
    ) -> Result<(), String> {
        if self.controls.contains_key(&node) {
            return Err(String::from("native node was already created"));
        }
        let identifiers: BTreeSet<_> = self
            .controls
            .values()
            .map(|control| control.identifier)
            .collect();
        let identifier = (0..u16::MAX)
            .find_map(|_| {
                let candidate = self.next_control;
                self.next_control = candidate.checked_add(1).unwrap_or(1);
                (!identifiers.contains(&candidate)).then_some(candidate)
            })
            .ok_or("native simultaneous control identity exhausted")?;
        let (class, style) = match descriptor.content {
            NodeContent::Hosted { .. } => ("JadixNativeHost", WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_TABSTOP),
            NodeContent::Container | NodeContent::Dialog { .. } | NodeContent::Popover { .. } => {
                ("JadixNativeHost", WS_CLIPCHILDREN | WS_CLIPSIBLINGS)
            }
            NodeContent::Scroll => (
                "JadixNativeHost",
                WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_VSCROLL | WS_HSCROLL,
            ),
            NodeContent::Select { .. } => (
                "COMBOBOX",
                WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32 | CBS_NOINTEGRALHEIGHT as u32,
            ),
            NodeContent::Text(_) => ("STATIC", 0),
            NodeContent::Image { .. } => ("JadixNativeHost", WS_CLIPSIBLINGS),
            NodeContent::Button { .. } => (
                "BUTTON",
                WS_TABSTOP
                    | BS_NOTIFY as u32
                    | if matches!(
                        descriptor.accessible_role,
                        Some(AccessibleRole::RadioItem { .. })
                    ) {
                        BS_RADIOBUTTON as u32
                    } else {
                        BS_PUSHBUTTON as u32
                    },
            ),
            NodeContent::Checkbox { .. } | NodeContent::Switch { .. } => (
                "BUTTON",
                WS_TABSTOP | BS_AUTOCHECKBOX as u32 | BS_NOTIFY as u32,
            ),
            NodeContent::Input { password, .. } => (
                "EDIT",
                WS_TABSTOP | ES_AUTOHSCROLL as u32 | if password { ES_PASSWORD as u32 } else { 0 },
            ),
            NodeContent::Textarea { .. } => (
                "EDIT",
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_WANTRETURN as u32,
            ),
        };
        let font = Font::new(descriptor, self.context.scale.get())?;
        let class = wide(class);
        let native_parent = if matches!(descriptor.content, NodeContent::Dialog { .. }) {
            let backdrop = unsafe {
                CreateWindowExW(
                    WS_EX_CONTROLPARENT,
                    wide("JadixNativeHost").as_ptr(),
                    wide("").as_ptr(),
                    WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                    0,
                    0,
                    1,
                    1,
                    self.window,
                    null_mut(),
                    GetModuleHandleW(null_mut()),
                    (&*self.context as *const WindowContext).cast::<c_void>(),
                )
            };
            if backdrop.is_null() {
                return Err(native_error("create native modal backdrop"));
            }
            self.backdrops.insert(node, backdrop);
            self.context
                .backdrops
                .borrow_mut()
                .insert(backdrop as usize, node);
            backdrop
        } else if matches!(descriptor.content, NodeContent::Popover { .. }) {
            self.portal_parent(parent)?
        } else {
            self.parent_window(parent)?
        };
        let window = unsafe {
            CreateWindowExW(
                if matches!(
                    descriptor.content,
                    NodeContent::Hosted { .. }
                        | NodeContent::Container
                        | NodeContent::Scroll
                        | NodeContent::Dialog { .. }
                        | NodeContent::Popover { .. }
                ) {
                    WS_EX_CONTROLPARENT
                } else {
                    0
                },
                class.as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | style,
                0,
                0,
                1,
                1,
                native_parent,
                identifier as usize as HMENU,
                GetModuleHandleW(null_mut()),
                (&*self.context as *const WindowContext).cast::<c_void>(),
            )
        };
        if window.is_null() {
            return Err(native_error("create native control"));
        }
        if unsafe { SetWindowSubclass(window, Some(button_content_proc), 2, 0) } == 0 {
            unsafe { DestroyWindow(window); }
            return Err(native_error("subclass native control content"));
        }
        if matches!(descriptor.content, NodeContent::Popover { .. }) {
            self.popovers.insert(node, window);
        }
        self.context
            .targets
            .borrow_mut()
            .insert(window as usize, node);
        if matches!(descriptor.content, NodeContent::Scroll) {
            self.context
                .scrolls
                .borrow_mut()
                .insert(window as usize, ScrollState::default());
        }
        if matches!(
            descriptor.content,
            NodeContent::Input { .. } | NodeContent::Textarea { .. }
        ) && unsafe {
            SetWindowSubclass(
                window,
                Some(edit_proc),
                1,
                (&*self.context as *const WindowContext) as usize,
            )
        } == 0
        {
            unsafe {
                DestroyWindow(window);
            }
            return Err(native_error("subclass native editor"));
        }
        self.controls.insert(
            node,
            Control {
                tree_accessibility: None,
                grid_accessibility: None,
                window,
                identifier,
                parent,
                descriptor: descriptor.clone(),
                revision: 0,
                font,
                initialized: false,
                accessible_role: false,
                image_generation: 0,
                shown: false,
                effective_enabled: true,
            },
        );
        self.update(node, descriptor)
    }

    fn update(&mut self, node: NodeId, descriptor: &NodeDescriptor) -> Result<(), String> {
        let mut ancestor = self.controls.get(&node).and_then(|control| control.parent);
        let mut tree_container = 0;
        let mut container_provider = None;
        while let Some(parent) = ancestor {
            let control = self
                .controls
                .get(&parent)
                .ok_or("unknown accessible tree ancestor")?;
            let container_role = if descriptor.accessible_role == Some(AccessibleRole::TabItem) {
                AccessibleRole::TabList
            } else {
                AccessibleRole::Tree
            };
            if control.descriptor.accessible_role == Some(container_role) {
                tree_container = control.window as usize;
                container_provider = control
                    .tree_accessibility
                    .as_ref()
                    .map(|provider| provider.container());
                break;
            }
            ancestor = control.parent;
        }
        if matches!(descriptor.content, NodeContent::Image { .. }) && self.images.is_none() {
            self.images = Some(image::Loader::new(self.window)?);
        }
        let control = self
            .controls
            .get_mut(&node)
            .ok_or("unknown native update")?;
        let old_style = &control.descriptor.style;
        let style = &descriptor.style;
        let font = if old_style.font_family != style.font_family
            || old_style.font_size != style.font_size
            || old_style.font_weight != style.font_weight
        {
            Some(Font::new(descriptor, self.context.scale.get())?)
        } else {
            None
        };
        let (text, enabled, revision) = match &descriptor.content {
            NodeContent::Hosted { specification } => {
                if !control.initialized || control.descriptor.content != descriptor.content {
                    let spec = jadix_core::hosted::HostedContent::from_json(specification)?;
                    if hosted::Leaf::supports(&spec) {
                        self.webviews.remove(&node);
                        if let Some(leaf) = self.hosted.get(&node) { leaf.update(spec)?; }
                        else {
                            let root = self.web.as_ref().ok_or("web host disposed")?.asset_root();
                            self.hosted.insert(node, hosted::Leaf::new(node, control.window, spec, &self.context, root)?);
                        }
                    } else {
                        self.hosted.remove(&node);
                        self.webviews.remove(&node);
                        let runtime = self.web.as_ref().ok_or("web host disposed")?.clone();
                        let leaf = web::Leaf::new(node, control.window, spec, runtime)?;
                        self.webviews.insert(node, leaf);
                    }
                }
                ("", true, 0)
            },
            NodeContent::Image { source, alt } => {
                let changed = match &control.descriptor.content {
                    NodeContent::Image {
                        source: previous, ..
                    } => !control.initialized || previous != source,
                    _ => true,
                };
                if changed {
                    control.image_generation = self
                        .images
                        .as_mut()
                        .ok_or("native image loader missing")?
                        .request(node, source, &self.web.as_ref().ok_or("web host disposed")?.asset_root().lock().unwrap())?;
                    self.context.images.borrow_mut().insert(
                        control.window as usize,
                        NativeImage {
                            bitmap: None,
                            alt: alt.clone(),
                            failed: false,
                            fit: descriptor.style.image_fit.unwrap_or_default(),
                            position: descriptor.style.image_position.unwrap_or_default(),
                        },
                    );
                    self.context.push(RawEvent::ImageLoad { control: node, generation: control.image_generation, event: ImageLoadEvent::Started });
                } else if let Some(image) = self
                    .context
                    .images
                    .borrow_mut()
                    .get_mut(&(control.window as usize))
                {
                    image.alt = alt.clone();
                    image.fit = descriptor.style.image_fit.unwrap_or_default();
                    image.position = descriptor.style.image_position.unwrap_or_default();
                }
                unsafe { InvalidateRect(control.window, null_mut(), 1) };
                (alt.as_str(), true, 0)
            }
            NodeContent::Text(text) => (text.as_str(), true, 0),
            NodeContent::Button { label, enabled } => (label.as_str(), *enabled, 0),
            NodeContent::Checkbox {
                label,
                enabled,
                checked,
                revision,
            }
            | NodeContent::Switch {
                label,
                enabled,
                checked,
                revision,
            } => {
                unsafe {
                    SendMessageW(control.window, BM_SETCHECK, usize::from(*checked), 0);
                }
                (label.as_str(), *enabled, *revision)
            }
            NodeContent::Input {
                value,
                enabled,
                revision,
                ..
            }
            | NodeContent::Textarea {
                value,
                enabled,
                revision,
                ..
            } => (value.as_str(), *enabled, *revision),
            NodeContent::Select {
                options,
                value,
                placeholder,
                enabled,
                revision,
            } => {
                let previous = match &control.descriptor.content {
                    NodeContent::Select { options, .. } => Some(options),
                    _ => None,
                };
                let count = unsafe { SendMessageW(control.window, CB_GETCOUNT, 0, 0) };
                if previous != Some(options) || count != options.len() as isize {
                    unsafe {
                        SendMessageW(control.window, CB_RESETCONTENT, 0, 0);
                    }
                    for option in options {
                        let label = wide(&option.label);
                        if unsafe {
                            SendMessageW(control.window, CB_ADDSTRING, 0, label.as_ptr() as isize)
                        } < 0
                        {
                            return Err(String::from("native selector allocation failed"));
                        }
                    }
                }
                let selected = options
                    .iter()
                    .position(|option| option.value == *value)
                    .unwrap_or(usize::MAX);
                let cue = wide(placeholder);
                unsafe {
                    SendMessageW(control.window, CB_SETCURSEL, selected, 0);
                    SendMessageW(control.window, CB_SETCUEBANNER, 0, cue.as_ptr() as isize);
                }
                ("", *enabled, *revision)
            }
            NodeContent::Container | NodeContent::Scroll => ("", true, 0),
            NodeContent::Dialog { revision, .. } | NodeContent::Popover { revision, .. } => {
                ("", true, *revision)
            }
        };
        if text.contains('\0') {
            return Err(String::from("native text cannot contain NUL"));
        }
        if !matches!(descriptor.content, NodeContent::Select { .. })
            && !self
                .context
                .compositions
                .borrow()
                .contains_key(&(control.window as usize))
            && unsafe { read_text(control.window) }? != text
        {
            let value = wide(text);
            if unsafe { SetWindowTextW(control.window, value.as_ptr()) } == 0 {
                return Err(native_error("SetWindowTextW"));
            }
        }
        unsafe {
            {
                let tab_stop = descriptor.can_focus() && match descriptor.accessible_role {
                    Some(AccessibleRole::TabItem) => descriptor.accessible_selected == Some(true),
                    Some(AccessibleRole::ListBoxOption { tab_stop }) => tab_stop,
                    Some(AccessibleRole::RadioItem { tab_stop }) => tab_stop,
                    _ => true,
                };
                let current = GetWindowLongPtrW(control.window, GWL_STYLE) as u32;
                let next = if tab_stop {
                    current | WS_TABSTOP
                } else {
                    current & !WS_TABSTOP
                };
                if current != next {
                    SetWindowLongPtrW(control.window, GWL_STYLE, next as isize);
                }
            }
            if control.effective_enabled != enabled {
                EnableWindow(control.window, i32::from(enabled));
                control.effective_enabled = enabled;
            }
            if !control.initialized || font.is_some() {
                SendMessageW(
                    control.window,
                    WM_SETFONT,
                    font.as_ref().unwrap_or(&control.font).0 as usize,
                    1,
                );
            }
            if let NodeContent::Input { password, .. } = descriptor.content {
                SendMessageW(
                    control.window,
                    EM_SETPASSWORDCHAR,
                    if password { 0x25cf } else { 0 },
                    0,
                );
            }
            if matches!(
                descriptor.content,
                NodeContent::Input { .. } | NodeContent::Textarea { .. }
            ) {
                SendMessageW(control.window, EM_SETLIMITTEXT, MAX_TEXT, 0);
                let placeholder = match &descriptor.content {
                    NodeContent::Input { placeholder, .. }
                    | NodeContent::Textarea { placeholder, .. } => wide(placeholder),
                    _ => unreachable!(),
                };
                SendMessageW(
                    control.window,
                    EM_SETCUEBANNER,
                    0,
                    placeholder.as_ptr() as isize,
                );
            }
        }
        if let Some(font) = font {
            control.font = font;
        }
        if descriptor.accessible_name != control.descriptor.accessible_name
            || matches!(descriptor.content, NodeContent::Image { .. })
            || (!control.initialized && descriptor.accessible_name.is_some())
        {
            let name = descriptor
                .accessible_name
                .as_deref()
                .or(match &descriptor.content {
                    NodeContent::Image { alt, .. } => Some(alt.as_str()),
                    _ => None,
                });
            self.accessibility.name(control.window, name)?;
        }
        if descriptor.accessible_description != control.descriptor.accessible_description
            || (!control.initialized && descriptor.accessible_description.is_some())
        {
            self.accessibility
                .description(control.window, descriptor.accessible_description.as_deref())?;
        }
        if matches!(descriptor.content, NodeContent::Button { .. })
            && matches!(
                descriptor.accessible_role,
                Some(AccessibleRole::RadioItem { .. })
            ) != matches!(
                control.descriptor.accessible_role,
                Some(AccessibleRole::RadioItem { .. })
            )
        {
            let button_style = if matches!(
                descriptor.accessible_role,
                Some(AccessibleRole::RadioItem { .. })
            ) {
                BS_RADIOBUTTON
            } else {
                BS_PUSHBUTTON
            };
            unsafe {
                SendMessageW(
                    control.window,
                    BM_SETSTYLE,
                    (BS_NOTIFY | button_style) as usize,
                    1,
                );
            }
        }
        let role = match descriptor.accessible_role {
            Some(AccessibleRole::RadioItem { .. })
                if matches!(descriptor.content, NodeContent::Button { .. }) =>
            {
                unsafe {
                    SendMessageW(
                        control.window,
                        BM_SETCHECK,
                        usize::from(descriptor.accessible_selected == Some(true)),
                        0,
                    );
                }
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_RADIOBUTTON as i32)
            }
            Some(AccessibleRole::RadioGroup) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_GROUPING as i32)
            }
            Some(AccessibleRole::RadioItem { .. }) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_RADIOBUTTON as i32)
            }
            Some(AccessibleRole::TabList) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_PAGETABLIST as i32)
            }
            Some(AccessibleRole::TabItem) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_PAGETAB as i32)
            }
            Some(AccessibleRole::TabPanel) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_PROPERTYPAGE as i32)
            }
            Some(AccessibleRole::ListBox) => Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_LIST as i32),
            Some(AccessibleRole::ListBoxOption { .. }) => Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_LISTITEM as i32),
            Some(AccessibleRole::Heading) => Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_STATICTEXT as i32),
            Some(AccessibleRole::Tree) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_OUTLINE as i32)
            }
            Some(AccessibleRole::TreeItem) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_OUTLINEITEM as i32)
            }
            Some(AccessibleRole::Grid { .. }) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_TABLE as i32)
            }
            Some(AccessibleRole::GridCell { .. }) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_CELL as i32)
            }
            Some(AccessibleRole::GridColumnHeader { .. }) => {
                Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_COLUMNHEADER as i32)
            }
            None => match descriptor.content {
                NodeContent::Image { .. } => {
                    Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_GRAPHIC as i32)
                }
                NodeContent::Dialog { .. } => {
                    Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_DIALOG as i32)
                }
                NodeContent::Popover { .. } => {
                    Some(windows::Win32::UI::Accessibility::ROLE_SYSTEM_GROUPING as i32)
                }
                _ => None,
            },
        };
        if control.accessible_role || role.is_some() {
            self.accessibility.role(control.window, role)?;
            control.accessible_role = role.is_some();
        }
        if (!control.initialized
            || descriptor.accessible_selected != control.descriptor.accessible_selected
            || descriptor.accessible_expanded != control.descriptor.accessible_expanded)
            && (descriptor.accessible_selected.is_some()
                || descriptor.accessible_expanded.is_some()
                || control.descriptor.accessible_selected.is_some()
                || control.descriptor.accessible_expanded.is_some())
        {
            self.accessibility.state(
                control.window,
                if matches!(
                    descriptor.accessible_role,
                    Some(AccessibleRole::RadioItem { .. })
                ) {
                    None
                } else {
                    descriptor.accessible_selected
                },
                descriptor.accessible_expanded,
            )?;
        }
        control.revision = control.revision.max(revision);
        if matches!(
            descriptor.accessible_role,
            Some(
                AccessibleRole::Grid { .. }
                    | AccessibleRole::GridCell { .. }
                    | AccessibleRole::GridColumnHeader { .. }
            )
        ) {
            if let Some(provider) = &control.grid_accessibility {
                provider.update(descriptor);
            } else {
                control.grid_accessibility = Some(accessibility::GridAccessibility::new(
                    control.window,
                    descriptor,
                )?);
            }
        } else {
            control.grid_accessibility = None;
        }
        if (descriptor.accessible_role == Some(AccessibleRole::TabList)
            && matches!(descriptor.content, NodeContent::Container))
            || ((matches!(
                descriptor.accessible_role,
                Some(AccessibleRole::TreeItem | AccessibleRole::TabItem)
            ) || (descriptor.accessible_role.is_none()
                && descriptor.accessible_expanded.is_some()))
                && matches!(descriptor.content, NodeContent::Button { .. }))
        {
            if let Some(provider) = &control.tree_accessibility {
                provider.update(descriptor, tree_container, container_provider);
            } else {
                control.tree_accessibility = Some(accessibility::TreeAccessibility::new(
                    control.window,
                    tree_container,
                    &self.context,
                    descriptor,
                    container_provider,
                )?);
            }
        } else {
            control.tree_accessibility = None;
        }
        if descriptor.accepts_keyboard() {
            self.context.keyboard.borrow_mut().insert(node, descriptor.keyboard.clone());
        } else {
            self.context.keyboard.borrow_mut().remove(&node);
        }
        control.descriptor = descriptor.clone();
        control.initialized = true;
        Ok(())
    }

    fn display_frame(&self, node: NodeId) -> Rect {
        let mut frame = self.controls[&node].descriptor.frame;
        if let NodeContent::Popover {
            anchor,
            placement,
            gap,
            ..
        } = self.controls[&node].descriptor.content
        {
            let position = crate::popover::position(
                self.display_frame(anchor),
                [frame.width, frame.height],
                self.viewport(),
                placement,
                gap,
            );
            frame.x = position[0];
            frame.y = position[1];
            return frame;
        }
        if matches!(
            self.controls[&node].descriptor.content,
            NodeContent::Dialog { .. }
        ) {
            return frame;
        }
        let mut parent = self.controls[&node].parent;
        while let Some(identifier) = parent {
            let control = &self.controls[&identifier];
            if matches!(control.descriptor.content, NodeContent::Popover { .. }) {
                let displayed = self.display_frame(identifier);
                frame.x += displayed.x - control.descriptor.frame.x;
                frame.y += displayed.y - control.descriptor.frame.y;
                break;
            }
            if matches!(control.descriptor.content, NodeContent::Dialog { .. }) {
                break;
            }
            if let Some(scroll) = self
                .context
                .scrolls
                .borrow()
                .get(&(control.window as usize))
            {
                frame.x -= f64::from(scroll.offset[0]) / self.context.scale.get();
                frame.y -= f64::from(scroll.offset[1]) / self.context.scale.get();
            }
            parent = control.parent;
        }
        frame
    }

    fn native_frame(&self, node: NodeId) -> Rect {
        let frame = self.display_frame(node);
        if !self.controls[&node].descriptor.content.accepts_children() {
            return frame;
        }
        let viewport = self.viewport();
        let mut left = 0.0f64;
        let mut top = 0.0f64;
        let mut right = f64::from(viewport[0]);
        let mut bottom = f64::from(viewport[1]);
        let mut parent = if matches!(
            self.controls[&node].descriptor.content,
            NodeContent::Dialog { .. } | NodeContent::Popover { .. }
        ) {
            None
        } else {
            self.controls[&node].parent
        };
        while let Some(identifier) = parent {
            let frame = self.display_frame(identifier);
            left = left.max(frame.x);
            top = top.max(frame.y);
            right = right.min(frame.x + frame.width);
            bottom = bottom.min(frame.y + frame.height);
            if matches!(
                self.controls[&identifier].descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            ) {
                break;
            }
            parent = self.controls[&identifier].parent;
        }
        let x = frame.x.clamp(left, right.max(left));
        let y = frame.y.clamp(top, bottom.max(top));
        Rect {
            x,
            y,
            width: (frame.x + frame.width).min(right).max(x) - x,
            height: (frame.y + frame.height).min(bottom).max(y) - y,
        }
    }

    fn refresh_scroll_positions(&mut self) -> Result<(), String> {
        if self.context.scroll_dirty.replace(false) {
            let mut pending: Vec<_> = self
                .controls
                .iter()
                .filter(|(_, control)| control.parent.is_none())
                .map(|(node, _)| *node)
                .collect();
            while let Some(node) = pending.pop() {
                self.position(node, None)?;
                pending.extend(
                    self.controls
                        .iter()
                        .filter(|(_, control)| control.parent == Some(node))
                        .map(|(node, _)| *node),
                );
            }
            self.raise_overlays()?;
            // Moving clipped parents and their children separately can copy stale
            // client pixels. Repaint the final hierarchy after every scroll batch.
            unsafe {
                RedrawWindow(self.window, null_mut(), null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
            }
        }
        Ok(())
    }

    fn position(&mut self, node: NodeId, previous: Option<HWND>) -> Result<(), String> {
        let control = self.controls.get(&node).ok_or("unknown native position")?;
        let mut frame = self.native_frame(node);
        if let Some(parent) = control.parent.filter(|_| {
            !matches!(
                control.descriptor.content,
                NodeContent::Dialog { .. } | NodeContent::Popover { .. }
            )
        }) {
            let parent = self.native_frame(parent);
            frame.x -= parent.x;
            frame.y -= parent.y;
        }
        let scale = self.context.scale.get();

        frame.x *= scale;
        frame.y *= scale;
        frame.width *= scale;
        frame.height *= scale;
        if frame.width > 32760.0 || frame.height > 32760.0 {
            return Err(String::from("native leaf exceeds Win32 coordinate limits"));
        }
        if unsafe {
            SetWindowPos(
                control.window,
                if matches!(
                    control.descriptor.content,
                    NodeContent::Dialog { .. } | NodeContent::Popover { .. }
                ) {
                    HWND_TOP
                } else {
                    previous.unwrap_or(HWND_TOP)
                },
                frame.x.round().clamp(-32760.0, 32760.0) as i32,
                frame.y.round().clamp(-32760.0, 32760.0) as i32,
                frame.width.round() as i32,
                if matches!(control.descriptor.content, NodeContent::Select { .. }) {
                    frame.height.round() as i32 + 240
                } else {
                    frame.height.round() as i32
                },
                SWP_NOACTIVATE | if previous.is_none() { SWP_NOZORDER } else { 0 },
            )
        } == 0
        {
            return Err(native_error("SetWindowPos"));
        }
        if let Some(web) = self.webviews.get(&node) {
            web.resize(frame.width.round().max(0.0) as u32, frame.height.round().max(0.0) as u32)?;
        }
        if let Some(leaf) = self.hosted.get(&node) { leaf.resize(); }
        self.stats.positions += 1;
        Ok(())
    }

    fn apply(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        if self.is_closed()
            || batch.previous != self.revision
            || batch.revision
                != self
                    .revision
                    .checked_add(1)
                    .ok_or("native revision exhausted")?
            || self.root.is_some_and(|root| root != batch.root)
        {
            return Err(String::from("stale, foreign or closed native batch"));
        }
        if let Some(web) = &self.web { web.evaluations.borrow_mut().sync(roots); }
        let focused = unsafe { GetFocus() };
        let focused_popover = self
            .context
            .popovers
            .borrow()
            .iter()
            .rev()
            .find(|panel| focused == panel.window || unsafe { IsChild(panel.window, focused) } != 0)
            .map(|panel| panel.node);
        let mut geometry = BTreeSet::new();
        let mut ordering = BTreeSet::new();
        for operation in &batch.operations {
            match operation {
                Operation::Create {
                    node,
                    parent,
                    descriptor,
                    ..
                } => {
                    self.create(*node, *parent, descriptor)?;
                    self.context.palette_dirty.set(true);
                    geometry.insert(*node);
                    ordering.insert(*parent);
                    self.stats.creates += 1;
                }
                Operation::Update { node, descriptor } => {
                    if self.controls.get(node).is_some_and(|control| {
                        control.descriptor.style.foreground != descriptor.style.foreground
                            || control.descriptor.style.background != descriptor.style.background
                    }) {
                        self.context.palette_dirty.set(true);
                    }
                    if self
                        .controls
                        .get(node)
                        .is_some_and(|control| control.descriptor.frame != descriptor.frame)
                    {
                        geometry.insert(*node);
                    }
                    self.update(*node, descriptor)?;
                    self.stats.updates += 1;
                }
                Operation::Move { node, parent, .. } => {
                    self.context.palette_dirty.set(true);
                    let window = self.controls.get(node).ok_or("unknown native move")?.window;
                    let destination = if self.popovers.contains_key(node) {
                        self.portal_parent(*parent)?
                    } else {
                        self.backdrops
                            .get(node)
                            .copied()
                            .map(Ok)
                            .unwrap_or_else(|| self.parent_window(*parent))?
                    };
                    let previous_parent = self.controls[node].parent;
                    if previous_parent != *parent
                        && unsafe { SetParent(window, destination) }.is_null()
                    {
                        return Err(native_error("SetParent"));
                    }
                    self.controls.get_mut(node).unwrap().parent = *parent;
                    geometry.insert(*node);
                    ordering.insert(previous_parent);
                    ordering.insert(*parent);
                    self.stats.moves += 1;
                }
                Operation::Remove { node } => {
                    let mut removed = vec![*node];
                    let mut offset = 0;
                    while offset < removed.len() {
                        removed.extend(
                            self.controls
                                .iter()
                                .filter(|(_, control)| control.parent == Some(removed[offset]))
                                .map(|(node, _)| *node)
                                .collect::<Vec<_>>(),
                        );
                        offset += 1;
                    }
                    for node in removed.into_iter().rev() {
                        self.webviews.remove(&node);
                        self.hosted.remove(&node);
                        #[cfg(feature = "windows-gpu")]
                        self.remove_gpu_plot(node);
                        if let Some(control) = self.controls.remove(&node) {
                            self.context.navigation.borrow_mut().remove(&node);
                            self.context.grid_navigation.borrow_mut().remove(&node);
                            self.context.keyboard.borrow_mut().remove(&node);
                            if control.accessible_role {
                                self.accessibility.role(control.window, None)?;
                            }
                            if control.descriptor.accessible_selected.is_some()
                                || control.descriptor.accessible_expanded.is_some()
                            {
                                self.accessibility.state(control.window, None, None)?;
                            }
                            self.context
                                .paints
                                .borrow_mut()
                                .remove(&(control.window as usize));
                            if let Some(loader) = &mut self.images {
                                loader.cancel(node);
                            }
                            self.context
                                .images
                                .borrow_mut()
                                .remove(&(control.window as usize));
                            self.context
                                .targets
                                .borrow_mut()
                                .remove(&(control.window as usize));
                            if control.descriptor.accessible_name.is_some()
                                || matches!(control.descriptor.content, NodeContent::Image { .. })
                            {
                                self.accessibility.name(control.window, None)?;
                            }
                            if control.descriptor.accessible_description.is_some() {
                                self.accessibility.description(control.window, None)?;
                            }
                            self.stats.removes += 1;
                            self.context
                                .scrolls
                                .borrow_mut()
                                .remove(&(control.window as usize));
                            unsafe {
                                DestroyWindow(control.window);
                            }
                            self.popovers.remove(&node);
                            if let Some(backdrop) = self.backdrops.remove(&node) {
                                self.context
                                    .backdrops
                                    .borrow_mut()
                                    .remove(&(backdrop as usize));
                                unsafe { DestroyWindow(backdrop) };
                            }
                        }
                    }
                }
                Operation::Focus { node } => {
                    if let Some(node) = node {
                        unsafe {
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                                self.controls
                                    .get(node)
                                    .ok_or("unknown native focus")?
                                    .window,
                            );
                        }
                    } else {
                        unsafe {
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.window);
                        }
                    }
                }
            }
        }
        let mut pending = vec![(roots, None)];
        while let Some((siblings, parent)) = pending.pop() {
            let mut previous = Some(HWND_TOP);
            for node in siblings {
                if ordering.contains(&parent) {
                    self.position(node.id, previous)?;
                } else if geometry.contains(&node.id)
                    || parent.is_some_and(|parent| geometry.contains(&parent))
                {
                    self.position(node.id, None)?;
                }
                previous = Some(self.controls[&node.id].window);
                pending.push((&node.children, Some(node.id)));
            }
        }
        for control in self.controls.values().filter(|control| {
            matches!(
                control.descriptor.accessible_role,
                Some(AccessibleRole::GridCell { .. } | AccessibleRole::GridColumnHeader { .. })
            )
        }) {
            let mut ancestor = control.parent;
            let mut owner = None;
            while let Some(parent) = ancestor {
                let parent = &self.controls[&parent];
                if matches!(
                    parent.descriptor.accessible_role,
                    Some(AccessibleRole::Grid { .. })
                ) {
                    owner = parent.grid_accessibility.as_ref();
                    break;
                }
                ancestor = parent.parent;
            }
            if let Some(provider) = &control.grid_accessibility {
                provider.link(owner);
            }
        }
        for control in self
            .controls
            .values()
            .filter(|control| matches!(control.descriptor.content, NodeContent::Scroll))
        {
            for axis in 0..2 {
                if !control.descriptor.style.scrollbars[axis] {
                    unsafe {
                        ShowScrollBar(control.window, if axis == 1 { SB_VERT } else { SB_HORZ }, 0);
                    }
                }
            }
            let mut frame = RECT::default();
            unsafe {
                GetClientRect(control.window, &mut frame);
            }
            let page = [
                (frame.right - frame.left).max(0),
                (frame.bottom - frame.top).max(0),
            ];
            let mut extent = page;
            for child in self.controls.values().filter(|child| {
                child
                    .parent
                    .is_some_and(|parent| self.controls[&parent].window == control.window)
            }) {
                extent[0] = extent[0].max(
                    (child.descriptor.frame.x - control.descriptor.frame.x
                        + child.descriptor.frame.width)
                        .mul_add(self.context.scale.get(), 0.0)
                        .ceil() as i32,
                );
                extent[1] = extent[1].max(
                    (child.descriptor.frame.y - control.descriptor.frame.y
                        + child.descriptor.frame.height)
                        .mul_add(self.context.scale.get(), 0.0)
                        .ceil() as i32,
                );
            }
            let previous = self.context.scrolls.borrow()[&(control.window as usize)];
            let state = ScrollState {
                page,
                maximum: std::array::from_fn(|axis| extent[axis].saturating_sub(page[axis])),
                ..previous
            };
            self.context
                .scrolls
                .borrow_mut()
                .insert(control.window as usize, state);
            unsafe {
                scroll_window(&self.context, control.window, previous.offset);
            }
            for axis in 0..2 {
                let info = SCROLLINFO {
                    cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                    nMin: 0,
                    nMax: extent[axis].saturating_sub(1),
                    nPage: page[axis] as u32,
                    nPos: self.context.scrolls.borrow()[&(control.window as usize)].offset[axis],
                    nTrackPos: 0,
                };
                unsafe {
                    SetScrollInfo(
                        control.window,
                        if axis == 1 { SB_VERT } else { SB_HORZ },
                        &info,
                        1,
                    );
                    if !control.descriptor.style.scrollbars[axis] {
                        ShowScrollBar(control.window, if axis == 1 { SB_VERT } else { SB_HORZ }, 0);
                    }
                }
            }
        }
        self.root = Some(batch.root);
        self.sync_paints()?;
        self.refresh_scroll_positions()?;
        self.sync_modals(roots)?;
        self.sync_popovers(focused_popover)?;
        #[cfg(feature = "windows-gpu")]
        { self.redraw_plots()?; self.context.invalidate_shaders(); }
        self.revision = batch.revision;
        self.stats.commits += 1;
        Ok(ApplyReceipt::for_batch(batch))
    }
}

impl PlatformHost for WindowsHost {
    fn configure_activation(&mut self, config: jadix_core::deep_link::ApplicationActivation) -> Result<bool, String> { WindowsHost::configure_activation(self, config) }
    fn take_application_launches(&mut self) -> Vec<jadix_core::deep_link::ApplicationLaunch> { WindowsHost::take_application_launches(self) }
    fn window_declarations(&self) -> Option<jadix_core::window::WindowDeclarations> { Some(self.windows.borrow().declarations()) }

    fn request_window(&mut self, request: &jadix_core::window::WindowRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::window::WindowResult>, String> { self.windows.get_mut().request(request) }

    fn show_share(&mut self, share: &jadix_core::desktop::Share) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>, String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        if share.anchor.is_some_and(|node| !self.controls.contains_key(&node)) { return Err("share anchor is not mounted in this window".into()); }
        self.services.as_ref().ok_or("native sharing is disposed")?.sharing.borrow_mut().request(share)
    }
    fn configure_deep_links(&mut self, config: jadix_core::deep_link::DeepLinkConfig) -> Result<(), String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        self.context.deep_links.borrow_mut().configure(config, &self.context.desktop)
    }
    fn take_deep_links(&mut self) -> Vec<jadix_core::deep_link::DeepLink> {
        self.context.deep_links.borrow_mut().router.drain()
    }
    fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        self.web.as_ref().map_or_else(Vec::new, |runtime| runtime.pump(&self.webviews))
    }
    fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> {
        self.web.as_ref().map_or_else(Vec::new, |runtime| runtime.resources.borrow_mut().take_work())
    }
    fn evaluate_web(&mut self, node: NodeId, script: &str) -> jadix_core::web_evaluation::WebEvaluation {
        self.web.as_ref().map_or_else(|| jadix_core::web_evaluation::WebEvaluation::failed(jadix_core::web_evaluation::WebEvaluationError::Unavailable), |runtime| runtime.evaluations.borrow_mut().evaluate(node, script))
    }
    fn request_file(&mut self, request: &jadix_core::file_request::FileRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::file_request::FileResult>, String> {
        self.services.as_ref().ok_or("native file service is disposed")?.files.request(request)
    }
    fn open_url(&mut self, url: &str) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        jadix_core::desktop::check_open_url(url)?;
        if self.is_closed() { return Err("native window is closed".into()); }
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let _ = send.send(files::open(self.native_window(), wide(url).as_ptr()).map(|()| true));
        (self.waker())();
        Ok(receive)
    }

    fn clipboard(&mut self, request: &jadix_core::clipboard::ClipboardRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::clipboard::ClipboardResult>, String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let _ = sender.send(clipboard::request(self.window, request));
        (self.waker())();
        Ok(receiver)
    }

    fn measurement_revision(&self) -> u64 { self.environment() }

    fn register_font(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.install_font(bytes).map(|registration| registration.families)
    }

    fn read_environment(&self) -> Result<Option<crate::environment::EnvironmentSnapshot>, String> {
        self.services.as_ref().ok_or("native services disposed")?.snapshot(&self.context)
    }

    fn audio(&self) -> Option<jadix_core::audio::AudioService> {
        self.services.as_ref().map(|s| s.audio.borrow().service.clone())
    }

    fn speech(&self) -> Option<jadix_core::speech::SpeechService> {
        self.services.as_ref().map(|s| s.speech.borrow().service.clone())
    }

    fn pump_audio(&mut self, _: RootId) {
        if let Some(services) = &self.services { services.audio.borrow_mut().pump(); services.speech.borrow_mut().pump(); }
    }

    fn fetch(&self) -> Option<jadix_net::service::FetchService> {
        self.services.as_ref().map(|services| services.fetch.borrow().service())
    }

    fn websocket(&self) -> Option<jadix_net::websocket::SocketService> {
        self.services.as_ref().map(|services| services.socket.service())
    }

    fn updater(&self) -> Option<jadix_core::updater::Updater> {
        self.updater.as_ref().map(|runtime| runtime.service.clone())
    }

    fn desktop_services(&self) -> Option<jadix_core::desktop_services::DesktopServices> {
        self.services.as_ref().map(|_| self.context.desktop.clone())
    }

    fn set_tray(&mut self, _: RootId, tray: Option<&jadix_core::tray::Tray>) -> Result<(), String> {
        if self.is_closed() { return Err("native window is closed".into()); }
        self.context.tray.set(self.window, tray)
    }

    fn take_tray_events(&mut self) -> Vec<String> { self.context.tray.take_events() }

    fn schedule_reactive_wake(&mut self, delay: Option<std::time::Duration>) {
        unsafe {
            KillTimer(self.window, REACTIVE_TIMER);
            if let Some(delay) = delay.filter(|_| !self.is_closed()) {
                let ms = delay.as_millis().saturating_add(u128::from(delay.subsec_nanos() % 1_000_000 != 0)).clamp(1, u128::from(u32::MAX)) as u32;
                if SetTimer(self.window, REACTIVE_TIMER, ms, None) == 0 { self.context.failed.set(true); }
            }
        }
    }
    fn bind_grid_navigation(&mut self, nodes: &[NodeId]) -> Result<bool, String> {
        if nodes.len() > self.controls.len()
            || nodes.iter().any(|node| {
                !self.controls.get(node).is_some_and(|control| {
                    matches!(
                        control.descriptor.content,
                        NodeContent::Input { enabled: true, .. }
                    )
                })
            })
        {
            return Err(String::from(
                "grid navigation binding requires mounted enabled native inputs",
            ));
        }
        *self.context.grid_navigation.borrow_mut() = nodes.iter().copied().collect();
        Ok(true)
    }

    fn bind_navigation(&mut self, nodes: &[NodeId]) -> Result<bool, String> {
        if nodes.len() > self.controls.len()
            || nodes.iter().any(|node| {
                !self.controls.get(node).is_some_and(|control| {
                    matches!(control.descriptor.content, NodeContent::Button { .. })
                })
            })
        {
            return Err(String::from(
                "navigation binding requires mounted native buttons",
            ));
        }
        *self.context.navigation.borrow_mut() = nodes.iter().copied().collect();
        Ok(true)
    }
    fn is_alive(&self) -> bool {
        !self.is_closed()
    }

    fn bind_drag(&mut self, nodes: &[NodeId]) -> Result<bool, String> {
        WindowsHost::bind_drag(self, nodes)?;
        Ok(true)
    }

    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        WindowsHost::scroll_to(self, node, offset)
    }

    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        self.pump();
        if self.is_closed() {
            return Err(String::from("native window is closed"));
        }
        WindowsHost::take_events(self)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            node_kinds: vec![
                NodeKind::Container,
                NodeKind::Text,
                NodeKind::Button,
                NodeKind::Input,
                NodeKind::Textarea,
                NodeKind::Checkbox,
                NodeKind::Switch,
                NodeKind::Scroll,
                NodeKind::Select,
                NodeKind::Image,
                NodeKind::Dialog,
                NodeKind::Popover,
                NodeKind::Hosted,
            ],
            text_editing: true,
            focus: true,
            scrolling: true,
            inline_raster_sources: true,
            inline_svg_sources: true,
            local_image_sources: true,
            remote_image_sources: true,
        }
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let _dpi = DpiScope::window(self.window);
        let scale = self.context.scale.get();
        if matches!(descriptor.content, NodeContent::Image { .. }) {
            let dimensions = self.controls.get(&request.key.node)
                .and_then(|control| self.context.images.borrow().get(&(control.window as usize)).and_then(|image| image.bitmap.as_ref().map(image::Bitmap::size)))
                .or_else(|| descriptor.content.image_dimensions()).unwrap_or([0, 0]);
            return Ok(MeasureResult { key: request.key, width: f64::from(dimensions[0]).min(request.key.max_width), height: f64::from(dimensions[1]).min(request.key.max_height), baseline: None });
        }
        let font = Font::new(descriptor, scale)
            .map_err(|_| LayoutError::MeasurementUnavailable(request.key.node))?;
        let text = match &descriptor.content {
            NodeContent::Text(value)
            | NodeContent::Input { value, .. }
            | NodeContent::Textarea { value, .. } => value,
            NodeContent::Button { label, .. }
            | NodeContent::Checkbox { label, .. }
            | NodeContent::Switch { label, .. } => label,
            NodeContent::Select {
                options,
                value,
                placeholder,
                ..
            } => options
                .iter()
                .find(|option| option.value == *value)
                .map_or(placeholder.as_str(), |option| option.label.as_str()),
            _ => "",
        };
        let text = wide(if text.is_empty() { " " } else { text });
        let context = unsafe { GetDC(self.window) };
        if context.is_null() {
            return Err(LayoutError::MeasurementUnavailable(request.key.node));
        }
        let previous = unsafe { SelectObject(context, font.0) };
        let mut frame = RECT {
            right: (request.key.max_width * scale)
                .min(f64::from(i32::MAX))
                .max(1.0) as i32,
            ..RECT::default()
        };
        let multiline = matches!(
            descriptor.content,
            NodeContent::Text(_) | NodeContent::Textarea { .. }
        );
        unsafe {
            DrawTextW(
                context,
                text.as_ptr(),
                (text.len() - 1) as i32,
                &mut frame,
                DT_CALCRECT
                    | DT_NOPREFIX
                    | if multiline {
                        DT_WORDBREAK
                    } else {
                        DT_SINGLELINE
                    },
            );
        }
        let mut metrics = TEXTMETRICW::default();
        let measured = unsafe { GetTextMetricsW(context, &mut metrics) } != 0;
        unsafe {
            SelectObject(context, previous);
            ReleaseDC(self.window, context);
        }
        if !measured {
            return Err(LayoutError::MeasurementUnavailable(request.key.node));
        }
        let chrome = if matches!(
            descriptor.content,
            NodeContent::Text(_) | NodeContent::Container
        ) {
            0.0
        } else {
            20.0
        };
        let height = if matches!(descriptor.content, NodeContent::Textarea { .. }) {
            (frame.bottom as f64 / scale).max(80.0)
        } else {
            f64::from(frame.bottom.max(metrics.tmHeight)) / scale
        };
        Ok(MeasureResult {
            key: request.key,
            width: (f64::from(frame.right) / scale + chrome).min(request.key.max_width),
            height: (height + if chrome == 0.0 { 0.0 } else { 10.0 }).min(request.key.max_height),
            baseline: Some((f64::from(metrics.tmAscent) / scale).min(request.key.max_height)),
        })
    }

    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Err(String::from(
            "Windows host requires validated incremental batches",
        ))
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        let _dpi = DpiScope::window(self.window);
        let previous_focus = unsafe { GetFocus() };
        self.context.suppress.set(true);
        let result = self.apply(batch, roots);
        self.context.suppress.set(false);
        if result.is_ok() && previous_focus != unsafe { GetFocus() } {
            if let Some(control) = self.context.target(previous_focus) {
                self.context.push(RawEvent::Blur { control });
            }
            if let Some(control) = self.context.target(unsafe { GetFocus() }) {
                if self.controls.get(&control).is_some_and(|control| {
                    matches!(
                        control.descriptor.content,
                        NodeContent::Button { enabled: true, .. }
                            | NodeContent::Input { enabled: true, .. }
                            | NodeContent::Textarea { enabled: true, .. }
                            | NodeContent::Checkbox { enabled: true, .. }
                            | NodeContent::Switch { enabled: true, .. }
                            | NodeContent::Select { enabled: true, .. }
                    )
                }) {
                    self.context.push(RawEvent::Focus { control });
                }
            }
        }
        result
    }

    fn dispose(&mut self) {
        self.context.deep_links.borrow_mut().activation.take();
        self.context.deep_links.borrow_mut().launches.clear();
        self.windows.get_mut().dispose();
        self.context.window_policy.get_mut().release_modal();
        if let Some(updater) = self.updater.take() { if self.owns_updater { updater.dispose(); } }
        self.context.tray.dispose();
        self.webviews.clear();
        self.hosted.clear();
        if let Some(web) = self.web.take() { web.dispose(); }
        unsafe { KillTimer(self.window, REACTIVE_TIMER); }
        if let Some(mut services) = self.services.take() { services.dispose(&self.context); }
        self.context.drag.cancel(&self.context, true);
        self.context.drag.clear();
        #[cfg(feature = "windows-gpu")]
        {
            self.plots.clear();
            self.context.gpu_windows.borrow_mut().clear();
            self.context.gpu_dirty.borrow_mut().clear();
        }
        if let Some(loader) = &mut self.images {
            loader.close();
        }
        self.images = None;
        for control in self.controls.values() {
            if control.accessible_role {
                let _ = self.accessibility.role(control.window, None);
            }
            if control.descriptor.accessible_selected.is_some()
                || control.descriptor.accessible_expanded.is_some()
            {
                let _ = self.accessibility.state(control.window, None, None);
            }
            if control.descriptor.accessible_name.is_some()
                || matches!(control.descriptor.content, NodeContent::Image { .. })
            {
                let _ = self.accessibility.name(control.window, None);
            }
            if control.descriptor.accessible_description.is_some() {
                let _ = self.accessibility.description(control.window, None);
            }
        }
        if !self.window.is_null() && self.context.root.get() == self.window {
            unsafe {
                DestroyWindow(self.window);
            }
        }
        self.window = null_mut();
        self.controls.clear();
        self.fonts.clear();
        self.context.targets.borrow_mut().clear();
        self.context.navigation.borrow_mut().clear();
        self.context.grid_navigation.borrow_mut().clear();
        self.context.keyboard.borrow_mut().clear();
        self.context.images.borrow_mut().clear();
        self.context.paints.borrow_mut().clear();
        self.context.popovers.borrow_mut().clear();
        self.popovers.clear();
        self.context.backdrops.borrow_mut().clear();
        self.context.modal.set(None);
        self.backdrops.clear();
        self.modal_returns.clear();
        self.context.scrolls.borrow_mut().clear();
        self.context.compositions.borrow_mut().clear();
        self.context.events.borrow_mut().clear();
        self.context.event_bytes.set(0);
        self.context.closed.set(true);
    }
}

impl Drop for WindowsHost {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::{grid_navigation_key, Composition, NavigationKey};

    #[test]
    fn wrapped_control_ids_keep_queued_events_bound_to_the_original_generation() {
        use super::*;
        use crate::host::NativeRoot;

        let mut host = WindowsHost::new("Jadix recycled native IDs", [320, 240]).unwrap();
        host.next_control = u16::MAX;
        let mut root = NativeRoot::new(host);
        let mut nodes = vec![PresentationNode {
            id: NodeId::new(root.model().id()),
            descriptor: NodeDescriptor {
                content: NodeContent::Button {
                    label: "old".into(),
                    enabled: true,
                },
                frame: Rect::default(),
                style: Default::default(),
                accessible_role: None,
                accessible_selected: None,
                accessible_expanded: None,
                interaction: Default::default(),
                keyboard: Default::default(),
                navigation_keys: Vec::new(),
                accessibility: Default::default(),
                accessibility_relations: Default::default(),
                accessible_description: None,
                accessible_name: None,
            },
            children: vec![],
        }];
        root.present(&mut nodes, [320.0, 240.0], 1).unwrap();
        let old = nodes[0].id;
        assert_eq!(root.host().controls[&old].identifier, u16::MAX);
        root.host().activate(old).unwrap();
        root.present(&mut [], [320.0, 240.0], 1).unwrap();
        nodes[0].id = NodeId::new(root.model().id());
        root.host_mut().next_control = u16::MAX;
        root.present(&mut nodes, [320.0, 240.0], 1).unwrap();
        let next = nodes[0].id;
        assert_eq!(root.host().controls[&next].identifier, u16::MAX);
        root.host().activate(next).unwrap();
        let events = root.poll_events().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.payload == EventPayload::Activate)
                .count(),
            1
        );
        assert!(events.iter().all(|event| event.node == next));
        assert_eq!(root.host().next_control, 1);
    }

    #[test]
    fn composition_replaces_utf16_selection_without_splitting_surrogates() {
        let composition = Composition {
            original: String::from("A😀中"),
            range: [1, 3],
        };
        let marked: Vec<_> = "🙂文".encode_utf16().collect();
        assert_eq!(
            composition.snapshot(&marked, 3),
            Some((String::from("A🙂文中"), [4, 4], [1, 4]))
        );
        assert!(composition.snapshot(&marked, 1).is_none());
        assert!(composition.snapshot(&[0xd800], 0).is_none());
        let invalid = Composition {
            original: String::from("😀"),
            range: [1, 2],
        };
        assert!(invalid.snapshot(&marked, 0).is_none());
    }

    #[test]
    fn grid_keys_preserve_native_caret_selection_ime_and_alt_shortcuts() {
        for key in [0x25, 0x26, 0x27, 0x28, 0x24, 0x23] {
            assert!(grid_navigation_key(key, false, false, false, false).is_none());
            assert!(grid_navigation_key(key, true, true, false, false).is_none());
            assert!(grid_navigation_key(key, false, true, false, false).is_some());
        }
        for key in [0x09, 0x0d, 0x21, 0x22, 0x23, 0x24, 0x25, 0x28] {
            assert!(grid_navigation_key(key, false, false, false, true).is_none());
            assert!(grid_navigation_key(key, false, true, true, false).is_none());
        }
        assert_eq!(
            grid_navigation_key(0x09, true, false, false, false),
            Some(NavigationKey::Tab)
        );
        assert_eq!(
            grid_navigation_key(0x0d, true, false, false, false),
            Some(NavigationKey::Enter)
        );
        assert!(grid_navigation_key(0x09, false, true, false, false).is_none());
    }
}
