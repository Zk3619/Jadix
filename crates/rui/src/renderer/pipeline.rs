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
    pub radii: [f32; 4],
    pub border_width: f32,
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_inset: f32,
    /// Visual rotation in degrees around rect center.
    pub rotate_deg: f32,
    /// Overflow clip in the same space as `rect_pos` (logical or physical after scale).
    /// When `clip_size[0] < 0.0`, clipping is disabled.
    pub clip_pos: [f32; 2],
    pub clip_size: [f32; 2],
    pub _pad: f32,
}

impl InstanceInput {
    /// Sentinel: fragment shader skips scissor-style discard.
    pub const NO_CLIP_SIZE: [f32; 2] = [-1.0, -1.0];

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
                wgpu::VertexAttribute {
                    offset: 92,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 100,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 104,
                    shader_location: 13,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 108,
                    shader_location: 14,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 116,
                    shader_location: 15,
                    format: wgpu::VertexFormat::Float32x2,
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
    pub bind_group: wgpu::BindGroup,
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

        let (bind_group, render_pipeline) =
            Self::bind_group_and_pipeline(device, config, &shader, &globals_buffer);

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
            premultiply,
        }
    }

    fn bind_group_and_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        globals_buffer: &wgpu::Buffer,
    ) -> (wgpu::BindGroup, wgpu::RenderPipeline) {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Globals Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Rui Render Pipeline Layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
        });

        (bind_group, render_pipeline)
    }

    /// 开发态热重载：替换主矩形着色器（入口仍?`vs_main` / `fs_main`）
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
        let (bind_group, render_pipeline) =
            Self::bind_group_and_pipeline(device, config, &shader, &self.globals_buffer);
        self.bind_group = bind_group;
        self.render_pipeline = render_pipeline;
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
