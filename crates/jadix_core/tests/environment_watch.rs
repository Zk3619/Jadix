#![cfg(all(feature = "system-environment", any(windows, target_os = "linux")))]
use jadix_core::environment::*;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

fn receive(watcher: &SystemEnvironmentWatcher, rx: &mpsc::Receiver<()>) -> EnvironmentSnapshot {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(value) = watcher.take() {
            return value.unwrap();
        }
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn native_notifications_coalesce_and_join_before_close_returns() {
    let (tx, rx) = mpsc::channel();
    let mut watcher = SystemEnvironmentWatcher::start(move || {
        let _ = tx.send(());
    })
    .unwrap();
    let initial = receive(&watcher, &rx);
    initial.validate().unwrap();
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        // Real OS broadcast delivery, without changing the user's preferences.
        let mut result = 0;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                0,
                SMTO_ABORTIFHUNG,
                1000,
                &mut result,
            );
        }
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            watcher.take().is_none(),
            "unchanged values must not publish"
        );
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("JADIX_TEST_SETTINGS").is_some() {
        // The acceptance runner supplies a private D-Bus and XDG config root.
        assert!(std::env::var("XDG_CONFIG_HOME").unwrap().contains("jadix"));
        let set = |key, value| {
            assert!(std::process::Command::new("gsettings")
                .args(["set", "org.gnome.desktop.interface", key, value])
                .status()
                .unwrap()
                .success())
        };
        let target = if initial.color_scheme == Some(ColorScheme::Light) {
            "prefer-dark"
        } else {
            "prefer-light"
        };
        set("color-scheme", target);
        let next = receive(&watcher, &rx);
        assert_ne!(next.color_scheme, initial.color_scheme);
        set("enable-animations", "false");
        set("text-scaling-factor", "1.25");
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let next = receive(&watcher, &rx);
            if next.reduced_motion == Some(true) && next.font_scale == 1.25 {
                break;
            }
            assert!(Instant::now() < deadline);
        }
        println!("isolated Linux theme/motion/font notifications passed");
    }
    watcher.close();
    assert!(watcher.is_finished());
    assert!(watcher.take().is_none());
    watcher.close();
    while rx.try_recv().is_ok() {}
    assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
}
