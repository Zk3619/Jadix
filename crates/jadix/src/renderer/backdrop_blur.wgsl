// Separable Gaussian backdrop blur with downsample / upsample.
// Mode: 0 = 4-tap downsample, 1 = 1D Gaussian, 2 = bilinear upsample.
// Group 0 = uniforms (dynamic offset); group 1 = source texture + sampler.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct BlurUniforms {
    texel_size: vec2<f32>,
    direction: vec2<f32>,
    /// Gaussian σ in source texels (mode 1), unused otherwise.
    sigma: f32,
    /// 0 = downsample, 1 = gaussian, 2 = upsample / blit.
    mode: f32,
    /// Mode 2: output alpha multiplier (modal underlay fade).
    blit_alpha: f32,
};

@group(0) @binding(0)
var<uniform> uniforms: BlurUniforms;

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

fn downsample4(uv: vec2<f32>, texel: vec2<f32>) -> vec4<f32> {
    // Average four samples around the destination UV (anti-aliased reduce).
    let o = texel * 0.5;
    var c = textureSample(src_tex, src_sampler, uv + vec2<f32>(-o.x, -o.y));
    c += textureSample(src_tex, src_sampler, uv + vec2<f32>(o.x, -o.y));
    c += textureSample(src_tex, src_sampler, uv + vec2<f32>(-o.x, o.y));
    c += textureSample(src_tex, src_sampler, uv + vec2<f32>(o.x, o.y));
    return c * 0.25;
}

fn gaussian1d(uv: vec2<f32>, texel: vec2<f32>, dir: vec2<f32>, sigma: f32) -> vec4<f32> {
    let s = max(sigma, 0.5);
    // Cover ~3σ; hard-cap taps for GLES / loop bounds.
    let support = i32(clamp(ceil(s * 3.0), 1.0, 24.0));
    let step_dir = dir * texel;

    var acc = textureSample(src_tex, src_sampler, uv);
    var wsum = 1.0;

    // Adjacent taps share a hardware bilinear fetch at their weighted midpoint.
    // This preserves the discrete Gaussian while nearly halving texture reads.
    for (var i = 1; i <= 24; i = i + 2) {
        if (i > support) {
            break;
        }
        let x = f32(i);
        let w0 = exp(-0.5 * (x / s) * (x / s));
        let x1 = x + 1.0;
        let w1 = select(0.0, exp(-0.5 * (x1 / s) * (x1 / s)), i + 1 <= support);
        let weight = w0 + w1;
        let o = step_dir * (x + w1 / weight);
        acc += textureSampleLevel(src_tex, src_sampler, uv + o, 0.0) * weight;
        acc += textureSampleLevel(src_tex, src_sampler, uv - o, 0.0) * weight;
        wsum += weight * 2.0;
    }
    return acc / wsum;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let mode = uniforms.mode;
    if (mode < 0.5) {
        return downsample4(in.uv, uniforms.texel_size);
    }
    if (mode < 1.5) {
        return gaussian1d(in.uv, uniforms.texel_size, uniforms.direction, uniforms.sigma);
    }
    // Upsample / fullscreen blit: hardware bilinear from low-res source.
    // Page dimming belongs on the modal underlay rect (bg alpha × overlay opacity),
    // not on the blurred capture — fading the blit made glyphs vanish instead of blur.
    let c = textureSample(src_tex, src_sampler, in.uv);
    return c;
}
