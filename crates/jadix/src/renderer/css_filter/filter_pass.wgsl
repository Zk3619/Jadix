// Multi-mode SVG/CSS filter primitive shader.
// mode: 0 copy, 1 colorMatrix, 2 blurH, 3 blurV, 4 offset, 5 flood,
// 6 composite, 7 blend, 8 dilate, 9 erode, 10 convolve, 11 displacement,
// 12 tile, 13 turbulence, 14 componentTransfer, 15 diffuse, 16 specular, 17 image
// Specialize only the primitive being drawn; -1 retains the dynamic pixel oracle.
override FILTER_MODE: f32 = -1.0;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct FilterUniforms {
    mode: f32,
    p0: f32,
    p1: f32,
    p2: f32,
    p3: f32,
    p4: f32,
    p5: f32,
    p6: f32,
    texel: vec2<f32>,
    _pad0: vec2<f32>,
    color: vec4<f32>,
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    m3: vec4<f32>,
    m4: vec4<f32>,
    k0: vec4<f32>,
    k1: vec4<f32>,
    k2: vec4<f32>,
    k3: vec4<f32>,
    k4: vec4<f32>,
    k5: vec4<f32>,
    k6: vec4<f32>,
    t0: vec4<f32>,
    t1: vec4<f32>,
    t2: vec4<f32>,
    t3: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> u: FilterUniforms;

@group(1) @binding(0)
var tex_a: texture_2d<f32>;
@group(1) @binding(1)
var samp_a: sampler;
@group(1) @binding(2)
var tex_b: texture_2d<f32>;
@group(1) @binding(3)
var samp_b: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(v.position, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}

fn sample_a(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(tex_a, samp_a, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
}

fn sample_b(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(tex_b, samp_b, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
}

fn sample_a_wrap(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(tex_a, samp_a, fract(uv));
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn apply_color_matrix(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(
        dot(u.m0, c) + u.m4.x,
        dot(u.m1, c) + u.m4.y,
        dot(u.m2, c) + u.m4.z,
        dot(u.m3, c) + u.m4.w,
    );
}

fn saturate_matrix(s: f32) -> mat4x4<f32> {
    let inv = 1.0 - s;
    let r = 0.213 * inv;
    let g = 0.715 * inv;
    let b = 0.072 * inv;
    return mat4x4<f32>(
        vec4<f32>(r + s, g, b, 0.0),
        vec4<f32>(r, g + s, b, 0.0),
        vec4<f32>(r, g, b + s, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
}

fn hue_rotate_matrix(deg: f32) -> mat4x4<f32> {
    let a = radians(deg);
    let cos_a = cos(a);
    let sin_a = sin(a);
    return mat4x4<f32>(
        vec4<f32>(0.213 + cos_a * 0.787 - sin_a * 0.213, 0.715 - cos_a * 0.715 - sin_a * 0.715, 0.072 - cos_a * 0.072 + sin_a * 0.928, 0.0),
        vec4<f32>(0.213 - cos_a * 0.213 + sin_a * 0.143, 0.715 + cos_a * 0.285 + sin_a * 0.140, 0.072 - cos_a * 0.072 - sin_a * 0.283, 0.0),
        vec4<f32>(0.213 - cos_a * 0.213 - sin_a * 0.787, 0.715 - cos_a * 0.715 + sin_a * 0.715, 0.072 + cos_a * 0.928 + sin_a * 0.072, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
}

fn gaussian_1d(uv: vec2<f32>, dir: vec2<f32>, sigma: f32) -> vec4<f32> {
    var sum = sample_a(uv) * u.p1;
    if (u.p2 > 0.0) {
        let offset = dir * u.k0.y;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k0.x;
    }
    if (u.p2 > 1.0) {
        let offset = dir * u.k0.w;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k0.z;
    }
    if (u.p2 > 2.0) {
        let offset = dir * u.k1.y;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k1.x;
    }
    if (u.p2 > 3.0) {
        let offset = dir * u.k1.w;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k1.z;
    }
    if (u.p2 > 4.0) {
        let offset = dir * u.k2.y;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k2.x;
    }
    if (u.p2 > 5.0) {
        let offset = dir * u.k2.w;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k2.z;
    }
    if (u.p2 > 6.0) {
        let offset = dir * u.k3.y;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k3.x;
    }
    if (u.p2 > 7.0) {
        let offset = dir * u.k3.w;
        sum += (sample_a(uv + offset) + sample_a(uv - offset)) * u.k3.z;
    }
    return sum;
}

fn composite_op(src: vec4<f32>, dst: vec4<f32>, op: i32) -> vec4<f32> {
    let as_ = src.a;
    let ad = dst.a;
    if (op == 0) { // over
        let a = as_ + ad * (1.0 - as_);
        let rgb = src.rgb * as_ + dst.rgb * ad * (1.0 - as_);
        return vec4<f32>(rgb / max(a, 1e-6), a);
    }
    if (op == 1) { // in
        let a = as_ * ad;
        return vec4<f32>(src.rgb * a / max(as_, 1e-6), a);
    }
    if (op == 2) { // out
        let a = as_ * (1.0 - ad);
        return vec4<f32>(src.rgb * a / max(as_, 1e-6), a);
    }
    if (op == 3) { // atop
        let a = ad;
        let rgb = src.rgb * as_ * ad / max(as_, 1e-6) + dst.rgb * ad * (1.0 - as_);
        return vec4<f32>(rgb / max(a, 1e-6), a);
    }
    if (op == 4) { // xor
        let a = as_ + ad - 2.0 * as_ * ad;
        let rgb = src.rgb * as_ * (1.0 - ad) + dst.rgb * ad * (1.0 - as_);
        return vec4<f32>(rgb / max(a, 1e-6), a);
    }
    if (op == 5) { // lighter
        let a = min(as_ + ad, 1.0);
        let rgb = min(src.rgb * as_ + dst.rgb * ad, vec3<f32>(1.0));
        return vec4<f32>(rgb / max(a, 1e-6), a);
    }
    // arithmetic
    let k1 = u.p1;
    let k2 = u.p2;
    let k3 = u.p3;
    let k4 = u.p4;
    let rgb = k1 * src.rgb * dst.rgb + k2 * src.rgb + k3 * dst.rgb + vec3<f32>(k4);
    let a = clamp(k1 * as_ * ad + k2 * as_ + k3 * ad + k4, 0.0, 1.0);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), a);
}

fn blend_op(src: vec4<f32>, dst: vec4<f32>, mode: i32) -> vec4<f32> {
    let sa = src.a;
    let da = dst.a;
    var b = src.rgb;
    if (mode == 1) { // multiply
        b = src.rgb * dst.rgb;
    } else if (mode == 2) { // screen
        b = 1.0 - (1.0 - src.rgb) * (1.0 - dst.rgb);
    } else if (mode == 3) { // darken
        b = min(src.rgb, dst.rgb);
    } else if (mode == 4) { // lighten
        b = max(src.rgb, dst.rgb);
    } else if (mode == 5) { // overlay approx
        b = select(2.0 * src.rgb * dst.rgb, 1.0 - 2.0 * (1.0 - src.rgb) * (1.0 - dst.rgb), dst.rgb > vec3<f32>(0.5));
    } else if (mode == 6) { // difference
        b = abs(src.rgb - dst.rgb);
    } else if (mode == 7) { // exclusion
        b = src.rgb + dst.rgb - 2.0 * src.rgb * dst.rgb;
    } else {
        // normal / others → src
        b = src.rgb;
    }
    let a = sa + da * (1.0 - sa);
    let rgb = (1.0 - da) * src.rgb * sa + (1.0 - sa) * dst.rgb * da + sa * da * b;
    return vec4<f32>(rgb / max(a, 1e-6), a);
}

fn kernel_at(i: i32) -> f32 {
    switch i {
        case 0: { return u.k0.x; }
        case 1: { return u.k0.y; }
        case 2: { return u.k0.z; }
        case 3: { return u.k0.w; }
        case 4: { return u.k1.x; }
        case 5: { return u.k1.y; }
        case 6: { return u.k1.z; }
        case 7: { return u.k1.w; }
        case 8: { return u.k2.x; }
        case 9: { return u.k2.y; }
        case 10: { return u.k2.z; }
        case 11: { return u.k2.w; }
        case 12: { return u.k3.x; }
        case 13: { return u.k3.y; }
        case 14: { return u.k3.z; }
        case 15: { return u.k3.w; }
        case 16: { return u.k4.x; }
        case 17: { return u.k4.y; }
        case 18: { return u.k4.z; }
        case 19: { return u.k4.w; }
        case 20: { return u.k5.x; }
        case 21: { return u.k5.y; }
        case 22: { return u.k5.z; }
        case 23: { return u.k5.w; }
        case 24: { return u.k6.x; }
        default: { return 0.0; }
    }
}

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u2 = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u2.x), mix(c, d, u2.x), u2.y);
}

fn channel_sel(c: vec4<f32>, sel: i32) -> f32 {
    if (sel == 0) { return c.r; }
    if (sel == 1) { return c.g; }
    if (sel == 2) { return c.b; }
    return c.a;
}

fn transfer_one(v: f32, ty: f32, p: vec4<f32>) -> f32 {
    // ty: 0 identity, 1 table, 2 discrete, 3 linear, 4 gamma
    // p: slope, intercept, amplitude, exponent  (+ offset in color channel via extras)
    if (ty < 0.5) {
        return v;
    }
    if (ty < 1.5) {
        // table: interpolate between p.x.. using discrete samples encoded poorly — treat as linear remap
        return clamp(mix(p.x, p.y, clamp(v, 0.0, 1.0)), 0.0, 1.0);
    }
    if (ty < 2.5) {
        return select(p.x, p.y, v >= 0.5);
    }
    if (ty < 3.5) {
        return clamp(p.x * v + p.y, 0.0, 1.0);
    }
    // gamma: amplitude * pow(v, exponent) + offset(p.z reused as offset when packed)
    return clamp(p.z * pow(max(v, 0.0), max(p.w, 0.0001)) + p.x, 0.0, 1.0);
}

fn height_normal(uv: vec2<f32>, scale: f32) -> vec3<f32> {
    let dx = u.texel.x;
    let dy = u.texel.y;
    let hL = sample_a(uv - vec2<f32>(dx, 0.0)).a * scale;
    let hR = sample_a(uv + vec2<f32>(dx, 0.0)).a * scale;
    let hU = sample_a(uv - vec2<f32>(0.0, dy)).a * scale;
    let hD = sample_a(uv + vec2<f32>(0.0, dy)).a * scale;
    return normalize(vec3<f32>(hL - hR, hU - hD, 1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mode = select(u.mode, FILTER_MODE, FILTER_MODE >= 0.0);
    let uv = in.uv;

    if (mode < 0.5) {
        return sample_a(uv);
    }

    if (mode < 1.5) {
        // color matrix: p0 = type (0 matrix, 1 saturate, 2 hueRotate, 3 luminanceToAlpha)
        let c = sample_a(uv);
        let ty = i32(u.p0 + 0.5);
        if (ty == 1) {
            let m = saturate_matrix(u.p1);
            return m * c;
        }
        if (ty == 2) {
            let m = hue_rotate_matrix(u.p1);
            return m * c;
        }
        if (ty == 3) {
            let l = luminance(c.rgb);
            return vec4<f32>(0.0, 0.0, 0.0, l);
        }
        return apply_color_matrix(c);
    }

    if (mode < 2.5) { return gaussian_1d(uv, vec2<f32>(u.texel.x,0.0),u.p0); }
    if (mode < 3.5) { return gaussian_1d(uv, vec2<f32>(0.0,u.texel.y),u.p0); }

    if (mode < 4.5) {
        // offset: p0=dx_px, p1=dy_px in UV via texel
        let off = vec2<f32>(u.p0 * u.texel.x, u.p1 * u.texel.y);
        return sample_a(uv - off);
    }

    if (mode < 5.5) {
        return u.color;
    }

    if (mode < 6.5) {
        let src = sample_a(uv);
        let dst = sample_b(uv);
        return composite_op(src, dst, i32(u.p0 + 0.5));
    }

    if (mode < 7.5) {
        let src = sample_a(uv);
        let dst = sample_b(uv);
        return blend_op(src, dst, i32(u.p0 + 0.5));
    }

    if (mode < 8.5 || mode < 9.5) {
        let dilate = mode < 8.5;
        let rx = i32(clamp(u.p0, 0.0, 8.0));
        let ry = i32(clamp(u.p1, 0.0, 8.0));
        var acc = select(vec4<f32>(1.0), vec4<f32>(0.0), dilate);
        for (var y = -ry; y <= ry; y = y + 1) {
            for (var x = -rx; x <= rx; x = x + 1) {
                let s = sample_a(uv + vec2<f32>(f32(x), f32(y)) * u.texel);
                if (dilate) {
                    acc = max(acc, s);
                } else {
                    acc = min(acc, s);
                }
            }
        }
        return acc;
    }

    if (mode < 10.5) {
        let ox = i32(u.p0 + 0.5);
        let oy = i32(u.p1 + 0.5);
        let tx = i32(u.p2 + 0.5);
        let ty = i32(u.p3 + 0.5);
        let divisor = max(u.p4, 1e-6);
        let bias = u.p5;
        let preserve_alpha = u.p6 > 0.5;
        var sum = vec4<f32>(0.0);
        var ki = 0;
        for (var y = 0; y < oy; y = y + 1) {
            for (var x = 0; x < ox; x = x + 1) {
                let sample_uv = uv + vec2<f32>(f32(x - tx), f32(y - ty)) * u.texel;
                sum += sample_a(sample_uv) * kernel_at(ki);
                ki = ki + 1;
            }
        }
        var outc = sum / divisor + vec4<f32>(bias);
        if (preserve_alpha) {
            outc.a = sample_a(uv).a;
        }
        return clamp(outc, vec4<f32>(0.0), vec4<f32>(1.0));
    }

    if (mode < 11.5) {
        let map = sample_b(uv);
        let scale = u.p0;
        let xs = channel_sel(map, i32(u.p1 + 0.5));
        let ys = channel_sel(map, i32(u.p2 + 0.5));
        let disp = vec2<f32>((xs - 0.5) * 2.0 * scale * u.texel.x, (ys - 0.5) * 2.0 * scale * u.texel.y);
        return sample_a(uv + disp);
    }

    if (mode < 12.5) {
        return sample_a_wrap(uv);
    }

    if (mode < 13.5) {
        let freq = vec2<f32>(max(u.p0, 0.001), max(u.p1, 0.001));
        let octaves = i32(clamp(u.p2, 1.0, 8.0));
        let seed = u.p3;
        let fractal = u.p4 > 0.5;
        var amp = 1.0;
        var sum = 0.0;
        var max_amp = 0.0;
        var p = uv * freq * 64.0 + vec2<f32>(seed);
        for (var i = 0; i < octaves; i = i + 1) {
            sum += value_noise(p) * amp;
            max_amp += amp;
            amp *= 0.5;
            p *= 2.0;
        }
        var n = sum / max(max_amp, 1e-6);
        if (!fractal) {
            n = n * 2.0 - 1.0;
            n = abs(n);
        }
        return vec4<f32>(n, n, n, 1.0);
    }

    if (mode < 14.5) {
        let c = sample_a(uv);
        // t0 = types rgba; t1/t2/t3 = params per channel (slope, intercept, amp, exp)
        let r = transfer_one(c.r, u.t0.x, u.t1);
        let g = transfer_one(c.g, u.t0.y, u.t2);
        let b = transfer_one(c.b, u.t0.z, u.t3);
        let a = transfer_one(c.a, u.t0.w, vec4<f32>(u.p0, u.p1, u.p2, u.p3));
        return vec4<f32>(r, g, b, a);
    }

    if (mode < 15.5) {
        // diffuse lighting from alpha height
        let n = height_normal(uv, u.p0);
        let L = normalize(vec3<f32>(u.p1, u.p2, u.p3));
        let ndotl = max(dot(n, L), 0.0);
        let kd = u.p4;
        let lc = u.color.rgb;
        let rgb = lc * kd * ndotl;
        return vec4<f32>(rgb, 1.0);
    }

    if (mode < 16.5) {
        let n = height_normal(uv, u.p0);
        let L = normalize(vec3<f32>(u.p1, u.p2, u.p3));
        let V = vec3<f32>(0.0, 0.0, 1.0);
        let H = normalize(L + V);
        let spec = pow(max(dot(n, H), 0.0), max(u.p5, 1.0)) * u.p4;
        let lc = u.color.rgb;
        let rgb = lc * spec;
        let a = max(max(rgb.r, rgb.g), rgb.b);
        return vec4<f32>(rgb, a);
    }

    // image / fallback: sample B (href texture) or transparent
    if (u.p0 > 0.5) {
        return sample_b(uv);
    }
    return vec4<f32>(0.0);
}
