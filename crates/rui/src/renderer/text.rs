use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
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
    /// Cached from last Miss/reshape so Exact scroll frames skip `text_contains_emoji`.
    pub has_emoji: bool,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct EmojiCacheKey {
    /// Content/style identity (`shape_hash`) — avoids cloning full emoji strings every scroll frame.
    pub sequence_hash: u64,
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

    /// Resolve glyphs for paint: exact wrap key, else any wrap-width sibling (same text/size).
    pub(crate) fn glyphs_for(&self, key: &EmojiCacheKey) -> &[glyphon::CustomGlyph] {
        self.entries
            .get(key)
            .or_else(|| self.find_wrap_sibling(key))
            .map_or(&[], |e| e.glyphs.as_slice())
    }

    /// Reuse custom-glyph ids/rasters from any wrap-width sibling of `key` (same text/size).
    pub(crate) fn find_wrap_sibling(&self, key: &EmojiCacheKey) -> Option<&EmojiGlyphEntry> {
        self.entries.iter().find_map(|(k, entry)| {
            (k.sequence_hash == key.sequence_hash
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
        let try_exact = |r: &EmojiRaster| -> Option<RasterizedCustomGlyph> {
            if r.width == input.width && r.height == input.height {
                Some(RasterizedCustomGlyph {
                    data: (*r.rgba).to_vec(),
                    content_type: ContentType::Color,
                })
            } else {
                None
            }
        };
        let try_any = |r: &EmojiRaster| -> Option<RasterizedCustomGlyph> {
            if let Some(exact) = try_exact(r) {
                return Some(exact);
            }
            // Size mismatch (e.g. paint quad ≠ cached Twemoji square): nearest-neighbor
            // resize instead of a transparent placeholder that blanks every emoji.
            if r.width == 0 || r.height == 0 || input.width == 0 || input.height == 0 {
                return None;
            }
            let dw = input.width as usize;
            let dh = input.height as usize;
            let sw = r.width as usize;
            let sh = r.height as usize;
            let mut data = vec![0u8; dw * dh * 4];
            for y in 0..dh {
                let sy = y * sh / dh;
                for x in 0..dw {
                    let sx = x * sw / dw;
                    let si = (sy * sw + sx) * 4;
                    let di = (y * dw + x) * 4;
                    data[di..di + 4].copy_from_slice(&r.rgba[si..si + 4]);
                }
            }
            Some(RasterizedCustomGlyph {
                data,
                content_type: ContentType::Color,
            })
        };
        if let Some(r) = self.id_rasters.get(&input.id) {
            if let Some(out) = try_any(r) {
                return Some(out);
            }
        }
        for entry in self.entries.values() {
            if let Some(r) = entry.rasters.get(&input.id) {
                if let Some(out) = try_any(r) {
                    return Some(out);
                }
            }
        }
        // Glyphon asserts Some→None never happens for the same request. If an id was
        // handed out but the bitmap is gone, return a transparent placeholder.
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
    /// Device-local shader/layout/pipeline cache. Atlases and draw vertices stay
    /// per renderer so preparing an overlay cannot overwrite the page mesh.
    gpu_cache: glyphon::Cache,
    /// Points at [`Self::owned_font_system`] or the primary window's FontSystem.
    /// `Copy` raw pointer so call sites can borrow atlas/cache disjointly via `unsafe { &mut *fs }`.
    pub(crate) font_system: *mut FontSystem,
    owned_font_system: Option<Box<FontSystem>>,
    /// CPU rasters are shared only with renderers using this FontSystem. GPU
    /// atlas slots and meshes remain independent, including custom emoji ids.
    pub swash_cache: Rc<RefCell<SwashCache>>,
    pub text_atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub viewport: Viewport,
    pub(crate) shaped_cache: Vec<ShapedTextEntry>,
    /// Band-cull / clip-strip reuse: FIFO queues keyed by `shape_hash`.
    /// Each prepare parks live `shaped_cache` here, then reclaims by identity
    /// (in-vec swap painted button captions inside Inputs).
    pub(crate) shaped_identity_pool: HashMap<u64, VecDeque<ShapedTextEntry>>,
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
        log::debug!("RuiTextRenderer::new after color-font log");
        let ptr = &mut *owned as *mut FontSystem;
        let out = Self::with_font_ptr(
            device,
            queue,
            config,
            ptr,
            Some(owned),
            glyphon::Cache::new(device),
            Rc::new(RefCell::new(SwashCache::new())),
        );
        log::debug!("RuiTextRenderer::new done");
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
        Self::with_font_ptr(
            device,
            queue,
            config,
            shared,
            None,
            glyphon::Cache::new(device),
            Rc::new(RefCell::new(SwashCache::new())),
        )
    }

    /// Same-device page/capture/overlay bands share expensive immutable GPU
    /// resources. Callers using a different device must use the font-only API.
    pub(crate) fn new_sharing_resources(
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        source: &Self,
    ) -> Self {
        Self::with_font_ptr(
            device,
            queue,
            config,
            source.font_system,
            None,
            source.gpu_cache.clone(),
            source.swash_cache.clone(),
        )
    }

    fn with_font_ptr(
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        font_system: *mut FontSystem,
        owned_font_system: Option<Box<FontSystem>>,
        cache: glyphon::Cache,
        swash_cache: Rc<RefCell<SwashCache>>,
    ) -> Self {
        log::debug!("with_font_ptr: glyphon Cache");
        log::debug!("with_font_ptr: TextAtlas::new");
        let mut text_atlas = TextAtlas::new(device, queue, &cache, config.format);
        log::debug!("with_font_ptr: TextRenderer::new");
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        log::debug!("with_font_ptr: Viewport::new");
        let viewport = Viewport::new(device, &cache);
        log::debug!("with_font_ptr: done");

        Self {
            gpu_cache: cache,
            font_system,
            owned_font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            viewport,
            shaped_cache: Vec::new(),
            shaped_identity_pool: HashMap::new(),
            text_area_capacity_hint: 0,
            emoji_cache: EmojiGlyphCache::default(),
            pending_warmup: Vec::new(),
        }
    }

    pub fn prepare_resolution<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        text_areas: impl Iterator<Item = TextArea<'a>>,
    ) {
        self.viewport
            .update(queue, glyphon::Resolution { width, height });
        let font_system = self.font_system;
        if let Err(err) = self.text_renderer.prepare_with_custom(
            device,
            queue,
            unsafe { &mut *font_system },
            &mut self.text_atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache.borrow_mut(),
            |input| self.emoji_cache.rasterize(input),
        ) {
            log::error!("glyphon TextRenderer::prepare failed: {err:?}");
        }
    }

    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        config: &SurfaceConfiguration,
        text_areas: impl Iterator<Item = TextArea<'a>>,
    ) {
        self.prepare_resolution(device, queue, config.width, config.height, text_areas);
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

    #[test]
    #[ignore = "native GPU glyph cache pixel comparison; run explicitly"]
    fn shared_cpu_glyph_rasters_preserve_independent_meshes() {
        use super::super::offscreen::{readback_texture, OffscreenGpu};
        let gpu = OffscreenGpu::with_backends(wgpu::Backends::GL).expect("native GL GPU");
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: 512,
            height: 128,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let mut primary = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
        let mut shared =
            RuiTextRenderer::new_sharing_resources(&gpu.device, &gpu.queue, &config, &primary);
        let mut independent = RuiTextRenderer::new_sharing_font_system(
            &gpu.device,
            &gpu.queue,
            &config,
            primary.font_system,
        );
        assert!(Rc::ptr_eq(&primary.swash_cache, &shared.swash_cache));
        assert!(
            primary.swash_cache.borrow().image_cache.is_empty(),
            "construction must not rasterize glyphs"
        );
        let mut buffer =
            glyphon::Buffer::new(primary.font_system_mut(), glyphon::Metrics::new(22.0, 30.0));
        buffer.set_size(Some(500.0), Some(120.0));
        buffer.set_text(
            "Gallery 字形缓存 ABC xyz",
            &glyphon::Attrs::new(),
            glyphon::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(primary.font_system_mut(), false);
        let area = |top| glyphon::TextArea {
            buffer: &buffer,
            left: 3.25,
            top,
            scale: 1.0,
            bounds: glyphon::TextBounds {
                left: 0,
                top: 0,
                right: 512,
                bottom: 128,
            },
            default_color: glyphon::Color::rgb(180, 220, 255),
            custom_glyphs: &[],
        };
        primary.prepare(&gpu.device, &gpu.queue, &config, std::iter::once(area(0.0)));
        let entries = primary.swash_cache.borrow().image_cache.len();
        assert!(entries > 0);
        shared.prepare(
            &gpu.device,
            &gpu.queue,
            &config,
            std::iter::once(area(45.0)),
        );
        assert_eq!(
            primary.swash_cache.borrow().image_cache.len(),
            entries,
            "integer translation must reuse CPU rasters"
        );
        independent.prepare(
            &gpu.device,
            &gpu.queue,
            &config,
            std::iter::once(area(45.0)),
        );
        let draw = |other: &RuiTextRenderer| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shared glyph raster pixels"),
                size: wgpu::Extent3d {
                    width: 512,
                    height: 128,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
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
                primary.render(&mut pass);
                other.render(&mut pass);
            }
            readback_texture(&gpu.device, &gpu.queue, &texture, 512, 128, encoder)
                .expect("readback")
        };
        let actual = draw(&shared);
        let expected = draw(&independent);
        assert_eq!(actual.pixels, expected.pixels);
        assert!(
            actual.pixels.chunks_exact(4).any(|p| p[3] != 0),
            "comparison must contain visible glyphs"
        );
        if let Some(dir) = std::env::var_os("RUI_GLYPH_TEST_OUTPUT") {
            std::fs::create_dir_all(&dir).unwrap();
            image::save_buffer(
                std::path::Path::new(&dir).join("shared-glyph-rasters.png"),
                &actual.pixels,
                512,
                128,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }

    fn sample_raster(w: u16, h: u16) -> EmojiRaster {
        EmojiRaster::new(w, h, vec![10; (w as usize) * (h as usize) * 4])
    }

    #[test]
    fn glyphs_for_falls_back_to_wrap_sibling() {
        std::env::set_var("RUI_EMOJI_NODE_CACHE_MAX", "8");
        let mut cache = EmojiGlyphCache::default();
        let key_a = EmojiCacheKey {
            sequence_hash: 0xE111,
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        };
        let mut rasters = HashMap::new();
        rasters.insert(3, sample_raster(16, 16));
        cache.insert_emoji_node(
            key_a.clone(),
            EmojiGlyphEntry {
                glyphs: vec![glyphon::CustomGlyph {
                    id: 3,
                    left: 0.0,
                    top: 0.0,
                    width: 16.0,
                    height: 16.0,
                    color: None,
                    snap_to_physical_pixel: true,
                    metadata: 0,
                }],
                rasters,
            },
        );
        let key_b = EmojiCacheKey {
            sequence_hash: 0xE111,
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 11, // different wrap bucket
        };
        assert!(
            cache.entries.get(&key_b).is_none(),
            "exact wrap miss expected"
        );
        assert_eq!(
            cache.glyphs_for(&key_b).len(),
            1,
            "paint must reuse wrap sibling glyphs (else emoji blanks on soft-width jitter)"
        );
    }

    #[test]
    fn rasterize_survives_node_lru_eviction() {
        let mut cache = EmojiGlyphCache::default();
        let key_a = EmojiCacheKey {
            sequence_hash: 0xA,
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        };
        let mut rasters = HashMap::new();
        rasters.insert(7, sample_raster(16, 16));
        cache.insert_emoji_node(
            key_a.clone(),
            EmojiGlyphEntry {
                glyphs: vec![],
                rasters,
            },
        );
        // Simulate node LRU eviction: entry map drops but id_rasters keeps stable ids.
        cache.entries.remove(&key_a);
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
    fn rasterize_resizes_when_request_size_differs_from_cache() {
        let mut cache = EmojiGlyphCache::default();
        let mut rgba = vec![0u8; 16 * 16 * 4];
        // Mark center pixel opaque red so resize must preserve ink (not all-zero placeholder).
        rgba[(8 * 16 + 8) * 4] = 255;
        rgba[(8 * 16 + 8) * 4 + 3] = 255;
        let key = EmojiCacheKey {
            sequence_hash: 0xE1,
            pixel_size: 16,
            scale_bits: 1.0f32.to_bits(),
            color_mode: 1,
            wrap_width_q: 10,
        };
        cache.insert_emoji_node(
            key,
            EmojiGlyphEntry {
                glyphs: vec![glyphon::CustomGlyph {
                    id: 3,
                    left: 0.0,
                    top: 0.0,
                    width: 14.0, // paint quad smaller than raster (old 1.15 bug shape)
                    height: 14.0,
                    color: None,
                    snap_to_physical_pixel: true,
                    metadata: 0,
                }],
                rasters: {
                    let mut m = HashMap::new();
                    m.insert(3, EmojiRaster::new(16, 16, rgba));
                    m
                },
            },
        );
        let out = cache
            .rasterize(RasterizeCustomGlyphRequest {
                id: 3,
                width: 14,
                height: 14,
                x_bin: glyphon::SubpixelBin::Zero,
                y_bin: glyphon::SubpixelBin::Zero,
                scale: 1.0,
            })
            .expect("must Some");
        assert_eq!(out.data.len(), 14 * 14 * 4);
        assert!(
            out.data.iter().any(|&b| b != 0),
            "size mismatch must NN-resize cached ink, not return transparent placeholder"
        );
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
