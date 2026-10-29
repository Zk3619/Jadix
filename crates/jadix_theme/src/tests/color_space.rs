use super::*;

#[test]
fn oklch_neutral_matches_v4_primary_scale() {
    // shadcn v4 :root --primary: oklch(0.205 0 0) → display sRGB 中性灰（L 为感知亮度，非 sRGB 分量）
    let p = oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0);
    assert!((p[0] - p[1]).abs() < 1e-4 && (p[1] - p[2]).abs() < 1e-4);
    assert!(p[0] > 0.08 && p[0] < 0.11, "r={}", p[0]);
}

fn hex3(c: [f32; 4]) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (c[2].clamp(0.0, 1.0) * 255.0).round() as u8
    )
}

#[test]
fn oklch_secondary_matches_web_f5() {
    // globals.css `--secondary: oklch(0.97 0 0)` → canvas `#F5F5F5`（非 `#FBFBFB`）
    let s = oklch_to_display_srgba(0.97, 0.0, 0.0, 1.0);
    assert_eq!(hex3(s), "F5F5F5");
}

#[test]
fn mistaken_srgb_double_encode_matches_reported_screen_hex() {
    // 若把 display `#33` 当线性值写入 *Srgb 目标，硬件再 encode → ~`#7C`（用户采样 `#7B`）。
    let display_33 = 0x33 as f32 / 255.0;
    let wrongly_stored = encode_linear_srgb_component(display_33);
    let byte = (wrongly_stored * 255.0).round() as u8;
    assert!((123..=124).contains(&byte), "got {byte:#x}");
    // 正确路径：先线性化再交由 *Srgb 目标 encode，应回到 `#33`。
    let linear = linearize_display_srgb_component(display_33);
    let roundtrip = encode_linear_srgb_component(linear);
    assert_eq!((roundtrip * 255.0).round() as u8, 0x33);
}

#[test]
fn display_to_linear_helper_roundtrips_via_encode() {
    let display = [0.2, 0.2, 0.2, 1.0]; // #333
    let linear = display_srgba_to_linear_srgba(display);
    let back = [
        encode_linear_srgb_component(linear[0]),
        encode_linear_srgb_component(linear[1]),
        encode_linear_srgb_component(linear[2]),
        linear[3],
    ];
    assert_eq!(hex3(back), "333333");
}

#[test]
fn color_mix_over_surface_matches_straight_alpha() {
    let fg = [1.0, 0.0, 0.0, 1.0];
    let surface = [0.0, 0.0, 0.0, 1.0];
    let mixed = color_mix_over_surface(fg, surface, 0.5);
    assert!((mixed[0] - 0.5).abs() < 1e-4);
    assert_eq!(mixed[3], 1.0);
}

#[test]
fn lerp_rgba_u8_midpoint() {
    let a = [0, 0, 0, 255];
    let b = [255, 255, 255, 255];
    let m = lerp_rgba_u8(a, b, 0.5);
    assert!(m[0] > 180 && m[0] < 190, "{}", m[0]);
}

#[test]
fn lerp_paint_chrome_interpolates_bg_and_fg() {
    let rest = PaintChrome {
        bg: [0.0, 0.0, 0.0, 1.0],
        fg: [0, 0, 0, 255],
        border: None,
    };
    let hover = PaintChrome {
        bg: [1.0, 1.0, 1.0, 1.0],
        fg: [255, 255, 255, 255],
        border: Some([0.5, 0.5, 0.5, 1.0]),
    };
    let mid = lerp_paint_chrome(rest, hover, 0.5);
    assert!(mid.bg[0] > 0.73 && mid.bg[0] < 0.74);
    assert!(mid.fg[0] > 180);
    assert!(mid.border.is_some());
}

#[test]
fn lerp_linear_midpoint_gray() {
    let a = [0.0, 0.0, 0.0, 1.0];
    let b = [1.0, 1.0, 1.0, 1.0];
    let m = lerp_display_srgb_linear(a, b, 0.5);
    // 线性中点 ≈ 0.5 线性 → sRGB ≈ 0.735
    assert!(m[0] > 0.73 && m[0] < 0.74, "{}", m[0]);
}

#[test]
fn display_srgb_oklab_roundtrip_primary() {
    for (r, g, b) in [(1.0, 0.0, 0.0), (0.0, 0.0, 1.0), (0.0, 1.0, 0.0)] {
        let (l, a, bb) = display_srgb_to_oklab(r, g, b);
        let back = oklab_to_display_srgb(l, a, bb);
        assert!(
            (back[0] - r).abs() < 1e-3,
            "r roundtrip for ({r},{g},{b}): {back:?}"
        );
        assert!(
            (back[1] - g).abs() < 1e-3,
            "g roundtrip for ({r},{g},{b}): {back:?}"
        );
        assert!(
            (back[2] - b).abs() < 1e-3,
            "b roundtrip for ({r},{g},{b}): {back:?}"
        );
    }
}

#[test]
fn mix_oklab_symmetric() {
    let a = [1.0, 0.0, 0.0, 1.0];
    let b = [0.0, 0.0, 1.0, 1.0];
    let m = mix_display_srgb_in_oklab(a, b, 0.5);
    assert!(m[3] - 1.0 < 1e-6);
    assert!(
        m[0] > 0.0 && m[2] > 0.0,
        "mixed red/blue should stay in both channels: {m:?}"
    );
}

#[test]
fn premultiplied_lerp_transparent_zero() {
    let a = [0.0, 0.0, 0.0, 0.0];
    let b = [0.5, 0.5, 0.5, 0.5];
    let o = lerp_premultiplied_display_srgba(a, b, 0.0);
    assert!(o[3] < 1e-4, "alpha={}", o[3]);
}
