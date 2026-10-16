//! Jadix: native caption hit regions live with the existing WindowState.
use crate::window_state::{WindowFlags, WindowState};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HTCAPTION, HTCLIENT, HTCLOSE, HTMAXBUTTON, HTMINBUTTON,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptionArea {
    Client,
    Drag,
    Minimize,
    Maximize,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionRegion {
    /// Logical client left, top, width, height. Last containing region wins.
    pub bounds: [f64; 4],
    pub area: CaptionArea,
}

impl CaptionRegion {
    pub fn is_valid(&self) -> bool {
        let [x, y, w, h] = self.bounds;
        self.bounds.iter().all(|v| v.is_finite())
            && x >= 0.0
            && y >= 0.0
            && w > 0.0
            && h > 0.0
            && (x + w).is_finite()
            && (y + h).is_finite()
    }
}

impl WindowState {
    pub(crate) fn caption_hit_test(&self, point: [f64; 2]) -> Option<isize> {
        let flags = self.window_flags;
        if flags.contains(WindowFlags::MARKER_DECORATIONS) || self.fullscreen.is_some() {
            return None;
        }
        let [px, py] = point;
        let region = self.caption_regions.iter().rev().find(|r| {
            let [x, y, w, h] = r.bounds;
            px >= x && py >= y && px < x + w && py < y + h
        })?;
        use CaptionArea::*;
        Some(match region.area {
            Drag => HTCAPTION,
            Maximize if flags.contains(WindowFlags::RESIZABLE | WindowFlags::MAXIMIZABLE) => {
                HTMAXBUTTON
            },
            Minimize if flags.contains(WindowFlags::MINIMIZABLE) => HTMINBUTTON,
            Close if flags.contains(WindowFlags::CLOSABLE) => HTCLOSE,
            _ => HTCLIENT,
        } as isize)
    }
}
