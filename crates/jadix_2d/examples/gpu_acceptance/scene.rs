use super::*;

pub(super) fn error(a: &[u8], b: &[u8]) -> (f64, f64) {
    let linear = jadix_theme::color_space::linearize_display_srgb_component;
    let mut premul = 0.;
    let mut alpha = 0.;
    let mut ink = 0;
    for (a, b) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let delta = f64::from(a[3].abs_diff(b[3]));
        premul += delta;
        if a[3] > 0 || b[3] > 0 {
            alpha += delta;
            ink += 1;
        }
        for k in 0..3 {
            premul += f64::from(
                (linear(a[k] as f32 / 255.) * a[3] as f32
                    - linear(b[k] as f32 / 255.) * b[3] as f32)
                    .abs(),
            );
        }
    }
    (premul / a.len() as f64, alpha / ink.max(1) as f64)
}

fn motif(c: &mut dyn Canvas2D, variant: usize, image: &Image2D, glyphs: &GlyphRun) {
    if variant == 4 {
        c.clip_path(&filled_circle(22., 22., 22., 32), FillRule::NonZero);
    }
    if variant == 3 {
        c.push_layer_with_options(LayerOptions {
            filters: vec![
                LayerFilter::Morphology {
                    radius: [1, 0],
                    operator: Morphology::Dilate,
                },
                LayerFilter::Gaussian { sigma: [0.7, 1.2] },
                LayerFilter::Offset { offset: [1., -1.] },
            ],
            ..Default::default()
        });
    }
    let stops = GradientStops::new(&[
        (0., [1., 0.15, 0.05, 0.8]),
        (0.4, [0.2, 0.8, 0.3, 0.6]),
        (0.4, [0.1, 0.4, 1., 0.9]),
        (1., [0.8, 0.2, 0.9, 0.8]),
    ])
    .unwrap();
    c.set_fill_brush(Brush2D::vector(
        VectorGradient::linear([2., 2.], [38., 28.], stops).unwrap(),
    ));
    c.fill_path(&filled_circle(20., 18., 16., 32));
    c.set_stroke("#ffffffc0");
    c.set_line_width(1.5);
    c.set_line_dash(&[3., 2.]);
    c.stroke_path(&polyline(&[(2., 32.), (20., 5.), (38., 32.)]));
    if variant == 1 {
        c.draw_rounded_rect_shadow(
            [4., 5., 32., 24.],
            [3., 8., 2., 6.],
            2.,
            [0.8, 0.1, 0.2, 0.6],
            true,
        );
    }
    c.set_image_sampling(ImageSampling::Bilinear);
    c.draw_image(
        &CanvasImageSource::Region {
            image,
            rect: [1, 0, 2, 2],
        },
        25.,
        24.,
        12.,
        12.,
    );
    c.set_fill("#ffe090");
    c.fill_glyph_run(glyphs);
    if variant == 3 {
        c.pop_layer();
    }
}
fn resources() -> (Image2D, GlyphRun) {
    let image = Image2D::from_rgba8(
        4,
        2,
        &[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 255, 255, 0, 255, 255, 0, 0, 255, 255,
            255, 255, 200, 100, 40, 200, 128, 255, 255, 0, 255,
        ],
    )
    .unwrap();
    let font = Font2D::new(
        &include_bytes!("../../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )
    .unwrap();
    let glyphs = GlyphRun::new(
        &font,
        12.,
        &[PositionedGlyph {
            id: font.glyph_id('A').unwrap(),
            x: 4.,
            y: 39.,
        }],
    )
    .unwrap();
    (image, glyphs)
}
pub(super) fn texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reusable-scene"),
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
    })
}
#[test]
#[ignore = "requires a native headless GPU adapter"]
fn frozen_nested_scene_content_transform_filter_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();
    let (image, glyphs) = resources();
    let mask = Image2D::from_rgba8(
        44,
        44,
        &(0..44 * 44)
            .flat_map(|i| [0, 0, 0, if i % 44 < 24 { 255 } else { 112 }])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let transforms = [
        [1., 0., 0., 1., 12., 14.],
        [0.9, 0.25, -0.25, 0.9, 24., 8.],
        [-1., 0., 0., 1., 67., 14.],
        [1.1, 0.1, 0.3, 0.8, 10., 17.],
    ];
    let mut records = Vec::new();
    let mut preview = vec![0; 384 * 384 * 4];
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = texture(&device, 96);
        let view = texture.create_view(&Default::default());
        for variant in 0..6 {
            let recordings = std::cell::Cell::new(0);
            let child = Scene2D::record([44, 44], |c| {
                recordings.set(recordings.get() + 1);
                if variant == 5 {
                    c.clip_image_mask(&mask);
                    c.push_layer_with_options(LayerOptions {
                        clip: Some([3., 4., 36., 34.]),
                        mask: Some(mask.clone()),
                        ..Default::default()
                    });
                }
                motif(c, variant, &image, &glyphs);
                if variant == 5 {
                    c.pop_layer();
                }
            })?;
            let parent = Scene2D::record([60, 60], |c| {
                c.translate(3., 2.);
                child.draw(c, SceneOptions::default()).unwrap();
                c.reset_transform();
                c.set_fill("#80ff80");
                c.fill_rect(45., 43., 5., 6.);
            })?;
            for (ti, transform) in transforms.iter().enumerate() {
                for isolated in [false, true] {
                    for opaque in [false, true] {
                        let options = SceneOptions {
                            transform: *transform,
                            clip_to_bounds: variant == 2,
                            layer: isolated.then(|| LayerOptions {
                                opacity: 0.7,
                                filters: if variant == 2 {
                                    vec![LayerFilter::DropShadow {
                                        offset: [2., 3.],
                                        sigma: [1., 1.5],
                                        color: [0., 0., 0., 0.7],
                                    }]
                                } else {
                                    vec![]
                                },
                                ..Default::default()
                            }),
                        };
                        let paint = |c: &mut dyn Canvas2D| {
                            if opaque {
                                c.set_fill("#284060");
                                c.fill_rect(0., 0., 96., 96.);
                            }
                            c.set_global_alpha(0.8);
                            c.clip_rect(6., 6., 82., 82.);
                            parent.draw(c, options.clone()).unwrap();
                            assert_eq!(c.current_transform(), Some([1., 0., 0., 1., 0., 0.]));
                            assert_eq!(c.current_global_alpha(), Some(0.8));
                        };
                        let mut cpu = RasterCanvas::new_linear(96, 96);
                        paint(&mut cpu);
                        let expected = cpu.get_image_data(0, 0, 96, 96).unwrap();
                        let mut canvas = GpuCanvas::new_linear(96, 96, None);
                        paint(&mut canvas);
                        assert!(!canvas.needs_cpu);
                        assert_eq!(canvas.get_image_data(0, 0, 96, 96).unwrap(), expected);
                        let ops = canvas.into_ops();
                        assert!(renderer.render(&device, &queue, &view, [96, 96], &ops));
                        let actual = readback(&device, &queue, &texture, 96)?;
                        let (mae, alpha) = error(&actual, &expected);
                        assert!(mae < 1. && alpha < 4., "variant={variant}, transform={ti}, isolated={isolated}, samples={samples}: {mae}/{alpha}");
                        assert!(renderer.render(&device, &queue, &view, [96, 96], &ops));
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
                        if samples == 4 && variant < 4 && isolated && opaque {
                            for y in 0..96 {
                                let dest = ((variant * 96 + y) * 384 + ti * 96) * 4;
                                preview[dest..dest + 384]
                                    .copy_from_slice(&actual[y * 384..(y + 1) * 384]);
                            }
                        }
                        records.push(json!({"variant":variant,"transform":ti,"isolated":isolated,"opaque":opaque,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha,"warm_prepared_ops_gpu_allocations":0,"warm_prepared_ops_image_uploads":0}));
                    }
                }
            }
            assert_eq!(recordings.get(), 1);
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join("scene-matrix.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,"cases":records}),
            )?,
        )?;
        image::save_buffer(
            dir.join("scene-preview.png"),
            &preview,
            384,
            384,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn frozen_scene_instances_share_gradient_and_gpu_resources() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let stops = GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap();
    let child = Scene2D::record([16, 16], |c| {
        c.set_fill_brush(Brush2D::vector(
            VectorGradient::linear([0., 0.], [16., 16.], stops).unwrap(),
        ));
        for i in 0..4 {
            c.fill_rect((i % 2 * 8) as f32, (i / 2 * 8) as f32, 6., 6.);
        }
    })?;
    let scene = Scene2D::record([256, 256], |c| {
        for y in 0..16 {
            for x in 0..16 {
                child
                    .draw(
                        c,
                        SceneOptions {
                            transform: [1., 0., 0., 1., x as f32 * 16., y as f32 * 16.],
                            ..Default::default()
                        },
                    )
                    .unwrap();
            }
        }
    })?;
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let texture = texture(&device, 256);
    let view = texture.create_view(&Default::default());
    for frame in 0..20 {
        let mut c = GpuCanvas::new_linear(256, 256, None);
        scene.draw(
            &mut c,
            SceneOptions {
                transform: [1., 0., 0., 1., (frame % 2) as f32, 0.],
                ..Default::default()
            },
        )?;
        assert!(renderer.render_premultiplied(&device, &queue, &view, [256, 256], &c.into_ops()));
        let s = renderer.last_stats();
        assert_eq!((s.draw_calls, s.queue_submissions), (1, 1));
        if frame > 0 {
            assert_eq!(
                (
                    s.buffer_allocations,
                    s.texture_allocations,
                    s.bind_group_allocations,
                    s.image_uploads
                ),
                (0, 0, 0, 0)
            );
        }
    }
    let actual = readback(&device, &queue, &texture, 256)?;
    assert!(actual.chunks_exact(4).filter(|p| p[3] > 0).count() > 30_000);
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("scene-pressure.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"frames":20,"shared_child_instances":256,"gradient_rectangles":1024,"retained_parent_commands":scene.stats().commands,"expanded_commands":scene.stats().expanded_commands,"draw_calls":1,"submissions":1,"warm_gpu_allocations":0,"warm_image_uploads":0,"hardware_instancing":false}),
            )?,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn frozen_scene_isolated_blend_and_porter_duff_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let scene = Scene2D::record([32, 32], |c| {
        c.set_fill("#e06020a0");
        c.fill_rect(0., 0., 24., 24.);
        c.set_fill("#60c0e080");
        c.fill_rect(8., 8., 24., 24.);
    })?;
    let compose = [
        CompositeMode::Clear,
        CompositeMode::Copy,
        CompositeMode::Destination,
        CompositeMode::SourceOver,
        CompositeMode::DestinationOver,
        CompositeMode::SourceIn,
        CompositeMode::DestinationIn,
        CompositeMode::SourceOut,
        CompositeMode::DestinationOut,
        CompositeMode::SourceAtop,
        CompositeMode::DestinationAtop,
        CompositeMode::Xor,
        CompositeMode::Plus,
        CompositeMode::PlusLighter,
        CompositeMode::LuminanceIn,
        CompositeMode::LuminanceOut,
    ];
    let blends = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    let mut records = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = texture(&device, 64);
        let view = texture.create_view(&Default::default());
        for group in 0..2 {
            for i in 0..16 {
                let layer = LayerOptions {
                    opacity: 0.65,
                    clip: Some([11., 9., 38., 42.]),
                    compose: if group == 0 {
                        compose[i]
                    } else {
                        CompositeMode::SourceOver
                    },
                    blend: if group == 1 {
                        blends[i]
                    } else {
                        BlendMode::Normal
                    },
                    ..Default::default()
                };
                let paint = |c: &mut dyn Canvas2D, frozen: bool| {
                    c.set_fill("#2080d080");
                    c.fill_rect(0., 0., 64., 64.);
                    c.set_global_alpha(0.7);
                    if frozen {
                        scene
                            .draw(
                                c,
                                SceneOptions {
                                    transform: [1., 0., 0., 1., 16., 16.],
                                    layer: Some(layer.clone()),
                                    ..Default::default()
                                },
                            )
                            .unwrap();
                    } else {
                        c.save();
                        let mut layer = layer.clone();
                        layer.opacity *= 0.7;
                        c.push_layer_with_options(layer);
                        c.set_global_alpha(1.);
                        c.set_fill("#e06020a0");
                        c.fill_rect(16., 16., 24., 24.);
                        c.set_fill("#60c0e080");
                        c.fill_rect(24., 24., 24., 24.);
                        c.pop_layer();
                        c.restore();
                    }
                };
                let mut cpu = RasterCanvas::new_linear(64, 64);
                paint(&mut cpu, false);
                let expected = cpu.get_image_data(0, 0, 64, 64).unwrap();
                let mut c = GpuCanvas::new_linear(64, 64, None);
                paint(&mut c, true);
                assert_eq!(c.get_image_data(0, 0, 64, 64).unwrap(), expected);
                assert!(renderer.render(&device, &queue, &view, [64, 64], &c.into_ops()));
                let actual = readback(&device, &queue, &texture, 64)?;
                let (mae, alpha) = error(&actual, &expected);
                assert!(
                    mae < 0.6 && alpha < 1.,
                    "group={group}, mode={i}, samples={samples}: {mae}/{alpha}"
                );
                records.push(json!({"group":group,"mode":i,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha}));
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("scene-composite.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":records}))?,
        )?;
    }
    Ok(())
}
