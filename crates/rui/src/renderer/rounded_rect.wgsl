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
    @location(15) clip_size: vec2<f32>,
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
};

struct Globals {
    resolution: vec2<f32>,
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
    out.clip_size = instance.clip_size;
    out.frag_px = physical_pos;

    return out;
}

// 2D Box Signed Distance Field
fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var r_sel = r.x;
    if (p.x > 0.0 && p.y > 0.0) { r_sel = r.y; }
    else if (p.x < 0.0 && p.y > 0.0) { r_sel = r.w; }
    else if (p.x < 0.0 && p.y < 0.0) { r_sel = r.z; }

    let q = abs(p) - b + vec2<f32>(r_sel);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r_sel;
}

/// IEC 61966-2-1: display sRGB channel → linear light (matches CPU `linearize_display_srgb`).
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // CSS overflow clip: keep full rounded-rect geometry (borders stay on true edges)
    // and discard fragments outside the scroll/overflow clip. Intersecting the rect into
    // the visible box would re-draw borders on the viewport edge ("sticky" top/bottom).
    if (in.clip_size.x >= 0.0 && in.clip_size.y >= 0.0) {
        let c1 = in.clip_pos + in.clip_size;
        if (in.frag_px.x < in.clip_pos.x || in.frag_px.x >= c1.x
            || in.frag_px.y < in.clip_pos.y || in.frag_px.y >= c1.y) {
            discard;
        }
    }

    let b = in.rect_size * 0.5;

    // SDF distance to the edge of the rounded box
    let d = sd_round_box(in.p, b, in.radii);

    // Anti-aliasing width
    let aa = 1.0;

    // Compute alpha for the inner rect body
    let alpha_bg = 1.0 - smoothstep(-aa, aa, d);

    var final_color = in.color;
    if (in.border_width > 0.0) {
        let alpha_border = 1.0 - smoothstep(-aa, aa, d + in.border_width);
        final_color = mix(in.border_color, in.color, alpha_border);
    }

    // Compute shadow factor.
    // CSS `box-shadow: 0 0 0 <spread> <color>` (Tailwind / shadcn `ring-*`) uses blur=0 +
    // positive spread: a hard band outside the border box. Gate on color + blur/spread/offset,
    // not blur alone — otherwise Hard focus rings never paint.
    var shadow_alpha = 0.0;
    let shadow_active = in.shadow_color.a > 0.001
        && (in.shadow_blur > 0.001
            || abs(in.shadow_spread) > 0.001
            || abs(in.shadow_offset.x) > 0.001
            || abs(in.shadow_offset.y) > 0.001);
    if (shadow_active) {
        // Hard rings (blur≈0) still need ~1px AA; soft rings keep their blur radius.
        let blur = max(in.shadow_blur, aa);
        let spread_signed = in.shadow_spread;
        if (in.shadow_inset > 0.5) {
            // Inset (CSS): shadow in the ring between the border box and an inner rounded rect
            // shrunk by spread; offset moves the inner boundary. Positive spread expands shadow inward.
            let inner_b = max(b - vec2<f32>(spread_signed), vec2<f32>(0.0));
            let inner_r = max(in.radii - vec4<f32>(spread_signed), vec4<f32>(0.0));
            let d_inner = sd_round_box(in.p - in.shadow_offset, inner_b, inner_r);
            // Fade from outer edge inward, and from inner shadow boundary outward.
            let from_outer = smoothstep(-blur, 0.0, d);
            let from_inner = smoothstep(0.0, blur, d_inner);
            shadow_alpha = (from_outer * from_inner) * in.shadow_color.a;
        } else {
            // Outset: fill the border-box expanded by spread; compositing with (1-alpha_bg)
            // leaves the visible band outside the control (works with or without a CSS border).
            let d_shadow = sd_round_box(
                in.p - in.shadow_offset,
                b + vec2<f32>(spread_signed),
                in.radii + vec4<f32>(spread_signed)
            );
            shadow_alpha = smoothstep(blur, 0.0, max(0.0, d_shadow)) * in.shadow_color.a;
        }
    }

    // Blend: composite shadow over fill; `s_a` is gated to inside/outside depending on inset.
    let fill_alpha = final_color.a * alpha_bg;
    let s_a = shadow_alpha * select((1.0 - alpha_bg), alpha_bg, in.shadow_inset > 0.5);
    let out_a = s_a + fill_alpha * (1.0 - s_a);
    var out_rgb = vec3<f32>(0.0);
    if (out_a > 0.0) {
        out_rgb = (in.shadow_color.rgb * s_a + final_color.rgb * fill_alpha * (1.0 - s_a)) / out_a;
    }

    // Instance colors are **display (gamma-encoded) sRGB** from the style/theme layer.
    // Swapchain / offscreen targets use `*Srgb` formats: the GPU expects **linear** fragment
    // output and encodes on write. Without this step, display values are encoded twice
    // (e.g. `#333` → `#7B`, `#F5F5F5` → `#FBFBFB`).
    return vec4<f32>(linearize_srgb_rgb(out_rgb), out_a);
}
