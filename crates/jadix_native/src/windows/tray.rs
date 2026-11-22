use super::*;
use jadix_core::tray::Tray;
use windows_sys::Win32::UI::Shell::{Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICON_VERSION_4};

pub(super) const MESSAGE: u32 = WM_APP + 0x4a3;

#[derive(Default)]
pub(super) struct StatusItem {
    value: RefCell<Option<Tray>>,
    window: Cell<HWND>,
    registered: Cell<bool>,
    revision: Cell<u64>,
    events: RefCell<VecDeque<String>>,
}

impl StatusItem {
    pub fn set(&self, window: HWND, value: Option<&Tray>) -> Result<(), String> {
        if let Some(value) = value { value.validate()?; }
        if self.window.get() == window && self.value.borrow().as_ref() == value && (value.is_none() || self.registered.get()) { return Ok(()); }
        if value.is_none() { self.dispose(); return Ok(()); }
        let mut data = self.data(window);
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = MESSAGE;
        data.hIcon = unsafe { SendMessageW(window, WM_GETICON, ICON_SMALL as usize, 0) } as HICON;
        if data.hIcon.is_null() { data.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) }; }
        let value = value.unwrap();
        let title = value.plain_title();
        copy_text(&mut data.szTip, if value.tooltip.is_empty() { &title } else { &value.tooltip });
        if unsafe { Shell_NotifyIconW(if self.registered.get() { NIM_MODIFY } else { NIM_ADD }, &data) } == 0 { return Err(native_error("publish status item")); }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } == 0 {
            unsafe { Shell_NotifyIconW(NIM_DELETE, &data); }
            self.registered.set(false);
            return Err(native_error("set status item version"));
        }
        self.window.set(window);
        self.registered.set(true);
        *self.value.borrow_mut() = Some(value.clone());
        self.revision.set(self.revision.get().wrapping_add(1));
        self.events.borrow_mut().clear();
        Ok(())
    }

    fn data(&self, window: HWND) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: window, uID: 1, ..Default::default() }
    }

    pub fn restart(&self, window: HWND) -> Result<(), String> {
        self.registered.set(false);
        let value = self.value.borrow().clone();
        self.set(window, value.as_ref())
    }

    pub fn message(&self, window: HWND, data: LPARAM) {
        // Version 4 packs the icon identity in HIWORD and the event in LOWORD.
        if ((data as u32 >> 16) & 0xffff) != 1 || !self.registered.get() { return; }
        if !matches!(data as u32 & 0xffff, NIN_SELECT | 0x405 /* NIN_KEYSELECT */ | WM_CONTEXTMENU) { return; }
        let Some(value) = self.value.borrow().clone() else { return; };
        let revision = self.revision.get();
        let mut ids = Vec::new();
        let Ok(menu) = hosted::Menu::new(&value.menu, &mut ids) else { return; };
        let title = value.plain_title();
        if !title.is_empty() {
            // Windows status items have no adjacent text API. Retain the full
            // shared title as a menu heading, beyond the shell tooltip limit.
            unsafe { InsertMenuW(menu.0, 0, MF_BYPOSITION | MF_GRAYED, 0, wide(&title).as_ptr()); }
        }
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point); SetForegroundWindow(window); }
        // TrackPopupMenu runs a nested message loop. Hold no RefCell borrow and
        // reject a selection if that loop replaced or removed this menu.
        let selected = unsafe { TrackPopupMenuEx(menu.0, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, point.x, point.y, window, null_mut()) };
        unsafe { PostMessageW(window, WM_NULL, 0, 0); }
        if revision == self.revision.get() && selected > 0 {
            if let Some(id) = ids.get(selected as usize - 1).filter(|id| value.accepts_command(id)) {
                let mut events = self.events.borrow_mut();
                if events.len() < 256 { events.push_back(id.clone()); }
            }
        }
    }

    pub fn take_events(&self) -> Vec<String> { self.events.borrow_mut().drain(..).collect() }

    pub fn dispose(&self) {
        if self.registered.replace(false) { unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data(self.window.get())); } }
        self.window.set(null_mut());
        self.value.borrow_mut().take();
        self.events.borrow_mut().clear();
        self.revision.set(self.revision.get().wrapping_add(1));
    }
}
impl Drop for StatusItem { fn drop(&mut self) { self.dispose(); } }

pub(super) fn taskbar_created() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) })
}

fn copy_text(target: &mut [u16], text: &str) {
    let mut length = 0;
    for character in text.chars() {
        if length + character.len_utf16() >= target.len() { break; }
        let mut units = [0; 2];
        for unit in character.encode_utf16(&mut units) { target[length] = *unit; length += 1; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tooltip_limit_keeps_surrogate_pairs_and_terminator() {
        let mut buffer = [0; 5];
        copy_text(&mut buffer, "中🙂🙂");
        assert_eq!(String::from_utf16(&buffer[..3]).unwrap(), "中🙂");
        assert_eq!(&buffer[3..], &[0, 0]);
    }
}
