use super::*;

#[test]
fn parse_simple_linear() {
    let g = parse_gradient("linear-gradient(blue, pink)").unwrap();
    match g {
        Gradient::Linear(l) => {
            assert_eq!(l.stops.len(), 2);
            assert!((l.angle_deg - 90.0).abs() < 0.1);
        }
        _ => panic!("expected linear"),
    }
}

#[test]
fn parse_to_right() {
    let g = parse_gradient("linear-gradient(to right, blue, pink)").unwrap();
    match g {
        Gradient::Linear(l) => assert!((l.angle_deg - 0.0).abs() < 0.1),
        _ => panic!(),
    }
}

#[test]
fn parse_angled_css() {
    // CSS 90deg = left→right = internal 0
    let g = parse_gradient("linear-gradient(90deg, red, blue)").unwrap();
    match g {
        Gradient::Linear(l) => assert!((l.angle_deg - 0.0).abs() < 0.1),
        _ => panic!(),
    }
}

#[test]
fn parse_multicolor_and_hard_line() {
    let g = parse_gradient("linear-gradient(to bottom left, cyan 50%, palegoldenrod 50%)").unwrap();
    match g {
        Gradient::Linear(l) => {
            assert_eq!(l.stops.len(), 2);
            assert!((l.stops[0].position.unwrap() - 0.5).abs() < 1e-4);
        }
        _ => panic!(),
    }
}

#[test]
fn parse_radial_basic() {
    let g = parse_gradient("radial-gradient(red, blue)").unwrap();
    assert!(matches!(g, Gradient::Radial(_)));
}

#[test]
fn parse_conic_basic() {
    let g = parse_gradient("conic-gradient(red, blue)").unwrap();
    assert!(matches!(g, Gradient::Conic(_)));
}

#[test]
fn parse_repeating_linear() {
    let g = parse_gradient("repeating-linear-gradient(-45deg, red, red 5px, blue 5px, blue 10px)")
        .unwrap();
    match g {
        Gradient::Linear(l) => {
            assert!(l.repeating);
            assert!(l.stops.iter().any(|s| s.position_px));
            assert!(
                (l.stops
                    .iter()
                    .find(|s| s.position_px)
                    .unwrap()
                    .position
                    .unwrap()
                    - 5.0)
                    .abs()
                    < 1e-3
            );
        }
        _ => panic!(),
    }
}

#[test]
fn byte_thresholds_match_scalar_conversion_at_boundaries_and_random_values() {
    let thresholds = linear_srgb_byte_thresholds();
    let verify = |linear: f32| {
        let expected = super::super::color_space::encode_linear_srgb_component(linear)
            .clamp(0.0, 1.0)
            .mul_add(255.0, 0.5) as u8;
        let actual = quantized_linear_srgb_component(linear, thresholds).mul_add(255.0, 0.5) as u8;
        assert_eq!(actual, expected, "linear={linear:?}");
    };
    for threshold in thresholds {
        for bits in threshold.to_bits() - 256..=threshold.to_bits() + 256 {
            verify(f32::from_bits(bits));
        }
    }
    let mut seed = 1_u32;
    for _ in 0..1_000_000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        verify(seed as f32 / u32::MAX as f32);
    }
    for value in [0.0, 1.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        verify(value);
    }
}

#[test]
fn prepared_stop_colors_preserve_linear_srgb_interpolation() {
    for (a, b) in [
        ([0.1, 0.4, 0.7, 0.25], [0.85, 0.3, 0.01, 0.9]),
        ([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]),
    ] {
        let stops =
            expand_and_resolve_stops(&[ColorStop::at(a, 0.0), ColorStop::at(b, 1.0)], &[], 100.0);
        // Clamped endpoints have always returned their original colors.
        assert_eq!(sample_resolved(&stops, &[], 0.0, false), a);
        assert_eq!(sample_resolved(&stops, &[], 1.0, false), b);
        for t in [0.01, 0.17, 0.5, 0.91] {
            assert_eq!(
                sample_resolved(&stops, &[], t, false),
                super::super::color_space::lerp_display_srgb_linear(a, b, t)
            );
        }
    }
}

#[test]
fn prepared_raster_matches_scalar_sampler_byte_for_byte() {
    for css in [
        "linear-gradient(37deg, red, blue 28%, green)",
        "linear-gradient(to right, #12345680, #abcdef)",
        "repeating-linear-gradient(to right, red 0px 5px, blue 5px 10px)",
        "linear-gradient(to right, red 10% 25%, 40%, blue 80%)",
        "repeating-linear-gradient(-45deg, red, red 5px, blue 5px, blue 10px)",
        "radial-gradient(ellipse farthest-corner at 20% 35%, red, blue)",
        "radial-gradient(circle closest-side at 70% 25%, red 3px, blue 20px)",
        "conic-gradient(from 37deg at 30% 70%, red, blue, green)",
        "repeating-conic-gradient(red 0deg 15deg, blue 15deg 30deg)",
        "linear-gradient(125deg, #12345680, #aabbcc00, #abcdef)",
    ] {
        let g = parse_gradient(css).unwrap();
        for (w, h) in [(1, 1), (63, 37), (127, 83)] {
            for radii in [[0.0; 4], [4.0, 7.0, 11.0, 2.0]] {
                let reference = rasterize_gradient_with_sampler(&g, w, h, radii, |x, y| {
                    sample_gradient_in_box(&g, x, y, w as f32, h as f32)
                });
                assert_eq!(
                    rasterize_gradient_with_radii(&g, w, h, radii),
                    reference,
                    "{css} {w}x{h}"
                );
            }
        }
    }
}

#[test]
#[ignore = "CPU gradient raster timing; run explicitly"]
fn gradient_raster_cold_cost() {
    for css in [
        "repeating-linear-gradient(-45deg, #2563eb, #2563eb 5px, #ef4444 5px, #ef4444 10px)",
        "linear-gradient(-45deg, #2563eb, #ef4444)",
    ] {
        let g = parse_gradient(css).unwrap();
        let now = std::time::Instant::now();
        let reference = rasterize_gradient_with_sampler(&g, 768, 192, [12.0; 4], |x, y| {
            sample_gradient_in_box(&g, x, y, 768.0, 192.0)
        });
        let old = now.elapsed();
        let now = std::time::Instant::now();
        let optimized = rasterize_gradient_with_radii(&g, 768, 192, [12.0; 4]);
        let new = now.elapsed();
        assert_eq!(reference, optimized);
        eprintln!("gradient-raster 768x192 css={css} reference={old:?} prepared={new:?}");
    }
}

#[test]
fn parallel_raster_preserves_samples_and_uneven_row_boundaries() {
    for css in [
        "repeating-linear-gradient(-45deg, #2563eb, #2563eb 5px, #ef4444 5px, #ef4444 10px)",
        "linear-gradient(125deg, #12345680, #aabbcc00, #abcdef)",
        "conic-gradient(from 37deg at 30% 70%, red, blue, green)",
        "linear-gradient(to right, #12345680, #abcdef)",
    ] {
        let g = parse_gradient(css).unwrap();
        for (w, h) in [(63, 3), (257, 513)] {
            let radii = [4.0, 7.0, 11.0, 2.0];
            let serial = rasterize_gradient_with_workers(
                &g,
                w,
                h,
                radii,
                prepared_gradient_sampler(&g, w as f32, h as f32),
                1,
            );
            let parallel = rasterize_gradient_with_workers(
                &g,
                w,
                h,
                radii,
                prepared_gradient_sampler(&g, w as f32, h as f32),
                4,
            );
            assert_eq!(serial, parallel, "{css} {w}x{h}");
        }
    }
}

#[test]
fn repeating_raster_is_antialiased_not_empty() {
    let g = parse_gradient("repeating-linear-gradient(-45deg, red, red 5px, blue 5px, blue 10px)")
        .unwrap();
    let rgba = rasterize_gradient(&g, 48, 48);
    assert_eq!(rgba.len(), 48 * 48 * 4);
    // SSAA should produce some intermediate (non-primary) samples along edges.
    let mixed = rgba.chunks(4).any(|p| {
        let r = p[0] as i32;
        let b = p[2] as i32;
        r > 20 && b > 20 && r < 240 && b < 240
    });
    assert!(
        mixed,
        "expected anti-aliased blend pixels between red/blue stripes"
    );
}

#[test]
fn sample_two_stop_horizontal() {
    let g = Gradient::Linear(LinearGradient::two_stop(
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        0.0,
    ));
    let left = sample_gradient(&g, 0.0, 0.5);
    let right = sample_gradient(&g, 1.0, 0.5);
    assert!(left[0] > 0.9);
    assert!(right[2] > 0.9);
}

#[test]
fn via_inserts_mid_stop() {
    let mut l = LinearGradient::two_stop([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], 0.0);
    l.set_via([0.0, 1.0, 0.0, 1.0]);
    assert_eq!(l.stops.len(), 3);
}
