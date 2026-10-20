use std::{
    future::Future,
    pin::pin,
    sync::Arc,
    task::{Context, Poll, Wake},
};

pub(super) fn block_on<F: Future>(f: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Arc::new(Noop).into();
    let mut cx = Context::from_waker(&waker);
    let mut f = pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

pub fn run() -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::GL;
    desc.backend_options.gl.gles_minor_version = wgpu::Gles3MinorVersion::Version1;
    desc.flags = wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL;
    let instance = wgpu::Instance::new(desc);
    run_with_instance(instance, false)
}

pub fn run_with_instance(
    instance: wgpu::Instance,
    native: bool,
) -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let info = adapter.get_info();
    if (!native && !info.name.contains("Direct3D11"))
        || (native && info.backend != wgpu::Backend::Dx11)
    {
        return Err(format!("unexpected renderer: {info:?}").into());
    }
    let srgb_blit = if native {
        serde_json::Value::Null
    } else {
        verify_srgb_blit(&adapter)?
    };
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: wgpu::Limits {
            max_inter_stage_shader_variables: adapter.limits().max_inter_stage_shader_variables,
            max_compute_workgroup_size_x: if native { 0 } else { 256 },
            max_compute_workgroup_size_y: if native { 0 } else { 256 },
            max_compute_workgroup_size_z: if native { 0 } else { 64 },
            max_compute_invocations_per_workgroup: if native { 0 } else { 256 },
            max_compute_workgroups_per_dimension: if native { 0 } else { 65535 },
            ..wgpu::Limits::downlevel_defaults()
        },
        ..Default::default()
    }))?;
    let surface_pixel = verify_surface(&instance, &adapter, &device, &queue)?;
    // Compile the real Jadix SDF shader on this backend before the tiny oracle.
    // ANGLE reserves an attribute for gl_InstanceID; an unused model UV at
    // location 1 previously pushed this program over its attribute budget.
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let sdf = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("real Jadix SDF diagnostic"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../../crates/jadix/src/renderer/rounded_rect.wgsl").into(),
        ),
    });
    let instance_attributes = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 8,
            shader_location: 3,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 4,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 32,
            shader_location: 5,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 48,
            shader_location: 6,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 64,
            shader_location: 7,
            format: wgpu::VertexFormat::Float32,
        },
        wgpu::VertexAttribute {
            offset: 68,
            shader_location: 8,
            format: wgpu::VertexFormat::Float32x4,
        },
        wgpu::VertexAttribute {
            offset: 84,
            shader_location: 9,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 92,
            shader_location: 10,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: 104,
            shader_location: 11,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 112,
            shader_location: 12,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 120,
            shader_location: 13,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 128,
            shader_location: 14,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 136,
            shader_location: 15,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    let model_attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
    let sdf_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("real Jadix SDF diagnostic"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &sdf,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &model_attributes,
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 152,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &instance_attributes,
                }),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: &sdf,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    if let Some(error) = block_on(scope.pop()) {
        return Err(error.into());
    }
    verify_sdf_draw(&device, &queue, &sdf_pipeline)?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            "@vertex fn vs_main(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
            let p = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
            return vec4f(p.x * 2. - 1., 1. - p.y * 2., 0., 1.);
        }
        @fragment fn ps_main() -> @location(0) vec4f { return vec4f(0.25, 0.5, 0.75, 1.); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("ps_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let extent = wgpu::Extent3d {
        width: super::SIZE as u32,
        height: super::SIZE as u32,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_scissor_rect(0, 0, 8, 8);
        pass.draw(0..3, 0..1);
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * super::SIZE as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(super::SIZE as u32),
            },
        },
        extent,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = buffer.slice(..).get_mapped_range()?;
    let pixels = mapped
        .chunks(256)
        .flat_map(|row| row[..super::SIZE * 4].iter().copied())
        .collect::<Vec<_>>();
    drop(mapped);
    buffer.unmap();
    Ok((
        serde_json::json!({"api": "D3D11", "implementation": if native {"native-wgpu-hal-d3d11"} else {"angle-d3d11"}, "device_name": info.name,
            "srgb_blit": srgb_blit,
            "surface_pixel": surface_pixel,
            "sdf_draw_with_polygon_clip": "passed",
            "vendor_id": info.vendor, "device_id": info.device, "driver": info.driver_info, "wgpu_backend": format!("{:?}", info.backend),
            "dxgi": if native {native_identity(&adapter)} else {super::angle_identity::selected_device(&adapter)},
            "max_storage_buffers_per_shader_stage": adapter.limits().max_storage_buffers_per_shader_stage,
            "max_compute_workgroups_per_dimension": adapter.limits().max_compute_workgroups_per_dimension}),
        pixels,
    ))
}

fn verify_sdf_draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::RenderPipeline,
) -> Result<(), Box<dyn std::error::Error>> {
    use wgpu::util::DeviceExt;
    let floats = |values: &[f32]| {
        values
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect::<Vec<_>>()
    };
    let buffer = |bytes: &[u8], usage| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("SDF draw regression"),
            contents: bytes,
            usage,
        })
    };
    let globals = buffer(&floats(&[16., 16., 0., 1.]), wgpu::BufferUsages::UNIFORM);
    let mut clip = vec![0_u8; 336];
    clip[..4].copy_from_slice(&3_u32.to_le_bytes());
    clip[16..40].copy_from_slice(&floats(&[0., 0., 16., 0., 0., 16.]));
    let clips = buffer(&clip, wgpu::BufferUsages::STORAGE);
    let group0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: clips.as_entire_binding(),
            },
        ],
    });
    let extent = wgpu::Extent3d {
        width: 16,
        height: 16,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SDF clipped pixels"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let backdrop = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let backdrop_view = backdrop.create_view(&Default::default());
    let sampler = device.create_sampler(&Default::default());
    let group1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&backdrop_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let vertices = buffer(
        &floats(&[
            0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 1., 0., 0., 1., 0., 0., 0., 1., 1.,
            0., 0.,
        ]),
        wgpu::BufferUsages::VERTEX,
    );
    let mut instance = [0_f32; 38];
    instance[2..8].copy_from_slice(&[16., 16., 1., 1., 1., 1.]);
    instance[12..16].fill(2.); // Rounded corners force the derivative path.
    instance[26] = 1.;
    instance[29] = 1.; // Identity transform.
    instance[34] = -1.;
    instance[35] = -1.; // No additional rectangular clip.
    let instances = buffer(&floats(&instance), wgpu::BufferUsages::VERTEX);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("SDF clip/derivative draw"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group0, &[]);
        pass.set_bind_group(1, &group1, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.draw(0..6, 0..1);
    }
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(16),
            },
        },
        extent,
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()??;
    let mapped = readback.slice(..).get_mapped_range()?;
    let inside = &mapped[4 * 256 + 4 * 4..4 * 256 + 4 * 4 + 4];
    let outside = &mapped[12 * 256 + 12 * 4..12 * 256 + 12 * 4 + 4];
    let valid = inside == [255, 255, 255, 255] && outside == [0, 0, 0, 255];
    let diagnostic = format!("SDF clip/derivative draw: inside {inside:?}, outside {outside:?}");
    drop(mapped);
    readback.unmap();
    if !valid {
        return Err(diagnostic.into());
    }
    Ok(())
}

// Reproduce the presentation transfer independently of the application's shaders.
// Upload encoded bytes into sRGB storage, then blit to another sRGB attachment.
fn verify_srgb_blit(
    adapter: &wgpu::Adapter,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use glow::HasContext;
    let hal = unsafe { adapter.as_hal::<wgpu::hal::gles::Api>() }.ok_or("not a GLES adapter")?;
    let gl = hal.adapter_context().lock();
    let expected = [24_u8, 24, 27, 255];
    unsafe {
        let _inherited_error = gl.get_error();
        let write_control = gl
            .supported_extensions()
            .contains("GL_EXT_sRGB_write_control");
        let mut objects = Vec::new();
        for _ in 0..2 {
            let texture = gl.create_texture()?;
            let framebuffer = gl.create_framebuffer()?;
            objects.push((texture, framebuffer));
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::SRGB8_ALPHA8 as i32,
                1,
                1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&expected)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE
            );
        }
        gl.disable(glow::SCISSOR_TEST);
        gl.color_mask(true, true, true, true);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(objects[0].1));
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(objects[1].1));
        let mut disabled = [0_u8; 4];
        let mut enabled = [0_u8; 4];
        for (conversion, pixel) in [(false, &mut disabled), (true, &mut enabled)] {
            if write_control {
                if conversion {
                    gl.enable(glow::FRAMEBUFFER_SRGB);
                } else {
                    gl.disable(glow::FRAMEBUFFER_SRGB);
                }
            }
            gl.blit_framebuffer(
                0,
                0,
                1,
                1,
                0,
                0,
                1,
                1,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(objects[1].1));
            gl.read_pixels(
                0,
                0,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(pixel)),
            );
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(objects[0].1));
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if write_control {
            gl.enable(glow::FRAMEBUFFER_SRGB);
        }
        for (texture, framebuffer) in objects {
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(texture);
        }
        let error = gl.get_error();
        if error != glow::NO_ERROR || enabled != expected {
            return Err(format!("sRGB blit failed: expected {expected:?}, first {disabled:?}, second {enabled:?}, GL {error:#x}").into());
        }
        Ok(
            serde_json::json!({"expected": expected, "write_control_supported": write_control, "first_blit": disabled, "second_blit": enabled}),
        )
    }
}

fn verify_surface(
    instance: &wgpu::Instance,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<[u8; 3]>, Box<dyn std::error::Error>> {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
    };
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
            Graphics::Gdi::{GetDC, GetPixel, ReleaseDC},
            UI::WindowsAndMessaging::*,
        },
    };
    struct Window(HWND);
    fn pump_messages() {
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    let window = Window(unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            w!("STATIC"),
            w!("ANGLE pixel oracle"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            16,
            16,
            None,
            None,
            None,
            None,
        )?
    });
    pump_messages();
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(RawDisplayHandle::Windows(WindowsDisplayHandle::new())),
            raw_window_handle: RawWindowHandle::Win32(Win32WindowHandle::new(
                std::num::NonZeroIsize::new(window.0 .0 as isize).ok_or("null HWND")?,
            )),
        })?
    };
    let mut config = surface
        .get_default_config(adapter, 16, 16)
        .ok_or("no surface config")?;
    config.format = if adapter.get_info().backend == wgpu::Backend::Dx11 {
        wgpu::TextureFormat::Bgra8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8UnormSrgb
    };
    surface.configure(device, &config);
    let mut pixels = Vec::new();
    for (index, expected) in [[24_u8, 24, 27], [1, 2, 3], [64, 128, 191], [255, 255, 255]]
        .into_iter()
        .enumerate()
    {
        if index == 2 {
            device.poll(wgpu::PollType::wait_indefinitely())?;
            unsafe {
                SetWindowPos(
                    window.0,
                    None,
                    0,
                    0,
                    32,
                    24,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                )?;
            }
            pump_messages();
            config.width = 32;
            config.height = 24;
            surface.configure(device, &config);
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            other => return Err(format!("surface acquire: {other:?}").into()),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let linear = |value: u8| {
            let encoded = f64::from(value) / 255.;
            if encoded <= 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            }
        };
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("window sRGB oracle"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: linear(expected[0]),
                        g: linear(expected[1]),
                        b: linear(expected[2]),
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        queue.submit(Some(encoder.finish()));
        queue.present(frame);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let pixel = unsafe {
            // Flip-model DXGI bypasses the HWND GDI backing store. Sample the composited screen.
            let dc = GetDC(None);
            let color = GetPixel(dc, 8, 8).0;
            ReleaseDC(None, dc);
            [
                (color & 255) as u8,
                ((color >> 8) & 255) as u8,
                ((color >> 16) & 255) as u8,
            ]
        };
        if pixel.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 1) {
            return Err(format!("window sRGB expected {expected:?}, got {pixel:?}").into());
        }
        pixels.push(pixel);
    }
    Ok(pixels)
}

fn native_identity(adapter: &wgpu::Adapter) -> serde_json::Value {
    #[cfg(feature = "native-wgpu")]
    {
        let hal = unsafe { adapter.as_hal::<wgpu_hal::dx11::Api>() }.unwrap();
        let d = unsafe { hal.raw_adapter().GetDesc1() }.unwrap();
        return serde_json::json!({"vendor_id":d.VendorId,"device_id":d.DeviceId,"adapter_luid":[d.AdapterLuid.HighPart,d.AdapterLuid.LowPart]});
    }
    #[cfg(not(feature = "native-wgpu"))]
    {
        let _ = adapter;
        unreachable!()
    }
}
