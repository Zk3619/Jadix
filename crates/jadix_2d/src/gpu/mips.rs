//! Render-pass NPOT area prefilter. Each pass reads only the previous mip view.
use super::*;
pub(super) struct MipGenerator {
    layout: wgpu::BindGroupLayout,
    pipeline: Option<wgpu::RenderPipeline>,
    #[cfg(feature = "compute")]
    compute: Option<wgpu::ComputePipeline>,
}
impl MipGenerator {
    pub(super) fn new(device: &wgpu::Device, cache: Option<&wgpu::PipelineCache>) -> Self {
        #[cfg(feature = "compute")]
        if Self::supports_compute(device) {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("compute-mip-source"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba16Float,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("compute-npot-mips"),
                source: wgpu::ShaderSource::Wgsl(include_str!("mips_compute.wgsl").into()),
            });
            let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("compute-npot-mips"),
                layout: Some(&pl),
                module: &shader,
                entry_point: Some("cs"),
                compilation_options: Default::default(),
                cache,
            });
            return Self {
                layout,
                pipeline: None,
                compute: Some(compute),
            };
        }
        let layout = texture_layout(device, "mip-source");
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("linear-npot-mips"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mips.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("linear-npot-mips"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache,
        });
        Self {
            layout,
            pipeline: Some(pipeline),
            #[cfg(feature = "compute")]
            compute: None,
        }
    }
    pub(super) fn supports_compute(device: &wgpu::Device) -> bool {
        #[cfg(feature = "compute")]
        {
            let l = device.limits();
            l.max_compute_workgroup_size_x >= 8
                && l.max_compute_workgroup_size_y >= 8
                && l.max_compute_workgroup_size_z >= 1
                && l.max_compute_invocations_per_workgroup >= 64
                && l.max_storage_textures_per_shader_stage >= 1
                && wgpu::TextureFormat::Rgba16Float
                    .guaranteed_format_features(device.features())
                    .allowed_usages
                    .contains(wgpu::TextureUsages::STORAGE_BINDING)
        }
        #[cfg(not(feature = "compute"))]
        {
            let _ = device;
            false
        }
    }
    pub(super) fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        stats: &mut RenderStats,
    ) {
        for level in 1..texture.mip_level_count() {
            let view = |level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            let source = view(level - 1);
            let output = view(level);
            #[cfg(feature = "compute")]
            if let Some(pipeline) = &self.compute {
                let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("compute-mip-level"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&output),
                        },
                    ],
                });
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("compute-mip-level"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &binding, &[]);
                pass.dispatch_workgroups(
                    (texture.width() >> level).max(1).div_ceil(8),
                    (texture.height() >> level).max(1).div_ceil(8),
                    1,
                );
                stats.mip_generation_passes += 1;
                stats.compute_dispatches += 1;
                stats.bind_group_allocations += 1;
                continue;
            }
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mip-source"),
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source),
                }],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip-downsample"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output,
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
            pass.set_pipeline(self.pipeline.as_ref().unwrap());
            pass.set_bind_group(0, &binding, &[]);
            pass.draw(0..3, 0..1);
            stats.mip_generation_passes += 1;
            stats.draw_calls += 1;
            stats.bind_group_allocations += 1;
        }
    }
}
