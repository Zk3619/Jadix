use crate::renderer::TexturedQuadPipeline;

pub struct GpuImageTexture {
    pub _texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
}

/// wgpu `write_texture` requires bytes-per-row to be a multiple of [`wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`] (256).
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
        false,
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
        false,
    )
}

/// SDF/MSDF text atlas — linear Unorm + bilinear sampler (FS reconstructs the edge).
#[allow(dead_code)]
pub fn upload_rgba8_msdf(
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
        false,
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
        label: Some("jadix image mips"),
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
    sdf_nearest: bool,
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
        label: Some("jadix image"),
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

    write_rgba8_mip(queue, &texture, 0, rgba, width, height);

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = if sdf_nearest {
        pipeline.create_texture_bind_group_sdf(device, &view)
    } else {
        pipeline.create_texture_bind_group(device, &view)
    };
    GpuImageTexture {
        _texture: texture,
        bind_group,
    }
}

/// Same size / format / `COPY_DST` / single mip → in-place `write_texture` is safe.
pub(crate) fn texture_allows_rgba8_overwrite(
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> bool {
    texture.width() == width
        && texture.height() == height
        && texture.format() == format
        && texture.mip_level_count() == 1
        && texture.usage().contains(wgpu::TextureUsages::COPY_DST)
}

/// Write packed RGBA8 into an existing texture. Returns `false` if the buffer length is wrong.
pub(crate) fn write_rgba8_texels(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> bool {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() != expected {
        return false;
    }
    write_rgba8_mip(queue, texture, 0, rgba, width, height);
    true
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

    #[test]
    #[cfg_attr(
        not(any(
            all(windows, feature = "backend-dx12"),
            all(target_vendor = "apple", feature = "backend-metal"),
            feature = "backend-gles",
            feature = "backend-vulkan",
        )),
        ignore = "requires a wgpu backend for this target"
    )]
    fn overwrite_requires_matching_slot() {
        let Some(gpu) = crate::renderer::offscreen::OffscreenGpu::new() else {
            eprintln!("skip overwrite_requires_matching_slot: no wgpu adapter");
            return;
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-overwrite-slot"),
            size: wgpu::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        assert!(super::texture_allows_rgba8_overwrite(
            &texture,
            4,
            2,
            wgpu::TextureFormat::Rgba8UnormSrgb
        ));
        assert!(
            !super::texture_allows_rgba8_overwrite(
                &texture,
                8,
                2,
                wgpu::TextureFormat::Rgba8UnormSrgb
            ),
            "size change must rebuild"
        );
        assert!(
            !super::texture_allows_rgba8_overwrite(&texture, 4, 2, wgpu::TextureFormat::Rgba8Unorm),
            "format change must rebuild"
        );
        let no_copy = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-overwrite-no-copy"),
            size: wgpu::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        assert!(
            !super::texture_allows_rgba8_overwrite(
                &no_copy,
                4,
                2,
                wgpu::TextureFormat::Rgba8UnormSrgb
            ),
            "RT without COPY_DST is not an image stream slot"
        );
    }

    #[test]
    #[cfg_attr(
        not(any(
            all(windows, feature = "backend-dx12"),
            all(target_vendor = "apple", feature = "backend-metal"),
            feature = "backend-gles",
            feature = "backend-vulkan",
        )),
        ignore = "requires a wgpu backend for this target"
    )]
    fn same_size_overwrite_does_not_recreate() {
        use crate::renderer::resources::TexturePool;
        use crate::renderer::textured_pipeline::TexturedQuadPipeline;
        use std::sync::Mutex;
        static GPU_LOCK: Mutex<()> = Mutex::new(());
        let _guard = GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let Some(gpu) = crate::renderer::offscreen::OffscreenGpu::new() else {
            eprintln!("skip same_size_overwrite_does_not_recreate: no wgpu adapter");
            return;
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: 8,
            height: 8,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        let pipeline = TexturedQuadPipeline::new(&gpu.device, &config);
        let mut pool = TexturePool::new();
        let first_px = vec![
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        let first = super::upload_rgba8(&gpu.device, &gpu.queue, &pipeline, &first_px, 2, 2);
        pool.insert_with_size("jadix-test-stream".into(), first, 2, 2);
        assert_eq!(pool.create_count(), 1);

        for i in 0..30u8 {
            let next_px = vec![i; 16];
            assert!(
                pool.try_overwrite_rgba8(
                    "jadix-test-stream",
                    &gpu.queue,
                    &next_px,
                    2,
                    2,
                    wgpu::TextureFormat::Rgba8UnormSrgb
                ),
                "same-size write {i} must reuse the slot"
            );
        }
        assert_eq!(
            pool.create_count(),
            1,
            "same size must not allocate a new wgpu::Texture"
        );

        let resized = vec![3u8; 64];
        assert!(
            !pool.try_overwrite_rgba8(
                "jadix-test-stream",
                &gpu.queue,
                &resized,
                4,
                4,
                wgpu::TextureFormat::Rgba8UnormSrgb
            ),
            "size change must fail overwrite"
        );
        let second = super::upload_rgba8(&gpu.device, &gpu.queue, &pipeline, &resized, 4, 4);
        pool.insert_with_size("jadix-test-stream".into(), second, 4, 4);
        assert_eq!(pool.create_count(), 2);
    }
}
