use crate::{
        custom_glyph::CustomGlyphCacheKey, text_transform_is_identity, ColorMode, ContentType,
        FontSystem, GlyphDetails, GlyphToRender, GpuCacheStatus, PrepareError,
        RasterizeCustomGlyphRequest, RasterizedCustomGlyph, RenderError, State, SwashCache,
        SwashContent, TextArea, TextAtlas, Viewport,
    };
use cosmic_text::{Color, SubpixelBin};
use std::slice;
use wgpu::{
    Buffer, BufferDescriptor, BufferUsages, DepthStencilState, Device, Extent3d, MultisampleState,
    Origin3d, Queue, RenderPass, RenderPipeline, TexelCopyBufferLayout, TexelCopyTextureInfo,
    TextureAspect, COPY_BUFFER_ALIGNMENT,
};

/// A text renderer that uses cached glyphs to render text into an existing render pass.
pub struct TextRenderer {
    vertex_buffer: Buffer,
    vertex_buffer_size: u64,
    pipeline: RenderPipeline,
    glyph_vertices: Vec<GlyphToRender>,
}

impl TextRenderer {
    /// Creates a new `TextRenderer`.
    pub fn new(
        atlas: &mut TextAtlas,
        device: &Device,
        multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>,
    ) -> Self {
        let vertex_buffer_size = next_copy_buffer_size(4096);
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("glyphon vertices"),
            size: vertex_buffer_size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipeline = atlas.get_or_create_pipeline(device, multisample, depth_stencil);

        Self {
            vertex_buffer,
            vertex_buffer_size,
            pipeline,
            glyph_vertices: Vec::new(),
        }
    }

    /// Prepares all of the provided text areas for rendering.
    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        atlas: &mut TextAtlas,
        viewport: &Viewport,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
        cache: &mut SwashCache,
    ) -> Result<(), PrepareError> {
        self.prepare_with_depth_and_custom(
            device,
            queue,
            font_system,
            atlas,
            viewport,
            text_areas,
            cache,
            zero_depth,
            |_| None,
        )
    }

    /// Prepares all of the provided text areas for rendering.
    pub fn prepare_with_depth<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        atlas: &mut TextAtlas,
        viewport: &Viewport,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
        cache: &mut SwashCache,
        metadata_to_depth: impl FnMut(usize) -> f32,
    ) -> Result<(), PrepareError> {
        self.prepare_with_depth_and_custom(
            device,
            queue,
            font_system,
            atlas,
            viewport,
            text_areas,
            cache,
            metadata_to_depth,
            |_| None,
        )
    }

    /// Prepares all of the provided text areas for rendering.
    pub fn prepare_with_custom<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        atlas: &mut TextAtlas,
        viewport: &Viewport,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
        cache: &mut SwashCache,
        rasterize_custom_glyph: impl FnMut(RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph>,
    ) -> Result<(), PrepareError> {
        self.prepare_with_depth_and_custom(
            device,
            queue,
            font_system,
            atlas,
            viewport,
            text_areas,
            cache,
            zero_depth,
            rasterize_custom_glyph,
        )
    }

    /// Prepares all of the provided text areas for rendering.
    pub fn prepare_with_depth_and_custom<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        atlas: &mut TextAtlas,
        viewport: &Viewport,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
        cache: &mut SwashCache,
        mut metadata_to_depth: impl FnMut(usize) -> f32,
        mut rasterize_custom_glyph: impl FnMut(
            RasterizeCustomGlyphRequest,
        ) -> Option<RasterizedCustomGlyph>,
    ) -> Result<(), PrepareError> {
        self.glyph_vertices.clear();

        let state = State { device, queue };
        let mut system = GlyphSystem {
            atlas,
            cache,
            font_system,
        };
        let resolution = viewport.resolution();

        for text_area in text_areas {
            let x_min = text_area.bounds.left.max(0);
            let y_min = text_area.bounds.top.max(0);

            let bounds = GlyphBounds {
                x: Bounds {
                    min: x_min,
                    max: text_area
                        .bounds
                        .right
                        .min(resolution.width as i32)
                        .max(x_min),
                },
                y: Bounds {
                    min: y_min,
                    max: text_area
                        .bounds
                        .bottom
                        .min(resolution.height as i32)
                        .max(y_min),
                },
            };

            // Keep 1× atlas always (no oversample path switch). Axis-aligned text uses
            // glyphon's pixel/subpixel snap for sharp small sizes; non-identity only
            // switches to float layout so VS can rotate the same glyphs.
            let xform = !text_transform_is_identity(&text_area.transform);
            let oversample = 1.0_f32;

            for glyph in text_area.custom_glyphs.iter() {
                let x_f = text_area.left + (glyph.left * text_area.scale);
                let y_f = text_area.top + (glyph.top * text_area.scale);
                let width = (glyph.width * text_area.scale * oversample).round().max(0.0) as u16;
                let height = (glyph.height * text_area.scale * oversample).round().max(0.0) as u16;

                let (pos_x, pos_y, x_bin, y_bin, float_pos) = if xform {
                    (x_f, y_f, SubpixelBin::Zero, SubpixelBin::Zero, true)
                } else if glyph.snap_to_physical_pixel {
                    (
                        x_f.round(),
                        y_f.round(),
                        SubpixelBin::Zero,
                        SubpixelBin::Zero,
                        false,
                    )
                } else {
                    let (x, x_bin) = SubpixelBin::new(x_f);
                    let (y, y_bin) = SubpixelBin::new(y_f);
                    (x as f32, y as f32, x_bin, y_bin, false)
                };

                let cache_key = GlyphonCacheKey::Custom(CustomGlyphCacheKey {
                    glyph_id: glyph.id,
                    width,
                    height,
                    x_bin,
                    y_bin,
                });

                let color = glyph.color.unwrap_or(text_area.default_color);

                if let Some(glyph_to_render) = prepare_glyph(
                    &state,
                    &mut system,
                    GlyphMetadata {
                        pos_x,
                        pos_y,
                        line_y: 0.0,
                        scale_factor: text_area.scale,
                        oversample,
                        float_pos,
                        color,
                        metadata: glyph.metadata,
                        cache_key,
                        transform: text_area.transform,
                        transform_origin: text_area.transform_origin,
                    },
                    bounds,
                    |_system, rasterize_custom_glyph| -> Option<GetGlyphImageResult> {
                        if width == 0 || height == 0 {
                            return None;
                        }

                        let input = RasterizeCustomGlyphRequest {
                            id: glyph.id,
                            width,
                            height,
                            x_bin,
                            y_bin,
                            scale: text_area.scale * oversample,
                        };

                        let output = (rasterize_custom_glyph)(input)?;

                        output.validate(&input, None);

                        Some(GetGlyphImageResult {
                            content_type: output.content_type,
                            top: 0,
                            left: 0,
                            width,
                            height,
                            data: output.data,
                        })
                    },
                    &mut metadata_to_depth,
                    &mut rasterize_custom_glyph,
                )? {
                    self.glyph_vertices.push(glyph_to_render);
                }
            }

            let is_run_visible = |run: &cosmic_text::LayoutRun| {
                let start_y_physical = (text_area.top + (run.line_top * text_area.scale)) as i32;
                let end_y_physical = start_y_physical + (run.line_height * text_area.scale) as i32;

                start_y_physical <= text_area.bounds.bottom
                    && text_area.bounds.top <= end_y_physical
            };

            let layout_runs = text_area
                .buffer
                .layout_runs()
                .skip_while(|run| !is_run_visible(run))
                .take_while(is_run_visible);

            for run in layout_runs {
                for glyph in run.glyphs.iter() {
                    let raster_scale = text_area.scale * oversample;
                    let physical_glyph =
                        glyph.physical((text_area.left, text_area.top), raster_scale);

                    let color = match glyph.color_opt {
                        Some(some) => some,
                        None => text_area.default_color,
                    };

                    let (pos_x, pos_y, float_pos) = if xform {
                        let x_offset = glyph.font_size * glyph.x_offset;
                        let y_offset = glyph.font_size * glyph.y_offset;
                        (
                            (glyph.x + x_offset) * text_area.scale + text_area.left,
                            (glyph.y - y_offset) * text_area.scale + text_area.top,
                            true,
                        )
                    } else {
                        (physical_glyph.x as f32, physical_glyph.y as f32, false)
                    };

                    if let Some(glyph_to_render) = prepare_glyph(
                        &state,
                        &mut system,
                        GlyphMetadata {
                            pos_x,
                            pos_y,
                            line_y: run.line_y,
                            color,
                            metadata: glyph.metadata,
                            cache_key: GlyphonCacheKey::Text(physical_glyph.cache_key),
                            scale_factor: text_area.scale,
                            oversample,
                            float_pos,
                            transform: text_area.transform,
                            transform_origin: text_area.transform_origin,
                        },
                        bounds,
                        |system, _rasterize_custom_glyph| -> Option<GetGlyphImageResult> {
                            let image = system
                                .cache
                                .get_image_uncached(system.font_system, physical_glyph.cache_key)?;

                            let content_type = match image.content {
                                SwashContent::Color => ContentType::Color,
                                SwashContent::Mask => ContentType::Mask,
                                SwashContent::SubpixelMask => {
                                    // Not implemented yet, but don't panic if this happens.
                                    ContentType::Mask
                                }
                            };

                            Some(GetGlyphImageResult {
                                content_type,
                                top: image.placement.top as i16,
                                left: image.placement.left as i16,
                                width: image.placement.width as u16,
                                height: image.placement.height as u16,
                                data: image.data,
                            })
                        },
                        &mut metadata_to_depth,
                        &mut rasterize_custom_glyph,
                    )? {
                        self.glyph_vertices.push(glyph_to_render);
                    }
                }
            }
        }

        let will_render = !self.glyph_vertices.is_empty();
        if !will_render {
            return Ok(());
        }

        let vertices = self.glyph_vertices.as_slice();
        let vertices_raw = unsafe {
            slice::from_raw_parts(
                vertices as *const _ as *const u8,
                std::mem::size_of_val(vertices),
            )
        };

        if self.vertex_buffer_size >= vertices_raw.len() as u64 {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_raw);
        } else {
            self.vertex_buffer.destroy();

            let (buffer, buffer_size) = create_oversized_buffer(
                device,
                Some("glyphon vertices"),
                vertices_raw,
                BufferUsages::VERTEX | BufferUsages::COPY_DST,
            );

            self.vertex_buffer = buffer;
            self.vertex_buffer_size = buffer_size;
        }

        Ok(())
    }

    /// Renders all layouts that were previously provided to `prepare`.
    pub fn render(
        &self,
        atlas: &TextAtlas,
        viewport: &Viewport,
        pass: &mut RenderPass<'_>,
    ) -> Result<(), RenderError> {
        if self.glyph_vertices.is_empty() {
            return Ok(());
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &atlas.bind_group, &[]);
        pass.set_bind_group(1, &viewport.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..4, 0..self.glyph_vertices.len() as u32);

        Ok(())
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TextColorConversion {
    None = 0,
    ConvertToLinear = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GlyphonCacheKey {
    Text(cosmic_text::CacheKey),
    Custom(CustomGlyphCacheKey),
}

fn next_copy_buffer_size(size: u64) -> u64 {
    let align_mask = COPY_BUFFER_ALIGNMENT - 1;
    ((size.next_power_of_two() + align_mask) & !align_mask).max(COPY_BUFFER_ALIGNMENT)
}

fn create_oversized_buffer(
    device: &Device,
    label: Option<&str>,
    contents: &[u8],
    usage: BufferUsages,
) -> (Buffer, u64) {
    let size = next_copy_buffer_size(contents.len() as u64);
    let buffer = device.create_buffer(&BufferDescriptor {
        label,
        size,
        usage,
        mapped_at_creation: true,
    });
    buffer
        .slice(..contents.len() as u64)
        .get_mapped_range_mut()
        .unwrap()
        .copy_from_slice(contents);
    buffer.unmap();
    (buffer, size)
}

fn zero_depth(_: usize) -> f32 {
    0f32
}

struct GetGlyphImageResult {
    content_type: ContentType,
    top: i16,
    left: i16,
    width: u16,
    height: u16,
    data: Vec<u8>,
}

struct GlyphMetadata {
    pos_x: f32,
    pos_y: f32,
    line_y: f32,
    scale_factor: f32,
    /// 1.0 normally; 2.0 when rasterizing for a non-identity transform.
    oversample: f32,
    /// Skip integer snap; keep subpixel layout for transformed quads.
    float_pos: bool,
    color: Color,
    metadata: usize,
    cache_key: GlyphonCacheKey,
    transform: [f32; 4],
    transform_origin: [f32; 2],
}

#[derive(Clone, Copy)]
struct Bounds {
    min: i32,
    max: i32,
}

#[derive(Clone, Copy)]
struct GlyphBounds {
    x: Bounds,
    y: Bounds,
}

struct GlyphSystem<'a> {
    atlas: &'a mut TextAtlas,
    cache: &'a mut SwashCache,
    font_system: &'a mut FontSystem,
}

fn prepare_glyph<R>(
    state: &State,
    system: &mut GlyphSystem,
    metadata: GlyphMetadata,
    bounds: GlyphBounds,
    get_glyph_image: impl FnOnce(&mut GlyphSystem, &mut R) -> Option<GetGlyphImageResult>,
    mut metadata_to_depth: impl FnMut(usize) -> f32,
    mut rasterize_custom_glyph: R,
) -> Result<Option<GlyphToRender>, PrepareError>
where
    R: FnMut(RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph>,
{
    let details =
        if let Some(details) = system.atlas.mask_atlas.glyph_cache.get(&metadata.cache_key) {
            system
                .atlas
                .mask_atlas
                .glyphs_in_use
                .insert(metadata.cache_key);
            details
        } else if let Some(details) = system
            .atlas
            .color_atlas
            .glyph_cache
            .get(&metadata.cache_key)
        {
            system
                .atlas
                .color_atlas
                .glyphs_in_use
                .insert(metadata.cache_key);
            details
        } else {
            let Some(image) = (get_glyph_image)(system, &mut rasterize_custom_glyph) else {
                return Ok(None);
            };

            let should_rasterize = image.width > 0 && image.height > 0;

            let (gpu_cache, atlas_id, inner) = if should_rasterize {
                let mut inner = system.atlas.inner_for_content_mut(image.content_type);

                // Find a position in the packer
                let allocation = loop {
                    match inner.try_allocate(image.width as usize, image.height as usize) {
                        Some(a) => break a,
                        None => {
                            if !system.atlas.grow(
                                state,
                                system.font_system,
                                system.cache,
                                image.content_type,
                                metadata.scale_factor,
                                &mut rasterize_custom_glyph,
                            ) {
                                return Err(PrepareError::AtlasFull);
                            }

                            inner = system.atlas.inner_for_content_mut(image.content_type);
                        }
                    }
                };
                let atlas_min = allocation.rectangle.min;

                state.queue.write_texture(
                    TexelCopyTextureInfo {
                        texture: &inner.texture,
                        mip_level: 0,
                        origin: Origin3d {
                            x: atlas_min.x as u32,
                            y: atlas_min.y as u32,
                            z: 0,
                        },
                        aspect: TextureAspect::All,
                    },
                    &image.data,
                    TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(image.width as u32 * inner.num_channels() as u32),
                        rows_per_image: None,
                    },
                    Extent3d {
                        width: image.width as u32,
                        height: image.height as u32,
                        depth_or_array_layers: 1,
                    },
                );

                (
                    GpuCacheStatus::InAtlas {
                        x: atlas_min.x as u16,
                        y: atlas_min.y as u16,
                        content_type: image.content_type,
                    },
                    Some(allocation.id),
                    inner,
                )
            } else {
                let inner = &mut system.atlas.color_atlas;
                (GpuCacheStatus::SkipRasterization, None, inner)
            };

            inner.glyphs_in_use.insert(metadata.cache_key);
            // Insert the glyph into the cache and return the details reference
            inner
                .glyph_cache
                .get_or_insert(metadata.cache_key, || GlyphDetails {
                    width: image.width,
                    height: image.height,
                    gpu_cache,
                    atlas_id,
                    top: image.top,
                    left: image.left,
                })
        };

    let inv_os = if metadata.oversample > 1.0 {
        1.0 / metadata.oversample
    } else {
        1.0
    };
    let left_px = details.left as f32 * inv_os;
    let top_px = details.top as f32 * inv_os;

    let (mut x, mut y) = if metadata.float_pos {
        (
            metadata.pos_x + left_px,
            metadata.line_y * metadata.scale_factor + metadata.pos_y - top_px,
        )
    } else {
        (
            metadata.pos_x + details.left as f32,
            (metadata.line_y * metadata.scale_factor).round() + metadata.pos_y - details.top as f32,
        )
    };

    let (mut atlas_x, mut atlas_y, content_type) = match details.gpu_cache {
        GpuCacheStatus::InAtlas { x, y, content_type } => (x, y, content_type),
        GpuCacheStatus::SkipRasterization => return Ok(None),
    };

    let atlas_w = details.width as f32;
    let atlas_h = details.height as f32;
    let mut width = atlas_w * inv_os;
    let mut height = atlas_h * inv_os;
    let mut uv_w = details.width;
    let mut uv_h = details.height;

    // Starts beyond right edge or ends beyond left edge
    let max_x = x + width;
    if x > bounds.x.max as f32 || max_x < bounds.x.min as f32 {
        return Ok(None);
    }

    // Starts beyond bottom edge or ends beyond top edge
    let max_y = y + height;
    if y > bounds.y.max as f32 || max_y < bounds.y.min as f32 {
        return Ok(None);
    }

    // Clip left edge
    if x < bounds.x.min as f32 {
        let right_shift = bounds.x.min as f32 - x;
        let atlas_shift = (right_shift * metadata.oversample).round().max(0.0) as u16;

        x = bounds.x.min as f32;
        width = max_x - bounds.x.min as f32;
        atlas_x = atlas_x.saturating_add(atlas_shift);
        uv_w = (uv_w as f32 - atlas_shift as f32).max(0.0) as u16;
    }

    // Clip right edge
    if x + width > bounds.x.max as f32 {
        let new_w = bounds.x.max as f32 - x;
        let shrink = width - new_w;
        width = new_w;
        uv_w = (uv_w as f32 - shrink * metadata.oversample)
            .round()
            .max(0.0) as u16;
    }

    // Clip top edge
    if y < bounds.y.min as f32 {
        let bottom_shift = bounds.y.min as f32 - y;
        let atlas_shift = (bottom_shift * metadata.oversample).round().max(0.0) as u16;

        y = bounds.y.min as f32;
        height = max_y - bounds.y.min as f32;
        atlas_y = atlas_y.saturating_add(atlas_shift);
        uv_h = (uv_h as f32 - atlas_shift as f32).max(0.0) as u16;
    }

    // Clip bottom edge
    if y + height > bounds.y.max as f32 {
        let new_h = bounds.y.max as f32 - y;
        let shrink = height - new_h;
        height = new_h;
        uv_h = (uv_h as f32 - shrink * metadata.oversample)
            .round()
            .max(0.0) as u16;
    }

    if width <= 0.0 || height <= 0.0 || uv_w == 0 || uv_h == 0 {
        return Ok(None);
    }

    let depth = metadata_to_depth(metadata.metadata);

    Ok(Some(GlyphToRender {
        pos: [x, y],
        dim: [width.round().max(1.0) as u16, height.round().max(1.0) as u16],
        uv: [atlas_x, atlas_y],
        color: metadata.color.0,
        content_type_with_srgb: [
            content_type as u16,
            match system.atlas.color_mode {
                ColorMode::Accurate => TextColorConversion::ConvertToLinear,
                ColorMode::Web => TextColorConversion::None,
            } as u16,
        ],
        depth,
        transform: metadata.transform,
        transform_origin: metadata.transform_origin,
        uv_size: [uv_w, uv_h],
        _pad: [0, 0],
    }))
}
