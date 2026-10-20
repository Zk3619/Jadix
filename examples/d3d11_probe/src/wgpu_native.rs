use super::angle::block_on;
use wgpu::util::DeviceExt;
pub fn run() -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    let instance = unsafe {
        wgpu::Instance::from_hal::<wgpu_hal::dx11::Api>(wgpu_hal::dx11::Instance::new()?)
    };
    let (mut report, pixels) = super::angle::run_with_instance(instance.clone(), true)?;
    verify_updates(&instance)?;
    report["indexed_instanced_texture_storage_partial_updates_frames"] = 96.into();
    Ok((report, pixels))
}
fn verify_updates(instance: &wgpu::Instance) -> Result<(), Box<dyn std::error::Error>> {
    let adapter = block_on(instance.request_adapter(&Default::default()))?;
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_compute_workgroup_size_x = 0;
    limits.max_compute_workgroup_size_y = 0;
    limits.max_compute_workgroup_size_z = 0;
    limits.max_compute_invocations_per_workgroup = 0;
    limits.max_compute_workgroups_per_dimension = 0;
    let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))?;
    let shader=device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("DX11 native update oracle"),source:wgpu::ShaderSource::Wgsl(r#"
 @group(0) @binding(0) var<uniform> color: vec4f;
 @group(0) @binding(1) var<storage,read> factors:array<vec4f>;
 @group(0) @binding(2) var image:texture_2d<f32>;
 @group(0) @binding(3) var image_sampler:sampler;
 struct V { @builtin(position) position:vec4f, @location(0) tint:vec4f }
 @vertex fn vs(@location(0) position:vec2f,@location(1) offset:vec2f,@builtin(instance_index) instance:u32)->V {
 var v:V;v.position=vec4f(position+offset,0.,1.);v.tint=factors[instance];return v;
 }
 @fragment fn fs(v:V)->@location(0) vec4f {return color*v.tint*textureSample(image,image_sampler,vec2f(0.5));}
 "#.into())});
    let vertex = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck_bytes(&[-1f32, -1., 1., -1., 1., 1., -1., 1.]),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let offsets = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0; 16],
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    });
    let index = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[0, 0, 1, 0, 2, 0, 0, 0, 2, 0, 3, 0],
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
    });
    let mut uniform_data = vec![0u8; 512];
    uniform_data[256..272].copy_from_slice(bytemuck_bytes(&[1f32, 1., 1., 1.]));
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &uniform_data,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let storage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck_bytes(&[0f32, 0., 0., 1., 1., 1., 1., 1.]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    let sampler = device.create_sampler(&Default::default());
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 1,
                    }],
                }),
            ],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform,
                    offset: 0,
                    size: std::num::NonZeroU64::new(16),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: storage.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(
                    &image.create_view(&Default::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    for frame in 0..96u32 {
        let (width, height) = if frame < 48 { (19, 13) } else { (23, 17) };
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let texel = [64 + (frame % 4) as u8 * 32, 128, 192, 255];
        queue.write_texture(
            image.as_image_copy(),
            &texel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let green = if frame % 2 == 0 { 1. } else { 0.5 };
        queue.write_buffer(&uniform, 260, bytemuck_bytes(&[green]));
        queue.write_buffer(&storage, 16, bytemuck_bytes(&[1f32, 1., 0.5, 1.]));
        queue.write_buffer(&offsets, 8, &[0; 8]);
        queue.write_buffer(&index, 0, &[0, 0, 1, 0]);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let view = target.create_view(&Default::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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
            // Vertex bindings before pipeline and a nonzero first instance exercise replay ordering.
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.set_vertex_buffer(1, offsets.slice(..));
            pass.set_pipeline(&pipeline);
            pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint16);
            pass.set_bind_group(0, &group, &[256]);
            pass.set_scissor_rect(frame % 5, 0, width - frame % 5, height);
            // A changed base followed by identical draws exercises constant upload reuse.
            pass.draw_indexed(0..0, 0, 0..1);
            pass.draw_indexed(0..6, 0, 1..2);
            pass.draw_indexed(0..6, 0, 1..2);
        }
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(height),
                },
            },
            extent,
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let mapped = readback.slice(..).get_mapped_range()?;
        for y in 0..height {
            for x in 0..width {
                let expected = if x < frame % 5 {
                    [0, 0, 0, 255]
                } else {
                    [texel[0], (128. * green).round() as u8, 96, 255]
                };
                let start = (y * 256 + x * 4) as usize;
                let actual = &mapped[start..start + 4];
                if actual.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 1) {
                    return Err(format!(
                        "native update frame {frame}, ({x},{y}): {actual:?} != {expected:?}"
                    )
                    .into());
                }
            }
        }
        drop(mapped);
        readback.unmap();
    }
    Ok(())
}
fn bytemuck_bytes(data: &[f32]) -> &[u8] {
    unsafe { core::slice::from_raw_parts(data.as_ptr().cast(), core::mem::size_of_val(data)) }
}
#[test]
#[ignore = "requires native D3D11 hardware and an on-screen color oracle"]
fn native_wgpu_resource_and_window_oracle() {
    let (report, pixels) = run().unwrap();
    let (_, direct) = super::native::run(false).unwrap();
    assert_eq!(pixels, direct);
    assert_eq!(
        report["indexed_instanced_texture_storage_partial_updates_frames"],
        96
    );
}

#[cfg(test)]
mod sm5_register_tests {
    use naga::back::hlsl::*;
    #[test]
    fn fixed_samplers_and_draw_constants_only_for_sm5() {
        let module=naga::front::wgsl::parse_str(r#"
        @group(2) @binding(3) var tex:texture_2d<f32>;
        @group(2) @binding(5) var tex_sampler:sampler;
        @vertex fn vs(@builtin(vertex_index) id:u32)->@builtin(position) vec4f {return vec4f(f32(id),0.,0.,1.);}
        @fragment fn fs()->@location(0) vec4f{return textureSample(tex,tex_sampler,vec2f(0.5));}
        "#).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        for shader_model in [ShaderModel::V5_0, ShaderModel::V5_1] {
            let mut options = Options {
                shader_model,
                fake_missing_bindings: false,
                special_constants_binding: Some(BindTarget {
                    register: 13,
                    ..Default::default()
                }),
                ..Default::default()
            };
            options.binding_map.insert(
                naga::ResourceBinding {
                    group: 2,
                    binding: 3,
                },
                BindTarget {
                    register: 4,
                    ..Default::default()
                },
            );
            options.binding_map.insert(
                naga::ResourceBinding {
                    group: 2,
                    binding: 5,
                },
                BindTarget {
                    register: 7,
                    ..Default::default()
                },
            );
            options.sampler_buffer_binding_map.insert(
                SamplerIndexBufferKey { group: 2 },
                BindTarget {
                    register: 8,
                    ..Default::default()
                },
            );
            let mut source = String::new();
            Writer::new(
                &mut source,
                &options,
                &PipelineOptions { entry_point: None },
            )
            .write(&module, &info, None)
            .unwrap();
            if shader_model == ShaderModel::V5_0 {
                assert!(source.contains("register(s7)"));
                assert!(source.contains("cbuffer NagaDrawConstants : register(b13)"));
                assert!(!source.contains("nagaSamplerHeap"));
                assert!(!source.contains("ConstantBuffer<"));
            } else {
                assert!(source.contains("nagaSamplerHeap"));
                assert!(source.contains("ConstantBuffer<NagaConstants>"));
            }
        }
    }
}
