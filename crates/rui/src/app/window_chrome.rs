//! Window chrome: frameless, transparent, and OS backdrop blur.
//!
//! Creation-time options live on [`WindowOptions`] (primary [`super::run_with`], secondary
//! [`open_secondary_window_with`](super::multi_window::open_secondary_window_with)).
//! Runtime helpers ([`start_window_drag`], [`close_window`], …) act on the **active**
//! window (the one whose UI tree is dispatching the event).

use winit::dpi::LogicalSize;
use winit::window::{Window, WindowAttributes};

/// Windows 11 DWM backdrop material (ignored on other platforms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBackdrop {
    /// OS default (usually title-bar only).
    Auto,
    /// No system backdrop.
    None,
    /// Windows 11 Mica (`DWMSBT_MAINWINDOW`).
    Mica,
    /// Windows 11 Acrylic / frosted (`DWMSBT_TRANSIENTWINDOW`).
    #[default]
    Acrylic,
    /// Windows 11 Mica Alt (`DWMSBT_TABBEDWINDOW`).
    MicaAlt,
}

/// Creation-time window attributes (title, size, decorations, compositor alpha).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowOptions {
    pub title: String,
    pub inner_width: Option<f64>,
    pub inner_height: Option<f64>,
    /// OS title bar / border. `false` = frameless (draw your own chrome).
    pub decorations: bool,
    /// Per-pixel alpha is composited with the desktop. Pair with translucent fills.
    pub transparent: bool,
    /// OS backdrop blur (macOS vibrancy, Windows 11 Acrylic/Mica, Wayland kwin).
    /// Implies compositor alpha even if [`Self::transparent`] is false.
    pub blur: bool,
    /// Windows 11 material when [`Self::blur`] is set.
    pub backdrop: WindowBackdrop,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: std::env::var("RUI_WINDOW_TITLE").unwrap_or_else(|_| "RUI Engine".into()),
            inner_width: None,
            inner_height: None,
            decorations: true,
            transparent: false,
            blur: false,
            backdrop: WindowBackdrop::Acrylic,
        }
    }
}

impl WindowOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn size(mut self, width: f64, height: f64) -> Self {
        self.inner_width = Some(width);
        self.inner_height = Some(height);
        self
    }

    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Shorthand for [`Self::decorations`]`(false)`.
    pub fn frameless(self) -> Self {
        self.decorations(false)
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    pub fn backdrop(mut self, backdrop: WindowBackdrop) -> Self {
        self.backdrop = backdrop;
        self
    }

    /// Surface / clear color must expose alpha (transparent **or** blur).
    pub fn compositor_alpha(&self) -> bool {
        self.transparent || self.blur
    }

    /// Win11: extend DWM glass into the client so Acrylic/Mica fills the window.
    pub(crate) fn windows_extends_dwm_frame(&self) -> bool {
        self.blur
    }

    /// Win11: punch clear pixels through to the desktop (no system material).
    pub(crate) fn windows_punch_through_desktop(&self) -> bool {
        self.transparent && !self.blur
    }

    pub fn to_window_attributes(&self) -> WindowAttributes {
        self.window_attributes(self.compositor_alpha())
    }

    /// `composition_swapchain`: Windows `WS_EX_NOREDIRECTIONBITMAP` so a Vulkan/DXGI
    /// premultiplied swapchain can composite over the desktop. Must stay **off** for the
    /// shared GL path — WGL cannot present into a DirectComposition visual.
    pub(crate) fn window_attributes(&self, composition_swapchain: bool) -> WindowAttributes {
        let mut attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_visible(false)
            .with_decorations(self.decorations)
            .with_transparent(self.compositor_alpha())
            .with_blur(self.blur);

        if let (Some(w), Some(h)) = (self.inner_width, self.inner_height) {
            if w > 0.0 && h > 0.0 {
                attrs = attrs.with_inner_size(LogicalSize::new(w, h));
            }
        } else if let (Ok(w), Ok(h)) = (
            std::env::var("RUI_WINDOW_WIDTH"),
            std::env::var("RUI_WINDOW_HEIGHT"),
        ) {
            if let (Ok(w), Ok(h)) = (w.parse::<f64>(), h.parse::<f64>()) {
                if w > 0.0 && h > 0.0 {
                    attrs = attrs.with_inner_size(LogicalSize::new(w, h));
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::{CornerPreference, WindowAttributesExtWindows};
            if !self.decorations {
                attrs = attrs
                    .with_undecorated_shadow(true)
                    .with_corner_preference(CornerPreference::Round);
            }
            if self.windows_extends_dwm_frame() {
                attrs = attrs.with_system_backdrop(backdrop_type(self.backdrop));
            }
            if composition_swapchain {
                attrs = attrs.with_no_redirection_bitmap(true);
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = composition_swapchain;

        attrs
    }
}

#[cfg(target_os = "windows")]
fn backdrop_type(kind: WindowBackdrop) -> winit::platform::windows::BackdropType {
    use winit::platform::windows::BackdropType;
    match kind {
        WindowBackdrop::Auto => BackdropType::Auto,
        WindowBackdrop::None => BackdropType::None,
        WindowBackdrop::Mica => BackdropType::MainWindow,
        WindowBackdrop::Acrylic => BackdropType::TransientWindow,
        WindowBackdrop::MicaAlt => BackdropType::TabbedWindow,
    }
}

/// Platform follow-up after `create_window` (DWM client-area glass, runtime blur).
///
/// Idempotent: call again after the GPU surface is configured / resized / shown.
///
/// `composition_swapchain` is set when the window presents a premultiplied Vulkan/DXGI
/// swapchain (`WS_EX_NOREDIRECTIONBITMAP`). In that case DWM empty-region blur-behind
/// is skipped — it targets the redirection bitmap we just disabled.
pub(crate) fn apply_created_window(
    window: &Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    #[cfg(not(target_os = "windows"))]
    if options.windows_extends_dwm_frame() {
        window.set_blur(true);
    }
    #[cfg(target_os = "windows")]
    apply_windows_glass(window, options, composition_swapchain);
}

#[cfg(target_os = "windows")]
fn apply_windows_glass(
    window: &Window,
    options: &WindowOptions,
    composition_swapchain: bool,
) {
    use winit::platform::windows::WindowExtWindows;

    let Some(hwnd) = win32_hwnd(window) else {
        return;
    };
    if options.windows_extends_dwm_frame() {
        set_use_host_backdrop_brush(hwnd, false);
        window.set_system_backdrop(backdrop_type(options.backdrop));
        extend_dwm_frame(hwnd, true);
        if !composition_swapchain {
            enable_dwm_punch_through(hwnd);
        }
    } else if options.windows_punch_through_desktop() && !composition_swapchain {
        set_use_host_backdrop_brush(hwnd, true);
        window.set_system_backdrop(winit::platform::windows::BackdropType::Auto);
        extend_dwm_frame(hwnd, false);
        enable_dwm_punch_through(hwnd);
    }
}

#[cfg(target_os = "windows")]
fn win32_hwnd(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return None;
    };
    Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

#[cfg(target_os = "windows")]
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

/// Empty blur region: DWM treats the window as glass so unpainted / alpha-0 pixels
/// composite over the desktop (or the system backdrop). Same trick winit uses for
/// `with_transparent(true)` — re-applied after WGL/DXGI surface creation, which
/// otherwise drops the region.
#[cfg(target_os = "windows")]
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

#[cfg(target_os = "windows")]
fn set_use_host_backdrop_brush(hwnd: windows_sys::Win32::Foundation::HWND, enable: bool) {
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

fn with_active_window<R>(f: impl FnOnce(&Window) -> R) -> Option<R> {
    super::runtime::with_active_winit_window(f)
}

/// Begin an OS window move. Call from **mousedown** on a custom title bar (not `click`).
pub fn start_window_drag() {
    let err = with_active_window(|w| w.drag_window().err()).flatten();
    if let Some(e) = err {
        log::debug!("start_window_drag: {e}");
    }
}

pub fn set_window_maximized(maximized: bool) {
    with_active_window(|w| w.set_maximized(maximized));
}

pub fn window_is_maximized() -> bool {
    with_active_window(|w| w.is_maximized()).unwrap_or(false)
}

pub fn toggle_window_maximized() {
    with_active_window(|w| w.set_maximized(!w.is_maximized()));
}

pub fn set_window_minimized(minimized: bool) {
    with_active_window(|w| w.set_minimized(minimized));
}

/// Close the active window. Primary → [`super::request_exit`]; secondary is dropped.
pub fn close_window() {
    match super::window_scope::tree_home_for(super::window_scope::active()) {
        super::window_scope::TreeHome::AppState => {
            super::ui_bridge::request_exit();
        }
        super::window_scope::TreeHome::SecondarySlot => {
            let Some(id) = super::window_scope::active_window_id() else {
                return;
            };
            let Some(proxy) = super::ui_bridge::proxy() else {
                log::warn!("close_window: event loop proxy not installed");
                return;
            };
            if let Err(e) = proxy.send_event(super::RuiUserEvent::CloseWindow { id }) {
                log::warn!("close_window: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_are_decorated_opaque() {
        let o = WindowOptions::new();
        assert!(o.decorations);
        assert!(!o.transparent);
        assert!(!o.blur);
        assert!(!o.compositor_alpha());
        let attrs = o.to_window_attributes();
        assert!(attrs.decorations);
        assert!(!attrs.transparent);
        assert!(!attrs.blur);
    }

    #[test]
    fn frameless_blur_enables_compositor_alpha() {
        let o = WindowOptions::new()
            .title("Glass")
            .size(400.0, 280.0)
            .frameless()
            .blur(true)
            .backdrop(WindowBackdrop::Acrylic);
        assert!(!o.decorations);
        assert!(o.compositor_alpha());
        let attrs = o.to_window_attributes();
        assert!(!attrs.decorations);
        assert!(attrs.transparent);
        assert!(attrs.blur);
        assert_eq!(attrs.title, "Glass");
    }

    #[test]
    fn transparent_without_blur_still_compositor_alpha() {
        let o = WindowOptions::new().transparent(true);
        assert!(o.compositor_alpha());
        assert!(o.to_window_attributes().transparent);
        assert!(!o.to_window_attributes().blur);
        assert!(o.windows_punch_through_desktop());
        assert!(!o.windows_extends_dwm_frame());
    }

    #[test]
    fn blur_extends_dwm_frame_transparent_does_not() {
        let blur = WindowOptions::new().transparent(true).blur(true);
        assert!(blur.windows_extends_dwm_frame());
        assert!(!blur.windows_punch_through_desktop());
        let clear = WindowOptions::new().transparent(true);
        assert!(!clear.windows_extends_dwm_frame());
        assert!(clear.windows_punch_through_desktop());
        let opaque = WindowOptions::new();
        assert!(!opaque.windows_extends_dwm_frame());
        assert!(!opaque.windows_punch_through_desktop());
    }
}
