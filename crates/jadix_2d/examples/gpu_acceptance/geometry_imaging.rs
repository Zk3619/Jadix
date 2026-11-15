use super::*;
fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("geometry-imaging"),
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
    })
}
fn evidence(name: &str, rows: serde_json::Value) -> Result<(), Box<dyn Error>> {
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join(name), serde_json::to_vec_pretty(&rows)?)?;
    Ok(())
}
#[test]
#[ignore = "requires native GPU"]
fn geometry_boolean_coverage_and_scene_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = target(&device);
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    let a = PathGeometry2D::from_path(
        &rounded_rect(1.125, 2.25, 23., 24., [3., 6., 2., 7.]),
        FillRule::NonZero,
        Default::default(),
    )?;
    let b = PathGeometry2D::from_path(
        &filled_circle(20., 15., 11., 64),
        FillRule::NonZero,
        Default::default(),
    )?;
    let mut rows = Vec::new();
    for op in [
        PathBooleanOp::Intersection,
        PathBooleanOp::Union,
        PathBooleanOp::Difference,
        PathBooleanOp::Xor,
    ] {
        let geometry = a.boolean(&b, op, Default::default())?;
        let scene = Scene2D::record([32, 32], |c| {
            c.set_fill("white");
            geometry.fill(c);
        })?;
        let mut cpu = RasterCanvas::new_hdr(32, 32);
        cpu.set_area_antialiasing(true);
        scene.draw(&mut cpu, Default::default())?;
        let mut canvas = GpuCanvas::new_linear(32, 32, None);
        scene.draw(&mut canvas, Default::default())?;
        assert!(!canvas.needs_cpu);
        assert!(renderer.render_premultiplied(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &canvas.into_ops()
        ));
        let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        let max = actual
            .iter()
            .zip(&cpu.buf)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(max < 0.003, "{op:?} {max}");
        let area = actual.iter().skip(3).step_by(4).sum::<f32>() as f64;
        assert!(
            (area - geometry.area()).abs() < 0.07,
            "{op:?} {area} vs {}",
            geometry.area()
        );
        rows.push(json!({"operation":format!("{op:?}"),"max_linear_error":max,"gpu_area":area,"geometric_area":geometry.area(),"triangles":geometry.triangle_count()}));
    }
    let a = PathGeometry2D::from_path(
        &filled_rect(1.125, 2.25, 0.5, 0.5),
        FillRule::NonZero,
        Default::default(),
    )?;
    let a = a.boolean(&a, PathBooleanOp::Intersection, Default::default())?;
    let mut c = GpuCanvas::new_linear(32, 32, None);
    c.set_fill("white");
    a.fill(&mut c);
    assert!(renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &c.into_ops()
    ));
    let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    assert!((actual[(2 * 32 + 1) * 4 + 3] - 0.25).abs() < 1e-5);
    evidence(
        "boolean-matrix.json",
        json!({"cases":rows,"identical_clip_alpha":actual[(2*32+1)*4+3]}),
    )
}
#[test]
#[ignore = "requires native GPU"]
fn high_quality_image_and_pattern_reconstruction_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = target(&device);
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    let rgba = (0..12)
        .flat_map(|i| {
            [
                i as f32 / 3. - 1.,
                (i % 3) as f32 * 0.3,
                1.7,
                (i % 4) as f32 / 3.,
            ]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(4, 3, &rgba).unwrap();
    let mut rows = Vec::new();
    for aa in [
        gpu::AaQuality::Area,
        gpu::AaQuality::Balanced,
        gpu::AaQuality::Reference,
    ] {
        renderer.set_aa_quality(aa);
        for quality in [
            ImageSampling::Nearest,
            ImageSampling::Bilinear,
            ImageSampling::Bicubic,
            ImageSampling::Lanczos3,
        ] {
            for route in 0..5 {
                let mode = [
                    ExtendMode::Pad,
                    ExtendMode::Repeat,
                    ExtendMode::Reflect,
                    ExtendMode::Decal,
                ][if route == 4 { 0 } else { route }];
                let mut c = GpuCanvas::new_linear(32, 32, None);
                if route == 4 {
                    c.set_image_sampling(quality);
                    c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 32., 32.);
                } else {
                    c.set_fill_brush(
                        Brush2D::image(image.clone())
                            .with_transform([4., 0., 0., 4., 7., 5.])
                            .with_extend(mode, mode)
                            .with_sampling(quality),
                    );
                    c.fill_rect(0., 0., 32., 32.);
                }
                assert!(!c.needs_cpu);
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [32, 32],
                    &c.into_ops()
                ));
                let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
                let mut max = 0f32;
                for y in 0..32 {
                    for x in 0..32 {
                        let samples = match aa {
                            gpu::AaQuality::Area => 1,
                            gpu::AaQuality::Reference => 4,
                            _ => 2,
                        };
                        let mut expected = [0.; 4];
                        for sy in 0..samples {
                            for sx in 0..samples {
                                let xx = x as f32 + (sx as f32 + 0.5) / samples as f32;
                                let yy = y as f32 + (sy as f32 + 0.5) / samples as f32;
                                let point = if route == 4 {
                                    [xx * 4. / 32., yy * 3. / 32.]
                                } else {
                                    [(xx - 7.) / 4., (yy - 5.) / 4.]
                                };
                                let value = image.sample_linear(point, quality, mode, mode);
                                for k in 0..4 {
                                    expected[k] += value[k] / (samples * samples) as f32;
                                }
                            }
                        }
                        let p = &actual[(y * 32 + x) * 4..][..4];
                        for k in 0..4 {
                            max = max.max((p[k] - expected[k]).abs());
                        }
                        assert!(p.iter().all(|v| v.is_finite()));
                        assert!((-0.001..=1.001).contains(&p[3]));
                    }
                }
                assert!(max < 0.008, "{quality:?} {route} {max}");
                rows.push(
                json!({"sampling":format!("{quality:?}"),"route":route,"aa":format!("{aa:?}"),"max_linear_error":max}),
            );
            }
        }
    }
    evidence("reconstruction-matrix.json", json!({"cases":rows}))
}
#[test]
#[ignore = "requires native GPU"]
fn cropped_images_all_aa_modes_do_not_bleed_or_batch_wrong_domains() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = target(&device);
    let image =
        Image2D::from_rgba8(3, 1, &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255]).unwrap();
    let mut rows = Vec::new();
    for aa in [
        gpu::AaQuality::Area,
        gpu::AaQuality::Balanced,
        gpu::AaQuality::High,
        gpu::AaQuality::Reference,
    ] {
        let mut renderer =
            PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
        renderer.set_aa_quality(aa);
        for quality in [
            ImageSampling::Nearest,
            ImageSampling::Bilinear,
            ImageSampling::Bicubic,
            ImageSampling::Lanczos3,
        ] {
            let mut c = GpuCanvas::new_linear(32, 32, None);
            c.set_image_sampling(quality);
            c.draw_image(
                &CanvasImageSource::Region {
                    image: &image,
                    rect: [1, 0, 1, 1],
                },
                0.,
                0.,
                16.,
                32.,
            );
            c.draw_image(
                &CanvasImageSource::Region {
                    image: &image,
                    rect: [0, 0, 1, 1],
                },
                16.,
                0.,
                16.,
                32.,
            );
            assert!(renderer.render_premultiplied(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [32, 32],
                &c.into_ops()
            ));
            let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
            let mut max = 0f32;
            for y in 0..32 {
                for x in 0..32 {
                    let expected = if x < 16 {
                        [0., 128. / 255., 0., 128. / 255.]
                    } else {
                        [1., 0., 0., 1.]
                    };
                    for k in 0..4 {
                        max = max.max((actual[(y * 32 + x) * 4 + k] - expected[k]).abs());
                    }
                }
            }
            assert!(max < 0.001, "{aa:?} {quality:?} {max}");
            rows.push(json!({"aa":format!("{aa:?}"),"sampling":format!("{quality:?}"),"max_linear_error":max}));
        }
    }
    evidence("crop-matrix.json", json!({"cases":rows}))
}

#[test]
#[ignore = "requires native GPU"]
fn boolean_clips_independent_rect_oracle_images() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    let mut renderer = PathGpu::try_new(&device, 1).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    for size in [256, 512] {
        let target = scene_tests::texture(&device, size);
        let scale = size as f32 / 256.;
        let mut c = GpuCanvas::new_linear(size, size, None);
        c.set_fill("#44aaff");
        for row in 0..4 {
            for col in 0..8 {
                let x = (8.2 + col as f32 * 30.) * scale;
                let y = (8.25 + row as f32 * 55.) * scale;
                let a = PathGeometry2D::from_path(
                    &filled_rect(x, y, 0.35 * scale, 35.4 * scale),
                    FillRule::NonZero,
                    Default::default(),
                )?;
                let clip = PathGeometry2D::from_path(
                    &filled_rect(
                        x + if col < 4 { 0. } else { 0.1 * scale },
                        y,
                        if col < 4 { 0.35 } else { 0.2 } * scale,
                        35.4 * scale,
                    ),
                    FillRule::NonZero,
                    Default::default(),
                )?;
                let geometry = a
                    .boolean(&clip, PathBooleanOp::Intersection, Default::default())?
                    .boolean(&clip, PathBooleanOp::Intersection, Default::default())?;
                geometry.fill(&mut c);
            }
        }
        assert!(renderer.render_premultiplied(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [size, size],
            &c.into_ops()
        ));
        let rgba = readback(&device, &queue, &target, size)?;
        image::save_buffer(
            out.join(format!("clip-conflation-{size}-gpu.png")),
            &rgba,
            size,
            size,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}
