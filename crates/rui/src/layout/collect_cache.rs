//! Cross-frame fingerprint display list: skip collect of clean subtrees.
//!
//! Analog of GPUI `Entity::cached` without Entity: paint-hash the retained tree, replay
//! last frame's ops when a sibling subtree is unchanged. FullClear GPU present still
//! receives a complete list (replay, not omit) — see [`crate::app::damage`].

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use taffy::prelude::NodeId;

use crate::core::tree::Element;
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
            Some(c) => (true, [c[0].to_bits(), c[1].to_bits(), c[2].to_bits(), c[3].to_bits()]),
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
            // Scroll / FullClear: do not clone the whole DL into `prev`.
            self.next.clear();
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
        splice_cached(cached, instances, textured, text, chrome, clip_polys, paint_ops);
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

fn subtree_is_skippable(el: &Element) -> bool {
    if el.children.is_empty() {
        return false;
    }
    if el.template_id.as_deref() == Some(PORTAL_LAYER_ID) {
        return false;
    }
    if el.attrs.get(ATTR_HOISTED_PORTAL).map(String::as_str) == Some("1") {
        return false;
    }
    if el.attrs.get("rui_portal").map(String::as_str) == Some("1") {
        return false;
    }
    if el
        .attrs
        .contains_key(crate::utils::drag::ATTR_DRAG_OVERLAY)
    {
        return false;
    }
    true
}

/// Bottom-up paint hash so a dirty leaf invalidates ancestors but not siblings.
pub(crate) fn stamp_subtree_paint_hashes(root: &mut Element) {
    fn rec(el: &mut Element) -> u64 {
        let mut h = DefaultHasher::new();
        hash_paint_self(el, &mut h);
        for child in &mut el.children {
            rec(child).hash(&mut h);
        }
        let hash = h.finish();
        el.collect_paint_hash = hash;
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
    el.style.transform.is_some().hash(h);
    el.style.filter.is_some().hash(h);
    el.text_style.color.hash(h);
    el.text_style
        .size
        .map(|s| s.to_bits())
        .hash(h);
    el.text_style.weight.hash(h);
    if let Some(class) = el.attrs.get("class").or_else(|| el.attrs.get("className")) {
        class.hash(h);
    }
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
