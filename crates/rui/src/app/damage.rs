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
//! - Page structure / page layout stay FullClear.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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
        if let Some(existing) = slot
            .iter_mut()
            .find(|d| d.key == key && d.kind == kind)
        {
            existing.bounds = union_rects(existing.bounds, bounds);
        } else {
            slot.push(PaintDirty { key, bounds, kind });
        }
    });
}

/// Record this element's visual box as its own dirty node (no cross-node union).
pub fn push_frame_paint_element(el: &Element) {
    let b = crate::core::tree::visual_paint_aabb(el, [0.0, 0.0]);
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
        PaintDirtyKind::Node,
    );
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
    push_paint_dirty(bounds_key(bounds) ^ 1, bounds, PaintDirtyKind::OverlayRestore);
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
    SNAP_SKIP.store(super::frame_stats::skipped_siblings_count(), Ordering::Relaxed);
    SNAP_PATCH.store(super::frame_stats::patched_dl_count(), Ordering::Relaxed);
    SNAPSHOT_READY.store(true, Ordering::Relaxed);
}

/// Reset counters (Gallery "Reset counters" / tests).
pub fn reset_frame_policy_counters() {
    PARTIAL_LOAD_FRAMES.store(0, Ordering::Relaxed);
    FULL_CLEAR_FRAMES.store(0, Ordering::Relaxed);
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
pub fn overlay_restore_needed(
    occupied: Option<[f32; 4]>,
    current: Option<[f32; 4]>,
) -> bool {
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
pub fn overlay_scissor_needs_clear(
    prev: Option<[f32; 4]>,
    cur: Option<[f32; 4]>,
) -> bool {
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
#[inline]
pub fn paint_dirty_matches_slot(dirty_key: u64, dirty_bounds: [f32; 4], slot_key: u64, slot_bounds: [f32; 4]) -> bool {
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
    cx >= dirty[0]
        && cy >= dirty[1]
        && cx <= dirty[0] + dirty[2]
        && cy <= dirty[1] + dirty[3]
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
    if nodes
        .iter()
        .any(|n| damage_covers_full(viewport, n.bounds))
    {
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
            "page geometry still FullClears"
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
}
