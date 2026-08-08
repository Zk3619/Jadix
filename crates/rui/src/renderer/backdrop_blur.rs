//! Fullscreen modal backdrop blur (downsample → separable Gaussian → upsample).
//!
//! Used when a portal underlay has [`crate::style::Style::backdrop_blur`] > 0.
//! General per-element backdrop-filter / dirty-region cache is deferred (plan B3).
//!
//! Uniforms use a dynamic-offset ring so each pass in one encoder keeps its own
//! σ / direction (a single shared write was overwriting prior passes and left only
//! the last vertical pass — looking like a vertical streak blur).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::pipeline::VertexInput;

/// wgpu minimum offset alignment for dynamic uniform buffers.
const UNIFORM_STRIDE: u64 = 256;
const MAX_PASSES: u64 = 8;

/// `mode`: 0 = downsample, 1 = Gaussian 1D, 2 = upsample.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct BlurUniforms {
    texel_size: [f32; 2],
    direction: [f32; 2],
    sigma: f32,
    mode: f32,
}

struct Rt {
    /// Kept alive so `view` / bind group remain valid.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Texture + sampler only (uniforms bound separately with dynamic offset).
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// Ping-pong blur + scene capture for modal underlays.
pub struct BackdropBlurPipeline {
    pipeline: wgpu::RenderPipeline,
    tex_bgl: wgpu::BindGroupLayout,
    uniforms_buffer: wgpu::Buffer,
    uniforms_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    /// Page layer capture (same format as surface).
    scene: Option<Rt>,
    work_a: Option<Rt>,
    work_b: Option<Rt>,
    full: Option<Rt>,
    width: u32,
    height: u32,
    work_w: u32,
    work_h: u32,
    format: wgpu::TextureFormat,
}

impl BackdropBlurPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Backdrop Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_blur.wgsl").into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Backdrop Blur Uniform BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<BlurUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Backdrop Blur Tex BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Backdrop Blur PL"),
            bind_group_layouts: &[&uniform_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Backdrop Blur Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[VertexInput::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Backdrop Blur Uniforms"),
            size: UNIFORM_STRIDE * MAX_PASSES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Backdrop Blur Uniforms BG"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<BlurUniforms>() as u64),
                }),
            }],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Backdrop Blur Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let verts: &[VertexInput] = &[
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
        ];
        let indices: &[u16] = &[0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Backdrop Blur VB"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Backdrop Blur IB"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            tex_bgl,
            uniforms_buffer,
            uniforms_bind_group,
            sampler,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            scene: None,
            work_a: None,
            work_b: None,
            full: None,
            width: 0,
            height: 0,
            work_w: 0,
            work_h: 0,
            format,
        }
    }

    fn make_rt(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        label: &str,
        tex_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Rt {
        let width = width.max(1);
        let height = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Backdrop Blur Tex BG"),
            layout: tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Rt {
            _texture: texture,
            view,
            bind_group,
            width,
            height,
        }
    }

    fn work_factor(radius_px: f32) -> u32 {
        if radius_px > 14.0 {
            4
        } else if radius_px > 5.0 {
            2
        } else {
            1
        }
    }

    pub fn ensure_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width
            && self.height == height
            && self.format == format
            && self.scene.is_some()
        {
            return;
        }
        self.width = width;
        self.height = height;
        self.format = format;
        self.work_w = 0;
        self.work_h = 0;
        self.scene = Some(Self::make_rt(
            device,
            format,
            width,
            height,
            "backdrop scene",
            &self.tex_bgl,
            &self.sampler,
        ));
        self.full = Some(Self::make_rt(
            device,
            format,
            width,
            height,
            "backdrop full",
            &self.tex_bgl,
            &self.sampler,
        ));
        self.work_a = None;
        self.work_b = None;
    }

    fn ensure_work(&mut self, device: &wgpu::Device, factor: u32) {
        let ww = (self.width / factor).max(1);
        let wh = (self.height / factor).max(1);
        if self.work_w == ww && self.work_h == wh && self.work_a.is_some() {
            return;
        }
        self.work_w = ww;
        self.work_h = wh;
        self.work_a = Some(Self::make_rt(
            device,
            self.format,
            ww,
            wh,
            "backdrop work A",
            &self.tex_bgl,
            &self.sampler,
        ));
        self.work_b = Some(Self::make_rt(
            device,
            self.format,
            ww,
            wh,
            "backdrop work B",
            &self.tex_bgl,
            &self.sampler,
        ));
    }

    pub fn scene_view(&self) -> Option<&wgpu::TextureView> {
        self.scene.as_ref().map(|r| &r.view)
    }

    /// Blur `scene` with a CSS-like Gaussian (σ ≈ radius_px).
    pub fn blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        radius_px: f32,
    ) {
        let radius = radius_px.clamp(0.5, 64.0);
        let factor = Self::work_factor(radius);
        self.ensure_work(device, factor);

        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        let Some(work_a) = self.work_a.as_ref() else {
            return;
        };
        let Some(work_b) = self.work_b.as_ref() else {
            return;
        };
        let Some(full) = self.full.as_ref() else {
            return;
        };

        // CSS Filter Effects: blur length ≈ Gaussian σ.
        let sigma_work = (radius / factor as f32).clamp(0.5, 32.0);
        // Destination texel in shared 0–1 UV space (correct footprint for reduce).
        let work_texel = [1.0 / work_a.width as f32, 1.0 / work_a.height as f32];
        let mut slot = 0u32;

        if factor > 1 {
            Self::dispatch_pass(
                &mut slot,
                &self.pipeline,
                &self.uniforms_buffer,
                &self.uniforms_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                self.num_indices,
                queue,
                encoder,
                &scene.bind_group,
                &work_a.view,
                work_texel,
                [0.0, 0.0],
                0.0,
                0.0,
            );
        } else {
            Self::dispatch_pass(
                &mut slot,
                &self.pipeline,
                &self.uniforms_buffer,
                &self.uniforms_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                self.num_indices,
                queue,
                encoder,
                &scene.bind_group,
                &work_a.view,
                work_texel,
                [0.0, 0.0],
                0.0,
                2.0,
            );
        }

        // Separable Gaussian: H then V (isotropic).
        Self::dispatch_pass(
            &mut slot,
            &self.pipeline,
            &self.uniforms_buffer,
            &self.uniforms_bind_group,
            &self.vertex_buffer,
            &self.index_buffer,
            self.num_indices,
            queue,
            encoder,
            &work_a.bind_group,
            &work_b.view,
            work_texel,
            [1.0, 0.0],
            sigma_work,
            1.0,
        );
        Self::dispatch_pass(
            &mut slot,
            &self.pipeline,
            &self.uniforms_buffer,
            &self.uniforms_bind_group,
            &self.vertex_buffer,
            &self.index_buffer,
            self.num_indices,
            queue,
            encoder,
            &work_b.bind_group,
            &work_a.view,
            work_texel,
            [0.0, 1.0],
            sigma_work,
            1.0,
        );

        if sigma_work > 4.0 {
            let s2 = sigma_work * 0.55;
            Self::dispatch_pass(
                &mut slot,
                &self.pipeline,
                &self.uniforms_buffer,
                &self.uniforms_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                self.num_indices,
                queue,
                encoder,
                &work_a.bind_group,
                &work_b.view,
                work_texel,
                [1.0, 0.0],
                s2,
                1.0,
            );
            Self::dispatch_pass(
                &mut slot,
                &self.pipeline,
                &self.uniforms_buffer,
                &self.uniforms_bind_group,
                &self.vertex_buffer,
                &self.index_buffer,
                self.num_indices,
                queue,
                encoder,
                &work_b.bind_group,
                &work_a.view,
                work_texel,
                [0.0, 1.0],
                s2,
                1.0,
            );
        }

        Self::dispatch_pass(
            &mut slot,
            &self.pipeline,
            &self.uniforms_buffer,
            &self.uniforms_bind_group,
            &self.vertex_buffer,
            &self.index_buffer,
            self.num_indices,
            queue,
            encoder,
            &work_a.bind_group,
            &full.view,
            work_texel,
            [0.0, 0.0],
            0.0,
            2.0,
        );
    }

    /// Blit the blurred full-res result onto `dst` with the same UV orientation as capture.
    ///
    /// Do **not** composite via [`crate::renderer::TexturedQuadPipeline`] — its unit-quad UVs
    /// are Y-flipped for uploaded images, which mirrors the page and makes the mask look
    /// unrelated to the UI underneath.
    pub fn blit_to(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        dst: &wgpu::TextureView,
    ) {
        let Some(full) = self.full.as_ref() else {
            return;
        };
        let mut slot = 0u32;
        Self::dispatch_pass(
            &mut slot,
            &self.pipeline,
            &self.uniforms_buffer,
            &self.uniforms_bind_group,
            &self.vertex_buffer,
            &self.index_buffer,
            self.num_indices,
            queue,
            encoder,
            &full.bind_group,
            dst,
            [1.0 / full.width as f32, 1.0 / full.height as f32],
            [0.0, 0.0],
            0.0,
            2.0,
        );
    }

    fn dispatch_pass(
        pass_slot: &mut u32,
        pipeline: &wgpu::RenderPipeline,
        uniforms_buffer: &wgpu::Buffer,
        uniforms_bind_group: &wgpu::BindGroup,
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        num_indices: u32,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_bg: &wgpu::BindGroup,
        dst_view: &wgpu::TextureView,
        texel_size: [f32; 2],
        direction: [f32; 2],
        sigma: f32,
        mode: f32,
    ) {
        let slot = *pass_slot as u64;
        debug_assert!(slot < MAX_PASSES, "backdrop blur exceeded MAX_PASSES");
        *pass_slot += 1;
        let offset = slot * UNIFORM_STRIDE;

        let u = BlurUniforms {
            texel_size,
            direction,
            sigma,
            mode,
        };
        queue.write_buffer(uniforms_buffer, offset, bytemuck::bytes_of(&u));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Backdrop Blur Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, uniforms_bind_group, &[offset as u32]);
        pass.set_bind_group(1, src_bg, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..num_indices, 0, 0..1);
    }

    pub fn blurred_view(&self) -> Option<&wgpu::TextureView> {
        self.full.as_ref().map(|r| &r.view)
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}
