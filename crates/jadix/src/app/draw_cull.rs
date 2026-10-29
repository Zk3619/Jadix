//! Cull collect buffers to a damage AABB before GPU encode (B8).
//!
//! Only valid with retain `LoadOp::Load` + scissor: FullClear to a discarded
//! swapchain would leave holes. Scroll / structure still FullClear (caller).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};

use crate::layout::layout_tree::{NodePaint, TextLayoutInfo};
use crate::renderer::{
    aabb_of_centered_transform, aabbs_disjoint, InstanceInput, ScaledTexturedDraw,
};
use crate::style::transform::MAT2D_IDENTITY;

use crate::app::damage::{intersect_rects, PaintDirty, PaintDirtyKind};

pub(crate) fn slot_paint_ops(
    ops: &[crate::layout::layout_tree::PaintOp],
    instances: (usize, usize),
    textures: (usize, usize),
) -> impl Iterator<Item = crate::layout::layout_tree::PaintOp> + '_ {
    use crate::layout::layout_tree::PaintOp;
    ops.iter().filter_map(move |op| {
        let (start, count, range) = match *op {
            PaintOp::Rects { start, count } => (start, count, instances),
            PaintOp::Textured { start, count } => (start, count, textures),
        };
        let lo = (start as usize).max(range.0);
        let hi = (start as usize + count as usize).min(range.1);
        if lo >= hi {
            return None;
        }
        Some(match op {
            PaintOp::Rects { .. } => PaintOp::Rects {
                start: lo as u32,
                count: (hi - lo) as u32,
            },
            PaintOp::Textured { .. } => PaintOp::Textured {
                start: lo as u32,
                count: (hi - lo) as u32,
            },
        })
    })
}

pub(crate) fn region_own_paint_ops(
    ops: &[crate::layout::layout_tree::PaintOp],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    texture_count: usize,
    bounds: [f32; 4],
    ui_scale: f32,
) -> Vec<crate::layout::layout_tree::PaintOp> {
    use crate::layout::layout_tree::PaintOp;
    let mut rectangles = Vec::new();
    let mut textures = Vec::new();
    let bounds = padded_xywh(bounds, 2.0 / ui_scale.max(0.001));
    for slot in node_paints {
        if slot.overlay || intersect_rects(slot.bounds, bounds).is_none() {
            continue;
        }
        rectangles.push(slot_own_chrome_range(slot, instances, ui_scale));
        for_each_own_textured_run(slot, node_paints, texture_count, |start, end| {
            textures.push((start, end));
        });
    }
    rectangles.sort_unstable();
    textures.sort_unstable();
    let mut result = Vec::new();
    for op in ops {
        let (start, count, ranges) = match *op {
            PaintOp::Rects { start, count } => (start, count, &rectangles),
            PaintOp::Textured { start, count } => (start, count, &textures),
        };
        let mut cursor = start as usize;
        let end = cursor + count as usize;
        for &(range_start, range_end) in ranges {
            let lo = cursor.max(range_start);
            let hi = end.min(range_end);
            if lo >= hi {
                continue;
            }
            result.push(match op {
                PaintOp::Rects { .. } => PaintOp::Rects {
                    start: lo as u32,
                    count: (hi - lo) as u32,
                },
                PaintOp::Textured { .. } => PaintOp::Textured {
                    start: lo as u32,
                    count: (hi - lo) as u32,
                },
            });
            cursor = hi;
        }
    }
    result
}

/// Same pad as scissor (`AppState::DAMAGE_SCISSOR_PAD_LOGICAL`) so cull ≥ clip.
pub(crate) const DAMAGE_CULL_PAD_LOGICAL: f32 = 16.0;

/// Glyph clip on a scroll-layer Clear. `DAMAGE_CULL_PAD_LOGICAL` is for SDF
/// shadows / scissor; using it on text reaches from the inset column into the
/// gallery rail and src-overs 「布局」/Gallery captions onto retain.
#[cfg(test)]
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

/// Bounded repair list for strips + sticky / overlay / named paint. Thirty-two
/// disjoint holes avoid repainting a 4K viewport when nested pins fragment it.
pub(crate) const CULL_RECTS_CAP: usize = 32;

/// N cull AABBs. Dual-axis L strips and extra pin holes — never their union
/// (Clear of the AABB would wipe the copied overlap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CullRects {
    rects: [[f32; 4]; CULL_RECTS_CAP],
    len: u8,
}

impl CullRects {
    /// Explicitly empty repair coverage; None means no culling to callers.
    pub(crate) const fn empty() -> Self {
        Self {
            rects: [[0.0; 4]; CULL_RECTS_CAP],
            len: 0,
        }
    }

    pub(crate) fn one(a: [f32; 4]) -> Self {
        let mut s = Self {
            rects: [[0.0; 4]; CULL_RECTS_CAP],
            len: 0,
        };
        let _ = s.push(a);
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
            #[cfg(test)]
            if let Ok(limit) = std::env::var("JADIX_PERF_REPAIR_LIMIT") {
                if s.len as usize >= limit.parse::<usize>().unwrap().min(CULL_RECTS_CAP) {
                    return None;
                }
            }
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
        self.len > 0 && self.hits_aabb(instance_paint_bounds(i))
    }

    pub(crate) fn hits_textured(self, d: &ScaledTexturedDraw) -> bool {
        self.strips().any(|r| textured_hits_damage(d, r))
    }

    pub(crate) fn hits_aabb(self, bounds: [f32; 4]) -> bool {
        self.strips().any(|r| !aabbs_disjoint(bounds, r))
    }

    #[cfg(test)]
    pub(crate) fn count(self) -> usize {
        self.len as usize
    }
}

pub(crate) fn scale_xywh(rect: [f32; 4], scale: f32) -> [f32; 4] {
    let s = scale.max(0.0);
    [rect[0] * s, rect[1] * s, rect[2] * s, rect[3] * s]
}

/// Compact caption slivers can share one painter replay. Keep tall repairs
/// separate so merging cannot bypass the full-pass fallback for broad regions.
/// Rebuild disjoint coverage after
/// merging: an expanded hole may meet a third hole, but alpha must paint once.
fn coalesce_scroll_repairs(rects: &mut Vec<[f32; 4]>) {
    if rects.len() < 4 || rects.len() > 128 {
        return;
    }
    loop {
        let mut best = None;
        let mut best_cost = f32::INFINITY;
        for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                let union = crate::app::damage::union_rects(rects[i], rects[j]);
                let sum = rects[i][2] * rects[i][3] + rects[j][2] * rects[j][3];
                let area = union[2] * union[3];
                let cost = area - sum;
                if union[3] <= 64.0 && cost <= 16_384.0 && area <= sum * 1.5 && cost < best_cost {
                    best = Some((i, j, union));
                    best_cost = cost;
                }
            }
        }
        let Some((i, j, union)) = best else { break };
        rects[i] = union;
        rects.swap_remove(j);
    }
    let mut disjoint = Vec::new();
    rects.sort_by(|a, b| (b[2] * b[3]).total_cmp(&(a[2] * a[3])));
    for rect in rects.drain(..) {
        crate::app::damage::push_disjoint_rect(&mut disjoint, rect);
    }
    *rects = disjoint;
}

/// Final scroll repair coverage. Inputs already include each owner's paint
/// outset and overflow clip: adding another generic pad leaks into the header.
pub(crate) fn scroll_repair_rects_physical(
    jobs: &[crate::app::damage::ScrollStripBlitJob],
    pins: &[[f32; 4]],
    scale: f32,
) -> Option<CullRects> {
    let mut coverage: Vec<_> = jobs
        .iter()
        .flat_map(|j| j.plan.exposed_strips())
        .chain(pins.iter().copied())
        .collect();
    // Parent repairs subsume their avatar/label descendants. Visit them first
    // so contained children cannot fragment one hole beyond the GPU limit.
    coverage.sort_by(|a, b| (b[2] * b[3]).total_cmp(&(a[2] * a[3])));
    let mut rects = Vec::new();
    for rect in coverage {
        let r = scale_xywh(rect, scale);
        let x = r[0].floor();
        let y = r[1].floor();
        let r = [x, y, (r[0] + r[2]).ceil() - x, (r[1] + r[3]).ceil() - y];
        crate::app::damage::push_disjoint_rect(&mut rects, r);
    }
    for job in jobs {
        let viewport = scale_xywh(job.plan.viewport, scale);
        let left = viewport[0].floor();
        let top = viewport[1].floor();
        let right = viewport[0] + viewport[2];
        let bottom = viewport[1] + viewport[3];
        if right.ceil() > right.floor() {
            crate::app::damage::push_disjoint_rect(
                &mut rects,
                [right.floor(), top, 1.0, bottom.ceil() - top],
            );
        }
        if bottom.ceil() > bottom.floor() {
            crate::app::damage::push_disjoint_rect(
                &mut rects,
                [left, bottom.floor(), right.ceil() - left, 1.0],
            );
        }
    }
    #[cfg(test)]
    let merge = std::env::var("JADIX_PERF_REPAIR_MERGE").as_deref() != Ok("0");
    #[cfg(not(test))]
    let merge = true;
    if merge {
        coalesce_scroll_repairs(&mut rects);
    }
    CullRects::from_rects(rects)
}

/// Replaying the painter list for many scissors costs more than one full pass.
/// Empty/unbounded coverage also needs a full clear so copied pixels cannot be
/// blended twice. Keep sparse edge repairs on the strip path.
pub(crate) fn scroll_repair_prefers_full(
    coverage: Option<CullRects>,
    width: u32,
    height: u32,
) -> bool {
    let max_passes = 12;
    #[cfg(test)]
    let max_passes = std::env::var("JADIX_PERF_SCROLL_PASS_LIMIT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(max_passes);
    #[cfg(test)]
    if max_passes == 0 {
        return false;
    }
    let Some(coverage) = coverage else {
        return true;
    };
    let mut count = 0;
    let mut area = 0_u64;
    for rect in coverage.strips() {
        if let Some((_, _, w, h)) =
            crate::app::damage::physical_xywh_to_scissor(rect, width, height)
        {
            count += 1;
            area += w as u64 * h as u64;
        }
    }
    count >= max_passes || area as f64 >= width as f64 * height as f64 * 0.6
}

pub(crate) fn physical_repair_text_cull(coverage: CullRects, scale: f32) -> CullRects {
    let scale = scale.max(1e-6);
    CullRects::from_rects(coverage.strips().map(|rect| {
        [
            (rect[0] - 1.0) / scale,
            (rect[1] - 1.0) / scale,
            (rect[2] + 2.0) / scale,
            (rect[3] + 2.0) / scale,
        ]
    }))
    .unwrap_or_else(CullRects::empty)
}

/// Text preparation coverage for visible Region/OverlayRestore clears.
/// Clip each hole before unioning: offscreen holes on opposite sides of the
/// viewport must not cause every intervening label to be prepared again.
pub(crate) fn region_repair_text_cull(
    dirties: &[PaintDirty],
    previous: &[(u64, [f32; 4])],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
    scale: f32,
    width: u32,
    height: u32,
) -> CullRects {
    let mut bounds = None;
    let mut pieces = Vec::new();
    let scale = scale.max(1e-6);
    for dirty in dirties {
        if !matches!(
            dirty.kind,
            PaintDirtyKind::Region | PaintDirtyKind::OverlayRestore
        ) {
            continue;
        }
        let hole = node_present_scissor_logical(
            dirty.kind,
            dirty.key,
            dirty.bounds,
            previous,
            paints,
            0.0,
        );
        let Some((x, y, w, h)) =
            crate::app::damage::damage_to_scissor_physical(hole, 0.0, scale, width, height)
        else {
            continue;
        };
        // Use the exact GPU scissor, not the fractional logical edge that
        // glyphon's TextBounds would truncate one pixel earlier.
        let hole = [
            x as f32 / scale,
            y as f32 / scale,
            w as f32 / scale,
            h as f32 / scale,
        ];
        // Keep one device pixel of raster context at the clip edge. Encode
        // still uses the exact clear scissor, so this cannot restamp neighbors.
        let hole = padded_xywh(hole, 1.0 / scale);
        if dirty.kind == PaintDirtyKind::OverlayRestore {
            // Same scissors as the clear. Unioning the raw halo pulls the
            // neighbor caption back into the prepare and src-overs a slice.
            let shadow = shadow.or(Some(dirty.bounds));
            for piece in scissors_keeping_stable_page_captions(vec![hole], paints, stamps, shadow) {
                crate::app::damage::push_disjoint_rect(&mut pieces, piece);
            }
        } else {
            bounds = Some(match bounds {
                Some(prev) => crate::app::damage::union_rects(prev, hole),
                None => hole,
            });
        }
    }
    if let Some(region) = bounds {
        pieces.insert(0, region);
    }
    if pieces.is_empty() {
        return CullRects::empty();
    }
    CullRects::from_rects(pieces.iter().copied()).unwrap_or_else(|| {
        let union = pieces
            .into_iter()
            .reduce(crate::app::damage::union_rects)
            .unwrap_or([0.0; 4]);
        CullRects::one(union)
    })
}

fn transform_is_identity(m: &[f32; 6]) -> bool {
    m.iter()
        .zip(MAT2D_IDENTITY.iter())
        .all(|(a, b)| (*a - *b).abs() <= 1e-5)
}

/// Outset the SDF vertex shader actually paints (`rounded_rect.wgsl` `ext` =
/// `|offset| + blur * 2 + spread + border + 2`). Dest-reset / overlay occupancy
/// that uses `blur` once leaves the halo in retain (dropdown smear on calendar).
pub fn sdf_outset_shadow_pad(blur: f32, spread: f32, offset: [f32; 2], border_width: f32) -> f32 {
    sdf_painted_halo_pad(blur, spread, offset) + border_width.max(0.0) + 2.0
}

/// Painted halo beyond the layout box (CSS spread / blur / offset). Vertex AA
/// (+2) and border sit on the box — stacking them on Hard ring spread 3 punches
/// `form_field` labels (`gap(4)`).
pub(crate) fn sdf_painted_halo_pad(blur: f32, spread: f32, offset: [f32; 2]) -> f32 {
    // Negative spread shrinks the SDF. `|spread|` treated toast/Latest −2 as
    // extra outset and dest-reset punched wrapping below the pill.
    (offset[0].abs().max(offset[1].abs()) + blur.max(0.0) * 2.0 + spread).max(0.0)
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
        // Always include vertex AA (+2). Skipping `extra == 2` (fill with no
        // shadow/border) left pulse SDF fringes in retain: last-present dest-reset
        // uses wipe_pad 0, so rounded AABB corners accumulated as a frame.
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

/// I-beam blink dirties a 2px column on a text leaf with no SDF fill.
/// Those pixels live in the swapchain overlay after retain blit — dest-reset
/// of this dirty would CLEAR Search and recapture a hairline slice (竖条).
pub(crate) fn paint_dirty_is_caret_overlay(
    dirty: &crate::app::damage::PaintDirty,
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    if dirty.kind != crate::app::damage::PaintDirtyKind::Node {
        return false;
    }
    let w = dirty.bounds[2];
    let h = dirty.bounds[3];
    if w > crate::utils::text_field::TEXT_EDIT_CARET_W + 2.5 || w < 0.5 || h < 8.0 {
        return false;
    }
    let has_keyed = paints_has_keyed_slot(dirty.key, dirty.bounds, node_paints);
    node_paints.iter().any(|p| {
        slot_matches_dirty(dirty.key, dirty.bounds, p, has_keyed)
            && !slot_has_sdf_dest_ink(p, instances)
            // Text I-beam lives in swapchain chrome. A retained box (OTP caret)
            // has neither glyphs nor chrome; opacity 0 must still dest-reset or
            // the previous stroke stays in retain and the caret never blinks off.
            && (p.text.0 < p.text.1 || p.chrome.0 < p.chrome.1)
    })
}

/// Chrome dest-reset / last-present box.
///
/// Hairline I-beam is a 2px fill. `instance_paint_bounds` always adds vertex AA
/// (+2 each side) → a 6px wipe through the glyph under the caret (Search「S」
/// 被白线贯穿). Hard-ring / outset chrome still needs the halo.
#[allow(dead_code)]
fn chrome_reset_bounds(i: &InstanceInput) -> [f32; 4] {
    if instance_has_outset_shadow(i) || i.border_width > 0.5 {
        instance_paint_bounds(i)
    } else {
        instance_fill_bounds(i)
    }
}

/// Opaque fill AABB (CSS transform applied). Outset box-shadow src-overs;
/// putting that halo in a layered dest-reset punches wrapping thread / the
/// overlay rail (Latest outset shadow while scale/ancestor transform is live).
fn instance_fill_bounds(i: &InstanceInput) -> [f32; 4] {
    let bounds = [i.rect_pos[0], i.rect_pos[1], i.rect_size[0], i.rect_size[1]];
    if transform_is_identity(&i.transform) {
        if i.clip_size[0] >= 0.0 && i.clip_size[1] >= 0.0 {
            intersect_rects(
                bounds,
                [i.clip_pos[0], i.clip_pos[1], i.clip_size[0], i.clip_size[1]],
            )
            .unwrap_or([0.0; 4])
        } else {
            bounds
        }
    } else {
        aabb_of_centered_transform(bounds, &i.transform)
    }
}

#[cfg(test)]
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
    // Single-line edits shift the glyph origin while retaining the viewport
    // width. Include the panned run so selection recoloring can recapture its
    // visible tail even when the origin is far outside the field.
    let pan = if n.style.overflow.allows_caret_scroll() {
        n.style.scroll_x.max(0.0)
    } else {
        0.0
    };
    // Glyphon applies paint_scale separately from the residual paint matrix.
    // The origin already includes CSS scaling; only the run's extent grows.
    let scale = if n.paint_scale.is_finite() && n.paint_scale > 0.0 {
        n.paint_scale
    } else {
        1.0
    };
    let mut bounds = [
        n.x - extra,
        n.y - extra,
        ((n.width + pan) * scale + extra * 2.0).max(0.0),
        (n.height * scale + extra * 2.0).max(0.0),
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
    // Raster coverage can extend one pixel beyond the layout line box.
    // This only broadens candidate selection; the owner's hard clip is kept.
    !aabbs_disjoint(padded_xywh(bounds, 1.0), damage_logical)
}

/// Label belongs to a hover-dirty widget when its **center** is inside the dirty box.
///
/// AABB intersect is too wide: gallery Button Variants sits labels a few px apart.
/// Capture **must not** use this — padded Input dirty covers sidebar captions in the
/// same horizontal band (`glyph_belongs_to_dirty` is collect-slot only).
#[allow(dead_code)]
pub(crate) fn text_owned_by_node_dirty(n: &TextLayoutInfo, dirty_logical: [f32; 4]) -> bool {
    crate::app::damage::text_center_in_dirty([n.x, n.y, n.width, n.height], dirty_logical)
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
#[cfg(test)]
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
    let emoji = crate::renderer::emoji_detect::text_contains_emoji(&n.text);
    let scaled = (n.paint_scale - 1.0).abs() > 1e-4;
    // Overflow-visible emoji: collect leaves `clip_bounds` as ancestor overflow
    // only. Replacing `None` with a rest-sized damage AABB is the hover:scale
    // Twemoji square crop (glyphon TextBounds of the unscaled strut).
    if emoji && scaled && n.clip_bounds.is_none() {
        let (pw, ph) = crate::renderer::emoji_layout::emoji_visible_paint_size(
            n.width,
            n.height,
            n.style.size_resolved(),
        );
        let ink = [
            n.x,
            n.y,
            (pw * n.paint_scale).max(0.0),
            (ph * n.paint_scale).max(0.0),
        ];
        if aabbs_disjoint(ink, clip) {
            return None;
        }
        return Some(n.clone());
    }
    let hit = match n.clip_bounds {
        Some(existing) => intersect_rects(existing, clip)?,
        None => clip,
    };
    if hit[2] <= 0.0 || hit[3] <= 0.0 {
        return None;
    }
    let ink = padded_xywh([n.x, n.y, n.width.max(0.0), n.height.max(0.0)], 1.0);
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
#[cfg(test)]
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

/// Scroll-layer text is rendered inside a pass whose scissor is the exposed
/// strip. Keep the complete line in glyphon's TextBounds: clipping its top or
/// bottom here makes glyphon discard the covered half of a glyph before the GPU
/// scissor gets a chance to constrain the repair.
fn clip_text_node_to_scroll_rect(n: &TextLayoutInfo, clip: [f32; 4]) -> Option<TextLayoutInfo> {
    if !text_hits_damage(n, clip) {
        return None;
    }
    if crate::app::damage::rect_contains_rect(clip, padded_xywh(text_paint_bounds(n), 1.0)) {
        return Some(n.clone());
    }
    let emoji = crate::renderer::emoji_detect::text_contains_emoji(&n.text);
    let scaled = (n.paint_scale - 1.0).abs() > 1e-4;
    if emoji && scaled && n.clip_bounds.is_none() {
        let (pw, ph) = crate::renderer::emoji_layout::emoji_visible_paint_size(
            n.width,
            n.height,
            n.style.size_resolved(),
        );
        let ink = [
            n.x,
            n.y,
            (pw * n.paint_scale).max(0.0),
            (ph * n.paint_scale).max(0.0),
        ];
        return (!aabbs_disjoint(ink, clip)).then(|| n.clone());
    }

    let base = n
        .clip_bounds
        .unwrap_or([n.x, n.y, n.width.max(0.0), n.height.max(0.0)]);
    if aabbs_disjoint(base, clip) {
        return None;
    }
    let x0 = base[0].max(clip[0]);
    let x1 = (base[0] + base[2]).min(clip[0] + clip[2]);
    let width = (x1 - x0).max(0.0);
    if width <= 0.0 || base[3] <= 0.0 {
        return None;
    }
    let mut cloned = n.clone();
    cloned.clip_bounds = Some([x0, base[1], width, base[3]]);
    Some(cloned)
}

/// Horizontal replay of a grazing caption. Scroll-strip capture intentionally
/// keeps the full ancestor clip vertically, but overlay replay must not carry
/// a full-page `clip_bounds` height into the late caption pass.
fn clip_text_node_to_grazing_rect(n: &TextLayoutInfo, clip: [f32; 4]) -> Option<TextLayoutInfo> {
    if !text_hits_damage(n, clip) {
        return None;
    }
    let emoji = crate::renderer::emoji_detect::text_contains_emoji(&n.text);
    let scaled = (n.paint_scale - 1.0).abs() > 1e-4;
    if emoji && scaled && n.clip_bounds.is_none() {
        let (pw, ph) = crate::renderer::emoji_layout::emoji_visible_paint_size(
            n.width,
            n.height,
            n.style.size_resolved(),
        );
        let ink = [
            n.x,
            n.y,
            (pw * n.paint_scale).max(0.0),
            (ph * n.paint_scale).max(0.0),
        ];
        return (!aabbs_disjoint(ink, clip)).then(|| n.clone());
    }
    let ancestor = n
        .clip_bounds
        .unwrap_or([n.x, n.y, n.width.max(0.0), n.height.max(0.0)]);
    let line = [n.x, n.y, n.width.max(0.0), n.height.max(0.0)];
    if aabbs_disjoint(ancestor, clip) || aabbs_disjoint(line, clip) {
        return None;
    }
    let x0 = line[0].max(ancestor[0]).max(clip[0]);
    let x1 = (line[0] + line[2])
        .min(ancestor[0] + ancestor[2])
        .min(clip[0] + clip[2]);
    let width = (x1 - x0).max(0.0);
    let y0 = line[1].max(ancestor[1]);
    let y1 = (line[1] + line[3]).min(ancestor[1] + ancestor[3]);
    let height = (y1 - y0).max(0.0);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let mut cloned = n.clone();
    cloned.clip_bounds = Some([x0, y0, width, height]);
    Some(cloned)
}

pub(crate) fn clip_text_nodes_to_cull(
    nodes: &[TextLayoutInfo],
    cull: CullRects,
) -> Vec<TextLayoutInfo> {
    let mut out = Vec::new();
    for node in nodes {
        let mut clips = Vec::new();
        let mut unclipped = false;
        for strip in cull.strips() {
            if let Some(cloned) = clip_text_node_to_scroll_rect(node, strip) {
                if let Some(clip) = cloned.clip_bounds {
                    // Vertical coverage preserves the whole glyph line. Two
                    // disjoint repair strips can therefore capture the same
                    // glyph pixels; submit that coverage only once.
                    crate::app::damage::push_disjoint_rect(&mut clips, clip);
                } else {
                    unclipped = true;
                }
            }
        }
        if unclipped {
            out.push(node.clone());
        } else {
            out.extend(clips.into_iter().map(|clip| {
                let mut cloned = node.clone();
                cloned.clip_bounds = Some(clip);
                cloned
            }));
        }
    }
    out
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
        && a.spans == b.spans
        && a.glyph_fx == b.glyph_fx
        && a.shimmer == b.shimmer
        && std::mem::discriminant(&a.glyph_raster) == std::mem::discriminant(&b.glyph_raster)
        && approx_f32(a.x, b.x, POS)
        && approx_f32(a.y, b.y, POS)
        && approx_f32(a.width, b.width, SIZE)
        && approx_f32(a.height, b.height, SIZE)
        && approx_f32(a.paint_scale, b.paint_scale, 0.002)
        && a.paint_transform
            .iter()
            .zip(b.paint_transform.iter())
            .all(|(x, y)| approx_f32(*x, *y, 0.002))
        && approx_f32(a.paint_origin[0], b.paint_origin[0], POS)
        && approx_f32(a.paint_origin[1], b.paint_origin[1], POS)
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
    if let Some(clip) = scroll_clip {
        let clipped = clip_text_nodes_to_cull(slice, clip);
        return if clipped.is_empty() {
            None
        } else {
            Some(Cow::Owned(clipped))
        };
    }
    if !partial_load {
        return Some(Cow::Borrowed(slice));
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

#[cfg(test)]
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

fn same_page_text_ink(a: &TextLayoutInfo, b: &TextLayoutInfo) -> bool {
    const POS: f32 = 0.02;
    const SIZE: f32 = 0.5;
    a.text == b.text
        && (a.x - b.x).abs() <= POS
        && (a.y - b.y).abs() <= POS
        && (a.width - b.width).abs() <= SIZE
        && (a.height - b.height).abs() <= SIZE
}

/// Page text that stays in retain during OverlayRestore.
///
/// The padded halo contains a trigger caption that sticks out of the shadow.
/// Encode no longer clears that ink. Page glyphon must not redraw it either:
/// that pass runs before the shadow and would src-over the retained glyphs.
/// Ink fully inside the shadow is still cleared, so it stays on the page mesh.
/// Panel chips sit inside the stamp and stay on the page mesh.
pub(crate) fn overlay_restore_fringe_text(
    page_text: &[TextLayoutInfo],
    dirties: &[PaintDirty],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
) -> Vec<TextLayoutInfo> {
    let restores: Vec<[f32; 4]> = dirties
        .iter()
        .filter(|d| {
            d.kind == PaintDirtyKind::OverlayRestore && d.bounds[2] > 0.5 && d.bounds[3] > 0.5
        })
        .map(|d| d.bounds)
        .collect();
    if restores.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in page_text {
        let ink = [n.x, n.y, n.width.max(0.0), n.height.max(0.0)];
        if ink[2] <= 0.5 || ink[3] <= 0.5 {
            continue;
        }
        if !restores.iter().any(|r| !aabbs_disjoint(ink, *r)) {
            continue;
        }
        // Inside the portal layout box the panel covers the ink. Page replay
        // draws that before the overlay. Flushing it afterwards paints the
        // chip on top of the panel.
        if stamps.iter().any(|(_, stamp)| {
            stamp[2] > 0.5 && stamp[3] > 0.5 && crate::app::damage::rect_contains_rect(*stamp, ink)
        }) {
            continue;
        }
        if let Some(shadow) = shadow {
            if shadow[2] > 0.5
                && shadow[3] > 0.5
                && crate::app::damage::rect_contains_rect(shadow, ink)
            {
                continue;
            }
        }
        // One clip for the whole line. Subtracting a stamp that does not
        // touch the ink splits nothing; subtracting one that cuts the line
        // would hand CJK and Latin to different prepares, and the non-ASCII
        // side waits a frame.
        let clip = padded_xywh(ink, NODE_PRESENT_GLYPH_CLIP_PAD);
        let clips = if stamps.iter().any(|(_, stamp)| !aabbs_disjoint(ink, *stamp)) {
            crate::app::damage::page_scissors_excluding_portal_boxes(clip, stamps)
        } else {
            vec![clip]
        };
        for clip in clips {
            if clip[2] <= 0.5 || clip[3] <= 0.5 || aabbs_disjoint(clip, ink) {
                continue;
            }
            let mut cloned = n.clone();
            cloned.clip_bounds = Some(clip);
            out.push(cloned);
        }
    }
    out
}

/// Logical coverage used by the caption/restore planning tests. GPU encode
/// records its actual repair scissors and deduplicates in physical pixels.
#[cfg(test)]
pub(crate) fn glyph_flush_logical_pieces(
    node_holes: &[[f32; 4]],
    dirties: &[PaintDirty],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
) -> Vec<[f32; 4]> {
    let mut pieces = Vec::new();
    for hole in node_holes {
        if hole[2] > 0.0 && hole[3] > 0.0 {
            crate::app::damage::push_disjoint_rect(&mut pieces, *hole);
        }
    }
    for dirty in dirties {
        let pad = match dirty.kind {
            PaintDirtyKind::Region | PaintDirtyKind::OverlayRestore => 2.0,
            PaintDirtyKind::Overlay | PaintDirtyKind::Node => continue,
        };
        let padded = padded_xywh(dirty.bounds, pad);
        let parts = if dirty.kind == PaintDirtyKind::OverlayRestore {
            scissors_keeping_stable_page_captions(vec![padded], paints, stamps, shadow)
        } else {
            vec![padded]
        };
        for part in parts {
            crate::app::damage::push_disjoint_rect(&mut pieces, part);
        }
    }
    pieces
}

/// Glyphon respects the render-pass scissor. Round/clamp first, then subtract
/// overlaps: neighboring fractional repairs can share a device-pixel column.
/// An AABB union or a full-target scissor would repaint untouched glyph edges.
pub(crate) fn capture_glyph_scissors_physical(
    repaired: &[[f32; 4]],
    width: u32,
    height: u32,
) -> Vec<[f32; 4]> {
    let mut pieces = Vec::new();
    for rect in repaired {
        if let Some((x, y, w, h)) =
            crate::app::damage::physical_xywh_to_scissor(*rect, width, height)
        {
            crate::app::damage::push_disjoint_rect(
                &mut pieces,
                [x as f32, y as f32, w as f32, h as f32],
            );
        }
    }
    pieces
}

/// Subtract replayed pixels from the late page-glyph capture after both sets
/// have been rounded to physical scissors. This keeps one source of coverage
/// for every device pixel when the two captures overlap.
pub(crate) fn capture_glyph_scissors_after_replay_physical(
    repaired: &[[f32; 4]],
    replayed: &[[f32; 4]],
    width: u32,
    height: u32,
) -> Vec<[f32; 4]> {
    let mut pieces = capture_glyph_scissors_physical(repaired, width, height);
    for replay in capture_glyph_scissors_physical(replayed, width, height) {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| crate::app::damage::subtract_rect(piece, replay))
            .collect();
    }
    pieces
}

/// Late page glyphs must never cover a portal painted earlier in the pass.
/// Subtract after outward rounding so fractional layout edges cannot reopen a
/// device-pixel row in the panel when the final GPU scissor is rounded again.
pub(crate) fn capture_glyph_scissors_excluding_portals_physical(
    repaired: &[[f32; 4]],
    portals: &[[f32; 4]],
    width: u32,
    height: u32,
) -> Vec<[f32; 4]> {
    let mut pieces = capture_glyph_scissors_physical(repaired, width, height);
    // The repair itself is an outward-rounded clear, but a portal only owns
    // pixels fully covered by its fractional layout rect. Keeping the partial
    // edge pixels in the page replay preserves the underlay beneath the portal's
    // antialiased boundary; excluding the outward-rounded edge leaves a white
    // line where CLEAR removed the retained page glyphs before the portal blend.
    for portal in capture_glyph_portal_coverage_physical(portals, width, height) {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| crate::app::damage::subtract_rect(piece, portal))
            .collect();
    }
    pieces
}

fn capture_glyph_portal_coverage_physical(
    portals: &[[f32; 4]],
    width: u32,
    height: u32,
) -> Vec<[f32; 4]> {
    let mut pieces = Vec::new();
    for [x, y, w, h] in portals {
        if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            continue;
        }
        let x0 = x.max(0.0).ceil();
        let y0 = y.max(0.0).ceil();
        let x1 = (x + w).min(width as f32).floor();
        let y1 = (y + h).min(height as f32).floor();
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        crate::app::damage::push_disjoint_rect(&mut pieces, [x0, y0, x1 - x0, y1 - y0]);
    }
    pieces
}

/// Largest page paint that contains `face` and does not meet a portal stamp.
///
/// That paint is the trigger (or the row that is only the trigger). The card
/// and the scrollport also contain the caption, but they contain the open
/// panel too, so they are not candidates.
fn page_trigger_box_for_face(
    face: [f32; 4],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
) -> Option<[f32; 4]> {
    let live = |stamp: &[f32; 4]| stamp[2] > 0.5 && stamp[3] > 0.5;
    paints
        .iter()
        .filter(|other| {
            !other.overlay
                && crate::app::damage::rect_contains_rect(other.bounds, face)
                && other.bounds[2] * other.bounds[3] > face[2] * face[3] + 1.0
                && stamps.iter().all(|(_, stamp)| {
                    live(stamp)
                        && intersect_rects(other.bounds, *stamp).is_none()
                        && !crate::app::damage::rect_contains_rect(other.bounds, *stamp)
                })
        })
        .max_by(|a, b| {
            let area = |p: &NodePaint| p.bounds[2] * p.bounds[3];
            area(a)
                .partial_cmp(&area(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|paint| paint.bounds)
}

/// Page text in the column the menu is leaving. Expand-up keeps the live stamp
/// at the trigger and the old footprint below it. A calendar row there does not
/// intersect the live stamp, so the trigger carve would slice the retained menu
/// into bands. The trigger caption sits above the stamp and is not this ink.
fn face_is_vacated_menu_ink(face: [f32; 4], stamps: &[(u64, [f32; 4])]) -> bool {
    let cx = face[0] + face[2] * 0.5;
    let cy = face[1] + face[3] * 0.5;
    let in_column = stamps.iter().any(|(_, stamp)| {
        stamp[2] > 0.5
            && stamp[3] > 0.5
            && cy >= stamp[1]
            && cx >= stamp[0]
            && cx < stamp[0] + stamp[2]
    });
    let on_stamp = stamps.iter().any(|(_, stamp)| {
        stamp[2] > 0.5 && stamp[3] > 0.5 && intersect_rects(face, *stamp).is_some()
    });
    in_column && !on_stamp
}

/// Coverage for [`scissors_keeping_stable_page_captions`] on an OverlayRestore hole.
///
/// `hole` is the footprint being erased (previous ∪ current). `live` is the panel
/// still on screen. Expand-up makes `live` shorter than `hole`. Captions in that
/// vacated strip sit inside `hole` only; using `live` alone leaves the old menu
/// pixels in retain after the panel has shrunk.
pub(crate) fn overlay_restore_caption_shadow(hole: [f32; 4], live: Option<[f32; 4]>) -> [f32; 4] {
    match live {
        Some(live) => crate::app::damage::union_rects(hole, live),
        None => hole,
    }
}

/// OverlayRestore / overlay-suffix scissors with stable page captions removed.
///
/// `打开` and `关闭` share one suffix box. The padded portal halo still contains
/// that box, so a fade tick CLEARs it and the shadow is src-overed on top.
/// Redrawing the line afterwards blends glyph coverage against that animating
/// shadow, so the unchanged Latin flickers. Leaf caption faces that stick out
/// of the shadow stay in retain. Text fully inside the shadow, the gap under
/// the trigger, and the panel itself stay in the scissor.
///
/// A caption that sits on the trigger is not the only ink the halo clears.
/// The restore pass runs after the button's own dest-reset, so punching the
/// button face paints the page color through the chrome for one frame. The
/// trigger box stays out of this scissor; the button node still dest-resets
/// its own fill.
pub(crate) fn scissors_keeping_stable_page_captions(
    pieces: Vec<[f32; 4]>,
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
) -> Vec<[f32; 4]> {
    if pieces.is_empty() || stamps.is_empty() {
        return pieces;
    }
    let mut faces = Vec::new();
    for paint in paints {
        if paint.overlay || paint.text.0 >= paint.text.1 {
            continue;
        }
        let face = paint.bounds;
        if face[2] <= 0.5 || face[3] <= 0.5 {
            continue;
        }
        if let Some(shadow) = shadow {
            if shadow[2] > 0.5
                && shadow[3] > 0.5
                && crate::app::damage::rect_contains_rect(shadow, face)
            {
                // A lone label under the halo stays in the clear so the shadow
                // can rebuild on it. A caption sitting on a page trigger does
                // not: the halo AABB covers it during fade, and clearing it
                // wipes both scripts on every opacity tick.
                let on_trigger = paints.iter().any(|other| {
                    other.key != paint.key
                        && !other.overlay
                        && crate::app::damage::rect_contains_rect(other.bounds, face)
                        && other.bounds[2] * other.bounds[3] > face[2] * face[3] + 1.0
                        && stamps.iter().all(|(_, stamp)| {
                            stamp[2] > 0.5
                                && stamp[3] > 0.5
                                && intersect_rects(other.bounds, *stamp).is_none()
                                && !crate::app::damage::rect_contains_rect(other.bounds, *stamp)
                        })
                });
                if !on_trigger {
                    continue;
                }
            }
        }
        // Vacated rows stay in the scissor so Restore replays the page there
        // and the overlay draw is not cut into the bands between them.
        if face_is_vacated_menu_ink(face, stamps) {
            continue;
        }
        let outside = stamps.iter().all(|(_, stamp)| {
            stamp[2] > 0.5
                && stamp[3] > 0.5
                && intersect_rects(face, *stamp).is_none()
                && !crate::app::damage::rect_contains_rect(face, *stamp)
        });
        if outside {
            faces.push(face);
        }
    }
    let mut leaves = Vec::new();
    for i in 0..faces.len() {
        let face = faces[i];
        let nested = (0..faces.len()).any(|j| {
            i != j
                && crate::app::damage::rect_contains_rect(face, faces[j])
                && face[2] * face[3] > faces[j][2] * faces[j][3] + 1.0
        });
        if !nested {
            leaves.push(face);
        }
    }
    let mut parts = pieces;
    for face in leaves {
        // The restore scissor overlaps the trigger while the layout stamp sits
        // below it. Carving only the glyphs leaves the swatch in the clear, so
        // the next fade tick paints the page color through the control.
        // A lone label has no larger page box that misses the stamp.
        let source = page_trigger_box_for_face(face, paints, stamps).unwrap_or(face);
        let hole = padded_xywh(source, NODE_PRESENT_GLYPH_CLIP_PAD);
        parts = parts
            .into_iter()
            .flat_map(|rect| crate::app::damage::subtract_rect(rect, hole))
            .filter(|rect| rect[2] > 0.5 && rect[3] > 0.5)
            .collect();
    }
    // A 2–4px cap past the trigger sits outside the live shadow. Clearing it
    // paints the page color where the shadow quad does not land, so it flashes
    // as a line on every fade tick — drop those.
    // A thin strip inside `shadow` but outside every live stamp is expand-up
    // paint the panel just left. Nothing redraws it; dropping it leaves the
    // menu fringe on the calendar.
    parts.retain(|rect| {
        if rect[2] >= 8.0 && rect[3] >= 8.0 {
            return true;
        }
        let in_shadow =
            shadow.is_some_and(|s| s[2] > 0.5 && s[3] > 0.5 && intersect_rects(*rect, s).is_some());
        in_shadow
            && !stamps.iter().any(|(_, stamp)| {
                stamp[2] > 0.5 && stamp[3] > 0.5 && intersect_rects(*rect, *stamp).is_some()
            })
    });
    parts
}

/// Region repairs may subsume a changed button Node. Preserve idle captions,
/// but keep the changed control in the clear so its hover/active fill updates.
pub(crate) fn region_scissors_keeping_stable_page_captions(
    pieces: Vec<[f32; 4]>,
    dirty: &PaintDirty,
    dirties: &[PaintDirty],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
) -> Vec<[f32; 4]> {
    // Translated slots need their complete old/new repair area. A portal
    // elsewhere also must not make ordinary page repairs preserve captions.
    let moving = paints
        .iter()
        .any(|p| p.key == dirty.key && !p.overlay && p.rest.is_some_and(|rest| rest != p.bounds));
    let near_portal = pieces.iter().any(|piece| {
        shadow.is_some_and(|bounds| intersect_rects(*piece, bounds).is_some())
            || stamps
                .iter()
                .any(|(_, bounds)| intersect_rects(*piece, *bounds).is_some())
    });
    if moving || !near_portal {
        return pieces;
    }
    let mut kept = scissors_keeping_stable_page_captions(pieces.clone(), paints, stamps, shadow);
    let mut changed: Vec<_> = dirties
        .iter()
        .filter(|d| d.kind == PaintDirtyKind::Node)
        .map(|d| padded_xywh(d.bounds, 2.0))
        .collect();
    changed.extend(
        paints
            .iter()
            .filter(|p| {
                p.key == dirty.key
                    && !p.overlay
                    && stamps
                        .iter()
                        .all(|(_, stamp)| intersect_rects(p.bounds, *stamp).is_none())
            })
            .map(|p| padded_xywh(p.bounds, 2.0)),
    );
    for bounds in changed {
        for piece in &pieces {
            if let Some(hit) = intersect_rects(*piece, bounds) {
                crate::app::damage::push_disjoint_rect(&mut kept, hit);
            }
        }
    }
    kept
}

/// A line that only grazes the portal (AvatarGroup caption above +N) stays in
/// the halo clear so the top shadow can paint. Those glyphs are replayed before
/// the overlay; carving the line out of the scissor also drops that shadow.
pub(crate) fn grazing_caption_replay_runs(
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    texts: &[TextLayoutInfo],
    cleared: &[[f32; 4]],
) -> Vec<TextLayoutInfo> {
    if cleared.is_empty() || texts.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut selected = BTreeSet::new();
    for paint in paints {
        if paint.overlay || paint.text.0 >= paint.text.1 {
            continue;
        }
        let face = paint.bounds;
        if face[2] <= 0.5 || face[3] <= 0.5 || face[3] > 72.0 {
            continue;
        }
        let live = |s: &[f32; 4]| s[2] > 0.5 && s[3] > 0.5;
        let grazes = if stamps.is_empty() {
            cleared
                .iter()
                .any(|hole| live(hole) && intersect_rects(face, *hole).is_some())
        } else {
            stamps.iter().any(|(_, stamp)| {
                live(stamp)
                    && intersect_rects(face, *stamp).is_some()
                    && !crate::app::damage::rect_contains_rect(face, *stamp)
                    && !crate::app::damage::rect_contains_rect(*stamp, face)
            })
        };
        if !grazes {
            continue;
        }
        let lo = paint.text.0.min(texts.len());
        let hi = paint.text.1.min(texts.len());
        for index in lo..hi {
            if selected.contains(&index) {
                continue;
            }
            let node = &texts[index];
            let mut repair: Option<TextLayoutInfo> = None;
            for hole in cleared {
                let Some(clipped) = clip_text_node_to_grazing_rect(node, *hole) else {
                    continue;
                };
                if let Some(piece) = clipped.clip_bounds {
                    if let Some(current) = repair.as_mut() {
                        if let Some(existing) = current.clip_bounds.as_mut() {
                            let right = (existing[0] + existing[2]).max(piece[0] + piece[2]);
                            existing[0] = existing[0].min(piece[0]);
                            existing[2] = (right - existing[0]).max(0.0);
                        }
                    } else {
                        repair = Some(clipped);
                    }
                }
            }
            if let Some(repair) = repair {
                selected.insert(index);
                let mut best = repair;
                // glyphon floors TextBounds edges. The clear scissor is
                // outward-rounded, so keep one logical pixel of coverage on
                // the far edges; the render-pass scissor still limits replay
                // to the repaired pixels.
                if let Some(clip) = best.clip_bounds {
                    best.clip_bounds = Some(padded_xywh(clip, NODE_PRESENT_GLYPH_CLIP_PAD));
                }
                runs.push(best);
            }
        }
    }
    runs
}

/// Cleared overlay / restore pixels that a grazing caption must be drawn back
/// into before the overlay. A live portal can shrink while its previous wider
/// footprint is restored, so both kinds of clear contribute to caption repair.
pub(crate) fn overlay_grazing_caption_replay(
    dirties: &[PaintDirty],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    occupied: Option<[f32; 4]>,
    texts: &[TextLayoutInfo],
    ui_scale: f32,
) -> (Vec<TextLayoutInfo>, Vec<[f32; 4]>) {
    let mut overlay_holes = Vec::new();
    if !stamps.is_empty() {
        let overlay_dirties: Vec<(u64, [f32; 4])> = dirties
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::Overlay)
            .map(|d| (d.key, d.bounds))
            .collect();
        for (keys, bounds) in
            crate::app::damage::merge_overlay_replay_dirties(&overlay_dirties, stamps)
        {
            let key = keys.first().copied().unwrap_or(0);
            let occupancy = crate::app::damage::overlay_encode_occupancy(
                key,
                Some(bounds),
                stamps,
                if stamps.len() <= 1 { occupied } else { None },
            );
            let Some(scissor) =
                crate::app::damage::overlay_encode_scissor(Some(bounds), occupancy, None)
            else {
                continue;
            };
            overlay_holes.extend(scissors_keeping_stable_page_captions(
                vec![scissor],
                paints,
                stamps,
                occupancy,
            ));
        }
    }
    let mut cleared = overlay_holes;
    for dirty in dirties {
        if dirty.kind != PaintDirtyKind::OverlayRestore
            || dirty.bounds[2] <= 0.5
            || dirty.bounds[3] <= 0.5
        {
            continue;
        }
        cleared.extend(scissors_keeping_stable_page_captions(
            vec![dirty.bounds],
            paints,
            stamps,
            occupied.or(Some(dirty.bounds)),
        ));
    }
    // Region / node clears beside the portal erase the same lines the overlay
    // fringe replays. The fringe clip alone restores only the letters inside
    // the halo, so the prefix stays blank after the wider clear. Fold in every
    // page piece that actually meets that halo; a distant button does not.
    let halo_pieces = cleared.clone();
    for dirty in dirties {
        let page_clear = matches!(dirty.kind, PaintDirtyKind::Region | PaintDirtyKind::Node);
        if !page_clear || dirty.bounds[2] <= 0.5 || dirty.bounds[3] <= 0.5 {
            continue;
        }
        let pieces = crate::app::damage::page_scissors_excluding_portal_boxes(dirty.bounds, stamps);
        let pieces = if dirty.kind == PaintDirtyKind::Region {
            region_scissors_keeping_stable_page_captions(
                pieces, dirty, dirties, paints, stamps, occupied,
            )
        } else {
            pieces
        };
        for piece in pieces {
            if piece[2] <= 0.5 || piece[3] <= 0.5 {
                continue;
            }
            if halo_pieces
                .iter()
                .any(|hole| intersect_rects(piece, *hole).is_some())
            {
                cleared.push(piece);
            }
        }
    }
    let candidates = grazing_caption_replay_runs(paints, stamps, texts, &cleared);
    // Padding belongs to preparation, not to pixel ownership. Replaying that
    // pad (or the retained gap between two repairs) blends glyph edges twice.
    // Split at device-pixel clear boundaries, shared with the page mesh.
    let scale = ui_scale.max(0.001);
    let cleared: Vec<_> = cleared
        .into_iter()
        .map(|r| {
            let left = (r[0] * scale).floor() / scale;
            let top = (r[1] * scale).floor() / scale;
            let right = ((r[0] + r[2]) * scale).ceil() / scale;
            let bottom = ((r[1] + r[3]) * scale).ceil() / scale;
            [left, top, right - left, bottom - top]
        })
        .collect();
    let mut runs = Vec::new();
    let mut flush = Vec::new();
    for node in candidates {
        let Some(clip) = node.clip_bounds else {
            continue;
        };
        let mut pieces = Vec::new();
        for hole in &cleared {
            if let Some(hit) = intersect_rects(clip, *hole) {
                // Glyphon floors both edges. Use its snapped bounds for every
                // owner so a fractional split cannot leave a white seam.
                if let Some(hit) = crate::renderer::prepare_text::integerize_clip_rect([
                    hit[0] * scale,
                    hit[1] * scale,
                    hit[2] * scale,
                    hit[3] * scale,
                ]) {
                    crate::app::damage::push_disjoint_rect(&mut pieces, hit);
                }
            }
        }
        for piece in pieces {
            let clip = piece.map(|v| v / scale);
            let mut repaired = node.clone();
            repaired.clip_bounds = Some(clip);
            runs.push(repaired);
            crate::app::damage::push_disjoint_rect(&mut flush, clip);
        }
    }
    (runs, flush)
}

/// Page glyphon is submitted before the overlay suffix. Grazing captions are
/// drawn once, from the capture mesh, before the overlay shadow.
pub(crate) fn page_runs_excluding_caption_replay(
    runs: Vec<TextLayoutInfo>,
    replay: &[TextLayoutInfo],
) -> Vec<TextLayoutInfo> {
    if replay.is_empty() {
        return runs;
    }
    let mut out = Vec::with_capacity(runs.len());
    for n in runs {
        let matching: Vec<_> = replay
            .iter()
            .filter(|r| r.text == n.text && (r.x - n.x).abs() < 0.5 && (r.y - n.y).abs() < 0.5)
            .collect();
        if matching.is_empty() {
            out.push(n);
            continue;
        }
        // Keep all four sides and disjoint gaps, including wrapped rows below
        // the halo. A horizontal-only split dropped those rows on full redraw.
        let mut pieces = vec![n
            .clip_bounds
            .unwrap_or_else(|| padded_xywh([n.x, n.y, n.width.max(0.0), n.height.max(0.0)], 1.0))];
        for repaired in matching {
            let Some(clip) = repaired.clip_bounds else {
                pieces.clear();
                break;
            };
            pieces = pieces
                .into_iter()
                .flat_map(|piece| crate::app::damage::subtract_rect(piece, clip))
                .collect();
        }
        for bounds in pieces {
            let mut kept = n.clone();
            kept.clip_bounds = Some(bounds);
            out.push(kept);
        }
    }
    out
}

/// Hover / focus / `:active` keeps the button Node-dirty while the pointer
/// stays on the trigger. The dest-reset is split on the shadow edge and runs
/// through the caption. That fill has to land under the glyphs; carving the
/// line out froze it, and the click then painted the page color into the hole.
/// Capture puts the glyphs back as one run after the shadow.
pub(crate) fn node_scissors_keeping_unchanged_captions(
    pieces: Vec<[f32; 4]>,
    _paints: &[NodePaint],
    _dirties: &[PaintDirty],
    _stamps: &[(u64, [f32; 4])],
) -> Vec<[f32; 4]> {
    pieces
}

/// Caption leaves outside the portal stamp while a portal is fading.
///
/// `dirty_key`: `Some(false)` keeps leaves whose text did not change,
/// `Some(true)` keeps leaves whose text did change, `None` keeps both.
fn trigger_caption_leaves(
    paints: &[NodePaint],
    dirties: &[PaintDirty],
    stamps: &[(u64, [f32; 4])],
    dirty_key: Option<bool>,
) -> Vec<[f32; 4]> {
    if stamps.is_empty()
        || !dirties
            .iter()
            .any(|d| d.kind == PaintDirtyKind::OverlayRestore)
    {
        return Vec::new();
    }
    let mut faces = Vec::new();
    for paint in paints {
        if paint.overlay || paint.text.0 >= paint.text.1 {
            continue;
        }
        let face = paint.bounds;
        if face[2] <= 0.5 || face[3] <= 0.5 {
            continue;
        }
        let key_dirty = dirties
            .iter()
            .any(|d| d.kind == PaintDirtyKind::Node && d.key == paint.key);
        match dirty_key {
            Some(want) if key_dirty != want => continue,
            _ => {}
        }
        let outside = stamps.iter().all(|(_, stamp)| {
            stamp[2] > 0.5
                && stamp[3] > 0.5
                && intersect_rects(face, *stamp).is_none()
                && !crate::app::damage::rect_contains_rect(face, *stamp)
        });
        if outside {
            faces.push(face);
        }
    }
    let mut leaves = Vec::new();
    for i in 0..faces.len() {
        let face = faces[i];
        let nested = (0..faces.len()).any(|j| {
            i != j
                && crate::app::damage::rect_contains_rect(face, faces[j])
                && face[2] * face[3] > faces[j][2] * faces[j][3] + 1.0
        });
        if !nested {
            leaves.push(face);
        }
    }
    leaves
}

fn unchanged_caption_leaves(
    paints: &[NodePaint],
    dirties: &[PaintDirty],
    stamps: &[(u64, [f32; 4])],
) -> Vec<[f32; 4]> {
    trigger_caption_leaves(paints, dirties, stamps, Some(false))
}

/// Smallest text face that owns `index` and sits outside the portal stamp.
fn trigger_caption_leaf_box(
    index: usize,
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
) -> Option<[f32; 4]> {
    if stamps.is_empty() {
        return None;
    }
    paints
        .iter()
        .filter(|p| {
            !p.overlay
                && p.text.0 <= index
                && index < p.text.1
                && p.bounds[2] > 0.5
                && p.bounds[3] > 0.5
                && stamps.iter().all(|(_, stamp)| {
                    stamp[2] > 0.5
                        && stamp[3] > 0.5
                        && intersect_rects(p.bounds, *stamp).is_none()
                        && !crate::app::damage::rect_contains_rect(p.bounds, *stamp)
                })
        })
        .min_by(|a, b| {
            let area = |p: &NodePaint| p.bounds[2] * p.bounds[3];
            area(a)
                .partial_cmp(&area(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| p.bounds)
}

fn unchanged_caption_glyph(
    index: usize,
    dirties: &[PaintDirty],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
) -> bool {
    let leaves = unchanged_caption_leaves(paints, dirties, stamps);
    if leaves.is_empty() {
        return false;
    }
    let Some(leaf) = paints
        .iter()
        .filter(|p| !p.overlay && p.text.0 <= index && index < p.text.1)
        .min_by(|a, b| {
            let area = |p: &NodePaint| p.bounds[2] * p.bounds[3];
            area(a)
                .partial_cmp(&area(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    else {
        return false;
    };
    if !leaves.iter().any(|face| *face == leaf.bounds) {
        return false;
    }
    // The button dest-reset covers this leaf and wipes the glyphs. Skipping
    // capture leaves the line blank until the next string change.
    !dirties.iter().any(|d| {
        (d.kind == PaintDirtyKind::Node && intersect_rects(d.bounds, leaf.bounds).is_some())
            || (d.kind == PaintDirtyKind::Region
                && paints
                    .iter()
                    .any(|p| p.key == d.key && !p.overlay && p.text.0 <= index && index < p.text.1))
    })
}

/// Drop page-glyphon runs that this frame's node capture will also draw.
///
/// A trigger toggle changes only the CJK prefix (`打开`/`关闭`) and also opens
/// or closes the popover. OverlayRestore replays the page mesh inside a hole
/// that contains the button, and node capture flushes that same caption after
/// chrome. The second src-over is the Latin suffix flicker. Ownership is the
/// collect slot capture uses (`node_present_glyphs_for_dirty`), and only when
/// capture actually flushes: a FullClear has no capture pass, and a chip under
/// the panel stamp is replayed by Restore alone.
pub(crate) fn page_submit_excluding_live_node_capture(
    runs: Vec<TextLayoutInfo>,
    page_text: &[TextLayoutInfo],
    dirties: &[PaintDirty],
    paints: &[NodePaint],
    stamps: &[(u64, [f32; 4])],
    shadow: Option<[f32; 4]>,
    capture_flushes: bool,
) -> Vec<TextLayoutInfo> {
    if !capture_flushes || runs.is_empty() || dirties.is_empty() || page_text.is_empty() {
        return runs;
    }
    let mut owned = BTreeSet::new();
    for dirty in dirties {
        if dirty.kind != PaintDirtyKind::Node {
            continue;
        }
        if crate::app::damage::paint_dirty_covered_by_overlay_restore(dirty, dirties, stamps)
            || crate::app::damage::page_node_present_occluded_by_overlay(dirty.bounds, stamps)
        {
            continue;
        }
        owned.extend(node_present_glyphs_for_dirty(dirty, page_text, paints));
    }
    let fringe = overlay_restore_fringe_text(page_text, dirties, stamps, shadow);
    // A fade frame still needs to suppress an unchanged caption that is nested
    // in the trigger while its shadow remains active. Once the overlay closes,
    // Restore has no shadow pass after page glyphon, so neighboring text must
    // stay submitted even when the restore fringe intersects it.
    let fringe_is_trigger_caption = |n: &TextLayoutInfo| {
        let Some(index) = page_text
            .iter()
            .position(|candidate| same_page_text_ink(candidate, n))
        else {
            return false;
        };
        paints.iter().any(|paint| {
            !paint.overlay
                && paint.text.0 <= index
                && index < paint.text.1
                && crate::app::damage::rect_contains_rect(
                    paint.bounds,
                    [n.x, n.y, n.width.max(0.0), n.height.max(0.0)],
                )
                && paint.bounds[2] * paint.bounds[3] > n.width.max(0.0) * n.height.max(0.0) + 1.0
        })
    };
    if owned.is_empty() && fringe.is_empty() {
        return runs;
    }
    runs.into_iter()
        .filter(|run| {
            let node_owned = owned.iter().any(|i| {
                page_text
                    .get(*i)
                    .is_some_and(|painted| same_page_text_ink(painted, run))
            });
            let fringe_owned = fringe.iter().any(|painted| {
                same_page_text_ink(painted, run)
                    && (!owned.is_empty()
                        || (shadow.is_some() && fringe_is_trigger_caption(painted)))
            });
            !node_owned && !fringe_owned
        })
        .collect()
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

/// Outset box-shadow geometry the SDF actually paints (Hard ring is spread-only,
/// `blur = 0`). Inset shadows stay inside the fill and do not grow dest-reset.
///
/// Do **not** gate on `shadow_color[3]`: Latest fade multiplies that alpha by
/// opacity, so mid-fade (`0.18 * 0.2`) drops below 0.05 and dest-reset used
/// fill-only — PartialLoad stacked every frame's halo (enter ring / hide ghost).
fn instance_has_outset_shadow(i: &InstanceInput) -> bool {
    i.shadow_inset == 0.0
        && (i.shadow_blur > 0.0
            || i.shadow_spread.abs() > 0.0
            || i.shadow_offset[0] != 0.0
            || i.shadow_offset[1] != 0.0)
}

/// Visible fill, border, or outset shadow — pixels dest-reset must erase.
/// Opacity animations (`animate-pulse`) paint below the opaque threshold; they
/// still src-over and must wipe last-present or the rounded AABB ghosts a frame.
pub(crate) fn instance_has_dest_ink(i: &InstanceInput) -> bool {
    i.color[3] > 0.05
        || (i.border_width > 0.0 && i.border_color[3] > 0.05)
        || instance_has_outset_shadow(i)
}

/// Filling this instance's AABB with `color` would paint outside its SDF
/// (capsule corners, outline ring). Dest-reset must clear with the layer
/// underneath and replay this chrome instead.
fn instance_aabb_clear_unsafe(i: &InstanceInput) -> bool {
    !instance_fill_opaque(i)
        || i.radii.iter().any(|r| *r > 0.5)
        || (i.border_width > 0.5 && i.border_color[3] > 0.05)
        || !transform_is_identity(&i.transform)
}

fn instance_rect_matches_slot(inst: &InstanceInput, slot: [f32; 4]) -> bool {
    const SLACK: f32 = 1.0;
    (inst.rect_pos[0] - slot[0]).abs() <= SLACK
        && (inst.rect_pos[1] - slot[1]).abs() <= SLACK
        && (inst.rect_size[0] - slot[2]).abs() <= SLACK
        && (inst.rect_size[1] - slot[3]).abs() <= SLACK
}

/// Fill / outset shadow on the slot box, or CSS outline (slot outset by width).
fn instance_is_slot_own_chrome(inst: &InstanceInput, slot: [f32; 4]) -> bool {
    if !instance_has_dest_ink(inst) {
        return false;
    }
    if instance_rect_matches_slot(inst, slot) {
        return true;
    }
    let pad = inst.border_width.max(0.0);
    if pad <= 0.5 || inst.border_color[3] <= 0.05 {
        return false;
    }
    instance_rect_matches_slot(inst, padded_xywh(slot, pad))
}

/// Collect paints own chrome (fill + outline) *before* children. Never return the
/// full parent range: that would submit every descendant quad on hover:scale.
fn slot_own_chrome_replay(
    owner: &NodePaint,
    instances: &[InstanceInput],
    ui_scale: f32,
) -> (usize, usize) {
    let slot = scale_xywh(owner.bounds, ui_scale.max(0.0));
    let lo = owner.instance.0.min(instances.len());
    let hi = owner.instance.1.min(instances.len());
    let mut end = lo;
    for i in lo..hi {
        if instance_is_slot_own_chrome(&instances[i], slot) {
            end = i + 1;
        } else {
            break;
        }
    }
    (lo, end)
}

/// Own fill/outline prefix. Parent collect ranges include every descendant quad;
/// uploading that range for a thumb hole is the page-sized encode.
pub(crate) fn slot_own_chrome_range(
    owner: &NodePaint,
    instances: &[InstanceInput],
    ui_scale: f32,
) -> (usize, usize) {
    slot_own_chrome_replay(owner, instances, ui_scale)
}

/// Texture runs this slot owns. Parent collect ranges include every descendant
/// bake; walking each index against every paint is a full-page scan (inline
/// hue/alpha hover encode). Subtract descendant ranges instead.
pub(crate) fn for_each_own_textured_run(
    owner: &NodePaint,
    paints: &[NodePaint],
    textured_len: usize,
    mut f: impl FnMut(usize, usize),
) {
    let lo = owner.textured.0.min(textured_len);
    let hi = owner.textured.1.min(textured_len);
    if lo >= hi {
        return;
    }
    let mut cuts: Vec<(usize, usize)> = paints
        .iter()
        .filter(|q| {
            q.key != owner.key
                && !q.overlay
                && q.textured.1 > q.textured.0
                && q.textured.0 >= lo
                && q.textured.1 <= hi
                && is_nested_foreign_slot(owner, q)
        })
        .map(|q| (q.textured.0, q.textured.1))
        .collect();
    cuts.sort_unstable();
    let mut cursor = lo;
    for (a, b) in cuts {
        if a > cursor {
            f(cursor, a);
        }
        if b > cursor {
            cursor = b;
        }
    }
    if cursor < hi {
        f(cursor, hi);
    }
}

/// Keyed widget hole that does not own glyphs. Present redraws own chrome plus
/// the ramp. An absolute track is a page-stack slot; that must not replay every
/// page band and submit the whole glyphon run. Slots that own text (Latest)
/// stay on the ordered composite path.
pub(crate) fn region_replays_own_chrome_only(
    key: u64,
    bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(key, bounds, node_paints);
    if node_present_clips_visual_overflow(key, bounds, node_paints) {
        return false;
    }
    // A solid overlay over a raster cannot be repaired by src-overing own
    // chrome. That either erases the raster with an ancestor fill or blends
    // its antialiased fringe twice. Clear and replay the ordered page bands.
    if node_paints.iter().any(|slot| {
        slot_matches_dirty(key, bounds, slot, has_keyed)
            && slot.layered
            && slot.textured.0 == slot.textured.1
            && node_paints.iter().any(|underlay| {
                !underlay.overlay
                    && underlay.key != slot.key
                    && underlay.textured.0 < underlay.textured.1
                    && intersect_rects(bounds, underlay.bounds)
                        .and_then(|hit| match underlay.overflow_clip {
                            Some(clip) => intersect_rects(hit, clip),
                            None => Some(hit),
                        })
                        .is_some()
            })
    }) {
        return false;
    }
    // A layered texture leaf belongs to its painted composite. Its repair can
    // also touch the parent's border and the old background; replay those in
    // scene order, including the frame that returns the parent to its rest pose.
    if node_paints.iter().any(|leaf| {
        slot_matches_dirty(key, bounds, leaf, has_keyed)
            && leaf.layered
            && leaf.textured.0 < leaf.textured.1
            && node_paints.iter().any(|parent| {
                parent.layered
                    && parent.instance.0 < parent.instance.1
                    && is_nested_foreign_slot(parent, leaf)
            })
    }) {
        return false;
    }
    // Fade omitted the quads. Own-chrome then draws nothing and PartialLoad
    // keeps the last gray track. Page-band replay fills that hole with the
    // content that was under the bar. A flat Node clear of the same box
    // samples one row and erases the thumb.
    let emits = node_paints
        .iter()
        .any(|p| slot_matches_dirty(key, bounds, p, has_keyed) && slot_emits_paint(p));
    if !emits || slot_requires_ordered_replay(key, bounds, node_paints) {
        return false;
    }
    let has_nested_paint = node_paints.iter().any(|owner| {
        slot_matches_dirty(key, bounds, owner, true)
            && node_paints.iter().any(|child| {
                is_nested_foreign_slot(owner, child)
                    && ((child.instance.1 > child.instance.0 && child.instance != owner.instance)
                        || (child.textured.1 > child.textured.0
                            && child.textured != owner.textured)
                        || (child.chrome.1 > child.chrome.0 && child.chrome != owner.chrome))
            })
    });
    let keyed = node_paints.iter().any(|p| p.key == key);
    // A caret stroke inside a cell. Own-chrome src-overs the cell fill
    // (post-order, no CLEAR) and the translucent `bg-input/30` darkens.
    // Page-band Region clears the stroke, then draws the cell once.
    keyed
        && !has_nested_paint
        && !node_dirty_has_text(key, bounds, node_paints)
        && !caret_hole_inside_larger_slot(key, bounds, node_paints)
}

fn caret_hole_inside_larger_slot(key: u64, bounds: [f32; 4], node_paints: &[NodePaint]) -> bool {
    if bounds[2] > 8.0 || bounds[3] > 28.0 || bounds[2] <= 0.5 || bounds[3] < 8.0 {
        return false;
    }
    let cx = bounds[0] + bounds[2] * 0.5;
    let cy = bounds[1] + bounds[3] * 0.5;
    node_paints.iter().any(|p| {
        p.key != key
            && !p.overlay
            && p.bounds[2] > bounds[2] + 8.0
            && p.bounds[3] > bounds[3] + 4.0
            && cx >= p.bounds[0]
            && cy >= p.bounds[1]
            && cx < p.bounds[0] + p.bounds[2]
            && cy < p.bounds[1] + p.bounds[3]
    })
}

/// Region replay no longer submits the page glyphon run. The hole still has to
/// recapture glyphs it owns — a Select trigger's collect range includes the
/// value caption, so own-chrome is false and the fill clear would otherwise
/// leave the button blank.
pub(crate) fn region_needs_glyph_capture(
    dirty: &PaintDirty,
    node_paints: &[NodePaint],
    page_text: &[TextLayoutInfo],
) -> bool {
    dirty.kind == PaintDirtyKind::Region
        && !node_present_glyphs_for_dirty(dirty, page_text, node_paints).is_empty()
}

/// Own-chrome outset halo. Overflow clips descendants, not this node's box-shadow
/// (SidebarInset / Dialog / Drawer). Walk the chrome prefix only — the collect
/// range includes every descendant quad.
fn slot_own_outset_halo_pad(p: &NodePaint, instances: &[InstanceInput], ui_scale: f32) -> f32 {
    let slot = scale_xywh(p.bounds, ui_scale.max(0.0));
    let (lo, hi) = slot_own_chrome_replay(p, instances, ui_scale);
    let mut halo = 0.0_f32;
    for inst in &instances[lo..hi] {
        if !instance_has_outset_shadow(inst) || !instance_rect_matches_slot(inst, slot) {
            continue;
        }
        halo = halo.max(sdf_outset_shadow_pad(
            inst.shadow_blur,
            inst.shadow_spread,
            inst.shadow_offset,
            inst.border_width,
        ));
    }
    halo
}

fn matching_slots_own_halo(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> f32 {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints
        .iter()
        .filter(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed))
        .map(|p| slot_own_outset_halo_pad(p, instances, ui_scale))
        .fold(0.0, f32::max)
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

fn paints_has_keyed_slot(dirty_key: u64, dirty_bounds: [f32; 4], paints: &[NodePaint]) -> bool {
    crate::app::damage::paint_dirty_has_keyed_slot(
        dirty_key,
        dirty_bounds,
        paints.iter().map(|q| (q.key, q.bounds)),
    )
}

fn slot_matches_dirty(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    p: &NodePaint,
    has_keyed: bool,
) -> bool {
    crate::app::damage::paint_dirty_matches_present_slot(
        dirty_key,
        dirty_bounds,
        p.key,
        p.bounds,
        has_keyed,
    )
}

fn dirty_slot_instance_ranges(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Vec<(usize, usize)> {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints
        .iter()
        .filter(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed))
        .map(|p| p.instance)
        .filter(|(a, b)| *b > *a)
        .collect()
}

/// Card/page fill behind a dirty widget. Skips the widget's own instances
/// (Default's opaque chrome would otherwise report itself as the background).
#[cfg(test)]
pub(crate) fn node_present_behind_color(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    x: f32,
    y: f32,
) -> Option<[f32; 4]> {
    let dirty_inst = dirty_slot_instance_ranges(dirty_key, dirty_bounds, node_paints);
    opaque_fill_color_at_skipping(instances, x, y, |i| {
        dirty_inst.iter().any(|&(a, b)| i >= a && i < b)
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

/// Like [`clamp_last_present_prev`], but clip prev to this neighborhood instead of
/// keeping a same-key dock verbatim. Latest last-present is often the full-width
/// 100% bar (area ≤ 4× the pill); unioning it dest-resets the overlay rail.
pub(crate) fn clamp_last_present_prev_tight(
    prev: [f32; 4],
    dirty_bounds: [f32; 4],
) -> Option<[f32; 4]> {
    if prev[2] <= 0.0 || prev[3] <= 0.0 || dirty_bounds[2] <= 0.0 || dirty_bounds[3] <= 0.0 {
        return None;
    }
    let prev_area = prev[2] * prev[3];
    let dirty_area = dirty_bounds[2] * dirty_bounds[3];
    if prev_area > dirty_area * 4.0 + 1.0 {
        return None;
    }
    intersect_rects(
        prev,
        padded_xywh(dirty_bounds, LAST_PRESENT_PREV_MAX_OUTSET),
    )
}

/// Hide-settle leftover of a compact overlay. Keep the pill+halo; drop a
/// same-key 100% dock last-present that would dest-reset wrapping.
fn clamp_layered_hide_prev(prev: [f32; 4], slot: [f32; 4]) -> Option<[f32; 4]> {
    if prev[2] <= 0.0 || prev[3] <= 0.0 || slot[2] <= 0.0 || slot[3] <= 0.0 {
        return None;
    }
    if prev[2] > slot[2] + 2.0 * LAST_PRESENT_PREV_MAX_OUTSET + 8.0 {
        return None;
    }
    intersect_rects(prev, padded_xywh(slot, LAST_PRESENT_PREV_MAX_OUTSET))
}

/// Tightest ancestor overflow clip on slots matching this dirty (logical px).
pub(crate) fn node_present_overflow_clip(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    // Geometry matching intentionally rejects recycled/remote draw slots, but
    // must not discard the live owner's clip when a cached dirty pose is stale.
    let has_keyed = node_paints.iter().any(|p| p.key == dirty_key);
    let mut acc: Option<[f32; 4]> = None;
    for p in node_paints {
        if if has_keyed {
            p.key != dirty_key
        } else {
            !slot_matches_dirty(dirty_key, dirty_bounds, p, false)
        } {
            continue;
        }
        let Some(c) = p.overflow_clip else {
            continue;
        };
        if c[2] <= 0.0 || c[3] <= 0.0 {
            // Some(empty) is the intersection of ancestor clips after this
            // subtree has scrolled out. It must remain a hard rejection, not
            // turn into None (unclipped) and permit a Region to clear the header.
            return Some(c);
        }
        acc = Some(match acc {
            Some(prev) => intersect_rects(prev, c).unwrap_or([0.0; 4]),
            None => c,
        });
    }
    if acc.is_none() && !has_keyed {
        // Culled / scrolled-out widget only. A still-collected Input (keyed,
        // no own overflow) must not pick up a neighbor clip — that cropped the
        // Hard-ring unfocus wipe.
        acc = fallback_overflow_clip_for_unmatched(dirty_bounds, node_paints);
    }
    acc
}

/// Live `overflow_clip` that a culled dirty substantially overlaps.
///
/// Skip clips the dirty **fills** while still starting on-window — that is Match
/// island swap (wipe leftover title/desc). A scrolled-out parent `prev∪now`
/// also fills the scroller, but starts far above it (`y < 0`); those must clip
/// so dest-reset / scissor cannot punch 「基础」.
///
/// Pick the **largest** scroll/page clip (≥64px). Tiny neighbor overflow-hidden
/// boxes are not ancestor clips.
fn fallback_overflow_clip_for_unmatched(
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    let mut best: Option<[f32; 4]> = None;
    let mut best_area = 0.0f32;
    for p in node_paints {
        let Some(c) = p.overflow_clip else {
            continue;
        };
        // Scroll / page clips only. A 3px neighbor overflow-hidden sliver is
        // not an ancestor clip (Input unfocus Hard ring).
        if c[2] < 64.0 || c[3] < 64.0 {
            continue;
        }
        if intersect_rects(dirty_bounds, c).is_none() {
            continue;
        }
        let island_swap =
            dest_reset_fills_overflow_clip(dirty_bounds, Some(c)) && dirty_bounds[1] >= -8.0;
        if island_swap {
            continue;
        }
        let area = c[2] * c[3];
        if area > best_area {
            best_area = area;
            best = Some(c);
        }
    }
    best
}

/// True when node-present dest-reset is the page island (Match body ∩ scroller),
/// not a widget hover. Occupancy clip against the *new* page then punches leftover
/// bands of the previous page (Gallery 浮层 → 数据 ViewportModal ghost).
fn dest_reset_fills_overflow_clip(dirty_bounds: [f32; 4], overflow: Option<[f32; 4]>) -> bool {
    let Some(c) = overflow else {
        return false;
    };
    if c[2] < 64.0 || c[3] < 64.0 {
        return false;
    }
    let Some(hit) = intersect_rects(dirty_bounds, c) else {
        return false;
    };
    hit[2] * hit[3] >= c[2] * c[3] * 0.45
}

fn live_scroll_overflow_clips(node_paints: &[NodePaint]) -> impl Iterator<Item = [f32; 4]> + '_ {
    node_paints.iter().filter_map(|p| {
        let c = p.overflow_clip?;
        (c[2] >= 64.0 && c[3] >= 64.0).then_some(c)
    })
}

fn dirty_entirely_above_overflow(dirty: [f32; 4], clip: [f32; 4]) -> bool {
    dirty[1] + dirty[3] <= clip[1] + 1.0
}

fn prev_intersects_overflow(prev: [f32; 4], clip: [f32; 4]) -> bool {
    intersect_rects(prev, clip).is_some()
}

/// Clip-culled shimmer / spin whose *current* box sits in the title bar.
/// Dest-reset of that phantom box CLEARs 「基础」 and stamps Marker captions onto
/// whatever now occupies the live edge.
///
/// - Prev inside the ScrollArea: this frame just scrolled the widget out.
/// - Prev missing: last-present already dropped the culled key (next shimmer
///   tick, or Region dest-reset which never inherits a page prev). Still a
///   phantom — the title-bar pixels belong to the header, not this slot.
/// - Prev present but *not* in the overflow: Match island-swap leftover title.
///   Those must still wipe.
fn scrolled_out_top_into_title(
    dirty_bounds: [f32; 4],
    prev_bounds: Option<[f32; 4]>,
    node_paints: &[NodePaint],
) -> bool {
    let clips: Vec<[f32; 4]> = live_scroll_overflow_clips(node_paints).collect();
    let cx = dirty_bounds[0] + dirty_bounds[2] * 0.5;
    let cy = dirty_bounds[1] + dirty_bounds[3] * 0.5;
    clips.iter().any(|c| {
        // A scrolled-out node remains in its viewport's horizontal span.
        // Unrelated Forms viewports to the right must not reject sidebar hover.
        if dirty_bounds[0] + dirty_bounds[2] <= c[0] || dirty_bounds[0] >= c[0] + c[2] {
            return false;
        }
        if !dirty_entirely_above_overflow(dirty_bounds, *c) {
            return false;
        }
        // A halo strip above a nested scroller (textarea inside the forms page)
        // still sits in the page port. That is not the title bar; zeroing it
        // leaves the Hard ring after blur. Only a dirty outside every larger
        // clip is the scrolled-out phantom.
        let inside_larger_clip = clips.iter().any(|outer| {
            if outer[2] * outer[3] <= c[2] * c[3] + 1.0 {
                return false;
            }
            cx >= outer[0] && cy >= outer[1] && cx < outer[0] + outer[2] && cy < outer[1] + outer[3]
        });
        if inside_larger_clip {
            return false;
        }
        match prev_bounds.filter(|p| p[2] > 0.5 && p[3] > 0.5) {
            Some(prev) => prev_intersects_overflow(prev, *c),
            None => true,
        }
    })
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
    _chrome: &[InstanceInput],
    pad: f32,
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> Option<[f32; 4]> {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let layered = matching_slot_is_layered(dirty_key, dirty_bounds, node_paints);
    let s = ui_scale.max(0.0);
    let mut ink: Option<[f32; 4]> = None;
    let mut slot_visual: Option<[f32; 4]> = None;
    let mut matched = false;
    let mut sdf_ink = false;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
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
        for (off, inst) in instances[lo..hi].iter().enumerate() {
            if nested_descendant_owns(p, node_paints, lo + off, |q| q.instance) {
                continue;
            }
            let mut b = instance_paint_bounds(inst);
            if b[2] <= 0.0 || b[3] <= 0.0 {
                continue;
            }
            if !layered
                && transform_is_identity(&inst.transform)
                && p.bounds[2] > 0.0
                && p.bounds[3] > 0.0
            {
                // Include vertex AA (+2). Halo-only pad clipped pulse dest-reset
                // back to the layout box, leaving rounded AABB corners in retain.
                let extra = sdf_outset_shadow_pad(
                    inst.shadow_blur,
                    inst.shadow_spread,
                    inst.shadow_offset,
                    inst.border_width,
                ) * s;
                match intersect_rects(b, padded_xywh(scale_xywh(p.bounds, s), extra)) {
                    Some(hit) => b = hit,
                    None => continue,
                }
            }
            if instance_has_dest_ink(inst) || !transform_is_identity(&inst.transform) {
                sdf_ink = true;
            }
            // Transparent same-slot fill (lucide + spin) has no dest-ink. Still
            // wipe that box so dest-reset has a fill fallback to grow from when
            // the rotated texture arrives — otherwise reset_rect is None.
            // Skip text leaves: a flex-grown caret slot matching a transparent
            // instance would wipe the whole Input content box.
            if instance_has_dest_ink(inst)
                || !transform_is_identity(&inst.transform)
                || (p.text.0 >= p.text.1
                    && instance_rect_matches_slot(inst, scale_xywh(p.bounds, s)))
            {
                ink = Some(match ink {
                    Some(prev) => union_xywh(prev, b),
                    None => b,
                });
            }
        }
        let tlo = p.textured.0.min(textured.len());
        let thi = p.textured.1.min(textured.len());
        for (off, d) in textured[tlo..thi].iter().enumerate() {
            // Parent collect ranges include the lucide bake. Using that bake as
            // this wrap's dest-ink made `clamp_last_present_prev` clip a 125px
            // caption last-present to lucide+48px (64px hole → 「Oliver i」).
            if nested_descendant_owns(p, node_paints, tlo + off, |q| q.textured) {
                continue;
            }
            let b = textured_paint_bounds(d);
            if b[2] <= 0.0 || b[3] <= 0.0 {
                continue;
            }
            ink = Some(match ink {
                Some(prev) => union_xywh(prev, b),
                None => b,
            });
        }
        // I-beam chrome is a swapchain overlay after retain blit — not dest-ink.
    }
    if !matched {
        // Match island swap often recycles the Taffy id. `last-present` for that
        // key may now be a parent/inset slot (must not union). The dirty box is
        // still the snapshotted old island — wipe it or PartialLoad src-overs
        // leftover title/desc glyphs onto the new page (Gallery 基础).
        //
        // Scrolled-out shimmer / spin is also unmatched (clip-cull dropped the
        // slot). If it now sits *above* the live ScrollArea (title bar) while
        // last-present was inside the scroller, do not CLEAR 「基础」 / the
        // MessageScroller that occupies the live edge.
        if scrolled_out_top_into_title(dirty_bounds, prev_bounds, node_paints) {
            return None;
        }
        let r = scale_xywh(dirty_bounds, s);
        if r[2] <= 0.5 || r[3] <= 0.5 {
            return None;
        }
        let overflow = node_present_overflow_clip(dirty_key, dirty_bounds, node_paints);
        return match overflow {
            Some(c) if !dest_reset_fills_overflow_clip(dirty_bounds, overflow) => {
                intersect_rects(r, scale_xywh(c, s))
            }
            _ => Some(r),
        };
    }
    let overflow = node_present_overflow_clip(dirty_key, dirty_bounds, node_paints);
    // Text/caret slots: dest-ink is the I-beam (chrome) / glyphs, not the
    // flex-grown layout box. Falling back to `slot_visual` dest-reset the whole
    // Input content slot on every blink.
    //
    // Text-only bind_text / shimmer still need a wipe when last-present is
    // missing (zero same-box leaf, first present). Prefer `prev` when usable —
    // caret blink dirties the full text-leaf AABB; using that as wipe punches
    // the Input ring / focus halo every OFF frame.
    let mut acc = ink.or(if sdf_ink { slot_visual } else { None });
    if acc.is_none() && node_dirty_has_text(dirty_key, dirty_bounds, node_paints) {
        let dirty_phys = scale_xywh(dirty_bounds, s);
        let prev_usable = prev_bounds.is_some_and(|p| p[2] > 0.5 && p[3] > 0.5);
        // Transparent captions have no SDF ink to contribute their new pose.
        // Using only last-present crops the suffix after a flex sibling grows.
        if prev_usable
            && slot_visual.is_some_and(|now| {
                prev_bounds.is_some_and(|prev| {
                    crate::app::damage::node_present_slot_moved(prev, now.map(|v| v / s.max(1e-6)))
                })
            })
        {
            acc = Some(dirty_phys);
        }
        // `Some([x,y,0,0])` is caret-blink OFF last-present, not "never presented".
        // Falling back to the flex leaf AABB wipes placeholder on blur (失焦闪一下).
        // bind_text first present passes `prev=None` and still needs this wipe.
        let caret_off_empty_prev = prev_bounds.is_some() && !prev_usable;
        if !prev_usable && !caret_off_empty_prev && dirty_phys[2] > 0.5 && dirty_phys[3] > 0.5 {
            acc = Some(dirty_phys);
        }
    }
    if layered {
        // `PaintDirty.bounds` already includes CSS shadow pad (style, not
        // opacity-multiplied instance alpha). Hide settle has no dest-ink;
        // fade frames still need this union or occupancy clips the halo off.
        // Clamp to the slot plus the SDF halo it actually paints — a +48px
        // neighborhood of a full-width dock dirty punched wrapping
        // ("removed from the … you're already at") past the pill.
        let dirty_phys = scale_xywh(dirty_bounds, s);
        let halo = matching_slots_own_halo(dirty_key, dirty_bounds, node_paints, instances, s)
            .max(2.0 * s);
        let limited = match slot_visual {
            Some(sb) => layered_slot_with_halo_pad(sb, dirty_phys, halo),
            None => dirty_phys,
        };
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, limited),
            None => limited,
        });
    }
    let clamp_box = if layered {
        let slot = layered_overlay_slot_bounds(dirty_key, dirty_bounds, node_paints)
            .map(|b| scale_xywh(b, s))
            .or(slot_visual);
        let halo = matching_slots_own_halo(dirty_key, dirty_bounds, node_paints, instances, s)
            .max(2.0 * s);
        match slot {
            Some(sb) => layered_slot_with_halo_pad(sb, scale_xywh(dirty_bounds, s), halo),
            None => scale_xywh(dirty_bounds, s),
        }
    } else {
        // Recycle 4× is vs this slot's **ink**, not the flex collect box.
        // Clamping to `slot_visual` kept a stale full-leaf last-present (caret
        // blink OFF stored `[leaf.x, leaf.y, 0, 0]` then expand unioned it)
        // and dest-reset the surrounding run. I-beam last-present vs I-beam
        // ink stays inside 4× without a slot-sized clamp.
        acc.unwrap_or_else(|| scale_xywh(dirty_bounds, s))
    };
    // `clamp_last_present_prev` is in logical px; `clamp_box` may already be scaled.
    let clamp_logical = if s > 0.0 {
        [
            clamp_box[0] / s,
            clamp_box[1] / s,
            clamp_box[2] / s,
            clamp_box[3] / s,
        ]
    } else {
        clamp_box
    };
    if let Some(prev) = prev_bounds.and_then(|p| {
        if layered {
            if layered_overlay_slot_in_draw_list(dirty_key, dirty_bounds, node_paints) {
                clamp_last_present_prev_tight(p, clamp_logical)
            } else {
                let slot = layered_overlay_slot_bounds(dirty_key, dirty_bounds, node_paints)
                    .unwrap_or(dirty_bounds);
                clamp_layered_hide_prev(p, slot)
            }
        } else {
            clamp_last_present_prev(p, clamp_logical)
        }
    }) {
        // Keep the previous SDF halo. Insetting by own-halo left the last fade
        // frame in PartialLoad (stacked enter ring / hide ghost pill).
        let pb = scale_xywh(prev, s);
        acc = Some(match acc {
            Some(now) => union_xywh(now, pb),
            None => pb,
        });
    }
    // Dest-reset wipe is the slot's real ink (SDF halo / textured wrap) union
    // last-present. Do **not** grow that by RESET_PAD when prev is known —
    // `instance_paint_bounds` already includes vertex AA (+2), and +3px on top
    // punches `form_field` labels (`gap(4)`) while the Input ring blinks.
    //
    // Transparent / text-only slots must not grow at all (Kbd InputAdornment
    // 1px parent border). When `prev_bounds` is missing (key recycle / first
    // present), keep RESET_PAD so the **theme** focus-ring halo (Hard spread /
    // Soft blur) is still covered by current blur-1 ink.
    let wipe_pad = if sdf_ink && ink.is_some() && prev_bounds.is_none() {
        let (blur, spread) = crate::style::focus_ring_params(crate::style::focus_ring_style());
        pad.max(0.0)
            .max(sdf_painted_halo_pad(blur, spread, [0.0, 0.0]) * s)
    } else {
        0.0
    };
    acc.map(|r| padded_xywh(r, wipe_pad))
        .and_then(|r| match overflow {
            Some(c) => {
                // NodePaint stores the ancestor clip. A node's own shadow may
                // escape its own overflow, but never its parent's viewport.
                intersect_rects(r, scale_xywh(c, s))
            }
            None => Some(r),
        })
        .map(|r| {
            // Gallery overlays → 数据: reused ScrollArea dest-reset occupancy-clipped
            // against the new table and left ViewportModal in the emptied bands.
            // When this dirty already fills the overflow clip, siblings outside
            // that clip (sidebar / 8b) stay safe without occupancy shrink.
            if dest_reset_fills_overflow_clip(dirty_bounds, overflow) {
                r
            } else {
                clip_node_present_rect(
                    r,
                    dirty_key,
                    dirty_bounds,
                    node_paints,
                    s,
                    layered,
                    instances,
                )
            }
        })
        .filter(|r| r[2] > 0.5 && r[3] > 0.5)
}

fn range_contains_index(r: (usize, usize), i: usize) -> bool {
    i >= r.0 && i < r.1
}

fn slot_range_nested(child: (usize, usize), parent: (usize, usize)) -> bool {
    child.1 > child.0 && child.0 >= parent.0 && child.1 <= parent.1
}

fn slot_ranges_covered_by(inner: &NodePaint, outer: &NodePaint) -> bool {
    let mut any = false;
    for (channel, (ir, or)) in [
        (inner.instance, outer.instance),
        (inner.textured, outer.textured),
        (inner.text, outer.text),
        (inner.chrome, outer.chrome),
    ]
    .into_iter()
    .enumerate()
    {
        if ir.1 <= ir.0 {
            continue;
        }
        if or.1 <= or.0 {
            // I-beam chrome is collected on the text leaf, not duplicated on the
            // wrap. Treating that as "not nested" made the wrap own the caret
            // last-present; blur then dest-reset the wrap and flashed ⌘K / Kbd.
            if channel == 3 {
                continue;
            }
            return false;
        }
        any = true;
        if !slot_range_nested(ir, or) {
            return false;
        }
    }
    any
}

fn slots_share_box_center(a: [f32; 4], b: [f32; 4]) -> bool {
    let ca = rect_center(a);
    let cb = rect_center(b);
    (ca[0] - cb[0]).abs() <= 2.0 && (ca[1] - cb[1]).abs() <= 2.0
}

fn slot_paint_ranges_overlap(a: &NodePaint, b: &NodePaint) -> bool {
    [
        (a.instance, b.instance),
        (a.textured, b.textured),
        (a.text, b.text),
        (a.chrome, b.chrome),
    ]
    .into_iter()
    .any(|(a, b)| a.0 < b.1 && b.0 < a.1)
}

fn is_nested_foreign_slot(parent: &NodePaint, child: &NodePaint) -> bool {
    if child.key == parent.key || child.overlay {
        return false;
    }
    if !slot_ranges_covered_by(child, parent) {
        // Collect ranges are per-slot (I-beam chrome on the leaf only). Occupancy
        // still marks a descendant so the wrap does not own caret last-present.
        if occupancy_descendant_slot(parent, child) {
            return true;
        }
        let parent_box = node_paint_occupancy(parent);
        let child_box = node_paint_occupancy(child);
        let same_box = slots_share_box_center(parent_box, child_box)
            && (parent_box[2] - child_box[2]).abs() <= 2.5
            && (parent_box[3] - child_box[3]).abs() <= 2.5;
        if same_box {
            return parent.key > child.key;
        }
        return false;
    }
    if slot_ranges_covered_by(parent, child) {
        // Equal collect spans (MarkerIcon wrap has no own ink). NodeId order is
        // not wrap-vs-leaf: the Icon is often allocated first (lower key).
        // Skip the shared bake from the *wrap* last-present; never from a
        // spinning leaf whose visual already exceeds its Taffy rest box.
        //
        // Occupancy (rest) for the center test: Latest hide translates the pill
        // 8px, so visual centers diverge and the full-width dock used to union
        // that slide — dest-reset of a 320×39 bar punched the overlay rail.
        let parent_box = node_paint_occupancy(parent);
        let child_box = node_paint_occupancy(child);
        // Equal collect spans still include the child's bake (Marker row ⊃ lucide).
        // Centers differ because the icon sits at the start of the row — that is a
        // layout ancestor, not Latest's same-box wrap. Returning false here unioned
        // lucide+caption into the row last-present; idle spin then looked like the
        // row translated and dest-reset 「Oliver is typing...」 every tick.
        if occupancy_descendant_slot(parent, child)
            && !slots_share_box_center(parent_box, child_box)
        {
            return true;
        }
        if !slots_share_box_center(parent_box, child_box) {
            return false;
        }
        if !aabb_contains(parent_box, child_box) {
            // A smaller equal-span leaf cannot own its containing wrapper.
            // Keep this relation directional, including transformed overflow.
            return !aabb_contains(child_box, parent_box);
        }
        if occupancy_descendant_slot(parent, child) {
            return true;
        }
        // Occupancy (rest), not visual bounds. `animate-spin` grows only the
        // leaf `NodePaint.bounds` to the circumcircle; the MarkerIcon wrap
        // stays the unrotated size-4 box. Comparing visual sizes then misses
        // same-box wrappers, wrap last-present unions the bake, and idle
        // PartialLoad dest-resets the wrap — src-overing the spinner twice.
        let same_box = (parent_box[2] - child_box[2]).abs() <= 2.5
            && (parent_box[3] - child_box[3]).abs() <= 2.5;
        let much_larger = parent.bounds[2] > child.bounds[2] * 1.5 + 8.0
            || parent.bounds[3] > child.bounds[3] * 1.5 + 8.0;
        if much_larger {
            return true;
        }
        // Identity + vertex AA: instance ink is wrap+2. Unioning it into the
        // wrap last-present looks like a translation every idle spin tick.
        //
        // Text-only same-box (MarkerContent + text leaf): neither rest grows.
        // Treating both as "wrap" skipped glyph ink on *both* → last-present
        // `[x,y,0,0]` and dest-reset ∪ prev collapsed. Lower key keeps ink
        // (leaf allocated first); higher key wrap skips.
        if same_box {
            let parent_grown = parent
                .rest
                .is_some_and(|r| parent.bounds[2] > r[2] + 2.5 || parent.bounds[3] > r[3] + 2.5);
            if parent_grown {
                return false;
            }
            return parent.key > child.key;
        }
        return false;
    }
    // Proper subset = descendant collect, not spatial containment.
    // Overflow:visible + `animate-spin` grows `NodePaint.bounds` to the
    // circumcircle, which sticks out of a Marker row's 16px box. Requiring
    // `aabb_contains` here unions that bake into the row last-present,
    // expand-dirties the row, and nested-skips the icon dest-reset (idle 残影).
    true
}

fn occupancy_descendant_slot(parent: &NodePaint, child: &NodePaint) -> bool {
    let parent_box = node_paint_occupancy(parent);
    let child_box = node_paint_occupancy(child);
    if child_box[2] <= 0.5 || child_box[3] <= 0.5 {
        return false;
    }
    aabb_contains(parent_box, child_box)
        && (parent_box[2] > child_box[2] + 2.5 || parent_box[3] > child_box[3] + 2.5)
}

fn nested_foreign_owns(
    parent: &NodePaint,
    paints: &[NodePaint],
    i: usize,
    range: impl Fn(&NodePaint) -> (usize, usize),
) -> bool {
    paints
        .iter()
        .any(|q| range_contains_index(range(q), i) && is_nested_foreign_slot(parent, q))
}

/// Dest-reset ink of a wrap must not include a descendant's bake.
///
/// The test reference `nested_foreign_owns` also treats same-box wrap↔leaf (equal collect spans)
/// as foreign — that dropped the lucide dest-reset on the spinning leaf, while
/// the Marker row still clamped last-present (caption ∪ bake) to bake+48px.
fn nested_descendant_owns(
    parent: &NodePaint,
    paints: &[NodePaint],
    i: usize,
    range: impl Fn(&NodePaint) -> (usize, usize),
) -> bool {
    paints.iter().any(|q| {
        range_contains_index(range(q), i)
            && q.key != parent.key
            && !q.overlay
            && occupancy_descendant_slot(parent, q)
    })
}

fn slot_has_sdf_dest_ink(p: &NodePaint, instances: &[InstanceInput]) -> bool {
    let lo = p.instance.0.min(instances.len());
    let hi = p.instance.1.min(instances.len());
    instances[lo..hi].iter().any(instance_has_dest_ink)
}

/// Collect-slot visual box plus this frame's transformed instance / texture / glyph ink.
#[cfg(test)]
pub(crate) fn node_paint_visual_bounds(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> [f32; 4] {
    node_paint_visual_bounds_in(p, instances, textured, texts, chrome, &[])
}

/// Like [`node_paint_visual_bounds`], but skip nested *foreign* descendant ink.
///
/// Parent collect ranges include children. Unioning a child's hover:scale Bake
/// into the parent last-present makes `expand_paint_dirties_for_translated_slots`
/// dest-reset an overflow:visible pill as a sharp rect (reaction bar chrome).
/// Same-box wrappers (MarkerIcon around Icon) are nested too — the leaf slot
/// owns the lucide bake so idle spin cannot dirty-and-redraw the wrap.
#[cfg(test)]
pub(crate) fn node_paint_visual_bounds_in(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    paints: &[NodePaint],
) -> [f32; 4] {
    node_paint_visual_bounds_with_ownership(p, instances, textured, texts, chrome, |channel, i| {
        nested_foreign_owns(p, paints, i, |q| match channel {
            0 => q.instance,
            1 => q.textured,
            _ => q.text,
        })
    })
}

fn node_paint_visual_bounds_with_ownership(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    foreign_owns: impl Fn(usize, usize) -> bool,
) -> [f32; 4] {
    // Text/caret leaves have no SDF. Starting from the flex `w(100%)` layout box
    // made last-present (and dest-reset ∪ prev) the whole Input content slot —
    // blink then punched the rounded stroke and the sibling clear X.
    let mut acc = if slot_has_sdf_dest_ink(p, instances) && p.bounds[2] > 0.0 && p.bounds[3] > 0.0 {
        Some(p.bounds)
    } else {
        None
    };
    let lo = p.instance.0.min(instances.len());
    let hi = p.instance.1.min(instances.len());
    for (off, inst) in instances[lo..hi].iter().enumerate() {
        if foreign_owns(0, lo + off) {
            continue;
        }
        // Layered last-present must cover the SDF halo so hide-settle dest-reset
        // can erase it. Occupancy still keeps the overlay rail out of the wipe.
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
    for (off, d) in textured[tlo..thi].iter().enumerate() {
        if foreign_owns(1, tlo + off) {
            continue;
        }
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
    //
    // I-beam chrome is a swapchain overlay after retain blit — not last-present.
    // Caret-leaf glyphs stay here so typing dest-reset can wipe the run. Blink
    // never dest-resets (`paint_dirty_is_caret_overlay`).
    let glo = p.text.0.min(texts.len());
    let ghi = p.text.1.min(texts.len());
    let _ = chrome;
    for (off, n) in texts[glo..ghi].iter().enumerate() {
        if foreign_owns(2, glo + off) {
            continue;
        }
        let Some(b) = text_ink_in_slot(n, p.bounds) else {
            continue;
        };
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    // Text/caret leaves have no SDF. Falling back to the flex `w(100%)` box made
    // last-present (and dest-reset ∪ prev) the whole Input content slot.
    let mut acc = match acc {
        Some(v) => v,
        None if slot_has_sdf_dest_ink(p, instances) && p.bounds[2] > 0.0 && p.bounds[3] > 0.0 => {
            p.bounds
        }
        None => [p.bounds[0], p.bounds[1], 0.0, 0.0],
    };
    if acc[2] <= 0.0 || acc[3] <= 0.0 {
        return acc;
    }
    if let Some(c) = p.overflow_clip {
        // Empty/disjoint clips mean no pixels were presented, not "unclipped".
        acc = intersect_rects(acc, c).unwrap_or([0.0; 4]);
    }
    acc
}

/// Glyph AABB that dest-reset / last-present may follow: shadow outset yes,
/// layout overflow into a neighbor slot no.
fn text_ink_in_slot(n: &TextLayoutInfo, slot: [f32; 4]) -> Option<[f32; 4]> {
    let mut b = text_paint_bounds(n);
    if b[2] <= 0.0 || b[3] <= 0.0 {
        return None;
    }
    // Input text leaf is `w(100%)` of the remaining flex slot. Glyphon's box is
    // that layout width; dest-reset of the empty stretch punched the rounded
    // stroke and the sibling clear X. Last-present follows painted advance.
    if n.style.overflow.allows_caret_scroll() {
        if let Some(ink_w) = text_run_ink_width(n) {
            b[2] = b[2].min(ink_w);
        }
    }
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

/// Painted advance of a nowrap edit run (not the flex layout box).
fn text_run_ink_width(n: &TextLayoutInfo) -> Option<f32> {
    if n.style.font_family.as_deref() == Some("lucide") {
        return None;
    }
    if let Some(p) = n.prepared.as_ref() {
        return Some(crate::text::intrinsic_width(p).max(0.0));
    }
    if n.text.is_empty() {
        return None;
    }
    Some(crate::text::measure_prefix_advance(
        &n.text,
        n.style.size_resolved(),
        n.style.weight_resolved(),
        n.height.max(1.0),
    ))
}

/// Page-node visual slots for `last_present_node_bounds` (logical px).
/// Call **before** `scale_instances_in_place` / `take` — dest-reset scales prev by `ui_scale`.
#[cfg(test)]
pub(crate) fn last_present_visual_slots(
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> Vec<(u64, [f32; 4])> {
    last_present_visual_slots_with_raster_owners(node_paints, instances, textured, texts, chrome).0
}

pub(crate) fn updated_texture_paint_nodes(
    paints: &[NodePaint],
    textured: &[ScaledTexturedDraw],
    updated: &HashSet<String>,
    visual_slots: &[(u64, [f32; 4])],
) -> Vec<PaintDirty> {
    if updated.is_empty() {
        return Vec::new();
    }
    paints
        .iter()
        .filter_map(|paint| {
            let changed = (paint.textured.0.min(textured.len())
                ..paint.textured.1.min(textured.len()))
                .any(|index| {
                    updated.contains(&textured[index].texture_key)
                        && !paints.iter().any(|child| {
                            range_contains_index(child.textured, index)
                                && is_nested_foreign_slot(paint, child)
                        })
                });
            changed.then_some(PaintDirty {
                key: paint.key,
                bounds: visual_slots
                    .iter()
                    .find(|(key, _)| *key == paint.key)
                    .map(|(_, bounds)| *bounds)
                    .unwrap_or(paint.bounds),
                kind: if paint.overlay {
                    PaintDirtyKind::Overlay
                } else {
                    PaintDirtyKind::Node
                },
            })
        })
        .collect()
}

pub(crate) fn last_present_visual_slots_with_raster_owners(
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> (Vec<(u64, [f32; 4])>, HashSet<u64>) {
    // Index each payload's candidate owners once. Testing a primitive's
    // ownership used to scan every paint slot, including unrelated siblings.
    let mut owners =
        [instances.len(), textured.len(), texts.len()].map(|len| vec![Vec::new(); len]);
    for p in node_paints.iter().filter(|p| !p.overlay) {
        for (channel, (lo, hi)) in [p.instance, p.textured, p.text].into_iter().enumerate() {
            let hi = hi.min(owners[channel].len());
            for candidates in &mut owners[channel][lo.min(hi)..hi] {
                candidates.push(p);
            }
        }
    }
    let mut raster_owners = HashSet::new();
    let slots = node_paints
        .iter()
        .filter(|p| !p.overlay)
        .filter_map(|p| {
            let foreign_owns = |channel: usize, i: usize| {
                owners[channel][i]
                    .iter()
                    .any(|q| is_nested_foreign_slot(p, q))
            };
            if (p.textured.0.min(textured.len())..p.textured.1.min(textured.len()))
                .any(|i| !foreign_owns(1, i))
                || (p.text.0.min(texts.len())..p.text.1.min(texts.len()))
                    .any(|i| !foreign_owns(2, i))
            {
                raster_owners.insert(p.key);
            }
            let b = node_paint_visual_bounds_with_ownership(
                p,
                instances,
                textured,
                texts,
                chrome,
                foreign_owns,
            );
            // Skip phantom `[layout_origin, 0×0]` (caret blink OFF). Recording that
            // origin made the next I-beam look translated from the flex leaf.
            if b[2] <= 0.5 || b[3] <= 0.5 {
                None
            } else {
                Some((p.key, b))
            }
        })
        .collect();
    #[cfg(test)]
    if std::env::var_os("JADIX_VISUAL_SLOTS_REFERENCE").is_some() {
        return (
            last_present_visual_slots_reference(node_paints, instances, textured, texts, chrome),
            raster_owners,
        );
    }
    (slots, raster_owners)
}

#[cfg(test)]
pub(crate) fn last_present_visual_slots_reference(
    paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
) -> Vec<(u64, [f32; 4])> {
    paints
        .iter()
        .filter(|p| !p.overlay)
        .filter_map(|p| {
            let b = node_paint_visual_bounds_in(p, instances, textured, texts, chrome, paints);
            (b[2] > 0.5 && b[3] > 0.5).then_some((p.key, b))
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
        PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region => None,
    }
}

/// Wave 51 miss → Region, except clip-culled overflow widgets (spin / shimmer).
///
/// Those still have a live ScrollArea. Promoting them skips Node dest-reset
/// (and its overflow clip) so CLEAR punches 「基础」 and capture stamps
/// Message / Shimmer captions into the title bar. Island-swap has no fallback
/// overflow (`dest_reset_fills` + on-window) and stays Region.
pub(crate) fn escalate_dirties_for_node_present(
    dirties: &mut [PaintDirty],
    node_paints: &[NodePaint],
    last_present: &[(u64, [f32; 4])],
) {
    for dirty in dirties {
        if dirty.kind != PaintDirtyKind::Node {
            continue;
        }
        let found_current = node_paints.iter().any(|p| !p.overlay && p.key == dirty.key);
        let had_previous = last_present.iter().any(|(k, _)| *k == dirty.key);
        let next = crate::app::damage::escalate_node_dest_reset_kind(
            dirty.kind,
            found_current,
            had_previous,
        );
        if next == PaintDirtyKind::Region {
            // Clip-culled overflow widgets keep Node dest-reset (overflow clip).
            // A box that has already left the scroller into the title bar has
            // *no* dirty∩clip, so overflow lookup fails — still keep Node so
            // dest-reset / scissor can skip the phantom instead of Region-CLEAR
            // punching 「基础」.
            let prev = last_present
                .iter()
                .find(|(k, _)| *k == dirty.key)
                .map(|(_, b)| *b);
            if node_present_overflow_clip(dirty.key, dirty.bounds, node_paints).is_some()
                || scrolled_out_top_into_title(dirty.bounds, prev, node_paints)
            {
                continue;
            }
        }
        dirty.kind = next;
    }
}

/// Finalize tiers before deciding Load/Clear or capturing any glyphs. A live
/// slot does not imply that its previous pixels have a recoverable backdrop.
pub(crate) fn restore_culled_dirty_clips(
    root: &crate::core::tree::Element,
    paints: &mut Vec<NodePaint>,
    dirties: &[PaintDirty],
) {
    use crate::core::tree::{child_scroll_accumulation, overflow_clip_axes, visual_paint_aabb};
    let mut missing: std::collections::HashSet<_> = dirties
        .iter()
        .filter(|d| matches!(d.kind, PaintDirtyKind::Node | PaintDirtyKind::Region))
        .map(|d| d.key)
        .collect();
    for p in paints.iter() {
        missing.remove(&p.key);
    }
    if missing.is_empty() {
        return;
    }
    let mut stack = vec![(root, [0.0, 0.0], None::<[f32; 4]>, false)];
    while let Some((el, mut scroll, mut clip, layered)) = stack.pop() {
        if crate::layout::layout_tree::escapes_ancestor_overflow(el) {
            scroll = [0.0, 0.0];
            clip = None;
        }
        let layered = layered
            || el.style.z_index != 0
            || el.style.position == taffy::style::Position::Absolute;
        let bounds = visual_paint_aabb(el, scroll);
        if let Some(key) = el
            .layout_node
            .map(u64::from)
            .filter(|key| missing.remove(key))
        {
            if let Some(clip) = clip {
                // Metadata only: the culled subtree owns no submitted draws.
                // Its retained ink still owes an erase bounded by its ancestors.
                paints.push(NodePaint {
                    key,
                    bounds,
                    overflow_clip: Some(clip),
                    overlay: false,
                    layered,
                    instance: (0, 0),
                    textured: (0, 0),
                    text: (0, 0),
                    chrome: (0, 0),
                    paint_ops: (0, 0),
                    rest: Some(bounds),
                });
            }
            if missing.is_empty() {
                break;
            }
        }
        let (cx, cy) = overflow_clip_axes(el);
        if cx || cy {
            let mut c = clip.unwrap_or([-1.0e7, -1.0e7, 2.0e7, 2.0e7]);
            for axis in 0..2 {
                if (axis == 0 && cx) || (axis == 1 && cy) {
                    let end = (c[axis] + c[axis + 2]).min(bounds[axis] + bounds[axis + 2]);
                    c[axis] = c[axis].max(bounds[axis]);
                    c[axis + 2] = (end - c[axis]).max(0.0);
                }
            }
            clip = Some(c);
        }
        let scroll = child_scroll_accumulation(el, scroll);
        for child in &el.children {
            stack.push((child, scroll, clip, layered));
        }
    }
}

/// Metadata restored for a clipped-out owner is not a drawable slot. Without
/// previous pixels there is no backdrop to reconstruct or sibling ink to scan.
fn owner_is_only_culled_metadata(key: u64, paints: &[NodePaint]) -> bool {
    let mut found = false;
    for p in paints.iter().filter(|p| p.key == key) {
        found = true;
        if p.instance.0 != p.instance.1
            || p.textured.0 != p.textured.1
            || p.text.0 != p.text.1
            || p.chrome.0 != p.chrome.1
            || p.paint_ops.0 != p.paint_ops.1
            || p.overflow_clip
                .is_none_or(|clip| intersect_rects(p.bounds, clip).is_some())
        {
            return false;
        }
    }
    found
}

/// Intersecting foreground and painted layered ancestors require scene-order repair.
fn slot_requires_ordered_replay(key: u64, bounds: [f32; 4], node_paints: &[NodePaint]) -> bool {
    let has_keyed = paints_has_keyed_slot(key, bounds, node_paints);
    node_paints
        .iter()
        .filter(|host| {
            slot_matches_dirty(key, bounds, host, has_keyed) && host.paint_ops.0 < host.paint_ops.1
        })
        .any(|host| {
            let painted_ancestor_start = node_paints
                .iter()
                .filter(|parent| {
                    parent.layered
                        && parent.instance.0 < host.instance.0
                        && slot_ranges_covered_by(host, parent)
                })
                .map(|parent| parent.paint_ops.0)
                .max();
            node_paints.iter().any(|peer| {
                !peer.overlay
                    && peer.key != host.key
                    && (peer.paint_ops.0 >= host.paint_ops.1
                        || painted_ancestor_start.is_some_and(|start| peer.paint_ops.1 <= start))
                    && peer.paint_ops.0 < peer.paint_ops.1
                    && !(intersect_rects(node_paint_occupancy(host), node_paint_occupancy(peer))
                        .is_none()
                        && crate::app::damage::rect_contains_rect(
                            padded_xywh(node_paint_occupancy(host), 1.0),
                            host.bounds,
                        )
                        && crate::app::damage::rect_contains_rect(
                            padded_xywh(node_paint_occupancy(peer), 1.0),
                            peer.bounds,
                        ))
                    && intersect_rects(host.bounds, peer.bounds)
                        .and_then(|hit| match peer.overflow_clip {
                            Some(clip) => intersect_rects(hit, clip),
                            None => Some(hit),
                        })
                        .and_then(|hit| match host.overflow_clip {
                            Some(clip) => intersect_rects(hit, clip),
                            None => Some(hit),
                        })
                        .is_some()
            })
        })
}

/// Compact layered overlay (Latest). Promoting it to Region window-clears the
/// halo AABB with `clear_color` — the enter white rectangle. A tall stacking
/// context that owns descendant text still takes that path: its box is the
/// page, not a hole. The pill stays Node when the hole can be reconstructed
/// (first paint src-overs retain; later frames dest-reset only the ink).
/// Wrapping in that wipe is the underlay of the same hole, drawn before the
/// pill. A pose that has left the cell is still a Region hole.
///
/// A layered slot with no current quads (overlay scrollbar settle-to-zero)
/// must NOT take this path. Node dest-reset then flat-fills the track with
/// one sampled row color and skips the thumb. That hole is a Region whose
/// page replay restores the content the bar covered.
fn layered_cell_keeps_node_dest_reset(
    key: u64,
    bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    last_present: &[(u64, [f32; 4])],
    ui_scale: f32,
) -> bool {
    if !matching_slot_is_layered(key, bounds, node_paints)
        || slot_requires_ordered_replay(key, bounds, node_paints)
    {
        return false;
    }
    let prev = last_present_prev_for_dirty(PaintDirtyKind::Node, key, last_present);
    if prev.is_some_and(|old| crate::app::damage::paint_poses_separated(old, bounds)) {
        return false;
    }
    let has_keyed = paints_has_keyed_slot(key, bounds, node_paints);
    if node_paints.iter().any(|owner| {
        slot_matches_dirty(key, bounds, owner, has_keyed)
            && slot_has_sdf_dest_ink(owner, instances)
            && node_paints
                .iter()
                .any(|child| is_nested_foreign_slot(owner, child) && slot_emits_paint(child))
            && {
                let lo = owner.instance.0.min(instances.len());
                let hi = owner.instance.1.min(instances.len());
                let visual = instances[lo..hi]
                    .iter()
                    .enumerate()
                    .filter(|(offset, _)| {
                        !nested_foreign_owns(owner, node_paints, lo + offset, |paint| {
                            paint.instance
                        })
                    })
                    .map(|(_, instance)| {
                        scale_xywh(instance_paint_bounds(instance), 1.0 / ui_scale.max(1e-6))
                    })
                    .fold(owner.bounds, union_xywh);
                owner
                    .rest
                    .into_iter()
                    .map(|old| (owner.bounds, old))
                    .chain(prev.map(|old| (visual, old)))
                    .any(|(now, old)| {
                        ((now[0] - old[0]).abs() > 0.01 || (now[1] - old[1]).abs() > 0.01)
                            && (now[2] - old[2]).abs() < 0.01
                            && (now[3] - old[3]).abs() < 0.01
                    })
            }
    }) {
        // A translated composite needs one ordered repair even when every
        // child is still Node and the old/new footprints overlap. Separate
        // ink wipes clip the old rounded AA fringe and can erase a child's
        // just-replayed glyphs. Compare rest too: a follow-up frame at the same
        // translated pose must not revert to those independent wipes.
        return false;
    }
    // Viewport-sized stacking bands keep Region. A short pill that owns its
    // label does not: window-clearing that AABB is the white rectangle.
    if node_dirty_has_text(key, bounds, node_paints) {
        let Some(slot) = layered_overlay_slot_bounds(key, bounds, node_paints) else {
            return false;
        };
        if slot[3] > 56.0 {
            return false;
        }
    }
    let has_keyed = paints_has_keyed_slot(key, bounds, node_paints);
    node_paints.iter().any(|p| {
        slot_matches_dirty(key, bounds, p, has_keyed)
            && (slot_has_sdf_dest_ink(p, instances)
                || p.text.0 < p.text.1
                || p.textured.0 < p.textured.1)
    })
}

/// How far `occ` extends past layout stamps. A full-window scrim is hundreds of
/// px; panel shadows in this UI stay inside the blur (≤24). Cap the fringe so
/// a scrim is not treated as shadow.
fn portal_shadow_fringe_pad(occ: [f32; 4], stamps: &[(u64, [f32; 4])]) -> f32 {
    let mut extent = 0.0_f32;
    for (_, stamp) in stamps {
        extent = extent.max((stamp[0] - occ[0]).max(0.0));
        extent = extent.max((stamp[1] - occ[1]).max(0.0));
        extent = extent.max(((occ[0] + occ[2]) - (stamp[0] + stamp[2])).max(0.0));
        extent = extent.max(((occ[1] + occ[3]) - (stamp[1] + stamp[3])).max(0.0));
    }
    if extent > 48.0 {
        32.0
    } else {
        extent.max(16.0)
    }
}

/// Region-replay only when the widget pokes through a scrim, not when it merely
/// sits in a panel's shadow.
///
/// Empty `stamps` keeps the old occupancy rule (callers that have not threaded
/// layout boxes). With stamps, a hit covered by the shadow pad stays `Node`:
/// halo-preserve clears the trigger face without reshaping the popover.
pub(crate) fn portal_occupancy_requires_region(
    logical: [f32; 4],
    occ: [f32; 4],
    stamps: &[(u64, [f32; 4])],
) -> bool {
    let Some(hit) = intersect_rects(logical, occ) else {
        return false;
    };
    if stamps.is_empty() {
        return true;
    }
    let pad = portal_shadow_fringe_pad(occ, stamps);
    let mut rest = vec![hit];
    for (_, stamp) in stamps {
        let padded = padded_xywh(*stamp, pad);
        rest = rest
            .into_iter()
            .flat_map(|r| crate::app::damage::subtract_rect(r, padded))
            .filter(|r| r[2] > 0.5 && r[3] > 0.5)
            .collect();
    }
    !rest.is_empty()
}

#[cfg(test)]
pub(crate) fn prepare_node_present_dirties(
    dirties: &mut Vec<PaintDirty>,
    node_paints: &[NodePaint],
    last_present: &[(u64, [f32; 4])],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    chrome: &[InstanceInput],
    ui_scale: f32,
    overlay: Option<[f32; 4]>,
) {
    prepare_node_present_dirties_ex(
        dirties,
        node_paints,
        last_present,
        instances,
        textured,
        chrome,
        ui_scale,
        overlay,
        &[],
    );
}

/// Prepare node dirties with portal layout stamps.
///
/// `overlay` is the visual halo (shadow included). A trigger under an open
/// popover intersects that halo every `transition-colors` tick. Escalating
/// those frames to Region replays the popover and reshapes its glyphs — the
/// hover-switch hitch — and src-overs the shadow onto the caption. Stamps
/// let the fringe stay on the Node path (halo preserved, face still cleared).
pub(crate) fn prepare_node_present_dirties_ex(
    dirties: &mut Vec<PaintDirty>,
    node_paints: &[NodePaint],
    last_present: &[(u64, [f32; 4])],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    chrome: &[InstanceInput],
    ui_scale: f32,
    overlay: Option<[f32; 4]>,
    portal_stamps: &[(u64, [f32; 4])],
) {
    escalate_dirties_for_node_present(dirties.as_mut_slice(), node_paints, last_present);
    let mut leftover_poses: Vec<PaintDirty> = Vec::new();
    for dirty in dirties.iter_mut() {
        if dirty.kind != PaintDirtyKind::Node
            || paint_dirty_is_caret_overlay(dirty, node_paints, instances)
        {
            continue;
        }
        let prev = last_present_prev_for_dirty(dirty.kind, dirty.key, last_present);
        if owner_is_only_culled_metadata(dirty.key, node_paints) {
            if let Some(previous) = prev {
                dirty.bounds = previous;
                dirty.kind = PaintDirtyKind::Region;
            }
            continue;
        }
        let visual = node_paints
            .iter()
            .filter(|p| !p.overlay && p.key == dirty.key)
            .map(|p| p.bounds)
            .reduce(union_xywh);
        if let Some(visual) = visual {
            let coverage = prev
                .filter(|old| !crate::app::damage::paint_poses_separated(*old, visual))
                .map(|old| union_xywh(visual, old))
                .unwrap_or(visual);
            if node_present_clips_visual_overflow(dirty.key, coverage, node_paints) {
                // A sibling's layout box is not an overflow clip. Repair the
                // old/new transformed ink and replay intersecting paint in scene
                // order, as scrolling does, rather than cropping the rotation.
                dirty.bounds =
                    padded_xywh(union_xywh(dirty.bounds, coverage), 2.0 / ui_scale.max(1e-6));
                dirty.kind = PaintDirtyKind::Region;
                continue;
            }
        }
        if matching_slot_is_layered(dirty.key, dirty.bounds, node_paints)
            || slot_requires_ordered_replay(dirty.key, dirty.bounds, node_paints)
        {
            if layered_cell_keeps_node_dest_reset(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                last_present,
                ui_scale,
            ) && (layered_src_over_skips_dest_reset(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                prev,
            ) || node_present_dest_reset_full(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                textured,
                chrome,
                ui_scale,
                prev,
            )
            .is_some_and(|reset| reset.clear[3] >= 0.999))
            {
                // Underlay can be reconstructed (src-over or dest-reset). Stay
                // Node so the clear is the ink wipe, not a window-colored AABB.
                continue;
            }
            // No reconstructable hole: empty quads, or a last pose that no
            // longer touches this box. Region replays the page. Unioning a
            // separated pose into this box paints a column of window clear.
            let mut bounds = dirty.bounds;
            let local_prev =
                prev.filter(|old| !crate::app::damage::paint_poses_separated(*old, dirty.bounds));
            if let Some(old) = prev {
                if local_prev.is_none() {
                    leftover_poses.push(PaintDirty {
                        key: dirty.key,
                        // The old pose needs the same raster fringe as the new
                        // one, or a separated drag leaves its rounded outline.
                        bounds: padded_xywh(old, 2.0 / ui_scale.max(1e-6)),
                        kind: PaintDirtyKind::Region,
                    });
                } else {
                    bounds = union_xywh(bounds, old);
                }
            }
            if let Some(rect) = node_present_reset_rect(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                textured,
                chrome,
                NODE_PRESENT_RESET_PAD_PHYSICAL,
                ui_scale,
                local_prev,
            ) {
                let s = ui_scale.max(1e-6);
                bounds = union_xywh(bounds, [rect[0] / s, rect[1] / s, rect[2] / s, rect[3] / s]);
            }
            // Rounded SDF coverage reaches beyond the nominal box even at
            // low opacity, when the ordinary dest-ink test ignores the fill.
            // Keep that fringe through the final transparent frame as well.
            dirty.bounds = padded_xywh(bounds, 2.0 / ui_scale.max(1e-6));
            dirty.kind = PaintDirtyKind::Region;
            continue;
        }
        let src_over = layered_src_over_skips_dest_reset(
            dirty.key,
            dirty.bounds,
            node_paints,
            instances,
            prev,
        );
        if src_over && overlay.is_none() {
            continue;
        }
        let reset = node_present_dest_reset_full(
            dirty.key,
            dirty.bounds,
            node_paints,
            instances,
            textured,
            chrome,
            ui_scale,
            prev,
        );
        let reset_rect = reset.as_ref().map(|r| r.rect).or_else(|| {
            node_present_reset_rect(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                textured,
                chrome,
                NODE_PRESENT_RESET_PAD_PHYSICAL,
                ui_scale,
                prev,
            )
        });
        let s = ui_scale.max(1e-6);
        let logical = reset_rect
            .map(|r| [r[0] / s, r[1] / s, r[2] / s, r[3] / s])
            .unwrap_or(dirty.bounds);
        if reset.as_ref().is_some_and(|reset| reset.clear[3] < 0.999) {
            dirty.kind = PaintDirtyKind::Region;
            dirty.bounds = logical;
            continue;
        }
        if overlay.is_some_and(|occ| portal_occupancy_requires_region(logical, occ, portal_stamps))
        {
            // Scrim (occupancy far outside the panel's shadow pad). A Node
            // dest-reset would punch it. The shadow fringe of an anchored
            // panel is not a scrim: Node halo-preserve clears the trigger
            // face and leaves the fringe in retain.
            dirty.kind = PaintDirtyKind::Region;
            dirty.bounds = logical;
            continue;
        }
        if src_over || reset.is_some() {
            continue;
        }
        dirty.bounds = logical;
        dirty.kind = PaintDirtyKind::Region;
    }
    dirties.extend(leftover_poses);
    // A concurrent in-flow caption must not be flushed after a Region that
    // already composited an overlapping Latest layer. Promote connected dirty
    // coverage too; unrelated nodes retain their independent Node fast path.
    // A compact layered slot must join that ordered repair too: a moving icon
    // can already be Region while its card stays Node. Clearing the card after
    // the icon's Region erases the freshly restored glyphs and leaves a trail.
    // Layered / unmatched regions still absorb overlapping captions. A keyed
    // thumb hole (textured ramp, no glyphs) must not promote the full-width
    // track into a page Region — that is the encode spike.
    let mut regions: Vec<_> = dirties
        .iter()
        .filter(|d| match d.kind {
            PaintDirtyKind::OverlayRestore => true,
            PaintDirtyKind::Region => !region_replays_own_chrome_only(d.key, d.bounds, node_paints),
            _ => false,
        })
        .map(|d| d.bounds)
        .collect();
    let mut cursor = 0;
    while cursor < regions.len() {
        let region = regions[cursor];
        cursor += 1;
        for dirty in dirties.iter_mut() {
            if dirty.kind != PaintDirtyKind::Node
                || paint_dirty_is_caret_overlay(dirty, node_paints, instances)
            {
                continue;
            }
            let prev = last_present_prev_for_dirty(dirty.kind, dirty.key, last_present);
            if prev.is_none() && owner_is_only_culled_metadata(dirty.key, node_paints) {
                continue;
            }
            let candidate = prev
                .filter(|r| !crate::app::damage::paint_poses_separated(*r, dirty.bounds))
                .map(|r| union_xywh(r, dirty.bounds))
                .unwrap_or(dirty.bounds);
            if intersect_rects(candidate, region).is_none() {
                continue;
            }
            let mut bounds = candidate;
            if let Some(rect) = node_present_reset_rect(
                dirty.key,
                dirty.bounds,
                node_paints,
                instances,
                textured,
                chrome,
                NODE_PRESENT_RESET_PAD_PHYSICAL,
                ui_scale,
                prev,
            ) {
                let s = ui_scale.max(1e-6);
                bounds = union_xywh(bounds, [rect[0] / s, rect[1] / s, rect[2] / s, rect[3] / s]);
            }
            dirty.kind = PaintDirtyKind::Region;
            // Match the direct layered Region path's raster fringe. The SDF
            // paints beyond its nominal box, including at the previous pose.
            dirty.bounds = if matching_slot_is_layered(dirty.key, bounds, node_paints) {
                padded_xywh(bounds, 2.0 / ui_scale.max(1e-6))
            } else {
                bounds
            };
            regions.push(dirty.bounds);
        }
    }
    crate::app::damage::sort_paint_dirty_nodes(dirties);
}

/// `animate-spin` / rotate grows `NodePaint.bounds` to the circumcircle while `rest`
/// stays the Taffy box. Nested dest-reset skip would let a larger hover dirty
/// (page-wide CursorMoved on a `hover:scale` tile) omit that wipe — retain freezes.
pub(crate) fn slot_visual_exceeds_rest(key: u64, paints: &[NodePaint]) -> bool {
    paints.iter().any(|p| {
        p.key == key
            && !p.overlay
            && p.rest
                .is_some_and(|r| p.bounds[2] > r[2] + 2.5 || p.bounds[3] > r[3] + 2.5)
    })
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
    node_present_scissor_logical_ex(
        kind,
        dirty_key,
        dirty_bounds,
        prev_slots,
        node_paints,
        scissor_pad,
        true,
    )
}

/// Scroll blit pins must dest-reset copied hover ink. Occupancy clip is a
/// node-present PartialLoad rule (do not punch a wrapping caption). Applying it
/// here treats a containing card as a foreign sibling, zeros the hole, and the
/// overlap blit keeps `:hover` until scroll settles.
fn node_present_scissor_logical_ex(
    kind: PaintDirtyKind,
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    prev_slots: &[(u64, [f32; 4])],
    node_paints: &[NodePaint],
    scissor_pad: f32,
    occupancy_clip: bool,
) -> [f32; 4] {
    // Restore replays the page and every intersecting portal in order. Sibling
    // exclusion would leave old shadow pixels outside the reconstructed hole.
    if kind == PaintDirtyKind::OverlayRestore {
        return dirty_bounds;
    }
    if kind == PaintDirtyKind::Region {
        // Region already contains prev/current coverage and replays every layer
        // in order. Node occupancy shrinking would cut away the last translated
        // label pixels as Latest exits, leaving faint text below "focus".
        // A keyed layer keeps its ancestor clip after promotion. The largest
        // fallback clip may belong to the outer page and include the header.
        let keyed = node_paints.iter().any(|p| p.key == dirty_key);
        let clip = if keyed {
            node_present_overflow_clip(dirty_key, dirty_bounds, node_paints)
        } else {
            let prev = prev_slots
                .iter()
                .find(|(key, _)| *key == dirty_key)
                .map(|(_, rect)| *rect);
            if scrolled_out_top_into_title(dirty_bounds, prev, node_paints) {
                return [0.0; 4];
            }
            fallback_overflow_clip_for_unmatched(dirty_bounds, node_paints)
        };
        return match clip {
            Some(clip) => intersect_rects(dirty_bounds, clip).unwrap_or([0.0; 4]),
            None => dirty_bounds,
        };
    }
    let prev_slot = prev_slots
        .iter()
        .find(|(k, _)| *k == dirty_key)
        .map(|(_, b)| *b);
    if scrolled_out_top_into_title(dirty_bounds, prev_slot, node_paints) {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let layered = matching_slot_is_layered(dirty_key, dirty_bounds, node_paints);
    // Latest `PaintDirty.bounds` already includes the CSS shadow pad. Clamping
    // GPU scissor to the opaque slot left the halo outside CLEAR — PartialLoad
    // stacked every fade frame. Ping/bounce dirty is the visual itself.
    let clamp_box = if layered {
        match layered_overlay_slot_bounds(dirty_key, dirty_bounds, node_paints) {
            Some(slot) => layered_slot_with_halo(slot, dirty_bounds),
            None => dirty_bounds,
        }
    } else {
        dirty_bounds
    };
    let prev = last_present_prev_for_dirty(kind, dirty_key, prev_slots).and_then(|p| {
        if layered {
            clamp_last_present_prev_tight(p, clamp_box)
        } else {
            clamp_last_present_prev(p, clamp_box)
        }
    });
    let mut scissor = match prev {
        Some(prev) => union_xywh(clamp_box, prev),
        None => clamp_box,
    };
    let overflow = match kind {
        PaintDirtyKind::Node => node_present_overflow_clip(dirty_key, dirty_bounds, node_paints),
        // Culled shimmer escalates to Region; the strip is prev∪now through the
        // title bar. Clip to the live ScrollArea so CLEAR cannot punch 「基础」.
        // Island-swap (`dest_reset_fills` + on-window) has no fallback clip.
        PaintDirtyKind::Region => fallback_overflow_clip_for_unmatched(dirty_bounds, node_paints),
        PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore => None,
    };
    if let Some(c) = overflow {
        scissor = intersect_rects(scissor, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    }
    let pad = if layered { 1.0 } else { scissor_pad };
    scissor = padded_xywh(scissor, pad);
    if let Some(c) = overflow {
        scissor = intersect_rects(scissor, c).unwrap_or([0.0, 0.0, 0.0, 0.0]);
    }
    if occupancy_clip && !dest_reset_fills_overflow_clip(dirty_bounds, overflow) {
        clip_node_present_rect(
            scissor,
            dirty_key,
            dirty_bounds,
            node_paints,
            1.0,
            layered,
            &[],
        )
    } else {
        scissor
    }
}

pub(crate) fn node_present_scissor_pad(kind: PaintDirtyKind) -> f32 {
    match kind {
        PaintDirtyKind::Overlay => 16.0,
        PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region => 0.0,
        PaintDirtyKind::Node => 8.0,
    }
}

/// How far outside a slot a previous underline may sit and still belong to it.
///
/// Leave has no live `underline` flag, so the old quad is the only record.
/// `gap-3` (12px) content starts further down than a normal offset, so a 16px
/// chase erases this slot's line without adopting the next row's.
const TEXT_DECORATION_LEAVE_CHASE_PX: f32 = 16.0;

/// Grow a node-present hole so CSS underline / overline ink is inside it.
///
/// Dest-reset replaces the 8px scissor pad with the border box. Positive
/// `text-underline-offset` paints below that box, so hover clips the line
/// until a scroll FullClear. Union the decoration quads only — not the widget
/// pad — and keep the ancestor overflow clip.
pub(crate) fn expand_scissor_for_text_decoration(
    scissor: [f32; 4],
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    text: &[TextLayoutInfo],
    prev_underlines: &[InstanceInput],
    ui_scale: f32,
) -> [f32; 4] {
    if scissor[2] <= 0.0 || scissor[3] <= 0.0 {
        return scissor;
    }
    let Some(extra) = text_decoration_hole_logical(
        dirty_key,
        dirty_bounds,
        node_paints,
        text,
        prev_underlines,
        ui_scale,
    ) else {
        return scissor;
    };
    let grown = union_xywh(scissor, extra);
    match node_present_overflow_clip(dirty_key, dirty_bounds, node_paints) {
        Some(c) => intersect_rects(grown, c).unwrap_or(scissor),
        None => grown,
    }
}

fn text_decoration_hole_logical(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    text: &[TextLayoutInfo],
    prev_underlines: &[InstanceInput],
    ui_scale: f32,
) -> Option<[f32; 4]> {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let mut acc: Option<[f32; 4]> = None;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        let lo = p.text.0.min(text.len());
        let hi = p.text.1.min(text.len());
        for n in &text[lo..hi] {
            let Some(ink) = crate::renderer::prepare_text::text_decoration_ink_logical(n, ui_scale)
            else {
                continue;
            };
            acc = Some(match acc {
                Some(prev) => union_xywh(prev, ink),
                None => ink,
            });
        }
    }
    let scale = ui_scale.max(1e-6);
    let chase = padded_xywh(dirty_bounds, TEXT_DECORATION_LEAVE_CHASE_PX);
    for ul in prev_underlines {
        if ul.rect_size[0] <= 0.0 || ul.rect_size[1] <= 0.0 {
            continue;
        }
        let logical = [
            ul.rect_pos[0] / scale,
            ul.rect_pos[1] / scale,
            ul.rect_size[0] / scale,
            ul.rect_size[1] / scale,
        ];
        let cx = logical[0] + logical[2] * 0.5;
        let x_owned = cx >= dirty_bounds[0] && cx < dirty_bounds[0] + dirty_bounds[2];
        if !x_owned || intersect_rects(logical, chase).is_none() {
            continue;
        }
        // Erase inflates the quad by 1px physical; keep that fringe in the hole.
        let erase = padded_xywh(logical, 1.0 / scale + 1.0);
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, erase),
            None => erase,
        });
    }
    acc
}

fn matching_slot_is_layered(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    layered_overlay_slot_bounds(dirty_key, dirty_bounds, node_paints).is_some()
}

/// Layered jump dirty may be the full-width dock. Keep the halo pad around the
/// opaque slot so dest-reset / blit pins cannot swallow the overlay rail.
/// `LAST_PRESENT_PREV_MAX_OUTSET` (48px) punched wrapping past Latest.
const LAYERED_SLOT_HALO_MAX: f32 = 16.0;

fn layered_slot_with_halo(slot: [f32; 4], dirty_bounds: [f32; 4]) -> [f32; 4] {
    layered_slot_with_halo_pad(slot, dirty_bounds, LAYERED_SLOT_HALO_MAX)
}

fn layered_slot_with_halo_pad(slot: [f32; 4], dirty_bounds: [f32; 4], pad: f32) -> [f32; 4] {
    let neighborhood = padded_xywh(slot, pad.max(0.0));
    let halo = intersect_rects(dirty_bounds, neighborhood).unwrap_or(slot);
    union_xywh(slot, halo)
}

/// Collect-box of the layered overlay slot (opaque pill, not shadow-padded dirty).
pub(crate) fn layered_overlay_slot_bounds(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let mut acc: Option<[f32; 4]> = None;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) || !p.layered {
            continue;
        }
        if p.bounds[2] <= 0.0 || p.bounds[3] <= 0.0 {
            continue;
        }
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, p.bounds),
            None => p.bounds,
        });
    }
    acc
}

/// Dest-reset of a later page-stack slot must rebuild in-flow fills that sit
/// in the hole. In-flow Node dest-reset stays membership-only.
pub(crate) fn node_present_restore_underlay(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    if matching_slot_is_layered(dirty_key, dirty_bounds, node_paints) {
        return true;
    }
    // Hide-settle Latest keys the pill; layered often lives on the short
    // absolute dock. Dest-reset still CLEARs wrapping under that bar.
    if overlaps_foreign_layered_bar(dirty_key, dirty_bounds, node_paints) {
        return true;
    }
    // Unkeyed chrome: recapture wrapping whenever this Node dirty overlaps
    // a layered underlay (not a foreign page slot).
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    if has_keyed {
        // Keyed in-flow slots stay membership-only, except a focus-ring fringe
        // that crosses into the next peer. Dest-reset keeps that strip so the
        // ring can leave and arrive; the neighbor border under it has to be
        // redrawn or the clear punches the shared edge.
        return peer_halo_fringe_needs_restore(dirty_key, dirty_bounds, node_paints);
    }
    dirty_bounds[2] > 0.5
        && dirty_bounds[3] > 0.5
        && node_paints.iter().any(|p| {
            p.layered
                && !p.overlay
                && p.bounds[2] > 0.5
                && p.bounds[3] > 0.5
                && intersect_rects(p.bounds, dirty_bounds)
                    .and_then(|hit| match p.overflow_clip {
                        Some(clip) => intersect_rects(hit, clip),
                        None => Some(hit),
                    })
                    .is_some()
        })
}

fn aabb_contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] + 0.5
        && outer[1] <= inner[1] + 0.5
        && outer[0] + outer[2] + 0.5 >= inner[0] + inner[2]
        && outer[1] + outer[3] + 0.5 >= inner[1] + inner[3]
}

fn rect_center(r: [f32; 4]) -> [f32; 2] {
    [r[0] + r[2] * 0.5, r[1] + r[3] * 0.5]
}

fn point_in_rect(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[1] >= r[1] && p[0] < r[0] + r[2] && p[1] < r[1] + r[3]
}

/// Taffy rest box when recorded; otherwise the visual collect slot.
/// Dest-reset occupancy must not use ping/bounce growth — a caption above is
/// not an already-overlapping earlier sibling just because scale/translate
/// swallowed it this frame.
fn node_paint_occupancy(p: &NodePaint) -> [f32; 4] {
    match p.rest {
        Some(r) if r[2] > 0.5 && r[3] > 0.5 => r,
        _ => p.bounds,
    }
}

fn node_paint_peer_occupancy(p: &NodePaint) -> [f32; 4] {
    let rest = node_paint_occupancy(p);
    // A translated plate blocks its live pose, not the layout slot it left.
    // Keep rest occupancy for scale/bounce growth and caption halo guards.
    if (p.bounds[2] - rest[2]).abs() <= 0.5 && (p.bounds[3] - rest[3]).abs() <= 0.5 {
        p.bounds
    } else {
        rest
    }
}

/// Occupancy clipping may remove scissor padding, but actual visual overflow
/// needs an ordered Region repair so both old ink and intersecting peers redraw.
fn node_present_clips_visual_overflow(key: u64, coverage: [f32; 4], paints: &[NodePaint]) -> bool {
    let overflows = paints.iter().any(|p| {
        p.key == key
            && !p.overlay
            && p.rest.is_some_and(|rest| {
                !crate::app::damage::rect_contains_rect(padded_xywh(rest, 2.5), coverage)
            })
    });
    overflows
        && !crate::app::damage::rect_contains_rect(
            clip_node_present_rect(
                coverage,
                key,
                coverage,
                paints,
                1.0,
                matching_slot_is_layered(key, coverage, paints),
                &[],
            ),
            coverage,
        )
}

/// Dest-reset / node-present scissor occupancy clip for independent Node repairs.
/// Repairs whose actual ink crosses a peer are promoted to Region before encode.
fn clip_node_present_rect(
    rect: [f32; 4],
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    slot_scale: f32,
    layered: bool,
    instances: &[InstanceInput],
) -> [f32; 4] {
    if layered {
        let r = clip_rect_against_layered_siblings(
            rect,
            dirty_key,
            dirty_bounds,
            node_paints,
            slot_scale,
        );
        clip_rect_against_foreign_siblings(
            r,
            dirty_key,
            dirty_bounds,
            node_paints,
            slot_scale,
            instances,
        )
    } else {
        clip_rect_against_foreign_siblings(
            rect,
            dirty_key,
            dirty_bounds,
            node_paints,
            slot_scale,
            instances,
        )
    }
}

/// Shrink a layered dest-reset so it does not cover **other** page-stack slots
/// (overlay auto scrollbar vs Latest). In-flow underlay stays in the wipe.
///
/// The jump dock is a full-width empty bar that shares Y with the overlay
/// rail. Occupancy clip against that bar used to cut the fading track's
/// bottom, leaving a groove+thumb remnant in retain (Latest's dirty band).
fn clip_rect_against_layered_siblings(
    mut rect: [f32; 4],
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    slot_scale: f32,
) -> [f32; 4] {
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return rect;
    }
    let s = slot_scale.max(0.0);
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let host_idx = node_paints
        .iter()
        .position(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed))
        .unwrap_or(usize::MAX);
    let rest_host = matching_rest_host(dirty_key, dirty_bounds, node_paints)
        .map(|bounds| scale_xywh(bounds, s));
    let matching: Vec<_> = node_paints
        .iter()
        .filter(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed))
        .collect();
    for (i, p) in node_paints.iter().enumerate() {
        if !p.layered || p.overlay || p.bounds[2] <= 0.0 || p.bounds[3] <= 0.0 {
            continue;
        }
        if slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        let related = matching.iter().any(|m| {
            slot_ranges_covered_by(m, p)
                    || slot_ranges_covered_by(p, m)
                    // Fade-to-zero emits no quads, so collect ranges are empty and
                    // `slot_ranges_covered_by` is false. The thumb still sits inside
                    // the track. Carving that hole keeps only the sliver beside the
                    // thumb; the track ink under the thumb stays in retain.
                    || occupancy_descendant_slot(m, p)
        });
        if related {
            continue;
        }
        let occ = scale_xywh(node_paint_peer_occupancy(p), s);
        if occ[2] <= 0.5 || occ[3] <= 0.5 {
            continue;
        }
        if aabb_contains(occ, rect) {
            continue;
        }
        if intersect_rects(rect, occ).is_none() {
            continue;
        }
        // Overlay chrome (Latest pill / overlay rail) sits on in-flow wrapping
        // that inherits `layered` from MessageScroller's stacking-context.
        // Shrinking the wipe against those wide siblings left the SDF halo in
        // PartialLoad (stacked enter ring / hide ghost) or punched a pill hole
        // with no recapture. Keep clipping a *peer* overlay (rail vs pill).
        if occ[2] > rect[2] * 1.5 + 8.0 {
            continue;
        }
        // Earlier rest-overlapping layers are underlay. In particular, a
        // textarea's absolute text viewport must not carve its later overlay
        // rail down to the sliver outside the viewport's horizontal padding.
        // Rest-disjoint captions and later foreground peers still clip. Keep
        // the same overlapping-circle exception as the foreign-sibling path.
        if i < host_idx
            && rest_host.is_some_and(|host| {
                intersect_rects(host, occ).is_some()
                    && (!crate::app::damage::is_rest_stack_chip(host)
                        || !crate::app::damage::is_rest_stack_chip(occ))
            })
        {
            continue;
        }
        rect = shrink_aabb_to_exclude(rect, occ);
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    rect
}

/// 1px join seam sitting on the shared edge.
///
/// The logical hole stops on that edge. `damage_to_scissor_physical` then ceils,
/// so at 125% DPI the device scissor includes the strip and the item fill is
/// submitted over it. The strip has to be drawn again after the fill.
pub(crate) fn node_present_replays_shared_seam(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    paint: &NodePaint,
    hole: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    if paint.overlay || !slot_emits_paint(paint) {
        return false;
    }
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    if slot_matches_dirty(dirty_key, dirty_bounds, paint, has_keyed) {
        return false;
    }
    let Some(host) = matching_rest_host(dirty_key, dirty_bounds, node_paints) else {
        return false;
    };
    let occ = node_paint_occupancy(paint);
    edge_hairline_beside(host, occ) && intersect_rects(hole, occ).is_some()
}

/// Horizontal or vertical strip of about 1px lying on `host`'s edge, spanning
/// that edge. Interior carets and full-size siblings are not strips.
fn edge_hairline_beside(host: [f32; 4], occ: [f32; 4]) -> bool {
    if occ[2] <= 0.5 || occ[3] <= 0.5 || host[2] <= 0.5 || host[3] <= 0.5 {
        return false;
    }
    if aabb_contains(occ, host) {
        return false;
    }
    let host_r = host[0] + host[2];
    let host_b = host[1] + host[3];
    let occ_r = occ[0] + occ[2];
    let occ_b = occ[1] + occ[3];
    let x_overlap = occ_r.min(host_r) - occ[0].max(host[0]);
    let y_overlap = occ_b.min(host_b) - occ[1].max(host[1]);
    let thin_h = occ[3] <= 1.5 && occ[2] + 0.5 >= host[2] * 0.5;
    let thin_w = occ[2] <= 1.5 && occ[3] + 0.5 >= host[3] * 0.5;
    if thin_h && !thin_w {
        let y_touch = (occ_b - host[1]).abs() <= 1.5 || (occ[1] - host_b).abs() <= 1.5;
        return y_touch && x_overlap + 0.5 >= host[2] * 0.5;
    }
    if thin_w && !thin_h {
        let x_touch = (occ_r - host[0]).abs() <= 1.5 || (occ[0] - host_r).abs() <= 1.5;
        return x_touch && y_overlap + 0.5 >= host[3] * 0.5;
    }
    false
}

/// True when `hit` (the wipe ∩ sibling occupancy) is only the outset past a
/// shared edge of two similar slots.
///
/// Rest boxes of a button group touch; they do not overlap (`intersect_rects`
/// is empty when they meet). The ring pad is a few pixels on that edge. A
/// label above an input fails the size check, so that gap still shrinks.
fn peer_edge_halo_fringe(rest_host: [f32; 4], occ: [f32; 4], hit: [f32; 4], scale: f32) -> bool {
    let limit = 16.0 * scale.max(1.0);
    let host_l = rest_host[0];
    let host_t = rest_host[1];
    let host_r = rest_host[0] + rest_host[2];
    let host_b = rest_host[1] + rest_host[3];
    let hit_l = hit[0];
    let hit_t = hit[1];
    let hit_r = hit[0] + hit[2];
    let hit_b = hit[1] + hit[3];
    let right = hit_l >= host_r - 1.0 && hit_r > host_r && (hit_r - host_r) <= limit;
    let left = hit_r <= host_l + 1.0 && hit_l < host_l && (host_l - hit_l) <= limit;
    let below = hit_t >= host_b - 1.0 && hit_b > host_b && (hit_b - host_b) <= limit;
    let above = hit_b <= host_t + 1.0 && hit_t < host_t && (host_t - hit_t) <= limit;
    let y_overlap = hit_b.min(host_b) - hit_t.max(host_t);
    let x_overlap = hit_r.min(host_r) - hit_l.max(host_l);
    if (left || right) && !(above || below) {
        // Horizontal shared edge, including the minus between OTP groups.
        // The strip is a few pixels wide and runs along the slot, so it is
        // not a label sitting in the gap above an input.
        let span = rest_host[3].min(occ[3]).max(1.0) * 0.5;
        y_overlap + 0.5 >= span
    } else if (above || below) && !(left || right) {
        // Same rule on a vertical group. A form_field label is shorter than
        // the input, so it still shrinks and the gap(4) glyphs stay.
        let wr = rest_host[2] / occ[2].max(1.0);
        if !(0.6..=1.6).contains(&wr) || (rest_host[3] - occ[3]).abs() > 2.5 * scale.max(1.0) {
            return false;
        }
        let span = rest_host[2].min(occ[2]).max(1.0) * 0.5;
        x_overlap + 0.5 >= span
    } else {
        false
    }
}

fn matching_rest_host(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> Option<[f32; 4]> {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let mut rest_host: Option<[f32; 4]> = None;
    for p in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        let rb = node_paint_occupancy(p);
        if rb[2] <= 0.5 || rb[3] <= 0.5 {
            continue;
        }
        rest_host = Some(match rest_host {
            Some(prev) => union_xywh(prev, rb),
            None => rb,
        });
    }
    rest_host
}

/// Layered paint that is the dirty slot itself, a descendant (OTP caret wrap),
/// or an ancestor of that slot. Not Latest's absolute dock sitting on wrapping.
fn layered_bar_belongs_to_dirty_slot(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    bar: &NodePaint,
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|host| {
        slot_matches_dirty(dirty_key, dirty_bounds, host, has_keyed)
            && (is_nested_foreign_slot(host, bar) || is_nested_foreign_slot(bar, host))
    })
}

fn overlaps_foreign_layered_bar(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    dirty_bounds[2] > 0.5
        && dirty_bounds[3] > 0.5
        && node_paints.iter().any(|p| {
            p.layered
                && !p.overlay
                && p.key != dirty_key
                && p.bounds[2] > 0.5
                && p.bounds[3] > 0.5
                && p.bounds[3] <= dirty_bounds[3] * 3.0 + 24.0
                && intersect_rects(p.bounds, dirty_bounds)
                    .and_then(|hit| match p.overflow_clip {
                        Some(clip) => intersect_rects(hit, clip),
                        None => Some(hit),
                    })
                    .is_some()
                && !layered_bar_belongs_to_dirty_slot(dirty_key, dirty_bounds, p, node_paints)
        })
}

/// In-flow peer fringe is the only reason to restore. Layered pills still
/// rebuild every underlay in the hole.
fn peer_fringe_restore_only(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    hole: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    if matching_slot_is_layered(dirty_key, dirty_bounds, node_paints) {
        return false;
    }
    if overlaps_foreign_layered_bar(dirty_key, dirty_bounds, node_paints) {
        return false;
    }
    paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints)
        && peer_halo_fringe_needs_restore_in_hole(dirty_key, dirty_bounds, hole, node_paints)
}

/// Adjacent in-flow chrome must be reconstructed in scene order, together with
/// the dirty slot. Testing the damage box misses the AA rows that dest-reset
/// clears outside it. A focus ring remains a separate overlay over its peers.
pub(crate) fn node_present_replays_peer_chrome(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    hole: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    peer_fringe_restore_only(dirty_key, dirty_bounds, hole, node_paints)
        && !node_paints
            .iter()
            .any(|p| p.key == dirty_key && paint_instances_carry_focus_ring(p, instances))
}

/// Which foreign slot to redraw into a dest-reset hole.
///
/// A layered overlay rebuilds every underlay the hole touches. An in-flow
/// focus ring only needs the adjacent peer: the group that owns the slot
/// also owns the slot's quad, and src-over of that translucent fill
/// (`bg-input/30`) darkens the cell and the ring. Blur then leaves the
/// darkened fill behind.
/// Ancestor collect ranges include the dirty slot's quad. Restoring that range
/// src-overs the translucent fill and the focus ring the slot paints next.
fn underlay_range_covers_dirty_slot(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    paint: &NodePaint,
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|host| {
        host.key != paint.key
            && slot_matches_dirty(dirty_key, dirty_bounds, host, has_keyed)
            // Canvas-only slots have no rect instances. Both an ancestor's
            // collect range and a descendant's range are already submitted by
            // the dirty slot; neither is a separate underlay.
            && (is_nested_foreign_slot(paint, host) || is_nested_foreign_slot(host, paint))
    })
}

pub(crate) fn node_present_underlay_paint(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    paint: &NodePaint,
    hole: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    if paint.overlay || intersect_rects(paint.bounds, hole).is_none() {
        return false;
    }
    if underlay_range_covers_dirty_slot(dirty_key, dirty_bounds, paint, node_paints) {
        return false;
    }
    if !peer_fringe_restore_only(dirty_key, dirty_bounds, hole, node_paints) {
        return true;
    }
    let Some(rest_host) = matching_rest_host(dirty_key, dirty_bounds, node_paints) else {
        return false;
    };
    let occ = node_paint_occupancy(paint);
    let Some(hit) = intersect_rects(hole, occ) else {
        return false;
    };
    peer_edge_halo_fringe(rest_host, occ, hit, 1.0)
        && !paint_instances_carry_focus_ring(paint, instances)
}

/// Outset focus ring (hard spread, or soft blur with no offset). Restoring that
/// quad into a neighbor's hole src-overs the ring the focused slot also draws.
/// `shadow-xs` is blur 2 with a 1px y offset and ~0.05 alpha, so it stays.
fn instance_is_focus_ring(i: &InstanceInput) -> bool {
    if i.shadow_inset != 0.0 || i.shadow_color[3] <= 0.2 {
        return false;
    }
    if i.shadow_spread >= 1.0 {
        return true;
    }
    i.shadow_blur >= 2.0 && i.shadow_offset[0].abs() < 0.01 && i.shadow_offset[1].abs() < 0.01
}

fn paint_instances_carry_focus_ring(paint: &NodePaint, instances: &[InstanceInput]) -> bool {
    let lo = paint.instance.0.min(instances.len());
    let hi = paint.instance.1.min(instances.len());
    instances[lo..hi].iter().any(instance_is_focus_ring)
}

/// Outset of the focus-ring quad. Layout occupancy stops at the cell; the
/// spread continues into the next cell, and a neighbor wipe that only
/// shrinks to the cell clears that strip.
fn focus_ring_paint_bounds(paint: &NodePaint, instances: &[InstanceInput]) -> Option<[f32; 4]> {
    let lo = paint.instance.0.min(instances.len());
    let hi = paint.instance.1.min(instances.len());
    let mut acc: Option<[f32; 4]> = None;
    for inst in &instances[lo..hi] {
        if !instance_is_focus_ring(inst) {
            continue;
        }
        let b = instance_paint_bounds(inst);
        if b[2] <= 0.5 || b[3] <= 0.5 {
            continue;
        }
        acc = Some(match acc {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
    }
    acc
}

/// The keyed slot's dirty box reaches a neighbor peer only along the shared
/// edge. Clearing that fringe deletes the neighbor border; present must
/// redraw it under the ring.
fn peer_halo_fringe_needs_restore(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    peer_halo_fringe_needs_restore_in_hole(dirty_key, dirty_bounds, dirty_bounds, node_paints)
}

fn peer_halo_fringe_needs_restore_in_hole(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    hole: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    let Some(rest_host) = matching_rest_host(dirty_key, dirty_bounds, node_paints) else {
        return false;
    };
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|p| {
        if p.overlay || !slot_emits_paint(p) {
            return false;
        }
        if slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            return false;
        }
        let occ = node_paint_occupancy(p);
        let Some(hit) = intersect_rects(hole, occ) else {
            return false;
        };
        peer_edge_halo_fringe(rest_host, occ, hit, 1.0)
    })
}

/// Shrink `rect` so it does not cover sibling collect slots (form_field label
/// above Input). Skip ancestors and interiors — those must stay in the wipe.
///
/// Ancestry is collect-range nesting, not layout-box containment.
/// Overflow:visible + `animate-spin` grows the dirty slot to a circumcircle that
/// sticks out of a Marker row; `aabb_contains(row, icon)` is false, and shrinking
/// against the row left only leftover strips outside the 16px band — idle
/// PartialLoad src-overs the loader-circle body until the gap seals.
///
/// Occupancy (center-in-host / earlier-sibling intersect / shrink) uses the
/// rest Taffy box. Visual host still detects a parent that contains the grown
/// slot. Clipping against a neighbor's grown visual was cropping pulse present
/// into nested dest-reset frames, and skipping the caption because ping/bounce
/// growth swallowed its center.
fn clip_rect_against_foreign_siblings(
    mut rect: [f32; 4],
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    slot_scale: f32,
    instances: &[InstanceInput],
) -> [f32; 4] {
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return rect;
    }
    let s = slot_scale.max(0.0);
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let mut host: Option<[f32; 4]> = None;
    let mut rest_host: Option<[f32; 4]> = None;
    let mut host_idx = usize::MAX;
    for (i, p) in node_paints.iter().enumerate() {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        if p.bounds[2] <= 0.0 || p.bounds[3] <= 0.0 {
            continue;
        }
        host_idx = host_idx.min(i);
        let b = scale_xywh(p.bounds, s);
        host = Some(match host {
            Some(prev) => union_xywh(prev, b),
            None => b,
        });
        let rb = scale_xywh(node_paint_occupancy(p), s);
        rest_host = Some(match rest_host {
            Some(prev) => union_xywh(prev, rb),
            None => rb,
        });
    }
    let Some(host) = host else {
        return rect;
    };
    let mut rest_host = rest_host.unwrap_or(host);
    let matching: Vec<_> = node_paints
        .iter()
        .filter(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed))
        .collect();
    // A leaf and its paint-less wrapper share one draw span. Its layout
    // occupancy is the wrapper's box, even when the leaf's glyph line starts
    // lower. Use that box to distinguish an overlapping underlay from a peer.
    for p in node_paints {
        if matching
            .iter()
            .any(|m| slot_ranges_covered_by(m, p) && slot_ranges_covered_by(p, m))
        {
            rest_host = union_xywh(rest_host, scale_xywh(node_paint_occupancy(p), s));
        }
    }
    let host_layered = node_paints
        .iter()
        .any(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) && p.layered);
    for (i, p) in node_paints.iter().enumerate() {
        if p.overlay || p.bounds[2] <= 0.0 || p.bounds[3] <= 0.0 {
            continue;
        }
        if slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        let related = matching.iter().any(|m| {
            // A capture leaf can share a texture with an ancestor whose
            // other channels include unrelated chrome. It is not a peer
            // to carve out of that texture's repair scissor.
            slot_paint_ranges_overlap(m, p)
        });
        if related {
            continue;
        }
        // Empty flex spacers (slider before/after) occupy the halo but paint
        // nothing. Shrinking the wipe against them clips the box-shadow to the
        // thumb's border box on that axis — the track then keeps the equatorial
        // lobes and the halo only survives on the free axis.
        if !slot_emits_paint(p) {
            continue;
        }
        let fb = scale_xywh(p.bounds, s);
        let mut occ = node_paint_peer_occupancy(p);
        // Scrolled text can extend across peers while only painting inside its viewport.
        if let Some(clip) = p.overflow_clip {
            let Some(visible) = intersect_rects(occ, clip) else {
                continue;
            };
            occ = visible;
        }
        let occ = scale_xywh(occ, s);
        if aabb_contains(fb, host) {
            continue;
        }
        if point_in_rect(rest_host, rect_center(occ)) {
            continue;
        }
        // MessageScroller `data-stacking-context` marks wrapping bubbles
        // layered. Occupancy shrink against those siblings left the SDF halo
        // in PartialLoad (stacked enter ring) or punched a pill hole with no
        // recapture. Same width skip as `clip_rect_against_layered_siblings`
        // — keep clipping a *peer* overlay (rail vs pill).
        if host_layered && p.layered && occ[2] > rect[2] * 1.5 + 8.0 {
            continue;
        }
        if host_layered && !p.layered {
            if !slot_visual_exceeds_rest(dirty_key, node_paints) {
                // Latest-style overlay: wrapping thread is underlay. Occupancy
                // shrink of those siblings left the SDF halo in PartialLoad
                // (stacked enter ring / hide ghost). Ping scale-2 still clips
                // rest-disjoint captions below.
                continue;
            }
            // Halo below the rest box sits on wrapping thread. Captions *above*
            // a layered ping still clip.
            let host_bottom = rest_host[1] + rest_host[3];
            if occ[1] + 0.5 >= host_bottom {
                continue;
            }
        }
        if i < host_idx && intersect_rects(rest_host, occ).is_some() {
            // hover:scale leave must wipe the enlarged overlap with an earlier
            // sibling (reaction chip over content). Rest-overlapping rounded
            // chips (AvatarGroup A/B/C) must not: AABB dest-reset of the later
            // circle punches the earlier disk's crescent.
            if !crate::app::damage::is_rest_stack_chip(rest_host)
                || !crate::app::damage::is_rest_stack_chip(occ)
            {
                continue;
            }
        }
        let Some(hit) = intersect_rects(rect, occ) else {
            continue;
        };
        // Side-by-side peers (OTP / button group) touch on one edge. The focus
        // ring is a thin strip past that edge. Shrinking the wipe to the rest
        // box drops the strip: the slot that blurred keeps the old right edge,
        // and the slot that focused never redraws it. A fat overlap (the whole
        // neighbor) still shrinks. A label above an input is not a size peer,
        // so the form_field gap stays clipped.
        // A neighbor that owns a focus ring redraws that ring itself. Keeping
        // this fringe CLEARs the ring and, if we then restore the peer, src-overs
        // it a second time (the halo goes from ~215 to ~190, and blur leaves it).
        // Shrinking only to the layout box still clears the outset: the ring
        // is painted past that edge, into the neighbor that just blurred.
        let carries_ring = paint_instances_carry_focus_ring(p, instances);
        if peer_edge_halo_fringe(rest_host, occ, hit, s) && !carries_ring {
            continue;
        }
        let cut = if carries_ring {
            focus_ring_paint_bounds(p, instances)
                .map(|halo| union_xywh(occ, halo))
                .unwrap_or(occ)
        } else {
            occ
        };
        rect = shrink_aabb_to_exclude(rect, cut);
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    rect
}

/// Collect recorded a draw for this slot (fill, ramp, glyphs, or caret chrome).
fn slot_emits_paint(p: &NodePaint) -> bool {
    p.instance.0 < p.instance.1
        || p.textured.0 < p.textured.1
        || p.text.0 < p.text.1
        || p.chrome.0 < p.chrome.1
}

fn shrink_aabb_to_exclude(wipe: [f32; 4], cut: [f32; 4]) -> [f32; 4] {
    if aabb_contains(cut, wipe) {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let w_x0 = wipe[0];
    let w_y0 = wipe[1];
    let w_x1 = wipe[0] + wipe[2];
    let w_y1 = wipe[1] + wipe[3];
    let c_x0 = cut[0];
    let c_y0 = cut[1];
    let c_x1 = cut[0] + cut[2];
    let c_y1 = cut[1] + cut[3];
    let mut best = wipe;
    let mut best_area = -1.0_f32;
    let consider = |r: [f32; 4], best: &mut [f32; 4], best_area: &mut f32| {
        if r[2] > 0.5 && r[3] > 0.5 {
            let a = r[2] * r[3];
            if a > *best_area {
                *best_area = a;
                *best = r;
            }
        }
    };
    if c_y1 < w_y1 {
        consider(
            [w_x0, c_y1, wipe[2], w_y1 - c_y1],
            &mut best,
            &mut best_area,
        );
    }
    if c_y0 > w_y0 {
        consider(
            [w_x0, w_y0, wipe[2], c_y0 - w_y0],
            &mut best,
            &mut best_area,
        );
    }
    if c_x1 < w_x1 {
        consider(
            [c_x1, w_y0, w_x1 - c_x1, wipe[3]],
            &mut best,
            &mut best_area,
        );
    }
    if c_x0 > w_x0 {
        consider(
            [w_x0, w_y0, c_x0 - w_x0, wipe[3]],
            &mut best,
            &mut best_area,
        );
    }
    if best_area < 0.0 {
        wipe
    } else {
        best
    }
}

/// Scroll blit cannot run node-present encode (`scroll_layer_clear`). Pin the same
/// dest-reset hole: last-present prev ∪ now, clamped, overflow-clipped, padded.
/// Using only `dirty.bounds` leaves the previous hover box in the copied overlap.
/// Do **not** occupancy-clip: a containing in-flow card zeros the hole and the
/// copied overlap keeps `:hover` until blit stops.
pub(crate) fn named_scroll_blit_pins(
    dirties: &[PaintDirty],
    prev_slots: &[(u64, [f32; 4])],
    node_paints: &[NodePaint],
) -> Vec<[f32; 4]> {
    dirties
        .iter()
        .map(|d| {
            node_present_scissor_logical_ex(
                d.kind,
                d.key,
                d.bounds,
                prev_slots,
                node_paints,
                node_present_scissor_pad(d.kind),
                false,
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
#[cfg(test)]
pub(crate) fn node_present_dest_reset_scaled(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> Option<([f32; 4], [f32; 4])> {
    node_present_dest_reset_full(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        textured,
        &[],
        ui_scale,
        prev_bounds,
    )
    .map(|d| (d.rect, d.clear))
}

/// Dest-reset hole: AABB `clear` (never the rounded fill itself) then optional
/// replay of that fill's own SDF chrome so a hover:scale Bake wipe cannot stamp
/// a sharp rect onto a capsule / outline.
#[derive(Debug)]
pub(crate) struct NodePresentDestReset {
    pub rect: [f32; 4],
    pub clear: [f32; 4],
    pub replay: (usize, usize),
}

/// Compact in-page stacking chrome (Latest) is its own page-stack band.
///
/// Skip dest-reset only on the **first visible paint** (overlay in the draw
/// list, no last-present leftover): wrapping is still in retain, src-over the
/// pill. Any leftover (fade / slide / hide settle) dest-resets that ink,
/// restores wrapping in the hole, then paints the overlay. AABB CLEAR without
/// wrapping restore is the enter white ring / hide hole; skip dest-reset during
/// fade is the stacked ghost.
pub(crate) fn layered_src_over_skips_dest_reset(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    prev_bounds: Option<[f32; 4]>,
) -> bool {
    let Some(slot) = layered_overlay_slot_bounds(dirty_key, dirty_bounds, node_paints) else {
        return false;
    };
    // Overlay rails / viewport-sized stacking bands dest-reset occupancy.
    if slot[3] > 56.0 {
        return false;
    }
    let has_prev = prev_bounds.is_some_and(|b| b[2] > 0.5 && b[3] > 0.5);
    if has_prev {
        return false;
    }
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    if node_paints.iter().any(|paint| {
        slot_matches_dirty(dirty_key, dirty_bounds, paint, has_keyed)
            && instances
                [paint.instance.0.min(instances.len())..paint.instance.1.min(instances.len())]
                .iter()
                .any(|instance| instance.backdrop_blur > 0.0)
    }) {
        return false;
    }
    if !node_paints.iter().any(|paint| {
        slot_matches_dirty(dirty_key, dirty_bounds, paint, has_keyed)
            && (paint.instance.0 < paint.instance.1 || paint.textured.0 < paint.textured.1)
    }) {
        return false;
    }
    // Hide settle has no dest-ink and often no last-present on the hidden
    // collect. Still dest-reset the leftover AABB. First visible paint (pill
    // in the draw list) src-overs wrapping that is still in retain.
    layered_overlay_slot_in_draw_list(dirty_key, dirty_bounds, node_paints)
}

/// Dest-reset of a layered overlay punched wrapping. Recapture those glyphs
/// in the wipe **before** the overlay slot. TextBounds is the hole (not the
/// wrap box) so glyphon cannot stamp the thread onto Latest's label.
pub(crate) fn node_present_flush_layered_underlay_glyphs(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    matching_slot_is_layered(dirty_key, dirty_bounds, node_paints)
}

/// In-flow dest-reset of wrapping that sits under a dirty Latest would CLEAR
/// the pill and flush wrap-box glyphon after the overlay slot ("gh" / "ps."
/// on the button). The overlay slot owns that overlap (src-over or dest-reset).
pub(crate) fn skip_in_flow_under_layered_overlay(
    dirty: &PaintDirty,
    nodes: &[PaintDirty],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    if matching_slot_is_layered(dirty.key, dirty.bounds, node_paints) {
        return false;
    }
    // A rail only reconstructs its small overlap. It cannot replace a dirty
    // field's own fill, border or focus ring outside that hole. Only glyph-only
    // wrapping can yield its repaint to the layered underlay capture.
    let has_keyed = paints_has_keyed_slot(dirty.key, dirty.bounds, node_paints);
    if node_paints.iter().any(|p| {
        if !slot_matches_dirty(dirty.key, dirty.bounds, p, has_keyed) {
            return false;
        }
        let lo = p.instance.0.min(instances.len());
        let hi = p.instance.1.min(instances.len());
        instances[lo..hi].iter().enumerate().any(|(offset, inst)| {
            !nested_descendant_owns(p, node_paints, lo + offset, |q| q.instance)
                && instance_has_dest_ink(inst)
        })
    }) {
        return false;
    }
    nodes.iter().any(|o| {
        o.kind == PaintDirtyKind::Node
            && o.key != dirty.key
            // Carets are drawn after the retained page blit. They cannot own
            // the page repaint that replaces the field's previous text.
            && !paint_dirty_is_caret_overlay(o, node_paints, instances)
            && layered_overlay_slot_bounds(o.key, o.bounds, node_paints).is_some()
            && intersect_rects(dirty.bounds, o.bounds).is_some()
    })
}

fn layered_overlay_slot_in_draw_list(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|p| {
        slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed)
            && p.layered
            && (p.instance.1 > p.instance.0 || p.text.1 > p.text.0 || p.textured.1 > p.textured.0)
    })
}

pub(crate) fn node_present_dest_reset_full(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    chrome: &[InstanceInput],
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> Option<NodePresentDestReset> {
    if layered_src_over_skips_dest_reset(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        prev_bounds,
    ) {
        return None;
    }
    let rect = node_present_reset_rect(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        textured,
        chrome,
        NODE_PRESENT_RESET_PAD_PHYSICAL,
        ui_scale,
        prev_bounds,
    )?;
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let dirty_start = node_paints
        .iter()
        .filter(|paint| slot_matches_dirty(dirty_key, dirty_bounds, paint, has_keyed))
        .map(|paint| paint.instance.0)
        .min()
        .unwrap_or(instances.len());
    let logical_rect = rect.map(|value| value / ui_scale.max(1e-6));
    let peer_ranges: Vec<_> = matching_rest_host(dirty_key, dirty_bounds, node_paints)
        .into_iter()
        .flat_map(|host| {
            node_paints.iter().filter(move |paint| {
                intersect_rects(logical_rect, node_paint_occupancy(paint)).is_some_and(|hit| {
                    peer_edge_halo_fringe(host, node_paint_occupancy(paint), hit, 1.0)
                }) && node_present_underlay_paint(
                    dirty_key,
                    dirty_bounds,
                    paint,
                    logical_rect,
                    node_paints,
                    instances,
                )
            })
        })
        .map(|paint| paint.instance)
        .collect();
    let skip_dirty = |index: usize| {
        index >= dirty_start
            || peer_ranges
                .iter()
                .any(|range| range_contains_index(*range, index))
    };
    let cx = rect[0] + rect[2] * 0.5;
    let cy = rect[1] + rect[3] * 0.5;
    let behind_idx = instances.iter().enumerate().rev().find_map(|(i, inst)| {
        if skip_dirty(i) {
            return None;
        }
        // Transparent Input fill still has a 1px rounded stroke. Skipping it
        // CLEARed the I-beam hole with page color and never replayed the SDF —
        // a sharp remnant on the inner-right radius.
        if !instance_fill_opaque(inst) && !instance_has_dest_ink(inst) {
            return None;
        }
        let [px, py] = inst.rect_pos;
        let [pw, ph] = inst.rect_size;
        if cx >= px && cy >= py && cx < px + pw && cy < py + ph {
            Some(i)
        } else {
            None
        }
    });
    let Some(idx) = behind_idx else {
        if let Some(clear) = node_present_behind_color_for_rect(
            dirty_key,
            dirty_bounds,
            node_paints,
            instances,
            rect,
        ) {
            return Some(NodePresentDestReset {
                rect,
                clear,
                replay: (0, 0),
            });
        }
        // Overlay auto-bar hangs in the viewport padding. No card fill contains the
        // thumb center, dest-reset used to return None, and PartialLoad kept the
        // last gray pixels (recovered only after a foreign FullClear).
        if matching_slot_is_layered(dirty_key, dirty_bounds, node_paints) {
            let clear = node_present_behind_color_in_overflow(
                dirty_key,
                dirty_bounds,
                node_paints,
                instances,
            )?;
            return Some(NodePresentDestReset {
                rect,
                clear,
                replay: (0, 0),
            });
        }
        return Some(NodePresentDestReset {
            rect,
            clear: [0.0; 4],
            replay: (0, 0),
        });
    };
    let behind = &instances[idx];
    if !instance_aabb_clear_unsafe(behind) {
        // Reset restores only the underlay. draw_matching_node_paints then
        // paints the dirty slot once, including its rounded fill and focus ring.
        // Replaying own chrome here composites translucent edges twice.
        return Some(NodePresentDestReset {
            rect,
            clear: uniform_opaque_fill_color_for_rect(instances, rect, skip_dirty)
                .unwrap_or([0.0; 4]),
            replay: (0, 0),
        });
    }
    let owner = node_paints
        .iter()
        .filter(|p| !p.overlay && range_contains_index(p.instance, idx))
        .min_by_key(|p| p.instance.1.saturating_sub(p.instance.0));
    // Own-chrome replay is the rounded widget we dest-reset (reaction-bar
    // hover:scale). A fingerprint skip-blob (viewport covering a bubble) owns
    // the same instance index but its slot box does not match this fill —
    // replaying nothing then filling with the color *behind* the bubble punches
    // a sharp card-white hole in the rounded underlay.
    let replay = owner
        .map(|o| slot_own_chrome_replay(o, instances, ui_scale))
        .filter(|r| r.0 <= idx && idx < r.1)
        .unwrap_or((idx, idx + 1));
    let clear = uniform_opaque_fill_color_for_rect(instances, rect, |i| {
        skip_dirty(i) || (i >= replay.0 && i < replay.1)
    })
    .unwrap_or([0.0; 4]);
    Some(NodePresentDestReset {
        rect,
        clear,
        replay,
    })
}

fn uniform_opaque_fill_color_for_rect(
    instances: &[InstanceInput],
    rect: [f32; 4],
    skip: impl Fn(usize) -> bool,
) -> Option<[f32; 4]> {
    let rect = intersect_rects(rect, [0.0, 0.0, f32::MAX, f32::MAX])?;
    let mut color = None;
    for (index, inst) in instances.iter().enumerate().rev() {
        if skip(index)
            || !instance_has_dest_ink(inst)
            || intersect_rects(instance_paint_bounds(inst), rect).is_none()
        {
            continue;
        }
        if instance_aabb_clear_unsafe(inst) || color.is_some_and(|color| color != inst.color) {
            return None;
        }
        color = Some(inst.color);
        let bounds = [
            inst.rect_pos[0],
            inst.rect_pos[1],
            inst.rect_size[0],
            inst.rect_size[1],
        ];
        if crate::app::damage::rect_contains_rect(bounds, rect)
            && (inst.clip_size[0] < 0.0
                || inst.clip_size[1] < 0.0
                || crate::app::damage::rect_contains_rect(
                    [
                        inst.clip_pos[0],
                        inst.clip_pos[1],
                        inst.clip_size[0],
                        inst.clip_size[1],
                    ],
                    rect,
                ))
        {
            return color;
        }
    }
    None
}

fn node_present_behind_color_for_rect(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    rect: [f32; 4],
) -> Option<[f32; 4]> {
    let dirty_inst = dirty_slot_instance_ranges(dirty_key, dirty_bounds, node_paints);
    let skip = |i: usize| dirty_inst.iter().any(|&(a, b)| i >= a && i < b);
    uniform_opaque_fill_color_for_rect(instances, rect, skip)
}

/// Page fill inside this slot's overflow clip, even if it does not contain the
/// wipe (overlay auto-bar lives in the viewport's right padding).
fn node_present_behind_color_in_overflow(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> Option<[f32; 4]> {
    let dirty_inst = dirty_slot_instance_ranges(dirty_key, dirty_bounds, node_paints);
    let skip = |i: usize| dirty_inst.iter().any(|&(a, b)| i >= a && i < b);
    let clip = node_present_overflow_clip(dirty_key, dirty_bounds, node_paints);
    instances.iter().enumerate().rev().find_map(|(i, inst)| {
        if skip(i) || !instance_fill_opaque(inst) {
            return None;
        }
        let b = [
            inst.rect_pos[0],
            inst.rect_pos[1],
            inst.rect_size[0],
            inst.rect_size[1],
        ];
        if b[2] <= 0.0 || b[3] <= 0.0 {
            return None;
        }
        clip.filter(|c| {
            !instance_aabb_clear_unsafe(inst) && crate::app::damage::rect_contains_rect(b, *c)
        })
        .map(|_| inst.color)
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
        PaintDirtyKind::Region => {
            // Table-row / table-head fill dest-reset is Region (no 8px Node pad)
            // but uses a layout-node key. `text_hits_damage` would recapture
            // neighbor captions on the same band (gallery: data_table hover
            // stamped invoice 「Status」 as 「Sta」 on Method).
            // Layered regions reconstruct the scene in the hole, including a
            // moved plate's vacated pose. Its key still names the live plate,
            // but that plate's text range cannot restore exposed peer glyphs.
            let slot_owned = node_paints.iter().any(|p| {
                !p.overlay
                    && !p.layered
                    && p.key == dirty.key
                    && p.text.0 < p.text.1
                    && intersect_rects(p.bounds, dirty.bounds).is_some()
            });
            if slot_owned {
                collect_dirty_slot_glyphs(dirty, page_text, node_paints, &mut idx);
            } else {
                for (i, n) in page_text.iter().enumerate() {
                    if text_hits_damage(n, dirty.bounds) {
                        idx.insert(i);
                    }
                }
            }
        }
        PaintDirtyKind::Overlay => {}
        PaintDirtyKind::Node => {
            collect_dirty_slot_glyphs(dirty, page_text, node_paints, &mut idx);
        }
    }
    idx
}

fn collect_dirty_slot_glyphs(
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    indices: &mut BTreeSet<usize>,
) {
    let has_keyed = paints_has_keyed_slot(dirty.key, dirty.bounds, node_paints);
    let dirty_area = (dirty.bounds[2] * dirty.bounds[3]).max(0.0);
    for paint in node_paints {
        if !slot_matches_dirty(dirty.key, dirty.bounds, paint, has_keyed) {
            continue;
        }
        let lo = paint.text.0.min(page_text.len());
        let hi = paint.text.1.min(page_text.len());
        let broad = (paint.bounds[2] * paint.bounds[3]).max(0.0) > dirty_area * 2.0 + 1.0;
        for i in lo..hi {
            if !broad || text_hits_damage(&page_text[i], dirty.bounds) {
                indices.insert(i);
            }
        }
    }
}

/// Capture copies with `clip_bounds` intersected to the owning dirty box.
///
/// Neighbor captions stay in the buffer for a single present, but cannot paint
/// inside another widget's dest-reset hole.
#[cfg(test)]
pub(crate) fn node_present_clipped_capture_text(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> Vec<TextLayoutInfo> {
    node_present_clipped_capture_text_wipes(dirty_nodes, page_text, node_paints, &[])
}

/// Like [`node_present_clipped_capture_text`], but clip to dest-reset wipes
/// (logical px) when present. Caret blink wipes only the I-beam; clipping to
/// the flex leaf `dirty.bounds` restamped the whole placeholder (叠影).
#[cfg(test)]
pub(crate) fn node_present_clipped_capture_text_wipes(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_clips: &[(u64, [f32; 4])],
) -> Vec<TextLayoutInfo> {
    node_present_clipped_capture_text_wipes_excluding_overlay(
        dirty_nodes,
        page_text,
        node_paints,
        wipe_clips,
        &[],
        &[],
        1.0,
    )
}

/// Logical AABB of one opaque fill. `instances` are physical when `ui_scale != 1`
/// (`scale_instances` scales position, size, and transform translation).
fn logical_opaque_fill_aabb(inst: &InstanceInput, ui_scale: f32) -> Option<[f32; 4]> {
    let scale = ui_scale.max(1e-6);
    if inst.color[3] <= 0.95 || inst.rect_size[0] <= 8.0 * scale || inst.rect_size[1] <= 8.0 * scale
    {
        return None;
    }
    let local = [
        inst.rect_pos[0] / scale,
        inst.rect_pos[1] / scale,
        inst.rect_size[0] / scale,
        inst.rect_size[1] / scale,
    ];
    let mut transform = inst.transform;
    transform[4] /= scale;
    transform[5] /= scale;
    let bounds = if !transform_is_identity(&transform) {
        aabb_of_centered_transform(local, &transform)
    } else {
        local
    };
    if inst.clip_size[0] >= 0.0 && inst.clip_size[1] >= 0.0 {
        let clip = [
            inst.clip_pos[0] / scale,
            inst.clip_pos[1] / scale,
            inst.clip_size[0] / scale,
            inst.clip_size[1] / scale,
        ];
        intersect_rects(bounds, clip)
    } else {
        Some(bounds)
    }
}

/// Hole-flush TextBounds minus opaque plates this glyph does not belong to.
///
/// A viewport Region replays page fills (the jump plate included) and then
/// flushes every glyph that hits the region. The owner clip is the viewport, so
/// the wrapping line is submitted after the plate and draws through Latest.
/// The plate's own label stays; the thread is restored only where the fill
/// does not cover it.
fn clip_minus_foreign_opaque_plates(
    glyph_index: usize,
    clip: [f32; 4],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> Vec<[f32; 4]> {
    if instances.is_empty() || clip[2] <= 0.5 || clip[3] <= 0.5 {
        return vec![clip];
    }
    let mut clips = vec![clip];
    for paint in node_paints {
        if !paint.layered || paint.overlay {
            continue;
        }
        let lo = paint.text.0.min(page_text.len());
        let hi = paint.text.1.min(page_text.len());
        // Text ranges follow the collected stacking order. An earlier sibling's
        // opaque fill cannot occlude a glyph in a later (higher) page band.
        if glyph_index >= lo && glyph_index < hi || hi <= glyph_index {
            continue;
        }
        let inst_lo = paint.instance.0.min(instances.len());
        let inst_hi = paint.instance.1.min(instances.len());
        for inst in &instances[inst_lo..inst_hi] {
            let Some(plate) = logical_opaque_fill_aabb(inst, ui_scale) else {
                continue;
            };
            clips = subtract_plate_from_clips(clips, plate);
            if clips.is_empty() {
                return clips;
            }
        }
    }
    // A stacking ancestor's text range includes the wrapping run, so the loop
    // above skips that paint — and its instance range is exactly where the
    // jump fill lives (the button inherits `layered` and is collected inside
    // the scroller). Subtract those fills one instance at a time when they do
    // not contain this glyph's origin. The bubble that owns the line does
    // contain the origin, so its fill stays. Latest's origin sits on the chip,
    // so the label is not punched.
    let Some(n) = page_text.get(glyph_index) else {
        return clips;
    };
    let origin = [n.x, n.y];
    // Resolve glyph ownership once. Aggregate ancestor ranges can contain
    // thousands of instances; rescanning every paint for each one is quadratic.
    let owners: Vec<_> = node_paints
        .iter()
        .filter(|p| !p.overlay && range_contains_index(p.text, glyph_index))
        .collect();
    let preceding_instances = owners.iter().map(|p| p.instance.0).max().unwrap_or(0);
    let own_chrome: Vec<_> = owners
        .iter()
        .map(|p| slot_own_chrome_range(p, instances, ui_scale))
        .collect();
    let selection: Vec<_> = if n.style.input_caret.is_some() {
        node_paints
            .iter()
            .filter(|p| p.text == (glyph_index, glyph_index + 1))
            .map(|p| p.instance)
            .collect()
    } else {
        Vec::new()
    };
    for paint in owners {
        let inst_lo = paint.instance.0.min(instances.len());
        let inst_hi = paint.instance.1.min(instances.len());
        for (offset, inst) in instances[inst_lo..inst_hi].iter().enumerate() {
            // Aggregate ancestor ranges also contain earlier sibling fills.
            // Only fills collected after this glyph's owner can cover it.
            if inst_lo + offset < preceding_instances {
                continue;
            }
            // Ancestor backgrounds precede their glyphs even when scrolling moves
            // the glyph origin outside the background's visible bounds. A higher
            // ancestor's aggregate instance range includes those same backgrounds.
            if own_chrome
                .iter()
                .any(|range| range_contains_index(*range, inst_lo + offset))
            {
                continue;
            }
            // A selection fill owns this run even when it starts after the glyph origin.
            if selection
                .iter()
                .any(|range| range_contains_index(*range, inst_lo + offset))
            {
                continue;
            }
            let Some(plate) = logical_opaque_fill_aabb(inst, ui_scale) else {
                continue;
            };
            if point_in_rect(plate, origin) {
                continue;
            }
            clips = subtract_plate_from_clips(clips, plate);
            if clips.is_empty() {
                return clips;
            }
        }
    }
    clips
}

fn subtract_plate_from_clips(clips: Vec<[f32; 4]>, plate: [f32; 4]) -> Vec<[f32; 4]> {
    let mut next = Vec::new();
    for piece in clips {
        next.extend(
            crate::app::damage::subtract_rect(piece, plate)
                .into_iter()
                .filter(|r| r[2] > 0.5 && r[3] > 0.5),
        );
    }
    next
}

pub(crate) fn node_present_clipped_capture_text_wipes_excluding_overlay(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_clips: &[(u64, [f32; 4])],
    portal_stamps: &[(u64, [f32; 4])],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> Vec<TextLayoutInfo> {
    node_present_capture_text_indices(dirty_nodes, page_text, node_paints)
        .into_iter()
        .filter_map(|i| {
            // Hover on the trigger dirties the button, which owns this caption.
            // Recapturing it while the portal shadow animates flickers the
            // suffix that did not change. The text leaf's own dirty still captures.
            if unchanged_caption_glyph(i, dirty_nodes, node_paints, portal_stamps) {
                return None;
            }
            let n = page_text.get(i)?;
            // Wrapping that sits under Latest is recaptured by the underlay
            // glyphon (hide) or left in retain (opaque src-over). Including it
            // here flushes wrap-box TextBounds *after* the pill.
            if glyph_is_underlay_for_any_layered(
                i,
                n,
                dirty_nodes,
                page_text,
                node_paints,
                wipe_clips,
            ) {
                return None;
            }
            let mut owner =
                owner_clip_for_glyph(i, n, dirty_nodes, page_text, node_paints, wipe_clips)?;
            // The button wipe grows with the focus ring. Unioning that into the
            // caption clip moves glyphon's integer clip and every glyph snaps.
            if let Some(leaf) = trigger_caption_leaf_box(i, node_paints, portal_stamps) {
                if let Some(hit) =
                    intersect_rects(owner, padded_xywh(leaf, NODE_PRESENT_GLYPH_CLIP_PAD))
                {
                    owner = hit;
                }
            }
            // Placeholder ellipsis scissors to the leaf. I-beam at byte 0 sits on
            // that left edge (Search「S」). Intersecting the dest-reset hole with
            // the leaf puts glyphon's hard clip on the stem — CLEAR stays white.
            // Inflate overflow by the coverage kernel only; GPU flush scissor is
            // still the unpadded hole, so the kernel cannot src-over S (叠粗).
            let mut clip = match n.clip_bounds {
                Some(existing) => {
                    intersect_rects(padded_xywh(existing, NODE_PRESENT_GLYPH_CLIP_PAD), owner)?
                }
                None => owner,
            };
            // The coverage kernel may cross a leaf edge, but must stay inside
            // the owning scrollport. Otherwise partial redraw leaks long text
            // into the fixed padding while a full draw correctly clips it.
            for paint in node_paints.iter().filter(|p| p.text == (i, i + 1)) {
                if let Some(overflow) = paint.overflow_clip {
                    clip = intersect_rects(clip, overflow)?;
                }
            }
            let mut cloned = n.clone();
            cloned.clip_bounds = Some(clip);
            Some((i, cloned))
        })
        .flat_map(|(glyph_index, n)| {
            // Region replays restore the underlay before repainting the portal.
            // Node replays must preserve it and clip glyphs just like primitives.
            // Subtract portal **layout** stamps (not the visual halo) so a trigger
            // caption above AnchoredOverlay keeps its glyphs while panel pixels
            // stay out of the page capture — same contract as Node dest-reset.
            let region = dirty_nodes.iter().any(|d| {
                matches!(
                    d.kind,
                    PaintDirtyKind::Region | PaintDirtyKind::OverlayRestore
                ) && text_hits_damage(&n, d.bounds)
            });
            let pieces: Vec<TextLayoutInfo> = if region || portal_stamps.is_empty() {
                vec![n]
            } else {
                let clip = n.clip_bounds.expect("capture glyphs have owner clips");
                crate::app::damage::page_scissors_excluding_portal_boxes(clip, portal_stamps)
                    .into_iter()
                    .map(|clip| {
                        let mut piece = n.clone();
                        piece.clip_bounds = Some(clip);
                        piece
                    })
                    .collect()
            };
            let mut out = Vec::new();
            for piece in pieces {
                let Some(clip) = piece.clip_bounds else {
                    continue;
                };
                for part in clip_minus_foreign_opaque_plates(
                    glyph_index,
                    clip,
                    page_text,
                    node_paints,
                    instances,
                    ui_scale,
                ) {
                    let mut kept = piece.clone();
                    kept.clip_bounds = Some(part);
                    out.push(kept);
                }
            }
            out
        })
        .collect()
}

/// Logical box of the GPU scissor [`crate::app::damage::damage_to_scissor_physical`]
/// uses for a dest-reset wipe (floor origin, ceil far edge).
///
/// Glyphon `integerize_clip_rect` floors **both** edges, so a 2px I-beam at a
/// fractional origin CLEARs one extra device pixel. GPU flush scissor is this
/// unpadded hole. Recapture TextBounds is the hole plus a 1px coverage kernel
/// so the CLEAR pixels through Search「S」are not a hard-clip edge.
pub(crate) fn dest_reset_wipe_scissor_logical(wipe: [f32; 4], ui_scale: f32) -> [f32; 4] {
    let s = ui_scale.max(1e-6);
    let x0 = (wipe[0] * s).floor();
    let y0 = (wipe[1] * s).floor();
    let x1 = ((wipe[0] + wipe[2]) * s).ceil().max(x0 + 1.0);
    let y1 = ((wipe[1] + wipe[3]) * s).ceil().max(y0 + 1.0);
    [x0 / s, y0 / s, (x1 - x0) / s, (y1 - y0) / s]
}

/// Dest-reset CLEAR / GPU flush scissor is the unpadded wipe hole. Recapture
/// TextBounds is that hole plus a 1px coverage kernel so glyphon's hard clip
/// is not on Search「S」's stem (白线贯穿). GPU scissor stays the hole so the
/// kernel cannot src-over retain (叠粗).
pub(crate) fn node_present_glyph_clip_logical(
    dirty: &PaintDirty,
    wipe_logical: Option<[f32; 4]>,
    ui_scale: f32,
) -> [f32; 4] {
    match wipe_logical {
        Some(w) if w[2] > 0.5 && w[3] > 0.5 => padded_xywh(
            dest_reset_wipe_scissor_logical(w, ui_scale),
            NODE_PRESENT_GLYPH_CLIP_PAD,
        ),
        _ => padded_xywh(dirty.bounds, NODE_PRESENT_GLYPH_CLIP_PAD),
    }
}

fn owner_clip_for_glyph(
    i: usize,
    n: &TextLayoutInfo,
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_clips: &[(u64, [f32; 4])],
) -> Option<[f32; 4]> {
    let mut acc: Option<[f32; 4]> = None;
    for dirty in dirty_nodes {
        if glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints) {
            let owner = wipe_clips
                .iter()
                .find(|(k, _)| *k == dirty.key)
                .map(|(_, w)| *w)
                .filter(|w| w[2] > 0.5 && w[3] > 0.5)
                .unwrap_or_else(|| padded_xywh(dirty.bounds, NODE_PRESENT_GLYPH_CLIP_PAD));
            acc = Some(match acc {
                Some(prev) => union_xywh(prev, owner),
                None => owner,
            });
        }
    }
    let Some(owner) = acc else {
        return None;
    };
    // Region / expanded prev∪now dirty boxes run through the title bar.
    // Intersect to the live ScrollArea so capture cannot stamp Shimmer /
    // Delivered / On it into 「基础」.
    let overflow = dirty_nodes.iter().find_map(|dirty| {
        if !glyph_belongs_to_dirty(i, n, dirty, page_text, node_paints) {
            return None;
        }
        match dirty.kind {
            PaintDirtyKind::Node => {
                node_present_overflow_clip(dirty.key, dirty.bounds, node_paints)
            }
            PaintDirtyKind::Region => {
                fallback_overflow_clip_for_unmatched(dirty.bounds, node_paints)
            }
            PaintDirtyKind::Overlay | PaintDirtyKind::OverlayRestore => None,
        }
    });
    match overflow {
        Some(c) => intersect_rects(owner, c),
        None => Some(owner),
    }
}

/// Capture glyphs that `capture_text_renderer.render()` would stamp into `dirty`'s
/// scissor even though they are not owned by that dirty node.
#[cfg(test)]
pub(crate) fn node_present_foreign_glyph_indices(
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    capture: &BTreeSet<usize>,
    scissor_pad_logical: f32,
) -> Vec<usize> {
    if !matches!(
        dirty.kind,
        PaintDirtyKind::Node | PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region
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

fn glyph_in_dirty_slot(
    i: usize,
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty.key, dirty.bounds, node_paints);
    let Some(n) = page_text.get(i) else {
        return false;
    };
    node_paints.iter().any(|p| {
        if !slot_matches_dirty(dirty.key, dirty.bounds, p, has_keyed) {
            return false;
        }
        let lo = p.text.0.min(page_text.len());
        let hi = p.text.1.min(page_text.len());
        if i < lo || i >= hi {
            return false;
        }
        // A recycled key can temporarily point at an ancestor slot whose text
        // range still spans several children. Membership alone would flush an
        // unchanged sibling into retain. Keep the range fast path for normal
        // slots, but require the glyph to hit the actual dirty box when the
        // matched slot is materially broader than that box.
        let dirty_area = (dirty.bounds[2] * dirty.bounds[3]).max(0.0);
        let slot_area = (p.bounds[2] * p.bounds[3]).max(0.0);
        slot_area <= dirty_area * 2.0 + 1.0 || text_hits_damage(n, dirty.bounds)
    })
}

fn glyph_belongs_to_dirty(
    i: usize,
    n: &TextLayoutInfo,
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> bool {
    match dirty.kind {
        PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region => {
            text_hits_damage(n, dirty.bounds)
        }
        PaintDirtyKind::Overlay => false,
        PaintDirtyKind::Node => {
            // Prefer collect-slot ownership, including layered jump chrome.
            // A recycled broad slot additionally requires the glyph to hit the
            // dirty box; otherwise an unchanged sibling would flush after the
            // overlay slot and stamp its text into retain.
            glyph_in_dirty_slot(i, dirty, page_text, node_paints)
        }
    }
}

fn layered_underlay_hole(dirty: &PaintDirty, node_paints: &[NodePaint]) -> Option<[f32; 4]> {
    // Hide settle may skip opacity-0 ink; dest-reset still CLEARs the last
    // visual. Recapture wrapping in that hole or the thread stays punched.
    // Do not require a live layered collect slot — hide-settle Latest often
    // has no dest-ink, and gating on `p.layered` left the pill hole empty.
    let hole = match layered_overlay_slot_bounds(dirty.key, dirty.bounds, node_paints) {
        Some(slot) => union_xywh(slot, dirty.bounds),
        None => {
            if !node_paints.iter().any(|p| {
                p.layered
                    && !p.overlay
                    && p.bounds[2] > 0.5
                    && p.bounds[3] > 0.5
                    && intersect_rects(p.bounds, dirty.bounds).is_some()
            }) {
                return None;
            }
            dirty.bounds
        }
    };
    if hole[2] <= 0.5 || hole[3] <= 0.5 {
        return None;
    }
    Some(padded_xywh(hole, NODE_PRESENT_GLYPH_CLIP_PAD))
}

/// Leaf `height` can be the first-line strut while wrap ink fills a **leaf**
/// `clip_bounds` (Gallery `<Label>` under Latest). Ancestor overflow (the
/// 24rem viewport / card) must not count — that recaptured composer + Latest
/// into the pill hole and PartialLoad stacked the enter halo.
fn clip_is_leaf_wrap_box(n: &TextLayoutInfo, c: [f32; 4]) -> bool {
    if c[2] <= 0.5 || c[3] <= 0.5 {
        return false;
    }
    let leaf_w = n.width.max(1.0);
    if c[2] > leaf_w * 1.25 + 8.0 || (c[0] - n.x).abs() > 24.0 {
        return false;
    }
    let line = n.height.max(n.style.size_resolved() * 1.4).max(12.0);
    c[3] <= line * 16.0 + 16.0
}

fn text_wrap_extent(n: &TextLayoutInfo) -> [f32; 4] {
    let extra = n.style.shadow_blur.max(0.0)
        + n.style.shadow_offset[0].abs()
        + n.style.shadow_offset[1].abs();
    let mut w = n.width.max(0.0);
    let mut h = n.height.max(0.0);
    let line = n.height.max(n.style.size_resolved() * 1.4).max(12.0);
    if let Some(prepared) = n.prepared.as_ref() {
        h = h.max(crate::text::layout(prepared, n.width).height);
    } else if let Some(c) = n.clip_bounds {
        if clip_is_leaf_wrap_box(n, c) {
            w = w.max((c[0] + c[2] - n.x).max(0.0));
            h = h.max((c[1] + c[3] - n.y).max(0.0));
        } else {
            // Viewport clip is not a wrap box. First-line strut still misses
            // hide holes in later wrap lines — extend a short wrap band.
            h = h.max(line * 12.0);
        }
    } else {
        h = h.max(line * 12.0);
    }
    [
        n.x - extra,
        n.y - extra,
        (w + extra * 2.0).max(0.0),
        (h + extra * 2.0).max(0.0),
    ]
}

fn glyph_hits_layered_hole(n: &TextLayoutInfo, hole: [f32; 4]) -> bool {
    // A nowrap run already has its full measured height. The wrapping fallback
    // below invents extra lines beneath it and can classify a selected input as
    // underlay for its scrollbar, dropping the selection's normal glyph capture.
    if n.style.white_space == crate::style::WhiteSpace::Nowrap
        || n.style.font_family.as_deref() == Some("lucide")
    {
        return text_hits_damage(n, hole);
    }
    // Clip is a glyphon scissor, not the ink box. Intersecting first would
    // drop wrap lines that sit in the Latest wipe but outside a first-line
    // / viewport-shrunk `text_paint_bounds`.
    let ink = [n.x, n.y, n.width.max(0.0), n.height.max(0.0)];
    text_hits_damage(n, hole)
        || !aabbs_disjoint(ink, hole)
        || !aabbs_disjoint(text_wrap_extent(n), hole)
}

/// Glyphon `TextArea.bounds` for wrapping restored under a dest-reset hole.
///
/// Glyphon does not always honor the wgpu scissor. Recapture `clip_bounds` of
/// the wrap/viewport box stamped wrapping onto Latest's label during enter
/// ("ek ls the" over "Latest"). TextBounds must stay the wipe; wrap layout
/// origin/width stay on the cloned node so wrap lines still shape.
fn layered_underlay_recapture_clip(n: &TextLayoutInfo, hole: [f32; 4]) -> [f32; 4] {
    match n.clip_bounds {
        Some(existing) => intersect_rects(existing, hole).unwrap_or(hole),
        None => hole,
    }
}

fn glyph_is_layered_underlay(
    i: usize,
    n: &TextLayoutInfo,
    dirty: &PaintDirty,
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    hole: [f32; 4],
) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    // Dest-reset CLEARs fill ∪ outset halo. Wrapping runs in the halo must
    // recapture here or the thread stays punched / the SDF stacks in retain.
    glyph_hits_layered_hole(n, hole) && !glyph_in_dirty_slot(i, dirty, page_text, node_paints)
}

fn glyph_is_underlay_for_any_layered(
    i: usize,
    n: &TextLayoutInfo,
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_clips: &[(u64, [f32; 4])],
) -> bool {
    dirty_nodes.iter().any(|d| {
        if !matching_slot_is_layered(d.key, d.bounds, node_paints) {
            return false;
        }
        let Some(hole) = layered_underlay_hole_with_wipe(d, node_paints, wipe_clips) else {
            return false;
        };
        glyph_is_layered_underlay(i, n, d, page_text, node_paints, hole)
    })
}

fn layered_underlay_hole_with_wipe(
    dirty: &PaintDirty,
    node_paints: &[NodePaint],
    wipe_rects: &[(u64, [f32; 4])],
) -> Option<[f32; 4]> {
    let hole = layered_underlay_hole(dirty, node_paints)?;
    let wipe = wipe_rects
        .iter()
        .find(|(k, _)| *k == dirty.key)
        .map(|(_, w)| *w)
        .filter(|w| w[2] > 0.5 && w[3] > 0.5);
    // TextBounds must be the CLEAR hole. Unioning the overlay/dirty collect box
    // made wrap-box clips that glyphon stamped onto Latest.
    Some(wipe.unwrap_or(hole))
}

/// Wrapping underlay glyphs dest-reset punched, restored *under* the overlay slot.
#[cfg(test)]
pub(crate) fn node_present_layered_underlay_capture_text(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> Vec<TextLayoutInfo> {
    node_present_layered_underlay_capture_text_wipes(
        dirty_nodes,
        page_text,
        node_paints,
        &[],
        &[],
        1.0,
    )
}

/// Fill box of a live opaque layered plate, without its shadow halo.
///
/// Underlay TextBounds must not include this rect. Glyphon stamps every glyph
/// inside TextBounds, and a later piece's underlay `render` ignores the GPU
/// scissor, so a clip that covers the chip redraws the wrapping line on top of
/// the plate ("es is the" through Latest).
#[cfg(test)]
pub(crate) fn opaque_slot_fill_aabb(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> Option<[f32; 4]> {
    if instances.is_empty() {
        return None;
    }
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    let mut acc: Option<[f32; 4]> = None;
    for paint in node_paints {
        if !slot_matches_dirty(dirty_key, dirty_bounds, paint, has_keyed) || !paint.layered {
            continue;
        }
        let lo = paint.instance.0.min(instances.len());
        let hi = paint.instance.1.min(instances.len());
        for inst in &instances[lo..hi] {
            if inst.color[3] <= 0.95 || inst.rect_size[0] <= 8.0 || inst.rect_size[1] <= 8.0 {
                continue;
            }
            let local = [
                inst.rect_pos[0],
                inst.rect_pos[1],
                inst.rect_size[0],
                inst.rect_size[1],
            ];
            let bounds = if !transform_is_identity(&inst.transform) {
                aabb_of_centered_transform(local, &inst.transform)
            } else {
                local
            };
            acc = Some(match acc {
                Some(prev) => union_xywh(prev, bounds),
                None => bounds,
            });
        }
    }
    acc
}

/// Same as [`node_present_layered_underlay_capture_text`], but TextBounds is the
/// dest-reset wipe (CLEAR) minus any live opaque plate. Hide-settle last-present
/// halo is in that wipe when dest-reset unions prev; the overlay collect box
/// must not become wrap-box TextBounds ("ek ls the" over Latest).
pub(crate) fn node_present_layered_underlay_capture_text_wipes(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_rects: &[(u64, [f32; 4])],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> Vec<TextLayoutInfo> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for dirty in dirty_nodes {
        if dirty.kind != PaintDirtyKind::Node {
            continue;
        }
        let Some(hole) = layered_underlay_hole_with_wipe(dirty, node_paints, wipe_rects) else {
            continue;
        };
        for (i, n) in page_text.iter().enumerate() {
            if seen.contains(&i) {
                continue;
            }
            if !glyph_is_layered_underlay(i, n, dirty, page_text, node_paints, hole) {
                continue;
            }
            // Do not use `clip_text_node_to_rect`: first-line hit-tests drop wrap
            // lines in the pill hole. TextBounds must still be the wipe — a wrap
            // / viewport clip stamps wrapping onto Latest (glyphon ignores GPU
            // scissor). Wrap layout origin/width stay on the cloned node.
            let hit = layered_underlay_recapture_clip(n, hole);
            if hit[2] <= 0.5 || hit[3] <= 0.5 {
                continue;
            }
            // The plate occludes. Remainder clips are the trail the fill does
            // not cover. A clip of the chip interior is the line on Latest.
            // Same subtraction as the post-plate hole flush: an ancestor whose
            // text range includes this run still owns the jump fill.
            let clips = clip_minus_foreign_opaque_plates(
                i,
                hit,
                page_text,
                node_paints,
                instances,
                ui_scale,
            );
            if clips.is_empty() {
                seen.insert(i);
                continue;
            }
            for clip in clips {
                let mut cloned = n.clone();
                cloned.clip_bounds = Some(clip);
                out.push(cloned);
            }
            seen.insert(i);
        }
    }
    out
}

/// Skip inner dest-reset only when a larger dirty ancestor will redraw the hole.
///
/// Opaque own fill is the common case (cards / fields). InputAdornment shells are
/// transparent-fill with border + Hard ring — still own chrome that dest-resets and
/// redraws. Geometry-only nesting without that chrome is not enough: gallery Item
/// outline→default is larger than its title but had no own fill, so skipping the
/// title wipe left `"outline"` glyphs under `"default"`.
///
/// Own chrome only — Item's collect range includes descendant quads (gear icon).
pub fn paint_dirty_skips_node_dest_reset_covered(
    dirty: &PaintDirty,
    nodes: &[PaintDirty],
    layered: bool,
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> bool {
    if layered || dirty.kind != PaintDirtyKind::Node {
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
            && node_dirty_repaints_nested_hole(o.key, o.bounds, node_paints, instances, ui_scale)
    })
}

/// Ancestor Node dirty already dest-resets and redraws this hole (InputAdornment
/// shell ⊃ Input). Encode must skip the nested slot entirely — skipping only
/// dest-reset still re-drew placeholder+caret and punched the Hard ring.
pub fn skip_covered_nested_node_present(
    dirty: &PaintDirty,
    nodes: &[PaintDirty],
    layered: bool,
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> bool {
    dirty.kind == PaintDirtyKind::Node
        && paint_dirty_skips_node_dest_reset_covered(
            dirty,
            nodes,
            layered,
            node_paints,
            instances,
            ui_scale,
        )
        && !slot_visual_exceeds_rest(dirty.key, node_paints)
}

/// Larger dirty redraws the hole: opaque fill, or own border / Hard-ring chrome
/// (InputAdornment shell is `background` fill + ring; plain transparent parents still fail).
fn node_dirty_repaints_nested_hole(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
    ui_scale: f32,
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|p| {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            return false;
        }
        let (lo, hi) = slot_own_chrome_replay(p, instances, ui_scale);
        instances[lo..hi].iter().any(|inst| {
            instance_fill_opaque(inst)
                || (inst.border_width > 0.5 && inst.border_color[3] > 0.05)
                || (inst.shadow_spread.abs() > 1.0 && inst.shadow_color[3] > 0.05)
        })
    })
}

/// Matching collect slot owns glyphon (paint-only `bind_text` labels have no SDF fill).
pub(crate) fn node_dirty_has_text(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints
        .iter()
        .any(|p| slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) && p.text.0 < p.text.1)
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
    chrome: &[InstanceInput],
    ui_scale: f32,
    prev_bounds: Option<[f32; 4]>,
) -> bool {
    // Must match encode's dest-reset (includes I-beam chrome). Scaled-only
    // missed the caret and skipped capture — blink ON punched a hole in the
    // glyph under the I-beam (光标脏到文字).
    node_present_dest_reset_full(
        dirty_key,
        dirty_bounds,
        node_paints,
        instances,
        textured,
        chrome,
        ui_scale,
        prev_bounds,
    )
    .is_some()
        || node_dirty_has_opaque_fill(dirty_key, dirty_bounds, node_paints, instances)
        // Missing last-present (`None`) is first present — still capture.
        // Caret OFF stores an empty box; dest-reset is skipped and retain
        // already has the glyphs (blur must not src-over placeholder).
        || (prev_bounds.is_none() && node_dirty_has_text(dirty_key, dirty_bounds, node_paints))
}

/// Hover chrome whose fill covers this dirty node's glyphs (primary vs link/ghost rest).
pub(crate) fn node_dirty_has_opaque_fill(
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    node_paints.iter().any(|p| {
        if !slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
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
#[path = "tests/draw_cull.rs"]
mod tests;

#[cfg(test)]
include!("tests/draw_cull_cases.rs");
