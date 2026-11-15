use super::*;
use gpu::low_level::{
    BindType, BufferProxy, Command, Download, DrawParams, Engine, EngineLimits,
    Error as RecordingError, ImageFormat, ImageProxy, Recording, ResourceProxy, ShaderId,
};

#[test]
#[ignore = "requires native compute"]
fn external_compute_resources_validate_usage_and_roundtrip() -> Result<(), Box<dyn Error>> {
    use wgpu::util::DeviceExt;
    let TestGpu { device, queue, .. } = test_gpu();
    let mut engine = Engine::new(&device, Default::default(), None)?;
    let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[7u32; 64]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let proxy = BufferProxy::new(256, "external");
    engine.import_buffer(proxy, source.clone())?;
    let shader=engine.add_compute_shader("imported","@group(0) @binding(0) var<storage,read_write> data:array<u32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){data[id.x]*=3u;}","main",&[BindType::Buffer])?;
    let mut r = Recording::default();
    r.dispatch(shader, [1, 1, 1], [proxy.into()]);
    r.download(proxy);
    let e = engine.execute(&queue, &r)?;
    let bytes = mapped(&device, &e.downloads[0])?;
    assert!(bytes
        .chunks_exact(4)
        .all(|b| u32::from_le_bytes(b.try_into().unwrap()) == 21));
    let mut bad = Recording::default();
    bad.clear_all(proxy);
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::InvalidBinding)
    );
    assert_eq!(
        engine.import_buffer(proxy, source).err(),
        Some(RecordingError::DescriptorConflict)
    );
    let target = initialization_tests::float_target(&device);
    let image = ImageProxy::new(32, 32, ImageFormat::Rgba16Float);
    engine.import_image(image, target.clone())?;
    let mut bad = Recording::default();
    bad.upload_image(image, vec![0; 32 * 32 * 8]);
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::InvalidBinding)
    );
    let draw=engine.add_render_shader("external-target","@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,y*2.-1.,0.,1.);} @fragment fn fs()->@location(0) vec4<f32>{return vec4(2.,-.125,1.,.5);}","vs","fs",ImageFormat::Rgba16Float,&[])?;
    let mut r = Recording::default();
    r.draw(DrawParams {
        shader: draw,
        target: image,
        resources: vec![],
        vertices: 0..3,
        instances: 0..1,
        clear: None,
    });
    r.download_image(image);
    r.free_image(image);
    r.free_buffer(proxy);
    let e = engine.execute(&queue, &r)?;
    assert_eq!(engine.live_resources(), 0);
    let bytes = mapped(&device, &e.downloads[0])?;
    let pixel = bytes[..8]
        .chunks_exact(2)
        .map(|b| half::f16::from_bits(u16::from_le_bytes(b.try_into().unwrap())).to_f32())
        .collect::<Vec<_>>();
    assert_eq!(pixel, vec![2., -0.125, 1., 0.5]);
    let retained = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    assert_eq!(&retained[..4], &pixel);
    std::fs::write(out().join("external-compute-resources.json"),"{\"status\":\"pass\",\"imported_buffer_elements\":64,\"usage_preflight\":true,\"external_image_target\":true,\"export_survives_release\":true}")?;
    Ok(())
}

fn mapped(device: &wgpu::Device, download: &Download) -> Result<Vec<u8>, Box<dyn Error>> {
    let (tx, rx) = std::sync::mpsc::channel();
    download
        .buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let bytes = download.buffer.slice(..).get_mapped_range()?.to_vec();
    download.buffer.unmap();
    Ok(bytes)
}
fn out() -> std::path::PathBuf {
    let p = std::env::var_os("JADIX_2D_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
#[ignore = "requires real compute adapter"]
fn recorded_resources_order_ranges_indirect_download_and_rejection() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let mut engine = Engine::new(&device, Default::default(), None)?;
    let shader=engine.add_compute_shader("ordered","@group(0) @binding(0) var<storage,read_write> a:array<u32>; @group(0) @binding(1) var<uniform> delta:vec4<u32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){if(id.x<257u){a[id.x]+=id.x+delta.x;}}","main",&[BindType::Buffer,BindType::Uniform])?;
    let mut recording = Recording::default();
    let values = recording.upload("values", vec![0; 257 * 4]);
    let uniform =
        recording.upload_uniform("params", bytemuck::cast_slice(&[3u32, 0, 0, 0]).to_vec());
    let args = recording.upload("indirect", bytemuck::cast_slice(&[5u32, 1, 1]).to_vec());
    recording.dispatch(shader, [5, 1, 1], [values.into(), uniform.into()]);
    recording.download(values);
    recording.push(Command::UploadUniform(
        uniform,
        bytemuck::cast_slice(&[7u32, 0, 0, 0]).to_vec().into(),
    ));
    recording.dispatch_indirect(shader, args, 0, [values.into(), uniform.into()]);
    recording.download(values);
    recording.push(Command::Clear(values, 256 * 4, Some(4)));
    recording.download(values);
    let execution = engine.execute(&queue, &recording)?;
    assert_eq!(execution.stats.dispatches, 2);
    assert_eq!(execution.stats.compiled_pipelines, 1);
    for (step, download) in execution.downloads.iter().enumerate() {
        let data = mapped(&device, download)?;
        for (i, b) in data[..257 * 4].chunks_exact(4).enumerate() {
            let expected = if step == 0 {
                i as u32 + 3
            } else if step == 2 && i == 256 {
                0
            } else {
                2 * i as u32 + 10
            };
            assert_eq!(
                u32::from_le_bytes(b.try_into().unwrap()),
                expected,
                "step {step} index {i}"
            );
        }
    }
    let copy=engine.add_compute_shader("range-copy","@group(0) @binding(0) var<storage,read> a:array<u32>; @group(0) @binding(1) var<storage,read_write> b:array<u32>; @compute @workgroup_size(1) fn main(){b[0]=a[0];}","main",&[BindType::BufReadOnly,BindType::Buffer])?;
    let mut range = Recording::default();
    let dest = BufferProxy::new(4, "range-result");
    let offset = device.limits().min_storage_buffer_offset_alignment as u64;
    range.dispatch(
        copy,
        [1, 1, 1],
        [
            ResourceProxy::BufferRange {
                proxy: values,
                offset,
                size: 4,
            },
            dest.into(),
        ],
    );
    range.download(dest);
    let execution = engine.execute(&queue, &range)?;
    let result = mapped(&device, &execution.downloads[0])?;
    assert_eq!(
        u32::from_le_bytes(result[..4].try_into().unwrap()),
        2 * (offset as u32 / 4) + 10
    );
    // A pipeline/entry-point failure must not submit preceding writes.
    let broken = engine.add_compute_shader(
        "bad-entry",
        "@compute @workgroup_size(1) fn actual(){}",
        "missing",
        &[],
    )?;
    let mut invalid_gpu = Recording::default();
    invalid_gpu.clear_all(values);
    invalid_gpu.dispatch(broken, [1, 1, 1], []);
    assert_eq!(
        engine.execute(&queue, &invalid_gpu).err(),
        Some(RecordingError::GpuValidation)
    );
    let mut unchanged = Recording::default();
    unchanged.download(values);
    let check = engine.execute(&queue, &unchanged)?;
    let bytes = mapped(&device, &check.downloads[0])?;
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 12);
    let before = engine.live_bytes();
    let mut bad = Recording::default();
    bad.clear_all(values);
    bad.dispatch(ShaderId(u64::MAX), [1, 1, 1], []);
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::InvalidShader)
    );
    assert_eq!(engine.live_bytes(), before);
    let mut bad = Recording::default();
    bad.dispatch(
        copy,
        [1, 1, 1],
        [
            ResourceProxy::BufferRange {
                proxy: values,
                offset: 1,
                size: 4,
            },
            dest.into(),
        ],
    );
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::InvalidRange)
    );
    let conflict = BufferProxy { size: 4, ..values };
    let mut bad = Recording::default();
    bad.download(conflict);
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::DescriptorConflict)
    );
    let mut release = Recording::default();
    for b in [values, uniform, args, dest] {
        release.free_buffer(b);
    }
    engine.execute(&queue, &release)?;
    assert_eq!(engine.live_resources(), 0);
    let mut bad = Recording::default();
    bad.download(values);
    assert_eq!(
        engine.execute(&queue, &bad).err(),
        Some(RecordingError::ReleasedResource)
    );
    engine.reset_resources();
    assert!(engine.remove_shader(shader));
    let mut limited = Engine::new(
        &device,
        EngineLimits {
            max_live_bytes: 16,
            ..Default::default()
        },
        None,
    )?;
    let mut bad = Recording::default();
    bad.upload("oversized", vec![0; 20]);
    assert_eq!(
        limited.execute(&queue, &bad).err(),
        Some(RecordingError::Budget)
    );
    assert_eq!(limited.live_resources(), 0);
    // Resources freed within an encoder are still retained by its commands.
    let mut bad = Recording::default();
    for _ in 0..2 {
        let b = bad.upload("peak", vec![0; 16]);
        bad.free_buffer(b);
    }
    assert_eq!(
        limited.execute(&queue, &bad).err(),
        Some(RecordingError::Budget)
    );
    std::fs::write(
        out().join("recorded-buffers.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","ordered_downloads":3,"elements":257,"direct_indirect":true,"range_readonly":true,"preflight_atomic":true,"released_and_conflicting_proxies_rejected":true,"peak_encoder_budget":true}),
        )?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires real compute adapter"]
fn recorded_image_upload_compute_draw_readback_and_canvas_interop() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let mut engine = Engine::new(&device, Default::default(), None)?;
    let shader=engine.add_compute_shader("image-compute","@group(0) @binding(0) var src:texture_2d<f32>; @group(0) @binding(1) var dst:texture_storage_2d<rgba16float,write>; @compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>){if(any(id.xy>=textureDimensions(dst))){return;} let c=textureLoad(src,vec2<i32>(id.xy),0);textureStore(dst,vec2<i32>(id.xy),vec4(c.r*2.,-0.125,c.b,0.5));}","main",&[BindType::ImageRead(ImageFormat::Rgba8),BindType::Image(ImageFormat::Rgba16Float)])?;
    let copy=engine.add_render_shader("image-draw","@group(0) @binding(0) var src:texture_2d<f32>; @vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let x=f32((i<<1u)&2u);let y=f32(i&2u);return vec4(x*2.-1.,y*2.-1.,0.,1.);} @fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{return textureLoad(src,vec2<i32>(p.xy),0);}","vs","fs",ImageFormat::Rgba16Float,&[BindType::ImageRead(ImageFormat::Rgba16Float)])?;
    let src = ImageProxy::new(13, 7, ImageFormat::Rgba8);
    let mid = ImageProxy::new(13, 7, ImageFormat::Rgba16Float);
    let dst = ImageProxy::new(13, 7, ImageFormat::Rgba16Float);
    let mut recording = Recording::default();
    recording.upload_image(src, [0, 0, 255, 255].repeat(13 * 7));
    recording.write_image(src, [2, 1], [3, 2], [255, 0, 0, 255].repeat(6));
    recording.dispatch(shader, [2, 1, 1], [src.into(), mid.into()]);
    recording.draw(DrawParams {
        shader: copy,
        target: dst,
        resources: vec![mid.into()],
        vertices: 0..3,
        instances: 0..1,
        clear: Some(wgpu::Color::TRANSPARENT),
    });
    recording.download_image(dst);
    let mut execution = engine.execute(&queue, &recording)?;
    assert_eq!(execution.stats.dispatches, 1);
    assert_eq!(execution.stats.draws, 1);
    let download = execution.take_download(dst.id).unwrap();
    let raw = mapped(&device, &download)?;
    let row = download.bytes_per_row.unwrap() as usize;
    for y in 0..7 {
        for x in 0..13 {
            let at = y * row + x * 8;
            let p: Vec<f32> = raw[at..at + 8]
                .chunks_exact(2)
                .map(|b| half::f16::from_bits(u16::from_le_bytes(b.try_into().unwrap())).to_f32())
                .collect();
            let red = (2..5).contains(&x) && (1..3).contains(&y);
            assert_eq!(
                p,
                if red {
                    vec![2., -0.125, 0., 0.5]
                } else {
                    vec![0., -0.125, 1., 0.5]
                }
            );
        }
    }
    let target = initialization_tests::float_target(&device);
    let mut renderer =
        PathGpu::try_new_with_format(&device, 1, wgpu::TextureFormat::Rgba16Float).unwrap();
    renderer.set_aa_quality(gpu::AaQuality::Area);
    let image = renderer.register_texture(
        engine.image(dst).unwrap().clone(),
        gpu::GpuImageColor::LinearSrgb,
        gpu::GpuImageAlpha::Straight,
    )?;
    let mut c = GpuCanvas::new_linear(32, 32, None);
    c.draw_image(&CanvasImageSource::Image(&image), 0., 0., 13., 7.);
    assert!(renderer.render(
        &device,
        &queue,
        &target.create_view(&Default::default()),
        [32, 32],
        &c.into_ops()
    ));
    let pixels = advanced_tests::read_float(&device, &queue, &target, [32, 32]);
    for (actual, expected) in pixels[(2 * 32 + 3) * 4..][..4]
        .iter()
        .zip([2., -0.125, 0., 0.5])
    {
        assert!((*actual - expected).abs() < 0.001);
    }
    let mut warm = Recording::default();
    warm.dispatch(shader, [2, 1, 1], [src.into(), mid.into()]);
    assert_eq!(engine.execute(&queue, &warm)?.stats.compiled_pipelines, 0);
    let mut release = Recording::default();
    for i in [src, mid, dst] {
        release.free_image(i);
    }
    engine.execute(&queue, &release)?;
    assert_eq!(engine.live_bytes(), 0);
    std::fs::write(
        out().join("recorded-images.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","size":[13,7],"partial_upload":[2,1,3,2],"row_stride":row,"hdr_signed":true,"compute_render_barriers":true,"canvas_interop":true,"warm_pipeline_compiles":0}),
        )?,
    )?;
    Ok(())
}

#[test]
#[ignore = "requires real GPU"]
fn staged_scene_capture_retained_output_and_encoded_fine() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let scene = Scene2D::record([32, 32], |c| {
        c.set_fill("#ff0000");
        c.fill_rect(0., 0., 32., 32.);
    })?;
    let mut render = gpu::low_level::Render::new(
        &device,
        &scene,
        [32, 32],
        SceneOptions::default(),
        gpu::RendererOptions {
            antialiasing: gpu::AaQuality::Area,
            ..Default::default()
        },
        true,
    )?;
    assert!(render.bump_estimate()?.triangles > 0);
    assert!(render.take_captured_scene().is_some());
    assert!(render.take_captured_scene().is_none());
    for _ in 0..2 {
        let mut encoder = device.create_command_encoder(&Default::default());
        render.record_fine(&device, &queue, &mut encoder, false)?;
        queue.submit([encoder.finish()]);
        let pixels = advanced_tests::read_float(&device, &queue, render.out_image(), [32, 32]);
        assert_eq!(&pixels[..4], &[1., 0., 0., 1.]);
    }
    assert_eq!(render.last_stats().queue_submissions, 0);
    std::fs::write(
        out().join("staged-scene.json"),
        "{\"status\":\"pass\",\"frames\":2,\"external_encoder\":true,\"capture_once\":true}",
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native compute"]
fn recorded_packets_warmup_and_caller_submission_order() -> Result<(), Box<dyn Error>> {
    use wgpu::util::DeviceExt;
    let TestGpu { device, queue, .. } = test_gpu();
    let mut engine = Engine::new(&device, Default::default(), None)?;
    let shader=engine.add_compute_shader("packets","@group(0) @binding(0) var<storage,read_write> data:array<u32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){data[id.x]*=3u;}","main",&[BindType::Buffer])?;
    let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[7u32; 64]),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    });
    let proxy = BufferProxy::new(256, "packet");
    engine.import_buffer(proxy, source.clone())?;
    let mut r = Recording::default();
    r.commands.push(Command::Upload(
        proxy,
        bytemuck::cast_slice(&[3u32; 64]).to_vec().into(),
    ));
    r.dispatch(shader, [1, 1, 1], [proxy.into()]);
    r.download(proxy);
    assert_eq!(engine.warm_up(&r)?, 1);
    assert_eq!(engine.warm_up(&r)?, 0);
    assert_eq!(engine.live_resources(), 1);
    let packet = engine.record(&r)?;
    assert_eq!(packet.stats.compiled_pipelines, 0);
    // An independent read proves neither warmup nor record submitted the upload.
    let before = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut copy = device.create_command_encoder(&Default::default());
    copy.copy_buffer_to_buffer(&source, 0, &before, 0, 256);
    queue.submit([copy.finish()]);
    let before = Download {
        resource: proxy.id,
        buffer: before,
        size: 256,
        bytes_per_row: None,
        image: None,
    };
    assert!(mapped(&device, &before)?
        .chunks_exact(4)
        .all(|b| u32::from_le_bytes(b.try_into().unwrap()) == 7));
    let mut prefix = device.create_command_encoder(&Default::default());
    prefix.clear_buffer(&source, 0, None);
    let mut suffix = device.create_command_encoder(&Default::default());
    suffix.copy_buffer_to_buffer(&source, 0, &before.buffer, 0, 256);
    queue.submit([prefix.finish(), packet.commands, suffix.finish()]);
    for d in [&before, &packet.downloads[0]] {
        assert!(mapped(&device, d)?
            .chunks_exact(4)
            .all(|b| u32::from_le_bytes(b.try_into().unwrap()) == 9));
    }
    let mut next = Recording::default();
    next.dispatch(shader, [1, 1, 1], [proxy.into()]);
    next.download(proxy);
    let e = engine.record(&next)?.submit(&queue);
    assert!(mapped(&device, &e.downloads[0])?
        .chunks_exact(4)
        .all(|b| u32::from_le_bytes(b.try_into().unwrap()) == 27));
    std::fs::write(out().join("recorded-packets.json"),"{\"status\":\"pass\",\"warmup_compiles\":[1,0],\"record_compiles\":0,\"caller_command_buffers\":3,\"before_submit\":7,\"after_submit\":9,\"after_next\":27}")?;
    Ok(())
}

#[test]
#[ignore = "requires native compute"]
fn pattern_texel_address_matches_integer_euclidean_oracle() -> Result<(), Box<dyn Error>> {
    let TestGpu { device, queue, .. } = test_gpu();
    let mut engine = Engine::new(&device, Default::default(), None)?;
    let original = include_str!("../../src/path_pipeline.wgsl");
    let start = original.find("fn pattern_index(").unwrap();
    let end = original.find("fn reconstruction_texel(").unwrap();
    let source=format!("{}\n@group(0) @binding(0) var<storage,read> input:array<vec4<f32>>; @group(0) @binding(1) var<storage,read_write> output:array<f32>; @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id:vec3<u32>){{if(id.x<arrayLength(&input)){{let p=input[id.x];output[id.x]=pattern_index(p.x,p.y,u32(p.z));}}}}",&original[start..end]);
    let shader = engine.add_compute_shader(
        "texel-address",
        &source,
        "main",
        &[BindType::BufReadOnly, BindType::Buffer],
    )?;
    let mut cases = vec![];
    let mut expected = vec![];
    for n in [1, 3, 15, 127] {
        for mode in 0..4 {
            for x in [
                -1e9f32, -10000., -254., -128., -127., -16., -15., -1., 0., 1., 14., 15., 16.,
                127., 128., 254., 10000., 1e9,
            ] {
                cases.push([x, n as f32, mode as f32, 0.]);
                let v = match mode {
                    1 => (x as i64).rem_euclid(n) as f32,
                    2 => {
                        ({
                            let q = (x as i64).rem_euclid(2 * n);
                            if q < n {
                                q
                            } else {
                                2 * n - 1 - q
                            }
                        }) as f32
                    }
                    3 => {
                        if x < 0. || x >= n as f32 {
                            -1.
                        } else {
                            x
                        }
                    }
                    _ => x.clamp(0., (n - 1) as f32),
                };
                expected.push(v);
            }
        }
    }
    let mut recording = Recording::default();
    let input = recording.upload("cases", bytemuck::cast_slice(&cases).to_vec());
    let output = BufferProxy::new(expected.len() as u64 * 4, "addresses");
    recording.dispatch(
        shader,
        [(expected.len() as u32).div_ceil(64), 1, 1],
        [input.into(), output.into()],
    );
    recording.download(output);
    let e = engine.execute(&queue, &recording)?;
    let actual = mapped(&device, &e.downloads[0])?;
    for (i, (b, e)) in actual.chunks_exact(4).zip(expected.iter()).enumerate() {
        assert_eq!(
            f32::from_le_bytes(b.try_into().unwrap()),
            *e,
            "case {:?}",
            cases[i]
        );
    }
    std::fs::write(
        out().join("texel-address.json"),
        serde_json::to_vec_pretty(
            &json!({"status":"pass","cases":cases.len(),"max_coordinate":1e9,"oracle":"signed i64 rem_euclid, not floating remainder","modes":4,"widths":[1,3,15,127]}),
        )?,
    )?;
    Ok(())
}
