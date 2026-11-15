use super::*;
use jadix_2d::text_layout::cosmic_text::{
    self as cosmic, Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache,
};

fn fonts() -> FontSystem {
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    for data in [
        &include_bytes!("../../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        &include_bytes!("../../../jadix_cosmic_text/fonts/NotoSansArabic.ttf")[..],
        &include_bytes!("../../../jadix_cosmic_text/fonts/NotoSansHebrew.ttf")[..],
        &include_bytes!("../../tests/fonts/colr-mini.ttf")[..],
        &include_bytes!("../../../jadix/assets/NotoColorEmoji.ttf")[..],
    ] {
        fonts.db_mut().load_font_data(data.to_vec());
    }
    fonts.db_mut().set_sans_serif_family("Inter");
    fonts
}
fn layout(
    fonts: &mut FontSystem,
    cache: &mut SwashCache,
    mode: TextRenderMode,
    scale: f32,
) -> PreparedText2D {
    let mut buffer = Buffer::new(fonts, Metrics::new(16., 23.));
    buffer.set_size(Some(148.), Some(106.));
    let base = Attrs::new().family(cosmic::Family::Name("Inter"));
    buffer.set_rich_text(
        [
            (
                "office a\u{301} ",
                base.clone()
                    .color(Color::rgba(255, 90, 20, 180))
                    .underline(cosmic::UnderlineStyle::Double),
            ),
            (
                "العربية אבג\n",
                base.clone().color(Color::rgb(20, 160, 250)),
            ),
            (
                "\u{E000} ",
                base.clone()
                    .family(cosmic::Family::Name("Jadix COLR Fixture"))
                    .color(Color::rgba(20, 200, 80, 180)),
            ),
            (
                "😀🚀\n",
                base.clone()
                    .family(cosmic::Family::Name("Noto Color Emoji")),
            ),
            ("Wrap & select", base.clone().strikethrough().overline()),
        ],
        &base,
        Shaping::Advanced,
        None,
    );
    PreparedText2D::new(
        fonts,
        &mut buffer,
        cache,
        TextPrepareOptions {
            mode,
            raster_scale: scale,
            color: Color::rgb(200, 140, 240),
            atlas: AtlasLimits {
                page_size: 256,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap()
}
fn paint(c: &mut dyn Canvas2D, p: &PreparedText2D, variant: usize, mask: &Image2D) {
    if variant == 2 {
        c.clear("#24384cff");
    }
    c.save();
    if variant == 1 {
        c.translate(12., 3.);
        c.rotate(5.);
        c.scale(0.9, 0.9);
    }
    if variant == 3 {
        c.clip_rect(8., 4., 140., 98.);
        c.clip_image_mask(mask);
    }
    if variant == 1 || variant == 4 {
        c.push_layer_with_options(LayerOptions {
            opacity: 0.8,
            filters: if variant == 1 {
                vec![LayerFilter::Gaussian { sigma: [0.5, 1.] }]
            } else {
                vec![
                    LayerFilter::Morphology {
                        radius: [1, 0],
                        operator: Morphology::Dilate,
                    },
                    LayerFilter::DropShadow {
                        sigma: [0.8, 1.5],
                        offset: [1.5, 2.],
                        color: [0.1, 0.2, 0.5, 0.7],
                    },
                ]
            },
            ..Default::default()
        });
    }
    if variant == 2 {
        c.push_layer_with_options(LayerOptions {
            blend: BlendMode::Screen,
            opacity: 0.8,
            ..Default::default()
        });
    }
    c.set_global_alpha(0.9);
    if variant == 5 {
        c.set_fill_brush(Brush2D::vector(
            VectorGradient::linear(
                [0., 0.],
                [148., 0.],
                GradientStops::new(&[(0., [1., 0.2, 0., 0.6]), (1., [0.1, 0.8, 1., 1.])]).unwrap(),
            )
            .unwrap(),
        ));
        c.draw_text_layout_with_brush(p, [4., 1.]).unwrap();
    } else {
        c.draw_text_layout(p, [4., 1.]);
    }
    if matches!(variant, 1 | 2 | 4) {
        c.pop_layer();
    }
    c.restore();
}
fn error(a: &[u8], b: &[u8]) -> (f64, f64) {
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

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn prepared_text_layout_color_atlas_and_vector_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();
    let mut fonts = fonts();
    let mut cache = SwashCache::new();
    let mask = Image2D::from_rgba8(
        160,
        112,
        &(0..160 * 112)
            .flat_map(|i| [0, 0, 0, if i % 160 < 80 { 255 } else { 96 }])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut records = Vec::new();
    let mut preview = vec![0u8; 640 * 448 * 4];
    let mut cell = 0;
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("prepared-text-matrix"),
            size: wgpu::Extent3d {
                width: 160,
                height: 112,
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
        for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
            for scale in [0.75, 1., 2., 3.] {
                let p = layout(&mut fonts, &mut cache, mode, scale);
                assert!(
                    p.glyphs()
                        .iter()
                        .filter(|g| g.color_bitmap || g.color_vector)
                        .count()
                        >= 3,
                    "COLR + real CBDT emoji must render"
                );
                assert!(
                    p.glyphs().iter().all(|g| g.glyph_id != 0),
                    "test text must resolve real fallback glyphs"
                );
                for variant in 0..if mode == TextRenderMode::Outline {
                    6
                } else {
                    5
                } {
                    let mut cpu = RasterCanvas::new_linear(160, 112);
                    paint(&mut cpu, &p, variant, &mask);
                    let expected = cpu.get_image_data(0, 0, 160, 112).unwrap();
                    let mut c = GpuCanvas::new_linear(160, 112, None);
                    paint(&mut c, &p, variant, &mask);
                    assert!(!c.needs_cpu);
                    assert_eq!(c.get_image_data(0, 0, 160, 112).unwrap(), expected);
                    let ops = c.into_ops();
                    assert!(renderer.render(&device, &queue, &view, [160, 112], &ops));
                    let actual = readback_rect(&device, &queue, &texture, 160, 112)?;
                    let (mae, alpha) = error(&actual, &expected);
                    assert!(
                        mae < 1. && alpha < 5.,
                        "{mode:?}/{scale}/{variant}/{samples}: {mae}/{alpha}"
                    );
                    assert!(renderer.render(&device, &queue, &view, [160, 112], &ops));
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
                    if variant == 0 && cell < 16 {
                        for y in 0..112 {
                            let dest = (((cell / 4) * 112 + y) * 640 + (cell % 4) * 160) * 4;
                            preview[dest..dest + 640]
                                .copy_from_slice(&actual[y * 640..(y + 1) * 640]);
                        }
                        cell += 1;
                    }
                    if samples == 4 && scale == 2. && variant == 0 {
                        if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
                            let dir = Path::new(&dir);
                            std::fs::create_dir_all(dir)?;
                            let label = if mode == TextRenderMode::Outline {
                                "outline"
                            } else {
                                "atlas"
                            };
                            image::save_buffer(
                                dir.join(format!("text-{label}-cpu.png")),
                                &expected,
                                160,
                                112,
                                image::ColorType::Rgba8,
                            )?;
                            image::save_buffer(
                                dir.join(format!("text-{label}-gpu.png")),
                                &actual,
                                160,
                                112,
                                image::ColorType::Rgba8,
                            )?;
                        }
                    }
                    records.push(json!({"mode":format!("{mode:?}"),"scale":scale,"variant":variant,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha,"glyphs":p.glyphs().len(),"atlas_entries":p.atlas().regions().len(),"atlas_pages":p.atlas().pages().len(),"atlas_bytes":p.atlas().byte_len(),"warm_gpu_allocations":0,"warm_image_uploads":0}));
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join("text-matrix.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,"cases":records}),
            )?,
        )?;
        image::save_buffer(
            dir.join("text-preview.png"),
            &preview,
            640,
            448,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn font_atlas_instances_reuse_and_batching() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut fonts = fonts();
    let mut cache = SwashCache::new();
    let mut b = Buffer::new(&mut fonts, Metrics::new(12., 16.));
    b.set_text(
        "AAAA",
        &Attrs::new().family(cosmic::Family::Name("Inter")),
        Shaping::Advanced,
        None,
    );
    b.set_size(Some(64.), Some(20.));
    let p = PreparedText2D::new(
        &mut fonts,
        &mut b,
        &mut cache,
        TextPrepareOptions {
            mode: TextRenderMode::Atlas,
            ..Default::default()
        },
    )
    .unwrap();
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas-instances"),
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
    let view = texture.create_view(&Default::default());
    for frame in 0..20 {
        let mut c = GpuCanvas::new_linear(256, 256, None);
        for y in 0..16 {
            for x in 0..4 {
                c.draw_text_layout(&p, [x as f32 * 64. + (frame % 2) as f32, y as f32 * 16.]);
            }
        }
        assert!(renderer.render_premultiplied(&device, &queue, &view, [256, 256], &c.into_ops()));
        let s = renderer.last_stats();
        assert_eq!((s.draw_calls, s.queue_submissions), (1, 1));
        if frame > 0 {
            assert_eq!(
                (
                    s.image_uploads,
                    s.buffer_allocations,
                    s.bind_group_allocations,
                    s.texture_allocations
                ),
                (0, 0, 0, 0)
            );
        }
    }
    let actual = readback(&device, &queue, &texture, 256)?;
    assert!(actual.chunks_exact(4).any(|p| p[3] > 0));
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("text-pressure.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"frames":20,"text_instances":64,"glyph_instances":256,"atlas_pages":p.atlas().pages().len(),"atlas_entries":p.atlas().regions().len(),"draw_calls":1,"submissions":1,"warm_gpu_allocations":0,"warm_image_uploads":0}),
            )?,
        )?;
    }
    Ok(())
}
