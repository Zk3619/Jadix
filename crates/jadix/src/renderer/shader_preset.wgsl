// Built-in `shader().preset` fragment. Mode in `u.custom[0].x` (EffectKind::as_mode),
// strength in `u.custom[0].y`. Samples `t_src` (1×1 white if unbound).

fn sample(uv: vec2<f32>) -> vec4<f32> {
    return textureSample(t_src, s_src, uv);
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
    let mode = u.custom[0].x;
    let s = u.custom[0].y;
    let texel = vec2<f32>(1.0) / max(u.resolution, vec2<f32>(1.0, 1.0));
    let uv = in.uv;
    let t = u.time;

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
        let d = length(uv - u.pointer) * 12.0;
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
    let n = fract(sin(dot(uv * 120.0, vec2<f32>(12.9898, 78.233)) + t) * 43758.5453);
    let c = sample(uv);
    return c + vec4<f32>(n * s * 0.35, n * s * 0.2, n * s * 0.5, 0.0);
}
