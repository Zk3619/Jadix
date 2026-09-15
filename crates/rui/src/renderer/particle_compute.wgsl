// Particle field: source composite + hashed sparkle + discrete sparks.
// Compute shaders cannot textureSample — use textureSampleLevel.

struct ParticleParams {
    time: f32,
    strength: f32,
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u: ParticleParams;
@group(0) @binding(1)
var src: texture_2d<f32>;
@group(0) @binding(2)
var src_samp: sampler;
@group(0) @binding(3)
var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4)
var<storage, read> bins: array<u32, 256>;

fn luma_bias() -> f32 {
    let mid = bins[64] + bins[128] + bins[192];
    return clamp(f32(mid) / 4000.0, 0.35, 1.75);
}

@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let size = textureDimensions(dst);
    if (gid.x >= size.x || gid.y >= size.y) {
        return;
    }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(size);
    let c = textureSampleLevel(src, src_samp, uv, 0.0);
    let t = u.time;
    let s = u.strength * luma_bias();
    let n = fract(sin(dot(uv * 120.0, vec2<f32>(12.9898, 78.233)) + t) * 43758.5453);
    var spark = 0.0;
    for (var i = 0u; i < 48u; i++) {
        let seed = f32(i) * 19.17 + t * 0.15;
        let px = fract(sin(seed) * 43758.5453);
        let py = fract(cos(seed * 1.31) * 24634.634);
        let d = uv - vec2<f32>(px, py);
        spark += exp(-dot(d, d) * 9000.0);
    }
    let add = n * s * 0.35 + spark * s;
    textureStore(
        dst,
        vec2<i32>(gid.xy),
        vec4<f32>(c.rgb + vec3<f32>(add, add * 0.65, add * 1.2), c.a),
    );
}
