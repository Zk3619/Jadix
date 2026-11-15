//! Persistent triangle meshes with actual hardware instance-rate attributes.
use super::*;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MeshInstance {
    pub transform: [f32; 6],
    pub opacity: f32,
}
impl Default for MeshInstance {
    fn default() -> Self {
        Self {
            transform: [1., 0., 0., 1., 0., 0.],
            opacity: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeshColorSpace {
    #[default]
    Srgb,
    LinearSrgb,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshUploadStats {
    pub instance_bytes: u64,
    pub buffer_allocations: u32,
    pub instances: u32,
}

/// Retains tessellated geometry and its GPU vertex buffer. prepare() only uploads
/// changed instance data; draw() emits one GPU instanced draw. The host owns the
/// render pass, ordering, clipping, MSAA resolve, and any later group composition.
pub struct GpuMesh {
    vertices: wgpu::Buffer,
    count: u32,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    binding: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: u64,
    previous: Vec<MeshInstance>,
    resolution: [u32; 2],
    stats: MeshUploadStats,
}
impl GpuMesh {
    pub fn try_new(
        device: &wgpu::Device,
        tris: &[PathTri],
        format: wgpu::TextureFormat,
        samples: u32,
        space: MeshColorSpace,
    ) -> Option<Self> {
        if tris.is_empty()
            || tris.len() > 1_000_000
            || !matches!(samples, 1 | 4)
            || !matches!(
                format,
                wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba16Float
            )
            || tris.iter().any(|t| {
                [t.a.0, t.a.1, t.b.0, t.b.1, t.c.0, t.c.1]
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
                    || t.color.iter().any(|v| !v.is_finite() || v.abs() > 16384.)
                    || !(0. ..=1.).contains(&t.color[3])
            })
        {
            return None;
        }
        let bytes = tris.len() as u64 * 3 * 24;
        if bytes > device.limits().max_buffer_size {
            return None;
        }
        jadix_renderer::wgpu_compat::try_gpu_init(device, "2D hardware mesh instances", || {
            let vertices = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh-geometry-cache"),
                size: bytes,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: true,
            });
            {
                let mut view = vertices
                    .slice(..)
                    .get_mapped_range_mut()
                    .expect("owned mapped mesh buffer");
                let mut values = vec![0f32; tris.len() * 18];
                let mut cursor = 0;
                for t in tris {
                    for p in [t.a, t.b, t.c] {
                        values[cursor..cursor + 6].copy_from_slice(&[
                            p.0, p.1, t.color[0], t.color[1], t.color[2], t.color[3],
                        ]);
                        cursor += 6;
                    }
                }
                view.copy_from_slice(bytemuck::cast_slice(&values));
            }
            vertices.unmap();
            let globals = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh-globals"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mesh-globals"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                }],
            });
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mesh-globals"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                }],
            });
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mesh-instances"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("mesh-instances"),
                source: wgpu::ShaderSource::Wgsl(include_str!("instances.wgsl").into()),
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh-instances"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 24,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x4],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 28,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![2=>Float32x4,3=>Float32x3],
                        }),
                    ],
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[(
                            "linear_colors",
                            if space == MeshColorSpace::LinearSrgb {
                                1.
                            } else {
                                0.
                            },
                        )],
                        ..Default::default()
                    },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            });
            let instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh-instances"),
                size: 28,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Self {
                vertices,
                count: tris.len() as u32 * 3,
                pipeline,
                globals,
                binding,
                instances,
                capacity: 28,
                previous: Vec::new(),
                resolution: [0; 2],
                stats: Default::default(),
            }
        })
    }
    /// Invalid input leaves the preceding prepared batch unchanged. Prepare one
    /// batch per mesh per queue submission: later queue writes would otherwise
    /// also change earlier draws encoded against these same buffers.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        instances: &[MeshInstance],
    ) -> bool {
        let bytes = instances.len() as u64 * 28;
        if size.contains(&0)
            || size
                .iter()
                .any(|v| *v > device.limits().max_texture_dimension_2d)
            || instances.len() > 1_000_000
            || bytes > device.limits().max_buffer_size
            || instances.iter().any(|i| {
                i.transform.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
                    || !i.opacity.is_finite()
                    || !(0. ..=1.).contains(&i.opacity)
            })
        {
            return false;
        }
        self.stats = MeshUploadStats {
            instances: instances.len() as u32,
            ..Default::default()
        };
        if self.resolution != size {
            queue.write_buffer(
                &self.globals,
                0,
                bytemuck::cast_slice(&[size[0] as f32, size[1] as f32, 0., 0.]),
            );
            self.resolution = size;
        }
        if self.previous != instances {
            if bytes > self.capacity {
                self.capacity = bytes
                    .next_power_of_two()
                    .min(device.limits().max_buffer_size);
                self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("mesh-instances"),
                    size: self.capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.stats.buffer_allocations = 1;
            }
            if bytes > 0 {
                queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
            }
            self.stats.instance_bytes = bytes;
            self.previous.clear();
            self.previous.extend_from_slice(instances);
        }
        true
    }
    pub fn last_upload_stats(&self) -> MeshUploadStats {
        self.stats
    }
    pub fn geometry_bytes(&self) -> u64 {
        self.vertices.size()
    }
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.previous.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.draw(0..self.count, 0..self.previous.len() as u32);
    }
}
