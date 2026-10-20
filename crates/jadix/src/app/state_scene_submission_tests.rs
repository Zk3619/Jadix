//! Same operation history, retained uploads/batching versus full uploads/single draws.
use super::*;
use crate::renderer::buffer_upload::RetainedBuffer;

fn backend() -> wgpu::Backends {
    if std::env::var("JADIX_UPLOAD_BACKEND").as_deref() == Ok("dx11") {
        wgpu::Backends::DX11
    } else if std::env::var("JADIX_UPLOAD_BACKEND").as_deref() == Ok("gl") {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::DX12
    }
}

#[test]
#[ignore = "requires native GPU readback; run explicitly"]
fn scene_submission_matches_full_upload_and_single_draws() {
    let gpu = OffscreenGpu::with_backends(backend()).expect("requested GPU");
    eprintln!("scene pixel oracle adapter={:?}", gpu.adapter_info);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 96,
        height: 64,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: Default::default(),
        view_formats: vec![],
    };
    let pipeline = TexturedQuadPipeline::new(&gpu.device, &config);
    let mut textures = HashMap::new();
    for (key, bytes) in [
        ("a", [255, 0, 0, 180, 0, 255, 0, 255]),
        ("b", [0, 0, 255, 160, 255, 255, 0, 255]),
    ] {
        textures.insert(
            key.into(),
            super::super::super::image_registry::upload_rgba8(
                &gpu.device,
                &gpu.queue,
                &pipeline,
                &bytes,
                2,
                1,
            ),
        );
    }
    let template =
        crate::renderer::TexturedDraw::clipped([8.0, 8.0, 48.0, 40.0], None, "a", 0.7).unwrap();
    let mut source = vec![template.clone(); 6];
    source[1].rect[0] = 20.0;
    source[2].rect[0] = 150.0; // invisible gap must split a same-texture run
    source[3].rect[0] = 32.0;
    source[4].texture_key = "missing".into();
    source[5].texture_key = "b".into();
    let initial = source.clone();
    let mut stream =
        RetainedBuffer::new(&gpu.device, "pixel scene", wgpu::BufferUsages::VERTEX, &[]);
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene oracle"),
        size: wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let cull = Some(CullRects::one([0.0, 0.0, 96.0, 64.0]));
    let mut first_image = None;
    for frame in 0..10 {
        if frame == 1 {
            // An offscreen submission overwrites the same globals buffer, even
            // though the last queue upload already had the window dimensions.
            let mut capture = gpu.device.create_command_encoder(&Default::default());
            pipeline.encode_resolution(&gpu.device, &mut capture, 48, 32);
            gpu.queue.submit(Some(capture.finish()));
        }
        pipeline.update_resolution(&gpu.queue, 96, 64);
        pipeline.update_resolution(&gpu.queue, 96, 64);
        match frame {
            2 => {
                source[1].uv_offset = [0.5, 0.0];
                source[1].uv_scale = [0.5, 1.0];
            }
            3 => source.truncate(2),
            4 => source = initial.clone(),
            5 => source.clear(),
            6 => {
                source = initial
                    .iter()
                    .cycle()
                    .take(initial.len() * 3)
                    .cloned()
                    .collect()
            }
            7 => {
                source[0].opacity = 0.3;
                source[1].texture_key = "b".into();
            }
            8 => {
                source[1].rounded_clips = [[16.0, 12.0, 32.0, 24.0], [8.0; 4], [0.0; 4], [0.0; 4]];
            }
            _ => {}
        }
        let draws = scale_textured_draws(&source, 1.0);
        let instances: Vec<_> = draws.iter().map(|d| d.to_instance()).collect();
        super::super::super::frame_stats::begin_scene_uploads();
        stream.upload(&gpu.device, &gpu.queue, bytemuck::cast_slice(&instances));
        let stats = crate::last_scene_uploads();
        if crate::renderer::buffer_upload::retained_uploads_enabled()
            && (frame == 1 || frame == 3 || frame == 5 || frame == 9)
        {
            assert_eq!(stats.write_calls, 0, "idle/shrink/empty frame {frame}");
        }
        let reference = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("full-upload oracle"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let render = |buffer: &wgpu::Buffer, batched: bool| {
            super::super::super::frame_stats::begin_scene_batches();
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                if batched {
                    draw_packed_textured_runs(
                        &mut pass,
                        &pipeline,
                        buffer,
                        &textures,
                        &draws,
                        0,
                        draws.len(),
                        cull,
                    );
                } else {
                    for i in 0..draws.len() {
                        draw_packed_textured_runs(
                            &mut pass,
                            &pipeline,
                            buffer,
                            &textures,
                            &draws,
                            i,
                            i + 1,
                            cull,
                        );
                    }
                }
            }
            let batches = crate::last_scene_batches().surface;
            let image = readback_texture(&gpu.device, &gpu.queue, &target, 96, 64, enc).unwrap();
            (image, batches)
        };
        let (actual, batches) = render(&stream.buffer, true);
        let (expected, reference_batches) = render(&reference, false);
        assert_eq!(actual.pixels, expected.pixels, "frame {frame}");
        if frame == 1 {
            assert_eq!(
                Some(&actual.pixels),
                first_image.as_ref(),
                "window resolution must be restored after an encoded capture"
            );
        }
        if frame == 0 {
            first_image = Some(actual.pixels.clone());
            assert_eq!(batches, 3);
            assert_eq!(reference_batches, 4);
            assert_ne!(
                actual.pixel(40, 30),
                Some([255; 4]),
                "must draw visible alpha content"
            );
        }
    }
}

#[test]
#[ignore = "requires native GPU readback; run explicitly"]
fn scene_submission_retained_clips_match_fresh_bindings() {
    let gpu = OffscreenGpu::with_backends(backend()).expect("requested GPU");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut pipeline = JadixPipeline::from_format(&gpu.device, format, 96, 64);
    let mut stream = RetainedBuffer::new(
        &gpu.device,
        "clip instances",
        wgpu::BufferUsages::VERTEX,
        &[],
    );
    let mut root = Element::new().w(96.0).h(64.0).bg([0.0, 0.0, 1.0, 0.6]);
    let instance = LayoutTree::new()
        .build_and_layout(&mut root, 96.0, 64.0, [-1.0; 2])
        .instances[0];
    let mut instances = vec![instance];
    let mut clips = vec![crate::style::GpuClipPoly::NONE];
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("clip binding oracle"),
        size: wgpu::Extent3d {
            width: 96,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    for frame in 0..10 {
        match frame {
            2 => {
                clips[0].round_rect = [24.0, 8.0, 48.0, 48.0];
                clips[0].round_radii = [12.0; 4];
            }
            3 => {
                instances.resize(9, instance);
                clips.resize(9, clips[0]);
            }
            4 => {
                instances.truncate(1);
                clips.truncate(1);
            }
            5 => {
                instances.clear();
                clips.clear();
            }
            6 => {
                instances.resize(8, instance);
                clips.resize(8, crate::style::GpuClipPoly::NONE);
            }
            7 => {
                instances[0].color = [1.0, 0.0, 0.0, 1.0];
                clips[1].round_rect = [0.0, 0.0, 16.0, 16.0];
            }
            _ => {}
        }
        stream.upload(&gpu.device, &gpu.queue, bytemuck::cast_slice(&instances));
        pipeline.ensure_clip_polys(&gpu.device, &gpu.queue, &clips);
        pipeline.prepare_main_instance_clips(&instances, &clips);
        let mut fresh = JadixPipeline::from_format(&gpu.device, format, 96, 64);
        fresh.ensure_clip_polys(&gpu.device, &gpu.queue, &clips);
        fresh.prepare_main_instance_clips(&instances, &clips);
        let buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fresh scene"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let render = |p: &JadixPipeline, buffer: &wgpu::Buffer| {
            let mut enc = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                if !instances.is_empty() {
                    p.bind_sdf(&mut pass, true, p.dummy_backdrop());
                    pass.set_vertex_buffer(0, p.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, buffer.slice(..));
                    pass.set_index_buffer(p.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    p.draw_main_instance_span(&mut pass, &instances, 0, instances.len() as u32);
                }
            }
            readback_texture(&gpu.device, &gpu.queue, &target, 96, 64, enc).unwrap()
        };
        let actual = render(&pipeline, &stream.buffer);
        assert_eq!(
            actual.pixels,
            render(&fresh, &buffer).pixels,
            "clip frame {frame}"
        );
        if frame == 2 {
            assert_eq!(actual.pixel(4, 4), Some([255; 4]));
            assert_ne!(actual.pixel(48, 32), Some([255; 4]));
        }
    }
}

#[test]
#[ignore = "requires native GPU completion; run explicitly"]
fn offscreen_submissions_complete_without_present_or_wait() {
    let gpu = OffscreenGpu::with_backends(backend()).expect("requested GPU");
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("offscreen completion work"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    for case in ["empty", "clear then empty", "upload then empty"] {
        match case {
            "clear then empty" => {
                let mut encoder = gpu.device.create_command_encoder(&Default::default());
                encoder.clear_buffer(&buffer, 0, None);
                gpu.queue.submit(Some(encoder.finish()));
            }
            "upload then empty" => gpu.queue.write_buffer(&buffer, 0, &[42; 256]),
            _ => {}
        }
        // Consecutive empty submissions must preserve completion ordering.
        for _ in 0..8 {
            gpu.queue.submit(std::iter::empty());
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        gpu.queue.on_submitted_work_done(move || sender.send(()).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while receiver.try_recv().is_err() {
            assert!(std::time::Instant::now() < deadline, "completion stalled: {case}");
            gpu.device.poll(wgpu::PollType::Poll).expect("nonblocking GPU poll");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}
