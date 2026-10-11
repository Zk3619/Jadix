//! Pointer session: begin → (pending threshold) → active tick → end.
//!
//! `window_events` collects [`DragHit`]s from the laid-out tree and calls [`tick`] /
//! [`end`]. Components start a session from `pointerdown` via [`begin`].

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use crate::core::reactivity::{batch, Signal};
use crate::core::tree::Element;

use crate::utils::pointer_action::{Inertia, InertiaOpts, VelocityTracker};

use super::autoscroll::AutoScroll;
use super::collision::{detect_with, Candidate, CollisionAlg, CollisionFn, CollisionInput};
#[cfg(test)]
use super::geometry::SnapGrid;
use super::geometry::{
    Activation, Axis, OverlayAlign, Rect, Restrict, RestrictAnchor, RestrictOverflow, Snap,
    SortMode,
};
use super::modifier::{
    apply_modifiers, builtin_modifiers, DragDelta, DragModifier, DragModifierCtx,
};
use super::{
    ATTR_DRAG_ACCEPT, ATTR_DRAG_AXIS, ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE,
    ATTR_DRAG_GROUP, ATTR_DRAG_HOLE_PREVIEW, ATTR_DRAG_INDEX, ATTR_DRAG_ITEM_SHELL, ATTR_DRAG_LIST,
    ATTR_DRAG_OVERLAY, ATTR_DRAG_PULL_CLONE, ATTR_DRAG_SCOPE, ATTR_DRAG_SKIP_FOLLOW,
    ATTR_DRAG_SKIP_SHIFT, ATTR_DRAG_TYPE,
};
use crate::utils::edge_scroll::pump_at;

pub type DragHandler = Rc<RefCell<dyn FnMut(&DragEvent)>>;
pub type DragPredicate = Rc<dyn Fn(&DragEvent) -> bool>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Pending,
    Active,
    /// Pointer is up; exponential fling / smoothEnd still drives the ghost.
    Coasting,
}

/// One item in a drag (single or MultiDrag freeze).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DragItemRef {
    pub id: String,
    pub from_list: Option<String>,
    pub from_index: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct DragEvent {
    pub scope_id: String,
    pub draggable_id: String,
    pub droppable_id: Option<String>,
    pub from_list: Option<String>,
    pub from_index: Option<usize>,
    pub to_list: Option<String>,
    pub to_index: Option<usize>,
    pub pointer: [f32; 2],
    /// Axis-filtered pointer travel during start/move; final modifier-adjusted
    /// travel relative to the live source slot on end (including parent scroll).
    /// Persist this directly without snapping it again.
    pub delta: [f32; 2],
    pub phase: DragPhase,
    /// SortableJS `pull: 'clone'` session.
    pub pull_clone: bool,
    /// Item [`super::registry::set_item_data`] at drag start.
    pub data: Option<Rc<dyn Any>>,
    /// Provider [`SortMode`] frozen at `begin`.
    pub sort_mode: SortMode,
    /// Frozen drag set. Length 1 = single item; `> 1` = MultiDrag.
    pub items: Vec<DragItemRef>,
}

/// A laid-out draggable or droppable (or both) collected from the element tree.
///
/// `rect` is the pointer-space **layout slot** (scroll applied, CSS translate
/// omitted) so sortable preview shift does not move collision targets.
#[derive(Clone, Debug)]
pub struct DragHit {
    pub id: String,
    pub scope_id: String,
    pub is_draggable: bool,
    pub is_droppable: bool,
    pub rect: Rect,
    pub item_type: String,
    pub accept: String,
    pub list_id: Option<String>,
    pub index: Option<usize>,
    pub group: String,
    pub disabled: bool,
    /// List-level sortable axis (`rui_drag_axis`). `None` inherits the provider.
    pub list_axis: Option<Axis>,
    pub skip_shift: bool,
    pub skip_follow: bool,
    pub pull_clone: bool,
}

impl DragHit {
    fn accepts(&self, dragged: &DragSession) -> bool {
        if self.disabled || !self.is_droppable {
            return false;
        }
        // Another DragProvider's droppables must not steal hover. Skipping when
        // `hit.scope_id` is empty let Gallery's opacity-0 list (same item
        // names, stacked above) count as a cross-list target: the origin closed,
        // the dimmed body translated away, and the list visually collapsed.
        if self.scope_id != dragged.scope_id {
            return false;
        }
        if self.id == dragged.draggable_id {
            return false;
        }
        if !self.accept.is_empty() {
            let want = dragged.item_type.as_str();
            let ok = self
                .accept
                .split(',')
                .map(|s| s.trim())
                .any(|t| t == want || t == "*");
            if !ok {
                return false;
            }
        } else if !self.item_type.is_empty()
            && !dragged.item_type.is_empty()
            && self.item_type != dragged.item_type
        {
            // Empty accept used to mean "anything". Nested SortableItem rows
            // (file chips inside a folder card) then accepted a parent-folder
            // drag, so hover_list jumped into the inner list and FLIP shoved
            // files into the outer column. Same-type default matches the
            // author's item_type without forcing every chip to set accept=.
            return false;
        }
        if !self.group.is_empty() && !dragged.group.is_empty() && self.group != dragged.group {
            return false;
        }
        true
    }
}

#[derive(Clone)]
struct DragSession {
    scope_id: String,
    draggable_id: String,
    phase: DragPhase,
    start_pointer: [f32; 2],
    last_pointer: [f32; 2],
    /// Pointer minus the **item** origin at press (interact.js grab offset).
    grab_offset: [f32; 2],
    start_bounds: [f32; 4],
    from_list: Option<String>,
    from_index: Option<usize>,
    item_type: String,
    group: String,
    /// Configured provider axis (`Start` stays here so sort_axis stays Y).
    axis: Axis,
    /// Resolved motion lock (interact `lockAxis`; `Start` becomes X or Y on activate).
    motion_axis: Axis,
    /// interact.js `startAxis` — must move this way to activate (`Xy` = any).
    start_axis: Axis,
    threshold: f32,
    restrict: Restrict,
    restrict_end_only: bool,
    restrict_anchor: RestrictAnchor,
    snap: Option<Snap>,
    capture_id: String,
    delay_ms: f32,
    activation: Activation,
    pressed_at: Instant,
    last_viewport: [f32; 2],
    pull_clone: bool,
    data: Option<Rc<dyn Any>>,
    sort_mode: SortMode,
    /// Frozen at activate (click-select set, or just the primary id).
    items: Vec<DragItemRef>,
}

struct ScopeProduct {
    sort_mode: SortMode,
    multi_drag: bool,
    selected: Signal<Vec<String>>,
    selection_anchor: Option<String>,
    shift_pressed: bool,
    reorder_preview: bool,
}

pub(super) struct ScopeRuntime {
    pub collision: CollisionAlg,
    pub axis: Axis,
    pub start_axis: Axis,
    pub threshold: f32,
    pub restrict: Restrict,
    pub restrict_end_only: bool,
    pub restrict_anchor: RestrictAnchor,
    pub restrict_overflow: RestrictOverflow,
    pub snap: Option<Snap>,
    pub overlay_registered: bool,
    /// In-list source opacity while a `DragOverlay` is open. `0` leaves the
    /// insert hole empty; default overlay uses [`OVERLAY_SOURCE_OPACITY`].
    pub source_opacity: f32,
    pub autoscroll: AutoScroll,
    pub empty_insert: f32,
    pub delay_ms: f32,
    pub activation: Activation,
    pub collision_fn: Option<CollisionFn>,
    pub overlay_align: OverlayAlign,
    pub can_drop: Option<DragPredicate>,
    pub on_start: Option<DragHandler>,
    pub on_move: Option<DragHandler>,
    pub on_over: Option<DragHandler>,
    pub on_end: Option<DragHandler>,
    pub overlay_open: Signal<bool>,
    pub overlay_pos: Signal<[f32; 2]>,
    pub active_draggable: Signal<Option<String>>,
    pub active_droppable: Signal<Option<String>>,
    pub from_list: Signal<Option<String>>,
    pub from_index: Signal<Option<usize>>,
    pub hover_list: Signal<Option<String>>,
    pub hover_index: Signal<Option<usize>>,
    pub drag_extent: Signal<[f32; 2]>,
    /// Last assigned [`super::geometry::DragStack::Auto`] rank in this scope.
    pub stack_seq: i32,
    /// Author modifiers (solid-dnd `addTransformer` / dnd-kit `Modifier`).
    /// Built-ins are assembled each tick, not stored here.
    pub modifiers: Vec<DragModifier>,
    /// Custom sensors (solid-dnd `addSensor`). Pointer is always available.
    pub sensors: Vec<super::sensor::DragSensor>,
}

thread_local! {
    static SESSION: RefCell<Option<DragSession>> = const { RefCell::new(None) };
    static SCOPES: RefCell<HashMap<String, ScopeRuntime>> = RefCell::new(HashMap::new());
    /// Runtime [`add_modifier`] stages, keyed by scope id. Survives
    /// [`register_scope`] rebuilds (builder `modifiers` are replaced).
    static EXTRA_MODIFIERS: RefCell<HashMap<String, Vec<DragModifier>>> =
        RefCell::new(HashMap::new());
    /// Last laid-out item boxes, keyed by `scope\0id`. Handle drags read this so
    /// the ghost tracks the **item**, not the handle's smaller hit box.
    static ITEM_BOUNDS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
    /// Per-item [`Restrict`] override (`Draggable.restrict`). Missing → scope.
    static ITEM_RESTRICT: RefCell<HashMap<String, Restrict>> = RefCell::new(HashMap::new());
    /// Pointer-space boxes for `template_id` / attr `id` (restrict-to-element).
    static NAMED_BOUNDS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
    /// Author-parent box of each draggable (`restrict="parent"`).
    static PARENT_BOUNDS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
    /// Author packing gap survives singleton lists, where neighbour stride is unavailable.
    static ITEM_PACK_GAPS: RefCell<HashMap<String, [f32; 2]>> = RefCell::new(HashMap::new());
    static LAST_HITS: RefCell<Vec<DragHit>> = const { RefCell::new(Vec::new()) };
    static LAST_SCROLL_AT: RefCell<Option<Instant>> = const { RefCell::new(None) };
    /// Packed row (item + author gap) and the author `gap` inside it,
    /// snapshotted at activate from rest hits. Live hits after the source
    /// shell collapses would drop the gap and make target padding / source
    /// vacate feed back on themselves. `pack_gap` must not be re-derived
    /// from live `drag_extent` (that signal is the paint chip and can equal
    /// the pitch).
    static DRAG_PACK_PITCH: Cell<f32> = const { Cell::new(0.0) };
    static DRAG_PACK_GAP: Cell<f32> = const { Cell::new(0.0) };
    // Logical source slots survive the source shell's cross-list size animation.
    static SOURCE_LIST_LAYOUT: RefCell<Vec<DragHit>> = const { RefCell::new(Vec::new()) };
    static SOURCE_LAYOUT_OFFSETS: RefCell<HashMap<String, [f32; 2]>> = RefCell::new(HashMap::new());
    /// Laid-out overlay child size for [`OverlayAlign::Center`]
    /// (`origin = pointer - size / 2`).
    static OVERLAY_GHOST_EXTENT: RefCell<HashMap<String, [f32; 2]>> = RefCell::new(HashMap::new());
    static VELOCITY: RefCell<VelocityTracker> = RefCell::new(VelocityTracker::default());
    static COAST: RefCell<Option<Inertia>> = RefCell::new(None);
    static SCOPE_INERTIA: RefCell<HashMap<String, InertiaOpts>> = RefCell::new(HashMap::new());
    /// Sort / multi-select product flags. Separate from [`ScopeRuntime`] so
    /// session tests do not reconstruct every field.
    static SCOPE_PRODUCT: RefCell<HashMap<String, ScopeProduct>> = RefCell::new(HashMap::new());
    static COAST_TICK: Cell<bool> = const { Cell::new(false) };
    static LAST_COAST_AT: RefCell<Option<Instant>> = const { RefCell::new(None) };
    /// Viewport-space restrict box while `RestrictOverflow::Hidden` (overlay clip).
    static RESTRICT_CLIP: RefCell<Option<[f32; 4]>> = const { RefCell::new(None) };
    /// Page ScrollArea / header clip at the restrict target. Applied as an extra
    /// scissor on the portal clip shell — the shell itself stays the parent box
    /// so rotation cannot leak into the padding beside the dashed arena.
    static RESTRICT_ANCESTOR_CLIP: RefCell<Option<[f32; 4]>> = const { RefCell::new(None) };
    /// Ancestor overflow clip (page ScrollArea / header) at each named restrict
    /// target. Hidden clip shells live on the portal layer and must scissor to
    /// this intersection or they paint over chrome that already clipped the parent.
    static ANCESTOR_OVERFLOW_CLIP: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
}

pub(super) fn set_scope_inertia(scope_id: &str, opts: InertiaOpts) {
    SCOPE_INERTIA.with(|m| {
        m.borrow_mut().insert(scope_id.to_string(), opts);
    });
}

fn scope_inertia(scope_id: &str) -> InertiaOpts {
    SCOPE_INERTIA.with(|m| m.borrow().get(scope_id).copied().unwrap_or_default())
}

pub fn is_coasting() -> bool {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|d| d.phase == DragPhase::Coasting)
    })
}

fn item_bounds_key(scope: &str, id: &str) -> String {
    format!("{scope}\0{id}")
}

pub(crate) fn remember_draggable_bounds(scope: &str, id: &str, bounds: [f32; 4]) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    ITEM_BOUNDS.with(|m| {
        m.borrow_mut().insert(item_bounds_key(scope, id), bounds);
    });
}

/// Item box from the last [`collect_hits`] / [`refresh_hit_cache`] walk.
pub fn last_draggable_bounds(scope: &str, id: &str) -> Option<[f32; 4]> {
    ITEM_BOUNDS.with(|m| m.borrow().get(&item_bounds_key(scope, id)).copied())
}

/// Pointer-space slot from the last [`tick`] hit list (`(scope, id)`).
///
/// Droppables are included (Gallery corner-rotate reads the arena). Falls back
/// to [`last_draggable_bounds`] when `tick` has not stored hits yet.
pub fn last_hit_rect(scope: &str, id: &str) -> Option<[f32; 4]> {
    let from_tick = LAST_HITS.with(|h| {
        h.borrow()
            .iter()
            .find(|hit| hit.scope_id == scope && hit.id == id)
            .map(|hit| hit.rect.to_bounds())
    });
    from_tick.or_else(|| last_draggable_bounds(scope, id))
}

pub(crate) fn remember_overlay_ghost_extent(scope: &str, size: [f32; 2]) {
    if size[0] <= 0.0 || size[1] <= 0.0 {
        return;
    }
    OVERLAY_GHOST_EXTENT.with(|m| {
        m.borrow_mut().insert(scope.to_string(), size);
    });
}

fn clear_overlay_ghost_extent(scope: &str) {
    OVERLAY_GHOST_EXTENT.with(|m| {
        m.borrow_mut().remove(scope);
    });
}

fn overlay_ghost_extent(scope: &str) -> Option<[f32; 2]> {
    OVERLAY_GHOST_EXTENT.with(|m| m.borrow().get(scope).copied())
}

fn resolved_overlay_ghost_size(sess: &DragSession) -> [f32; 2] {
    overlay_ghost_extent(&sess.scope_id)
        .or_else(|| {
            SCOPES.with(|m| {
                m.borrow()
                    .get(&sess.scope_id)
                    .map(|s| s.drag_extent.peek())
                    .filter(|e| e[0] > 0.0 && e[1] > 0.0)
            })
        })
        .unwrap_or([sess.start_bounds[2].max(1.0), sess.start_bounds[3].max(1.0)])
}

/// Recompute the Center ghost origin after Taffy (or a style measure) knows
/// the real chip size. `None` when this scope is not `OverlayAlign::Center`.
pub(crate) fn overlay_center_origin_now(scope: &str) -> Option<[f32; 2]> {
    let align = SCOPES.with(|m| {
        m.borrow()
            .get(scope)
            .map(|s| s.overlay_align)
            .unwrap_or(OverlayAlign::Grab)
    });
    if align != OverlayAlign::Center {
        return None;
    }
    let origin = SESSION.with(|s| {
        let sess = s.borrow();
        let sess = sess.as_ref()?;
        if sess.scope_id != scope {
            return None;
        }
        let hits = LAST_HITS.with(|h| h.borrow().clone());
        Some(ghost_origin(
            sess,
            sess.last_pointer,
            sess.last_viewport,
            &hits,
            false,
        ))
    })?;
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(scope) {
            if rt.overlay_pos.peek() != origin {
                rt.overlay_pos.set_silent(origin);
            }
        }
    });
    Some(origin)
}

/// Remember a measured ghost size, then pin origin to the live pointer.
/// Center align recomputes from the chip size; Grab uses `overlay_pos`.
pub(crate) fn overlay_origin_after_ghost_measure(scope: &str, size: [f32; 2]) -> Option<[f32; 2]> {
    remember_overlay_ghost_extent(scope, size);
    overlay_center_origin_now(scope).or_else(|| peek_overlay_pos(scope))
}

pub(crate) fn remember_item_restrict(scope: &str, id: &str, restrict: Option<Restrict>) {
    ITEM_RESTRICT.with(|m| {
        let key = item_bounds_key(scope, id);
        match restrict {
            Some(r) => {
                m.borrow_mut().insert(key, r);
            }
            None => {
                m.borrow_mut().remove(&key);
            }
        }
    });
}

fn item_restrict_override(scope: &str, id: &str) -> Option<Restrict> {
    ITEM_RESTRICT.with(|m| m.borrow().get(&item_bounds_key(scope, id)).cloned())
}

fn remember_named_bounds(id: &str, bounds: [f32; 4]) {
    if id.is_empty() || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    NAMED_BOUNDS.with(|m| {
        m.borrow_mut().insert(id.to_string(), bounds);
    });
}

fn remember_ancestor_overflow_clip(id: &str, clip: Option<[f32; 4]>) {
    if id.is_empty() {
        return;
    }
    ANCESTOR_OVERFLOW_CLIP.with(|m| match clip.filter(|c| c[2] > 0.0 && c[3] > 0.0) {
        Some(c) => {
            m.borrow_mut().insert(id.to_string(), c);
        }
        None => {
            m.borrow_mut().remove(id);
        }
    });
}

fn last_ancestor_overflow_clip(id: &str) -> Option<[f32; 4]> {
    ANCESTOR_OVERFLOW_CLIP.with(|m| m.borrow().get(id).copied())
}

fn intersect_xywh(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

fn child_overflow_clip(
    el: &Element,
    slot: [f32; 4],
    ancestor: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    if !crate::core::tree::element_clips_overflow(el) {
        return ancestor;
    }
    Some(match ancestor {
        Some(c) => intersect_xywh(c, slot),
        None => slot,
    })
}

fn restrict_ancestor_clip_for(sess: &DragSession) -> Option<[f32; 4]> {
    match &sess.restrict {
        Restrict::Element(id) => last_ancestor_overflow_clip(id),
        Restrict::Parent => {
            last_ancestor_overflow_clip(&item_bounds_key(&sess.scope_id, &sess.draggable_id))
        }
        _ => None,
    }
}

fn last_named_bounds(id: &str) -> Option<[f32; 4]> {
    NAMED_BOUNDS.with(|m| m.borrow().get(id).copied())
}

fn remember_parent_bounds(scope: &str, id: &str, bounds: [f32; 4]) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    PARENT_BOUNDS.with(|m| {
        m.borrow_mut().insert(item_bounds_key(scope, id), bounds);
    });
}

fn last_parent_bounds(scope: &str, id: &str) -> Option<[f32; 4]> {
    PARENT_BOUNDS.with(|m| m.borrow().get(&item_bounds_key(scope, id)).copied())
}

/// Live clamp box for [`Restrict::Element`]: same-scope hit, then any hit, then
/// `template_id` / attr `id` from the last collect walk.
fn lookup_restrict_element(scope: &str, id: &str, hits: &[DragHit]) -> Option<[f32; 4]> {
    hits.iter()
        .find(|h| h.id == id && h.scope_id == scope && h.rect.is_usable())
        .or_else(|| hits.iter().find(|h| h.id == id && h.rect.is_usable()))
        .map(|h| h.rect.to_bounds())
        .or_else(|| last_named_bounds(id))
        .or_else(|| last_hit_rect(scope, id))
}

fn resolve_live_restrict(
    restrict: &Restrict,
    scope: &str,
    draggable_id: &str,
    hits: &[DragHit],
) -> Restrict {
    match restrict {
        Restrict::Element(id) => lookup_restrict_element(scope, id, hits)
            .map(Restrict::Rect)
            .unwrap_or(Restrict::None),
        Restrict::Parent => last_parent_bounds(scope, draggable_id)
            .map(Restrict::Rect)
            .unwrap_or(Restrict::None),
        Restrict::Viewport => Restrict::Viewport,
        Restrict::Rect(r) => Restrict::Rect(*r),
        Restrict::None => Restrict::None,
    }
}

/// Walk the laid-out tree so handle `pointerdown` can resolve the item rect.
pub fn refresh_hit_cache(root: &Element) {
    let hits = collect_hits(root);
    LAST_HITS.with(|h| *h.borrow_mut() = hits);
}

/// Grab offset + start box for [`begin`]. Prefers the cached item rect so a
/// `<DragHandle>` does not shrink the overlay to the grip.
pub fn start_geometry(
    scope: &str,
    id: &str,
    pointer: [f32; 2],
    fallback: [f32; 4],
) -> ([f32; 2], [f32; 4]) {
    let bounds = last_draggable_bounds(scope, id).unwrap_or(fallback);
    ([pointer[0] - bounds[0], pointer[1] - bounds[1]], bounds)
}

pub fn scope_exists(scope_id: &str) -> bool {
    SCOPES.with(|m| m.borrow().contains_key(scope_id))
}

/// Builder-registered sensors for one provider (merged with [`super::sensor::add_sensor`] at run time).
pub fn scope_sensors(scope_id: &str) -> Vec<super::sensor::DragSensor> {
    SCOPES.with(|m| {
        m.borrow()
            .get(scope_id)
            .map(|s| s.sensors.clone())
            .unwrap_or_default()
    })
}

pub fn active_pull_clone() -> bool {
    SESSION.with(|s| s.borrow().as_ref().is_some_and(|d| d.pull_clone))
}

pub(super) fn register_scope_product(
    id: String,
    sort_mode: SortMode,
    multi_drag: bool,
    selected: Signal<Vec<String>>,
) {
    SCOPE_PRODUCT.with(|m| {
        let mut m = m.borrow_mut();
        let selection_anchor = m.get(&id).and_then(|p| p.selection_anchor.clone());
        let shift_pressed = m.get(&id).is_some_and(|p| p.shift_pressed);
        m.insert(
            id,
            ScopeProduct {
                sort_mode,
                multi_drag,
                selected,
                selection_anchor,
                shift_pressed,
                reorder_preview: false,
            },
        );
    });
}

pub(super) fn set_reorder_preview(scope: &str, enabled: bool) {
    SCOPE_PRODUCT.with(|m| {
        if let Some(p) = m.borrow_mut().get_mut(scope) {
            p.reorder_preview = enabled;
        }
    });
}

pub(super) fn reorder_preview(scope: &str) -> bool {
    SCOPE_PRODUCT.with(|m| m.borrow().get(scope).is_some_and(|p| p.reorder_preview))
}

pub(super) fn active_reorder_preview() -> bool {
    active_scope_id().is_some_and(|scope| reorder_preview(&scope))
}

pub(super) fn selection_press(scope: &str, shift: bool) {
    SCOPE_PRODUCT.with(|m| {
        if let Some(p) = m.borrow_mut().get_mut(scope) {
            p.shift_pressed = shift;
        }
    });
}

pub(super) fn scope_sort_mode(id: &str) -> SortMode {
    SCOPE_PRODUCT.with(|m| m.borrow().get(id).map(|p| p.sort_mode).unwrap_or_default())
}

pub(super) fn scope_multi_drag(id: &str) -> bool {
    SCOPE_PRODUCT.with(|m| m.borrow().get(id).is_some_and(|p| p.multi_drag))
}

pub(super) fn scope_selected_contains(scope: &str, id: &str) -> bool {
    SCOPE_PRODUCT.with(|m| {
        m.borrow()
            .get(scope)
            .is_some_and(|p| p.selected.peek().iter().any(|x| x == id))
    })
}

/// Provider [`SortMode`] of the live session (`Insert` when idle).
pub fn active_sort_mode() -> SortMode {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .map(|d| d.sort_mode)
            .unwrap_or(SortMode::Insert)
    })
}

/// Frozen drag ids (`[primary]` when idle / single). Overlay stacks these.
/// Pending press is not a drag — returning the id here would dim/raise the
/// source (`source_opacity=0`) before a ghost exists.
pub fn active_drag_ids() -> Vec<String> {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .filter(|d| d.phase == DragPhase::Active || d.phase == DragPhase::Coasting)
            .map(|d| {
                if d.items.is_empty() {
                    vec![d.draggable_id.clone()]
                } else {
                    d.items.iter().map(|i| i.id.clone()).collect()
                }
            })
            .unwrap_or_default()
    })
}

fn try_click_select(scope: &str, id: &str, hits: &[DragHit]) {
    let Some((selected, anchor, shift)) = SCOPE_PRODUCT.with(|m| {
        let mut m = m.borrow_mut();
        let p = m.get_mut(scope)?;
        if !p.multi_drag {
            return None;
        }
        let anchor = p.selection_anchor.clone();
        if !p.shift_pressed || anchor.is_none() {
            p.selection_anchor = Some(id.to_string());
        }
        Some((p.selected, anchor, p.shift_pressed))
    }) else {
        return;
    };
    let mut v = selected.peek();
    let range = anchor.as_deref().and_then(|anchor| {
        let a = hits
            .iter()
            .find(|h| h.scope_id == scope && h.id == anchor && h.is_draggable)?;
        let b = hits
            .iter()
            .find(|h| h.scope_id == scope && h.id == id && h.is_draggable)?;
        if a.list_id.is_none() || a.list_id != b.list_id {
            return None;
        }
        Some((
            a.list_id.clone(),
            a.index?.min(b.index?),
            a.index?.max(b.index?),
        ))
    });
    if let Some((list, first, last)) = range.filter(|_| shift) {
        let mut rows: Vec<_> = hits
            .iter()
            .filter(|h| {
                h.scope_id == scope
                    && h.is_draggable
                    && !h.disabled
                    && h.list_id == list
                    && h.index.is_some_and(|i| i >= first && i <= last)
            })
            .collect();
        rows.sort_by_key(|h| h.index);
        for h in rows {
            if !v.contains(&h.id) {
                v.push(h.id.clone());
            }
        }
    } else if let Some(i) = v.iter().position(|x| x == id) {
        v.remove(i);
    } else {
        v.push(id.to_string());
    }
    selected.set(v);
    crate::app::runtime::request_redraw();
}

fn freeze_drag_items(sess: &mut DragSession, hits: &[DragHit]) {
    let primary = DragItemRef {
        id: sess.draggable_id.clone(),
        from_list: sess.from_list.clone(),
        from_index: sess.from_index,
    };
    if sess.sort_mode == SortMode::Swap || !scope_multi_drag(&sess.scope_id) {
        sess.items = vec![primary];
        return;
    }
    let selected = SCOPE_PRODUCT.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|p| p.selected.peek())
            .unwrap_or_default()
    });
    if !selected.iter().any(|sid| sid == &sess.draggable_id) {
        sess.items = vec![primary];
        return;
    }
    let from_list = sess.from_list.clone();
    let mut refs: Vec<DragItemRef> = hits
        .iter()
        .filter(|h| {
            h.scope_id == sess.scope_id
                && h.is_draggable
                && !h.disabled
                && h.index.is_some()
                && h.list_id == from_list
                && selected.iter().any(|sid| sid == &h.id)
        })
        .map(|h| DragItemRef {
            id: h.id.clone(),
            from_list: h.list_id.clone(),
            from_index: h.index,
        })
        .collect();
    refs.sort_by_key(|r| r.from_index);
    refs.dedup_by(|a, b| a.id == b.id);
    if refs.is_empty() {
        refs.push(primary);
    }
    sess.items = refs;
}

pub(super) fn register_scope(id: String, rt: ScopeRuntime) {
    SCOPES.with(|m| {
        let mut map = m.borrow_mut();
        let mut rt = rt;
        if let Some(old) = map.get(&id) {
            rt.overlay_registered = rt.overlay_registered || old.overlay_registered;
            rt.source_opacity = old.source_opacity;
            rt.overlay_align = old.overlay_align;
            rt.stack_seq = old.stack_seq;
        }
        map.insert(id, rt);
    });
}

pub(super) fn with_scope<R>(id: &str, f: impl FnOnce(&mut ScopeRuntime) -> R) -> Option<R> {
    SCOPES.with(|m| m.borrow_mut().get_mut(id).map(f))
}

/// Register an author modifier on a live scope (solid-dnd `addTransformer`).
/// Replaces an existing stage with the same `id`. Survives provider rebuilds.
pub fn add_modifier(scope_id: &str, modifier: DragModifier) -> bool {
    let exists = SCOPES.with(|m| m.borrow().contains_key(scope_id));
    if !exists {
        return false;
    }
    EXTRA_MODIFIERS.with(|m| {
        let mut map = m.borrow_mut();
        let list = map.entry(scope_id.to_string()).or_default();
        list.retain(|t| t.id != modifier.id);
        list.push(modifier);
    });
    true
}

/// Remove an author modifier by id (runtime extra, then builder list).
pub fn remove_modifier(scope_id: &str, id: &str) -> bool {
    let from_extra = EXTRA_MODIFIERS.with(|m| {
        let mut map = m.borrow_mut();
        let Some(list) = map.get_mut(scope_id) else {
            return false;
        };
        let before = list.len();
        list.retain(|t| t.id != id);
        let changed = list.len() != before;
        if list.is_empty() {
            map.remove(scope_id);
        }
        changed
    });
    let from_builder = with_scope(scope_id, |rt| {
        let before = rt.modifiers.len();
        rt.modifiers.retain(|t| t.id != id);
        rt.modifiers.len() != before
    })
    .unwrap_or(false);
    from_extra || from_builder
}

pub(super) fn mark_overlay_registered(scope_id: &str) {
    with_scope(scope_id, |rt| rt.overlay_registered = true);
}

/// Next [`super::geometry::DragStack::Auto`] rank for `scope_id` (1, 2, …).
pub(super) fn bump_auto_stack(scope_id: &str) -> i32 {
    with_scope(scope_id, |rt| {
        rt.stack_seq = rt.stack_seq.saturating_add(1);
        rt.stack_seq
    })
    .unwrap_or(1)
}

pub fn overlay_is_registered(scope_id: &str) -> bool {
    SCOPES.with(|m| {
        m.borrow()
            .get(scope_id)
            .is_some_and(|s| s.overlay_registered)
    })
}

/// Viewport origin of the drag ghost. Peek-only: subscribers remount the overlay island.
pub(super) fn peek_overlay_pos(scope_id: &str) -> Option<[f32; 2]> {
    SCOPES.with(|m| m.borrow().get(scope_id).map(|s| s.overlay_pos.peek()))
}

/// [`peek_overlay_pos`] for authors (Gallery overlay `rotate` / HUD). Does not subscribe.
pub fn last_overlay_pos(scope_id: &str) -> Option<[f32; 2]> {
    peek_overlay_pos(scope_id)
}

pub(super) fn scope_restrict_overflow(scope_id: &str) -> RestrictOverflow {
    SCOPES.with(|m| {
        m.borrow()
            .get(scope_id)
            .map(|s| s.restrict_overflow)
            .unwrap_or(RestrictOverflow::Visible)
    })
}

/// Viewport-space restrict rectangle while overflow is [`RestrictOverflow::Hidden`].
pub(super) fn peek_restrict_clip() -> Option<[f32; 4]> {
    RESTRICT_CLIP.with(|c| *c.borrow())
}

/// Page overflow that still clips a Hidden portal shell (header / ScrollArea).
pub(crate) fn peek_restrict_ancestor_clip() -> Option<[f32; 4]> {
    RESTRICT_ANCESTOR_CLIP.with(|c| *c.borrow())
}

fn set_restrict_clip(box_: Option<[f32; 4]>) {
    RESTRICT_CLIP.with(|c| *c.borrow_mut() = box_);
}

pub(crate) fn set_restrict_ancestor_clip(box_: Option<[f32; 4]>) {
    RESTRICT_ANCESTOR_CLIP.with(|c| *c.borrow_mut() = box_);
}

fn clear_restrict_clip_state() {
    set_restrict_clip(None);
    set_restrict_ancestor_clip(None);
}

fn restrict_clip_bounds(resolved: &Restrict, viewport: [f32; 2]) -> Option<[f32; 4]> {
    match resolved {
        Restrict::None | Restrict::Parent | Restrict::Element(_) => None,
        Restrict::Viewport => Some([0.0, 0.0, viewport[0].max(1.0), viewport[1].max(1.0)]),
        Restrict::Rect(r) => Some(*r),
    }
}

/// Live ghost size for this scope (`drag_extent`). Peek-only.
pub fn last_drag_extent(scope_id: &str) -> Option<[f32; 2]> {
    SCOPES.with(|m| m.borrow().get(scope_id).map(|s| s.drag_extent.peek()))
}

/// Default source-island opacity while `<DragOverlay>` is open and the item
/// does not set its own `opacity`.
pub const OVERLAY_SOURCE_OPACITY: f32 = 0.4;

pub fn overlay_source_opacity(scope_id: &str) -> f32 {
    SCOPES.with(|m| {
        m.borrow()
            .get(scope_id)
            .map(|s| s.source_opacity)
            .unwrap_or(1.0)
    })
}

pub fn overlay_hides_source(scope_id: &str) -> bool {
    overlay_source_opacity(scope_id) <= 0.001
}

pub fn is_dragging() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

pub fn is_active() -> bool {
    SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|d| d.phase == DragPhase::Active)
    })
}

pub fn active_draggable_id() -> Option<String> {
    SESSION.with(|s| s.borrow().as_ref().map(|d| d.draggable_id.clone()))
}

pub fn active_scope_id() -> Option<String> {
    SESSION.with(|s| s.borrow().as_ref().map(|d| d.scope_id.clone()))
}

/// Collision winner for the active scope (`None` when idle or vetoed).
pub fn active_droppable_id() -> Option<String> {
    let scope = active_scope_id()?;
    SCOPES.with(|m| {
        m.borrow()
            .get(&scope)
            .and_then(|s| s.active_droppable.peek())
    })
}

/// Last `tick` hit table (process-wide). Prefer [`last_hits_in_scope`] or `DragCtx::hits`.
pub fn last_hits() -> Vec<DragHit> {
    LAST_HITS.with(|h| h.borrow().clone())
}

/// [`last_hits`] filtered to one provider. Empty when idle or after another scope's tick.
pub fn last_hits_in_scope(scope_id: &str) -> Vec<DragHit> {
    last_hits()
        .into_iter()
        .filter(|h| h.scope_id == scope_id)
        .collect()
}

pub(crate) fn last_hits_snapshot() -> Vec<DragHit> {
    last_hits()
}

/// Translate to apply on the source when no overlay is registered.
pub fn active_source_translate() -> Option<[f32; 2]> {
    SESSION.with(|s| {
        let sess = s.borrow();
        let sess = sess.as_ref()?;
        if sess.phase != DragPhase::Active && sess.phase != DragPhase::Coasting {
            return None;
        }
        let origin =
            SCOPES.with(|m| m.borrow().get(&sess.scope_id).map(|r| r.overlay_pos.peek()))?;
        LAST_HITS.with(|hits| Some(source_translation(sess, origin, &hits.borrow())))
    })
}

fn event_from(
    sess: &DragSession,
    droppable: Option<&DragHit>,
    to_list: Option<String>,
    to_index: Option<usize>,
) -> DragEvent {
    event_from_delta(sess, droppable, to_list, to_index, None)
}

fn event_from_delta(
    sess: &DragSession,
    droppable: Option<&DragHit>,
    to_list: Option<String>,
    to_index: Option<usize>,
    delta: Option<[f32; 2]>,
) -> DragEvent {
    let [dx, dy] = delta.unwrap_or_else(|| {
        let (dx, dy) = sess.motion_axis.apply(
            sess.last_pointer[0] - sess.start_pointer[0],
            sess.last_pointer[1] - sess.start_pointer[1],
        );
        [dx, dy]
    });
    DragEvent {
        scope_id: sess.scope_id.clone(),
        draggable_id: sess.draggable_id.clone(),
        droppable_id: droppable.map(|d| d.id.clone()),
        from_list: sess.from_list.clone(),
        from_index: sess.from_index,
        to_list: to_list.or_else(|| droppable.and_then(|d| d.list_id.clone())),
        to_index,
        pointer: sess.last_pointer,
        delta: [dx, dy],
        phase: sess.phase,
        pull_clone: sess.pull_clone,
        data: sess.data.clone(),
        sort_mode: sess.sort_mode,
        items: if sess.items.is_empty() {
            vec![DragItemRef {
                id: sess.draggable_id.clone(),
                from_list: sess.from_list.clone(),
                from_index: sess.from_index,
            }]
        } else {
            sess.items.clone()
        },
    }
}

fn list_passes_can_drop(
    pred: &DragPredicate,
    sess: &DragSession,
    hits: &[DragHit],
    list: &str,
) -> bool {
    let hit = hits
        .iter()
        .find(|h| h.list_id.as_deref() == Some(list) && h.index.is_none())
        .or_else(|| hits.iter().find(|h| h.list_id.as_deref() == Some(list)));
    match hit {
        Some(h) => pred(&event_from(sess, Some(h), Some(list.to_string()), h.index)),
        None => true,
    }
}

/// Start a pointer session. Does not fire `on_drag_start` until the threshold is crossed.
pub fn begin(opts: BeginDrag) {
    if is_coasting() {
        let same = SESSION.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|d| d.draggable_id == opts.draggable_id)
        });
        let allow = SESSION.with(|s| {
            s.borrow()
                .as_ref()
                .map(|d| scope_inertia(&d.scope_id).allow_resume)
                .unwrap_or(false)
        });
        if same && allow {
            COAST.with(|c| *c.borrow_mut() = None);
            LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
            VELOCITY.with(|v| v.borrow_mut().reset());
            SESSION.with(|s| {
                if let Some(sess) = s.borrow_mut().as_mut() {
                    sess.phase = DragPhase::Active;
                    sess.last_pointer = opts.pointer;
                }
            });
            crate::utils::pointer_drag::begin(
                crate::utils::pointer_drag::DragKind::Scope,
                opts.draggable_id.clone(),
                opts.pointer[0],
                opts.pointer[1],
            );
            crate::app::set_pointer_capture(opts.capture_id);
            return;
        }
        let hits = LAST_HITS.with(|h| h.borrow().clone());
        let _ = commit_end(&hits);
    }
    VELOCITY.with(|v| v.borrow_mut().reset());
    COAST.with(|c| *c.borrow_mut() = None);
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::Scope,
        opts.draggable_id.clone(),
        opts.pointer[0],
        opts.pointer[1],
    );
    crate::app::set_pointer_capture(opts.capture_id.clone());
    let (
        axis,
        threshold,
        scope_restrict,
        snap,
        delay_ms,
        activation,
        start_axis,
        restrict_end_only,
        restrict_anchor,
    ) = SCOPES.with(|m| {
        m.borrow()
            .get(&opts.scope_id)
            .map(|s| {
                (
                    s.axis,
                    s.threshold,
                    s.restrict.clone(),
                    s.snap.clone(),
                    s.delay_ms,
                    s.activation,
                    s.start_axis,
                    s.restrict_end_only,
                    s.restrict_anchor,
                )
            })
            .unwrap_or((
                Axis::Xy,
                4.0,
                Restrict::None,
                None,
                0.0,
                Activation::And,
                Axis::Xy,
                false,
                RestrictAnchor::FULL,
            ))
    });
    let restrict =
        item_restrict_override(&opts.scope_id, &opts.draggable_id).unwrap_or(scope_restrict);
    let motion_axis = if axis == Axis::Start { Axis::Xy } else { axis };
    let pull_clone = super::registry::item_flags(&opts.scope_id, &opts.draggable_id).pull_clone;
    let data = super::registry::item_data(&opts.scope_id, &opts.draggable_id);
    let sort_mode = scope_sort_mode(&opts.scope_id);
    selection_press(&opts.scope_id, false);
    let items = vec![DragItemRef {
        id: opts.draggable_id.clone(),
        from_list: opts.from_list.clone(),
        from_index: opts.from_index,
    }];
    clear_overlay_ghost_extent(&opts.scope_id);
    SESSION.with(|s| {
        *s.borrow_mut() = Some(DragSession {
            capture_id: opts.capture_id,
            scope_id: opts.scope_id,
            draggable_id: opts.draggable_id,
            phase: DragPhase::Pending,
            start_pointer: opts.pointer,
            last_pointer: opts.pointer,
            grab_offset: opts.grab_offset,
            start_bounds: opts.start_bounds,
            from_list: opts.from_list,
            from_index: opts.from_index,
            item_type: opts.item_type,
            group: opts.group,
            axis,
            motion_axis,
            start_axis,
            threshold,
            restrict,
            restrict_end_only,
            restrict_anchor,
            snap,
            delay_ms,
            activation,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone,
            data,
            sort_mode,
            items,
        });
    });
}

pub struct BeginDrag {
    pub scope_id: String,
    pub draggable_id: String,
    pub pointer: [f32; 2],
    pub grab_offset: [f32; 2],
    pub start_bounds: [f32; 4],
    pub from_list: Option<String>,
    pub from_index: Option<usize>,
    pub item_type: String,
    pub group: String,
    pub capture_id: String,
}

fn live_source_slot(sess: &DragSession, hits: &[DragHit]) -> Option<Rect> {
    hits.iter()
        .find(|h| h.is_draggable && h.id == sess.draggable_id && h.scope_id == sess.scope_id)
        .map(|h| h.rect)
        .filter(|r| r.is_usable())
}

fn source_translation(sess: &DragSession, origin: [f32; 2], hits: &[DragHit]) -> [f32; 2] {
    // Scroll moves the source's screen-space slot without changing its authored
    // left/top. Both preview translate and persisted delta use that live slot.
    let slot = live_source_slot(sess, hits)
        .map(|r| [r.x, r.y])
        .unwrap_or([sess.start_bounds[0], sess.start_bounds[1]]);
    [origin[0] - slot[0], origin[1] - slot[1]]
}

fn ghost_origin(
    sess: &DragSession,
    pointer: [f32; 2],
    viewport: [f32; 2],
    hits: &[DragHit],
    at_end: bool,
) -> [f32; 2] {
    let mut ctx = DragModifierCtx {
        pointer,
        start_pointer: sess.start_pointer,
        grab_offset: sess.grab_offset,
        start_bounds: sess.start_bounds,
        viewport,
        axis: sess.motion_axis,
        snap: sess.snap.clone(),
        restrict: resolve_live_restrict(&sess.restrict, &sess.scope_id, &sess.draggable_id, hits),
        live_slot: live_source_slot(sess, hits),
        at_end,
        restrict_end_only: sess.restrict_end_only,
        restrict_anchor: sess.restrict_anchor,
    };
    let align = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|s| s.overlay_align)
            .unwrap_or(OverlayAlign::Grab)
    });
    if align == OverlayAlign::Center {
        // Treat the grab point as the ghost midpoint so axis / restrict / snap
        // operate on the chip that follows the pointer, not the source box.
        let [gw, gh] = resolved_overlay_ghost_size(sess);
        ctx.grab_offset = [gw * 0.5, gh * 0.5];
        ctx.start_bounds = [
            sess.start_pointer[0] - gw * 0.5,
            sess.start_pointer[1] - gh * 0.5,
            gw.max(1.0),
            gh.max(1.0),
        ];
    }
    let mut pipeline = builtin_modifiers();
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            pipeline.extend(rt.modifiers.iter().cloned());
        }
    });
    EXTRA_MODIFIERS.with(|m| {
        if let Some(extra) = m.borrow().get(&sess.scope_id) {
            for t in extra {
                pipeline.retain(|x| x.id != t.id);
                pipeline.push(t.clone());
            }
        }
    });
    let delta = DragDelta {
        x: pointer[0] - sess.start_pointer[0],
        y: pointer[1] - sess.start_pointer[1],
    };
    let origin = ctx.origin_of(apply_modifiers(delta, &pipeline, &ctx));
    let overflow = scope_restrict_overflow(&sess.scope_id);
    if overflow.is_hidden() && !(sess.restrict_end_only && !at_end) {
        // Failed Element/Parent lookup must keep the last clip box. Clearing it
        // remounts the overlay without a shell, and `write_ghost_origin` then
        // walks the clip shell with the chip — the ghost escapes the parent.
        if let Some(box_) = restrict_clip_bounds(&ctx.restrict, viewport) {
            // Shell geometry is the parent restrict box. Page overflow is a
            // second scissor — shrinking the shell to the viewport let rotation
            // paint in the padding beside the dashed arena after scroll.
            set_restrict_clip(Some(box_));
            if let Some(anc) = restrict_ancestor_clip_for(sess) {
                set_restrict_ancestor_clip(Some(anc));
            }
        }
    } else {
        clear_restrict_clip_state();
    }
    origin
}

/// Gallery stacks two Sortable lists that reuse planet names. Collision
/// `detect` ids are unique *within the session*, but a bare `find(id)` on the
/// full hit list binds hover to the first tree (opacity 0) — dimmed source
/// then looks cross-list and insert-index walks the wrong column (本体丢失 /
/// 松手进队尾).
fn hit_in_scope<'a>(hits: &'a [DragHit], id: &str, scope: &str) -> Option<&'a DragHit> {
    hits.iter().find(|h| h.id == id && h.scope_id == scope)
}

/// Collision / insert box: the **item slot** at the ghost origin.
///
/// `start_bounds` and even `collect_hits` layout boxes can be the stretched
/// `also_droppable` island (`w=100%` in a column / ScrollArea). Using that
/// height as the dragged box puts `center` past the last row — siblings close
/// the origin and the dimmed body only reappears as a tail insert (Gallery
/// 单列 SortableList). Cap to the row pitch. Identity is `(scope, id)`: two
/// Gallery lists reuse planet names.
fn moving_item_rect(sess: &DragSession, hits: &[DragHit], origin: [f32; 2]) -> Rect {
    let from_start = Rect::from_bounds(sess.start_bounds).translated(
        origin[0] - sess.start_bounds[0],
        origin[1] - sess.start_bounds[1],
    );
    let Some(src) = hits
        .iter()
        .find(|h| h.id == sess.draggable_id && h.scope_id == sess.scope_id && h.rect.is_usable())
    else {
        return from_start;
    };
    let mut width = src.rect.width.max(1.0);
    let mut height = src.rect.height.max(1.0);
    if let Some(list) = sess.from_list.as_deref() {
        let stride = slot_extent_along_axis(
            hits,
            list,
            sess.from_index,
            sess.axis,
            match sess.axis {
                Axis::X => width,
                Axis::Y | Axis::Xy | Axis::Start => height,
            },
        );
        match sess.axis {
            Axis::X => width = width.min(stride).max(1.0),
            Axis::Y | Axis::Xy | Axis::Start => height = height.min(stride).max(1.0),
        }
    }
    Rect {
        x: origin[0],
        y: origin[1],
        width,
        height,
    }
}

/// `index=None` list shells have their center at mid-column. `closest-center`
/// then prefers the shell over any row once the pointer is near half the list.
fn closest_alg_skips_indexed_list_shell(alg: CollisionAlg) -> bool {
    matches!(
        alg,
        CollisionAlg::ClosestCenter | CollisionAlg::ClosestCorners
    )
}

fn list_has_indexed_candidate(hits: &[DragHit], sess: &DragSession, list: &str) -> bool {
    hits.iter()
        .any(|h| h.accepts(sess) && h.index.is_some() && h.list_id.as_deref() == Some(list))
}

/// Sortable hover target list: pointer layout only — never collision `drop_hit`
/// (the transformed ghost must not choose the column).
///
/// Nested lists: pick the **innermost** accepting hit (row or shell) that
/// contains the pointer so a parent `SortableItem` wrapping a child list does
/// not steal hover from empty nested padding.
///
/// Outside every accepting list (gap between folders, below the last card):
/// solid-dnd `closestCenter` — nearest existing slot among all lists, not a
/// phantom append hole and not the Kanban x-band of a vertical stack.
fn resolve_sortable_hover_list(
    hits: &[DragHit],
    sess: &DragSession,
    pointer: [f32; 2],
    prev_list: Option<&str>,
) -> Option<String> {
    // Folder-in-folder: the dragged row's box contains its nested list. Stay
    // on `from_list` while the pointer is still in that island.
    if source_layout_contains(sess, hits, pointer) {
        return sess.from_list.clone();
    }
    if let Some(list) = innermost_accepting_shell_at(hits, sess, pointer) {
        return Some(list);
    }
    if let Some(list) = innermost_list_at(hits, sess, pointer) {
        if !pointer_past_list_indexed_content(hits, &list, pointer, sess.axis) {
            return Some(list);
        }
    }
    nearest_list_by_slot(hits, sess, pointer, prev_list).or_else(|| sess.from_list.clone())
}

/// Smallest accepting droppable (indexed row or list shell) under the pointer.
fn innermost_list_at(hits: &[DragHit], sess: &DragSession, pointer: [f32; 2]) -> Option<String> {
    hits.iter()
        .filter(|h| {
            h.accepts(sess)
                && h.rect.is_usable()
                && h.rect.contains_point(pointer)
                && h.list_id.as_ref().is_some_and(|s| !s.is_empty())
        })
        .min_by(|a, b| {
            a.rect
                .area()
                .partial_cmp(&b.rect.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|h| h.list_id.clone())
}

/// Innermost accepting **list shell** (`index=None`) under the pointer.
/// Unlike [`innermost_list_at`], past-the-last-row still counts so Kanban
/// `min_h` padding keeps the column, and nested folder gaps do not.
fn innermost_accepting_shell_at(
    hits: &[DragHit],
    sess: &DragSession,
    pointer: [f32; 2],
) -> Option<String> {
    hits.iter()
        .filter(|h| {
            h.accepts(sess)
                && h.index.is_none()
                && h.rect.is_usable()
                && h.rect.contains_point(pointer)
                && h.list_id.as_ref().is_some_and(|s| !s.is_empty())
        })
        .min_by(|a, b| {
            a.rect
                .area()
                .partial_cmp(&b.rect.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|h| h.list_id.clone())
}

fn list_shell_contains(hits: &[DragHit], list_id: &str, pointer: [f32; 2]) -> bool {
    hits.iter().any(|h| {
        h.list_id.as_deref() == Some(list_id)
            && h.index.is_none()
            && h.rect.is_usable()
            && h.rect.contains_point(pointer)
    })
}

/// Distance from `p` to the closest point on `rect` (0 when inside).
fn point_rect_distance(p: [f32; 2], rect: Rect) -> f32 {
    let dx = if p[0] < rect.left() {
        rect.left() - p[0]
    } else if p[0] > rect.right() {
        p[0] - rect.right()
    } else {
        0.0
    };
    let dy = if p[1] < rect.top() {
        rect.top() - p[1]
    } else if p[1] > rect.bottom() {
        p[1] - rect.bottom()
    } else {
        0.0
    };
    (dx * dx + dy * dy).sqrt()
}

const NEAREST_SLOT_STICKY_PX: f32 = 12.0;

/// Closest existing insert slot among accepting lists (solid-dnd closestCenter
/// when the pointer is not over any container). Empty shells count as index 0.
/// Indexed rows include the lifted source so the vacated hole still competes.
fn nearest_slot_in_list(
    hits: &[DragHit],
    sess: &DragSession,
    list_id: &str,
    pointer: [f32; 2],
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    let mut saw_item = false;
    for h in hits {
        if h.list_id.as_deref() != Some(list_id) || !h.rect.is_usable() {
            continue;
        }
        if let Some(idx) = h.index {
            saw_item = true;
            let d = point_rect_distance(pointer, h.rect);
            match best {
                Some((_, bd)) if d >= bd - 1e-4 => {}
                _ => best = Some((idx, d)),
            }
        }
    }
    if saw_item {
        return best;
    }
    hits.iter()
        .filter(|h| {
            h.accepts(sess)
                && h.list_id.as_deref() == Some(list_id)
                && h.index.is_none()
                && h.rect.is_usable()
        })
        .map(|h| (0usize, point_rect_distance(pointer, h.rect)))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

fn nearest_list_by_slot(
    hits: &[DragHit],
    sess: &DragSession,
    pointer: [f32; 2],
    prev_list: Option<&str>,
) -> Option<String> {
    let mut lists: Vec<String> = Vec::new();
    for h in hits {
        if !h.accepts(sess) {
            continue;
        }
        let Some(list) = h.list_id.as_deref() else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        if !lists.iter().any(|l| l == list) {
            lists.push(list.to_string());
        }
    }
    let mut best: Option<(String, f32)> = None;
    for list in &lists {
        let Some((_, d)) = nearest_slot_in_list(hits, sess, list, pointer) else {
            continue;
        };
        match &best {
            Some((_, bd)) if d >= *bd - 1e-4 => {}
            _ => best = Some((list.clone(), d)),
        }
    }
    let (best_list, best_d) = best?;
    if let Some(prev) = prev_list {
        if prev == best_list.as_str() {
            return Some(best_list);
        }
        if let Some((_, d_prev)) = nearest_slot_in_list(hits, sess, prev, pointer) {
            if d_prev <= best_d + NEAREST_SLOT_STICKY_PX {
                return Some(prev.to_string());
            }
        }
    }
    Some(best_list)
}

fn list_max_index_in_hits(hits: &[DragHit], list_id: &str) -> Option<usize> {
    hits.iter()
        .filter(|h| h.list_id.as_deref() == Some(list_id))
        .filter_map(|h| h.index)
        .max()
}

/// True when the probe is past the last indexed row slot (column padding below chips).
fn pointer_past_list_indexed_content(
    hits: &[DragHit],
    list_id: &str,
    pointer: [f32; 2],
    axis: Axis,
) -> bool {
    let Some(end) = list_indexed_content_end(hits, list_id, axis) else {
        return false;
    };
    let p = match axis {
        Axis::X => pointer[0],
        Axis::Y | Axis::Xy | Axis::Start => pointer[1],
    };
    p >= end
}

fn list_indexed_content_end(hits: &[DragHit], list_id: &str, axis: Axis) -> Option<f32> {
    let mut items: Vec<&DragHit> = hits
        .iter()
        .filter(|h| {
            h.list_id.as_deref() == Some(list_id) && h.index.is_some() && h.rect.is_usable()
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|h| h.index.unwrap_or(0));
    let last = items[items.len() - 1];
    let fallback = match axis {
        Axis::X => last.rect.width,
        Axis::Y | Axis::Xy | Axis::Start => last.rect.height,
    }
    .max(1.0);
    let pitch = slot_extent_along_axis(hits, list_id, last.index, axis, fallback);
    Some(axis_origin(last.rect, axis) + pitch)
}

/// Same-list insert holes are the N layout slots (`0..=max`). `hover = max+1`
/// is Sortable "append after last", but the source is still in the tree — there
/// is no extra row. Siblings with `index <= hover` already vacated the last
/// slot; `insert_hole_origin` then paints the dimmed body one pitch past it
/// (Gallery 单列: 拖到最后再向下一个项高 → 末槽空白). Cross-list keeps `len`
/// so the target can open a real tail slot.
fn clamp_tail_append_hover(
    sess: &DragSession,
    list_id: &str,
    hits: &[DragHit],
    hover: usize,
) -> usize {
    if sess.from_list.as_deref() != Some(list_id) || sess.pull_clone || sess.items.len() > 1 {
        return hover;
    }
    let from = sess.from_index.unwrap_or(hover);
    let max = list_max_index_in_hits(hits, list_id).unwrap_or(from);
    hover.min(max)
}

/// CursorMoved paint-follow: `overlay_pos` + Overlay dirty from [`LAST_HITS`].
/// No tree walk, no collision / sortable `hover_list` — those run on vsync [`tick`].
pub fn tick_follow(x: f32, y: f32, viewport: [f32; 2]) -> bool {
    if !is_active() && !is_coasting() {
        return false;
    }
    crate::utils::pointer_drag::tick(x, y);
    let Some(sess) = SESSION.with(|s| s.borrow().clone()) else {
        return false;
    };
    SESSION.with(|s| {
        if let Some(d) = s.borrow_mut().as_mut() {
            d.last_pointer = [x, y];
            d.last_viewport = viewport;
        }
    });
    if sess.phase == DragPhase::Active {
        VELOCITY.with(|v| v.borrow_mut().sample([x, y], Instant::now()));
    }
    let hits = LAST_HITS.with(|h| h.borrow().clone());
    let origin = ghost_origin(&sess, [x, y], viewport, &hits, false);
    write_ghost_follow(&sess.scope_id, origin);
    true
}

fn write_ghost_follow(scope_id: &str, origin: [f32; 2]) {
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(scope_id) {
            if rt.overlay_pos.peek() != origin {
                rt.overlay_pos.set_silent(origin);
            }
        }
    });
    super::overlay::note_ghost_follow_tick(origin);
}

/// After layout (and the frame's scroll-offset chase), refresh `overlay_pos` +
/// Hidden clip from live visual hits so the clip shell tracks a scrolled parent.
/// Collision / `hover_list` stay on vsync [`tick`] — this must not remount.
pub(crate) fn resync_overlay_follow_after_layout(root: &mut Element) {
    if !is_active() {
        return;
    }
    let Some(sess) = SESSION.with(|s| s.borrow().clone()) else {
        return;
    };
    let hits = collect_hits(root);
    let at_origin = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .is_some_and(|rt| sess.from_list.is_some() && rt.hover_list.peek() == sess.from_list)
    }) && !sess.pull_clone
        && active_sort_mode() != SortMode::Swap;
    let offsets: HashMap<String, [f32; 2]> = if at_origin {
        sess.from_list
            .as_deref()
            .and_then(|list| source_list_layout(&hits, list))
            .unwrap_or_default()
            .iter()
            .filter_map(|slot| {
                let live = hits.iter().find(|h| {
                    h.id == slot.id && h.scope_id == slot.scope_id && h.index == slot.index
                })?;
                slot.index.map(|_| {
                    (
                        slot.id.clone(),
                        [slot.rect.x - live.rect.x, slot.rect.y - live.rect.y],
                    )
                })
            })
            .collect()
    } else {
        HashMap::new()
    };
    sync_source_layout_offsets(root, &sess.scope_id, &offsets);
    SOURCE_LAYOUT_OFFSETS.with(|last| *last.borrow_mut() = offsets);
    LAST_HITS.with(|h| *h.borrow_mut() = hits.clone());
    let origin = ghost_origin(&sess, sess.last_pointer, sess.last_viewport, &hits, false);
    write_ghost_follow(&sess.scope_id, origin);
    // The scroll chase can run after reactive_sync. Apply the same live
    // translation before collect so a held-still source stays inside this frame.
    if !overlay_is_registered(&sess.scope_id) && !sess.pull_clone {
        if let Some(path) = crate::core::tree::find_path_in_tree(root, |el| {
            el.attrs.get(ATTR_DRAG_SCOPE) == Some(&sess.scope_id)
                && el.attrs.get(ATTR_DRAG_DRAGGABLE) == Some(&sess.draggable_id)
                && !el.attrs.contains_key(ATTR_DRAG_SKIP_FOLLOW)
        }) {
            let mut source = &mut *root;
            for &index in &path {
                source = &mut source.children[index];
            }
            let translate = source_translation(&sess, origin, &hits);
            if source.style.translate != translate {
                crate::app::damage::push_frame_paint_element(source, source.paint_scroll_acc);
                source.style.translate = translate;
                crate::utils::motion::clear_motion(source);
                crate::app::damage::push_frame_paint_element(source, source.paint_scroll_acc);
                crate::layout::collect_cache::invalidate_collect_paint_hash_path(root, &path);
            }
        }
    }
}

// Compensate layout on the kit shell, separately from the body's FLIP tween.
// Retargeting that tween as the source slot grows would animate the same move twice.
fn sync_source_layout_offsets(
    root: &mut Element,
    scope: &str,
    offsets: &HashMap<String, [f32; 2]>,
) {
    if root.attrs.contains_key(super::ATTR_DRAG_ITEM_SHELL) {
        let offset = crate::core::tree::find_in_tree(root, |e| {
            e.attrs.contains_key(super::ATTR_DRAG_DRAGGABLE)
        })
        .filter(|e| e.attrs.get(super::ATTR_DRAG_SCOPE).map(String::as_str) == Some(scope))
        .and_then(|e| e.attrs.get(super::ATTR_DRAG_DRAGGABLE))
        .and_then(|id| offsets.get(id))
        .copied()
        .unwrap_or([0.0, 0.0]);
        root.style.translate = offset;
    }
    for child in &mut root.children {
        sync_source_layout_offsets(child, scope, offsets);
    }
}

pub(crate) fn live_source_layout_offset(scope: &str, id: &str) -> [f32; 2] {
    LAST_HITS.with(|hits| {
        let hits = hits.borrow();
        let Some(live) = hits.iter().find(|h| h.scope_id == scope && h.id == id) else {
            return [0.0, 0.0];
        };
        let Some(layout) = live
            .list_id
            .as_deref()
            .and_then(|list| source_list_layout(&hits, list))
        else {
            return [0.0, 0.0];
        };
        layout
            .iter()
            .find(|h| h.id == id && h.index == live.index)
            .map(|slot| [slot.rect.x - live.rect.x, slot.rect.y - live.rect.y])
            .unwrap_or([0.0, 0.0])
    })
}

pub(crate) fn painted_source_layout_offset(id: &str) -> [f32; 2] {
    SOURCE_LAYOUT_OFFSETS.with(|offsets| offsets.borrow().get(id).copied().unwrap_or([0.0, 0.0]))
}

/// Wake a pending drag once time is the only remaining activation condition.
pub(crate) fn activation_next_wake_at() -> Option<Instant> {
    SESSION.with(|s| {
        let session = s.borrow();
        let sess = session.as_ref()?;
        if sess.phase != DragPhase::Pending || sess.delay_ms <= 0.0 {
            return None;
        }
        let dx = sess.last_pointer[0] - sess.start_pointer[0];
        let dy = sess.last_pointer[1] - sess.start_pointer[1];
        let dist = dx.hypot(dy);
        if dist == 0.0
            || !sess.start_axis.allows_start_move(dx, dy)
            || (sess.activation == Activation::And && dist < sess.threshold)
        {
            return None;
        }
        Some(sess.pressed_at + std::time::Duration::from_secs_f32(sess.delay_ms / 1000.0))
    })
}

/// Advance the session. `hits` must be collected from the current layout tree.
/// Returns `true` when the UI should redraw.
pub fn tick(x: f32, y: f32, hits: &[DragHit], viewport: [f32; 2]) -> bool {
    crate::utils::pointer_drag::tick(x, y);
    let coasting = SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|d| d.phase == DragPhase::Coasting)
    });
    if coasting && !COAST_TICK.get() {
        return false;
    }
    let mut hits = hits.to_vec();
    cap_stretched_indexed_rects(&mut hits);
    let from_list = SESSION.with(|s| s.borrow().as_ref().and_then(|sess| sess.from_list.clone()));
    expand_collapsed_empty_list_shells(&mut hits, from_list.as_deref());
    let hits = &hits;
    let Some(mut sess) = SESSION.with(|s| s.borrow().clone()) else {
        return false;
    };
    sess.last_pointer = [x, y];
    sess.last_viewport = viewport;
    SESSION.with(|s| {
        if let Some(d) = s.borrow_mut().as_mut() {
            d.last_pointer = sess.last_pointer;
            d.last_viewport = sess.last_viewport;
        }
    });
    if sess.phase == DragPhase::Active {
        VELOCITY.with(|v| v.borrow_mut().sample([x, y], Instant::now()));
    }
    let dist = {
        let dx = x - sess.start_pointer[0];
        let dy = y - sess.start_pointer[1];
        (dx * dx + dy * dy).sqrt()
    };

    if sess.phase == DragPhase::Pending {
        let waited_ms = sess.pressed_at.elapsed().as_secs_f32() * 1000.0;
        // Press-only (`dist == 0`) stays Pending even when `threshold` / `delay_ms`
        // are 0. Opening the overlay on pointerdown applies `source_opacity` and
        // Auto-stack raise before a ghost exists — `source_opacity=0` then
        // erases the source. A drag *attempt* is the first movement.
        let attempted = dist > 0.0;
        let far_enough = dist >= sess.threshold;
        let long_enough = waited_ms >= sess.delay_ms;
        let activated = attempted
            && match sess.activation {
                Activation::And => far_enough && long_enough,
                Activation::Or => far_enough || long_enough,
            };
        let dx = x - sess.start_pointer[0];
        let dy = y - sess.start_pointer[1];
        // interact cancels the prepared action; we stay Pending so the user can
        // still cross the required axis (beyond interact).
        if !activated || !sess.start_axis.allows_start_move(dx, dy) {
            SESSION.with(|s| *s.borrow_mut() = Some(sess));
            return false;
        }
        sess.phase = DragPhase::Active;
        if sess.axis == Axis::Start && dist >= 1.0 {
            sess.motion_axis = Axis::resolve_start(dx, dy);
        }
        freeze_drag_items(&mut sess, hits);
        // Overlay subscribers run synchronously when activate_scope opens it.
        // Publish the frozen group first so the first ghost has every item.
        SESSION.with(|s| *s.borrow_mut() = Some(sess.clone()));
        // Clip box + origin must exist before `overlay_open` so Hidden overflow
        // mounts a clip shell on the first island rebuild (no extra mousemove).
        let origin = ghost_origin(&sess, [x, y], viewport, hits, false);
        activate_scope(&sess, hits, origin);
        let start_ev = event_from(&sess, None, None, None);
        fire(&sess.scope_id, HandlerKind::Start, &start_ev);
        let accepting: Vec<String> = hits
            .iter()
            .filter(|h| h.is_droppable && h.accepts(&sess))
            .map(|h| h.id.clone())
            .collect();
        super::dropzone::on_drag_activated(&sess.scope_id, &accepting, &start_ev);
    }

    if sess.axis == Axis::Start && sess.motion_axis == Axis::Xy {
        let dx = x - sess.start_pointer[0];
        let dy = y - sess.start_pointer[1];
        if dist >= 1.0 {
            sess.motion_axis = Axis::resolve_start(dx, dy);
        }
    }

    let origin = ghost_origin(&sess, [x, y], viewport, hits, false);
    let dragged = moving_item_rect(&sess, hits, origin);

    // Paint-follow first (same as Point overlay tick): `overlay_pos` + Overlay
    // dirty before collision / sortable, so CursorMoved vsync already dest-resets.
    write_ghost_follow(&sess.scope_id, origin);

    let (collision, empty_insert, can_drop, collision_fn) = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|s| {
                (
                    s.collision,
                    s.empty_insert,
                    s.can_drop.clone(),
                    s.collision_fn.clone(),
                )
            })
            .unwrap_or((CollisionAlg::default(), 0.0, None, None))
    });
    let skip_shell = closest_alg_skips_indexed_list_shell(collision);
    let candidates: Vec<Candidate> = hits
        .iter()
        .filter(|h| h.accepts(&sess) && h.rect.is_usable())
        .filter(|h| {
            if !skip_shell || h.index.is_some() {
                return true;
            }
            let Some(list) = h.list_id.as_deref() else {
                return true;
            };
            !list_has_indexed_candidate(hits, &sess, list)
        })
        .filter_map(|h| {
            let rect = if h.index.is_none() && empty_insert > 0.0 {
                h.rect.inflated(empty_insert)
            } else {
                h.rect
            };
            let overlap = super::dropzone::overlap_for(&sess.scope_id, &h.id);
            if !overlap.passes([x, y], dragged, rect) {
                return None;
            }
            Some(Candidate {
                id: h.id.clone(),
                rect,
            })
        })
        .collect();
    let prev_drop = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .and_then(|s| s.active_droppable.peek())
    });
    let mut next_id = detect_with(
        collision,
        collision_fn.as_ref(),
        CollisionInput {
            pointer: [x, y],
            dragged,
            droppables: &candidates,
            active_id: prev_drop.as_deref(),
            draggable_id: sess.draggable_id.as_str(),
            skip_innermost: false,
        },
    );
    let mut drop_hit = next_id
        .as_ref()
        .and_then(|id| hit_in_scope(hits, id, &sess.scope_id));
    if let (Some(pred), Some(hit)) = (can_drop.as_ref(), drop_hit) {
        let probe = event_from(&sess, Some(hit), hit.list_id.clone(), hit.index);
        if !pred(&probe) {
            next_id = None;
            drop_hit = None;
        }
    }
    if let (Some(id), Some(hit)) = (next_id.as_ref(), drop_hit) {
        if let Some(checker) = super::dropzone::checker_for(&sess.scope_id, id) {
            let probe = event_from(&sess, Some(hit), hit.list_id.clone(), hit.index);
            if !checker(&probe, true) {
                next_id = None;
                drop_hit = None;
            }
        }
    }

    LAST_HITS.with(|h| *h.borrow_mut() = hits.to_vec());

    // Copy handles out first: `.set` may flush overlay islands, which must
    // not run under `SCOPES.borrow()` (same as [`activate_scope`]). One
    // `batch` so hover list / index / highlight do not remount twice.
    let ui = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|rt| (rt.active_droppable, rt.hover_list, rt.hover_index))
    });
    if let Some((active_droppable, hover_list_sig, hover_index_sig)) = ui {
        let prev_list = hover_list_sig.peek();
        let mut hover_list = resolve_sortable_hover_list(hits, &sess, [x, y], prev_list.as_deref());
        // `can_drop` (Sortable `onMove`) must also veto sortable hover — otherwise
        // Gallery Locked still parks in the empty column: active_droppable is
        // cleared but hole preview / apply_transfer follow hover_list.
        if let (Some(pred), Some(list)) = (can_drop.as_ref(), hover_list.as_deref()) {
            if !list_passes_can_drop(pred, &sess, hits, list) {
                hover_list = sess.from_list.clone();
            }
        }
        // Probe with the pointer, not `dragged.center()`. A stretched
        // island box puts the center past the last row (Gallery: 松手永远进队尾).
        let prev_hover = hover_index_sig.peek();
        let hover_index = hover_list
            .as_deref()
            .and_then(|list| {
                if sess.sort_mode == SortMode::Swap {
                    resolve_swap_hover_index(hits, list, [x, y])
                } else if sess.items.len() > 1 {
                    let restored = source_list_layout(hits, list);
                    let slots = restored.as_deref().unwrap_or(hits);
                    insert_index_along_axis(
                        slots,
                        list,
                        [x, y],
                        sort_axis_for_list(slots, list, sess.axis),
                    )
                    .or_else(|| nearest_end_insert_slot(slots, &sess, list, [x, y]))
                } else if prev_list.is_some()
                    && prev_list.as_deref() != Some(list)
                    && sess.from_list.as_deref() == Some(list)
                    && !sess.pull_clone
                {
                    resolve_source_reentry_index(hits, &sess, list, [x, y])
                } else {
                    resolve_sortable_hover_index(hits, &sess, list, [x, y], prev_hover)
                }
            })
            .or(sess.from_index);
        let drop_changed = active_droppable.peek() != next_id;
        let list_changed = hover_list_sig.peek() != hover_list;
        let index_changed = hover_index_sig.peek() != hover_index;
        if drop_changed || list_changed || index_changed {
            batch(|| {
                if drop_changed {
                    active_droppable.set(next_id.clone());
                }
                if list_changed {
                    hover_list_sig.set(hover_list.clone());
                }
                if index_changed {
                    hover_index_sig.set(hover_index);
                }
            });
        }
    }

    let (to_list, to_index) = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|s| (s.hover_list.peek(), s.hover_index.peek()))
            .unwrap_or((None, None))
    });
    let ev = event_from(&sess, drop_hit, to_list, to_index);
    fire(&sess.scope_id, HandlerKind::Move, &ev);
    if prev_drop != next_id {
        fire(&sess.scope_id, HandlerKind::Over, &ev);
    }
    super::dropzone::on_hover_change(
        &sess.scope_id,
        prev_drop.as_deref(),
        next_id.as_deref(),
        &ev,
    );

    SESSION.with(|s| *s.borrow_mut() = Some(sess));
    true
}

/// Swap mode: hover is the **item** under the pointer, never an insert slot.
fn resolve_swap_hover_index(hits: &[DragHit], list_id: &str, pointer: [f32; 2]) -> Option<usize> {
    let items = indexed_list_items(hits, list_id);
    if items.is_empty() {
        return None;
    }
    for h in &items {
        if h.rect.contains_point(pointer) {
            return h.index;
        }
    }
    items
        .iter()
        .min_by(|a, b| {
            point_rect_distance(pointer, a.rect)
                .partial_cmp(&point_rect_distance(pointer, b.rect))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|h| h.index)
}

/// Reenter the source using the remaining rows of its currently vacated pack.
fn resolve_source_reentry_index(
    hits: &[DragHit],
    sess: &DragSession,
    list: &str,
    pointer: [f32; 2],
) -> Option<usize> {
    let from = sess.from_index?;
    // The lifted row has no slot in the vacated pack. Probe only the remaining
    // rows, then convert the insert slot back to a same-list reorder index.
    let remaining: Vec<DragHit> = hits
        .iter()
        .filter(|h| {
            h.list_id.as_deref() == Some(list)
                && h.scope_id == sess.scope_id
                && h.index != Some(from)
        })
        .cloned()
        .collect();
    let axis = sort_axis_for_list(hits, list, sess.axis);
    let raw = insert_index_along_axis(&remaining, list, pointer, axis)
        .or_else(|| {
            let items = indexed_list_items(&remaining, list);
            let first = items.first()?;
            if axis_origin(first.rect, axis)
                > match axis {
                    Axis::X => pointer[0],
                    _ => pointer[1],
                }
            {
                first.index
            } else {
                items.last()?.index.map(|i| i + 1)
            }
        })
        .unwrap_or(0);
    Some(raw.saturating_sub(usize::from(raw > from)))
}

/// Sortable insert index: midpoint row split, then pin below the opened hole.
fn resolve_sortable_hover_index(
    hits: &[DragHit],
    sess: &DragSession,
    list_id: &str,
    pointer: [f32; 2],
    prev_hover: Option<usize>,
) -> Option<usize> {
    let restored = source_list_layout(hits, list_id);
    let hits = restored.as_deref().unwrap_or(hits);
    // Pre-normalize layout bounds (remembered at collect). While the pointer is
    // still on the lifted chip, keep hover=from — slot math alone can read
    // append when pitch < chip height (Gallery Kanban dimmed body pushed down).
    if source_layout_contains(sess, hits, pointer) {
        return sess.from_index;
    }
    let axis = sort_axis_for_list(hits, list_id, sess.axis);
    let in_shell = list_shell_contains(hits, list_id, pointer);
    let slots_ok = list_slots_usable(hits, list_id, axis);
    // Insert slots are `0..=len` (before each row, and after the last). Picking
    // the nearest *item index* maps "below Gamma" onto slot 0 and opens a hole
    // under the folder title (Gallery: 移出 Later → 标题与 Gamma 空一行).
    match insert_index_along_axis(hits, list_id, pointer, axis) {
        Some(hover) => {
            let hover = clamp_tail_append_hover(sess, list_id, hits, hover);
            Some(pin_hover_below_opened_hole(
                hits, list_id, sess, pointer, prev_hover, hover,
            ))
        }
        None if in_shell && slots_ok => {
            match list_max_index_in_hits(hits, list_id) {
                Some(max) => Some(clamp_tail_append_hover(sess, list_id, hits, max + 1)),
                // Empty nested Inbox: no indexed rows, so `?` used to yield
                // `None` and `tick` filled `from_index` (the origin list).
                None => Some(0),
            }
        }
        None if slots_ok => nearest_end_insert_slot(hits, sess, list_id, pointer),
        None => hover_from_press_box(sess, pointer),
    }
}

/// Clustered local slots (all origins equal) cannot split rows. Compare the
/// page-space pointer to the press box: above the lifted chip opens the head
/// slot so Inbox `Beta` → folder title commits `[Beta, Alpha]` instead of
/// snapping back to `from_index`.
fn hover_from_press_box(sess: &DragSession, pointer: [f32; 2]) -> Option<usize> {
    let from = sess.from_index?;
    let p = match sess.axis {
        Axis::X => pointer[0],
        Axis::Y | Axis::Xy | Axis::Start => pointer[1],
    };
    let origin = match sess.axis {
        Axis::X => sess.start_bounds[0],
        Axis::Y | Axis::Xy | Axis::Start => sess.start_bounds[1],
    };
    if p + 0.5 < origin {
        Some(0)
    } else {
        Some(from)
    }
}

fn list_slots_usable(hits: &[DragHit], list_id: &str, axis: Axis) -> bool {
    let items = indexed_list_items(hits, list_id);
    if items.len() <= 1 {
        return true;
    }
    let first = axis_origin(items[0].rect, axis);
    let last = axis_origin(items[items.len() - 1].rect, axis);
    let avg = (last - first).abs() / (items.len() - 1) as f32;
    avg >= MIN_SLOT_PITCH
}

/// When the pointer is past every `[origin, origin+2*pitch)` band, choose the
/// closer of the head slot (`0`) and the tail slot (`len`).
fn nearest_end_insert_slot(
    hits: &[DragHit],
    sess: &DragSession,
    list_id: &str,
    pointer: [f32; 2],
) -> Option<usize> {
    let items = indexed_list_items(hits, list_id);
    if items.is_empty() {
        return Some(0);
    }
    let p = match sess.axis {
        Axis::X => pointer[0],
        Axis::Y | Axis::Xy | Axis::Start => pointer[1],
    };
    let first = axis_origin(items[0].rect, sess.axis);
    let last_h = items[items.len() - 1];
    let last = axis_origin(last_h.rect, sess.axis);
    let fallback = axis_size(last_h.rect, sess.axis).max(1.0);
    let pitch = slot_extent_along_axis(hits, list_id, last_h.index, sess.axis, fallback);
    let tail = last + pitch;
    if (p - tail).abs() < (p - first).abs() {
        let max = list_max_index_in_hits(hits, list_id)?;
        Some(clamp_tail_append_hover(sess, list_id, hits, max + 1))
    } else {
        items[0].index
    }
}

/// While the pointer is still within one pitch below the preview hole, do not
/// bump `hover` — siblings already opened that slot; advancing early leaves a
/// blank row and the dimmed body chases the overlay (Gallery Mars → Jupiter).
fn pin_hover_below_opened_hole(
    hits: &[DragHit],
    list_id: &str,
    sess: &DragSession,
    pointer: [f32; 2],
    prev_hover: Option<usize>,
    computed: usize,
) -> usize {
    let prev = match prev_hover {
        Some(h) if h != sess.from_index.unwrap_or(h) => h,
        _ => return computed,
    };
    if sess.from_list.as_deref() != Some(list_id) || computed <= prev {
        return computed;
    }
    let from = sess.from_index.unwrap_or(prev);
    if from >= prev {
        return computed;
    }
    let fallback = match sess.axis {
        Axis::X => sess.start_bounds[2].max(1.0),
        Axis::Y | Axis::Xy | Axis::Start => sess.start_bounds[3].max(1.0),
    };
    let pitch = slot_extent_along_axis(hits, list_id, sess.from_index, sess.axis, fallback);
    let Some(hole_pt) = insert_hole_origin(hits, list_id, Some(prev), sess.axis, pitch) else {
        return computed;
    };
    let p = match sess.axis {
        Axis::X => pointer[0],
        Axis::Y | Axis::Xy | Axis::Start => pointer[1],
    };
    let hole = match sess.axis {
        Axis::X => hole_pt[0],
        Axis::Y | Axis::Xy | Axis::Start => hole_pt[1],
    };
    if p < hole + pitch {
        prev
    } else {
        computed
    }
}

fn pointer_on_other_origin_row(sess: &DragSession, hits: &[DragHit], pointer: [f32; 2]) -> bool {
    let Some(list) = sess.from_list.as_deref() else {
        return false;
    };
    hits.iter().any(|h| {
        h.list_id.as_deref() == Some(list)
            && h.index.is_some()
            && h.index != sess.from_index
            && h.id != sess.draggable_id
            && h.rect.is_usable()
            && h.rect.contains_point(pointer)
    })
}

fn source_layout_contains(sess: &DragSession, hits: &[DragHit], pointer: [f32; 2]) -> bool {
    // The lifted island (or its `w=100%` paint box) can still cover Alpha
    // while the pointer is already on that row. That is a hover on Alpha.
    if pointer_on_other_origin_row(sess, hits, pointer) {
        return false;
    }
    let slots_ok = sess
        .from_list
        .as_deref()
        .is_some_and(|list| list_slots_usable(hits, list, sess.axis));
    if !slots_ok {
        // Clustered local slots (all y=0) vs page-space pointer: the press box
        // is the only trustworthy grab island.
        return Rect::from_bounds(sess.start_bounds).contains_point(pointer);
    }
    // Usable row pitch: the tick's capped hit slot, never `last_draggable_bounds`
    // / raw `start_bounds` (those can be the `w=100%` paint box covering Alpha
    // while dragging Beta).
    let Some(hit) = hits.iter().find(|h| {
        h.id == sess.draggable_id && (h.scope_id.is_empty() || h.scope_id == sess.scope_id)
    }) else {
        let mut rect = Rect::from_bounds(sess.start_bounds);
        cap_rect_to_row_stride(&mut rect, sess, hits);
        return rect.contains_point(pointer);
    };
    let mut rect = hit.rect;
    cap_rect_to_row_stride(&mut rect, sess, hits);
    rect.contains_point(pointer)
}

fn cap_rect_to_row_stride(rect: &mut Rect, sess: &DragSession, hits: &[DragHit]) {
    let Some(list) = sess.from_list.as_deref() else {
        return;
    };
    let layout_extent = axis_size(*rect, sess.axis).max(1.0);
    let stride = slot_extent_along_axis(hits, list, sess.from_index, sess.axis, layout_extent);
    if layout_extent > stride * STRETCH_RATIO {
        match sess.axis {
            Axis::X => rect.width = stride,
            Axis::Y | Axis::Xy | Axis::Start => rect.height = stride,
        }
    }
}

/// Cap stretched `also_droppable` island bounds to one row pitch for FLIP shift.
fn capped_drag_extent(sess: &DragSession, hits: &[DragHit]) -> [f32; 2] {
    let mut w = sess.start_bounds[2].max(1.0);
    let mut h = sess.start_bounds[3].max(1.0);
    if let Some(list) = sess.from_list.as_deref() {
        let layout = match sess.axis {
            Axis::X => w,
            Axis::Y | Axis::Xy | Axis::Start => h,
        };
        let slot = slot_extent_along_axis(hits, list, sess.from_index, sess.axis, layout);
        let v = if layout > slot * STRETCH_RATIO {
            slot
        } else {
            layout
        };
        match sess.axis {
            Axis::X => w = v.max(1.0),
            Axis::Y | Axis::Xy | Axis::Start => h = v.max(1.0),
        }
    }
    [w, h]
}

fn activate_scope(sess: &DragSession, hits: &[DragHit], origin: [f32; 2]) {
    let extent = capped_drag_extent(sess, hits);
    let chip = match sess.axis {
        Axis::X => extent[0],
        Axis::Y | Axis::Xy | Axis::Start => extent[1],
    };
    let pitch = row_pitch_along(hits, sess.from_list.as_deref(), sess.axis, chip);
    DRAG_PACK_PITCH.set(pitch);
    let gap = sess
        .from_list
        .as_deref()
        .map(|list| inferred_list_gap(&indexed_list_items(hits, list), sess.axis))
        .unwrap_or(0.0)
        .max(0.0);
    DRAG_PACK_GAP.set(gap);
    SOURCE_LIST_LAYOUT.with(|layout| {
        *layout.borrow_mut() = hits
            .iter()
            .filter(|h| {
                sess.from_list.is_some()
                    && h.list_id == sess.from_list
                    && h.scope_id == sess.scope_id
            })
            .cloned()
            .collect();
    });
    // Copy signal handles out first: `.set` flushes the overlay island, which
    // must not run under `SCOPES.borrow()` (`with_scope` is `borrow_mut`).
    let ui = SCOPES.with(|m| {
        m.borrow().get(&sess.scope_id).map(|rt| {
            (
                rt.active_draggable,
                rt.from_list,
                rt.from_index,
                rt.drag_extent,
                rt.overlay_pos,
                rt.overlay_open,
                rt.overlay_registered,
            )
        })
    });
    if let Some((
        active,
        from_list,
        from_index,
        drag_extent,
        overlay_pos,
        overlay_open,
        registered,
    )) = ui
    {
        // One flush, same as [`reset_scope_ui`]. Sequential `.set` remounted the
        // overlay while `open` was still false (empty 0×0), then again when
        // `open` flipped — a miss on that second pass hid the source with no ghost.
        batch(|| {
            active.set(Some(sess.draggable_id.clone()));
            from_list.set(sess.from_list.clone());
            from_index.set(sess.from_index);
            drag_extent.set(extent);
            overlay_pos.set_silent(origin);
            if registered {
                overlay_open.set(true);
            }
        });
    }
    crate::app::set_pointer_capture(sess.capture_id.clone());
}

enum HandlerKind {
    Start,
    Move,
    Over,
    End,
}

fn fire(scope_id: &str, kind: HandlerKind, ev: &DragEvent) {
    let handler = SCOPES.with(|m| {
        m.borrow().get(scope_id).and_then(|s| match kind {
            HandlerKind::Start => s.on_start.clone(),
            HandlerKind::Move => s.on_move.clone(),
            HandlerKind::Over => s.on_over.clone(),
            HandlerKind::End => s.on_end.clone(),
        })
    });
    if let Some(h) = handler {
        h.borrow_mut()(ev);
    }
}

fn reset_scope_ui(rt: &ScopeRuntime) {
    // One transition: overlay / active / hover are one session, not six effects.
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    SOURCE_LIST_LAYOUT.with(|layout| layout.borrow_mut().clear());
    SOURCE_LAYOUT_OFFSETS.with(|offsets| offsets.borrow_mut().clear());
    clear_restrict_clip_state();
    super::overlay::clear_ghost_follow();
    batch(|| {
        rt.overlay_open.set(false);
        rt.active_draggable.set(None);
        rt.active_droppable.set(None);
        rt.from_list.set(None);
        rt.from_index.set(None);
        rt.hover_list.set(None);
        rt.hover_index.set(None);
    });
}

/// Finish the session and fire `on_drag_end` when it had become active.
///
/// Free-move scopes with [`.inertia`](super::builder::DragScopeBuilder::inertia)
/// enter [`DragPhase::Coasting`] instead of committing, so the ghost flings.
/// Sortable (`from_list`) always commits — FLIP must not fight a coast.
pub fn end(hits: &[DragHit]) -> bool {
    if !is_coasting() {
        let can_coast = SESSION.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|sess| sess.phase == DragPhase::Active && sess.from_list.is_none())
        });
        if can_coast {
            let sess = SESSION.with(|s| s.borrow().clone()).unwrap();
            if try_start_coast(&sess, hits) {
                SESSION.with(|s| {
                    if let Some(d) = s.borrow_mut().as_mut() {
                        d.phase = DragPhase::Coasting;
                    }
                });
                crate::utils::pointer_drag::clear();
                crate::app::release_pointer_capture();
                LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
                return true;
            }
        }
    }
    commit_end(hits)
}

fn try_start_coast(sess: &DragSession, hits: &[DragHit]) -> bool {
    let opts = scope_inertia(&sess.scope_id);
    let velocity = VELOCITY.with(|v| v.borrow().velocity());
    if let Some(i) = Inertia::try_fling(velocity, opts) {
        COAST.with(|c| *c.borrow_mut() = Some(i));
        return true;
    }
    let need_end_mod =
        sess.snap.as_ref().map(|s| s.end_only()).unwrap_or(false) || sess.restrict_end_only;
    if !need_end_mod {
        return false;
    }
    let from = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|r| r.overlay_pos.peek())
            .unwrap_or([sess.start_bounds[0], sess.start_bounds[1]])
    });
    let to = ghost_origin(sess, sess.last_pointer, sess.last_viewport, hits, true);
    if let Some(i) = Inertia::try_smooth_end(from, to, opts) {
        COAST.with(|c| *c.borrow_mut() = Some(i));
        true
    } else {
        false
    }
}

fn commit_end(hits: &[DragHit]) -> bool {
    let Some(mut sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        return false;
    };
    crate::utils::pointer_drag::clear();
    crate::app::release_pointer_capture();
    VELOCITY.with(|v| v.borrow_mut().reset());
    COAST.with(|c| *c.borrow_mut() = None);
    LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
    let was_active = sess.phase == DragPhase::Active || sess.phase == DragPhase::Coasting;
    let ev = if was_active {
        sess.phase = DragPhase::Active;
        // Commit the modifier result, including ordinary snap / restrict;
        // raw pointer travel can extend past the clamped preview at an edge.
        let end_origin = ghost_origin(&sess, sess.last_pointer, sess.last_viewport, hits, true);
        SCOPES.with(|m| {
            if let Some(rt) = m.borrow().get(&sess.scope_id) {
                if rt.overlay_pos.peek() != end_origin {
                    rt.overlay_pos.set_silent(end_origin);
                }
            }
        });
        let drop_id = SCOPES.with(|m| {
            m.borrow()
                .get(&sess.scope_id)
                .and_then(|s| s.active_droppable.peek())
        });
        let drop_hit = drop_id
            .as_ref()
            .and_then(|id| hit_in_scope(hits, id, &sess.scope_id));
        let (to_list, to_index) = SCOPES.with(|m| {
            m.borrow()
                .get(&sess.scope_id)
                .map(|s| (s.hover_list.peek(), s.hover_index.peek()))
                .unwrap_or((None, None))
        });
        Some(event_from_delta(
            &sess,
            drop_hit,
            to_list,
            to_index,
            Some(source_translation(&sess, end_origin, hits)),
        ))
    } else {
        try_click_select(&sess.scope_id, &sess.draggable_id, hits);
        None
    };
    // Clear live preview before `on_drag_end` mutates the Vec, so the rebuild
    // does not apply a stale gap / source dim.
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            reset_scope_ui(rt);
        }
    });
    if let Some(ev) = ev {
        if let Some(id) = ev.droppable_id.as_deref() {
            super::dropzone::on_drop(&sess.scope_id, id, &ev);
        }
        super::dropzone::on_drag_deactivated(&sess.scope_id, &ev);
        fire(&sess.scope_id, HandlerKind::End, &ev);
    }
    was_active
}

/// Advance a [`DragPhase::Coasting`] fling / smoothEnd. Call from vsync / wait.
pub fn pump_inertia(root: &Element, viewport: [f32; 2]) -> bool {
    if !is_coasting() {
        LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
        return false;
    }
    let dt = LAST_COAST_AT.with(|slot| {
        let now = Instant::now();
        let dt = slot
            .borrow()
            .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        *slot.borrow_mut() = Some(now);
        dt
    });
    let dt = if dt < 1.0 / 240.0 {
        1.0 / 60.0
    } else {
        dt.min(0.05)
    };
    let Some(mut inertia) = COAST.with(|c| c.borrow_mut().take()) else {
        return commit_end(&collect_hits(root));
    };
    match inertia.step(dt) {
        Some(dxy) => {
            COAST.with(|c| *c.borrow_mut() = Some(inertia));
            let next = SESSION.with(|s| {
                let mut b = s.borrow_mut();
                if let Some(sess) = b.as_mut() {
                    sess.last_pointer[0] += dxy[0];
                    sess.last_pointer[1] += dxy[1];
                    sess.last_pointer
                } else {
                    [0.0, 0.0]
                }
            });
            let hits = collect_hits(root);
            COAST_TICK.set(true);
            let dirty = tick(next[0], next[1], &hits, viewport);
            COAST_TICK.set(false);
            dirty
        }
        None => commit_end(&collect_hits(root)),
    }
}

pub fn clear_if_any() {
    if is_dragging() {
        let _ = end(&[]);
    }
}

/// Abort without firing `on_drag_end` (Escape). SortableJS has no first-class cancel.
pub fn cancel() -> bool {
    let Some(sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        return false;
    };
    clear_restrict_clip_state();
    crate::utils::pointer_drag::clear();
    crate::app::release_pointer_capture();
    VELOCITY.with(|v| v.borrow_mut().reset());
    COAST.with(|c| *c.borrow_mut() = None);
    LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            reset_scope_ui(rt);
        }
    });
    if sess.phase == DragPhase::Active || sess.phase == DragPhase::Coasting {
        let ev = event_from(&sess, None, None, None);
        super::dropzone::on_drag_deactivated(&sess.scope_id, &ev);
    }
    true
}

pub fn last_pointer() -> Option<[f32; 2]> {
    SESSION.with(|s| s.borrow().as_ref().map(|d| d.last_pointer))
}

/// Keyboard: move the insert index along the active list.
pub fn nudge_insert(delta: i32) -> bool {
    if delta == 0 || !is_active() {
        return false;
    }
    let Some(sess) = SESSION.with(|s| s.borrow().clone()) else {
        return false;
    };
    let list = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .and_then(|s| s.hover_list.peek())
            .or_else(|| sess.from_list.clone())
    });
    let Some(list) = list else {
        return false;
    };
    let hits = LAST_HITS.with(|h| h.borrow().clone());
    let mut indices: Vec<usize> = hits
        .iter()
        .filter(|h| h.list_id.as_deref() == Some(list.as_str()) && h.index.is_some())
        .filter_map(|h| h.index)
        .collect();
    indices.sort_unstable();
    indices.dedup();
    if indices.is_empty() {
        return false;
    }
    let cur = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .and_then(|s| s.hover_index.peek())
            .or(sess.from_index)
            .unwrap_or(indices[0])
    });
    let pos = indices.iter().position(|&i| i == cur).unwrap_or(0);
    let next_pos = (pos as i32 + delta).clamp(0, indices.len() as i32 - 1) as usize;
    let next = indices[next_pos];
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            rt.hover_index.set(Some(next));
            rt.hover_list.set(Some(list));
        }
    });
    crate::app::runtime::request_redraw();
    true
}

fn locked_axis_drift(sess: &DragSession, hits: &[DragHit], overlay_pos: [f32; 2]) -> bool {
    let Some(slot) = live_source_slot(sess, hits) else {
        return false;
    };
    match sess.motion_axis {
        Axis::Y => (slot.x - overlay_pos[0]).abs() > 0.5,
        Axis::X => (slot.y - overlay_pos[1]).abs() > 0.5,
        Axis::Xy | Axis::Start => false,
    }
}

/// Continue edge auto-scroll while the pointer is held still (vsync / wait).
///
/// After a nudge (or a parent `scroll_behavior=smooth` chase) the ghost must
/// be ticked against live hits: `window_events` collects hits *before*
/// autoscroll, and vsync pumps scroll with no mousemove. `axis="y"` would
/// otherwise freeze overlay X while Gallery `scroll-overlays` pans.
pub fn pump_autoscroll(root: &Element, viewport: [f32; 2]) -> bool {
    if !is_active() {
        LAST_SCROLL_AT.with(|t| *t.borrow_mut() = None);
        return false;
    }
    let Some(pointer) = last_pointer() else {
        return false;
    };
    let (cfg, axis, overlay_pos) = SESSION.with(|s| {
        let sess = s.borrow();
        let Some(d) = sess.as_ref() else {
            return (AutoScroll::default(), Axis::Xy, [0.0, 0.0]);
        };
        let (cfg, overlay_pos) = SCOPES.with(|m| {
            m.borrow()
                .get(&d.scope_id)
                .map(|r| (r.autoscroll, r.overlay_pos.peek()))
                .unwrap_or((
                    AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    [0.0, 0.0],
                ))
        });
        (cfg, d.motion_axis, overlay_pos)
    });
    let moved = if cfg.enabled {
        LAST_SCROLL_AT.with(|slot| pump_at(root, pointer, cfg, axis, slot))
    } else {
        LAST_SCROLL_AT.with(|slot| {
            slot.borrow_mut().take();
        });
        false
    };
    // XY has no locked-axis drift. Skip a second full-tree `collect_hits` on
    // every mousemove when autoscroll did not actually pan (Gallery drag 不跟手).
    if !moved && !matches!(axis, Axis::X | Axis::Y) {
        return false;
    }
    let hits = collect_hits(root);
    let drift = SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .map(|sess| locked_axis_drift(sess, &hits, overlay_pos))
            .unwrap_or(false)
    });
    if moved || drift {
        tick(pointer[0], pointer[1], &hits, viewport);
        return true;
    }
    false
}

/// Walk the laid-out tree and collect drag targets (skip the drag ghost).
///
/// Rects are pointer-space **layout slots** (scroll applied, CSS translate not).
/// Do not use raw `layout_bounds` (pre-scroll) or `visual_hit_box` (includes
/// sortable preview translate).
pub fn collect_hits(root: &Element) -> Vec<DragHit> {
    NAMED_BOUNDS.with(|m| m.borrow_mut().clear());
    PARENT_BOUNDS.with(|m| m.borrow_mut().clear());
    ITEM_PACK_GAPS.with(|m| m.borrow_mut().clear());
    ANCESTOR_OVERFLOW_CLIP.with(|m| m.borrow_mut().clear());
    let mut out = Vec::new();
    collect_hits_inner(root, [0.0, 0.0], None, None, &mut out);
    cap_stretched_indexed_rects(&mut out);
    super::registry::sync_registered_from_collect(&out);
    out
}

/// `also_droppable` + `w=100%` islands report layout bounds taller than the
/// row pitch. Hits must be **slot** rects (origin + pitch), not paint extent.
fn cap_stretched_indexed_rects(hits: &mut Vec<DragHit>) {
    use std::collections::HashSet;
    let lists: HashSet<String> = hits
        .iter()
        .filter(|h| h.index.is_some())
        .filter_map(|h| h.list_id.clone())
        .collect();
    for list in lists {
        let mut idxs: Vec<usize> = hits
            .iter()
            .enumerate()
            .filter(|(_, h)| h.list_id.as_deref() == Some(list.as_str()) && h.index.is_some())
            .map(|(i, _)| i)
            .collect();
        if idxs.len() < 2 {
            continue;
        }
        idxs.sort_by_key(|i| hits[*i].index.unwrap_or(0));
        for w in 0..idxs.len() - 1 {
            let i0 = idxs[w];
            let i1 = idxs[w + 1];
            let stride_y = (hits[i1].rect.y - hits[i0].rect.y).abs();
            if stride_y >= MIN_SLOT_PITCH && hits[i0].rect.height > stride_y * STRETCH_RATIO {
                hits[i0].rect.height = stride_y;
            }
            let stride_x = (hits[i1].rect.x - hits[i0].rect.x).abs();
            if stride_x >= MIN_SLOT_PITCH && hits[i0].rect.width > stride_x * STRETCH_RATIO {
                hits[i0].rect.width = stride_x;
            }
        }
        let last = idxs[idxs.len() - 1];
        let prev = idxs[idxs.len() - 2];
        let stride_y = (hits[last].rect.y - hits[prev].rect.y).abs();
        if stride_y >= MIN_SLOT_PITCH && hits[last].rect.height > stride_y * STRETCH_RATIO {
            hits[last].rect.height = stride_y;
        }
        let stride_x = (hits[last].rect.x - hits[prev].rect.x).abs();
        if stride_x >= MIN_SLOT_PITCH && hits[last].rect.width > stride_x * STRETCH_RATIO {
            hits[last].rect.width = stride_x;
        }
    }
}

/// Nested folder: an empty inner list lays out at 0 height (`h=auto`, no
/// children). Hover and collision then skip it (`Rect::is_usable`), so dropping
/// a file onto the Inbox header cannot select `tree-inbox` or open chrome
/// padding. Borrow the smallest enclosing *other* hit (the folder item) as the
/// shell's drop zone until the list has its own box. Kanban empty columns
/// already have `min_h` and stay untouched.
///
/// Do not expand the **origin** list. Vacating the last file collapses that
/// shell to 0; borrowing the folder box would steal hover back from Later and
/// undo the shrink (Gallery: Inbox 留下一块过大空洞).
fn expand_collapsed_empty_list_shells(hits: &mut [DragHit], from_list: Option<&str>) {
    let mut empty_shells: Vec<usize> = Vec::new();
    'shells: for (i, h) in hits.iter().enumerate() {
        if h.index.is_some() {
            continue;
        }
        let Some(list) = h.list_id.as_deref() else {
            continue;
        };
        if from_list == Some(list) {
            continue;
        }
        if list.is_empty() || h.rect.is_usable() {
            continue;
        }
        for other in hits.iter() {
            if other.list_id.as_deref() == Some(list) && other.index.is_some() {
                continue 'shells;
            }
        }
        empty_shells.push(i);
    }
    for si in empty_shells {
        let r = hits[si].rect;
        let seed = [r.x + r.width * 0.5, r.y + r.height * 0.5];
        if !seed[0].is_finite() || !seed[1].is_finite() {
            continue;
        }
        let mut best: Option<(usize, f32)> = None;
        for (i, h) in hits.iter().enumerate() {
            if i == si || !h.rect.is_usable() || !h.rect.contains_point(seed) {
                continue;
            }
            let a = h.rect.area();
            match best {
                Some((_, ba)) if a >= ba - 1e-4 => {}
                _ => best = Some((i, a)),
            }
        }
        if let Some((hi, _)) = best {
            hits[si].rect = hits[hi].rect;
        }
    }
}

fn collect_hits_inner(
    el: &Element,
    scroll_acc: [f32; 2],
    ancestor: Option<RestrictAncestor>,
    overflow_clip: Option<[f32; 4]>,
    out: &mut Vec<DragHit>,
) {
    if el.attrs.contains_key(ATTR_DRAG_OVERLAY) || el.attrs.contains_key(ATTR_DRAG_HOLE_PREVIEW) {
        return;
    }
    // Pointer-space layout slot (scroll applied). Sortable preview translate
    // is ignored so insert-index stays on the original rows.
    let (scroll_acc, overflow_clip) = if el.template_id.as_deref()
        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        ([0.0_f32, 0.0_f32], None)
    } else {
        (scroll_acc, overflow_clip)
    };
    let slot = crate::app::tree_hit::collision_hit_box(el, scroll_acc);
    let item_shell = el.attrs.contains_key(ATTR_DRAG_ITEM_SHELL);
    let child_anc = RestrictAncestor {
        slot,
        item_shell,
        gap: if item_shell {
            ancestor.map(|a| a.gap).unwrap_or_default()
        } else {
            let zoom = if el.style.zoom.is_finite() && el.style.zoom > 0.0 {
                el.style.zoom
            } else {
                1.0
            };
            [el.style.gap.width, el.style.gap.height]
                .map(|gap| crate::layout::lp_length_px(gap).unwrap_or(0.0).max(0.0) * zoom)
        },
        outer: if item_shell {
            author_parent_slot(ancestor)
        } else {
            Some(slot)
        },
    };
    let drag_id = el.attrs.get(ATTR_DRAG_DRAGGABLE).cloned();
    let drop_id = el.attrs.get(ATTR_DRAG_DROPPABLE).cloned();
    let named = el.template_id.as_deref().filter(|s| !s.is_empty());
    let attr_id = el
        .attrs
        .get("id")
        .map(String::as_str)
        .filter(|s| !s.is_empty());
    if let Some(n) = named {
        remember_named_bounds(n, slot);
        remember_ancestor_overflow_clip(n, overflow_clip);
    }
    if let Some(n) = attr_id {
        remember_named_bounds(n, slot);
        remember_ancestor_overflow_clip(n, overflow_clip);
    }
    if drag_id.is_some() || drop_id.is_some() {
        let id = drag_id.clone().or(drop_id.clone()).unwrap_or_default();
        let scope_id = el.attrs.get(ATTR_DRAG_SCOPE).cloned().unwrap_or_default();
        remember_draggable_bounds(&scope_id, &id, slot);
        remember_ancestor_overflow_clip(&id, overflow_clip);
        if el.attrs.contains_key(ATTR_DRAG_INDEX) {
            if let Some(parent) = ancestor {
                ITEM_PACK_GAPS.with(|m| {
                    m.borrow_mut()
                        .insert(item_bounds_key(&scope_id, &id), parent.gap);
                });
            }
        }
        if drag_id.is_some() {
            if let Some(parent) = author_parent_slot(ancestor) {
                remember_parent_bounds(&scope_id, &id, parent);
                remember_ancestor_overflow_clip(&item_bounds_key(&scope_id, &id), overflow_clip);
            }
        }
        let disabled = el
            .attrs
            .get(ATTR_DRAG_DISABLED)
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let skip_shift = el.attrs.contains_key(ATTR_DRAG_SKIP_SHIFT);
        let skip_follow = el.attrs.contains_key(ATTR_DRAG_SKIP_FOLLOW);
        let pull_clone = el.attrs.contains_key(ATTR_DRAG_PULL_CLONE);
        out.push(DragHit {
            id,
            scope_id,
            is_draggable: drag_id.is_some(),
            is_droppable: drop_id.is_some(),
            rect: Rect::from_bounds(slot),
            item_type: el.attrs.get(ATTR_DRAG_TYPE).cloned().unwrap_or_default(),
            accept: el.attrs.get(ATTR_DRAG_ACCEPT).cloned().unwrap_or_default(),
            list_id: el.attrs.get(ATTR_DRAG_LIST).cloned(),
            index: el.attrs.get(ATTR_DRAG_INDEX).and_then(|s| s.parse().ok()),
            group: el.attrs.get(ATTR_DRAG_GROUP).cloned().unwrap_or_default(),
            disabled,
            list_axis: el.attrs.get(ATTR_DRAG_AXIS).map(|s| Axis::parse(s)),
            skip_shift,
            skip_follow,
            pull_clone,
        });
    }
    let mut next_acc = crate::app::tree_hit::next_hit_scroll_acc(el, scroll_acc);
    // Child layout_bounds include ancestor translates. The kit shell's layout
    // compensation belongs to paint, not the sortable collision slots.
    if el.attrs.contains_key(super::ATTR_DRAG_ITEM_SHELL) {
        next_acc[0] += el.style.translate[0];
        next_acc[1] += el.style.translate[1];
    }
    let next_clip = child_overflow_clip(el, slot, overflow_clip);
    for c in &el.children {
        collect_hits_inner(c, next_acc, Some(child_anc), next_clip, out);
    }
}

#[derive(Clone, Copy)]
struct RestrictAncestor {
    slot: [f32; 4],
    gap: [f32; 2],
    /// Kit [`ATTR_DRAG_ITEM_SHELL`] — skip so `restrict=parent` is the author box.
    item_shell: bool,
    outer: Option<[f32; 4]>,
}

fn author_parent_slot(anc: Option<RestrictAncestor>) -> Option<[f32; 4]> {
    let a = anc?;
    if a.item_shell {
        a.outer
    } else {
        Some(a.slot)
    }
}

/// Sortable-style insert index: dropping on a container (no index) appends.
pub fn resolve_insert_index(hit: Option<&DragHit>, fallback_len: usize) -> Option<usize> {
    hit.map(|h| h.index.unwrap_or(fallback_len))
}

/// Origin deltas below this are layout jitter, not a sortable row pitch.
const MIN_SLOT_PITCH: f32 = 8.0;

/// List-level `rui_drag_axis` overrides the provider. Missing → `fallback`
/// (`Xy` sorts as `Y`).
pub(super) fn sort_axis_for_list(hits: &[DragHit], list_id: &str, fallback: Axis) -> Axis {
    hits.iter()
        .filter(|h| h.list_id.as_deref() == Some(list_id))
        .find_map(|h| h.list_axis)
        .unwrap_or(fallback)
        .sort_axis()
}

pub(super) fn peek_sort_axis(list: Option<&str>, fallback: Axis) -> Axis {
    let Some(id) = list else {
        return fallback.sort_axis();
    };
    LAST_HITS.with(|h| sort_axis_for_list(&h.borrow(), id, fallback))
}

/// Sortable insert index along `axis` (SortableJS midpoint semantics).
///
/// Upper half of row `i` → insert before `i`; lower half → insert after `i`.
/// Each row occupies `[origin(i), origin(i+1))`; the tail row ends at
/// `origin(last) + pitch`.
pub fn insert_index_along_axis(
    hits: &[DragHit],
    list_id: &str,
    pointer: [f32; 2],
    axis: Axis,
) -> Option<usize> {
    let mut items: Vec<&DragHit> = hits
        .iter()
        .filter(|h| {
            h.list_id.as_deref() == Some(list_id) && h.index.is_some() && h.rect.is_usable()
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    items.sort_by_key(|h| h.index.unwrap_or(0));
    let p = match axis {
        Axis::X => pointer[0],
        Axis::Y | Axis::Xy | Axis::Start => pointer[1],
    };
    let fallback = match axis {
        Axis::X => items[0].rect.width,
        Axis::Y | Axis::Xy | Axis::Start => items[0].rect.height,
    }
    .max(1.0);
    let last_idx = items[items.len() - 1].index;
    let tail_pitch = slot_extent_along_axis(hits, list_id, last_idx, axis, fallback);
    let first = axis_origin(items[0].rect, axis);
    let last = axis_origin(items[items.len() - 1].rect, axis);
    if items.len() > 1 {
        let avg = (last - first).abs() / (items.len() - 1) as f32;
        if avg < MIN_SLOT_PITCH {
            return None;
        }
    }
    if p < first {
        return items[0].index;
    }
    for (i, h) in items.iter().enumerate() {
        let start = axis_origin(h.rect, axis);
        let end = if i + 1 < items.len() {
            axis_origin(items[i + 1].rect, axis)
        } else {
            start + tail_pitch
        };
        if p < start {
            continue;
        }
        let mid = (start + end) * 0.5;
        if p <= mid {
            return h.index;
        }
        if p < end {
            return h.index.map(|idx| idx + 1);
        }
    }
    // Past every [start, end). Append only when the probe is still adjacent to
    // the column. Clustered / jittered slots used to fall through to max+1.
    if p < last + tail_pitch * 2.0 {
        return items.iter().filter_map(|h| h.index).max().map(|i| i + 1);
    }
    None
}

fn axis_origin(rect: Rect, axis: Axis) -> f32 {
    match axis {
        Axis::X => rect.x,
        Axis::Y | Axis::Xy | Axis::Start => rect.y,
    }
}

/// Pitch between consecutive layout slots in `list_id` (item size **plus**
/// author gap). Preview shift must use this, not the dragged box alone —
/// otherwise siblings overlap by the gap and the stack visually shortens.
pub(crate) fn slot_stride_along_axis(
    hits: &[DragHit],
    list_id: &str,
    axis: Axis,
    fallback: f32,
) -> f32 {
    let mut items: Vec<&DragHit> = hits
        .iter()
        .filter(|h| h.list_id.as_deref() == Some(list_id) && h.index.is_some())
        .collect();
    if items.len() < 2 {
        return fallback;
    }
    items.sort_by_key(|h| h.index.unwrap_or(0));
    for w in items.windows(2) {
        let a = w[0].index.unwrap_or(0);
        let b = w[1].index.unwrap_or(0);
        if b != a + 1 {
            continue;
        }
        let stride = match axis {
            Axis::X => (w[1].rect.x - w[0].rect.x).abs(),
            Axis::Y | Axis::Xy | Axis::Start => (w[1].rect.y - w[0].rect.y).abs(),
        };
        // 2px layout jitter is not a row pitch: walking it always falls off
        // the last origin and appends (Gallery dimmed Sortable vs opacity 0).
        if stride >= MIN_SLOT_PITCH {
            return stride;
        }
    }
    fallback
}

/// Paint box vs packed neighbour distance. Above this, the island is stretched
/// (`also_droppable` / column height), not a naturally taller row.
const STRETCH_RATIO: f32 = 1.5;

fn indexed_list_items<'a>(hits: &'a [DragHit], list_id: &str) -> Vec<&'a DragHit> {
    let mut items: Vec<&DragHit> = hits
        .iter()
        .filter(|h| h.list_id.as_deref() == Some(list_id) && h.index.is_some())
        .collect();
    items.sort_by_key(|h| h.index.unwrap_or(0));
    items
}

fn axis_size(rect: Rect, axis: Axis) -> f32 {
    match axis {
        Axis::X => rect.width,
        Axis::Y | Axis::Xy | Axis::Start => rect.height,
    }
}

fn inferred_list_gap(items: &[&DragHit], axis: Axis) -> f32 {
    for w in items.windows(2) {
        if w[1].index.unwrap_or(0) != w[0].index.unwrap_or(0) + 1 {
            continue;
        }
        let stride = (axis_origin(w[1].rect, axis) - axis_origin(w[0].rect, axis)).abs();
        if stride < MIN_SLOT_PITCH {
            continue;
        }
        let size = axis_size(w[0].rect, axis);
        if size > stride * STRETCH_RATIO {
            continue;
        }
        return (stride - size).max(0.0);
    }
    0.0
}

/// Advance along `axis` when packing `items[orig_pos]` into a visual order.
/// Followed rows keep the original slot (size + gap); the visual last row is
/// paint size only. Stretched islands use the packed neighbour stride.
fn pack_advance(
    items: &[&DragHit],
    orig_pos: usize,
    visual_last: bool,
    axis: Axis,
    gap: f32,
) -> f32 {
    let size = axis_size(items[orig_pos].rect, axis).max(1.0);
    let next_stride = items.get(orig_pos + 1).and_then(|n| {
        let s = (axis_origin(n.rect, axis) - axis_origin(items[orig_pos].rect, axis)).abs();
        (s >= MIN_SLOT_PITCH).then_some(s)
    });
    let prev_stride = orig_pos.checked_sub(1).and_then(|p| {
        let s = (axis_origin(items[orig_pos].rect, axis) - axis_origin(items[p].rect, axis)).abs();
        (s >= MIN_SLOT_PITCH).then_some(s)
    });
    let stretched = next_stride.is_some_and(|s| size > s * STRETCH_RATIO)
        || prev_stride.is_some_and(|s| size > s * STRETCH_RATIO);
    let paint = if stretched {
        next_stride.or(prev_stride).unwrap_or(size)
    } else {
        size
    };
    if visual_last {
        paint
    } else {
        next_stride.unwrap_or(paint + gap)
    }
}

/// Slot of `index` in `list_id`: packed neighbour distance, or paint size for
/// the last (unstretched) row. Stretched last rows copy the previous stride.
pub(crate) fn slot_extent_along_axis(
    hits: &[DragHit],
    list_id: &str,
    index: Option<usize>,
    axis: Axis,
    fallback: f32,
) -> f32 {
    let items = indexed_list_items(hits, list_id);
    let Some(index) = index else {
        return fallback.max(1.0);
    };
    let Some(pos) = items.iter().position(|h| h.index == Some(index)) else {
        return fallback.max(1.0);
    };
    pack_advance(&items, pos, false, axis, inferred_list_gap(&items, axis))
}

fn visual_index_order(items: &[&DragHit], from: usize, hover: usize) -> Vec<usize> {
    let idxs: Vec<usize> = items.iter().filter_map(|h| h.index).collect();
    if from == hover || idxs.is_empty() {
        return idxs;
    }
    let remaining: Vec<usize> = idxs.iter().copied().filter(|&i| i != from).collect();
    let insert_at = remaining
        .iter()
        .filter(|&&i| if from < hover { i <= hover } else { i < hover })
        .count()
        .min(remaining.len());
    let mut order = remaining;
    order.insert(insert_at, from);
    order
}

/// Source collapse is a visual preview, not a change to its logical sort slots.
/// Rebase activation-time slots on the live list shell so scrolling and moving
/// ancestor containers still move both hover targets and packed origins.
fn source_list_layout(hits: &[DragHit], list_id: &str) -> Option<Vec<DragHit>> {
    SOURCE_LIST_LAYOUT.with(|layout| {
        let layout = layout.borrow();
        let shell = layout
            .iter()
            .find(|h| h.index.is_none() && h.list_id.as_deref() == Some(list_id))?;
        let live_shell = hits
            .iter()
            .find(|h| h.id == shell.id && h.scope_id == shell.scope_id && h.index.is_none())?;
        let delta = [
            live_shell.rect.x - shell.rect.x,
            live_shell.rect.y - shell.rect.y,
        ];
        Some(
            layout
                .iter()
                .map(|slot| {
                    let mut hit = slot.clone();
                    if hit.index.is_some() {
                        hit.rect.x += delta[0];
                        hit.rect.y += delta[1];
                    } else {
                        hit.rect = live_shell.rect;
                    }
                    hit
                })
                .collect(),
        )
    })
}

/// Packed origins after lifting `from` and inserting at `hover` (same list).
/// Total span equals the layout span (reorder does not grow the list).
fn pack_same_list_origins(
    hits: &[DragHit],
    list_id: &str,
    from: usize,
    hover: usize,
    axis: Axis,
) -> HashMap<usize, [f32; 2]> {
    let restored = source_list_layout(hits, list_id);
    let hits = restored.as_deref().unwrap_or(hits);
    let items = indexed_list_items(hits, list_id);
    let mut map = HashMap::new();
    if items.is_empty() {
        return map;
    }
    let start = [items[0].rect.x, items[0].rect.y];
    let gap = inferred_list_gap(&items, axis);
    let group = SESSION
        .with(|s| {
            s.borrow().as_ref().filter(|s| s.items.len() > 1).map(|s| {
                s.items
                    .iter()
                    .filter_map(|i| i.from_index)
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();
    let order = if group.is_empty() {
        visual_index_order(&items, from, hover)
    } else {
        let mut order: Vec<_> = items
            .iter()
            .filter_map(|h| h.index)
            .filter(|i| !group.contains(i))
            .collect();
        let insert = order.iter().filter(|i| **i < hover).count();
        order.splice(insert..insert, group);
        order
    };
    let index_to_pos: HashMap<usize, usize> = items
        .iter()
        .enumerate()
        .filter_map(|(p, h)| h.index.map(|i| (i, p)))
        .collect();
    let mut cursor = match axis {
        Axis::X => start[0],
        Axis::Y | Axis::Xy | Axis::Start => start[1],
    };
    for (vi, idx) in order.iter().enumerate() {
        let origin = match axis {
            Axis::X => [cursor, start[1]],
            Axis::Y | Axis::Xy | Axis::Start => [start[0], cursor],
        };
        map.insert(*idx, origin);
        let Some(&orig_pos) = index_to_pos.get(idx) else {
            continue;
        };
        cursor += pack_advance(&items, orig_pos, vi + 1 == order.len(), axis, gap);
    }
    map
}

fn pack_item_translate(
    hits: &[DragHit],
    my_list: Option<&str>,
    my_index: Option<usize>,
    from_list: Option<&str>,
    from_index: Option<usize>,
    hover_list: Option<&str>,
    hover_index: Option<usize>,
    axis: Axis,
) -> [f32; 2] {
    if (active_pull_clone() || active_sort_mode() == SortMode::Swap) && !active_reorder_preview() {
        return [0.0, 0.0];
    }
    let (Some(my_list), Some(my_i), Some(hover_list)) = (my_list, my_index, hover_list) else {
        return [0.0, 0.0];
    };
    if active_sort_mode() == SortMode::Swap {
        let source = hits.iter().find(|h| {
            h.list_id.as_deref() == from_list && h.index == from_index && h.index.is_some()
        });
        let target = hits.iter().find(|h| {
            h.list_id.as_deref() == Some(hover_list) && h.index == hover_index && h.index.is_some()
        });
        if let (Some(source), Some(target)) = (source, target) {
            let delta = [target.rect.x - source.rect.x, target.rect.y - source.rect.y];
            if my_list == from_list.unwrap_or("") && Some(my_i) == from_index {
                return delta;
            }
            if my_list == hover_list && Some(my_i) == hover_index {
                return [-delta[0], -delta[1]];
            }
        }
        return [0.0, 0.0];
    }
    if active_pull_clone() {
        if my_list == hover_list && hover_index.is_some_and(|i| my_i >= i) {
            let extent = packed_row_extent(hits, Some(my_list), axis, 1.0);
            return if axis == Axis::X {
                [extent, 0.0]
            } else {
                [0.0, extent]
            };
        }
        return [0.0, 0.0];
    }
    if let Some(hit) = hits
        .iter()
        .find(|h| h.list_id.as_deref() == Some(my_list) && h.index == Some(my_i))
    {
        if hit.skip_shift {
            return [0.0, 0.0];
        }
    }
    if from_list == Some(hover_list) {
        let (Some(from), Some(hover), true) = (from_index, hover_index, from_list == Some(my_list))
        else {
            return [0.0, 0.0];
        };
        // Returning from another list restores the source's in-flow slot.
        // Its cached hits can still describe the vacated pack for this frame.
        if from == hover && active_drag_ids().len() <= 1 {
            return [0.0, 0.0];
        }
        let packed = pack_same_list_origins(hits, my_list, from, hover, axis);
        let Some(new) = packed.get(&my_i) else {
            return [0.0, 0.0];
        };
        let restored = source_list_layout(hits, my_list);
        let Some(layout) = indexed_list_items(restored.as_deref().unwrap_or(hits), my_list)
            .into_iter()
            .find(|h| h.index == Some(my_i))
        else {
            return [0.0, 0.0];
        };
        return [new[0] - layout.rect.x, new[1] - layout.rect.y];
    }
    if from_list == Some(my_list) {
        // The source shell's size animation closes the origin hole in layout.
        // A second -extent translate would move siblings twice, into the header.
        return [0.0, 0.0];
    }
    if hover_list != my_list {
        return [0.0, 0.0];
    }
    let Some(hover) = hover_index else {
        return [0.0, 0.0];
    };
    if my_i < hover {
        return [0.0, 0.0];
    }
    let fallback = indexed_list_items(hits, my_list)
        .into_iter()
        .find(|h| h.index == Some(my_i))
        .map(|h| axis_size(h.rect, axis))
        .unwrap_or(1.0);
    // Occupied target: one packed row. Prefer the target's own neighbour
    // stride (Inbox Alpha→Beta includes the author gap). A singleton source
    // snapshots chip-only, and shifting Inbox by that pitch sits Gamma flush
    // on Alpha/Beta (Gallery: Later→Inbox 插到任意项之上).
    let chip = fallback.max(1.0);
    let extent = group_insert_extent(hits, my_list, axis, chip);
    match axis {
        Axis::X => [extent, 0.0],
        Axis::Y | Axis::Xy | Axis::Start => [0.0, extent],
    }
}

pub(crate) fn live_pack_item_translate(
    my_list: Option<&str>,
    my_index: Option<usize>,
    from_list: Option<&str>,
    from_index: Option<usize>,
    hover_list: Option<&str>,
    hover_index: Option<usize>,
    axis: Axis,
) -> [f32; 2] {
    LAST_HITS.with(|h| {
        pack_item_translate(
            &h.borrow(),
            my_list,
            my_index,
            from_list,
            from_index,
            hover_list,
            hover_index,
            axis,
        )
    })
}

#[cfg(test)]
pub(crate) fn list_slot_stride(list_id: Option<&str>, axis: Axis, fallback: f32) -> f32 {
    let Some(list_id) = list_id else {
        return fallback;
    };
    LAST_HITS.with(|h| slot_stride_along_axis(&h.borrow(), list_id, axis, fallback))
}

/// One packed row for chrome padding / source vacate. Prefer neighbour stride
/// (item + gap). A stretched `also_droppable` island can be taller than the
/// row; using `.max(chip)` then pulls the origin folder by the whole card
/// (Gallery: 移出 Beta 后 Inbox 收缩过大).
#[cfg(test)]
pub(crate) fn list_row_extent(list_id: Option<&str>, axis: Axis, chip: f32) -> f32 {
    let chip = chip.max(0.0);
    let stride = list_slot_stride(list_id, axis, chip);
    if stride >= MIN_SLOT_PITCH && (chip < 1.0 || stride <= chip * STRETCH_RATIO) {
        stride
    } else {
        chip
    }
}

fn row_pitch_along(hits: &[DragHit], list_id: Option<&str>, axis: Axis, chip: f32) -> f32 {
    let chip = chip.max(0.0);
    let Some(list_id) = list_id else {
        return chip;
    };
    let items = indexed_list_items(hits, list_id);
    if let [item] = items.as_slice() {
        let gap = ITEM_PACK_GAPS
            .with(|m| {
                m.borrow()
                    .get(&item_bounds_key(&item.scope_id, &item.id))
                    .copied()
            })
            .unwrap_or_default();
        return chip + gap[if axis == Axis::X { 0 } else { 1 }];
    }
    let stride = slot_stride_along_axis(hits, list_id, axis, chip);
    if stride >= MIN_SLOT_PITCH && (chip < 1.0 || stride <= chip * STRETCH_RATIO) {
        stride
    } else {
        chip
    }
}

/// One packed row for a cross-list hole: the larger of the target neighbour
/// stride and the snapshotted source pitch. Target stride keeps Inbox's
/// author gap when the source was a singleton (Later/Gamma). Source pitch
/// keeps item+gap when the target itself is a singleton (Later).
fn packed_row_extent(hits: &[DragHit], target_list: Option<&str>, axis: Axis, chip: f32) -> f32 {
    let chip = chip.max(0.0);
    let target = row_pitch_along(hits, target_list, axis, chip);
    let source = DRAG_PACK_PITCH.get();
    if source >= MIN_SLOT_PITCH {
        source.max(target)
    } else {
        target.max(chip)
    }
}

/// Cross-list insert hole on the **target**. Empty lists open `chip` (no gap
/// until a second row exists). Occupied lists open one packed row.
pub(crate) fn pack_insert_extent(target_list: Option<&str>, axis: Axis, chip: f32) -> f32 {
    let chip = chip.max(0.0);
    let Some(list) = target_list else {
        return chip;
    };
    LAST_HITS.with(|h| {
        let hits = h.borrow();
        let occupied = hits
            .iter()
            .any(|hit| hit.list_id.as_deref() == Some(list) && hit.index.is_some());
        if !occupied && active_drag_ids().len() <= 1 {
            return chip;
        }
        group_insert_extent(&hits, list, axis, chip)
    })
}

fn group_insert_extent(hits: &[DragHit], list: &str, axis: Axis, chip: f32) -> f32 {
    let ids = active_drag_ids();
    if ids.len() <= 1 {
        return packed_row_extent(hits, Some(list), axis, chip);
    }
    let source = SOURCE_LIST_LAYOUT.with(|layout| layout.borrow().clone());
    let rows = indexed_list_items(hits, list);
    let gap = inferred_list_gap(&rows, axis)
        .max(DRAG_PACK_GAP.get())
        .max(0.0);
    let size: f32 = ids
        .iter()
        .map(|id| {
            source
                .iter()
                .find(|h| &h.id == id)
                .map(|h| axis_size(h.rect, axis))
                .unwrap_or(chip)
        })
        .sum();
    size + gap * (ids.len() - usize::from(rows.is_empty())) as f32
}

/// True when the hover is past the last occupied slot (append), or the list
/// is empty. Mid-list insert opens the hole by sibling FLIP — trailing pad
/// would stack a second hole under the last row.
#[cfg(test)]
pub(crate) fn list_is_append_hover(list: &str, hover_index: Option<usize>) -> bool {
    match (list_max_index(Some(list)), hover_index) {
        (None, _) => true,
        (Some(last), Some(hover)) => hover > last,
        (Some(_), None) => true,
    }
}

/// Author gap inside the snapshotted packed row. Used to cancel the flex
/// `gap` a 0-height source shell still occupies. Snapshotted at activate
/// from neighbour stride − item size — not `pitch - live drag_extent`,
/// which is 0 when those two happen to match.
pub(crate) fn pack_gap(_chip: f32) -> f32 {
    DRAG_PACK_GAP.get().max(0.0)
}

#[cfg(test)]
pub(crate) fn list_max_index(list_id: Option<&str>) -> Option<usize> {
    let list_id = list_id?;
    LAST_HITS.with(|h| {
        h.borrow()
            .iter()
            .filter(|hit| hit.list_id.as_deref() == Some(list_id))
            .filter_map(|hit| hit.index)
            .max()
    })
}

/// Layout origin of the opened insert hole in `list_id` (pre-translate slots).
/// Append (`hover` past last index) uses the snapshotted packed row so a
/// one-item target still opens item+gap — live `slot_extent` of a singleton
/// is chip-only (Gallery: Beta 移到 Later/Gamma 之下时贴死).
pub(crate) fn insert_hole_origin(
    hits: &[DragHit],
    list_id: &str,
    hover_index: Option<usize>,
    axis: Axis,
    fallback_extent: f32,
) -> Option<[f32; 2]> {
    let mut items: Vec<&DragHit> = hits
        .iter()
        .filter(|h| h.list_id.as_deref() == Some(list_id) && h.index.is_some())
        .collect();
    items.sort_by_key(|h| h.index.unwrap_or(0));
    let stride = packed_row_extent(hits, Some(list_id), axis, fallback_extent.max(1.0));
    if let Some(hover) = hover_index {
        if let Some(h) = items.iter().find(|h| h.index == Some(hover)) {
            return Some([h.rect.x, h.rect.y]);
        }
        if let Some(last) = items.last() {
            let (dx, dy) = match axis {
                Axis::X => (stride, 0.0),
                Axis::Y | Axis::Xy | Axis::Start => (0.0, stride),
            };
            return Some([last.rect.x + dx, last.rect.y + dy]);
        }
    }
    hits.iter()
        .find(|h| h.list_id.as_deref() == Some(list_id) && h.index.is_none())
        .map(|h| [h.rect.x, h.rect.y])
}

/// Translate that places the origin-list source onto the target insert hole.
#[cfg(test)]
pub(crate) fn source_translate_to_insert_hole(
    hits: &[DragHit],
    from_list: &str,
    from_index: usize,
    hover_list: &str,
    hover_index: Option<usize>,
    axis: Axis,
) -> [f32; 2] {
    source_translate_to_insert_hole_at(
        hits,
        from_list,
        from_index,
        hover_list,
        hover_index,
        axis,
        None,
    )
    .unwrap_or([0.0, 0.0])
}

#[cfg(test)]
fn source_translate_to_insert_hole_at(
    hits: &[DragHit],
    from_list: &str,
    from_index: usize,
    hover_list: &str,
    hover_index: Option<usize>,
    axis: Axis,
    origin: Option<[f32; 2]>,
) -> Option<[f32; 2]> {
    let src = hits
        .iter()
        .find(|h| h.list_id.as_deref() == Some(from_list) && h.index == Some(from_index));
    let fallback = src
        .map(|s| match axis {
            Axis::X => s.rect.width,
            Axis::Y | Axis::Xy | Axis::Start => s.rect.height,
        })
        .unwrap_or(0.0)
        .max(1.0);
    let hole = insert_hole_origin(hits, hover_list, hover_index, axis, fallback)?;
    let origin = origin.or_else(|| src.map(|s| [s.rect.x, s.rect.y]))?;
    Some([hole[0] - origin[0], hole[1] - origin[1]])
}

/// Same-list dimmed source: layout-measured hole, not `(hover-from)*pitch`.
pub(crate) fn live_same_list_source_translate(
    from_list: &str,
    from_index: usize,
    hover_list: &str,
    hover_index: usize,
    axis: Axis,
) -> [f32; 2] {
    LAST_HITS.with(|h| {
        pack_item_translate(
            &h.borrow(),
            Some(from_list),
            Some(from_index),
            Some(from_list),
            Some(from_index),
            Some(hover_list),
            Some(hover_index),
            axis,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;
    use crate::core::tree::Element;
    use std::rc::Rc;

    fn hit(id: &str, bounds: [f32; 4], drop: bool) -> DragHit {
        DragHit {
            id: id.into(),
            scope_id: "s".into(),
            is_draggable: !drop,
            is_droppable: drop,
            rect: Rect::from_bounds(bounds),
            item_type: "card".into(),
            accept: String::new(),
            list_id: Some("list".into()),
            index: None,
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        }
    }

    #[test]
    fn restricted_source_and_drop_follow_parent_scroll() {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::{
            scroll_offset_x_signal_key, scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
        };

        let (_owner, ()) = create_root(|| {
            let committed = Signal::new([0.0, 0.0]);
            let _scope = super::super::drag_scope("scroll-drop")
                .threshold(0.0)
                .restrict(Restrict::Parent)
                .on_drag_end(move |ev| committed.set(ev.delta))
                .child(Element::new);
            let sx = use_signal(&scroll_offset_x_signal_key("scroll-drop-page"), || 0.0_f32);
            let sy = use_signal(&scroll_offset_y_signal_key("scroll-drop-page"), || 0.0_f32);
            let mut card = Element::new();
            card.layout_bounds = [80.0, 240.0, 40.0, 24.0];
            card.set_attr(ATTR_DRAG_DRAGGABLE, "card");
            card.set_attr(ATTR_DRAG_SCOPE, "scroll-drop");
            let mut arena = Element::new();
            arena.layout_bounds = [10.0, 200.0, 200.0, 120.0];
            arena.set_attr("id", "scroll-drop-arena");
            arena = arena.child(card);
            let mut page = Element::new();
            page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}scroll-drop-page"));
            page.layout_bounds = [0.0, 0.0, 800.0, 600.0];
            page = page.child(arena);

            for axis in [Axis::Xy, Axis::X, Axis::Y] {
                for scroll in [[160.0, 180.0], [40.0, 20.0]] {
                    for pointer in [[-500.0, -500.0], [900.0, 900.0]] {
                        sx.set_silent(100.0);
                        sy.set_silent(100.0);
                        SCOPES.with(|scopes| {
                            scopes.borrow_mut().get_mut("scroll-drop").unwrap().axis = axis;
                        });
                        let hits = collect_hits(&page);
                        let start = hits.iter().find(|h| h.id == "card").unwrap().rect;
                        begin(BeginDrag {
                            scope_id: "scroll-drop".into(),
                            draggable_id: "card".into(),
                            pointer: [start.x + 20.0, start.y + 12.0],
                            grab_offset: [20.0, 12.0],
                            start_bounds: start.to_bounds(),
                            from_list: None,
                            from_index: None,
                            item_type: "chip".into(),
                            group: String::new(),
                            capture_id: "cap".into(),
                        });
                        assert!(tick(pointer[0], pointer[1], &hits, [800.0, 600.0]));
                        sx.set_silent(scroll[0]);
                        sy.set_silent(scroll[1]);
                        // The page scrolls while the pointer stays at the edge.
                        resync_overlay_follow_after_layout(&mut page);
                        let ghost = peek_overlay_pos("scroll-drop").unwrap();
                        let hits = collect_hits(&page);
                        let slot = hits.iter().find(|h| h.id == "card").unwrap().rect;
                        let translate = active_source_translate().unwrap();
                        assert_eq!(
                            [slot.x + translate[0], slot.y + translate[1]],
                            ghost,
                            "scrolled source must paint at the restricted origin: axis={axis:?}, scroll={scroll:?}"
                        );
                        let painted = crate::core::tree::find_in_tree(&page, |el| {
                            el.attrs.get(ATTR_DRAG_DRAGGABLE).map(String::as_str) == Some("card")
                        })
                        .unwrap()
                        .style
                        .translate;
                        assert_eq!(
                            [slot.x + painted[0], slot.y + painted[1]],
                            ghost,
                            "post-layout sync must update the source before paint"
                        );
                        let parent = last_parent_bounds("scroll-drop", "card").unwrap();
                        assert!(ghost[0] >= parent[0] && ghost[1] >= parent[1]);
                        assert!(ghost[0] + slot.width <= parent[0] + parent[2]);
                        assert!(ghost[1] + slot.height <= parent[1] + parent[3]);
                        assert!(tick(pointer[0], pointer[1], &hits, [800.0, 600.0]));
                        let ghost = peek_overlay_pos("scroll-drop").unwrap();
                        assert!(end(&hits));
                        let delta = committed.peek();
                        assert_eq!(
                            [slot.x + delta[0], slot.y + delta[1]],
                            ghost,
                            "persisted position must match preview after scroll: axis={axis:?}, scroll={scroll:?}"
                        );
                    }
                }
            }
        });
    }

    #[test]
    fn threshold_then_activate_and_end() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 8.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(!tick(12.0, 10.0, &drops, [400.0, 300.0]));
            assert!(!is_active());
            assert!(tick(30.0, 10.0, &drops, [400.0, 300.0]));
            assert!(is_active());
            assert_eq!(active_draggable.peek().as_deref(), Some("a"));
            assert_eq!(overlay_pos.peek(), [28.0, 8.0]);
            assert_eq!(last_hits().len(), 1);
            assert_eq!(last_hits_in_scope("s")[0].id, "zone");
            assert!(last_hits_in_scope("other").is_empty());
            let _ = end(&drops);
            assert!(!is_dragging());
            assert!(active_draggable.peek().is_none());
        });
    }

    #[test]
    fn threshold_zero_stays_pending_on_press_activates_on_first_move() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [20.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [10.0, 8.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(!is_active(), "begin stays pending until tick");
            assert!(!overlay_open.peek());
            assert!(
                !tick(20.0, 16.0, &[], [400.0, 300.0]),
                "press must not activate: source_opacity/stack chrome would fire with no ghost"
            );
            assert!(!is_active());
            assert!(
                !overlay_open.peek(),
                "overlay stays closed on press so a 0-opacity source is not erased"
            );
            assert!(
                tick(21.0, 16.0, &[], [400.0, 300.0]),
                "first movement is the drag attempt — overlay opens immediately"
            );
            assert!(is_active());
            assert!(overlay_open.peek());
            let _ = end(&[]);
        });
    }

    fn seed_velocity(vx: f32, vy: f32) {
        VELOCITY.with(|v| {
            let mut t = v.borrow_mut();
            t.reset();
            let t0 = Instant::now();
            t.sample([0.0, 0.0], t0);
            t.sample(
                [vx * 0.02, vy * 0.02],
                t0 + std::time::Duration::from_millis(20),
            );
        });
    }

    #[test]
    fn free_move_inertia_coasts_then_commits() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            set_scope_inertia(
                "s",
                crate::utils::pointer_action::InertiaOpts {
                    enabled: true,
                    min_speed: 50.0,
                    ..Default::default()
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [10.0, 10.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops: Vec<DragHit> = vec![];
            assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
            seed_velocity(400.0, 0.0);
            assert!(end(&drops));
            assert!(is_coasting());
            assert!(is_dragging());
            let dummy = Element::new();
            let mut frames = 0;
            while is_coasting() && frames < 200 {
                let _ = pump_inertia(&dummy, [400.0, 300.0]);
                frames += 1;
            }
            assert!(!is_coasting(), "frames={frames}");
            assert!(!is_dragging());
        });
    }

    #[test]
    fn sortable_does_not_coast() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            set_scope_inertia(
                "s",
                crate::utils::pointer_action::InertiaOpts {
                    enabled: true,
                    min_speed: 1.0,
                    ..Default::default()
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [10.0, 10.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            let _ = tick(40.0, 10.0, &drops, [400.0, 300.0]);
            seed_velocity(400.0, 0.0);
            assert!(end(&drops));
            assert!(!is_coasting());
            assert!(!is_dragging());
        });
    }

    #[test]
    fn lock_axis_start_pins_secondary_after_first_move() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s-start".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Start,
                    start_axis: Axis::Xy,
                    threshold: 4.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s-start".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(tick(30.0, 18.0, &drops, [400.0, 300.0]));
            let pos = overlay_pos.peek();
            assert!(
                (pos[0] - 28.0).abs() < 1e-3 && (pos[1] - 8.0).abs() < 1e-3,
                "first move dx>dy must lock X (Y stays at grab origin); got {pos:?}"
            );
            let _ = end(&drops);
        });
    }

    #[test]
    fn start_axis_x_stays_pending_on_vertical_then_activates_on_horizontal() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s-sa".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::X,
                    threshold: 4.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s-sa".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [10.0, 10.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(!tick(10.0, 30.0, &drops, [400.0, 300.0]));
            assert!(
                !is_active(),
                "vertical first move must not activate start_axis=x"
            );
            assert!(tick(30.0, 12.0, &drops, [400.0, 300.0]));
            assert!(is_active());
            let _ = end(&drops);
        });
    }

    #[test]
    fn snap_end_only_applies_on_end_not_tick() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s-snap".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: Some(Snap::grid(SnapGrid::new(10.0, 10.0).end_only(true))),
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s-snap".into(),
                draggable_id: "a".into(),
                pointer: [0.0, 0.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 0.0, 20.0, 20.0],
                from_list: None,
                from_index: None,
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [0.0, 0.0, 400.0, 300.0], true)];
            assert!(tick(14.0, 0.0, &drops, [400.0, 300.0]));
            let mid = overlay_pos.peek();
            assert!(
                (mid[0] - 14.0).abs() < 1e-3,
                "end_only must not snap during tick; got {mid:?}"
            );
            let _ = end(&drops);
            let done = overlay_pos.peek();
            assert!(
                (done[0] - 10.0).abs() < 1e-3,
                "end_only must snap on end; got {done:?}"
            );
        });
    }

    #[test]
    fn axis_y_overlay_x_follows_scrolled_layout_slot() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [110.0, 50.0],
                grab_offset: [10.0, 10.0],
                start_bounds: [100.0, 40.0, 80.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut src = hit("a", [40.0, 40.0, 80.0, 24.0], false);
            src.index = Some(0);
            let hits = vec![src];
            assert!(tick(200.0, 90.0, &hits, [400.0, 300.0]));
            let pos = overlay_pos.peek();
            assert!(
                (pos[0] - 40.0).abs() < 0.5,
                "axis=y must pin overlay X to the scrolled layout slot, not press-time start_bounds; got {:?}",
                pos
            );
            assert!(
                (pos[1] - 80.0).abs() < 0.5,
                "axis=y free axis still follows the pointer (50+40-10); got {:?}",
                pos
            );
            let _ = end(&hits);
        });
    }

    #[test]
    fn axis_xy_overlay_does_not_pin_to_scrolled_slot() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [110.0, 50.0],
                grab_offset: [10.0, 10.0],
                start_bounds: [100.0, 40.0, 80.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits = vec![hit("a", [40.0, 40.0, 80.0, 24.0], false)];
            assert!(tick(200.0, 90.0, &hits, [400.0, 300.0]));
            assert_eq!(
                overlay_pos.peek(),
                [190.0, 80.0],
                "unlocked axis still follows pointer − grab, not the scrolled slot"
            );
            let _ = end(&hits);
        });
    }

    #[test]
    fn pump_autoscroll_resyncs_axis_y_after_parent_scroll() {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::{
            scroll_offset_x_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
        };

        let (_o, _) = create_root(|| {
            use_signal(&scroll_offset_x_signal_key("drag-axis-scroll"), || 0.0_f32).set_silent(0.0);
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            let mut chip = Element::new();
            chip.layout_bounds = [100.0, 40.0, 80.0, 24.0];
            chip.set_attr(ATTR_DRAG_DRAGGABLE, "a");
            chip.set_attr(ATTR_DRAG_SCOPE, "s");
            let mut page = Element::new();
            page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}drag-axis-scroll"));
            page.layout_bounds = [0.0, 0.0, 400.0, 300.0];
            page = page.child(chip);

            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [110.0, 50.0],
                grab_offset: [10.0, 10.0],
                start_bounds: [100.0, 40.0, 80.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits0 = collect_hits(&page);
            assert!(tick(110.0, 80.0, &hits0, [400.0, 300.0]));
            assert!(
                (overlay_pos.peek()[0] - 100.0).abs() < 0.5,
                "precondition: overlay X at unscrolled slot; {:?}",
                overlay_pos.peek()
            );

            // Parent ScrollArea panned (autoscroll / smooth chase) with no mousemove.
            use_signal(&scroll_offset_x_signal_key("drag-axis-scroll"), || 0.0_f32)
                .set_silent(60.0);
            assert!(
                pump_autoscroll(&page, [400.0, 300.0]),
                "held-still pump must resync overlay after parent scroll"
            );
            let pos = overlay_pos.peek();
            assert!(
                (pos[0] - 40.0).abs() < 1.0,
                "axis=y overlay X must follow the scrolled slot (100-60); got {:?}",
                pos
            );
            let _ = end(&collect_hits(&page));
        });
    }

    #[test]
    fn end_clears_preview_before_on_end() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(true);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(Some("a".into()));
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(Some("list".into()));
            let from_index = Signal::new(Some(0usize));
            let hover_list = Signal::new(Some("list".into()));
            let hover_index = Signal::new(Some(1usize));
            let drag_extent = Signal::new([40.0_f32, 20.0]);
            let saw_stale = std::rc::Rc::new(std::cell::Cell::new(false));
            let flag = saw_stale.clone();
            let hover_index_cb = hover_index;
            let overlay_open_cb = overlay_open;
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                        move |_: &DragEvent| {
                            if overlay_open_cb.peek() || hover_index_cb.peek().is_some() {
                                flag.set(true);
                            }
                        },
                    ))),
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
            assert!(end(&drops));
            assert!(
                !saw_stale.get(),
                "on_drag_end must not see leftover overlay/hover (rebuild would keep a gap)"
            );
        });
    }

    #[test]
    fn tick_resolves_insert_index_when_collision_hits_container() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 5.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 0.0, 40.0, 20.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
            a.index = Some(0);
            a.is_draggable = true;
            let mut b = hit("b", [0.0, 20.0, 40.0, 20.0], true);
            b.index = Some(1);
            b.is_draggable = true;
            let container = hit("list", [0.0, 0.0, 40.0, 80.0], true);
            let hits = vec![container, a, b];
            assert!(tick(10.0, 28.0, &hits, [400.0, 300.0]));
            assert_eq!(hover_index.peek(), Some(1));
            assert_eq!(hover_list.peek().as_deref(), Some("list"));
            let _ = end(&hits);
        });
    }

    #[test]
    fn cancel_does_not_fire_end() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            let ended = std::rc::Rc::new(std::cell::Cell::new(false));
            let ended_cb = ended.clone();
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                        move |_: &DragEvent| ended_cb.set(true),
                    ))),
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(tick(20.0, 10.0, &drops, [400.0, 300.0]));
            assert!(cancel());
            assert!(!is_dragging());
            assert!(!ended.get());
        });
    }

    #[test]
    fn collect_skips_overlay_and_reads_attrs() {
        let mut leaf = Element::new();
        leaf.layout_bounds = [1.0, 2.0, 10.0, 12.0];
        leaf.set_attr(ATTR_DRAG_DROPPABLE, "z");
        leaf.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut ghost = Element::new();
        ghost.set_attr(ATTR_DRAG_OVERLAY, "1");
        ghost.set_attr(ATTR_DRAG_DROPPABLE, "ghost");
        let mut nested = Element::new();
        nested.set_attr(ATTR_DRAG_DROPPABLE, "nested-in-ghost");
        nested.layout_bounds = [0.0, 0.0, 4.0, 4.0];
        ghost = ghost.child(nested);
        let mut root = Element::new();
        root = root.child(leaf).child(ghost);
        let hits = collect_hits(&root);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "z");
    }

    /// `restrict="parent"` must skip the kit hug-content shell and bind the
    /// author container — no id required on that parent.
    #[test]
    fn collect_hits_records_author_parent_skipping_item_shell() {
        let mut island = Element::new();
        island.layout_bounds = [8.0, 8.0, 40.0, 24.0];
        island.set_attr(ATTR_DRAG_DRAGGABLE, "card");
        island.set_attr(ATTR_DRAG_SCOPE, "s");
        let mut shell = Element::new();
        shell.layout_bounds = [8.0, 8.0, 40.0, 24.0];
        shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
        shell = shell.child(island);
        let mut parent = Element::new();
        parent.layout_bounds = [0.0, 0.0, 200.0, 120.0];
        parent = parent.child(shell);
        let hits = collect_hits(&parent);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "card");
        assert_eq!(
            last_parent_bounds("s", "card"),
            Some([0.0, 0.0, 200.0, 120.0]),
            "must skip hug shell; got {:?}",
            last_parent_bounds("s", "card")
        );
    }

    #[test]
    fn singleton_cross_list_hole_keeps_author_gap_through_item_shell() {
        for axis in [Axis::X, Axis::Y] {
            let mut item = Element::new();
            item.layout_bounds = [10.0, 20.0, 40.0, 24.0];
            item.set_attr(ATTR_DRAG_DRAGGABLE, "singleton-gap-card");
            item.set_attr(ATTR_DRAG_SCOPE, "singleton-gap-scope");
            item.set_attr(ATTR_DRAG_LIST, "singleton-gap-list");
            item.set_attr(ATTR_DRAG_INDEX, "0");
            let mut shell = Element::new().child(item);
            shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
            let mut parent = Element::new().child(shell);
            parent.style.gap.width = taffy::LengthPercentage::length(4.0);
            parent.style.gap.height = taffy::LengthPercentage::length(6.0);
            let chip = if axis == Axis::X { 40.0 } else { 24.0 };
            let gap = if axis == Axis::X { 4.0 } else { 6.0 };
            let hits = collect_hits(&parent);
            LAST_HITS.with(|h| *h.borrow_mut() = hits.clone());
            DRAG_PACK_PITCH.set(0.0);
            assert_eq!(
                pack_insert_extent(Some("singleton-gap-list"), axis, chip),
                chip + gap
            );
            let hole =
                insert_hole_origin(&hits, "singleton-gap-list", Some(1), axis, chip).unwrap();
            let advance = if axis == Axis::X {
                hole[0] - 10.0
            } else {
                hole[1] - 20.0
            };
            assert_eq!(
                advance,
                chip + gap,
                "preview and layout must use the same packed row"
            );

            // A subsequent collect replaces authored spacing, including a genuine zero gap.
            parent.style.gap = taffy::Size::zero();
            let hits = collect_hits(&parent);
            assert_eq!(
                row_pitch_along(&hits, Some("singleton-gap-list"), axis, chip),
                chip
            );
        }
    }

    #[test]
    fn insert_index_uses_item_slots_not_container() {
        let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
        a.index = Some(0);
        let mut b = hit("b", [0.0, 20.0, 40.0, 20.0], true);
        b.index = Some(1);
        let container = hit("list", [0.0, 0.0, 40.0, 40.0], true);
        let hits = vec![container, a, b];
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 5.0], Axis::Y),
            Some(0)
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 25.0], Axis::Y),
            Some(1)
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 38.0], Axis::Y),
            Some(2),
            "lower half of the last row inserts after it"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 41.0], Axis::Y),
            Some(2),
            "past the last slot bottom appends"
        );
    }

    #[test]
    fn insert_index_past_last_slot_appends() {
        let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
        a.index = Some(0);
        let hits = vec![a];
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 4.0], Axis::Y),
            Some(0)
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 8.0], Axis::Y),
            Some(0),
            "upper half of the lone row stays on index 0"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 16.0], Axis::Y),
            Some(1),
            "lower half of the lone row appends"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [10.0, 21.0], Axis::Y),
            Some(1)
        );
    }

    #[test]
    fn insert_index_at_source_center_stays_from() {
        let mut a = hit("a", [0.0, 0.0, 80.0, 32.0], true);
        a.index = Some(0);
        let mut b = hit("b", [0.0, 40.0, 80.0, 32.0], true);
        b.index = Some(1);
        let hits = vec![a, b];
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            Some(0),
            "grab at row centre must not report hover=1"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 41.0], Axis::Y),
            Some(1),
            "entering the next layout slot opens the hole"
        );
    }

    /// Gallery `also_droppable` island is `w=100%`. Layout can report each row's
    /// height as the whole list; using that as `end` swallows every probe into
    /// index 0 (or, with a page-tall moving box, past-the-end append).
    #[test]
    fn insert_index_uses_slot_pitch_not_stretched_island_height() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit(
                    [
                        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus",
                        "Neptune",
                    ][i],
                    [0.0, i as f32 * 40.0, 80.0, 320.0],
                    true,
                );
                h.index = Some(i);
                h
            })
            .collect();
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            Some(0),
            "pointer in the first pitch must stay on from"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y),
            Some(1),
            "stretched paint height must not keep hover on row 0; got {:?}",
            insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y)
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 90.0], Axis::Y),
            Some(2)
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 300.0], Axis::Y),
            Some(7),
            "pointer still in the last pitch must not append"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 321.0], Axis::Y),
            Some(8)
        );
    }

    #[test]
    fn insert_index_clustered_slots_do_not_append() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit(
                    [
                        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus",
                        "Neptune",
                    ][i],
                    [0.0, 0.0, 80.0, 32.0],
                    true,
                );
                h.index = Some(i);
                h
            })
            .collect();
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            None,
            "clustered slots cannot infer pitch; session keeps from_index"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 400.0], Axis::Y),
            None,
            "page-offset pointer vs local-Y slots must not report hover=len; got {:?}",
            insert_index_along_axis(&hits, "list", [40.0, 400.0], Axis::Y)
        );
    }

    /// Gallery dimmed Sortable: auto-height chips can report 2px origin jitter
    /// while opacity 0 keeps a real ~40px pitch. Walking a 2px stride always
    /// appends — even when the pointer is still on the first 32px row.
    #[test]
    fn insert_index_jitter_pitch_does_not_append() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit(
                    [
                        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus",
                        "Neptune",
                    ][i],
                    [0.0, i as f32 * 2.0, 80.0, 32.0],
                    true,
                );
                h.index = Some(i);
                h
            })
            .collect();
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            None,
            "jittered origins cannot infer pitch; session keeps from_index"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y),
            None,
            "jittered origins must not walk to len; got {:?}",
            insert_index_along_axis(&hits, "list", [40.0, 50.0], Axis::Y)
        );
    }

    #[test]
    fn insert_index_skips_collapsed_zero_height_slots() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit("x", [0.0, 0.0, 80.0, 0.0], true);
                h.index = Some(i);
                h
            })
            .collect();
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            None,
            "0-height rows must not fall through to append (Gallery: 松手永远进队尾)"
        );
    }

    #[test]
    fn slot_stride_includes_gap_between_consecutive_items() {
        let mut a = hit("a", [0.0, 0.0, 40.0, 20.0], true);
        a.index = Some(0);
        let mut b = hit("b", [0.0, 28.0, 40.0, 20.0], true);
        b.index = Some(1);
        let hits = vec![a, b];
        assert_eq!(
            slot_stride_along_axis(&hits, "list", Axis::Y, 20.0),
            28.0,
            "preview shift must use slot pitch (20 item + 8 gap), not box height"
        );
        assert_eq!(
            slot_stride_along_axis(&hits, "missing", Axis::Y, 20.0),
            20.0
        );
    }

    #[test]
    fn cap_stretched_keeps_naturally_unequal_folder_heights() {
        let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
        inbox.index = Some(0);
        inbox.list_id = Some("tree-root".into());
        let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
        later.index = Some(1);
        later.list_id = Some("tree-root".into());
        let mut hits = vec![inbox, later];
        cap_stretched_indexed_rects(&mut hits);
        assert!(
            (hits[0].rect.height - 200.0).abs() < 0.5,
            "tall folder must keep paint height, got {}",
            hits[0].rect.height
        );
        assert!(
            (hits[1].rect.height - 80.0).abs() < 0.5,
            "short last folder must not inherit the previous stride, got {}",
            hits[1].rect.height
        );
    }

    #[test]
    fn unchanged_insert_slot_ignores_vacated_layout_hits() {
        for axis in [Axis::X, Axis::Y] {
            let mut source = hit("Move me", [0.0, 0.0, 40.0, 16.0], true);
            source.index = Some(0);
            let mut peer = hit("Locked", [0.0, 0.0, 40.0, 16.0], true);
            peer.index = Some(1);
            for index in 0..2 {
                assert_eq!(
                    pack_item_translate(
                        &[source.clone(), peer.clone()],
                        Some("list"),
                        Some(index),
                        Some("list"),
                        Some(0),
                        Some("list"),
                        Some(0),
                        axis,
                    ),
                    [0.0, 0.0],
                    "unchanged order must not introduce a gap from stale hits"
                );
            }
        }
    }

    #[test]
    fn pack_variable_height_reorder_preserves_span() {
        let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
        inbox.index = Some(0);
        inbox.list_id = Some("tree-root".into());
        let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
        later.index = Some(1);
        later.list_id = Some("tree-root".into());
        let hits = vec![inbox, later];
        let later_up = pack_item_translate(
            &hits,
            Some("tree-root"),
            Some(0),
            Some("tree-root"),
            Some(1),
            Some("tree-root"),
            Some(0),
            Axis::Y,
        );
        assert!(
            (later_up[1] - 88.0).abs() < 1.0,
            "Inbox must shift by Later's slot (80+8 gap), not Inbox's 208 stride; got {}",
            later_up[1]
        );
        let later_self = pack_item_translate(
            &hits,
            Some("tree-root"),
            Some(1),
            Some("tree-root"),
            Some(1),
            Some("tree-root"),
            Some(0),
            Axis::Y,
        );
        assert!(
            (later_self[1] + 208.0).abs() < 1.0,
            "Later moves from 208 to 0; got {}",
            later_self[1]
        );
        let packed = pack_same_list_origins(&hits, "tree-root", 1, 0, Axis::Y);
        let later_y = packed.get(&1).unwrap()[1];
        let inbox_y = packed.get(&0).unwrap()[1];
        assert!((later_y - 0.0).abs() < 0.5);
        assert!(
            (inbox_y - 88.0).abs() < 1.0,
            "Inbox packed under Later+gap, got {inbox_y}"
        );
        let span = inbox_y + 200.0;
        assert!(
            (span - 288.0).abs() < 1.0,
            "reorder must not grow the list; span={span}"
        );
    }

    #[test]
    fn slot_extent_uses_each_row() {
        let mut a = hit("a", [0.0, 0.0, 40.0, 200.0], true);
        a.index = Some(0);
        let mut b = hit("b", [0.0, 208.0, 40.0, 80.0], true);
        b.index = Some(1);
        let hits = vec![a, b];
        assert!(
            (slot_extent_along_axis(&hits, "list", Some(0), Axis::Y, 80.0) - 208.0).abs() < 0.5
        );
        assert!((slot_extent_along_axis(&hits, "list", Some(1), Axis::Y, 80.0) - 88.0).abs() < 0.5);
    }

    #[test]
    fn cross_list_source_translate_lands_on_target_hole() {
        let mut src = hit("src", [0.0, 0.0, 80.0, 32.0], true);
        src.list_id = Some("a".into());
        src.index = Some(0);
        let mut dst = hit("dst", [200.0, 0.0, 80.0, 32.0], true);
        dst.list_id = Some("b".into());
        dst.index = Some(0);
        let mut col_b = hit("col-b", [200.0, 0.0, 80.0, 100.0], true);
        col_b.list_id = Some("b".into());
        col_b.index = None;
        let hits = [src, dst, col_b];
        assert_eq!(
            source_translate_to_insert_hole(&hits, "a", 0, "b", Some(0), Axis::Y),
            [200.0, 0.0]
        );
        assert_eq!(
            source_translate_to_insert_hole(&hits, "a", 0, "b", Some(1), Axis::Y),
            [200.0, 32.0],
            "append hole is last origin + fallback extent"
        );
    }

    /// `restrict="corners-arena"`: ghost stays inside the named droppable, not
    /// the window — Gallery 四角旋转 must not leave the dashed box.
    #[test]
    fn tick_restrict_element_clamps_ghost_to_named_box() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::Element("arena".into()),
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
            card.scope_id = "s".into();
            card.is_draggable = true;
            let mut arena = hit("arena", [0.0, 0.0, 200.0, 120.0], true);
            arena.scope_id = "s".into();
            let hits = vec![card, arena];
            assert!(tick(500.0, 500.0, &hits, [800.0, 600.0]));
            let pos = overlay_pos.peek();
            assert!(
                (pos[0] - 160.0).abs() < 0.5 && (pos[1] - 96.0).abs() < 0.5,
                "ghost must clamp to arena (200-40, 120-24), not the 800x600 viewport; got {pos:?}"
            );
            let _ = end(&hits);
            remember_item_restrict("s", "card", Some(Restrict::Viewport));
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(900.0, 900.0, &hits, [800.0, 600.0]));
            let pos = overlay_pos.peek();
            remember_item_restrict("s", "card", None);
            let _ = end(&hits);
            assert!(
                (pos[0] - 760.0).abs() < 0.5 && (pos[1] - 576.0).abs() < 0.5,
                "item restrict=viewport overrides the arena and uses the window; got {pos:?}"
            );
        });
    }

    #[test]
    fn restrict_overflow_hidden_records_clip_box() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::Element("arena".into()),
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Hidden,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
            card.scope_id = "s".into();
            card.is_draggable = true;
            let mut arena = hit("arena", [10.0, 20.0, 200.0, 120.0], true);
            arena.scope_id = "s".into();
            let hits = vec![card, arena];
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(500.0, 500.0, &hits, [800.0, 600.0]));
            let clip = peek_restrict_clip().expect("hidden overflow stores the restrict box");
            assert!(
                (clip[0] - 10.0).abs() < 0.5
                    && (clip[1] - 20.0).abs() < 0.5
                    && (clip[2] - 200.0).abs() < 0.5
                    && (clip[3] - 120.0).abs() < 0.5,
                "clip must be the arena box, got {clip:?}"
            );
            let _ = end(&hits);
            assert!(peek_restrict_clip().is_none());
        });
    }

    #[test]
    fn restrict_overflow_hidden_clip_follows_scrolled_arena() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::Element("arena".into()),
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Hidden,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            let mut card = hit("card", [80.0, 40.0, 40.0, 24.0], false);
            card.scope_id = "s".into();
            card.is_draggable = true;
            let mut arena = hit("arena", [10.0, 20.0, 200.0, 120.0], true);
            arena.scope_id = "s".into();
            let hits = vec![card.clone(), arena];
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(110.0, 62.0, &hits, [800.0, 600.0]));
            let clip0 = peek_restrict_clip().expect("hidden overflow stores the restrict box");
            assert!(
                (clip0[1] - 20.0).abs() < 0.5,
                "initial clip y must be the arena, got {clip0:?}"
            );

            let mut arena_scrolled = hit("arena", [10.0, -60.0, 200.0, 120.0], true);
            arena_scrolled.scope_id = "s".into();
            let mut card_scrolled = card.clone();
            card_scrolled.rect = Rect::from_bounds([80.0, -40.0, 40.0, 24.0]);
            let scrolled = vec![card_scrolled, arena_scrolled];
            assert!(tick(110.0, 62.0, &scrolled, [800.0, 600.0]));
            let clip1 = peek_restrict_clip().expect("clip must survive page scroll");
            assert!(
                (clip1[0] - 10.0).abs() < 0.5
                    && (clip1[1] + 60.0).abs() < 0.5
                    && (clip1[2] - 200.0).abs() < 0.5
                    && (clip1[3] - 120.0).abs() < 0.5,
                "clip shell must track the scrolled arena, got {clip1:?}"
            );
            let pos = overlay_pos.peek();
            assert!(
                pos[1] + 0.5 >= clip1[1] && pos[1] <= clip1[1] + clip1[3] + 0.5,
                "ghost origin must stay inside the scrolled restrict box, pos={pos:?} clip={clip1:?}"
            );

            assert!(tick(110.0, 62.0, &[], [800.0, 600.0]));
            let clip_kept = peek_restrict_clip().expect("failed lookup must keep last clip");
            assert_eq!(clip_kept, clip1, "stale hits must not drop Hidden clip");

            assert!(tick(110.0, 62.0, &hits, [800.0, 600.0]));
            let clip2 = peek_restrict_clip().expect("clip restores after scroll-back");
            assert!(
                (clip2[1] - 20.0).abs() < 0.5,
                "scroll-back must restore the arena clip, got {clip2:?}"
            );
            let _ = end(&hits);
        });
    }

    #[test]
    fn restrict_overflow_hidden_clip_intersects_ancestor_scroll() {
        use crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX;
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::Element("arena".into()),
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Hidden,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            let mut card = Element::new();
            card.layout_bounds = [80.0, 100.0, 40.0, 24.0];
            card.set_attr(ATTR_DRAG_DRAGGABLE, "card");
            card.set_attr(ATTR_DRAG_SCOPE, "s");
            let mut arena = Element::new();
            arena.layout_bounds = [10.0, 20.0, 200.0, 240.0];
            arena.set_attr(ATTR_DRAG_DROPPABLE, "arena");
            arena.set_attr(ATTR_DRAG_SCOPE, "s");
            arena = arena.child(card);
            let mut vp = Element::new().overflow("hidden");
            vp.layout_bounds = [0.0, 80.0, 400.0, 200.0];
            vp.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}page"));
            vp = vp.child(arena);
            let hits = collect_hits(&vp);
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 110.0],
                grab_offset: [20.0, 10.0],
                start_bounds: [80.0, 100.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(110.0, 120.0, &hits, [800.0, 600.0]));
            let clip = peek_restrict_clip().expect("hidden clip");
            assert!(
                (clip[0] - 10.0).abs() < 0.5 && (clip[1] - 20.0).abs() < 0.5,
                "clip shell must stay on the arena, not the scroll viewport; got {clip:?}"
            );
            let ancestor = peek_restrict_ancestor_clip().expect("page overflow");
            assert!(
                (ancestor[1] - 80.0).abs() < 0.5,
                "ancestor clip must be the scroll viewport, got {ancestor:?}"
            );
            let visible = intersect_xywh(clip, ancestor);
            assert!(
                (visible[1] - 80.0).abs() < 0.5 && visible[0] >= 10.0 - 0.5,
                "visible scissor must start at the viewport and keep the arena left, got {visible:?}"
            );
            assert!(
                visible[0] + 0.5 >= 10.0,
                "rotation must not paint in the padding left of the arena; visible={visible:?}"
            );
            let _ = end(&hits);
        });
    }

    /// `restrict="parent"`: ghost stays inside the author box recorded by
    /// [`collect_hits`], not the window and not a named id.
    #[test]
    fn tick_restrict_parent_clamps_ghost_to_author_box() {
        let (_o, _) = create_root(|| {
            let committed_delta = Rc::new(RefCell::new(None));
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 24.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::Parent,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: Some(Rc::new(RefCell::new({
                        let committed_delta = committed_delta.clone();
                        move |ev: &DragEvent| *committed_delta.borrow_mut() = Some(ev.delta)
                    }))),
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            let mut island = Element::new();
            island.layout_bounds = [80.0, 40.0, 40.0, 24.0];
            island.set_attr(ATTR_DRAG_DRAGGABLE, "card");
            island.set_attr(ATTR_DRAG_SCOPE, "s");
            let mut shell = Element::new();
            shell.layout_bounds = [80.0, 40.0, 40.0, 24.0];
            shell.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
            shell = shell.child(island);
            let mut parent = Element::new();
            parent.layout_bounds = [0.0, 0.0, 200.0, 120.0];
            parent = parent.child(shell);
            let hits = collect_hits(&parent);
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "card".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(200.0, 108.0, &hits, [800.0, 600.0]));
            assert!(tick_follow(500.0, 500.0, [800.0, 600.0]));
            let pos = overlay_pos.peek();
            let _ = end(&hits);
            assert!(
                (pos[0] - 160.0).abs() < 0.5 && (pos[1] - 96.0).abs() < 0.5,
                "ghost must clamp to author parent (200-40, 120-24), not the window or hug shell; got {pos:?}"
            );
            let delta = committed_delta.borrow().expect("active drag must commit");
            assert_eq!(
                [80.0 + delta[0], 40.0 + delta[1]],
                pos,
                "drop must persist the restricted preview after dragging beyond the edge"
            );

            // Offset edges deliberately fall between snap points. A second
            // snap in a drop handler could push the committed tile outside.
            use super::super::geometry::SnapTargets;
            let arena = [5.0, 7.0, 200.0, 120.0];
            parent.layout_bounds = arena;
            parent.set_attr("id", "arena");
            let hits = collect_hits(&parent);
            for (restrict, bounds) in [
                (Restrict::Parent, arena),
                (Restrict::Rect(arena), arena),
                (Restrict::Element("arena".into()), arena),
                (Restrict::Viewport, [0.0, 0.0, 205.0, 127.0]),
            ] {
                for (kind, snap) in [
                    ("none", None),
                    (
                        "grid",
                        Some(Snap::grid(SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]))),
                    ),
                    (
                        "end-grid",
                        Some(Snap::grid(
                            SnapGrid::new(32.0, 32.0)
                                .offset([16.0, 16.0])
                                .end_only(true),
                        )),
                    ),
                    (
                        "targets",
                        Some(Snap::targets(
                            SnapTargets::new([
                                SnapGrid::new(8.0, 8.0),
                                SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]),
                            ])
                            .range(14.0),
                        )),
                    ),
                    (
                        "fn",
                        Some(Snap::targets(
                            SnapTargets::default().with_fn(Rc::new(|x, _| Some([x, 72.0]))),
                        )),
                    ),
                ] {
                    for overlay_registered in [false, true] {
                        for pointer in [
                            [-500.0, -500.0],
                            [-500.0, 500.0],
                            [500.0, -500.0],
                            [500.0, 500.0],
                        ] {
                            SCOPES.with(|scopes| {
                                let mut scopes = scopes.borrow_mut();
                                let rt = scopes.get_mut("s").unwrap();
                                rt.restrict = restrict.clone();
                                rt.snap = snap.clone();
                                rt.overlay_registered = overlay_registered;
                            });
                            begin(BeginDrag {
                                scope_id: "s".into(),
                                draggable_id: "card".into(),
                                pointer: [100.0, 52.0],
                                grab_offset: [20.0, 12.0],
                                start_bounds: [80.0, 40.0, 40.0, 24.0],
                                from_list: None,
                                from_index: None,
                                item_type: "chip".into(),
                                group: String::new(),
                                capture_id: "cap".into(),
                            });
                            assert!(tick(101.0, 53.0, &hits, [205.0, 127.0]));
                            assert!(tick_follow(pointer[0], pointer[1], [205.0, 127.0]));
                            let expected = [
                                if pointer[0] < 0.0 {
                                    bounds[0]
                                } else {
                                    bounds[0] + bounds[2] - 40.0
                                },
                                if kind == "fn" {
                                    72.0
                                } else if pointer[1] < 0.0 {
                                    bounds[1]
                                } else {
                                    bounds[1] + bounds[3] - 24.0
                                },
                            ];
                            if kind != "end-grid" {
                                assert_eq!(
                                    overlay_pos.peek(),
                                    expected,
                                    "restricted preview: {restrict:?}, {kind}, {pointer:?}"
                                );
                            }
                            assert!(end(&hits));
                            let delta = committed_delta.borrow().unwrap();
                            assert_eq!(
                                [80.0 + delta[0], 40.0 + delta[1]],
                                expected,
                                "restricted drop: {restrict:?}, {kind}, {pointer:?}, overlay={overlay_registered}"
                            );
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn empty_target_hole_uses_container_origin() {
        let mut src = hit("src", [0.0, 0.0, 80.0, 32.0], true);
        src.list_id = Some("a".into());
        src.index = Some(0);
        let mut col_b = hit("col-b", [200.0, 8.0, 80.0, 100.0], true);
        col_b.list_id = Some("b".into());
        let hits = [src, col_b];
        assert_eq!(
            source_translate_to_insert_hole(&hits, "a", 0, "b", Some(0), Axis::Y),
            [200.0, 8.0]
        );
    }

    #[test]
    fn midpoint_hover_apply_lands_in_opened_slot() {
        use crate::utils::drag_reorder::apply_reorder_vec;
        let mk = |id: &str, y: f32, i: usize| {
            let mut h = hit(id, [0.0, y, 40.0, 20.0], true);
            h.index = Some(i);
            h
        };
        let hits = vec![
            mk("a", 0.0, 0),
            mk("b", 20.0, 1),
            mk("c", 40.0, 2),
            mk("d", 60.0, 3),
        ];
        // Inside `c` (40..60). Past `c`'s bottom (61) appends toward `d`.
        let hover = insert_index_along_axis(&hits, "list", [10.0, 61.0], Axis::Y);
        assert_eq!(hover, Some(3));
        let mut v = vec![0, 1, 2, 3];
        apply_reorder_vec(&mut v, 0, hover.unwrap());
        assert_eq!(
            v,
            vec![1, 2, 3, 0],
            "commit must occupy the preview hole at hover, not hover-1"
        );
        let hover2 = insert_index_along_axis(&hits, "list", [10.0, 25.0], Axis::Y);
        assert_eq!(hover2, Some(1));
        apply_reorder_vec(&mut v, 3, hover2.unwrap());
        assert_eq!(v, vec![1, 0, 2, 3]);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3]);
    }

    /// Gallery hosts Drag in `ScrollArea`. Pointer / `current_bounds` are visual;
    /// `layout_bounds` are pre-scroll. Hits used for overlay_pos + collision must
    /// match the pointer, or the ghost is pinned below the fold (user: 浮层丢失).
    #[test]
    fn collect_hits_rect_is_visual_inside_scrolled_viewport() {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::{
            scroll_offset_y_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
        };

        let (_o, (layout_y, hit_y, grab_y, start_y)) = create_root(|| {
            use_signal(&scroll_offset_y_signal_key("drag-scroll"), || 0.0_f32).set_silent(400.0);
            let mut chip = Element::new()
                .w(80.0)
                .h(32.0)
                .min_h(32.0)
                .flex_shrink(0.0)
                .bg([0.9, 0.9, 0.9, 1.0]);
            chip.set_attr(ATTR_DRAG_DRAGGABLE, "chip");
            chip.set_attr(ATTR_DRAG_SCOPE, "s");
            let mut vp = Element::new()
                .w(200.0)
                .h(120.0)
                .flex_col()
                .overflow_y("hidden")
                .overflow_x("hidden")
                .child(
                    Element::new()
                        .w(80.0)
                        .h(400.0)
                        .min_h(400.0)
                        .flex_shrink(0.0),
                )
                .child(chip);
            vp.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}drag-scroll"));
            vp.scroll_offset = [0.0, 400.0];
            let mut root = Element::new().w(200.0).h(120.0).child(vp);
            let mut tree = crate::layout::LayoutTree::new();
            let _ = tree.build_and_layout(&mut root, 200.0, 120.0, [0.0, 0.0]);

            fn find_chip(el: &Element) -> Option<&Element> {
                if el.attrs.get(ATTR_DRAG_DRAGGABLE).map(String::as_str) == Some("chip") {
                    Some(el)
                } else {
                    el.children.iter().find_map(find_chip)
                }
            }
            let chip = find_chip(&root).expect("chip");
            let layout_y = chip.layout_bounds[1];
            let hits = collect_hits(&root);
            let hit = hits.iter().find(|h| h.id == "chip").expect("hit");
            let pointer = [hit.rect.x + 10.0, hit.rect.y + 8.0];
            let (grab, start) = start_geometry("s", "chip", pointer, [0.0; 4]);
            (layout_y, hit.rect.y, grab[1], start[1])
        });
        assert!(
            layout_y > 300.0,
            "precondition: item layout_bounds stay in content space; layout_y={layout_y}"
        );
        assert!(
            (hit_y - (layout_y - 400.0)).abs() < 3.0,
            "collect_hits must use visual box (layout - scroll), not layout_bounds; layout_y={layout_y} hit_y={hit_y}"
        );
        assert!(
            (start_y - hit_y).abs() < 1.0,
            "start_geometry cache must match the visual hit box; start_y={start_y} hit_y={hit_y}"
        );
        assert!(
            (grab_y - 8.0).abs() < 2.0,
            "grab offset vs visual pointer must not include scroll; grab_y={grab_y}"
        );
    }

    /// Sortable preview translates siblings to open a gap. Collision / insert
    /// index must keep the **layout slots** (scroll-adjusted, no CSS translate),
    /// or `hover_index` chases the moving boxes and live reordering vanishes.
    #[test]
    fn collect_hits_ignores_preview_translate() {
        let mut item = Element::new();
        item.layout_bounds = [0.0, 40.0, 80.0, 32.0];
        item.style.translate = [0.0, -32.0];
        item.set_attr(ATTR_DRAG_DRAGGABLE, "venus");
        item.set_attr(ATTR_DRAG_DROPPABLE, "venus");
        item.set_attr(ATTR_DRAG_LIST, "planets");
        item.set_attr(ATTR_DRAG_INDEX, "1");
        let hits = collect_hits(&item);
        assert_eq!(hits.len(), 1);
        assert!(
            (hits[0].rect.y - 40.0).abs() < 0.5,
            "sortable collision slot must ignore preview translate; got y={}",
            hits[0].rect.y
        );
        assert!(
            (hits[0].rect.height - 32.0).abs() < 0.5,
            "slot height must stay the laid-out size; got h={}",
            hits[0].rect.height
        );
    }

    #[test]
    fn tick_hover_index_uses_slot_not_translated_paint() {
        let (_o, hover) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 5.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 0.0, 40.0, 20.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut a = Element::new();
            a.layout_bounds = [0.0, 0.0, 40.0, 20.0];
            a.set_attr(ATTR_DRAG_DRAGGABLE, "a");
            a.set_attr(ATTR_DRAG_DROPPABLE, "a");
            a.set_attr(ATTR_DRAG_LIST, "list");
            a.set_attr(ATTR_DRAG_INDEX, "0");
            a.set_attr(ATTR_DRAG_SCOPE, "s");
            a.set_attr(ATTR_DRAG_TYPE, "card");
            let mut b = Element::new();
            b.layout_bounds = [0.0, 20.0, 40.0, 20.0];
            b.style.translate = [0.0, -20.0];
            b.set_attr(ATTR_DRAG_DRAGGABLE, "b");
            b.set_attr(ATTR_DRAG_DROPPABLE, "b");
            b.set_attr(ATTR_DRAG_LIST, "list");
            b.set_attr(ATTR_DRAG_INDEX, "1");
            b.set_attr(ATTR_DRAG_SCOPE, "s");
            b.set_attr(ATTR_DRAG_TYPE, "card");
            let mut root = Element::new();
            root = root.child(a).child(b);
            let hits = collect_hits(&root);
            assert!(tick(10.0, 28.0, &hits, [400.0, 300.0]));
            let hover = hover_index.peek();
            let _ = end(&hits);
            hover
        });
        assert_eq!(
            hover,
            Some(1),
            "pointer over b's layout slot must keep hover=1 even if b has already shifted"
        );
    }

    fn drop_hit(
        id: &str,
        scope: &str,
        list: &str,
        index: Option<usize>,
        bounds: [f32; 4],
    ) -> DragHit {
        DragHit {
            id: id.into(),
            scope_id: scope.into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: Rect::from_bounds(bounds),
            item_type: "planet".into(),
            accept: "planet".into(),
            list_id: Some(list.into()),
            index,
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        }
    }

    #[test]
    fn tick_single_column_closest_center_does_not_jump_hover_to_mid_list() {
        let (_o, (at_lift, at_half, at_tail)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 320.0]);
            register_scope(
                "gal-drag-sort".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-sort".into(),
                draggable_id: "Mercury".into(),
                pointer: [40.0, 16.0],
                grab_offset: [40.0, 16.0],
                // Stretched island / list box (not the 32px row). Using this
                // center for insert-index was Gallery: 拖起塌陷, 拖到一半才出现.
                start_bounds: [0.0, 0.0, 80.0, 320.0],
                from_list: Some("planets".into()),
                from_index: Some(0),
                item_type: "planet".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let names = [
                "Mercury", "Venus", "Earth", "Jupiter", "Uranus", "Mars", "Neptune", "Saturn",
            ];
            let mut hits: Vec<DragHit> = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    drop_hit(
                        name,
                        "gal-drag-sort",
                        "planets",
                        Some(i),
                        [0.0, i as f32 * 40.0, 80.0, 32.0],
                    )
                })
                .collect();
            hits.push(drop_hit(
                "planets",
                "gal-drag-sort",
                "planets",
                None,
                [0.0, 0.0, 80.0, 320.0],
            ));
            assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
            let at_lift = hover_index.peek();
            assert!(tick(40.0, 176.0, &hits, [800.0, 600.0]));
            let at_half = hover_index.peek();
            assert!(tick(40.0, 300.0, &hits, [800.0, 600.0]));
            let at_tail = hover_index.peek();
            let _ = end(&hits);
            (at_lift, at_half, at_tail)
        });
        assert_eq!(
            at_lift,
            Some(0),
            "lift on row 0 must not report hover≈len/2 from a stretched start box; got {at_lift:?}"
        );
        assert_eq!(
            at_half,
            Some(4),
            "pointer at mid-list should open the mid slot, not stay stuck or append; got {at_half:?}"
        );
        assert_eq!(
            at_tail,
            Some(7),
            "pointer still inside the last slot must not drop the body (append); got {at_tail:?}"
        );
    }

    /// Gallery: each `also_droppable` island can report the list height. Probing
    /// `dragged.center()` (origin + 160) then always sits past the last 32px
    /// sibling — hover=len, origin closes, dimmed body is translated below the
    /// fold, drop appends (user: 半透明本体丢失, 松手永远进队尾).
    #[test]
    fn tick_stretched_island_hits_follow_pointer_not_box_center() {
        let (_o, (at_lift, at_earth, at_tail)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 320.0]);
            register_scope(
                "gal-drag-sort".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-sort".into(),
                draggable_id: "Mercury".into(),
                pointer: [40.0, 16.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 0.0, 80.0, 320.0],
                from_list: Some("planets".into()),
                from_index: Some(0),
                item_type: "planet".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let names = [
                "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
            ];
            let mut hits: Vec<DragHit> = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    drop_hit(
                        name,
                        "gal-drag-sort",
                        "planets",
                        Some(i),
                        [0.0, i as f32 * 40.0, 80.0, 320.0],
                    )
                })
                .collect();
            hits.push(drop_hit(
                "planets",
                "gal-drag-sort",
                "planets",
                None,
                [0.0, 0.0, 80.0, 320.0],
            ));
            assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
            let at_lift = hover_index.peek();
            assert!(tick(40.0, 90.0, &hits, [800.0, 600.0]));
            let at_earth = hover_index.peek();
            assert!(tick(40.0, 300.0, &hits, [800.0, 600.0]));
            let at_tail = hover_index.peek();
            let _ = end(&hits);
            (at_lift, at_earth, at_tail)
        });
        assert_eq!(
            at_lift,
            Some(0),
            "lift on row 0 must keep hover=from; got {at_lift:?}"
        );
        assert_eq!(
            at_earth,
            Some(2),
            "pointer on Earth must open slot 2, not stay on 0 or append; got {at_earth:?}"
        );
        assert_eq!(
            at_tail,
            Some(7),
            "pointer in the last pitch must not append; got {at_tail:?}"
        );
    }

    /// Gallery: pointer is in window space; SortableItem slots can all report
    /// the island origin (local Y = 0). Walking those slots then always
    /// appends (`hover=len`), the dimmed body is translated out of the
    /// ScrollArea, and `on_drag_end` parks the row at the tail.
    #[test]
    fn tick_clustered_local_slots_do_not_append_or_drop_body() {
        let committed = std::rc::Rc::new(std::cell::Cell::new(None::<Option<usize>>));
        let committed_end = committed.clone();
        let (_o, (at_lift, at_move)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "gal-drag-sort".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: Some(std::rc::Rc::new(std::cell::RefCell::new(
                        move |ev: &DragEvent| {
                            committed_end.set(Some(ev.to_index));
                        },
                    ))),
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-sort".into(),
                draggable_id: "Mercury".into(),
                pointer: [40.0, 416.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 400.0, 80.0, 32.0],
                from_list: Some("planets".into()),
                from_index: Some(0),
                item_type: "planet".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let names = [
                "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
            ];
            let mut hits: Vec<DragHit> = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    drop_hit(
                        name,
                        "gal-drag-sort",
                        "planets",
                        Some(i),
                        [0.0, 0.0, 80.0, 32.0],
                    )
                })
                .collect();
            hits.push(drop_hit(
                "planets",
                "gal-drag-sort",
                "planets",
                None,
                [0.0, 0.0, 80.0, 176.0],
            ));
            assert!(tick(41.0, 416.0, &hits, [800.0, 600.0]));
            let at_lift = hover_index.peek();
            assert!(tick(40.0, 480.0, &hits, [800.0, 600.0]));
            let at_move = hover_index.peek();
            assert!(end(&hits));
            (at_lift, at_move)
        });
        assert_eq!(
            at_lift,
            Some(0),
            "lift must keep hover=from, not len; got {at_lift:?}"
        );
        assert_eq!(
            at_move,
            Some(0),
            "moving a page-offset pointer vs local slots must not append; got {at_move:?}"
        );
        assert_eq!(
            committed.get(),
            Some(Some(0)),
            "commit must stay on from, not the tail; got {:?}",
            committed.get()
        );
    }

    #[test]
    fn foreign_scope_hits_do_not_steal_sortable_hover() {
        let (_o, (at_lift, at_earth)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "gal-drag-sort".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-sort".into(),
                draggable_id: "Mercury".into(),
                pointer: [40.0, 416.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 400.0, 80.0, 32.0],
                from_list: Some("planets".into()),
                from_index: Some(0),
                item_type: "planet".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            // Hide-source list stacked above (same planet names). Its last row is
            // closer to the pointer than planets[1] if the session does not
            // isolate scopes — that was Gallery 半透明本体 collapsing.
            let hits = vec![
                drop_hit(
                    "Mercury",
                    "gal-drag-sort-hide",
                    "planets-hide",
                    Some(0),
                    [0.0, 0.0, 80.0, 32.0],
                ),
                drop_hit(
                    "Earth",
                    "gal-drag-sort-hide",
                    "planets-hide",
                    Some(2),
                    [0.0, 80.0, 80.0, 32.0],
                ),
                drop_hit(
                    "planets-hide",
                    "gal-drag-sort-hide",
                    "planets-hide",
                    None,
                    [0.0, 0.0, 80.0, 320.0],
                ),
                drop_hit(
                    "Mercury",
                    "gal-drag-sort",
                    "planets",
                    Some(0),
                    [0.0, 400.0, 80.0, 32.0],
                ),
                drop_hit(
                    "Venus",
                    "gal-drag-sort",
                    "planets",
                    Some(1),
                    [0.0, 440.0, 80.0, 32.0],
                ),
                drop_hit(
                    "Earth",
                    "gal-drag-sort",
                    "planets",
                    Some(2),
                    [0.0, 480.0, 80.0, 32.0],
                ),
                drop_hit(
                    "planets",
                    "gal-drag-sort",
                    "planets",
                    None,
                    [0.0, 400.0, 80.0, 320.0],
                ),
            ];
            assert!(tick(41.0, 416.0, &hits, [800.0, 600.0]));
            let at_lift = (hover_list.peek(), hover_index.peek());
            assert!(tick(40.0, 496.0, &hits, [800.0, 600.0]));
            let at_earth = (hover_list.peek(), hover_index.peek());
            let _ = end(&hits);
            (at_lift, at_earth)
        });
        assert_eq!(
            at_lift.0.as_deref(),
            Some("planets"),
            "stacked opacity-0 list must not become hover_list; got {:?}",
            at_lift.0
        );
        assert_eq!(
            at_lift.1,
            Some(0),
            "grab in the source slot must keep hover=from so the dimmed body stays; got {:?}",
            at_lift.1
        );
        assert_eq!(
            at_earth.0.as_deref(),
            Some("planets"),
            "duplicate planet ids must resolve drop_hit in the session scope; got {:?}",
            at_earth.0
        );
        assert_eq!(
            at_earth.1,
            Some(2),
            "pointer on dimmed Earth must not append via opacity-0 Earth; got {:?}",
            at_earth.1
        );
    }

    #[test]
    fn empty_scope_hit_does_not_join_named_session() {
        let (_o, hover_list) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "named".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "named".into(),
                draggable_id: "Mercury".into(),
                pointer: [40.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [0.0, 8.0, 80.0, 32.0],
                from_list: Some("planets".into()),
                from_index: Some(0),
                item_type: "planet".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits = vec![
                drop_hit("Venus", "", "planets-hide", Some(1), [0.0, 0.0, 80.0, 80.0]),
                drop_hit(
                    "Earth",
                    "named",
                    "planets",
                    Some(1),
                    [0.0, 48.0, 80.0, 32.0],
                ),
            ];
            assert!(tick(40.0, 20.0, &hits, [800.0, 600.0]));
            let hover = hover_list.peek();
            let _ = end(&hits);
            hover
        });
        assert_eq!(
            hover_list.as_deref(),
            Some("planets"),
            "hit with empty scope_id must not join a named DragProvider; got {hover_list:?}"
        );
    }

    /// Gallery Kanban: first also_droppable island can report the column height.
    /// A uniform `start + stride` loop then swallows every probe into index 0.
    #[test]
    fn insert_index_stretched_first_row_does_not_swallow_column() {
        let mut a = hit("a", [0.0, 0.0, 80.0, 88.0], true);
        a.index = Some(0);
        let mut b = hit("b", [0.0, 40.0, 80.0, 32.0], true);
        b.index = Some(1);
        let hits = vec![a, b];
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 56.0], Axis::Y),
            Some(1),
            "pointer on row 1 must not fall into row 0 when row 0 paint height is stretched"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 16.0], Axis::Y),
            Some(0)
        );
    }

    /// Tail row: pitch < chip height — pointer on the chip must stay on from_index.
    #[test]
    fn pointer_on_short_pitch_tail_row_stays_from_index() {
        let mut a = hit("a", [0.0, 0.0, 80.0, 32.0], true);
        a.index = Some(0);
        a.scope_id = "gal-drag-board".into();
        a.list_id = Some("list".into());
        let mut b = hit("b", [0.0, 38.0, 80.0, 48.0], true);
        b.index = Some(1);
        b.scope_id = "gal-drag-board".into();
        b.list_id = Some("list".into());
        b.is_draggable = true;
        let mut hits = vec![a, b];
        cap_stretched_indexed_rects(&mut hits);
        remember_draggable_bounds("gal-drag-board", "b", [0.0, 38.0, 80.0, 48.0]);
        let sess = DragSession {
            scope_id: "gal-drag-board".into(),
            draggable_id: "b".into(),
            phase: DragPhase::Active,
            start_pointer: [40.0, 82.0],
            last_pointer: [40.0, 82.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 38.0, 80.0, 48.0],
            from_list: Some("list".into()),
            from_index: Some(1),
            item_type: String::new(),
            group: String::new(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        };
        assert_eq!(
            insert_index_along_axis(&hits, "list", [40.0, 82.0], Axis::Y),
            Some(2),
            "slot math alone may append when pitch < chip height"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "list", [40.0, 82.0], None),
            Some(1),
            "pointer still on source layout bounds must keep from_index"
        );
    }

    #[test]
    fn tail_append_clamped_when_dragging_last_item() {
        let sess = DragSession {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            phase: DragPhase::Active,
            start_pointer: [40.0, 120.0],
            last_pointer: [40.0, 120.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 80.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: String::new(),
            group: String::new(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        };
        let mut draft = hit("todo:Draft RFC", [0.0, 0.0, 80.0, 32.0], true);
        draft.list_id = Some("todo".into());
        draft.index = Some(0);
        let mut review = hit("todo:Review PR", [0.0, 40.0, 80.0, 32.0], true);
        review.list_id = Some("todo".into());
        review.index = Some(1);
        let hits = vec![draft, review];
        assert_eq!(
            clamp_tail_append_hover(&sess, "todo", &hits, 2),
            1,
            "2-item list: append after last row is phantom slot while dragging that row"
        );
        let mut sess0 = sess.clone();
        sess0.from_index = Some(0);
        sess0.draggable_id = "todo:Draft RFC".into();
        assert_eq!(
            clamp_tail_append_hover(&sess0, "todo", &hits, 2),
            1,
            "same-list has no slot past last; first row must not open phantom append"
        );
    }

    /// Same-list reorder occupies the N layout slots. `insert_index_along_axis`
    /// reports `max+1` once the pointer is a pitch past the last row; that
    /// phantom index makes siblings vacate the last slot while the dimmed body
    /// is painted one pitch below it (user: 拖到最后再向下一个项高 → 末槽空白).
    #[test]
    fn same_list_past_last_row_keeps_body_in_last_slot() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit(
                    [
                        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus",
                        "Neptune",
                    ][i],
                    [0.0, i as f32 * 40.0, 80.0, 32.0],
                    true,
                );
                h.list_id = Some("planets".into());
                h.index = Some(i);
                h
            })
            .collect();
        let sess = DragSession {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            phase: DragPhase::Active,
            start_pointer: [40.0, 16.0],
            last_pointer: [40.0, 16.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        };
        assert_eq!(
            insert_index_along_axis(&hits, "planets", [40.0, 340.0], Axis::Y),
            Some(8),
            "raw midpoint past last+pitch is Sortable append (len)"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 300.0], None),
            Some(7),
            "pointer in the last slot opens the last real hole"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 340.0], Some(7)),
            Some(7),
            "one pitch past the last row must keep hover on the last slot, not max+1"
        );
        let hover =
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 340.0], Some(7)).unwrap();
        let src =
            source_translate_to_insert_hole(&hits, "planets", 0, "planets", Some(hover), Axis::Y);
        assert_eq!(
            src,
            [0.0, 280.0],
            "dimmed body must sit on Neptune's layout slot, not one pitch below the list"
        );
        assert_eq!(
            source_translate_to_insert_hole(&hits, "planets", 0, "planets", Some(8), Axis::Y),
            [0.0, 320.0],
            "unclamped hover=len is the blank-slot translate (last origin + pitch)"
        );
    }

    /// Gallery semi-transparent Sortable: Mars (index 3) dragged below Jupiter,
    /// then nudged one row — hover must not advance and leave a blank slot.
    #[test]
    fn mars_below_jupiter_pins_hover_when_nudged_one_row() {
        let hits: Vec<DragHit> = (0..8)
            .map(|i| {
                let mut h = hit(
                    [
                        "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus",
                        "Neptune",
                    ][i],
                    [0.0, i as f32 * 40.0, 80.0, 320.0],
                    true,
                );
                h.list_id = Some("planets".into());
                h.index = Some(i);
                h
            })
            .collect();
        let mut hits = hits;
        cap_stretched_indexed_rects(&mut hits);
        let sess = DragSession {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mars".into(),
            phase: DragPhase::Active,
            start_pointer: [40.0, 136.0],
            last_pointer: [40.0, 136.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 120.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(3),
            item_type: "planet".into(),
            group: String::new(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        };
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 190.0], None),
            Some(5),
            "lower half of Jupiter row opens insert after Jupiter"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 230.0], Some(5)),
            Some(5),
            "one pitch below the opened hole must not bump hover"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "planets", [40.0, 250.0], Some(5)),
            Some(6),
            "past hole+pitch band may advance"
        );
        let hole = insert_hole_origin(&hits, "planets", Some(5), Axis::Y, 40.0).unwrap();
        let src = source_translate_to_insert_hole(&hits, "planets", 3, "planets", Some(5), Axis::Y);
        assert_eq!(
            src,
            [hole[0] - 0.0, hole[1] - 120.0],
            "dimmed body translate must match layout hole origin"
        );
    }

    #[test]
    fn kanban_past_todo_content_resolves_doing_column() {
        let sess = DragSession {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            phase: DragPhase::Active,
            start_pointer: [120.0, 120.0],
            last_pointer: [120.0, 120.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 40.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        };
        let mut draft = hit("todo:Draft RFC", [0.0, 0.0, 80.0, 32.0], true);
        draft.scope_id = "gal-drag-board".into();
        draft.list_id = Some("todo".into());
        draft.index = Some(0);
        draft.item_type = "task".into();
        draft.group = "board".into();
        let mut review = hit("todo:Review PR", [0.0, 40.0, 80.0, 32.0], true);
        review.scope_id = "gal-drag-board".into();
        review.list_id = Some("todo".into());
        review.index = Some(1);
        review.item_type = "task".into();
        review.group = "board".into();
        let mut todo_shell = hit("todo", [0.0, 0.0, 80.0, 200.0], true);
        todo_shell.scope_id = "gal-drag-board".into();
        todo_shell.list_id = Some("todo".into());
        todo_shell.item_type = "task".into();
        todo_shell.accept = "task".into();
        todo_shell.group = "board".into();
        todo_shell.item_type = "task".into();
        todo_shell.accept = "task".into();
        todo_shell.group = "board".into();
        let mut doing_shell = hit("doing", [88.0, 0.0, 80.0, 200.0], true);
        doing_shell.scope_id = "gal-drag-board".into();
        doing_shell.list_id = Some("doing".into());
        doing_shell.item_type = "task".into();
        doing_shell.accept = "task".into();
        doing_shell.group = "board".into();
        let mut task = hit("todo:Write tests", [88.0, 0.0, 80.0, 32.0], true);
        task.scope_id = "gal-drag-board".into();
        task.list_id = Some("doing".into());
        task.index = Some(0);
        task.item_type = "task".into();
        task.group = "board".into();
        let hits = vec![draft, review, todo_shell, doing_shell, task];
        let pointer = [120.0, 120.0];
        assert!(
            pointer_past_list_indexed_content(&hits, "todo", pointer, Axis::Y),
            "pointer is below todo indexed rows"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, pointer, None).as_deref(),
            Some("doing"),
            "past todo padding + doing column band must target doing, not phantom todo slot"
        );
    }

    fn nest_file_drag_session() -> DragSession {
        DragSession {
            scope_id: "s".into(),
            draggable_id: "tree-inbox:Beta".into(),
            phase: DragPhase::Active,
            start_pointer: [110.0, 72.0],
            last_pointer: [110.0, 72.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [8.0, 60.0, 204.0, 24.0],
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            axis: Axis::Y,
            motion_axis: Axis::Y,
            start_axis: Axis::Xy,
            threshold: 0.0,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            capture_id: String::new(),
            delay_ms: 0.0,
            activation: Activation::And,
            pressed_at: Instant::now(),
            last_viewport: [0.0, 0.0],
            pull_clone: false,
            data: None,
            sort_mode: SortMode::Insert,
            items: Vec::new(),
        }
    }

    /// Gallery nested folders: Inbox (Alpha, Beta) stacked above Later (Gamma).
    fn nest_folder_file_hits() -> Vec<DragHit> {
        let mut root = hit("tree-root", [0.0, 0.0, 220.0, 280.0], true);
        root.list_id = Some("tree-root".into());
        root.accept = "folder".into();
        root.item_type = String::new();
        let mut inbox_card = hit("folder:Inbox", [0.0, 0.0, 220.0, 88.0], true);
        inbox_card.list_id = Some("tree-root".into());
        inbox_card.index = Some(0);
        inbox_card.item_type = "folder".into();
        let mut inbox_shell = hit("tree-inbox", [8.0, 28.0, 204.0, 52.0], true);
        inbox_shell.list_id = Some("tree-inbox".into());
        inbox_shell.accept = "file".into();
        inbox_shell.group = "nest".into();
        inbox_shell.item_type = String::new();
        let mut alpha = hit("tree-inbox:Alpha", [8.0, 32.0, 204.0, 24.0], true);
        alpha.list_id = Some("tree-inbox".into());
        alpha.index = Some(0);
        alpha.item_type = "file".into();
        alpha.group = "nest".into();
        let mut beta = hit("tree-inbox:Beta", [8.0, 60.0, 204.0, 24.0], true);
        beta.list_id = Some("tree-inbox".into());
        beta.index = Some(1);
        beta.item_type = "file".into();
        beta.group = "nest".into();
        beta.is_draggable = true;
        let mut later_card = hit("folder:Later", [0.0, 108.0, 220.0, 60.0], true);
        later_card.list_id = Some("tree-root".into());
        later_card.index = Some(1);
        later_card.item_type = "folder".into();
        let mut later_shell = hit("tree-later", [8.0, 136.0, 204.0, 28.0], true);
        later_shell.list_id = Some("tree-later".into());
        later_shell.accept = "file".into();
        later_shell.group = "nest".into();
        later_shell.item_type = String::new();
        let mut gamma = hit("tree-later:Gamma", [8.0, 140.0, 204.0, 24.0], true);
        gamma.list_id = Some("tree-later".into());
        gamma.index = Some(0);
        gamma.item_type = "file".into();
        gamma.group = "nest".into();
        vec![
            root,
            inbox_card,
            inbox_shell,
            alpha,
            beta,
            later_card,
            later_shell,
            gamma,
        ]
    }

    /// Pointer in the gap between stacked nested lists must not flip columns
    /// every frame (that grows/shrinks both lists). Stick to the closer slot.
    #[test]
    fn nested_lists_gap_sticky_nearest_slot() {
        let sess = nest_file_drag_session();
        let hits = nest_folder_file_hits();
        let gap = [110.0, 112.0];
        assert!(
            !list_shell_contains(&hits, "tree-inbox", gap)
                && !list_shell_contains(&hits, "tree-later", gap),
            "fixture pointer sits in the folder gap, not inside either inner shell"
        );
        let first = resolve_sortable_hover_list(&hits, &sess, gap, None);
        assert!(
            first.as_deref() == Some("tree-inbox") || first.as_deref() == Some("tree-later"),
            "gap must pick a nested file list, not tree-root; got {first:?}"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, gap, first.as_deref()),
            first,
            "sticky nearest-slot must not oscillate while the pointer stays in the gap"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, gap, Some("tree-inbox")).as_deref(),
            Some("tree-inbox"),
            "12px sticky keeps Inbox when the pointer is between the two folders"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, gap, Some("tree-later")).as_deref(),
            Some("tree-later"),
            "12px sticky keeps Later when the pointer is between the two folders"
        );
    }

    /// Dragging the last inbox file above the first: hover must be slot 0 even
    /// when the uncapped `also_droppable` island still covers Alpha.
    #[test]
    fn nested_same_list_last_item_above_first_opens_head_slot() {
        let sess = nest_file_drag_session();
        let hits = nest_folder_file_hits();
        remember_draggable_bounds("s", "tree-inbox:Beta", [8.0, 28.0, 204.0, 200.0]);
        let above_alpha = [110.0, 40.0];
        assert!(
            hits.iter()
                .any(|h| h.id == "tree-inbox:Alpha" && h.rect.contains_point(above_alpha)),
            "fixture pointer sits on Alpha"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, above_alpha, None).as_deref(),
            Some("tree-inbox")
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "tree-inbox", above_alpha, None),
            Some(0),
            "Beta dragged above Alpha must open slot 0, not stay from_index=1"
        );
    }

    /// Clustered local slots (all y=0) vs page-space press box: pointer on the
    /// folder title is above Beta's press origin, so hover is the head slot.
    #[test]
    fn nested_clustered_slots_last_item_above_press_opens_head() {
        let mut sess = nest_file_drag_session();
        sess.start_bounds = [8.0, 60.0, 204.0, 24.0];
        let mut alpha = hit("tree-inbox:Alpha", [8.0, 0.0, 204.0, 24.0], true);
        alpha.list_id = Some("tree-inbox".into());
        alpha.index = Some(0);
        alpha.item_type = "file".into();
        alpha.group = "nest".into();
        let mut beta = hit("tree-inbox:Beta", [8.0, 0.0, 204.0, 24.0], true);
        beta.list_id = Some("tree-inbox".into());
        beta.index = Some(1);
        beta.item_type = "file".into();
        beta.group = "nest".into();
        beta.is_draggable = true;
        let mut shell = hit("tree-inbox", [8.0, 0.0, 204.0, 24.0], true);
        shell.list_id = Some("tree-inbox".into());
        shell.accept = "file".into();
        shell.group = "nest".into();
        shell.item_type = String::new();
        let hits = vec![shell, alpha, beta];
        let title = [110.0, 12.0];
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "tree-inbox", title, None),
            Some(0),
            "pointer above Beta's press box must open slot 0 even when local slots share y=0"
        );
    }

    /// Pointer below Later: nearest *insert* slot is after Gamma (`len`), not
    /// Gamma's own index (which opens a hole under the Later title).
    #[test]
    fn nested_lists_outside_second_list_uses_last_real_slot() {
        let sess = nest_file_drag_session();
        let hits = nest_folder_file_hits();
        let below = [110.0, 220.0];
        assert!(
            !list_shell_contains(&hits, "tree-later", below),
            "pointer is outside Later's list shell"
        );
        assert_eq!(
            resolve_sortable_hover_list(&hits, &sess, below, None).as_deref(),
            Some("tree-later"),
            "below both folders the nearest file list is Later"
        );
        assert_eq!(
            insert_index_along_axis(&hits, "tree-later", below, Axis::Y),
            None,
            "raw midpoint is far past last+2*pitch"
        );
        assert_eq!(
            resolve_sortable_hover_index(&hits, &sess, "tree-later", below, None),
            Some(1),
            "below Gamma the nearest insert slot is after Gamma, not slot 0"
        );
        let hover = resolve_sortable_hover_index(&hits, &sess, "tree-later", below, None).unwrap();
        let src = source_translate_to_insert_hole(
            &hits,
            "tree-inbox",
            1,
            "tree-later",
            Some(hover),
            Axis::Y,
        );
        assert_eq!(
            src,
            [0.0, 104.0],
            "dimmed body sits in the append hole (Gamma origin + pitch)"
        );
        assert_eq!(
            source_translate_to_insert_hole(&hits, "tree-inbox", 1, "tree-later", Some(0), Axis::Y),
            [0.0, 80.0],
            "hover=0 is the title-gap hole (Gamma's layout origin)"
        );
    }

    /// Activate snapshots Inbox Alpha→Beta stride (item+gap). A one-item Later
    /// has no neighbour stride, so live `slot_extent(Gamma)` is chip-only and
    /// the dimmed body sits flush on Gamma (Gallery: Beta 移到 Gamma 之下).
    #[test]
    fn nested_cross_list_append_under_gamma_keeps_packed_gap() {
        let hits = nest_folder_file_hits();
        let chip = 24.0;
        let source_pitch = row_pitch_along(&hits, Some("tree-inbox"), Axis::Y, chip);
        assert!(
            (source_pitch - 28.0).abs() < 0.5,
            "Inbox Alpha→Beta pitch is item+gap; got {source_pitch}"
        );
        DRAG_PACK_PITCH.set(source_pitch);
        DRAG_PACK_GAP.set(4.0);
        let src =
            source_translate_to_insert_hole(&hits, "tree-inbox", 1, "tree-later", Some(1), Axis::Y);
        let hole = insert_hole_origin(&hits, "tree-later", Some(1), Axis::Y, chip).unwrap();
        DRAG_PACK_PITCH.set(0.0);
        DRAG_PACK_GAP.set(0.0);
        assert_eq!(
            src,
            [0.0, 108.0],
            "append under Gamma must be origin + packed pitch (28), not chip (24); got {src:?}"
        );
        let gamma = hits.iter().find(|h| h.id == "tree-later:Gamma").unwrap();
        let gap = hole[1] - (gamma.rect.y + gamma.rect.height);
        assert!(
            (gap - 4.0).abs() < 0.5,
            "dimmed Beta must keep author gap under Gamma; gap={gap} hole={hole:?} gamma={:?}",
            gamma.rect
        );
    }

    /// After the origin list shrinks, live hits may drop the absolute source.
    /// The target hole origin is still well-defined from the destination list.
    #[test]
    fn cross_list_hole_origin_without_src_hit() {
        let hits = nest_folder_file_hits()
            .into_iter()
            .filter(|h| h.list_id.as_deref() != Some("tree-inbox") || h.index.is_none())
            .collect::<Vec<_>>();
        assert!(
            hits.iter()
                .all(|h| !(h.list_id.as_deref() == Some("tree-inbox") && h.index == Some(1))),
            "precondition: Beta hit is gone"
        );
        DRAG_PACK_PITCH.set(28.0);
        DRAG_PACK_GAP.set(4.0);
        let origin = [8.0, 34.0];
        let hole = insert_hole_origin(&hits, "tree-later", Some(1), Axis::Y, 24.0).unwrap();
        let got = source_translate_to_insert_hole_at(
            &hits,
            "tree-inbox",
            1,
            "tree-later",
            Some(1),
            Axis::Y,
            Some(origin),
        );
        DRAG_PACK_PITCH.set(0.0);
        DRAG_PACK_GAP.set(0.0);
        assert_eq!(
            got,
            Some([hole[0] - origin[0], hole[1] - origin[1]]),
            "missing src hit must not collapse to [0,0]; hole={hole:?} origin={origin:?}"
        );
        assert!(
            got.unwrap()[1].abs() > 8.0,
            "must still move the dimmed body into Later; got {got:?}"
        );
    }

    #[test]
    fn kanban_drag_last_item_below_rows_keeps_hover_index() {
        let (_o, (hover_list, hover_index)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "gal-drag-board".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-board".into(),
                draggable_id: "todo:Review PR".into(),
                pointer: [40.0, 56.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 40.0, 80.0, 32.0],
                from_list: Some("todo".into()),
                from_index: Some(1),
                item_type: "task".into(),
                group: "board".into(),
                capture_id: "cap".into(),
            });
            let mut draft = drop_hit(
                "todo:Draft RFC",
                "gal-drag-board",
                "todo",
                Some(0),
                [0.0, 0.0, 80.0, 32.0],
            );
            let mut review = drop_hit(
                "todo:Review PR",
                "gal-drag-board",
                "todo",
                Some(1),
                [0.0, 40.0, 80.0, 32.0],
            );
            review.is_draggable = true;
            review.item_type = "task".into();
            review.group = "board".into();
            draft.is_draggable = true;
            draft.item_type = "task".into();
            draft.group = "board".into();
            let mut todo_shell = drop_hit(
                "todo",
                "gal-drag-board",
                "todo",
                None,
                [0.0, 0.0, 80.0, 200.0],
            );
            todo_shell.item_type = "task".into();
            todo_shell.accept = "task".into();
            todo_shell.group = "board".into();
            let hits = vec![draft, review, todo_shell];
            assert!(tick(40.0, 100.0, &hits, [800.0, 600.0]));
            let list = hover_list.peek();
            let idx = hover_index.peek();
            let _ = end(&hits);
            (list, idx)
        });
        assert_eq!(hover_list.as_deref(), Some("todo"));
        assert_eq!(
            hover_index,
            Some(1),
            "below rows in todo padding must not open phantom index 2"
        );
    }

    #[test]
    fn pointer_indexed_insert_beats_tail_append_on_stretched_column() {
        let (_o, hover_index) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "gal-drag-board".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-board".into(),
                draggable_id: "todo:Review PR".into(),
                pointer: [40.0, 82.0],
                grab_offset: [40.0, 42.0],
                start_bounds: [0.0, 40.0, 80.0, 48.0],
                from_list: Some("todo".into()),
                from_index: Some(1),
                item_type: "task".into(),
                group: "board".into(),
                capture_id: "cap".into(),
            });
            let mut draft = drop_hit(
                "todo:Draft RFC",
                "gal-drag-board",
                "todo",
                Some(0),
                [0.0, 0.0, 80.0, 88.0],
            );
            let mut review = drop_hit(
                "todo:Review PR",
                "gal-drag-board",
                "todo",
                Some(1),
                [0.0, 40.0, 80.0, 48.0],
            );
            review.is_draggable = true;
            draft.is_draggable = true;
            let hits = vec![draft, review];
            assert!(tick(41.0, 82.0, &hits, [800.0, 600.0]));
            let idx = hover_index.peek();
            let _ = end(&hits);
            idx
        });
        assert_eq!(
            hover_index,
            Some(1),
            "lift on Review PR must stay from_index, not append"
        );
    }

    #[test]
    fn pointer_indexed_row_beats_ghost_column_collision() {
        let (_o, (hover_list, hover_index)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "gal-drag-board".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: true,
                    source_opacity: 0.4,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "gal-drag-board".into(),
                draggable_id: "todo:Review PR".into(),
                pointer: [40.0, 56.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 40.0, 80.0, 32.0],
                from_list: Some("todo".into()),
                from_index: Some(1),
                item_type: "task".into(),
                group: "board".into(),
                capture_id: "cap".into(),
            });
            let mut draft = drop_hit(
                "todo:Draft RFC",
                "gal-drag-board",
                "todo",
                Some(0),
                [0.0, 0.0, 80.0, 32.0],
            );
            let mut review = drop_hit(
                "todo:Review PR",
                "gal-drag-board",
                "todo",
                Some(1),
                [0.0, 40.0, 80.0, 32.0],
            );
            review.is_draggable = true;
            draft.is_draggable = true;
            // Column shell shorter than rows (Kanban label-only container hit).
            let hits = vec![
                draft,
                review,
                drop_hit(
                    "todo",
                    "gal-drag-board",
                    "todo",
                    None,
                    [0.0, 0.0, 80.0, 24.0],
                ),
                drop_hit(
                    "doing",
                    "gal-drag-board",
                    "doing",
                    None,
                    [88.0, 0.0, 80.0, 88.0],
                ),
            ];
            assert!(tick(41.0, 56.0, &hits, [800.0, 600.0]));
            let list = hover_list.peek();
            let idx = hover_index.peek();
            let _ = end(&hits);
            (list, idx)
        });
        assert_eq!(
            hover_list.as_deref(),
            Some("todo"),
            "hover_list must follow the indexed row under the pointer, not ghost collision"
        );
        assert_eq!(
            hover_index,
            Some(1),
            "insert index must stay on the lifted row"
        );
    }

    #[test]
    fn nested_list_shell_beats_parent_folder_row() {
        let (_o, hover) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([40.0_f32, 20.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 8.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "file".into(),
                pointer: [10.0, 310.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 300.0, 40.0, 20.0],
                from_list: Some("inbox".into()),
                from_index: Some(0),
                item_type: "file".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut folder = hit("folder-later", [0.0, 0.0, 200.0, 200.0], true);
            folder.list_id = Some("root".into());
            folder.index = Some(1);
            folder.is_draggable = true;
            folder.item_type = "folder".into();
            let mut nested = hit("later-shell", [10.0, 40.0, 180.0, 140.0], true);
            nested.list_id = Some("later".into());
            nested.index = None;
            nested.accept = "file".into();
            nested.item_type = String::new();
            let mut src = hit("file", [0.0, 300.0, 40.0, 20.0], true);
            src.list_id = Some("inbox".into());
            src.index = Some(0);
            src.is_draggable = true;
            src.item_type = "file".into();
            let hits = vec![folder, nested, src];
            assert!(tick(20.0, 80.0, &hits, [400.0, 400.0]));
            let list = hover_list.peek();
            let _ = end(&hits);
            list
        });
        assert_eq!(
            hover.as_deref(),
            Some("later"),
            "empty nested list padding must win over the wrapping folder row"
        );
    }

    #[test]
    fn empty_nested_inbox_header_accepts_file_drop() {
        let (_o, hover) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([160.0_f32, 32.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 8.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "tree-later:Beta".into(),
                pointer: [20.0, 96.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 80.0, 200.0, 32.0],
                from_list: Some("tree-later".into()),
                from_index: Some(0),
                item_type: "file".into(),
                group: "nest".into(),
                capture_id: "cap".into(),
            });
            let mut folder = hit("folder:Inbox", [0.0, 0.0, 200.0, 48.0], true);
            folder.list_id = Some("tree-root".into());
            folder.index = Some(0);
            folder.is_draggable = true;
            folder.item_type = "folder".into();
            folder.accept = "folder".into();
            folder.group = "nest".into();
            let mut inbox = hit("tree-inbox", [8.0, 40.0, 184.0, 0.0], true);
            inbox.list_id = Some("tree-inbox".into());
            inbox.index = None;
            inbox.accept = "file".into();
            inbox.item_type = String::new();
            inbox.group = "nest".into();
            let mut later_shell = hit("tree-later", [0.0, 80.0, 200.0, 40.0], true);
            later_shell.list_id = Some("tree-later".into());
            later_shell.index = None;
            later_shell.accept = "file".into();
            later_shell.group = "nest".into();
            let mut src = hit("tree-later:Beta", [0.0, 80.0, 200.0, 32.0], true);
            src.list_id = Some("tree-later".into());
            src.index = Some(0);
            src.is_draggable = true;
            src.item_type = "file".into();
            src.accept = "file".into();
            src.group = "nest".into();
            let hits = vec![folder, inbox, later_shell, src];
            assert!(tick(20.0, 16.0, &hits, [400.0, 400.0]));
            let list = hover_list.peek();
            let index = hover_index.peek();
            let _ = end(&hits);
            (list, index)
        });
        assert_eq!(
            hover.0.as_deref(),
            Some("tree-inbox"),
            "collapsed Inbox list must borrow the folder box so a file over the header selects it; hover_list={:?}",
            hover.0
        );
        assert_eq!(
            hover.1,
            Some(0),
            "empty Inbox insert index must be 0, not the origin list's from_index; hover_index={:?}",
            hover.1
        );
    }

    #[test]
    fn origin_empty_shell_does_not_steal_hover_from_dest() {
        let (_o, hover) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([160.0_f32, 32.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 8.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "tree-inbox:Beta".into(),
                pointer: [20.0, 40.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [8.0, 20.0, 184.0, 32.0],
                from_list: Some("tree-inbox".into()),
                from_index: Some(0),
                item_type: "file".into(),
                group: "nest".into(),
                capture_id: "cap".into(),
            });
            let mut folder = hit("folder:Inbox", [0.0, 0.0, 200.0, 80.0], true);
            folder.list_id = Some("tree-root".into());
            folder.index = Some(0);
            folder.item_type = "folder".into();
            folder.accept = "folder".into();
            folder.group = "nest".into();
            let mut inbox = hit("tree-inbox", [8.0, 40.0, 184.0, 0.0], true);
            inbox.list_id = Some("tree-inbox".into());
            inbox.index = None;
            inbox.accept = "file".into();
            inbox.group = "nest".into();
            let mut later = hit("tree-later", [0.0, 30.0, 200.0, 80.0], true);
            later.list_id = Some("tree-later".into());
            later.index = None;
            later.accept = "file".into();
            later.group = "nest".into();
            let mut gamma = hit("tree-later:Gamma", [8.0, 38.0, 184.0, 32.0], true);
            gamma.list_id = Some("tree-later".into());
            gamma.index = Some(0);
            gamma.item_type = "file".into();
            gamma.accept = "file".into();
            gamma.group = "nest".into();
            let hits = vec![folder, inbox, later, gamma];
            assert!(tick(20.0, 70.0, &hits, [400.0, 400.0]));
            let list = hover_list.peek();
            let _ = end(&hits);
            list
        });
        assert_eq!(
            hover.as_deref(),
            Some("tree-later"),
            "vacated origin Inbox must not borrow the folder box and steal Later; hover_list={:?}",
            hover
        );
    }

    #[test]
    fn list_row_extent_uses_packed_stride_not_stretched_chip() {
        let mut a = hit("a", [0.0, 0.0, 40.0, 24.0], true);
        a.index = Some(0);
        a.list_id = Some("list".into());
        let mut b = hit("b", [0.0, 28.0, 40.0, 24.0], true);
        b.index = Some(1);
        b.list_id = Some("list".into());
        LAST_HITS.with(|h| *h.borrow_mut() = vec![a, b]);
        assert!(
            (list_row_extent(Some("list"), Axis::Y, 200.0) - 28.0).abs() < 0.5,
            "stretched island height must not become the vacate row; got {}",
            list_row_extent(Some("list"), Axis::Y, 200.0)
        );
    }

    /// Cross-list padding looks up the *target* list. A one-item Later has no
    /// neighbour stride, so live `list_row_extent` is `chip` and drops the
    /// author gap. `pack_insert_extent` uses the snapshotted source pitch.
    #[test]
    fn list_row_extent_singleton_target_must_keep_source_row_pitch() {
        let mut alpha = hit("tree-inbox:Alpha", [0.0, 0.0, 160.0, 32.0], true);
        alpha.index = Some(0);
        alpha.list_id = Some("tree-inbox".into());
        let mut beta = hit("tree-inbox:Beta", [0.0, 38.0, 160.0, 32.0], true);
        beta.index = Some(1);
        beta.list_id = Some("tree-inbox".into());
        let mut gamma = hit("tree-later:Gamma", [0.0, 80.0, 160.0, 32.0], true);
        gamma.index = Some(0);
        gamma.list_id = Some("tree-later".into());
        LAST_HITS.with(|h| *h.borrow_mut() = vec![alpha, beta, gamma]);
        let source_pitch = list_row_extent(Some("tree-inbox"), Axis::Y, 32.0);
        let live_target = list_row_extent(Some("tree-later"), Axis::Y, 32.0);
        assert!(
            (source_pitch - 38.0).abs() < 0.5,
            "Inbox Alpha→Beta pitch is item+gap; got {source_pitch}"
        );
        assert!(
            (live_target - 32.0).abs() < 0.5,
            "live Later stride is chip-only; got {live_target}"
        );
        DRAG_PACK_PITCH.set(source_pitch);
        DRAG_PACK_GAP.set(6.0);
        let target_pad = pack_insert_extent(Some("tree-later"), Axis::Y, 32.0);
        let gap = pack_gap(32.0);
        DRAG_PACK_PITCH.set(0.0);
        DRAG_PACK_GAP.set(0.0);
        assert!(
            (target_pad - source_pitch).abs() < 0.5,
            "Later pad must open the same packed row, not chip-only; \
             target_pad={target_pad} source_pitch={source_pitch}"
        );
        assert!(
            (gap - 6.0).abs() < 0.5,
            "pack_gap is pitch - chip; got {gap}"
        );
    }

    /// Cross-list: source shell leaves the pack (`position: absolute`). Origin
    /// siblings already reflow into the hole — a second `-extent` FLIP walks
    /// them into the folder header (Gallery: Alpha 拖到 Later 时 Beta 盖住 Inbox).
    /// Target siblings must open the snapshotted packed row, not the live
    /// collapsed source slot (0 gap between dimmed body and Gamma).
    #[test]
    fn cross_list_origin_sibling_stays_put_target_opens_packed_row() {
        let mut alpha = hit("tree-inbox:Alpha", [8.0, 40.0, 160.0, 32.0], true);
        alpha.index = Some(0);
        alpha.list_id = Some("tree-inbox".into());
        let mut beta = hit("tree-inbox:Beta", [8.0, 40.0, 160.0, 32.0], true);
        beta.index = Some(1);
        beta.list_id = Some("tree-inbox".into());
        let mut gamma = hit("tree-later:Gamma", [8.0, 200.0, 160.0, 32.0], true);
        gamma.index = Some(0);
        gamma.list_id = Some("tree-later".into());
        let hits = vec![alpha, beta, gamma];
        DRAG_PACK_PITCH.set(38.0);
        DRAG_PACK_GAP.set(6.0);
        let origin_beta = pack_item_translate(
            &hits,
            Some("tree-inbox"),
            Some(1),
            Some("tree-inbox"),
            Some(0),
            Some("tree-later"),
            Some(0),
            Axis::Y,
        );
        let target_gamma = pack_item_translate(
            &hits,
            Some("tree-later"),
            Some(0),
            Some("tree-inbox"),
            Some(0),
            Some("tree-later"),
            Some(0),
            Axis::Y,
        );
        DRAG_PACK_PITCH.set(0.0);
        DRAG_PACK_GAP.set(0.0);
        assert_eq!(
            origin_beta,
            [0.0, 0.0],
            "origin sibling must not FLIP up after the source left the pack; got {origin_beta:?}"
        );
        assert!(
            (target_gamma[1] - 38.0).abs() < 0.5,
            "target sibling must open the snapshotted packed row (item+gap), not the collapsed live slot; got {}",
            target_gamma[1]
        );
    }

    /// Later is a singleton, so activate snapshots chip-only. Inbox already
    /// has Alpha→Beta author gap — the hole must use that stride, not 32.
    #[test]
    fn cross_list_occupied_target_uses_its_own_packed_row() {
        let mut alpha = hit("tree-inbox:Alpha", [8.0, 40.0, 160.0, 32.0], true);
        alpha.index = Some(0);
        alpha.list_id = Some("tree-inbox".into());
        let mut beta = hit("tree-inbox:Beta", [8.0, 78.0, 160.0, 32.0], true);
        beta.index = Some(1);
        beta.list_id = Some("tree-inbox".into());
        let mut gamma = hit("tree-later:Gamma", [8.0, 200.0, 160.0, 32.0], true);
        gamma.index = Some(0);
        gamma.list_id = Some("tree-later".into());
        let hits = vec![alpha, beta, gamma];
        LAST_HITS.with(|h| *h.borrow_mut() = hits.clone());
        DRAG_PACK_PITCH.set(32.0);
        DRAG_PACK_GAP.set(0.0);
        let above_alpha = pack_item_translate(
            &hits,
            Some("tree-inbox"),
            Some(0),
            Some("tree-later"),
            Some(0),
            Some("tree-inbox"),
            Some(0),
            Axis::Y,
        );
        let above_beta = pack_item_translate(
            &hits,
            Some("tree-inbox"),
            Some(1),
            Some("tree-later"),
            Some(0),
            Some("tree-inbox"),
            Some(1),
            Axis::Y,
        );
        let alpha_stays = pack_item_translate(
            &hits,
            Some("tree-inbox"),
            Some(0),
            Some("tree-later"),
            Some(0),
            Some("tree-inbox"),
            Some(1),
            Axis::Y,
        );
        assert!(!list_is_append_hover("tree-inbox", Some(0)));
        assert!(!list_is_append_hover("tree-inbox", Some(1)));
        assert!(list_is_append_hover("tree-inbox", Some(2)));
        DRAG_PACK_PITCH.set(0.0);
        DRAG_PACK_GAP.set(0.0);
        LAST_HITS.with(|h| h.borrow_mut().clear());
        assert!(
            (above_alpha[1] - 38.0).abs() < 0.5,
            "insert above Alpha must open Inbox pitch (32+6), not Later chip; got {}",
            above_alpha[1]
        );
        assert!(
            (above_beta[1] - 38.0).abs() < 0.5,
            "insert above Beta must open Inbox pitch; got {}",
            above_beta[1]
        );
        assert_eq!(
            alpha_stays,
            [0.0, 0.0],
            "Alpha stays when the hole is below it; got {alpha_stays:?}"
        );
    }

    #[test]
    fn folder_drag_over_inner_files_stays_on_outer_list() {
        let (_o, hover) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([200.0_f32, 80.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::MostIntersecting,
                    axis: Axis::Y,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 8.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: Vec::new(),
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "folder:Later".into(),
                pointer: [20.0, 220.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [0.0, 208.0, 200.0, 80.0],
                from_list: Some("tree-root".into()),
                from_index: Some(1),
                item_type: "folder".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut root = hit("tree-root", [0.0, 0.0, 200.0, 400.0], true);
            root.list_id = Some("tree-root".into());
            root.index = None;
            root.accept = "folder".into();
            root.item_type = String::new();
            let mut inbox = hit("folder:Inbox", [0.0, 0.0, 200.0, 200.0], true);
            inbox.list_id = Some("tree-root".into());
            inbox.index = Some(0);
            inbox.is_draggable = true;
            inbox.item_type = "folder".into();
            let mut inbox_shell = hit("tree-inbox", [8.0, 40.0, 184.0, 150.0], true);
            inbox_shell.list_id = Some("tree-inbox".into());
            inbox_shell.index = None;
            inbox_shell.accept = "file".into();
            inbox_shell.item_type = String::new();
            inbox_shell.group = "nest".into();
            let mut alpha = hit("tree-inbox:Alpha", [8.0, 48.0, 184.0, 32.0], true);
            alpha.list_id = Some("tree-inbox".into());
            alpha.index = Some(0);
            alpha.is_draggable = true;
            alpha.item_type = "file".into();
            alpha.group = "nest".into();
            let mut later = hit("folder:Later", [0.0, 208.0, 200.0, 80.0], true);
            later.list_id = Some("tree-root".into());
            later.index = Some(1);
            later.is_draggable = true;
            later.item_type = "folder".into();
            let hits = vec![root, inbox, inbox_shell, alpha, later];
            assert!(tick(20.0, 60.0, &hits, [400.0, 400.0]));
            let list = hover_list.peek();
            let _ = end(&hits);
            list
        });
        assert_eq!(
            hover.as_deref(),
            Some("tree-root"),
            "folder drag over inner file rows must stay on the outer list, not tree-inbox"
        );
    }

    #[test]
    fn author_modifier_shifts_overlay_origin() {
        use crate::utils::drag::modifier::{DragDelta, DragModifier, MODIFIER_ORDER_AUTHOR};
        let (_o, pos) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([20.0_f32, 20.0]);
            register_scope(
                "s".into(),
                ScopeRuntime {
                    collision: CollisionAlg::PointerWithin,
                    axis: Axis::Xy,
                    start_axis: Axis::Xy,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    restrict_end_only: false,
                    restrict_anchor: RestrictAnchor::FULL,
                    snap: None,
                    restrict_overflow: RestrictOverflow::Visible,
                    overlay_registered: false,
                    source_opacity: 1.0,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
                    activation: Activation::And,
                    collision_fn: None,
                    overlay_align: OverlayAlign::Grab,
                    can_drop: None,
                    on_start: None,
                    on_move: None,
                    on_over: None,
                    on_end: None,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                    stack_seq: 0,
                    modifiers: vec![DragModifier::new("nudge", MODIFIER_ORDER_AUTHOR, |t, _| {
                        DragDelta {
                            x: t.x + 8.0,
                            y: t.y,
                        }
                    })],
                    sensors: Vec::new(),
                },
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [10.0, 10.0, 20.0, 20.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits = vec![hit("a", [10.0, 10.0, 20.0, 20.0], false)];
            assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
            let pos = overlay_pos.peek();
            let _ = end(&hits);
            pos
        });
        assert!(
            (pos[0] - 18.0).abs() < 0.5 && (pos[1] - 30.0).abs() < 0.5,
            "author modifier must shift origin after sensor delta, got {pos:?}"
        );
    }

    #[test]
    fn add_modifier_survives_register_scope() {
        use crate::utils::drag::modifier::{DragDelta, DragModifier, MODIFIER_ORDER_AUTHOR};
        use crate::utils::drag::session::{add_modifier, remove_modifier};
        let (_o, (kept, after_remove)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([20.0_f32, 20.0]);
            let mk = |overlay_open,
                      overlay_pos,
                      active_draggable,
                      active_droppable,
                      from_list,
                      from_index,
                      hover_list,
                      hover_index,
                      drag_extent| ScopeRuntime {
                collision: CollisionAlg::PointerWithin,
                axis: Axis::Xy,
                start_axis: Axis::Xy,
                threshold: 0.0,
                restrict: Restrict::None,
                restrict_end_only: false,
                restrict_anchor: RestrictAnchor::FULL,
                snap: None,
                restrict_overflow: RestrictOverflow::Visible,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: AutoScroll {
                    enabled: false,
                    ..Default::default()
                },
                empty_insert: 0.0,
                delay_ms: 0.0,
                activation: Activation::And,
                collision_fn: None,
                overlay_align: OverlayAlign::Grab,
                can_drop: None,
                on_start: None,
                on_move: None,
                on_over: None,
                on_end: None,
                overlay_open,
                overlay_pos,
                active_draggable,
                active_droppable,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
                stack_seq: 0,
                modifiers: Vec::new(),
                sensors: Vec::new(),
            };
            register_scope(
                "s-mod-rt".into(),
                mk(
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            assert!(add_modifier(
                "s-mod-rt",
                DragModifier::new("nudge", MODIFIER_ORDER_AUTHOR, |t, _| DragDelta {
                    x: t.x + 8.0,
                    y: t.y,
                }),
            ));
            // Provider rebuild replaces builder modifiers; runtime extra must stay.
            register_scope(
                "s-mod-rt".into(),
                mk(
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s-mod-rt".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [0.0, 0.0],
                start_bounds: [10.0, 10.0, 20.0, 20.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let mut src = hit("a", [10.0, 10.0, 20.0, 20.0], false);
            src.scope_id = "s-mod-rt".into();
            let hits = vec![src];
            assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
            let kept = overlay_pos.peek();
            assert!(remove_modifier("s-mod-rt", "nudge"));
            assert!(tick(10.0, 30.0, &hits, [400.0, 300.0]));
            let after_remove = overlay_pos.peek();
            let _ = end(&hits);
            (kept, after_remove)
        });
        assert!(
            (kept[0] - 18.0).abs() < 0.5 && (kept[1] - 30.0).abs() < 0.5,
            "runtime modifier must survive register_scope, got {kept:?}"
        );
        assert!(
            (after_remove[0] - 10.0).abs() < 0.5 && (after_remove[1] - 30.0).abs() < 0.5,
            "remove_modifier must drop the extra stage, got {after_remove:?}"
        );
    }

    fn scope_rt(
        collision: CollisionAlg,
        axis: Axis,
        threshold: f32,
        delay_ms: f32,
        activation: Activation,
        collision_fn: Option<CollisionFn>,
        can_drop: Option<DragPredicate>,
        overlay_align: OverlayAlign,
        overlay_open: Signal<bool>,
        overlay_pos: Signal<[f32; 2]>,
        active_draggable: Signal<Option<String>>,
        active_droppable: Signal<Option<String>>,
        from_list: Signal<Option<String>>,
        from_index: Signal<Option<usize>>,
        hover_list: Signal<Option<String>>,
        hover_index: Signal<Option<usize>>,
        drag_extent: Signal<[f32; 2]>,
    ) -> ScopeRuntime {
        ScopeRuntime {
            collision,
            axis,
            start_axis: Axis::Xy,
            threshold,
            restrict: Restrict::None,
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            snap: None,
            restrict_overflow: RestrictOverflow::Visible,
            overlay_registered: true,
            source_opacity: 1.0,
            autoscroll: AutoScroll {
                enabled: false,
                ..Default::default()
            },
            empty_insert: 0.0,
            delay_ms,
            activation,
            collision_fn,
            overlay_align,
            can_drop,
            on_start: None,
            on_move: None,
            on_over: None,
            on_end: None,
            overlay_open,
            overlay_pos,
            active_draggable,
            active_droppable,
            from_list,
            from_index,
            hover_list,
            hover_index,
            drag_extent,
            stack_seq: 0,
            modifiers: Vec::new(),
            sensors: Vec::new(),
        }
    }

    #[test]
    fn collision_fn_session_picks_non_enum_winner() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            let f: CollisionFn = Rc::new(|_| Some("far".into()));
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::ClosestCenter,
                    Axis::Xy,
                    0.0,
                    0.0,
                    Activation::And,
                    Some(f),
                    None,
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![
                hit("near", [12.0, 0.0, 10.0, 10.0], true),
                hit("far", [80.0, 80.0, 10.0, 10.0], true),
            ];
            assert!(tick(85.0, 85.0, &drops, [400.0, 300.0]));
            assert_eq!(active_droppable.peek().as_deref(), Some("far"));
            let _ = end(&drops);
        });
    }

    #[test]
    fn collision_fn_then_can_drop_vetoes() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            let f: CollisionFn = Rc::new(|_| Some("far".into()));
            let pred: DragPredicate = Rc::new(|ev| ev.droppable_id.as_deref() != Some("far"));
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::ClosestCenter,
                    Axis::Xy,
                    0.0,
                    0.0,
                    Activation::And,
                    Some(f),
                    Some(pred),
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![
                hit("near", [12.0, 0.0, 10.0, 10.0], true),
                hit("far", [80.0, 80.0, 10.0, 10.0], true),
            ];
            assert!(tick(12.0, 5.0, &drops, [400.0, 300.0]));
            assert!(
                active_droppable.peek().is_none(),
                "can_drop must veto after collision_fn selects"
            );
            let _ = end(&drops);
        });
    }

    #[test]
    fn activation_or_fires_on_delay_without_distance() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::PointerWithin,
                    Axis::Xy,
                    999.0,
                    5.0,
                    Activation::Or,
                    None,
                    None,
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            assert!(!tick(10.0, 10.0, &drops, [400.0, 300.0]));
            assert!(!is_active());
            assert!(
                activation_next_wake_at().is_none(),
                "press-only must not arm activation"
            );
            std::thread::sleep(std::time::Duration::from_millis(12));
            assert!(
                !tick(10.0, 10.0, &drops, [400.0, 300.0]),
                "delay without movement is still a press — do not raise/dim the source"
            );
            assert!(!is_active());
            assert!(
                tick(11.0, 10.0, &drops, [400.0, 300.0]),
                "Or + delay already met: first movement must activate immediately"
            );
            assert!(is_active());
            assert!(
                activation_next_wake_at().is_none(),
                "active drag must clear its deadline"
            );
            let _ = end(&drops);
        });
    }

    #[test]
    fn activation_and_needs_distance_even_after_delay() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([0.0_f32, 0.0]);
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::PointerWithin,
                    Axis::Xy,
                    999.0,
                    5.0,
                    Activation::And,
                    None,
                    None,
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let drops = vec![hit("zone", [50.0, 50.0, 80.0, 80.0], true)];
            std::thread::sleep(std::time::Duration::from_millis(12));
            assert!(!tick(10.0, 10.0, &drops, [400.0, 300.0]));
            assert!(!is_active());
            assert!(!tick(11.0, 10.0, &drops, [400.0, 300.0]));
            assert!(
                activation_next_wake_at().is_none(),
                "And still needs the distance threshold"
            );
            let _ = end(&drops);
        });
    }

    fn axis_hit(
        id: &str,
        list: &str,
        bounds: [f32; 4],
        index: Option<usize>,
        list_axis: Axis,
    ) -> DragHit {
        DragHit {
            id: id.into(),
            scope_id: "s".into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: Rect::from_bounds(bounds),
            item_type: "card".into(),
            accept: String::new(),
            list_id: Some(list.into()),
            index,
            group: String::new(),
            disabled: false,
            list_axis: Some(list_axis),
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        }
    }

    #[test]
    fn multi_drag_preview_and_commit_use_the_same_boundary() {
        use crate::utils::drag::{apply_event, drag_scope, use_drag};
        let (_owner, ()) = create_root(|| {
            let items = Signal::new(vec!["a", "b", "c", "d", "e"]);
            let _scope = drag_scope("s")
                .multi_drag(true)
                .threshold(0.0)
                .on_drag_end(move |ev| {
                    let mut v = items.peek();
                    apply_event(&mut v, None, ev);
                    items.set(v);
                })
                .child(|| {
                    use_drag()
                        .unwrap()
                        .selected
                        .set(vec!["a".into(), "b".into(), "c".into()]);
                    Element::new()
                });
            for (pointer_y, expected) in [
                (145.0, vec!["d", "e", "a", "b", "c"]),
                (1.0, vec!["a", "b", "c", "d", "e"]),
            ] {
                let mut hits = vec![axis_hit(
                    "list",
                    "list",
                    [0.0, 0.0, 80.0, 160.0],
                    None,
                    Axis::Y,
                )];
                hits.extend(items.peek().iter().enumerate().map(|(i, id)| {
                    axis_hit(
                        id,
                        "list",
                        [0.0, i as f32 * 24.0, 80.0, 20.0],
                        Some(i),
                        Axis::Y,
                    )
                }));
                let primary = hits.iter().find(|h| h.id == "a").unwrap();
                begin(BeginDrag {
                    scope_id: "s".into(),
                    draggable_id: "a".into(),
                    pointer: [40.0, primary.rect.y + 10.0],
                    grab_offset: [40.0, 10.0],
                    start_bounds: primary.rect.to_bounds(),
                    from_list: Some("list".into()),
                    from_index: primary.index,
                    item_type: "card".into(),
                    group: String::new(),
                    capture_id: "cap".into(),
                });
                assert!(tick(40.0, pointer_y, &hits, [800.0, 600.0]));
                assert_eq!(active_drag_ids(), vec!["a", "b", "c"]);
                let hover = SCOPES
                    .with(|scopes| scopes.borrow()["s"].hover_index.peek())
                    .unwrap();
                for row in hits.iter().filter(|h| h.index.is_some()) {
                    let shift = pack_item_translate(
                        &hits,
                        Some("list"),
                        row.index,
                        Some("list"),
                        primary.index,
                        Some("list"),
                        Some(hover),
                        Axis::Y,
                    );
                    let expected_index = expected.iter().position(|id| *id == row.id).unwrap();
                    assert_eq!(
                        row.rect.y + shift[1],
                        expected_index as f32 * 24.0,
                        "{} must occupy its committed slot",
                        row.id
                    );
                }
                assert!(end(&hits));
                assert_eq!(items.peek(), expected);
            }
        });
    }

    #[test]
    fn shift_selection_uses_live_list_order_and_skips_disabled_rows() {
        let (_owner, ()) = create_root(|| {
            let selected = Signal::new(Vec::new());
            register_scope_product("s".into(), SortMode::Insert, true, selected);
            let mut hits: Vec<_> = ["a", "b", "c", "d", "e"]
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    axis_hit(
                        id,
                        "list",
                        [0.0, i as f32 * 24.0, 80.0, 20.0],
                        Some(i),
                        Axis::Y,
                    )
                })
                .collect();
            hits[3].disabled = true;
            selection_press("s", false);
            try_click_select("s", "b", &hits);
            selection_press("s", true);
            try_click_select("s", "e", &hits);
            assert_eq!(selected.peek(), vec!["b", "c", "e"]);
            selection_press("s", false);
            try_click_select("s", "c", &hits);
            assert_eq!(selected.peek(), vec!["b", "e"]);
        });
    }

    #[test]
    fn swap_hover_picks_item_not_insert_slot() {
        let hits = vec![
            axis_hit("a", "list", [0.0, 0.0, 40.0, 20.0], Some(0), Axis::Y),
            axis_hit("b", "list", [0.0, 24.0, 40.0, 20.0], Some(1), Axis::Y),
            axis_hit("c", "list", [0.0, 48.0, 40.0, 20.0], Some(2), Axis::Y),
        ];
        assert_eq!(
            resolve_swap_hover_index(&hits, "list", [20.0, 34.0]),
            Some(1),
            "pointer inside B"
        );
        assert_eq!(
            resolve_swap_hover_index(&hits, "list", [20.0, 100.0]),
            Some(2),
            "past last row still the last item, not append"
        );
        assert_eq!(
            resolve_swap_hover_index(&hits, "list", [20.0, 10.0]),
            Some(0)
        );
    }

    #[test]
    fn pull_clone_skips_sibling_shift() {
        use crate::utils::drag::registry::{set_item_flags, ItemFlags};
        set_item_flags(
            "s",
            "a",
            ItemFlags {
                pull_clone: true,
                ..Default::default()
            },
        );
        begin(BeginDrag {
            scope_id: "s".into(),
            draggable_id: "a".into(),
            pointer: [0.0, 0.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [0.0, 0.0, 10.0, 10.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(active_pull_clone());
        let mut a = hit("a", [0.0, 0.0, 10.0, 10.0], true);
        a.list_id = Some("list".into());
        a.index = Some(0);
        let mut b = hit("b", [0.0, 10.0, 10.0, 10.0], true);
        b.list_id = Some("list".into());
        b.index = Some(1);
        let hits = vec![a, b];
        assert_eq!(
            pack_item_translate(
                &hits,
                Some("list"),
                Some(1),
                Some("list"),
                Some(0),
                Some("list"),
                Some(2),
                Axis::Y
            ),
            [0.0, 0.0]
        );
        let _ = cancel();
    }

    #[test]
    fn sortable_list_direction_overrides_provider_axis() {
        let (_o, (on_vert, on_horiz)) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::PointerWithin,
                    Axis::Y,
                    0.0,
                    0.0,
                    Activation::And,
                    None,
                    None,
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "v0".into(),
                pointer: [40.0, 16.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 0.0, 80.0, 32.0],
                from_list: Some("vert".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits = vec![
                axis_hit("v0", "vert", [0.0, 0.0, 80.0, 32.0], Some(0), Axis::Y),
                axis_hit("v1", "vert", [0.0, 40.0, 80.0, 32.0], Some(1), Axis::Y),
                axis_hit("v2", "vert", [0.0, 80.0, 80.0, 32.0], Some(2), Axis::Y),
                axis_hit("vert", "vert", [0.0, 0.0, 80.0, 120.0], None, Axis::Y),
                axis_hit("h0", "horiz", [0.0, 200.0, 40.0, 32.0], Some(0), Axis::X),
                axis_hit("h1", "horiz", [40.0, 200.0, 40.0, 32.0], Some(1), Axis::X),
                axis_hit("h2", "horiz", [80.0, 200.0, 40.0, 32.0], Some(2), Axis::X),
                axis_hit("horiz", "horiz", [0.0, 200.0, 120.0, 32.0], None, Axis::X),
            ];
            assert!(tick(40.0, 56.0, &hits, [400.0, 300.0]));
            let on_vert = hover_index.peek();
            assert_eq!(hover_list.peek().as_deref(), Some("vert"));
            assert!(tick(90.0, 216.0, &hits, [400.0, 300.0]));
            let on_horiz = hover_index.peek();
            assert_eq!(hover_list.peek().as_deref(), Some("horiz"));
            let _ = end(&hits);
            (on_vert, on_horiz)
        });
        assert_eq!(on_vert, Some(1), "vertical list still splits on Y");
        assert_eq!(
            on_horiz,
            Some(2),
            "horizontal list must split on X (Y would yield 0)"
        );
    }

    #[test]
    fn tick_follow_moves_ghost_without_crossing_hover_list() {
        let (_o, _) = create_root(|| {
            let overlay_open = Signal::new(false);
            let overlay_pos = Signal::new([0.0_f32, 0.0]);
            let active_draggable = Signal::new(None::<String>);
            let active_droppable = Signal::new(None::<String>);
            let from_list = Signal::new(None::<String>);
            let from_index = Signal::new(None::<usize>);
            let hover_list = Signal::new(None::<String>);
            let hover_index = Signal::new(None::<usize>);
            let drag_extent = Signal::new([80.0_f32, 32.0]);
            register_scope(
                "s".into(),
                scope_rt(
                    CollisionAlg::PointerWithin,
                    Axis::Y,
                    0.0,
                    0.0,
                    Activation::And,
                    None,
                    None,
                    OverlayAlign::Grab,
                    overlay_open,
                    overlay_pos,
                    active_draggable,
                    active_droppable,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                ),
            );
            begin(BeginDrag {
                scope_id: "s".into(),
                draggable_id: "v0".into(),
                pointer: [40.0, 16.0],
                grab_offset: [40.0, 16.0],
                start_bounds: [0.0, 0.0, 80.0, 32.0],
                from_list: Some("vert".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let hits = vec![
                axis_hit("v0", "vert", [0.0, 0.0, 80.0, 32.0], Some(0), Axis::Y),
                axis_hit("v1", "vert", [0.0, 40.0, 80.0, 32.0], Some(1), Axis::Y),
                axis_hit("vert", "vert", [0.0, 0.0, 80.0, 120.0], None, Axis::Y),
                axis_hit("h0", "horiz", [0.0, 200.0, 40.0, 32.0], Some(0), Axis::X),
                axis_hit("horiz", "horiz", [0.0, 200.0, 120.0, 32.0], None, Axis::X),
            ];
            assert!(tick(40.0, 56.0, &hits, [400.0, 300.0]));
            let grab_pos = overlay_pos.peek();
            assert_eq!(hover_list.peek().as_deref(), Some("vert"));
            assert!(tick_follow(90.0, 216.0, [400.0, 300.0]));
            let live = overlay_pos.peek();
            assert_ne!(
                live, grab_pos,
                "ghost origin must follow without collect_hits"
            );
            assert_eq!(
                hover_list.peek().as_deref(),
                Some("vert"),
                "跨区 hover_list must wait for vsync tick"
            );
            assert!(tick(90.0, 216.0, &hits, [400.0, 300.0]));
            assert_eq!(hover_list.peek().as_deref(), Some("horiz"));
            let _ = end(&hits);
        });
    }
}
