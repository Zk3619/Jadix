use super::*;

#[test]
fn parse_rgb_and_argb() {
    assert_eq!(parse_glass_tint("#112233"), Some([0xff, 0x11, 0x22, 0x33]));
    assert_eq!(parse_glass_tint("64FFFFFF"), Some([0x64, 0xff, 0xff, 0xff]));
    assert_eq!(parse_glass_tint("zz"), None);
}

#[test]
fn accent_abgr_packs_alpha_high() {
    assert_eq!(tint_to_accent_abgr([0x64, 0xff, 0x00, 0x00]), 0x640000FF);
}

#[test]
fn white_tint_is_suppressed() {
    let m = tint_opacity_modifier([255, 255, 255, 255]);
    assert!((m - 0.45).abs() < 0.02, "white modifier={m}");
}

#[test]
fn mid_gray_keeps_high_opacity() {
    let m = tint_opacity_modifier([255, 128, 128, 128]);
    assert!(m > 0.85, "gray modifier={m}");
}

#[test]
fn aero_inactive_lifts_blur() {
    let (c, b) = aero_inactive_balances(0.08, 0.49);
    assert!((c - 0.032).abs() < 1e-5);
    assert!((b - 0.796).abs() < 1e-5);
}

#[test]
fn aero_overlay_inactive_is_grayer_and_dimmer() {
    let tint = [0x40, 0x4f, 0xc3, 0xf7];
    let glow = [0x55, 0xa8, 0xe8, 0xff];
    let on = aero_overlay(tint, glow, 0.16, 0.43, 0.45, true, false);
    let off = aero_overlay(tint, glow, 0.16, 0.43, 0.45, false, false);
    let dark = aero_overlay(tint, glow, 0.16, 0.43, 0.45, true, true);
    assert!(
        dark.tint_rgb[2] < on.tint_rgb[2],
        "dark wash should lose cyan"
    );
    assert!(
        dark.tint_rgb[0] < on.tint_rgb[0],
        "dark wash should be darker red-channel"
    );
    assert!(
        on.tint_opacity < 0.20,
        "active wash must stay a veil, got {}",
        on.tint_opacity
    );
    assert!(
        off.tint_opacity >= on.tint_opacity * 0.85,
        "inactive must not become more transparent ({off} vs {on})",
        off = off.tint_opacity,
        on = on.tint_opacity
    );
    assert!(
        off.tint_rgb[2] < on.tint_rgb[2],
        "inactive should lose blue"
    );
    assert!(off.reflection < on.reflection * 0.4);
}

#[test]
fn effective_tint_with_luminosity_skips_modifier() {
    let t = effective_tint_color([0x80, 255, 255, 255], 1.0, Some(0.65));
    assert_eq!(t[0], 0x80);
}

#[test]
fn luminosity_explicit_keeps_rgb() {
    let c = luminosity_color([0x80, 200, 40, 40], Some(0.65));
    assert_eq!(c[1], 200);
    assert_eq!(c[2], 40);
    assert_eq!(c[3], 40);
    assert_eq!(c[0], (0.65_f64 * 255.0).round() as u8);
}

#[test]
fn luminosity_derived_clamps_white_v() {
    let c = luminosity_color([255, 255, 255, 255], None);
    assert!(c[1] < 255 && c[1] > 240, "clamped V rgb={}", c[1]);
    assert_eq!(c[1], c[2]);
    assert_eq!(c[2], c[3]);
    assert_eq!(c[0], 255);
}

#[test]
fn luminosity_derived_lifts_near_black() {
    let c = luminosity_color([255, 0, 0, 0], None);
    assert!(c[1] > 20, "min V should lift black, got {}", c[1]);
    assert_eq!(c[1], c[2]);
    assert_eq!(c[0], 255);
}

#[test]
fn effective_luminosity_applies_tint_alpha_first() {
    let c = effective_luminosity_color([128, 10, 20, 30], 0.5, Some(0.4));
    assert_eq!(c[1], 10);
    assert_eq!(c[0], (0.4 * 255.0_f64).round() as u8);
}
