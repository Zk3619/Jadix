//! Dual-write builders: `drag_scope` / `draggable` / `droppable` / `drag_handle`.
//! RSX tags expand to these functions (ADR-9). Not in `prelude`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::pointer_events::{dom_button, EventPhase};
use crate::app::reactive::reactive_view_sized;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_signal, with_child_owner, Context, Signal,
};
use crate::core::tree::{Element, IntoOpacity, LayoutStatus};
use crate::layout::LayoutTree;
use crate::primitives::vstack;
use crate::style::{TimingFunction, TransitionEntry, TransitionProperty};
use crate::utils::motion::{
    clear_motion, motion_is_animating, motion_sync_status, prepare_motion_sync, MotionBootstrap,
    MotionEpsilon, MotionStyle, MotionTransform,
};
use crate::utils::resolve_instance_id;

use super::autoscroll::{AutoScroll, ScrollLock};
use super::collision::CollisionAlg;
use super::geometry::{Axis, DragStack, Restrict, SnapGrid};
use super::session::{
    active_source_translate, begin, bump_auto_stack, list_max_index, list_slot_stride,
    live_cross_list_source_translate, live_same_list_source_translate, overlay_is_registered,
    overlay_source_opacity, start_geometry, BeginDrag, DragHandler, DragPredicate, ScopeRuntime,
};
use super::sortable::{sortable_preview, SORTABLE_FLIP_MS};
use super::{
    ATTR_DRAG_ACCEPT, ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE,
    ATTR_DRAG_GROUP, ATTR_DRAG_HANDLE, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX, ATTR_DRAG_LIST,
    ATTR_DRAG_OVER, ATTR_DRAG_SCOPE, ATTR_DRAG_TYPE,
};

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
}

thread_local! {
    static SOURCE_PREVIEWS: RefCell<HashMap<(String, String), SourcePreview>> =
        RefCell::new(HashMap::new());
}

fn register_source_preview(
    scope: &str,
    id: &str,
    body: Rc<dyn Fn() -> Element>,
    item: DragItemCtx,
) {
    SOURCE_PREVIEWS.with(|m| {
        m.borrow_mut().insert(
            (scope.to_string(), id.to_string()),
            SourcePreview { body, item },
        );
    });
}

/// Re-run the active item's author body (not the dimmed/translated island wrapper).
pub(super) fn clone_source_preview(scope: &str, draggable_id: &str) -> Option<Element> {
    let preview = SOURCE_PREVIEWS.with(|m| {
        m.borrow()
            .get(&(scope.to_string(), draggable_id.to_string()))
            .cloned()
    })?;
    Some(with_child_owner(|| {
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
	/// both pills share `rects → text` and lower glyphs paint on the upper fill.
	fn keyed_item_shell(island: Element) -> Element {
	    let mut el =
	        Element::from(vstack().min_w(0.0).flex_shrink(0.0).child(island)).overflow("visible");
	    el.set_attr("data-stacking-context", "1");
	    el
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
    if el.style.z_index != want_z {
        el.style.z_index = want_z;
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
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
    drag_extent: Signal<[f32; 2]>,
) -> [f32; 2] {
    if !also_droppable {
        return [0.0, 0.0];
    }
    let extent = drag_extent.peek();
    let fallback = if axis == Axis::X {
        extent[0]
    } else {
        extent[1]
    };
    let item_e = list_slot_stride(list, axis, fallback.max(1.0));
    sortable_preview(
        from_list.peek().as_deref(),
        from_index.peek(),
        hover_list.peek().as_deref(),
        hover_index.peek(),
        list,
        index,
        item_e,
        axis,
        list_max_index(list),
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
    live_same_list_source_translate(fl, from, hl, hover, axis)
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
) -> LayoutStatus {
    let list = list_sig.peek();
    let index = index_sig.peek();
    let is_active = active.peek().as_deref() == Some(id);
    let overlay_on = overlay_is_registered(scope) && overlay_open.peek();
    let mut want_opacity = author_opacity;
    let mut want_translate = [0.0_f32, 0.0];
    let mut follow_pointer = false;
    if is_active {
        if overlay_on {
            let src_op = item_source_opacity.unwrap_or_else(|| overlay_source_opacity(scope));
            let hide = src_op <= 0.001;
            let from_l = from_list.peek();
            let hover_l = hover_list.peek();
            let cross = from_l.is_some() && hover_l.is_some() && from_l != hover_l;
            // opacity 0: origin slot empty. Dimmed: body sits in the insert hole
            // siblings opened — origin is covered by 让位, so pinning the dimmed
            // chip there is invisible (Gallery screenshot).
            want_opacity = src_op;
            if hide {
                follow_pointer = true;
            } else if cross {
                if let (Some(fl), Some(fi), Some(hl)) =
                    (from_l.as_deref(), from_index.peek(), hover_l.as_deref())
                {
                    want_translate =
                        live_cross_list_source_translate(fl, fi, hl, hover_index.peek(), axis);
                }
            } else {
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
        if also_droppable {
            want_translate = live_sortable_translate(
                also_droppable,
                axis,
                list.as_deref(),
                index,
                from_list,
                from_index,
                hover_list,
                hover_index,
                drag_extent,
            );
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
    motion_sync_status(
        el,
        changed != LayoutStatus::Clean || shift_changed,
        follow_pointer,
    )
}

/// Paint-only droppable chrome: `rui_drag_over` (solid-dnd `isActiveDroppable`)
/// on the **hover target**, not the origin list, and a stacking bump so a
/// dimmed source translated into another column paints above the target
/// container. Colors stay with the caller.
fn apply_live_droppable_chrome(
    el: &mut Element,
    id: &str,
    list: &str,
    active_droppable: Signal<Option<String>>,
    from_list: Signal<Option<String>>,
    hover_list: Signal<Option<String>>,
    author_z: i32,
) -> LayoutStatus {
    let from = from_list.peek();
    let hover = hover_list.peek();
    let over = active_droppable.peek().as_deref() == Some(id) || hover.as_deref() == Some(list);
    let is_origin = from.as_deref() == Some(list);
    let cross = is_origin && hover.is_some() && hover != from;
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
    let want_z = if cross { author_z.max(2) } else { author_z };
    if el.style.z_index != want_z {
        el.style.z_index = want_z;
        changed = changed.merge(LayoutStatus::StyleChanged);
    }
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
    threshold: f32,
    restrict: Restrict,
    snap: Option<SnapGrid>,
    delay_ms: f32,
    empty_insert: f32,
    autoscroll: AutoScroll,
    can_drop: Option<DragPredicate>,
    on_start: Option<DragHandler>,
    on_move: Option<DragHandler>,
    on_over: Option<DragHandler>,
    on_end: Option<DragHandler>,
    /// Sibling / source-hole FLIP. Default `true` (`SORTABLE_FLIP_MS`). `false` snaps.
    animation: bool,
}

pub fn drag_scope(id: impl Into<String>) -> DragScopeBuilder {
    DragScopeBuilder {
        id: id.into(),
        collision: CollisionAlg::MostIntersecting,
        axis: Axis::Xy,
        threshold: 4.0,
        restrict: Restrict::None,
        snap: None,
        delay_ms: 0.0,
        empty_insert: 8.0,
        autoscroll: AutoScroll::default(),
        can_drop: None,
        on_start: None,
        on_move: None,
        on_over: None,
        on_end: None,
        animation: true,
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
    pub fn threshold(mut self, px: f32) -> Self {
        self.threshold = px.max(0.0);
        self
    }
    pub fn restrict(mut self, restrict: Restrict) -> Self {
        self.restrict = restrict;
        self
    }
    pub fn snap(mut self, snap: SnapGrid) -> Self {
        self.snap = Some(snap);
        self
    }
    /// Hold delay in milliseconds before the drag activates (Sortable `delay`).
    pub fn delay_ms(mut self, ms: f32) -> Self {
        self.delay_ms = ms.max(0.0);
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
        super::session::register_scope(
            id.clone(),
            ScopeRuntime {
                collision: self.collision,
                axis: self.axis,
                threshold: self.threshold,
                restrict: self.restrict,
                snap: self.snap,
                overlay_registered: false,
                source_opacity: 1.0,
                autoscroll: self.autoscroll,
                empty_insert: self.empty_insert,
                delay_ms: self.delay_ms,
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
            },
        );
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
    let auto_bumped_shell = Rc::clone(&auto_bumped);
    let author_z_shell = Rc::clone(&author_z_cell);
    let body: Rc<dyn Fn() -> Element> = Rc::new(body);

    let island_w = if also_droppable { "100%" } else { "auto" };
    let island = reactive_view_sized(tid, island_w, "auto", move || {
        let index = index_sig.get();
        let list = list_sig.get();
        let mut item_ctx = item_ctx.clone();
        item_ctx.index = index;
        item_ctx.list = list.clone();
        register_source_preview(
            &scope_for_mark,
            &id_for_mark,
            Rc::clone(&body),
            item_ctx.clone(),
        );
        let mut el = with_child_owner(|| {
            provide_context(item_context(), item_ctx.clone());
            wrap_fragment((body)())
        });
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
}

pub fn droppable(id: impl Into<String>) -> DroppableBuilder {
    DroppableBuilder {
        id: id.into(),
        accept: String::new(),
        group: String::new(),
        list: None,
        disabled: false,
        opacity: None,
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
    let tid = format!("{scope}__droppable:{id}");
    let accept = cfg.accept;
    let group = cfg.group;
    let list = cfg.list.unwrap_or_else(|| id.clone());
    let disabled = cfg.disabled;
    let rest_opacity = cfg.opacity;
    let id_mark = id.clone();
    let scope_mark = scope.clone();

    let active_droppable = ctx.active_droppable;
    let from_list = ctx.from_list;
    let hover_list = ctx.hover_list;

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
        let author_z = el.style.z_index;
        let prev = el.reactive_sync.take();
        let id_chrome = id_mark.clone();
        let list_chrome = list.clone();
        el = el.reactive_sync(move |el, lt| {
            let mut st = prev
                .as_ref()
                .map(|p| p.run(el, lt))
                .unwrap_or(LayoutStatus::Clean);
            st = st.merge(apply_live_droppable_chrome(
                el,
                &id_chrome,
                &list_chrome,
                active_droppable,
                from_list,
                hover_list,
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
    b.child(body)
}

pub struct DroppableRsxOpts {
    pub id: String,
    pub accept: String,
    pub group: String,
    pub list: Option<String>,
    pub disabled: bool,
    pub opacity: Option<f32>,
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
