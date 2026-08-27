struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
    @location(0) pos: vec2<f32>,
    @location(1) dim: u32,
    @location(2) uv: u32,
    @location(3) color: u32,
    @location(4) content_type_with_srgb: u32,
    @location(5) depth: f32,
    // Linear 2×2 [a, b, c, d]: x' = a*x + c*y, y' = b*x + d*y
    @location(6) transform: vec4<f32>,
    @location(7) transform_origin: vec2<f32>,
    // Atlas texel size (equals dim unless oversampled)
    @location(8) uv_size: u32,
}

struct VertexOutput {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) content_type: u32,
};

struct Params {
    screen_resolution: vec2<u32>,
    _pad: vec2<u32>,
};

@group(0) @binding(0)
var color_atlas_texture: texture_2d<f32>;

@group(0) @binding(1)
var mask_atlas_texture: texture_2d<f32>;

@group(0) @binding(2)
var atlas_sampler: sampler;

@group(1) @binding(0)
var<uniform> params: Params;

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    } else {
        return pow((c + 0.055) / 1.055, 2.4);
    }
}

@vertex
fn vs_main(in_vert: VertexInput) -> VertexOutput {
    let width = in_vert.dim & 0xffffu;
    let height = (in_vert.dim & 0xffff0000u) >> 16u;
    let uv_w = in_vert.uv_size & 0xffffu;
    let uv_h = (in_vert.uv_size & 0xffff0000u) >> 16u;
    let color = in_vert.color;
    var uv = vec2<u32>(in_vert.uv & 0xffffu, (in_vert.uv & 0xffff0000u) >> 16u);

    let corner_position = vec2<u32>(
        in_vert.vertex_idx & 1u,
        (in_vert.vertex_idx >> 1u) & 1u,
    );

    let screen_offset = vec2<f32>(vec2<u32>(width, height) * corner_position);
    let uv_offset = vec2<u32>(uv_w, uv_h) * corner_position;

    uv = uv + uv_offset;
    var pos_f = in_vert.pos + screen_offset;

    // Apply 2D linear transform about transform_origin (identity when transform = (1,0,0,1)).
    let origin = in_vert.transform_origin;
    let t = in_vert.transform;
    let centered = pos_f - origin;
    // column-major Mat2d [a,b,c,d]: [[a,c],[b,d]] * v
    pos_f = vec2<f32>(
        t.x * centered.x + t.z * centered.y,
        t.y * centered.x + t.w * centered.y,
    ) + origin;

    var vert_output: VertexOutput;

    vert_output.position = vec4<f32>(
        2.0 * pos_f / vec2<f32>(params.screen_resolution) - 1.0,
        in_vert.depth,
        1.0,
    );

    vert_output.position.y *= -1.0;

    let content_type = in_vert.content_type_with_srgb & 0xffffu;
    let srgb = (in_vert.content_type_with_srgb & 0xffff0000u) >> 16u;

    switch srgb {
        case 0u: {
            vert_output.color = vec4<f32>(
                f32((color & 0x00ff0000u) >> 16u) / 255.0,
                f32((color & 0x0000ff00u) >> 8u) / 255.0,
                f32((color & 0x000000ffu)) / 255.0,
                f32((color & 0xff000000u) >> 24u) / 255.0,
            );
        }
        case 1u: {
            vert_output.color = vec4<f32>(
                srgb_to_linear(f32((color & 0x00ff0000u) >> 16u) / 255.0),
                srgb_to_linear(f32((color & 0x0000ff00u) >> 8u) / 255.0),
                srgb_to_linear(f32((color & 0x000000ffu)) / 255.0),
                f32((color & 0xff000000u) >> 24u) / 255.0,
            );
        }
        default: {}
    }

    var dim: vec2<u32> = vec2(0u);
    switch content_type {
        case 0u: {
            dim = textureDimensions(color_atlas_texture);
            break;
        }
        case 1u: {
            dim = textureDimensions(mask_atlas_texture);
            break;
        }
        default: {}
    }

    vert_output.content_type = content_type;
    vert_output.uv = vec2<f32>(uv) / vec2<f32>(dim);

    return vert_output;
}

@fragment
fn fs_main(in_frag: VertexOutput) -> @location(0) vec4<f32> {
    switch in_frag.content_type {
        case 0u: {
            return textureSampleLevel(color_atlas_texture, atlas_sampler, in_frag.uv, 0.0);
        }
        case 1u: {
            return vec4<f32>(
                in_frag.color.rgb,
                in_frag.color.a
                    * textureSampleLevel(mask_atlas_texture, atlas_sampler, in_frag.uv, 0.0).x,
            );
        }
        default: {
            return vec4<f32>(0.0);
        }
    }
}
