// Large-image downsample to rgba8unorm storage (backdrop / particle prep).
// Compute shaders cannot textureSample — use textureSampleLevel.

@group(0) @binding(0)
var src: texture_2d<f32>;
@group(0) @binding(1)
var src_samp: sampler;
@group(0) @binding(2)
var dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn cs_down(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(dst);
    if (gid.x >= dim.x || gid.y >= dim.y) {
        return;
    }
    let uv = (vec2<f32>(gid.xy) + vec2<f32>(0.5)) / vec2<f32>(dim);
    let c = textureSampleLevel(src, src_samp, uv, 0.0);
    textureStore(dst, vec2<i32>(gid.xy), c);
}
