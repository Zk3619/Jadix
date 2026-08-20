use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::OnceLock;

use super::font_loader;

use glyphon::{
    ContentType, FontSystem, RasterizeCustomGlyphRequest, RasterizedCustomGlyph, SwashCache,
    TextArea, TextAtlas, TextRenderer, Viewport,
};
use wgpu::{Device, Queue, RenderPass, SurfaceConfiguration};

pub(crate) struct ShapedTextEntry {
    /// Full cache key (content + style + bounds). Exact hit skips reshape.
    pub key_hash: u64,
    /// Content/style identity without leaf width/height (iced `Difference::Shape` key).
    /// Same `shape_hash` + different bounds → cheap `set_size` reflow only.
    pub shape_hash: u64,
    pub buffer: glyphon::Buffer,
    pub measured_text_w: f32,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct EmojiCacheKey {
    pub sequence: String,
    pub pixel_size: u16,
    pub scale_bits: u32,
    pub color_mode: u8,
    /// Quantized wrap width (`scaled_w / 8`, same as `node_shape_hash`).
    /// CustomGlyph `left`/`top` follow soft-wrap, so a width-only reshape must miss.
    pub wrap_width_q: u16,
}

#[derive(Default)]
pub(crate) struct EmojiGlyphCache {
    pub entries: HashMap<EmojiCacheKey, EmojiGlyphEntry>,
    entry_lru: VecDeque<EmojiCacheKey>,
    pub raster_entries: HashMap<EmojiRasterCacheKey, EmojiRaster>,
    raster_lru: VecDeque<EmojiRasterCacheKey>,
    /// Stable id → raster. Glyphon panics if `rasterize` returns `None` after `Some`
    /// for the same request; node LRU must not drop these.
    id_rasters: HashMap<u16, EmojiRaster>,
    pub next_custom_id: u16,
    pub stats: EmojiCacheStats,
}

fn emoji_raster_cache_max() -> usize {
    static MAX: OnceLock<usize> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("RUI_EMOJI_RASTER_CACHE_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(64)
    })
}

fn emoji_node_cache_max() -> usize {
    static MAX: OnceLock<usize> = OnceLock::new();
    *MAX.get_or_init(|| {
        std::env::var("RUI_EMOJI_NODE_CACHE_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&n| n > 0)
            .unwrap_or(32)
    })
}

impl EmojiGlyphCache {
    pub(crate) fn get_raster_cached(&mut self, key: &EmojiRasterCacheKey) -> Option<EmojiRaster> {
        let v = self.raster_entries.get(key).cloned()?;
        if let Some(pos) = self.raster_lru.iter().position(|k| *k == *key) {
            let k = self.raster_lru.remove(pos).unwrap();
            self.raster_lru.push_back(k);
        }
        Some(v)
    }

    pub(crate) fn insert_raster(&mut self, key: EmojiRasterCacheKey, value: EmojiRaster) {
        let max = emoji_raster_cache_max();
        if self.raster_entries.contains_key(&key) {
            self.raster_entries.insert(key.clone(), value);
            if let Some(pos) = self.raster_lru.iter().position(|k| *k == key) {
                let k = self.raster_lru.remove(pos).unwrap();
                self.raster_lru.push_back(k);
            }
            return;
        }
        while self.raster_entries.len() >= max {
            if let Some(old) = self.raster_lru.pop_front() {
                self.raster_entries.remove(&old);
            } else {
                break;
            }
        }
        self.raster_lru.push_back(key.clone());
        self.raster_entries.insert(key, value);
    }

    pub(crate) fn insert_emoji_node(&mut self, key: EmojiCacheKey, entry: EmojiGlyphEntry) {
        for (id, raster) in &entry.rasters {
            self.id_rasters.insert(*id, raster.clone());
        }
        let max = emoji_node_cache_max();
        if self.entries.contains_key(&key) {
            self.entries.insert(key.clone(), entry);
            if let Some(pos) = self.entry_lru.iter().position(|k| *k == key) {
                let k = self.entry_lru.remove(pos).unwrap();
                self.entry_lru.push_back(k);
            }
            return;
        }
        while self.entries.len() >= max {
            if let Some(old) = self.entry_lru.pop_front() {
                self.entries.remove(&old);
            } else {
                break;
            }
        }
        self.entry_lru.push_back(key.clone());
        self.entries.insert(key, entry);
    }

    /// Reuse custom-glyph ids/rasters from any wrap-width sibling of `key` (same text/size).
    pub(crate) fn find_wrap_sibling(&self, key: &EmojiCacheKey) -> Option<&EmojiGlyphEntry> {
        self.entries.iter().find_map(|(k, entry)| {
            (k.sequence == key.sequence
                && k.pixel_size == key.pixel_size
                && k.scale_bits == key.scale_bits
                && k.color_mode == key.color_mode
                && k.wrap_width_q != key.wrap_width_q)
                .then_some(entry)
        })
    }

    pub(crate) fn resident_counts(&self) -> (usize, usize, usize) {
        (
            self.entries.len(),
            self.raster_entries.len(),
            self.id_rasters.len(),
        )
    }

    pub fn rasterize(&self, input: RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph> {
        let try_raster = |r: &EmojiRaster| -> Option<RasterizedCustomGlyph> {
            if r.width == input.width && r.height == input.height {
                Some(RasterizedCustomGlyph {
                    data: (*r.rgba).to_vec(),
                    content_type: ContentType::Color,
                })
            } else {
                None
            }
        };
        if let Some(r) = self.id_rasters.get(&input.id) {
            if let Some(out) = try_raster(r) {
                return Some(out);
            }
        }
        for entry in self.entries.values() {
            if let Some(r) = entry.rasters.get(&input.id) {
                if let Some(out) = try_raster(r) {
                    return Some(out);
                }
            }
        }
        // Glyphon asserts Some→None never happens for the same request. If an id was
        // handed out but the bitmap is gone/mismatched, return a transparent placeholder.
        let n = (input.width as usize)
            .saturating_mul(input.height as usize)
            .saturating_mul(4);
        Some(RasterizedCustomGlyph {
            data: vec![0; n],
            content_type: ContentType::Color,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct EmojiRasterCacheKey {
    pub sequence: String,
    pub pixel_size: u16,
}

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct EmojiCacheStats {
    pub lookups: u64,
    pub hits: u64,
    pub uploads: u64,
    pub upload_bytes: u64,
    pub svg_attempts: u64,
    pub svg_hits: u64,
    pub svg_raster_ms: f64,
    pub svg_fallback_to_png: u64,
}

pub(crate) struct EmojiGlyphEntry {
    pub glyphs: Vec<glyphon::CustomGlyph>,
    pub rasters: HashMap<u16, EmojiRaster>,
}

#[derive(Clone)]
pub(crate) struct EmojiRaster {
    pub width: u16,
    pub height: u16,
    pub rgba: Arc<[u8]>,
}

impl EmojiRaster {
    pub(crate) fn new(width: u16, height: u16, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba: Arc::from(rgba.into_boxed_slice()),
        }
    }
}

fn has_table(face: &ttf_parser::Face<'_>, tag: &[u8; 4]) -> bool {
    let t = ttf_parser::Tag::from_bytes(tag);
    face.raw_face().table(t).is_some()
}

fn log_color_font_capabilities(db: &glyphon::fontdb::Database) {
    let mut color_faces = 0usize;
    for face in db.faces() {
        let info = db.with_face_data(face.id, |bytes, index| {
            let parsed = ttf_parser::Face::parse(bytes, index).ok()?;
            let has_colr = has_table(&parsed, b"COLR");
            let has_cpal = has_table(&parsed, b"CPAL");
            let has_cbdt = has_table(&parsed, b"CBDT");
            let has_sbix = has_table(&parsed, b"sbix");
            Some((has_colr, has_cpal, has_cbdt, has_sbix))
        });
        let Some(Some((has_colr, has_cpal, has_cbdt, has_sbix))) = info else {
            continue;
        };
        if has_colr || has_cpal || has_cbdt || has_sbix {
            color_faces += 1;
            log::debug!(
                "color-font face families={:?} post_script={:?} COLR={} CPAL={} CBDT={} SBIX={}",
                face.families,
                face.post_script_name,
                has_colr,
                has_cpal,
                has_cbdt,
                has_sbix
            );
        }
    }
    log::info!("detected {} color-font capable faces", color_faces);
}

pub struct RuiTextRenderer {
    /// Points at [`Self::owned_font_system`] or the primary window's FontSystem.
    /// `Copy` raw pointer so call sites can borrow atlas/cache disjointly via `unsafe { &mut *fs }`.
    pub(crate) font_system: *mut FontSystem,
    owned_font_system: Option<Box<FontSystem>>,
    pub swash_cache: SwashCache,
    pub text_atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub viewport: Viewport,
    pub(crate) shaped_cache: Vec<ShapedTextEntry>,
    pub(crate) text_area_capacity_hint: usize,
    pub(crate) emoji_cache: EmojiGlyphCache,
    pub(crate) pending_warmup: Vec<usize>,
}

impl RuiTextRenderer {
    #[inline]
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        unsafe { &mut *self.font_system }
    }

    #[inline]
    pub fn font_system(&self) -> &FontSystem {
        unsafe { &*self.font_system }
    }

    /// Whether this renderer owns its FontSystem (primary) or shares the primary's.
    pub fn shares_font_system(&self) -> bool {
        self.owned_font_system.is_none()
    }

    pub fn new(device: &Device, queue: &Queue, config: &SurfaceConfiguration) -> Self {
        let mut owned = Box::new(font_loader::create_font_system());
        log_color_font_capabilities(owned.db());
        crate::debug_stack::crumb("RuiTextRenderer::new after color-font log");
        let ptr = &mut *owned as *mut FontSystem;
        let out = Self::with_font_ptr(device, queue, config, ptr, Some(owned));
        crate::debug_stack::crumb("RuiTextRenderer::new done");
        out
    }

    /// Secondary windows: shape/render against the primary [`FontSystem`] (same as measure TLS).
    pub fn new_sharing_font_system(
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        shared: *mut FontSystem,
    ) -> Self {
        debug_assert!(!shared.is_null());
        Self::with_font_ptr(device, queue, config, shared, None)
    }

    fn with_font_ptr(
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        font_system: *mut FontSystem,
        owned_font_system: Option<Box<FontSystem>>,
    ) -> Self {
        crate::debug_stack::crumb("with_font_ptr: SwashCache");
        let swash_cache = SwashCache::new();
        crate::debug_stack::crumb("with_font_ptr: glyphon Cache");
        let cache = glyphon::Cache::new(device);
        crate::debug_stack::crumb("with_font_ptr: TextAtlas::new");
        let mut text_atlas = TextAtlas::new(device, queue, &cache, config.format);
        crate::debug_stack::crumb("with_font_ptr: TextRenderer::new");
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        crate::debug_stack::crumb("with_font_ptr: Viewport::new");
        let viewport = Viewport::new(device, &cache);
        crate::debug_stack::crumb("with_font_ptr: done");

        Self {
            font_system,
            owned_font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            viewport,
            shaped_cache: Vec::new(),
            text_area_capacity_hint: 0,
            emoji_cache: EmojiGlyphCache::default(),
            pending_warmup: Vec::new(),
        }
    }

    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        text_areas: impl Iterator<Item = TextArea<'a>>,
    ) {
        self.viewport.update(
            queue,
            glyphon::Resolution {
                width: config.width,
                height: config.height,
            },
        );

        let font_system = self.font_system;
        if let Err(err) = self.text_renderer.prepare_with_custom(
            device,
            queue,
            unsafe { &mut *font_system },
            &mut self.text_atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
            |input| self.emoji_cache.rasterize(input),
        ) {
            log::error!("glyphon TextRenderer::prepare failed: {err:?}");
        }
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut RenderPass<'pass>) {
        if let Err(err) = self
            .text_renderer
            .render(&self.text_atlas, &self.viewport, render_pass)
        {
            log::error!("glyphon TextRenderer::render failed: {err:?}");
        }
    }

    /// Clear per-frame `glyphs_in_use` so glyphon can LRU-evict atlas slots not
    /// referenced on the next prepare (see glyphon `hello-world` example).
    pub(crate) fn trim_glyph_atlas(&mut self) {
        self.text_atlas.trim();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphon::RasterizeCustomGlyphRequest;

    fn sample_raster(w: u16, h: u16) -> EmojiRaster {
        EmojiRaster::new(w, h, vec![10; (w as usize) * (h as usize) * 4])
    }

    #[test]
    fn rasterize_survives_node_lru_eviction() {
        std::env::set_var("RUI_EMOJI_NODE_CACHE_MAX", "1");
        let mut cache = EmojiGlyphCache::default();
        let key_a = EmojiCacheKey {
            sequence: "a".into(),
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        };
        let mut rasters = HashMap::new();
        rasters.insert(7, sample_raster(16, 16));
        cache.insert_emoji_node(
            key_a,
            EmojiGlyphEntry {
                glyphs: vec![],
                rasters,
            },
        );
        // Evict A by inserting B (max=1).
        let key_b = EmojiCacheKey {
            sequence: "b".into(),
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        };
        cache.insert_emoji_node(
            key_b,
            EmojiGlyphEntry {
                glyphs: vec![],
                rasters: HashMap::new(),
            },
        );
        assert!(!cache.entries.contains_key(&EmojiCacheKey {
            sequence: "a".into(),
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        }));
        let out = cache.rasterize(RasterizeCustomGlyphRequest {
            id: 7,
            width: 16,
            height: 16,
            x_bin: glyphon::SubpixelBin::Zero,
            y_bin: glyphon::SubpixelBin::Zero,
            scale: 1.0,
        });
        assert!(
            out.is_some(),
            "glyphon requires Some after Some — id_rasters must outlive node LRU"
        );
        assert_eq!(out.unwrap().data.len(), 16 * 16 * 4);
    }

    #[test]
    fn rasterize_never_returns_none() {
        let cache = EmojiGlyphCache::default();
        let out = cache.rasterize(RasterizeCustomGlyphRequest {
            id: 99,
            width: 8,
            height: 8,
            x_bin: glyphon::SubpixelBin::Zero,
            y_bin: glyphon::SubpixelBin::Zero,
            scale: 1.0,
        });
        assert!(out.is_some());
        assert_eq!(out.unwrap().data.len(), 8 * 8 * 4);
    }
}
