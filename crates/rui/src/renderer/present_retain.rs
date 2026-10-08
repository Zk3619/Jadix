//! Retained color buffer for partial `LoadOp::Load` + scissor.
//!
//! Swapchain images (FLIP_DISCARD / GLES) do not preserve prior frame contents, so
//! Load+scissor directly on the surface leaves undefined/black pixels outside the
//! damage rect. UI is drawn into this texture; each present blits the full buffer
//! to the surface. Scroll may copy a still-visible overlap (`copy_scroll_strip`)
//! through a scratch texture before the Load pass.

use wgpu::util::DeviceExt;

use super::pipeline::VertexInput;

struct RetainRt {
    /// Kept alive so `view` / bind group remain valid. `COPY_DST` for strip blit.
    texture: wgpu::Texture,
    /// Ping-pong for overlapping `copy_texture_to_texture` (same-texture copies are invalid).
    scratch: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// Offscreen present target + fullscreen blit to the swapchain.
pub struct PresentRetainPipeline {
    pipeline: wgpu::RenderPipeline,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    retain: Option<RetainRt>,
    format: wgpu::TextureFormat,
    /// True when `retain` holds a complete frame (safe to `LoadOp::Load`).
    /// Cleared after surface-direct FullClear so the next PartialLoad warms the buffer.
    contents_valid: bool,
}

impl PresentRetainPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Present Retain Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("present_retain.wgsl").into()),
        });

        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Present Retain Tex BGL"),
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
            label: Some("Present Retain PL"),
            bind_group_layouts: &[Some(&tex_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Present Retain Blit Pipeline"),
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Present Retain Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // Same UV orientation as backdrop blur capture / TexturedQuadPipeline.
        let verts: &[VertexInput] = &[
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
        let indices: &[u16] = &[0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Present Retain VB"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Present Retain IB"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            tex_bgl,
            sampler,
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            retain: None,
            format,
            contents_valid: false,
        }
    }

    #[inline]
    pub fn contents_valid(&self) -> bool {
        self.contents_valid && self.retain.is_some()
    }

    /// After a surface-direct FullClear the retain buffer is stale.
    pub fn invalidate_contents(&mut self) {
        self.contents_valid = false;
    }

    pub fn mark_contents_valid(&mut self) {
        self.contents_valid = self.retain.is_some();
    }

    /// Ensure the retain buffer matches `width`×`height`. Returns `true` when the texture
    /// was (re)created and prior contents are invalid — caller must FullClear this frame.
    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(rt) = &self.retain {
            if rt.width == width && rt.height == height {
                return false;
            }
        }
        self.contents_valid = false;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Present Retain Color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Present Retain BG"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let scratch = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Present Retain Strip Scratch"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.retain = Some(RetainRt {
            texture,
            scratch,
            view,
            bind_group,
            width,
            height,
        });
        true
    }

    pub fn color_view(&self) -> Option<&wgpu::TextureView> {
        self.retain.as_ref().map(|r| &r.view)
    }

    #[cfg(test)]
    pub(crate) fn color_texture_for_test(&self) -> Option<&wgpu::Texture> {
        self.retain.as_ref().map(|r| &r.texture)
    }

    /// Copy the still-visible overlap to its new origin via scratch (may overlap).
    /// Returns `false` when the retain buffer is missing or the rects leave the texture.
    pub fn copy_scroll_strip(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src_x: u32,
        src_y: u32,
        dst_x: u32,
        dst_y: u32,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(rt) = self.retain.as_ref() else {
            return false;
        };
        if width == 0 || height == 0 {
            return false;
        }
        let src_ok =
            src_x.saturating_add(width) <= rt.width && src_y.saturating_add(height) <= rt.height;
        let dst_ok =
            dst_x.saturating_add(width) <= rt.width && dst_y.saturating_add(height) <= rt.height;
        if !src_ok || !dst_ok || width > rt.width || height > rt.height {
            return false;
        }
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &rt.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: src_x,
                    y: src_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &rt.scratch,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &rt.scratch,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &rt.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst_x,
                    y: dst_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            extent,
        );
        true
    }

    /// Blit the full retain buffer onto the swapchain view (always Clear → full image).
    ///
    /// `overlay` runs in the same pass after the blit (I-beam / input caret). GLES
    /// cannot `Load` the swapchain, so caret cannot be a second pass.
    pub fn blit_to_surface<F>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
        clear: wgpu::Color,
        overlay: F,
    ) where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let Some(rt) = self.retain.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Present Retain → Surface"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: surface_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &rt.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.num_indices, 0, 0..1);
        overlay(&mut pass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::InstanceInput;
    use std::time::Instant;
    use wgpu::util::DeviceExt;

    fn dest_view(
        gpu: &OffscreenGpu,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("retain-blit-dest"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        (tex, view)
    }

    fn time_blit_ms(
        gpu: &OffscreenGpu,
        retain: &mut PresentRetainPipeline,
        w: u32,
        h: u32,
        frames: u32,
    ) -> f32 {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        retain.ensure_size(&gpu.device, w, h);
        let (_tex, view) = dest_view(gpu, format, w, h);
        let clear = wgpu::Color::BLACK;
        for _ in 0..3 {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            retain.blit_to_surface(&mut encoder, &view, clear, |_| {});
            gpu.queue.submit(Some(encoder.finish()));
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        }
        let t = Instant::now();
        for _ in 0..frames {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            retain.blit_to_surface(&mut encoder, &view, clear, |_| {});
            gpu.queue.submit(Some(encoder.finish()));
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        }
        t.elapsed().as_secs_f32() * 1000.0 / frames as f32
    }

    fn report_blit(label: &str, gpu: &OffscreenGpu) -> (f32, f32, f32) {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut retain = PresentRetainPipeline::new(&gpu.device, format);
        let frames = 20u32;
        let ms_800 = time_blit_ms(gpu, &mut retain, 800, 600, frames);
        let ms_1080 = time_blit_ms(gpu, &mut retain, 1920, 1080, frames);
        let ms_1440 = time_blit_ms(gpu, &mut retain, 2560, 1440, frames);
        eprintln!(
            "present_retain blit {label} ({:?} {}): 800x600={ms_800:.2}ms 1920x1080={ms_1080:.2}ms 2560x1440={ms_1440:.2}ms",
            gpu.adapter_info.backend, gpu.adapter_info.name
        );
        (ms_800, ms_1080, ms_1440)
    }

    /// Gallery maximize stutter: every present full-blits retain → swapchain
    /// (FLIP_DISCARD). Gallery on Windows is GL-first; D3D12 offscreen numbers
    /// do not represent the live path.
    #[test]
    fn present_retain_full_blit_cost_grows_with_window() {
        let Some(gpu) = OffscreenGpu::new() else {
            eprintln!("skip present_retain_full_blit_cost_grows_with_window: no wgpu adapter");
            return;
        };
        let (_a, ms_1080, ms_1440) = report_blit("default", &gpu);
        if let Some(gl) = OffscreenGpu::with_backends(wgpu::Backends::GL) {
            let (_ga, gl_1080, gl_1440) = report_blit("gl", &gl);
            assert!(
                gl_1080 < 8.0,
                "GL 1080p retain→surface blit {gl_1080:.2}ms (1440p={gl_1440:.2}ms) \
                 would drop frames after maximize on the gallery GL path"
            );
        }
        assert!(
            ms_1080 < 8.0,
            "1080p retain→surface blit {ms_1080:.2}ms (1440p={ms_1440:.2}ms) \
             would drop frames after maximize; blit is full-window regardless of damage"
        );
    }

    fn solid_inst(w: f32, h: f32, radii: f32, blur: f32) -> InstanceInput {
        InstanceInput {
            rect_pos: [0.0, 0.0],
            rect_size: [w, h],
            color: [0.15, 0.15, 0.18, 1.0],
            border_color: [0.0; 4],
            radii: [radii; 4],
            border_width: 0.0,
            shadow_color: if blur > 0.0 {
                [0.0, 0.0, 0.0, 0.25]
            } else {
                [0.0; 4]
            },
            shadow_offset: [0.0, 4.0],
            shadow_blur: blur,
            shadow_spread: 0.0,
            shadow_inset: 0.0,
            transform: crate::style::transform::MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 0.0,
            backdrop_blur: 0.0,
        }
    }

    #[test]
    fn simple_fill_vertex_color_decode_matches_small_sdf_and_alpha_blend() {
        let Some(gpu) = OffscreenGpu::new() else {
            return;
        };
        for alpha in [1.0, 0.5] {
            let mut big = solid_inst(64.0, 64.0, 0.0, 0.0);
            big.rect_pos = [4.0, 4.0];
            big.color = [0.15, 0.4, 0.8, alpha];
            let mut small = big;
            small.rect_pos = [80.0, 4.0];
            small.rect_size = [24.0, 24.0];
            let img = gpu
                .render_instances_readback(112, 72, wgpu::Color::WHITE, &[big, small])
                .expect("color readback");
            let big_pixel = img.pixel(24, 24).unwrap();
            let small_pixel = img.pixel(92, 16).unwrap();
            assert!(
                big_pixel
                    .iter()
                    .zip(small_pixel)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "vertex decode and fragment decode must agree: {big_pixel:?} vs {small_pixel:?}"
            );
            if alpha == 1.0 {
                for (channel, expected) in big_pixel[..3].iter().zip([38_u8, 102, 204]) {
                    assert!(
                        channel.abs_diff(expected) <= 1,
                        "display sRGB must round-trip once"
                    );
                }
            }
            assert_eq!(
                img.pixel(110, 70),
                Some([255; 4]),
                "paint stays inside its geometry"
            );
        }
    }

    fn time_sdf_ms(
        gpu: &OffscreenGpu,
        w: u32,
        h: u32,
        instances: &[InstanceInput],
        frames: u32,
    ) -> f32 {
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let pipeline = crate::renderer::RuiPipeline::from_format(&gpu.device, format, w, h);
        pipeline.update_resolution(&gpu.queue, w, h);
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sdf-fill-dest"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let ib = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sdf-fill-inst"),
                contents: bytemuck::cast_slice(instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let draw = |encoder: &mut wgpu::CommandEncoder| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sdf-fill"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline.render_pipeline);
            pass.set_bind_group(0, &pipeline.bind_group, &[]);
            pass.set_bind_group(1, pipeline.dummy_backdrop(), &[]);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, ib.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..pipeline.num_indices, 0, 0..instances.len() as u32);
        };
        for _ in 0..3 {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            draw(&mut encoder);
            gpu.queue.submit(Some(encoder.finish()));
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        }
        let t = Instant::now();
        for _ in 0..frames {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            draw(&mut encoder);
            gpu.queue.submit(Some(encoder.finish()));
            let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        }
        t.elapsed().as_secs_f32() * 1000.0 / frames as f32
    }

    /// Gallery maximize: same overlapping card stack, stretched to the window.
    /// SDF + box-shadow fill on Intel GL is the work that scales with pixels.
    #[test]
    fn rounded_rect_fill_cost_grows_with_window() {
        let Some(gpu) = OffscreenGpu::with_backends(wgpu::Backends::GL).or_else(OffscreenGpu::new)
        else {
            eprintln!("skip rounded_rect_fill_cost_grows_with_window: no wgpu adapter");
            return;
        };
        let frames = 20u32;
        let layers = 8usize;
        let simple_800: Vec<_> = (0..layers)
            .map(|_| solid_inst(800.0, 600.0, 0.0, 0.0))
            .collect();
        let simple_1080: Vec<_> = (0..layers)
            .map(|_| solid_inst(1920.0, 1080.0, 0.0, 0.0))
            .collect();
        let card_800: Vec<_> = (0..layers)
            .map(|i| {
                let mut inst = solid_inst(720.0, 120.0, 12.0, 16.0);
                inst.rect_pos = [40.0, 20.0 + i as f32 * 70.0];
                inst
            })
            .collect();
        let card_1080: Vec<_> = (0..layers)
            .map(|i| {
                let mut inst = solid_inst(1760.0, 140.0, 12.0, 16.0);
                inst.rect_pos = [80.0, 40.0 + i as f32 * 90.0];
                inst
            })
            .collect();
        let s800 = time_sdf_ms(&gpu, 800, 600, &simple_800, frames);
        let s1080 = time_sdf_ms(&gpu, 1920, 1080, &simple_1080, frames);
        let c800 = time_sdf_ms(&gpu, 800, 600, &card_800, frames);
        let c1080 = time_sdf_ms(&gpu, 1920, 1080, &card_1080, frames);
        eprintln!(
            "sdf fill 8x overlay ({:?} {}): simple 800={s800:.2}ms 1080={s1080:.2}ms | shadow+radius 800={c800:.2}ms 1080={c1080:.2}ms",
            gpu.adapter_info.backend, gpu.adapter_info.name
        );
        assert!(
            s1080 < 6.0,
            "1080p 8-layer simple fill {s1080:.2}ms (800x600={s800:.2}ms) — stacked 100% backgrounds \
             after maximize must stay well under vsync (was ~11ms of sd_shaped_box on Intel GL)"
        );
        assert!(
            c1080 < 8.0,
            "1080p 8 gallery-sized rounded+shadow cards {c1080:.2}ms (800x600={c800:.2}ms)"
        );
    }
}
