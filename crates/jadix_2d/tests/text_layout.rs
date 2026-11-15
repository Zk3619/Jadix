#![cfg(feature = "text-layout")]
use jadix_2d::text_layout::cosmic_text::{
    self as cosmic, Attrs, Buffer, Color, Cursor, FontSystem, Metrics, Shaping, SwashCache,
};
use jadix_2d::*;
fn fonts() -> FontSystem {
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    for data in [
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        &include_bytes!("../../jadix_cosmic_text/fonts/NotoSansArabic.ttf")[..],
        &include_bytes!("../../jadix_cosmic_text/fonts/NotoSansHebrew.ttf")[..],
        &include_bytes!("fonts/colr-mini.ttf")[..],
    ] {
        fonts.db_mut().load_font_data(data.to_vec());
    }
    fonts.db_mut().set_sans_serif_family("Inter");
    fonts
}
fn buffer(fonts: &mut FontSystem, text: &str) -> Buffer {
    let mut b = Buffer::new(fonts, Metrics::new(24., 32.));
    b.set_size(Some(240.), Some(160.));
    b.set_text(
        text,
        &Attrs::new().family(cosmic::Family::Name("Inter")),
        Shaping::Advanced,
        None,
    );
    b
}
#[test]
fn complex_layout_fallback_bidi_clusters_and_interaction_snapshot() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let text = "office a\u{301} العربية אבג\nSecond line wrapped to fit more words";
    let mut b = buffer(&mut f, text);
    let p = PreparedText2D::new(&mut f, &mut b, &mut cache, TextPrepareOptions::default()).unwrap();
    assert!(p.line_count() >= 3);
    assert!(p.glyphs().iter().any(|g| g.bidi_level % 2 == 1));
    assert!(p.glyphs().iter().any(|g| g.cluster.len() > 1));
    let mut ids: Vec<_> = p.glyphs().iter().map(|g| g.font_id).collect();
    ids.sort();
    ids.dedup();
    assert!(ids.len() >= 3);
    for g in p.glyphs() {
        let line = &b.lines[g.line].text();
        assert!(line.is_char_boundary(g.cluster.start) && line.is_char_boundary(g.cluster.end));
    }
    let cursor = Cursor::new(0, 2);
    assert_eq!(p.hit([20., 15.]), b.hit(20., 15.));
    let rects = p.selection_rects(
        Cursor::new(0, 0),
        Cursor::new(0, text.lines().next().unwrap().len()),
    );
    assert!(!rects.is_empty());
    assert!(rects.iter().all(|r| r[2] > 0. && r[3] > 0.));
    assert!(p.caret_rect(cursor).is_some());
    let before = p.hit([20., 15.]);
    b.set_text("changed", &Attrs::new(), Shaping::Advanced, None);
    b.shape_until_scroll(&mut f, false);
    assert_eq!(p.hit([20., 15.]), before);
    assert!(p.hit([f32::NAN, 0.]).is_none());
}
#[test]
fn colr_foreground_palette_alpha_and_raster_atlas() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = buffer(&mut f, "\u{E000}\u{E000}");
    b.set_text(
        "\u{E000}\u{E000}",
        &Attrs::new().family(cosmic::Family::Name("Jadix COLR Fixture")),
        Shaping::Advanced,
        None,
    );
    for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
        let p = PreparedText2D::new(
            &mut f,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                mode,
                color: Color::rgba(20, 180, 60, 128),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.glyphs().len(), 2);
        assert!(p
            .glyphs()
            .iter()
            .all(|g| if mode == TextRenderMode::Outline {
                g.color_vector && !g.color_bitmap
            } else {
                g.color_bitmap && !g.color_vector
            }));
        assert_eq!(
            p.atlas().pages().len(),
            usize::from(mode == TextRenderMode::Atlas)
        );
        let mut sample = RasterCanvas::new_linear(80, 48);
        p.try_draw(&mut sample, [2., 3.]).unwrap();
        let sample_pixels = sample.get_image_data(0, 0, 80, 48).unwrap();
        let pixels = if mode == TextRenderMode::Atlas {
            p.atlas().pages()[0].rgba8()
        } else {
            &sample_pixels
        };
        assert!(
            pixels
                .chunks_exact(4)
                .any(|c| c[0] > 245 && c[1] == 0 && c[3] > 50 && c[3] < 80),
            "COLR palette must be unpremultiplied before Image2D upload"
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|c| c[1] > 160 && c[0] < 30 && c[3] > 110),
            "foreground must use prepared text color"
        );
        let mut c = RasterCanvas::new_linear(80, 48);
        c.draw_text_layout(&p, [2., 3.]);
        assert!(c
            .get_image_data(0, 0, 80, 48)
            .unwrap()
            .chunks_exact(4)
            .any(|p| p[3] > 0));
        assert_eq!(p.clone().atlas().pages(), p.atlas().pages());
    }
}
#[test]
fn modes_colors_decorations_state_and_software_continuation() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = buffer(&mut f, "AA AA");
    b.set_rich_text(
        [
            (
                "AA",
                Attrs::new()
                    .color(Color::rgba(240, 80, 20, 160))
                    .underline(cosmic::UnderlineStyle::Double),
            ),
            (
                " AA",
                Attrs::new().color(Color::rgb(20, 80, 240)).strikethrough(),
            ),
        ],
        &Attrs::new().family(cosmic::Family::Name("Inter")),
        Shaping::Advanced,
        None,
    );
    for mode in [TextRenderMode::Outline, TextRenderMode::Atlas] {
        let p = PreparedText2D::new(
            &mut f,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                mode,
                raster_scale: 2.,
                ..Default::default()
            },
        )
        .unwrap();
        if mode == TextRenderMode::Outline {
            assert!(p.atlas().pages().is_empty());
        } else {
            assert!(!p.atlas().pages().is_empty());
            assert!(p.atlas().regions().len() < p.glyphs().len());
        }
        let mut brush = RasterCanvas::new_linear(160, 80);
        brush.set_fill_brush(Brush2D::vector(
            VectorGradient::linear(
                [0., 0.],
                [100., 0.],
                GradientStops::new(&[(0., [1., 0., 0., 1.]), (1., [0., 0., 1., 1.])]).unwrap(),
            )
            .unwrap(),
        ));
        let result = brush.draw_text_layout_with_brush(&p, [0., 0.]);
        if mode == TextRenderMode::Outline {
            assert!(result.is_ok());
            assert!(brush.into_pixels().iter().any(|v| *v > 0.));
        } else {
            assert_eq!(result, Err(TextLayoutError::BrushRequiresOutlines));
            assert!(brush.into_pixels().iter().all(|v| *v == 0.));
        }
        let paint = |c: &mut dyn Canvas2D| {
            c.set_fill("green");
            c.save();
            c.translate(5., 7.);
            c.rotate(8.);
            c.clip_rect(0., 0., 125., 64.);
            c.push_layer_with_options(LayerOptions {
                opacity: 0.8,
                filters: vec![LayerFilter::Gaussian { sigma: [0.5, 1.] }],
                ..Default::default()
            });
            c.set_global_alpha(0.7);
            c.draw_text_layout(&p, [3., 1.]);
            c.pop_layer();
            c.restore();
            c.fill_rect(140., 70., 2., 2.);
        };
        let mut c = RasterCanvas::new_linear(160, 80);
        paint(&mut c);
        let mut g = GpuCanvas::new_linear(160, 80, None);
        paint(&mut g);
        assert!(!g.needs_cpu);
        assert_eq!(
            c.get_image_data(0, 0, 160, 80),
            g.get_image_data(0, 0, 160, 80)
        );
        c.put_image_data(0, 0, 1, 1, &[255; 4]);
        g.put_image_data(0, 0, 1, 1, &[255; 4]);
        assert_eq!(c.into_pixels(), g.into_raster());
    }
}
#[test]
fn preparation_limits_fail_explicitly() {
    let mut f = fonts();
    let mut cache = SwashCache::new();
    let mut b = buffer(&mut f, "abc");
    for scale in [f32::NAN, 0., 9.] {
        assert_eq!(
            PreparedText2D::new(
                &mut f,
                &mut b,
                &mut cache,
                TextPrepareOptions {
                    raster_scale: scale,
                    ..Default::default()
                }
            )
            .unwrap_err(),
            TextLayoutError::InvalidOptions
        );
    }
    assert_eq!(
        PreparedText2D::new(
            &mut f,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                max_glyphs: 1,
                ..Default::default()
            }
        )
        .unwrap_err(),
        TextLayoutError::GlyphBudget
    );
    assert!(matches!(
        PreparedText2D::new(
            &mut f,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                mode: TextRenderMode::Atlas,
                atlas: AtlasLimits {
                    page_size: 8,
                    ..Default::default()
                },
                ..Default::default()
            }
        ),
        Err(TextLayoutError::Atlas(AtlasError::EntryTooLarge))
    ));
}

#[test]
fn variable_weight_and_italic_keep_shaped_font_instance() {
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    fonts
        .db_mut()
        .load_font_data(include_bytes!("../../jadix_cosmic_text/fonts/InterVariable.ttf").to_vec());
    let family = fonts.db().faces().next().unwrap().families[0].0.clone();
    let mut cache = SwashCache::new();
    let mut hashes = Vec::new();
    for (weight, style) in [
        (100, cosmic::Style::Normal),
        (900, cosmic::Style::Normal),
        (100, cosmic::Style::Italic),
    ] {
        let mut b = Buffer::new(&mut fonts, Metrics::new(32., 40.));
        b.set_size(Some(120.), Some(48.));
        b.set_text(
            "ABC",
            &Attrs::new()
                .family(cosmic::Family::Name(&family))
                .weight(cosmic::Weight(weight))
                .style(style),
            Shaping::Advanced,
            None,
        );
        let p = PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions::default(),
        )
        .unwrap();
        let mut c = RasterCanvas::new_linear(128, 48);
        c.draw_text_layout(&p, [2., 0.]);
        hashes.push(c.get_image_data(0, 0, 128, 48).unwrap());
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[0], hashes[2]);
}

#[test]
fn formerly_unsupported_colrv1_now_prepares_and_renders() {
    let mut fonts =
        FontSystem::new_with_locale_and_db("en-US".into(), cosmic::fontdb::Database::new());
    fonts
        .db_mut()
        .load_font_data(include_bytes!("fonts/colr-v1-unsupported.ttf").to_vec());
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(24., 32.));
    buffer.set_text(
        "\u{E000}",
        &Attrs::new().family(cosmic::Family::Name("Jadix COLR Fixture")),
        Shaping::Advanced,
        None,
    );
    let mut cache = SwashCache::new();
    let prepared = PreparedText2D::new(
        &mut fonts,
        &mut buffer,
        &mut cache,
        TextPrepareOptions::default(),
    )
    .unwrap();
    assert!(prepared.glyphs()[0].color_vector);
    let mut c = RasterCanvas::new_linear(64, 48);
    prepared.try_draw(&mut c, [2., 2.]).unwrap();
    assert!(c
        .get_image_data(0, 0, 64, 48)
        .unwrap()
        .chunks_exact(4)
        .any(|p| p[0] == 255 && p[3] > 0));
}

#[test]
fn colrv1_palette_errors_and_atomic_draw_preflight() {
    let mut fonts = fonts();
    fonts
        .db_mut()
        .load_font_data(include_bytes!("fonts/colr-v1.ttf").to_vec());
    let mut b = Buffer::new(&mut fonts, Metrics::new(24., 32.));
    b.set_size(Some(160.), Some(48.));
    b.set_rich_text(
        [
            ("A", Attrs::new().family(cosmic::Family::Name("Inter"))),
            (
                "\u{e100}",
                Attrs::new().family(cosmic::Family::Name("Jadix COLRv1 Fixture")),
            ),
        ],
        &Attrs::new(),
        Shaping::Advanced,
        None,
    );
    let mut cache = SwashCache::new();
    let mut outputs = Vec::new();
    for palette in [0, 1] {
        let p = PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                palette,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(p.glyphs().iter().any(|g| g.color_vector));
        let mut c = RasterCanvas::new_linear(160, 48);
        c.scale(0., 1.);
        assert_eq!(
            p.try_draw(&mut c, [0., 0.]),
            Err(TextLayoutError::Scene(SceneError::InvalidTransform))
        );
        assert!(c.buf.iter().all(|v| *v == 0.));
        c.reset_transform();
        p.try_draw(&mut c, [0., 0.]).unwrap();
        outputs.push(c.into_pixels());
    }
    assert_ne!(outputs[0], outputs[1]);
    assert_eq!(
        PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions {
                palette: 2,
                ..Default::default()
            }
        )
        .unwrap_err(),
        TextLayoutError::Color(ColorGlyphError::InvalidPalette)
    );
    b.set_text(
        "\u{e10c}",
        &Attrs::new().family(cosmic::Family::Name("Jadix COLRv1 Fixture")),
        Shaping::Advanced,
        None,
    );
    assert_eq!(
        PreparedText2D::new(
            &mut fonts,
            &mut b,
            &mut cache,
            TextPrepareOptions::default()
        )
        .unwrap_err(),
        TextLayoutError::Color(ColorGlyphError::MalformedPaint)
    );
}
