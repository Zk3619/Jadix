struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var theme_target: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;
struct Progress { value: vec4<f32>, }
@group(0) @binding(3) var<uniform> progress: Progress;
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return mix(textureSample(source, image_sampler, in.uv),
               textureSample(theme_target, image_sampler, in.uv), progress.value.x);
}
