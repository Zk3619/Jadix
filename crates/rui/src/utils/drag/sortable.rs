//! Sortable lists (solid-dnd `createSortable` + SortableJS group / direction).
//!
//! The kit does **not** mutate the author's `Vec` on drop — fire `on_drag_end`
//! and apply [`apply_reorder_vec`] / [`apply_transfer`] (same contract as
//! solid-dnd). Visual shift while dragging is [`sortable_shift`], eased with
//! [`SORTABLE_FLIP_MS`] when the provider `animation` is on (SortableJS
//! `animation`; `false` snaps).

use crate::core::tree::{Element, IntoOpacity};
use crate::utils::drag_reorder::apply_reorder_vec;

use super::builder::{draggable, droppable, DroppableBuilder};
use super::geometry::{Axis, DragStack};
use super::session::DragEvent;

pub use crate::utils::drag_reorder::{
    drag_commit_signal_key, drag_hover_signal_key, drag_insert_index_after_remove,
};

/// SortableJS `animation` default. Live gap / source-placeholder FLIP when the
/// provider leaves `animation` on. Session end **snaps** leftover translate so
/// commit layout is not inverted. `animation={false}` skips the tween entirely.
pub const SORTABLE_FLIP_MS: f32 = 150.0;

/// FLIP-lite translate for siblings while an item is dragged.
///
/// `from` / `hover` are indices in the **pre-remove** list. The dragged index
/// stays in its **layout** slot; siblings close the origin and open the insert
/// hole. Dimmed overlay then paints the source body onto that hole via
/// [`sortable_source_to_hole`]; source `opacity` 0 leaves the hole empty.
/// `max_index` is accepted for call-site stability.
pub fn sortable_shift(
    from: usize,
    hover: usize,
    index: usize,
    item_extent: f32,
    axis: Axis,
    max_index: Option<usize>,
) -> [f32; 2] {
    let _ = max_index;
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

/// Paint-space translate that puts the dimmed source onto the opened insert hole.
///
/// Siblings already vacated that slot ([`sortable_shift`]). Source `opacity` 0
/// skips this (hidden at origin). Append (`hover == len`) is one pitch past the
/// last real index.
pub fn sortable_source_to_hole(
    from: usize,
    hover: usize,
    item_extent: f32,
    axis: Axis,
) -> [f32; 2] {
    if from == hover {
        return [0.0, 0.0];
    }
    let delta = (hover as i32 - from as i32) as f32 * item_extent;
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
    max_index: Option<usize>,
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
            return sortable_shift(from, hover, my_i, item_extent, axis, max_index);
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
    rest_opacity: Option<f32>,
    source_opacity: Option<f32>,
    stack: DragStack,
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
        rest_opacity: None,
        source_opacity: None,
        stack: DragStack::Auto,
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
    /// Resting opacity (same as [`crate::core::tree::Element::opacity`]).
    pub fn opacity(mut self, o: impl IntoOpacity) -> Self {
        self.rest_opacity = Some(o.into_opacity());
        self
    }
    /// Drag-time source opacity while a `DragOverlay` is open. `0` leaves the hole empty.
    pub fn source_opacity(mut self, o: impl IntoOpacity) -> Self {
        self.source_opacity = Some(o.into_opacity());
        self
    }
    /// Same as [`crate::utils::drag::DraggableBuilder::stack`].
    pub fn stack(mut self, stack: impl Into<DragStack>) -> Self {
        self.stack = stack.into();
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let list = if self.list.is_empty() {
            self.id.clone()
        } else {
            self.list
        };
        let mut b = draggable(self.id)
            .item_type(self.item_type)
            .group(self.group)
            .list(list)
            .index(self.index)
            .disabled(self.disabled)
            .handle_only(self.handle_only)
            .also_droppable(true)
            .stack(self.stack);
        if let Some(o) = self.rest_opacity {
            b = b.opacity(o);
        }
        if let Some(o) = self.source_opacity {
            b = b.source_opacity(o);
        }
        b.child(body)
    }
}

#[cfg(test)]
mod tests {
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
}
