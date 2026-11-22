#![cfg(all(windows, feature = "windows"))]

use jadix_core::desktop::{Share, ShareOutcome};
use jadix_native::{host::PlatformHost, windows::WindowsHost};
use std::time::{Duration, Instant};

#[test]
#[ignore = "desktop acceptance: close the real Windows share picker using its close button"]
fn system_share_picker_reports_user_dismissal() {
    let mut host = WindowsHost::new("Jadix system sharing acceptance", [640, 400]).unwrap();
    host.show();
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(host.native_window() as _);
    }
    let shown = Instant::now() + Duration::from_secs(2);
    while Instant::now() < shown {
        host.pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    let path = std::env::temp_dir().join(format!(
        "jadix-share-desktop-{}-中文😀.txt",
        std::process::id()
    ));
    std::fs::write(&path, "真实桌面分享 😀").unwrap();
    let response = host
        .show_share(&Share {
            subject: Some("Jadix 桌面分享 😀".into()),
            text: Some("中文分享内容 😀".into()),
            url: Some("https://example.com/jadix".into()),
            files: vec![path.clone()],
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    let result = loop {
        host.pump();
        if let Ok(result) = response.try_recv() {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "system share picker did not report dismissal"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(result, Ok(ShareOutcome::Dismissed));
    drop(host);
    std::fs::remove_file(path).unwrap();
}
