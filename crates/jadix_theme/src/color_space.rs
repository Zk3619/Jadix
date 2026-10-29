//! OKLCH 令牌转换与线性 sRGB / Oklab 混合（见仓库根目录 `docs/engine/color.md`）。
//! OKLab/OKLCH 矩阵来自 CSS Color Level 4 / color.js 64-bit 精度（内联，无 `palette` 依赖）。
//!
//! 路径：linear sRGB ↔ XYZ D65 ↔ LMS^(1/3) ↔ OKLab（与浏览器 `oklch()` / `color-mix(in oklab)` 一致）。

/// linear sRGB → XYZ D65（CSS Color 4）。
const LINEAR_SRGB_TO_XYZ: [[f32; 3]; 3] = [
    [0.41239079926595934, 0.357584339383878, 0.1804807884018343],
    [0.21263900587151027, 0.715168678767756, 0.07219231536073371],
    [0.01933081871559182, 0.11919477979462598, 0.9505321522496607],
];

/// XYZ D65 → linear sRGB。
const XYZ_TO_LINEAR_SRGB: [[f32; 3]; 3] = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];

/// XYZ D65 → LMS（OKLab 白点与 CSS 一致，见 color.js `oklab.js`）。
const XYZ_TO_LMS: [[f32; 3]; 3] = [
    [0.8190224379967030, 0.3619062600528904, -0.1288737815209879],
    [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
    [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
];

/// LMS → XYZ D65。
const LMS_TO_XYZ: [[f32; 3]; 3] = [
    [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
    [-0.0405757452148008, 1.1122868032803170, -0.0717110580655164],
    [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
];

/// LMS^(1/3) → OKLab。
const LMS3_TO_OKLAB: [[f32; 3]; 3] = [
    [0.2104542683093140, 0.7936177747023054, -0.0040720430116193],
    [1.9779985324311684, -2.4285922420485799, 0.4505937096174110],
    [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];

/// OKLab → LMS^(1/3)。
const OKLAB_TO_LMS3: [[f32; 3]; 3] = [
    [1.0, 0.3963377773761749, 0.2158037573099136],
    [1.0, -0.1055613458156586, -0.0638541728258133],
    [1.0, -0.0894841775298119, -1.2914855480194092],
];

#[inline]
fn mat3_mul_vec3(m: [[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// CSS / shadcn 风格 OKLCH：`L`∈[0,1]，`C` 无界（中性色为 0），`h` 为度。
#[inline]
pub fn oklch_to_display_srgba(l: f32, c: f32, h_deg: f32, a: f32) -> [f32; 4] {
    let h = h_deg.to_radians();
    let lab_a = c * h.cos();
    let lab_b = c * h.sin();
    let rgb = oklab_to_display_srgb(l, lab_a, lab_b);
    [rgb[0], rgb[1], rgb[2], a]
}

/// OKLab (L,a,b) → display sRGB RGB.
#[inline]
pub fn oklab_to_display_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    encode_linear_srgb_to_display_srgb(oklab_to_linear_srgb(l, a, b))
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

#[inline]
fn display_srgb_to_oklab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let rgb = linearize_display_srgb([r, g, b]);
    linear_srgb_to_oklab(rgb)
}

#[inline]
fn linear_srgb_to_oklab(rgb: [f32; 3]) -> (f32, f32, f32) {
    let xyz = mat3_mul_vec3(LINEAR_SRGB_TO_XYZ, rgb);
    let lms = mat3_mul_vec3(XYZ_TO_LMS, xyz);
    let lms3 = [lms[0].cbrt(), lms[1].cbrt(), lms[2].cbrt()];
    let oklab = mat3_mul_vec3(LMS3_TO_OKLAB, lms3);
    (oklab[0], oklab[1], oklab[2])
}

#[inline]
fn oklab_to_linear_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let lms3 = mat3_mul_vec3(OKLAB_TO_LMS3, [l, a, b]);
    let lms = [lms3[0].powi(3), lms3[1].powi(3), lms3[2].powi(3)];
    let xyz = mat3_mul_vec3(LMS_TO_XYZ, lms);
    mat3_mul_vec3(XYZ_TO_LINEAR_SRGB, xyz)
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
///
/// 当 rest 端点 α≈0 时，中间态会折叠为黑（见 `docs/engine/color.md` §B.2）。
/// Ghost Button 等「透明→有色」路径须保留 rest RGB、勿用本函数。
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
    let (l1, a1, b1) = display_srgb_to_oklab(a[0], a[1], a[2]);
    let (l2, a2, b2) = display_srgb_to_oklab(b[0], b[1], b[2]);
    let mixed_l = l1 * (1.0 - t) + l2 * t;
    let mixed_a = a1 * (1.0 - t) + a2 * t;
    let mixed_b = b1 * (1.0 - t) + b2 * t;
    let rgb = oklab_to_display_srgb(mixed_l, mixed_a, mixed_b);
    let ao = a[3] * (1.0 - t) + b[3] * t;
    [rgb[0], rgb[1], rgb[2], ao]
}

// ---------------------------------------------------------------------------
// ① Mix / lerp primitives (see `docs/engine/color.md` §B)
// ---------------------------------------------------------------------------

/// Straight-channel α blend: `fg` at opacity `alpha` over `surface`, opaque result (`α=1`).
///
/// Matches Tailwind `token/N` and ShadCN soft tints (not linear-light lerp).
pub fn color_mix_over_surface(fg: [f32; 4], surface: [f32; 4], alpha: f32) -> [f32; 4] {
    let a = alpha.clamp(0.0, 1.0);
    [
        fg[0] * a + surface[0] * (1.0 - a),
        fg[1] * a + surface[1] * (1.0 - a),
        fg[2] * a + surface[2] * (1.0 - a),
        1.0,
    ]
}

/// Linear-light lerp for 8-bit RGBA (caption / icon foreground hover).
pub fn lerp_rgba_u8(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let af = [
        a[0] as f32 / 255.0,
        a[1] as f32 / 255.0,
        a[2] as f32 / 255.0,
        a[3] as f32 / 255.0,
    ];
    let bf = [
        b[0] as f32 / 255.0,
        b[1] as f32 / 255.0,
        b[2] as f32 / 255.0,
        b[3] as f32 / 255.0,
    ];
    let o = lerp_display_srgb_linear(af, bf, t);
    [
        (o[0] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[1] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        (o[3] * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// Alias for text/caption hover (same as [`lerp_rgba_u8`]).
#[inline]
pub fn lerp_text_rgba(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    lerp_rgba_u8(a, b, t)
}

/// Rest/hover paint triple for component chrome interpolation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintChrome {
    pub bg: [f32; 4],
    pub fg: [u8; 4],
    pub border: Option<[f32; 4]>,
}

fn lerp_border_opt(a: Option<[f32; 4]>, b: Option<[f32; 4]>, t: f32) -> Option<[f32; 4]> {
    match (a, b) {
        (Some(ab), Some(bb)) => Some(lerp_display_srgb_linear(ab, bb, t)),
        (Some(c), None) | (None, Some(c)) => Some(c),
        (None, None) => None,
    }
}

/// Interpolate bg / fg / border between two chrome endpoints.
pub fn lerp_paint_chrome(a: PaintChrome, b: PaintChrome, t: f32) -> PaintChrome {
    let t = t.clamp(0.0, 1.0);
    PaintChrome {
        bg: lerp_display_srgb_linear(a.bg, b.bg, t),
        fg: lerp_rgba_u8(a.fg, b.fg, t),
        border: lerp_border_opt(a.border, b.border, t),
    }
}

/// Display sRGB RGBA → 线性光 RGBA（alpha 不变）。
///
/// 写入 wgpu `*Srgb` 渲染目标前，片元输出须为线性值；若把 display 编码色直接写出，
/// 硬件会再 encode 一次（`#333`→`#7B`、`#F5`→`#FB`）。矩形管线在 WGSL 末尾做等价转换。
#[inline]
pub fn display_srgba_to_linear_srgba(c: [f32; 4]) -> [f32; 4] {
    let rgb = linearize_display_srgb([c[0], c[1], c[2]]);
    [rgb[0], rgb[1], rgb[2], c[3]]
}

#[cfg(test)]
#[path = "tests/color_space.rs"]
mod tests;
