//! 从已布局的 glyphon [`Buffer`] 推导自定义 emoji 贴图与占位字形对齐的 `CustomGlyph` 坐标。

use glyphon::cosmic_text::LineIter;
use unicode_segmentation::UnicodeSegmentation;

use super::emoji_detect;

/// Ideograph `一` (U+4E00) — ~1em advance via CJK fallback, **not** Unicode whitespace.
///
/// Must not be a `Zs` space (old EM SPACE U+2003): cosmic-text strips leading whitespace after
/// soft-wrap, which dropped the first emoji on each wrapped line and caused stair-step quads.
/// Fullwidth ASCII (U+FF03) is too narrow in Segoe UI (~0.65em); CJK keeps the em-square slot.
///
/// Paint path must color this slot with alpha=0 (`set_shaped_buffer_text`) — otherwise the
/// ideograph stroke shows as a black line through every Twemoji quad.
pub const EMOJI_LAYOUT_SLOT: char = '\u{4E00}';

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

/// Whether paint replaces emoji with fixed-width slots + color quads (default on).
pub fn custom_glyph_layout_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RUI_EMOJI_CUSTOM_GLYPH")
            .map(|v| {
                !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
            })
            .unwrap_or(true)
    })
}

/// Text to shape for layout/measure when custom emoji quads are enabled.
pub fn shape_text_for_layout(text: &str) -> std::borrow::Cow<'_, str> {
    if custom_glyph_layout_enabled() && emoji_detect::text_contains_emoji(text) {
        std::borrow::Cow::Owned(plan_emoji_layout_text(text))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
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

/// 水平对齐占位 glyph 的中心，垂直在行盒内居中放置 `emoji_width`×`emoji_height` 的方块。
///
/// 不依赖 Mask tofu 的 `placement`（避免底切），也不要求 swash 光栅。
/// `emoji_width` 可略大于占位 advance（例如 1.15em），以贴近系统 emoji 视觉大小。
pub fn custom_emoji_left_centered_top(
    buffer: &glyphon::Buffer,
    layout_text: &str,
    global_byte: usize,
    emoji_width: f32,
    emoji_height: f32,
) -> Option<(f32, f32)> {
    let (line_i, local) = global_byte_to_line_local(layout_text, global_byte)?;
    let (run, g) = find_glyph_for_line_byte(buffer, line_i, local)?;
    let left = g.x + (g.w - emoji_width) * 0.5;
    let pad = (run.line_height - emoji_height).max(0.0) * 0.5;
    let top = run.line_top + pad;
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
        *cursor = 0.0;
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
mod tests {
    use super::*;

    #[test]
    fn newline_byte_maps_to_second_line() {
        let s = "a\n😀";
        let idx = s.find('😀').unwrap();
        let (li, loc) = global_byte_to_line_local(s, idx).unwrap();
        assert_eq!(li, 1);
        assert_eq!(loc, 0);
    }

    #[test]
    fn plan_collapses_family_zwj_to_one_slot() {
        let raw = "A👨‍👩‍👧‍👦B";
        let layout = plan_emoji_layout_text(raw);
        assert_eq!(layout.chars().count(), 3);
        assert!(layout.contains(EMOJI_LAYOUT_SLOT));
        assert!(!layout.contains('\u{200d}'));
    }

    #[test]
    fn left_cursor_resets_on_soft_wrap_line() {
        let slot = 16.0_f32;
        let mut cursor = 0.0_f32;
        let mut line_top = None;
        let l0 = advance_emoji_left_cursor(100.0, 0.0, slot, &mut cursor, &mut line_top);
        assert!((l0 - 100.0).abs() < f32::EPSILON);
        // Same line: collapsed left is pushed forward.
        let l1 = advance_emoji_left_cursor(10.0, 0.0, slot, &mut cursor, &mut line_top);
        assert!(
            l1 + 0.1 >= 100.0,
            "same-line backstop should keep monotonic left, got {l1}"
        );
        // Soft-wrap to next visual line: must return to the measured left (margin), not stair-step.
        let l2 = advance_emoji_left_cursor(4.0, 24.0, slot, &mut cursor, &mut line_top);
        assert!(
            (l2 - 4.0).abs() < 0.01,
            "wrapped emoji must keep line-start left, got {l2} (stair-step regression)"
        );
    }

    #[test]
    fn slot_advance_near_one_em_with_coverage() {
        use crate::renderer::font_loader;
        use crate::renderer::text_hit::best_font_family;

        let slot = EMOJI_LAYOUT_SLOT.to_string();
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_coverage_for_text(&mut fs, &slot);
        let ui = best_font_family(&fs);
        let size = 14.0_f32;
        let lh = size * 1.5;
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(Some(64.0), Some(lh + 2.0));
        buffer.set_text(
            &slot,
            &glyphon::Attrs::new().family(ui),
            glyphon::Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fs, false);
        let w = buffer
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0.0_f32, f32::max);
        let ratio = w / size;
        println!(
            "slot U+{:04X} whitespace={} advance={w:.2} ratio={ratio:.2}",
            EMOJI_LAYOUT_SLOT as u32,
            EMOJI_LAYOUT_SLOT.is_whitespace()
        );
        assert!(
            ratio > 0.85 && ratio < 1.35,
            "emoji layout slot must be ~1em after coverage, got ratio={ratio}"
        );
    }

    #[test]
    fn emoji_layout_slot_is_not_whitespace() {
        assert!(
            !EMOJI_LAYOUT_SLOT.is_whitespace(),
            "slot must survive soft-wrap leading-whitespace strip (was EM SPACE)"
        );
    }

    #[test]
    fn invisible_slot_attrs_still_emit_layout_glyphs() {
        use crate::renderer::font_loader;
        use crate::renderer::text_hit::best_font_family;

        let text = "Hi 😀 there";
        let layout = plan_emoji_layout_text(text);
        let mut fs = font_loader::create_font_system();
        font_loader::apply_default_families(fs.db_mut());
        font_loader::ensure_coverage_for_text(&mut fs, &layout);
        let ui = best_font_family(&fs);
        let size = 14.0_f32;
        let lh = size * 1.5;
        let visible = glyphon::Attrs::new()
            .family(ui)
            .color(glyphon::Color::rgba(0, 0, 0, 255));
        let invisible = visible.clone().color(glyphon::Color::rgba(0, 0, 0, 0));
        let slot_start = layout.find(EMOJI_LAYOUT_SLOT).unwrap();
        let slot_end = slot_start + EMOJI_LAYOUT_SLOT.len_utf8();
        let owned = [
            (layout[..slot_start].to_string(), visible),
            (layout[slot_start..slot_end].to_string(), invisible),
            (layout[slot_end..].to_string(), visible),
        ];
        let rich: Vec<(&str, glyphon::Attrs)> =
            owned.iter().map(|(t, a)| (t.as_str(), a.clone())).collect();
        let mut buffer = glyphon::Buffer::new(&mut fs, glyphon::Metrics::new(size, lh));
        buffer.set_size(Some(400.0), Some(lh * 2.0));
        buffer.set_rich_text(rich, &visible, glyphon::Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut fs, false);
        let (left, _top) = custom_emoji_left_centered_top(&buffer, &layout, slot_start, size, size)
            .expect("alpha=0 slot must still produce a layout glyph for Twemoji alignment");
        assert!(left.is_finite() && left >= 0.0, "left={left}");
    }
}
