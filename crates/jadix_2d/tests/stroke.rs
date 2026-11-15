use jadix_2d::*;

#[test]
fn caps_joins_dashes_and_miter_limit_share_rendering_and_hit_geometry() {
    let line = polyline(&[(8., 16.), (40., 16.)]);
    for cap in [
        StrokeLinecap::Butt,
        StrokeLinecap::Round,
        StrokeLinecap::Square,
    ] {
        let style = PathStyle::stroke([1., 0., 0., 0.5], 4.).linecap(cap);
        let pixels = rasterize_path_rgba(&line, &style, &[], 48, 32, 8);
        for y in 0..32 {
            for x in 0..48 {
                let mut inside = 0;
                for sy in 0..8 {
                    for sx in 0..8 {
                        inside += u32::from(path_stroke_contains(
                            &line,
                            &style,
                            (
                                x as f32 + (sx as f32 + 0.5) / 8.,
                                y as f32 + (sy as f32 + 0.5) / 8.,
                            ),
                        ));
                    }
                }
                assert!(
                    (pixels[(y * 48 + x) * 4 + 3] as f32 - inside as f32 * 127.5 / 64.).abs() <= 1.
                );
            }
        }
        assert_eq!(
            path_stroke_contains(&line, &style, (6.2, 16.)),
            cap != StrokeLinecap::Butt
        );
        assert_eq!(
            path_stroke_contains(&line, &style, (6.2, 17.8)),
            cap == StrokeLinecap::Square
        );
    }
    let dashed = PathStyle::stroke([1.; 4], 2.)
        .linecap(StrokeLinecap::Butt)
        .dashes(&[4., 2., 2.], 0.);
    assert!(path_stroke_contains(&line, &dashed, (10., 16.)));
    assert!(!path_stroke_contains(&line, &dashed, (13., 16.)));
    assert_eq!(
        stroke_outline(&line, &dashed, 0.05),
        stroke_outline(
            &line,
            &dashed.clone().dashes(&[4., 2., 2., 4., 2., 2.], 0.),
            0.05
        )
    );
    let corner = polyline(&[(8., 24.), (24., 8.), (40., 24.)]);
    let miter = PathStyle::stroke([1.; 4], 8.).linejoin(StrokeLinejoin::Miter);
    assert!(path_stroke_contains(&corner, &miter, (24., 3.)));
    assert!(!path_stroke_contains(
        &corner,
        &miter.miter_limit(1.),
        (24., 3.)
    ));
}

#[test]
fn half_transparent_self_intersections_and_subpaths_are_painted_once() {
    let path = decode_path_d("M4 4 L28 28 L4 28 L28 4 M16 0 V32");
    let style = PathStyle::stroke([1., 0., 0., 0.5], 6.);
    let rgba = rasterize_path_rgba(&path, &style, &[], 32, 32, 4);
    assert!(rgba.chunks_exact(4).all(|p| p[3] <= 128));
    assert_eq!(rgba[(16 * 32 + 16) * 4 + 3], 128);
    let mesh = tessellate_path_mesh(&path, &style, &[], 0.);
    // Probe non-edge points: the direct GPU mesh must have no overlapping faces.
    for y in 0..32 {
        for x in 0..32 {
            let p = (x as f32 + 0.371, y as f32 + 0.613);
            let count = mesh
                .iter()
                .filter(|t| {
                    let cross = |a: Point2, b: Point2| {
                        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
                    };
                    let v = [cross(t.a, t.b), cross(t.b, t.c), cross(t.c, t.a)];
                    v.iter().all(|v| *v > 0.) || v.iter().all(|v| *v < 0.)
                })
                .count();
            assert!(count <= 1, "{p:?}: {count}");
        }
    }
}

#[test]
fn affine_strokes_transform_width_caps_and_dash_lengths_in_user_space() {
    for mut canvas in [
        Box::new(RasterCanvas::new(64, 32)) as Box<dyn Canvas2D>,
        Box::new(GpuCanvas::new(64, 32, None)),
    ] {
        canvas.set_line_width(4.);
        canvas.set_line_cap(LineCap::Butt);
        canvas.set_line_dash(&[4., 2.]);
        canvas.set_stroke("#ff000080");
        canvas.set_transform([3., 0., 1., 1., 0., 0.]);
        let line = polyline(&[(2., 12.), (14., 12.)]);
        canvas.stroke_path(&line);
        assert!(canvas.is_point_in_stroke(&line, 21., 12.));
        assert!(!canvas.is_point_in_stroke(&line, 33., 12.));
        assert!(!canvas.is_point_in_stroke(&line, 21., 15.));
        let image = canvas.get_image_data(0, 0, 64, 32).unwrap();
        assert_eq!(image[(12 * 64 + 21) * 4 + 3], 128);
        assert_eq!(image[(15 * 64 + 21) * 4 + 3], 0);
        canvas.save();
        canvas.set_line_dash(&[]);
        canvas.restore();
        canvas.set_line_dash(&[-1.]); // Invalid updates preserve the previous pattern.
        assert!(!canvas.is_point_in_stroke(&line, 33., 12.));
    }
    assert!(stroke_outline(&line_path(), &PathStyle::stroke([1.; 4], 0.), 0.05).is_empty());
    assert!(stroke_outline(
        &[
            PathCommand::MoveTo((f32::NAN, 0.)),
            PathCommand::LineTo((1., 1.))
        ],
        &PathStyle::default(),
        0.05
    )
    .is_empty());
}
fn line_path() -> Vec<PathCommand> {
    polyline(&[(0., 0.), (1., 1.)])
}
