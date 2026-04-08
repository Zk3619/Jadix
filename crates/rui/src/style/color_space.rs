//! OKLCH 令牌转换与线性 sRGB / Oklab 混合（见仓库根目录 `docs/color.md`）。
use palette::{IntoColor, Mix, Oklab, Oklch, Srgb};

/// CSS / shadcn 风格 OKLCH：`L`∈[0,1]，`C` 无界（中性色为 0），`h` 为度。
#[inline]
pub fn oklch_to_display_srgba(l: f32, c: f32, h_deg: f32, a: f32) -> [f32; 4] {
    let ok = Oklch::new(l, c, h_deg);
    let srgb: Srgb<f32> = ok.into_color();
    let (r, g, b) = srgb.into_components();
    [r, g, b, a]
}

/// 单分量 sRGB 编码（0–1）→ 线性光（0–1）。
#[inline]
pub fn linearize_display_srgb_component(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    if u <= 0.04045 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

/// 线性光 → display sRGB 分量（0–1）。
#[inline]
pub fn encode_linear_srgb_component(linear: f32) -> f32 {
    let l = linear.clamp(0.0, 1.0);
    if l <= 0.0031308 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
pub fn linearize_display_srgb(rgb: [f32; 3]) -> [f32; 3] {
    [
        linearize_display_srgb_component(rgb[0]),
        linearize_display_srgb_component(rgb[1]),
        linearize_display_srgb_component(rgb[2]),
    ]
}

#[inline]
pub fn encode_linear_srgb_to_display_srgb(linear: [f32; 3]) -> [f32; 3] {
    [
        encode_linear_srgb_component(linear[0]),
        encode_linear_srgb_component(linear[1]),
        encode_linear_srgb_component(linear[2]),
    ]
}

/// 在线性 sRGB 下插值 **RGB**，alpha 按 `t` 做线性插值（直通道）。
pub fn lerp_display_srgb_linear(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let la = linearize_display_srgb([a[0], a[1], a[2]]);
    let lb = linearize_display_srgb([b[0], b[1], b[2]]);
    let mixed = [
        la[0] * (1.0 - t) + lb[0] * t,
        la[1] * (1.0 - t) + lb[1] * t,
        la[2] * (1.0 - t) + lb[2] * t,
    ];
    let o = encode_linear_srgb_to_display_srgb(mixed);
    let ao = a[3] * (1.0 - t) + b[3] * t;
    [o[0], o[1], o[2], ao]
}

/// 在线性光下对 **预乘 alpha** 的 RGBA 做 lerp，再还原为直通道 display sRGB。
/// 透明与实色之间过渡时，与「透明叠在背景上」一致；用于 ghost 等从全透明到 accent 的 hover。
pub fn lerp_premultiplied_display_srgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let la = linearize_display_srgb([a[0], a[1], a[2]]);
    let lb = linearize_display_srgb([b[0], b[1], b[2]]);
    let pa = [la[0] * a[3], la[1] * a[3], la[2] * a[3], a[3]];
    let pb = [lb[0] * b[3], lb[1] * b[3], lb[2] * b[3], b[3]];
    let pm = [
        pa[0] * (1.0 - t) + pb[0] * t,
        pa[1] * (1.0 - t) + pb[1] * t,
        pa[2] * (1.0 - t) + pb[2] * t,
        pa[3] * (1.0 - t) + pb[3] * t,
    ];
    let alpha = pm[3];
    if alpha <= 1e-7 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let linear_rgb = [pm[0] / alpha, pm[1] / alpha, pm[2] / alpha];
    let o = encode_linear_srgb_to_display_srgb(linear_rgb);
    [o[0], o[1], o[2], alpha.min(1.0)]
}

/// 在 **Oklab** 中混合 RGB（alpha 单独 lerp），接近 CSS `color-mix(in oklab, …)`。
pub fn mix_display_srgb_in_oklab(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let c1: Oklab<f32> = Srgb::new(a[0], a[1], a[2]).into_color();
    let c2: Oklab<f32> = Srgb::new(b[0], b[1], b[2]).into_color();
    let mixed = c1.mix(c2, t);
    let out: Srgb<f32> = mixed.into_color();
    let (r, g, b_chan) = out.into_components();
    let ao = a[3] * (1.0 - t) + b[3] * t;
    [r, g, b_chan, ao]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oklch_neutral_matches_v4_primary_scale() {
        // shadcn v4 :root --primary: oklch(0.205 0 0) → display sRGB 中性灰（L 为感知亮度，非 sRGB 分量）
        let p = oklch_to_display_srgba(0.205, 0.0, 0.0, 1.0);
        assert!((p[0] - p[1]).abs() < 1e-4 && (p[1] - p[2]).abs() < 1e-4);
        assert!(p[0] > 0.08 && p[0] < 0.11, "r={}", p[0]);
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
    fn mix_oklab_symmetric() {
        let a = [1.0, 0.0, 0.0, 1.0];
        let b = [0.0, 0.0, 1.0, 1.0];
        let m = mix_display_srgb_in_oklab(a, b, 0.5);
        assert!(m[3] - 1.0 < 1e-6);
        assert!(m[0] > 0.0 && m[2] > 0.0);
    }

    #[test]
    fn premultiplied_lerp_transparent_zero() {
        let a = [0.0, 0.0, 0.0, 0.0];
        let b = [0.5, 0.5, 0.5, 0.5];
        let o = lerp_premultiplied_display_srgba(a, b, 0.0);
        assert!(o[3] < 1e-4, "alpha={}", o[3]);
    }
}
