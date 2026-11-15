use jadix_2d::*;

fn pixel(c: &dyn Canvas2D, x: u32, y: u32) -> [u8; 4] {
    c.get_image_data(x, y, 1, 1).unwrap().try_into().unwrap()
}
fn red_bias(alpha: f32) -> LayerFilter {
    let mut m = [0.; 20];
    m[4] = 1.;
    m[19] = alpha;
    LayerFilter::ColorMatrix(m)
}
#[test]
fn filter_order_empty_content_final_mask_and_software_continuation() {
    for reverse in [false, true] {
        let paint = |c: &mut dyn Canvas2D| {
            c.clear("#0000ff");
            let mut filters = vec![red_bias(0.5), LayerFilter::Gaussian { sigma: [2., 1.] }];
            if reverse {
                filters.reverse();
            }
            c.push_layer_with_options(LayerOptions {
                filters,
                clip: Some([4.5, 0., 3., 16.]),
                mask: Some(Image2D::from_rgba8(16, 16, &[255; 16 * 16 * 4]).unwrap()),
                ..Default::default()
            });
            c.pop_layer();
        };
        let mut cpu = RasterCanvas::new_linear(16, 16);
        paint(&mut cpu);
        let mut gpu = GpuCanvas::new_linear(16, 16, None);
        paint(&mut gpu);
        assert_eq!(
            cpu.get_image_data(0, 0, 16, 16),
            gpu.get_image_data(0, 0, 16, 16)
        );
        assert!(
            !gpu.into_ops()[1].is_empty(),
            "matrix bias creates content in an empty group"
        );
        assert_eq!(pixel(&cpu, 3, 8), [0, 0, 255, 255]);
        assert!(pixel(&cpu, 5, 8)[0] > 180);
        assert!(pixel(&cpu, 4, 8)[0] < pixel(&cpu, 5, 8)[0]);
        // Force continuation while an effect group is active, then pop once.
        let mut gpu = GpuCanvas::new_linear(16, 16, None);
        gpu.push_layer_with_options(LayerOptions {
            filters: vec![red_bias(0.5)],
            ..Default::default()
        });
        gpu.put_image_data(0, 0, 1, 1, &[255; 4]);
        gpu.pop_layer();
        assert!(gpu.needs_cpu);
        assert_eq!(pixel(&gpu, 8, 8), [255, 0, 0, 128]);
    }
}

#[test]
fn fractional_shadow_uses_source_alpha_and_original_painter_order() {
    let mut c = RasterCanvas::new_linear(8, 8);
    c.push_layer_with_options(LayerOptions {
        filters: vec![LayerFilter::DropShadow {
            sigma: [0., 0.],
            offset: [1.5, 0.],
            color: [1., 0., 0., 0.5],
        }],
        ..Default::default()
    });
    c.put_image_data(2, 2, 1, 1, &[0, 0, 255, 128]);
    c.pop_layer();
    assert_eq!(pixel(&c, 2, 2), [0, 0, 255, 128]);
    assert_eq!(pixel(&c, 3, 2), [255, 0, 0, 32]);
    assert_eq!(pixel(&c, 4, 2), [255, 0, 0, 32]);
    assert_eq!(pixel(&c, 5, 2), [0; 4]);
}

#[test]
fn four_corners_inset_zero_sigma_normalization_and_capture() {
    let radii = [0., 12., 4., 8.];
    let paint = |c: &mut dyn Canvas2D| {
        c.save();
        c.translate(3., 2.);
        c.scale(1.2, 0.8);
        c.draw_rounded_rect_shadow([4., 4., 24., 24.], radii, 2., [0.2, 0.5, 1., 0.8], true);
        c.restore();
    };
    let mut c = RasterCanvas::new_linear(48, 40);
    paint(&mut c);
    let mut g = GpuCanvas::new_linear(48, 40, None);
    paint(&mut g);
    assert_eq!(
        c.get_image_data(0, 0, 48, 40),
        g.get_image_data(0, 0, 48, 40)
    );
    assert_eq!(pixel(&c, 3, 3), [0; 4]);
    assert_eq!(pixel(&c, 20, 16), [0; 4]);
    assert!(c
        .get_image_data(0, 0, 48, 40)
        .unwrap()
        .iter()
        .any(|v| *v > 25));
    let mut c = RasterCanvas::new_linear(32, 32);
    c.draw_rounded_rect_shadow([0., 0., 32., 32.], radii, 0., [1.; 4], true);
    assert!(c
        .get_image_data(0, 0, 32, 32)
        .unwrap()
        .iter()
        .all(|v| *v == 0));
    c.draw_rounded_rect_shadow([0., 0., 32., 32.], [0., 100., 0., 100.], 0., [1.; 4], false);
    assert!(pixel(&c, 0, 0)[3] > 0);
    assert_eq!(pixel(&c, 31, 0)[3], 0);
    let mut g = GpuCanvas::new_linear(32, 32, None);
    g.draw_blurred_rounded_rect([4., 4., 16., 16.], 4., 2., [1.; 4]);
    g.draw_rounded_rect_shadow([4., 4., 16., 16.], radii, 2., [1.; 4], true);
    let ops = g.into_ops();
    let PathGpuOp::Fill(a) = &ops[0] else {
        panic!()
    };
    let PathGpuOp::Fill(b) = &ops[1] else {
        panic!()
    };
    assert_eq!(a.grad.unwrap().kind, 8);
    assert_eq!(a.grad.unwrap().center, [4., 2.]);
    assert_eq!(b.grad.unwrap().kind, 9);
}

#[test]
fn invalid_effects_are_atomic_identity_and_large_sigma_is_finite() {
    let mut c = RasterCanvas::new_linear(8, 8);
    c.push_layer_with_options(LayerOptions {
        filters: vec![
            LayerFilter::Gaussian {
                sigma: [f32::NAN, 2.],
            },
            LayerFilter::DropShadow {
                sigma: [1., -1.],
                offset: [0., 0.],
                color: [1.; 4],
            },
            LayerFilter::ColorMatrix([f32::INFINITY; 20]),
        ],
        ..Default::default()
    });
    c.set_fill("#804020");
    c.fill_rect(2., 2., 2., 2.);
    c.pop_layer();
    assert_eq!(pixel(&c, 2, 2), [128, 64, 32, 255]);
    assert_eq!(pixel(&c, 1, 1), [0; 4]);
    c.push_layer_with_options(LayerOptions {
        filters: vec![LayerFilter::Gaussian {
            sigma: [f32::MAX; 2],
        }],
        ..Default::default()
    });
    c.set_fill("white");
    c.fill_rect(0., 0., 8., 8.);
    c.pop_layer();
    assert!(c.into_pixels().iter().all(|v| v.is_finite()));
}
