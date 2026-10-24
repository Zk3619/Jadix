// Fullscreen blit: retained color buffer → swapchain (non–Y-flipped UVs).

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var t_color: texture_2d<f32>;
@group(0) @binding(1) var s_color: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_color, s_color, in.uv);
}

fn encode_srgb(value: vec3<f32>) -> vec3<f32> {
    return select(value * 12.92, 1.055 * pow(max(value, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, value > vec3<f32>(0.0031308));
}

fn decode_srgb(value: vec3<f32>) -> vec3<f32> {
    return select(value / 12.92, pow((value + 0.055) / 1.055, vec3<f32>(2.4)), value > vec3<f32>(0.04045));
}

@fragment
fn fs_compositor(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_color, s_color, in.uv);
    let straight = color.rgb / max(color.a, 0.00001);
    return vec4<f32>(decode_srgb(encode_srgb(straight) * color.a), color.a);
}
