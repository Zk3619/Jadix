//! User-space Windows glass: AccentPolicy fallback + optional DComp recipes.

mod accent;

#[cfg(feature = "backend-dx12")]
mod dcomp;
#[cfg(feature = "backend-dx12")]
mod effects;
#[cfg(feature = "backend-dx12")]
mod recipes;

use crate::app::WindowOptions;
use winit::window::Window;

#[cfg(feature = "backend-dx12")]
fn is_os_dark(window: &dyn Window) -> bool {
    matches!(window.theme(), Some(winit::window::Theme::Dark))
}

pub fn apply_accent(hwnd: windows_sys::Win32::Foundation::HWND, options: &WindowOptions) {
    accent::apply_accent_policy(hwnd, options);
}

#[cfg(feature = "backend-dx12")]
pub fn try_init_custom(window: &dyn Window, options: &WindowOptions) -> bool {
    if !options.compositor_alpha() {
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

#[cfg(not(feature = "backend-dx12"))]
pub fn try_init_custom(_window: &dyn Window, _options: &WindowOptions) -> bool {
    false
}

#[cfg(feature = "backend-dx12")]
pub fn has_host(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    dcomp::has_host(hwnd as isize)
}

#[cfg(not(feature = "backend-dx12"))]
pub fn has_host(_hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    false
}

#[cfg(feature = "backend-dx12")]
pub fn composition_surface_handle(window: &dyn Window) -> Option<*mut std::ffi::c_void> {
    let hwnd = super::window::hwnd(window)?;
    dcomp::surface_handle(hwnd as isize)
}

pub fn detach_window(window: &dyn Window) {
    #[cfg(feature = "backend-dx12")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::detach(hwnd as isize);
    }
    let _ = window;
}

pub fn on_focus(window: &dyn Window, focused: bool) {
    #[cfg(feature = "backend-dx12")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::set_active(hwnd as isize, focused);
    }
    let _ = (window, focused);
}

pub fn on_shown(window: &dyn Window) {
    #[cfg(feature = "backend-dx12")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::refresh(hwnd as isize);
    }
    let _ = window;
}

pub fn on_moved(window: &dyn Window) {
    #[cfg(feature = "backend-dx12")]
    if let Some(hwnd) = super::window::hwnd(window) {
        dcomp::on_moved(hwnd as isize);
    }
    let _ = window;
}

#[cfg(feature = "backend-dx12")]
pub fn sync_jadix_theme() {
    dcomp::sync_jadix_theme();
}

#[cfg(feature = "backend-dx12")]
pub fn set_os_dark_all(os_dark: bool) {
    dcomp::set_os_dark_all(os_dark);
}

#[cfg(test)]
mod tests {
    #[test]
    fn accent_module_loads() {
        assert!(super::accent::nt_build_number() > 0);
    }
}
