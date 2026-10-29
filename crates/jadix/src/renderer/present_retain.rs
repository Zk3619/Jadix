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
    scratch_view: wgpu::TextureView,
    scratch_bind_group: wgpu::BindGroup,
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
    compositor_encode: bool,
}

pub(crate) fn surface_needs_compositor_encoding(config: &wgpu::SurfaceConfiguration) -> bool {
    cfg!(windows)
        && config.alpha_mode == wgpu::CompositeAlphaMode::PreMultiplied
        && config.format.is_srgb()
}

impl PresentRetainPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::with_compositor_encoding(device, format, false)
    }

    pub(crate) fn new_for_surface(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        Self::with_compositor_encoding(
            device,
            config.format,
            surface_needs_compositor_encoding(config),
        )
    }

    fn with_compositor_encoding(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        compositor_encode: bool,
    ) -> Self {
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
                entry_point: Some(if compositor_encode {
                    "fs_compositor"
                } else {
                    "fs_main"
                }),
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
            compositor_encode,
        }
    }

    pub(crate) fn needs_compositor_encoding(&self) -> bool {
        self.compositor_encode
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
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scratch_view = scratch.create_view(&wgpu::TextureViewDescriptor::default());
        let scratch_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Present Retain Scratch BG"),
            layout: &self.tex_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scratch_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.retain = Some(RetainRt {
            texture,
            scratch,
            scratch_view,
            scratch_bind_group,
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

    pub(crate) fn color_texture(&self) -> Option<&wgpu::Texture> {
        self.retain.as_ref().map(|r| &r.texture)
    }

    /// Replace final sampling when format/extent match and no overlay or theme fade is needed.
    pub(crate) fn copy_to_surface(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface: &wgpu::Texture,
    ) -> bool {
        if self.compositor_encode {
            return false;
        }
        let Some(source) = self.color_texture() else {
            return false;
        };
        if source.format() != surface.format()
            || source.size() != surface.size()
            || source.sample_count() != 1
            || surface.sample_count() != 1
            || !surface.usage().contains(wgpu::TextureUsages::COPY_DST)
        {
            return false;
        }
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            surface.as_image_copy(),
            source.size(),
        );
        true
    }

    #[cfg(test)]
    pub(crate) fn color_texture_for_test(&self) -> Option<&wgpu::Texture> {
        self.retain.as_ref().map(|r| &r.texture)
    }

    /// Copy the still-visible overlap to its new origin via scratch (may overlap).
    /// Returns `false` when the retain buffer is missing or the rects leave the texture.
    pub fn copy_scroll_strip(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        src_x: u32,
        src_y: u32,
        dst_x: u32,
        dst_y: u32,
        width: u32,
        height: u32,
    ) -> bool {
        let Some(rt) = self.retain.as_mut() else {
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
        // Large overlaps used to travel to scratch and back. Copy the rest of
        // the frame unchanged, copy the moved overlap once, then swap targets.
        // For small overlaps the original two-copy path moves fewer pixels.
        if u64::from(width) * u64::from(height) * 2 > u64::from(rt.width) * u64::from(rt.height) {
            let rectangles = [
                (0, 0, rt.width, dst_y),
                (0, dst_y + height, rt.width, rt.height - dst_y - height),
                (0, dst_y, dst_x, height),
                (dst_x + width, dst_y, rt.width - dst_x - width, height),
                (dst_x, dst_y, width, height),
            ];
            for (i, &(x, y, w, h)) in rectangles.iter().enumerate() {
                if w == 0 || h == 0 {
                    continue;
                }
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &rt.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: if i == 4 { src_x } else { x },
                            y: if i == 4 { src_y } else { y },
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &rt.scratch,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            }
            std::mem::swap(&mut rt.texture, &mut rt.scratch);
            std::mem::swap(&mut rt.view, &mut rt.scratch_view);
            std::mem::swap(&mut rt.bind_group, &mut rt.scratch_bind_group);
            return true;
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
#[path = "tests/present_retain.rs"]
mod tests;
