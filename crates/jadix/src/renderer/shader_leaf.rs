//! Public `shader()` leaf: user WGSL fragment → offscreen RT → `jadix_shader:` texture.
//!
//! Compile failure keeps the last frame or draws magenta. Never panics.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::app::image_registry::GpuImageTexture;
use crate::primitives::shader::take_shader_jobs;
use crate::renderer::pipeline::VertexInput;
use crate::renderer::TexturedQuadPipeline;

static RUNTIME_ON: AtomicBool = AtomicBool::new(true);

pub(crate) fn shader_gpu_enabled() -> bool {
    RUNTIME_ON.load(Ordering::Relaxed)
}

pub(crate) fn disable_shader_gpu_runtime() {
    RUNTIME_ON.store(false, Ordering::Relaxed);
}

const PREAMBLE: &str = include_str!("shader_preamble.wgsl");
const FALLBACK_FS: &str = r#"
@fragment
fn fs_main(_in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 1.0, 1.0);
}
"#;

fn resolve_src_view(
    images: &HashMap<String, GpuImageTexture>,
    user_key: Option<&str>,
    use_buf: bool,
    buf: Option<&wgpu::Texture>,
) -> Option<wgpu::TextureView> {
    if let Some(k) = user_key {
        return images.get(k).map(|img| {
            img._texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
    }
    if use_buf {
        return buf.map(|tex| tex.create_view(&wgpu::TextureViewDescriptor::default()));
    }
    None
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct ShaderUniforms {
    time: f32,
    _pad0: f32,
    resolution: [f32; 2],
    pointer: [f32; 2],
    _pad1: [f32; 2],
    custom: [f32; 16],
}

enum CachedPipe {
    Ok(wgpu::RenderPipeline),
    Failed,
}

pub struct ShaderGpu {
    fallback: wgpu::RenderPipeline,
    layout: wgpu::PipelineLayout,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    dummy_tex: wgpu::Texture,
    dummy_view: wgpu::TextureView,
    dummy_ready: bool,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    format: wgpu::TextureFormat,
    pipelines: HashMap<u64, CachedPipe>,
    target_size: HashMap<String, (u32, u32)>,
    /// Keys that compiled once so a later failure can keep the last good frame.
    had_good_frame: HashMap<String, bool>,
    /// Previous-frame ping textures for `.buffer(true)` (empty src slot).
    buf_tex: HashMap<String, wgpu::Texture>,
}

impl ShaderGpu {
    /// Magenta fallback PSO. `None` if even that fails — caller skips shader leaves (E2).
    pub fn try_new(device: &wgpu::Device) -> Option<Self> {
        match crate::renderer::wgpu_compat::try_gpu_init(
            device,
            "shader() fallback pipeline failed; shader leaves disabled",
            || Self::build(device),
        ) {
            Some(Some(gpu)) => Some(gpu),
            Some(None) => {
                log::warn!("shader() fallback pipeline failed; shader leaves disabled");
                None
            }
            None => None,
        }
    }

    fn build(device: &wgpu::Device) -> Option<Self> {
        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-shader-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<ShaderUniforms>() as u64
                    ),
                },
                count: None,
            }],
        });
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("jadix-shader-tex-bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("jadix-shader-pl"),
            bind_group_layouts: &[Some(&uniform_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });
        let fallback = create_pipeline(
            device,
            &layout,
            &compose_user_wgsl(FALLBACK_FS),
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?;

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("jadix-shader-uniforms"),
            size: std::mem::size_of::<ShaderUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-shader-ubg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("jadix-shader-src-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-shader-dummy-src"),
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
        let dummy_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());

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
            label: Some("jadix-shader-vb"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("jadix-shader-ib"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Some(Self {
            fallback,
            layout,
            uniform_buf,
            uniform_bg,
            tex_bgl,
            sampler,
            dummy_tex,
            dummy_view,
            dummy_ready: false,
            vertex_buffer,
            index_buffer,
            num_indices: 6,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            pipelines: HashMap::new(),
            target_size: HashMap::new(),
            had_good_frame: HashMap::new(),
            buf_tex: HashMap::new(),
        })
    }

    /// Returns `true` if any leaf was drawn (caller should `window.request_redraw` for time).
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        time_s: f32,
        pointer: [f32; 2],
    ) -> bool {
        let mut last: HashMap<String, crate::primitives::shader::ShaderJob> = HashMap::new();
        for job in take_shader_jobs() {
            last.insert(job.key.clone(), job);
        }
        let mut any = false;
        for job in last.into_values() {
            if self.render_job(device, queue, textured, images, job, time_s, pointer) {
                any = true;
            }
        }
        if any {
            crate::app::pending_images::bump_dynamic_texture_epoch();
        }
        any
    }

    pub(crate) fn prepare_source(&mut self, device: &wgpu::Device, source: &str) -> u64 {
        let composed = compose_user_wgsl(source);
        let hash = hash_source(&composed);
        self.pipelines.entry(hash).or_insert_with(|| {
            match create_pipeline(device, &self.layout, &composed, self.format) {
                Some(p) => CachedPipe::Ok(p),
                None => {
                    log::warn!("shader() WGSL compile failed; using last frame or magenta");
                    CachedPipe::Failed
                }
            }
        });
        hash
    }

    fn render_job(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        textured: &TexturedQuadPipeline,
        images: &mut HashMap<String, GpuImageTexture>,
        job: crate::primitives::shader::ShaderJob,
        time_s: f32,
        pointer: [f32; 2],
    ) -> bool {
        let w = job.width.max(2);
        let h = job.height.max(2);
        let hash = self.prepare_source(device, &job.source);
        let use_fallback = matches!(self.pipelines.get(&hash), Some(CachedPipe::Failed));
        if use_fallback && self.had_good_frame.get(&job.key) == Some(&true) {
            return false;
        }
        self.ensure_target(device, textured, images, &job.key, w, h);
        let Some(gpu_img) = images.get(&job.key) else {
            return false;
        };
        let color_view = gpu_img
            ._texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let uniforms = ShaderUniforms {
            time: job.time.unwrap_or(time_s),
            _pad0: 0.0,
            resolution: [w as f32, h as f32],
            pointer: job.pointer.unwrap_or(pointer),
            _pad1: [0.0, 0.0],
            custom: job.custom,
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
        self.ensure_dummy(queue);

        let slot0_user = job.tex_key.as_deref().filter(|k| *k != job.key.as_str());
        let slot1_user = job.tex_key1.as_deref().filter(|k| *k != job.key.as_str());
        let slot2_user = job.tex_key2.as_deref().filter(|k| *k != job.key.as_str());
        let buf_on_0 = job.buffer && slot0_user.is_none();
        let buf_on_1 = job.buffer && slot0_user.is_some() && slot1_user.is_none();
        let buf_on_2 =
            job.buffer && slot0_user.is_some() && slot1_user.is_some() && slot2_user.is_none();
        if buf_on_0 || buf_on_1 || buf_on_2 {
            self.ensure_buf(device, &job.key, w, h);
        }
        let src0_owned = resolve_src_view(images, slot0_user, buf_on_0, self.buf_tex.get(&job.key));
        let src1_owned = resolve_src_view(images, slot1_user, buf_on_1, self.buf_tex.get(&job.key));
        let src2_owned = resolve_src_view(images, slot2_user, buf_on_2, self.buf_tex.get(&job.key));
        let src0 = src0_owned.as_ref().unwrap_or(&self.dummy_view);
        let src1 = src1_owned.as_ref().unwrap_or(&self.dummy_view);
        let src2 = src2_owned.as_ref().unwrap_or(&self.dummy_view);
        let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("jadix-shader-src-bg"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src0),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(src1),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(src2),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-shader-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix-shader-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
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
            let pipeline = match self.pipelines.get(&hash) {
                Some(CachedPipe::Ok(p)) => p,
                _ => &self.fallback,
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_bind_group(1, &tex_bg, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        drop(tex_bg);
        if job.buffer {
            self.copy_to_buf(device, queue, images, &job.key, w, h);
        }
        if !use_fallback {
            self.had_good_frame.insert(job.key, true);
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
            label: Some("jadix-shader-color"),
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
        self.buf_tex.remove(key);
    }

    fn ensure_buf(&mut self, device: &wgpu::Device, key: &str, w: u32, h: u32) {
        if self.buf_tex.contains_key(key) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jadix-shader-buf"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.buf_tex.insert(key.to_string(), texture);
    }

    fn copy_to_buf(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &HashMap<String, GpuImageTexture>,
        key: &str,
        w: u32,
        h: u32,
    ) {
        self.ensure_buf(device, key, w, h);
        let Some(src) = images.get(key) else {
            return;
        };
        let Some(dst) = self.buf_tex.get(key) else {
            return;
        };
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("jadix-shader-buf-copy"),
        });
        enc.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(enc.finish()));
    }

    fn ensure_dummy(&mut self, queue: &wgpu::Queue) {
        if self.dummy_ready {
            return;
        }
        let mut row = [0u8; 256];
        row[0] = 255;
        row[1] = 255;
        row[2] = 255;
        row[3] = 255;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.dummy_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &row,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.dummy_ready = true;
    }
}

pub(crate) fn compose_user_wgsl(user: &str) -> String {
    let mut s = String::with_capacity(PREAMBLE.len() + user.len() + 2);
    s.push_str(PREAMBLE);
    s.push('\n');
    s.push_str(user);
    s
}

fn hash_source(src: &str) -> u64 {
    let mut h = DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    source: &str,
    format: wgpu::TextureFormat,
) -> Option<wgpu::RenderPipeline> {
    let module = match super::wgpu_compat::try_shader_module(device, "jadix-shader-user", source) {
        Some(m) => m,
        None => return None,
    };

    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("jadix-shader-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[Some(VertexInput::desc())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        })
    })) {
        Ok(p) => p,
        Err(_) => {
            let _ = super::wgpu_compat::wait_error_scope(device, scope);
            return None;
        }
    };
    if super::wgpu_compat::wait_error_scope(device, scope).is_some() {
        return None;
    }
    Some(pipeline)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_match_wgsl_layout() {
        assert_eq!(std::mem::size_of::<ShaderUniforms>(), 96);
        assert_eq!(std::mem::offset_of!(ShaderUniforms, resolution), 8);
        assert_eq!(std::mem::offset_of!(ShaderUniforms, pointer), 16);
        assert_eq!(std::mem::offset_of!(ShaderUniforms, custom), 32);
    }

    #[test]
    fn compose_injects_vs_and_uniforms() {
        let src = compose_user_wgsl(
            r#"
            @fragment
            fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                return vec4<f32>(in.uv, u.time, 1.0);
            }
        "#,
        );
        assert!(src.contains("fn vs_main"));
        assert!(src.contains("var<uniform> u"));
        assert!(src.contains("var t_src"));
        assert!(src.contains("var t_src1"));
        assert!(src.contains("var t_src2"));
        assert!(src.contains("fn fs_main"));
    }
}
