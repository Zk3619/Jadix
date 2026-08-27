use crate::renderer::TexturedQuadPipeline;

pub struct GpuImageTexture {
    pub _texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
}

/// wgpu `write_texture` 要求每行字节数为 [`wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`]（256）的倍数。
pub(crate) fn padded_bytes_per_row(width: u32) -> u32 {
    let unpadded = width.saturating_mul(4);
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    unpadded.div_ceil(align) * align
}

pub fn upload_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &TexturedQuadPipeline,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> GpuImageTexture {
    upload_rgba8_with_format(
        device,
        queue,
        pipeline,
        rgba,
        width,
        height,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )
}

/// Coverage / mask text bake must stay linear — sRGB + bilinear rotate fog-blurs strokes.
/// Single mip level only (mip minify washed out supersample AA on sticky Bake).
pub fn upload_rgba8_linear(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &TexturedQuadPipeline,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> GpuImageTexture {
    upload_rgba8_with_format(
        device,
        queue,
        pipeline,
        rgba,
        width,
        height,
        wgpu::TextureFormat::Rgba8Unorm,
    )
}

/// Linear Unorm upload with a full mip chain (CPU box-filter). Optional; text Bake skips this.
#[allow(dead_code)]
pub fn upload_rgba8_linear_with_mips(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &TexturedQuadPipeline,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> GpuImageTexture {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    assert_eq!(
        rgba.len(),
        expected,
        "RGBA buffer size must be width*height*4"
    );
    let max_dim = width.max(height).max(1);
    let mip_level_count = (max_dim.ilog2() + 1).max(1);

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rui image mips"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut level_rgba: Vec<u8> = rgba.to_vec();
    let mut lw = width;
    let mut lh = height;
    for level in 0..mip_level_count {
        write_rgba8_mip(queue, &texture, level, &level_rgba, lw, lh);
        if level + 1 >= mip_level_count {
            break;
        }
        let (nw, nh, next) = box_filter_rgba_half(&level_rgba, lw, lh);
        level_rgba = next;
        lw = nw;
        lh = nh;
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = pipeline.create_texture_bind_group(device, &view);
    GpuImageTexture {
        _texture: texture,
        bind_group,
    }
}

fn write_rgba8_mip(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    mip_level: u32,
    rgba: &[u8],
    width: u32,
    height: u32,
) {
    let unpadded = width.saturating_mul(4);
    let padded = padded_bytes_per_row(width);
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(height),
    };
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let dest = wgpu::TexelCopyTextureInfo {
        texture,
        mip_level,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    };
    if padded == unpadded {
        queue.write_texture(dest, rgba, layout, extent);
    } else {
        let mut staged = vec![0u8; padded as usize * height as usize];
        for y in 0..height as usize {
            let src = y * unpadded as usize;
            let dst = y * padded as usize;
            staged[dst..dst + unpadded as usize]
                .copy_from_slice(&rgba[src..src + unpadded as usize]);
        }
        queue.write_texture(dest, &staged, layout, extent);
    }
}

fn box_filter_rgba_half(src: &[u8], sw: u32, sh: u32) -> (u32, u32, Vec<u8>) {
    let dw = (sw / 2).max(1);
    let dh = (sh / 2).max(1);
    let mut dst = vec![0u8; (dw as usize) * (dh as usize) * 4];
    for y in 0..dh {
        for x in 0..dw {
            let x0 = (x * 2).min(sw.saturating_sub(1));
            let y0 = (y * 2).min(sh.saturating_sub(1));
            let x1 = (x0 + 1).min(sw.saturating_sub(1));
            let y1 = (y0 + 1).min(sh.saturating_sub(1));
            for c in 0..4u32 {
                let sum = src[((y0 * sw + x0) * 4 + c) as usize] as u32
                    + src[((y0 * sw + x1) * 4 + c) as usize] as u32
                    + src[((y1 * sw + x0) * 4 + c) as usize] as u32
                    + src[((y1 * sw + x1) * 4 + c) as usize] as u32;
                dst[((y * dw + x) * 4 + c) as usize] = (sum / 4) as u8;
            }
        }
    }
    (dw, dh, dst)
}

fn upload_rgba8_with_format(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &TexturedQuadPipeline,
    rgba: &[u8],
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> GpuImageTexture {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    assert_eq!(
        rgba.len(),
        expected,
        "RGBA buffer size must be width*height*4"
    );

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rui image"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let unpadded = width.saturating_mul(4);
    let padded = padded_bytes_per_row(width);
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(height),
    };
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let dest = wgpu::TexelCopyTextureInfo {
        texture: &texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
    };
    if padded == unpadded {
        queue.write_texture(dest, rgba, layout, extent);
    } else {
        let mut staged = vec![0u8; padded as usize * height as usize];
        for y in 0..height as usize {
            let src = y * unpadded as usize;
            let dst = y * padded as usize;
            staged[dst..dst + unpadded as usize]
                .copy_from_slice(&rgba[src..src + unpadded as usize]);
        }
        queue.write_texture(dest, &staged, layout, extent);
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = pipeline.create_texture_bind_group(device, &view);
    GpuImageTexture {
        _texture: texture,
        bind_group,
    }
}

#[cfg(test)]
mod tests {
    use super::padded_bytes_per_row;

    #[test]
    fn padded_row_is_256_aligned() {
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(180) % 256, 0);
        assert!(padded_bytes_per_row(180) >= 180 * 4);
        assert_eq!(padded_bytes_per_row(256), 1024);
    }
}
