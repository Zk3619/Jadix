use super::*;

#[test]
fn compiled_kind_matches_cfg() {
    let kind = compiled_host_kind();
    if cfg!(target_os = "windows") {
        assert_eq!(kind, HostKind::Windows);
    } else if cfg!(target_os = "macos") {
        assert_eq!(kind, HostKind::Macos);
    } else if cfg!(target_os = "linux") && !cfg!(target_env = "ohos") {
        assert_eq!(kind, HostKind::Linux);
    } else if cfg!(target_os = "ios") {
        assert_eq!(kind, HostKind::Ios);
    } else if cfg!(target_os = "android") {
        assert_eq!(kind, HostKind::Android);
    } else if cfg!(target_arch = "wasm32") {
        assert_eq!(kind, HostKind::Web);
    }
    assert!(!kind.as_str().is_empty());
}

#[test]
fn desktop_capabilities_allow_multi_window() {
    let caps = HostCapabilities::for_kind(compiled_host_kind());
    if matches!(
        compiled_host_kind(),
        HostKind::Windows | HostKind::Macos | HostKind::Linux
    ) {
        assert!(caps.multi_window);
        assert!(caps.decorations);
        assert!(caps.blocking_run);
        assert!(!caps.recreate_window_after_destroy);
    }
}

#[test]
fn mobile_and_web_capability_tables() {
    let ios = HostCapabilities::for_kind(HostKind::Ios);
    assert!(!ios.multi_window);
    assert!(ios.suspend_resume);
    assert!(ios.blocking_run);

    let android = HostCapabilities::for_kind(HostKind::Android);
    assert!(android.recreate_window_after_destroy);
    assert!(android.suspend_resume);

    let web = HostCapabilities::for_kind(HostKind::Web);
    assert!(!web.blocking_run);
    assert!(!web.multi_window);

    let ohos = HostCapabilities::for_kind(HostKind::HarmonyOs);
    assert!(ohos.suspend_resume);
    assert!(ohos.recreate_window_after_destroy);
    assert!(!ohos.blocking_run);
}

#[test]
fn host_kind_display_is_stable() {
    assert_eq!(HostKind::Windows.to_string(), "windows");
    assert_eq!(HostKind::HarmonyOs.to_string(), "harmonyos");
    assert_eq!(HostKind::Web.to_string(), "web");
}

#[test]
fn ensure_winit_host_ok_without_external() {
    set_external_host(None);
    assert!(ensure_winit_host().is_ok());
}
