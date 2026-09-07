//! Dual-write builders: `drag_scope` / `draggable` / `droppable` / `drag_handle`.
//! RSX tags expand to these functions (ADR-9). Not in `prelude`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::app::pointer_events::{EventPhase, dom_button};
use crate::app::reactive::reactive_view_sized;
use crate::core::reactivity::{
    Context, Signal, create_context, provide_context, use_context, use_signal, with_child_owner,
};
use crate::core::tree::Element;
use crate::primitives::vstack;
use crate::utils::resolve_instance_id;

use super::autoscroll::AutoScroll;
use super::collision::CollisionAlg;
use super::geometry::{Axis, Restrict, SnapGrid};
use super::session::{
    BeginDrag, DragHandler, DragPredicate, ScopeRuntime, active_source_translate, begin,
    overlay_hides_source, overlay_is_registered, start_geometry,
};
use super::sortable::sortable_preview;
use super::{
    ATTR_DRAG_ACCEPT, ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE,
    ATTR_DRAG_GROUP, ATTR_DRAG_HANDLE, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX, ATTR_DRAG_LIST,
    ATTR_DRAG_SCOPE, ATTR_DRAG_TYPE,
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
fn keyed_item_shell(island: Element) -> Element {
    Element::from(vstack().min_w(0.0).child(island))
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
                hide_source: false,
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
        };
        let mut root = with_child_owner(|| {
            provide_context(drag_context(), ctx);
            body()
        });
        if root.template_id.is_none() {
            root.template_id = Some(format!("{id}__drag_scope"));
        }
        mark_scope(&mut root, &id);
        root
    }
}

/// RSX `<DragProvider>` — same `child` path as the Builder.
pub fn drag_scope_from_rsx(opts: DragScopeRsxOpts, body: impl FnOnce() -> Element) -> Element {
    let mut b = drag_scope(opts.id)
        .collision(opts.collision)
        .axis(opts.axis);
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

#[derive(Default)]
pub struct DragScopeRsxOpts {
    pub id: String,
    pub collision: CollisionAlg,
    pub axis: Axis,
    pub threshold: f32,
    pub restrict: Restrict,
    pub on_drag_start: Option<DragHandler>,
    pub on_drag_over: Option<DragHandler>,
    pub on_drag_end: Option<DragHandler>,
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
    let pos = ctx.overlay_pos;
    let from_list_sig = ctx.from_list;
    let from_index_sig = ctx.from_index;
    let hover_list_sig = ctx.hover_list;
    let hover_index_sig = ctx.hover_index;
    let drag_extent_sig = ctx.drag_extent;
    let axis = ctx.axis;
    let handle_only = cfg.handle_only;
    let disabled = cfg.disabled;
    let also_droppable = cfg.also_droppable;
    let item_type = cfg.item_type;
    let group = cfg.group;
    let accept = cfg.accept;
    let id_for_mark = id.clone();
    let scope_for_mark = scope.clone();
    let index_sig = live_slot(format!("{tid}__idx").as_str(), cfg.index);
    let list_sig = live_slot(format!("{tid}__list").as_str(), cfg.list.clone());
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
        let is_active = active.get().as_deref() == Some(id_for_mark.as_str());
        // Subscribe even when inactive so overlay close rebuilds from a fresh body
        // (default opacity 1). Do not write opacity 1 as a restore — that papers over
        // a missed rebuild.
        let overlay_on = overlay_is_registered(&scope_for_mark) && overlay_open.get();
        if is_active {
            let _ = pos.get();
            if overlay_on {
                let dim = if overlay_hides_source(&scope_for_mark) {
                    0.0
                } else {
                    0.4
                };
                el = el.opacity(dim);
            } else if let Some(t) = active_source_translate() {
                el = el.translate(t[0], t[1]);
            }
        } else {
            if also_droppable {
                let _ = hover_index_sig.get();
                let _ = hover_list_sig.get();
                let _ = from_index_sig.get();
                let _ = from_list_sig.get();
                let extent = drag_extent_sig.peek();
                let item_e = if axis == Axis::X {
                    extent[0]
                } else {
                    extent[1]
                };
                let t = sortable_preview(
                    from_list_sig.peek().as_deref(),
                    from_index_sig.peek(),
                    hover_list_sig.peek().as_deref(),
                    hover_index_sig.peek(),
                    list.as_deref(),
                    index,
                    item_e.max(1.0),
                    axis,
                );
                if t != [0.0, 0.0] {
                    el = el.translate(t[0], t[1]);
                } else {
                    el = el.translate(0.0, 0.0);
                }
            }
        }
        if !handle_only && !disabled {
            let scope = scope_for_mark.clone();
            let drag_id = id_for_mark.clone();
            let item_type = item_type.clone();
            let group = group.clone();
            let list = list.clone();
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
                    from_list: list.clone(),
                    from_index: index,
                    item_type: item_type.clone(),
                    group: group.clone(),
                    capture_id: capture.clone(),
                });
                e.stop_propagation();
            });
        }
        el
    });
    keyed_item_shell(island)
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
}

pub fn droppable(id: impl Into<String>) -> DroppableBuilder {
    DroppableBuilder {
        id: id.into(),
        accept: String::new(),
        group: String::new(),
        list: None,
        disabled: false,
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
    let id_mark = id.clone();
    let scope_mark = scope.clone();

    // Height `auto` (not `100%`): a 100% shell in a wrap HStack next to an
    // auto-width chip wraps onto a row whose containing-block height is still
    // indefinite, so the zone collapses to a sliver (Gallery: 源卡片的容器丢失).
    // Do not subscribe `active_droppable` here — hover is paint, not structure.
    reactive_view_sized(tid, "auto", "auto", move || {
        let mut el = wrap_fragment(body());
        mark_scope(&mut el, &scope_mark);
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
    b.child(body)
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
}

pub fn droppable_from_rsx(opts: DroppableRsxOpts, body: impl Fn() -> Element + 'static) -> Element {
    let mut b = droppable(opts.id)
        .accept(opts.accept)
        .group(opts.group)
        .disabled(opts.disabled);
    if let Some(list) = opts.list {
        b = b.list(list);
    }
    b.child(body)
}

pub struct DroppableRsxOpts {
    pub id: String,
    pub accept: String,
    pub group: String,
    pub list: Option<String>,
    pub disabled: bool,
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
}
