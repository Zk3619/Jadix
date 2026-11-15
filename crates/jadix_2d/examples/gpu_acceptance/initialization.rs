use super::*;
#[test]
#[ignore = "requires native GPU"]
fn dedicated_ewa_matches_mixed_sampling_shader_with_hdr_alpha() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = float_target(&device);
    let data = (0..31 * 17)
        .flat_map(|i| {
            [
                i as f32 / 200.,
                -0.125,
                (i % 13) as f32 / 6.,
                if i % 5 == 0 { 0. } else { 0.5 },
            ]
        })
        .collect::<Vec<_>>();
    let image = Image2D::from_linear_rgba32f(31, 17, &data)
        .unwrap()
        .with_mipmaps(Default::default())?;
    let mut max = 0f32;
    for quality in [
        gpu::AaQuality::Area,
        gpu::AaQuality::Balanced,
        gpu::AaQuality::Reference,
    ] {
        let make = |sampling| {
            let mut c = GpuCanvas::new_linear(32, 32, None);
            c.set_fill_brush(
                Brush2D::image(image.clone())
                    .with_transform([0.13, 0.07, -0.11, 0.4, 3., 4.])
                    .with_extend(ExtendMode::Repeat, ExtendMode::Reflect)
                    .with_sampling(sampling),
            );
            c.fill_rect(0., 0., 16., 32.);
            c.set_image_sampling(sampling);
            c.draw_image(&CanvasImageSource::Image(&image), 16., 0., 16., 32.);
            c.into_ops()
        };
        let ewa = make(ImageSampling::Ewa);
        let high = make(ImageSampling::Bicubic);
        let mut isolated =
            PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
        isolated.set_aa_quality(quality);
        assert!(isolated.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &ewa
        ));
        let a = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        let mut mixed =
            PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
        mixed.set_aa_quality(quality);
        assert!(mixed.warm_up(&device, &high));
        assert!(mixed.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &ewa
        ));
        let b = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        for (a, b) in a.iter().zip(&b) {
            max = max.max((a - b).abs());
        }
        // Stronger tier must continue to draw an earlier non-EWA quality correctly.
        assert!(mixed.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &high
        ));
        let expected = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        assert!(isolated.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &high
        ));
        let actual = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        for (a, b) in expected.iter().zip(&actual) {
            max = max.max((a - b).abs());
        }
    }
    assert!(max < 0.002, "specialized/mixed sampling mismatch {max}");
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("ewa-specialization.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","max_error":max,"qualities":3,"materials":["image","pattern"],"hdr_signed_alpha":true,"upgrade_order":["ewa","bicubic"]}),
        )?,
    )?;
    Ok(())
}
#[test]
#[ignore = "requires a real selected GPU backend"]
fn cold_start_profile() -> Result<(), Box<dyn Error>> {
    let begin = Instant::now();
    let TestGpu { device, queue, .. } = test_gpu();
    let device_ms = begin.elapsed().as_secs_f64() * 1000.;
    let mut rows = Vec::new();
    for iteration in 0..2 {
        let start = Instant::now();
        let mut renderer = PathGpu::try_new(&device, 1).unwrap();
        renderer.set_aa_quality(gpu::AaQuality::Area);
        let init_ms = start.elapsed().as_secs_f64() * 1000.;
        let target = scene_tests::texture(&device, 32);
        let mut canvas = GpuCanvas::new_linear(32, 32, None);
        canvas.set_fill("#ff0000");
        canvas.fill_rect(0., 0., 32., 32.);
        let start = Instant::now();
        assert!(renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &canvas.into_ops()
        ));
        let pixels = readback(&device, &queue, &target, 32)?;
        let solid_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
        let image = Image2D::from_rgba8(8, 8, &[255; 8 * 8 * 4])
            .unwrap()
            .with_mipmaps(Default::default())?;
        let mut canvas = GpuCanvas::new_linear(32, 32, None);
        canvas.set_fill_brush(Brush2D::image(image.clone()).with_sampling(ImageSampling::Bilinear));
        canvas.fill_rect(0., 0., 32., 32.);
        let start = Instant::now();
        assert!(renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &canvas.into_ops()
        ));
        let pixels = readback(&device, &queue, &target, 32)?;
        let basic_pattern_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(&pixels[..4], &[255; 4]);
        let mut canvas = GpuCanvas::new_linear(32, 32, None);
        canvas.set_fill_brush(Brush2D::image(image).with_sampling(ImageSampling::Ewa));
        canvas.fill_rect(0., 0., 32., 32.);
        let start = Instant::now();
        assert!(renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &canvas.into_ops()
        ));
        let pixels = readback(&device, &queue, &target, 32)?;
        let pattern_ms = start.elapsed().as_secs_f64() * 1000.;
        assert_eq!(&pixels[..4], &[255; 4]);
        rows.push(json!({"iteration":iteration,"constructor_ms":init_ms,"first_solid_ms":solid_ms,"first_bilinear_pattern_ms":basic_pattern_ms,"first_ewa_pattern_ms":pattern_ms}));
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("cold-start.json"),
        serde_json::to_vec_pretty(
            &json!({"device_ms":device_ms,"rows":rows,"scope":"New renderer per iteration, same device; driver/system caches not cleared; phases include validation and GPU wait, not a compiler-only profile."}),
        )?,
    )?;
    Ok(())
}

pub(super) fn float_target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
#[test]
#[ignore = "requires selected native GPU"]
fn cpu_execution_hdr_budget_and_scene_estimate() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let target = float_target(&device);
    let hdr = Image2D::from_linear_rgba32f(1, 1, &[2., -0.1, 0.5, 0.75]).unwrap();
    let scene = Scene2D::record([32, 32], |c| {
        c.push_layer_with_options(LayerOptions {
            opacity: 0.5,
            ..Default::default()
        });
        c.draw_image(&CanvasImageSource::Image(&hdr), 4., 4., 16., 16.);
        c.pop_layer();
    })?;
    let estimate = scene.bump_estimate([32, 32], SceneOptions::default())?;
    assert!(
        estimate.triangles >= 2
            && estimate.layers == 1
            && estimate.payload_bytes().unwrap() > estimate.geometry_bytes
    );
    let transformed = scene.bump_estimate(
        [64, 64],
        SceneOptions {
            transform: [2., 0., 0., 2., 0., 0.],
            ..Default::default()
        },
    )?;
    assert!(transformed.layer_surface_bytes > estimate.layer_surface_bytes);
    let mut renderer = gpu::SceneRenderer2D::new(
        &device,
        1,
        wgpu::TextureFormat::Rgba16Float,
        gpu::RendererOptions {
            use_cpu: true,
            antialiasing: gpu::AaQuality::Area,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(renderer.uses_cpu() && renderer.gpu_renderer_mut().is_none());
    for premul in [false, true] {
        renderer.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &scene,
            SceneOptions::default(),
            premul,
        )?;
        let pixels = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
        let pixel = &pixels[(12 * 32 + 12) * 4..][..4];
        let expected = if premul {
            [0.75, -0.0375, 0.1875, 0.375]
        } else {
            [2., -0.1, 0.5, 0.375]
        };
        for (a, b) in pixel.iter().zip(expected) {
            assert!((*a - b).abs() < 0.003, "{pixel:?}");
        }
        assert_eq!(renderer.last_stats().cpu_rasterized_pixels, 1024);
        assert_eq!(renderer.last_stats().draw_calls, 1);
    }
    let mut bounded = gpu::SceneRenderer2D::new(
        &device,
        1,
        wgpu::TextureFormat::Rgba16Float,
        gpu::RendererOptions {
            use_cpu: true,
            max_surface_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        bounded.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &scene,
            SceneOptions::default(),
            true
        ),
        Err(SceneError::ImageBudget)
    );
    assert_eq!(
        scene.bump_estimate(
            [32, 32],
            SceneOptions {
                transform: [0.; 6],
                ..Default::default()
            }
        ),
        Err(SceneError::InvalidTransform)
    );
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("scene-estimate.json"),
        serde_json::to_vec_pretty(
            &json!({"operations":estimate.operations,"triangles":estimate.triangles,"layers":estimate.layers,"payload_bytes":estimate.payload_bytes(),"area_triangle_references":estimate.area_triangle_references,"area_pixel_triangle_work_upper_bound":estimate.area_pixel_triangle_work_upper_bound,"layer_surface_bytes":estimate.layer_surface_bytes,"transformed_layer_surface_bytes":transformed.layer_surface_bytes}),
        )?,
    )?;
    Ok(())
}
#[test]
#[ignore = "requires selected native GPU"]
fn lazy_area_parallel_warmup_and_persistent_pipeline_cache() -> Result<(), Box<dyn Error>> {
    let cache_requested = std::env::var("JADIX_TEST_BACKEND").as_deref() == Ok("vulkan");
    let TestGpu { device, queue, .. } = test_gpu_with_features(if cache_requested {
        wgpu::Features::PIPELINE_CACHE
    } else {
        wgpu::Features::empty()
    });
    // Fresh cache has no externally supplied bytes and belongs to this exact device.
    let cache = if cache_requested {
        Some(unsafe {
            device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: None,
                data: None,
                fallback: true,
            })
        })
    } else {
        None
    };
    let target = scene_tests::texture(&device, 32);
    let mut solid = GpuCanvas::new_linear(32, 32, None);
    solid.set_fill("red");
    solid.fill_rect(0., 0., 32., 32.);
    let solid = solid.into_ops();
    let mut renderer = PathGpu::try_new_with_options(
        &device,
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        gpu::RendererOptions {
            antialiasing: gpu::AaQuality::Area,
            pipeline_cache: cache.clone(),
            num_init_threads: std::num::NonZeroUsize::new(3),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &solid
    ));
    #[cfg(feature = "test-hooks")]
    {
        assert_eq!(renderer.area_pipeline_state(), [true, false, false, false]);
        assert_eq!(renderer.pipeline_state(), [false; 4]);
    }
    let image = Image2D::from_rgba8(2, 2, &[255; 16]).unwrap();
    let mut c = GpuCanvas::new_linear(32, 32, None);
    c.set_fill_brush(Brush2D::image(image.clone()));
    c.fill_rect(0., 0., 16., 32.);
    c.draw_image(&CanvasImageSource::Image(&image), 16., 0., 16., 32.);
    let ops = c.into_ops();
    assert!(renderer.warm_up(&device, &ops));
    #[cfg(feature = "test-hooks")]
    assert_eq!(renderer.area_pipeline_state(), [true, false, true, true]);
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &ops
    ));
    let a = readback(&device, &queue, &target, 32)?;
    let mut serial = PathGpu::try_new_with_options(
        &device,
        1,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        gpu::RendererOptions {
            antialiasing: gpu::AaQuality::Area,
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: cache.clone(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(serial.warm_up(&device, &ops));
    assert!(serial.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &ops
    ));
    assert_eq!(a, readback(&device, &queue, &target, 32)?);
    #[cfg(feature = "test-hooks")]
    assert_eq!(renderer.sampling_pipeline_state(), [false; 4]);
    let mut ewa = GpuCanvas::new_linear(32, 32, None);
    ewa.set_fill_brush(Brush2D::image(image.clone()).with_sampling(ImageSampling::Ewa));
    ewa.fill_rect(0., 0., 16., 32.);
    ewa.set_image_sampling(ImageSampling::Ewa);
    ewa.draw_image(&CanvasImageSource::Image(&image), 16., 0., 16., 32.);
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &ewa.into_ops()
    ));
    #[cfg(feature = "test-hooks")]
    assert_eq!(
        renderer.sampling_pipeline_state(),
        [false, false, true, true]
    );
    assert_eq!(a, readback(&device, &queue, &target, 32)?);
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &ops
    ));
    #[cfg(feature = "test-hooks")]
    assert_eq!(
        renderer.sampling_pipeline_state(),
        [false, false, true, true]
    );
    let bytes = cache.and_then(|c| c.get_data());
    if cache_requested {
        assert!(bytes.as_ref().is_some_and(|v| !v.is_empty()));
    }
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    if let Some(data) = &bytes {
        std::fs::write(out.join("pipeline-cache.bin"), data)?;
        // Data was produced by this same device in this test, not an untrusted cache file.
        let restored = unsafe {
            device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                label: None,
                data: Some(data),
                fallback: false,
            })
        };
        let mut restored = PathGpu::try_new_with_options(
            &device,
            1,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            gpu::RendererOptions {
                antialiasing: gpu::AaQuality::Area,
                pipeline_cache: Some(restored),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(restored.render(
            &device,
            &queue,
            &target.create_view(&Default::default()),
            [32, 32],
            &ops
        ));
        assert_eq!(a, readback(&device, &queue, &target, 32)?);
    }
    std::fs::write(
        out.join("pipeline-controls.json"),
        serde_json::to_vec_pretty(
            &json!({"native_worker_limit":3,"serial_pixels_equal":true,"cache_requested":cache_requested,"cache_roundtrip_pixels_equal":cache_requested,"persisted_cache_bytes":bytes.as_ref().map(Vec::len)}),
        )?,
    )?;
    Ok(())
}
#[cfg(feature = "compute")]
#[test]
#[ignore = "requires selected native compute backend"]
fn compute_direct_indirect_and_storage_readback() -> Result<(), Box<dyn Error>> {
    use wgpu::util::DeviceExt;
    let TestGpu {
        device,
        queue,
        adapter,
        ..
    } = test_gpu();
    let limits = device.limits();
    let supported = limits.max_compute_workgroup_size_x >= 64
        && limits.max_compute_invocations_per_workgroup >= 64
        && limits.max_storage_buffers_per_shader_stage >= 1
        && adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS);
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    if !supported {
        std::fs::write(
            out.join("compute-dispatch.json"),
            "{\"status\":\"unsupported-adapter\"}",
        )?;
        return Ok(());
    }
    assert!(wgpu::COMPUTE_FEATURE_ENABLED);
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:None,source:wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<storage,read_write> result:array<u32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){if(id.x<257u){result[id.x]=id.x*id.x+7u;}}".into())});
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 257 * 4,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let read = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 257 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let args = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[5u32, 1, 1]),
        usage: wgpu::BufferUsages::INDIRECT,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    for indirect in [false, true] {
        queue.write_buffer(&buffer, 0, &[0; 257 * 4]);
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            if indirect {
                pass.dispatch_workgroups_indirect(&args, 0)
            } else {
                pass.dispatch_workgroups(5, 1, 1)
            }
        }
        encoder.copy_buffer_to_buffer(&buffer, 0, &read, 0, 257 * 4);
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        {
            let data = read.slice(..).get_mapped_range()?;
            for (i, bytes) in data.chunks_exact(4).enumerate() {
                assert_eq!(
                    u32::from_le_bytes(bytes.try_into().unwrap()),
                    i as u32 * i as u32 + 7
                );
            }
        }
        read.unmap();
    }
    compute_effects::verify(&device, &queue, &out)?;
    std::fs::write(
        out.join("compute-dispatch.json"),
        "{\"status\":\"pass\",\"elements_per_dispatch\":257,\"dispatches\":2}",
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU"]
fn one_pixel_and_strip_targets_render_all_aa_and_sampled_layers() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let image = Image2D::from_linear_rgba32f(1, 1, &[2., -0.125, 1.2, 0.5]).unwrap();
    let mut cases = 0;
    let mut max = 0f32;
    for size in [[1, 1], [1, 9], [9, 1]] {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        for quality in [
            gpu::AaQuality::Area,
            gpu::AaQuality::Aliased,
            gpu::AaQuality::Balanced,
            gpu::AaQuality::High,
            gpu::AaQuality::Reference,
        ] {
            let mut renderer =
                PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
            renderer.set_aa_quality(quality);
            for layer in [false, true] {
                for premultiplied in [false, true] {
                    let mut c = GpuCanvas::new_linear(size[0], size[1], None);
                    if layer {
                        c.push_layer(0.5, BlendMode::Normal);
                    }
                    c.draw_image(
                        &CanvasImageSource::Image(&image),
                        0.,
                        0.,
                        size[0] as f32,
                        size[1] as f32,
                    );
                    if layer {
                        c.pop_layer();
                    }
                    let ops = c.into_ops();
                    let ok = if premultiplied {
                        renderer.render_premultiplied(
                            &device,
                            &queue,
                            &target.create_view(&Default::default()),
                            size,
                            &ops,
                        )
                    } else {
                        renderer.render(
                            &device,
                            &queue,
                            &target.create_view(&Default::default()),
                            size,
                            &ops,
                        )
                    };
                    assert!(ok, "tiny target {size:?} {quality:?} layer{layer}");
                    let actual = advanced_tests::read_float(&device, &queue, &target, size);
                    let alpha = if layer { 0.25 } else { 0.5 };
                    let factor = if premultiplied { alpha } else { 1. };
                    for p in actual.chunks_exact(4) {
                        for (a, b) in
                            p.iter()
                                .zip([2. * factor, -0.125 * factor, 1.2 * factor, alpha])
                        {
                            max = max.max((a - b).abs());
                        }
                    }
                    cases += 1;
                }
            }
        }
        let scene = Scene2D::record(size, |c| {
            c.set_fill("#ff0000");
            c.fill_rect(0., 0., size[0] as f32, size[1] as f32);
        })?;
        for grid in [[1, 1], [1, 2], [2, 1], [2, 4]] {
            let sampled = scene.sampled_grid(size, Default::default(), grid, 1024 * 1024)?;
            let mut prepared = sampled.prepare_gpu(&device, wgpu::TextureFormat::Rgba16Float)?;
            assert!(prepared.render(
                &device,
                &queue,
                &target.create_view(&Default::default()),
                false
            ));
            let values = advanced_tests::read_float(&device, &queue, &target, size);
            assert!(values.chunks_exact(4).all(|p| p == [1., 0., 0., 1.]));
            cases += 1;
        }
    }
    assert!(max < 0.004, "tiny target HDR max {max}");
    let out = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("tiny-targets.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","cases":cases,"max_error":max,"sizes":[[1,1],[1,9],[9,1]],"aa_modes":5}),
        )?,
    )?;
    Ok(())
}
