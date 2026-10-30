//! Jadix additions: expose compositor configure facts without leaking protocol handles.
use crate::winit_core::window::WindowButtons;
use sctk::reexports::csd_frame::WindowManagerCapabilities;
use sctk::shell::xdg::window::{DecorationMode, WindowConfigure};

use super::{Window, state::WindowType};

/// Last received compositor configuration; absent before first configure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFrameState {
    pub server_decorated: bool,
    /// Top, right, bottom, left. These are compositor facts, not geometry guesses.
    pub tiled_edges: [bool; 4],
    pub buttons: WindowButtons,
    pub window_menu: bool,
}

impl From<&WindowConfigure> for WindowFrameState {
    fn from(configure: &WindowConfigure) -> Self {
        let caps = configure.capabilities;
        let mut buttons = WindowButtons::CLOSE;
        buttons.set(
            WindowButtons::MINIMIZE,
            caps.contains(WindowManagerCapabilities::MINIMIZE),
        );
        buttons.set(
            WindowButtons::MAXIMIZE,
            caps.contains(WindowManagerCapabilities::MAXIMIZE),
        );
        Self {
            server_decorated: configure.decoration_mode == DecorationMode::Server,
            tiled_edges: [
                configure.is_tiled_top(),
                configure.is_tiled_right(),
                configure.is_tiled_bottom(),
                configure.is_tiled_left(),
            ],
            buttons,
            window_menu: caps.contains(WindowManagerCapabilities::WINDOW_MENU),
        }
    }
}

impl Window {
    pub fn frame_state(&self) -> Option<WindowFrameState> {
        let state = self.window_state.lock().unwrap();
        match &state.window {
            WindowType::Window { last_configure, .. } => last_configure.as_ref().map(Into::into),
            WindowType::Popup { .. } => None,
        }
    }

    /// Submit xdg_toplevel.show_window_menu only with a valid input seat/serial.
    /// Success means submitted; the compositor decides whether to display it.
    pub fn try_show_window_menu(
        &self,
        position: dpi::Position,
    ) -> Result<(), crate::winit_core::error::RequestError> {
        let state = self.window_state.lock().unwrap();
        let position = position.to_logical::<f64>(state.scale_factor());
        if ![position.x, position.y]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= i32::MAX as f64)
        {
            return Err(crate::winit_core::error::NotSupportedError::new(
                "window menu position is out of range",
            )
            .into());
        }
        state.try_show_window_menu(position.cast())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sctk::reexports::csd_frame::WindowState;

    #[test]
    fn projects_real_configure_capabilities_and_each_tiled_edge() {
        let mut configure = WindowConfigure::default();
        configure.decoration_mode = DecorationMode::Server;
        configure.capabilities = WindowManagerCapabilities::MINIMIZE;
        configure.state = WindowState::TILED_TOP | WindowState::TILED_LEFT;
        let frame = WindowFrameState::from(&configure);
        assert!(frame.server_decorated);
        assert_eq!(frame.tiled_edges, [true, false, false, true]);
        assert_eq!(
            frame.buttons,
            WindowButtons::CLOSE | WindowButtons::MINIMIZE
        );
        assert!(!frame.window_menu);
        configure.capabilities =
            WindowManagerCapabilities::MAXIMIZE | WindowManagerCapabilities::WINDOW_MENU;
        configure.state = WindowState::TILED_BOTTOM | WindowState::TILED_RIGHT;
        let frame = WindowFrameState::from(&configure);
        assert_eq!(frame.tiled_edges, [false, true, true, false]);
        assert_eq!(
            frame.buttons,
            WindowButtons::CLOSE | WindowButtons::MAXIMIZE
        );
        assert!(frame.window_menu);
    }
}
