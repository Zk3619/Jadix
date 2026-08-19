// SDF Based Rounded Rectangle Shader

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct InstanceInput {
    @location(2) rect_pos: vec2<f32>,
    @location(3) rect_size: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) radii: vec4<f32>, // top-right, bottom-right, top-left, bottom-left
    @location(7) border_width: f32,
    @location(8) shadow_color: vec4<f32>,
    @location(9) shadow_offset: vec2<f32>,
    @location(10) shadow_blur: f32,
    @location(11) shadow_spread: f32,
    @location(12) shadow_inset: f32,
    @location(13) rotate_deg: f32,
    @location(14) clip_pos: vec2<f32>,
    @location(15) clip_size_and_extra: vec3<f32>, // xy=clip_size, z=packed border_extra
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) rect_size: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) shadow_color: vec4<f32>,
    @location(6) shadow_offset: vec2<f32>,
    @location(7) shadow_blur: f32,
    @location(8) shadow_spread: f32,
    @location(9) shadow_inset: f32,
    @location(10) p: vec2<f32>, // local point relative to rect center (unrotated)
    @location(11) clip_pos: vec2<f32>,
    @location(12) clip_size: vec2<f32>,
    @location(13) frag_px: vec2<f32>,
    @location(14) border_extra: f32,
};

struct Globals {
    resolution: vec2<f32>,
    premultiply: f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

fn rotate2(v: vec2<f32>, c: f32, s: f32) -> vec2<f32> {
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    let center = instance.rect_pos + instance.rect_size * 0.5;
    let angle = instance.rotate_deg * 0.017453292519943295;
    let ca = cos(angle);
    let sa = sin(angle);

    // Extend quad: outset needs extra for blur/offset/spread; inset shadow lives inside the rect.
    let spread = max(0.0, instance.shadow_spread);
    var ext = vec2<f32>(abs(instance.shadow_offset.x), abs(instance.shadow_offset.y))
        + vec2<f32>(instance.shadow_blur * 2.0 + instance.border_width + spread);
    if (instance.shadow_inset > 0.5) {
        // Inset ring can extend inward with positive spread; keep margin for blur + spread sampling.
        ext = vec2<f32>(
            instance.shadow_blur * 2.0 + instance.border_width + abs(instance.shadow_spread)
        );
    }

    // Grow AABB enough to cover rotated corners.
    let half = instance.rect_size * 0.5 + ext;
    let c0 = abs(rotate2(vec2<f32>( half.x,  half.y), ca, sa));
    let c1 = abs(rotate2(vec2<f32>( half.x, -half.y), ca, sa));
    let c2 = abs(rotate2(vec2<f32>(-half.x,  half.y), ca, sa));
    let c3 = abs(rotate2(vec2<f32>(-half.x, -half.y), ca, sa));
    let rot_half = max(max(c0, c1), max(c2, c3));

    let local = (model.position - 0.5) * 2.0 * rot_half;
    let physical_pos = center + rotate2(local, ca, sa);

    // Convert to NDC
    let x = (physical_pos.x / globals.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (physical_pos.y / globals.resolution.y) * 2.0;

    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    // SDF samples in unrotated local space.
    out.p = local;
    out.rect_size = instance.rect_size;
    out.color = instance.color;
    out.border_color = instance.border_color;
    out.radii = instance.radii;
    out.border_width = instance.border_width;
    out.shadow_color = instance.shadow_color;
    out.shadow_offset = instance.shadow_offset;
    out.shadow_blur = instance.shadow_blur;
    out.shadow_spread = instance.shadow_spread;
    out.shadow_inset = instance.shadow_inset;
    out.clip_pos = instance.clip_pos;
    out.clip_size = instance.clip_size_and_extra.xy;
    out.frag_px = physical_pos;
    out.border_extra = instance.clip_size_and_extra.z;

    return out;
}

// 2D Box Signed Distance Field (round corners)
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b + vec2<f32>(r_sel);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r_sel;
}

// Angle (chamfer/bevel) corner SDF
fn sd_angle_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b;
    let outer = max(q.x, q.y);
    if (outer > -r_sel) {
        return outer + r_sel - max(0.0, r_sel - (q.x + q.y + r_sel * 2.0) * 0.5);
    }
    return outer;
}

// Superellipse corner SDF: |x/a|^n + |y/b|^n = 1
fn sd_superellipse_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>, n: f32) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b + vec2<f32>(r_sel);
    if (q.x <= 0.0 && q.y <= 0.0) {
        return max(q.x, q.y);
    }
    let qp = max(q, vec2<f32>(0.0));
    let e = pow(pow(qp.x / max(r_sel, 0.001), n) + pow(qp.y / max(r_sel, 0.001), n), 1.0 / n) * r_sel;
    return e - r_sel;
}

// Notch corner SDF: right-angle step cut
fn sd_notch_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b + vec2<f32>(r_sel);
    if (q.x <= 0.0 && q.y <= 0.0) {
        return max(q.x, q.y);
    }
    if (q.x > 0.0 && q.y > 0.0) {
        return max(q.x, q.y);
    }
    return max(q.x, q.y);
}

// Scoop (concave arc) corner SDF
fn sd_scoop_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b;
    let outer = max(q.x, q.y);
    // In the corner region, invert the rounding: cut inward
    if (q.x > -r_sel && q.y > -r_sel && r_sel > 0.0) {
        let cq = q + vec2<f32>(r_sel);
        let corner_dist = r_sel - length(cq);
        return max(outer, corner_dist);
    }
    return outer;
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

// Dotted pattern: returns 0 in gaps, 1 on dots
fn dotted_mask(p: vec2<f32>, b: vec2<f32>, bw: f32) -> f32 {
    let period = bw * 2.0;
    // Project onto nearest edge to get edge-local coordinate
    let q = abs(p);
    let dx = b.x - q.x;
    let dy = b.y - q.y;
    var t: f32;
    if (dx < dy) {
        t = q.y; // on left/right edge, parameterize by y
    } else {
        t = q.x; // on top/bottom edge, parameterize by x
    }
    let phase = fract(t / period);
    let dot_r = bw * 0.45;
    let center = 0.25;
    let dist = abs(phase - center) * period;
    return smoothstep(dot_r + 0.5, dot_r - 0.5, dist);
}

// Dashed pattern
fn dashed_mask(p: vec2<f32>, b: vec2<f32>, bw: f32) -> f32 {
    let dash_len = bw * 3.0;
    let gap_len = bw * 1.5;
    let period = dash_len + gap_len;
    let q = abs(p);
    let dx = b.x - q.x;
    let dy = b.y - q.y;
    var t: f32;
    if (dx < dy) {
        t = q.y;
    } else {
        t = q.x;
    }
    let phase = fract(t / period) * period;
    return smoothstep(0.5, -0.5, phase - dash_len);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // CSS overflow clip
    if (in.clip_size.x >= 0.0 && in.clip_size.y >= 0.0) {
        let c1 = in.clip_pos + in.clip_size;
        if (in.frag_px.x < in.clip_pos.x || in.frag_px.x >= c1.x
            || in.frag_px.y < in.clip_pos.y || in.frag_px.y >= c1.y) {
            discard;
        }
    }

    let b = in.rect_size * 0.5;

    // Unpack border_extra: border_style + corner_shape * 16 + round(corner_param * 100) * 256
    let packed = in.border_extra;
    let bstyle = packed % 16.0;
    let shape_type = floor(packed / 16.0) % 16.0;
    let shape_param = floor(packed / 256.0) / 100.0;

    // SDF distance to the edge of the shaped box
    let d = sd_shaped_box(in.p, b, in.radii, shape_type, shape_param);

    // Anti-aliasing width
    let aa = 1.0;

    // Compute alpha for the inner rect body
    let alpha_bg = 1.0 - smoothstep(-aa, aa, d);

    var final_color = in.color;
    if (in.border_width > 0.0 && bstyle > 0.5) {
        let d_inner = sd_shaped_box(in.p, b - vec2<f32>(in.border_width), in.radii - vec4<f32>(in.border_width), shape_type, shape_param);
        let in_border = step(0.0, d_inner); // 1 in border region, 0 inside

        if (bstyle < 1.5) {
            // Solid
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(in.border_color, in.color, alpha_border);
        } else if (bstyle < 2.5) {
            // Dotted
            let mask = dotted_mask(in.p, b - vec2<f32>(in.border_width * 0.5), in.border_width);
            let border_alpha = in_border * mask;
            final_color = mix(in.color, in.border_color, border_alpha);
        } else if (bstyle < 3.5) {
            // Dashed
            let mask = dashed_mask(in.p, b - vec2<f32>(in.border_width * 0.5), in.border_width);
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
            // Groove: outer half dark, inner half light
            let half = in.border_width * 0.5;
            let d_mid = sd_shaped_box(in.p, b - vec2<f32>(half), in.radii - vec4<f32>(half), shape_type, shape_param);
            let in_outer = in_border * (1.0 - step(0.0, d_mid));
            let in_inner_half = step(0.0, d_mid) * step(0.0, d_inner);
            let dark = in.border_color.rgb * 0.6;
            let light = min(in.border_color.rgb * 1.4, vec3<f32>(1.0));
            let groove_rgb = dark * in_outer + light * in_inner_half;
            let groove_a = max(in_outer, in_inner_half) * in.border_color.a;
            final_color = mix(in.color, vec4<f32>(groove_rgb / max(groove_a, 0.001), groove_a), max(in_outer, in_inner_half));
        } else if (bstyle < 6.5) {
            // Ridge: reverse of groove
            let half = in.border_width * 0.5;
            let d_mid = sd_shaped_box(in.p, b - vec2<f32>(half), in.radii - vec4<f32>(half), shape_type, shape_param);
            let in_outer = in_border * (1.0 - step(0.0, d_mid));
            let in_inner_half = step(0.0, d_mid) * step(0.0, d_inner);
            let light = min(in.border_color.rgb * 1.4, vec3<f32>(1.0));
            let dark = in.border_color.rgb * 0.6;
            let ridge_rgb = light * in_outer + dark * in_inner_half;
            let ridge_a = max(in_outer, in_inner_half) * in.border_color.a;
            final_color = mix(in.color, vec4<f32>(ridge_rgb / max(ridge_a, 0.001), ridge_a), max(in_outer, in_inner_half));
        } else if (bstyle < 7.5) {
            // Inset: top/left dark, bottom/right light
            let is_top_left = select(0.0, 1.0, in.p.x < 0.0 || in.p.y < 0.0);
            let dark = in.border_color.rgb * 0.6;
            let light = min(in.border_color.rgb * 1.4, vec3<f32>(1.0));
            let side_rgb = mix(light, dark, is_top_left);
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(vec4<f32>(side_rgb, in.border_color.a), in.color, alpha_border);
        } else {
            // Outset: reverse of inset
            let is_top_left = select(0.0, 1.0, in.p.x < 0.0 || in.p.y < 0.0);
            let dark = in.border_color.rgb * 0.6;
            let light = min(in.border_color.rgb * 1.4, vec3<f32>(1.0));
            let side_rgb = mix(dark, light, is_top_left);
            let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
            final_color = mix(vec4<f32>(side_rgb, in.border_color.a), in.color, alpha_border);
        }
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
            shadow_alpha = smoothstep(blur, 0.0, max(0.0, d_shadow)) * in.shadow_color.a;
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
