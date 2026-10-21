//! Author-facing modal surface kind + logical viewport.
//!
//! Leaf module: no engine import. Fade/slide implementation is
//! [`crate::utils::modal_motion`] `engine`; authors should use [`crate::utils::viewport_modal`].

use std::cell::RefCell;

/// 面板运动曲线（Dialog 淡入缩放；Sheet/Drawer 贴边滑入）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalSurfaceMotion {
    DialogFade,
    SlideFromTop,
    SlideFromBottom,
    SlideFromLeft,
    SlideFromRight,
}

impl ModalSurfaceMotion {
    pub fn from_attr(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "slide-top" | "sheet-top" | "top" => Self::SlideFromTop,
            "slide-bottom" | "sheet-bottom" | "bottom" => Self::SlideFromBottom,
            "slide-left" | "sheet-left" | "left" => Self::SlideFromLeft,
            "slide-right" | "sheet-right" | "right" | "sheet-slide" => Self::SlideFromRight,
            _ => Self::DialogFade,
        }
    }
}

thread_local! {
    /// Logical viewport written each frame by the window loop (slide travel).
    static MODAL_VIEWPORT: RefCell<(f32, f32)> = const { RefCell::new((800.0, 600.0)) };
}

/// 由窗口事件循环每帧写入逻辑视口（Sheet / Drawer / ViewportModal 滑行距离）。
pub fn set_modal_viewport_size(w: f32, h: f32) {
    MODAL_VIEWPORT.with(|v| *v.borrow_mut() = (w.max(1.0), h.max(1.0)));
}

pub fn modal_viewport_size() -> (f32, f32) {
    #[cfg(all(debug_assertions, feature = "devtools"))]
    if let Some(size) = crate::app::devtools::preview_viewport_size() { return size; }
    MODAL_VIEWPORT.with(|v| *v.borrow())
}

/// Pre-layout fallback: Sheet/Drawer pass viewport; `0` collapses travel to 1px
/// (instant open, overlay fades while the panel sits until ExitGate).
pub fn default_slide_axis_px(motion: ModalSurfaceMotion) -> f32 {
    let (vw, vh) = modal_viewport_size();
    match motion {
        ModalSurfaceMotion::DialogFade => 0.0,
        ModalSurfaceMotion::SlideFromTop | ModalSurfaceMotion::SlideFromBottom => vh,
        ModalSurfaceMotion::SlideFromLeft | ModalSurfaceMotion::SlideFromRight => vw,
    }
}
