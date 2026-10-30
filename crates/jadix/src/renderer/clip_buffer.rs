use crate::style::GpuClipPoly;
use std::borrow::Cow;

const TEXELS_PER_CLIP: u32 = (std::mem::size_of::<GpuClipPoly>() / 16) as u32;
const CLIPS_PER_ROW: u32 = 32;

pub(crate) enum ClipBuffer {
    Storage(super::buffer_upload::RetainedBuffer),
    Texture {
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        bytes: Vec<u8>,
    },
}

impl ClipBuffer {
    pub(crate) fn new(device: &wgpu::Device, label: &'static str) -> Self {
        if device.limits().max_storage_buffers_per_shader_stage > 0 {
            return Self::Storage(super::buffer_upload::RetainedBuffer::new(
                device,
                label,
                wgpu::BufferUsages::STORAGE,
                bytemuck::bytes_of(&GpuClipPoly::NONE),
            ));
        }
        let texture = Self::texture(device, 1);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self::Texture {
            texture,
            view,
            bytes: Vec::new(),
        }
    }

    fn texture(device: &wgpu::Device, height: u32) -> wgpu::Texture {
        assert!(
            height <= device.limits().max_texture_dimension_2d,
            "clip texture exceeds device capacity"
        );
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Clip polygon texture"),
            size: wgpu::Extent3d {
                width: TEXELS_PER_CLIP * CLIPS_PER_ROW,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        polys: &[GpuClipPoly],
    ) -> bool {
        let none = [GpuClipPoly::NONE];
        let polys = if polys.is_empty() { &none[..] } else { polys };
        let source = bytemuck::cast_slice(polys);
        match self {
            Self::Storage(buffer) => buffer.upload(device, queue, source),
            Self::Texture {
                texture,
                view,
                bytes,
            } => {
                if bytes == source {
                    return false;
                }
                let height = (polys.len() as u32)
                    .div_ceil(CLIPS_PER_ROW)
                    .next_power_of_two();
                let grew = height > texture.height();
                if grew {
                    *texture = Self::texture(device, height);
                    *view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                }
                let row_bytes = TEXELS_PER_CLIP * CLIPS_PER_ROW * 16;
                let mut padded = vec![0u8; row_bytes as usize * height as usize];
                padded[..source.len()].copy_from_slice(source);
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &padded,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row_bytes),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width: TEXELS_PER_CLIP * CLIPS_PER_ROW,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                bytes.clear();
                bytes.extend_from_slice(source);
                grew
            }
        }
    }

    pub(crate) fn binding(&self) -> wgpu::BindingResource<'_> {
        match self {
            Self::Storage(buffer) => buffer.buffer.as_entire_binding(),
            Self::Texture { view, .. } => wgpu::BindingResource::TextureView(view),
        }
    }

    pub(crate) fn binding_type(device: &wgpu::Device) -> wgpu::BindingType {
        if device.limits().max_storage_buffers_per_shader_stage > 0 {
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            }
        } else {
            wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Uint,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            }
        }
    }
}

pub(crate) fn shader_source<'source>(
    device: &wgpu::Device,
    source: &'source str,
) -> Cow<'source, str> {
    if device.limits().max_storage_buffers_per_shader_stage > 0 {
        return Cow::Borrowed(source);
    }
    Cow::Owned(
        source
            .replace("\r\n", "\n")
            .replace(
                "@group(0) @binding(1)\nvar<storage, read> clip_polys: array<GpuClipPoly>;",
                include_str!("clip_texture.wgsl"),
            )
            .replace("arrayLength(&clip_polys)", "clip_poly_count()")
            .replace("clip_polys[inst_id].count2", "clip_texel(inst_id, 0u).z")
            .replace("clip_polys[inst_id].fill2", "clip_texel(inst_id, 0u).w")
            .replace("clip_polys[inst_id].count", "clip_texel(inst_id, 0u).x")
            .replace("clip_polys[inst_id].fill", "clip_texel(inst_id, 0u).y")
            .replace(
                "clip_polys[inst_id].round_rect2",
                "bitcast<vec4<f32>>(clip_texel(inst_id, 19u))",
            )
            .replace(
                "clip_polys[inst_id].round_radii2",
                "bitcast<vec4<f32>>(clip_texel(inst_id, 20u))",
            )
            .replace(
                "clip_polys[inst_id].round_rect",
                "bitcast<vec4<f32>>(clip_texel(inst_id, 17u))",
            )
            .replace(
                "clip_polys[inst_id].round_radii",
                "bitcast<vec4<f32>>(clip_texel(inst_id, 18u))",
            )
            .replace(
                "clip_polys[inst_id].pts2[index]",
                "clip_texture_point(inst_id, index, true)",
            )
            .replace(
                "clip_polys[inst_id].pts[index]",
                "clip_texture_point(inst_id, index, false)",
            ),
    )
}

#[cfg(all(test, windows, feature = "backend-dx12"))]
#[path = "tests/clip_buffer.rs"]
mod tests;
