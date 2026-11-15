use jadix_2d::*;
fn filtered(image: &Image2D, effect: LayerFilter, linear: bool) -> RasterCanvas {
    let mut c = if linear {
        RasterCanvas::new_linear(image.width(), image.height())
    } else {
        RasterCanvas::new(image.width(), image.height())
    };
    c.push_layer_with_options(LayerOptions {
        filters: vec![effect],
        ..Default::default()
    });
    c.draw_image(
        &CanvasImageSource::Image(image),
        0.,
        0.,
        image.width() as f32,
        image.height() as f32,
    );
    c.pop_layer();
    c
}
#[test]
fn transfers_use_straight_components_and_explicit_color_space() {
    let im = Image2D::from_rgba8(1, 1, &[128, 64, 255, 128]).unwrap();
    let channels = [
        TransferFunction::Linear {
            slope: -1.,
            intercept: 1.,
        },
        TransferFunction::Gamma {
            amplitude: 1.,
            exponent: 2.,
            offset: 0.,
        },
        TransferFunction::table(&[0., 0.2, 0.8]).unwrap(),
        TransferFunction::discrete(&[0.25, 0.75]).unwrap(),
    ];
    for linear in [false, true] {
        let c = filtered(
            &im,
            LayerFilter::ComponentTransfer {
                channels: channels.clone(),
                color_space: FilterColorSpace::Srgb,
            },
            linear,
        );
        let actual = c.get_image_data(0, 0, 1, 1).unwrap();
        let expected = [127u8, 16, 204, 191];
        assert!(
            actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
            "{actual:?}"
        );
    }
    let srgb = filtered(
        &im,
        LayerFilter::ComponentTransfer {
            channels: [
                TransferFunction::Linear {
                    slope: 0.5,
                    intercept: 0.,
                },
                TransferFunction::Identity,
                TransferFunction::Identity,
                TransferFunction::Identity,
            ],
            color_space: FilterColorSpace::Srgb,
        },
        true,
    );
    let linear = filtered(
        &im,
        LayerFilter::ComponentTransfer {
            channels: [
                TransferFunction::Linear {
                    slope: 0.5,
                    intercept: 0.,
                },
                TransferFunction::Identity,
                TransferFunction::Identity,
                TransferFunction::Identity,
            ],
            color_space: FilterColorSpace::LinearSrgb,
        },
        true,
    );
    assert_eq!(srgb.get_image_data(0, 0, 1, 1).unwrap()[0], 64);
    assert!(linear.get_image_data(0, 0, 1, 1).unwrap()[0] > 90);
}
#[test]
fn convolution_orientation_edges_alpha_and_fractional_units_have_independent_oracles() {
    let rgba = [64, 0, 0, 128, 128, 0, 0, 192, 255, 0, 0, 255];
    let im = Image2D::from_rgba8(3, 1, &rgba).unwrap();
    for (edge, red, alpha) in [
        (FilterEdge::Transparent, 0, 0),
        (FilterEdge::Duplicate, 255, 255),
        (FilterEdge::Wrap, 64, 128),
    ] {
        let mut conv = Convolution::new([3, 1], &[1., 0., 0.]).unwrap();
        conv.edge = edge;
        conv.color_space = FilterColorSpace::Srgb;
        let c = filtered(&im, LayerFilter::ConvolveMatrix(conv), true);
        assert_eq!(
            &c.get_image_data(0, 0, 3, 1).unwrap()[8..12],
            &[red, 0, 0, alpha],
            "{edge:?}"
        );
    }
    let mut conv = Convolution::new([3, 1], &[1., 0., 0.]).unwrap();
    conv.preserve_alpha = true;
    conv.color_space = FilterColorSpace::Srgb;
    let c = filtered(&im, LayerFilter::ConvolveMatrix(conv.clone()), true);
    assert_eq!(&c.get_image_data(1, 0, 1, 1).unwrap(), &[255, 0, 0, 192]);
    conv.unit_length = [0.5, 1.];
    let c = filtered(&im, LayerFilter::ConvolveMatrix(conv), true);
    assert!(c.get_image_data(0, 0, 1, 1).unwrap()[0].abs_diff(96) <= 1);
    let mut identity = Convolution::new([1, 1], &[2.]).unwrap();
    identity.bias = 0.1;
    identity.preserve_alpha = true;
    identity.color_space = FilterColorSpace::Srgb;
    let c = filtered(&im, LayerFilter::ConvolveMatrix(identity), true);
    assert!(c.get_image_data(0, 0, 1, 1).unwrap()[0].abs_diff(90) <= 1);
}
#[test]
fn empty_groups_invalid_filters_and_scene_continuation() {
    let effect = LayerFilter::ComponentTransfer {
        channels: [
            TransferFunction::Linear {
                slope: 0.,
                intercept: 1.,
            },
            TransferFunction::Identity,
            TransferFunction::Identity,
            TransferFunction::Linear {
                slope: 0.,
                intercept: 0.5,
            },
        ],
        color_space: FilterColorSpace::LinearSrgb,
    };
    let scene = Scene2D::record([8, 8], |c| {
        c.push_layer_with_options(LayerOptions {
            filters: vec![effect.clone()],
            ..Default::default()
        });
        c.pop_layer();
    })
    .unwrap();
    let mut cpu = RasterCanvas::new_linear(8, 8);
    scene.draw(&mut cpu, Default::default()).unwrap();
    assert_eq!(cpu.get_image_data(2, 2, 1, 1).unwrap(), [255, 0, 0, 128]);
    let mut gpu = GpuCanvas::new_linear(8, 8, None);
    scene.draw(&mut gpu, Default::default()).unwrap();
    assert!(!gpu.needs_cpu);
    assert_eq!(
        gpu.get_image_data(0, 0, 8, 8),
        cpu.get_image_data(0, 0, 8, 8)
    );
    gpu.put_image_data(0, 0, 1, 1, &[255; 4]);
    cpu.put_image_data(0, 0, 1, 1, &[255; 4]);
    assert_eq!(gpu.into_raster(), cpu.into_pixels());
    assert!(TransferFunction::table(&[]).is_err());
    assert!(TransferFunction::table(&[0.; 17]).is_err());
    assert!(Convolution::new([8, 1], &[1.; 8]).is_err());
    let im = Image2D::from_rgba8(1, 1, &[255, 0, 0, 128]).unwrap();
    let mut c = Convolution::new([1, 1], &[1.]).unwrap();
    c.divisor = f32::NAN;
    let out = filtered(&im, LayerFilter::ConvolveMatrix(c), true);
    assert_eq!(out.get_image_data(0, 0, 1, 1).unwrap(), [255, 0, 0, 128]);
}
