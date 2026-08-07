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

/// Union an arbitrary list of rects. Returns `None` when `rects` is empty.
pub fn rects_union(rects: &[[f32; 4]]) -> Option<[f32; 4]> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    Some(iter.fold(first, union_rects))
}

/// True when `damage` fully covers `viewport` (same coordinate space, `[x,y,w,h]`).
///
/// Empty / degenerate damage never covers. Floating slack of 0.5px accounts for layout rounding.
pub fn damage_covers_full(viewport: [f32; 4], damage: [f32; 4]) -> bool {
    if viewport[2] <= 0.0 || viewport[3] <= 0.0 || damage[2] <= 0.0 || damage[3] <= 0.0 {
        return false;
    }
    const EPS: f32 = 0.5;
    let (vx, vy, vw, vh) = (viewport[0], viewport[1], viewport[2], viewport[3]);
    let (dx, dy, dw, dh) = (damage[0], damage[1], damage[2], damage[3]);
    dx <= vx + EPS
        && dy <= vy + EPS
        && dx + dw + EPS >= vx + vw
        && dy + dh + EPS >= vy + vh
}

#[cfg(test)]
mod tests {
    use super::{damage_covers_full, rects_union, union_rects};

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

    #[test]
    fn rects_union_empty_and_many() {
        assert_eq!(rects_union(&[]), None);
        assert_eq!(
            rects_union(&[[0.0, 0.0, 10.0, 10.0]]),
            Some([0.0, 0.0, 10.0, 10.0])
        );
        let u = rects_union(&[
            [0.0, 0.0, 10.0, 10.0],
            [20.0, 5.0, 10.0, 10.0],
            [5.0, 0.0, 5.0, 5.0],
        ])
        .expect("non-empty");
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn damage_covers_full_viewport() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert!(damage_covers_full(vp, vp));
        assert!(damage_covers_full(vp, [-1.0, -1.0, 802.0, 602.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 400.0, 600.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 0.0, 0.0]));
    }
}
