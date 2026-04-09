//! 列表重排拖拽：`focus_id` 约定与信号键，配合 [`crate::app`] 指针路由。
//!
//! - `list_id` 必须不含 `:`（用作 `focus_id` 分段）。
//! - 释放在信号 [`drag_commit_signal_key`] 写入 `Some((from, to))`，由业务 `create_effect` 消费并清空。

/// `focus_id` 前缀：`__rui_drag:{list_id}:{index}`。
pub const DRAG_ITEM_PREFIX: &str = "__rui_drag:";

#[derive(Clone, Debug)]
pub struct DragReorderState {
    pub list_id: String,
    pub from_index: usize,
    pub hover_index: Option<usize>,
}

pub fn drag_item_focus_id(list_id: &str, index: usize) -> String {
    format!("{}{}:{}", DRAG_ITEM_PREFIX, list_id, index)
}

pub fn parse_drag_item_focus_id(fid: &str) -> Option<(String, usize)> {
    let rest = fid.strip_prefix(DRAG_ITEM_PREFIX)?;
    let (list_id, idx_str) = rest.rsplit_once(':')?;
    let index = idx_str.parse().ok()?;
    Some((list_id.to_string(), index))
}

pub fn drag_hover_signal_key(list_id: &str) -> String {
    format!("__rui_drag_hover:{}", list_id)
}

pub fn drag_commit_signal_key(list_id: &str) -> String {
    format!("__rui_drag_commit:{}", list_id)
}

/// `hover` 为指针下「目标行」在**拖拽开始前**列表中的下标；在 `from` 已 `remove` 之后应插入的位置。
pub fn drag_insert_index_after_remove(from: usize, hover: usize) -> usize {
    if hover > from {
        if hover == from + 1 {
            hover
        } else {
            hover - 1
        }
    } else {
        hover
    }
}

/// 按 [`drag_insert_index_after_remove`] 将 `from` 移到 `hover` 行位置（`hover` 为按下前的列表下标）。
pub fn apply_reorder_vec<T>(items: &mut Vec<T>, from: usize, hover: usize) {
    if from >= items.len() || hover >= items.len() {
        return;
    }
    if from == hover {
        return;
    }
    let item = items.remove(from);
    let i = drag_insert_index_after_remove(from, hover).min(items.len());
    items.insert(i, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_insert_adjacent_after() {
        // 拖到正下方相邻格：`hover == from + 1` 时插入下标保持为 `hover`（见实现分支）。
        assert_eq!(drag_insert_index_after_remove(1, 2), 2);
    }

    #[test]
    fn drag_insert_skip_over_removed() {
        assert_eq!(drag_insert_index_after_remove(1, 3), 2);
    }

    #[test]
    fn drag_insert_before_source() {
        assert_eq!(drag_insert_index_after_remove(2, 0), 0);
    }

    #[test]
    fn apply_reorder_head_to_tail() {
        let mut v = vec![0, 1, 2, 3];
        apply_reorder_vec(&mut v, 0, 3);
        // `hover` 为移除前下标；`hover - 1` 对应移除后插入位置（非相邻下移）。
        assert_eq!(v, vec![1, 2, 0, 3]);
    }

    #[test]
    fn apply_reorder_tail_to_head() {
        let mut v = vec![0, 1, 2, 3];
        apply_reorder_vec(&mut v, 3, 0);
        assert_eq!(v, vec![3, 0, 1, 2]);
    }

    #[test]
    fn apply_reorder_ignores_oob() {
        let mut v = vec![1, 2, 3];
        apply_reorder_vec(&mut v, 0, 10);
        assert_eq!(v, vec![1, 2, 3]);
    }
}
