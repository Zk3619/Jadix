// Framework VS + uniforms for the public `shader()` leaf.
// User WGSL supplies `@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32>`.
// Do not redefine `vs_main` / `ShaderUniforms` / `u`.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct ShaderUniforms {
    time: f32,
    _pad0: f32,
    resolution: vec2<f32>,
    pointer: vec2<f32>,
    _pad1: vec2<f32>,
    custom: array<vec4<f32>, 4>,
};

@group(0) @binding(0)
var<uniform> u: ShaderUniforms;

// S6: always present so the pipeline layout is one. Missing `.texture` binds 1×1 white.
@group(1) @binding(0)
var t_src: texture_2d<f32>;
@group(1) @binding(1)
var s_src: sampler;
@group(1) @binding(2)
var t_src1: texture_2d<f32>;
@group(1) @binding(3)
var s_src1: sampler;
@group(1) @binding(4)
var t_src2: texture_2d<f32>;
@group(1) @binding(5)
var s_src2: sampler;

@vertex
fn vs_main(v: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(v.position, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}
