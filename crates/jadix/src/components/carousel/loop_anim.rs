//! Carousel seamless ring animation state (product layer).
//!
//! Don't tear off the ring during dragging: you must settle when interrupting, and you can't just throw the flag.

/// Seamless loop: first roll to the first/last clone slot, then teleport to the real item after reaching the position.
#[derive(Clone, Copy, Debug)]
pub struct LoopAnim {
    pub target_offset: f32,
    pub settle_index: usize,
    pub settle_offset: f32,
}

/// Whether the visual offset has reached the loop target (within tolerance).
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
