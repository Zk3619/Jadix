// Separable Gaussian blur (S7). Sample src, write rgba8unorm storage.
// Compute shaders cannot textureSample — use textureSampleLevel.

struct BlurParams {
    texel: vec2<f32>,
    dir: vec2<f32>,
    sigma: f32,
    _pad0: f32,
    _pad1: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> p: BlurParams;
@group(0) @binding(1)
var src: texture_2d<f32>;
@group(0) @binding(2)
var src_samp: sampler;
@group(0) @binding(3)
var dst: texture_storage_2d<rgba8unorm, write>;

fn weight(x: f32, sigma: f32) -> f32 {
    let s = max(sigma, 0.01);
    return exp(-(x * x) / (2.0 * s * s));
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(dst);
    if (gid.x >= size.x || gid.y >= size.y) {
        return;
    }
    let uv0 = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) * p.texel;
    let sigma = p.sigma;
    let radius = i32(min(ceil(sigma * 3.0), 8.0));
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -radius; i <= radius; i = i + 1) {
        let fi = f32(i);
        let w = weight(fi, sigma);
        let uv = uv0 + p.dir * p.texel * fi;
        acc += textureSampleLevel(src, src_samp, uv, 0.0) * w;
        wsum += w;
    }
    textureStore(dst, vec2<i32>(gid.xy), acc / max(wsum, 1e-5));
}
