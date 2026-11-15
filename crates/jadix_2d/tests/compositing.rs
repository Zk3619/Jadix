use jadix_2d::*;

#[test]
fn gradient_strokes_preserve_dash_transform_state_and_software_continuation() {
    let gradient = gradient::parse_gradient("linear-gradient(to right,#ff0000,#0000ff)").unwrap();
    let mut c = GpuCanvas::new_linear(32, 16, None);
    c.set_line_width(4.);
    c.set_line_cap(LineCap::Butt);
    c.set_line_dash(&[4., 4.]);
    c.set_global_alpha(0.5);
    c.set_stroke_gradient(gradient.clone());
    c.save();
    c.set_stroke("#00ff00");
    c.restore();
    c.scale(2., 1.);
    c.stroke_path(&polyline(&[(0., 8.), (16., 8.)]));
    assert!(!c.needs_cpu);
    let red = c.get_image_data(2, 8, 1, 1).unwrap();
    assert!(red[0] > red[2] && red[3] == 128);
    assert_eq!(c.get_image_data(10, 8, 1, 1).unwrap(), [0; 4]);
    let before = c.get_image_data(0, 0, 32, 16).unwrap();
    c.put_image_data(0, 0, 1, 1, &[0; 4]);
    assert!(c.needs_cpu);
    assert_eq!(c.get_image_data(0, 0, 32, 16).unwrap(), before);
    // More than eight stops follow the existing software gradient path.
    let mut c = GpuCanvas::new_linear(32, 16, None);
    c.set_stroke_gradient(
        gradient::parse_gradient("linear-gradient(red,blue,red,blue,red,blue,red,blue,red)")
            .unwrap(),
    );
    c.stroke_path(&polyline(&[(1., 8.), (31., 8.)]));
    assert!(c.needs_cpu);
}

fn pixel(c: &dyn Canvas2D) -> [u8; 4] {
    c.get_image_data(2, 2, 1, 1).unwrap().try_into().unwrap()
}

#[test]
fn linear_canvas_matches_gpu_working_space_and_preserves_legacy_constructor() {
    for (mut c, expected) in [
        (
            Box::new(RasterCanvas::new(8, 8)) as Box<dyn Canvas2D>,
            [128, 0, 128, 255],
        ),
        (Box::new(RasterCanvas::new_linear(8, 8)), [188, 0, 188, 255]),
        (
            Box::new(GpuCanvas::new_linear(8, 8, None)),
            [188, 0, 188, 255],
        ),
    ] {
        c.clear("#0000ff");
        c.set_fill("#ff000080");
        c.fill_rect(0., 0., 8., 8.);
        let actual = pixel(&*c);
        assert!(
            actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
            "{actual:?}"
        );
    }
}

#[test]
fn linear_pixel_io_and_premultiplied_export_keep_display_srgb_contract() {
    let mut c = RasterCanvas::new_linear(8, 8);
    c.put_image_data(2, 2, 1, 1, &[117, 42, 223, 73]);
    assert_eq!(pixel(&c), [117, 42, 223, 73]);
    let out = premul_to_straight_rgba(&c.into_pixels(), 8, 8);
    assert_eq!(&out[(2 * 8 + 2) * 4..][..4], &[117, 42, 223, 73]);
    let mut c = GpuCanvas::new_linear(8, 8, None);
    c.clear("#0000ff");
    c.set_fill("#ff000080");
    c.fill_rect(0., 0., 8., 8.);
    c.put_image_data(0, 0, 1, 1, &[0, 0, 0, 255]);
    assert!(c.needs_cpu);
    let out = premul_to_straight_rgba(&c.into_raster(), 8, 8);
    assert_eq!(&out[(2 * 8 + 2) * 4..][..4], &[188, 0, 187, 255]);
}

#[test]
fn linear_images_integrate_fractional_edges_before_compositing() {
    let image = Image2D::from_rgba8(1, 1, &[255, 0, 0, 128]).unwrap();
    let mut c = RasterCanvas::new_linear(4, 4);
    c.draw_image(&CanvasImageSource::Image(&image), 0.5, 0.5, 1., 1.);
    let pixels = c.get_image_data(0, 0, 2, 2).unwrap();
    for p in pixels.chunks_exact(4) {
        assert_eq!(p, [255, 0, 0, 32]);
    }
    // Four quarter-covered pixels retain the original half-alpha unit area.
    assert_eq!(
        pixels.chunks_exact(4).map(|p| u32::from(p[3])).sum::<u32>(),
        128
    );
}

#[test]
fn linear_continuation_covers_gradients_images_masks_and_nested_groups() {
    let image = Image2D::from_rgba8(1, 1, &[255, 0, 0, 128]).unwrap();
    let mask = Image2D::from_rgba8(8, 8, &[255; 8 * 8 * 4]).unwrap();
    let paint = |c: &mut dyn Canvas2D| {
        c.clear("#0000ff");
        c.clip_image_mask(&mask);
        c.push_layer(0.6, BlendMode::Screen);
        c.push_layer(0.7, BlendMode::Multiply);
        c.set_fill_gradient(gradient::parse_gradient("linear-gradient(#102030,#c08040)").unwrap());
        c.fill_rect(0., 0., 8., 8.);
        c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 8., 8.);
        c.pop_layer();
        c.pop_layer();
    };
    let mut cpu = RasterCanvas::new_linear(8, 8);
    paint(&mut cpu);
    let mut gpu = GpuCanvas::new_linear(8, 8, None);
    paint(&mut gpu);
    assert_eq!(pixel(&cpu), pixel(&gpu));
    gpu.put_image_data(0, 0, 1, 1, &[0; 4]);
    assert_eq!(pixel(&cpu), pixel(&gpu));
}

#[test]
fn porter_duff_matches_independent_region_weights_including_empty_groups() {
    use CompositeMode::*;
    let modes = [
        Clear,
        Copy,
        Destination,
        SourceOver,
        DestinationOver,
        SourceIn,
        DestinationIn,
        SourceOut,
        DestinationOut,
        SourceAtop,
        DestinationAtop,
        Xor,
        Plus,
        PlusLighter,
        LuminanceIn,
        LuminanceOut,
    ];
    for mode in modes {
        for empty in [false, true] {
            let mut c = RasterCanvas::new_linear(4, 4);
            c.clear("#0000ff80");
            c.push_layer_with_options(LayerOptions {
                opacity: 0.5,
                compose: mode,
                ..Default::default()
            });
            if !empty {
                c.set_fill("#ff000080");
                c.fill_rect(0., 0., 4., 4.);
            }
            c.pop_layer();
            let (s, d): (f32, f32) = (if empty { 0. } else { 128. / 255. * 0.5 }, 128. / 255.);
            let only_s = s * (1. - d);
            let only_d = d * (1. - s);
            let overlap = s * d;
            // Independently select the source-only, destination-only and overlap regions.
            let (red, blue) = match mode {
                Clear => (0., 0.),
                Copy => (s, 0.),
                Destination => (0., d),
                SourceOver => (only_s + overlap, only_d),
                DestinationOver => (only_s, only_d + overlap),
                SourceIn => (overlap, 0.),
                DestinationIn => (0., overlap),
                SourceOut => (only_s, 0.),
                DestinationOut => (0., only_d),
                SourceAtop => (overlap, only_d),
                DestinationAtop => (only_s, overlap),
                Xor => (only_s, only_d),
                Plus | PlusLighter => (s, d),
                LuminanceIn => (0., d * s * 0.2126),
                LuminanceOut => (0., d * (1. - s * 0.2126)),
            };
            let alpha: f32 = (red + blue).min(1.);
            let expected = [red, 0., blue, alpha];
            assert!(
                c.buf[..4]
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() < 1e-6),
                "{mode:?} empty={empty}: {:?}",
                &c.buf[..4]
            );
        }
    }
}

#[test]
fn destructive_composition_is_bounded_by_final_clip_and_mask() {
    let mut mask = vec![0; 4 * 4 * 4];
    for p in mask.chunks_exact_mut(4) {
        p[3] = 128;
    }
    let mask = Image2D::from_rgba8(4, 4, &mask).unwrap();
    let options = LayerOptions {
        compose: CompositeMode::Clear,
        clip: Some([1.5, 0., 1., 4.]),
        mask: Some(mask),
        ..Default::default()
    };
    let mut cpu = RasterCanvas::new_linear(4, 4);
    cpu.clear("#8040c0");
    cpu.push_layer_with_options(options.clone());
    cpu.pop_layer();
    let a = cpu.get_image_data(0, 0, 4, 1).unwrap();
    assert_eq!(a[3], 255);
    assert_eq!(a[15], 255);
    assert_eq!(a[7], 191);
    assert_eq!(a[11], 191);
    let mut gpu = GpuCanvas::new_linear(4, 4, None);
    gpu.clear("#8040c0");
    gpu.push_layer_with_options(options);
    gpu.pop_layer();
    assert_eq!(gpu.get_image_data(0, 0, 4, 1).unwrap(), a);
    assert!(!gpu.needs_cpu);
    assert!(
        !gpu.into_ops()[1].is_empty(),
        "empty destructive layers must survive pruning"
    );
}

#[test]
fn advanced_layers_survive_active_software_continuation_and_invalid_bounds() {
    let options = LayerOptions {
        compose: CompositeMode::DestinationOut,
        blend: BlendMode::Hue,
        clip: Some([1., 1., 5., 5.]),
        opacity: 0.7,
        ..Default::default()
    };
    let paint = |c: &mut dyn Canvas2D| {
        c.clear("#204080");
        c.push_layer_with_options(options.clone());
        c.set_fill("#c08040a0");
        c.fill_rect(0., 0., 8., 8.);
    };
    let mut cpu = RasterCanvas::new_linear(8, 8);
    paint(&mut cpu);
    let mut gpu = GpuCanvas::new_linear(8, 8, None);
    paint(&mut gpu);
    for c in [&mut cpu as &mut dyn Canvas2D, &mut gpu] {
        c.put_image_data(2, 2, 1, 1, &[255, 0, 0, 255]);
        c.pop_layer();
        // Invalid bounds must not turn a destructive empty group into a full-canvas clear.
        c.push_layer_with_options(LayerOptions {
            compose: CompositeMode::Clear,
            clip: Some([f32::NAN, 0., 8., 8.]),
            ..Default::default()
        });
        c.pop_layer();
    }
    assert!(gpu.needs_cpu);
    assert_eq!(
        cpu.get_image_data(0, 0, 8, 8),
        gpu.get_image_data(0, 0, 8, 8)
    );
    assert_eq!(cpu.get_image_data(0, 0, 1, 1).unwrap(), [32, 64, 128, 255]);
}

#[test]
fn complete_mix_set_matches_w3c_midtone_and_singular_cases() {
    let source = [0.8f32, 0.2, 0.6];
    let backdrop = [0.2f32, 0.6, 0.8];
    for (mode, expected) in [
        (BlendMode::ColorDodge, [1., 0.75, 1.]),
        (BlendMode::ColorBurn, [0., 0., 2. / 3.]),
        (BlendMode::HardLight, [0.68, 0.24, 0.84]),
        (BlendMode::SoftLight, [0.3488, 0.456, 0.81888545]),
        (BlendMode::Hue, [0.878, 0.278, 0.678]),
        (BlendMode::Saturation, [0.2, 0.6, 0.8]),
        (BlendMode::Color, [0.878, 0.278, 0.678]),
        (BlendMode::Luminosity, [0.122, 0.522, 0.722]),
    ] {
        // Inject exact working-space values, avoiding byte/color-conversion quantization.
        let mut c = RasterCanvas::new_linear(2, 2);
        for p in c.buf.chunks_exact_mut(4) {
            p.copy_from_slice(&[backdrop[0], backdrop[1], backdrop[2], 1.]);
        }
        c.push_layer(1., mode);
        for p in c.buf.chunks_exact_mut(4) {
            p.copy_from_slice(&[source[0], source[1], source[2], 1.]);
        }
        c.pop_layer();
        assert!(
            c.buf[..3]
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-5),
            "{mode:?}: {:?}",
            &c.buf[..3]
        );
    }
    for mode in [
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ] {
        for (back, src) in [
            ("#000000", "#ffffff"),
            ("#ffffff", "#000000"),
            ("#808080", "#808080"),
        ] {
            let mut c = RasterCanvas::new_linear(2, 2);
            c.clear(back);
            c.push_layer(1., mode);
            c.clear(src);
            c.pop_layer();
            assert!(c
                .buf
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v)));
        }
    }
}
