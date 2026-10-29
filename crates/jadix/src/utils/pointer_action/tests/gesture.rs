use super::*;

#[test]
fn pinch_out_doubles_scale() {
    let mut g = GestureSession::begin([0.0, 0.0], [10.0, 0.0]);
    let s = g.tick([0.0, 0.0], [20.0, 0.0]);
    assert!((s.scale - 2.0).abs() < 1e-4);
    assert!((s.ds - 1.0).abs() < 1e-4);
    assert!((s.distance - 20.0).abs() < 1e-4);
}

#[test]
fn rotate_quarter_turn() {
    let mut g = GestureSession::begin([0.0, 0.0], [10.0, 0.0]);
    let s = g.tick([0.0, 0.0], [0.0, 10.0]);
    assert!((s.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
    assert!(s.da > 1.0);
}

#[test]
fn centroid_translation() {
    let mut g = GestureSession::begin([0.0, 0.0], [10.0, 0.0]);
    let s = g.tick([10.0, 10.0], [20.0, 10.0]);
    assert!((s.centroid[0] - 15.0).abs() < 1e-4);
    assert!((s.translation[0] - 10.0).abs() < 1e-4);
    assert!((s.translation[1] - 10.0).abs() < 1e-4);
    assert!((s.scale - 1.0).abs() < 1e-4);
}

#[test]
fn end_zeroes_deltas() {
    let mut g = GestureSession::begin([0.0, 0.0], [10.0, 0.0]);
    g.tick([0.0, 0.0], [20.0, 0.0]);
    let e = g.end();
    assert!((e.scale - 2.0).abs() < 1e-4);
    assert_eq!(e.ds, 0.0);
    assert_eq!(e.da, 0.0);
}

#[test]
fn from_pair_and_multi_box() {
    let s = GestureState::from_pair([[0.0, 0.0], [4.0, 0.0]], [[0.0, 0.0], [8.0, 0.0]]);
    assert!((s.scale - 2.0).abs() < 1e-4);
    let b = GestureState::box_of(&[[1.0, 2.0], [5.0, 8.0], [3.0, 0.0]]);
    assert!((b.x - 1.0).abs() < 1e-4);
    assert!((b.width - 4.0).abs() < 1e-4);
    assert!((b.height - 8.0).abs() < 1e-4);
}
