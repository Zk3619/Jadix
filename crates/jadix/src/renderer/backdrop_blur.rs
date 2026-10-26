//! Fullscreen modal backdrop blur (downsample → separable Gaussian → upsample).
//!
//! Local `mount=` scrims: capture prefix → `blur()` → the scrim SDF samples it.
//! Modal underlays: capture the **already-frosted** page → `blur()` → [`Self::blit_to`].
//! The two filters stack (CSS nested `backdrop-filter`); the modal blit must not
//! skip local capture and replace it with a dummy-backdrop page.
//!
//! Uniforms use a dynamic-offset ring so each pass in one encoder keeps its own
//! σ / direction (a single shared write was overwriting prior passes and left only
//! the last vertical pass — looking like a vertical streak blur).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::compute::ComputePipelines;
use super::pipeline::VertexInput;

/// wgpu minimum offset alignment for dynamic uniform buffers.
const UNIFORM_STRIDE: u64 = 256;
const MAX_PASSES: u64 = 8;

/// `mode`: 0 = downsample, 1 = Gaussian 1D, 2 = upsample.
/// WGSL uniform layout rounds struct size to vec2 alignment (32 bytes).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct BlurUniforms {
    texel_size: [f32; 2],
    direction: [f32; 2],
    sigma: f32,
    mode: f32,
    blit_alpha: f32,
    _pad: [f32; 1],
}

const _: () = assert!(std::mem::size_of::<BlurUniforms>() == 32);

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
    modal_blit_bind_group: Option<wgpu::BindGroup>,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Backdrop Blur PL"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Backdrop Blur Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(VertexInput::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
            multiview_mask: None,
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
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
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
            modal_blit_bind_group: None,
            width: 0,
            height: 0,
            work_w: 0,
            work_h: 0,
            format,
        }
    }

    /// Independent capture/work textures and uniforms, sharing the compiled GPU
    /// program. First modal open must not compile the local blur shader again.
    pub(crate) fn new_target(&self, device: &wgpu::Device) -> Self {
        let uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Modal Blur Uniforms"),
            size: UNIFORM_STRIDE * MAX_PASSES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Modal Blur Uniforms BG"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<BlurUniforms>() as u64),
                }),
            }],
        });
        Self {
            pipeline: self.pipeline.clone(),
            tex_bgl: self.tex_bgl.clone(),
            uniforms_buffer,
            uniforms_bind_group,
            sampler: self.sampler.clone(),
            vertex_buffer: self.vertex_buffer.clone(),
            index_buffer: self.index_buffer.clone(),
            num_indices: self.num_indices,
            scene: None,
            work_a: None,
            work_b: None,
            full: None,
            modal_blit_bind_group: None,
            width: 0,
            height: 0,
            work_w: 0,
            work_h: 0,
            format: self.format,
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
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

    pub(crate) fn local_capture_pad_px(radius_px: f32) -> u32 {
        let radius = radius_px.clamp(0.5, 64.0);
        let factor = Self::work_factor(radius);
        let sigma_work = (radius / factor as f32).clamp(0.5, 32.0);
        // Match gaussian1d's discrete tap support, including the optional second
        // H/V pair. Also cover downsample and final bilinear sample footprints.
        let support = (sigma_work * 3.0).ceil().clamp(1.0, 24.0) as u32
            + if sigma_work > 4.0 {
                (sigma_work * 0.55 * 3.0).ceil().clamp(1.0, 24.0) as u32
            } else { 0 };
        (support + 2) * factor + 2
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
        self.modal_blit_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Modal Blur Composite"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.full.as_ref().unwrap().view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.scene.as_ref().unwrap().view),
                },
            ],
        }));
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

    /// Map a scene-pixel scissor onto a (possibly downsampled) render target.
    pub(crate) fn map_scissor(
        region: (u32, u32, u32, u32),
        src_w: u32,
        src_h: u32,
        dst_w: u32,
        dst_h: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
            return None;
        }
        let sx = dst_w as f32 / src_w as f32;
        let sy = dst_h as f32 / src_h as f32;
        let x0 = (region.0 as f32 * sx).floor().max(0.0) as u32;
        let y0 = (region.1 as f32 * sy).floor().max(0.0) as u32;
        let x1 = ((region.0 as f32 + region.2 as f32) * sx)
            .ceil()
            .min(dst_w as f32)
            .max(0.0) as u32;
        let y1 = ((region.1 as f32 + region.3 as f32) * sy)
            .ceil()
            .min(dst_h as f32)
            .max(0.0) as u32;
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    /// Blur `scene` with a CSS-like Gaussian (σ ≈ radius_px).
    ///
    /// With `--features compute` and a live [`ComputePipelines`], H/V (and large-image
    /// downsample) run as compute; fragment downsample/upsample stay for sRGB surfaces.
    pub fn blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        radius_px: f32,
        compute: Option<&mut ComputePipelines>,
    ) {
        self.blur_with_region(device, queue, encoder, radius_px, compute, None);
    }

    /// Like [`Self::blur`], but each fragment pass is scissored to `region` in scene
    /// pixels. Local `mount=` frost only samples that AABB; filling the whole
    /// window for a 22rem card is what dropped gallery「浮层 / 反馈」frames.
    pub fn blur_with_region(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        radius_px: f32,
        compute: Option<&mut ComputePipelines>,
        region: Option<(u32, u32, u32, u32)>,
    ) {
        self.blur_impl(device, queue, encoder, radius_px, compute, region, None);
    }

    /// Modal fades keep one work resolution for the whole animation. Choosing
    /// it from the current (fading) sigma reallocates targets at threshold frames
    /// and runs the smallest sigmas at full screen resolution.
    pub(crate) fn blur_modal(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        radius_px: f32,
        target_radius_px: f32,
        compute: Option<&mut ComputePipelines>,
    ) {
        let factor = Self::work_factor(target_radius_px).max(2);
        self.blur_impl(device, queue, encoder, radius_px, compute, None, Some(factor));
    }

    fn blur_impl(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        radius_px: f32,
        mut compute: Option<&mut ComputePipelines>,
        region: Option<(u32, u32, u32, u32)>,
        work_factor: Option<u32>,
    ) {
        let radius = radius_px.clamp(0.5, 64.0);
        let factor = work_factor.unwrap_or_else(|| Self::work_factor(radius));
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
        let work_w = work_a.width;
        let work_h = work_a.height;
        let work_scissor =
            region.and_then(|r| Self::map_scissor(r, self.width, self.height, work_w, work_h));
        let full_scissor = region
            .and_then(|r| Self::map_scissor(r, self.width, self.height, full.width, full.height));
        // Compute path fills the whole work RT; local frost stays on the fragment
        // scissor so a 22rem card does not gaussian the gallery window.
        let use_compute = region.is_none();

        if use_compute {
            if let Some(pipes) = compute.as_mut() {
                if Self::try_compute_from_scene(
                    device,
                    queue,
                    encoder,
                    pipes,
                    &self.pipeline,
                    &self.uniforms_buffer,
                    &self.uniforms_bind_group,
                    &self.vertex_buffer,
                    &self.index_buffer,
                    self.num_indices,
                    &self.tex_bgl,
                    &self.sampler,
                    &scene.view,
                    &full.view,
                    factor,
                    work_w,
                    work_h,
                    work_texel,
                    sigma_work,
                ) {
                    return;
                }
            }
        }

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
                1.0,
                wgpu::Color::TRANSPARENT,
                work_scissor,
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
                1.0,
                wgpu::Color::TRANSPARENT,
                work_scissor,
            );
        }

        if use_compute {
            if let Some(pipes) = compute.as_mut() {
                if Self::try_compute_hv(
                    device,
                    queue,
                    encoder,
                    pipes,
                    &self.pipeline,
                    &self.uniforms_buffer,
                    &self.uniforms_bind_group,
                    &self.vertex_buffer,
                    &self.index_buffer,
                    self.num_indices,
                    &self.tex_bgl,
                    &self.sampler,
                    &work_a.view,
                    &full.view,
                    work_w,
                    work_h,
                    work_texel,
                    sigma_work,
                ) {
                    return;
                }
            }
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
            1.0,
            wgpu::Color::TRANSPARENT,
            work_scissor,
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
            1.0,
            wgpu::Color::TRANSPARENT,
            work_scissor,
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
                1.0,
                wgpu::Color::TRANSPARENT,
                work_scissor,
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
                1.0,
                wgpu::Color::TRANSPARENT,
                work_scissor,
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
            1.0,
            wgpu::Color::TRANSPARENT,
            full_scissor,
        );
    }

    fn tex_bind(
        device: &wgpu::Device,
        tex_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Backdrop Blur Compute Tex BG"),
            layout: tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }

    fn upsample_from(
        pipeline: &wgpu::RenderPipeline,
        uniforms_buffer: &wgpu::Buffer,
        uniforms_bind_group: &wgpu::BindGroup,
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        num_indices: u32,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src_bg: &wgpu::BindGroup,
        dst: &wgpu::TextureView,
        work_texel: [f32; 2],
    ) {
        let mut slot = 0u32;
        Self::dispatch_pass(
            &mut slot,
            pipeline,
            uniforms_buffer,
            uniforms_bind_group,
            vertex_buffer,
            index_buffer,
            num_indices,
            queue,
            encoder,
            src_bg,
            dst,
            work_texel,
            [0.0, 0.0],
            0.0,
            2.0,
            1.0,
            wgpu::Color::TRANSPARENT,
            None,
        );
    }

    fn try_compute_hv(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipes: &mut ComputePipelines,
        pipeline: &wgpu::RenderPipeline,
        uniforms_buffer: &wgpu::Buffer,
        uniforms_bind_group: &wgpu::BindGroup,
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        num_indices: u32,
        tex_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        src: &wgpu::TextureView,
        full: &wgpu::TextureView,
        work_w: u32,
        work_h: u32,
        work_texel: [f32; 2],
        sigma_work: f32,
    ) -> bool {
        let Some(cs) = pipes.blur.as_mut() else {
            return false;
        };
        if !cs.blur_encode(device, queue, encoder, src, work_w, work_h, sigma_work) {
            return false;
        }
        let Some(view) = cs.result_view() else {
            return false;
        };
        let bg = Self::tex_bind(device, tex_bgl, sampler, view);
        Self::upsample_from(
            pipeline,
            uniforms_buffer,
            uniforms_bind_group,
            vertex_buffer,
            index_buffer,
            num_indices,
            queue,
            encoder,
            &bg,
            full,
            work_texel,
        );
        true
    }

    fn try_compute_from_scene(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        pipes: &mut ComputePipelines,
        pipeline: &wgpu::RenderPipeline,
        uniforms_buffer: &wgpu::Buffer,
        uniforms_bind_group: &wgpu::BindGroup,
        vertex_buffer: &wgpu::Buffer,
        index_buffer: &wgpu::Buffer,
        num_indices: u32,
        tex_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        scene: &wgpu::TextureView,
        full: &wgpu::TextureView,
        factor: u32,
        work_w: u32,
        work_h: u32,
        work_texel: [f32; 2],
        sigma_work: f32,
    ) -> bool {
        if factor <= 1 {
            return Self::try_compute_hv(
                device,
                queue,
                encoder,
                pipes,
                pipeline,
                uniforms_buffer,
                uniforms_bind_group,
                vertex_buffer,
                index_buffer,
                num_indices,
                tex_bgl,
                sampler,
                scene,
                full,
                work_w,
                work_h,
                work_texel,
                sigma_work,
            );
        }
        let down_ok = pipes
            .histogram
            .as_mut()
            .is_some_and(|h| h.downsample_encode(device, queue, encoder, scene, work_w, work_h));
        if !down_ok {
            return false;
        }
        let src = {
            let Some(view) = pipes.histogram.as_ref().and_then(|h| h.result_view()) else {
                return false;
            };
            // Copy the view handle by encoding from it immediately.
            let Some(cs) = pipes.blur.as_mut() else {
                return false;
            };
            if !cs.blur_encode(device, queue, encoder, view, work_w, work_h, sigma_work) {
                return false;
            }
            cs.result_view().is_some()
        };
        if !src {
            return false;
        }
        let Some(view) = pipes.blur.as_ref().and_then(|b| b.result_view()) else {
            return false;
        };
        let bg = Self::tex_bind(device, tex_bgl, sampler, view);
        Self::upsample_from(
            pipeline,
            uniforms_buffer,
            uniforms_bind_group,
            vertex_buffer,
            index_buffer,
            num_indices,
            queue,
            encoder,
            &bg,
            full,
            work_texel,
        );
        true
    }

    /// Blit the blurred full-res result onto `dst` with the same UV orientation as capture.
    ///
    /// Prefer this path over [`crate::renderer::TexturedQuadPipeline`] for fullscreen page
    /// composites (dedicated blit UVs, no instance path).
    pub fn blit_to(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        dst: &wgpu::TextureView,
        alpha: f32,
        clear: wgpu::Color,
    ) {
        let Some(full) = self.full.as_ref() else {
            return;
        };
        // blur() and this blit are recorded into the same encoder. Queue buffer
        // writes run before submission, so reusing slot 0 overwrites the first
        // downsample pass's uniforms with blit mode. Reserve the final slot.
        let mut slot = (MAX_PASSES - 1) as u32;
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
            self.modal_blit_bind_group.as_ref().unwrap_or(&full.bind_group),
            dst,
            [1.0 / full.width as f32, 1.0 / full.height as f32],
            [0.0, 0.0],
            0.0,
            2.0,
            alpha.clamp(0.0, 1.0),
            clear,
            None,
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
        blit_alpha: f32,
        clear: wgpu::Color,
        scissor: Option<(u32, u32, u32, u32)>,
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
            blit_alpha,
            _pad: [0.0],
        };
        queue.write_buffer(uniforms_buffer, offset, bytemuck::bytes_of(&u));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Backdrop Blur Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, uniforms_bind_group, &[offset as u32]);
        pass.set_bind_group(1, src_bg, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        if let Some((x, y, w, h)) = scissor {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.draw_indexed(0..num_indices, 0, 0..1);
    }

    pub fn blurred_view(&self) -> Option<&wgpu::TextureView> {
        self.full.as_ref().map(|r| &r.view)
    }

    #[cfg(test)]
    pub(crate) fn blurred_texture(&self) -> Option<&wgpu::Texture> {
        self.full.as_ref().map(|r| &r._texture)
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    #[cfg(test)]
    pub(crate) fn work_size(&self) -> (u32, u32) {
        (self.work_w, self.work_h)
    }
}

#[cfg(test)]
mod map_scissor_tests {
    use super::BackdropBlurPipeline;

    #[test]
    fn identity_when_src_matches_dst() {
        assert_eq!(
            BackdropBlurPipeline::map_scissor((100, 80, 220, 140), 800, 600, 800, 600),
            Some((100, 80, 220, 140))
        );
    }

    #[test]
    fn halves_when_work_rt_is_half_res() {
        assert_eq!(
            BackdropBlurPipeline::map_scissor((100, 80, 220, 140), 800, 600, 400, 300),
            Some((50, 40, 110, 70))
        );
    }

    #[test]
    fn clamps_to_dst_bounds() {
        assert_eq!(
            BackdropBlurPipeline::map_scissor((700, 500, 200, 200), 800, 600, 800, 600),
            Some((700, 500, 100, 100))
        );
    }

    #[test]
    fn local_capture_pad_covers_each_gaussian_pass() {
        assert_eq!(BackdropBlurPipeline::local_capture_pad_px(2.0), 10);
        assert_eq!(BackdropBlurPipeline::local_capture_pad_px(12.0), 62);
        assert_eq!(BackdropBlurPipeline::local_capture_pad_px(20.0), 106);
    }
}
