//! Cross-frame fingerprint display list: skip collect of clean subtrees.
//!
//! Analog of GPUI `Entity::cached` without Entity: paint-hash the retained tree, replay
//! last frame's ops when a sibling subtree is unchanged. Fully-visible scrolled subtrees
//! translate those ops by `GeomKey.scroll` instead of recollecting. FullClear GPU present
//! still receives a complete list (replay, not omit) — see [`crate::app::damage`].

#[cfg(test)]
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::rc::{Rc, Weak};

#[cfg(test)]
std::thread_local! {
    static STAMP_HASHED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Fast non-crypto hasher for paint fingerprints (SipHash was a collect-floor on gallery).
struct PaintHasher {
    state: u64,
}

impl PaintHasher {
    fn new() -> Self {
        Self {
            state: 0x243f_6a88_85a3_08d3,
        }
    }
}

impl Hasher for PaintHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i + 8 <= bytes.len() {
            let chunk = u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            self.write_u64(chunk);
            i += 8;
        }
        while i < bytes.len() {
            self.state = self
                .state
                .rotate_left(5)
                .wrapping_mul(0x517c_c1b7_2722_0a95)
                ^ (bytes[i] as u64);
            i += 1;
        }
    }

    fn write_u64(&mut self, i: u64) {
        self.state = self
            .state
            .rotate_left(5)
            .wrapping_mul(0x517c_c1b7_2722_0a95)
            ^ i;
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

use taffy::prelude::NodeId;

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::dimension::{CssAngle, CssDim};
use crate::layout::layout_tree::{
    PaintOp, TextLayoutInfo, ATTR_ESCAPE_OVERFLOW_CLIP, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::{
    BoxShadow, ClipCoord, ClipPath, ColorStop, FilterFunction, GpuClipPoly, Gradient, OffsetPath,
    RadialExtent, TransformFunction, TransformOrigin, TransformOriginX, TransformOriginY,
};

/// Indices into collect buffers at the start of a subtree visit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DlCursor {
    pub instances: usize,
    pub textured: usize,
    pub text: usize,
    pub chrome: usize,
    pub clip_polys: usize,
    pub paint_ops: usize,
    pub node_paints: usize,
}

impl DlCursor {
    fn zip(self, other: Self, combine: impl Fn(usize, usize) -> usize) -> Self {
        Self {
            instances: combine(self.instances, other.instances),
            textured: combine(self.textured, other.textured),
            text: combine(self.text, other.text),
            chrome: combine(self.chrome, other.chrome),
            clip_polys: combine(self.clip_polys, other.clip_polys),
            paint_ops: combine(self.paint_ops, other.paint_ops),
            node_paints: combine(self.node_paints, other.node_paints),
        }
    }

    pub fn capture(
        instances: &[InstanceInput],
        textured: &[TexturedDraw],
        text: &[TextLayoutInfo],
        chrome: &[InstanceInput],
        clip_polys: &[GpuClipPoly],
        paint_ops: &[PaintOp],
    ) -> Self {
        Self {
            instances: instances.len(),
            textured: textured.len(),
            text: text.len(),
            chrome: chrome.len(),
            clip_polys: clip_polys.len(),
            paint_ops: paint_ops.len(),
            node_paints: crate::layout::layout_tree::node_paint_cursor(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GeomKey {
    abs: [u32; 2],
    size: [u32; 2],
    scroll: [u32; 2],
    clip: [u32; 4],
    has_clip: bool,
    inherited_clip_hash: u64,
    opacity: u32,
    scale: [u32; 2],
    /// Ancestor 2×2 (`a,b,c,d`). Centered children keep the same Taffy origin when
    /// a parent rotates; skip must still bust so the VS linear is not the rest pose.
    linear: [u32; 4],
}

impl GeomKey {
    fn new(
        abs: [f32; 2],
        size: [f32; 2],
        scroll: [f32; 2],
        clip: Option<[f32; 4]>,
        opacity: f32,
        scale: [f32; 2],
        linear: &crate::style::Mat2d,
    ) -> Self {
        let (has_clip, clip_bits) = match clip {
            Some(c) => (
                true,
                [
                    c[0].to_bits(),
                    c[1].to_bits(),
                    c[2].to_bits(),
                    c[3].to_bits(),
                ],
            ),
            None => (false, [0; 4]),
        };
        Self {
            abs: [abs[0].to_bits(), abs[1].to_bits()],
            size: [size[0].to_bits(), size[1].to_bits()],
            scroll: [scroll[0].to_bits(), scroll[1].to_bits()],
            clip: clip_bits,
            has_clip,
            inherited_clip_hash: 0,
            opacity: opacity.to_bits(),
            scale: [scale[0].to_bits(), scale[1].to_bits()],
            linear: [
                linear[0].to_bits(),
                linear[1].to_bits(),
                linear[2].to_bits(),
                linear[3].to_bits(),
            ],
        }
    }
}

impl GeomKey {
    pub(crate) fn with_inherited_clip(mut self, clip: &GpuClipPoly) -> Self {
        if clip.is_active() {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for (count, fill, pts) in [
                (clip.count, clip.fill, &clip.pts),
                (clip.count2, clip.fill2, &clip.pts2),
            ] {
                count.hash(&mut h);
                fill.hash(&mut h);
                for point in pts.iter().take(count as usize) {
                    point.map(f32::to_bits).hash(&mut h);
                }
            }
            for values in [
                clip.round_rect,
                clip.round_radii,
                clip.round_rect2,
                clip.round_radii2,
            ] {
                values.map(f32::to_bits).hash(&mut h);
            }
            self.inherited_clip_hash = h.finish();
        }
        self
    }
}

impl Hash for GeomKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.abs.hash(state);
        self.size.hash(state);
        self.scroll.hash(state);
        self.has_clip.hash(state);
        self.clip.hash(state);
        self.inherited_clip_hash.hash(state);
        self.opacity.hash(state);
        self.scale.hash(state);
        self.linear.hash(state);
    }
}

#[derive(Default)]
pub(crate) struct CachedSubtree {
    paint_hash: u64,
    geom: GeomKey,
    payload: Option<Rc<CachedPayload>>,
    start: DlCursor,
    end: DlCursor,
    paint_ops: Vec<PaintOp>,
    extent: [f32; 2],
}

#[derive(Default)]
struct CachedPayload {
    base: DlCursor,
    instances: Vec<InstanceInput>,
    textured: Vec<TexturedDraw>,
    text: Vec<TextLayoutInfo>,
    chrome: Vec<InstanceInput>,
    clip_polys: Vec<GpuClipPoly>,
    node_paints: Vec<crate::layout::layout_tree::NodePaint>,
}

impl CachedPayload {
    fn inline_bytes(&self) -> usize {
        self.instances.capacity() * std::mem::size_of::<InstanceInput>()
            + self.textured.capacity() * std::mem::size_of::<TexturedDraw>()
            + self.text.capacity() * std::mem::size_of::<TextLayoutInfo>()
            + self.chrome.capacity() * std::mem::size_of::<InstanceInput>()
            + self.clip_polys.capacity() * std::mem::size_of::<GpuClipPoly>()
            + self.node_paints.capacity()
                * std::mem::size_of::<crate::layout::layout_tree::NodePaint>()
    }
}

#[derive(Default)]
pub(crate) struct CollectSkipState {
    /// When false (layout / resize / settle / full rebuild), never replay and do not clone the DL.
    /// Smooth scroll keeps this true: fully-clipped-inside subtrees translate by `GeomKey.scroll`;
    /// clean siblings (sidebar, idle cards) fingerprint-replay.
    pub allowed: bool,
    prev: HashMap<NodeId, CachedSubtree>,
    pending: Vec<NodeId>,
    spare: Option<CachedPayload>,
    snapshots: Vec<Weak<CachedPayload>>,
    captured_this_frame: bool,
    frame_bytes: usize,
    pub snapshot_items: usize,
    pub skipped: u32,
    pub patched: u32,
    /// First frame of a CSS transform tween under an idle wrapper. `reuse_idle`
    /// would keep the rest paint-hash and skip-replay the rest Bake (collapsed
    /// reaction hover:scale looks like it lost enlarge). One-shot per collect.
    pub hash_bust: bool,
    /// Roots skipped by band/clip culling. Descendants are classified through
    /// Taffy parents when repairing previous paint slots, without walking hidden content.
    pub clip_culled: HashSet<u64>,
    pub clip_culled_expanded: std::cell::OnceCell<HashSet<u64>>,
}

impl CollectSkipState {
    pub fn begin_frame(&mut self) {
        debug_assert!(self.pending.is_empty());
        self.snapshot_items = 0;
        self.captured_this_frame = false;
        self.frame_bytes = 0;
        self.skipped = 0;
        self.patched = 0;
        self.clip_culled.clear();
        self.clip_culled_expanded.take();
    }

    /// Drop fingerprint entries for a Taffy id (node recycled after unmount).
    pub fn evict(&mut self, node: NodeId) {
        self.prev.remove(&node);
    }

    /// Full Taffy rebuild may reuse `NodeId`s — drop fingerprint entries.
    pub fn invalidate(&mut self) {
        self.prev.clear();
        self.pending.clear();
        self.snapshots.clear();
        self.spare = None;
        self.skipped = 0;
        self.patched = 0;
        self.hash_bust = false;
        self.clip_culled.clear();
        self.clip_culled_expanded.take();
    }

    pub fn end_frame(&mut self) {
        debug_assert!(self.pending.is_empty());
        if !self.allowed {
            // Layout / resize / settle: absolute DLs are stale. Keeping `prev` across
            // a skip-off frame is what let maximize replay the pre-grow list.
            // Smooth scroll must not take this branch — wiping the cache every wheel
            // tick recollects the whole gallery (sidebar + idle cards + +N portals).
            self.prev.clear();
            self.snapshots.clear();
            self.spare = None;
            return;
        }
        if !self.captured_this_frame {
            return;
        }
        let mut retained_bytes = 0usize;
        self.snapshots.retain(|snapshot| {
            if let Some(payload) = snapshot.upgrade() {
                retained_bytes += payload.inline_bytes();
                true
            } else {
                false
            }
        });
        let budget = self.frame_bytes.saturating_mul(3);
        if retained_bytes > budget {
            let mut evicted = HashSet::new();
            for snapshot in &self.snapshots {
                if retained_bytes <= budget {
                    break;
                }
                if let Some(payload) = snapshot.upgrade() {
                    retained_bytes = retained_bytes.saturating_sub(payload.inline_bytes());
                    evicted.insert(Rc::as_ptr(&payload) as usize);
                }
            }
            self.prev.retain(|_, cached| {
                cached
                    .payload
                    .as_ref()
                    .is_some_and(|payload| !evicted.contains(&(Rc::as_ptr(payload) as usize)))
            });
        }
        if self
            .spare
            .as_ref()
            .is_some_and(|payload| payload.inline_bytes() > self.frame_bytes)
        {
            self.spare = None;
        }
    }

    /// A scroll-only frame can keep offscreen hit geometry when both the
    /// complete subtree fingerprint and its content-space geometry are unchanged.
    pub fn reusable_culled_extent(
        &self,
        node: NodeId,
        el: &Element,
        geom: &GeomKey,
    ) -> Option<[f32; 2]> {
        if !self.allowed || !subtree_is_skippable(el) || el.collect_skip_scroll_pin {
            return None;
        }
        let cached = self.prev.get(&node)?;
        (cached.paint_hash == el.collect_paint_hash && geom_eq_except_scroll(&cached.geom, geom))
            .then_some(cached.extent)
    }

    pub fn try_replay(
        &mut self,
        node: NodeId,
        el: &Element,
        geom: GeomKey,
        inherited_clip: &GpuClipPoly,
        instances: &mut Vec<InstanceInput>,
        textured: &mut Vec<TexturedDraw>,
        text: &mut Vec<TextLayoutInfo>,
        chrome: &mut Vec<InstanceInput>,
        clip_polys: &mut Vec<GpuClipPoly>,
        paint_ops: &mut Vec<PaintOp>,
    ) -> Option<[f32; 2]> {
        if !self.allowed || !subtree_is_skippable(el) {
            return None;
        }
        let cached = self.prev.get(&node)?;
        cached.payload.as_ref()?;
        if cached.paint_hash != el.collect_paint_hash {
            if cached.paint_hash != 0 {
                self.patched = self.patched.saturating_add(1);
            }
            return None;
        }
        if geom_eq(&cached.geom, &geom) {
            replay_cached_node_paints(
                cached, geom, [0.0; 2], instances, textured, text, chrome, paint_ops,
            );
            splice_cached(
                cached, instances, textured, text, chrome, clip_polys, paint_ops,
            );
            self.skipped = self.skipped.saturating_add(1);
            return Some(cached.extent);
        }
        let Some(delta) = scroll_translate_delta(&cached.geom, &geom) else {
            if cached.paint_hash != 0 {
                self.patched = self.patched.saturating_add(1);
            }
            return None;
        };
        if el.collect_skip_scroll_pin {
            self.patched = self.patched.saturating_add(1);
            return None;
        }
        // Overflow/clip containers cull descendants; their cached DL is incomplete.
        if node_clips_descendants(el) {
            self.patched = self.patched.saturating_add(1);
            return None;
        }
        // Incomplete DLs: clip-cull dropped descendants while this box straddled / sat
        // outside the viewport. Translating that slice would omit rows that just entered.
        if !subtree_fully_inside_clip(&cached.geom, el) || !subtree_fully_inside_clip(&geom, el) {
            self.patched = self.patched.saturating_add(1);
            return None;
        }
        replay_cached_node_paints(
            cached, geom, delta, instances, textured, text, chrome, paint_ops,
        );
        splice_cached_translated(
            cached,
            delta,
            geom,
            inherited_clip,
            instances,
            textured,
            text,
            chrome,
            clip_polys,
            paint_ops,
        );
        self.skipped = self.skipped.saturating_add(1);
        Some(cached.extent)
    }

    pub fn store(
        &mut self,
        node: NodeId,
        el: &Element,
        geom: GeomKey,
        start: DlCursor,
        instances: &[InstanceInput],
        textured: &[TexturedDraw],
        text: &[TextLayoutInfo],
        chrome: &[InstanceInput],
        clip_polys: &[GpuClipPoly],
        paint_ops: &[PaintOp],
        extent: [f32; 2],
    ) {
        if !self.allowed {
            return;
        }
        if !subtree_is_skippable(el) {
            return;
        }
        let cached = self.prev.entry(node).or_default();
        if let Some(payload) = cached
            .payload
            .take()
            .and_then(|payload| Rc::try_unwrap(payload).ok())
        {
            self.spare = Some(payload);
        }
        cached.start = start;
        cached.end = DlCursor::capture(instances, textured, text, chrome, clip_polys, paint_ops);
        cached.paint_hash = el.collect_paint_hash;
        cached.geom = geom;
        cached.extent = extent;
        remap_ops(
            paint_ops.get(start.paint_ops..).unwrap_or(&[]),
            start.instances as u32,
            start.textured as u32,
            &mut cached.paint_ops,
        );
        self.pending.push(node);
    }

    pub fn flush_snapshots(
        &mut self,
        instances: &[InstanceInput],
        textured: &[TexturedDraw],
        text: &[TextLayoutInfo],
        chrome: &[InstanceInput],
        clip_polys: &[GpuClipPoly],
    ) {
        self.frame_bytes = self.frame_bytes.max(
            std::mem::size_of_val(instances)
                + std::mem::size_of_val(textured)
                + std::mem::size_of_val(text)
                + std::mem::size_of_val(chrome)
                + std::mem::size_of_val(clip_polys)
                + crate::layout::layout_tree::node_paint_cursor()
                    * std::mem::size_of::<crate::layout::layout_tree::NodePaint>(),
        );
        if self.pending.is_empty() {
            return;
        }
        self.captured_this_frame = true;
        let first = &self.prev[&self.pending[0]];
        let mut base = first.start.zip(first.end, usize::min);
        let mut end = first.end;
        for node in &self.pending[1..] {
            let cached = &self.prev[node];
            base = base.zip(cached.start.zip(cached.end, usize::min), usize::min);
            end = end.zip(cached.end, usize::max);
        }
        let mut payload = self.spare.take().unwrap_or_default();
        instances[base.instances..end.instances].clone_into(&mut payload.instances);
        textured[base.textured..end.textured].clone_into(&mut payload.textured);
        text[base.text..end.text].clone_into(&mut payload.text);
        chrome[base.chrome..end.chrome].clone_into(&mut payload.chrome);
        clip_polys[base.clip_polys..end.clip_polys].clone_into(&mut payload.clip_polys);
        crate::layout::layout_tree::cached_node_paints(base.node_paints, &mut payload.node_paints);
        payload
            .node_paints
            .truncate(end.node_paints - base.node_paints);
        payload.base = base;
        self.frame_bytes = self.frame_bytes.max(payload.inline_bytes());
        self.snapshot_items += payload.instances.len()
            + payload.textured.len()
            + payload.text.len()
            + payload.chrome.len()
            + payload.clip_polys.len()
            + payload.node_paints.len();
        let payload = Rc::new(payload);
        self.snapshots.push(Rc::downgrade(&payload));
        for node in self.pending.drain(..) {
            self.prev.get_mut(&node).unwrap().payload = Some(payload.clone());
        }
    }
}

fn geom_eq(a: &GeomKey, b: &GeomKey) -> bool {
    geom_eq_except_scroll(a, b) && a.scroll == b.scroll
}

fn geom_eq_except_scroll(a: &GeomKey, b: &GeomKey) -> bool {
    a.abs == b.abs
        && a.size == b.size
        && a.has_clip == b.has_clip
        && a.clip == b.clip
        && a.inherited_clip_hash == b.inherited_clip_hash
        && a.opacity == b.opacity
        && a.scale == b.scale
        && a.linear == b.linear
}

/// `render = abs - scroll`. Content moves by `old_scroll - new_scroll`.
fn scroll_translate_delta(old: &GeomKey, new: &GeomKey) -> Option<[f32; 2]> {
    if !geom_eq_except_scroll(old, new) {
        return None;
    }
    if old.scroll == new.scroll {
        return None;
    }
    Some([
        f32::from_bits(old.scroll[0]) - f32::from_bits(new.scroll[0]),
        f32::from_bits(old.scroll[1]) - f32::from_bits(new.scroll[1]),
    ])
}

fn geom_paint_rect(g: &GeomKey) -> [f32; 4] {
    [
        f32::from_bits(g.abs[0]) - f32::from_bits(g.scroll[0]),
        f32::from_bits(g.abs[1]) - f32::from_bits(g.scroll[1]),
        f32::from_bits(g.size[0]),
        f32::from_bits(g.size[1]),
    ]
}

fn geom_clip_rect(g: &GeomKey) -> Option<[f32; 4]> {
    if !g.has_clip {
        return None;
    }
    Some([
        f32::from_bits(g.clip[0]),
        f32::from_bits(g.clip[1]),
        f32::from_bits(g.clip[2]),
        f32::from_bits(g.clip[3]),
    ])
}

fn subtree_fully_inside_clip(g: &GeomKey, _el: &Element) -> bool {
    let Some(clip) = geom_clip_rect(g) else {
        return true;
    };
    // Layout box, no shadow pad: a flush-bottom row must still translate-replay.
    // Partial overlap is incomplete (clip-cull dropped descendants).
    const EPS: f32 = 0.5;
    let r = geom_paint_rect(g);
    r[0] + EPS >= clip[0]
        && r[1] + EPS >= clip[1]
        && r[0] + r[2] <= clip[0] + clip[2] + EPS
        && r[1] + r[3] <= clip[1] + clip[3] + EPS
}

/// Fingerprint of a subtree's **paint** geometry.
///
/// `abs` is the paint origin (`parent abs_offset + taffy.location`), not the
/// parent origin alone — a flex Footer keeps the same size while `location.y`
/// moves on maximize.
pub(crate) fn geom_key(
    abs: [f32; 2],
    size: [f32; 2],
    scroll: [f32; 2],
    clip: Option<[f32; 4]>,
    opacity: f32,
    scale: [f32; 2],
    linear: crate::style::Mat2d,
) -> GeomKey {
    GeomKey::new(abs, size, scroll, clip, opacity, scale, &linear)
}

/// Collect walk has side channels (portal `overlay_split`, in-page
/// `page_stack_bands`, drag overlay) that are not stored in the fingerprint DL.
/// Skipping an **ancestor** splices rects/glyphs but never records those cursors
/// — present then draws all rects before all text (glyph bleed through the top
/// card / empty overlay suffix).
fn node_has_collect_side_channel(el: &Element) -> bool {
    // A replayed display list does not contain scroll metrics. Keep viewport
    // owners live so the next wheel frame can retain/copy their previous clip.
    el.focus_id.as_deref().is_some_and(|id| {
        id.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
    }) || el.template_id.as_deref() == Some(PORTAL_LAYER_ID)
        || el.template_id.as_deref() == Some("__jadix_devtools_highlight")
        || el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
        || el.attrs.get("jadix_portal").map(String::as_str) == Some("1")
        || el
            .style
            .filter
            .as_ref()
            .is_some_and(|filters| !filters.is_empty())
        || crate::utils::drag::runtime::keeps_collect_live(el)
        || crate::utils::anchored_overlay::point_overlay_drag_collect_live(el)
        || node_records_page_stack_bands(el)
}

/// Same opt-in as [`crate::layout::layout_tree`] `collect_begin_walk`: outermost
/// `data-stacking-context=1` or any direct child `z-index != 0` records
/// `page_stack_bands`, including nested z-index contexts.
/// Stamping those parents live still lets clean **siblings** fingerprint-replay.
fn node_records_page_stack_bands(el: &Element) -> bool {
    if el.attrs.get("data-stacking-context").map(String::as_str) == Some("1") {
        return true;
    }
    el.children.iter().any(|c| {
        c.style.z_index != 0 && !crate::layout::layout_tree::z_index_is_paint_order_only(c)
    })
}

/// This node opens a later page-stack band (`layout_tree` `child_opens_page_stack_band`).
/// Skipping it records one parent NodePaint and drops descendant slots that
/// dest-reset / scroll-blit pins key by (absolute jump chrome).
fn node_opens_stacking_band(el: &Element) -> bool {
    if crate::layout::layout_tree::z_index_is_paint_order_only(el) {
        return false;
    }
    el.style.z_index != 0
        || el.style.position == taffy::style::Position::Absolute
        || el.style.position_fixed
        || el.attrs.get("data-stacking-context").map(String::as_str) == Some("1")
}

pub(crate) fn subtree_is_skippable(el: &Element) -> bool {
    // Anim / caret only. Portal side-channels use `collect_skip_side_channel` so a
    // page StyleChanged that zeros a shared ancestor (inline ColorPicker hue↔alpha)
    // can still stamp-reuse neighbors — but collect must not skip-replay a portal
    // host (overlay cursors are outside the fingerprint DL).
    if el.collect_skip_live {
        return false;
    }
    if el.collect_skip_side_channel {
        return false;
    }
    // Wave 52: hash surgically zeroed by reconcile / Window FullClear — must
    // recollect. (Fresh nodes also start at 0; first stamp fills the cache.)
    if el.collect_paint_hash == 0 {
        return false;
    }
    if el.collect_skip_has_scroll {
        return false;
    }
    if node_has_collect_side_channel(el) {
        return false;
    }
    if node_opens_stacking_band(el) {
        return false;
    }
    // Overlay thumbs are relative leaves. Skip-replay uses Taffy leftover
    // (min cap at the unscrolled top) and ignores `thumb_geometry_override`
    // / live cubic — frozen / invisible rail while the list scrolls.
    if node_is_scrollbar_chrome(el) {
        return false;
    }
    true
}

fn node_is_scrollbar_chrome(el: &Element) -> bool {
    el.focus_id.as_deref().is_some_and(|fid| {
        fid.starts_with(crate::core::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX)
            || fid.starts_with(crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
    })
}

/// Out-of-flow page chrome (later sibling of a scroller, not sticky / portal).
/// `reuse_idle` otherwise skip-replays the last opacity collect after wheel
/// leaves the live edge — a jump / toast pill stays invisible until hover.
fn node_is_out_of_flow_page_chrome(el: &Element) -> bool {
    (el.style.position == taffy::style::Position::Absolute || el.style.position_fixed)
        && !node_is_scrollbar_chrome(el)
}

fn node_clips_descendants(el: &Element) -> bool {
    !matches!(el.style.overflow.x, taffy::style::Overflow::Visible)
        || !matches!(el.style.overflow.y, taffy::style::Overflow::Visible)
}

fn node_collect_is_live(el: &Element) -> bool {
    if crate::core::tree::opacity_groups_enabled()
        && el
            .attrs
            .get(crate::core::tree::ATTR_OPACITY_GROUP)
            .map(String::as_str)
            == Some("1")
        && el.style.opacity > 0.0
        && el.style.opacity < 1.0
    {
        return true;
    }
    if crate::primitives::canvas::canvas_paint_is_dirty(&el.attrs) {
        return true;
    }
    // Caret on/off is a global clock, not a hash field. Skip would freeze the
    // last hairline (solid or hidden) until the next typed character.
    if el.text_style.input_caret.is_some() {
        return true;
    }
    node_paint_needs_frame(el)
}

/// Local continuously sampled paint, excluding descendants and timer-driven carets.
pub(crate) fn node_paint_needs_frame(el: &Element) -> bool {
    if el
        .liquid_flow
        .as_ref()
        .is_some_and(crate::style::liquid_flow::LiquidFlow::is_active)
    {
        return true;
    }
    if el
        .layout_motion
        .as_ref()
        .is_some_and(crate::motion::LayoutMotion::is_active)
    {
        return true;
    }
    if el.attrs.get("jadix_animate").is_some_and(|v| {
        matches!(
            v.as_str(),
            "spin" | "ping" | "pulse" | "bounce" | "tw-shimmer"
        )
    }) {
        return true;
    }
    if el
        .attrs
        .get(crate::renderer::ATTR_SHIMMER_BOX)
        .map(String::as_str)
        == Some("1")
    {
        return true;
    }
    if el
        .attrs
        .get(crate::renderer::ATTR_GLYPH_FX)
        .map(String::as_str)
        == Some("shimmer")
    {
        return true;
    }
    if el.icon_playback.is_some() {
        return true;
    }
    if el
        .attrs
        .contains_key(crate::utils::icon_provider::ATTR_ICON_HOVER_ANIMATION)
    {
        return true;
    }
    #[cfg(all(feature = "svg", feature = "animation-path"))]
    if el.attrs.contains_key(crate::utils::ATTR_MORPH_ICON) {
        return true;
    }
    if crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot) {
        return true;
    }
    if el
        .attrs
        .contains_key(crate::primitives::shader::ATTR_SHADER)
    {
        return true;
    }
    el.transition_snapshot.has_active_tweens()
}

/// Zero paint hashes from `root` down `path` so ViewportsOnly `reuse_idle` will
/// rehash that spine (and only that spine) instead of skip-replaying idle wrappers.
///
/// Bind `StyleChanged` (gallery Item `variant={sig}`) writes new chrome on a node
/// that is not `:hover`. Hovering the clicked button keeps `scroll_only` + idle
/// ancestors, so `stamp_subtree_paint_hashes(reuse_idle=true)` would return the
/// previous hash without walking the Item. Pointer leave hash-busts the whole
/// tree — this is the surgical equivalent for in-place binds.
pub(crate) fn invalidate_collect_paint_hash_path(root: &mut Element, path: &[usize]) {
    let mut el = root;
    el.collect_paint_hash = 0;
    for &index in path {
        if index >= el.children.len() {
            return;
        }
        el = &mut el.children[index];
        el.collect_paint_hash = 0;
    }
    // Variant bind zeros the Item spine; title/media leaves must rehash too or
    // reuse_idle skip-replays "Reactive variant: outline" on top of the new
    // chrome (stacked glyphs + leftover ring).
    zero_collect_paint_hash_descendants(el);
}

/// Zero hashes from `root` down to each Taffy node so `reuse_idle` rehashes that
/// spine only. A global `hash_bust` rehashed the whole gallery on every hover
/// edge (trigger switch hitch). Returns false if any id is not in the tree.
pub fn invalidate_collect_paint_hash_spines(
    root: &mut Element,
    nodes: &[taffy::prelude::NodeId],
) -> bool {
    fn rec(el: &mut Element, want: taffy::prelude::NodeId) -> bool {
        if el.layout_node == Some(want) {
            el.collect_paint_hash = 0;
            zero_collect_paint_hash_descendants(el);
            return true;
        }
        for child in &mut el.children {
            if rec(child, want) {
                el.collect_paint_hash = 0;
                return true;
            }
        }
        false
    }
    let mut all = true;
    for &id in nodes {
        if !rec(root, id) {
            all = false;
        }
    }
    all
}

fn zero_collect_paint_hash_descendants(el: &mut Element) {
    for child in &mut el.children {
        child.collect_paint_hash = 0;
        zero_collect_paint_hash_descendants(child);
    }
}

/// Zero hashes on the DFS path to `template_id` (secondary windows have no child-index cache).
pub(crate) fn invalidate_collect_paint_hash_for_template(
    root: &mut Element,
    template_id: &str,
) -> bool {
    fn rec(el: &mut Element, want: &str) -> bool {
        let hit = el
            .template_id
            .as_deref()
            .is_some_and(|id| id.replace('\\', "/").eq_ignore_ascii_case(want));
        if hit {
            el.collect_paint_hash = 0;
            zero_collect_paint_hash_descendants(el);
            return true;
        }
        for child in &mut el.children {
            if rec(child, want) {
                el.collect_paint_hash = 0;
                return true;
            }
        }
        false
    }
    rec(root, template_id)
}

/// Bottom-up paint hash so a dirty leaf invalidates ancestors but not siblings.
///
/// `reuse_idle`: skip rehashing idle **subtrees** on scroll-only / ViewportsOnly
/// frames. `hash_paint_self` on every wrapper was the collect floor (~9ms). Live
/// and scroll-viewport paths still rehash so spin / `scroll_offset` stay fresh.
pub(crate) fn stamp_subtree_paint_hashes(root: &mut Element, reuse_idle: bool) {
    fn invalidate_dirty_canvases(el: &mut Element) -> bool {
        if el.collect_paint_hash != 0 && !el.collect_skip_has_canvas {
            return false;
        }
        let mut dirty = crate::primitives::canvas::canvas_paint_is_dirty(&el.attrs);
        for child in &mut el.children {
            dirty |= invalidate_dirty_canvases(child);
        }
        if dirty {
            el.collect_paint_hash = 0;
        }
        dirty
    }
    if crate::primitives::canvas::has_dirty_canvases() {
        invalidate_dirty_canvases(root);
    }
    fn rec(el: &mut Element, reuse_idle: bool, ancestor_live: bool) -> u64 {
        let node_live = node_collect_is_live(el);
        let self_live = ancestor_live || node_live;
        // Portal / scroll-viewport **self** stays unskippable (fade opacity, scroll
        // metrics). Their *ancestors* must remain reuse_idle-skippable when only a
        // sibling paint-hash was busted — otherwise inline ColorPicker slider hover
        // restamps the whole panel (SV + format Select portal + swatches).
        if reuse_idle
            && !self_live
            && !el.collect_skip_live
            && !el.collect_skip_has_scroll
            && !node_has_collect_side_channel(el)
            && !node_is_scrollbar_chrome(el)
            && !node_is_out_of_flow_page_chrome(el)
            && el.collect_paint_hash != 0
        {
            return el.collect_paint_hash;
        }
        #[cfg(test)]
        STAMP_HASHED.with(|c| c.set(c.get().saturating_add(1)));
        let mut h = PaintHasher::new();
        hash_paint_self(el, &mut h);
        // Anim/caret live bubbles UP so ancestors keep walking into spinners.
        // Portal side-channel bubbles on `collect_skip_side_channel` only — it must
        // not join `collect_skip_live`, or a leaf StyleChanged that zeros the shared
        // parent force-restamps every neighbor that merely hosts a Select portal
        // (inline ColorPicker SV + swatches next to format Select).
        let mut live = node_live;
        let mut side = node_has_collect_side_channel(el);
        let mut scroll_pin = el.style.position_sticky
            || el.style.position_fixed
            || node_is_scrollbar_chrome(el)
            || el.attrs.get(ATTR_ESCAPE_OVERFLOW_CLIP).map(String::as_str) == Some("1");
        let mut has_scroll = node_seeds_scroll_skip(el);
        let mut has_canvas = el
            .attrs
            .contains_key(crate::primitives::canvas::ATTR_CANVAS_PAINT);
        let mut css_active = node_paint_needs_frame(el)
            || !el.style.transition.is_empty()
            || el
                .class_set
                .as_ref()
                .is_some_and(|cs| !cs.variants.is_empty() || cs.last_pointer_hover == Some(true));
        let mut hover_armed =
            el.class_set.as_ref().and_then(|cs| cs.last_pointer_hover) == Some(true);
        let mut hover_cursor = el.on_hover.is_some() || el.cursor.is_some();
        let mut paint_sync = !el.authoring_state.is_empty()
            || el.input_paint_sync.is_some()
            || el.text_style.selectable
            || el.text_style.input_caret.is_some();
        let mut reactive_sync = el.reactive_sync.is_some();
        let mut presentation_sync =
            el.presentation_sync.is_some() || !el.tree_presentation_sync.is_empty();
        paint_sync |= presentation_sync || el.after_style_sync.is_some();
        let mut layout_motion = el.layout_motion.is_some() || el.liquid_flow.is_some();
        let mut collapse_work = reactive_sync
            || el.attrs.contains_key("jadix_collapse_measuring")
            || el.attrs.contains_key("jadix_collapse_damage");
        let mut floating_panel = el.attrs.contains_key(crate::utils::popup::ATTR_FLOATING)
            || el.attrs.contains_key("jadix_select_menu");
        for child in &mut el.children {
            rec(child, reuse_idle, self_live).hash(&mut h);
            live |= child.collect_skip_live;
            side |= child.collect_skip_side_channel;
            scroll_pin |= child.collect_skip_scroll_pin;
            has_scroll |= child.collect_skip_has_scroll;
            has_canvas |= child.collect_skip_has_canvas;
            css_active |= child.css_scroll_active;
            hover_armed |= child.css_hover_armed;
            paint_sync |= child.subtree_paint_sync;
            reactive_sync |= child.subtree_reactive_sync;
            presentation_sync |= child.subtree_presentation_sync;
            layout_motion |= child.subtree_layout_motion;
            collapse_work |= child.subtree_collapse_work;
            floating_panel |= child.subtree_floating_panel;
            hover_cursor |= child.subtree_hover_cursor;
        }
        let hash = h.finish();
        el.collect_paint_hash = hash;
        el.collect_skip_live = live || ancestor_live;
        el.collect_skip_side_channel = side;
        el.collect_skip_scroll_pin = scroll_pin;
        el.collect_skip_has_scroll = has_scroll;
        el.collect_skip_has_canvas = has_canvas;
        el.css_scroll_active = css_active;
        el.css_hover_armed = hover_armed;
        el.subtree_paint_sync = paint_sync;
        el.subtree_reactive_sync = reactive_sync;
        el.subtree_presentation_sync = presentation_sync;
        el.subtree_layout_motion = layout_motion;
        el.subtree_collapse_work = collapse_work;
        el.subtree_floating_panel = floating_panel;
        el.subtree_hover_cursor = hover_cursor;
        hash
    }
    rec(root, reuse_idle, false);
}

/// Collect calls this after stamping, when `collect_skip_live` includes every caret.
/// Focus can change independently, so a focused text edit still needs discovery.
pub(crate) fn stamped_subtree_holds_live_text_edit(el: &Element, focused: &str) -> bool {
    if el.collect_paint_hash != 0
        && !el.collect_skip_live
        && crate::utils::text_field::text_edit_rest(focused).is_none()
    {
        return false;
    }
    crate::core::tree::subtree_holds_live_text_edit(el, focused)
}

/// Scroll-only sibling culling must keep a subtree whose paint clock is still
/// active. The normal collect hash carries this bit upward, but the band walk
/// cannot infer it from geometry alone. Skipping a live canvas or shader here
/// stops its next-frame request and leaves the last texture on screen.
pub(crate) fn stamped_subtree_holds_live_paint(el: &Element) -> bool {
    el.collect_skip_live || el.children.iter().any(stamped_subtree_holds_live_paint)
}

pub(crate) fn invalidate_animation_shell_hashes(root: &mut Element) {
    fn visit(el: &mut Element) -> bool {
        let shell = el.template_id.as_deref().is_some_and(|id| {
            id.ends_with("__modal_overlay")
                || id.ends_with("__modal_panel")
                || id.ends_with("__modal_zoom")
                || id.ends_with("__floating_panel")
        });
        if shell {
            el.collect_paint_hash = 0;
            zero_collect_paint_hash_descendants(el);
            return true;
        }
        let mut changed = false;
        for child in &mut el.children {
            changed |= visit(child);
        }
        if changed {
            el.collect_paint_hash = 0;
        }
        changed
    }
    visit(root);
}

fn node_seeds_scroll_skip(el: &Element) -> bool {
    if el.contains_scroll_viewport {
        return true;
    }
    if el.scroll_offset[0] != 0.0 || el.scroll_offset[1] != 0.0 {
        return true;
    }
    if matches!(el.style.overflow.x, taffy::style::Overflow::Scroll)
        || matches!(el.style.overflow.y, taffy::style::Overflow::Scroll)
    {
        return true;
    }
    el.focus_id.as_deref().is_some_and(|id| {
        id.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
    })
}

fn hash_paint_self(el: &Element, h: &mut impl Hasher) {
    el.attrs.get(crate::core::tree::ATTR_OPACITY_GROUP).hash(h);
    el.paint_edge.is_some().hash(h);
    if let Some(edge) = el.paint_edge {
        edge.edge.hash(h);
        edge.width.to_bits().hash(h);
        hash_f32s(h, &edge.color);
    }
    el.paint_opaque_fill.hash(h);
    el.paint_region_replay.hash(h);
    el.paint_children_unculled.hash(h);
    el.attrs
        .contains_key(crate::style::liquid_flow::ITEM)
        .hash(h);
    if let Some(sample) = el
        .liquid_flow
        .as_ref()
        .and_then(crate::style::liquid_flow::LiquidFlow::sample)
    {
        hash_f32s(h, &sample);
    }
    if el.layout_motion.is_some() {
        hash_f32s(h, &crate::motion::presentation_translate(el));
        hash_f32s(h, &crate::motion::presentation_scale(el));
    }
    // Do not hash `layout_bounds`: collect stamps them after this pass, so
    // frame N's fingerprint would never match frame N+1. Position/size live in GeomKey.
    hash_f32s(h, &el.scroll_offset);
    el.text.hash(h);
    el.template_id.hash(h);
    el.image_texture.hash(h);
    el.attrs.get(crate::primitives::path::ATTR_PATH_D).hash(h);
    el.attrs.get("jadix_video_size").hash(h);
    el.attrs.get("jadix_video_fit").hash(h);
    el.attrs.get("jadix_video_content_ratio").hash(h);
    el.attrs.get("jadix_video_rotation").hash(h);
    el.attrs.get("jadix_video_mirror_x").hash(h);
    el.attrs.get("jadix_video_mirror_y").hash(h);
    el.children.len().hash(h);
    hash_f32s(h, &el.style.bg_color);
    hash_f32s(h, &el.style.border_color);
    el.style.border_width.to_bits().hash(h);
    el.style.opacity.to_bits().hash(h);
    // Auto scrollbar cubic is not baked into `bg_color`. Hash the live peek so
    // fingerprint skip busts the rail without disabling collect-skip for the page.
    if let Some(v) = crate::core::scrollbar::auto_scrollbar_live_paint_alpha(el) {
        v.to_bits().hash(h);
    }
    // Overlay thumbs are signal-driven (ViewportsOnly does not write Taffy margin).
    // Hash live offset / metrics or skip replays a frozen rail while the list scrolls.
    if let Some(bits) = crate::core::scrollbar::scrollbar_live_geometry_bits(el) {
        bits.hash(h);
    }
    hash_f32s(h, &el.style.radii);
    for s in el.style.resolved_corner_shapes() {
        std::mem::discriminant(&s).hash(h);
        s.to_shader_f32().to_bits().hash(h);
        s.shader_param().to_bits().hash(h);
    }
    hash_f32s(h, &el.style.shadow_color);
    hash_f32s(h, &el.style.shadow_offset);
    el.style.shadow_blur.to_bits().hash(h);
    el.style.shadow_spread.to_bits().hash(h);
    el.style.shadow_inset.hash(h);
    hash_f32s(h, &el.style.translate);
    // Combined transform samples `translate` px, not `%` of the box. Hashing
    // `translate_pct` here is required: fingerprint runs before collect stamps
    // `layout_bounds`, so the composed matrix would see a 0×0 box.
    hash_f32s(h, &el.style.translate_pct);
    // Independent transform props are sampled every CSS tick. Paint-only skip
    // must bust when they change or idle spin replays the rest-pose DL.
    hash_f32s(h, &el.style.scale);
    el.style.rotate_deg.to_bits().hash(h);
    // `transform-origin` keywords / % resolve against the border box. At
    // fingerprint time `layout_bounds` are often still 0×0, so Left/Center/Right
    // collapse to the same combined matrix. Hash the specified origin, not the
    // resolved px.
    hash_transform_origin(h, &el.style.transform_origin);
    crate::layout::lpa_length_px(el.style.inset.left)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    crate::layout::lpa_length_px(el.style.inset.top)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    el.style.z_index.hash(h);
    // Values, not list shape: bind_prop / CSS ticks can keep `len` / `Some` while
    // changing color, stops, or blur — skip would splice the previous DL.
    for sh in &el.style.extra_shadows {
        hash_box_shadow(h, sh);
    }
    match el.style.bg_gradient.as_ref() {
        Some(g) => {
            true.hash(h);
            hash_gradient(h, g);
        }
        None => false.hash(h),
    }
    hash_f32s(h, &el.style.bg_position);
    el.style.backdrop_blur.to_bits().hash(h);
    el.style.glass.is_some().hash(h);
    if let Some(glass) = el.style.glass {
        hash_f32s(h, &glass.gpu_params());
    }
    match el.style.transform.as_ref() {
        Some(list) => {
            true.hash(h);
            list.len().hash(h);
            for f in list {
                hash_transform_function(h, f);
            }
        }
        None => false.hash(h),
    }
    // Every specified transform input (including list values and origin) was
    // hashed above. Do not also compose/hash its derived matrix: composition
    // belongs to collect and would introduce last-frame layout_bounds here.
    match el.style.filter.as_ref() {
        Some(list) => {
            true.hash(h);
            hash_filter_list(h, list);
        }
        None => false.hash(h),
    }
    hash_clip_path(h, &el.style.clip_path);
    el.style.clip_path_name.hash(h);
    hash_f32s(h, &el.style.outline_color);
    el.style.outline_width.to_bits().hash(h);
    el.style.outline_offset.to_bits().hash(h);
    std::mem::discriminant(&el.style.outline_style).hash(h);
    for c in el.style.resolved_border_colors() {
        hash_f32s(h, &c);
    }
    for w in el.style.resolved_border_widths() {
        w.to_bits().hash(h);
    }
    for s in el.style.resolved_border_styles() {
        std::mem::discriminant(&s).hash(h);
    }
    el.style.border_image_source.hash(h);
    hash_f32s(h, &el.style.border_image_slice);
    el.style.border_image_slice_fill.hash(h);
    match el.style.border_image_width {
        Some(w) => {
            true.hash(h);
            hash_f32s(h, &w);
        }
        None => false.hash(h),
    }
    hash_f32s(h, &el.style.border_image_outset);
    for r in &el.style.border_image_repeat {
        std::mem::discriminant(r).hash(h);
    }
    el.text_style.letter_spacing.to_bits().hash(h);
    el.text_style.underline.hash(h);
    el.text_style.overline.hash(h);
    el.text_style.line_through.hash(h);
    el.text_style.decoration_color.hash(h);
    el.text_style
        .decoration_thickness
        .map(|t| t.to_bits())
        .hash(h);
    el.text_style.underline_offset.map(|t| t.to_bits()).hash(h);
    match el.text_style.decoration_style {
        Some(s) => {
            true.hash(h);
            std::mem::discriminant(&s).hash(h);
        }
        None => false.hash(h),
    }
    el.text_style.shadow_color.hash(h);
    hash_f32s(h, &el.text_style.shadow_offset);
    el.text_style.shadow_blur.to_bits().hash(h);
    if let Some(v) = el.attrs.get("jadix_animate") {
        v.hash(h);
    }
    el.text_style.color.hash(h);
    el.text_style.size.map(|s| s.to_bits()).hash(h);
    #[cfg(all(feature = "svg", feature = "animation-path"))]
    crate::utils::morph_icon::hash_morph_icon_paint(el, h);
    el.text_style.weight.hash(h);
    el.text_style.font_family.hash(h);
    el.text_style.align_last.hash(h);
    match el.text_style.align {
        Some(a) => {
            true.hash(h);
            std::mem::discriminant(&a).hash(h);
        }
        None => false.hash(h),
    }
    std::mem::discriminant(&el.text_style.white_space).hash(h);
    std::mem::discriminant(&el.text_style.wrap_mode).hash(h);
    std::mem::discriminant(&el.text_style.overflow).hash(h);
    std::mem::discriminant(&el.text_style.direction).hash(h);
    el.text_style.line_height_px.map(|v| v.to_bits()).hash(h);
    el.text_style.line_height_scale.map(|v| v.to_bits()).hash(h);
    std::mem::discriminant(&el.style.writing_mode).hash(h);
    // Focus / caret / selection are paint-only. Omitting them lets a parent
    // replay the unfocused DL: dest-reset wipes the field and the hairline
    // never comes back (Gallery Kbd Search).
    match el.text_style.input_caret.as_ref() {
        Some(c) => {
            true.hash(h);
            c.caret_byte_index.hash(h);
            c.selection.hash(h);
            c.extra_carets.hash(h);
            c.extra_selections.hash(h);
            c.overwrite_highlight.hash(h);
            c.preedit_underline.hash(h);
            c.extra_preedit_underlines.hash(h);
            c.preedit_ul.hash(h);
            c.drop_caret.hash(h);
            c.caret_color.hash(h);
            c.selection_bg.hash(h);
            c.selection_foreground.hash(h);
        }
        None => false.hash(h),
    }
    el.text_style.scroll_x.to_bits().hash(h);
    // Motion path is sampled at paint (`offset_path::sample`). Animation writes
    // `offset_distance` without LayoutChanged; skip would splice the rest pose.
    match &el.style.offset_path {
        OffsetPath::None => 0u8.hash(h),
        OffsetPath::Path(s) => {
            1u8.hash(h);
            s.hash(h);
        }
        OffsetPath::Url(s) => {
            2u8.hash(h);
            s.hash(h);
        }
    }
    el.style.offset_distance.to_bits().hash(h);
    match el.style.offset_rotate_deg {
        Some(d) => {
            true.hash(h);
            d.to_bits().hash(h);
        }
        None => false.hash(h),
    }
    if let Some(class) = el.attrs.get("class").or_else(|| el.attrs.get("className")) {
        class.hash(h);
    }
    // Overflow clip is baked into instance `clip_*` at collect (layout border box,
    // not rotated vis). Skipping a parent after visible→hidden replays unclipped
    // children. Discriminant only: Visible vs Hidden vs Scroll, not Taffy geometry.
    std::mem::discriminant(&el.style.overflow.x).hash(h);
    std::mem::discriminant(&el.style.overflow.y).hash(h);
    if let Some(v) = el.attrs.get(ATTR_ESCAPE_OVERFLOW_CLIP) {
        v.hash(h);
    }
}

/// Fingerprint skip is a **paint-only** cache of absolute display-list ops.
///
/// `Clean` / `StyleChanged`: geometry matches the previous frame; replay is valid
/// for subtrees that are not live. `StructureChanged` and live resize/settle still
/// gate skip off (new Taffy ids / mid-drag viewport). `LayoutChanged` (maximize
/// fit, wrap) **is** allowed: [`GeomKey`] includes origin+size, so moved flex
/// boxes miss replay instead of ghosting at the pre-grow Y. Gating skip off for
/// LayoutChanged forced a full collect every glyphon-fit frame after maximize.
///
/// Smooth scroll (`scroll_paint_only`) is **not** a gate: fully-visible scrolled
/// nodes translate last-frame ops by `GeomKey.scroll`; clean siblings keep replaying.
pub(crate) fn collect_skip_allowed_for_frame(
    layout: LayoutStatus,
    _scroll_paint_only: bool,
    full_rebuild: bool,
    viewport_resizing: bool,
    text_settling: bool,
) -> bool {
    matches!(
        layout,
        LayoutStatus::Clean | LayoutStatus::StyleChanged | LayoutStatus::LayoutChanged
    ) && !full_rebuild
        && !viewport_resizing
        && !text_settling
}

fn hash_f32s(h: &mut impl Hasher, xs: &[f32]) {
    for x in xs {
        x.to_bits().hash(h);
    }
}

fn hash_transform_origin(h: &mut impl Hasher, origin: &TransformOrigin) {
    hash_origin_x(h, &origin.x);
    hash_origin_y(h, &origin.y);
    origin.z.to_bits().hash(h);
}

fn hash_transform_function(h: &mut impl Hasher, f: &TransformFunction) {
    std::mem::discriminant(f).hash(h);
    match f {
        TransformFunction::Translate(x, y) => {
            hash_css_dim_paint(h, x);
            hash_css_dim_paint(h, y);
        }
        TransformFunction::TranslateX(x) | TransformFunction::TranslateY(x) => {
            hash_css_dim_paint(h, x)
        }
        TransformFunction::TranslateZ(d) => hash_css_dim_paint(h, d),
        TransformFunction::Translate3d(x, y, z) => {
            hash_css_dim_paint(h, x);
            hash_css_dim_paint(h, y);
            hash_css_dim_paint(h, z);
        }
        TransformFunction::Scale(a, b) => {
            a.to_bits().hash(h);
            b.is_some().hash(h);
            (*b).unwrap_or(*a).to_bits().hash(h);
        }
        TransformFunction::ScaleX(v)
        | TransformFunction::ScaleY(v)
        | TransformFunction::ScaleZ(v) => {
            v.to_bits().hash(h);
        }
        TransformFunction::Scale3d(x, y, z) => {
            x.to_bits().hash(h);
            y.to_bits().hash(h);
            z.to_bits().hash(h);
        }
        TransformFunction::Rotate(a)
        | TransformFunction::SkewX(a)
        | TransformFunction::SkewY(a)
        | TransformFunction::RotateX(a)
        | TransformFunction::RotateY(a)
        | TransformFunction::RotateZ(a) => hash_css_angle(h, a),
        TransformFunction::Skew(a, b) => {
            hash_css_angle(h, a);
            b.is_some().hash(h);
            if let Some(b) = b {
                hash_css_angle(h, b);
            }
        }
        TransformFunction::Rotate3d(x, y, z, a) => {
            x.to_bits().hash(h);
            y.to_bits().hash(h);
            z.to_bits().hash(h);
            hash_css_angle(h, a);
        }
        TransformFunction::Matrix(m) => hash_f32s(h, m),
        TransformFunction::Matrix3d(m) => hash_f32s(h, m),
        TransformFunction::Perspective(d) => hash_css_dim_paint(h, d),
    }
}

fn hash_origin_x(h: &mut impl Hasher, ox: &TransformOriginX) {
    match ox {
        TransformOriginX::Left => 0u8.hash(h),
        TransformOriginX::Center => 1u8.hash(h),
        TransformOriginX::Right => 2u8.hash(h),
        TransformOriginX::Length(d) => {
            3u8.hash(h);
            hash_css_dim_paint(h, d);
        }
    }
}

fn hash_origin_y(h: &mut impl Hasher, oy: &TransformOriginY) {
    match oy {
        TransformOriginY::Top => 0u8.hash(h),
        TransformOriginY::Center => 1u8.hash(h),
        TransformOriginY::Bottom => 2u8.hash(h),
        TransformOriginY::Length(d) => {
            3u8.hash(h);
            hash_css_dim_paint(h, d);
        }
    }
}

fn hash_css_dim_paint(h: &mut impl Hasher, d: &CssDim) {
    std::mem::discriminant(d).hash(h);
    match d {
        CssDim::Px(v)
        | CssDim::Percent(v)
        | CssDim::Em(v)
        | CssDim::Rem(v)
        | CssDim::Vw(v)
        | CssDim::Vh(v)
        | CssDim::Number(v) => v.to_bits().hash(h),
        CssDim::CalcAdd(a, b)
        | CssDim::CalcSub(a, b)
        | CssDim::CalcMul(a, b)
        | CssDim::CalcDiv(a, b) => {
            hash_css_dim_paint(h, a);
            hash_css_dim_paint(h, b);
        }
        CssDim::CalcSize { basis, calc } => {
            hash_css_dim_paint(h, basis);
            hash_css_dim_paint(h, calc);
        }
        CssDim::Auto
        | CssDim::FitContent
        | CssDim::MinContent
        | CssDim::MaxContent
        | CssDim::Size => {}
    }
}

fn hash_opt_f32(h: &mut impl Hasher, v: &Option<f32>) {
    match v {
        Some(x) => {
            true.hash(h);
            x.to_bits().hash(h);
        }
        None => false.hash(h),
    }
}

fn hash_box_shadow(h: &mut impl Hasher, sh: &BoxShadow) {
    hash_f32s(h, &sh.color);
    hash_f32s(h, &sh.offset);
    sh.blur.to_bits().hash(h);
    sh.spread.to_bits().hash(h);
    sh.inset.hash(h);
}

fn hash_color_stop(h: &mut impl Hasher, s: &ColorStop) {
    hash_f32s(h, &s.color);
    hash_opt_f32(h, &s.position);
    hash_opt_f32(h, &s.position_end);
    s.position_px.hash(h);
    s.position_end_px.hash(h);
}

fn hash_gradient_stops_and_hints(
    h: &mut impl Hasher,
    stops: &[ColorStop],
    hints: &[Option<crate::style::ColorHint>],
    repeating: bool,
) {
    repeating.hash(h);
    stops.len().hash(h);
    for s in stops {
        hash_color_stop(h, s);
    }
    hints.len().hash(h);
    for hint in hints {
        match hint {
            Some(c) => {
                true.hash(h);
                c.position.to_bits().hash(h);
            }
            None => false.hash(h),
        }
    }
}

fn hash_radial_extent(h: &mut impl Hasher, extent: &RadialExtent) {
    std::mem::discriminant(extent).hash(h);
    match *extent {
        RadialExtent::Keyword(k) => {
            std::mem::discriminant(&k).hash(h);
        }
        RadialExtent::Ellipse { rx, ry } => {
            rx.to_bits().hash(h);
            ry.to_bits().hash(h);
        }
        RadialExtent::Circle(r) => r.to_bits().hash(h),
    }
}

fn hash_gradient(h: &mut impl Hasher, g: &Gradient) {
    std::mem::discriminant(g).hash(h);
    match g {
        Gradient::Linear(lg) => {
            lg.angle_deg.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &lg.stops, &lg.hints, lg.repeating);
        }
        Gradient::Radial(rg) => {
            std::mem::discriminant(&rg.shape).hash(h);
            hash_radial_extent(h, &rg.extent);
            rg.position.x.to_bits().hash(h);
            rg.position.y.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &rg.stops, &rg.hints, rg.repeating);
        }
        Gradient::Conic(cg) => {
            cg.from_deg.to_bits().hash(h);
            cg.position.x.to_bits().hash(h);
            cg.position.y.to_bits().hash(h);
            hash_gradient_stops_and_hints(h, &cg.stops, &cg.hints, cg.repeating);
        }
    }
}

fn hash_css_dim(h: &mut impl Hasher, d: &CssDim) {
    std::mem::discriminant(d).hash(h);
    match d {
        CssDim::Auto
        | CssDim::FitContent
        | CssDim::MinContent
        | CssDim::MaxContent
        | CssDim::Size => {}
        CssDim::Px(v)
        | CssDim::Percent(v)
        | CssDim::Em(v)
        | CssDim::Rem(v)
        | CssDim::Vw(v)
        | CssDim::Vh(v)
        | CssDim::Number(v) => v.to_bits().hash(h),
        CssDim::CalcAdd(a, b)
        | CssDim::CalcSub(a, b)
        | CssDim::CalcMul(a, b)
        | CssDim::CalcDiv(a, b) => {
            hash_css_dim(h, a);
            hash_css_dim(h, b);
        }
        CssDim::CalcSize { basis, calc } => {
            hash_css_dim(h, basis);
            hash_css_dim(h, calc);
        }
    }
}

fn hash_opt_css_dim(h: &mut impl Hasher, d: &Option<CssDim>) {
    match d {
        Some(v) => {
            true.hash(h);
            hash_css_dim(h, v);
        }
        None => false.hash(h),
    }
}

fn hash_css_angle(h: &mut impl Hasher, a: &CssAngle) {
    std::mem::discriminant(a).hash(h);
    match *a {
        CssAngle::Deg(v) | CssAngle::Rad(v) | CssAngle::Grad(v) | CssAngle::Turn(v) => {
            v.to_bits().hash(h);
        }
    }
}

fn hash_filter_fn(h: &mut impl Hasher, f: &FilterFunction) {
    std::mem::discriminant(f).hash(h);
    match f {
        FilterFunction::Blur(d) => hash_opt_css_dim(h, d),
        FilterFunction::Brightness(v)
        | FilterFunction::Contrast(v)
        | FilterFunction::Grayscale(v)
        | FilterFunction::Invert(v)
        | FilterFunction::Opacity(v)
        | FilterFunction::Saturate(v)
        | FilterFunction::Sepia(v) => hash_opt_f32(h, v),
        FilterFunction::HueRotate(a) => match a {
            Some(ang) => {
                true.hash(h);
                hash_css_angle(h, ang);
            }
            None => false.hash(h),
        },
        FilterFunction::DropShadow {
            color,
            offset_x,
            offset_y,
            blur,
        } => {
            match color {
                Some(c) => {
                    true.hash(h);
                    hash_f32s(h, c);
                }
                None => false.hash(h),
            }
            hash_css_dim(h, offset_x);
            hash_css_dim(h, offset_y);
            hash_opt_css_dim(h, blur);
        }
        FilterFunction::Url(s) => s.hash(h),
    }
}

fn hash_filter_list(h: &mut impl Hasher, list: &[FilterFunction]) {
    list.len().hash(h);
    for f in list {
        hash_filter_fn(h, f);
    }
}

fn hash_clip_coord(h: &mut impl Hasher, c: &ClipCoord) {
    std::mem::discriminant(c).hash(h);
    match *c {
        ClipCoord::Px(v) | ClipCoord::Percent(v) => v.to_bits().hash(h),
    }
}

fn hash_clip_path(h: &mut impl Hasher, path: &ClipPath) {
    std::mem::discriminant(path).hash(h);
    match path {
        ClipPath::None => {}
        ClipPath::Inset { trbl } => {
            for c in trbl {
                hash_clip_coord(h, c);
            }
        }
        ClipPath::Polygon { fill, points } => {
            std::mem::discriminant(fill).hash(h);
            points.len().hash(h);
            for p in points {
                hash_clip_coord(h, &p[0]);
                hash_clip_coord(h, &p[1]);
            }
        }
        ClipPath::Circle { r, cx, cy } => {
            hash_clip_coord(h, r);
            hash_clip_coord(h, cx);
            hash_clip_coord(h, cy);
        }
        ClipPath::Ellipse { rx, ry, cx, cy } => {
            hash_clip_coord(h, rx);
            hash_clip_coord(h, ry);
            hash_clip_coord(h, cx);
            hash_clip_coord(h, cy);
        }
        ClipPath::Url(s) => s.hash(h),
    }
}

fn remap_ops(ops: &[PaintOp], inst_base: u32, tex_base: u32, out: &mut Vec<PaintOp>) {
    out.clear();
    out.extend(ops.iter().map(|op| match *op {
        PaintOp::Rects { start, count } => PaintOp::Rects {
            start: start.saturating_sub(inst_base),
            count,
        },
        PaintOp::Textured { start, count } => PaintOp::Textured {
            start: start.saturating_sub(tex_base),
            count,
        },
    }));
}

fn replay_cached_node_paints(
    cached: &CachedSubtree,
    geom: GeomKey,
    delta: [f32; 2],
    instances: &[InstanceInput],
    textured: &[TexturedDraw],
    text: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    paint_ops: &[PaintOp],
) {
    let payload = cached.payload.as_ref().unwrap();
    crate::layout::layout_tree::replay_node_paints(
        payload.node_paints[cached.start.node_paints - payload.base.node_paints
            ..cached.end.node_paints - payload.base.node_paints]
            .iter()
            .copied()
            .map(|mut p| {
                for (range, source, base) in [
                    (&mut p.instance, cached.start.instances, instances.len()),
                    (&mut p.textured, cached.start.textured, textured.len()),
                    (&mut p.text, cached.start.text, text.len()),
                    (&mut p.chrome, cached.start.chrome, chrome.len()),
                    (&mut p.paint_ops, cached.start.paint_ops, paint_ops.len()),
                ] {
                    if range.0 < source || range.1 < range.0 {
                        *range = (base, base);
                    } else {
                        range.0 = range.0 - source + base;
                        range.1 = range.1 - source + base;
                    }
                }
                p.bounds[0] += delta[0];
                p.bounds[1] += delta[1];
                if let Some(r) = &mut p.rest {
                    r[0] += delta[0];
                    r[1] += delta[1];
                }
                if let Some(r) = &mut p.overflow_clip {
                    if clip_is_parent([r[0], r[1]], [r[2], r[3]], &cached.geom) {
                        p.overflow_clip = geom_clip_rect(&geom);
                    } else {
                        r[0] += delta[0];
                        r[1] += delta[1];
                    }
                }
                p
            }),
    );
}

fn splice_cached(
    cached: &CachedSubtree,
    instances: &mut Vec<InstanceInput>,
    textured: &mut Vec<TexturedDraw>,
    text: &mut Vec<TextLayoutInfo>,
    chrome: &mut Vec<InstanceInput>,
    clip_polys: &mut Vec<GpuClipPoly>,
    paint_ops: &mut Vec<PaintOp>,
) {
    let inst_base = instances.len() as u32;
    let tex_base = textured.len() as u32;
    let payload = cached.payload.as_ref().unwrap();
    let start = cached.start.zip(payload.base, usize::saturating_sub);
    let end = cached.end.zip(payload.base, usize::saturating_sub);
    instances.extend_from_slice(
        payload
            .instances
            .get(start.instances..end.instances)
            .unwrap_or(&[]),
    );
    textured.extend_from_slice(
        payload
            .textured
            .get(start.textured..end.textured)
            .unwrap_or(&[]),
    );
    text.extend_from_slice(payload.text.get(start.text..end.text).unwrap_or(&[]));
    chrome.extend_from_slice(payload.chrome.get(start.chrome..end.chrome).unwrap_or(&[]));
    clip_polys.extend_from_slice(
        payload
            .clip_polys
            .get(start.clip_polys..end.clip_polys)
            .unwrap_or(&[]),
    );
    while clip_polys.len() < instances.len() {
        clip_polys.push(GpuClipPoly::NONE);
    }
    for op in &cached.paint_ops {
        match *op {
            PaintOp::Rects { start, count } => {
                paint_ops.push(PaintOp::Rects {
                    start: start + inst_base,
                    count,
                });
            }
            PaintOp::Textured { start, count } => {
                paint_ops.push(PaintOp::Textured {
                    start: start + tex_base,
                    count,
                });
            }
        }
    }
}

fn splice_cached_translated(
    cached: &CachedSubtree,
    delta: [f32; 2],
    new_geom: GeomKey,
    inherited_clip: &GpuClipPoly,
    instances: &mut Vec<InstanceInput>,
    textured: &mut Vec<TexturedDraw>,
    text: &mut Vec<TextLayoutInfo>,
    chrome: &mut Vec<InstanceInput>,
    clip_polys: &mut Vec<GpuClipPoly>,
    paint_ops: &mut Vec<PaintOp>,
) {
    let inst0 = instances.len();
    let tex0 = textured.len();
    let text0 = text.len();
    let chrome0 = chrome.len();
    let poly0 = clip_polys.len();
    splice_cached(
        cached, instances, textured, text, chrome, clip_polys, paint_ops,
    );
    let old = &cached.geom;
    for inst in &mut instances[inst0..] {
        nudge_instance(inst, delta, old, &new_geom);
    }
    for draw in &mut textured[tex0..] {
        nudge_textured(draw, delta, old, &new_geom, inherited_clip);
    }
    for node in &mut text[text0..] {
        nudge_text(node, delta, old, &new_geom);
    }
    for inst in &mut chrome[chrome0..] {
        nudge_instance(inst, delta, old, &new_geom);
    }
    for poly in &mut clip_polys[poly0..] {
        nudge_clip_poly(poly, delta, old, &new_geom, inherited_clip);
    }
}

fn clip_is_parent(pos: [f32; 2], size: [f32; 2], geom: &GeomKey) -> bool {
    geom.has_clip
        && size[0] >= 0.0
        && pos[0].to_bits() == geom.clip[0]
        && pos[1].to_bits() == geom.clip[1]
        && size[0].to_bits() == geom.clip[2]
        && size[1].to_bits() == geom.clip[3]
}

fn assign_parent_clip(pos: &mut [f32; 2], size: &mut [f32; 2], geom: &GeomKey) {
    if geom.has_clip {
        *pos = [f32::from_bits(geom.clip[0]), f32::from_bits(geom.clip[1])];
        *size = [f32::from_bits(geom.clip[2]), f32::from_bits(geom.clip[3])];
    } else {
        *pos = [0.0, 0.0];
        *size = InstanceInput::NO_CLIP_SIZE;
    }
}

fn nudge_instance(inst: &mut InstanceInput, delta: [f32; 2], old: &GeomKey, new: &GeomKey) {
    inst.rect_pos[0] += delta[0];
    inst.rect_pos[1] += delta[1];
    if inst.clip_size[0] < 0.0 {
        return;
    }
    if clip_is_parent(inst.clip_pos, inst.clip_size, old) {
        assign_parent_clip(&mut inst.clip_pos, &mut inst.clip_size, new);
    } else {
        inst.clip_pos[0] += delta[0];
        inst.clip_pos[1] += delta[1];
    }
}

fn nudge_textured(
    draw: &mut TexturedDraw,
    delta: [f32; 2],
    old: &GeomKey,
    new: &GeomKey,
    inherited_clip: &GpuClipPoly,
) {
    draw.rect[0] += delta[0];
    draw.rect[1] += delta[1];
    for i in [0, 2] {
        let radii = draw.rounded_clips[i + 1];
        let rect = &mut draw.rounded_clips[i];
        if inherited_round_clip(*rect, radii, inherited_clip) {
            continue;
        }
        if round_rect_is_parent(*rect, old) {
            assign_round_rect_parent(rect, new);
        } else if rect[2] > 0.5 {
            rect[0] += delta[0];
            rect[1] += delta[1];
        }
    }
    if draw.clip_size[0] < 0.0 {
        return;
    }
    if clip_is_parent(draw.clip_pos, draw.clip_size, old) {
        assign_parent_clip(&mut draw.clip_pos, &mut draw.clip_size, new);
    } else {
        draw.clip_pos[0] += delta[0];
        draw.clip_pos[1] += delta[1];
    }
}

fn nudge_text(node: &mut TextLayoutInfo, delta: [f32; 2], old: &GeomKey, new: &GeomKey) {
    node.x += delta[0];
    node.y += delta[1];
    node.paint_origin[0] += delta[0];
    node.paint_origin[1] += delta[1];
    if let Some(clip) = node.clip_bounds.as_mut() {
        if clip_is_parent([clip[0], clip[1]], [clip[2], clip[3]], old) {
            if new.has_clip {
                *clip = [
                    f32::from_bits(new.clip[0]),
                    f32::from_bits(new.clip[1]),
                    f32::from_bits(new.clip[2]),
                    f32::from_bits(new.clip[3]),
                ];
            }
        } else {
            clip[0] += delta[0];
            clip[1] += delta[1];
        }
    }
}

fn round_rect_is_parent(xywh: [f32; 4], geom: &GeomKey) -> bool {
    geom.has_clip
        && xywh[2] > 0.5
        && xywh[0].to_bits() == geom.clip[0]
        && xywh[1].to_bits() == geom.clip[1]
        && xywh[2].to_bits() == geom.clip[2]
        && xywh[3].to_bits() == geom.clip[3]
}

fn assign_round_rect_parent(xywh: &mut [f32; 4], geom: &GeomKey) {
    if geom.has_clip {
        *xywh = [
            f32::from_bits(geom.clip[0]),
            f32::from_bits(geom.clip[1]),
            f32::from_bits(geom.clip[2]),
            f32::from_bits(geom.clip[3]),
        ];
    } else {
        *xywh = [0.0, 0.0, 0.0, 0.0];
    }
}

fn nudge_pts(pts: &mut [[f32; 2]], count: u32, delta: [f32; 2]) {
    let n = (count as usize).min(pts.len());
    for p in pts.iter_mut().take(n) {
        p[0] += delta[0];
        p[1] += delta[1];
    }
}

// Ancestor masks are already in destination/world coordinates. A scroll moves
// content and its local masks, while inherited masks stay fixed. The AABB in
// GeomKey may be an intersection of several masks, so equality with that AABB
// alone cannot identify a fixed rounded or polygon clip.
fn inherited_round_clip(rect: [f32; 4], radii: [f32; 4], inherited: &GpuClipPoly) -> bool {
    rect[2] > 0.5
        && [
            (inherited.round_rect, inherited.round_radii),
            (inherited.round_rect2, inherited.round_radii2),
        ]
        .iter()
        .any(|&(r, rs)| r == rect && rs == radii)
}

fn nudge_clip_poly(
    poly: &mut GpuClipPoly,
    delta: [f32; 2],
    old: &GeomKey,
    new: &GeomKey,
    inherited: &GpuClipPoly,
) {
    for (rect, radii) in [
        (&mut poly.round_rect, poly.round_radii),
        (&mut poly.round_rect2, poly.round_radii2),
    ] {
        if inherited_round_clip(*rect, radii, inherited) {
            continue;
        }
        if round_rect_is_parent(*rect, old) {
            assign_round_rect_parent(rect, new);
        } else if rect[2] > 0.5 {
            rect[0] += delta[0];
            rect[1] += delta[1];
        }
    }
    for (count, fill, pts) in [
        (poly.count, poly.fill, &mut poly.pts),
        (poly.count2, poly.fill2, &mut poly.pts2),
    ] {
        let is_inherited = count >= 3
            && [
                (inherited.count, inherited.fill, &inherited.pts),
                (inherited.count2, inherited.fill2, &inherited.pts2),
            ]
            .iter()
            .any(|&(n, f, ps)| n == count && f == fill && ps == pts);
        if !is_inherited {
            nudge_pts(pts.as_mut_slice(), count, delta);
        }
    }
}

#[cfg(test)]
#[path = "tests/collect_cache.rs"]
mod tests;
