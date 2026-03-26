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
    @location(9) shadow_blur: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) rect_size: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) border_width: f32,
    @location(5) shadow_color: vec4<f32>,
    @location(6) shadow_blur: f32,
    @location(7) p: vec2<f32>, // physical point relative to rect center
};

struct Globals {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;
    
    let center = instance.rect_pos + instance.rect_size * 0.5;
    
    // Extend the generated vertex bounds to accommodate shadow blur
    let expand = instance.shadow_blur * 2.0 + instance.border_width;
    let expand_offset = (model.position - 0.5) * 2.0 * expand;
    
    let physical_pos = instance.rect_pos + model.position * instance.rect_size + expand_offset;
    
    // Convert to NDC
    let x = (physical_pos.x / globals.resolution.x) * 2.0 - 1.0;
    let y = 1.0 - (physical_pos.y / globals.resolution.y) * 2.0;

    out.clip_position = vec4<f32>(x, y, 0.0, 1.0);
    out.p = physical_pos - center;
    out.rect_size = instance.rect_size;
    out.color = instance.color;
    out.border_color = instance.border_color;
    out.radii = instance.radii;
    out.border_width = instance.border_width;
    out.shadow_color = instance.shadow_color;
    out.shadow_blur = instance.shadow_blur;
    
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
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
    
    // Compute shadow factor
    var shadow_alpha = 0.0;
    if (in.shadow_blur > 0.0) {
        shadow_alpha = smoothstep(in.shadow_blur, 0.0, max(0.0, d)) * in.shadow_color.a;
    }

    // Blend: Shadow behind, rect in front
    // We treat shadow contribution only outside the rect
    let s_a = shadow_alpha * (1.0 - alpha_bg);
    let fill_alpha = final_color.a * alpha_bg;
    
    let out_a = fill_alpha + s_a * (1.0 - fill_alpha);
    var out_rgb = vec3<f32>(0.0);
    if (out_a > 0.0) {
        out_rgb = (final_color.rgb * fill_alpha + in.shadow_color.rgb * s_a * (1.0 - fill_alpha)) / out_a;
    }
    
    return vec4<f32>(out_rgb, out_a);
}
