//! A retained sampled scene and one GPU-only final resolve, without readback.
use super::*;
use crate::{SampledScene2D, SceneError};
/// Retained GPU resources for one frozen scene, sample grid and output format.
/// The target view must match size(), format and device; output replaces it.
/// GPU intermediate surfaces are linear premultiplied Rgba16Float.
pub struct PreparedSampledSceneGpu {
    sampled: SampledScene2D,
    ops: Vec<PathGpuOp>,
    renderer: PathGpu,
    high: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    params: wgpu::Buffer,
    binding: wgpu::BindGroup,
}
impl SampledScene2D {
    pub fn prepare_gpu(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Result<PreparedSampledSceneGpu, SceneError> {
        self.prepare_gpu_with_options(device, format, RendererOptions::default())
    }
    /// Same snapshot preparation with explicit shader worker/cache controls.
    pub fn prepare_gpu_with_options(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        options: RendererOptions,
    ) -> Result<PreparedSampledSceneGpu, SceneError> {
        if self.high_size.contains(&0) {
            return Err(SceneError::InvalidSize);
        }
        if self
            .high_size
            .iter()
            .any(|v| *v > device.limits().max_texture_dimension_2d)
        {
            return Err(SceneError::ImageBudget);
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba16Float
        ) {
            return Err(SceneError::InvalidOptions);
        }
        let ops = self.gpu_ops()?;
        let mut renderer = PathGpu::try_new_with_options(
            device,
            1,
            wgpu::TextureFormat::Rgba16Float,
            options.clone(),
        )
        .ok_or(SceneError::UnsupportedCanvas)?;
        renderer.set_aa_quality(AaQuality::Aliased);
        let built = jadix_renderer::wgpu_compat::try_gpu_init(device, "sampled scene", || {
            let high = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("whole-scene-samples"),
                size: wgpu::Extent3d {
                    width: self.high_size[0],
                    height: self.high_size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene-resolve"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16),
                        },
                        count: None,
                    },
                ],
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("scene-resolve"),
                source: wgpu::ShaderSource::Wgsl(include_str!("sampled.wgsl").into()),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene-resolve"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
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
                cache: options.pipeline_cache.as_ref(),
            });
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene-sampling"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &high.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: params.as_entire_binding(),
                    },
                ],
            });
            (high, pipeline, params, binding)
        })
        .ok_or(SceneError::UnsupportedCanvas)?;
        Ok(PreparedSampledSceneGpu {
            sampled: self.clone(),
            ops,
            renderer,
            high: built.0,
            pipeline: built.1,
            params: built.2,
            binding: built.3,
        })
    }
}
impl PreparedSampledSceneGpu {
    /// One encoder/submission for all draws, layers, filters and final resolve.
    /// False is an explicit renderer/device-budget failure; no CPU fallback.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        premultiplied: bool,
    ) -> bool {
        let mut encoder = device.create_command_encoder(&Default::default());
        if !self.renderer.encode_impl(
            device,
            queue,
            &mut encoder,
            &self.high.create_view(&Default::default()),
            self.sampled.high_size,
            &self.ops,
            true,
        ) {
            return false;
        }
        queue.write_buffer(
            &self.params,
            0,
            bytemuck::cast_slice(&[
                self.sampled.grid[0],
                u32::from(premultiplied),
                self.sampled.grid[1],
                0,
            ]),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("whole-scene-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_bind_group(0, &self.binding, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        self.renderer.stats.queue_submissions += 1;
        self.renderer.stats.draw_calls += 1;
        true
    }
    pub fn size(&self) -> [u32; 2] {
        self.sampled.size
    }
    pub fn last_stats(&self) -> RenderStats {
        self.renderer.last_stats()
    }
}

impl PathGpu {
    /// Prepare whole-scene samples with a snapshot of this renderer's external
    /// texture registrations. Subsequent changes to the parent are independent.
    /// The prepared renderer retains handles while its frozen scene owns images.
    pub fn prepare_sampled_scene(
        &self,
        device: &wgpu::Device,
        scene: &SampledScene2D,
        format: wgpu::TextureFormat,
    ) -> Result<PreparedSampledSceneGpu, SceneError> {
        let mut prepared = scene.prepare_gpu_with_options(device, format, self.options.clone())?;
        prepared.renderer.external = self.external.clone();
        Ok(prepared)
    }
}
impl PreparedSampledSceneGpu {
    /// Update this prepared renderer after the registered source is modified.
    /// Parent renderer registries remain independent (copy-on-write snapshots).
    pub fn mark_override_image_dirty(&mut self, image: &Image2D) -> Result<(), GpuImageError> {
        self.renderer.mark_override_image_dirty(image)
    }
    pub fn set_registered_texture_mipmaps(
        &mut self,
        image: &Image2D,
        limits: Option<crate::MipmapLimits>,
    ) -> Result<(), GpuImageError> {
        self.renderer.set_registered_texture_mipmaps(image, limits)
    }
}
