use jadix_2d::{gradient::parse_gradient, *};

fn pixel(c: &dyn Canvas2D, x: u32, y: u32) -> [u8; 4] {
    c.get_image_data(x, y, 1, 1).unwrap().try_into().unwrap()
}
fn tile() -> Image2D {
    Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]).unwrap()
}
#[test]
fn explicit_gradient_domain_survives_shape_bounds_and_affine_state() {
    let paint = |c: &mut dyn Canvas2D| {
        c.set_fill_brush(Brush2D::gradient(
            parse_gradient("linear-gradient(to right,red,blue)").unwrap(),
            [0., 0., 16., 8.],
        ));
        c.fill_rect(0., 0., 8., 8.);
        c.fill_rect(8., 0., 8., 8.);
        c.save();
        c.set_fill("lime");
        c.translate(0., 8.);
        c.restore();
        c.translate(0., 8.);
        c.fill_rect(0., 0., 16., 8.);
    };
    let mut cpu = RasterCanvas::new_linear(16, 16);
    paint(&mut cpu);
    assert!(pixel(&cpu, 7, 2)[0] > pixel(&cpu, 8, 2)[0]);
    assert!(pixel(&cpu, 7, 2)[2] < pixel(&cpu, 8, 2)[2]);
    assert_eq!(pixel(&cpu, 7, 2), pixel(&cpu, 7, 10));
    let mut gpu = GpuCanvas::new_linear(16, 16, None);
    paint(&mut gpu);
    assert_eq!(
        cpu.get_image_data(0, 0, 16, 16),
        gpu.get_image_data(0, 0, 16, 16)
    );
    assert!(!gpu.needs_cpu);
    let ops = gpu.into_ops();
    assert!(matches!(&ops[0], PathGpuOp::Fill(f) if f.grad.unwrap().bbox == [0.,0.,16.,8.]));
    let PathGpuOp::Fill(f) = &ops[2] else {
        panic!()
    };
    assert_eq!(f.grad.unwrap().device_to_brush, [1., 0., 0., 1., 0., -8.]);
}

#[test]
fn image_extension_has_independent_axes_and_negative_coordinates() {
    let modes = [
        ExtendMode::Pad,
        ExtendMode::Repeat,
        ExtendMode::Reflect,
        ExtendMode::Decal,
    ];
    let expected = [
        [0, 0, 0, 1, 1, 1, 1, 1],
        [0, 1, 0, 1, 0, 1, 0, 1],
        [1, 0, 0, 1, 1, 0, 0, 1],
        [-1, -1, 0, 1, -1, -1, -1, -1],
    ];
    for (mode, indices) in modes.into_iter().zip(expected) {
        let mut c = RasterCanvas::new_linear(8, 3);
        c.set_fill_brush(
            Brush2D::image(tile())
                .with_extend(mode, ExtendMode::Decal)
                .with_transform([1., 0., 0., 1., 2., 1.]),
        );
        c.fill_rect(0., 0., 8., 3.);
        for (x, i) in indices.into_iter().enumerate() {
            assert_eq!(
                pixel(&c, x as u32, 1),
                match i {
                    0 => [255, 0, 0, 255],
                    1 => [0, 0, 255, 255],
                    _ => [0; 4],
                },
                "{mode:?} x={x}"
            );
            assert_eq!(pixel(&c, x as u32, 0), [0; 4]);
            assert_eq!(pixel(&c, x as u32, 2), [0; 4]);
        }
    }
}

#[test]
fn image_bilinear_extends_taps_at_seams_and_filters_premultiplied_linear() {
    let mut c = RasterCanvas::new_linear(4, 1);
    c.set_fill_brush(
        Brush2D::image(tile())
            .with_sampling(ImageSampling::Bilinear)
            .with_transform([1., 0., 0., 1., 0.5, 0.]),
    );
    c.fill_rect(0., 0., 4., 1.);
    assert_eq!(pixel(&c, 0, 0), [188, 0, 188, 255]);
    assert_eq!(pixel(&c, 1, 0), [188, 0, 188, 255]);
    let transparent = Image2D::from_rgba8(2, 1, &[255, 0, 0, 255, 0, 0, 255, 0]).unwrap();
    c.clear("transparent");
    c.set_fill_brush(
        Brush2D::image(transparent)
            .with_sampling(ImageSampling::Bilinear)
            .with_transform([1., 0., 0., 1., 0.5, 0.]),
    );
    c.fill_rect(0., 0., 4., 1.);
    assert_eq!(pixel(&c, 0, 0), [255, 0, 0, 128]);
    assert_eq!(pixel(&c, 1, 0), [255, 0, 0, 128]);
}

#[test]
fn pattern_stroke_clip_mask_layer_and_restore_preserve_coordinates() {
    let paint = |c: &mut dyn Canvas2D| {
        c.set_stroke_brush(Brush2D::image(tile()).with_transform([2., 0., 0., 2., 0., 0.]));
        c.set_line_width(2.);
        c.translate(2., 2.);
        c.clip_rect(0., 0., 6., 6.);
        c.clip_image_mask(&Image2D::from_rgba8(8, 8, &[255; 256]).unwrap());
        c.push_layer(0.5, BlendMode::Normal);
        c.save();
        c.set_stroke("lime");
        c.restore();
        c.stroke_path(&polyline(&[(0., 2.), (8., 2.)]));
        c.pop_layer();
    };
    let mut c = RasterCanvas::new_linear(12, 12);
    paint(&mut c);
    assert_eq!(pixel(&c, 2, 4), [255, 0, 0, 128]);
    assert_eq!(pixel(&c, 4, 4), [0, 0, 255, 128]);
    assert_eq!(pixel(&c, 8, 4), [0; 4]);
    let mut g = GpuCanvas::new_linear(12, 12, None);
    paint(&mut g);
    assert!(!g.needs_cpu);
    assert_eq!(
        g.get_image_data(0, 0, 12, 12),
        c.get_image_data(0, 0, 12, 12)
    );
}

#[test]
fn singular_nonfinite_brushes_and_invalid_domains_paint_nothing() {
    for matrix in [
        [0.; 6],
        [1., 0., 0., 1., f32::NAN, 0.],
        [f32::INFINITY, 0., 0., 1., 0., 0.],
    ] {
        let mut c = GpuCanvas::new_linear(8, 8, None);
        c.set_fill_brush(Brush2D::image(tile()).with_transform(matrix));
        c.fill_rect(0., 0., 8., 8.);
        c.set_stroke_brush(Brush2D::image(tile()).with_transform(matrix));
        c.stroke_path(&polyline(&[(0., 4.), (8., 4.)]));
        assert!(c.into_ops().is_empty());
    }
    for rect in [[0., 0., 0., 8.], [0., 0., 8., -1.], [f32::NAN, 0., 8., 8.]] {
        let mut c = RasterCanvas::new_linear(8, 8);
        c.set_fill_brush(Brush2D::gradient(
            parse_gradient("linear-gradient(red,blue)").unwrap(),
            rect,
        ));
        c.fill_rect(0., 0., 8., 8.);
        assert_eq!(pixel(&c, 4, 4), [0; 4]);
    }
}

#[test]
fn unsupported_gradient_brush_software_continuation_preserves_draw_order() {
    let mut c = GpuCanvas::new_linear(8, 8, None);
    c.set_fill_brush(Brush2D::image(tile()));
    c.fill_rect(0., 0., 8., 8.);
    c.set_fill_brush(Brush2D::gradient(
        parse_gradient("linear-gradient(red,blue,red,blue,red,blue,red,blue,red)").unwrap(),
        [0., 0., 8., 8.],
    ));
    c.fill_rect(4., 0., 4., 8.);
    assert!(c.needs_cpu);
    assert_eq!(pixel(&c, 1, 4), [0, 0, 255, 255]);
    assert_ne!(pixel(&c, 5, 4), [0, 0, 255, 255]);
    c.set_fill("lime");
    c.fill_rect(0., 0., 2., 2.);
    assert_eq!(pixel(&c, 1, 1), [0, 255, 0, 255]);
}

#[test]
fn brush_transform_composes_with_canvas_transform_at_draw_time() {
    let mut c = RasterCanvas::new_linear(12, 12);
    c.set_fill_brush(Brush2D::image(tile()).with_transform([2., 0., 0., 2., 0., 0.]));
    c.set_transform([0., 1., -1., 0., 10., 2.]);
    c.fill_rect(0., 0., 8., 4.);
    assert_eq!(pixel(&c, 8, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&c, 8, 4), [0, 0, 255, 255]);
    assert_eq!(pixel(&c, 8, 6), [255, 0, 0, 255]);
    // Changing paint does not retroactively change earlier pixels.
    c.set_fill("lime");
    c.reset_transform();
    c.fill_rect(0., 0., 1., 1.);
    assert_eq!(pixel(&c, 8, 4), [0, 0, 255, 255]);
}
