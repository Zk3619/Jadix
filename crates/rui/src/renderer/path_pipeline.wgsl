// PathPipeline: leaf-space triangles → premul MSAA RT.
// Vertex `pos` is in leaf pixels (y down). Coverage scales premul color.
// fill_kind: 0 = vertex color, 1 = linear, 2 = radial (object-bbox UV).

struct PathVert {
    @location(0) pos: vec2<f32>,
    @location(1) cov_pad: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct PathGlobals {
    resolution: vec2<f32>,
    fill_kind: f32,
    angle: f32,
    bbox: vec4<f32>,
    center: vec2<f32>,
    radii: vec2<f32>,
    // Three f32, not vec3: vec3 aligns to 16 and naga sizes this UBO at 240.
    // Rust PathGlobals is packed 224 (`stop_t01` at offset 64).
    stop_n: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    stop_t01: vec4<f32>,
    stop_t45: vec4<f32>,
    c0: vec4<f32>,
    c1: vec4<f32>,
    c2: vec4<f32>,
    c3: vec4<f32>,
    c4: vec4<f32>,
    c5: vec4<f32>,
    c6: vec4<f32>,
    c7: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> g: PathGlobals;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) coverage: f32,
    @location(2) leaf: vec2<f32>,
};

@vertex
fn vs_main(v: PathVert) -> VsOut {
    var out: VsOut;
    let rx = max(g.resolution.x, 1.0);
    let ry = max(g.resolution.y, 1.0);
    let ndc = vec2<f32>(
        v.pos.x / rx * 2.0 - 1.0,
        1.0 - v.pos.y / ry * 2.0,
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.color = v.color;
    out.coverage = v.cov_pad.x;
    out.leaf = v.pos;
    return out;
}

fn stop_t(i: u32) -> f32 {
    if (i < 4u) {
        return g.stop_t01[i];
    }
    return g.stop_t45[i - 4u];
}

fn stop_c(i: u32) -> vec4<f32> {
    switch i {
        case 0u: { return g.c0; }
        case 1u: { return g.c1; }
        case 2u: { return g.c2; }
        case 3u: { return g.c3; }
        case 4u: { return g.c4; }
        case 5u: { return g.c5; }
        case 6u: { return g.c6; }
        default: { return g.c7; }
    }
}

fn sample_stops(t_in: f32) -> vec4<f32> {
    let t = clamp(t_in, 0.0, 1.0);
    let n = u32(max(g.stop_n, 1.0));
    if (n <= 1u) {
        return stop_c(0u);
    }
    var c = stop_c(0u);
    for (var i = 0u; i < 7u; i++) {
        if (i + 1u >= n) {
            break;
        }
        let t0 = stop_t(i);
        let t1 = stop_t(i + 1u);
        if (t <= t1) {
            let u = select(0.0, (t - t0) / max(t1 - t0, 1e-6), t1 > t0);
            return mix(stop_c(i), stop_c(i + 1u), u);
        }
        c = stop_c(i + 1u);
    }
    return c;
}

fn linearize_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)),
        c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var color = in.color;
    if (g.fill_kind > 0.5) {
        let bw = max(g.bbox.z, 1e-6);
        let bh = max(g.bbox.w, 1e-6);
        let uv = vec2<f32>(
            (in.leaf.x - g.bbox.x) / bw,
            (in.leaf.y - g.bbox.y) / bh,
        );
        var t = 0.0;
        if (g.fill_kind < 1.5) {
            let rad = g.angle * 3.14159265 / 180.0;
            let dx = cos(rad);
            let dy = sin(rad);
            let px = uv.x - 0.5;
            let py = uv.y - 0.5;
            let proj = px * dx + py * dy;
            let half = 0.5 * max(abs(dx) + abs(dy), 1e-6);
            t = (proj / half + 1.0) * 0.5;
        } else {
            let nx = (uv.x - g.center.x) / max(g.radii.x, 1e-6);
            let ny = (uv.y - g.center.y) / max(g.radii.y, 1e-6);
            t = sqrt(nx * nx + ny * ny);
        }
        var sampled = sample_stops(t);
        sampled.a = sampled.a * in.color.a;
        color = sampled;
    }
    let alpha = color.a * in.coverage;
    return vec4<f32>(linearize_srgb(color.rgb) * alpha, alpha);
}

@group(1) @binding(0)
var coverage_tex: texture_2d<f32>;

@vertex
fn vs_resolve(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_resolve(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let scale = vec2<i32>(round(vec2<f32>(textureDimensions(coverage_tex)) / g.resolution));
    let base = vec2<i32>(p.xy) * scale;
    var color = vec4<f32>(0.0);
    for (var y = 0; y < scale.y; y++) {
        for (var x = 0; x < scale.x; x++) {
            color += textureLoad(coverage_tex, base + vec2<i32>(x, y), 0);
        }
    }
    color /= f32(scale.x * scale.y);
    // Average linear premultiplied color/coverage, then export the straight-alpha
    // texture expected by the shared image compositor. Never blur opaque interiors.
    if (color.a <= 1e-6) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(color.rgb / color.a, color.a);
}
