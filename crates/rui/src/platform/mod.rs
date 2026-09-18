//! OS-specific hooks.
//!
//! Cross-platform window *options* stay on [`crate::app::WindowOptions`].
//! Implementations: `desktop` defaults, `os` overrides (Windows DWM / PE stack,
//! macOS Mach RSS, Linux `/proc`). Public path `crate::platform::*` is stable.
//!
//! Architecture: `docs/engine/platform.md`.
//!
//! The framework does not emit PE `/STACK` or require a large main-thread stack.
//! Deep UI trees are walked with heap work queues instead of unbounded recursion.

use std::path::PathBuf;

use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

mod common;
mod desktop;
mod font_script;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod os;

pub use common::UI_STACK_RESERVE;
pub(crate) use font_script::SystemFontScript;

/// Windows PE `/STACK` helpers for downstream binaries (`cfg(target_os = "windows")`).
#[cfg(target_os = "windows")]
pub use os::windows;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    options: &WindowOptions,
    composition_swapchain: bool,
) -> WindowAttributes {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::decorate_window_attributes(attrs, options, composition_swapchain)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::decorate_window_attributes(attrs, options, composition_swapchain)
    }
}

pub(crate) fn apply_created_window(
    window: &Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::apply_created_window(window, options, composition_swapchain);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::apply_created_window(window, options, composition_swapchain);
    }
}

pub(crate) fn try_init_custom_glass(window: &Window, options: &WindowOptions) -> bool {
    #[cfg(target_os = "windows")]
    {
        os::windows::glass::try_init_custom(window, options)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, options);
        false
    }
}

#[cfg(all(windows, feature = "backend-dx12"))]
pub(crate) fn custom_glass_surface_handle(window: &Window) -> Option<*mut std::ffi::c_void> {
    os::windows::glass::composition_surface_handle(window)
}

pub(crate) fn glass_detach(window: &Window) {
    #[cfg(target_os = "windows")]
    {
        os::windows::glass::detach_window(window);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

pub(crate) fn glass_on_focus(window: &Window, focused: bool) {
    #[cfg(target_os = "windows")]
    {
        os::windows::glass::on_focus(window, focused);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, focused);
    }
}

pub(crate) fn glass_on_shown(window: &Window) {
    #[cfg(target_os = "windows")]
    {
        os::windows::glass::on_shown(window);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

pub(crate) fn glass_on_moved(window: &Window) {
    #[cfg(target_os = "windows")]
    {
        os::windows::glass::on_moved(window);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
    }
}

pub(crate) fn glass_sync_rui_theme() {
    #[cfg(all(target_os = "windows", feature = "backend-dx12"))]
    {
        os::windows::glass::sync_rui_theme();
        crate::app::runtime::request_redraw_all();
    }
}

pub(crate) fn glass_on_os_theme(os_dark: bool) {
    #[cfg(all(target_os = "windows", feature = "backend-dx12"))]
    {
        os::windows::glass::set_os_dark_all(os_dark);
        crate::app::runtime::request_redraw_all();
    }
    #[cfg(not(all(target_os = "windows", feature = "backend-dx12")))]
    {
        let _ = os_dark;
    }
}

pub(crate) fn process_private_mb() -> f64 {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::process_private_mb()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::process_private_mb()
    }
}

pub(crate) fn system_scroll_lines() -> f32 {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::system_scroll_lines()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::system_scroll_lines()
    }
}

pub(crate) fn apply_ime_cursor_area(
    window: &Window,
    area_x: f32,
    area_y: f32,
    area_w: f32,
    area_h: f32,
    ui_scale: f32,
    scroll_acc: [f32; 2],
    composing: bool,
    field_in_overflow_clip: bool,
) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::apply_ime_cursor_area(
            window,
            area_x,
            area_y,
            area_w,
            area_h,
            ui_scale,
            scroll_acc,
            composing,
            field_in_overflow_clip,
        );
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::apply_ime_cursor_area(
            window,
            area_x,
            area_y,
            area_w,
            area_h,
            ui_scale,
            scroll_acc,
            composing,
            field_in_overflow_clip,
        );
    }
}

pub(crate) fn sync_ime_focus_owner(
    window: &Window,
    focus_id: Option<&str>,
    on_leave_field: impl FnOnce(&str),
) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::sync_focus_owner(window, focus_id, on_leave_field);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::sync_focus_owner(window, focus_id, on_leave_field);
    }
}

pub(crate) fn ime_release_ui(window: &Window, cancel_comp: bool) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::release_ui_control(window, cancel_comp);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::release_ui_control(window, cancel_comp);
    }
}

pub(crate) fn ime_cancel_composition() {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::cancel_active_composition();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::cancel_active_composition();
    }
}

pub(crate) fn ime_end_composition_ui() {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::end_composition_ui();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::end_composition_ui();
    }
}

pub(crate) fn ime_disallow(window: &Window) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::ime_disallow(window);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::ime_disallow(window);
    }
}

pub(crate) fn system_font_dirs() -> Vec<PathBuf> {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::system_font_dirs()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::system_font_dirs()
    }
}

/// First existing `dir.join(file_name)` under [`system_font_dirs`].
pub(crate) fn resolve_system_font(file_name: &str) -> Option<PathBuf> {
    for dir in system_font_dirs() {
        let path = dir.join(file_name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub(crate) fn eager_ui_font_files() -> &'static [&'static str] {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::eager_ui_font_files()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::eager_ui_font_files()
    }
}

pub(crate) fn lazy_script_font_files(script: SystemFontScript) -> &'static [&'static str] {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::lazy_script_font_files(script)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::lazy_script_font_files(script)
    }
}

pub(crate) fn clipboard_set_text(text: &str) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::clipboard_set_text(text);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::clipboard_set_text(text);
    }
}

pub(crate) fn clipboard_get_text() -> Option<String> {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::clipboard_get_text()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::clipboard_get_text()
    }
}

pub(crate) fn edit_shortcut_modifiers(modifiers: &ModifiersState) -> bool {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::edit_shortcut_modifiers(modifiers)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::edit_shortcut_modifiers(modifiers)
    }
}

/// GPU backends to try, in order (ADR-3). `RUI_GPU_BACKEND` moves one to the front.
pub(crate) fn gpu_backend_candidates(
    compositor_alpha: bool,
) -> Vec<(wgpu::Backends, &'static str)> {
    let order = {
        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            os::current::gpu_backend_order(compositor_alpha)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            desktop::gpu_backend_order(compositor_alpha)
        }
    };
    desktop::with_requested_backend_front(order)
}

/// Enable ANSI on Windows consoles (no-op elsewhere). Safe to call repeatedly.
pub(crate) fn enable_ansi_stderr() {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::enable_ansi_stderr();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::enable_ansi_stderr();
    }
}

/// Local wall-clock hour/minute/second, or `None` if the OS clock is unavailable.
pub(crate) fn local_hms() -> Option<(u16, u16, u16)> {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::local_hms()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::local_hms()
    }
}

#[cfg(test)]
mod tests {
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
        let super_key = super::edit_shortcut_modifiers(&ModifiersState::SUPER);
        if cfg!(target_os = "macos") {
            assert!(super_key && !ctrl);
        } else {
            assert!(ctrl && !super_key);
        }
    }

    #[test]
    fn gpu_backend_candidates_match_host() {
        let opaque = super::gpu_backend_candidates(false);
        assert!(!opaque.is_empty());
        if cfg!(target_os = "macos") {
            assert_eq!(opaque.first().map(|(_, n)| *n), Some("metal"));
        } else if cfg!(target_os = "windows") {
            assert_eq!(opaque.first().map(|(_, n)| *n), Some("gl"));
        }

        let compositor = super::gpu_backend_candidates(true);
        if cfg!(target_os = "macos") {
            assert_eq!(compositor.first().map(|(_, n)| *n), Some("metal"));
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
                assert!(
                    !cfg!(any(
                        target_os = "windows",
                        target_os = "macos",
                        target_os = "linux"
                    )),
                    "Win/macOS/Linux must expose a local clock"
                );
            }
        }
    }

    #[test]
    fn enable_ansi_stderr_does_not_panic() {
        super::enable_ansi_stderr();
        super::enable_ansi_stderr();
    }
}
