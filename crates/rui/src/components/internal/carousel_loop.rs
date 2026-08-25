//! Carousel seamless 环动画状态（WP-SI.10）。
//!
//! 拖拽中途不得撕环：打断时须 settle，不能只丢 flag。

/// 无缝循环：先滚到首/尾克隆槽，到位后再瞬移到真实项。
#[derive(Clone, Copy, Debug)]
pub struct LoopAnim {
    pub target_offset: f32,
    pub settle_index: usize,
    pub settle_offset: f32,
}

/// 视觉 offset 是否已到达 loop 目标（容差内）。
pub fn loop_reached(visual: f32, target: f32, epsilon: f32) -> bool {
    (visual - target).abs() <= epsilon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_reached_epsilon() {
        let a = LoopAnim {
            target_offset: -100.0,
            settle_index: 0,
            settle_offset: 0.0,
        };
        assert!(loop_reached(-100.2, a.target_offset, 0.5));
        assert!(!loop_reached(-110.0, a.target_offset, 0.5));
    }
}
