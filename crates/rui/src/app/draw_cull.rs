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

/// Content box ∪ drop-shadow (outset only). Coordinates match `rect_pos` space.
pub(crate) fn instance_paint_bounds(i: &InstanceInput) -> [f32; 4] {
    let mut bounds = [i.rect_pos[0], i.rect_pos[1], i.rect_size[0], i.rect_size[1]];
    if i.shadow_inset == 0.0 {
        let extra = i.shadow_blur.max(0.0) + i.shadow_spread.abs();
        if extra > 0.0 || i.shadow_offset[0] != 0.0 || i.shadow_offset[1] != 0.0 {
            let sx = i.rect_pos[0] + i.shadow_offset[0] - extra;
            let sy = i.rect_pos[1] + i.shadow_offset[1] - extra;
            let sw = i.rect_size[0] + extra * 2.0;
            let sh = i.rect_size[1] + extra * 2.0;
            bounds = union_xywh(bounds, [sx, sy, sw, sh]);
        }
    }
    if !transform_is_identity(&i.transform) {
        bounds = aabb_of_centered_transform(bounds, &i.transform);
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
    if d.clip_size[0] >= 0.0 && d.clip_size[1] >= 0.0 {
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
    if let Some(clip) = n.clip_bounds {
        match intersect_rects(bounds, clip) {
            Some(hit) => bounds = hit,
            None => return [0.0, 0.0, 0.0, 0.0],
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

/// Contiguous instance ranges inside `start..start+count` that hit `damage`.
pub(crate) fn visible_instance_runs(
    instances: &[InstanceInput],
    start: u32,
    count: u32,
    damage: [f32; 4],
) -> Vec<(u32, u32)> {
    let start = start as usize;
    let end = (start + count as usize).min(instances.len());
    let start = start.min(end);
    let mut runs = Vec::new();
    let mut run_start: Option<usize> = None;
    for i in start..end {
        if instance_hits_damage(&instances[i], damage) {
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
        .filter_map(|n| {
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
        })
        .collect()
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
    scroll_clip: Option<[f32; 4]>,
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
        let clipped = clip_text_nodes_to_rect(slice, clip);
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
    scroll_clip: Option<[f32; 4]>,
    partial_load: bool,
) -> bool {
    page_stack_band_submit_text(prev, curr, lo, hi, scroll_clip, partial_load).is_some()
}

/// Compact chrome / underline / caret uploads to hits only.
pub(crate) fn filter_instances(
    instances: &[InstanceInput],
    damage: [f32; 4],
) -> Vec<InstanceInput> {
    instances
        .iter()
        .copied()
        .filter(|i| instance_hits_damage(i, damage))
        .collect()
}

/// Opaque enough that src-over replaces dest (covers retain glyphs).
pub(crate) fn instance_fill_opaque(i: &InstanceInput) -> bool {
    i.color[3] > 0.95
}

/// Opaque fill, visible border, or drop-shadow — pixels dest-reset must erase.
fn instance_has_dest_ink(i: &InstanceInput) -> bool {
    instance_fill_opaque(i)
        || (i.border_width > 0.0 && i.border_color[3] > 0.05)
        || (i.shadow_blur > 0.0 && i.shadow_color[3] > 0.05)
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
            let b = instance_paint_bounds(inst);
            if b[2] <= 0.0 || b[3] <= 0.0 {
                continue;
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
    let mut acc = ink.or(slot_visual);
    if let Some(prev) = prev_bounds {
        if prev[2] > 0.0 && prev[3] > 0.0 {
            let pb = scale_xywh(prev, s);
            acc = Some(match acc {
                Some(now) => union_xywh(now, pb),
                None => pb,
            });
        }
    }
    // SDF AA fringe around real ink. Transparent / text-only slots must not grow
    // by RESET_PAD — that 3px eats a 1px parent border (Kbd InputAdornment).
    let wipe_pad = if ink.is_some() { pad.max(0.0) } else { 0.0 };
    acc.map(|r| padded_xywh(r, wipe_pad))
}

/// Collect-slot visual box plus this frame's transformed instance / texture ink.
pub(crate) fn node_paint_visual_bounds(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
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
    acc.unwrap_or(p.bounds)
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
        PaintDirtyKind::Node => {
            for (i, n) in page_text.iter().enumerate() {
                if glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints) {
                    idx.insert(i);
                }
            }
        }
        PaintDirtyKind::Overlay => {}
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
        PaintDirtyKind::Overlay => false,
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
    node_present_needs_glyphs_scaled(dirty_key, dirty_bounds, node_paints, instances, 1.0)
}

pub(crate) fn node_present_needs_glyphs_scaled(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> bool {
    node_present_dest_reset_scaled(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        &[],
        ui_scale,
        None,
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
        let runs = visible_instance_runs(&[a, b], 0, 2, damage);
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
    fn middle_gap_splits_runs() {
        let inst = [
            blank_instance(0.0, 0.0, 10.0, 10.0),
            blank_instance(200.0, 0.0, 10.0, 10.0),
            blank_instance(8.0, 0.0, 10.0, 10.0),
        ];
        let damage = [0.0, 0.0, 30.0, 30.0];
        assert_eq!(
            visible_instance_runs(&inst, 0, 3, damage),
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
            !page_stack_band_needs_glyphon(&prev, &curr, 0, 2, Some(ms), true),
            "inner follow Clear must not submit the rail band"
        );
        assert!(
            page_stack_band_needs_glyphon(&prev, &curr, 2, 4, Some(ms), true),
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
            [0.0, 0.0, 40.0, 40.0],
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
}
