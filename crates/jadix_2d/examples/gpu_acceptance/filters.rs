use super::*;

fn matrix() -> LayerFilter {
    LayerFilter::ColorMatrix([
        0., 0., 1., 0., 0.1, 0., 1., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0.8, 0.05,
    ])
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn msaa_resolve_ignores_last_draw_scissor() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("resolve-scissor-regression"),
        size: wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut c = GpuCanvas::new_linear(8, 8, None);
    c.set_fill("blue");
    c.fill_rect(0., 0., 8., 8.);
    assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
    for clip in [[1., 1., 6., 6.], [2., 2., 4., 4.], [0., 0., 3., 3.]] {
        let mut c = GpuCanvas::new_linear(8, 8, None);
        c.clip_rect(clip[0], clip[1], clip[2], clip[3]);
        c.set_fill("red");
        c.fill_rect(0., 0., 8., 8.);
        let expected = c.get_image_data(0, 0, 8, 8).unwrap();
        assert!(renderer.render(&device, &queue, &view, [8, 8], &c.into_ops()));
        assert_eq!(
            readback(&device, &queue, &texture, 8)?,
            expected,
            "MSAA resolve must replace pixels outside last scissor"
        );
    }
    Ok(())
}
fn bias() -> LayerFilter {
    let mut m = [0.; 20];
    m[4] = 0.25;
    m[9] = 0.5;
    m[14] = 1.;
    m[19] = 0.6;
    LayerFilter::ColorMatrix(m)
}
fn filter_scene(
    c: &mut dyn Canvas2D,
    filters: Vec<LayerFilter>,
    variant: usize,
    mode: CompositeMode,
) {
    c.clear("#20406080");
    let mask = Image2D::from_rgba8(
        64,
        64,
        &(0..64 * 64)
            .flat_map(|i| [0, 0, 0, if i % 64 < 32 { 255 } else { 128 }])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    c.push_layer_with_options(LayerOptions {
        filters,
        opacity: 0.75,
        blend: BlendMode::Screen,
        compose: mode,
        clip: if variant == 1 {
            Some([8.5, 4.25, 48., 50.5])
        } else {
            None
        },
        mask: if variant == 1 { Some(mask) } else { None },
        ..Default::default()
    });
    if variant < 2 {
        c.set_fill("#ff400080");
        c.fill_rect(0., 16., 28., 20.);
        c.push_layer(0.7, BlendMode::Normal);
        c.set_fill("#20a0ffb0");
        c.fill_rect(18., 8., 18., 30.);
        c.pop_layer();
        c.save();
        c.translate(40., 40.);
        c.rotate(17.);
        c.set_fill("#a08040a0");
        c.fill_rect(-8., -6., 16., 12.);
        c.restore();
    }
    c.pop_layer();
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn rounded_shadows_and_group_filters_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();

    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut records = Vec::new();
        let mut sheet = vec![0u8; 512 * 512 * 4];
        let mut cell = 0;
        let mut check = |label: String,
                         paint: &dyn Fn(&mut dyn Canvas2D)|
         -> Result<(), Box<dyn Error>> {
            let mut c = RasterCanvas::new_linear(64, 64);
            paint(&mut c);
            let expected = c.get_image_data(0, 0, 64, 64).unwrap();
            let mut c = GpuCanvas::new_linear(64, 64, None);
            paint(&mut c);
            assert!(!c.needs_cpu);
            assert_eq!(c.get_image_data(0, 0, 64, 64).unwrap(), expected);
            let ops = c.into_ops();
            assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
            let actual = readback(&device, &queue, &texture, 64)?;
            let mut sum = 0.;
            let mut alpha = 0.;
            let mut ink = 0f64;
            let mut max = 0f64;
            for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
                if a[3] > 0 || b[3] > 0 {
                    ink += 1.;
                    alpha += f64::from(a[3].abs_diff(b[3]));
                }
                for i in 0..3 {
                    let linear = jadix_theme::color_space::linearize_display_srgb_component;
                    let e = f64::from(
                        (linear(a[i] as f32 / 255.) * a[3] as f32
                            - linear(b[i] as f32 / 255.) * b[3] as f32)
                            .abs(),
                    );
                    sum += e;
                    max = max.max(e);
                }
            }
            let mae = (sum + alpha) / (64. * 64. * 4.);
            let ink_alpha = alpha / ink.max(1.);
            assert!(
                mae < 1.5 && ink_alpha < 6.,
                "{label} {samples}: premul MAE {mae}, ink alpha {ink_alpha}, max {max}"
            );
            assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
            let stats = renderer.last_stats();
            assert_eq!(
                (
                    stats.buffer_allocations,
                    stats.texture_allocations,
                    stats.bind_group_allocations
                ),
                (0, 0, 0),
                "warm {label}"
            );
            assert_eq!(stats.queue_submissions, 1);
            if cell < 64 {
                let (cx, cy) = (cell % 8, cell / 8);
                for y in 0..64 {
                    let from = y * 64 * 4;
                    let to = ((cy * 64 + y) * 512 + cx * 64) * 4;
                    sheet[to..to + 256].copy_from_slice(&actual[from..from + 256]);
                }
                cell += 1;
            }
            records.push(json!({"case":label,"linear_premultiplied_mae":mae,"ink_alpha_mae":ink_alpha,"max_premultiplied_error":max,"warm_gpu_allocations":0,"queue_submissions":1}));
            Ok(())
        };
        for radii in [[0.; 4], [0., 16., 4., 10.], [200., 40., 0., 60.], [12.; 4]] {
            for sigma in [0., 0.25, 2., 12.] {
                for inset in [false, true] {
                    for transform in 0..3 {
                        check(
                            format!("shadow/{radii:?}/{sigma}/{inset}/{transform}"),
                            &|c| {
                                c.save();
                                if transform == 1 {
                                    c.translate(32., 20.);
                                    c.rotate(27.);
                                    c.translate(-32., -20.);
                                    c.clip_rect(4., 4., 52., 48.);
                                }
                                if transform == 2 {
                                    c.scale(0.8, 1.3);
                                    c.push_layer(0.7, BlendMode::Normal);
                                }
                                c.draw_rounded_rect_shadow(
                                    [12., 12., 32., 24.],
                                    radii,
                                    sigma,
                                    [0.25, 0.6, 1., 0.8],
                                    inset,
                                );
                                if transform == 2 {
                                    c.pop_layer();
                                }
                                c.restore();
                            },
                        )?;
                    }
                }
            }
        }
        let shadow = LayerFilter::DropShadow {
            sigma: [2., 4.],
            offset: [-2.5, 3.25],
            color: [0.8, 0.2, 0.1, 0.7],
        };
        let chains = vec![
            vec![LayerFilter::Gaussian { sigma: [0.; 2] }],
            vec![LayerFilter::Gaussian { sigma: [0.2, 3.] }],
            vec![LayerFilter::Gaussian { sigma: [5., 0.] }],
            vec![LayerFilter::DropShadow {
                sigma: [0.; 2],
                offset: [1.5, -0.5],
                color: [0., 1., 0., 0.5],
            }],
            vec![shadow.clone()],
            vec![matrix()],
            vec![
                shadow.clone(),
                matrix(),
                LayerFilter::Gaussian { sigma: [1., 2.] },
            ],
            vec![
                LayerFilter::Gaussian { sigma: [1., 2.] },
                shadow,
                LayerFilter::Gaussian { sigma: [1., 1.] },
            ],
            vec![bias()],
            vec![bias(), LayerFilter::Gaussian { sigma: [3., 4.] }],
            vec![LayerFilter::Gaussian {
                sigma: [f32::NAN, 2.],
            }],
            vec![LayerFilter::Gaussian {
                sigma: [f32::MAX; 2],
            }],
            vec![LayerFilter::DropShadow {
                sigma: [1., 1.],
                offset: [f32::MAX; 2],
                color: [1.; 4],
            }],
        ];
        for (i, filters) in chains.into_iter().enumerate() {
            for variant in 0..3 {
                for mode in [
                    CompositeMode::SourceOver,
                    CompositeMode::DestinationIn,
                    CompositeMode::LuminanceIn,
                    CompositeMode::Plus,
                ] {
                    check(format!("filter/{i}/{variant}/{mode:?}"), &|c| {
                        filter_scene(c, filters.clone(), variant, mode)
                    })?;
                }
            }
        }
        for depth in [1, 3, 8] {
            check(format!("nested-filters/{depth}"), &|c| {
                c.clear("#204060");
                for i in 0..depth {
                    c.push_layer_with_options(LayerOptions {
                        filters: if i % 2 == 0 {
                            vec![LayerFilter::Gaussian { sigma: [0.5, 1.] }]
                        } else {
                            vec![matrix()]
                        },
                        opacity: 0.9,
                        clip: Some([2., 2., 60., 60.]),
                        ..Default::default()
                    });
                }
                c.set_fill("#ff4000a0");
                c.fill_rect(4., 12., 48., 32.);
                for _ in 0..depth {
                    c.pop_layer();
                }
            })?;
        }
        // Force the stencil/cover path for the new payload, as well as batching.
        #[cfg(feature = "test-hooks")]
        {
            renderer.use_general_cover_for_test(&device);
            let mut c = GpuCanvas::new_linear(64, 64, None);
            c.draw_rounded_rect_shadow([8., 8., 40., 32.], [0., 16., 4., 8.], 3., [1.; 4], true);
            assert!(renderer.render(&device, &queue, &view, [64, 64], &c.into_ops()));
        }
        if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
            let dir = Path::new(&dir);
            std::fs::create_dir_all(dir)?;
            std::fs::write(
                dir.join(format!("filters-matrix-{samples}.json")),
                serde_json::to_vec_pretty(
                    &json!({"backend":format!("{backend:?}"),"adapter":format!("{:?}",adapter.get_info()),"cases":records}),
                )?,
            )?;
            image::save_buffer(
                dir.join(format!("shadows-preview-{samples}.png")),
                &sheet,
                512,
                512,
                image::ColorType::Rgba8,
            )?;
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter; bounded resource pressure"]
fn animated_filter_resource_pressure() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();

    let mut records = Vec::new();
    let mut renderer = PathGpu::try_new(&device, 1).unwrap();
    for (size, groups) in [(256u32, 64usize), (512, 16)] {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("filter-pressure"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut times = Vec::new();
        let mut cold_textures = 0;
        let mut peak_bytes = 0;
        for frame in 0..17 {
            let mut c = GpuCanvas::new_linear(size, size, None);
            // Alternate peak and sparse scenes, then reuse peak resources.
            let count = if frame % 3 == 2 { groups / 4 } else { groups };
            for i in 0..count {
                let t = (frame as f32 * 0.1).sin();
                c.push_layer_with_options(LayerOptions {
                    filters: vec![
                        LayerFilter::DropShadow {
                            sigma: [1.5 + t * 0.2, 2.],
                            offset: [2.5 + t, 3.],
                            color: [0.1, 0.2, 1., 0.6],
                        },
                        matrix(),
                    ],
                    ..Default::default()
                });
                c.set_fill("#ff802080");
                c.fill_rect(
                    ((i % 8) * size as usize / 8) as f32,
                    ((i / 8) * size as usize / 8) as f32,
                    20.,
                    16.,
                );
                c.pop_layer();
            }
            let ops = c.into_ops();
            let start = Instant::now();
            assert!(renderer.render_premultiplied(&device, &queue, &view, [size, size], &ops));
            device.poll(wgpu::PollType::wait_indefinitely())?;
            let elapsed = start.elapsed().as_secs_f64() * 1000.;
            let stats = renderer.last_stats();
            if frame == 0 {
                cold_textures = stats.texture_allocations;
            }
            peak_bytes = peak_bytes.max(stats.uploaded_bytes);
            if frame >= 5 {
                assert_eq!(
                    (
                        stats.buffer_allocations,
                        stats.texture_allocations,
                        stats.bind_group_allocations
                    ),
                    (0, 0, 0),
                    "pressure frame {frame}"
                );
                assert_eq!(stats.queue_submissions, 1);
                times.push(elapsed);
            }
        }
        let pixels = readback(&device, &queue, &texture, size)?;
        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
        records.push(json!({"size":size,"peak_groups":groups,"frames":17,"measured_frames":12,"cold_texture_allocations":cold_textures,
            "fp16_scratch_bytes":3*size as u64*size as u64*8,"peak_uploaded_bytes":peak_bytes,
            "warm_gpu_allocations":0,"queue_submissions_per_frame":1,"encode_submit_wait":percentiles(times)}));
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join("filter-pressure.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"adapter":format!("{:?}",adapter.get_info()),"results":records}),
            )?,
        )?;
    }
    Ok(())
}
