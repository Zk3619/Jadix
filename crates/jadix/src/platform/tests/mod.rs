#[test]
fn process_private_mb_nonzero_on_supported_desktop() {
    let mb = super::process_private_mb();
    if cfg!(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )) {
        assert!(
            mb > 0.0,
            "expected a real process metric on Win/macOS/Linux, got {mb}"
        );
    }
}

#[test]
fn system_scroll_lines_positive() {
    let n = super::system_scroll_lines();
    assert!(n > 0.0, "scroll lines must be positive, got {n}");
}

#[test]
fn eager_ui_font_files_nonempty() {
    assert!(
        !super::eager_ui_font_files().is_empty(),
        "each desktop OS must list at least one eager UI face"
    );
}

#[test]
fn resolve_system_font_joins_first_dir() {
    let name = super::eager_ui_font_files()[0];
    let expected = super::system_font_dirs()[0].join(name);
    let got = super::resolve_system_font(name);
    if expected.is_file() {
        assert_eq!(got.as_deref(), Some(expected.as_path()));
    } else {
        assert!(got.is_none());
    }
}

#[cfg(target_os = "windows")]
#[test]
fn windows_host_exposes_segoe_and_yahei_names() {
    assert_eq!(super::eager_ui_font_files()[0], "segoeui.ttf");
    assert_eq!(
        super::lazy_script_font_files(super::SystemFontScript::Cjk)[0],
        "msyh.ttc"
    );
}

#[test]
fn edit_shortcut_modifiers_match_host() {
    use winit::keyboard::ModifiersState;
    let ctrl = super::edit_shortcut_modifiers(&ModifiersState::CONTROL);
    let super_key = super::edit_shortcut_modifiers(&ModifiersState::META);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        assert!(super_key && !ctrl);
    } else {
        assert!(ctrl && !super_key);
    }
}

#[test]
fn gpu_backend_candidates_match_host() {
    let opaque = super::gpu_backend_candidates(false);
    assert!(!opaque.is_empty());
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        assert_eq!(opaque.first().map(|(_, n)| *n), Some("metal"));
    } else if cfg!(target_arch = "wasm32") {
        assert_eq!(opaque.first().map(|(_, n)| *n), Some("webgpu"));
    } else if cfg!(target_os = "android") {
        assert_eq!(opaque.first().map(|(_, n)| *n), Some("gl"));
    } else if cfg!(target_os = "windows") {
        assert_eq!(opaque.first().map(|(_, n)| *n), Some("gl"));
    }

    let compositor = super::gpu_backend_candidates(true);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        assert_eq!(compositor.first().map(|(_, n)| *n), Some("metal"));
    } else if cfg!(target_arch = "wasm32") {
        assert_eq!(compositor.first().map(|(_, n)| *n), Some("webgpu"));
    } else if cfg!(target_os = "windows") {
        if cfg!(feature = "backend-vulkan") {
            assert_eq!(compositor.first().map(|(_, n)| *n), Some("vulkan"));
        } else if cfg!(feature = "backend-dx12") {
            assert_eq!(compositor.first().map(|(_, n)| *n), Some("dx12"));
        } else {
            assert_eq!(
                compositor.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
                vec!["gl"]
            );
        }
    }
}

#[test]
fn local_hms_sane_on_supported_desktop() {
    match super::local_hms() {
        Some((h, m, s)) => {
            assert!(h < 24 && m < 60 && s < 60, "hms=({h},{m},{s})");
        }
        None => {
            if cfg!(any(
                target_os = "windows",
                target_os = "macos",
                target_os = "linux"
            )) {
                panic!("Win/macOS/Linux must expose a local clock");
            }
        }
    }
}

#[test]
fn enable_ansi_stderr_does_not_panic() {
    super::enable_ansi_stderr();
    super::enable_ansi_stderr();
}
