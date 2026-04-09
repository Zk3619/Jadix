//! 拖拽视觉分层：与列表重排 primitive 配合的 z-index 约定，便于幽灵层 / 抬升一致化。

/// 拖拽时子项相对抬升的 z-index（低于 Portal/Toast，仅用于同级重排高亮）。
pub const DRAG_INDICATOR_Z_BOOST: i32 = 24;

#[inline]
pub const fn drag_indicator_z_boost() -> i32 {
    DRAG_INDICATOR_Z_BOOST
}
