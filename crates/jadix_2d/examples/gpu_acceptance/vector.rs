use super::*;
fn pixel_error(actual: &[u8], expected: &[u8]) -> (f64, f64) {
    let mut sum = 0f64;
    let mut alpha = 0f64;
    for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
        alpha += f64::from(a[3].abs_diff(b[3]));
        for k in 0..3 {
            let linear = jadix_theme::color_space::linearize_display_srgb_component;
            sum += f64::from(
                (linear(a[k] as f32 / 255.) * a[3] as f32
                    - linear(b[k] as f32 / 255.) * b[3] as f32)
                    .abs(),
            );
        }
    }
    (
        (sum + alpha) / actual.len() as f64,
        alpha / (actual.len() / 4) as f64,
    )
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn sweep_center_angle_is_defined() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sweep-center"),
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
    let mut c = GpuCanvas::new_linear(64, 64, None);
    c.set_fill_brush(Brush2D::vector(
        VectorGradient::sweep(
            [32.5, 32.5],
            0.,
            360.,
            GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap(),
        )
        .unwrap(),
    ));
    c.fill_rect(0., 0., 64., 64.);
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [64, 64],
        &c.into_ops()
    ));
    let pixels = readback(&device, &queue, &target, 64)?;
    let at = (32 * 64 + 32) * 4;
    assert_eq!(&pixels[at..at + 4], &[255, 0, 0, 255]);
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("sweep-center.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"samples":4,"center":[32.5,32.5],"angle":0,"actual_rgba":&pixels[at..at+4]}),
            )?,
        )?;
    }
    // Legacy CSS conics use normalized UVs; interpolation of an odd-size
    // rectangle can miss its center by a few ulps. Exercise many triangles,
    // both MSAA modes and multiple frames, independent of the CPU area sampler.
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("css-conic-centers"),
        size: wgpu::Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        for _ in 0..3 {
            let mut c = GpuCanvas::new_linear(256, 256, None);
            for y in 0..8 {
                for x in 0..8 {
                    c.set_fill_gradient(
                        gradient::parse_gradient("conic-gradient(#ff0000,#0000ff,#ff0000)")
                            .unwrap(),
                    );
                    c.fill_rect(4. + x as f32 * 31., 4. + y as f32 * 31., 25., 25.);
                }
            }
            assert!(renderer.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                [256, 256],
                &c.into_ops()
            ));
            let pixels = readback(&device, &queue, &target, 256)?;
            for y in 0..8 {
                for x in 0..8 {
                    let at = ((16 + y * 31) * 256 + 16 + x * 31) * 4;
                    // CSS's default -90-degree start maps the +X origin
                    // convention to the red/blue midpoint (linear RGB .5).
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip([188u8, 0, 188, 255])
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "CSS center {backend:?}/{samples}/{x}/{y}: {:?}",
                        &pixels[at..at + 4]
                    );
                }
            }
        }
    }
    Ok(())
}
fn stops(n: usize) -> GradientStops {
    GradientStops::new(
        &(0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                (
                    t,
                    if n > 33 {
                        [t, 0.3, 1. - t, 0.4 + t * 0.6]
                    } else {
                        [
                            if i % 3 == 0 { 1. } else { 0.2 },
                            t,
                            1. - t,
                            if i % 4 == 0 { 0. } else { 0.4 + t * 0.6 },
                        ]
                    },
                )
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn gradient(kind: usize, s: GradientStops) -> VectorGradient {
    match kind {
        0 => VectorGradient::linear([8., 12.], [56., 48.], s),
        1 => VectorGradient::radial([32., 32.], 0., [32., 32.], 25., s),
        2 => VectorGradient::radial([20., 28.], 4., [36., 32.], 24., s),
        3 => VectorGradient::radial([16., 32.], 0., [40., 32.], 24., s),
        4 => VectorGradient::radial([24., 32.], 12., [40., 32.], 12., s),
        5 => VectorGradient::radial([32., 32.], 24., [28., 32.], 3., s),
        6 => VectorGradient::sweep([32., 32.], -40., 220., s),
        _ => VectorGradient::sweep([32., 32.], 120., -120., s),
    }
    .unwrap()
}
fn paint(c: &mut dyn Canvas2D, g: VectorGradient, variant: usize) {
    c.save();
    if variant == 1 {
        c.translate(32., 32.);
        c.rotate(17.);
        c.scale(0.8, 1.1);
        c.translate(-32., -32.);
        c.clip_rect(6., 8., 52., 48.);
        c.push_layer_with_options(LayerOptions {
            opacity: 0.8,
            filters: vec![LayerFilter::Gaussian { sigma: [0.5, 1.] }],
            ..Default::default()
        });
    }
    c.set_global_alpha(0.8);
    c.set_fill_brush(Brush2D::vector(g.clone()));
    c.fill_path(&decode_path_d("M4 4 H60 V60 H4 Z M24 24 H40 V40 H24 Z"));
    c.set_stroke_brush(Brush2D::vector(g));
    c.set_line_width(3.);
    c.stroke_path(&polyline(&[(8., 56.), (28., 36.), (56., 52.)]));
    if variant == 1 {
        c.pop_layer();
    }
    c.restore();
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn exact_vector_gradients_and_extended_filter_matrix() -> Result<(), Box<dyn Error>> {
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
            label: Some("vector-gradient-matrix"),
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
            let (mae, alpha_mae) = pixel_error(&actual, &expected);
            assert!(
                mae < 1. && alpha_mae < 1.,
                "{label}/{samples}: premul MAE {mae}, alpha {alpha_mae}"
            );
            assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
            let s = renderer.last_stats();
            assert_eq!(
                (
                    s.buffer_allocations,
                    s.texture_allocations,
                    s.bind_group_allocations
                ),
                (0, 0, 0),
                "warm {label}"
            );
            assert_eq!(s.queue_submissions, 1);
            if cell < 64 {
                let (cx, cy) = (cell % 8, cell / 8);
                for y in 0..64 {
                    let dest = ((cy * 64 + y) * 512 + cx * 64) * 4;
                    sheet[dest..dest + 256].copy_from_slice(&actual[y * 256..y * 256 + 256]);
                }
                cell += 1;
            }
            records.push(json!({"case":label,"linear_premultiplied_mae":mae,"alpha_mae":alpha_mae,"warm_gpu_allocations":0,"queue_submissions":1}));
            Ok(())
        };
        for n in [2, 9, 33, 257] {
            let s = stops(n);
            for kind in 0..8 {
                for mode in [
                    ExtendMode::Pad,
                    ExtendMode::Repeat,
                    ExtendMode::Reflect,
                    ExtendMode::Decal,
                ] {
                    for alpha in [GradientAlpha::Premultiplied, GradientAlpha::Straight] {
                        for variant in 0..2 {
                            let g = gradient(kind, s.clone())
                                .with_extend(mode)
                                .with_alpha_mode(alpha);
                            check(
                                format!("gradient/{n}/{kind}/{mode:?}/{alpha:?}/{variant}"),
                                &|c| paint(c, g.clone(), variant),
                            )?;
                        }
                    }
                }
            }
        }
        let hard = GradientStops::new(&[
            (0., [1., 0., 0., 1.]),
            (0.5, [1., 0., 0., 1.]),
            (0.5, [0., 0., 1., 0.5]),
            (1., [0., 0., 1., 0.5]),
        ])
        .unwrap();
        for s in [hard, stops(4096)] {
            for kind in [0, 2, 6] {
                check(format!("large-hard/{}/{kind}", s.len()), &|c| {
                    paint(c, gradient(kind, s.clone()), 0)
                })?;
            }
        }
        let mask = Image2D::from_rgba8(
            64,
            64,
            &(0..4096)
                .flat_map(|i| [0, 0, 0, if i % 64 < 32 { 255 } else { 96 }])
                .collect::<Vec<_>>(),
        )
        .unwrap();
        for kind in [0, 2, 6] {
            for n in [1, 9, 4096] {
                let s = if n == 1 {
                    GradientStops::new(&[(0.5, [0.2, 0.7, 1., 0.6])]).unwrap()
                } else {
                    stops(n)
                };
                check(format!("masked/{kind}/{n}"), &|c| {
                    c.clip_image_mask(&mask);
                    paint(
                        c,
                        gradient(kind, s.clone()).with_extend(ExtendMode::Reflect),
                        1,
                    );
                })?;
            }
        }
        let mut effects = vec![
            vec![LayerFilter::Offset {
                offset: [2.5, -1.25],
            }],
            vec![LayerFilter::Offset {
                offset: [f32::MAX; 2],
            }],
        ];
        for midpoint in [0.1, 0.25, 0.5, 0.8, 0.95] {
            let s = stops(2).with_midpoints(&[Some(midpoint)]).unwrap();
            for kind in [0, 2, 6] {
                for alpha in [GradientAlpha::Premultiplied, GradientAlpha::Straight] {
                    let g = gradient(kind, s.clone()).with_alpha_mode(alpha);
                    check(format!("midpoint/{midpoint}/{kind}/{alpha:?}"), &|c| {
                        paint(c, g.clone(), 0)
                    })?;
                }
            }
        }
        for radius in [[0, 0], [1, 3], [4, 0], [u32::MAX, 1]] {
            for operator in [Morphology::Dilate, Morphology::Erode] {
                effects.push(vec![LayerFilter::Morphology { radius, operator }]);
            }
        }
        effects.push(vec![LayerFilter::DropShadowOnly {
            sigma: [2., 3.],
            offset: [-2.5, 3.],
            color: [1., 0.2, 0.1, 0.6],
        }]);
        effects.push(vec![
            LayerFilter::Morphology {
                radius: [2, 3],
                operator: Morphology::Dilate,
            },
            LayerFilter::Gaussian { sigma: [1., 2.] },
            LayerFilter::Offset { offset: [0.5, -1.] },
        ]);
        for (i, effects) in effects.into_iter().enumerate() {
            for variant in 0..3 {
                check(format!("effect/{i}/{variant}"), &|c| {
                    c.push_layer_with_options(LayerOptions {
                        filters: effects.clone(),
                        clip: Some([3.5, 2., 56., 59.]),
                        ..Default::default()
                    });
                    if variant < 2 {
                        c.set_fill("#ff400080");
                        c.fill_rect(0., 8., 32., 24.);
                        c.set_fill("#2080ffc0");
                        c.fill_rect(20., 24., 24., 20.);
                        if variant == 1 {
                            c.push_luminance_layer(0.7, false);
                            c.set_fill("white");
                            c.fill_rect(0., 0., 64., 32.);
                            c.pop_layer();
                        }
                    }
                    c.pop_layer();
                })?;
            }
        }
        drop(check);
        #[cfg(feature = "test-hooks")]
        {
            renderer.use_general_cover_for_test(&device);
            for kind in [0, 2, 6] {
                let mut c = GpuCanvas::new_linear(64, 64, None);
                paint(&mut c, gradient(kind, stops(33)), 0);
                let expected = c.get_image_data(0, 0, 64, 64).unwrap();
                assert!(renderer.render(&device, &queue, &view, [64, 64], &c.into_ops()));
                let actual = readback(&device, &queue, &texture, 64)?;
                let (mae, alpha_mae) = pixel_error(&actual, &expected);
                assert!(
                    mae < 1. && alpha_mae < 1.,
                    "general cover {kind}: {mae}/{alpha_mae}"
                );
                records.push(json!({"case":format!("general-cover/{kind}"),"linear_premultiplied_mae":mae,"alpha_mae":alpha_mae}));
            }
        }
        if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
            let dir = Path::new(&dir);
            std::fs::create_dir_all(dir)?;
            std::fs::write(
                dir.join(format!("vector-matrix-{samples}.json")),
                serde_json::to_vec_pretty(
                    &json!({"backend":format!("{backend:?}"),"adapter":format!("{:?}",adapter.get_info()),"cases":records}),
                )?,
            )?;
            image::save_buffer(
                dir.join(format!("vector-preview-{samples}.png")),
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
#[ignore = "requires a native headless GPU adapter"]
fn shared_4096_stops_batch_and_resource_reuse() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shared-stops-pressure"),
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
    let shared = stops(4096);
    let mut prepare = Vec::new();
    let mut gpu = Vec::new();
    for frame in 0..45 {
        let start = Instant::now();
        let mut c = GpuCanvas::new_linear(64, 64, None);
        let g = VectorGradient::linear([0., 0.], [64., 0.], shared.clone()).unwrap();
        for y in 0..16 {
            for x in 0..16 {
                c.set_fill_brush(Brush2D::vector(g.clone()).with_transform([
                    1.,
                    0.,
                    0.,
                    1.,
                    (frame as f32 * 0.05).sin(),
                    0.,
                ]));
                c.fill_rect(x as f32 * 4., y as f32 * 4., 3., 3.);
            }
        }
        let ops = c.into_ops();
        let prep = start.elapsed().as_secs_f64() * 1000.;
        let submit = Instant::now();
        assert!(renderer.render_premultiplied(&device, &queue, &view, [64, 64], &ops));
        device.poll(wgpu::PollType::wait_indefinitely())?;
        let elapsed = submit.elapsed().as_secs_f64() * 1000.;
        let s = renderer.last_stats();
        assert_eq!(s.gradient_records, 256);
        assert_eq!(s.gradient_stops, 4096);
        assert_eq!(s.gradient_table_bytes, 196608);
        assert_eq!(s.draw_calls, 1);
        assert_eq!(s.queue_submissions, 1);
        if frame >= 5 {
            assert_eq!(
                (
                    s.buffer_allocations,
                    s.texture_allocations,
                    s.bind_group_allocations
                ),
                (0, 0, 0)
            );
            prepare.push(prep);
            gpu.push(elapsed);
        }
    }
    let actual = readback(&device, &queue, &texture, 64)?;
    assert!(actual.chunks_exact(4).any(|p| p[3] > 0));
    // Independent stop arrays must respect the device's finite table limit.
    // A rejected oversized frame must neither submit nor poison later reuse.
    let limit = device.limits().max_texture_dimension_2d;
    let mut oversized = GpuCanvas::new_linear(64, 64, None);
    for _ in 0..(limit / 512 + 1) {
        oversized.set_fill_brush(Brush2D::vector(gradient(0, stops(4096))));
        oversized.fill_rect(0., 0., 64., 64.);
    }
    assert!(!renderer.render_premultiplied(
        &device,
        &queue,
        &view,
        [64, 64],
        &oversized.into_ops()
    ));
    assert_eq!(renderer.last_stats().queue_submissions, 0);
    let mut recovered = GpuCanvas::new_linear(64, 64, None);
    recovered.set_fill("blue");
    recovered.fill_rect(0., 0., 64., 64.);
    assert!(renderer.render_premultiplied(&device, &queue, &view, [64, 64], &recovered.into_ops()));
    assert!(readback(&device, &queue, &texture, 64)?
        .chunks_exact(4)
        .all(|p| p == [0, 0, 255, 255]));
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join("shared-stops.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"adapter":format!("{:?}",adapter.get_info()),"instances":256,"unique_stops":4096,"gradient_table_bytes":196608,"without_stop_sharing_bytes":33619968u64,"frames":45,"warmup":5,"measured_frames":40,"cpu_prepare":percentiles(prepare),"encode_submit_wait":percentiles(gpu),"warm_gpu_allocations":0,"draw_calls":1,"queue_submissions":1,"table_height_limit":limit,"overflow_rejected_without_submit":true,"recovered_after_overflow":true}),
            )?,
        )?;
    }
    Ok(())
}
