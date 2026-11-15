use super::*;
#[test]
#[ignore = "requires native GPU"]
fn analytic_area_materials_and_micro_geometry() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        device,
        queue,
        backend,
        ..
    } = test_gpu();
    let format = wgpu::TextureFormat::Rgba16Float;
    let size = 64;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("area-test"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer = PathGpu::try_new_with_format(&device, 4, format).ok_or("init")?;
    renderer.set_aa_quality(gpu::AaQuality::Area);
    let mut rows = Vec::new();
    for case in 0..10 {
        let draw = |c: &mut dyn Canvas2D| {
            c.set_fill("#4488cc99");
            if case == 9 {
                c.set_fill("white");
                c.fill_path(&filled_circle(16., 16., 5., 256));
                return;
            }
            if case == 0 {
                for i in 0..400 {
                    let x = (i % 20) as f32 * 2.2 + 0.13;
                    let y = (i / 20) as f32 * 2.2 + 0.27;
                    c.fill_rect(x, y, 0.37, 0.49);
                }
                return;
            }
            if case == 5 || case == 6 {
                let image = Image2D::from_rgba8(
                    2,
                    2,
                    &[
                        255, 0, 0, 128, 0, 255, 0, 128, 0, 0, 255, 128, 255, 255, 0, 128,
                    ],
                )
                .unwrap();
                if case == 5 {
                    c.set_fill_brush(Brush2D::image(image));
                    c.fill_rect(2.25, 3.5, 23.2, 19.6);
                } else {
                    c.draw_image(&CanvasImageSource::Image(&image), 3.25, 4.5, 24.2, 20.6);
                }
                return;
            }
            if case == 7 {
                c.clip_path(
                    &polygon(&[(2., 2.), (49., 3.), (23., 52.)]),
                    FillRule::NonZero,
                );
                c.fill_rect(1.13, 2.27, 40.4, 43.6);
                return;
            }
            if case == 8 {
                c.set_stroke("#44ffaaaa");
                c.set_line_width(1.3);
                c.stroke_path(&polygon(&[(2.3, 3.4), (47.8, 12.4), (18.3, 52.7)]));
                return;
            }
            if case == 1 {
                let mut path = filled_rect(2.2, 2.3, 48.6, 48.2);
                path.extend(filled_rect(13.1, 12.8, 20.4, 20.7));
                c.set_fill_rule(FillRule::EvenOdd);
                c.fill_path(&path);
                return;
            }
            if case == 2 {
                c.push_layer_with_options(LayerOptions {
                    opacity: 0.7,
                    filters: vec![LayerFilter::Gaussian { sigma: [0.7, 1.2] }],
                    ..Default::default()
                });
            }
            let gradient = VectorGradient::linear(
                [0., 0.],
                [64., 64.],
                GradientStops::new(&[(0., [1., 0., 0., 0.7]), (1., [0., 0., 1., 0.9])]).unwrap(),
            )
            .unwrap();
            c.set_fill_brush(Brush2D::vector(gradient));
            c.fill_rect(3.12, 4.35, 46.23, 38.19);
            if case == 2 {
                c.pop_layer();
            }
            if case == 3 {
                c.set_fill("#22ff88aa");
                c.fill_path(&filled_circle(25., 25., 10., 100));
            }
            if case == 4 {
                c.clip_rect(7.125, 8.25, 33.5, 27.5);
                c.set_fill("white");
                c.fill_rect(0., 0., 64., 64.);
            }
        };
        let mut cpu = RasterCanvas::new_hdr(size, size);
        cpu.set_area_antialiasing(true);
        if case == 9 {
            assert!(cpu.set_curve_tolerance(0.001));
        }
        draw(&mut cpu);
        let expected = cpu.into_linear_rgba32f();
        let mut gpu = GpuCanvas::new_linear(size, size, None);
        if case == 9 {
            assert!(gpu.set_curve_tolerance(0.001));
        }
        draw(&mut gpu);
        assert!(!gpu.needs_cpu);
        let ops = gpu.into_ops();
        assert!(renderer.render_premultiplied(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [size, size],
            &ops
        ));
        let actual = advanced_tests::read_float(&device, &queue, &target, [size, size]);
        let mae = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs() as f64)
            .sum::<f64>()
            / actual.len() as f64;
        assert!(mae < 0.006, "{case}: {mae}");
        if case == 9 {
            let area = actual.iter().skip(3).step_by(4).sum::<f32>();
            assert!(
                (area - std::f32::consts::PI * 25.).abs() < 0.04,
                "GPU curve area {area}"
            );
        }
        assert!(renderer.last_stats().area_primitives > 0);
        rows.push(json!({"case":case,"linear_mae":mae,"primitives":renderer.last_stats().area_primitives,"draw_calls":renderer.last_stats().draw_calls}));
    }
    let tri = PathTri {
        a: (0., 0.),
        b: (64., 0.),
        c: (64., 64.),
        color: [1.; 4],
    };
    let mut rejected =
        build_fill_op(&filled_rect(0., 0., 64., 64.), &PathStyle::fill([1.; 4])).unwrap();
    rejected.stencil = vec![tri; 4097];
    assert!(!renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [size, size],
        &[PathGpuOp::Fill(rejected)]
    ));
    let mut excessive =
        build_fill_op(&filled_rect(0., 0., 64., 64.), &PathStyle::fill([1.; 4])).unwrap();
    excessive.stencil = vec![tri; 4096];
    assert!(!renderer.render_premultiplied(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [size, size],
        &vec![PathGpuOp::Fill(excessive); 5]
    ));
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("jadix-area-{backend:?}")));
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("area-matrix.json"),
        serde_json::to_vec_pretty(&json!({"cases":rows}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn automatic_tight_layer_domains_match_full_canvas() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        device,
        queue,
        backend,
        ..
    } = test_gpu();
    let size = 128;
    let mut rows = Vec::new();
    let format = wgpu::TextureFormat::Rgba16Float;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("tight-domains"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    for quality in [gpu::AaQuality::Area, gpu::AaQuality::Reference] {
        for case in 0..4 {
            let mut c = GpuCanvas::new_linear(size, size, None);
            c.set_fill("#4488aa88");
            c.fill_rect(0., 0., 128., 128.);
            let filters = match case {
                0 => vec![LayerFilter::Gaussian { sigma: [1., 2.] }],
                1 => vec![
                    LayerFilter::Offset {
                        offset: [2.3, -1.4],
                    },
                    LayerFilter::Morphology {
                        radius: [2, 1],
                        operator: Morphology::Dilate,
                    },
                    LayerFilter::DropShadow {
                        sigma: [1., 1.],
                        offset: [-3., 4.],
                        color: [0., 0., 1., 0.7],
                    },
                ],
                2 => vec![LayerFilter::ColorMatrix([
                    1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0.3,
                ])],
                _ => vec![LayerFilter::Gaussian { sigma: [1., 2.] }],
            };
            c.push_layer_with_options(LayerOptions {
                opacity: 0.8,
                filters,
                ..Default::default()
            });
            if case == 3 {
                c.push_layer_with_options(LayerOptions {
                    clip: Some([45., 45., 4., 4.]),
                    filters: vec![LayerFilter::Gaussian { sigma: [2., 2.] }],
                    ..Default::default()
                });
            }
            c.set_fill("#ff5588aa");
            c.fill_rect(44.25, 43.5, 8.5, 9.25);
            if case == 3 {
                c.pop_layer();
            }
            c.pop_layer();
            let ops = c.into_ops();
            let mut full = PathGpu::try_new_with_format(&device, 4, format).ok_or("init")?;
            full.set_aa_quality(quality);
            assert!(full.render_premultiplied(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [size, size],
                &ops
            ));
            let expected = advanced_tests::read_float(&device, &queue, &target, [size, size]);
            let mut compact = PathGpu::try_new_with_format(&device, 4, format).ok_or("init")?;
            compact.set_aa_quality(quality);
            compact.set_tight_layer_bounds(true);
            for _ in 0..2 {
                assert!(compact.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    [size, size],
                    &ops
                ));
            }
            let actual = advanced_tests::read_float(&device, &queue, &target, [size, size]);
            let max = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            let stats = compact.last_stats();
            assert!(max < 0.003, "{case}/{quality:?} delta {max}");
            assert_eq!(stats.texture_allocations, 0);
            if case != 2 {
                assert!(
                    stats.layer_source_pixels < stats.layer_full_canvas_pixels,
                    "{case}: {stats:?}"
                );
            }
            rows.push(json!({"case":case,"quality":format!("{quality:?}"),"max_linear_delta":max,"source_pixels":stats.layer_source_pixels,"full_canvas_pixels":stats.layer_full_canvas_pixels}));
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("tight-domains.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}
