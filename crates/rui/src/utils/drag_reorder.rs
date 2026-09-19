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

/// Write the live preview hover. Call only when `AppState` is not borrowed —
/// `Signal::set` flushes `bind_prop` which re-borrows the tree (C1 / Wave 53).
pub(crate) fn set_drag_hover(list_id: &str, hover: Option<usize>) {
    crate::core::reactivity::use_signal(&drag_hover_signal_key(list_id), || None::<usize>)
        .set(hover);
}

pub fn drag_commit_signal_key(list_id: &str) -> String {
    format!("__rui_drag_commit:{}", list_id)
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
mod tests {
    use super::*;

    #[test]
    fn drag_insert_adjacent_after() {
        assert_eq!(drag_insert_index_after_remove(1, 2), 2);
    }

    #[test]
    fn drag_insert_non_adjacent_down_keeps_hover_slot() {
        // Preview opens a hole at `hover=3`; commit must use 3, not 2.
        assert_eq!(drag_insert_index_after_remove(1, 3), 3);
    }

    #[test]
    fn drag_insert_before_source() {
        assert_eq!(drag_insert_index_after_remove(2, 0), 0);
    }

    #[test]
    fn apply_reorder_head_to_tail_slot() {
        let mut v = vec![0, 1, 2, 3];
        apply_reorder_vec(&mut v, 0, 3);
        assert_eq!(v, vec![1, 2, 3, 0]);
    }

    #[test]
    fn apply_reorder_tail_to_head() {
        let mut v = vec![0, 1, 2, 3];
        apply_reorder_vec(&mut v, 3, 0);
        assert_eq!(v, vec![3, 0, 1, 2]);
    }

    #[test]
    fn apply_reorder_head_to_end() {
        let mut v = vec![0, 1, 2];
        apply_reorder_vec(&mut v, 0, 3);
        assert_eq!(v, vec![1, 2, 0]);
    }

    #[test]
    fn apply_reorder_ignores_oob() {
        let mut v = vec![1, 2, 3];
        apply_reorder_vec(&mut v, 0, 10);
        assert_eq!(v, vec![1, 2, 3]);
    }

    fn committed_index(len: usize, from: usize, hover: usize) -> usize {
        let mut v: Vec<usize> = (0..len).collect();
        let id = v[from];
        apply_reorder_vec(&mut v, from, hover);
        v.iter().position(|&x| x == id).unwrap()
    }

    #[test]
    fn commit_lands_on_preview_hole() {
        // Down more than one row: hole is at `hover`, not `hover - 1`.
        assert_eq!(committed_index(8, 0, 3), 3);
        assert_eq!(committed_index(8, 1, 5), 5);
        assert_eq!(committed_index(4, 0, 2), 2);
        // Adjacent down.
        assert_eq!(committed_index(8, 2, 3), 3);
        // Up.
        assert_eq!(committed_index(8, 5, 1), 1);
        assert_eq!(committed_index(4, 3, 0), 0);
        // Append past last midpoint (`hover == len`).
        assert_eq!(committed_index(8, 0, 8), 7);
        assert_eq!(committed_index(3, 0, 3), 2);
    }

    #[test]
    fn repeated_reorders_stay_a_permutation() {
        let mut v: Vec<usize> = (0..8).collect();
        for &(from, hover) in &[(0, 4), (2, 7), (5, 1), (0, 8), (6, 2), (3, 3), (1, 0)] {
            apply_reorder_vec(&mut v, from, hover);
        }
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..8).collect::<Vec<_>>());
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn drag_hover_set_lands_after_tree_borrow_drop() {
        use crate::app::reactive::{
            bind_prop, drain_deferred_view_remounts, reset_registered_anchors,
        };
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root};
        use crate::core::tree::{Element, LayoutStatus};
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();
        let tree = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&tree));
        let list = "drag-hover-drop";
        let (_handle, _) = create_root({
            let tree = Rc::clone(&tree);
            move || {
                create_effect({
                    let tree = Rc::clone(&tree);
                    let list = list.to_string();
                    move || {
                        let leaf = bind_prop(
                            Element::new().template_id("drag-hover-leaf"),
                            "drag-hover-leaf",
                            {
                                let list = list.clone();
                                move || {
                                    crate::core::reactivity::use_signal(
                                        &drag_hover_signal_key(&list),
                                        || None::<usize>,
                                    )
                                    .get()
                                }
                            },
                            |el, n| {
                                el.text = Some(n.map(|i| i.to_string()).unwrap_or_default());
                                LayoutStatus::StyleChanged
                            },
                        );
                        *tree.borrow_mut() = leaf;
                    }
                });
            }
        });
        drain_deferred_view_remounts();
        assert_eq!(tree.borrow().text.as_deref(), Some(""));

        {
            let held = tree.borrow_mut();
            set_drag_hover(list, Some(2));
            assert_eq!(
                held.text.as_deref(),
                Some(""),
                "hover bind must miss while the live tree is borrowed"
            );
        }
        drain_deferred_view_remounts();
        assert_eq!(
            tree.borrow().text.as_deref(),
            Some("2"),
            "leaf bind must land once try_borrow_mut succeeds"
        );
        set_drag_hover(list, None);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }
}
