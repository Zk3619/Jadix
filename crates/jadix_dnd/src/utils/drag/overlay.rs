//! Drag overlay (solid-dnd `DragOverlay`): a Pinned portal whose author inset
//! follows the pointer. Uses [`portal_sync`] + [`OverlayMode::Pinned`] so the
//! ghost is not on the Esc stack (overlay_shells.md §9.3).
//!
//! Geometry is **Unanchored** (`ATTR_FLOATING` only). Point placement runs
//! [`compute_cursor_menu_placement`](crate::utils::popup::compute_cursor_menu_placement)
//! and clamps Y to the viewport floor; seeding `apply_stored_floating_place`
//! with `match_trigger=0` + `strategy=fixed` writes overflow-hidden at (0,0)
//! until measure and the ghost vanishes.
//!
//! Pointer follow shares Point overlay paint-follow: Taffy stays at the grab
//! origin, collect patches `abs`, CursorMoved queues Overlay dirty. Clip shells
//! still write inner relative inset (restrict box is not the chip).

use std::cell::Cell;
use std::rc::Rc;

use crate::app::portal_overlay::{pinned_overlay_z, portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::authoring::mount::pin_overlay_origin as pin_overlay_follow_insets;
use crate::core::reactivity::{isolate_reads, with_child_owner, Signal};
use crate::core::tree::{lpa_length_px, Element, IntoCssDim, IntoOpacity, LayoutStatus, UnitSlot};
use crate::layout::{dim_kind, dim_length_px, DimKind};
use crate::utils::popup::{flow_hidden_portal_host, mount_anchored_overlay, ATTR_FLOATING};

use super::builder::{
    clone_source_preview, provide_drag_ctx, require_drag, source_preview_item, DragCtx, DragItemCtx,
};
use super::geometry::{Axis, OverlayAlign};
use super::session::{
    active_drag_ids, mark_overlay_registered, overlay_center_origin_now,
    overlay_origin_after_ghost_measure, peek_restrict_clip, remember_overlay_ghost_extent,
    with_scope,
};
use super::{
    ATTR_DRAG_DISABLED, ATTR_DRAG_DRAGGABLE, ATTR_DRAG_DROPPABLE, ATTR_DRAG_GROUP,
    ATTR_DRAG_HANDLE, ATTR_DRAG_IGNORE, ATTR_DRAG_INDEX, ATTR_DRAG_ITEM_SHELL, ATTR_DRAG_LIST,
    ATTR_DRAG_OVER, ATTR_DRAG_OVERLAY, ATTR_DRAG_OVERLAY_CLIP, ATTR_DRAG_OVERLAY_INNER,
    ATTR_DRAG_SCOPE, ATTR_DRAG_TYPE,
};

/// Pin the ghost at viewport `origin` (item top-left, already grab-offset).
///
/// `fixed` so left/top resolve against the window even before hoist — Gallery
/// hosts the overlay placeholder inside a ScrollArea. No POINT / PLACEMENT, so
/// `sync_floating_placement` does not rewrite the inset.
///
/// Placement only (`fixed` left/top). Size is decided by the caller: follow-source
/// clones pin `drag_extent` so `w=100%` does not resolve against the portal CB;
/// a custom overlay child keeps its author size (stretching a chip to the Handle
/// row is the huge empty slab).
///
/// `clip` (`RestrictOverflow::Hidden`): wrap in a viewport clip shell so rotation
/// / center-anchor corners do not paint past the restrict box. Inner ghost is
/// `absolute` (not `fixed`) so it does not escape the shell's overflow.
fn place_drag_ghost(
    mut panel: Element,
    ghost_id: &str,
    origin: [f32; 2],
    clip: Option<[f32; 4]>,
) -> Element {
    super::session_runtime::install();
    panel.style.overflow.x = taffy::style::Overflow::Visible;
    panel.style.overflow.y = taffy::style::Overflow::Visible;
    if let Some(bbox) = clip.filter(|b| b[2] > 0.5 && b[3] > 0.5) {
        // Inner is `absolute` and must not carry `ATTR_DRAG_OVERLAY`: the heap
        // walk in `sync_overlay_ghost_insets` would rewrite relative inset with
        // the viewport origin.
        panel = panel
            .pointer_events_none()
            .absolute()
            .left(origin[0] - bbox[0])
            .top(origin[1] - bbox[1]);
        let scope = ghost_id.strip_suffix("__ghost").unwrap_or(ghost_id);
        panel.set_attr(ATTR_DRAG_OVERLAY_INNER, scope);
        let mut shell = Element::new()
            .pointer_events_none()
            .fixed()
            .left(bbox[0])
            .top(bbox[1])
            .w(bbox[2])
            .h(bbox[3])
            .overflow("hidden")
            .child(panel);
        shell.set_attr(ATTR_FLOATING, ghost_id);
        shell.set_attr(ATTR_DRAG_OVERLAY, "1");
        shell.set_attr(ATTR_DRAG_OVERLAY_CLIP, "1");
        shell.set_attr(crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP, "1");
        shell
    } else {
        panel.set_attr(ATTR_FLOATING, ghost_id);
        panel.set_attr(ATTR_DRAG_OVERLAY, "1");
        panel.set_attr(crate::layout::layout_tree::ATTR_ESCAPE_OVERFLOW_CLIP, "1");
        panel
            .pointer_events_none()
            .fixed()
            .left(origin[0])
            .top(origin[1])
    }
}

fn write_len(el: &mut Element, slot: UnitSlot, v: f32) -> bool {
    let same = match slot {
        UnitSlot::Width => dim_length_px(el.style.size.width).is_some_and(|x| (x - v).abs() < 0.01),
        UnitSlot::Height => {
            dim_length_px(el.style.size.height).is_some_and(|x| (x - v).abs() < 0.01)
        }
        UnitSlot::InsetL => {
            lpa_length_px(el.style.inset.left).is_some_and(|x| (x - v).abs() < 0.01)
        }
        UnitSlot::InsetT => lpa_length_px(el.style.inset.top).is_some_and(|x| (x - v).abs() < 0.01),
        _ => return false,
    };
    if same {
        return false;
    }
    el.style.set_css_dim(slot, v.into_css_dim());
    true
}

/// Write `fixed` left/top without reconstructing the ghost. Returns whether inset changed.
fn write_ghost_origin(el: &mut Element, origin: [f32; 2]) -> bool {
    let dx = write_len(el, UnitSlot::InsetL, origin[0]);
    let dy = write_len(el, UnitSlot::InsetT, origin[1]);
    dx || dy
}

fn write_clip_shell(el: &mut Element, bbox: [f32; 4], origin: [f32; 2]) -> (bool, bool) {
    let mut origin_moved = write_ghost_origin(el, [bbox[0], bbox[1]]);
    let size_moved =
        write_len(el, UnitSlot::Width, bbox[2]) || write_len(el, UnitSlot::Height, bbox[3]);
    if let Some(inner) = el.children.first_mut() {
        origin_moved =
            write_ghost_origin(inner, [origin[0] - bbox[0], origin[1] - bbox[1]]) || origin_moved;
    }
    (origin_moved || size_moved, size_moved)
}

/// Dest-reset old + new ghost boxes. Origin follow is paint-only (`StyleChanged`);
/// without Overlay dirty, PartialLoad keeps the previous chip.
fn note_ghost_follow_damage(el: &Element, origin: [f32; 2]) {
    crate::authoring::mount::invalidate_paint(el);
    let w = el.layout_bounds[2];
    let h = el.layout_bounds[3];
    remember_ghost_follow_chrome(el, origin, [w, h]);
    if w > 0.5 && h > 0.5 {
        crate::app::damage::push_frame_overlay_damage(
            crate::layout::layout_tree::overlay_portal_stamp_key(el),
            [origin[0], origin[1], w, h],
        );
    }
}

/// Dest-reset the restrict clip box (old + live) and the chip. Page scroll
/// moves the shell; chip-only dirty leaves ink at the pre-scroll parent.
fn note_clip_shell_damage(el: &Element, old: [f32; 4], bbox: [f32; 4], origin: [f32; 2]) {
    crate::authoring::mount::invalidate_paint(el);
    let stamp = crate::layout::layout_tree::overlay_portal_stamp_key(el);
    if old[2] > 0.5 && old[3] > 0.5 {
        crate::app::damage::push_frame_overlay_damage(stamp, old);
    }
    if bbox[2] > 0.5 && bbox[3] > 0.5 {
        crate::app::damage::push_frame_overlay_damage(stamp, bbox);
    }
    let inner = el
        .children
        .first()
        .map(|c| [c.layout_bounds[2], c.layout_bounds[3]]);
    let size = match inner {
        Some([w, h]) if w > 0.5 && h > 0.5 => [w, h],
        _ => [bbox[2], bbox[3]],
    };
    remember_ghost_follow_chrome(el, origin, size);
    if size[0] > 0.5 && size[1] > 0.5 {
        crate::app::damage::push_frame_overlay_damage(
            stamp,
            [origin[0], origin[1], size[0], size[1]],
        );
    }
}

struct GhostFollow {
    stamp_key: Cell<u64>,
    panel_size: Cell<[f32; 2]>,
    last_visual: Cell<Option<[f32; 2]>>,
    base: Cell<Option<[f32; 2]>>,
}

crate::authoring::window::window_local! {
    static GHOST_FOLLOW: GhostFollow = const {
        GhostFollow {
            stamp_key: Cell::new(0),
            panel_size: Cell::new([0.0, 0.0]),
            last_visual: Cell::new(None),
            base: Cell::new(None),
        }
    };
}

fn remember_ghost_follow_chrome(el: &Element, visual: [f32; 2], size: [f32; 2]) {
    GHOST_FOLLOW.with(|g| {
        g.stamp_key
            .set(crate::layout::layout_tree::overlay_portal_stamp_key(el));
        if size[0] > 0.5 && size[1] > 0.5 {
            g.panel_size.set(size);
        }
        g.last_visual.set(Some(visual));
    });
}

fn ghost_follow_base() -> Option<[f32; 2]> {
    GHOST_FOLLOW.with(|g| g.base.get())
}

fn capture_ghost_follow_base(base: [f32; 2]) {
    GHOST_FOLLOW.with(|g| {
        if g.base.get().is_none() {
            g.base.set(Some(base));
        }
    });
}

/// Pin Taffy at the first origin; later ticks only patch collect `abs`.
fn pin_ghost_follow(el: &mut Element, visual: [f32; 2]) {
    let base = ghost_follow_base().unwrap_or(visual);
    capture_ghost_follow_base(base);
    pin_overlay_follow_insets(el, base);
}

pub(crate) fn clear_ghost_follow() {
    GHOST_FOLLOW.with(|g| {
        g.stamp_key.set(0);
        g.panel_size.set([0.0, 0.0]);
        g.last_visual.set(None);
        g.base.set(None);
    });
}

/// CursorMoved: Overlay dirty at the live origin before RedrawRequested take.
pub(crate) fn note_ghost_follow_tick(visual: [f32; 2]) {
    GHOST_FOLLOW.with(|g| {
        crate::app::damage::queue_overlay_follow_tick_damage(
            g.stamp_key.get(),
            g.panel_size.get(),
            g.last_visual.get(),
            visual,
        );
        if g.stamp_key.get() != 0 && g.panel_size.get()[0] > 0.5 && g.panel_size.get()[1] > 0.5 {
            g.last_visual.set(Some(visual));
        }
    });
}

fn paint_target(el: &mut Element) -> &mut Element {
    if el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_some() && !el.children.is_empty() {
        &mut el.children[0]
    } else {
        el
    }
}

/// After Taffy measured an auto-sized Center ghost, pin paint origin to the
/// live pointer in the same collect. Taffy stays at the grab origin (Point
/// follow); collect patches `abs`. Clip shells patch to the live restrict box
/// so page / autoscroll moves overflow clip with the parent.
pub(crate) fn adjust_overlay_collect_origin(
    el: &mut Element,
    measured: [f32; 2],
) -> Option<[f32; 2]> {
    let scale = crate::authoring::mount::painted_scale(el);
    let measured = [measured[0] * scale[0], measured[1] * scale[1]];
    if el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_some() {
        return peek_restrict_clip()
            .filter(|b| b[2] > 0.5 && b[3] > 0.5)
            .map(|b| [b[0], b[1]]);
    }
    if let Some(scope) = el.attrs.get(ATTR_DRAG_OVERLAY_INNER).cloned() {
        let origin = overlay_origin_after_ghost_measure(&scope, measured)?;
        remember_ghost_follow_chrome(el, origin, measured);
        return Some(origin);
    }
    if el.attrs.get(ATTR_DRAG_OVERLAY).is_none() {
        return None;
    }
    let scope = el
        .attrs
        .get(ATTR_FLOATING)?
        .strip_suffix("__ghost")?
        .to_string();
    let origin = overlay_origin_after_ghost_measure(&scope, measured)?;
    pin_ghost_follow(el, origin);
    remember_ghost_follow_chrome(el, origin, measured);
    Some(origin)
}

const ATTR_GHOST_FOR: &str = "jadix_drag_ghost_for";

fn build_ghost_body(
    custom: &Option<Rc<dyn Fn() -> Element>>,
    body_for: &Option<Rc<dyn Fn(&DragItemCtx) -> Element>>,
    decorate: &Option<Rc<dyn Fn(Element) -> Element>>,
    source_id: Option<String>,
    scope_id: &str,
    ghost_id: &str,
    extent: [f32; 2],
    drag_ctx: &DragCtx,
) -> Element {
    with_child_owner(|| {
        provide_drag_ctx(drag_ctx.clone());
        build_ghost_body_inner(
            custom, body_for, decorate, source_id, scope_id, ghost_id, extent,
        )
    })
}

fn build_ghost_body_inner(
    custom: &Option<Rc<dyn Fn() -> Element>>,
    body_for: &Option<Rc<dyn Fn(&DragItemCtx) -> Element>>,
    decorate: &Option<Rc<dyn Fn(Element) -> Element>>,
    source_id: Option<String>,
    scope_id: &str,
    ghost_id: &str,
    extent: [f32; 2],
) -> Element {
    let following_source = custom.is_none() && body_for.is_none();
    let mut panel = if let Some(for_item) = body_for {
        match source_id.as_deref() {
            Some(id) => {
                let item = source_preview_item(scope_id, id).unwrap_or_else(|| DragItemCtx {
                    id: id.to_string(),
                    scope_id: scope_id.to_string(),
                    list: None,
                    index: None,
                    item_type: String::new(),
                    group: String::new(),
                });
                isolate_reads(|| for_item(&item))
            }
            None => Element::new().w(0.0).h(0.0),
        }
    } else if let Some(body) = custom {
        isolate_reads(|| body())
    } else if let Some(id) = source_id.as_deref() {
        isolate_reads(|| {
            let ids = active_drag_ids();
            if ids.len() > 1 {
                stacked_ghost_panel(scope_id, &ids)
            } else {
                clone_source_preview(scope_id, id).unwrap_or_else(Element::new)
            }
        })
    } else {
        Element::new().w(0.0).h(0.0)
    };
    strip_source_session_attrs(&mut panel);
    suffix_ghost_ids(&mut panel, ghost_id);
    if following_source {
        let ids = active_drag_ids();
        if ids.len() <= 1 {
            panel = pin_source_size_to_extent(panel, extent);
        }
    }
    if let Some(dec) = decorate {
        panel = dec(panel);
    }
    if let (Some(w), Some(h)) = (
        dim_length_px(panel.style.size.width),
        dim_length_px(panel.style.size.height),
    ) {
        remember_overlay_ghost_extent(scope_id, [w, h]);
    }
    panel
}

/// The portal node *is* [`place_drag_ghost`] output — same as remount, without
/// `try_reconcile`. Grafting a clone onto a previously laid-out 0×0 host
/// (`adopt`) keeps `used_layout` / border-box at a point; close then leaves
/// that bordered pill on the layer.
fn write_placed_ghost(
    host: &mut Element,
    body: Element,
    ghost_id: &str,
    origin: [f32; 2],
    clip: Option<[f32; 4]>,
) {
    crate::authoring::mount::replace_portal_content(
        host,
        place_drag_ghost(body, ghost_id, origin, clip),
    );
}

/// Paint-time ghost body + origin. The island itself must not remount on
/// `overlay_open`: `try_reconcile_subtree` misses when AppState is borrowed
/// (pointerdown / first activating move), and source chrome already hid the body.
fn sync_drag_overlay_ghost(
    panel: Element,
    open: Signal<bool>,
    pos: Signal<[f32; 2]>,
    active: Signal<Option<String>>,
    extent: Signal<[f32; 2]>,
    scope_id: String,
    ghost_id: String,
    custom: Option<Rc<dyn Fn() -> Element>>,
    body_for: Option<Rc<dyn Fn(&DragItemCtx) -> Element>>,
    decorate: Option<Rc<dyn Fn(Element) -> Element>>,
    drag_ctx: DragCtx,
) -> Element {
    panel.reactive_sync(move |el, _| {
        let mut changed = LayoutStatus::Clean;
        let is_open = open.peek();
        let source_id = active.peek();
        let clip = peek_restrict_clip().filter(|b| b[2] > 0.5 && b[3] > 0.5);
        let have_clip = el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_some();
        // Once Hidden opened a clip shell, keep it even if a tick fails to
        // resolve the restrict box (page scroll / stale hits). Dropping the
        // shell remounts an unclipped ghost that breaks out of the parent.
        let want_clip = is_open && (clip.is_some() || have_clip);
        let body_key = if !is_open {
            String::new()
        } else if let Some(id) = &source_id {
            id.clone()
        } else {
            "__open__".to_string()
        };
        let have = el.attrs.get(ATTR_GHOST_FOR).cloned().unwrap_or_default();
        if body_key != have || want_clip != have_clip {
            if body_key.is_empty() {
                write_placed_ghost(el, Element::new(), &ghost_id, [0.0, 0.0], None);
                el.attrs.remove(ATTR_GHOST_FOR);
            } else {
                let body = build_ghost_body(
                    &custom,
                    &body_for,
                    &decorate,
                    source_id,
                    &scope_id,
                    &ghost_id,
                    extent.peek(),
                    &drag_ctx,
                );
                write_placed_ghost(el, body, &ghost_id, pos.peek(), clip);
                el.set_attr(ATTR_GHOST_FOR, body_key);
            }
            // New Element tree, no `layout_node`. LayoutChanged would keep the
            // empty host's 0×0 Taffy box (bordered radius → a leftover dot).
            changed = changed.merge(LayoutStatus::StructureChanged);
        }
        if !is_open {
            return changed;
        }
        let origin = pos.peek();
        let clip_bbox = peek_restrict_clip();
        let old_shell = el.layout_bounds;
        let (moved, size_moved) = if el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_some() {
            if let Some(bbox) = clip_bbox {
                write_clip_shell(el, bbox, origin)
            } else {
                // Keep the last restrict box. Moving the shell to the chip
                // origin lets overflow-hidden travel with the ghost.
                (false, false)
            }
        } else {
            pin_ghost_follow(el, origin);
            (true, false)
        };
        if size_moved {
            // Clip-shell size is Taffy geometry. Pointer follow is not.
            changed = changed.merge(LayoutStatus::LayoutChanged);
        } else if moved {
            // Inset is paint chrome (`adjust_overlay_collect_origin` patches abs).
            // LayoutChanged disabled page collect-skip and the Gallery ghost lagged.
            changed = changed.merge(LayoutStatus::StyleChanged);
            if let Some(bbox) = clip_bbox {
                note_clip_shell_damage(el, old_shell, bbox, origin);
            } else {
                note_ghost_follow_damage(el, origin);
            }
        }
        if is_open {
            if let Some(dec) = &decorate {
                let sample = dec(Element::new());
                let target = paint_target(el);
                if (target.style.rotate_deg - sample.style.rotate_deg).abs() > 0.01 {
                    target.style.rotate_deg = sample.style.rotate_deg;
                    changed = changed.merge(LayoutStatus::StyleChanged);
                }
            }
        }
        changed
    })
}

/// Heap walk so a deep gallery tree does not map onto the OS stack.
pub(crate) fn sync_overlay_ghost_insets(el: &mut Element) {
    let mut stack: Vec<*mut Element> = vec![el as *mut Element];
    while let Some(ptr) = stack.pop() {
        let el = unsafe { &mut *ptr };
        if el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
            if let Some(scope) = el
                .attrs
                .get(ATTR_FLOATING)
                .and_then(|fid| fid.strip_suffix("__ghost"))
                .map(str::to_string)
            {
                if el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_none() {
                    let [_, _, w, h] = el.layout_bounds;
                    if w > 1.0 && h > 1.0 {
                        remember_overlay_ghost_extent(&scope, [w, h]);
                    }
                }
            }
            let origin = el.attrs.get(ATTR_FLOATING).cloned().and_then(|fid| {
                fid.strip_suffix("__ghost").and_then(|scope| {
                    overlay_center_origin_now(scope)
                        .or_else(|| super::session::peek_overlay_pos(scope))
                })
            });
            if let Some(p) = origin {
                if el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).is_some() {
                    if let Some(bbox) = peek_restrict_clip() {
                        let old = el.layout_bounds;
                        write_clip_shell(el, bbox, p);
                        note_clip_shell_damage(el, old, bbox, p);
                    }
                } else if el.style.position_fixed {
                    pin_ghost_follow(el, p);
                    note_ghost_follow_damage(el, p);
                }
            }
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
}

/// Percentages resolve against the drag extent instead of the portal. Auto
/// dimensions retain the measured author body, including parent-driven stretch;
/// the drag extent can belong to a larger island wrapper around a compact chip.
fn pin_source_size_to_extent(mut panel: Element, extent: [f32; 2]) -> Element {
    let size = |dim, measured, fallback| match dim_kind(dim) {
        DimKind::Auto => measured,
        DimKind::Percent(_) => fallback,
        _ => 0.0,
    };
    let w = size(panel.style.size.width, panel.layout_bounds[2], extent[0]);
    let h = size(panel.style.size.height, panel.layout_bounds[3], extent[1]);
    if w.is_finite() && w > 0.0 {
        panel = panel.w(w);
    }
    if h.is_finite() && h > 0.0 {
        panel = panel.h(h);
    }
    panel
}

/// Builder: register an overlay slot inside the current [`super::drag_scope`].
///
/// Default (no [`Self::child`]): the ghost is the active draggable's author body.
/// [`Self::style`] is applied on top of that clone (or on a custom child).
pub struct DragOverlayBuilder {
    body: Option<Rc<dyn Fn() -> Element>>,
    body_for: Option<Rc<dyn Fn(&DragItemCtx) -> Element>>,
    decorate: Option<Rc<dyn Fn(Element) -> Element>>,
    /// Source-island opacity while this overlay is open. Applied to the **body**,
    /// not the ghost. `None` → [`super::OVERLAY_SOURCE_OPACITY`].
    source_opacity: Option<f32>,
    align: OverlayAlign,
}

impl DragOverlayBuilder {
    pub fn child(mut self, body: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Rc::new(body));
        self
    }

    /// Custom ghost from the active item (solid-dnd Overlay function children).
    /// Rebuilds with `open` / `active` / `extent` — not `overlay_pos`.
    pub fn child_for(mut self, f: impl Fn(&DragItemCtx) -> Element + 'static) -> Self {
        self.body_for = Some(Rc::new(f));
        self
    }

    /// Ghost origin vs the pointer. Default [`OverlayAlign::Grab`].
    /// [`OverlayAlign::Center`] pins the ghost midpoint to the pointer.
    pub fn align(mut self, align: OverlayAlign) -> Self {
        self.align = align;
        self
    }

    /// Overlay the clone / custom child. Later calls compose after earlier ones.
    pub fn style(mut self, f: impl Fn(Element) -> Element + 'static) -> Self {
        let prev = self.decorate.take();
        self.decorate = Some(Rc::new(move |el| {
            let el = match &prev {
                Some(p) => p(el),
                None => el,
            };
            f(el)
        }));
        self
    }

    /// Opacity of the in-list **source** while this overlay is open.
    /// `0` leaves the insert hole empty; default is [`super::OVERLAY_SOURCE_OPACITY`].
    /// Does not paint the ghost — RSX `opacity` / [`Self::style`] is resting/ghost paint.
    pub fn source_opacity(mut self, o: impl IntoOpacity) -> Self {
        self.source_opacity = Some(o.into_opacity());
        self
    }
}

fn strip_source_session_attrs(el: &mut Element) {
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
    ] {
        el.attrs.remove(key);
    }
    el.on_mousedown = None;
    el.on_mouseup = None;
    el.on_mousemove = None;
    el.on_click = None;
    for child in &mut el.children {
        strip_source_session_attrs(child);
    }
}

fn suffix_ghost_ids(el: &mut Element, suffix: &str) {
    if let Some(tid) = &mut el.template_id {
        tid.push_str("::");
        tid.push_str(suffix);
    }
    if let Some(fid) = &mut el.focus_id {
        fid.push_str("::");
        fid.push_str(suffix);
    }
    for child in &mut el.children {
        suffix_ghost_ids(child, suffix);
    }
}

pub(crate) fn stacked_ghost_panel(scope: &str, ids: &[String]) -> Element {
    let axis = with_scope(scope, |rt| {
        super::session::peek_sort_axis(rt.from_list.peek().as_deref(), rt.axis)
    })
    .unwrap_or(Axis::Y);
    let mut wrap = Element::new().relative().gap(super::session::pack_gap(0.0));
    wrap = if axis == Axis::X {
        wrap.flex_row()
    } else {
        wrap.flex_col()
    };
    let extent = super::session::last_drag_extent(scope).unwrap_or([0.0, 0.0]);
    for id in ids {
        let mut chip = clone_source_preview(scope, id).unwrap_or_else(Element::new);
        chip = pin_source_size_to_extent(chip, extent).flex_shrink(0.0);
        wrap = wrap.child(chip);
    }
    wrap
}

impl From<DragOverlayBuilder> for Element {
    fn from(builder: DragOverlayBuilder) -> Self {
        let ctx = require_drag("DragOverlay");
        mark_overlay_registered(&ctx.id);
        with_scope(&ctx.id, |rt| {
            rt.source_opacity = builder
                .source_opacity
                .unwrap_or(super::OVERLAY_SOURCE_OPACITY);
            rt.overlay_align = builder.align;
        });
        let custom = builder.body;
        let body_for = builder.body_for;
        let decorate = builder.decorate;
        let view_id = format!("{}__drag_overlay", ctx.id);
        let open = ctx.overlay_open;
        let pos = ctx.overlay_pos;
        let active = ctx.active_draggable;
        let extent = ctx.drag_extent;
        let scope_id = ctx.id.clone();
        let ghost_id = format!("{}__ghost", ctx.id);

        reactive_anchor(view_id.clone(), move || {
            // Only the active provider mounts a portal. The local 0x0 controller
            // also synchronizes during paint, so activation while AppState is
            // borrowed does not have to wait for an island remount. Subscribe
            // only to membership changes, ensuring offscreen close still removes
            // the portal slot; pointer follow stays in the ghost's paint hook.
            let _ = open.get();
            let _ = portal_sync(
                ghost_id.as_str(),
                open,
                OverlayKind::Popover,
                OverlayMode::Pinned,
            );
            let scope_id = scope_id.clone();
            let ghost_id = ghost_id.clone();
            let custom = custom.clone();
            let body_for = body_for.clone();
            let decorate = decorate.clone();
            let ctx = ctx.clone();
            flow_hidden_portal_host(Element::new()).reactive_sync(move |host, _| {
                if !open.peek() {
                    if host.children.is_empty() {
                        return LayoutStatus::Clean;
                    }
                    host.children.clear();
                    return LayoutStatus::StructureChanged;
                }
                // Hoisting replaces the panel with a keep-alive slot. Retain it
                // throughout the gesture rather than rebuilding on every frame.
                if !host.children.is_empty() {
                    return LayoutStatus::Clean;
                }
                let panel = place_drag_ghost(Element::new(), &ghost_id, pos.peek(), None);
                let panel = sync_drag_overlay_ghost(
                    panel,
                    open,
                    pos,
                    active,
                    extent,
                    scope_id.clone(),
                    ghost_id.clone(),
                    custom.clone(),
                    body_for.clone(),
                    decorate.clone(),
                    ctx.clone(),
                );
                host.children
                    .push(mount_anchored_overlay(panel, pinned_overlay_z().panel));
                LayoutStatus::StructureChanged
            })
        });
        flow_hidden_portal_host(
            reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none(),
        )
    }
}

pub fn drag_overlay() -> DragOverlayBuilder {
    super::session_runtime::install();
    DragOverlayBuilder {
        body: None,
        body_for: None,
        decorate: None,
        source_opacity: None,
        align: OverlayAlign::Grab,
    }
}

/// RSX `<DragOverlay>` with children — same implementation as the Builder.
pub fn drag_overlay_from_rsx(body: impl Fn() -> Element + 'static) -> Element {
    drag_overlay().child(body).into()
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/overlay.rs"]
mod tests;
