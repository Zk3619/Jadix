//! 虚拟列表：根据滚动偏移计算可见行区间。

/// 根据垂直滚动偏移、视口高度、行高与总行数，返回 `[start, end)` 行索引（不含 `end`）。
pub fn visible_item_range(
    scroll_y: f32,
    viewport_h: f32,
    item_h: f32,
    total: usize,
    overscan: usize,
) -> (usize, usize) {
    if total == 0 || item_h <= 0.0 {
        return (0, 0);
    }
    let first = ((scroll_y / item_h).floor() as isize).max(0) as usize;
    let vis = (viewport_h / item_h).ceil() as usize;
    let extra = overscan.saturating_mul(2).saturating_add(1);
    let end = (first + vis + extra).min(total);
    (first.min(total), end)
}
