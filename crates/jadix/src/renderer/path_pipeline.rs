//! Product GPU 2D path: stroke mesh + stencil-cover fill → MSAA (or GPU SSAA)
//! → linear coverage resolve → straight-alpha `jadix_path:` / `jadix_canvas:` textures.
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

fn append_vertices(verts: &mut Vec<PathVert>, tris: &[PathTri]) -> std::ops::Range<u32> {
    let start = verts.len() as u32;
    for tri in tris {
        verts.extend_from_slice(&PathVert::from_tri(*tri));
    }
    start..verts.len() as u32
}

pub struct PathGpu {
    stroke: wgpu::RenderPipeline,
    stencil_eo: Option<wgpu::RenderPipeline>,
    stencil_nz: Option<wgpu::RenderPipeline>,
    cover: Option<wgpu::RenderPipeline>,
    gradient_cover: Option<wgpu::RenderPipeline>,
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    resolve: wgpu::RenderPipeline,
    resolve_bgl: wgpu::BindGroupLayout,
    bgl: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    sample_count: u32,
    msaa: Option<(u32, u32, wgpu::Texture)>,
    stencil: Option<(u32, u32, wgpu::Texture)>,
    coverage: Option<(u32, u32, wgpu::Texture)>,
    target_size: HashMap<String, (u32, u32)>,
}

impl PathGpu {
    /// Build the path PSO. Returns `None` (and logs) if WGSL / pipeline validation fails
    /// so the caller can fall back to CPU raster (`error_handling.md` E2).
    pub fn try_new(device: &wgpu::Device, sample_count: u32) -> Option<Self> {
        crate::renderer::wgpu_compat::try_gpu_init(
            device,
            "path GPU pipeline failed; falling back to CPU raster",
            || Self::build(device, sample_count),
        )
    }

    fn build(device: &wgpu::Device, sample_count: u32) -> Self {
        let sample_count = if sample_count >= 4 { 4 } else { 1 };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jadix-path-pipeline"),
            source: wgpu::ShaderSource::Wgsl(include_str!("path_pipeline.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-path-bgl"),
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
            label: Some("jadix-path-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let keep = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Always,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let stroke = make_pipeline(
            device,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::ALL,
            keep,
            keep,
            "jadix-path-stroke",
        );
        let resolve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-path-resolve-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let resolve_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-path-resolve-layout"),
            bind_group_layouts: &[Some(&bgl), Some(&resolve_bgl)],
            immediate_size: 0,
        });
        let resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("jadix-path-resolve"),
            layout: Some(&resolve_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_resolve"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_resolve"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jadix-path-globals"),
            size: std::mem::size_of::<PathGlobals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-path-ubg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        Self {
            stroke,
            stencil_eo: None,
            stencil_nz: None,
            cover: None,
            gradient_cover: None,
            shader,
            layout,
            resolve,
            resolve_bgl,
            bgl,
            uniform_buf,
            uniform_bg,
            sample_count,
            msaa: None,
            stencil: None,
            coverage: None,
            target_size: HashMap::new(),
        }
    }

    /// Stroke-only jobs must not pay for either winding rule or the fill cover.
    /// Keep the same PSOs, creating each only when a nonempty fill needs it.
    fn ensure_fill_pipelines(&mut self, device: &wgpu::Device, ops: &[PathGpuOp]) -> bool {
        let mut needed = [false; 3];
        for op in ops {
            if let PathGpuOp::Fill(fill) = op {
                if !fill.stencil.is_empty() {
                    needed[usize::from(!fill.even_odd)] = true;
                    needed[2] |= fill.grad.is_none();
                }
            }
        }
        let missing = (needed[0] && self.stencil_eo.is_none())
            || (needed[1] && self.stencil_nz.is_none())
            || (needed[2] && self.cover.is_none());
        if !missing {
            return true;
        }
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
        crate::renderer::wgpu_compat::try_gpu_init(
            device,
            "path fill pipeline failed; falling back to CPU raster",
            || {
                for (slot, required, front, back, write, label) in [
                    (
                        &mut self.stencil_eo,
                        needed[0],
                        invert,
                        invert,
                        wgpu::ColorWrites::empty(),
                        "jadix-path-stencil-eo",
                    ),
                    (
                        &mut self.stencil_nz,
                        needed[1],
                        dec_front,
                        inc_back,
                        wgpu::ColorWrites::empty(),
                        "jadix-path-stencil-nz",
                    ),
                    (
                        &mut self.cover,
                        needed[2],
                        cover_face,
                        cover_face,
                        wgpu::ColorWrites::ALL,
                        "jadix-path-cover-solid",
                    ),
                ] {
                    if required && slot.is_none() {
                        *slot = Some(make_pipeline(
                            device,
                            &self.layout,
                            &self.shader,
                            self.sample_count,
                            write,
                            front,
                            back,
                            label,
                        ));
                    }
                }
            },
        )
        .is_some()
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
        let canvas_rt = job.key.starts_with("jadix_canvas:");
        let always = job.force_redraw || canvas_rt;
        let w = job.width.max(2);
        let h = job.height.max(2);
        let reuse =
            self.target_size.get(&job.key) == Some(&(w, h)) && images.contains_key(&job.key);
        if reuse && !always {
            return false;
        }
        #[cfg(test)]
        let profile = std::env::var_os("JADIX_PATH_GPU_PROFILE").map(|_| std::time::Instant::now());
        let ops = job.ops.into_ops();
        #[cfg(test)]
        let geometry_ms = profile.map(|t| t.elapsed().as_secs_f64() * 1000.0);
        if ops.iter().all(PathGpuOp::is_empty) && !always {
            return false;
        }
        if !self.ensure_fill_pipelines(device, &ops) {
            crate::renderer::geometry2d::disable_gpu2d_runtime();
            return false;
        }
        // Most path leaves (spinners, progress and charts) only use vertex
        // colors. Compile the gradient program only for a real gradient fill.
        if self.gradient_cover.is_none()
            && ops.iter().any(|op| {
                matches!(op, PathGpuOp::Fill(fill) if !fill.stencil.is_empty() && fill.grad.is_some())
            })
        {
            let face = wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::NotEqual,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Replace,
            };
            self.gradient_cover = crate::renderer::wgpu_compat::try_gpu_init(
                device,
                "path gradient pipeline failed; falling back to CPU raster",
                || make_pipeline(
                    device, &self.layout, &self.shader, self.sample_count,
                    wgpu::ColorWrites::ALL, face, face, "jadix-path-cover",
                ),
            );
            if self.gradient_cover.is_none() {
                crate::renderer::geometry2d::disable_gpu2d_runtime();
                return false;
            }
        }
        #[cfg(test)]
        let pipeline_ms = profile.map(|t| t.elapsed().as_secs_f64() * 1000.0);
        self.ensure_target(device, textured, images, &job.key, w, h);
        let Some(gpu_img) = images.get(&job.key) else {
            return false;
        };
        let resolve_view = gpu_img
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Four coverage samples even when the backend cannot multisample stencil.
        // Geometry/gradient coordinates stay in job pixels; the viewport scales them.
        let limit = device.limits().max_texture_dimension_2d;
        let scale = if self.sample_count == 1 && w <= limit / 2 && h <= limit / 2 {
            2
        } else {
            1
        };
        let rw = w * scale;
        let rh = h * scale;
        self.ensure_coverage(device, rw, rh);
        let coverage_view = self
            .coverage
            .as_ref()
            .unwrap()
            .2
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.ensure_msaa(device, rw, rh);
        self.ensure_stencil(device, rw, rh);
        let msaa_view = self
            .msaa
            .as_ref()
            .map(|(_, _, t)| t.create_view(&wgpu::TextureViewDescriptor::default()));
        let stencil_view = self
            .stencil
            .as_ref()
            .map(|(_, _, t)| t.create_view(&wgpu::TextureViewDescriptor::default()));

        #[cfg(test)]
        let target_ms = profile.map(|t| t.elapsed().as_secs_f64() * 1000.0);
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&PathGlobals::resolution(w, h)),
        );

        // Upload this job's ordered geometry once. Draw ranges retain each
        // fill's stencil/cover pair and each mesh's original stacking order.
        let mut vertices = Vec::new();
        let ranges: Vec<_> = ops
            .iter()
            .map(|op| {
                let clip = op.clip().map(|c| c.map(|v| v * scale as f32));
                let scissor = scissor_rect(clip, rw, rh);
                if scissor.is_none() {
                    return (None, 0..0, 0..0);
                }
                let (geometry, cover) = match op {
                    PathGpuOp::Fill(fill) if !fill.stencil.is_empty() => (
                        append_vertices(&mut vertices, &fill.stencil),
                        append_vertices(&mut vertices, &fill.cover),
                    ),
                    PathGpuOp::Mesh(mesh) => (append_vertices(&mut vertices, &mesh.tris), 0..0),
                    _ => (0..0, 0..0),
                };
                (scissor, geometry, cover)
            })
            .collect();
        let vertex_buffer = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("jadix-path-job-vb"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let mut bufs: Vec<wgpu::Buffer> = Vec::new();
        let mut bgs: Vec<wgpu::BindGroup> = Vec::new();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-path-enc"),
        });
        {
            let color_view;
            let resolve;
            if let Some(msaa) = msaa_view.as_ref() {
                color_view = msaa;
                resolve = Some(&coverage_view);
            } else {
                color_view = &coverage_view;
                resolve = None;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-path-pass"),
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
            if let Some(buffer) = vertex_buffer.as_ref() {
                pass.set_vertex_buffer(0, buffer.slice(..));
            }
            for (op, (scissor, geometry, cover)) in ops.iter().zip(&ranges) {
                let Some([x, y, width, height]) = *scissor else {
                    continue;
                };
                pass.set_scissor_rect(x, y, width, height);
                match op {
                    PathGpuOp::Fill(fill) => {
                        if fill.stencil.is_empty() {
                            continue;
                        }
                        pass.set_pipeline(if fill.even_odd {
                            self.stencil_eo.as_ref().unwrap()
                        } else {
                            self.stencil_nz.as_ref().unwrap()
                        });
                        pass.set_bind_group(0, &self.uniform_bg, &[]);
                        pass.draw(geometry.clone(), 0..1);
                        let use_grad = fill.grad.is_some();
                        if use_grad {
                            let gbuf =
                                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("jadix-path-fill-ub"),
                                    contents: bytemuck::bytes_of(&PathGlobals::for_fill(
                                        w, h, fill,
                                    )),
                                    usage: wgpu::BufferUsages::UNIFORM,
                                });
                            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some("jadix-path-fill-bg"),
                                layout: &self.bgl,
                                entries: &[wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: gbuf.as_entire_binding(),
                                }],
                            });
                            bufs.push(gbuf);
                            bgs.push(bg);
                        }
                        pass.set_pipeline(if use_grad {
                            self.gradient_cover.as_ref().unwrap()
                        } else {
                            self.cover.as_ref().unwrap()
                        });
                        if use_grad {
                            pass.set_bind_group(0, bgs.last().unwrap(), &[]);
                        } else {
                            pass.set_bind_group(0, &self.uniform_bg, &[]);
                        }
                        pass.draw(cover.clone(), 0..1);
                    }
                    PathGpuOp::Mesh(mesh) => {
                        if mesh.tris.is_empty() {
                            continue;
                        }
                        pass.set_pipeline(&self.stroke);
                        pass.set_bind_group(0, &self.uniform_bg, &[]);
                        pass.draw(geometry.clone(), 0..1);
                    }
                }
            }
        }
        let coverage_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-path-resolve-bg"),
            layout: &self.resolve_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&coverage_view),
            }],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-path-coverage-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &resolve_view,
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
            pass.set_pipeline(&self.resolve);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &coverage_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        #[cfg(test)]
        let encode_ms = profile.map(|t| t.elapsed().as_secs_f64() * 1000.0);
        queue.submit(Some(encoder.finish()));
        #[cfg(test)]
        if let Some(t) = profile {
            let total = t.elapsed().as_secs_f64() * 1000.0;
            if total > 1.0 {
                eprintln!("path-gpu/{} {}x{} ops={} geometry={:.3} pipeline={:.3} target={:.3} encode={:.3} submit={:.3}",
                    job.key, w, h, ops.len(), geometry_ms.unwrap(),
                    pipeline_ms.unwrap()-geometry_ms.unwrap(), target_ms.unwrap()-pipeline_ms.unwrap(),
                    encode_ms.unwrap()-target_ms.unwrap(), total-encode_ms.unwrap());
            }
        }
        drop((bufs, bgs));
        if crate::primitives::path::take_path_rt_readback_request(&job.key) {
            if let Some(gpu_img) = images.get(&job.key) {
                let enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("jadix-path-readback"),
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
            label: Some("jadix-path-color"),
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
            label: Some("jadix-path-msaa"),
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

    fn ensure_coverage(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self
            .coverage
            .as_ref()
            .is_some_and(|(cw, ch, _)| *cw == w && *ch == h)
        {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-path-coverage"),
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
        self.coverage = Some((w, h, texture));
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
            label: Some("jadix-path-stencil"),
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
    // Meshes carry their color in vertices; only fill-cover triangles use
    // gradient uniforms. Stencil-only passes need no color computation at all.
    let fragment_entry = if write_mask.is_empty() {
        "fs_stencil"
    } else if label == "jadix-path-stroke" || label == "jadix-path-cover-solid" {
        "fs_solid"
    } else {
        "fs_main"
    };
    #[cfg(test)]
    let fragment_entry = if std::env::var_os("JADIX_PATH_GENERAL_SHADER").is_some() {
        "fs_main"
    } else {
        fragment_entry
    };
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
            entry_point: Some(fragment_entry),
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

fn scissor_rect(clip: Option<[f32; 4]>, tw: u32, th: u32) -> Option<[u32; 4]> {
    let (x, y, w, h) = match clip {
        None => (0u32, 0u32, tw, th),
        Some(c) => {
            if c[2] <= 0.0 || c[3] <= 0.0 {
                return None;
            }
            let x0 = c[0].floor().max(0.0) as u32;
            let y0 = c[1].floor().max(0.0) as u32;
            let x1 = (c[0] + c[2]).ceil().max(0.0) as u32;
            let y1 = (c[1] + c[3]).ceil().max(0.0) as u32;
            let x1 = x1.min(tw);
            let y1 = y1.min(th);
            if x0 >= x1 || y0 >= y1 {
                return None;
            }
            (x0, y0, x1 - x0, y1 - y0)
        }
    };
    if w == 0 || h == 0 || x >= tw || y >= th {
        return None;
    }
    Some([x, y, w.min(tw - x), h.min(th - y)])
}

/// 4× MSAA when color **and** depth-stencil formats support it; otherwise 1×.
pub fn path_msaa_sample_count(adapter: &wgpu::Adapter) -> u32 {
    let color = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba8UnormSrgb);
    let ds = adapter.get_texture_format_features(STENCIL_FORMAT);
    let x4 = wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4;
    if color
        .flags
        .contains(x4 | wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE)
        && ds.flags.contains(x4)
    {
        4
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod antialias;

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
