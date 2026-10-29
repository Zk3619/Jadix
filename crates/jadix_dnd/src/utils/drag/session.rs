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
#[cfg(all(test, feature = "product-tests"))]
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
#[derive(Clone, Debug, PartialEq)]
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
    /// List-level sortable axis (`jadix_drag_axis`). `None` inherits the provider.
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

crate::authoring::window::window_local! {
    static SESSION: RefCell<Option<DragSession>> = const { RefCell::new(None) };
    static SESSION_LIFETIME: RefCell<Rc<()>> = RefCell::new(Rc::new(()));
    static SCOPE_LIFETIMES: RefCell<HashMap<String, Rc<()>>> = RefCell::new(HashMap::new());
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
    // Logical source slots survive the source shell's cross-list collapse.
    static SOURCE_LIST_LAYOUT: RefCell<Vec<DragHit>> = const { RefCell::new(Vec::new()) };
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
    static DEBUG_SLOT_CLIPS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
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

pub(super) fn debug_slot_clip(scope: &str, id: &str) -> Option<[f32; 4]> {
    DEBUG_SLOT_CLIPS.with(|clips| clips.borrow().get(&item_bounds_key(scope, id)).copied())
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
        .map(|h| {
            // Collision uses the visible intersection; constraints retain the
            // full author box and apply ancestor clipping separately.
            let visible = h.rect.to_bounds();
            last_draggable_bounds(&h.scope_id, id)
                .filter(|raw| {
                    last_ancestor_overflow_clip(id)
                        .is_some_and(|clip| intersect_xywh(*raw, clip) == visible)
                })
                .unwrap_or(visible)
        })
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
    super::session_runtime::install();
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

pub(super) fn guard_update(update: super::runtime::Update) -> super::runtime::Update {
    let lifetime = SESSION_LIFETIME.with(|slot| slot.borrow().clone());
    Box::new(move || {
        let valid = SESSION_LIFETIME.with(|slot| Rc::ptr_eq(&slot.borrow(), &lifetime));
        valid && update()
    })
}

pub(super) fn register_scope(id: String, rt: ScopeRuntime) {
    let lifetime = Rc::new(());
    SCOPE_LIFETIMES.with(|slots| slots.borrow_mut().insert(id.clone(), lifetime.clone()));
    let cleanup_id = id.clone();
    let window = crate::authoring::window::Scope::capture();
    crate::core::reactivity::on_cleanup(move || {
        let _window = window.enter();
        let current = SCOPE_LIFETIMES.with(|slots| {
            slots
                .borrow()
                .get(&cleanup_id)
                .is_some_and(|current| Rc::ptr_eq(current, &lifetime))
        });
        if !current {
            return;
        }
        let owns_session = SESSION.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|session| session.scope_id == cleanup_id)
        });
        if owns_session {
            cancel();
        }
        let still_current = SCOPE_LIFETIMES.with(|slots| {
            slots
                .borrow()
                .get(&cleanup_id)
                .is_some_and(|current| Rc::ptr_eq(current, &lifetime))
        });
        if !still_current {
            return;
        }
        SCOPE_LIFETIMES.with(|slots| slots.borrow_mut().remove(&cleanup_id));
        let removed = SCOPES.with(|slots| slots.borrow_mut().remove(&cleanup_id));
        SCOPE_PRODUCT.with(|slots| slots.borrow_mut().remove(&cleanup_id));
        SCOPE_INERTIA.with(|slots| slots.borrow_mut().remove(&cleanup_id));
        let modifiers = EXTRA_MODIFIERS.with(|slots| slots.borrow_mut().remove(&cleanup_id));
        clear_overlay_ghost_extent(&cleanup_id);
        let prefix = format!("{cleanup_id}\0");
        ITEM_BOUNDS.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        ITEM_RESTRICT.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        NAMED_BOUNDS.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        PARENT_BOUNDS.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        ITEM_PACK_GAPS.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        ANCESTOR_OVERFLOW_CLIP.with(|slots| {
            slots
                .borrow_mut()
                .retain(|key, _| !key.starts_with(&prefix))
        });
        LAST_HITS.with(|hits| hits.borrow_mut().retain(|hit| hit.scope_id != cleanup_id));
        SOURCE_LIST_LAYOUT.with(|hits| hits.borrow_mut().retain(|hit| hit.scope_id != cleanup_id));
        let previews = super::builder::take_scope_previews(&cleanup_id);
        let data = super::registry::take_scope(&cleanup_id);
        let zones = super::dropzone::take_scope(&cleanup_id);
        let sensors = super::sensor::take_scope(&cleanup_id);
        drop((removed, modifiers, previews, data, zones, sensors));
    });
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
    super::session_runtime::install();
    SESSION_LIFETIME.with(|lifetime| *lifetime.borrow_mut() = Rc::new(()));
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
            crate::utils::drag::pointer::begin(
                crate::utils::drag::pointer::DragKind::Scope,
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
    crate::utils::drag::pointer::begin(
        crate::utils::drag::pointer::DragKind::Scope,
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

/// A sensor activator has already accepted its gesture; pointer thresholds and
/// axis gates must not defer its first ghost until a later mouse movement.
pub(super) fn begin_sensor(opts: BeginDrag) {
    let pointer = opts.pointer;
    let hits = super::registry::registered_hits_in_scope(&opts.scope_id);
    begin(opts);
    let (width, height) = crate::utils::modal_surface::modal_viewport_size();
    if tick_session(pointer[0], pointer[1], &hits, [width, height], true) {
        crate::app::runtime::request_redraw();
    }
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
    if (at_end || !sess.restrict_end_only) && overflow.is_hidden() {
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
    crate::utils::drag::pointer::tick(x, y);
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
                crate::authoring::mount::invalidate_paint(source);
                source.style.translate = translate;
                crate::utils::motion::clear_motion(source);
                crate::authoring::mount::invalidate_paint(source);
                crate::authoring::mount::invalidate_collect_paint(root, &path);
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
    tick_session(x, y, hits, viewport, false)
}

fn tick_session(
    x: f32,
    y: f32,
    hits: &[DragHit],
    viewport: [f32; 2],
    sensor_activated: bool,
) -> bool {
    crate::utils::drag::pointer::tick(x, y);
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
        if !sensor_activated && (!activated || !sess.start_axis.allows_start_move(dx, dy)) {
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

fn reset_scope_ui(scope: &str) {
    let signals = SCOPES.with(|slots| {
        slots.borrow().get(scope).map(|runtime| {
            (
                runtime.overlay_open,
                runtime.active_draggable,
                runtime.active_droppable,
                runtime.from_list,
                runtime.from_index,
                runtime.hover_list,
                runtime.hover_index,
            )
        })
    });
    let Some((
        overlay_open,
        active_draggable,
        active_droppable,
        from_list,
        from_index,
        hover_list,
        hover_index,
    )) = signals
    else {
        return;
    };
    // One transition: overlay / active / hover are one session, not six effects.
    DRAG_PACK_PITCH.set(0.0);
    DRAG_PACK_GAP.set(0.0);
    SOURCE_LIST_LAYOUT.with(|layout| layout.borrow_mut().clear());
    clear_restrict_clip_state();
    super::overlay::clear_ghost_follow();
    batch(|| {
        overlay_open.set(false);
        active_draggable.set(None);
        active_droppable.set(None);
        from_list.set(None);
        from_index.set(None);
        hover_list.set(None);
        hover_index.set(None);
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
                crate::utils::drag::pointer::clear();
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
    crate::utils::drag::pointer::clear();
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
    reset_scope_ui(&sess.scope_id);
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
    prepare_inertia(root, viewport)()
}

pub(super) fn prepare_inertia(root: &Element, viewport: [f32; 2]) -> super::runtime::Update {
    if !is_coasting() {
        LAST_COAST_AT.with(|slot| slot.borrow_mut().take());
        return Box::new(|| false);
    }
    let lifetime = SESSION_LIFETIME.with(|slot| slot.borrow().clone());
    let hits = collect_hits(root);
    Box::new(move || {
        let current = SESSION_LIFETIME.with(|slot| Rc::ptr_eq(&slot.borrow(), &lifetime));
        current && is_coasting() && pump_inertia_with_hits(&hits, viewport)
    })
}

fn pump_inertia_with_hits(hits: &[DragHit], viewport: [f32; 2]) -> bool {
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
        return commit_end(hits);
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
            struct CoastTick(bool);
            impl Drop for CoastTick {
                fn drop(&mut self) {
                    COAST_TICK.set(self.0);
                }
            }
            let _coast_tick = CoastTick(COAST_TICK.replace(true));
            tick(next[0], next[1], hits, viewport)
        }
        None => commit_end(hits),
    }
}

pub fn clear_if_any() {
    if is_dragging() {
        let _ = end(&[]);
    }
}

/// Abort without firing `on_drag_end` (Escape). SortableJS has no first-class cancel.
pub fn cancel() -> bool {
    SESSION_LIFETIME.with(|slot| *slot.borrow_mut() = Rc::new(()));
    let Some(sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        return false;
    };
    clear_restrict_clip_state();
    crate::utils::drag::pointer::clear();
    crate::app::release_pointer_capture();
    VELOCITY.with(|v| v.borrow_mut().reset());
    COAST.with(|c| *c.borrow_mut() = None);
    LAST_COAST_AT.with(|t| *t.borrow_mut() = None);
    reset_scope_ui(&sess.scope_id);
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
    prepare_autoscroll(root, viewport)()
}

pub(super) fn prepare_autoscroll(root: &Element, viewport: [f32; 2]) -> super::runtime::Update {
    if !is_active() {
        LAST_SCROLL_AT.with(|t| *t.borrow_mut() = None);
        return Box::new(|| false);
    }
    let Some(pointer) = last_pointer() else {
        return Box::new(|| false);
    };
    let lifetime = SESSION_LIFETIME.with(|slot| slot.borrow().clone());
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
        return Box::new(|| false);
    }
    let hits = collect_hits(root);
    let drift = SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .map(|sess| locked_axis_drift(sess, &hits, overlay_pos))
            .unwrap_or(false)
    });
    Box::new(move || {
        let current = SESSION_LIFETIME.with(|slot| Rc::ptr_eq(&slot.borrow(), &lifetime));
        if current && is_active() && (moved || drift) {
            tick(pointer[0], pointer[1], &hits, viewport);
            return true;
        }
        false
    })
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
    DEBUG_SLOT_CLIPS.with(|clips| clips.borrow_mut().clear());
    let mut out = Vec::new();
    collect_hits_inner(root, [0.0, 0.0], None, None, &mut out);
    cap_stretched_indexed_rects(&mut out);
    super::registry::sync_registered_from_collect(&out);
    out
}

/// `also_droppable` + `w=100%` islands report layout bounds taller than the
/// row pitch. Hits must be **slot** rects (origin + pitch), not paint extent.
fn cap_stretched_indexed_rects(hits: &mut [DragHit]) {
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
    let slot = crate::authoring::mount::collision_bounds(el, scroll_acc);
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
    let hit_slot = if drop_id.is_some() {
        overflow_clip
            .map(|clip| intersect_xywh(slot, clip))
            .unwrap_or(slot)
    } else {
        slot
    };
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
        if let Some(clip) = overflow_clip {
            DEBUG_SLOT_CLIPS.with(|clips| {
                clips
                    .borrow_mut()
                    .insert(item_bounds_key(&scope_id, &id), clip);
            });
        }
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
            rect: Rect::from_bounds(hit_slot),
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
    let mut next_acc = crate::authoring::mount::child_scroll_offset(el, scroll_acc);
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

/// List-level `jadix_drag_axis` overrides the provider. Missing → `fallback`
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

#[expect(
    clippy::too_many_arguments,
    reason = "The drag adapter receives independent session, geometry and owner state without changing its existing interface"
)]
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
        // The source shell closes the origin hole in layout at the column boundary.
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

#[cfg(all(test, feature = "product-tests"))]
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
#[cfg(all(test, feature = "product-tests"))]
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
#[cfg(all(test, feature = "product-tests"))]
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

#[cfg(all(test, feature = "product-tests"))]
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
#[cfg(all(test, feature = "product-tests"))]
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

#[cfg(all(test, feature = "product-tests"))]
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/session.rs"]
mod tests;
