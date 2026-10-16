//! Shared geometry for client-side window chrome. Coordinates are logical pixels.

use crate::error::{NotSupportedError, RequestError};
use crate::window::{ResizeDirection, Window, WindowButtons};

/// Edges attached to another window or screen boundary (top/right/bottom/left).
/// Wayland supplies these through configure events; other backends leave them empty.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TiledEdges {
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    pub left: bool,
}

impl TiledEdges {
    pub fn allows_resize(self, direction: ResizeDirection) -> bool {
        use ResizeDirection::*;
        match direction {
            North => !self.top,
            South => !self.bottom,
            East => !self.right,
            West => !self.left,
            NorthEast => !self.top && !self.right,
            NorthWest => !self.top && !self.left,
            SouthEast => !self.bottom && !self.right,
            SouthWest => !self.bottom && !self.left,
        }
    }

    /// Visual shadow insets. Keep the configured shadow size stable across restore;
    /// only its visual insets disappear on tiled edges.
    pub fn frame_insets(self, shadow: f64) -> [f64; 4] {
        [self.top, self.right, self.bottom, self.left].map(|tiled| if tiled { 0.0 } else { shadow })
    }
}

/// Current backend facts and title-bar policy. Query again when handling an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowChrome {
    /// OS or Winit already supplies a title bar; do not draw duplicate controls.
    pub decorated: bool,
    pub buttons: WindowButtons,
    pub resizable: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub tiled: TiledEdges,
}

/// Native non-client regions. Client masks an interactive child out of a drag
/// region; the last containing region wins. Windows consumes caption actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControlArea {
    Client,
    Drag,
    Minimize,
    Maximize,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowControlRegion {
    /// Logical client left, top, width, height. Re-publish after layout changes.
    pub bounds: [f64; 4],
    pub area: WindowControlArea,
}

impl WindowChrome {
    pub fn can_maximize(self) -> bool {
        self.resizable && !self.fullscreen && self.buttons.contains(WindowButtons::MAXIMIZE)
    }

    pub fn can_resize(self, direction: ResizeDirection) -> bool {
        self.resizable && !self.maximized && !self.fullscreen && self.tiled.allows_resize(direction)
    }
}

/// Jadix additions to Winit's shared Window trait. No second window/core type.
pub trait WindowExtJadix {
    fn chrome_state(&self) -> WindowChrome;
    /// Native title-bar double click. macOS honors AppleActionOnDoubleClick;
    /// other desktop platforms maximize/restore only when permitted.
    fn titlebar_double_click(&self) -> Result<(), RequestError>;
    /// Submit the native system menu. Windows and Wayland only; unsupported
    /// backends and invalid Wayland input serials return an error.
    fn try_show_window_menu(&self, position: crate::dpi::Position) -> Result<(), RequestError>;
    /// Recheck live fixed-size/fullscreen/maximize/tiled-edge policy before dispatch.
    fn start_window_resize(&self, direction: ResizeDirection) -> Result<(), RequestError>;
}

impl WindowExtJadix for dyn Window + '_ {
    fn chrome_state(&self) -> WindowChrome {
        let mut state = WindowChrome {
            decorated: self.is_decorated(),
            buttons: self.enabled_buttons(),
            resizable: self.is_resizable(),
            maximized: self.is_maximized(),
            fullscreen: self.fullscreen().is_some(),
            tiled: TiledEdges::default(),
        };
        #[cfg(wayland_platform)]
        if let Some(window) = self.cast_ref::<winit_wayland::Window>() {
            if let Some(frame) = window.frame_state() {
                state.decorated |= frame.server_decorated;
                state.buttons &= frame.buttons;
                let [top, right, bottom, left] = frame.tiled_edges;
                state.tiled = TiledEdges {
                    top,
                    right,
                    bottom,
                    left,
                };
            } else {
                // Capabilities are unknown until first configure.
                state.buttons = WindowButtons::empty();
            }
        }
        #[cfg(any(ios_platform, android_platform, web_platform, orbital_platform))]
        {
            state.buttons = WindowButtons::empty();
        }
        // Keep this mutable on targets without compositor capability overrides.
        let _ = &mut state;
        state
    }

    fn titlebar_double_click(&self) -> Result<(), RequestError> {
        #[cfg(macos_platform)]
        {
            crate::platform::macos::titlebar_double_click(self)
        }
        #[cfg(not(macos_platform))]
        {
            let state = self.chrome_state();
            if !state.can_maximize() {
                return Err(
                    NotSupportedError::new("window cannot be maximized by its title bar").into(),
                );
            }
            self.set_maximized(!state.maximized);
            Ok(())
        }
    }

    fn try_show_window_menu(&self, position: crate::dpi::Position) -> Result<(), RequestError> {
        let p = position.to_logical::<f64>(self.scale_factor());
        if ![p.x, p.y]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= i32::MAX as f64)
        {
            return Err(NotSupportedError::new("window menu position is out of range").into());
        }
        #[cfg(windows_platform)]
        {
            self.show_window_menu(position);
            return Ok(());
        }
        #[cfg(wayland_platform)]
        if let Some(window) = self.cast_ref::<winit_wayland::Window>() {
            return window.try_show_window_menu(position);
        }
        #[allow(unreachable_code)]
        Err(NotSupportedError::new("native system window menu requires Windows or Wayland").into())
    }

    fn start_window_resize(&self, direction: ResizeDirection) -> Result<(), RequestError> {
        if !self.chrome_state().can_resize(direction) {
            return Err(NotSupportedError::new("window policy prevents border resize").into());
        }
        self.drag_resize_window(direction)
    }
}

/// Hit-test a narrow band around the visible frame, excluding the outer shadow.
/// Insets are top/right/bottom/left in logical pixels. Tiled corners require both
/// adjacent edges to be free. Out-of-surface, invalid or empty geometry has no hit.
pub fn resize_direction_at_frame(
    size: [f64; 2],
    point: [f64; 2],
    insets: [f64; 4],
    tiled: TiledEdges,
    band: f64,
) -> Option<ResizeDirection> {
    let [w, h] = size;
    let [x, y] = point;
    if !size
        .into_iter()
        .chain(point)
        .chain(insets)
        .chain([band])
        .all(f64::is_finite)
        || w <= 0.0
        || h <= 0.0
        || band <= 0.0
        || insets.iter().any(|v| *v < 0.0)
        || x < 0.0
        || y < 0.0
        || x >= w
        || y >= h
    {
        return None;
    }
    let [top, right, bottom, left] = insets;
    let right = w - right;
    let bottom = h - bottom;
    if left >= right || top >= bottom {
        return None;
    }
    let on_left = (x - left).abs() <= band && y >= top - band && y <= bottom + band;
    let on_right = (x - right).abs() <= band && y >= top - band && y <= bottom + band;
    let on_top = (y - top).abs() <= band && x >= left - band && x <= right + band;
    let on_bottom = (y - bottom).abs() <= band && x >= left - band && x <= right + band;
    use ResizeDirection::*;
    [
        (NorthWest, on_top && on_left),
        (NorthEast, on_top && on_right),
        (SouthWest, on_bottom && on_left),
        (SouthEast, on_bottom && on_right),
        (North, on_top),
        (South, on_bottom),
        (West, on_left),
        (East, on_right),
    ]
    .into_iter()
    .find(|(d, hit)| *hit && tiled.allows_resize(*d))
    .map(|(d, _)| d)
}

/// Hit-test an inside resize band in logical client coordinates. Corners win.
pub fn resize_direction_at(
    size: [f64; 2],
    point: [f64; 2],
    inset: f64,
) -> Option<crate::window::ResizeDirection> {
    use crate::window::ResizeDirection::*;
    let [width, height] = size;
    let [x, y] = point;
    if ![width, height, x, y, inset].iter().all(|n| n.is_finite())
        || width <= 0.0
        || height <= 0.0
        || inset <= 0.0
        || x < 0.0
        || y < 0.0
        || x >= width
        || y >= height
    {
        return None;
    }
    let horizontal = inset.min(width / 2.0);
    let vertical = inset.min(height / 2.0);
    let left = x < horizontal;
    let right = x >= width - horizontal;
    let top = y < vertical;
    let bottom = y >= height - vertical;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(NorthWest),
        (_, true, true, _) => Some(NorthEast),
        (true, _, _, true) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, _, _, _) => Some(West),
        (_, true, _, _) => Some(East),
        (_, _, true, _) => Some(North),
        (_, _, _, true) => Some(South),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frameless() -> WindowChrome {
        WindowChrome {
            decorated: false,
            buttons: WindowButtons::all(),
            resizable: true,
            maximized: false,
            fullscreen: false,
            tiled: TiledEdges::default(),
        }
    }

    #[test]
    fn shadow_is_not_a_resize_extension_line_and_tiled_edges_stay_attached() {
        use ResizeDirection::*;
        let size = [300.0, 200.0];
        let tiled = TiledEdges::default();
        let insets = tiled.frame_insets(20.0);
        for (point, direction) in [
            ([20.0, 20.0], NorthWest),
            ([150.0, 20.0], North),
            ([280.0, 20.0], NorthEast),
            ([20.0, 100.0], West),
            ([280.0, 100.0], East),
            ([20.0, 180.0], SouthWest),
            ([150.0, 180.0], South),
            ([280.0, 180.0], SouthEast),
        ] {
            assert_eq!(
                resize_direction_at_frame(size, point, insets, tiled, 4.0),
                Some(direction)
            );
        }
        for point in [
            [1.0, 20.0],
            [20.0, 1.0],
            [1.0, 1.0],
            [150.0, 100.0],
            [300.0, 20.0],
            [f64::NAN, 20.0],
        ] {
            assert_eq!(
                resize_direction_at_frame(size, point, insets, tiled, 4.0),
                None
            );
        }
        let tiled = TiledEdges {
            top: true,
            left: true,
            ..tiled
        };
        assert_eq!(tiled.frame_insets(20.0), [0.0, 20.0, 20.0, 0.0]);
        assert_eq!(
            resize_direction_at_frame(size, [0.0, 0.0], tiled.frame_insets(20.0), tiled, 4.0),
            None
        );
        assert_eq!(
            resize_direction_at_frame(size, [280.0, 0.0], tiled.frame_insets(20.0), tiled, 4.0),
            Some(East)
        );
        assert_eq!(
            resize_direction_at_frame(size, [1.0, 1.0], [160.0; 4], tiled, 4.0),
            None
        );
        assert_eq!(
            resize_direction_at_frame(size, [1.0, 1.0], [0.0; 4], tiled, f64::INFINITY),
            None
        );
    }

    #[test]
    fn every_tiling_combination_and_live_resize_policy_is_checked() {
        use ResizeDirection::*;
        for bits in 0..16 {
            let tiled = TiledEdges {
                top: bits & 1 != 0,
                right: bits & 2 != 0,
                bottom: bits & 4 != 0,
                left: bits & 8 != 0,
            };
            let mut state = WindowChrome {
                tiled,
                ..frameless()
            };
            for (direction, mask) in [
                (North, 1),
                (East, 2),
                (South, 4),
                (West, 8),
                (NorthEast, 3),
                (SouthEast, 6),
                (SouthWest, 12),
                (NorthWest, 9),
            ] {
                assert_eq!(state.can_resize(direction), bits & mask == 0);
            }
            for mode in 0..3 {
                state = WindowChrome {
                    tiled,
                    ..frameless()
                };
                match mode {
                    0 => state.resizable = false,
                    1 => state.fullscreen = true,
                    _ => state.maximized = true,
                }
                for direction in [
                    North, East, South, West, NorthEast, SouthEast, SouthWest, NorthWest,
                ] {
                    assert!(!state.can_resize(direction));
                }
            }
        }
    }

    #[test]
    fn resize_hit_test_covers_eight_edges_and_rejects_invalid_geometry() {
        use crate::window::ResizeDirection::*;
        for (point, direction) in [
            ([1.0, 1.0], NorthWest),
            ([50.0, 1.0], North),
            ([99.0, 1.0], NorthEast),
            ([1.0, 40.0], West),
            ([99.0, 40.0], East),
            ([1.0, 79.0], SouthWest),
            ([50.0, 79.0], South),
            ([99.0, 79.0], SouthEast),
        ] {
            assert_eq!(
                resize_direction_at([100.0, 80.0], point, 5.0),
                Some(direction)
            );
        }
        for point in [
            [50.0, 40.0],
            [-1.0, 5.0],
            [100.0, 5.0],
            [1.0, 80.0],
            [f64::NAN, 0.0],
        ] {
            assert_eq!(resize_direction_at([100.0, 80.0], point, 5.0), None);
        }
        assert_eq!(resize_direction_at([0.0, 80.0], [0.0, 1.0], 5.0), None);
        assert_eq!(
            resize_direction_at([100.0, 80.0], [0.0, 1.0], f64::INFINITY),
            None
        );
        assert_eq!(resize_direction_at([100.0, 80.0], [0.0, 1.0], 0.0), None);
        assert_eq!(
            resize_direction_at([2.0, 2.0], [1.0, 1.0], 5.0),
            Some(SouthEast)
        );
        // Fractional logical coordinates: no rounding to physical integer pixels.
        assert_eq!(
            resize_direction_at([100.5, 80.5], [99.75, 40.0], 1.0),
            Some(East)
        );
    }
}
