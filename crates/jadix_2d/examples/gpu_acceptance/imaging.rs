use super::scene_tests::{error, texture};
use super::*;

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn polar_gradient_hue_alpha_geometry_and_achromatic_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let target = texture(&device, 48);
    let view = target.create_view(&Default::default());
    let mut rows = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        for space in [GradientSpace::Oklch, GradientSpace::Hsl] {
            for hue in [
                HueDirection::Shorter,
                HueDirection::Longer,
                HueDirection::Increasing,
                HueDirection::Decreasing,
            ] {
                for alpha in [GradientAlpha::Premultiplied, GradientAlpha::Straight] {
                    for extend in [
                        ExtendMode::Pad,
                        ExtendMode::Repeat,
                        ExtendMode::Reflect,
                        ExtendMode::Decal,
                    ] {
                        for geometry in 0..3 {
                            for variant in 0..3 {
                                let colors = match variant {
                                    0 => [[1., 0., 0., 1.], [0., 0., 1., 1.]],
                                    1 => [[0.5, 0.5, 0.5, 0.4], [0., 0.2, 1., 0.9]],
                                    _ => [[1., 0., 0., 0.], [0., 1., 0., 1.]],
                                };
                                let stops =
                                    GradientStops::new(&[(0., colors[0]), (1., colors[1])])?
                                        .with_midpoints(&[Some(0.4)])?;
                                let g = match geometry {
                                    0 => VectorGradient::linear([8., 6.], [38., 40.], stops)?,
                                    1 => VectorGradient::radial(
                                        [20., 20.],
                                        2.,
                                        [24., 24.],
                                        20.,
                                        stops,
                                    )?,
                                    _ => {
                                        VectorGradient::sweep_range([24., 24.], -80., 420., stops)?
                                    }
                                }
                                .with_color_space(space)
                                .with_hue_direction(hue)
                                .with_alpha_mode(alpha)
                                .with_extend(extend);
                                let paint = |c: &mut dyn Canvas2D| {
                                    c.set_fill_brush(Brush2D::vector(g.clone()));
                                    c.fill_rect(0., 0., 48., 48.);
                                };
                                let mut cpu = RasterCanvas::new_linear(48, 48);
                                paint(&mut cpu);
                                let expected = cpu.get_image_data(0, 0, 48, 48).unwrap();
                                let mut gpu = GpuCanvas::new_linear(48, 48, None);
                                paint(&mut gpu);
                                let ops = gpu.into_ops();
                                assert!(renderer.render(&device, &queue, &view, [48, 48], &ops));
                                let actual = readback(&device, &queue, &target, 48)?;
                                let (mae, ink) = error(&actual, &expected);
                                assert!(mae<0.7&&ink<2.,"{space:?}/{hue:?}/{alpha:?}/{extend:?}/{geometry}/{variant}/{samples}: {mae}/{ink}");
                                rows.push(json!({"space":format!("{space:?}"),"hue":format!("{hue:?}"),"alpha":format!("{alpha:?}"),"extend":format!("{extend:?}"),"geometry":geometry,"variant":variant,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":ink}));
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("polar-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}

fn effect(index: usize, space: FilterColorSpace, edge: FilterEdge, preserve: bool) -> LayerFilter {
    match index {
        0 => LayerFilter::ComponentTransfer {
            channels: [
                TransferFunction::Linear {
                    slope: -1.,
                    intercept: 1.,
                },
                TransferFunction::Gamma {
                    amplitude: 0.9,
                    exponent: 0.6,
                    offset: 0.05,
                },
                TransferFunction::table(&[0., 0.8, 0.2, 1.]).unwrap(),
                TransferFunction::Linear {
                    slope: 0.7,
                    intercept: 0.1,
                },
            ],
            color_space: space,
        },
        1 => LayerFilter::ComponentTransfer {
            channels: [
                TransferFunction::discrete(&[0., 0.2, 0.8, 1.]).unwrap(),
                TransferFunction::Identity,
                TransferFunction::table(&[0.3]).unwrap(),
                TransferFunction::Gamma {
                    amplitude: 0.75,
                    exponent: 0.,
                    offset: 0.,
                },
            ],
            color_space: space,
        },
        _ => {
            let (order, kernel) = match index {
                2 => ([3, 3], vec![0., -1., 0., -1., 5., -1., 0., -1., 0.]),
                3 => ([3, 3], vec![-2., -1., 0., -1., 1., 1., 0., 1., 2.]),
                4 => ([3, 1], vec![1., 0., 0.]),
                _ => ([7, 7], vec![1.; 49]),
            };
            let mut c = Convolution::new(order, &kernel).unwrap();
            c.edge = edge;
            c.preserve_alpha = preserve;
            c.color_space = space;
            if index == 3 {
                c.bias = 0.12;
            }
            if index == 4 {
                c.target = [0, 0];
                c.unit_length = [1.5, 0.5];
            }
            LayerFilter::ConvolveMatrix(c)
        }
    }
}
fn paint(c: &mut dyn Canvas2D, filters: Vec<LayerFilter>, variant: usize, image: &Image2D) {
    if variant == 1 {
        c.set_fill("#20304080");
        c.fill_rect(0., 0., 64., 64.);
    }
    c.push_layer_with_options(LayerOptions {
        filters,
        opacity: 0.8,
        blend: if variant == 1 {
            BlendMode::Screen
        } else {
            BlendMode::Normal
        },
        clip: if variant == 1 {
            Some([2.5, 3., 58., 56.])
        } else {
            None
        },
        ..Default::default()
    });
    if variant != 2 {
        c.set_image_sampling(ImageSampling::Nearest);
        c.draw_image(&CanvasImageSource::Image(image), 0., 0., 64., 64.);
        c.push_layer(0.8, BlendMode::Normal);
        c.set_fill("#ffffff80");
        c.fill_rect(16., 24., 32., 16.);
        c.pop_layer();
    }
    c.pop_layer();
}
#[test]
#[ignore = "requires a native headless GPU adapter"]
fn component_transfer_convolution_color_space_and_edge_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let data = (0..64)
        .flat_map(|y| {
            (0..64).flat_map(move |x| {
                [
                    ((x / 8) * 30 + 15) as u8,
                    ((y / 8) * 30 + 10) as u8,
                    ((x + y) % 180 + 20) as u8,
                    if x % 16 < 8 { 128 } else { 224 },
                ]
            })
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_rgba8(64, 64, &data).unwrap();
    let target = texture(&device, 64);
    let view = target.create_view(&Default::default());
    let mut rows = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        for space in [FilterColorSpace::LinearSrgb, FilterColorSpace::Srgb] {
            for edge in [
                FilterEdge::Transparent,
                FilterEdge::Duplicate,
                FilterEdge::Wrap,
            ] {
                for preserve in [false, true] {
                    for index in 0..6 {
                        for variant in 0..3 {
                            let mut filters = vec![effect(index, space, edge, preserve)];
                            if variant == 1 {
                                filters.push(LayerFilter::Gaussian { sigma: [0.4, 0.8] });
                                filters.push(LayerFilter::Offset {
                                    offset: [0.5, -0.25],
                                });
                            }
                            let mut cpu = RasterCanvas::new_linear(64, 64);
                            paint(&mut cpu, filters.clone(), variant, &image);
                            let expected = cpu.get_image_data(0, 0, 64, 64).unwrap();
                            let mut gpu = GpuCanvas::new_linear(64, 64, None);
                            paint(&mut gpu, filters, variant, &image);
                            assert!(!gpu.needs_cpu);
                            assert_eq!(gpu.get_image_data(0, 0, 64, 64).unwrap(), expected);
                            let ops = gpu.into_ops();
                            assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                            let actual = readback(&device, &queue, &target, 64)?;
                            let (mae, alpha) = error(&actual, &expected);
                            assert!(mae<1. && alpha<2.,"{space:?}/{edge:?}/{preserve}/{index}/{variant}/{samples}: {mae}/{alpha}");
                            assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                            let s = renderer.last_stats();
                            assert_eq!(
                                (
                                    s.buffer_allocations,
                                    s.texture_allocations,
                                    s.bind_group_allocations,
                                    s.image_uploads
                                ),
                                (0, 0, 0, 0)
                            );
                            rows.push(json!({"samples":samples,"space":format!("{space:?}"),"edge":format!("{edge:?}"),"preserve_alpha":preserve,"effect":index,"variant":variant,"premultiplied_mae":mae,"ink_alpha_mae":alpha,"warm_gpu_allocations":0}));
                            if samples == 4
                                && space == FilterColorSpace::Srgb
                                && edge == FilterEdge::Duplicate
                                && !preserve
                                && variant == 0
                            {
                                if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
                                    std::fs::create_dir_all(&dir)?;
                                    image::save_buffer(
                                        Path::new(&dir).join(format!("imaging-{index}.png")),
                                        &actual,
                                        64,
                                        64,
                                        image::ColorType::Rgba8,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("imaging-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}

#[cfg(feature = "text-layout")]
#[test]
#[ignore = "requires a native headless GPU adapter"]
fn shaped_axis_instances_and_filter_chains_matrix() -> Result<(), Box<dyn Error>> {
    use jadix_2d::text_layout::cosmic_text::{self as cosmic, *};
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    for data in [
        &include_bytes!("../../tests/fonts/axes-mini.ttf")[..],
        &include_bytes!("../../tests/fonts/Nabla.ttf")[..],
    ] {
        fonts.db_mut().load_font_data(data.to_vec());
    }
    fonts.db_mut().set_sans_serif_family("Jadix Axes Fixture");
    let mut cache = SwashCache::new();
    let target = texture(&device, 128);
    let view = target.create_view(&Default::default());
    let mut rows = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
            for width in [50., 100., 200.] {
                for depth in [0., 200.] {
                    for transformed in [false, true] {
                        let mut b = Buffer::new(&mut fonts, Metrics::new(24., 36.));
                        b.set_size(Some(120.), Some(120.));
                        let attrs = Attrs::new();
                        b.set_rich_text(
                            [
                                (
                                    "AA fi\n",
                                    Attrs::new()
                                        .family(Family::Name("Jadix Axes Fixture"))
                                        .font_variations(
                                            FontVariations::new(&[
                                                (*b"wdth", width),
                                                (*b"wght", 700.),
                                            ])
                                            .unwrap(),
                                        ),
                                ),
                                (
                                    "AB12",
                                    Attrs::new().family(Family::Name("Nabla")).font_variations(
                                        FontVariations::new(&[(*b"EDPT", depth), (*b"EHLT", 24.)])
                                            .unwrap(),
                                    ),
                                ),
                            ],
                            &attrs,
                            Shaping::Advanced,
                            None,
                        );
                        let text = PreparedText2D::new(
                            &mut fonts,
                            &mut b,
                            &mut cache,
                            TextPrepareOptions {
                                mode,
                                raster_scale: 2.,
                                ..Default::default()
                            },
                        )?;
                        let scene =
                            Scene2D::record([128, 128], |c| text.try_draw(c, [6., 6.]).unwrap())?;
                        let paint = |c: &mut dyn Canvas2D| {
                            c.save();
                            if transformed {
                                c.translate(12., 4.);
                                c.rotate(5.);
                                c.scale(0.9, 0.9);
                            }
                            scene
                                .draw(
                                    c,
                                    SceneOptions {
                                        layer: Some(LayerOptions {
                                            filters: vec![
                                                effect(
                                                    0,
                                                    FilterColorSpace::Srgb,
                                                    FilterEdge::Duplicate,
                                                    false,
                                                ),
                                                LayerFilter::DropShadow {
                                                    sigma: [0.6, 0.8],
                                                    offset: [1., 1.],
                                                    color: [0., 0., 1., 0.6],
                                                },
                                            ],
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    },
                                )
                                .unwrap();
                            c.restore();
                        };
                        let mut cpu = RasterCanvas::new_linear(128, 128);
                        paint(&mut cpu);
                        let expected = cpu.get_image_data(0, 0, 128, 128).unwrap();
                        let mut gpu = GpuCanvas::new_linear(128, 128, None);
                        paint(&mut gpu);
                        assert!(!gpu.needs_cpu);
                        assert_eq!(gpu.get_image_data(0, 0, 128, 128).unwrap(), expected);
                        let ops = gpu.into_ops();
                        assert!(renderer.render(&device, &queue, &view, [128, 128], &ops));
                        let actual = readback(&device, &queue, &target, 128)?;
                        let (mae, alpha) = error(&actual, &expected);
                        assert!(
                            mae < 1. && alpha < 3.,
                            "{mode:?}/{width}/{depth}/{transformed}/{samples}: {mae}/{alpha}"
                        );
                        rows.push(json!({"mode":format!("{mode:?}"),"width":width,"depth":depth,"transformed":transformed,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha}));
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("axis-layout-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}

#[cfg(feature = "text-layout")]
#[test]
#[ignore = "requires a native headless GPU adapter"]
fn animated_axes_and_transfer_tables_do_not_reuse_stale_pixels() -> Result<(), Box<dyn Error>> {
    use jadix_2d::text_layout::cosmic_text::{self as cosmic, *};
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    fonts
        .db_mut()
        .load_font_data(include_bytes!("../../tests/fonts/axes-mini.ttf").to_vec());
    fonts.db_mut().set_sans_serif_family("Jadix Axes Fixture");
    let mut b = Buffer::new(&mut fonts, Metrics::new(20., 24.));
    b.set_size(Some(128.), Some(128.));
    let mut cache = SwashCache::new();
    let target = texture(&device, 128);
    let view = target.create_view(&Default::default());
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Reference);
    let mut max_mae = 0f64;
    let mut previous = None;
    for frame in 0..60 {
        let width = 50. + (frame % 5) as f32 * 37.5;
        b.set_text(
            "AA fi",
            &Attrs::new()
                .family(Family::Name("Jadix Axes Fixture"))
                .font_variations(
                    FontVariations::new(&[(*b"wdth", width), (*b"wght", 750.)]).unwrap(),
                ),
            Shaping::Advanced,
            None,
        );
        let prepared = PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions::default(),
        )?;
        let child = Scene2D::record([128, 128], |c| prepared.try_draw(c, [0., 0.]).unwrap())?;
        let mut convolution = Convolution::new([3, 1], &[0.2, 0.6, 0.2])?;
        convolution.color_space = FilterColorSpace::Srgb;
        let filters = vec![
            LayerFilter::ComponentTransfer {
                channels: [
                    TransferFunction::table(&[0.1, 0.3 + (frame % 5) as f32 * 0.15, 1.])?,
                    TransferFunction::Linear {
                        slope: 0.,
                        intercept: 0.25,
                    },
                    TransferFunction::Identity,
                    TransferFunction::Identity,
                ],
                color_space: FilterColorSpace::Srgb,
            },
            LayerFilter::ConvolveMatrix(convolution),
        ];
        let paint = |c: &mut dyn Canvas2D| {
            c.push_layer_with_options(LayerOptions {
                filters: filters.clone(),
                ..Default::default()
            });
            for row in 0..4 {
                child
                    .draw(
                        c,
                        SceneOptions {
                            transform: [1., 0., 0., 1., 4., row as f32 * 26.],
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
            c.pop_layer();
        };
        let mut gpu = GpuCanvas::new_linear(128, 128, None);
        paint(&mut gpu);
        assert!(!gpu.needs_cpu);
        let ops = gpu.into_ops();
        assert!(renderer.render(&device, &queue, &view, [128, 128], &ops));
        let actual = readback(&device, &queue, &target, 128)?;
        if let Some(old) = previous {
            assert_ne!(old, actual, "axis/table animation must alter pixels");
        }
        previous = Some(actual.clone());
        if frame % 5 == 0 {
            let mut cpu = RasterCanvas::new_linear(128, 128);
            paint(&mut cpu);
            let (mae, alpha) = error(&actual, &cpu.get_image_data(0, 0, 128, 128).unwrap());
            max_mae = max_mae.max(mae);
            assert!(mae < 1. && alpha < 2., "frame {frame}: {mae}/{alpha}");
        }
        if frame >= 5 {
            let s = renderer.last_stats();
            assert_eq!(
                (
                    s.buffer_allocations,
                    s.texture_allocations,
                    s.bind_group_allocations,
                    s.image_uploads
                ),
                (0, 0, 0, 0)
            );
            assert_eq!(s.queue_submissions, 1);
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("imaging-pressure.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"frames":60,"unique_axis_presets":5,"instances_per_frame":4,"warmup":5,"warm_gpu_allocations":0,"image_uploads":0,"submissions_per_frame":1,"worst_premultiplied_mae":max_mae,"scope":"No CPU RSS/VRAM or elapsed-time benchmark"}),
            )?,
        )?;
    }
    Ok(())
}
