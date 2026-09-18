use serde::{Deserialize, Serialize};

use crate::flags::StateFlags;
use crate::view::{MonitorRect, WindowView};

/// Per-window persisted geometry. Physical pixels (same as tauri-plugin-window-state).
///
/// `prev_x` / `prev_y` keep the last non-maximized origin: maximize snaps `x`/`y` to the
/// monitor corner, so restore must use the previous pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub prev_x: i32,
    #[serde(default)]
    pub prev_y: i32,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub decorated: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

fn default_true() -> bool {
    true
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            prev_x: 0,
            prev_y: 0,
            maximized: false,
            visible: true,
            decorated: true,
            fullscreen: false,
        }
    }
}

impl WindowState {
    /// Never-captured placeholder (skip restore so the OS places the first launch).
    pub fn is_placeholder(&self) -> bool {
        self == &Self::default()
    }

    pub fn note_moved(&mut self, x: i32, y: i32) {
        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;
    }

    pub fn note_resized(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.width = width;
            self.height = height;
        }
    }

    /// Snapshot live window fields selected by `flags`.
    ///
    /// Size / position are skipped while maximized or minimized so the restored
    /// "normal" bounds stay intact.
    pub fn capture_from(&mut self, window: &dyn WindowView, flags: StateFlags) {
        let maximized = flags
            .intersects(StateFlags::MAXIMIZED | StateFlags::POSITION | StateFlags::SIZE)
            && window.is_maximized();
        let minimized =
            flags.intersects(StateFlags::POSITION | StateFlags::SIZE) && window.is_minimized();

        if flags.contains(StateFlags::MAXIMIZED) {
            self.maximized = maximized;
        }
        if flags.contains(StateFlags::FULLSCREEN) {
            self.fullscreen = window.is_fullscreen();
        }
        if flags.contains(StateFlags::DECORATIONS) {
            self.decorated = window.is_decorated();
        }
        if flags.contains(StateFlags::VISIBLE) {
            self.visible = window.is_visible();
        }
        if flags.contains(StateFlags::SIZE) && !maximized && !minimized {
            let (width, height) = window.inner_size();
            self.note_resized(width, height);
        }
        if flags.contains(StateFlags::POSITION) && !maximized && !minimized {
            if let Some((x, y)) = window.outer_position() {
                self.x = x;
                self.y = y;
            }
        }
    }

    /// Apply saved fields. Position is ignored when no saved monitor still intersects.
    pub fn apply_to(&self, window: &dyn WindowView, flags: StateFlags) {
        if self.is_placeholder() {
            return;
        }

        if flags.contains(StateFlags::DECORATIONS) {
            window.set_decorations(self.decorated);
        }

        if flags.contains(StateFlags::POSITION) {
            let (x, y) = if self.maximized {
                (self.prev_x, self.prev_y)
            } else {
                (self.x, self.y)
            };
            let size = (self.width, self.height);
            let pos = (x, y);
            if window
                .available_monitors()
                .iter()
                .any(|m| m.intersects(pos, size))
            {
                window.set_outer_position(x, y);
            }
        }

        if flags.contains(StateFlags::SIZE) && self.width > 0 && self.height > 0 {
            window.set_inner_size(self.width, self.height);
        }

        if flags.contains(StateFlags::MAXIMIZED) && self.maximized {
            window.set_maximized(true);
        }

        if flags.contains(StateFlags::FULLSCREEN) {
            window.set_fullscreen(self.fullscreen);
        }

        if flags.contains(StateFlags::VISIBLE) && self.visible {
            window.set_visible(true);
            window.focus();
        }
    }
}

impl MonitorRect {
    pub fn intersects(self, position: (i32, i32), size: (u32, u32)) -> bool {
        let left = self.x;
        let right = self.x.saturating_add(self.width as i32);
        let top = self.y;
        let bottom = self.y.saturating_add(self.height as i32);
        let (x, y) = position;
        let (w, h) = size;
        [
            (x, y),
            (x.saturating_add(w as i32), y),
            (x, y.saturating_add(h as i32)),
            (x.saturating_add(w as i32), y.saturating_add(h as i32)),
        ]
        .into_iter()
        .any(|(cx, cy)| cx >= left && cx < right && cy >= top && cy < bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_default() {
        assert!(WindowState::default().is_placeholder());
        let mut s = WindowState::default();
        s.width = 800;
        s.height = 600;
        assert!(!s.is_placeholder());
    }

    #[test]
    fn moved_keeps_previous_origin() {
        let mut s = WindowState::default();
        s.note_moved(10, 20);
        s.note_moved(0, 0);
        assert_eq!((s.prev_x, s.prev_y), (10, 20));
        assert_eq!((s.x, s.y), (0, 0));
    }

    #[test]
    fn monitor_intersect_any_corner() {
        let m = MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert!(m.intersects((1900, 10), (40, 40)));
        assert!(!m.intersects((2000, 10), (40, 40)));
        let right = MonitorRect {
            x: 1920,
            y: 0,
            width: 1080,
            height: 1920,
        };
        assert!(right.intersects((1930, 10), (100, 100)));
        assert!(!right.intersects((100, 10), (100, 100)));
    }
}
