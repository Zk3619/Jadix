//! Toast 滑动关闭策略（产品层）。会话壳走 `utils/drag/pointer`；阈值/轴向在此。

/// 任一侧向超过该阈值则退场。
pub const SWIPE_DISMISS_PX: f32 = 56.0;

/// 拖拽关闭允许的轴向（四角可沿靠近的两边；中上/中下仅垂直反向）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToastSwipeDirs {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl ToastSwipeDirs {
    pub fn for_edge(top: bool, bottom: bool, left: bool, right: bool) -> Self {
        Self {
            up: top,
            down: bottom,
            left,
            right,
        }
    }
}

pub fn clamp_swipe_offset(dx: f32, dy: f32, dirs: ToastSwipeDirs) -> [f32; 2] {
    let ox = if (dirs.left && dx < 0.0) || (dirs.right && dx > 0.0) {
        dx
    } else {
        0.0
    };
    let oy = if (dirs.up && dy < 0.0) || (dirs.down && dy > 0.0) {
        dy
    } else {
        0.0
    };
    [ox, oy]
}

pub fn should_dismiss(offset: [f32; 2]) -> bool {
    offset[0].abs() >= SWIPE_DISMISS_PX || offset[1].abs() >= SWIPE_DISMISS_PX
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/swipe.rs"]
mod tests;
