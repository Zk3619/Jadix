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
//!   inner viewports / overlay scrollbar rails). Nested containment blits the outer and pins the inner.
//!   Overlapping siblings blit both and pin the intersection occupancy (copy
//!   smear). `force_full_redraw`, invalid retain, `|delta| ≥ viewport`, and
//!   `RUI_SCROLL_STRIP_BLIT=0` stay abort / layer-Clear gates. Spawn without
//!   prev metrics does not abort sibling blits (no box to pin). A named
//!   spin/shimmer slot that moved **with** that layer must not
//!   `force_full_redraw` — that aborts strip blit. Pin holes dest-reset those
//!   widgets in the copied overlap. Slot motion **without** a scroll layer
//!   (maximize / `glyphon_fit`) still FullClears.
//! - Page route / maximize / miss → **Window** FullClear (PresentTier::Window).
//! - Nested island Structure / portal / scroll viewport → **Region** Clear
//!   (PresentTier::Region) — not window, not single-node dest-reset.
//! - Box-stable ink (bind_text, caret, spin) → **Node** dest-reset
//!   (PresentTier::Node).

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::core::tree::{Element, LayoutStatus};

/// R5 present tiers (Wave 46+): Window / Region / Node.
///
/// Layout/Structure no longer mean "always window FullClear". Nested islands are
/// Region; only page-route geometry, maximize, or dest-reset miss escalate to Window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentTier {
    /// Clear the window (or full inset) and redraw.
    Window,
    /// Scissor-Clear an island / overlay / viewport clip; redraw inside the hole.
    Region,
    /// Dest-reset one node's prev∪now; clean siblings may fingerprint-skip.
    Node,
}

/// How a paint-dirty item is submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintDirtyKind {
    /// This node's own primitives (hover chrome, bind_text, …).
    Node,
    /// Nested Structure island / list clip / overlap group: region Clear + underlay.
    Region,
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
    // Shown/entering at opacity≈0 has no dest-ink. Dest-reset of the empty
    // fill+shadow AABB punches underlay (Latest pill over wrapping thread).
    // Hide (`data-active` not `true`) still dest-resets the last faded box.
    if skip_transparent_shown_dest_reset(el) {
        return;
    }
    let b = clamp_overlay_clip_box(el, crate::core::tree::visual_paint_aabb(el, scroll_acc));
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
    let pad = element_paint_outset_pad(el, rot_pad);
    let kind = paint_kind_for_element(el);
    let key = if kind == PaintDirtyKind::Overlay {
        el.paint_overlay_stamp
            .filter(|k| *k != 0)
            .unwrap_or_else(|| crate::layout::layout_tree::overlay_portal_stamp_key(el))
    } else {
        paint_element_key(el)
    };
    push_paint_dirty(
        key,
        [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0],
        kind,
    );
}

/// Outset beyond the visual layout box that this node actually paints (CSS
/// shadow spread/blur/offset, outline, spin circumcircle). Vertex AA and border
/// are not dirty outset — Hard ring spread 3 plus those pads punched
/// `form_field` labels (`gap(4)`) on Input focus.
fn element_paint_outset_pad(el: &Element, rot_pad: f32) -> f32 {
    let mut halo = if el.style.shadow_inset {
        0.0
    } else {
        super::draw_cull::sdf_painted_halo_pad(
            el.style.shadow_blur,
            el.style.shadow_spread,
            el.style.shadow_offset,
        )
    };
    for sh in &el.style.extra_shadows {
        if sh.inset {
            continue;
        }
        halo = halo.max(super::draw_cull::sdf_painted_halo_pad(
            sh.blur, sh.spread, sh.offset,
        ));
    }
    let outline = el.style.outline_width.max(0.0) + el.style.outline_offset.max(0.0);
    halo = halo.max(outline);
    if halo <= 0.0 {
        rot_pad
    } else {
        halo + rot_pad
    }
}

/// Expand pin / Taffy can report content height while `max-height` is the accordion clip.
/// Overlay dirty must be that clip — the unconstrained AABB becomes a fixed transparent
/// hole (calendar showing through) until the fill catches up.
fn clamp_overlay_clip_box(el: &Element, mut b: [f32; 4]) -> [f32; 4] {
    if paint_kind_for_element(el) != PaintDirtyKind::Overlay {
        return b;
    }
    if let Some(h) = crate::layout::dim_length_px(el.style.max_size.height).filter(|h| *h >= 0.0) {
        b[3] = b[3].min(h);
    }
    if let Some(w) = crate::layout::dim_length_px(el.style.max_size.width).filter(|w| *w >= 0.0) {
        b[2] = b[2].min(w);
    }
    b
}

/// Hoisted modal/popover chrome must not dest-reset as a page `Node`.
/// Descendants (reaction chips, menu items) inherit overlay membership from
/// last collect (`paint_overlay_stamp`) — they are not themselves `__floating_panel`.
pub fn paint_kind_for_element(el: &Element) -> PaintDirtyKind {
    if el.paint_overlay_stamp.is_some_and(|k| k != 0) {
        return PaintDirtyKind::Overlay;
    }
    if el.attrs.get("rui_hoisted_portal").map(String::as_str) == Some("1") {
        return PaintDirtyKind::Overlay;
    }
    match el.template_id.as_deref() {
        Some(t)
            if t.ends_with("__modal_overlay")
                || t.ends_with("__modal_panel")
                || t.ends_with("__modal_zoom")
                || t.ends_with("__floating_panel") =>
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

/// Dest-reset the previous overlay box, then dirty the live follow origin.
/// CursorMoved must queue this before RedrawRequested `take_frame_paint_nodes`.
pub fn queue_overlay_follow_tick_damage(
    key: u64,
    size: [f32; 2],
    prev: Option<[f32; 2]>,
    visual: [f32; 2],
) {
    if key == 0 || size[0] < 0.5 || size[1] < 0.5 {
        return;
    }
    if let Some(prev) = prev {
        if prev != visual {
            push_frame_overlay_damage(key, [prev[0], prev[1], size[0], size[1]]);
        }
    }
    push_frame_overlay_damage(key, [visual[0], visual[1], size[0], size[1]]);
}

/// Overlay closed/shrunk: clear this box, restore intersecting page nodes.
pub fn push_frame_overlay_restore(bounds: [f32; 4]) {
    push_paint_dirty(
        bounds_key(bounds) ^ 1,
        bounds,
        PaintDirtyKind::OverlayRestore,
    );
}

/// Nested Structure island / list clip: region Clear (Wave 46/49), not node dest-reset.
pub fn push_frame_region_paint(key: u64, bounds: [f32; 4]) {
    push_paint_dirty(key, bounds, PaintDirtyKind::Region);
}

/// Restore page in the occupancy hole, then region clears, then page nodes, then overlay.
///
/// Within `Node`, larger boxes first. Nested Input inside InputAdornment skips its own
/// dest-reset when the shell is also dirty; if the Input present ran *before* the shell
/// wipe, caret / typed glyphs / ⌘K were cleared and never redrawn (click-focus flicker,
/// keystrokes only flash the placeholder).
pub fn sort_paint_dirty_nodes(nodes: &mut [PaintDirty]) {
    nodes.sort_by(|a, b| {
        let rank = |d: &PaintDirty| match d.kind {
            PaintDirtyKind::OverlayRestore => 0u8,
            PaintDirtyKind::Region => 1,
            PaintDirtyKind::Node => 2,
            PaintDirtyKind::Overlay => 3,
        };
        rank(a).cmp(&rank(b)).then_with(|| {
            if a.kind != PaintDirtyKind::Node || b.kind != PaintDirtyKind::Node {
                return std::cmp::Ordering::Equal;
            }
            let aa = a.bounds[2] * a.bounds[3];
            let ba = b.bounds[2] * b.bounds[3];
            ba.partial_cmp(&aa).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
}

/// Take this frame's dirty nodes (order: restore, region, large→small page nodes, overlay).
pub fn take_frame_paint_nodes() -> Vec<PaintDirty> {
    FRAME_PAINT_NODES.with(|slot| {
        let mut nodes = slot.borrow_mut().split_off(0);
        sort_paint_dirty_nodes(&mut nodes);
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
/// Same-size fade is not shrink — OverlayRestore *with* `overlay_restore_clear`
/// punches the panel (cannot open) and flashes the gallery. Any Overlay dirty
/// while the portal is still mounted is a **replace** ([`overlay_fade_replace_needed`])
/// so Load cannot src-over the previous keyframe (dark shadow / leftover white frame).
#[inline]
pub fn overlay_restore_needed(occupied: Option<[f32; 4]>, current: Option<[f32; 4]>) -> bool {
    match (occupied, current) {
        (Some(_), None) => true,
        (Some(occ), Some(cur)) => overlay_box_extends_beyond(occ, cur, 2.0),
        _ => false,
    }
}

/// True when the overlay footprint did not grow or move (`slack` px).
#[inline]
pub fn overlay_same_footprint(
    prev: Option<[f32; 4]>,
    current: Option<[f32; 4]>,
    slack: f32,
) -> bool {
    match (prev, current) {
        (Some(a), Some(b)) => {
            !overlay_box_extends_beyond(a, b, slack) && !overlay_box_extends_beyond(b, a, slack)
        }
        _ => false,
    }
}

/// Overlay CSS tick on PartialLoad: dest-reset the previous GPU stamp, restore
/// page in that hole, then draw overlay once at the current clip / opacity.
///
/// Src-over of a new keyframe onto the previous stacks the outset shadow
/// (Select expand/collapse looks darker than the idle panel). Accordion
/// leftover-strip Restore dest-resets a padded ring with window Clear and
/// cuts page captions — replace the previous panel box instead.
/// This Restore must not set `overlay_restore_clear` (that flag disables
/// page glyphon retain). Close (`overlay_present == false`) is occupancy
/// restore, not a keyframe replace.
#[inline]
pub fn overlay_fade_replace_needed(
    had_occupancy: bool,
    overlay_present: bool,
    overlay_dirty: bool,
    _shrink_or_close: bool,
    _same_footprint: bool,
) -> bool {
    had_occupancy && overlay_present && overlay_dirty
}

/// Previous portal boxes minus current ones. Flip below→above must Restore the
/// old chip-side box only — not the union AABB that includes the page gap
/// (forms / 2c heading) between the two positions.
pub fn overlay_restore_leftover_boxes(
    prev_boxes: &[[f32; 4]],
    cur_boxes: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    if prev_boxes.is_empty() {
        return Vec::new();
    }
    let cur: Vec<[f32; 4]> = cur_boxes
        .iter()
        .copied()
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
        .collect();
    let mut out = Vec::new();
    for prev in prev_boxes
        .iter()
        .copied()
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
    {
        let mut remnants = vec![prev];
        for c in &cur {
            remnants = remnants
                .into_iter()
                .flat_map(|r| subtract_rect(r, *c))
                .collect();
        }
        out.extend(remnants);
    }
    out
}

/// Previous portal stamps whose keys are gone this frame.
/// Nested-inside-parent must still yield the nested box — AABB subtract of the
/// parent stamp covers the menu and would skip dest-reset, then occupancy-union
/// fallbacks punched the DatePicker halo.
pub fn overlay_restore_leftover_stamps(
    prev: &[(u64, [f32; 4])],
    cur: &[(u64, [f32; 4])],
) -> Vec<(u64, [f32; 4])> {
    prev.iter()
        .copied()
        .filter(|(k, b)| {
            b[2] > 0.0
                && b[3] > 0.0
                && !cur
                    .iter()
                    .any(|(ck, cb)| *ck == *k && cb[2] > 0.0 && cb[3] > 0.0)
        })
        .collect()
}

/// Overlay suffix is submitted once per present.
///
/// Fade CSS (`push_frame_paint_element`) and portal membership each name an
/// Overlay dirty for the same panel. Encoding both replays the full overlay
/// band (`cull None`) under overlapping scissors — the fill/shadow src-over
/// twice on the first open frame (halo looks too dark), then once on later
/// ticks (looks lighter).
#[inline]
pub fn overlay_primitive_encode_count(overlay_dirty_count: usize) -> usize {
    overlay_dirty_count.min(1)
}

/// `outer` minus `hole` as up to four AABBs. Empty when `hole` covers `outer`.
pub fn subtract_rect(outer: [f32; 4], hole: [f32; 4]) -> Vec<[f32; 4]> {
    let Some(i) = intersect_rects(outer, hole) else {
        return vec![outer];
    };
    let mut out = Vec::new();
    if i[1] > outer[1] {
        out.push([outer[0], outer[1], outer[2], i[1] - outer[1]]);
    }
    let outer_y2 = outer[1] + outer[3];
    let i_y2 = i[1] + i[3];
    if i_y2 < outer_y2 {
        out.push([outer[0], i_y2, outer[2], outer_y2 - i_y2]);
    }
    if i[0] > outer[0] {
        out.push([outer[0], i[1], i[0] - outer[0], i[3]]);
    }
    let outer_x2 = outer[0] + outer[2];
    let i_x2 = i[0] + i[2];
    if i_x2 < outer_x2 {
        out.push([i_x2, i[1], outer_x2 - i_x2, i[3]]);
    }
    out.into_iter()
        .filter(|r| r[2] > 0.0 && r[3] > 0.0)
        .collect()
}

/// Page node-present scissor with overlay occupancy cut out. Multi-rect
/// remainders (trigger pad around a dropdown) keep the largest AABB so dest-reset
/// cannot punch overlay pixels. `None` = dirty is entirely inside the overlay.
pub fn page_scissor_excluding_overlay(
    scissor: [f32; 4],
    overlay_occupied: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let Some(occ) = overlay_occupied.filter(|b| b[2] > 0.0 && b[3] > 0.0) else {
        return Some(scissor);
    };
    subtract_rect(scissor, occ).into_iter().max_by(|a, b| {
        (a[2] * a[3])
            .partial_cmp(&(b[2] * b[3]))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Clicked AlertDialog trigger sits under the fullscreen scrim. Skipping dest-reset
/// alone still lets node-present glyphon src-over that caption onto the retained
/// scrim while overlay suffix scissors to the panel stamp. Capture and flush must
/// use the same occupancy test.
pub fn page_node_present_occluded_by_overlay(
    dirty_bounds: [f32; 4],
    overlay_occupied: Option<[f32; 4]>,
) -> bool {
    let scissor = super::draw_cull::padded_xywh(
        dirty_bounds,
        super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Node),
    );
    page_scissor_excluding_overlay(scissor, overlay_occupied).is_none()
}

/// Overlap of a page node-present scissor with overlay occupancy.
#[inline]
pub fn overlay_page_punch_overlap(
    overlay_occupied: Option<[f32; 4]>,
    page_scissor: [f32; 4],
) -> Option<[f32; 4]> {
    overlay_occupied
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
        .and_then(|occ| intersect_rects(occ, page_scissor))
}

/// True when dest-reset will punch occupancy: overlap exists and the page
/// scissor was not cut down (`clipped == original`, e.g. a caller that skipped
/// [`page_scissor_excluding_overlay`]). Fully-inside dirties (`clipped == None`)
/// skip dest-reset and are not punches.
#[inline]
pub fn overlay_page_dirty_punches_occupancy(
    original_scissor: [f32; 4],
    clipped_scissor: Option<[f32; 4]>,
    overlay_occupied: Option<[f32; 4]>,
) -> bool {
    let Some(overlap) = overlay_page_punch_overlap(overlay_occupied, original_scissor) else {
        return false;
    };
    match clipped_scissor {
        None => false,
        Some(c) => c == original_scissor && overlap[2] > 0.0 && overlap[3] > 0.0,
    }
}

/// Overlay suffix scissor after Restore/Node dest-reset.
///
/// Fade Overlay dirty may sit in the wrong space — occupancy is the GPU footprint
/// and unions with the dirty so the panel stays in scissor. A dirty that **strictly
/// contains** occupancy (pinned expand overflow) keeps the stamp so the scissor is
/// not unconstrained content height. A dirty that merely sticks *out* of occupancy
/// (month menu flipped above the DatePicker) must union, or PartialLoad never draws
/// it. Hover dest-reset that could not be clipped redraws only the punch strip.
pub fn overlay_encode_scissor(
    overlay_dirty_scissor: Option<[f32; 4]>,
    overlay_occupied: Option<[f32; 4]>,
    page_punch_overlap: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let dirty = overlay_dirty_scissor.filter(|b| b[2] > 0.0 && b[3] > 0.0);
    let occ = overlay_occupied
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
        .map(|b| {
            super::draw_cull::padded_xywh(
                b,
                super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Overlay),
            )
        });
    let punch = page_punch_overlap
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
        .map(|b| {
            super::draw_cull::padded_xywh(
                b,
                super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Overlay),
            )
        });
    match (dirty, occ, punch) {
        (Some(d), Some(o), _) => {
            let slack = overlay_stamp_slack();
            // Accordion overflow: dirty is unconstrained content height of *this*
            // clip stamp (same origin / width). A ColorPicker Fade dirty that
            // merely contains a leftover DatePicker occupancy is a different
            // portal — KEEP occupancy clips the popover off-screen (blank open).
            if overlay_dirty_is_expand_overflow(d, o, slack) {
                Some(o)
            } else {
                Some(union_rects(d, o))
            }
        }
        (Some(d), None, _) => Some(d),
        (None, _, Some(p)) => Some(p),
        _ => None,
    }
}

/// Overlay paint dirty is that portal's layout box plus shadow / Overlay scissor pad.
fn overlay_stamp_slack() -> f32 {
    super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Overlay) * 2.0
}

/// Expand pin reports content height while occupancy is the accordion clip stamp.
/// Same `x` / `y` / `w` — only height grew. Foreign leftover occupancy (DatePicker
/// vs ColorPicker) shares neither origin nor width.
fn overlay_dirty_is_expand_overflow(dirty: [f32; 4], occupancy: [f32; 4], slack: f32) -> bool {
    if !overlay_box_extends_beyond(dirty, occupancy, slack)
        || overlay_box_extends_beyond(occupancy, dirty, slack)
    {
        return false;
    }
    (dirty[0] - occupancy[0]).abs() <= slack
        && (dirty[1] - occupancy[1]).abs() <= slack
        && (dirty[2] - occupancy[2]).abs() <= slack
}

/// Layout box of the portal stamp `key`.
pub fn overlay_stamp_box(stamps: &[(u64, [f32; 4])], key: u64) -> Option<[f32; 4]> {
    stamps
        .iter()
        .find(|(k, b)| *k == key && b[2] > 0.0 && b[3] > 0.0)
        .map(|(_, b)| *b)
}

/// Portal layout boxes this Overlay dirty is a keyframe of (geometry tests only).
pub fn overlay_boxes_intersecting_dirty(
    overlay_dirty: Option<[f32; 4]>,
    portal_boxes: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    let Some(d) = overlay_dirty.filter(|b| b[2] > 0.0 && b[3] > 0.0) else {
        return Vec::new();
    };
    let slack = overlay_stamp_slack();
    portal_boxes
        .iter()
        .copied()
        .filter(|b| b[2] > 0.0 && b[3] > 0.0 && overlay_same_footprint(Some(*b), Some(d), slack))
        .collect()
}

/// Overlay dirties for a portal join / leave (not a same-set CSS tick).
///
/// Overlay **paint** is a join: stamps in `cur` that were not in `prev`.
///
/// A leave is [`overlay_restore_leftover_stamps`] (OverlayRestore), not an Overlay
/// dirty of the closed key. Painting the leftover DatePicker key while ColorPicker
/// is opening makes `dirty_has_keyed_band` true for the DatePicker band and skips
/// the ColorPicker band — the popover never submits (blank open).
pub fn overlay_membership_paint_stamps(
    prev: &[(u64, [f32; 4])],
    cur: &[(u64, [f32; 4])],
) -> Vec<(u64, [f32; 4])> {
    overlay_added_portal_stamps(prev, cur)
}

/// Overlay dirties for a portal join / leave from layout boxes (geometry tests).
pub fn overlay_membership_paint_boxes(prev: &[[f32; 4]], cur: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let added = overlay_added_portal_boxes(prev, cur);
    if !added.is_empty() {
        return added;
    }
    let closed = overlay_added_portal_boxes(cur, prev);
    if !closed.is_empty() {
        return closed;
    }
    Vec::new()
}

/// Portals in `cur` whose stamp key was not in `prev`.
pub fn overlay_added_portal_stamps(
    prev: &[(u64, [f32; 4])],
    cur: &[(u64, [f32; 4])],
) -> Vec<(u64, [f32; 4])> {
    cur.iter()
        .copied()
        .filter(|(k, b)| b[2] > 0.0 && b[3] > 0.0 && overlay_stamp_box(prev, *k).is_none())
        .collect()
}

/// Portals in `cur` that were not already in `prev` (membership join).
pub fn overlay_added_portal_boxes(prev: &[[f32; 4]], cur: &[[f32; 4]]) -> Vec<[f32; 4]> {
    cur.iter()
        .copied()
        .filter(|c| c[2] > 0.0 && c[3] > 0.0)
        .filter(|c| {
            !prev
                .iter()
                .any(|p| overlay_same_footprint(Some(*p), Some(*c), 2.0))
        })
        .collect()
}

/// Occupancy fed to [`overlay_encode_scissor`] for a fade / membership Overlay dirty.
///
/// Named by stamp **key**, never AABB / sibling-union occupancy. Wrong-space dirty
/// bounds still resolve to this portal's layout box. Closed / unstamped keys use
/// the Overlay dirty hole itself — not the remaining parent visual.
pub fn overlay_encode_occupancy(
    dirty_key: u64,
    overlay_dirty: Option<[f32; 4]>,
    stamps: &[(u64, [f32; 4])],
    full_occupancy: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let dirty = overlay_dirty.filter(|b| b[2] > 0.0 && b[3] > 0.0);
    if let Some(layout) = overlay_stamp_box(stamps, dirty_key) {
        let with_dirty = match dirty {
            Some(d) if overlay_same_footprint(Some(d), Some(layout), overlay_stamp_slack()) => {
                union_rects(layout, d)
            }
            Some(d) if overlay_dirty_is_contained_keyframe(d, layout) => d,
            _ => layout,
        };
        if stamps.len() <= 1 {
            return Some(match full_occupancy.filter(|b| b[2] > 0.0 && b[3] > 0.0) {
                Some(visual) if overlay_visual_is_stamp_halo(with_dirty, visual) => {
                    union_rects(with_dirty, visual)
                }
                _ => with_dirty,
            });
        }
        return Some(with_dirty);
    }
    dirty
}

/// `visual` is this portal's outset halo (contains the layout box), not a leftover
/// sibling occupancy from a TabsContent unmount (DatePicker box vs ColorPicker).
pub fn overlay_visual_is_stamp_halo(layout: [f32; 4], visual: [f32; 4]) -> bool {
    let slack = overlay_stamp_slack();
    if overlay_same_footprint(Some(layout), Some(visual), slack) {
        return true;
    }
    if overlay_box_extends_beyond(layout, visual, slack) {
        return false;
    }
    if !overlay_box_extends_beyond(visual, layout, slack) {
        return false;
    }
    // Outset shadow, not a leftover sibling popover that happens to contain this stamp.
    let extra_l = (layout[0] - visual[0]).max(0.0);
    let extra_t = (layout[1] - visual[1]).max(0.0);
    let extra_r = ((visual[0] + visual[2]) - (layout[0] + layout[2])).max(0.0);
    let extra_b = ((visual[1] + visual[3]) - (layout[1] + layout[3])).max(0.0);
    extra_l.max(extra_t).max(extra_r).max(extra_b) <= slack
}

/// Fade-replace Restore boxes: this dirty's stamp, or the Overlay dirty hole
/// when the key has already left (never the union of sibling portals).
pub fn overlay_fade_replace_restore_boxes(
    dirty_key: u64,
    overlay_dirty: Option<[f32; 4]>,
    stamps: &[(u64, [f32; 4])],
) -> Vec<[f32; 4]> {
    if let Some(b) = overlay_stamp_box(stamps, dirty_key) {
        return vec![b];
    }
    overlay_dirty
        .filter(|b| b[2] > 0.0 && b[3] > 0.0)
        .into_iter()
        .collect()
}

/// GPU ink that a mounted overlay must dest-reset before redrawing.
///
/// Identity is the stamp **key**. Join (no previous box for this key) dest-resets
/// nothing — OverlayRestore of occupancy would punch the parent. Leave dest-resets
/// the previous box of that key even when it sits inside the parent AABB.
pub fn overlay_mounted_replace_holes(
    prev_stamps: &[(u64, [f32; 4])],
    cur_stamps: &[(u64, [f32; 4])],
    dirty_key: u64,
    overlay_dirty: Option<[f32; 4]>,
) -> Vec<[f32; 4]> {
    let prev = overlay_stamp_box(prev_stamps, dirty_key);
    let cur = overlay_stamp_box(cur_stamps, dirty_key);
    let dirty = overlay_dirty.filter(|b| b[2] > 0.0 && b[3] > 0.0);
    let trust_dirty = |stamp: [f32; 4], d: [f32; 4]| {
        overlay_same_footprint(Some(d), Some(stamp), overlay_stamp_slack())
    };
    match (prev, cur) {
        // Unstamped join (measure omits the nested key, or instances still live
        // in the parent band). Dest-reset of `dirty` is the unconstrained expand
        // AABB — it punches the DatePicker fill and page captions behind it.
        (None, None) => Vec::new(),
        (None, Some(_)) => Vec::new(),
        (Some(p), None) => match dirty {
            Some(d) if trust_dirty(p, d) => vec![union_rects(p, d)],
            _ => vec![p],
        },
        (Some(p), Some(c)) => {
            if intersect_rects(p, c).is_none() {
                let mut out = vec![p, c];
                if let Some(d) = dirty {
                    if trust_dirty(c, d) {
                        out[1] = union_rects(c, d);
                    } else if trust_dirty(p, d) {
                        out[0] = union_rects(p, d);
                    }
                }
                return out;
            }
            let mut hole = union_rects(p, c);
            if let Some(d) = dirty {
                if trust_dirty(hole, d) || trust_dirty(p, d) || trust_dirty(c, d) {
                    hole = union_rects(hole, d);
                } else if overlay_dirty_is_contained_keyframe(d, hole) {
                    // Dialog zoom card lives inside a fullscreen centering stage.
                    // Dest-reset of the empty host punches the sibling scrim;
                    // overlay_band_should_encode then skips the underlay because
                    // the dirty key is the panel stamp.
                    hole = d;
                }
            }
            vec![hole]
        }
    }
}

/// Whether this overlay paint band should be submitted.
///
/// Named by stamp key. A leftover Restore of a **closed** nested key still needs
/// remaining parent bands in that hole. Never "AABB intersects encode scissor".
///
/// Unkeyed stacking bands belong to the preceding portal-layer stamp — see
/// [`overlay_band_encode_flags`]. Nested Select first-open still lives in the
/// sole DatePicker band: the dirty key has no band yet, so that host encodes.
pub fn overlay_band_should_encode(
    band_key: u64,
    overlay_dirty_keys: &[u64],
    closed_restore: bool,
    remaining_stamp_keys: &[u64],
    overlay_band_keys: &[u64],
) -> bool {
    if band_key != 0 && overlay_dirty_keys.iter().any(|k| *k == band_key) {
        return true;
    }
    if closed_restore && band_key != 0 && remaining_stamp_keys.iter().any(|k| *k == band_key) {
        return true;
    }
    if band_key == 0 {
        return !overlay_dirty_keys.is_empty() || closed_restore;
    }
    let dirty_has_keyed_band = overlay_dirty_keys
        .iter()
        .any(|k| *k != 0 && overlay_band_keys.iter().any(|b| *b == *k));
    if dirty_has_keyed_band {
        return false;
    }
    let keyed_count = overlay_band_keys.iter().filter(|k| **k != 0).count();
    keyed_count == 1 && !overlay_dirty_keys.is_empty()
}

/// Per-band encode flags. Unkeyed stacking (ColorPicker SV/hue, month-list
/// z-index) follows the preceding keyed portal overlay: skipped when that
/// host is skipped, submitted when that host is submitted.
pub fn overlay_band_encode_flags(
    overlay_band_keys: &[u64],
    overlay_dirty_keys: &[u64],
    closed_restore: bool,
    remaining_stamp_keys: &[u64],
) -> Vec<bool> {
    let mut flags = Vec::with_capacity(overlay_band_keys.len());
    let mut seen_keyed = false;
    let mut keyed_host_encoding = false;
    for &band_key in overlay_band_keys {
        let encode = if band_key != 0 {
            seen_keyed = true;
            let e = overlay_band_should_encode(
                band_key,
                overlay_dirty_keys,
                closed_restore,
                remaining_stamp_keys,
                overlay_band_keys,
            );
            keyed_host_encoding = e;
            e
        } else if seen_keyed {
            keyed_host_encoding
        } else {
            overlay_band_should_encode(
                0,
                overlay_dirty_keys,
                closed_restore,
                remaining_stamp_keys,
                overlay_band_keys,
            )
        };
        flags.push(encode);
    }
    flags
}

/// OverlayRestore keys that are no longer mounted (closed nested hole).
pub fn overlay_restore_is_closed_portal(
    restore_keys: &[u64],
    remaining_stamp_keys: &[u64],
) -> bool {
    restore_keys
        .iter()
        .any(|k| *k != 0 && !remaining_stamp_keys.contains(k))
}

/// Overlay primitives are submitted only after their own physical scissor is set.
/// Drawing under a leftover page Node scissor clips the panel to the trigger chip
/// (invisible but hittable; FullClear on page switch shows it).
#[inline]
pub fn overlay_suffix_submitted(physical_scissor: Option<(u32, u32, u32, u32)>) -> bool {
    physical_scissor.is_some()
}

/// Shown (`data-active=true`) with no current dest-ink.
fn skip_transparent_shown_dest_reset(el: &Element) -> bool {
    el.attrs.get("data-active").map(String::as_str) == Some("true") && el.style.opacity < 0.01
}

/// CSS dirty of a descendant (Dialog `{id}__modal_zoom` card) inside a much larger
/// portal host (fullscreen centering stage). Not the same footprint — dest-reset of
/// the host punches the sibling scrim.
pub(crate) fn overlay_dirty_is_contained_keyframe(dirty: [f32; 4], stamp: [f32; 4]) -> bool {
    let slack = overlay_stamp_slack();
    !overlay_box_extends_beyond(dirty, stamp, slack)
        && overlay_box_extends_beyond(stamp, dirty, slack)
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

/// Whether this Node dirty should skip its own dest-reset because a larger
/// ancestor dirty will redraw the fill.
///
/// In-flow children (Kbd Input inside InputAdornment) skip. Later page-stack
/// (`layered`) slots — MessageScroller Latest pill, overlay scrollbar — paint in
/// their own band. Ancestor present does not recapture that band, so skipping
/// the wipe leaves the last opaque pixels in PartialLoad retain.
#[inline]
pub fn paint_dirty_skips_node_dest_reset(
    dirty: &PaintDirty,
    nodes: &[PaintDirty],
    layered: bool,
) -> bool {
    !layered && paint_dirty_nested_in_larger_node(dirty, nodes)
}

/// Node dirty whose center sits in an OverlayRestore / Region hole.
///
/// Cover overlays (AvatarGroup +N / Bubble +N) and Wave 49 Region islands punch
/// overlapping underlay, then Restore/Region redraws the page prefix in tree
/// order. A follow-up per-face Node dest-reset CLEARs that stacking (later face
/// wiped by an earlier sibling) for one frame.
#[inline]
pub fn paint_dirty_covered_by_overlay_restore(dirty: &PaintDirty, nodes: &[PaintDirty]) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    if dirty.bounds[2] <= 0.0 || dirty.bounds[3] <= 0.0 {
        return false;
    }
    let area = dirty.bounds[2] * dirty.bounds[3];
    let cx = dirty.bounds[0] + dirty.bounds[2] * 0.5;
    let cy = dirty.bounds[1] + dirty.bounds[3] * 0.5;
    nodes.iter().any(|o| {
        matches!(
            o.kind,
            PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region
        ) && o.bounds[2] * o.bounds[3] > area + 1.0
            && cx >= o.bounds[0]
            && cy >= o.bounds[1]
            && cx < o.bounds[0] + o.bounds[2]
            && cy < o.bounds[1] + o.bounds[3]
    })
}

/// Overlay dest-reset hole: repaint page fills that sit behind it.
///
/// [`restore_slot_in_hole`] skips the card/page that fully contains the hole so a
/// close Restore does not pull every descendant caption. That left the dest-reset
/// AABB (window/card white) in retain around a rounded panel and through the
/// translucent shadow — a flashing white frame on every expand/collapse tick.
/// Instance ranges are per-widget; scissor already clips to the hole.
#[inline]
pub fn restore_page_behind_overlay(slot_bounds: [f32; 4], hole: [f32; 4]) -> bool {
    intersect_rects(slot_bounds, hole).is_some()
}

/// Roughly-square chip (Avatar / +N). Wide group rows and form fields are not.
#[inline]
pub fn is_rest_stack_chip(bounds: [f32; 4]) -> bool {
    let w = bounds[2];
    let h = bounds[3];
    if w < 8.0 || h < 8.0 || w > 96.0 || h > 96.0 {
        return false;
    }
    w.max(h) <= w.min(h) * 1.35 + 0.5
}

/// OverlayRestore replay rank: containing fills (0) before leaf widgets (1).
///
/// `node_paints` is post-order (AvatarGroup faces, then card). Replaying every
/// intersecting slot in that order paints the card/page fill *after* overlapping
/// A/B/C/+N and wipes stacking for a frame. `None` = skip (neighbor / non-intersect).
///
/// Cover overlay holes are often *larger* than the collapsed row, so the row is
/// not `area > hole`. Rank 0 is every non-chip restore slot (row / page / card);
/// rank 1 is only stacked chips so they src-over last.
#[inline]
pub fn overlay_restore_replay_rank(slot_bounds: [f32; 4], hole: [f32; 4]) -> Option<u8> {
    if !restore_page_behind_overlay(slot_bounds, hole) {
        return None;
    }
    let slot_area = (slot_bounds[2] * slot_bounds[3]).max(1.0);
    let hole_area = (hole[2] * hole[3]).max(1.0);
    if is_rest_stack_chip(slot_bounds) {
        return if restore_slot_in_hole(slot_bounds, hole) {
            Some(1)
        } else {
            None
        };
    }
    if slot_area > hole_area + 1.0 || restore_slot_in_hole(slot_bounds, hole) {
        Some(0)
    } else {
        None
    }
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

/// After chrome sync: classify into [`PresentTier`], then Window → FullClear.
///
/// Nested island `StructureChanged` (MessageScroller Send / For inside a clip)
/// names a local **Region** and must stay PartialLoad — FullClear is what kept
/// the sidebar Footer on the Send present path.
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
    has_region_damage: bool,
) -> bool {
    matches!(
        classify_present_tier(
            pre_chrome,
            sync_impact,
            page_geometry_changed,
            portal_structure_skip,
            has_named_damage,
            has_region_damage,
        ),
        PresentTier::Window
    )
}

/// Classify the frame present tier (R5 / Wave 46).
///
/// - `page_geometry_changed` → Window (route Match, maximize, fit reflow).
/// - Portal membership only → Region (overlay layer Clear).
/// - Structure with named/region damage → Region (island clip).
/// - Structure with nothing named → Window fallback.
/// - Layout with named damage → Node (widget-local / absorbed leaf).
/// - Layout with nothing named → Window fallback.
/// - Style/Clean with named → Node; else Window fallback.
pub fn classify_present_tier(
    pre_chrome: LayoutStatus,
    sync_impact: LayoutStatus,
    page_geometry_changed: bool,
    portal_structure_skip: bool,
    has_named_damage: bool,
    has_region_damage: bool,
) -> PresentTier {
    if page_geometry_changed {
        return PresentTier::Window;
    }
    if portal_structure_skip {
        return PresentTier::Region;
    }
    let structure = matches!(pre_chrome, LayoutStatus::StructureChanged)
        || matches!(sync_impact, LayoutStatus::StructureChanged);
    if structure {
        if has_region_damage || has_named_damage {
            return PresentTier::Region;
        }
        return PresentTier::Window;
    }
    let layout = matches!(pre_chrome, LayoutStatus::LayoutChanged)
        || matches!(sync_impact, LayoutStatus::LayoutChanged);
    if layout {
        if has_region_damage {
            return PresentTier::Region;
        }
        if has_named_damage {
            return PresentTier::Node;
        }
        return PresentTier::Window;
    }
    if has_region_damage {
        return PresentTier::Region;
    }
    if has_named_damage {
        return PresentTier::Node;
    }
    PresentTier::Window
}

/// Route-sized island damage (Gallery `Match` section body) must Window-Clear,
/// not dest-reset a title slot. True when the region covers most of the viewport.
pub fn region_is_page_route_tier(region: [f32; 4], viewport: [f32; 4]) -> bool {
    if region[2] <= 0.0 || region[3] <= 0.0 || viewport[2] <= 0.0 || viewport[3] <= 0.0 {
        return false;
    }
    if damage_covers_full(viewport, region) {
        return true;
    }
    let area = region[2] * region[3];
    let vp_area = viewport[2] * viewport[3];
    area >= vp_area * 0.45
}

/// Dest-reset miss (Taffy id / last-present gone) must escalate: Node → Region → Window.
pub fn promote_present_tier_on_miss(tier: PresentTier, miss: bool) -> PresentTier {
    if !miss {
        return tier;
    }
    match tier {
        PresentTier::Node => PresentTier::Region,
        PresentTier::Region | PresentTier::Window => PresentTier::Window,
    }
}

/// True when a dest-reset lookup failed: previous slot existed but the key is
/// gone from this frame's paints (Taffy id recycle / unmounted). First present
/// (`!had_previous`) is not a miss — wipe `now` only.
pub fn dest_reset_slot_miss(found_current: bool, had_previous: bool) -> bool {
    had_previous && !found_current
}

/// Wave 51 escalate for a Node dirty: miss → Region; visual AABB growth stays Node.
///
/// Spin / hover-scale change size every tick (`node_present_slot_moved` is true).
/// Those must keep Node dest-reset (`prev∪now`). Promoting them to Region was the
/// Gallery spin 叠影 regression after R5.
pub fn escalate_node_dest_reset_kind(
    kind: PaintDirtyKind,
    found_current: bool,
    had_previous: bool,
) -> PaintDirtyKind {
    if kind != PaintDirtyKind::Node {
        return kind;
    }
    if dest_reset_slot_miss(found_current, had_previous) {
        PaintDirtyKind::Region
    } else {
        PaintDirtyKind::Node
    }
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
/// the previous visual Y in retain.
///
/// Detection only. Callers must pass the result through
/// [`node_present_moved_forces_full_clear`]: a scroll viewport layer already
/// translated sibling pixels (and pin holes dest-reset the named widget).
/// Window FullClear is for slot motion **without** that layer.
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

/// Nested island StructureChanged (Tabs panel, MessageScroller Send) must dest-reset
/// the **island**, not the ancestor overflow clip.
///
/// Gallery Forms `<TabsContent>` lives in the page `ScrollArea`. Dirtying that clip
/// punches sibling 8b ToggleGroup glyphs on PartialLoad; the next empty-damage frame
/// FullClears — one-frame light/dark text flash. MessageScroller Footer stays safe
/// because the island is still intersected with the scroller clip.
///
/// `None` = unclipped page structure (route `Show`) → caller FullClears. Empty
/// island (0×0 hidden shell) also returns `None` so a sibling hide can name the box.
pub fn clipped_island_structure_damage(
    island: [f32; 4],
    clip: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    region_island_structure_damage(clip, island, None)
}

/// Wave 49 Region Clear bounds: island overflow clip ∩ (prev ∪ now).
///
/// Callers pass the pre-reconcile box as `prev` and the post-reconcile visual as
/// `now`. Missing `now` falls back to `prev` only (same as
/// [`clipped_island_structure_damage`]).
pub fn region_island_structure_damage(
    clip: Option<[f32; 4]>,
    prev: [f32; 4],
    now: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let clip = clip?;
    let island = match now {
        Some(n) if n[2] > 0.5 && n[3] > 0.5 => union_rects(prev, n),
        _ => prev,
    };
    let d = intersect_rects(island, clip).unwrap_or(island);
    if d[2] > 0.5 && d[3] > 0.5 {
        Some(d)
    } else {
        None
    }
}

/// Name every page slot that translated so node-present can dest-reset `prev∪now`
/// instead of window-FullClearing sibling captions (Tabs height change → 8b).
///
/// `current_slots` / `previous_slots` must be the same space: visual slots from
/// [`crate::app::draw_cull::last_present_visual_slots`], not layout `NodePaint.bounds`.
/// Mixing layout-now with visual-prev treats a stable outset shadow as a translate.
/// Visual slots skip nested foreign descendant ink so hover:scale Bake on a child
/// cannot dest-reset an overflow:visible parent.
///
/// Skip on a scroll-layer Clear — every slot moves, and expanding here would abort
/// strip blit / pin holes.
pub fn expand_paint_dirties_for_translated_slots(
    dirties: &mut Vec<PaintDirty>,
    current_slots: &[(u64, [f32; 4])],
    previous_slots: &[(u64, [f32; 4])],
) {
    if previous_slots.is_empty() {
        return;
    }
    for (key, now) in current_slots {
        let Some(prev) = previous_slots
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, b)| *b)
        else {
            continue;
        };
        if !node_present_slot_moved(prev, *now) {
            continue;
        }
        let bounds = union_rects(prev, *now);
        if let Some(existing) = dirties
            .iter_mut()
            .find(|d| d.key == *key && d.kind == PaintDirtyKind::Node)
        {
            existing.bounds = union_rects(existing.bounds, bounds);
        } else {
            dirties.push(PaintDirty {
                key: *key,
                bounds,
                kind: PaintDirtyKind::Node,
            });
        }
    }
}

/// True when some page slot translated but is not a named Node dirty (dest-reset
/// cannot wipe its previous Y). Callers FullClear unless a scroll layer owns it.
pub fn unnamed_translated_page_slots(
    current_slots: &[(u64, [f32; 4])],
    previous_slots: &[(u64, [f32; 4])],
    dirties: &[PaintDirty],
) -> bool {
    if previous_slots.is_empty() {
        return false;
    }
    current_slots.iter().any(|(key, now)| {
        let Some(prev) = previous_slots
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, b)| *b)
        else {
            return false;
        };
        if !node_present_slot_moved(prev, *now) {
            return false;
        }
        !dirties
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == *key)
    })
}

/// Window FullClear when a named slot moved **and** no scroll layer owns the
/// translation.
///
/// `scroll_layer_clear` already forbids node-present (`plan_gpu_present`) and
/// either strip-blits or scissor-Clears the viewport. `mark_full_redraw` on top
/// of that wipes blit jobs and named dirties — Marker spin/shimmer then
/// FullClears the window on every scroll tick.
#[inline]
pub fn node_present_moved_forces_full_clear(dirties_moved: bool, scroll_layer_clear: bool) -> bool {
    dirties_moved && !scroll_layer_clear
}

/// True when a viewport that already existed last frame changed its offset.
///
/// Portal join (bubble `+N`) hashes a newly spawned overlay ScrollArea into the
/// scroll fingerprint. That is not page motion — treating it as scroll FullClears
/// the gallery. Conversely, skipping the scroll layer because the portal joined
/// leaves every translated page slot to dest-reset `prev∪now` (a tall strip of
/// content below the chip) and hitch.
pub fn existing_scroll_offset_changed(
    prev: &HashMap<String, [u32; 2]>,
    cur: &HashMap<String, [u32; 2]>,
) -> bool {
    cur.iter()
        .any(|(id, off)| prev.get(id).is_some_and(|p| p != off))
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

/// Absolute Latest / jump pill that shares pixels with a blit viewport.
///
/// The dock is a later sibling of the scroll viewport, not sticky. Strip blit
/// copies the overlap (messages under the pill). Overlay rails already pin every
/// scroll frame; without the same pin Latest only dest-resets on StyleChanged
/// and the next blit wipes it.
pub fn message_scroller_jump_scroll_blit_pins(
    root: &Element,
    blit_viewports: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    if blit_viewports.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        if el.attrs.get("data-slot").map(String::as_str) != Some("message-scroller-button") {
            return;
        }
        if el.attrs.get("data-active").map(String::as_str) != Some("true") {
            return;
        }
        if el.style.opacity <= 0.05 {
            return;
        }
        let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
        if overlay_needs_scroll_blit_pin(b, blit_viewports) {
            out.push(b);
        }
    });
    out
}

/// Focused Input / Textarea I-beam (`input_caret` leaf) sharing a strip-blit viewport.
///
/// Strip blit copies retain pixels that may be mid blink-OFF. A later Clear of the
/// exposed band (or a blink-OFF named pin) then leaves Gallery Kbd Search looking
/// unfocused until the next blink ON (图二 → 图三). Pin the live caret leaf every
/// scroll frame so Clear+redraw pulls chrome from this frame's collect.
pub fn focused_input_caret_scroll_blit_pins(
    root: &Element,
    blit_viewports: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    if blit_viewports.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        if el.text_style.input_caret.is_none() {
            return;
        }
        let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
        if !overlay_needs_scroll_blit_pin(b, blit_viewports) {
            return;
        }
        // Pad covers the I-beam hairline and a plain_chrome parent Hard-ring outset.
        let pad = 8.0;
        out.push([
            b[0] - pad,
            b[1] - pad,
            (b[2] + pad * 2.0).max(0.0),
            (b[3] + pad * 2.0).max(0.0),
        ]);
    });
    out
}

/// Overlay (inside) scrollbar tracks that share pixels with a blit viewport.
///
/// The rail is a sibling of the viewport, not sticky / portal overlay. Strip blit
/// copies the overlap including the previous thumb; without a pin the painted
/// thumb stays frozen while content scrolls.
pub fn overlay_scrollbar_scroll_blit_pins(
    root: &Element,
    viewport_ids: &[String],
    blit_viewports: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    if viewport_ids.is_empty() || blit_viewports.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    crate::core::tree::walk_elements(root, |el| {
        let Some(fid) = el.focus_id.as_deref() else {
            return;
        };
        let Some(rest_axis) =
            fid.strip_prefix(crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
        else {
            return;
        };
        let (rest, _) = rest_axis.rsplit_once(':').unwrap_or((rest_axis, "y"));
        if !viewport_ids.iter().any(|id| id == rest) {
            return;
        }
        let overlay = el.attrs.get("rui_scrollbar_overlay").map(String::as_str) == Some("1")
            || el.style.position == taffy::style::Position::Absolute;
        if !overlay {
            return;
        }
        let b = el.layout_bounds;
        if overlay_needs_scroll_blit_pin(b, blit_viewports) {
            out.push(b);
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
        let mut floating = Element::new();
        floating.template_id = Some("cal-month__floating_panel".into());
        assert_eq!(
            paint_kind_for_element(&floating),
            PaintDirtyKind::Overlay,
            "Select/Popover fade chrome must not dest-reset as a page Node"
        );
    }

    #[test]
    fn hoisted_overlay_descendant_hover_scale_dirties_portal_stamp() {
        // +N expand chips live under `__floating_panel` but have no chrome template_id.
        // Classifying them as page `Node` lets `page_scissor_excluding_overlay` drop
        // every hover:scale frame (fully inside occupancy, not a punch). The overlay
        // pass only encodes Overlay-kind keys, so GPU retains rest until occupancy
        // jumps — rest↔1.25 snap instead of the 150ms tween.
        let _ = take_frame_paint_nodes();
        let mut chip = Element::new().w(16.0).h(16.0);
        crate::style::apply_class_list(
            &mut chip,
            "cursor-pointer transition-transform duration-150 ease-out hover:scale-125",
        );
        let mut panel = Element::new().w(80.0).h(32.0).child(chip);
        panel.template_id = Some("rx__floating_panel".into());
        panel.set_attr("rui_portal", "1");
        panel.set_attr(crate::layout::layout_tree::ATTR_PORTAL_KEY, "rx-expand");

        let mut root = Element::new().w(400.0).h(400.0).child(panel);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);

        fn find_hover_scale_chip(el: &Element) -> Option<&Element> {
            if el
                .class_set
                .as_ref()
                .is_some_and(crate::style::ClassSet::has_pointer_hover_linear_xform)
            {
                return Some(el);
            }
            el.children.iter().find_map(find_hover_scale_chip)
        }
        fn find_hoisted(el: &Element) -> Option<&Element> {
            if el.attrs.get("rui_hoisted_portal").map(String::as_str) == Some("1") {
                return Some(el);
            }
            el.children.iter().find_map(find_hoisted)
        }
        let chip = find_hover_scale_chip(&root).expect("hover:scale chip");
        let panel = find_hoisted(&root).expect("hoisted panel");
        let stamp = crate::layout::layout_tree::overlay_portal_stamp_key(panel);

        assert_eq!(
            paint_kind_for_element(chip),
            PaintDirtyKind::Overlay,
            "overlay descendants must Overlay-dirty; page Node is scissored out of occupancy"
        );
        let chip_box = crate::core::tree::visual_paint_aabb(chip, chip.paint_scroll_acc);
        let occ = crate::core::tree::visual_paint_aabb(panel, panel.paint_scroll_acc);
        assert!(
            page_scissor_excluding_overlay(chip_box, Some(occ)).is_none(),
            "chip box is inside overlay occupancy — a page Node dirty would never present"
        );

        let _ = take_frame_paint_nodes();
        push_frame_paint_element(chip, chip.paint_scroll_acc);
        let dirties = take_frame_paint_nodes();
        assert_eq!(dirties.len(), 1, "expected one dirty, got {dirties:?}");
        assert_eq!(dirties[0].kind, PaintDirtyKind::Overlay);
        assert_eq!(
            dirties[0].key, stamp,
            "descendant Overlay dirty must use the host portal stamp so the band encodes"
        );
        assert!(
            overlay_band_should_encode(stamp, &[dirties[0].key], false, &[stamp], &[stamp]),
            "chip hover:scale must encode the host overlay band"
        );
    }

    #[test]
    fn push_frame_paint_element_without_halo_stays_on_layout_box() {
        let _ = take_frame_paint_nodes();
        let mut el = Element::new();
        el.layout_bounds = [10.0, 20.0, 100.0, 30.0];
        push_frame_paint_element(&el, [0.0, 0.0]);
        let dirties = take_frame_paint_nodes();
        assert_eq!(dirties.len(), 1, "expected one dirty node, got {dirties:?}");
        let b = dirties[0].bounds;
        assert!(
            (b[0] - 10.0).abs() < 0.01
                && (b[1] - 20.0).abs() < 0.01
                && (b[2] - 100.0).abs() < 0.01
                && (b[3] - 30.0).abs() < 0.01,
            "no-halo node must dirty the visual layout box, not vertex-AA pad, got {b:?}"
        );
    }

    #[test]
    fn push_frame_paint_skips_transparent_shown_enter_without_product_slot() {
        let _ = take_frame_paint_nodes();
        let mut el = Element::new();
        el.layout_bounds = [120.0, 340.0, 80.0, 32.0];
        el.style.opacity = 0.0;
        el.set_attr("data-active", "true");
        push_frame_paint_element(&el, [0.0, 0.0]);
        assert!(
            take_frame_paint_nodes().is_empty(),
            "shown opacity≈0 enter must not dest-reset empty fill+shadow"
        );

        el.set_attr("data-active", "false");
        push_frame_paint_element(&el, [0.0, 0.0]);
        assert_eq!(
            take_frame_paint_nodes().len(),
            1,
            "hide settle still dest-resets the last faded box"
        );
    }

    #[test]
    fn overlay_expand_clip_dirty_must_not_use_unconstrained_height() {
        let _ = take_frame_paint_nodes();
        let mut el = Element::new();
        el.template_id = Some("cal-month__floating_panel".into());
        el.layout_bounds = [100.0, 400.0, 144.0, 220.0];
        el.style.max_size.height = taffy::style::Dimension::length(40.0);
        push_frame_paint_element(&el, [0.0, 0.0]);
        let dirties = take_frame_paint_nodes();
        assert_eq!(
            dirties.len(),
            1,
            "expected one overlay dirty, got {dirties:?}"
        );
        let b = dirties[0].bounds;
        assert_eq!(dirties[0].kind, PaintDirtyKind::Overlay);
        assert!(
            b[3] < 80.0,
            "expand Overlay dirty must be the max-height clip, not pinned content height; got {b:?}"
        );
    }

    #[test]
    fn overlay_same_size_fade_is_not_restore() {
        let panel = [90.0, 140.0, 40.0, 160.0];
        assert!(
            !overlay_restore_needed(Some(panel), Some(panel)),
            "same-size occupancy is not a shrink — OverlayRestore Clear punches the panel"
        );
        assert!(!overlay_restore_needed(None, Some(panel)));
        assert!(
            overlay_partial_load_retains_page(true, false, true, true),
            "fade PartialLoad must keep page glyphon retain"
        );
    }

    #[test]
    fn overlay_same_size_fade_replace_does_not_arm_restore_clear() {
        assert!(
            overlay_fade_replace_needed(true, true, true, false, true),
            "occupancy + Overlay dirty must replace retain (shadow stack / lost exit)"
        );
        assert!(
            !overlay_fade_replace_needed(false, true, true, false, true),
            "first paint has no prior overlay pixels to replace"
        );
        assert!(
            overlay_fade_replace_needed(true, true, true, true, true),
            "shrink while mounted replaces the previous stamp — leftover-strip Clear cuts captions"
        );
        assert!(!overlay_fade_replace_needed(true, true, false, false, true));
        assert!(
            overlay_fade_replace_needed(true, true, true, false, false),
            "expand accordion CSS tick must replace the previous keyframe (src-over stacks shadow)"
        );
        assert!(
            !overlay_fade_replace_needed(true, false, true, true, true),
            "unmount restore uses overlay_restore_clear so page glyphs return in the hole"
        );
        assert!(
            overlay_partial_load_retains_page(true, false, true, true),
            "fade replace Restore must keep page glyphon retain"
        );
        assert!(
            !overlay_partial_load_retains_page(true, true, true, true),
            "unmount restore_clear still repaints page glyphs in the hole"
        );
    }

    #[test]
    fn overlay_primitive_encode_is_once_per_present() {
        assert_eq!(overlay_primitive_encode_count(0), 0);
        assert_eq!(overlay_primitive_encode_count(1), 1);
        assert_eq!(overlay_primitive_encode_count(2), 1);
        assert_eq!(overlay_primitive_encode_count(5), 1);
    }

    #[test]
    fn overlay_suffix_must_not_submit_without_physical_scissor() {
        assert!(
            !overlay_suffix_submitted(None),
            "leftover page Node scissor clips the panel to the trigger (invisible but hittable)"
        );
        assert!(overlay_suffix_submitted(Some((10, 20, 40, 160))));
    }

    #[test]
    fn page_trigger_scissor_must_not_enter_overlay_occupancy() {
        let occ = [90.0, 140.0, 40.0, 160.0];
        let trigger = [90.0, 110.0, 40.0, 32.0];
        let trigger_scissor = crate::app::draw_cull::padded_xywh(trigger, 8.0);
        assert!(
            overlay_page_punch_overlap(Some(occ), trigger_scissor).is_some(),
            "Select trigger pad overlaps the dropdown"
        );
        let clipped = page_scissor_excluding_overlay(trigger_scissor, Some(occ))
            .expect("trigger still has page chrome above the menu");
        assert!(
            intersect_rects(clipped, occ).is_none(),
            "clipped trigger scissor must not dest-reset overlay pixels"
        );
        assert!(
            !overlay_page_dirty_punches_occupancy(trigger_scissor, Some(clipped), Some(occ)),
            "a single-AABB clip is not a punch — overlay stays in retain"
        );
        assert!(
            overlay_encode_scissor(None, Some(occ), None).is_none(),
            "idle overlay stays in retain — do not src-over the halo again"
        );
        let inside = [90.0, 150.0, 40.0, 20.0];
        assert!(
            page_scissor_excluding_overlay(inside, Some(occ)).is_none(),
            "page dirty entirely in the overlay must not dest-reset as a Node"
        );
    }

    #[test]
    fn page_trigger_under_modal_scrim_skips_dest_reset() {
        // Gallery AlertDialog: the clicked trigger sits under the fullscreen scrim,
        // not under the centered panel. Hover dest-reset of that caption punches
        // through unless occupancy includes the scrim (not just the card).
        let scrim = [0.0, 0.0, 1280.0, 800.0];
        let trigger = [24.0, 80.0, 96.0, 32.0];
        let trigger_scissor = crate::app::draw_cull::padded_xywh(trigger, 8.0);
        assert!(
            overlay_page_punch_overlap(Some(scrim), trigger_scissor).is_some(),
            "clicked trigger is inside the scrim"
        );
        assert!(
            page_scissor_excluding_overlay(trigger_scissor, Some(scrim)).is_none(),
            "dest-reset of the trigger would wipe scrim pixels and redraw the caption on top"
        );
        assert!(
            page_node_present_occluded_by_overlay(trigger, Some(scrim)),
            "glyph capture/flush must skip the same trigger or the caption src-overs the scrim"
        );
        let neighbor = [200.0, 80.0, 96.0, 32.0];
        assert!(
            !page_node_present_occluded_by_overlay(neighbor, Some([90.0, 140.0, 40.0, 160.0])),
            "a dropdown occupancy that misses the trigger must still present its glyphs"
        );
    }

    #[test]
    fn l_shaped_page_scissor_keeps_largest_remainder_outside_overlay() {
        let occ = [90.0, 100.0, 80.0, 80.0];
        let corner = [80.0, 90.0, 40.0, 40.0];
        assert!(subtract_rect(corner, occ).len() > 1);
        let clipped = page_scissor_excluding_overlay(corner, Some(occ))
            .expect("corner still has page chrome outside the overlay");
        assert!(
            intersect_rects(clipped, occ).is_none(),
            "largest remainder must not dest-reset overlay pixels"
        );
        assert!(!overlay_page_dirty_punches_occupancy(
            corner,
            Some(clipped),
            Some(occ)
        ));
    }

    #[test]
    fn overlay_encode_scissor_unions_occupancy_so_fade_dirty_cannot_clip_panel() {
        let fade_document = [0.0, 8000.0, 40.0, 160.0];
        let occ = [90.0, 140.0, 40.0, 160.0];
        let scissor = overlay_encode_scissor(Some(fade_document), Some(occ), None)
            .expect("occupancy must keep the panel in the encode scissor");
        assert!(
            intersect_rects(scissor, occ).is_some(),
            "fade dirty in document space must not clip overlay to an off-screen hole"
        );
    }

    #[test]
    fn overlay_encode_scissor_clips_overflow_dirty_to_stamp() {
        // Expand pin reports content height; accordion clip is the stamp.
        let clip = [100.0, 400.0, 144.0, 40.0];
        let tall = [100.0, 400.0, 144.0, 220.0];
        let scissor = overlay_encode_scissor(Some(tall), Some(clip), None)
            .expect("clip occupancy must keep the growing menu in scissor");
        assert!(
            intersect_rects(scissor, clip).is_some(),
            "encode must still cover the clip stamp; scissor={scissor:?}"
        );
        assert!(
            scissor[3] < 120.0,
            "overflow dirty must not reopen the measured height as a transparent hole; scissor={scissor:?}"
        );
    }

    const PARENT_KEY: u64 = 1;
    const NESTED_KEY: u64 = 2;
    const PANEL_KEY: u64 = 3;
    const COLOR_KEY: u64 = 4;

    #[test]
    fn dialog_exit_panel_stamp_must_not_punch_scrim_without_redraw() {
        // AlertDialog: overlay and the centering stage are sibling portals. The
        // stage layout box is fullscreen (`inset 0` / 100%), but CSS ticks the
        // inner `{id}__modal_zoom` card which inherits the stage stamp.
        // Dest-reset of that empty host wiped the scrim; overlay_band_should_encode
        // then skipped the underlay because the dirty key is the panel stamp —
        // "遮罩瞬间关闭、稍等后弹窗也瞬间关闭".
        const OVERLAY: u64 = 0xA1;
        const STAGE: u64 = 0xA2;
        let vp = [0.0, 0.0, 800.0, 600.0];
        let card = [144.0, 120.0, 512.0, 280.0];
        let stamps = [(OVERLAY, vp), (STAGE, vp)];
        let holes = overlay_mounted_replace_holes(&stamps, &stamps, STAGE, Some(card));
        let flags =
            overlay_band_encode_flags(&[OVERLAY, STAGE], &[STAGE], false, &[OVERLAY, STAGE]);
        let punches = holes.iter().any(|h| damage_covers_full(vp, *h));
        assert!(
            !punches || flags[0],
            "panel-stamp dest-reset must not wipe the fullscreen scrim without encoding the overlay band; holes={holes:?} flags={flags:?}"
        );
        let occ = overlay_encode_occupancy(STAGE, Some(card), &stamps, Some(vp));
        assert!(
            occ.is_some_and(|b| !damage_covers_full(vp, b)),
            "zoom occupancy must be the card, not the empty fullscreen stage; occ={occ:?}"
        );
    }

    #[test]
    fn overlay_encode_scissor_must_keep_nested_menu_flipped_above_parent() {
        // DatePicker month Select places above the popover; the boxes do not overlap.
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [88.0, 40.0, 144.0, 220.0];
        assert!(intersect_rects(parent, nested).is_none());
        let stamps = [(PARENT_KEY, parent)];
        assert_eq!(
            overlay_encode_occupancy(NESTED_KEY, Some(nested), &stamps, Some(parent)),
            Some(nested),
            "flip-above dirty must not fall back to the DatePicker visual occupancy"
        );
        let nested_scissor =
            overlay_encode_scissor(Some(nested), Some(nested), None).expect("nested occupancy");
        assert!(intersect_rects(nested_scissor, nested).is_some());
        assert!(overlay_band_should_encode(
            NESTED_KEY,
            &[NESTED_KEY],
            false,
            &[PARENT_KEY, NESTED_KEY],
            &[PARENT_KEY, NESTED_KEY],
        ));
        assert!(
            !overlay_band_should_encode(
                PARENT_KEY,
                &[NESTED_KEY],
                false,
                &[PARENT_KEY, NESTED_KEY],
                &[PARENT_KEY, NESTED_KEY],
            ),
            "parent must not restack the DatePicker shadow during nested encode"
        );
        let leftover = overlay_restore_leftover_stamps(
            &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
            &[(PARENT_KEY, parent)],
        );
        assert_eq!(leftover, vec![(NESTED_KEY, nested)]);
    }

    #[test]
    fn nested_overlay_fade_replace_must_not_restore_sibling_union() {
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [88.0, 40.0, 144.0, 220.0];
        let fade = nested;
        let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
        let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade), &stamps);
        assert_eq!(restore, vec![nested]);
        assert!(
            intersect_rects(restore[0], parent).is_none(),
            "Restore of the nested menu must not dest-reset the parent popover / page between them"
        );
        let encode_occ = overlay_encode_occupancy(
            NESTED_KEY,
            Some(fade),
            &stamps,
            rects_union(&[parent, nested]),
        );
        assert_eq!(encode_occ, Some(nested));
    }

    #[test]
    fn nested_overlay_over_parent_must_not_restore_or_encode_parent() {
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [100.0, 400.0, 144.0, 220.0];
        assert!(intersect_rects(parent, nested).is_some());
        let fade = nested;
        let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
        let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade), &stamps);
        assert_eq!(restore, vec![nested]);
        assert!(
            !rect_contains_rect(restore[0], parent),
            "Restore of the month menu must not dest-reset the DatePicker popover"
        );
        let encode_occ = overlay_encode_occupancy(
            NESTED_KEY,
            Some(fade),
            &stamps,
            rects_union(&[parent, nested]),
        );
        assert_eq!(
            encode_occ,
            Some(nested),
            "sibling-union occupancy must not replace the dirty's own portal"
        );
        let encode = overlay_encode_scissor(Some(fade), encode_occ, None).expect("nested scissor");
        assert!(
            !rect_contains_rect(encode, parent),
            "encode scissor must not cover the parent halo (would restack the shadow); encode={encode:?}"
        );

        let parent_only = [(PARENT_KEY, parent)];
        let join_holes =
            overlay_mounted_replace_holes(&parent_only, &stamps, NESTED_KEY, Some(fade));
        assert!(
            join_holes.is_empty(),
            "opening a nested menu has no prev stamp — OverlayRestore would punch the parent; holes={join_holes:?}"
        );
        let mounted_holes = overlay_mounted_replace_holes(&stamps, &stamps, NESTED_KEY, Some(fade));
        assert_eq!(
            mounted_holes,
            vec![nested],
            "nested keyframe replace dest-resets the menu stamp only"
        );
        let close_holes =
            overlay_mounted_replace_holes(&stamps, &parent_only, NESTED_KEY, Some(fade));
        assert_eq!(
            close_holes,
            vec![nested],
            "closing the nested menu dest-resets the menu stamp"
        );
        assert!(overlay_restore_is_closed_portal(
            &[NESTED_KEY],
            &[PARENT_KEY]
        ));
        assert!(!overlay_restore_is_closed_portal(
            &[NESTED_KEY],
            &[PARENT_KEY, NESTED_KEY]
        ));
        assert!(overlay_band_should_encode(
            NESTED_KEY,
            &[NESTED_KEY],
            false,
            &[PARENT_KEY, NESTED_KEY],
            &[PARENT_KEY, NESTED_KEY],
        ));
        assert!(!overlay_band_should_encode(
            PARENT_KEY,
            &[NESTED_KEY],
            false,
            &[PARENT_KEY, NESTED_KEY],
            &[PARENT_KEY, NESTED_KEY],
        ));
        assert!(
            overlay_band_should_encode(
                PARENT_KEY,
                &[NESTED_KEY],
                true,
                &[PARENT_KEY],
                &[PARENT_KEY]
            ),
            "close Restore must redraw the parent calendar in the hole"
        );
        assert!(
            overlay_band_should_encode(
                PARENT_KEY,
                &[NESTED_KEY],
                false,
                &[PARENT_KEY, NESTED_KEY],
                &[PARENT_KEY],
            ),
            "nested first-open: menu instances still live in the sole DatePicker band"
        );
        assert!(
            overlay_band_should_encode(0, &[NESTED_KEY], false, &[PARENT_KEY, NESTED_KEY], &[0]),
            "synthetic whole-suffix band still draws the nested menu"
        );
        assert!(
            overlay_band_should_encode(0, &[PARENT_KEY], false, &[PARENT_KEY], &[0, NESTED_KEY],),
            "DatePicker dirty whose portal-layer band is unkeyed must still encode when a nested Select already has a keyed band"
        );
        assert!(
            overlay_band_should_encode(
                PARENT_KEY,
                &[PARENT_KEY],
                false,
                &[PARENT_KEY],
                &[PARENT_KEY, NESTED_KEY],
            ),
            "opening the calendar encodes the DatePicker band, not only the nested Select"
        );
        assert_eq!(
            overlay_band_encode_flags(
                &[PARENT_KEY, 0, NESTED_KEY, 0],
                &[NESTED_KEY],
                false,
                &[PARENT_KEY, NESTED_KEY],
            ),
            vec![false, false, true, true],
            "ColorPicker/month stacking 0-key follows the dirty host; skipped DatePicker 0-key must not submit calendar glyphs"
        );
        assert_eq!(
            overlay_band_encode_flags(&[PARENT_KEY, 0], &[PARENT_KEY], false, &[PARENT_KEY],),
            vec![true, true],
            "ColorPicker SV/hue unkeyed stacking must submit with the ColorPicker stamp"
        );
        assert_eq!(
            overlay_band_encode_flags(
                &[PARENT_KEY, 0, COLOR_KEY, 0],
                &[COLOR_KEY],
                false,
                &[COLOR_KEY],
            ),
            vec![false, false, true, true],
            "ColorPicker join dirty must encode its own stamp, not the leftover DatePicker band"
        );
        assert_eq!(
            overlay_added_portal_stamps(&parent_only, &stamps)
                .into_iter()
                .map(|(_, b)| b)
                .collect::<Vec<_>>(),
            vec![nested]
        );
        assert_eq!(
            overlay_membership_paint_stamps(&parent_only, &stamps)
                .into_iter()
                .map(|(_, b)| b)
                .collect::<Vec<_>>(),
            vec![nested],
            "join Overlay dirty is the new menu only"
        );
        assert!(
            overlay_membership_paint_stamps(&stamps, &parent_only).is_empty(),
            "leave is OverlayRestore of leftover stamps, not an Overlay dirty of the closed key"
        );
        assert_eq!(
            overlay_restore_leftover_stamps(&stamps, &parent_only)
                .into_iter()
                .map(|(_, b)| b)
                .collect::<Vec<_>>(),
            vec![nested],
            "leave Restore is the leftover hole, not the remaining DatePicker"
        );
        let color = [640.0, 120.0, 280.0, 360.0];
        let leftover_dp = [64.0, 344.0, 312.0, 352.0];
        let prev_dp = [(PARENT_KEY, leftover_dp)];
        let cur_cp = [(COLOR_KEY, color)];
        assert_eq!(
            overlay_membership_paint_stamps(&prev_dp, &cur_cp),
            vec![(COLOR_KEY, color)],
            "opening ColorPicker must Overlay-paint ColorPicker, not leftover DatePicker"
        );
        assert_eq!(
            overlay_band_encode_flags(
                &[PARENT_KEY, 0, COLOR_KEY, 0],
                &[COLOR_KEY],
                false,
                &[COLOR_KEY],
            ),
            vec![false, false, true, true],
            "leftover DatePicker band must stay skipped while ColorPicker encodes"
        );
        let leftover_occ = overlay_encode_occupancy(
            NESTED_KEY,
            Some(nested),
            &parent_only,
            Some([64.0, 344.0, 312.0, 352.0]),
        );
        assert_eq!(
            leftover_occ,
            Some(nested),
            "closed nested hole must not fall back to the parent visual occupancy"
        );
        let leftover_scissor =
            overlay_encode_scissor(Some(nested), leftover_occ, None).expect("leave scissor");
        assert!(
            !rect_contains_rect(leftover_scissor, parent),
            "leave encode must stay on the hole; restacking the parent halo jumps the shadow; scissor={leftover_scissor:?}"
        );
    }

    #[test]
    fn overlay_encode_occupancy_single_portal_covers_visual_halo() {
        let layout = [90.0, 140.0, 40.0, 160.0];
        let visual = [74.0, 124.0, 72.0, 192.0];
        let occ = overlay_encode_occupancy(
            PANEL_KEY,
            Some(layout),
            &[(PANEL_KEY, layout)],
            Some(visual),
        )
        .expect("single portal encode occupancy");
        assert!(
            occ[0] <= visual[0] + 1e-3 && occ[1] <= visual[1] + 1e-3,
            "encode occupancy must reach the dest-reset visual halo, not just the Taffy box; occ={occ:?}"
        );
        assert!(
            occ[0] + occ[2] + 1e-3 >= visual[0] + visual[2]
                && occ[1] + occ[3] + 1e-3 >= visual[1] + visual[3],
            "encode occupancy must cover visual occupancy; occ={occ:?} visual={visual:?}"
        );
    }

    #[test]
    fn overlay_encode_occupancy_must_not_union_foreign_leftover_after_tab_switch() {
        // TabsContent unmounts gal-date; overlay_occupied still holds that popover.
        // Opening ColorPicker / gal-month must not clip encode to the leftover box
        // (Inline Calendar dest-reset sliver, picker appears not to open).
        let leftover = [80.0, 360.0, 280.0, 320.0];
        let color = [640.0, 88.0, 36.0, 32.0];
        assert!(intersect_rects(leftover, color).is_none());
        let occ = overlay_encode_occupancy(
            COLOR_KEY,
            Some(color),
            &[(COLOR_KEY, color)],
            Some(leftover),
        );
        assert_eq!(
            occ,
            Some(color),
            "leftover DatePicker occupancy is not this portal's shadow halo"
        );
        let scissor = overlay_encode_scissor(Some(color), occ, None).expect("color scissor");
        assert!(intersect_rects(scissor, color).is_some());
        assert!(
            intersect_rects(scissor, leftover).is_none(),
            "ColorPicker encode must not dest-reset the previous tab's DatePicker hole"
        );
    }

    #[test]
    fn overlay_encode_scissor_must_not_clip_color_picker_popover_to_leftover_datepicker() {
        // Gallery 2b: ColorPicker is top-right; leftover DatePicker occupancy is the
        // calendar below the tabs. Expand-overflow KEEP occupancy clips the popover
        // to that leftover box — PartialLoad submits nothing where the panel is.
        let leftover = [80.0, 360.0, 280.0, 320.0];
        let popover = [640.0, 120.0, 280.0, 360.0];
        assert!(intersect_rects(leftover, popover).is_none());
        let unconstrained = union_rects(leftover, popover);
        let scissor = overlay_encode_scissor(Some(unconstrained), Some(leftover), None)
            .expect("ColorPicker encode scissor");
        assert!(
            intersect_rects(scissor, popover).is_some(),
            "ColorPicker popover must stay in the encode scissor; leftover DatePicker clip stamp is a different portal; scissor={scissor:?} popover={popover:?}"
        );
        let trigger = [640.0, 88.0, 36.0, 32.0];
        let scissor_from_trigger =
            overlay_encode_scissor(Some(popover), Some(trigger), None).expect("popover vs trigger");
        assert!(
            intersect_rects(scissor_from_trigger, popover).is_some(),
            "Fade popover dirty must not clip to the trigger-sized occupancy; scissor={scissor_from_trigger:?}"
        );
    }

    #[test]
    fn nested_overlay_wrong_space_fade_does_not_punch_sibling_on_restore() {
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [88.0, 400.0, 144.0, 220.0];
        let fade_document = [0.0, 8000.0, 40.0, 160.0];
        let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
        let restore = overlay_fade_replace_restore_boxes(NESTED_KEY, Some(fade_document), &stamps);
        assert_eq!(restore, vec![nested]);
        let occ = rects_union(&[parent, nested]);
        let encode_occ = overlay_encode_occupancy(NESTED_KEY, Some(fade_document), &stamps, occ);
        assert_eq!(
            encode_occ,
            Some(nested),
            "wrong-space fade must resolve to the nested stamp, never sibling-union occupancy"
        );
    }

    #[test]
    fn overlay_flip_restore_must_not_punch_page_gap_between_positions() {
        let below = [88.0, 400.0, 144.0, 80.0];
        let above = [88.0, 40.0, 144.0, 220.0];
        let leftover = overlay_restore_leftover_boxes(&[below], &[above]);
        assert_eq!(leftover, vec![below]);
        let gap = [88.0, 260.0, 144.0, 140.0];
        assert!(
            leftover.iter().all(|b| intersect_rects(*b, gap).is_none()),
            "union AABB of below∪above would dest-reset the page between the two positions"
        );
        assert!(overlay_same_footprint(Some(below), Some(below), 2.0));
        assert!(!overlay_same_footprint(Some(below), Some(above), 2.0));
        let holes = overlay_mounted_replace_holes(
            &[(PANEL_KEY, below)],
            &[(PANEL_KEY, above)],
            PANEL_KEY,
            Some(above),
        );
        assert_eq!(holes, vec![below, above]);
        assert!(holes.iter().all(|b| intersect_rects(*b, gap).is_none()));
    }

    #[test]
    fn overlay_grow_has_no_leftover_restore() {
        let small = [90.0, 140.0, 40.0, 40.0];
        let grown = [90.0, 140.0, 40.0, 160.0];
        assert!(overlay_restore_leftover_boxes(&[small], &[grown]).is_empty());
        assert!(!overlay_same_footprint(Some(small), Some(grown), 2.0));
        assert!(
            overlay_restore_leftover_stamps(&[(PANEL_KEY, small)], &[(PANEL_KEY, grown)])
                .is_empty()
        );
    }

    #[test]
    fn expand_measure_box_must_not_restore_against_first_clip() {
        let measure = [90.0, 140.0, 40.0, 200.0];
        let clip = [90.0, 140.0, 40.0, 40.0];
        assert!(
            !overlay_restore_leftover_boxes(&[measure], &[clip]).is_empty(),
            "document: recording the unconstrained measure box makes the next clip look like a shrink"
        );
        assert!(
            overlay_restore_leftover_boxes(&[], &[clip]).is_empty(),
            "measure frames must not enter last_overlay_boxes"
        );
        assert!(
            overlay_restore_leftover_stamps(&[], &[(PANEL_KEY, clip)]).is_empty(),
            "measure frames omit the portal key so leftover is empty"
        );
        assert!(
            !overlay_restore_needed(None, Some(clip)),
            "first painted clip is a grow from empty occupancy — Restore dest-resets a white frame"
        );
        assert!(!overlay_restore_needed(
            Some(clip),
            Some([90.0, 140.0, 40.0, 80.0])
        ));
        assert!(overlay_restore_needed(
            Some([90.0, 140.0, 40.0, 80.0]),
            Some(clip)
        ));
        assert!(
            overlay_fade_replace_needed(true, true, true, false, false),
            "expand growth still replaces the previous stamp (src-over would stack the shadow)"
        );
    }

    #[test]
    fn overlay_mounted_replace_holes_shrink_is_union_not_leftover_ring() {
        let prev = [80.0, 30.0, 164.0, 240.0];
        let cur = [90.0, 140.0, 144.0, 100.0];
        let leftover = overlay_restore_leftover_boxes(&[prev], &[cur]);
        assert!(
            leftover.len() > 1 || leftover.iter().any(|b| b[2] > 0.0 && b[3] > 0.0),
            "leftover of shrink is a strip/ring"
        );
        let holes = overlay_mounted_replace_holes(
            &[(PANEL_KEY, prev)],
            &[(PANEL_KEY, cur)],
            PANEL_KEY,
            Some(cur),
        );
        assert_eq!(holes, vec![union_rects(prev, cur)]);
        assert!(
            leftover.iter().all(|s| rect_contains_rect(holes[0], *s)),
            "filled previous visual covers leftover strips so dest-reset is not a caption-cutting ring"
        );
    }

    #[test]
    fn overlay_mounted_replace_holes_flip_does_not_punch_gap() {
        let below = [88.0, 400.0, 144.0, 80.0];
        let above = [88.0, 40.0, 144.0, 220.0];
        let holes = overlay_mounted_replace_holes(
            &[(PANEL_KEY, below)],
            &[(PANEL_KEY, above)],
            PANEL_KEY,
            Some(above),
        );
        assert_eq!(holes, vec![below, above]);
        let gap = [88.0, 260.0, 144.0, 140.0];
        assert!(
            holes.iter().all(|b| intersect_rects(*b, gap).is_none()),
            "union AABB of below∪above would dest-reset the page between the two positions"
        );
    }

    #[test]
    fn overlay_mounted_replace_holes_nested_does_not_restore_parent() {
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [88.0, 40.0, 144.0, 220.0];
        let fade = nested;
        let stamps = [(PARENT_KEY, parent), (NESTED_KEY, nested)];
        let holes = overlay_mounted_replace_holes(&stamps, &stamps, NESTED_KEY, Some(fade));
        assert_eq!(holes, vec![nested]);
        assert!(intersect_rects(holes[0], parent).is_none());
    }

    #[test]
    fn nested_expand_unstamped_must_not_dest_reset_parent_halo() {
        let parent = [80.0, 360.0, 280.0, 320.0];
        let nested = [100.0, 400.0, 144.0, 80.0];
        let parent_only = [(PARENT_KEY, parent)];
        let join =
            overlay_mounted_replace_holes(&parent_only, &parent_only, NESTED_KEY, Some(nested));
        assert!(
            join.is_empty(),
            "unstamped nested join dest-resets nothing — the expand measure AABB would punch DatePicker fill and page captions"
        );
        let unconstrained = [80.0, 360.0, 280.0, 800.0];
        assert!(overlay_mounted_replace_holes(
            &parent_only,
            &parent_only,
            NESTED_KEY,
            Some(unconstrained)
        )
        .is_empty());
        let leave =
            overlay_mounted_replace_holes(&parent_only, &parent_only, NESTED_KEY, Some(nested));
        assert!(leave.is_empty());
        let leftover = overlay_restore_leftover_stamps(
            &[(PARENT_KEY, parent), (NESTED_KEY, nested)],
            &parent_only,
        );
        assert_eq!(leftover, vec![(NESTED_KEY, nested)]);
        assert!(
            overlay_restore_leftover_stamps(&parent_only, &parent_only).is_empty(),
            "parent outset halo must not count as a leftover of the same key"
        );
    }

    #[test]
    fn overlay_mounted_replace_holes_same_size_fade() {
        let panel = [90.0, 140.0, 40.0, 160.0];
        let holes = overlay_mounted_replace_holes(
            &[(PANEL_KEY, panel)],
            &[(PANEL_KEY, panel)],
            PANEL_KEY,
            Some(panel),
        );
        assert_eq!(holes, vec![panel]);
    }

    #[test]
    fn overlay_restore_sorts_before_overlay_encode() {
        let mut nodes = vec![
            PaintDirty {
                key: 1,
                bounds: [90.0, 140.0, 40.0, 160.0],
                kind: PaintDirtyKind::Overlay,
            },
            PaintDirty {
                key: 2,
                bounds: [80.0, 130.0, 80.0, 200.0],
                kind: PaintDirtyKind::OverlayRestore,
            },
        ];
        sort_paint_dirty_nodes(&mut nodes);
        assert_eq!(nodes[0].kind, PaintDirtyKind::OverlayRestore);
        assert_eq!(nodes[1].kind, PaintDirtyKind::Overlay);
    }

    #[test]
    fn node_dirties_sort_larger_before_nested() {
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
        let mut nodes = vec![input, shell];
        sort_paint_dirty_nodes(&mut nodes);
        assert_eq!(nodes[0].key, 1, "shell wipe must precede nested Input redraw");
        assert_eq!(nodes[1].key, 2);
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
    fn layered_page_stack_dirty_must_not_skip_dest_reset_inside_viewport() {
        let viewport = PaintDirty {
            key: 1,
            bounds: [0.0, 0.0, 320.0, 384.0],
            kind: PaintDirtyKind::Node,
        };
        let latest = PaintDirty {
            key: 2,
            bounds: [120.0, 340.0, 80.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        assert!(
            paint_dirty_nested_in_larger_node(&latest, &[viewport, latest]),
            "precondition: Latest center sits in the viewport dirty"
        );
        assert!(
            !paint_dirty_skips_node_dest_reset(&latest, &[viewport, latest], true),
            "page-stack Latest / overlay scrollbar must still dest-reset; ancestor in-flow present does not recapture that band"
        );
        assert!(
            paint_dirty_skips_node_dest_reset(&latest, &[viewport, latest], false),
            "in-flow children still skip under a larger ancestor"
        );
    }

    #[test]
    fn overlay_restore_must_replay_page_fill_before_overlapping_avatar_faces() {
        // Cover-overlay close: hole is the expanded panel (larger than A/C/+N).
        // Card/page fill must paint before stacked faces, or dest-reset white /
        // a later fill wipes the overlap for a frame.
        let hole = [8.0, 40.0, 96.0, 48.0];
        let face_a = [8.0, 40.0, 32.0, 32.0];
        let face_c = [24.0, 40.0, 32.0, 32.0];
        let count = [40.0, 40.0, 32.0, 32.0];
        let page = [0.0, 0.0, 320.0, 384.0];
        assert_eq!(overlay_restore_replay_rank(page, hole), Some(0));
        let row = [8.0, 40.0, 80.0, 32.0];
        assert_eq!(
            overlay_restore_replay_rank(row, hole),
            Some(0),
            "collapsed group row is not a chip and must fill before A/B/C"
        );
        assert_eq!(overlay_restore_replay_rank(face_a, hole), Some(1));
        assert_eq!(overlay_restore_replay_rank(face_c, hole), Some(1));
        assert_eq!(overlay_restore_replay_rank(count, hole), Some(1));
        let beside = [200.0, 40.0, 32.0, 32.0];
        assert_eq!(overlay_restore_replay_rank(beside, hole), None);
    }

    #[test]
    fn overlay_dirty_is_contained_keyframe_uses_inner_card_not_empty_stage() {
        let stage = [0.0, 0.0, 800.0, 600.0];
        let card = [200.0, 120.0, 400.0, 280.0];
        assert!(
            overlay_dirty_is_contained_keyframe(card, stage),
            "zoom card inside fullscreen stage must dest-reset the card, not the host"
        );
        assert!(!overlay_dirty_is_contained_keyframe(stage, card));
        assert!(
            !overlay_dirty_is_contained_keyframe(card, card),
            "same footprint is not a contained keyframe"
        );
    }

    #[test]
    fn overlay_restore_must_skip_overlapping_avatar_node_dirties() {
        // AvatarGroup +N close: Restore hole covers A/C/+N. Each face is also a
        // Node dirty. Dest-reset of A after C punches the overlap (stacking flash).
        let restore = PaintDirty {
            key: 9,
            bounds: [8.0, 40.0, 96.0, 48.0],
            kind: PaintDirtyKind::OverlayRestore,
        };
        let face_a = PaintDirty {
            key: 1,
            bounds: [8.0, 40.0, 32.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let face_c = PaintDirty {
            key: 2,
            bounds: [24.0, 40.0, 32.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let nodes = [restore, face_a, face_c];
        assert!(
            paint_dirty_covered_by_overlay_restore(&face_a, &nodes),
            "earlier overlapped face is inside the Restore hole"
        );
        assert!(
            paint_dirty_covered_by_overlay_restore(&face_c, &nodes),
            "later overlapped face is inside the Restore hole"
        );
        assert!(
            !paint_dirty_covered_by_overlay_restore(&restore, &nodes),
            "Restore itself is not a Node skip"
        );
        let beside = PaintDirty {
            key: 3,
            bounds: [200.0, 40.0, 32.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        assert!(
            !paint_dirty_covered_by_overlay_restore(&beside, &nodes),
            "faces outside the overlay hole still dest-reset"
        );
        let page = PaintDirty {
            key: 0,
            bounds: [0.0, 0.0, 320.0, 384.0],
            kind: PaintDirtyKind::Node,
        };
        assert!(
            !paint_dirty_covered_by_overlay_restore(&page, &nodes),
            "larger page dest-reset is not swallowed by a small Restore hole"
        );

        // Wave 49/51: Region island (group AABB) also swallows per-face Node wipes.
        let region = PaintDirty {
            key: 8,
            bounds: [8.0, 40.0, 96.0, 48.0],
            kind: PaintDirtyKind::Region,
        };
        let region_nodes = [region, face_a, face_c];
        assert!(
            paint_dirty_covered_by_overlay_restore(&face_a, &region_nodes),
            "Region group AABB must skip earlier face dest-reset"
        );
        assert!(
            paint_dirty_covered_by_overlay_restore(&face_c, &region_nodes),
            "Region group AABB must skip later face dest-reset"
        );
    }

    #[test]
    fn region_island_unions_prev_and_now_inside_clip() {
        let clip = Some([0.0, 0.0, 400.0, 300.0]);
        let prev = [10.0, 100.0, 200.0, 40.0];
        let now = [10.0, 140.0, 200.0, 80.0];
        let d = region_island_structure_damage(clip, prev, Some(now)).expect("region");
        assert!(
            (d[1] - 100.0).abs() < 0.5 && (d[3] - 120.0).abs() < 0.5,
            "Region must cover prev∪now Y span, got {d:?}"
        );
        let page = region_island_structure_damage(
            Some([0.0, 0.0, 800.0, 600.0]),
            [0.0, 48.0, 752.0, 520.0],
            Some([0.0, 48.0, 752.0, 520.0]),
        )
        .expect("page-sized");
        assert!(
            region_is_page_route_tier(page, [0.0, 0.0, 800.0, 600.0]),
            "Match section_body sized region is Window tier"
        );
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
        assert!(
            rect_contains_rect(occupied, visual_panel),
            "occupancy is the instance paint AABB; dest-reset that hole, do not pad a white frame"
        );
        assert!(rect_contains_rect(occupied, chip));
        assert!(overlay_restore_needed(Some(occupied), Some(chip)));
    }

    #[test]
    fn force_full_redraw_keeps_partial_load_for_named_nodes_without_page_geom() {
        use crate::core::tree::LayoutStatus::*;
        // Typing / caret / spin: pre_chrome may still be LayoutChanged from
        // fit-relayout of a fixed Input box. Named dirties must PartialLoad.
        assert!(
            !force_full_redraw_after_chrome(LayoutChanged, StyleChanged, false, false, true, false),
            "pre_chrome LayoutChanged must not undo Input+spin PartialLoad"
        );
        assert_eq!(
            classify_present_tier(LayoutChanged, StyleChanged, false, false, true, false),
            PresentTier::Node,
            "Input+spin is PresentTier::Node"
        );
        assert!(
            !force_full_redraw_after_chrome(Clean, LayoutChanged, false, false, true, false),
            "chrome LayoutChanged with named dirties stays PartialLoad"
        );
        assert!(
            force_full_redraw_after_chrome(LayoutChanged, StyleChanged, true, false, true, false),
            "page geometry (window resize / text-fit settle) FullClears even with named spin"
        );
        assert_eq!(
            classify_present_tier(LayoutChanged, StyleChanged, true, false, true, false),
            PresentTier::Window,
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
                true,
                false,
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
        let scroll_tick = [PaintDirty {
            key: 7,
            bounds: [10.0, 12.0, 16.0, 16.0],
            kind: PaintDirtyKind::Node,
        }];
        let moved = node_present_dirties_moved(
            &scroll_tick,
            &[(7, [10.0, 12.0, 16.0, 16.0])],
            &[(7, [10.0, 10.0, 16.0, 16.0])],
        );
        assert!(
            !node_present_moved_forces_full_clear(moved, true),
            "scroll viewport layer already translated pixels; named spin must not window-Clear"
        );
        assert!(
            node_present_moved_forces_full_clear(moved, false),
            "without a scroll layer, dest-reset of now leaves previous visual Y"
        );
        assert!(
            !node_present_moved_forces_full_clear(false, true),
            "scroll layer without a moved named slot does not FullClear"
        );
        assert!(
            force_full_redraw_after_chrome(LayoutChanged, Clean, false, false, false, false),
            "nobody named a dirty box → FullClear fallback"
        );
        assert!(
            !force_full_redraw_after_chrome(StructureChanged, Clean, false, false, true, false),
            "island StructureChanged with named damage stays PartialLoad (Send must not wipe Footer)"
        );
        assert_eq!(
            classify_present_tier(StructureChanged, Clean, false, false, true, true),
            PresentTier::Region,
            "Send nested island is PresentTier::Region"
        );
        assert_eq!(
            classify_present_tier(StructureChanged, Clean, true, false, true, true),
            PresentTier::Window,
            "App:section_body page Match is PresentTier::Window"
        );
        assert!(
            force_full_redraw_after_chrome(StructureChanged, Clean, true, false, true, false),
            "page geometry StructureChanged FullClears"
        );
        assert!(
            force_full_redraw_after_chrome(StructureChanged, Clean, false, false, false, false),
            "StructureChanged with no named box → FullClear fallback"
        );
        assert!(
            !force_full_redraw_after_chrome(StructureChanged, Clean, false, true, true, false),
            "portal-only structure skip"
        );
        assert_eq!(
            classify_present_tier(StructureChanged, Clean, false, true, true, false),
            PresentTier::Region,
            "portal-only is Region, not Window"
        );
        assert_eq!(
            classify_present_tier(LayoutChanged, Clean, false, false, true, true),
            PresentTier::Region,
            "Layout + region dirty is Region, not Node"
        );
        assert!(
            !force_full_redraw_after_chrome(LayoutChanged, Clean, false, false, true, true),
            "has_region_damage must reach the helper (not hardcoded false)"
        );
        assert!(
            region_is_page_route_tier([0.0, 0.0, 900.0, 700.0], [0.0, 0.0, 1280.0, 800.0]),
            "large inset Match body is page-route Window"
        );
        assert!(
            !region_is_page_route_tier([10.0, 100.0, 320.0, 200.0], [0.0, 0.0, 1280.0, 800.0]),
            "MessageScroller Send clip stays Region"
        );
        assert_eq!(
            promote_present_tier_on_miss(PresentTier::Node, true),
            PresentTier::Region
        );
        assert_eq!(
            promote_present_tier_on_miss(PresentTier::Region, true),
            PresentTier::Window
        );
        assert_eq!(
            promote_present_tier_on_miss(PresentTier::Node, false),
            PresentTier::Node
        );
        assert!(dest_reset_slot_miss(false, true));
        assert!(!dest_reset_slot_miss(true, false));
        assert!(!dest_reset_slot_miss(true, true));
        assert!(!dest_reset_slot_miss(false, false));
        // Circumcircle growth looks like slot_moved, but must stay Node dest-reset.
        let rest = [10.0, 10.0, 16.0, 16.0];
        let spun = [4.0, 4.0, 28.0, 28.0];
        assert!(
            node_present_slot_moved(rest, spun),
            "precondition: spin grows visual AABB"
        );
        assert_eq!(
            escalate_node_dest_reset_kind(PaintDirtyKind::Node, true, true),
            PaintDirtyKind::Node,
            "spin / hover-scale AABB growth must not escalate to Region (叠影)"
        );
        assert_eq!(
            escalate_node_dest_reset_kind(PaintDirtyKind::Node, false, true),
            PaintDirtyKind::Region,
            "Taffy / last-present miss still escalates"
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
        assert!(
            restore_page_behind_overlay(page, hole),
            "overlay dest-reset AABB must be covered by the page/card fill, not left as a white frame"
        );
        assert!(restore_page_behind_overlay(bubble, hole));
        assert!(!restore_page_behind_overlay(neighbor, hole));
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
    fn portal_join_must_not_suppress_existing_page_scroll() {
        // Bubble +N hashes a new overlay ScrollArea into the fingerprint. That spawn
        // is not page motion. A page viewport that actually moved still owns the
        // layer Clear — otherwise dest-reset of prev∪now punches content below.
        let mut prev = HashMap::new();
        prev.insert("main".into(), [0u32, 100u32]);
        let mut cur_spawn = prev.clone();
        cur_spawn.insert("overlay".into(), [0, 0]);
        assert!(
            !existing_scroll_offset_changed(&prev, &cur_spawn),
            "overlay ScrollArea spawn is not page scroll"
        );
        let mut cur_scrolled = HashMap::new();
        cur_scrolled.insert("main".into(), [0, 140]);
        cur_scrolled.insert("overlay".into(), [0, 0]);
        assert!(
            existing_scroll_offset_changed(&prev, &cur_scrolled),
            "+N during scroll must still see the page viewport move"
        );
        let prev_slots = [(1u64, [10.0, 200.0, 300.0, 400.0])];
        let now_slots = [(1u64, [10.0, 120.0, 300.0, 400.0])];
        let overlay_only = [PaintDirty {
            key: 99,
            bounds: [40.0, 80.0, 48.0, 28.0],
            kind: PaintDirtyKind::Overlay,
        }];
        let unnamed = unnamed_translated_page_slots(&now_slots, &prev_slots, &overlay_only);
        assert!(
            unnamed,
            "overlay dirties do not name the translated page card"
        );
        let page_scrolled = existing_scroll_offset_changed(&prev, &cur_scrolled);
        assert!(
            !node_present_moved_forces_full_clear(unnamed, page_scrolled),
            "page scroll layer owns the translation; +N must not window-FullClear"
        );
        assert!(
            node_present_moved_forces_full_clear(unnamed, false),
            "without a scroll layer the unnamed card would FullClear — that is the flicker"
        );
        let mut dirties = Vec::new();
        expand_paint_dirties_for_translated_slots(&mut dirties, &now_slots, &prev_slots);
        assert_eq!(dirties.len(), 1);
        assert!(
            (dirties[0].bounds[3] - 480.0).abs() < 0.5,
            "dest-reset prev∪now of a scrolled 400px card is a 480px strip over content below\n bounds={:?}",
            dirties[0].bounds
        );
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
    fn chase_strip_blit_delta_must_match_glyphon_quad() {
        // Copied overlap pixels must move by the same device delta as glyphon
        // quads. A 1px disagreement is the chase-time "text jitter" (blit then
        // a later FullClear snaps back).
        use crate::app::scroll_support::{smooth_scroll_step, snap_scroll_offset_to_device_px};

        fn glyphon_quad_y(layout_y: f32, scroll_y: f32, ui_scale: f32) -> i32 {
            let top = (layout_y - scroll_y) * ui_scale;
            top.trunc() as i32
        }

        let vp = [0.0, 48.6, 200.0, 400.0];
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            let mut scroll = 0.0_f32;
            let target = snap_scroll_offset_to_device_px(80.0, scale);
            let layout_y = 120.4_f32;
            loop {
                let (next, anim) = smooth_scroll_step(scroll, target, 0.016, scale);
                let dy = next - scroll;
                if dy.abs() >= 0.5 {
                    let plan = plan_scroll_strip_blit(vp, [0.0, dy]).expect("plan");
                    let copy =
                        scroll_strip_copy_physical(&plan, scale, 800, 1200).expect("physical copy");
                    let blit = copy.src_y as i32 - copy.dst_y as i32;
                    let glyph = glyphon_quad_y(layout_y, scroll, scale)
                        - glyphon_quad_y(layout_y, next, scale);
                    assert_eq!(
                        blit, glyph,
                        "scale={scale} scroll {scroll} -> {next}: blit {blit}px vs glyph {glyph}px"
                    );
                }
                scroll = next;
                if !anim {
                    break;
                }
            }
        }
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

    #[test]
    fn overlay_scrollbar_on_blit_viewport_is_a_pin() {
        let mut root = Element::new();
        let mut track = Element::new();
        track.focus_id = Some(format!(
            "{}sa-main:y",
            crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX
        ));
        track.set_attr("rui_scrollbar_overlay", "1");
        track.layout_bounds = [392.0, 8.0, 8.0, 280.0];
        root.children.push(track);
        let vp = [0.0, 0.0, 400.0, 300.0];
        let pins = overlay_scrollbar_scroll_blit_pins(&root, &[String::from("sa-main")], &[vp]);
        assert_eq!(
            pins,
            vec![[392.0, 8.0, 8.0, 280.0]],
            "inside overlay rail sharing the blit viewport must pin"
        );
        let disjoint = overlay_scrollbar_scroll_blit_pins(&root, &[String::from("other")], &[vp]);
        assert!(
            disjoint.is_empty(),
            "rails of other viewports must not pin, got {disjoint:?}"
        );
    }

    #[test]
    fn message_scroller_jump_on_blit_viewport_is_a_pin() {
        let mut root = Element::new();
        let mut btn = Element::new();
        btn.set_attr("data-slot", "message-scroller-button");
        btn.set_attr("data-active", "true");
        btn.style.opacity = 1.0;
        btn.layout_bounds = [120.0, 340.0, 80.0, 32.0];
        root.children.push(btn);
        let vp = [0.0, 0.0, 320.0, 384.0];
        let pins = message_scroller_jump_scroll_blit_pins(&root, &[vp]);
        assert_eq!(
            pins,
            vec![[120.0, 340.0, 80.0, 32.0]],
            "active Latest sharing the blit viewport must pin every scroll frame"
        );

        let mut hidden = Element::new();
        hidden.set_attr("data-slot", "message-scroller-button");
        hidden.set_attr("data-active", "false");
        hidden.style.opacity = 0.0;
        hidden.layout_bounds = [120.0, 340.0, 80.0, 32.0];
        let mut idle = Element::new();
        idle.children.push(hidden);
        assert!(
            message_scroller_jump_scroll_blit_pins(&idle, &[vp]).is_empty(),
            "hidden Latest must not pin"
        );
    }

    #[test]
    fn focused_input_caret_on_blit_viewport_is_a_pin() {
        let mut leaf = Element::new();
        leaf.layout_bounds = [40.0, 60.0, 120.0, 24.0];
        leaf.text_style.input_caret = Some(crate::style::InputCaretDraw::default());
        let mut root = Element::new();
        root.children.push(leaf);
        let vp = [0.0, 0.0, 400.0, 320.0];
        let pins = focused_input_caret_scroll_blit_pins(&root, &[vp]);
        assert_eq!(pins.len(), 1, "focused caret leaf must pin, got {pins:?}");
        assert!(
            pins[0][0] < 40.0 && pins[0][1] < 60.0,
            "pin must pad the caret leaf AABB; got {:?}",
            pins[0]
        );

        let mut idle = Element::new();
        idle.layout_bounds = [40.0, 60.0, 120.0, 24.0];
        assert!(
            focused_input_caret_scroll_blit_pins(&idle, &[vp]).is_empty(),
            "leaf without input_caret must not pin"
        );
    }
}
