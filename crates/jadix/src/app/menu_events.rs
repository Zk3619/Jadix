//! Muda has one process-wide event handler shared by menu bars and tray menus.
//! Install it once so enabling both features never replaces either dispatcher.

#[cfg(not(feature = "system-tray"))]
use muda::MenuEvent;
#[cfg(feature = "system-tray")]
use tray_icon::menu::MenuEvent;

pub(super) fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let id = event.id.0;
            super::run_on_ui_thread(move || {
                #[cfg(feature = "system-tray")]
                super::tray::dispatch_tray_menu_click(&id);
                super::native_menu::dispatch(&id);
            });
        }));
    });
}
