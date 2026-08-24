//! GPU CSS/SVG filter pass — executes [`FilterGraph`] primitives into textures.

use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::job::FilterJob;
use crate::app::image_registry::GpuImageTexture;
use crate::renderer::pipeline::VertexInput;
use crate::renderer::TexturedQuadPipeline;
use crate::style::svg_filter::{
    BlendMode, ChannelSelector, ColorMatrixType, CompositeOperator, FilterInput, FilterPrimitive,
    LightSource, MorphologyOperator, TransferFunction, TransferType,
};

/// Must match `filter_pass.wgsl` (std140-ish: vec2/vec4 align 8/16).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FilterUniforms {
    pub mode: f32,
    pub p0: f32,
    pub p1: f32,
    pub p2: f32,
    pub p3: f32,
    pub p4: f32,
    pub p5: f32,
    pub p6: f32,
    pub texel: [f32; 2],
    pub _pad0: [f32; 2],
    pub color: [f32; 4],
    pub m0: [f32; 4],
    pub m1: [f32; 4],
    pub m2: [f32; 4],
    pub m3: [f32; 4],
    pub m4: [f32; 4],
    pub k0: [f32; 4],
    pub k1: [f32; 4],
    pub k2: [f32; 4],
    pub k3: [f32; 4],
    pub k4: [f32; 4],
    pub k5: [f32; 4],
    pub k6: [f32; 4],
    pub t0: [f32; 4],
    pub t1: [f32; 4],
    pub t2: [f32; 4],
    pub t3: [f32; 4],
}

impl Default for FilterUniforms {
    fn default() -> Self {
        Self::zeroed()
    }
}

pub struct CssFilterPassGpu {
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    tex_bgl: wgpu::BindGroupLayout,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    target_size: HashMap<String, (u32, u32)>,
    /// Outputs that finished with a valid SourceGraphic input.
    completed_outputs: HashSet<String>,
    /// 1x1 transparent fallback when an input is missing.
    fallback_tex: Option<GpuImageTexture>,
    format: wgpu::TextureFormat,
}

impl CssFilterPassGpu {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rui-css-filter-pass"),
            source: wgpu::ShaderSource::Wgsl(include_str!("filter_pass.wgsl").into()),
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rui-css-filter-uniform-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<FilterUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rui-css-filter-tex-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rui-css-filter-pl"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rui-css-filter-pipeline"),
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
                // Must match capture / ping-pong RTs (`Rgba8UnormSrgb`), not the swapchain format.
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
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
            label: Some("rui-css-filter-uniforms"),
            size: std::mem::size_of::<FilterUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rui-css-filter-ubg"),
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
            label: Some("rui-css-filter-vb"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rui-css-filter-ib"),
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
            completed_outputs: HashSet::new(),
            fallback_tex: None,
            format,
        }
    }

    /// Apply filter graphs only (captures must already be in `images`).
    /// `ui_scale` must match the capture pass so ping-pong RTs match SourceGraphic size.
    pub fn flush_filter_jobs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        filters: &[FilterJob],
        ui_scale: f32,
    ) -> bool {
        if filters.is_empty() {
            return false;
        }
        self.ensure_fallback(device, textured);
        let mut any = false;
        let mut seen = std::collections::HashSet::new();
        for job in filters {
            if !seen.insert(job.output_key.clone()) {
                continue;
            }
            let scale = ui_scale.max(0.01);
            let w = (job.bounds[2] * scale).round().max(2.0) as u32;
            let h = (job.bounds[3] * scale).round().max(2.0) as u32;
            // Stable content keys: skip GPU when result already matches size (scroll path).
            if self.completed_outputs.contains(&job.output_key)
                && images.contains_key(&job.output_key)
                && self.target_size.get(&job.output_key) == Some(&(w, h))
            {
                continue;
            }
            if self.run_filter_job(device, queue, textured, images, job, ui_scale) {
                self.completed_outputs.insert(job.output_key.clone());
                any = true;
            }
        }
        if any {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        any
    }

    fn ensure_fallback(&mut self, device: &wgpu::Device, textured: &TexturedQuadPipeline) {
        if self.fallback_tex.is_some() {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rui-css-filter-fallback"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // transparent pixel written lazily via queue at first use — create view/bind only
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = textured.create_texture_bind_group(device, &view);
        self.fallback_tex = Some(GpuImageTexture {
            _texture: texture,
            bind_group,
        });
        let _ = self.format;
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
            label: Some("rui-css-filter-target"),
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

    fn run_filter_job(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        job: &FilterJob,
        ui_scale: f32,
    ) -> bool {
        if images.get(&job.input_key).is_none() {
            return false;
        }
        let scale = ui_scale.max(0.01);
        let w = (job.bounds[2] * scale).round().max(2.0) as u32;
        let h = (job.bounds[3] * scale).round().max(2.0) as u32;
        let texel = [1.0 / w as f32, 1.0 / h as f32];

        let ping = format!("rui_filter_tmp:{}:a", job.output_key);
        let pong = format!("rui_filter_tmp:{}:b", job.output_key);
        self.ensure_target(device, textured, images, &ping, w, h);
        self.ensure_target(device, textured, images, &pong, w, h);
        self.ensure_target(device, textured, images, &job.output_key, w, h);

        let mut named: HashMap<String, String> = HashMap::new();
        named.insert("SourceGraphic".into(), job.input_key.clone());
        // SourceAlpha uses same texture; shader modes that need alpha-only can sample .a
        named.insert("SourceAlpha".into(), job.input_key.clone());

        let mut last_key = job.input_key.clone();
        let mut use_ping = true;

        for (pi, prim) in job.graph.primitives.iter().enumerate() {
            let dst_key = if use_ping { ping.clone() } else { pong.clone() };
            use_ping = !use_ping;

            let ok = self.dispatch_primitive(
                device, queue, textured, images, &named, &last_key, &dst_key, prim, texel, w, h,
                pi, scale,
            );
            if !ok {
                continue;
            }
            last_key = dst_key.clone();
            let result = prim.result_name();
            if !result.is_empty() {
                // Persist named result into a stable slot so later prims can read it after ping-pong reuse.
                let persist = format!("rui_filter_rt:{}:{}", job.output_key, result);
                self.ensure_target(device, textured, images, &persist, w, h);
                if self.blit_copy(device, queue, textured, images, &dst_key, &persist, texel) {
                    named.insert(result.to_string(), persist);
                } else {
                    named.insert(result.to_string(), dst_key.clone());
                }
            }
        }

        self.blit_copy(
            device,
            queue,
            textured,
            images,
            &last_key,
            &job.output_key,
            texel,
        )
    }

    fn resolve_input(
        &self,
        named: &HashMap<String, String>,
        last_key: &str,
        input: &FilterInput,
    ) -> String {
        match input {
            FilterInput::SourceGraphic | FilterInput::SourceAlpha => named
                .get("SourceGraphic")
                .cloned()
                .unwrap_or_else(|| last_key.to_string()),
            FilterInput::BackgroundImage
            | FilterInput::BackgroundAlpha
            | FilterInput::FillPaint
            | FilterInput::StrokePaint => named
                .get("SourceGraphic")
                .cloned()
                .unwrap_or_else(|| last_key.to_string()),
            FilterInput::Result(name) if name.is_empty() => last_key.to_string(),
            FilterInput::Result(name) => named
                .get(name)
                .cloned()
                .unwrap_or_else(|| last_key.to_string()),
        }
    }

    fn dispatch_primitive(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        named: &HashMap<String, String>,
        last_key: &str,
        dst_key: &str,
        prim: &FilterPrimitive,
        texel: [f32; 2],
        _w: u32,
        _h: u32,
        _pi: usize,
        ui_scale: f32,
    ) -> bool {
        let mut u = FilterUniforms::default();
        u.texel = texel;

        match prim {
            FilterPrimitive::ColorMatrix {
                in1, ty, values, ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 1.0;
                match ty {
                    ColorMatrixType::Matrix => {
                        u.p0 = 0.0;
                        fill_matrix_from_values(&mut u, values);
                    }
                    ColorMatrixType::Saturate => {
                        u.p0 = 1.0;
                        u.p1 = values.first().copied().unwrap_or(1.0);
                    }
                    ColorMatrixType::HueRotate => {
                        u.p0 = 2.0;
                        u.p1 = values.first().copied().unwrap_or(0.0);
                    }
                    ColorMatrixType::LuminanceToAlpha => {
                        u.p0 = 3.0;
                    }
                }
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::GaussianBlur {
                in1, std_deviation, ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                // H then V into dst via temp swap: write H to dst, then V from dst to alternate then copy
                // Use two-step with intermediate in the other ping buffer already being dst — do H into dst, V needs another buffer.
                // Simpler: H into dst_key from src; then V from dst into a scratch then blit back.
                // Here dst_key is the chosen ping; use the other named keys for intermediate.
                let mid = format!("{dst_key}__blur_mid");
                // ensure mid exists by cloning size from dst via ensure on images
                if let Some(&(ww, hh)) = self.target_size.get(dst_key) {
                    self.ensure_target(device, textured, images, &mid, ww, hh);
                }
                u.mode = 2.0;
                u.p0 = (std_deviation.0 * ui_scale).max(0.01);
                if !self.draw(device, queue, textured, images, &src, &src, &mid, &u) {
                    return false;
                }
                u.mode = 3.0;
                u.p0 = (std_deviation.1 * ui_scale).max(0.01);
                self.draw(device, queue, textured, images, &mid, &mid, dst_key, &u)
            }
            FilterPrimitive::Offset { in1, dx, dy, .. } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 4.0;
                u.p0 = *dx * ui_scale;
                u.p1 = *dy * ui_scale;
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::Flood { color, opacity, .. } => {
                u.mode = 5.0;
                u.color = [
                    color[0],
                    color[1],
                    color[2],
                    (color[3] * *opacity).clamp(0.0, 1.0),
                ];
                let src = last_key;
                self.draw(device, queue, textured, images, src, src, dst_key, &u)
            }
            FilterPrimitive::Composite {
                in1,
                in2,
                operator,
                k1,
                k2,
                k3,
                k4,
                ..
            } => {
                let a = self.resolve_input(named, last_key, in1);
                let b = self.resolve_input(named, last_key, in2);
                u.mode = 6.0;
                u.p0 = composite_mode(*operator) as f32;
                u.p1 = *k1;
                u.p2 = *k2;
                u.p3 = *k3;
                u.p4 = *k4;
                self.draw(device, queue, textured, images, &a, &b, dst_key, &u)
            }
            FilterPrimitive::Blend { in1, in2, mode, .. } => {
                let a = self.resolve_input(named, last_key, in1);
                let b = self.resolve_input(named, last_key, in2);
                u.mode = 7.0;
                u.p0 = blend_mode(*mode) as f32;
                self.draw(device, queue, textured, images, &a, &b, dst_key, &u)
            }
            FilterPrimitive::Merge { nodes, .. } => {
                // Layered over from first to last into dst
                if nodes.is_empty() {
                    return self
                        .blit_copy(device, queue, textured, images, last_key, dst_key, texel);
                }
                let first = self.resolve_input(named, last_key, &nodes[0]);
                if nodes.len() == 1 {
                    return self.blit_copy(device, queue, textured, images, &first, dst_key, texel);
                }
                let mut acc = first;
                let merge_tmp = format!("{dst_key}__merge");
                if let Some(&(ww, hh)) = self.target_size.get(dst_key) {
                    self.ensure_target(device, textured, images, &merge_tmp, ww, hh);
                }
                for (i, n) in nodes.iter().enumerate().skip(1) {
                    let layer = self.resolve_input(named, last_key, n);
                    u = FilterUniforms::default();
                    u.texel = texel;
                    u.mode = 6.0;
                    u.p0 = 0.0; // over: layer over acc — SVG merge paints later on top
                                // composite_op(src=layer, dst=acc) with over
                    let out = if i + 1 == nodes.len() {
                        dst_key.to_string()
                    } else if i % 2 == 1 {
                        merge_tmp.clone()
                    } else {
                        format!("{merge_tmp}2")
                    };
                    if i % 2 == 0 && i + 1 != nodes.len() {
                        if let Some(&(ww, hh)) = self.target_size.get(dst_key) {
                            self.ensure_target(
                                device,
                                textured,
                                images,
                                &format!("{merge_tmp}2"),
                                ww,
                                hh,
                            );
                        }
                    }
                    if !self.draw(device, queue, textured, images, &layer, &acc, &out, &u) {
                        return false;
                    }
                    acc = out;
                }
                if acc != dst_key {
                    return self.blit_copy(device, queue, textured, images, &acc, dst_key, texel);
                }
                true
            }
            FilterPrimitive::Morphology {
                in1,
                operator,
                radius,
                ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = match operator {
                    MorphologyOperator::Dilate => 8.0,
                    MorphologyOperator::Erode => 9.0,
                };
                u.p0 = radius.0.max(0.0);
                u.p1 = radius.1.max(0.0);
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::ConvolveMatrix {
                in1,
                order_x,
                order_y,
                kernel,
                divisor,
                bias,
                target_x,
                target_y,
                preserve_alpha,
                ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 10.0;
                let ox = (*order_x).min(5);
                let oy = (*order_y).min(5);
                u.p0 = ox as f32;
                u.p1 = oy as f32;
                u.p2 = target_x.unwrap_or(ox / 2) as f32;
                u.p3 = target_y.unwrap_or(oy / 2) as f32;
                let div = divisor.unwrap_or_else(|| {
                    let s: f32 = kernel.iter().sum();
                    if s.abs() < 1e-6 {
                        1.0
                    } else {
                        s
                    }
                });
                u.p4 = div;
                u.p5 = *bias;
                u.p6 = if *preserve_alpha { 1.0 } else { 0.0 };
                fill_kernel(&mut u, kernel);
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::DisplacementMap {
                in1,
                in2,
                scale,
                x_channel,
                y_channel,
                ..
            } => {
                let a = self.resolve_input(named, last_key, in1);
                let b = self.resolve_input(named, last_key, in2);
                u.mode = 11.0;
                u.p0 = *scale;
                u.p1 = channel_sel(*x_channel) as f32;
                u.p2 = channel_sel(*y_channel) as f32;
                self.draw(device, queue, textured, images, &a, &b, dst_key, &u)
            }
            FilterPrimitive::Tile { in1, .. } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 12.0;
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::Turbulence {
                base_frequency,
                num_octaves,
                seed,
                fractal_noise,
                ..
            } => {
                u.mode = 13.0;
                u.p0 = base_frequency.0;
                u.p1 = base_frequency.1;
                u.p2 = (*num_octaves).max(1) as f32;
                u.p3 = *seed;
                u.p4 = if *fractal_noise { 1.0 } else { 0.0 };
                self.draw(
                    device, queue, textured, images, last_key, last_key, dst_key, &u,
                )
            }
            FilterPrimitive::ComponentTransfer {
                in1,
                func_r,
                func_g,
                func_b,
                func_a,
                ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 14.0;
                u.t0 = [
                    transfer_ty(func_r),
                    transfer_ty(func_g),
                    transfer_ty(func_b),
                    transfer_ty(func_a),
                ];
                u.t1 = transfer_params(func_r);
                u.t2 = transfer_params(func_g);
                u.t3 = transfer_params(func_b);
                let ap = transfer_params(func_a);
                u.p0 = ap[0];
                u.p1 = ap[1];
                u.p2 = ap[2];
                u.p3 = ap[3];
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::DiffuseLighting {
                in1,
                surface_scale,
                diffuse_constant,
                lighting_color,
                light,
                ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 15.0;
                u.p0 = *surface_scale;
                let (lx, ly, lz) = light_dir(light);
                u.p1 = lx;
                u.p2 = ly;
                u.p3 = lz;
                u.p4 = *diffuse_constant;
                u.color = *lighting_color;
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::SpecularLighting {
                in1,
                surface_scale,
                specular_constant,
                specular_exponent,
                lighting_color,
                light,
                ..
            } => {
                let src = self.resolve_input(named, last_key, in1);
                u.mode = 16.0;
                u.p0 = *surface_scale;
                let (lx, ly, lz) = light_dir(light);
                u.p1 = lx;
                u.p2 = ly;
                u.p3 = lz;
                u.p4 = *specular_constant;
                u.p5 = *specular_exponent;
                u.color = *lighting_color;
                self.draw(device, queue, textured, images, &src, &src, dst_key, &u)
            }
            FilterPrimitive::Image { href, .. } => {
                u.mode = 17.0;
                let has = images.contains_key(href);
                u.p0 = if has { 1.0 } else { 0.0 };
                let b_key = if has { href.as_str() } else { last_key };
                self.draw(
                    device, queue, textured, images, last_key, b_key, dst_key, &u,
                )
            }
        }
    }

    fn blit_copy(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        src_key: &str,
        dst_key: &str,
        texel: [f32; 2],
    ) -> bool {
        if src_key == dst_key {
            return true;
        }
        let mut u = FilterUniforms::default();
        u.mode = 0.0;
        u.texel = texel;
        self.draw(
            device, queue, textured, images, src_key, src_key, dst_key, &u,
        )
    }

    fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &HashMap<String, GpuImageTexture>,
        src_a: &str,
        src_b: &str,
        dst_key: &str,
        uniforms: &FilterUniforms,
    ) -> bool {
        let Some(dst) = images.get(dst_key) else {
            return false;
        };
        let fallback = self.fallback_tex.as_ref();
        let tex_a = images.get(src_a).or(fallback).expect("fallback must exist");
        let tex_b = images.get(src_b).or(fallback).expect("fallback must exist");

        let view_a = tex_a
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let view_b = tex_b
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let dst_view = dst
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rui-css-filter-tex-bg"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view_a),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&textured.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view_b),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&textured.sampler),
                },
            ],
        });

        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(uniforms));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rui-css-filter-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-css-filter-pass"),
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

fn fill_matrix_from_values(u: &mut FilterUniforms, values: &[f32]) {
    let mut v = [
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        1.0, 0.0,
    ];
    for (i, x) in values.iter().take(20).enumerate() {
        v[i] = *x;
    }
    // SVG matrix is 20 values in row-major 4x5
    u.m0 = [v[0], v[1], v[2], v[3]];
    u.m4[0] = v[4];
    u.m1 = [v[5], v[6], v[7], v[8]];
    u.m4[1] = v[9];
    u.m2 = [v[10], v[11], v[12], v[13]];
    u.m4[2] = v[14];
    u.m3 = [v[15], v[16], v[17], v[18]];
    u.m4[3] = v[19];
}

fn fill_kernel(u: &mut FilterUniforms, kernel: &[f32]) {
    let mut flat = [0.0f32; 25];
    for (i, k) in kernel.iter().take(25).enumerate() {
        flat[i] = *k;
    }
    u.k0 = [flat[0], flat[1], flat[2], flat[3]];
    u.k1 = [flat[4], flat[5], flat[6], flat[7]];
    u.k2 = [flat[8], flat[9], flat[10], flat[11]];
    u.k3 = [flat[12], flat[13], flat[14], flat[15]];
    u.k4 = [flat[16], flat[17], flat[18], flat[19]];
    u.k5 = [flat[20], flat[21], flat[22], flat[23]];
    u.k6 = [flat[24], 0.0, 0.0, 0.0];
}

fn composite_mode(op: CompositeOperator) -> u32 {
    match op {
        CompositeOperator::Over => 0,
        CompositeOperator::In => 1,
        CompositeOperator::Out => 2,
        CompositeOperator::Atop => 3,
        CompositeOperator::Xor => 4,
        CompositeOperator::Lighter => 5,
        CompositeOperator::Arithmetic => 6,
    }
}

fn blend_mode(mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Darken => 3,
        BlendMode::Lighten => 4,
        BlendMode::Overlay | BlendMode::HardLight | BlendMode::SoftLight => 5,
        BlendMode::Difference => 6,
        BlendMode::Exclusion => 7,
        _ => 1, // approximate remaining with multiply
    }
}

fn channel_sel(c: ChannelSelector) -> u32 {
    match c {
        ChannelSelector::R => 0,
        ChannelSelector::G => 1,
        ChannelSelector::B => 2,
        ChannelSelector::A => 3,
    }
}

fn transfer_ty(f: &TransferFunction) -> f32 {
    match f.ty {
        TransferType::Identity => 0.0,
        TransferType::Table => 1.0,
        TransferType::Discrete => 2.0,
        TransferType::Linear => 3.0,
        TransferType::Gamma => 4.0,
    }
}

fn transfer_params(f: &TransferFunction) -> [f32; 4] {
    match f.ty {
        TransferType::Table | TransferType::Discrete => {
            let a = f.table_values.first().copied().unwrap_or(0.0);
            let b = f.table_values.get(1).copied().unwrap_or(1.0);
            [a, b, 0.0, 0.0]
        }
        TransferType::Linear => [f.slope, f.intercept, 0.0, 0.0],
        TransferType::Gamma => [f.offset, 0.0, f.amplitude, f.exponent],
        TransferType::Identity => [1.0, 0.0, 1.0, 1.0],
    }
}

fn light_dir(light: &LightSource) -> (f32, f32, f32) {
    match light {
        LightSource::Distant { azimuth, elevation } => {
            let az = azimuth.to_radians();
            let el = elevation.to_radians();
            (az.cos() * el.cos(), az.sin() * el.cos(), el.sin())
        }
        LightSource::Point { x, y, z } | LightSource::Spot { x, y, z, .. } => (*x, *y, *z),
    }
}
