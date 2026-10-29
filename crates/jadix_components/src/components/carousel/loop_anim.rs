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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/loop_anim.rs"]
mod tests;
