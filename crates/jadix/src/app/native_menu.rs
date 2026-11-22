//! Optional OS menu bars. Windows attaches a menu to each HWND; macOS swaps
//! the application menu when a window gains focus. Linux winit windows are not
//! GTK windows: installation returns an error there (keep using `Menubar`).
//!
//! Install on the UI thread, after a window exists. Callbacks run in that
//! window's reactive/input scope. Replacing/closing a menu discards stale events.

use super::multi_window::WindowHandle;
#[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
use std::rc::Rc;
use winit::window::WindowId;

pub use jadix_core::menu::MenuItem as NativeMenuItem;

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
    jadix_core::menu::validate(&items)?;
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

#[cfg(feature = "i18n")]
pub(crate) fn refresh_locale() {
    #[cfg(all(feature = "native-menu", any(windows, target_os = "macos")))]
    native::refresh_locale();
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
        labels: Vec<(jadix_core::menu::MenuRole, muda::PredefinedMenuItem)>,
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
        labels: &mut Vec<(jadix_core::menu::MenuRole, muda::PredefinedMenuItem)>,
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
                        state,
                        ..
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
                            callbacks.insert(native_id.clone(), on_select.clone());
                        }
                        use jadix_core::menu::{MenuRole, MenuState};
                        if let MenuState::Role(role) | MenuState::LocalizedRole(role) = state {
                            if !enabled || !parent_enabled {
                                return Err(
                                    "disabled native roles must be removed from the menu".into()
                                );
                            }
                            let item = match role {
                                MenuRole::Copy => muda::PredefinedMenuItem::copy(Some(&label)),
                                MenuRole::Cut => muda::PredefinedMenuItem::cut(Some(&label)),
                                MenuRole::Paste => muda::PredefinedMenuItem::paste(Some(&label)),
                                MenuRole::Undo => muda::PredefinedMenuItem::undo(Some(&label)),
                                MenuRole::Redo => muda::PredefinedMenuItem::redo(Some(&label)),
                                MenuRole::SelectAll => {
                                    muda::PredefinedMenuItem::select_all(Some(&label))
                                }
                                MenuRole::Quit => muda::PredefinedMenuItem::quit(Some(&label)),
                            };
                            if matches!(state, MenuState::LocalizedRole(_)) {
                                labels.push((role, item.clone()));
                            }
                            return Ok(Box::new(item));
                        }
                        let checked = match state {
                            MenuState::Checked(v) => Some(v),
                            MenuState::Radio { selected, .. } => Some(selected),
                            _ => None,
                        };
                        if let Some(checked) = checked {
                            let item = muda::CheckMenuItem::with_id(
                                native_id.clone(),
                                label,
                                enabled,
                                checked,
                                accelerator,
                            );
                            if enabled && parent_enabled {
                                let reset = item.clone();
                                callbacks.insert(
                                    native_id,
                                    Rc::new(move || {
                                        reset.set_checked(checked);
                                        on_select();
                                    }),
                                );
                            }
                            return Ok(Box::new(item));
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
                        ..
                    } => {
                        let submenu = muda::Submenu::new(label, enabled);
                        let children = append_items(
                            items,
                            generation,
                            ids,
                            callbacks,
                            parent_enabled && enabled,
                            labels,
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

    struct BuiltMenu {
        menu: muda::Menu,
        callbacks: HashMap<String, Rc<dyn Fn()>>,
        labels: Vec<(jadix_core::menu::MenuRole, muda::PredefinedMenuItem)>,
    }

    fn build(mut items: Vec<NativeMenuItem>) -> Result<BuiltMenu, String> {
        jadix_core::menu::localize(&mut items, &menu_locale())?;
        if items
            .iter()
            .any(|i| !matches!(i, NativeMenuItem::Submenu { .. }))
        {
            return Err("native menu: top-level entries must be submenus".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let generation = NEXT.fetch_add(1, Ordering::Relaxed);
        let mut callbacks = HashMap::new();
        let mut labels = Vec::new();
        let children = append_items(
            items,
            generation,
            &mut HashSet::new(),
            &mut callbacks,
            true,
            &mut labels,
        )?;
        let menu = muda::Menu::new();
        for child in children {
            menu.append(child.as_ref()).map_err(|e| e.to_string())?;
        }
        Ok(BuiltMenu {
            menu,
            callbacks,
            labels,
        })
    }

    fn menu_locale() -> String {
        #[cfg(feature = "i18n")]
        {
            crate::i18n::locale().to_string()
        }
        #[cfg(not(feature = "i18n"))]
        {
            "en".into()
        }
    }

    fn window_scope(id: WindowId) -> super::super::window_scope::WindowScopeGuard {
        let primary = super::super::runtime::window_handles()
            .first()
            .map(WindowHandle::id);
        if Some(id) == primary {
            super::super::window_scope::enter_primary()
        } else {
            super::super::window_scope::enter_secondary_id(id)
        }
    }

    #[cfg(feature = "i18n")]
    pub(super) fn refresh_locale() {
        let menus = MENUS.with(|menus| {
            menus
                .borrow()
                .iter()
                .map(|(id, menu)| (*id, menu.labels.clone()))
                .collect::<Vec<_>>()
        });
        for (id, labels) in menus {
            let _scope = window_scope(id);
            let locale = menu_locale();
            for (role, item) in labels {
                item.set_text(role.label(&locale));
            }
        }
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
        let _scope = window_scope(window.id());
        let BuiltMenu {
            menu,
            callbacks,
            labels,
        } = build(items)?;
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
            labels,
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
    #[path = "tests/mod.rs"]
    mod tests;
}
