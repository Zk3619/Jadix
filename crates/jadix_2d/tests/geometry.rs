use jadix_2d::*;
fn shape(path: &[PathCommand], rule: FillRule) -> PathGeometry2D {
    PathGeometry2D::from_path(path, rule, PathBooleanOptions::default()).unwrap()
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> PathGeometry2D {
    shape(&filled_rect(x, y, w, h), FillRule::NonZero)
}
#[test]
fn boolean_rectangles_area_bounds_and_identity() {
    let a = rect(1.125, 2.25, 5., 4.);
    let b = rect(3.125, 3.25, 6., 5.);
    for (op, area) in [
        (PathBooleanOp::Intersection, 9.),
        (PathBooleanOp::Union, 41.),
        (PathBooleanOp::Difference, 11.),
        (PathBooleanOp::Xor, 32.),
    ] {
        let r = a.boolean(&b, op, Default::default()).unwrap();
        assert!((r.area() - area).abs() < 1e-10, "{op:?} {}", r.area());
    }
    assert_eq!(
        a.boolean(&a, PathBooleanOp::Difference, Default::default())
            .unwrap()
            .area(),
        0.
    );
    assert_eq!(
        a.boolean(&a, PathBooleanOp::Xor, Default::default())
            .unwrap()
            .area(),
        0.
    );
    assert!(
        (a.boolean(&a, PathBooleanOp::Intersection, Default::default())
            .unwrap()
            .area()
            - 20.)
            .abs()
            < 1e-10
    );
    assert_eq!(a.bounds(), Some([1.125, 2.25, 6.125, 6.25]));
    assert_eq!(a.to_path().as_ptr(), a.clone().to_path().as_ptr());
}
#[test]
fn winding_holes_touching_and_slivers() {
    let mut path = filled_rect(0., 0., 10., 10.);
    path.extend(filled_rect(2., 2., 6., 6.));
    let hole = shape(&path, FillRule::EvenOdd);
    let nonzero = shape(&path, FillRule::NonZero);
    assert!((hole.area() - 64.).abs() < 1e-10);
    assert!((nonzero.area() - 100.).abs() < 1e-10);
    assert!(!hole.contains([5., 5.]));
    assert!(nonzero.contains([5., 5.]));
    let cut = hole
        .boolean(
            &rect(0., 0., 5., 10.),
            PathBooleanOp::Intersection,
            Default::default(),
        )
        .unwrap();
    assert!((cut.area() - 32.).abs() < 1e-10);
    assert!(rect(0., 0., 1., 1.)
        .boolean(
            &rect(1., 0., 1., 1.),
            PathBooleanOp::Intersection,
            Default::default()
        )
        .unwrap()
        .is_empty());
    let tiny = rect(1., 1., 0.0001, 10.);
    let tiny = tiny
        .boolean(&tiny, PathBooleanOp::Intersection, Default::default())
        .unwrap();
    assert!((tiny.area() - 0.001).abs() < 2e-6);
}
#[test]
fn independent_point_set_algebra_on_rotated_concave_and_curved_shapes() {
    let a = shape(
        &polygon(&[(0., 0.), (9., 1.), (4., 4.), (8., 10.), (0., 8.)]),
        FillRule::NonZero,
    );
    let b = shape(&filled_circle(5., 4., 3.8, 48), FillRule::NonZero)
        .transformed([0.9, 0.2, -0.3, 1.1, 0.5, 0.2])
        .unwrap();
    for op in [
        PathBooleanOp::Union,
        PathBooleanOp::Intersection,
        PathBooleanOp::Difference,
        PathBooleanOp::Xor,
    ] {
        let r = a.boolean(&b, op, Default::default()).unwrap();
        for y in 0..101 {
            for x in 0..103 {
                let p = [x as f64 * 0.113 + 0.00131, y as f64 * 0.117 + 0.00373];
                let (aa, bb) = (a.contains(p), b.contains(p));
                let expected = match op {
                    PathBooleanOp::Union => aa || bb,
                    PathBooleanOp::Intersection => aa && bb,
                    PathBooleanOp::Difference => aa && !bb,
                    PathBooleanOp::Xor => aa != bb,
                };
                assert_eq!(r.contains(p), expected, "{op:?} {p:?}");
            }
        }
    }
}
#[test]
fn geometry_transforms_errors_and_budgets_are_atomic() {
    let a = rect(0., 0., 4., 3.);
    let b = a.transformed([-2., 0., 0., 3., 10., 1.]).unwrap();
    assert!((b.area() - 72.).abs() < 1e-10);
    assert!(b.contains([5., 5.]));
    assert!(a.transformed([0.; 6]).unwrap().is_empty());
    assert_eq!(
        a.transformed([f64::NAN; 6]).unwrap_err(),
        PathBooleanError::InvalidGeometry
    );
    assert_eq!(
        PathGeometry2D::from_path(
            &[PathCommand::MoveTo((f32::NAN, 0.))],
            FillRule::NonZero,
            Default::default()
        )
        .unwrap_err(),
        PathBooleanError::InvalidGeometry
    );
    assert_eq!(
        a.boolean(
            &b,
            PathBooleanOp::Intersection,
            PathBooleanOptions {
                max_work: 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        PathBooleanError::WorkBudget
    );
    assert_eq!(
        a.boolean(
            &b,
            PathBooleanOp::Union,
            PathBooleanOptions {
                max_triangles: 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        PathBooleanError::TriangleBudget
    );
    assert_eq!(
        PathGeometry2D::from_path(
            &filled_circle(0., 0., 10., 64),
            FillRule::NonZero,
            PathBooleanOptions {
                max_path_commands: 2,
                ..Default::default()
            }
        )
        .unwrap_err(),
        PathBooleanError::PathBudget
    );
    assert_eq!(a.area(), 12.);
}
#[test]
fn resolved_clip_is_covered_once_and_records_in_a_scene() {
    let a = rect(1.125, 2.25, 0.5, 0.5);
    let visible = a
        .boolean(&a, PathBooleanOp::Intersection, Default::default())
        .unwrap();
    let scene = Scene2D::record([8, 8], |c| {
        c.set_fill("white");
        visible.fill(c);
    })
    .unwrap();
    let mut c = RasterCanvas::new_hdr(8, 8);
    c.set_area_antialiasing(true);
    scene.draw(&mut c, Default::default()).unwrap();
    assert!((c.buf[(2 * 8 + 1) * 4 + 3] - 0.25).abs() < 1e-6);
}

#[test]
fn invalid_arc_radii_and_flatten_budget_reject_before_tessellation() {
    for radius in [f32::NAN, f32::INFINITY, 1e8] {
        let arc = [
            PathCommand::MoveTo((0., 0.)),
            PathCommand::Arc {
                radii: (radius, 4.),
                x_rot_deg: 0.,
                large: false,
                sweep: true,
                to: (5., 5.),
            },
        ];
        assert_eq!(
            PathGeometry2D::from_path(&arc, FillRule::NonZero, Default::default()).unwrap_err(),
            PathBooleanError::InvalidGeometry
        );
    }
    let curve = decode_path_d("M 0 0 C 100 0 100 100 0 100 Z");
    assert_eq!(
        PathGeometry2D::from_path(
            &curve,
            FillRule::NonZero,
            PathBooleanOptions {
                max_flattened_commands: 4,
                ..Default::default()
            }
        )
        .unwrap_err(),
        PathBooleanError::PathBudget
    );
}

#[test]
fn tessellation_output_budget_aborts_during_emission() {
    let mut path = Vec::new();
    for i in 0..200 {
        path.extend(filled_rect(
            (i % 20) as f32 * 2.,
            (i / 20) as f32 * 2.,
            1.,
            1.,
        ));
    }
    assert_eq!(
        PathGeometry2D::from_path(
            &path,
            FillRule::NonZero,
            PathBooleanOptions {
                max_triangles: 4,
                ..Default::default()
            }
        )
        .unwrap_err(),
        PathBooleanError::TriangleBudget
    );
    assert_eq!(rect(0., 0., 1., 1.).area(), 1.);
}

#[test]
fn close_then_line_reopens_at_the_previous_subpath_start() {
    let path = decode_path_d("M0 0 L4 0 L0 4 Z L-4 0 L0 -4 Z");
    let geometry = shape(&path, FillRule::NonZero);
    assert!((geometry.area() - 16.).abs() < 1e-8);
    assert!(geometry.contains([-1., -1.]));
    assert!(geometry.contains([1., 1.]));
}
