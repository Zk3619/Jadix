use jadix_2d::*;

fn font() -> Font2D {
    Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        0,
    )
    .unwrap()
}
fn pixel(c: &dyn Canvas2D, x: u32, y: u32) -> [u8; 4] {
    c.get_image_data(x, y, 1, 1).unwrap().try_into().unwrap()
}

#[test]
fn positioned_glyphs_reuse_outlines_and_preserve_baseline_brush_and_state() {
    let f = font();
    let a = f.glyph_id('O').unwrap();
    let space = f.glyph_id(' ').unwrap();
    let run = GlyphRun::new(
        &f,
        24.,
        &[
            PositionedGlyph {
                id: a,
                x: 2.,
                y: 28.,
            },
            PositionedGlyph {
                id: a,
                x: 28.,
                y: 28.,
            },
            PositionedGlyph {
                id: space,
                x: 54.,
                y: 28.,
            },
        ],
    )
    .unwrap();
    assert_eq!((run.glyph_count(), run.unique_glyph_count()), (3, 2));
    assert!(run.paths().last().unwrap().is_empty());
    let metrics = f.metrics(24.).unwrap();
    assert!(metrics.ascent > 0. && metrics.descent < 0.);
    assert!(f.advance(a, 24.).unwrap() > 10.);
    let paint = |c: &mut dyn Canvas2D| {
        c.set_fill_brush(Brush2D::gradient(
            gradient::parse_gradient("linear-gradient(to right,red,blue)").unwrap(),
            [0., 0., 64., 32.],
        ));
        c.fill_glyph_run(&run);
    };
    let mut c = RasterCanvas::new_linear(64, 40);
    paint(&mut c);
    let mut g = GpuCanvas::new_linear(64, 40, None);
    paint(&mut g);
    assert_eq!(
        c.get_image_data(0, 0, 64, 40),
        g.get_image_data(0, 0, 64, 40)
    );
    assert!(!g.needs_cpu);
    let rgba = c.get_image_data(0, 0, 64, 40).unwrap();
    assert!(rgba.chunks_exact(4).filter(|p| p[3] > 0).count() > 100);
    assert!(
        rgba.chunks_exact(64 * 4)
            .skip(29)
            .all(|row| row.chunks_exact(4).all(|p| p[3] == 0)),
        "baseline must flip font Y-up to canvas Y-down"
    );
    // Glyphs remain separate operations, allowing ordinary painter-order blending.
    assert_eq!(g.into_ops().len(), 2);
    let duplicate = GlyphRun::new(
        &f,
        24.,
        &[PositionedGlyph {
            id: a,
            x: 2.,
            y: 28.,
        }; 2],
    )
    .unwrap();
    c.clear("transparent");
    c.set_fill("#ff000080");
    c.fill_glyph_run(&duplicate);
    assert!(c
        .get_image_data(0, 0, 64, 40)
        .unwrap()
        .chunks_exact(4)
        .any(|p| p[3].abs_diff(192) <= 1));
}

#[test]
fn font_variations_change_real_outlines_and_reject_invalid_runs_atomically() {
    let font = Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/InterVariable.ttf")[..],
        0,
    )
    .unwrap();
    let id = font.glyph_id('A').unwrap();
    let glyphs = [PositionedGlyph { id, x: 0., y: 30. }];
    let light = font.with_variations(&[(*b"wght", 100.)]).unwrap();
    let bold = font.with_variations(&[(*b"wght", 900.)]).unwrap();
    assert_ne!(
        GlyphRun::new(&light, 30., &glyphs).unwrap().paths().next(),
        GlyphRun::new(&bold, 30., &glyphs).unwrap().paths().next()
    );
    assert!(matches!(
        font.with_variations(&[(*b"zzzz", 1.)]),
        Err(FontError::InvalidVariation)
    ));
    assert!(matches!(
        font.with_variations(&[(*b"wght", f32::NAN)]),
        Err(FontError::InvalidVariation)
    ));
    assert!(matches!(
        GlyphRun::new(&font, 0., &glyphs),
        Err(FontError::InvalidSize)
    ));
    assert!(matches!(
        GlyphRun::new(
            &font,
            30.,
            &[PositionedGlyph {
                id: u16::MAX,
                x: 0.,
                y: 0.
            }]
        ),
        Err(FontError::InvalidGlyph)
    ));
    assert!(matches!(
        GlyphRun::new(
            &font,
            30.,
            &[PositionedGlyph {
                id,
                x: f32::NAN,
                y: 0.
            }]
        ),
        Err(FontError::InvalidPosition)
    ));
    assert!(Font2D::new(&b"invalid font"[..], 0).is_err());
    assert!(Font2D::new(
        &include_bytes!("../../jadix_cosmic_text/fonts/Inter-Regular.ttf")[..],
        u32::MAX
    )
    .is_err());
}

#[test]
fn glyph_stroke_under_affine_clip_mask_and_group_keeps_vector_operations() {
    let f = font();
    let run = GlyphRun::new(
        &f,
        24.,
        &[PositionedGlyph {
            id: f.glyph_id('B').unwrap(),
            x: 4.,
            y: 30.,
        }],
    )
    .unwrap();
    let paint = |c: &mut dyn Canvas2D| {
        c.translate(8., 0.);
        c.clip_rect(0., 0., 24., 40.);
        c.push_layer(0.6, BlendMode::Normal);
        c.set_stroke_brush(Brush2D::gradient(
            gradient::parse_gradient("linear-gradient(to right,red,blue)").unwrap(),
            [0., 0., 32., 40.],
        ));
        c.set_line_width(1.5);
        c.set_line_dash(&[3., 1.]);
        c.stroke_glyph_run(&run);
        c.pop_layer();
    };
    let mut cpu = RasterCanvas::new_linear(48, 40);
    paint(&mut cpu);
    let mut gpu = GpuCanvas::new_linear(48, 40, None);
    paint(&mut gpu);
    assert!(!gpu.needs_cpu);
    assert_eq!(
        cpu.get_image_data(0, 0, 48, 40),
        gpu.get_image_data(0, 0, 48, 40)
    );
    assert!(cpu
        .get_image_data(0, 0, 48, 40)
        .unwrap()
        .chunks_exact(4)
        .any(|p| p[3] > 0));
}

#[test]
fn luminance_masks_use_premultiplied_source_not_alpha_or_hsl_luma() {
    for (color, luma) in [
        ("#ff000080", 0.2126 * 128. / 255.),
        ("#00ff00", 0.7152),
        ("#0000ff", 0.0722),
        ("black", 0.),
        ("white", 1.),
    ] {
        for invert in [false, true] {
            let mut c = RasterCanvas::new_linear(4, 4);
            c.clear("#8040c080");
            c.push_luminance_layer(0.5, invert);
            c.set_fill(color);
            c.fill_rect(0., 0., 4., 4.);
            c.pop_layer();
            let expected =
                (128_f32 * if invert { 1. - luma * 0.5 } else { luma * 0.5 }).round() as u8;
            assert!(
                pixel(&c, 2, 2)[3].abs_diff(expected) <= 1,
                "{color} {invert}"
            );
            if expected > 0 {
                assert!(pixel(&c, 2, 2)[0].abs_diff(128) <= 1);
            }
        }
    }
}

#[test]
fn empty_luminance_groups_and_fractional_final_bounds_preserve_operator_semantics() {
    let mut c = GpuCanvas::new_linear(4, 4, None);
    c.clear("#ff0000");
    c.push_layer_with_options(LayerOptions {
        compose: CompositeMode::LuminanceIn,
        clip: Some([1.5, 0., 2., 4.]),
        ..Default::default()
    });
    c.pop_layer();
    assert_eq!(pixel(&c, 0, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&c, 1, 2), [255, 0, 0, 128]);
    assert_eq!(pixel(&c, 2, 2), [0; 4]);
    assert!(
        matches!(&c.into_ops()[1],PathGpuOp::Layer(l) if !PathGpuOp::Layer(l.clone()).is_empty())
    );
    let mut c = RasterCanvas::new_linear(4, 4);
    c.clear("#ff0000");
    c.push_luminance_layer(1., true);
    c.pop_layer();
    assert_eq!(pixel(&c, 2, 2), [255, 0, 0, 255]);
}

#[test]
fn rounded_gaussian_blur_has_tails_symmetry_and_zero_sigma_geometry() {
    let mut c = RasterCanvas::new_linear(40, 40);
    c.draw_blurred_rounded_rect([10., 10., 20., 20.], 5., 2., [1., 0., 0., 1.]);
    assert!(pixel(&c, 8, 20)[3] > 0);
    assert_eq!(pixel(&c, 0, 20)[3], 0);
    assert!(pixel(&c, 20, 20)[3] > 250);
    for y in 0..40 {
        for x in 0..40 {
            assert_eq!(pixel(&c, x, y), pixel(&c, 39 - x, 39 - y));
        }
    }
    c.clear("transparent");
    c.draw_blurred_rounded_rect([10., 10., 20., 20.], 5., 0., [0., 1., 0., 1.]);
    assert_eq!(pixel(&c, 10, 10), [0; 4]);
    assert_eq!(pixel(&c, 20, 10), [0, 255, 0, 255]);
    assert_eq!(pixel(&c, 9, 20), [0; 4]);
}

#[test]
fn blur_capture_software_continuation_and_paint_state_are_consistent() {
    let paint = |c: &mut dyn Canvas2D| {
        c.set_fill("lime");
        c.set_global_alpha(0.8);
        c.translate(8., 4.);
        c.rotate(15.);
        c.clip_rect(0., 0., 28., 28.);
        c.push_layer(0.6, BlendMode::Multiply);
        c.draw_blurred_rounded_rect([4., 4., 16., 12.], 4., 2., [1., 0., 0., 0.8]);
        c.pop_layer();
        c.reset_transform();
        c.fill_rect(0., 0., 2., 2.);
    };
    let mut c = RasterCanvas::new_linear(40, 40);
    paint(&mut c);
    let mut g = GpuCanvas::new_linear(40, 40, None);
    paint(&mut g);
    assert!(!g.needs_cpu);
    assert_eq!(
        c.get_image_data(0, 0, 40, 40),
        g.get_image_data(0, 0, 40, 40)
    );
    let expected = g.get_image_data(0, 0, 40, 40).unwrap();
    g.put_image_data(39, 39, 1, 1, &[0; 4]);
    assert!(g.needs_cpu);
    assert_eq!(expected, g.get_image_data(0, 0, 40, 40).unwrap());
}

#[test]
fn invalid_blurs_paint_nothing_and_clamped_radius_is_finite() {
    let mut c = GpuCanvas::new_linear(16, 16, None);
    for (rect, r, s) in [
        ([0., 0., 0., 8.], 2., 2.),
        ([0., 0., 8., 8.], f32::NAN, 1.),
        ([0., 0., 8., 8.], 1., -1.),
        ([0., 0., 8., 8.], 1., f32::MAX),
    ] {
        c.draw_blurred_rounded_rect(rect, r, s, [1.; 4]);
    }
    assert!(c.into_ops().is_empty());
    let mut c = RasterCanvas::new_linear(16, 16);
    c.draw_blurred_rounded_rect([4., 4., 8., 8.], 100., 2., [1.; 4]);
    assert!(c.buf.iter().all(|v| v.is_finite()));
    assert!(pixel(&c, 8, 8)[3] > 100);
}

#[test]
fn glyph_recording_detects_unsupported_brushes_before_gpu_export() {
    let f = font();
    let run = GlyphRun::new(
        &f,
        24.,
        &[PositionedGlyph {
            id: f.glyph_id('A').unwrap(),
            x: 4.,
            y: 28.,
        }],
    )
    .unwrap();
    for stroke in [false, true] {
        let brush = Brush2D::gradient(
            gradient::parse_gradient("linear-gradient(red,blue,red,blue,red,blue,red,blue,red)")
                .unwrap(),
            [0., 0., 32., 32.],
        );
        let paint = |c: &mut dyn Canvas2D| {
            c.draw_blurred_rounded_rect([1., 1., 28., 28.], 4., 2., [0., 0., 1., 0.5]);
            if stroke {
                c.set_stroke_brush(brush.clone());
                c.stroke_glyph_run(&run);
            } else {
                c.set_fill_brush(brush.clone());
                c.fill_glyph_run(&run);
            }
        };
        let mut cpu = RasterCanvas::new_linear(32, 32);
        paint(&mut cpu);
        let mut gpu = GpuCanvas::new_linear(32, 32, None);
        paint(&mut gpu);
        assert!(gpu.needs_cpu);
        assert_eq!(
            cpu.get_image_data(0, 0, 32, 32),
            gpu.get_image_data(0, 0, 32, 32)
        );
    }
}

#[test]
fn cff_font_extracts_cubic_outlines_and_renders_counters() {
    let data = &include_bytes!("../../../examples/gallery_web/assets/gallery-sans.otf")[..];
    let font = Font2D::new(data, 0).unwrap();
    let run = GlyphRun::new(
        &font,
        32.,
        &[PositionedGlyph {
            id: font.glyph_id('O').unwrap(),
            x: 4.,
            y: 36.,
        }],
    )
    .unwrap();
    assert!(
        run.paths()
            .flatten()
            .any(|c| matches!(c, PathCommand::CubicTo { .. })),
        "CFF font should retain cubic contours"
    );
    let mut c = RasterCanvas::new_linear(48, 48);
    c.set_fill("#ffffff");
    c.fill_glyph_run(&run);
    let ink = c
        .get_image_data(0, 0, 48, 48)
        .unwrap()
        .chunks_exact(4)
        .filter(|p| p[3] > 0)
        .count();
    assert!(ink > 100 && ink < 600, "O counter/outline ink: {ink}");
}
