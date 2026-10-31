//! 将布局得到的 `TextLayoutInfo` 转为 glyphon 文本区与下划线实例，并执行 `prepare`。
//!
//! 调试：设置环境变量 `JADIX_TEXT_DEBUG`（见 [`super::text_prepare_debug`]）。

use std::borrow::Cow;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use web_time::{Duration, Instant};
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
use super::text::JadixTextRenderer;
use super::text_hit::{best_font_family, measure_prefix_width, truncate_with_ellipsis};
use super::twemoji_name;
use super::InstanceInput;

#[derive(Debug, Clone)]
enum TextSegment {
    Plain,
    Emoji { text: String, start: usize },
}

/// Embedded object slot — ~1em advance; must not be whitespace (see [`emoji_layout::EMOJI_LAYOUT_SLOT`]).
const EMOJI_LAYOUT_SLOT: char = emoji_layout::EMOJI_LAYOUT_SLOT;

fn stable_physical_text_y(y: f32) -> f32 {
    // Nested device-pixel scroll offsets can leave an integer a few ulps below
    // its true value. Cosmic-text truncates Y for hinting, so that noise would
    // move fresh glyphs one pixel relative to retained scroll copies. Preserve
    // intentional fractional placement; only remove sub-millipixel noise.
    let nearest = y.round();
    if (y - nearest).abs() <= 0.001 {
        nearest
    } else {
        y
    }
}

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
        for (byte, ch) in line.text().char_indices() {
            out.push(
                if line.attrs_list().get_span(byte).metadata == emoji_layout::EMOJI_SLOT_METADATA {
                    EMOJI_LAYOUT_SLOT
                } else {
                    ch
                },
            );
        }
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
    /// Same space as `rect`. `None` = no fragment clip (must not be used for
    /// scroll/overflow text — unclipped quads flicker against glyphon TextBounds).
    pub clip: Option<[f32; 4]>,
}

/// `prepare_text_frame` 的完整输出。
pub struct PrepareTextOutput {
    pub underline_instances: Vec<InstanceInput>,
    /// Input/label caret hairlines — drawn *after* glyphon text so they stay visible.
    pub caret_instances: Vec<InstanceInput>,
    pub shadow_new_bitmaps: Vec<ShadowNewBitmap>,
    pub shadow_tex_draws: Vec<ShadowTexDraw>,
    pub needs_warmup: bool,
    /// Nodes existed but every TextArea was culled. The drawable glyph mesh is
    /// empty; shaped buffers and atlas storage remain reusable. Callers must not
    /// treat this as a successful nonempty prepare cache or submit an old mesh.
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

/// Composite coverage with the same integer rounding as the shadow reference.
/// Clip once per glyph; transparent outline pixels never change the destination.
fn stamp_shadow_glyph(
    dst: &mut [u16],
    dst_size: [u32; 2],
    coverage: &[u8],
    glyph_size: [u32; 2],
    origin: [i32; 2],
    alpha: u8,
) {
    let [bw, bh] = dst_size;
    let [gw, gh] = glyph_size;
    let [x, y] = origin;
    let left = (-x).max(0).min(gw as i32) as usize;
    let right = (bw as i32 - x).clamp(left as i32, gw as i32) as usize;
    let top = (-y).max(0).min(gh as i32) as usize;
    let bottom = (bh as i32 - y).clamp(top as i32, gh as i32) as usize;
    if left == right || alpha == 0 {
        return;
    }
    for row in top..bottom {
        let src_start = row * gw as usize + left;
        let dst_start = (y + row as i32) as usize * bw as usize + (x + left as i32) as usize;
        let len = right - left;
        for (&a, dest) in coverage[src_start..src_start + len]
            .iter()
            .zip(&mut dst[dst_start..dst_start + len])
        {
            // src + dst - floor(src*dst/255) == src + ceil(dst*(255-src)/255).
            // Widen intermediates so checked builds can also vectorize this loop;
            // even a full u16 destination cannot overflow these u32 products/sums.
            let src = (u32::from(a) * u32::from(alpha)) / 255;
            *dest = (src + (u32::from(*dest) * (255 - src)).div_ceil(255)) as u16;
        }
    }
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
    }
    let mut glyph_entries: Vec<GlyphEntry> = Vec::new();
    for run in buffer.layout_runs() {
        let line_y_px = run.line_y.round() as i32;
        for g in run.glyphs.iter() {
            if emoji_layout::glyph_is_emoji_layout_slot(&run, g) {
                continue;
            }
            let physical = g.physical((0.0, 0.0), 1.0);
            let img = match swash_cache
                .get_image(font_system, physical.cache_key)
                .as_ref()
            {
                Some(img) => img,
                None => continue,
            };
            glyph_entries.push(GlyphEntry {
                px: physical.x + img.placement.left,
                py: line_y_px + physical.y - img.placement.top,
                data: if matches!(img.content, glyphon::SwashContent::Color) {
                    img.data.chunks_exact(4).map(|px| px[3]).collect()
                } else {
                    img.data.clone()
                },
                width: img.placement.width,
                height: img.placement.height,
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
    let draw_blur = radius > TEXTURED_SHADOW_MIN_BLUR && alpha > 6;

    let mut stamp = |ox: f32, oy: f32, sample_alpha: u8| {
        if sample_alpha == 0 {
            return;
        }
        for ge in &glyph_entries {
            let px0 = ge.px - bmp_x0 + ox.round() as i32;
            let py0 = ge.py - bmp_y0 + oy.round() as i32;
            stamp_shadow_glyph(
                &mut alpha_buf,
                [bw, bh],
                &ge.data,
                [ge.width, ge.height],
                [px0, py0],
                sample_alpha,
            );
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
    requested_weight: u16,
    synthetic_bold: bool,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    ((scaled_size * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    ((line_height * 10.0).clamp(0.0, u16::MAX as f32) as u16).hash(&mut h);
    weight.hash(&mut h);
    requested_weight.hash(&mut h);
    synthetic_bold.hash(&mut h);
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
        weight,
        false,
    );
    node_shape_key_from_identity(identity, scaled_w, scaled_h, force_fast_mode)
}

fn rich_shape_identity_hash(base: u64, spans: Option<&[crate::style::TextSpan]>) -> u64 {
    let Some(spans) = spans.filter(|spans| !spans.is_empty()) else {
        return base;
    };
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    base.hash(&mut hash);
    spans.len().hash(&mut hash);
    for span in spans {
        span.text.len().hash(&mut hash);
        span.color.hash(&mut hash);
        span.weight.hash(&mut hash);
        span.size.map(f32::to_bits).hash(&mut hash);
        span.italic.hash(&mut hash);
        span.monospace.hash(&mut hash);
        span.background.hash(&mut hash);
        std::mem::discriminant(&span.kind).hash(&mut hash);
    }
    hash.finish()
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

fn set_buffer_text_alignment(
    buffer: &mut glyphon::Buffer,
    prewrapped_source: Option<&str>,
    align: glyphon::cosmic_text::Align,
    style: &crate::style::TextStyle,
    use_css_alignment: bool,
) {
    use glyphon::cosmic_text::{Align, AlignLast, BidiParagraphs};
    let count_content = |text: &str| {
        text.chars()
            .filter(|character| !character.is_whitespace() && *character != '\u{200b}')
            .count()
    };
    let source = prewrapped_source.map(crate::renderer::emoji_layout::shape_text_for_layout);
    let mut source_paragraphs = Vec::new();
    if let Some(source) = source.as_deref() {
        let mut paragraphs = BidiParagraphs::new(source);
        while let Some((paragraph, rtl)) = paragraphs.next_with_direction() {
            source_paragraphs.push((count_content(paragraph), rtl));
        }
    }
    let mut paragraphs = source_paragraphs.into_iter();
    let mut paragraph = paragraphs.next().unwrap_or((0, false));
    for line in &mut buffer.lines {
        let content = count_content(line.text());
        let (final_line, paragraph_rtl) = if source.is_some() {
            while paragraph.0 == 0 && content > 0 {
                paragraph = paragraphs.next().unwrap_or((content, false));
            }
            let rtl = paragraph.1;
            paragraph.0 = paragraph.0.saturating_sub(content);
            (paragraph.0 == 0, rtl)
        } else {
            let rtl = BidiParagraphs::new(line.text())
                .next_with_direction()
                .is_some_and(|(_, rtl)| rtl);
            (true, rtl)
        };
        let rtl = match style.direction {
            TextDirection::Ltr => false,
            TextDirection::Rtl => true,
            TextDirection::Auto => paragraph_rtl,
        };
        let start = if rtl { Align::Right } else { Align::Left };
        let end = if rtl { Align::Left } else { Align::Right };
        let line_align = if use_css_alignment {
            match style.align_resolved() {
                crate::style::TextAlign::Start | crate::style::TextAlign::MatchParent => start,
                crate::style::TextAlign::End => end,
                _ => align,
            }
        } else {
            align
        };
        let last = if final_line {
            match style
                .align_last_resolved()
                .to_glyphon(style.align_resolved())
            {
                Some(AlignLast::Start) => Some(if rtl {
                    AlignLast::Right
                } else {
                    AlignLast::Left
                }),
                Some(AlignLast::End) => Some(if rtl {
                    AlignLast::Left
                } else {
                    AlignLast::Right
                }),
                None if source.is_some() && line_align == Align::Justified => Some(if rtl {
                    AlignLast::Right
                } else {
                    AlignLast::Left
                }),
                last => last,
            }
        } else if line_align == Align::Justified {
            Some(AlignLast::Justified)
        } else {
            None
        };
        line.set_align(Some(line_align));
        line.set_align_last(last);
        if source.is_some() && final_line {
            paragraph = paragraphs.next().unwrap_or((0, false));
        }
    }
}

#[cfg(test)]
#[path = "tests/text_alignment.rs"]
mod text_alignment_tests;

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
    text_style: &crate::style::TextStyle,
) -> f32 {
    if nowrap || soft_w_target <= 8.0 {
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
        let align = buffer
            .lines
            .first()
            .and_then(|line| line.align())
            .unwrap_or(glyphon::cosmic_text::Align::Left);
        buffer.set_wrap(glyphon::Wrap::None);
        // fit.shape_text may already contain emoji object slots — keep them invisible.
        set_shaped_buffer_text(
            buffer,
            font_system,
            fit.shape_text.as_ref(),
            attrs_owned.as_attrs(),
            glyphon::Shaping::Advanced,
            emoji_custom_glyph_enabled(),
            size * ui_scale,
            line_height,
            &[],
        );
        set_buffer_text_alignment(
            buffer,
            Some(text),
            align,
            text_style,
            text_style.input_caret.is_none(),
        );
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
        let raw = std::env::var("JADIX_TEXT_PERF")
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
        std::env::var("JADIX_TEXT_PRESHAPE_MARGIN")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(768.0)
    })
}

fn preshape_budget() -> usize {
    static BUDGET: OnceLock<usize> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("JADIX_TEXT_PRESHAPE_BUDGET")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4)
    })
}

const CARET_BLINK_PHASE_MS: u128 = 520;
/// After a caret move, keep the I-beam solid this long before the on/off cycle starts.
const CARET_BLINK_SOLID_MS: u128 = 400;

struct CaretBlinkState {
    origin: Instant,
    gen: u128,
}

static CARET_BLINK: Mutex<Option<CaretBlinkState>> = Mutex::new(None);

fn caret_blink_state() -> MutexGuard<'static, Option<CaretBlinkState>> {
    CARET_BLINK.lock().unwrap_or_else(|e| e.into_inner())
}

fn caret_blink_origin() -> Instant {
    let mut g = caret_blink_state();
    g.get_or_insert_with(|| CaretBlinkState {
        origin: Instant::now(),
        gen: 0,
    })
    .origin
}

/// Reset the caret to a solid I-beam, then blink after [`CARET_BLINK_SOLID_MS`].
pub(crate) fn restart_caret_blink() {
    let mut g = caret_blink_state();
    let gen = g.as_ref().map(|s| s.gen.wrapping_add(1)).unwrap_or(1);
    *g = Some(CaretBlinkState {
        origin: Instant::now(),
        gen,
    });
}

/// Whether the I-beam should paint for a given elapsed-since-restart (ms).
/// Solid pause first; then odd phases are hidden so blinking starts with an off beat.
pub(crate) fn caret_blink_visible_at_elapsed(elapsed_ms: u128) -> bool {
    if elapsed_ms < CARET_BLINK_SOLID_MS {
        return true;
    }
    let blink = (elapsed_ms - CARET_BLINK_SOLID_MS) / CARET_BLINK_PHASE_MS;
    !blink.is_multiple_of(2)
}

pub(crate) fn caret_blink_next_remain_ms(elapsed_ms: u128) -> u64 {
    if elapsed_ms < CARET_BLINK_SOLID_MS {
        return (CARET_BLINK_SOLID_MS - elapsed_ms).max(1) as u64;
    }
    let into = (elapsed_ms - CARET_BLINK_SOLID_MS) % CARET_BLINK_PHASE_MS;
    (CARET_BLINK_PHASE_MS - into).max(1) as u64
}

pub(crate) fn caret_blink_visible() -> bool {
    // Unit tests call push_input_chrome_instances in a tight loop; wall-clock blink
    // phase stays OFF for all iterations and flakes caret geometry tests.
    if cfg!(test) {
        return true;
    }
    caret_blink_visible_at_elapsed(caret_blink_origin().elapsed().as_millis())
}

pub(crate) fn caret_blink_phase() -> u128 {
    let (elapsed, gen) = {
        let origin = caret_blink_origin();
        let gen = caret_blink_state().as_ref().map(|s| s.gen).unwrap_or(0);
        (origin.elapsed().as_millis(), gen)
    };
    if elapsed < CARET_BLINK_SOLID_MS {
        gen.saturating_mul(2)
    } else {
        let blink = (elapsed - CARET_BLINK_SOLID_MS) / CARET_BLINK_PHASE_MS;
        gen.saturating_mul(2) + 2 + blink
    }
}

/// Instant when the caret blink phase will flip next (for `ControlFlow::WaitUntil`).
pub(crate) fn caret_blink_next_toggle_at() -> Instant {
    let elapsed_ms = caret_blink_origin().elapsed().as_millis();
    Instant::now() + Duration::from_millis(caret_blink_next_remain_ms(elapsed_ms))
}

#[allow(dead_code)]
fn input_line_height(style: &crate::style::TextStyle, scaled_size: f32, ui_scale: f32) -> f32 {
    let lh = if let Some(lh) = style.line_height_px {
        lh * ui_scale
    } else {
        let scale = style
            .line_height_scale
            .unwrap_or_else(crate::utils::text_field::text_edit_default_leading_scale);
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

/// Cap for band-cull identity pool (counts buffers, not unique hashes).
const SHAPED_IDENTITY_POOL_CAP: usize = 256;

type ShapedIdentityPool = std::collections::HashMap<u64, VecDeque<super::text::ShapedTextEntry>>;

#[inline]
fn identity_pool_len(pool: &ShapedIdentityPool) -> usize {
    pool.values().map(VecDeque::len).sum()
}

#[inline]
fn park_shaped_entry(pool: &mut ShapedIdentityPool, entry: super::text::ShapedTextEntry) {
    if entry.shape_hash == 0 {
        return;
    }
    while identity_pool_len(pool) >= SHAPED_IDENTITY_POOL_CAP {
        let victim = pool
            .keys()
            .copied()
            .find(|&k| k != entry.shape_hash)
            .or_else(|| pool.keys().copied().next());
        let Some(k) = victim else {
            break;
        };
        let empty = if let Some(q) = pool.get_mut(&k) {
            q.pop_front();
            q.is_empty()
        } else {
            break;
        };
        if empty {
            pool.remove(&k);
        }
    }
    pool.entry(entry.shape_hash).or_default().push_back(entry);
}

#[inline]
fn take_pooled_entry(
    pool: &mut ShapedIdentityPool,
    shape_hash: u64,
) -> Option<super::text::ShapedTextEntry> {
    let q = pool.get_mut(&shape_hash)?;
    let entry = q.pop_front()?;
    if q.is_empty() {
        pool.remove(&shape_hash);
    }
    Some(entry)
}

/// Park every live slot so the next list (prefix-shift, clip-strip subset,
/// duplicate emoji runs) can FIFO-reclaim by `shape_hash`.
///
/// Live buffers always win: inserting them one-by-one into a full pool evicted
/// the entries we were about to reclaim (`hits ≈ cap − leftover`, perpetual
/// `shape_miss ≈ nodes − hits` while scrolling the gallery).
#[cfg(test)]
fn park_shaped_cache(cache: &mut Vec<super::text::ShapedTextEntry>, pool: &mut ShapedIdentityPool) {
    park_shaped_cache_with_cap(cache, pool, SHAPED_IDENTITY_POOL_CAP);
}

#[cfg(test)]
fn park_shaped_cache_with_cap(
    cache: &mut Vec<super::text::ShapedTextEntry>,
    pool: &mut ShapedIdentityPool,
    cap: usize,
) {
    park_shaped_suffix(cache, pool, 0, cap);
}

fn park_shaped_suffix(
    cache: &mut Vec<super::text::ShapedTextEntry>,
    pool: &mut ShapedIdentityPool,
    start: usize,
    cap: usize,
) {
    if start >= cache.len() {
        return;
    }
    let mut live = ShapedIdentityPool::new();
    for entry in cache.drain(start..) {
        if entry.shape_hash == 0 {
            continue;
        }
        live.entry(entry.shape_hash).or_default().push_back(entry);
    }
    let live_count = identity_pool_len(&live);
    let mut room = cap.saturating_sub(live_count);
    for (k, mut q) in std::mem::take(pool) {
        if q.is_empty() || room == 0 {
            continue;
        }
        let dest = live.entry(k).or_default();
        while room > 0 && !q.is_empty() {
            dest.push_back(q.pop_front().unwrap());
            room -= 1;
        }
    }
    live.retain(|_, q| !q.is_empty());
    *pool = live;
}

fn assign_shaped_slot(
    cache: &mut Vec<super::text::ShapedTextEntry>,
    pool: &mut ShapedIdentityPool,
    font_system: &mut glyphon::FontSystem,
    i: usize,
    shape_hash: u64,
) {
    if i < cache.len() && cache[i].shape_hash != shape_hash {
        // A membership/order change invalidates only the unvisited suffix.
        // Park it before reclaiming so prefix drops and duplicate runs remain
        // FIFO, without swapping a buffer already assigned to this frame.
        park_shaped_suffix(cache, pool, i, SHAPED_IDENTITY_POOL_CAP);
    }
    while cache.len() < i {
        cache.push(make_dummy_cache_entry(font_system));
    }
    if cache.len() == i {
        cache.push(
            take_pooled_entry(pool, shape_hash)
                .unwrap_or_else(|| make_dummy_cache_entry(font_system)),
        );
    }
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

/// Whether this glyph node should paint this frame.
///
/// `origin` / `clip` are visual (already scroll-adjusted). Caret pan stores
/// `scroll_x` and the leaf clip in layout space, so the test rect adds
/// `scroll_x` back. A dest-reset fragment is a clip tighter than the leaf and
/// stays visual: adding `scroll_x` shoves the test rect off the left piece.
/// That piece is then `preshape_only`, and a non-ASCII caption (关闭 + Latin)
/// is warmed up a frame later than the piece that still intersects — the two
/// scripts change one after the other.
pub(crate) fn glyph_node_visible_this_frame(
    origin: [f32; 2],
    size: [f32; 2],
    scroll_x: f32,
    clip: Option<[f32; 4]>,
    viewport: [f32; 4],
) -> bool {
    let w = size[0].max(0.0);
    let h = size[1].max(0.0);
    let cull_x = origin[0] + scroll_x.max(0.0);
    let node_rect = [cull_x, origin[1], w, h];
    if clip_rect_to_bounds(node_rect, clip)
        .and_then(|c| clip_rect_to_bounds(c, Some(viewport)))
        .is_some()
    {
        return true;
    }
    let Some(c) = clip else {
        return false;
    };
    let tight = c[2] + 0.5 < w || c[3] + 0.5 < h;
    tight && clip_rect_to_bounds(c, Some(viewport)).is_some()
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

/// Overflow:hidden clips descendant shadows (including synthetic-bold). Expanding
/// this by blur/offset let the last sidebar label paint past the footer separator.
fn shadow_clip_for_glyphon(
    node_clip: Option<[f32; 4]>,
    _outset: f32,
    viewport: [f32; 4],
) -> Option<[f32; 4]> {
    clip_rect_to_bounds(node_clip?, Some(viewport))
}

/// Integer overflow clip for glyphon `TextBounds` and textured synthetic-bold.
///
/// Near edges use `floor` (same as the scroll-chase coverage path — `ceil` origin
/// shrank the box every sub-pixel step and strobed). Far edges use `floor` so
/// overflow:hidden never grows onto the next sibling (sidebar footer separator).
/// A sliver that snaps empty must **skip** paint; treating it as "no clip"
/// draws the last visible row unclipped for a frame after scroll stops.
#[inline]
pub fn integerize_clip_rect(c: [f32; 4]) -> Option<[f32; 4]> {
    let left = c[0].floor();
    let top = c[1].floor();
    let right = (c[0] + c[2]).floor();
    let bottom = (c[1] + c[3]).floor();
    let w = right - left;
    let h = bottom - top;
    if w <= 0.0 || h <= 0.0 {
        None
    } else {
        Some([left, top, w, h])
    }
}

/// GPU/glyphon clip for a node.
///
/// - `Some(None)`: no overflow ancestor (draw unclipped).
/// - `Some(Some(rect))`: integer overflow scissor.
/// - `None`: clipped away — **do not draw**, and do not pass `clip: None`
///   to [`shadow_tex_instance_clip`] (that disables the scissor).
#[inline]
pub fn paint_overflow_clip(
    clip: Option<[f32; 4]>,
    had_overflow_clip: bool,
) -> Option<Option<[f32; 4]>> {
    match clip {
        None if !had_overflow_clip => Some(None),
        None => None,
        Some(c) => integerize_clip_rect(c).map(Some),
    }
}

/// Overflow clip for textured `text-shadow` quads (`clip_size[0] < 0` disables).
#[inline]
pub fn shadow_tex_instance_clip(clip: Option<[f32; 4]>) -> ([f32; 2], [f32; 2]) {
    match clip {
        Some([x, y, w, h]) if w > 0.0 && h > 0.0 => ([x, y], [w, h]),
        _ => ([0.0, 0.0], [-1.0, -1.0]),
    }
}
#[inline]
fn text_shadow_active(shadow_color: Option<[u8; 4]>, offset: [f32; 2], blur: f32) -> bool {
    shadow_color.is_some_and(|c| c[3] > 0)
        && (blur > 0.0 || offset[0].abs() > 0.0 || offset[1].abs() > 0.0)
}

/// Blur below this is a single extra Coverage layer — not worth a separate bitmap.
const TEXTURED_SHADOW_MIN_BLUR: f32 = 0.75;

/// Bake `text-shadow` into a textured quad only when the user asked for a soft
/// blur. Synthetic-bold (and sharp dx/dy shadows) must stay on the glyphon
/// Coverage path: the bitmap is stamped at `physical((0,0))` while glyphs use
/// `physical((left, top))`, so a 1 device-px chase makes complex CJK and the
/// fake-bold halo walk out of lockstep.
#[inline]
fn uses_textured_shadow_bitmap(user_text_shadow: bool, blur_px: f32) -> bool {
    user_text_shadow && blur_px > TEXTURED_SHADOW_MIN_BLUR
}

pub(crate) fn text_bounds_from_clip(
    clip: Option<[f32; 4]>,
    width: u32,
    height: u32,
) -> glyphon::TextBounds {
    if let Some(b) = clip.and_then(integerize_clip_rect) {
        glyphon::TextBounds {
            left: b[0] as i32,
            top: b[1] as i32,
            right: (b[0] + b[2]) as i32,
            bottom: (b[1] + b[3]) as i32,
        }
    } else if clip.is_some() {
        glyphon::TextBounds {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    } else {
        glyphon::TextBounds {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        }
    }
}

/// Coverage line box (device px). Lucide uses 1em — Tailwind leading would drop the
/// glyph in a size-4 leaf (Select chevron during Dialog zoom capture).
pub(crate) fn coverage_line_height(
    st: &crate::style::TextStyle,
    ui_scale: f32,
    is_input_text: bool,
) -> f32 {
    let is_icon = st.font_family.as_deref() == Some("lucide");
    let scaled_size = st.size_resolved() * ui_scale;
    let mut line_height = if is_icon {
        scaled_size
    } else if let Some(line_height_px) = st.line_height_px {
        line_height_px * ui_scale
    } else {
        let scale = st
            .line_height_scale
            .unwrap_or_else(|| tailwind_default_leading_scale(st.size_resolved()));
        scaled_size * scale
    };
    if is_input_text {
        let min_lh = scaled_size * crate::utils::text_field::text_edit_default_leading_scale();
        line_height = line_height.max(min_lh);
    }
    line_height
}

/// Vertical origin inside a taller nowrap leaf. Cosmic lays out from the buffer top.
pub(crate) fn coverage_nowrap_glyph_dy(
    is_input_text: bool,
    nowrap: bool,
    hard_lines: usize,
    scaled_h: f32,
    line_height: f32,
) -> f32 {
    if !is_input_text && nowrap && hard_lines == 1 && scaled_h > line_height + 1.0 {
        (scaled_h - line_height) * 0.5
    } else {
        0.0
    }
}

fn emoji_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("JADIX_EMOJI_DEBUG")
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
        std::env::var("JADIX_EMOJI_SVG")
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
        std::env::var("JADIX_TEXTAREA_DIAG")
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

fn set_document_rich_text(
    buffer: &mut glyphon::Buffer,
    spans: &[(String, glyphon::Attrs<'_>)],
    attrs: &glyphon::Attrs<'_>,
    shaping: glyphon::Shaping,
) {
    if !spans.iter().any(|(text, _)| text.contains('\r')) {
        buffer.set_rich_text(
            spans
                .iter()
                .map(|(text, attributes)| (text.as_str(), attributes.clone())),
            attrs,
            shaping,
            None,
        );
        return;
    }
    let text: String = spans.iter().map(|(text, _)| text.as_str()).collect();
    buffer.set_text(&text, attrs, shaping, None);
    let mut span_index = 0;
    let mut span_start = 0;
    let mut line_start = 0;
    for line in &mut buffer.lines {
        let line_end = line_start + line.text().len();
        let mut attributes = glyphon::AttrsList::new(attrs);
        while let Some((body, style)) = spans.get(span_index) {
            let span_end = span_start + body.len();
            let start = span_start.max(line_start);
            let end = span_end.min(line_end);
            if start < end {
                attributes.add_span(start - line_start..end - line_start, style);
            }
            if span_end > line_end {
                break;
            }
            span_start = span_end;
            span_index += 1;
        }
        line_start = line_end + line.ending().as_str().len();
        line.set_attrs_list(attributes);
    }
}

/// Isolate an explicit RTL paragraph. LTR/Auto must not inject U+200E: many CJK
/// fallbacks draw LRM as a visible arrow/box at the start of every string.
fn direction_mark(dir: TextDirection) -> &'static str {
    match dir {
        TextDirection::Rtl => "\u{200F}",
        TextDirection::Ltr | TextDirection::Auto => "",
    }
}

/// Recolor glyphs for a shadcn `shimmer` sweep. Positions stay stable (color-only).
pub(crate) fn apply_shimmer_sweep(
    buffer: &mut glyphon::Buffer,
    font_system: &mut glyphon::FontSystem,
    text: &str,
    st: &crate::style::TextStyle,
    family: glyphon::Family,
    font_weight: u16,
    shaping: glyphon::Shaping,
    params: &crate::renderer::ShimmerParams,
) {
    if text.is_empty() {
        return;
    }
    let mut line_w = 1.0f32;
    let mut glyph_u = Vec::new();
    for run in buffer.layout_runs() {
        line_w = line_w.max(run.line_w.max(1.0));
        for g in run.glyphs.iter() {
            if g.w <= 0.0 {
                continue;
            }
            glyph_u.push((g.x + g.w * 0.5) / run.line_w.max(1.0));
        }
    }
    let t = params.progress(crate::style::global_time_s());
    let spread = (params.spread_px_or_default(st.size_resolved()) / line_w).clamp(0.12, 0.85);
    let base = st.color_resolved();
    let highlight = params.highlight(base);
    let attrs = glyphon::Attrs::new()
        .family(family)
        .weight(glyphon::fontdb::Weight(font_weight));
    let n = text.chars().count().max(1);
    let mut owned: Vec<(String, glyphon::Attrs)> = Vec::with_capacity(n);
    for (i, ch) in text.chars().enumerate() {
        let u = glyph_u
            .get(i)
            .copied()
            .unwrap_or((i as f32 + 0.5) / n as f32);
        let mix = crate::renderer::shimmer_band_mix(u, t, spread, params.reverse, params.angle_deg);
        let c = crate::renderer::mix_rgba(base, highlight, mix);
        let mut body = ch.to_string();
        if i == 0 {
            let m = direction_mark(st.direction);
            if !m.is_empty() {
                body = format!("{m}{body}");
            }
        }
        owned.push((
            body,
            attrs
                .clone()
                .color(glyphon::Color::rgba(c[0], c[1], c[2], c[3])),
        ));
    }
    let rich: Vec<(&str, glyphon::Attrs)> =
        owned.iter().map(|(s, a)| (s.as_str(), a.clone())).collect();
    buffer.set_rich_text(rich, &attrs, shaping, None);
    buffer.shape_until_scroll(font_system, false);
}

/// Color selected glyphs via `set_rich_text` (cosmic-text has no public layout_mut).
///
/// `text` must already be the paint string (letter-spacing / direction mark /
/// UAX#14 prewrap applied). Color-only spans stay `compatible()` for shaping, so
/// we must not insert extra marks or per-span spacing — that split BiDi/WJ runs
/// and dropped glyphs.
#[cfg(test)]
fn set_selection_colored_buffer_text(
    buffer: &mut glyphon::Buffer,
    text: &str,
    lo: usize,
    hi: usize,
    fg: [u8; 4],
    attrs: &glyphon::Attrs,
    shaping: glyphon::Shaping,
) {
    set_selection_colored_buffer_ranges(buffer, text, &[((lo, hi), fg)], attrs, shaping);
}

fn set_selection_colored_buffer_ranges(
    buffer: &mut glyphon::Buffer,
    text: &str,
    ranges: &[((usize, usize), [u8; 4])],
    attrs: &glyphon::Attrs,
    shaping: glyphon::Shaping,
) {
    let len = text.len();
    let mut segs: Vec<(usize, usize, [u8; 4])> = ranges
        .iter()
        .filter_map(|&((a, b), fg)| {
            let lo = text.floor_char_boundary(a.min(b).min(len));
            let hi = text.floor_char_boundary(a.max(b).min(len));
            (lo < hi).then_some((lo, hi, fg))
        })
        .collect();
    segs.sort_unstable_by_key(|&(lo, _, _)| lo);
    let mut merged: Vec<(usize, usize, [u8; 4])> = Vec::with_capacity(segs.len());
    for (lo, hi, fg) in segs {
        match merged.last_mut() {
            Some((_, end, last_fg)) if lo <= *end && *last_fg == fg => {
                *end = (*end).max(hi);
            }
            _ => merged.push((lo, hi, fg)),
        }
    }
    if merged.is_empty() {
        buffer.set_text(text, attrs, shaping, None);
        return;
    }
    let mut spans: Vec<(&str, glyphon::Attrs)> = Vec::with_capacity(merged.len() * 2 + 1);
    let mut at = 0usize;
    for (lo, hi, fg) in merged {
        if at < lo {
            spans.push((&text[at..lo], attrs.clone()));
        }
        spans.push((
            &text[lo..hi],
            attrs
                .clone()
                .color(glyphon::Color::rgba(fg[0], fg[1], fg[2], fg[3])),
        ));
        at = hi;
    }
    if at < len {
        spans.push((&text[at..], attrs.clone()));
    }
    buffer.set_rich_text(spans, attrs, shaping, None);
}

/// Map a byte range on `original` onto `paint` when UAX#14 prewrap inserted `\n`.
fn map_original_range_to_paint(
    original: &str,
    paint: &str,
    lo: usize,
    hi: usize,
) -> (usize, usize) {
    if original == paint || paint.is_empty() {
        let n = paint.len();
        return (lo.min(n), hi.min(n));
    }
    let map = |orig_i: usize| -> usize {
        let target = orig_i.min(original.len());
        let ob = original.as_bytes();
        let pb = paint.as_bytes();
        let mut oi = 0usize;
        let mut pi = 0usize;
        while oi < target && pi < pb.len() {
            if ob[oi] == pb[pi] {
                oi += 1;
                pi += 1;
            } else if pb[pi] == b'\n' {
                pi += 1;
            } else {
                oi += 1;
            }
        }
        pi.min(paint.len())
    };
    let mut a = paint.floor_char_boundary(map(lo));
    let mut b = paint.floor_char_boundary(map(hi));
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    (a, b)
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
    selection: &[((usize, usize), [u8; 4])],
) {
    // UAX#14 fit.shape_text / emoji-only chips may already contain EMOJI_LAYOUT_SLOT
    // (U+FFFC). That string has no emoji codepoints, so `hide_emoji_font_glyphs` is often
    // false — still must paint slots invisible or the shaping glyph shows as a
    // horizontal line through Twemoji quads (gallery bubble reactions).
    if text.contains(EMOJI_LAYOUT_SLOT) {
        emoji_layout::set_buffer_with_emoji_slots_colored(
            buffer,
            text,
            &attrs,
            shaping,
            font_size,
            line_height,
            selection,
        );
        return;
    }
    if !hide_emoji_font_glyphs {
        if selection.is_empty() {
            buffer.set_text(text, &attrs, shaping, None);
        } else {
            set_selection_colored_buffer_ranges(buffer, text, selection, &attrs, shaping);
        }
        return;
    }
    if !emoji_detect::text_contains_emoji(text) {
        if selection.is_empty() {
            buffer.set_text(text, &attrs, shaping, None);
        } else {
            set_selection_colored_buffer_ranges(buffer, text, selection, &attrs, shaping);
        }
        return;
    }
    let (layout, _segments) = plan_emoji_layout(text);
    let mapped: Vec<((usize, usize), [u8; 4])> = selection
        .iter()
        .map(|&((lo, hi), fg)| {
            (
                (
                    emoji_layout::map_original_byte_to_emoji_layout(text, lo),
                    emoji_layout::map_original_byte_to_emoji_layout(text, hi),
                ),
                fg,
            )
        })
        .collect();
    emoji_layout::set_buffer_with_emoji_slots_colored(
        buffer,
        &layout,
        &attrs,
        shaping,
        font_size,
        line_height,
        &mapped,
    );
}

/// 与占位字形对齐后的额外垂直微调（仅当设置了 `JADIX_EMOJI_BASELINE_FACTOR` 时非零）。
fn emoji_baseline_layout_tweak(text_size: f32) -> f32 {
    let Some(factor) = std::env::var("JADIX_EMOJI_BASELINE_FACTOR")
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
    let factor = std::env::var("JADIX_EMOJI_BASELINE_FACTOR")
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
    std::env::var("JADIX_EMOJI_SVG_RASTER_MS_THRESHOLD")
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
        let fs = font_loader::create_emoji_font_system();
        Mutex::new((fs, glyphon::SwashCache::new()))
    })
    .lock()
    .unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "tests/isolated_emoji.rs"]
mod isolated_emoji_tests;

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
        let layer = outline.get(i)?;
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
    if font_system.db().is_empty() {
        return None;
    }
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
    text_renderer: &mut JadixTextRenderer,
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
    text_renderer: &mut JadixTextRenderer,
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
    let c = clip?;
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

/// Logical AABB of the decoration quads [`prepare_text_frame`] will emit.
///
/// Positive `text-underline-offset` places the line below the layout box.
/// Callers that scissor to that box (node-present dest-reset) must cover this
/// rect or the line is clipped until a full redraw.
pub(crate) fn text_decoration_ink_logical(n: &TextLayoutInfo, ui_scale: f32) -> Option<[f32; 4]> {
    let under = n.style.underline_resolved();
    let over = n.style.overline_resolved();
    if !under && !over {
        return None;
    }
    let scale = if ui_scale.is_finite() && ui_scale > 0.0 {
        ui_scale
    } else {
        1.0
    };
    let ps = if n.paint_scale.is_finite() && n.paint_scale > 0.0 {
        n.paint_scale
    } else {
        1.0
    };
    let text_size = n.style.size_resolved() * scale;
    let ul_h = if n.style.decoration_thickness_resolved() > 0.0 {
        n.style.decoration_thickness_resolved() * scale
    } else {
        (text_size * 0.07).max(1.0)
    } * ps;
    let offset = n.style.underline_offset_resolved();
    let box_y = n.y * scale;
    let box_h = n.height * scale;
    let mut y0 = box_y;
    let mut y1 = box_y + box_h * ps;
    let extend = |line_y: f32, y0: &mut f32, y1: &mut f32| {
        let (top, bot) = match n.style.decoration_style_resolved() {
            TextDecorationStyle::Double => {
                let gap = (ul_h * 0.8).max(1.0);
                (line_y - gap, line_y + gap + ul_h)
            }
            TextDecorationStyle::Wavy => {
                let amp = (ul_h * 0.75).max(1.0);
                (line_y - amp, line_y + amp + ul_h)
            }
            _ => (line_y, line_y + ul_h),
        };
        *y0 = y0.min(top - 1.0);
        *y1 = y1.max(bot + 1.0);
    };
    if under {
        extend(
            underline_line_y(box_y, box_h, ul_h, offset, scale, ps),
            &mut y0,
            &mut y1,
        );
    }
    if over {
        extend(box_y + offset * scale * ps, &mut y0, &mut y1);
    }
    let h = ((y1 - y0) / scale).max(0.0);
    if n.width <= 0.0 || h <= 0.0 {
        return None;
    }
    Some([n.x, y0 / scale, n.width, h])
}

/// 排版文本、构建下划线 `InstanceInput`，并调用 glyphon `prepare`。
pub fn prepare_text_frame(
    text_renderer: &mut JadixTextRenderer,
    device: &Device,
    queue: &Queue,
    config: &SurfaceConfiguration,
    text_nodes: &[TextLayoutInfo],
    ui_scale: f32,
    force_fast_mode: bool,
    shadow_cached_keys: &std::collections::HashSet<u64>,
    shadow_cached_rects: &std::collections::HashMap<u64, [f32; 4]>,
) -> PrepareTextOutput {
    prepare_text_frame_at(
        text_renderer,
        device,
        queue,
        config.width,
        config.height,
        text_nodes,
        ui_scale,
        force_fast_mode,
        shadow_cached_keys,
        shadow_cached_rects,
    )
}

/// Like [`prepare_text_frame`] for an offscreen RT (`width`×`height` device px).
pub fn prepare_text_frame_at(
    text_renderer: &mut JadixTextRenderer,
    device: &Device,
    queue: &Queue,
    width: u32,
    height: u32,
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
        text_renderer.prepare_resolution(device, queue, width, height, std::iter::empty());
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
    let viewport_clip = [0.0, 0.0, width as f32, height as f32];

    // Compute growth before parking. First mount is not permission to shape the whole
    // document: keep the same bounded near-viewport prefetch used by later frames.
    let initial_preshape = text_renderer.shaped_cache.len() < text_nodes.len();
    // Keep the matching prefix in place. Shrinking clips park their unused tail;
    // assign_shaped_slot parks the remaining suffix at the first identity change.
    park_shaped_suffix(
        &mut text_renderer.shaped_cache,
        &mut text_renderer.shaped_identity_pool,
        text_nodes.len(),
        SHAPED_IDENTITY_POOL_CAP,
    );
    let effective_margin = preshape_margin();
    let effective_budget = preshape_budget();

    let mut preshape_miss_count = 0usize;
    let mut preshape_warmup: Vec<(usize, f32, f32, f32, f32)> = Vec::new();

    let _t0 = web_time::Instant::now();
    let mut _shape_misses = 0usize;
    let mut _emoji_misses = 0usize;
    // Reuse family selection until on-demand coverage changes the database.
    // Do not scan the database again for every cached caption.
    let mut default_family = None;
    let mut explicit_families = std::collections::HashMap::new();
    let mut family_face_count = unsafe { &*text_renderer.font_system }.db().len();

    for (i, info) in text_nodes.iter().enumerate() {
        let st = &info.style;

        // Coordinate scaling + early visibility / margin check.
        // Runs before expensive emoji-detection and font-resolution so that
        // nodes completely outside the pre-shape margin are skipped cheaply.
        let scaled_w = info.width * ui_scale;
        let scaled_h = info.height * ui_scale;
        // Preserve fractional layout origins (rounding can overflow the box).
        // Remove only floating-point noise from device-pixel scroll arithmetic.
        let x = info.x * ui_scale;
        let y = stable_physical_text_y(info.y * ui_scale);
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
                        .unwrap_or_else(|| tailwind_default_leading_scale(st.size_resolved()));
                    st.size_resolved() * scale
                })
                .max(1.0)
                * ui_scale;
            scaled_h
                .max(hard * lh)
                .max(scaled_clip.map(|c| c[3]).unwrap_or(0.0))
        } else {
            scaled_h
        };
        // `info.x` is already `render_x - scroll_x`. Visibility adds `scroll_x`
        // back for caret-pan clips; a tighter dest-reset fragment stays visual
        // so one side of a mixed caption is not deferred a frame.
        let visibility_h = if !is_input_text_early {
            if let Some(c) = scaled_clip {
                // Dest-reset recapture clip is the pill wipe. First-line strut
                // can sit above Latest while wrap lines occupy the hole.
                // Viewport overflow clip is not a wipe — do not inflate to it
                // or off-screen labels start preparing.
                let wipe = c[2] < scaled_w * 0.85 && c[3] < visibility_h * 8.0 + 48.0;
                if wipe {
                    visibility_h.max(((c[1] + c[3]) - y).max(0.0))
                } else {
                    visibility_h
                }
            } else {
                visibility_h
            }
        } else {
            visibility_h
        };
        let scroll_x_px = st.scroll_x.max(0.0) * ui_scale;
        let node_rect = [x + scroll_x_px, y, scaled_w.max(0.0), visibility_h.max(0.0)];
        let visible = glyph_node_visible_this_frame(
            [x, y],
            [scaled_w.max(0.0), visibility_h.max(0.0)],
            scroll_x_px,
            scaled_clip,
            viewport_clip,
        );
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
                width as f32 + 2.0 * margin,
                height as f32 + 2.0 * margin,
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
        let face_count = unsafe { &*text_renderer.font_system }.db().len();
        if face_count != family_face_count {
            default_family = None;
            explicit_families.clear();
            family_face_count = face_count;
        }
        let family = if let Some(f) = &st.font_family {
            *explicit_families.entry(f.as_str()).or_insert_with(|| {
                crate::utils::font::resolve_family(unsafe { &*text_renderer.font_system }.db(), f)
            })
        } else {
            *default_family
                .get_or_insert_with(|| best_font_family(unsafe { &*text_renderer.font_system }))
        };

        let is_icon = st.font_family.as_deref() == Some("lucide");
        let requested_weight = if is_icon { 400 } else { st.weight_resolved() };
        let resolved_face = crate::utils::font::resolve_face_weight(
            unsafe { &*text_renderer.font_system }.db(),
            family,
            requested_weight,
        );
        let font_weight = resolved_face.attrs_weight;
        let scaled_size = st.size_resolved() * ui_scale;
        let is_input_text =
            super::text_prepare_debug::is_input_inline_text(info.template_id.as_deref());
        let line_height = coverage_line_height(st, ui_scale, is_input_text);
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
                    st.size_resolved(),
                    font_weight,
                    logical_lh,
                    st.overflow,
                    st.overflow == TextOverflow::Ellipsis,
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
        // CSS: `line-height` sizes the strut / leaf. Glyph ink may overflow that line
        // box (`overflow: visible`). Cosmic clips at buffer height, so do **not** bind
        // the buffer to the leaf — a `Some(lh + pad)` fudge is not a line box.
        // glyphon TextBounds follows ancestor `overflow:hidden` (sidebar Content host
        // still scissors the last nav row off the footer separator).
        // `Some(content_h)` also races shaped_cache reuse (Textarea hard-line drop).
        let buffer_height_opt: Option<f32> = None;
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
            resolve_text_align(st.align_resolved(), st.direction)
        };
        let display_text: Cow<'_, str> = if st.overflow == TextOverflow::Ellipsis {
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
        let mut shape_hash = node_shape_identity_hash(
            shape_text.as_ref(),
            scaled_size,
            line_height,
            font_weight,
            family,
            align,
            shaping,
            st.letter_spacing,
            st.direction,
            st.color_resolved(),
            requested_weight,
            resolved_face.synthetic_bold,
        );
        shape_hash = rich_shape_identity_hash(shape_hash, info.spans.as_deref());
        {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            shape_hash.hash(&mut hash);
            st.align_last_resolved().hash(&mut hash);
            st.align_resolved().as_str().hash(&mut hash);
            shape_hash = hash.finish();
        }
        // Glyph recolor is baked into Attrs; Exact reuse must miss when the range/color moves.
        if let Some(caret) = st.input_caret.as_ref() {
            let ranges = caret.selection_recolor_ranges();
            if !ranges.is_empty() {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                shape_hash.hash(&mut h);
                ranges.hash(&mut h);
                shape_hash = h.finish();
            }
        }
        // Derive Exact key from identity — do not re-hash the full string (scroll frames
        // visit every visible label; browsers keep glyphs and only translate).
        let key_hash = node_shape_key_from_identity(shape_hash, scaled_w, scaled_h, false);

        assign_shaped_slot(
            &mut text_renderer.shaped_cache,
            &mut text_renderer.shaped_identity_pool,
            unsafe { &mut *text_renderer.font_system },
            i,
            shape_hash,
        );

        if preshape_only {
            let will_miss = text_renderer.shaped_cache[i].key_hash != key_hash;
            if will_miss {
                preshape_miss_count += 1;
                if preshape_miss_count > effective_budget {
                    frame_meta.push(None);
                    continue;
                }
            }
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
            if nowrap {
                return soft_w_target;
            }
            // Prewrap hard-breaks are sized for the leaf; do not shrink buffer below leaf
            // (that reflows the last word on settle).
            if crate::text::needs_logical_prewrap(info.text.as_str()) {
                return soft_w_target;
            }
            // Keep live resize on the same bounded width bucket as measurement, regardless
            // of FIT_CACHE warmth, so a few subpixel changes cannot alternate wrapped lines.
            if force_fast_mode {
                if crate::text::layout_text_fast_mode() {
                    return (crate::text::layout_wrap_target(layout_w, true) * ui_scale).max(1.0);
                }
                return soft_w_target;
            }
            if info.text.is_ascii() || info.prepared.is_none() {
                return soft_w_target;
            }
            if let Some(fit) = crate::text::try_glyphon_fit_soft_wrap(
                info.text.as_str(),
                st.size_resolved(),
                font_weight,
                logical_lh,
                fit_lookup_w,
            ) {
                return (fit.wrap_width * ui_scale).max(1.0);
            }
            let fit = crate::text::glyphon_fit_soft_wrap(
                info.text.as_str(),
                st.size_resolved(),
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
                        st.size_resolved(),
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
                                st.size_resolved(),
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
                                st,
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
                            st.size_resolved(),
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
                                st.size_resolved(),
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
                                st,
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
                        st.size_resolved(),
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
                        st.size_resolved(),
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
                        st,
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
                            st.size_resolved(),
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
                            st,
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
                        st.size_resolved(),
                        font_weight,
                        logical_lh,
                        prewrap_target,
                    )
                    .unwrap_or_else(|| {
                        crate::text::glyphon_fit_soft_wrap(
                            info.text.as_str(),
                            st.size_resolved(),
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
                        st.color_resolved()[0],
                        st.color_resolved()[1],
                        st.color_resolved()[2],
                        st.color_resolved()[3],
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
                            let lh = line_height * (sz / st.size_resolved().max(1.0));
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
                    set_document_rich_text(&mut buffer, &owned, &attrs, shaping);
                } else {
                    let paint_text = prewrapped.as_deref().unwrap_or_else(|| shape_text.as_ref());
                    let mapped: Vec<((usize, usize), [u8; 4])> = st
                        .input_caret
                        .as_ref()
                        .map(|c| {
                            c.selection_recolor_ranges()
                                .into_iter()
                                .map(|((lo, hi), fg)| {
                                    let (slo, shi) = map_original_range_to_paint(
                                        info.text.as_str(),
                                        paint_text,
                                        lo,
                                        hi,
                                    );
                                    ((slo, shi), fg)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    set_shaped_buffer_text(
                        &mut buffer,
                        unsafe { &mut *text_renderer.font_system },
                        paint_text,
                        attrs,
                        shaping,
                        emoji_custom_glyph_enabled() && has_emoji,
                        scaled_size,
                        line_height,
                        &mapped,
                    );
                }
                let prewrapped_source = prewrapped
                    .as_ref()
                    .filter(|_| info.spans.as_ref().is_none_or(|spans| spans.is_empty()))
                    .map(|_| info.text.as_str());
                set_buffer_text_alignment(
                    &mut buffer,
                    prewrapped_source,
                    align,
                    st,
                    !is_input_text,
                );
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
                    st.size_resolved(),
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
                    st,
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
        let dy =
            coverage_nowrap_glyph_dy(is_input_text, nowrap, hard_lines_n, scaled_h, line_height);
        let mut glyph_color = st.color_resolved();
        let mut glyph_dy = dy;
        if let Some(raw) = info.glyph_fx.as_deref() {
            let kind = crate::renderer::GlyphFxKind::parse(raw);
            if kind == crate::renderer::GlyphFxKind::Shimmer {
                if let Some(params) = info.shimmer.as_ref() {
                    apply_shimmer_sweep(
                        &mut text_renderer.shaped_cache[i].buffer,
                        unsafe { &mut *text_renderer.font_system },
                        info.text.as_str(),
                        st,
                        family,
                        font_weight,
                        shaping,
                        params,
                    );
                }
            } else if kind != crate::renderer::GlyphFxKind::None {
                let fx = crate::renderer::GlyphFx::new(kind, crate::style::global_time_s().fract());
                let n = info.text.chars().count().max(1);
                glyph_color[3] = ((glyph_color[3] as f32) * fx.opacity(0, n))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let (_, off_y) = fx.offset(0, n);
                glyph_dy += off_y * ui_scale;
            }
        }
        let sdx0 = st.shadow_offset[0] * ui_scale;
        let sdy0 = st.shadow_offset[1] * ui_scale;
        let sblur0 = st.shadow_blur * ui_scale;
        let mut sdx = sdx0;
        let mut sdy = sdy0;
        let mut sblur = sblur0;
        let mut shadow_col = st.shadow_color;
        let user_text_shadow = text_shadow_active(shadow_col, [sdx0, sdy0], sblur0);
        // CSS `font-synthesis: weight` — thicken Regular when Medium/Bold face is missing.
        // Reuses the existing 0-blur/soft shadow rasterizer; skipped if the node already
        // specified a text-shadow.
        if resolved_face.synthetic_bold && !is_icon && !user_text_shadow {
            let (c, off, blur) =
                crate::utils::font::synthetic_bold_shadow(st.color_resolved(), ui_scale);
            shadow_col = Some(c);
            sdx = off[0];
            sdy = off[1];
            sblur = blur;
        }
        let has_shadow = text_shadow_active(shadow_col, [sdx, sdy], sblur);

        if has_shadow {
            if let Some(sc) = shadow_col {
                if uses_textured_shadow_bitmap(user_text_shadow, sblur) {
                    let intersected = shadow_clip_for_glyphon(
                        scaled_clip,
                        sdx.abs() + sdy.abs() + sblur + 3.0,
                        viewport_clip,
                    );
                    if let Some(shadow_clip) =
                        paint_overflow_clip(intersected, scaled_clip.is_some())
                    {
                        let skey = shadow_cache_key(key_hash, sc, sdx, sdy, sblur);
                        let mut drew_tex = false;
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
                                    clip: shadow_clip,
                                });
                                drew_tex = true;
                            }
                        } else if let Some(bmp) = generate_shadow_bitmap(
                            &text_renderer.shaped_cache[i].buffer,
                            unsafe { &mut *text_renderer.font_system },
                            &mut text_renderer.swash_cache.borrow_mut(),
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
                                clip: shadow_clip,
                            });
                            shadow_new_bitmaps.push(ShadowNewBitmap {
                                cache_key: skey,
                                rgba: bmp.rgba,
                                width: bmp.width,
                                height: bmp.height,
                                draw_rect: bmp.draw_rect,
                            });
                            drew_tex = true;
                        }
                        // Only skip the glyphon fallback when a textured quad was actually queued.
                        // Cache-key-without-rect or empty-glyph bitmap miss used to skip *and*
                        // omit a draw → synthetic bold vanished until the next reshape (scroll flicker).
                        if drew_tex {
                            shadow_skip_indices.insert(i);
                        }
                        // Bitmap miss → glyphon multi-layer TextArea fallback below (Coverage path).
                    }
                }
            }
        }

        frame_meta.push(Some(FrameTextNodeMeta {
            x,
            y,
            box_w: scaled_w,
            box_h: scaled_h,
            dy: glyph_dy * info.paint_scale,
            text_size: scaled_size,
            paint_scale: if info.paint_scale.is_finite() && info.paint_scale > 0.0 {
                info.paint_scale
            } else {
                1.0
            },
            align,
            measured_text_w,
            underline: st.underline_resolved(),
            overline: st.overline_resolved(),
            line_through: st.line_through_resolved(),
            decoration_style: st.decoration_style_resolved(),
            decoration_color: st.decoration_color,
            decoration_thickness: st.decoration_thickness_resolved(),
            underline_offset: st.underline_offset_resolved(),
            color: glyph_color,
            shadow_col,
            shadow_dx: sdx,
            shadow_dy: sdy,
            shadow_blur: sblur,
            clip_bounds: scaled_clip,
            emoji_cache_key,
        }));
    }

    let _t1 = web_time::Instant::now();

    let mut underline_instances: Vec<InstanceInput> = Vec::new();
    let caret_instances: Vec<InstanceInput> = Vec::new();

    // Chip/Mention backgrounds (`jadix_chip` Attrs.metadata = span_index+1). Drawn under glyphs.
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
                        backdrop_fade: 0.0,
                        transform: crate::style::transform::MAT2D_IDENTITY,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: 0.0,
                        backdrop_blur: 0.0,
                        glass: [0.0; 4],
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
        if paint_overflow_clip(tb.clip_bounds, tb.clip_bounds.is_some()).is_none() {
            continue;
        }
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
                    let shadow_bounds_clip = shadow_clip_for_glyphon(
                        tb.clip_bounds,
                        radius + tb.shadow_dx.abs() + tb.shadow_dy.abs() + 3.0,
                        viewport_clip,
                    );
                    if let Some(gpu_clip) =
                        paint_overflow_clip(shadow_bounds_clip, tb.clip_bounds.is_some())
                    {
                        let bounds = text_bounds_from_clip(gpu_clip, width, height);

                        let mut push_shadow_layer = |left: f32, top: f32, layer_alpha: u8| {
                            if layer_alpha == 0 {
                                return;
                            }
                            let shadow_rect = [left, top, tb.box_w, tb.box_h];
                            let Some(shadow_visible) =
                                clip_rect_to_bounds(shadow_rect, tb.clip_bounds)
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
                                default_color: glyphon::Color::rgba(
                                    sc[0],
                                    sc[1],
                                    sc[2],
                                    layer_alpha,
                                ),
                                custom_glyphs: tb
                                    .emoji_cache_key
                                    .as_ref()
                                    .map_or(&[], |k| text_renderer.emoji_cache.glyphs_for(k)),
                            });
                        };

                        let draw_blur = radius > TEXTURED_SHADOW_MIN_BLUR
                            && radius > min_radius
                            && alpha > min_alpha;
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
                            let center_alpha =
                                ((alpha as f32) * inv).round().clamp(0.0, 255.0) as u8;
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
        }

        let bounds = text_bounds_from_clip(tb.clip_bounds, width, height);
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
                    target: "jadix::textarea_diag",
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
                        backdrop_fade: 0.0,
                        transform: crate::style::transform::MAT2D_IDENTITY,
                        clip_pos: [0.0, 0.0],
                        clip_size: InstanceInput::NO_CLIP_SIZE,
                        border_extra: 0.0,
                        backdrop_blur: 0.0,
                        glass: [0.0; 4],
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
                                backdrop_fade: 0.0,
                                transform: crate::style::transform::MAT2D_IDENTITY,
                                clip_pos: [0.0, 0.0],
                                clip_size: InstanceInput::NO_CLIP_SIZE,
                                border_extra: 0.0,
                                backdrop_blur: 0.0,
                                glass: [0.0; 4],
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
                    backdrop_fade: 0.0,
                    transform: crate::style::transform::MAT2D_IDENTITY,
                    clip_pos: [0.0, 0.0],
                    clip_size: InstanceInput::NO_CLIP_SIZE,
                    border_extra: 0.0,
                    backdrop_blur: 0.0,
                    glass: [0.0; 4],
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

    let _t2 = web_time::Instant::now();
    // Keep viewport in sync on resize even when every node is culled.
    text_renderer
        .viewport
        .update(queue, glyphon::Resolution { width, height });

    let text_area_count = text_areas.len();
    if text_area_count == 0 {
        // A nonempty input can produce no visible glyphs after scrolling. Leaving
        // the previous GPU batch intact replays pre-scroll Message captions (or
        // subtree-capture-local glyphs) into unrelated node-present scissors.
        // Empty prepare clears draw vertices, not the font/shaping/atlas caches.
        text_renderer.prepare_resolution(device, queue, width, height, std::iter::empty());
        return PrepareTextOutput {
            underline_instances,
            caret_instances,
            shadow_new_bitmaps,
            shadow_tex_draws,
            needs_warmup: !text_renderer.pending_warmup.is_empty(),
            culled_skip_prepare: true,
        };
    }

    let _warmup_count = preshape_warmup.len();
    if !preshape_warmup.is_empty() {
        let mut warmup_areas: Vec<glyphon::TextArea> = Vec::with_capacity(preshape_warmup.len());
        for &(idx, _px, _py, pw, ph) in &preshape_warmup {
            let entry = &text_renderer.shaped_cache[idx];
            let bw = pw.ceil().min(width as f32) as i32;
            let bh = ph.ceil().min(height as f32) as i32;
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
            &mut text_renderer.swash_cache.borrow_mut(),
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
        &mut text_renderer.swash_cache.borrow_mut(),
        |input| text_renderer.emoji_cache.rasterize(input),
    ) {
        log::error!("glyphon prepare failed: {err:?}");
    }
    emoji_metrics_log_tick(text_renderer.emoji_cache.stats);
    text_renderer.text_area_capacity_hint = text_area_count;

    let _t3 = web_time::Instant::now();
    let _total_ms = (_t3 - _t0).as_secs_f64() * 1000.0;
    if _total_ms > 4.0 || (cfg!(test) && std::env::var_os("JADIX_PERF_TEXT_DETAIL").is_some()) {
        log::warn!(
            "prepare_text_frame {:.1}ms | loop={:.1}ms areas={:.1}ms gpu={:.1}ms | nodes={} shape_miss={} emoji_miss={} areas={} warmup={} preshape={} pool={}",
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
            identity_pool_len(&text_renderer.shaped_identity_pool),
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
#[path = "tests/coverage_placement.rs"]
mod coverage_placement_tests;

#[cfg(test)]
#[path = "tests/text_shadow.rs"]
mod text_shadow_tests;

#[cfg(test)]
#[path = "tests/shimmer_sweep.rs"]
mod shimmer_sweep_tests;

#[cfg(test)]
#[path = "tests/shape_cache_key.rs"]
mod shape_cache_key_tests;

#[cfg(test)]
#[path = "tests/hidden_scroll_cull.rs"]
mod hidden_scroll_cull_tests;

#[cfg(test)]
#[path = "tests/input_shaping_stability.rs"]
mod input_shaping_stability_tests;

#[cfg(test)]
#[path = "tests/glyph_fragment_visibility.rs"]
mod glyph_fragment_visibility_tests;

#[cfg(test)]
include!("tests/prepare_text_cases.rs");
