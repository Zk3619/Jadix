//! Grid 模板辅助：等分列、多 `fr` 轨等（布局计算仍由 Taffy 完成）。

use taffy::prelude::*;

/// Taffy 0.13 templates are generic over named-line idents; RUI does not use names.
pub type GridTrack = GridTemplateComponent<String>;

/// Tailwind `grid-cols-{n}` 风格：N 列等分 `minmax(0, 1fr)`。
#[inline]
pub fn cols_equal_fr(n: u16) -> Vec<GridTrack> {
    evenly_sized_tracks(n)
}

/// 多列（或行）按给定 `fr` 权重分配。
#[inline]
pub fn tracks_fr(weights: &[f32]) -> Vec<GridTrack> {
    weights
        .iter()
        .copied()
        .map(flex::<f32, GridTrack>)
        .collect()
}

/// 单行或单列：`1fr` 占满剩余空间。
#[inline]
pub fn track_fr_1() -> GridTrack {
    flex::<f32, GridTrack>(1.0)
}
