//! Caller-owned GPU textures with explicit color/alpha contracts and CPU fallbacks.
use super::*;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Texture RGB interpretation after the texture's automatic format decoding.
/// Srgb means encoded display sRGB in an Unorm texture; sRGB texture formats
/// already decode to linear and therefore use LinearSrgb here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuImageColor {
    Srgb,
    LinearSrgb,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuImageAlpha {
    Straight,
    Premultiplied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuImageError {
    InvalidTexture,
    SizeMismatch,
    UnsupportedFormat,
    BudgetExceeded,
    NotRegistered,
    RenderFailed,
    Scene(crate::SceneError),
}
impl std::fmt::Display for GpuImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GPU image: {self:?}")
    }
}
impl std::error::Error for GpuImageError {}
#[derive(Clone)]
pub(super) struct ExternalImage {
    owner: Weak<crate::image::ImageData>,
    texture: wgpu::Texture,
    color: GpuImageColor,
    alpha: GpuImageAlpha,
    revision: u64,
    region: [u32; 4],
    mip_limits: Option<crate::MipmapLimits>,
}
#[derive(Default)]
pub(super) struct ExternalImages {
    pub entries: HashMap<u64, ExternalImage>,
    serial: u64,
}
impl Clone for ExternalImages {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            serial: self.serial,
        }
    }
}
impl PathGpu {
    /// Register a texture on this renderer and all its nested layer renderers.
    /// Source usage must contain TEXTURE_BINDING or COPY_SRC (copied on GPU).
    /// GPU data stays on GPU. CPU Canvas fallback is explicitly transparent.
    /// For a meaningful CPU fallback, use override_image with a real Image2D.
    /// At most 16M pixels/image, 64M registered pixels in total and 4096 live
    /// registrations. Same device is required. CPU fallback storage counts pixels.
    pub fn register_texture(
        &mut self,
        texture: wgpu::Texture,
        color: GpuImageColor,
        alpha: GpuImageAlpha,
    ) -> Result<Image2D, GpuImageError> {
        validate(&texture, color)?;
        let pixels = texture.width() as usize * texture.height() as usize;
        if pixels > 16_777_216 {
            return Err(GpuImageError::BudgetExceeded);
        }
        let fallback = Image2D::from_rgba8(texture.width(), texture.height(), &vec![0; pixels * 4])
            .ok_or(GpuImageError::InvalidTexture)?;
        self.override_image(&fallback, texture, color, alpha)?;
        Ok(fallback)
    }
    /// Replace GPU sampling of an immutable image; CPU sampling keeps its pixels.
    /// Changes become visible on the next encode. Existing submitted commands
    /// retain their GPU resources; unsubmitted frame encoders must be submitted
    /// before replacing/dirtying registrations used by them.
    pub fn override_image(
        &mut self,
        image: &Image2D,
        texture: wgpu::Texture,
        color: GpuImageColor,
        alpha: GpuImageAlpha,
    ) -> Result<(), GpuImageError> {
        let region = [0, 0, image.width(), image.height()];
        if texture.width() != image.width() || texture.height() != image.height() {
            return Err(GpuImageError::SizeMismatch);
        }
        self.override_image_region(image, texture, region, color, alpha, None)
    }
    /// Import an independent source rectangle. Coarse levels never include
    /// neighboring atlas entries. None keeps level zero; Some explicitly bounds
    /// the converted FP16 base plus GPU mip chain (not the caller source texture).
    /// CPU fallback is transparent; dirty notifications update this region only.
    pub fn register_texture_region(
        &mut self,
        texture: wgpu::Texture,
        region: [u32; 4],
        color: GpuImageColor,
        alpha: GpuImageAlpha,
        mip_limits: Option<crate::MipmapLimits>,
    ) -> Result<Image2D, GpuImageError> {
        validate(&texture, color)?;
        validate_region(&texture, region)?;
        gpu_mip_levels([region[2], region[3]], mip_limits)?;
        let pixels = region[2] as usize * region[3] as usize;
        if pixels > 16_777_216 {
            return Err(GpuImageError::BudgetExceeded);
        }
        let fallback = Image2D::from_rgba8(region[2], region[3], &vec![0; pixels * 4])
            .ok_or(GpuImageError::InvalidTexture)?;
        self.override_image_region(&fallback, texture, region, color, alpha, mip_limits)?;
        Ok(fallback)
    }
    /// Region counterpart of override_image. All validation/budget failures leave
    /// previous registrations intact. Mip generation is GPU-only and lazy at encode.
    pub fn override_image_region(
        &mut self,
        image: &Image2D,
        texture: wgpu::Texture,
        region: [u32; 4],
        color: GpuImageColor,
        alpha: GpuImageAlpha,
        mip_limits: Option<crate::MipmapLimits>,
    ) -> Result<(), GpuImageError> {
        validate(&texture, color)?;
        validate_region(&texture, region)?;
        if [image.width(), image.height()] != [region[2], region[3]] {
            return Err(GpuImageError::SizeMismatch);
        }
        gpu_mip_levels([region[2], region[3]], mip_limits)?;
        // Account the entire retained source texture, even if only a crop is used.
        let registry = Arc::make_mut(&mut self.external);
        registry.entries.retain(|_, v| v.owner.strong_count() > 0);
        let total = registry
            .entries
            .iter()
            .filter(|(id, _)| **id != image.0.id)
            .map(|(_, v)| v.texture.width() as u64 * v.texture.height() as u64)
            .sum::<u64>();
        if total + texture.width() as u64 * texture.height() as u64 > 67_108_864
            || (registry.entries.len() >= 4096 && !registry.entries.contains_key(&image.0.id))
        {
            return Err(GpuImageError::BudgetExceeded);
        }
        registry.serial = registry.serial.wrapping_add(1);
        registry.entries.insert(
            image.0.id,
            ExternalImage {
                owner: Arc::downgrade(&image.0),
                texture,
                color,
                alpha,
                region,
                mip_limits,
                revision: registry.serial,
            },
        );
        self.clear_image_cache();
        Ok(())
    }
    /// Request/reconfigure mips for an existing GPU registration. Uses the same
    /// byte/level contract as region imports; None removes the generated chain.
    pub fn set_registered_texture_mipmaps(
        &mut self,
        image: &Image2D,
        limits: Option<crate::MipmapLimits>,
    ) -> Result<(), GpuImageError> {
        let entry = self
            .external
            .entries
            .get(&image.0.id)
            .ok_or(GpuImageError::NotRegistered)?;
        gpu_mip_levels([entry.region[2], entry.region[3]], limits)?;
        let registry = Arc::make_mut(&mut self.external);
        registry.serial = registry.serial.wrapping_add(1);
        let entry = registry.entries.get_mut(&image.0.id).unwrap();
        entry.mip_limits = limits;
        entry.revision = registry.serial;
        self.clear_image_cache();
        Ok(())
    }
    /// Call after changing GPU texture contents. The next encode reconverts it
    /// once per renderer/cache slot, without any CPU readback or image upload.
    pub fn mark_override_image_dirty(&mut self, image: &Image2D) -> Result<(), GpuImageError> {
        let registry = Arc::make_mut(&mut self.external);
        if !registry.entries.contains_key(&image.0.id) {
            return Err(GpuImageError::NotRegistered);
        }
        registry.serial = registry.serial.wrapping_add(1);
        registry.entries.get_mut(&image.0.id).unwrap().revision = registry.serial;
        // Conversion views used as layer masks may be cached inside compositors.
        // Invalidate those bindings when replacing the backing conversion texture.
        self.clear_image_cache();
        Ok(())
    }
    /// Remove this renderer's override, including nested groups; later GPU draws
    /// use the original CPU image (transparent for register_texture handles).
    pub fn unregister_texture(&mut self, image: &Image2D) -> bool {
        let removed = Arc::make_mut(&mut self.external)
            .entries
            .remove(&image.0.id)
            .is_some();
        if removed {
            self.clear_image_cache();
        }
        removed
    }
    /// Encode a prepared scene to a sampleable GPU texture and register it as an
    /// Image2D. Returned image has transparent CPU fallback; no readback occurs.
    /// Uses this renderer's target format, sample count and quality. This method
    /// submits before registration; it is safe to draw the image in a later frame.
    pub fn render_to_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &crate::PreparedSceneGpu,
    ) -> Result<Image2D, GpuImageError> {
        let size = scene.size();
        if size[0] as u64 * size[1] as u64 > 16_777_216
            || size
                .iter()
                .any(|v| *v > device.limits().max_texture_dimension_2d)
        {
            return Err(GpuImageError::BudgetExceeded);
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("prepared-scene-image"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        if !self.render_premultiplied(
            device,
            queue,
            &texture.create_view(&Default::default()),
            size,
            scene.ops(),
        ) {
            return Err(GpuImageError::RenderFailed);
        }
        self.register_texture(
            texture,
            GpuImageColor::LinearSrgb,
            GpuImageAlpha::Premultiplied,
        )
    }
    /// Release GPU registrations whose Image2D handles are no longer retained.
    /// Live registrations are not evicted implicitly; callers unregister them.
    pub fn prune_registered_textures(&mut self) -> usize {
        let dead = self
            .external
            .entries
            .values()
            .filter(|v| v.owner.strong_count() == 0)
            .count();
        if dead > 0 {
            Arc::make_mut(&mut self.external)
                .entries
                .retain(|_, v| v.owner.strong_count() > 0);
            self.clear_image_cache();
        }
        dead
    }
    pub fn registered_texture_count(&self) -> usize {
        self.external
            .entries
            .values()
            .filter(|v| v.owner.strong_count() > 0)
            .count()
    }
    pub(super) fn prepare_external(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        image: &Image2D,
        mask: bool,
    ) -> Option<bool> {
        let source = self.external.entries.get(&image.0.id)?.clone();
        if self
            .textures
            .get(&(image.0.id, mask))
            .is_some_and(|v| v.external_revision == Some(source.revision))
        {
            return Some(true);
        }
        if source.texture.width() > device.limits().max_texture_dimension_2d
            || source.texture.height() > device.limits().max_texture_dimension_2d
        {
            return Some(false);
        }
        let format = if mask {
            wgpu::TextureFormat::R8Unorm
        } else {
            wgpu::TextureFormat::Rgba16Float
        };
        let levels = if mask {
            1
        } else {
            match gpu_mip_levels([image.width(), image.height()], source.mip_limits) {
                Ok(v) => v,
                Err(_) => return Some(false),
            }
        };
        let importer = self
            .external_import
            .get_or_insert_with(|| Import::new(device, self.options.pipeline_cache.clone()));
        let key = (mask, source.color, source.alpha);
        if !importer.pipelines.contains_key(&key) {
            importer.pipelines.insert(
                key,
                importer.pipeline(device, format, mask, source.color, source.alpha),
            );
        }
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("external-image-linear-snapshot"),
            size: wgpu::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | if !mask && super::mips::MipGenerator::supports_compute(device) {
                    wgpu::TextureUsages::STORAGE_BINDING
                } else {
                    wgpu::TextureUsages::empty()
                },
            view_formats: &[],
        });
        let copied = if source
            .texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            None
        } else {
            let copy = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("external-copy-source"),
                size: source.texture.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: source.texture.format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            encoder.copy_texture_to_texture(
                source.texture.as_image_copy(),
                copy.as_image_copy(),
                source.texture.size(),
            );
            self.stats.texture_allocations += 1;
            Some(copy)
        };
        let sampled = copied.as_ref().unwrap_or(&source.texture);
        let input = sampled.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });
        let output = target.create_view(&wgpu::TextureViewDescriptor {
            mip_level_count: Some(1),
            ..Default::default()
        });
        let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("external-image-source"),
            layout: &importer.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input),
            }],
        });
        let region_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("import-region"),
            contents: bytemuck::cast_slice(&source.region),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let region_binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &importer.region_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: region_buffer.as_entire_binding(),
            }],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("external-image-convert"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output,
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
            pass.set_pipeline(&importer.pipelines[&key]);
            pass.set_bind_group(0, &binding, &[]);
            pass.set_bind_group(1, &region_binding, &[]);
            pass.draw(0..3, 0..1);
        }
        if levels > 1 {
            if self.mip_generator.is_none() {
                self.mip_generator = Some(super::mips::MipGenerator::new(
                    device,
                    self.options.pipeline_cache.as_ref(),
                ));
            }
            self.mip_generator
                .as_ref()
                .unwrap()
                .encode(device, encoder, &target, &mut self.stats);
        }
        let output = target.create_view(&Default::default());
        let mask_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("external-image-mask"),
            layout: &self.resolve_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&output),
            }],
        });
        let bind = |filter| {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            });
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("external-image-sample"),
                layout: &self.image_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&output),
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
                owner: Arc::downgrade(&image.0),
                _texture: target,
                mask_bg,
                nearest_bg,
                linear_bg,
                mask,
                external_revision: Some(source.revision),
            },
        );
        self.stats.texture_allocations += 1;
        self.stats.bind_group_allocations += if mask { 3 } else { 5 };
        self.stats.buffer_allocations += 1;
        self.stats.uploaded_bytes += 16;
        self.stats.uniform_uploads += 1;
        self.stats.external_image_conversions += 1;
        self.stats.draw_calls += 1;
        Some(true)
    }
}
fn validate(texture: &wgpu::Texture, color: GpuImageColor) -> Result<(), GpuImageError> {
    if texture.dimension() != wgpu::TextureDimension::D2
        || texture.sample_count() != 1
        || texture.depth_or_array_layers() != 1
        || !texture
            .usage()
            .intersects(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC)
    {
        return Err(GpuImageError::InvalidTexture);
    }
    if !matches!(
        texture.format(),
        wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::Rgba16Float
    ) {
        return Err(GpuImageError::UnsupportedFormat);
    }
    if color == GpuImageColor::Srgb
        && matches!(
            texture.format(),
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
        )
    {
        return Err(GpuImageError::UnsupportedFormat);
    }
    Ok(())
}
pub(super) struct Import {
    cache: Option<wgpu::PipelineCache>,
    layout: wgpu::BindGroupLayout,
    region_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    shader: wgpu::ShaderModule,
    pipelines: HashMap<(bool, GpuImageColor, GpuImageAlpha), wgpu::RenderPipeline>,
}
impl Import {
    fn new(device: &wgpu::Device, cache: Option<wgpu::PipelineCache>) -> Self {
        let layout = texture_layout(device, "external-image-source");
        let region_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("import-region"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout), Some(&region_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("external-image-convert"),
            source: wgpu::ShaderSource::Wgsl(include_str!("external.wgsl").into()),
        });
        Self {
            cache,
            layout,
            region_layout,
            pipeline_layout,
            shader,
            pipelines: HashMap::new(),
        }
    }
    fn pipeline(
        &self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        mask: bool,
        color: GpuImageColor,
        alpha: GpuImageAlpha,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("external-image-convert"),
            layout: Some(&self.pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some(if mask { "mask" } else { "color" }),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[
                        (
                            "decode_srgb",
                            if color == GpuImageColor::Srgb { 1. } else { 0. },
                        ),
                        (
                            "premultiply",
                            if alpha == GpuImageAlpha::Straight {
                                1.
                            } else {
                                0.
                            },
                        ),
                    ],
                    ..Default::default()
                },
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: self.cache.as_ref(),
        })
    }
}

fn validate_region(texture: &wgpu::Texture, r: [u32; 4]) -> Result<(), GpuImageError> {
    if r[2] == 0
        || r[3] == 0
        || r[0].checked_add(r[2]).is_none_or(|v| v > texture.width())
        || r[1].checked_add(r[3]).is_none_or(|v| v > texture.height())
    {
        Err(GpuImageError::InvalidTexture)
    } else {
        Ok(())
    }
}
fn gpu_mip_levels(
    size: [u32; 2],
    limits: Option<crate::MipmapLimits>,
) -> Result<u32, GpuImageError> {
    let Some(limits) = limits else { return Ok(1) };
    if limits.max_bytes == 0
        || limits.max_bytes > 1024 * 1024 * 1024
        || limits.max_levels == 0
        || limits.max_levels > 32
    {
        return Err(GpuImageError::BudgetExceeded);
    }
    let mut size = size;
    let mut levels = 1;
    let mut bytes = size[0] as u64 * size[1] as u64 * 8;
    while size != [1, 1] && levels < limits.max_levels {
        size = size.map(|v| (v / 2).max(1));
        bytes += size[0] as u64 * size[1] as u64 * 8;
        levels += 1;
    }
    if bytes > limits.max_bytes as u64 {
        return Err(GpuImageError::BudgetExceeded);
    }
    Ok(levels)
}
