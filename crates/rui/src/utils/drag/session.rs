//! Pointer session: begin → (pending threshold) → active tick → end.
//!
//! `window_events` collects [`DragHit`]s from the laid-out tree and calls [`tick`] /
//! [`end`]. Components start a session from `pointerdown` via [`begin`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use crate::core::reactivity::{batch, Signal};
use crate::core::tree::Element;

use super::autoscroll::AutoScroll;
use super::collision::{detect, Candidate, CollisionAlg};
use super::geometry::{clamp_origin_to_box, Axis, Rect, Restrict, SnapGrid};
use super::{
    ATTR_DRAG_ACCEPT, ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE,
    ATTR_DRAG_GROUP, ATTR_DRAG_INDEX, ATTR_DRAG_LIST, ATTR_DRAG_OVERLAY, ATTR_DRAG_SCOPE,
    ATTR_DRAG_TYPE,
};

pub type DragHandler = Rc<RefCell<dyn FnMut(&DragEvent)>>;
pub type DragPredicate = Rc<dyn Fn(&DragEvent) -> bool>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    Pending,
    Active,
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
    pub delta: [f32; 2],
    pub phase: DragPhase,
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
}

impl DragHit {
    fn accepts(&self, dragged: &DragSession) -> bool {
        if self.disabled || !self.is_droppable {
            return false;
        }
        // Another DragProvider's droppables must not steal hover. Skipping when
        // `hit.scope_id` is empty let Gallery's hide_source list (same item
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
    axis: Axis,
    threshold: f32,
    restrict: Restrict,
    snap: Option<SnapGrid>,
    capture_id: String,
    delay_ms: f32,
    pressed_at: Instant,
}

pub(super) struct ScopeRuntime {
    pub collision: CollisionAlg,
    pub axis: Axis,
    pub threshold: f32,
    pub restrict: Restrict,
    pub snap: Option<SnapGrid>,
    pub overlay_registered: bool,
    /// When true, the source island is fully hidden (`opacity: 0`) while the overlay is open.
    pub hide_source: bool,
    pub autoscroll: AutoScroll,
    pub empty_insert: f32,
    pub delay_ms: f32,
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
}

thread_local! {
    static SESSION: RefCell<Option<DragSession>> = const { RefCell::new(None) };
    static SCOPES: RefCell<HashMap<String, ScopeRuntime>> = RefCell::new(HashMap::new());
    /// Last laid-out item boxes, keyed by `scope\0id`. Handle drags read this so
    /// the ghost tracks the **item**, not the handle's smaller hit box.
    static ITEM_BOUNDS: RefCell<HashMap<String, [f32; 4]>> = RefCell::new(HashMap::new());
    static LAST_HITS: RefCell<Vec<DragHit>> = const { RefCell::new(Vec::new()) };
    static LAST_SCROLL_AT: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

fn item_bounds_key(scope: &str, id: &str) -> String {
    format!("{scope}\0{id}")
}

fn remember_draggable_bounds(scope: &str, id: &str, bounds: [f32; 4]) {
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

/// Walk the laid-out tree so handle `pointerdown` can resolve the item rect.
pub fn refresh_hit_cache(root: &Element) {
    let _ = collect_hits(root);
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

pub(super) fn register_scope(id: String, rt: ScopeRuntime) {
    SCOPES.with(|m| {
        let mut map = m.borrow_mut();
        let mut rt = rt;
        if let Some(old) = map.get(&id) {
            rt.overlay_registered = rt.overlay_registered || old.overlay_registered;
        }
        map.insert(id, rt);
    });
}

pub(super) fn with_scope<R>(id: &str, f: impl FnOnce(&mut ScopeRuntime) -> R) -> Option<R> {
    SCOPES.with(|m| m.borrow_mut().get_mut(id).map(f))
}

pub(super) fn mark_overlay_registered(scope_id: &str) {
    with_scope(scope_id, |rt| rt.overlay_registered = true);
}

pub fn overlay_is_registered(scope_id: &str) -> bool {
    SCOPES.with(|m| {
        m.borrow()
            .get(scope_id)
            .is_some_and(|s| s.overlay_registered)
    })
}

pub fn overlay_hides_source(scope_id: &str) -> bool {
    SCOPES.with(|m| m.borrow().get(scope_id).is_some_and(|s| s.hide_source))
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

/// Translate to apply on the source when no overlay is registered.
pub fn active_source_translate() -> Option<[f32; 2]> {
    SESSION.with(|s| {
        let sess = s.borrow();
        let sess = sess.as_ref()?;
        if sess.phase != DragPhase::Active {
            return None;
        }
        let origin =
            SCOPES.with(|m| m.borrow().get(&sess.scope_id).map(|r| r.overlay_pos.peek()))?;
        Some([
            origin[0] - sess.start_bounds[0],
            origin[1] - sess.start_bounds[1],
        ])
    })
}

fn event_from(
    sess: &DragSession,
    droppable: Option<&DragHit>,
    to_list: Option<String>,
    to_index: Option<usize>,
) -> DragEvent {
    let (dx, dy) = sess.axis.apply(
        sess.last_pointer[0] - sess.start_pointer[0],
        sess.last_pointer[1] - sess.start_pointer[1],
    );
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
    }
}

/// Start a pointer session. Does not fire `on_drag_start` until the threshold is crossed.
pub fn begin(opts: BeginDrag) {
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::Scope,
        opts.draggable_id.clone(),
        opts.pointer[0],
        opts.pointer[1],
    );
    crate::app::set_pointer_capture(opts.capture_id.clone());
    let (axis, threshold, restrict, snap, delay_ms) = SCOPES.with(|m| {
        m.borrow()
            .get(&opts.scope_id)
            .map(|s| (s.axis, s.threshold, s.restrict, s.snap, s.delay_ms))
            .unwrap_or((Axis::Xy, 4.0, Restrict::None, None, 0.0))
    });
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
            threshold,
            restrict,
            snap,
            delay_ms,
            pressed_at: Instant::now(),
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

fn ghost_origin(sess: &DragSession, pointer: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    let (dx, dy) = sess.axis.apply(
        pointer[0] - sess.start_pointer[0],
        pointer[1] - sess.start_pointer[1],
    );
    // Keep the original grab point under the cursor (interact.js).
    let mut origin = [
        sess.start_pointer[0] + dx - sess.grab_offset[0],
        sess.start_pointer[1] + dy - sess.grab_offset[1],
    ];
    if let Some(snap) = sess.snap {
        origin = snap.apply(origin[0], origin[1]);
    }
    match sess.restrict {
        Restrict::None => origin,
        Restrict::Viewport => clamp_origin_to_box(
            origin,
            [sess.start_bounds[2], sess.start_bounds[3]],
            [0.0, 0.0, viewport[0].max(1.0), viewport[1].max(1.0)],
        ),
        Restrict::Rect(r) => {
            clamp_origin_to_box(origin, [sess.start_bounds[2], sess.start_bounds[3]], r)
        }
    }
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
        let stride = slot_stride_along_axis(
            hits,
            list,
            sess.axis,
            match sess.axis {
                Axis::X => width,
                Axis::Y | Axis::Xy => height,
            },
        );
        match sess.axis {
            Axis::X => width = width.min(stride).max(1.0),
            Axis::Y | Axis::Xy => height = height.min(stride).max(1.0),
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

/// Advance the session. `hits` must be collected from the current layout tree.
/// Returns `true` when the UI should redraw.
pub fn tick(x: f32, y: f32, hits: &[DragHit], viewport: [f32; 2]) -> bool {
    crate::utils::pointer_drag::tick(x, y);
    let Some(mut sess) = SESSION.with(|s| s.borrow().clone()) else {
        return false;
    };
    sess.last_pointer = [x, y];
    let dist = {
        let dx = x - sess.start_pointer[0];
        let dy = y - sess.start_pointer[1];
        (dx * dx + dy * dy).sqrt()
    };

    if sess.phase == DragPhase::Pending {
        let waited_ms = sess.pressed_at.elapsed().as_secs_f32() * 1000.0;
        if dist < sess.threshold || waited_ms < sess.delay_ms {
            SESSION.with(|s| *s.borrow_mut() = Some(sess));
            return false;
        }
        sess.phase = DragPhase::Active;
        activate_scope(&sess);
        fire(
            &sess.scope_id,
            HandlerKind::Start,
            &event_from(&sess, None, None, None),
        );
    }

    let origin = ghost_origin(&sess, [x, y], viewport);
    let dragged = moving_item_rect(&sess, hits, origin);

    let (collision, empty_insert, can_drop) = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .map(|s| (s.collision, s.empty_insert, s.can_drop.clone()))
            .unwrap_or((CollisionAlg::default(), 0.0, None))
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
        .map(|h| {
            let rect = if h.index.is_none() && empty_insert > 0.0 {
                h.rect.inflated(empty_insert)
            } else {
                h.rect
            };
            Candidate {
                id: h.id.clone(),
                rect,
            }
        })
        .collect();
    let prev_drop = SCOPES.with(|m| {
        m.borrow()
            .get(&sess.scope_id)
            .and_then(|s| s.active_droppable.peek())
    });
    let mut next_id = detect(
        collision,
        [x, y],
        dragged,
        &candidates,
        prev_drop.as_deref(),
    );
    let mut drop_hit = next_id
        .as_ref()
        .and_then(|id| hits.iter().find(|h| h.id == *id));
    if let (Some(pred), Some(hit)) = (can_drop.as_ref(), drop_hit) {
        let probe = event_from(&sess, Some(hit), hit.list_id.clone(), hit.index);
        if !pred(&probe) {
            next_id = None;
            drop_hit = None;
        }
    }

    LAST_HITS.with(|h| *h.borrow_mut() = hits.to_vec());

    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            if rt.overlay_pos.peek() != origin {
                rt.overlay_pos.set(origin);
            }
            if rt.active_droppable.peek() != next_id {
                rt.active_droppable.set(next_id.clone());
            }
            let hover_list = drop_hit
                .and_then(|h| h.list_id.clone())
                .filter(|s| !s.is_empty());
            // Probe with the pointer, not `dragged.center()`. A stretched
            // island box puts the center past the last row (Gallery: 松手永远进队尾).
            let hover_index = hover_list
                .as_deref()
                .and_then(|list| insert_index_along_axis(hits, list, [x, y], sess.axis))
                .or_else(|| drop_hit.and_then(|h| h.index));
            if rt.hover_list.peek() != hover_list {
                rt.hover_list.set(hover_list.clone());
            }
            if rt.hover_index.peek() != hover_index {
                rt.hover_index.set(hover_index);
            }
        }
    });

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

    SESSION.with(|s| *s.borrow_mut() = Some(sess));
    true
}

fn activate_scope(sess: &DragSession) {
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            rt.active_draggable.set(Some(sess.draggable_id.clone()));
            rt.from_list.set(sess.from_list.clone());
            rt.from_index.set(sess.from_index);
            rt.drag_extent
                .set([sess.start_bounds[2], sess.start_bounds[3]]);
            let origin = [sess.start_bounds[0], sess.start_bounds[1]];
            rt.overlay_pos.set(origin);
            if rt.overlay_registered {
                rt.overlay_open.set(true);
            }
        }
    });
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
pub fn end(hits: &[DragHit]) -> bool {
    let Some(mut sess) = SESSION.with(|s| s.borrow_mut().take()) else {
        return false;
    };
    crate::utils::pointer_drag::clear();
    crate::app::release_pointer_capture();
    let was_active = sess.phase == DragPhase::Active;
    let ev = if was_active {
        sess.phase = DragPhase::Active;
        let drop_id = SCOPES.with(|m| {
            m.borrow()
                .get(&sess.scope_id)
                .and_then(|s| s.active_droppable.peek())
        });
        let drop_hit = drop_id
            .as_ref()
            .and_then(|id| hits.iter().find(|h| h.id == *id));
        let (to_list, to_index) = SCOPES.with(|m| {
            m.borrow()
                .get(&sess.scope_id)
                .map(|s| (s.hover_list.peek(), s.hover_index.peek()))
                .unwrap_or((None, None))
        });
        Some(event_from(&sess, drop_hit, to_list, to_index))
    } else {
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
        fire(&sess.scope_id, HandlerKind::End, &ev);
    }
    was_active
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
    crate::utils::pointer_drag::clear();
    crate::app::release_pointer_capture();
    SCOPES.with(|m| {
        if let Some(rt) = m.borrow().get(&sess.scope_id) {
            reset_scope_ui(rt);
        }
    });
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
    let Some(list) = sess.from_list.clone() else {
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

/// Continue edge auto-scroll while the pointer is held still (vsync / wait).
pub fn pump_autoscroll(root: &Element, viewport: [f32; 2]) -> bool {
    let _ = viewport;
    if !is_active() {
        LAST_SCROLL_AT.with(|t| *t.borrow_mut() = None);
        return false;
    }
    let Some(pointer) = last_pointer() else {
        return false;
    };
    let cfg = SESSION.with(|s| {
        let sess = s.borrow();
        sess.as_ref()
            .and_then(|d| SCOPES.with(|m| m.borrow().get(&d.scope_id).map(|r| r.autoscroll)))
    });
    let Some(cfg) = cfg else {
        return false;
    };
    if !cfg.enabled {
        return false;
    }
    let dt = LAST_SCROLL_AT.with(|slot| {
        let now = Instant::now();
        let dt = slot
            .borrow()
            .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
            .unwrap_or(1.0 / 60.0);
        *slot.borrow_mut() = Some(now);
        dt
    });
    let mut views = Vec::new();
    super::autoscroll::collect_scroll_hits(root, &mut views);
    super::autoscroll::apply(pointer, &views, cfg, dt)
}

/// Walk the laid-out tree and collect drag targets (skip the drag ghost).
///
/// Rects are pointer-space **layout slots** (scroll applied, CSS translate not).
/// Do not use raw `layout_bounds` (pre-scroll) or `visual_hit_box` (includes
/// sortable preview translate).
pub fn collect_hits(root: &Element) -> Vec<DragHit> {
    let mut out = Vec::new();
    collect_hits_inner(root, [0.0, 0.0], &mut out);
    out
}

fn collect_hits_inner(el: &Element, scroll_acc: [f32; 2], out: &mut Vec<DragHit>) {
    if el.attrs.contains_key(ATTR_DRAG_OVERLAY) {
        return;
    }
    // Pointer-space layout slot (scroll applied). Sortable preview translate
    // is ignored so insert-index stays on the original rows.
    let scroll_acc = if el.template_id.as_deref()
        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
        || el.style.position_fixed
    {
        [0.0_f32, 0.0_f32]
    } else {
        scroll_acc
    };
    let drag_id = el.attrs.get(ATTR_DRAG_DRAGGABLE).cloned();
    let drop_id = el.attrs.get(ATTR_DRAG_DROPPABLE).cloned();
    if drag_id.is_some() || drop_id.is_some() {
        let id = drag_id.clone().or(drop_id.clone()).unwrap_or_default();
        let scope_id = el.attrs.get(ATTR_DRAG_SCOPE).cloned().unwrap_or_default();
        let slot = crate::app::tree_hit::collision_hit_box(el, scroll_acc);
        if drag_id.is_some() {
            remember_draggable_bounds(&scope_id, &id, slot);
        }
        let disabled = el
            .attrs
            .get(ATTR_DRAG_DISABLED)
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
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
        });
    }
    let next_acc = crate::app::tree_hit::next_hit_scroll_acc(el, scroll_acc);
    for c in &el.children {
        collect_hits_inner(c, next_acc, out);
    }
}

/// Sortable-style insert index: dropping on a container (no index) appends.
pub fn resolve_insert_index(hit: Option<&DragHit>, fallback_len: usize) -> Option<usize> {
    hit.map(|h| h.index.unwrap_or(fallback_len))
}

/// Sortable insert index: which **layout slot** contains the probe along `axis`.
///
/// Slot end is origin + **pitch** (item size plus author gap), not the paint
/// box height. A `w=100%` also_droppable row can report height = the whole
/// list; using that as `end` swallows every probe into index 0, or a page-tall
/// moving-box center past the last sibling appends. Collapsed (`!is_usable`)
/// rows are skipped so 0-height hits do not fall through to `len`.
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
        Axis::Y | Axis::Xy => pointer[1],
    };
    let fallback = match axis {
        Axis::X => items[0].rect.width,
        Axis::Y | Axis::Xy => items[0].rect.height,
    }
    .max(1.0);
    let stride = slot_stride_along_axis(hits, list_id, axis, fallback);
    for h in &items {
        let start = match axis {
            Axis::X => h.rect.x,
            Axis::Y | Axis::Xy => h.rect.y,
        };
        if p < start + stride {
            return h.index;
        }
    }
    items.iter().filter_map(|h| h.index).max().map(|i| i + 1)
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
            Axis::Y | Axis::Xy => (w[1].rect.y - w[0].rect.y).abs(),
        };
        if stride > 1.0 {
            return stride;
        }
    }
    fallback
}

pub(crate) fn list_slot_stride(list_id: Option<&str>, axis: Axis, fallback: f32) -> f32 {
    let Some(list_id) = list_id else {
        return fallback;
    };
    LAST_HITS.with(|h| slot_stride_along_axis(&h.borrow(), list_id, axis, fallback))
}

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
    let stride = slot_stride_along_axis(hits, list_id, axis, fallback_extent.max(1.0));
    if let Some(hover) = hover_index {
        if let Some(h) = items.iter().find(|h| h.index == Some(hover)) {
            return Some([h.rect.x, h.rect.y]);
        }
        if let Some(last) = items.last() {
            let (dx, dy) = match axis {
                Axis::X => (stride, 0.0),
                Axis::Y | Axis::Xy => (0.0, stride),
            };
            return Some([last.rect.x + dx, last.rect.y + dy]);
        }
    }
    hits.iter()
        .find(|h| h.list_id.as_deref() == Some(list_id) && h.index.is_none())
        .map(|h| [h.rect.x, h.rect.y])
}

/// Translate that places the origin-list source onto the target insert hole.
pub(crate) fn source_translate_to_insert_hole(
    hits: &[DragHit],
    from_list: &str,
    from_index: usize,
    hover_list: &str,
    hover_index: Option<usize>,
    axis: Axis,
) -> [f32; 2] {
    let Some(src) = hits
        .iter()
        .find(|h| h.list_id.as_deref() == Some(from_list) && h.index == Some(from_index))
    else {
        return [0.0, 0.0];
    };
    let fallback = match axis {
        Axis::X => src.rect.width,
        Axis::Y | Axis::Xy => src.rect.height,
    };
    let Some(hole) = insert_hole_origin(hits, hover_list, hover_index, axis, fallback) else {
        return [0.0, 0.0];
    };
    [hole[0] - src.rect.x, hole[1] - src.rect.y]
}

pub(crate) fn live_cross_list_source_translate(
    from_list: &str,
    from_index: usize,
    hover_list: &str,
    hover_index: Option<usize>,
    axis: Axis,
) -> [f32; 2] {
    LAST_HITS.with(|h| {
        source_translate_to_insert_hole(
            &h.borrow(),
            from_list,
            from_index,
            hover_list,
            hover_index,
            axis,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;
    use crate::core::tree::Element;

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
        }
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
                    threshold: 8.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: false,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
            let _ = end(&drops);
            assert!(!is_dragging());
            assert!(active_draggable.peek().is_none());
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: true,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: false,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: false,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
            Some(1),
            "still inside the last slot (20..40) must not append"
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
            insert_index_along_axis(&hits, "list", [10.0, 16.0], Axis::Y),
            Some(0),
            "source centre must stay on from, or the next row slides under the dimmed body"
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: false,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: true,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
            assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: true,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
            assert!(tick(40.0, 16.0, &hits, [800.0, 600.0]));
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

    #[test]
    fn foreign_scope_hits_do_not_steal_sortable_hover() {
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
                "gal-drag-sort".into(),
                ScopeRuntime {
                    collision: CollisionAlg::ClosestCenter,
                    axis: Axis::Y,
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: true,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
                    "Neptune",
                    "gal-drag-sort-hide",
                    "planets-hide",
                    Some(7),
                    [0.0, 384.0, 80.0, 32.0],
                ),
                drop_hit(
                    "planets-hide",
                    "gal-drag-sort-hide",
                    "planets-hide",
                    None,
                    [0.0, 0.0, 80.0, 384.0],
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
                    "planets",
                    "gal-drag-sort",
                    "planets",
                    None,
                    [0.0, 400.0, 80.0, 320.0],
                ),
            ];
            assert!(tick(40.0, 416.0, &hits, [800.0, 600.0]));
            let out = (hover_list.peek(), hover_index.peek());
            let _ = end(&hits);
            out
        });
        assert_eq!(
            hover_list.as_deref(),
            Some("planets"),
            "stacked hide_source list must not become hover_list; got {hover_list:?}"
        );
        assert_eq!(
            hover_index,
            Some(0),
            "grab in the source slot must keep hover=from so the dimmed body stays; got {hover_index:?}"
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
                    threshold: 0.0,
                    restrict: Restrict::None,
                    snap: None,
                    overlay_registered: false,
                    hide_source: false,
                    autoscroll: AutoScroll {
                        enabled: false,
                        ..Default::default()
                    },
                    empty_insert: 0.0,
                    delay_ms: 0.0,
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
}
