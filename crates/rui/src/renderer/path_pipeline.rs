//! Product GPU 2D path: stroke mesh + stencil-cover fill → 4× MSAA offscreen → `rui_path:`.
//!
//! Does **not** go through [`InstanceInput`] / `rounded_rect.wgsl`. CPU
//! [`crate::primitives::path::paint_path_premul`] stays the golden / fallback.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::app::image_registry::GpuImageTexture;
use crate::primitives::path::{take_path_jobs, PathGpuFill, PathGpuJob, PathGpuOp, PathTri};
use crate::renderer::TexturedQuadPipeline;

const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PathVert {
    pos: [f32; 2],
    cov_pad: [f32; 2],
    color: [f32; 4],
}

impl PathVert {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVert>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }

    fn from_tri(tri: PathTri) -> [Self; 3] {
        let v = |p: (f32, f32)| Self {
            pos: [p.0, p.1],
            cov_pad: [1.0, 0.0],
            color: tri.color,
        };
        [v(tri.a), v(tri.b), v(tri.c)]
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PathGlobals {
    resolution: [f32; 2],
    fill_kind: f32,
    angle: f32,
    bbox: [f32; 4],
    center: [f32; 2],
    radii: [f32; 2],
    stop_n: f32,
    /// Matches WGSL three `f32` pads (not `vec3` — that is 16-byte aligned → 240).
    _pad0: [f32; 3],
    stop_t01: [f32; 4],
    stop_t45: [f32; 4],
    c0: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
    c3: [f32; 4],
    c4: [f32; 4],
    c5: [f32; 4],
    c6: [f32; 4],
    c7: [f32; 4],
}

impl PathGlobals {
    fn resolution(w: u32, h: u32) -> Self {
        Self {
            resolution: [w as f32, h as f32],
            fill_kind: 0.0,
            angle: 0.0,
            bbox: [0.0, 0.0, 1.0, 1.0],
            center: [0.5, 0.5],
            radii: [1.0, 1.0],
            stop_n: 0.0,
            _pad0: [0.0; 3],
            stop_t01: [0.0; 4],
            stop_t45: [1.0; 4],
            c0: [0.0; 4],
            c1: [0.0; 4],
            c2: [0.0; 4],
            c3: [0.0; 4],
            c4: [0.0; 4],
            c5: [0.0; 4],
            c6: [0.0; 4],
            c7: [0.0; 4],
        }
    }

    fn for_fill(w: u32, h: u32, fill: &PathGpuFill) -> Self {
        let mut g = Self::resolution(w, h);
        if let Some(grad) = fill.grad {
            g.fill_kind = grad.kind as f32;
            g.angle = grad.angle;
            g.bbox = grad.bbox;
            g.center = grad.center;
            g.radii = grad.radii;
            g.stop_n = grad.stop_n as f32;
            g.stop_t01 = [
                grad.stop_t[0],
                grad.stop_t[1],
                grad.stop_t[2],
                grad.stop_t[3],
            ];
            g.stop_t45 = [
                grad.stop_t[4],
                grad.stop_t[5],
                grad.stop_t[6],
                grad.stop_t[7],
            ];
            g.c0 = grad.stop_c[0];
            g.c1 = grad.stop_c[1];
            g.c2 = grad.stop_c[2];
            g.c3 = grad.stop_c[3];
            g.c4 = grad.stop_c[4];
            g.c5 = grad.stop_c[5];
            g.c6 = grad.stop_c[6];
            g.c7 = grad.stop_c[7];
        }
        g
    }
}

fn tris_to_verts(tris: &[PathTri]) -> Vec<PathVert> {
    let mut verts = Vec::with_capacity(tris.len() * 3);
    for tri in tris {
        verts.extend_from_slice(&PathVert::from_tri(*tri));
    }
    verts
}

pub struct PathGpu {
    stroke: wgpu::RenderPipeline,
    stencil_eo: wgpu::RenderPipeline,
    stencil_nz: wgpu::RenderPipeline,
    cover: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    sample_count: u32,
    msaa: Option<(u32, u32, wgpu::Texture)>,
    stencil: Option<(u32, u32, wgpu::Texture)>,
    target_size: HashMap<String, (u32, u32)>,
}

impl PathGpu {
    pub fn new(device: &wgpu::Device, sample_count: u32) -> Self {
        let sample_count = if sample_count >= 4 { 4 } else { 1 };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rui-path-pipeline"),
            source: wgpu::ShaderSource::Wgsl(include_str!("path_pipeline.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rui-path-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<PathGlobals>() as u64
                    ),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rui-path-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let keep = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let invert = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Invert,
        };
        let inc_back = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::IncrementWrap,
        };
        let dec_front = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::DecrementWrap,
        };
        let cover_face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::NotEqual,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        let stroke = make_pipeline(
            device,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::ALL,
            keep,
            keep,
            "rui-path-stroke",
        );
        let stencil_eo = make_pipeline(
            device,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::empty(),
            invert,
            invert,
            "rui-path-stencil-eo",
        );
        let stencil_nz = make_pipeline(
            device,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::empty(),
            dec_front,
            inc_back,
            "rui-path-stencil-nz",
        );
        let cover = make_pipeline(
            device,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::ALL,
            cover_face,
            cover_face,
            "rui-path-cover",
        );
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rui-path-globals"),
            size: std::mem::size_of::<PathGlobals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rui-path-ubg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        Self {
            stroke,
            stencil_eo,
            stencil_nz,
            cover,
            bgl,
            uniform_buf,
            uniform_bg,
            sample_count,
            msaa: None,
            stencil: None,
            target_size: HashMap::new(),
        }
    }

    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
    ) -> bool {
        let mut last: HashMap<String, PathGpuJob> = HashMap::new();
        for job in take_path_jobs() {
            last.insert(job.key.clone(), job);
        }
        let mut any = false;
        for job in last.into_values() {
            if self.render_job(device, queue, textured, images, job) {
                any = true;
            }
        }
        if any {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        any
    }

    fn render_job(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        job: PathGpuJob,
    ) -> bool {
        let canvas_rt = job.key.starts_with("rui_canvas:");
        let always = job.force_redraw || canvas_rt;
        if job.ops.iter().all(PathGpuOp::is_empty) && !always {
            return false;
        }
        let w = job.width.max(2);
        let h = job.height.max(2);
        let reuse =
            self.target_size.get(&job.key) == Some(&(w, h)) && images.contains_key(&job.key);
        if reuse && !always {
            return false;
        }
        self.ensure_target(device, textured, images, &job.key, w, h);
        let Some(gpu_img) = images.get(&job.key) else {
            return false;
        };
        let resolve_view = gpu_img
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.ensure_msaa(device, w, h);
        self.ensure_stencil(device, w, h);
        let msaa_view = self
            .msaa
            .as_ref()
            .map(|(_, _, t)| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let stencil_view = self
            .stencil
            .as_ref()
            .map(|(_, _, t)| t.create_view(&wgpu::TextureViewDescriptor::default()));

        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&PathGlobals::resolution(w, h)),
        );

        let mut bufs: Vec<wgpu::Buffer> = Vec::new();
        let mut bgs: Vec<wgpu::BindGroup> = Vec::new();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rui-path-enc"),
        });
        {
            let color_view;
            let resolve;
            if let Some(msaa) = msaa_view.as_ref() {
                color_view = msaa;
                resolve = Some(&resolve_view);
            } else {
                color_view = &resolve_view;
                resolve = None;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-path-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: if self.sample_count > 1 {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: stencil_view.as_ref().map(|view| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        }),
                    }
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_stencil_reference(0);
            for op in &job.ops {
                if !apply_scissor(&mut pass, op.clip(), w, h) {
                    continue;
                }
                match op {
                    PathGpuOp::Fill(fill) => {
                        if fill.stencil.is_empty() {
                            continue;
                        }
                        let stencil_verts = tris_to_verts(&fill.stencil);
                        let sv = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rui-path-stencil-vb"),
                            contents: bytemuck::cast_slice(&stencil_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        pass.set_pipeline(if fill.even_odd {
                            &self.stencil_eo
                        } else {
                            &self.stencil_nz
                        });
                        pass.set_bind_group(0, &self.uniform_bg, &[]);
                        pass.set_vertex_buffer(0, sv.slice(..));
                        pass.draw(0..stencil_verts.len() as u32, 0..1);
                        bufs.push(sv);

                        let cover_verts = tris_to_verts(&fill.cover);
                        let cv = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rui-path-cover-vb"),
                            contents: bytemuck::cast_slice(&cover_verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        let use_grad = fill.grad.is_some();
                        if use_grad {
                            let gbuf =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("rui-path-fill-ub"),
                                    contents: bytemuck::bytes_of(&PathGlobals::for_fill(
                                        w, h, fill,
                                    )),
                                    usage: wgpu::BufferUsages::UNIFORM,
                                });
                            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("rui-path-fill-bg"),
                                layout: &self.bgl,
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: gbuf.as_entire_binding(),
                                }],
                            });
                            bufs.push(gbuf);
                            bgs.push(bg);
                        }
                        pass.set_pipeline(&self.cover);
                        if use_grad {
                            pass.set_bind_group(0, bgs.last().unwrap(), &[]);
                        } else {
                            pass.set_bind_group(0, &self.uniform_bg, &[]);
                        }
                        pass.set_vertex_buffer(0, cv.slice(..));
                        pass.draw(0..cover_verts.len() as u32, 0..1);
                        bufs.push(cv);
                    }
                    PathGpuOp::Mesh(mesh) => {
                        if mesh.tris.is_empty() {
                            continue;
                        }
                        let verts = tris_to_verts(&mesh.tris);
                        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("rui-path-mesh-vb"),
                            contents: bytemuck::cast_slice(&verts),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                        pass.set_pipeline(&self.stroke);
                        pass.set_bind_group(0, &self.uniform_bg, &[]);
                        pass.set_vertex_buffer(0, vb.slice(..));
                        pass.draw(0..verts.len() as u32, 0..1);
                        bufs.push(vb);
                    }
                }
            }
        }
        queue.submit(Some(encoder.finish()));
        drop((bufs, bgs));
        if crate::primitives::path::take_path_rt_readback_request(&job.key) {
            if let Some(gpu_img) = images.get(&job.key) {
                let enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rui-path-readback"),
                });
                if let Some(img) = crate::renderer::offscreen::readback_texture(
                    device,
                    queue,
                    &gpu_img._texture,
                    w,
                    h,
                    enc,
                ) {
                    crate::primitives::path::store_path_rt_readback(
                        job.key.clone(),
                        img.width,
                        img.height,
                        img.pixels,
                    );
                }
            }
        }
        true
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
            label: Some("rui-path-color"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
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

    fn ensure_msaa(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self.sample_count <= 1 {
            self.msaa = None;
            return;
        }
        let reuse = self
            .msaa
            .as_ref()
            .is_some_and(|(cw, ch, _)| *cw == w && *ch == h);
        if reuse {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rui-path-msaa"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.msaa = Some((w, h, texture));
    }

    fn ensure_stencil(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        let reuse = self
            .stencil
            .as_ref()
            .is_some_and(|(cw, ch, _)| *cw == w && *ch == h);
        if reuse {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rui-path-stencil"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: STENCIL_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.stencil = Some((w, h, texture));
    }
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    sample_count: u32,
    write_mask: wgpu::ColorWrites,
    stencil_front: wgpu::StencilFaceState,
    stencil_back: wgpu::StencilFaceState,
    label: &str,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(PathVert::desc())],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            front_face: wgpu::FrontFace::Ccw,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: STENCIL_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState {
                front: stencil_front,
                back: stencil_back,
                read_mask: 0xFF,
                write_mask: 0xFF,
            },
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

fn apply_scissor(
    pass: &mut wgpu::RenderPass<'_>,
    clip: Option<[f32; 4]>,
    tw: u32,
    th: u32,
) -> bool {
    let (x, y, w, h) = match clip {
        None => (0u32, 0u32, tw, th),
        Some(c) => {
            if c[2] <= 0.0 || c[3] <= 0.0 {
                return false;
            }
            let x0 = c[0].floor().max(0.0) as u32;
            let y0 = c[1].floor().max(0.0) as u32;
            let x1 = (c[0] + c[2]).ceil().max(0.0) as u32;
            let y1 = (c[1] + c[3]).ceil().max(0.0) as u32;
            let x1 = x1.min(tw);
            let y1 = y1.min(th);
            if x0 >= x1 || y0 >= y1 {
                return false;
            }
            (x0, y0, x1 - x0, y1 - y0)
        }
    };
    if w == 0 || h == 0 || x >= tw || y >= th {
        return false;
    }
    pass.set_scissor_rect(x, y, w.min(tw - x), h.min(th - y));
    true
}

/// 4× MSAA when color **and** depth-stencil formats support it; otherwise 1×.
pub fn path_msaa_sample_count(adapter: &wgpu::Adapter) -> u32 {
    let color = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8UnormSrgb);
    let ds = adapter.get_texture_format_features(STENCIL_FORMAT);
    let x4 = wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4;
    if color.flags.contains(x4) && ds.flags.contains(x4) {
        4
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_stride_is_32() {
        assert_eq!(std::mem::size_of::<PathVert>(), 32);
        assert_eq!(std::mem::offset_of!(PathVert, color), 16);
    }

    #[test]
    fn globals_stride_is_224() {
        assert_eq!(std::mem::size_of::<PathGlobals>(), 224);
        assert_eq!(std::mem::offset_of!(PathGlobals, stop_n), 48);
        assert_eq!(std::mem::offset_of!(PathGlobals, stop_t01), 64);
    }
}
