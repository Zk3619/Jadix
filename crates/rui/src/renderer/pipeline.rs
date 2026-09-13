use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub resolution: [f32; 2],
    pub premultiply: f32,
    pub _pad: f32,
}

impl Globals {
    pub fn new(width: f32, height: f32, premultiply: f32) -> Self {
        Self {
            resolution: [width, height],
            premultiply,
            _pad: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct VertexInput {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

impl VertexInput {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VertexInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct InstanceInput {
    pub rect_pos: [f32; 2],
    pub rect_size: [f32; 2],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub radii: [f32; 4], // CSS border-radius: TL, TR, BR, BL
    pub border_width: f32,
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_inset: f32,
    /// 2D affine transform `[a, b, c, d, tx, ty]`.
    /// Identity = `[1, 0, 0, 1, 0, 0]`.
    pub transform: [f32; 6],
    /// Overflow clip in `rect_pos` space. FS tests after undoing the instance affine.
    /// When `clip_size[0] < 0.0`, clipping is disabled.
    /// Dest-reset / node-present scissors use `NodePaint.overflow_clip` (paint AABB).
    pub clip_pos: [f32; 2],
    pub clip_size: [f32; 2],
    /// Packed border/corner data: `border_style + corner_shape * 16 + round(corner_param * 100) * 256`.
    /// border_style: 0=none, 1=solid, 2=dotted, 3=dashed, 4=double, 5=groove, 6=ridge, 7=inset, 8=outset.
    /// corner_shape: 0=round, 1=angle, 2=superellipse, 3=notch, 4=scoop.
    pub border_extra: f32,
    /// CSS `backdrop-filter: blur()` radius in logical px. Sampled by `rounded_rect.wgsl`
    /// against bind group 1 (blurred page capture). `0` skips sampling.
    pub backdrop_blur: f32,
}

impl InstanceInput {
    /// Sentinel: fragment shader skips scissor-style discard.
    pub const NO_CLIP_SIZE: [f32; 2] = [-1.0, -1.0];

    /// Pack border_style (0-8), corner_shape (0-4), and corner_param into a single f32.
    pub fn pack_border_extra(border_style: f32, corner_shape: f32, corner_param: f32) -> f32 {
        border_style + corner_shape * 16.0 + (corner_param * 100.0).round() * 256.0
    }

    /// Inverse of [`Self::pack_border_extra`]. Matches `rounded_rect.wgsl` unpack.
    pub fn unpack_border_extra(packed: f32) -> (f32, f32, f32) {
        let param_enc = (packed / 256.0).floor();
        let rem = packed - param_enc * 256.0;
        let shape = (rem / 16.0).floor();
        let style = rem - shape * 16.0;
        (style, shape, param_enc / 100.0)
    }

    pub fn with_clip(mut self, clip: Option<[f32; 4]>) -> Self {
        match clip {
            Some([x, y, w, h]) if w > 0.0 && h > 0.0 => {
                self.clip_pos = [x, y];
                self.clip_size = [w, h];
            }
            _ => {
                self.clip_pos = [0.0, 0.0];
                self.clip_size = Self::NO_CLIP_SIZE;
            }
        }
        self
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 68,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 84,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // shadow_params: [blur, spread, inset]
                wgpu::VertexAttribute {
                    offset: 92,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // transform_ab [a, b]
                wgpu::VertexAttribute {
                    offset: 104,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // transform_cd [c, d]
                wgpu::VertexAttribute {
                    offset: 112,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // transform_txty [tx, ty]
                wgpu::VertexAttribute {
                    offset: 120,
                    shader_location: 13,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // clip_pos
                wgpu::VertexAttribute {
                    offset: 128,
                    shader_location: 14,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // clip_size + border_extra + backdrop_blur
                wgpu::VertexAttribute {
                    offset: 136,
                    shader_location: 15,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct RuiPipeline {
    pub render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub globals_buffer: wgpu::Buffer,
    /// Safe default: a single `GpuClipPoly::NONE`. Chrome / underlines / offscreen subsets.
    pub bind_group: wgpu::BindGroup,
    /// Per-SDF-instance polygon clip; `instance_index` matches the main instance buffer.
    pub bind_group_clip_poly: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    backdrop_bgl: wgpu::BindGroupLayout,
    /// 1×1 transparent; instances with `backdrop_blur == 0` never sample it.
    dummy_backdrop_bg: wgpu::BindGroup,
    _dummy_backdrop_tex: wgpu::Texture,
    _dummy_backdrop_sampler: wgpu::Sampler,
    clip_poly_buffer: wgpu::Buffer,
    clip_poly_none_buffer: wgpu::Buffer,
    clip_poly_capacity: usize,
    premultiply: f32,
}

impl RuiPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        Self::with_wgsl(device, config, include_str!("rounded_rect.wgsl"))
    }

    /// Offscreen / headless: same shader path as [`Self::new`], without a window surface.
    pub fn from_format(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        Self::new(device, &config)
    }

    pub fn with_wgsl(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        wgsl_source: &str,
    ) -> Self {
        let premultiply = compositor_premultiply(config.alpha_mode);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rounded Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Buffer"),
            contents: bytemuck::cast_slice(&[Globals::new(
                config.width as f32,
                config.height as f32,
                premultiply,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let clip_none = crate::style::GpuClipPoly::NONE;
        let clip_poly_none_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Clip poly NONE"),
            contents: bytemuck::bytes_of(&clip_none),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let clip_poly_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Clip poly storage"),
            contents: bytemuck::bytes_of(&clip_none),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = Self::create_bind_group_layout(device);
        let backdrop_bgl = Self::create_backdrop_bind_group_layout(device);
        let (dummy_backdrop_tex, dummy_backdrop_sampler, dummy_backdrop_bg) =
            Self::make_dummy_backdrop(device, &backdrop_bgl);
        let bind_group = Self::make_bind_group(
            device,
            &bind_group_layout,
            &globals_buffer,
            &clip_poly_none_buffer,
        );
        let bind_group_clip_poly = Self::make_bind_group(
            device,
            &bind_group_layout,
            &globals_buffer,
            &clip_poly_buffer,
        );
        let render_pipeline =
            Self::make_render_pipeline(device, config, &shader, &bind_group_layout, &backdrop_bgl);

        let vertices = &[
            VertexInput {
                position: [0.0, 0.0],
                uv: [0.0, 0.0],
            },
            VertexInput {
                position: [1.0, 0.0],
                uv: [1.0, 0.0],
            },
            VertexInput {
                position: [1.0, 1.0],
                uv: [1.0, 1.0],
            },
            VertexInput {
                position: [0.0, 1.0],
                uv: [0.0, 1.0],
            },
        ];

        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            globals_buffer,
            bind_group,
            bind_group_clip_poly,
            bind_group_layout,
            backdrop_bgl,
            dummy_backdrop_bg,
            _dummy_backdrop_tex: dummy_backdrop_tex,
            _dummy_backdrop_sampler: dummy_backdrop_sampler,
            clip_poly_buffer,
            clip_poly_none_buffer,
            clip_poly_capacity: 1,
            premultiply,
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Globals Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_backdrop_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF Backdrop Bind Group Layout"),
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
        })
    }

    fn make_dummy_backdrop(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> (wgpu::Texture, wgpu::Sampler, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF dummy backdrop"),
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SDF dummy backdrop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF dummy backdrop BG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        (texture, sampler, bind_group)
    }

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals_buffer: &wgpu::Buffer,
        clip_poly_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: clip_poly_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn make_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        bind_group_layout: &wgpu::BindGroupLayout,
        backdrop_bgl: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Rui Render Pipeline Layout"),
                bind_group_layouts: &[Some(bind_group_layout), Some(backdrop_bgl)],
                immediate_size: 0,
            });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rui Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(VertexInput::desc()), Some(InstanceInput::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(crate::renderer::color_target_blend(config.alpha_mode)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    /// Upload per-instance polygon clips. Empty input stores one `NONE` (wgpu forbids 0-size storage).
    pub fn ensure_clip_polys(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        polys: &[crate::style::GpuClipPoly],
    ) {
        let data: Vec<crate::style::GpuClipPoly> = if polys.is_empty() {
            vec![crate::style::GpuClipPoly::NONE]
        } else {
            polys.to_vec()
        };
        let bytes = bytemuck::cast_slice(&data);
        if data.len() > self.clip_poly_capacity {
            self.clip_poly_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Clip poly storage"),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
            self.clip_poly_capacity = data.len();
            self.bind_group_clip_poly = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_buffer,
                &self.clip_poly_buffer,
            );
        } else {
            queue.write_buffer(&self.clip_poly_buffer, 0, bytes);
        }
    }

    /// Dev hot-reload: replace the main rect shader (entry points stay `vs_main` / `fs_main`).
    pub fn rebuild_wgsl(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        wgsl_source: &str,
    ) {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rounded Rect Shader (hot reload)"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });
        let (bind_group, bind_group_clip_poly, render_pipeline) = {
            let bind_group = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_buffer,
                &self.clip_poly_none_buffer,
            );
            let bind_group_clip_poly = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_buffer,
                &self.clip_poly_buffer,
            );
            let render_pipeline = Self::make_render_pipeline(
                device,
                config,
                &shader,
                &self.bind_group_layout,
                &self.backdrop_bgl,
            );
            (bind_group, bind_group_clip_poly, render_pipeline)
        };
        self.bind_group = bind_group;
        self.bind_group_clip_poly = bind_group_clip_poly;
        self.render_pipeline = render_pipeline;
    }

    /// Bind group 1 when the instance does not sample a blurred capture.
    pub fn dummy_backdrop(&self) -> &wgpu::BindGroup {
        &self.dummy_backdrop_bg
    }

    /// Bind group 1 for CSS `backdrop-filter` sampling (`frag_px / resolution`).
    pub fn make_backdrop_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF backdrop BG"),
            layout: &self.backdrop_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn bind_sdf<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        clip_poly: bool,
        backdrop: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(
            0,
            if clip_poly {
                &self.bind_group_clip_poly
            } else {
                &self.bind_group
            },
            &[],
        );
        pass.set_bind_group(1, backdrop, &[]);
    }

    pub fn update_resolution(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let globals = Globals::new(width as f32, height as f32, self.premultiply);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));
    }
}

pub(crate) fn compositor_premultiply(alpha_mode: wgpu::CompositeAlphaMode) -> f32 {
    match alpha_mode {
        wgpu::CompositeAlphaMode::PreMultiplied => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::InstanceInput;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
    }

    #[test]
    fn pack_unpack_border_extra_roundtrip() {
        let cases = [
            (1.0, 0.0, 0.0), // solid + round
            (2.0, 0.0, 0.0), // dotted + round
            (3.0, 0.0, 0.0), // dashed
            (1.0, 1.0, 0.0), // solid + angle
            (1.0, 2.0, 4.0), // superellipse(4)
            (1.0, 2.0, 8.0), // superellipse(8)
            (1.0, 3.0, 0.0), // notch
            (1.0, 4.0, 0.0), // scoop
        ];
        for (style, shape, param) in cases {
            let packed = InstanceInput::pack_border_extra(style, shape, param);
            let (s, sh, p) = InstanceInput::unpack_border_extra(packed);
            approx(s, style);
            approx(sh, shape);
            approx(p, param);
        }
    }
}
