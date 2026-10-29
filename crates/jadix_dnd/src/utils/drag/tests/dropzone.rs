use super::*;
use crate::utils::drag::geometry::Rect;

#[test]
fn overlap_center_requires_ghost_center_in_zone() {
    let zone = Rect::from_bounds([0.0, 0.0, 100.0, 100.0]);
    let dragged = Rect::from_bounds([90.0, 90.0, 30.0, 30.0]);
    assert!(DropOverlap::Pointer.passes([95.0, 95.0], dragged, zone));
    assert!(!DropOverlap::Center.passes([95.0, 95.0], dragged, zone));
    let centered = Rect::from_bounds([40.0, 40.0, 20.0, 20.0]);
    assert!(DropOverlap::Center.passes([50.0, 50.0], centered, zone));
}

#[test]
fn overlap_ratio_threshold() {
    let zone = Rect::from_bounds([0.0, 0.0, 100.0, 100.0]);
    let half_out = Rect::from_bounds([75.0, 0.0, 50.0, 100.0]);
    assert!(DropOverlap::Ratio(0.49).passes([80.0, 50.0], half_out, zone));
    assert!(!DropOverlap::Ratio(0.51).passes([80.0, 50.0], half_out, zone));
}
