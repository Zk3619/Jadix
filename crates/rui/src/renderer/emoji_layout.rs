//! 从已布局的 glyphon [`Buffer`] 推导自定义 emoji 贴图与占位字形对齐的 `CustomGlyph` 坐标。

use glyphon::cosmic_text::LineIter;
use unicode_segmentation::UnicodeSegmentation;

use super::emoji_detect;

/// EM SPACE — ~1em advance in UI sans, matches the custom emoji quad slot.
pub const EMOJI_LAYOUT_SLOT: char = '\u{2003}';

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

/// Whether paint replaces emoji with EM SPACE slots + color quads (default on).
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

/// 水平对齐占位 glyph 的 `x`，垂直在行盒内居中放置 `emoji_height` 的方块。
///
/// 不依赖 Mask tofu 的 `placement`（避免底切），也不要求 swash 光栅。
pub fn custom_emoji_left_centered_top(
    buffer: &glyphon::Buffer,
    layout_text: &str,
    global_byte: usize,
    emoji_height: f32,
) -> Option<(f32, f32)> {
    let (line_i, local) = global_byte_to_line_local(layout_text, global_byte)?;
    let (run, g) = find_glyph_for_line_byte(buffer, line_i, local)?;
    let left = g.x;
    let pad = (run.line_height - emoji_height).max(0.0) * 0.5;
    let top = run.line_top + pad;
    Some((left, top))
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
}
