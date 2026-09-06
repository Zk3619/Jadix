//! Simple textured-quad pipeline (`image_texture` nodes), separate from the rounded-rect pipeline.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::pipeline::{Globals, VertexInput};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct TexturedInstanceInput {
    pub rect_pos: [f32; 2],
    pub rect_size: [f32; 2],
    pub tint: [f32; 4],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// 2D affine `[a, b, c, d, tx, ty]` about rect center (identity = no rotate).
    pub transform: [f32; 6],
    /// `0.0` = RGBA, `1.0` = MSDF, `2.0` = sparse capture blit (empty texels discard).
    pub flags: f32,
    /// Overflow clip in the same space as `rect_pos`. `clip_size[0] < 0` disables.
    pub clip_pos: [f32; 2],
    pub clip_size: [f32; 2],
    pub _pad: f32,
}

impl TexturedInstanceInput {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TexturedInstanceInput>() as wgpu::BufferAddress,
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
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 40,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 56,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 72,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 76,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 84,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

pub struct TexturedQuadPipeline {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub render_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub globals_buffer: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub sdf_sampler: wgpu::Sampler,
    premultiply: f32,
}

impl TexturedQuadPipeline {
    pub fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Textured Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("textured_rect.wgsl").into()),
        });

        let premultiply = super::pipeline::compositor_premultiply(config.alpha_mode);
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Textured Globals"),
            contents: bytemuck::cast_slice(&[Globals::new(
                config.width as f32,
                config.height as f32,
                premultiply,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Textured Bind Group Layout"),
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
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Rui image sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let sdf_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Rui SDF text sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Textured Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Textured Quad Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    Some(VertexInput::desc()),
                    Some(TexturedInstanceInput::desc()),
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertices = &[
            // Match capture / path RT space: rect top → uv.y = 0 (uploaded row 0 = image top).
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
            label: Some("Textured Quad VB"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Textured Quad IB"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            bind_group_layout,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            globals_buffer,
            sampler,
            sdf_sampler,
            premultiply,
        }
    }

    pub fn update_resolution(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let globals = Globals::new(width as f32, height as f32, self.premultiply);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));
    }

    pub fn create_texture_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.create_texture_bind_group_with_sampler(device, view, &self.sampler)
    }

    /// SDF/MSDF text atlases — Nearest avoids pre-blurring the distance field before FS reconstruct.
    pub fn create_texture_bind_group_sdf(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.create_texture_bind_group_with_sampler(device, view, &self.sdf_sampler)
    }

    fn create_texture_bind_group_with_sampler(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Image bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}
