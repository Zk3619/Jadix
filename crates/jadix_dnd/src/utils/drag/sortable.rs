//! Sortable lists (solid-dnd `createSortable` + SortableJS group / direction).
//!
//! The kit does **not** mutate the author's `Vec` on drop — fire `on_drag_end`
//! and apply [`apply_reorder_vec`] / [`apply_transfer`] (same contract as
//! solid-dnd). Visual shift while dragging is [`sortable_shift`], eased with
//! [`SORTABLE_FLIP_MS`] when the provider `animation` is on (SortableJS
//! `animation`; `false` snaps).

use std::any::Any;
use std::rc::Rc;

use crate::core::tree::{Element, IntoOpacity};
use crate::utils::drag_reorder::apply_reorder_vec;

use super::builder::{draggable, droppable, DroppableBuilder};
use super::geometry::{Axis, DragStack, Restrict, SortMode};
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
        Axis::Y | Axis::Xy | Axis::Start => [0.0, delta],
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
        Axis::Y | Axis::Xy | Axis::Start => [0.0, delta],
    }
}

/// Live gap for same-list reorder **and** cross-list transfer.
///
/// Same-list defers to [`sortable_shift`]. Cross-list: origin siblings stay
/// put (the source shell has left the pack); target items at/after `hover`
/// open a slot.
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
        // Source leaves the pack on cross-list; origin siblings reflow in layout.
        0.0
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
        Axis::Y | Axis::Xy | Axis::Start => [0.0, delta],
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
pub fn apply_event<T: Clone>(from: &mut Vec<T>, to: Option<&mut Vec<T>>, ev: &DragEvent) {
    if ev.pull_clone {
        apply_clone_event(from, to, ev);
        return;
    }
    if ev.sort_mode == SortMode::Swap {
        apply_swap_event(from, to, ev);
        return;
    }
    if ev.items.len() > 1 {
        apply_multi_event(from, to, ev);
        return;
    }
    let Some(from_index) = ev.from_index else {
        return;
    };
    let to_index = ev.to_index.unwrap_or(from.len().saturating_sub(1));
    match to {
        None => apply_reorder_vec(from, from_index, to_index),
        Some(dest) => apply_transfer(from, dest, from_index, to_index),
    }
}

/// Sortable Swap plugin: exchange two indices. Cross-list swaps values in place.
pub fn apply_swap_event<T>(from: &mut Vec<T>, to: Option<&mut Vec<T>>, ev: &DragEvent) {
    let Some(fi) = ev.from_index else {
        return;
    };
    let Some(ti) = ev.to_index else {
        return;
    };
    match to {
        None => {
            if fi < from.len() && ti < from.len() && fi != ti {
                from.swap(fi, ti);
            }
        }
        Some(_) if ev.from_list.as_deref() == ev.to_list.as_deref() && ev.from_list.is_some() => {
            if fi < from.len() && ti < from.len() && fi != ti {
                from.swap(fi, ti);
            }
        }
        Some(dest) => {
            if fi < from.len() && ti < dest.len() {
                std::mem::swap(&mut from[fi], &mut dest[ti]);
            }
        }
    }
}

/// MultiDrag: lift selected indices (stable relative order) and splice at `to_index`.
pub fn apply_multi_event<T>(from: &mut Vec<T>, to: Option<&mut Vec<T>>, ev: &DragEvent) {
    let mut idxs: Vec<usize> = ev.items.iter().filter_map(|i| i.from_index).collect();
    idxs.sort_unstable();
    idxs.dedup();
    if idxs.len() <= 1 {
        let Some(from_index) = ev.from_index.or(idxs.first().copied()) else {
            return;
        };
        let to_index = ev.to_index.unwrap_or(from.len().saturating_sub(1));
        match to {
            None => apply_reorder_vec(from, from_index, to_index),
            Some(dest) => apply_transfer(from, dest, from_index, to_index),
        }
        return;
    }
    let mut taken = Vec::with_capacity(idxs.len());
    for i in idxs.iter().rev() {
        if *i < from.len() {
            taken.push(from.remove(*i));
        }
    }
    taken.reverse();
    if taken.is_empty() {
        return;
    }
    let to_index = ev.to_index.unwrap_or(0);
    let same_list = ev.from_list.is_some() && ev.from_list == ev.to_list;
    if to.is_none() || same_list {
        let removed_before = idxs.iter().filter(|i| **i < to_index).count();
        let insert_at = to_index.saturating_sub(removed_before).min(from.len());
        for (k, item) in taken.into_iter().enumerate() {
            from.insert(insert_at + k, item);
        }
        return;
    }
    if let Some(dest) = to {
        let insert_at = to_index.min(dest.len());
        for (k, item) in taken.into_iter().enumerate() {
            dest.insert(insert_at + k, item);
        }
    }
}

/// SortableJS `pull: 'clone'` — insert a copy without removing the source.
pub fn apply_clone_event<T: Clone>(from: &mut Vec<T>, to: Option<&mut Vec<T>>, ev: &DragEvent) {
    let Some(from_index) = ev.from_index else {
        return;
    };
    let Some(item) = from.get(from_index).cloned() else {
        return;
    };
    let to_index = ev.to_index.unwrap_or(from.len());
    match to {
        None => {
            from.insert(to_index.min(from.len()), item);
        }
        Some(_) if ev.from_list.as_deref() == ev.to_list.as_deref() && ev.from_list.is_some() => {
            from.insert(to_index.min(from.len()), item);
        }
        Some(dest) => {
            dest.insert(to_index.min(dest.len()), item);
        }
    }
}

/// Container droppable wrapping a sortable list (SortableJS root).
pub struct SortableListBuilder {
    inner: DroppableBuilder,
    direction: Option<Axis>,
}

pub fn sortable_list(id: impl Into<String>) -> SortableListBuilder {
    let id = id.into();
    SortableListBuilder {
        inner: droppable(id.clone()).list(id),
        direction: None,
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
        self.direction = Some(direction);
        self
    }
    pub fn disabled(mut self, yes: bool) -> Self {
        self.inner = self.inner.disabled(yes);
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let inner = if let Some(d) = self.direction {
            self.inner.direction(d)
        } else {
            self.inner
        };
        inner.child(body)
    }
}

/// Item that is both draggable and droppable (solid-dnd `createSortable`).
pub struct SortableItemBuilder {
    id: String,
    list: String,
    index: usize,
    item_type: String,
    group: String,
    accept: String,
    disabled: bool,
    handle_only: bool,
    rest_opacity: Option<f32>,
    source_opacity: Option<f32>,
    stack: DragStack,
    restrict: Option<Restrict>,
    skip_shift: bool,
    skip_follow: bool,
    pull_clone: bool,
    data: Option<Rc<dyn Any>>,
}

pub fn sortable_item(id: impl Into<String>) -> SortableItemBuilder {
    SortableItemBuilder {
        id: id.into(),
        list: String::new(),
        index: 0,
        item_type: String::new(),
        group: String::new(),
        accept: String::new(),
        disabled: false,
        handle_only: false,
        rest_opacity: None,
        source_opacity: None,
        stack: DragStack::Auto,
        restrict: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
        data: None,
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
    /// Drop filter for this row when it is also a droppable (`also_droppable`).
    /// Empty + a non-empty [`Self::item_type`] defaults to same-type only.
    pub fn accept(mut self, accept: impl Into<String>) -> Self {
        self.accept = accept.into();
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
    /// Same as [`crate::utils::drag::DraggableBuilder::restrict`].
    pub fn restrict(mut self, restrict: impl Into<Restrict>) -> Self {
        self.restrict = Some(restrict.into());
        self
    }
    pub fn skip_shift(mut self, yes: bool) -> Self {
        self.skip_shift = yes;
        self
    }
    pub fn skip_follow(mut self, yes: bool) -> Self {
        self.skip_follow = yes;
        self
    }
    pub fn pull_clone(mut self, yes: bool) -> Self {
        self.pull_clone = yes;
        self
    }
    pub fn data(mut self, payload: Rc<dyn Any>) -> Self {
        self.data = Some(payload);
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
            .accept(self.accept)
            .list(list)
            .index(self.index)
            .disabled(self.disabled)
            .handle_only(self.handle_only)
            .also_droppable(true)
            .stack(self.stack)
            .skip_shift(self.skip_shift)
            .skip_follow(self.skip_follow)
            .pull_clone(self.pull_clone);
        if let Some(d) = self.data {
            b = b.data(d);
        }
        if let Some(r) = self.restrict {
            b = b.restrict(r);
        }
        if let Some(o) = self.rest_opacity {
            b = b.opacity(o);
        }
        if let Some(o) = self.source_opacity {
            b = b.source_opacity(o);
        }
        b.child(body)
    }
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/sortable.rs"]
mod tests;
