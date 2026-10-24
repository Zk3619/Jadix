// SDF Based Rounded Rectangle Shader

struct VertexInput {
    @location(0) position: vec2<f32>,
};

struct InstanceInput {
    @location(1) glass: vec4<f32>,
    @location(2) rect_pos: vec2<f32>,
    @location(3) rect_size: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) radii: vec4<f32>, // CSS border-radius: TL, TR, BR, BL
    @location(7) border_width: f32,
    @location(8) shadow_color: vec4<f32>,
    @location(9) shadow_offset: vec2<f32>,
    @location(10) shadow_params: vec4<f32>,
    @location(11) transform_ab: vec2<f32>,   // [a, b] of 2D affine
    @location(12) transform_cd: vec2<f32>,   // [c, d] of 2D affine
    @location(13) transform_txty: vec2<f32>, // [tx, ty] of 2D affine
    @location(14) clip_pos: vec2<f32>,
    @location(15) clip_size_and_extra: vec4<f32>, // xy=clip_size, z=packed border_extra, w=backdrop_blur
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // Instance constants have no spatial interpolation. Keeping them flat also
    // lets the driver make the simple-fill branch uniform across the primitive.
    @location(0) @interpolate(flat) rect_size: vec2<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) @interpolate(flat) border_color: vec4<f32>,
    @location(3) @interpolate(flat) radii: vec4<f32>,
    @location(4) @interpolate(flat) border_width: f32,
    @location(5) @interpolate(flat) shadow_color: vec4<f32>,
    @location(6) @interpolate(flat) shadow_offset: vec2<f32>,
    @location(7) @interpolate(flat) shadow_blur: f32,
    @location(8) @interpolate(flat) shadow_spread: f32,
    @location(9) @interpolate(flat) shadow_inset: f32,
    @location(10) p: vec2<f32>, // local point relative to rect center (unrotated)
    @location(11) @interpolate(flat) clip_rect: vec4<f32>,
    @location(12) @interpolate(flat) glass: vec4<f32>,
    @location(13) frag_px: vec2<f32>,
    /// x = border_extra; y = clip index; z = backdrop blur; w = world-clip / fast-fill flags.
    /// Must stay ≤ location 14 (`max_inter_stage_shader_variables` is 0..=14).
    @location(14) @interpolate(flat) extra_and_id: vec4<f32>,
};

struct Globals {
    resolution: vec2<f32>,
    premultiply: f32,
    clip_poly_enabled: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct GpuClipPoly {
    count: u32,
    fill: u32,
    count2: u32,
    fill2: u32,
    pts: array<vec2<f32>, 16>,
    pts2: array<vec2<f32>, 16>,
    round_rect: vec4<f32>,
    round_radii: vec4<f32>,
    round_rect2: vec4<f32>,
    round_radii2: vec4<f32>,
};

@group(0) @binding(1)
var<storage, read> clip_polys: array<GpuClipPoly>;

@group(1) @binding(0)
var backdrop_tex: texture_2d<f32>;
@group(1) @binding(1)
var backdrop_samp: sampler;
@group(1) @binding(2)
var optical_source_tex: texture_2d<f32>;

// Apply the linear part (2x2) of the affine transform.
fn affine_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(ab.x * v.x + cd.x * v.y,
                     ab.y * v.x + cd.y * v.y);
}

// Inverse of the 2x2 linear part.
fn affine_inv_linear(v: vec2<f32>, ab: vec2<f32>, cd: vec2<f32>) -> vec2<f32> {
    let det = ab.x * cd.y - ab.y * cd.x;
    let inv_det = 1.0 / det;
    return vec2<f32>( cd.y * v.x - cd.x * v.y,
                     -ab.y * v.x + ab.x * v.y) * inv_det;
}

fn rect_vertex(
    model: VertexInput,
    instance: InstanceInput,
    instance_idx: u32,
    mode: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let ab = instance.transform_ab;
    let cd = instance.transform_cd;
    let txty = instance.transform_txty;

    // +2px AA pad: outer SDF smoothstep fringe (esp. rounded corners where fwidth(d) grows)
    // must stay inside the draw quad — otherwise 1px borders look broken at the apex.
    let shadow_blur = instance.shadow_params.x;
    let shadow_spread = instance.shadow_params.y;
    let shadow_inset = instance.shadow_params.z;
    // No size gate: a 2px I-beam is entirely inside the +2 vertex AA fringe, so
    // dest-reset of the fill leaves a translucent hole (white line through S).
    let simple_fill = instance.radii.x <= 0.0 && instance.radii.y <= 0.0
        && instance.radii.z <= 0.0 && instance.radii.w <= 0.0
        && instance.border_width <= 0.0
        && instance.shadow_color.a <= 0.001
        && instance.glass.x >= 0.0
        && instance.clip_size_and_extra.w <= 0.5
        && instance.clip_size_and_extra.z < 0.5;
    let plain_fill = instance.border_width <= 0.0
        && instance.shadow_color.a <= 0.001
        && instance.clip_size_and_extra.w <= 0.5;
    var ext = vec2<f32>(0.0);
    if (!simple_fill) {
        let spread = max(0.0, shadow_spread);
        ext = vec2<f32>(abs(instance.shadow_offset.x), abs(instance.shadow_offset.y))
            + vec2<f32>(shadow_blur * 2.0 + instance.border_width + spread + 2.0);
        if (shadow_inset > 0.5) {
            ext = vec2<f32>(
                shadow_blur * 2.0 + instance.border_width + abs(shadow_spread) + 2.0
            );
        }
    }

    // Grow AABB enough to cover all four transformed corners.
    // Capture (`zoom_layer_shadow_outset`) must use this same `ext` or the RT
    // clips the halo to a sharp rectangle that pops off when Coverage takes over.
    let half = instance.rect_size * 0.5 + ext;
    let c0 = abs(affine_linear(vec2<f32>( half.x,  half.y), ab, cd));
    let c1 = abs(affine_linear(vec2<f32>( half.x, -half.y), ab, cd));
    let c2 = abs(affine_linear(vec2<f32>(-half.x,  half.y), ab, cd));
    let c3 = abs(affine_linear(vec2<f32>(-half.x, -half.y), ab, cd));
    let aabb_half = max(max(c0, c1), max(c2, c3));

    var local_scaled = (model.position - 0.5) * 2.0 * aabb_half;
    if (mode != 0u) {
        let inset = max(max(instance.radii.x, instance.radii.y), max(instance.radii.z, instance.radii.w))
            + max(instance.border_width, 0.0) + 4.0;
        let inner_half = max(instance.rect_size * 0.5 - vec2<f32>(inset), vec2<f32>(0.0));
        if (mode == 1u) {
            // Eight disjoint perimeter patches. The center is submitted separately
            // with a constant-color shader, in the same instance painter order.
            let side = select(vec2<f32>(-1.0), vec2<f32>(1.0), model.position > vec2<f32>(0.5));
            let outer = (model.position < vec2<f32>(0.1)) | (model.position > vec2<f32>(0.9));
            local_scaled = side * select(inner_half, aabb_half, outer);
        } else {
            local_scaled = (model.position - 0.5) * 2.0 * inner_half;
        }
    }
    // Inverse-transform the fragment position back to local space for SDF.
    let local = affine_inv_linear(local_scaled, ab, cd);

    // Physical position: rect_pos already includes the affine tx/ty offset.
    let center = instance.rect_pos + instance.rect_size * 0.5;
    let physical_pos = center + local_scaled + txty;

    // Convert to NDC
    let x = (physical_pos.x / globals.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (physical_pos.y / globals.resolution.y) * 2.0;

    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.p = local;
    out.rect_size = instance.rect_size;
    // Simple fill color is constant per instance. Decode sRGB here rather than
    // running three pow() calls for every pixel of each full-window background.
    var fill_color = instance.color;
    if (plain_fill || mode == 2u) {
        fill_color = vec4<f32>(linearize_srgb_rgb(instance.color.rgb), instance.color.a);
    }
    out.color = fill_color;
    out.border_color = instance.border_color;
    out.radii = instance.radii;
    out.border_width = instance.border_width;
    out.shadow_color = instance.shadow_color;
    out.shadow_offset = instance.shadow_offset;
    out.shadow_blur = shadow_blur;
    out.shadow_spread = shadow_spread;
    out.shadow_inset = shadow_inset;
    // Overflow clip is the ancestor layout box in `rect_pos` space. FS tests `in.p`
    // (already affine-inverted) against clip shifted to this instance's center.
    out.clip_rect = vec4<f32>(instance.clip_pos - center, instance.clip_size_and_extra.xy);
    out.glass = instance.glass;
    out.frag_px = physical_pos;
    // Bind groups distinguish unclipped draws from polygon-clipped draws.
    // The flag comes from the uniform; storage stays fragment-only on GLES.
    let has_world_clip = globals.clip_poly_enabled > 0.5;
    let fast_fill = simple_fill && instance.rect_size.x > 32.0 && instance.rect_size.y > 32.0;
    out.extra_and_id = vec4<f32>(
        instance.clip_size_and_extra.z,
        f32(instance_idx),
        instance.clip_size_and_extra.w,
        select(0.0, 1.0, has_world_clip) + select(0.0, 2.0, fast_fill)
            + select(0.0, 4.0, plain_fill) + clamp(instance.shadow_params.w, 0.0, 1.0) * 0.5,
    );

    return out;
}

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput, @builtin(instance_index) index: u32) -> VertexOutput {
    return rect_vertex(model, instance, index, 0u);
}

@vertex
fn vs_ring(model: VertexInput, instance: InstanceInput, @builtin(instance_index) index: u32) -> VertexOutput {
    return rect_vertex(model, instance, index, 1u);
}

@vertex
fn vs_interior(model: VertexInput, instance: InstanceInput, @builtin(instance_index) index: u32) -> VertexOutput {
    return rect_vertex(model, instance, index, 2u);
}

// 2D Box Signed Distance Field (round corners)
// `r` = CSS [TL, TR, BR, BL]; `p` is centered (+y down).
fn select_corner_radius(p: vec2<f32>, r: vec4<f32>) -> f32 {
    if (p.x > 0.0 && p.y > 0.0) { return r.z; } // BR
    if (p.x < 0.0 && p.y > 0.0) { return r.w; } // BL
    if (p.x < 0.0 && p.y < 0.0) { return r.x; } // TL
    return r.y; // TR
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let r_sel = select_corner_radius(p, r);

    let q = abs(p) - b + vec2<f32>(r_sel);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r_sel;
}

// fwidth(distance) samples a 2x2 pixel quad. At a curved edge its result
// changes when an integer scroll swaps the even/odd pixel lanes, so copied
// corners differ from a fresh raster. Differentiate the rounded-box SDF
// analytically, then transform that gradient into screen-pixel space.
fn shape_aa(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>, d: f32, shape_type: f32) -> f32 {
    let fallback = fwidth(d);
    let dx = dpdx(p);
    let dy = dpdy(p);
    if (shape_type < 0.5) {
        let q = abs(p) - b + vec2<f32>(select_corner_radius(p, r));
        let outside = max(q, vec2<f32>(0.0));
        let len = length(outside);
        var normal = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), q.x > q.y);
        if (len > 0.00001) {
            normal = outside / len;
        }
        let gradient = normal * sign(p);
        return clamp(abs(dot(gradient, dx)) + abs(dot(gradient, dy)), 0.75, 2.0);
    }
    return clamp(fallback, 0.75, 2.0);
}

fn sd_aabb(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let q = abs(p) - b;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0)));
}

// Angle (chamfer/bevel): 45° cut of size `r` from each edge.
fn sd_angle_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let r_sel = max(select_corner_radius(p, r), 0.0);
    let q = abs(p) - b;
    let d_box = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0)));
    if (r_sel < 0.001) {
        return d_box;
    }
    let d_cut = (q.x + q.y + r_sel) * 0.70710678;
    return max(d_box, d_cut);
}

// Superellipse corner SDF: same IQ round-box split, Lp instead of L2 on the outside.
// The old interior branch returned `max(q.x,q.y)` without `- r`, so d jumped by `r` on
// the inner-box boundary and painted a flickering axis-aligned frame (worst on the bottom).
fn sd_superellipse_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>, n: f32) -> f32 {
    let r_sel = max(select_corner_radius(p, r), 0.001);
    let nn = max(n, 0.5);
    let q = abs(p) - b + vec2<f32>(r_sel);
    let qpos = max(q, vec2<f32>(0.0));
    let lp = pow(
        pow(qpos.x / r_sel, nn) + pow(qpos.y / r_sel, nn),
        1.0 / nn
    ) * r_sel;
    return min(max(q.x, q.y), 0.0) + lp - r_sel;
}

// Notch: rectangular step cut — union of (full-width, height-r) and (width-r, full-height).
fn sd_notch_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let r_sel = max(select_corner_radius(p, r), 0.0);
    if (r_sel < 0.001) {
        return sd_aabb(p, b);
    }
    let d1 = sd_aabb(p, vec2<f32>(b.x, max(b.y - r_sel, 0.0)));
    let d2 = sd_aabb(p, vec2<f32>(max(b.x - r_sel, 0.0), b.y));
    return min(d1, d2);
}

// Scoop: concave quarter-circle bite centered on the sharp corner.
fn sd_scoop_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let r_sel = max(select_corner_radius(p, r), 0.0);
    let q = abs(p) - b;
    let d_box = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0)));
    if (r_sel < 0.001) {
        return d_box;
    }
    return max(d_box, r_sel - length(q));
}

// Unified SDF dispatch based on corner_shape
fn sd_shaped_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>, shape: f32, param: f32) -> f32 {
    if (shape < 0.5) {
        return sd_round_box(p, b, r);
    } else if (shape < 1.5) {
        return sd_angle_box(p, b, r);
    } else if (shape < 2.5) {
        return sd_superellipse_box(p, b, r, param);
    } else if (shape < 3.5) {
        return sd_notch_box(p, b, r);
    } else {
        return sd_scoop_box(p, b, r);
    }
}

/// IEC 61966-2-1: display sRGB channel -> linear light (matches CPU `linearize_display_srgb`).
fn linearize_srgb_channel(u: f32) -> f32 {
    let c = clamp(u, 0.0, 1.0);
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn linearize_srgb_rgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linearize_srgb_channel(rgb.x),
        linearize_srgb_channel(rgb.y),
        linearize_srgb_channel(rgb.z),
    );
}

fn display_srgb_channel(value: f32) -> f32 {
    let channel = max(value, 0.0);
    if (channel <= 0.0031308) {
        return channel * 12.92;
    }
    return 1.055 * pow(channel, 1.0 / 2.4) - 0.055;
}

fn display_srgb_rgb(rgb: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(display_srgb_channel(rgb.r), display_srgb_channel(rgb.g), display_srgb_channel(rgb.b));
}

// CSS 3D border tones: mix toward black / white so a dark `primary` still shows a bevel.
fn bevel_dark(c: vec3<f32>) -> vec3<f32> {
    return c * 0.32;
}
fn bevel_light(c: vec3<f32>) -> vec3<f32> {
    return mix(c, vec3<f32>(1.0), 0.58);
}

// 1 = nearest edge is top or left (inset shadow side).
fn side_lit_dark(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let to_left = p.x + b.x;
    let to_right = b.x - p.x;
    let to_top = p.y + b.y;
    let to_bottom = b.y - p.y;
    let is_vert = min(to_top, to_bottom) < min(to_left, to_right);
    return select(
        select(0.0, 1.0, to_left < to_right),
        select(0.0, 1.0, to_top < to_bottom),
        is_vert
    );
}

fn bevel_tone(c: vec3<f32>, is_dark: f32) -> vec3<f32> {
    return mix(bevel_light(c), bevel_dark(c), is_dark);
}

fn copysign2(mag: vec2<f32>, sgn: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        select(-mag.x, mag.x, sgn.x >= 0.0),
        select(-mag.y, mag.y, sgn.y >= 0.0)
    );
}

fn stroke_ib(c: vec2<f32>, r: f32) -> vec2<f32> {
    return max(c - vec2<f32>(max(r, 0.0)), vec2<f32>(0.0));
}

fn stroke_peri(c: vec2<f32>, r: f32) -> f32 {
    let ib = stroke_ib(c, r);
    return max(4.0 * (ib.x + ib.y) + 6.2831853 * max(r, 0.0), 1.0);
}

fn wrap_peri(t: f32, peri: f32) -> f32 {
    var s = t - peri * floor(t / peri);
    if (s < 0.0) {
        s = s + peri;
    }
    return s;
}

/// Closest point on the border centerline. Sharp corners collapse to the vertex
/// (no 45° split), so dotted/dashed see one parameter at a right angle.
fn closest_on_centerline(p: vec2<f32>, c: vec2<f32>, r: f32) -> vec2<f32> {
    let ib = stroke_ib(c, r);
    let q = abs(p);
    if (q.x > ib.x && q.y > ib.y) {
        if (r > 0.5) {
            let d = q - ib;
            return copysign2(ib + (d / max(length(d), 1e-4)) * r, p);
        }
        return copysign2(c, p);
    }
    if (q.x > ib.x) {
        return vec2<f32>(select(-c.x, c.x, p.x >= 0.0), clamp(p.y, -ib.y, ib.y));
    }
    if (q.y > ib.y) {
        return vec2<f32>(clamp(p.x, -ib.x, ib.x), select(-c.y, c.y, p.y >= 0.0));
    }
    let dvert = c.x - q.x;
    let dhorz = c.y - q.y;
    if (dvert < dhorz) {
        return vec2<f32>(select(-c.x, c.x, p.x >= 0.0), p.y);
    }
    return vec2<f32>(p.x, select(-c.y, c.y, p.y >= 0.0));
}

/// Clockwise arc length from the top-left of the top edge.
fn centerline_t(q: vec2<f32>, c: vec2<f32>, r: f32) -> f32 {
    let ib = stroke_ib(c, r);
    let half_pi = 1.57079632;
    let arc = half_pi * r;
    let top = 2.0 * ib.x;
    let side = 2.0 * ib.y;
    let t_tr = top;
    let t_right = t_tr + arc;
    let t_br = t_right + side;
    let t_bottom = t_br + arc;
    let t_bl = t_bottom + top;
    let t_left = t_bl + arc;
    let t_tl = t_left + side;
    let eps = 0.51;

    if (abs(q.y + c.y) <= eps && q.x >= -ib.x - eps && q.x <= ib.x + eps) {
        return clamp(q.x + ib.x, 0.0, top);
    }
    if (abs(q.x - c.x) <= eps && q.y >= -ib.y - eps && q.y <= ib.y + eps) {
        return t_right + clamp(q.y + ib.y, 0.0, side);
    }
    if (abs(q.y - c.y) <= eps && q.x >= -ib.x - eps && q.x <= ib.x + eps) {
        return t_bottom + clamp(ib.x - q.x, 0.0, top);
    }
    if (abs(q.x + c.x) <= eps && q.y >= -ib.y - eps && q.y <= ib.y + eps) {
        return t_left + clamp(ib.y - q.y, 0.0, side);
    }
    if (r > 0.5) {
        if (q.x >= 0.0 && q.y <= 0.0) {
            let d = vec2<f32>(q.x - ib.x, q.y + ib.y);
            let ang = atan2(-d.y, d.x);
            return t_tr + clamp(half_pi - ang, 0.0, half_pi) * r;
        }
        if (q.x >= 0.0 && q.y >= 0.0) {
            let d = vec2<f32>(q.x - ib.x, q.y - ib.y);
            return t_br + clamp(atan2(d.y, d.x), 0.0, half_pi) * r;
        }
        if (q.x <= 0.0 && q.y >= 0.0) {
            let d = vec2<f32>(q.x + ib.x, q.y - ib.y);
            return t_bl + clamp(atan2(-d.x, d.y), 0.0, half_pi) * r;
        }
        let d = vec2<f32>(q.x + ib.x, q.y + ib.y);
        return t_tl + clamp(atan2(-d.y, -d.x), 0.0, half_pi) * r;
    }
    if (q.x >= 0.0 && q.y <= 0.0) {
        return t_tr;
    }
    if (q.x >= 0.0 && q.y >= 0.0) {
        return t_br;
    }
    if (q.x <= 0.0 && q.y >= 0.0) {
        return t_bl;
    }
    return t_tl;
}

fn centerline_point(t: f32, c: vec2<f32>, r: f32) -> vec2<f32> {
    let ib = stroke_ib(c, r);
    let half_pi = 1.57079632;
    let arc = half_pi * r;
    let top = 2.0 * ib.x;
    let side = 2.0 * ib.y;
    let peri = stroke_peri(c, r);
    var s = wrap_peri(t, peri);
    if (s <= top) {
        return vec2<f32>(-ib.x + s, -c.y);
    }
    s = s - top;
    if (s <= arc) {
        let ang = half_pi - s / max(r, 1e-4);
        return vec2<f32>(ib.x + cos(ang) * r, -ib.y - sin(ang) * r);
    }
    s = s - arc;
    if (s <= side) {
        return vec2<f32>(c.x, -ib.y + s);
    }
    s = s - side;
    if (s <= arc) {
        let ang = s / max(r, 1e-4);
        return vec2<f32>(ib.x + cos(ang) * r, ib.y + sin(ang) * r);
    }
    s = s - arc;
    if (s <= top) {
        return vec2<f32>(ib.x - s, c.y);
    }
    s = s - top;
    if (s <= arc) {
        let ang = s / max(r, 1e-4);
        return vec2<f32>(-ib.x - sin(ang) * r, ib.y + cos(ang) * r);
    }
    s = s - arc;
    if (s <= side) {
        return vec2<f32>(-c.x, ib.y - s);
    }
    s = s - side;
    let ang = s / max(r, 1e-4);
    return vec2<f32>(-ib.x - cos(ang) * r, -ib.y - sin(ang) * r);
}

fn nearest_dot_on_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, spacing: f32) -> f32 {
    let ab = b - a;
    let len = length(ab);
    if (len < 1.0) {
        return length(p - a);
    }
    let n = max(round(len / max(spacing, 1.0)), 1.0);
    let t = clamp(dot(p - a, ab) / (len * len), 0.0, 1.0) * len;
    let k = clamp(round(t * n / len), 0.0, n);
    var dmin = 1e6;
    for (var i = -1; i <= 1; i++) {
        let ki = clamp(k + f32(i), 0.0, n);
        dmin = min(dmin, length(p - mix(a, b, ki / n)));
    }
    return dmin;
}

/// True circular dots. Sharp corners always get a corner-centered circle.
fn dotted_mask(p: vec2<f32>, c: vec2<f32>, bw: f32, radii: vec4<f32>) -> f32 {
    let r = min(select_corner_radius(p, radii), min(c.x, c.y));
    let spacing = max(bw * 2.0, 8.0);
    let dot_r = bw * 0.48;
    var dmin = 1e6;
    if (r > 0.5) {
        let q = closest_on_centerline(p, c, r);
        let t = centerline_t(q, c, r);
        let peri = stroke_peri(c, r);
        let n = max(round(peri / spacing), 4.0);
        let period = peri / n;
        let t0 = round(t / period) * period;
        dmin = 1e6;
        for (var k = -1; k <= 1; k++) {
            let dp = centerline_point(wrap_peri(t0 + f32(k) * period, peri), c, r);
            dmin = min(dmin, length(p - dp));
        }
    } else {
        let tl = vec2<f32>(-c.x, -c.y);
        let tr = vec2<f32>(c.x, -c.y);
        let br = vec2<f32>(c.x, c.y);
        let bl = vec2<f32>(-c.x, c.y);
        dmin = nearest_dot_on_segment(p, tl, tr, spacing);
        dmin = min(dmin, nearest_dot_on_segment(p, tr, br, spacing));
        dmin = min(dmin, nearest_dot_on_segment(p, br, bl, spacing));
        dmin = min(dmin, nearest_dot_on_segment(p, bl, tl, spacing));
    }
    return 1.0 - smoothstep(dot_r - 0.55, dot_r + 0.55, dmin);
}

/// Dash pattern with a dash at both endpoints (n gaps, n+1 dashes), scaled to `len`.
fn dash_both_ends(t: f32, len: f32, dash_len: f32, gap_len: f32) -> f32 {
    if (len <= dash_len * 1.25) {
        return 1.0;
    }
    let period_base = dash_len + gap_len;
    let n_gaps = max(round((len - dash_len) / period_base), 1.0);
    let n_dashes = n_gaps + 1.0;
    let scale = len / max(n_dashes * dash_len + n_gaps * gap_len, 1.0);
    let d = dash_len * scale;
    let g = gap_len * scale;
    let period = d + g;
    let t0 = clamp(t, 0.0, len);
    if (t0 >= len - d) {
        return 1.0;
    }
    let local = t0 - period * floor(t0 / period);
    return 1.0 - smoothstep(d - 0.6, d + 0.6, local);
}

/// Sharp corners use a square join cap (the full bw×bw L). Closest-point 1D dash
/// still Voronoi-splits that square at 45° and "bites" a triangle out of the join.
fn dashed_mask(p: vec2<f32>, c: vec2<f32>, bw: f32, radii: vec4<f32>) -> f32 {
    let r = min(select_corner_radius(p, radii), min(c.x, c.y));
    let dash_len = max(bw * 4.0, 12.0);
    let gap_len = max(bw * 2.5, 8.0);

    if (r > 0.5) {
        let q = closest_on_centerline(p, c, r);
        let t = centerline_t(q, c, r);
        let peri = stroke_peri(c, r);
        return dash_both_ends(t, peri, dash_len, gap_len);
    }

    let q = abs(p);
    let half = bw * 0.5;
    // Intersection of the two border strips: square join, never a 45° cut.
    if (q.x >= c.x - half && q.y >= c.y - half) {
        return 1.0;
    }
    if (q.y >= c.y - half) {
        let len = max(2.0 * c.x, 1.0);
        let t_local = select(p.x + c.x, c.x - p.x, p.y >= 0.0);
        return dash_both_ends(t_local, len, dash_len, gap_len);
    }
    if (q.x >= c.x - half) {
        let len = max(2.0 * c.y, 1.0);
        let t_local = select(c.y - p.y, p.y + c.y, p.x >= 0.0);
        return dash_both_ends(t_local, len, dash_len, gap_len);
    }
    return 0.0;
}

fn clip_edge_left(a: vec2<f32>, b: vec2<f32>, p: vec2<f32>) -> f32 {
    return (b.x - a.x) * (p.y - a.y) - (p.x - a.x) * (b.y - a.y);
}

fn round_clip_contains(p: vec2<f32>, xywh: vec4<f32>, radii: vec4<f32>) -> bool {
    if (xywh.z < 0.5 || xywh.w < 0.5) {
        return true;
    }
    let half = xywh.zw * 0.5;
    let local = p - (xywh.xy + half);
    return sd_round_box(local, half, radii) <= 0.0;
}

fn clip_poly_contains(p: vec2<f32>, inst_id: u32) -> bool {
    let n = arrayLength(&clip_polys);
    if (inst_id >= n) {
        return true;
    }
    // Read only active fields. Copying this struct and passing both vertex arrays
    // by value creates large fragment-local arrays even for rectangular clips.
    if (clip_polys[inst_id].count >= 3u) {
        if (!clip_layer_contains(p, clip_polys[inst_id].count, clip_polys[inst_id].fill, inst_id, false)) {
            return false;
        }
    }
    if (clip_polys[inst_id].count2 >= 3u) {
        if (!clip_layer_contains(p, clip_polys[inst_id].count2, clip_polys[inst_id].fill2, inst_id, true)) {
            return false;
        }
    }
    if (!round_clip_contains(p, clip_polys[inst_id].round_rect, clip_polys[inst_id].round_radii)) {
        return false;
    }
    if (!round_clip_contains(p, clip_polys[inst_id].round_rect2, clip_polys[inst_id].round_radii2)) {
        return false;
    }
    return true;
}

fn clip_point(inst_id: u32, index: u32, second: bool) -> vec2<f32> {
    if (second) {
        return clip_polys[inst_id].pts2[index];
    }
    return clip_polys[inst_id].pts[index];
}

fn clip_layer_contains(p: vec2<f32>, count: u32, fill: u32, inst_id: u32, second: bool) -> bool {
    if (count < 3u) {
        return true;
    }
    if (fill == 0u) {
        var inside = false;
        var j = count - 1u;
        for (var i = 0u; i < count; i++) {
            let a = clip_point(inst_id, i, second);
            let b = clip_point(inst_id, j, second);
            if ((a.y > p.y) != (b.y > p.y)) {
                let x_int = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
                if (p.x < x_int) {
                    inside = !inside;
                }
            }
            j = i;
        }
        return inside;
    }
    var w = 0;
    var jn = count - 1u;
    for (var i = 0u; i < count; i++) {
        let a = clip_point(inst_id, i, second);
        let b = clip_point(inst_id, jn, second);
        if (a.y <= p.y) {
            if (b.y > p.y && clip_edge_left(a, b, p) > 0.0) {
                w = w + 1;
            }
        } else {
            if (b.y <= p.y && clip_edge_left(a, b, p) < 0.0) {
                w = w - 1;
            }
        }
        jn = i;
    }
    return w != 0;
}

fn optical_glass(in: VertexOutput, distance: f32, local_dx: vec2<f32>, local_dy: vec2<f32>) -> vec4<f32> {
    let uv = in.frag_px / globals.resolution;
    let half_size = in.rect_size * 0.5;
    let packed = in.extra_and_id.x;
    let encoded_param = floor(packed / 256.0);
    let shape = floor((packed - encoded_param * 256.0) / 16.0);
    let epsilon = 0.25;
    let gradient = vec2<f32>(
        sd_shaped_box(in.p + vec2<f32>(epsilon, 0.0), half_size, in.radii, shape, encoded_param / 100.0)
            - sd_shaped_box(in.p - vec2<f32>(epsilon, 0.0), half_size, in.radii, shape, encoded_param / 100.0),
        sd_shaped_box(in.p + vec2<f32>(0.0, epsilon), half_size, in.radii, shape, encoded_param / 100.0)
            - sd_shaped_box(in.p - vec2<f32>(0.0, epsilon), half_size, in.radii, shape, encoded_param / 100.0)
    );
    let screen_gradient = vec2<f32>(dot(gradient, local_dx), dot(gradient, local_dy));
    let outward = screen_gradient / max(length(screen_gradient), 0.0001);
    let depth = max(-distance, 0.0);
    let edge_width = max(min(in.glass.y * 2.0, min(half_size.x, half_size.y)), 0.5);
    let slope = (1.0 - smoothstep(0.0, edge_width, depth)) * 0.98;
    let normal = vec3<f32>(outward * slope, sqrt(max(1.0 - slope * slope, 0.0001)));
    let ray = refract(vec3<f32>(0.0, 0.0, -1.0), normal, 1.0 / in.glass.x);
    let pixel_scale = inverseSqrt(max(length(local_dx) * length(local_dy), 0.0001));
    let displacement = ray.xy / max(abs(ray.z), 0.05) * in.glass.y * pixel_scale / globals.resolution;
    let uv_min = vec2<f32>(0.5) / globals.resolution;
    let uv_max = vec2<f32>(1.0) - uv_min;
    let red = textureSampleLevel(backdrop_tex, backdrop_samp, clamp(uv + displacement * (1.0 + in.glass.z), uv_min, uv_max), 0.0);
    let green = textureSampleLevel(backdrop_tex, backdrop_samp, clamp(uv + displacement, uv_min, uv_max), 0.0);
    let blue = textureSampleLevel(backdrop_tex, backdrop_samp, clamp(uv + displacement * (1.0 - in.glass.z), uv_min, uv_max), 0.0);
    let base_reflectance = pow((in.glass.x - 1.0) / (in.glass.x + 1.0), 2.0);
    let fresnel = base_reflectance + (1.0 - base_reflectance) * pow(1.0 - normal.z, 5.0);
    let light = normalize(vec3<f32>(-0.45, -0.65, 0.6));
    let reflected = reflect(vec3<f32>(0.0, 0.0, -1.0), normal);
    let specular = pow(max(dot(reflected, light), 0.0), 48.0);
    let rim = exp(-depth / 1.25) * max(dot(outward, normalize(light.xy)), 0.0);
    let reflection_uv = clamp(uv + reflected.xy * in.glass.y * pixel_scale / globals.resolution, uv_min, uv_max);
    let environment = textureSampleLevel(backdrop_tex, backdrop_samp, reflection_uv, 0.0).rgb;
    var rgb = vec3<f32>(red.r, green.g, blue.b);
    rgb = mix(rgb, environment, fresnel * in.glass.w);
    rgb = rgb + vec3<f32>((specular * 0.18 + rim * 0.2) * in.glass.w);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), green.a);
}

fn paint_rect(in: VertexOutput) -> vec4<f32> {
    let local_dx = dpdx(in.p);
    let local_dy = dpdy(in.p);
    let b = in.rect_size * 0.5;
    // Large axis-aligned fills skip SDF; overflow still uses clip_poly + clip_pos.
    let flags = u32(in.extra_and_id.w);
    let simple_fill = (flags & 2u) != 0u;
    // Derivatives must precede storage-dependent clip/discard control flow.
    // ANGLE maps clip_polys to UAVs; FXC rejects gradients after those discards.
    let packed = in.extra_and_id.x;
    let param_enc = floor(packed / 256.0);
    let rem = packed - param_enc * 256.0;
    let shape_type = floor(rem / 16.0);
    let bstyle = rem - shape_type * 16.0;
    let shape_param = param_enc / 100.0;
    var d = 0.0;
    var aa = 1.0;
    if (!simple_fill) {
        d = sd_shaped_box(in.p, b, in.radii, shape_type, shape_param);
        aa = shape_aa(in.p, b, in.radii, d, shape_type);
    }
    // World overflow / clip-path (rotated descendants, restrict shells).
    if ((flags & 1u) != 0u && !clip_poly_contains(in.frag_px, u32(in.extra_and_id.y + 0.5))) {
        discard;
    }
    if (simple_fill) {
        if (in.clip_rect.z >= 0.0 && in.clip_rect.w >= 0.0) {
            let c1 = in.clip_rect.xy + in.clip_rect.zw;
            if (in.p.x < in.clip_rect.xy.x || in.p.x >= c1.x
                || in.p.y < in.clip_rect.xy.y || in.p.y >= c1.y) {
                discard;
            }
        }
        var rgb = in.color.rgb;
        if (globals.premultiply > 0.5) {
            rgb = rgb * in.color.a;
        }
        return vec4<f32>(rgb, in.color.a);
    }

    // Axis-aligned overflow: local border box (same space as SDF `in.p`).
    if (in.clip_rect.z >= 0.0 && in.clip_rect.w >= 0.0) {
        let c1 = in.clip_rect.xy + in.clip_rect.zw;
        if (in.p.x < in.clip_rect.xy.x || in.p.x >= c1.x
            || in.p.y < in.clip_rect.xy.y || in.p.y >= c1.y) {
            discard;
        }
    }

    // Compute alpha for the inner rect body
    let alpha_bg = 1.0 - smoothstep(-aa, aa, d);
    if (in.glass.x < 0.0) {
        let u = in.p.x / max(in.rect_size.x, 1.0) + 0.5 + in.glass.w;
        let band = max(1.0 - abs(u - in.glass.y) / in.glass.z, 0.0);
        let alpha = in.color.a * alpha_bg * band * band;
        var rgb = in.color.rgb;
        if (globals.premultiply > 0.5) {
            rgb = rgb * alpha;
        }
        return vec4<f32>(rgb, alpha);
    }
    // Radius changes coverage, not the fill RGB. Decode a constant fill once
    // per vertex instead of three pow() calls for each pixel of rounded panels.
    if ((flags & 4u) != 0u) {
        let alpha = in.color.a * alpha_bg;
        var rgb = in.color.rgb;
        if (globals.premultiply > 0.5) {
            rgb = rgb * alpha;
        }
        return vec4<f32>(rgb, alpha);
    }

    var final_color = in.color;
    if (in.border_width > 0.0 && bstyle > 0.5) {
        let d_inner = sd_shaped_box(in.p, b - vec2<f32>(in.border_width), in.radii - vec4<f32>(in.border_width), shape_type, shape_param);
        let in_border = step(0.0, d_inner); // 1 in border region, 0 inside

        if (bstyle < 1.5) {
            // Solid
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(in.border_color, in.color, alpha_border);
        } else if (bstyle < 2.5) {
            // Dotted — circular stamps on the centerline (not 1D slices of the L-border).
            let bw = in.border_width;
            let r_mid = max(in.radii - vec4<f32>(bw * 0.5), vec4<f32>(0.0));
            let mask = dotted_mask(in.p, b - vec2<f32>(bw * 0.5), bw, r_mid);
            let fill_clip = smoothstep(-aa, aa, d_inner);
            let border_alpha = mask * fill_clip;
            final_color = mix(in.color, in.border_color, border_alpha);
        } else if (bstyle < 3.5) {
            // Dashed — per-side pattern + square corner joins (no 45° Voronoi bite).
            let bw = in.border_width;
            let r_mid = max(in.radii - vec4<f32>(bw * 0.5), vec4<f32>(0.0));
            let mask = dashed_mask(in.p, b - vec2<f32>(bw * 0.5), bw, r_mid);
            let border_alpha = in_border * mask;
            final_color = mix(in.color, in.border_color, border_alpha);
        } else if (bstyle < 4.5) {
            // Double: two lines with gap
            let third = in.border_width / 3.0;
            let d_outer_inner = sd_shaped_box(in.p, b - vec2<f32>(third), in.radii - vec4<f32>(third), shape_type, shape_param);
            let d_inner_outer = sd_shaped_box(in.p, b - vec2<f32>(in.border_width - third), in.radii - vec4<f32>(in.border_width - third), shape_type, shape_param);
            let outer_line = step(0.0, d_outer_inner) * (1.0 - smoothstep(-aa, aa, d));
            let inner_line = (1.0 - smoothstep(-aa, aa, d_inner_outer)) * step(0.0, d_inner);
            let border_alpha = max(outer_line, inner_line);
            final_color = mix(in.color, in.border_color, border_alpha);
        } else if (bstyle < 5.5) {
            // Groove: outer band = inset lighting, inner band = outset (carved trench).
            let half = in.border_width * 0.5;
            let d_mid = sd_shaped_box(in.p, b - vec2<f32>(half), in.radii - vec4<f32>(half), shape_type, shape_param);
            let in_outer = in_border * (1.0 - step(0.0, d_mid));
            let in_inner_half = step(0.0, d_mid) * step(0.0, d_inner);
            let is_dark = side_lit_dark(in.p, b);
            let rgb = bevel_tone(in.border_color.rgb, is_dark) * in_outer
                + bevel_tone(in.border_color.rgb, 1.0 - is_dark) * in_inner_half;
            let band = max(in_outer, in_inner_half);
            final_color = mix(in.color, vec4<f32>(rgb, in.border_color.a), band);
        } else if (bstyle < 6.5) {
            // Ridge: outer = outset, inner = inset (raised ridge).
            let half = in.border_width * 0.5;
            let d_mid = sd_shaped_box(in.p, b - vec2<f32>(half), in.radii - vec4<f32>(half), shape_type, shape_param);
            let in_outer = in_border * (1.0 - step(0.0, d_mid));
            let in_inner_half = step(0.0, d_mid) * step(0.0, d_inner);
            let is_dark = side_lit_dark(in.p, b);
            let rgb = bevel_tone(in.border_color.rgb, 1.0 - is_dark) * in_outer
                + bevel_tone(in.border_color.rgb, is_dark) * in_inner_half;
            let band = max(in_outer, in_inner_half);
            final_color = mix(in.color, vec4<f32>(rgb, in.border_color.a), band);
        } else if (bstyle < 7.5) {
            // Inset: top/left dark, bottom/right light (sunken, single tone per side).
            let is_dark = side_lit_dark(in.p, b);
            let side_rgb = bevel_tone(in.border_color.rgb, is_dark);
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(vec4<f32>(side_rgb, in.border_color.a), in.color, alpha_border);
        } else {
            // Outset: reverse of inset (raised).
            let is_dark = 1.0 - side_lit_dark(in.p, b);
            let side_rgb = bevel_tone(in.border_color.rgb, is_dark);
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(vec4<f32>(side_rgb, in.border_color.a), in.color, alpha_border);
        }
    }

    // CSS backdrop-filter: sample the blurred capture at this fragment, then composite
    // this instance's background/border on top. SDF `alpha_bg` still clips to radii.
    if (in.extra_and_id.z > 0.5) {
        let uv = in.frag_px / globals.resolution;
        var blurred = textureSampleLevel(backdrop_tex, backdrop_samp, uv, 0.0);
        if (in.glass.x >= 1.0 && in.glass.y > 0.0) {
            blurred = optical_glass(in, d, local_dx, local_dy);
        }
        let material_opacity = 1.0 - fract(in.extra_and_id.w) * 2.0;
        let src_a = clamp(final_color.a / max(material_opacity, 0.00001), 0.0, 1.0);
        // Page capture is opaque → frost α=1 (CSS backdrop-filter). Dummy / empty
        // capture has α=0 → keep the scrim's own α so already-drawn glyphs are not
        // punched out by an opaque black hole.
        let out_a = src_a + (1.0 - src_a) * blurred.a;
        final_color = vec4<f32>(
            display_srgb_rgb((blurred.rgb * blurred.a * (1.0 - src_a) + linearize_srgb_rgb(final_color.rgb) * src_a) / max(out_a, 0.00001)),
            out_a * material_opacity
        );
    }

    // Compute shadow factor.
    var shadow_alpha = 0.0;
    let shadow_active = in.shadow_color.a > 0.001
        && (in.shadow_blur > 0.001
            || abs(in.shadow_spread) > 0.001
            || abs(in.shadow_offset.x) > 0.001
            || abs(in.shadow_offset.y) > 0.001);
    if (shadow_active) {
        let blur = max(in.shadow_blur, aa);
        let spread_signed = in.shadow_spread;
        if (in.shadow_inset > 0.5) {
            let inner_b = max(b - vec2<f32>(spread_signed), vec2<f32>(0.0));
            let inner_r = max(in.radii - vec4<f32>(spread_signed), vec4<f32>(0.0));
            let d_inner_sh = sd_shaped_box(in.p - in.shadow_offset, inner_b, inner_r, shape_type, shape_param);
            let from_outer = smoothstep(-blur, 0.0, d);
            let from_inner = smoothstep(0.0, blur, d_inner_sh);
            shadow_alpha = (from_outer * from_inner) * in.shadow_color.a;
        } else {
            let d_shadow = sd_shaped_box(
                in.p - in.shadow_offset,
                b + vec2<f32>(spread_signed),
                in.radii + vec4<f32>(spread_signed),
                shape_type, shape_param
            );
            // Reversed smoothstep edges are undefined on GLSL / GLES drivers.
            shadow_alpha = (1.0 - smoothstep(0.0, blur, max(0.0, d_shadow))) * in.shadow_color.a;
        }
    }

    // Blend: composite shadow over fill
    let fill_alpha = final_color.a * alpha_bg;
    let s_a = shadow_alpha * select((1.0 - alpha_bg), alpha_bg, in.shadow_inset > 0.5);
    let out_a = s_a + fill_alpha * (1.0 - s_a);
    var out_rgb = vec3<f32>(0.0);
    if (out_a > 0.0) {
        out_rgb = (in.shadow_color.rgb * s_a + final_color.rgb * fill_alpha * (1.0 - s_a)) / out_a;
    }

    var rgb = linearize_srgb_rgb(out_rgb);
    if (globals.premultiply > 0.5) {
        rgb = rgb * out_a;
    }
    return vec4<f32>(rgb, out_a);
}

// Separate entry point keeps shadow/backdrop/decorative-border temporaries out
// of the ordinary fill pipeline. CPU dispatch admits only normal boxes with
// no active shadow/backdrop and a none/solid border, in unchanged painter order.
@fragment
fn fs_solid(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = u32(in.extra_and_id.w);
    var d = 0.0;
    var aa = 1.0;
    if ((flags & 2u) == 0u) {
        d = sd_shaped_box(in.p, in.rect_size * 0.5, in.radii, 0.0, 0.0);
        aa = shape_aa(in.p, in.rect_size * 0.5, in.radii, d, 0.0);
    }
    if ((flags & 1u) != 0u && !clip_poly_contains(in.frag_px, u32(in.extra_and_id.y + 0.5))) {
        discard;
    }
    if (in.clip_rect.z >= 0.0 && in.clip_rect.w >= 0.0) {
        let c1 = in.clip_rect.xy + in.clip_rect.zw;
        if (in.p.x < in.clip_rect.xy.x || in.p.x >= c1.x
            || in.p.y < in.clip_rect.xy.y || in.p.y >= c1.y) {
            discard;
        }
    }
    var coverage = 1.0;
    var color = in.color;
    if ((flags & 2u) == 0u) {
        coverage = 1.0 - smoothstep(-aa, aa, d);
        if (in.border_width > 0.0 && in.extra_and_id.x > 0.5) {
            let border_coverage = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            color = mix(in.border_color, in.color, border_coverage);
        }
    }
    let alpha = color.a * coverage;
    var rgb = color.rgb;
    if ((flags & 4u) == 0u) {
        rgb = linearize_srgb_rgb(rgb);
    }
    if (globals.premultiply > 0.5) {
        rgb = rgb * alpha;
    }
    return vec4<f32>(rgb, alpha);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return paint_rect(in);
}

@fragment
fn fs_interior(in: VertexOutput) -> @location(0) vec4<f32> {
    let flags = u32(in.extra_and_id.w);
    if ((flags & 1u) != 0u && !clip_poly_contains(in.frag_px, u32(in.extra_and_id.y + 0.5))) { discard; }
    if (in.clip_rect.z >= 0.0 && in.clip_rect.w >= 0.0) {
        let c1 = in.clip_rect.xy + in.clip_rect.zw;
        if (in.p.x < in.clip_rect.xy.x || in.p.x >= c1.x || in.p.y < in.clip_rect.xy.y || in.p.y >= c1.y) { discard; }
    }
    // CPU dispatch proves full opacity and coverage, without inset shadow,
    // backdrop sampling or a transformed edge. Color was decoded in the VS.
    return vec4<f32>(in.color.rgb, 1.0);
}
