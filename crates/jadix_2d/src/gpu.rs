//! Product GPU 2D path: stroke mesh + stencil-cover fill → MSAA (or GPU SSAA)
//! → linear coverage resolve → straight-alpha `jadix_path:` / `jadix_canvas:` textures.
//!
//! Does **not** go through UI instance quads / `rounded_rect.wgsl`. CPU
//! [`crate::path::paint_path_premul`] stays the golden / fallback.

use crate::{Image2D, ImageSampling};
use bytemuck::{Pod, Zeroable};
use std::{collections::HashMap, sync::Weak};

use crate::path::{PathGpuFill, PathGpuOp, PathTri};
mod area;
mod estimate;
#[cfg(feature = "compute")]
pub mod low_level;
pub use estimate::SceneMemoryEstimate;
mod scene_renderer;
pub use scene_renderer::SceneRenderer2D;
mod options;
pub use options::RendererOptions;
mod mips;
mod sampled;
pub use sampled::PreparedSampledSceneGpu;
mod bounds;
mod external;
pub use external::{GpuImageAlpha, GpuImageColor, GpuImageError};
mod filters;
mod instances;
mod layers;
pub use instances::{GpuMesh, MeshColorSpace, MeshInstance, MeshUploadStats};

const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// Quality/cost tradeoff independent of native backend capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AaQuality {
    /// One f32 pixel-center triangle test; requires renderer sample_count=1.
    /// Pair with GpuCanvas::set_point_sampling for binary path clips.
    /// Shares Area triangle/work budgets and rejects stencil-only expert fills.
    Aliased,
    /// Per-primitive pixel area over non-overlapping tessellations. Brush colors
    /// use pixel centers; no MSAA/SSAA target. Expert stencil-only fills reject.
    Area,
    #[default]
    Balanced,
    /// 2x in each dimension, combined with 4x MSAA when available (16 samples).
    High,
    /// 4x in each dimension (64 samples with MSAA); intended for export/goldens.
    Reference,
}

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
    brush_x: [f32; 4],
    brush_y: [f32; 4],
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
            brush_x: [1., 0., 0., 0.],
            brush_y: [0., 1., 0., 0.],
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
            let m = grad.device_to_brush;
            g.brush_x = [m[0], m[2], m[4], 0.];
            g.brush_y = [m[1], m[3], m[5], 0.];
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

/// Per-submission resource and draw counters (excludes pipeline construction).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// CPU-mode spatial sample count; GPU execution leaves this zero.
    pub cpu_rasterized_pixels: u64,
    pub compute_dispatches: u32,
    /// Sum of allocated source domains used this frame, excluding compositor targets.
    pub layer_source_pixels: u64,
    pub layer_full_canvas_pixels: u64,
    pub area_primitives: usize,
    pub area_triangles: usize,
    pub buffer_allocations: u32,
    pub bind_group_allocations: u32,
    pub texture_allocations: u32,
    pub uploaded_bytes: u64,
    pub draw_calls: u32,
    pub operations: usize,
    /// Retained vertex upload buffer capacity; excludes textures and driver overhead.
    pub vertex_capacity_bytes: u64,
    pub index_capacity_bytes: u64,
    pub unique_vertices: usize,
    pub indices: usize,
    pub queue_submissions: u32,
    /// Retained brush-table/uniform capacity in 256-byte rows (headers plus stop data).
    pub gradient_slots: usize,
    /// Actual gradient headers (image domain headers excluded).
    pub gradient_records: usize,
    pub gradient_stops: usize,
    /// Shared material table bytes, including image sampling domains.
    pub gradient_table_bytes: u64,
    pub uniform_uploads: u32,
    pub image_uploads: u32,
    /// GPU conversion passes for registered texture revisions, excluding CPU uploads.
    pub external_image_conversions: u32,
    /// GPU-only downsample passes; zero for unchanged registered revisions.
    pub mip_generation_passes: u32,
    pub image_region_uploads: u32,
    pub image_uploaded_bytes: u64,
    pub cached_images: usize,
}

impl RenderStats {
    fn add(&mut self, other: Self) {
        self.cpu_rasterized_pixels += other.cpu_rasterized_pixels;
        self.compute_dispatches += other.compute_dispatches;
        self.layer_source_pixels += other.layer_source_pixels;
        self.layer_full_canvas_pixels += other.layer_full_canvas_pixels;
        self.area_primitives += other.area_primitives;
        self.area_triangles += other.area_triangles;
        self.buffer_allocations += other.buffer_allocations;
        self.bind_group_allocations += other.bind_group_allocations;
        self.texture_allocations += other.texture_allocations;
        self.uploaded_bytes += other.uploaded_bytes;
        self.draw_calls += other.draw_calls;
        self.vertex_capacity_bytes += other.vertex_capacity_bytes;
        self.index_capacity_bytes += other.index_capacity_bytes;
        self.unique_vertices += other.unique_vertices;
        self.indices += other.indices;
        self.queue_submissions += other.queue_submissions;
        self.gradient_slots += other.gradient_slots;
        self.gradient_records += other.gradient_records;
        self.gradient_stops += other.gradient_stops;
        self.gradient_table_bytes += other.gradient_table_bytes;
        self.uniform_uploads += other.uniform_uploads;
        self.image_uploads += other.image_uploads;
        self.external_image_conversions += other.external_image_conversions;
        self.mip_generation_passes += other.mip_generation_passes;
        self.image_region_uploads += other.image_region_uploads;
        self.image_uploaded_bytes += other.image_uploaded_bytes;
        self.cached_images += other.cached_images;
    }
}

pub struct PathGpu {
    options: RendererOptions,
    area: Option<area::Area>,
    tight_layers: bool,
    external: std::sync::Arc<external::ExternalImages>,
    external_import: Option<external::Import>,
    mip_generator: Option<mips::MipGenerator>,
    format: wgpu::TextureFormat,
    layers: Option<Box<layers::Layers>>,
    quality: AaQuality,
    stroke: wgpu::RenderPipeline,
    stencil_eo: Option<wgpu::RenderPipeline>,
    stencil_nz: Option<wgpu::RenderPipeline>,
    cover: Option<wgpu::RenderPipeline>,
    gradient_cover: Option<wgpu::RenderPipeline>,
    vector_cover: Option<wgpu::RenderPipeline>,
    direct_gradient: Option<wgpu::RenderPipeline>,
    brush_layout: wgpu::PipelineLayout,
    brushes: Option<(u32, wgpu::Texture, wgpu::BindGroup)>,
    image_pipeline: Option<wgpu::RenderPipeline>,
    pattern_pipeline: Option<wgpu::RenderPipeline>,
    pattern_cover: Option<wgpu::RenderPipeline>,
    pattern_tier: u8,
    image_tier: u8,
    simple_shader: wgpu::ShaderModule,
    ewa_shader: wgpu::ShaderModule,
    basic_shader: wgpu::ShaderModule,
    image_bgl: wgpu::BindGroupLayout,
    image_layout: wgpu::PipelineLayout,
    textures: HashMap<(u64, bool), CachedImage>,
    recycled_images: Vec<CachedImage>,
    white: Image2D,
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
    coverage_bg: Option<wgpu::BindGroup>,
    vertices: Vec<PathVert>,
    vertex_buffer: Option<wgpu::Buffer>,
    vertex_capacity: u64,
    indexed_vertices: Vec<PathVert>,
    indices: Vec<u32>,
    vertex_map: HashMap<[u32; 8], u32, foldhash::fast::RandomState>,
    index_buffer: Option<wgpu::Buffer>,
    index_capacity: u64,
    globals_data: Vec<u8>,
    globals_capacity: u64,
    globals_stride: u64,
    stats: RenderStats,
    force_general: bool,
}

impl PathGpu {
    /// Infer compact isolated source/filter domains. Opt-in; masks, source-creating
    /// effects and domain-dependent convolution retain the full canvas.
    pub fn set_tight_layer_bounds(&mut self, enabled: bool) {
        self.tight_layers = enabled;
    }
    pub fn set_aa_quality(&mut self, quality: AaQuality) {
        self.quality = quality;
        self.options.antialiasing = quality;
    }
    /// Inspect the last submission, including allocations on cold or resized frames.
    pub fn last_stats(&self) -> RenderStats {
        self.stats
    }

    /// Build the path PSO. Returns `None` (and logs) if WGSL / pipeline validation fails
    /// so the caller can fall back to CPU raster (`error_handling.md` E2).
    pub fn try_new(device: &wgpu::Device, sample_count: u32) -> Option<Self> {
        Self::try_new_with_format(device, sample_count, wgpu::TextureFormat::Rgba8UnormSrgb)
    }
    /// The target and every intermediate use this format. Rgba16Float retains
    /// extended linear-sRGB components; it does not configure an OS HDR swapchain.
    pub fn try_new_with_format(
        device: &wgpu::Device,
        sample_count: u32,
        format: wgpu::TextureFormat,
    ) -> Option<Self> {
        Self::try_new_with_options(device, sample_count, format, RendererOptions::default())
    }
    /// GPU-only construction. use_cpu is handled by SceneRenderer2D and is rejected
    /// here, rather than silently ignored. Cache and thread controls reach children.
    pub fn try_new_with_options(
        device: &wgpu::Device,
        sample_count: u32,
        format: wgpu::TextureFormat,
        options: RendererOptions,
    ) -> Option<Self> {
        if options.use_cpu {
            return None;
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Rgba16Float
        ) {
            return None;
        }
        jadix_renderer::wgpu_compat::try_gpu_init(
            device,
            "path GPU pipeline initialization",
            || Self::build(device, sample_count, format, options),
        )
    }
    pub fn target_format(&self) -> wgpu::TextureFormat {
        self.format
    }
    fn build(
        device: &wgpu::Device,
        sample_count: u32,
        format: wgpu::TextureFormat,
        options: RendererOptions,
    ) -> Self {
        let sample_count = if sample_count >= 4 { 4 } else { 1 };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("jadix-path-pipeline"),
            source: wgpu::ShaderSource::Wgsl(include_str!("path_pipeline.wgsl").into()),
        });
        let ewa_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path-ewa-sampling"),
            source: wgpu::ShaderSource::Wgsl(sampling_shader_source(2)),
        });
        let simple_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path-simple-sampling"),
            source: wgpu::ShaderSource::Wgsl(sampling_shader_source(0)),
        });
        let basic_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("2D-basic-sampling"),
            source: wgpu::ShaderSource::Wgsl(sampling_shader_source(1)),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-path-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<PathGlobals>() as u64
                    ),
                },
                count: None,
            }],
        });
        let resolve_bgl = texture_layout(device, "jadix-path-mask-or-resolve");
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-path-pl"),
            bind_group_layouts: &[Some(&bgl), Some(&resolve_bgl)],
            immediate_size: 0,
        });
        let image_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-2d-image-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let image_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-2d-image-pl"),
            bind_group_layouts: &[
                Some(&bgl),
                Some(&resolve_bgl),
                Some(&image_bgl),
                Some(&resolve_bgl),
            ],
            immediate_size: 0,
        });
        let brush_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-2d-brush-table-layout"),
            bind_group_layouts: &[Some(&bgl), Some(&resolve_bgl), Some(&resolve_bgl)],
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
            format,
            &layout,
            &shader,
            sample_count,
            wgpu::ColorWrites::ALL,
            keep,
            keep,
            "jadix-path-stroke",
            options.pipeline_cache.as_ref(),
        );
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: options.pipeline_cache.as_ref(),
        });
        let globals_stride = (std::mem::size_of::<PathGlobals>() as u64).div_ceil(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        )) * u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jadix-path-globals"),
            size: globals_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-path-ubg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<PathGlobals>() as u64),
                }),
            }],
        });
        Self {
            area: None,
            tight_layers: false,
            format,
            external: Default::default(),
            external_import: None,
            mip_generator: None,
            layers: None,
            quality: options.antialiasing,
            options,
            stroke,
            stencil_eo: None,
            stencil_nz: None,
            cover: None,
            gradient_cover: None,
            vector_cover: None,
            direct_gradient: None,
            brush_layout,
            brushes: None,
            image_pipeline: None,
            pattern_pipeline: None,
            pattern_cover: None,
            pattern_tier: 0,
            image_tier: 0,
            simple_shader,
            ewa_shader,
            basic_shader,
            image_bgl,
            image_layout,
            textures: HashMap::new(),
            recycled_images: Vec::new(),
            white: Image2D::from_rgba8(1, 1, &[255; 4]).unwrap(),
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
            coverage_bg: None,
            vertices: Vec::new(),
            vertex_buffer: None,
            vertex_capacity: 0,
            indexed_vertices: Vec::new(),
            indices: Vec::new(),
            vertex_map: HashMap::with_hasher(foldhash::fast::RandomState::default()),
            index_buffer: None,
            index_capacity: 0,
            globals_data: Vec::new(),
            globals_capacity: globals_stride,
            globals_stride,
            stats: RenderStats::default(),
            force_general: false,
        }
    }

    fn ensure_standard_pipelines(&mut self, device: &wgpu::Device, ops: &[PathGpuOp]) -> bool {
        for op in ops {
            match op {
                PathGpuOp::Pattern(p)
                    if sampling_tier(p.sampling) | self.pattern_tier != self.pattern_tier =>
                {
                    self.pattern_tier |= sampling_tier(p.sampling);
                    self.pattern_pipeline = None;
                    self.pattern_cover = None;
                }
                PathGpuOp::Image(p)
                    if sampling_tier(p.sampling) | self.image_tier != self.image_tier =>
                {
                    self.image_tier |= sampling_tier(p.sampling);
                    self.image_pipeline = None;
                }
                _ => {}
            }
        }
        let mut needed = [false; 9];
        for op in ops {
            if let Some(f) = match op {
                PathGpuOp::Fill(f) => Some(f),
                PathGpuOp::Pattern(p) => Some(&p.fill),
                _ => None,
            } {
                let general = !f.direct || self.force_general;
                if general && !f.stencil.is_empty() {
                    needed[usize::from(!f.even_odd)] = true;
                }
                if let PathGpuOp::Fill(_) = op {
                    needed[2] |= general && f.grad.is_none();
                    needed[3] |= general && f.grad.is_some() && f.paint.is_none();
                    needed[4] |= general && f.paint.is_some();
                    needed[5] |= !general && f.grad.is_some();
                } else {
                    needed[6] |= !general;
                    needed[7] |= general;
                }
            }
            needed[8] |= matches!(op, PathGpuOp::Image(_));
        }
        let slots = [
            &self.stencil_eo,
            &self.stencil_nz,
            &self.cover,
            &self.gradient_cover,
            &self.vector_cover,
            &self.direct_gradient,
            &self.pattern_pipeline,
            &self.pattern_cover,
            &self.image_pipeline,
        ];
        let jobs = (0..9)
            .filter(|i| needed[*i] && slots[*i].is_none())
            .collect::<Vec<_>>();
        let Some(pipelines) = jadix_renderer::wgpu_compat::try_gpu_init(
            device,
            "2D material pipeline initialization",
            || {
                options::initialize(jobs.len(), self.options.initialization_threads(), |i| {
                    let kind = jobs[i];
                    let keep = wgpu::StencilFaceState::IGNORE;
                    let cover = wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::NotEqual,
                        pass_op: wgpu::StencilOperation::Replace,
                        ..keep
                    };
                    let invert = wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Always,
                        pass_op: wgpu::StencilOperation::Invert,
                        ..keep
                    };
                    let inc = wgpu::StencilFaceState {
                        compare: wgpu::CompareFunction::Always,
                        pass_op: wgpu::StencilOperation::IncrementWrap,
                        ..keep
                    };
                    let dec = wgpu::StencilFaceState {
                        pass_op: wgpu::StencilOperation::DecrementWrap,
                        ..inc
                    };
                    let (front, back) = match kind {
                        0 => (invert, invert),
                        1 => (dec, inc),
                        2..=4 | 7 => (cover, cover),
                        _ => (keep, keep),
                    };
                    let layout = match kind {
                        4 | 5 => &self.brush_layout,
                        6..=8 => &self.image_layout,
                        _ => &self.layout,
                    };
                    let label = [
                        "jadix-path-stencil-eo",
                        "jadix-path-stencil-nz",
                        "jadix-path-cover-solid",
                        "jadix-path-cover",
                        "jadix-path-vector-cover",
                        "jadix-path-direct-gradient",
                        "jadix-path-pattern",
                        "jadix-path-pattern",
                        "jadix-path-image",
                    ][kind];
                    make_pipeline(
                        device,
                        self.format,
                        layout,
                        match kind {
                            6 | 7 => match self.pattern_tier {
                                0 => &self.simple_shader,
                                1 => &self.basic_shader,
                                2 => &self.ewa_shader,
                                _ => &self.shader,
                            },
                            8 => match self.image_tier {
                                0 => &self.simple_shader,
                                1 => &self.basic_shader,
                                2 => &self.ewa_shader,
                                _ => &self.shader,
                            },
                            _ => &self.shader,
                        },
                        self.sample_count,
                        if kind < 2 {
                            wgpu::ColorWrites::empty()
                        } else {
                            wgpu::ColorWrites::ALL
                        },
                        front,
                        back,
                        label,
                        self.options.pipeline_cache.as_ref(),
                    )
                })
            },
        ) else {
            return false;
        };
        let slots = [
            &mut self.stencil_eo,
            &mut self.stencil_nz,
            &mut self.cover,
            &mut self.gradient_cover,
            &mut self.vector_cover,
            &mut self.direct_gradient,
            &mut self.pattern_pipeline,
            &mut self.pattern_cover,
            &mut self.image_pipeline,
        ];
        for (i, pipeline) in jobs.into_iter().zip(pipelines) {
            *slots[i] = Some(pipeline);
        }
        true
    }
    /// Initialize only leaf materials used by these operations, with configured
    /// bounded native workers. Layers are initialized at encode with their sizes.
    pub fn warm_up(&mut self, device: &wgpu::Device, ops: &[PathGpuOp]) -> bool {
        if matches!(self.quality, AaQuality::Area | AaQuality::Aliased) {
            let mut area = self
                .area
                .take()
                .unwrap_or_else(|| area::Area::new(device, self));
            let result = area.ensure_pipelines(device, self, ops);
            self.area = Some(area);
            result
        } else {
            self.ensure_standard_pipelines(device, ops)
        }
    }
    /// Render prepared operations into a caller-owned RGBA8 sRGB texture view.
    /// The view must match `size` and support RENDER_ATTACHMENT.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolve_view: &wgpu::TextureView,
        size: [u32; 2],
        ops: &[PathGpuOp],
    ) -> bool {
        self.render_impl(device, queue, resolve_view, size, ops, false)
    }

    /// Render linear-light premultiplied color to the configured target format.
    /// A host compositor must use premultiplied blending. RGBA8 sRGB targets
    /// encode linear components; RGBA16Float targets retain extended linear RGB.
    /// At 4x MSAA this resolves directly into the caller's target, avoiding the
    /// straight-alpha export texture/pass. At 1x the SSAA resolve stays enabled.
    pub fn render_premultiplied(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        ops: &[PathGpuOp],
    ) -> bool {
        self.render_impl(device, queue, target, size, ops, true)
    }

    fn render_impl(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        size: [u32; 2],
        ops: &[PathGpuOp],
        premultiplied: bool,
    ) -> bool {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-2d-frame"),
        });
        let result = self.encode_impl(
            device,
            queue,
            &mut encoder,
            target,
            size,
            ops,
            premultiplied,
        );
        if result {
            queue.submit(Some(encoder.finish()));
            self.stats.queue_submissions += 1;
        }
        result
    }

    // Each child renderer encodes once per frame, retaining its own upload buffers.
    #[expect(
        clippy::too_many_arguments,
        reason = "Explicit frame encoder and GPU target parameters"
    )]
    fn encode_impl(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        resolve_view: &wgpu::TextureView,
        size: [u32; 2],
        ops: &[PathGpuOp],
        premultiplied: bool,
    ) -> bool {
        self.stats = RenderStats {
            operations: ops.len(),
            ..Default::default()
        };
        let [w, h] = size;
        if self.quality == AaQuality::Aliased && self.sample_count != 1 {
            return false;
        }
        if w == 0
            || h == 0
            || w > device.limits().max_texture_dimension_2d
            || h > device.limits().max_texture_dimension_2d
        {
            return false;
        }
        fn valid_depth(ops: &[PathGpuOp], remaining: usize) -> bool {
            ops.iter().all(|op| match op {
                PathGpuOp::Layer(l) => remaining > 0 && valid_depth(&l.ops, remaining - 1),
                _ => true,
            })
        }
        if !valid_depth(ops, 32) {
            return false;
        }
        // Upload immutable resources once, retaining only weak references to CPU owners.
        self.recycled_images.clear();
        let expired: Vec<_> = self
            .textures
            .iter()
            .filter(|(_, c)| c.owner.strong_count() == 0)
            .map(|(k, _)| *k)
            .collect();
        for key in expired {
            self.recycled_images
                .push(self.textures.remove(&key).unwrap());
        }
        if ops.iter().any(|op| matches!(op, PathGpuOp::Layer(_))) {
            let white = self.white.clone();
            if !self.prepare_image(device, queue, encoder, &white, true) {
                return false;
            }
            // Sole-group folding can expose nested masks to this compositor.
            // Prepare the same chain that Layers::render may unwrap, not the whole tree.
            let mut visible = ops;
            loop {
                for op in visible {
                    if let PathGpuOp::Layer(layer) = op {
                        if let Some(mask) = &layer.mask {
                            if mask.width() != w
                                || mask.height() != h
                                || !self.prepare_image(device, queue, encoder, mask, true)
                            {
                                return false;
                            }
                        }
                    }
                }
                if let [PathGpuOp::Layer(layer)] = visible {
                    if layer.compose == crate::CompositeMode::SourceOver
                        && layer.clip.is_none()
                        && layer.mask.is_none()
                        && layer.filters.is_empty()
                    {
                        visible = &layer.ops;
                        continue;
                    }
                }
                break;
            }
            let mut layers = match self.layers.take() {
                Some(layers) if layers.size() == size => layers,
                _ => {
                    self.stats.buffer_allocations += 1;
                    let Some(layers) = jadix_renderer::wgpu_compat::try_gpu_init(
                        device,
                        "2D layer compositor initialization failed",
                        || layers::Layers::new(device, size, self.format, &self.options),
                    ) else {
                        return false;
                    };
                    Box::new(layers)
                }
            };
            let result = layers.render(
                device,
                queue,
                encoder,
                resolve_view,
                ops,
                (
                    self.sample_count,
                    self.quality,
                    premultiplied,
                    self.tight_layers,
                ),
                &mut self.stats,
                &self.textures,
                self.white.0.id,
                &self.external,
            );
            self.layers = Some(layers);
            self.stats.cached_images += self.textures.len();
            return result;
        }
        if ops
            .iter()
            .any(|op| op.mask().is_some_and(|m| m.width() != w || m.height() != h))
        {
            return false;
        }
        let white = self.white.clone();
        self.prepare_image(device, queue, encoder, &white, true);
        for op in ops {
            if let Some(mask) = op.mask() {
                if !self.prepare_image(device, queue, encoder, mask, true) {
                    return false;
                }
            }
            if let PathGpuOp::Pattern(p) = op {
                if !self.prepare_image(device, queue, encoder, &p.image, false) {
                    return false;
                }
            }
            if let PathGpuOp::Image(image) = op {
                if image.source_rect.is_some_and(|[x, y, w, h]| {
                    w == 0
                        || h == 0
                        || x.checked_add(w).is_none_or(|v| v > image.image.width())
                        || y.checked_add(h).is_none_or(|v| v > image.image.height())
                }) {
                    return false;
                }
                if !self.prepare_image(device, queue, encoder, &image.image, false) {
                    return false;
                }
            }
        }
        if matches!(self.quality, AaQuality::Area | AaQuality::Aliased) {
            let mut area = match self.area.take() {
                Some(v) => v,
                None => {
                    let Some(v) = jadix_renderer::wgpu_compat::try_gpu_init(
                        device,
                        "analytic area pipeline",
                        || area::Area::new(device, self),
                    ) else {
                        return false;
                    };
                    v
                }
            };
            let result = area.render(
                self,
                device,
                queue,
                encoder,
                resolve_view,
                size,
                ops,
                premultiplied,
            );
            self.area = Some(area);
            return result;
        }
        if !self.ensure_standard_pipelines(device, ops) {
            return false;
        }
        // Four coverage samples even when the backend cannot multisample stencil.
        // Geometry/gradient coordinates stay in job pixels; the viewport scales them.
        let limit = device.limits().max_texture_dimension_2d;
        let wanted = match self.quality {
            AaQuality::Balanced => {
                if self.sample_count == 1 {
                    2
                } else {
                    1
                }
            }
            AaQuality::Aliased | AaQuality::Area => 1,
            AaQuality::High => 2,
            AaQuality::Reference => 4,
        };
        if w > limit / wanted || h > limit / wanted {
            return false;
        }
        let scale = wanted;
        let rw = w * scale;
        let rh = h * scale;
        let direct_output = premultiplied && self.sample_count > 1 && scale == 1;
        let coverage_view = if direct_output {
            None
        } else {
            self.ensure_coverage(device, rw, rh);
            Some(
                self.coverage
                    .as_ref()
                    .unwrap()
                    .2
                    .create_view(&Default::default()),
            )
        };
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

        let mut brush_data: Vec<PathGlobals> = Vec::new();
        let brush_indices: Vec<_> = ops
            .iter()
            .map(|op| {
                if let PathGpuOp::Fill(fill) = op {
                    if ((fill.direct && !self.force_general) || fill.paint.is_some())
                        && fill.grad.is_some()
                    {
                        let index = brush_data.len();
                        brush_data.push(PathGlobals::for_fill(w, h, fill));
                        return Some(index);
                    }
                }
                if let PathGpuOp::Pattern(pattern) = op {
                    if pattern.sampling as u8 >= 4 {
                        let m = pattern.device_to_brush;
                        let mut globals = PathGlobals::resolution(w, h);
                        globals.brush_x = self.clamp_sampling_plan(
                            &pattern.image,
                            crate::mipmap::footprint_plan(
                                [m[0], m[1]],
                                [m[2], m[3]],
                                pattern.sampling,
                            ),
                        );
                        let index = brush_data.len();
                        brush_data.push(globals);
                        return Some(index);
                    }
                }
                if let PathGpuOp::Image(image) = op {
                    if image.source_rect.is_some() || image.sampling as u8 >= 2 {
                        let mut globals = PathGlobals::resolution(w, h);
                        let [x, y, w, h] = image.source_rect.unwrap_or([
                            0,
                            0,
                            image.image.width(),
                            image.image.height(),
                        ]);
                        globals.bbox = [x as f32, y as f32, (x + w) as f32, (y + h) as f32];
                        globals.angle = image.sampling as u8 as f32;
                        globals.brush_x =
                            self.clamp_sampling_plan(&image.image, image_sampling_plan(image));
                        let index = brush_data.len();
                        brush_data.push(globals);
                        return Some(index);
                    }
                }
                None
            })
            .collect();
        if !brush_data.is_empty() {
            let mut table = vec![[0f32; 4]; brush_data.len() * 16];
            let mut stop_offsets = HashMap::new();
            for (op, index) in ops.iter().zip(&brush_indices) {
                if let (PathGpuOp::Fill(fill), Some(index)) = (op, index) {
                    if let Some(paint) = &fill.paint {
                        let offset =
                            *stop_offsets
                                .entry(paint.stops.identity())
                                .or_insert_with(|| {
                                    let offset = table.len();
                                    for (t, c, exponent) in paint.stops.data() {
                                        table.push([*t, *exponent, 0., 0.]);
                                        table.push(*c);
                                    }
                                    self.stats.gradient_stops += paint.stops.len();
                                    offset
                                });
                        brush_data[*index].stop_t01[0] = offset as f32;
                    }
                }
            }
            let count = table.len().div_ceil(16) as u32;
            table.resize(count as usize * 16, [0.; 4]);
            for (index, g) in brush_data.iter().enumerate() {
                let header: &[[f32; 4]] = bytemuck::cast_slice(std::slice::from_ref(g));
                table[index * 16..index * 16 + 16].copy_from_slice(header);
            }
            if count > device.limits().max_texture_dimension_2d {
                return false;
            }
            if self
                .brushes
                .as_ref()
                .is_none_or(|(capacity, _, _)| *capacity < count)
            {
                let capacity = count
                    .next_power_of_two()
                    .min(device.limits().max_texture_dimension_2d);
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("jadix-2d-brush-table"),
                    size: wgpu::Extent3d {
                        width: 16,
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
                let view = texture.create_view(&Default::default());
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("jadix-2d-brush-table-bg"),
                    layout: &self.resolve_bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    }],
                });
                self.brushes = Some((capacity, texture, bg));
                self.stats.texture_allocations += 1;
                self.stats.bind_group_allocations += 1;
            }
            queue.write_texture(
                self.brushes.as_ref().unwrap().1.as_image_copy(),
                bytemuck::cast_slice(&table),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(count),
                },
                wgpu::Extent3d {
                    width: 16,
                    height: count,
                    depth_or_array_layers: 1,
                },
            );
            self.stats.uploaded_bytes += (table.len() * 16) as u64;
            self.stats.gradient_records += ops
                .iter()
                .zip(&brush_indices)
                .filter(|(op, index)| matches!(op, PathGpuOp::Fill(_)) && index.is_some())
                .count();
            self.stats.gradient_table_bytes += (table.len() * 16) as u64;
        }
        // Upload this job's ordered geometry once. Draw ranges retain each
        // fill's stencil/cover pair and each mesh's original stacking order.
        self.vertices.clear();
        let vertices = &mut self.vertices;
        let ranges: Vec<_> = ops
            .iter()
            .enumerate()
            .map(|(op_index, op)| {
                let clip = op.clip().map(|c| c.map(|v| v * scale as f32));
                let scissor = scissor_rect(clip, rw, rh);
                if scissor.is_none() {
                    return (None, 0..0, 0..0);
                }
                let (geometry, cover) = match op {
                    PathGpuOp::Fill(fill) if !fill.stencil.is_empty() => (
                        append_vertices(vertices, &fill.stencil),
                        if fill.direct && !self.force_general {
                            0..0
                        } else {
                            append_vertices(vertices, &fill.cover)
                        },
                    ),
                    PathGpuOp::Mesh(mesh) => (append_vertices(vertices, &mesh.tris), 0..0),
                    PathGpuOp::Pattern(p) => {
                        let start = vertices.len();
                        let geometry = append_vertices(vertices, &p.fill.stencil);
                        let cover = if p.fill.direct && !self.force_general {
                            0..0
                        } else {
                            append_vertices(vertices, &p.fill.cover)
                        };
                        let m = p.device_to_brush;
                        for v in &mut vertices[start..] {
                            v.cov_pad = [
                                m[0] * v.pos[0] + m[2] * v.pos[1] + m[4],
                                m[1] * v.pos[0] + m[3] * v.pos[1] + m[5],
                            ];
                            v.color[0] = p.extend_x as u8 as f32;
                            v.color[1] = p.extend_y as u8 as f32;
                            v.color[2] = p.sampling as u8 as f32
                                + brush_indices[op_index].map_or(0., |i| i as f32 * 8.);
                        }
                        (geometry, cover)
                    }
                    PathGpuOp::Image(image) => {
                        let start = vertices.len() as u32;
                        let [x, y, w, h] = image.source_rect.unwrap_or([
                            0,
                            0,
                            image.image.width(),
                            image.image.height(),
                        ]);
                        let [u0, v0, u1, v1] = [
                            x as f32 / image.image.width() as f32,
                            y as f32 / image.image.height() as f32,
                            (x as f32 + w as f32) / image.image.width() as f32,
                            (y as f32 + h as f32) / image.image.height() as f32,
                        ];
                        let uv = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
                        for i in [0usize, 1, 2, 0, 2, 3] {
                            vertices.push(PathVert {
                                pos: [image.corners[i].0, image.corners[i].1],
                                cov_pad: uv[i],
                                color: [
                                    brush_indices[op_index].map_or(-1., |i| i as f32),
                                    1.0,
                                    1.0,
                                    image.opacity,
                                ],
                            });
                        }
                        (start..vertices.len() as u32, 0..0)
                    }
                    _ => (0..0, 0..0),
                };
                if let Some(brush) =
                    brush_indices[op_index].filter(|_| matches!(op, PathGpuOp::Fill(_)))
                {
                    for vertex in &mut vertices[geometry.start as usize..geometry.end as usize] {
                        vertex.cov_pad[1] = brush as f32;
                    }
                }
                (scissor, geometry, cover)
            })
            .collect();
        self.indexed_vertices.clear();
        self.indices.clear();
        self.vertex_map.clear();
        for vertex in vertices.iter() {
            let key = bytemuck::cast::<PathVert, [u32; 8]>(*vertex);
            let index = *self.vertex_map.entry(key).or_insert_with(|| {
                let index = self.indexed_vertices.len() as u32;
                self.indexed_vertices.push(*vertex);
                index
            });
            self.indices.push(index);
        }
        self.stats.unique_vertices = self.indexed_vertices.len();
        self.stats.indices = self.indices.len();
        let index_bytes = bytemuck::cast_slice::<u32, u8>(&self.indices);
        if index_bytes.len() as u64 > device.limits().max_buffer_size {
            return false;
        }
        if !index_bytes.is_empty() {
            let required = index_bytes.len() as u64;
            if required > self.index_capacity {
                self.index_capacity = required
                    .next_power_of_two()
                    .min(device.limits().max_buffer_size);
                self.index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("jadix-2d-reusable-indices"),
                    size: self.index_capacity,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.stats.buffer_allocations += 1;
            }
            queue.write_buffer(self.index_buffer.as_ref().unwrap(), 0, index_bytes);
            self.stats.uploaded_bytes += required;
        }
        let vertex_bytes = bytemuck::cast_slice::<PathVert, u8>(&self.indexed_vertices);
        if vertex_bytes.len() as u64 > device.limits().max_buffer_size {
            return false;
        }
        if !vertex_bytes.is_empty() {
            let required = vertex_bytes.len() as u64;
            if required > self.vertex_capacity {
                self.vertex_capacity = required
                    .checked_next_power_of_two()
                    .unwrap_or(required)
                    .min(device.limits().max_buffer_size);
                self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("jadix-path-reusable-vertices"),
                    size: self.vertex_capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
                self.stats.buffer_allocations += 1;
            }
            queue.write_buffer(self.vertex_buffer.as_ref().unwrap(), 0, vertex_bytes);
            self.stats.uploaded_bytes += required;
        }
        self.globals_data.clear();
        self.globals_data.resize(self.globals_stride as usize, 0);
        self.globals_data[..std::mem::size_of::<PathGlobals>()].copy_from_slice(
            bytemuck::bytes_of(&{
                let mut globals = PathGlobals::resolution(w, h);
                globals._pad0[0] = if premultiplied { 1.0 } else { 0.0 };
                globals
            }),
        );
        let mut gradient_indices = Vec::with_capacity(ops.len());
        for (op, brush_index) in ops.iter().zip(&brush_indices) {
            if let PathGpuOp::Fill(fill) = op {
                if fill.grad.is_some()
                    && !fill.stencil.is_empty()
                    && (!fill.direct || self.force_general)
                {
                    let offset = self.globals_data.len();
                    self.globals_data
                        .resize(offset + self.globals_stride as usize, 0);
                    self.globals_data[offset..offset + std::mem::size_of::<PathGlobals>()]
                        .copy_from_slice(bytemuck::bytes_of(
                            &brush_index.map_or_else(
                                || PathGlobals::for_fill(w, h, fill),
                                |i| brush_data[i],
                            ),
                        ));
                    gradient_indices.push(offset as u32);
                    continue;
                }
            }
            gradient_indices.push(0);
        }
        let required = self.globals_data.len() as u64;
        if required > device.limits().max_buffer_size || required > u32::MAX as u64 {
            return false;
        }
        if required > self.globals_capacity {
            self.globals_capacity = required
                .next_power_of_two()
                .min(device.limits().max_buffer_size);
            self.uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jadix-2d-packed-uniforms"),
                size: self.globals_capacity,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("jadix-2d-packed-uniform-bg"),
                layout: &self.bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform_buf,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<PathGlobals>() as u64),
                    }),
                }],
            });
            self.stats.buffer_allocations += 1;
            self.stats.bind_group_allocations += 1;
        }
        queue.write_buffer(&self.uniform_buf, 0, &self.globals_data);
        self.stats.uniform_uploads = 1;
        self.stats.uploaded_bytes += required;
        {
            let color_view;
            let resolve;
            if let Some(msaa) = msaa_view.as_ref() {
                color_view = msaa;
                resolve = Some(if direct_output {
                    resolve_view
                } else {
                    coverage_view.as_ref().unwrap()
                });
            } else {
                color_view = coverage_view.as_ref().unwrap();
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
            if let Some(buffer) = self.vertex_buffer.as_ref() {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.set_index_buffer(
                    self.index_buffer.as_ref().unwrap().slice(..),
                    wgpu::IndexFormat::Uint32,
                );
            }
            pass.set_bind_group(3, &self.textures[&(self.white.0.id, true)].mask_bg, &[]);
            let mut skip_to = 0;
            for (op_index, ((op, (scissor, geometry, cover)), gradient_index)) in
                ops.iter().zip(&ranges).zip(gradient_indices).enumerate()
            {
                if op_index < skip_to {
                    continue;
                }
                let Some([x, y, width, height]) = *scissor else {
                    continue;
                };
                pass.set_scissor_rect(x, y, width, height);
                let mask = op.mask().unwrap_or(&self.white);
                pass.set_bind_group(1, &self.textures[&(mask.0.id, true)].mask_bg, &[]);
                match op {
                    PathGpuOp::Fill(fill) => {
                        if fill.stencil.is_empty() {
                            continue;
                        }
                        if fill.direct && !self.force_general {
                            pass.set_pipeline(if fill.grad.is_some() {
                                self.direct_gradient.as_ref().unwrap()
                            } else {
                                &self.stroke
                            });
                            pass.set_bind_group(0, &self.uniform_bg, &[0]);
                            let mut end = geometry.end;
                            if fill.grad.is_some() {
                                pass.set_bind_group(2, &self.brushes.as_ref().unwrap().2, &[]);
                            }
                            skip_to = op_index + 1;
                            while skip_to < ops.len() {
                                let PathGpuOp::Fill(next) = &ops[skip_to] else {
                                    break;
                                };
                                if !next.direct
                                    || next.grad.is_some() != fill.grad.is_some()
                                    || next.mask != fill.mask
                                    || ranges[skip_to].0 != *scissor
                                {
                                    break;
                                }
                                end = ranges[skip_to].1.end;
                                skip_to += 1;
                            }
                            pass.draw_indexed(geometry.start..end, 0, 0..1);
                            self.stats.draw_calls += 1;
                            continue;
                        }
                        pass.set_pipeline(if fill.even_odd {
                            self.stencil_eo.as_ref().unwrap()
                        } else {
                            self.stencil_nz.as_ref().unwrap()
                        });
                        pass.set_bind_group(0, &self.uniform_bg, &[0]);
                        pass.draw_indexed(geometry.clone(), 0, 0..1);
                        self.stats.draw_calls += 1;
                        let use_grad = fill.grad.is_some();
                        if fill.paint.is_some() {
                            pass.set_bind_group(2, &self.brushes.as_ref().unwrap().2, &[]);
                        }
                        pass.set_pipeline(if fill.paint.is_some() {
                            self.vector_cover.as_ref().unwrap()
                        } else if use_grad {
                            self.gradient_cover.as_ref().unwrap()
                        } else {
                            self.cover.as_ref().unwrap()
                        });
                        if use_grad {
                            pass.set_bind_group(0, &self.uniform_bg, &[gradient_index]);
                        } else {
                            pass.set_bind_group(0, &self.uniform_bg, &[0]);
                        }
                        pass.draw_indexed(cover.clone(), 0, 0..1);
                        self.stats.draw_calls += 1;
                    }
                    PathGpuOp::Layer(_) => unreachable!("layers are dispatched before geometry"),
                    PathGpuOp::Mesh(mesh) => {
                        if mesh.tris.is_empty() {
                            continue;
                        }
                        pass.set_pipeline(&self.stroke);
                        pass.set_bind_group(0, &self.uniform_bg, &[0]);
                        let mut end = geometry.end;
                        skip_to = op_index + 1;
                        while skip_to < ops.len() {
                            let PathGpuOp::Mesh(next) = &ops[skip_to] else {
                                break;
                            };
                            if next.mask != mesh.mask || ranges[skip_to].0 != *scissor {
                                break;
                            }
                            end = ranges[skip_to].1.end;
                            skip_to += 1;
                        }
                        pass.draw_indexed(geometry.start..end, 0, 0..1);
                        self.stats.draw_calls += 1;
                    }
                    PathGpuOp::Pattern(p) => {
                        pass.set_bind_group(
                            3,
                            self.brushes
                                .as_ref()
                                .map_or(&self.textures[&(self.white.0.id, true)].mask_bg, |v| &v.2),
                            &[],
                        );
                        if p.fill.stencil.is_empty() {
                            continue;
                        }
                        pass.set_bind_group(0, &self.uniform_bg, &[0]);
                        pass.set_bind_group(
                            2,
                            self.textures[&(p.image.0.id, false)]
                                .nearest_bg
                                .as_ref()
                                .unwrap(),
                            &[],
                        );
                        if p.fill.direct && !self.force_general {
                            pass.set_pipeline(self.pattern_pipeline.as_ref().unwrap());
                            let mut end = geometry.end;
                            skip_to = op_index + 1;
                            while skip_to < ops.len() {
                                let PathGpuOp::Pattern(next) = &ops[skip_to] else {
                                    break;
                                };
                                if !next.fill.direct
                                    || next.image != p.image
                                    || next.fill.mask != p.fill.mask
                                    || ranges[skip_to].0 != *scissor
                                {
                                    break;
                                }
                                // Transform, sampling and per-axis extension are vertex data.
                                end = ranges[skip_to].1.end;
                                skip_to += 1;
                            }
                            pass.draw_indexed(geometry.start..end, 0, 0..1);
                            self.stats.draw_calls += 1;
                        } else {
                            pass.set_pipeline(if p.fill.even_odd {
                                self.stencil_eo.as_ref().unwrap()
                            } else {
                                self.stencil_nz.as_ref().unwrap()
                            });
                            pass.draw_indexed(geometry.clone(), 0, 0..1);
                            pass.set_pipeline(self.pattern_cover.as_ref().unwrap());
                            pass.draw_indexed(cover.clone(), 0, 0..1);
                            self.stats.draw_calls += 2;
                        }
                    }
                    PathGpuOp::Image(image) => {
                        pass.set_pipeline(self.image_pipeline.as_ref().unwrap());
                        pass.set_bind_group(0, &self.uniform_bg, &[0]);
                        pass.set_bind_group(
                            3,
                            self.brushes
                                .as_ref()
                                .map_or(&self.textures[&(self.white.0.id, true)].mask_bg, |v| &v.2),
                            &[],
                        );
                        let texture = &self.textures[&(image.image.0.id, false)];
                        pass.set_bind_group(
                            2,
                            if image.sampling == ImageSampling::Bilinear {
                                texture.linear_bg.as_ref().unwrap()
                            } else {
                                texture.nearest_bg.as_ref().unwrap()
                            },
                            &[],
                        );
                        let mut end = geometry.end;
                        skip_to = op_index + 1;
                        while skip_to < ops.len() {
                            let PathGpuOp::Image(next) = &ops[skip_to] else {
                                break;
                            };
                            if next.image != image.image
                                || next.sampling != image.sampling
                                || next.mask != image.mask
                                || ranges[skip_to].0 != *scissor
                            {
                                break;
                            }
                            end = ranges[skip_to].1.end;
                            skip_to += 1;
                        }
                        pass.draw_indexed(geometry.start..end, 0, 0..1);
                        self.stats.draw_calls += 1;
                    }
                }
            }
        }
        if !direct_output {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-path-coverage-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: resolve_view,
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
            pass.set_bind_group(0, &self.uniform_bg, &[0]);
            pass.set_bind_group(1, self.coverage_bg.as_ref().unwrap(), &[]);
            pass.draw(0..3, 0..1);
        }
        self.stats.draw_calls += u32::from(!direct_output);
        self.stats.vertex_capacity_bytes = self.vertex_capacity;
        self.stats.index_capacity_bytes = self.index_capacity;
        self.stats.gradient_slots = ((self.globals_capacity / self.globals_stride) as usize - 1)
            .max(self.brushes.as_ref().map_or(0, |v| v.0 as usize));
        self.stats.cached_images = self.textures.len();
        true
    }
    /// Release cached GPU bitmap resources. CPU images upload on their next use;
    /// registrations remain live and regenerate GPU conversions without readback.
    pub fn clear_image_cache(&mut self) {
        self.recycled_images.clear();
        self.textures.clear();
        self.layers = None;
    }

    fn prepare_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        image: &Image2D,
        mask: bool,
    ) -> bool {
        if let Some(result) = self.prepare_external(device, encoder, image, mask) {
            return result;
        }
        if self.textures.contains_key(&(image.0.id, mask)) {
            return true;
        }
        if image.width() > device.limits().max_texture_dimension_2d
            || image.height() > device.limits().max_texture_dimension_2d
        {
            return false;
        }
        let predecessor = image.0.update.and_then(|(id, _)| {
            self.recycled_images.iter().position(|c| {
                c.id == id
                    && c.mask == mask
                    && c._texture.mip_level_count()
                        == if mask { 1 } else { image.mip_level_count() }
                    && c._texture.format()
                        == if mask {
                            wgpu::TextureFormat::R8Unorm
                        } else if image.linear_rgba32f().is_some() || image.mip_level_count() > 1 {
                            wgpu::TextureFormat::Rgba16Float
                        } else {
                            wgpu::TextureFormat::Rgba8UnormSrgb
                        }
            })
        });
        let reused_index = predecessor.or_else(|| {
            self.recycled_images.iter().position(|c| {
                c.mask == mask
                    && c._texture.width() == image.width()
                    && c._texture.height() == image.height()
                    && c._texture.mip_level_count()
                        == if mask { 1 } else { image.mip_level_count() }
                    && c._texture.format()
                        == if mask {
                            wgpu::TextureFormat::R8Unorm
                        } else if image.linear_rgba32f().is_some() || image.mip_level_count() > 1 {
                            wgpu::TextureFormat::Rgba16Float
                        } else {
                            wgpu::TextureFormat::Rgba8UnormSrgb
                        }
            })
        });
        let reused = reused_index.map(|i| self.recycled_images.swap_remove(i));
        let format = if mask {
            wgpu::TextureFormat::R8Unorm
        } else if image.linear_rgba32f().is_some() || image.mip_level_count() > 1 {
            wgpu::TextureFormat::Rgba16Float
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        };
        let texture = reused.is_none().then(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("jadix-2d-image"),
                size: wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: if mask { 1 } else { image.mip_level_count() },
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });
        let texture_ref = reused
            .as_ref()
            .map(|c| &c._texture)
            .or(texture.as_ref())
            .unwrap();
        let alpha;
        let pixels = if mask {
            alpha = image
                .rgba8()
                .chunks_exact(4)
                .map(|p| p[3])
                .collect::<Vec<_>>();
            &alpha[..]
        } else {
            image.gpu_rgba()
        };
        let partial = image
            .0
            .update
            .filter(|_| image.mip_level_count() == 1)
            .filter(|(id, _)| reused.as_ref().is_some_and(|c| c.id == *id));
        let pixel_bytes = if mask {
            1
        } else if image.linear_rgba32f().is_some() || image.mip_level_count() > 1 {
            8
        } else {
            4
        };
        let mut upload = texture_ref.as_image_copy();
        let region_pixels;
        let (pixels, extent, row_bytes) = if let Some((_, [x, y, w, h])) = partial {
            region_pixels = (0..h as usize)
                .flat_map(|row| {
                    let at =
                        ((y as usize + row) * image.width() as usize + x as usize) * pixel_bytes;
                    pixels[at..at + w as usize * pixel_bytes].iter().copied()
                })
                .collect::<Vec<_>>();
            upload.origin = wgpu::Origin3d { x, y, z: 0 };
            self.stats.image_region_uploads += 1;
            (
                &region_pixels[..],
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                w * pixel_bytes as u32,
            )
        } else {
            (
                pixels,
                texture_ref.size(),
                image.width() * pixel_bytes as u32,
            )
        };
        queue.write_texture(
            upload,
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(extent.height),
            },
            extent,
        );
        if !mask {
            for level in 1..image.mip_level_count() {
                let [w, h] = image.mip_level_size(level).unwrap();
                let data = image.mip_gpu_bytes(level);
                let mut copy = texture_ref.as_image_copy();
                copy.mip_level = level;
                queue.write_texture(
                    copy,
                    data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 8),
                        rows_per_image: Some(h),
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
                self.stats.uploaded_bytes += data.len() as u64;
                self.stats.image_uploaded_bytes += data.len() as u64;
            }
        }
        self.stats.image_uploads += 1;
        self.stats.uploaded_bytes += pixels.len() as u64;
        self.stats.image_uploaded_bytes += pixels.len() as u64;
        if let Some(mut cached) = reused {
            cached.owner = std::sync::Arc::downgrade(&image.0);
            cached.id = image.0.id;
            self.textures.insert((image.0.id, mask), cached);
            return true;
        }
        let texture = texture.unwrap();
        let view = texture.create_view(&Default::default());
        let mask_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-2d-mask-bg"),
            layout: &self.resolve_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let bind = |filter| {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("jadix-2d-image-bg"),
                layout: &self.image_bgl,
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
            })
        };
        let nearest_bg = (!mask).then(|| bind(wgpu::FilterMode::Nearest));
        let linear_bg = (!mask).then(|| bind(wgpu::FilterMode::Linear));
        self.textures.insert(
            (image.0.id, mask),
            CachedImage {
                id: image.0.id,
                mask,
                external_revision: None,
                owner: std::sync::Arc::downgrade(&image.0),
                _texture: texture,
                mask_bg,
                nearest_bg,
                linear_bg,
            },
        );
        self.stats.texture_allocations += 1;
        self.stats.bind_group_allocations += if mask { 1 } else { 3 };
        true
    }

    fn clamp_sampling_plan(&self, image: &Image2D, mut plan: [f32; 4]) -> [f32; 4] {
        let levels = self
            .textures
            .get(&(image.0.id, false))
            .map_or(1, |c| c._texture.mip_level_count());
        plan[2] = plan[2].clamp(0., (levels - 1) as f32);
        plan
    }

    #[cfg(feature = "test-hooks")]
    pub fn use_general_cover_for_test(&mut self, device: &wgpu::Device) {
        self.force_general = true;
        let face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::NotEqual,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Replace,
        };
        self.cover = Some(make_pipeline(
            device,
            self.format,
            &self.layout,
            &self.shader,
            self.sample_count,
            wgpu::ColorWrites::ALL,
            face,
            face,
            "jadix-path-cover",
            self.options.pipeline_cache.as_ref(),
        ));
    }

    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn area_pipeline_state(&self) -> [bool; 4] {
        self.area
            .as_ref()
            .map_or([false; 4], |v| v.pipeline_state())
    }
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn sampling_pipeline_state(&self) -> [bool; 4] {
        let area = self.area.as_ref().map_or([false; 2], |a| a.ewa_state());
        [
            self.pattern_tier & 2 != 0,
            self.image_tier & 2 != 0,
            area[0],
            area[1],
        ]
    }
    #[doc(hidden)]
    pub fn pipeline_state(&self) -> [bool; 4] {
        [
            self.stencil_eo.is_some(),
            self.stencil_nz.is_some(),
            self.cover.is_some(),
            self.gradient_cover.is_some() || self.direct_gradient.is_some(),
        ]
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
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.stats.texture_allocations += 1;
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
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.coverage_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-path-resolve-bg"),
            layout: &self.resolve_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        }));
        self.stats.bind_group_allocations += 1;
        self.stats.texture_allocations += 1;
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
        self.stats.texture_allocations += 1;
        self.stencil = Some((w, h, texture));
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Pipeline state is supplied explicitly at each call site"
)]
fn make_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    sample_count: u32,
    write_mask: wgpu::ColorWrites,
    stencil_front: wgpu::StencilFaceState,
    stencil_back: wgpu::StencilFaceState,
    label: &str,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    // Meshes carry their color in vertices; only fill-cover triangles use
    // gradient uniforms. Stencil-only passes need no color computation at all.
    let fragment_entry = if write_mask.is_empty() {
        "fs_stencil"
    } else if label == "jadix-path-stroke" || label == "jadix-path-cover-solid" {
        "fs_solid"
    } else if label == "jadix-path-direct-gradient" {
        "fs_brush"
    } else if label == "jadix-path-vector-cover" {
        "fs_vector"
    } else if label == "jadix-path-pattern" {
        "fs_pattern"
    } else if label == "jadix-path-image" {
        "fs_image"
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
                format,
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
        cache,
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
#[path = "tests/gpu.rs"]
mod tests;

struct CachedImage {
    id: u64,
    owner: Weak<crate::image::ImageData>,
    _texture: wgpu::Texture,
    mask_bg: wgpu::BindGroup,
    nearest_bg: Option<wgpu::BindGroup>,
    linear_bg: Option<wgpu::BindGroup>,
    mask: bool,
    external_revision: Option<u64>,
}
fn texture_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

fn image_sampling_plan(image: &crate::PathGpuImage) -> [f32; 4] {
    // Atlas crops must be prefiltered independently via mipmapped_region().
    if image
        .source_rect
        .is_some_and(|r| r != [0, 0, image.image.width(), image.image.height()])
    {
        return [0., 0., 0., 1.];
    }
    let [a, b, _, d] = image.corners;
    let x = [b.0 - a.0, b.1 - a.1];
    let y = [d.0 - a.0, d.1 - a.1];
    let det = x[0] * y[1] - x[1] * y[0];
    if !det.is_finite() || det == 0. {
        return [0., 0., 0., 1.];
    }
    let w = image.image.width() as f32;
    let h = image.image.height() as f32;
    crate::mipmap::footprint_plan(
        [y[1] / det * w, -x[1] / det * h],
        [-y[0] / det * w, x[0] / det * h],
        image.sampling,
    )
}

// Basic image/pattern pipelines contain no EWA implementation at all. This avoids
// compiling that large kernel for ordinary images/font atlases on legacy DX12 FXC.
// A renderer upgrades a material once if a later draw actually requests EWA.
fn sampling_shader_source(tier: u8) -> std::borrow::Cow<'static, str> {
    let source = include_str!("path_pipeline.wgsl");
    if tier == 3 {
        return source.into();
    }
    let mut source = source.to_string();
    if tier == 0 || tier == 2 {
        let start = source.find("fn reconstruct_image(").unwrap();
        let end = source.find("@fragment fn fs_pattern(").unwrap();
        let body = &source[start..end];
        let cut = body.find("    var first=").unwrap();
        source = format!(
            "{}{}    return vec4(0.);\n}}\n{}",
            &source[..start],
            &body[..cut],
            &source[end..]
        );
    }
    if tier == 0 {
        let start = source.find("fn mip_bilinear(").unwrap();
        let end = start + source[start..].find("@fragment").unwrap();
        source.replace_range(start..end,"fn reconstruction_with_plan(at:vec2<f32>,mode:vec2<f32>,quality:f32,bounds:vec4<f32>,plan:vec4<f32>)->vec4<f32>{return reconstruct_image(at,mode,quality,bounds); }\n");
    } else if tier == 1 {
        let start = source.find("fn ewa_level(").unwrap();
        let end = source.find("fn reconstruction_with_plan(").unwrap();
        source.replace_range(start..end,"fn ewa_level(at:vec2<f32>,mode:vec2<f32>,bounds:vec4<f32>,plan:vec4<f32>,level:i32)->vec4<f32>{return mip_bilinear(at,mode,bounds,level);}\n");
    } else {
        let start = source.find("fn reconstruction_with_plan(").unwrap();
        let end = start + source[start..].find("@vertex").unwrap();
        source.replace_range(start..end,"fn reconstruction_with_plan(at:vec2<f32>,mode:vec2<f32>,quality:f32,bounds:vec4<f32>,plan:vec4<f32>)->vec4<f32>{if(quality<4.){return reconstruct_image(at,mode,quality,bounds);}let lod=max(plan.z,0.);let a=i32(floor(lod));let b=i32(ceil(lod));let value=ewa_level(at,mode,bounds,plan,a);if(a==b){return value;}return mix(value,ewa_level(at,mode,bounds,plan,b),fract(lod));}\n");
    }
    source.into()
}

fn sampling_tier(s: ImageSampling) -> u8 {
    match s {
        ImageSampling::Nearest | ImageSampling::Bilinear => 0,
        ImageSampling::Ewa => 2,
        _ => 1,
    }
}
