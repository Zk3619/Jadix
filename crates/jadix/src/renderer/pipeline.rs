use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub resolution: [f32; 2],
    pub premultiply: f32,
    /// Shader clip-enable flag; zero for the default unclipped bind group.
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
    pub radii: [f32; 4], // CSS border-radius: TL, TR, BR, BL
    pub border_width: f32,
    pub shadow_color: [f32; 4],
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_inset: f32,
    pub backdrop_fade: f32,
    /// 2D affine transform `[a, b, c, d, tx, ty]`.
    /// Identity = `[1, 0, 0, 1, 0, 0]`.
    pub transform: [f32; 6],
    /// Overflow clip in `rect_pos` space. FS tests after undoing the instance affine.
    /// When `clip_size[0] < 0.0`, clipping is disabled.
    /// Dest-reset / node-present scissors use `NodePaint.overflow_clip` (paint AABB).
    pub clip_pos: [f32; 2],
    pub clip_size: [f32; 2],
    /// Packed border/corner data: `border_style + corner_shape * 16 + round(corner_param * 100) * 256`.
    /// border_style: 0=none, 1=solid, 2=dotted, 3=dashed, 4=double, 5=groove, 6=ridge, 7=inset, 8=outset.
    /// corner_shape: 0=round, 1=angle, 2=superellipse, 3=notch, 4=scoop.
    pub border_extra: f32,
    /// CSS `backdrop-filter: blur()` radius in logical px. Sampled by `rounded_rect.wgsl`
    /// against bind group 1 (blurred page capture). `0` skips sampling.
    pub backdrop_blur: f32,
    pub glass: [f32; 4],
}

impl InstanceInput {
    /// Sentinel: fragment shader skips scissor-style discard.
    pub const NO_CLIP_SIZE: [f32; 2] = [-1.0, -1.0];

    /// Pack border_style (0-8), corner_shape (0-4), and corner_param into a single f32.
    pub fn pack_border_extra(border_style: f32, corner_shape: f32, corner_param: f32) -> f32 {
        border_style + corner_shape * 16.0 + (corner_param * 100.0).round() * 256.0
    }

    /// Inverse of [`Self::pack_border_extra`]. Matches `rounded_rect.wgsl` unpack.
    pub fn unpack_border_extra(packed: f32) -> (f32, f32, f32) {
        let param_enc = (packed / 256.0).floor();
        let rem = packed - param_enc * 256.0;
        let shape = (rem / 16.0).floor();
        let style = rem - shape * 16.0;
        (style, shape, param_enc / 100.0)
    }

    pub fn with_clip(mut self, clip: Option<[f32; 4]>) -> Self {
        // Rotated instances test overflow in world space via `clip_poly`
        // (`frag_px`). Local `clip_pos` vs `in.p` would treat an unrotated
        // ancestor AABB as a local box and leak rotated corners / chop bake.
        let clip = if crate::style::mat2d_is_axis_aligned(&self.transform) {
            clip
        } else {
            None
        };
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
                // shadow_params: [blur, spread, inset]
                wgpu::VertexAttribute {
                    offset: 92,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
                // transform_ab [a, b]
                wgpu::VertexAttribute {
                    offset: 108,
                    shader_location: 11,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // transform_cd [c, d]
                wgpu::VertexAttribute {
                    offset: 116,
                    shader_location: 12,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // transform_txty [tx, ty]
                wgpu::VertexAttribute {
                    offset: 124,
                    shader_location: 13,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // clip_pos
                wgpu::VertexAttribute {
                    offset: 132,
                    shader_location: 14,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // clip_size + border_extra + backdrop_blur
                wgpu::VertexAttribute {
                    offset: 140,
                    shader_location: 15,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 156,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

fn solid_fill_eligible(instance: &InstanceInput) -> bool {
    let shadow_active = instance.shadow_color[3] > 0.001
        && (instance.shadow_blur > 0.001
            || instance.shadow_spread.abs() > 0.001
            || instance.shadow_offset[0].abs() > 0.001
            || instance.shadow_offset[1].abs() > 0.001);
    // Small chrome saves little fragment work and alternating its pipeline can
    // cost more than the shader branch. Restrict specialization to large fills.
    instance.rect_size[0] * instance.rect_size[1] >= 65_536.0
        && instance.glass[0] >= 0.0
        && instance.border_extra >= 0.0
        && instance.border_extra < 1.5
        && instance.backdrop_blur <= 0.5
        && !shadow_active
}

// The VS emits the transformed, padded AABB even for rotated instances.
// Only convex rounded boxes can be proved to contain it by testing corners;
// arbitrary clip paths keep their fragment tests (four corners are insufficient).
fn instance_clip_needed(i: &InstanceInput, clip: &crate::style::GpuClipPoly) -> bool {
    if !clip.is_active() {
        return false;
    }
    if clip.count >= 3
        || clip.count2 >= 3
        || i.rect_size[0] * i.rect_size[1] < 65_536.0
        || clip
            .round_radii
            .iter()
            .chain(&clip.round_radii2)
            .any(|r| !r.is_finite() || *r < 0.0)
    {
        return true;
    }
    // CPU hit testing clamps radii; the GPU SDF consumes them verbatim. Do
    // not use that proof for an unnormalized clip or invalid geometry.
    for (rect, radii) in [
        (clip.round_rect, clip.round_radii),
        (clip.round_rect2, clip.round_radii2),
    ] {
        if rect.iter().any(|v| !v.is_finite()) {
            return true;
        }
        if rect[2] >= 0.5 && rect[3] >= 0.5 && radii.iter().any(|r| *r > rect[2].min(rect[3]) * 0.5)
        {
            return true;
        }
    }
    let pad = i.shadow_blur.max(0.0) * 2.0 + i.border_width.max(0.0) + i.shadow_spread.abs() + 3.0;
    let ex = i.shadow_offset[0].abs() + pad;
    let ey = i.shadow_offset[1].abs() + pad;
    let bounds = crate::style::aabb_of_centered_transform(
        [
            i.rect_pos[0] - ex,
            i.rect_pos[1] - ey,
            i.rect_size[0] + ex * 2.0,
            i.rect_size[1] + ey * 2.0,
        ],
        &i.transform,
    );
    let [x, y, w, h] = bounds;
    ![[x, y], [x + w, y], [x, y + h], [x + w, y + h]]
        .into_iter()
        .all(|p| p.iter().all(|v| v.is_finite()) && clip.contains_point(p))
}

fn opaque_box_eligible(i: &InstanceInput) -> bool {
    if i.color[3] != 1.0
        || i.glass[0] < 0.0
        || i.rect_size[0] * i.rect_size[1] < 262_144.0
        || i.border_extra < 0.0
        || i.border_extra >= 1.5
        || i.backdrop_blur > 0.5
        || i.transform[..4] != [1.0, 0.0, 0.0, 1.0]
        || (i.shadow_inset > 0.5 && i.shadow_color[3] > 0.001)
        || i.radii.iter().any(|r| !r.is_finite() || *r < 0.0)
    {
        return false;
    }
    let inset = i.radii.iter().copied().fold(0.0, f32::max) + i.border_width.max(0.0) + 4.0;
    i.rect_size
        .iter()
        .all(|side| side.is_finite() && *side > inset * 2.0)
}

struct OpaqueBoxPipeline {
    ring: wgpu::RenderPipeline,
    interior: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

pub struct JadixPipeline {
    pub render_pipeline: wgpu::RenderPipeline,
    pub(crate) clear_render_pipeline: wgpu::RenderPipeline,
    solid_render_pipeline: Option<wgpu::RenderPipeline>,
    opaque_box: Option<OpaqueBoxPipeline>,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    pub globals_buffer: wgpu::Buffer,
    /// Safe default: a single `GpuClipPoly::NONE`. Chrome / underlines / offscreen subsets.
    pub bind_group: wgpu::BindGroup,
    /// Per-SDF-instance polygon clip; `instance_index` matches the main instance buffer.
    pub bind_group_clip_poly: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
    backdrop_bgl: wgpu::BindGroupLayout,
    /// 1×1 transparent; instances with `backdrop_blur == 0` never sample it.
    dummy_backdrop_bg: wgpu::BindGroup,
    _dummy_backdrop_tex: wgpu::Texture,
    _dummy_backdrop_sampler: wgpu::Sampler,
    globals_clip_poly_buffer: wgpu::Buffer,
    clip_poly_buffer: super::buffer_upload::RetainedBuffer,
    clip_poly_none_buffer: wgpu::Buffer,
    main_clip_needed: Vec<bool>,
    premultiply: f32,
    resolution: std::sync::Mutex<[u32; 2]>,
}

impl JadixPipeline {
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

        let globals_clip_poly_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Globals with polygon clips"),
                contents: bytemuck::cast_slice(&[Globals {
                    _pad: 1.0,
                    ..Globals::new(config.width as f32, config.height as f32, premultiply)
                }]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let clip_none = crate::style::GpuClipPoly::NONE;
        let clip_poly_none_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Clip poly NONE"),
            contents: bytemuck::bytes_of(&clip_none),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let clip_poly_buffer = super::buffer_upload::RetainedBuffer::new(
            device,
            "Clip poly storage",
            wgpu::BufferUsages::STORAGE,
            bytemuck::bytes_of(&clip_none),
        );

        let bind_group_layout = Self::create_bind_group_layout(device);
        let backdrop_bgl = Self::create_backdrop_bind_group_layout(device);
        let (dummy_backdrop_tex, dummy_backdrop_sampler, dummy_backdrop_bg) =
            Self::make_dummy_backdrop(device, &backdrop_bgl);
        let bind_group = Self::make_bind_group(
            device,
            &bind_group_layout,
            &globals_buffer,
            &clip_poly_none_buffer,
        );
        let bind_group_clip_poly = Self::make_bind_group(
            device,
            &bind_group_layout,
            &globals_clip_poly_buffer,
            &clip_poly_buffer.buffer,
        );
        let render_pipeline =
            Self::make_render_pipeline(device, config, &shader, &bind_group_layout, &backdrop_bgl);
        let clear_render_pipeline = Self::make_render_pipeline_entry(
            device,
            config,
            &shader,
            &bind_group_layout,
            &backdrop_bgl,
            "vs_main",
            "fs_main",
            None,
        );
        let solid_render_pipeline = wgsl_source.contains("fn fs_solid(").then(|| {
            Self::make_render_pipeline_entry(
                device,
                config,
                &shader,
                &bind_group_layout,
                &backdrop_bgl,
                "vs_main",
                "fs_solid",
                Some(crate::renderer::color_target_blend(config.alpha_mode)),
            )
        });

        let opaque_box = Self::make_opaque_box_pipeline(
            device,
            config,
            &shader,
            &bind_group_layout,
            &backdrop_bgl,
            wgsl_source,
        );
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
            clear_render_pipeline,
            solid_render_pipeline,
            opaque_box,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            globals_buffer,
            bind_group,
            bind_group_clip_poly,
            bind_group_layout,
            backdrop_bgl,
            dummy_backdrop_bg,
            _dummy_backdrop_tex: dummy_backdrop_tex,
            _dummy_backdrop_sampler: dummy_backdrop_sampler,
            globals_clip_poly_buffer,
            clip_poly_buffer,
            clip_poly_none_buffer,
            main_clip_needed: Vec::new(),
            premultiply,
            resolution: std::sync::Mutex::new([config.width, config.height]),
        }
    }

    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Globals Bind Group Layout"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
    }

    fn create_backdrop_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SDF Backdrop Bind Group Layout"),
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
            ],
        })
    }

    fn make_dummy_backdrop(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
    ) -> (wgpu::Texture, wgpu::Sampler, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("SDF dummy backdrop"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SDF dummy backdrop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF dummy backdrop BG"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        (texture, sampler, bind_group)
    }

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals_buffer: &wgpu::Buffer,
        clip_poly_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: clip_poly_buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn make_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        bind_group_layout: &wgpu::BindGroupLayout,
        backdrop_bgl: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        Self::make_render_pipeline_entry(
            device,
            config,
            shader,
            bind_group_layout,
            backdrop_bgl,
            "vs_main",
            "fs_main",
            Some(crate::renderer::color_target_blend(config.alpha_mode)),
        )
    }

    fn make_render_pipeline_entry(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        bind_group_layout: &wgpu::BindGroupLayout,
        backdrop_bgl: &wgpu::BindGroupLayout,
        vertex_entry: &str,
        fragment_entry: &str,
        blend: Option<wgpu::BlendState>,
    ) -> wgpu::RenderPipeline {
        let mut vertex_layout = VertexInput::desc();
        vertex_layout.attributes = &vertex_layout.attributes[..1];
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Jadix Render Pipeline Layout"),
                bind_group_layouts: &[Some(bind_group_layout), Some(backdrop_bgl)],
                immediate_size: 0,
            });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Jadix Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vertex_entry),
                buffers: &[Some(vertex_layout), Some(InstanceInput::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend,
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
        })
    }

    fn make_opaque_box_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        shader: &wgpu::ShaderModule,
        globals: &wgpu::BindGroupLayout,
        backdrop: &wgpu::BindGroupLayout,
        source: &str,
    ) -> Option<OpaqueBoxPipeline> {
        if !source.contains("fn vs_ring(")
            || !source.contains("fn vs_interior(")
            || !source.contains("fn fs_interior(")
        {
            return None;
        }
        let mut vertices = Vec::with_capacity(32);
        let mut indices: Vec<u16> = Vec::with_capacity(48);
        for y in 0..3 {
            for x in 0..3 {
                if x == 1 && y == 1 {
                    continue;
                }
                let base = vertices.len() as u16;
                for (cx, cy) in [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)] {
                    vertices.push(VertexInput {
                        position: [cx as f32 / 3.0, cy as f32 / 3.0],
                        uv: [0.0; 2],
                    });
                }
                indices.extend([base, base + 1, base + 2, base + 2, base + 3, base]);
            }
        }
        Some(OpaqueBoxPipeline {
            ring: Self::make_render_pipeline_entry(
                device,
                config,
                shader,
                globals,
                backdrop,
                "vs_ring",
                // Share the compiled general fragment; a custom hot-reload
                // shader may still supply a distinct ring fragment.
                if source.contains("fn fs_ring(") {
                    "fs_ring"
                } else {
                    "fs_main"
                },
                Some(crate::renderer::color_target_blend(config.alpha_mode)),
            ),
            interior: Self::make_render_pipeline_entry(
                device,
                config,
                shader,
                globals,
                backdrop,
                "vs_interior",
                "fs_interior",
                Some(crate::renderer::color_target_blend(config.alpha_mode)),
            ),
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Opaque box perimeter vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Opaque box perimeter indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
        })
    }

    /// Upload per-instance polygon clips. Empty input stores one `NONE` (wgpu forbids 0-size storage).
    pub fn ensure_clip_polys(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        polys: &[crate::style::GpuClipPoly],
    ) {
        self.main_clip_needed.clear();
        let none = [crate::style::GpuClipPoly::NONE];
        let data = if polys.is_empty() { &none[..] } else { polys };
        if self
            .clip_poly_buffer
            .upload(device, queue, bytemuck::cast_slice(data))
        {
            self.bind_group_clip_poly = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_clip_poly_buffer,
                &self.clip_poly_buffer.buffer,
            );
        }
    }

    /// Dev hot-reload: replace the main rect shader (entry points stay `vs_main` / `fs_main`).
    ///
    /// Returns `false` and keeps the previous pipeline if WGSL or PSO validation fails.
    pub fn rebuild_wgsl(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        wgsl_source: &str,
    ) -> bool {
        let Some(shader) = crate::renderer::wgpu_compat::try_shader_module(
            device,
            "Rounded Rect Shader (hot reload)",
            wgsl_source,
        ) else {
            log::warn!("hot-reload WGSL: shader module failed; keeping previous pipeline");
            return false;
        };
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let bind_group = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_buffer,
                &self.clip_poly_none_buffer,
            );
            let bind_group_clip_poly = Self::make_bind_group(
                device,
                &self.bind_group_layout,
                &self.globals_clip_poly_buffer,
                &self.clip_poly_buffer.buffer,
            );
            let render_pipeline = Self::make_render_pipeline(
                device,
                config,
                &shader,
                &self.bind_group_layout,
                &self.backdrop_bgl,
            );
            let solid_render_pipeline = wgsl_source.contains("fn fs_solid(").then(|| {
                Self::make_render_pipeline_entry(
                    device,
                    config,
                    &shader,
                    &self.bind_group_layout,
                    &self.backdrop_bgl,
                    "vs_main",
                    "fs_solid",
                    Some(crate::renderer::color_target_blend(config.alpha_mode)),
                )
            });
            let clear_render_pipeline = Self::make_render_pipeline_entry(
                device,
                config,
                &shader,
                &self.bind_group_layout,
                &self.backdrop_bgl,
                "vs_main",
                "fs_main",
                None,
            );
            let opaque_box = Self::make_opaque_box_pipeline(
                device,
                config,
                &shader,
                &self.bind_group_layout,
                &self.backdrop_bgl,
                wgsl_source,
            );
            (
                bind_group,
                bind_group_clip_poly,
                render_pipeline,
                clear_render_pipeline,
                solid_render_pipeline,
                opaque_box,
            )
        }));
        let (
            bind_group,
            bind_group_clip_poly,
            render_pipeline,
            clear_render_pipeline,
            solid_render_pipeline,
            opaque_box,
        ) = match built {
            Ok(tuple) => tuple,
            Err(_) => {
                let _ = crate::renderer::wgpu_compat::wait_error_scope(device, scope);
                log::warn!(
                    "hot-reload WGSL: pipeline create panicked; keeping previous pipeline"
                );
                return false;
            }
        };
        if crate::renderer::wgpu_compat::wait_error_scope(device, scope).is_some() {
            log::warn!("hot-reload WGSL: pipeline validation failed; keeping previous pipeline");
            return false;
        }
        self.bind_group = bind_group;
        self.bind_group_clip_poly = bind_group_clip_poly;
        self.render_pipeline = render_pipeline;
        self.clear_render_pipeline = clear_render_pipeline;
        self.solid_render_pipeline = solid_render_pipeline;
        self.opaque_box = opaque_box;
        true
    }

    /// Split only consecutive styles. Bind groups and instance indices remain
    /// unchanged, so polygon clips and translucent painter order stay intact.
    pub(crate) fn draw_instance_span(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instances: &[InstanceInput],
        start: u32,
        count: u32,
    ) {
        let end = start + count;
        if self.solid_render_pipeline.is_none() || end as usize > instances.len() {
            pass.set_pipeline(&self.render_pipeline);
            pass.draw_indexed(0..self.num_indices, 0, start..end);
            return;
        }
        let mode = |i: &InstanceInput| {
            if self.opaque_box.is_some() && opaque_box_eligible(i) {
                2
            } else if solid_fill_eligible(i) {
                1
            } else {
                0
            }
        };
        let mut lo = start;
        while lo < end {
            let kind = mode(&instances[lo as usize]);
            let mut hi = lo + 1;
            while hi < end && mode(&instances[hi as usize]) == kind {
                hi += 1;
            }
            if kind == 2 {
                let opaque = self.opaque_box.as_ref().unwrap();
                // A later perimeter can cover an earlier center. Keep the two
                // draws adjacent for each instance, including overlapping cards.
                for index in lo..hi {
                    pass.set_pipeline(&opaque.ring);
                    pass.set_vertex_buffer(0, opaque.vertices.slice(..));
                    pass.set_index_buffer(opaque.indices.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..48, 0, index..index + 1);
                    pass.set_pipeline(&opaque.interior);
                    pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    pass.draw_indexed(0..self.num_indices, 0, index..index + 1);
                }
            } else {
                pass.set_pipeline(if kind == 1 {
                    self.solid_render_pipeline.as_ref().unwrap()
                } else {
                    &self.render_pipeline
                });
                pass.draw_indexed(0..self.num_indices, 0, lo..hi);
            }
            lo = hi;
        }
        // Existing direct draws can follow this ranged helper.
        pass.set_pipeline(&self.render_pipeline);
    }

    /// Associate the main instance buffer with its uploaded world clips.
    /// Subset/capture buffers continue to use their own explicit clip bind group.
    pub(crate) fn prepare_main_instance_clips(
        &mut self,
        instances: &[InstanceInput],
        polys: &[crate::style::GpuClipPoly],
    ) {
        self.main_clip_needed.clear();
        self.main_clip_needed
            .extend(instances.iter().enumerate().map(|(index, inst)| {
                polys
                    .get(index)
                    .is_some_and(|clip| instance_clip_needed(inst, clip))
            }));
    }

    pub(crate) fn draw_main_instance_span(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instances: &[InstanceInput],
        start: u32,
        count: u32,
    ) {
        let end = start + count;
        if end as usize > self.main_clip_needed.len() {
            self.draw_instance_span(pass, instances, start, count);
            return;
        }
        let mut lo = start;
        while lo < end {
            let clipped = self.main_clip_needed[lo as usize];
            let mut hi = lo + 1;
            while hi < end && self.main_clip_needed[hi as usize] == clipped {
                hi += 1;
            }
            pass.set_bind_group(
                0,
                if clipped {
                    &self.bind_group_clip_poly
                } else {
                    &self.bind_group
                },
                &[],
            );
            self.draw_instance_span(pass, instances, lo, hi - lo);
            lo = hi;
        }
        // Leave the full-buffer binding for subsequent direct draws.
        pass.set_bind_group(0, &self.bind_group_clip_poly, &[]);
    }

    /// Bind group 1 when the instance does not sample a blurred capture.
    pub fn dummy_backdrop(&self) -> &wgpu::BindGroup {
        &self.dummy_backdrop_bg
    }

    /// Bind group 1 for CSS `backdrop-filter` sampling (`frag_px / resolution`).
    pub fn make_backdrop_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.make_optical_backdrop_bind_group(device, view, view, sampler)
    }

    pub fn make_optical_backdrop_bind_group(
        &self,
        device: &wgpu::Device,
        blurred: &wgpu::TextureView,
        source: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF backdrop BG"),
            layout: &self.backdrop_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(blurred),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(source),
                },
            ],
        })
    }

    pub fn bind_sdf<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        clip_poly: bool,
        backdrop: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(
            0,
            if clip_poly {
                &self.bind_group_clip_poly
            } else {
                &self.bind_group
            },
            &[],
        );
        pass.set_bind_group(1, backdrop, &[]);
    }

    /// Bind SDF with a clip-poly SSBO that starts at index 0.
    ///
    /// Pair with an instance vertex buffer that also starts at 0. GLES `instance_index`
    /// is relative to `firstInstance`; a ranged draw against the full SSBO would clip
    /// to `clip_polys[0]` (another widget) instead of this range. The returned buffer
    /// and bind group must stay alive until the draw is encoded.
    pub fn bind_sdf_with_clip_polys<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        device: &wgpu::Device,
        backdrop: &'a wgpu::BindGroup,
        polys: &[crate::style::GpuClipPoly],
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let data: Vec<crate::style::GpuClipPoly> = if polys.is_empty() {
            vec![crate::style::GpuClipPoly::NONE]
        } else {
            polys.to_vec()
        };
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Clip poly range"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let bg = Self::make_bind_group(
            device,
            &self.bind_group_layout,
            &self.globals_clip_poly_buffer,
            &buf,
        );
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.set_bind_group(1, backdrop, &[]);
        (buf, bg)
    }

    pub fn update_resolution(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let mut resolution = self.resolution.lock().unwrap();
        if *resolution == [width, height] {
            return;
        }
        let globals = Globals::new(width as f32, height as f32, self.premultiply);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));
        queue.write_buffer(
            &self.globals_clip_poly_buffer,
            0,
            bytemuck::cast_slice(&[Globals {
                _pad: 1.0,
                ..globals
            }]),
        );
        *resolution = [width, height];
    }
}

pub(crate) fn compositor_premultiply(alpha_mode: wgpu::CompositeAlphaMode) -> f32 {
    match alpha_mode {
        wgpu::CompositeAlphaMode::PreMultiplied => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::InstanceInput;

    #[test]
    fn opaque_center_requires_full_coverage_and_untransformed_painter_geometry() {
        let mut inst = InstanceInput {
            rect_pos: [0.0; 2],
            rect_size: [640.0; 2],
            color: [1.0; 4],
            border_color: [0.0; 4],
            radii: [12.0; 4],
            border_width: 3.0,
            shadow_color: [0.0, 0.0, 0.0, 0.2],
            shadow_offset: [4.0, 6.0],
            shadow_blur: 12.0,
            shadow_spread: 0.0,
            shadow_inset: 0.0,
            backdrop_fade: 0.0,
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0; 2],
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 1.0,
            backdrop_blur: 0.0,
            glass: [0.0; 4],
        };
        assert!(super::opaque_box_eligible(&inst));
        inst.shadow_inset = 1.0;
        assert!(
            !super::opaque_box_eligible(&inst),
            "inset shadow colors the center"
        );
        inst.shadow_inset = 0.0;
        inst.color[3] = 0.9;
        assert!(
            !super::opaque_box_eligible(&inst),
            "alpha order must remain unchanged"
        );
        inst.color[3] = 1.0;
        inst.border_extra = 2.0;
        assert!(
            !super::opaque_box_eligible(&inst),
            "decorative borders keep general coverage"
        );
        inst.border_extra = 1.0;
        inst.transform[0] = 1.01;
        assert!(
            !super::opaque_box_eligible(&inst),
            "transformed SDF uses its original AABB"
        );
        inst.transform[0] = 1.0;
        inst.backdrop_blur = 4.0;
        assert!(!super::opaque_box_eligible(&inst));
        inst.backdrop_blur = 0.0;
        inst.radii = [320.0; 4];
        assert!(
            !super::opaque_box_eligible(&inst),
            "round body has no opaque rectangular center"
        );
        inst.radii = [8.0; 4];
        inst.shadow_color = [0.0; 4];
        assert!(super::solid_fill_eligible(&inst));
        inst.glass = [-1.0, 0.5, 0.22, 0.0];
        assert!(!super::solid_fill_eligible(&inst), "a sweep is not a constant fill");
        assert!(!super::opaque_box_eligible(&inst), "a sweep has transparent edges");
    }

    #[test]
    fn contained_large_quad_skips_only_redundant_convex_clips() {
        use super::{instance_clip_needed, InstanceInput};
        use crate::style::GpuClipPoly;
        let mut inst = InstanceInput {
            rect_pos: [64.0, 64.0],
            rect_size: [256.0, 256.0],
            color: [1.0; 4],
            border_color: [0.0; 4],
            radii: [12.0; 4],
            border_width: 3.0,
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            shadow_inset: 0.0,
            backdrop_fade: 0.0,
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0; 2],
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 1.0,
            backdrop_blur: 0.0,
            glass: [0.0; 4],
        };
        let clip = GpuClipPoly::from_rounded_rect([0.0, 0.0, 384.0, 384.0], [24.0; 4]);
        assert!(!instance_clip_needed(&inst, &clip));
        inst.rect_pos = [12.0, 12.0];
        assert!(
            instance_clip_needed(&inst, &clip),
            "AA/border may touch rounded corner"
        );
        inst.rect_pos = [64.0, 64.0];
        inst.shadow_blur = 40.0;
        assert!(
            instance_clip_needed(&inst, &clip),
            "shadow halo leaves clip"
        );
        inst.shadow_blur = 0.0;
        inst.transform = [0.707, 0.707, -0.707, 0.707, 0.0, 0.0];
        assert!(
            instance_clip_needed(&inst, &clip),
            "rotated AABB leaves rounded corner"
        );
        assert!(!instance_clip_needed(&inst, &GpuClipPoly::NONE));
        let poly = GpuClipPoly::from_world_points(
            &[[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0]],
            crate::style::ClipFillRule::NonZero,
        );
        assert!(
            instance_clip_needed(&inst, &poly),
            "polygon paths retain fragment tests"
        );
        inst.transform = crate::style::transform::MAT2D_IDENTITY;
        let mut second = clip;
        second.round_rect2 = [100.0, 100.0, 100.0, 100.0];
        assert!(instance_clip_needed(&inst, &second));
    }

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "expected {a} ~= {b}");
    }

    #[test]
    fn pack_unpack_border_extra_roundtrip() {
        let cases = [
            (1.0, 0.0, 0.0), // solid + round
            (2.0, 0.0, 0.0), // dotted + round
            (3.0, 0.0, 0.0), // dashed
            (1.0, 1.0, 0.0), // solid + angle
            (1.0, 2.0, 4.0), // superellipse(4)
            (1.0, 2.0, 8.0), // superellipse(8)
            (1.0, 3.0, 0.0), // notch
            (1.0, 4.0, 0.0), // scoop
        ];
        for (style, shape, param) in cases {
            let packed = InstanceInput::pack_border_extra(style, shape, param);
            let (s, sh, p) = InstanceInput::unpack_border_extra(packed);
            approx(s, style);
            approx(sh, shape);
            approx(p, param);
        }
    }
}
