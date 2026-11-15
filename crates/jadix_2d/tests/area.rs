use jadix_2d::*;
#[test]
fn analytic_rectangles_clips_holes_and_per_draw_composition() {
    let mut c = RasterCanvas::new_hdr(8, 8);
    c.set_area_antialiasing(true);
    c.set_fill("#ffffff");
    c.fill_rect(1.125, 2.25, 0.5, 0.5);
    assert!((c.buf[(2 * 8 + 1) * 4 + 3] - 0.25).abs() < 1e-6);
    c.fill_rect(1.125, 2.25, 0.5, 0.5);
    assert!((c.buf[(2 * 8 + 1) * 4 + 3] - 0.4375).abs() < 1e-6);
    let mut c = RasterCanvas::new_hdr(8, 8);
    c.set_area_antialiasing(true);
    c.set_fill("white");
    c.clip_rect(1.25, 1.5, 2., 2.);
    c.fill_rect(0., 0., 8., 8.);
    assert!((c.buf[(8 + 1) * 4 + 3] - 0.375).abs() < 1e-6);
    let mut path = filled_rect(0., 0., 4., 4.);
    path.extend(filled_rect(1., 1., 2., 2.));
    let mut even = RasterCanvas::new_hdr(8, 8);
    even.set_area_antialiasing(true);
    even.set_fill("white");
    even.set_fill_rule(FillRule::EvenOdd);
    even.fill_path(&path);
    assert_eq!(even.buf[(8 + 1) * 4 + 3], 0.);
    assert!((even.buf.iter().skip(3).step_by(4).sum::<f32>() - 12.).abs() < 1e-5);
    let mut nonzero = RasterCanvas::new_hdr(8, 8);
    nonzero.set_area_antialiasing(true);
    nonzero.set_fill("white");
    nonzero.fill_path(&path);
    assert_eq!(nonzero.buf[(8 + 1) * 4 + 3], 1.);
}
#[test]
fn analytic_slivers_and_transformed_curve_area() {
    let mut c = RasterCanvas::new_hdr(32, 32);
    c.set_area_antialiasing(true);
    c.set_fill("white");
    c.fill_rect(1.2, 1.2, 0.001, 10.);
    let area = c.buf.iter().skip(3).step_by(4).sum::<f32>();
    assert!((area - 0.01).abs() < 2e-6, "{area}");
    let mut c = RasterCanvas::new_hdr(32, 32);
    c.set_area_antialiasing(true);
    c.set_fill("white");
    assert!(c.set_curve_tolerance(0.001));
    c.fill_path(&filled_circle(16., 16., 5., 256));
    let area = c.buf.iter().skip(3).step_by(4).sum::<f32>();
    assert!((area - std::f32::consts::PI * 25.).abs() < 0.04, "{area}");
}

#[test]
fn subpixel_rounded_rectangles_preserve_radii_and_invalid_dimensions() {
    let mut c = RasterCanvas::new_hdr(4, 4);
    c.set_area_antialiasing(true);
    assert!(c.set_curve_tolerance(0.001));
    c.set_fill("white");
    c.fill_path(&rounded_rect(1.125, 1.25, 0.5, 0.5, [0.25; 4]));
    let area = c.buf.iter().skip(3).step_by(4).sum::<f32>();
    assert!(
        (area - std::f32::consts::PI * 0.0625).abs() < 0.003,
        "{area}"
    );
    assert!(filled_rect(0., 0., -1., 1.).is_empty());
    assert!(filled_rect(f32::NAN, 0., 1., 1.).is_empty());
    assert!(!c.set_curve_tolerance(0.));
    assert!(!c.set_curve_tolerance(f32::NAN));
}
