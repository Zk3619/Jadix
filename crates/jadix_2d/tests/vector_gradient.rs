use jadix_2d::*;
fn stops() -> GradientStops {
    GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap()
}
fn near(a: [f32; 4], b: [f32; 4]) {
    assert!(
        a.iter().zip(b).all(|(a, b)| (*a - b).abs() < 0.00002),
        "{a:?} vs {b:?}"
    );
}

#[test]
fn interval_midpoints_are_analytic_and_copy_on_write() {
    let original = stops();
    for midpoint in [0.1, 0.25, 0.5, 0.8, 0.95] {
        let biased = original.clone().with_midpoints(&[Some(midpoint)]).unwrap();
        let g = VectorGradient::linear([0., 0.], [1., 0.], biased).unwrap();
        near(g.sample([midpoint, 0.]), [0.5, 0., 0.5, 1.]);
        near(g.sample([0., 0.]), [1., 0., 0., 1.]);
        near(g.sample([1., 0.]), [0., 0., 1., 1.]);
    }
    near(
        VectorGradient::linear([0., 0.], [1., 0.], original.clone())
            .unwrap()
            .sample([0.25, 0.]),
        [0.75, 0., 0.25, 1.],
    );
    assert!(original.clone().with_midpoints(&[]).is_err());
    for invalid in [0., 1., -1., f32::NAN, f32::INFINITY] {
        assert!(original.clone().with_midpoints(&[Some(invalid)]).is_err());
    }
    let single = GradientStops::new(&[(0.5, [1.; 4])])
        .unwrap()
        .with_midpoints(&[])
        .unwrap();
    near(
        VectorGradient::linear([0., 0.], [1., 0.], single)
            .unwrap()
            .sample([0., 0.]),
        [1.; 4],
    );
}

#[test]
fn exact_hard_stops_spread_and_alpha_interpolation() {
    let hard = GradientStops::new(&[
        (0., [1., 0., 0., 1.]),
        (0.5, [1., 0., 0., 1.]),
        (0.5, [0., 0., 1., 1.]),
        (1., [0., 0., 1., 1.]),
    ])
    .unwrap();
    let gradient = VectorGradient::linear([0., 0.], [10., 0.], hard).unwrap();
    near(gradient.sample([4.999, 0.]), [1., 0., 0., 1.]);
    near(gradient.sample([5., 0.]), [0., 0., 1., 1.]);
    let g = VectorGradient::linear([0., 0.], [10., 0.], stops()).unwrap();
    near(
        g.clone().with_extend(ExtendMode::Repeat).sample([-2.5, 4.]),
        [0.25, 0., 0.75, 1.],
    );
    near(
        g.clone()
            .with_extend(ExtendMode::Reflect)
            .sample([-2.5, 4.]),
        [0.75, 0., 0.25, 1.],
    );
    near(
        g.clone().with_extend(ExtendMode::Decal).sample([10.1, 4.]),
        [0.; 4],
    );
    near(g.sample([-1., 4.]), [1., 0., 0., 1.]);
    let fade = GradientStops::new(&[(0., [1., 0., 0., 0.]), (1., [0., 0., 1., 1.])]).unwrap();
    let g = VectorGradient::linear([0., 0.], [10., 0.], fade).unwrap();
    near(g.sample([5., 0.]), [0., 0., 0.5, 0.5]);
    near(
        g.with_alpha_mode(GradientAlpha::Straight).sample([5., 0.]),
        [0.25, 0., 0.25, 0.5],
    );
}
#[test]
fn radial_cones_equal_radii_tangent_decreasing_and_sweep() {
    let g = VectorGradient::radial([0., 0.], 4., [0., 0.], 20., stops()).unwrap();
    near(g.sample([16., 0.]), [0.25, 0., 0.75, 1.]);
    let g = VectorGradient::radial([0., 0.], 20., [0., 0.], 4., stops()).unwrap();
    near(g.sample([16., 0.]), [0.75, 0., 0.25, 1.]);
    let g = VectorGradient::radial([0., 0.], 0., [10., 0.], 10., stops()).unwrap();
    near(g.sample([10., 0.]), [0.5, 0., 0.5, 1.]);
    near(g.sample([-10., 0.]), [0.; 4]);
    let g = VectorGradient::radial([0., 0.], 5., [10., 0.], 5., stops()).unwrap();
    near(g.sample([5., 0.]), [0., 0., 1., 1.]);
    near(g.sample([5., 6.]), [0.; 4]);
    let g = VectorGradient::sweep([0., 0.], 0., 180., stops())
        .unwrap()
        .with_extend(ExtendMode::Decal);
    near(g.sample([0., 10.]), [0.5, 0., 0.5, 1.]);
    near(g.sample([0., 0.]), [1., 0., 0., 1.]);
    near(g.sample([-0., -0.]), [1., 0., 0., 1.]);
    near(g.sample([0., -10.]), [0.; 4]);
    let reverse = VectorGradient::sweep([0., 0.], 0., -180., stops()).unwrap();
    near(reverse.sample([0., -10.]), [0.5, 0., 0.5, 1.]);
}
#[test]
fn thousands_of_stops_validation_and_boundary_colors() {
    let data: Vec<_> = (0..4096)
        .map(|i| {
            let t = i as f32 / 4095.;
            (t, [t, 0., 1. - t, 0.2 + t * 0.8])
        })
        .collect();
    let s = GradientStops::new(&data).unwrap();
    assert_eq!(s.len(), 4096);
    let g = VectorGradient::linear([0., 0.], [1., 0.], s).unwrap();
    near(g.sample([0., 0.]), [0., 0., 0.2, 0.2]);
    near(g.sample([1., 0.]), [1., 0., 0., 1.]);
    assert!(GradientStops::new(&[]).is_err());
    assert!(GradientStops::new(&vec![(0., [1.; 4]); 4097]).is_err());
    assert!(GradientStops::new(&[(0.5, [1.; 4]), (0.4, [1.; 4])]).is_err());
    assert!(GradientStops::new(&[(f32::NAN, [1.; 4])]).is_err());
    assert!(VectorGradient::linear([0.; 2], [0.; 2], stops()).is_err());
    assert!(VectorGradient::radial([0.; 2], 1., [0.; 2], 1., stops()).is_err());
    assert!(VectorGradient::radial([0.; 2], -1., [0.; 2], 1., stops()).is_err());
    assert!(VectorGradient::sweep([0.; 2], 0., 361., stops()).is_err());
    near(g.sample([f32::NAN, 0.]), [0.; 4]);
}
#[test]
fn vector_brush_affine_stroke_clips_groups_and_software_continuation() {
    let paint = |c: &mut dyn Canvas2D| {
        c.save();
        c.translate(8., 4.);
        c.scale(1.2, 0.8);
        c.clip_rect(0., 0., 32., 32.);
        c.push_layer_with_options(LayerOptions {
            filters: vec![LayerFilter::Gaussian { sigma: [1., 2.] }],
            ..Default::default()
        });
        c.set_fill_brush(Brush2D::vector(
            VectorGradient::linear([0., 0.], [32., 0.], stops()).unwrap(),
        ));
        c.fill_rect(0., 0., 32., 32.);
        c.set_stroke_brush(Brush2D::vector(
            VectorGradient::radial([16., 16.], 0., [16., 16.], 20., stops()).unwrap(),
        ));
        c.set_line_width(3.);
        c.stroke_path(&polyline(&[(0., 0.), (24., 24.)]));
        c.pop_layer();
        c.restore();
    };
    let mut cpu = RasterCanvas::new_linear(64, 48);
    paint(&mut cpu);
    let mut gpu = GpuCanvas::new_linear(64, 48, None);
    paint(&mut gpu);
    assert!(!gpu.needs_cpu);
    assert_eq!(
        cpu.get_image_data(0, 0, 64, 48),
        gpu.get_image_data(0, 0, 64, 48)
    );
    gpu.put_image_data(60, 40, 1, 1, &[255; 4]);
    cpu.put_image_data(60, 40, 1, 1, &[255; 4]);
    assert!(gpu.needs_cpu);
    assert_eq!(gpu.into_raster(), cpu.into_pixels());
}
