//! OS-specific hooks.
//!
//! Cross-platform window *options* stay on [`crate::app::WindowOptions`].
//! Win32 DWM glass and process metrics live under [`windows`].
//! macOS / Linux currently only apply winit `set_blur`.
//!
//! The framework does not emit PE `/STACK` or require a large main-thread stack.
//! Deep UI trees are walked with heap work queues instead of unbounded recursion.

use winit::window::{Window, WindowAttributes};

use crate::app::WindowOptions;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

/// Optional stack when an app **explicitly** spawns a worker that builds a huge
/// UI tree (e.g. gallery's all-sections test). Not applied to the process main
/// thread; not a PE `/STACK` reserve.
pub const UI_STACK_RESERVE: usize = 8 * 1024 * 1024;

pub(crate) fn decorate_window_attributes(
    attrs: WindowAttributes,
    options: &WindowOptions,
    composition_swapchain: bool,
) -> WindowAttributes {
    #[cfg(target_os = "windows")]
    {
        windows::decorate_window_attributes(attrs, options, composition_swapchain)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (options, composition_swapchain);
        attrs
    }
}

pub(crate) fn apply_created_window(
    window: &Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    #[cfg(target_os = "windows")]
    windows::apply_created_window(window, options, composition_swapchain);
    #[cfg(target_os = "macos")]
    macos::apply_created_window(window, options, composition_swapchain);
    #[cfg(target_os = "linux")]
    linux::apply_created_window(window, options, composition_swapchain);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = composition_swapchain;
        if options.blur {
            window.set_blur(true);
        }
    }
}

pub(crate) fn process_private_mb() -> f64 {
    #[cfg(target_os = "windows")]
    {
        windows::process_private_mb()
    }
    #[cfg(not(target_os = "windows"))]
    {
        0.0
    }
}

pub(crate) fn system_scroll_lines() -> f32 {
    #[cfg(target_os = "windows")]
    {
        windows::system_scroll_lines()
    }
    #[cfg(not(target_os = "windows"))]
    {
        3.0
    }
}
