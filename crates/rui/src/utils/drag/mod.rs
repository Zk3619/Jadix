//! Headless **drag** kit (**WP-DRAG.1**).
//!
//! Maps solid-dnd (provider / draggable / droppable / overlay / collision),
//! SortableJS (lists, groups, handles, cross-container, auto-scroll, delay,
//! empty-insert, onMove veto), and interact.js (axis lock, restrict, snap,
//! threshold) onto RUI's pointer session + Point-mode
//! [`crate::utils::anchored_overlay`] ghost (overlay_shells.md §9).
//!
//! North star: **surpass SortableJS** (~40k★) — live sibling shift, edge
//! auto-scroll while held, Escape cancel, arrow-key insert nudge, keyboard
//! first-class. Not in `prelude` — `use rui::utils::drag::{…}`. Dual-write:
//! Builder is the source of truth; RSX expands to the same functions (ADR-9).

pub mod autoscroll;
pub mod collision;
pub mod geometry;
pub mod overlay;
pub mod session;
pub mod sortable;

mod builder;

pub use autoscroll::{edge_delta, AutoScroll, ScrollHit};
pub use builder::{
    drag_handle, drag_handle_from_rsx, drag_ignore, drag_ignore_from_rsx, drag_scope,
    drag_scope_from_rsx, draggable, draggable_from_rsx, droppable, droppable_from_rsx, use_drag,
    use_drag_item, DragCtx, DragItemCtx, DragScopeBuilder, DragScopeRsxOpts, DraggableBuilder,
    DraggableRsxOpts, DroppableBuilder, DroppableRsxOpts,
};
pub use collision::{detect, Candidate, CollisionAlg};
pub use geometry::{clamp_origin_to_box, distance, Axis, Rect, Restrict, SnapGrid};
pub use overlay::{drag_overlay, drag_overlay_from_rsx, DragOverlayBuilder};
pub use session::{
    active_draggable_id, active_scope_id, active_source_translate, begin, cancel, clear_if_any,
    collect_hits, end, insert_index_along_axis, is_active, is_dragging, last_draggable_bounds,
    last_pointer, nudge_insert, overlay_hides_source, overlay_is_registered, pump_autoscroll,
    refresh_hit_cache, resolve_insert_index, start_geometry, tick, BeginDrag, DragEvent,
    DragHandler, DragHit, DragPhase, DragPredicate,
};
pub use sortable::{
    apply_event, apply_transfer, sortable_item, sortable_list, sortable_preview, sortable_shift,
    SortableItemBuilder, SortableListBuilder, SORTABLE_FLIP_MS,
};

/// Re-export list-reorder helpers so callers can import one module.
pub use crate::utils::drag_reorder::{
    apply_reorder_vec, drag_commit_signal_key, drag_hover_signal_key,
    drag_insert_index_after_remove,
};

pub const ATTR_DRAG_SCOPE: &str = "rui_drag_scope";
pub const ATTR_DRAG_DRAGGABLE: &str = "rui_drag_draggable";
pub const ATTR_DRAG_DROPPABLE: &str = "rui_drag_droppable";
pub const ATTR_DRAG_TYPE: &str = "rui_drag_type";
pub const ATTR_DRAG_ACCEPT: &str = "rui_drag_accept";
pub const ATTR_DRAG_LIST: &str = "rui_drag_list";
pub const ATTR_DRAG_INDEX: &str = "rui_drag_index";
pub const ATTR_DRAG_GROUP: &str = "rui_drag_group";
pub const ATTR_DRAG_DISABLED: &str = "rui_drag_disabled";
pub const ATTR_DRAG_HANDLE: &str = "rui_drag_handle";
pub const ATTR_DRAG_OVERLAY: &str = "rui_drag_overlay";
pub const ATTR_DRAG_IGNORE: &str = "rui_drag_ignore";
/// Collision target (solid-dnd `isActiveDroppable`). Paint-only; not CSS `:hover`.
pub const ATTR_DRAG_OVER: &str = "rui_drag_over";
