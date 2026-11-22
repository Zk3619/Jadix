//! Common-control leaves. Shared HostedContent owns values and event validation.
use super::*;
use jadix_core::hosted::{CivilDate, DateControlMode, HostedContent, HostedMenuItem, TimeOfDay};
use windows_sys::Win32::UI::Controls::Dialogs::*;

const SUBCLASS: usize = 0x4a444843;
#[cfg(feature = "windows-gpu")]
const SHADER_TIMER: usize = 0x4a445348;
// TBM_GETPOS is WM_USER; windows-sys does not export this alias.
const TBM_GETPOS: u32 = WM_USER;
mod text;
mod canvas;
mod media;
pub(super) struct Leaf {
    parent: HWND,
    node: NodeId,
    context: *const WindowContext,
    spec: RefCell<HostedContent>,
    children: RefCell<Vec<HWND>>,
    updating: Cell<bool>,
    colors: RefCell<[COLORREF; 16]>,
    text: RefCell<Option<text::Text>>,
    canvas: RefCell<Option<canvas::Canvas>>,
    media: RefCell<Option<media::Media>>,
    #[cfg(feature = "windows-gpu")]
    shader: RefCell<Option<super::gpu::Shader>>,
    asset_root: std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>,
    pending: RefCell<VecDeque<(String, serde_json::Value)>>,
    pending_bytes: Cell<usize>,
}
impl Leaf {
    pub fn supports(spec: &HostedContent) -> bool {
        #[cfg(feature = "windows-gpu")]
        if matches!(spec, HostedContent::Shader { .. }) { return true; }
        matches!(spec, HostedContent::Date { .. } | HostedContent::Time { .. }
            | HostedContent::Color { .. } | HostedContent::MenuButton { .. }
            | HostedContent::TabBar { .. } | HostedContent::Segments { .. }
            | HostedContent::Terminal { .. } | HostedContent::RichEditor { .. } | HostedContent::Canvas { .. } | HostedContent::Media { .. })
    }
    pub fn new(node: NodeId, parent: HWND, spec: HostedContent, context: &WindowContext, asset_root: std::sync::Arc<std::sync::Mutex<std::path::PathBuf>>) -> Result<Box<Self>, String> {
        if !Self::supports(&spec) { return Err("unsupported Windows hosted content".into()); }
        let controls = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_DATE_CLASSES | ICC_TAB_CLASSES | ICC_BAR_CLASSES };
        if unsafe { InitCommonControlsEx(&controls) } == 0 { return Err(native_error("initialize hosted controls")); }
        let leaf = Box::new(Self { parent, node, context, spec: RefCell::new(spec.clone()), children: Default::default(), updating: Cell::new(false), colors: RefCell::new([0; 16]), text: Default::default(), canvas: Default::default(), media: Default::default(),
            #[cfg(feature = "windows-gpu")]
            shader: Default::default(),
            asset_root, pending: Default::default(), pending_bytes: Cell::new(0) });
        if unsafe { SetWindowSubclass(parent, Some(procedure), SUBCLASS, (&*leaf as *const Self) as usize) } == 0 { return Err(native_error("hosted subclass")); }
        if let Err(error) = leaf.update(spec) { return Err(error); }
        Ok(leaf)
    }
    fn context(&self) -> &WindowContext { unsafe { &*self.context } }
    fn child(&self, class: &str, text: &str, style: u32) -> Result<HWND, String> {
        let id = 1001 + self.children.borrow().len();
        let child = unsafe { CreateWindowExW(0, wide(class).as_ptr(), wide(text).as_ptr(), WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
            0, 0, 1, 1, self.parent, id as HMENU, GetModuleHandleW(null_mut()), null_mut()) };
        if child.is_null() { return Err(native_error("create hosted control")); }
        self.context().targets.borrow_mut().insert(child as usize, self.node);
        self.children.borrow_mut().push(child);
        unsafe { SendMessageW(child, WM_SETFONT, SendMessageW(self.parent, WM_GETFONT, 0, 0) as usize, 0); }
        Ok(child)
    }
    fn clear(&self) {
        self.text.borrow_mut().take();
        self.canvas.borrow_mut().take();
        self.media.borrow_mut().take();
        #[cfg(feature = "windows-gpu")]
        { unsafe { KillTimer(self.parent, SHADER_TIMER); } self.context().shader_windows.borrow_mut().remove(&(self.parent as usize)); self.shader.borrow_mut().take(); }
        self.pending.borrow_mut().clear();
        self.pending_bytes.set(0);
        let children = std::mem::take(&mut *self.children.borrow_mut());
        for child in children {
            self.context().targets.borrow_mut().remove(&(child as usize));
            unsafe { DestroyWindow(child); }
        }
    }
    pub fn update(&self, spec: HostedContent) -> Result<(), String> {
        spec.validate()?;
        // Publish native edits against the revision that produced them before
        // an unrelated presentation update can replace that specification.
        self.drain();
        // Native setters can synchronously notify. Never borrow the specification
        // across them, and suppress their notifications like ordinary controls.
        self.updating.set(true);
        let old = self.spec.replace(spec.clone());
        let empty = self.children.borrow().is_empty() && self.canvas.borrow().is_none()
            && self.media.borrow().is_none() && self.text.borrow().is_none();
        #[cfg(feature = "windows-gpu")]
        let empty = empty && self.shader.borrow().is_none();
        let rebuild = empty || shape(&old) != shape(&spec);
        let result = (|| {
            if rebuild {
                self.clear();
                match &spec {
                    HostedContent::Canvas { .. } => { *self.canvas.borrow_mut() = Some(canvas::Canvas::new()?); }
                    #[cfg(feature = "windows-gpu")]
                    HostedContent::Shader { .. } => {
                        *self.shader.borrow_mut() = Some(super::gpu::Shader::new(self.parent, self.context().root.get(), &spec, self.context().scale.get() as f32, self.asset_root.clone())?);
                        self.context().shader_windows.borrow_mut().insert(self.parent as usize);
                    }
                    HostedContent::Media { .. } => { *self.media.borrow_mut() = Some(media::Media::new(self)?); }
                    HostedContent::Date { mode: DateControlMode::Calendar, .. } => { self.child("SysMonthCal32", "", 0)?; }
                    HostedContent::Date { .. } => { self.child("SysDateTimePick32", "", DTS_SHOWNONE)?; }
                    HostedContent::Time { .. } => { self.child("SysDateTimePick32", "", DTS_TIMEFORMAT)?; }
                    HostedContent::Color { alpha, .. } => {
                        self.child("BUTTON", "", BS_PUSHBUTTON as u32)?;
                        if *alpha { self.child("msctls_trackbar32", "Opacity", TBS_NOTICKS)?; }
                    }
                    HostedContent::MenuButton { .. } => { self.child("BUTTON", "", BS_PUSHBUTTON as u32)?; }
                    HostedContent::TabBar { .. } => { self.child("SysTabControl32", "", TCS_BUTTONS)?; }
                    HostedContent::Segments { items, .. } => {
                        for (index, _) in items.iter().enumerate() { self.child("BUTTON", "", BS_AUTORADIOBUTTON as u32 | BS_PUSHLIKE as u32 | if index == 0 { WS_GROUP } else { 0 })?; }
                    }
                    HostedContent::Terminal { .. } | HostedContent::RichEditor { .. } => {
                        *self.text.borrow_mut() = Some(text::Text::new(self, &spec)?);
                    }
                    _ => return Err("unsupported Windows hosted control".into()),
                }
            }
            if let Some(media) = self.media.borrow_mut().as_mut() { media.update(self, &old, &spec)?; }
            #[cfg(feature = "windows-gpu")]
            if let Some(shader) = self.shader.borrow_mut().as_mut() {
                shader.update(&spec, self.context().scale.get() as f32)?;
                unsafe { KillTimer(self.parent, SHADER_TIMER); }
                if unsafe { IsWindowVisible(self.parent) } != 0 {
                    if let Some(interval) = shader.interval() { if unsafe { SetTimer(self.parent, SHADER_TIMER, interval, None) } == 0 { return Err(native_error("start shader frames")); } }
                }
            }
            let children = self.children.borrow().clone();
            let Some(&first) = children.first() else { self.resize(); return Ok(()); };
            unsafe { match &spec {
                HostedContent::Date { value, minimum, maximum, mode, enabled, .. } => {
                    let calendar = *mode == DateControlMode::Calendar;
                    let range = [minimum.map(system_date).unwrap_or_default(), maximum.map(system_date).unwrap_or_default()];
                    SendMessageW(first, if calendar { MCM_SETRANGE } else { DTM_SETRANGE },
                        (if minimum.is_some() { GDTR_MIN } else { 0 } | if maximum.is_some() { GDTR_MAX } else { 0 }) as usize, range.as_ptr() as isize);
                    if let Some(value) = value {
                        let date = system_date(*value);
                        SendMessageW(first, if calendar { MCM_SETCURSEL } else { DTM_SETSYSTEMTIME }, GDT_VALID as usize, &date as *const _ as isize);
                    } else if !calendar { SendMessageW(first, DTM_SETSYSTEMTIME, GDT_NONE as usize, 0); }
                    EnableWindow(first, i32::from(*enabled));
                }
                HostedContent::Time { value, seconds, enabled } => {
                    let date = SYSTEMTIME { wYear: 2000, wMonth: 1, wDay: 1, wHour: value.hour as u16, wMinute: value.minute as u16, wSecond: if *seconds { value.second as u16 } else { 0 }, ..Default::default() };
                    SendMessageW(first, DTM_SETFORMATW, 0, wide(if *seconds { "HH':'mm':'ss" } else { "HH':'mm" }).as_ptr() as isize);
                    SendMessageW(first, DTM_SETSYSTEMTIME, GDT_VALID as usize, &date as *const _ as isize);
                    EnableWindow(first, i32::from(*enabled));
                }
                HostedContent::MenuButton { label, enabled, .. } => {
                    SetWindowTextW(first, wide(label).as_ptr()); EnableWindow(first, i32::from(*enabled));
                }
                HostedContent::Color { rgba, alpha, enabled } => {
                    SetWindowTextW(first, wide(&format!("#{:02X}{:02X}{:02X}", rgba[0], rgba[1], rgba[2])).as_ptr());
                    EnableWindow(first, i32::from(*enabled));
                    if *alpha { SendMessageW(children[1], TBM_SETRANGE, 1, 255 << 16); SendMessageW(children[1], TBM_SETPOS, 1, rgba[3] as isize); EnableWindow(children[1], i32::from(*enabled)); }
                }
                HostedContent::Segments { items, selected, enabled } => {
                    for (child, item) in children.iter().zip(items) {
                        SetWindowTextW(*child, wide(&item.label).as_ptr());
                        SendMessageW(*child, BM_SETCHECK, usize::from(selected.as_ref() == Some(&item.id)), 0);
                        EnableWindow(*child, i32::from(*enabled && item.enabled));
                        let style = GetWindowLongPtrW(*child, GWL_STYLE) as u32;
                        let tab_stop = selected.as_ref().map_or_else(|| items.iter().find(|i| i.enabled).is_some_and(|i| i.id == item.id), |id| *id == item.id);
                        SetWindowLongPtrW(*child, GWL_STYLE, (if tab_stop { style | WS_TABSTOP } else { style & !WS_TABSTOP }) as isize);
                    }
                }
                HostedContent::TabBar { items, selected, enabled, hidden } => {
                    // Replacing labels preserves the HWND and keyboard focus.
                    SendMessageW(first, TCM_DELETEALLITEMS, 0, 0);
                    for (index, item) in items.iter().enumerate() {
                        let label = item.badge.as_ref().map_or_else(|| item.label.clone(), |badge| format!("{} ({})", item.label, if badge.is_empty() { "•" } else { badge }));
                        let mut label = wide(&label);
                        let tab = TCITEMW { mask: TCIF_TEXT, pszText: label.as_mut_ptr(), ..Default::default() };
                        SendMessageW(first, TCM_INSERTITEMW, index, &tab as *const _ as isize);
                    }
                    let index = items.iter().position(|item| Some(&item.id) == selected.as_ref()).map_or(usize::MAX, |i| i);
                    SendMessageW(first, TCM_SETCURSEL, index, 0);
                    EnableWindow(first, i32::from(*enabled)); ShowWindow(first, if *hidden { SW_HIDE } else { SW_SHOWNA });
                }
                _ => {}
            } }
            if let Some(text) = self.text.borrow_mut().as_mut() { text.update(self, &spec)?; }
            self.resize();
            Ok(())
        })();
        self.updating.set(false);
        result
    }
    pub fn resize(&self) {
        if let Some(media) = self.media.borrow().as_ref() { media.resize(); return; }
        if self.canvas.borrow().is_some() { unsafe { InvalidateRect(self.parent, null_mut(), 0); } }
        #[cfg(feature = "windows-gpu")]
        if self.shader.borrow().is_some() { unsafe { InvalidateRect(self.parent, null_mut(), 0); } }
        let mut frame = RECT::default(); unsafe { GetClientRect(self.parent, &mut frame); }
        let children = self.children.borrow().clone();
        let spec = self.spec.borrow();
        let segments = matches!(*spec, HostedContent::Segments { .. });
        for (index, child) in children.iter().enumerate() {
            let (x, width) = if segments { let count = children.len() as i32; let left = frame.right * index as i32 / count; (left, frame.right * (index as i32 + 1) / count - left) }
                else if children.len() == 2 { if index == 0 { (0, frame.right * 2 / 3) } else { (frame.right * 2 / 3, frame.right - frame.right * 2 / 3) } }
                else { (0, frame.right) };
            unsafe { SetWindowPos(*child, null_mut(), x, 0, width.max(0), frame.bottom.max(0), SWP_NOZORDER | SWP_NOACTIVATE); }
        }
    }
    fn emit(&self, name: &str, value: serde_json::Value) {
        if self.spec.borrow().accepts_event(name, &value) {
            self.context().push(RawEvent::Hosted { control: self.node, event: name.into(), data: value });
        }
    }
    fn defer(&self, name: &str, value: serde_json::Value) {
        let bytes = name.len() + value.to_string().len() + 64;
        let mut pending = self.pending.borrow_mut();
        if pending.len() < MAX_EVENTS && bytes <= MAX_EVENT_BYTES.saturating_sub(self.pending_bytes.get()) {
            self.pending_bytes.set(self.pending_bytes.get() + bytes);
            pending.push_back((name.into(), value));
        }
        else { self.context().failed.set(true); }
    }
    pub fn drain(&self) {
        if let Some(media) = self.media.borrow_mut().as_mut() { media.pump(self); }
        if let Some(text) = self.text.borrow_mut().as_mut() {
            if let Err(error) = text.publish(self) { self.defer("error", error.into()); }
        }
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        self.pending_bytes.set(0);
        for (name, value) in pending { self.emit(&name, value); }
    }
    fn menu(&self, items: &[HostedMenuItem]) {
        let mut ids = Vec::new();
        let Ok(menu) = Menu::new(items, &mut ids) else { return; };
        let mut frame = RECT::default(); unsafe { GetWindowRect(self.children.borrow()[0], &mut frame); }
        let selected = unsafe { TrackPopupMenuEx(menu.0, TPM_RETURNCMD | TPM_NONOTIFY, frame.left, frame.bottom, self.parent, null_mut()) };
        if selected > 0 { if let Some(id) = ids.get(selected as usize - 1) { self.emit("select", serde_json::json!(id)); } }
    }
    fn color(&self, mut rgba: [u8; 4]) {
        let mut colors = *self.colors.borrow();
        let mut dialog = CHOOSECOLORW { lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32, hwndOwner: self.parent,
            rgbResult: rgb(rgba), lpCustColors: colors.as_mut_ptr(), Flags: CC_RGBINIT | CC_FULLOPEN, ..Default::default() };
        if unsafe { ChooseColorW(&mut dialog) } != 0 {
            rgba[0] = dialog.rgbResult as u8; rgba[1] = (dialog.rgbResult >> 8) as u8; rgba[2] = (dialog.rgbResult >> 16) as u8;
            *self.colors.borrow_mut() = colors;
            self.emit("change", serde_json::json!(rgba));
        }
    }
}
impl Drop for Leaf {
    fn drop(&mut self) { self.updating.set(true); unsafe { RemoveWindowSubclass(self.parent, Some(procedure), SUBCLASS); } self.clear(); }
}
fn shape(spec: &HostedContent) -> (std::mem::Discriminant<HostedContent>, usize) {
    (std::mem::discriminant(spec), match spec { HostedContent::Date { mode, .. } => usize::from(*mode == DateControlMode::Calendar), HostedContent::Color { alpha, .. } => usize::from(*alpha), HostedContent::Segments { items, .. } => items.len(), _ => 0 })
}
fn system_date(date: CivilDate) -> SYSTEMTIME { SYSTEMTIME { wYear: date.year, wMonth: date.month as u16, wDay: date.day as u16, ..Default::default() } }
fn civil_date(date: SYSTEMTIME) -> CivilDate { CivilDate { year: date.wYear, month: date.wMonth as u8, day: date.wDay as u8 } }
fn rgb(rgba: [u8; 4]) -> COLORREF { rgba[0] as u32 | (rgba[1] as u32) << 8 | (rgba[2] as u32) << 16 }

unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, reference: usize) -> LRESULT {
    let leaf = unsafe { &*(reference as *const Leaf) };
    if leaf.updating.get() { return unsafe { DefSubclassProc(window, message, parameter, data) }; }
    match message {
        WM_PRINTCLIENT if leaf.canvas.borrow().is_some() => {
            let background = leaf.context().paints.borrow().get(&(window as usize)).map(|paint| paint.background)
                .unwrap_or_else(|| unsafe { GetSysColor(COLOR_WINDOW) });
            if let HostedContent::Canvas { commands } = &*leaf.spec.borrow() {
                if let Err(error) = leaf.canvas.borrow().as_ref().unwrap().print(window, parameter as HDC, commands, background) { leaf.defer("error", error.into()); }
            }
            return 0;
        }
        #[cfg(feature = "windows-gpu")]
        WM_PRINTCLIENT => {
            // An ancestor can be part of a nested shader's backdrop. Its frame
            // is already being composed, so never recursively borrow it.
            if let Ok(mut shader) = leaf.shader.try_borrow_mut() {
                if let Some(shader) = shader.as_mut() {
                    if let Err(error) = shader.draw(true).and_then(|_| shader.print(parameter as HDC)) { leaf.defer("error", error.into()); }
                    return 0;
                }
            } else { return 0; }
        }
        #[cfg(feature = "windows-gpu")]
        super::image::READY if leaf.shader.borrow().is_some() => {
            let result = leaf.shader.borrow_mut().as_mut().unwrap().drain();
            match result { Ok(errors) => for error in errors { leaf.defer("error", error.into()); }, Err(error) => leaf.defer("error", error.into()) }
            leaf.drain();
            return 0;
        }
        #[cfg(feature = "windows-gpu")]
        WM_PAINT if leaf.shader.borrow().is_some() => {
            let mut paint = PAINTSTRUCT::default();
            unsafe { BeginPaint(window, &mut paint); }
            let result = leaf.shader.borrow_mut().as_mut().unwrap().draw(true);
            unsafe { EndPaint(window, &paint); }
            if let Err(error) = result { unsafe { KillTimer(window, SHADER_TIMER); } leaf.defer("error", error.into()); }
            return 0;
        }
        #[cfg(feature = "windows-gpu")]
        WM_TIMER if parameter == SHADER_TIMER => {
            let result = leaf.shader.borrow_mut().as_mut().map(|shader| shader.draw(true));
            if let Some(Err(error)) = result { unsafe { KillTimer(window, SHADER_TIMER); } leaf.defer("error", error.into()); }
            if leaf.shader.borrow().as_ref().and_then(|shader| shader.interval()).is_none() { unsafe { KillTimer(window, SHADER_TIMER); } }
            leaf.drain();
            return 0;
        }
        #[cfg(feature = "windows-gpu")]
        WM_ERASEBKGND if leaf.shader.borrow().is_some() => return 1,
        #[cfg(feature = "windows-gpu")]
        WM_SHOWWINDOW if leaf.shader.borrow().is_some() => {
            unsafe { KillTimer(window, SHADER_TIMER); }
            if parameter != 0 {
                if let Some(interval) = leaf.shader.borrow().as_ref().and_then(|shader| shader.interval()) {
                    if unsafe { SetTimer(window, SHADER_TIMER, interval, None) } == 0 { leaf.defer("error", native_error("resume shader frames").into()); }
                }
                unsafe { InvalidateRect(window, null_mut(), 0); }
            }
        }
        WM_PAINT if leaf.canvas.borrow().is_some() => {
            let mut paint = PAINTSTRUCT::default();
            unsafe { BeginPaint(window, &mut paint); }
            let background = leaf.context().paints.borrow().get(&(window as usize)).map(|paint| paint.background)
                .unwrap_or_else(|| unsafe { GetSysColor(COLOR_WINDOW) });
            let result = if let HostedContent::Canvas { commands } = &*leaf.spec.borrow() {
                leaf.canvas.borrow_mut().as_mut().unwrap().paint(window, commands, background)
            } else { Ok(()) };
            unsafe { EndPaint(window, &paint); }
            if let Err(error) = result { leaf.defer("error", error.into()); }
            return 0;
        }
        WM_ERASEBKGND if leaf.canvas.borrow().is_some() => return 1,
        media::READY => { leaf.drain(); return 0; }
        WM_TIMER if parameter == media::TIMER => {
            if let Some(media) = leaf.media.borrow().as_ref() { media.progress(leaf); }
            leaf.drain();
            return 0;
        }
        WM_SIZE => leaf.resize(),
        WM_SETFONT => {
            leaf.updating.set(true);
            if let Some(text) = leaf.text.borrow().as_ref() { text.set_font(parameter); }
            else { let children = leaf.children.borrow().clone(); for child in children { unsafe { SendMessageW(child, message, parameter, data); } } }
            leaf.updating.set(false);
        }
        WM_SETFOCUS if leaf.media.borrow().is_some() => { leaf.media.borrow().as_ref().unwrap().focus(); }
        WM_SETFOCUS => { let first = leaf.children.borrow().first().copied(); if let Some(first) = first { unsafe { SetFocus(first); } } }
        WM_COMMAND if data != 0 => {
            if !leaf.children.borrow().contains(&(data as HWND)) { return unsafe { DefSubclassProc(window, message, parameter, data) }; }
            let notification = (parameter >> 16) as u32;
            if let Some(text) = leaf.text.borrow_mut().as_mut() { text.notify(notification); }
            let spec = leaf.spec.borrow().clone();
            if notification == BN_CLICKED {
                if let Some(media) = leaf.media.borrow_mut().as_mut() { media.command(leaf, data as HWND, false); }
                match spec {
                    HostedContent::MenuButton { items, enabled: true, .. } => leaf.menu(&items),
                    HostedContent::Color { mut rgba, alpha, enabled: true } => { if !alpha { rgba[3] = 255; } leaf.color(rgba); }
                    HostedContent::Segments { items, .. } => {
                        let index = (parameter & 0xffff).saturating_sub(1001);
                        if let Some(item) = items.get(index) { leaf.emit("select", serde_json::json!(item.id)); }
                    }
                    _ => {}
                }
                return 0;
            }
        }
        WM_HSCROLL if data != 0 => {
            if let Some(media) = leaf.media.borrow_mut().as_mut() { media.command(leaf, data as HWND, true); return 0; }
            if let HostedContent::Color { mut rgba, alpha: true, .. } = leaf.spec.borrow().clone() {
                rgba[3] = unsafe { SendMessageW(data as HWND, TBM_GETPOS, 0, 0) } as u8;
                leaf.emit("change", serde_json::json!(rgba));
                return 0;
            }
        }
        WM_NOTIFY if data != 0 => {
            let header = unsafe { &*(data as *const NMHDR) };
            if !leaf.children.borrow().contains(&header.hwndFrom) { return unsafe { DefSubclassProc(window, message, parameter, data) }; }
            let spec = leaf.spec.borrow().clone();
            if let Some(text) = leaf.text.borrow_mut().as_mut() { text.selection_changed(header.code); }
            match (header.code, spec) {
                (DTN_DATETIMECHANGE, HostedContent::Date { .. }) => {
                    let change = unsafe { &*(data as *const NMDATETIMECHANGE) };
                    if change.dwFlags == GDT_VALID { leaf.emit("change", serde_json::json!(civil_date(change.st))); }
                }
                (MCN_SELCHANGE, HostedContent::Date { .. }) => {
                    let change = unsafe { &*(data as *const NMSELCHANGE) };
                    leaf.emit("change", serde_json::json!(civil_date(change.stSelStart)));
                }
                (DTN_DATETIMECHANGE, HostedContent::Time { seconds, .. }) => {
                    let change = unsafe { &*(data as *const NMDATETIMECHANGE) };
                    if change.dwFlags == GDT_VALID { leaf.emit("change", serde_json::json!(TimeOfDay { hour: change.st.wHour as u8, minute: change.st.wMinute as u8, second: if seconds { change.st.wSecond as u8 } else { 0 } })); }
                }
                (TCN_SELCHANGE, HostedContent::TabBar { items, selected, .. }) => {
                    let index = unsafe { SendMessageW(header.hwndFrom, TCM_GETCURSEL, 0, 0) };
                    if let Some(item) = usize::try_from(index).ok().and_then(|index| items.get(index)).filter(|item| item.enabled) { leaf.emit("select", serde_json::json!(item.id)); }
                    else { let previous = items.iter().position(|item| Some(&item.id) == selected.as_ref()).unwrap_or(usize::MAX); unsafe { SendMessageW(header.hwndFrom, TCM_SETCURSEL, previous, 0); } }
                }
                _ => {}
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}
pub(super) struct Menu(pub(super) HMENU);
impl Menu {
    pub(super) fn new(items: &[HostedMenuItem], ids: &mut Vec<String>) -> Result<Self, String> {
        let menu = Self(unsafe { CreatePopupMenu() });
        if menu.0.is_null() { return Err(native_error("create hosted menu")); }
        menu.append(items, ids, true)?; Ok(menu)
    }
    fn append(&self, items: &[HostedMenuItem], ids: &mut Vec<String>, parent_enabled: bool) -> Result<(), String> {
        for item in items {
            let enabled = parent_enabled && item.enabled;
            let flags = if enabled { MF_ENABLED } else { MF_GRAYED } | if item.checked { MF_CHECKED } else { 0 };
            let label = item.accelerator.as_ref().map_or_else(|| item.label.clone(), |key| format!("{}\t{}", item.label, key));
            if item.inline { self.append(&item.children, ids, enabled)?; continue; }
            let (flags, id, submenu) = if item.children.is_empty() {
                ids.push(item.id.clone()); (flags, ids.len(), None)
            } else { let child = Self::new(&item.children, ids)?; (flags | MF_POPUP, child.0 as usize, Some(child)) };
            if unsafe { AppendMenuW(self.0, flags, id, wide(&label).as_ptr()) } == 0 { return Err(native_error("append hosted menu")); }
            // The parent menu now owns its submenu, including on later failures.
            if let Some(submenu) = submenu { std::mem::forget(submenu); }
        }
        Ok(())
    }
}
impl Drop for Menu { fn drop(&mut self) { unsafe { DestroyMenu(self.0); } } }
