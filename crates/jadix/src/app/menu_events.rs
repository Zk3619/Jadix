//! Muda has one process-wide event handler shared by menu bars and tray menus.
//! Install it once so enabling both features never replaces either dispatcher.

#[cfg(not(all(
    feature = "system-tray",
    any(
        windows,
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
)))]
use muda::MenuEvent;
#[cfg(all(
    feature = "system-tray",
    any(
        windows,
        target_os = "macos",
        all(target_os = "linux", not(target_env = "ohos"))
    )
))]
use tray_icon::menu::MenuEvent;

pub(super) fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let id = event.id.0;
            super::run_on_ui_thread(move || {
                #[cfg(all(
                    feature = "system-tray",
                    any(
                        windows,
                        target_os = "macos",
                        all(target_os = "linux", not(target_env = "ohos"))
                    )
                ))]
                super::tray::dispatch_tray_menu_click(&id);
                super::native_menu::dispatch(&id);
            });
        }));
    });
}
