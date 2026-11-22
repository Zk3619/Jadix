//! RichEdit is an OS text surface; parsing, stream state and HTML trust stay shared.
use super::*;
use jadix_core::terminal::{CellStyle, TerminalScreen, TerminalSpec};
use jadix_document::native_html::{prepare_native_html, NativeHtmlElement, NativeHtmlPlan, NativeHtmlRun};
use windows::Win32::UI::Controls::RichEdit as re;
use windows::core::Interface;
use windows_sys::Win32::System::LibraryLoader::{LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};

const TEXT_SUBCLASS: usize = 0x4a445458;
enum Content {
    Terminal { screen: TerminalScreen, configuration: TerminalSpec, revision: Option<u64>, application_cursor: bool, bracketed_paste: bool },
    Editor { plan: NativeHtmlPlan, runs: Vec<NativeHtmlRun>, source: String, published: String },
}
pub(super) struct Text {
    window: HWND,
    document: Option<re::ITextDocument2>,
    content: Content,
    dirty: bool,
    selection_dirty: bool,
    surrogate: Option<u16>,
    composing: bool,
}
fn document(window: HWND) -> Result<re::ITextDocument2, String> {
    let mut pointer: *mut std::ffi::c_void = null_mut();
    if unsafe { SendMessageW(window, re::EM_GETOLEINTERFACE, 0, &mut pointer as *mut _ as isize) } == 0 || pointer.is_null() {
        return Err("system rich editor has no text object model".into());
    }
    let object = unsafe { windows::core::IUnknown::from_raw(pointer) };
    object.cast().map_err(|error| format!("system rich editor text object model: {error}"))
}
fn font(document: &re::ITextDocument2, start: usize, end: usize) -> Result<re::ITextFont2, String> {
    unsafe { document.Range2(start as i32, end as i32).and_then(|range| range.GetFont2()) }
        .map_err(|error| format!("native rich text range: {error}"))
}
fn format() -> re::CHARFORMAT2W {
    let mut value = re::CHARFORMAT2W::default();
    value.Base.cbSize = std::mem::size_of::<re::CHARFORMAT2W>() as u32;
    value
}
fn select(window: HWND, range: [usize; 2]) {
    let range = re::CHARRANGE { cpMin: range[0].min(i32::MAX as usize) as i32, cpMax: range[1].min(i32::MAX as usize) as i32 };
    unsafe { SendMessageW(window, re::EM_EXSETSEL, 0, &range as *const _ as isize); }
}
fn selected(window: HWND) -> [usize; 2] {
    let mut range = re::CHARRANGE::default();
    unsafe { SendMessageW(window, re::EM_EXGETSEL, 0, &mut range as *mut _ as isize); }
    [range.cpMin.max(0) as usize, range.cpMax.max(0) as usize]
}
fn native_text(window: HWND) -> Result<String, String> {
    let query = re::GETTEXTLENGTHEX { flags: re::GTL_PRECISE | re::GTL_NUMCHARS, codepage: 1200 };
    let count = unsafe { SendMessageW(window, re::EM_GETTEXTLENGTHEX, &query as *const _ as usize, 0) };
    if !(0..=1_048_576).contains(&count) { return Err("native rich text exceeds budget".into()); }
    let mut text = vec![0u16; count as usize + 1];
    let query = re::GETTEXTEX { cb: (text.len() * 2) as u32, flags: re::GT_RAWTEXT, codepage: 1200, ..Default::default() };
    let count = unsafe { SendMessageW(window, re::EM_GETTEXTEX, &query as *const _ as usize, text.as_mut_ptr() as isize) };
    if count < 0 || count as usize >= text.len() { return Err("read native rich text failed".into()); }
    String::from_utf16(&text[..count as usize]).map(|text| text.replace('\r', "\n")).map_err(|_| "invalid native Unicode text".into())
}
fn apply_format(window: HWND, value: &re::CHARFORMAT2W, scope: u32) {
    unsafe { SendMessageW(window, re::EM_SETCHARFORMAT, scope as usize, value as *const _ as isize); }
}
fn color(value: u32) -> windows::Win32::Foundation::COLORREF {
    windows::Win32::Foundation::COLORREF(((value >> 16) & 255) | (value & 0xff00) | ((value & 255) << 16))
}
fn style_format(style: &CellStyle) -> re::CHARFORMAT2W {
    let mut value = format();
    value.Base.dwMask = re::CFM_MASK(re::CFM_BOLD.0 | re::CFM_ITALIC.0 | re::CFM_UNDERLINE.0 | re::CFM_COLOR.0 | re::CFM_BACKCOLOR.0);
    value.Base.dwEffects = re::CFE_EFFECTS(if style.bold { re::CFE_BOLD.0 } else { 0 }
        | if style.italic { re::CFE_ITALIC.0 } else { 0 } | if style.underline { re::CFE_UNDERLINE.0 } else { 0 });
    let (foreground, background) = if style.inverse { (style.background.unwrap_or(0x1d1f21), style.foreground.unwrap_or(0xc5c8c6)) }
        else { (style.foreground.unwrap_or(0xc5c8c6), style.background.unwrap_or(0x1d1f21)) };
    value.Base.crTextColor = color(foreground); value.crBackColor = color(background);
    value
}
impl Text {
    pub fn set_font(&self, handle: usize) {
        let mut native = LOGFONTW::default();
        if unsafe { GetObjectW(handle as _, std::mem::size_of::<LOGFONTW>() as i32, (&mut native as *mut LOGFONTW).cast()) } == 0 { return; }
        // WM_SETFONT resets RichEdit's character styles, including existing bold
        // spans. Change only the inherited face/size instead.
        let mut value = format();
        value.Base.dwMask = re::CFM_SIZE;
        value.Base.yHeight = native.lfHeight.unsigned_abs().saturating_mul(1440).checked_div(unsafe { GetDpiForWindow(self.window) }.max(1)).unwrap_or(240) as i32;
        if matches!(self.content, Content::Editor { .. }) {
            value.Base.dwMask.0 |= re::CFM_FACE.0;
            value.Base.szFaceName = native.lfFaceName;
        }
        apply_format(self.window, &value, re::SCF_ALL);
    }
    pub fn new(leaf: &Leaf, spec: &HostedContent) -> Result<Self, String> {
        static LIBRARY: OnceLock<usize> = OnceLock::new();
        let library = *LIBRARY.get_or_init(|| unsafe { LoadLibraryExW(wide("Msftedit.dll").as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) as usize });
        if library == 0 { return Err(native_error("load system rich editor")); }
        let content = match spec {
            HostedContent::Terminal { columns, rows, scrollback, .. } => Content::Terminal {
                screen: TerminalScreen::new(TerminalSpec { columns: *columns, rows: *rows, scrollback: *scrollback }).map_err(str::to_string)?,
                configuration: TerminalSpec { columns: *columns, rows: *rows, scrollback: *scrollback },
                revision: None, application_cursor: false, bracketed_paste: false },
            HostedContent::RichEditor { .. } => Content::Editor { plan: prepare_native_html("").map_err(str::to_string)?, runs: vec![], source: String::new(), published: String::new() },
            _ => return Err("unsupported rich text content".into()),
        };
        // ES_NOOLEDRAGDROP prevents an unvalidated OLE object from entering HTML.
        let window = leaf.child("RICHEDIT50W", "", ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL | 8)?;
        if unsafe { SetWindowSubclass(window, Some(procedure), TEXT_SUBCLASS, leaf as *const Leaf as usize) } == 0 { return Err(native_error("rich text subclass")); }
        unsafe {
            SendMessageW(window, re::EM_EXLIMITTEXT, 0, 1_048_576);
            SendMessageW(window, re::EM_SETEVENTMASK, 0, (re::ENM_CHANGE | re::ENM_SELCHANGE) as isize);
            SendMessageW(window, re::EM_SETUNDOLIMIT, 100, 0);
            // Disable automatic URLs: native link activation must not bypass navigation policy.
            SendMessageW(window, re::EM_AUTOURLDETECT, 0, 0);
        }
        let document = if matches!(content, Content::Editor { .. }) { Some(document(window)?) } else { None };
        Ok(Self { window, document, content, dirty: false, selection_dirty: false, surrogate: None, composing: false })
    }
    pub fn update(&mut self, leaf: &Leaf, spec: &HostedContent) -> Result<(), String> {
        match (&mut self.content, spec) {
            (Content::Terminal { screen, configuration, revision, application_cursor, bracketed_paste }, HostedContent::Terminal { columns, rows, scrollback, input, input_revision }) => {
                let configured = TerminalSpec { columns: *columns, rows: *rows, scrollback: *scrollback };
                let changed = *configuration != configured;
                screen.configure(configured.clone()).map_err(str::to_string)?;
                *configuration = configured;
                if *revision == Some(*input_revision) && !changed { return Ok(()); }
                if *revision != Some(*input_revision) {
                    if let Some(input) = input { for chunk in input.as_bytes().chunks(64 * 1024) { screen.feed(chunk).map_err(str::to_string)?; } }
                }
                *revision = Some(*input_revision);
                let frame = screen.frame();
                *application_cursor = frame.application_cursor; *bracketed_paste = frame.bracketed_paste;
                let text: String = frame.runs.iter().map(|run| run.text.as_str()).collect();
                if text.encode_utf16().count() > 1_048_576 { return Err("terminal display exceeds budget".into()); }
                unsafe {
                    SendMessageW(self.window, WM_SETREDRAW, 0, 0);
                    SendMessageW(self.window, EM_SETREADONLY, 0, 0);
                    SetWindowTextW(self.window, wide(&text.replace('\n', "\r")).as_ptr());
                    SendMessageW(self.window, re::EM_SETBKGNDCOLOR, 0, color(0x1d1f21).0 as isize);
                }
                let mut default = style_format(&CellStyle::default());
                default.Base.dwMask.0 |= re::CFM_FACE.0;
                let face: Vec<u16> = "Cascadia Mono".encode_utf16().collect();
                default.Base.szFaceName[..face.len()].copy_from_slice(&face);
                apply_format(self.window, &default, re::SCF_ALL);
                let mut offset = 0;
                for run in frame.runs {
                    let end = offset + run.text.encode_utf16().count();
                    select(self.window, [offset, end]); apply_format(self.window, &style_format(&run.style), re::SCF_SELECTION); offset = end;
                }
                select(self.window, [frame.cursor_utf16; 2]);
                unsafe {
                    SendMessageW(self.window, EM_SCROLLCARET, 0, 0);
                    SendMessageW(self.window, EM_EMPTYUNDOBUFFER, 0, 0);
                    SendMessageW(self.window, EM_SETREADONLY, 1, 0);
                    SendMessageW(self.window, WM_SETREDRAW, 1, 0); InvalidateRect(self.window, null_mut(), 1);
                }
                for reply in frame.replies { leaf.defer("input", serde_json::json!({"bytes": reply})); }
            }
            (Content::Editor { plan, runs, source, published }, HostedContent::RichEditor { html, editable, selection, .. }) => {
                if html != source && html != published {
                    let next = prepare_native_html(html).map_err(str::to_string)?;
                    let next_runs = next.text_runs().map_err(str::to_string)?;
                    let text: String = next_runs.iter().map(|run| run.text.as_str()).collect();
                    unsafe { SendMessageW(self.window, EM_SETREADONLY, 0, 0); SetWindowTextW(self.window, wide(&text.replace('\n', "\r")).as_ptr()); }
                    let mut offset = 0;
                    for (index, run) in next_runs.iter().enumerate() {
                        let end = offset + run.text.encode_utf16().count(); select(self.window, [offset, end]);
                        let value = html_format(&next, run);
                        unsafe { font(self.document.as_ref().ok_or("missing rich text document")?, offset, end)?.SetCookie(index as i32 + 1) }
                            .map_err(|error| format!("retain rich text element: {error}"))?;
                        apply_format(self.window, &value, re::SCF_SELECTION);
                        offset = end;
                    }
                    *plan = next; *runs = next_runs; *published = html.clone();
                    unsafe { SendMessageW(self.window, EM_EMPTYUNDOBUFFER, 0, 0); }
                }
                source.clone_from(html);
                unsafe { SendMessageW(self.window, EM_SETREADONLY, usize::from(!editable), 0); }
                if let Some(range) = selection {
                    let text = native_text(self.window)?;
                    let clamp = |index| { let scalar = jadix_core::presentation::input::utf16_to_scalar(&text, index); text.chars().take(scalar).map(char::len_utf16).sum() };
                    select(self.window, [clamp(range[0]), clamp(range[1])]);
                }
            }
            _ => return Err("native text kind changed without recreation".into()),
        }
        self.dirty = false; self.selection_dirty = false;
        Ok(())
    }
    pub fn notify(&mut self, notification: u32) { if notification == EN_CHANGE { self.dirty = true; } }
    pub fn selection_changed(&mut self, code: u32) { if code == re::EN_SELCHANGE { self.selection_dirty = true; } }
    pub fn publish(&mut self, leaf: &Leaf) -> Result<(), String> {
        let HostedContent::RichEditor { revision, editable, .. } = *leaf.spec.borrow() else { return Ok(()); };
        if self.composing { return Ok(()); }
        if self.dirty && editable {
            self.dirty = false;
            leaf.updating.set(true);
            let result = self.export();
            leaf.updating.set(false);
            let html = result?;
            if let Content::Editor { published, .. } = &mut self.content { published.clone_from(&html); }
            leaf.defer("change", serde_json::json!({"html": html, "revision": revision}));
        }
        if std::mem::take(&mut self.selection_dirty) { leaf.defer("selection", serde_json::json!({"range": selected(self.window), "revision": revision})); }
        Ok(())
    }
    fn export(&self) -> Result<String, String> {
        let Content::Editor { plan, runs, .. } = &self.content else { return Err("not a rich editor".into()); };
        let text = native_text(self.window)?; let units: Vec<u16> = text.encode_utf16().collect();
        let selection = selected(self.window);
        let mut edited = Vec::new(); let mut output_plan = plan.clone();
        let result = collect(self.window, self.document.as_ref().ok_or("missing rich text document")?, &units, 0, units.len(), runs, &mut output_plan, &mut edited)
            .and_then(|()| output_plan.export_runs(&edited).map_err(str::to_string));
        select(self.window, selection);
        result
    }
    fn input(&self, leaf: &Leaf, value: String) { if value.len() <= 64 * 1024 { leaf.emit("input", serde_json::json!({"bytes": value})); } }
    fn paste(&self, leaf: &Leaf) {
        use jadix_core::clipboard::{ClipboardRequest, ClipboardResponse};
        let Ok(ClipboardResponse::Item(Some(item))) = clipboard::request(self.window, &ClipboardRequest::Read { mime_types: vec!["text/plain".into()] }) else { return; };
        let Ok(mut text) = String::from_utf8(item.bytes) else { return; };
        if text.len() > 64 * 1024 - 12 { return; }
        if matches!(self.content, Content::Terminal { bracketed_paste: true, .. }) { text = format!("\x1b[200~{text}\x1b[201~"); }
        self.input(leaf, text);
    }
}
impl Drop for Text { fn drop(&mut self) { unsafe { RemoveWindowSubclass(self.window, Some(procedure), TEXT_SUBCLASS); } } }

fn html_format(plan: &NativeHtmlPlan, run: &NativeHtmlRun) -> re::CHARFORMAT2W {
    let mut value = format();
    value.Base.dwMask = re::CFM_MASK(re::CFM_BOLD.0 | re::CFM_ITALIC.0 | re::CFM_UNDERLINE.0 | re::CFM_STRIKEOUT.0 | re::CFM_COLOR.0 | re::CFM_BACKCOLOR.0);
    value.Base.dwEffects = re::CFE_EFFECTS(re::CFE_AUTOCOLOR.0 | re::CFE_AUTOBACKCOLOR.0);
    for element in run.elements.iter().filter_map(|&index| plan.elements.get(index)) {
        value.Base.dwEffects.0 |= match element.name.as_str() { "b" | "strong" => re::CFE_BOLD.0, "em" | "i" => re::CFE_ITALIC.0, "u" => re::CFE_UNDERLINE.0, "s" | "strike" | "del" => re::CFE_STRIKEOUT.0, _ => 0 };
        for (_, style) in element.attributes.iter().filter(|(name, _)| name == "style") {
            for pair in style.split(';') { let Some((key, text)) = pair.split_once(':') else { continue; }; let text = text.trim();
                match key.trim() {
                    "font-weight" if text == "bold" || text.parse::<u16>().is_ok_and(|n| n >= 600) => value.Base.dwEffects.0 |= re::CFE_BOLD.0,
                    "font-style" if text == "italic" => value.Base.dwEffects.0 |= re::CFE_ITALIC.0,
                    "text-decoration" if text.contains("underline") => value.Base.dwEffects.0 |= re::CFE_UNDERLINE.0,
                    "color" | "background-color" => if let Some(rgb) = html_color(text) {
                        if key.trim() == "color" { value.Base.crTextColor = color(rgb); value.Base.dwEffects.0 &= !re::CFE_AUTOCOLOR.0; }
                        else { value.crBackColor = color(rgb); value.Base.dwEffects.0 &= !re::CFE_AUTOBACKCOLOR.0; }
                    },
                    _ => {}
                }
            }
        }
    }
    value
}
fn html_color(text: &str) -> Option<u32> {
    if let Some(hex) = text.strip_prefix('#') { return match hex.len() { 6 => u32::from_str_radix(hex, 16).ok(), 3 => u32::from_str_radix(&hex.chars().flat_map(|c| [c, c]).collect::<String>(), 16).ok(), _ => None }; }
    Some(match text.to_ascii_lowercase().as_str() { "black" => 0, "white" => 0xffffff, "red" => 0xff0000, "green" => 0x008000, "blue" => 0x0000ff, "yellow" => 0xffff00, "gray" | "grey" => 0x808080, _ => return None })
}
fn collect(window: HWND, document: &re::ITextDocument2, text: &[u16], start: usize, end: usize, originals: &[NativeHtmlRun], plan: &mut NativeHtmlPlan, runs: &mut Vec<NativeHtmlRun>) -> Result<(), String> {
    if start == end { return Ok(()); }
    if runs.len() >= 32_768 { return Err("native rich text formatting exceeds budget".into()); }
    select(window, [start, end]); let mut value = format();
    unsafe { SendMessageW(window, re::EM_GETCHARFORMAT, re::SCF_SELECTION as usize, &mut value as *mut _ as isize); }
    let cookie = unsafe { font(document, start, end)?.GetCookie() }.map_err(|error| format!("read rich text element: {error}"))?;
    let required = re::CFM_BOLD.0 | re::CFM_ITALIC.0 | re::CFM_UNDERLINE.0 | re::CFM_STRIKEOUT.0 | re::CFM_COLOR.0 | re::CFM_BACKCOLOR.0;
    if (cookie == re::tomUndefined.0 || value.Base.dwMask.0 & required != required) && end - start > 1 {
        let mut middle = start + (end - start) / 2;
        if (0xdc00..=0xdfff).contains(&text[middle]) { middle -= 1; }
        if middle > start {
            collect(window, document, text, start, middle, originals, plan, runs)?;
            return collect(window, document, text, middle, end, originals, plan, runs);
        }
    }
    let original = cookie.checked_sub(1).and_then(|index| originals.get(index as usize));
    let mut run = original.cloned().unwrap_or(NativeHtmlRun { text: String::new(), elements: vec![], image: None, structural: false });
    run.text = String::from_utf16(&text[start..end]).map_err(|_| "native rich text split a Unicode character")?;
    let effects = value.Base.dwEffects.0;
    let css = format!("font-weight:{};font-style:{};text-decoration:{};", if effects & re::CFE_BOLD.0 != 0 { "bold" } else { "normal" }, if effects & re::CFE_ITALIC.0 != 0 { "italic" } else { "normal" },
        match (effects & re::CFE_UNDERLINE.0 != 0, effects & re::CFE_STRIKEOUT.0 != 0) { (true, true) => "underline line-through", (true, false) => "underline", (false, true) => "line-through", _ => "none" });
    let css = [(&css[..]).to_string(), if effects & re::CFE_AUTOCOLOR.0 == 0 { format!("color:#{:02x}{:02x}{:02x};", value.Base.crTextColor.0 & 255, value.Base.crTextColor.0 >> 8 & 255, value.Base.crTextColor.0 >> 16 & 255) } else { String::new() },
        if effects & re::CFE_AUTOBACKCOLOR.0 == 0 { format!("background-color:#{:02x}{:02x}{:02x};", value.crBackColor.0 & 255, value.crBackColor.0 >> 8 & 255, value.crBackColor.0 >> 16 & 255) } else { String::new() }].concat();
    let index = plan.elements.len();
    plan.elements.push(NativeHtmlElement { name: "span".into(), attributes: vec![("style".into(), css.clone())], open: format!("<span style=\"{css}\">"), close: "</span>".into(), block: false, parent: run.elements.last().copied() });
    run.elements.push(index); runs.push(run); Ok(())
}

unsafe extern "system" fn procedure(window: HWND, message: u32, parameter: WPARAM, data: LPARAM, _: usize, reference: usize) -> LRESULT {
    let leaf = unsafe { &*(reference as *const Leaf) };
    if leaf.updating.get() { return unsafe { DefSubclassProc(window, message, parameter, data) }; }
    if message == WM_SETFOCUS || message == WM_KILLFOCUS {
        if let HostedContent::RichEditor { revision, .. } = *leaf.spec.borrow() { leaf.defer("focus", serde_json::json!({"focused": message == WM_SETFOCUS, "revision": revision})); }
    }
    if message == WM_IME_STARTCOMPOSITION || message == WM_IME_ENDCOMPOSITION {
        if let Some(text) = leaf.text.borrow_mut().as_mut() { text.composing = message == WM_IME_STARTCOMPOSITION; }
    }
    let terminal = matches!(*leaf.spec.borrow(), HostedContent::Terminal { .. });
    if terminal {
        if message == WM_GETDLGCODE { return (DLGC_WANTALLKEYS | DLGC_WANTCHARS) as isize; }
        if message == WM_PASTE { if let Some(text) = leaf.text.borrow().as_ref() { text.paste(leaf); } return 0; }
        if message == WM_IME_COMPOSITION {
            if data as u32 & GCS_RESULTSTR != 0 {
                let context = unsafe { ImmGetContext(window) };
                if !context.is_null() {
                    let size = unsafe { ImmGetCompositionStringW(context, GCS_RESULTSTR, null_mut(), 0) };
                    if (0..=128 * 1024).contains(&size) {
                        let mut buffer = vec![0u16; size as usize / 2];
                        let read = unsafe { ImmGetCompositionStringW(context, GCS_RESULTSTR, buffer.as_mut_ptr().cast(), size as u32) };
                        if read == size { if let Ok(value) = String::from_utf16(&buffer) { if let Some(text) = leaf.text.borrow().as_ref() { text.input(leaf, value); } } }
                    }
                    unsafe { ImmReleaseContext(window, context); }
                }
            }
            return 0;
        }
        if message == WM_KEYDOWN || message == WM_SYSKEYDOWN {
            let control = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
            let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
            let alt = unsafe { GetKeyState(VK_MENU as i32) } < 0;
            if control && shift && parameter == b'C' as usize { unsafe { SendMessageW(window, WM_COPY, 0, 0); } return 0; }
            if control && shift && parameter == b'V' as usize { if let Some(text) = leaf.text.borrow().as_ref() { text.paste(leaf); } return 0; }
            let key = match parameter { 0x25 => "left", 0x26 => "up", 0x27 => "right", 0x28 => "down", 0x24 => "home", 0x23 => "end", 0x2d => "insert", 0x2e => "delete", 0x21 => "page_up", 0x22 => "page_down", 9 => "tab", 13 => "return", 8 => "backspace", 27 => "escape", _ => "" };
            let composing = leaf.text.borrow().as_ref().is_some_and(|text| text.composing);
            if composing { return unsafe { DefSubclassProc(window, message, parameter, data) }; }
            if let Some(text) = leaf.text.borrow().as_ref() {
                let app = matches!(text.content, Content::Terminal { application_cursor: true, .. });
                let key = if (0x70..=0x7b).contains(&parameter) { format!("f{}", parameter - 0x6f) } else { key.into() };
                if let Some(bytes) = jadix_core::terminal::special_key(&key, shift, alt, control, app) { text.input(leaf, bytes); return 0; }
            }
        }
        if message == WM_CHAR || message == WM_SYSCHAR {
            let mut state = leaf.text.borrow_mut();
            if let Some(text) = state.as_mut() {
                let unit = parameter as u16;
                if unsafe { GetKeyState(VK_CONTROL as i32) } < 0 && unsafe { GetKeyState(VK_SHIFT as i32) } < 0 && matches!(unit, 3 | 22) { return 0; }
                if matches!(unit, 8 | 9 | 13 | 27) { return 0; } // keydown already emitted these.
                if (0xd800..=0xdbff).contains(&unit) { text.surrogate = Some(unit); return 0; }
                let units = text.surrogate.take().map_or_else(|| vec![unit], |high| vec![high, unit]);
                if let Ok(mut value) = String::from_utf16(&units) {
                    if message == WM_SYSCHAR { value.insert(0, '\x1b'); }
                    text.input(leaf, value);
                }
            }
            return 0;
        }
    } else if message == WM_PASTE {
        // Accept plain Unicode only; imported HTML/OLE objects must pass the shared parser.
        let editable = matches!(*leaf.spec.borrow(), HostedContent::RichEditor { editable: true, .. });
        if editable { unsafe { SendMessageW(window, re::EM_PASTESPECIAL, 13, 0); } } return 0;
    }
    unsafe { DefSubclassProc(window, message, parameter, data) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::NativeRoot;

    fn surface(spec: HostedContent) -> (NativeRoot<WindowsHost>, NodeId) {
        let host = WindowsHost::new("Jadix Windows hosted text probe", [640, 360]).unwrap();
        let mut root = NativeRoot::new(host);
        let id = NodeId::new(root.model().id());
        let descriptor = NodeDescriptor {
            content: NodeContent::Hosted { specification: serde_json::to_string(&spec).unwrap() },
            frame: Rect { x: 12.0, y: 12.0, width: 600.0, height: 320.0 },
            style: Default::default(), accessible_role: None, accessible_selected: None,
            accessible_expanded: None, interaction: Default::default(), keyboard: Default::default(),
            navigation_keys: Vec::new(), accessibility: Default::default(),
            accessibility_relations: Default::default(), accessible_description: None,
            accessible_name: None,
        };
        root.present(&mut [PresentationNode { id, descriptor, children: vec![] }], [640.0, 360.0], 0).unwrap();
        root.host().show(); root.host().pump();
        (root, id)
    }

    #[test]
    fn windows_rich_editor_round_trips_native_edits_and_undo() {
        let (mut root, id) = surface(HostedContent::RichEditor { html: "<p><b>A😀</b> tail</p>".into(), editable: true, selection: None, revision: 7 });
        let window = root.host().hosted[&id].children.borrow()[0];
        select(window, [0, 3]);
        let mut value = format();
        unsafe { SendMessageW(window, re::EM_GETCHARFORMAT, re::SCF_SELECTION as usize, &mut value as *mut _ as isize); }
        assert_eq!(unsafe { font(&document(window).unwrap(), 0, 3).unwrap().GetCookie() }.unwrap(), 1, "system RichEdit must retain shared markup identities");
        assert_ne!(value.Base.dwEffects.0 & re::CFE_BOLD.0, 0);
        select(window, [1, 3]);
        unsafe { SendMessageW(window, EM_REPLACESEL, 1, wide("中文🙂").as_ptr() as isize); }
        assert!(native_text(window).unwrap().starts_with("A中文🙂 tail"));
        // A layout refresh can reapply the acknowledged value before events drain.
        // It must preserve both the native edit and its undo notification.
        root.host().hosted[&id].update(HostedContent::RichEditor { html: "<p><b>A😀</b> tail</p>".into(), editable: true, selection: None, revision: 7 }).unwrap();
        let events = root.poll_events().unwrap();
        let event = events.iter().find_map(|event| match &event.payload { EventPayload::Hosted { event, data } if event == "change" => Some(data), _ => None }).expect("native edit must publish change");
        assert_eq!(event["revision"], 7);
        let html = event["html"].as_str().unwrap();
        assert!(html.contains("中文🙂"), "{html}"); assert!(html.contains("<b>"), "{html}");
        unsafe { SendMessageW(window, EM_UNDO, 0, 0); }
        assert!(native_text(window).unwrap().starts_with("A😀 tail"));
        assert!(root.poll_events().unwrap().iter().any(|event| matches!(&event.payload, EventPayload::Hosted { event, .. } if event == "change")));
    }

    #[test]
    fn windows_terminal_preserves_stream_styles_and_repaints_resize() {
        let spec = HostedContent::Terminal { columns: 20, rows: 4, scrollback: 10, input: Some("\x1b[31m红😀\x1b[0m\x1b[6n".into()), input_revision: 1 };
        let (mut root, id) = surface(spec);
        let window = root.host().hosted[&id].children.borrow()[0];
        assert!(native_text(window).unwrap().starts_with("红😀"));
        select(window, [0, 1]); let mut value = format();
        unsafe { SendMessageW(window, re::EM_GETCHARFORMAT, re::SCF_SELECTION as usize, &mut value as *mut _ as isize); }
        assert_ne!(value.Base.crTextColor.0, color(0xc5c8c6).0);
        assert!(root.poll_events().unwrap().iter().any(|event| matches!(&event.payload, EventPayload::Hosted { event, data } if event == "input" && data["bytes"].as_str().is_some_and(|s| s.ends_with('R')))));
        root.host().hosted[&id].update(HostedContent::Terminal { columns: 10, rows: 2, scrollback: 10, input: Some("must not replay".into()), input_revision: 1 }).unwrap();
        let resized = native_text(window).unwrap();
        assert_eq!(resized.matches('\n').count(), 1, "{resized:?}"); assert!(!resized.contains("must not"));
        unsafe { SendMessageW(window, WM_KEYDOWN, 0x25, 0); }
        assert!(root.poll_events().unwrap().iter().any(|event| matches!(&event.payload, EventPayload::Hosted { event, data } if event == "input" && data["bytes"] == "\x1b[D")));
    }
}
