use jadix_live2d::View;

#[test]
fn fitted_view_round_trips_center_and_preserves_aspect() {
    let view = View::fit([-2.0, -1.0, 2.0, 1.0], [800, 400], 0.1).unwrap();
    let center = view.model_position([400.0, 200.0]).unwrap();
    assert!(center[0].abs() < 1e-5 && center[1].abs() < 1e-5);
    assert!((view.matrix().scale_x() * 800.0 - view.matrix().scale_y() * 400.0).abs() < 1e-4);
}

#[test]
fn invalid_view_and_pointer_inputs_are_rejected() {
    assert!(View::fit([0.0, 0.0, 1.0, 1.0], [0, 10], 0.0).is_err());
    assert!(View::fit([1.0, 0.0, 0.0, 1.0], [10, 10], 0.0).is_err());
    let view = View::fit([-1.0, -1.0, 1.0, 1.0], [100, 100], 0.0).unwrap();
    assert!(view.model_position([f32::NAN, 0.0]).is_none());
    assert!(View::fit(
        [0.0, 0.0, f32::from_bits(1), f32::from_bits(1)],
        [1, 1],
        0.0
    )
    .is_err());
    let large = View::fit([-f32::MAX, -f32::MAX, f32::MAX, f32::MAX], [100, 100], 0.0).unwrap();
    assert_eq!(large.model_position([50.0, 50.0]), Some([0.0, 0.0]));
    assert!(large.model_position([f32::MAX, f32::MAX]).is_none());
}
