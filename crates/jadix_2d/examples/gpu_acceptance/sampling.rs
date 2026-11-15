use super::*;
#[test]
#[ignore = "requires native GPU"]
fn mipmapped_sampling_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mips"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let p = (0..127 * 65)
        .flat_map(|i| {
            [
                if i % 2 == 0 { 2. } else { 0. },
                if i / 127 < 32 { -0.25 } else { 0.5 },
                1.2,
                (i % 3) as f32 * 0.5,
            ]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(127, 65, &p)
        .unwrap()
        .with_mipmaps(Default::default())?;
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    let mut rows = Vec::new();
    let transform = [0.13, 0.07, -0.11, 0.4, 3., 4.];
    let det = transform[0] * transform[3] - transform[1] * transform[2];
    let m = [
        transform[3] / det,
        -transform[1] / det,
        -transform[2] / det,
        transform[0] / det,
    ];
    for aa in [
        gpu::AaQuality::Area,
        gpu::AaQuality::Balanced,
        gpu::AaQuality::Reference,
    ] {
        renderer.set_aa_quality(aa);
        for quality in [
            ImageSampling::Trilinear,
            ImageSampling::Anisotropic,
            ImageSampling::Ewa,
        ] {
            for route in 0..2 {
                let mut c = GpuCanvas::new_linear(32, 32, None);
                c.set_image_sampling(quality);
                if route == 0 {
                    c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 32., 32.);
                } else {
                    c.set_fill_brush(
                        Brush2D::image(image.clone())
                            .with_sampling(quality)
                            .with_transform(transform)
                            .with_extend(ExtendMode::Repeat, ExtendMode::Reflect),
                    );
                    c.fill_rect(0., 0., 32., 32.);
                }
                let ops = c.into_ops();
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [32, 32],
                    &ops
                ));
                let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
                let mut max = 0f32;
                let mut worst = String::new();
                let samples = match aa {
                    gpu::AaQuality::Area => 1,
                    gpu::AaQuality::Reference => 4,
                    _ => 2,
                };
                for y in 0..32 {
                    for x in 0..32 {
                        let mut expected = [0.; 4];
                        for sy in 0..samples {
                            for sx in 0..samples {
                                let xx = x as f32 + (sx as f32 + 0.5) / samples as f32;
                                let yy = y as f32 + (sy as f32 + 0.5) / samples as f32;
                                let value = if route == 0 {
                                    image.sample_linear_with_footprint(
                                        [xx * 127. / 32., yy * 65. / 32.],
                                        [127. / 32., 0.],
                                        [0., 65. / 32.],
                                        quality,
                                        ExtendMode::Pad,
                                        ExtendMode::Pad,
                                    )
                                } else {
                                    image.sample_linear_with_footprint(
                                        [
                                            m[0] * (xx - 3.) + m[2] * (yy - 4.),
                                            m[1] * (xx - 3.) + m[3] * (yy - 4.),
                                        ],
                                        [m[0], m[1]],
                                        [m[2], m[3]],
                                        quality,
                                        ExtendMode::Repeat,
                                        ExtendMode::Reflect,
                                    )
                                };
                                for k in 0..4 {
                                    expected[k] += value[k] / (samples * samples) as f32;
                                }
                            }
                        }
                        for k in 0..4 {
                            let error = (actual[(y * 32 + x) * 4 + k] - expected[k]).abs();
                            if error > max {
                                worst = format!(
                                    "{x},{y} actual {:?} expected {:?}",
                                    &actual[(y * 32 + x) * 4..][..4],
                                    expected
                                );
                            }
                            max = max.max(error);
                        }
                    }
                }
                assert!(max < 0.012, "{aa:?} {quality:?} {route}: {max}: {worst}");
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [32, 32],
                    &ops
                ));
                assert_eq!(renderer.last_stats().image_uploads, 0);
                rows.push(json!({"aa":format!("{aa:?}"),"sampling":format!("{quality:?}"),"route":route,"max_linear_error":max,"worst":worst,"mip_levels":image.mip_level_count(),"warm_image_uploads":renderer.last_stats().image_uploads}));
            }
        }
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("mipmap-matrix.json"),
        serde_json::to_vec_pretty(&json!({"cases":rows}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn whole_scene_sampling_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sampled-scene-output"),
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let child = Scene2D::record([32, 32], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 12.25, 32.);
        c.set_fill("#0000ff");
        c.fill_rect(12.25, 0., 19.75, 32.);
    })?;
    let mut convolution = Convolution::new([3, 1], &[0.25, 0.5, 0.25])?;
    convolution.unit_length = [0.75, 1.];
    let filters = vec![
        vec![],
        vec![
            LayerFilter::Gaussian { sigma: [0.7, 1.1] },
            LayerFilter::Offset {
                offset: [0.25, -0.5],
            },
        ],
        vec![LayerFilter::DropShadow {
            sigma: [0.75, 0.5],
            offset: [1.25, 1.5],
            color: [0., 1., 0., 0.75],
        }],
        vec![LayerFilter::DropShadowOnly {
            sigma: [0.4, 1.],
            offset: [1., -0.5],
            color: [0.4, 0.2, 1., 0.8],
        }],
        vec![
            LayerFilter::Morphology {
                radius: [1, 2],
                operator: Morphology::Erode,
            },
            LayerFilter::ConvolveMatrix(convolution),
        ],
        vec![LayerFilter::ComponentTransfer {
            channels: [
                TransferFunction::Identity,
                TransferFunction::Linear {
                    slope: 1.,
                    intercept: 0.1,
                },
                TransferFunction::Identity,
                TransferFunction::Identity,
            ],
            color_space: FilterColorSpace::LinearSrgb,
        }],
    ];
    let mut rows = vec![];
    for n in [1, 2, 4, 8] {
        for (case, filters) in filters.iter().enumerate() {
            let scene = Scene2D::record([32, 32], |c| {
                c.set_fill("#202020");
                c.fill_rect(0., 0., 32., 32.);
                c.save();
                let clip = filled_rect(2.25, 3.25, 26., 25.);
                c.clip_path(&clip, FillRule::NonZero);
                c.clip_path(&clip, FillRule::NonZero);
                c.draw_scene(
                    &child,
                    SceneOptions {
                        layer: Some(LayerOptions {
                            opacity: 0.8,
                            clip: Some([1.25, 2.25, 28., 28.]),
                            filters: filters.clone(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )
                .unwrap();
                c.restore();
            })?;
            let sampled = scene.sampled(
                [32, 32],
                Default::default(),
                SceneSampling {
                    axis_samples: n,
                    ..Default::default()
                },
            )?;
            let cpu = sampled.rasterize()?;
            let expected = cpu.linear_rgba32f().unwrap();
            let mut prepared = sampled.prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
            assert!(prepared.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                true
            ));
            let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
            let mut max = 0f32;
            for (a, e) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
                for k in 0..4 {
                    let value = if k == 3 { e[3] } else { e[k] * e[3] };
                    max = max.max((a[k] - value).abs());
                }
            }
            assert!(max < 0.018, "axis{n},case{case}: {max}");
            assert_eq!(prepared.last_stats().queue_submissions, 1);
            assert!(prepared.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                true
            ));
            assert_eq!(prepared.last_stats().image_uploads, 0);
            rows.push(json!({"axis_samples":n,"case":case,"max_premultiplied_error":max,"submissions":prepared.last_stats().queue_submissions}));
        }
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("whole-scene-sampling.json"),
        serde_json::to_vec_pretty(&json!({"cases":rows}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn reference_sampling_images() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    let pixels = (0..383 * 191)
        .flat_map(|i| {
            let v = if (i % 383 + i / 383) % 2 == 0 { 255 } else { 0 };
            [v, v, v, 255]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_rgba8(383, 191, &pixels)
        .unwrap()
        .with_mipmaps(Default::default())?;
    for size in [32, 64] {
        let target = scene_tests::texture(&device, size);
        let seams = Scene2D::record([size, size], |c| {
            c.set_fill("#ff0000");
            c.fill_rect(0., 0., 12.25, size as f32);
            c.set_fill("#0000ff");
            c.fill_rect(12.25, 0., size as f32 - 12.25, size as f32);
        })?;
        let mut sampled = seams
            .sampled([size, size], Default::default(), SceneSampling::default())?
            .prepare_gpu(&device, wgpu::TextureFormat::Rgba8UnormSrgb)?;
        assert!(sampled.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            false
        ));
        let rgba = readback(&device, &queue, &target, size)?;
        image::save_buffer(
            out.join(format!("sampling-seams-{size}-gpu.png")),
            &rgba,
            size,
            size,
            image::ColorType::Rgba8,
        )?;
        let mut renderer = PathGpu::try_new(&device, 1).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Area);
        let mut c = GpuCanvas::new_linear(size, size, None);
        c.set_image_sampling(ImageSampling::Trilinear);
        c.draw_image(
            &CanvasImageSource::Image(&image),
            0.,
            0.,
            size as f32,
            size as f32,
        );
        assert!(renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [size, size],
            &c.into_ops()
        ));
        let rgba = readback(&device, &queue, &target, size)?;
        image::save_buffer(
            out.join(format!("sampling-minify-{size}-gpu.png")),
            &rgba,
            size,
            size,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn sampled_painter_order_porter_duff_hdr_and_soft_masks() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mask = Image2D::from_rgba8(
        32,
        32,
        &(0..1024)
            .flat_map(|i| [255, 255, 255, ((i % 32) * 7 + 20) as u8])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let clip = polygon(&[(2.17, 1.37), (29.83, 3.11), (27.13, 29.41), (4.31, 27.93)]);
    let tied = Scene2D::record([32, 32], |c| {
        c.clip_rect(0., 0., 31.875, 32.);
        c.set_fill("#ffffff");
        c.fill_rect(0., 0., 32., 32.);
    })?;
    let mut tied = tied
        .sampled([32, 32], Default::default(), SceneSampling::default())?
        .prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
    assert!(tied.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        true
    ));
    let pixels = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    assert_eq!(pixels[31 * 4 + 3], 0.75);
    let mut rows = vec![];
    for n in [2, 4, 8] {
        for compose in [
            CompositeMode::SourceOver,
            CompositeMode::Xor,
            CompositeMode::SourceIn,
            CompositeMode::DestinationOut,
            CompositeMode::LuminanceIn,
        ] {
            let scene = Scene2D::record([32, 32], |c| {
                c.clip_path(&clip, FillRule::NonZero);
                c.clip_path(&clip, FillRule::NonZero);
                c.set_fill_brush(
                    Color2D::new(ColorSpaceTag::LinearSrgb, [2., -0.1, 0.6, 0.6])
                        .unwrap()
                        .brush()
                        .unwrap(),
                );
                c.fill_rect(0., 0., 22.31, 32.);
                c.push_layer_with_options(LayerOptions {
                    opacity: 0.73,
                    compose,
                    mask: Some(mask.clone()),
                    ..Default::default()
                });
                c.set_fill_brush(
                    Color2D::new(ColorSpaceTag::LinearSrgb, [0.2, 0.7, 1.5, 0.55])
                        .unwrap()
                        .brush()
                        .unwrap(),
                );
                c.fill_path(&polygon(&[
                    (3.37, 4.11),
                    (30.17, 7.33),
                    (25.31, 27.13),
                    (8.39, 28.11),
                ]));
                c.pop_layer();
            })?;
            let sampled = scene.sampled(
                [32, 32],
                SceneOptions {
                    layer: Some(LayerOptions {
                        clip: Some([1.25, 1.75, 29.25, 28.5]),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                SceneSampling {
                    axis_samples: n,
                    ..Default::default()
                },
            )?;
            let cpu = sampled.rasterize()?;
            let mut gpu = sampled.prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
            let mut errors = vec![];
            for premultiplied in [true, false] {
                assert!(gpu.render(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    premultiplied
                ));
                let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
                let mut max = 0f32;
                let mut worst = String::new();
                for (index, (a, e)) in actual
                    .chunks_exact(4)
                    .zip(cpu.linear_rgba32f().unwrap().chunks_exact(4))
                    .enumerate()
                {
                    for k in 0..4 {
                        let v = if k == 3 || !premultiplied {
                            e[k]
                        } else {
                            e[k] * e[3]
                        };
                        let delta = (a[k] - v).abs();
                        if delta > max {
                            worst = format!("{index}: {a:?} expected {e:?}");
                        }
                        max = max.max(delta);
                    }
                }
                assert!(
                    max < 0.025,
                    "axis{n} {compose:?} premul{premultiplied}: {max} {worst}"
                );
                errors.push(max);
            }
            rows.push(json!({"axis_samples":n,"compose":format!("{compose:?}"),"max_premultiplied_error":errors[0],"max_straight_error":errors[1]}));
        }
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("sampled-composition.json"),
        serde_json::to_vec_pretty(&json!({"cases":rows}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn rectangular_sample_grids_preserve_composition_filters_and_hdr() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = initialization_tests::float_target(&device);
    let mut max = 0f32;
    let image = Image2D::from_linear_rgba32f(1, 1, &[2., -0.125, 1.2, 0.5]).unwrap();
    for grid in [[1, 2], [2, 1], [2, 4], [4, 2], [4, 8], [8, 4]] {
        for filtered in [false, true] {
            let scene = Scene2D::record([32, 32], |c| {
                let clip = filled_rect(1.25, 2.25, 25.5, 26.);
                c.clip_path(&clip, FillRule::NonZero);
                c.clip_path(&clip, FillRule::NonZero);
                c.push_layer_with_options(LayerOptions {
                    filters: if filtered {
                        vec![
                            LayerFilter::Gaussian { sigma: [0.6, 1.1] },
                            LayerFilter::Offset {
                                offset: [0.25, 0.5],
                            },
                        ]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                });
                c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 12.25, 32.);
                c.set_fill("#0000ff80");
                c.fill_rect(12.25, 0., 19.75, 32.);
                c.pop_layer();
            })?;
            let sampled =
                scene.sampled_grid([32, 32], Default::default(), grid, 128 * 1024 * 1024)?;
            let cpu = sampled.rasterize()?;
            let expected = cpu.linear_rgba32f().unwrap();
            let mut gpu = sampled.prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
            assert!(gpu.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                true
            ));
            let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
            for (a, e) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
                for k in 0..4 {
                    let value = if k == 3 { e[3] } else { e[k] * e[3] };
                    max = max.max((a[k] - value).abs());
                }
            }
            assert_eq!(gpu.last_stats().queue_submissions, 1);
        }
    }
    assert!(max < 0.02, "rectangular grids max error {max}");
    std::fs::write(
        std::env::var_os("JADIX_2D_EVIDENCE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("rectangular-sampling.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","grids":6,"cases":12,"max_error":max,"hdr_signed_alpha":true}),
        )?,
    )?;
    Ok(())
}
