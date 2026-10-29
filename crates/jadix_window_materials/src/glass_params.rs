//! Shared window-glass parameters (DWMBlurGlass-inspired recipes, user-space only).
//!
//! Math is platform-independent so CI can cover tint / Aero inactive remaps without DWM.

/// Default Gaussian standard deviation (DWMBlurGlass `customBlurAmount`).
pub const DEFAULT_BLUR_AMOUNT: f32 = 20.0;
/// Acrylic / Mica luminosity layer opacity.
pub const DEFAULT_LUMINOSITY_OPACITY: f32 = 0.65;
/// Win7 Aero color weight (`aeroColorBalance`).
pub const DEFAULT_AERO_COLOR_BALANCE: f32 = 0.08;
/// Win7 Aero afterglow weight.
pub const DEFAULT_AERO_AFTERGLOW_BALANCE: f32 = 0.43;
/// Win7 Aero blurred-backdrop weight.
pub const DEFAULT_AERO_BLUR_BALANCE: f32 = 0.49;
/// Acrylic noise multiply opacity (WinUI recipe).
#[allow(dead_code)]
pub const ACRYLIC_NOISE_AMOUNT: f32 = 0.02;
/// Aero specular overlay (0 = off). Used when `aero_reflection` is unset on Aero.
pub const DEFAULT_AERO_REFLECTION: f32 = 0.42;
/// Window-vs-monitor parallax for the Aero specular layer.
pub const DEFAULT_AERO_PARALLAX: f32 = 0.1;

/// ARGB bytes (`[a, r, g, b]`).
pub type GlassTint = [u8; 4];

pub fn clamp_blur_amount(v: f32) -> f32 {
    v.clamp(0.0, 250.0)
}

pub fn clamp_unit(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

pub fn parse_glass_tint(s: &str) -> Option<GlassTint> {
    let t = s.trim();
    let hex = t.strip_prefix('#').unwrap_or(t);
    match hex.len() {
        6 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            Some([
                0xff,
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
            ])
        }
        8 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            Some([
                ((n >> 24) & 0xff) as u8,
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
            ])
        }
        _ => None,
    }
}

/// AccentPolicy `nColor` packing: alpha << 24 | B << 16 | G << 8 | R.
pub fn tint_to_accent_abgr(tint: GlassTint) -> u32 {
    let [a, r, g, b] = tint;
    (a as u32) << 24 | (b as u32) << 16 | (g as u32) << 8 | r as u32
}

/// WinUI Acrylic: suppress max tint opacity at extreme luminosity, restore with saturation.
#[cfg_attr(not(feature = "windows-dcomp"), allow(dead_code))]
pub fn tint_opacity_modifier(tint: GlassTint) -> f64 {
    const MID: f64 = 0.50;
    const WHITE_MAX: f64 = 0.45;
    const MID_MAX: f64 = 0.90;
    const BLACK_MAX: f64 = 0.85;

    let r = tint[1] as f64 / 255.0;
    let g = tint[2] as f64 / 255.0;
    let b = tint[3] as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let v = max;
    let s = if max.abs() < f64::EPSILON {
        0.0
    } else {
        (max - min) / max
    };

    let mut opacity_modifier = MID_MAX;
    if (v - MID).abs() > f64::EPSILON {
        let (lowest_max, max_deviation) = if v > MID {
            (WHITE_MAX, 1.0 - MID)
        } else {
            (BLACK_MAX, MID)
        };
        let mut max_suppression = MID_MAX - lowest_max;
        if s > 0.0 {
            max_suppression *= (1.0 - (s * 2.0)).max(0.0);
        }
        let normalized = (v - MID).abs() / max_deviation;
        opacity_modifier = MID_MAX - max_suppression * normalized;
    }
    opacity_modifier
}

#[derive(Clone, Copy)]
struct Hsv {
    h: f64,
    s: f64,
    v: f64,
}

fn rgb01(tint: GlassTint) -> (f64, f64, f64) {
    (
        tint[1] as f64 / 255.0,
        tint[2] as f64 / 255.0,
        tint[3] as f64 / 255.0,
    )
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> Hsv {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < f64::EPSILON {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f64::EPSILON {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    let s = if max < f64::EPSILON { 0.0 } else { d / max };
    Hsv { h, s, v: max }
}

fn hsv_to_rgb(hsv: Hsv) -> (f64, f64, f64) {
    let c = hsv.v * hsv.s;
    let x = c * (1.0 - ((hsv.h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = hsv.v - c;
    let (r, g, b) = if hsv.h < 60.0 {
        (c, x, 0.0)
    } else if hsv.h < 120.0 {
        (x, c, 0.0)
    } else if hsv.h < 180.0 {
        (0.0, c, x)
    } else if hsv.h < 240.0 {
        (0.0, x, c)
    } else if hsv.h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (r + m, g + m, b + m)
}

fn rgba_bytes(r: f64, g: f64, b: f64, a: f64) -> GlassTint {
    [
        (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

/// WinUI Acrylic luminosity input: optional explicit opacity, otherwise HSV-V clamp.
///
/// When `luminosity_opacity` is `None`, V is clamped to 0.125–0.965 and alpha is
/// mapped from the tint's alpha (0.15–1.03, then capped at 1).
#[cfg_attr(not(any(test, feature = "windows-dcomp")), allow(dead_code))]
pub fn luminosity_color(tint: GlassTint, luminosity_opacity: Option<f32>) -> GlassTint {
    let (r, g, b) = rgb01(tint);
    if let Some(op) = luminosity_opacity {
        return rgba_bytes(r, g, b, op as f64);
    }
    const MIN_V: f64 = 0.125;
    const MAX_V: f64 = 0.965;
    const MIN_OP: f64 = 0.15;
    const MAX_OP: f64 = 1.03;
    let hsv = rgb_to_hsv(r, g, b);
    let clamped = Hsv {
        h: hsv.h,
        s: hsv.s,
        v: hsv.v.clamp(MIN_V, MAX_V),
    };
    let (lr, lg, lb) = hsv_to_rgb(clamped);
    let mapped = (tint[0] as f64 / 255.0) * (MAX_OP - MIN_OP) + MIN_OP;
    rgba_bytes(lr, lg, lb, mapped.min(1.0))
}

/// Tint alpha applied first (WinUI `GetEffectiveLuminosityColor`), then [`luminosity_color`].
#[cfg_attr(not(any(test, feature = "windows-dcomp")), allow(dead_code))]
pub fn effective_luminosity_color(
    tint: GlassTint,
    tint_opacity: f32,
    luminosity_opacity: Option<f32>,
) -> GlassTint {
    let mut t = tint;
    t[0] = (tint[0] as f64 * tint_opacity as f64)
        .round()
        .clamp(0.0, 255.0) as u8;
    luminosity_color(t, luminosity_opacity)
}

/// Effective tint after user opacity and (optional) luminosity-driven modifier.
#[cfg_attr(not(feature = "windows-dcomp"), allow(dead_code))]
pub fn effective_tint_color(
    tint: GlassTint,
    tint_opacity: f32,
    luminosity_opacity: Option<f32>,
) -> GlassTint {
    let mut out = tint;
    let a = if luminosity_opacity.is_some() {
        (tint[0] as f64 * tint_opacity as f64).round()
    } else {
        (tint[0] as f64 * tint_opacity as f64 * tint_opacity_modifier(tint)).round()
    };
    out[0] = a.clamp(0.0, 255.0) as u8;
    out
}

/// Inactive Aero: dim the color layer, lift the blur layer (DWMBlurGlass).
#[allow(dead_code)]
pub fn aero_inactive_balances(color_balance: f32, blur_balance: f32) -> (f32, f32) {
    (
        clamp_unit(color_balance * 0.4),
        clamp_unit(0.4 * blur_balance + 0.6),
    )
}

/// Sprite-overlay Aero (WUC cannot host D2D Tint / Composite-PLUS).
#[cfg(any(test, feature = "windows-dcomp"))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AeroOverlay {
    /// RGB of the color wash.
    pub tint_rgb: [u8; 3],
    /// Visual / brush alpha in 0..=1 (keep ≤ ~0.18 so HostBackdrop frost stays visible).
    pub tint_opacity: f32,
    pub glow_rgb: [u8; 3],
    pub glow_opacity: f32,
    pub reflection: f32,
}

#[cfg(any(test, feature = "windows-dcomp"))]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

/// SOURCE-OVER veil over HostBackdrop frost. Keep it thin — a 0.6 sheet hides the blur.
#[cfg(any(test, feature = "windows-dcomp"))]
pub fn aero_overlay(
    tint: GlassTint,
    glow: GlassTint,
    color_balance: f32,
    afterglow_balance: f32,
    reflection: f32,
    active: bool,
    dark: bool,
) -> AeroOverlay {
    let mut rgb = [tint[1], tint[2], tint[3]];
    let mut glow_rgb = [glow[1], glow[2], glow[3]];
    let tint_a = tint[0] as f32 / 255.0;
    let mut tint_opacity = clamp_unit(color_balance * 0.55 + tint_a * 0.12).clamp(0.07, 0.18);
    let mut glow_opacity = clamp_unit(afterglow_balance * 0.10 + 0.02).clamp(0.02, 0.08);
    let mut reflection = clamp_unit(reflection).min(0.38);
    if dark {
        rgb = [
            lerp_u8(rgb[0], 16, 0.78),
            lerp_u8(rgb[1], 22, 0.78),
            lerp_u8(rgb[2], 40, 0.62),
        ];
        glow_rgb = [
            lerp_u8(glow_rgb[0], 28, 0.7),
            lerp_u8(glow_rgb[1], 34, 0.7),
            lerp_u8(glow_rgb[2], 56, 0.55),
        ];
        tint_opacity = clamp_unit(tint_opacity + 0.03);
        reflection *= 0.7;
    }
    if !active {
        rgb = [
            lerp_u8(rgb[0], 148, 0.58),
            lerp_u8(rgb[1], 152, 0.58),
            lerp_u8(rgb[2], 156, 0.58),
        ];
        glow_rgb = [
            lerp_u8(glow_rgb[0], 144, 0.65),
            lerp_u8(glow_rgb[1], 144, 0.65),
            lerp_u8(glow_rgb[2], 144, 0.65),
        ];
        glow_opacity *= 0.4;
        reflection *= 0.35;
    }
    AeroOverlay {
        tint_rgb: rgb,
        tint_opacity,
        glow_rgb,
        glow_opacity,
        reflection,
    }
}

#[cfg(test)]
#[path = "tests/glass_params.rs"]
mod tests;
