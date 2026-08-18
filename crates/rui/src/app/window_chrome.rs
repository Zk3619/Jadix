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

    pub fn to_window_attributes(&self) -> WindowAttributes {
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
            use winit::platform::windows::{
                BackdropType, CornerPreference, WindowAttributesExtWindows,
            };
            if !self.decorations {
                attrs = attrs
                    .with_undecorated_shadow(true)
                    .with_corner_preference(CornerPreference::Round);
            }
            if self.blur {
                attrs = attrs.with_system_backdrop(backdrop_type(self.backdrop));
            } else if self.transparent {
                attrs = attrs.with_system_backdrop(BackdropType::None);
            }
        }

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
pub(crate) fn apply_created_window(window: &Window, options: &WindowOptions) {
    if options.blur {
        window.set_blur(true);
    }
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowExtWindows;
        if options.blur {
            window.set_system_backdrop(backdrop_type(options.backdrop));
        }
        if options.compositor_alpha() {
            extend_dwm_frame(window);
        }
    }
}

#[cfg(target_os = "windows")]
fn extend_dwm_frame(window: &Window) {
    use windows_sys::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
    use windows_sys::Win32::UI::Controls::MARGINS;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };
    let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
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
    }
}
