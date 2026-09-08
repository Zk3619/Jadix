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
