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

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

mod common;
mod desktop;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod os;

pub use common::UI_STACK_RESERVE;

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
) {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        os::current::apply_ime_cursor_area(
            window, area_x, area_y, area_w, area_h, ui_scale, scroll_acc, composing,
        );
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        desktop::apply_ime_cursor_area(
            window, area_x, area_y, area_w, area_h, ui_scale, scroll_acc, composing,
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
}
