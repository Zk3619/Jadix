//! 从已布局的 glyphon [`Buffer`] 推导自定义 emoji 贴图与占位字形对齐的 `CustomGlyph` 坐标。

use glyphon::cosmic_text::LineIter;

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

/// 计算与占位彩色字形 quad 对齐的 `CustomGlyph.left` / `CustomGlyph.top`（逻辑单位，乘 `TextArea.scale` 后与 `TextArea.left/top` 组合）。
///
/// 与 glyphon 对普通字形的 `prepare_glyph` 位置公式一致：`round(line_y*scale) + physical.y - placement.top`。
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
}
