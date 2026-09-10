//! GPU texture / glyph cache diagnostics (Wave P P.2.4).

use crate::core::tree_stats::process_private_mb;
use crate::renderer::text::EmojiGlyphCache;
use crate::renderer::RuiTextRenderer;
use crate::renderer::TexturePool;
use crate::utils::text_texture::text_xform_cpu_cache_stats;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextRendererCacheStats {
    pub shaped_cache: usize,
    pub emoji_nodes: usize,
    pub emoji_rasters: usize,
    pub emoji_id_rasters: usize,
}

impl TextRendererCacheStats {
    pub fn from_renderer(renderer: &RuiTextRenderer) -> Self {
        Self::from_emoji_and_shaped(&renderer.emoji_cache, renderer.shaped_cache.len())
    }

    fn from_emoji_and_shaped(emoji: &EmojiGlyphCache, shaped_cache: usize) -> Self {
        let (emoji_nodes, emoji_rasters, emoji_id_rasters) = emoji.resident_counts();
        Self {
            shaped_cache,
            emoji_nodes,
            emoji_rasters,
            emoji_id_rasters,
        }
    }

    pub fn format_short(&self) -> String {
        format!(
            "shaped={} emoji_n={} emoji_r={} emoji_id={}",
            self.shaped_cache, self.emoji_nodes, self.emoji_rasters, self.emoji_id_rasters,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureCacheStats {
    pub texture_pool: usize,
    pub texture_pool_frame: u64,
    pub texture_pool_max: usize,
    pub texture_pool_stale_120f: usize,
    pub shadow_textures: usize,
    pub page: TextRendererCacheStats,
    pub overlay: TextRendererCacheStats,
    pub overlay_bands: usize,
    pub text_xform_cpu: usize,
    pub text_xform_cpu_bytes: usize,
}

impl TextureCacheStats {
    pub fn collect(
        pool: &TexturePool,
        shadow_textures: usize,
        page: &RuiTextRenderer,
        overlay: &RuiTextRenderer,
        overlay_bands: &[RuiTextRenderer],
    ) -> Self {
        let xform = text_xform_cpu_cache_stats();
        Self {
            texture_pool: pool.len(),
            texture_pool_frame: pool.frame(),
            texture_pool_max: pool.max_entries(),
            texture_pool_stale_120f: pool.stale_count(120),
            shadow_textures,
            page: TextRendererCacheStats::from_renderer(page),
            overlay: TextRendererCacheStats::from_renderer(overlay),
            overlay_bands: overlay_bands.len(),
            text_xform_cpu: xform.0,
            text_xform_cpu_bytes: xform.1,
        }
    }

    pub fn format_line(&self) -> String {
        format!(
            "pool={}/{} frame={} stale120={} shadow={} xform_cpu={}/{} | page[{}] overlay[{}] obands={}",
            self.texture_pool,
            self.texture_pool_max,
            self.texture_pool_frame,
            self.texture_pool_stale_120f,
            self.shadow_textures,
            self.text_xform_cpu,
            self.text_xform_cpu_bytes,
            self.page.format_short(),
            self.overlay.format_short(),
            self.overlay_bands,
        )
    }
}

pub fn texture_stats_enabled() -> bool {
    matches!(
        std::env::var("RUI_TEXTURE_STATS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

pub fn emit_texture_stats(
    pool: &TexturePool,
    shadow_textures: usize,
    page: &RuiTextRenderer,
    overlay: &RuiTextRenderer,
    overlay_bands: &[RuiTextRenderer],
) {
    if !texture_stats_enabled() {
        return;
    }
    let stats = TextureCacheStats::collect(pool, shadow_textures, page, overlay, overlay_bands);
    log::info!(
        target: "rui::texture_stats",
        "private={:.1} MiB {}",
        process_private_mb(),
        stats.format_line()
    );
}
