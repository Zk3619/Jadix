struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) rect: vec4<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) uv_rect: vec4<f32>,
    @location(5) transform_abcd: vec4<f32>,
    @location(6) transform_txty: vec2<f32>,
    @location(7) flags: f32,
    @location(8) clip_rect: vec4<f32>,
    @location(9) warp_c01: vec4<f32>,
    @location(10) warp_c23: vec4<f32>,
    @location(11) warp_w: vec4<f32>,
    @location(12) round_rect: vec4<f32>,
    @location(13) round_radii: vec4<f32>,
    @location(14) round_rect2: vec4<f32>,
    @location(15) round_radii2: vec4<f32>,
};

struct Globals {
    resolution: vec2<f32>,
    premultiply: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@group(0) @binding(1)
var tex: texture_2d<f32>;

@group(0) @binding(2)
var tex_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) @interpolate(flat) flags: f32,
    @location(3) frag_px: vec2<f32>,
    @location(4) clip_pos: vec2<f32>,
    @location(5) clip_size: vec2<f32>,
    @location(6) world_px: vec2<f32>,
    @location(7) @interpolate(flat) round_rect: vec4<f32>,
    @location(8) @interpolate(flat) round_radii: vec4<f32>,
    @location(9) @interpolate(flat) round_rect2: vec4<f32>,
    @location(10) @interpolate(flat) round_radii2: vec4<f32>,
};

fn affine_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ab.x * v.x + cd.x * v.y, ab.y * v.x + cd.y * v.y);
}

fn affine_inv_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    let det = ab.x * cd.y - ab.y * cd.x;
    let inv_det = 1.0 / det;
    return vec2<f32>(cd.y * v.x - cd.x * v.y, -ab.y * v.x + ab.x * v.y) * inv_det;
}

/// MSDF median (single-channel SDF packed as R=G=B still works).
fn median3(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

fn flags_msdf(flags: f32) -> bool {
    return (flags > 0.5 && flags < 1.5) || (flags > 3.5 && flags < 4.5);
}

fn flags_warp(flags: f32) -> bool {
    return flags > 2.5;
}

@vertex
fn vs_main(v: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;
    out.round_rect = inst.round_rect;
    out.round_radii = inst.round_radii;
    out.round_rect2 = inst.round_rect2;
    out.round_radii2 = inst.round_radii2;
    var physical: vec2<f32>;
    var clip_w: f32 = 1.0;

    if (flags_warp(inst.flags)) {
        let c0 = inst.warp_c01.xy;
        let c1 = inst.warp_c01.zw;
        let c2 = inst.warp_c23.xy;
        let c3 = inst.warp_c23.zw;
        let t = v.position;
        let top = mix(c0, c1, t.x);
        let bot = mix(c3, c2, t.x);
        physical = mix(top, bot, t.y);
        let wtop = mix(inst.warp_w.x, inst.warp_w.y, t.x);
        let wbot = mix(inst.warp_w.w, inst.warp_w.z, t.x);
        clip_w = max(mix(wtop, wbot, t.y), 1e-5);
    } else {
        let ab = inst.transform_abcd.xy;
        let cd = inst.transform_abcd.zw;
        let txty = inst.transform_txty;
        let local = (v.position - 0.5) * inst.rect.zw;
        let rotated = affine_linear(local, ab, cd);
        let center = inst.rect.xy + inst.rect.zw * 0.5;
        physical = center + rotated + txty;
        // Axis-aligned: local clip (same as rounded_rect `in.p`). Rotated: world
        // `physical` vs unshifted AABB so an unrotated ancestor (DragOverlay
        // restrict shell) scissors the transformed quad instead of chopping
        // local bake ink along a diagonal.
        let axis_aligned = abs(ab.y) < 1e-5 && abs(cd.x) < 1e-5;
        if (axis_aligned) {
            out.frag_px = affine_inv_linear(physical - center - txty, ab, cd);
            out.clip_pos = inst.clip_rect.xy - center;
        } else {
            out.frag_px = physical;
            out.clip_pos = inst.clip_rect.xy;
        }
        out.clip_size = inst.clip_rect.zw;
        let ndc_x = (physical.x / globals.resolution.x) * 2.0 - 1.0;
        let ndc_y = 1.0 - (physical.y / globals.resolution.y) * 2.0;
        out.world_px = physical;
        out.clip_position = vec4<f32>(ndc_x * clip_w, ndc_y * clip_w, 0.0, clip_w);
        out.uv = inst.uv_rect.xy + v.uv * inst.uv_rect.zw;
        out.tint = inst.tint;
        out.flags = inst.flags;
        return out;
    }

    let ndc_x = (physical.x / globals.resolution.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (physical.y / globals.resolution.y) * 2.0;
    out.world_px = physical;
    out.clip_position = vec4<f32>(ndc_x * clip_w, ndc_y * clip_w, 0.0, clip_w);
    out.uv = inst.uv_rect.xy + v.uv * inst.uv_rect.zw;
    out.tint = inst.tint;
    out.flags = inst.flags;
    // Perspective warp: clip stays world AABB (`frag_px` = physical).
    out.frag_px = physical;
    out.clip_pos = inst.clip_rect.xy;
    out.clip_size = inst.clip_rect.zw;
    return out;
}

fn rounded_clip_coverage(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let half_size = rect.zw * 0.5;
    let q = p - rect.xy - half_size;
    let radius = min(select(select(radii.x, radii.y, q.x > 0.0),
        select(radii.w, radii.z, q.x > 0.0), q.y > 0.0), min(half_size.x, half_size.y));
    let d = abs(q) - half_size + radius;
    let distance = length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - radius;
    let aa = max(fwidth(distance), 0.5);
    return 1.0 - smoothstep(-0.5 * aa, 0.5 * aa, distance);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.clip_size.x >= 0.0 && in.clip_size.y >= 0.0) {
        let c1 = in.clip_pos + in.clip_size;
        if (in.frag_px.x < in.clip_pos.x || in.frag_px.x >= c1.x
            || in.frag_px.y < in.clip_pos.y || in.frag_px.y >= c1.y) {
            discard;
        }
    }
    var clip_coverage = 1.0;
    if (in.round_rect.z > 0.5 && in.round_rect.w > 0.5) {
        clip_coverage *= rounded_clip_coverage(in.world_px, in.round_rect, in.round_radii);
    }
    if (in.round_rect2.z > 0.5 && in.round_rect2.w > 0.5) {
        clip_coverage *= rounded_clip_coverage(in.world_px, in.round_rect2, in.round_radii2);
    }
    let sample = textureSample(tex, tex_sampler, in.uv);
    var c: vec4<f32>;
    if (flags_msdf(in.flags)) {
        let sd = median3(sample.r, sample.g, sample.b);
        let grad = vec2<f32>(dpdx(sd), dpdy(sd));
        let w = max(length(grad), 1e-5);
        let edge = (sd - 0.5) / w;
        let coverage = smoothstep(-0.6, 0.6, edge);
        c = vec4<f32>(in.tint.rgb, coverage * in.tint.a);
    } else if (in.flags > 1.5 && in.flags < 2.5) {
        // Transparent-RT capture (zoom / filter). Src-over onto clear(0) stores
        // premul rgb = color * coverage. Sparse discard keeps live inner SDF visible.
        // Un-premultiply so ALPHA_BLENDING does not apply coverage twice — that
        // wash made AlertDialog descriptions look like muted-foreground until settle.
        let a_tex = sample.a;
        let a = a_tex * in.tint.a * clip_coverage;
        if (a < (1.0 / 255.0)) {
            discard;
        }
        let rgb_unpremul = sample.rgb / max(a_tex, 1e-5);
        c = vec4<f32>(rgb_unpremul * in.tint.rgb, a);
        if (globals.premultiply > 0.5) {
            return vec4<f32>(sample.rgb * in.tint.rgb * in.tint.a * clip_coverage, a);
        }
        return c;
    } else {
        c = sample * in.tint;
    }
    c.a *= clip_coverage;
    var rgb = c.rgb;
    if (globals.premultiply > 0.5) {
        rgb = rgb * c.a;
    }
    return vec4<f32>(rgb, c.a);
}
