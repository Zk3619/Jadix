use super::*;
use crate::{CompositeMode, LayerOptions};

struct Source {
    renderer: PathGpu,
    texture: wgpu::Texture,
    filters: Option<super::filters::Filters>,
    region: [u32; 4],
}

/// One retained compositor per active recursion level. Children own their own
/// upload buffers: later passes never overwrite data still needed by earlier ones.
pub(super) struct Layers {
    options: RendererOptions,
    format: wgpu::TextureFormat,
    size: [u32; 2],
    targets: Option<[wgpu::Texture; 2]>,
    sources: Vec<Source>,
    filter_shared: HashMap<[u32; 2], std::sync::Weak<super::filters::Shared>>,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    uniform_capacity: u64,
    uniform_stride: usize,
    params: Vec<u8>,
    bindings: HashMap<(usize, usize, u64), wgpu::BindGroup>,
}

fn texture(device: &wgpu::Device, size: [u32; 2], format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("jadix-2d-layer"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

impl Layers {
    pub(super) fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        options: &RendererOptions,
    ) -> Self {
        let entry = |binding| wgpu::BindGroupLayoutEntry {
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
            label: Some("layer-composite"),
            entries: &[
                entry(0),
                entry(1),
                entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(48),
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
            label: Some("layer-composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("layers.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("layer-composite"),
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
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[(
                        "extended",
                        if format == wgpu::TextureFormat::Rgba16Float {
                            1.
                        } else {
                            0.
                        },
                    )],
                    ..Default::default()
                },
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: options.pipeline_cache.as_ref(),
        });
        Self {
            options: options.clone(),
            format,
            size,
            targets: None,
            sources: Vec::new(),
            filter_shared: HashMap::new(),
            layout,
            pipeline,
            bindings: HashMap::new(),
            uniform_capacity: 48,
            uniform_stride: 48usize
                .div_ceil(device.limits().min_uniform_buffer_offset_alignment as usize)
                * device.limits().min_uniform_buffer_offset_alignment as usize,
            params: Vec::new(),
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("layer-composite"),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit pass views, mask and parameter block"
    )]
    fn composite(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        views: [&wgpu::TextureView; 3],
        key: (usize, usize, u64),
        params: [f32; 8],
        origin: [f32; 2],
        mask: &wgpu::TextureView,
        stats: &mut RenderStats,
    ) {
        let [dst, src, out] = views;
        let offset = self.params.len();
        self.params.resize(offset + self.uniform_stride, 0);
        self.params[offset..offset + 32].copy_from_slice(bytemuck::cast_slice(&params));
        self.params[offset + 32..offset + 40].copy_from_slice(bytemuck::cast_slice(&origin));
        if !self.bindings.contains_key(&key) {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("layer-composite"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(mask),
                    },
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(dst),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.uniform,
                            offset: 0,
                            size: wgpu::BufferSize::new(48),
                        }),
                    },
                ],
            });
            stats.bind_group_allocations += 1;
            self.bindings.insert(key, bg);
        }
        let bg = &self.bindings[&key];
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer-composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bg, &[offset as u32]);
            pass.draw(0..3, 0..1);
        }
        stats.draw_calls += 1;
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "One child slot encodes into the shared frame"
    )]
    fn source(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        ops: &[PathGpuOp],
        filters: &[crate::LayerFilter],
        config: (u32, AaQuality, bool),
        stats: &mut RenderStats,
        external: &std::sync::Arc<super::external::ExternalImages>,
    ) -> Option<(wgpu::TextureView, [f32; 2])> {
        let region = if config.2 {
            super::bounds::domain(ops, filters, self.size)
        } else {
            [0, 0, self.size[0], self.size[1]]
        };
        let local_size = [region[2], region[3]];
        let shifted = (region[0] != 0 || region[1] != 0)
            .then(|| super::bounds::shift(ops, [region[0], region[1]]));
        let ops = shifted.as_deref().unwrap_or(ops);
        if slot == self.sources.len() {
            let renderer =
                PathGpu::try_new_with_options(device, config.0, self.format, self.options.clone())?;
            self.sources.push(Source {
                renderer,
                texture: texture(device, local_size, self.format),
                region,
                filters: None,
            });
            stats.texture_allocations += 1;
            stats.buffer_allocations += 1;
            stats.bind_group_allocations += 1;
        }
        let source = &mut self.sources[slot];
        if source.region[2..] != region[2..] {
            source.texture = texture(device, local_size, self.format);
            source.filters = None;
            self.bindings.clear();
            stats.texture_allocations += 1;
        }
        source.region = region;
        stats.layer_source_pixels += local_size[0] as u64 * local_size[1] as u64;
        stats.layer_full_canvas_pixels += self.size[0] as u64 * self.size[1] as u64;
        source.renderer.set_tight_layer_bounds(config.2);

        if !std::sync::Arc::ptr_eq(&source.renderer.external, external) {
            source.renderer.clear_image_cache();
            source.renderer.external = external.clone();
        }
        source.renderer.set_aa_quality(config.1);
        let view = source.texture.create_view(&Default::default());
        if !source
            .renderer
            .encode_impl(device, queue, encoder, &view, local_size, ops, true)
        {
            return None;
        }
        stats.add(source.renderer.last_stats());
        if filters.iter().any(crate::LayerFilter::valid) {
            self.filter_shared.retain(|_, v| v.strong_count() > 0);
            let shared = self
                .filter_shared
                .get(&local_size)
                .and_then(std::sync::Weak::upgrade)
                .unwrap_or_else(|| {
                    let v = std::sync::Arc::new(super::filters::Shared::new(
                        device,
                        local_size,
                        self.format,
                        stats,
                        self.options.pipeline_cache.as_ref(),
                    ));
                    self.filter_shared
                        .insert(local_size, std::sync::Arc::downgrade(&v));
                    v
                });
            let pipeline = source
                .filters
                .get_or_insert_with(|| super::filters::Filters::new(device, shared.clone(), stats));
            if !pipeline.render(device, queue, encoder, &view, filters, stats) {
                return None;
            }
        }
        Some((view, [region[0] as f32, region[1] as f32]))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Shared frame encoder, target and counters remain explicit"
    )]
    pub(super) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        out: &wgpu::TextureView,
        ops: &[PathGpuOp],
        config: (u32, AaQuality, bool, bool),
        stats: &mut RenderStats,
        textures: &HashMap<(u64, bool), CachedImage>,
        white_id: u64,
        external: &std::sync::Arc<super::external::ExternalImages>,
    ) -> bool {
        // A sole group has a transparent backdrop. Its blend function therefore
        // cancels out; opacity can be applied once at final export, including
        // chains of single nested groups. Never fold a group with siblings.
        self.bindings
            .retain(|(_, _, id), _| *id == white_id || textures.contains_key(&(*id, true)));
        let mut ops = ops;
        let mut root_opacity = 1.0;
        while let [PathGpuOp::Layer(layer)] = ops {
            if layer.compose != CompositeMode::SourceOver
                || layer.clip.is_some()
                || layer.mask.is_some()
                || !layer.filters.is_empty()
            {
                break;
            }
            root_opacity *= if layer.opacity.is_finite() {
                layer.opacity.clamp(0., 1.)
            } else {
                0.
            };
            ops = &layer.ops;
        }
        if root_opacity == 0. || ops.is_empty() {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("empty-layer"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: out,
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
            return true;
        }
        let required = (ops.len() as u64 + 1) * self.uniform_stride as u64;
        if required > device.limits().max_buffer_size || required > u32::MAX as u64 {
            return false;
        }
        if required > self.uniform_capacity {
            self.uniform_capacity = required
                .next_power_of_two()
                .min(device.limits().max_buffer_size);
            self.uniform = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("layer-frame-params"),
                size: self.uniform_capacity,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.bindings.clear();
            stats.buffer_allocations += 1;
        }
        self.params.clear();
        let white = textures[&(white_id, true)]
            ._texture
            .create_view(&Default::default());
        let (samples, quality, premultiplied, tight) = config;
        if !ops.iter().any(|op| matches!(op, PathGpuOp::Layer(_))) {
            let Some((src, origin)) = self.source(
                device,
                queue,
                encoder,
                0,
                ops,
                &[],
                (samples, quality, tight),
                stats,
                external,
            ) else {
                return false;
            };
            self.composite(
                device,
                encoder,
                [&src, &src, out],
                (usize::MAX, 2, white_id),
                [
                    root_opacity,
                    100.,
                    if premultiplied { 0. } else { 1. },
                    0.,
                    0.,
                    0.,
                    self.size[0] as f32,
                    self.size[1] as f32,
                ],
                origin,
                &white,
                stats,
            );
            queue.write_buffer(&self.uniform, 0, &self.params);
            stats.uniform_uploads += 1;
            stats.uploaded_bytes += self.params.len() as u64;
            return true;
        }
        if self.targets.is_none() {
            self.targets = Some([
                texture(device, self.size, self.format),
                texture(device, self.size, self.format),
            ]);
            stats.texture_allocations += 2;
        }
        let views = self
            .targets
            .as_ref()
            .unwrap()
            .each_ref()
            .map(|t| t.create_view(&Default::default()));
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("layer-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &views[0],
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
        }
        let mut cursor = 0;
        let mut slot = 0;
        let mut active = 0;
        while cursor < ops.len() {
            let (items, options) = if let PathGpuOp::Layer(layer) = &ops[cursor] {
                cursor += 1;
                (
                    &layer.ops[..],
                    LayerOptions {
                        opacity: layer.opacity,
                        blend: layer.blend,
                        compose: layer.compose,
                        clip: layer.clip,
                        mask: layer.mask.clone(),
                        filters: layer.filters.clone(),
                    }
                    .normalized(self.size[0], self.size[1]),
                )
            } else {
                let start = cursor;
                while cursor < ops.len() && !matches!(&ops[cursor], PathGpuOp::Layer(_)) {
                    cursor += 1;
                }
                (&ops[start..cursor], LayerOptions::default())
            };
            if options.compose == CompositeMode::Destination
                || (((items.is_empty()
                    && !options
                        .filters
                        .iter()
                        .any(crate::LayerFilter::creates_content))
                    || options.opacity == 0.)
                    && options.compose.preserves_destination_with_empty_source())
            {
                continue;
            }
            let Some((src, origin)) = self.source(
                device,
                queue,
                encoder,
                slot,
                items,
                &options.filters,
                (samples, quality, tight),
                stats,
                external,
            ) else {
                return false;
            };
            let mask_id = options.mask.as_ref().map_or(white_id, |m| m.0.id);
            let mask = textures[&(mask_id, true)]
                ._texture
                .create_view(&Default::default());
            let clip = options
                .clip
                .unwrap_or([0., 0., self.size[0] as f32, self.size[1] as f32]);
            self.composite(
                device,
                encoder,
                [&views[active], &src, &views[1 - active]],
                (slot, active, mask_id),
                [
                    options.opacity,
                    options.blend as u8 as f32,
                    0.,
                    options.compose as u8 as f32,
                    clip[0],
                    clip[1],
                    clip[2],
                    clip[3],
                ],
                origin,
                &mask,
                stats,
            );
            active = 1 - active;
            slot += 1;
        }
        // Retain peak slots and bindings for stable animation frames.
        self.composite(
            device,
            encoder,
            [&views[1 - active], &views[active], out],
            (usize::MAX, active, white_id),
            [
                root_opacity,
                100.,
                if premultiplied { 0. } else { 1. },
                0.,
                0.,
                0.,
                self.size[0] as f32,
                self.size[1] as f32,
            ],
            [0.; 2],
            &white,
            stats,
        );
        queue.write_buffer(&self.uniform, 0, &self.params);
        stats.uniform_uploads += 1;
        stats.uploaded_bytes += self.params.len() as u64;
        true
    }
    pub(super) fn size(&self) -> [u32; 2] {
        self.size
    }
}
