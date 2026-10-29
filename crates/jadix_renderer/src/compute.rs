//! Optional wgpu compute pipelines (S7). Off unless `--features compute`.
//! GLES never constructs these even with the feature (adapter probe).
//!
//! Consumers: separable Gaussian (`effect_wrap("blur")`, CSS `filter: blur`,
//! `backdrop-filter`), luminance histogram + large-image downsample, particle fx.
//! Missing feature, adapter compute, or compile failure → fragment / CPU path.
//! User `shader()` leaves stay fragment.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::gpu_caps::GpuCaps;
#[cfg(feature = "compute")]
use super::wgpu_compat::{try_compute_pipeline, try_shader_module};

#[cfg(feature = "compute")]
const BLUR_WGSL: &str = include_str!("blur_compute.wgsl");
#[cfg(feature = "compute")]
const HIST_WGSL: &str = include_str!("histogram_compute.wgsl");
#[cfg(feature = "compute")]
const DOWN_WGSL: &str = include_str!("downsample_compute.wgsl");
#[cfg(feature = "compute")]
const PARTICLE_WGSL: &str = include_str!("particle_compute.wgsl");

const HIST_BINS: u64 = 256;
const HIST_BYTES: u64 = HIST_BINS * 4;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct BlurParams {
    texel: [f32; 2],
    dir: [f32; 2],
    sigma: f32,
    _pad0: f32,
    _pad1: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct ParticleParams {
    time: f32,
    strength: f32,
    resolution: [f32; 2],
}

struct Scratch {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
    w: u32,
    h: u32,
}

/// Cached blur compute pipeline + ping-pong storage textures.
pub struct ComputeBlur {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform_buf: wgpu::Buffer,
    scratch_a: Option<Scratch>,
    scratch_b: Option<Scratch>,
}

/// 256-bin luminance histogram + downsample to rgba8unorm storage.
pub struct ComputeHistogram {
    hist_pipeline: wgpu::ComputePipeline,
    hist_bgl: wgpu::BindGroupLayout,
    down_pipeline: wgpu::ComputePipeline,
    down_bgl: wgpu::BindGroupLayout,
    bins: wgpu::Buffer,
    sampler: wgpu::Sampler,
    scratch: Option<Scratch>,
}

/// Particle field written to rgba8unorm storage (histogram-weighted).
pub struct ComputeParticles {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    empty_bins: wgpu::Buffer,
    scratch: Option<Scratch>,
}

pub struct ComputePipelines {
    pub blur: Option<ComputeBlur>,
    pub histogram: Option<ComputeHistogram>,
    pub particles: Option<ComputeParticles>,
}

impl ComputePipelines {
    /// Empty when the Cargo feature is off, `caps.compute` is false, or compile fails.
    pub fn new(device: &wgpu::Device, caps: &mut GpuCaps) -> Self {
        #[cfg(not(feature = "compute"))]
        {
            let _ = device;
            caps.compute = false;
            return Self {
                blur: None,
                histogram: None,
                particles: None,
            };
        }
        #[cfg(feature = "compute")]
        {
            if !caps.compute {
                return Self {
                    blur: None,
                    histogram: None,
                    particles: None,
                };
            }
            let blur = ComputeBlur::try_new(device);
            let histogram = ComputeHistogram::try_new(device);
            let particles = ComputeParticles::try_new(device);
            if blur.is_none() && histogram.is_none() && particles.is_none() {
                log::warn!("compute pipelines failed; disabling compute for this process");
                caps.compute = false;
            }
            Self {
                blur,
                histogram,
                particles,
            }
        }
    }

    pub fn enabled(&self) -> bool {
        self.blur.is_some() || self.histogram.is_some() || self.particles.is_some()
    }
}

impl ComputeBlur {
    pub fn try_new(device: &wgpu::Device) -> Option<Self> {
        #[cfg(not(feature = "compute"))]
        {
            let _ = device;
            return None;
        }
        #[cfg(feature = "compute")]
        {
            let module = try_shader_module(device, "jadix-blur-cs", BLUR_WGSL)?;
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jadix-blur-cs-bgl"),
                entries: &[
                    uniform_entry(std::mem::size_of::<BlurParams>() as u64),
                    sampled_tex_entry(1),
                    sampler_entry(2),
                    storage_tex_entry(3),
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jadix-blur-cs-pl"),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });
            let pipeline = try_compute_pipeline(
                device,
                &wgpu::ComputePipelineDescriptor {
                    label: Some("jadix-blur-cs"),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some("cs_main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            )?;
            let uniform_buf =
                make_uniform(device, "jadix-blur-cs-u", std::mem::size_of::<BlurParams>());
            let sampler = make_linear_sampler(device, "jadix-blur-cs-samp");
            Some(Self {
                pipeline,
                bgl,
                sampler,
                uniform_buf,
                scratch_a: None,
                scratch_b: None,
            })
        }
    }

    /// Two-pass separable Gaussian into the caller's encoder. Result is [`Self::result_view`].
    #[expect(
        clippy::too_many_arguments,
        reason = "GPU passes receive explicit device, resources, geometry and encoding state without intermediate allocations"
    )]
    pub fn blur_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        w: u32,
        h: u32,
        sigma: f32,
    ) -> bool {
        let w = w.max(1);
        let h = h.max(1);
        let sigma = sigma.clamp(0.5, 32.0);
        self.ensure_scratch(device, w, h);
        let texel = [1.0 / w as f32, 1.0 / h as f32];
        let Some(a) = self.scratch_a.as_ref() else {
            return false;
        };
        let Some(b) = self.scratch_b.as_ref() else {
            return false;
        };
        dispatch_blur(
            device,
            queue,
            encoder,
            &self.pipeline,
            &self.bgl,
            &self.sampler,
            &self.uniform_buf,
            src,
            &a.view,
            w,
            h,
            texel,
            [1.0, 0.0],
            sigma,
        );
        dispatch_blur(
            device,
            queue,
            encoder,
            &self.pipeline,
            &self.bgl,
            &self.sampler,
            &self.uniform_buf,
            &a.view,
            &b.view,
            w,
            h,
            texel,
            [0.0, 1.0],
            sigma,
        );
        if sigma > 4.0 {
            let s2 = sigma * 0.55;
            dispatch_blur(
                device,
                queue,
                encoder,
                &self.pipeline,
                &self.bgl,
                &self.sampler,
                &self.uniform_buf,
                &b.view,
                &a.view,
                w,
                h,
                texel,
                [1.0, 0.0],
                s2,
            );
            dispatch_blur(
                device,
                queue,
                encoder,
                &self.pipeline,
                &self.bgl,
                &self.sampler,
                &self.uniform_buf,
                &a.view,
                &b.view,
                w,
                h,
                texel,
                [0.0, 1.0],
                s2,
            );
        }
        true
    }

    /// Two-pass separable Gaussian. Result is [`Self::result_view`] (Rgba8Unorm).
    pub fn blur_submit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::TextureView,
        w: u32,
        h: u32,
        sigma: f32,
    ) -> Option<&wgpu::TextureView> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-blur-cs-enc"),
        });
        if !self.blur_encode(device, queue, &mut encoder, src, w, h, sigma) {
            return None;
        }
        queue.submit(Some(encoder.finish()));
        self.result_view()
    }

    pub fn result_view(&self) -> Option<&wgpu::TextureView> {
        self.scratch_b.as_ref().map(|s| &s.view)
    }

    fn ensure_scratch(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        let reuse = |s: &Option<Scratch>| s.as_ref().is_some_and(|t| t.w == w && t.h == h);
        if reuse(&self.scratch_a) && reuse(&self.scratch_b) {
            return;
        }
        self.scratch_a = Some(make_scratch(device, w, h, "jadix-blur-cs-a"));
        self.scratch_b = Some(make_scratch(device, w, h, "jadix-blur-cs-b"));
    }
}

impl ComputeHistogram {
    pub fn try_new(device: &wgpu::Device) -> Option<Self> {
        #[cfg(not(feature = "compute"))]
        {
            let _ = device;
            return None;
        }
        #[cfg(feature = "compute")]
        {
            let hist_mod = try_shader_module(device, "jadix-hist-cs", HIST_WGSL)?;
            let down_mod = try_shader_module(device, "jadix-down-cs", DOWN_WGSL)?;
            let hist_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jadix-hist-cs-bgl"),
                entries: &[
                    sampled_tex_entry(0),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(HIST_BYTES),
                        },
                        count: None,
                    },
                ],
            });
            let down_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jadix-down-cs-bgl"),
                entries: &[sampled_tex_entry(0), sampler_entry(1), storage_tex_entry(2)],
            });
            let hist_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jadix-hist-cs-pl"),
                bind_group_layouts: &[Some(&hist_bgl)],
                immediate_size: 0,
            });
            let down_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jadix-down-cs-pl"),
                bind_group_layouts: &[Some(&down_bgl)],
                immediate_size: 0,
            });
            let hist_pipeline = try_compute_pipeline(
                device,
                &wgpu::ComputePipelineDescriptor {
                    label: Some("jadix-hist-cs"),
                    layout: Some(&hist_pl),
                    module: &hist_mod,
                    entry_point: Some("cs_hist"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            )?;
            let down_pipeline = try_compute_pipeline(
                device,
                &wgpu::ComputePipelineDescriptor {
                    label: Some("jadix-down-cs"),
                    layout: Some(&down_pl),
                    module: &down_mod,
                    entry_point: Some("cs_down"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            )?;
            let bins = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jadix-hist-bins"),
                size: HIST_BYTES,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Some(Self {
                hist_pipeline,
                hist_bgl,
                down_pipeline,
                down_bgl,
                bins,
                sampler: make_linear_sampler(device, "jadix-hist-samp"),
                scratch: None,
            })
        }
    }

    pub fn bins_buffer(&self) -> &wgpu::Buffer {
        &self.bins
    }

    /// Clear + accumulate 256 luma bins. Own encoder (safe before a later particle submit).
    pub fn submit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> bool {
        let w = w.max(1);
        let h = h.max(1);
        queue.write_buffer(&self.bins, 0, &[0u8; HIST_BYTES as usize]);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-hist-cs-bg"),
            layout: &self.hist_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.bins.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-hist-cs-enc"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("jadix-hist-cs"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.hist_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        queue.submit(Some(encoder.finish()));
        true
    }

    /// Average-resample `src` into a storage texture of `w`×`h`. Result is [`Self::result_view`].
    pub fn downsample_encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> bool {
        let _ = queue;
        let w = w.max(1);
        let h = h.max(1);
        if !self.scratch.as_ref().is_some_and(|s| s.w == w && s.h == h) {
            self.scratch = Some(make_scratch(device, w, h, "jadix-down-cs"));
        }
        let Some(dst) = self.scratch.as_ref() else {
            return false;
        };
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-down-cs-bg"),
            layout: &self.down_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dst.view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("jadix-down-cs"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.down_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        true
    }

    pub fn result_view(&self) -> Option<&wgpu::TextureView> {
        self.scratch.as_ref().map(|s| &s.view)
    }
}

impl ComputeParticles {
    pub fn try_new(device: &wgpu::Device) -> Option<Self> {
        #[cfg(not(feature = "compute"))]
        {
            let _ = device;
            return None;
        }
        #[cfg(feature = "compute")]
        {
            let module = try_shader_module(device, "jadix-particle-cs", PARTICLE_WGSL)?;
            let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("jadix-particle-cs-bgl"),
                entries: &[
                    uniform_entry(std::mem::size_of::<ParticleParams>() as u64),
                    sampled_tex_entry(1),
                    sampler_entry(2),
                    storage_tex_entry(3),
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(HIST_BYTES),
                        },
                        count: None,
                    },
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("jadix-particle-cs-pl"),
                bind_group_layouts: &[Some(&bgl)],
                immediate_size: 0,
            });
            let pipeline = try_compute_pipeline(
                device,
                &wgpu::ComputePipelineDescriptor {
                    label: Some("jadix-particle-cs"),
                    layout: Some(&layout),
                    module: &module,
                    entry_point: Some("cs_main"),
                    compilation_options: Default::default(),
                    cache: None,
                },
            )?;
            let empty_bins = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("jadix-particle-empty-bins"),
                contents: &[0u8; HIST_BYTES as usize],
                usage: wgpu::BufferUsages::STORAGE,
            });
            Some(Self {
                pipeline,
                bgl,
                uniform_buf: make_uniform(
                    device,
                    "jadix-particle-cs-u",
                    std::mem::size_of::<ParticleParams>(),
                ),
                sampler: make_linear_sampler(device, "jadix-particle-cs-samp"),
                empty_bins,
                scratch: None,
            })
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "GPU passes receive explicit device, resources, geometry and encoding state without intermediate allocations"
    )]
    pub fn submit(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::TextureView,
        bins: Option<&wgpu::Buffer>,
        w: u32,
        h: u32,
        time: f32,
        strength: f32,
    ) -> Option<&wgpu::TextureView> {
        let w = w.max(1);
        let h = h.max(1);
        if !self.scratch.as_ref().is_some_and(|s| s.w == w && s.h == h) {
            self.scratch = Some(make_scratch(device, w, h, "jadix-particle-cs"));
        }
        let dst = self.scratch.as_ref()?;
        let params = ParticleParams {
            time,
            strength,
            resolution: [w as f32, h as f32],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&params));
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-particle-cs-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&dst.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bins.unwrap_or(&self.empty_bins).as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-particle-cs-enc"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("jadix-particle-cs"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
        }
        queue.submit(Some(encoder.finish()));
        self.scratch.as_ref().map(|s| &s.view)
    }
}

#[cfg(feature = "compute")]
fn uniform_entry(size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

#[cfg(feature = "compute")]
fn sampled_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    }
}

#[cfg(feature = "compute")]
fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

#[cfg(feature = "compute")]
fn storage_tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

#[cfg(feature = "compute")]
fn make_uniform(device: &wgpu::Device, label: &str, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(feature = "compute")]
fn make_linear_sampler(device: &wgpu::Device, label: &str) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(label),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "GPU passes receive explicit device, resources, geometry and encoding state without intermediate allocations"
)]
fn dispatch_blur(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniform_buf: &wgpu::Buffer,
    src: &wgpu::TextureView,
    dst: &wgpu::TextureView,
    w: u32,
    h: u32,
    texel: [f32; 2],
    dir: [f32; 2],
    sigma: f32,
) {
    let params = BlurParams {
        texel,
        dir,
        sigma,
        _pad0: 0.0,
        _pad1: [0.0, 0.0],
    };
    let upload = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("jadix-blur-cs-params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    encoder.copy_buffer_to_buffer(
        &upload,
        0,
        uniform_buf,
        0,
        std::mem::size_of::<BlurParams>() as u64,
    );
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("jadix-blur-cs-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(src),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(dst),
            },
        ],
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("jadix-blur-cs"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
    }
}

fn make_scratch(device: &wgpu::Device, w: u32, h: u32, label: &str) -> Scratch {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    Scratch {
        _tex: tex,
        view,
        w,
        h,
    }
}

#[cfg(test)]
#[path = "tests/compute.rs"]
mod tests;
