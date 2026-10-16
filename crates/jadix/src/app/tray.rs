//! # System tray
//!
//! System tray icon + menu. The default build is a compilable stub; enable feature `system-tray` and access it.
//! [`tray-icon`](https://crates.io/crates/tray-icon) (contains muda menu).
//! 菜单点击从原生线程经 [`crate::app::run_on_ui_thread`] 回到 UI 线程再调 `on_click`。
//! See [`docs/ops/tray.md`](../../../../docs/ops/tray.md).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "system-tray")]
use super::run_on_ui_thread;

thread_local! {
    static MENU_CLICKS: RefCell<HashMap<String, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
    #[cfg(feature = "system-tray")]
    static NATIVE: RefCell<HashMap<u64, NativeTray>> = RefCell::new(HashMap::new());
}

static NEXT_TRAY_INSTANCE: AtomicU64 = AtomicU64::new(1);

fn next_instance() -> u64 {
    NEXT_TRAY_INSTANCE.fetch_add(1, Ordering::Relaxed)
}

fn callback_key(instance: u64, item_id: &str) -> String {
    format!("jadix-tray-{instance}-{item_id}")
}

fn register_menu_callbacks(instance: u64, items: &[TrayMenuItem]) -> Vec<String> {
    let mut keys = Vec::new();
    MENU_CLICKS.with(|map| {
        let mut map = map.borrow_mut();
        for item in items {
            let key = callback_key(instance, &item.id);
            if let Some(cb) = &item.on_click {
                map.insert(key.clone(), Rc::clone(cb));
            }
            keys.push(key);
        }
    });
    keys
}

fn unregister_menu_callbacks(keys: &[String]) {
    // `try_with`: TrayIcon may Drop during TLS teardown (Gallery stores the handle
    // in `thread_local`). `with` panics with AccessError → abort (0xC0000409).
    let _ = MENU_CLICKS.try_with(|map| {
        let mut map = map.borrow_mut();
        for key in keys {
            map.remove(key);
        }
    });
}

/// Drop native icons and callbacks while the event loop / UI TLS are still alive.
pub(crate) fn dispose_trays_for_shutdown() {
    #[cfg(feature = "system-tray")]
    {
        let natives = NATIVE
            .try_with(|m| std::mem::take(&mut *m.borrow_mut()))
            .unwrap_or_default();
        drop(natives);
    }
    let _ = MENU_CLICKS.try_with(|m| m.borrow_mut().clear());
}

/// Dispatch a native menu id on the UI thread. Native path calls this from
/// [`run_on_ui_thread`]; tests call it directly.
#[cfg(any(test, feature = "system-tray"))]
pub(crate) fn dispatch_tray_menu_click(menu_id: &str) {
    let cb = MENU_CLICKS
        .try_with(|map| map.borrow().get(menu_id).cloned())
        .ok()
        .flatten();
    if let Some(cb) = cb {
        cb();
    }
}

/// `true` when this crate was built with native tray-icon wiring.
pub const fn system_tray_native() -> bool {
    cfg!(feature = "system-tray")
}

/// 托盘菜单项。
#[derive(Clone)]
pub struct TrayMenuItem {
    pub id: String,
    pub label: String,
    pub on_click: Option<Rc<dyn Fn()>>,
}

impl TrayMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on_click: None,
        }
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

/// [`TrayIcon`] 构建器。
pub struct TrayIconBuilder {
    tooltip: String,
    menu: Vec<TrayMenuItem>,
    icon_rgba: Option<(Vec<u8>, u32, u32)>,
}

/// 系统托盘句柄。启用 `system-tray` 时持有原生图标；否则为无副作用 stub。
///
/// 句柄必须保持存活，否则原生托盘会随 [`Drop`] 移除。
pub struct TrayIcon {
    pub tooltip: String,
    pub menu: Vec<TrayMenuItem>,
    #[cfg_attr(not(feature = "system-tray"), allow(dead_code))]
    instance: u64,
    callback_keys: Vec<String>,
}

#[cfg(feature = "system-tray")]
struct NativeTray {
    icon: tray_icon::TrayIcon,
    _items: Vec<tray_icon::menu::MenuItem>,
}

/// Start building the tray icon.
pub fn tray_icon() -> TrayIconBuilder {
    TrayIconBuilder {
        tooltip: String::new(),
        menu: Vec::new(),
        icon_rgba: None,
    }
}

impl TrayIconBuilder {
    pub fn tooltip(mut self, s: impl Into<String>) -> Self {
        self.tooltip = s.into();
        self
    }

    pub fn menu_item(mut self, item: TrayMenuItem) -> Self {
        self.menu.push(item);
        self
    }

    pub fn menu(mut self, items: Vec<TrayMenuItem>) -> Self {
        self.menu = items;
        self
    }

    /// 32bpp RGBA 图标。未设置时，原生路径使用内置圆点。
    pub fn icon_rgba(mut self, rgba: Vec<u8>, width: u32, height: u32) -> Self {
        self.icon_rgba = Some((rgba, width, height));
        self
    }

    /// 构建句柄。`system-tray` 开启时尝试注册原生托盘；失败则降为 stub 并打 warn。
    pub fn build(self) -> TrayIcon {
        let instance = next_instance();
        let callback_keys = register_menu_callbacks(instance, &self.menu);

        #[cfg(feature = "system-tray")]
        {
            ensure_menu_event_bridge();
            match try_build_native(&self, instance) {
                Ok(native) => {
                    log::debug!("jadix tray: native TrayIcon registered");
                    NATIVE.with(|m| {
                        m.borrow_mut().insert(instance, native);
                    });
                    return TrayIcon {
                        tooltip: self.tooltip,
                        menu: self.menu,
                        instance,
                        callback_keys,
                    };
                }
                Err(e) => {
                    log::warn!("jadix tray: native registration failed ({e}); stub handle");
                }
            }
        }
        #[cfg(not(feature = "system-tray"))]
        {
            let _ = &self.icon_rgba;
            log::debug!("jadix tray: stub TrayIcon (enable feature system-tray for native)");
        }
        TrayIcon {
            tooltip: self.tooltip,
            menu: self.menu,
            instance,
            callback_keys,
        }
    }
}

impl TrayIcon {
    /// 显示托盘图标。stub 下为空操作。
    pub fn show(&self) {
        #[cfg(feature = "system-tray")]
        {
            let _ = NATIVE.try_with(|m| {
                if let Some(native) = m.borrow().get(&self.instance) {
                    if let Err(e) = native.icon.set_visible(true) {
                        log::warn!("jadix tray: show failed: {e}");
                    }
                }
            });
        }
    }

    /// 隐藏托盘图标。stub 下为空操作。
    pub fn hide(&self) {
        #[cfg(feature = "system-tray")]
        {
            let _ = NATIVE.try_with(|m| {
                if let Some(native) = m.borrow().get(&self.instance) {
                    if let Err(e) = native.icon.set_visible(false) {
                        log::warn!("jadix tray: hide failed: {e}");
                    }
                }
            });
        }
    }

    /// 更新 tooltip。原生路径同步到系统托盘。
    pub fn set_tooltip(&mut self, s: impl Into<String>) {
        self.tooltip = s.into();
        #[cfg(feature = "system-tray")]
        {
            let _ = NATIVE.try_with(|m| {
                if let Some(native) = m.borrow().get(&self.instance) {
                    if let Err(e) = native.icon.set_tooltip(Some(self.tooltip.as_str())) {
                        log::warn!("jadix tray: set_tooltip failed: {e}");
                    }
                }
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn callback_keys(&self) -> &[String] {
        &self.callback_keys
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unregister_menu_callbacks(&self.callback_keys);
        #[cfg(feature = "system-tray")]
        {
            let native = NATIVE
                .try_with(|m| m.borrow_mut().remove(&self.instance))
                .ok()
                .flatten();
            drop(native);
        }
    }
}

#[cfg(feature = "system-tray")]
fn ensure_menu_event_bridge() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tray_icon::menu::MenuEvent::set_event_handler(Some(|event: tray_icon::menu::MenuEvent| {
            let id = event.id.0.clone();
            run_on_ui_thread(move || dispatch_tray_menu_click(&id));
        }));
    });
}

#[cfg(feature = "system-tray")]
fn default_tray_rgba() -> (Vec<u8>, u32, u32) {
    const S: u32 = 32;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let i = ((y * S + x) * 4) as usize;
            if dx * dx + dy * dy <= 14.0 * 14.0 {
                rgba[i] = 0x25;
                rgba[i + 1] = 0x63;
                rgba[i + 2] = 0xeb;
                rgba[i + 3] = 255;
            }
        }
    }
    (rgba, S, S)
}

#[cfg(feature = "system-tray")]
fn try_build_native(builder: &TrayIconBuilder, instance: u64) -> Result<NativeTray, String> {
    use tray_icon::menu::{Menu, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder as NativeBuilder};

    let (rgba, w, h) = builder.icon_rgba.clone().unwrap_or_else(default_tray_rgba);
    let icon = Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())?;

    let menu = Menu::new();
    let mut native_items = Vec::with_capacity(builder.menu.len());
    for spec in &builder.menu {
        let item = MenuItem::with_id(callback_key(instance, &spec.id), &spec.label, true, None);
        menu.append(&item).map_err(|e| e.to_string())?;
        native_items.push(item);
    }

    let native = NativeBuilder::new()
        .with_icon(icon)
        .with_tooltip(&builder.tooltip)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|e| e.to_string())?;

    Ok(NativeTray {
        icon: native,
        _items: native_items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn stub_build_stores_tooltip_and_menu() {
        let tray = tray_icon()
            .tooltip("hello")
            .menu_item(TrayMenuItem::new("quit", "Quit"))
            .build();
        assert_eq!(tray.tooltip, "hello");
        assert_eq!(tray.menu.len(), 1);
        assert_eq!(tray.menu[0].id, "quit");
        tray.show();
        tray.hide();
    }

    #[test]
    fn icon_rgba_is_accepted_on_builder() {
        let tray = tray_icon().icon_rgba(vec![255, 0, 0, 255], 1, 1).build();
        let _ = tray.tooltip;
    }

    #[test]
    fn menu_click_dispatch_invokes_on_click() {
        let hit = Rc::new(Cell::new(false));
        let hit2 = Rc::clone(&hit);
        let tray = tray_icon()
            .menu_item(TrayMenuItem::new("ping", "Ping").on_click(move || hit2.set(true)))
            .build();
        let key = tray.callback_keys()[0].clone();
        dispatch_tray_menu_click(&key);
        assert!(hit.get(), "on_click must run on UI-thread dispatch");
    }

    #[test]
    fn drop_unregisters_menu_callbacks() {
        let tray = tray_icon()
            .menu_item(TrayMenuItem::new("x", "X").on_click(|| {}))
            .build();
        let key = tray.callback_keys()[0].clone();
        drop(tray);
        // After drop, dispatch is a no-op (would panic if we stored a dangling Rc).
        dispatch_tray_menu_click(&key);
    }

    #[test]
    fn system_tray_native_matches_feature() {
        assert_eq!(system_tray_native(), cfg!(feature = "system-tray"));
    }

    #[cfg(feature = "system-tray")]
    #[test]
    fn default_icon_from_rgba_is_valid() {
        let (rgba, w, h) = default_tray_rgba();
        tray_icon::Icon::from_rgba(rgba, w, h).expect("default tray rgba");
    }

    #[test]
    fn drop_from_thread_local_does_not_panic_on_thread_exit() {
        thread_local! {
            static SLOT: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
        }
        let join = std::thread::Builder::new()
            .name("tray-tls-drop".into())
            .spawn(|| {
                SLOT.with(|s| {
                    *s.borrow_mut() = Some(
                        tray_icon()
                            .menu_item(TrayMenuItem::new("x", "X").on_click(|| {}))
                            .build(),
                    );
                });
            })
            .expect("spawn");
        join.join()
            .expect("TrayIcon Drop must not AccessError TLS during thread-local teardown");
    }
}
