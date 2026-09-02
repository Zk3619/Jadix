//! 将布局得到的 `TextLayoutInfo` 转为 glyphon 文本区与下划线实例，并执行 `prepare`。
//!
//! 调试：设置环境变量 `RUI_TEXT_DEBUG`（见 [`super::text_prepare_debug`]）。

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
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
use super::text_hit::{best_font_family, measure_prefix_width, truncate_with_ellipsis};
use super::twemoji_name;
use super::InstanceInput;

#[derive(Debug, Clone)]
enum TextSegment {
    Plain,
    Emoji { text: String, start: usize },
}

/// CJK `一` slot — ~1em advance; must not be whitespace (see [`emoji_layout::EMOJI_LAYOUT_SLOT`]).
const EMOJI_LAYOUT_SLOT: char = emoji_layout::EMOJI_LAYOUT_SLOT;

/// Replace each emoji grapheme with a single-width slot so layout advance matches the
/// custom glyph square. Keeps ZWJ/RI clusters from reserving 2–3× em of invisible Mask width.
fn plan_emoji_layout(text: &str) -> (String, Vec<TextSegment>) {
    let mut layout = String::with_capacity(text.len());
    let mut segments = Vec::new();
    let mut plain = false;
    for grapheme in text.graphemes(true) {
        if emoji_detect::grapheme_is_emoji(grapheme) {
            if plain {
                segments.push(TextSegment::Plain);
                plain = false;
            }
            segments.push(TextSegment::Emoji {
                text: grapheme.to_string(),
                start: layout.len(),
            });
            layout.push(EMOJI_LAYOUT_SLOT);
        } else {
            layout.push_str(grapheme);
            plain = true;
        }
    }
    if plain {
        segments.push(TextSegment::Plain);
    }
    (layout, segments)
}

/// Pair original emoji graphemes with UTF-8 starts of `EMOJI_LAYOUT_SLOT` in slotted layout text
/// (e.g. UAX#14 `fit.shape_text` that already replaced emoji and inserted soft `\n`).
fn emoji_segments_from_slotted_layout(original: &str, slotted_layout: &str) -> Vec<TextSegment> {
    let emojis: Vec<String> = original
        .graphemes(true)
        .filter(|g| emoji_detect::grapheme_is_emoji(g))
        .map(|g| g.to_string())
        .collect();
    if emojis.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(emojis.len());
    let mut emoji_i = 0usize;
    for (idx, c) in slotted_layout.char_indices() {
        if c == EMOJI_LAYOUT_SLOT {
            if let Some(text) = emojis.get(emoji_i) {
                out.push(TextSegment::Emoji {
                    text: text.clone(),
                    start: idx,
                });
                emoji_i += 1;
            }
        }
    }
    out
}

fn buffer_joined_text(buffer: &glyphon::Buffer) -> String {
    let mut out = String::new();
    for (i, line) in buffer.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line.text());
    }
    out
}

/// True when the shaped buffer already carries UAX#14 hard breaks for this leaf width.
///
/// Fast Miss used to shape raw BiDi with `Wrap::WordOrGlyph`. Settle then hit Bounds
/// (same `shape_hash`) and reused that buffer — WJ pairs stayed illegally split.
fn bidi_prewrap_applied(
    buffer: &glyphon::Buffer,
    text: &str,
    size: f32,
    weight: u16,
    logical_lh: f32,
    layout_w: f32,
) -> bool {
    let target = crate::text::layout_wrap_target_prewrap(layout_w).max(1.0);
    let Some(fit) = crate::text::try_glyphon_fit_soft_wrap(text, size, weight, logical_lh, target)
    else {
        return false;
    };
    buffer_joined_text(buffer) == fit.shape_text.as_ref()
}

#[derive(Clone)]
struct FrameTextNodeMeta {
    x: f32,
    y: f32,
    box_w: f32,
    box_h: f32,
    dy: f32,
    text_size: f32,
    /// CSS/`ancestor_scale` → glyphon `TextArea.scale` (1 = identity).
    paint_scale: f32,
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
    /// Nodes existed but every TextArea was culled (common mid-resize). Atlas was **not**
    /// wiped with an empty `prepare` — caller must not treat this as a successful prepare cache.
    pub culled_skip_prepare: bool,
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
    // Textured quad: rect top → uv.y = 0 → uploaded row 0 (see textured_pipeline).
    // Do **not** Y-flip here — that matched the old V=1-at-top convention and made
    // shadows vanish / invert after the Bake/capture UV fix.
    for y in 0..bh as usize {
        let row = y * bw as usize;
        for x in 0..bw as usize {
            let a = alpha_buf[row + x].min(255) as u8;
            if a > 0 {
                let di = (row + x) * 4;
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

fn quantize_shape_dim(v: f32, force_fast: bool) -> u16 {
    let q = if force_fast && v > 0.0 {
        // egui-style: coarsen during interactive resize to cut churn; paint still set_size
        // to the real leaf width on Bounds path.
        (v / 2.0).round() * 2.0
    } else {
        v.round()
    };
    q.clamp(0.0, u16::MAX as f32) as u16
}

/// Content + style identity (no leaf bounds). Matching this with different width = Bounds miss.
fn node_shape_identity_hash(
    text: &str,
    scaled_size: f32,
    line_height: f32,
    weight: u16,
    family: glyphon::Family,
    align: glyphon::cosmic_text::Align,
    shaping: glyphon::Shaping,
    letter_spacing: f32,
    direction: TextDirection,
    color: [u8; 4],
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    ((scaled_size * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
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
    color.hash(&mut h);
    h.finish()
}

/// Test helper: full Exact key = identity ⊕ quantized bounds.
#[cfg(test)]
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
    // Baked into glyphon Attrs on shape — must invalidate when opacity/color flips
    // (e.g. Input adornments hidden with opacity 0 then shown).
    color: [u8; 4],
    force_fast_mode: bool,
) -> u64 {
    let identity = node_shape_identity_hash(
        text,
        scaled_size,
        line_height,
        weight,
        family,
        align,
        shaping,
        letter_spacing,
        direction,
        color,
    );
    node_shape_key_from_identity(identity, scaled_w, scaled_h, force_fast_mode)
}

/// Bounds half of the Exact key — avoids re-hashing full text when identity is known.
fn node_shape_key_from_identity(
    identity: u64,
    scaled_w: f32,
    scaled_h: f32,
    force_fast_mode: bool,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut h);
    // Bounds: 1px at rest; 2px during resize/scroll fast path (fewer Exact misses →
    // more cheap Bounds `set_size` paths). Never reuse a buffer without set_size.
    quantize_shape_dim(scaled_w, force_fast_mode).hash(&mut h);
    quantize_shape_dim(scaled_h, force_fast_mode).hash(&mut h);
    h.finish()
}

fn make_dummy_cache_entry(font_system: &mut glyphon::FontSystem) -> super::text::ShapedTextEntry {
    let mut buffer = glyphon::Buffer::new(font_system, glyphon::Metrics::new(1.0, 1.0));
    buffer.set_size(Some(1.0), Some(1.0));
    super::text::ShapedTextEntry {
        key_hash: 0,
        shape_hash: 0,
        buffer,
        measured_text_w: 0.0,
        has_emoji: false,
    }
}

/// After shaping: if cosmic ink spills the leaf horizontally **or** soft-wrap line count
/// exceeds the leaf height, run one `glyphon_fit` (fills FIT_CACHE) and request relayout.
/// During resize/scroll fast mode, skip the expensive fit (settle frame corrects).
fn correct_bidi_ink_spill(
    buffer: &mut glyphon::Buffer,
    font_system: &mut glyphon::FontSystem,
    text: &str,
    size: f32,
    weight: u16,
    logical_lh: f32,
    fit_lookup_w: f32,
    soft_w_target: f32,
    ui_scale: f32,
    line_height: f32,
    scaled_h: f32,
    height_opt: Option<f32>,
    wrap_w: f32,
    nowrap: bool,
    force_fast: bool,
) -> f32 {
    if nowrap || text.is_ascii() || soft_w_target <= 8.0 {
        return wrap_w;
    }
    let mut max_ink = 0.0_f32;
    let mut runs = 0usize;
    for run in buffer.layout_runs() {
        runs += 1;
        let gr = run.glyphs.iter().map(|g| g.x + g.w).fold(0.0_f32, f32::max);
        max_ink = max_ink.max(gr).max(run.line_w);
    }
    let horizontal_spill = max_ink > soft_w_target + 1.0;
    let vertical_spill = (runs as f32 * line_height) > scaled_h + 1.0;
    if !horizontal_spill && !vertical_spill {
        return wrap_w;
    }
    if force_fast {
        // Keep resize fluid; resting frame (!fast) will fit + relayout.
        return wrap_w;
    }
    let fit = crate::text::glyphon_fit_soft_wrap(text, size, weight, logical_lh, fit_lookup_w);
    let fit_w = (fit.wrap_width * ui_scale).max(1.0);
    if crate::text::needs_logical_prewrap(text) {
        // Width-dependent UAX#14 prewrap: must set_text, not only set_size.
        let attrs_owned = buffer
            .lines
            .first()
            .map(|l| glyphon::AttrsOwned::new(&l.attrs_list().defaults()))
            .unwrap_or_else(|| glyphon::AttrsOwned::new(&glyphon::Attrs::new()));
        let aligns: Vec<_> = buffer.lines.iter().map(|l| l.align()).collect();
        buffer.set_wrap(glyphon::Wrap::None);
        // fit.shape_text may already contain emoji slots (`一`) — keep them invisible.
        set_shaped_buffer_text(
            buffer,
            font_system,
            fit.shape_text.as_ref(),
            attrs_owned.as_attrs(),
            glyphon::Shaping::Advanced,
            emoji_custom_glyph_enabled(),
            size * ui_scale,
            line_height,
        );
        for (line, align) in buffer.lines.iter_mut().zip(aligns) {
            line.set_align(align);
        }
        buffer.set_size(Some(fit_w), height_opt);
        buffer.shape_until_scroll(font_system, false);
    } else if horizontal_spill && (fit_w - wrap_w).abs() > 0.5 {
        buffer.set_size(Some(fit_w), height_opt);
        buffer.shape_until_scroll(font_system, false);
    }
    let need_h = fit.line_count.max(1) as f32 * line_height;
    if need_h > scaled_h + 1.0 || vertical_spill || fit.line_count > runs {
        crate::text::note_fit_needs_relayout();
    }
    if horizontal_spill {
        fit_w
    } else {
        wrap_w
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
    // Unit tests call push_input_chrome_instances in a tight loop; wall-clock blink
    // phase stays OFF for all iterations and flakes caret geometry tests.
    if cfg!(test) {
        return true;
    }
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
    let lh = if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(|| crate::utils::text_field::text_edit_default_leading_scale());
        scaled_size * scale
    };
    lh.max(scaled_size * crate::utils::text_field::text_edit_default_leading_scale())
}

/// Glyphon wrap/align width for nowrap runs.
/// Hidden/clip keep the full line (pan + scissor). Static ellipsis already fits `box_w`.
fn nowrap_buffer_width(
    overflow: TextOverflow,
    static_ellipsis: bool,
    intrinsic: f32,
    box_w: f32,
) -> f32 {
    if overflow == TextOverflow::Ellipsis && static_ellipsis {
        box_w.max(1.0)
    } else {
        intrinsic.max(box_w).max(1.0)
    }
}

/// Live intrinsic for `whitespace-nowrap` paint.
///
/// Prefer this over a layout-attached `PreparedText`: IME preedit is spliced into
/// `info.text` every keystroke while `prepared` can lag one frame. A stale (too
/// narrow) wrap width lets Word-wrap invent extra runs so a textarea line appears
/// to jump downward while composing Latin syllables after CJK.
fn nowrap_layout_width_from_text(
    text: &str,
    size: f32,
    weight: u16,
    line_height_logical: f32,
    overflow: TextOverflow,
    static_ellipsis: bool,
    box_w: f32,
) -> f32 {
    let prepared = crate::text::prepare(text, size, weight, line_height_logical.max(1.0));
    nowrap_buffer_width(
        overflow,
        static_ellipsis,
        crate::text::intrinsic_width(&prepared),
        box_w,
    )
}

/// Cap for band-cull identity pool (excess parked entries are dropped).
const SHAPED_IDENTITY_POOL_CAP: usize = 128;

#[inline]
fn park_shaped_entry(
    pool: &mut std::collections::HashMap<u64, super::text::ShapedTextEntry>,
    entry: super::text::ShapedTextEntry,
) {
    if entry.shape_hash == 0 {
        return;
    }
    while pool.len() >= SHAPED_IDENTITY_POOL_CAP && !pool.contains_key(&entry.shape_hash) {
        if let Some(&k) = pool.keys().next() {
            pool.remove(&k);
        } else {
            break;
        }
    }
    pool.insert(entry.shape_hash, entry);
}

/// Scroll fast-path: Exact-hit idle Input may skip `shape_until_scroll`.
#[inline]
pub(crate) fn scroll_fast_skips_input_reshape(
    force_fast_mode: bool,
    is_input_text: bool,
    has_caret: bool,
) -> bool {
    force_fast_mode && is_input_text && !has_caret
}

/// Scroll fast-path: idle Input may avoid live `text::prepare` for nowrap width.
#[inline]
pub(crate) fn scroll_fast_skips_input_intrinsic_prepare(
    force_fast_mode: bool,
    is_input_text: bool,
    has_caret: bool,
    scroll_x: f32,
) -> bool {
    force_fast_mode && is_input_text && !has_caret && scroll_x.abs() < 0.5
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

/// Shadow layers translate the whole buffer; using the same `TextBounds` as the fill
/// clips soft samples. Expand the node clip by outset and intersect the viewport.
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

/// CSS `text-shadow` is active when a color is set and either blur or offset is non-zero
/// (`blur=0` is a sharp offset shadow — gallery「横向偏移」).
#[inline]
fn text_shadow_active(shadow_color: Option<[u8; 4]>, offset: [f32; 2], blur: f32) -> bool {
    shadow_color.is_some_and(|c| c[3] > 0)
        && (blur > 0.0 || offset[0].abs() > 0.0 || offset[1].abs() > 0.0)
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

fn emoji_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUI_EMOJI_DEBUG")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn emoji_custom_glyph_enabled() -> bool {
    emoji_layout::custom_glyph_layout_enabled()
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
    if emoji_custom_glyph_enabled()
        && super::emoji_config::emoji_bitmap_override_active()
        && emoji_svg_enabled()
    {
        "color-svg"
    } else if super::emoji_config::emoji_bitmap_override_active() {
        "color-twemoji"
    } else if emoji_custom_glyph_enabled() {
        // Slots + system/user color-font rasters (optically centered).
        "color-font-slot"
    } else {
        "font"
    }
}

fn textarea_diag_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUI_TEXTAREA_DIAG")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
    })
}

fn apply_letter_spacing_approx<'a>(text: &'a str, spacing_px: f32, font_size: f32) -> Cow<'a, str> {
    if spacing_px.abs() < 0.5 || text.is_empty() {
        return Cow::Borrowed(text);
    }
    let hair_w = (font_size * 0.05).max(0.5);
    let n = (spacing_px / hair_w).round().max(0.0) as usize;
    if n == 0 {
        return Cow::Borrowed(text);
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
    Cow::Owned(out)
}

/// Content-only key for Twemoji node cache (stable across style-only shape_hash changes).
fn emoji_sequence_content_hash(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Match `EmojiCacheKey` docs: coarse wrap buckets so 1px soft-width jitter does not miss.
fn emoji_wrap_width_q(buffer_soft_w: f32) -> u16 {
    (buffer_soft_w / 8.0).floor().clamp(0.0, u16::MAX as f32) as u16
}

fn direction_mark(dir: TextDirection) -> &'static str {
    match dir {
        TextDirection::Ltr => "\u{200E}",
        TextDirection::Rtl => "\u{200F}",
        TextDirection::Auto => "",
    }
}

fn resolve_text_align(
    align: crate::style::TextAlign,
    dir: TextDirection,
) -> glyphon::cosmic_text::Align {
    align.to_glyphon(dir)
}

/// Shape text for the UI FontSystem.
///
/// When custom emoji glyphs are enabled, each emoji grapheme is replaced with a
/// fixed-width non-whitespace slot so layout advance = [`emoji_layout::EMOJI_ADVANCE_EM`]×fs
/// while the color quad ink = [`emoji_layout::EMOJI_INK_EM`]×fs (positive bearing). Painting
/// real emoji codepoints (even with alpha=0) is wrong for two reasons:
/// 1) ZWJ/RI clusters expand to multiple Mask advances → holes beside the ink quad
/// 2) glyphon ignores alpha on CBDT Color glyphs if a color-emoji face is present
///
/// Slot must not be Unicode whitespace: cosmic-text strips leading spaces after soft-wrap,
/// which dropped the first emoji on each wrapped line (gallery stair-step overflow).
fn set_shaped_buffer_text(
    buffer: &mut glyphon::Buffer,
    _font_system: &mut glyphon::FontSystem,
    text: &str,
    attrs: glyphon::Attrs,
    shaping: glyphon::Shaping,
    hide_emoji_font_glyphs: bool,
    font_size: f32,
    line_height: f32,
) {
    if !hide_emoji_font_glyphs {
        buffer.set_text(text, &attrs, shaping, None);
        return;
    }
    // UAX#14 fit.shape_text may already contain EMOJI_LAYOUT_SLOT (`一`). That string has
    // no emoji codepoints, so text_contains_emoji is false — still must paint slots
    // invisible or the ideograph stroke shows as a line through Twemoji quads.
    if text.contains(EMOJI_LAYOUT_SLOT) {
        emoji_layout::set_buffer_with_emoji_slots(
            buffer,
            text,
            &attrs,
            shaping,
            font_size,
            line_height,
        );
        return;
    }
    if !emoji_detect::text_contains_emoji(text) {
        buffer.set_text(text, &attrs, shaping, None);
        return;
    }
    let (layout, _segments) = plan_emoji_layout(text);
    emoji_layout::set_buffer_with_emoji_slots(
        buffer,
        &layout,
        &attrs,
        shaping,
        font_size,
        line_height,
    );
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

fn twemoji_dir() -> Option<PathBuf> {
    super::emoji_config::twemoji_asset_dir()
}

fn twemoji_filename(sequence: &str) -> String {
    format!("{}.png", twemoji_name::twemoji_stem(sequence))
}

fn decode_png_rgba(bytes: &[u8]) -> Option<super::text::EmojiRaster> {
    let d = crate::media::decode_bytes(bytes, crate::media::DecodeOptions::image())?;
    Some(super::text::EmojiRaster::new(
        d.width as u16,
        d.height as u16,
        d.rgba,
    ))
}

fn trim_rgba_to_ink(src: &super::text::EmojiRaster) -> super::text::EmojiRaster {
    let w = src.width as usize;
    let h = src.height as usize;
    if w == 0 || h == 0 {
        return src.clone();
    }
    let mut minx = w;
    let mut miny = h;
    let mut maxx = 0usize;
    let mut maxy = 0usize;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if src.rgba[(y * w + x) * 4 + 3] > 16 {
                any = true;
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x);
                maxy = maxy.max(y);
            }
        }
    }
    if !any {
        return src.clone();
    }
    let nw = (maxx + 1 - minx).max(1);
    let nh = (maxy + 1 - miny).max(1);
    if nw == w && nh == h {
        return src.clone();
    }
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let src_row = ((miny + y) * w + minx) * 4;
        let dst_row = y * nw * 4;
        out[dst_row..dst_row + nw * 4].copy_from_slice(&src.rgba[src_row..src_row + nw * 4]);
    }
    super::text::EmojiRaster::new(nw as u16, nh as u16, out)
}

fn sample_rgba_bilinear(src: &super::text::EmojiRaster, x: f32, y: f32) -> [u8; 4] {
    let sw = src.width.max(1) as usize;
    let sh = src.height.max(1) as usize;
    let x = x.clamp(0.0, (sw - 1) as f32);
    let y = y.clamp(0.0, (sh - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(sw - 1);
    let y1 = (y0 + 1).min(sh - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let pix = |px: usize, py: usize| {
        let i = (py * sw + px) * 4;
        [
            src.rgba[i] as f32,
            src.rgba[i + 1] as f32,
            src.rgba[i + 2] as f32,
            src.rgba[i + 3] as f32,
        ]
    };
    let p00 = pix(x0, y0);
    let p10 = pix(x1, y0);
    let p01 = pix(x0, y1);
    let p11 = pix(x1, y1);
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bot = p01[c] * (1.0 - fx) + p11[c] * fx;
        out[c] = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn resize_rgba_nn(src: &super::text::EmojiRaster, dst_size: u16) -> super::text::EmojiRaster {
    let dst = dst_size.max(1) as usize;
    let sw = src.width.max(1) as usize;
    let sh = src.height.max(1) as usize;
    // Letterbox into a square — never stretch. Stretching a taller COLR ink-box into
    // an em-square was crushing faces into vertical ovals.
    //
    // Keep a 1px transparent inset so ink never sits on the texture edge. Tilted
    // glyphs (🤣) and circular faces otherwise put tears/chords on the last row;
    // glyphon TextBounds snap then shaves them flat.
    let inset = if dst >= 4 { 1 } else { 0 };
    let inner = dst.saturating_sub(2 * inset).max(1);
    if sw == inner && sh == inner && inset == 0 {
        return src.clone();
    }
    let mut out = vec![0u8; dst * dst * 4];
    let scale = (inner as f32 / sw as f32).min(inner as f32 / sh as f32);
    let dw = (sw as f32 * scale).round().max(1.0) as usize;
    let dh = (sh as f32 * scale).round().max(1.0) as usize;
    let dw = dw.min(inner);
    let dh = dh.min(inner);
    let ox = (dst - dw) / 2;
    let oy = (dst - dh) / 2;
    for y in 0..dh {
        let sy = (y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5;
        for x in 0..dw {
            let sx = (x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5;
            let px = sample_rgba_bilinear(src, sx, sy);
            let di = ((oy + y) * dst + (ox + x)) * 4;
            out[di..di + 4].copy_from_slice(&px);
        }
    }
    super::text::EmojiRaster::new(dst as u16, dst as u16, out)
}

/// Fit a fallback/Twemoji raster into the ink square used by custom glyphs.
pub(crate) fn fit_emoji_to_pixel_square(
    src: &super::text::EmojiRaster,
    pixel_size: u16,
) -> super::text::EmojiRaster {
    // Trim transparent pad first, then scale ink bbox → INK_EM (fills without empty pad).
    let trimmed = trim_rgba_to_ink(src);
    resize_rgba_nn(&trimmed, pixel_size)
}

/// Paint / raster edge for custom emoji quads = round([`emoji_layout::EMOJI_INK_EM`]×fs).
/// Must equal glyphon's `RasterizeCustomGlyphRequest` width/height for the stored bitmap.
pub(crate) fn emoji_paint_pixel_size(scaled_text_size: f32) -> u16 {
    emoji_layout::emoji_ink_pixel_size(scaled_text_size)
}

pub(crate) fn load_twemoji_raster(
    sequence: &str,
    pixel_size: u16,
) -> Option<super::text::EmojiRaster> {
    let base = twemoji_dir()?;
    let mut path = base.join(twemoji_filename(sequence));
    if !Path::new(&path).exists() {
        let stripped: String = sequence.chars().filter(|&c| c as u32 != 0xFE0F).collect();
        if stripped != sequence {
            path = base.join(twemoji_filename(&stripped));
        }
    }
    if !Path::new(&path).exists() {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let raw = decode_png_rgba(&bytes)?;
    Some(fit_emoji_to_pixel_square(&raw, pixel_size.max(1)))
}

/// Isolated FontSystem for color-emoji fallback rasters.
///
/// Must **not** load Segoe UI Emoji / Noto Color Emoji into the UI `FontSystem`:
/// glyphon's fragment shader ignores text color for `SwashContent::Color`, so
/// `Attrs.color(alpha=0)` cannot hide CBDT glyphs. Once those faces are in the
/// main DB, cosmic-text falls back into them and ZWJ / RI clusters leak as
/// fragmented color ink under Twemoji custom glyphs (gallery: family/flags).
fn emoji_fallback_engines() -> MutexGuard<'static, (glyphon::FontSystem, glyphon::SwashCache)> {
    static CELL: OnceLock<Mutex<(glyphon::FontSystem, glyphon::SwashCache)>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_emoji_font_fallback(&mut fs);
        Mutex::new((fs, glyphon::SwashCache::new()))
    })
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

struct FallbackGlyphBlit {
    x0: i32,
    y0: i32,
    w: u32,
    h: u32,
    color: bool,
    data: Vec<u8>,
}

fn blit_premultiplied_or_mask(dst: &mut [u8], dst_w: i32, dst_h: i32, blit: &FallbackGlyphBlit) {
    for row in 0..blit.h as i32 {
        for col in 0..blit.w as i32 {
            let x = blit.x0 + col;
            let y = blit.y0 + row;
            if x < 0 || y < 0 || x >= dst_w || y >= dst_h {
                continue;
            }
            let di = ((y * dst_w + x) * 4) as usize;
            let (sr, sg, sb, sa) = if blit.color {
                let si = ((row * blit.w as i32 + col) * 4) as usize;
                (
                    blit.data[si],
                    blit.data[si + 1],
                    blit.data[si + 2],
                    blit.data[si + 3],
                )
            } else {
                let a = blit.data[(row * blit.w as i32 + col) as usize];
                (255, 255, 255, a)
            };
            if sa == 0 {
                continue;
            }
            // Source-over (non-premultiplied).
            let da = dst[di + 3] as u32;
            let out_a = sa as u32 + ((255 - sa as u32) * da) / 255;
            if out_a == 0 {
                continue;
            }
            let inv = 255 - sa as u32;
            dst[di] = (((sr as u32 * sa as u32) + (dst[di] as u32 * inv * da) / 255) / out_a) as u8;
            dst[di + 1] =
                (((sg as u32 * sa as u32) + (dst[di + 1] as u32 * inv * da) / 255) / out_a) as u8;
            dst[di + 2] =
                (((sb as u32 * sa as u32) + (dst[di + 2] as u32 * inv * da) / 255) / out_a) as u8;
            dst[di + 3] = out_a as u8;
        }
    }
}

fn blit_mask_rgba(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    mask: &[u8],
    mask_w: u32,
    mask_h: u32,
    dx: i32,
    dy: i32,
    color: [u8; 4],
) {
    if mask_w == 0 || mask_h == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    let color_a = color[3] as u32;
    for row in 0..mask_h as i32 {
        let y = dy + row;
        if y < 0 || y >= dst_h as i32 {
            continue;
        }
        let src_row = &mask[(row as u32 * mask_w) as usize..];
        for col in 0..mask_w as i32 {
            let x = dx + col;
            if x < 0 || x >= dst_w as i32 {
                continue;
            }
            let a = (src_row[col as usize] as u32 * color_a) >> 8;
            if a == 0 {
                continue;
            }
            let di = ((y as u32 * dst_w + x as u32) * 4) as usize;
            if a >= 255 {
                dst[di] = color[0];
                dst[di + 1] = color[1];
                dst[di + 2] = color[2];
                dst[di + 3] = 255;
            } else {
                let inv = 255 - a;
                for i in 0..3 {
                    let d = dst[di + i] as u32;
                    dst[di + i] = ((inv * d + a * color[i] as u32) >> 8) as u8;
                }
                let d = dst[di + 3] as u32;
                dst[di + 3] = ((inv * d + a * 255) >> 8) as u8;
            }
        }
    }
}

/// COLR/CPAL raster matching swash `ColorOutline`, except `base_y` uses `floor`
/// instead of `ceil` so circular Segoe faces keep their bottom curve.
fn rasterize_colr_glyph_unclipped(
    font: swash::FontRef<'_>,
    glyph_id: u16,
    size: f32,
) -> Option<(i32, i32, u32, u32, Vec<u8>)> {
    use swash::scale::ScaleContext;
    use swash::zeno::{Mask, Origin, Scratch};

    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(size).hint(true).build();
    if !scaler.has_color_outlines() {
        return None;
    }
    let outline = scaler.scale_color_outline(glyph_id)?;
    if outline.is_empty() {
        return None;
    }
    let bounds = outline.bounds();
    // Mirror swash::scale::Render ColorOutline placement, with:
    //   base_y = floor(min.y)  (swash uses ceil → crops face bottoms)
    //   +1px pad so layer AA / tilted tears are not clipped by the canvas edge
    const PAD: i32 = 1;
    let base_x = bounds.min.x.floor() as i32 - PAD;
    let base_y = bounds.min.y.floor() as i32 - PAD;
    let max_x = bounds.max.x.ceil() as i32 + PAD;
    let max_y = bounds.max.y.ceil() as i32 + PAD;
    let base_w = (max_x - base_x).max(1) as u32;
    let base_h = (max_y - base_y).max(1) as u32;
    if base_w > 512 || base_h > 512 {
        return None;
    }

    let mut rgba = vec![0u8; (base_w * base_h * 4) as usize];
    let palette = font.color_palettes().next();
    let foreground = [128u8, 128, 128, 255];
    let mut scratch = Vec::new();
    let mut rcx = Scratch::new();

    for i in 0..outline.len() {
        let Some(layer) = outline.get(i) else {
            return None;
        };
        scratch.clear();
        let placement = Mask::with_scratch(layer.path(), &mut rcx)
            .origin(Origin::BottomLeft)
            .inspect(|fmt, w, h| {
                scratch.resize(fmt.buffer_size(w, h), 0);
            })
            .render_into(&mut scratch[..], None);
        let color = layer
            .color_index()
            .and_then(|idx| palette.map(|p| p.get(idx)))
            .unwrap_or(foreground);
        let dx = placement.left.wrapping_sub(base_x);
        let dy = (base_h as i32 + base_y).wrapping_sub(placement.top);
        blit_mask_rgba(
            &mut rgba,
            base_w,
            base_h,
            &scratch,
            placement.width,
            placement.height,
            dx,
            dy,
            color,
        );
    }

    let placement_left = base_x;
    let placement_top = base_h as i32 + base_y;
    Some((placement_left, placement_top, base_w, base_h, rgba))
}

/// Rasterize an emoji sequence with the isolated color-emoji face.
///
/// Composites **all** usable glyphs (not just the first Color). Segoe often
/// emits multiple CBDT glyphs for ZWJ families; flags may only yield Mask.
pub(crate) fn load_font_fallback_raster(
    sequence: &str,
    pixel_size: u16,
    font_weight: u16,
) -> Option<super::text::EmojiRaster> {
    let mut engines = emoji_fallback_engines();
    let (font_system, swash_cache) = &mut *engines;
    let emoji_family = crate::utils::font::resolve_family(
        font_system.db(),
        "Segoe UI Emoji, Apple Color Emoji, Noto Color Emoji, emoji, sans-serif",
    );
    let size = pixel_size.max(1) as f32;
    // Wide enough for ZWJ strips (family) and RI pairs.
    let canvas_w = (size * 6.0).max(size * 2.0);
    let canvas_h = (size * 2.0).max(size);
    let mut buffer = glyphon::Buffer::new(font_system, glyphon::Metrics::new(size, size * 1.2));
    buffer.set_size(Some(canvas_w), Some(canvas_h));
    buffer.set_text(
        sequence,
        &glyphon::Attrs::new()
            .family(emoji_family)
            .weight(glyphon::fontdb::Weight(font_weight)),
        glyphon::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);

    let mut blits: Vec<FallbackGlyphBlit> = Vec::new();
    for run in buffer.layout_runs() {
        for g in run.glyphs.iter() {
            let physical = g.physical((0.0, 0.0), 1.0);
            // Prefer our COLR path (avoids swash ColorOutline bottom crop).
            let font_arc = font_system.get_font(g.font_id, g.font_weight);
            let colr = font_arc
                .as_ref()
                .and_then(|f| rasterize_colr_glyph_unclipped(f.as_swash(), g.glyph_id, size));
            if let Some((left, top, w, h, data)) = colr {
                if w > 0 && h > 0 {
                    blits.push(FallbackGlyphBlit {
                        x0: physical.x + left,
                        y0: physical.y - top,
                        w,
                        h,
                        color: true,
                        data,
                    });
                    continue;
                }
            }
            let Some(image) = swash_cache.get_image_uncached(font_system, physical.cache_key)
            else {
                continue;
            };
            let w = image.placement.width;
            let h = image.placement.height;
            if w == 0 || h == 0 {
                continue;
            }
            let color = matches!(image.content, glyphon::SwashContent::Color);
            let is_mask = matches!(image.content, glyphon::SwashContent::Mask);
            if !color && !is_mask {
                continue;
            }
            let x0 = physical.x + image.placement.left;
            let y0 = physical.y - image.placement.top;
            blits.push(FallbackGlyphBlit {
                x0,
                y0,
                w,
                h,
                color,
                data: image.data,
            });
        }
    }
    if blits.is_empty() {
        return None;
    }
    // Prefer Color layers when any exist (drop ZWJ outline crumbs). Else keep Masks
    // for non-flag sequences (rare). Regional-indicator flags on Windows are Mask-only
    // RI letter outlines via swash — painting them looks like faint gray "CN"/"US".
    let has_color = blits.iter().any(|b| b.color);
    if has_color {
        blits.retain(|b| b.color);
    } else if emoji_detect::grapheme_is_regional_flag(sequence) {
        return None;
    } else {
        blits.retain(|b| b.w > 1 && b.h > 1);
    }
    if blits.is_empty() {
        return None;
    }

    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;
    for b in &blits {
        min_x = min_x.min(b.x0);
        min_y = min_y.min(b.y0);
        max_x = max_x.max(b.x0 + b.w as i32);
        max_y = max_y.max(b.y0 + b.h as i32);
    }
    let out_w = (max_x - min_x).clamp(1, 512);
    let out_h = (max_y - min_y).clamp(1, 512);
    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];
    for b in &blits {
        let shifted = FallbackGlyphBlit {
            x0: b.x0 - min_x,
            y0: b.y0 - min_y,
            w: b.w,
            h: b.h,
            color: b.color,
            data: b.data.clone(),
        };
        blit_premultiplied_or_mask(&mut rgba, out_w, out_h, &shifted);
    }
    Some(super::text::EmojiRaster::new(
        out_w as u16,
        out_h as u16,
        rgba,
    ))
}

fn load_emoji_raster_chain(
    text_renderer: &mut RuiTextRenderer,
    sequence: &str,
    pixel_size: u16,
    _family: glyphon::Family,
    font_weight: u16,
) -> Option<super::text::EmojiRaster> {
    // Bitmap override only: Twemoji PNG (when dir configured / CUSTOM_GLYPH=1).
    if super::emoji_config::emoji_bitmap_override_active() {
        if let Some(png) = load_twemoji_raster(sequence, pixel_size) {
            return Some(png);
        }

        // Optional SVG (feature + env); skip when disabled.
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
    }

    // System/user color-emoji on an *isolated* FontSystem — never open seguimj on the UI FS
    // while slot layout is on (CBDT leak under alpha=0 slots). Always used as the default
    // raster path; Twemoji/SVG only when bitmap override is active (above).
    let oversample = pixel_size.saturating_mul(2).clamp(pixel_size, 128);
    load_font_fallback_raster(sequence, oversample, font_weight)
        .map(|r| fit_emoji_to_pixel_square(&r, pixel_size.max(1)))
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

/// CSS `text-underline-offset`：正值让下划线离文字更远（屏幕 Y 向下增大）。
#[inline]
fn underline_line_y(
    box_y: f32,
    box_h: f32,
    ul_h: f32,
    underline_offset: f32,
    ui_scale: f32,
    paint_scale: f32,
) -> f32 {
    box_y + box_h * paint_scale - (ul_h * 0.45) + (underline_offset * ui_scale * paint_scale)
}

/// 文本盒 scissor 常贴着 layout bounds；正 `underline-offset` 会画到盒外。
/// 下/上各扩一点，避免 Breadcrumb / inline link hover 下划线被裁没。
#[inline]
fn expand_clip_for_text_decoration(
    clip: Option<[f32; 4]>,
    underline: bool,
    overline: bool,
    offset: f32,
    ul_h: f32,
    ui_scale: f32,
    paint_scale: f32,
) -> Option<[f32; 4]> {
    let Some(c) = clip else {
        return None;
    };
    let grow = (offset.max(0.0) * ui_scale * paint_scale) + ul_h + 1.0;
    let mut y = c[1];
    let mut h = c[3];
    if underline {
        h += grow;
    }
    if overline {
        y -= grow;
        h += grow;
    }
    Some([c[0], y, c[2], h])
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
            culled_skip_prepare: false,
        };
    }

    crate::text::fit_cache_frame_tick();

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
        for entry in text_renderer.shaped_cache.drain(text_nodes.len()..) {
            park_shaped_entry(&mut text_renderer.shaped_identity_pool, entry);
        }
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
        // Input/Textarea: leaf box can match the scrollport while the buffer holds more
        // hard lines. After scroll_offset moves `y` up, a short node_rect can miss the
        // clip entirely and skip shaping — looks like "clip locked to textarea height".
        let is_input_text_early =
            super::text_prepare_debug::is_input_inline_text(info.template_id.as_deref());
        let visibility_h = if is_input_text_early {
            let hard = info.text.split('\n').count().max(1) as f32;
            let lh = st
                .line_height_px
                .unwrap_or_else(|| {
                    let scale = st
                        .line_height_scale
                        .unwrap_or_else(|| tailwind_default_leading_scale(st.size));
                    st.size * scale
                })
                .max(1.0)
                * ui_scale;
            scaled_h
                .max(hard * lh)
                .max(scaled_clip.map(|c| c[3]).unwrap_or(0.0))
        } else {
            scaled_h
        };
        // `info.x` is already `render_x - scroll_x`. Culling against the shifted origin
        // + viewport-sized leaf misses the clip after a caret pan (selection still paints
        // → empty gray block). Restore the layout origin for visibility only.
        let cull_x = x + st.scroll_x.max(0.0) * ui_scale;
        let node_rect = [cull_x, y, scaled_w.max(0.0), visibility_h.max(0.0)];
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
                    .push(make_dummy_cache_entry(unsafe {
                        &mut *text_renderer.font_system
                    }));
            }
            frame_meta.push(None);
            continue;
        }

        // Twemoji custom glyphs keep layout on UI sans (`best_font_family`) — same as the
        // default family path — so emoji detection need not run before the Exact key.
        // Defer `text_contains_emoji` until after cache lookup (scroll: Exact hits dominate).
        let family = if let Some(f) = &st.font_family {
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
        let mut line_height = if is_icon {
            scaled_size
        } else if let Some(line_height_px) = st.line_height_px {
            line_height_px * ui_scale
        } else {
            let scale = st
                .line_height_scale
                .unwrap_or_else(|| tailwind_default_leading_scale(st.size));
            scaled_size * scale
        };

        let is_input_text =
            super::text_prepare_debug::is_input_inline_text(info.template_id.as_deref());
        // Input/Textarea: never allow leading-none / cascade to crush Metrics below
        // leading-normal — that packs CJK into a flat strip while the caret (style lh)
        // still looks normal-height.
        if is_input_text {
            let min_lh = scaled_size * crate::utils::text_field::text_edit_default_leading_scale();
            line_height = line_height.max(min_lh);
        }
        // Input field text must keep a stable shaping mode across focus: caret install used to
        // flip ASCII placeholders (e.g. password "Password") from Basic → Advanced and tighten
        // glyph advances. Labels may still use Basic for scroll perf.
        let is_ascii_basic = !is_input_text && st.input_caret.is_none() && info.text.is_ascii();
        let shaping = match perf_mode {
            // Editing/caret keeps advanced shaping for cursor fidelity.
            TextPerfMode::HighQuality => {
                if (force_fast_mode && st.input_caret.is_none() && !is_input_text) || is_ascii_basic
                {
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
        let shaping = if force_fast_mode
            && st.input_caret.is_none()
            && info.text.is_ascii()
            && !is_input_text
        {
            glyphon::Shaping::Basic
        } else {
            shaping
        };
        // Soft-wrap width for glyphon. When layout attached a PreparedText, consult
        // `layout(&prepared, info.width)` for line-count / max-width hints and drive
        // buffer size from the same logical avail width. Full glyph/buffer sharing with
        // the layout-only FontSystem is still deferred (separate FontSystems).
        // `whitespace-nowrap`: shape against **live** intrinsic (or wider box), never
        // soft-wrap mid-line. Do not use a possibly-stale `prepared` width — IME preedit
        // updates `info.text` before layout re-prepares (textarea: line jumps down).
        // Ellipsis captions are already truncated to the box — using intrinsic here would
        // center a short "Very long…" run inside the full caption width (left-side gap).
        let nowrap = st.white_space == crate::style::WhiteSpace::Nowrap;
        let logical_lh = line_height / ui_scale.max(0.001);
        let layout_w = if nowrap {
            if scroll_fast_skips_input_intrinsic_prepare(
                force_fast_mode,
                is_input_text,
                st.input_caret.is_some(),
                st.scroll_x,
            ) {
                // Prefer layout-attached prepared (no new prepare). Else leaf width:
                // short placeholders == max(intrinsic, box); long idle fields clip at
                // the leaf while scrolling (overflow:hidden, scroll_x≈0).
                if let Some(p) = info.prepared.as_ref() {
                    nowrap_buffer_width(
                        st.overflow,
                        true,
                        crate::text::intrinsic_width(p),
                        info.width,
                    )
                } else {
                    info.width.max(1.0)
                }
            } else {
                nowrap_layout_width_from_text(
                    info.text.as_str(),
                    st.size,
                    font_weight,
                    logical_lh,
                    st.overflow,
                    st.input_caret.is_none(),
                    info.width,
                )
            }
        } else {
            // Absorb ≤1px shortfall vs an unbreakable segment so Word-wrap does not
            // orphan the last glyph when the leaf was sized to a fractional intrinsic.
            let mut w = info.width.max(1.0);
            if let Some(p) = info.prepared.as_ref() {
                let seg = p.max_segment_width;
                if seg > w && seg <= w + 1.0 {
                    w = seg;
                }
            }
            crate::text::snap_content_text_width(w).max(info.width.max(1.0))
        };
        let soft_w_target = (layout_w * ui_scale).max(1.0);
        let metrics = info
            .prepared
            .as_ref()
            .map(|p| crate::text::layout(p, layout_w));
        let hard_lines = info.text.split('\n').count().max(1) as f32;
        let ink_fudge = if is_input_text {
            crate::utils::text_field::text_edit_ink_pad_px(st.size) * ui_scale
        } else {
            2.0
        };
        // Height from segment arithmetic / leaf first. glyphon_fit only on Shape miss (below).
        let metrics_h = metrics
            .as_ref()
            .map(|m| {
                (m.line_count.max(1) as f32 * line_height)
                    .max(m.height * ui_scale)
                    .max(hard_lines * line_height)
                    .max(line_height)
            })
            .unwrap_or_else(|| (hard_lines * line_height.max(1.0)).max(line_height));
        let buffer_soft_h = if is_input_text {
            metrics_h + ink_fudge
        } else {
            metrics_h.max(scaled_h) + ink_fudge
        };
        // Unbounded height_opt for Input/Textarea: `Some(content soft_h)` still races with
        // shaped_cache index reuse / stale short windows. `None` keeps every hard line.
        let buffer_height_opt: Option<f32> = if is_input_text {
            None
        } else {
            Some(buffer_soft_h)
        };
        // CSS text-overflow: when the nowrap line is wider than the box, keep
        // inline-start in view (ellipsis/clip at inline-end). Center only applies
        // to lines that fit — otherwise a truncated caption sits in a huge gap.
        let source_intrinsic = info
            .prepared
            .as_ref()
            .map(crate::text::intrinsic_width)
            .unwrap_or(info.width);
        let align = if st.overflow.start_align_when_line_overflows()
            && nowrap
            && source_intrinsic > info.width + 0.5
        {
            match st.direction {
                TextDirection::Rtl => glyphon::cosmic_text::Align::Right,
                _ => glyphon::cosmic_text::Align::Left,
            }
        } else {
            resolve_text_align(st.align, st.direction)
        };
        let display_text: Cow<'_, str> =
            if st.overflow == TextOverflow::Ellipsis && st.input_caret.is_none() {
                Cow::Owned(truncate_with_ellipsis(
                    unsafe { &mut *text_renderer.font_system },
                    info.text.as_str(),
                    scaled_size,
                    line_height,
                    font_weight,
                    family,
                    scaled_w.max(0.0),
                ))
            } else if let Some(raw) = info.glyph_fx.as_deref() {
                let kind = crate::renderer::GlyphFxKind::parse(raw);
                if kind == crate::renderer::GlyphFxKind::Split {
                    let t = crate::style::global_time_s().fract();
                    let n = info.text.chars().count().max(1);
                    let keep = ((n as f32) * t).ceil() as usize;
                    Cow::Owned(info.text.chars().take(keep.max(1)).collect())
                } else {
                    Cow::Borrowed(info.text.as_str())
                }
            } else {
                Cow::Borrowed(info.text.as_str())
            };
        let with_suffix: Cow<'_, str> = match info.shape_suffix.as_deref() {
            Some(s) if !s.is_empty() => Cow::Owned(format!("{}{}", display_text, s)),
            _ => display_text,
        };
        let with_spacing =
            apply_letter_spacing_approx(with_suffix.as_ref(), st.letter_spacing, scaled_size);
        let mark = direction_mark(st.direction);
        // Keep `with_suffix` when spacing is a no-op borrow (zero alloc on scroll).
        let shape_text: Cow<'_, str> = if mark.is_empty() {
            match with_spacing {
                Cow::Borrowed(_) => with_suffix,
                Cow::Owned(s) => Cow::Owned(s),
            }
        } else {
            Cow::Owned(format!("{mark}{with_spacing}"))
        };
        let shape_hash = node_shape_identity_hash(
            shape_text.as_ref(),
            scaled_size,
            line_height,
            font_weight,
            family,
            align,
            shaping,
            st.letter_spacing,
            st.direction,
            st.color,
        );
        // Derive Exact key from identity — do not re-hash the full string (scroll frames
        // visit every visible label; browsers keep glyphs and only translate).
        let key_hash = node_shape_key_from_identity(shape_hash, scaled_w, scaled_h, false);

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
                .push(make_dummy_cache_entry(unsafe {
                    &mut *text_renderer.font_system
                }));
        }

        // Wrap: FIT_CACHE hit always. Fast miss → leaf (smooth resize). Settle miss →
        // one glyphon_fit (bidi/emoji ink must not paint past the leaf).
        // BiDi/WJ prewrap uses floor-8 buckets (≤ leaf) so Wrap::None paint cannot reflow.
        let fit_lookup_w = {
            if crate::text::needs_logical_prewrap(info.text.as_str()) {
                crate::text::layout_wrap_target_prewrap(layout_w)
            } else if force_fast_mode {
                crate::text::layout_wrap_target(layout_w, true)
            } else {
                layout_w
            }
        };
        let resolve_paint_wrap = || -> f32 {
            if nowrap || info.text.is_ascii() || info.prepared.is_none() {
                return soft_w_target;
            }
            // Prewrap hard-breaks are sized for the leaf; do not shrink buffer below leaf
            // (that reflows the last word on settle).
            if crate::text::needs_logical_prewrap(info.text.as_str()) {
                return soft_w_target;
            }
            if let Some(fit) = crate::text::try_glyphon_fit_soft_wrap(
                info.text.as_str(),
                st.size,
                font_weight,
                logical_lh,
                fit_lookup_w,
            ) {
                return (fit.wrap_width * ui_scale).max(1.0);
            }
            if force_fast_mode {
                return soft_w_target;
            }
            let fit = crate::text::glyphon_fit_soft_wrap(
                info.text.as_str(),
                st.size,
                font_weight,
                logical_lh,
                fit_lookup_w,
            );
            (fit.wrap_width * ui_scale).max(1.0)
        };

        enum ShapePath {
            Exact { measured: f32, wrap_w: f32 },
            Bounds { measured: f32, wrap_w: f32 },
            Miss,
        }

        // shaped_cache stays index-aligned. Band-cull reordering reclaims buffers from
        // `shaped_identity_pool` by shape_hash — never swap two live slots in the vec
        // (that painted button captions inside Inputs). Input stays out of bounds_only.
        // Try reclaim before Exact/Bounds so gallery forms survive sibling band cull.
        {
            let entry = &text_renderer.shaped_cache[i];
            if entry.shape_hash != shape_hash
                && shape_hash != 0
                && text_renderer.shaped_identity_pool.contains_key(&shape_hash)
            {
                let reclaimed = text_renderer
                    .shaped_identity_pool
                    .remove(&shape_hash)
                    .expect("contains_key");
                let displaced = std::mem::replace(&mut text_renderer.shaped_cache[i], reclaimed);
                park_shaped_entry(&mut text_renderer.shaped_identity_pool, displaced);
            }
        }

        // Always detect (CJK/Arabic fast-reject is cheap). Caching has_emoji on Exact was
        // unsafe: a first-frame false negative left buffers without emoji slots while later
        // Exact frames skipped detect → blank/flickering Twemoji quads (gallery text page).
        let has_emoji = emoji_detect::text_contains_emoji(info.text.as_str())
            || info
                .shape_suffix
                .as_deref()
                .is_some_and(|s| !s.is_empty() && emoji_detect::text_contains_emoji(s));
        text_renderer.shaped_cache[i].has_emoji = has_emoji;

        // Skip script coverage when this node will paint from an Exact buffer without reshape
        // (gallery text page scroll: many multilingual labels per frame).
        let skip_coverage = {
            let entry = &text_renderer.shaped_cache[i];
            let exact_peek = entry.key_hash == key_hash && entry.shape_hash == shape_hash;
            if !exact_peek {
                false
            } else if is_input_text {
                scroll_fast_skips_input_reshape(force_fast_mode, true, st.input_caret.is_some())
            } else if force_fast_mode {
                nowrap
                    || !crate::text::needs_logical_prewrap(info.text.as_str())
                    || bidi_prewrap_applied(
                        &entry.buffer,
                        info.text.as_str(),
                        st.size,
                        font_weight,
                        logical_lh,
                        layout_w,
                    )
            } else {
                let runs = entry.buffer.layout_runs().count().max(1);
                let height_ok = (runs as f32 * line_height) <= scaled_h + 1.0;
                height_ok && entry.measured_text_w > 0.0
            }
        };
        if !skip_coverage {
            let shape_text_for_coverage: Cow<'_, str> = match info.shape_suffix.as_deref() {
                Some(s) if !s.is_empty() => Cow::Owned(format!("{}{}", info.text, s)),
                _ => Cow::Borrowed(info.text.as_str()),
            };
            font_loader::ensure_coverage_for_text(
                unsafe { &mut *text_renderer.font_system },
                shape_text_for_coverage.as_ref(),
            );
            if has_emoji && emoji_custom_glyph_enabled() {
                font_loader::ensure_coverage_for_text(
                    unsafe { &mut *text_renderer.font_system },
                    emoji_layout::EMOJI_LAYOUT_SLOT.encode_utf8(&mut [0; 4]),
                );
            }
        }

        let path = {
            let entry = &mut text_renderer.shaped_cache[i];
            let exact_hit = entry.key_hash == key_hash && entry.shape_hash == shape_hash;
            let bounds_only = !exact_hit
                && entry.shape_hash == shape_hash
                && entry.shape_hash != 0
                && !is_input_text;

            if exact_hit {
                let mut ok = true;
                if is_input_text {
                    let hard = info.text.split('\n').count().max(1);
                    let runs = entry.buffer.layout_runs().count();
                    let finite_height = entry.buffer.size().1.is_some();
                    if runs < hard || finite_height {
                        ok = false;
                    }
                }
                if ok {
                    if is_input_text {
                        // Page-scroll fast path: buffer text/metrics unchanged — skip
                        // `shape_until_scroll` (gallery forms: many idle Inputs per frame).
                        // Editing / settle frames still reshape for caret + hard-line fidelity.
                        if scroll_fast_skips_input_reshape(
                            force_fast_mode,
                            true,
                            st.input_caret.is_some(),
                        ) {
                            let cur_wrap = entry.buffer.size().0.unwrap_or(soft_w_target);
                            ShapePath::Exact {
                                measured: entry.measured_text_w,
                                wrap_w: cur_wrap,
                            }
                        } else {
                            let mut wrap_w = resolve_paint_wrap();
                            if nowrap {
                                entry.buffer.set_wrap(glyphon::Wrap::None);
                            }
                            entry.buffer.set_size(Some(wrap_w), buffer_height_opt);
                            entry
                                .buffer
                                .set_scroll(glyphon::cosmic_text::Scroll::default());
                            entry.buffer.shape_until_scroll(
                                unsafe { &mut *text_renderer.font_system },
                                false,
                            );
                            wrap_w = correct_bidi_ink_spill(
                                &mut entry.buffer,
                                unsafe { &mut *text_renderer.font_system },
                                info.text.as_str(),
                                st.size,
                                font_weight,
                                logical_lh,
                                fit_lookup_w,
                                soft_w_target,
                                ui_scale,
                                line_height,
                                scaled_h,
                                buffer_height_opt,
                                wrap_w,
                                nowrap,
                                force_fast_mode,
                            );
                            entry.measured_text_w = entry
                                .buffer
                                .layout_runs()
                                .map(|run| run.line_w)
                                .fold(0.0_f32, f32::max);
                            ShapePath::Exact {
                                measured: entry.measured_text_w,
                                wrap_w,
                            }
                        }
                    } else if !nowrap
                        && crate::text::needs_logical_prewrap(info.text.as_str())
                        && !bidi_prewrap_applied(
                            &entry.buffer,
                            info.text.as_str(),
                            st.size,
                            font_weight,
                            logical_lh,
                            layout_w,
                        )
                    {
                        // Stale Exact entry shaped raw BiDi (fast WordOrGlyph) — must re-Miss.
                        ShapePath::Miss
                    } else {
                        // Exact hit (labels): reuse buffer when ink already fits the leaf.
                        // Avoid `resolve_paint_wrap` / glyphon_fit every settle frame — that
                        // dominated gallery text-page scroll (CJK/Arabic/emoji labels).
                        let cur_wrap = entry.buffer.size().0.unwrap_or(soft_w_target);
                        let runs = entry.buffer.layout_runs().count().max(1);
                        let height_ok = (runs as f32 * line_height) <= scaled_h + 1.0;
                        if force_fast_mode || (height_ok && entry.measured_text_w > 0.0) {
                            ShapePath::Exact {
                                measured: entry.measured_text_w,
                                wrap_w: cur_wrap,
                            }
                        } else {
                            let mut wrap_w = resolve_paint_wrap();
                            let need_reshape = (wrap_w - cur_wrap).abs() > 0.5
                                || (runs as f32 * line_height) > scaled_h + 1.0;
                            if need_reshape {
                                entry.buffer.set_size(Some(wrap_w), buffer_height_opt);
                                entry.buffer.shape_until_scroll(
                                    unsafe { &mut *text_renderer.font_system },
                                    false,
                                );
                            }
                            wrap_w = correct_bidi_ink_spill(
                                &mut entry.buffer,
                                unsafe { &mut *text_renderer.font_system },
                                info.text.as_str(),
                                st.size,
                                font_weight,
                                logical_lh,
                                fit_lookup_w,
                                soft_w_target,
                                ui_scale,
                                line_height,
                                scaled_h,
                                buffer_height_opt,
                                wrap_w,
                                nowrap,
                                force_fast_mode,
                            );
                            entry.measured_text_w = entry
                                .buffer
                                .layout_runs()
                                .map(|run| run.line_w)
                                .fold(0.0_f32, f32::max);
                            ShapePath::Exact {
                                measured: entry.measured_text_w,
                                wrap_w,
                            }
                        }
                    }
                } else {
                    ShapePath::Miss
                }
            } else if bounds_only {
                // iced Difference::Bounds: same Buffer, set_size only (no set_text).
                // BiDi/WJ prewrap is width-dependent → re-Miss when wrap target moves OR
                // when the buffer still has raw text from a fast WordOrGlyph Miss.
                let logical_prewrap = !nowrap
                    && !is_input_text
                    && crate::text::needs_logical_prewrap(info.text.as_str());
                let cur_wrap = entry.buffer.size().0.unwrap_or(soft_w_target);
                let measured = entry.measured_text_w;
                let runs = entry.buffer.layout_runs().count().max(1);
                let height_ok = (runs as f32 * line_height) <= scaled_h + 1.0;
                let want_wrap = resolve_paint_wrap();
                let wrap_matches_leaf = (cur_wrap - want_wrap).abs() <= 1.0;
                let prewrap_ok = !logical_prewrap
                    || bidi_prewrap_applied(
                        &entry.buffer,
                        info.text.as_str(),
                        st.size,
                        font_weight,
                        logical_lh,
                        layout_w,
                    );
                if logical_prewrap && (!wrap_matches_leaf || !prewrap_ok) {
                    ShapePath::Miss
                } else if measured > 0.0
                    && measured <= soft_w_target + 0.5
                    && height_ok
                    && wrap_matches_leaf
                {
                    entry.key_hash = key_hash;
                    entry.shape_hash = shape_hash;
                    ShapePath::Bounds {
                        measured,
                        wrap_w: cur_wrap,
                    }
                } else if logical_prewrap {
                    // Width still matches but ink/height drifted — spill path can set_text.
                    let mut wrap_w = want_wrap;
                    entry.buffer.set_size(Some(wrap_w), buffer_height_opt);
                    entry
                        .buffer
                        .shape_until_scroll(unsafe { &mut *text_renderer.font_system }, false);
                    wrap_w = correct_bidi_ink_spill(
                        &mut entry.buffer,
                        unsafe { &mut *text_renderer.font_system },
                        info.text.as_str(),
                        st.size,
                        font_weight,
                        logical_lh,
                        fit_lookup_w,
                        soft_w_target,
                        ui_scale,
                        line_height,
                        scaled_h,
                        buffer_height_opt,
                        wrap_w,
                        nowrap,
                        force_fast_mode,
                    );
                    let measured = entry
                        .buffer
                        .layout_runs()
                        .map(|run| run.line_w)
                        .fold(0.0_f32, f32::max);
                    entry.key_hash = key_hash;
                    entry.shape_hash = shape_hash;
                    entry.measured_text_w = measured;
                    ShapePath::Bounds { measured, wrap_w }
                } else {
                    // Skip set_size only when wrap already matches the desired width. "ink fits
                    // wider leaf" alone is wrong on grow: old narrow wrap leaves empty space on
                    // the right (gallery Checkbox blurb after widening the window).
                    if measured > 0.0
                        && measured <= soft_w_target + 0.5
                        && height_ok
                        && wrap_matches_leaf
                    {
                        entry.key_hash = key_hash;
                        entry.shape_hash = shape_hash;
                        ShapePath::Bounds {
                            measured,
                            wrap_w: cur_wrap,
                        }
                    } else {
                        let mut wrap_w = want_wrap;
                        if nowrap {
                            entry.buffer.set_wrap(glyphon::Wrap::None);
                        }
                        entry.buffer.set_size(Some(wrap_w), buffer_height_opt);
                        entry
                            .buffer
                            .shape_until_scroll(unsafe { &mut *text_renderer.font_system }, false);
                        wrap_w = correct_bidi_ink_spill(
                            &mut entry.buffer,
                            unsafe { &mut *text_renderer.font_system },
                            info.text.as_str(),
                            st.size,
                            font_weight,
                            logical_lh,
                            fit_lookup_w,
                            soft_w_target,
                            ui_scale,
                            line_height,
                            scaled_h,
                            buffer_height_opt,
                            wrap_w,
                            nowrap,
                            force_fast_mode,
                        );
                        let measured = entry
                            .buffer
                            .layout_runs()
                            .map(|run| run.line_w)
                            .fold(0.0_f32, f32::max);
                        entry.key_hash = key_hash;
                        entry.shape_hash = shape_hash;
                        entry.measured_text_w = measured;
                        ShapePath::Bounds { measured, wrap_w }
                    }
                }
            } else {
                ShapePath::Miss
            }
        };

        let (measured_text_w, cache_hit, buffer_soft_w) = match path {
            ShapePath::Exact { measured, wrap_w } | ShapePath::Bounds { measured, wrap_w } => {
                (measured, true, wrap_w)
            }
            ShapePath::Miss => {
                _shape_misses += 1;
                let mut buffer_soft_w = resolve_paint_wrap();
                // Park displaced identity before overwrite (band-cull may need it next frame).
                let should_park = {
                    let entry = &text_renderer.shaped_cache[i];
                    entry.shape_hash != 0 && entry.shape_hash != shape_hash
                };
                if should_park {
                    let dummy = make_dummy_cache_entry(unsafe { &mut *text_renderer.font_system });
                    let displaced = std::mem::replace(&mut text_renderer.shaped_cache[i], dummy);
                    park_shaped_entry(&mut text_renderer.shaped_identity_pool, displaced);
                }
                let entry = &mut text_renderer.shaped_cache[i];
                let mut buffer = glyphon::Buffer::new(
                    unsafe { &mut *text_renderer.font_system },
                    glyphon::Metrics::new(scaled_size.max(1.0), line_height.max(1.0)),
                );
                let logical_prewrap = !nowrap
                    && !is_input_text
                    && crate::text::needs_logical_prewrap(info.text.as_str());
                // UAX#14 prewrap from FIT_CACHE. Use 8px width buckets so resize fast and
                // settle share the same breaks (avoids last-word jump on mouse-up).
                // Never pair Wrap::None with raw text.
                let prewrapped: Option<std::sync::Arc<str>> = if logical_prewrap {
                    let prewrap_target = crate::text::layout_wrap_target_prewrap(layout_w).max(1.0);
                    // Always UAX#14 prewrap — including force_fast. Skipping left raw text
                    // + WordOrGlyph in the cache; Bounds then reused it and split WJ.
                    let fit = crate::text::try_glyphon_fit_soft_wrap(
                        info.text.as_str(),
                        st.size,
                        font_weight,
                        logical_lh,
                        prewrap_target,
                    )
                    .unwrap_or_else(|| {
                        crate::text::glyphon_fit_soft_wrap(
                            info.text.as_str(),
                            st.size,
                            font_weight,
                            logical_lh,
                            prewrap_target,
                        )
                    });
                    buffer_soft_w = soft_w_target;
                    buffer.set_wrap(glyphon::Wrap::None);
                    Some(fit.shape_text)
                } else {
                    None
                };
                if nowrap {
                    buffer.set_wrap(glyphon::Wrap::None);
                }
                buffer.set_size(Some(buffer_soft_w), buffer_height_opt);
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
                            apply_letter_spacing_approx(&span.text, st.letter_spacing, scaled_size)
                                .into_owned();
                        if si == 0 {
                            let m = direction_mark(st.direction);
                            if !m.is_empty() {
                                body = format!("{m}{body}");
                            }
                        }
                        let mut a = attrs.clone();
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
                        if span.background.is_some()
                            || matches!(span.kind, TextSpanKind::Chip | TextSpanKind::Mention)
                        {
                            a = a.metadata(si + 1);
                        }
                        owned.push((body, a));
                    }
                    let rich: Vec<(&str, glyphon::Attrs)> =
                        owned.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
                    buffer.set_rich_text(rich, &attrs, shaping, None);
                } else {
                    let paint_text = prewrapped.as_deref().unwrap_or_else(|| shape_text.as_ref());
                    set_shaped_buffer_text(
                        &mut buffer,
                        unsafe { &mut *text_renderer.font_system },
                        paint_text,
                        attrs,
                        shaping,
                        emoji_custom_glyph_enabled() && has_emoji,
                        scaled_size,
                        line_height,
                    );
                }
                for line in buffer.lines.iter_mut() {
                    line.set_align(Some(align));
                }
                if is_input_text {
                    buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
                }
                buffer.shape_until_scroll(unsafe { &mut *text_renderer.font_system }, false);
                if is_input_text {
                    buffer.set_scroll(glyphon::cosmic_text::Scroll::default());
                }
                buffer_soft_w = correct_bidi_ink_spill(
                    &mut buffer,
                    unsafe { &mut *text_renderer.font_system },
                    info.text.as_str(),
                    st.size,
                    font_weight,
                    logical_lh,
                    fit_lookup_w,
                    soft_w_target,
                    ui_scale,
                    line_height,
                    scaled_h,
                    buffer_height_opt,
                    buffer_soft_w,
                    nowrap,
                    force_fast_mode,
                );
                let measured = buffer
                    .layout_runs()
                    .map(|run| run.line_w)
                    .fold(0.0_f32, f32::max);
                entry.key_hash = key_hash;
                entry.shape_hash = shape_hash;
                entry.buffer = buffer;
                entry.measured_text_w = measured;
                (measured, false, buffer_soft_w)
            }
        };
        let emoji_cache_key = if emoji_custom_glyph_enabled() && has_emoji {
            Some(super::text::EmojiCacheKey {
                // Content-only hash (not full shape_hash): stable across color/weight so
                // scroll Exact frames keep hitting the same Twemoji glyph entry.
                sequence_hash: emoji_sequence_content_hash(shape_text.as_ref()),
                pixel_size: emoji_paint_pixel_size(scaled_size),
                scale_bits: ui_scale.to_bits(),
                color_mode: 1,
                // Coarse quantize — 1px buffer_soft_w jitter must not miss every frame.
                wrap_width_q: emoji_wrap_width_q(buffer_soft_w),
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
                let sibling_ids: Vec<(u16, super::text::EmojiRaster)> = text_renderer
                    .emoji_cache
                    .find_wrap_sibling(&key)
                    .map(|e| {
                        e.glyphs
                            .iter()
                            .filter_map(|g| e.rasters.get(&g.id).map(|r| (g.id, r.clone())))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut sibling_iter = sibling_ids.into_iter();
                let mut next_custom_id = text_renderer.emoji_cache.next_custom_id;
                // Prefer the text actually in the Buffer (may be UAX#14-prewrapped + slotted).
                let slotted = buffer_joined_text(&text_renderer.shaped_cache[i].buffer);
                let (layout_text, segments) = if slotted.contains(EMOJI_LAYOUT_SLOT) {
                    let segs = emoji_segments_from_slotted_layout(shape_text.as_ref(), &slotted);
                    (slotted, segs)
                } else {
                    plan_emoji_layout(shape_text.as_ref())
                };
                let mut glyphs = Vec::new();
                let mut rasters = std::collections::HashMap::new();
                // Same-line backstop if a slot advance collapses; reset on soft-wrap.
                // NEG_INFINITY so the first glyph may sit slightly left of 0 when ink
                // overflows the slot equally on both sides (not packed to the left).
                let mut emoji_left_cursor = f32::NEG_INFINITY;
                let mut emoji_line_top: Option<f32> = None;
                for seg in segments {
                    let TextSegment::Emoji { text, start } = seg else {
                        continue;
                    };
                    let (ink, advance, _) = emoji_layout::emoji_metrics(scaled_size);
                    // Raster + CustomGlyph must share this pixel size (glyphon requests
                    // round(glyph.width * scale); mismatch → transparent placeholder).
                    let pixel_size = emoji_paint_pixel_size(scaled_size);
                    let ink_draw = (pixel_size as f32).min(ink);
                    let (left, top) = match emoji_layout::custom_emoji_left_centered_top(
                        &text_renderer.shaped_cache[i].buffer,
                        &layout_text,
                        start,
                        ink_draw,
                        ink_draw,
                    ) {
                        Some((l, t)) => (l, t + emoji_baseline_layout_tweak(scaled_size)),
                        None => {
                            let prefix = layout_text.get(..start).unwrap_or("");
                            let prefix_w = measure_prefix_width(
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
                            // Same bearing as the slot path — prefix-only left pasted ink
                            // to the start of the cell with blank on the right.
                            let left = prefix_w + (advance - ink_draw) * 0.5;
                            let top = emoji_baseline_offset_fallback(scaled_size);
                            (left, top)
                        }
                    };
                    let left = emoji_layout::advance_emoji_left_cursor(
                        left,
                        top,
                        advance,
                        &mut emoji_left_cursor,
                        &mut emoji_line_top,
                    );
                    let (glyph_id, raster) = if let Some((id, raster)) = sibling_iter.next() {
                        (id, Some(raster))
                    } else {
                        let glyph_id = next_custom_id;
                        next_custom_id = next_custom_id.wrapping_add(1).max(1);
                        let raster = load_emoji_raster_cached(
                            text_renderer,
                            &text,
                            pixel_size,
                            family,
                            font_weight,
                        );
                        (glyph_id, raster)
                    };
                    if let Some(raster) = raster {
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
                    } else {
                        log::warn!(
                            "emoji custom glyph: no raster for {text:?} (Twemoji miss + font fallback failed)"
                        );
                    }
                }
                if emoji_debug_enabled() {
                    log::debug!(
                        "emoji cache miss sequence_hash={:#x} glyphs={} uploads={} bytes={}",
                        key.sequence_hash,
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

        let hard_lines_n = info.text.split('\n').count().max(1);
        // Padded ContentBox chips (Tabs): leaf border is taller than one line. Cosmic lays out
        // from the top of the buffer — without a dy, labels sit in the top padding (and look
        // top-biased). Center a single nowrap line in the border box.
        let dy = if !is_input_text && nowrap && hard_lines_n == 1 && scaled_h > line_height + 1.0 {
            (scaled_h - line_height) * 0.5
        } else {
            0.0
        };
        let mut glyph_color = st.color;
        let mut glyph_dy = dy;
        if let Some(raw) = info.glyph_fx.as_deref() {
            let kind = crate::renderer::GlyphFxKind::parse(raw);
            if kind != crate::renderer::GlyphFxKind::None {
                let fx = crate::renderer::GlyphFx::new(kind, crate::style::global_time_s().fract());
                let n = info.text.chars().count().max(1);
                glyph_color[3] = ((glyph_color[3] as f32) * fx.opacity(0, n))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let (_, off_y) = fx.offset(0, n);
                glyph_dy += off_y * ui_scale;
            }
        }
        let sdx = st.shadow_offset[0] * ui_scale;
        let sdy = st.shadow_offset[1] * ui_scale;
        let sblur = st.shadow_blur * ui_scale;
        let has_shadow = text_shadow_active(st.shadow_color, [sdx, sdy], sblur);

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
            } else if let Some(bmp) = generate_shadow_bitmap(
                &text_renderer.shaped_cache[i].buffer,
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
            // Bitmap miss → glyphon multi-layer TextArea fallback below (Coverage path).
        }

        frame_meta.push(Some(FrameTextNodeMeta {
            x,
            y,
            box_w: scaled_w,
            box_h: scaled_h,
            dy: glyph_dy,
            text_size: scaled_size,
            paint_scale: if info.paint_scale.is_finite() && info.paint_scale > 0.0 {
                info.paint_scale
            } else {
                1.0
            },
            align,
            measured_text_w,
            underline: st.underline,
            overline: st.overline,
            line_through: st.line_through,
            decoration_style: st.decoration_style,
            decoration_color: st.decoration_color,
            decoration_thickness: st.decoration_thickness,
            underline_offset: st.underline_offset,
            color: glyph_color,
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
                    let ps = if tb.paint_scale.is_finite() && tb.paint_scale > 0.0 {
                        tb.paint_scale
                    } else {
                        1.0
                    };
                    let bx = tb.x + (min_x - pad) * ps;
                    let by = tb.y + line_top * ps;
                    let bw = ((max_x - min_x) + 2.0 * pad) * ps;
                    let bh = line_height.max(1.0) * ps;
                    let radius = (bh * 0.25).min(6.0 * ui_scale * ps);
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
                        transform: crate::style::transform::MAT2D_IDENTITY,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: 0.0,
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
            if !shadow_skip_indices.contains(&i) {
                // Texture-shadow miss: fall back to glyphon multi-layer TextAreas (Coverage).
                let radius = tb.shadow_blur;
                let alpha = sc[3];
                if alpha > 0 && text_shadow_active(Some(sc), [tb.shadow_dx, tb.shadow_dy], radius) {
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
                            scale: tb.paint_scale,
                            bounds,
                            default_color: glyphon::Color::rgba(sc[0], sc[1], sc[2], layer_alpha),
                            custom_glyphs: tb
                                .emoji_cache_key
                                .as_ref()
                                .map_or(&[], |k| text_renderer.emoji_cache.glyphs_for(k)),
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

                        let mut weight_sum = 1.0_f32;
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
        if let Some(info) = text_nodes.get(i) {
            if textarea_diag_enabled()
                && super::text_prepare_debug::is_input_inline_text(info.template_id.as_deref())
            {
                let runs: Vec<_> = entry
                    .buffer
                    .layout_runs()
                    .map(|r| (r.line_i, r.line_top, r.line_height))
                    .collect();
                let hard_n = info.text.split('\n').count();
                let clip_h = tb.clip_bounds.map(|c| c[3]).unwrap_or(-1.0);
                log::debug!(
                    target: "rui::textarea_diag",
                    "tid={:?} text={:?} leaf_h={:.1} clip_h={:.1} \
                     bounds_h={} top={} bottom={} buf_h={:?} hard={} runs_len={} runs={:?}",
                    info.template_id,
                    info.text,
                    tb.box_h,
                    clip_h,
                    bounds.bottom - bounds.top,
                    bounds.top,
                    bounds.bottom,
                    entry.buffer.size().1,
                    hard_n,
                    runs.len(),
                    runs,
                );
            }
        }
        text_areas.push(glyphon::TextArea {
            buffer: &entry.buffer,
            left: tb.x,
            top: tb.y + tb.dy,
            scale: tb.paint_scale,
            bounds,
            default_color: glyphon::Color::rgba(tb.color[0], tb.color[1], tb.color[2], tb.color[3]),
            custom_glyphs: tb
                .emoji_cache_key
                .as_ref()
                .map_or(&[], |k| text_renderer.emoji_cache.glyphs_for(k)),
        });

        if emoji_debug_enabled() {
            if let Some(entry) = tb.emoji_cache_key.as_ref().and_then(|k| {
                text_renderer
                    .emoji_cache
                    .entries
                    .get(k)
                    .or_else(|| text_renderer.emoji_cache.find_wrap_sibling(k))
            }) {
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
                        transform: crate::style::transform::MAT2D_IDENTITY,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: 0.0,
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
                                transform: crate::style::transform::MAT2D_IDENTITY,
                                clip_pos: [0.0, 0.0],
                                clip_size: InstanceInput::NO_CLIP_SIZE,
                                border_extra: 0.0,
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
            let ps = if tb.paint_scale.is_finite() && tb.paint_scale > 0.0 {
                tb.paint_scale
            } else {
                1.0
            };
            let ul_h = if tb.decoration_thickness > 0.0 {
                tb.decoration_thickness
            } else {
                (tb.text_size * 0.07).max(1.0)
            } * ps;
            let layout_ul_w = tb.measured_text_w.min(tb.box_w);
            let ul_w = layout_ul_w * ps;
            let ul_x = match tb.align {
                glyphon::cosmic_text::Align::Center => tb.x + (tb.box_w - layout_ul_w) * 0.5 * ps,
                glyphon::cosmic_text::Align::Right => tb.x + (tb.box_w - layout_ul_w) * ps,
                _ => tb.x,
            };
            let dc = tb.decoration_color.unwrap_or(tb.color);
            let line_color = [
                dc[0] as f32 / 255.0,
                dc[1] as f32 / 255.0,
                dc[2] as f32 / 255.0,
                dc[3] as f32 / 255.0,
            ];
            let deco_clip = expand_clip_for_text_decoration(
                tb.clip_bounds,
                tb.underline,
                tb.overline,
                tb.underline_offset,
                ul_h,
                ui_scale,
                ps,
            );
            let mut push_line = |x: f32, y: f32, w: f32, h: f32| {
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let Some([cx, cy, cw, ch]) = clip_rect_to_bounds([x, y, w, h], deco_clip) else {
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
                    transform: crate::style::transform::MAT2D_IDENTITY,
                    clip_pos: [0.0, 0.0],
                    clip_size: InstanceInput::NO_CLIP_SIZE,
                    border_extra: 0.0,
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
                // CSS `text-underline-offset`: positive = further from the text (down).
                let y = underline_line_y(tb.y, tb.box_h, ul_h, tb.underline_offset, ui_scale, ps);
                draw_at(y);
            }
            if tb.overline {
                let y = tb.y + (tb.underline_offset * ui_scale * ps);
                draw_at(y);
            }
            if tb.line_through {
                let y = tb.y + (tb.box_h * ps - ul_h) * 0.5;
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
    // Keep viewport in sync on resize even when every node is culled — but never
    // `prepare([])` while nodes exist (wipes atlas → blank text until geometry drifts
    // enough to bust text_cache_valid).
    text_renderer.viewport.update(
        queue,
        glyphon::Resolution {
            width: config.width,
            height: config.height,
        },
    );

    let text_area_count = text_areas.len();
    if text_area_count == 0 {
        return PrepareTextOutput {
            underline_instances,
            caret_instances,
            shadow_new_bitmaps,
            shadow_tex_draws,
            needs_warmup: true,
            culled_skip_prepare: true,
        };
    }

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
        culled_skip_prepare: false,
    }
}

#[cfg(test)]
mod text_shadow_tests {
    use super::{generate_shadow_bitmap, text_shadow_active};
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;

    #[test]
    fn text_shadow_active_allows_zero_blur_with_offset() {
        assert!(text_shadow_active(Some([0, 0, 0, 255]), [4.0, 0.0], 0.0));
        assert!(text_shadow_active(Some([0, 0, 0, 255]), [0.0, 0.0], 2.0));
        assert!(!text_shadow_active(Some([0, 0, 0, 255]), [0.0, 0.0], 0.0));
        assert!(!text_shadow_active(None, [4.0, 4.0], 2.0));
        assert!(!text_shadow_active(Some([0, 0, 0, 0]), [4.0, 4.0], 2.0));
    }

    #[test]
    fn shadow_bitmap_keeps_positive_dy_mass_below_centroid() {
        // Regression: textured UV now maps top→row0; Y-flipping the shadow bitmap
        // inverted soft shadows after the Bake/capture UV fix.
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        let family = best_font_family(&fs);
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(24.0, 36.0));
        buffer.set_size(Some(200.0), Some(48.0));
        let attrs = glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(700));
        buffer.set_text("Shadow", &attrs, glyphon::Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut fs, false);
        let mut swash = glyphon::SwashCache::new();
        let Some(bmp) =
            generate_shadow_bitmap(&buffer, &mut fs, &mut swash, [0, 0, 0, 200], 0.0, 8.0, 2.0)
        else {
            // Fonts unavailable in some CI images.
            return;
        };
        assert!(bmp.width > 0 && bmp.height > 0);
        assert!(bmp.rgba.chunks_exact(4).any(|p| p[3] > 0));

        let mut sum_a = 0.0_f64;
        let mut sum_ay = 0.0_f64;
        for y in 0..bmp.height as usize {
            for x in 0..bmp.width as usize {
                let a = bmp.rgba[(y * bmp.width as usize + x) * 4 + 3] as f64;
                if a > 0.0 {
                    sum_a += a;
                    sum_ay += a * y as f64;
                }
            }
        }
        assert!(sum_a > 0.0);
        let cy = sum_ay / sum_a;
        // draw_rect origin is content_min - pad; with +dy the alpha mass sits in the
        // lower half of the bitmap (not the top — that was the inverted/flipped bug).
        assert!(
            cy > bmp.height as f64 * 0.35,
            "shadow alpha centroid y={cy} should not sit in the top third of h={}",
            bmp.height
        );
    }
}

#[cfg(test)]
mod shape_cache_key_tests {
    use super::{
        apply_letter_spacing_approx, node_shape_hash, node_shape_identity_hash,
        node_shape_key_from_identity,
    };
    use crate::style::TextDirection;
    use std::borrow::Cow;
    use std::time::Instant;

    #[test]
    fn emoji_wrap_width_q_buckets_one_px_jitter() {
        assert_eq!(
            super::emoji_wrap_width_q(400.0),
            super::emoji_wrap_width_q(407.0)
        );
        assert_ne!(
            super::emoji_wrap_width_q(400.0),
            super::emoji_wrap_width_q(408.0)
        );
    }

    #[test]
    fn emoji_sequence_hash_is_content_only() {
        let a = super::emoji_sequence_content_hash("Family: 👨‍👩‍👧‍👦");
        let b = super::emoji_sequence_content_hash("Family: 👨‍👩‍👧‍👦");
        let c = super::emoji_sequence_content_hash("Skin tone: 👍🏽");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn letter_spacing_zero_borrows_without_alloc() {
        let s = "The quick brown fox 中文 عربي 😀";
        match apply_letter_spacing_approx(s, 0.0, 14.0) {
            Cow::Borrowed(b) => assert_eq!(b, s),
            Cow::Owned(_) => panic!("zero letter-spacing must not allocate on scroll hot path"),
        }
    }

    #[test]
    fn shape_key_from_identity_avoids_rehashing_text() {
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let text = "日本語テスト：こんにちは世界。汉字・かな";
        let identity = node_shape_identity_hash(
            text,
            14.0,
            21.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            [0, 0, 0, 255],
        );
        let from_id = node_shape_key_from_identity(identity, 320.0, 40.0, false);
        let full = node_shape_hash(
            text,
            14.0,
            21.0,
            320.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            [0, 0, 0, 255],
            false,
        );
        assert_eq!(from_id, full);
    }

    /// Feedback loop: scroll-like CPU for dense multilingual labels must stay cheap.
    /// Pre-fix: every node did text.to_string() + double full-text hash each frame.
    #[test]
    fn scroll_like_label_cpu_stays_under_budget() {
        let samples = [
            "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.",
            "中文测试：输入法、标点符号、全角半角、换行和字距表现。",
            "日本語テスト：こんにちは世界。漢字・かな・カタカナ・句読点「」『』の表示確認。",
            "Arabic: مرحبا بالعالم، هذا اختبار لاتجاه الكتابة والمحاذاة وعرض الحروف.",
            "Emoji: 😀 😁 😂 🤣 😅 🥲 🧠 🧪 🚀 🌍 🔥 ✅ ❌ 💡 🧩 👨‍👩‍👧‍👦",
            "Bidirectional sample: English 123 - العربية ٤٥٦ - עברית 789",
        ];
        // Simulate ~80 visible labels (text page band) × 60 scroll frames.
        let labels: Vec<&str> = samples.iter().cycle().take(80).copied().collect();
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let t0 = Instant::now();
        for _ in 0..60 {
            for (i, text) in labels.iter().enumerate() {
                let spaced = apply_letter_spacing_approx(text, 0.0, 14.0);
                assert!(matches!(spaced, Cow::Borrowed(_)));
                let identity = node_shape_identity_hash(
                    spaced.as_ref(),
                    14.0,
                    21.0,
                    400,
                    family,
                    align,
                    shaping,
                    0.0,
                    TextDirection::Auto,
                    [0, 0, 0, 255],
                );
                let _key =
                    node_shape_key_from_identity(identity, 400.0 + (i % 7) as f32, 24.0, false);
            }
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        // Debug + CI headroom: 80×60 identity hashes must stay well under a frame budget.
        // (Was allocating+double-hashing full strings — multi× slower.)
        assert!(
            ms < 50.0,
            "scroll-like shape-key CPU too slow: {ms:.2}ms for 80 labels × 60 frames"
        );
    }

    #[test]
    fn shape_hash_changes_when_text_alpha_flips() {
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let hidden = node_shape_hash(
            "x",
            14.0,
            14.0,
            20.0,
            20.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            [100, 100, 100, 0],
            false,
        );
        let shown = node_shape_hash(
            "x",
            14.0,
            14.0,
            20.0,
            20.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            [100, 100, 100, 200],
            false,
        );
        assert_ne!(
            hidden, shown,
            "opacity/color alpha must invalidate shaped cache (Input adornments)"
        );
    }

    #[test]
    fn bounds_miss_keeps_shape_identity_when_only_width_changes() {
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let text = "Bidirectional sample: English 123 - العربية";
        let color = [30u8, 30, 30, 255];
        let shape_a = node_shape_identity_hash(
            text,
            14.0,
            21.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
        );
        let shape_b = node_shape_identity_hash(
            text,
            14.0,
            21.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
        );
        assert_eq!(shape_a, shape_b, "identity must ignore leaf bounds");

        let key_narrow = node_shape_hash(
            text,
            14.0,
            21.0,
            160.0,
            42.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            false,
        );
        let key_wide = node_shape_hash(
            text,
            14.0,
            21.0,
            260.0,
            42.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            false,
        );
        assert_ne!(key_narrow, key_wide, "bounds change must miss Exact key");
        // Same shape_hash + different key_hash ⇒ Bounds path (iced Difference::Bounds).
        assert_eq!(
            node_shape_identity_hash(
                text,
                14.0,
                21.0,
                400,
                family,
                align,
                shaping,
                0.0,
                TextDirection::Auto,
                color,
            ),
            shape_a
        );
    }

    #[test]
    fn paint_key_hash_ignores_force_fast_flag() {
        // prepare_text_frame always passes force_fast=false into node_shape_hash so
        // scroll fast↔settle does not mass-miss (text page white flash).
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let color = [0u8, 0, 0, 255];
        let settle = node_shape_hash(
            "Typography",
            14.0,
            21.0,
            201.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Ltr,
            color,
            false,
        );
        let scroll = node_shape_hash(
            "Typography",
            14.0,
            21.0,
            201.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Ltr,
            color,
            false,
        );
        assert_eq!(settle, scroll);
    }

    #[test]
    fn fast_mode_quantizes_bounds_to_cut_exact_churn() {
        let family = glyphon::Family::SansSerif;
        let align = glyphon::cosmic_text::Align::Left;
        let shaping = glyphon::Shaping::Advanced;
        let color = [0u8, 0, 0, 255];
        let a = node_shape_hash(
            "hi",
            14.0,
            21.0,
            201.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            true,
        );
        let b = node_shape_hash(
            "hi",
            14.0,
            21.0,
            202.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            true,
        );
        assert_eq!(
            a, b,
            "2px quantize during fast should collapse nearby widths"
        );
        let c = node_shape_hash(
            "hi",
            14.0,
            21.0,
            201.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            false,
        );
        let d = node_shape_hash(
            "hi",
            14.0,
            21.0,
            202.0,
            40.0,
            400,
            family,
            align,
            shaping,
            0.0,
            TextDirection::Auto,
            color,
            false,
        );
        assert_ne!(c, d, "rest path keeps 1px Exact sensitivity");
    }
}

#[cfg(test)]
mod hidden_scroll_cull_tests {
    use super::clip_rect_to_bounds;

    #[test]
    fn caret_pan_cull_uses_unshifted_origin() {
        let scroll = 80.0_f32;
        let field = [10.0, 0.0, 40.0, 20.0];
        let shifted_x = field[0] - scroll;
        let shifted = clip_rect_to_bounds([shifted_x, 0.0, field[2], field[3]], Some(field));
        assert!(
            shifted.is_none(),
            "shifted viewport-sized rect must miss the field"
        );
        let cull = clip_rect_to_bounds([shifted_x + scroll, 0.0, field[2], field[3]], Some(field));
        assert!(cull.is_some(), "unshifted origin must overlap the field");
    }

    #[test]
    fn ellipsis_buffer_uses_box_width_not_intrinsic() {
        use crate::style::TextOverflow;
        let intrinsic = 240.0_f32;
        let box_w = 80.0_f32;
        let w = super::nowrap_buffer_width(TextOverflow::Ellipsis, true, intrinsic, box_w);
        assert!((w - box_w).abs() < 0.01, "ellipsis buffer w={w}");
        let hidden = super::nowrap_buffer_width(TextOverflow::Hidden, true, intrinsic, box_w);
        assert!(
            (hidden - intrinsic).abs() < 0.01,
            "hidden still shapes the full line, got {hidden}"
        );
    }

    #[test]
    fn overflowing_text_overflow_forces_start_align() {
        assert!(crate::style::TextOverflow::Ellipsis.start_align_when_line_overflows());
        assert!(crate::style::TextOverflow::Hidden.start_align_when_line_overflows());
        assert!(!crate::style::TextOverflow::Newline.start_align_when_line_overflows());
    }
}

#[cfg(test)]
mod input_shaping_stability_tests {
    use super::nowrap_layout_width_from_text;
    use crate::renderer::font_loader;
    use crate::renderer::text_hit::best_font_family;
    use crate::renderer::text_prepare_debug::is_input_inline_text;
    use crate::style::TextOverflow;

    fn shaped_line_w(fs: &mut glyphon::FontSystem, text: &str, shaping: glyphon::Shaping) -> f32 {
        let size = 14.0_f32;
        let lh = size * 1.5;
        let family = best_font_family(fs);
        let mut buffer = glyphon::Buffer::new(fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(Some(1024.0), Some(lh + 2.0));
        buffer.set_text(
            text,
            &glyphon::Attrs::new()
                .family(family)
                .weight(glyphon::fontdb::Weight(400)),
            shaping,
            None,
        );
        buffer.shape_until_scroll(fs, false);
        buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0_f32, f32::max)
    }

    #[test]
    fn scroll_fast_skips_reshape_only_for_idle_inputs() {
        assert!(super::scroll_fast_skips_input_reshape(true, true, false));
        assert!(
            !super::scroll_fast_skips_input_reshape(true, true, true),
            "focused caret must reshape"
        );
        assert!(!super::scroll_fast_skips_input_reshape(true, false, false));
        assert!(!super::scroll_fast_skips_input_reshape(false, true, false));
    }

    #[test]
    fn scroll_fast_skips_intrinsic_only_for_idle_unpanned_inputs() {
        assert!(super::scroll_fast_skips_input_intrinsic_prepare(
            true, true, false, 0.0
        ));
        assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
            true, true, true, 0.0
        ));
        assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
            true, true, false, 12.0
        ));
        assert!(!super::scroll_fast_skips_input_intrinsic_prepare(
            false, true, false, 0.0
        ));
    }

    #[test]
    fn shaped_identity_pool_reclaims_across_index_churn() {
        // Documents: band cull reorders text_nodes; identity pool (not in-vec swap)
        // must let the same shape_hash survive at a new index.
        let mut pool = std::collections::HashMap::new();
        let mut fs = crate::renderer::font_loader::create_font_system();
        let mut a = super::make_dummy_cache_entry(&mut fs);
        a.shape_hash = 0xA11CE;
        a.key_hash = 0xA11CE_u64.wrapping_mul(3);
        a.measured_text_w = 42.0;
        super::park_shaped_entry(&mut pool, a);
        assert!(pool.contains_key(&0xA11CE));

        let mut slot = super::make_dummy_cache_entry(&mut fs);
        slot.shape_hash = 0xB0B;
        let reclaimed = pool.remove(&0xA11CE).expect("parked");
        let displaced = std::mem::replace(&mut slot, reclaimed);
        super::park_shaped_entry(&mut pool, displaced);
        assert_eq!(slot.shape_hash, 0xA11CE);
        assert_eq!(slot.measured_text_w, 42.0);
        assert!(pool.contains_key(&0xB0B));
        assert!(!pool.contains_key(&0xA11CE));
    }

    #[test]
    fn password_placeholder_basic_vs_advanced_differs() {
        // Documents why Input must not flip shaping on focus (caret install).
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        let basic = shaped_line_w(&mut fs, "Password", glyphon::Shaping::Basic);
        let advanced = shaped_line_w(&mut fs, "Password", glyphon::Shaping::Advanced);
        assert!(
            (basic - advanced).abs() > 0.05,
            "expected Basic/Advanced advance difference for Password; basic={basic} adv={advanced}"
        );
    }

    #[test]
    fn input_inline_template_id_detected() {
        assert!(is_input_inline_text(Some("__rui_input:pw:text")));
        assert!(!is_input_inline_text(Some("__rui_input:pw:clear")));
        assert!(!is_input_inline_text(Some("label")));
        assert!(!is_input_inline_text(None));
    }

    #[test]
    fn nowrap_live_intrinsic_prevents_soft_wrap_after_cjk_latin_preedit() {
        // Textarea IME: committed CJK + in-flight pinyin under a narrow leaf.
        // Stale prepared intrinsic (CJK only) + Word wrap → extra runs → line jumps down.
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        crate::text::install_shared_font_system(&mut fs);

        let size = 14.0_f32;
        let lh = 21.0_f32;
        let box_w = 72.0_f32;
        let committed = "你好世界";
        let live = format!("{committed}nihaoshijie");
        let stale = crate::text::prepare(committed, size, 400, lh);
        let stale_w = crate::text::intrinsic_width(&stale);
        let live_w =
            nowrap_layout_width_from_text(&live, size, 400, lh, TextOverflow::Clip, false, box_w);
        assert!(
            live_w > stale_w + 8.0,
            "live wrap must exceed stale CJK-only intrinsic (live={live_w} stale={stale_w})"
        );

        let family = best_font_family(&fs);
        let attrs = glyphon::Attrs::new()
            .family(family)
            .weight(glyphon::fontdb::Weight(400));

        let mut soft = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        soft.set_size(Some(stale_w.max(box_w)), None);
        soft.set_text(&live, &attrs, glyphon::Shaping::Advanced, None);
        soft.shape_until_scroll(&mut fs, false);
        let soft_runs = soft.layout_runs().count();

        let mut hard = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        hard.set_wrap(glyphon::Wrap::None);
        hard.set_size(Some(live_w), None);
        hard.set_text(&live, &attrs, glyphon::Shaping::Advanced, None);
        hard.shape_until_scroll(&mut fs, false);
        let hard_runs = hard.layout_runs().count();

        assert!(
            soft_runs > 1,
            "stale narrow Word-wrap must invent extra runs (got {soft_runs})"
        );
        assert_eq!(
            hard_runs, 1,
            "nowrap + live intrinsic must keep one run; got {hard_runs}"
        );
    }

    #[test]
    fn resize_rgba_nn_letterboxes_tall_source() {
        // Tall 8x16 opaque block → 16x16 must stay ~8px wide (not stretched to 16).
        let mut rgba = vec![0u8; 8 * 16 * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[255, 200, 0, 255]);
        }
        let src = crate::renderer::text::EmojiRaster::new(8, 16, rgba);
        let out = super::resize_rgba_nn(&src, 16);
        assert_eq!((out.width, out.height), (16, 16));
        let mut minx = 16usize;
        let mut maxx = 0usize;
        let mut last_row = 0usize;
        for y in 0..16 {
            for x in 0..16 {
                if out.rgba[(y * 16 + x) * 4 + 3] > 0 {
                    minx = minx.min(x);
                    maxx = maxx.max(x);
                    if y == 15 {
                        last_row += 1;
                    }
                }
            }
        }
        let ink_w = maxx + 1 - minx;
        assert!(
            ink_w <= 9,
            "letterbox must preserve aspect (got ink_w={ink_w}; stretch would be ~16)"
        );
        assert_eq!(last_row, 0, "safe inset must keep last row transparent");
    }

    #[test]
    fn resize_rgba_nn_pads_full_bleed_source() {
        let mut rgba = vec![0u8; 14 * 14 * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[255, 220, 0, 255]);
        }
        let src = crate::renderer::text::EmojiRaster::new(14, 14, rgba);
        let out = super::resize_rgba_nn(&src, 14);
        let last = (0..14)
            .filter(|&x| out.rgba[(13 * 14 + x) * 4 + 3] > 0)
            .count();
        let first = (0..14).filter(|&x| out.rgba[x * 4 + 3] > 0).count();
        assert_eq!(last, 0, "full-bleed source must gain bottom pad");
        assert_eq!(first, 0, "full-bleed source must gain top pad");
    }

    #[test]
    fn underline_offset_positive_moves_away_from_text() {
        // CSS text-underline-offset: larger → further below the box (larger Y).
        let y0 = super::underline_line_y(10.0, 20.0, 2.0, 0.0, 1.0, 1.0);
        let y4 = super::underline_line_y(10.0, 20.0, 2.0, 4.0, 1.0, 1.0);
        assert!(
            (y4 - y0 - 4.0).abs() < 0.01,
            "underline-offset-4 must shift +4px down (y0={y0}, y4={y4})"
        );
    }

    #[test]
    fn underline_offset_survives_tight_text_box_clip() {
        // Tight leaf scissor = text box; CSS +offset paints below the box.
        let box_clip = Some([10.0, 10.0, 40.0, 14.0]);
        let ul_h = 1.0;
        let y = super::underline_line_y(10.0, 14.0, ul_h, 2.0, 1.0, 1.0);
        assert!(
            super::clip_rect_to_bounds([10.0, y, 40.0, ul_h], box_clip).is_none(),
            "without expansion the underline is fully clipped"
        );
        let expanded =
            super::expand_clip_for_text_decoration(box_clip, true, false, 2.0, ul_h, 1.0, 1.0);
        assert!(
            super::clip_rect_to_bounds([10.0, y, 40.0, ul_h], expanded).is_some(),
            "decoration clip expand must keep underline visible"
        );
    }
}
