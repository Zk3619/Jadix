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
    use crate::app::reactive::{bind_prop, drain_deferred_view_remounts, reset_registered_anchors};
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
