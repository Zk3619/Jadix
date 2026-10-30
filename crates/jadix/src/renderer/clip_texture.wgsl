@group(0) @binding(1)
var clip_poly_texture: texture_2d<u32>;

fn clip_poly_count() -> u32 {
    let size = textureDimensions(clip_poly_texture);
    return size.x * size.y / 21u;
}

fn clip_texel(index: u32, offset: u32) -> vec4<u32> {
    let width = textureDimensions(clip_poly_texture).x;
    let address = index * 21u + offset;
    return textureLoad(clip_poly_texture, vec2<i32>(i32(address % width), i32(address / width)), 0);
}

fn clip_texture_point(index: u32, point: u32, second: bool) -> vec2<f32> {
    let offset = select(1u, 9u, second) + point / 2u;
    let pair = bitcast<vec4<f32>>(clip_texel(index, offset));
    return select(pair.xy, pair.zw, (point % 2u) != 0u);
}
