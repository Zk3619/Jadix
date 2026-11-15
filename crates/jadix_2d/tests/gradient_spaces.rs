use jadix_2d::*;

#[test]
fn polar_hue_paths_powerless_hue_and_alpha_match_analytic_hsl() {
    let stops = GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap();
    let base = VectorGradient::linear([0., 0.], [1., 0.], stops)
        .unwrap()
        .with_color_space(GradientSpace::Hsl);
    for (direction, expected) in [
        (HueDirection::Shorter, [1., 0., 1., 1.]),
        (HueDirection::Longer, [0., 1., 0., 1.]),
        (HueDirection::Increasing, [0., 1., 0., 1.]),
        (HueDirection::Decreasing, [1., 0., 1., 1.]),
    ] {
        assert!(base
            .clone()
            .with_hue_direction(direction)
            .sample([0.5, 0.])
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-5));
    }
    // Premultiply S/L, never hue: t=.5 of transparent red and blue has
    // full saturation/lightness at 300deg, then alpha .5.
    let base = VectorGradient::linear(
        [0., 0.],
        [1., 0.],
        GradientStops::new(&[(0., [1., 0., 0., 0.]), (1., [0., 0., 1., 1.])]).unwrap(),
    )
    .unwrap()
    .with_color_space(GradientSpace::Hsl);
    assert!(base
        .sample([0.5, 0.])
        .iter()
        .zip([0.5, 0., 0.5, 0.5])
        .all(|(a, b)| (a - b).abs() < 1e-5));
    // A gray endpoint borrows the chromatic endpoint's hue.
    for space in [GradientSpace::Oklch, GradientSpace::Hsl] {
        let g = VectorGradient::linear(
            [0., 0.],
            [1., 0.],
            GradientStops::new(&[(0., [0.5, 0.5, 0.5, 1.]), (1., [0., 0., 1., 1.])]).unwrap(),
        )
        .unwrap()
        .with_color_space(space);
        let a = g
            .clone()
            .with_hue_direction(HueDirection::Shorter)
            .sample([0.5, 0.]);
        let b = g
            .clone()
            .with_hue_direction(HueDirection::Increasing)
            .sample([0.5, 0.]);
        assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5));
    }
}
fn stops() -> GradientStops {
    GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap()
}
#[test]
fn linear_srgb_encoded_srgb_and_oklab_midpoints_match_known_values() {
    let base = VectorGradient::linear([0., 0.], [1., 0.], stops()).unwrap();
    let linear = base.clone().sample([0.5, 0.]);
    assert_eq!(linear, [0.5, 0., 0.5, 1.]);
    let srgb = base
        .clone()
        .with_color_space(GradientSpace::Srgb)
        .sample([0.5, 0.]);
    assert!((srgb[0] - 0.21404114).abs() < 1e-6 && (srgb[2] - srgb[0]).abs() < 1e-6);
    let lab = base
        .with_color_space(GradientSpace::Oklab)
        .sample([0.5, 0.]);
    for (a, b) in lab.into_iter().zip([0.2637343, 0.0865717, 0.3628243, 1.]) {
        assert!((a - b).abs() < 2e-5, "{a}/{b}");
    }
}
#[test]
fn premultiplied_space_interpolation_cannot_leak_transparent_color() {
    let stops = GradientStops::new(&[(0., [1., 0., 0., 0.]), (1., [0., 0., 1., 1.])]).unwrap();
    for space in [
        GradientSpace::LinearSrgb,
        GradientSpace::Srgb,
        GradientSpace::Oklab,
    ] {
        let g = VectorGradient::linear([0., 0.], [1., 0.], stops.clone())
            .unwrap()
            .with_color_space(space);
        let p = g.sample([0.5, 0.]);
        assert!(p[0] < 1e-5 && p[1] < 1e-5 && (p[2] - 0.5).abs() < 1e-5);
        assert!(g.with_alpha_mode(GradientAlpha::Straight).sample([0.5, 0.])[0] > 0.05);
    }
}
#[test]
fn absolute_sweep_domain_and_signed_radial_preserve_color_geometry() {
    let g = VectorGradient::sweep_range([0., 0.], -90., 630., stops()).unwrap();
    let p = g.sample([1., 0.]);
    assert!((p[2] - 0.125).abs() < 1e-6);
    let p = g.sample([0., -1.]);
    assert!((p[2] - 0.5).abs() < 1e-6);
    let g = VectorGradient::radial_signed([0., 0.], -10., [0., 0.], 10., stops()).unwrap();
    assert!((g.sample([5., 0.])[2] - 0.75).abs() < 1e-6);
    assert!(VectorGradient::radial([0., 0.], -10., [0., 0.], 10., stops()).is_err());
}
