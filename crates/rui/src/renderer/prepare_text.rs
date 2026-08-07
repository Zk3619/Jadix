//! 将布局得到的 `TextLayoutInfo` 转为 glyphon 文本区与下划线实例，并执行 `prepare`。
//!
//! 调试：设置环境变量 `RUI_TEXT_DEBUG`（见 [`super::text_prepare_debug`]）。

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

use wgpu::{Device, Queue, SurfaceConfiguration};

use crate::core::tree::TextLayoutInfo;
use crate::style::{
    tailwind_default_leading_scale, TextDecorationStyle, TextDirection, TextOverflow, TextSpanKind,
};

use super::emoji_detect;
use super::emoji_layout;
use super::emoji_svg;
use super::font_loader;
use super::text::RuiTextRenderer;
use super::text_hit::{
    best_font_family, is_single_line_text_box, measure_prefix_width, truncate_with_ellipsis,
};
use super::twemoji_name;
use super::InstanceInput;

#[derive(Debug, Clone)]
enum TextSegment {
    Plain,
    Emoji { text: String, start: usize },
}

#[derive(Clone)]
struct FrameTextNodeMeta {
    x: f32,
    y: f32,
    box_w: f32,
    box_h: f32,
    dy: f32,
    text_size: f32,
    align: glyphon::cosmic_text::Align,
    measured_text_w: f32,
    underline: bool,
    overline: bool,
    line_through: bool,
    decoration_style: TextDecorationStyle,
    decoration_color: Option<[u8; 4]>,
    decoration_thickness: f32,
    underline_offset: f32,
    color: [u8; 4],
    clip_bounds: Option<[f32; 4]>,
    shadow_col: Option<[u8; 4]>,
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_blur: f32,
    emoji_cache_key: Option<super::text::EmojiCacheKey>,
}

/// 预渲染一帧需要上传的阴影位图。
#[derive(Clone)]
pub struct ShadowNewBitmap {
    pub cache_key: u64,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// 相对于文本节点 (x,y) 的绘制偏移与尺寸 [dx, dy, w, h]。
    pub draw_rect: [f32; 4],
}

/// 需要在纹理绘制阶段渲染的阴影项。
#[derive(Clone)]
pub struct ShadowTexDraw {
    pub cache_key: u64,
    /// 屏幕矩形 [x, y, w, h]，物理像素。
    pub rect: [f32; 4],
}

/// `prepare_text_frame` 的完整输出。
pub struct PrepareTextOutput {
    pub underline_instances: Vec<InstanceInput>,
    /// Input/label caret hairlines — drawn *after* glyphon text so they stay visible.
    pub caret_instances: Vec<InstanceInput>,
    pub shadow_new_bitmaps: Vec<ShadowNewBitmap>,
    pub shadow_tex_draws: Vec<ShadowTexDraw>,
    pub needs_warmup: bool,
}

fn shadow_cache_key(
    shape_hash: u64,
    shadow_col: [u8; 4],
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_blur: f32,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    shape_hash.hash(&mut h);
    shadow_col.hash(&mut h);
    shadow_dx.to_bits().hash(&mut h);
    shadow_dy.to_bits().hash(&mut h);
    shadow_blur.to_bits().hash(&mut h);
    h.finish()
}

/// 阴影位图生成结果。
struct ShadowBitmapOutput {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    /// 绘制矩形相对于文本节点 (x, y) 的偏移与尺寸。
    draw_rect: [f32; 4],
}

/// 使用 swash 将文本阴影栅格化为 RGBA 位图（Vogel spiral 多采样）。
/// 仅在缓存未命中时调用（每个唯一阴影文本仅一次）。
fn generate_shadow_bitmap(
    buffer: &glyphon::Buffer,
    font_system: &mut glyphon::FontSystem,
    swash_cache: &mut glyphon::SwashCache,
    shadow_color: [u8; 4],
    shadow_dx: f32,
    shadow_dy: f32,
    shadow_blur: f32,
) -> Option<ShadowBitmapOutput> {
    let radius = shadow_blur;

    struct GlyphEntry {
        px: i32,
        py: i32,
        data: Vec<u8>,
        width: u32,
        height: u32,
        is_color: bool,
    }
    let mut glyph_entries: Vec<GlyphEntry> = Vec::new();
    for run in buffer.layout_runs() {
        let line_y_px = run.line_y.round() as i32;
        for g in run.glyphs.iter() {
            let physical = g.physical((0.0, 0.0), 1.0);
            let img = match swash_cache.get_image_uncached(font_system, physical.cache_key) {
                Some(img) => img,
                None => continue,
            };
            glyph_entries.push(GlyphEntry {
                px: physical.x + img.placement.left,
                py: line_y_px + physical.y - img.placement.top,
                data: img.data,
                width: img.placement.width,
                height: img.placement.height,
                is_color: matches!(img.content, glyphon::SwashContent::Color),
            });
        }
    }
    if glyph_entries.is_empty() {
        return None;
    }

    // 根据实际字形位置计算内容边界（字形可能超出 [0, box_h] 范围）
    let mut content_min_x = i32::MAX;
    let mut content_min_y = i32::MAX;
    let mut content_max_x = i32::MIN;
    let mut content_max_y = i32::MIN;
    for ge in &glyph_entries {
        content_min_x = content_min_x.min(ge.px);
        content_min_y = content_min_y.min(ge.py);
        content_max_x = content_max_x.max(ge.px + ge.width as i32);
        content_max_y = content_max_y.max(ge.py + ge.height as i32);
    }

    let blur_pad = (radius + 4.0).ceil() as i32;
    let offset_pad = shadow_dx.abs().max(shadow_dy.abs()).ceil() as i32 + 1;
    let pad = blur_pad + offset_pad;

    let bmp_x0 = content_min_x - pad;
    let bmp_y0 = content_min_y - pad;
    let bw = ((content_max_x + pad - bmp_x0) as u32).max(1);
    let bh = ((content_max_y + pad - bmp_y0) as u32).max(1);
    let pixel_count = bw as usize * bh as usize;
    let mut alpha_buf = vec![0u16; pixel_count];

    let perf = text_perf_mode();
    let (base_samples, radial_scale, max_samples) = match perf {
        TextPerfMode::HighQuality => (20_usize, 9.0_f32, 72_usize),
        TextPerfMode::Balanced => (12_usize, 6.5_f32, 48_usize),
        TextPerfMode::Fast => (4_usize, 3.0_f32, 16_usize),
    };

    let alpha = shadow_color[3];
    const SHARP_BLUR_EPS: f32 = 0.75;
    let draw_blur = radius > SHARP_BLUR_EPS && alpha > 6;

    let mut stamp = |ox: f32, oy: f32, sample_alpha: u8| {
        if sample_alpha == 0 {
            return;
        }
        for ge in &glyph_entries {
            let px0 = ge.px - bmp_x0 + ox.round() as i32;
            let py0 = ge.py - bmp_y0 + oy.round() as i32;
            for row in 0..ge.height as i32 {
                let by = py0 + row;
                if by < 0 || by >= bh as i32 {
                    continue;
                }
                let row_off = (row as u32 * ge.width) as usize;
                let dst_row = by as usize * bw as usize;
                for col in 0..ge.width as i32 {
                    let bx = px0 + col;
                    if bx < 0 || bx >= bw as i32 {
                        continue;
                    }
                    let src_a = if ge.is_color {
                        ge.data[(row_off + col as usize) * 4 + 3]
                    } else {
                        ge.data[row_off + col as usize]
                    };
                    let src = (src_a as u16 * sample_alpha as u16) / 255;
                    let dst = alpha_buf[dst_row + bx as usize];
                    alpha_buf[dst_row + bx as usize] = src + dst - (src * dst) / 255;
                }
            }
        }
    };

    if !draw_blur {
        stamp(shadow_dx, shadow_dy, alpha);
    } else {
        const GOLDEN_ANGLE: f32 = 2.399_963_1;
        let sample_count = (base_samples as f32 + radius * radial_scale)
            .round()
            .clamp(base_samples as f32, max_samples as f32) as usize;
        let sigma = (radius * 0.58).max(0.75);
        let two_sigma2 = 2.0 * sigma * sigma;
        let mut weight_sum = 1.0_f32;
        for idx in 0..sample_count {
            let t = (idx as f32 + 0.5) / sample_count as f32;
            let r = radius * t.sqrt();
            weight_sum += (-(r * r) / two_sigma2).exp();
        }
        let inv = 1.0 / weight_sum.max(0.001);
        let ca = ((alpha as f32) * inv).round().clamp(0.0, 255.0) as u8;
        stamp(shadow_dx, shadow_dy, ca);
        for idx in 0..sample_count {
            let t = (idx as f32 + 0.5) / sample_count as f32;
            let r = radius * t.sqrt();
            let theta = idx as f32 * GOLDEN_ANGLE;
            let w = (-(r * r) / two_sigma2).exp();
            let wa = ((alpha as f32) * w * inv).round().clamp(0.0, 255.0) as u8;
            stamp(shadow_dx + theta.cos() * r, shadow_dy + theta.sin() * r, wa);
        }
    }

    let [sr, sg, sb] = [shadow_color[0], shadow_color[1], shadow_color[2]];
    let mut rgba = vec![0u8; pixel_count * 4];
    // 纹理管线 UV 映射 Y 轴翻转（quad 顶部 → V=1 → 纹理底行），
    // 写入时直接反转行序使最终显示正确。
    for src_y in 0..bh as usize {
        let dst_y = bh as usize - 1 - src_y;
        let src_row = src_y * bw as usize;
        let dst_row = dst_y * bw as usize;
        for x in 0..bw as usize {
            let a = alpha_buf[src_row + x].min(255) as u8;
            if a > 0 {
                let di = (dst_row + x) * 4;
                rgba[di] = sr;
                rgba[di + 1] = sg;
                rgba[di + 2] = sb;
                rgba[di + 3] = a;
            }
        }
    }

    let draw_rect = [bmp_x0 as f32, bmp_y0 as f32, bw as f32, bh as f32];
    Some(ShadowBitmapOutput {
        rgba,
        width: bw,
        height: bh,
        draw_rect,
    })
}

fn align_to_u8(align: glyphon::cosmic_text::Align) -> u8 {
    match align {
        glyphon::cosmic_text::Align::Left => 0,
        glyphon::cosmic_text::Align::Center => 1,
        glyphon::cosmic_text::Align::Right => 2,
        glyphon::cosmic_text::Align::Justified => 3,
        glyphon::cosmic_text::Align::End => 4,
    }
}

fn family_hash(family: glyphon::Family) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match family {
        glyphon::Family::Name(name) => {
            1u8.hash(&mut h);
            name.hash(&mut h);
        }
        glyphon::Family::Serif => 2u8.hash(&mut h),
        glyphon::Family::SansSerif => 3u8.hash(&mut h),
        glyphon::Family::Monospace => 4u8.hash(&mut h),
        glyphon::Family::Cursive => 5u8.hash(&mut h),
        glyphon::Family::Fantasy => 6u8.hash(&mut h),
    }
    h.finish()
}

fn node_shape_hash(
    text: &str,
    scaled_size: f32,
    line_height: f32,
    scaled_w: f32,
    scaled_h: f32,
    weight: u16,
    family: glyphon::Family,
    align: glyphon::cosmic_text::Align,
    shaping: glyphon::Shaping,
    letter_spacing: f32,
    direction: TextDirection,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    ((scaled_size * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((scaled_w / 8.0).round().clamp(0.0, (u16::MAX as f32) / 8.0) as u16).hash(&mut h);
    ((scaled_h / 8.0).round().clamp(0.0, (u16::MAX as f32) / 8.0) as u16).hash(&mut h);
    weight.hash(&mut h);
    family_hash(family).hash(&mut h);
    align_to_u8(align).hash(&mut h);
    let shaping_mode: u8 = match shaping {
        glyphon::Shaping::Advanced => 1,
        _ => 0,
    };
    shaping_mode.hash(&mut h);
    letter_spacing.to_bits().hash(&mut h);
    (match direction {
        TextDirection::Auto => 0u8,
        TextDirection::Ltr => 1u8,
        TextDirection::Rtl => 2u8,
    })
    .hash(&mut h);
    h.finish()
}

fn make_dummy_cache_entry(font_system: &mut glyphon::FontSystem) -> super::text::ShapedTextEntry {
    let mut buffer = glyphon::Buffer::new(font_system, glyphon::Metrics::new(1.0, 1.0));
    buffer.set_size(font_system, Some(1.0), Some(1.0));
    super::text::ShapedTextEntry {
        key_hash: 0,
        buffer,
        measured_text_w: 0.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextPerfMode {
    HighQuality,
    Balanced,
    Fast,
}

fn text_perf_mode() -> TextPerfMode {
    static MODE: OnceLock<TextPerfMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        let raw = std::env::var("RUI_TEXT_PERF")
            .unwrap_or_else(|_| "balanced".to_string())
            .to_ascii_lowercase();
        match raw.as_str() {
            "fast" | "low" => TextPerfMode::Fast,
            "high" | "quality" => TextPerfMode::HighQuality,
            _ => TextPerfMode::Balanced,
        }
    })
}

fn preshape_margin() -> f32 {
    static MARGIN: OnceLock<f32> = OnceLock::new();
    *MARGIN.get_or_init(|| {
        std::env::var("RUI_TEXT_PRESHAPE_MARGIN")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(768.0)
    })
}

fn preshape_budget() -> usize {
    static BUDGET: OnceLock<usize> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("RUI_TEXT_PRESHAPE_BUDGET")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4)
    })
}

pub(crate) fn caret_blink_visible() -> bool {
    caret_blink_phase().is_multiple_of(2)
}

pub(crate) fn caret_blink_phase() -> u128 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_millis() / 520
}

/// Instant when the caret blink phase will flip next (for `ControlFlow::WaitUntil`).
pub(crate) fn caret_blink_next_toggle_at() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let elapsed_ms = start.elapsed().as_millis();
    const PHASE_MS: u128 = 520;
    let into = elapsed_ms % PHASE_MS;
    let remain = (PHASE_MS - into).max(1);
    Instant::now() + Duration::from_millis(remain as u64)
}

#[allow(dead_code)]
fn input_line_height(style: &crate::style::TextStyle, scaled_size: f32, ui_scale: f32) -> f32 {
    if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(|| tailwind_default_leading_scale(style.size));
        scaled_size * scale
    }
}

fn clip_rect_to_bounds(rect: [f32; 4], clip: Option<[f32; 4]>) -> Option<[f32; 4]> {
    let Some(c) = clip else {
        return Some(rect);
    };
    let x0 = rect[0].max(c[0]);
    let y0 = rect[1].max(c[1]);
    let x1 = (rect[0] + rect[2]).min(c[0] + c[2]);
    let y1 = (rect[1] + rect[3]).min(c[1] + c[3]);
    let w = (x1 - x0).max(0.0);
    let h = (y1 - y0).max(0.0);
    if w <= 0.0 || h <= 0.0 {
        None
    } else {
        Some([x0, y0, w, h])
    }
}

fn text_bounds_from_clip(
    clip: Option<[f32; 4]>,
    config: &SurfaceConfiguration,
) -> glyphon::TextBounds {
    if let Some(b) = clip {
        glyphon::TextBounds {
            left: b[0].floor() as i32,
            top: b[1].floor() as i32,
            right: (b[0] + b[2]).ceil() as i32,
            bottom: (b[1] + b[3]).ceil() as i32,
        }
    } else {
        glyphon::TextBounds {
            left: 0,
            top: 0,
            right: config.width as i32,
            bottom: config.height as i32,
        }
    }
}

fn split_text_with_emoji(text: &str) -> Vec<TextSegment> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut current = 0usize;
    for grapheme in text.graphemes(true) {
        let start = current;
        current += grapheme.len();
        if emoji_detect::grapheme_is_emoji(grapheme) {
            if !plain.is_empty() {
                plain.clear();
                out.push(TextSegment::Plain);
            }
            out.push(TextSegment::Emoji {
                text: grapheme.to_string(),
                start,
            });
        } else {
            plain.push_str(grapheme);
        }
    }
    if !plain.is_empty() {
        out.push(TextSegment::Plain);
    }
    out
}

fn emoji_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUI_EMOJI_DEBUG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn emoji_custom_glyph_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        // Default ON: color Twemoji PNGs. Set RUI_EMOJI_CUSTOM_GLYPH=0 to use font glyphs
        // (often monochrome outline boxes with Segoe UI).
        std::env::var("RUI_EMOJI_CUSTOM_GLYPH")
            .map(|v| {
                !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
            })
            .unwrap_or(true)
    })
}

fn emoji_svg_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUI_EMOJI_SVG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn emoji_mode_label() -> &'static str {
    if emoji_custom_glyph_enabled() && emoji_svg_enabled() {
        "color-svg"
    } else if emoji_custom_glyph_enabled() {
        "color-twemoji"
    } else {
        "font"
    }
}

fn apply_letter_spacing_approx(text: &str, spacing_px: f32, font_size: f32) -> String {
    if spacing_px.abs() < 0.5 || text.is_empty() {
        return text.to_string();
    }
    let hair_w = (font_size * 0.05).max(0.5);
    let n = (spacing_px / hair_w).round().max(0.0) as usize;
    if n == 0 {
        return text.to_string();
    }
    let hair = "\u{200A}".repeat(n);
    let mut out = String::new();
    let mut first = true;
    for g in text.graphemes(true) {
        if !first {
            out.push_str(&hair);
        }
        first = false;
        out.push_str(g);
    }
    out
}

fn direction_mark(dir: TextDirection) -> &'static str {
    match dir {
        TextDirection::Ltr => "\u{200E}",
        TextDirection::Rtl => "\u{200F}",
        TextDirection::Auto => "",
    }
}

fn resolve_text_align(
    align: glyphon::cosmic_text::Align,
    dir: TextDirection,
) -> glyphon::cosmic_text::Align {
    match dir {
        TextDirection::Rtl if matches!(align, glyphon::cosmic_text::Align::Left) => {
            glyphon::cosmic_text::Align::Right
        }
        TextDirection::Ltr if matches!(align, glyphon::cosmic_text::Align::Right) => {
            glyphon::cosmic_text::Align::Left
        }
        _ => align,
    }
}

/// Shape text; when Twemoji custom glyphs are on, paint emoji codepoints transparent so
/// Segoe UI's monochrome outline glyphs do not show under/over the color PNGs.
fn set_shaped_buffer_text(
    buffer: &mut glyphon::Buffer,
    font_system: &mut glyphon::FontSystem,
    text: &str,
    attrs: glyphon::Attrs,
    shaping: glyphon::Shaping,
    hide_emoji_font_glyphs: bool,
) {
    if !hide_emoji_font_glyphs || !emoji_detect::text_contains_emoji(text) {
        buffer.set_text(font_system, text, attrs, shaping);
        return;
    }
    let invisible = attrs.color(glyphon::Color::rgba(0, 0, 0, 0));
    let mut spans: Vec<(&str, glyphon::Attrs)> = Vec::new();
    let mut plain_start = 0usize;
    let mut byte = 0usize;
    for grapheme in text.graphemes(true) {
        let start = byte;
        byte += grapheme.len();
        if emoji_detect::grapheme_is_emoji(grapheme) {
            if plain_start < start {
                spans.push((&text[plain_start..start], attrs));
            }
            spans.push((&text[start..byte], invisible));
            plain_start = byte;
        }
    }
    if plain_start < text.len() {
        spans.push((&text[plain_start..], attrs));
    }
    if spans.is_empty() {
        buffer.set_text(font_system, text, attrs, shaping);
    } else {
        buffer.set_rich_text(font_system, spans, attrs, shaping);
    }
}

/// 与占位字形对齐后的额外垂直微调（仅当设置了 `RUI_EMOJI_BASELINE_FACTOR` 时非零）。
fn emoji_baseline_layout_tweak(text_size: f32) -> f32 {
    let Some(factor) = std::env::var("RUI_EMOJI_BASELINE_FACTOR")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
    else {
        return 0.0;
    };
    -(text_size * factor)
}

/// 无法从 layout 推导位置时的回退垂直偏移（沿用原平台启发式）。
fn emoji_baseline_offset_fallback(text_size: f32) -> f32 {
    let platform_default = if cfg!(target_os = "windows") {
        0.12
    } else {
        0.1
    };
    let factor = std::env::var("RUI_EMOJI_BASELINE_FACTOR")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(platform_default);
    -(text_size * factor)
}

fn emoji_metrics_log_tick(stats: super::text::EmojiCacheStats) {
    static FRAMES: AtomicU64 = AtomicU64::new(0);
    let frame = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if frame.is_multiple_of(120) {
        let hit_rate = if stats.lookups == 0 {
            0.0
        } else {
            stats.hits as f64 / stats.lookups as f64
        };
        log::warn!(
            "emoji_cache stats: mode={} lookups={} hits={} hit_rate={:.2} uploads={} upload_bytes={} svg_attempts={} svg_hits={} svg_raster_ms={:.2} svg_fallback_to_png={}",
            emoji_mode_label(),
            stats.lookups,
            stats.hits,
            hit_rate,
            stats.uploads,
            stats.upload_bytes,
            stats.svg_attempts,
            stats.svg_hits,
            stats.svg_raster_ms,
            stats.svg_fallback_to_png
        );
    }
}

fn svg_raster_threshold_ms() -> f64 {
    std::env::var("RUI_EMOJI_SVG_RASTER_MS_THRESHOLD")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(8.0)
}

fn svg_runtime_allowed(stats: super::text::EmojiCacheStats) -> bool {
    if !emoji_svg_enabled() {
        return false;
    }
    if stats.svg_attempts == 0 {
        return true;
    }
    (stats.svg_raster_ms / stats.svg_attempts as f64) <= svg_raster_threshold_ms()
}

fn twemoji_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("RUI_TWEMOJI_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return p;
        }
    }
    // Absolute candidates — never rely solely on process cwd.
    let mut candidates = vec![
        PathBuf::from("assets/twemoji/72x72"),
        PathBuf::from("crates/rui/assets/twemoji/72x72"),
    ];
    // crates/rui → ../../assets/twemoji/72x72
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("../../assets/twemoji/72x72"));
    candidates.push(manifest.join("assets/twemoji/72x72"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets/twemoji/72x72"));
        candidates.push(cwd.join("crates/rui/assets/twemoji/72x72"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("assets/twemoji/72x72"));
            candidates.push(parent.join("twemoji/72x72"));
            // Walk up a few levels from target/release toward the workspace.
            let mut walk = parent.to_path_buf();
            for _ in 0..5 {
                candidates.push(walk.join("assets/twemoji/72x72"));
                if !walk.pop() {
                    break;
                }
            }
        }
    }
    for c in candidates {
        if let Ok(canon) = c.canonicalize() {
            if canon.is_dir() {
                return canon;
            }
        } else if c.is_dir() {
            return c;
        }
    }
    PathBuf::from("assets/twemoji/72x72")
}

fn twemoji_filename(sequence: &str) -> String {
    format!("{}.png", twemoji_name::twemoji_stem(sequence))
}

fn decode_png_rgba(bytes: &[u8]) -> Option<super::text::EmojiRaster> {
    let cursor = Cursor::new(bytes);
    let decoder = png::Decoder::new(cursor);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let (w, h) = (info.width as usize, info.height as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match info.color_type {
        png::ColorType::Rgba => {
            rgba.copy_from_slice(data);
        }
        png::ColorType::Rgb => {
            for i in 0..(w * h) {
                let si = i * 3;
                let di = i * 4;
                rgba[di] = data[si];
                rgba[di + 1] = data[si + 1];
                rgba[di + 2] = data[si + 2];
                rgba[di + 3] = 255;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for i in 0..(w * h) {
                let si = i * 2;
                let di = i * 4;
                rgba[di] = data[si];
                rgba[di + 1] = data[si];
                rgba[di + 2] = data[si];
                rgba[di + 3] = data[si + 1];
            }
        }
        png::ColorType::Grayscale => {
            for (i, &gray) in data.iter().enumerate().take(w * h) {
                let di = i * 4;
                rgba[di] = gray;
                rgba[di + 1] = gray;
                rgba[di + 2] = gray;
                rgba[di + 3] = 255;
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(super::text::EmojiRaster::new(
        info.width as u16,
        info.height as u16,
        rgba,
    ))
}

fn resize_rgba_nn(src: &super::text::EmojiRaster, dst_size: u16) -> super::text::EmojiRaster {
    let dst = dst_size.max(1) as usize;
    let sw = src.width.max(1) as usize;
    let sh = src.height.max(1) as usize;
    if sw == dst && sh == dst {
        return src.clone();
    }
    let mut out = vec![0u8; dst * dst * 4];
    for y in 0..dst {
        let sy = y * sh / dst;
        for x in 0..dst {
            let sx = x * sw / dst;
            let si = (sy * sw + sx) * 4;
            let di = (y * dst + x) * 4;
            out[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
    super::text::EmojiRaster::new(dst as u16, dst as u16, out)
}

fn load_twemoji_raster(sequence: &str, pixel_size: u16) -> Option<super::text::EmojiRaster> {
    let base = twemoji_dir();
    let path = base.join(twemoji_filename(sequence));
    if !Path::new(&path).exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let raw = decode_png_rgba(&bytes)?;
    Some(resize_rgba_nn(&raw, pixel_size.max(1)))
}

fn load_font_fallback_raster(
    text_renderer: &mut RuiTextRenderer,
    sequence: &str,
    pixel_size: u16,
    _family: glyphon::Family,
    font_weight: u16,
) -> Option<super::text::EmojiRaster> {
    // Prefer a real color-emoji face; UI sans yields monochrome outline "emoji".
    let emoji_family = crate::utils::font::resolve_family(
        unsafe { &*text_renderer.font_system }.db(),
        "Segoe UI Emoji, Apple Color Emoji, Noto Color Emoji, emoji, sans-serif",
    );
    let size = pixel_size.max(1) as f32;
    let mut buffer = glyphon::Buffer::new(
        unsafe { &mut *text_renderer.font_system },
        glyphon::Metrics::new(size, size * 1.2),
    );
    buffer.set_size(
        unsafe { &mut *text_renderer.font_system },
        Some(size * 2.0),
        Some(size * 2.0),
    );
    buffer.set_text(
        unsafe { &mut *text_renderer.font_system },
        sequence,
        glyphon::Attrs::new()
            .family(emoji_family)
            .weight(glyphon::fontdb::Weight(font_weight)),
        glyphon::Shaping::Advanced,
    );
    buffer.shape_until_scroll(unsafe { &mut *text_renderer.font_system }, false);
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            let physical = g.physical((0.0, 0.0), 1.0);
            let image = text_renderer
                .swash_cache
                .get_image_uncached(unsafe { &mut *text_renderer.font_system }, physical.cache_key)?;
            let w = image.placement.width as usize;
            let h = image.placement.height as usize;
            if w == 0 || h == 0 {
                continue;
            }
            // Only accept true color bitmaps — Mask/SubpixelMask is the "outline box" look.
            let glyphon::SwashContent::Color = image.content else {
                continue;
            };
            return Some(super::text::EmojiRaster::new(
                image.placement.width as u16,
                image.placement.height as u16,
                image.data,
            ));
        }
    }
    None
}

fn load_emoji_raster_chain(
    text_renderer: &mut RuiTextRenderer,
    sequence: &str,
    pixel_size: u16,
    family: glyphon::Family,
    font_weight: u16,
) -> Option<super::text::EmojiRaster> {
    // 1) Twemoji PNG (preferred — always color), scaled to text size.
    if let Some(png) = load_twemoji_raster(sequence, pixel_size) {
        return Some(png);
    }

    // 2) Optional SVG (feature + env); skip when disabled.
    if emoji_custom_glyph_enabled() && svg_runtime_allowed(text_renderer.emoji_cache.stats) {
        text_renderer.emoji_cache.stats.svg_attempts += 1;
        let start = Instant::now();
        let seq = sequence.to_string();
        let px = pixel_size;
        let svg = std::thread::spawn(move || emoji_svg::load_svg_raster(&seq, px))
            .join()
            .ok()
            .flatten();
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        text_renderer.emoji_cache.stats.svg_raster_ms += elapsed_ms;
        if let Some(svg) = svg {
            text_renderer.emoji_cache.stats.svg_hits += 1;
            return Some(svg);
        }
        text_renderer.emoji_cache.stats.svg_fallback_to_png += 1;
    }

    // 3) System color-emoji font (CBDT/sbix) — only after Twemoji miss.
    font_loader::ensure_emoji_font_fallback(unsafe { &mut *text_renderer.font_system });
    load_font_fallback_raster(text_renderer, sequence, pixel_size, family, font_weight)
        .map(|r| resize_rgba_nn(&r, pixel_size.max(1)))
}

fn load_emoji_raster_cached(
    text_renderer: &mut RuiTextRenderer,
    sequence: &str,
    pixel_size: u16,
    family: glyphon::Family,
    font_weight: u16,
) -> Option<super::text::EmojiRaster> {
    let key = super::text::EmojiRasterCacheKey {
        sequence: sequence.to_string(),
        pixel_size,
    };
    if let Some(raster) = text_renderer.emoji_cache.get_raster_cached(&key) {
        return Some(raster);
    }
    let raster = load_emoji_raster_chain(text_renderer, sequence, pixel_size, family, font_weight)?;
    text_renderer
        .emoji_cache
        .insert_raster(key.clone(), raster.clone());
    Some(raster)
}

/// 文本阴影层会把整块 buffer 平移；若仍用正文同款 `TextBounds`，glyphon 会按边界裁字，
/// 多圈模糊采样时会出现缺块/不对称。对阴影扩大裁剪并与视口求交。
fn shadow_clip_for_glyphon(
    node_clip: Option<[f32; 4]>,
    outset: f32,
    viewport: [f32; 4],
) -> Option<[f32; 4]> {
    let c = node_clip?;
    let expanded = [
        c[0] - outset,
        c[1] - outset,
        c[2] + 2.0 * outset,
        c[3] + 2.0 * outset,
    ];
    clip_rect_to_bounds(expanded, Some(viewport))
}

/// 排版文本、构建下划线 `InstanceInput`，并调用 glyphon `prepare`。
pub fn prepare_text_frame(
    text_renderer: &mut RuiTextRenderer,
    device: &Device,
    queue: &Queue,
    config: &SurfaceConfiguration,
    text_nodes: &[TextLayoutInfo],
    ui_scale: f32,
    force_fast_mode: bool,
    shadow_cached_keys: &std::collections::HashSet<u64>,
    shadow_cached_rects: &std::collections::HashMap<u64, [f32; 4]>,
) -> PrepareTextOutput {
    let perf_mode = if force_fast_mode {
        TextPerfMode::Fast
    } else {
        text_perf_mode()
    };
    if text_nodes.is_empty() {
        text_renderer.prepare(device, queue, config, std::iter::empty());
        return PrepareTextOutput {
            underline_instances: Vec::new(),
            caret_instances: Vec::new(),
            shadow_new_bitmaps: Vec::new(),
            shadow_tex_draws: Vec::new(),
            needs_warmup: false,
        };
    }

    let mut shadow_new_bitmaps: Vec<ShadowNewBitmap> = Vec::new();
    let mut shadow_tex_draws: Vec<ShadowTexDraw> = Vec::new();
    let mut shadow_skip_indices: std::collections::HashSet<usize> =
        std::collections::HashSet::new();

    let mut frame_meta: Vec<Option<FrameTextNodeMeta>> = Vec::with_capacity(text_nodes.len());
    let estimated = text_nodes.len().saturating_mul(3);
    let cap = text_renderer.text_area_capacity_hint.max(estimated);
    let mut text_areas = Vec::with_capacity(cap);
    let viewport_clip = [0.0, 0.0, config.width as f32, config.height as f32];

    if text_renderer.shaped_cache.len() > text_nodes.len() {
        text_renderer.shaped_cache.truncate(text_nodes.len());
    }

    let initial_preshape = text_renderer.shaped_cache.len() < text_nodes.len();
    let effective_margin = if initial_preshape {
        f32::MAX
    } else {
        preshape_margin()
    };
    let effective_budget = if initial_preshape {
        usize::MAX
    } else {
        preshape_budget()
    };

    let mut preshape_miss_count = 0usize;
    let mut preshape_warmup: Vec<(usize, f32, f32, f32, f32)> = Vec::new();

    let _t0 = std::time::Instant::now();
    let mut _shape_misses = 0usize;
    let mut _emoji_misses = 0usize;

    for (i, info) in text_nodes.iter().enumerate() {
        let st = &info.style;

        // Coordinate scaling + early visibility / margin check.
        // Runs before expensive emoji-detection and font-resolution so that
        // nodes completely outside the pre-shape margin are skipped cheaply.
        let scaled_w = info.width * ui_scale;
        let scaled_h = info.height * ui_scale;
        let x = info.x * ui_scale;
        let y = info.y * ui_scale;
        let scaled_clip = info.clip_bounds.map(|b| {
            [
                b[0] * ui_scale,
                b[1] * ui_scale,
                b[2] * ui_scale,
                b[3] * ui_scale,
            ]
        });
        let node_rect = [x, y, scaled_w.max(0.0), scaled_h.max(0.0)];
        let visible = clip_rect_to_bounds(node_rect, scaled_clip)
            .and_then(|c| clip_rect_to_bounds(c, Some(viewport_clip)))
            .is_some();
        let preshape_only = if !visible {
            let margin = effective_margin;
            if margin <= 0.0 {
                frame_meta.push(None);
                continue;
            }
            let expanded_clip = scaled_clip.map(|c| {
                [
                    c[0] - margin,
                    c[1] - margin,
                    c[2] + 2.0 * margin,
                    c[3] + 2.0 * margin,
                ]
            });
            let expanded_vp = [
                -margin,
                -margin,
                config.width as f32 + 2.0 * margin,
                config.height as f32 + 2.0 * margin,
            ];
            let in_margin = clip_rect_to_bounds(node_rect, expanded_clip)
                .and_then(|c| clip_rect_to_bounds(c, Some(expanded_vp)))
                .is_some();
            if !in_margin {
                frame_meta.push(None);
                continue;
            }
            true
        } else {
            false
        };

        if preshape_only && initial_preshape && info.text.is_ascii() {
            while text_renderer.shaped_cache.len() <= i {
                text_renderer
                    .shaped_cache
                    .push(make_dummy_cache_entry(unsafe { &mut *text_renderer.font_system }));
            }
            frame_meta.push(None);
            continue;
        }

        let has_emoji = emoji_detect::text_contains_emoji(info.text.as_str())
            || info
                .shape_suffix
                .as_deref()
                .is_some_and(|s| !s.is_empty() && emoji_detect::text_contains_emoji(s));

        // Lazy-open CJK / emoji / script faces before family resolve + shaping (EUI-style).
        let shape_text_for_coverage: Cow<'_, str> = match info.shape_suffix.as_deref() {
            Some(s) if !s.is_empty() => Cow::Owned(format!("{}{}", info.text, s)),
            _ => Cow::Borrowed(info.text.as_str()),
        };
        font_loader::ensure_coverage_for_text(
            unsafe { &mut *text_renderer.font_system },
            shape_text_for_coverage.as_ref(),
        );

        let family = if has_emoji {
            // Twemoji custom glyphs paint color emoji; keep layout on UI sans.
            // System emoji font opens only on Twemoji miss (ensure_emoji_font_fallback).
            best_font_family(unsafe { &*text_renderer.font_system })
        } else if let Some(f) = &st.font_family {
            crate::utils::font::resolve_family(unsafe { &*text_renderer.font_system }.db(), f)
        } else {
            best_font_family(unsafe { &*text_renderer.font_system })
        };

        let is_icon = st.font_family.as_deref() == Some("lucide");
        let requested_weight = if is_icon { 400 } else { st.weight };
        let font_weight = crate::utils::font::normalize_weight_for_family(
            unsafe { &*text_renderer.font_system }.db(),
            family,
            requested_weight,
        );
        let scaled_size = st.size * ui_scale;
        let line_height = if is_icon {
            scaled_size
        } else if let Some(line_height_px) = st.line_height_px {
            line_height_px * ui_scale
        } else {
            let scale = st
                .line_height_scale
                .unwrap_or_else(|| tailwind_default_leading_scale(st.size));
            scaled_size * scale
        };

        let is_ascii_basic = st.input_caret.is_none() && info.text.is_ascii();
        let shaping = match perf_mode {
            // Editing/caret keeps advanced shaping for cursor fidelity.
            TextPerfMode::HighQuality => {
                if (force_fast_mode && st.input_caret.is_none()) || is_ascii_basic {
                    glyphon::Shaping::Basic
                } else {
                    glyphon::Shaping::Advanced
                }
            }
            // Balanced: prioritize frame stability while scrolling.
            TextPerfMode::Balanced | TextPerfMode::Fast => {
                if is_ascii_basic {
                    glyphon::Shaping::Basic
                } else {
                    glyphon::Shaping::Advanced
                }
            }
        };
        // Scroll/drag frame: only ASCII non-input text is downgraded to Basic.
        let shaping = if force_fast_mode && st.input_caret.is_none() && info.text.is_ascii() {
            glyphon::Shaping::Basic
        } else {
            shaping
        };
        // Soft-wrap width for glyphon. When layout attached a PreparedText, consult
        // `layout(&prepared, info.width)` for line-count / max-width hints and drive
        // buffer size from the same logical avail width. Full glyph/buffer sharing with
        // the layout-only FontSystem is still deferred (separate FontSystems).
        let (buffer_soft_w, buffer_soft_h) = if let Some(ref prepared) = info.prepared {
            let metrics = crate::text::layout(prepared, info.width.max(1.0));
            let soft_w = (info.width.max(1.0) * ui_scale).max(1.0);
            let soft_h = (metrics.height * ui_scale)
                .max(scaled_h)
                .max(line_height)
                + 2.0;
            let _ = metrics.line_count;
            let _ = metrics.max_line_width;
            (soft_w, soft_h)
        } else {
            (scaled_w.max(0.0), scaled_h + 2.0)
        };
        let align = resolve_text_align(st.align, st.direction);
        let display_text: Cow<'_, str> = if st.overflow == TextOverflow::Ellipsis
            && st.input_caret.is_none()
            && is_single_line_text_box(line_height, scaled_h)
        {
            Cow::Owned(truncate_with_ellipsis(
                unsafe { &mut *text_renderer.font_system },
                info.text.as_str(),
                scaled_size,
                line_height,
                font_weight,
                family,
                scaled_w.max(0.0),
            ))
        } else {
            Cow::Borrowed(info.text.as_str())
        };
        let shape_text: Cow<'_, str> = match info.shape_suffix.as_deref() {
            Some(s) if !s.is_empty() => Cow::Owned(format!("{}{}", display_text, s)),
            _ => display_text,
        };
        let spaced =
            apply_letter_spacing_approx(shape_text.as_ref(), st.letter_spacing, scaled_size);
        let mark = direction_mark(st.direction);
        let shape_text: Cow<'_, str> = if mark.is_empty() {
            Cow::Owned(spaced)
        } else {
            Cow::Owned(format!("{mark}{spaced}"))
        };
        let key_hash = node_shape_hash(
            shape_text.as_ref(),
            scaled_size,
            line_height,
            scaled_w,
            scaled_h,
            font_weight,
            family,
            align,
            shaping,
            st.letter_spacing,
            st.direction,
        );

        if preshape_only {
            let will_miss = text_renderer.shaped_cache.len() <= i
                || text_renderer.shaped_cache[i].key_hash != key_hash;
            if will_miss {
                preshape_miss_count += 1;
                if preshape_miss_count > effective_budget {
                    frame_meta.push(None);
                    continue;
                }
            }
        }

        while text_renderer.shaped_cache.len() <= i {
            text_renderer
                .shaped_cache
                .push(make_dummy_cache_entry(unsafe { &mut *text_renderer.font_system }));
        }
        let (measured_text_w, cache_hit) = {
            let entry = &mut text_renderer.shaped_cache[i];
            let cache_hit = entry.key_hash == key_hash;
            let measured_text_w = if cache_hit {
                entry.measured_text_w
            } else {
                _shape_misses += 1;
                let mut buffer = glyphon::Buffer::new(
                    unsafe { &mut *text_renderer.font_system },
                    glyphon::Metrics::new(scaled_size.max(1.0), line_height.max(1.0)),
                );
                buffer.set_size(
                    unsafe { &mut *text_renderer.font_system },
                    Some(buffer_soft_w),
                    Some(buffer_soft_h),
                );
                let attrs = glyphon::Attrs::new()
                    .family(family)
                    .weight(glyphon::fontdb::Weight(font_weight))
                    .color(glyphon::Color::rgba(
                        st.color[0],
                        st.color[1],
                        st.color[2],
                        st.color[3],
                    ));
                if let Some(spans) = info.spans.as_ref().filter(|s| !s.is_empty()) {
                    let mut owned: Vec<(String, glyphon::Attrs)> = Vec::with_capacity(spans.len());
                    for (si, span) in spans.iter().enumerate() {
                        let mut body =
                            apply_letter_spacing_approx(&span.text, st.letter_spacing, scaled_size);
                        if si == 0 {
                            let m = direction_mark(st.direction);
                            if !m.is_empty() {
                                body = format!("{m}{body}");
                            }
                        }
                        let mut a = attrs;
                        if let Some(c) = span.color {
                            a = a.color(glyphon::Color::rgba(c[0], c[1], c[2], c[3]));
                        }
                        if let Some(w) = span.weight {
                            a = a.weight(glyphon::fontdb::Weight(w));
                        }
                        if let Some(sz) = span.size {
                            let lh = line_height * (sz / st.size.max(1.0));
                            a = a.metrics(glyphon::Metrics::new(
                                (sz * ui_scale).max(1.0),
                                lh.max(1.0),
                            ));
                        }
                        if span.italic {
                            a = a.style(glyphon::Style::Italic);
                        }
                        if span.monospace {
                            a = a.family(glyphon::Family::Monospace);
                        }
                        // Chip/Mention/code backgrounds: metadata = span_index + 1.
                        if span.background.is_some()
                            || matches!(span.kind, TextSpanKind::Chip | TextSpanKind::Mention)
                        {
                            a = a.metadata(si + 1);
                        }
                        owned.push((body, a));
                    }
                    let rich: Vec<(&str, glyphon::Attrs)> =
                        owned.iter().map(|(t, a)| (t.as_str(), *a)).collect();
                    buffer.set_rich_text(unsafe { &mut *text_renderer.font_system }, rich, attrs, shaping);
                } else {
                    set_shaped_buffer_text(
                        &mut buffer,
                        unsafe { &mut *text_renderer.font_system },
                        shape_text.as_ref(),
                        attrs,
                        shaping,
                        emoji_custom_glyph_enabled() && has_emoji,
                    );
                }
                for line in buffer.lines.iter_mut() {
                    line.set_align(Some(align));
                }
                buffer.shape_until_scroll(unsafe { &mut *text_renderer.font_system }, false);
                let measured = buffer
                    .layout_runs()
                    .map(|run| run.line_w)
                    .fold(0.0_f32, f32::max);
                entry.key_hash = key_hash;
                entry.buffer = buffer;
                entry.measured_text_w = measured;
                measured
            };
            (measured_text_w, cache_hit)
        };

        let emoji_cache_key = if emoji_custom_glyph_enabled() && has_emoji {
            Some(super::text::EmojiCacheKey {
                sequence: shape_text.to_string(),
                pixel_size: scaled_size.ceil().clamp(1.0, u16::MAX as f32) as u16,
                scale_bits: ui_scale.to_bits(),
                color_mode: 1,
            })
        } else {
            None
        };
        if let Some(key) = emoji_cache_key.clone() {
            text_renderer.emoji_cache.stats.lookups += 1;
            if text_renderer.emoji_cache.entries.contains_key(&key) {
                text_renderer.emoji_cache.stats.hits += 1;
            } else {
                _emoji_misses += 1;
                let mut next_custom_id = text_renderer.emoji_cache.next_custom_id;
                let segments = split_text_with_emoji(shape_text.as_ref());
                let mut glyphs = Vec::new();
                let mut rasters = std::collections::HashMap::new();
                // When font advances collapse (missing emoji face), Twemoji quads would
                // all land on the same x — advance a layout cursor as a backstop.
                let mut emoji_left_cursor = 0.0_f32;
                for seg in segments {
                    let TextSegment::Emoji { text, start } = seg else {
                        continue;
                    };
                    let (mut left, top) = match emoji_layout::custom_emoji_left_top(
                        &text_renderer.shaped_cache[i].buffer,
                        shape_text.as_ref(),
                        start,
                        x,
                        y,
                        1.0,
                        unsafe { &mut *text_renderer.font_system },
                        &mut text_renderer.swash_cache,
                    ) {
                        Some((l, t)) => (l, t + emoji_baseline_layout_tweak(scaled_size)),
                        None => {
                            let prefix = shape_text.get(..start).unwrap_or("");
                            let left = measure_prefix_width(
                                unsafe { &mut *text_renderer.font_system },
                                prefix,
                                scaled_size,
                                line_height,
                                font_weight,
                                family,
                                scaled_w.max(1.0),
                            );
                            if emoji_debug_enabled() {
                                log::debug!(
                                    "emoji layout fallback: no glyph for utf8 byte index {}",
                                    start
                                );
                            }
                            (left, emoji_baseline_offset_fallback(scaled_size))
                        }
                    };
                    let pixel_size = scaled_size.ceil().clamp(1.0, 512.0) as u16;
                    let slot = pixel_size as f32;
                    if left + 0.5 < emoji_left_cursor {
                        left = emoji_left_cursor;
                    }
                    emoji_left_cursor = left + slot * 0.95;
                    let glyph_id = next_custom_id;
                    next_custom_id = next_custom_id.wrapping_add(1).max(1);
                    let raster = load_emoji_raster_cached(
                        text_renderer,
                        &text,
                        pixel_size,
                        family,
                        font_weight,
                    );
                    if let Some(raster) = raster {
                        // Match logical quad to font size (raster already resized).
                        let width = pixel_size as f32;
                        let height = pixel_size as f32;
                        text_renderer.emoji_cache.stats.uploads += 1;
                        text_renderer.emoji_cache.stats.upload_bytes += raster.rgba.len() as u64;
                        glyphs.push(glyphon::CustomGlyph {
                            id: glyph_id,
                            left,
                            top,
                            width,
                            height,
                            color: None,
                            snap_to_physical_pixel: true,
                            metadata: 0,
                        });
                        rasters.insert(glyph_id, raster);
                    }
                }
                if emoji_debug_enabled() {
                    log::debug!(
                        "emoji cache miss sequence_len={} glyphs={} uploads={} bytes={}",
                        key.sequence.len(),
                        glyphs.len(),
                        text_renderer.emoji_cache.stats.uploads,
                        text_renderer.emoji_cache.stats.upload_bytes
                    );
                }
                text_renderer
                    .emoji_cache
                    .insert_emoji_node(key, super::text::EmojiGlyphEntry { glyphs, rasters });
                text_renderer.emoji_cache.next_custom_id = next_custom_id;
            }
        }

        if preshape_only {
            if !cache_hit && !info.text.is_ascii() {
                if initial_preshape {
                    text_renderer.pending_warmup.push(i);
                } else {
                    preshape_warmup.push((i, x, y, scaled_w, scaled_h));
                }
            }
            frame_meta.push(None);
            continue;
        }

        if super::text_prepare_debug::enabled()
            && super::text_prepare_debug::should_log(
                info.template_id.as_deref(),
                st.input_caret.is_some(),
            )
            && (!super::text_prepare_debug::reshape_only() || !cache_hit)
        {
            let lh_log = line_height / ui_scale.max(0.001);
            super::text_prepare_debug::log_line(
                i,
                info,
                family,
                font_weight,
                lh_log,
                st.input_caret.is_some(),
                &text_renderer.shaped_cache[i].buffer,
                unsafe { &*text_renderer.font_system },
                cache_hit,
                shaping,
            );
        }
        super::text_prepare_debug::maybe_log_family_mismatch(
            info,
            family,
            font_weight,
            &text_renderer.shaped_cache[i].buffer,
            unsafe { &*text_renderer.font_system },
        );

        let dy = 0.0;
        let sdx = st.shadow_offset[0] * ui_scale;
        let sdy = st.shadow_offset[1] * ui_scale;
        let sblur = st.shadow_blur * ui_scale;
        let has_shadow = st.shadow_color.is_some() && sblur > 0.0;

        if has_shadow {
            let Some(sc) = st.shadow_color else { continue };
            let skey = shadow_cache_key(key_hash, sc, sdx, sdy, sblur);
            if shadow_cached_keys.contains(&skey) {
                if let Some(cached_rect) = shadow_cached_rects.get(&skey) {
                    shadow_tex_draws.push(ShadowTexDraw {
                        cache_key: skey,
                        rect: [
                            x + cached_rect[0],
                            y + cached_rect[1],
                            cached_rect[2],
                            cached_rect[3],
                        ],
                    });
                }
                shadow_skip_indices.insert(i);
            } else {
                let entry = &text_renderer.shaped_cache[i];
                if let Some(bmp) = generate_shadow_bitmap(
                    &entry.buffer,
                    unsafe { &mut *text_renderer.font_system },
                    &mut text_renderer.swash_cache,
                    sc,
                    sdx,
                    sdy,
                    sblur,
                ) {
                    shadow_tex_draws.push(ShadowTexDraw {
                        cache_key: skey,
                        rect: [
                            x + bmp.draw_rect[0],
                            y + bmp.draw_rect[1],
                            bmp.draw_rect[2],
                            bmp.draw_rect[3],
                        ],
                    });
                    shadow_new_bitmaps.push(ShadowNewBitmap {
                        cache_key: skey,
                        rgba: bmp.rgba,
                        width: bmp.width,
                        height: bmp.height,
                        draw_rect: bmp.draw_rect,
                    });
                    shadow_skip_indices.insert(i);
                }
                // 如果 generate_shadow_bitmap 返回 None（无法栅格化字形），
                // 不加入 shadow_skip_indices，回退到 glyphon TextArea 渲染。
            }
        }

        frame_meta.push(Some(FrameTextNodeMeta {
            x,
            y,
            box_w: scaled_w,
            box_h: scaled_h,
            dy,
            text_size: scaled_size,
            align: st.align,
            measured_text_w,
            underline: st.underline,
            overline: st.overline,
            line_through: st.line_through,
            decoration_style: st.decoration_style,
            decoration_color: st.decoration_color,
            decoration_thickness: st.decoration_thickness,
            underline_offset: st.underline_offset,
            color: st.color,
            shadow_col: st.shadow_color,
            shadow_dx: sdx,
            shadow_dy: sdy,
            shadow_blur: sblur,
            clip_bounds: scaled_clip,
            emoji_cache_key,
        }));
    }

    let _t1 = std::time::Instant::now();

    let mut underline_instances: Vec<InstanceInput> = Vec::new();
    let caret_instances: Vec<InstanceInput> = Vec::new();

    // Chip/Mention backgrounds (`rui_chip` Attrs.metadata = span_index+1). Drawn under glyphs.
    for (i, info) in text_nodes.iter().enumerate() {
        let Some(spans) = info.spans.as_ref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if !spans.iter().any(|s| s.background.is_some()) {
            continue;
        }
        let Some(tb) = frame_meta.get(i).and_then(|x| x.as_ref()) else {
            continue;
        };
        let entry = &text_renderer.shaped_cache[i];
        for run in entry.buffer.layout_runs() {
            let line_top = run.line_top;
            let line_height = run.line_height;
            let mut cur_meta: Option<usize> = None;
            let mut min_x = 0.0_f32;
            let mut max_x = 0.0_f32;
            let flush_group =
                |meta: usize, min_x: f32, max_x: f32, instances: &mut Vec<InstanceInput>| {
                    let Some(span) = spans.get(meta.saturating_sub(1)) else {
                        return;
                    };
                    let Some(bg) = span.background else {
                        return;
                    };
                    if max_x <= min_x {
                        return;
                    }
                    let pad = span.padding_x * ui_scale;
                    let bx = tb.x + min_x - pad;
                    let by = tb.y + line_top;
                    let bw = (max_x - min_x) + 2.0 * pad;
                    let bh = line_height.max(1.0);
                    let radius = (bh * 0.25).min(6.0 * ui_scale);
                    let Some([cx, cy, cw, ch]) =
                        clip_rect_to_bounds([bx, by, bw, bh], tb.clip_bounds)
                    else {
                        return;
                    };
                    instances.push(InstanceInput {
                        rect_pos: [cx, cy],
                        rect_size: [cw, ch],
                        color: [
                            bg[0] as f32 / 255.0,
                            bg[1] as f32 / 255.0,
                            bg[2] as f32 / 255.0,
                            bg[3] as f32 / 255.0,
                        ],
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        radii: [radius, radius, radius, radius],
                        border_width: 0.0,
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_spread: 0.0,
                        shadow_inset: 0.0,
                        rotate_deg: 0.0,
                        _padding: 0.0,
                    });
                };
            for g in run.glyphs.iter() {
                let meta = g.metadata;
                if meta == 0 {
                    if let Some(m) = cur_meta.take() {
                        flush_group(m, min_x, max_x, &mut underline_instances);
                    }
                    continue;
                }
                if cur_meta != Some(meta) {
                    if let Some(m) = cur_meta.take() {
                        flush_group(m, min_x, max_x, &mut underline_instances);
                    }
                    cur_meta = Some(meta);
                    min_x = g.x;
                    max_x = g.x + g.w;
                } else {
                    min_x = min_x.min(g.x);
                    max_x = max_x.max(g.x + g.w);
                }
            }
            if let Some(m) = cur_meta.take() {
                flush_group(m, min_x, max_x, &mut underline_instances);
            }
        }
    }

    for (i, tb) in frame_meta.iter().enumerate() {
        let Some(tb) = tb.as_ref() else {
            continue;
        };
        let entry = &text_renderer.shaped_cache[i];
        if let Some(sc) = tb.shadow_col {
            if shadow_skip_indices.contains(&i) {
                // 阴影已通过纹理缓存渲染，跳过 glyphon TextArea
            } else {
                let radius = tb.shadow_blur;
                let alpha = sc[3];
                if alpha > 0 {
                    let shadow_perf = text_perf_mode();
                    let center_left = tb.x + tb.shadow_dx;
                    let center_top = tb.y + tb.dy + tb.shadow_dy;
                    let (min_radius, min_alpha, base_samples, radial_scale, max_samples) =
                        match shadow_perf {
                            TextPerfMode::HighQuality => {
                                (0.35_f32, 6_u8, 20_usize, 9.0_f32, 72_usize)
                            }
                            TextPerfMode::Balanced => (0.8_f32, 20_u8, 12_usize, 6.5_f32, 48_usize),
                            TextPerfMode::Fast => (1.5_f32, 40_u8, 4_usize, 3.0_f32, 16_usize),
                        };
                    const SHARP_BLUR_EPS: f32 = 0.75;
                    let shadow_bounds_clip = shadow_clip_for_glyphon(
                        tb.clip_bounds,
                        radius + tb.shadow_dx.abs() + tb.shadow_dy.abs() + 3.0,
                        viewport_clip,
                    );
                    let bounds = text_bounds_from_clip(shadow_bounds_clip, config);

                    let mut push_shadow_layer = |left: f32, top: f32, layer_alpha: u8| {
                        if layer_alpha == 0 {
                            return;
                        }
                        let shadow_rect = [left, top, tb.box_w, tb.box_h];
                        let Some(shadow_visible) = clip_rect_to_bounds(shadow_rect, tb.clip_bounds)
                        else {
                            return;
                        };
                        if clip_rect_to_bounds(shadow_visible, Some(viewport_clip)).is_none() {
                            return;
                        }
                        text_areas.push(glyphon::TextArea {
                            buffer: &entry.buffer,
                            left,
                            top,
                            scale: 1.0,
                            bounds,
                            default_color: glyphon::Color::rgba(sc[0], sc[1], sc[2], layer_alpha),
                            custom_glyphs: tb
                                .emoji_cache_key
                                .as_ref()
                                .and_then(|k| text_renderer.emoji_cache.entries.get(k))
                                .map_or(&[], |v| v.glyphs.as_slice()),
                        });
                    };

                    let draw_blur =
                        radius > SHARP_BLUR_EPS && radius > min_radius && alpha > min_alpha;
                    if !draw_blur {
                        push_shadow_layer(center_left, center_top, alpha);
                    } else {
                        const GOLDEN_ANGLE: f32 = 2.399_963_1;
                        let sample_count = (base_samples as f32 + radius * radial_scale)
                            .round()
                            .clamp(base_samples as f32, max_samples as f32)
                            as usize;
                        let sigma = (radius * 0.58).max(0.75);
                        let two_sigma2 = 2.0 * sigma * sigma;

                        let mut weight_sum = 1.0_f32; // center sample
                        for idx in 0..sample_count {
                            let t = (idx as f32 + 0.5) / (sample_count as f32);
                            let r = radius * t.sqrt();
                            let w = (-(r * r) / two_sigma2).exp();
                            weight_sum += w;
                        }
                        let inv = 1.0 / weight_sum.max(0.001);
                        let center_alpha = ((alpha as f32) * inv).round().clamp(0.0, 255.0) as u8;
                        push_shadow_layer(center_left, center_top, center_alpha);

                        for idx in 0..sample_count {
                            let t = (idx as f32 + 0.5) / (sample_count as f32);
                            let r = radius * t.sqrt();
                            let theta = idx as f32 * GOLDEN_ANGLE;
                            let rx = theta.cos() * r;
                            let ry = theta.sin() * r;
                            let w = (-(r * r) / two_sigma2).exp();
                            let wa = ((alpha as f32) * w * inv).round().clamp(0.0, 255.0) as u8;
                            push_shadow_layer(center_left + rx, center_top + ry, wa);
                        }
                    }
                }
            }
        }

        let bounds = text_bounds_from_clip(tb.clip_bounds, config);
        text_areas.push(glyphon::TextArea {
            buffer: &entry.buffer,
            left: tb.x,
            top: tb.y,
            scale: 1.0,
            bounds,
            default_color: glyphon::Color::rgba(tb.color[0], tb.color[1], tb.color[2], tb.color[3]),
            custom_glyphs: tb
                .emoji_cache_key
                .as_ref()
                .and_then(|k| text_renderer.emoji_cache.entries.get(k))
                .map_or(&[], |v| v.glyphs.as_slice()),
        });

        if emoji_debug_enabled() {
            if let Some(entry) = tb
                .emoji_cache_key
                .as_ref()
                .and_then(|k| text_renderer.emoji_cache.entries.get(k))
            {
                let baseline_y = tb.y + tb.text_size * 0.86;
                if let Some([cx, cy, cw, ch]) =
                    clip_rect_to_bounds([tb.x, baseline_y, tb.box_w, 1.0], tb.clip_bounds)
                {
                    underline_instances.push(InstanceInput {
                        rect_pos: [cx, cy],
                        rect_size: [cw, ch],
                        color: [1.0, 0.2, 0.2, 0.75],
                        border_color: [0.0, 0.0, 0.0, 0.0],
                        radii: [0.0, 0.0, 0.0, 0.0],
                        border_width: 0.0,
                        shadow_color: [0.0, 0.0, 0.0, 0.0],
                        shadow_offset: [0.0, 0.0],
                        shadow_blur: 0.0,
                        shadow_spread: 0.0,
                        shadow_inset: 0.0,
                        rotate_deg: 0.0,
                        _padding: 0.0,
                    });
                }
                for g in &entry.glyphs {
                    let gx = tb.x + g.left;
                    let gy = tb.y + g.top;
                    let gw = g.width.max(1.0);
                    let gh = g.height.max(1.0);
                    for line in [
                        [gx, gy, gw, 1.0],
                        [gx, gy + gh - 1.0, gw, 1.0],
                        [gx, gy, 1.0, gh],
                        [gx + gw - 1.0, gy, 1.0, gh],
                    ] {
                        if let Some([cx, cy, cw, ch]) = clip_rect_to_bounds(line, tb.clip_bounds) {
                            underline_instances.push(InstanceInput {
                                rect_pos: [cx, cy],
                                rect_size: [cw, ch],
                                color: [0.0, 1.0, 0.0, 0.8],
                                border_color: [0.0, 0.0, 0.0, 0.0],
                                radii: [0.0, 0.0, 0.0, 0.0],
                                border_width: 0.0,
                                shadow_color: [0.0, 0.0, 0.0, 0.0],
                                shadow_offset: [0.0, 0.0],
                                shadow_blur: 0.0,
                                shadow_spread: 0.0,
                                shadow_inset: 0.0,
                                rotate_deg: 0.0,
                                _padding: 0.0,
                            });
                        }
                    }
                }
            }
        }
    }

    for maybe_tb in &frame_meta {
        let Some(tb) = maybe_tb.as_ref() else {
            continue;
        };
        if tb.underline || tb.overline || tb.line_through {
            let ul_h = if tb.decoration_thickness > 0.0 {
                tb.decoration_thickness
            } else {
                (tb.text_size * 0.07).max(1.0)
            };
            let ul_w = tb.measured_text_w.min(tb.box_w);
            let ul_x = match tb.align {
                glyphon::cosmic_text::Align::Center => tb.x + (tb.box_w - ul_w) * 0.5,
                glyphon::cosmic_text::Align::Right => tb.x + (tb.box_w - ul_w),
                _ => tb.x,
            };
            let dc = tb.decoration_color.unwrap_or(tb.color);
            let line_color = [
                dc[0] as f32 / 255.0,
                dc[1] as f32 / 255.0,
                dc[2] as f32 / 255.0,
                dc[3] as f32 / 255.0,
            ];
            let mut push_line = |x: f32, y: f32, w: f32, h: f32| {
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let Some([cx, cy, cw, ch]) = clip_rect_to_bounds([x, y, w, h], tb.clip_bounds)
                else {
                    return;
                };
                underline_instances.push(InstanceInput {
                    rect_pos: [cx, cy],
                    rect_size: [cw, ch],
                    color: line_color,
                    border_color: [0.0, 0.0, 0.0, 0.0],
                    radii: [0.0, 0.0, 0.0, 0.0],
                    border_width: 0.0,
                    shadow_color: [0.0, 0.0, 0.0, 0.0],
                    shadow_offset: [0.0, 0.0],
                    shadow_blur: 0.0,
                    shadow_spread: 0.0,
                    shadow_inset: 0.0,
                    rotate_deg: 0.0,
                    _padding: 0.0,
                });
            };
            let mut draw_at = |line_y: f32| match tb.decoration_style {
                TextDecorationStyle::Solid => {
                    push_line(ul_x, line_y, ul_w, ul_h);
                }
                TextDecorationStyle::Double => {
                    let gap = (ul_h * 0.8).max(1.0);
                    push_line(ul_x, line_y - gap, ul_w, ul_h);
                    push_line(ul_x, line_y + gap, ul_w, ul_h);
                }
                TextDecorationStyle::Dotted => {
                    // 明显留出间隔，避免细线 + 密集采样看起来像 solid
                    let dot_w = ((ul_h * 1.35).max(2.0)).min(ul_w.max(1.0));
                    let gap = (ul_h * 1.1).max(2.0);
                    let step = dot_w + gap;
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        let w = dot_w.min(ul_x + ul_w - cx);
                        if w > 0.0 {
                            push_line(cx, line_y, w, ul_h);
                        }
                        cx += step;
                    }
                }
                TextDecorationStyle::Dashed => {
                    let dash = (ul_h * 4.0).max(4.0);
                    let gap = (ul_h * 2.0).max(2.0);
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        push_line(cx, line_y, dash.min(ul_x + ul_w - cx), ul_h);
                        cx += dash + gap;
                    }
                }
                TextDecorationStyle::Wavy => {
                    // 沿正弦路径采样短横条，避免“上下交替”被看成两条虚线。
                    let wavelength = (ul_h * 6.0).max(8.0);
                    let amp = (ul_h * 0.75).max(1.0);
                    let step = (ul_h * 0.65).max(1.0);
                    let mut cx = ul_x;
                    while cx < ul_x + ul_w {
                        let t = ((cx - ul_x) / wavelength) * std::f32::consts::TAU;
                        let y = line_y + amp * t.sin();
                        let w = step.min(ul_x + ul_w - cx);
                        push_line(cx, y, w, ul_h);
                        cx += step;
                    }
                }
            };
            if tb.underline {
                // Keep underline visually closer to the text bottom edge.
                let y = tb.y + tb.box_h - (ul_h * 0.45) - (tb.underline_offset * ui_scale);
                draw_at(y);
            }
            if tb.overline {
                let y = tb.y + (tb.underline_offset * ui_scale);
                draw_at(y);
            }
            if tb.line_through {
                let y = tb.y + (tb.box_h - ul_h) * 0.5;
                draw_at(y);
            }
        }
    }

    // Selection + caret chrome are emitted during layout collect (`input_chrome_instances`).
    let _ = (&caret_instances, ui_scale);

    const WARMUP_BATCH_SIZE: usize = 4;
    if !initial_preshape && !text_renderer.pending_warmup.is_empty() {
        let batch_end = WARMUP_BATCH_SIZE.min(text_renderer.pending_warmup.len());
        for &idx in &text_renderer.pending_warmup[..batch_end] {
            if idx < text_nodes.len() && idx < text_renderer.shaped_cache.len() {
                let info = &text_nodes[idx];
                let sw = info.width * ui_scale;
                let sh = info.height * ui_scale;
                preshape_warmup.push((idx, 0.0, 0.0, sw, sh));
            }
        }
        text_renderer.pending_warmup.drain(..batch_end);
    }

    let _t2 = std::time::Instant::now();

    text_renderer.viewport.update(
        queue,
        glyphon::Resolution {
            width: config.width,
            height: config.height,
        },
    );

    let _warmup_count = preshape_warmup.len();
    if !preshape_warmup.is_empty() {
        let mut warmup_areas: Vec<glyphon::TextArea> = Vec::with_capacity(preshape_warmup.len());
        for &(idx, _px, _py, pw, ph) in &preshape_warmup {
            let entry = &text_renderer.shaped_cache[idx];
            let bw = pw.ceil().min(config.width as f32) as i32;
            let bh = ph.ceil().min(config.height as f32) as i32;
            warmup_areas.push(glyphon::TextArea {
                buffer: &entry.buffer,
                left: 0.0,
                top: 0.0,
                scale: 1.0,
                bounds: glyphon::TextBounds {
                    left: 0,
                    top: 0,
                    right: bw.max(1),
                    bottom: bh.max(1),
                },
                default_color: glyphon::Color::rgba(0, 0, 0, 0),
                custom_glyphs: &[],
            });
        }
        let fs = text_renderer.font_system;
        if let Err(err) = text_renderer.text_renderer.prepare_with_custom(
            device,
            queue,
            unsafe { &mut *fs },
            &mut text_renderer.text_atlas,
            &text_renderer.viewport,
            warmup_areas,
            &mut text_renderer.swash_cache,
            |input| text_renderer.emoji_cache.rasterize(input),
        ) {
            log::error!("glyphon warmup prepare failed: {err:?}");
        }
    }

    let text_area_count = text_areas.len();
    let fs = text_renderer.font_system;
    if let Err(err) = text_renderer.text_renderer.prepare_with_custom(
        device,
        queue,
        unsafe { &mut *fs },
        &mut text_renderer.text_atlas,
        &text_renderer.viewport,
        text_areas,
        &mut text_renderer.swash_cache,
        |input| text_renderer.emoji_cache.rasterize(input),
    ) {
        log::error!("glyphon prepare failed: {err:?}");
    }
    emoji_metrics_log_tick(text_renderer.emoji_cache.stats);
    text_renderer.text_area_capacity_hint = text_area_count;

    let _t3 = std::time::Instant::now();
    let _total_ms = (_t3 - _t0).as_secs_f64() * 1000.0;
    if _total_ms > 4.0 {
        log::warn!(
            "prepare_text_frame {:.1}ms | loop={:.1}ms areas={:.1}ms gpu={:.1}ms | nodes={} shape_miss={} emoji_miss={} areas={} warmup={} preshape={}",
            _total_ms,
            (_t1 - _t0).as_secs_f64() * 1000.0,
            (_t2 - _t1).as_secs_f64() * 1000.0,
            (_t3 - _t2).as_secs_f64() * 1000.0,
            text_nodes.len(),
            _shape_misses,
            _emoji_misses,
            text_area_count,
            _warmup_count,
            initial_preshape,
        );
    }

    PrepareTextOutput {
        underline_instances,
        caret_instances,
        shadow_new_bitmaps,
        shadow_tex_draws,
        needs_warmup: !text_renderer.pending_warmup.is_empty(),
    }
}
