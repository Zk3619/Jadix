use super::*;
use std::cell::Cell;

#[test]
fn shared_icon_builder_and_unavailable_native_updates() {
    let icon = super::super::AppIcon::from_rgba(vec![42; 16], 2, 2).unwrap();
    let builder = tray_icon().icon(icon.clone());
    assert_eq!(builder.icon_rgba.as_ref().unwrap(), &(vec![42; 16], 2, 2));
    #[cfg(not(feature = "system-tray"))]
    {
        assert!(builder.try_build().is_err());
        let tray = tray_icon().build();
        assert!(!tray.is_native());
        assert!(tray.set_icon(icon).is_err());
    }
}

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
    assert_eq!(
        system_tray_native(),
        cfg!(all(
            feature = "system-tray",
            any(windows, target_os = "macos", target_os = "linux")
        ))
    );
}

#[cfg(all(
    feature = "system-tray",
    any(windows, target_os = "macos", target_os = "linux")
))]
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
