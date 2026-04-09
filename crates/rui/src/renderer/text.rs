use std::collections::HashMap;
use std::path::Path;

use glyphon::{FontSystem, SwashCache, TextArea, TextAtlas, TextRenderer, Viewport};
use wgpu::{Device, Queue, SurfaceConfiguration, RenderPass};

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
    pub entries: HashMap<EmojiCacheKey, Vec<glyphon::CustomGlyph>>,
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
}

fn load_system_emoji_fonts(db: &mut glyphon::fontdb::Database) {
    let candidates = if cfg!(target_os = "windows") {
        vec!["C:/Windows/Fonts/seguiemj.ttf"]
    } else if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/Apple Color Emoji.ttc",
            "/System/Library/Fonts/Apple Color Emoji.ttc/Contents/Resources/Apple Color Emoji.ttc",
        ]
    } else {
        vec![
            "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/google-noto-color-emoji/NotoColorEmoji.ttf",
        ]
    };
    for path in candidates {
        if Path::new(path).exists() {
            if let Ok(bytes) = std::fs::read(path) {
                db.load_font_source(fontdb::Source::Binary(std::sync::Arc::new(bytes)));
                log::info!("loaded system emoji font: {}", path);
            }
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

impl RuiTextRenderer {
    pub fn new(device: &Device, queue: &Queue, config: &SurfaceConfiguration) -> Self {
        let mut font_system = FontSystem::new();
        {
            let db = font_system.db_mut();
            // Load bundled fonts
            db.load_font_data(include_bytes!("../../assets/lucide.ttf").to_vec());
            // Sans Super OTC: includes SC/TC/HK/JP/KR variants in one bundle.
            db.load_font_data(include_bytes!("../../assets/NotoSansCJK.ttc").to_vec());
            // Super OTC: includes JP/KR/SC/TC/HK variants with full static weights.
            db.load_font_data(include_bytes!("../../assets/NotoSerifCJK.ttc").to_vec());
            // Symbols / Math / Music / Emoji coverage.
            db.load_font_data(include_bytes!("../../assets/NotoSansSymbols-wght.ttf").to_vec());
            db.load_font_data(include_bytes!("../../assets/NotoSansSymbols2-Regular.ttf").to_vec());
            db.load_font_data(include_bytes!("../../assets/NotoSansMath-Regular.ttf").to_vec());
            db.load_font_data(include_bytes!("../../assets/NotoMusic-Regular.ttf").to_vec());
            // Prefer Windows-compatible bundle first, keep original as fallback.
            db.load_font_data(include_bytes!("../../assets/NotoColorEmoji_WindowsCompatible.ttf").to_vec());
            db.load_font_data(include_bytes!("../../assets/NotoColorEmoji.ttf").to_vec());
            load_system_emoji_fonts(db);
            
            // Prefer a single CJK+Latin family first for cross-language consistency.
            db.set_sans_serif_family("Noto Sans CJK SC");
            db.set_serif_family("Noto Serif CJK SC");
        }
        log_color_font_capabilities(font_system.db());

        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let mut text_atlas = TextAtlas::new(device, queue, &cache, config.format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, device, wgpu::MultisampleState::default(), None);
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

        self.text_renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();
    }

    pub fn render<'pass>(&'pass self, render_pass: &mut RenderPass<'pass>) {
        self.text_renderer.render(&self.text_atlas, &self.viewport, render_pass).unwrap();
    }
}
