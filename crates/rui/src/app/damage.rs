//! Per-node paint invalidation (logical pixel coordinates).
//!
//! ## Present policy
//!
//! - **Paint-only**: each dirty **node** is a separate present item. Same-key bounds
//!   may merge; **different nodes never union**. GPU Load + per-node scissor draws
//!   only that node's primitives. Hover and overlay cannot pollute each other.
//! - Swapchain always gets a **full blit** (`Clear`). Never `Load` a FLIP_DISCARD /
//!   GLES surface. `RUI_PARTIAL_DAMAGE=0` forces surface FullClear.
//! - A **single** node covering the viewport (modal underlay) FullClears. The union
//!   of many local nodes must not.
//! - Scroll invalidates the **viewport layer** (scissor Clear), not paint nodes.
//!   A valid retain may **strip-blit** the overlap and Clear only the newly
//!   exposed band(s) plus pin holes (sticky / overlay / named paint / nested
//!   inner viewports). Nested containment blits the outer and pins the inner.
//!   Overlapping siblings blit both and pin the intersection occupancy (copy
//!   smear). `force_full_redraw`, invalid retain, `|delta| ≥ viewport`, and
//!   `RUI_SCROLL_STRIP_BLIT=0` stay abort / layer-Clear gates. Spawn without
//!   prev metrics does not abort sibling blits (no box to pin).
//! - Page structure / page layout stay FullClear.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::core::tree::{Element, LayoutStatus};

/// How a paint-dirty item is submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintDirtyKind {
    /// This node's own primitives (hover chrome, bind_text, …).
    Node,
    /// Hoisted overlay primitives only — never page slots.
    Overlay,
    /// Erase leftover overlay pixels, then redraw intersecting **page** nodes.
    OverlayRestore,
}

/// One dirty component node (not a union AABB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintDirty {
    pub key: u64,
    pub bounds: [f32; 4],
    pub kind: PaintDirtyKind,
}

thread_local! {
    static FRAME_PAINT_NODES: RefCell<Vec<PaintDirty>> = RefCell::new(Vec::new());
}

fn bounds_key(bounds: [f32; 4]) -> u64 {
    let mut h = DefaultHasher::new();
    bounds[0].to_bits().hash(&mut h);
    bounds[1].to_bits().hash(&mut h);
    bounds[2].to_bits().hash(&mut h);
    bounds[3].to_bits().hash(&mut h);
    h.finish()
}

/// Stable key for a laid-out element (Taffy `NodeId`, else the visual box).
pub fn paint_element_key(el: &Element) -> u64 {
    el.layout_node
        .map(|n| u64::from(n))
        .unwrap_or_else(|| bounds_key(el.layout_bounds))
}

fn push_paint_dirty(key: u64, bounds: [f32; 4], kind: PaintDirtyKind) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    FRAME_PAINT_NODES.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(existing) = slot.iter_mut().find(|d| d.key == key && d.kind == kind) {
            existing.bounds = union_rects(existing.bounds, bounds);
        } else {
            slot.push(PaintDirty { key, bounds, kind });
        }
    });
}

/// Record this element's **visual** box as its own dirty node (no cross-node union).
///
/// `scroll_acc` must be the same accumulation collect uses when emitting instances
/// (`layout_bounds − scroll + sticky + translate`). Hardcoding `[0, 0]` puts the
/// scissor in document space while ink is in visual space — dest-reset misses,
/// PartialLoad stacks the previous offset (Marker labels after scroll).
pub fn push_frame_paint_element(el: &Element, scroll_acc: [f32; 2]) {
    let b = crate::core::tree::visual_paint_aabb(el, scroll_acc);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    let spinning = el.attrs.get("rui_animate").map(|s| s.as_str()) == Some("spin")
        || el.style.animations.iter().any(|a| a.name == "spin")
        || el.style.rotate_deg.abs() > 0.01;
    // Circumcircle of the box vs AABB — previous 45° frame sits outside the axis-aligned box.
    let rot_pad = if spinning {
        (b[2].hypot(b[3]) - b[2].min(b[3])).max(0.0) * 0.5
    } else {
        0.0
    };
    let pad = (el.style.shadow_blur + el.style.shadow_spread.abs() + 2.0 + rot_pad).max(2.0);
    push_paint_dirty(
        paint_element_key(el),
        [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0],
        paint_kind_for_element(el),
    );
}

/// Hoisted modal/popover chrome must not dest-reset as a page `Node`.
pub fn paint_kind_for_element(el: &Element) -> PaintDirtyKind {
    if el.attrs.get("rui_hoisted_portal").map(String::as_str) == Some("1") {
        return PaintDirtyKind::Overlay;
    }
    match el.template_id.as_deref() {
        Some(t)
            if t.ends_with("__modal_overlay")
                || t.ends_with("__modal_panel")
                || t.ends_with("__modal_zoom") =>
        {
            PaintDirtyKind::Overlay
        }
        _ => PaintDirtyKind::Node,
    }
}

/// Record a local paint dirty **node**. Disjoint calls stay separate nodes.
pub fn push_frame_paint_damage(bounds: [f32; 4]) {
    push_paint_dirty(bounds_key(bounds), bounds, PaintDirtyKind::Node);
}

/// Same as [`push_frame_paint_damage`] but keyed to a collect slot (`paint_element_key`).
pub fn push_frame_paint_key(key: u64, bounds: [f32; 4]) {
    push_paint_dirty(key, bounds, PaintDirtyKind::Node);
}

/// Overlay panel: own node, never merged with page hover.
pub fn push_frame_overlay_damage(key: u64, bounds: [f32; 4]) {
    push_paint_dirty(key, bounds, PaintDirtyKind::Overlay);
}

/// Overlay closed/shrunk: clear this box, restore intersecting page nodes.
pub fn push_frame_overlay_restore(bounds: [f32; 4]) {
    push_paint_dirty(
        bounds_key(bounds) ^ 1,
        bounds,
        PaintDirtyKind::OverlayRestore,
    );
}

/// Take this frame's dirty nodes (order: restore, page, overlay).
pub fn take_frame_paint_nodes() -> Vec<PaintDirty> {
    FRAME_PAINT_NODES.with(|slot| {
        let mut nodes = slot.borrow_mut().split_off(0);
        nodes.sort_by_key(|d| match d.kind {
            PaintDirtyKind::OverlayRestore => 0u8,
            PaintDirtyKind::Node => 1,
            PaintDirtyKind::Overlay => 2,
        });
        nodes
    })
}

/// Drop queued paint nodes (page StructureChanged FullClear). Stale Taffy keys
/// from the pre-rebuild tree must not dest-reset the sidebar footer after Send.
pub fn clear_frame_paint_nodes() {
    FRAME_PAINT_NODES.with(|slot| slot.borrow_mut().clear());
}

/// Legacy: first dirty node's bounds only. Prefer [`take_frame_paint_nodes`].
pub fn take_frame_paint_damage() -> Option<[f32; 4]> {
    take_frame_paint_nodes().first().map(|d| d.bounds)
}

/// GPU color attachment load strategy for the main present pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameClearPolicy {
    /// Keep prior framebuffer; scissor to the damage rect.
    PartialLoad,
    /// Clear the whole attachment.
    FullClear,
}

static PARTIAL_LOAD_FRAMES: AtomicU64 = AtomicU64::new(0);
static FULL_CLEAR_FRAMES: AtomicU64 = AtomicU64::new(0);
static SCROLL_STRIP_BLIT_FRAMES: AtomicU64 = AtomicU64::new(0);
static SUPPRESS_NOTES: AtomicU32 = AtomicU32::new(0);
static ARM_SNAPSHOT: AtomicBool = AtomicBool::new(false);
static SNAPSHOT_READY: AtomicBool = AtomicBool::new(false);
static SNAP_PARTIAL: AtomicU64 = AtomicU64::new(0);
static SNAP_FULL: AtomicU64 = AtomicU64::new(0);
static SNAP_SKIP: AtomicU64 = AtomicU64::new(0);
static SNAP_PATCH: AtomicU64 = AtomicU64::new(0);

/// Frozen policy counters after a present (Gallery WP-2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub partial_load: u64,
    pub full_clear: u64,
    pub skipped_siblings: u64,
    pub patched_dl: u64,
}

/// Present-pass Load vs Clear counters (WP-2.2 / Gallery acceptance).
pub fn partial_load_frame_count() -> u64 {
    PARTIAL_LOAD_FRAMES.load(Ordering::Relaxed)
}

pub fn full_clear_frame_count() -> u64 {
    FULL_CLEAR_FRAMES.load(Ordering::Relaxed)
}

/// Frames that applied a content strip blit (overlap copy + exposed-band Clear).
pub fn scroll_strip_blit_count() -> u64 {
    SCROLL_STRIP_BLIT_FRAMES.load(Ordering::Relaxed)
}

/// Default **on**. `RUI_SCROLL_STRIP_BLIT=0` / `false` / `off` keeps a full viewport-layer Clear.
pub fn scroll_strip_blit_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("RUI_SCROLL_STRIP_BLIT") {
        Ok(v) => {
            let v = v.trim();
            !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
        }
        Err(_) => true,
    })
}

/// Record a committed strip blit (overlap copied; only the exposed band is cleared).
pub fn note_scroll_strip_blit() {
    SCROLL_STRIP_BLIT_FRAMES.fetch_add(1, Ordering::Relaxed);
}

/// Skip the next `frames` [`note_frame_clear_policy`] increments (stats-label paint).
pub fn suppress_policy_counts(frames: u32) {
    if frames == 0 {
        return;
    }
    SUPPRESS_NOTES.fetch_add(frames, Ordering::Relaxed);
}

/// After the next present, stash totals for [`take_policy_snapshot`].
pub fn arm_policy_snapshot() {
    ARM_SNAPSHOT.store(true, Ordering::Relaxed);
}

/// Consume a snapshot armed by [`arm_policy_snapshot`] (once per armed present).
pub fn take_policy_snapshot() -> Option<PolicySnapshot> {
    if !SNAPSHOT_READY.swap(false, Ordering::Relaxed) {
        return None;
    }
    Some(PolicySnapshot {
        partial_load: SNAP_PARTIAL.load(Ordering::Relaxed),
        full_clear: SNAP_FULL.load(Ordering::Relaxed),
        skipped_siblings: SNAP_SKIP.load(Ordering::Relaxed),
        patched_dl: SNAP_PATCH.load(Ordering::Relaxed),
    })
}

fn stash_policy_snapshot() {
    SNAP_PARTIAL.store(partial_load_frame_count(), Ordering::Relaxed);
    SNAP_FULL.store(full_clear_frame_count(), Ordering::Relaxed);
    SNAP_SKIP.store(
        super::frame_stats::skipped_siblings_count(),
        Ordering::Relaxed,
    );
    SNAP_PATCH.store(super::frame_stats::patched_dl_count(), Ordering::Relaxed);
    SNAPSHOT_READY.store(true, Ordering::Relaxed);
}

/// Reset counters (Gallery "Reset counters" / tests).
pub fn reset_frame_policy_counters() {
    PARTIAL_LOAD_FRAMES.store(0, Ordering::Relaxed);
    FULL_CLEAR_FRAMES.store(0, Ordering::Relaxed);
    SCROLL_STRIP_BLIT_FRAMES.store(0, Ordering::Relaxed);
    SUPPRESS_NOTES.store(0, Ordering::Relaxed);
    ARM_SNAPSHOT.store(false, Ordering::Relaxed);
    SNAPSHOT_READY.store(false, Ordering::Relaxed);
    super::frame_stats::reset_collect_skip_counters();
}

#[cfg(test)]
pub fn reset_frame_policy_counters_for_test() {
    reset_frame_policy_counters();
}

/// Record which present policy was chosen (call once per submitted frame).
pub fn note_frame_clear_policy(policy: FrameClearPolicy) {
    let suppress = SUPPRESS_NOTES.load(Ordering::Relaxed);
    if suppress > 0 {
        SUPPRESS_NOTES.store(suppress - 1, Ordering::Relaxed);
    } else {
        match policy {
            FrameClearPolicy::PartialLoad => {
                PARTIAL_LOAD_FRAMES.fetch_add(1, Ordering::Relaxed);
            }
            FrameClearPolicy::FullClear => {
                FULL_CLEAR_FRAMES.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    if ARM_SNAPSHOT.swap(false, Ordering::Relaxed) {
        stash_policy_snapshot();
    }
}

/// Overlay PartialLoad must not redraw page chrome. Drawing bubble fills without
/// glyphs (or with culled glyphon) is the gallery empty-shell bug. Keep the retain
/// buffer and composite only the hoisted overlay.
///
/// `page_text_unchanged` is required: a chip `+1` → emoji still lives on the page,
/// so that scissor must repaint page glyphs. A scissor Clear (scroll layer or
/// overlay close/shrink) wiped those pixels and must repaint the page too.
#[inline]
pub fn overlay_partial_load_retains_page(
    use_partial_damage: bool,
    scissor_cleared: bool,
    overlay_present: bool,
    page_text_unchanged: bool,
) -> bool {
    use_partial_damage && !scissor_cleared && overlay_present && page_text_unchanged
}

/// Overlay hoist/close is not a page structure change. Skip window FullClear when only
/// the portal layer membership changed; the frame path still scissors to overlay bounds.
#[inline]
pub fn skip_full_clear_for_portal_structure(
    portal_membership_changed: bool,
    page_geometry_changed: bool,
) -> bool {
    portal_membership_changed && !page_geometry_changed
}

/// Overlay pixels must not be clipped by leftover page/hover scissors.
///
/// Bubble +N expand: hover damage is the chip; the panel is larger (or still 0×0
/// before floating place). PartialLoad with only the chip hides the overlay —
/// "cannot expand". No laid-out box → FullClear that frame.
#[inline]
pub fn overlay_pixels_need_full_clear(
    overlay_present: bool,
    overlay_box: Option<[f32; 4]>,
) -> bool {
    overlay_present && overlay_box.is_none()
}

/// Overlay leftover pixels in `present_retain` that this frame will not cover.
///
/// Expand-from-chip **shrinks** the painted box while the portal is still mounted.
/// Load + overlay draw leaves every previous keyframe in retain (stacked ghosts).
/// Clear the occupancy hole, restore page slots, then draw the current overlay.
/// Grow does not need Clear: new pixels were page and the overlay covers them.
#[inline]
pub fn overlay_restore_needed(occupied: Option<[f32; 4]>, current: Option<[f32; 4]>) -> bool {
    match (occupied, current) {
        (Some(_), None) => true,
        (Some(occ), Some(cur)) => overlay_box_extends_beyond(occ, cur, 2.0),
        _ => false,
    }
}

/// True when `outer` sticks out of `inner` by more than `slack` px (`[x,y,w,h]`).
#[inline]
pub fn overlay_box_extends_beyond(outer: [f32; 4], inner: [f32; 4], slack: f32) -> bool {
    let slack = slack.max(0.0);
    outer[0] < inner[0] - slack
        || outer[1] < inner[1] - slack
        || outer[0] + outer[2] > inner[0] + inner[2] + slack
        || outer[1] + outer[3] > inner[1] + inner[3] + slack
}

/// Overlay leftover pixels need a scissor Clear when occupancy is gone or shrank.
#[inline]
pub fn overlay_scissor_needs_clear(prev: Option<[f32; 4]>, cur: Option<[f32; 4]>) -> bool {
    overlay_restore_needed(prev, cur)
}

/// Dirty node vs collect slot: same Taffy key **and** overlapping box, or the
/// same visual box (padded `push_frame_paint_damage` vs unpadded `layout_bounds`).
///
/// Taffy `NodeId` is recycled on StructureChanged. A key-only hit would treat
/// the Gallery sidebar footer as the MessageScroller island, dest-reset the
/// rail bottom, and src-over 「布局」/Gallery/文本 into retain.
///
/// Do **not** use AABB intersect / ancestor containment for *different* keys: a
/// popover box intersects the bubble underneath, and a page root contains every
/// widget — both would stamp foreign chrome/glyphs into the dirty scissor.
///
/// `same_box` across *different* keys is the bounds-key fallback. Nested
/// same-size wrappers (MarkerIcon `size-4` around Icon `size-4`) also share
/// that box, and the parent's collect range includes the child's textured
/// quad. Present must use [`paint_dirty_matches_present_slot`]: when a keyed
/// slot exists, skip the wrapper or PartialLoad src-overs the spinner twice
/// (idle looks thicker than a FullClear scroll frame).
#[inline]
pub fn paint_dirty_matches_slot(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    slot_key: u64,
    slot_bounds: [f32; 4],
) -> bool {
    const SLACK: f32 = 8.0;
    let same_box = (dirty_bounds[0] - slot_bounds[0]).abs() <= SLACK
        && (dirty_bounds[1] - slot_bounds[1]).abs() <= SLACK
        && (dirty_bounds[2] - slot_bounds[2]).abs() <= SLACK * 2.0
        && (dirty_bounds[3] - slot_bounds[3]).abs() <= SLACK * 2.0;
    if same_box {
        return true;
    }
    if slot_key != dirty_key {
        return false;
    }
    // Same NodeId after a rebuild: only trust it when the boxes still share space
    // (hover/caret of that widget). 32px covers a 1-line reflow, not rail ↔ inset.
    const REUSE_PAD: f32 = 32.0;
    let padded = [
        dirty_bounds[0] - REUSE_PAD,
        dirty_bounds[1] - REUSE_PAD,
        (dirty_bounds[2] + REUSE_PAD * 2.0).max(0.0),
        (dirty_bounds[3] + REUSE_PAD * 2.0).max(0.0),
    ];
    intersect_rects(padded, slot_bounds).is_some()
}

/// True when some collect slot shares `dirty_key` and still occupies this box.
#[inline]
pub fn paint_dirty_has_keyed_slot(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    slots: impl IntoIterator<Item = (u64, [f32; 4])>,
) -> bool {
    slots
        .into_iter()
        .any(|(k, b)| k == dirty_key && paint_dirty_matches_slot(dirty_key, dirty_bounds, k, b))
}

/// Node-present slot filter. Prefer the keyed widget; `same_box` is only the
/// fallback when chrome recorded a bounds-key and no Taffy slot owns that key.
#[inline]
pub fn paint_dirty_matches_present_slot(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    slot_key: u64,
    slot_bounds: [f32; 4],
    has_keyed_slot: bool,
) -> bool {
    if has_keyed_slot && slot_key != dirty_key {
        return false;
    }
    if !has_keyed_slot && slot_key != dirty_key {
        // same_box fallback must not dest-reset a larger ancestor (shimmer-box
        // dirty matching the card / scroll viewport after a NodeId recycle).
        let dirty_area = dirty_bounds[2] * dirty_bounds[3];
        let slot_area = slot_bounds[2] * slot_bounds[3];
        if slot_area > dirty_area * 2.0 + 1.0 {
            return false;
        }
    }
    paint_dirty_matches_slot(dirty_key, dirty_bounds, slot_key, slot_bounds)
}

/// Inner widget whose dest-reset would punch a hole in a larger dirty ancestor
/// already being presented (Kbd Search Input inside InputAdornment). Skip the
/// inner wipe; the ancestor slot redraws the fill, then the inner slot paints
/// caret chrome on top.
///
/// Nesting is **center-in-larger**, not AABB intersect. Stacked Inputs pad their
/// dirty boxes by ring+8px; those pads overlap across `gap-2`. Intersect+area
/// treated the newly focused sibling as an ancestor, skipped the leave field's
/// dest-reset, and src-over'd another Hard ring onto retain (color stacking).
#[inline]
pub fn paint_dirty_nested_in_larger_node(dirty: &PaintDirty, nodes: &[PaintDirty]) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    let area = dirty.bounds[2] * dirty.bounds[3];
    let cx = dirty.bounds[0] + dirty.bounds[2] * 0.5;
    let cy = dirty.bounds[1] + dirty.bounds[3] * 0.5;
    nodes.iter().any(|o| {
        o.kind == PaintDirtyKind::Node
            && o.key != dirty.key
            && o.bounds[2] * o.bounds[3] > area + 1.0
            && cx >= o.bounds[0]
            && cy >= o.bounds[1]
            && cx < o.bounds[0] + o.bounds[2]
            && cy < o.bounds[1] + o.bounds[3]
    })
}

/// Overlay close: redraw this collect slot into the cleared hole.
///
/// Intersecting **leaf/widget** slots only. A page/row that fully contains the
/// hole would bring every descendant caption with it.
#[inline]
pub fn restore_slot_in_hole(slot_bounds: [f32; 4], hole: [f32; 4]) -> bool {
    if intersect_rects(slot_bounds, hole).is_none() {
        return false;
    }
    if rect_contains_rect(slot_bounds, hole) {
        let slot_area = (slot_bounds[2] * slot_bounds[3]).max(1.0);
        let hole_area = (hole[2] * hole[3]).max(1.0);
        if slot_area > hole_area * 1.5 {
            return false;
        }
    }
    true
}

/// Extra logical px around a hoisted overlay when clearing leftover chips/shadows.
///
/// Overlay present scissors with 16px; expand-from-chip scale / drop-shadow / wrap
/// overflow sit outside Taffy `layout_bounds`. Restore must cover that paint, not
/// just the layout box — otherwise collapse leaves the panel ghosted on the bubble.
pub const OVERLAY_RESTORE_PAD_LOGICAL: f32 = 24.0;

/// Grow overlay occupancy by a visual paint AABB (logical px). Empty `visual` is ignored.
pub fn union_overlay_visual_bounds(
    occupied: Option<[f32; 4]>,
    visual: [f32; 4],
) -> Option<[f32; 4]> {
    if visual[2] <= 0.0 || visual[3] <= 0.0 {
        return occupied;
    }
    Some(match occupied {
        Some(o) => union_rects(o, visual),
        None => visual,
    })
}

/// Glyph box belongs to a hover-dirty widget when its center is inside `dirty`.
///
/// Intersect-only is too wide: adjacent Button Variants labels share a few px of
/// padding, so "Default" would stamp into the hovered sibling after chrome redraw.
#[inline]
pub fn text_center_in_dirty(text: [f32; 4], dirty: [f32; 4]) -> bool {
    if text[2] <= 0.0 || text[3] <= 0.0 || dirty[2] <= 0.0 || dirty[3] <= 0.0 {
        return false;
    }
    let cx = text[0] + text[2] * 0.5;
    let cy = text[1] + text[3] * 0.5;
    cx >= dirty[0] && cy >= dirty[1] && cx <= dirty[0] + dirty[2] && cy <= dirty[1] + dirty[3]
}

/// True when `outer` fully covers `inner` (`[x,y,w,h]`, 0.5px layout slack).
#[inline]
pub fn rect_contains_rect(outer: [f32; 4], inner: [f32; 4]) -> bool {
    if inner[2] <= 0.0 || inner[3] <= 0.0 {
        return true;
    }
    if outer[2] <= 0.0 || outer[3] <= 0.0 {
        return false;
    }
    const EPS: f32 = 0.5;
    outer[0] <= inner[0] + EPS
        && outer[1] <= inner[1] + EPS
        && outer[0] + outer[2] + EPS >= inner[0] + inner[2]
        && outer[1] + outer[3] + EPS >= inner[1] + inner[3]
}

/// Overlay open/close scissors to the hoisted box. Leftover page/hover unions
/// (class `:hover` leave on every gallery card) cover the main pane — PartialLoad
/// then redraws bubble chrome and, with culled glyphon, leaves empty shells.
///
/// A scroll-layer Clear already dirtied the viewport layer: do not shrink that
/// rect down to the overlay chip (stale scrolled pixels outside the popover).
#[inline]
pub fn overlay_membership_damage(
    leftover: Option<[f32; 4]>,
    overlay_union: Option<[f32; 4]>,
    viewport: [f32; 4],
    membership_changed: bool,
    scroll_layer_clear: bool,
) -> Option<[f32; 4]> {
    if scroll_layer_clear {
        return leftover;
    }
    if membership_changed {
        return overlay_union;
    }
    match (leftover, overlay_union) {
        (Some(a), Some(b)) => {
            let u = union_rects(a, b);
            if damage_covers_full(viewport, u) {
                overlay_union
            } else {
                Some(u)
            }
        }
        (None, o) => o,
        (l, None) => l,
    }
}

/// True when the **page** tree rebuilt (route, `Show`/`For`, list remount).
///
/// A popover joining the portal layer still sets `StructureChanged` so Taffy can attach
/// the new overlay nodes. Treating that as a page rebuild pins class `:hover` to a
/// sentinel mouse, every hovered card leaves at once, and the unioned paint damage
/// covers the viewport → FullClear (bubble +N flash after overlay-only FullClear was skipped).
#[inline]
pub fn page_structure_rebuild(
    structure_changed: bool,
    portal_membership_changed: bool,
    page_geometry_changed: bool,
) -> bool {
    structure_changed
        && !skip_full_clear_for_portal_structure(portal_membership_changed, page_geometry_changed)
}

/// Decide Clear vs Load+scissor from present flags and the accumulated damage rect.
pub fn frame_clear_policy(
    has_presented_frame: bool,
    force_full_redraw: bool,
    damage: Option<[f32; 4]>,
    viewport: [f32; 4],
) -> FrameClearPolicy {
    if !has_presented_frame || force_full_redraw {
        return FrameClearPolicy::FullClear;
    }
    match damage {
        Some(d) if !damage_covers_full(viewport, d) => {
            // Off-screen / non-intersecting damage cannot PartialLoad safely.
            if intersect_rects(d, viewport).is_none() {
                FrameClearPolicy::FullClear
            } else {
                FrameClearPolicy::PartialLoad
            }
        }
        _ => FrameClearPolicy::FullClear,
    }
}

/// After chrome sync: FullClear only when the **page** structure/geometry moved,
/// or nobody named a dirty node (fallback).
///
/// Nested island `StructureChanged` (MessageScroller Send / For inside a clip)
/// names a local box and must stay PartialLoad — FullClear is what kept the
/// sidebar Footer on the Send present path.
///
/// Widget-local `LayoutChanged` (InputAdornment ring, fit-relayout of a fixed
/// box) must not FullClear — that wipes CSS spin dirties and Load-stamps
/// leftover glyphs on the next PartialLoad. `pre_chrome` LayoutChanged is the
/// same gate as `sync_impact`; it must not undo a named-node PartialLoad.
///
/// Viewport grow / `layout_fast` then `glyphon_fit` is **not** that local case:
/// boxes below the reflow move. Node-present dest-resets the *new* box only, so
/// pixels at the previous Y stay (Marker `shimmer-box` ghosting onto the next
/// section). Callers must pass `page_geometry_changed` when
/// [`fit_relayout_is_page_geometry`].
pub fn force_full_redraw_after_chrome(
    pre_chrome: LayoutStatus,
    sync_impact: LayoutStatus,
    page_geometry_changed: bool,
    portal_structure_skip: bool,
    has_named_damage: bool,
) -> bool {
    if portal_structure_skip {
        return false;
    }
    let structure = matches!(pre_chrome, LayoutStatus::StructureChanged)
        || matches!(sync_impact, LayoutStatus::StructureChanged);
    if structure {
        // Nested island StructureChanged (MessageScroller Send / For) names its clip
        // box and must not FullClear the sidebar Footer. Page geometry (route Show)
        // still Clears. No named box → fallback Clear.
        return page_geometry_changed || !has_named_damage;
    }
    let layout = matches!(pre_chrome, LayoutStatus::LayoutChanged)
        || matches!(sync_impact, LayoutStatus::LayoutChanged);
    if !layout {
        return false;
    }
    page_geometry_changed || !has_named_damage
}

/// Dest-reset wipes `now` only. If the last presented box is not that box,
/// Load keeps the old pixels (duplicate spin / stacked labels).
pub fn node_present_slot_moved(prev: [f32; 4], now: [f32; 4]) -> bool {
    // Sub-pixel layout noise stays paint-in-place. Smooth scroll of 1px+ is a
    // page translation: dest-reset of the named spin cannot wipe sibling labels.
    const SLACK: f32 = 0.5;
    (prev[0] - now[0]).abs() > SLACK
        || (prev[1] - now[1]).abs() > SLACK
        || (prev[2] - now[2]).abs() > SLACK * 2.0
        || (prev[3] - now[3]).abs() > SLACK * 2.0
}

/// Named dirties license paint-in-place, not a page translation.
///
/// `NodePaint.bounds` and chrome dirties are **visual** (abs − scroll). Scroll,
/// maximize, and `glyphon_fit` all move that box. Dest-reset of *now* only leaves
/// the previous visual Y in retain — this gate FullClears when the last presented
/// visual slot is not the current one.
pub fn node_present_dirties_moved(
    dirties: &[PaintDirty],
    current_slots: &[(u64, [f32; 4])],
    previous_slots: &[(u64, [f32; 4])],
) -> bool {
    if previous_slots.is_empty() {
        return false;
    }
    dirties.iter().any(|d| {
        if d.kind != PaintDirtyKind::Node {
            return false;
        }
        let Some(now) = current_slots
            .iter()
            .find(|(k, b)| paint_dirty_matches_slot(d.key, d.bounds, *k, *b))
            .map(|(_, b)| *b)
        else {
            return false;
        };
        let Some(prev) = previous_slots
            .iter()
            .find(|(k, _)| *k == d.key)
            .map(|(_, b)| *b)
        else {
            return false;
        };
        node_present_slot_moved(prev, now)
    })
}

/// `glyphon_fit` after a live-resize / fast-measure frame reflows **page** Y.
/// Local Input fit in a stable viewport is not this — that box does not move siblings.
pub fn fit_relayout_is_page_geometry(
    layout_fast: bool,
    viewport_resizing: bool,
    text_settling: bool,
) -> bool {
    layout_fast || viewport_resizing || text_settling
}

/// Paint-node present: PartialLoad unless a **single** node covers the viewport.
/// The union of many local nodes must not FullClear (mass hover-leave).
pub fn frame_clear_policy_for_nodes(
    has_presented_frame: bool,
    force_full_redraw: bool,
    nodes: &[PaintDirty],
    viewport: [f32; 4],
) -> FrameClearPolicy {
    if !has_presented_frame || force_full_redraw {
        return FrameClearPolicy::FullClear;
    }
    if nodes.is_empty() {
        return FrameClearPolicy::FullClear;
    }
    if nodes.iter().any(|n| damage_covers_full(viewport, n.bounds)) {
        return FrameClearPolicy::FullClear;
    }
    if nodes
        .iter()
        .all(|n| intersect_rects(n.bounds, viewport).is_none())
    {
        return FrameClearPolicy::FullClear;
    }
    FrameClearPolicy::PartialLoad
}

/// Axis-aligned intersection of `[x,y,w,h]` rects. `None` when empty / no overlap.
pub fn intersect_rects(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let x2 = ax2.min(bx2);
    let y2 = ay2.min(by2);
    if x2 <= x || y2 <= y {
        return None;
    }
    Some([x, y, x2 - x, y2 - y])
}

/// Convert logical damage + pad to a physical scissor contained in the render target.
///
/// Returns `None` when the padded rect does not intersect the target (caller must FullClear
/// or skip scissor). Never returns a rect that fails wgpu's "contained in render target" check.
pub fn damage_to_scissor_physical(
    damage: [f32; 4],
    pad_logical: f32,
    scale: f32,
    target_w: u32,
    target_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if target_w == 0 || target_h == 0 || damage[2] <= 0.0 || damage[3] <= 0.0 {
        return None;
    }
    let scale = scale.max(0.0);
    let pad = pad_logical.max(0.0);
    let x0 = ((damage[0] - pad) * scale).floor();
    let y0 = ((damage[1] - pad) * scale).floor();
    let x1 = ((damage[0] + damage[2] + pad) * scale).ceil();
    let y1 = ((damage[1] + damage[3] + pad) * scale).ceil();

    let x0 = x0.clamp(0.0, target_w as f32) as u32;
    let y0 = y0.clamp(0.0, target_h as f32) as u32;
    let x1 = x1.clamp(0.0, target_w as f32) as u32;
    let y1 = y1.clamp(0.0, target_h as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// Logical plan: copy the still-visible overlap, Clear+redraw only the newly exposed band(s).
///
/// `delta` is `new_scroll − old_scroll`. Positive `dy` means the user scrolled down
/// (content moves up on screen). Near-zero on both axes or `|delta| ≥ viewport`
/// → `None` (caller keeps a full viewport-layer Clear). Dual-axis is an L of two
/// disjoint strips — never Clear their AABB union (that would wipe the copy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollStripBlit {
    pub viewport: [f32; 4],
    pub delta: [f32; 2],
    pub src: [f32; 4],
    pub dst_origin: [f32; 2],
    /// Newly exposed primary band `[x,y,w,h]` (logical px). Single-axis: the only strip.
    /// Dual-axis: full-width `|dy|` band (corner lives here).
    pub exposed: [f32; 4],
    /// Dual-axis vertical `|dx| × overlap_h` band. Disjoint from [`Self::exposed`].
    pub exposed_second: Option<[f32; 4]>,
}

impl ScrollStripBlit {
    pub fn exposed_strips(self) -> impl Iterator<Item = [f32; 4]> {
        std::iter::once(self.exposed).chain(self.exposed_second)
    }

    /// AABB of the L. Present must **not** Clear this — only each strip.
    pub fn exposed_union(self) -> [f32; 4] {
        match self.exposed_second {
            Some(b) => union_rects(self.exposed, b),
            None => self.exposed,
        }
    }
}

/// Integer-pixel copy of [`ScrollStripBlit::src`] → [`ScrollStripBlit::dst_origin`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollStripCopyPhysical {
    pub src_x: u32,
    pub src_y: u32,
    pub dst_x: u32,
    pub dst_y: u32,
    pub width: u32,
    pub height: u32,
}

const STRIP_NEAR_ZERO: f32 = 0.5;

/// One planned strip blit, tagged with the scroll viewport id.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollStripBlitJob {
    pub viewport_id: String,
    pub plan: ScrollStripBlit,
}

/// Conservative present gates in front of [`plan_scroll_strip_blit`].
///
/// Overlay / named paint are **pin holes**, not abort gates (fields kept so
/// callers still report them). Overlapping siblings blit both and pin occupancy
/// ([`overlapping_sibling_occupancy_pins`]). `false` → keep today's full
/// viewport-layer Clear. Does **not** sign whole B8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollStripBlitPolicy {
    pub enabled: bool,
    pub gpu_partial: bool,
    pub retain_valid: bool,
    pub force_full_redraw: bool,
    /// Named (non-scroll) paint dirty set is non-empty — pin, do not abort.
    pub named_paint_dirty: bool,
    /// Overlay occupancy / leftover — pin when it intersects a blit viewport.
    pub overlay: bool,
}

impl ScrollStripBlitPolicy {
    pub fn allows(self) -> bool {
        self.enabled && self.gpu_partial && self.retain_valid && !self.force_full_redraw
    }
}

/// All viewports whose offset bits changed. Spawn / despawn do **not** abort
/// moved ids (those jobs blit; despawned bounds are pinned by the caller).
///
/// Empty moved set → `None` (spawn-only / idle).
pub fn changed_viewport_scroll_deltas(
    prev: &HashMap<String, [u32; 2]>,
    cur: &HashMap<String, [u32; 2]>,
) -> Option<Vec<(String, [f32; 2])>> {
    let mut out = Vec::new();
    for (id, off) in cur {
        let Some(old) = prev.get(id) else {
            continue;
        };
        if old == off {
            continue;
        }
        let dx = f32::from_bits(off[0]) - f32::from_bits(old[0]);
        let dy = f32::from_bits(off[1]) - f32::from_bits(old[1]);
        out.push((id.clone(), [dx, dy]));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Viewport ids present last frame and gone this frame (pin last clip bounds).
pub fn despawned_viewport_ids(
    prev: &HashMap<String, [u32; 2]>,
    cur: &HashMap<String, [u32; 2]>,
) -> Vec<String> {
    prev.keys()
        .filter(|id| !cur.contains_key(*id))
        .cloned()
        .collect()
}

/// Exactly one viewport changed its offset bits.
///
/// Two viewports moving, or an empty moved set (idle / spawn-only) → `None`.
pub fn single_viewport_scroll_delta(
    prev: &HashMap<String, [u32; 2]>,
    cur: &HashMap<String, [u32; 2]>,
) -> Option<(String, [f32; 2])> {
    let mut v = changed_viewport_scroll_deltas(prev, cur)?;
    if v.len() == 1 {
        Some(v.pop().unwrap())
    } else {
        None
    }
}

/// Nested scrollers cannot blit independently (outer copy smears inner pixels).
///
/// - **Containment** (inner inside outer): blit the outer, drop the inner so the
///   caller can pin-Clear the inner viewport.
/// - **Overlap without containment** (siblings): keep both; caller pins
///   [`overlapping_sibling_occupancy_pins`].
/// - **Disjoint**: keep both.
pub fn partition_disjoint_blit_jobs(
    jobs: Vec<ScrollStripBlitJob>,
) -> (Vec<ScrollStripBlitJob>, Vec<ScrollStripBlitJob>) {
    let n = jobs.len();
    if n <= 1 {
        return (jobs, Vec::new());
    }
    let mut keep = vec![true; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let a = jobs[i].plan.viewport;
            let b = jobs[j].plan.viewport;
            if intersect_rects(a, b).is_none() {
                continue;
            }
            let a_has_b = rect_contains_rect(a, b);
            let b_has_a = rect_contains_rect(b, a);
            if a_has_b && !b_has_a {
                keep[j] = false;
            } else if b_has_a && !a_has_b {
                keep[i] = false;
            } else if a_has_b && b_has_a {
                keep[i] = false;
                keep[j] = false;
            }
        }
    }
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for (job, k) in jobs.into_iter().zip(keep) {
        if k {
            kept.push(job);
        } else {
            dropped.push(job);
        }
    }
    (kept, dropped)
}

#[inline]
fn translate_rect(r: [f32; 4], delta: [f32; 2]) -> [f32; 4] {
    [r[0] + delta[0], r[1] + delta[1], r[2], r[3]]
}

/// Intersection occupancy after both copies. Nested containment → `None`
/// (inner is a full-viewport pin, not this hole).
pub fn overlapping_sibling_occupancy_pin(
    a_viewport: [f32; 4],
    a_delta: [f32; 2],
    b_viewport: [f32; 4],
    b_delta: [f32; 2],
) -> Option<[f32; 4]> {
    let inter = intersect_rects(a_viewport, b_viewport)?;
    if rect_contains_rect(a_viewport, b_viewport) || rect_contains_rect(b_viewport, a_viewport) {
        return None;
    }
    let hole = union_rects(
        inter,
        union_rects(
            translate_rect(inter, a_delta),
            translate_rect(inter, b_delta),
        ),
    );
    intersect_rects(hole, union_rects(a_viewport, b_viewport))
}

/// Pairwise occupancy pins for kept blit jobs that overlap without containment.
pub fn overlapping_sibling_occupancy_pins(jobs: &[ScrollStripBlitJob]) -> Vec<[f32; 4]> {
    let mut pins = Vec::new();
    for i in 0..jobs.len() {
        for j in (i + 1)..jobs.len() {
            if let Some(pin) = overlapping_sibling_occupancy_pin(
                jobs[i].plan.viewport,
                jobs[i].plan.delta,
                jobs[j].plan.viewport,
                jobs[j].plan.delta,
            ) {
                push_scroll_blit_pin(&mut pins, pin);
            }
        }
    }
    pins
}

/// Overlay leftover / current paint must pin when it shares pixels with a blit.
pub fn overlay_needs_scroll_blit_pin(overlay: [f32; 4], viewports: &[[f32; 4]]) -> bool {
    if overlay[2] <= 0.0 || overlay[3] <= 0.0 {
        return false;
    }
    viewports
        .iter()
        .any(|v| intersect_rects(*v, overlay).is_some())
}

/// Sticky descendants of `root` with a non-empty layout box.
pub fn sticky_layout_bounds(root: &Element) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        if el.style.position_sticky {
            let b = el.layout_bounds;
            if b[2] > 0.0 && b[3] > 0.0 {
                out.push(b);
            }
        }
    });
    out
}

fn push_scroll_blit_pin(pins: &mut Vec<[f32; 4]>, r: [f32; 4]) {
    if r[2] <= 0.0 || r[3] <= 0.0 {
        return;
    }
    if pins.iter().any(|p| {
        (p[0] - r[0]).abs() < 0.01
            && (p[1] - r[1]).abs() < 0.01
            && (p[2] - r[2]).abs() < 0.01
            && (p[3] - r[3]).abs() < 0.01
    }) {
        return;
    }
    pins.push(r);
}

/// Extra Clear+redraw holes on top of exposed strips. Never unioned into one AABB.
pub fn merge_scroll_blit_pins(
    overlay: Option<[f32; 4]>,
    overlay_boxes: &[[f32; 4]],
    overlay_restore: bool,
    named: &[[f32; 4]],
    sticky: &[[f32; 4]],
    nested_layer_clear: &[[f32; 4]],
    blit_viewports: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    let mut pins = Vec::new();
    if overlay_restore {
        if let Some(o) = overlay {
            push_scroll_blit_pin(&mut pins, o);
        }
        for b in overlay_boxes {
            push_scroll_blit_pin(&mut pins, *b);
        }
    } else if let Some(o) = overlay {
        if overlay_needs_scroll_blit_pin(o, blit_viewports) {
            push_scroll_blit_pin(&mut pins, o);
            for b in overlay_boxes {
                push_scroll_blit_pin(&mut pins, *b);
            }
        }
    }
    for n in named {
        push_scroll_blit_pin(&mut pins, *n);
    }
    for s in sticky {
        push_scroll_blit_pin(&mut pins, *s);
    }
    for v in nested_layer_clear {
        push_scroll_blit_pin(&mut pins, *v);
    }
    pins
}

/// Axis-aligned strip copy of a rotated viewport smears. Abort that viewport
/// to layer Clear; do **not** blit the rotated AABB as strips. Walls in
/// [`plan_scroll_strip_blit`] stay walls.
pub fn scroll_strip_blit_viewport_axis_aligned(el: &Element) -> bool {
    if el.style.rotate_deg.abs() > 1e-4 {
        return false;
    }
    if !crate::style::mat2d_is_axis_aligned(&el.paint_ancestor_linear) {
        return false;
    }
    let w = el.layout_bounds[2].max(1.0);
    let h = el.layout_bounds[3].max(1.0);
    let offset = crate::style::offset_path::sample(&el.style, [w, h]);
    let local = crate::style::rotate_and_list_mat2d(
        el.style.rotate_deg + offset.rotate_deg,
        el.style.transform.as_ref(),
        w,
        h,
    );
    crate::style::mat2d_is_axis_aligned(&crate::style::mat2d_linear(&local))
}

/// Pure geometry. No GPU. One or two axes; `|delta| ≥ viewport` or both near-zero → `None`.
pub fn plan_scroll_strip_blit(viewport: [f32; 4], delta: [f32; 2]) -> Option<ScrollStripBlit> {
    let [vx, vy, vw, vh] = viewport;
    if vw <= 0.0 || vh <= 0.0 {
        return None;
    }
    let dx = if delta[0].abs() >= STRIP_NEAR_ZERO {
        delta[0]
    } else {
        0.0
    };
    let dy = if delta[1].abs() >= STRIP_NEAR_ZERO {
        delta[1]
    } else {
        0.0
    };
    let ax = dx.abs();
    let ay = dy.abs();
    if ax < STRIP_NEAR_ZERO && ay < STRIP_NEAR_ZERO {
        return None;
    }
    if ax + 1e-3 >= vw || ay + 1e-3 >= vh {
        return None;
    }
    let overlap_w = vw - ax;
    let overlap_h = vh - ay;
    let src = [vx + dx.max(0.0), vy + dy.max(0.0), overlap_w, overlap_h];
    let dst_origin = [vx + (-dx).max(0.0), vy + (-dy).max(0.0)];
    let exp_h = if ay >= STRIP_NEAR_ZERO {
        let y = if dy > 0.0 { vy + vh - ay } else { vy };
        Some([vx, y, vw, ay])
    } else {
        None
    };
    let exp_v = if ax >= STRIP_NEAR_ZERO {
        let x = if dx > 0.0 { vx + vw - ax } else { vx };
        let y = if dy > 0.0 { vy } else { vy + ay };
        Some([x, y, ax, overlap_h])
    } else {
        None
    };
    let (exposed, exposed_second) = match (exp_h, exp_v) {
        (Some(h), Some(v)) => (h, Some(v)),
        (Some(h), None) => (h, None),
        (None, Some(v)) => (v, None),
        (None, None) => return None,
    };
    Some(ScrollStripBlit {
        viewport,
        delta,
        src,
        dst_origin,
        exposed,
        exposed_second,
    })
}

/// Integer copy matching [`plan_scroll_strip_blit`]. `None` when the rounded physical
/// delta is 0 on both axes or the copy would leave the target.
pub fn scroll_strip_copy_physical(
    plan: &ScrollStripBlit,
    scale: f32,
    target_w: u32,
    target_h: u32,
) -> Option<ScrollStripCopyPhysical> {
    let (x, y, w, h) = damage_to_scissor_physical(plan.viewport, 0.0, scale, target_w, target_h)?;
    let dpx = (plan.delta[0] * scale.max(0.0)).round() as i32;
    let dpy = (plan.delta[1] * scale.max(0.0)).round() as i32;
    if dpx == 0 && dpy == 0 {
        return None;
    }
    let adx = dpx.unsigned_abs();
    let ady = dpy.unsigned_abs();
    if adx >= w || ady >= h {
        return None;
    }
    let src_x = if dpx > 0 { x + adx } else { x };
    let src_y = if dpy > 0 { y + ady } else { y };
    let dst_x = if dpx < 0 { x + adx } else { x };
    let dst_y = if dpy < 0 { y + ady } else { y };
    let width = w - adx;
    let height = h - ady;
    if src_x.saturating_add(width) > target_w
        || src_y.saturating_add(height) > target_h
        || dst_x.saturating_add(width) > target_w
        || dst_y.saturating_add(height) > target_h
    {
        return None;
    }
    Some(ScrollStripCopyPhysical {
        src_x,
        src_y,
        dst_x,
        dst_y,
        width,
        height,
    })
}

/// Union `bounds` into `damage_rect` for paint-only updates (same-width `bind_text`, etc.).
///
/// Does **not** set `force_full_redraw` — that would force a full Clear and defeat WP-2.2.
pub fn accumulate_paint_damage(damage_rect: &mut Option<[f32; 4]>, bounds: [f32; 4]) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    *damage_rect = Some(match *damage_rect {
        Some(existing) => {
            rects_union(&[existing, bounds]).unwrap_or_else(|| union_rects(existing, bounds))
        }
        None => bounds,
    });
}

/// Union two axis-aligned rects `[x, y, w, h]`.
pub fn union_rects(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let x2 = ax2.max(bx2);
    let y2 = ay2.max(by2);
    [x, y, x2 - x, y2 - y]
}

/// Union an arbitrary list of rects. Returns `None` when `rects` is empty.
pub fn rects_union(rects: &[[f32; 4]]) -> Option<[f32; 4]> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    Some(iter.fold(first, union_rects))
}

/// True when `damage` fully covers `viewport` (same coordinate space, `[x,y,w,h]`).
///
/// Empty / degenerate damage never covers. Floating slack of 0.5px accounts for layout rounding.
pub fn damage_covers_full(viewport: [f32; 4], damage: [f32; 4]) -> bool {
    if viewport[2] <= 0.0 || viewport[3] <= 0.0 || damage[2] <= 0.0 || damage[3] <= 0.0 {
        return false;
    }
    const EPS: f32 = 0.5;
    let (vx, vy, vw, vh) = (viewport[0], viewport[1], viewport[2], viewport[3]);
    let (dx, dy, dw, dh) = (damage[0], damage[1], damage[2], damage[3]);
    dx <= vx + EPS && dy <= vy + EPS && dx + dw + EPS >= vx + vw && dy + dh + EPS >= vy + vh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_kind_for_modal_chrome_is_overlay() {
        let mut overlay = Element::new();
        overlay.template_id = Some("dlg__modal_overlay".into());
        assert_eq!(paint_kind_for_element(&overlay), PaintDirtyKind::Overlay);
        let mut hoisted = Element::new();
        hoisted.set_attr("rui_hoisted_portal", "1");
        assert_eq!(paint_kind_for_element(&hoisted), PaintDirtyKind::Overlay);
        assert_eq!(
            paint_kind_for_element(&Element::new()),
            PaintDirtyKind::Node
        );
    }

    #[test]
    fn nested_input_dirty_skips_dest_reset_inside_adornment() {
        let shell = PaintDirty {
            key: 1,
            bounds: [10.0, 10.0, 320.0, 40.0],
            kind: PaintDirtyKind::Node,
        };
        let input = PaintDirty {
            key: 2,
            bounds: [12.0, 12.0, 260.0, 36.0],
            kind: PaintDirtyKind::Node,
        };
        assert!(paint_dirty_nested_in_larger_node(&input, &[shell, input]));
        assert!(!paint_dirty_nested_in_larger_node(&shell, &[shell, input]));
        assert!(!paint_dirty_nested_in_larger_node(&input, &[input]));
    }

    #[test]
    fn stacked_input_dirties_must_not_skip_sibling_dest_reset() {
        // Gallery forms: leave field already unfocused (pad blur+8=9), enter field
        // focused (pad spread+8=11). Pads overlap across gap-8. Intersect+area
        // treated the larger enter dirty as an ancestor and skipped the leave wipe.
        let leave = PaintDirty {
            key: 1,
            bounds: [8.0, 8.0, 380.0, 36.0 + 18.0],
            kind: PaintDirtyKind::Node,
        };
        let enter = PaintDirty {
            key: 2,
            bounds: [8.0, 8.0 + 36.0 + 8.0 - 11.0, 380.0, 36.0 + 22.0],
            kind: PaintDirtyKind::Node,
        };
        assert!(
            intersect_rects(leave.bounds, enter.bounds).is_some(),
            "precondition: padded sibling dirties must overlap (the false nested case)"
        );
        assert!(
            enter.bounds[2] * enter.bounds[3] > leave.bounds[2] * leave.bounds[3] + 1.0,
            "precondition: focused sibling dirty is larger (unfocus pad vs ring pad)"
        );
        assert!(
            !paint_dirty_nested_in_larger_node(&leave, &[leave, enter]),
            "leave Input dest-reset must still run when a sibling focus dirty overlaps"
        );
        assert!(!paint_dirty_nested_in_larger_node(&enter, &[leave, enter]));
    }

    #[test]
    fn bind_text_dirty_must_use_visual_scroll_not_document() {
        let _ = take_frame_paint_nodes();
        let mut el = Element::new().w(400.0).h(20.0);
        el.layout_node = Some(taffy::prelude::NodeId::from(42u64));
        el.layout_bounds = [16.0, 800.0, 400.0, 18.0];
        el.paint_scroll_acc = [0.0, 600.0];
        let visual = crate::core::tree::visual_paint_aabb(&el, el.paint_scroll_acc);
        assert!(
            (visual[1] - 200.0).abs() < 0.5,
            "visual y must be document y minus scroll, got {}",
            visual[1]
        );

        push_frame_paint_element(&el, [0.0, 0.0]);
        let document_dirty = take_frame_paint_nodes();
        assert_eq!(document_dirty.len(), 1);
        assert!(
            !paint_dirty_matches_slot(
                document_dirty[0].key,
                document_dirty[0].bounds,
                paint_element_key(&el),
                visual,
            ),
            "document-space bind_text dirty must not match the scrolled collect slot (gallery click caption freeze / wipe)"
        );

        push_frame_paint_element(&el, el.paint_scroll_acc);
        let visual_dirty = take_frame_paint_nodes();
        assert_eq!(visual_dirty.len(), 1);
        assert!(
            paint_dirty_matches_slot(
                visual_dirty[0].key,
                visual_dirty[0].bounds,
                paint_element_key(&el),
                visual,
            ),
            "bind_text StyleChanged must dest-reset the visual box collect actually painted"
        );
    }

    #[test]
    fn union_disjoint_rects() {
        let u = union_rects([0.0, 0.0, 10.0, 10.0], [20.0, 5.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn union_overlapping_rects() {
        let u = union_rects([0.0, 0.0, 20.0, 20.0], [10.0, 10.0, 20.0, 20.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 30.0]);
    }

    #[test]
    fn union_contained_rect() {
        let u = union_rects([0.0, 0.0, 100.0, 100.0], [25.0, 25.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 100.0, 100.0]);
    }

    #[test]
    fn rects_union_empty_and_many() {
        assert_eq!(rects_union(&[]), None);
        assert_eq!(
            rects_union(&[[0.0, 0.0, 10.0, 10.0]]),
            Some([0.0, 0.0, 10.0, 10.0])
        );
        let u = rects_union(&[
            [0.0, 0.0, 10.0, 10.0],
            [20.0, 5.0, 10.0, 10.0],
            [5.0, 0.0, 5.0, 5.0],
        ])
        .expect("non-empty");
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn damage_covers_full_viewport() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert!(damage_covers_full(vp, vp));
        assert!(damage_covers_full(vp, [-1.0, -1.0, 802.0, 602.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 400.0, 600.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn frame_clear_policy_partial_for_local_damage() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::PartialLoad
        );
    }

    #[test]
    fn frame_clear_policy_full_when_damage_outside_viewport() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, false, Some([100.0, 700.0, 200.0, 40.0]), vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn damage_to_scissor_clamps_inside_target() {
        // Regression: y=738 with h=1 on a 600-tall target panicked wgpu validation.
        assert_eq!(
            damage_to_scissor_physical([228.0, 730.0, 230.0, 20.0], 8.0, 1.0, 800, 600),
            None
        );
        let s = damage_to_scissor_physical([100.0, 500.0, 50.0, 80.0], 8.0, 1.0, 800, 600)
            .expect("intersects bottom");
        assert!(s.0 + s.2 <= 800);
        assert!(s.1 + s.3 <= 600);
        assert!(s.3 >= 1);
    }

    #[test]
    fn intersect_rects_basic() {
        assert_eq!(
            intersect_rects([0.0, 0.0, 100.0, 100.0], [50.0, 50.0, 100.0, 100.0]),
            Some([50.0, 50.0, 50.0, 50.0])
        );
        assert_eq!(
            intersect_rects([0.0, 0.0, 10.0, 10.0], [20.0, 20.0, 10.0, 10.0]),
            None
        );
    }

    #[test]
    fn frame_clear_policy_full_when_forced_or_covers() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, true, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(false, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(true, false, Some(vp), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(true, false, None, vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn overlay_partial_load_does_not_repaint_page() {
        assert!(overlay_partial_load_retains_page(true, false, true, true));
        assert!(
            !overlay_partial_load_retains_page(false, false, true, true),
            "FullClear must still paint page glyphs"
        );
        assert!(
            !overlay_partial_load_retains_page(true, true, true, true),
            "scroll-layer Clear wiped the scissor; page must repaint"
        );
        assert!(!overlay_partial_load_retains_page(true, false, false, true));
        assert!(
            !overlay_partial_load_retains_page(true, false, true, false),
            "page text in the overlay scissor changed; must repaint glyphs"
        );
        assert!(
            overlay_partial_load_retains_page(true, false, true, true),
            "overlay present + PartialLoad must keep retain page pixels"
        );
    }

    #[test]
    fn overlay_close_or_shrink_needs_scissor_clear() {
        let panel = [90.0, 140.0, 40.0, 160.0];
        let chip = [90.0, 260.0, 32.0, 28.0];
        assert!(overlay_scissor_needs_clear(Some(panel), None));
        assert!(
            overlay_scissor_needs_clear(Some(panel), Some(chip)),
            "exit keyframes leave expanded pixels in retain; Restore then redraw"
        );
        assert!(
            !overlay_scissor_needs_clear(Some(chip), Some(panel)),
            "grow: new pixels were page; Load + overlay draw is enough"
        );
        assert!(!overlay_scissor_needs_clear(Some(panel), Some(panel)));
        assert!(!overlay_scissor_needs_clear(None, Some(panel)));
        assert!(overlay_restore_needed(Some(panel), Some(chip)));
        assert!(!overlay_restore_needed(Some(chip), Some(panel)));
        assert!(!overlay_restore_needed(None, Some(panel)));
    }

    #[test]
    fn skip_full_clear_only_when_portal_membership_is_isolated() {
        assert!(skip_full_clear_for_portal_structure(true, false));
        assert!(!skip_full_clear_for_portal_structure(true, true));
        assert!(!skip_full_clear_for_portal_structure(false, false));
        assert!(!skip_full_clear_for_portal_structure(false, true));
    }

    #[test]
    fn overlay_open_is_not_a_page_structure_rebuild() {
        assert!(
            !page_structure_rebuild(true, true, false),
            "popover / bubble +N hoist must keep real pointer for class :hover"
        );
        assert!(page_structure_rebuild(true, false, false));
        assert!(page_structure_rebuild(true, true, true));
        assert!(!page_structure_rebuild(false, true, false));
    }

    #[test]
    fn overlay_without_layout_box_cannot_partial_load() {
        assert!(overlay_pixels_need_full_clear(true, None));
        assert!(!overlay_pixels_need_full_clear(
            true,
            Some([90.0, 190.0, 160.0, 48.0])
        ));
        assert!(!overlay_pixels_need_full_clear(false, None));
    }

    #[test]
    fn leftover_chip_damage_grows_to_overlay_box() {
        let chip = [100.0, 200.0, 24.0, 24.0];
        let overlay = [90.0, 190.0, 160.0, 48.0];
        let u = union_rects(chip, overlay);
        assert!(u[2] >= overlay[2] - 0.5);
        assert!(u[3] >= overlay[3] - 0.5);
        assert!(u[0] <= overlay[0] + 0.5);
        assert!(u[1] <= overlay[1] + 0.5);
    }

    #[test]
    fn overlay_restore_occupancy_covers_visual_overflow() {
        let chip = [400.0, 200.0, 32.0, 24.0];
        let visual_panel = [360.0, 140.0, 120.0, 88.0];
        assert_eq!(
            union_overlay_visual_bounds(None, [0.0, 0.0, 0.0, 0.0]),
            None
        );
        let occupied = union_overlay_visual_bounds(Some(chip), visual_panel).expect("visual");
        let hole = crate::app::draw_cull::padded_xywh(occupied, OVERLAY_RESTORE_PAD_LOGICAL);
        assert!(
            rect_contains_rect(hole, visual_panel),
            "collapse Clear must cover transform/overflow paint, not just the chip layout box"
        );
        assert!(rect_contains_rect(hole, chip));
        assert!(overlay_restore_needed(Some(occupied), Some(chip)));
    }

    #[test]
    fn force_full_redraw_keeps_partial_load_for_named_nodes_without_page_geom() {
        use crate::core::tree::LayoutStatus::*;
        // Typing / caret / spin: pre_chrome may still be LayoutChanged from
        // fit-relayout of a fixed Input box. Named dirties must PartialLoad.
        assert!(
            !force_full_redraw_after_chrome(LayoutChanged, StyleChanged, false, false, true),
            "pre_chrome LayoutChanged must not undo Input+spin PartialLoad"
        );
        assert!(
            !force_full_redraw_after_chrome(Clean, LayoutChanged, false, false, true),
            "chrome LayoutChanged with named dirties stays PartialLoad"
        );
        assert!(
            force_full_redraw_after_chrome(LayoutChanged, StyleChanged, true, false, true),
            "page geometry (window resize / text-fit settle) FullClears even with named spin"
        );
        assert!(
            fit_relayout_is_page_geometry(true, false, false),
            "maximize layout_fast then glyphon_fit is page geometry"
        );
        assert!(
            !fit_relayout_is_page_geometry(false, false, false),
            "stable-viewport Input fit is not page geometry"
        );
        assert!(
            force_full_redraw_after_chrome(
                LayoutChanged,
                StyleChanged,
                fit_relayout_is_page_geometry(true, false, false),
                false,
                true
            ),
            "named spin must not keep PartialLoad after maximize fit reflow — dest-reset of the new box leaves the old Y"
        );
        assert!(
            node_present_dirties_moved(
                &[PaintDirty {
                    key: 7,
                    bounds: [10.0, 90.0, 16.0, 16.0],
                    kind: PaintDirtyKind::Node,
                }],
                &[(7, [10.0, 90.0, 16.0, 16.0])],
                &[(7, [10.0, 10.0, 16.0, 16.0])],
            ),
            "named spin after a page Y shift cannot dest-reset the previous pixels"
        );
        assert!(
            !node_present_dirties_moved(
                &[PaintDirty {
                    key: 7,
                    bounds: [10.0, 10.0, 16.0, 16.0],
                    kind: PaintDirtyKind::Node,
                }],
                &[(7, [10.0, 10.0, 16.0, 16.0])],
                &[(7, [10.0, 10.0, 16.0, 16.0])],
            ),
            "idle spin at a stable box stays paint-in-place"
        );
        assert!(
            node_present_dirties_moved(
                &[PaintDirty {
                    key: 7,
                    bounds: [10.0, 12.0, 16.0, 16.0],
                    kind: PaintDirtyKind::Node,
                }],
                &[(7, [10.0, 12.0, 16.0, 16.0])],
                &[(7, [10.0, 10.0, 16.0, 16.0])],
            ),
            "smooth-scroll 2px is still a visual translation — 8px slack left stacked labels"
        );
        assert!(
            !node_present_dirties_moved(
                &[PaintDirty {
                    key: 7,
                    bounds: [10.0, 10.0, 16.0, 16.0],
                    kind: PaintDirtyKind::Node,
                }],
                &[(7, [10.0, 10.0, 16.0, 16.0])],
                &[],
            ),
            "first present has no previous pixels to leave behind"
        );
        assert!(
            force_full_redraw_after_chrome(LayoutChanged, Clean, false, false, false),
            "nobody named a dirty box → FullClear fallback"
        );
        assert!(
            !force_full_redraw_after_chrome(StructureChanged, Clean, false, false, true),
            "island StructureChanged with named damage stays PartialLoad (Send must not wipe Footer)"
        );
        assert!(
            force_full_redraw_after_chrome(StructureChanged, Clean, true, false, true),
            "page geometry StructureChanged FullClears"
        );
        assert!(
            force_full_redraw_after_chrome(StructureChanged, Clean, false, false, false),
            "StructureChanged with no named box → FullClear fallback"
        );
        assert!(
            !force_full_redraw_after_chrome(StructureChanged, Clean, false, true, true),
            "portal-only structure skip"
        );
    }

    #[test]
    fn overlay_membership_drops_viewport_hover_union() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        let overlay = [90.0, 190.0, 160.0, 48.0];
        let d = overlay_membership_damage(Some(vp), Some(overlay), vp, true, false);
        assert_eq!(d, Some(overlay));
        assert_eq!(
            frame_clear_policy(true, false, d, vp),
            FrameClearPolicy::PartialLoad
        );
        let kept = overlay_membership_damage(
            Some([100.0, 200.0, 24.0, 24.0]),
            Some(overlay),
            vp,
            false,
            false,
        )
        .expect("union");
        assert!(kept[2] >= overlay[2] - 0.5);
        assert!(!damage_covers_full(vp, kept));
    }

    #[test]
    fn overlay_in_scene_partial_loads_when_box_is_local() {
        // Overlay instances existing is not `force_full_redraw`. Chip hover + overlay box
        // stay PartialLoad (gallery bubble +N / +1 → emoji must not FullClear the page).
        let vp = [0.0, 0.0, 800.0, 600.0];
        let chip = [100.0, 200.0, 24.0, 24.0];
        let overlay = [90.0, 190.0, 160.0, 48.0];
        let damage = union_rects(chip, overlay);
        assert_eq!(
            frame_clear_policy(true, false, Some(damage), vp),
            FrameClearPolicy::PartialLoad
        );
        assert!(!overlay_pixels_need_full_clear(true, Some(overlay)));
        assert!(skip_full_clear_for_portal_structure(true, false));
    }

    #[test]
    fn mass_hover_leave_union_covers_viewport_full_clears() {
        // Class-hover sentinel leave on a gallery of cards: union → FullClear even when
        // force_full_redraw is false (the leftover flash after overlay-only skip).
        let vp = [0.0, 0.0, 800.0, 600.0];
        let tiles = [
            [0.0, 0.0, 420.0, 320.0],
            [380.0, 0.0, 420.0, 320.0],
            [0.0, 280.0, 420.0, 320.0],
            [380.0, 280.0, 420.0, 320.0],
        ];
        let u = rects_union(&tiles).expect("tiles");
        assert!(damage_covers_full(vp, u));
        assert_eq!(
            frame_clear_policy(true, false, Some(u), vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn popover_bounds_partial_load_modal_underlay_full_clear() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, false, Some([120.0, 80.0, 72.0, 36.0]), vp),
            FrameClearPolicy::PartialLoad
        );
        assert_eq!(
            frame_clear_policy(true, false, Some(vp), vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn accumulate_paint_damage_does_not_require_full_clear() {
        let mut damage = None;
        let force = false;
        accumulate_paint_damage(&mut damage, [100.0, 200.0, 50.0, 16.0]);
        // Callers must not set force_full when using this helper (WP-2.2 bind_text).
        assert!(!force);
        assert_eq!(damage, Some([100.0, 200.0, 50.0, 16.0]));
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, force, damage, vp),
            FrameClearPolicy::PartialLoad
        );
    }

    #[test]
    fn push_frame_paint_damage_is_local_not_full_viewport() {
        let _ = take_frame_paint_nodes();
        push_frame_paint_damage([10.0, 20.0, 40.0, 24.0]);
        push_frame_paint_damage([30.0, 20.0, 40.0, 24.0]);
        let nodes = take_frame_paint_nodes();
        assert_eq!(nodes.len(), 2, "disjoint widgets must not union");
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert!(!nodes.iter().any(|d| damage_covers_full(vp, d.bounds)));
        assert_eq!(
            frame_clear_policy_for_nodes(true, false, &nodes, vp),
            FrameClearPolicy::PartialLoad
        );
        assert!(take_frame_paint_nodes().is_empty());
    }

    #[test]
    fn disjoint_nodes_covering_viewport_still_partial_load() {
        let _ = take_frame_paint_nodes();
        push_frame_paint_damage([0.0, 0.0, 400.0, 600.0]);
        push_frame_paint_damage([400.0, 0.0, 400.0, 600.0]);
        let nodes = take_frame_paint_nodes();
        let vp = [0.0, 0.0, 800.0, 600.0];
        let union = rects_union(&nodes.iter().map(|d| d.bounds).collect::<Vec<_>>()).unwrap();
        assert!(damage_covers_full(vp, union));
        assert_eq!(
            frame_clear_policy_for_nodes(true, false, &nodes, vp),
            FrameClearPolicy::PartialLoad,
            "union covering the viewport is not a node FullClear"
        );
    }

    #[test]
    fn hover_node_text_keeps_own_label_rejects_sibling() {
        let default_btn = [8.0, 8.0, 88.0, 36.0];
        let default_label = [24.0, 16.0, 56.0, 18.0];
        let outline_label = [104.0, 16.0, 56.0, 18.0];
        assert!(
            text_center_in_dirty(default_label, default_btn),
            "hovered Default must redraw its own label after chrome overwrites retain"
        );
        assert!(
            !text_center_in_dirty(outline_label, default_btn),
            "sibling Outline must not stamp into Default's scissor"
        );
    }

    #[test]
    fn dirty_slot_match_rejects_ancestor_and_underlapping_neighbor() {
        let chip = [400.0, 200.0, 32.0, 24.0];
        let padded = [396.0, 196.0, 40.0, 32.0];
        let page = [0.0, 0.0, 800.0, 600.0];
        let title = [80.0, 180.0, 120.0, 20.0];
        assert!(paint_dirty_matches_slot(1, padded, 1, chip));
        assert!(paint_dirty_matches_slot(99, padded, 1, chip));
        assert!(
            !paint_dirty_matches_slot(1, chip, 2, page),
            "page root must not paint into a chip scissor"
        );
        assert!(
            !paint_dirty_matches_slot(1, chip, 3, title),
            "nearby caption must not paint into a chip scissor"
        );
    }

    #[test]
    fn keyed_spin_must_not_also_match_same_box_parent() {
        // MarkerIcon `size-4` wraps Icon `size-4`. `same_box` matches both;
        // the parent's textured range includes the lucide quad. Drawing both
        // src-overs the spinner twice on PartialLoad (idle thicker than scroll).
        let icon = [8.0, 8.0, 16.0, 16.0];
        let parent = [8.0, 8.0, 16.0, 16.0];
        let rot_pad = (16.0f32.hypot(16.0) - 16.0).max(0.0) * 0.5;
        let pad = 2.0 + rot_pad;
        let dirty = [
            icon[0] - pad,
            icon[1] - pad,
            icon[2] + pad * 2.0,
            icon[3] + pad * 2.0,
        ];
        let icon_key = 10u64;
        let parent_key = 9u64;
        let slots = [(icon_key, icon), (parent_key, parent)];
        assert!(
            paint_dirty_matches_slot(icon_key, dirty, parent_key, parent),
            "raw same_box still matches the wrapper — present must not use that"
        );
        let has_keyed = paint_dirty_has_keyed_slot(icon_key, dirty, slots);
        assert!(has_keyed, "Icon layout_node must win over the wrapper");
        assert!(paint_dirty_matches_present_slot(
            icon_key, dirty, icon_key, icon, has_keyed
        ));
        assert!(
            !paint_dirty_matches_present_slot(icon_key, dirty, parent_key, parent, has_keyed),
            "wrapper collect range must not redraw the child's spinner a second time"
        );
        let bounds_key = 99u64;
        let has_keyed = paint_dirty_has_keyed_slot(bounds_key, dirty, slots);
        assert!(!has_keyed);
        assert!(
            paint_dirty_matches_present_slot(bounds_key, dirty, icon_key, icon, has_keyed),
            "bounds-key chrome with no Taffy slot still uses same_box"
        );
    }

    #[test]
    fn recycled_taffy_key_must_not_match_disjoint_sidebar_footer() {
        let footer = [0.0, 752.0, 207.0, 48.0];
        let chat = [420.0, 180.0, 384.0, 384.0];
        assert!(
            !paint_dirty_matches_slot(42, chat, 42, footer),
            "NodeId reuse after Send must not treat SidebarFooter as the MessageScroller island"
        );
        let hover = [8.0, 760.0, 190.0, 36.0];
        assert!(
            paint_dirty_matches_slot(42, hover, 42, footer),
            "same-widget hover still matches the footer slot"
        );
    }

    #[test]
    fn restore_slot_skips_page_root_keeps_covered_widget() {
        let hole = [400.0, 200.0, 80.0, 60.0];
        let page = [0.0, 0.0, 800.0, 600.0];
        let bubble = [410.0, 210.0, 60.0, 36.0];
        let neighbor = [80.0, 40.0, 160.0, 20.0];
        assert!(!restore_slot_in_hole(page, hole));
        assert!(restore_slot_in_hole(bubble, hole));
        assert!(!restore_slot_in_hole(neighbor, hole));
    }

    #[test]
    fn note_frame_clear_policy_counts() {
        reset_frame_policy_counters_for_test();
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        note_frame_clear_policy(FrameClearPolicy::FullClear);
        assert_eq!(partial_load_frame_count(), 2);
        assert_eq!(full_clear_frame_count(), 1);
    }

    #[test]
    fn suppress_skips_increment_and_snapshot_captures() {
        reset_frame_policy_counters_for_test();
        suppress_policy_counts(1);
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        assert_eq!(partial_load_frame_count(), 0);
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        assert_eq!(partial_load_frame_count(), 1);
        arm_policy_snapshot();
        note_frame_clear_policy(FrameClearPolicy::FullClear);
        let snap = take_policy_snapshot().expect("armed");
        assert_eq!(snap.partial_load, 1);
        assert_eq!(snap.full_clear, 1);
        assert!(take_policy_snapshot().is_none());
    }

    #[test]
    fn scroll_layer_clear_keeps_viewport_damage() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        let overlay = [90.0, 190.0, 160.0, 48.0];
        let d = overlay_membership_damage(Some(vp), Some(overlay), vp, true, true);
        assert_eq!(d, Some(vp));
    }

    #[test]
    fn scroll_strip_blit_viewport_axis_aligned_rejects_rotate() {
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 200.0, 100.0];
        assert!(
            scroll_strip_blit_viewport_axis_aligned(&el),
            "identity viewport may blit"
        );
        el.style.rotate_deg = 45.0;
        assert!(
            !scroll_strip_blit_viewport_axis_aligned(&el),
            "local rotate must abort strip blit"
        );
        el.style.rotate_deg = 0.0;
        el.paint_ancestor_linear = crate::style::mat2d_rotate(45.0_f32.to_radians());
        assert!(
            !scroll_strip_blit_viewport_axis_aligned(&el),
            "ancestor rotate must abort strip blit"
        );
        el.paint_ancestor_linear = crate::style::MAT2D_IDENTITY;
        assert!(
            scroll_strip_blit_viewport_axis_aligned(&el),
            "cleared rotate must blit again"
        );
        assert!(
            plan_scroll_strip_blit([0.0, 0.0, 200.0, 100.0], [0.0, 16.0]).is_some(),
            "axis-aligned plan walls unchanged"
        );
        assert!(
            plan_scroll_strip_blit([0.0, 0.0, 200.0, 100.0], [0.0, 100.0]).is_none(),
            "|delta|≥viewport stays a wall"
        );
    }

    #[test]
    fn plan_scroll_strip_blit_down_exposes_bottom() {
        let vp = [10.0, 20.0, 200.0, 100.0];
        let plan = plan_scroll_strip_blit(vp, [0.0, 24.0]).expect("down");
        assert_eq!(plan.src, [10.0, 44.0, 200.0, 76.0]);
        assert_eq!(plan.dst_origin, [10.0, 20.0]);
        assert_eq!(plan.exposed, [10.0, 96.0, 200.0, 24.0]);
        assert!(plan.exposed_second.is_none());
        assert!(plan.exposed[3] < vp[3]);
    }

    #[test]
    fn plan_scroll_strip_blit_up_exposes_top() {
        let vp = [0.0, 0.0, 100.0, 80.0];
        let plan = plan_scroll_strip_blit(vp, [0.0, -16.0]).expect("up");
        assert_eq!(plan.src, [0.0, 0.0, 100.0, 64.0]);
        assert_eq!(plan.dst_origin, [0.0, 16.0]);
        assert_eq!(plan.exposed, [0.0, 0.0, 100.0, 16.0]);
        assert!(plan.exposed_second.is_none());
    }

    #[test]
    fn plan_scroll_strip_blit_right_exposes_right() {
        let vp = [0.0, 0.0, 120.0, 40.0];
        let plan = plan_scroll_strip_blit(vp, [30.0, 0.0]).expect("right");
        assert_eq!(plan.src, [30.0, 0.0, 90.0, 40.0]);
        assert_eq!(plan.dst_origin, [0.0, 0.0]);
        assert_eq!(plan.exposed, [90.0, 0.0, 30.0, 40.0]);
        assert!(plan.exposed_second.is_none());
    }

    #[test]
    fn plan_scroll_strip_blit_left_exposes_left() {
        let vp = [8.0, 4.0, 80.0, 50.0];
        let plan = plan_scroll_strip_blit(vp, [-20.0, 0.0]).expect("left");
        assert_eq!(plan.src, [8.0, 4.0, 60.0, 50.0]);
        assert_eq!(plan.dst_origin, [28.0, 4.0]);
        assert_eq!(plan.exposed, [8.0, 4.0, 20.0, 50.0]);
        assert!(plan.exposed_second.is_none());
    }

    #[test]
    fn plan_scroll_strip_blit_rejects_diagonal_full_and_tiny() {
        let vp = [0.0, 0.0, 200.0, 100.0];
        let diag = plan_scroll_strip_blit(vp, [12.0, 8.0]).expect("dual-axis");
        assert_eq!(diag.src, [12.0, 8.0, 188.0, 92.0]);
        assert_eq!(diag.dst_origin, [0.0, 0.0]);
        assert_eq!(diag.exposed, [0.0, 92.0, 200.0, 8.0]);
        assert_eq!(diag.exposed_second, Some([188.0, 0.0, 12.0, 92.0]));
        assert_eq!(diag.exposed_union()[2], 200.0);
        assert_eq!(diag.exposed_union()[3], 100.0);
        assert!(
            intersect_rects(diag.exposed, diag.exposed_second.unwrap()).is_none(),
            "L strips must be disjoint (corner lives in the |dy| band only)"
        );
        assert!(plan_scroll_strip_blit(vp, [0.0, 100.0]).is_none());
        assert!(plan_scroll_strip_blit(vp, [0.0, 0.25]).is_none());
        assert!(plan_scroll_strip_blit(vp, [0.0, 0.0]).is_none());
        assert!(plan_scroll_strip_blit([0.0, 0.0, 0.0, 10.0], [0.0, 4.0]).is_none());
    }

    #[test]
    fn scroll_strip_copy_physical_integer_delta() {
        let vp = [0.0, 0.0, 200.0, 100.0];
        let plan = plan_scroll_strip_blit(vp, [0.0, 10.0]).expect("down");
        let copy = scroll_strip_copy_physical(&plan, 2.0, 400, 200).expect("phys");
        assert_eq!(copy.src_y, 20);
        assert_eq!(copy.dst_y, 0);
        assert_eq!(copy.height, 180);
        assert_eq!(copy.width, 400);
        assert!(scroll_strip_copy_physical(&plan, 2.0, 400, 200).is_some());
        let tiny = plan_scroll_strip_blit(vp, [0.0, 0.6]).expect("logical ok");
        // 0.6 * 1.0 rounds to 1px; 0.2 * 1.0 rounds to 0 → None
        assert!(scroll_strip_copy_physical(&tiny, 1.0, 200, 100).is_some());
        let sub = plan_scroll_strip_blit(vp, [0.0, 0.6]).expect("sub");
        assert!(scroll_strip_copy_physical(&sub, 0.4, 80, 40).is_none());
        let diag = plan_scroll_strip_blit(vp, [12.0, 8.0]).expect("dual");
        let copy = scroll_strip_copy_physical(&diag, 1.0, 200, 100).expect("dual phys");
        assert_eq!(copy.src_x, 12);
        assert_eq!(copy.src_y, 8);
        assert_eq!(copy.dst_x, 0);
        assert_eq!(copy.dst_y, 0);
        assert_eq!(copy.width, 188);
        assert_eq!(copy.height, 92);
    }

    #[test]
    fn note_scroll_strip_blit_counts() {
        reset_frame_policy_counters_for_test();
        note_scroll_strip_blit();
        note_scroll_strip_blit();
        assert_eq!(scroll_strip_blit_count(), 2);
        reset_frame_policy_counters_for_test();
        assert_eq!(scroll_strip_blit_count(), 0);
    }

    fn bits(v: f32) -> u32 {
        v.to_bits()
    }

    fn allow_policy() -> ScrollStripBlitPolicy {
        ScrollStripBlitPolicy {
            enabled: true,
            gpu_partial: true,
            retain_valid: true,
            force_full_redraw: false,
            named_paint_dirty: false,
            overlay: false,
        }
    }

    #[test]
    fn scroll_strip_blit_policy_rejects_invalid_retain_not_overlay_or_named_paint() {
        assert!(allow_policy().allows());
        assert!(
            ScrollStripBlitPolicy {
                overlay: true,
                ..allow_policy()
            }
            .allows(),
            "overlay is a pin hole, not an abort gate"
        );
        assert!(
            ScrollStripBlitPolicy {
                named_paint_dirty: true,
                ..allow_policy()
            }
            .allows(),
            "named paint is a pin hole, not an abort gate"
        );
        assert!(!ScrollStripBlitPolicy {
            retain_valid: false,
            ..allow_policy()
        }
        .allows());
        assert!(!ScrollStripBlitPolicy {
            force_full_redraw: true,
            ..allow_policy()
        }
        .allows());
        assert!(!ScrollStripBlitPolicy {
            gpu_partial: false,
            ..allow_policy()
        }
        .allows());
        assert!(!ScrollStripBlitPolicy {
            enabled: false,
            ..allow_policy()
        }
        .allows());
    }

    #[test]
    fn single_viewport_scroll_delta_one_axis_ok_two_viewports_or_spawn_none() {
        let mut prev = HashMap::new();
        prev.insert("a".into(), [bits(0.0), bits(0.0)]);
        prev.insert("b".into(), [bits(0.0), bits(8.0)]);
        let mut cur = prev.clone();
        cur.insert("a".into(), [bits(0.0), bits(16.0)]);
        let (id, delta) = single_viewport_scroll_delta(&prev, &cur).expect("one viewport");
        assert_eq!(id, "a");
        assert!((delta[1] - 16.0).abs() < 1e-4);
        assert!(delta[0].abs() < 1e-4);

        cur.insert("b".into(), [bits(0.0), bits(24.0)]);
        assert!(
            single_viewport_scroll_delta(&prev, &cur).is_none(),
            "two viewports moving"
        );

        let mut spawn = prev.clone();
        spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
        assert!(single_viewport_scroll_delta(&prev, &spawn).is_none());
        assert!(single_viewport_scroll_delta(&prev, &prev).is_none());
    }

    #[test]
    fn changed_viewport_scroll_deltas_two_disjoint_ok_spawn_none() {
        let mut prev = HashMap::new();
        prev.insert("a".into(), [bits(0.0), bits(0.0)]);
        prev.insert("b".into(), [bits(0.0), bits(8.0)]);
        let mut cur = prev.clone();
        cur.insert("a".into(), [bits(0.0), bits(16.0)]);
        cur.insert("b".into(), [bits(0.0), bits(24.0)]);
        let mut deltas = changed_viewport_scroll_deltas(&prev, &cur).expect("both moved");
        deltas.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].0, "a");
        assert!((deltas[0].1[1] - 16.0).abs() < 1e-4);
        assert_eq!(deltas[1].0, "b");
        assert!((deltas[1].1[1] - 16.0).abs() < 1e-4);

        let mut spawn = prev.clone();
        spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
        assert!(changed_viewport_scroll_deltas(&prev, &spawn).is_none());
        assert!(changed_viewport_scroll_deltas(&prev, &prev).is_none());

        let mut moved_and_spawn = prev.clone();
        moved_and_spawn.insert("a".into(), [bits(0.0), bits(16.0)]);
        moved_and_spawn.insert("c".into(), [bits(0.0), bits(0.0)]);
        let deltas =
            changed_viewport_scroll_deltas(&prev, &moved_and_spawn).expect("A still moved");
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].0, "a");

        let mut despawn = prev.clone();
        despawn.remove("b");
        despawn.insert("a".into(), [bits(0.0), bits(16.0)]);
        let deltas =
            changed_viewport_scroll_deltas(&prev, &despawn).expect("A moved under despawn");
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].0, "a");
        let mut gone = despawned_viewport_ids(&prev, &despawn);
        gone.sort();
        assert_eq!(gone, ["b"]);
    }

    fn blit_job(id: &str, viewport: [f32; 4], dy: f32) -> ScrollStripBlitJob {
        ScrollStripBlitJob {
            viewport_id: id.into(),
            plan: plan_scroll_strip_blit(viewport, [0.0, dy]).expect("plan"),
        }
    }

    #[test]
    fn nested_inner_is_pinned_outer_and_disjoint_still_blit() {
        let outer = blit_job("outer", [0.0, 0.0, 400.0, 600.0], 16.0);
        let inner = blit_job("inner", [40.0, 80.0, 320.0, 400.0], 8.0);
        let side = blit_job("side", [500.0, 0.0, 200.0, 600.0], 12.0);
        let (kept, dropped) = partition_disjoint_blit_jobs(vec![outer, inner, side]);
        let mut kept_ids: Vec<_> = kept.iter().map(|j| j.viewport_id.as_str()).collect();
        kept_ids.sort_unstable();
        assert_eq!(kept_ids, ["outer", "side"]);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].viewport_id, "inner");
        let occ = overlapping_sibling_occupancy_pins(&kept);
        assert!(
            occ.is_empty(),
            "nested/disjoint kept jobs must not emit sibling occupancy, got {occ:?}"
        );
    }

    #[test]
    fn three_level_nesting_keeps_outermost_only() {
        let outer = blit_job("outer", [0.0, 0.0, 400.0, 600.0], 16.0);
        let mid = blit_job("mid", [20.0, 40.0, 360.0, 500.0], 8.0);
        let inner = blit_job("inner", [40.0, 80.0, 320.0, 400.0], 12.0);
        let (kept, dropped) = partition_disjoint_blit_jobs(vec![outer, mid, inner]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].viewport_id, "outer");
        let mut dropped_ids: Vec<_> = dropped.iter().map(|j| j.viewport_id.as_str()).collect();
        dropped_ids.sort_unstable();
        assert_eq!(dropped_ids, ["inner", "mid"]);
    }

    #[test]
    fn overlapping_sibling_viewports_keep_both() {
        let a = blit_job("a", [0.0, 0.0, 200.0, 200.0], 16.0);
        let b = blit_job("b", [100.0, 100.0, 200.0, 200.0], 8.0);
        let (kept, dropped) = partition_disjoint_blit_jobs(vec![a, b]);
        let mut kept_ids: Vec<_> = kept.iter().map(|j| j.viewport_id.as_str()).collect();
        kept_ids.sort_unstable();
        assert_eq!(kept_ids, ["a", "b"]);
        assert!(dropped.is_empty());
        assert_eq!(overlapping_sibling_occupancy_pins(&kept).len(), 1);
    }

    #[test]
    fn overlapping_sibling_occupancy_covers_intersection_and_smear() {
        let pin = overlapping_sibling_occupancy_pin(
            [0.0, 0.0, 200.0, 200.0],
            [0.0, 16.0],
            [100.0, 100.0, 200.0, 200.0],
            [0.0, 8.0],
        )
        .expect("overlap");
        let inter =
            intersect_rects([0.0, 0.0, 200.0, 200.0], [100.0, 100.0, 200.0, 200.0]).unwrap();
        assert!(
            rect_contains_rect(pin, inter),
            "pin must cover intersection"
        );
        assert!(
            rect_contains_rect(pin, [100.0, 116.0, 100.0, 84.0]),
            "pin must cover A's dest smear inside A∪B, got {pin:?}"
        );
        assert!(overlapping_sibling_occupancy_pin(
            [0.0, 0.0, 400.0, 600.0],
            [0.0, 16.0],
            [40.0, 80.0, 320.0, 400.0],
            [0.0, 8.0],
        )
        .is_none());
    }

    #[test]
    fn identical_viewports_drop_both() {
        let a = blit_job("a", [0.0, 0.0, 200.0, 200.0], 16.0);
        let b = blit_job("b", [0.0, 0.0, 200.0, 200.0], 8.0);
        let (kept, dropped) = partition_disjoint_blit_jobs(vec![a, b]);
        assert!(kept.is_empty());
        assert_eq!(dropped.len(), 2);
    }

    #[test]
    fn overlay_disjoint_from_viewport_needs_no_pin() {
        let scroller = [240.0, 0.0, 560.0, 600.0];
        assert!(!overlay_needs_scroll_blit_pin(
            [8.0, 8.0, 48.0, 32.0],
            &[scroller]
        ));
        assert!(overlay_needs_scroll_blit_pin(
            [300.0, 40.0, 160.0, 80.0],
            &[scroller]
        ));
    }

    #[test]
    fn merge_scroll_blit_pins_overlay_intersect_named_sticky() {
        let scroller = [0.0, 0.0, 400.0, 300.0];
        let overlay = [40.0, 40.0, 80.0, 48.0];
        let hover = [500.0, 10.0, 32.0, 16.0];
        let sticky = [0.0, 0.0, 400.0, 28.0];
        let pins = merge_scroll_blit_pins(
            Some(overlay),
            &[],
            false,
            &[hover],
            &[sticky],
            &[],
            &[scroller],
        );
        assert!(
            pins.iter().any(|p| (p[2] - 80.0).abs() < 0.1),
            "intersecting overlay must pin, got {pins:?}"
        );
        assert!(pins.iter().any(|p| (p[0] - 500.0).abs() < 0.1));
        assert!(pins.iter().any(|p| (p[3] - 28.0).abs() < 0.1));

        let disjoint = merge_scroll_blit_pins(
            Some([500.0, 0.0, 40.0, 40.0]),
            &[],
            false,
            &[],
            &[],
            &[],
            &[scroller],
        );
        assert!(
            disjoint.is_empty(),
            "disjoint overlay must not pin, got {disjoint:?}"
        );
    }

    #[test]
    fn sticky_layout_bounds_collects_nonzero_boxes() {
        let mut root = Element::new();
        let mut sticky = Element::new();
        sticky.style.position_sticky = true;
        sticky.layout_bounds = [0.0, 0.0, 200.0, 32.0];
        let mut other = Element::new();
        other.layout_bounds = [0.0, 32.0, 200.0, 100.0];
        root.children.push(sticky);
        root.children.push(other);
        assert_eq!(sticky_layout_bounds(&root), vec![[0.0, 0.0, 200.0, 32.0]]);
    }
}
