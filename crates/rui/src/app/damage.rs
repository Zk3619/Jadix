//! Dirty-region helpers for partial redraw (logical pixel coordinates).

/// Union two axis-aligned rects `[x, y, w, h]`.
pub fn union_rects(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let x2 = ax2.max(bx2);
    let y2 = ay2.max(by2);
    [x, y, x2 - x, y2 - y]
}

#[cfg(test)]
mod tests {
    use super::union_rects;

    #[test]
    fn union_disjoint_rects() {
        let u = union_rects([0.0, 0.0, 10.0, 10.0], [20.0, 5.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn union_overlapping_rects() {
        let u = union_rects([0.0, 0.0, 20.0, 20.0], [10.0, 10.0, 20.0, 20.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 30.0]);
    }

    #[test]
    fn union_contained_rect() {
        let u = union_rects([0.0, 0.0, 100.0, 100.0], [25.0, 25.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 100.0, 100.0]);
    }
}
