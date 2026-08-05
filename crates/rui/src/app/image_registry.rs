use crate::renderer::TexturedQuadPipeline;

pub struct GpuImageTexture {
    pub _texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
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

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = pipeline.create_texture_bind_group(device, &view);
    GpuImageTexture {
        _texture: texture,
        bind_group,
    }
}
