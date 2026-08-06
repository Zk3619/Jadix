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
    pub key_hash: u64,
    pub buffer: glyphon::Buffer,
    pub measured_text_w: f32,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct EmojiCacheKey {
    pub sequence: String,
    pub pixel_size: u16,
    pub scale_bits: u32,
    pub color_mode: u8,
}

#[derive(Default)]
pub(crate) struct EmojiGlyphCache {
    pub entries: HashMap<EmojiCacheKey, EmojiGlyphEntry>,
    entry_lru: VecDeque<EmojiCacheKey>,
    pub raster_entries: HashMap<EmojiRasterCacheKey, EmojiRaster>,
    raster_lru: VecDeque<EmojiRasterCacheKey>,
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

    pub fn rasterize(&self, input: RasterizeCustomGlyphRequest) -> Option<RasterizedCustomGlyph> {
        for entry in self.entries.values() {
            if let Some(r) = entry.rasters.get(&input.id) {
                if r.width == input.width && r.height == input.height {
                    return Some(RasterizedCustomGlyph {
                        data: (*r.rgba).to_vec(),
                        content_type: ContentType::Color,
                    });
                }
            }
        }
        None
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
    pub font_system: FontSystem,
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
    pub fn new(device: &Device, queue: &Queue, config: &SurfaceConfiguration) -> Self {
        let font_system = font_loader::create_font_system();
        log_color_font_capabilities(font_system.db());

        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let mut text_atlas = TextAtlas::new(device, queue, &cache, config.format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        let viewport = Viewport::new(device, &cache);

        Self {
            font_system,
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

        if let Err(err) = self.text_renderer.prepare_with_custom(
            device,
            queue,
            &mut self.font_system,
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
}
