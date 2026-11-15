use jadix_2d::*;
fn font() -> Font2D {
    Font2D::new(
        include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf").as_slice(),
        0,
    )
    .unwrap()
}
fn ink(run: &GlyphRun) -> Vec<u8> {
    let mut c = RasterCanvas::new_linear(120, 64);
    c.set_fill("white");
    c.fill_glyph_run(run);
    c.get_image_data(0, 0, 120, 64).unwrap()
}
#[test]
fn hinted_emboldened_and_per_glyph_transforms_are_prepared() {
    let f = font();
    let id = f.glyph_id('H').unwrap();
    let glyphs = [
        PositionedGlyph { id, x: 8., y: 36. },
        PositionedGlyph { id, x: 48., y: 36. },
    ];
    let plain = GlyphRun::with_options(&f, 17., &glyphs, &Default::default()).unwrap();
    let hinted = GlyphRun::with_options(
        &f,
        17.,
        &glyphs,
        &GlyphRunOptions {
            hint: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(hinted.paths().flatten().count() > 0);
    let changed = (9..25).any(|size| {
        let a =
            GlyphRun::with_options(&f, size as f32 + 0.3, &glyphs, &Default::default()).unwrap();
        let b = GlyphRun::with_options(
            &f,
            size as f32 + 0.3,
            &glyphs,
            &GlyphRunOptions {
                hint: true,
                ..Default::default()
            },
        )
        .unwrap();
        a.paths().collect::<Vec<_>>() != b.paths().collect::<Vec<_>>()
    });
    assert!(
        changed,
        "hinting should adjust some outline at fractional ppem"
    );
    let bold = GlyphRun::with_options(
        &f,
        17.,
        &glyphs,
        &GlyphRunOptions {
            embolden: FontEmbolden {
                amount: [0.8, 0.5],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let sum = |run: &GlyphRun| {
        ink(run)
            .chunks_exact(4)
            .map(|p| u64::from(p[3]))
            .sum::<u64>()
    };
    assert!(sum(&bold) > sum(&plain));
    assert_eq!(bold.glyph_count(), 2);
    assert_eq!(bold.unique_glyph_count(), 1);
    let transformed = GlyphRun::with_options(
        &f,
        17.,
        &glyphs,
        &GlyphRunOptions {
            per_glyph_transforms: vec![[1., 0., 0., 1., 0., 0.], [1., 0., 0., 1., 8., 0.]],
            ..Default::default()
        },
    )
    .unwrap();
    let a = plain.paths().collect::<Vec<_>>();
    let b = transformed.paths().collect::<Vec<_>>();
    assert_eq!(a[0], b[0]);
    assert_ne!(a[1], b[1]);
    let mut gpu = GpuCanvas::new_linear(120, 64, None);
    gpu.set_fill("white");
    gpu.fill_glyph_run(&bold);
    assert!(!gpu.needs_cpu);
    assert_eq!(gpu.get_image_data(0, 0, 120, 64).unwrap(), ink(&bold));
    assert!(GlyphRun::with_options(
        &f,
        17.,
        &glyphs,
        &GlyphRunOptions {
            normalized_coords: Some(vec![0]),
            ..Default::default()
        }
    )
    .is_err());
    assert!(GlyphRun::with_options(
        &f,
        17.,
        &glyphs,
        &GlyphRunOptions {
            per_glyph_transforms: vec![[1.; 6]],
            ..Default::default()
        }
    )
    .is_err());
}
#[test]
fn normalized_axis_outlines_match_user_settings() {
    let font = Font2D::new(include_bytes!("fonts/axes-mini.ttf").as_slice(), 0).unwrap();
    let varied = font
        .with_variations(&[(*b"wdth", 125.), (*b"wght", 650.)])
        .unwrap();
    let coords = varied.normalized_coords().unwrap();
    assert_eq!(coords.len(), 2);
    assert!(coords.iter().any(|v| *v != 0));
    let glyphs = [PositionedGlyph {
        id: font.glyph_id('A').unwrap(),
        x: 8.,
        y: 40.,
    }];
    let user = GlyphRun::with_options(&varied, 32., &glyphs, &Default::default()).unwrap();
    let normalized = GlyphRun::with_options(
        &font,
        32.,
        &glyphs,
        &GlyphRunOptions {
            normalized_coords: Some(coords),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        user.paths().collect::<Vec<_>>(),
        normalized.paths().collect::<Vec<_>>()
    );
    assert!(GlyphRun::with_options(
        &font,
        32.,
        &glyphs,
        &GlyphRunOptions {
            normalized_coords: Some(vec![20000, 0]),
            ..Default::default()
        }
    )
    .is_err());
}
