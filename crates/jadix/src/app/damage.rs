//! Per-node paint invalidation (logical pixel coordinates).
//!
//! ## Present policy
//!
//! - **Paint-only**: each dirty **node** is a separate present item. Same-key bounds
//!   may merge; **different nodes never union**. GPU Load + per-node scissor draws
//!   only that node's primitives. Hover and overlay cannot pollute each other.
//! - Swapchain always gets a **full blit** (`Clear`). Never `Load` a FLIP_DISCARD /
//!   GLES surface. `JADIX_PARTIAL_DAMAGE=0` forces surface FullClear.
//! - A **single** node covering the viewport (modal underlay) FullClears. The union
//!   of many local nodes must not.
//! - Scroll invalidates the **viewport layer** (scissor Clear), not paint nodes.
//!   A valid retain may **strip-blit** the overlap and Clear only the newly
//!   exposed band(s) plus pin holes (sticky / overlay / named paint / nested
//!   inner viewports / overlay scrollbar rails / clip-culled last-present). Nested containment blits the outer and pins the inner.
//!   Overlapping siblings blit both and pin the intersection occupancy (copy
//!   smear). `force_full_redraw`, invalid retain, `|delta| ≥ viewport`, and
//!   `JADIX_SCROLL_STRIP_BLIT=0` stay abort / layer-Clear gates. Spawn without
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
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::core::tree::{Element, LayoutStatus};

/// Present tiers: Window / Region / Node.
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

/// Portal changes rebuild the affected composite in z order. Lower-layer node
/// bounds must never carve holes into an upper layer's shadow.
pub(crate) fn portal_composite_damage(
    nodes: &[PaintDirty],
    occupied: Option<[f32; 4]>,
    viewport: [f32; 4],
) -> Option<[f32; 4]> {
    if !nodes.iter().any(|node| {
        matches!(
            node.kind,
            PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore
        ) || occupied.is_some_and(|portal| intersect_rects(node.bounds, portal).is_some())
    }) {
        return None;
    }
    nodes
        .iter()
        .filter_map(|node| {
            let bounds = intersect_rects(
                super::draw_cull::padded_xywh(node.bounds, OVERLAY_RESTORE_PAD_LOGICAL),
                viewport,
            )?;
            (bounds[2] > 0.0 && bounds[3] > 0.0).then_some(bounds)
        })
        .reduce(union_rects)
}

thread_local! {
    static FRAME_PAINT_NODES: RefCell<HashMap<super::window_scope::WindowScope, Vec<PaintDirty>>> = RefCell::new(HashMap::new());
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
        let mut windows = slot.borrow_mut();
        let slot = windows.entry(super::window_scope::active()).or_default();
        if let Some(existing) = slot.iter_mut().find(|d| d.key == key && d.kind == kind) {
            // Same-key union of two poses that do not overlap is a slot-wide
            // column. Present then `draw_scissor_fill`s that column with the
            // window clear (OTP focus on a scrolled forms page).
            if paint_poses_separated(existing.bounds, bounds) {
                slot.push(PaintDirty { key, bounds, kind });
            } else {
                existing.bounds = union_rects(existing.bounds, bounds);
            }
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
    push_frame_paint_element_change(el, scroll_acc, el.style.opacity);
}

/// A transparent result still owes an erase if this change replaced visible ink.
pub(crate) fn push_frame_paint_element_change(
    el: &Element,
    scroll_acc: [f32; 2],
    previous_opacity: f32,
) {
    push_frame_paint_element_change_from_before(el, scroll_acc, previous_opacity, None, None);
}

/// Record a paint-only change together with the visual box occupied before the
/// change.  Reactive overlays commonly move by changing `translate` while
/// retaining the same layout box; clearing only the new box leaves the old
/// pixels behind as white trails until a later full repaint.
pub(crate) fn push_frame_paint_element_change_from_before(
    el: &Element,
    scroll_acc: [f32; 2],
    previous_opacity: f32,
    previous_visual: Option<[f32; 4]>,
    overflow_clip: Option<[f32; 4]>,
) {
    // Entering at opacity≈0 has no dest-ink. Dest-reset of the empty
    // fill+shadow AABB punches underlay. Hide-settle still dest-resets
    // the last faded box (`settled_this_tick`).
    if previous_opacity < 0.01 && skip_transparent_shown_dest_reset(el) {
        return;
    }
    let current = crate::core::geometry::paint_bounds(
        el,
        clamp_overlay_clip_box(el, crate::core::tree::visual_paint_aabb(el, scroll_acc)),
    );
    let b = previous_visual
        .map(|old| union_rects(current, old))
        .unwrap_or(current);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return;
    }
    if let Some(inset) = el.paint_fill_inset {
        push_fill_dest_reset(el, b, inset);
        return;
    }
    let spinning = el.attrs.get("jadix_animate").map(|s| s.as_str()) == Some("spin")
        || el.style.animations.iter().any(|a| a.name == "spin")
        || el.style.rotate_deg.abs() > 0.01;
    // Circumcircle of the box vs AABB — previous 45° frame sits outside the axis-aligned box.
    let rot_pad = if spinning {
        (b[2].hypot(b[3]) - b[2].min(b[3])).max(0.0) * 0.5
    } else {
        0.0
    };
    let mut kind = paint_kind_for_element(el);
    let composite_scrollbar = kind == PaintDirtyKind::Region
        && crate::core::scrollbar::auto_scrollbar_live_paint_alpha(el).is_some();
    let pad =
        element_paint_outset_pad(el, rot_pad).max(if composite_scrollbar { 2.0 } else { 0.0 });
    let key = if el.attrs.get("jadix_devtools_composite").map(String::as_str) == Some("1")
        || composite_scrollbar
    {
        bounds_key(b)
    } else if kind == PaintDirtyKind::Overlay {
        el.paint_overlay_stamp
            .filter(|k| *k != 0)
            .unwrap_or_else(|| crate::layout::layout_tree::overlay_portal_stamp_key(el))
    } else {
        paint_element_key(el)
    };
    let mut bounds = [b[0] - pad, b[1] - pad, b[2] + pad * 2.0, b[3] + pad * 2.0];
    if kind == PaintDirtyKind::Node {
        if let Some(clip) = overflow_clip {
            let Some(clipped) = intersect_rects(bounds, clip) else {
                return;
            };
            if clipped != bounds {
                kind = PaintDirtyKind::Region;
            }
            bounds = clipped;
        }
    }
    push_paint_dirty(key, bounds, kind);
}

fn push_fill_dest_reset(el: &Element, bounds: [f32; 4], inset: [f32; 4]) {
    let width = (bounds[2] - inset[0] - inset[2]).max(0.0);
    let height = (bounds[3] - inset[1] - inset[3]).max(0.0);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    push_paint_dirty(
        paint_element_key(el),
        [bounds[0] + inset[0], bounds[1] + inset[1], width, height],
        PaintDirtyKind::Region,
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
        let scale = crate::core::tree::paint_scale_with_ancestor(el, el.paint_ancestor_scale);
        halo * scale[0].max(scale[1]) + rot_pad
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
    if el.paint_region_replay
        || el.attrs.get("jadix_devtools_composite").map(String::as_str) == Some("1")
    {
        return PaintDirtyKind::Region;
    }
    if el.paint_overlay_stamp.is_some_and(|k| k != 0) {
        return PaintDirtyKind::Overlay;
    }
    if el.attrs.get("jadix_hoisted_portal").map(String::as_str) == Some("1") {
        return PaintDirtyKind::Overlay;
    }
    if crate::core::scrollbar::auto_scrollbar_live_paint_alpha(el).is_some() {
        return PaintDirtyKind::Region;
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
        _ if el
            .paint_ancestor_scale
            .iter()
            .any(|scale| scale.abs() > 1e-6 && (*scale - 1.0).abs() > 1e-6) =>
        {
            PaintDirtyKind::Region
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

#[cfg(feature = "devtools")]
pub(crate) fn push_frame_region_damage(bounds: [f32; 4]) {
    push_frame_region_paint(bounds_key(bounds), bounds);
}

/// Dest-reset a frame around `inner` (Hard ring / 1px border) without wiping the
/// content hole. Same-key union would collapse four strips into one AABB and
/// flash placeholder / ⌘K (Gallery InputAdornment blur).
pub fn push_frame_halo_strips(inner: [f32; 4], pad: f32) {
    if inner[2] <= 0.5 || inner[3] <= 0.5 || pad <= 0.0 {
        return;
    }
    let outer = [
        inner[0] - pad,
        inner[1] - pad,
        inner[2] + 2.0 * pad,
        inner[3] + 2.0 * pad,
    ];
    let strips = [
        [outer[0], outer[1], outer[2], pad],
        [outer[0], inner[1] + inner[3], outer[2], pad],
        [outer[0], inner[1], pad, inner[3]],
        [inner[0] + inner[2], inner[1], pad, inner[3]],
    ];
    for s in strips {
        if s[2] > 0.5 && s[3] > 0.5 {
            push_paint_dirty(bounds_key(s), s, PaintDirtyKind::Region);
        }
    }
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
        let mut nodes = slot
            .borrow_mut()
            .remove(&super::window_scope::active())
            .unwrap_or_default();
        sort_paint_dirty_nodes(&mut nodes);
        nodes
    })
}

/// Drop queued paint nodes (page StructureChanged FullClear). Stale Taffy keys
/// from the pre-rebuild tree must not dest-reset the sidebar footer after Send.
pub fn clear_frame_paint_nodes() {
    clear_window_paint_nodes(super::window_scope::active());
}

pub(crate) fn clear_window_paint_nodes(scope: super::window_scope::WindowScope) {
    let _ = FRAME_PAINT_NODES.try_with(|slot| slot.borrow_mut().remove(&scope));
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

/// Default **on**. `JADIX_SCROLL_STRIP_BLIT=0` / `false` / `off` keeps a full viewport-layer Clear.
pub fn scroll_strip_blit_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("JADIX_SCROLL_STRIP_BLIT") {
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

/// A missing box can only require a window clear when there is no retained
/// overlay footprint to restore locally. During close/reopen animation the
/// previous footprint is the repair region; clearing the window drops the
/// trigger and page into a visible flash.
#[inline]
pub fn overlay_requires_full_clear(
    overlay_present: bool,
    overlay_box: Option<[f32; 4]>,
    had_occupancy: bool,
) -> bool {
    overlay_pixels_need_full_clear(overlay_present, overlay_box) && !had_occupancy
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

/// The last portal's GPU footprint includes shadows outside its layout stamp.
/// Once no portal remains, restore that footprint instead of leaving its halo
/// in retain. With live siblings, preserve the existing per-key restore rule.
pub(crate) fn overlay_restore_leftover_coverage(
    previous: &[(u64, [f32; 4])],
    current: &[(u64, [f32; 4])],
    previous_visual: Option<[f32; 4]>,
) -> Vec<(u64, [f32; 4])> {
    if current.is_empty() {
        if let Some(visual) = previous_visual.filter(|b| b[2] > 0.0 && b[3] > 0.0) {
            let mut coverage = vec![(0, visual)];
            coverage.extend(previous.iter().filter_map(|(key, bounds)| {
                let padded = overlay_pad_dest_reset_hole(*bounds);
                let covered = visual[0] <= padded[0]
                    && visual[1] <= padded[1]
                    && visual[0] + visual[2] >= padded[0] + padded[2]
                    && visual[1] + visual[3] >= padded[1] + padded[3];
                (!covered).then_some((*key, padded))
            }));
            return coverage;
        }
    }
    previous
        .iter()
        .flat_map(|(key, bounds)| {
            if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
                return Vec::new();
            }
            if overlay_stamp_box(current, *key) == Some(*bounds) {
                // Stamp already matches. Painted ink can still hang below the
                // max-height clamp (one portal's own visual, not a sibling union).
                // Restore only that overflow — wiping the visual itself blanks the
                // live panel when this frame has no overlay redraw.
                return single_portal_paint_overflow(
                    *key,
                    *bounds,
                    previous.len(),
                    previous_visual,
                );
            }
            // Layout stamp + the 16px encode pad stops short of the SDF halo
            // (`blur * 2 + border + 2`). Expand-up then never revisits that
            // fringe, so the calendar keeps the old menu. A recorded visual
            // that is this stamp's own halo is the pixels that were painted.
            // A single portal's visual may be the unclamped content box, far
            // past the 32px halo slack; that ink is still this portal.
            let visual = previous_visual.filter(|b| b[2] > 0.0 && b[3] > 0.0);
            let hole = match visual {
                Some(visual)
                    if overlay_visual_is_stamp_halo(*bounds, visual)
                        || (previous.len() == 1
                            && !overlay_box_extends_beyond(
                                *bounds,
                                visual,
                                overlay_stamp_slack(),
                            )) =>
                {
                    union_rects(overlay_pad_dest_reset_hole(*bounds), visual)
                }
                _ => super::draw_cull::padded_xywh(*bounds, OVERLAY_RESTORE_PAD_LOGICAL),
            };
            vec![(*key, hole)]
        })
        .collect()
}

/// Ink of a lone portal that sits outside its clamped stamp. Sibling occupancy
/// is not this portal's paint.
fn single_portal_paint_overflow(
    key: u64,
    stamp: [f32; 4],
    previous_len: usize,
    previous_visual: Option<[f32; 4]>,
) -> Vec<(u64, [f32; 4])> {
    if previous_len != 1 {
        return Vec::new();
    }
    let Some(visual) = previous_visual.filter(|b| b[2] > 0.0 && b[3] > 0.0) else {
        return Vec::new();
    };
    if overlay_box_extends_beyond(stamp, visual, overlay_stamp_slack())
        || !overlay_box_extends_beyond(visual, stamp, overlay_stamp_slack())
    {
        return Vec::new();
    }
    let live = super::draw_cull::padded_xywh(stamp, OVERLAY_RESTORE_PAD_LOGICAL);
    subtract_rect(visual, live)
        .into_iter()
        .filter(|rect| rect[2] > 0.5 && rect[3] > 0.5)
        .map(|rect| (key, rect))
        .collect()
}

/// Replay coverage includes panel chrome even when the last visible child ink
/// is smaller during an exit animation.
pub(crate) fn overlay_replay_coverage(
    stamps: &[(u64, [f32; 4])],
    visual: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    stamps.iter().fold(visual, |coverage, (_, bounds)| {
        let padded = super::draw_cull::padded_xywh(*bounds, 16.0);
        Some(coverage.map_or(padded, |b| union_rects(b, padded)))
    })
}

/// A retained overlay replay must restore every pixel in its encode scissor,
/// including translucent shadows outside the portal's layout box.
pub(crate) fn overlay_replay_restore_bounds(
    key: u64,
    dirty: [f32; 4],
    stamps: &[(u64, [f32; 4])],
    visual: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let occupancy = overlay_encode_occupancy(
        key,
        Some(dirty),
        stamps,
        if stamps.len() <= 1 { visual } else { None },
    );
    overlay_encode_scissor(Some(dirty), occupancy, None)
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

/// Page hole minus each portal's **layout** box.
///
/// The occupied halo includes the outset shadow. Subtracting that halo cuts a
/// trigger's hover fill where the shadow overlaps it, and the later portal
/// replay src-overs the uncleared fringe so the shadow stacks darker. The
/// opaque panel stays out of the clear; the fringe stays in so it can be
/// rebuilt and the shadow drawn once.
pub fn page_scissors_excluding_portal_boxes(
    scissor: [f32; 4],
    stamps: &[(u64, [f32; 4])],
) -> Vec<[f32; 4]> {
    if scissor[2] <= 0.0 || scissor[3] <= 0.0 {
        return Vec::new();
    }
    let mut parts = vec![scissor];
    for (_, box_) in stamps {
        if box_[2] <= 0.0 || box_[3] <= 0.0 {
            continue;
        }
        parts = parts
            .into_iter()
            .flat_map(|r| subtract_rect(r, *box_))
            .filter(|r| r[2] > 0.5 && r[3] > 0.5)
            .collect();
    }
    parts
}

/// Node present scissors after [`page_scissors_excluding_portal_boxes`].
///
/// Keeps the widget **layout** face (hover fill must update even when the portal
/// halo AABB overlaps the trigger) but does **not** CLEAR the translucent halo
/// ring in the gap below. Wiping that ring forced an Overlay punch-rebuild on
/// every `transition-colors` tick (~150ms): switching triggers stuttered, and
/// punching the face after glyph flush src-overed the shadow onto the caption
/// (叠粗中间态). Encode must **not** Overlay-punch after this preserve.
pub fn page_node_present_scissors_preserving_portal_halo(
    pieces: Vec<[f32; 4]>,
    widget_bounds: [f32; 4],
    coverage: Option<[f32; 4]>,
    stamps: &[(u64, [f32; 4])],
) -> Vec<[f32; 4]> {
    let Some(cov) = coverage.filter(|b| b[2] > 0.0 && b[3] > 0.0) else {
        return pieces;
    };
    let mut halo_ring = vec![cov];
    for (_, panel) in stamps {
        if panel[2] <= 0.0 || panel[3] <= 0.0 {
            continue;
        }
        halo_ring = halo_ring
            .into_iter()
            .flat_map(|r| subtract_rect(r, *panel))
            .filter(|r| r[2] > 0.5 && r[3] > 0.5)
            .collect();
    }
    if halo_ring.is_empty() {
        return pieces;
    }
    let mut out = Vec::new();
    for piece in pieces {
        let mut kept = vec![piece];
        for hole in &halo_ring {
            kept = kept
                .into_iter()
                .flat_map(|r| subtract_rect(r, *hole))
                .filter(|r| r[2] > 0.5 && r[3] > 0.5)
                .collect();
        }
        for r in kept {
            push_disjoint_rect(&mut out, r);
        }
        if let Some(core) = intersect_rects(piece, widget_bounds) {
            if core[2] > 0.5 && core[3] > 0.5 {
                push_disjoint_rect(&mut out, core);
            }
        }
    }
    out
}

/// All disjoint visible parts of a page dirty. Every part must be submitted;
/// choosing just one loses updates on the other side of an overlapping portal.
pub fn page_scissors_excluding_overlay(
    scissor: [f32; 4],
    overlay_occupied: Option<[f32; 4]>,
) -> Vec<[f32; 4]> {
    if scissor[2] <= 0.0 || scissor[3] <= 0.0 {
        return Vec::new();
    }
    match overlay_occupied.filter(|b| b[2] > 0.0 && b[3] > 0.0) {
        Some(occ) => subtract_rect(scissor, occ),
        None => vec![scissor],
    }
}

/// Add coverage without overlapping any existing rectangle (one alpha replay per pixel).
pub(crate) fn push_disjoint_rect(rects: &mut Vec<[f32; 4]>, rect: [f32; 4]) {
    let mut pieces = vec![rect];
    for existing in rects.iter() {
        pieces = pieces
            .into_iter()
            .flat_map(|r| subtract_rect(r, *existing))
            .collect();
    }
    rects.extend(pieces);
}

/// Compatibility query for a single visible part. Rendering must use
/// [`page_scissors_excluding_overlay`] to preserve every visible part.
pub fn page_scissor_excluding_overlay(
    scissor: [f32; 4],
    overlay_occupied: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    page_scissors_excluding_overlay(scissor, overlay_occupied)
        .into_iter()
        .max_by(|a, b| {
            (a[2] * a[3])
                .partial_cmp(&(b[2] * b[3]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Clicked AlertDialog trigger sits under the fullscreen scrim stamp. Skipping
/// dest-reset alone still lets node-present glyphon src-over that caption onto
/// the retained scrim while overlay suffix scissors to the panel stamp. Capture
/// and flush must use the same **layout-stamp** occupancy as Node dest-reset —
/// the visual halo covers AnchoredOverlay triggers above the panel and would
/// drop their glyphs (flicker) while the uncleared fringe stacks the shadow.
pub fn page_node_present_occluded_by_overlay(
    dirty_bounds: [f32; 4],
    portal_stamps: &[(u64, [f32; 4])],
) -> bool {
    let scissor = super::draw_cull::padded_xywh(
        dirty_bounds,
        super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Node),
    );
    page_scissors_excluding_portal_boxes(scissor, portal_stamps).is_empty()
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

/// Whether a page dirty's clear under portal coverage must Overlay-punch.
///
/// `Node` hover uses [`page_node_present_scissors_preserving_portal_halo`] and
/// must stay page-local: punching would rebuild the popover on every
/// `transition-colors` tick and stack shadow onto the trigger caption.
#[inline]
pub fn page_dirty_punches_overlay_after_clear(kind: PaintDirtyKind) -> bool {
    !matches!(kind, PaintDirtyKind::Node)
}

/// Page-node hover (`transition-colors`) must not reshape portal glyphs.
///
/// While a popover is open, every button hover frame is a partial present.
/// Preparing overlay glyphon anyway rewrites the whole panel mesh (~the hitch
/// when moving between triggers) even though those bands are not encoded.
/// Prepare only when this frame will actually draw overlay text.
#[inline]
pub fn overlay_glyphon_prepare_needed(node_present: bool, kinds: &[PaintDirtyKind]) -> bool {
    if !node_present {
        return true;
    }
    kinds.iter().any(|k| {
        matches!(
            k,
            PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region
        )
    })
}

#[inline]
pub fn node_present_skips_page_stack_glyphon(node_present: bool, region_present: bool) -> bool {
    // Node Present clears only local holes and flushes the current glyphs from
    // `capture_text_renderer` after primitive replay. Preparing the page-band
    // mesh as well submits the same trigger caption twice when its text/style
    // changed, which makes unchanged glyphs flicker during rapid toggles. Any
    // page text change outside a local Node capture promotes the frame out of
    // Node Present, so the capture is the sole text source on this path.
    node_present && !region_present
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

/// Overlay suffix dirty fed to occupancy / scissor.
///
/// Must be the raw Overlay [`PaintDirty::bounds`]. Present
/// [`super::draw_cull::node_present_scissor_logical`] already pads 16px;
/// encode pads occupancy again, and that extra ring src-overs AlertDialog
/// card chrome on footer-button hover.
pub fn overlay_suffix_encode_bounds(dirty_bounds: [f32; 4]) -> [f32; 4] {
    dirty_bounds
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

/// Expand pin reports content size while occupancy is the clip stamp.
/// Same origin — height (accordion) or width (AvatarGroup `max-width`) grew.
/// A face past that clip must not widen the encode scissor: own chrome is not
/// clipped by its own overflow, so the extra strip paints the border and the
/// overflow face, and restore only covers the stamp.
/// Foreign leftover occupancy (DatePicker vs ColorPicker) does not share origin.
fn overlay_dirty_is_expand_overflow(dirty: [f32; 4], occupancy: [f32; 4], slack: f32) -> bool {
    if !overlay_box_extends_beyond(dirty, occupancy, slack)
        || overlay_box_extends_beyond(occupancy, dirty, slack)
    {
        return false;
    }
    (dirty[0] - occupancy[0]).abs() <= slack && (dirty[1] - occupancy[1]).abs() <= slack
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
    cur.iter()
        .copied()
        .filter(|(key, bounds)| {
            bounds[2] > 0.0
                && bounds[3] > 0.0
                && overlay_stamp_box(prev, *key).is_none_or(|old| old != *bounds)
        })
        .collect()
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
                // Slack is 32px, so zoom-in-90 (~90% of the card) looks like
                // the same footprint. Encoding the rest card while opacity is
                // still 0 PAGE-wipes first-open.
                if overlay_box_extends_beyond(layout, d, 1.0) {
                    d
                } else {
                    union_rects(layout, d)
                }
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
                    // Encode scissor pads occupancy by Overlay pad (16px). Dest-reset
                    // of the unpadded card leaves elevated chrome in that pad —
                    // AlertDialog footer hover src-overs the border/shadow thicker.
                    // Only expand when a sibling is the fullscreen scrim; padding a
                    // lone popover / nested Select would punch DatePicker captions.
                    if cur_stamps
                        .iter()
                        .any(|(k, b)| *k != dirty_key && overlay_sibling_is_scrim_host(*b, hole))
                    {
                        // zoom-in-90 is same-footprint only because slack is 32px.
                        // Dest-reset of the rest card with PAGE while encode is
                        // opacity 0 looks like the dialog never opens.
                        if overlay_box_extends_beyond(hole, d, 1.0) {
                            hole = overlay_pad_dest_reset_hole(d);
                        } else {
                            hole = overlay_pad_dest_reset_hole(hole);
                        }
                    }
                } else if overlay_dirty_is_contained_keyframe(d, hole) {
                    // Chip dest-reset only. Promoting to the padded card punched
                    // the backdrop-blur ring around every AlertDialog edge on
                    // Cancel/OK hover.
                    hole = overlay_contained_replace_hole(d, hole);
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

/// Submit sibling overlay bands whose ink a dest-reset hole already wiped.
///
/// AlertDialog overlay + centering stage are both fullscreen. Overlay fade
/// dest-resets the viewport; [`overlay_band_should_encode`] then skips STAGE
/// (`dirty_has_keyed_band`) and the card vanishes instead of zooming out.
/// Button hover dest-resets a padded chip that intersects the same-footprint
/// scrim — that sibling must encode or the pad punches the dim.
pub fn overlay_band_promote_dest_reset_siblings(
    flags: &mut [bool],
    overlay_band_keys: &[u64],
    overlay_dirty_keys: &[u64],
    stamps: &[(u64, [f32; 4])],
    dest_reset_holes: &[[f32; 4]],
) {
    if dest_reset_holes.is_empty() || overlay_band_keys.is_empty() {
        return;
    }
    let slack = overlay_stamp_slack();
    let dirty_stamps: Vec<[f32; 4]> = overlay_dirty_keys
        .iter()
        .filter_map(|k| overlay_stamp_box(stamps, *k))
        .collect();
    let mut promote_unkeyed_after = false;
    for (i, &key) in overlay_band_keys.iter().enumerate() {
        if i >= flags.len() {
            break;
        }
        if key == 0 {
            if promote_unkeyed_after {
                flags[i] = true;
            }
            continue;
        }
        if flags[i] {
            promote_unkeyed_after = true;
            continue;
        }
        let Some(stamp) = overlay_stamp_box(stamps, key) else {
            promote_unkeyed_after = false;
            continue;
        };
        let punched = dest_reset_holes
            .iter()
            .any(|h| damage_covers_full(stamp, *h));
        let same_footprint_intersect = dirty_stamps.iter().any(|ds| {
            overlay_same_footprint(Some(*ds), Some(stamp), slack)
                && dest_reset_holes
                    .iter()
                    .any(|h| intersect_rects(*h, stamp).is_some())
        });
        // Runtime stage stamp is the card (empty host uses child-union), not the
        // fullscreen scrim. A padded card dest-reset spills into the dim ring —
        // encode the underlay or hover/exit leaves a bright halo / stacked shadow.
        let scrim_halo_spill = dest_reset_holes.iter().any(|h| {
            dirty_stamps.iter().any(|ds| {
                overlay_box_extends_beyond(*h, *ds, 0.5)
                    && overlay_sibling_is_scrim_host(stamp, *ds)
            })
        });
        if punched || same_footprint_intersect || scrim_halo_spill {
            flags[i] = true;
            promote_unkeyed_after = true;
        } else {
            promote_unkeyed_after = false;
        }
    }
}

/// Collapse Overlay dirties whose portal stamps share a footprint so Dialog
/// overlay + stage cannot encode twice in one present (src-over stacks chrome).
pub(crate) fn merge_overlay_replay_dirties(
    dirties: &[(u64, [f32; 4])],
    stamps: &[(u64, [f32; 4])],
) -> Vec<(Vec<u64>, [f32; 4])> {
    let mut merged: Vec<(u64, [f32; 4])> = Vec::new();
    for &(key, bounds) in dirties {
        if let Some((_, acc)) = merged.iter_mut().find(|(k, _)| *k == key) {
            *acc = union_rects(*acc, bounds);
        } else {
            merged.push((key, bounds));
        }
    }
    overlay_merge_same_footprint_dirties(&merged, stamps)
}

pub fn overlay_merge_same_footprint_dirties(
    dirties: &[(u64, [f32; 4])],
    stamps: &[(u64, [f32; 4])],
) -> Vec<(Vec<u64>, [f32; 4])> {
    let slack = overlay_stamp_slack();
    let mut groups: Vec<(Vec<u64>, [f32; 4], [f32; 4])> = Vec::new();
    for &(key, bounds) in dirties {
        if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            continue;
        }
        let stamp = overlay_stamp_box(stamps, key).unwrap_or(bounds);
        if let Some((keys, acc, _group_stamp)) = groups
            .iter_mut()
            .find(|(_, _, gs)| overlay_same_footprint(Some(*gs), Some(stamp), slack))
        {
            if !keys.contains(&key) {
                keys.push(key);
            }
            *acc = union_rects(*acc, bounds);
        } else {
            groups.push((vec![key], bounds, stamp));
        }
    }
    groups.into_iter().map(|(k, b, _)| (k, b)).collect()
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

/// Opacity≈0 with no current dest-ink. Hide-settle still dest-resets
/// (CSS transition latch, or a `@keyframes` run that just finished —
/// Dialog fade-out-0 is keyframes, not a transition).
fn skip_transparent_shown_dest_reset(el: &Element) -> bool {
    if el.style.opacity >= 0.01 {
        return false;
    }
    if el.transition_snapshot.settled_this_tick() {
        return false;
    }
    if el.animation_snapshot.runs.iter().any(|r| r.finished) {
        return false;
    }
    true
}

/// CSS dirty of a descendant (Dialog `{id}__modal_zoom` card) inside a much larger
/// portal host (fullscreen centering stage). Not the same footprint — dest-reset of
/// the host punches the sibling scrim.
pub(crate) fn overlay_dirty_is_contained_keyframe(dirty: [f32; 4], stamp: [f32; 4]) -> bool {
    let slack = overlay_stamp_slack();
    !overlay_box_extends_beyond(dirty, stamp, slack)
        && overlay_box_extends_beyond(stamp, dirty, slack)
}

/// Dest-reset hole for a contained overlay keyframe. Matches the Overlay encode
/// scissor pad so parent card chrome in that pad is wiped before redraw.
pub fn overlay_contained_replace_hole(dirty: [f32; 4], stamp: [f32; 4]) -> [f32; 4] {
    if !overlay_dirty_is_contained_keyframe(dirty, stamp) {
        return dirty;
    }
    overlay_pad_dest_reset_hole(dirty)
}

/// Same pad [`overlay_encode_scissor`] adds around occupancy.
fn overlay_pad_dest_reset_hole(hole: [f32; 4]) -> [f32; 4] {
    super::draw_cull::padded_xywh(
        hole,
        super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Overlay),
    )
}

/// Fullscreen underlay around a card-sized dest-reset (AlertDialog overlay vs stage).
/// Nested DatePicker/Select stay unmerged: parent area is not several times the hole.
fn overlay_sibling_is_scrim_host(sibling: [f32; 4], hole: [f32; 4]) -> bool {
    if hole[2] <= 0.0 || hole[3] <= 0.0 {
        return false;
    }
    if intersect_rects(sibling, hole).is_none() {
        return false;
    }
    let pad = super::draw_cull::node_present_scissor_pad(PaintDirtyKind::Overlay);
    if !overlay_box_extends_beyond(sibling, hole, pad + 1.0) {
        return false;
    }
    sibling[2] * sibling[3] > hole[2] * hole[3] * 3.0
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

/// Node dirty fully covered by an OverlayRestore / Region hole.
///
/// Cover overlays (AvatarGroup +N / Bubble +N) and Wave 49 Region islands punch
/// overlapping underlay, then Restore/Region redraws the page prefix in tree
/// order. A follow-up per-face Node dest-reset CLEARs that stacking (later face
/// wiped by an earlier sibling) for one frame.
///
/// OverlayRestore holes are encode-padded (shadow ring). That pad often fully
/// contains the AnchoredOverlay / Select trigger above the panel — especially
/// after an exclusive peer dismiss queues leftover Restore. Skipping that Node
/// leaves the hover fringe uncleared while the portal shadow src-overs darker.
/// Region islands stay unconditional; Restore only swallows Nodes that sit under
/// an opaque portal **layout** stamp (same test as Node present scissors).
#[inline]
pub fn paint_dirty_covered_by_overlay_restore(
    dirty: &PaintDirty,
    nodes: &[PaintDirty],
    portal_stamps: &[(u64, [f32; 4])],
) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    if dirty.bounds[2] <= 0.0 || dirty.bounds[3] <= 0.0 {
        return false;
    }
    let contains = |hole: [f32; 4]| {
        hole[0] <= dirty.bounds[0]
            && hole[1] <= dirty.bounds[1]
            && hole[0] + hole[2] >= dirty.bounds[0] + dirty.bounds[2]
            && hole[1] + hole[3] >= dirty.bounds[1] + dirty.bounds[3]
    };
    if nodes
        .iter()
        .any(|o| o.kind == PaintDirtyKind::Region && contains(o.bounds))
    {
        return true;
    }
    if !nodes
        .iter()
        .any(|o| o.kind == PaintDirtyKind::OverlayRestore && contains(o.bounds))
    {
        return false;
    }
    page_scissors_excluding_portal_boxes(dirty.bounds, portal_stamps).is_empty()
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
    // No chrome change is not an unnamed paint miss. Keep the retained frame
    // so the following scroll pass can copy it and repair the exposed strip.
    if !page_geometry_changed
        && pre_chrome == LayoutStatus::Clean
        && sync_impact == LayoutStatus::Clean
    {
        return false;
    }
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

/// Classify the frame present tier.
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
/// Gallery spin 叠影 regression.
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
    // Zero-area is "no ink" (caret blink OFF), not a box at the layout origin.
    // Treating `[leaf.x, leaf.y, 0, 0]` vs the I-beam as a translate made
    // `expand_paint_dirties_for_translated_slots` union the flex leaf into the
    // blink dirty — dest-reset then punched surrounding glyphs / chrome.
    if prev[2] <= SLACK || prev[3] <= SLACK || now[2] <= SLACK || now[3] <= SLACK {
        return false;
    }
    if last_present_stroke_resize(prev, now) {
        return false;
    }
    (prev[0] - now[0]).abs() > SLACK
        || (prev[1] - now[1]).abs() > SLACK
        || (prev[2] - now[2]).abs() > SLACK * 2.0
        || (prev[3] - now[3]).abs() > SLACK * 2.0
}

/// Focused Hard-ring vs unfocused 1px `input` stroke: last-present insets 1px
/// around a stable center. That is not a slot translation — expanding it
/// dest-resets the content hole (Kbd ⌘K flash). 1px scroll moves the center.
fn last_present_stroke_resize(prev: [f32; 4], now: [f32; 4]) -> bool {
    const CENTER: f32 = 0.5;
    const MAX_SIDE: f32 = 2.5;
    let pcx = prev[0] + prev[2] * 0.5;
    let pcy = prev[1] + prev[3] * 0.5;
    let ncx = now[0] + now[2] * 0.5;
    let ncy = now[1] + now[3] * 0.5;
    if (pcx - ncx).abs() > CENTER || (pcy - ncy).abs() > CENTER {
        return false;
    }
    (prev[2] - now[2]).abs() <= MAX_SIDE * 2.0 && (prev[3] - now[3]).abs() <= MAX_SIDE * 2.0
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
        // A scrolled 36px cell and its last visual pose do not overlap. The
        // AABB is a column through every row between them; clear that and the
        // window color stays (the forms-page dirty rectangle). Erase each pose.
        if paint_poses_separated(prev, *now) {
            let covers = |b: [f32; 4], pose: [f32; 4]| !paint_poses_separated(b, pose);
            let mut saw_prev = false;
            let mut saw_now = false;
            for d in dirties.iter_mut() {
                if d.key != *key || d.kind != PaintDirtyKind::Node {
                    continue;
                }
                if covers(d.bounds, prev) && covers(d.bounds, *now) {
                    d.bounds = *now;
                }
                saw_prev |= covers(d.bounds, prev);
                saw_now |= covers(d.bounds, *now);
            }
            if !saw_prev {
                dirties.push(PaintDirty {
                    key: *key,
                    bounds: prev,
                    kind: PaintDirtyKind::Node,
                });
            }
            if !saw_now {
                dirties.push(PaintDirty {
                    key: *key,
                    bounds: *now,
                    kind: PaintDirtyKind::Node,
                });
            }
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

/// Clip-culled scroll children vanish from `current_slots` but their last-present
/// pixels still sit at the live edge (Builder / On it / Delivered on
/// `1c-ms) MessageScroller`). Translate expand cannot name them.
///
/// Only keys collect actually skipped as outside overflow. Vanished last-present
/// that is *not* in that set is Taffy recycle / unmount (KbdShowcase keycaps,
/// previous-page islands) — dest-reset of those CLEARs whatever now occupies
/// the box (blank 基础 Button card) because Node capture is membership-only.
///
/// Present as **Region** so the hole recaptures live occupants (title / card),
/// not Node membership of a gone key.
pub fn expand_paint_dirties_for_clip_culled_slots(
    dirties: &mut Vec<PaintDirty>,
    current_slots: &[(u64, [f32; 4])],
    previous_slots: &[(u64, [f32; 4])],
    clip_culled_keys: &HashSet<u64>,
) {
    if previous_slots.is_empty() || clip_culled_keys.is_empty() {
        return;
    }
    for (key, prev) in previous_slots {
        if prev[2] <= 0.5 || prev[3] <= 0.5 {
            continue;
        }
        if !clip_culled_keys.contains(key) {
            continue;
        }
        if current_slots.iter().any(|(k, _)| k == key) {
            continue;
        }
        if dirties
            .iter()
            .any(|d| d.key == *key && d.kind != PaintDirtyKind::Overlay)
        {
            continue;
        }
        dirties.push(PaintDirty {
            key: *key,
            bounds: *prev,
            kind: PaintDirtyKind::Region,
        });
    }
}

/// Scroll-layer Clear cannot dest-reset (`scroll_layer_clear` skips node-present).
/// Clip-culled internals still leave last-present pixels in the copied overlap —
/// Thinking / Reading sit at the live edge on `1c-ms)` until the whole card
/// leaves and blit finally carries them out. Pin the last in-viewport box (and
/// the post-blit dest if it still hits the clip) so collect redraws occupants.
///
/// Keep distinct culled nodes separate. The final repair pass deduplicates
/// overlapping parent/child coverage before applying its bounded rectangle list.
pub fn clip_culled_scroll_blit_pins(
    previous_slots: &[(u64, [f32; 4])],
    clip_culled_keys: &HashSet<u64>,
    viewports: &[[f32; 4]],
    deltas: &[[f32; 2]],
) -> Vec<[f32; 4]> {
    if previous_slots.is_empty() || clip_culled_keys.is_empty() || viewports.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, vp) in viewports.iter().enumerate() {
        let delta = deltas.get(i).copied().unwrap_or([0.0, 0.0]);
        for (key, prev) in previous_slots {
            // Keep distant edge repairs separate; their union wipes copied pixels.
            let mut acc: Option<[f32; 4]> = None;
            if prev[2] <= 0.5 || prev[3] <= 0.5 {
                continue;
            }
            if !clip_culled_keys.contains(key) {
                continue;
            }
            if let Some(hit) = intersect_rects(*prev, *vp) {
                acc = Some(match acc {
                    Some(a) => union_rects(a, hit),
                    None => hit,
                });
            }
            if delta[0].abs() >= 0.5 || delta[1].abs() >= 0.5 {
                let dest = [prev[0] - delta[0], prev[1] - delta[1], prev[2], prev[3]];
                if let Some(hit) = intersect_rects(dest, *vp) {
                    acc = Some(match acc {
                        Some(a) => union_rects(a, hit),
                        None => hit,
                    });
                }
            }
            if let Some(r) = acc {
                if r[2] > 0.5 && r[3] > 0.5 {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// Collection can reveal a new slot or resize a clipped texture as it enters
/// the viewport. Previously clipped glyph rows may land inside the copied
/// overlap, where the exposed strip cannot restore them. Repaint changed ink;
/// ordinary translated slots keep their retained pixels. Only text/texture owners
/// need resize repair: clipping an ancestor box does not rerasterize its ink.
pub fn changed_scroll_blit_pins(
    previous_slots: &[(u64, [f32; 4])],
    current_slots: &[(u64, [f32; 4])],
    raster_owners: &HashSet<u64>,
    viewports: &[[f32; 4]],
    deltas: &[[f32; 2]],
) -> Vec<[f32; 4]> {
    let previous: HashMap<u64, [f32; 4]> = previous_slots.iter().copied().collect();
    let mut pins = Vec::new();
    for (key, bounds) in current_slots {
        let prev = previous.get(key);
        if prev.is_some_and(|p| {
            !raster_owners.contains(key)
                || ((p[2] - bounds[2]).abs() <= 0.5 && (p[3] - bounds[3]).abs() <= 0.5)
        }) {
            continue;
        }
        for (i, viewport) in viewports.iter().enumerate() {
            let current = intersect_rects(*bounds, *viewport);
            let old = prev.and_then(|p| {
                let delta = deltas.get(i).copied().unwrap_or([0.0; 2]);
                intersect_rects([p[0] - delta[0], p[1] - delta[1], p[2], p[3]], *viewport)
            });
            if let Some(pin) = match (current, old) {
                (Some(a), Some(b)) => Some(union_rects(a, b)),
                (a, b) => a.or(b),
            } {
                pins.push(pin);
            }
        }
    }
    pins
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
    // Each node has an independently clipped encode path. If all animation
    // damage is offscreen, Load preserves the complete retained frame and the
    // node loop submits nothing; clearing here redraws every visible avatar.
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

/// Whether a failed strip copy leaves the retained scroll pixels as the frame's
/// drawing surface. A full-clear frame must still render the page immediately.
pub fn should_retain_unmoved_scroll(
    use_partial_damage: bool,
    has_scroll_blit: bool,
    strip_copy_committed: bool,
) -> bool {
    use_partial_damage && has_scroll_blit && !strip_copy_committed
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
    crate::core::tree::walk_elements_pruned(root, |el| {
        // Collect already stamps this subtree summary. Unstamped callers still walk.
        if el.collect_paint_hash != 0 && !el.collect_skip_scroll_pin {
            return false;
        }
        if el.style.position_sticky {
            let b = el.layout_bounds;
            if b[2] > 0.0 && b[3] > 0.0 {
                out.push(b);
            }
        }
        true
    });
    out
}

/// Ancestor borders and rounded clips are fixed chrome in the retained composite. Their
/// edge pixels cannot ride a rectangular scroll copy with the content.
pub fn scroll_ancestor_chrome_blit_pins(
    root: &Element,
    viewport_id: &str,
    viewport: [f32; 4],
) -> Vec<[f32; 4]> {
    fn visit(el: &Element, id: &str, viewport: [f32; 4], pins: &mut Vec<[f32; 4]>) -> bool {
        let is_viewport = el.focus_id.as_deref().and_then(|f| {
            f.strip_prefix(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
        }) == Some(id);
        if !is_viewport
            && !el
                .children
                .iter()
                .any(|child| child.contains_scroll_viewport && visit(child, id, viewport, pins))
        {
            return false;
        }
        let radius = el.style.radii.into_iter().fold(0.0_f32, f32::max);
        let (clip_x, clip_y) = crate::core::tree::overflow_clip_axes(el);
        let [bt, br, bb, bl] = el.style.resolved_border_widths();
        let border_pad = |width: f32| if width > 0.0 { width + 1.0 } else { 0.0 };
        let rounded_pad = if radius > 0.0 && (clip_x || clip_y) {
            radius + 2.0
        } else {
            0.0
        };
        if rounded_pad > 0.0 || [bt, br, bb, bl].into_iter().any(|width| width > 0.0) {
            let [x, y, w, h] = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
            for edge in [
                [x, y, rounded_pad.max(border_pad(bl)).min(w), h],
                [
                    x + (w - rounded_pad.max(border_pad(br))).max(0.0),
                    y,
                    rounded_pad.max(border_pad(br)).min(w),
                    h,
                ],
                [x, y, w, rounded_pad.max(border_pad(bt)).min(h)],
                [
                    x,
                    y + (h - rounded_pad.max(border_pad(bb))).max(0.0),
                    w,
                    rounded_pad.max(border_pad(bb)).min(h),
                ],
            ] {
                if let Some(hit) = intersect_rects(edge, viewport) {
                    push_scroll_blit_pin(pins, hit);
                }
            }
        }
        true
    }
    let mut pins = Vec::new();
    visit(root, viewport_id, viewport, &mut pins);
    pins
}

/// Out-of-flow page chrome (absolute/fixed later sibling of a scroller) that
/// shares pixels with a blit viewport.
///
/// The dock is not sticky. Strip blit copies the overlap (messages under a
/// jump / toast pill). Overlay rails already pin every scroll frame; without
/// the same pin the pill only dest-resets on StyleChanged and the next blit
/// wipes it. Descendants of a **blitted** scroll viewport ride the copy;
/// `contains_scroll_viewport` is only an ancestor flag and must not mark a
/// sibling dock as in-scroll (Gallery Latest ghosts).
pub fn out_of_flow_scroll_blit_pins(
    root: &Element,
    viewport_ids: &[String],
    blit_viewports: &[[f32; 4]],
) -> Vec<[f32; 4]> {
    if blit_viewports.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk_out_of_flow_scroll_blit_pins(root, false, false, viewport_ids, blit_viewports, &mut out);
    out
}

/// Strip blit copies `src → src − delta`. Oof chrome is not in the scrolling
/// layer, so occupancy after the copy is the live box **and** the smeared box.
/// Pinning only the dock leaves "Latest" stamped into the thread.
pub fn smear_out_of_flow_after_strip_blit(
    pins: &[[f32; 4]],
    blit_viewports: &[[f32; 4]],
    blit_deltas: &[[f32; 2]],
) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    for pin in pins {
        if pin[2] <= 0.0 || pin[3] <= 0.0 {
            continue;
        }
        for (vp, delta) in blit_viewports.iter().zip(blit_deltas.iter()) {
            if !overlay_needs_scroll_blit_pin(*pin, &[*vp]) {
                continue;
            }
            if delta[0].abs() < STRIP_NEAR_ZERO && delta[1].abs() < STRIP_NEAR_ZERO {
                continue;
            }
            let smeared = [pin[0] - delta[0], pin[1] - delta[1], pin[2], pin[3]];
            if let Some(hit) = intersect_rects(smeared, *vp) {
                if hit[2] > 0.5 && hit[3] > 0.5 {
                    push_scroll_blit_pin(&mut out, hit);
                }
            }
        }
    }
    out
}

fn element_is_strip_blit_viewport(
    el: &Element,
    viewport_ids: &[String],
    blit_viewports: &[[f32; 4]],
) -> bool {
    let Some(fid) = el.focus_id.as_deref() else {
        return false;
    };
    let Some(vid) = fid.strip_prefix(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
    else {
        return false;
    };
    if !viewport_ids.is_empty() {
        return viewport_ids.iter().any(|id| id == vid);
    }
    let b = el.layout_bounds;
    blit_viewports.iter().any(|vp| {
        (b[0] - vp[0]).abs() < 1.5
            && (b[1] - vp[1]).abs() < 1.5
            && (b[2] - vp[2]).abs() < 1.5
            && (b[3] - vp[3]).abs() < 1.5
    })
}

fn is_out_of_flow_position(el: &Element) -> bool {
    el.style.position == taffy::style::Position::Absolute || el.style.position_fixed
}

fn node_has_own_dest_ink(el: &Element) -> bool {
    el.style.bg_color[3] > 0.05
        || (el.style.border_color[3] > 0.05
            && el
                .style
                .resolved_border_widths()
                .into_iter()
                .any(|w| w > 0.5))
        || el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn walk_out_of_flow_scroll_blit_pins(
    el: &Element,
    in_scroll: bool,
    under_oof: bool,
    viewport_ids: &[String],
    blit_viewports: &[[f32; 4]],
    out: &mut Vec<[f32; 4]>,
) {
    // Membership is the blitted viewport, not `contains_scroll_viewport`.
    // MessageScroller (and a Gallery page ScrollArea) contain a viewport; the
    // Latest dock is a sibling of that viewport, not scrolling content.
    let in_scroll = in_scroll || element_is_strip_blit_viewport(el, viewport_ids, blit_viewports);
    // Every descendant belongs to the copied content too. None can be an
    // external pin; stop before scanning the entire offscreen Gallery page.
    if in_scroll {
        return;
    }
    let under_oof = under_oof || is_out_of_flow_position(el);
    if under_oof
        && el.style.opacity > 0.05
        && node_has_own_dest_ink(el)
        && el.focus_id.as_deref().is_none_or(|fid| {
            !fid.starts_with(crate::core::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX)
                && !fid.starts_with(crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
        })
    {
        let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
        if overlay_needs_scroll_blit_pin(b, blit_viewports) {
            out.push(b);
        }
    }
    for child in &el.children {
        walk_out_of_flow_scroll_blit_pins(
            child,
            in_scroll,
            under_oof,
            viewport_ids,
            blit_viewports,
            out,
        );
    }
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
    crate::core::tree::walk_elements_pruned(root, |el| {
        if el.collect_paint_hash != 0 && !el.collect_skip_live {
            return false;
        }
        if el.text_style.input_caret.is_none() {
            return true;
        }
        let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
        if !overlay_needs_scroll_blit_pin(b, blit_viewports) {
            return true;
        }
        // Pad covers the I-beam hairline and a plain_chrome parent Hard-ring outset.
        let pad = 8.0;
        out.push([
            b[0] - pad,
            b[1] - pad,
            (b[2] + pad * 2.0).max(0.0),
            (b[3] + pad * 2.0).max(0.0),
        ]);
        true
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
    crate::core::tree::walk_elements_pruned(root, |el| {
        // The collect stamp includes scrollbar chrome, so idle content branches
        // cannot contain a rail. Unstamped callers still search the whole tree.
        if el.collect_paint_hash != 0 && !el.collect_skip_scroll_pin {
            return false;
        }
        let Some(fid) = el.focus_id.as_deref() else {
            return true;
        };
        let Some(rest_axis) =
            fid.strip_prefix(crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
        else {
            return true;
        };
        let (rest, _) = rest_axis.rsplit_once(':').unwrap_or((rest_axis, "y"));
        if !viewport_ids.iter().any(|id| id == rest) {
            return true;
        }
        let overlay = el.attrs.get("jadix_scrollbar_overlay").map(String::as_str) == Some("1")
            || el.style.position == taffy::style::Position::Absolute;
        if !overlay {
            return true;
        }
        let b = crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc);
        if overlay_needs_scroll_blit_pin(b, blit_viewports) {
            out.push(b);
        }
        true
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

/// Destination of the overlap copy (`src` moved to `dst_origin`).
pub fn scroll_blit_copied_dst(plan: &ScrollStripBlit) -> [f32; 4] {
    [
        plan.dst_origin[0],
        plan.dst_origin[1],
        plan.src[2],
        plan.src[3],
    ]
}

/// Single pass-scissor AABB for a strip-blit frame. `None` = no single AABB
/// (caller scissors each exposed strip and pin). Never a job viewport — that
/// AABB covers the copied overlap and lets straddling card fills wipe retain
/// (gallery page scroll: empty shells, doubled captions, dest-reset icons).
pub fn scroll_blit_pass_scissor_logical(
    jobs: &[ScrollStripBlit],
    pins: &[[f32; 4]],
) -> Option<[f32; 4]> {
    if jobs.is_empty() {
        return None;
    }
    // Pins / dual-axis / extra viewports are extra cull holes. Expanding this
    // AABB to `viewport ∪ pins` equals the scroller and wipes the copy.
    if jobs.len() > 1 || jobs.iter().any(|j| j.exposed_second.is_some()) || !pins.is_empty() {
        return None;
    }
    Some(jobs[0].exposed)
}

/// Already-physical `[x,y,w,h]` → wgpu scissor contained in the target.
pub fn physical_xywh_to_scissor(
    rect: [f32; 4],
    target_w: u32,
    target_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if target_w == 0 || target_h == 0 || rect[2] <= 0.0 || rect[3] <= 0.0 {
        return None;
    }
    let x0 = rect[0].floor().clamp(0.0, target_w as f32) as u32;
    let y0 = rect[1].floor().clamp(0.0, target_h as f32) as u32;
    let x1 = (rect[0] + rect[2]).ceil().clamp(0.0, target_w as f32) as u32;
    let y1 = (rect[1] + rect[3]).ceil().clamp(0.0, target_h as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// Integer copy matching [`plan_scroll_strip_blit`]. `None` when the rounded physical
/// delta is 0 on both axes or the copy would leave the target.
pub fn scroll_strip_copy_physical(
    plan: &ScrollStripBlit,
    scale: f32,
    target_w: u32,
    target_h: u32,
) -> Option<ScrollStripCopyPhysical> {
    // A damage scissor rounds the far edge outward to cover every dirty pixel.
    // A scroll copy must instead match the content's integer overflow clip:
    // copying the ceil row/column moves sibling separator AA into the content
    // and overwrites the fixed separator at the viewport boundary.
    let scale = scale.max(0.0);
    let clip =
        crate::renderer::prepare_text::integerize_clip_rect(plan.viewport.map(|v| v * scale))?;
    let (x, y, w, h) = damage_to_scissor_physical(clip, 0.0, 1.0, target_w, target_h)?;
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

/// Two paint poses with a real gap between them. A 1px slop still counts as
/// the same smear (sub-pixel scroll, 1px ring). A scrolled cell does not.
pub fn paint_poses_separated(a: [f32; 4], b: [f32; 4]) -> bool {
    if a[2] <= 0.5 || a[3] <= 0.5 || b[2] <= 0.5 || b[3] <= 0.5 {
        return false;
    }
    let grow = |r: [f32; 4]| [r[0] - 1.0, r[1] - 1.0, r[2] + 2.0, r[3] + 2.0];
    intersect_rects(grow(a), grow(b)).is_none()
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
#[path = "tests/damage.rs"]
mod tests;

#[cfg(test)]
include!("tests/damage_cases.rs");
