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
};

fn affine_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ab.x * v.x + cd.x * v.y, ab.y * v.x + cd.y * v.y);
}

@vertex
fn vs_main(v: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    let ab = inst.transform_ab;
    let cd = inst.transform_cd;
    let txty = inst.transform_txty;

    // Local point relative to rect center; apply **linear** part only.
    // CSS origin is handled by the SDF/textured convention: pivot = rect center,
    // so callers must zero matrix tx/ty (origin wrap lives in that pivot, not txty).
    let local = (v.position - 0.5) * inst.rect_size;
    let rotated = affine_linear(local, ab, cd);
    let center = inst.rect_pos + inst.rect_size * 0.5;
    let physical = center + rotated + txty;

    let x = (physical.x / globals.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (physical.y / globals.resolution.y) * 2.0;
    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = inst.uv_offset + v.uv * inst.uv_scale;
    out.tint = inst.tint;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(tex, tex_sampler, in.uv) * in.tint;
    var rgb = c.rgb;
    if (globals.premultiply > 0.5) {
        rgb = rgb * c.a;
    }
    return vec4<f32>(rgb, c.a);
}
