use super::*;

#[test]
fn shift_moves_intervening_items() {
    // Drag 0 toward 2: index 1 and 2 slide up; source stays in its slot.
    assert_eq!(sortable_shift(0, 2, 1, 10.0, Axis::Y, None), [0.0, -10.0]);
    assert_eq!(sortable_shift(0, 2, 2, 10.0, Axis::Y, None), [0.0, -10.0]);
    assert_eq!(sortable_shift(0, 2, 0, 10.0, Axis::Y, None), [0.0, 0.0]);
    assert_eq!(sortable_shift(2, 0, 1, 10.0, Axis::Y, None), [0.0, 10.0]);
    assert_eq!(sortable_shift(2, 0, 2, 10.0, Axis::Y, None), [0.0, 0.0]);
}

#[test]
fn source_slot_does_not_ride_with_siblings() {
    let extent = 10.0;
    let src = sortable_shift(0, 1, 0, extent, Axis::Y, None);
    let next = sortable_shift(0, 1, 1, extent, Axis::Y, None);
    assert_eq!(src, [0.0, 0.0]);
    assert_eq!(next, [0.0, -10.0]);
}

#[test]
fn dimmed_source_translate_lands_on_insert_hole() {
    assert_eq!(sortable_source_to_hole(0, 2, 10.0, Axis::Y), [0.0, 20.0]);
    assert_eq!(sortable_source_to_hole(2, 0, 10.0, Axis::Y), [0.0, -20.0]);
    assert_eq!(sortable_source_to_hole(0, 0, 10.0, Axis::Y), [0.0, 0.0]);
    assert_eq!(
        sortable_source_to_hole(0, 4, 10.0, Axis::Y),
        [0.0, 40.0],
        "append hover=len is one pitch past last index 3"
    );
}

#[test]
fn append_hover_leaves_source_in_slot() {
    assert_eq!(sortable_shift(0, 4, 0, 10.0, Axis::Y, Some(3)), [0.0, 0.0]);
}

#[test]
fn preview_cross_list_opens_and_closes_gaps() {
    assert_eq!(
        sortable_preview(
            Some("a"),
            Some(0),
            Some("b"),
            Some(1),
            Some("a"),
            Some(1),
            10.0,
            Axis::Y,
            None
        ),
        [0.0, 0.0],
        "cross-list origin sibling stays put; source already left the pack"
    );
    assert_eq!(
        sortable_preview(
            Some("a"),
            Some(0),
            Some("b"),
            Some(1),
            Some("b"),
            Some(1),
            10.0,
            Axis::Y,
            None
        ),
        [0.0, 10.0]
    );
    assert_eq!(
        sortable_preview(
            Some("a"),
            Some(0),
            Some("b"),
            Some(1),
            Some("b"),
            Some(0),
            10.0,
            Axis::Y,
            None
        ),
        [0.0, 0.0]
    );
}

#[test]
fn transfer_cross_list() {
    let mut a = vec!["a", "b", "c"];
    let mut b = vec!["x"];
    apply_transfer(&mut a, &mut b, 1, 0);
    assert_eq!(a, vec!["a", "c"]);
    assert_eq!(b, vec!["b", "x"]);
}

#[test]
fn clone_event_inserts_without_remove() {
    use crate::utils::drag::{DragEvent, DragPhase, SortMode};
    let mut items = vec!["a", "b", "c"];
    let ev = DragEvent {
        scope_id: "s".into(),
        draggable_id: "a".into(),
        droppable_id: None,
        from_list: Some("list".into()),
        from_index: Some(0),
        to_list: Some("list".into()),
        to_index: Some(2),
        pointer: [0.0, 0.0],
        delta: [0.0, 0.0],
        phase: DragPhase::Active,
        pull_clone: true,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    apply_clone_event(&mut items, None, &ev);
    assert_eq!(items, vec!["a", "b", "a", "c"]);
    assert_eq!(items.len(), 4);
}

#[test]
fn apply_event_routes_clone() {
    use crate::utils::drag::{DragEvent, DragPhase, SortMode};
    let mut items = vec![1, 2, 3];
    let ev = DragEvent {
        scope_id: "s".into(),
        draggable_id: "1".into(),
        droppable_id: None,
        from_list: Some("l".into()),
        from_index: Some(0),
        to_list: Some("l".into()),
        to_index: Some(1),
        pointer: [0.0, 0.0],
        delta: [0.0, 0.0],
        phase: DragPhase::Active,
        pull_clone: true,
        data: None,
        sort_mode: SortMode::Insert,
        items: Vec::new(),
    };
    apply_event(&mut items, None, &ev);
    assert_eq!(items, vec![1, 1, 2, 3]);
}

fn ev(
    from_list: &str,
    from_index: usize,
    to_list: &str,
    to_index: usize,
    sort_mode: SortMode,
    items: Vec<(usize, &str)>,
) -> crate::utils::drag::DragEvent {
    use crate::utils::drag::{DragEvent, DragItemRef, DragPhase};
    DragEvent {
        scope_id: "s".into(),
        draggable_id: items
            .first()
            .map(|(_, id)| (*id).to_string())
            .unwrap_or_else(|| "x".into()),
        droppable_id: None,
        from_list: Some(from_list.into()),
        from_index: Some(from_index),
        to_list: Some(to_list.into()),
        to_index: Some(to_index),
        pointer: [0.0, 0.0],
        delta: [0.0, 0.0],
        phase: DragPhase::Active,
        pull_clone: false,
        data: None,
        sort_mode,
        items: items
            .into_iter()
            .map(|(i, id)| DragItemRef {
                id: id.into(),
                from_list: Some(from_list.into()),
                from_index: Some(i),
            })
            .collect(),
    }
}

#[test]
fn apply_swap_same_list() {
    let mut items = vec!["a", "b", "c", "d"];
    apply_event(
        &mut items,
        None,
        &ev("l", 0, "l", 2, SortMode::Swap, vec![(0, "a")]),
    );
    assert_eq!(items, vec!["c", "b", "a", "d"]);
}

#[test]
fn apply_swap_cross_list() {
    let mut a = vec!["a", "b"];
    let mut b = vec!["x", "y"];
    apply_event(
        &mut a,
        Some(&mut b),
        &ev("left", 0, "right", 1, SortMode::Swap, vec![(0, "a")]),
    );
    assert_eq!(a, vec!["y", "b"]);
    assert_eq!(b, vec!["x", "a"]);
}

#[test]
fn apply_multi_same_list_block() {
    let mut items = vec!["a", "b", "c", "d", "e"];
    apply_event(
        &mut items,
        None,
        &ev("l", 1, "l", 5, SortMode::Insert, vec![(1, "b"), (3, "d")]),
    );
    // remove b@1 d@3 → rest [a,c,e]; append slot 5, removed_before=2 → insert 3
    assert_eq!(items, vec!["a", "c", "e", "b", "d"]);
}

#[test]
fn apply_multi_to_head() {
    let mut items = vec!["a", "b", "c", "d"];
    apply_event(
        &mut items,
        None,
        &ev("l", 1, "l", 0, SortMode::Insert, vec![(1, "b"), (2, "c")]),
    );
    assert_eq!(items, vec!["b", "c", "a", "d"]);
}

#[test]
fn apply_multi_cross_list() {
    let mut a = vec!["a", "b", "c"];
    let mut b = vec!["x"];
    apply_event(
        &mut a,
        Some(&mut b),
        &ev(
            "left",
            0,
            "right",
            1,
            SortMode::Insert,
            vec![(0, "a"), (2, "c")],
        ),
    );
    assert_eq!(a, vec!["b"]);
    assert_eq!(b, vec!["x", "a", "c"]);
}
