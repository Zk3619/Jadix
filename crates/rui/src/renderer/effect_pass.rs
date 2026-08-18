//! GPU effect + subtree capture pass (B16). Consumes pooled textures, writes `rui_fx:` / `rui_capture:`.

use std::collections::HashMap;

use wgpu::util::DeviceExt;

use super::effect::{CaptureJob, EffectJob, EffectUniforms};
use super::pipeline::VertexInput;
use super::{InstanceInput, TexturedInstanceInput, TexturedQuadPipeline};
use crate::app::image_registry::GpuImageTexture;

pub struct EffectPassGpu {
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    tex_bgl: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    target_size: HashMap<String, (u32, u32)>,
}

impl EffectPassGpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rui-effect-pass"),
            source: wgpu::ShaderSource::Wgsl(include_str!("effect_pass.wgsl").into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rui-effect-uniform-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<EffectUniforms>() as u64,
                    ),
                },
                count: None,
            }],
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rui-effect-tex-bgl"),
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
            label: Some("rui-effect-pl"),
            bind_group_layouts: &[&uniform_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rui-effect-pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rui-effect-uniforms"),
            size: std::mem::size_of::<EffectUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rui-effect-ubg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let vertices = &[
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
        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rui-effect-vb"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rui-effect-ib"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            uniform_buf,
            uniform_bg,
            tex_bgl,
            vertex_buffer,
            index_buffer,
            num_indices: 6,
            target_size: HashMap::new(),
        }
    }

    /// Render capture slices then apply queued effect jobs. Returns true if any work ran.
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        captures: &[CaptureJob],
        effects: &[EffectJob],
        time_s: f32,
        pointer: [f32; 2],
        ui_scale: f32,
        rect_pipeline: &super::RuiPipeline,
    ) -> bool {
        if captures.is_empty() && effects.is_empty() {
            return false;
        }
        let mut any = false;
        for cap in captures {
            if cap.instances.is_empty() && cap.textured_draws.is_empty() {
                continue;
            }
            let w = cap.bounds[2].round().max(2.0) as u32;
            let h = cap.bounds[3].round().max(2.0) as u32;
            self.ensure_target(device, textured, images, &cap.key, w, h);
            if self.render_capture(
                device,
                queue,
                textured,
                rect_pipeline,
                images,
                cap,
                w,
                h,
                ui_scale,
            ) {
                any = true;
            }
        }
        for job in effects {
            let w = job.bounds[2].round().max(2.0) as u32;
            let h = job.bounds[3].round().max(2.0) as u32;
            self.ensure_target(device, textured, images, &job.output_key, w, h);
            if images.get(&job.input_key).is_none() {
                continue;
            }
            if self.apply_effect(device, queue, textured, images, job, w, h, time_s, pointer) {
                any = true;
            }
        }
        if any {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        any
    }

    fn ensure_target(
        &mut self,
        device: &wgpu::Device,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        key: &str,
        w: u32,
        h: u32,
    ) {
        let stale = match self.target_size.get(key) {
            Some(&(cw, ch)) => cw != w || ch != h,
            None => true,
        };
        if !stale && images.contains_key(key) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rui-fx-target"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = textured.create_texture_bind_group(device, &view);
        images.insert(
            key.to_string(),
            GpuImageTexture {
                _texture: texture,
                bind_group,
            },
        );
        self.target_size.insert(key.to_string(), (w, h));
    }

    fn render_capture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        rect_pipeline: &super::RuiPipeline,
        images: &HashMap<String, GpuImageTexture>,
        cap: &CaptureJob,
        w: u32,
        h: u32,
        ui_scale: f32,
    ) -> bool {
        let Some(gpu_img) = images.get(&cap.key) else {
            return false;
        };
        let view = gpu_img
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let ox = cap.bounds[0];
        let oy = cap.bounds[1];
        let insts: Vec<InstanceInput> = cap
            .instances
            .iter()
            .map(|i| {
                let mut out = *i;
                out.rect_pos[0] = (out.rect_pos[0] - ox) * ui_scale;
                out.rect_pos[1] = (out.rect_pos[1] - oy) * ui_scale;
                out.rect_size[0] *= ui_scale;
                out.rect_size[1] *= ui_scale;
                out
            })
            .collect();
        let tex_insts: Vec<TexturedInstanceInput> = cap
            .textured_draws
            .iter()
            .map(|d| TexturedInstanceInput {
                rect_pos: [(d.rect[0] - ox) * ui_scale, (d.rect[1] - oy) * ui_scale],
                rect_size: [d.rect[2] * ui_scale, d.rect[3] * ui_scale],
                tint: [1.0, 1.0, 1.0, d.opacity],
                uv_offset: d.uv_offset,
                uv_scale: d.uv_scale,
            })
            .collect();

        rect_pipeline.update_resolution(queue, w, h);
        textured.update_resolution(queue, w, h);

        let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("capture-inst"),
            contents: bytemuck::cast_slice(&insts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let tex_buf = if tex_insts.is_empty() {
            None
        } else {
            Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("capture-tex-inst"),
                contents: bytemuck::cast_slice(&tex_insts),
                usage: wgpu::BufferUsages::VERTEX,
            }))
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rui-capture-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-capture-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            if !insts.is_empty() {
                pass.set_pipeline(&rect_pipeline.render_pipeline);
                pass.set_bind_group(0, &rect_pipeline.bind_group, &[]);
                pass.set_vertex_buffer(0, rect_pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(
                    rect_pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                pass.draw_indexed(0..rect_pipeline.num_indices, 0, 0..insts.len() as u32);
            }
            if let Some(buf) = tex_buf.as_ref() {
                pass.set_pipeline(&textured.render_pipeline);
                let mut ti = 0usize;
                while ti < tex_insts.len() {
                    let key = cap.textured_draws[ti].texture_key.as_str();
                    let Some(src) = images.get(key) else {
                        ti += 1;
                        continue;
                    };
                    let run = cap.textured_draws[ti..]
                        .iter()
                        .take_while(|d| d.texture_key == key)
                        .count();
                    pass.set_bind_group(0, &src.bind_group, &[]);
                    pass.set_vertex_buffer(0, textured.vertex_buffer.slice(..));
                    pass.set_vertex_buffer(1, buf.slice(..));
                    pass.set_index_buffer(textured.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..textured.num_indices, 0, ti as u32..(ti + run) as u32);
                    ti += run;
                }
            }
        }
        queue.submit(std::iter::once(encoder.finish()));
        true
    }

    fn apply_effect(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &HashMap<String, GpuImageTexture>,
        job: &EffectJob,
        w: u32,
        h: u32,
        time_s: f32,
        pointer: [f32; 2],
    ) -> bool {
        let Some(src) = images.get(&job.input_key) else {
            return false;
        };
        let Some(dst) = images.get(&job.output_key) else {
            return false;
        };
        let src_view = src
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = dst
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rui-effect-tex-bg"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&textured.sampler),
                },
            ],
        });
        let u = EffectUniforms {
            mode: job.kind.as_mode() as f32,
            time: time_s,
            pointer: [
                (pointer[0] - job.bounds[0]) / job.bounds[2].max(1.0),
                (pointer[1] - job.bounds[1]) / job.bounds[3].max(1.0),
            ],
            strength: job.strength,
            texel: [1.0 / w as f32, 1.0 / h as f32],
            _pad: 0.0,
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rui-effect-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-effect-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &tex_bg, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
        true
    }
}
