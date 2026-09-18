//! Cross-backend window snapshot / apply surface.
//!
//! `jadix` core does not persist windows; this trait is the injection point
//! (same shape as `IconProvider`). Tests use a mock; `winit::window::Window`
//! implements it behind the `winit` feature.

/// Physical monitor rectangle (origin + size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub trait WindowView {
    fn inner_size(&self) -> (u32, u32);
    fn outer_position(&self) -> Option<(i32, i32)>;
    fn is_maximized(&self) -> bool;
    fn is_minimized(&self) -> bool;
    fn is_visible(&self) -> bool;
    fn is_decorated(&self) -> bool;
    fn is_fullscreen(&self) -> bool;
    fn available_monitors(&self) -> Vec<MonitorRect>;

    fn set_outer_position(&self, x: i32, y: i32);
    fn set_inner_size(&self, width: u32, height: u32);
    fn set_maximized(&self, maximized: bool);
    fn set_fullscreen(&self, fullscreen: bool);
    fn set_decorations(&self, decorated: bool);
    fn set_visible(&self, visible: bool);
    fn focus(&self);
}

#[cfg(feature = "winit")]
impl WindowView for winit::window::Window {
    fn inner_size(&self) -> (u32, u32) {
        let s = winit::window::Window::inner_size(self);
        (s.width, s.height)
    }

    fn outer_position(&self) -> Option<(i32, i32)> {
        winit::window::Window::outer_position(self)
            .ok()
            .map(|p| (p.x, p.y))
    }

    fn is_maximized(&self) -> bool {
        winit::window::Window::is_maximized(self)
    }

    fn is_minimized(&self) -> bool {
        winit::window::Window::is_minimized(self).unwrap_or(false)
    }

    fn is_visible(&self) -> bool {
        winit::window::Window::is_visible(self).unwrap_or(true)
    }

    fn is_decorated(&self) -> bool {
        winit::window::Window::is_decorated(self)
    }

    fn is_fullscreen(&self) -> bool {
        winit::window::Window::fullscreen(self).is_some()
    }

    fn available_monitors(&self) -> Vec<MonitorRect> {
        winit::window::Window::available_monitors(self)
            .map(|m| {
                let p = m.position();
                let s = m.size();
                MonitorRect {
                    x: p.x,
                    y: p.y,
                    width: s.width,
                    height: s.height,
                }
            })
            .collect()
    }

    fn set_outer_position(&self, x: i32, y: i32) {
        winit::window::Window::set_outer_position(self, winit::dpi::PhysicalPosition::new(x, y));
    }

    fn set_inner_size(&self, width: u32, height: u32) {
        let _ = winit::window::Window::request_inner_size(
            self,
            winit::dpi::PhysicalSize::new(width, height),
        );
    }

    fn set_maximized(&self, maximized: bool) {
        winit::window::Window::set_maximized(self, maximized);
    }

    fn set_fullscreen(&self, fullscreen: bool) {
        winit::window::Window::set_fullscreen(
            self,
            fullscreen.then_some(winit::window::Fullscreen::Borderless(None)),
        );
    }

    fn set_decorations(&self, decorated: bool) {
        winit::window::Window::set_decorations(self, decorated);
    }

    fn set_visible(&self, visible: bool) {
        winit::window::Window::set_visible(self, visible);
    }

    fn focus(&self) {
        winit::window::Window::focus_window(self);
    }
}
