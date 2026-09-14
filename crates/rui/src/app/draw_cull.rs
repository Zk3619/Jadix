//! Cull collect buffers to a damage AABB before GPU encode (B8).
//!
//! Only valid with retain `LoadOp::Load` + scissor: FullClear to a discarded
//! swapchain would leave holes. Scroll / structure still FullClear (caller).

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::layout::layout_tree::{NodePaint, TextLayoutInfo};
use crate::renderer::{
    aabb_of_centered_transform, aabbs_disjoint, InstanceInput, ScaledTexturedDraw,
};
use crate::style::transform::MAT2D_IDENTITY;

use super::damage::{intersect_rects, PaintDirty, PaintDirtyKind};

/// Same pad as scissor (`AppState::DAMAGE_SCISSOR_PAD_LOGICAL`) so cull ≥ clip.
pub(crate) const DAMAGE_CULL_PAD_LOGICAL: f32 = 16.0;

/// Glyph clip on a scroll-layer Clear. `DAMAGE_CULL_PAD_LOGICAL` is for SDF
/// shadows / scissor; using it on text reaches from the inset column into the
/// gallery rail and src-overs 「布局」/Gallery captions onto retain.
pub(crate) const SCROLL_LAYER_TEXT_PAD_LOGICAL: f32 = 1.0;

pub(crate) fn padded_xywh(rect: [f32; 4], pad: f32) -> [f32; 4] {
    let pad = pad.max(0.0);
    [
        rect[0] - pad,
        rect[1] - pad,
        (rect[2] + 2.0 * pad).max(0.0),
        (rect[3] + 2.0 * pad).max(0.0),
    ]
}

/// Cap on Clear/cull holes for one scroll blit frame (strips + sticky / overlay / named paint).
pub(crate) const CULL_RECTS_CAP: usize = 12;

/// N cull AABBs. Dual-axis L strips and extra pin holes — never their union
/// (Clear of the AABB would wipe the copied overlap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CullRects {
    rects: [[f32; 4]; CULL_RECTS_CAP],
    len: u8,
}

impl CullRects {
    pub(crate) fn one(a: [f32; 4]) -> Self {
        let mut s = Self {
            rects: [[0.0; 4]; CULL_RECTS_CAP],
            len: 0,
        };
        let _ = s.push(a);
        s
    }

    pub(crate) fn pair(a: [f32; 4], b: [f32; 4]) -> Self {
        let mut s = Self::one(a);
        let _ = s.push(b);
        s
    }

    pub(crate) fn push(&mut self, r: [f32; 4]) -> bool {
        if r[2] <= 0.0 || r[3] <= 0.0 {
            return true;
        }
        let n = self.len as usize;
        if n >= CULL_RECTS_CAP {
            return false;
        }
        self.rects[n] = r;
        self.len += 1;
        true
    }

    /// `None` when empty or more than [`CULL_RECTS_CAP`] non-empty rects.
    pub(crate) fn from_rects(rects: impl IntoIterator<Item = [f32; 4]>) -> Option<Self> {
        let mut s = Self {
            rects: [[0.0; 4]; CULL_RECTS_CAP],
            len: 0,
        };
        for r in rects {
            if !s.push(r) {
                return None;
            }
        }
        if s.len == 0 {
            None
        } else {
            Some(s)
        }
    }

    pub(crate) fn strips(self) -> impl Iterator<Item = [f32; 4]> {
        self.rects.into_iter().take(self.len as usize)
    }

    pub(crate) fn hits_instance(self, i: &InstanceInput) -> bool {
        self.strips().any(|r| instance_hits_damage(i, r))
    }

    pub(crate) fn hits_textured(self, d: &ScaledTexturedDraw) -> bool {
        self.strips().any(|r| textured_hits_damage(d, r))
    }

    pub(crate) fn hits_aabb(self, bounds: [f32; 4]) -> bool {
        self.strips().any(|r| !aabbs_disjoint(bounds, r))
    }

    pub(crate) fn count(self) -> usize {
        self.len as usize
    }
}

pub(crate) fn scale_xywh(rect: [f32; 4], scale: f32) -> [f32; 4] {
    let s = scale.max(0.0);
    [rect[0] * s, rect[1] * s, rect[2] * s, rect[3] * s]
}

/// Logical damage → physical cull AABB (pad then scale).
pub(crate) fn damage_cull_rect_physical(
    damage_logical: [f32; 4],
    pad_logical: f32,
    scale: f32,
) -> [f32; 4] {
    scale_xywh(padded_xywh(damage_logical, pad_logical), scale)
}

fn transform_is_identity(m: &[f32; 6]) -> bool {
    m.iter()
        .zip(MAT2D_IDENTITY.iter())
        .all(|(a, b)| (*a - *b).abs() <= 1e-5)
}

/// Outset the SDF vertex shader actually paints (`rounded_rect.wgsl` `ext` =
/// `|offset| + blur * 2 + spread + border + 2`). Dest-reset / overlay occupancy
/// that uses `blur` once leaves the halo in retain (dropdown smear on calendar).
pub(crate) fn sdf_outset_shadow_pad(
    blur: f32,
    spread: f32,
    offset: [f32; 2],
    border_width: f32,
) -> f32 {
    offset[0].abs().max(offset[1].abs())
        + blur.max(0.0) * 2.0
        + spread.abs()
        + border_width.max(0.0)
        + 2.0
}

/// Content box ∪ drop-shadow (outset only). Coordinates match `rect_pos` space.
pub(crate) fn instance_paint_bounds(i: &InstanceInput) -> [f32; 4] {
    let mut bounds = [i.rect_pos[0], i.rect_pos[1], i.rect_size[0], i.rect_size[1]];
    if i.shadow_inset == 0.0 {
        // Offset shifts the halo; pad is the SDF `ext` without `|offset|` (that
        // would double-count). Vertex shader uses `|offset| + blur*2 + …` as a
        // uniform quad pad — union of fill ∪ shifted halo is the same AABB.
        let extra =
            sdf_outset_shadow_pad(i.shadow_blur, i.shadow_spread, [0.0, 0.0], i.border_width);
        if extra > 2.0 || i.shadow_offset[0] != 0.0 || i.shadow_offset[1] != 0.0 {
            let sx = i.rect_pos[0] + i.shadow_offset[0] - extra;
            let sy = i.rect_pos[1] + i.shadow_offset[1] - extra;
            let sw = i.rect_size[0] + extra * 2.0;
            let sh = i.rect_size[1] + extra * 2.0;
            bounds = union_xywh(bounds, [sx, sy, sw, sh]);
        }
    }
    if !transform_is_identity(&i.transform) {
        bounds = aabb_of_centered_transform(bounds, &i.transform);
        // Layout `clip_*` is the local overflow box. Dest-reset / last-present
        // clip with `NodePaint.overflow_clip` (paint AABB). Intersecting the
        // world AABB with that layout box drops diamond tips.
        return bounds;
    }
    if i.clip_size[0] >= 0.0 && i.clip_size[1] >= 0.0 {
        if let Some(hit) = intersect_rects(
            bounds,
            [i.clip_pos[0], i.clip_pos[1], i.clip_size[0], i.clip_size[1]],
        ) {
            bounds = hit;
        } else {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    bounds
}

pub(crate) fn instance_hits_damage(i: &InstanceInput, damage: [f32; 4]) -> bool {
    !aabbs_disjoint(instance_paint_bounds(i), damage)
}

/// Texture quad AABB (transform + clip). Coordinates match `d.rect` space.
pub(crate) fn textured_paint_bounds(d: &ScaledTexturedDraw) -> [f32; 4] {
    let mut bounds = if transform_is_identity(&d.transform) {
        d.rect
    } else {
        aabb_of_centered_transform(d.rect, &d.transform)
    };
    if transform_is_identity(&d.transform) && d.clip_size[0] >= 0.0 && d.clip_size[1] >= 0.0 {
        match intersect_rects(
            bounds,
            [d.clip_pos[0], d.clip_pos[1], d.clip_size[0], d.clip_size[1]],
        ) {
            Some(hit) => bounds = hit,
            None => return [0.0, 0.0, 0.0, 0.0],
        }
    }
    bounds
}

pub(crate) fn textured_hits_damage(d: &ScaledTexturedDraw, damage: [f32; 4]) -> bool {
    let bounds = textured_paint_bounds(d);
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return false;
    }
    !aabbs_disjoint(bounds, damage)
}

/// Glyph box in logical px (shadow + paint transform + clip).
pub(crate) fn text_paint_bounds(n: &TextLayoutInfo) -> [f32; 4] {
    let extra = n.style.shadow_blur.max(0.0)
        + n.style.shadow_offset[0].abs()
        + n.style.shadow_offset[1].abs();
    let mut bounds = [
        n.x - extra,
        n.y - extra,
        (n.width + extra * 2.0).max(0.0),
        (n.height + extra * 2.0).max(0.0),
    ];
    if n.paint_scale != 1.0 || !transform_is_identity(&n.paint_transform) {
        bounds = aabb_of_centered_transform(bounds, &n.paint_transform);
    }
    if transform_is_identity(&n.paint_transform) && n.paint_scale == 1.0 {
        if let Some(clip) = n.clip_bounds {
            match intersect_rects(bounds, clip) {
                Some(hit) => bounds = hit,
                None => return [0.0, 0.0, 0.0, 0.0],
            }
        }
    }
    bounds
}

pub(crate) fn text_hits_damage(n: &TextLayoutInfo, damage_logical: [f32; 4]) -> bool {
    let bounds = text_paint_bounds(n);
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return false;
    }
    !aabbs_disjoint(bounds, damage_logical)
}

/// Label belongs to a hover-dirty widget when its **center** is inside the dirty box.
///
/// AABB intersect is too wide: gallery Button Variants sits labels a few px apart.
/// Capture **must not** use this — padded Input dirty covers sidebar captions in the
/// same horizontal band (`glyph_belongs_to_dirty` is collect-slot only).
#[allow(dead_code)]
pub(crate) fn text_owned_by_node_dirty(n: &TextLayoutInfo, dirty_logical: [f32; 4]) -> bool {
    super::damage::text_center_in_dirty([n.x, n.y, n.width, n.height], dirty_logical)
}

/// Contiguous instance ranges inside `start..start+count` that hit `cull`.
pub(crate) fn visible_instance_runs(
    instances: &[InstanceInput],
    start: u32,
    count: u32,
    cull: CullRects,
) -> Vec<(u32, u32)> {
    let start = start as usize;
    let end = (start + count as usize).min(instances.len());
    let start = start.min(end);
    let mut runs = Vec::new();
    let mut run_start: Option<usize> = None;
    for i in start..end {
        if cull.hits_instance(&instances[i]) {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else if let Some(s) = run_start.take() {
            runs.push((s as u32, (i - s) as u32));
        }
    }
    if let Some(s) = run_start {
        runs.push((s as u32, (end - s) as u32));
    }
    runs
}

/// Keep text nodes whose paint box hits padded logical damage.
pub(crate) fn filter_text_nodes(
    nodes: &[TextLayoutInfo],
    damage_logical: [f32; 4],
) -> Vec<TextLayoutInfo> {
    nodes
        .iter()
        .filter(|n| text_hits_damage(n, damage_logical))
        .cloned()
        .collect()
}

/// Scroll-layer glyphon: drop nodes outside `clip`, and intersect `clip_bounds`
/// so a false-positive AABB cannot stamp rail captions (glyphon does not always
/// honor the wgpu scissor used for the viewport Clear).
pub(crate) fn clip_text_nodes_to_rect(
    nodes: &[TextLayoutInfo],
    clip: [f32; 4],
) -> Vec<TextLayoutInfo> {
    if clip[2] <= 0.0 || clip[3] <= 0.0 {
        return Vec::new();
    }
    nodes
        .iter()
        .filter_map(|n| clip_text_node_to_rect(n, clip))
        .collect()
}

fn clip_text_node_to_rect(n: &TextLayoutInfo, clip: [f32; 4]) -> Option<TextLayoutInfo> {
    if !text_hits_damage(n, clip) {
        return None;
    }
    let hit = match n.clip_bounds {
        Some(existing) => intersect_rects(existing, clip)?,
        None => clip,
    };
    if hit[2] <= 0.0 || hit[3] <= 0.0 {
        return None;
    }
    let ink = [n.x, n.y, n.width.max(0.0), n.height.max(0.0)];
    if aabbs_disjoint(ink, hit) {
        return None;
    }
    let mut cloned = n.clone();
    cloned.clip_bounds = Some(hit);
    Some(cloned)
}

/// A node that spans several strips is emitted once per hit strip with disjoint
/// clips so overlap-only glyphs stay in the copied retain (glyphon clip_bounds
/// is AABB). Extra pin holes (sticky / overlay / named paint) use the same rule.
pub(crate) fn clip_text_nodes_to_strips(
    nodes: &[TextLayoutInfo],
    strips: &[[f32; 4]],
) -> Vec<TextLayoutInfo> {
    match strips {
        [] => Vec::new(),
        [a] => clip_text_nodes_to_rect(nodes, *a),
        _ => {
            let mut out = Vec::new();
            for n in nodes {
                for strip in strips {
                    if let Some(clipped) = clip_text_node_to_rect(n, *strip) {
                        out.push(clipped);
                    }
                }
            }
            out
        }
    }
}

pub(crate) fn clip_text_nodes_to_cull(
    nodes: &[TextLayoutInfo],
    cull: CullRects,
) -> Vec<TextLayoutInfo> {
    let strips: Vec<[f32; 4]> = cull.strips().collect();
    clip_text_nodes_to_strips(nodes, &strips)
}

fn approx_f32(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn text_run_unchanged(a: &TextLayoutInfo, b: &TextLayoutInfo) -> bool {
    const POS: f32 = 0.02;
    const SIZE: f32 = 0.5;
    // Collect clip_bounds is rebuilt every layout (scroll parent, inset). A clip-only
    // delta is not new ink — re-preparing that run on Load src-overs the caption.
    a.text == b.text
        && a.shape_suffix == b.shape_suffix
        && a.template_id == b.template_id
        && a.style == b.style
        && approx_f32(a.x, b.x, POS)
        && approx_f32(a.y, b.y, POS)
        && approx_f32(a.width, b.width, SIZE)
        && approx_f32(a.height, b.height, SIZE)
}

/// Glyphon payload for one page-stack band.
///
/// `None` = keep last GPU buffer and **do not** `render()` (Load retain already
/// has those pixels). `Some` = prepare/submit exactly these runs.
///
/// Send used to treat the whole page as one dirty text buffer, rebuild every
/// band (or the mixed sidebar+messages band in full), and `render()` stamped
/// rail captions into retain. Unchanged runs stay in retain; only new/moved
/// runs are submitted.
pub(crate) fn page_stack_band_submit_text<'a>(
    prev: &[TextLayoutInfo],
    curr: &'a [TextLayoutInfo],
    lo: usize,
    hi: usize,
    scroll_clip: Option<CullRects>,
    partial_load: bool,
) -> Option<Cow<'a, [TextLayoutInfo]>> {
    let hi = hi.min(curr.len());
    let lo = lo.min(hi);
    if lo >= hi {
        return None;
    }
    let slice = &curr[lo..hi];
    if !partial_load {
        return Some(Cow::Borrowed(slice));
    }
    if let Some(clip) = scroll_clip {
        let clipped = clip_text_nodes_to_cull(slice, clip);
        return if clipped.is_empty() {
            None
        } else {
            Some(Cow::Owned(clipped))
        };
    }
    // Match by ink identity, not collect index. Send inserts a bubble in the
    // middle of the band; shifting later slots used to mark 布局 as "new".
    let mut used = vec![false; prev.len()];
    let mut changed = Vec::new();
    for n in slice {
        let found = prev
            .iter()
            .enumerate()
            .find(|(i, p)| *i < used.len() && !used[*i] && text_run_unchanged(p, n));
        if let Some((i, _)) = found {
            used[i] = true;
        } else {
            changed.push(n.clone());
        }
    }
    if changed.is_empty() {
        None
    } else {
        Some(Cow::Owned(changed))
    }
}

pub(crate) fn page_stack_band_needs_glyphon(
    prev: &[TextLayoutInfo],
    curr: &[TextLayoutInfo],
    lo: usize,
    hi: usize,
    scroll_clip: Option<CullRects>,
    partial_load: bool,
) -> bool {
    page_stack_band_submit_text(prev, curr, lo, hi, scroll_clip, partial_load).is_some()
}

/// Compact chrome / underline / caret uploads to hits only.
pub(crate) fn filter_instances(instances: &[InstanceInput], cull: CullRects) -> Vec<InstanceInput> {
    instances
        .iter()
        .copied()
        .filter(|i| cull.hits_instance(i))
        .collect()
}

/// Opaque enough that src-over replaces dest (covers retain glyphs).
pub(crate) fn instance_fill_opaque(i: &InstanceInput) -> bool {
    i.color[3] > 0.95
}

/// Outset box-shadow pixels the SDF actually paints (Hard ring is spread-only,
/// `blur = 0`). Inset shadows stay inside the fill and do not grow dest-reset.
fn instance_has_outset_shadow(i: &InstanceInput) -> bool {
    i.shadow_inset == 0.0
        && i.shadow_color[3] > 0.05
        && (i.shadow_blur > 0.0
            || i.shadow_spread.abs() > 0.0
            || i.shadow_offset[0] != 0.0
            || i.shadow_offset[1] != 0.0)
}

/// Opaque fill, visible border, or outset shadow — pixels dest-reset must erase.
fn instance_has_dest_ink(i: &InstanceInput) -> bool {
    instance_fill_opaque(i)
        || (i.border_width > 0.0 && i.border_color[3] > 0.05)
        || instance_has_outset_shadow(i)
}

/// Topmost opaque fill containing `x,y` (walk instances in reverse draw order).
pub(crate) fn opaque_fill_color_at(
    instances: &[InstanceInput],
    x: f32,
    y: f32,
) -> Option<[f32; 4]> {
    opaque_fill_color_at_skipping(instances, x, y, |_| false)
}

pub(crate) fn opaque_fill_color_at_skipping(
    instances: &[InstanceInput],
    x: f32,
    y: f32,
    skip: impl Fn(usize) -> bool,
) -> Option<[f32; 4]> {
    instances.iter().enumerate().rev().find_map(|(i, inst)| {
        if skip(i) || !instance_fill_opaque(inst) {
            return None;
        }
        let [px, py] = inst.rect_pos;
        let [pw, ph] = inst.rect_size;
        if x >= px && y >= py && x < px + pw && y < py + ph {
            Some(inst.color)
        } else {
            None
        }
    })
}

fn slot_matches_dirty(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    p: &NodePaint,
    paints: &[NodePaint],
) -> bool {
    if p.overlay {
        return false;
    }
    let has_keyed = super::damage::paint_dirty_has_keyed_slot(
        dirty_key,
        dirty_bounds,
        paints
            .iter()
            .filter(|q| !q.overlay)
            .map(|q| (q.key, q.bounds)),
    );
    super::damage::paint_dirty_matches_present_slot(
        dirty_key,
        dirty_bounds,
        p.key,
        p.bounds,
        has_keyed,
    )
}

/// Card/page fill behind a dirty widget. Skips the widget's own instances
/// (Default's opaque chrome would otherwise report itself as the background).
pub(crate) fn node_present_behind_color(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    x: f32,
    y: f32,
) -> Option<[f32; 4]> {
    opaque_fill_color_at_skipping(instances, x, y, |i| {
        node_paints.iter().any(|p| {
            slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints)
                && i >= p.instance.0
                && i < p.instance.1
        })
    })
}

/// Shader vertex AA pad is +2px; keep this below gallery `gap-2` (8 logical px).
pub(crate) const NODE_PRESENT_RESET_PAD_PHYSICAL: f32 = 3.0;

/// Last-present may follow a modest shrink (8px outline, 12px shadow, filter wrap).
/// A recycled NodeId whose previous slot was the card / inset must not union that
/// AABB into dest-reset or the node-present scissor.
pub(crate) const LAST_PRESENT_PREV_MAX_OUTSET: f32 = 48.0;

/// Clip last-present prev to this dirty box plus [`LAST_PRESENT_PREV_MAX_OUTSET`].
///
/// A same-size slot that moved (translate / last-frame spin AABB) must keep
/// prev. A recycled NodeId whose previous slot was the card / inset has a much
/// larger area — clamp that to the dirty neighborhood.
pub(crate) fn clamp_last_present_prev(prev: [f32; 4], dirty_bounds: [f32; 4]) -> Option<[f32; 4]> {
    if prev[2] <= 0.0 || prev[3] <= 0.0 || dirty_bounds[2] <= 0.0 || dirty_bounds[3] <= 0.0 {
        return None;
    }
    let prev_area = prev[2] * prev[3];
    let dirty_area = dirty_bounds[2] * dirty_bounds[3];
    if prev_area <= dirty_area * 4.0 + 1.0 {
        return Some(prev);
    }
    intersect_rects(
        prev,
        padded_xywh(dirty_bounds, LAST_PRESENT_PREV_MAX_OUTSET),
    )
}

/// Tightest ancestor overflow clip on slots matching this dirty (logical px).
pub(crate) fn node_present_overflow_clip(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    let mut acc: Option<[f32; 4]> = None;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints) {
            continue;
        }
        let Some(c) = p.overflow_clip else {
            continue;
        };
        if c[2] <= 0.0 || c[3] <= 0.0 {
            continue;
        }
        acc = Some(match acc {
            Some(prev) => intersect_rects(prev, c).unwrap_or(prev),
            None => c,
        });
    }
    acc
}

/// Owner clip on capture glyphs. 1px covers coverage AA; the 8px node scissor
/// pad must not be used here — it overlaps `gap-2` siblings and stamps captions.
pub(crate) const NODE_PRESENT_GLYPH_CLIP_PAD: f32 = 1.0;

/// Tight AABB of this dirty node's **ink**, not the scissor-padded dirty box.
///
/// `PaintDirty.bounds` includes Input caret slack (+8px) and spin circumcircle
/// chrome. Wiping the former punches InputAdornment border / ⌘K; those pixels
/// belong to other collect slots. Dest-reset may only erase what this slot
/// painted: opaque/border/shadow SDF, textured quads, or — for text-only
/// `bind_text` — the unpadded collect box.
///
/// Spin leftover at identity angle is the previous frame's transformed AABB
/// (`prev_bounds` / `last_present_node_bounds`), not this frame's dirty pad.
///
/// `prev_bounds` is the last presented **visual** slot for this key. Dest-reset of
/// `now` only leaves a maximize/scroll ghost a few pixels away.
pub(crate) fn node_present_reset_rect(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    pad: f32,
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let s = ui_scale.max(0.0);
    let mut ink: Option<[f32; 4]> = None;
    let mut slot_visual: Option<[f32; 4]> = None;
    let mut matched = false;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints) {
            continue;
        }
        matched = true;
        if p.bounds[2] > 0.0 && p.bounds[3] > 0.0 {
            let sb = scale_xywh(p.bounds, s);
            slot_visual = Some(match slot_visual {
                Some(prev) => union_xywh(prev, sb),
                None => sb,
            });
        }
        let lo = p.instance.0.min(instances.len());
        let hi = p.instance.1.min(instances.len());
        for inst in &instances[lo..hi] {
            let mut b = instance_paint_bounds(inst);
            if b[2] <= 0.0 || b[3] <= 0.0 {
                continue;
            }
            if transform_is_identity(&inst.transform) && p.bounds[2] > 0.0 && p.bounds[3] > 0.0 {
                let extra = (inst.shadow_blur.max(0.0)
                    + inst.shadow_spread.abs()
                    + inst.shadow_offset[0].abs()
                    + inst.shadow_offset[1].abs()
                    + inst.border_width.max(0.0)
                    + 2.0)
                    * s;
                match intersect_rects(b, padded_xywh(scale_xywh(p.bounds, s), extra)) {
                    Some(hit) => b = hit,
                    None => continue,
                }
            }
            if instance_has_dest_ink(inst) || !transform_is_identity(&inst.transform) {
                ink = Some(match ink {
                    Some(prev) => union_xywh(prev, b),
                    None => b,
                });
            }
        }
        let tlo = p.textured.0.min(textured.len());
        let thi = p.textured.1.min(textured.len());
        for d in &textured[tlo..thi] {
            let b = textured_paint_bounds(d);
            if b[2] <= 0.0 || b[3] <= 0.0 {
                continue;
            }
            ink = Some(match ink {
                Some(prev) => union_xywh(prev, b),
                None => b,
            });
        }
    }
    if !matched {
        return None;
    }
    let overflow = node_present_overflow_clip(dirty_key, dirty_bounds, node_paints);
    let mut acc = ink.or(slot_visual);
    if let Some(prev) = prev_bounds.and_then(|p| clamp_last_present_prev(p, dirty_bounds)) {
        let pb = scale_xywh(prev, s);
        acc = Some(match acc {
            Some(now) => union_xywh(now, pb),
            None => pb,
        });
    }
    // SDF AA fringe around real ink. Transparent / text-only slots must not grow
    // by RESET_PAD — that 3px eats a 1px parent border (Kbd InputAdornment).
    // Hard ring is 3 *logical* px (spread, blur=0). RESET_PAD is 3 *physical*
    // px; at 150%+ that is smaller than the ring. When `prev_bounds` is missing
    // (key recycle / first present), dest-ink wipes still have to cover it.
    let wipe_pad = if ink.is_some() {
        let aa = pad.max(0.0);
        if prev_bounds.is_none() {
            aa.max(crate::style::FOCUS_RING_HARD_SPREAD * s)
        } else {
            aa
        }
    } else {
        0.0
    };
    acc.map(|r| padded_xywh(r, wipe_pad))
        .and_then(|r| match overflow {
            Some(c) => intersect_rects(r, scale_xywh(c, s)),
            None => Some(r),
        })
}

/// Collect-slot visual box plus this frame's transformed instance / texture / glyph ink.
pub(crate) fn node_paint_visual_bounds(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> [f32; 4] {
    let mut acc = if p.bounds[2] > 0.0 && p.bounds[3] > 0.0 {
        Some(p.bounds)
    } else {
        None
    };
    let lo = p.instance.0.min(instances.len());
    let hi = p.instance.1.min(instances.len());
    for inst in &instances[lo..hi] {
        let b = instance_paint_bounds(inst);
        if b[2] <= 0.0 || b[3] <= 0.0 {
            continue;
        }
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    let tlo = p.textured.0.min(textured.len());
    let thi = p.textured.1.min(textured.len());
    for d in &textured[tlo..thi] {
        let b = textured_paint_bounds(d);
        if b[2] <= 0.0 || b[3] <= 0.0 {
            continue;
        }
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    // Last-present must cover glyph outset (text-shadow). Dest-reset of *now*
    // still does not treat text as dest-ink — that 3px pad eats a 1px parent
    // border (Kbd InputAdornment). Shrink uses `prev_bounds` instead.
    //
    // Clip each run to this slot plus shadow/decoration outset. Unclipped
    // overflow (nowrap past `min-w-0`, wrapping measure miss) used to grow
    // last-present into a sticky header / sibling column; dest-reset of that
    // AABB punched foreign slots and only this node's glyphs redrew.
    let glo = p.text.0.min(texts.len());
    let ghi = p.text.1.min(texts.len());
    for n in &texts[glo..ghi] {
        let Some(b) = text_ink_in_slot(n, p.bounds) else {
            continue;
        };
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    // Caret / Hard-ring live in `input_chrome_instances`, not the SDF instance
    // range. Dest-reset of *now* still skips chrome as dest-ink (pad vs ⌘K).
    // Do not clip to the layout slot: a caret / ring outset outside RESET_PAD
    // must still enter last-present so unfocus dest-reset can erase it.
    let clo = p.chrome.0.min(chrome.len());
    let chi = p.chrome.1.min(chrome.len());
    for inst in &chrome[clo..chi] {
        let b = instance_paint_bounds(inst);
        if b[2] <= 0.0 || b[3] <= 0.0 {
            continue;
        }
        let b = match p.overflow_clip {
            Some(c) if c[2] > 0.0 && c[3] > 0.0 => match intersect_rects(b, c) {
                Some(hit) => hit,
                None => continue,
            },
            _ => b,
        };
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    let mut acc = acc.unwrap_or(p.bounds);
    if let Some(c) = p.overflow_clip {
        if c[2] > 0.0 && c[3] > 0.0 {
            if let Some(hit) = intersect_rects(acc, c) {
                acc = hit;
            }
        }
    }
    acc
}

/// Glyph AABB that dest-reset / last-present may follow: shadow outset yes,
/// layout overflow into a neighbor slot no.
fn text_ink_in_slot(n: &TextLayoutInfo, slot: [f32; 4]) -> Option<[f32; 4]> {
    let b = text_paint_bounds(n);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return None;
    }
    if slot[2] <= 0.0 || slot[3] <= 0.0 {
        return Some(b);
    }
    let extra = n.style.shadow_blur.max(0.0)
        + n.style.shadow_offset[0].abs()
        + n.style.shadow_offset[1].abs()
        + 2.0;
    intersect_rects(b, padded_xywh(slot, extra))
}

/// Page-node visual slots for `last_present_node_bounds` (logical px).
/// Call **before** `scale_instances_in_place` / `take` — dest-reset scales prev by `ui_scale`.
pub(crate) fn last_present_visual_slots(
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> Vec<(u64, [f32; 4])> {
    node_paints
        .iter()
        .filter(|p| !p.overlay)
        .map(|p| {
            (
                p.key,
                node_paint_visual_bounds(p, instances, textured, texts, chrome),
            )
        })
        .collect()
}

/// Node-present dest-reset may union last-present **only** for page `Node` dirties.
/// Overlay / OverlayRestore scissor to their own hole and must not inherit a page prev.
pub(crate) fn last_present_prev_for_dirty(
    kind: PaintDirtyKind,
    key: u64,
    slots: &[(u64, [f32; 4])],
) -> Option<[f32; 4]> {
    match kind {
        PaintDirtyKind::Node => slots.iter().find(|(k, _)| *k == key).map(|(_, b)| *b),
        PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore => None,
    }
}

/// Final **logical** node-present scissor: clamp last-present, union dirty, intersect
/// ancestor overflow, pad, then intersect overflow again so the 8px Node pad cannot
/// leak into a title bar sitting above a ScrollArea.
pub(crate) fn node_present_scissor_logical(
    kind: PaintDirtyKind,
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    prev_slots: &[(u64, [f32; 4])],
    node_paints: &[NodePaint],
    scissor_pad: f32,
) -> [f32; 4] {
    let prev = last_present_prev_for_dirty(kind, dirty_key, prev_slots)
        .and_then(|p| clamp_last_present_prev(p, dirty_bounds));
    let mut scissor = match prev {
        Some(prev) => union_xywh(dirty_bounds, prev),
        None => dirty_bounds,
    };
    let overflow = match kind {
        PaintDirtyKind::Node => node_present_overflow_clip(dirty_key, dirty_bounds, node_paints),
        PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore => None,
    };
    if let Some(c) = overflow {
        scissor = intersect_rects(scissor, c).unwrap_or(scissor);
    }
    scissor = padded_xywh(scissor, scissor_pad);
    if let Some(c) = overflow {
        scissor = intersect_rects(scissor, c).unwrap_or(scissor);
    }
    scissor
}

pub(crate) fn node_present_scissor_pad(kind: PaintDirtyKind) -> f32 {
    match kind {
        PaintDirtyKind::Overlay => 16.0,
        PaintDirtyKind::OverlayRestore => 0.0,
        PaintDirtyKind::Node => 8.0,
    }
}

/// Scroll blit cannot run node-present encode (`scroll_layer_clear`). Pin the same
/// dest-reset hole: last-present prev ∪ now, clamped, overflow-clipped, padded.
/// Using only `dirty.bounds` leaves the previous hover box in the copied overlap.
pub(crate) fn named_scroll_blit_pins(
    dirties: &[PaintDirty],
    prev_slots: &[(u64, [f32; 4])],
    node_paints: &[NodePaint],
) -> Vec<[f32; 4]> {
    dirties
        .iter()
        .map(|d| {
            node_present_scissor_logical(
                d.kind,
                d.key,
                d.bounds,
                prev_slots,
                node_paints,
                node_present_scissor_pad(d.kind),
            )
        })
        .filter(|r| r[2] > 0.0 && r[3] > 0.0)
        .collect()
}

/// Axis-aligned dest wipe for node present: card fill behind this widget, never the widget itself.
#[cfg(test)]
pub(crate) fn node_present_dest_reset(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> Option<([f32; 4], [f32; 4])> {
    node_present_dest_reset_scaled(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        &[],
        1.0,
        None,
    )
}

/// [`node_present_dest_reset`] with `ui_scale` so a text-only dirty box maps into
/// the scaled instance space used at present.
pub(crate) fn node_present_dest_reset_scaled(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> Option<([f32; 4], [f32; 4])> {
    let rect = node_present_reset_rect(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        textured,
        NODE_PRESENT_RESET_PAD_PHYSICAL,
        ui_scale,
        prev_bounds,
    )?;
    let color =
        node_present_behind_color_for_rect(dirty_key, dirty_bounds, node_paints, instances, rect)?;
    Some((rect, color))
}

fn node_present_behind_color_for_rect(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    rect: [f32; 4],
) -> Option<[f32; 4]> {
    let skip = |i: usize| {
        node_paints.iter().any(|p| {
            slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints)
                && i >= p.instance.0
                && i < p.instance.1
        })
    };
    let samples = [
        [rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5],
        [rect[0] + 1.0, rect[1] + 1.0],
        [rect[0] + rect[2] - 1.0, rect[1] + 1.0],
        [rect[0] + 1.0, rect[1] + rect[3] - 1.0],
        [rect[0] + rect[2] - 1.0, rect[1] + rect[3] - 1.0],
    ];
    if let Some(c) = samples.iter().find_map(|p| {
        node_present_behind_color(dirty_key, dirty_bounds, node_paints, instances, p[0], p[1])
    }) {
        return Some(c);
    }
    instances.iter().enumerate().rev().find_map(|(i, inst)| {
        if skip(i) || !instance_fill_opaque(inst) {
            return None;
        }
        let b = instance_paint_bounds(inst);
        if b[2] <= 0.0 || b[3] <= 0.0 {
            return None;
        }
        intersect_rects(b, rect).map(|_| inst.color)
    })
}

/// Text indices owned by any page dirty node (restore hole or widget).
///
/// Encode prepares this set **once** with per-glyph owner clips, then draws it
/// **once** after every dest-reset. Drawing the union under each node's scissor
/// stamps neighbor captions into retain (`Load` + glyph alpha).
pub(crate) fn node_present_capture_text_indices(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> BTreeSet<usize> {
    let mut idx = BTreeSet::new();
    for dirty in dirty_nodes {
        idx.extend(node_present_glyphs_for_dirty(dirty, page_text, node_paints));
    }
    idx
}

/// Glyphs that belong to this dirty node (not the padded-scissor union).
pub(crate) fn node_present_glyphs_for_dirty(
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> BTreeSet<usize> {
    let mut idx = BTreeSet::new();
    match dirty.kind {
        PaintDirtyKind::OverlayRestore => {
            for (i, n) in page_text.iter().enumerate() {
                if text_hits_damage(n, dirty.bounds) {
                    idx.insert(i);
                }
            }
        }
        PaintDirtyKind::Overlay => {}
        PaintDirtyKind::Node => {
            for (i, n) in page_text.iter().enumerate() {
                if glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints) {
                    idx.insert(i);
                }
            }
        }
    }
    idx
}

/// Capture copies with `clip_bounds` intersected to the owning dirty box.
///
/// Neighbor captions stay in the buffer for a single present, but cannot paint
/// inside another widget's dest-reset hole.
pub(crate) fn node_present_clipped_capture_text(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> Vec<TextLayoutInfo> {
    node_present_capture_text_indices(dirty_nodes, page_text, node_paints)
        .into_iter()
        .filter_map(|i| {
            let n = page_text.get(i)?;
            let owner = owner_clip_for_glyph(i, n, dirty_nodes, page_text, node_paints)?;
            let clip = match n.clip_bounds {
                Some(existing) => intersect_rects(existing, owner)?,
                None => owner,
            };
            let mut cloned = n.clone();
            cloned.clip_bounds = Some(clip);
            Some(cloned)
        })
        .collect()
}

fn owner_clip_for_glyph(
    i: usize,
    n: &TextLayoutInfo,
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    let mut acc: Option<[f32; 4]> = None;
    for dirty in dirty_nodes {
        if glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints) {
            let owner = padded_xywh(dirty.bounds, NODE_PRESENT_GLYPH_CLIP_PAD);
            acc = Some(match acc {
                Some(prev) => union_xywh(prev, owner),
                None => owner,
            });
        }
    }
    acc
}

/// Capture glyphs that `capture_text_renderer.render()` would stamp into `dirty`'s
/// scissor even though they are not owned by that dirty node.
pub(crate) fn node_present_foreign_glyph_indices(
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    capture: &BTreeSet<usize>,
    scissor_pad_logical: f32,
) -> Vec<usize> {
    if !matches!(
        dirty.kind,
        PaintDirtyKind::Node | PaintDirtyKind::OverlayRestore
    ) {
        return Vec::new();
    }
    let scissor = padded_xywh(dirty.bounds, scissor_pad_logical);
    capture
        .iter()
        .copied()
        .filter(|&i| {
            let Some(n) = page_text.get(i) else {
                return false;
            };
            if !text_hits_damage(n, scissor) {
                return false;
            }
            glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints)
                .then_some(())
                .is_none()
        })
        .collect()
}

fn glyph_belongs_to_dirty(
    i: usize,
    n: &TextLayoutInfo,
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> bool {
    match dirty.kind {
        PaintDirtyKind::OverlayRestore => text_hits_damage(n, dirty.bounds),
        PaintDirtyKind::Overlay => false,
        PaintDirtyKind::Node => {
            // Collect-slot membership only. Center-in-box steals neighbor captions
            // whose ink sits in the padded dirty (Gallery sidebar 「布局」 sharing
            // a horizontal band with Kbd Search) and src-overs them into retain.
            node_paints.iter().any(|p| {
                slot_matches_dirty(dirty.key, dirty.bounds, p, node_paints) && {
                    let lo = p.text.0.min(page_text.len());
                    let hi = p.text.1.min(page_text.len());
                    i >= lo && i < hi
                }
            })
        }
    }
}

/// Matching collect slot owns glyphon (paint-only `bind_text` labels have no SDF fill).
pub(crate) fn node_dirty_has_text(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    node_paints
        .iter()
        .any(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints) && p.text.0 < p.text.1)
}

/// Dest-reset wipes retain glyphs; opaque chrome also covers them. Text-only
/// `bind_text` slots have neither fill nor dest-reset instances — they still
/// need the capture glyphon path or mousemove counters freeze in retain.
#[cfg(test)]
pub(crate) fn node_present_needs_glyphs(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    node_present_needs_glyphs_scaled(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        &[],
        1.0,
        None,
    )
}

pub(crate) fn node_present_needs_glyphs_scaled(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> bool {
    node_present_dest_reset_scaled(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        textured,
        ui_scale,
        prev_bounds,
    )
    .is_some()
        || node_dirty_has_opaque_fill(dirty_key, dirty_bounds, node_paints, instances)
        || node_dirty_has_text(dirty_key, dirty_bounds, node_paints)
}

/// Hover chrome whose fill covers this dirty node's glyphs (primary vs link/ghost rest).
pub(crate) fn node_dirty_has_opaque_fill(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    node_paints.iter().any(|p| {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, node_paints) {
            return false;
        }
        let lo = p.instance.0.min(instances.len());
        let hi = p.instance.1.min(instances.len());
        instances[lo..hi].iter().any(instance_fill_opaque)
    })
}

fn union_xywh(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].min(b[0]);
    let y0 = a[1].min(b[1]);
    let x1 = (a[0] + a[2]).max(b[0] + b[2]);
    let y1 = (a[1] + a[3]).max(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::InstanceInput;
    use crate::style::TextStyle;

    fn blank_instance(x: f32, y: f32, w: f32, h: f32) -> InstanceInput {
        InstanceInput {
            rect_pos: [x, y],
            rect_size: [w, h],
            color: [1.0, 0.0, 0.0, 1.0],
            border_color: [0.0; 4],
            radii: [0.0; 4],
            border_width: 0.0,
            shadow_color: [0.0; 4],
            shadow_offset: [0.0, 0.0],
            shadow_blur: 0.0,
            shadow_spread: 0.0,
            shadow_inset: 0.0,
            transform: MAT2D_IDENTITY,
            clip_pos: [0.0, 0.0],
            clip_size: InstanceInput::NO_CLIP_SIZE,
            border_extra: 0.0,
            backdrop_blur: 0.0,
        }
    }

    fn blank_textured(x: f32, y: f32, w: f32, h: f32, transform: [f32; 6]) -> ScaledTexturedDraw {
        ScaledTexturedDraw {
            rect: [x, y, w, h],
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            texture_key: String::new(),
            opacity: 1.0,
            tint_rgb: [1.0, 1.0, 1.0],
            transform,
            clip_pos: [0.0, 0.0],
            clip_size: crate::renderer::TexturedDraw::NO_CLIP_SIZE,
        }
    }

    #[test]
    fn far_sibling_instance_is_culled() {
        let a = blank_instance(0.0, 0.0, 20.0, 20.0);
        let b = blank_instance(400.0, 0.0, 20.0, 20.0);
        let damage = [0.0, 0.0, 40.0, 40.0];
        assert!(instance_hits_damage(&a, damage));
        assert!(!instance_hits_damage(&b, damage));
        let runs = visible_instance_runs(&[a, b], 0, 2, CullRects::one(damage));
        assert_eq!(runs, vec![(0, 1)]);
    }

    #[test]
    fn outset_shadow_keeps_instance_when_box_is_outside() {
        let mut i = blank_instance(50.0, 0.0, 10.0, 10.0);
        i.shadow_offset = [-40.0, 0.0];
        i.shadow_blur = 8.0;
        i.shadow_color = [0.0, 0.0, 0.0, 1.0];
        let damage = [0.0, 0.0, 20.0, 20.0];
        assert!(
            instance_hits_damage(&i, damage),
            "drop-shadow must inflate cull bounds"
        );
    }

    #[test]
    fn instance_paint_bounds_covers_sdf_blur_twice() {
        // Select menu Elevation::Lg blur=12. Shader ext is blur*2+2; 1× blur
        // occupancy left the halo on the calendar after dest-reset of the fill.
        let mut i = blank_instance(100.0, 100.0, 40.0, 160.0);
        i.shadow_blur = 12.0;
        i.shadow_color = [0.0, 0.0, 0.0, 0.14];
        let b = instance_paint_bounds(&i);
        let pad = sdf_outset_shadow_pad(12.0, 0.0, [0.0, 0.0], 0.0);
        assert!(pad >= 26.0, "SDF ext is blur*2+2 = 26, got {pad}");
        assert!(
            b[0] <= 100.0 - 24.0 && b[1] <= 100.0 - 24.0,
            "occupancy must cover the SDF halo, got {b:?}"
        );
        assert!(b[2] >= 40.0 + 48.0 && b[3] >= 160.0 + 48.0);
    }

    #[test]
    fn instance_paint_bounds_rotated_keeps_diamond_outside_layout_clip() {
        let mut i = blank_instance(0.0, 0.0, 80.0, 80.0);
        i.transform = crate::style::mat2d_world_center_pivot(
            &MAT2D_IDENTITY,
            &crate::style::mat2d_rotate(45.0_f32.to_radians()),
            [40.0, 40.0],
            [80.0, 80.0],
        );
        i.clip_pos = [0.0, 0.0];
        i.clip_size = [80.0, 80.0];
        let b = instance_paint_bounds(&i);
        assert!(
            b[1] < -8.0 && b[0] < -8.0 && b[2] > 90.0 && b[3] > 90.0,
            "world AABB of the 45° quad must keep diamond tips, not layout clip [0,0,80,80], got {b:?}"
        );
        let mut identity = blank_instance(0.0, 0.0, 80.0, 80.0);
        identity.clip_pos = [8.0, 8.0];
        identity.clip_size = [40.0, 40.0];
        let clipped = instance_paint_bounds(&identity);
        assert!(
            (clipped[0] - 8.0).abs() < 0.01 && (clipped[2] - 40.0).abs() < 0.01,
            "identity dest-reset still intersects layout clip, got {clipped:?}"
        );
    }

    #[test]
    fn middle_gap_splits_runs() {
        let inst = [
            blank_instance(0.0, 0.0, 10.0, 10.0),
            blank_instance(200.0, 0.0, 10.0, 10.0),
            blank_instance(8.0, 0.0, 10.0, 10.0),
        ];
        let damage = [0.0, 0.0, 30.0, 30.0];
        assert_eq!(
            visible_instance_runs(&inst, 0, 3, CullRects::one(damage)),
            vec![(0, 1), (2, 1)]
        );
    }

    fn sample_text(text: &str, x: f32) -> TextLayoutInfo {
        TextLayoutInfo {
            text: text.into(),
            shape_suffix: None,
            spans: None,
            template_id: None,
            x,
            y: 0.0,
            width: 20.0,
            height: 16.0,
            clip_bounds: None,
            style: TextStyle::default(),
            prepared: None,
            glyph_fx: None,
            shimmer: None,
            paint_scale: 1.0,
            paint_transform: MAT2D_IDENTITY,
            paint_origin: [x, 0.0],
            glyph_raster: crate::renderer::GlyphRasterMode::Coverage,
        }
    }

    fn slot(key: u64, bounds: [f32; 4], text: (usize, usize)) -> NodePaint {
        NodePaint {
            key,
            bounds,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text,
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        }
    }

    #[test]
    fn text_outside_damage_filtered() {
        let kept = filter_text_nodes(
            &[sample_text("in", 0.0), sample_text("out", 400.0)],
            [0.0, 0.0, 40.0, 40.0],
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "in");
    }

    #[test]
    fn scroll_layer_text_clip_drops_rail_captions_in_the_same_y_band() {
        // Gallery MessageScroller: inset column starts ~216px, 16px instance pad
        // grazes a full-width sidebar label. Glyphs must follow the unpadded viewport.
        let mut rail = sample_text("布局", 8.0);
        rail.y = 200.0;
        rail.width = 200.0;
        rail.height = 20.0;
        let mut msg = sample_text("hello", 260.0);
        msg.y = 200.0;
        msg.width = 80.0;
        msg.height = 20.0;
        let inset = [216.0, 8.0, 1056.0, 784.0];
        let padded = padded_xywh(inset, DAMAGE_CULL_PAD_LOGICAL);
        assert!(
            text_hits_damage(&rail, padded),
            "16px instance pad is what used to steal rail captions"
        );
        let tight = padded_xywh(inset, SCROLL_LAYER_TEXT_PAD_LOGICAL);
        let kept = clip_text_nodes_to_rect(&[rail.clone(), msg.clone()], tight);
        assert!(
            kept.iter().all(|n| n.text != "布局"),
            "scroll-layer glyphon must not prepare 布局, got {:?}",
            kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
        );
        assert!(
            kept.iter().any(|n| n.text == "hello"),
            "in-scroller glyphs must stay, got {:?}",
            kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
        );

        let ms = [240.0, 128.0, 400.0, 384.0];
        let mut y_band = sample_text("Gallery", 8.0);
        y_band.y = 180.0;
        y_band.width = 48.0;
        y_band.height = 20.0;
        let kept_ms =
            clip_text_nodes_to_rect(&[y_band], padded_xywh(ms, SCROLL_LAYER_TEXT_PAD_LOGICAL));
        assert!(
            kept_ms.is_empty(),
            "Y-overlapping rail caption must not follow the inner MessageScroller viewport"
        );
    }

    #[test]
    fn dual_axis_strips_must_not_prepare_overlap_only_glyphs() {
        // Viewport 200×100, delta [12, 8]: bottom 8px band + right 12×92 band.
        let exp_h = [0.0, 92.0, 200.0, 8.0];
        let exp_v = [188.0, 0.0, 12.0, 92.0];
        let mut overlap = sample_text("kept-by-copy", 40.0);
        overlap.y = 20.0;
        overlap.width = 80.0;
        overlap.height = 20.0;
        let mut bottom = sample_text("new-bottom", 10.0);
        bottom.y = 94.0;
        bottom.width = 40.0;
        bottom.height = 12.0;
        let mut right = sample_text("new-right", 190.0);
        right.y = 10.0;
        right.width = 16.0;
        right.height = 20.0;
        let mut spanning = sample_text("span", 0.0);
        spanning.y = 0.0;
        spanning.width = 200.0;
        spanning.height = 100.0;
        let kept = clip_text_nodes_to_strips(&[overlap, bottom, right, spanning], &[exp_h, exp_v]);
        assert!(
            kept.iter().all(|n| n.text != "kept-by-copy"),
            "overlap-only glyphs must stay in the copied retain, got {:?}",
            kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
        );
        assert!(kept.iter().any(|n| n.text == "new-bottom"));
        assert!(kept.iter().any(|n| n.text == "new-right"));
        let span_clips: Vec<_> = kept
            .iter()
            .filter(|n| n.text == "span")
            .filter_map(|n| n.clip_bounds)
            .collect();
        assert_eq!(
            span_clips.len(),
            2,
            "spanning glyph must split into two strip clips"
        );
        assert!(span_clips
            .iter()
            .any(|c| (c[1] - 92.0).abs() < 0.1 && (c[3] - 8.0).abs() < 0.1));
        assert!(span_clips
            .iter()
            .any(|c| (c[0] - 188.0).abs() < 0.1 && (c[2] - 12.0).abs() < 0.1));
    }

    #[test]
    fn cull_rects_from_rects_caps_at_twelve() {
        let too_many: Vec<_> = (0..13).map(|i| [i as f32, 0.0, 1.0, 1.0]).collect();
        assert!(CullRects::from_rects(too_many).is_none());
        let ok: Vec<_> = (0..12).map(|i| [i as f32, 0.0, 1.0, 1.0]).collect();
        let cull = CullRects::from_rects(ok).expect("cap");
        assert_eq!(cull.count(), 12);
        assert!(CullRects::from_rects(std::iter::empty()).is_none());
    }

    #[test]
    fn pin_hole_must_prepare_overlap_glyph_that_strips_drop() {
        let exp_h = [0.0, 92.0, 200.0, 8.0];
        let exp_v = [188.0, 0.0, 12.0, 92.0];
        let pin = [8.0, 8.0, 40.0, 24.0];
        let mut overlap = sample_text("sticky-header", 10.0);
        overlap.y = 10.0;
        overlap.width = 36.0;
        overlap.height = 20.0;
        let strips_only = clip_text_nodes_to_strips(&[overlap.clone()], &[exp_h, exp_v]);
        assert!(
            strips_only.iter().all(|n| n.text != "sticky-header"),
            "overlap-only sticky must not be in L strips"
        );
        let with_pin = clip_text_nodes_to_strips(&[overlap], &[exp_h, exp_v, pin]);
        assert!(
            with_pin.iter().any(|n| n.text == "sticky-header"),
            "sticky pin hole must re-prepare the header after copy"
        );
    }

    #[test]
    fn send_append_must_not_rebuild_sidebar_glyphon_band() {
        // Gallery Send: new bubble is later in collect order. Band 0 is the rail.
        let mut layout = sample_text("布局", 8.0);
        layout.y = 200.0;
        let mut gallery = sample_text("Gallery", 8.0);
        gallery.y = 236.0;
        let mut hello = sample_text("hello", 260.0);
        hello.y = 200.0;
        let mut next = sample_text("next", 260.0);
        next.y = 240.0;
        let prev = vec![layout.clone(), gallery.clone(), hello.clone()];
        let curr = vec![layout, gallery, hello, next];
        assert!(
            !page_stack_band_needs_glyphon(&prev, &curr, 0, 2, None, true),
            "sidebar band must reuse after Send"
        );
        assert!(
            page_stack_band_needs_glyphon(&prev, &curr, 2, 4, None, true),
            "message band must rebuild after Send"
        );
        assert!(
            page_stack_band_needs_glyphon(&prev, &curr, 0, 2, None, false),
            "FullClear still rebuilds the rail band"
        );
        let ms = [240.0, 128.0, 400.0, 384.0];
        assert!(
            !page_stack_band_needs_glyphon(&prev, &curr, 0, 2, Some(CullRects::one(ms)), true),
            "inner follow Clear must not submit the rail band"
        );
        assert!(
            page_stack_band_needs_glyphon(&prev, &curr, 2, 4, Some(CullRects::one(ms)), true),
            "inner follow Clear must submit the message band"
        );
    }

    #[test]
    fn send_append_mixed_band_must_not_submit_sidebar_runs() {
        // Production often keeps 布局 and the new bubble in one stacking band.
        // PartialLoad must submit only the new run — not re-prepare the rail.
        let mut layout = sample_text("布局", 8.0);
        layout.y = 200.0;
        let mut gallery = sample_text("Gallery", 8.0);
        gallery.y = 236.0;
        let mut hello = sample_text("hello", 260.0);
        hello.y = 200.0;
        let mut next = sample_text("next", 260.0);
        next.y = 240.0;
        let prev = vec![layout.clone(), gallery.clone(), hello.clone()];
        let curr = vec![layout, gallery, hello, next];
        let submitted = page_stack_band_submit_text(&prev, &curr, 0, 4, None, true)
            .expect("Send must submit the new bubble");
        assert!(
            submitted.iter().any(|n| n.text == "next"),
            "new bubble must be submitted, got {:?}",
            submitted
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            submitted
                .iter()
                .all(|n| n.text != "布局" && n.text != "Gallery" && n.text != "hello"),
            "unchanged rail/bubble runs must stay in retain, got {:?}",
            submitted
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>()
        );
        let full = page_stack_band_submit_text(&prev, &curr, 0, 4, None, false)
            .expect("FullClear still owns the whole band");
        assert_eq!(full.len(), 4);
    }

    #[test]
    fn send_insert_before_rail_must_not_submit_unchanged_captions() {
        let mut layout = sample_text("布局", 8.0);
        layout.y = 200.0;
        let mut gallery = sample_text("Gallery", 8.0);
        gallery.y = 236.0;
        let mut next = sample_text("next", 260.0);
        next.y = 200.0;
        let prev = vec![layout.clone(), gallery.clone()];
        let curr = vec![next.clone(), layout, gallery];
        let submitted = page_stack_band_submit_text(&prev, &curr, 0, 3, None, true)
            .expect("inserted bubble must submit");
        assert!(submitted.iter().any(|n| n.text == "next"));
        assert!(
            submitted
                .iter()
                .all(|n| n.text != "布局" && n.text != "Gallery"),
            "index shift must not re-submit rail ink, got {:?}",
            submitted
                .iter()
                .map(|n| n.text.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn clip_bounds_only_change_must_not_resubmit_rail() {
        let mut prev = sample_text("布局", 8.0);
        prev.y = 200.0;
        let mut curr = prev.clone();
        curr.clip_bounds = Some([0.0, 0.0, 208.0, 800.0]);
        assert!(
            page_stack_band_submit_text(&[prev], &[curr], 0, 1, None, true).is_none(),
            "layout-rebuilt clip is not new ink"
        );
    }

    #[test]
    fn node_dirty_owns_centered_label_not_sibling() {
        let dirty = [8.0, 8.0, 88.0, 36.0];
        assert!(text_owned_by_node_dirty(
            &sample_text("Default", 24.0),
            dirty
        ));
        assert!(!text_owned_by_node_dirty(
            &sample_text("Outline", 104.0),
            dirty
        ));
    }

    #[test]
    fn chrome_filter_drops_far_rects() {
        let kept = filter_instances(
            &[
                blank_instance(0.0, 0.0, 10.0, 10.0),
                blank_instance(400.0, 0.0, 10.0, 10.0),
            ],
            CullRects::one([0.0, 0.0, 40.0, 40.0]),
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].rect_pos, [0.0, 0.0]);
    }

    #[test]
    fn physical_overlay_instance_converts_to_logical_occupancy() {
        let mut i = blank_instance(800.0, 400.0, 240.0, 176.0);
        i.shadow_blur = 16.0;
        i.shadow_color = [0.0, 0.0, 0.0, 0.4];
        let logical = scale_xywh(instance_paint_bounds(&i), 0.5);
        assert!(logical[2] > 120.0);
        assert!(logical[3] > 88.0);
        assert!(logical[0] < 400.0);
        assert!(logical[1] < 200.0);
    }

    #[test]
    fn opaque_fill_skips_transparent_and_picks_topmost() {
        let mut card = blank_instance(0.0, 0.0, 100.0, 100.0);
        card.color = [0.9, 0.9, 0.9, 1.0];
        let mut link = blank_instance(10.0, 10.0, 40.0, 20.0);
        link.color = [0.0, 0.0, 0.0, 0.0];
        assert_eq!(
            opaque_fill_color_at(&[card, link], 20.0, 15.0),
            Some([0.9, 0.9, 0.9, 1.0])
        );
    }

    #[test]
    fn node_dirty_has_opaque_fill_false_for_transparent_slot() {
        let mut inst = blank_instance(0.0, 0.0, 40.0, 20.0);
        inst.color = [0.0; 4];
        let p = NodePaint {
            key: 1,
            bounds: [0.0, 0.0, 40.0, 20.0],
            overlay: false,
            instance: (0, 1),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        assert!(!node_dirty_has_opaque_fill(
            1,
            [0.0, 0.0, 40.0, 20.0],
            &[p],
            &[inst]
        ));
        inst.color = [0.0, 0.0, 0.0, 1.0];
        assert!(node_dirty_has_opaque_fill(
            1,
            [0.0, 0.0, 40.0, 20.0],
            &[p],
            &[inst]
        ));
    }

    #[test]
    fn dest_reset_skips_self_and_uses_card_behind() {
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let mut button = blank_instance(10.0, 10.0, 40.0, 20.0);
        button.color = [0.1, 0.1, 0.1, 1.0];
        let p = NodePaint {
            key: 7,
            bounds: [10.0, 10.0, 40.0, 20.0],
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let instances = [card, button];
        assert_eq!(
            node_present_behind_color(7, p.bounds, &[p], &instances, 30.0, 20.0),
            Some([0.96, 0.96, 0.96, 1.0])
        );
        let (rect, color) =
            node_present_dest_reset(7, p.bounds, &[p], &instances).expect("card behind default");
        assert_eq!(color, [0.96, 0.96, 0.96, 1.0]);
        assert!(rect[0] <= 10.0 - 2.0);
        assert!(rect[1] <= 10.0 - 2.0);
        assert!(rect[0] + rect[2] >= 50.0 + 2.0);
        assert!(rect[1] + rect[3] >= 30.0 + 2.0);
        assert!(rect[2] < 80.0, "must not expand to the whole card");
        assert!(node_present_needs_glyphs(7, p.bounds, &[p], &instances));
    }

    #[test]
    fn dest_reset_covers_rotated_textured_spin_not_just_unrotated_fill() {
        // Icon+spin is a lucide texture + transform. A same-slot unrotated fill
        // used to win dest-reset; the previous 45° corners stayed in retain.
        let mut card = blank_instance(0.0, 0.0, 80.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(32.0, 32.0, 16.0, 16.0);
        fill.color = [0.0, 0.0, 0.0, 0.0];
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let spin = blank_textured(32.0, 32.0, 16.0, 16.0, [c, s, -s, c, 0.0, 0.0]);
        let p = NodePaint {
            key: 11,
            bounds: [32.0, 32.0, 16.0, 16.0],
            overlay: false,
            instance: (1, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let instances = [card, fill];
        let (without_tex, _) =
            node_present_dest_reset(11, p.bounds, &[p], &instances).expect("fill fallback");
        let (with_tex, _) =
            node_present_dest_reset_scaled(11, p.bounds, &[p], &instances, &[spin], 1.0, None)
                .expect("spin behind card");
        let spun = crate::renderer::aabb_of_centered_transform(
            [32.0, 32.0, 16.0, 16.0],
            &[c, s, -s, c, 0.0, 0.0],
        );
        assert!(
            with_tex[0] <= spun[0] + 0.5,
            "wipe must cover the rotated quad, not only the 16×16 fill\n\
             fill_wipe={without_tex:?} spin_wipe={with_tex:?} spun={spun:?}"
        );
        assert!(with_tex[1] <= spun[1] + 0.5);
        assert!(with_tex[0] + with_tex[2] >= spun[0] + spun[2] - 0.5);
        assert!(with_tex[1] + with_tex[3] >= spun[1] + spun[3] - 0.5);
        assert!(
            with_tex[2] > without_tex[2] + 1.0 || with_tex[3] > without_tex[3] + 1.0,
            "rotated ink must expand the dest-reset beyond the unrotated fill"
        );
    }

    #[test]
    fn dest_reset_spin_must_wipe_circumcircle_at_identity_angle() {
        // 0° ink is the unrotated box. Leftover 45° coverage is last frame's
        // transformed AABB (`prev_bounds`), not this frame's scissor-padded dirty.
        let mut card = blank_instance(0.0, 0.0, 80.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(32.0, 32.0, 16.0, 16.0);
        fill.color = [0.0, 0.0, 0.0, 0.0];
        let box16 = [32.0, 32.0, 16.0, 16.0];
        let p = NodePaint {
            key: 12,
            bounds: box16,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let spun = crate::renderer::aabb_of_centered_transform(box16, &[c, s, -s, c, 0.0, 0.0]);
        let (wipe, _) =
            node_present_dest_reset_scaled(12, box16, &[p], &[card, fill], &[], 1.0, Some(spun))
                .expect("card behind spin");
        assert!(
            wipe[0] <= spun[0] + 0.5
                && wipe[1] <= spun[1] + 0.5
                && wipe[0] + wipe[2] >= spun[0] + spun[2] - 0.5
                && wipe[1] + wipe[3] >= spun[1] + spun[3] - 0.5,
            "0° dest-reset must still cover the 45° circumcircle\n wipe={wipe:?} spun={spun:?}"
        );
        assert!(
            wipe[2] < 80.0 && wipe[3] < 80.0,
            "must not expand to the whole card, got {wipe:?}"
        );
    }

    #[test]
    fn dest_reset_unions_previous_visual_slot() {
        let mut card = blank_instance(0.0, 0.0, 200.0, 200.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let mut icon = blank_instance(32.0, 32.0, 16.0, 16.0);
        icon.color = [0.0, 0.0, 0.0, 0.0];
        let p = NodePaint {
            key: 4,
            bounds: [32.0, 32.0, 16.0, 16.0],
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let prev = [32.0, 112.0, 16.0, 16.0];
        let (rect, _) =
            node_present_dest_reset_scaled(4, p.bounds, &[p], &[card, icon], &[], 1.0, Some(prev))
                .expect("card behind");
        assert!(
            rect[1] <= 32.0 && rect[1] + rect[3] >= 112.0 + 16.0,
            "wipe must cover previous visual Y and now, got {rect:?}"
        );
    }

    #[test]
    fn dest_reset_works_for_transparent_ghost_on_card() {
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [1.0, 1.0, 1.0, 1.0];
        let mut ghost = blank_instance(60.0, 10.0, 40.0, 20.0);
        ghost.color = [0.2, 0.2, 0.2, 0.0];
        let p = NodePaint {
            key: 8,
            bounds: [60.0, 10.0, 40.0, 20.0],
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let instances = [card, ghost];
        assert!(!node_dirty_has_opaque_fill(8, p.bounds, &[p], &instances));
        let (_, color) =
            node_present_dest_reset(8, p.bounds, &[p], &instances).expect("card behind ghost");
        assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
        assert!(node_present_needs_glyphs(8, p.bounds, &[p], &instances));
    }

    #[test]
    fn dest_reset_and_glyphs_for_text_only_bind_label() {
        // Gallery MouseEvent log: bind_text Label has glyphs, no SDF fill.
        // Node-present used to skip capture (needs_glyphs false) so mousemove
        // froze the retain buffer until a FullClear (click / focus).
        let mut card = blank_instance(0.0, 0.0, 400.0, 160.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let bounds = [12.0, 80.0, 376.0, 56.0];
        let label = slot(9, bounds, (0, 1));
        let instances = [card];
        assert!(
            node_dirty_has_text(9, bounds, &[label]),
            "text-only slot must be recognized as having glyphs"
        );
        assert!(
            node_present_needs_glyphs(9, bounds, &[label], &instances),
            "bind_text label with no fill must still capture glyphs"
        );
        let (rect, color) = node_present_dest_reset(9, bounds, &[label], &instances)
            .expect("must wipe the text box with the card behind it");
        assert_eq!(color, [0.94, 0.94, 0.95, 1.0]);
        assert!(rect[0] <= 12.0);
        assert!(rect[1] <= 80.0);
        assert!(rect[0] + rect[2] >= 388.0);
        assert!(rect[1] + rect[3] >= 136.0);
        assert!(rect[2] < 400.0, "must not expand to the whole card");

        // Gallery bind_text used to key dirty by bounds hash, not Taffy id.
        let bounds_keyed = PaintDirty {
            key: 0xDEAD_BEEF,
            bounds,
            kind: PaintDirtyKind::Node,
        };
        let page = [sample_text("mousemove", 20.0)];
        let owned = node_present_glyphs_for_dirty(&bounds_keyed, &page, &[label]);
        assert!(
            owned.contains(&0),
            "same-box dirty must still own the label glyphs without a Taffy key match"
        );
        let (rect2, _) =
            node_present_dest_reset(bounds_keyed.key, bounds_keyed.bounds, &[label], &instances)
                .expect("same-box text-only dirty must dest-reset");
        assert!((rect2[2] - rect[2]).abs() < 1.0);
    }

    #[test]
    fn clipped_capture_keeps_neighbor_caption_out_of_input_dirty() {
        let mut nav = sample_text("NavLayout", 170.0);
        nav.width = 40.0;
        let mut typed = sample_text("asdasd", 240.0);
        typed.width = 80.0;
        let page = [nav.clone(), typed.clone()];
        let dirty_nav = PaintDirty {
            key: 1,
            bounds: [8.0, 0.0, 200.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let dirty_input = PaintDirty {
            key: 2,
            bounds: [208.0, 0.0, 400.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let paints = [
            slot(1, dirty_nav.bounds, (0, 1)),
            slot(2, dirty_input.bounds, (1, 2)),
        ];
        assert!(text_owned_by_node_dirty(&nav, dirty_nav.bounds));
        assert!(!text_owned_by_node_dirty(&nav, dirty_input.bounds));

        let input_owned = node_present_glyphs_for_dirty(&dirty_input, &page, &paints);
        assert!(input_owned.contains(&1));
        assert!(
            !input_owned.contains(&0),
            "Input's own glyph set must not include NavLayout"
        );

        let clipped = node_present_clipped_capture_text(&[dirty_nav, dirty_input], &page, &paints);
        let nav_clip = clipped
            .iter()
            .find(|n| n.text == "NavLayout")
            .and_then(|n| n.clip_bounds)
            .expect("NavLayout stays in the capture with an owner clip");
        assert!(
            nav_clip[0] + nav_clip[2] <= dirty_input.bounds[0] + NODE_PRESENT_GLYPH_CLIP_PAD + 1e-3,
            "NavLayout clip {nav_clip:?} must not extend into Input dirty {:?}",
            dirty_input.bounds
        );
        let typed_clip = clipped
            .iter()
            .find(|n| n.text == "asdasd")
            .and_then(|n| n.clip_bounds)
            .expect("typed text stays in the capture with an owner clip");
        assert!(
            typed_clip[0] + 1e-3 >= dirty_input.bounds[0] - NODE_PRESENT_GLYPH_CLIP_PAD,
            "typed clip {typed_clip:?} must stay in the Input dirty {:?}",
            dirty_input.bounds
        );
    }

    #[test]
    fn node_dirty_owns_only_its_collect_slot_not_center_in_box() {
        // Gallery: padded Input dirty can cover a sidebar caption's center
        // (「布局」 sharing a horizontal band with Kbd Search) without that
        // caption belonging to the Input collect slot.
        let mut caption = sample_text("布局", 190.0);
        caption.width = 40.0; // center x=210, inside Input dirty x=208
        let mut typed = sample_text("asdasd", 240.0);
        typed.width = 80.0;
        let page = [caption, typed];
        let dirty_input = PaintDirty {
            key: 2,
            bounds: [208.0, 0.0, 400.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let paints = [
            slot(1, [8.0, 0.0, 200.0, 32.0], (0, 1)),
            slot(2, [208.0, 0.0, 400.0, 32.0], (1, 2)),
        ];
        assert!(
            text_owned_by_node_dirty(&page[0], dirty_input.bounds),
            "precondition: caption center sits inside the padded Input dirty"
        );
        let owned = node_present_glyphs_for_dirty(&dirty_input, &page, &paints);
        assert!(owned.contains(&1), "Input text must stay owned");
        assert!(
            !owned.contains(&0),
            "sidebar caption must not ride along because its center sits in the Input pad"
        );
        let capture = node_present_capture_text_indices(&[dirty_input], &page, &paints);
        assert!(
            !capture.contains(&0),
            "capture must not include 布局 when only Input is dirty"
        );
    }

    fn aabb_covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
        outer[0] <= inner[0] + 0.5
            && outer[1] <= inner[1] + 0.5
            && outer[0] + outer[2] >= inner[0] + inner[2] - 0.5
            && outer[1] + outer[3] >= inner[1] + inner[3] - 0.5
    }

    fn input_like(x: f32, y: f32, w: f32, h: f32) -> InstanceInput {
        let mut i = blank_instance(x, y, w, h);
        i.color = [0.0, 0.0, 0.0, 0.0];
        i.border_width = 1.0;
        i.border_color = [0.72, 0.72, 0.75, 1.0];
        i.radii = [6.0; 4];
        i
    }

    #[test]
    fn dest_reset_hard_ring_spread_is_dest_ink() {
        // ShadCN Hard ring: blur 0, spread 3, ~50% ring color. Transparent fill.
        let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let box_ = [8.0, 8.0, 220.0, 36.0];
        let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
        focused.border_color = [0.45, 0.35, 0.95, 1.0];
        focused.shadow_blur = 0.0;
        focused.shadow_spread = 3.0;
        focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let p = NodePaint {
            key: 5,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let ring = instance_paint_bounds(&focused);
        assert!(
            ring[0] <= box_[0] - 2.5 && ring[0] + ring[2] >= box_[0] + box_[2] + 2.5,
            "focused paint bounds must include the 3px outset, got {ring:?}"
        );
        let (wipe, _) =
            node_present_dest_reset(5, box_, &[p], &[card, focused]).expect("card behind input");
        assert!(
            aabb_covers(wipe, ring),
            "focused dest-reset must erase the Hard ring\n wipe={wipe:?} ring={ring:?}"
        );
    }

    #[test]
    fn dest_reset_spread_only_ring_without_border_is_still_ink() {
        // Ghost / plain chrome: no fill, no border, Hard ring is the only outset ink.
        let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let box_ = [8.0, 8.0, 220.0, 36.0];
        let mut ring_only = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        ring_only.color = [0.0, 0.0, 0.0, 0.0];
        ring_only.shadow_blur = 0.0;
        ring_only.shadow_spread = 3.0;
        ring_only.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let p = NodePaint {
            key: 6,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let ring = instance_paint_bounds(&ring_only);
        let (wipe, _) =
            node_present_dest_reset(6, box_, &[p], &[card, ring_only]).expect("card behind ring");
        assert!(
            aabb_covers(wipe, ring),
            "spread-only shadow must be dest-ink so unfocus can erase it\n wipe={wipe:?} ring={ring:?}"
        );
    }

    #[test]
    fn dest_reset_unfocus_prev_must_cover_hard_ring() {
        let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let box_ = [8.0, 8.0, 220.0, 36.0];
        let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
        focused.border_color = [0.45, 0.35, 0.95, 1.0];
        focused.shadow_blur = 0.0;
        focused.shadow_spread = 3.0;
        focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let mut unfocused = input_like(box_[0], box_[1], box_[2], box_[3]);
        unfocused.shadow_blur = 1.0;
        unfocused.shadow_spread = 0.0;
        unfocused.shadow_color = [0.0, 0.0, 0.0, 0.08];
        let p = NodePaint {
            key: 7,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let ring = instance_paint_bounds(&focused);
        let prev = node_paint_visual_bounds(&p, &[card, focused], &[], &[], &[]);
        let (wipe, _) =
            node_present_dest_reset_scaled(7, box_, &[p], &[card, unfocused], &[], 1.0, Some(prev))
                .expect("card behind unfocused input");
        assert!(
            aabb_covers(wipe, ring),
            "unfocus dest-reset must union last-present ring AABB\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
    }

    #[test]
    fn dest_reset_unfocus_high_dpi_prev_must_cover_scaled_hard_ring() {
        // RESET_PAD is 3 physical px. Hard ring is 3 logical px = 6 physical at 200%.
        // Unfocused current ink (blur 1 logical) + pad cannot cover it; prev must.
        let s = 2.0_f32;
        let mut card = blank_instance(0.0, 0.0, 240.0 * s, 80.0 * s);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let box_ = [8.0, 8.0, 220.0, 36.0];
        let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
        focused.border_color = [0.45, 0.35, 0.95, 1.0];
        focused.shadow_blur = 0.0;
        focused.shadow_spread = 3.0;
        focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let mut unfocused = input_like(box_[0] * s, box_[1] * s, box_[2] * s, box_[3] * s);
        unfocused.shadow_blur = 1.0 * s;
        unfocused.shadow_spread = 0.0;
        unfocused.shadow_color = [0.0, 0.0, 0.0, 0.08];
        unfocused.border_width = 1.0 * s;
        let p = NodePaint {
            key: 8,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let ring_logical = instance_paint_bounds(&focused);
        let ring = scale_xywh(ring_logical, s);
        let prev = node_paint_visual_bounds(
            &p,
            &[blank_instance(0.0, 0.0, 240.0, 80.0), focused],
            &[],
            &[],
            &[],
        );
        let (wipe, _) =
            node_present_dest_reset_scaled(8, box_, &[p], &[card, unfocused], &[], s, Some(prev))
                .expect("card behind 2x unfocused input");
        assert!(
            aabb_covers(wipe, ring),
            "2x unfocus dest-reset must still erase the 6px physical ring\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
        let (wipe_no_prev, _) =
            node_present_dest_reset_scaled(8, box_, &[p], &[card, unfocused], &[], s, None)
                .expect("card behind 2x unfocused input without prev");
        assert!(
            aabb_covers(wipe_no_prev, ring),
            "2x unfocus without last-present must still cover the Hard ring\n wipe={wipe_no_prev:?} ring={ring:?}"
        );
        assert!(
            node_present_needs_glyphs_scaled(8, box_, &[p], &[card, unfocused], &[], s, Some(prev)),
            "wipe of the field must recapture glyphs (placeholder / value)"
        );
    }

    #[test]
    fn dest_reset_input_parent_slot_must_capture_child_text() {
        // Input container dirty; glyphs live on the text child inside the parent range.
        let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let input_box = [8.0, 8.0, 380.0, 36.0];
        let mut field = input_like(input_box[0], input_box[1], input_box[2], input_box[3]);
        field.color = [1.0, 1.0, 1.0, 1.0];
        let parent = NodePaint {
            key: 10,
            bounds: input_box,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let child = NodePaint {
            key: 11,
            bounds: [16.0, 14.0, 80.0, 20.0],
            overlay: false,
            instance: (2, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let mut typed = sample_text("asdasd", 16.0);
        typed.y = 14.0;
        typed.width = 80.0;
        typed.height = 20.0;
        let dirty = PaintDirty {
            key: 10,
            bounds: [
                input_box[0] - 8.0,
                input_box[1] - 8.0,
                input_box[2] + 16.0,
                input_box[3] + 16.0,
            ],
            kind: PaintDirtyKind::Node,
        };
        let paints = [parent, child];
        let owned = node_present_glyphs_for_dirty(&dirty, &[typed.clone()], &paints);
        assert!(
            owned.contains(&0),
            "Input container dirty must own the child text run"
        );
        assert!(
            node_present_needs_glyphs(10, dirty.bounds, &paints, &[card, field]),
            "dest-reset of the field must recapture its glyphs"
        );
        let (wipe, color) =
            node_present_dest_reset(10, dirty.bounds, &paints, &[card, field]).expect("wipe");
        assert_eq!(color, [0.94, 0.94, 0.95, 1.0]);
        assert!(
            aabb_covers(wipe, [16.0, 14.0, 80.0, 20.0]),
            "wipe must cover the text box so dest-reset cannot leave a gray glyph hole\n wipe={wipe:?}"
        );
    }

    #[test]
    fn dest_reset_outline_remove_must_union_prev() {
        // CSS outline paints outside the layout box. Collect skip now busts on
        // outline color/width (Wave 12); dest-reset of *now* only leaves the ring.
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let outline_w = 8.0_f32;
        let mut outline = blank_instance(
            box_[0] - outline_w,
            box_[1] - outline_w,
            box_[2] + outline_w * 2.0,
            box_[3] + outline_w * 2.0,
        );
        outline.color = [0.0, 0.0, 0.0, 0.0];
        outline.border_width = outline_w;
        outline.border_color = [0.0, 0.0, 1.0, 1.0];
        let p_on = NodePaint {
            key: 21,
            bounds: box_,
            overlay: false,
            instance: (1, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let p_off = NodePaint {
            instance: (1, 2),
            ..p_on
        };
        let ring = instance_paint_bounds(&outline);
        let prev = node_paint_visual_bounds(&p_on, &[card, fill, outline], &[], &[], &[]);
        let (wipe, _) =
            node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, Some(prev))
                .expect("card behind after outline off");
        assert!(
            aabb_covers(wipe, ring),
            "removing outline must dest-reset the previous ring\n wipe={wipe:?} ring={ring:?} prev={prev:?}"
        );
        let (wipe_no_prev, _) =
            node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, None)
                .expect("wipe without last-present");
        assert!(
            !aabb_covers(wipe_no_prev, ring),
            "3px reset pad on the fill cannot cover an 8px outline; prev visual is required\n wipe={wipe_no_prev:?} ring={ring:?}"
        );
    }

    #[test]
    fn dest_reset_extra_shadow_remove_must_union_prev() {
        // Extra-shadow is its own instance (Wave 11). Dropping it must union last-present.
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let mut shadow = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        shadow.color = [0.0, 0.0, 0.0, 0.0];
        shadow.shadow_color = [0.0, 0.0, 0.0, 0.8];
        shadow.shadow_offset = [12.0, 12.0];
        shadow.shadow_blur = 4.0;
        let p_on = NodePaint {
            key: 22,
            bounds: box_,
            overlay: false,
            instance: (1, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let p_off = NodePaint {
            instance: (1, 2),
            ..p_on
        };
        let blob = instance_paint_bounds(&shadow);
        let prev = node_paint_visual_bounds(&p_on, &[card, fill, shadow], &[], &[], &[]);
        let (wipe, _) =
            node_present_dest_reset_scaled(22, box_, &[p_off], &[card, fill], &[], 1.0, Some(prev))
                .expect("card behind after extra-shadow off");
        assert!(
            aabb_covers(wipe, blob),
            "removing extra-shadow must dest-reset the previous outset\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
        let (wipe_no_prev, _) =
            node_present_dest_reset_scaled(22, box_, &[p_off], &[card, fill], &[], 1.0, None)
                .expect("wipe without last-present");
        assert!(
            !aabb_covers(wipe_no_prev, blob),
            "fill + 3px pad cannot cover offset 12 / blur 4 extra-shadow; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
    }

    #[test]
    fn dest_reset_filter_wrap_shrink_must_union_prev() {
        // CSS filter capture is a textured wrap larger than the layout box.
        // Shrinking blur must dest-reset the previous outset (Wave 11 collect + Wave 13 present).
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let big = blank_textured(8.0, 8.0, 64.0, 44.0, MAT2D_IDENTITY);
        let small = blank_textured(16.0, 16.0, 48.0, 28.0, MAT2D_IDENTITY);
        let p_big = NodePaint {
            key: 23,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let p_small = NodePaint {
            textured: (0, 1),
            ..p_big
        };
        let prev = node_paint_visual_bounds(&p_big, &[card, fill], &[big.clone()], &[], &[]);
        let blob = textured_paint_bounds(&big);
        let (wipe, _) = node_present_dest_reset_scaled(
            23,
            box_,
            &[p_small],
            &[card, fill],
            &[small.clone()],
            1.0,
            Some(prev),
        )
        .expect("card behind after filter shrink");
        assert!(
            aabb_covers(wipe, blob),
            "shrinking filter wrap must dest-reset the previous capture AABB\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
        let (wipe_no_prev, _) = node_present_dest_reset_scaled(
            23,
            box_,
            &[p_small],
            &[card, fill],
            &[small],
            1.0,
            None,
        )
        .expect("wipe without last-present");
        assert!(
            !aabb_covers(wipe_no_prev, blob),
            "current small wrap + 3px pad cannot cover the previous filter outset; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
    }

    #[test]
    fn dest_reset_text_shadow_shrink_must_union_prev() {
        // Glyph ink is not dest-ink (3px pad would eat a 1px parent border).
        // Last-present visual must still cover text-shadow outset.
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let mut shadowed = sample_text("hi", box_[0]);
        shadowed.y = box_[1];
        shadowed.width = box_[2];
        shadowed.height = box_[3];
        shadowed.style.shadow_offset = [12.0, 12.0];
        shadowed.style.shadow_blur = 4.0;
        let p = NodePaint {
            key: 24,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let blob = text_paint_bounds(&shadowed);
        let prev = node_paint_visual_bounds(&p, &[card, fill], &[], &[shadowed], &[]);
        let (wipe, _) =
            node_present_dest_reset_scaled(24, box_, &[p], &[card, fill], &[], 1.0, Some(prev))
                .expect("card behind after text-shadow shrink");
        assert!(
            aabb_covers(wipe, blob),
            "shrinking text-shadow must dest-reset the previous glyph outset\n wipe={wipe:?} blob={blob:?} prev={prev:?}"
        );
        let (wipe_no_prev, _) =
            node_present_dest_reset_scaled(24, box_, &[p], &[card, fill], &[], 1.0, None)
                .expect("wipe without last-present");
        assert!(
            !aabb_covers(wipe_no_prev, blob),
            "fill + 3px pad cannot cover offset 12 / blur 4 text-shadow; prev visual is required\n wipe={wipe_no_prev:?} blob={blob:?}"
        );
    }

    fn rect_contains_point(r: [f32; 4], x: f32, y: f32) -> bool {
        x >= r[0] && y >= r[1] && x < r[0] + r[2] && y < r[1] + r[3]
    }

    #[test]
    fn dest_reset_overflowing_glyphs_must_not_wipe_header_or_sibling() {
        // Gallery Marker: unclipped "Generating response…" / shimmer-box last-present
        // grew to the glyph AABB and dest-reset punched the sticky title and the
        // next column. This node only recaptures its own slot.
        let mut page = blank_instance(0.0, 0.0, 400.0, 200.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let header = [0.0, 0.0, 400.0, 32.0];
        let mut header_fill = blank_instance(header[0], header[1], header[2], header[3]);
        header_fill.color = [1.0, 1.0, 1.0, 1.0];
        let sibling = [220.0, 40.0, 160.0, 24.0];
        let mut sibling_fill = blank_instance(sibling[0], sibling[1], sibling[2], sibling[3]);
        sibling_fill.color = [0.2, 0.4, 0.8, 1.0];
        let label_box = [8.0, 40.0, 80.0, 20.0];
        let mut overflow = sample_text("Generating response…", 8.0);
        overflow.y = 16.0;
        overflow.width = 360.0;
        overflow.height = 48.0;
        overflow.clip_bounds = None;
        let p = NodePaint {
            key: 31,
            bounds: label_box,
            overlay: false,
            instance: (3, 3),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let instances = [page, header_fill, sibling_fill];
        let prev = node_paint_visual_bounds(&p, &instances, &[], &[overflow], &[]);
        assert!(
            !rect_contains_point(prev, 200.0, 16.0),
            "last-present must not follow unclipped overflow into the header\n prev={prev:?} slot={label_box:?}"
        );
        assert!(
            !rect_contains_point(prev, sibling[0] + 8.0, sibling[1] + 8.0),
            "last-present must not follow unclipped overflow into a sibling column\n prev={prev:?} sibling={sibling:?}"
        );
        let (wipe, _) =
            node_present_dest_reset_scaled(31, label_box, &[p], &instances, &[], 1.0, Some(prev))
                .expect("page behind overflowing label");
        assert!(
            !rect_contains_point(wipe, 200.0, 16.0),
            "overflow dest-reset must not punch the page header\n wipe={wipe:?} header={header:?} prev={prev:?}"
        );
        assert!(
            !rect_contains_point(wipe, sibling[0] + 8.0, sibling[1] + 8.0),
            "overflow dest-reset must not punch a sibling column\n wipe={wipe:?} sibling={sibling:?} prev={prev:?}"
        );
        assert!(
            wipe[2] < 160.0 && wipe[3] < 80.0,
            "wipe must stay near the label slot, not a page band\n wipe={wipe:?}"
        );
    }

    #[test]
    fn dest_reset_recycled_prev_must_not_punch_title_or_sibling() {
        // Gallery: NodeId recycle / last-present of a parent slot made prev the
        // whole inset. Dest-reset unioned that AABB and emptied the title bar
        // and sibling Marker columns while this leaf's glyphs redrew.
        let mut page = blank_instance(0.0, 0.0, 400.0, 208.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let title = [0.0, 0.0, 400.0, 48.0];
        let body_clip = [0.0, 48.0, 400.0, 160.0];
        let sibling = [220.0, 56.0, 160.0, 80.0];
        let label_box = [8.0, 56.0, 80.0, 20.0];
        let mut fill = blank_instance(label_box[0], label_box[1], label_box[2], label_box[3]);
        fill.color = [0.9, 0.9, 0.92, 1.0];
        let p = NodePaint {
            key: 41,
            bounds: label_box,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(body_clip),
        };
        let huge_prev = [0.0, 0.0, 400.0, 208.0];
        let (wipe, _) = node_present_dest_reset_scaled(
            41,
            label_box,
            &[p],
            &[page, fill],
            &[],
            1.0,
            Some(huge_prev),
        )
        .expect("page behind recycled prev");
        assert!(
            !rect_contains_point(wipe, title[0] + 40.0, title[1] + 24.0),
            "recycled last-present must not dest-reset the title\n wipe={wipe:?} title={title:?}"
        );
        assert!(
            !rect_contains_point(wipe, sibling[0] + 8.0, sibling[1] + 8.0),
            "recycled last-present must not dest-reset a sibling column\n wipe={wipe:?} sibling={sibling:?}"
        );
        assert!(
            wipe[2] < 200.0 && wipe[3] < 120.0,
            "wipe must stay near the leaf, not the inset\n wipe={wipe:?}"
        );
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            41,
            label_box,
            &[(41, huge_prev)],
            &[p],
            8.0,
        );
        assert!(
            !rect_contains_point(scissor, title[0] + 40.0, title[1] + 24.0),
            "node-present scissor must not cover the title\n scissor={scissor:?} title={title:?}"
        );
        // Title bottom sits 4px above the ScrollArea. Pad-after-overflow without
        // re-intersect leaks 8px into that strip; the helper must not.
        assert!(
            !rect_contains_point(scissor, title[0] + 40.0, title[1] + title[3] - 4.0),
            "scissor pad must not leak past overflow clip into the title bottom\n scissor={scissor:?} title={title:?}"
        );
    }

    #[test]
    fn overlay_restore_scissor_is_visual_occupancy() {
        assert_eq!(
            node_present_scissor_pad(PaintDirtyKind::OverlayRestore),
            0.0,
            "Restore dest-reset is already visual occupancy; extra pad leaves a white frame the overlay fill cannot cover"
        );
    }

    #[test]
    fn node_present_scissor_pad_must_not_leak_past_overflow_clip() {
        let title = [0.0, 0.0, 400.0, 48.0];
        let body_clip = [0.0, 48.0, 400.0, 160.0];
        let label_box = [8.0, 56.0, 80.0, 20.0];
        let p = NodePaint {
            key: 41,
            bounds: label_box,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(body_clip),
        };
        let huge_prev = [0.0, 0.0, 400.0, 208.0];
        let clipped = intersect_rects(
            union_xywh(
                label_box,
                clamp_last_present_prev(huge_prev, label_box).unwrap_or(label_box),
            ),
            body_clip,
        )
        .expect("prev ∩ body");
        let leaked = padded_xywh(clipped, 8.0);
        assert!(
            rect_contains_point(leaked, title[0] + 40.0, title[1] + title[3] - 4.0),
            "precondition: pad after overflow without re-intersect leaks into the title\n leaked={leaked:?}"
        );
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            41,
            label_box,
            &[(41, huge_prev)],
            &[p],
            8.0,
        );
        assert!(
            !rect_contains_point(scissor, title[0] + 40.0, title[1] + title[3] - 4.0),
            "product scissor must re-intersect overflow after pad\n scissor={scissor:?} leaked={leaked:?}"
        );
        assert!(
            rect_contains_point(scissor, label_box[0] + 4.0, label_box[1] + 4.0),
            "scissor must still cover the dirty leaf\n scissor={scissor:?} slot={label_box:?}"
        );
    }

    fn outline_remove_fixture() -> (
        InstanceInput,
        InstanceInput,
        InstanceInput,
        NodePaint,
        NodePaint,
        [f32; 4],
        [f32; 4],
    ) {
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let outline_w = 8.0_f32;
        let mut outline = blank_instance(
            box_[0] - outline_w,
            box_[1] - outline_w,
            box_[2] + outline_w * 2.0,
            box_[3] + outline_w * 2.0,
        );
        outline.color = [0.0, 0.0, 0.0, 0.0];
        outline.border_width = outline_w;
        outline.border_color = [0.0, 0.0, 1.0, 1.0];
        let p_on = NodePaint {
            key: 21,
            bounds: box_,
            overlay: false,
            instance: (1, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let p_off = NodePaint {
            instance: (1, 2),
            ..p_on
        };
        let ring = instance_paint_bounds(&outline);
        (card, fill, outline, p_on, p_off, box_, ring)
    }

    #[test]
    fn last_present_after_instance_take_drops_outline_outset() {
        // Product path used to record last-present *after* take: instances empty,
        // visual collapsed to the layout box, dest-reset could not cover the ring.
        let (card, fill, outline, p_on, p_off, box_, ring) = outline_remove_fixture();
        let mut instances = vec![card, fill, outline];
        let before = last_present_visual_slots(&[p_on], &instances, &[], &[], &[]);
        let _taken = std::mem::take(&mut instances);
        let after = last_present_visual_slots(&[p_on], &instances, &[], &[], &[]);
        let prev_before = before.iter().find(|(k, _)| *k == 21).map(|(_, b)| *b);
        let prev_after = after.iter().find(|(k, _)| *k == 21).map(|(_, b)| *b);
        assert!(
            aabb_covers(prev_before.expect("slot before take"), ring),
            "logical last-present must cover outline outset before take"
        );
        assert!(
            !aabb_covers(prev_after.expect("slot after take"), ring),
            "recording after take drops instance outset; product path must snapshot first\n after={prev_after:?} ring={ring:?}"
        );
        let (wipe_before, _) = node_present_dest_reset_scaled(
            21,
            box_,
            &[p_off],
            &[card, fill],
            &[],
            1.0,
            prev_before,
        )
        .expect("card behind");
        assert!(
            aabb_covers(wipe_before, ring),
            "prev recorded before take must dest-reset the ring"
        );
        let (wipe_after, _) =
            node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], 1.0, prev_after)
                .expect("card behind after-take prev");
        assert!(
            !aabb_covers(wipe_after, ring),
            "prev recorded after take cannot dest-reset the ring\n wipe={wipe_after:?} ring={ring:?}"
        );
    }

    #[test]
    fn last_present_prev_stays_logical_at_2x() {
        let (card, fill, outline, p_on, p_off, box_, ring) = outline_remove_fixture();
        let instances = [card, fill, outline];
        let prev = last_present_visual_slots(&[p_on], &instances, &[], &[], &[])
            .into_iter()
            .find(|(k, _)| *k == 21)
            .map(|(_, b)| b)
            .expect("slot");
        let s = 2.0_f32;
        let ring_phys = scale_xywh(ring, s);
        let (wipe, _) =
            node_present_dest_reset_scaled(21, box_, &[p_off], &[card, fill], &[], s, Some(prev))
                .expect("2x dest-reset");
        assert!(
            aabb_covers(wipe, ring_phys),
            "logical prev × ui_scale must cover the physical ring\n wipe={wipe:?} ring={ring_phys:?} prev={prev:?}"
        );
        let (wipe_double, _) = node_present_dest_reset_scaled(
            21,
            box_,
            &[p_off],
            &[card, fill],
            &[],
            s,
            Some(scale_xywh(prev, s)),
        )
        .expect("double-scaled prev");
        assert!(
            wipe_double[2] > wipe[2] * 1.4,
            "recording after scale_instances_in_place would double-scale prev and over-wipe\n logical_wipe={wipe:?} doubled={wipe_double:?}"
        );
    }

    #[test]
    fn last_present_before_take_covers_filter_wrap_and_text_shadow() {
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let big = blank_textured(8.0, 8.0, 64.0, 44.0, MAT2D_IDENTITY);
        let p_tex = NodePaint {
            key: 23,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let mut tex = vec![big.clone()];
        let before = last_present_visual_slots(&[p_tex], &[card, fill], &tex, &[], &[]);
        let _ = std::mem::take(&mut tex);
        let after = last_present_visual_slots(&[p_tex], &[card, fill], &tex, &[], &[]);
        let blob = textured_paint_bounds(&big);
        let prev_before = before.iter().find(|(k, _)| *k == 23).map(|(_, b)| *b);
        let prev_after = after.iter().find(|(k, _)| *k == 23).map(|(_, b)| *b);
        assert!(aabb_covers(prev_before.expect("tex before"), blob));
        assert!(
            !aabb_covers(prev_after.expect("tex after take"), blob),
            "taking textured draws must drop filter-wrap outset from last-present"
        );

        let mut shadowed = sample_text("hi", box_[0]);
        shadowed.y = box_[1];
        shadowed.width = box_[2];
        shadowed.height = box_[3];
        shadowed.style.shadow_offset = [12.0, 12.0];
        shadowed.style.shadow_blur = 4.0;
        let p_text = NodePaint {
            key: 24,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let glyph = text_paint_bounds(&shadowed);
        let prev_text = last_present_visual_slots(&[p_text], &[card, fill], &[], &[shadowed], &[])
            .into_iter()
            .find(|(k, _)| *k == 24)
            .map(|(_, b)| b)
            .expect("text slot");
        assert!(
            aabb_covers(prev_text, glyph),
            "last-present recorded with texts must cover text-shadow outset\n prev={prev_text:?} glyph={glyph:?}"
        );
        let (wipe, _) = node_present_dest_reset_scaled(
            24,
            box_,
            &[p_text],
            &[card, fill],
            &[],
            1.0,
            Some(prev_text),
        )
        .expect("card behind text-shadow");
        assert!(aabb_covers(wipe, glyph));
    }

    #[test]
    fn last_present_before_take_covers_input_chrome() {
        // Caret lives in `input_chrome_instances`, outside fill + 3px RESET_PAD.
        // Dest-reset of *now* still skips chrome as dest-ink (pad vs ⌘K).
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let mut caret = blank_instance(box_[0] + box_[2] + 8.0, box_[1], 2.0, 16.0);
        caret.color = [0.1, 0.1, 0.1, 1.0];
        let p_on = NodePaint {
            key: 41,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 1),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let p_off = NodePaint {
            chrome: (0, 0),
            ..p_on
        };
        let caret_box = instance_paint_bounds(&caret);
        let mut chrome = vec![caret];
        let before = last_present_visual_slots(&[p_on], &[card, fill], &[], &[], &chrome);
        let _ = std::mem::take(&mut chrome);
        let after = last_present_visual_slots(&[p_on], &[card, fill], &[], &[], &chrome);
        let prev_before = before.iter().find(|(k, _)| *k == 41).map(|(_, b)| *b);
        let prev_after = after.iter().find(|(k, _)| *k == 41).map(|(_, b)| *b);
        assert!(
            aabb_covers(prev_before.expect("chrome before take"), caret_box),
            "logical last-present must cover caret chrome before take"
        );
        assert!(
            !aabb_covers(prev_after.expect("chrome after take"), caret_box),
            "recording after take of chrome drops caret AABB\n after={prev_after:?} caret={caret_box:?}"
        );
        let (wipe_before, _) = node_present_dest_reset_scaled(
            41,
            box_,
            &[p_off],
            &[card, fill],
            &[],
            1.0,
            prev_before,
        )
        .expect("card behind");
        assert!(
            aabb_covers(wipe_before, caret_box),
            "prev recorded before chrome take must dest-reset the caret"
        );
        let (wipe_after, _) =
            node_present_dest_reset_scaled(41, box_, &[p_off], &[card, fill], &[], 1.0, prev_after)
                .expect("card behind after-take prev");
        assert!(
            !aabb_covers(wipe_after, caret_box),
            "prev recorded after chrome take cannot dest-reset the caret\n wipe={wipe_after:?} caret={caret_box:?}"
        );
        let (wipe_now, _) =
            node_present_dest_reset_scaled(41, box_, &[p_on], &[card, fill], &[], 1.0, None)
                .expect("now without prev");
        assert!(
            !aabb_covers(wipe_now, caret_box),
            "dest-reset of *now* must not treat chrome as dest-ink\n wipe={wipe_now:?} caret={caret_box:?}"
        );
    }

    #[test]
    fn last_present_prev_for_dirty_skips_overlay_kinds() {
        // Overlay dest-reset scissors its hole; unioning a page last-present AABB
        // would punch the page behind the overlay.
        let page = [10.0, 20.0, 30.0, 40.0];
        let slots = [(50u64, page)];
        assert_eq!(
            last_present_prev_for_dirty(PaintDirtyKind::Node, 50, &slots),
            Some(page)
        );
        assert_eq!(
            last_present_prev_for_dirty(PaintDirtyKind::Node, 99, &slots),
            None
        );
        assert_eq!(
            last_present_prev_for_dirty(PaintDirtyKind::Overlay, 50, &slots),
            None
        );
        assert_eq!(
            last_present_prev_for_dirty(PaintDirtyKind::OverlayRestore, 50, &slots),
            None
        );
    }

    #[test]
    fn named_scroll_blit_pins_union_clamped_prev() {
        let dirty = PaintDirty {
            key: 7,
            bounds: [100.0, 10.0, 40.0, 20.0],
            kind: PaintDirtyKind::Node,
        };
        let prev = [(7u64, [10.0, 10.0, 40.0, 20.0])];
        let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
        assert_eq!(pins.len(), 1);
        let p = pins[0];
        assert!(
            p[0] <= 2.0 && p[0] + p[2] >= 148.0,
            "scroll blit pin must dest-reset old hover ∪ new, got {p:?}"
        );
        let now_only = named_scroll_blit_pins(&[dirty], &[], &[]);
        assert!(
            now_only[0][2] < p[2] - 40.0,
            "without prev the pin must be the now box, got now={:?} with_prev={p:?}",
            now_only[0]
        );
    }

    #[test]
    fn named_scroll_blit_pins_overlay_skips_page_prev() {
        let dirty = PaintDirty {
            key: 7,
            bounds: [200.0, 200.0, 80.0, 40.0],
            kind: PaintDirtyKind::Overlay,
        };
        let prev = [(7u64, [0.0, 0.0, 800.0, 600.0])];
        let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
        assert_eq!(pins.len(), 1);
        let p = pins[0];
        assert!(
            p[2] < 200.0 && p[3] < 120.0,
            "overlay pin must not inherit a page last-present AABB, got {p:?}"
        );
    }

    #[test]
    fn named_scroll_blit_pins_clamps_recycled_prev() {
        let dirty = PaintDirty {
            key: 1,
            bounds: [8.0, 80.0, 32.0, 16.0],
            kind: PaintDirtyKind::Node,
        };
        let prev = [(1u64, [0.0, 0.0, 800.0, 600.0])];
        let pins = named_scroll_blit_pins(&[dirty], &prev, &[]);
        assert_eq!(pins.len(), 1);
        let p = pins[0];
        assert!(
            p[2] * p[3] < 200.0 * 200.0,
            "recycled inset prev must clamp to the dirty neighborhood, got {p:?}"
        );
    }

    #[test]
    fn last_present_visual_slots_omit_overlay_nodes() {
        let mut card = blank_instance(0.0, 0.0, 200.0, 80.0);
        card.color = [0.96, 0.96, 0.96, 1.0];
        let box_ = [20.0, 20.0, 40.0, 20.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [1.0, 0.0, 0.0, 1.0];
        let page = NodePaint {
            key: 60,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
        };
        let overlay = NodePaint {
            key: 61,
            overlay: true,
            instance: (0, 0),
            ..page
        };
        let slots = last_present_visual_slots(&[page, overlay], &[card, fill], &[], &[], &[]);
        assert!(
            slots.iter().any(|(k, _)| *k == 60),
            "page nodes stay in last-present"
        );
        assert!(
            slots.iter().all(|(k, _)| *k != 61),
            "overlay nodes must not enter last-present (dest-reset does not use overlay prev)"
        );
    }
}
