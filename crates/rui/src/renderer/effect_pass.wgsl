// Pluggable post-process effect pass (B16). Mode in uniforms.mode:
// 0 passthrough, 1 displacement, 2 blur, 3 glass, 4 ripple, 5 chromatic, 6 vhs, 7 glitch, 8 particle

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct EffectUniforms {
    mode: f32,
    time: f32,
    pointer: vec2<f32>,
    strength: f32,
    _pad0: f32,
    texel: vec2<f32>,
    _pad1: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: EffectUniforms;

@group(1) @binding(0)
var src_tex: texture_2d<f32>;

@group(1) @binding(1)
var src_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(v.position, 0.0, 1.0);
    out.uv = v.uv;
    return out;
}

fn sample(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(src_tex, src_sampler, uv);
}

fn blur9(uv: vec2<f32>, texel: vec2<f32>, strength: f32) -> vec4<f32> {
    let r = texel * (2.0 + strength * 6.0);
    var c = sample(uv);
    c += sample(uv + vec2<f32>(r.x, 0.0));
    c += sample(uv - vec2<f32>(r.x, 0.0));
    c += sample(uv + vec2<f32>(0.0, r.y));
    c += sample(uv - vec2<f32>(0.0, r.y));
    c += sample(uv + vec2<f32>(r.x, r.y));
    c += sample(uv - vec2<f32>(r.x, r.y));
    c += sample(uv + vec2<f32>(r.x, -r.y));
    c += sample(uv - vec2<f32>(r.x, -r.y));
    return c / 9.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mode = uniforms.mode;
    let uv = in.uv;
    let t = uniforms.time;
    let s = uniforms.strength;
    let texel = uniforms.texel;

    if (mode < 0.5) {
        return sample(uv);
    }
    if (mode < 1.5) {
        let wave = sin(uv.y * 24.0 + t * 4.0) * s * 0.02;
        return sample(uv + vec2<f32>(wave, 0.0));
    }
    if (mode < 2.5) {
        return blur9(uv, texel, s);
    }
    if (mode < 3.5) {
        let c = blur9(uv, texel, s * 0.6);
        let edge = sample(uv + texel);
        return mix(c, edge, 0.15 + s * 0.25);
    }
    if (mode < 4.5) {
        let d = length(uv - uniforms.pointer) * 12.0;
        let ripple = sin(d - t * 6.0) * s * 0.015;
        return sample(uv + vec2<f32>(ripple, ripple));
    }
    if (mode < 5.5) {
        let off = texel * s * 3.0;
        return vec4<f32>(
            sample(uv + vec2<f32>(off.x, 0.0)).r,
            sample(uv).g,
            sample(uv - vec2<f32>(off.x, 0.0)).b,
            sample(uv).a,
        );
    }
    if (mode < 6.5) {
        let scan = step(0.5, fract(uv.y * 180.0 + t * 30.0));
        let c = sample(uv + vec2<f32>(sin(t) * s * 0.01, 0.0));
        return vec4<f32>(c.rgb * (0.85 + 0.15 * scan), c.a);
    }
    if (mode < 7.5) {
        let block = floor(uv.x * 40.0 + t * 20.0);
        let jump = fract(sin(block) * 43758.5453) * s * 0.04;
        return sample(uv + vec2<f32>(jump, 0.0));
    }
    // particle composite: sparkle noise
    let n = fract(sin(dot(uv * 120.0, vec2<f32>(12.9898, 78.233)) + t) * 43758.5453);
    let c = sample(uv);
    return c + vec4<f32>(n * s * 0.35, n * s * 0.2, n * s * 0.5, 0.0);
}
