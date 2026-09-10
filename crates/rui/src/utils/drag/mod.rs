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
pub mod debug;
pub mod dropzone;
pub mod geometry;
pub mod modifier;
pub mod overlay;
pub mod registry;
pub mod sensor;
pub mod session;
pub mod sortable;

mod builder;

pub use crate::utils::pointer_action::InertiaOpts;
pub use autoscroll::{edge_delta, AutoScroll, ScrollHit, ScrollLock};
pub use builder::{
    drag_handle, drag_handle_from_rsx, drag_ignore, drag_ignore_from_rsx, drag_scope,
    drag_scope_from_rsx, draggable, draggable_from_rsx, droppable, droppable_from_rsx, use_drag,
    use_drag_item, DragCtx, DragItemCtx, DragScopeBuilder, DragScopeRsxOpts, DraggableBuilder,
    DraggableRsxOpts, DroppableBuilder, DroppableRsxOpts,
};
pub use collision::{detect, detect_with, Candidate, CollisionAlg, CollisionFn, CollisionInput};
pub use debug::{
    debug_drag_hits_enabled, debug_frames_for_tree, debug_hit_frames, last_debug_frames,
    set_debug_drag_hits, DragDebugEmphasis, DragDebugFrame, DragDebugRole, DragDebugStroke,
};
pub use dropzone::{
    set_dropzone_config, DropCheckerFn, DropOverlap, DropzoneConfig, DropzoneHandler, DropzoneHooks,
};
pub use geometry::{
    clamp_origin_to_box, clamp_origin_to_box_anchor, distance, Activation, Axis, DragStack,
    OverlayAlign, Rect, Restrict, RestrictAnchor, Snap, SnapGrid, SnapPointFn, SnapTarget,
    SnapTargets, SortMode,
};
pub use modifier::{
    apply_modifiers, builtin_modifiers, DragDelta, DragModifier, DragModifierCtx,
    MODIFIER_ORDER_AUTHOR, MODIFIER_ORDER_AXIS, MODIFIER_ORDER_PIN_AXIS, MODIFIER_ORDER_RESTRICT,
    MODIFIER_ORDER_SNAP,
};
pub use overlay::{drag_overlay, drag_overlay_from_rsx, DragOverlayBuilder};
pub use registry::{item_data, item_flags, registered_hits_in_scope, set_item_data, ItemFlags};
pub use sensor::{
    add_sensor, begin_draggable, remove_sensor, run_scope_sensors, DragSensor, DragSensorCtx,
};
pub use session::{
    active_drag_ids, active_draggable_id, active_droppable_id, active_scope_id, active_sort_mode,
    active_source_translate, add_modifier, begin, cancel, clear_if_any, collect_hits, end,
    insert_index_along_axis, is_active, is_coasting, is_dragging, last_drag_extent,
    last_draggable_bounds, last_hit_rect, last_hits, last_hits_in_scope, last_overlay_pos,
    last_pointer, nudge_insert, overlay_hides_source, overlay_is_registered,
    overlay_source_opacity, pump_autoscroll, pump_inertia, refresh_hit_cache, remove_modifier,
    resolve_insert_index, start_geometry, tick, BeginDrag, DragEvent, DragHandler, DragHit,
    DragItemRef, DragPhase, DragPredicate, OVERLAY_SOURCE_OPACITY,
};
pub use sortable::{
    apply_clone_event, apply_event, apply_multi_event, apply_swap_event, apply_transfer,
    sortable_item, sortable_list, sortable_preview, sortable_shift, sortable_source_to_hole,
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
/// Kit wrapper around a draggable island. [`Restrict::Parent`] skips this node
/// so the clamp lands on the author's container, not a hug-content shell.
pub const ATTR_DRAG_ITEM_SHELL: &str = "rui_drag_item_shell";
/// Sortable list direction (`x` / `y`). Overrides the provider `axis` for insert / FLIP.
pub const ATTR_DRAG_AXIS: &str = "rui_drag_axis";
/// Skip sibling FLIP / sortable shift for this item (solid-dnd `skipTransform` → shift).
pub const ATTR_DRAG_SKIP_SHIFT: &str = "rui_drag_skip_shift";
/// Skip kit pointer-follow / hole translate; author owns motion.
pub const ATTR_DRAG_SKIP_FOLLOW: &str = "rui_drag_skip_follow";
/// SortableJS `pull: 'clone'` — drag a copy; source stays on drop insert.
pub const ATTR_DRAG_PULL_CLONE: &str = "rui_drag_pull_clone";
/// Swap-mode peer under the pointer (Sortable `swapClass`).
pub const ATTR_DRAG_SWAP: &str = "rui_drag_swap";
/// MultiDrag click-select (Sortable `selectedClass`).
pub const ATTR_DRAG_SELECTED: &str = "rui_drag_selected";
