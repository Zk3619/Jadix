//! User-space Windows glass: AccentPolicy fallback + optional DComp recipes.

mod accent;

#[cfg(feature = "windows-dcomp")]
mod dcomp;
#[cfg(feature = "windows-dcomp")]
mod effects;
#[cfg(feature = "windows-dcomp")]
mod recipes;

use crate::app::WindowOptions;
use winit::window::Window;

#[cfg(feature = "windows-dcomp")]
fn is_os_dark(window: &dyn Window) -> bool {
    matches!(window.theme(), Some(winit::window::Theme::Dark))
}

pub fn apply_accent(hwnd: windows_sys::Win32::Foundation::HWND, options: &WindowOptions) {
    accent::apply_accent_policy(hwnd, options);
}

/// Documented system backdrops require Windows 11 22H2 or newer.
pub fn system_backdrop_available() -> bool {
    accent::nt_build_number() >= accent::BUILD_WIN11_SYSTEM_BACKDROP
}

#[cfg(feature = "windows-dcomp")]
pub fn try_init_custom(window: &dyn Window, options: &WindowOptions) -> bool {
    if !(options.wants_custom_dcomp() || options.wants_dcomp_swapchain()) {
        return false;
    }
    let Some(hwnd) = super::window::hwnd(window) else {
        return false;
    };
    if options.blur {
        super::window::set_use_host_backdrop_brush(hwnd, true);
    }
    dcomp::try_init(hwnd as isize, options, is_os_dark(window))
}

#[cfg(not(feature = "windows-dcomp"))]
pub fn try_init_custom(_window: &dyn Window, _options: &WindowOptions) -> bool {
    false
}

#[cfg(feature = "windows-dcomp")]
pub fn has_host(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    dcomp::has_host(hwnd as isize)
}

#[cfg(not(feature = "windows-dcomp"))]
pub fn has_host(_hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    false
}

#[cfg(feature = "windows-dcomp")]
pub fn composition_surface_handle(window: &dyn Window) -> Option<*mut std::ffi::c_void> {
    let hwnd = super::window::hwnd(window)?;
    dcomp::surface_handle(hwnd as isize)
}

pub fn detach_window(window: &dyn Window) {
    #[cfg(feature = "windows-dcomp")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::detach(hwnd as isize);
    }
    let _ = window;
}

pub fn on_focus(window: &dyn Window, focused: bool) {
    #[cfg(feature = "windows-dcomp")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::set_active(hwnd as isize, focused);
    }
    let _ = (window, focused);
}

pub fn on_shown(window: &dyn Window) {
    #[cfg(feature = "windows-dcomp")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::refresh(hwnd as isize);
    }
    let _ = window;
}

pub fn on_moved(window: &dyn Window) {
    #[cfg(feature = "windows-dcomp")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::on_moved(hwnd as isize);
    }
    let _ = window;
}

#[cfg(feature = "windows-dcomp")]
pub fn sync_jadix_theme(window: &dyn Window) {
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::refresh(hwnd as isize);
    }
}

#[cfg(feature = "windows-dcomp")]
pub fn set_os_dark_all(os_dark: bool) {
    dcomp::set_os_dark_all(os_dark);
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
