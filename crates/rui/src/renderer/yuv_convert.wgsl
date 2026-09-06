// BT.709 limited-range YUV → display sRGB (WP-M.3).
// Coefficients match `media/yuv.rs` `bt709_yuv_to_rgb`.

struct Params {
    // 0 = I420 (Y + U + V), 1 = NV12 / P010 (Y + interleaved UV in u_tex)
    layout: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var y_tex: texture_2d<f32>;
@group(0) @binding(1) var u_tex: texture_2d<f32>;
@group(0) @binding(2) var v_tex: texture_2d<f32>;
@group(0) @binding(3) var samp: sampler;
@group(0) @binding(4) var<uniform> params: Params;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let u = f32((vi << 1u) & 2u);
    let v = f32(vi & 2u);
    out.clip_pos = vec4<f32>(u * 2.0 - 1.0, 1.0 - v * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(u, v);
    return out;
}

fn bt709_limited(y: f32, u: f32, v: f32) -> vec3<f32> {
    let yy = (y - 16.0 / 255.0) * (255.0 / 219.0);
    let cb = (u - 128.0 / 255.0) * (255.0 / 224.0);
    let cr = (v - 128.0 / 255.0) * (255.0 / 224.0);
    let r = yy + 1.5748 * cr;
    let g = yy - 0.187324 * cb - 0.468124 * cr;
    let b = yy + 1.8556 * cb;
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let y = textureSampleLevel(y_tex, samp, in.uv, 0.0).r;
    var u: f32;
    var v: f32;
    if (params.layout < 0.5) {
        u = textureSampleLevel(u_tex, samp, in.uv, 0.0).r;
        v = textureSampleLevel(v_tex, samp, in.uv, 0.0).r;
    } else {
        let packed = textureSampleLevel(u_tex, samp, in.uv, 0.0);
        u = packed.r;
        v = packed.g;
    }
    return vec4<f32>(bt709_limited(y, u, v), 1.0);
}
