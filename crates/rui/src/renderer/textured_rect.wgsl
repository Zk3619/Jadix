struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) rect_pos: vec2<f32>,
    @location(3) rect_size: vec2<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) uv_offset: vec2<f32>,
    @location(6) uv_scale: vec2<f32>,
    @location(7) transform_ab: vec2<f32>,   // [a, b]
    @location(8) transform_cd: vec2<f32>,   // [c, d]
    @location(9) transform_txty: vec2<f32>, // [tx, ty]
    @location(10) flags: f32,
    @location(11) clip_pos: vec2<f32>,
    @location(12) clip_size: vec2<f32>,     // x<0 disables overflow clip
    @location(13) warp_c01: vec4<f32>,      // TL.xy, TR.xy
    @location(14) warp_c23: vec4<f32>,      // BR.xy, BL.xy
    @location(15) warp_w: vec4<f32>,        // clip w at TL/TR/BR/BL
};

struct Globals {
    resolution: vec2<f32>,
    premultiply: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var tex: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) @interpolate(flat) flags: f32,
    @location(3) frag_px: vec2<f32>,
    @location(4) clip_pos: vec2<f32>,
    @location(5) clip_size: vec2<f32>,
};

fn affine_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ab.x * v.x + cd.x * v.y, ab.y * v.x + cd.y * v.y);
}

/// MSDF median (single-channel SDF packed as R=G=B still works).
fn median3(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

fn flags_msdf(flags: f32) -> bool {
    return (flags > 0.5 && flags < 1.5) || (flags > 3.5 && flags < 4.5);
}

fn flags_warp(flags: f32) -> bool {
    return flags > 2.5;
}

@vertex
fn vs_main(v: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    var physical: vec2<f32>;
    var clip_w: f32 = 1.0;

    if (flags_warp(inst.flags)) {
        let c0 = inst.warp_c01.xy;
        let c1 = inst.warp_c01.zw;
        let c2 = inst.warp_c23.xy;
        let c3 = inst.warp_c23.zw;
        let t = v.position;
        let top = mix(c0, c1, t.x);
        let bot = mix(c3, c2, t.x);
        physical = mix(top, bot, t.y);
        let wtop = mix(inst.warp_w.x, inst.warp_w.y, t.x);
        let wbot = mix(inst.warp_w.w, inst.warp_w.z, t.x);
        clip_w = max(mix(wtop, wbot, t.y), 1e-5);
    } else {
        let ab = inst.transform_ab;
        let cd = inst.transform_cd;
        let txty = inst.transform_txty;
        let local = (v.position - 0.5) * inst.rect_size;
        let rotated = affine_linear(local, ab, cd);
        let center = inst.rect_pos + inst.rect_size * 0.5;
        physical = center + rotated + txty;
    }

    let ndc_x = (physical.x / globals.resolution.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (physical.y / globals.resolution.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x * clip_w, ndc_y * clip_w, 0.0, clip_w);
    out.uv = inst.uv_offset + v.uv * inst.uv_scale;
    out.tint = inst.tint;
    out.flags = inst.flags;
    out.frag_px = physical;
    out.clip_pos = inst.clip_pos;
    out.clip_size = inst.clip_size;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.clip_size.x >= 0.0 && in.clip_size.y >= 0.0) {
        let c1 = in.clip_pos + in.clip_size;
        if (in.frag_px.x < in.clip_pos.x || in.frag_px.x >= c1.x
            || in.frag_px.y < in.clip_pos.y || in.frag_px.y >= c1.y) {
            discard;
        }
    }
    let sample = textureSample(tex, tex_sampler, in.uv);
    var c: vec4<f32>;
    if (flags_msdf(in.flags)) {
        let sd = median3(sample.r, sample.g, sample.b);
        let grad = vec2<f32>(dpdx(sd), dpdy(sd));
        let w = max(length(grad), 1e-5);
        let edge = (sd - 0.5) / w;
        let coverage = smoothstep(-0.6, 0.6, edge);
        c = vec4<f32>(in.tint.rgb, coverage * in.tint.a);
    } else {
        c = sample * in.tint;
        if (in.flags > 1.5 && in.flags < 2.5 && c.a < (1.0 / 255.0)) {
            discard;
        }
    }
    var rgb = c.rgb;
    if (globals.premultiply > 0.5) {
        rgb = rgb * c.a;
    }
    return vec4<f32>(rgb, c.a);
}
