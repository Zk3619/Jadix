//! Sortable lists (solid-dnd `createSortable` + SortableJS group / direction).
//!
//! The kit does **not** mutate the author's `Vec` on drop — fire `on_drag_end`
//! and apply [`apply_reorder_vec`] / [`apply_transfer`] (same contract as
//! solid-dnd). Visual shift while dragging is [`sortable_shift`].

use crate::core::tree::Element;
use crate::utils::drag_reorder::apply_reorder_vec;

use super::builder::{draggable, droppable, DroppableBuilder};
use super::geometry::Axis;
use super::session::DragEvent;

pub use crate::utils::drag_reorder::{
    drag_commit_signal_key, drag_hover_signal_key, drag_insert_index_after_remove,
};

/// FLIP-lite translate for siblings while an item is dragged inside one list.
///
/// `from` / `hover` are indices in the **pre-remove** list. The dragged index
/// itself returns `[0, 0]` (the overlay / source-follow owns its motion).
pub fn sortable_shift(
    from: usize,
    hover: usize,
    index: usize,
    item_extent: f32,
    axis: Axis,
) -> [f32; 2] {
    if from == hover || index == from {
        return [0.0, 0.0];
    }
    let delta = if from < hover {
        if index > from && index <= hover {
            -item_extent
        } else {
            0.0
        }
    } else if index >= hover && index < from {
        item_extent
    } else {
        0.0
    };
    match axis {
        Axis::X => [delta, 0.0],
        Axis::Y | Axis::Xy => [0.0, delta],
    }
}

/// Live gap for same-list reorder **and** cross-list transfer.
///
/// Source list items after `from` close the hole; target list items at/after
/// `hover` open a slot. Same-list defers to [`sortable_shift`].
pub fn sortable_preview(
    from_list: Option<&str>,
    from_index: Option<usize>,
    hover_list: Option<&str>,
    hover_index: Option<usize>,
    my_list: Option<&str>,
    my_index: Option<usize>,
    item_extent: f32,
    axis: Axis,
) -> [f32; 2] {
    let (Some(my_list), Some(my_i)) = (my_list, my_index) else {
        return [0.0, 0.0];
    };
    let Some(hover_list) = hover_list else {
        return [0.0, 0.0];
    };
    if from_list == Some(hover_list) {
        let (Some(from), Some(hover)) = (from_index, hover_index) else {
            return [0.0, 0.0];
        };
        if from_list == Some(my_list) {
            return sortable_shift(from, hover, my_i, item_extent, axis);
        }
        return [0.0, 0.0];
    }
    let delta = if from_list == Some(my_list) {
        if my_i > from_index.unwrap_or(my_i) {
            -item_extent
        } else {
            0.0
        }
    } else if hover_list == my_list {
        match hover_index {
            Some(hover) if my_i >= hover => item_extent,
            None => 0.0,
            Some(_) => 0.0,
        }
    } else {
        0.0
    };
    match axis {
        Axis::X => [delta, 0.0],
        Axis::Y | Axis::Xy => [0.0, delta],
    }
}

/// Move `from_index` out of `from` and insert at `to_index` in `to`.
/// Same-list callers should use [`apply_reorder_vec`] instead (overlapping borrows).
pub fn apply_transfer<T>(from: &mut Vec<T>, to: &mut Vec<T>, from_index: usize, to_index: usize) {
    if from_index >= from.len() {
        return;
    }
    let item = from.remove(from_index);
    let i = to_index.min(to.len());
    to.insert(i, item);
}

/// Apply a drop onto either the same list or a second list.
pub fn apply_event<T>(from: &mut Vec<T>, to: Option<&mut Vec<T>>, ev: &DragEvent) {
    let Some(from_index) = ev.from_index else {
        return;
    };
    let to_index = ev.to_index.unwrap_or(from.len().saturating_sub(1));
    match to {
        None => apply_reorder_vec(from, from_index, to_index),
        Some(dest) => apply_transfer(from, dest, from_index, to_index),
    }
}

/// Container droppable wrapping a sortable list (SortableJS root).
pub struct SortableListBuilder {
    inner: DroppableBuilder,
    direction: Axis,
}

pub fn sortable_list(id: impl Into<String>) -> SortableListBuilder {
    let id = id.into();
    SortableListBuilder {
        inner: droppable(id.clone()).list(id),
        direction: Axis::Y,
    }
}

impl SortableListBuilder {
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.inner = self.inner.group(group);
        self
    }
    pub fn accept(mut self, accept: impl Into<String>) -> Self {
        self.inner = self.inner.accept(accept);
        self
    }
    pub fn list(mut self, list_id: impl Into<String>) -> Self {
        self.inner = self.inner.list(list_id);
        self
    }
    pub fn direction(mut self, direction: Axis) -> Self {
        self.direction = direction;
        self
    }
    pub fn disabled(mut self, yes: bool) -> Self {
        self.inner = self.inner.disabled(yes);
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let _ = self.direction;
        self.inner.child(body)
    }
}

/// Item that is both draggable and droppable (solid-dnd `createSortable`).
pub struct SortableItemBuilder {
    id: String,
    list: String,
    index: usize,
    item_type: String,
    group: String,
    disabled: bool,
    handle_only: bool,
}

pub fn sortable_item(id: impl Into<String>) -> SortableItemBuilder {
    SortableItemBuilder {
        id: id.into(),
        list: String::new(),
        index: 0,
        item_type: String::new(),
        group: String::new(),
        disabled: false,
        handle_only: false,
    }
}

impl SortableItemBuilder {
    pub fn list(mut self, list: impl Into<String>) -> Self {
        self.list = list.into();
        self
    }
    pub fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }
    pub fn item_type(mut self, t: impl Into<String>) -> Self {
        self.item_type = t.into();
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
    pub fn handle_only(mut self, yes: bool) -> Self {
        self.handle_only = yes;
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let list = if self.list.is_empty() {
            self.id.clone()
        } else {
            self.list
        };
        draggable(self.id)
            .item_type(self.item_type)
            .group(self.group)
            .list(list)
            .index(self.index)
            .disabled(self.disabled)
            .handle_only(self.handle_only)
            .also_droppable(true)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_moves_intervening_items() {
        // Drag 0 toward 2: index 1 and 2 slide up.
        assert_eq!(sortable_shift(0, 2, 1, 10.0, Axis::Y), [0.0, -10.0]);
        assert_eq!(sortable_shift(0, 2, 2, 10.0, Axis::Y), [0.0, -10.0]);
        assert_eq!(sortable_shift(0, 2, 0, 10.0, Axis::Y), [0.0, 0.0]);
        assert_eq!(sortable_shift(2, 0, 1, 10.0, Axis::Y), [0.0, 10.0]);
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
                Axis::Y
            ),
            [0.0, -10.0]
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
                Axis::Y
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
                Axis::Y
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
}
