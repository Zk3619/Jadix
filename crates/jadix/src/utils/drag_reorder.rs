//! 列表重排拖拽：`focus_id` 约定与信号键，配合 [`crate::app`] 指针路由。
//!
//! - `list_id` 必须不含 `:`（用作 `focus_id` 分段）。
//! - 释放在信号 [`drag_commit_signal_key`] 写入 `Some((from, to))`，由业务 `create_effect` 消费并清空。

/// `focus_id` 前缀：`__jadix_drag:{list_id}:{index}`。
pub const DRAG_ITEM_PREFIX: &str = "__jadix_drag:";

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
    format!("__jadix_drag_hover:{}", list_id)
}

/// Write the live preview hover. Call only when `AppState` is not borrowed —
/// `Signal::set` flushes `bind_prop` which re-borrows the tree (C1 / Wave 53).
pub(crate) fn set_drag_hover(list_id: &str, hover: Option<usize>) {
    crate::core::reactivity::use_signal(&drag_hover_signal_key(list_id), || None::<usize>)
        .set(hover);
}

pub fn drag_commit_signal_key(list_id: &str) -> String {
    format!("__jadix_drag_commit:{}", list_id)
}

/// `hover` is the **pre-remove** slot the live preview opened (`sortable_shift`
/// / `insert_index_along_axis`). After `remove(from)` the item must land in
/// that same slot — not `hover - 1`. The old `hover - 1` (except adjacent)
/// made non-adjacent down-drags commit one row short; repeating that scramble
/// the list.
pub fn drag_insert_index_after_remove(_from: usize, hover: usize) -> usize {
    hover
}

/// Move `from` into the preview hole at `hover` (`hover == items.len()` appends).
pub fn apply_reorder_vec<T>(items: &mut Vec<T>, from: usize, hover: usize) {
    if from >= items.len() || hover > items.len() {
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
#[path = "tests/drag_reorder.rs"]
mod tests;
