//! 虚拟列表：根据滚动偏移计算可见行区间。

/// Fixed single-line row height from font size × leading scale (WP-2.1 / VirtualList).
///
/// Matches the common UI default `line_height = font_size * leading_scale` used by
/// [`crate::text::prepare`] callers (e.g. 14 × 1.5 → 21).
pub fn item_height_for_text(font_size: f32, leading_scale: f32) -> f32 {
    (font_size.max(1.0) * leading_scale.max(0.5)).max(1.0)
}

/// 根据垂直滚动偏移、视口高度、行高与总行数，返回 `[start, end)` 行索引（不含 `end`）。
///
/// `overscan` 向窗口两侧各扩展若干行（不足则夹到 `[0, total)`）。
pub fn visible_item_range(
    scroll_y: f32,
    viewport_h: f32,
    item_h: f32,
    total: usize,
    overscan: usize,
) -> (usize, usize) {
    if total == 0
        || !item_h.is_finite()
        || item_h <= 0.0
        || !scroll_y.is_finite()
        || !viewport_h.is_finite()
        || viewport_h <= 0.0
    {
        return (0, 0);
    }
    let offset = f64::from(scroll_y.max(0.0));
    let extent = f64::from(item_h);
    let first = (offset / extent).floor() as usize;
    let start = first.saturating_sub(overscan).min(total);
    let end = (((offset + f64::from(viewport_h)) / extent).ceil() as usize)
        .saturating_add(overscan)
        .min(total);
    (start, end.max(start))
}

#[cfg(test)]
#[path = "tests/virtual_list.rs"]
mod tests;
