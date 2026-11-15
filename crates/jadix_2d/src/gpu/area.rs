//! GPU pixel area integration over tessellated fills, with existing brush evaluators.
use super::*;
use crate::{PathGpuImage, PathGpuPattern};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    bounds: [f32; 4],
    range: [u32; 4],
    color: [f32; 4],
    clip: [f32; 4],
    uv_x: [f32; 4],
    uv_y: [f32; 4],
    sampling_plan: [f32; 4],
}
impl Instance {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![0=>Float32x4,1=>Uint32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4,6=>Float32x4];
        wgpu::VertexBufferLayout {
            array_stride: 112,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRS,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    kind: usize,
    mask: u64,
    image: u64,
    linear: bool,
}
struct Batch {
    key: Key,
    range: std::ops::Range<u32>,
}
struct Table {
    texture: wgpu::Texture,
    bg: wgpu::BindGroup,
    capacity: u32,
}
pub(super) struct Area {
    pipelines: [Option<wgpu::RenderPipeline>; 4],
    shader: wgpu::ShaderModule,
    basic_shader: wgpu::ShaderModule,
    tier: [u8; 2],
    simple_shader: wgpu::ShaderModule,
    ewa_shader: wgpu::ShaderModule,
    geometry: Option<Table>,
    brushes: Option<Table>,
    instances: Option<wgpu::Buffer>,
    capacity: u64,
}
impl Area {
    pub(super) fn new(device: &wgpu::Device, _parent: &PathGpu) -> Self {
        let source = format!(
            "{}\n{}",
            sampling_shader_source(3),
            include_str!("area.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixel-area"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let basic_source = format!(
            "{}\n{}",
            sampling_shader_source(1),
            include_str!("area.wgsl")
        );
        let basic_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixel-area-basic"),
            source: wgpu::ShaderSource::Wgsl(basic_source.into()),
        });
        let simple_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixel-area-simple"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    sampling_shader_source(0),
                    include_str!("area.wgsl")
                )
                .into(),
            ),
        });
        let ewa_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixel-area-ewa"),
            source: wgpu::ShaderSource::Wgsl(
                format!(
                    "{}\n{}",
                    sampling_shader_source(2),
                    include_str!("area.wgsl")
                )
                .into(),
            ),
        });
        Self {
            ewa_shader,
            simple_shader,
            pipelines: std::array::from_fn(|_| None),
            shader,
            basic_shader,
            tier: [0; 2],
            geometry: None,
            brushes: None,
            instances: None,
            capacity: 0,
        }
    }
    pub(super) fn ensure_pipelines(
        &mut self,
        device: &wgpu::Device,
        parent: &PathGpu,
        ops: &[PathGpuOp],
    ) -> bool {
        for op in ops {
            let material = match op {
                PathGpuOp::Pattern(p) => Some((0, p.sampling)),
                PathGpuOp::Image(p) => Some((1, p.sampling)),
                _ => None,
            };
            if let Some((i, sampling)) = material {
                let tier = sampling_tier(sampling) | self.tier[i];
                if tier > self.tier[i] {
                    self.tier[i] = tier;
                    self.pipelines[i + 2] = None;
                }
            }
        }
        let mut needed = [false; 4];
        for op in ops {
            match op {
                PathGpuOp::Fill(f) => {
                    needed[usize::from(f.grad.is_some() || f.paint.is_some())] = true
                }
                PathGpuOp::Mesh(_) => needed[0] = true,
                PathGpuOp::Pattern(_) => needed[2] = true,
                PathGpuOp::Image(_) => needed[3] = true,
                PathGpuOp::Layer(_) => {}
            }
        }
        let jobs = (0..4)
            .filter(|i| needed[*i] && self.pipelines[*i].is_none())
            .collect::<Vec<_>>();
        let Some(pipelines) = jadix_renderer::wgpu_compat::try_gpu_init(
            device,
            "2D area material initialization",
            || {
                options::initialize(jobs.len(), parent.options.initialization_threads(), |job| {
                    let i = jobs[job];
                    let material = if i < 2 {
                        &parent.resolve_bgl
                    } else {
                        &parent.image_bgl
                    };
                    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("pixel-area"),
                        bind_group_layouts: &[
                            Some(&parent.bgl),
                            Some(&parent.resolve_bgl),
                            Some(material),
                            Some(&parent.resolve_bgl),
                        ],
                        immediate_size: 0,
                    });
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("pixel-area"),
                        layout: Some(&layout),
                        vertex: wgpu::VertexState {
                            module: if i >= 2 {
                                match self.tier[i - 2] {
                                    0 => &self.simple_shader,
                                    1 => &self.basic_shader,
                                    2 => &self.ewa_shader,
                                    _ => &self.shader,
                                }
                            } else {
                                &self.shader
                            },
                            entry_point: Some("vs_area"),
                            buffers: &[Some(Instance::layout())],
                            compilation_options: Default::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: if i >= 2 {
                                match self.tier[i - 2] {
                                    0 => &self.simple_shader,
                                    1 => &self.basic_shader,
                                    2 => &self.ewa_shader,
                                    _ => &self.shader,
                                }
                            } else {
                                &self.shader
                            },
                            entry_point: Some(
                                [
                                    "fs_area_solid",
                                    "fs_area_brush",
                                    "fs_area_pattern",
                                    "fs_area_image",
                                ][i],
                            ),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: parent.format,
                                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: Default::default(),
                        }),
                        primitive: Default::default(),
                        depth_stencil: None,
                        multisample: Default::default(),
                        multiview_mask: None,
                        cache: parent.options.pipeline_cache.as_ref(),
                    })
                })
            },
        ) else {
            return false;
        };
        for (i, pipeline) in jobs.into_iter().zip(pipelines) {
            self.pipelines[i] = Some(pipeline);
        }
        true
    }
    #[cfg(feature = "test-hooks")]
    pub(super) fn ewa_state(&self) -> [bool; 2] {
        self.tier.map(|tier| tier & 2 != 0)
    }
    #[cfg(feature = "test-hooks")]
    pub(super) fn pipeline_state(&self) -> [bool; 4] {
        std::array::from_fn(|i| self.pipelines[i].is_some())
    }
    fn table(
        slot: &mut Option<Table>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        data: &mut Vec<[f32; 4]>,
        stats: &mut RenderStats,
    ) -> bool {
        let rows = data.len().div_ceil(width as usize).max(1) as u32;
        if rows > device.limits().max_texture_dimension_2d {
            return false;
        }
        if slot.as_ref().is_none_or(|v| v.capacity < rows) {
            let capacity = rows
                .next_power_of_two()
                .min(device.limits().max_texture_dimension_2d);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("area-table"),
                size: wgpu::Extent3d {
                    width,
                    height: capacity,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("area-table"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                }],
            });
            *slot = Some(Table {
                texture,
                bg,
                capacity,
            });
            stats.texture_allocations += 1;
            stats.bind_group_allocations += 1;
        }
        data.resize(rows as usize * width as usize, [0.; 4]);
        queue.write_texture(
            slot.as_ref().unwrap().texture.as_image_copy(),
            bytemuck::cast_slice(data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 16),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width,
                height: rows,
                depth_or_array_layers: 1,
            },
        );
        stats.uploaded_bytes += (data.len() * 16) as u64;
        true
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Frame encoder, target and explicit parent resources"
    )]
    pub(super) fn render(
        &mut self,
        parent: &mut PathGpu,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        ops: &[PathGpuOp],
        premultiplied: bool,
    ) -> bool {
        let mut work = 0u64;
        let mut geometry = Vec::new();
        let mut instances = Vec::new();
        let mut batches: Vec<Batch> = Vec::new();
        let mut paints = Vec::new();
        let mut stops = Vec::new();
        for op in ops {
            let clip = op
                .clip()
                .unwrap_or([0., 0., size[0] as f32, size[1] as f32]);
            if clip.iter().any(|v| !v.is_finite()) {
                return false;
            }
            let clip = [
                clip[0].max(0.),
                clip[1].max(0.),
                (clip[0] + clip[2]).min(size[0] as f32),
                (clip[1] + clip[3]).min(size[1] as f32),
            ];
            let mut material = 0;
            let mut image = 0;
            let mut linear = false;
            let mut mapping = [1., 0., 0., 1., 0., 0.];
            let mut brush = 0;
            let triangles: &[PathTri];
            let owned;
            let mut image_uv = None;
            match op {
                PathGpuOp::Fill(f) => {
                    if !f.direct {
                        return false;
                    }
                    triangles = &f.stencil;
                    if f.grad.is_some() {
                        material = 1;
                        brush = paints.len() as u32;
                        let mut g = PathGlobals::for_fill(size[0], size[1], f);
                        if let Some(v) = &f.paint {
                            let offset = stops.len();
                            for (t, c, e) in v.stops.data() {
                                stops.push([*t, *e, 0., 0.]);
                                stops.push(*c);
                            }
                            g.stop_t01[0] = offset as f32;
                        }
                        paints.push((g, f.paint.is_some()));
                    }
                }
                PathGpuOp::Mesh(m) => {
                    triangles = &m.tris;
                }
                PathGpuOp::Pattern(PathGpuPattern {
                    fill,
                    image: im,
                    device_to_brush,
                    extend_x: _,
                    extend_y: _,
                    sampling,
                }) => {
                    if !fill.direct {
                        return false;
                    }
                    triangles = &fill.stencil;
                    material = 2;
                    image = im.0.id;
                    linear = *sampling == ImageSampling::Bilinear;
                    mapping = *device_to_brush;
                }
                PathGpuOp::Image(PathGpuImage {
                    image: im,
                    source_rect,
                    corners,
                    opacity: _,
                    sampling,
                    ..
                }) => {
                    material = 3;
                    image = im.0.id;
                    linear = *sampling == ImageSampling::Bilinear;
                    let [x, y, w, h] = source_rect.unwrap_or([0, 0, im.width(), im.height()]);
                    let iw = im.width() as f32;
                    let ih = im.height() as f32;
                    image_uv = Some([
                        [x as f32 / iw, y as f32 / ih],
                        [(x + w) as f32 / iw, y as f32 / ih],
                        [(x + w) as f32 / iw, (y + h) as f32 / ih],
                        [x as f32 / iw, (y + h) as f32 / ih],
                    ]);
                    owned = vec![
                        PathTri {
                            a: corners[0],
                            b: corners[1],
                            c: corners[2],
                            color: [1.; 4],
                        },
                        PathTri {
                            a: corners[0],
                            b: corners[2],
                            c: corners[3],
                            color: [1.; 4],
                        },
                    ];
                    triangles = &owned;
                }
                PathGpuOp::Layer(_) => return false,
            }
            if triangles.is_empty() {
                continue;
            }
            // Bound both fragment loops and the estimated whole-frame triangle/pixel work.
            if triangles.len() > 262_144 || geometry.len() + triangles.len() * 3 > 1_048_560 {
                return false;
            }
            let same_color = triangles.iter().all(|t| t.color == triangles[0].color);
            // Spatial lists widen the supported primitive domain without
            // compositing partial triangle coverage separately at tile borders.
            let portions = if same_color && image_uv.is_none() && triangles.len() > 256 {
                let Some(v) = tile_portions(
                    triangles,
                    clip,
                    1_048_560usize.saturating_sub(geometry.len()) / 3,
                ) else {
                    return false;
                };
                v
            } else {
                let chunks = if same_color { triangles.len() } else { 1 };
                (0..triangles.len())
                    .step_by(chunks)
                    .map(|first| {
                        (
                            (first..(first + chunks).min(triangles.len())).collect(),
                            [
                                clip[0].floor(),
                                clip[1].floor(),
                                clip[2].ceil(),
                                clip[3].ceil(),
                            ],
                        )
                    })
                    .collect()
            };
            for (indices, tile) in portions {
                let tris = indices.iter().map(|i| &triangles[*i]).collect::<Vec<_>>();
                if tris.len() > 4096 || geometry.len() + tris.len() * 3 > 1_048_560 {
                    return false;
                }
                let start = geometry.len() as u32;
                let mut bounds = [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ];
                for (i, t) in tris.iter().enumerate() {
                    for (j, p) in [t.a, t.b, t.c].into_iter().enumerate() {
                        if !p.0.is_finite()
                            || !p.1.is_finite()
                            || p.0.abs() > 1e7
                            || p.1.abs() > 1e7
                        {
                            return false;
                        }
                        bounds[0] = bounds[0].min(p.0);
                        bounds[1] = bounds[1].min(p.1);
                        bounds[2] = bounds[2].max(p.0);
                        bounds[3] = bounds[3].max(p.1);
                        let uv = image_uv.map_or([0.; 2], |v| v[[0, 1, 2, 0, 2, 3][i * 3 + j]]);
                        geometry.push([p.0, p.1, uv[0], uv[1]]);
                    }
                }
                bounds = [
                    bounds[0].max(clip[0]).floor().max(tile[0]),
                    bounds[1].max(clip[1]).floor().max(tile[1]),
                    bounds[2].min(clip[2]).ceil().min(tile[2]),
                    bounds[3].min(clip[3]).ceil().min(tile[3]),
                ];
                if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
                    continue;
                }
                work += (bounds[2] - bounds[0]) as u64
                    * (bounds[3] - bounds[1]) as u64
                    * tris.len() as u64;
                if work > 67_108_864 {
                    return false;
                }
                let mut color = tris[0].color;
                if let PathGpuOp::Pattern(p) = op {
                    color[0] = p.extend_x as u8 as f32;
                    color[1] = p.extend_y as u8 as f32;
                    color[2] = p.sampling as u8 as f32;
                }
                if let PathGpuOp::Image(p) = op {
                    color = [1., 1., 1., p.opacity];
                    let [x, y, w, h] =
                        p.source_rect
                            .unwrap_or([0, 0, p.image.width(), p.image.height()]);
                    mapping = [
                        x as f32,
                        y as f32,
                        (x + w) as f32,
                        (y + h) as f32,
                        p.sampling as u8 as f32,
                        0.,
                    ];
                }
                let sampling_plan = match op {
                    PathGpuOp::Image(p) => {
                        parent.clamp_sampling_plan(&p.image, image_sampling_plan(p))
                    }
                    PathGpuOp::Pattern(p) => {
                        let m = p.device_to_brush;
                        parent.clamp_sampling_plan(
                            &p.image,
                            crate::mipmap::footprint_plan([m[0], m[1]], [m[2], m[3]], p.sampling),
                        )
                    }
                    _ => [0., 0., 0., 1.],
                };
                let index = instances.len() as u32;
                instances.push(Instance {
                    bounds,
                    sampling_plan,
                    range: [start, tris.len() as u32, material as u32, brush],
                    color,
                    clip,
                    uv_x: if material == 3 {
                        [mapping[0], mapping[1], mapping[2], mapping[3]]
                    } else {
                        [mapping[0], mapping[2], mapping[4], 0.]
                    },
                    uv_y: if material == 3 {
                        [mapping[4], 0., 0., 0.]
                    } else {
                        [mapping[1], mapping[3], mapping[5], 0.]
                    },
                });
                let key = Key {
                    kind: material,
                    mask: op.mask().map_or(parent.white.0.id, |v| v.0.id),
                    image,
                    linear,
                };
                if let Some(last) = batches.last_mut().filter(|b| b.key == key) {
                    last.range.end = index + 1;
                } else {
                    batches.push(Batch {
                        key,
                        range: index..index + 1,
                    });
                }
            }
        }
        if !self.ensure_pipelines(device, parent, ops) {
            return false;
        }
        let triangle_count = geometry.len() / 3;
        let header_size = paints.len() * 16;
        let mut table = Vec::new();
        for (g, has_stops) in &mut paints {
            if *has_stops {
                g.stop_t01[0] += header_size as f32;
            }
            let row: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(g));
            table.extend_from_slice(row);
        }
        table.extend(stops);
        if !Self::table(
            &mut self.geometry,
            device,
            queue,
            &parent.resolve_bgl,
            256.min(device.limits().max_texture_dimension_2d),
            &mut geometry,
            &mut parent.stats,
        ) || !Self::table(
            &mut self.brushes,
            device,
            queue,
            &parent.resolve_bgl,
            16,
            &mut table,
            &mut parent.stats,
        ) {
            return false;
        }
        let bytes = bytemuck::cast_slice::<Instance, u8>(&instances);
        let needed = bytes.len() as u64;
        if needed > device.limits().max_buffer_size {
            return false;
        }
        if needed > self.capacity {
            self.capacity = needed
                .next_power_of_two()
                .min(device.limits().max_buffer_size);
            self.instances = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("area-instances"),
                size: self.capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            parent.stats.buffer_allocations += 1;
        }
        if needed > 0 {
            queue.write_buffer(self.instances.as_ref().unwrap(), 0, bytes);
        }
        parent.stats.uploaded_bytes += needed;
        let mut g = PathGlobals::resolution(size[0], size[1]);
        g._pad0[0] = if premultiplied { 1. } else { 0. };
        g._pad0[1] = if parent.quality == AaQuality::Aliased {
            1.
        } else {
            0.
        };
        queue.write_buffer(&parent.uniform_buf, 0, bytemuck::bytes_of(&g));
        parent.stats.uniform_uploads += 1;
        let intermediate = if premultiplied {
            None
        } else {
            parent.ensure_coverage(device, size[0], size[1]);
            Some(
                parent
                    .coverage
                    .as_ref()
                    .unwrap()
                    .2
                    .create_view(&Default::default()),
            )
        };
        let view = intermediate.as_ref().unwrap_or(target);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("analytic-area"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            if let Some(buffer) = &self.instances {
                pass.set_vertex_buffer(0, buffer.slice(..));
            }
            pass.set_bind_group(0, &parent.uniform_bg, &[0]);
            pass.set_bind_group(3, &self.geometry.as_ref().unwrap().bg, &[]);
            for batch in &batches {
                let key = batch.key;
                pass.set_pipeline(self.pipelines[key.kind].as_ref().unwrap());
                pass.set_bind_group(1, &parent.textures[&(key.mask, true)].mask_bg, &[]);
                let binding = if key.kind < 2 {
                    &self.brushes.as_ref().unwrap().bg
                } else {
                    let im = &parent.textures[&(key.image, false)];
                    if key.linear {
                        im.linear_bg.as_ref().unwrap()
                    } else {
                        im.nearest_bg.as_ref().unwrap()
                    }
                };
                pass.set_bind_group(2, binding, &[]);
                pass.draw(0..6, batch.range.clone());
                parent.stats.draw_calls += 1;
            }
        }
        if intermediate.is_some() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("area-export"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(&parent.resolve);
            pass.set_bind_group(0, &parent.uniform_bg, &[0]);
            pass.set_bind_group(1, parent.coverage_bg.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
            parent.stats.draw_calls += 1;
        }
        parent.stats.vertex_capacity_bytes += self.capacity;
        parent.stats.area_primitives += instances.len();
        parent.stats.area_triangles += triangle_count;
        parent.stats.cached_images += parent.textures.len();
        true
    }
}

// Conservative per-tile triangle lists, no geometry clipping or changed winding.
// At most 16x16 pixel fragments scan a list; original painter order is retained.
// CPU allocation/work is bounded by the existing geometry texel budget.
fn tile_portions(
    triangles: &[PathTri],
    clip: [f32; 4],
    budget: usize,
) -> Option<Vec<(Vec<usize>, [f32; 4])>> {
    let mut tiles = std::collections::BTreeMap::<(u32, u32), Vec<usize>>::new();
    let mut refs = 0usize;
    for (i, t) in triangles.iter().enumerate() {
        let p = [t.a, t.b, t.c];
        if p.iter()
            .any(|p| !p.0.is_finite() || !p.1.is_finite() || p.0.abs() > 1e7 || p.1.abs() > 1e7)
        {
            return None;
        }
        let lo = [
            p.iter()
                .map(|v| v.0)
                .fold(f32::INFINITY, f32::min)
                .max(clip[0])
                .max(0.),
            p.iter()
                .map(|v| v.1)
                .fold(f32::INFINITY, f32::min)
                .max(clip[1])
                .max(0.),
        ];
        let hi = [
            p.iter()
                .map(|v| v.0)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clip[2]),
            p.iter()
                .map(|v| v.1)
                .fold(f32::NEG_INFINITY, f32::max)
                .min(clip[3]),
        ];
        if hi[0] <= lo[0] || hi[1] <= lo[1] {
            continue;
        }
        let a = lo.map(|v| (v / 16.).floor() as u32);
        let b = hi.map(|v| (v / 16.).ceil() as u32);
        let count = (b[0] - a[0]) as usize * (b[1] - a[1]) as usize;
        refs = refs.checked_add(count)?;
        if refs > budget {
            return None;
        }
        for y in a[1]..b[1] {
            for x in a[0]..b[0] {
                let list = tiles.entry((x, y)).or_default();
                if list.len() >= 4096 {
                    return None;
                }
                list.push(i);
            }
        }
    }
    Some(
        tiles
            .into_iter()
            .map(|((x, y), indices)| {
                (
                    indices,
                    [
                        x as f32 * 16.,
                        y as f32 * 16.,
                        (x + 1) as f32 * 16.,
                        (y + 1) as f32 * 16.,
                    ],
                )
            })
            .collect(),
    )
}
