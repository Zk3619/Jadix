use crate::{
    text_render::GlyphonCacheKey, Cache, ContentType, FontSystem, GlyphDetails,
    RasterizeCustomGlyphRequest, RasterizedCustomGlyph, State, SwashCache,
};
use etagere::{size2, Allocation, BucketedAtlasAllocator};
use lru::LruCache;
use rustc_hash::FxHasher;
use std::{collections::HashSet, hash::BuildHasherDefault};
use wgpu::{
    BindGroup, DepthStencilState, Device, Extent3d, MultisampleState, Origin3d, Queue,
    RenderPipeline, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

type Hasher = BuildHasherDefault<FxHasher>;

/// CPU pixels are allocated only when the atlas receives its first glyph. A
/// prepare uploads one dirty rectangle, rather than one driver call per glyph.
#[derive(Default)]
struct AtlasPixels {
    data: Vec<u8>,
    dirty: Option<[u32; 4]>,
}

impl AtlasPixels {
    fn write(&mut self, size: u32, channels: usize, rect: [u32; 4], pixels: &[u8]) {
        let [x, y, width, height] = rect;
        let stride = size as usize * channels;
        let row_bytes = width as usize * channels;
        self.data.resize(size as usize * stride, 0);
        for row in 0..height as usize {
            let dest = (y as usize + row) * stride + x as usize * channels;
            self.data[dest..dest + row_bytes]
                .copy_from_slice(&pixels[row * row_bytes..(row + 1) * row_bytes]);
        }
        self.dirty = Some(match self.dirty {
            None => rect,
            Some([dx, dy, dw, dh]) => {
                let left = x.min(dx);
                let top = y.min(dy);
                [
                    left,
                    top,
                    (x + width).max(dx + dw) - left,
                    (y + height).max(dy + dh) - top,
                ]
            }
        });
    }

    fn grow(&mut self, old: u32, new: u32, channels: usize) {
        if self.data.is_empty() {
            return;
        }
        let old_stride = old as usize * channels;
        let new_stride = new as usize * channels;
        let mut data = vec![0; new as usize * new_stride];
        for row in 0..old as usize {
            data[row * new_stride..row * new_stride + old_stride]
                .copy_from_slice(&self.data[row * old_stride..(row + 1) * old_stride]);
        }
        self.data = data;
        // The new GPU texture starts empty; every resident glyph must be copied.
        self.dirty = Some([0, 0, old, old]);
    }

    fn flush(&mut self, queue: &Queue, texture: &Texture, size: u32, channels: usize) {
        let Some([x, y, width, height]) = self.dirty.take() else {
            return;
        };
        let stride = size as usize * channels;
        let offset = y as usize * stride + x as usize * channels;
        queue.write_texture(
            TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: Origin3d { x, y, z: 0 },
                aspect: TextureAspect::All,
            },
            &self.data[offset..],
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride as u32),
                rows_per_image: None,
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[allow(dead_code)]
pub(crate) struct InnerAtlas {
    pub kind: Kind,
    pub texture: Texture,
    pub texture_view: TextureView,
    pub packer: BucketedAtlasAllocator,
    pub size: u32,
    pub glyph_cache: LruCache<GlyphonCacheKey, GlyphDetails, Hasher>,
    pub glyphs_in_use: HashSet<GlyphonCacheKey, Hasher>,
    pub max_texture_dimension_2d: u32,
    pixels: AtlasPixels,
}

impl InnerAtlas {
    const INITIAL_SIZE: u32 = 256;

    fn new(state: &State, kind: Kind) -> Self {
        let max_texture_dimension_2d = state.device.limits().max_texture_dimension_2d;
        let size = Self::INITIAL_SIZE.min(max_texture_dimension_2d);

        let packer = BucketedAtlasAllocator::new(size2(size as i32, size as i32));

        // Create a texture to use for our atlas
        let texture = state.device.create_texture(&TextureDescriptor {
            label: Some("glyphon atlas"),
            size: Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: kind.texture_format(),
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        let glyph_cache = LruCache::unbounded_with_hasher(Hasher::default());
        let glyphs_in_use = HashSet::with_hasher(Hasher::default());

        Self {
            kind,
            texture,
            texture_view,
            packer,
            size,
            glyph_cache,
            glyphs_in_use,
            max_texture_dimension_2d,
            pixels: AtlasPixels::default(),
        }
    }

    pub(crate) fn try_allocate(&mut self, width: usize, height: usize) -> Option<Allocation> {
        let size = size2(width as i32, height as i32);

        loop {
            let allocation = self.packer.allocate(size);

            if allocation.is_some() {
                return allocation;
            }

            // Try to free least recently used allocation
            let (mut key, mut value) = self.glyph_cache.peek_lru()?;

            // Find a glyph with an actual size
            while value.atlas_id.is_none() {
                // All sized glyphs are in use, cache is full
                if self.glyphs_in_use.contains(key) {
                    return None;
                }

                let _ = self.glyph_cache.pop_lru();

                (key, value) = self.glyph_cache.peek_lru()?;
            }

            // All sized glyphs are in use, cache is full
            if self.glyphs_in_use.contains(key) {
                return None;
            }

            let (_, value) = self.glyph_cache.pop_lru().unwrap();
            self.packer.deallocate(value.atlas_id.unwrap());
        }
    }

    pub(crate) fn grow(
        &mut self,
        state: &State,
        _font_system: &mut FontSystem,
        _cache: &mut SwashCache,
        _scale_factor: f32,
        _rasterize_custom_glyph: impl FnMut(
            RasterizeCustomGlyphRequest,
        ) -> Option<RasterizedCustomGlyph>,
    ) -> bool {
        if self.size >= self.max_texture_dimension_2d {
            return false;
        }

        // Grow each dimension by a factor of 2. The growth factor was chosen to match the growth
        // factor of `Vec`.`
        const GROWTH_FACTOR: u32 = 2;
        let new_size = (self.size * GROWTH_FACTOR).min(self.max_texture_dimension_2d);

        self.packer.grow(size2(new_size as i32, new_size as i32));

        // Create a texture to use for our atlas
        self.texture = state.device.create_texture(&TextureDescriptor {
            label: Some("glyphon atlas"),
            size: Extent3d {
                width: new_size,
                height: new_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: self.kind.texture_format(),
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        // Preserve resident pixels, including custom glyphs and the exact original
        // scale. Growth must not invoke either rasterizer again.
        self.pixels
            .grow(self.size, new_size, self.kind.num_channels());

        self.texture_view = self.texture.create_view(&TextureViewDescriptor::default());
        self.size = new_size;

        true
    }

    pub(crate) fn write_glyph(&mut self, x: u32, y: u32, width: u32, height: u32, data: &[u8]) {
        self.pixels.write(
            self.size,
            self.kind.num_channels(),
            [x, y, width, height],
            data,
        );
    }

    fn flush(&mut self, queue: &Queue) {
        self.pixels
            .flush(queue, &self.texture, self.size, self.kind.num_channels());
    }

    fn trim(&mut self) {
        self.glyphs_in_use.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Mask,
    Color { srgb: bool },
}

impl Kind {
    fn num_channels(self) -> usize {
        match self {
            Kind::Mask => 1,
            Kind::Color { .. } => 4,
        }
    }

    fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Kind::Mask => TextureFormat::R8Unorm,
            Kind::Color { srgb } => {
                if srgb {
                    TextureFormat::Rgba8UnormSrgb
                } else {
                    TextureFormat::Rgba8Unorm
                }
            }
        }
    }
}

/// The color mode of a [`TextAtlas`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Accurate color management.
    ///
    /// This mode will use a proper sRGB texture for colored glyphs. This will
    /// produce physically accurate color blending when rendering.
    Accurate,

    /// Web color management.
    ///
    /// This mode reproduces the color management strategy used in the Web and
    /// implemented by browsers.
    ///
    /// This entails storing glyphs colored using the sRGB color space in a
    /// linear RGB texture. Blending will not be physically accurate, but will
    /// produce the same results as most UI toolkits.
    ///
    /// This mode should be used to render to a linear RGB texture containing
    /// sRGB colors.
    Web,
}

/// An atlas containing a cache of rasterized glyphs that can be rendered.
pub struct TextAtlas {
    cache: Cache,
    pub(crate) bind_group: BindGroup,
    pub(crate) color_atlas: InnerAtlas,
    pub(crate) mask_atlas: InnerAtlas,
    pub(crate) format: TextureFormat,
    pub(crate) color_mode: ColorMode,
}

impl TextAtlas {
    /// Creates a new [`TextAtlas`].
    pub fn new(device: &Device, queue: &Queue, cache: &Cache, format: TextureFormat) -> Self {
        Self::with_color_mode(device, queue, cache, format, ColorMode::Accurate)
    }

    /// Creates a new [`TextAtlas`] with the given [`ColorMode`].
    pub fn with_color_mode(
        device: &Device,
        _queue: &Queue,
        cache: &Cache,
        format: TextureFormat,
        color_mode: ColorMode,
    ) -> Self {
        let state = State { device };
        let color_atlas = InnerAtlas::new(
            &state,
            Kind::Color {
                srgb: match color_mode {
                    ColorMode::Accurate => true,
                    ColorMode::Web => false,
                },
            },
        );
        let mask_atlas = InnerAtlas::new(&state, Kind::Mask);

        let bind_group = cache.create_atlas_bind_group(
            device,
            &color_atlas.texture_view,
            &mask_atlas.texture_view,
        );

        Self {
            cache: cache.clone(),
            bind_group,
            color_atlas,
            mask_atlas,
            format,
            color_mode,
        }
    }

    pub(crate) fn flush(&mut self, queue: &Queue) {
        self.mask_atlas.flush(queue);
        self.color_atlas.flush(queue);
    }

    pub fn trim(&mut self) {
        self.mask_atlas.trim();
        self.color_atlas.trim();
    }

    pub(crate) fn grow(
        &mut self,
        state: &State,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        content_type: ContentType,
        scale_factor: f32,
        rasterize_custom_glyph: impl FnMut(RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph>,
    ) -> bool {
        let did_grow = match content_type {
            ContentType::Mask => self.mask_atlas.grow(
                state,
                font_system,
                cache,
                scale_factor,
                rasterize_custom_glyph,
            ),
            ContentType::Color => self.color_atlas.grow(
                state,
                font_system,
                cache,
                scale_factor,
                rasterize_custom_glyph,
            ),
        };

        if did_grow {
            self.rebind(state.device);
        }

        did_grow
    }

    pub(crate) fn inner_for_content_mut(&mut self, content_type: ContentType) -> &mut InnerAtlas {
        match content_type {
            ContentType::Color => &mut self.color_atlas,
            ContentType::Mask => &mut self.mask_atlas,
        }
    }

    pub(crate) fn get_or_create_pipeline(
        &self,
        device: &Device,
        multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>,
    ) -> RenderPipeline {
        self.cache
            .get_or_create_pipeline(device, self.format, multisample, depth_stencil)
    }

    fn rebind(&mut self, device: &wgpu::Device) {
        self.bind_group = self.cache.create_atlas_bind_group(
            device,
            &self.color_atlas.texture_view,
            &self.mask_atlas.texture_view,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::AtlasPixels;

    #[test]
    fn batched_writes_preserve_rows_and_union_dirty_bounds() {
        let mut pixels = AtlasPixels::default();
        pixels.write(8, 1, [1, 2, 2, 2], &[1, 2, 3, 4]);
        pixels.write(8, 1, [5, 1, 1, 1], &[9]);
        assert_eq!(pixels.dirty, Some([1, 1, 5, 3]));
        assert_eq!(&pixels.data[17..19], &[1, 2]);
        assert_eq!(&pixels.data[25..27], &[3, 4]);
        assert_eq!(pixels.data[13], 9);
    }

    #[test]
    fn growth_preserves_color_pixels_and_new_writes() {
        let mut pixels = AtlasPixels::default();
        pixels.write(2, 4, [1, 1, 1, 1], &[1, 2, 3, 4]);
        pixels.grow(2, 4, 4);
        assert_eq!(&pixels.data[20..24], &[1, 2, 3, 4]);
        assert_eq!(pixels.dirty, Some([0, 0, 2, 2]));
        pixels.write(4, 4, [3, 3, 1, 1], &[5, 6, 7, 8]);
        assert_eq!(&pixels.data[20..24], &[1, 2, 3, 4]);
        assert_eq!(&pixels.data[60..64], &[5, 6, 7, 8]);
        assert_eq!(pixels.dirty, Some([0, 0, 4, 4]));
    }
}
