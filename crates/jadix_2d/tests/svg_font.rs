use jadix_2d::*;
fn prepare(i: u16, options: ColorGlyphOptions) -> Result<ColorGlyphRun, ColorGlyphError> {
    let font = Font2D::new(&include_bytes!("fonts/svg-mini.ttf")[..], 0).unwrap();
    ColorGlyphRun::new(
        &font,
        40.,
        &[PositionedGlyph {
            id: 4 + i,
            x: 8.,
            y: 48.,
        }],
        options,
    )
}
#[cfg(not(feature = "svg-fonts"))]
#[test]
fn svg_fonts_need_explicit_feature() {
    assert_eq!(
        prepare(0, ColorGlyphOptions::default()).unwrap_err(),
        ColorGlyphError::UnsupportedSvg
    );
}
#[cfg(feature = "svg-fonts")]
#[test]
fn svg_svgz_palettes_foreground_baseline_and_bitmap_alpha() {
    let mut results = Vec::new();
    for i in [0, 1, 2, 7, 8, 9, 10] {
        let run = prepare(
            i,
            ColorGlyphOptions {
                foreground: [0., 1., 0., 1.],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(run.bitmap_bytes() > 0);
        let mut c = RasterCanvas::new_linear(64, 64);
        run.draw(&mut c).unwrap();
        let expected = c.get_image_data(0, 0, 64, 64).unwrap();
        let mut g = GpuCanvas::new_linear(64, 64, None);
        run.draw(&mut g).unwrap();
        assert_eq!(g.get_image_data(0, 0, 64, 64).unwrap(), expected);
        assert!(!g.needs_cpu);
        assert_eq!(
            &expected[(30 * 64 + 12) * 4..][..4],
            &[255, 0, 0, 128],
            "glyph={i}"
        );
        if i != 2 {
            assert_eq!(&expected[(30 * 64 + 34) * 4..][..4], &[0, 255, 0, 255]);
        }
        results.push(expected);
    }
    for i in [1, 3, 4, 5, 6] {
        assert_eq!(results[0], results[i], "SVG selection variant {i}");
    }
    let run = prepare(
        0,
        ColorGlyphOptions {
            palette: 1,
            raster_scale: 2.,
            ..Default::default()
        },
    )
    .unwrap();
    let mut c = RasterCanvas::new_linear(64, 64);
    run.draw(&mut c).unwrap();
    assert_eq!(c.get_image_data(12, 30, 1, 1).unwrap(), [0, 255, 0, 255]);
    assert!(
        run.bitmap_bytes()
            > prepare(0, ColorGlyphOptions::default())
                .unwrap()
                .bitmap_bytes()
    );
}
#[cfg(feature = "svg-fonts")]
#[test]
fn unsupported_svg_and_decompression_pixel_budgets_fail_explicitly() {
    for i in [3, 4] {
        assert_eq!(
            prepare(i, ColorGlyphOptions::default()).unwrap_err(),
            ColorGlyphError::UnsupportedSvg
        );
    }
    assert_eq!(
        prepare(5, ColorGlyphOptions::default()).unwrap_err(),
        ColorGlyphError::MalformedSvg
    );
    assert_eq!(
        prepare(
            6,
            ColorGlyphOptions {
                max_svg_bytes: 512,
                ..Default::default()
            }
        )
        .unwrap_err(),
        ColorGlyphError::SvgBudget
    );
    assert_eq!(
        prepare(
            0,
            ColorGlyphOptions {
                limits: SceneLimits {
                    max_copied_image_bytes: 16,
                    ..Default::default()
                },
                ..Default::default()
            }
        )
        .unwrap_err(),
        ColorGlyphError::SvgBudget
    );
}
