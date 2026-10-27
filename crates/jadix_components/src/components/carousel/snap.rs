//! Carousel snap / nearest-index (product layer, shipped with carousel directory).

/// The nearest index is derived from the current orbit offset.
pub fn index_from_offset(
    off: f32,
    step: f32,
    clone_count: usize,
    count: usize,
    max_index: usize,
    wrap_loop: bool,
) -> usize {
    let n = count.max(1);
    let hi = max_index;
    if step <= 1.0 {
        return 0;
    }
    let track_i = (-off / step).round() as i32;
    let logical = track_i - clone_count as i32;
    if wrap_loop {
        let n_i = n as i32;
        let wrapped = ((logical % n_i) + n_i) % n_i;
        (wrapped as usize).min(hi)
    } else {
        logical.clamp(0, hi as i32) as usize
    }
}

/// After letting go, snap to the grid point: return to the target offset (negative direction).
pub fn snap_target_for_index(index: usize, step: f32, clone_count: usize) -> f32 {
    -((index + clone_count) as f32) * step
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn nearest_index_and_snap_target() {
        assert_eq!(index_from_offset(-200.0, 100.0, 0, 5, 4, false), 2);
        assert!((snap_target_for_index(2, 100.0, 0) - (-200.0)).abs() < 0.01);
        // clones=2, off=-200 → track_i=2 → logical=0
        assert_eq!(index_from_offset(-200.0, 100.0, 2, 5, 4, true), 0);
    }
}
