//! One scene entry point with an actual CPU execution/debug mode.
use super::*;
use crate::{PreparedSceneGpu, Scene2D, SceneError, SceneOptions, SceneSampling};
/// Retained scene renderer. CPU mode executes drawing/clip/filter/composition on
/// RasterCanvas, then uploads only the final pixels through a small export pass.
/// This is intentionally different from Vello's CPU-coarse/GPU-fine debugging.
pub struct SceneRenderer2D {
    options: RendererOptions,
    format: wgpu::TextureFormat,
    gpu: Option<PathGpu>,
    cpu: Option<CpuOutput>,
    stats: RenderStats,
}
impl SceneRenderer2D {
    pub fn new(
        device: &wgpu::Device,
        sample_count: u32,
        format: wgpu::TextureFormat,
        options: RendererOptions,
    ) -> Option<Self> {
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Rgba8UnormSrgb
        ) || options.max_surface_bytes == 0
            || options.max_surface_bytes > 1024 * 1024 * 1024
        {
            return None;
        }
        let gpu = if options.use_cpu {
            None
        } else {
            Some(PathGpu::try_new_with_options(
                device,
                sample_count,
                format,
                options.clone(),
            )?)
        };
        let cpu = if options.use_cpu {
            Some(jadix_renderer::wgpu_compat::try_gpu_init(
                device,
                "CPU scene export",
                || CpuOutput::new(device, format, &options),
            )?)
        } else {
            None
        };
        Some(Self {
            options,
            format,
            gpu,
            cpu,
            stats: Default::default(),
        })
    }
    pub fn uses_cpu(&self) -> bool {
        self.options.use_cpu
    }
    pub fn gpu_renderer_mut(&mut self) -> Option<&mut PathGpu> {
        self.gpu.as_mut()
    }
    pub fn last_stats(&self) -> RenderStats {
        self.stats
    }
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// Target view must match size, format and device. Failure preflight does not
    /// encode a draw. CPU source images use their CPU fallback, never GPU readback.
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit scene, render target and output alpha contract"
    )]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scene: &Scene2D,
        options: SceneOptions,
        premultiplied: bool,
    ) -> Result<(), SceneError> {
        if size.contains(&0)
            || size
                .iter()
                .any(|n| *n > device.limits().max_texture_dimension_2d)
        {
            return Err(SceneError::InvalidSize);
        }
        if let Some(gpu) = &mut self.gpu {
            let prepared = PreparedSceneGpu::new(scene, size, options)?;
            let ok = if premultiplied {
                gpu.render_premultiplied(device, queue, target, size, prepared.ops())
            } else {
                gpu.render(device, queue, target, size, prepared.ops())
            };
            if !ok {
                return Err(SceneError::UnsupportedCanvas);
            }
            self.stats = gpu.last_stats();
            return Ok(());
        }
        let axis = match self.options.antialiasing {
            AaQuality::Aliased | AaQuality::Area => 1,
            AaQuality::Balanced => 2,
            AaQuality::High => 4,
            AaQuality::Reference => 8,
        };
        let prepared = scene.sampled(
            size,
            options.clone(),
            SceneSampling {
                axis_samples: axis,
                max_surface_bytes: self.options.max_surface_bytes,
            },
        )?;
        let pixels = if self.options.antialiasing == AaQuality::Area {
            let mut canvas = crate::RasterCanvas::new_hdr(size[0], size[1]);
            canvas.set_area_antialiasing(true);
            scene.draw(&mut canvas, options)?;
            canvas.into_linear_rgba32f()
        } else {
            let image = prepared.rasterize()?;
            image
                .0
                .linear
                .as_ref()
                .unwrap()
                .chunks_exact(4)
                .flat_map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
                .collect()
        };
        self.stats = RenderStats {
            operations: scene.stats().expanded_commands,
            cpu_rasterized_pixels: size[0] as u64 * size[1] as u64 * axis as u64 * axis as u64,
            ..Default::default()
        };
        self.cpu.as_mut().unwrap().render(
            device,
            queue,
            target,
            size,
            &pixels,
            premultiplied,
            &mut self.stats,
        );
        Ok(())
    }
}
struct CpuOutput {
    layout: wgpu::BindGroupLayout,
    pipelines: [wgpu::RenderPipeline; 2],
    image: Option<(wgpu::Texture, wgpu::BindGroup)>,
}
impl CpuOutput {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, options: &RendererOptions) -> Self {
        let layout = texture_layout(device, "cpu-scene-export");
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu-scene-export"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene_renderer.wgsl").into()),
        });
        let mut pipelines = options::initialize(2, options.initialization_threads(), |i| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cpu-scene-export"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(if i == 0 { "straight" } else { "premul" }),
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
            })
        });
        let b = pipelines.pop().unwrap();
        let a = pipelines.pop().unwrap();
        Self {
            layout,
            pipelines: [a, b],
            image: None,
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit output contract and frame counters"
    )]
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        pixels: &[f32],
        premultiplied: bool,
        stats: &mut RenderStats,
    ) {
        if self
            .image
            .as_ref()
            .is_none_or(|v| [v.0.width(), v.0.height()] != size)
        {
            let image = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("cpu-scene-pixels"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &image.create_view(&Default::default()),
                    ),
                }],
            });
            self.image = Some((image, binding));
            stats.texture_allocations += 1;
            stats.bind_group_allocations += 1;
        }
        let image = self.image.as_ref().unwrap();
        let data = pixels
            .iter()
            .map(|v| half::f16::from_f32(*v))
            .collect::<Vec<_>>();
        queue.write_texture(
            image.0.as_image_copy(),
            bytemuck::cast_slice(&data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size[0] * 8),
                rows_per_image: Some(size[1]),
            },
            image.0.size(),
        );
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu-scene-export"),
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
            pass.set_pipeline(&self.pipelines[usize::from(premultiplied)]);
            pass.set_bind_group(0, &image.1, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        stats.queue_submissions = 1;
        stats.draw_calls = 1;
        stats.image_uploads = 1;
        stats.image_uploaded_bytes = (data.len() * 2) as u64;
        stats.uploaded_bytes = stats.image_uploaded_bytes;
    }
}
