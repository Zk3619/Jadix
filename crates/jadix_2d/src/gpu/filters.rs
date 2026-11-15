use super::*;
use crate::LayerFilter;

/// Three FP16 surfaces shared by sequential sibling groups preserve shadow inputs.
/// All stages share the caller's encoder; uniforms are uploaded once per chain.
pub(super) struct Shared {
    textures: [wgpu::Texture; 3],
    layout: wgpu::BindGroupLayout,
    pipelines: [wgpu::RenderPipeline; 2],
}
pub(super) struct Filters {
    shared: std::sync::Arc<Shared>,
    uniform: wgpu::Buffer,
    capacity: u64,
    stride: usize,
    bindings: HashMap<(usize, usize), wgpu::BindGroup>,
}
struct Stage {
    src: usize,
    extra: usize,
    out: usize,
    params: [f32; 96],
}
impl Shared {
    pub(super) fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        stats: &mut RenderStats,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let textures = std::array::from_fn(|_| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("group-filter-fp16"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("group-filter"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(384),
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
            label: Some("group-filter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("filters.wgsl").into()),
        });
        let pipelines = [wgpu::TextureFormat::Rgba16Float, format].map(|format| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("group-filter"),
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
                        format,
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
            })
        });
        stats.texture_allocations += 3;
        Self {
            textures,
            layout,
            pipelines,
        }
    }
}
impl Filters {
    pub(super) fn new(
        device: &wgpu::Device,
        shared: std::sync::Arc<Shared>,
        stats: &mut RenderStats,
    ) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("group-filter"),
            size: 384,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        stats.buffer_allocations += 1;
        Self {
            shared,
            uniform,
            capacity: 384,
            stride: 384usize.div_ceil(device.limits().min_uniform_buffer_offset_alignment as usize)
                * device.limits().min_uniform_buffer_offset_alignment as usize,
            bindings: HashMap::new(),
        }
    }
    fn gaussian(
        stages: &mut Vec<Stage>,
        mut current: usize,
        original: usize,
        sigma: [f32; 2],
    ) -> usize {
        for (axis, s) in sigma.into_iter().enumerate() {
            if s < 0.001 {
                continue;
            }
            let out = (1..=3).find(|i| *i != current && *i != original).unwrap();
            let mut params = [0.; 96];
            params[0] = 1.;
            params[1] = s;
            params[2] = axis as f32;
            stages.push(Stage {
                src: current,
                extra: current,
                out,
                params,
            });
            current = out;
        }
        current
    }
    pub(super) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        filters: &[LayerFilter],
        stats: &mut RenderStats,
    ) -> bool {
        let mut stages = Vec::new();
        let mut current = 0;
        for filter in filters.iter().filter(|f| f.valid()) {
            match filter {
                LayerFilter::ComponentTransfer {
                    channels,
                    color_space,
                } => {
                    let out = (1..=3).find(|i| *i != current).unwrap();
                    let mut params = [0.; 96];
                    params[0] = 6.;
                    params[1] = if *color_space == crate::FilterColorSpace::Srgb {
                        1.
                    } else {
                        0.
                    };
                    for (i, channel) in channels.iter().enumerate() {
                        channel.pack(&mut params[4 + i * 20..24 + i * 20]);
                    }
                    stages.push(Stage {
                        src: current,
                        extra: current,
                        out,
                        params,
                    });
                    current = out;
                }
                LayerFilter::ConvolveMatrix(c) => {
                    let out = (1..=3).find(|i| *i != current).unwrap();
                    let mut params = [0.; 96];
                    params[0] = 7.;
                    params[1] = c.order[0] as f32;
                    params[2] = c.order[1] as f32;
                    params[3] = if c.preserve_alpha { 1. } else { 0. };
                    params[4] = c.target[0] as f32;
                    params[5] = c.target[1] as f32;
                    params[6] = c.divisor();
                    params[7] = c.bias;
                    params[8] = c.unit_length[0];
                    params[9] = c.unit_length[1];
                    params[10] = match c.edge {
                        crate::FilterEdge::Transparent => 0.,
                        crate::FilterEdge::Duplicate => 1.,
                        crate::FilterEdge::Wrap => 2.,
                    };
                    params[11] = if c.color_space == crate::FilterColorSpace::Srgb {
                        1.
                    } else {
                        0.
                    };
                    params[12..12 + c.kernel.len()].copy_from_slice(&c.kernel);
                    stages.push(Stage {
                        src: current,
                        extra: current,
                        out,
                        params,
                    });
                    current = out;
                }
                LayerFilter::Offset { offset } => {
                    let out = (1..=3).find(|i| *i != current).unwrap();
                    let mut params = [0.; 96];
                    params[0] = 4.;
                    params[4..6].copy_from_slice(offset);
                    stages.push(Stage {
                        src: current,
                        extra: current,
                        out,
                        params,
                    });
                    current = out;
                }
                LayerFilter::Morphology { radius, operator } => {
                    for (axis, r) in radius.iter().enumerate() {
                        if *r == 0 {
                            continue;
                        }
                        let out = (1..=3).find(|i| *i != current).unwrap();
                        let mut params = [0.; 96];
                        params[0] = 5.;
                        params[1] = *r as f32;
                        params[2] = axis as f32;
                        params[3] = if *operator == crate::Morphology::Erode {
                            1.
                        } else {
                            0.
                        };
                        stages.push(Stage {
                            src: current,
                            extra: current,
                            out,
                            params,
                        });
                        current = out;
                    }
                }
                LayerFilter::Gaussian { sigma } => {
                    current = Self::gaussian(&mut stages, current, 0, *sigma);
                }
                LayerFilter::DropShadow {
                    sigma,
                    offset,
                    color,
                }
                | LayerFilter::DropShadowOnly {
                    sigma,
                    offset,
                    color,
                } => {
                    let original = current;
                    current = Self::gaussian(&mut stages, current, original, *sigma);
                    let out = (1..=3).find(|i| *i != current && *i != original).unwrap();
                    let mut params = [0.; 96];
                    params[0] = 2.;
                    params[3] = if matches!(filter, LayerFilter::DropShadowOnly { .. }) {
                        1.
                    } else {
                        0.
                    };
                    params[4..6].copy_from_slice(offset);
                    params[8..12].copy_from_slice(&crate::filter::shadow_color(*color, true));
                    stages.push(Stage {
                        src: original,
                        extra: current,
                        out,
                        params,
                    });
                    current = out;
                }
                LayerFilter::ColorMatrix(m) => {
                    let out = (1..=3).find(|i| *i != current).unwrap();
                    let mut params = [0.; 96];
                    params[0] = 3.;
                    for row in 0..4 {
                        params[12 + row * 4..16 + row * 4]
                            .copy_from_slice(&m[row * 5..row * 5 + 4]);
                        params[28 + row] = m[row * 5 + 4];
                    }
                    stages.push(Stage {
                        src: current,
                        extra: current,
                        out,
                        params,
                    });
                    current = out;
                }
            }
        }
        if current == 0 {
            return true;
        }
        stages.push(Stage {
            src: current,
            extra: current,
            out: 0,
            params: [0.; 96],
        });
        let Some(required) = (stages.len() as u64).checked_mul(self.stride as u64) else {
            return false;
        };
        if required > device.limits().max_buffer_size || required > u32::MAX as u64 {
            return false;
        }
        if required > self.capacity {
            self.capacity = required
                .next_power_of_two()
                .min(device.limits().max_buffer_size);
            self.uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("group-filter-frame"),
                size: self.capacity,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.bindings.clear();
            stats.buffer_allocations += 1;
        }
        let scratch = self
            .shared
            .textures
            .each_ref()
            .map(|t| t.create_view(&Default::default()));
        let views = [source, &scratch[0], &scratch[1], &scratch[2]];
        let mut bytes = vec![0u8; required as usize];
        for (i, stage) in stages.iter().enumerate() {
            let offset = i * self.stride;
            bytes[offset..offset + 384].copy_from_slice(bytemuck::cast_slice(&stage.params));
            let key = (stage.src, stage.extra);
            let bg = self.bindings.entry(key).or_insert_with(|| {
                stats.bind_group_allocations += 1;
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("group-filter"),
                    layout: &self.shared.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(views[stage.src]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(views[stage.extra]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.uniform,
                                offset: 0,
                                size: wgpu::BufferSize::new(384),
                            }),
                        },
                    ],
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("group-filter"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: views[stage.out],
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
            pass.set_pipeline(&self.shared.pipelines[usize::from(stage.out == 0)]);
            pass.set_bind_group(0, &*bg, &[offset as u32]);
            pass.draw(0..3, 0..1);
            stats.draw_calls += 1;
        }
        queue.write_buffer(&self.uniform, 0, &bytes);
        stats.uniform_uploads += 1;
        stats.uploaded_bytes += bytes.len() as u64;
        true
    }
}
