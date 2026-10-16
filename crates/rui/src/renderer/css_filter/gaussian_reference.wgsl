// Test reference: original discrete Gaussian, including its 16 texel radius cap.
fn gaussian_1d(uv: vec2<f32>, dir: vec2<f32>, sigma: f32) -> vec4<f32> {
    let s = max(sigma, 0.01);
    let radius = i32(min(ceil(s * 3.0), 16.0));
    var sum = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -radius; i <= radius; i = i + 1) {
        let fi = f32(i);
        let w = exp(-0.5 * (fi * fi) / (s * s));
        sum += sample_a(uv + dir * fi) * w;
        wsum += w;
    }
    return sum / max(wsum, 1e-6);
}
