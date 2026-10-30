//! Win32 extensions plus native caption hit regions (including Snap hover).
use crate::error::RequestError;
use crate::window::Window as CoreWindow;
pub use crate::winit_win32::*;
use std::sync::Arc;

/// Publish logical caption rectangles after layout. The backend owns their
/// lifetime and checks current native flags during WM_NCHITTEST. Empty clears.
/// Safe from any thread, like other Winit setters. Windows owns caption actions;
/// do not also activate the same rectangles in client pointer handlers.
/// HTMAXBUTTON enables the Snap integration point; flyout visibility is OS policy.
pub fn set_window_control_regions(
    window: &Arc<dyn CoreWindow>,
    regions: Vec<crate::chrome::WindowControlRegion>,
) -> Result<(), RequestError> {
    let regions = regions
        .into_iter()
        .map(|r| {
            use crate::chrome::WindowControlArea as A;
            let area = match r.area {
                A::Client => CaptionArea::Client,
                A::Drag => CaptionArea::Drag,
                A::Minimize => CaptionArea::Minimize,
                A::Maximize => CaptionArea::Maximize,
                A::Close => CaptionArea::Close,
            };
            CaptionRegion {
                bounds: r.bounds,
                area,
            }
        })
        .collect();
    window.set_caption_regions(regions)
}

#[cfg(test)]
#[path = "tests/windows.rs"]
mod tests;
