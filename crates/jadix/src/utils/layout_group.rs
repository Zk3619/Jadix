//! LayoutGroup / slot_list — rank → 位移（+ 可选 scale）。
//!
//! 对齐 [`docs/components/custom_components_suggestions.md`]：Stack/List Toast、部分列表浮层
//! 不应各自手算 `rank * peek` / 展开步长 / scale 原点补偿。
//!
//! CSS mid 仍由 [`crate::utils::motion`] + `transition_snapshot` 推进；本模块只算**目标**。

/// 一维槽位配置（垂直 Toast / 水平条均可）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotListConfig {
    /// 单项主轴尺寸（如卡高）。用于展开步长与 scale 原点补偿。
    pub item_size: f32,
    /// 收起时相邻项步进（如 Sonner peek）。
    pub collapsed_step: f32,
    /// 展开时间距（不含 `item_size`）；展开步进 = `item_size + expanded_gap`。
    pub expanded_gap: f32,
    /// 收起时每深入一层的 scale 衰减（0 = 不缩）。
    pub scale_per_rank: f32,
    /// scale 下限。
    pub min_scale: f32,
    /// `true`：rank↑ 沿主轴正方向（顶栏堆叠向下）；`false`：负方向（底栏向上）。
    pub positive_axis: bool,
}

impl SlotListConfig {
    /// Toast stack 默认口径（peek / expand gap / 5% 缩尺）。
    pub fn toast_stack(
        item_size: f32,
        collapsed_step: f32,
        expanded_gap: f32,
        from_start: bool,
    ) -> Self {
        Self {
            item_size,
            collapsed_step,
            expanded_gap,
            scale_per_rank: 0.05,
            min_scale: 0.90,
            positive_axis: from_start,
        }
    }

    /// 等距列表（无缩尺）：步进 = `item_size + gap`。
    pub fn linear(item_size: f32, gap: f32, positive_axis: bool) -> Self {
        let step = item_size + gap;
        Self {
            item_size,
            collapsed_step: step,
            expanded_gap: gap,
            scale_per_rank: 0.0,
            min_scale: 1.0,
            positive_axis,
        }
    }

    #[inline]
    pub fn expanded_step(self) -> f32 {
        self.item_size + self.expanded_gap
    }
}

/// rank 对应的目标姿态（写入 style 前；scale 原点补偿见 [`slot_visual_offset`]）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotPose {
    /// 主轴原始偏移（未做 scale 原点补偿）。
    pub offset: f32,
    pub scale: f32,
    /// 收起且非最前：适合压暗阴影 / 降低命中。
    pub dim: bool,
}

/// rank → 原始 offset / scale / dim。
pub fn slot_list_pose(rank: usize, expanded: bool, cfg: &SlotListConfig) -> SlotPose {
    let scale = if expanded || cfg.scale_per_rank <= 0.0 {
        1.0
    } else {
        (1.0 - rank as f32 * cfg.scale_per_rank).max(cfg.min_scale)
    };
    let step = if expanded {
        cfg.expanded_step()
    } else {
        cfg.collapsed_step
    };
    let signed = if cfg.positive_axis {
        rank as f32 * step
    } else {
        -(rank as f32 * step)
    };
    SlotPose {
        offset: signed,
        scale,
        dim: !expanded && rank > 0,
    }
}

/// 等距列表：仅 rank × step（无 scale）。
#[inline]
pub fn linear_slot_offset(rank: usize, step: f32, positive_axis: bool) -> f32 {
    if positive_axis {
        rank as f32 * step
    } else {
        -(rank as f32 * step)
    }
}

/// 缩放绕项中心时，把「起始边」钉住：补偿 `item_size * (1-scale) / 2`。
///
/// - `keep_start_edge=true`（顶栏）：offset 加上补偿，顶边不因缩小上移  
/// - `keep_start_edge=false`（底栏）：offset 减去补偿，底边钉住
pub fn slot_visual_offset(raw: f32, scale: f32, item_size: f32, keep_start_edge: bool) -> f32 {
    let origin_fix = item_size * (1.0 - scale) * 0.5;
    if keep_start_edge {
        raw + origin_fix
    } else {
        raw - origin_fix
    }
}

/// 命名布局组：持有 [`SlotListConfig`]，便于组件内复用。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutGroup {
    pub config: SlotListConfig,
}

impl LayoutGroup {
    pub fn new(config: SlotListConfig) -> Self {
        Self { config }
    }

    pub fn pose(&self, rank: usize, expanded: bool) -> SlotPose {
        slot_list_pose(rank, expanded, &self.config)
    }

    /// 主轴视觉偏移（含 scale 原点补偿）。`keep_start_edge` 通常等于 `config.positive_axis`。
    pub fn visual_offset(&self, rank: usize, expanded: bool) -> f32 {
        let p = self.pose(rank, expanded);
        slot_visual_offset(
            p.offset,
            p.scale,
            self.config.item_size,
            self.config.positive_axis,
        )
    }
}

#[cfg(test)]
#[path = "tests/layout_group.rs"]
mod tests;
