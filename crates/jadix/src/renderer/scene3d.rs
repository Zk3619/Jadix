//! WP-3.13：同一 wgpu device 上的透视立方 → `jadix_scene3d:` 贴图。
//!
//! 仅 `scene3d` feature。无 glTF / 无 three.js。采样走 [`TexturedQuadPipeline`]。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::app::image_registry::GpuImageTexture;
use crate::primitives::scene3d::take_scene3d_jobs;
use crate::renderer::textured_pipeline::TexturedInstanceInput;
use crate::renderer::TexturedQuadPipeline;

static RUNTIME_ON: AtomicBool = AtomicBool::new(true);

pub(crate) fn scene3d_gpu_enabled() -> bool {
    RUNTIME_ON.load(Ordering::Relaxed)
}

pub(crate) fn disable_scene3d_gpu_runtime() {
    RUNTIME_ON.store(false, Ordering::Relaxed);
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CubeVert {
    pos: [f32; 3],
    color: [f32; 3],
}

impl CubeVert {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CubeVert>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

pub struct Scene3dGpu {
    pipeline: wgpu::RenderPipeline,
    target_format: wgpu::TextureFormat,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    vertex_count: u32,
    depth: Option<(u32, u32, wgpu::Texture)>,
    /// 与 `image_textures` 同步的尺寸，避免每帧重建离屏目标。
    target_size: HashMap<String, (u32, u32)>,
}

impl Scene3dGpu {
    pub fn try_new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Option<Self> {
        crate::renderer::wgpu_compat::try_gpu_init(
            device,
            "scene3d GPU pipeline failed; 3D leaves disabled",
            || Self::build(device, format),
        )
    }

    fn build(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jadix-scene3d-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene3d.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-scene3d-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-scene3d-pl"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("jadix-scene3d-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(CubeVert::desc())],
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
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jadix-scene3d-mvp"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-scene3d-ubg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let verts = cube_vertices();
        let vertex_count = verts.len() as u32;
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jadix-scene3d-cube"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            target_format: format,
            uniform_buf,
            uniform_bg,
            vertex_buf,
            vertex_count,
            depth: None,
            target_size: HashMap::new(),
        }
    }

    /// Returns the updated texture keys for local damage and the animation frame clock.
    pub fn flush_updated_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
    ) -> Vec<String> {
        let mut last: HashMap<String, crate::primitives::scene3d::Scene3dJob> = HashMap::new();
        for job in take_scene3d_jobs() {
            last.insert(job.key.clone(), job);
        }
        let mut updated = Vec::with_capacity(last.len());
        for job in last.into_values() {
            updated.push(job.key.clone());
            self.render_job(device, queue, textured, images, job);
        }
        if !updated.is_empty() {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        updated
    }

    fn render_job(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        job: crate::primitives::scene3d::Scene3dJob,
    ) {
        let w = job.width.max(2);
        let h = job.height.max(2);
        let stale = match self.target_size.get(&job.key) {
            Some(&(cw, ch)) => cw != w || ch != h,
            None => true,
        };
        if stale {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("jadix-scene3d-color"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.target_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = textured.create_texture_bind_group(device, &view);
            images.insert(
                job.key.clone(),
                GpuImageTexture {
                    _texture: texture,
                    bind_group,
                },
            );
            self.target_size.insert(job.key.clone(), (w, h));
        }

        let color_view = {
            let Some(gpu_img) = images.get(&job.key) else {
                return;
            };
            gpu_img
                ._texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        self.ensure_depth(device, w, h);
        let Some((_, _, depth_tex)) = self.depth.as_ref() else {
            return;
        };
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = w as f32 / h as f32;
        let mvp = mul4(
            perspective(job.fov_deg.to_radians(), aspect, 0.1, 20.0),
            mul4(
                translation(0.0, 0.0, -job.camera_z),
                mul4(rotate_y(job.yaw), rotate_x(job.pitch)),
            ),
        );
        let _ = (job.light, job.material);
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&mvp));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-scene3d-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-scene3d-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: job.bg[0] as f64,
                            g: job.bg[1] as f64,
                            b: job.bg[2] as f64,
                            a: job.bg[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
        }

        // SDF/MSDF text billboard: project the world plate through MVP → 4 corners + clip w
        // (perspective-correct UV). Affine AABB would squash the trapezoid.
        if let Some(tex_key) = job.text_tex_key.as_ref() {
            if let Some(label_img) = images.get(tex_key) {
                let hw = 0.55f32;
                let hh = 0.14f32;
                let y = 0.95f32;
                let world = [
                    [-hw, y, 0.0],
                    [hw, y, 0.0],
                    [hw, y + hh * 2.0, 0.0],
                    [-hw, y + hh * 2.0, 0.0],
                ];
                if let Some(warp) =
                    crate::renderer::project_billboard_warp(&mvp, world, w as f32, h as f32)
                {
                    let (warp_c01, warp_c23, warp_w) = warp.warp_attrs();
                    let flags =
                        if tex_key.starts_with(crate::utils::text_texture::TEXT_MSDF_TEX_PREFIX) {
                            4.0
                        } else {
                            3.0
                        };
                    let inst = TexturedInstanceInput {
                        rect_pos: [0.0, 0.0],
                        rect_size: [1.0, 1.0],
                        tint: [0.98, 0.99, 1.0, 1.0],
                        uv_offset: [0.0, 0.0],
                        uv_scale: [1.0, 1.0],
                        transform: crate::style::transform::MAT2D_IDENTITY,
                        flags,
                        clip_pos: [0.0, 0.0],
                        clip_size: [-1.0, -1.0],
                        warp_c01,
                        warp_c23,
                        warp_w,
                        rounded_clips: [[0.0; 4]; 4],
                    };
                    textured.update_resolution(queue, w, h);
                    let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("jadix-scene3d-label-inst"),
                        contents: bytemuck::cast_slice(&[inst]),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("jadix-scene3d-label"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &color_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        pass.set_pipeline(&textured.render_pipeline);
                        pass.set_bind_group(0, &label_img.bind_group, &[]);
                        pass.set_vertex_buffer(0, textured.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, inst_buf.slice(..));
                        pass.set_index_buffer(
                            textured.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        pass.draw_indexed(0..textured.num_indices, 0, 0..1);
                    }
                }
            }
        }

        queue.submit(Some(encoder.finish()));
    }

    fn ensure_depth(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        let reuse = self
            .depth
            .as_ref()
            .is_some_and(|(dw, dh, _)| *dw == w && *dh == h);
        if reuse {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-scene3d-depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth = Some((w, h, texture));
    }
}

fn cube_vertices() -> Vec<CubeVert> {
    let s = 0.55;
    let mut out = Vec::with_capacity(36);
    let mut face = |quad: [[f32; 3]; 4], color: [f32; 3]| {
        let [a, b, c, d] = quad;
        for p in [a, b, c, a, c, d] {
            out.push(CubeVert { pos: p, color });
        }
    };
    // +Z
    face(
        [[-s, -s, s], [s, -s, s], [s, s, s], [-s, s, s]],
        [0.35, 0.62, 0.96],
    );
    // -Z
    face(
        [[s, -s, -s], [-s, -s, -s], [-s, s, -s], [s, s, -s]],
        [0.64, 0.42, 0.90],
    );
    // +X
    face(
        [[s, -s, s], [s, -s, -s], [s, s, -s], [s, s, s]],
        [0.94, 0.35, 0.38],
    );
    // -X
    face(
        [[-s, -s, -s], [-s, -s, s], [-s, s, s], [-s, s, -s]],
        [0.96, 0.62, 0.28],
    );
    // +Y
    face(
        [[-s, s, s], [s, s, s], [s, s, -s], [-s, s, -s]],
        [0.30, 0.78, 0.55],
    );
    // -Y
    face(
        [[-s, -s, -s], [s, -s, -s], [s, -s, s], [-s, -s, s]],
        [0.92, 0.78, 0.28],
    );
    out
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn mul4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            r[col * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum();
        }
    }
    r
}

fn perspective(fovy: f32, aspect: f32, n: f32, f: f32) -> [f32; 16] {
    let t = (fovy * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = 1.0 / (aspect.max(0.01) * t);
    m[5] = 1.0 / t;
    m[10] = (f + n) / (n - f);
    m[11] = -1.0;
    m[14] = (2.0 * f * n) / (n - f);
    m
}

fn translation(x: f32, y: f32, z: f32) -> [f32; 16] {
    let mut m = identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

fn rotate_y(a: f32) -> [f32; 16] {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[2] = -s;
    m[8] = s;
    m[10] = c;
    m
}

fn rotate_x(a: f32) -> [f32; 16] {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[5] = c;
    m[6] = s;
    m[9] = -s;
    m[10] = c;
    m
}

#[cfg(test)]
#[path = "tests/scene3d.rs"]
mod tests;
