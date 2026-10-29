//! 瀑布流虚拟化：最短列优先装箱 + 视口可见集。
//!
//! 行高来自调用方 [`item_height`] 回调（可接 WP-2.1 测宽）或 [`height_from_cache`] 缓存。
//! 对照 EUI `virtualMasonry`（不搬命令式 DSL）。

use std::collections::HashMap;

/// 一格在内容坐标系中的框。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MasonryItemFrame {
    pub index: usize,
    pub column: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// 全量装箱结果（千级可每帧重算）。
#[derive(Clone, Debug)]
pub struct MasonryLayout {
    pub items: Vec<MasonryItemFrame>,
    pub total_height: f32,
    pub column_width: f32,
}

/// 列宽：`(content_width - gap * (columns-1)) / columns`。
pub fn masonry_column_width(content_width: f32, columns: usize, gap: f32) -> f32 {
    let cols = columns.max(1) as f32;
    let total_gap = gap.max(0.0) * (cols - 1.0);
    ((content_width - total_gap) / cols).max(1.0)
}

/// 缓存命中则用测得高度，否则走 `estimate(index, col_w)`。
pub fn height_from_cache(
    cache: &HashMap<usize, f32>,
    index: usize,
    col_w: f32,
    estimate: impl FnOnce(usize, f32) -> f32,
) -> f32 {
    cache
        .get(&index)
        .copied()
        .filter(|h| *h > 0.5)
        .unwrap_or_else(|| estimate(index, col_w).max(1.0))
}

/// 最短列优先装箱。`heights[i]` 为第 i 项高度。
pub fn pack_masonry(
    heights: &[f32],
    columns: usize,
    content_width: f32,
    gap: f32,
) -> MasonryLayout {
    let columns = columns.max(1);
    let gap = gap.max(0.0);
    let column_width = masonry_column_width(content_width, columns, gap);
    if heights.is_empty() || content_width <= 0.0 {
        return MasonryLayout {
            items: Vec::new(),
            total_height: 0.0,
            column_width,
        };
    }

    let mut column_heights = vec![0.0_f32; columns];
    let mut items = Vec::with_capacity(heights.len());
    for (index, &raw_h) in heights.iter().enumerate() {
        let height = raw_h.max(1.0);
        let mut column = 0usize;
        for c in 1..columns {
            if column_heights[c] < column_heights[column] {
                column = c;
            }
        }
        let x = column as f32 * (column_width + gap);
        let y = column_heights[column];
        items.push(MasonryItemFrame {
            index,
            column,
            x,
            y,
            width: column_width,
            height,
        });
        column_heights[column] = y + height + gap;
    }

    let mut total_height = 0.0_f32;
    for h in column_heights {
        total_height = total_height.max(h);
    }
    total_height = (total_height - gap).max(0.0);

    MasonryLayout {
        items,
        total_height,
        column_width,
    }
}

/// 与视口相交（含 `overscan_px`）的项下标，按 `items` 原序。
pub fn visible_masonry_indices(
    items: &[MasonryItemFrame],
    scroll_y: f32,
    viewport_h: f32,
    overscan_px: f32,
) -> Vec<usize> {
    if items.is_empty() || viewport_h <= 0.0 {
        return Vec::new();
    }
    let top = (scroll_y - overscan_px).max(0.0);
    let bottom = scroll_y + viewport_h + overscan_px.max(0.0);
    items
        .iter()
        .filter(|f| f.y + f.height >= top && f.y <= bottom)
        .map(|f| f.index)
        .collect()
}

#[cfg(test)]
#[path = "tests/virtual_masonry.rs"]
mod tests;
