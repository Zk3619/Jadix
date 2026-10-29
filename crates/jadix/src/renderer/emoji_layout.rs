//! 从已布局的 glyphon [`Buffer`] 推导自定义 emoji 贴图与占位字形对齐的 `CustomGlyph` 坐标。
//!
//! # Ink / bearing / advance（三数模型）
//!
//! 系统 UI（Segoe / Chrome）观感不是「1em 槽塞满」也不是「1.15 墨水溢出 1em 槽」：
//!
//! ```text
//! |←──────────── advance ≈ 1.22em ────────────→|
//! | bearing | ←──── ink ≈ 1.10em ────→ | bearing |
//! ```
//!
//! - **ink**：trim 后墨水缩放到的视觉尺寸（`CustomGlyph` / 光栅）。
//! - **bearing**：墨水与邻字/光标的正留白。
//! - **advance**：布局/光标/选区宽度 = ink + 2×bearing（由公式导出，勿单独手改）。
//!
//! 水平方向墨水绝不越过 advance；垂直方向允许相对行盒轻微溢出（光学居中）。

use glyphon::cosmic_text::LineIter;
use unicode_segmentation::UnicodeSegmentation;

use super::emoji_detect;

/// Object replacement character (U+FFFC), reserved for embedded emoji objects.
///
/// Must not be a `Zs` space (old EM SPACE U+2003): cosmic-text strips leading whitespace after
/// soft-wrap, which dropped the first emoji on each wrapped line and caused stair-step quads.
/// Using an ordinary ideograph here made literal `一` in labels such as `上一页`
/// indistinguishable from a hidden emoji slot, including mixed text/emoji runs.
/// Paint keeps the object placeholder transparent (`set_shaped_buffer_text`).
///
/// Slot run uses [`emoji_slot_metrics`] so shaped advance ≈ [`EMOJI_ADVANCE_EM`]×font_size.
pub const EMOJI_LAYOUT_SLOT: char = '\u{FFFC}';

// Shape the object with the established 1em CJK fallback, but identify it by
// span metadata. Both codepoints occupy three UTF-8 bytes, so caret/emoji maps
// keep their offsets. A literal 一 never receives the object metadata.
const EMOJI_SHAPING_GLYPH: char = '一';
pub(crate) const EMOJI_SLOT_METADATA: usize = usize::MAX;

/// Trim 后墨水视觉尺寸（相对正文 `font_size`）。
pub const EMOJI_INK_EM: f32 = 1.10;
/// 每侧呼吸感（相对正文 `font_size`）；光标/邻字与墨水的间隙。
pub const EMOJI_BEARING_EM: f32 = 0.06;
/// 布局 advance = ink + 2×bearing。勿单独改此常数——由上两项导出。
pub const EMOJI_ADVANCE_EM: f32 = EMOJI_INK_EM + 2.0 * EMOJI_BEARING_EM;

/// `(ink_px, advance_px, bearing_px)` in logical/physical font units for `font_size`.
#[inline]
pub fn emoji_metrics(font_size: f32) -> (f32, f32, f32) {
    let fs = font_size.max(0.001);
    (
        fs * EMOJI_INK_EM,
        fs * EMOJI_ADVANCE_EM,
        fs * EMOJI_BEARING_EM,
    )
}

/// Paint-box size for an overflow-visible emoji leaf: at least the 1.10em ink
/// square, even when Taffy/leading-none sized the strut to 1em.
///
/// CSS `overflow: visible` lets ink paint past the line box. Collect dest, Coverage
/// clip, and visual AABB must use this box — otherwise `hover:scale-125` enlarges
/// a 1em AABB crop of the circular Twemoji (screenshot chords).
#[inline]
pub fn emoji_visible_paint_size(layout_w: f32, layout_h: f32, font_size: f32) -> (f32, f32) {
    let ink = emoji_metrics(font_size).0;
    (layout_w.max(ink).max(1.0), layout_h.max(ink).max(1.0))
}

/// Grow `layout_bounds` on overflow-visible axes so visual AABB / scissor cover ink.
#[inline]
pub fn layout_bounds_with_visible_emoji_ink(
    bounds: [f32; 4],
    text: Option<&str>,
    font_size: f32,
    overflow_x_visible: bool,
    overflow_y_visible: bool,
) -> [f32; 4] {
    let Some(text) = text else {
        return bounds;
    };
    if !emoji_detect::text_contains_emoji(text) {
        return bounds;
    }
    let ink = emoji_metrics(font_size).0;
    let mut b = bounds;
    if overflow_x_visible && b[2] + 1e-3 < ink {
        let extra = ink - b[2];
        b[0] -= extra * 0.5;
        b[2] = ink;
    }
    if overflow_y_visible && b[3] + 1e-3 < ink {
        let extra = ink - b[3];
        b[1] -= extra * 0.5;
        b[3] = ink;
    }
    b
}

/// Raster / `CustomGlyph` edge length — must match glyphon's request (= round(ink × scale)).
#[inline]
pub fn emoji_ink_pixel_size(font_size: f32) -> u16 {
    (font_size.max(0.001) * EMOJI_INK_EM)
        .round()
        .clamp(1.0, 512.0) as u16
}

/// Metrics for an invisible [`EMOJI_LAYOUT_SLOT`] run: scale the object placeholder so it
/// advances ≈ [`EMOJI_ADVANCE_EM`]×`font_size`, while keeping the paragraph `line_height`.
#[inline]
pub fn emoji_slot_metrics(font_size: f32, line_height: f32) -> glyphon::Metrics {
    glyphon::Metrics::new(
        (font_size.max(0.001) * EMOJI_ADVANCE_EM).max(1.0),
        line_height.max(1.0),
    )
}

/// Invisible attrs for a layout slot: alpha=0 + [`emoji_slot_metrics`].
pub fn emoji_slot_attrs<'a>(
    base: &'a glyphon::Attrs<'a>,
    font_size: f32,
    line_height: f32,
) -> glyphon::Attrs<'a> {
    base.clone()
        .color(glyphon::Color::rgba(0, 0, 0, 0))
        .metadata(EMOJI_SLOT_METADATA)
        .metrics(emoji_slot_metrics(font_size, line_height))
}

/// True when this layout glyph is an embedded emoji placeholder. Paint skips it
/// because the custom emoji quad supplies the visible ink.
pub fn glyph_is_emoji_layout_slot(
    _run: &glyphon::LayoutRun<'_>,
    glyph: &glyphon::LayoutGlyph,
) -> bool {
    glyph.metadata == EMOJI_SLOT_METADATA
}

/// Replace each emoji grapheme with [`EMOJI_LAYOUT_SLOT`] so layout advance ≈ custom quad.
pub fn plan_emoji_layout_text(text: &str) -> String {
    let mut layout = String::with_capacity(text.len());
    for grapheme in text.graphemes(true) {
        if emoji_detect::grapheme_is_emoji(grapheme) {
            layout.push(EMOJI_LAYOUT_SLOT);
        } else {
            layout.push_str(grapheme);
        }
    }
    layout
}

/// Whether paint replaces emoji with fixed-width slots + optically centered color quads.
///
/// **Always on** for every emoji run (mixed prose and chip leaves alike): ink is
/// centered in the glyph advance box the same way regular glyphs sit in theirs.
/// Raster *source* (system color font vs Twemoji/SVG) is separate — see
/// [`super::emoji_config::emoji_bitmap_override_active`].
pub fn custom_glyph_layout_enabled() -> bool {
    true
}

/// Text to shape for layout/measure when custom emoji quads are enabled.
pub fn shape_text_for_layout(text: &str) -> std::borrow::Cow<'_, str> {
    if custom_glyph_layout_enabled() && emoji_detect::text_contains_emoji(text) {
        std::borrow::Cow::Owned(plan_emoji_layout_text(text))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// Shape `layout` (may contain [`EMOJI_LAYOUT_SLOT`]) with ADVANCE_EM metrics on slots.
///
/// Used by paint + measure so caret / wrap / ink share one advance.
pub fn set_buffer_with_emoji_slots(
    buffer: &mut glyphon::Buffer,
    layout: &str,
    attrs: &glyphon::Attrs,
    shaping: glyphon::Shaping,
    font_size: f32,
    line_height: f32,
) {
    set_buffer_with_emoji_slots_colored(
        buffer,
        layout,
        attrs,
        shaping,
        font_size,
        line_height,
        &[],
    );
}

fn selection_fg_at(byte: usize, ranges: &[((usize, usize), [u8; 4])]) -> Option<[u8; 4]> {
    ranges.iter().find_map(|&((a, b), fg)| {
        let lo = a.min(b);
        let hi = a.max(b);
        (byte >= lo && byte < hi).then_some(fg)
    })
}

/// Like [`set_buffer_with_emoji_slots`], plus `::selection` color on non-slot runs.
///
/// Slot glyphs stay invisible (custom emoji quads paint on top). Ranges are in
/// **layout** UTF-8 bytes (after emoji → [`EMOJI_LAYOUT_SLOT`]).
pub fn set_buffer_with_emoji_slots_colored(
    buffer: &mut glyphon::Buffer,
    layout: &str,
    attrs: &glyphon::Attrs,
    shaping: glyphon::Shaping,
    font_size: f32,
    line_height: f32,
    ranges: &[((usize, usize), [u8; 4])],
) {
    if !layout.contains(EMOJI_LAYOUT_SLOT) && ranges.is_empty() {
        buffer.set_text(layout, attrs, shaping, None);
        return;
    }
    let visible = glyphon::AttrsOwned::new(attrs);
    let invisible = glyphon::AttrsOwned::new(&emoji_slot_attrs(attrs, font_size, line_height));
    let mut owned: Vec<(String, glyphon::AttrsOwned)> = Vec::new();
    let mut buf = String::new();
    let mut run_slot = false;
    let mut run_fg: Option<[u8; 4]> = None;
    let flush = |owned: &mut Vec<(String, glyphon::AttrsOwned)>,
                 buf: &mut String,
                 run_slot: bool,
                 run_fg: Option<[u8; 4]>| {
        if buf.is_empty() {
            return;
        }
        let a = if run_slot {
            invisible.clone()
        } else if let Some(fg) = run_fg {
            glyphon::AttrsOwned::new(
                &attrs
                    .clone()
                    .color(glyphon::Color::rgba(fg[0], fg[1], fg[2], fg[3])),
            )
        } else {
            visible.clone()
        };
        owned.push((std::mem::take(buf), a));
    };
    let mut byte = 0usize;
    for c in layout.chars() {
        let is_slot = c == EMOJI_LAYOUT_SLOT;
        let fg = if is_slot {
            None
        } else {
            selection_fg_at(byte, ranges)
        };
        if !buf.is_empty() && (is_slot != run_slot || fg != run_fg) {
            flush(&mut owned, &mut buf, run_slot, run_fg);
        }
        run_slot = is_slot;
        run_fg = fg;
        buf.push(if is_slot { EMOJI_SHAPING_GLYPH } else { c });
        byte += c.len_utf8();
    }
    flush(&mut owned, &mut buf, run_slot, run_fg);
    if owned.is_empty() {
        buffer.set_text(layout, attrs, shaping, None);
        return;
    }
    let rich: Vec<(&str, glyphon::Attrs)> = owned
        .iter()
        .map(|(t, a)| (t.as_str(), a.as_attrs()))
        .collect();
    // Default must be the invisible slot attrs. cosmic-text `set_rich_text`
    // drops spans that equal defaults — if the default were the opaque body
    // color, a dropped slot span would paint the placeholder glyph
    // (reaction chips are emoji-only, so the whole line is one slot run).
    let slot_defaults = invisible.as_attrs();
    buffer.set_rich_text(rich, &slot_defaults, shaping, None);
}

/// Original UTF-8 byte → layout byte after each emoji grapheme becomes [`EMOJI_LAYOUT_SLOT`].
pub fn map_original_byte_to_emoji_layout(original: &str, orig_byte: usize) -> usize {
    let target = orig_byte.min(original.len());
    if !emoji_detect::text_contains_emoji(original) {
        return target;
    }
    let mut oi = 0usize;
    let mut li = 0usize;
    for g in original.graphemes(true) {
        if oi >= target {
            return li;
        }
        let next_oi = oi + g.len();
        let layout_len = if emoji_detect::grapheme_is_emoji(g) {
            EMOJI_LAYOUT_SLOT.len_utf8()
        } else {
            g.len()
        };
        if target < next_oi {
            if emoji_detect::grapheme_is_emoji(g) {
                return if target > oi { li + layout_len } else { li };
            }
            return li + (target - oi);
        }
        oi = next_oi;
        li += layout_len;
    }
    li
}

/// `shape_text` 内 UTF-8 字节下标 →（buffer 行号, 该行内字节下标）。
pub fn global_byte_to_line_local(shape_text: &str, global_byte: usize) -> Option<(usize, usize)> {
    if global_byte > shape_text.len() {
        return None;
    }
    for (line_i, (range, _)) in LineIter::new(shape_text).enumerate() {
        if global_byte >= range.start && global_byte < range.end {
            return Some((line_i, global_byte - range.start));
        }
    }
    None
}

/// 返回覆盖 `local_byte` 的第一个 [`glyphon::LayoutGlyph`] 及其所在 run。
pub fn find_glyph_for_line_byte<'a>(
    buffer: &'a glyphon::Buffer,
    line_i: usize,
    local_byte: usize,
) -> Option<(glyphon::LayoutRun<'a>, &'a glyphon::LayoutGlyph)> {
    for run in buffer.layout_runs() {
        if run.line_i != line_i {
            continue;
        }
        for g in run.glyphs.iter() {
            if local_byte >= g.start && local_byte < g.end {
                return Some((run, g));
            }
        }
    }
    None
}

/// 计算与占位彩色字形 quad 对齐的 `CustomGlyph.left` / `CustomGlyph.top`（逻辑单位）。
///
/// 与 glyphon 对普通字形的 `prepare_glyph` 位置公式一致：`round(line_y*scale) + physical.y - placement.top`。
///
/// 注意：当占位符是 UI-sans 的 Mask「豆腐」而贴图是满 em 方块时，用 Mask 的 `placement.top`
/// 会把方块压到行盒底部并被 `TextBounds` 裁切。优先用 [`custom_emoji_left_centered_top`]。
pub fn custom_emoji_left_top(
    buffer: &glyphon::Buffer,
    shape_text: &str,
    global_byte: usize,
    text_area_left: f32,
    text_area_top: f32,
    scale: f32,
    font_system: &mut glyphon::FontSystem,
    swash_cache: &mut glyphon::SwashCache,
) -> Option<(f32, f32)> {
    let scale = scale.max(0.001);
    let (line_i, local) = global_byte_to_line_local(shape_text, global_byte)?;
    let (run, g) = find_glyph_for_line_byte(buffer, line_i, local)?;
    let physical = g.physical((text_area_left, text_area_top), scale);
    let image = swash_cache.get_image_uncached(font_system, physical.cache_key)?;
    let x_quad = physical.x + image.placement.left;
    let y_quad = (run.line_y * scale).round() as i32 + physical.y - image.placement.top;
    let left = (x_quad as f32 - text_area_left) / scale;
    let top = (y_quad as f32 - text_area_top) / scale;
    Some((left, top))
}

/// 将 `ink_w`×`ink_h` 墨水方块水平居中于占位字形盒，垂直光学居中于行盒。
///
/// - 水平：以 `max(g.w, designed advance)` 为字形盒居中。切勿把 `ink` 钳进 `g.w`
///   再贴左——槽宽一旦略小于墨水，溢出会全部堆在右侧，胶囊看起来左边距更窄。
/// - 垂直：允许相对 `line_height` 轻微溢出（系统 emoji 亦非坐死基线）。
pub fn custom_emoji_left_centered_top(
    buffer: &glyphon::Buffer,
    layout_text: &str,
    global_byte: usize,
    ink_w: f32,
    ink_h: f32,
) -> Option<(f32, f32)> {
    let (line_i, local) = global_byte_to_line_local(layout_text, global_byte)?;
    let (run, g) = find_glyph_for_line_byte(buffer, line_i, local)?;
    let ink_w = ink_w.max(0.0);
    let eh = ink_h.max(0.0);
    // Rounded paint size can be 1px off `font_size * INK_EM`; recover the 1.22em cell
    // so a collapsed slot (leading-none, span metrics ignored) still centers.
    let designed = ink_w * (EMOJI_ADVANCE_EM / EMOJI_INK_EM);
    let cell_w = g.w.max(designed);
    let left = g.x + (cell_w - ink_w) * 0.5;
    let top = run.line_top + (run.line_height - eh) * 0.5;
    Some((left, top))
}

/// Same-line backstop when a slot advance collapses; soft-wrap must reset the cursor
/// so the next emoji returns to the left margin (no stair-step overflow).
pub fn advance_emoji_left_cursor(
    left: f32,
    top: f32,
    slot: f32,
    cursor: &mut f32,
    line_top: &mut Option<f32>,
) -> f32 {
    if line_top.map(|t| (top - t).abs() > 0.5).unwrap_or(false) {
        // First glyph on a wrapped line may sit slightly left of 0 when ink > slot
        // (symmetric overflow). Resetting to 0 packed that overflow onto the right.
        *cursor = f32::NEG_INFINITY;
    }
    *line_top = Some(top);
    let mut left = left;
    if left + 0.5 < *cursor {
        left = *cursor;
    }
    *cursor = left + slot * 0.95;
    left
}

#[cfg(test)]
#[path = "tests/emoji_layout.rs"]
mod tests;
