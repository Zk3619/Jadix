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
use crate::core::reactivity::{isolate_reads, with_child_owner, Signal};
use crate::core::tree::{lpa_length_px, Element, IntoCssDim, IntoOpacity, LayoutStatus, UnitSlot};
use crate::layout::layout_tree::{ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT};
use crate::layout::{dim_kind, dim_length_px, DimKind};
use crate::utils::anchored_overlay::pin_overlay_follow_insets;
use crate::utils::popup::{mount_anchored_overlay, ATTR_FLOATING};

use super::builder::{
    clone_source_preview, provide_drag_ctx, require_drag, source_preview_item, DragCtx, DragItemCtx,
};
use super::geometry::OverlayAlign;
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
    crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
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
    crate::app::damage::push_frame_paint_element(el, el.paint_scroll_acc);
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

thread_local! {
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

const ATTR_GHOST_FOR: &str = "rui_drag_ghost_for";

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
            panel = pin_percent_size_to_extent(panel, extent);
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
    let sync = host.reactive_sync.take();
    let z = host.style.z_index;
    let portal_key = host.attrs.get(ATTR_PORTAL_KEY).cloned();
    let portal = host.attrs.get("rui_portal").cloned();
    let portal_mount = host.attrs.get(ATTR_PORTAL_MOUNT).cloned();
    *host = place_drag_ghost(body, ghost_id, origin, clip);
    host.reactive_sync = sync;
    if z != 0 {
        host.style.z_index = z;
    }
    if let Some(k) = portal_key {
        host.set_attr(ATTR_PORTAL_KEY, k);
    }
    if let Some(v) = portal {
        host.set_attr("rui_portal", v);
    }
    if let Some(m) = portal_mount {
        host.set_attr(ATTR_PORTAL_MOUNT, m);
    }
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

/// Follow-source clones of `w=100%` / `h=100%` must pin the used `drag_extent`
/// (portal CB would resolve the percentage into a slab). Auto-sized chips must
/// keep author size — `start_bounds` is the stretched island, not the chip.
fn pin_percent_size_to_extent(mut panel: Element, extent: [f32; 2]) -> Element {
    if matches!(dim_kind(panel.style.size.width), DimKind::Percent(_)) && extent[0] > 0.0 {
        panel = panel.w(extent[0]);
    }
    if matches!(dim_kind(panel.style.size.height), DimKind::Percent(_)) && extent[1] > 0.0 {
        panel = panel.h(extent[1]);
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

fn stacked_ghost_panel(scope: &str, ids: &[String]) -> Element {
    let mut wrap = Element::new().relative();
    for (i, id) in ids.iter().enumerate() {
        let mut chip = clone_source_preview(scope, id).unwrap_or_else(Element::new);
        if i > 0 {
            let o = i as f32 * 4.0;
            chip = chip.absolute().left(o).top(o);
        }
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
            // Mount the portal host once. Do **not** subscribe `open` / `active` /
            // `overlay_pos`: remount-on-open races AppState `try_borrow_mut`
            // (pointerdown / first activating move) and leaves source_opacity=0
            // with no stand-in. Body + origin are paint (`sync_drag_overlay_ghost`).
            let _ = portal_sync(
                ghost_id.as_str(),
                open,
                OverlayKind::Popover,
                OverlayMode::Pinned,
            );
            let mut panel = place_drag_ghost(Element::new(), ghost_id.as_str(), pos.peek(), None);
            panel = sync_drag_overlay_ghost(
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
            mount_anchored_overlay(panel, pinned_overlay_z().panel)
        });
        reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
    }
}

pub fn drag_overlay() -> DragOverlayBuilder {
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

#[cfg(test)]
mod tests {
    extern crate self as rui;

    use super::*;
    use crate::core::reactivity::{create_render_effect, create_root, Signal};
    use crate::utils::popup::{ATTR_FLOATING_PLACEMENT, ATTR_FLOATING_POINT};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn overlay_island_reruns_when_open_flips() {
        let (_o, _) = create_root(|| {
            let open = Signal::new(false);
            let runs = Rc::new(Cell::new(0));
            let runs2 = runs.clone();
            create_render_effect(move || {
                runs2.set(runs2.get() + 1);
                let is_open = open.get();
                if !is_open {
                    let _ = portal_sync(
                        "drag-ov-test",
                        open,
                        OverlayKind::Popover,
                        OverlayMode::Pinned,
                    );
                    return;
                }
                let _ = portal_sync(
                    "drag-ov-test",
                    open,
                    OverlayKind::Popover,
                    OverlayMode::Pinned,
                );
            });
            assert_eq!(runs.get(), 1);
            open.set(true);
            assert!(runs.get() >= 2);
        });
    }

    #[test]
    fn overlay_default_source_opacity_is_dimmed() {
        use crate::utils::drag::{
            drag_overlay, drag_scope, overlay_hides_source, overlay_source_opacity,
        };
        let (_o, _) = create_root(|| {
            let _el: Element = drag_scope("item-op").child(|| Element::from(drag_overlay()));
            assert!(
                (overlay_source_opacity("item-op") - 0.4).abs() < 0.001,
                "default overlay session opacity is 0.4"
            );
            assert!(!overlay_hides_source("item-op"));
        });
    }

    #[test]
    fn overlay_opacity_zero_marks_scope() {
        use crate::utils::drag::{drag_overlay, drag_scope};
        let (_o, _) = create_root(|| {
            let _el: Element =
                drag_scope("hide-src").child(|| Element::from(drag_overlay().source_opacity(0.0)));
            assert!(
                crate::utils::drag::overlay_hides_source("hide-src"),
                "DragOverlay.source_opacity(0) must stick on the scope for assemble_draggable"
            );
        });
    }

    /// Press at threshold 0 must mount the overlay ghost *and* dim/hide the
    /// source in the same chrome pass. A second mousemove used to be required
    /// Press must not apply `source_opacity` / overlay — `0` would erase the
    /// chip before a ghost exists. First movement opens the ghost immediately.
    #[test]
    fn press_keeps_source_first_move_shows_ghost() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, tick, BeginDrag,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-press").threshold(0.0).child(move || {
                let chip = draggable("chip")
                    .item_type("chip")
                    .source_opacity(0.0)
                    .child(|| Element::new().text("src").w(80.0).h(32.0));
                let zone = droppable("inbox")
                    .accept("chip")
                    .child(|| Element::new().w(120.0).h(48.0).text("drop"));
                let overlay: Element = drag_overlay().source_opacity(0.0).into();
                Element::from(vstack().children([chip, zone, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid.clone(),
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(
            !tick(40.0, 16.0, &[], [800.0, 600.0]),
            "press stays pending so source_opacity=0 cannot erase the chip"
        );
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let src = find_draggable(&tree, "chip").expect("source");
            assert!(
                src.style.opacity > 0.5,
                "press must leave the source at rest opacity; opacity={}",
                src.style.opacity
            );
            if let Some(ghost) = find_ghost(&tree) {
                assert!(
                    ghost.layout_bounds[2] < 1.0 && ghost.layout_bounds[3] < 1.0,
                    "ghost must stay an empty host on press; bounds={:?}",
                    ghost.layout_bounds
                );
            }
        }
        assert!(
            tick(41.0, 16.0, &[], [800.0, 600.0]),
            "first movement opens the overlay immediately"
        );
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("overlay host");
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 10.0,
                "ghost must have size on the first drag attempt; bounds={:?}",
                ghost.layout_bounds
            );
            let src = find_draggable(&tree, "chip").expect("source");
            assert!(
                src.style.opacity <= 0.001,
                "source_opacity 0 applies once the ghost is up; opacity={}",
                src.style.opacity
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Custom overlay child: same press / first-move contract as the default clone.
    #[test]
    fn press_without_move_mounts_custom_ghost() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-press-custom").threshold(0.0).child(move || {
                let chip = draggable("chip")
                    .item_type("chip")
                    .source_opacity(0.0)
                    .child(|| Element::new().text("源卡片").w(80.0).h(32.0));
                let overlay: Element = drag_overlay()
                    .source_opacity(0.0)
                    .child(|| Element::new().text("自定义替身").w(80.0).h(32.0))
                    .into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(!tick(40.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let src = find_draggable(&tree, "chip").expect("source");
            assert!(
                src.style.opacity > 0.5,
                "press must keep the custom-demo source visible; opacity={}",
                src.style.opacity
            );
        }
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("custom overlay host");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "自定义替身"),
                "custom ghost must paint on the first drag attempt; texts={texts:?} bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 10.0,
                "custom ghost must have size on first move; bounds={:?}",
                ghost.layout_bounds
            );
            let src = find_draggable(&tree, "chip").expect("source");
            assert!(
                src.style.opacity <= 0.001,
                "source_opacity 0 applies once the ghost is up; opacity={}",
                src.style.opacity
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Production pointerdown holds AppState, so `try_reconcile_subtree` misses.
    /// The ghost must still appear on the first paint walk (no remount, no drain).
    #[test]
    fn activate_paints_ghost_when_reconcile_misses() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-miss").threshold(0.0).child(move || {
                let chip = draggable("chip")
                    .item_type("chip")
                    .source_opacity(0.0)
                    .child(|| Element::new().text("src").w(80.0).h(32.0));
                let overlay: Element = drag_overlay().source_opacity(0.0).into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        clear_test_reconcile_root();
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        install_test_reconcile_root(std::rc::Rc::clone(&live));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("host");
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 10.0,
                "ghost must paint without a reconcile remount; bounds={:?}",
                ghost.layout_bounds
            );
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("src")),
                "ghost body missing after paint-only fill; texts={texts:?}"
            );
            let src = find_draggable(&tree, "chip").expect("source");
            assert!(src.style.opacity <= 0.001);
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery `card_chip`: auto-sized chip (padding / border / text, no w×h).
    /// Page layout measures the empty overlay host first; the ghost must be
    /// `place_drag_ghost(clone)`, not a 0×0 bordered pill grafted onto that host.
    #[test]
    fn gallery_auto_chip_ghost_is_not_a_dot() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-gallery-chip").threshold(0.0).child(move || {
                let chip = draggable("chip").item_type("chip").child(|| {
                    Element::new()
                        .text("可拖卡片")
                        .p(8.0)
                        .rounded(8.0)
                        .border_style("solid")
                        .border_width(1.0)
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "ghost must clone chip text, not an empty bordered shell; texts={texts:?}"
            );
            assert!(
                ghost.layout_bounds[2] > 40.0 && ghost.layout_bounds[3] > 16.0,
                "auto chip must not collapse to a border-radius dot; bounds={:?}",
                ghost.layout_bounds
            );
        }
        let _ = crate::utils::drag::end(&[]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("closed host");
            assert!(
                ghost.style.border_width < 0.01
                    && ghost.layout_bounds[2] < 1.0
                    && ghost.layout_bounds[3] < 1.0,
                "closed host must be an empty placement shell; bounds={:?} border={}",
                ghost.layout_bounds,
                ghost.style.border_width
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery `card_chip`: HStack + Icon + Label, no w×h.
    /// Leaf `.text().p()` staying green does not cover this path — the live
    /// symptom is a bordered radius dot with no chip text.
    #[test]
    fn gallery_hstack_icon_label_ghost_is_not_a_dot() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::{Label, LabelProps};
        use crate::primitives::vstack;
        use crate::ui;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        fn card_chip(label: &str) -> Element {
            ui! {
                <HStack
                    p="0.5rem 0.65rem"
                    gap="0.5rem"
                    items="center"
                    rounded="0.5rem"
                    border_style="solid"
                    border_width="1px"
                    border_color="border"
                    cursor="grab"
                    bg="transparent"
                >
                    <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
                    <Label text_size="0.875rem" color="foreground">{ label.to_string() }</Label>
                </HStack>
            }
        }

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-gallery-card-chip")
                .threshold(0.0)
                .child(move || {
                    let chip = draggable("chip")
                        .item_type("chip")
                        .child(|| card_chip("可拖卡片"));
                    let overlay: Element = drag_overlay().into();
                    Element::from(vstack().children([chip, overlay]))
                })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            let kids = ghost.children.len();
            let [_, _, w, h] = ghost.layout_bounds;
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "hstack+icon+label ghost must clone chip text, not an empty bordered shell; texts={texts:?} kids={kids} bounds={:?} border={}",
                ghost.layout_bounds,
                ghost.style.border_width
            );
            assert!(
                w > 40.0 && h >= 14.0,
                "auto card_chip must hug cloned content, not collapse to a border-radius dot; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                w < 200.0,
                "auto card_chip must not inherit the stretched parent box; bounds={:?}",
                ghost.layout_bounds
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Product path: idle full layout, then `write_placed_ghost` + `build_and_layout_fast`.
    /// `hoist_and_layout` (`LayoutTree::new` + full rebuild) stays green while Gallery
    /// paints a bordered radius dot — the clone has no `layout_node` and fast path
    /// keeps the empty host's 0×0 Taffy box.
    #[test]
    fn gallery_card_chip_ghost_is_not_a_dot_on_appstate_fast_path() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::{Label, LabelProps};
        use crate::primitives::vstack;
        use crate::ui;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        fn card_chip(label: &str) -> Element {
            ui! {
                <HStack
                    p="0.5rem 0.65rem"
                    gap="0.5rem"
                    items="center"
                    rounded="0.5rem"
                    border_style="solid"
                    border_width="1px"
                    border_color="border"
                    cursor="grab"
                    bg="transparent"
                >
                    <Icon name="grip-vertical" size="14px" color="muted-foreground" pointer_events="none" />
                    <Label text_size="0.875rem" color="foreground">{ label.to_string() }</Label>
                </HStack>
            }
        }

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-gallery-fast-chip")
                .threshold(0.0)
                .child(move || {
                    let chip = draggable("chip")
                        .item_type("chip")
                        .child(|| card_chip("可拖卡片"));
                    let overlay: Element = drag_overlay().into();
                    Element::from(vstack().children([chip, overlay]))
                })
        });
        let mut lt = crate::layout::LayoutTree::new();
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout_persisted(&mut lt, &mut tree, false);
            let ghost = find_ghost(&tree).expect("idle host");
            assert!(
                ghost.layout_bounds[2] < 1.0 && ghost.layout_bounds[3] < 1.0,
                "idle overlay host must be an empty 0×0 shell; bounds={:?}",
                ghost.layout_bounds
            );
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout_persisted(&mut lt, &mut tree, true);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            let kids = ghost.children.len();
            let [_, _, w, h] = ghost.layout_bounds;
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "fast-path ghost must clone chip text, not an empty bordered shell; texts={texts:?} kids={kids} bounds={:?} border={}",
                ghost.layout_bounds,
                ghost.style.border_width
            );
            assert!(
                w > 40.0 && h >= 14.0,
                "fast-path auto card_chip must hug cloned content, not collapse to a border-radius dot; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                w < 200.0,
                "fast-path auto card_chip must not inherit the stretched parent box; bounds={:?}",
                ghost.layout_bounds
            );
        }
        let _ = crate::utils::drag::end(&[]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout_persisted(&mut lt, &mut tree, true);
            let ghost = find_ghost(&tree).expect("closed host");
            assert!(
                ghost.style.border_width < 0.01
                    && ghost.layout_bounds[2] < 1.0
                    && ghost.layout_bounds[3] < 1.0,
                "closed host on the fast path must be an empty placement shell; bounds={:?} border={}",
                ghost.layout_bounds,
                ghost.style.border_width
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Always-mounted host: close must un-adopt chip chrome. Clearing children
    /// only leaves a bordered 0-content pill on the portal layer.
    #[test]
    fn overlay_close_restores_empty_shell() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-close-shell").threshold(0.0).child(move || {
                let chip = draggable("chip")
                    .item_type("chip")
                    .source_opacity(0.0)
                    .child(|| {
                        Element::new()
                            .text("src")
                            .w(80.0)
                            .h(32.0)
                            .border_style("solid")
                            .border_width(1.0)
                            .rounded(8.0)
                    });
                let overlay: Element = drag_overlay().source_opacity(0.0).into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("open host");
            assert!(
                ghost.style.border_width > 0.5,
                "open ghost must carry the clone border"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("closed host stays mounted");
            assert!(
                ghost.children.is_empty() && ghost.text.is_none(),
                "closed host must not keep clone children"
            );
            assert!(
                ghost.style.border_width < 0.01,
                "closed host must not keep adopted border (leftover dot); width={}",
                ghost.style.border_width
            );
            assert!(
                ghost.layout_bounds[2] < 1.0 && ghost.layout_bounds[3] < 1.0,
                "closed host must not keep a painted box; bounds={:?}",
                ghost.layout_bounds
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery veto demo: drop into an empty column then drag again. Close
    /// must empty the portal ghost; a second drag must follow `overlay_pos`
    /// (not a frozen hoist-adopt leftover).
    #[test]
    fn overlay_ghost_tracks_pos_after_empty_list_transfer() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, droppable, end,
            last_overlay_pos, sortable_item, start_geometry, tick, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("veto")
                .accept("veto")
                .group("veto")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w("100%").h(32.0))
        }

        let open = Signal::new(vec!["Move me".to_string()]);
        let empty = Signal::new(Vec::<String>::new());

        let (_o, live) = mount_scope_tree({
            let open = open;
            let empty = empty;
            move || {
                drag_scope("ov-empty-xfer")
                    .threshold(0.0)
                    .empty_insert(48.0)
                    .on_drag_end({
                        let open = open;
                        let empty = empty;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "open" {
                                open.peek()
                            } else {
                                empty.peek()
                            };
                            let mut dst = if to == "open" {
                                open.peek()
                            } else {
                                empty.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "open" {
                                open.set(src);
                                empty.set(dst);
                            } else {
                                empty.set(src);
                                open.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let mk = |id: &'static str, files: Signal<Vec<String>>, title| {
                            droppable(id)
                                .list(id)
                                .group("veto")
                                .accept("veto")
                                .child(move || {
                                    let items = files.get();
                                    let rows: Vec<Element> = items
                                        .iter()
                                        .enumerate()
                                        .map(|(i, name)| chip(id, name.clone(), i))
                                        .collect();
                                    Element::from(
                                        vstack()
                                            .w(200.0)
                                            .p(8.0)
                                            .min_h(80.0)
                                            .gap(6.0)
                                            .child(Element::new().text(title).h(20.0))
                                            .child(vstack().w("100%").min_h(0.0).children(rows)),
                                    )
                                })
                        };
                        let overlay: Element = drag_overlay().into();
                        Element::from(hstack().gap(40.0).children([
                            mk("open", open, "Open"),
                            mk("empty", empty, "Empty"),
                            overlay,
                        ]))
                    })
            }
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0.iter().find(|h| h.id == "open:Move me").expect("src");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-empty-xfer",
            "open:Move me",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-empty-xfer".into(),
            draggable_id: "open:Move me".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("open".into()),
            from_index: Some(0),
            item_type: "veto".into(),
            group: "veto".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let empty_hit = hits0.iter().find(|h| h.id == "empty").expect("empty");
        let dest = [
            empty_hit.rect.x + 20.0,
            empty_hit.rect.y + empty_hit.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let mut lt = crate::layout::LayoutTree::new();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout_persisted(&mut lt, &mut tree, false);
            let empty_el = find_droppable(&tree, "empty").expect("empty");
            assert!(
                find_hole_preview(empty_el).is_some(),
                "cross-list empty column must paint a dimmed body"
            );
            let pos_enter = crate::utils::drag::last_overlay_pos("ov-empty-xfer")
                .expect("overlay_pos on enter");
            let ghost_enter = find_ghost(&tree).expect("ghost on enter");
            assert!(
                (ghost_enter.layout_bounds[0] - pos_enter[0]).abs() < 2.0
                    && (ghost_enter.layout_bounds[1] - pos_enter[1]).abs() < 2.0,
                "ghost must still follow overlay_pos when hover first enters the empty column; \
                 bounds={:?} pos={pos_enter:?}",
                ghost_enter.layout_bounds
            );
        }

        let dest_move = [dest[0] + 36.0, dest[1] + 28.0];
        let hits_move = collect_hits(&live.borrow());
        assert!(tick(dest_move[0], dest_move[1], &hits_move, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            lt.set_collect_skip_allowed(true);
            hoist_and_layout_persisted(&mut lt, &mut tree, true);
            let pos = crate::utils::drag::last_overlay_pos("ov-empty-xfer")
                .expect("overlay_pos after move");
            let ghost = find_ghost(&tree).expect("ghost after move");
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 8.0,
                "hovering empty must not collapse the overlay; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                (ghost.layout_bounds[0] - pos[0]).abs() < 2.0
                    && (ghost.layout_bounds[1] - pos[1]).abs() < 2.0,
                "overlay must keep following the pointer over the empty column, not freeze \
                 at first hover; bounds={:?} pos={pos:?}",
                ghost.layout_bounds
            );
        }

        let hits_end = collect_hits(&live.borrow());
        assert!(end(&hits_end));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("closed host stays mounted");
            assert!(
                ghost.children.is_empty() && ghost.text.is_none(),
                "closed host must not keep clone children after empty-list drop"
            );
            assert!(
                ghost.style.border_width < 0.01 && ghost.layout_bounds[2] < 1.0,
                "closed host must not keep a painted box; bounds={:?} border={}",
                ghost.layout_bounds,
                ghost.style.border_width
            );
            let mut n = 0usize;
            crate::core::tree::walk_elements(&tree, |el| {
                if el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
                    n += 1;
                }
            });
            assert_eq!(
                n, 1,
                "must not leak a second frozen ghost on the portal layer"
            );
        }

        let hits2 = collect_hits(&live.borrow());
        let src2 = hits2
            .iter()
            .find(|h| h.id == "empty:Move me")
            .expect("transferred");
        let pointer2 = [
            src2.rect.x + src2.rect.width * 0.5,
            src2.rect.y + src2.rect.height * 0.5,
        ];
        let (grab2, bounds2) = start_geometry(
            "ov-empty-xfer",
            "empty:Move me",
            pointer2,
            [src2.rect.x, src2.rect.y, src2.rect.width, src2.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-empty-xfer".into(),
            draggable_id: "empty:Move me".into(),
            pointer: pointer2,
            grab_offset: grab2,
            start_bounds: bounds2,
            from_list: Some("empty".into()),
            from_index: Some(0),
            item_type: "veto".into(),
            group: "veto".into(),
            capture_id: "cap2".into(),
        });
        let dest2 = [pointer2[0] + 60.0, pointer2[1] + 40.0];
        assert!(tick(dest2[0], dest2[1], &hits2, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let pos = last_overlay_pos("ov-empty-xfer").expect("overlay_pos");
            let ghost = find_ghost(&tree).expect("second-drag ghost");
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 8.0,
                "second drag must rebuild the ghost, not keep a 0×0 leftover; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                (ghost.layout_bounds[0] - pos[0]).abs() < 2.0
                    && (ghost.layout_bounds[1] - pos[1]).abs() < 2.0,
                "second-drag ghost must follow overlay_pos, not freeze at the last drop; \
                 bounds={:?} pos={pos:?}",
                ghost.layout_bounds
            );
        }

        let hits_end2 = collect_hits(&live.borrow());
        let _ = end(&hits_end2);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Paint-time clone is not a tracked effect. Nested islands must inline
    /// their render, not return an empty placeholder.
    #[test]
    fn overlay_clone_inlines_nested_island() {
        use crate::app::reactive::{reactive_view, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-inline-island")
                .threshold(0.0)
                .child(move || {
                    let chip = draggable("chip").item_type("chip").child(|| {
                        reactive_view("ov-inline-island__lbl", || {
                            Element::new().text("岛内文案").w(80.0).h(32.0)
                        })
                    });
                    let overlay: Element = drag_overlay().into();
                    Element::from(vstack().children([chip, overlay]))
                })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("岛内文案")),
                "clone must inline nested reactive_view; texts={texts:?}"
            );
            assert!(
                ghost.layout_bounds[2] > 10.0 && ghost.layout_bounds[3] > 10.0,
                "inlined island must keep size; bounds={:?}",
                ghost.layout_bounds
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Droppable paint chrome (`rui_drag_over`) must flip on the activating press
    /// tick, not the first mousemove.
    #[test]
    fn press_without_move_marks_active_droppable() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, tick, BeginDrag, DragHit,
            ATTR_DRAG_OVER,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-press-drop").threshold(0.0).child(move || {
                let chip = draggable("chip")
                    .item_type("chip")
                    .child(|| Element::new().text("src").w(80.0).h(32.0));
                let zone = droppable("inbox")
                    .accept("chip")
                    .child(|| Element::new().w(200.0).h(80.0).text("drop"));
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, zone, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        let hits = vec![
            DragHit {
                id: "chip".into(),
                scope_id: sid.clone(),
                is_draggable: true,
                is_droppable: false,
                rect: crate::utils::drag::Rect::from_bounds([30.0, 8.0, 80.0, 32.0]),
                item_type: "chip".into(),
                accept: String::new(),
                list_id: None,
                index: None,
                group: String::new(),
                disabled: false,
                list_axis: None,
                skip_shift: false,
                skip_follow: false,
                pull_clone: false,
            },
            DragHit {
                id: "inbox".into(),
                scope_id: sid.clone(),
                is_draggable: false,
                is_droppable: true,
                rect: crate::utils::drag::Rect::from_bounds([0.0, 0.0, 200.0, 80.0]),
                item_type: String::new(),
                accept: "chip".into(),
                list_id: Some("inbox".into()),
                index: None,
                group: String::new(),
                disabled: false,
                list_axis: None,
                skip_shift: false,
                skip_follow: false,
                pull_clone: false,
            },
        ];
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(!tick(40.0, 16.0, &hits, [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox");
            assert_ne!(
                zone.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "droppable chrome must not light on press"
            );
        }
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox");
            assert_eq!(
                zone.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "droppable chrome lights on the first drag attempt"
            );
        }
        let _ = crate::utils::drag::end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn overlay_child_for_sees_active_id() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};
        use std::cell::RefCell;

        let seen = Rc::new(RefCell::new(None::<String>));
        let (_o, live) = mount_scope_tree({
            let seen = Rc::clone(&seen);
            move || {
                let seen = Rc::clone(&seen);
                drag_scope("ov-cf").threshold(0.0).child(move || {
                    let seen = Rc::clone(&seen);
                    let chip =
                        draggable("chip").child(|| Element::new().text("src").w(80.0).h(32.0));
                    let overlay: Element = drag_overlay()
                        .child_for(move |item| {
                            seen.replace(Some(item.id.clone()));
                            Element::new()
                                .text(format!("ghost-{}", item.id))
                                .w(16.0)
                                .h(16.0)
                        })
                        .into();
                    Element::from(vstack().children([chip, overlay]))
                })
            }
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 80.0, 32.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "ghost-chip"),
                "child_for ghost must use active id, got {texts:?}"
            );
        }
        assert_eq!(seen.borrow().as_deref(), Some("chip"));
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn overlay_center_aligns_ghost_to_pointer() {
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag};
        let (_o, (pos, ghost)) = create_root(|| {
            let _el: Element = drag_scope("ov-c")
                .threshold(0.0)
                .child(|| Element::from(drag_overlay().align(OverlayAlign::Center)));
            remember_overlay_ghost_extent("ov-c", [20.0, 10.0]);
            begin(BeginDrag {
                scope_id: "ov-c".into(),
                draggable_id: "card-a".into(),
                pointer: [10.0, 10.0],
                grab_offset: [2.0, 2.0],
                start_bounds: [8.0, 8.0, 40.0, 24.0],
                from_list: Some("list".into()),
                from_index: Some(0),
                item_type: "card".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            remember_overlay_ghost_extent("ov-c", [20.0, 10.0]);
            let hits: [crate::utils::drag::DragHit; 0] = [];
            assert!(tick(30.0, 10.0, &hits, [400.0, 300.0]));
            let pos = super::super::session::peek_overlay_pos("ov-c").unwrap();
            let _ = end(&hits);
            (pos, [20.0_f32, 10.0])
        });
        let ghost_c = [pos[0] + ghost[0] * 0.5, pos[1] + ghost[1] * 0.5];
        let pointer = [30.0_f32, 10.0];
        assert!(
            (ghost_c[0] - pointer[0]).abs() < 0.5 && (ghost_c[1] - pointer[1]).abs() < 0.5,
            "center ghost {ghost_c:?} vs pointer {pointer:?} (origin {pos:?})"
        );
    }

    #[test]
    fn overlay_center_pins_autosize_child_for_to_pointer_after_layout() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-c-auto").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| Element::new().text("src").w(80.0).h(32.0));
                let overlay: Element = drag_overlay()
                    .align(OverlayAlign::Center)
                    .child_for(|_| Element::new().text("g").p(4.0))
                    .into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid,
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let pointer = [70.0_f32, 40.0];
        assert!(tick(pointer[0], pointer[1], &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let [x, y, w, h] = ghost.layout_bounds;
            assert!(
                w > 1.0 && h > 1.0,
                "auto-sized child_for ghost must layout, got {w}x{h}"
            );
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            assert!(
                (cx - pointer[0]).abs() < 0.5 && (cy - pointer[1]).abs() < 0.5,
                "ghost center [{cx}, {cy}] vs pointer {pointer:?} bounds={:?}",
                ghost.layout_bounds
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_pins_viewport_origin_without_point_attr() {
        let el = place_drag_ghost(Element::new().text("card"), "g1", [120.0, 80.0], None);
        assert_eq!(el.attrs.get(ATTR_FLOATING).map(String::as_str), Some("g1"));
        assert_eq!(
            el.attrs.get(ATTR_DRAG_OVERLAY).map(String::as_str),
            Some("1")
        );
        assert!(
            el.attrs.get(ATTR_FLOATING_POINT).is_none(),
            "POINT would let cursor-menu clamp Y to the viewport floor"
        );
        assert!(
            el.attrs.get(ATTR_FLOATING_PLACEMENT).is_none(),
            "PLACEMENT would let sync_floating_placement rewrite inset"
        );
        assert!(
            el.style.position_fixed,
            "fixed: viewport containing block before hoist (ScrollArea host)"
        );
        assert_eq!(
            crate::layout::lpa_length_px(el.style.inset.left),
            Some(120.0)
        );
        assert_eq!(crate::layout::lpa_length_px(el.style.inset.top), Some(80.0));
        assert!(
            crate::layout::dim_length_px(el.style.size.width).is_none(),
            "source drag_extent must not become the overlay width"
        );
        assert!(
            crate::layout::dim_length_px(el.style.size.height).is_none(),
            "source drag_extent must not become the overlay height"
        );
    }

    #[test]
    fn ghost_follow_pins_taffy_collect_patches_abs() {
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag};
        let (_o, _) = crate::core::reactivity::create_root(|| {
            let _el: Element = drag_scope("gf-abs")
                .threshold(0.0)
                .child(|| Element::from(drag_overlay()));
            begin(BeginDrag {
                scope_id: "gf-abs".into(),
                draggable_id: "chip".into(),
                pointer: [40.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [30.0, 8.0, 80.0, 32.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(50.0, 20.0, &[], [800.0, 600.0]));
            let grab = crate::utils::drag::last_overlay_pos("gf-abs").expect("grab origin");
            let mut ghost = place_drag_ghost(
                Element::new().w(80.0).h(32.0).text("g"),
                "gf-abs__ghost",
                grab,
                None,
            );
            ghost.layout_bounds = [grab[0], grab[1], 80.0, 32.0];
            sync_overlay_ghost_insets(&mut ghost);
            assert!(tick(90.0, 70.0, &[], [800.0, 600.0]));
            let visual = crate::utils::drag::last_overlay_pos("gf-abs").expect("live origin");
            assert_ne!(visual, grab, "pointer moved");
            let patched = adjust_overlay_collect_origin(&mut ghost, [80.0, 32.0]);
            assert_eq!(
                patched,
                Some(visual),
                "collect must patch abs so occupancy / children follow"
            );
            assert_eq!(
                crate::layout::lpa_length_px(ghost.style.inset.left),
                Some(grab[0]),
                "Taffy inset stays at grab origin during drag"
            );
            assert_eq!(
                crate::layout::lpa_length_px(ghost.style.inset.top),
                Some(grab[1])
            );
            assert_eq!(ghost.style.translate, [0.0, 0.0]);
            let _ = end(&[]);
        });
    }

    #[test]
    fn ghost_follow_tick_queues_overlay_damage_before_render() {
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag};
        let (_o, _) = crate::core::reactivity::create_root(|| {
            let _el: Element = drag_scope("gf-tick")
                .threshold(0.0)
                .child(|| Element::from(drag_overlay()));
            begin(BeginDrag {
                scope_id: "gf-tick".into(),
                draggable_id: "chip".into(),
                pointer: [40.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [30.0, 8.0, 80.0, 32.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(50.0, 20.0, &[], [800.0, 600.0]));
            let grab = crate::utils::drag::last_overlay_pos("gf-tick").expect("grab origin");
            let mut ghost =
                place_drag_ghost(Element::new().w(80.0).h(32.0), "gf-tick__ghost", grab, None);
            ghost.layout_bounds = [grab[0], grab[1], 80.0, 32.0];
            sync_overlay_ghost_insets(&mut ghost);
            let _ = crate::app::damage::take_frame_paint_nodes();
            assert!(tick(90.0, 70.0, &[], [800.0, 600.0]));
            let visual = crate::utils::drag::last_overlay_pos("gf-tick").expect("live origin");
            let dirties = crate::app::damage::take_frame_paint_nodes();
            assert!(
                dirties.iter().any(|d| {
                    d.kind == crate::app::damage::PaintDirtyKind::Overlay
                        && d.bounds[0] <= visual[0] + 0.5
                        && d.bounds[1] <= visual[1] + 0.5
                        && d.bounds[0] + d.bounds[2] >= visual[0] + 80.0 - 0.5
                        && d.bounds[1] + d.bounds[3] >= visual[1] + 32.0 - 0.5
                }),
                "CursorMoved must dest-reset the live origin (same-key union ok); got {dirties:?}"
            );
            let _ = end(&[]);
        });
    }

    #[test]
    fn ghost_clip_shell_wraps_and_offsets_inner() {
        let el = place_drag_ghost(
            Element::new().w(40.0).h(24.0).text("card"),
            "g1",
            [80.0, 40.0],
            Some([10.0, 20.0, 200.0, 120.0]),
        );
        assert_eq!(
            el.attrs.get(ATTR_DRAG_OVERLAY_CLIP).map(String::as_str),
            Some("1")
        );
        assert!(el.style.position_fixed);
        assert_eq!(
            crate::layout::lpa_length_px(el.style.inset.left),
            Some(10.0)
        );
        assert_eq!(crate::layout::lpa_length_px(el.style.inset.top), Some(20.0));
        assert_eq!(
            crate::layout::dim_length_px(el.style.size.width),
            Some(200.0)
        );
        assert_eq!(
            crate::layout::dim_length_px(el.style.size.height),
            Some(120.0)
        );
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(el.style.overflow.y, taffy::style::Overflow::Hidden);
        let inner = el.children.first().expect("clip shell wraps the ghost");
        assert!(inner.attrs.get(ATTR_DRAG_OVERLAY).is_none());
        assert_eq!(
            inner.attrs.get(ATTR_DRAG_OVERLAY_INNER).map(String::as_str),
            Some("g1")
        );
        assert!(!inner.style.position_fixed);
        assert_eq!(
            crate::layout::lpa_length_px(inner.style.inset.left),
            Some(70.0)
        );
        assert_eq!(
            crate::layout::lpa_length_px(inner.style.inset.top),
            Some(20.0)
        );
    }

    #[test]
    fn ghost_clip_inner_collect_patches_abs() {
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag};
        let (_o, _) = crate::core::reactivity::create_root(|| {
            let _el: Element = drag_scope("gf-clip")
                .threshold(0.0)
                .child(|| Element::from(drag_overlay()));
            begin(BeginDrag {
                scope_id: "gf-clip".into(),
                draggable_id: "chip".into(),
                pointer: [40.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [30.0, 8.0, 80.0, 32.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(50.0, 20.0, &[], [800.0, 600.0]));
            let grab = crate::utils::drag::last_overlay_pos("gf-clip").expect("grab origin");
            let bbox = [10.0_f32, 20.0, 400.0, 240.0];
            let mut shell = place_drag_ghost(
                Element::new().w(80.0).h(32.0).text("g"),
                "gf-clip__ghost",
                grab,
                Some(bbox),
            );
            assert!(adjust_overlay_collect_origin(&mut shell, [400.0, 240.0]).is_none());
            assert!(tick(90.0, 70.0, &[], [800.0, 600.0]));
            let visual = crate::utils::drag::last_overlay_pos("gf-clip").expect("live origin");
            assert_ne!(visual, grab, "pointer moved");
            let inner = shell.children.first_mut().expect("clip inner");
            let patched = adjust_overlay_collect_origin(inner, [80.0, 32.0]);
            assert_eq!(
                patched,
                Some(visual),
                "clip inner collect must patch viewport abs so the chip follows"
            );
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.left),
                Some(bbox[0]),
                "clip shell stays on the restrict box"
            );
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.top),
                Some(bbox[1])
            );
            let _ = end(&[]);
        });
    }

    #[test]
    fn ghost_clip_shell_collect_tracks_scrolled_restrict_box() {
        use crate::utils::drag::geometry::Rect;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag, DragHit};
        let (_o, _) = crate::core::reactivity::create_root(|| {
            let _el: Element = drag_scope("gf-clip-scroll")
                .threshold(0.0)
                .restrict("arena")
                .restrict_overflow("hidden")
                .child(|| Element::from(drag_overlay()));
            fn hit(id: &str, bounds: [f32; 4], drop: bool) -> DragHit {
                DragHit {
                    id: id.into(),
                    scope_id: "gf-clip-scroll".into(),
                    is_draggable: !drop,
                    is_droppable: drop,
                    rect: Rect::from_bounds(bounds),
                    item_type: "chip".into(),
                    accept: String::new(),
                    list_id: None,
                    index: None,
                    group: String::new(),
                    disabled: false,
                    list_axis: None,
                    skip_shift: false,
                    skip_follow: false,
                    pull_clone: false,
                }
            }
            let card = hit("chip", [80.0, 40.0, 40.0, 24.0], false);
            let arena = hit("arena", [10.0, 20.0, 400.0, 240.0], true);
            begin(BeginDrag {
                scope_id: "gf-clip-scroll".into(),
                draggable_id: "chip".into(),
                pointer: [100.0, 52.0],
                grab_offset: [20.0, 12.0],
                start_bounds: [80.0, 40.0, 40.0, 24.0],
                from_list: None,
                from_index: None,
                item_type: "chip".into(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            assert!(tick(
                110.0,
                62.0,
                &[card.clone(), arena.clone()],
                [800.0, 600.0]
            ));
            let grab = crate::utils::drag::last_overlay_pos("gf-clip-scroll").expect("grab");
            let bbox0 = peek_restrict_clip().expect("hidden clip");
            let mut shell = place_drag_ghost(
                Element::new().w(80.0).h(32.0).text("g"),
                "gf-clip-scroll__ghost",
                grab,
                Some(bbox0),
            );
            shell.layout_bounds = bbox0;
            assert_eq!(
                adjust_overlay_collect_origin(&mut shell, [bbox0[2], bbox0[3]]),
                Some([bbox0[0], bbox0[1]]),
                "clip shell collect must patch to the live restrict origin"
            );

            let arena_scrolled = hit("arena", [10.0, -60.0, 400.0, 240.0], true);
            let mut card_scrolled = card.clone();
            card_scrolled.rect = Rect::from_bounds([80.0, -40.0, 40.0, 24.0]);
            assert!(tick(
                110.0,
                62.0,
                &[card_scrolled, arena_scrolled],
                [800.0, 600.0]
            ));
            let bbox1 = peek_restrict_clip().expect("scrolled clip");
            assert!(
                (bbox1[1] + 60.0).abs() < 0.5,
                "clip TLS must follow the scrolled arena, got {bbox1:?}"
            );
            let visual = crate::utils::drag::last_overlay_pos("gf-clip-scroll").expect("live");
            sync_overlay_ghost_insets(&mut shell);
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.left),
                Some(bbox1[0]),
                "clip shell must stay on the restrict box, not the chip"
            );
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.top),
                Some(bbox1[1]),
                "clip shell y must track the scrolled parent, not the ghost origin {visual:?}"
            );
            assert_eq!(
                adjust_overlay_collect_origin(&mut shell, [bbox1[2], bbox1[3]]),
                Some([bbox1[0], bbox1[1]])
            );
            let inner = shell.children.first_mut().expect("clip inner");
            assert_eq!(
                adjust_overlay_collect_origin(inner, [80.0, 32.0]),
                Some(visual),
                "inner still patches viewport abs so the chip follows"
            );

            assert!(tick(110.0, 62.0, &[card, arena], [800.0, 600.0]));
            let bbox2 = peek_restrict_clip().expect("restored clip");
            sync_overlay_ghost_insets(&mut shell);
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.top),
                Some(bbox2[1]),
                "scroll-back must restore the clip shell, not leave a leftover origin"
            );
            assert_ne!(
                crate::layout::lpa_length_px(shell.style.inset.top),
                Some(visual[1]),
                "shell must not have been written to the chip origin"
            );
            let _ = end(&[]);
        });
    }

    #[test]
    fn ghost_clip_shell_keeps_restrict_box_when_clip_lookup_misses() {
        use crate::utils::drag::{begin, drag_overlay, drag_scope, end, tick, BeginDrag};
        let (_o, _) = crate::core::reactivity::create_root(|| {
            let _el: Element = drag_scope("gf-clip-keep")
                .threshold(0.0)
                .restrict("arena")
                .restrict_overflow("hidden")
                .child(|| Element::from(drag_overlay()));
            begin(BeginDrag {
                scope_id: "gf-clip-keep".into(),
                draggable_id: "chip".into(),
                pointer: [40.0, 16.0],
                grab_offset: [10.0, 8.0],
                start_bounds: [30.0, 8.0, 80.0, 32.0],
                from_list: None,
                from_index: None,
                item_type: String::new(),
                group: String::new(),
                capture_id: "cap".into(),
            });
            let bbox = [10.0_f32, 20.0, 400.0, 240.0];
            let mut shell = place_drag_ghost(
                Element::new().w(80.0).h(32.0).text("g"),
                "gf-clip-keep__ghost",
                [80.0, 40.0],
                Some(bbox),
            );
            assert!(tick(90.0, 70.0, &[], [800.0, 600.0]));
            let visual = crate::utils::drag::last_overlay_pos("gf-clip-keep").expect("live");
            sync_overlay_ghost_insets(&mut shell);
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.left),
                Some(bbox[0]),
                "missing clip TLS must not move the shell to the chip"
            );
            assert_eq!(
                crate::layout::lpa_length_px(shell.style.inset.top),
                Some(bbox[1])
            );
            assert_ne!(
                crate::layout::lpa_length_px(shell.style.inset.left),
                Some(visual[0])
            );
            let _ = end(&[]);
        });
    }

    #[test]
    fn rotated_ghost_clips_to_restrict_box_in_world() {
        let mut inner = Element::new()
            .w(80.0)
            .h(80.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("溢出隐藏");
        inner.style.rotate_deg = -135.0;
        let panel = place_drag_ghost(inner, "g1", [20.0, 80.0], Some([10.0, 20.0, 200.0, 120.0]));
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 902));
        assert!(
            crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1,
            "ghost must hoist"
        );
        let mut tree = crate::layout::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let fill_i = result
            .instances
            .iter()
            .position(|i| i.color[0] > 0.9 && i.rect_size[0] > 60.0)
            .expect("rotated ghost fill");
        let fill = &result.instances[fill_i];
        assert!(
            fill.clip_size[0] < 0.0 || fill.clip_size[1] < 0.0,
            "rotated ghost must not local-AABB clip (chops internals), clip_size={:?}",
            fill.clip_size
        );
        let poly = result.clip_polys[fill_i];
        assert!(
            poly.is_active(),
            "restrict overflow:hidden shell must stamp a world clip poly"
        );
        let cx = fill.rect_pos[0] + fill.rect_size[0] * 0.5;
        let cy = fill.rect_pos[1] + fill.rect_size[1] * 0.5;
        let world = |lx: f32, ly: f32| -> [f32; 2] {
            let (rx, ry) = crate::style::mat2d_transform_vec(&fill.transform, lx, ly);
            [cx + rx + fill.transform[4], cy + ry + fill.transform[5]]
        };
        let inside = world(0.0, 35.0);
        assert!(
            inside[0] >= 10.0 && inside[0] < 210.0 && inside[1] >= 20.0 && inside[1] < 140.0,
            "fixture: local (0,35) must land inside the restrict box, got {inside:?}"
        );
        assert!(
            poly.contains_point(inside),
            "world clip must keep ink still inside the restrict box (not a local diagonal chop), p={inside:?}"
        );
        let leaked = world(-40.0, -40.0);
        assert!(
            leaked[1] >= 140.0 || leaked[0] < 10.0 || leaked[0] >= 210.0,
            "fixture: rotated TL must leave the restrict box, got {leaked:?}"
        );
        assert!(
            !poly.contains_point(leaked),
            "world clip must hide the rotated corner outside the dashed box, p={leaked:?}"
        );
        for d in &result.textured_draws {
            if d.clip_size[0] < 0.0 || d.clip_size[1] < 0.0 {
                continue;
            }
            assert!(
                crate::renderer::overflow_clip_contains_world(d.clip_pos, d.clip_size, inside),
                "rotated bake must world-clip to the restrict AABB, not local; p={inside:?} clip={:?}",
                (d.clip_pos, d.clip_size)
            );
            assert!(
                !crate::renderer::overflow_clip_contains_world(d.clip_pos, d.clip_size, leaked),
                "rotated bake must discard ink outside the restrict box, p={leaked:?}"
            );
        }
    }

    #[test]
    fn rotated_hidden_ghost_does_not_paint_above_ancestor_clip() {
        let mut inner = Element::new()
            .w(80.0)
            .h(80.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("溢出隐藏");
        inner.style.rotate_deg = -135.0;
        // Shell already matches the visible parent strip (header already clipped).
        let panel = place_drag_ghost(inner, "g1", [20.0, 90.0], Some([10.0, 80.0, 200.0, 180.0]));
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 903));
        assert!(crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = crate::layout::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let fill_i = result
            .instances
            .iter()
            .position(|i| i.color[0] > 0.9 && i.rect_size[0] > 60.0)
            .expect("rotated ghost fill");
        let poly = result.clip_polys[fill_i];
        assert!(poly.is_active(), "hidden shell must stamp a world clip");
        let header = [40.0_f32, 40.0];
        assert!(
            !poly.contains_point(header),
            "portal ghost must not paint in the page header above the clipped parent, p={header:?}"
        );
    }

    #[test]
    fn hidden_ghost_ancestor_scissor_keeps_parent_left_edge() {
        let mut inner = Element::new()
            .w(80.0)
            .h(80.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("溢出隐藏");
        inner.style.rotate_deg = -135.0;
        let panel = place_drag_ghost(inner, "g1", [20.0, 90.0], Some([10.0, 20.0, 200.0, 240.0]));
        crate::utils::drag::session::set_restrict_ancestor_clip(Some([0.0, 80.0, 400.0, 200.0]));
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 904));
        assert!(crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = crate::layout::LayoutTree::new();
        let result = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        crate::utils::drag::session::set_restrict_ancestor_clip(None);
        let fill_i = result
            .instances
            .iter()
            .position(|i| i.color[0] > 0.9 && i.rect_size[0] > 60.0)
            .expect("rotated ghost fill");
        let poly = result.clip_polys[fill_i];
        assert!(poly.is_active(), "hidden shell must stamp a world clip");
        assert!(
            !poly.contains_point([40.0, 40.0]),
            "must not paint in the header above the scroll viewport"
        );
        assert!(
            !poly.contains_point([5.0, 120.0]),
            "must not paint in the padding left of the dashed parent"
        );
        assert!(
            poly.contains_point([100.0, 120.0]),
            "must still paint inside the visible arena"
        );
    }

    fn find_ghost(el: &Element) -> Option<&Element> {
        if el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
            return Some(el);
        }
        el.children.iter().find_map(find_ghost)
    }

    fn find_hole_preview(el: &Element) -> Option<&Element> {
        if el
            .attrs
            .contains_key(crate::utils::drag::ATTR_DRAG_HOLE_PREVIEW)
        {
            return Some(el);
        }
        el.children.iter().find_map(find_hole_preview)
    }

    /// Hole preview origin is paint `translate` (inset stays 0).
    fn preview_visual_y(el: &Element) -> f32 {
        el.layout_bounds[1] + el.style.translate[1]
    }

    fn collect_tids(el: &Element, out: &mut Vec<String>) {
        if let Some(id) = &el.template_id {
            out.push(id.clone());
        }
        for c in &el.children {
            collect_tids(c, out);
        }
    }

    fn collect_texts(el: &Element, out: &mut Vec<String>) {
        if let Some(t) = &el.text {
            if !t.is_empty() {
                out.push(t.clone());
            }
        }
        for c in &el.children {
            collect_texts(c, out);
        }
    }

    fn find_scope_id(el: &Element) -> Option<String> {
        el.attrs
            .get(crate::utils::drag::ATTR_DRAG_SCOPE)
            .cloned()
            .or_else(|| el.children.iter().find_map(find_scope_id))
    }

    fn ghost_is_under_portal_layer(el: &Element) -> bool {
        fn walk(el: &Element, under_layer: bool) -> bool {
            let here = under_layer
                || el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
            if here && el.attrs.get(ATTR_DRAG_OVERLAY).is_some() {
                return true;
            }
            el.children.iter().any(|c| walk(c, here))
        }
        walk(el, false)
    }

    /// Gallery path: section `isolate_reads` → DragProvider + DragOverlay island →
    /// `begin`/`tick` → reconcile + hoist. Asserts the user-visible symptom (ghost on
    /// the portal layer at `overlay_pos`), not a helper in isolation.
    #[test]
    fn gallery_path_ghost_is_on_portal_layer_after_activate() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, isolate_reads};
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, overlay_is_registered, tick, BeginDrag,
        };
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();

        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));

        let (_o, _) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                let el = isolate_reads(|| {
                    drag_scope("ov-loop").threshold(0.0).child(|| {
                        let chip = draggable("chip").child(|| {
                            Element::new()
                                .text("card")
                                .w(80.0)
                                .h(32.0)
                                .bg([0.9, 0.9, 0.9, 1.0])
                        });
                        let overlay = drag_overlay()
                            .child(|| Element::new().text("ghost").bg([1.0, 0.0, 0.0, 1.0]));
                        Element::from(vstack().children([chip, overlay.into()]))
                    })
                });
                *live_e.borrow_mut() = el;
            });
        });
        drain_deferred_view_remounts();

        let sid = find_scope_id(&live.borrow()).expect("drag scope attr");
        assert!(
            overlay_is_registered(&sid),
            "DragOverlay From must set overlay_registered before activate; sid={sid}"
        );

        begin(BeginDrag {
            scope_id: sid.clone(),
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        // origin = start + delta - grab = [40+10-10, 16+4-8] = [40, 12]
        assert!(
            tick(50.0, 20.0, &[], [800.0, 600.0]),
            "threshold 0 must activate"
        );
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let mut lt = crate::layout::LayoutTree::new();
            let _ = lt.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
            let mut tids = Vec::new();
            collect_tids(&tree, &mut tids);
            let ghost = find_ghost(&tree);
            assert!(
                ghost.is_some(),
                "ghost missing from tree after activate (overlay island did not mount). tids={tids:?}"
            );
            assert!(
                ghost_is_under_portal_layer(&tree),
                "ghost is in the tree but was not hoisted out of the 0×0 mount (ScrollArea would clip it). tids={tids:?}"
            );
            let g = ghost.unwrap();
            assert!(
                g.layout_bounds[2] > 10.0 && g.layout_bounds[3] > 10.0,
                "ghost hoisted but 0-sized: {:?}",
                g.layout_bounds
            );
            assert!(
                (g.layout_bounds[0] - 40.0).abs() < 2.0,
                "ghost x not following overlay_pos; bounds={:?}",
                g.layout_bounds
            );
            assert!(
                (g.layout_bounds[1] - 12.0).abs() < 2.0,
                "ghost y not following overlay_pos; bounds={:?}",
                g.layout_bounds
            );
        }

        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn mount_scope_tree(
        build: impl Fn() -> Element + 'static,
    ) -> (
        crate::core::reactivity::OwnerHandle,
        std::rc::Rc<std::cell::RefCell<Element>>,
    ) {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
        use crate::core::reactivity::{create_effect, create_root, isolate_reads};
        use std::cell::RefCell;
        use std::rc::Rc;

        reset_registered_anchors();
        clear_test_reconcile_root();
        let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
        install_test_reconcile_root(Rc::clone(&live));
        let (owner, _) = create_root(|| {
            let live_e = Rc::clone(&live);
            create_effect(move || {
                *live_e.borrow_mut() = isolate_reads(|| build());
            });
        });
        drain_deferred_view_remounts();
        (owner, live)
    }

    fn find_droppable<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DROPPABLE)
            .map(String::as_str)
            == Some(id)
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_droppable(c, id))
    }

    fn find_draggable<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
            .map(String::as_str)
            == Some(id)
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_draggable(c, id))
    }

    fn find_item_shell<'a>(el: &'a Element, id: &str) -> Option<&'a Element> {
        if let Some(inner) = el.children.iter().find_map(|c| find_item_shell(c, id)) {
            return Some(inner);
        }
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_ITEM_SHELL)
            .is_some()
            && find_draggable(el, id).is_some()
        {
            return Some(el);
        }
        None
    }

    fn shell_block_collapsed(el: &Element) -> bool {
        use crate::layout::{dim_kind, DimKind};
        let max_h = dim_kind(el.style.max_size.height);
        let size_h = dim_kind(el.style.size.height);
        matches!(max_h, DimKind::Length(v) if v < 0.5)
            || matches!(size_h, DimKind::Length(v) if v < 0.5)
    }

    fn find_draggable_in_scope<'a>(el: &'a Element, scope: &str, id: &str) -> Option<&'a Element> {
        let id_ok = el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
            .map(String::as_str)
            == Some(id);
        let scope_ok = el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_SCOPE)
            .map(String::as_str)
            == Some(scope);
        if id_ok && scope_ok {
            return Some(el);
        }
        el.children
            .iter()
            .find_map(|c| find_draggable_in_scope(c, scope, id))
    }

    /// Gallery park: source `<Show>` unmounts `chip` and the droppable mounts
    /// `chip:inbox`. Kanban uses the same location-scoped id so the parked
    /// island is new and actually paints inside the zone.
    #[test]
    fn park_location_scoped_id_paints_inside_droppable() {
        use crate::app::reactive::{reset_registered_anchors, show_reactive};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::hstack;
        use crate::utils::drag::{drag_scope, draggable, droppable};

        let parked = Signal::new(false);
        let (_o, live) = mount_scope_tree({
            let parked = parked;
            move || {
                drag_scope("ov-park-scoped").child(move || {
                    let src = show_reactive(
                        "ov-park-scoped-src",
                        move || !parked.get(),
                        || {
                            draggable("chip")
                                .item_type("chip")
                                .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0))
                        },
                    );
                    let zone = droppable("inbox").accept("chip").child({
                        let parked = parked;
                        move || {
                            if parked.get() {
                                draggable("chip:inbox")
                                    .item_type("chip")
                                    .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0))
                            } else {
                                Element::new().text("放到这里")
                            }
                        }
                    });
                    Element::from(hstack().children([src, zone]))
                })
            }
        });
        parked.set(true);
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "location-scoped parked id must paint inside the droppable, got {texts:?}"
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery park row: auto-width chip + Droppable in a wrap HStack.
    /// A `100%`×`100%` droppable shell wraps and collapses; the zone must keep
    /// the author min-size so the dashed container stays visible.
    #[test]
    fn droppable_zone_keeps_min_size_beside_auto_chip() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::{boxed, hstack};
        use crate::utils::drag::{drag_scope, draggable, droppable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-zone-size").child(|| {
                let src = draggable("chip")
                    .item_type("chip")
                    .child(|| Element::new().text("可拖卡片").w(80.0).h(32.0));
                let zone = droppable("inbox").accept("chip").child(|| {
                    Element::from(
                        boxed()
                            .min_h(72.0)
                            .min_w(128.0)
                            .flex_grow(1.0)
                            .child(Element::new().text("放到这里")),
                    )
                });
                Element::from(
                    hstack()
                        .w("100%")
                        .items("stretch")
                        .wrap("wrap")
                        .gap(12.0)
                        .children([src, zone]),
                )
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let zone = find_droppable(&tree, "inbox").expect("inbox zone");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("放到这里")),
                "droppable body must mount without a hover rebuild, got {texts:?}"
            );
            assert!(
                zone.layout_bounds[2] >= 120.0,
                "droppable must not shrink to a sliver beside the chip; w={}",
                zone.layout_bounds[2]
            );
            assert!(
                zone.layout_bounds[3] >= 70.0,
                "droppable must keep author min-height; h={}",
                zone.layout_bounds[3]
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery: park into a Droppable, then drag that parked island and drop
    /// on the same zone. Overlay dim / empty placeholder must not stick.
    ///
    /// Do **not** hoist+layout repeatedly here — Windows debug stacks overflow
    /// (same class as the deleted scrolled-host ghost test).
    #[test]
    fn parked_item_restores_style_after_redrop_in_same_zone() {
        use crate::app::reactive::{reset_registered_anchors, show_reactive};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::hstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, end, tick, BeginDrag,
        };

        let parked = Signal::new(false);
        let (_o, live) = mount_scope_tree({
            let parked = parked;
            move || {
                drag_scope("ov-park-redrop").threshold(0.0).child(move || {
                    let src = show_reactive(
                        "ov-park-redrop-src",
                        move || !parked.get(),
                        || {
                            draggable("chip").item_type("chip").child(|| {
                                Element::new()
                                    .text("可拖卡片")
                                    .w(80.0)
                                    .h(32.0)
                                    .bg([0.2, 0.3, 0.8, 1.0])
                            })
                        },
                    );
                    let zone = droppable("inbox").accept("chip").child({
                        let parked = parked;
                        move || {
                            if parked.get() {
                                draggable("chip:inbox").item_type("chip").child(|| {
                                    Element::new()
                                        .text("可拖卡片")
                                        .w(80.0)
                                        .h(32.0)
                                        .bg([0.2, 0.3, 0.8, 1.0])
                                })
                            } else {
                                Element::new().text("放到这里")
                            }
                        }
                    });
                    let overlay: Element = drag_overlay().source_opacity(0.0).into();
                    Element::from(hstack().children([src, zone, overlay]))
                })
            }
        });
        parked.set(true);
        crate::app::reactive::drain_deferred_view_remounts();
        begin(BeginDrag {
            scope_id: "ov-park-redrop".into(),
            draggable_id: "chip:inbox".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: "chip".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let inbox = crate::utils::drag::DragHit {
            id: "inbox".into(),
            scope_id: "ov-park-redrop".into(),
            is_draggable: false,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, 0.0, 200.0, 80.0]),
            item_type: String::new(),
            accept: "chip".into(),
            list_id: Some("inbox".into()),
            index: None,
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let hits = [inbox];
        assert!(tick(50.0, 20.0, &hits, [800.0, 600.0]));
        crate::app::reactive::drain_deferred_view_remounts();
        assert!(end(&hits));
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let zone = find_droppable(&tree, "inbox").expect("inbox after redrop");
            let mut texts = Vec::new();
            collect_texts(zone, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("可拖卡片")),
                "parked chip must still paint inside the droppable after redrop, got {texts:?}"
            );
            let item = find_draggable(&tree, "chip:inbox").expect("parked chip after redrop");
            assert!(
                (item.style.opacity - 1.0).abs() < 0.05,
                "opacity-0 dim must clear after dropping on the same zone; opacity={}",
                item.style.opacity
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn activate_chip(
        live: &std::cell::RefCell<Element>,
        sid: &str,
        pointer: [f32; 2],
        grab: [f32; 2],
        bounds: [f32; 4],
    ) {
        use crate::utils::drag::{begin, tick, BeginDrag};
        begin(BeginDrag {
            scope_id: sid.into(),
            draggable_id: "chip".into(),
            pointer,
            grab_offset: grab,
            start_bounds: bounds,
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(
            pointer[0] + 10.0,
            pointer[1] + 4.0,
            &[],
            [800.0, 600.0]
        ));
        // Same order as AppState paint: chrome (fills the always-mounted host) then layout.
        let mut tree = live.borrow_mut();
        sync_drag_chrome(&mut tree);
    }

    fn hoist_and_layout(tree: &mut Element) {
        crate::core::reactivity::with_ui_build_stack(|| {
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(tree);
            let mut lt = crate::layout::LayoutTree::new();
            let _ = lt.build_and_layout(tree, 800.0, 600.0, [0.0, 0.0]);
        });
    }

    /// Same `LayoutTree` across frames — Gallery paint never does `LayoutTree::new()`
    /// after idle. `fast` is AppState's `LayoutChanged` path (`build_and_layout_fast`).
    fn hoist_and_layout_persisted(
        lt: &mut crate::layout::LayoutTree,
        tree: &mut Element,
        fast: bool,
    ) {
        crate::core::reactivity::with_ui_build_stack(|| {
            if fast {
                let _ = lt.build_and_layout_fast(tree, 800.0, 600.0, [0.0, 0.0], false, false);
            } else {
                let _ = crate::layout::layout_tree::hoist_portal_subtrees(tree);
                let _ = lt.build_and_layout(tree, 800.0, 600.0, [0.0, 0.0]);
            }
        });
    }

    /// Empty `<DragOverlay>` must render the source item's text, not a dummy
    /// placeholder like Gallery's old `card_chip("handled")`.
    #[test]
    fn ghost_follows_source_text_by_default() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-follow").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("拖左侧手柄；点文字不会启动")
                        .w(200.0)
                        .h(40.0)
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 200.0, 40.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("拖左侧手柄")),
                "ghost must follow source text, got {texts:?}"
            );
            assert!(
                texts.iter().all(|t| t != "handled"),
                "ghost leaked placeholder text: {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Nested SortableItem body re-runs inside the overlay clone. Inner
    /// `assemble_draggable` must not reuse the live `reactive_view` tid
    /// (Gallery folder ghosts otherwise collapse to the header chip).
    #[test]
    fn overlay_clone_of_nested_sortable_keeps_inner_live_island() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, sortable_item, sortable_list, tick, BeginDrag,
        };

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-nest-clone").threshold(0.0).child(|| {
                let folder = sortable_item("folder:Later")
                    .item_type("folder")
                    .list("tree-root")
                    .index(0)
                    .child(|| {
                        let inner = sortable_list("tree-later").accept("file").child(|| {
                            sortable_item("tree-later:Gamma")
                                .item_type("file")
                                .list("tree-later")
                                .index(0)
                                .child(|| Element::new().text("Gamma").w(80.0).h(24.0))
                        });
                        Element::from(
                            vstack()
                                .w("100%")
                                .child(Element::new().text("Later"))
                                .child(inner),
                        )
                    });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([folder, overlay]))
            })
        });
        {
            let tree = live.borrow();
            let gamma = find_draggable(&tree, "tree-later:Gamma").expect("live Gamma before drag");
            let mut texts = Vec::new();
            collect_texts(gamma, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("Gamma")),
                "live inner chip must paint before clone, got {texts:?}"
            );
        }
        begin(BeginDrag {
            scope_id: "ov-nest-clone".into(),
            draggable_id: "folder:Later".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 200.0, 80.0],
            from_list: Some("tree-root".into()),
            from_index: Some(0),
            item_type: "folder".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(50.0, 20.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            hoist_and_layout(&mut tree);
            let gamma = find_draggable(&tree, "tree-later:Gamma")
                .expect("clone must not steal the live inner island");
            let mut live_texts = Vec::new();
            collect_texts(gamma, &mut live_texts);
            assert!(
                live_texts.iter().any(|t| t.contains("Gamma")),
                "live Gamma text missing after clone, got {live_texts:?}"
            );
            let ghost = find_ghost(&tree).expect("ghost");
            let mut ghost_texts = Vec::new();
            collect_texts(ghost, &mut ghost_texts);
            assert!(
                ghost_texts.iter().any(|t| t.contains("Later")),
                "ghost must include the folder title, got {ghost_texts:?}"
            );
            assert!(
                ghost_texts.iter().any(|t| t.contains("Gamma")),
                "ghost must clone nested list body, not a header-only chip; got {ghost_texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_default_clone_is_undimmed_author_body() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-undimmed").threshold(0.0).child(|| {
                let chip = draggable("chip")
                    .child(|| Element::new().text("本体文案").w(80.0).h(32.0).opacity(1.0));
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 80.0, 32.0],
        );
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.style.opacity - 1.0).abs() < 0.01,
                "default overlay must clone the undimmed author body; got {}",
                ghost.style.opacity
            );
            let source = find_draggable(&tree, "chip").expect("source");
            assert!(
                (source.style.opacity - 0.4).abs() < 0.05,
                "in-list source is what the kit dims; opacity={}",
                source.style.opacity
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Paint chrome runs every frame while the overlay is open. Snapshot must
    /// stay the resting author body — `opacity > 0.05` used to recapture 0.4.
    #[test]
    fn source_snapshot_ignores_live_source_opacity() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{begin, drag_overlay, drag_scope, draggable, tick, BeginDrag};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-snap-undim").threshold(0.0).child(|| {
                let chip = draggable("chip")
                    .child(|| Element::new().text("本体文案").w(80.0).h(32.0).opacity(1.0));
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        begin(BeginDrag {
            scope_id: sid.clone(),
            draggable_id: "chip".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [30.0, 8.0, 80.0, 32.0],
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(41.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let source = find_draggable(&tree, "chip").expect("source");
            assert!(
                (source.style.opacity - 0.4).abs() < 0.05,
                "live source is dimmed; opacity={}",
                source.style.opacity
            );
            // Second paint: the old `opacity > 0.05` guard recaptured 0.4.
            sync_drag_chrome(&mut tree);
            let clone = super::clone_source_preview(&sid, "chip").expect("snapshot");
            assert!(
                (clone.style.opacity - 1.0).abs() < 0.01,
                "ghost snapshot must stay the undimmed author body; got {}",
                clone.style.opacity
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_author_style_overrides_source_opacity() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-style").threshold(0.0).child(|| {
                let chip = draggable("chip")
                    .child(|| Element::new().text("本体文案").w(80.0).h(32.0).opacity(1.0));
                let overlay: Element = drag_overlay().style(|el| el.opacity(0.5)).into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 80.0, 32.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.style.opacity - 0.5).abs() < 0.01,
                "DragOverlay style must override clone opacity; got {}",
                ghost.style.opacity
            );
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "本体文案"),
                "style overlay must keep source text, got {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// RSX `rotate={last_pointer…}` is `.style()` on the ghost. Island rebuild
    /// skips `overlay_pos`; chrome must re-sample or Gallery 四角 freezes.
    #[test]
    fn overlay_style_rotate_tracks_pointer_without_remount() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable, last_pointer, tick};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-rot").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| Element::new().text("tilt").w(80.0).h(32.0));
                let overlay: Element = drag_overlay()
                    .style(|el| {
                        let deg = last_pointer().map(|p| p[0]).unwrap_or(0.0);
                        el.rotate(deg)
                    })
                    .into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 80.0, 32.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let first = ghost.style.rotate_deg;
            assert!(
                (first - 50.0).abs() < 0.5,
                "activate tick pointer.x=50; rotate={first}"
            );
        }
        assert!(tick(200.0, 16.0, &[], [800.0, 600.0]));
        {
            let mut tree = live.borrow_mut();
            let ghost = find_ghost(&tree).expect("ghost");
            let frozen = ghost.style.rotate_deg;
            assert!(
                (frozen - 50.0).abs() < 0.5,
                "tick must not remount the island; rotate still first sample, got {frozen}"
            );
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let ghost = find_ghost(&tree).expect("ghost after chrome");
            assert!(
                (ghost.style.rotate_deg - 200.0).abs() < 0.5,
                "chrome must re-sample style rotate from last_pointer; got {}",
                ghost.style.rotate_deg
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Follow-source clone of a `w=100%` row must pin `drag_extent`, otherwise
    /// the percentage resolves against the portal / window and the ghost is a slab.
    #[test]
    fn ghost_follow_source_pins_used_extent() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-extent").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("拖左侧手柄；点文字不会启动")
                        .w("100%")
                        .h(48.0)
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [24.0, 16.0, 420.0, 48.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                (ghost.layout_bounds[2] - 420.0).abs() < 8.0,
                "follow-source ghost must pin used width, not 100% of portal; bounds={:?}",
                ghost.layout_bounds
            );
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("拖左侧手柄")),
                "extent pin must not replace source text, got {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Auto-sized follow-source clone must not inherit the stretched island
    /// `start_bounds` (Gallery chip next to a tall Droppable).
    #[test]
    fn ghost_auto_sized_clone_ignores_stretched_extent() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-hug").threshold(0.0).child(|| {
                let chip = draggable("chip").child(|| {
                    Element::new()
                        .text("源卡片")
                        .w(80.0)
                        .h(32.0)
                        .bg([0.2, 0.4, 0.9, 1.0])
                });
                let overlay: Element = drag_overlay().into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [24.0, 16.0, 420.0, 72.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            assert!(
                ghost.layout_bounds[2] < 160.0,
                "auto chip ghost must not pin stretched row width; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                ghost.layout_bounds[3] < 48.0,
                "auto chip ghost must not pin stretched row height; bounds={:?}",
                ghost.layout_bounds
            );
            assert!(
                (ghost.style.bg_color[2] - 0.9).abs() < 0.05,
                "ghost must keep author fill; bg={:?}",
                ghost.style.bg_color
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_custom_child_keeps_its_own_text() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_overlay, drag_scope, draggable};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("ov-custom").threshold(0.0).child(|| {
                let chip =
                    draggable("chip").child(|| Element::new().text("本体文案").w(80.0).h(32.0));
                let overlay: Element = drag_overlay()
                    .child(|| Element::new().text("custom-ghost").w(80.0).h(32.0))
                    .into();
                Element::from(vstack().children([chip, overlay]))
            })
        });
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_chip(
            &live,
            &sid,
            [40.0, 16.0],
            [10.0, 8.0],
            [30.0, 8.0, 80.0, 32.0],
        );
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
            let ghost = find_ghost(&tree).expect("ghost");
            let mut texts = Vec::new();
            collect_texts(ghost, &mut texts);
            assert!(
                texts.iter().any(|t| t == "custom-ghost"),
                "custom overlay child must replace source, got {texts:?}"
            );
            assert!(
                texts.iter().all(|t| t != "本体文案"),
                "custom child leaked source text: {texts:?}"
            );
        }
        let _ = crate::utils::drag::end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    #[test]
    fn ghost_layout_keeps_extent_at_origin() {
        let panel = place_drag_ghost(
            Element::new()
                .w(96.0)
                .h(36.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .text("task"),
            "g1",
            [120.0, 80.0],
            None,
        );
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 901));
        assert!(
            crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1,
            "ghost must hoist out of the 0×0 mount"
        );
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let ghost = find_ghost(&root).expect("hoisted ghost");
        assert!(
            ghost.layout_bounds[2] > 50.0,
            "ghost width vanished (0×0 portal CB); w={}",
            ghost.layout_bounds[2]
        );
        assert!(
            ghost.layout_bounds[3] > 20.0,
            "ghost height vanished (0×0 portal CB); h={}",
            ghost.layout_bounds[3]
        );
        assert!(
            (ghost.layout_bounds[0] - 120.0).abs() < 2.0,
            "ghost x should follow origin; x={}",
            ghost.layout_bounds[0]
        );
        assert!(
            (ghost.layout_bounds[1] - 80.0).abs() < 2.0,
            "ghost y should follow origin; y={}",
            ghost.layout_bounds[1]
        );
    }

    /// Custom overlay child (a small chip) must keep its author size even when
    /// the source item is a `w=100%` row. Pinning `drag_extent` belongs only to
    /// the default follow-source clone.
    #[test]
    fn ghost_keeps_author_size_when_source_extent_is_wider() {
        let inner = Element::new()
            .w(80.0)
            .h(32.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("handled");
        let panel = place_drag_ghost(inner, "g1", [24.0, 16.0], None);
        let mut root = Element::new()
            .w(800.0)
            .h(600.0)
            .child(mount_anchored_overlay(panel, 901));
        assert!(crate::layout::layout_tree::hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let ghost = find_ghost(&root).expect("hoisted ghost");
        assert!(
            ghost.layout_bounds[2] > 40.0 && ghost.layout_bounds[2] < 120.0,
            "ghost must keep overlay-body size, not stretch to source row; bounds={:?}",
            ghost.layout_bounds
        );
        assert!(
            ghost.layout_bounds[3] > 16.0 && ghost.layout_bounds[3] < 40.0,
            "ghost height stretched to source row; bounds={:?}",
            ghost.layout_bounds
        );
        assert!(
            (ghost.layout_bounds[0] - 24.0).abs() < 2.0
                && (ghost.layout_bounds[1] - 16.0).abs() < 2.0,
            "origin lost; bounds={:?}",
            ghost.layout_bounds
        );
    }

    fn contains_portal(el: &Element) -> bool {
        crate::core::tree::find_by_attr(el, "rui_portal", "1").is_some()
    }

    /// Live `AppState::reconcile_subtree`: `reconcile_elements` then assign, then hoist.
    /// `install_test_reconcile_root` skips this and wholesale-replaces the slot.
    fn apply_like_appstate(root: &mut Element, anchor: &str, mut fresh: Element) {
        let want = anchor.replace('\\', "/").to_ascii_lowercase();
        let needs = {
            let Some(old) = crate::core::tree::find_in_tree_mut(root, |n| {
                n.template_id
                    .as_deref()
                    .is_some_and(|t| t.replace('\\', "/").to_ascii_lowercase() == want)
            }) else {
                panic!("anchor {anchor} not in tree");
            };
            let _ = crate::app::test_support::reconcile_elements(&mut fresh, old);
            let needs = contains_portal(&fresh);
            *old = fresh;
            needs
        };
        if needs {
            let n = crate::layout::layout_tree::hoist_portal_subtrees(root);
            assert!(
                n >= 1,
                "open fragment had rui_portal but hoist moved 0 nodes"
            );
        }
    }

    fn open_ghost_at(tid: &str, origin: [f32; 2], inner_tid: Option<&str>) -> Element {
        let mut inner = Element::new()
            .w(96.0)
            .h(36.0)
            .bg([1.0, 0.0, 0.0, 1.0])
            .text("ghost");
        if let Some(t) = inner_tid {
            inner.template_id = Some(t.into());
        }
        let panel = place_drag_ghost(inner, "g1", origin, None);
        let mut el = mount_anchored_overlay(panel, 901);
        el.template_id = Some(tid.into());
        el
    }

    fn red_paint_at(
        result: &crate::layout::RenderResult,
        origin: [f32; 2],
        min_size: [f32; 2],
    ) -> bool {
        result.instances.iter().any(|i| {
            i.color[0] > 0.85
                && i.color[1] < 0.2
                && i.color[2] < 0.2
                && i.color[3] > 0.85
                && i.rect_size[0] >= min_size[0]
                && i.rect_size[1] >= min_size[1]
                && (i.rect_pos[0] - origin[0]).abs() < 3.0
                && (i.rect_pos[1] - origin[1]).abs() < 3.0
                && (i.clip_size[0] < 0.0
                    || (i.clip_size[0] >= min_size[0] && i.clip_size[1] >= min_size[1]))
        })
    }

    /// Gallery idle frame has already laid out the 0×0 placeholder. The first
    /// overlay open then a pointer move both go through `reconcile_elements`
    /// (slot vs new portal, equal child len). Wholesale test reconcile never
    /// hits this.
    #[test]
    fn appstate_reconcile_after_idle_layout_paints_ghost_and_follows() {
        use crate::app::reactive::reactive_placeholder_sized;

        const TID: &str = "ov-loop__drag_overlay";
        let placeholder = reactive_placeholder_sized(TID, 0.0, 0.0).pointer_events_box_none();
        let scroll = Element::new()
            .w(400.0)
            .h(120.0)
            .overflow("hidden")
            .child(placeholder);
        let later = Element::new().w(400.0).h(400.0).bg([0.1, 0.1, 0.1, 1.0]);
        let mut root = Element::new().w(800.0).h(600.0).child(scroll).child(later);

        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);

        apply_like_appstate(
            &mut root,
            TID,
            open_ghost_at(TID, [40.0, 200.0], Some("card_chip")),
        );
        let painted = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g = find_ghost(&root).expect("ghost missing after first AppState reconcile");
        assert!(
            ghost_is_under_portal_layer(&root),
            "ghost not on portal layer after AppState open"
        );
        assert!(
            g.layout_bounds[2] > 50.0 && g.layout_bounds[3] > 20.0,
            "ghost 0-sized after AppState open: {:?}",
            g.layout_bounds
        );
        assert!(
            (g.layout_bounds[0] - 40.0).abs() < 2.0 && (g.layout_bounds[1] - 200.0).abs() < 2.0,
            "ghost not at overlay_pos after AppState open: {:?}",
            g.layout_bounds
        );
        assert!(
            red_paint_at(&painted, [40.0, 200.0], [50.0, 20.0]),
            "ghost has layout but paint missed it (clip/z). overlay_split.start={} instances={}",
            painted.overlay_split.instance_start,
            painted.instances.len()
        );

        let _ = crate::utils::popup::sync_floating_placement(&mut root, 800.0, 600.0);
        apply_like_appstate(
            &mut root,
            TID,
            open_ghost_at(TID, [80.0, 220.0], Some("card_chip")),
        );
        let painted2 = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let _ = crate::utils::popup::sync_floating_placement(&mut root, 800.0, 600.0);
        let painted3 = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g2 = find_ghost(&root).expect("ghost missing after pos tick reconcile");
        assert!(
            (g2.layout_bounds[0] - 80.0).abs() < 2.0 && (g2.layout_bounds[1] - 220.0).abs() < 2.0,
            "ghost not following after second AppState reconcile: {:?}",
            g2.layout_bounds
        );
        assert!(
            red_paint_at(&painted3, [80.0, 220.0], [50.0, 20.0])
                || red_paint_at(&painted2, [80.0, 220.0], [50.0, 20.0]),
            "ghost layout followed but paint did not. bounds={:?} n0={} n1={}",
            g2.layout_bounds,
            painted2.instances.len(),
            painted3.instances.len()
        );
    }

    #[test]
    fn appstate_reconcile_plain_inner_without_template_id_still_paints() {
        use crate::app::reactive::reactive_placeholder_sized;

        const TID: &str = "ov-plain__drag_overlay";
        let placeholder = reactive_placeholder_sized(TID, 0.0, 0.0);
        let mut root = Element::new().w(800.0).h(600.0).child(
            Element::new()
                .w(400.0)
                .h(80.0)
                .overflow("hidden")
                .child(placeholder),
        );

        let mut tree = crate::layout::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        apply_like_appstate(&mut root, TID, open_ghost_at(TID, [16.0, 24.0], None));
        apply_like_appstate(&mut root, TID, open_ghost_at(TID, [64.0, 48.0], None));
        let painted = tree.build_and_layout(&mut root, 800.0, 600.0, [0.0, 0.0]);
        let g = find_ghost(&root).expect("plain inner ghost missing");
        assert!(
            g.layout_bounds[2] > 50.0 && (g.layout_bounds[0] - 64.0).abs() < 2.0,
            "plain inner (no tid, pairs with portal slot) vanished: {:?}",
            g.layout_bounds
        );
        assert!(
            red_paint_at(&painted, [64.0, 48.0], [50.0, 20.0]),
            "plain inner ghost not in paint at overlay_pos"
        );
    }

    fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
        if el.template_id.as_deref() == Some(tid) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_tid(c, tid))
    }

    /// Fast sortable drag used to remount every item island on `hover_index` /
    /// `overlay_pos` (h=auto placeholders collapsed → list height jitter).
    #[test]
    fn sortable_preview_does_not_remount_or_jitter_height() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_scope, draggable, end, tick, Axis, BeginDrag, DragHit,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let runs = Rc::new(Cell::new(0));
        let (_o, live) = mount_scope_tree({
            let runs = runs.clone();
            move || {
                let runs = runs.clone();
                drag_scope("ov-sort-h")
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .child(move || {
                        let mk = |id: &'static str, idx: usize| {
                            let runs = runs.clone();
                            draggable(id)
                                .also_droppable(true)
                                .item_type("planet")
                                .list("planets")
                                .index(idx)
                                .child(move || {
                                    runs.set(runs.get() + 1);
                                    Element::new().text(id).w(80.0).h(32.0)
                                })
                        };
                        let mut list = Element::from(vstack().gap(8.0).children([
                            mk("mercury", 0),
                            mk("venus", 1),
                            mk("earth", 2),
                        ]));
                        list.template_id = Some("ov-sort-h-list".into());
                        list
                    })
            }
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }
        let h0 = find_tid(&live.borrow(), "ov-sort-h-list")
            .expect("list")
            .layout_bounds[3];
        assert!(
            h0 > 80.0,
            "precondition: three slotted rows must lay out; h={h0}"
        );
        let n0 = runs.get();
        assert!(n0 >= 3, "three author bodies should mount; runs={n0}");

        begin(BeginDrag {
            scope_id: "ov-sort-h".into(),
            draggable_id: "mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, y: f32, index: usize| DragHit {
            id: id.into(),
            scope_id: "ov-sort-h".into(),
            is_draggable: true,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, y, 80.0, 32.0]),
            item_type: "planet".into(),
            accept: String::new(),
            list_id: Some("planets".into()),
            index: Some(index),
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let hits = [
            hit("mercury", 0.0, 0),
            hit("venus", 40.0, 1),
            hit("earth", 80.0, 2),
        ];
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(
            tick(40.0, 88.0, &hits, [800.0, 600.0]),
            "pointer past last midpoint must keep the session active"
        );
        drain_deferred_view_remounts();
        assert_eq!(
            runs.get(),
            n0,
            "hover / overlay_pos must not remount author bodies; before={n0} after={}",
            runs.get()
        );

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let h1 = find_tid(&tree, "ov-sort-h-list")
                .expect("list after hover")
                .layout_bounds[3];
            assert!(
                (h1 - h0).abs() < 1.0,
                "sortable preview must not change list layout height; before={h0} after={h1}"
            );
            let venus = find_draggable(&tree, "venus").expect("venus");
            assert!(
                venus.style.translate[1].abs() > 1.0,
                "siblings must still shift via reactive_sync; translate={:?}",
                venus.style.translate
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Overlay used to `.get()` `overlay_pos`, remounting `clone_source_preview`
    /// (and the author body) on every pointer / autoscroll / cross-list tick.
    #[test]
    fn overlay_pointer_scroll_and_cross_list_do_not_remount() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::scroll_offset_y_signal_key;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, end, pump_autoscroll, tick, Axis,
            BeginDrag, DragHit,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let runs = Rc::new(Cell::new(0));
        let (_o, live) = mount_scope_tree({
            let runs = runs.clone();
            move || {
                let runs = runs.clone();
                drag_scope("ov-jank")
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .child(move || {
                        let mk = |id: &'static str, list: &'static str, idx: usize| {
                            let runs = runs.clone();
                            draggable(id)
                                .also_droppable(true)
                                .item_type("file")
                                .group("nest")
                                .list(list)
                                .index(idx)
                                .child(move || {
                                    runs.set(runs.get() + 1);
                                    Element::new().text(id).w(80.0).h(32.0)
                                })
                        };
                        let inbox = Element::from(
                            vstack()
                                .gap(6.0)
                                .children([mk("alpha", "inbox", 0), mk("beta", "inbox", 1)]),
                        );
                        let later =
                            Element::from(vstack().gap(6.0).children([mk("gamma", "later", 0)]));
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().children([
                            Element::from(hstack().gap(16.0).children([inbox, later])),
                            overlay,
                        ]))
                    })
            }
        });
        let n0 = runs.get();
        assert!(n0 >= 3, "three author bodies should mount; runs={n0}");

        begin(BeginDrag {
            scope_id: "ov-jank".into(),
            draggable_id: "alpha".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, x: f32, y: f32, list: &str, index: usize| DragHit {
            id: id.into(),
            scope_id: "ov-jank".into(),
            is_draggable: true,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([x, y, 80.0, 32.0]),
            item_type: "file".into(),
            accept: String::new(),
            list_id: Some(list.into()),
            index: Some(index),
            group: "nest".into(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let inbox_hits = [
            hit("alpha", 0.0, 0.0, "inbox", 0),
            hit("beta", 0.0, 38.0, "inbox", 1),
        ];
        let later_hits = [
            hit("alpha", 0.0, 0.0, "inbox", 0),
            hit("beta", 0.0, 38.0, "inbox", 1),
            hit("gamma", 96.0, 0.0, "later", 0),
        ];
        assert!(tick(41.0, 16.0, &inbox_hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let n_open = runs.get();
        assert!(
            n_open >= n0 && n_open <= n0 + 1,
            "opening the overlay may clone the source once; before={n0} after={n_open}"
        );

        for y in [24.0, 40.0, 56.0, 72.0, 88.0] {
            assert!(tick(40.0, y, &inbox_hits, [800.0, 600.0]));
        }
        drain_deferred_view_remounts();
        assert_eq!(
            runs.get(),
            n_open,
            "pointer moves must not remount author bodies / ghost clone; before={n_open} after={}",
            runs.get()
        );

        assert!(tick(120.0, 16.0, &later_hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert_eq!(
            runs.get(),
            n_open,
            "cross-list hover must not remount author bodies / ghost clone; before={n_open} after={}",
            runs.get()
        );

        use_signal(&scroll_offset_y_signal_key("ov-jank-scroll"), || 0.0_f32).set_silent(40.0);
        {
            let tree = live.borrow();
            let _ = pump_autoscroll(&tree, [800.0, 600.0]);
        }
        drain_deferred_view_remounts();
        assert_eq!(
            runs.get(),
            n_open,
            "autoscroll / scroll offset must not remount author bodies; before={n_open} after={}",
            runs.get()
        );

        let _ = end(&later_hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Dimmed source occupies the insert hole (opacity 0.4). opacity 0 leaves
    /// that hole empty (opacity 0 at origin). Siblings still FLIP the gap.
    #[test]
    fn dimmed_source_occupies_insert_hole() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, end, tick, Axis, BeginDrag, DragHit,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-sort-src")
                .axis(Axis::Y)
                .threshold(0.0)
                .child(move || {
                    // Register overlay so chrome dims the source. Do not parent
                    // the ghost island: portal + sync_reactive overflowed the stack.
                    let _ = Element::from(drag_overlay());
                    let mk = |id: &'static str, idx: usize| {
                        draggable(id)
                            .also_droppable(true)
                            .item_type("planet")
                            .list("planets")
                            .index(idx)
                            .child(move || Element::new().text(id).w(80.0).h(32.0))
                    };
                    Element::from(vstack().gap(8.0).children([
                        mk("mercury", 0),
                        mk("venus", 1),
                        mk("earth", 2),
                    ]))
                })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        begin(BeginDrag {
            scope_id: "ov-sort-src".into(),
            draggable_id: "mercury".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, y: f32, index: usize| DragHit {
            id: id.into(),
            scope_id: "ov-sort-src".into(),
            is_draggable: true,
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([0.0, y, 80.0, 32.0]),
            item_type: "planet".into(),
            accept: String::new(),
            list_id: Some("planets".into()),
            index: Some(index),
            group: String::new(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let hits = [
            hit("mercury", 0.0, 0),
            hit("venus", 40.0, 1),
            hit("earth", 80.0, 2),
        ];
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "mercury").expect("mercury at grab");
            let venus = find_draggable(&tree, "venus").expect("venus at grab");
            assert!(
                mercury.style.translate[1].abs() < 1.0,
                "grab at source centre must not swap with the next row; translate={:?}",
                mercury.style.translate
            );
            assert!(
                venus.style.translate[1].abs() < 1.0,
                "neighbour must not slide under the dimmed body at grab; translate={:?}",
                venus.style.translate
            );
        }
        assert!(tick(40.0, 88.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let mercury = find_draggable(&tree, "mercury").expect("mercury");
            let venus = find_draggable(&tree, "venus").expect("venus");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "source stays visible and dimmed; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1] > 40.0,
                "dimmed body must sit in the insert hole, not under siblings at origin; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source must paint above overlapping siblings; z={}",
                mercury.style.z_index
            );
            assert!(
                venus.style.translate[1] < -1.0,
                "neighbour must still vacate the hole; translate={:?}",
                venus.style.translate
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Default overlay (opacity 0.4): the target list paints a dimmed hole preview
    /// (source island stays in the origin tree at opacity 0).
    #[test]
    fn dimmed_source_cross_list_appears_in_target_hole() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, drag_overlay, drag_scope, draggable, droppable, end, tick, BeginDrag, DragHit,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-cross-src").threshold(0.0).child(move || {
                let _ = Element::from(drag_overlay());
                let mk = |id: &'static str, list: &'static str, idx: usize| {
                    draggable(id)
                        .also_droppable(true)
                        .item_type("task")
                        .group("board")
                        .list(list)
                        .index(idx)
                        .child(move || Element::new().text(id).w(80.0).h(32.0))
                };
                let left = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(move || Element::from(vstack().w(80.0).child(mk("src", "todo", 0))));
                let right = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(move || Element::from(vstack().w(80.0).child(mk("dst", "doing", 0))));
                Element::from(hstack().gap(40.0).children([left, right]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        begin(BeginDrag {
            scope_id: "ov-cross-src".into(),
            draggable_id: "src".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, list: &str, x: f32, y: f32, index: Option<usize>| DragHit {
            id: id.into(),
            scope_id: "ov-cross-src".into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([x, y, 80.0, 32.0]),
            item_type: "task".into(),
            accept: "task".into(),
            list_id: Some(list.into()),
            index,
            group: "board".into(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let hits = [
            hit("src", "todo", 0.0, 0.0, Some(0)),
            hit("todo", "todo", 0.0, 0.0, None),
            hit("dst", "doing", 120.0, 0.0, Some(0)),
            hit("doing", "doing", 120.0, 0.0, None),
        ];
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(tick(140.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let src = find_draggable(&tree, "src").expect("src");
            assert!(
                src.style.opacity < 0.05,
                "cross-list source island stays in origin at 0; opacity={}",
                src.style.opacity
            );
            assert!(
                src.style.translate[0].abs() < 1.0 && src.style.translate[1].abs() < 1.0,
                "source must not translate into the target; translate={:?}",
                src.style.translate
            );
            let doing = find_droppable(&tree, "doing").expect("doing");
            let preview = find_hole_preview(doing).expect("target hole preview");
            assert!(
                (preview.style.opacity - 0.4).abs() < 0.05,
                "target hole preview must be dimmed; opacity={}",
                preview.style.opacity
            );
            let mut texts = Vec::new();
            collect_texts(preview, &mut texts);
            assert!(
                texts.iter().any(|t| t.contains("src")),
                "hole preview must clone the source body; texts={texts:?}"
            );
            assert!(
                crate::layout::lpa_length_px(preview.style.inset.left)
                    .unwrap_or(0.0)
                    .abs()
                    < 0.5
                    && crate::layout::lpa_length_px(preview.style.inset.top)
                        .unwrap_or(0.0)
                        .abs()
                        < 0.5,
                "hole preview origin must be paint translate, not inset; inset=({:?}, {:?})",
                preview.style.inset.left,
                preview.style.inset.top
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn width_spec_is_percent(el: &Element) -> bool {
        use crate::style::UnitSlot;
        el.style
            .unit_specs
            .iter()
            .any(|(s, d)| matches!(*s, UnitSlot::Width | UnitSlot::InlineSize) && d.has_percent())
    }

    /// Gallery kanban `list_chip` is `w=100%` inside a padded, bordered column.
    /// Pinning `size.width` alone is rebaked from `unit_specs` (`width: 100%` of
    /// the abspos padding box) — a sliver wider than the in-flow sibling.
    #[test]
    fn cross_list_hole_preview_keeps_stretched_row_width() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, draggable, droppable, end,
            start_geometry, tick, BeginDrag,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-cross-w").threshold(0.0).child(move || {
                let overlay: Element = drag_overlay().into();
                let mk = |id: &'static str, list: &'static str, idx: usize| {
                    draggable(id)
                        .also_droppable(true)
                        .item_type("task")
                        .group("board")
                        .list(list)
                        .index(idx)
                        .child(move || Element::new().text("Draft RFC").w("100%").h(32.0))
                };
                let left = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p(8.0)
                                .border_style("solid")
                                .border_width(1.0)
                                .child(mk("src", "todo", 0)),
                        )
                    });
                let right = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p(8.0)
                                .border_style("solid")
                                .border_width(1.0)
                                .child(mk("dst", "doing", 0)),
                        )
                    });
                Element::from(hstack().gap(40.0).children([left, right, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0.iter().find(|h| h.id == "src").expect("src");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-cross-w",
            "src",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-cross-w".into(),
            draggable_id: "src".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let doing_hit = hits0.iter().find(|h| h.id == "doing").expect("doing");
        let dest = [
            doing_hit.rect.x + 20.0,
            doing_hit.rect.y + doing_hit.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let doing = find_droppable(&tree, "doing").expect("doing");
            let dst = find_draggable(&tree, "dst").expect("dst");
            let preview = find_hole_preview(doing).expect("target hole preview");
            assert!(
                !width_spec_is_percent(preview),
                "pin must drop w=100% from unit_specs or layout rebakes the padding box"
            );
            assert!(
                (preview.layout_bounds[2] - dst.layout_bounds[2]).abs() < 2.0,
                "hole preview must match the in-flow sibling, not the column padding box; \
                 preview_w={} dst_w={} doing_w={}",
                preview.layout_bounds[2],
                dst.layout_bounds[2],
                doing.layout_bounds[2]
            );
            assert!(
                preview.layout_bounds[2] + 8.0 < doing.layout_bounds[2],
                "preview must stay inside padding/border; preview_w={} doing_w={}",
                preview.layout_bounds[2],
                doing.layout_bounds[2]
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery board: `p="0.5rem"` column + title + inner `w=100%` list of
    /// `SortableItem` chips. The hole clone must match the in-flow chip, not
    /// `100%` of the column padding box (~16px too wide).
    #[test]
    fn cross_list_hole_preview_matches_kanban_chip_inside_padded_column() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, droppable, end, sortable_item,
            start_geometry, tick, BeginDrag,
        };

        fn chip(label: &'static str) -> Element {
            Element::from(
                hstack()
                    .w("100%")
                    .p("0.5rem")
                    .h(32.0)
                    .border_style("solid")
                    .border_width(1.0)
                    .child(Element::new().text(label)),
            )
        }
        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-kanban-w").threshold(0.0).child(move || {
                let overlay: Element = drag_overlay().into();
                let item =
                    |id: &'static str, list: &'static str, idx: usize, label: &'static str| {
                        sortable_item(id)
                            .item_type("task")
                            .group("board")
                            .list(list)
                            .index(idx)
                            .child(move || chip(label))
                    };
                let todo = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p("0.5rem")
                                .border_style("solid")
                                .border_width(1.0)
                                .child(Element::new().text("todo").h(16.0))
                                .child(vstack().w("100%").gap("0.375rem").child(item(
                                    "src",
                                    "todo",
                                    0,
                                    "Draft RFC",
                                ))),
                        )
                    });
                let doing = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p("0.5rem")
                                .border_style("solid")
                                .border_width(1.0)
                                .child(Element::new().text("doing").h(16.0))
                                .child(vstack().w("100%").gap("0.375rem").child(item(
                                    "dst",
                                    "doing",
                                    0,
                                    "Write tests",
                                ))),
                        )
                    });
                Element::from(hstack().gap(40.0).children([todo, doing, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0.iter().find(|h| h.id == "src").expect("src");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-kanban-w",
            "src",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-kanban-w".into(),
            draggable_id: "src".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let doing_hit = hits0.iter().find(|h| h.id == "doing").expect("doing");
        let dest = [
            doing_hit.rect.x + 20.0,
            doing_hit.rect.y + doing_hit.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let doing = find_droppable(&tree, "doing").expect("doing");
            let dst = find_draggable(&tree, "dst").expect("dst");
            let preview = find_hole_preview(doing).expect("target hole preview");
            assert!(
                !width_spec_is_percent(preview),
                "kanban hole preview must not keep w=100% in unit_specs"
            );
            assert!(
                (preview.layout_bounds[2] - dst.layout_bounds[2]).abs() < 2.0,
                "kanban hole preview must match Write tests, not the column padding box; \
                 preview_w={} dst_w={} doing_w={}",
                preview.layout_bounds[2],
                dst.layout_bounds[2],
                doing.layout_bounds[2]
            );
            assert!(
                preview.layout_bounds[2] + 8.0 < doing.layout_bounds[2],
                "preview must sit inside 0.5rem padding; preview_w={} doing_w={}",
                preview.layout_bounds[2],
                doing.layout_bounds[2]
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery `use_drag().hits` empty column: title + inner `w=100%` list.
    /// The dimmed body must sit in the list pack, not over the header.
    #[test]
    fn cross_list_empty_column_hole_preview_sits_below_title() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, droppable, end, sortable_item,
            start_geometry, tick, BeginDrag,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-empty-title").threshold(0.0).child(move || {
                let overlay: Element = drag_overlay().into();
                let src = droppable("open")
                    .list("open")
                    .group("veto")
                    .accept("veto")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p(8.0)
                                .border_style("solid")
                                .border_width(1.0)
                                .child(Element::new().text("Open").h(20.0))
                                .child(
                                    vstack().w("100%").child(
                                        sortable_item("Move me")
                                            .item_type("veto")
                                            .group("veto")
                                            .list("open")
                                            .index(0)
                                            .source_opacity(0.4)
                                            .child(|| {
                                                Element::new().text("Move me").w("100%").h(32.0)
                                            }),
                                    ),
                                ),
                        )
                    });
                let empty = droppable("empty")
                    .list("empty")
                    .group("veto")
                    .accept("veto")
                    .child(move || {
                        Element::from(
                            vstack()
                                .w(200.0)
                                .p(8.0)
                                .border_style("solid")
                                .border_width(1.0)
                                .child(Element::new().text("Empty").h(20.0))
                                .child(vstack().w("100%").min_h(0.0)),
                        )
                    });
                Element::from(hstack().gap(40.0).children([src, empty, overlay]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0.iter().find(|h| h.id == "Move me").expect("src");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-empty-title",
            "Move me",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-empty-title".into(),
            draggable_id: "Move me".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("open".into()),
            from_index: Some(0),
            item_type: "veto".into(),
            group: "veto".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let empty_hit = hits0.iter().find(|h| h.id == "empty").expect("empty");
        let dest = [
            empty_hit.rect.x + 20.0,
            empty_hit.rect.y + empty_hit.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            fn zero_trailing_pack(el: &mut Element) {
                if el
                    .attrs
                    .get(crate::utils::drag::ATTR_DRAG_DROPPABLE)
                    .map(String::as_str)
                    == Some("empty")
                {
                    for c in el.children.iter_mut().rev() {
                        if c.attrs
                            .contains_key(crate::utils::drag::ATTR_DRAG_HOLE_PREVIEW)
                            || c.style.position == taffy::style::Position::Absolute
                            || c.style.position_fixed
                        {
                            continue;
                        }
                        c.layout_bounds = [0.0, 0.0, 0.0, 0.0];
                        return;
                    }
                }
                for c in &mut el.children {
                    zero_trailing_pack(c);
                }
            }
            zero_trailing_pack(&mut tree);
            // Droppable islands keep inner layout_bounds 0-based while hits
            // stay page-space. Chrome must not subtract the hit origin.
            fn shift_layout_tree(el: &mut Element, dx: f32, dy: f32) {
                el.layout_bounds[0] += dx;
                el.layout_bounds[1] += dy;
                for c in &mut el.children {
                    shift_layout_tree(c, dx, dy);
                }
            }
            fn localize_droppable(el: &mut Element, id: &str) -> bool {
                if el
                    .attrs
                    .get(crate::utils::drag::ATTR_DRAG_DROPPABLE)
                    .map(String::as_str)
                    == Some(id)
                {
                    let dx = -el.layout_bounds[0];
                    let dy = -el.layout_bounds[1];
                    shift_layout_tree(el, dx, dy);
                    return true;
                }
                el.children.iter_mut().any(|c| localize_droppable(c, id))
            }
            localize_droppable(&mut tree, "empty");
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let empty = find_droppable(&tree, "empty").expect("empty");
            let title = empty
                .children
                .iter()
                .find(|c| c.text.as_deref() == Some("Empty"))
                .expect("title");
            let title_bottom = title.layout_bounds[1] + title.layout_bounds[3];
            let preview = find_hole_preview(empty).expect("empty hole preview");
            let py = preview_visual_y(preview);
            assert!(
                py + 1.0 >= title_bottom,
                "dimmed body must sit below the column title, not over it; \
                 preview_y={py} title_bottom={title_bottom} empty_y={}",
                empty.layout_bounds[1]
            );
            assert!(
                py + 1.0 >= empty.layout_bounds[1]
                    && py < empty.layout_bounds[1] + empty.layout_bounds[3] + 48.0,
                "dimmed body must stay inside the empty column, not translate to (0,0); \
                 preview_y={py} empty={:?}",
                empty.layout_bounds
            );
            assert!(
                preview.layout_bounds[2] > 10.0 && preview.layout_bounds[3] > 8.0,
                "empty-column hole preview must keep chip size; bounds={:?}",
                preview.layout_bounds
            );
            assert!(
                preview.style.translate[0].abs() < empty.layout_bounds[2] + 8.0
                    && preview.style.translate[1] >= -1.0
                    && preview.style.translate[1] < empty.layout_bounds[3] + 48.0,
                "empty-column hole translate must be local to the column CB, not \
                 pack-minus-hit-origin; translate={:?} empty={:?}",
                preview.style.translate,
                empty.layout_bounds
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery Locked: `can_drop` rejects the item. Pickup is allowed (`disabled`
    /// is the no-drag switch); parking in the empty column is not.
    #[test]
    fn can_drop_veto_does_not_preview_or_hover_empty_column() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, droppable, end, sortable_item,
            start_geometry, tick, BeginDrag, ATTR_DRAG_OVER,
        };

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("ov-locked-veto")
                .threshold(0.0)
                .can_drop(|ev| !ev.draggable_id.contains("Locked"))
                .child(move || {
                    let overlay: Element = drag_overlay().into();
                    let src = droppable("open")
                        .list("open")
                        .group("veto")
                        .accept("veto")
                        .child(move || {
                            Element::from(
                                vstack().w(200.0).p(8.0).child(
                                    sortable_item("open:Locked")
                                        .item_type("veto")
                                        .group("veto")
                                        .list("open")
                                        .index(0)
                                        .source_opacity(0.4)
                                        .child(|| Element::new().text("Locked").w("100%").h(32.0)),
                                ),
                            )
                        });
                    let empty = droppable("empty")
                        .list("empty")
                        .group("veto")
                        .accept("veto")
                        .child(move || {
                            Element::from(
                                vstack()
                                    .w(200.0)
                                    .p(8.0)
                                    .min_h(80.0)
                                    .child(Element::new().text("Empty").h(20.0))
                                    .child(vstack().w("100%").min_h(0.0)),
                            )
                        });
                    Element::from(hstack().gap(40.0).children([src, empty, overlay]))
                })
        });
        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "open:Locked")
            .expect("locked");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-locked-veto",
            "open:Locked",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-locked-veto".into(),
            draggable_id: "open:Locked".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("open".into()),
            from_index: Some(0),
            item_type: "veto".into(),
            group: "veto".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let empty_hit = hits0.iter().find(|h| h.id == "empty").expect("empty");
        let dest = [
            empty_hit.rect.x + 20.0,
            empty_hit.rect.y + empty_hit.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            hoist_and_layout(&mut tree);
            let empty = find_droppable(&tree, "empty").expect("empty");
            assert!(
                find_hole_preview(empty).is_none(),
                "can_drop veto must not paint a hole preview in the empty column"
            );
            assert_ne!(
                empty.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "can_drop veto must not mark the empty column as hover"
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Auto-height sortable lists: cross-list hover must shrink the source list
    /// (vacate the row in the parent flow), not only pad the target. After
    /// `apply_transfer` the origin list height must drop by about one row.
    #[test]
    fn nested_cross_list_transfer_shrinks_source_folder() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let later = Signal::new(vec!["Gamma".to_string()]);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            move || {
                drag_scope("ov-nest-shrink")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let mut dst = if to == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "tree-inbox" {
                                inbox.set(src);
                                later.set(dst);
                            } else {
                                later.set(src);
                                inbox.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let mk = |id: &'static str, files: Signal<Vec<String>>| {
                            sortable_list(id)
                                .accept("file")
                                .group("nest")
                                .child(move || {
                                    let items = files.get();
                                    let rows: Vec<Element> = items
                                        .iter()
                                        .enumerate()
                                        .map(|(i, name)| chip(id, name.clone(), i))
                                        .collect();
                                    Element::from(
                                        vstack().w("100%").min_h(0.0).gap(6.0).children(rows),
                                    )
                                })
                        };
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(200.0).min_h(0.0).gap(8.0).children([
                            mk("tree-inbox", inbox),
                            mk("tree-later", later),
                            overlay,
                        ]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let list_h =
            |tree: &Element, id: &str| find_droppable(tree, id).expect(id).layout_bounds[3];
        let pad_b = |tree: &Element, id: &str| {
            crate::layout::lp_length_px(find_droppable(tree, id).expect(id).style.padding.bottom)
                .unwrap_or(0.0)
        };
        let mar_b = |tree: &Element, id: &str| {
            crate::layout::lpa_length_px(find_droppable(tree, id).expect(id).style.margin.bottom)
                .unwrap_or(0.0)
        };

        let h0_inbox = list_h(&live.borrow(), "tree-inbox");
        let h0_later = list_h(&live.borrow(), "tree-later");
        assert!(
            h0_inbox > h0_later + 20.0,
            "precondition: Inbox (2 files) taller than Later (1); inbox={h0_inbox} later={h0_later}"
        );

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-shrink",
            "tree-inbox:Alpha",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-shrink".into(),
            draggable_id: "tree-inbox:Alpha".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let later_hit = hits0
            .iter()
            .find(|h| h.id == "tree-later")
            .expect("later list");
        let dest = [
            later_hit.rect.x + 20.0,
            later_hit.rect.y + later_hit.rect.height * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
        }

        let later_pad = pad_b(&live.borrow(), "tree-later");
        let inbox_margin = mar_b(&live.borrow(), "tree-inbox");
        assert!(
            later_pad > 16.0,
            "target list must pad by dragged extent; pad={later_pad}"
        );
        assert!(
            inbox_margin >= -1.0,
            "source must not eat trailing margin/padding; margin={inbox_margin}"
        );
        {
            let tree = live.borrow();
            let shell = find_item_shell(&tree, "tree-inbox:Alpha").expect("Alpha shell");
            assert!(
                shell_block_collapsed(shell),
                "source row must leave the origin pack by collapsing the shell, not a negative margin; size={:?} max={:?}",
                shell.style.size.height,
                shell.style.max_size.height
            );
        }

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            hoist_and_layout(&mut tree);
        }

        assert_eq!(inbox.peek(), vec!["Beta".to_string()]);
        assert!(later.peek().iter().any(|s| s == "Alpha"));
        let h1_inbox = list_h(&live.borrow(), "tree-inbox");
        let h1_later = list_h(&live.borrow(), "tree-later");
        assert!(
            h1_later > h0_later + 16.0,
            "Later must grow after taking Alpha; before={h0_later} after={h1_later}"
        );
        assert!(
            h1_inbox < h0_inbox - 16.0,
            "Inbox must shrink after Alpha leaves; before={h0_inbox} after={h1_inbox}"
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery nested folders: inner files live inside outer `SortableItem`
    /// cards. Cross-list hover must keep the dimmed source (not only the
    /// overlay ghost) and shrink the origin folder card — sibling-list
    /// padding/margin on the inner shell is not enough when the parent is a
    /// folder row (`keyed_item_shell` stacking + flex auto height).
    #[test]
    fn nested_folder_inner_file_cross_list_keeps_dimmed_source_and_shrinks_folder() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::layout::{dim_kind, DimKind};
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let later = Signal::new(vec!["Gamma".to_string()]);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            move || {
                drag_scope("ov-nest-folder")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let mut dst = if to == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "tree-inbox" {
                                inbox.set(src);
                                later.set(dst);
                            } else {
                                later.set(src);
                                inbox.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let inbox = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(220.0).children([inbox, later, overlay]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let folder_h =
            |tree: &Element, id: &str| find_draggable(tree, id).expect(id).layout_bounds[3];
        let h0_inbox = folder_h(&live.borrow(), "folder:Inbox");
        let h0_later = folder_h(&live.borrow(), "folder:Later");
        assert!(
            h0_inbox > h0_later + 20.0,
            "precondition: Inbox folder taller than Later; inbox={h0_inbox} later={h0_later}"
        );

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-folder",
            "tree-inbox:Alpha",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-folder".into(),
            draggable_id: "tree-inbox:Alpha".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        // Same-list hover on Beta opens a -pitch FLIP. Crossing to Later must
        // snap that leftover away: layout already packed Beta into Alpha's
        // slot, so easing -pitch→0 starts at the Inbox container top.
        let beta_hit = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let over_beta = [
            beta_hit.rect.x + 20.0,
            beta_hit.rect.y + beta_hit.rect.height * 0.5,
        ];
        let hits_beta = collect_hits(&live.borrow());
        assert!(tick(over_beta[0], over_beta[1], &hits_beta, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
        }

        let later_hit = hits0
            .iter()
            .find(|h| h.id == "tree-later")
            .expect("later list");
        let dest = [later_hit.rect.x + 20.0, later_hit.rect.y + 4.0];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            hoist_and_layout(&mut tree);
        }

        {
            let tree = live.borrow();
            let ghost = find_ghost(&tree).expect("overlay ghost");
            let mut ghost_texts = Vec::new();
            collect_texts(ghost, &mut ghost_texts);
            assert!(
                ghost_texts.iter().any(|t| t.contains("Alpha")),
                "overlay ghost must keep Alpha while hover is outside the source list; got {ghost_texts:?}"
            );

            let alpha = find_draggable(&tree, "tree-inbox:Alpha").expect("source island");
            let beta = find_draggable(&tree, "tree-inbox:Beta").expect("origin sibling");
            let gamma = find_draggable(&tree, "tree-later:Gamma").expect("target sibling");
            let later = find_droppable(&tree, "tree-later").expect("tree-later");
            let preview = find_hole_preview(later).expect("Later hole preview");
            let inbox_b = find_draggable(&tree, "folder:Inbox")
                .expect("Inbox")
                .layout_bounds;
            assert!(
                alpha.style.opacity < 0.05,
                "cross-list source island stays in origin at 0; opacity={}",
                alpha.style.opacity
            );
            assert!(
                (preview.style.opacity - 0.4).abs() < 0.05,
                "target hole preview must stay dimmed (0.4); opacity={}",
                preview.style.opacity
            );
            assert!(
                preview.layout_bounds[3] > 8.0,
                "hole preview must keep a paint box; height={}",
                preview.layout_bounds[3]
            );
            assert!(
                beta.style.translate[1].abs() < 2.0,
                "origin sibling must not FLIP up into the folder header after the source left the pack; translate={:?}",
                beta.style.translate
            );
            assert!(
                !crate::utils::motion::motion_is_animating(&beta.style, &beta.transition_snapshot),
                "origin sibling must snap when the source leaves the pack; leftover FLIP eases from the Inbox top"
            );
            assert!(
                beta.style.transition.is_empty(),
                "origin sibling must not keep a transform FLIP across lists"
            );
            let beta_visual_y = beta.layout_bounds[1] + beta.style.translate[1];
            assert!(
                beta_visual_y >= inbox_b[1] + 16.0,
                "Beta must stay below the Inbox title; visual_y={beta_visual_y} inbox_y={} translate={:?}",
                inbox_b[1],
                beta.style.translate
            );
            let preview_top = preview_visual_y(preview);
            let preview_bot = preview_top + preview.layout_bounds[3];
            let gamma_vis_top = gamma.layout_bounds[1] + gamma.style.translate[1];
            let hole_gap = gamma_vis_top - preview_bot;
            assert!(
                (hole_gap - 6.0).abs() < 2.5,
                "hole preview and Gamma must keep the author list gap; gap={hole_gap} \
                 preview=[{preview_top}, {preview_bot}] gamma_vis_top={gamma_vis_top} \
                 preview_b={:?} gamma_t={:?}",
                preview.layout_bounds,
                gamma.style.translate
            );

            let alpha_shell = find_item_shell(&tree, "tree-inbox:Alpha").expect("Alpha shell");
            assert!(
                shell_block_collapsed(alpha_shell),
                "cross-list source row must leave the origin pack by collapsing the shell; size={:?} max={:?}",
                alpha_shell.style.size.height,
                alpha_shell.style.max_size.height
            );

            let inbox_list = find_droppable(&tree, "tree-inbox").expect("tree-inbox");
            let inbox_margin =
                crate::layout::lpa_length_px(inbox_list.style.margin.bottom).unwrap_or(0.0);
            assert!(
                inbox_margin >= -1.0,
                "origin list must keep author margin (negative margin eats folder padding); margin={inbox_margin}"
            );
            assert!(
                matches!(dim_kind(inbox_list.style.size.height), DimKind::Auto),
                "origin nested list must stay height:auto; capping clips the last file"
            );
        }

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            inbox.peek(),
            vec!["Beta".to_string()],
            "drop on Later must leave Inbox, not append Alpha to the source tail"
        );
        assert!(
            later.peek().iter().any(|s| s == "Alpha"),
            "Alpha must land in Later; later={:?}",
            later.peek()
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Last inner file (Beta) is the row a list-height cap would clip. Cross-list
    /// hover must keep the dimmed body (same 0.4 as Alpha — not a per-item hide)
    /// and vacate via the source shell's trailing margin.
    #[test]
    fn nested_folder_last_file_cross_list_keeps_dimmed_source() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::layout::{dim_kind, DimKind};
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let later = Signal::new(vec!["Gamma".to_string()]);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            move || {
                drag_scope("ov-nest-folder-beta")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let mut dst = if to == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "tree-inbox" {
                                inbox.set(src);
                                later.set(dst);
                            } else {
                                later.set(src);
                                inbox.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let inbox = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(220.0).children([inbox, later, overlay]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-folder-beta",
            "tree-inbox:Beta",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-folder-beta".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let later_hit = hits0
            .iter()
            .find(|h| h.id == "tree-later")
            .expect("later list");
        let dest = [
            later_hit.rect.x + 20.0,
            later_hit.rect.y + later_hit.rect.height * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            hoist_and_layout(&mut tree);
        }

        {
            let tree = live.borrow();
            let beta = find_draggable(&tree, "tree-inbox:Beta").expect("source island");
            let later = find_droppable(&tree, "tree-later").expect("tree-later");
            let preview = find_hole_preview(later).expect("Later hole preview");
            assert!(
                beta.style.opacity < 0.05,
                "cross-list source island stays in origin at 0; opacity={}",
                beta.style.opacity
            );
            assert!(
                (preview.style.opacity - 0.4).abs() < 0.05,
                "Beta hole preview uses source_opacity 0.4; opacity={}",
                preview.style.opacity
            );
            assert!(
                preview.layout_bounds[3] > 8.0,
                "last-item hole preview must keep a paint box; height={}",
                preview.layout_bounds[3]
            );

            let beta_shell = find_item_shell(&tree, "tree-inbox:Beta").expect("Beta shell");
            assert!(
                shell_block_collapsed(beta_shell),
                "last file must leave the origin pack by collapsing the shell; size={:?} max={:?}",
                beta_shell.style.size.height,
                beta_shell.style.max_size.height
            );

            let inbox_list = find_droppable(&tree, "tree-inbox").expect("tree-inbox");
            assert!(
                matches!(dim_kind(inbox_list.style.size.height), DimKind::Auto),
                "must not cap the inner list height (that clips Beta)"
            );

            let alpha = find_draggable(&tree, "tree-inbox:Alpha").expect("Alpha");
            let alpha_h = alpha.layout_bounds[3];
            assert!(
                alpha_h > 24.0,
                "remaining Alpha must keep its chip height; height={alpha_h}"
            );
            let inbox_h = find_draggable(&tree, "folder:Inbox")
                .expect("Inbox")
                .layout_bounds[3];
            // folder_card: p=8*2 + header 20 + gap 6 + remaining Alpha
            let min_folder = 8.0 + 20.0 + 6.0 + alpha_h + 8.0;
            assert!(
                inbox_h + 1.0 >= min_folder,
                "Inbox must wrap padding and remaining Alpha; inbox={inbox_h} need>={min_folder} alpha={alpha_h}"
            );
            let inbox_b = find_draggable(&tree, "folder:Inbox")
                .expect("Inbox")
                .layout_bounds;
            let alpha_b = alpha.layout_bounds;
            let alpha_bottom = alpha_b[1] + alpha_b[3];
            let inbox_bottom = inbox_b[1] + inbox_b[3];
            assert!(
                alpha_b[1] + 1.0 >= inbox_b[1] && alpha_bottom <= inbox_bottom + 1.0,
                "Alpha must sit inside the Inbox card; inbox={inbox_b:?} alpha={alpha_b:?}"
            );
        }

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            inbox.peek(),
            vec!["Alpha".to_string()],
            "drop on Later must leave Alpha in Inbox; inbox={:?}",
            inbox.peek()
        );
        assert!(
            later.peek().iter().any(|s| s == "Beta"),
            "Beta must land in Later; later={:?}",
            later.peek()
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery nested Inbox: drag Beta onto Later, then back onto Inbox.
    /// Live preview must vacate / restore **one packed row** (chip + author
    /// gap). Collapsing the source shell to 0 while leaving it in a gapped
    /// flex pack, or padding the target by chip height alone, leaves a hole
    /// under Alpha and under-expands Inbox / under-shrinks Later on the way
    /// back (docs/bugs.md nested-list drag).
    #[test]
    fn nested_folder_beta_roundtrip_preserves_row_pitch() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::layout::lp_length_px;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_scope, sortable_item, sortable_list, start_geometry, tick,
            Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        fn relayout(tree: &mut Element) {
            sync_chrome(tree);
            hoist_and_layout(tree);
        }

        fn bounds(tree: &Element, id: &str) -> [f32; 4] {
            find_draggable(tree, id)
                .or_else(|| find_droppable(tree, id))
                .unwrap_or_else(|| panic!("missing {id}"))
                .layout_bounds
        }

        fn h_of(b: [f32; 4]) -> f32 {
            b[3]
        }
        fn bottom(b: [f32; 4]) -> f32 {
            b[1] + b[3]
        }

        let (_o, live) = mount_scope_tree({
            move || {
                drag_scope("ov-nest-pitch")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .child(move || {
                        let inbox_card = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later_card = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        Element::from(
                            vstack()
                                .w(220.0)
                                .gap(8.0)
                                .children([inbox_card, later_card]),
                        )
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let (h0_inbox, h0_later, rest_below_beta, row_pitch, alpha0, beta0, inbox0) = {
            let tree = live.borrow();
            let inbox_b = bounds(&tree, "folder:Inbox");
            let later_b = bounds(&tree, "folder:Later");
            let alpha_b = bounds(&tree, "tree-inbox:Alpha");
            let beta_b = bounds(&tree, "tree-inbox:Beta");
            let list_b = find_droppable(&tree, "tree-inbox")
                .expect("tree-inbox")
                .layout_bounds;
            (
                h_of(inbox_b),
                h_of(later_b),
                bottom(list_b) - bottom(beta_b),
                beta_b[1] - alpha_b[1],
                alpha_b,
                beta_b,
                inbox_b,
            )
        };
        assert!(
            row_pitch > 32.0,
            "precondition: Alpha→Beta stride includes author gap; pitch={row_pitch}"
        );

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-pitch",
            "tree-inbox:Beta",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-pitch".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let later_hit = hits0
            .iter()
            .find(|h| h.id == "tree-later")
            .expect("later list");
        let dest_later = [
            later_hit.rect.x + 20.0,
            later_hit.rect.y + later_hit.rect.height * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest_later[0], dest_later[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            relayout(&mut tree);
        }

        let (h1_inbox, h1_later, below_alpha, later_pad, alpha1, beta1, inbox1) = {
            let tree = live.borrow();
            let inbox_b = bounds(&tree, "folder:Inbox");
            let later_b = bounds(&tree, "folder:Later");
            let alpha_b = bounds(&tree, "tree-inbox:Alpha");
            let beta_b = bounds(&tree, "tree-inbox:Beta");
            let list_b = find_droppable(&tree, "tree-inbox")
                .expect("tree-inbox")
                .layout_bounds;
            let pad = lp_length_px(
                find_droppable(&tree, "tree-later")
                    .expect("tree-later")
                    .style
                    .padding
                    .bottom,
            )
            .unwrap_or(0.0);
            let inbox_list = find_droppable(&tree, "tree-inbox").expect("tree-inbox");
            assert_eq!(
                inbox_list.style.position,
                taffy::style::Position::Relative,
                "origin list must be the containing block for the vacated shell"
            );
            (
                h_of(inbox_b),
                h_of(later_b),
                bottom(list_b) - bottom(alpha_b),
                pad,
                alpha_b,
                beta_b,
                inbox_b,
            )
        };

        assert!(
            (beta1[2] - beta0[2]).abs() < 2.0,
            "source island width must keep the list slot, not fill the folder card; \
             rest_w={} live_w={} inbox0={inbox0:?} inbox1={inbox1:?} beta0={beta0:?} beta1={beta1:?}",
            beta0[2],
            beta1[2]
        );
        assert!(
            beta1[1] > inbox1[1] + 16.0,
            "source island must stay in the file pack, not sit on the folder header; \
             beta_y={} inbox_y={} alpha_y={} beta1={beta1:?} inbox1={inbox1:?}",
            beta1[1],
            inbox1[1],
            alpha1[1]
        );
        assert!(
            h1_later + 1.0 >= h0_later,
            "Later must not collapse when a file hovers it; later_h0={h0_later} later_h1={h1_later}"
        );
        assert!(
            (below_alpha - rest_below_beta).abs() < 2.0,
            "after Beta leaves, space below Alpha must match rest space below Beta; \
             below_alpha={below_alpha} rest_below_beta={rest_below_beta} \
             leftover={} inbox_h0={h0_inbox} inbox_h1={h1_inbox} \
             later_h0={h0_later} later_h1={h1_later} later_pad={later_pad} \
             row_pitch={row_pitch} alpha0={alpha0:?} alpha1={alpha1:?}",
            below_alpha - rest_below_beta
        );
        assert!(
            ((h0_inbox - h1_inbox) - row_pitch).abs() < 2.0,
            "Inbox must shrink by one packed row when Beta hovers Later; \
             shrink={} row_pitch={row_pitch} inbox_h0={h0_inbox} inbox_h1={h1_inbox} \
             later_pad={later_pad}",
            h0_inbox - h1_inbox
        );
        assert!(
            ((h1_later - h0_later) - row_pitch).abs() < 2.0,
            "Later must grow by one packed row when Beta hovers it; \
             grow={} row_pitch={row_pitch} later_h0={h0_later} later_h1={h1_later} \
             later_pad={later_pad}",
            h1_later - h0_later
        );

        let hits2 = collect_hits(&live.borrow());
        let inbox_hit = hits2
            .iter()
            .find(|h| h.id == "tree-inbox")
            .expect("inbox list after vacate");
        let dest_inbox = [
            inbox_hit.rect.x + 20.0,
            inbox_hit.rect.y + inbox_hit.rect.height.max(8.0) * 0.5,
        ];
        assert!(tick(dest_inbox[0], dest_inbox[1], &hits2, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            relayout(&mut tree);
        }

        let (h2_inbox, h2_later) = {
            let tree = live.borrow();
            (
                h_of(bounds(&tree, "folder:Inbox")),
                h_of(bounds(&tree, "folder:Later")),
            )
        };
        let inbox_expand = h2_inbox - h1_inbox;
        let later_shrink = h1_later - h2_later;
        assert!(
            (inbox_expand - row_pitch).abs() < 2.0,
            "returning Beta to Inbox must expand Inbox by one packed row; \
             expand={inbox_expand} row_pitch={row_pitch} \
             h0={h0_inbox} h1={h1_inbox} h2={h2_inbox} \
             later_h0={h0_later} later_h1={h1_later} later_h2={h2_later}"
        );
        assert!(
            (later_shrink - row_pitch).abs() < 2.0,
            "returning Beta to Inbox must shrink Later by one packed row; \
             shrink={later_shrink} row_pitch={row_pitch} \
             later_h0={h0_later} later_h1={h1_later} later_h2={h2_later} \
             inbox_h0={h0_inbox} inbox_h1={h1_inbox} inbox_h2={h2_inbox}"
        );

        let _ = crate::utils::drag::end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery: drag Beta under Later/Gamma (append, not committed). The
    /// dimmed body must keep the author list gap — chip-only append sits flush.
    #[test]
    fn nested_folder_beta_under_gamma_keeps_author_gap() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        fn relayout(tree: &mut Element) {
            sync_chrome(tree);
            hoist_and_layout(tree);
        }

        let (_o, live) = mount_scope_tree({
            move || {
                drag_scope("ov-nest-under-gamma")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .animation(false)
                    .child(move || {
                        let inbox_card = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later_card = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        Element::from(
                            vstack()
                                .w(220.0)
                                .gap(8.0)
                                .children([inbox_card, later_card, overlay]),
                        )
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let (row_pitch, hits0) = {
            let tree = live.borrow();
            let alpha = find_draggable(&tree, "tree-inbox:Alpha")
                .expect("Alpha")
                .layout_bounds;
            let beta = find_draggable(&tree, "tree-inbox:Beta")
                .expect("Beta")
                .layout_bounds;
            (beta[1] - alpha[1], collect_hits(&tree))
        };
        assert!(
            row_pitch > 32.0,
            "precondition: Inbox stride includes author gap; pitch={row_pitch}"
        );

        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-under-gamma",
            "tree-inbox:Beta",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-under-gamma".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let gamma0 = hits0
            .iter()
            .find(|h| h.id == "tree-later:Gamma")
            .expect("Gamma");
        let dest_under = [
            gamma0.rect.x + 20.0,
            gamma0.rect.y + gamma0.rect.height * 0.75,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest_under[0], dest_under[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            relayout(&mut tree);
        }
        let hits2 = collect_hits(&live.borrow());
        assert!(tick(dest_under[0], dest_under[1], &hits2, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            relayout(&mut tree);
        }

        {
            let tree = live.borrow();
            let later = find_droppable(&tree, "tree-later").expect("tree-later");
            let gamma = find_draggable(&tree, "tree-later:Gamma").expect("Gamma");
            let preview = find_hole_preview(later).expect("Later hole preview");
            let gamma_bottom =
                gamma.layout_bounds[1] + gamma.style.translate[1] + gamma.layout_bounds[3];
            let preview_top = preview_visual_y(preview);
            let gap = preview_top - gamma_bottom;
            let author_gap = row_pitch - 32.0;
            assert!(
                (gap - author_gap).abs() < 2.0,
                "hole preview under Gamma must keep author gap; \
                 gap={gap} author_gap={author_gap} row_pitch={row_pitch} \
                 gamma={:?} g_tr={:?} preview={:?}",
                gamma.layout_bounds,
                gamma.style.translate,
                preview.layout_bounds
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery: Later/Gamma onto Inbox above Alpha. Source Later is a singleton
    /// (chip-only pitch). The hole must keep Inbox's author gap, not sit flush.
    #[test]
    fn nested_folder_gamma_above_inbox_item_keeps_author_gap() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let (_o, live) = mount_scope_tree({
            move || {
                drag_scope("ov-nest-gamma-above-alpha")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .animation(false)
                    .child(move || {
                        let inbox_card = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later_card = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        Element::from(
                            vstack()
                                .w(220.0)
                                .gap(8.0)
                                .children([inbox_card, later_card, overlay]),
                        )
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let (row_pitch, hits0) = {
            let tree = live.borrow();
            let alpha = find_draggable(&tree, "tree-inbox:Alpha")
                .expect("Alpha")
                .layout_bounds;
            let beta = find_draggable(&tree, "tree-inbox:Beta")
                .expect("Beta")
                .layout_bounds;
            (beta[1] - alpha[1], collect_hits(&tree))
        };
        let author_gap = row_pitch - 32.0;
        assert!(
            author_gap > 4.0,
            "precondition: Inbox has an author gap; pitch={row_pitch} gap={author_gap}"
        );

        let src = hits0
            .iter()
            .find(|h| h.id == "tree-later:Gamma")
            .expect("Gamma");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-gamma-above-alpha",
            "tree-later:Gamma",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-gamma-above-alpha".into(),
            draggable_id: "tree-later:Gamma".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-later".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let alpha0 = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let dest = [
            alpha0.rect.x + 20.0,
            alpha0.rect.y + alpha0.rect.height * 0.25,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            hoist_and_layout(&mut tree);
        }

        {
            let tree = live.borrow();
            let inbox = find_droppable(&tree, "tree-inbox").expect("tree-inbox");
            let alpha = find_draggable(&tree, "tree-inbox:Alpha").expect("Alpha");
            let preview = find_hole_preview(inbox).expect("Inbox hole preview");
            let preview_bot = preview_visual_y(preview) + preview.layout_bounds[3];
            let alpha_top = alpha.layout_bounds[1] + alpha.style.translate[1];
            let gap = alpha_top - preview_bot;
            assert!(
                (gap - author_gap).abs() < 2.0,
                "Gamma hole preview above Alpha must keep Inbox author gap; \
                 gap={gap} author_gap={author_gap} row_pitch={row_pitch} \
                 preview={:?} alpha={:?} a_tr={:?}",
                preview.layout_bounds,
                alpha.layout_bounds,
                alpha.style.translate
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Production path: overlay + live hits + chrome + layout.
    /// After Inbox→Later, the target list paints the dimmed body in the hole
    /// on the first paint — holding still does not tick again.
    #[test]
    fn nested_folder_beta_cross_list_dimmed_body_appears_in_later() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let (_o, live) = mount_scope_tree({
            move || {
                drag_scope("ov-nest-body-in-later")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .animation(false)
                    .child(move || {
                        let inbox_card = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .children([
                                                    chip("tree-inbox", "Alpha".into(), 0),
                                                    chip("tree-inbox", "Beta".into(), 1),
                                                ]),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later_card = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Gamma".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        // Gallery hosts the nest inside ScrollArea (`overflow: auto`).
                        // A 0-tall vacated shell is clip-culled and never paints Beta.
                        Element::from(
                            vstack()
                                .w(220.0)
                                .h(400.0)
                                .overflow("hidden")
                                .gap(8.0)
                                .children([inbox_card, later_card, overlay]),
                        )
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-body-in-later",
            "tree-inbox:Beta",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-body-in-later".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let gamma0 = hits0
            .iter()
            .find(|h| h.id == "tree-later:Gamma")
            .expect("Gamma");
        let dest = [
            gamma0.rect.x + 20.0,
            gamma0.rect.y + gamma0.rect.height * 0.75,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let painted = {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
            crate::core::reactivity::with_ui_build_stack(|| {
                let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
                let mut lt = crate::layout::LayoutTree::new();
                lt.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0])
            })
        };

        {
            let tree = live.borrow();
            let later = find_droppable(&tree, "tree-later").expect("tree-later");
            let gamma = find_draggable(&tree, "tree-later:Gamma").expect("Gamma");
            let beta = find_draggable(&tree, "tree-inbox:Beta").expect("source island");
            let preview = find_hole_preview(later).expect("Later hole preview");
            assert!(
                beta.style.opacity < 0.05,
                "cross-list source island stays in origin at 0; opacity={}",
                beta.style.opacity
            );
            assert!(
                (preview.style.opacity - 0.4).abs() < 0.05,
                "hole preview must stay dimmed; opacity={}",
                preview.style.opacity
            );
            let later_top = later.layout_bounds[1];
            let later_bottom = later.layout_bounds[1] + later.layout_bounds[3];
            let preview_top = preview_visual_y(preview);
            let preview_bot = preview_top + preview.layout_bounds[3];
            assert!(
                preview_bot > later_top - 1.0 && preview_top < later_bottom + 1.0,
                "hole preview must sit inside Later; preview={:?} later={:?} gamma={:?}",
                preview.layout_bounds,
                later.layout_bounds,
                gamma.layout_bounds
            );
            let mut preview_texts = Vec::new();
            collect_texts(preview, &mut preview_texts);
            assert!(
                preview_texts.iter().any(|t| t.contains("Beta")),
                "hole preview must clone Beta, not the portal ghost; texts={preview_texts:?}"
            );
            let ghost = find_ghost(&tree).expect("overlay ghost");
            let painted_beta = painted
                .text_nodes
                .iter()
                .filter(|t| t.text.contains("Beta"))
                .count();
            assert!(
                painted_beta >= 2,
                "ghost + hole preview must both paint Beta; count={painted_beta} ghost_z={}",
                ghost.style.z_index
            );
        }

        let _ = end(&collect_hits(&live.borrow()));
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery nested folders: after every file leaves Inbox the inner list is
    /// 0-tall. Dragging a file onto the Inbox header must grow the list, show
    /// the dimmed body, and commit the transfer.
    #[test]
    fn empty_nested_inbox_accepts_file_from_later() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::layout::lp_length_px;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_transfer, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        fn sync_chrome(tree: &mut Element) {
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }

        let inbox = Signal::new(Vec::<String>::new());
        let later = Signal::new(vec!["Beta".to_string()]);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            move || {
                drag_scope("ov-nest-empty-inbox")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .collision(crate::utils::drag::CollisionAlg::MostIntersecting)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        move |ev| {
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            if from == to {
                                return;
                            }
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            let mut src = if from == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let mut dst = if to == "tree-inbox" {
                                inbox.peek()
                            } else {
                                later.peek()
                            };
                            let to_index = ev.to_index.unwrap_or(dst.len());
                            apply_transfer(&mut src, &mut dst, from_index, to_index);
                            if from == "tree-inbox" {
                                inbox.set(src);
                                later.set(dst);
                            } else {
                                later.set(src);
                                inbox.set(dst);
                            }
                        }
                    })
                    .child(move || {
                        let inbox = sortable_item("folder:Inbox")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(0)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-inbox")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack().w("100%").min_h(0.0).h("auto").gap(6.0),
                                        )
                                    });
                                folder_card("Inbox", inner)
                            });
                        let later = sortable_item("folder:Later")
                            .item_type("folder")
                            .accept("folder")
                            .list("tree-root")
                            .index(1)
                            .source_opacity(0.4)
                            .child(|| {
                                let inner = sortable_list("tree-later")
                                    .accept("file")
                                    .group("nest")
                                    .child(|| {
                                        Element::from(
                                            vstack()
                                                .w("100%")
                                                .min_h(0.0)
                                                .h("auto")
                                                .gap(6.0)
                                                .child(chip("tree-later", "Beta".into(), 0)),
                                        )
                                    });
                                folder_card("Later", inner)
                            });
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(220.0).children([inbox, later, overlay]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "tree-later:Beta")
            .expect("Beta");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-empty-inbox",
            "tree-later:Beta",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-empty-inbox".into(),
            draggable_id: "tree-later:Beta".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("tree-later".into()),
            from_index: Some(0),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        let inbox_folder = hits0
            .iter()
            .find(|h| h.id == "folder:Inbox")
            .expect("Inbox folder");
        let dest = [inbox_folder.rect.x + 20.0, inbox_folder.rect.y + 12.0];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_chrome(&mut tree);
        }

        {
            let tree = live.borrow();
            let inbox_list = find_droppable(&tree, "tree-inbox").expect("tree-inbox");
            let pad = lp_length_px(inbox_list.style.padding.bottom).unwrap_or(0.0);
            assert!(
                pad > 16.0,
                "empty Inbox must grow a row of padding for the incoming file; pad={pad}"
            );
            let beta = find_draggable(&tree, "tree-later:Beta").expect("source island");
            let preview = find_hole_preview(inbox_list).expect("Inbox hole preview");
            assert!(
                beta.style.opacity < 0.05,
                "cross-list source island stays in origin at 0; opacity={}",
                beta.style.opacity
            );
            assert!(
                (preview.style.opacity - 0.4).abs() < 0.05,
                "empty Inbox hole preview must stay dimmed; opacity={}",
                preview.style.opacity
            );
        }

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            inbox.peek(),
            vec!["Beta".to_string()],
            "drop on empty Inbox must insert Beta; inbox={:?}",
            inbox.peek()
        );
        assert!(
            later.peek().is_empty(),
            "Later must be empty after the transfer; later={:?}",
            later.peek()
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Same-list: drop the last chip on the first. Hover slot 0 must commit
    /// `[Beta, Alpha]`, not stick at `from_index`.
    #[test]
    fn same_list_last_chip_drop_on_first_reorders() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_reorder_vec, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        let items = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let (_o, live) = mount_scope_tree({
            let items = items;
            move || {
                drag_scope("ov-same-last-first")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .on_drag_end({
                        let items = items;
                        move |ev| {
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            if ev.from_list.as_deref() != Some("files") {
                                return;
                            }
                            let mut v = items.peek();
                            apply_reorder_vec(
                                &mut v,
                                from_index,
                                ev.to_index.unwrap_or(from_index),
                            );
                            items.set(v);
                        }
                    })
                    .child(move || {
                        let list = sortable_list("files").accept("file").child({
                            let items = items;
                            move || {
                                let rows: Vec<Element> = items
                                    .get()
                                    .iter()
                                    .enumerate()
                                    .map(|(i, name)| {
                                        let label = name.clone();
                                        sortable_item(format!("files:{label}"))
                                            .item_type("file")
                                            .accept("file")
                                            .list("files")
                                            .index(i)
                                            .source_opacity(0.4)
                                            .child(move || {
                                                Element::new().text(label.clone()).w(160.0).h(32.0)
                                            })
                                    })
                                    .collect();
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            }
                        });
                        let overlay: Element = drag_overlay().into();
                        Element::from(vstack().w(200.0).children([list, overlay]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let beta = hits0.iter().find(|h| h.id == "files:Beta").expect("Beta");
        let alpha = hits0.iter().find(|h| h.id == "files:Alpha").expect("Alpha");
        let grab = [
            beta.rect.x + beta.rect.width * 0.5,
            beta.rect.y + beta.rect.height * 0.5,
        ];
        let dest = [
            alpha.rect.x + alpha.rect.width * 0.5,
            alpha.rect.y + alpha.rect.height * 0.25,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-same-last-first",
            "files:Beta",
            grab,
            [beta.rect.x, beta.rect.y, beta.rect.width, beta.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-same-last-first".into(),
            draggable_id: "files:Beta".into(),
            pointer: grab,
            grab_offset,
            start_bounds,
            from_list: Some("files".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(grab[0] + 1.0, grab[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            items.peek(),
            vec!["Beta".to_string(), "Alpha".to_string()],
            "dropping Beta on Alpha's upper half must reorder, not stay [Alpha, Beta]; got {:?}",
            items.peek()
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery nested Inbox: drag Beta onto Alpha's upper half. Preview already
    /// stacks Beta above Alpha; drop must commit `[Beta, Alpha]`, not snap back.
    #[test]
    fn nested_folder_last_file_drop_on_first_reorders() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_reorder_vec, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn chip(list: &'static str, name: String, idx: usize) -> Element {
            let label = name.clone();
            sortable_item(format!("{list}:{label}"))
                .item_type("file")
                .accept("file")
                .group("nest")
                .list(list)
                .index(idx)
                .source_opacity(0.4)
                .child(move || Element::new().text(label.clone()).w(160.0).h(32.0))
        }

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let later = Signal::new(vec!["Gamma".to_string()]);
        let dropped = Signal::new(None::<(Option<String>, Option<usize>)>);

        let (_o, live) = mount_scope_tree({
            let inbox = inbox;
            let later = later;
            let dropped = dropped;
            move || {
                drag_scope("ov-nest-reorder")
                    .threshold(0.0)
                    .axis(Axis::Y)
                    .collision(crate::utils::drag::CollisionAlg::MostIntersecting)
                    .on_drag_end({
                        let inbox = inbox;
                        let later = later;
                        let dropped = dropped;
                        move |ev| {
                            dropped.set(Some((ev.to_list.clone(), ev.to_index)));
                            let Some(from) = ev.from_list.as_deref() else {
                                return;
                            };
                            let Some(to) = ev.to_list.as_deref() else {
                                return;
                            };
                            let Some(from_index) = ev.from_index else {
                                return;
                            };
                            if from == to {
                                let mut v = if from == "tree-inbox" {
                                    inbox.peek()
                                } else {
                                    later.peek()
                                };
                                apply_reorder_vec(
                                    &mut v,
                                    from_index,
                                    ev.to_index.unwrap_or(from_index),
                                );
                                if from == "tree-inbox" {
                                    inbox.set(v);
                                } else {
                                    later.set(v);
                                }
                                return;
                            }
                        }
                    })
                    .child(move || {
                        let overlay: Element = drag_overlay().into();
                        let tree = sortable_list("tree-root").accept("folder").child(|| {
                            let inbox_card = sortable_item("folder:Inbox")
                                .item_type("folder")
                                .accept("folder")
                                .list("tree-root")
                                .index(0)
                                .source_opacity(0.4)
                                .child(|| {
                                    let inner = sortable_list("tree-inbox")
                                        .accept("file")
                                        .group("nest")
                                        .child(|| {
                                            Element::from(
                                                vstack()
                                                    .w("100%")
                                                    .min_h(0.0)
                                                    .h("auto")
                                                    .gap(6.0)
                                                    .children([
                                                        chip("tree-inbox", "Alpha".into(), 0),
                                                        chip("tree-inbox", "Beta".into(), 1),
                                                    ]),
                                            )
                                        });
                                    folder_card("Inbox", inner)
                                });
                            let later_card = sortable_item("folder:Later")
                                .item_type("folder")
                                .accept("folder")
                                .list("tree-root")
                                .index(1)
                                .source_opacity(0.4)
                                .child(|| {
                                    let inner = sortable_list("tree-later")
                                        .accept("file")
                                        .group("nest")
                                        .child(|| {
                                            Element::from(
                                                vstack()
                                                    .w("100%")
                                                    .min_h(0.0)
                                                    .h("auto")
                                                    .gap(6.0)
                                                    .child(chip("tree-later", "Gamma".into(), 0)),
                                            )
                                        });
                                    folder_card("Later", inner)
                                });
                            Element::from(vstack().w("100%").children([inbox_card, later_card]))
                        });
                        Element::from(vstack().w(220.0).children([tree, overlay]))
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let beta = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let alpha = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let folder = hits0
            .iter()
            .find(|h| h.id == "folder:Inbox")
            .expect("Inbox folder");
        let grab = [
            beta.rect.x + beta.rect.width * 0.5,
            beta.rect.y + beta.rect.height * 0.5,
        ];
        // Release on the folder title band (Gallery screenshot: Beta overlapping Inbox).
        let dest = [
            alpha.rect.x + alpha.rect.width * 0.5,
            (folder.rect.y + alpha.rect.y) * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-reorder",
            "tree-inbox:Beta",
            grab,
            [beta.rect.x, beta.rect.y, beta.rect.width, beta.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-reorder".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer: grab,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(grab[0] + 1.0, grab[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = crate::core::tree::LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
        }
        {
            let tree = live.borrow();
            let src = find_draggable(&tree, "tree-inbox:Beta").expect("Beta source");
            assert!(
                src.style.translate[1] < -8.0,
                "preview must lift Beta onto Alpha's slot; translate={:?}",
                src.style.translate
            );
        }
        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            dropped.peek(),
            Some((Some("tree-inbox".into()), Some(0))),
            "drop on Alpha's upper half must commit hover slot 0, not from_index; got {:?}",
            dropped.peek()
        );
        assert_eq!(
            inbox.peek(),
            vec!["Beta".to_string(), "Alpha".to_string()],
            "Inbox must keep Beta above Alpha after drop; got {:?}",
            inbox.peek()
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn collect_list_draggable_ids(el: &Element, list: &str, out: &mut Vec<String>) {
        if el.attrs.contains_key(ATTR_DRAG_OVERLAY) {
            return;
        }
        if el
            .attrs
            .get(crate::utils::drag::ATTR_DRAG_LIST)
            .map(String::as_str)
            == Some(list)
            && el
                .attrs
                .contains_key(crate::utils::drag::ATTR_DRAG_DRAGGABLE)
            && el.attrs.contains_key(crate::utils::drag::ATTR_DRAG_INDEX)
        {
            if let Some(id) = el.attrs.get(crate::utils::drag::ATTR_DRAG_DRAGGABLE) {
                out.push(id.clone());
            }
            return;
        }
        for c in &el.children {
            collect_list_draggable_ids(c, list, out);
        }
    }

    /// Gallery `keyed_sortable_chips` + nested folder: after drop the live tree
    /// (not only the Vec) must pack Beta above Alpha. Static `chip(..., idx)`
    /// tests miss this — they never rebuild children.
    #[test]
    fn nested_folder_for_keyed_drop_reorders_dom_and_layout() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutTree;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_reorder_vec, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, Axis, BeginDrag,
        };

        fn folder_card(title: &str, inner: Element) -> Element {
            Element::from(
                vstack()
                    .w("100%")
                    .min_h(0.0)
                    .h("auto")
                    .gap(6.0)
                    .p(8.0)
                    .child(Element::new().text(title.to_string()).h(20.0))
                    .child(inner),
            )
        }

        let inbox = Signal::new(vec!["Alpha".to_string(), "Beta".to_string()]);
        let list_runs = std::rc::Rc::new(std::cell::Cell::new(0_u32));

        let (_o, live) =
            mount_scope_tree({
                let inbox = inbox;
                let list_runs = list_runs.clone();
                move || {
                    drag_scope("ov-nest-fk")
                        .threshold(0.0)
                        .axis(Axis::Y)
                        .collision(crate::utils::drag::CollisionAlg::MostIntersecting)
                        .on_drag_end({
                            let inbox = inbox;
                            move |ev| {
                                if ev.from_list.as_deref() != Some("tree-inbox") {
                                    return;
                                }
                                if ev.to_list.as_deref() != Some("tree-inbox") {
                                    return;
                                }
                                let Some(from_index) = ev.from_index else {
                                    return;
                                };
                                let mut v = inbox.peek();
                                apply_reorder_vec(
                                    &mut v,
                                    from_index,
                                    ev.to_index.unwrap_or(from_index),
                                );
                                inbox.set(v);
                            }
                        })
                        .child({
                            let list_runs = list_runs.clone();
                            move || {
                                let overlay: Element = drag_overlay().into();
                                let tree =
                                    sortable_list("tree-root").accept("folder").child({
                                        let inbox = inbox;
                                        let list_runs = list_runs.clone();
                                        move || {
                                            sortable_item("folder:Inbox")
                                                .item_type("folder")
                                                .accept("folder")
                                                .list("tree-root")
                                                .index(0)
                                                .source_opacity(0.4)
                                                .child({
                                                    let inbox = inbox;
                                                    let list_runs = list_runs.clone();
                                                    move || {
                                                        let inner = sortable_list("tree-inbox")
                                                .accept("file")
                                                .group("nest")
                                                .child({
                                                    let inbox = inbox;
                                                    let list_runs = list_runs.clone();
                                                    move || {
                                                        list_runs.set(list_runs.get() + 1);
                                                        let items = inbox.get();
                                                        let rows = crate::app::reactive::for_keyed(
                                                            "ov-nest-fk-inbox-rows",
                                                            items.as_slice(),
                                                            |s: &String| s.clone(),
                                                            |label: &String| {
                                                                let name = label.clone();
                                                                let i = items
                                                                    .iter()
                                                                    .position(|x| x == &name)
                                                                    .unwrap_or(0);
                                                                sortable_item(format!(
                                                                    "tree-inbox:{name}"
                                                                ))
                                                                .item_type("file")
                                                                .accept("file")
                                                                .group("nest")
                                                                .list("tree-inbox")
                                                                .index(i)
                                                                .source_opacity(0.4)
                                                                .child(move || {
                                                                    Element::new()
                                                                        .text(name.clone())
                                                                        .w(160.0)
                                                                        .h(32.0)
                                                                })
                                                            },
                                                        );
                                                        Element::from(
                                                            vstack()
                                                                .w("100%")
                                                                .min_h(0.0)
                                                                .h("auto")
                                                                .gap(6.0)
                                                                .children(rows),
                                                        )
                                                    }
                                                });
                                                        folder_card("Inbox", inner)
                                                    }
                                                })
                                        }
                                    });
                                Element::from(vstack().w(220.0).children([tree, overlay]))
                            }
                        })
                }
            });

        let mut lt = LayoutTree::new();
        {
            let mut tree = live.borrow_mut();
            let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut tree);
            let _ = lt.build_and_layout(&mut tree, 800.0, 600.0, [0.0, 0.0]);
        }

        let hits0 = collect_hits(&live.borrow());
        let beta = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Beta")
            .expect("Beta");
        let alpha = hits0
            .iter()
            .find(|h| h.id == "tree-inbox:Alpha")
            .expect("Alpha");
        let grab = [
            beta.rect.x + beta.rect.width * 0.5,
            beta.rect.y + beta.rect.height * 0.5,
        ];
        let dest = [
            alpha.rect.x + alpha.rect.width * 0.5,
            alpha.rect.y + alpha.rect.height * 0.25,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "ov-nest-fk",
            "tree-inbox:Beta",
            grab,
            [beta.rect.x, beta.rect.y, beta.rect.width, beta.rect.height],
        );
        begin(BeginDrag {
            scope_id: "ov-nest-fk".into(),
            draggable_id: "tree-inbox:Beta".into(),
            pointer: grab,
            grab_offset,
            start_bounds,
            from_list: Some("tree-inbox".into()),
            from_index: Some(1),
            item_type: "file".into(),
            group: "nest".into(),
            capture_id: "cap".into(),
        });
        assert!(tick(grab[0] + 1.0, grab[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(dest[0], dest[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        drain_deferred_view_remounts();

        assert_eq!(
            inbox.peek(),
            vec!["Beta".to_string(), "Alpha".to_string()],
            "signal must commit [Beta, Alpha]; got {:?}",
            inbox.peek()
        );
        assert!(
            list_runs.get() >= 2,
            "droppable body must rebuild after inbox.set; runs={}",
            list_runs.get()
        );

        let mut ids = Vec::new();
        collect_list_draggable_ids(&live.borrow(), "tree-inbox", &mut ids);
        assert_eq!(
            ids,
            vec![
                "tree-inbox:Beta".to_string(),
                "tree-inbox:Alpha".to_string()
            ],
            "for_keyed live tree must list Beta before Alpha; got {ids:?}"
        );

        {
            let mut tree = live.borrow_mut();
            lt.build_and_layout_fast_ex(&mut tree, 800.0, 600.0, [0.0, 0.0], false, false, None);
            let beta = find_draggable(&tree, "tree-inbox:Beta").expect("Beta after drop");
            let alpha = find_draggable(&tree, "tree-inbox:Alpha").expect("Alpha after drop");
            assert!(
                beta.layout_bounds[1] < alpha.layout_bounds[1],
                "fast layout must pack Beta above Alpha; Beta y={} Alpha y={}",
                beta.layout_bounds[1],
                alpha.layout_bounds[1]
            );
        }

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery `kanban_column`: paint `reactive_sync` must see `hover_list`
    /// after Todo→Doing. Origin stays at rest. `use_drag()` walks `current_owner`;
    /// the frame prepass does not restore the provider owner — capture `DragCtx`
    /// at column build (same as `apply_live_droppable_chrome`).
    #[test]
    fn kanban_column_highlight_todo_to_doing() {
        use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::hstack;
        use crate::style::parse_color;
        use crate::utils::drag::{
            begin, drag_scope, droppable, end, tick, use_drag, BeginDrag, DragHit, ATTR_DRAG_OVER,
        };

        fn column(list: &'static str) -> Element {
            let ctx = use_drag();
            let mut el = Element::new().w(80.0).min_h(128.0);
            el.style.bg_color = parse_color("muted");
            el.style.border_color = parse_color("border");
            el.reactive_sync(move |el, _| {
                let over = ctx.as_ref().and_then(|c| c.hover_list.peek()).as_deref() == Some(list);
                el.style.bg_color = parse_color(if over { "accent" } else { "muted" });
                el.style.border_color = parse_color(if over { "primary" } else { "border" });
                LayoutStatus::StyleChanged
            })
        }

        let (_o, live) = mount_scope_tree(move || {
            drag_scope("gal-drag-board").threshold(0.0).child(move || {
                let left = droppable("todo")
                    .list("todo")
                    .group("board")
                    .accept("task")
                    .child(|| column("todo"));
                let right = droppable("doing")
                    .list("doing")
                    .group("board")
                    .accept("task")
                    .child(|| column("doing"));
                Element::from(hstack().gap(8.0).children([left, right]))
            })
        });

        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "task-a".into(),
            pointer: [40.0, 16.0],
            grab_offset: [10.0, 8.0],
            start_bounds: [0.0, 0.0, 80.0, 32.0],
            from_list: Some("todo".into()),
            from_index: Some(0),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hit = |id: &str, list: &str, x: f32, index: Option<usize>| DragHit {
            id: id.into(),
            scope_id: "gal-drag-board".into(),
            is_draggable: index.is_some(),
            is_droppable: true,
            rect: crate::utils::drag::Rect::from_bounds([x, 0.0, 80.0, 128.0]),
            item_type: "task".into(),
            accept: "task".into(),
            list_id: Some(list.into()),
            index,
            group: "board".into(),
            disabled: false,
            list_axis: None,
            skip_shift: false,
            skip_follow: false,
            pull_clone: false,
        };
        let hits = [
            hit("task-a", "todo", 0.0, Some(0)),
            hit("todo", "todo", 0.0, None),
            hit("doing", "doing", 88.0, None),
        ];
        assert!(tick(41.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();
        assert!(tick(128.0, 16.0, &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            assert!(
                crate::core::reactivity::current_owner().is_none(),
                "paint prepass has no component owner — same as AppState::render"
            );
            let doing = find_droppable(&tree, "doing").expect("doing");
            assert_eq!(
                doing.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "engine chrome must mark the hover column; collision is not the bug"
            );
            assert_eq!(
                doing.style.bg_color,
                parse_color("accent"),
                "Doing column must highlight from hover_list at paint; got {:?}",
                doing.style.bg_color
            );
            assert_eq!(doing.style.border_color, parse_color("primary"));
            let todo = find_droppable(&tree, "todo").expect("todo");
            assert_ne!(
                todo.attrs.get(ATTR_DRAG_OVER).map(String::as_str),
                Some("1"),
                "origin column is not the drop target"
            );
            assert_eq!(
                todo.style.border_color,
                parse_color("border"),
                "Todo origin column stays at rest; only the hover target highlights"
            );
            assert_eq!(todo.style.bg_color, parse_color("muted"));
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery Kanban Todo: `Droppable` + `for_keyed` + `w=100%` also_droppable
    /// islands. Lift Review PR must keep hover=from and the dimmed body must not
    /// translate (no yield to the overlay clone).
    #[test]
    fn kanban_todo_lift_review_pr_keeps_source_slot() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::{hstack, vstack};
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, droppable, end, insert_index_along_axis,
            sortable_item, start_geometry, tick, Axis, BeginDrag, CollisionAlg,
        };

        const NAMES: [&str; 2] = ["Draft RFC", "Review PR"];
        let (_o, live) = mount_scope_tree(move || {
            drag_scope("gal-drag-board")
                .collision(CollisionAlg::MostIntersecting)
                .axis(Axis::Y)
                .threshold(0.0)
                .child(move || {
                    let todo = droppable("todo")
                        .list("todo")
                        .group("board")
                        .accept("task")
                        .child(|| {
                            let todo_rows = for_keyed(
                                "gal-drag-todo-rows",
                                &NAMES,
                                |s: &&str| (*s).to_string(),
                                |name: &&str| {
                                    let id = format!("todo:{}", *name);
                                    let i = NAMES.iter().position(|n| n == name).unwrap_or(0);
                                    sortable_item(id)
                                        .item_type("task")
                                        .group("board")
                                        .list("todo")
                                        .index(i)
                                        .child({
                                            let label = (*name).to_string();
                                            move || {
                                                Element::new()
                                                    .text(label.clone())
                                                    .w("100%")
                                                    .h(32.0)
                                                    .min_h(32.0)
                                            }
                                        })
                                },
                            );
                            Element::from(vstack().w("100%").gap(6.0).p(8.0).children(todo_rows))
                        });
                    let doing = droppable("doing")
                        .list("doing")
                        .group("board")
                        .accept("task")
                        .child(|| Element::from(vstack().w("100%").p(8.0).min_h(128.0)));
                    let overlay: Element = drag_overlay().into();
                    Element::from(hstack().gap(8.0).w(400.0).children([todo, doing, overlay]))
                })
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits = collect_hits(&live.borrow());
        let todo_rows: Vec<_> = hits
            .iter()
            .filter(|h| h.list_id.as_deref() == Some("todo") && h.index.is_some())
            .collect();
        assert_eq!(
            todo_rows.len(),
            2,
            "todo column must expose two indexed rows; hits={:?}",
            hits.iter()
                .map(|h| (h.id.clone(), h.index, h.rect.y, h.rect.height))
                .collect::<Vec<_>>()
        );
        let y0 = todo_rows[0].rect.y;
        let y1 = todo_rows[1].rect.y;
        assert!(
            (y1 - y0) >= 8.0,
            "row origins must be distinct (not clustered); y0={y0} y1={y1}"
        );
        for h in &todo_rows {
            assert!(
                h.rect.height > 8.0 && h.rect.height < 80.0,
                "row slot must be chip-sized, not the column shell; {} h={}",
                h.id,
                h.rect.height
            );
        }

        let review = todo_rows
            .iter()
            .find(|h| h.index == Some(1))
            .expect("Review PR row");
        let pointer = [
            review.rect.x + review.rect.width * 0.5,
            review.rect.y + review.rect.height * 0.5,
        ];
        let (grab, start_bounds) = start_geometry(
            "gal-drag-board",
            "todo:Review PR",
            pointer,
            [
                review.rect.x,
                review.rect.y,
                review.rect.width,
                review.rect.height,
            ],
        );

        begin(BeginDrag {
            scope_id: "gal-drag-board".into(),
            draggable_id: "todo:Review PR".into(),
            pointer,
            grab_offset: grab,
            start_bounds,
            from_list: Some("todo".into()),
            from_index: Some(1),
            item_type: "task".into(),
            group: "board".into(),
            capture_id: "cap".into(),
        });
        let hits = collect_hits(&live.borrow());
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits, [800.0, 600.0]));
        drain_deferred_view_remounts();

        assert_eq!(
            insert_index_along_axis(&hits, "todo", pointer, Axis::Y),
            Some(1),
            "insert index must match the lifted row; hits={:?}",
            hits.iter()
                .map(|h| (h.id.clone(), h.index, h.rect.y, h.rect.height))
                .collect::<Vec<_>>()
        );

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let review_el =
                find_draggable_in_scope(&tree, "gal-drag-board", "todo:Review PR").expect("review");
            let draft_el =
                find_draggable_in_scope(&tree, "gal-drag-board", "todo:Draft RFC").expect("draft");
            assert!(
                review_el.style.translate[0].abs() < 1.0
                    && review_el.style.translate[1].abs() < 1.0,
                "dimmed source must not cross-translate or yield at lift; translate={:?}",
                review_el.style.translate
            );
            assert!(
                draft_el.style.translate[1].abs() < 1.0,
                "sibling must not shift at lift; translate={:?}",
                draft_el.style.translate
            );
            assert!(
                (review_el.style.opacity - 0.4).abs() < 0.05,
                "source stays dimmed; opacity={}",
                review_el.style.opacity
            );
        }

        let _ = end(&hits);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery「Sortable 列表 · 半透明本体」: `SortableList` + `for_keyed` +
    /// `DragOverlay` (default source opacity 0.4). Drag must not collapse the source row,
    /// the dimmed body stays in-list, and commit follows the hovered slot —
    /// not always `len` (tail insert).
    #[test]
    fn gallery_sortable_without_hide_source_keeps_dimmed_body() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            apply_reorder_vec, begin, collect_hits, drag_overlay, drag_scope, end, sortable_item,
            sortable_list, start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child(move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets")
                                            .index(i)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    })
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let item_hits: Vec<_> = hits0
            .iter()
            .filter(|h| h.index.is_some() && h.list_id.as_deref() == Some("planets"))
            .cloned()
            .collect();
        assert_eq!(
            item_hits.len(),
            8,
            "eight SortableItems must stay in the hit list; hits={:?}",
            hits0
                .iter()
                .map(|h| (h.id.clone(), h.index, h.rect.height))
                .collect::<Vec<_>>()
        );
        for h in &item_hits {
            assert!(
                h.rect.height > 8.0 && h.rect.height < 80.0,
                "row slot must be a chip, not the list shell; {} h={}",
                h.id,
                h.rect.height
            );
        }

        let src = item_hits
            .iter()
            .find(|h| h.id == "Mercury")
            .expect("Mercury");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "Mercury").expect("Mercury must stay in the list");
            assert!(
                mercury.layout_bounds[3] > 8.0,
                "source row must not collapse to 0; bounds={:?}",
                mercury.layout_bounds
            );
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "in-list source is dimmed, not opacity 0; opacity={}",
                mercury.style.opacity
            );
        }

        let at_lift = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_lift,
            Some(0),
            "lift on row 0 must keep hover=from so the dimmed body stays; got {at_lift:?}"
        );

        let earth = item_hits.iter().find(|h| h.id == "Earth").expect("Earth");
        let earth_p = [earth.rect.x + 20.0, earth.rect.y + earth.rect.height * 0.5];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(earth_p[0], earth_p[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();

        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury = find_draggable(&tree, "Mercury").expect("Mercury after move");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "dimmed body must remain while hovering another row; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1] > 8.0,
                "dimmed body must occupy the Earth hole, not stay hidden at origin; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source must paint above siblings; z={}",
                mercury.style.z_index
            );
        }
        let at_earth = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_earth,
            Some(2),
            "hovering Earth must open slot 2, not append at len; got {at_earth:?}"
        );

        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            committed.get(),
            Some(Some(2)),
            "commit must use the hovered slot, not the tail; got {:?}",
            committed.get()
        );
        let mut order: Vec<&str> = names.to_vec();
        apply_reorder_vec(&mut order, 0, 2);
        assert_eq!(order[2], "Mercury");

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Same Gallery path, but the list sits below a 400px spacer (page
    /// ScrollArea offset). Insert-index must use the visual slots, not fall
    /// through to `len` because the pointer Y is large.
    #[test]
    fn gallery_sortable_below_page_offset_does_not_append() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                let spacer = Element::new().w(240.0).h(400.0).min_h(400.0);
                let list = drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child(move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-offset-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets")
                                            .index(i)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-offset-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    });
                Element::from(vstack().w(240.0).children([spacer, list]))
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let src = hits0
            .iter()
            .find(|h| h.id == "Mercury" && h.index == Some(0))
            .expect("Mercury");
        assert!(
            src.rect.y > 200.0,
            "precondition: list is below the spacer; mercury.y={}",
            src.rect.y
        );
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        let at_lift = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_lift,
            Some(0),
            "page-offset lift must keep hover=from, not len; got {at_lift:?}"
        );
        let hits1 = collect_hits(&live.borrow());
        assert!(end(&hits1));
        assert_eq!(
            committed.get(),
            Some(Some(0)),
            "commit on the source slot must not append; got {:?}",
            committed.get()
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery stacks opacity-0 above the dimmed list (same planet names,
    /// outer `scroll-overlays`). Dimmed chrome must match opacity-0 except
    /// opacity 0.4, and moving to Earth must not append.
    #[test]
    fn gallery_stacked_dimmed_matches_hide_source() {
        use crate::app::reactive::{
            drain_deferred_view_remounts, for_keyed, reset_registered_anchors,
        };
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::components::scroll_area::scroll_area;
        use crate::core::reactivity::Signal;
        use crate::core::tree::LayoutStatus;
        use crate::primitives::vstack;
        use crate::utils::drag::{
            begin, collect_hits, drag_overlay, drag_scope, end, sortable_item, sortable_list,
            start_geometry, tick, use_drag, Axis, BeginDrag, CollisionAlg,
        };
        use std::cell::Cell;
        use std::rc::Rc;

        let names = [
            "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
        ];
        let hover_sig = Rc::new(Cell::new(None::<Signal<Option<usize>>>));
        let committed = Rc::new(Cell::new(None::<Option<usize>>));
        let committed_end = Rc::clone(&committed);

        let (_o, live) = mount_scope_tree({
            let hover_sig = Rc::clone(&hover_sig);
            move || {
                let hide = drag_scope("gal-drag-sort-hide")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .child(|| {
                        let list = sortable_list("planets-hide").accept("planet").child({
                            let names = names;
                            move || {
                                let rows = for_keyed(
                                    "gal-drag-sort-hide-rows",
                                    &names,
                                    |s: &&str| (*s).to_string(),
                                    |name: &&str| {
                                        let id = (*name).to_string();
                                        let i = names.iter().position(|n| n == name).unwrap_or(0);
                                        sortable_item(id.clone())
                                            .item_type("planet")
                                            .list("planets-hide")
                                            .index(i)
                                            .source_opacity(0.0)
                                            .child({
                                                let id = id.clone();
                                                move || {
                                                    Element::new().text(id.clone()).w(200.0).h(32.0)
                                                }
                                            })
                                    },
                                );
                                Element::from(vstack().w("100%").gap(6.0).children(rows))
                            }
                        });
                        let overlay: Element = drag_overlay().into();
                        let scroll: Element = scroll_area("gal-drag-sort-hide-scroll")
                            .h(176.0)
                            .w("100%")
                            .overflow_y("auto")
                            .child(list)
                            .into();
                        Element::from(vstack().w(240.0).children([scroll, overlay]))
                    });
                let dimmed = drag_scope("gal-drag-sort")
                    .collision(CollisionAlg::ClosestCenter)
                    .axis(Axis::Y)
                    .threshold(0.0)
                    .on_drag_end({
                        let committed_end = Rc::clone(&committed_end);
                        move |ev| {
                            committed_end.set(Some(ev.to_index));
                        }
                    })
                    .child({
                        let hover_sig = Rc::clone(&hover_sig);
                        move || {
                            let ctx = use_drag().expect("DragProvider");
                            hover_sig.set(Some(ctx.hover_index));
                            let list = sortable_list("planets").accept("planet").child({
                                let names = names;
                                move || {
                                    let rows = for_keyed(
                                        "gal-drag-sort-rows",
                                        &names,
                                        |s: &&str| (*s).to_string(),
                                        |name: &&str| {
                                            let id = (*name).to_string();
                                            let i =
                                                names.iter().position(|n| n == name).unwrap_or(0);
                                            sortable_item(id.clone())
                                                .item_type("planet")
                                                .list("planets")
                                                .index(i)
                                                .child({
                                                    let id = id.clone();
                                                    move || {
                                                        Element::new()
                                                            .text(id.clone())
                                                            .w(200.0)
                                                            .h(32.0)
                                                    }
                                                })
                                        },
                                    );
                                    Element::from(vstack().w("100%").gap(6.0).children(rows))
                                }
                            });
                            let overlay: Element = drag_overlay().into();
                            let scroll: Element = scroll_area("gal-drag-sort-scroll")
                                .h(176.0)
                                .w("100%")
                                .overflow_y("auto")
                                .child(list)
                                .into();
                            Element::from(vstack().w(240.0).children([scroll, overlay]))
                        }
                    });
                let page: Element = scroll_area("scroll-overlays")
                    .h(600.0)
                    .w(240.0)
                    .overflow_y("auto")
                    .child(Element::from(vstack().w(240.0).children([hide, dimmed])))
                    .into();
                page
            }
        });

        {
            let mut tree = live.borrow_mut();
            hoist_and_layout(&mut tree);
        }

        let hits0 = collect_hits(&live.borrow());
        let item_hits: Vec<_> = hits0
            .iter()
            .filter(|h| h.index.is_some() && h.list_id.as_deref() == Some("planets"))
            .cloned()
            .collect();
        assert_eq!(item_hits.len(), 8, "dimmed list must keep eight rows");
        let src = item_hits
            .iter()
            .find(|h| h.id == "Mercury")
            .expect("Mercury");
        let earth = item_hits.iter().find(|h| h.id == "Earth").expect("Earth");
        let pointer = [
            src.rect.x + src.rect.width * 0.5,
            src.rect.y + src.rect.height.max(8.0) * 0.5,
        ];
        let (grab_offset, start_bounds) = start_geometry(
            "gal-drag-sort",
            "Mercury",
            pointer,
            [src.rect.x, src.rect.y, src.rect.width, src.rect.height],
        );
        begin(BeginDrag {
            scope_id: "gal-drag-sort".into(),
            draggable_id: "Mercury".into(),
            pointer,
            grab_offset,
            start_bounds,
            from_list: Some("planets".into()),
            from_index: Some(0),
            item_type: "planet".into(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(pointer[0] + 1.0, pointer[1], &hits0, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury =
                find_draggable_in_scope(&tree, "gal-drag-sort", "Mercury").expect("dimmed Mercury");
            assert!(
                (mercury.style.opacity - 0.4).abs() < 0.05,
                "stacked dimmed source must stay at 0.4, not hide; opacity={}",
                mercury.style.opacity
            );
            assert!(
                mercury.style.translate[1].abs() < 1.0,
                "lift on from=hover keeps the dimmed body in slot; translate={:?}",
                mercury.style.translate
            );
            assert!(
                mercury.style.z_index >= 1,
                "dimmed source is stacked above siblings; z={}",
                mercury.style.z_index
            );
            assert!(
                mercury.layout_bounds[3] > 4.0,
                "dimmed body must not collapse; bounds={:?}",
                mercury.layout_bounds
            );
        }
        let earth_p = [
            earth.rect.x + 20.0,
            earth.rect.y + earth.rect.height.max(8.0) * 0.5,
        ];
        let hits1 = collect_hits(&live.borrow());
        assert!(tick(earth_p[0], earth_p[1], &hits1, [800.0, 600.0]));
        drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            let mut lt = crate::layout::LayoutTree::new();
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut tree,
                &mut lt,
                &mut impact,
                800.0,
                600.0,
                [0.0, 0.0],
            );
            let mercury =
                find_draggable_in_scope(&tree, "gal-drag-sort", "Mercury").expect("dimmed Mercury");
            assert!(
                mercury.style.translate[1] > 8.0,
                "stacked dimmed body must occupy the Earth hole; translate={:?}",
                mercury.style.translate
            );
        }
        let at_earth = hover_sig.get().expect("hover signal").peek();
        assert_eq!(
            at_earth,
            Some(2),
            "stacked dimmed list must open Earth, not append; got {at_earth:?}"
        );
        let hits2 = collect_hits(&live.borrow());
        assert!(end(&hits2));
        assert_eq!(
            committed.get(),
            Some(Some(2)),
            "commit must use Earth, not the tail; got {:?}",
            committed.get()
        );

        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    fn activate_named(sid: &str, id: &str, pointer: [f32; 2], bounds: [f32; 4]) {
        use crate::utils::drag::{begin, tick, BeginDrag};
        begin(BeginDrag {
            scope_id: sid.into(),
            draggable_id: id.into(),
            pointer,
            grab_offset: [10.0, 8.0],
            start_bounds: bounds,
            from_list: None,
            from_index: None,
            item_type: String::new(),
            group: String::new(),
            capture_id: "cap".into(),
        });
        assert!(tick(
            pointer[0] + 16.0,
            pointer[1] + 8.0,
            &[],
            [800.0, 600.0]
        ));
        crate::app::reactive::drain_deferred_view_remounts();
    }

    fn sync_drag_chrome(tree: &mut Element) {
        let mut lt = crate::layout::LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            tree,
            &mut lt,
            &mut impact,
            800.0,
            600.0,
            [0.0, 0.0],
        );
    }

    fn two_free_draggables(
        scope: &'static str,
        stack_a: crate::utils::drag::DragStack,
        stack_b: crate::utils::drag::DragStack,
        z_a: i32,
        z_b: i32,
    ) -> (
        crate::core::reactivity::OwnerHandle,
        std::rc::Rc<std::cell::RefCell<Element>>,
    ) {
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_scope, draggable};
        mount_scope_tree(move || {
            drag_scope(scope).threshold(0.0).child(move || {
                let a = draggable("a").stack(stack_a).child(move || {
                    Element::new()
                        .text("Draggable 1")
                        .w(80.0)
                        .h(32.0)
                        .z_index(z_a)
                });
                let b = draggable("b").stack(stack_b).child(move || {
                    Element::new()
                        .text("Draggable 2")
                        .w(80.0)
                        .h(32.0)
                        .z_index(z_b)
                });
                Element::from(vstack().children([a, b]))
            })
        })
    }

    /// Default `stack=auto`: dragging raises the item above its sibling and the
    /// new order survives `end` (solid-dnd arbitrary move).
    #[test]
    fn auto_stack_raises_on_drag_and_keeps_after_end() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) = two_free_draggables("stack-auto", DragStack::Auto, DragStack::Auto, 0, 0);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a rest");
            let b = find_draggable(&tree, "b").expect("b rest");
            assert_eq!(a.style.z_index, 0);
            assert_eq!(b.style.z_index, 0);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "a", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a drag");
            let b = find_draggable(&tree, "b").expect("b drag");
            assert!(
                a.style.z_index > b.style.z_index,
                "dragged auto must sit above sibling; a={} b={}",
                a.style.z_index,
                b.style.z_index
            );
            assert!(a.style.z_index >= 1);
        }
        assert!(end(&[]));
        crate::app::reactive::drain_deferred_view_remounts();
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a end");
            let b = find_draggable(&tree, "b").expect("b end");
            assert!(
                a.style.z_index > b.style.z_index,
                "auto rank must persist after drop; a={} b={}",
                a.style.z_index,
                b.style.z_index
            );
        }
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// `stack=keep`: author z-index is unchanged while dragging, even when the
    /// lower item is the active drag.
    #[test]
    fn keep_stack_preserves_author_z_while_dragging() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) = two_free_draggables("stack-keep", DragStack::Keep, DragStack::Keep, 1, 4);
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "a", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a");
            let b = find_draggable(&tree, "b").expect("b");
            assert_eq!(a.style.z_index, 1, "keep must not raise the dragged item");
            assert_eq!(b.style.z_index, 4);
            assert!(a.style.z_index < b.style.z_index);
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Same author z + `keep`: both stay equal (tree order paints).
    #[test]
    fn keep_stack_same_author_z_stays_equal() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::utils::drag::{end, DragStack};

        let (_o, live) =
            two_free_draggables("stack-keep-eq", DragStack::Keep, DragStack::Keep, 2, 2);
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "b", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let a = find_draggable(&tree, "a").expect("a");
            let b = find_draggable(&tree, "b").expect("b");
            assert_eq!(a.style.z_index, 2);
            assert_eq!(b.style.z_index, 2);
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// `top` stays above a dragged `auto`; `bottom` stays below even when dragged.
    #[test]
    fn top_and_bottom_bands_ignore_drag_raise() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_scope, draggable, end, DragStack};

        let (_o, live) = mount_scope_tree(|| {
            drag_scope("stack-bands").threshold(0.0).child(|| {
                let top = draggable("top")
                    .stack(DragStack::Top)
                    .child(|| Element::new().text("top").w(80.0).h(32.0).z_index(0));
                let auto = draggable("auto")
                    .stack(DragStack::Auto)
                    .child(|| Element::new().text("auto").w(80.0).h(32.0).z_index(0));
                let bottom = draggable("bottom")
                    .stack(DragStack::Bottom)
                    .child(|| Element::new().text("bottom").w(80.0).h(32.0).z_index(0));
                Element::from(vstack().children([top, auto, bottom]))
            })
        });
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let top = find_draggable(&tree, "top").expect("top rest");
            let bottom = find_draggable(&tree, "bottom").expect("bottom rest");
            assert!(top.style.z_index >= DragStack::TOP_BAND);
            assert!(bottom.style.z_index <= DragStack::BOTTOM_BAND + 1);
            assert!(top.style.z_index > bottom.style.z_index);
        }
        let sid = find_scope_id(&live.borrow()).expect("scope");
        activate_named(&sid, "auto", [20.0, 16.0], [8.0, 8.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let top = find_draggable(&tree, "top").expect("top");
            let auto = find_draggable(&tree, "auto").expect("auto");
            let bottom = find_draggable(&tree, "bottom").expect("bottom");
            assert!(
                auto.style.z_index > bottom.style.z_index,
                "auto drag must still sit above always-bottom"
            );
            assert!(
                top.style.z_index > auto.style.z_index,
                "always-top must stay above dragged auto; top={} auto={}",
                top.style.z_index,
                auto.style.z_index
            );
        }
        let _ = end(&[]);
        activate_named(&sid, "bottom", [20.0, 48.0], [8.0, 40.0, 80.0, 32.0]);
        {
            let mut tree = live.borrow_mut();
            sync_drag_chrome(&mut tree);
            let auto = find_draggable(&tree, "auto").expect("auto rest rank");
            let bottom = find_draggable(&tree, "bottom").expect("bottom drag");
            assert!(
                bottom.style.z_index < auto.style.z_index,
                "dragging always-bottom must not raise it above auto; bottom={} auto={}",
                bottom.style.z_index,
                auto.style.z_index
            );
        }
        let _ = end(&[]);
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Idle sortable rows used to stamp `data-stacking-context` on every shell.
    /// Gallery's page ScrollArea already owns `page_stack_depth`, so each row
    /// opened a glyphon band — scrolling the list on/off-screen reshaped every
    /// label (`slow render text=150ms+` / `collect=60ms`).
    #[test]
    fn idle_sortable_rows_do_not_open_a_paint_band_each() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::layout::LayoutTree;
        use crate::primitives::vstack;
        use crate::utils::drag::{drag_scope, draggable, Axis};
        const N: usize = 12;
        let (_owner, live) = mount_scope_tree(move || {
            let drag = drag_scope("gal-sort-bands")
                .axis(Axis::Y)
                .threshold(0.0)
                .child(move || {
                    let rows: Vec<Element> = (0..N)
                        .map(|i| {
                            let id = format!("row-{i}");
                            draggable(id.clone())
                                .also_droppable(true)
                                .list("rows")
                                .index(i)
                                .child(move || Element::new().text(id.clone()).w(160.0).h(28.0))
                        })
                        .collect();
                    Element::from(vstack().gap(4.0).children(rows))
                });
            let bar = Element::new()
                .w(8.0)
                .h(80.0)
                .absolute()
                .right(0.0)
                .z_index(1000)
                .bg([0.0, 0.0, 0.0, 0.4]);
            Element::new()
                .w(400.0)
                .h(400.0)
                .relative()
                .child(drag)
                .child(bar)
        });
        let mut tree = live.borrow_mut();
        sync_drag_chrome(&mut tree);
        for i in 0..N {
            let shell = find_item_shell(&tree, &format!("row-{i}")).expect("shell");
            assert!(
                shell.attrs.get("data-stacking-context").is_none(),
                "idle in-flow row {i} must not open a nested paint band"
            );
        }
        let mut lt = LayoutTree::new();
        let painted = lt.build_and_layout(&mut tree, 400.0, 400.0, [0.0, 0.0]);
        let bands = painted.overlay_split.page_stack_bands.len();
        assert!(
            bands <= 4,
            "idle sortable list must not split one paint band per row; bands={bands} rows={N}"
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }

    /// Gallery wraps overlapping pills in a relative arena under DragProvider,
    /// and the page ScrollArea already owns `page_stack_depth`. The item shell
    /// must be a stacking-context so each pill paints `rects → text` (otherwise
    /// the lower glyph sits on the upper fill).
    #[test]
    fn wrapped_arena_inside_page_stack_keeps_glyphs_with_shell() {
        use crate::app::reactive::reset_registered_anchors;
        use crate::app::runtime::clear_test_reconcile_root;
        use crate::layout::LayoutTree;
        use crate::utils::drag::{drag_scope, draggable};
        let (_owner, live) = mount_scope_tree(move || {
            let drag = drag_scope("gal-wrap-stack").threshold(0.0).child(move || {
                let a = draggable("a").child(move || {
                    Element::new()
                        .bg([0.1, 0.2, 0.9, 1.0])
                        .w(176.0)
                        .h(92.0)
                        .absolute()
                        .left(16.0)
                        .top(20.0)
                        .child(Element::new().text("A"))
                });
                let b = draggable("b").child(move || {
                    Element::new()
                        .bg([0.9, 0.1, 0.1, 1.0])
                        .w(176.0)
                        .h(92.0)
                        .absolute()
                        .left(88.0)
                        .top(52.0)
                        .child(Element::new().text("B"))
                });
                Element::new().w(400.0).h(200.0).relative().children([a, b])
            });
            let bar = Element::new()
                .w(8.0)
                .h(80.0)
                .absolute()
                .right(0.0)
                .z_index(1000)
                .bg([0.0, 0.0, 0.0, 0.4]);
            Element::new()
                .w(400.0)
                .h(240.0)
                .relative()
                .child(drag)
                .child(bar)
        });
        let mut tree = live.borrow_mut();
        sync_drag_chrome(&mut tree);
        {
            let shell_a = find_item_shell(&tree, "a").expect("shell a");
            let shell_b = find_item_shell(&tree, "b").expect("shell b");
            assert_eq!(
                shell_a
                    .attrs
                    .get("data-stacking-context")
                    .map(String::as_str),
                Some("1"),
                "absolute overlapping shells must opt into nested page_stack splits"
            );
            assert_eq!(
                shell_b
                    .attrs
                    .get("data-stacking-context")
                    .map(String::as_str),
                Some("1")
            );
        }
        let mut lt = LayoutTree::new();
        let painted = lt.build_and_layout(&mut tree, 400.0, 240.0, [0.0, 0.0]);
        let ranges = painted.overlay_split.page_stack_band_ranges();
        let band_of = |label: &str| {
            ranges.iter().position(|r| {
                painted.text_nodes[r.text.0..r.text.1]
                    .iter()
                    .any(|t| t.text == label)
            })
        };
        let a = band_of("A");
        let b = band_of("B");
        assert!(
            a.is_some() && b.is_some() && a != b,
            "overlapping pills must not share a paint band (A={a:?} B={b:?} bands={})",
            ranges.len()
        );
        assert!(
            b > a,
            "later sibling B must paint after A so its fill occludes A's glyph (A={a:?} B={b:?})"
        );
        clear_test_reconcile_root();
        reset_registered_anchors();
    }
}
