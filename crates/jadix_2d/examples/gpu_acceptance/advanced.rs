use super::scene_tests::texture;
use super::*;

#[cfg(windows)]
fn process_memory() -> serde_json::Value {
    #[repr(C)]
    struct Memory {
        cb: u32,
        faults: u32,
        values: [usize; 9],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            memory: *mut Memory,
            size: u32,
        ) -> i32;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            creation: *mut u64,
            exit: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
    }
    let mut m = Memory {
        cb: std::mem::size_of::<Memory>() as u32,
        faults: 0,
        values: [0; 9],
    };
    let (mut creation, mut exit, mut kernel, mut user) = (0, 0, 0, 0);
    // All pointers reference initialized, correctly sized writable storage;
    // the pseudo-handle is valid for this process and must not be closed.
    let ok = unsafe {
        K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut m,
            std::mem::size_of::<Memory>() as u32,
        ) != 0
            && GetProcessTimes(
                GetCurrentProcess(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            ) != 0
    };
    if !ok {
        return json!({"available":false});
    }
    json!({"available":true,"peak_working_set_bytes":m.values[0],"working_set_bytes":m.values[1],"private_bytes":m.values[8],"peak_commit_bytes":m.values[7],"cpu_seconds":(kernel+user) as f64/10_000_000.})
}
#[cfg(not(windows))]
fn process_memory() -> serde_json::Value {
    json!({"available":false,"reason":"Windows process counters only"})
}

#[cfg(windows)]
fn gpu_process_memory() -> serde_json::Value {
    use std::os::windows::process::CommandExt;
    let script=format!("Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory -ErrorAction Stop | Where-Object {{$_.Name -like 'pid_{}_*'}} | Select-Object Name,DedicatedUsage,SharedUsage,TotalCommitted | ConvertTo-Json -Compress",std::process::id());
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x08000000)
        .output();
    match output {
        Ok(o) if o.status.success() => serde_json::from_slice(&o.stdout)
            .unwrap_or(json!({"available":false,"reason":"no GPU process counter instance"})),
        _ => json!({"available":false,"reason":"GPU CIM query failed"}),
    }
}
#[cfg(not(windows))]
fn gpu_process_memory() -> serde_json::Value {
    json!({"available":false,"reason":"Windows GPU counters only"})
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn large_instanced_scene_resource_pressure() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let start = process_memory();
    let tri = [
        PathTri {
            a: (0., 0.),
            b: (1., 0.),
            c: (1., 1.),
            color: [0.2, 0.6, 1., 1.],
        },
        PathTri {
            a: (0., 0.),
            b: (1., 1.),
            c: (0., 1.),
            color: [0.2, 0.6, 1., 1.],
        },
    ];
    let mut mesh = gpu::GpuMesh::try_new(
        &device,
        &tri,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        gpu::MeshColorSpace::Srgb,
    )
    .unwrap();
    let mut instances = (0..100000)
        .map(|i| gpu::MeshInstance {
            transform: [1., 0., 0., 1., (i % 512) as f32, (i / 512) as f32],
            opacity: 1.,
        })
        .collect::<Vec<_>>();
    let target = texture(&device, 512);
    let view = target.create_view(&Default::default());
    let mut samples = Vec::new();
    for frame in 0..60 {
        instances[frame].transform[4] += 0.25;
        assert!(mesh.prepare(&device, &queue, [512, 512], &instances));
        if frame > 0 {
            assert_eq!(mesh.last_upload_stats().buffer_allocations, 0);
        }
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("100k-instances"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            mesh.draw(&mut pass);
        }
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely())?;
        if frame % 12 == 0 || frame == 59 {
            samples
                .push(json!({"frame":frame,"process":process_memory(),"gpu":gpu_process_memory()}));
        }
    }
    let pixels = readback(&device, &queue, &target, 512)?;
    assert!(pixels.chunks_exact(4).filter(|p| p[3] > 0).count() > 99000);
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("resource-pressure.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"frames":60,"instances_per_frame":100000,"geometry_bytes":mesh.geometry_bytes(),"instance_payload_bytes":2800000,"start":start,"end":process_memory(),"samples":samples,"scope":"Observed Windows process peak RSS/commit/CPU and sampled GPU dedicated/shared/committed counters; not a complete driver allocation peak or performance benchmark"}),
            )?,
        )?;
    }
    Ok(())
}

pub(super) fn read_float(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: [u32; 2],
) -> Vec<f32> {
    let stride = (size[0] * 8).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: stride as u64 * size[1] as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(size[1]),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let view = buffer.slice(..).get_mapped_range().unwrap();
    let pixels = view
        .chunks(stride as usize)
        .flat_map(|row| {
            row[..size[0] as usize * 8]
                .chunks_exact(2)
                .map(|v| half::f16::from_bits(u16::from_le_bytes([v[0], v[1]])).to_f32())
        })
        .collect();
    drop(view);
    buffer.unmap();
    pixels
}
fn float_texture(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hdr-acceptance"),
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
    })
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn hdr_float_targets_images_and_nested_porter_duff() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let size = [32, 32];
    let target = float_texture(&device, size);
    let image = Image2D::from_color_space(
        2,
        1,
        &[1., 0., 0., 0.5, 0., 1., 0., 1.],
        ColorSpaceTag::DisplayP3,
    )
    .unwrap();
    let mut rows = Vec::new();
    for samples in [1, 4] {
        for compose in [
            CompositeMode::Copy,
            CompositeMode::SourceOver,
            CompositeMode::SourceIn,
            CompositeMode::Plus,
            CompositeMode::DestinationOut,
        ] {
            for pattern in [false, true] {
                let paint = |c: &mut dyn Canvas2D| {
                    c.set_fill_brush(
                        Color2D::new(ColorSpaceTag::LinearSrgb, [4., -0.25, 0.5, 1.])
                            .unwrap()
                            .brush()
                            .unwrap(),
                    );
                    c.fill_rect(0., 0., 32., 32.);
                    c.push_layer_with_options(LayerOptions {
                        compose,
                        filters: vec![
                            LayerFilter::Gaussian { sigma: [0.5; 2] },
                            LayerFilter::Offset { offset: [0.5, 0.] },
                        ],
                        ..Default::default()
                    });
                    c.push_layer(0.75, BlendMode::Normal);
                    if pattern {
                        c.set_fill_brush(
                            Brush2D::image(image.clone())
                                .with_transform([8., 0., 0., 16., 8., 8.])
                                .with_sampling(ImageSampling::Bilinear),
                        );
                        c.fill_rect(8., 8., 16., 16.);
                    } else {
                        c.draw_image(&CanvasImageSource::Image(&image), 8., 8., 16., 16.);
                    }
                    c.pop_layer();
                    c.pop_layer();
                };
                let mut cpu = RasterCanvas::new_hdr(32, 32);
                paint(&mut cpu);
                let expected = cpu.into_linear_rgba32f();
                let mut recorded = GpuCanvas::new_linear(32, 32, None);
                paint(&mut recorded);
                let ops = recorded.into_ops();
                let mut renderer = PathGpu::try_new_with_format(
                    &device,
                    samples,
                    wgpu::TextureFormat::Rgba16Float,
                )
                .unwrap();
                renderer.set_aa_quality(gpu::AaQuality::Reference);
                assert!(renderer.render_premultiplied(
                    &device,
                    &queue,
                    &target.create_view(&Default::default()),
                    size,
                    &ops
                ));
                let actual = read_float(&device, &queue, &target, size);
                let mae = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| (a - b).abs() as f64)
                    .sum::<f64>()
                    / actual.len() as f64;
                assert!(
                    mae < 0.025,
                    "{backend:?}/{samples}/{compose:?}/{pattern}: {mae}"
                );
                if compose == CompositeMode::SourceOver {
                    assert!(actual[0] > 3.9 && actual[1] < -0.24);
                }
                rows.push(json!({"samples":samples,"compose":format!("{compose:?}"),"pattern":pattern,"linear_float_mae":mae}));
            }
        }
    }
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("hdr-matrix.json"),
            serde_json::to_vec_pretty(&json!({"backend":format!("{backend:?}"),"cases":rows}))?,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn atlas_partial_updates_and_snapshot_isolation() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let mut cache = AtlasCache::new(AtlasLimits {
        page_size: 32,
        max_pages: 1,
        max_entries: 16,
    })?;
    let target = texture(&device, 32);
    let view = target.create_view(&Default::default());
    let mut renderer = PathGpu::try_new(&device, 4).unwrap();
    let red = Image2D::from_rgba8(2, 2, &[255, 0, 0, 255].repeat(4)).unwrap();
    cache.insert(0, &red)?;
    let mut bytes = Vec::new();
    let mut frozen = None;
    for frame in 0..8 {
        if frame > 0 {
            cache.insert(
                frame,
                &Image2D::from_rgba8(2, 2, &[0, 0, 255, 255].repeat(4)).unwrap(),
            )?;
        }
        let snapshot = cache.get(&0).unwrap();
        if frame == 0 {
            frozen = Some(snapshot.clone());
        }
        let mut c = GpuCanvas::new_linear(32, 32, None);
        snapshot.draw(&mut c, [0., 0., 32., 32.], ImageSampling::Bilinear);
        let ops = c.into_ops();
        assert!(renderer.render(&device, &queue, &view, [32, 32], &ops));
        let pixels = readback(&device, &queue, &target, 32)?;
        assert_eq!(&pixels[(16 * 32 + 16) * 4..][..4], &[255, 0, 0, 255]);
        let stats = renderer.last_stats();
        if frame >= 2 {
            assert_eq!(stats.image_region_uploads, 1);
            assert_eq!(stats.image_uploaded_bytes, 64);
            assert_eq!(stats.texture_allocations, 0);
        }
        bytes.push(stats.image_uploaded_bytes);
    }
    cache.clear();
    let snapshot = frozen.unwrap();
    let mut cpu = RasterCanvas::new_linear(4, 4);
    snapshot.draw(&mut cpu, [0., 0., 4., 4.], ImageSampling::Bilinear);
    assert_eq!(cpu.get_image_data(1, 1, 1, 1).unwrap(), [255, 0, 0, 255]);
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("atlas-incremental.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"page_bytes":4096,"per_frame_image_bytes":bytes,"old_snapshot_preserved":true}),
            )?,
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires a native headless GPU adapter"]
fn persistent_mesh_hardware_instances_and_offscreen_regions() -> Result<(), Box<dyn Error>> {
    let TestGpu {
        backend,
        device,
        queue,
        ..
    } = test_gpu();
    let triangles = [
        PathTri {
            a: (0., 0.),
            b: (6., 0.),
            c: (6., 6.),
            color: [1., 0.25, 0., 1.],
        },
        PathTri {
            a: (0., 0.),
            b: (6., 6.),
            c: (0., 6.),
            color: [1., 0.25, 0., 1.],
        },
    ];
    let mut mesh = gpu::GpuMesh::try_new(
        &device,
        &triangles,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        gpu::MeshColorSpace::Srgb,
    )
    .unwrap();
    let instances = (0..16)
        .map(|i| gpu::MeshInstance {
            transform: [1., 0., 0., 1., (i % 4 * 8) as f32, (i / 4 * 8) as f32],
            opacity: 0.75,
        })
        .collect::<Vec<_>>();
    let target = texture(&device, 32);
    let view = target.create_view(&Default::default());
    assert!(mesh.prepare(&device, &queue, [32, 32], &instances));
    assert!(mesh.prepare(&device, &queue, [32, 32], &instances));
    assert_eq!(mesh.last_upload_stats().instance_bytes, 0);
    assert_eq!(mesh.geometry_bytes(), 144);
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        mesh.draw(&mut pass);
    }
    queue.submit([encoder.finish()]);
    let actual = readback(&device, &queue, &target, 32)?;
    assert!((actual[(2 * 32 + 2) * 4 + 3] as i32 - 191).abs() <= 1);
    assert_eq!(actual[(7 * 32 + 7) * 4 + 3], 0);
    let scene = Scene2D::record([128, 128], |c| {
        c.set_fill("red");
        c.fill_rect(-6., 3., 6., 10.);
    })?;
    let region = SceneRegion {
        origin: [0, 0],
        size: [16, 16],
        halo: 8,
        filters: vec![LayerFilter::Gaussian { sigma: [2.; 2] }],
    };
    let prepared = region.prepare_gpu(&scene)?;
    assert_eq!(prepared.size(), [32, 32]);
    let float = float_texture(&device, prepared.size());
    let mut renderer =
        PathGpu::try_new_with_format(&device, 4, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Reference);
    assert!(renderer.render_premultiplied(
        &device,
        &queue,
        &float.create_view(&Default::default()),
        prepared.size(),
        prepared.ops()
    ));
    let actual = read_float(&device, &queue, &float, prepared.size());
    let cpu = region.rasterize(&scene)?;
    let expected = cpu.image.linear_rgba32f().unwrap();
    let mae = actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .flat_map(|(a, b)| {
            (0..4).map(move |i| (a[i] - if i < 3 { b[i] * b[3] } else { b[3] }).abs() as f64)
        })
        .sum::<f64>()
        / actual.len() as f64;
    assert!(mae < 0.005, "{backend:?}: {mae}");
    assert!(actual[((8 + 8) * 32 + 8) * 4 + 3] > 0.3);
    if let Some(dir) = std::env::var_os("JADIX_2D_EVIDENCE") {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(
            Path::new(&dir).join("mesh-region.json"),
            serde_json::to_vec_pretty(
                &json!({"backend":format!("{backend:?}"),"instances":16,"instanced_draw_calls":1,"geometry_bytes":144,"warm_instance_upload_bytes":0,"region_size":[32,32],"region_float_mae":mae}),
            )?,
        )?;
    }
    Ok(())
}
