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
        !super::texture_allows_rgba8_overwrite(&texture, 8, 2, wgpu::TextureFormat::Rgba8UnormSrgb),
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
        !super::texture_allows_rgba8_overwrite(&no_copy, 4, 2, wgpu::TextureFormat::Rgba8UnormSrgb),
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
