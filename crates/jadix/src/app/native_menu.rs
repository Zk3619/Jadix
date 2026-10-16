//! Optional OS menu bars. Windows attaches a menu to each HWND; macOS swaps
//! the application menu when a window gains focus. Linux winit windows are not
//! GTK windows: installation returns an error there (keep using `Menubar`).
//!
//! Install on the UI thread, after a window exists. Callbacks run in that
//! window's reactive/input scope. Replacing/closing a menu discards stale events.

use super::multi_window::WindowHandle;
use std::rc::Rc;
use winit::window::WindowId;

/// A command, nested submenu, or separator in an OS menu.
#[derive(Clone)]
pub enum NativeMenuItem {
    Command {
        id: String,
        label: String,
        enabled: bool,
        /// Muda syntax, e.g. `CmdOrCtrl+Shift+S`.
        accelerator: Option<String>,
        on_select: Rc<dyn Fn()>,
    },
    Submenu {
        label: String,
        enabled: bool,
        items: Vec<NativeMenuItem>,
    },
    Separator,
}

impl NativeMenuItem {
    pub fn command(
        id: impl Into<String>,
        label: impl Into<String>,
        on_select: impl Fn() + 'static,
    ) -> Self {
        Self::Command {
            id: id.into(),
            label: label.into(),
            enabled: true,
            accelerator: None,
            on_select: Rc::new(on_select),
        }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<Self>) -> Self {
        Self::Submenu {
            label: label.into(),
            enabled: true,
            items,
        }
    }

    pub fn enabled(mut self, value: bool) -> Self {
        match &mut self {
            Self::Command { enabled, .. } | Self::Submenu { enabled, .. } => *enabled = value,
            Self::Separator => {}
        }
        self
    }

    pub fn accelerator(mut self, value: impl Into<String>) -> Self {
        if let Self::Command { accelerator, .. } = &mut self {
            *accelerator = Some(value.into());
        }
        self
    }
}

/// Whether OS menu bars are compiled for this host. Does not imply runtime validation.
pub const fn native_menu_supported() -> bool {
    cfg!(all(
        feature = "native-menu",
        any(windows, target_os = "macos")
    ))
}

/// Replace a window's menu. Invalid IDs/accelerators leave its old menu intact.
/// Top-level entries must be submenus (required by the macOS application menu).
pub fn set_window_menu(window: &WindowHandle, items: Vec<NativeMenuItem>) -> Result<(), String> {
    #[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
    {
        native::install(window, items)
    }
    #[cfg(not(all(feature = "native-menu", any(windows, target_os = "macos"))))]
    {
        let _ = (window, items);
        Err("OS menu bars require native-menu on Windows or macOS; use the in-app Menubar on this host".into())
    }
}

pub fn clear_window_menu(window: &WindowHandle) {
    remove_window(window.id());
}

pub(crate) fn remove_window(id: WindowId) {
    #[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
    native::remove(id);
    #[cfg(not(all(feature = "native-menu", any(windows, target_os = "macos"))))]
    let _ = id;
}

pub(crate) fn focus_window(id: WindowId) {
    #[cfg(all(feature = "native-menu", target_os = "macos"))]
    native::focus(id);
    #[cfg(not(all(feature = "native-menu", target_os = "macos")))]
    let _ = id;
}

pub(crate) fn dispose_all() {
    #[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
    native::dispose_all();
}

#[cfg(any(
    feature = "system-tray",
    all(feature = "native-menu", any(windows, target_os = "macos"))
))]
pub(crate) fn dispatch(id: &str) {
    #[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
    native::dispatch(id);
    #[cfg(not(all(feature = "native-menu", any(windows, target_os = "macos"))))]
    let _ = id;
}

#[cfg(all(feature = "native-menu", windows))]
pub(crate) unsafe fn translate_accelerator(message: *const std::ffi::c_void) -> bool {
    // SAFETY: caller supplies winit's live MSG pointer on the event-loop thread.
    unsafe { native::translate_accelerator(message) }
}

#[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
mod native {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Installed {
        menu: muda::Menu,
        window: WindowHandle,
        callbacks: HashMap<String, Rc<dyn Fn()>>,
    }

    thread_local! {
        static MENUS: RefCell<HashMap<WindowId, Installed>> = RefCell::new(HashMap::new());
        #[cfg(target_os = "macos")]
        static FOCUSED: std::cell::Cell<Option<WindowId>> = const { std::cell::Cell::new(None) };
    }

    fn append_items(
        items: Vec<NativeMenuItem>,
        generation: u64,
        ids: &mut HashSet<String>,
        callbacks: &mut HashMap<String, Rc<dyn Fn()>>,
        parent_enabled: bool,
    ) -> Result<Vec<Box<dyn muda::IsMenuItem>>, String> {
        items
            .into_iter()
            .map(|item| -> Result<Box<dyn muda::IsMenuItem>, String> {
                match item {
                    NativeMenuItem::Command {
                        id,
                        label,
                        enabled,
                        accelerator,
                        on_select,
                    } => {
                        if id.trim().is_empty() || !ids.insert(id.clone()) {
                            return Err(format!(
                                "native menu: empty or duplicate command id {id:?}"
                            ));
                        }
                        let accelerator = accelerator
                            .map(|s| {
                                s.parse::<muda::accelerator::Accelerator>()
                                    .map_err(|e| e.to_string())
                            })
                            .transpose()?;
                        let native_id = format!("jadix-menu-{generation}-{id}");
                        if enabled && parent_enabled {
                            callbacks.insert(native_id.clone(), on_select);
                        }
                        Ok(Box::new(muda::MenuItem::with_id(
                            native_id,
                            label,
                            enabled,
                            accelerator,
                        )))
                    }
                    NativeMenuItem::Submenu {
                        label,
                        enabled,
                        items,
                    } => {
                        let submenu = muda::Submenu::new(label, enabled);
                        let children = append_items(
                            items,
                            generation,
                            ids,
                            callbacks,
                            parent_enabled && enabled,
                        )?;
                        for child in children {
                            submenu.append(child.as_ref()).map_err(|e| e.to_string())?;
                        }
                        Ok(Box::new(submenu))
                    }
                    NativeMenuItem::Separator => {
                        Ok(Box::new(muda::PredefinedMenuItem::separator()))
                    }
                }
            })
            .collect()
    }

    fn build(
        items: Vec<NativeMenuItem>,
    ) -> Result<(muda::Menu, HashMap<String, Rc<dyn Fn()>>), String> {
        if items
            .iter()
            .any(|i| !matches!(i, NativeMenuItem::Submenu { .. }))
        {
            return Err("native menu: top-level entries must be submenus".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let generation = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut callbacks = HashMap::new();
        let children = append_items(items, generation, &mut HashSet::new(), &mut callbacks, true)?;
        let menu = muda::Menu::new();
        for child in children {
            menu.append(child.as_ref()).map_err(|e| e.to_string())?;
        }
        Ok((menu, callbacks))
    }

    #[cfg(windows)]
    fn hwnd(window: &dyn winit::window::Window) -> Result<isize, String> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
            RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
            _ => Err("native menu: expected a Win32 window".into()),
        }
    }

    pub(super) fn install(window: &WindowHandle, items: Vec<NativeMenuItem>) -> Result<(), String> {
        if !super::super::runtime::window_handles()
            .iter()
            .any(|w| w.id() == window.id())
        {
            return Err("native menu: install on the window's UI thread".into());
        }
        let live = window.upgrade()?;
        let (menu, callbacks) = build(items)?;
        super::super::menu_events::install();
        #[cfg(windows)]
        {
            let hwnd = hwnd(live.as_ref())?;
            // Detach the previous subclass before installing the new one.
            remove(window.id());
            // SAFETY: live keeps this HWND valid; installation runs on the UI thread.
            unsafe { menu.init_for_hwnd(hwnd) }.map_err(|e| e.to_string())?;
        }
        let installed = Installed {
            menu,
            callbacks,
            window: window.clone(),
        };
        MENUS.with(|m| m.borrow_mut().insert(window.id(), installed));
        #[cfg(target_os = "macos")]
        {
            if live.has_focus() || FOCUSED.with(|f| f.get() == Some(window.id())) {
                focus(window.id());
            }
        }
        Ok(())
    }

    pub(super) fn remove(id: WindowId) {
        let installed = MENUS
            .try_with(|m| m.borrow_mut().remove(&id))
            .ok()
            .flatten();
        if let Some(installed) = installed {
            #[cfg(windows)]
            if let Ok(window) = installed.window.upgrade() {
                if let Ok(hwnd) = hwnd(window.as_ref()) {
                    // SAFETY: the upgraded Arc keeps the window alive through detach.
                    let _ = unsafe { installed.menu.remove_for_hwnd(hwnd) };
                }
            }
            #[cfg(target_os = "macos")]
            if FOCUSED.try_with(|f| f.get() == Some(id)).unwrap_or(false) {
                installed.menu.remove_for_nsapp();
                let _ = FOCUSED.try_with(|f| f.set(None));
            }
        }
    }

    pub(super) fn dispose_all() {
        let ids = MENUS.with(|m| m.borrow().keys().copied().collect::<Vec<_>>());
        for id in ids {
            remove(id);
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn focus(id: WindowId) {
        MENUS.with(|m| {
            let menus = m.borrow();
            if let Some(menu) = menus.get(&id) {
                menu.menu.init_for_nsapp();
                FOCUSED.with(|f| f.set(Some(id)));
            } else if let Some(old) = FOCUSED
                .with(|f| f.replace(None))
                .and_then(|id| menus.get(&id))
            {
                old.menu.remove_for_nsapp();
            }
        });
    }

    pub(super) fn dispatch(id: &str) {
        let callback = MENUS.with(|m| {
            m.borrow().values().find_map(|menu| {
                menu.callbacks
                    .get(id)
                    .cloned()
                    .map(|cb| (menu.window.clone(), cb))
            })
        });
        let Some((window, callback)) = callback else {
            return;
        };
        if !window.is_alive() {
            return;
        }
        let primary = super::super::runtime::window_handles()
            .first()
            .map(WindowHandle::id);
        let _scope = if Some(window.id()) == primary {
            super::super::window_scope::enter_primary()
        } else {
            super::super::window_scope::enter_secondary_id(window.id())
        };
        // No registry borrow across callbacks: commands can replace menus / mutate Signals.
        callback();
        super::super::runtime::request_redraw();
    }

    #[cfg(windows)]
    pub(super) unsafe fn translate_accelerator(message: *const std::ffi::c_void) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetAncestor, TranslateAcceleratorW, GA_ROOT, MSG,
        };
        // SAFETY: winit owns this MSG for the duration of its message hook.
        let msg = unsafe { &*message.cast::<MSG>() };
        let root = unsafe { GetAncestor(msg.hwnd, GA_ROOT) };
        // Clone before TranslateAccelerator: Windows can re-enter the message loop.
        let menu = MENUS.with(|m| {
            m.borrow().values().find_map(|menu| {
                let window = menu.window.upgrade().ok()?;
                (hwnd(window.as_ref()).ok()? == root as isize).then(|| menu.menu.clone())
            })
        });
        menu.is_some_and(|menu| unsafe {
            TranslateAcceleratorW(root, menu.haccel() as _, msg) != 0
        })
    }

    #[cfg(all(test, windows))]
    mod tests {
        use super::*;

        #[test]
        fn rejects_duplicate_ids_and_invalid_shortcuts_before_install() {
            let cmd = || NativeMenuItem::command("save", "Save", || {});
            assert!(build(vec![NativeMenuItem::submenu("File", vec![cmd(), cmd()])]).is_err());
            assert!(build(vec![NativeMenuItem::submenu(
                "File",
                vec![cmd().accelerator("Ctrl+DefinitelyNotAKey")]
            )])
            .is_err());
            assert!(build(vec![cmd()]).is_err());
        }

        #[test]
        fn generations_isolate_stale_events_and_disabled_ancestors() {
            let items = || {
                vec![NativeMenuItem::submenu(
                    "File",
                    vec![NativeMenuItem::command("save", "Save", || {}).accelerator("CmdOrCtrl+S")],
                )]
            };
            let (_, a) = build(items()).unwrap();
            let (_, b) = build(items()).unwrap();
            assert_eq!(a.len(), 1);
            assert!(!b.contains_key(a.keys().next().unwrap()));
            let (_, disabled) = build(vec![NativeMenuItem::submenu(
                "File",
                vec![NativeMenuItem::command("save", "Save", || {})],
            )
            .enabled(false)])
            .unwrap();
            assert!(disabled.is_empty());
        }
    }
}
