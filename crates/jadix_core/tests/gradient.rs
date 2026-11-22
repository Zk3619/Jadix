use jadix_core::presentation::style::{GradientKind, GradientPosition, GradientSize, GradientStop, ViewGradient};

fn stop(position: Option<f32>, color: [u8; 4]) -> GradientStop {
    GradientStop { color, position: position.map(GradientPosition::Fraction) }
}

#[test]
fn omitted_stops_interpolate_between_explicit_neighbors_in_radians() {
    let gradient = ViewGradient { geometry: GradientKind::Linear { angle: std::f32::consts::FRAC_PI_2 }, repeating: false,
        stops: vec![stop(Some(0.2), [255, 0, 0, 255]), stop(None, [0, 255, 0, 255]), stop(Some(0.8), [0, 0, 255, 255])] };
    let paint = gradient.resolve([100., 50.]).unwrap();
    assert!((paint.start[0]).abs() < 0.001);
    assert!((paint.end[0] - 100.).abs() < 0.001);
    assert!((paint.start[1] - 25.).abs() < 0.001);
    assert!((paint.positions[2] - 0.5).abs() < 0.001);
}

#[test]
fn clipping_outside_stops_interpolates_premultiplied_alpha() {
    let gradient = ViewGradient { geometry: GradientKind::Linear { angle: 0. }, repeating: false,
        stops: vec![stop(Some(-1.), [255, 0, 0, 0]), stop(Some(1.), [0, 0, 255, 255])] };
    let paint = gradient.resolve([20., 20.]).unwrap();
    assert_eq!(paint.colors[0], [0, 0, 255, 128]);
    assert_eq!(paint.positions.first(), Some(&0.));
    assert_eq!(paint.positions.last(), Some(&1.));
}

#[test]
fn repeating_hard_seams_and_off_center_ellipse_are_preserved() {
    let gradient = ViewGradient { geometry: GradientKind::Radial { circle: false, size: GradientSize::ClosestSide, center: [0.25, 0.5] }, repeating: true,
        stops: vec![stop(Some(0.), [255, 0, 0, 255]), stop(Some(0.5), [0, 0, 255, 255])] };
    let paint = gradient.resolve([200., 100.]).unwrap();
    assert_eq!(paint.start, [50., 50.]);
    assert!(paint.radii[0] > 150. && paint.radii[1] > 150.);
    assert!(paint.positions.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(paint.positions.windows(2).any(|pair| pair[0] == pair[1]));
    assert!(gradient.resolve([0., 100.]).is_none());
}
