use super::scene_tests::{error, texture};
use super::*;

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn colrv1_palettes_variations_composites_and_transforms_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        adapter,
        device,
        queue,
        ..
    } = test_gpu();
    let base = Font2D::new(&include_bytes!("../../tests/fonts/colr-v1.ttf")[..], 0)?;
    let mut records = Vec::new();
    let mut preview = vec![0; 768 * 480 * 4];
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = texture(&device, 96);
        let view = texture.create_view(&Default::default());
        for index in (0..12).chain(13..41) {
            for palette in [0, 1] {
                for variant in [0, 1] {
                    let font = base
                        .with_variations(&[(*b"wght", if variant == 0 { 400. } else { 900. })])?;
                    let run = ColorGlyphRun::new(
                        &font,
                        48.,
                        &[PositionedGlyph {
                            id: font
                                .glyph_id(char::from_u32(0xe100 + index).unwrap())
                                .unwrap(),
                            x: 12.,
                            y: 68.,
                        }],
                        ColorGlyphOptions {
                            palette,
                            foreground: [0.1, 0.9, 0.3, 0.7],
                            ..Default::default()
                        },
                    )?;
                    let paint = |c: &mut dyn Canvas2D| {
                        c.set_fill("#25406080");
                        c.fill_rect(0., 0., 96., 96.);
                        c.set_global_alpha(0.7);
                        c.save();
                        if variant == 1 {
                            c.translate(16., 3.);
                            c.rotate(12.);
                            c.scale(0.8, 0.9);
                            c.clip_rect(6., 6., 72., 72.);
                            c.push_layer_with_options(LayerOptions {
                                opacity: 0.85,
                                filters: vec![LayerFilter::Gaussian { sigma: [0.6, 0.9] }],
                                ..Default::default()
                            });
                        }
                        c.fill_color_glyph_run(&run).unwrap();
                        if variant == 1 {
                            c.pop_layer();
                        }
                        c.restore();
                    };
                    let mut cpu = RasterCanvas::new_linear(96, 96);
                    paint(&mut cpu);
                    let expected = cpu.get_image_data(0, 0, 96, 96).unwrap();
                    let mut c = GpuCanvas::new_linear(96, 96, None);
                    paint(&mut c);
                    assert!(!c.needs_cpu);
                    assert_eq!(c.get_image_data(0, 0, 96, 96).unwrap(), expected);
                    let ops = c.into_ops();
                    assert!(renderer.render(&device, &queue, &view, [96, 96], &ops));
                    let actual = readback(&device, &queue, &texture, 96)?;
                    let (mae, alpha) = error(&actual, &expected);
                    assert!(
                        mae < 1. && alpha < 3.,
                        "COLR {index}/{palette}/{variant}/{samples}: {mae}/{alpha}"
                    );
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
                    if samples == 4 && palette == 0 && variant == 0 {
                        let cell = if index < 12 {
                            index as usize
                        } else {
                            index as usize - 1
                        };
                        for y in 0..96 {
                            let at = ((cell / 8 * 96 + y) * 768 + cell % 8 * 96) * 4;
                            preview[at..at + 384].copy_from_slice(&actual[y * 384..(y + 1) * 384]);
                        }
                    }
                    records.push(json!({"glyph":index,"palette":palette,"variant":variant,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha,"warm_prepared_ops_allocations":0,"warm_prepared_ops_uploads":0}));
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        let dir = Path::new(&dir);
        std::fs::write(
            dir.join("colrv1-matrix.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"adapter":adapter.get_info().name,"cases":records}),
            )?,
        )?;
        image::save_buffer(
            dir.join("colrv1-preview.png"),
            &preview,
            768,
            480,
            image::ColorType::Rgba8,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn interpolation_spaces_and_extended_gradient_domains_matrix() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let stops = GradientStops::new(&[
        (0., [1., 0., 0., 0.]),
        (0.4, [0., 1., 0., 0.7]),
        (0.4, [1., 0.2, 0., 0.8]),
        (1., [0.1, 0., 1., 1.]),
    ])?
    .with_midpoints(&[Some(0.3), None, Some(0.7)])?;
    let mut records = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = texture(&device, 64);
        let view = texture.create_view(&Default::default());
        for space in [
            GradientSpace::LinearSrgb,
            GradientSpace::Srgb,
            GradientSpace::Oklab,
        ] {
            for alpha_mode in [GradientAlpha::Premultiplied, GradientAlpha::Straight] {
                for extend in [
                    ExtendMode::Pad,
                    ExtendMode::Repeat,
                    ExtendMode::Reflect,
                    ExtendMode::Decal,
                ] {
                    for geometry in 0..5 {
                        let g = match geometry {
                            0 => VectorGradient::linear([16., 8.], [48., 54.], stops.clone())?,
                            1 => VectorGradient::radial(
                                [22., 22.],
                                2.,
                                [32., 32.],
                                24.,
                                stops.clone(),
                            )?,
                            2 => VectorGradient::sweep([32., 32.], 20., 320., stops.clone())?,
                            3 => {
                                VectorGradient::sweep_range([32., 32.], -90., 630., stops.clone())?
                            }
                            _ => VectorGradient::radial_signed(
                                [24., 24.],
                                -12.,
                                [36., 36.],
                                20.,
                                stops.clone(),
                            )?,
                        }
                        .with_color_space(space)
                        .with_alpha_mode(alpha_mode)
                        .with_extend(extend);
                        let paint = |c: &mut dyn Canvas2D| {
                            c.set_fill_brush(Brush2D::vector(g.clone()));
                            c.fill_rect(0., 0., 64., 64.);
                        };
                        let mut cpu = RasterCanvas::new_linear(64, 64);
                        paint(&mut cpu);
                        let expected = cpu.get_image_data(0, 0, 64, 64).unwrap();
                        let mut c = GpuCanvas::new_linear(64, 64, None);
                        paint(&mut c);
                        let ops = c.into_ops();
                        assert!(renderer.render(&device, &queue, &view, [64, 64], &ops));
                        let actual = readback(&device, &queue, &texture, 64)?;
                        let (mae, alpha) = error(&actual, &expected);
                        assert!(mae<0.6 && alpha<2.,"{space:?}/{alpha_mode:?}/{extend:?}/{geometry}/{samples}: {mae}/{alpha}");
                        records.push(json!({"space":format!("{space:?}"),"alpha":format!("{alpha_mode:?}"),"extend":format!("{extend:?}"),"geometry":geometry,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha}));
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("gradient-spaces-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":records}))?,
        )?;
    }
    Ok(())
}

#[cfg(all(feature = "text-layout", feature = "svg-fonts"))]
#[test]
#[ignore = "requires a native headless GPU adapter"]
fn real_color_fonts_layout_vector_atlas_svgz_and_palette_matrix() -> Result<(), Box<dyn Error>> {
    use jadix_2d::text_layout::cosmic_text::{
        self as cosmic, Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache,
    };
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    for data in [
        &include_bytes!("../../tests/fonts/Nabla.ttf")[..],
        &include_bytes!("../../tests/fonts/colr-v1.ttf")[..],
        &include_bytes!("../../tests/fonts/svg-mini.ttf")[..],
    ] {
        fonts.db_mut().load_font_data(data.to_vec());
    }
    let mut cache = SwashCache::new();
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(20., 28.));
    buffer.set_size(Some(192.), Some(100.));
    buffer.set_rich_text(
        [
            ("AB12\n", Attrs::new().family(cosmic::Family::Name("Nabla"))),
            (
                "\u{e102}\u{e103}\u{e104}\u{e10b}\n",
                Attrs::new()
                    .family(cosmic::Family::Name("Jadix COLRv1 Fixture"))
                    .weight(cosmic::Weight(900)),
            ),
            (
                "\u{e100}\u{e101}\u{e102}",
                Attrs::new().family(cosmic::Family::Name("Jadix SVG Fixture")),
            ),
        ],
        &Attrs::new(),
        Shaping::Advanced,
        None,
    );
    let mut records = Vec::new();
    for samples in [1, 4] {
        let mut renderer = PathGpu::try_new(&device, samples).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Reference);
        let texture = texture(&device, 208);
        let view = texture.create_view(&Default::default());
        for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
            for scale in [1., 2., 3.] {
                for palette in [0, 1] {
                    for transformed in [false, true] {
                        let p = PreparedText2D::new(
                            &mut fonts,
                            &mut buffer,
                            &mut cache,
                            TextPrepareOptions {
                                mode,
                                raster_scale: scale,
                                palette,
                                color: Color::rgba(80, 240, 120, 180),
                                ..Default::default()
                            },
                        )?;
                        assert_eq!(p.glyphs().len(), 11);
                        assert!(p.glyphs().iter().all(|g| g.glyph_id != 0));
                        assert!(p.glyphs().iter().all(|g| g.color_bitmap || g.color_vector));
                        if mode == TextRenderMode::Outline {
                            assert_eq!(p.glyphs().iter().filter(|g| g.color_vector).count(), 8);
                        }
                        let paint = |c: &mut dyn Canvas2D| {
                            c.set_global_alpha(0.8);
                            c.translate(6., 26.);
                            if transformed {
                                c.rotate(10.);
                                c.push_layer_with_options(LayerOptions {
                                    opacity: 0.8,
                                    filters: vec![LayerFilter::DropShadow {
                                        sigma: [1., 1.5],
                                        offset: [2., 3.],
                                        color: [0., 0.3, 1., 0.5],
                                    }],
                                    ..Default::default()
                                });
                            }
                            p.try_draw(c, [0., 0.]).unwrap();
                            if transformed {
                                c.pop_layer();
                            }
                        };
                        let mut cpu = RasterCanvas::new_linear(208, 208);
                        paint(&mut cpu);
                        let expected = cpu.get_image_data(0, 0, 208, 208).unwrap();
                        let mut c = GpuCanvas::new_linear(208, 208, None);
                        paint(&mut c);
                        assert!(!c.needs_cpu);
                        assert_eq!(c.get_image_data(0, 0, 208, 208).unwrap(), expected);
                        assert!(renderer.render(&device, &queue, &view, [208, 208], &c.into_ops()));
                        let actual = readback(&device, &queue, &texture, 208)?;
                        let (mae, alpha) = error(&actual, &expected);
                        assert!(
                            mae < 1. && alpha < 5.,
                            "{mode:?}/{scale}/{palette}/{transformed}/{samples}: {mae}/{alpha}"
                        );
                        if samples == 4 && scale == 2. && palette == 0 && !transformed {
                            if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
                                std::fs::create_dir_all(&dir)?;
                                let label = if mode == TextRenderMode::Outline {
                                    "outline"
                                } else {
                                    "atlas"
                                };
                                image::save_buffer(
                                    Path::new(&dir).join(format!("real-font-{label}.png")),
                                    &actual,
                                    208,
                                    208,
                                    image::ColorType::Rgba8,
                                )?;
                            }
                        }
                        records.push(json!({"mode":format!("{mode:?}"),"raster_scale":scale,"palette":palette,"transformed":transformed,"samples":samples,"premultiplied_mae":mae,"ink_alpha_mae":alpha,"atlas_pages":p.atlas().pages().len()}));
                    }
                }
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("real-font-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":records}))?,
        )?;
    }
    Ok(())
}
