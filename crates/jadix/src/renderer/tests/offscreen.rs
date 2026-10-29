use super::*;

#[test]
fn linear_golden_tolerance_tracks_coverage_without_hiding_color_or_geometry_changes() {
    let pixel = |rgb, alpha| RgbaImage {
        width: 1,
        height: 1,
        pixels: vec![rgb, rgb, rgb, alpha],
    };
    let edge = |rgb| RgbaImage {
        width: 2,
        height: 1,
        pixels: vec![rgb, rgb, rgb, 255, 255, 255, 255, 255],
    };
    assert!(images_match_linear(&edge(43), &edge(73), 16));
    assert!(!images_match_linear(&pixel(0, 255), &pixel(40, 255), 16));
    assert!(!images_match_linear(&pixel(255, 255), &pixel(225, 255), 16));
    assert!(!images_match_linear(&edge(0), &edge(128), 16));
    assert!(!images_match_linear(&pixel(43, 255), &pixel(43, 200), 16));
    assert!(!images_match_linear(&edge(43), &edge(44), 0));
}

#[test]
fn golden_encode_decode_roundtrip() {
    let img = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ],
    };
    let decoded = decode_rgba_golden(&encode_rgba_golden(&img)).expect("decode");
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    assert!(images_match(&img, &decoded, 0));
}

/// Pure-CPU synthetic 8×8 pattern golden (no GPU) for Stage 8 CI smoke.
#[test]
fn stage8_pattern_golden_cpu() {
    let mut pixels = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8u32 {
        for x in 0..8u32 {
            let on = ((x ^ y) & 1) == 0;
            if on {
                pixels.extend_from_slice(&[0x22, 0xC5, 0x5E, 0xFF]); // green
            } else {
                pixels.extend_from_slice(&[0x0F, 0x17, 0x2A, 0xFF]); // slate
            }
        }
    }
    let img = RgbaImage {
        width: 8,
        height: 8,
        pixels,
    };
    // Roundtrip encode first, then assert against stored golden.
    let decoded = decode_rgba_golden(&encode_rgba_golden(&img)).expect("roundtrip");
    assert!(images_match(&img, &decoded, 0));
    assert_matches_golden("stage8_pattern", &img, 0);
}

#[test]
fn golden_decode_rejects_foreign_bytes() {
    assert!(decode_rgba_golden(b"PNG\x00short").is_none());
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn offscreen_clear_readback_golden() {
    let Some(gpu) = OffscreenGpu::new() else {
        eprintln!("skip: no wgpu adapter");
        return;
    };
    let img = gpu
        .clear_readback(
            8,
            8,
            wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            },
        )
        .expect("readback");
    // Software rasterizers differ slightly in sRGB encode; compare with slack.
    assert_matches_golden("offscreen_clear_blue_8x8", &img, 8);
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn offscreen_clear_readback_smoke() {
    let Some(gpu) = OffscreenGpu::new() else {
        eprintln!("skip: no wgpu adapter");
        return;
    };
    let img = gpu
        .clear_readback(
            8,
            8,
            wgpu::Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        )
        .expect("readback");
    assert_eq!(img.width, 8);
    let p = img.pixel(0, 0).unwrap();
    // sRGB encode may not be exact 255; allow slack
    assert!(p[0] > 200, "expected red-ish, got {p:?}");
    assert!(p[1] < 40 && p[2] < 40, "expected red-ish, got {p:?}");
}
