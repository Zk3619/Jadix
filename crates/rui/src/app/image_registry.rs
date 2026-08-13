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
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let unpadded = width.saturating_mul(4);
    let padded = padded_bytes_per_row(width);
    let layout = wgpu::ImageDataLayout {
        offset: 0,
        bytes_per_row: Some(padded),
        rows_per_image: Some(height),
    };
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let dest = wgpu::ImageCopyTexture {
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
