//! Per-window theme crossfade. Content and interactions keep using the final theme;
//! only presentation is animated, so rich text, charts and portals follow together.

use super::{pipeline::VertexInput, present_retain::PresentRetainPipeline};
use wgpu::util::DeviceExt;

struct Targets {
    source: wgpu::Texture,
    presented: wgpu::Texture,
    presented_view: wgpu::TextureView,
    binding: wgpu::BindGroup,
    size: [u32; 2],
}

/// Retains the actually presented image, including an interrupted crossfade.
pub(crate) struct ThemeTransition {
    pipeline: wgpu::RenderPipeline,
    surface_pipeline: Option<wgpu::RenderPipeline>,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    progress: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    targets: Option<Targets>,
    generation: u64,
    started_ms: Option<f64>,
    pending_start: bool,
    config: crate::style::ThemeTransitionConfig,
    valid: bool,
}

impl ThemeTransition {
    #[cfg(test)]
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::with_compositor_encoding(device, format, false)
    }

    pub(crate) fn new_for_surface(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        Self::with_compositor_encoding(
            device,
            config.format,
            super::present_retain::surface_needs_compositor_encoding(config),
        )
    }

    fn with_compositor_encoding(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        compositor_encode: bool,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Theme transition layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Theme transition shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("theme_transition.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline_for = |entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Theme transition pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(VertexInput::desc())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
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
                cache: None,
            })
        };
        let pipeline = pipeline_for("fs_main");
        let surface_pipeline = compositor_encode.then(|| pipeline_for("fs_compositor"));
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[
                VertexInput {
                    position: [-1.0, -1.0],
                    uv: [0.0, 1.0],
                },
                VertexInput {
                    position: [1.0, -1.0],
                    uv: [1.0, 1.0],
                },
                VertexInput {
                    position: [1.0, 1.0],
                    uv: [1.0, 0.0],
                },
                VertexInput {
                    position: [-1.0, 1.0],
                    uv: [0.0, 0.0],
                },
            ]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[0u16, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::INDEX,
        });
        let progress = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[1.0f32, 0.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        Self {
            pipeline,
            surface_pipeline,
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor::default()),
            progress,
            vertices,
            indices,
            targets: None,
            generation: crate::style::theme_generation(),
            started_ms: None,
            pending_start: false,
            config: crate::style::ThemeTransitionConfig::default(),
            valid: false,
        }
    }

    pub(crate) fn needs_frame(&self) -> bool {
        self.started_ms.is_some() || self.generation != crate::style::theme_generation()
    }

    /// Called before content rendering, so an interrupted transition copies its last visible frame.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        content: &PresentRetainPipeline,
    ) {
        let Some(texture) = content.color_texture() else {
            return;
        };
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size != [texture.width(), texture.height()])
        {
            self.targets = None;
            self.started_ms = None;
            self.valid = false;
        }
        let config = crate::style::theme_transition_config();
        let enabled =
            config.enabled && config.duration_ms > 0.0 && !crate::style::prefers_reduced_motion();
        if !enabled {
            self.started_ms = None;
            self.targets = None;
            self.valid = false;
        }
        let generation = crate::style::theme_generation();
        if generation == self.generation {
            return;
        }
        self.generation = generation;
        if !enabled || !content.contents_valid() {
            self.started_ms = None;
            return;
        }
        let size = [texture.width(), texture.height()];
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let make = |label| {
                device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: texture.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: texture.format(),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                })
            };
            let source = make("Theme transition source");
            let source_view = source.create_view(&Default::default());
            let presented = make("Theme transition presented");
            let presented_view = presented.create_view(&Default::default());
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&source_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(content.color_view().unwrap()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.progress.as_entire_binding(),
                    },
                ],
            });
            self.targets = Some(Targets {
                source,
                presented,
                presented_view,
                binding,
                size,
            });
            self.valid = false;
            self.started_ms = None;
        }
        let targets = self.targets.as_ref().unwrap();
        let previous = if self.started_ms.is_some() && self.valid {
            &targets.presented
        } else {
            texture
        };
        encoder.copy_texture_to_texture(
            previous.as_image_copy(),
            targets.source.as_image_copy(),
            texture.size(),
        );
        self.config = config;
        self.started_ms = Some(crate::style::css_timeline_now_ms());
        self.pending_start = true;
    }

    /// Returns true until the final frame has been presented.
    pub(crate) fn present<F>(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        content: &PresentRetainPipeline,
        surface: &wgpu::TextureView,
        clear: wgpu::Color,
        overlay: F,
    ) -> bool
    where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let Some(targets) = self.targets.as_ref() else {
            content.blit_to_surface(encoder, surface, clear, overlay);
            return false;
        };
        if std::mem::take(&mut self.pending_start) && self.started_ms.is_some() {
            // Layout/glyph preparation on the first new-theme frame must not consume the fade.
            self.started_ms = Some(crate::style::css_timeline_now_ms());
        }
        let progress = if let Some(start) = self.started_ms {
            let t = ((crate::style::css_timeline_now_ms() - start) / self.config.duration_ms as f64)
                .clamp(0.0, 1.0) as f32;
            if t >= 1.0 {
                self.started_ms = None;
                1.0
            } else {
                self.config.timing().eval(t).clamp(0.0, 1.0)
            }
        } else {
            1.0
        };
        if self.started_ms.is_some() {
            queue.write_buffer(
                &self.progress,
                0,
                bytemuck::cast_slice(&[progress, 0.0, 0.0, 0.0]),
            );
            self.draw(
                encoder,
                &targets.presented_view,
                &targets.binding,
                clear,
                false,
                |_| {},
            );
            self.draw(encoder, surface, &targets.binding, clear, true, overlay);
        } else {
            content.blit_to_surface(encoder, surface, clear, overlay);
        }
        self.valid = true;
        let running = self.started_ms.is_some();
        if !running {
            // Keep only the normal retained content while idle.
            self.targets = None;
            self.valid = false;
        }
        running
    }

    #[cfg(test)]
    pub(crate) fn presented_texture_for_test(&self) -> Option<&wgpu::Texture> {
        if self.started_ms.is_some() && self.valid {
            self.targets.as_ref().map(|targets| &targets.presented)
        } else {
            None
        }
    }

    fn draw<F>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        binding: &wgpu::BindGroup,
        clear: wgpu::Color,
        compositor_output: bool,
        overlay: F,
    ) where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Theme crossfade"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(if compositor_output {
            self.surface_pipeline.as_ref().unwrap_or(&self.pipeline)
        } else {
            &self.pipeline
        });
        pass.set_bind_group(0, binding, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..1);
        overlay(&mut pass);
    }
}

#[cfg(test)]
#[path = "tests/theme_transition.rs"]
mod tests;
