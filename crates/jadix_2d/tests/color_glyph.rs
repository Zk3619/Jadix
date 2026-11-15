use jadix_2d::*;
fn font() -> Font2D {
    Font2D::new(&include_bytes!("fonts/colr-v1.ttf")[..], 0).unwrap()
}
fn run(index: u32, palette: u16) -> ColorGlyphRun {
    let font = font();
    ColorGlyphRun::new(
        &font,
        40.,
        &[PositionedGlyph {
            id: font
                .glyph_id(char::from_u32(0xe100 + index).unwrap())
                .unwrap(),
            x: 8.,
            y: 48.,
        }],
        ColorGlyphOptions {
            palette,
            foreground: [0., 1., 0., 1.],
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
fn palette_foreground_alpha_and_variable_paint_are_resolved_once() {
    for (index, palette, expected) in [
        (0, 0, [255, 0, 0, 64]),
        (0, 1, [0, 255, 0, 128]),
        (1, 0, [0, 255, 0, 128]),
    ] {
        let r = run(index, palette);
        let mut c = RasterCanvas::new_linear(64, 64);
        c.set_global_alpha(0.5);
        r.draw(&mut c).unwrap();
        let p = c.get_image_data(12, 30, 1, 1).unwrap();
        assert_eq!(p, expected);
        assert_eq!(c.current_global_alpha(), Some(0.5));
    }
    let font = font();
    let id = font.glyph_id('\u{e10b}').unwrap();
    let mut pixels = Vec::new();
    for weight in [400., 900.] {
        let f = font.with_variations(&[(*b"wght", weight)]).unwrap();
        let r = ColorGlyphRun::new(
            &f,
            40.,
            &[PositionedGlyph { id, x: 8., y: 48. }],
            ColorGlyphOptions::default(),
        )
        .unwrap();
        let mut c = RasterCanvas::new_linear(64, 64);
        r.draw(&mut c).unwrap();
        pixels.push(c.get_image_data(0, 0, 64, 64).unwrap());
    }
    assert_ne!(pixels[0], pixels[1]);
    assert_eq!(pixels[0][(30 * 64 + 10) * 4 + 3], 255);
    assert_eq!(pixels[1][(30 * 64 + 10) * 4 + 3], 0);
}
#[test]
fn all_colr_paint_families_and_composites_survive_gpu_recording_and_scene_reuse() {
    for index in (0..12).chain(13..41) {
        let r = run(index, 0);
        let clone = r.clone();
        drop(r);
        let scene = Scene2D::record([64, 64], |c| clone.draw(c).unwrap()).unwrap();
        let paint = |c: &mut dyn Canvas2D| {
            c.set_fill("#20406080");
            c.fill_rect(0., 0., 64., 64.);
            scene.draw(c, SceneOptions::default()).unwrap();
        };
        let mut c = RasterCanvas::new_linear(64, 64);
        paint(&mut c);
        let expected = c.get_image_data(0, 0, 64, 64).unwrap();
        let mut g = GpuCanvas::new_linear(64, 64, None);
        paint(&mut g);
        assert!(!g.needs_cpu);
        assert_eq!(
            g.get_image_data(0, 0, 64, 64).unwrap(),
            expected,
            "glyph {index}"
        );
        c.put_image_data(0, 0, 1, 1, &[255; 4]);
        g.put_image_data(0, 0, 1, 1, &[255; 4]);
        assert_eq!(g.into_raster(), c.into_pixels());
    }
}
#[test]
fn cycles_bad_palettes_and_recording_budgets_fail_explicitly() {
    let font = font();
    let glyph = |cp| PositionedGlyph {
        id: font.glyph_id(cp).unwrap(),
        x: 0.,
        y: 40.,
    };
    assert_eq!(
        ColorGlyphRun::new(
            &font,
            40.,
            &[glyph('\u{e129}')],
            ColorGlyphOptions::default()
        )
        .unwrap_err(),
        ColorGlyphError::UnboundedPaint
    );
    let empty = ColorGlyphRun::new(
        &font,
        40.,
        &[glyph('\u{e12a}')],
        ColorGlyphOptions::default(),
    )
    .unwrap();
    assert_eq!(empty.bounds(), None);
    let mut blank = RasterCanvas::new_linear(64, 64);
    empty.draw(&mut blank).unwrap();
    assert!(blank.buf.iter().all(|v| *v == 0.));
    assert_eq!(
        ColorGlyphRun::new(
            &font,
            40.,
            &[glyph('\u{e10c}')],
            ColorGlyphOptions::default()
        )
        .unwrap_err(),
        ColorGlyphError::MalformedPaint
    );
    assert_eq!(
        ColorGlyphRun::new(
            &font,
            40.,
            &[glyph('\u{e100}')],
            ColorGlyphOptions {
                palette: 3,
                ..Default::default()
            }
        )
        .unwrap_err(),
        ColorGlyphError::InvalidPalette
    );
    assert_eq!(
        ColorGlyphRun::new(
            &font,
            40.,
            &[glyph('\u{e100}')],
            ColorGlyphOptions {
                limits: SceneLimits {
                    max_path_commands: 2,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .unwrap_err(),
        ColorGlyphError::Scene(SceneError::PathBudget)
    );
}

#[test]
fn colr_composites_match_independent_premultiplied_arithmetic() {
    let a = 128. / 255. * 0.8;
    let b = 0.7;
    let cases = [
        (0, [0.; 4]),
        (1, [0., 0., b, b]),
        (2, [a, 0., 0., a]),
        (3, [a * (1. - b), 0., b, a + b - a * b]),
        (5, [0., 0., a * b, a * b]),
        (6, [a * b, 0., 0., a * b]),
        (11, [a * (1. - b), 0., b * (1. - a), a + b - 2. * a * b]),
        (12, [a, 0., b, 1.]),
        (23, [a * (1. - b), 0., b * (1. - a), a + b - a * b]),
    ];
    for (mode, expected) in cases {
        let r = run(13 + mode, 0);
        let mut c = RasterCanvas::new_linear(64, 64);
        r.draw(&mut c).unwrap();
        for (actual, expected) in c.buf[(36 * 64 + 24) * 4..][..4].iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.005,
                "mode {mode}: {actual}/{expected}"
            );
        }
    }
}

#[test]
fn real_nabla_colrv1_varied_axes_and_palettes_keep_vector_ink() {
    let font = Font2D::new(&include_bytes!("fonts/Nabla.ttf")[..], 0).unwrap();
    let space = ColorGlyphRun::new(
        &font,
        40.,
        &[PositionedGlyph {
            id: font.glyph_id(' ').unwrap(),
            x: 0.,
            y: 48.,
        }],
        ColorGlyphOptions::default(),
    )
    .unwrap();
    let mut blank = RasterCanvas::new_linear(64, 64);
    space.draw(&mut blank).unwrap();
    assert!(blank.buf.iter().all(|v| *v == 0.));
    let glyphs = "AB12"
        .chars()
        .enumerate()
        .map(|(i, c)| PositionedGlyph {
            id: font.glyph_id(c).unwrap(),
            x: 8. + i as f32 * 40.,
            y: 68.,
        })
        .collect::<Vec<_>>();
    let mut images = Vec::new();
    for (depth, highlight, palette) in [(0., 0., 0), (200., 24., 0), (100., 12., 2)] {
        let font = font
            .with_variations(&[(*b"EDPT", depth), (*b"EHLT", highlight)])
            .unwrap();
        let run = ColorGlyphRun::new(
            &font,
            40.,
            &glyphs,
            ColorGlyphOptions {
                palette,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(run.unique_glyph_count(), 4);
        let mut c = RasterCanvas::new_linear(192, 96);
        run.draw(&mut c).unwrap();
        let image = c.get_image_data(0, 0, 192, 96).unwrap();
        assert!(image.chunks_exact(4).filter(|p| p[3] > 0).count() > 500);
        images.push(image);
    }
    assert_ne!(images[0], images[1]);
    assert_ne!(images[1], images[2]);
}
