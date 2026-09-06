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
    @location(10) flags: f32,               // >0.5 = SDF / MSDF sample
    @location(11) clip_pos: vec2<f32>,
    @location(12) clip_size: vec2<f32>,     // x<0 disables overflow clip
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

/// MSDF median (works for single-channel SDF packed as R=G=B too).
fn median3(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

@vertex
fn vs_main(v: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let ab = inst.transform_ab;
    let cd = inst.transform_cd;
    let txty = inst.transform_txty;

    // Local point relative to rect center; apply **linear** part only (mat2 path).
    // CSS origin is handled by the SDF/textured convention: pivot = rect center,
    // so callers must zero matrix tx/ty (origin wrap lives in that pivot, not txty).
    // 3D mat4 path: project billboard corners on CPU (scene3d) then feed this VS.
    let local = (v.position - 0.5) * inst.rect_size;
    let rotated = affine_linear(local, ab, cd);
    let center = inst.rect_pos + inst.rect_size * 0.5;
    let physical = center + rotated + txty;

    let x = (physical.x / globals.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (physical.y / globals.resolution.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
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
    // Overflow clip in world pixels (after rotate). Do not AABB-clip the
    // pre-rotation quad — writing-mode Bake swaps w/h then rotates.
    if (in.clip_size.x >= 0.0 && in.clip_size.y >= 0.0) {
        let c1 = in.clip_pos + in.clip_size;
        if (in.frag_px.x < in.clip_pos.x || in.frag_px.x >= c1.x
            || in.frag_px.y < in.clip_pos.y || in.frag_px.y >= c1.y) {
            discard;
        }
    }
    let sample = textureSample(tex, tex_sampler, in.uv);
    var c: vec4<f32>;
    // `flags`: 0 = RGBA, 1 = MSDF, 2 = sparse capture blit (zoom/filter).
    // Must not treat 2 as MSDF — empty dest texels would skip the discard below.
    if (in.flags > 0.5 && in.flags < 1.5) {
        // Median of RGB: true MSDF when channels diverge; single-channel SDF when R=G=B.
        // Packing: 0.5 = edge, >0.5 inside (see text_texture / SDF_SPREAD_PX).
        let sd = median3(sample.r, sample.g, sample.b);
        // Screen-space distance in SDF-gradient units; ~1.1px smoothstep band.
        // Linear sampling + narrow AA: sharp without the old px_range=2 fog or Nearest stairs.
        let grad = vec2<f32>(dpdx(sd), dpdy(sd));
        let w = max(length(grad), 1e-5);
        let edge = (sd - 0.5) / w;
        let coverage = smoothstep(-0.6, 0.6, edge);
        c = vec4<f32>(in.tint.rgb, coverage * in.tint.a);
    } else {
        c = sample * in.tint;
        // Sparse offscreen blit. Empty padded texels must not write: ALPHA_BLENDING
        // of (0,0,0,ε) darkens the scrim into a sharp rectangular frame around Dialog.
        if (in.flags > 1.5 && c.a < (1.0 / 255.0)) {
            discard;
        }
    }
    var rgb = c.rgb;
    if (globals.premultiply > 0.5) {
        rgb = rgb * c.a;
    }
    return vec4<f32>(rgb, c.a);
}
