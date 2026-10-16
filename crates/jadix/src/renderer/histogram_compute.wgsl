// Luminance histogram: 256 atomic bins.

@group(0) @binding(0)
var src: texture_2d<f32>;
@group(0) @binding(1)
var<storage, read_write> bins: array<atomic<u32>, 256>;

@compute @workgroup_size(8, 8)
fn cs_hist(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dim = textureDimensions(src);
    if (gid.x >= dim.x || gid.y >= dim.y) {
        return;
    }
    let c = textureLoad(src, vec2<i32>(gid.xy), 0);
    let y = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bin = min(u32(clamp(y, 0.0, 1.0) * 255.0), 255u);
    atomicAdd(&bins[bin], 1u);
}
