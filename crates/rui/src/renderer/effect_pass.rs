//! GPU effect + subtree capture pass (B16). Consumes pooled textures, writes `rui_fx:` / `rui_capture:`.

use std::collections::{HashMap, HashSet};

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
    /// CSS filter captures whose SourceGraphic textures were ready at capture time.
    completed_filter_caps: HashSet<String>,
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
                        std::mem::size_of::<EffectUniforms>() as u64
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
            bind_group_layouts: &[Some(&uniform_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rui-effect-pipeline"),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
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
            completed_filter_caps: HashSet::new(),
        }
    }

    /// Render capture slices then apply queued effect jobs. Returns true if any work ran.
    ///
    /// When `text` is provided, capture jobs that include [`CaptureJob::text_nodes`] are
    /// reshaped and drawn into the RT (offscreen text-xform fallback).
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
        mut text: Option<&mut super::text::RuiTextRenderer>,
    ) -> bool {
        if captures.is_empty() && effects.is_empty() {
            return false;
        }
        let mut any = false;
        for cap in captures {
            if cap.instances.is_empty()
                && cap.textured_draws.is_empty()
                && cap.text_nodes.is_empty()
            {
                continue;
            }
            let scale = ui_scale;
            let w = (cap.bounds[2] * scale).round().max(2.0) as u32;
            let h = (cap.bounds[3] * scale).round().max(2.0) as u32;
            // CSS filter captures: content-keyed reuse across scroll.
            let filter_cap = cap.key.starts_with("rui_filter_cap:");
            let sources_ready = cap
                .textured_draws
                .iter()
                .all(|d| images.contains_key(&d.texture_key));
            if filter_cap
                && sources_ready
                && self.completed_filter_caps.contains(&cap.key)
                && images.contains_key(&cap.key)
                && self.target_size.get(&cap.key) == Some(&(w, h))
            {
                continue;
            }
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
                scale,
                text.as_deref_mut(),
            ) {
                any = true;
                if filter_cap && sources_ready {
                    self.completed_filter_caps.insert(cap.key.clone());
                }
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
            // Must match SDF / glyphon pipelines (surface sRGB).
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
        mut text: Option<&mut super::text::RuiTextRenderer>,
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
                // Clip rects are authored in page space; remap into the capture RT or the
                // SDF clip test rejects every fragment (empty / black SourceGraphic).
                if out.clip_size[0] >= 0.0 && out.clip_size[1] >= 0.0 {
                    out.clip_pos[0] = (out.clip_pos[0] - ox) * ui_scale;
                    out.clip_pos[1] = (out.clip_pos[1] - oy) * ui_scale;
                    out.clip_size[0] *= ui_scale;
                    out.clip_size[1] *= ui_scale;
                }
                // Shadow / blur offsets are in logical px.
                out.shadow_offset[0] *= ui_scale;
                out.shadow_offset[1] *= ui_scale;
                out.shadow_blur *= ui_scale;
                out.shadow_spread *= ui_scale;
                out.border_width *= ui_scale;
                out.radii[0] *= ui_scale;
                out.radii[1] *= ui_scale;
                out.radii[2] *= ui_scale;
                out.radii[3] *= ui_scale;
                out
            })
            .collect();
        let tex_insts: Vec<TexturedInstanceInput> = cap
            .textured_draws
            .iter()
            .map(|d| {
                let flags = if d
                    .texture_key
                    .starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX)
                {
                    1.0
                } else {
                    0.0
                };
                TexturedInstanceInput {
                    rect_pos: [(d.rect[0] - ox) * ui_scale, (d.rect[1] - oy) * ui_scale],
                    rect_size: [d.rect[2] * ui_scale, d.rect[3] * ui_scale],
                    tint: [d.tint_rgb[0], d.tint_rgb[1], d.tint_rgb[2], d.opacity],
                    uv_offset: d.uv_offset,
                    uv_scale: d.uv_scale,
                    transform: {
                        let mut t = d.transform;
                        t[4] *= ui_scale;
                        t[5] *= ui_scale;
                        t
                    },
                    flags,
                    _pad: [0.0; 3],
                }
            })
            .collect();

        // Shape capture text into glyphon buffers (axis-aligned; blit applies the matrix).
        // Match main prepare_text attrs (family / weight / align) so capture ink matches
        // unrotated labels — incomplete Attrs previously made strokes look thin/wrong.
        let mut capture_buffers: Vec<glyphon::Buffer> = Vec::new();
        if let Some(tr) = text.as_mut() {
            if !cap.text_nodes.is_empty() {
                let fs = tr.font_system;
                for info in &cap.text_nodes {
                    let size = (info.style.size_resolved() * ui_scale).max(1.0);
                    let lh = if let Some(px) = info.style.line_height_px {
                        px * ui_scale
                    } else if let Some(sc) = info.style.line_height_scale {
                        size * sc
                    } else {
                        size * crate::style::tailwind_default_leading_scale(info.style.size_resolved())
                    };
                    let family = if let Some(ref name) = info.style.font_family {
                        crate::utils::font::resolve_family(unsafe { &*fs }.db(), name)
                    } else {
                        crate::renderer::text_hit::best_font_family(unsafe { &*fs })
                    };
                    let weight = crate::utils::font::normalize_weight_for_family(
                        unsafe { &*fs }.db(),
                        family,
                        info.style.weight_resolved(),
                    );
                    let mut buffer =
                        glyphon::Buffer::new(unsafe { &mut *fs }, glyphon::Metrics::new(size, lh));
                    let color = info.style.color_resolved();
                    let attrs = glyphon::Attrs::new()
                        .family(family)
                        .weight(glyphon::fontdb::Weight(weight))
                        .color(glyphon::Color::rgba(color[0], color[1], color[2], color[3]));
                    buffer.set_text(&info.text, &attrs, glyphon::Shaping::Advanced, None);
                    let align = info.style.align_resolved().to_glyphon(info.style.direction);
                    for line in buffer.lines.iter_mut() {
                        line.set_align(Some(align));
                    }
                    let bw = (info.width * ui_scale).max(1.0);
                    let bh = (info.height * ui_scale).max(1.0);
                    buffer.set_size(Some(bw), Some(bh));
                    buffer.shape_until_scroll(unsafe { &mut *fs }, false);
                    capture_buffers.push(buffer);
                }
                let mut areas: Vec<glyphon::TextArea<'_>> =
                    Vec::with_capacity(capture_buffers.len());
                for (info, buffer) in cap.text_nodes.iter().zip(capture_buffers.iter()) {
                    let left = (info.x - ox) * ui_scale;
                    let top = (info.y - oy) * ui_scale;
                    let ps = if info.paint_scale.is_finite() && info.paint_scale > 0.0 {
                        info.paint_scale
                    } else {
                        1.0
                    };
                    areas.push(glyphon::TextArea {
                        buffer,
                        left,
                        top,
                        scale: ps,
                        bounds: glyphon::TextBounds {
                            left: 0,
                            top: 0,
                            right: w as i32,
                            bottom: h as i32,
                        },
                        default_color: glyphon::Color::rgba(
                            info.style.color_resolved()[0],
                            info.style.color_resolved()[1],
                            info.style.color_resolved()[2],
                            info.style.color_resolved()[3],
                        ),
                        custom_glyphs: &[],
                    });
                }
                tr.prepare_resolution(device, queue, w, h, areas.into_iter());
            }
        }

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
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("capture-tex-inst"),
                    contents: bytemuck::cast_slice(&tex_insts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            )
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rui-capture-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-capture-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
                    pass.set_index_buffer(
                        textured.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    pass.draw_indexed(0..textured.num_indices, 0, ti as u32..(ti + run) as u32);
                    ti += run;
                }
            }
            if let Some(tr) = text.as_ref() {
                if !cap.text_nodes.is_empty() {
                    tr.render(&mut pass);
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
            _pad0: 0.0,
            texel: [1.0 / w as f32, 1.0 / h as f32],
            _pad1: [0.0; 4],
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
