use super::*;
use jadix_core::desktop_services::{ServiceCommand, ServiceError, ServiceEvent, ServiceReply};
use jadix_core::environment::{EnvironmentSnapshot, Lifecycle, SystemEnvironmentWatcher};
use jadix_net::{native_fetch::NativeFetch, native_websocket::NativeWebSocket};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};

pub(super) struct Services {
    pub fetch: RefCell<NativeFetch>,
    pub audio: RefCell<super::audio::Audio>,
    pub speech: RefCell<super::speech::Speech>,
    pub files: super::files::Files,
    pub socket: NativeWebSocket,
    pub notifications: super::notifications::Notifications,
    pub sharing: RefCell<super::share::Sharing>,
    pub environment: RefCell<Result<EnvironmentSnapshot, String>>,
    watcher: SystemEnvironmentWatcher,
}

impl Services {
    pub fn new(host: &WindowsHost) -> Result<Self, String> {
        let curl = system_curl()?;
        let fetch = NativeFetch::new(curl.clone(), host.waker());
        fetch.service().set_command_waker(Some(std::rc::Rc::new(host.waker())));
        host.context.desktop.set_command_waker(Some(std::rc::Rc::new(host.waker())));
        Ok(Self {
            audio: RefCell::new(super::audio::Audio::new(curl, host.waker())?),
            speech: RefCell::new(super::speech::Speech::new(host.waker())),
            fetch: RefCell::new(fetch),
            files: super::files::Files::new(host.native_window(), host.waker())?,
            socket: NativeWebSocket::new(host.waker())?,
            notifications: super::notifications::Notifications::new(host.waker()),
            sharing: RefCell::new(super::share::Sharing::new(host.waker())),
            environment: RefCell::new(jadix_core::environment::system_snapshot().map_err(|e| e.to_string())),
            watcher: SystemEnvironmentWatcher::start(host.waker()).map_err(|e| e.to_string())?,
        })
    }

    pub fn pump(&self, context: &WindowContext) {
        self.fetch.borrow_mut().pump();
        self.audio.borrow_mut().pump();
        self.speech.borrow_mut().pump();
        self.sharing.borrow_mut().pump(context.root.get());
        if let Some(snapshot) = self.watcher.take() {
            *self.environment.borrow_mut() = snapshot.map_err(|e| e.to_string());
            context.environment.set(context.environment.get().saturating_add(1));
        }
        // Apply cancellation before delivering callbacks queued on a WinRT thread.
        for command in context.desktop.take_commands() {
            let id = command.id();
            let error = match command {
                ServiceCommand::RegisterShortcut { shortcut, .. } => {
                    let number = (1..=0xbfff).find(|n| !context.hotkeys.borrow().contains_key(n));
                    let key = shortcut_key(&shortcut.key);
                    let modifiers = MOD_NOREPEAT | if shortcut.command { MOD_WIN } else { 0 }
                        | if shortcut.control { MOD_CONTROL } else { 0 }
                        | if shortcut.option { MOD_ALT } else { 0 }
                        | if shortcut.shift { MOD_SHIFT } else { 0 };
                    match (number, key) {
                        (Some(number), Some(key)) if context.hotkeys.borrow().len() < 256 => {
                            if unsafe { RegisterHotKey(context.root.get(), number, modifiers, key) } != 0 {
                                context.hotkeys.borrow_mut().insert(number, id);
                                None
                            } else { Some(ServiceError::Conflict) }
                        }
                        _ => Some(ServiceError::Failed),
                    }
                }
                ServiceCommand::UnregisterShortcut { .. } => {
                    let number = context.hotkeys.borrow().iter().find_map(|(n, owner)| (*owner == id).then_some(*n));
                    if let Some(number) = number {
                        unsafe { UnregisterHotKey(context.root.get(), number); }
                        context.hotkeys.borrow_mut().remove(&number);
                    }
                    continue;
                }
                ServiceCommand::Permission { .. } => {
                    let (authorization, error) = self.notifications.permission();
                    context.desktop.reply(id, ServiceReply::Permission { authorization, error });
                    continue;
                }
                ServiceCommand::Notify { notification, .. } => self.notifications.notify(id, notification).err(),
                ServiceCommand::CancelNotification { .. } => { self.notifications.cancel(id); continue; }
                ServiceCommand::Haptic { .. } => None,
                ServiceCommand::Badge { label, .. } => context.badge.set(context.root.get(), label.as_deref()).err().map(|_| ServiceError::Failed),
                ServiceCommand::OpenAuthSession { request, .. } => {
                    let enabled = context.deep_links.borrow().router.config().schemes.contains(&request.callback_scheme.to_ascii_lowercase());
                    let error = if request.ephemeral || !enabled { Some(ServiceError::Unavailable) }
                        else { super::files::open(context.root.get() as usize, wide(&request.url).as_ptr()).err().map(|_| ServiceError::Failed) };
                    if let Some(error) = error { context.desktop.reply(id, ServiceReply::AuthSession { outcome: None, error: Some(error) }); }
                    continue;
                }
                ServiceCommand::CloseAuthSession { .. } => continue,
            };
            context.desktop.reply(id, ServiceReply::Complete { error });
        }
        self.notifications.pump(&context.desktop, context.root.get());
    }

    pub fn snapshot(&self, context: &WindowContext) -> Result<Option<EnvironmentSnapshot>, String> {
        let mut snapshot = self.environment.borrow().clone()?;
        snapshot.lifecycle = if context.closed.get() || unsafe { IsIconic(context.root.get()) != 0 || IsWindowVisible(context.root.get()) == 0 } {
            Lifecycle::Background
        } else if unsafe { GetForegroundWindow() } == context.root.get() { Lifecycle::Active } else { Lifecycle::Inactive };
        snapshot.wake_count = context.wake_count.get();
        Ok(Some(snapshot))
    }

    pub fn dispose(&mut self, context: &WindowContext) {
        self.audio.borrow_mut().dispose();
        self.speech.borrow_mut().dispose();
        self.sharing.borrow_mut().dispose();
        context.desktop.dispose();
        self.notifications.dispose();
        for number in context.hotkeys.borrow().keys() { unsafe { UnregisterHotKey(context.root.get(), *number); } }
        context.hotkeys.borrow_mut().clear();
        self.files.dispose();
        self.fetch.borrow_mut().dispose();
        self.socket.dispose();
        self.watcher.close();
    }
}

fn shortcut_key(key: &str) -> Option<u32> {
    if key.len() == 1 && key.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) { return Some(u32::from(key.as_bytes()[0])); }
    if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=20).contains(n)) { return Some(0x6f + n); }
    Some(match key {
        "Space" => 0x20, "Tab" => 9, "Enter" => 13, "Escape" => 27,
        "ArrowLeft" => 0x25, "ArrowUp" => 0x26, "ArrowRight" => 0x27, "ArrowDown" => 0x28,
        "Home" => 0x24, "End" => 0x23, "PageUp" => 0x21, "PageDown" => 0x22,
        "Backspace" => 8, "Delete" => 0x2e, _ => return None,
    })
}

pub(super) fn hotkey(context: &WindowContext, number: i32) {
    if let Some(id) = context.hotkeys.borrow().get(&number) { context.desktop.event(*id, ServiceEvent::Shortcut); }
}

pub(super) fn system_curl() -> Result<std::path::PathBuf, String> {
    let mut system = [0u16; 32768];
    let size = unsafe { windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) } as usize;
    if size == 0 || size >= system.len() { return Err(native_error("GetSystemDirectoryW")); }
    let curl = std::path::PathBuf::from(String::from_utf16(&system[..size]).map_err(|e| e.to_string())?).join("curl.exe");
    Ok(curl)
}
