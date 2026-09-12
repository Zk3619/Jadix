//! Cross-frame fingerprint display list: skip collect of clean subtrees.
//!
//! Analog of GPUI `Entity::cached` without Entity: paint-hash the retained tree, replay
//! last frame's ops when a sibling subtree is unchanged. FullClear GPU present still
//! receives a complete list (replay, not omit) — see [`crate::app::damage`].

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use taffy::prelude::NodeId;

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::layout_tree::{PaintOp, TextLayoutInfo, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID};
use crate::renderer::{InstanceInput, TexturedDraw};
use crate::style::GpuClipPoly;

/// Indices into collect buffers at the start of a subtree visit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DlCursor {
    pub instances: usize,
    pub textured: usize,
    pub text: usize,
    pub chrome: usize,
    pub clip_polys: usize,
    pub paint_ops: usize,
}

impl DlCursor {
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
    opacity: u32,
    scale: [u32; 2],
}

impl GeomKey {
    fn new(
        abs: [f32; 2],
        size: [f32; 2],
        scroll: [f32; 2],
        clip: Option<[f32; 4]>,
        opacity: f32,
        scale: [f32; 2],
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
            opacity: opacity.to_bits(),
            scale: [scale[0].to_bits(), scale[1].to_bits()],
        }
    }
}

impl Hash for GeomKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.abs.hash(state);
        self.size.hash(state);
        self.scroll.hash(state);
        self.has_clip.hash(state);
        self.clip.hash(state);
        self.opacity.hash(state);
        self.scale.hash(state);
    }
}

pub(crate) struct CachedSubtree {
    paint_hash: u64,
    geom: GeomKey,
    instances: Vec<InstanceInput>,
    textured: Vec<TexturedDraw>,
    text: Vec<TextLayoutInfo>,
    chrome: Vec<InstanceInput>,
    clip_polys: Vec<GpuClipPoly>,
    paint_ops: Vec<PaintOp>,
    extent: [f32; 2],
}

#[derive(Default)]
pub(crate) struct CollectSkipState {
    /// When false (scroll / cold FullClear), never replay and do not clone the DL.
    pub allowed: bool,
    prev: HashMap<NodeId, CachedSubtree>,
    next: HashMap<NodeId, CachedSubtree>,
    pub skipped: u32,
    pub patched: u32,
}

impl CollectSkipState {
    pub fn begin_frame(&mut self) {
        self.next.clear();
        self.skipped = 0;
        self.patched = 0;
    }

    /// Drop fingerprint entries for a Taffy id (node recycled after unmount).
    pub fn evict(&mut self, node: NodeId) {
        self.prev.remove(&node);
        self.next.remove(&node);
    }

    /// Full Taffy rebuild may reuse `NodeId`s — drop fingerprint entries.
    pub fn invalidate(&mut self) {
        self.prev.clear();
        self.next.clear();
        self.skipped = 0;
        self.patched = 0;
    }

    pub fn end_frame(&mut self) {
        if self.allowed {
            // Skipped subtrees never `store` into `next`; keep last-frame entries
            // so a later dirty ancestor can still replay a clean descendant.
            for (k, v) in std::mem::take(&mut self.next) {
                self.prev.insert(k, v);
            }
        } else {
            // Layout / scroll / resize: absolute DLs are stale. Keeping `prev` across
            // a skip-off frame is what let maximize replay the pre-grow list.
            self.next.clear();
            self.prev.clear();
        }
    }

    pub fn try_replay(
        &mut self,
        node: NodeId,
        el: &Element,
        geom: GeomKey,
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
        if cached.paint_hash != el.collect_paint_hash || !geom_eq(&cached.geom, &geom) {
            if cached.paint_hash != 0 {
                self.patched = self.patched.saturating_add(1);
            }
            return None;
        }
        splice_cached(
            cached, instances, textured, text, chrome, clip_polys, paint_ops,
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
        let cached = capture_slice(
            el.collect_paint_hash,
            geom,
            start,
            instances,
            textured,
            text,
            chrome,
            clip_polys,
            paint_ops,
            extent,
        );
        self.next.insert(node, cached);
    }
}

fn geom_eq(a: &GeomKey, b: &GeomKey) -> bool {
    a.abs == b.abs
        && a.size == b.size
        && a.scroll == b.scroll
        && a.has_clip == b.has_clip
        && a.clip == b.clip
        && a.opacity == b.opacity
        && a.scale == b.scale
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
) -> GeomKey {
    GeomKey::new(abs, size, scroll, clip, opacity, scale)
}

/// Collect walk has side channels (portal `overlay_split`, drag overlay) that are
/// not stored in the fingerprint DL. Skipping an **ancestor** splices rects/glyphs
/// but never marks those cursors.
fn node_has_collect_side_channel(el: &Element) -> bool {
    el.template_id.as_deref() == Some(PORTAL_LAYER_ID)
        || el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1")
        || el.attrs.get("rui_portal").map(String::as_str) == Some("1")
        || el.attrs.contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY)
}

fn subtree_is_skippable(el: &Element) -> bool {
    if el.children.is_empty() {
        return false;
    }
    if el.collect_skip_live {
        return false;
    }
    if node_has_collect_side_channel(el) {
        return false;
    }
    true
}

fn node_collect_is_live(el: &Element) -> bool {
    // Caret on/off is a global clock, not a hash field. Skip would freeze the
    // last hairline (solid or hidden) until the next typed character.
    if el.text_style.input_caret.is_some() {
        return true;
    }
    if el.attrs.get("rui_animate").is_some_and(|v| {
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
    if el.attrs.contains_key(crate::utils::ATTR_ANIMATED_ICON) {
        return true;
    }
    if crate::style::css_animation_needs_frame(&el.style, &el.animation_snapshot) {
        return true;
    }
    el.transition_snapshot.has_active_tweens()
}

/// Bottom-up paint hash so a dirty leaf invalidates ancestors but not siblings.
pub(crate) fn stamp_subtree_paint_hashes(root: &mut Element) {
    fn rec(el: &mut Element) -> u64 {
        let mut h = DefaultHasher::new();
        hash_paint_self(el, &mut h);
        // Side-channel nodes stamp `live` so ancestors walk into them. Skipping the
        // root while a portal layer sits underneath parks `overlay_split` at `len`.
        let mut live = node_collect_is_live(el) || node_has_collect_side_channel(el);
        for child in &mut el.children {
            rec(child).hash(&mut h);
            live |= child.collect_skip_live;
        }
        let hash = h.finish();
        el.collect_paint_hash = hash;
        el.collect_skip_live = live;
        hash
    }
    rec(root);
}

fn hash_paint_self(el: &Element, h: &mut impl Hasher) {
    // Do not hash `layout_bounds`: collect stamps them after this pass, so
    // frame N's fingerprint would never match frame N+1. Position/size live in GeomKey.
    hash_f32s(h, &el.scroll_offset);
    el.text.hash(h);
    el.template_id.hash(h);
    el.image_texture.hash(h);
    el.children.len().hash(h);
    hash_f32s(h, &el.style.bg_color);
    hash_f32s(h, &el.style.border_color);
    el.style.border_width.to_bits().hash(h);
    el.style.opacity.to_bits().hash(h);
    hash_f32s(h, &el.style.radii);
    hash_f32s(h, &el.style.shadow_color);
    hash_f32s(h, &el.style.shadow_offset);
    el.style.shadow_blur.to_bits().hash(h);
    el.style.shadow_spread.to_bits().hash(h);
    el.style.shadow_inset.hash(h);
    hash_f32s(h, &el.style.translate);
    // Independent transform props are sampled every CSS tick. Paint-only skip
    // must bust when they change or idle spin replays the rest-pose DL.
    hash_f32s(h, &el.style.scale);
    el.style.rotate_deg.to_bits().hash(h);
    crate::layout::lpa_length_px(el.style.inset.left)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    crate::layout::lpa_length_px(el.style.inset.top)
        .unwrap_or(0.0)
        .to_bits()
        .hash(h);
    el.style.z_index.hash(h);
    el.style.extra_shadows.len().hash(h);
    el.style.bg_gradient.is_some().hash(h);
    match el.style.transform.as_ref() {
        Some(list) => {
            true.hash(h);
            list.len().hash(h);
            for f in list {
                std::mem::discriminant(f).hash(h);
            }
        }
        None => false.hash(h),
    }
    // CSS `animate-spin` writes the angle into the transform list, not `rotate_deg`.
    // Discriminant-only hashing above would keep the rest-pose fingerprint after maximize.
    let xf = crate::style::compute_combined_transform(
        &el.style.translate,
        el.style.rotate_deg,
        &el.style.scale,
        el.style.transform.as_ref(),
        &el.style.transform_origin,
        el.layout_bounds[2],
        el.layout_bounds[3],
    );
    for v in xf {
        v.to_bits().hash(h);
    }
    match el.style.filter.as_ref() {
        Some(list) => {
            true.hash(h);
            list.len().hash(h);
            for f in list {
                std::mem::discriminant(f).hash(h);
            }
        }
        None => false.hash(h),
    }
    if let Some(v) = el.attrs.get("rui_animate") {
        v.hash(h);
    }
    el.text_style.color.hash(h);
    el.text_style.size.map(|s| s.to_bits()).hash(h);
    el.text_style.weight.hash(h);
    // Focus / caret / selection are paint-only. Omitting them lets a parent
    // replay the unfocused DL: dest-reset wipes the field and the hairline
    // never comes back (Gallery Kbd Search).
    match el.text_style.input_caret.as_ref() {
        Some(c) => {
            true.hash(h);
            c.caret_byte_index.hash(h);
            c.selection.hash(h);
            c.overwrite_highlight.hash(h);
            c.preedit_underline.hash(h);
            c.drop_caret.hash(h);
        }
        None => false.hash(h),
    }
    el.text_style.scroll_x.to_bits().hash(h);
    if let Some(class) = el.attrs.get("class").or_else(|| el.attrs.get("className")) {
        class.hash(h);
    }
}

/// Fingerprint skip is a **paint-only** cache of absolute display-list ops.
///
/// `Clean` / `StyleChanged`: geometry matches the previous frame; replay is valid
/// for subtrees that are not live. `LayoutChanged` / `StructureChanged` (maximize,
/// text-fit settle, wrap) move flex boxes — replaying pre-reflow ops ghosts glyphs
/// and freezes time-sampled paint. Smooth scroll and live resize are the same class.
pub(crate) fn collect_skip_allowed_for_frame(
    layout: LayoutStatus,
    scroll_paint_only: bool,
    full_rebuild: bool,
    viewport_resizing: bool,
    text_settling: bool,
) -> bool {
    matches!(layout, LayoutStatus::Clean | LayoutStatus::StyleChanged)
        && !scroll_paint_only
        && !full_rebuild
        && !viewport_resizing
        && !text_settling
}

fn hash_f32s(h: &mut impl Hasher, xs: &[f32]) {
    for x in xs {
        x.to_bits().hash(h);
    }
}

fn capture_slice(
    paint_hash: u64,
    geom: GeomKey,
    start: DlCursor,
    instances: &[InstanceInput],
    textured: &[TexturedDraw],
    text: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    clip_polys: &[GpuClipPoly],
    paint_ops: &[PaintOp],
    extent: [f32; 2],
) -> CachedSubtree {
    let inst = instances.get(start.instances..).unwrap_or(&[]).to_vec();
    let tex = textured.get(start.textured..).unwrap_or(&[]).to_vec();
    let txt = text.get(start.text..).unwrap_or(&[]).to_vec();
    let chr = chrome.get(start.chrome..).unwrap_or(&[]).to_vec();
    let clips = clip_polys.get(start.clip_polys..).unwrap_or(&[]).to_vec();
    let ops = remap_ops(
        paint_ops.get(start.paint_ops..).unwrap_or(&[]),
        start.instances as u32,
        start.textured as u32,
    );
    CachedSubtree {
        paint_hash,
        geom,
        instances: inst,
        textured: tex,
        text: txt,
        chrome: chr,
        clip_polys: clips,
        paint_ops: ops,
        extent,
    }
}

fn remap_ops(ops: &[PaintOp], inst_base: u32, tex_base: u32) -> Vec<PaintOp> {
    ops.iter()
        .map(|op| match *op {
            PaintOp::Rects { start, count } => PaintOp::Rects {
                start: start.saturating_sub(inst_base),
                count,
            },
            PaintOp::Textured { start, count } => PaintOp::Textured {
                start: start.saturating_sub(tex_base),
                count,
            },
        })
        .collect()
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
    instances.extend_from_slice(&cached.instances);
    textured.extend_from_slice(&cached.textured);
    text.extend_from_slice(&cached.text);
    chrome.extend_from_slice(&cached.chrome);
    clip_polys.extend_from_slice(&cached.clip_polys);
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
