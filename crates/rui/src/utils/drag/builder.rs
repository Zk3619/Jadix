//! Dual-write builders: `drag_scope` / `draggable` / `droppable` / `drag_handle`.
//! RSX tags expand to these functions (ADR-9). Not in `prelude`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::pointer_events::{dom_button, EventPhase};
use crate::app::reactive::reactive_view_sized;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{Element, IntoOpacity, LayoutStatus, UnitSlot, ATTR_POINTER_EVENTS};
use crate::layout::dimension::CssDim;
use crate::layout::{dim_kind, DimKind, LayoutTree};
use crate::primitives::vstack;
use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
use crate::utils::motion::{
    clear_motion, motion_is_animating, motion_sync_status, prepare_motion_sync, MotionBootstrap,
    MotionEpsilon, MotionStyle, MotionTransform,
};
use crate::utils::resolve_instance_id;

use super::autoscroll::{AutoScroll, ScrollLock};
use super::collision::{CollisionAlg, CollisionFn, CollisionInput};
use super::dropzone::{DropCheckerFn, DropOverlap, DropzoneHandler};
use super::geometry::{
    Activation, Axis, DragStack, OverlayAlign, Restrict, RestrictAnchor, RestrictOverflow, Snap,
    SnapPointFn, SnapTargets, SortMode,
};
use super::session::{
    active_drag_ids, active_draggable_id, active_pull_clone, active_sort_mode,
    active_source_translate, begin, bump_auto_stack, insert_hole_origin, last_hits,
    live_pack_item_translate, live_same_list_source_translate, overlay_is_registered,
    overlay_source_opacity, pack_insert_extent, peek_sort_axis, register_scope_product,
    remember_draggable_bounds, remember_item_restrict, scope_selected_contains, start_geometry,
    BeginDrag, DragHandler, DragPredicate, ScopeRuntime,
};
use super::sortable::SORTABLE_FLIP_MS;
use super::{
    ATTR_DRAG_ACCEPT, ATTR_DRAG_AXIS, ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE,
    ATTR_DRAG_GROUP, ATTR_DRAG_HANDLE, ATTR_DRAG_HOLE_PREVIEW, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX,
    ATTR_DRAG_ITEM_SHELL, ATTR_DRAG_LIST, ATTR_DRAG_OVER, ATTR_DRAG_PULL_CLONE, ATTR_DRAG_SCOPE,
    ATTR_DRAG_SELECTED, ATTR_DRAG_SKIP_FOLLOW, ATTR_DRAG_SKIP_SHIFT, ATTR_DRAG_SWAP,
    ATTR_DRAG_TYPE,
};
use crate::utils::pointer_action::InertiaOpts;

#[derive(Clone)]
pub struct DragCtx {
    pub id: String,
    pub overlay_open: Signal<bool>,
    pub overlay_pos: Signal<[f32; 2]>,
    pub active_draggable: Signal<Option<String>>,
    pub active_droppable: Signal<Option<String>>,
    pub from_list: Signal<Option<String>>,
    pub from_index: Signal<Option<usize>>,
    pub hover_list: Signal<Option<String>>,
    pub hover_index: Signal<Option<usize>>,
    pub drag_extent: Signal<[f32; 2]>,
    pub axis: Axis,
    /// Sibling / source-hole FLIP. `false` snaps translate (SortableJS `animation: 0`).
    pub animation: bool,
    pub sort_mode: SortMode,
    pub multi_drag: bool,
    /// Click-select set when [`Self::multi_drag`] is on. Survives rebuilds via named signal.
    pub selected: Signal<Vec<String>>,
}

impl DragCtx {
    /// Tick-cached collision slots for this scope (solid-dnd context store analog).
    ///
    /// Peek-only — does **not** subscribe `overlay_pos`. Pair with
    /// [`Self::active_droppable`] / [`Self::overlay_open`] so the island reruns
    /// when the winner changes. While dragging uses tick cache; idle uses registered slots.
    pub fn hits(&self) -> Vec<super::session::DragHit> {
        if super::session::is_dragging() {
            super::session::last_hits_in_scope(&self.id)
        } else {
            super::registry::registered_hits_in_scope(&self.id)
        }
    }
}

#[derive(Clone)]
pub struct DragItemCtx {
    pub id: String,
    pub scope_id: String,
    pub list: Option<String>,
    pub index: Option<usize>,
    pub item_type: String,
    pub group: String,
}

impl DragItemCtx {
    /// Item [`super::registry::set_item_data`] bag (solid-dnd `data` record).
    pub fn data(&self) -> Option<Rc<dyn Any>> {
        super::registry::item_data(&self.scope_id, &self.id)
    }

    pub fn data_as<T: Any + Clone>(&self) -> Option<T> {
        self.data()?.downcast_ref::<T>().cloned()
    }
}

fn drag_context() -> Context<DragCtx> {
    static CTX: OnceLock<Context<DragCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DragCtx>)
}

fn item_context() -> Context<DragItemCtx> {
    static CTX: OnceLock<Context<DragItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<DragItemCtx>)
}

pub fn use_drag() -> Option<DragCtx> {
    use_context(drag_context())
}

pub fn use_drag_item() -> Option<DragItemCtx> {
    use_context(item_context())
}

pub(super) fn require_drag(part: &str) -> DragCtx {
    use_drag().unwrap_or_else(|| {
        panic!("{part} must be used within <DragProvider> / drag_scope");
    })
}

/// Paint-time overlay clone is not under the provider owner — re-provide `DragCtx`.
pub(super) fn provide_drag_ctx(ctx: DragCtx) {
    provide_context(drag_context(), ctx);
}

fn wrap_fragment(mut el: Element) -> Element {
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        Element::from(vstack().children(std::mem::take(&mut el.children)))
    } else {
        el
    }
}

/// Author body of each draggable, used by `<DragOverlay>` when it has no children
/// (solid-dnd default: the ghost follows the live source, it is not a second tree).
#[derive(Clone)]
struct SourcePreview {
    body: Rc<dyn Fn() -> Element>,
    item: DragItemCtx,
    source_opacity: Option<f32>,
}

thread_local! {
    static SOURCE_PREVIEWS: RefCell<HashMap<(String, String), SourcePreview>> =
        RefCell::new(HashMap::new());
    /// Last materialized author tree (before drag chrome). Overlay paint copies
    /// this instead of re-running `ui!` / components — that second evaluation
    /// is not under the live owner and overflows the MSVC-debug stack on
    /// Gallery `card_chip` (HStack + Icon + Label), leaving a bordered dot.
    static SOURCE_SNAPSHOTS: RefCell<HashMap<(String, String), Element>> =
        RefCell::new(HashMap::new());
    /// Overlay follow-source clone re-runs the author body. Nested
    /// `assemble_draggable` / `assemble_droppable` would otherwise reuse the
    /// live `reactive_view` tid and steal (or empty) the on-list island —
    /// Gallery nested folder ghosts collapsed to a header chip.
    static CLONING_OVERLAY_GHOST: Cell<bool> = const { Cell::new(false) };
}

struct OverlayGhostCloneGuard {
    prev: bool,
    _inline: crate::app::reactive::InlineIslandGuard,
}

impl OverlayGhostCloneGuard {
    fn enter() -> Self {
        let inline = crate::app::reactive::InlineIslandGuard::enter();
        CLONING_OVERLAY_GHOST.with(|c| {
            let prev = c.get();
            c.set(true);
            Self {
                prev,
                _inline: inline,
            }
        })
    }
}

impl Drop for OverlayGhostCloneGuard {
    fn drop(&mut self) {
        CLONING_OVERLAY_GHOST.with(|c| c.set(self.prev));
    }
}

fn is_cloning_overlay_ghost() -> bool {
    CLONING_OVERLAY_GHOST.with(|c| c.get())
}

fn register_drag_slot_meta(
    scope: &str,
    id: &str,
    is_draggable: bool,
    is_droppable: bool,
    item_type: &str,
    accept: &str,
    list_id: Option<&str>,
    index: Option<usize>,
    group: &str,
    disabled: bool,
    list_axis: Option<Axis>,
    flags: super::registry::ItemFlags,
) {
    use super::geometry::Rect;
    use super::session::DragHit;
    super::registry::register_slot_meta(DragHit {
        id: id.to_string(),
        scope_id: scope.to_string(),
        is_draggable,
        is_droppable,
        rect: Rect::default(),
        item_type: item_type.to_string(),
        accept: accept.to_string(),
        list_id: list_id.map(String::from),
        index,
        group: group.to_string(),
        disabled,
        list_axis,
        skip_shift: flags.skip_shift,
        skip_follow: flags.skip_follow,
        pull_clone: flags.pull_clone,
    });
}

fn register_source_preview(
    scope: &str,
    id: &str,
    body: Rc<dyn Fn() -> Element>,
    item: DragItemCtx,
    source_opacity: Option<f32>,
) {
    SOURCE_PREVIEWS.with(|m| {
        m.borrow_mut().insert(
            (scope.to_string(), id.to_string()),
            SourcePreview {
                body,
                item,
                source_opacity,
            },
        );
    });
}

fn registered_source_opacity(scope: &str, id: &str) -> Option<f32> {
    SOURCE_PREVIEWS.with(|m| {
        m.borrow()
            .get(&(scope.to_string(), id.to_string()))
            .and_then(|p| p.source_opacity)
    })
}

fn effective_source_opacity(scope: &str, id: &str) -> f32 {
    registered_source_opacity(scope, id).unwrap_or_else(|| overlay_source_opacity(scope))
}

fn remember_source_snapshot(scope: &str, id: &str, el: &Element) {
    SOURCE_SNAPSHOTS.with(|m| {
        m.borrow_mut()
            .insert((scope.to_string(), id.to_string()), clone_visual_tree(el));
    });
}

/// Assemble-time `wrap_fragment(body)` still has `reactive_view` shells
/// (`REACTIVE_PLACEHOLDER_HASH`). Using that copy as the ghost drops nested
/// island text (folder → header-only chip). Prefer a snapshot only when every
/// island has already painted.
fn snapshot_has_unresolved_island(el: &Element) -> bool {
    let mut stack: Vec<*const Element> = vec![el as *const Element];
    while let Some(ptr) = stack.pop() {
        let el = unsafe { &*ptr };
        if el.template_hash.as_deref() == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH) {
            return true;
        }
        for child in &el.children {
            stack.push(child as *const Element);
        }
    }
    false
}

/// Paint-only copy: styles / text / children, no handlers, no Taffy identity.
/// Heap walk — do not map snapshot depth onto the OS stack (Gallery `card_chip`
/// clone used to re-enter `ui!` and overflow MSVC-debug 1 MiB).
fn clone_visual_tree(src: &Element) -> Element {
    fn copy_fields(src: &Element) -> Element {
        let mut dst = Element::new();
        *dst.style = (*src.style).clone();
        *dst.text_style = (*src.text_style).clone();
        dst.text = src.text.clone();
        dst.text_spans = src.text_spans.clone();
        dst.text_shape_suffix = src.text_shape_suffix.clone();
        dst.cursor = src.cursor.clone();
        dst.template_hash = src.template_hash.clone();
        dst.template_id = src.template_id.clone();
        dst.focus_id = src.focus_id.clone();
        dst.image_texture = src.image_texture.clone();
        dst.class_set = src.class_set.clone();
        dst.attrs = src.attrs.clone();
        dst
    }
    let mut root = copy_fields(src);
    let mut stack: Vec<(*const Element, *mut Element)> =
        vec![(src as *const Element, &mut root as *mut Element)];
    while let Some((s, d)) = stack.pop() {
        let src = unsafe { &*s };
        let dst = unsafe { &mut *d };
        dst.children = src.children.iter().map(copy_fields).collect();
        let n = dst.children.len();
        for i in 0..n {
            stack.push((
                &src.children[i] as *const Element,
                &mut dst.children[i] as *mut Element,
            ));
        }
    }
    root
}

/// Item metadata captured with the author body (Overlay `child_for`).
pub(super) fn source_preview_item(scope: &str, draggable_id: &str) -> Option<DragItemCtx> {
    SOURCE_PREVIEWS.with(|m| {
        m.borrow()
            .get(&(scope.to_string(), draggable_id.to_string()))
            .map(|p| p.item.clone())
    })
}

/// Copy the last painted author body (not the dimmed/translated island wrapper).
/// Re-running `body` during overlay paint is a second tree in the wrong owner —
/// Gallery `card_chip` overflows and the ghost collapses to a bordered dot.
pub(super) fn clone_source_preview(scope: &str, draggable_id: &str) -> Option<Element> {
    let key = (scope.to_string(), draggable_id.to_string());
    if let Some(el) = SOURCE_SNAPSHOTS.with(|m| {
        m.borrow().get(&key).and_then(|snap| {
            if snapshot_has_unresolved_island(snap) {
                None
            } else {
                Some(clone_visual_tree(snap))
            }
        })
    }) {
        return Some(el);
    }
    let preview = SOURCE_PREVIEWS.with(|m| m.borrow().get(&key).cloned())?;
    Some(with_child_owner(|| {
        let _ghost = OverlayGhostCloneGuard::enter();
        provide_context(item_context(), preview.item);
        wrap_fragment((preview.body)())
    }))
}

/// `reactive_anchor` is idempotent: later `assemble_draggable` calls with the same
/// tid keep the first closure. Push fresh `index` / `list` through a slot so
/// `for_keyed` remaps after `apply_transfer` do not freeze `from_index`.
fn live_slot<T: Clone + PartialEq + 'static>(key: &str, value: T) -> Signal<T> {
    let sig = use_signal(key, || value.clone());
    if sig.peek() != value {
        sig.set(value);
    }
    sig
}

/// `for_keyed` stamps `template_id` on the element the mapper returns. If that
/// node is the `reactive_view` placeholder, reconcile misses and the item
/// collapses to 0×0 (empty sortable list). Keep the island id on a child.
///
/// `data-stacking-context` belongs on **this** shell, not only DragProvider.
/// Gallery wraps items in a relative arena; the provider then has one child,
/// so its stacking-context opens a single band. Nested `z-index` inside an
/// already-open page_stack (ScrollArea overlay bar) does not split further —
/// overlapping pills share `rects → text` and lower glyphs paint on the upper fill.
///
/// Idle in-flow sortable rows must **not** opt in: each `data-stacking-context`
/// under a live `page_stack_depth` is another glyphon band. Scrolling those
/// rows on/off the gallery page reshapes every label (slow render `text=150ms+`).
fn keyed_item_shell(island: Element) -> Element {
    let mut el = Element::from(
        vstack()
            .min_w(0.0)
            .min_h(0.0)
            .flex_shrink(0.0)
            .child(island),
    )
    .overflow("visible");
    el.set_attr(ATTR_DRAG_ITEM_SHELL, "1");
    el
}

const ATTR_STACKING_CONTEXT: &str = "data-stacking-context";

fn subtree_paints_own_band(el: &Element) -> bool {
    if el.style.z_index != 0
        || el.style.position == taffy::style::Position::Absolute
        || el.style.position_fixed
    {
        return true;
    }
    el.children.iter().any(subtree_paints_own_band)
}

fn sync_stacking_context_attr(el: &mut Element, on: bool) -> bool {
    let have = el.attrs.get(ATTR_STACKING_CONTEXT).map(|s| s.as_str()) == Some("1");
    if have == on {
        return false;
    }
    if on {
        el.set_attr(ATTR_STACKING_CONTEXT, "1");
    } else {
        el.attrs.remove(ATTR_STACKING_CONTEXT);
    }
    true
}

fn trail_margin_px(el: &Element, axis: Axis) -> f32 {
    let edge = match axis {
        Axis::X => el.style.margin.right,
        Axis::Y | Axis::Xy | Axis::Start => el.style.margin.bottom,
    };
    crate::layout::lpa_length_px(edge).unwrap_or(0.0)
}

fn set_trail_margin(el: &mut Element, axis: Axis, px: f32) {
    let edge = match axis {
        Axis::X => &mut el.style.margin.right,
        Axis::Y | Axis::Xy | Axis::Start => &mut el.style.margin.bottom,
    };
    *edge = taffy::style::LengthPercentageAuto::length(px);
}

type AuthorBlock = (
    taffy::style::Dimension,
    taffy::style::Dimension,
    taffy::style::Dimension,
);

fn block_dims(el: &Element, axis: Axis) -> AuthorBlock {
    match axis {
        Axis::X => (
            el.style.size.width,
            el.style.min_size.width,
            el.style.max_size.width,
        ),
        Axis::Y | Axis::Xy | Axis::Start => (
            el.style.size.height,
            el.style.min_size.height,
            el.style.max_size.height,
        ),
    }
}

fn set_block_dims(
    el: &mut Element,
    axis: Axis,
    size: taffy::style::Dimension,
    min: taffy::style::Dimension,
    max: taffy::style::Dimension,
) {
    match axis {
        Axis::X => {
            el.style.size.width = size;
            el.style.min_size.width = min;
            el.style.max_size.width = max;
        }
        Axis::Y | Axis::Xy | Axis::Start => {
            el.style.size.height = size;
            el.style.min_size.height = min;
            el.style.max_size.height = max;
        }
    }
}

fn dims_eq(a: taffy::style::Dimension, b: taffy::style::Dimension) -> bool {
    match (dim_kind(a), dim_kind(b)) {
        (DimKind::Length(x), DimKind::Length(y)) => (x - y).abs() < 0.5,
        (DimKind::Percent(x), DimKind::Percent(y)) => (x - y).abs() < 1e-4,
        (DimKind::Auto, DimKind::Auto) => true,
        _ => false,
    }
}

fn dims_changed(a: AuthorBlock, b: AuthorBlock) -> bool {
    !dims_eq(a.0, b.0) || !dims_eq(a.1, b.1) || !dims_eq(a.2, b.2)
}

/// z-index must live on the author-parented shell (direct child of the
/// stacking-context DragProvider root). Nested pill `z-index` is not a
/// sibling of the other item, so page_stack bands would never split and
/// lower glyphs would paint after every shell.
fn apply_shell_stack_z(
    el: &mut Element,
    id: &str,
    scope: &str,
    active: Signal<Option<String>>,
    stack: DragStack,
    auto_rank: Signal<i32>,
    auto_bumped: &Cell<bool>,
    author_z: &Cell<i32>,
    from_list: Signal<Option<String>>,
    hover_list: Signal<Option<String>>,
    drag_extent: Signal<[f32; 2]>,
    axis: Axis,
    list_sig: Signal<Option<String>>,
    author_margin: &Cell<Option<f32>>,
    author_block: &Cell<Option<AuthorBlock>>,
    author_pos: &Cell<Option<taffy::style::Position>>,
    author_width: &Cell<Option<taffy::style::Dimension>>,
) -> LayoutStatus {
    let is_active = active.peek().as_deref() == Some(id);
    if stack == DragStack::Auto && is_active && !auto_bumped.get() {
        auto_rank.set(bump_auto_stack(scope));
        auto_bumped.set(true);
    }
    let mut want_z = stack.resolve(author_z.get(), auto_rank.peek());
    if stack == DragStack::Auto && is_active {
        want_z = want_z.max(1);
    }
    let mut changed = LayoutStatus::Clean;
    if el.style.z_index != want_z {
        el.style.z_index = want_z;
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    // Cross-list: take the shell out of the origin pack so Taffy flex `gap`
    // is not reserved around a 0-height sibling. Absolute's containing block
    // must be the origin *list* (`position: relative` there). A z-index
    // folder ancestor as CB made `w=100%` fill the header row and flattened
    // the destination card (Gallery: Beta 贴在 Inbox 标题上、Later 一行塌陷).
    let from_l = from_list.peek();
    let hover_l = hover_list.peek();
    let cross = is_active && from_l.is_some() && hover_l.is_some() && from_l != hover_l;
    let _ = (list_sig, drag_extent);
    let base_margin = match author_margin.get() {
        Some(v) => v,
        None => {
            let v = trail_margin_px(el, axis);
            author_margin.set(Some(v));
            v
        }
    };
    let have_margin = trail_margin_px(el, axis);
    if (have_margin - base_margin).abs() > 0.5 {
        set_trail_margin(el, axis, base_margin);
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if author_pos.get().is_none() {
        author_pos.set(Some(el.style.position));
    }
    if author_width.get().is_none() {
        author_width.set(Some(el.style.size.width));
    }
    let want_pos = if cross {
        taffy::style::Position::Absolute
    } else {
        author_pos.get().unwrap_or(taffy::style::Position::Relative)
    };
    if el.style.position != want_pos {
        el.style.position = want_pos;
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    let want_width = if cross {
        taffy::style::Dimension::percent(1.0)
    } else {
        author_width
            .get()
            .unwrap_or(taffy::style::Dimension::auto())
    };
    if !dims_eq(el.style.size.width, want_width) {
        el.style.size.width = want_width;
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if author_block.get().is_none() {
        // Snapshot before collapsing. The first chrome pass is often already
        // cross (hover moved before sync); capturing afterwards would store 0
        // and refuse to restore the row on the way back.
        author_block.set(Some(block_dims(el, axis)));
    }
    let author = author_block.get().unwrap_or((
        taffy::style::Dimension::auto(),
        taffy::style::Dimension::auto(),
        taffy::style::Dimension::auto(),
    ));
    let zero = taffy::style::Dimension::length(0.0);
    let want_block = if cross { (zero, zero, zero) } else { author };
    let have_block = block_dims(el, axis);
    if dims_changed(have_block, want_block) {
        set_block_dims(el, axis, want_block.0, want_block.1, want_block.2);
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if cross {
        el.style.overflow.x = taffy::style::Overflow::Visible;
        el.style.overflow.y = taffy::style::Overflow::Visible;
    }
    // Overlapping free-move tiles (absolute / author z) still need a nested
    // paint band. In-flow sortable rows must not — Gallery ScrollArea already
    // owns `page_stack_depth`, and a band per row reshapes text on scroll.
    let need_stack = want_z != 0
        || el.style.position == taffy::style::Position::Absolute
        || el.style.position_fixed
        || el.children.iter().any(subtree_paints_own_band);
    if sync_stacking_context_attr(el, need_stack) {
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    changed
}

/// Same-list FLIP translate for a sortable row (source placeholder or sibling).
fn live_sortable_translate(
    also_droppable: bool,
    axis: Axis,
    list: Option<&str>,
    index: Option<usize>,
    from_list: Signal<Option<String>>,
    from_index: Signal<Option<usize>>,
    hover_list: Signal<Option<String>>,
    hover_index: Signal<Option<usize>>,
) -> [f32; 2] {
    if !also_droppable {
        return [0.0, 0.0];
    }
    live_pack_item_translate(
        list,
        index,
        from_list.peek().as_deref(),
        from_index.peek(),
        hover_list.peek().as_deref(),
        hover_index.peek(),
        peek_sort_axis(list, axis),
    )
}

/// Dimmed overlay source: sit in the insert hole siblings just opened.
fn live_source_hole_translate(
    axis: Axis,
    list: Option<&str>,
    index: Option<usize>,
    from_list: Signal<Option<String>>,
    from_index: Signal<Option<usize>>,
    hover_list: Signal<Option<String>>,
    hover_index: Signal<Option<usize>>,
    drag_extent: Signal<[f32; 2]>,
) -> [f32; 2] {
    let _ = drag_extent;
    let from_l = from_list.peek();
    let hover_l = hover_list.peek();
    let (Some(list), Some(index), Some(from), Some(hover), Some(fl), Some(hl)) = (
        list,
        index,
        from_index.peek(),
        hover_index.peek(),
        from_l.as_deref(),
        hover_l.as_deref(),
    ) else {
        return [0.0, 0.0];
    };
    if list != fl || list != hl || index != from {
        return [0.0, 0.0];
    }
    live_same_list_source_translate(fl, from, hl, hover, peek_sort_axis(Some(list), axis))
}

fn sortable_flip_entry() -> TransitionEntry {
    TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: SORTABLE_FLIP_MS,
        delay_ms: 0.0,
        timing: TimingFunction::Ease,
    }
}

/// Live gap FLIP: CSS `transition: transform` (includes translate) while the session is active.
/// Pointer-follow, session end, and `animation: false` snap — leftover tween would invert
/// commit layout (`end` clears hover before `on_drag_end` mutates the `Vec`).
fn apply_item_shift_motion(
    el: &mut Element,
    want_translate: [f32; 2],
    want_opacity: f32,
    follow_pointer: bool,
    session_live: bool,
    animation: bool,
    motion: &mut MotionStyle,
) -> bool {
    let snap = follow_pointer || !session_live || !animation;
    if snap {
        let was_animating = motion_is_animating(&el.style, &el.transition_snapshot);
        clear_motion(el);
        motion.reset();
        let want_opacity = want_opacity.clamp(0.0, 1.0);
        let changed =
            el.style.translate != want_translate || (el.style.opacity - want_opacity).abs() > 0.001;
        el.style.translate = want_translate;
        el.style.opacity = want_opacity;
        changed || was_animating
    } else {
        let entries = [sortable_flip_entry()];
        let bootstrap = if el.style.transition.is_empty() {
            Some(MotionBootstrap::EnsureAfterSnap)
        } else {
            Some(MotionBootstrap::EnsureOnly)
        };
        prepare_motion_sync(el, &entries, bootstrap, motion, false);
        let target = MotionTransform {
            translate: want_translate,
            translate_pct: el.style.translate_pct,
            opacity: want_opacity,
            scale: el.style.scale,
        };
        motion.apply(&mut el.style, target, MotionEpsilon::default())
    }
}

fn write_flag_attr(el: &mut Element, key: &str, on: bool) -> bool {
    let have = el.attrs.get(key).map(String::as_str);
    if on {
        if have != Some("1") {
            el.set_attr(key, "1");
            return true;
        }
    } else if have.is_some() {
        el.attrs.remove(key);
        return true;
    }
    false
}

/// Live sortable chrome (opacity / sibling translate) is paint-only.
/// Subscribing `hover_index` / `overlay_pos` inside the island remounts every
/// pointer move; `h=auto` placeholders then collapse and the list height jitters.
fn apply_live_drag_chrome(
    el: &mut Element,
    scope: &str,
    id: &str,
    also_droppable: bool,
    axis: Axis,
    list_sig: Signal<Option<String>>,
    index_sig: Signal<Option<usize>>,
    active: Signal<Option<String>>,
    overlay_open: Signal<bool>,
    from_list: Signal<Option<String>>,
    from_index: Signal<Option<usize>>,
    hover_list: Signal<Option<String>>,
    hover_index: Signal<Option<usize>>,
    drag_extent: Signal<[f32; 2]>,
    author_opacity: f32,
    item_source_opacity: Option<f32>,
    author_z: i32,
    stack: DragStack,
    auto_rank: Signal<i32>,
    auto_bumped: &Cell<bool>,
    animation: bool,
    motion: &mut MotionStyle,
    author_island_block: &Cell<Option<(taffy::style::Dimension, taffy::style::Dimension)>>,
) -> LayoutStatus {
    let _ = drag_extent;
    if el.layout_bounds[2] > 1.0 && el.layout_bounds[3] > 1.0 {
        remember_draggable_bounds(scope, id, el.layout_bounds);
    }
    // Paint-time tree has nested islands filled. Assemble snapshot does not.
    // Only recapture while the island is still at author opacity. `> 0.05`
    // used to keep writing after the kit dimmed to 0.4, so the next ghost
    // fill cloned the faded source.
    if (el.style.opacity - author_opacity).abs() <= 0.02 && !snapshot_has_unresolved_island(el) {
        remember_source_snapshot(scope, id, el);
    }
    let list = list_sig.peek();
    let index = index_sig.peek();
    let is_active = active.peek().as_deref() == Some(id);
    let overlay_on = overlay_is_registered(scope) && overlay_open.peek();
    let from_l = from_list.peek();
    let hover_l = hover_list.peek();
    let lists_cross = from_l.is_some() && hover_l.is_some() && from_l != hover_l;
    let flags = super::registry::item_flags(scope, id);
    let clone_drag = is_active && super::session::active_pull_clone();
    let mut want_opacity = author_opacity;
    let mut want_translate = [0.0_f32, 0.0];
    let mut follow_pointer = false;
    if is_active {
        if flags.skip_follow {
            want_opacity = author_opacity;
            want_translate = [0.0, 0.0];
            follow_pointer = false;
        } else if clone_drag {
            want_opacity = author_opacity;
            want_translate = [0.0, 0.0];
            follow_pointer = false;
        } else if overlay_on {
            let src_op = item_source_opacity.unwrap_or_else(|| overlay_source_opacity(scope));
            let hide = src_op <= 0.001;
            let cross = lists_cross;
            let swap = active_sort_mode() == SortMode::Swap;
            if swap {
                want_opacity = src_op;
                want_translate = [0.0, 0.0];
                follow_pointer = false;
            } else if hide {
                want_opacity = src_op;
                follow_pointer = true;
            } else if cross {
                // Body is painted by the target list's hole preview. Translating
                // this island across containers cannot follow a moving origin.
                want_opacity = 0.0;
                want_translate = [0.0, 0.0];
                follow_pointer = true;
            } else {
                want_opacity = src_op;
                want_translate = live_source_hole_translate(
                    axis,
                    list.as_deref(),
                    index,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                    drag_extent,
                );
                // Hole must track hover instantly; FLIP tween leaves a visible gap.
                follow_pointer = true;
            }
        } else if let Some(t) = active_source_translate() {
            want_translate = t;
            follow_pointer = true;
        }
        if stack == DragStack::Auto && !auto_bumped.get() {
            auto_rank.set(bump_auto_stack(scope));
            auto_bumped.set(true);
        }
    } else {
        auto_bumped.set(false);
        let in_group = active_drag_ids().iter().any(|x| x == id);
        if active_sort_mode() == SortMode::Swap {
            want_translate = [0.0, 0.0];
        } else if in_group {
            let src_op = item_source_opacity.unwrap_or_else(|| overlay_source_opacity(scope));
            want_opacity = src_op;
            want_translate = [0.0, 0.0];
        } else if also_droppable {
            if flags.skip_shift || super::session::active_pull_clone() {
                want_translate = [0.0, 0.0];
            } else {
                want_translate = live_sortable_translate(
                    also_droppable,
                    axis,
                    list.as_deref(),
                    index,
                    from_list,
                    from_index,
                    hover_list,
                    hover_index,
                );
            }
            // Source shell left the pack (`position: absolute`). Remaining
            // origin rows already occupy the hole. A leftover same-list
            // `-pitch` FLIP would start at the folder's inner top and ease
            // down (Gallery: Alpha→Later 后 Beta 从 Inbox 顶部下滑).
            if lists_cross && list == from_l {
                follow_pointer = true;
            }
        }
    }
    let mut want_z = stack.resolve(author_z, auto_rank.peek());
    if stack == DragStack::Auto && is_active {
        want_z = want_z.max(1);
    }
    let session_live = active.peek().is_some();
    let mut changed = LayoutStatus::Clean;
    if let Some(i) = index {
        let s = i.to_string();
        if el.attrs.get(ATTR_DRAG_INDEX).map(String::as_str) != Some(s.as_str()) {
            el.set_attr(ATTR_DRAG_INDEX, s);
        }
    }
    if let Some(ref list) = list {
        if el.attrs.get(ATTR_DRAG_LIST).map(String::as_str) != Some(list.as_str()) {
            el.set_attr(ATTR_DRAG_LIST, list);
        }
    }
    let swap_peer = active_sort_mode() == SortMode::Swap
        && session_live
        && !is_active
        && hover_index.peek() == index
        && hover_list.peek() == list;
    if write_flag_attr(el, ATTR_DRAG_SWAP, swap_peer) {
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    if write_flag_attr(el, ATTR_DRAG_SELECTED, scope_selected_contains(scope, id)) {
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    if (el.style.opacity - want_opacity).abs() > 0.001 {
        el.style.opacity = want_opacity.clamp(0.0, 1.0);
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    if el.style.z_index != want_z {
        el.style.z_index = want_z;
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    let shift_changed = apply_item_shift_motion(
        el,
        want_translate,
        want_opacity,
        follow_pointer,
        session_live,
        animation,
        motion,
    );
    let from_l = from_list.peek();
    let hover_l = hover_list.peek();
    let cross = is_active && from_l.is_some() && hover_l.is_some() && from_l != hover_l;
    let author_sz = match author_island_block.get() {
        Some(v) => v,
        None => {
            let v = match axis {
                Axis::X => (el.style.size.width, el.style.min_size.width),
                Axis::Y | Axis::Xy | Axis::Start => {
                    (el.style.size.height, el.style.min_size.height)
                }
            };
            author_island_block.set(Some(v));
            v
        }
    };
    let want_sz = author_sz;
    let have_sz = match axis {
        Axis::X => (el.style.size.width, el.style.min_size.width),
        Axis::Y | Axis::Xy | Axis::Start => (el.style.size.height, el.style.min_size.height),
    };
    if !dims_eq(have_sz.0, want_sz.0) || !dims_eq(have_sz.1, want_sz.1) {
        match axis {
            Axis::X => {
                el.style.size.width = want_sz.0;
                el.style.min_size.width = want_sz.1;
            }
            Axis::Y | Axis::Xy | Axis::Start => {
                el.style.size.height = want_sz.0;
                el.style.min_size.height = want_sz.1;
            }
        }
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if cross {
        el.style.overflow.x = taffy::style::Overflow::Visible;
        el.style.overflow.y = taffy::style::Overflow::Visible;
    }
    motion_sync_status(
        el,
        changed != LayoutStatus::Clean || shift_changed,
        follow_pointer,
    )
}

fn strip_preview_session_attrs(el: &mut Element) {
    for key in [
        ATTR_DRAG_DRAGGABLE,
        ATTR_DRAG_DROPPABLE,
        ATTR_DRAG_HANDLE,
        ATTR_DRAG_IGNORE,
        ATTR_DRAG_SCOPE,
        ATTR_DRAG_TYPE,
        ATTR_DRAG_LIST,
        ATTR_DRAG_INDEX,
        ATTR_DRAG_GROUP,
        ATTR_DRAG_DISABLED,
        ATTR_DRAG_OVER,
        ATTR_DRAG_ITEM_SHELL,
        ATTR_DRAG_HOLE_PREVIEW,
    ] {
        el.attrs.remove(key);
    }
    el.on_mousedown = None;
    el.on_mouseup = None;
    el.on_mousemove = None;
    el.on_click = None;
    for child in &mut el.children {
        strip_preview_session_attrs(child);
    }
}

/// In-flow chip width under the target list (skip the abspos hole clone).
/// Hits can lag a frame; the live tree is what Taffy just painted.
fn inflow_indexed_width(list_el: &Element, list: &str) -> Option<f32> {
    fn walk(el: &Element, list: &str, out: &mut Option<f32>) {
        if el.attrs.contains_key(ATTR_DRAG_HOLE_PREVIEW) {
            return;
        }
        let listed = el.attrs.get(ATTR_DRAG_LIST).map(String::as_str) == Some(list);
        if listed
            && el.attrs.contains_key(ATTR_DRAG_INDEX)
            && el.attrs.contains_key(ATTR_DRAG_DRAGGABLE)
            && el.layout_bounds[2] > 1.0
        {
            let w = el.layout_bounds[2];
            *out = Some(out.map_or(w, |prev| prev.min(w)));
            return;
        }
        for c in &el.children {
            walk(c, list, out);
        }
    }
    let mut out = None;
    walk(list_el, list, &mut out);
    out
}

fn slot_has_percent(el: &Element, slot: UnitSlot) -> bool {
    el.style
        .unit_specs
        .iter()
        .any(|(s, d)| *s == slot && d.has_percent())
}

/// Target row width for a `w=100%` clone.
///
/// Prefer an in-flow chip under the list (Gallery "Write tests"), then a hit
/// rect, then `drag_extent`, then the list content box. Absolute `width: 100%`
/// resolves against the padding box — that is a sliver wider than the chip.
fn hole_preview_slot_extent(list_el: &Element, list: &str, drag_extent: [f32; 2]) -> [f32; 2] {
    let sibling_w = inflow_indexed_width(list_el, list).or_else(|| {
        last_hits()
            .iter()
            .find(|h| {
                h.list_id.as_deref() == Some(list)
                    && h.index.is_some()
                    && h.rect.is_usable()
                    && h.rect.width > 1.0
            })
            .map(|h| h.rect.width)
    });
    let [_, border_r, _, border_l] = list_el.style.resolved_border_widths();
    let pad_l = crate::layout::lp_length_px(list_el.style.padding.left).unwrap_or(0.0);
    let pad_r = crate::layout::lp_length_px(list_el.style.padding.right).unwrap_or(0.0);
    let content_w = (list_el.layout_bounds[2] - pad_l - pad_r - border_l - border_r).max(0.0);
    let mut w = sibling_w
        .or_else(|| (drag_extent[0] > 1.0).then_some(drag_extent[0]))
        .filter(|x| *x > 1.0)
        .unwrap_or(content_w);
    if content_w > 1.0 {
        w = w.min(content_w);
    }
    [w.max(0.0), drag_extent[1].max(0.0)]
}

/// Pin through [`Style::set_css_dim`] so `unit_specs` drops `width: 100%`.
/// Writing `size.width` alone is overwritten on the next Taffy bake.
fn pin_percent_size_to_slot(el: &mut Element, slot: [f32; 2]) -> bool {
    let mut changed = false;
    let hole = el.attrs.contains_key(ATTR_DRAG_HOLE_PREVIEW);
    let spec_w_pct =
        slot_has_percent(el, UnitSlot::Width) || slot_has_percent(el, UnitSlot::InlineSize);
    let pin_w = slot[0] > 0.0
        && (spec_w_pct
            || matches!(dim_kind(el.style.size.width), DimKind::Percent(_))
            || (hole && matches!(dim_kind(el.style.size.width), DimKind::Length(_))));
    if pin_w {
        let want = taffy::style::Dimension::length(slot[0]);
        if spec_w_pct || !dims_eq(el.style.size.width, want) {
            el.style.set_css_dim(UnitSlot::Width, CssDim::Px(slot[0]));
            changed = true;
        }
    }
    let spec_h_pct =
        slot_has_percent(el, UnitSlot::Height) || slot_has_percent(el, UnitSlot::BlockSize);
    if slot[1] > 0.0
        && (spec_h_pct || matches!(dim_kind(el.style.size.height), DimKind::Percent(_)))
    {
        let want = taffy::style::Dimension::length(slot[1]);
        if spec_h_pct || !dims_eq(el.style.size.height, want) {
            el.style.set_css_dim(UnitSlot::Height, CssDim::Px(slot[1]));
            changed = true;
        }
    }
    changed
}

fn preview_visual_bounds(el: &Element) -> [f32; 4] {
    let b = el.layout_bounds;
    [
        b[0] + el.style.translate[0],
        b[1] + el.style.translate[1],
        b[2],
        b[3],
    ]
}

/// Paint-only origin: inset is layout (full-page Taffy + FullClear every
/// `hover_index`). Translate is the same path as sibling FLIP.
fn sync_preview_paint_origin(preview: &mut Element, local: [f32; 2]) -> LayoutStatus {
    let mut changed = LayoutStatus::Clean;
    let have_l = crate::layout::lpa_length_px(preview.style.inset.left).unwrap_or(0.0);
    let have_t = crate::layout::lpa_length_px(preview.style.inset.top).unwrap_or(0.0);
    if have_l.abs() > 0.5 || have_t.abs() > 0.5 {
        preview.style.inset.left = taffy::style::LengthPercentageAuto::length(0.0);
        preview.style.inset.top = taffy::style::LengthPercentageAuto::length(0.0);
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if (preview.style.translate[0] - local[0]).abs() > 0.5
        || (preview.style.translate[1] - local[1]).abs() > 0.5
    {
        crate::app::damage::push_frame_paint_damage(preview_visual_bounds(preview));
        preview.style.translate = local;
        crate::app::damage::push_frame_paint_damage(preview_visual_bounds(preview));
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    changed
}

/// Empty-list insert hole in **`list_el`'s layout space** (page or island-local).
/// Just below the first in-flow chrome (Gallery kanban title). Skip a trailing
/// 0×0 / unstamped child — that origin is `(0,0)`. Skip a child that fills the
/// column (`min_h`) so the hole does not sit past the bottom.
fn empty_list_pack_origin(list_el: &Element) -> Option<[f32; 2]> {
    let lb = list_el.layout_bounds;
    if lb[2] <= 1.0 && lb[3] <= 1.0 {
        return None;
    }
    let in_shell = |b: [f32; 4]| {
        let cx = b[0] + b[2] * 0.5;
        let cy = b[1] + b[3] * 0.5;
        cx >= lb[0] - 1.0
            && cx <= lb[0] + lb[2] + 1.0
            && cy >= lb[1] - 1.0
            && cy <= lb[1] + lb[3] + 1.0
    };
    let gap = crate::layout::lp_length_px(list_el.style.gap.height).unwrap_or(0.0);
    let mut below_chrome = None;
    for c in &list_el.children {
        if c.attrs.contains_key(ATTR_DRAG_HOLE_PREVIEW) {
            continue;
        }
        if c.style.position == taffy::style::Position::Absolute || c.style.position_fixed {
            continue;
        }
        let b = c.layout_bounds;
        if b[3] > 1.0 && b[3] < lb[3] - 2.0 && in_shell(b) {
            below_chrome = Some([b[0], b[1] + b[3] + gap]);
            continue;
        }
        if below_chrome.is_some() && in_shell(b) {
            return Some([b[0], b[1]]);
        }
    }
    below_chrome
}

/// Abspos hole translate is relative to the droppable CB (`el`), not the hit
/// table. Island inner `layout_bounds` are often 0-based while `LAST_HITS` are
/// page — subtracting the hit origin sends the dimmed body off-screen.
fn empty_list_hole_local(list_el: &Element) -> Option<[f32; 2]> {
    let pack = empty_list_pack_origin(list_el)?;
    Some([
        pack[0] - list_el.layout_bounds[0],
        pack[1] - list_el.layout_bounds[1],
    ])
}

/// Cross-list dimmed body: a paint-only clone parented by the **target** list.
/// Translate (not inset) tracks the hole so origin-column vacate does not
/// LayoutChanged every pointer move. `w=100%` is pinned to the target slot —
/// the overlay ghost already does this via `pin_percent_size_to_extent`.
fn sync_cross_list_hole_preview(
    el: &mut Element,
    scope: &str,
    list: &str,
    from_list: Option<&str>,
    hover_list: Option<&str>,
    hover_index: Option<usize>,
    axis: Axis,
    chip: f32,
    overlay_on: bool,
    drag_extent: [f32; 2],
) -> LayoutStatus {
    let show = overlay_on
        && from_list.is_some()
        && hover_list == Some(list)
        && from_list != hover_list
        && active_sort_mode() != SortMode::Swap
        && !active_pull_clone();
    let existing = el
        .children
        .iter()
        .position(|c| c.attrs.contains_key(ATTR_DRAG_HOLE_PREVIEW));
    let src_id = active_draggable_id();
    let src_op = src_id
        .as_deref()
        .map(|id| effective_source_opacity(scope, id))
        .unwrap_or(0.0);
    if !show || src_op <= 0.001 || src_id.is_none() {
        if let Some(i) = existing {
            el.children.remove(i);
            return LayoutStatus::StructureChanged;
        }
        return LayoutStatus::Clean;
    }
    let src_id = src_id.unwrap();
    let hits = last_hits();
    let axis = peek_sort_axis(Some(list), axis);
    let Some(hole) = insert_hole_origin(&hits, list, hover_index, axis, chip.max(1.0)) else {
        return LayoutStatus::Clean;
    };
    let has_items = hits
        .iter()
        .any(|h| h.list_id.as_deref() == Some(list) && h.index.is_some());
    let list_origin = hits
        .iter()
        .find(|h| h.list_id.as_deref() == Some(list) && h.index.is_none())
        .map(|h| [h.rect.x, h.rect.y])
        .unwrap_or([el.layout_bounds[0], el.layout_bounds[1]]);
    let local = if !has_items {
        empty_list_hole_local(el).unwrap_or([hole[0] - list_origin[0], hole[1] - list_origin[1]])
    } else {
        [hole[0] - list_origin[0], hole[1] - list_origin[1]]
    };
    let slot = hole_preview_slot_extent(el, list, drag_extent);
    if let Some(i) = existing {
        let preview = &mut el.children[i];
        let mut changed = sync_preview_paint_origin(preview, local);
        if (preview.style.opacity - src_op).abs() > 0.001 {
            preview.style.opacity = src_op;
            changed = changed.merge(LayoutStatus::StyleChanged);
        }
        if pin_percent_size_to_slot(preview, slot) {
            changed = changed.merge(LayoutStatus::LayoutChanged);
        }
        return changed;
    }
    let Some(mut preview) = clone_source_preview(scope, &src_id) else {
        return LayoutStatus::Clean;
    };
    strip_preview_session_attrs(&mut preview);
    preview.set_attr(ATTR_DRAG_HOLE_PREVIEW, "1");
    preview.set_attr(ATTR_POINTER_EVENTS, "none");
    preview.style.position = taffy::style::Position::Absolute;
    preview.style.align_self = Some(taffy::style::AlignSelf::FLEX_START);
    preview.style.inset.left = taffy::style::LengthPercentageAuto::length(0.0);
    preview.style.inset.top = taffy::style::LengthPercentageAuto::length(0.0);
    preview.style.translate = local;
    preview.style.opacity = src_op;
    let _ = pin_percent_size_to_slot(&mut preview, slot);
    el.children.push(preview);
    LayoutStatus::StructureChanged
}

/// Paint-only droppable chrome: `rui_drag_over` (solid-dnd `isActiveDroppable`)
/// on the **hover target**, not the origin list. Cross-list hover parents a
/// hole preview here so the dimmed body is a target descendant. Colors stay
/// with the caller.
fn apply_live_droppable_chrome(
    el: &mut Element,
    scope: &str,
    id: &str,
    list: &str,
    active_droppable: Signal<Option<String>>,
    from_list: Signal<Option<String>>,
    hover_list: Signal<Option<String>>,
    hover_index: Signal<Option<usize>>,
    overlay_open: Signal<bool>,
    drag_extent: Signal<[f32; 2]>,
    axis: Axis,
    author_pad: &Cell<Option<f32>>,
    author_margin: &Cell<Option<f32>>,
    author_pos: &Cell<Option<taffy::style::Position>>,
    author_z: i32,
) -> LayoutStatus {
    let axis = peek_sort_axis(Some(list), axis);
    let from = from_list.peek();
    let hover = hover_list.peek();
    let over = active_droppable.peek().as_deref() == Some(id) || hover.as_deref() == Some(list);
    let is_origin = from.as_deref() == Some(list);
    let is_origin_cross = is_origin && hover.is_some() && hover != from;
    let is_target_cross = hover.as_deref() == Some(list) && from.is_some() && from != hover;
    let mut changed = LayoutStatus::Clean;
    let want_over = if over { Some("1") } else { None };
    let have_over = el.attrs.get(ATTR_DRAG_OVER).map(String::as_str);
    if have_over != want_over {
        if let Some(v) = want_over {
            el.set_attr(ATTR_DRAG_OVER, v);
        } else {
            el.attrs.remove(ATTR_DRAG_OVER);
        }
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    let want_z = author_z;
    if el.style.z_index != want_z {
        el.style.z_index = want_z;
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
    let base_pad = match author_pad.get() {
        Some(v) => v,
        None => {
            let edge = match axis {
                Axis::X => el.style.padding.right,
                Axis::Y | Axis::Xy | Axis::Start => el.style.padding.bottom,
            };
            let v = crate::layout::lp_length_px(edge).unwrap_or(0.0);
            author_pad.set(Some(v));
            v
        }
    };
    let chip = {
        let e = drag_extent.peek();
        match axis {
            Axis::X => e[0],
            Axis::Y | Axis::Xy | Axis::Start => e[1],
        }
        .max(0.0)
    };
    // Occupied target: one packed row. Empty lists still open `chip` only.
    // Mid-list insert FLIPs siblings (paint-only); trailing pad grows the
    // list so the last row still fits after the shift.
    let extent = pack_insert_extent(Some(list), axis, chip);
    let extra_pad = if hover.as_deref() == Some(list) && from.as_deref().is_some() && from != hover
    {
        extent
    } else {
        0.0
    };
    let want_pad = base_pad + extra_pad;
    let pad_edge = match axis {
        Axis::X => &mut el.style.padding.right,
        Axis::Y | Axis::Xy | Axis::Start => &mut el.style.padding.bottom,
    };
    let have_pad = crate::layout::lp_length_px(*pad_edge).unwrap_or(0.0);
    if (have_pad - want_pad).abs() > 0.5 {
        *pad_edge = taffy::style::LengthPercentage::length(want_pad);
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    // Keep the author's trailing margin on the *list*. Vacating a row is the
    // source shell leaving the pack (`position: absolute` vs this list CB).
    // A list-level height cap clips the last item; a guessed full-row
    // negative margin ate remaining files / folder padding.
    let base_margin = match author_margin.get() {
        Some(v) => v,
        None => {
            let edge = match axis {
                Axis::X => el.style.margin.right,
                Axis::Y | Axis::Xy | Axis::Start => el.style.margin.bottom,
            };
            let v = crate::layout::lpa_length_px(edge).unwrap_or(0.0);
            author_margin.set(Some(v));
            v
        }
    };
    let margin_edge = match axis {
        Axis::X => &mut el.style.margin.right,
        Axis::Y | Axis::Xy | Axis::Start => &mut el.style.margin.bottom,
    };
    let have_margin = crate::layout::lpa_length_px(*margin_edge).unwrap_or(0.0);
    if (have_margin - base_margin).abs() > 0.5 {
        *margin_edge = taffy::style::LengthPercentageAuto::length(base_margin);
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    // Origin list is the containing block for the vacated absolute shell so
    // `w=100%` resolves against the file column, not a z-index folder card.
    let base_pos = match author_pos.get() {
        Some(v) => v,
        None => {
            let v = el.style.position;
            author_pos.set(Some(v));
            v
        }
    };
    let want_pos = if is_origin_cross || is_target_cross {
        taffy::style::Position::Relative
    } else {
        base_pos
    };
    if el.style.position != want_pos {
        el.style.position = want_pos;
        changed = changed.merge(LayoutStatus::LayoutChanged);
    }
    if is_origin_cross || is_target_cross {
        el.style.overflow.x = taffy::style::Overflow::Visible;
        el.style.overflow.y = taffy::style::Overflow::Visible;
    }
    let overlay_on = overlay_is_registered(scope) && overlay_open.peek();
    changed = changed.merge(sync_cross_list_hole_preview(
        el,
        scope,
        list,
        from.as_deref(),
        hover.as_deref(),
        hover_index.peek(),
        axis,
        chip,
        overlay_on,
        drag_extent.peek(),
    ));
    changed
}

fn mark_scope(el: &mut Element, scope: &str) {
    el.set_attr(ATTR_DRAG_SCOPE, scope);
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct DragScopeBuilder {
    id: String,
    collision: CollisionAlg,
    axis: Axis,
    start_axis: Axis,
    threshold: f32,
    restrict: Restrict,
    restrict_end_only: bool,
    restrict_anchor: RestrictAnchor,
    restrict_overflow: RestrictOverflow,
    snap: Option<Snap>,
    delay_ms: f32,
    empty_insert: f32,
    autoscroll: AutoScroll,
    can_drop: Option<DragPredicate>,
    collision_fn: Option<CollisionFn>,
    activation: Activation,
    on_start: Option<DragHandler>,
    on_move: Option<DragHandler>,
    on_over: Option<DragHandler>,
    on_end: Option<DragHandler>,
    /// Sibling / source-hole FLIP. Default `true` (`SORTABLE_FLIP_MS`). `false` snaps.
    animation: bool,
    /// Insert-and-shift (default) vs live Swap (no FLIP, highlight peer).
    sort_mode: SortMode,
    /// Click-select + batch drag. Overlay stacks frozen ids.
    multi_drag: bool,
    modifiers: Vec<super::modifier::DragModifier>,
    sensors: Vec<super::sensor::DragSensor>,
    /// Paint collision slots (solid-dnd `DragDropDebugger`). Sticky process-wide flag.
    debug_hits: bool,
    /// interact inertia (free-move only; Sortable commits immediately).
    inertia: InertiaOpts,
}

pub fn drag_scope(id: impl Into<String>) -> DragScopeBuilder {
    DragScopeBuilder {
        id: id.into(),
        collision: CollisionAlg::MostIntersecting,
        axis: Axis::Xy,
        start_axis: Axis::Xy,
        threshold: 4.0,
        restrict: Restrict::None,
        restrict_end_only: false,
        restrict_anchor: RestrictAnchor::FULL,
        restrict_overflow: RestrictOverflow::Visible,
        snap: None,
        delay_ms: 0.0,
        empty_insert: 8.0,
        autoscroll: AutoScroll::default(),
        can_drop: None,
        collision_fn: None,
        activation: Activation::And,
        on_start: None,
        on_move: None,
        on_over: None,
        on_end: None,
        animation: true,
        sort_mode: SortMode::Insert,
        multi_drag: false,
        modifiers: Vec::new(),
        sensors: Vec::new(),
        debug_hits: false,
        inertia: InertiaOpts::default(),
    }
}

impl DragScopeBuilder {
    pub fn collision(mut self, collision: CollisionAlg) -> Self {
        self.collision = collision;
        self
    }
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    /// interact.js `startAxis`. `Xy` (default) accepts any first movement;
    /// `X` / `Y` stay pending until the pointer's dominant axis matches.
    pub fn start_axis(mut self, axis: Axis) -> Self {
        self.start_axis = axis;
        self
    }
    pub fn threshold(mut self, px: f32) -> Self {
        self.threshold = px.max(0.0);
        self
    }
    /// Ghost travel box for every drag in this scope. `none` (default) does not
    /// clamp; `viewport` uses the window; `parent` uses the item's author
    /// container; any other token is a live element id. A
    /// [`DraggableBuilder::restrict`] on the item overrides this.
    pub fn restrict(mut self, restrict: impl Into<Restrict>) -> Self {
        self.restrict = restrict.into();
        self
    }
    /// Apply restrict only on drop (interact `endOnly`). Drag-time travel is free.
    pub fn restrict_end_only(mut self, yes: bool) -> Self {
        self.restrict_end_only = yes;
        self
    }
    /// Which fraction of the ghost must stay in the restrict box (interact
    /// `elementRect`). [`RestrictAnchor::CENTER`] only pins the midpoint.
    pub fn restrict_anchor(mut self, anchor: RestrictAnchor) -> Self {
        self.restrict_anchor = anchor;
        self
    }
    /// Ghost paint vs the restrict box: `visible` (default, 透出) or `hidden` (裁切).
    /// Position is still clamped; this only clips overlay ink that extends past the box.
    pub fn restrict_overflow(mut self, overflow: impl Into<RestrictOverflow>) -> Self {
        self.restrict_overflow = overflow.into();
        self
    }
    pub fn snap(mut self, snap: impl Into<Snap>) -> Self {
        self.snap = Some(snap.into());
        self
    }
    /// interact.js `snap.targets` — multiple grids; closest in-range snap wins.
    pub fn snap_targets(mut self, targets: SnapTargets) -> Self {
        self.snap = Some(Snap::targets(targets));
        self
    }
    /// interact.js `snap.targets` function entry — return a snap point or `None`.
    pub fn snap_fn(mut self, f: SnapPointFn) -> Self {
        let targets = match self.snap {
            Some(Snap::Targets(t)) => t.with_fn(f),
            Some(Snap::Grid(g)) => SnapTargets::new(vec![g]).with_fn(f),
            None => SnapTargets::default().with_fn(f),
        };
        self.snap = Some(Snap::targets(targets));
        self
    }
    /// Hold delay in milliseconds before the drag activates (Sortable `delay`).
    pub fn delay_ms(mut self, ms: f32) -> Self {
        self.delay_ms = ms.max(0.0);
        self
    }
    /// Distance **and** delay (default) vs distance **or** delay (solid-dnd pointer sensor).
    pub fn activation(mut self, activation: Activation) -> Self {
        self.activation = activation;
        self
    }
    /// Author collision detector. When set, it wins over [`Self::collision`]; innermost
    /// nested refine still runs unless the callback sets `skip_innermost`.
    pub fn collision_fn(
        mut self,
        f: impl for<'a> Fn(&CollisionInput<'a>) -> Option<String> + 'static,
    ) -> Self {
        self.collision_fn = Some(Rc::new(f));
        self
    }
    /// Inflate empty-container hit boxes (Sortable `emptyInsertThreshold`).
    pub fn empty_insert(mut self, px: f32) -> Self {
        self.empty_insert = px.max(0.0);
        self
    }
    pub fn autoscroll(mut self, enabled: bool) -> Self {
        self.autoscroll.enabled = enabled;
        self
    }
    /// Full edge-scroll config (Sortable `scrollSensitivity` / `scrollSpeed` + lock).
    pub fn autoscroll_opts(mut self, opts: AutoScroll) -> Self {
        self.autoscroll = opts;
        self
    }
    /// Edge band width in px (Sortable `scrollSensitivity`, default 40).
    pub fn scroll_sensitivity(mut self, px: f32) -> Self {
        self.autoscroll.sensitivity = px.max(0.0);
        self
    }
    /// Scroll step per display frame (default ~14px @ 60Hz).
    pub fn scroll_speed(mut self, px: f32) -> Self {
        self.autoscroll.speed = px.max(0.0);
        self
    }
    /// Edge auto-scroll axis freeze for **all** drags in this scope.
    /// Independent of [`Self::axis`]. Default [`super::ScrollLock::Auto`].
    /// `"all"` / `"x"` / `"y"` / `"none"` / `"auto"`.
    pub fn scroll_lock(mut self, lock: impl Into<ScrollLock>) -> Self {
        self.autoscroll.lock = lock.into();
        self
    }
    /// Live sibling / source-hole translate easing (SortableJS `animation`).
    /// `true` (default) uses [`SORTABLE_FLIP_MS`]; `false` jumps instantly.
    pub fn animation(mut self, yes: bool) -> Self {
        self.animation = yes;
        self
    }
    /// `insert` (default, sibling FLIP) or `swap` (Sortable Swap: no shift, highlight peer).
    pub fn sort_mode(mut self, mode: SortMode) -> Self {
        self.sort_mode = mode;
        self
    }
    /// Click toggles selection; dragging a selected item freezes the set.
    pub fn multi_drag(mut self, yes: bool) -> Self {
        self.multi_drag = yes;
        self
    }
    /// Draw layout-slot rectangles (dashed idle, bold active droppable).
    /// Sets the process-wide [`super::set_debug_drag_hits`] flag when `true`.
    pub fn debug_hits(mut self, yes: bool) -> Self {
        self.debug_hits = yes;
        self
    }
    /// interact.js inertia. Free-move tiles fling after pointerup; Sortable lists
    /// still commit immediately so sibling FLIP is not fighting a coast.
    pub fn inertia(mut self, yes: bool) -> Self {
        self.inertia.enabled = yes;
        self
    }
    pub fn inertia_opts(mut self, opts: InertiaOpts) -> Self {
        self.inertia = opts;
        self
    }
    /// Extra press-delta stage (solid-dnd `addTransformer` / dnd-kit `Modifier`).
    /// Does not write CSS `transform`. Built-ins occupy orders 10 (axis), 20
    /// (snap), 25 (pin-locked-axis), 30 (restrict); author stages typically
    /// use order 40.
    pub fn modifier(
        mut self,
        id: impl Into<String>,
        order: i32,
        f: impl Fn(
                super::modifier::DragDelta,
                &super::modifier::DragModifierCtx,
            ) -> super::modifier::DragDelta
            + 'static,
    ) -> Self {
        let id = id.into();
        self.modifiers.retain(|t| t.id != id);
        self.modifiers
            .push(super::modifier::DragModifier::new(id, order, f));
        self
    }
    /// Custom sensor activator (solid-dnd `addSensor`). Pointer remains default.
    pub fn sensor(
        mut self,
        id: impl Into<String>,
        activator: impl Fn(&super::sensor::DragSensorCtx) -> bool + 'static,
    ) -> Self {
        let id = id.into();
        self.sensors.retain(|s| s.id != id);
        self.sensors
            .push(super::sensor::DragSensor::new(id, activator));
        self
    }
    /// Return `false` to reject a droppable this frame (Sortable `onMove`).
    pub fn can_drop(mut self, f: impl Fn(&super::session::DragEvent) -> bool + 'static) -> Self {
        self.can_drop = Some(Rc::new(f));
        self
    }
    pub fn on_drag_start(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.on_start = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drag_move(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.on_move = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drag_over(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.on_over = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drag_end(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.on_end = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        if self.debug_hits {
            super::set_debug_drag_hits(true);
        }
        let id = resolve_instance_id(self.id, "drag");
        let overlay_open = use_signal(format!("{id}__drag_open").as_str(), || false);
        let overlay_pos = use_signal(format!("{id}__drag_pos").as_str(), || [0.0_f32, 0.0]);
        let active_draggable = use_signal(format!("{id}__drag_source").as_str(), || None::<String>);
        let active_droppable = use_signal(format!("{id}__drag_drop").as_str(), || None::<String>);
        let from_list = use_signal(format!("{id}__drag_from_list").as_str(), || None::<String>);
        let from_index = use_signal(format!("{id}__drag_from").as_str(), || None::<usize>);
        let hover_list = use_signal(format!("{id}__drag_hover_list").as_str(), || None::<String>);
        let hover_index = use_signal(format!("{id}__drag_hover_index").as_str(), || None::<usize>);
        let drag_extent = use_signal(format!("{id}__drag_extent").as_str(), || [0.0_f32, 0.0]);
        let selected = use_signal(format!("{id}__drag_selected").as_str(), Vec::<String>::new);
        register_scope_product(id.clone(), self.sort_mode, self.multi_drag, selected);
        super::session::register_scope(
            id.clone(),
            ScopeRuntime {
                collision: self.collision,
                axis: self.axis,
                start_axis: self.start_axis,
                threshold: self.threshold,
                restrict: self.restrict,
                restrict_end_only: self.restrict_end_only,
                restrict_anchor: self.restrict_anchor,
                restrict_overflow: self.restrict_overflow,
                snap: self.snap,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: self.autoscroll,
                empty_insert: self.empty_insert,
                delay_ms: self.delay_ms,
                activation: self.activation,
                collision_fn: self.collision_fn,
                overlay_align: OverlayAlign::Grab,
                can_drop: self.can_drop,
                on_start: self.on_start,
                on_move: self.on_move,
                on_over: self.on_over,
                on_end: self.on_end,
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
                modifiers: self.modifiers,
                sensors: self.sensors,
            },
        );
        super::session::set_scope_inertia(&id, self.inertia);
        let ctx = DragCtx {
            id: id.clone(),
            overlay_open,
            overlay_pos,
            active_draggable,
            active_droppable,
            from_list,
            from_index,
            hover_list,
            hover_index,
            drag_extent,
            axis: self.axis,
            animation: self.animation,
            sort_mode: self.sort_mode,
            multi_drag: self.multi_drag,
            selected,
        };
        let mut root = with_child_owner(|| {
            provide_context(drag_context(), ctx);
            body()
        });
        if root.template_id.is_none() {
            root.template_id = Some(format!("{id}__drag_scope"));
        }
        mark_scope(&mut root, &id);
        // Gallery (and any page already in a page_stack) only splits rect/text
        // bands when the parent opts in. Overlapping draggables then draw
        // shell+glyphs together instead of all glyphs after all shells.
        root.set_attr("data-stacking-context", "1");
        root
    }
}

/// RSX `<DragProvider>` — same `child` path as the Builder.
pub fn drag_scope_from_rsx(opts: DragScopeRsxOpts, body: impl FnOnce() -> Element) -> Element {
    let mut b = drag_scope(opts.id)
        .collision(opts.collision)
        .axis(opts.axis)
        .animation(opts.animation);
    b.threshold = opts.threshold;
    b.restrict = opts.restrict;
    b.restrict_end_only = opts.restrict_end_only;
    b.restrict_anchor = opts.restrict_anchor;
    b.restrict_overflow = opts.restrict_overflow;
    if let Some(snap) = opts.snap {
        b = b.snap(snap);
    }
    if let Some(targets) = opts.snap_targets {
        b = b.snap_targets(targets);
    }
    if let Some(f) = opts.snap_fn {
        b = b.snap_fn(f);
    }
    if let Some(h) = opts.on_drag_end {
        b.on_end = Some(h);
    }
    if let Some(h) = opts.on_drag_start {
        b.on_start = Some(h);
    }
    if let Some(h) = opts.on_drag_over {
        b.on_over = Some(h);
    }
    b.child(body)
}

pub struct DragScopeRsxOpts {
    pub id: String,
    pub collision: CollisionAlg,
    pub axis: Axis,
    pub threshold: f32,
    pub restrict: Restrict,
    pub restrict_end_only: bool,
    pub restrict_anchor: RestrictAnchor,
    pub restrict_overflow: RestrictOverflow,
    pub snap: Option<Snap>,
    pub snap_targets: Option<SnapTargets>,
    pub snap_fn: Option<SnapPointFn>,
    pub animation: bool,
    pub on_drag_start: Option<DragHandler>,
    pub on_drag_over: Option<DragHandler>,
    pub on_drag_end: Option<DragHandler>,
}

impl Default for DragScopeRsxOpts {
    fn default() -> Self {
        Self {
            id: String::new(),
            collision: CollisionAlg::default(),
            axis: Axis::default(),
            threshold: 4.0,
            restrict: Restrict::default(),
            restrict_end_only: false,
            restrict_anchor: RestrictAnchor::FULL,
            restrict_overflow: RestrictOverflow::Visible,
            snap: None,
            snap_targets: None,
            snap_fn: None,
            animation: true,
            on_drag_start: None,
            on_drag_over: None,
            on_drag_end: None,
        }
    }
}

impl Default for DragScopeBuilder {
    fn default() -> Self {
        drag_scope("drag")
    }
}

// ---------------------------------------------------------------------------
// Draggable
// ---------------------------------------------------------------------------

pub struct DraggableBuilder {
    id: String,
    item_type: String,
    group: String,
    list: Option<String>,
    index: Option<usize>,
    disabled: bool,
    handle_only: bool,
    also_droppable: bool,
    accept: String,
    /// Resting opacity (`opacity` attr). Drag-time 本体 uses [`Self::source_opacity`].
    opacity: Option<f32>,
    /// Drag-time source opacity while a `DragOverlay` is open. `None` uses the
    /// overlay session value (default [`super::OVERLAY_SOURCE_OPACITY`]).
    source_opacity: Option<f32>,
    /// Overlap stacking vs siblings. Default [`DragStack::Auto`].
    stack: DragStack,
    /// Item-level ghost clamp. `None` inherits the provider [`Restrict`].
    restrict: Option<Restrict>,
    skip_shift: bool,
    skip_follow: bool,
    pull_clone: bool,
    data: Option<Rc<dyn Any>>,
}

pub fn draggable(id: impl Into<String>) -> DraggableBuilder {
    DraggableBuilder {
        id: id.into(),
        item_type: String::new(),
        group: String::new(),
        list: None,
        index: None,
        disabled: false,
        handle_only: false,
        also_droppable: false,
        accept: String::new(),
        opacity: None,
        source_opacity: None,
        stack: DragStack::Auto,
        restrict: None,
        skip_shift: false,
        skip_follow: false,
        pull_clone: false,
        data: None,
    }
}

impl DraggableBuilder {
    pub fn item_type(mut self, t: impl Into<String>) -> Self {
        self.item_type = t.into();
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }
    pub fn list(mut self, list: impl Into<String>) -> Self {
        self.list = Some(list.into());
        self
    }
    pub fn index(mut self, index: usize) -> Self {
        self.index = Some(index);
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
    pub fn also_droppable(mut self, yes: bool) -> Self {
        self.also_droppable = yes;
        self
    }
    pub fn accept(mut self, accept: impl Into<String>) -> Self {
        self.accept = accept.into();
        self
    }

    /// Resting opacity of this item (same as [`Element::opacity`]).
    pub fn opacity(mut self, o: impl IntoOpacity) -> Self {
        self.opacity = Some(o.into_opacity());
        self
    }

    /// Opacity of **this** source island while a `DragOverlay` is open.
    /// `0` leaves the insert hole empty. Does not apply at rest.
    pub fn source_opacity(mut self, o: impl IntoOpacity) -> Self {
        self.source_opacity = Some(o.into_opacity());
        self
    }

    /// Overlap stacking: `auto` (drag raises and keeps), `top` / `bottom`
    /// (always), `keep` (author `z-index` only).
    pub fn stack(mut self, stack: impl Into<DragStack>) -> Self {
        self.stack = stack.into();
        self
    }

    /// Ghost travel box for **this** item. Omitted inherits the provider.
    /// `viewport` / `window` = the window; `parent` = the author parent (no id
    /// needed); any other token is a live element id (Droppable / `id` /
    /// `template_id`). Cross-parent drops use `viewport` or omit; Gallery
    /// 四角旋转 can use `parent` or the arena id.
    pub fn restrict(mut self, restrict: impl Into<Restrict>) -> Self {
        self.restrict = Some(restrict.into());
        self
    }

    /// Skip sibling FLIP / sortable shift (solid-dnd `skipTransform` → shift).
    pub fn skip_shift(mut self, yes: bool) -> Self {
        self.skip_shift = yes;
        self
    }

    /// Skip kit pointer-follow / hole translate; author owns motion.
    pub fn skip_follow(mut self, yes: bool) -> Self {
        self.skip_follow = yes;
        self
    }

    /// SortableJS `pull: 'clone'` — source stays; drop inserts a copy.
    pub fn pull_clone(mut self, yes: bool) -> Self {
        self.pull_clone = yes;
        self
    }

    /// Arbitrary payload (solid-dnd `data` bag). Readable from [`DragItemCtx`] /
    /// [`super::session::DragEvent::data`].
    pub fn data(mut self, payload: Rc<dyn Any>) -> Self {
        self.data = Some(payload);
        self
    }

    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let ctx = require_drag("Draggable");
        let item_ctx = DragItemCtx {
            id: self.id.clone(),
            scope_id: ctx.id.clone(),
            list: self.list.clone(),
            index: self.index,
            item_type: self.item_type.clone(),
            group: self.group.clone(),
        };
        assemble_draggable(ctx, self, item_ctx, body)
    }
}

fn assemble_draggable(
    ctx: DragCtx,
    cfg: DraggableBuilder,
    item_ctx: DragItemCtx,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let scope = ctx.id.clone();
    let id = cfg.id.clone();
    // Overlay clone must inline the author body. `reactive_view_sized` is
    // idempotent on tid: a nested SortableItem inside the clone would steal
    // (or empty) the live island, and a fresh tid only yields an unfilled
    // placeholder under `isolate_reads`.
    if is_cloning_overlay_ghost() {
        let el = with_child_owner(|| {
            provide_context(item_context(), item_ctx);
            wrap_fragment(body())
        });
        return keyed_item_shell(el);
    }
    remember_item_restrict(&scope, &id, cfg.restrict.clone());
    let flags = super::registry::ItemFlags {
        skip_shift: cfg.skip_shift,
        skip_follow: cfg.skip_follow,
        pull_clone: cfg.pull_clone,
    };
    super::registry::set_item_flags(&scope, &id, flags);
    if let Some(data) = cfg.data.clone() {
        super::registry::set_item_data(&scope, &id, data);
    }
    let item_flags = flags;
    let tid = format!("{scope}__draggable:{id}");
    let capture = format!("{scope}__drag:{id}");
    let active = ctx.active_draggable;
    let overlay_open = ctx.overlay_open;
    let from_list_sig = ctx.from_list;
    let from_index_sig = ctx.from_index;
    let hover_list_sig = ctx.hover_list;
    let hover_index_sig = ctx.hover_index;
    let drag_extent_sig = ctx.drag_extent;
    let axis = ctx.axis;
    let animation = ctx.animation;
    let selected_sig = ctx.selected;
    let handle_only = cfg.handle_only;
    let disabled = cfg.disabled;
    let also_droppable = cfg.also_droppable;
    let item_type = cfg.item_type;
    let group = cfg.group;
    let accept = cfg.accept;
    let rest_opacity = cfg.opacity;
    let source_opacity = cfg.source_opacity;
    let stack = cfg.stack;
    let id_for_mark = id.clone();
    let scope_for_mark = scope.clone();
    let index_sig = live_slot(format!("{tid}__idx").as_str(), cfg.index);
    let list_sig = live_slot(format!("{tid}__list").as_str(), cfg.list.clone());
    let auto_rank = use_signal(format!("{tid}__zrank").as_str(), || 0i32);
    let auto_bumped = Rc::new(Cell::new(false));
    let author_z_cell = Rc::new(Cell::new(0i32));
    let author_island_block = Rc::new(Cell::new(
        None::<(taffy::style::Dimension, taffy::style::Dimension)>,
    ));
    let auto_bumped_shell = Rc::clone(&auto_bumped);
    let author_z_shell = Rc::clone(&author_z_cell);
    let body: Rc<dyn Fn() -> Element> = Rc::new(body);

    let island_w = if also_droppable { "100%" } else { "auto" };
    let island = reactive_view_sized(tid, island_w, "auto", move || {
        let index = index_sig.get();
        let list = list_sig.get();
        let _ = selected_sig.get();
        let mut item_ctx = item_ctx.clone();
        item_ctx.index = index;
        item_ctx.list = list.clone();
        register_drag_slot_meta(
            &scope_for_mark,
            &id_for_mark,
            true,
            also_droppable,
            &item_type,
            &accept,
            list.as_deref(),
            index,
            &group,
            disabled,
            None,
            item_flags,
        );
        register_source_preview(
            &scope_for_mark,
            &id_for_mark,
            Rc::clone(&body),
            item_ctx.clone(),
            source_opacity,
        );
        let mut el = with_child_owner(|| {
            provide_context(item_context(), item_ctx.clone());
            wrap_fragment((body)())
        });
        remember_source_snapshot(&scope_for_mark, &id_for_mark, &el);
        mark_scope(&mut el, &scope_for_mark);
        el.set_attr(ATTR_DRAG_DRAGGABLE, &id_for_mark);
        if also_droppable {
            el.set_attr(ATTR_DRAG_DROPPABLE, &id_for_mark);
        }
        if !item_type.is_empty() {
            el.set_attr(ATTR_DRAG_TYPE, &item_type);
        }
        if !accept.is_empty() {
            el.set_attr(ATTR_DRAG_ACCEPT, &accept);
        }
        if !group.is_empty() {
            el.set_attr(ATTR_DRAG_GROUP, &group);
        }
        if let Some(ref list) = list {
            el.set_attr(ATTR_DRAG_LIST, list);
        }
        if let Some(index) = index {
            el.set_attr(ATTR_DRAG_INDEX, index.to_string());
        }
        if disabled {
            el.set_attr(ATTR_DRAG_DISABLED, "1");
        }
        if item_flags.skip_shift {
            el.set_attr(ATTR_DRAG_SKIP_SHIFT, "1");
        }
        if item_flags.skip_follow {
            el.set_attr(ATTR_DRAG_SKIP_FOLLOW, "1");
        }
        if item_flags.pull_clone {
            el.set_attr(ATTR_DRAG_PULL_CLONE, "1");
        }
        if el.focus_id.is_none() {
            el = el.focus_id(capture.as_str());
        }
        if let Some(o) = rest_opacity {
            el = el.opacity(o);
        }
        let author_opacity = el.style.opacity;
        let author_z = el.style.z_index;
        author_z_cell.set(author_z);
        let preview_id = id_for_mark.clone();
        let preview_scope = scope_for_mark.clone();
        let motion = Rc::new(RefCell::new(MotionStyle::new()));
        let auto_bumped = Rc::clone(&auto_bumped);
        let author_island_block = Rc::clone(&author_island_block);
        el = el.reactive_sync(move |el, _lt: &mut LayoutTree| {
            apply_live_drag_chrome(
                el,
                &preview_scope,
                &preview_id,
                also_droppable,
                axis,
                list_sig,
                index_sig,
                active,
                overlay_open,
                from_list_sig,
                from_index_sig,
                hover_list_sig,
                hover_index_sig,
                drag_extent_sig,
                author_opacity,
                source_opacity,
                author_z,
                stack,
                auto_rank,
                auto_bumped.as_ref(),
                animation,
                &mut motion.borrow_mut(),
                author_island_block.as_ref(),
            )
        });
        if !handle_only && !disabled {
            let scope = scope_for_mark.clone();
            let drag_id = id_for_mark.clone();
            let item_type = item_type.clone();
            let group = group.clone();
            let capture = capture.clone();
            el = el.on_pointer_down(move |e| {
                if e.phase == EventPhase::Capture || dom_button(e.button) != 0 {
                    return;
                }
                let (grab_offset, start_bounds) =
                    start_geometry(&scope, &drag_id, [e.x, e.y], e.current_bounds);
                begin(BeginDrag {
                    scope_id: scope.clone(),
                    draggable_id: drag_id.clone(),
                    pointer: [e.x, e.y],
                    grab_offset,
                    start_bounds,
                    from_list: list_sig.peek(),
                    from_index: index_sig.peek(),
                    item_type: item_type.clone(),
                    group: group.clone(),
                    capture_id: capture.clone(),
                });
                e.stop_propagation();
            });
        }
        el
    });
    let mut shell = keyed_item_shell(island);
    let shell_id = id.clone();
    let shell_scope = scope.clone();
    let author_margin_shell = Rc::new(Cell::new(None::<f32>));
    let author_block_shell = Rc::new(Cell::new(None::<AuthorBlock>));
    let author_pos_shell = Rc::new(Cell::new(None::<taffy::style::Position>));
    let author_width_shell = Rc::new(Cell::new(None::<taffy::style::Dimension>));
    shell = shell.reactive_sync(move |el, _lt: &mut LayoutTree| {
        apply_shell_stack_z(
            el,
            &shell_id,
            &shell_scope,
            active,
            stack,
            auto_rank,
            auto_bumped_shell.as_ref(),
            author_z_shell.as_ref(),
            from_list_sig,
            hover_list_sig,
            drag_extent_sig,
            axis,
            list_sig,
            author_margin_shell.as_ref(),
            author_block_shell.as_ref(),
            author_pos_shell.as_ref(),
            author_width_shell.as_ref(),
        )
    });
    shell
}

// ---------------------------------------------------------------------------
// Droppable
// ---------------------------------------------------------------------------

pub struct DroppableBuilder {
    id: String,
    accept: String,
    group: String,
    list: Option<String>,
    disabled: bool,
    opacity: Option<f32>,
    /// Sortable insert / FLIP axis for this list. `None` inherits the provider.
    direction: Option<Axis>,
    dropzone: super::dropzone::DropzoneConfig,
}

pub fn droppable(id: impl Into<String>) -> DroppableBuilder {
    DroppableBuilder {
        id: id.into(),
        accept: String::new(),
        group: String::new(),
        list: None,
        disabled: false,
        opacity: None,
        direction: None,
        dropzone: super::dropzone::DropzoneConfig::default(),
    }
}

impl DroppableBuilder {
    pub fn accept(mut self, accept: impl Into<String>) -> Self {
        self.accept = accept.into();
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }
    pub fn list(mut self, list_id: impl Into<String>) -> Self {
        self.list = Some(list_id.into());
        self
    }
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
    pub fn opacity(mut self, o: impl IntoOpacity) -> Self {
        self.opacity = Some(o.into_opacity());
        self
    }
    /// List-level sortable axis. Overrides the provider for insert index / FLIP.
    pub fn direction(mut self, direction: Axis) -> Self {
        self.direction = Some(direction);
        self
    }
    /// interact dropzone `overlap` (`pointer` / `center` / `0..1` ratio).
    pub fn overlap(mut self, overlap: super::dropzone::DropOverlap) -> Self {
        self.dropzone.overlap = overlap;
        self
    }
    /// interact dropzone `checker` — veto when it returns `false`.
    pub fn drop_checker(mut self, f: super::dropzone::DropCheckerFn) -> Self {
        self.dropzone.checker = Some(f);
        self
    }
    pub fn on_drop_activate(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.dropzone.hooks.on_activate = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drop_deactivate(
        mut self,
        f: impl FnMut(&super::session::DragEvent) + 'static,
    ) -> Self {
        self.dropzone.hooks.on_deactivate = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drag_enter(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.dropzone.hooks.on_enter = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drag_leave(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.dropzone.hooks.on_leave = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drop_move(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.dropzone.hooks.on_move = Some(Rc::new(RefCell::new(f)));
        self
    }
    pub fn on_drop(mut self, f: impl FnMut(&super::session::DragEvent) + 'static) -> Self {
        self.dropzone.hooks.on_drop = Some(Rc::new(RefCell::new(f)));
        self
    }
    /// RSX / pre-wrapped [`DropzoneHandler`] (same shape as [`DragHandler`]).
    pub fn on_drop_activate_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_activate = Some(h);
        self
    }
    pub fn on_drop_deactivate_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_deactivate = Some(h);
        self
    }
    pub fn on_drag_enter_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_enter = Some(h);
        self
    }
    pub fn on_drag_leave_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_leave = Some(h);
        self
    }
    pub fn on_drop_move_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_move = Some(h);
        self
    }
    pub fn on_drop_handler(mut self, h: super::dropzone::DropzoneHandler) -> Self {
        self.dropzone.hooks.on_drop = Some(h);
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        let ctx = require_drag("Droppable");
        assemble_droppable(ctx, self, body)
    }
}

fn assemble_droppable(
    ctx: DragCtx,
    cfg: DroppableBuilder,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let scope = ctx.id.clone();
    let id = cfg.id.clone();
    super::dropzone::set_dropzone_config(&scope, &id, cfg.dropzone.clone());
    if is_cloning_overlay_ghost() {
        return wrap_fragment(body());
    }
    let tid = format!("{scope}__droppable:{id}");
    let accept = cfg.accept;
    let group = cfg.group;
    let list = cfg.list.unwrap_or_else(|| id.clone());
    let disabled = cfg.disabled;
    let rest_opacity = cfg.opacity;
    let direction = cfg.direction;
    let id_mark = id.clone();
    let scope_mark = scope.clone();

    let active_droppable = ctx.active_droppable;
    let from_list = ctx.from_list;
    let hover_list = ctx.hover_list;
    let hover_index = ctx.hover_index;
    let overlay_open = ctx.overlay_open;
    let drag_extent = ctx.drag_extent;
    let axis = ctx.axis;

    // Height `auto` (not `100%`): a 100% shell in a wrap HStack next to an
    // auto-width chip wraps onto a row whose containing-block height is still
    // indefinite, so the zone collapses to a sliver (Gallery: 源卡片的容器丢失).
    // Do not subscribe `active_droppable` here — hover is paint, not structure.
    reactive_view_sized(tid, "auto", "auto", move || {
        let mut el = wrap_fragment(body());
        mark_scope(&mut el, &scope_mark);
        if let Some(o) = rest_opacity {
            el = el.opacity(o);
        }
        el.set_attr(ATTR_DRAG_DROPPABLE, &id_mark);
        el.set_attr(ATTR_DRAG_LIST, &list);
        if !accept.is_empty() {
            el.set_attr(ATTR_DRAG_ACCEPT, &accept);
        }
        if !group.is_empty() {
            el.set_attr(ATTR_DRAG_GROUP, &group);
        }
        if disabled {
            el.set_attr(ATTR_DRAG_DISABLED, "1");
        }
        if let Some(d) = direction {
            el.set_attr(
                ATTR_DRAG_AXIS,
                match d.sort_axis() {
                    Axis::X => "x",
                    _ => "y",
                },
            );
        }
        register_drag_slot_meta(
            &scope_mark,
            &id_mark,
            false,
            true,
            "",
            &accept,
            Some(list.as_str()),
            None,
            &group,
            disabled,
            direction.map(|d| d.sort_axis()),
            super::registry::ItemFlags::default(),
        );
        let author_z = el.style.z_index;
        let prev = el.reactive_sync.take();
        let id_chrome = id_mark.clone();
        let list_chrome = list.clone();
        let scope_chrome = scope_mark.clone();
        let author_pad = Rc::new(Cell::new(None::<f32>));
        let author_margin = Rc::new(Cell::new(None::<f32>));
        let author_pos = Rc::new(Cell::new(None::<taffy::style::Position>));
        el = el.reactive_sync(move |el, lt| {
            let mut st = prev
                .as_ref()
                .map(|p| p.run(el, lt))
                .unwrap_or(LayoutStatus::Clean);
            st = st.merge(apply_live_droppable_chrome(
                el,
                &scope_chrome,
                &id_chrome,
                &list_chrome,
                active_droppable,
                from_list,
                hover_list,
                hover_index,
                overlay_open,
                drag_extent,
                axis,
                author_pad.as_ref(),
                author_margin.as_ref(),
                author_pos.as_ref(),
                author_z,
            ));
            st
        });
        el
    })
}

// ---------------------------------------------------------------------------
// Handle
// ---------------------------------------------------------------------------

pub fn drag_handle(mut el: Element) -> Element {
    let item = use_drag_item().unwrap_or_else(|| {
        panic!("<DragHandle> / drag_handle() must be used within <Draggable> / <SortableItem>");
    });
    el.set_attr(ATTR_DRAG_HANDLE, "1");
    if is_cloning_overlay_ghost() {
        return el;
    }
    if el.focus_id.is_none() {
        el = el.focus_id(format!("{}__drag_handle:{}", item.scope_id, item.id));
    }
    let capture = el.focus_id.clone().unwrap_or_else(|| item.id.clone());
    el.on_pointer_down(move |e| {
        if e.phase == EventPhase::Capture || dom_button(e.button) != 0 {
            return;
        }
        let (grab_offset, start_bounds) =
            start_geometry(&item.scope_id, &item.id, [e.x, e.y], e.current_bounds);
        begin(BeginDrag {
            scope_id: item.scope_id.clone(),
            draggable_id: item.id.clone(),
            pointer: [e.x, e.y],
            grab_offset,
            start_bounds,
            from_list: item.list.clone(),
            from_index: item.index,
            item_type: item.item_type.clone(),
            group: item.group.clone(),
            capture_id: capture.clone(),
        });
        e.stop_propagation();
    })
}

pub fn drag_handle_from_rsx(body: impl FnOnce() -> Element) -> Element {
    drag_handle(wrap_fragment(body()))
}

/// Sortable `filter`: stop a nested control from starting a parent drag.
pub fn drag_ignore(mut el: Element) -> Element {
    el.set_attr(ATTR_DRAG_IGNORE, "1");
    el.on_pointer_down(move |e| {
        e.stop_propagation();
    })
}

pub fn drag_ignore_from_rsx(body: impl FnOnce() -> Element) -> Element {
    drag_ignore(wrap_fragment(body()))
}

/// RSX `<Draggable>` / `<Droppable>` entry points.
pub fn draggable_from_rsx(opts: DraggableRsxOpts, body: impl Fn() -> Element + 'static) -> Element {
    let mut b = draggable(opts.id)
        .item_type(opts.item_type)
        .group(opts.group)
        .disabled(opts.disabled)
        .handle_only(opts.handle_only)
        .also_droppable(opts.also_droppable)
        .accept(opts.accept);
    if let Some(list) = opts.list {
        b = b.list(list);
    }
    if let Some(index) = opts.index {
        b = b.index(index);
    }
    if let Some(opacity) = opts.opacity {
        b = b.opacity(opacity);
    }
    if let Some(source_opacity) = opts.source_opacity {
        b = b.source_opacity(source_opacity);
    }
    if let Some(restrict) = opts.restrict {
        b = b.restrict(restrict);
    }
    if opts.skip_shift {
        b = b.skip_shift(true);
    }
    if opts.skip_follow {
        b = b.skip_follow(true);
    }
    if opts.pull_clone {
        b = b.pull_clone(true);
    }
    if let Some(data) = opts.data {
        b = b.data(data);
    }
    b.stack(opts.stack).child(body)
}

pub struct DraggableRsxOpts {
    pub id: String,
    pub item_type: String,
    pub group: String,
    pub list: Option<String>,
    pub index: Option<usize>,
    pub disabled: bool,
    pub handle_only: bool,
    pub also_droppable: bool,
    pub accept: String,
    pub opacity: Option<f32>,
    pub source_opacity: Option<f32>,
    pub stack: DragStack,
    pub restrict: Option<Restrict>,
    pub skip_shift: bool,
    pub skip_follow: bool,
    pub pull_clone: bool,
    pub data: Option<Rc<dyn Any>>,
}

pub fn droppable_from_rsx(opts: DroppableRsxOpts, body: impl Fn() -> Element + 'static) -> Element {
    let mut b = droppable(opts.id)
        .accept(opts.accept)
        .group(opts.group)
        .disabled(opts.disabled);
    if let Some(list) = opts.list {
        b = b.list(list);
    }
    if let Some(opacity) = opts.opacity {
        b = b.opacity(opacity);
    }
    if let Some(direction) = opts.direction {
        b = b.direction(direction);
    }
    if let Some(overlap) = opts.overlap {
        b = b.overlap(overlap);
    }
    if let Some(checker) = opts.drop_checker {
        b = b.drop_checker(checker);
    }
    if let Some(h) = opts.on_drop_activate {
        b = b.on_drop_activate_handler(h);
    }
    if let Some(h) = opts.on_drop_deactivate {
        b = b.on_drop_deactivate_handler(h);
    }
    if let Some(h) = opts.on_drag_enter {
        b = b.on_drag_enter_handler(h);
    }
    if let Some(h) = opts.on_drag_leave {
        b = b.on_drag_leave_handler(h);
    }
    if let Some(h) = opts.on_drop_move {
        b = b.on_drop_move_handler(h);
    }
    if let Some(h) = opts.on_drop {
        b = b.on_drop_handler(h);
    }
    b.child(body)
}

pub struct DroppableRsxOpts {
    pub id: String,
    pub accept: String,
    pub group: String,
    pub list: Option<String>,
    pub disabled: bool,
    pub opacity: Option<f32>,
    pub direction: Option<Axis>,
    pub overlap: Option<DropOverlap>,
    pub drop_checker: Option<DropCheckerFn>,
    pub on_drop_activate: Option<DropzoneHandler>,
    pub on_drop_deactivate: Option<DropzoneHandler>,
    pub on_drag_enter: Option<DropzoneHandler>,
    pub on_drag_leave: Option<DropzoneHandler>,
    pub on_drop_move: Option<DropzoneHandler>,
    pub on_drop: Option<DropzoneHandler>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_render_effect, create_root};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn live_slot_notifies_when_index_changes() {
        let (_o, _) = create_root(|| {
            let runs = Rc::new(Cell::new(0));
            let runs2 = runs.clone();
            let slot = live_slot("drag-live-idx", Some(1usize));
            create_render_effect(move || {
                let _ = slot.get();
                runs2.set(runs2.get() + 1);
            });
            assert_eq!(runs.get(), 1);
            live_slot("drag-live-idx", Some(0usize));
            assert!(
                runs.get() >= 2,
                "for_keyed remap must refresh frozen island index; runs={}",
                runs.get()
            );
            assert_eq!(slot.peek(), Some(0));
        });
    }

    #[test]
    fn sortable_shift_flip_animates_mid_then_snaps_on_session_end() {
        use crate::style::{
            set_css_transition_timeline_ms, set_css_transition_virtual_timeline,
            set_prefers_reduced_motion, TimingFunction, TransitionEntry, TransitionProperty,
        };
        use crate::utils::motion::{ensure_motion_transitions, snap_motion_baseline};
        set_prefers_reduced_motion(Some(false));
        set_css_transition_virtual_timeline(true);
        set_css_transition_timeline_ms(0.0);
        let mut el = Element::new();
        el.style.translate = [0.0, 0.0];
        ensure_motion_transitions(
            &mut el.style,
            &[TransitionEntry {
                property: TransitionProperty::Transform,
                duration_ms: SORTABLE_FLIP_MS,
                delay_ms: 0.0,
                timing: TimingFunction::Ease,
            }],
        );
        snap_motion_baseline(&mut el);
        el.style.translate = [0.0, 40.0];
        assert!(motion_is_animating(&el.style, &el.transition_snapshot));
        let _ = el.tick_css_transitions(0.0);
        set_css_transition_timeline_ms(f64::from(SORTABLE_FLIP_MS) * 0.5);
        el.style.translate = [0.0, 40.0];
        let _ = el.tick_css_transitions(0.0);
        assert!(
            el.style.translate[1] > 8.0 && el.style.translate[1] < 36.0,
            "direct motion helpers mid, got {}",
            el.style.translate[1]
        );
        let mut motion = MotionStyle::new();
        apply_item_shift_motion(&mut el, [0.0, 24.0], 1.0, false, true, true, &mut motion);
        assert!(
            !el.style.transition.is_empty(),
            "live session must attach FLIP transition"
        );
        assert_eq!(el.style.translate[1], 24.0);
        assert!(apply_item_shift_motion(
            &mut el,
            [0.0, 0.0],
            1.0,
            false,
            false,
            true,
            &mut motion
        ));
        assert_eq!(el.style.translate, [0.0, 0.0]);
        assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
        set_css_transition_virtual_timeline(false);
        set_prefers_reduced_motion(None);
    }

    #[test]
    fn pointer_follow_skips_flip_transition() {
        let mut el = Element::new();
        let mut motion = MotionStyle::new();
        apply_item_shift_motion(&mut el, [12.0, 8.0], 1.0, true, true, true, &mut motion);
        assert_eq!(el.style.translate, [12.0, 8.0]);
        assert!(el.style.transition.is_empty());
        apply_item_shift_motion(&mut el, [20.0, 14.0], 1.0, true, true, true, &mut motion);
        assert_eq!(el.style.translate, [20.0, 14.0]);
        assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
    }

    #[test]
    fn sortable_shift_snaps_when_animation_disabled() {
        let mut el = Element::new();
        let mut motion = MotionStyle::new();
        apply_item_shift_motion(&mut el, [0.0, 40.0], 1.0, false, true, false, &mut motion);
        assert_eq!(el.style.translate, [0.0, 40.0]);
        assert!(
            el.style.transition.is_empty(),
            "animation=false must not attach FLIP transition"
        );
        apply_item_shift_motion(&mut el, [0.0, 0.0], 1.0, false, true, false, &mut motion);
        assert_eq!(el.style.translate, [0.0, 0.0]);
        assert!(!motion_is_animating(&el.style, &el.transition_snapshot));
    }
}
