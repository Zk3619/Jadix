//! Frameless chrome, DWM glass (Acrylic/Mica), and desktop punch-through.

use winit::window::{Window, WindowAttributes};

use crate::app::{WindowBackdrop, WindowOptions};

pub(crate) fn decorate_window_attributes(
    mut attrs: WindowAttributes,
    options: &WindowOptions,
    composition_swapchain: bool,
) -> WindowAttributes {
    use winit::platform::windows::{CornerPreference, WindowAttributesWindows};
    let mut win = WindowAttributesWindows::default();
    if !options.decorations {
        win = win
            .with_undecorated_shadow(true)
            .with_corner_preference(CornerPreference::Round);
    }
    if options.windows_extends_dwm_frame() {
        win = win.with_system_backdrop(backdrop_type(options.backdrop));
    }
    if composition_swapchain {
        win = win.with_no_redirection_bitmap(true);
    }
    attrs.with_platform_attributes(Box::new(win))
}

fn backdrop_type(kind: WindowBackdrop) -> winit::platform::windows::BackdropType {
    use winit::platform::windows::BackdropType;
    match kind {
        WindowBackdrop::Auto => BackdropType::Auto,
        WindowBackdrop::None => BackdropType::None,
        WindowBackdrop::Mica => BackdropType::MainWindow,
        WindowBackdrop::Acrylic | WindowBackdrop::Blur | WindowBackdrop::Aero => {
            BackdropType::TransientWindow
        }
        WindowBackdrop::MicaAlt => BackdropType::TabbedWindow,
    }
}

/// Idempotent: call again after the GPU surface is configured / resized / shown.
///
/// `composition_swapchain` is set when the window presents a premultiplied
/// Vulkan/DXGI swapchain (`WS_EX_NOREDIRECTIONBITMAP`). DWM empty-region
/// blur-behind is then skipped — it targets the redirection bitmap we disabled.
pub(crate) fn apply_created_window(
    window: &dyn Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    use winit::platform::windows::WindowExtWindows;

    let Some(hwnd) = hwnd(window) else {
        return;
    };
    if composition_swapchain || super::glass::has_host(hwnd) {
        return;
    }
    if options.blur {
        set_use_host_backdrop_brush(hwnd, false);
        window.set_system_backdrop(backdrop_type(options.backdrop));
        extend_dwm_frame(hwnd, true);
        if !composition_swapchain {
            enable_dwm_punch_through(hwnd);
        }
        super::glass::apply_accent(hwnd, options);
    } else if options.windows_punch_through_desktop() && !composition_swapchain {
        set_use_host_backdrop_brush(hwnd, true);
        window.set_system_backdrop(winit::platform::windows::BackdropType::Auto);
        extend_dwm_frame(hwnd, false);
        enable_dwm_punch_through(hwnd);
    }
}

pub(crate) fn hwnd(window: &dyn Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return None;
    };
    Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

fn extend_dwm_frame(hwnd: windows_sys::Win32::Foundation::HWND, into_client: bool) {
    use windows_sys::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
    use windows_sys::Win32::UI::Controls::MARGINS;

    let m = if into_client { -1 } else { 0 };
    let margins = MARGINS {
        cxLeftWidth: m,
        cxRightWidth: m,
        cyTopHeight: m,
        cyBottomHeight: m,
    };
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
    }
}

/// Empty blur region: DWM treats the window as glass so unpainted / alpha-0
/// pixels composite over the desktop (or the system backdrop). Same trick
/// winit uses for `with_transparent(true)` — re-applied after WGL/DXGI
/// surface creation, which otherwise drops the region.
fn enable_dwm_punch_through(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::Foundation::TRUE;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND,
    };
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject};

    let region = unsafe { CreateRectRgn(0, 0, -1, -1) };
    let bb = DWM_BLURBEHIND {
        dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
        fEnable: TRUE,
        hRgnBlur: region,
        fTransitionOnMaximized: 0,
    };
    unsafe {
        let _ = DwmEnableBlurBehindWindow(hwnd, &bb);
        let _ = DeleteObject(region);
    }
}

pub(crate) fn set_use_host_backdrop_brush(
    hwnd: windows_sys::Win32::Foundation::HWND,
    enable: bool,
) {
    use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_HOSTBACKDROPBRUSH};

    let value: i32 = if enable { 1 } else { 0 };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH as u32,
            &value as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }
}
