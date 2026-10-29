use super::*;

#[test]
fn nearest_index_and_snap_target() {
    assert_eq!(index_from_offset(-200.0, 100.0, 0, 5, 4, false), 2);
    assert!((snap_target_for_index(2, 100.0, 0) - (-200.0)).abs() < 0.01);
    // clones=2, off=-200 → track_i=2 → logical=0
    assert_eq!(index_from_offset(-200.0, 100.0, 2, 5, 4, true), 0);
}
