//! Cull collect buffers to a damage AABB before GPU encode (B8).
//!
//! Only valid with retain `LoadOp::Load` + scissor: FullClear to a discarded
//! swapchain would leave holes. Scroll / structure still FullClear (caller).

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::layout::layout_tree::{NodePaint, TextLayoutInfo};
use crate::renderer::{
    InstanceInput, ScaledTexturedDraw, aabb_of_centered_transform, aabbs_disjoint,
};
use crate::style::transform::MAT2D_IDENTITY;

use super::damage::{PaintDirty, PaintDirtyKind, intersect_rects};

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
        if s.len == 0 { None } else { Some(s) }
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

    #[cfg(test)]
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

/// Final scroll repair coverage. Inputs already include each owner's paint
/// outset and overflow clip: adding another generic pad leaks into the header.
pub(crate) fn scroll_repair_rects_physical(
    jobs: &[super::damage::ScrollStripBlitJob],
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
        super::damage::push_disjoint_rect(&mut rects, r);
    }
    CullRects::from_rects(rects)
}

/// Text preparation coverage for visible Region/OverlayRestore clears.
/// Clip each hole before unioning: offscreen holes on opposite sides of the
/// viewport must not cause every intervening label to be prepared again.
pub(crate) fn region_repair_text_cull(
    dirties: &[PaintDirty],
    previous: &[(u64, [f32; 4])],
    paints: &[NodePaint],
    scale: f32,
    width: u32,
    height: u32,
) -> CullRects {
    let mut bounds = None;
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
            super::damage::damage_to_scissor_physical(hole, 0.0, scale, width, height)
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
        bounds = Some(match bounds {
            Some(prev) => super::damage::union_rects(prev, hole),
            None => hole,
        });
    }
    bounds.map(CullRects::one).unwrap_or_else(CullRects::empty)
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
    dirty: &super::damage::PaintDirty,
    node_paints: &[NodePaint],
    instances: &[InstanceInput],
) -> bool {
    if dirty.kind != super::damage::PaintDirtyKind::Node {
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
        bounds
    } else {
        aabb_of_centered_transform(bounds, &i.transform)
    }
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
fn instance_has_dest_ink(i: &InstanceInput) -> bool {
    i.color[3] > 0.05
        || (i.border_width > 0.0 && i.border_color[3] > 0.05)
        || instance_has_outset_shadow(i)
}

/// Filling this instance's AABB with `color` would paint outside its SDF
/// (capsule corners, outline ring). Dest-reset must clear with the layer
/// underneath and replay this chrome instead.
fn instance_aabb_clear_unsafe(i: &InstanceInput) -> bool {
    i.radii.iter().any(|r| *r > 0.5)
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
    super::damage::paint_dirty_has_keyed_slot(
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
    super::damage::paint_dirty_matches_present_slot(
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
    live_scroll_overflow_clips(node_paints).any(|c| {
        if !dirty_entirely_above_overflow(dirty_bounds, c) {
            return false;
        }
        match prev_bounds.filter(|p| p[2] > 0.5 && p[3] > 0.5) {
            Some(prev) => prev_intersects_overflow(prev, c),
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
            // Layered ping/bounce stay on the fill so occupancy can clip a
            // rest-disjoint caption. Latest outset shadow must wipe the SDF halo
            // or PartialLoad stacks every fade frame (enter ring / hide ghost).
            let mut b = if layered && !instance_has_outset_shadow(inst) {
                instance_fill_bounds(inst)
            } else {
                instance_paint_bounds(inst)
            };
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
                clip_node_present_rect(r, dirty_key, dirty_bounds, node_paints, s, layered)
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
    for (ir, or) in [
        (inner.instance, outer.instance),
        (inner.textured, outer.textured),
        (inner.text, outer.text),
        (inner.chrome, outer.chrome),
    ] {
        if ir.1 <= ir.0 {
            continue;
        }
        if or.1 <= or.0 {
            // I-beam chrome is collected on the text leaf, not duplicated on the
            // wrap. Treating that as "not nested" made the wrap own the caret
            // last-present; blur then dest-reset the wrap and flashed ⌘K / Kbd.
            continue;
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
            return true;
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
        .any(|q| is_nested_foreign_slot(parent, q) && range_contains_index(range(q), i))
}

/// Dest-reset ink of a wrap must not include a descendant's bake.
///
/// [`nested_foreign_owns`] also treats same-box wrap↔leaf (equal collect spans)
/// as foreign — that dropped the lucide dest-reset on the spinning leaf, while
/// the Marker row still clamped last-present (caption ∪ bake) to bake+48px.
fn nested_descendant_owns(
    parent: &NodePaint,
    paints: &[NodePaint],
    i: usize,
    range: impl Fn(&NodePaint) -> (usize, usize),
) -> bool {
    paints.iter().any(|q| {
        q.key != parent.key
            && !q.overlay
            && occupancy_descendant_slot(parent, q)
            && range_contains_index(range(q), i)
    })
}

fn slot_has_sdf_dest_ink(p: &NodePaint, instances: &[InstanceInput]) -> bool {
    let lo = p.instance.0.min(instances.len());
    let hi = p.instance.1.min(instances.len());
    instances[lo..hi].iter().any(instance_has_dest_ink)
}

/// Collect-slot visual box plus this frame's transformed instance / texture / glyph ink.
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
pub(crate) fn node_paint_visual_bounds_in(
    p: &NodePaint,
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    texts: &[TextLayoutInfo],
    chrome: &[InstanceInput],
    paints: &[NodePaint],
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
        if nested_foreign_owns(p, paints, lo + off, |q| q.instance) {
            continue;
        }
        // Layered last-present must cover the SDF halo so hide-settle dest-reset
        // can erase it. Occupancy still keeps the overlay rail out of the wipe.
        let b = if p.layered && !instance_has_outset_shadow(inst) {
            instance_fill_bounds(inst)
        } else {
            instance_paint_bounds(inst)
        };
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
        if nested_foreign_owns(p, paints, tlo + off, |q| q.textured) {
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
        if nested_foreign_owns(p, paints, glo + off, |q| q.text) {
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
        .filter_map(|p| {
            let b = node_paint_visual_bounds_in(p, instances, textured, texts, chrome, node_paints);
            // Skip phantom `[layout_origin, 0×0]` (caret blink OFF). Recording that
            // origin made the next I-beam look translated from the flex leaf.
            if b[2] <= 0.5 || b[3] <= 0.5 {
                None
            } else {
                Some((p.key, b))
            }
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
        let next =
            super::damage::escalate_node_dest_reset_kind(dirty.kind, found_current, had_previous);
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

pub(crate) fn prepare_node_present_dirties(
    dirties: &mut [PaintDirty],
    node_paints: &[NodePaint],
    last_present: &[(u64, [f32; 4])],
    instances: &[InstanceInput],
    textured: &[ScaledTexturedDraw],
    chrome: &[InstanceInput],
    ui_scale: f32,
    overlay: Option<[f32; 4]>,
) {
    escalate_dirties_for_node_present(dirties, node_paints, last_present);
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
        if matching_slot_is_layered(dirty.key, dirty.bounds, node_paints) {
            // A stacking layer is a composite, not an independently erasable
            // text/fill node. Background glyph restoration followed by a global
            // label flush interleaves Latest and its text children incorrectly.
            // Rebuild the local hole in page-band order, including the last
            // visible frame when the layer has just reached opacity zero.
            let mut bounds = dirty.bounds;
            if let Some(old) = prev {
                bounds = union_xywh(bounds, old);
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
                prev,
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
        let src_over =
            layered_src_over_skips_dest_reset(dirty.key, dirty.bounds, node_paints, prev);
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
        if overlay.is_some_and(|occ| intersect_rects(logical, occ).is_some()) {
            // Portal AABBs include translucent shadows/AA. Treating the entire
            // AABB as opaque preserves stale underlay at those pixels. Rebuild
            // the local composite and replay the portal in the same hole.
            dirty.kind = PaintDirtyKind::Region;
            dirty.bounds = logical;
            continue;
        }
        if src_over || reset.is_some() {
            continue;
        }
        if let Some(rect) = reset_rect {
            dirty.bounds = [rect[0] / s, rect[1] / s, rect[2] / s, rect[3] / s];
            dirty.kind = PaintDirtyKind::Region;
        }
    }
    // A concurrent in-flow caption must not be flushed after a Region that
    // already composited an overlapping Latest layer. Promote connected dirty
    // coverage too; unrelated nodes retain their independent Node fast path.
    let mut regions: Vec<_> = dirties
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                PaintDirtyKind::Region | PaintDirtyKind::OverlayRestore
            )
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
            dirty.bounds = bounds;
            regions.push(bounds);
        }
    }
    super::damage::sort_paint_dirty_nodes(dirties);
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
        clip_node_present_rect(scissor, dirty_key, dirty_bounds, node_paints, 1.0, layered)
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
    if dirty_bounds[2] > 0.5
        && dirty_bounds[3] > 0.5
        && node_paints.iter().any(|p| {
            p.layered
                && !p.overlay
                && p.key != dirty_key
                && p.bounds[2] > 0.5
                && p.bounds[3] > 0.5
                && p.bounds[3] <= dirty_bounds[3] * 3.0 + 24.0
                && intersect_rects(p.bounds, dirty_bounds).is_some()
        })
    {
        return true;
    }
    // Unkeyed chrome: recapture wrapping whenever this Node dirty overlaps
    // a layered underlay (not a foreign page slot).
    let has_keyed = paints_has_keyed_slot(dirty_key, dirty_bounds, node_paints);
    if has_keyed {
        return false;
    }
    dirty_bounds[2] > 0.5
        && dirty_bounds[3] > 0.5
        && node_paints.iter().any(|p| {
            p.layered
                && !p.overlay
                && p.bounds[2] > 0.5
                && p.bounds[3] > 0.5
                && intersect_rects(p.bounds, dirty_bounds).is_some()
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

/// Dest-reset / node-present scissor occupancy clip.
///
/// Layered slots (absolute ping, jump pills) still punch rest-overlapping
/// underlay, but must not paint or wipe a rest-disjoint caption above — GPU
/// scissor used to stay unclipped, so `animate-ping` src-overed the label.
///
/// In-flow `animate-spin` grows to a circumcircle. Skipping occupancy clip let
/// the 8px node scissor dest-reset the caption every tick (ghost 「play-state」).
/// Clip rest-disjoint captions again; the gallery stage keeps the diamond ink
/// off the title so the clip only removes pad leak, not leftover frames.
fn clip_node_present_rect(
    rect: [f32; 4],
    dirty_key: u64,
    dirty_bounds: [f32; 4],
    node_paints: &[NodePaint],
    slot_scale: f32,
    layered: bool,
) -> [f32; 4] {
    if layered {
        let r = clip_rect_against_layered_siblings(
            rect,
            dirty_key,
            dirty_bounds,
            node_paints,
            slot_scale,
        );
        clip_rect_against_foreign_siblings(r, dirty_key, dirty_bounds, node_paints, slot_scale)
    } else {
        clip_rect_against_foreign_siblings(rect, dirty_key, dirty_bounds, node_paints, slot_scale)
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
    for p in node_paints {
        if !p.layered || p.overlay || p.bounds[2] <= 0.0 || p.bounds[3] <= 0.0 {
            continue;
        }
        if slot_matches_dirty(dirty_key, dirty_bounds, p, has_keyed) {
            continue;
        }
        let related = node_paints.iter().any(|m| {
            slot_matches_dirty(dirty_key, dirty_bounds, m, has_keyed)
                && (slot_ranges_covered_by(m, p) || slot_ranges_covered_by(p, m))
        });
        if related {
            continue;
        }
        let occ = scale_xywh(node_paint_occupancy(p), s);
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
        rect = shrink_aabb_to_exclude(rect, occ);
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    rect
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
    let rest_host = rest_host.unwrap_or(host);
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
        let related = node_paints.iter().any(|m| {
            slot_matches_dirty(dirty_key, dirty_bounds, m, has_keyed)
                && (slot_ranges_covered_by(m, p) || slot_ranges_covered_by(p, m))
        });
        if related {
            continue;
        }
        let fb = scale_xywh(p.bounds, s);
        let occ = scale_xywh(node_paint_occupancy(p), s);
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
            if !super::damage::is_rest_stack_chip(rest_host)
                || !super::damage::is_rest_stack_chip(occ)
            {
                continue;
            }
        }
        if intersect_rects(rect, occ).is_none() {
            continue;
        }
        rect = shrink_aabb_to_exclude(rect, occ);
        if rect[2] <= 0.5 || rect[3] <= 0.5 {
            return [0.0, 0.0, 0.0, 0.0];
        }
    }
    rect
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
    if best_area < 0.0 { wipe } else { best }
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
) -> bool {
    if dirty.kind != PaintDirtyKind::Node {
        return false;
    }
    if matching_slot_is_layered(dirty.key, dirty.bounds, node_paints) {
        return false;
    }
    nodes.iter().any(|o| {
        o.kind == PaintDirtyKind::Node
            && o.key != dirty.key
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
    if layered_src_over_skips_dest_reset(dirty_key, dirty_bounds, node_paints, prev_bounds) {
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
    let dirty_inst = dirty_slot_instance_ranges(dirty_key, dirty_bounds, node_paints);
    let skip_dirty = |i: usize| dirty_inst.iter().any(|&(a, b)| i >= a && i < b);
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
        return None;
    };
    let behind = &instances[idx];
    if !instance_aabb_clear_unsafe(behind) {
        return Some(NodePresentDestReset {
            rect,
            clear: behind.color,
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
    // Rounded underlay with nothing further behind (gallery Item on a rounded
    // card, no page fill at the sample): falling through to None skipped the
    // wipe and PartialLoad kept the previous 1px outline. CLEAR the hole with
    // the underlay's own color; replay still redraws the SDF in the scissor.
    let clear = opaque_fill_color_at_skipping(instances, cx, cy, |i| {
        skip_dirty(i) || (i >= replay.0 && i < replay.1)
    })
    .unwrap_or(behind.color);
    Some(NodePresentDestReset {
        rect,
        clear,
        replay,
    })
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
        if let Some(c) = clip {
            intersect_rects(b, c).map(|_| inst.color)
        } else {
            Some(inst.color)
        }
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
            let slot_owned = node_paints
                .iter()
                .any(|p| !p.overlay && p.key == dirty.key && p.text.0 < p.text.1);
            if slot_owned {
                for (i, _n) in page_text.iter().enumerate() {
                    if glyph_in_dirty_slot(i, dirty, page_text, node_paints) {
                        idx.insert(i);
                    }
                }
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
    node_present_clipped_capture_text_wipes(dirty_nodes, page_text, node_paints, &[])
}

/// Like [`node_present_clipped_capture_text`], but clip to dest-reset wipes
/// (logical px) when present. Caret blink wipes only the I-beam; clipping to
/// the flex leaf `dirty.bounds` restamped the whole placeholder (叠影).
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
        None,
    )
}

pub(crate) fn node_present_clipped_capture_text_wipes_excluding_overlay(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_clips: &[(u64, [f32; 4])],
    overlay: Option<[f32; 4]>,
) -> Vec<TextLayoutInfo> {
    node_present_capture_text_indices(dirty_nodes, page_text, node_paints)
        .into_iter()
        .filter_map(|i| {
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
            let owner =
                owner_clip_for_glyph(i, n, dirty_nodes, page_text, node_paints, wipe_clips)?;
            // Placeholder ellipsis scissors to the leaf. I-beam at byte 0 sits on
            // that left edge (Search「S」). Intersecting the dest-reset hole with
            // the leaf puts glyphon's hard clip on the stem — CLEAR stays white.
            // Inflate overflow by the coverage kernel only; GPU flush scissor is
            // still the unpadded hole, so the kernel cannot src-over S (叠粗).
            let clip = match n.clip_bounds {
                Some(existing) => {
                    intersect_rects(padded_xywh(existing, NODE_PRESENT_GLYPH_CLIP_PAD), owner)?
                }
                None => owner,
            };
            let mut cloned = n.clone();
            cloned.clip_bounds = Some(clip);
            Some(cloned)
        })
        .flat_map(|n| {
            // Region replays restore the underlay before repainting the portal.
            // Node replays must preserve it and clip glyphs just like primitives.
            let region = dirty_nodes.iter().any(|d| {
                matches!(
                    d.kind,
                    PaintDirtyKind::Region | PaintDirtyKind::OverlayRestore
                ) && text_hits_damage(&n, d.bounds)
            });
            if region || overlay.is_none() {
                return vec![n];
            }
            let clip = n.clip_bounds.expect("capture glyphs have owner clips");
            super::damage::page_scissors_excluding_overlay(clip, overlay)
                .into_iter()
                .map(|clip| {
                    let mut piece = n.clone();
                    piece.clip_bounds = Some(clip);
                    piece
                })
                .collect()
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
    node_paints.iter().any(|p| {
        slot_matches_dirty(dirty.key, dirty.bounds, p, has_keyed) && {
            let lo = p.text.0.min(page_text.len());
            let hi = p.text.1.min(page_text.len());
            i >= lo && i < hi
        }
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
            // Collect-slot membership only — including layered jump chrome.
            // Underlay wrapping runs that merely intersect the shadow pad used
            // to join this set and flush *after* the overlay slot, stamping the
            // thread on top of Latest.
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
    if let Some(c) = n.clip_bounds {
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
pub(crate) fn node_present_layered_underlay_capture_text(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
) -> Vec<TextLayoutInfo> {
    node_present_layered_underlay_capture_text_wipes(dirty_nodes, page_text, node_paints, &[])
}

/// Same as [`node_present_layered_underlay_capture_text`], but TextBounds is the
/// dest-reset wipe (CLEAR). Hide-settle last-present halo is in that wipe when
/// dest-reset unions prev; the overlay collect box must not become wrap-box
/// TextBounds ("ek ls the" over Latest).
pub(crate) fn node_present_layered_underlay_capture_text_wipes(
    dirty_nodes: &[PaintDirty],
    page_text: &[TextLayoutInfo],
    node_paints: &[NodePaint],
    wipe_rects: &[(u64, [f32; 4])],
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
            let mut cloned = n.clone();
            cloned.clip_bounds = Some(hit);
            seen.insert(i);
            out.push(cloned);
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

    #[test]
    fn layered_underlay_must_recapture_wrap_lines_below_first_strut() {
        // Gallery `<Label>` under Latest: collect height is the first-line
        // strut; later wrap lines live in clip_bounds. Hide-settle dest-reset
        // punches "removed from the tab order…" and first-line hit-tests miss.
        let mut wrap = sample_text(
            "The scroll button is a real button with a label, and it's removed from the tab order when you're already at the bottom — no ghost focus stops.",
            16.0,
        );
        wrap.y = 200.0;
        wrap.width = 280.0;
        wrap.height = 18.0;
        wrap.clip_bounds = Some([16.0, 200.0, 280.0, 72.0]);
        wrap.paint_origin = [16.0, 200.0];
        let hole = [80.0, 230.0, 100.0, 36.0];
        assert!(
            !text_hits_damage(&wrap, hole),
            "precondition: first-line strut must miss the hide hole"
        );
        let mut latest = slot(9, [110.0, 228.0, 80.0, 32.0], (0, 0));
        latest.layered = true;
        let dirty = crate::app::damage::PaintDirty {
            key: 9,
            bounds: hole,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let underlay = node_present_layered_underlay_capture_text_wipes(
            &[dirty],
            &[wrap.clone()],
            &[latest],
            &[(9, hole)],
        );
        assert!(
            underlay
                .iter()
                .any(|t| t.text.contains("removed from the tab order")),
            "hide settle must recapture wrap lines below the first-line strut, got {}",
            underlay.len()
        );
        let clip = underlay[0].clip_bounds.expect("wrap recapture clip");
        assert!(
            (underlay[0].x - wrap.x).abs() < 0.5 && (underlay[0].width - wrap.width).abs() < 0.5,
            "recapture must keep wrap layout origin/width so wrap lines still shape\n run=({},{},{},{}) wrap=({},{},{},{})",
            underlay[0].x,
            underlay[0].y,
            underlay[0].width,
            underlay[0].height,
            wrap.x,
            wrap.y,
            wrap.width,
            wrap.height
        );
        assert!(
            clip[2] < wrap.width - 8.0 && clip[3] < 80.0,
            "glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} hole={hole:?} wrap_w={}",
            wrap.width
        );
    }

    #[test]
    fn latest_fade_and_wrap_dirty_must_not_stamp_thread_on_pill() {
        // Click-to-bottom / enter fade: wrapping dest-reset + hole_text flush
        // after Latest stamped "gh" / "as they stream" onto the translucent pill.
        let btn = [110.0, 228.0, 80.0, 32.0];
        let wrap_box = [16.0, 200.0, 280.0, 72.0];
        let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
        card.color = [1.0, 1.0, 1.0, 1.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 0.4];
        btn_fill.radii = [16.0; 4];
        let mut wrap = sample_text("bottom — no ghost focus stops.", 16.0);
        wrap.y = 220.0;
        wrap.width = 280.0;
        wrap.height = 18.0;
        wrap.clip_bounds = Some(wrap_box);
        wrap.paint_origin = [16.0, 220.0];
        let mut latest_label = sample_text("Latest", btn[0] + 20.0);
        latest_label.y = btn[1] + 8.0;
        latest_label.width = 40.0;
        latest_label.paint_origin = [btn[0] + 20.0, btn[1] + 8.0];
        let wrap_p = slot(1, wrap_box, (0, 1));
        let mut btn_p = slot(2, btn, (1, 2));
        btn_p.layered = true;
        btn_p.instance = (1, 2);
        let instances = [card, btn_fill];
        let paints = [wrap_p, btn_p];
        let wrap_dirty = crate::app::damage::PaintDirty {
            key: 1,
            bounds: wrap_box,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let btn_dirty = crate::app::damage::PaintDirty {
            key: 2,
            bounds: btn,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let dirties = [wrap_dirty, btn_dirty];
        assert!(
            skip_in_flow_under_layered_overlay(&wrap_dirty, &dirties, &paints),
            "wrapping dest-reset under Latest must yield the overlap to the overlay slot"
        );
        assert!(
            !skip_in_flow_under_layered_overlay(&btn_dirty, &dirties, &paints),
            "Latest itself is the overlay slot"
        );
        assert!(
            node_present_dest_reset_full(2, btn, &paints, &instances, &[], &[], 1.0, None)
                .is_none(),
            "first visible Latest src-overs wrapping still in retain"
        );
        let fade =
            node_present_dest_reset_full(2, btn, &paints, &instances, &[], &[], 1.0, Some(btn))
                .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            fade.rect[2] <= btn[2] + 40.0,
            "fade dest-reset must stay on the pill, wipe={:?}",
            fade.rect
        );
        assert!(
            node_present_flush_layered_underlay_glyphs(2, btn, &paints),
            "fade dest-reset must recapture wrapping in the wipe before Latest paints"
        );
        let underlay = node_present_layered_underlay_capture_text_wipes(
            &[btn_dirty],
            &[wrap.clone(), latest_label.clone()],
            &paints,
            &[(2, btn)],
        );
        assert!(
            underlay.iter().any(|t| t.text == wrap.text),
            "wrapping in the dest-reset hole must restore under the pill, got {:?}",
            underlay.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
        );
        assert!(
            underlay.iter().all(|t| t.text != "Latest"),
            "underlay must not steal Latest's label"
        );
        let hole_text = node_present_clipped_capture_text_wipes(
            &dirties,
            &[wrap.clone(), latest_label.clone()],
            &paints,
            &[(2, btn)],
        );
        assert!(
            hole_text.iter().all(|t| t.text != wrap.text),
            "page capture must not flush wrapping after the pill, got {:?}",
            hole_text
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            hole_text.iter().any(|t| t.text == "Latest"),
            "Latest label stays in the overlay slot capture"
        );

        let mut btn_hidden = btn_p;
        btn_hidden.instance = (2, 2);
        btn_hidden.text = (1, 1);
        assert!(
            node_present_flush_layered_underlay_glyphs(2, btn, &[wrap_p, btn_hidden]),
            "hide settle must recapture wrapping in the dest-reset hole"
        );
        let opaque = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        let mut opaque_fill = opaque;
        opaque_fill.color = [1.0, 1.0, 1.0, 1.0];
        opaque_fill.radii = [16.0; 4];
        let opaque_reset = node_present_dest_reset_full(
            2,
            btn,
            &paints,
            &[card, opaque_fill],
            &[],
            &[],
            1.0,
            Some(btn),
        )
        .expect("settled Latest with last-present dest-resets leftover (enter ghost)");
        assert!(
            opaque_reset.rect[2] <= btn[2] + 40.0,
            "settled dest-reset must stay on the pill, wipe={:?}",
            opaque_reset.rect
        );
    }

    #[test]
    fn layered_underlay_recapture_must_keep_mid_line_wrap_layout_clip() {
        // Gallery follow-up: "Got it. Stay at the live edge and I'll keep following."
        // Hide dest-reset CLEARs the pill in the middle of the line. Wrap-box
        // TextBounds stamped wrapping onto Latest; hole TextBounds must keep
        // wrap layout origin so mid-line glyphs still shape.
        let mut wrap = sample_text(
            "Got it. Stay at the live edge and I'll keep following.",
            16.0,
        );
        wrap.y = 320.0;
        wrap.width = 280.0;
        wrap.height = 18.0;
        wrap.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
        wrap.paint_origin = [16.0, 320.0];
        let hole = [80.0, 318.0, 100.0, 36.0];
        assert!(
            !aabbs_disjoint([wrap.x, wrap.y, wrap.width, wrap.height], hole),
            "precondition: first-line strut must overlap the hide hole"
        );
        let mut latest = slot(9, [110.0, 328.0, 80.0, 32.0], (0, 0));
        latest.layered = true;
        let dirty = crate::app::damage::PaintDirty {
            key: 9,
            bounds: hole,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let underlay = node_present_layered_underlay_capture_text_wipes(
            &[dirty],
            &[wrap.clone()],
            &[latest],
            &[(9, hole)],
        );
        let captured = underlay
            .iter()
            .find(|t| t.text.contains("keep following"))
            .expect("hide settle must recapture the punched wrap line");
        assert!(
            (captured.x - wrap.x).abs() < 0.5 && (captured.width - wrap.width).abs() < 0.5,
            "recapture must keep wrap layout origin/width\n x={} w={} wrap_x={} wrap_w={}",
            captured.x,
            captured.width,
            wrap.x,
            wrap.width
        );
        let clip = captured.clip_bounds.expect("wrap recapture clip");
        assert!(
            clip[2] < wrap.width - 8.0 && clip[3] < 80.0,
            "glyphon TextBounds must stay the wipe or wrapping stamps onto Latest\n clip={clip:?} hole={hole:?} wrap_w={}",
            wrap.width
        );
    }

    #[test]
    fn viewport_clip_must_not_recapture_foreign_caption_or_latest_as_underlay() {
        // Ancestor overflow used to inflate wrap extent to the 24rem viewport.
        // Hide-settle then stacked composer + "Latest" in the pill hole.
        let mut caption = sample_text("Thanks — that matches Slack / iMessage.", 16.0);
        caption.y = 24.0;
        caption.width = 240.0;
        caption.height = 18.0;
        caption.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
        caption.paint_origin = [16.0, 24.0];
        let mut latest = sample_text("Latest", 120.0);
        latest.y = 340.0;
        latest.width = 64.0;
        latest.height = 16.0;
        latest.clip_bounds = Some([0.0, 0.0, 320.0, 384.0]);
        latest.paint_origin = [120.0, 340.0];
        let mut composer = sample_text("No messages queued. Reset the conversation.", 16.0);
        composer.y = 400.0;
        composer.width = 288.0;
        composer.height = 18.0;
        composer.clip_bounds = Some([0.0, 0.0, 320.0, 480.0]);
        composer.paint_origin = [16.0, 400.0];
        let hole = [80.0, 330.0, 100.0, 36.0];
        let mut pill = slot(9, [110.0, 328.0, 80.0, 32.0], (1, 2));
        pill.layered = true;
        let dirty = crate::app::damage::PaintDirty {
            key: 9,
            bounds: hole,
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let underlay = node_present_layered_underlay_capture_text_wipes(
            &[dirty],
            &[caption, latest, composer],
            &[pill],
            &[(9, hole)],
        );
        assert!(
            underlay.iter().all(|t| t.text != "Latest"
                && !t.text.contains("Slack")
                && !t.text.contains("queued")),
            "viewport clip must not pull foreign/Latest/composer into the pill hole, got {:?}",
            underlay.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
        );
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
            layered: false,
            rest: None,
        }
    }

    #[test]
    fn layered_clip_must_not_cut_overlay_rail_against_full_width_jump_dock() {
        // Latest dock is a full-width empty bar. Occupancy clip against it used
        // to chop the fading overlay rail, leaving a groove+thumb remnant.
        let mut rail = slot(1, [310.0, 2.0, 10.0, 380.0], (0, 0));
        rail.overlay = true;
        rail.layered = true;
        rail.rest = Some(rail.bounds);
        let mut dock = slot(2, [0.0, 336.0, 320.0, 32.0], (0, 0));
        dock.layered = true;
        dock.rest = Some(dock.bounds);
        let wipe = clip_rect_against_layered_siblings(
            rail.bounds,
            rail.key,
            rail.bounds,
            &[rail, dock],
            1.0,
        );
        let remnant = [310.0, 336.0, 10.0, 32.0];
        assert!(
            intersect_rects(wipe, remnant).is_some(),
            "rail wipe must still overlap Latest's Y, wipe={wipe:?}"
        );
        assert!(
            wipe[1] + wipe[3] + 1.0 >= remnant[1] + remnant[3],
            "rail wipe must keep the bottom under Latest, wipe={wipe:?} remnant={remnant:?}"
        );
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

    /// Nested Select accordion occupancy is the stamp. Overlay glyphon ignores the
    /// wgpu scissor used for chrome, so labels below the clip survive as leftover ink.
    #[test]
    fn overlay_stamp_clips_select_labels_below_accordion() {
        let mut hex = sample_text("HEX", 8.0);
        hex.y = 8.0;
        hex.width = 64.0;
        hex.height = 32.0;
        let mut rgb = sample_text("RGB", 8.0);
        rgb.y = 80.0;
        rgb.width = 64.0;
        rgb.height = 32.0;
        let stamp = [0.0, 0.0, 80.0, 40.0];
        let kept = clip_text_nodes_to_rect(&[hex, rgb], stamp);
        assert!(
            kept.iter().any(|n| n.text == "HEX"),
            "visible first row must stay, got {:?}",
            kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
        );
        assert!(
            kept.iter().all(|n| n.text != "RGB"),
            "labels below accordion must not enter overlay glyphon, got {:?}",
            kept.iter().map(|n| n.text.as_str()).collect::<Vec<_>>()
        );
        for n in &kept {
            let pb = text_paint_bounds(n);
            assert!(
                pb[1] + pb[3] <= stamp[1] + stamp[3] + 1.0,
                "prepared overlay text must clip to the stamp, paint={pb:?} stamp={stamp:?}"
            );
        }
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
        assert!(
            span_clips
                .iter()
                .any(|c| (c[1] - 92.0).abs() < 0.1 && (c[3] - 8.0).abs() < 0.1)
        );
        assert!(
            span_clips
                .iter()
                .any(|c| (c[0] - 188.0).abs() < 0.1 && (c[2] - 12.0).abs() < 0.1)
        );
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
    fn dest_reset_scrolled_out_unmatched_must_not_punch_above_overflow() {
        let mut page = blank_instance(0.0, 0.0, 400.0, 212.0);
        page.color = [0.94, 0.94, 0.95, 1.0];
        let mut title = blank_instance(0.0, 0.0, 400.0, 52.0);
        title.color = [1.0, 1.0, 1.0, 1.0];
        let mut body = blank_instance(0.0, 52.0, 400.0, 160.0);
        body.color = [0.96, 0.96, 0.96, 1.0];
        let scroller = NodePaint {
            key: 10,
            bounds: [0.0, 52.0, 400.0, 160.0],
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 52.0, 400.0, 160.0]),
            layered: false,
            rest: None,
        };
        // Culled spin: prev∪now strip through the title, key not in node_paints.
        let dirty = [-8.0, -156.0, 32.0, 228.0];
        let (wipe, _) = node_present_dest_reset_scaled(
            99,
            dirty,
            &[scroller],
            &[page, title, body],
            &[],
            1.0,
            Some([0.0, 54.0, 16.0, 16.0]),
        )
        .expect("clipped leftover");
        assert!(
            wipe[1] + 0.5 >= 52.0,
            "scrolled-out unmatched dest-reset must stay in the ScrollArea\n wipe={wipe:?}"
        );
        assert!(
            wipe[1] > 24.0 || wipe[1] + wipe[3] <= 24.0,
            "must not punch the title bar\n wipe={wipe:?}"
        );
        let scissor = node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Node,
            99,
            dirty,
            &[(99, [0.0, 54.0, 16.0, 16.0])],
            &[scroller],
            8.0,
        );
        assert!(
            scissor[3] <= 0.5 || scissor[1] + 0.5 >= 52.0,
            "scrolled-out unmatched scissor must stay in the ScrollArea\n scissor={scissor:?}"
        );
        let region = node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Region,
            99,
            dirty,
            &[(99, [0.0, 54.0, 16.0, 16.0])],
            &[scroller],
            0.0,
        );
        assert!(
            region[3] <= 0.5 || region[1] + 0.5 >= 52.0,
            "culled-shimmer Region scissor must stay in the ScrollArea\n scissor={region:?}"
        );
        assert!(
            !rect_contains_point(region, 8.0, 12.0),
            "culled-shimmer Region scissor punches 基础\n scissor={region:?}"
        );
        // Next shimmer tick: last-present already dropped the culled key, dirty
        // sits in the title bar. Must not CLEAR 「基础」 just because prev is gone.
        let title_phantom = [0.0, 12.0, 400.0, 32.0];
        assert!(
            node_present_dest_reset_scaled(
                99,
                title_phantom,
                &[scroller],
                &[page, title, body],
                &[],
                1.0,
                None,
            )
            .is_none(),
            "culled shimmer dest-reset with no last-present must not punch 基础"
        );
        let phantom_scissor = node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Node,
            99,
            title_phantom,
            &[],
            &[scroller],
            8.0,
        );
        assert!(
            phantom_scissor[2] <= 0.5 || phantom_scissor[3] <= 0.5,
            "culled shimmer scissor with no last-present must not punch 基础\n scissor={phantom_scissor:?}"
        );
        assert!(
            !rect_contains_point(phantom_scissor, 200.0, 12.0),
            "culled shimmer scissor with no last-present punches 基础\n scissor={phantom_scissor:?}"
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
            layered: false,
            rest: None,
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
    fn caret_blink_text_leaf_must_not_wipe_flex_content_box() {
        // Blink dirties the I-beam. That dirty is a swapchain overlay — dest-reset
        // never sees it. Last-present of the leaf is the glyph run (typing wipe),
        // not chrome and not the flex-grown content box.
        let leaf = [12.0, 20.0, 300.0, 24.0];
        let ibeam = [40.0, 22.0, 2.0, 18.0];
        let mut ph = sample_text("Search", leaf[0]);
        ph.y = leaf[1];
        ph.width = 80.0;
        ph.height = leaf[3];
        ph.style.input_caret = Some(crate::style::InputCaretDraw::default());
        let p = slot(5, leaf, (0, 1));
        let dirty = PaintDirty {
            key: 5,
            bounds: ibeam,
            kind: PaintDirtyKind::Node,
        };
        assert!(
            paint_dirty_is_caret_overlay(&dirty, &[p], &[]),
            "I-beam dirty on a text leaf is caret overlay"
        );
        let prev = last_present_visual_slots(&[p], &[], &[], &[ph], &[])
            .into_iter()
            .find(|(k, _)| *k == 5)
            .map(|(_, b)| b)
            .expect("caret leaf last-present is the glyph run");
        assert!(
            prev[2] > ibeam[2] * 2.0,
            "last-present must follow glyphs, not the I-beam\n prev={prev:?} ibeam={ibeam:?}"
        );
        assert!(
            prev[2] < leaf[2],
            "last-present must not be the flex-grown leaf\n prev={prev:?} leaf={leaf:?}"
        );
    }

    #[test]
    fn caret_blink_must_not_recapture_placeholder_glyphs() {
        // I-beam is a swapchain overlay after retain blit. Blink must not dest-reset
        // or recapture Search (竖条 / 叠影). Last-present of the leaf is the run.
        let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
        card.color = [0.96, 0.96, 0.97, 1.0];
        let leaf = [12.0, 20.0, 300.0, 24.0];
        let ibeam = [40.0, 22.0, 2.0, 18.0];
        let mut caret = blank_instance(ibeam[0], ibeam[1], ibeam[2], ibeam[3]);
        caret.color = [0.1, 0.1, 0.1, 1.0];
        let mut ph = sample_text("Search documentation", leaf[0]);
        ph.y = leaf[1];
        ph.width = 180.0;
        ph.height = leaf[3];
        ph.style.input_caret = Some(crate::style::InputCaretDraw::default());
        let p_on = NodePaint {
            key: 5,
            bounds: leaf,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 1),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let p_off = NodePaint {
            chrome: (0, 0),
            ..p_on
        };
        let texts = [ph.clone()];
        let dirty = PaintDirty {
            key: 5,
            bounds: ibeam,
            kind: PaintDirtyKind::Node,
        };
        assert!(
            paint_dirty_is_caret_overlay(&dirty, &[p_on], &[card]),
            "blink I-beam dirty must be caret overlay (encode skips dest-reset + recapture)"
        );
        let prev_on = last_present_visual_slots(&[p_on], &[card], &[], &texts, &[caret.clone()])
            .into_iter()
            .find(|(k, _)| *k == 5)
            .map(|(_, b)| b)
            .expect("caret leaf last-present");
        assert!(
            !aabb_covers(prev_on, ibeam) || prev_on[2] > ibeam[2] * 4.0,
            "last-present must be the placeholder run, not the I-beam overlay\n prev={prev_on:?} ibeam={ibeam:?}"
        );
        assert!(
            prev_on[2] < leaf[2],
            "last-present must not be the flex leaf\n prev={prev_on:?} leaf={leaf:?}"
        );
        let prev_off = last_present_visual_slots(&[p_off], &[card], &[], &texts, &[])
            .into_iter()
            .find(|(k, _)| *k == 5)
            .map(|(_, b)| b);
        if let (Some(a), Some(b)) = (Some(prev_on), prev_off) {
            assert!(
                (a[2] - b[2]).abs() < 1.0 && (a[0] - b[0]).abs() < 1.0,
                "ON/OFF last-present of the leaf must match (chrome is overlay)\n on={a:?} off={b:?}"
            );
        }
        let hole = node_present_clipped_capture_text_wipes(&[dirty], &texts, &[p_on], &[]);
        assert!(
            hole.is_empty() || paint_dirty_is_caret_overlay(&dirty, &[p_on], &[card]),
            "overlay blink must not recapture placeholder into retain"
        );
        let _ = caret;
    }

    #[test]
    fn dest_reset_glyph_clip_covers_scissor_ceil_pixel_through_s() {
        // I-beam at a fractional origin: CLEAR floor/ceils to 3 device px,
        // glyphon floor/floors the float wipe to 2. The extra pixel through S
        // stayed white. Capture clip must be that scissor, still hairline.
        let wipe = [40.3, 22.0, 2.0, 18.0];
        let dirty = PaintDirty {
            key: 5,
            bounds: wipe,
            kind: PaintDirtyKind::Node,
        };
        let clip = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.0);
        let scissor = dest_reset_wipe_scissor_logical(wipe, 1.0);
        assert_eq!(scissor, [40.0, 22.0, 3.0, 18.0]);
        assert!(
            clip[0] <= scissor[0]
                && clip[1] <= scissor[1]
                && clip[0] + clip[2] >= scissor[0] + scissor[2]
                && clip[1] + clip[3] >= scissor[1] + scissor[3],
            "glyphon clip must cover the CLEAR scissor\n clip={clip:?} scissor={scissor:?}"
        );
        assert!(
            (clip[2] - scissor[2]).abs() <= NODE_PRESENT_GLYPH_CLIP_PAD * 2.0 + 0.01,
            "coverage kernel is 1px, not vertex-AA through S\n clip={clip:?} scissor={scissor:?}"
        );
        assert!(
            clip[2] < 8.0,
            "ceil pixel is not an AA pad through the rest of S\n clip={clip:?}"
        );

        let clip_hi = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.5);
        let scissor_hi = dest_reset_wipe_scissor_logical(wipe, 1.5);
        assert!(
            clip_hi[0] <= scissor_hi[0] && clip_hi[0] + clip_hi[2] >= scissor_hi[0] + scissor_hi[2],
            "1.5x glyphon clip must cover the CLEAR scissor\n clip={clip_hi:?} scissor={scissor_hi:?}"
        );
        assert!(
            scissor_hi[2] > wipe[2],
            "1.5x must include the ceil device pixel\n scissor={scissor_hi:?} wipe={wipe:?}"
        );
        assert!(
            scissor_hi[2] < wipe[2] + 2.0,
            "must not grow by vertex-AA pad\n scissor={scissor_hi:?}"
        );

        let on_pixel = [40.0, 22.0, 2.0, 18.0];
        assert_eq!(
            dest_reset_wipe_scissor_logical(on_pixel, 1.0),
            on_pixel,
            "integer wipe must stay identity"
        );
    }

    #[test]
    fn dest_reset_placeholder_must_not_src_over_left_of_s() {
        // Focused empty Search uses text-overflow:ellipsis, so clip_bounds is the
        // leaf. I-beam at byte 0 sits on that left edge. Recapture TextBounds must
        // keep a 1px kernel so glyphon's hard clip is not on S's stem (白线), and
        // the painted region (TextBounds ∩ CLEAR scissor) must stay the hole
        // (叠粗).
        let wipe = [40.3, 22.0, 2.0, 18.0];
        let dirty = PaintDirty {
            key: 5,
            bounds: wipe,
            kind: PaintDirtyKind::Node,
        };
        let mut search = sample_text("Search…", 40.3);
        search.y = 22.0;
        search.width = 80.0;
        search.height = 18.0;
        search.clip_bounds = Some([40.3, 22.0, 80.0, 18.0]);
        search.paint_origin = [40.3, 22.0];
        let p = slot(5, [40.3, 22.0, 80.0, 18.0], (0, 1));
        let owner = node_present_glyph_clip_logical(&dirty, Some(wipe), 1.0);
        let hole = node_present_clipped_capture_text_wipes(
            &[dirty],
            &[search.clone()],
            &[p],
            &[(5, owner)],
        );
        let cap = hole
            .iter()
            .find(|n| n.text.contains("Search"))
            .expect("must recapture Search")
            .clip_bounds
            .expect("wipe clip");
        let scissor = dest_reset_wipe_scissor_logical(wipe, 1.0);
        let int_clip = crate::renderer::prepare_text::integerize_clip_rect(cap)
            .expect("positive capture clip");
        assert!(
            int_clip[0] <= scissor[0] - 0.99,
            "ellipsis leaf clip must not eat the coverage kernel left of Search「S」\n cap={cap:?} int={int_clip:?} scissor={scissor:?} owner={owner:?}"
        );
        assert!(
            cap[2] < 8.0,
            "clip must stay the I-beam hole + kernel, not a pad through S\n cap={cap:?}"
        );
        assert!(
            int_clip[0] + int_clip[2] >= scissor[0] + scissor[2] - 0.01,
            "integerized clip must cover the ceil pixel through S\n int={int_clip:?} scissor={scissor:?}"
        );
        let paint = crate::app::damage::intersect_rects(int_clip, scissor).expect("paint");
        assert!(
            paint[0] >= scissor[0] - 0.01 && paint[0] + paint[2] <= scissor[0] + scissor[2] + 0.01,
            "GPU scissor must keep the kernel from painting left of CLEAR (叠粗)\n paint={paint:?} scissor={scissor:?}"
        );
        for scale in [1.25_f32, 1.5, 1.75] {
            let scissor_hi = dest_reset_wipe_scissor_logical(wipe, scale);
            let owner_hi = node_present_glyph_clip_logical(&dirty, Some(wipe), scale);
            let hole_hi = node_present_clipped_capture_text_wipes(
                &[dirty],
                &[search.clone()],
                &[p],
                &[(5, owner_hi)],
            );
            let cap_hi = hole_hi
                .iter()
                .find(|n| n.text.contains("Search"))
                .expect("must recapture Search at hi-dpi")
                .clip_bounds
                .expect("wipe clip");
            let scaled = [
                cap_hi[0] * scale,
                cap_hi[1] * scale,
                cap_hi[2] * scale,
                cap_hi[3] * scale,
            ];
            let int_hi = crate::renderer::prepare_text::integerize_clip_rect(scaled)
                .expect("positive hi-dpi clip");
            let scissor_px = [
                scissor_hi[0] * scale,
                scissor_hi[1] * scale,
                scissor_hi[2] * scale,
                scissor_hi[3] * scale,
            ];
            assert!(
                int_hi[0] + int_hi[2] >= scissor_px[0] + scissor_px[2] - 0.01
                    && int_hi[0] <= scissor_px[0] + 0.01,
                "{scale}x integerize must cover CLEAR through S\n int={int_hi:?} scissor={scissor_px:?}"
            );
        }
    }

    #[test]
    fn transparent_item_must_not_skip_title_dest_reset() {
        // Gallery Item outline→default: both the row and the bind_text title are
        // dirty. Geometry nesting used to skip the title wipe; the Item has no
        // fill, so "outline" glyphs stayed in retain under "default".
        let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
        card.color = [0.96, 0.96, 0.97, 1.0];
        card.radii = [8.0; 4];
        let item_box = [8.0, 8.0, 384.0, 56.0];
        let title_box = [48.0, 16.0, 200.0, 20.0];
        let item_p = slot(2, item_box, (0, 0));
        let title_p = slot(3, title_box, (0, 1));
        let item_dirty = PaintDirty {
            key: 2,
            bounds: item_box,
            kind: PaintDirtyKind::Node,
        };
        let title_dirty = PaintDirty {
            key: 3,
            bounds: title_box,
            kind: PaintDirtyKind::Node,
        };
        let dirties = [item_dirty, title_dirty];
        let paints = [item_p, title_p];
        let instances = [card];
        assert!(
            crate::app::damage::paint_dirty_nested_in_larger_node(&title_dirty, &dirties),
            "precondition: title center sits in the Item dirty"
        );
        assert!(
            crate::app::damage::paint_dirty_skips_node_dest_reset(&title_dirty, &dirties, false),
            "precondition: geometry-only skip would fire"
        );
        assert!(
            !paint_dirty_skips_node_dest_reset_covered(
                &title_dirty,
                &dirties,
                false,
                &paints,
                &instances,
                1.0,
            ),
            "transparent Item must not skip the title dest-reset"
        );
        assert!(
            node_present_dest_reset_full(
                3,
                title_box,
                &paints,
                &instances,
                &[],
                &[],
                1.0,
                Some(title_box),
            )
            .is_some(),
            "title dest-reset must wipe leftover outline glyphs using the card behind"
        );
    }

    #[test]
    fn transparent_item_with_icon_must_not_skip_title_dest_reset() {
        // Gallery Item has ItemMedia (gear). Collect range includes that opaque
        // descendant; skip must look at Item's own chrome only.
        let mut card = blank_instance(0.0, 0.0, 400.0, 80.0);
        card.color = [0.96, 0.96, 0.97, 1.0];
        let mut item_fill = blank_instance(8.0, 8.0, 384.0, 56.0);
        item_fill.color = [0.0, 0.0, 0.0, 0.0];
        let mut icon = blank_instance(16.0, 20.0, 24.0, 24.0);
        icon.color = [0.4, 0.4, 0.45, 1.0];
        let item_box = [8.0, 8.0, 384.0, 56.0];
        let title_box = [48.0, 16.0, 200.0, 20.0];
        let mut item_p = slot(2, item_box, (0, 0));
        item_p.instance = (1, 3);
        let title_p = slot(3, title_box, (0, 1));
        let item_dirty = PaintDirty {
            key: 2,
            bounds: item_box,
            kind: PaintDirtyKind::Node,
        };
        let title_dirty = PaintDirty {
            key: 3,
            bounds: title_box,
            kind: PaintDirtyKind::Node,
        };
        let dirties = [item_dirty, title_dirty];
        assert!(
            node_dirty_has_opaque_fill(2, item_box, &[item_p, title_p], &[card, item_fill, icon]),
            "precondition: descendant icon would look like Item fill"
        );
        assert!(
            !paint_dirty_skips_node_dest_reset_covered(
                &title_dirty,
                &dirties,
                false,
                &[item_p, title_p],
                &[card, item_fill, icon],
                1.0,
            ),
            "opaque ItemMedia must not make default Item skip the title dest-reset"
        );
    }

    #[test]
    fn opaque_shell_still_skips_nested_input_dest_reset() {
        // Kbd Input inside InputAdornment: the shell redraws opaque fill, so the
        // nested field still skips its own wipe (avoids punching gap-2 neighbors).
        let mut fill = blank_instance(10.0, 10.0, 320.0, 40.0);
        fill.color = [1.0, 1.0, 1.0, 1.0];
        let mut shell_p = slot(1, [10.0, 10.0, 320.0, 40.0], (0, 0));
        shell_p.instance = (0, 1);
        let input_p = slot(2, [12.0, 12.0, 260.0, 36.0], (0, 0));
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
        let dirties = [shell, input];
        assert!(
            paint_dirty_skips_node_dest_reset_covered(
                &input,
                &dirties,
                false,
                &[shell_p, input_p],
                &[fill],
                1.0,
            ),
            "opaque InputAdornment must still nested-skip the field dest-reset"
        );
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
    fn modal_scrim_occludes_clicked_trigger_glyph_capture() {
        // AlertDialog: dest-reset skip still left node-present glyphon to flush
        // "one" onto the retained scrim. Occupancy must drop that dirty first.
        let trigger = [24.0, 80.0, 96.0, 32.0];
        let scrim = [0.0, 0.0, 1280.0, 800.0];
        let dirty = PaintDirty {
            key: 7,
            bounds: trigger,
            kind: PaintDirtyKind::Node,
        };
        let mut caption = sample_text("one", 28.0);
        caption.y = 86.0;
        caption.paint_origin = [28.0, 86.0];
        let paints = [slot(7, trigger, (0, 1))];
        let page = [caption];
        let captured = node_present_clipped_capture_text(&[dirty], &page, &paints);
        assert!(
            captured.iter().any(|n| n.text == "one"),
            "without occupancy the clicked caption is in the capture buffer"
        );
        let dirties: Vec<PaintDirty> = [dirty]
            .into_iter()
            .filter(|d| {
                !crate::app::damage::page_node_present_occluded_by_overlay(d.bounds, Some(scrim))
            })
            .collect();
        let hidden = node_present_clipped_capture_text(&dirties, &page, &paints);
        assert!(
            hidden.iter().all(|n| n.text != "one"),
            "fullscreen scrim occupancy must drop the trigger from glyphon capture"
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
    fn dest_reset_with_last_present_must_not_pad_beyond_real_ink() {
        // Caret blink dest-resets a focused Input with last-present. RESET_PAD on
        // top of ring ink (spread 3 + vertex 2) punched `form_field` gap(4) labels.
        let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let box_ = [8.0, 8.0, 220.0, 36.0];
        let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
        focused.border_color = [0.45, 0.35, 0.95, 1.0];
        focused.shadow_blur = 0.0;
        focused.shadow_spread = 3.0;
        focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let p = NodePaint {
            key: 9,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let ring = instance_paint_bounds(&focused);
        let prev = node_paint_visual_bounds(&p, &[card, focused], &[], &[], &[]);
        let expected = super::union_xywh(ring, prev);
        let (wipe, _) =
            node_present_dest_reset_scaled(9, box_, &[p], &[card, focused], &[], 1.0, Some(prev))
                .expect("card behind focused input");
        assert!(
            aabb_covers(wipe, ring),
            "wipe must still cover real ring ink\n wipe={wipe:?} ring={ring:?}"
        );
        assert!(
            (wipe[0] - expected[0]).abs() <= 0.5
                && (wipe[1] - expected[1]).abs() <= 0.5
                && (wipe[2] - expected[2]).abs() <= 0.5
                && (wipe[3] - expected[3]).abs() <= 0.5,
            "steady-state dest-reset must be the ink ∪ last-present box, not +RESET_PAD\n wipe={wipe:?} expected={expected:?}"
        );
    }

    #[test]
    fn dest_reset_focus_ring_must_not_wipe_sibling_label() {
        // WP-3.9: Hard ring spread 3 sits in form_field gap(4). Vertex AA / dest-reset
        // pad must not punch the label slot above the Input.
        let mut card = blank_instance(0.0, 0.0, 240.0, 80.0);
        card.color = [0.94, 0.94, 0.95, 1.0];
        let label = [8.0, 0.0, 40.0, 12.0];
        let box_ = [8.0, 16.0, 220.0, 36.0];
        let mut focused = input_like(box_[0], box_[1], box_[2], box_[3]);
        focused.border_color = [0.45, 0.35, 0.95, 1.0];
        focused.shadow_blur = 0.0;
        focused.shadow_spread = 3.0;
        focused.shadow_color = [0.45, 0.35, 0.95, 0.5];
        let label_p = NodePaint {
            key: 1,
            bounds: label,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let input_p = NodePaint {
            key: 5,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let paints = [label_p, input_p];
        let visible_ring = super::padded_xywh(box_, crate::style::FOCUS_RING_HARD_SPREAD);
        let prev = node_paint_visual_bounds(&input_p, &[card, focused], &[], &[], &[]);
        let (wipe, _) = node_present_dest_reset_scaled(
            5,
            box_,
            &paints,
            &[card, focused],
            &[],
            1.0,
            Some(prev),
        )
        .expect("card behind focused input");
        assert!(
            aabb_covers(wipe, visible_ring),
            "wipe must still cover the 3px Hard ring in the gap\n wipe={wipe:?} ring={visible_ring:?}"
        );
        assert!(
            !super::point_in_rect(wipe, [label[0] + 8.0, label[1] + label[3] - 0.5]),
            "focus dest-reset must not punch the label slot\n wipe={wipe:?} label={label:?}"
        );
        let scissor = super::node_present_scissor_logical(
            PaintDirtyKind::Node,
            5,
            [box_[0] - 3.0, box_[1] - 3.0, box_[2] + 6.0, box_[3] + 6.0],
            &[(5, prev)],
            &paints,
            8.0,
        );
        assert!(
            !super::point_in_rect(scissor, [label[0] + 8.0, label[1] + label[3] - 0.5]),
            "node scissor must not cover the label after sibling clip\n scissor={scissor:?} label={label:?}"
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
            layered: false,
            rest: None,
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
            node_present_needs_glyphs_scaled(
                8,
                box_,
                &[p],
                &[card, unfocused],
                &[],
                &[],
                s,
                Some(prev),
            ),
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
    fn ancestor_clip_caps_shadow_wipe_and_last_present() {
        let mut fill = blank_instance(120.0, 102.0, 80.0, 30.0);
        fill.color = [1.0; 4];
        fill.shadow_color = [0.0, 0.0, 0.0, 0.25];
        fill.shadow_blur = 8.0;
        let mut p = slot(7, [120.0, 102.0, 80.0, 30.0], (0, 0));
        p.instance = (0, 1);
        p.overflow_clip = Some([0.0, 100.0, 400.0, 300.0]);
        let prev = node_paint_visual_bounds(&p, &[fill], &[], &[], &[]);
        assert!(
            prev[1] >= 100.0,
            "shadow must not invent pixels above viewport"
        );
        let wipe = node_present_reset_rect(
            7,
            p.bounds,
            &[p],
            &[fill],
            &[],
            &[],
            NODE_PRESENT_RESET_PAD_PHYSICAL,
            1.0,
            Some(prev),
        )
        .unwrap();
        assert!(wipe[1] >= 100.0, "shadow expansion must not erase header");
        for clip in [[0.0; 4], [0.0, 250.0, 400.0, 100.0]] {
            p.overflow_clip = Some(clip);
            let hidden = node_paint_visual_bounds(&p, &[fill], &[], &[], &[]);
            assert_eq!(
                hidden, [0.0; 4],
                "culled ink must not become last-present damage"
            );
        }
    }

    #[test]
    fn dest_reset_overflow_hidden_must_cover_own_outset_shadow() {
        // Gallery SidebarInset: overflow:hidden + Elevation::Md (blur 8, offset y 2).
        // Own chrome paints the halo outside overflow (Dialog/Drawer rule). Dest-reset
        // / last-present must still wipe that halo or PartialLoad src-overs it darker
        // every CSS tick.
        let box_ = [40.0, 40.0, 200.0, 120.0];
        let mut page = blank_instance(0.0, 0.0, 400.0, 300.0);
        page.color = [0.12, 0.12, 0.14, 1.0];
        let mut fill = blank_instance(box_[0], box_[1], box_[2], box_[3]);
        fill.color = [0.98, 0.98, 0.99, 1.0];
        fill.radii = [12.0; 4];
        fill.shadow_color = [0.0, 0.0, 0.0, 0.25];
        fill.shadow_offset = [0.0, 2.0];
        fill.shadow_blur = 8.0;
        let p = NodePaint {
            key: 7,
            bounds: box_,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            // collect records the parent clip, not this node's own overflow.
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let blob = instance_paint_bounds(&fill);
        assert!(
            blob[0] < box_[0] - 8.0 && blob[1] < box_[1] - 8.0,
            "precondition: outset halo extends past the overflow box, got blob={blob:?} box={box_:?}"
        );
        let prev = node_paint_visual_bounds(&p, &[page, fill], &[], &[], &[]);
        let (wipe, _) =
            node_present_dest_reset_scaled(7, box_, &[p], &[page, fill], &[], 1.0, Some(prev))
                .expect("page fill behind inset");
        assert!(
            aabb_covers(wipe, blob),
            "overflow:hidden dest-reset must wipe own outset shadow\n wipe={wipe:?} blob={blob:?} prev={prev:?} overflow={:?}",
            p.overflow_clip
        );
        let scissor = node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Node,
            7,
            box_,
            &[(7, prev)],
            &[p],
            8.0,
        );
        let painted = intersect_rects(blob, scissor).expect("halo sits in the node scissor");
        assert!(
            aabb_covers(wipe, painted),
            "GPU scissor paints halo that dest-reset missed — PartialLoad stacks it\n wipe={wipe:?} painted={painted:?} scissor={scissor:?} blob={blob:?}"
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
            layered: false,
            rest: None,
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
    fn last_present_must_not_union_foreign_descendant_scale() {
        // Reaction bar collect range includes the chip Bake. Hover:scale grows that
        // quad; last-present of the pill must stay on own chrome or expand_paint
        // dest-resets the capsule as a sharp rect over outline / sibling chips.
        let bar = [10.0, 10.0, 48.0, 28.0];
        let chip = [32.0, 16.0, 16.0, 16.0];
        let leaf = [32.0, 18.0, 16.0, 12.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 80.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        fill.color = [0.82, 0.82, 0.84, 1.0];
        fill.radii = [14.0; 4];
        let rest_bake = blank_textured(30.0, 14.0, 20.0, 20.0, MAT2D_IDENTITY);
        let hover_bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
        let bar_p = NodePaint {
            key: 1,
            bounds: bar,
            overlay: false,
            instance: (1, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let chip_p = NodePaint {
            key: 2,
            bounds: chip,
            overlay: false,
            instance: (2, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let leaf_p = NodePaint {
            key: 3,
            bounds: leaf,
            overlay: false,
            instance: (2, 2),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let paints = [bar_p, chip_p, leaf_p];
        let instances = [page, fill];
        let rest_slots = last_present_visual_slots(&paints, &instances, &[rest_bake], &[], &[]);
        let hover_slots = last_present_visual_slots(&paints, &instances, &[hover_bake], &[], &[]);
        let rest_bar = rest_slots
            .iter()
            .find(|(k, _)| *k == 1)
            .map(|(_, b)| *b)
            .expect("bar rest");
        let hover_bar = hover_slots
            .iter()
            .find(|(k, _)| *k == 1)
            .map(|(_, b)| *b)
            .expect("bar hover");
        assert!(
            !crate::app::damage::node_present_slot_moved(rest_bar, hover_bar),
            "parent last-present must not grow with child hover:scale Bake\n rest={rest_bar:?} hover={hover_bar:?}"
        );
        let rest_chip = rest_slots
            .iter()
            .find(|(k, _)| *k == 2)
            .map(|(_, b)| *b)
            .expect("chip rest");
        let hover_chip = hover_slots
            .iter()
            .find(|(k, _)| *k == 2)
            .map(|(_, b)| *b)
            .expect("chip hover");
        assert!(
            crate::app::damage::node_present_slot_moved(rest_chip, hover_chip),
            "chip last-present must keep the leaf Bake so shrink can dest-reset it\n rest={rest_chip:?} hover={hover_chip:?}"
        );
        let mut dirties = vec![crate::app::damage::PaintDirty {
            key: 2,
            bounds: hover_chip,
            kind: crate::app::damage::PaintDirtyKind::Node,
        }];
        crate::app::damage::expand_paint_dirties_for_translated_slots(
            &mut dirties,
            &hover_slots,
            &rest_slots,
        );
        assert!(
            dirties
                .iter()
                .all(|d| d.kind != crate::app::damage::PaintDirtyKind::Node || d.key != 1),
            "hover:scale must not dest-reset the parent pill\n dirties={dirties:?}"
        );
        assert!(
            !crate::app::damage::unnamed_translated_page_slots(&hover_slots, &rest_slots, &dirties,),
            "parent visual growth used to FullClear the page as an unnamed translate"
        );
    }

    #[test]
    fn last_present_must_not_union_overflow_visible_spin_sticking_out() {
        // Gallery Marker row (icon + "Thinking...") is taller-parent by width, not
        // equal-span. 45° spin `NodePaint.bounds` is the circumcircle and sticks
        // out of the row's 16px box — `aabb_contains(row, icon)` is false.
        let row = [0.0, 8.0, 104.0, 16.0];
        let icon = [-3.3, 4.7, 22.6, 22.6];
        let content = [24.0, 8.0, 80.0, 16.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 48.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut row_fill = blank_instance(row[0], row[1], row[2], row[3]);
        row_fill.color = [0.0, 0.0, 0.0, 0.0];
        let mut content_fill = blank_instance(content[0], content[1], content[2], content[3]);
        content_fill.color = [0.85, 0.85, 0.85, 1.0];
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let spin = blank_textured(0.0, 8.0, 16.0, 16.0, [c, s, -s, c, 0.0, 0.0]);
        let rest_spin = blank_textured(0.0, 8.0, 16.0, 16.0, MAT2D_IDENTITY);
        let row_p = NodePaint {
            key: 1,
            bounds: row,
            overlay: false,
            instance: (1, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let icon_p = NodePaint {
            key: 2,
            bounds: icon,
            overlay: false,
            instance: (3, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let content_p = NodePaint {
            key: 3,
            bounds: content,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let paints = [row_p, icon_p, content_p];
        let instances = [page, row_fill, content_fill];
        let rest_slots = last_present_visual_slots(&paints, &instances, &[rest_spin], &[], &[]);
        let spin_slots = last_present_visual_slots(&paints, &instances, &[spin], &[], &[]);
        let rest_row = rest_slots
            .iter()
            .find(|(k, _)| *k == 1)
            .map(|(_, b)| *b)
            .expect("row rest");
        let spin_row = spin_slots
            .iter()
            .find(|(k, _)| *k == 1)
            .map(|(_, b)| *b)
            .expect("row spin");
        assert!(
            !crate::app::damage::node_present_slot_moved(rest_row, spin_row),
            "Marker row last-present must not grow with child spin circumcircle\n rest={rest_row:?} spin={spin_row:?}"
        );
        let mut dirties = vec![crate::app::damage::PaintDirty {
            key: 2,
            bounds: icon,
            kind: crate::app::damage::PaintDirtyKind::Node,
        }];
        crate::app::damage::expand_paint_dirties_for_translated_slots(
            &mut dirties,
            &spin_slots,
            &rest_slots,
        );
        assert!(
            dirties
                .iter()
                .all(|d| d.kind != crate::app::damage::PaintDirtyKind::Node || d.key != 1),
            "idle spin must not dirty the Marker row"
        );
        assert!(
            !crate::app::damage::paint_dirty_nested_in_larger_node(&dirties[0], &dirties),
            "icon dest-reset must run; nested-skip under the row leaves 45° corners in retain"
        );
    }

    #[test]
    fn dest_reset_spin_must_not_be_cut_by_overflow_visible_marker_row() {
        // Gallery idle loader-circle: dest-reset of the icon is the circumcircle.
        // `clip_rect_against_foreign_siblings` treated the Marker row (wider, same
        // 16px height) as a foreign sibling because `aabb_contains(row, circumcircle)`
        // is false. Shrink then kept only the leftover strips *outside* the row —
        // the spinner body src-overs forever and the gap seals.
        let row = [0.0, 8.0, 104.0, 16.0];
        let icon = [-3.3, 4.7, 22.6, 22.6];
        let content = [24.0, 8.0, 80.0, 16.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 48.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut row_fill = blank_instance(row[0], row[1], row[2], row[3]);
        row_fill.color = [0.0, 0.0, 0.0, 0.0];
        let mut content_fill = blank_instance(content[0], content[1], content[2], content[3]);
        content_fill.color = [0.85, 0.85, 0.85, 1.0];
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let xf = [c, s, -s, c, 0.0, 0.0];
        let spin = blank_textured(0.0, 8.0, 16.0, 16.0, xf);
        let row_p = NodePaint {
            key: 1,
            bounds: row,
            overlay: false,
            instance: (1, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let icon_p = NodePaint {
            key: 2,
            bounds: icon,
            overlay: false,
            instance: (3, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let content_p = NodePaint {
            key: 3,
            bounds: content,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let paints = [row_p, icon_p, content_p];
        let instances = [page, row_fill, content_fill];
        let prev = [0.0, 8.0, 16.0, 16.0];
        let (wipe, _) =
            node_present_dest_reset_scaled(2, icon, &paints, &instances, &[spin], 1.0, Some(prev))
                .expect("card behind spinner");
        let spun = crate::renderer::aabb_of_centered_transform([0.0, 8.0, 16.0, 16.0], &xf);
        assert!(
            wipe[0] <= spun[0] + 0.5
                && wipe[1] <= spun[1] + 0.5
                && wipe[0] + wipe[2] >= spun[0] + spun[2] - 0.5
                && wipe[1] + wipe[3] >= spun[1] + spun[3] - 0.5,
            "idle dest-reset must wipe the loader-circle body, not leftover strips outside the Marker row\n wipe={wipe:?} spun={spun:?} row={row:?}"
        );
        assert!(
            wipe[2] > 18.0 && wipe[3] > 18.0,
            "wipe must stay the circumcircle, not a 3px leftover band\n wipe={wipe:?}"
        );
    }

    #[test]
    fn node_present_scissor_must_not_crop_overlay_reaction_hover_scale_top() {
        // Overlay-layout pill nips bubble content. Content is an earlier collect
        // slot; shrinking occupancy against it set scissor.top = content.bottom
        // and AABB-cropped the 😮 top during hover:scale (intact after settle).
        let content = [0.0, 0.0, 166.0, 40.0];
        let chip = [135.66, 37.66, 16.68, 16.68];
        let dest_ink = [137.58, 38.93, 13.18, 13.10];
        let dirty = [130.30, 33.76, 27.40, 24.58];
        let bar_clip = [110.0, 32.0, 48.0, 28.0];
        let content_p = NodePaint {
            key: 1,
            bounds: content,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let chip_p = NodePaint {
            key: 2,
            bounds: chip,
            overlay: false,
            instance: (0, 0),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(bar_clip),
            layered: false,
            rest: None,
        };
        let paints = [content_p, chip_p];
        let sc = node_present_scissor_logical(
            crate::app::damage::PaintDirtyKind::Node,
            2,
            dirty,
            &[],
            &paints,
            node_present_scissor_pad(crate::app::damage::PaintDirtyKind::Node),
        );
        assert!(
            sc[1] <= dest_ink[1] + 0.5,
            "occupancy must not crop overlay hover:scale dest at content.bottom; \
             scissor={sc:?} dest_ink={dest_ink:?} content={content:?}"
        );
    }

    #[test]
    fn dest_reset_overlay_reaction_leave_must_wipe_hover_scale_top() {
        // Occupancy draws the enlarged dest over content. Leave dest-reset still
        // sibling-clipped to content.bottom, so the overlap strip stays
        // (live: 收起残影 at the top of 😮).
        let content = [0.0, 0.0, 166.0, 40.0];
        let chip = [135.66, 37.66, 16.68, 16.68];
        let prev = [130.30, 33.76, 27.40, 24.58];
        let dirty = [132.0, 36.0, 20.0, 20.0];
        let bar_clip = [110.0, 32.0, 48.0, 28.0];
        let content_p = NodePaint {
            key: 1,
            bounds: content,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let chip_p = NodePaint {
            key: 2,
            bounds: chip,
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(bar_clip),
            layered: false,
            rest: None,
        };
        let paints = [content_p, chip_p];
        let wipe =
            super::node_present_reset_rect(2, dirty, &paints, &[], &[], &[], 0.0, 1.0, Some(prev))
                .expect("leave dest-reset");
        assert!(
            wipe[1] <= prev[1] + 0.5,
            "leave dest-reset must wipe the enlarged top over content (ghost); \
             wipe={wipe:?} prev={prev:?} content={content:?}"
        );
    }

    fn paint_slot(
        key: u64,
        bounds: [f32; 4],
        rest: Option<[f32; 4]>,
        instance: (usize, usize),
    ) -> NodePaint {
        NodePaint {
            key,
            bounds,
            overlay: false,
            instance,
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest,
        }
    }

    #[test]
    fn dest_reset_later_avatar_must_not_punch_earlier_disk_crescent() {
        // 32px circles, -8px overlap. Earlier-sibling occupancy used to skip the
        // clip (hover:scale leave). AABB dest-reset of B then CLEARed A's crescent.
        let a = [0.0, 0.0, 32.0, 32.0];
        let b = [24.0, 0.0, 32.0, 32.0];
        let c = [48.0, 0.0, 32.0, 32.0];
        let mut page = blank_instance(0.0, 0.0, 320.0, 80.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fa = blank_instance(a[0], a[1], a[2], a[3]);
        fa.color = [0.9, 0.2, 0.2, 1.0];
        fa.radii = [16.0; 4];
        let mut fb = blank_instance(b[0], b[1], b[2], b[3]);
        fb.color = [0.2, 0.7, 0.3, 1.0];
        fb.radii = [16.0; 4];
        let mut fc = blank_instance(c[0], c[1], c[2], c[3]);
        fc.color = [0.95, 0.95, 0.95, 1.0];
        fc.radii = [16.0; 4];
        let paints = [
            paint_slot(1, a, Some(a), (1, 2)),
            paint_slot(2, b, Some(b), (2, 3)),
            paint_slot(3, c, Some(c), (3, 4)),
            paint_slot(
                0,
                [0.0, 0.0, 320.0, 80.0],
                Some([0.0, 0.0, 320.0, 80.0]),
                (0, 1),
            ),
        ];
        let instances = [page, fa, fb, fc];
        let (wipe_b, _) = node_present_dest_reset_scaled(2, b, &paints, &instances, &[], 1.0, None)
            .expect("card behind B");
        let crescent = [22.0, 16.0];
        let in_a_disk = {
            let dx = crescent[0] - 16.0;
            let dy = crescent[1] - 16.0;
            dx * dx + dy * dy <= 16.0 * 16.0
        };
        let in_b_disk = {
            let dx = crescent[0] - 40.0;
            let dy = crescent[1] - 16.0;
            dx * dx + dy * dy <= 16.0 * 16.0
        };
        assert!(
            in_a_disk && !in_b_disk,
            "precondition: (22,16) is A's crescent vs B"
        );
        assert!(
            crescent[0] < wipe_b[0]
                || crescent[0] >= wipe_b[0] + wipe_b[2]
                || crescent[1] < wipe_b[1]
                || crescent[1] >= wipe_b[1] + wipe_b[3],
            "B dest-reset must not AABB-clear A's crescent, wipe={wipe_b:?}"
        );
        let (wipe_c, _) = node_present_dest_reset_scaled(3, c, &paints, &instances, &[], 1.0, None)
            .expect("card behind C");
        let crescent_bc = [46.0, 16.0];
        assert!(
            crescent_bc[0] < wipe_c[0]
                || crescent_bc[0] >= wipe_c[0] + wipe_c[2]
                || crescent_bc[1] < wipe_c[1]
                || crescent_bc[1] >= wipe_c[1] + wipe_c[3],
            "C dest-reset must not AABB-clear B's crescent, wipe={wipe_c:?}"
        );
    }

    #[test]
    fn dest_reset_layered_ping_must_not_paint_or_wipe_caption() {
        // Gallery ping is `position:absolute` (layered). Scale-2 visual overlaps
        // the caption; dest-reset and scissor must clip that rest-disjoint sibling
        // so the ring cannot src-over the label.
        let label = [8.0, 0.0, 40.0, 14.0];
        let rest = [16.0, 22.0, 32.0, 32.0];
        let visual = [0.0, 6.0, 64.0, 64.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(visual[0], visual[1], visual[2], visual[3]);
        fill.color = [0.2, 0.2, 0.85, 1.0];
        fill.radii = [32.0; 4];
        let mut ping_slot = paint_slot(2, visual, Some(rest), (1, 2));
        ping_slot.layered = true;
        let paints = [paint_slot(1, label, Some(label), (0, 0)), ping_slot];
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            visual,
            &paints,
            &[page, fill],
            &[],
            1.0,
            Some(visual),
        )
        .expect("card behind ping");
        assert!(
            super::intersect_rects(wipe, label).is_none(),
            "layered ping dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
        );
        assert!(
            wipe[2] > 20.0 && wipe[3] > 20.0,
            "wipe must still cover the ping body, not collapse\n wipe={wipe:?}"
        );
        let sc = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            visual,
            &[(2, visual)],
            &paints,
            node_present_scissor_pad(PaintDirtyKind::Node),
        );
        assert!(
            super::intersect_rects(sc, label).is_none(),
            "layered ping scissor must not paint the caption\n scissor={sc:?} label={label:?}"
        );
    }

    #[test]
    fn dest_reset_play_state_spin_must_cover_circumcircle_against_caption_and_button() {
        // Gallery play-state stages the 2.5rem spinner in 5rem so the 45°
        // circumcircle (+ 8px scissor pad) stays off the caption/button.
        let label = [8.0, 0.0, 72.0, 14.0];
        let rest = [20.0, 40.0, 40.0, 40.0];
        let button = [16.0, 106.0, 48.0, 28.0];
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let xf = [c, s, -s, c, 0.0, 0.0];
        let visual = crate::renderer::aabb_of_centered_transform(rest, &xf);
        let mut page = blank_instance(0.0, 0.0, 200.0, 150.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(rest[0], rest[1], rest[2], rest[3]);
        fill.color = [0.1, 0.1, 0.12, 1.0];
        fill.radii = [6.0; 4];
        fill.transform = xf;
        let mut btn_fill = blank_instance(button[0], button[1], button[2], button[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 1.0];
        btn_fill.radii = [6.0; 4];
        let spin_slot = paint_slot(2, visual, Some(rest), (1, 2));
        let paints = [
            paint_slot(1, label, Some(label), (0, 0)),
            spin_slot,
            paint_slot(3, button, Some(button), (2, 3)),
        ];
        let instances = [page, fill, btn_fill];
        let (wipe, _) =
            node_present_dest_reset_scaled(2, visual, &paints, &instances, &[], 1.0, Some(visual))
                .expect("card behind play-state spin");
        assert!(
            aabb_covers(wipe, visual),
            "pause dest-reset must wipe the 45° circumcircle, not leftover frames\n wipe={wipe:?} visual={visual:?} rest={rest:?} label={label:?} button={button:?}"
        );
        assert!(
            super::intersect_rects(wipe, label).is_none(),
            "spin dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
        );
        let sc = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            visual,
            &[(2, visual)],
            &paints,
            node_present_scissor_pad(PaintDirtyKind::Node),
        );
        assert!(
            aabb_covers(sc, visual),
            "pause scissor must cover the 45° circumcircle\n scissor={sc:?} visual={visual:?}"
        );
        assert!(
            super::intersect_rects(sc, label).is_none(),
            "spin scissor must not dest-reset the caption\n scissor={sc:?} label={label:?}"
        );
        let encode = super::union_xywh(sc, wipe);
        assert!(
            super::intersect_rects(encode, label).is_none(),
            "GPU scissor ∪ wipe must not dest-reset the caption\n encode={encode:?} label={label:?}"
        );
    }

    #[test]
    fn dest_reset_play_state_spin_must_not_punch_caption_when_circumcircle_nips_it() {
        // Tight column (no stage): 45° tips nip the title. Occupancy clip must
        // still keep dest-reset/scissor off the caption (ghost 「play-state」).
        let label = [8.0, 0.0, 72.0, 14.0];
        let rest = [16.0, 20.0, 40.0, 40.0];
        let button = [12.0, 66.0, 48.0, 28.0];
        let deg45 = std::f32::consts::FRAC_PI_4;
        let (s, c) = deg45.sin_cos();
        let xf = [c, s, -s, c, 0.0, 0.0];
        let visual = crate::renderer::aabb_of_centered_transform(rest, &xf);
        let mut page = blank_instance(0.0, 0.0, 200.0, 120.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(rest[0], rest[1], rest[2], rest[3]);
        fill.color = [0.1, 0.1, 0.12, 1.0];
        fill.radii = [6.0; 4];
        fill.transform = xf;
        let mut btn_fill = blank_instance(button[0], button[1], button[2], button[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 1.0];
        btn_fill.radii = [6.0; 4];
        let spin_slot = paint_slot(2, visual, Some(rest), (1, 2));
        let paints = [
            paint_slot(1, label, Some(label), (0, 0)),
            spin_slot,
            paint_slot(3, button, Some(button), (2, 3)),
        ];
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            visual,
            &paints,
            &[page, fill, btn_fill],
            &[],
            1.0,
            Some(visual),
        )
        .expect("card behind play-state spin");
        assert!(
            super::intersect_rects(wipe, label).is_none(),
            "tight-column spin dest-reset must not punch the caption\n wipe={wipe:?} label={label:?} visual={visual:?}"
        );
        assert!(
            aabb_covers(wipe, rest),
            "wipe must still cover the unrotated box\n wipe={wipe:?} rest={rest:?}"
        );
        let sc = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            visual,
            &[(2, visual)],
            &paints,
            node_present_scissor_pad(PaintDirtyKind::Node),
        );
        assert!(
            super::intersect_rects(sc, label).is_none(),
            "tight-column spin scissor must not dest-reset the caption\n scissor={sc:?} label={label:?}"
        );
    }

    #[test]
    fn dest_reset_bounce_translate_must_not_punch_caption_above() {
        let label = [8.0, 0.0, 48.0, 14.0];
        let rest = [16.0, 22.0, 32.0, 32.0];
        let visual = [16.0, -3.0, 32.0, 32.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(visual[0], visual[1], visual[2], visual[3]);
        fill.color = [0.92, 0.92, 0.94, 1.0];
        fill.radii = [6.0; 4];
        let paints = [
            paint_slot(1, label, Some(label), (0, 0)),
            paint_slot(2, visual, Some(rest), (1, 2)),
        ];
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            visual,
            &paints,
            &[page, fill],
            &[],
            1.0,
            Some(visual),
        )
        .expect("card behind bounce");
        let lx = label[0] + label[2] * 0.5;
        let ly = label[1] + label[3] * 0.5;
        assert!(
            !super::point_in_rect(wipe, [lx, ly]),
            "bounce dest-reset must not punch the caption\n wipe={wipe:?} label={label:?}"
        );
    }

    #[test]
    fn dest_reset_pulse_must_not_crop_to_grown_ping_neighbor() {
        // HStack gap 20px: ping scale-2 visual nips the padded pulse scissor but
        // rest boxes stay disjoint. Clipping occupancy against the grown visual
        // left nested dest-reset frames on alternate-pulse.
        let ping_rest = [0.0, 20.0, 32.0, 32.0];
        let ping_visual = [-16.0, 4.0, 64.0, 64.0];
        let pulse = [52.0, 20.0, 48.0, 32.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut ping_fill = blank_instance(
            ping_visual[0],
            ping_visual[1],
            ping_visual[2],
            ping_visual[3],
        );
        ping_fill.color = [0.2, 0.2, 0.85, 0.4];
        ping_fill.radii = [32.0; 4];
        let mut pulse_fill = blank_instance(pulse[0], pulse[1], pulse[2], pulse[3]);
        pulse_fill.color = [0.55, 0.35, 0.85, 0.5];
        pulse_fill.radii = [6.0; 4];
        let paints = [
            paint_slot(1, ping_visual, Some(ping_rest), (1, 2)),
            paint_slot(2, pulse, Some(pulse), (2, 3)),
        ];
        let instances = [page, ping_fill, pulse_fill];
        let (wipe, _) =
            node_present_dest_reset_scaled(2, pulse, &paints, &instances, &[], 1.0, Some(pulse))
                .expect("card behind pulse");
        assert!(
            aabb_covers(wipe, pulse),
            "pulse dest-reset must wipe the full rounded box, not a leftover inner frame\n wipe={wipe:?} pulse={pulse:?}"
        );
        let sc = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            pulse,
            &[(2, pulse)],
            &paints,
            node_present_scissor_pad(PaintDirtyKind::Node),
        );
        assert!(
            aabb_covers(sc, pulse),
            "pulse scissor must not crop to ping's grown visual\n scissor={sc:?} pulse={pulse:?} ping_visual={ping_visual:?}"
        );
    }

    #[test]
    fn dest_reset_pulse_last_present_must_cover_vertex_aa() {
        // `animation: … alternate pulse` only changes opacity. Last-present dest-reset
        // used the layout box (no +2 vertex AA), so rounded-md corners left a frame.
        let pulse = [52.0, 20.0, 48.0, 32.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 90.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut pulse_fill = blank_instance(pulse[0], pulse[1], pulse[2], pulse[3]);
        pulse_fill.color = [0.55, 0.35, 0.85, 0.5];
        pulse_fill.radii = [6.0; 4];
        let paints = [paint_slot(2, pulse, Some(pulse), (1, 2))];
        let ink = instance_paint_bounds(&pulse_fill);
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            pulse,
            &paints,
            &[page, pulse_fill],
            &[],
            1.0,
            Some(pulse),
        )
        .expect("card behind pulse");
        assert!(
            aabb_covers(wipe, ink),
            "pulse dest-reset must cover SDF vertex AA, not just the layout box\n wipe={wipe:?} ink={ink:?} pulse={pulse:?}"
        );
        assert!(
            wipe[0] <= pulse[0] - 1.5 && wipe[1] <= pulse[1] - 1.5,
            "wipe must outset the rounded box by vertex AA\n wipe={wipe:?} pulse={pulse:?}"
        );
    }

    #[test]
    fn instance_paint_bounds_includes_vertex_aa_for_simple_fill() {
        let i = blank_instance(10.0, 10.0, 40.0, 20.0);
        let b = instance_paint_bounds(&i);
        assert!(
            b[0] <= 8.0 && b[1] <= 8.0 && b[0] + b[2] >= 52.0 && b[1] + b[3] >= 32.0,
            "simple fill occupancy must include shader vertex AA (+2), got {b:?}"
        );
    }

    #[test]
    fn dest_reset_scale_chip_must_not_aabb_fill_capsule() {
        // Reaction chip hover:scale dest-resets a Bake AABB that sits on a capsule
        // with outline. Filling that AABB with the pill's muted color squares the
        // end and erases the 3px card ring. Clear must be the card; replay is the
        // pill's own SDF chrome (fill + outline), not the parent's children.
        let bar = [10.0, 10.0, 48.0, 28.0];
        let chip = [32.0, 16.0, 16.0, 16.0];
        let mut page = blank_instance(0.0, 0.0, 200.0, 80.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        fill.color = [0.82, 0.82, 0.84, 1.0];
        fill.radii = [14.0; 4];
        let mut outline = blank_instance(7.0, 7.0, 54.0, 34.0);
        outline.color = [0.0, 0.0, 0.0, 0.0];
        outline.border_color = [0.96, 0.96, 0.96, 1.0];
        outline.border_width = 3.0;
        outline.radii = [17.0; 4];
        let bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
        let bar_p = NodePaint {
            key: 1,
            bounds: bar,
            overlay: false,
            instance: (1, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let chip_p = NodePaint {
            key: 2,
            bounds: chip,
            overlay: false,
            instance: (3, 3),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let card_color = page.color;
        let muted = fill.color;
        let paints = [bar_p, chip_p];
        let instances = [page, fill, outline];
        let bake_bounds = textured_paint_bounds(&bake);
        let reset =
            node_present_dest_reset_full(2, chip, &paints, &instances, &[bake], &[], 1.0, None)
                .expect("card under capsule");
        assert_eq!(
            reset.clear, card_color,
            "AABB clear of the pill fill would square the capsule; must use the card\n clear={:?} muted={:?}",
            reset.clear, muted
        );
        assert_eq!(
            reset.replay,
            (1, 3),
            "replay must be the pill fill+outline, not descendant Bake\n replay={:?}",
            reset.replay
        );
        assert!(
            aabb_covers(reset.rect, bake_bounds),
            "wipe must still cover the Bake quad so shrink can erase it\n wipe={:?} bake={:?}",
            reset.rect,
            bake_bounds
        );
    }

    #[test]
    fn dest_reset_replay_must_not_dump_descendant_instances() {
        // Gallery hitch: hover:scale dest-reset used to replay the owner's full
        // collect range (every descendant quad, cull:None) when overflow chips
        // sit outside the pill AABB so nested-foreign detection misses them.
        let bar = [10.0, 10.0, 48.0, 28.0];
        let chip = [32.0, 16.0, 16.0, 16.0];
        let mut page = blank_instance(0.0, 0.0, 800.0, 2400.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        fill.color = [0.82, 0.82, 0.84, 1.0];
        fill.radii = [14.0; 4];
        let mut outline = blank_instance(7.0, 7.0, 54.0, 34.0);
        outline.color = [0.0, 0.0, 0.0, 0.0];
        outline.border_color = [0.96, 0.96, 0.96, 1.0];
        outline.border_width = 3.0;
        outline.radii = [17.0; 4];
        let extra = 64usize;
        let mut instances = vec![page, fill, outline];
        let mut paints = Vec::new();
        let bar_end = 3 + extra;
        paints.push(NodePaint {
            key: 1,
            bounds: bar,
            overlay: false,
            instance: (1, bar_end),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        });
        for i in 0..extra {
            let x = 80.0 + i as f32 * 12.0;
            let mut child = blank_instance(x, 48.0, 10.0, 10.0);
            child.color = [1.0, 0.0, 0.0, 1.0];
            instances.push(child);
            let idx = 3 + i;
            paints.push(NodePaint {
                key: 10 + i as u64,
                bounds: [x, 48.0, 10.0, 10.0],
                overlay: false,
                instance: (idx, idx + 1),
                textured: (0, 0),
                text: (0, 0),
                chrome: (0, 0),
                paint_ops: (0, 0),
                overflow_clip: None,
                layered: false,
                rest: None,
            });
        }
        paints.push(NodePaint {
            key: 2,
            bounds: chip,
            overlay: false,
            instance: (bar_end, bar_end),
            textured: (0, 1),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        });
        let bake = blank_textured(27.0, 11.0, 26.0, 26.0, MAT2D_IDENTITY);
        let reset =
            node_present_dest_reset_full(2, chip, &paints, &instances, &[bake], &[], 1.0, None)
                .expect("card under capsule");
        assert_eq!(
            reset.replay,
            (1, 3),
            "replay must stop at pill fill+outline, not dump {extra} overflow descendants\n replay={:?}",
            reset.replay
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
    fn dest_reset_unmatched_island_key_must_wipe_old_title_box() {
        // Gallery events → 基础: Match stamps a new Taffy id on the title island.
        // Dest-reset used to return None when the dirty key is gone from this
        // frame's node_paints, so PartialLoad kept 「指针事件」 under 「基础」.
        let old_title = [80.0, 8.0, 96.0, 24.0];
        let new_title = [80.0, 8.0, 36.0, 24.0];
        let mut page = blank_instance(0.0, 0.0, 400.0, 200.0);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut now_fill = blank_instance(new_title[0], new_title[1], new_title[2], new_title[3]);
        now_fill.color = [0.96, 0.96, 0.96, 1.0];
        let now = NodePaint {
            key: 99,
            bounds: new_title,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let (wipe, _) = node_present_dest_reset_scaled(
            41,
            old_title,
            &[now],
            &[page, now_fill],
            &[],
            1.0,
            None,
        )
        .expect("unmatched island dirty must still dest-reset the snapshotted box");
        assert!(
            aabb_covers(wipe, old_title),
            "must wipe leftover 「指针事件」 glyphs, not only the new 「基础」 box\n wipe={wipe:?} old={old_title:?} now={new_title:?}"
        );
        assert!(
            !aabb_covers(wipe, [0.0, 0.0, 400.0, 200.0]),
            "must not fall back to a window wipe\n wipe={wipe:?}"
        );
    }

    #[test]
    fn dest_reset_reused_page_island_must_wipe_previous_page_band() {
        // Gallery 浮层 → 数据: Match reuses the ScrollArea Taffy id. Occupancy
        // used to shrink that dest-reset against a tall foreign slot whose
        // center sits above the island, leaving ViewportModal in retain.
        let island = [200.0, 80.0, 800.0, 600.0];
        let leftover = [220.0, 100.0, 400.0, 180.0];
        let table = [220.0, 300.0, 760.0, 140.0];
        let tall_foreign = [200.0, -1000.0, 800.0, 1200.0];
        let mut page = blank_instance(island[0], island[1], island[2], island[3]);
        page.color = [0.96, 0.96, 0.96, 1.0];
        let mut table_fill = blank_instance(table[0], table[1], table[2], table[3]);
        table_fill.color = [1.0, 1.0, 1.0, 1.0];
        let mut foreign_fill = blank_instance(
            tall_foreign[0],
            tall_foreign[1],
            tall_foreign[2],
            tall_foreign[3],
        );
        foreign_fill.color = [0.9, 0.9, 0.9, 1.0];
        let host = NodePaint {
            key: 1,
            bounds: island,
            overlay: false,
            instance: (0, 1),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(island),
            layered: false,
            rest: Some(island),
        };
        let table_slot = NodePaint {
            key: 2,
            bounds: table,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some(island),
            layered: false,
            rest: Some(table),
        };
        let foreign = NodePaint {
            key: 3,
            bounds: tall_foreign,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: Some(tall_foreign),
        };
        let (wipe, _) = node_present_dest_reset_scaled(
            1,
            island,
            &[host, table_slot, foreign],
            &[page, table_fill, foreign_fill],
            &[],
            1.0,
            Some(island),
        )
        .expect("reused page island must dest-reset");
        let lx = leftover[0] + leftover[2] * 0.5;
        let ly = leftover[1] + leftover[3] * 0.5;
        assert!(
            rect_contains_point(wipe, lx, ly),
            "must wipe leftover ViewportModal band, not occupancy-clip it away\n wipe={wipe:?} leftover={leftover:?}"
        );
        assert!(
            aabb_covers(wipe, table) || rect_contains_point(wipe, table[0] + 8.0, table[1] + 8.0),
            "island dest-reset should still cover the new table neighborhood\n wipe={wipe:?} table={table:?}"
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
            !prev_after.is_some_and(|b| aabb_covers(b, ring)),
            "recording after take drops instance outset (empty visual is omitted)\n after={prev_after:?} ring={ring:?}"
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
        assert_ne!(
            wipe_double, wipe,
            "recording after scale_instances_in_place would mis-scale prev and over-wipe\n logical_wipe={wipe:?} doubled={wipe_double:?}"
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
            layered: false,
            rest: None,
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
            layered: false,
            rest: None,
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
    fn last_present_omits_input_caret_overlay() {
        // I-beam is a swapchain overlay after retain blit. Last-present / dest-reset
        // of chrome punched Search into a hairline slice (竖条).
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
            layered: false,
            rest: None,
        };
        let caret_box = [
            caret.rect_pos[0],
            caret.rect_pos[1],
            caret.rect_size[0],
            caret.rect_size[1],
        ];
        let chrome = vec![caret];
        let prev = last_present_visual_slots(&[p_on], &[card, fill], &[], &[], &chrome)
            .into_iter()
            .find(|(k, _)| *k == 41)
            .map(|(_, b)| b);
        if let Some(prev) = prev {
            assert!(
                !aabb_covers(prev, caret_box),
                "last-present must omit I-beam overlay\n prev={prev:?} caret={caret_box:?}"
            );
        }
        let (wipe_now, _) =
            node_present_dest_reset_full(41, box_, &[p_on], &[card, fill], &[], &chrome, 1.0, None)
                .map(|d| (d.rect, d.clear))
                .expect("now without prev");
        assert!(
            !aabb_covers(wipe_now, caret_box),
            "dest-reset of *now* must not treat chrome as dest-ink\n wipe={wipe_now:?} caret={caret_box:?}"
        );
        let ibeam_dirty = crate::app::damage::PaintDirty {
            key: 5,
            bounds: [40.0, 22.0, 2.0, 18.0],
            kind: crate::app::damage::PaintDirtyKind::Node,
        };
        let leaf = slot(5, [12.0, 20.0, 300.0, 24.0], (0, 1));
        assert!(
            paint_dirty_is_caret_overlay(&ibeam_dirty, &[leaf], &[]),
            "I-beam dirty on a text leaf with no SDF is caret overlay"
        );
        assert!(!paint_dirty_is_caret_overlay(
            &ibeam_dirty,
            &[p_on],
            &[card, fill]
        ));
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
    fn named_scroll_blit_pins_keep_hover_scale_leftover_past_caption() {
        // `hover:scale` last-present hangs into the caption below. Occupancy clip
        // (node-present) cuts that leftover so dest-reset cannot punch wrapping
        // text. Scroll blit copies those pixels in the overlap — the pin must
        // still dest-reset the scaled hole or hover sticks until blit stops.
        let rest = [20.0, 40.0, 80.0, 32.0];
        let scaled = [20.0, 32.0, 80.0, 48.0];
        let caption = [20.0, 72.0, 80.0, 24.0];
        let dirty = PaintDirty {
            key: 2,
            bounds: rest,
            kind: PaintDirtyKind::Node,
        };
        let chip_p = NodePaint {
            key: 2,
            bounds: rest,
            overlay: false,
            instance: (0, 1),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 200.0, 200.0]),
            layered: false,
            rest: Some(rest),
        };
        let cap_p = NodePaint {
            key: 3,
            bounds: caption,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 200.0, 200.0]),
            layered: false,
            rest: Some(caption),
        };
        let occupancy = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            rest,
            &[(2, scaled)],
            &[chip_p.clone(), cap_p.clone()],
            8.0,
        );
        assert!(
            occupancy[1] + occupancy[3] <= caption[1] + 1.0,
            "node-present occupancy must still spare the caption, got {occupancy:?}"
        );
        let pins = named_scroll_blit_pins(&[dirty], &[(2, scaled)], &[chip_p, cap_p]);
        assert_eq!(pins.len(), 1);
        let p = pins[0];
        assert!(
            p[1] <= scaled[1] + 1.0 && p[1] + p[3] >= scaled[1] + scaled[3] - 1.0,
            "scroll blit pin must dest-reset scaled leftover, got {p:?} scaled={scaled:?}"
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
            layered: false,
            rest: None,
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

    #[test]
    fn layered_overlay_dest_reset_covers_overlapping_underlay() {
        // Jump pill hangs off the last bubble. In-flow dest-reset would shrink
        // the wipe away from that sibling; a stacking overlay must still erase
        // its own ink on the bubble.
        let mut card = blank_instance(0.0, 0.0, 400.0, 300.0);
        card.color = [0.96, 0.96, 0.97, 1.0];
        let bubble = [100.0, 200.0, 200.0, 60.0];
        let mut bubble_fill = blank_instance(bubble[0], bubble[1], bubble[2], bubble[3]);
        bubble_fill.color = [0.15, 0.15, 0.18, 1.0];
        bubble_fill.radii = [16.0; 4];
        let btn = [160.0, 240.0, 80.0, 36.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [0.1, 0.1, 0.12, 1.0];
        btn_fill.radii = [18.0; 4];
        btn_fill.shadow_color = [0.0, 0.0, 0.0, 0.2];
        btn_fill.shadow_blur = 8.0;
        btn_fill.shadow_offset = [0.0, 4.0];
        let bubble_p = NodePaint {
            key: 1,
            bounds: bubble,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let btn_p = NodePaint {
            key: 2,
            bounds: btn,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (1, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: true,
            rest: None,
        };
        let instances = [card, bubble_fill, btn_fill];
        let blob = instance_paint_bounds(&instances[2]);
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            btn,
            &[bubble_p, btn_p],
            &instances,
            &[],
            1.0,
            Some(blob),
        )
        .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            aabb_covers(wipe, btn),
            "layered dest-reset must erase leftover overlay on the bubble\n wipe={wipe:?} btn={btn:?}"
        );
        assert!(
            aabb_covers(wipe, blob) || wipe[1] + wipe[3] >= btn[1] + btn[3] + 4.0,
            "layered dest-reset must wipe the outset halo or PartialLoad stacks it\n wipe={wipe:?} btn={btn:?} blob={blob:?}"
        );
        assert!(
            node_present_flush_layered_underlay_glyphs(2, btn, &[bubble_p, btn_p]),
            "dest-reset of Latest must recapture wrapping in the wipe"
        );
        let mut btn_hidden = btn_p;
        btn_hidden.instance = (3, 3);
        let (wipe, _) = node_present_dest_reset_scaled(
            2,
            btn,
            &[bubble_p, btn_hidden],
            &instances,
            &[],
            1.0,
            Some(blob),
        )
        .expect("hide dest-reset of jump");
        assert!(
            aabb_covers(wipe, btn),
            "layered dest-reset must erase the opaque pill on the bubble\n wipe={wipe:?} btn={btn:?}"
        );
        assert!(
            aabb_covers(wipe, blob),
            "layered dest-reset must wipe the outset halo or PartialLoad stacks it\n wipe={wipe:?} btn={btn:?} blob={blob:?}"
        );
        assert!(
            node_present_restore_underlay(2, btn, &[bubble_p, btn_p]),
            "layered slot must restore underlay in the dest-reset hole"
        );
        let mut hello = sample_text("hello", 170.0);
        hello.y = 245.0;
        hello.width = 48.0;
        hello.height = 16.0;
        let dirty = PaintDirty {
            key: 2,
            bounds: padded_xywh(btn, 8.0),
            kind: PaintDirtyKind::Node,
        };
        let page_text = [hello];
        let captured = node_present_glyphs_for_dirty(&dirty, &page_text, &[bubble_p, btn_p]);
        assert!(
            captured.is_empty(),
            "post-overlay capture is Latest only; underlay wrapping must not stamp the thread, got {captured:?}"
        );
        let underlay =
            node_present_layered_underlay_capture_text(&[dirty], &page_text, &[bubble_p, btn_p]);
        assert_eq!(
            underlay.len(),
            1,
            "layered dest-reset must restore underlay glyphs under the overlay slot, got {} runs",
            underlay.len()
        );
    }

    #[test]
    fn layered_jump_transformed_shadow_must_not_expand_dest_reset() {
        // Gallery Latest: `0 4px 14px` + hide scale 0.95 / ancestor card matrix.
        // `instance_paint_bounds` of that halo is ~144×96. Dest-reset of it
        // punched wrapping thread and the overlay rail.
        let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
        card.color = [1.0, 1.0, 1.0, 1.0];
        let wrap = [16.0, 300.0, 280.0, 80.0];
        let mut wrap_fill = blank_instance(wrap[0], wrap[1], wrap[2], wrap[3]);
        wrap_fill.color = [0.96, 0.96, 0.97, 1.0];
        let btn = [120.0, 336.0, 80.0, 32.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 1.0];
        btn_fill.radii = [16.0; 4];
        btn_fill.shadow_color = [0.06, 0.09, 0.16, 0.18];
        btn_fill.shadow_blur = 14.0;
        btn_fill.shadow_offset = [0.0, 4.0];
        // Subpixel ancestor translate — not identity, used to take the shadow path.
        btn_fill.transform = [1.0, 0.0, 0.0, 1.0, 0.25, 0.0];
        let bar = [310.0, 0.0, 10.0, 120.0];
        let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        bar_fill.color = [0.1, 0.1, 0.1, 0.4];
        let wrap_p = NodePaint {
            key: 1,
            bounds: wrap,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: false,
            rest: Some(wrap),
        };
        let btn_p = NodePaint {
            key: 2,
            bounds: btn,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (1, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(btn),
        };
        let bar_p = NodePaint {
            key: 3,
            bounds: bar,
            overlay: false,
            instance: (3, 4),
            textured: (0, 0),
            text: (1, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(bar),
        };
        let instances = [card, wrap_fill, btn_fill, bar_fill];
        let halo = instance_paint_bounds(&instances[2]);
        let dirty = padded_xywh(btn, 32.0);
        let paints = [wrap_p, btn_p, bar_p];
        let reset =
            node_present_dest_reset_full(2, dirty, &paints, &instances, &[], &[], 1.0, Some(halo))
                .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            aabb_covers(reset.rect, btn) || intersect_rects(reset.rect, btn).is_some(),
            "wipe must still cover the leftover pill\n wipe={:?} btn={btn:?}",
            reset.rect
        );
        assert!(
            intersect_rects(reset.rect, bar).is_none(),
            "dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
            reset.rect
        );
        assert!(
            node_present_flush_layered_underlay_glyphs(2, dirty, &paints),
            "dest-reset of Latest must recapture wrapping in the wipe"
        );
        let mut btn_hidden = btn_p;
        btn_hidden.instance = (3, 3);
        let hide_paints = [wrap_p, btn_hidden, bar_p];
        let reset = node_present_dest_reset_full(
            2,
            dirty,
            &hide_paints,
            &[card, wrap_fill, bar_fill],
            &[],
            &[],
            1.0,
            Some(halo),
        )
        .expect("hide dest-reset of Latest");
        let wrap_line = [16.0, 368.0, 280.0, 16.0];
        assert!(
            aabb_covers(reset.rect, btn) || intersect_rects(reset.rect, btn).is_some(),
            "wipe must still cover the opaque pill\n wipe={:?} btn={btn:?}",
            reset.rect
        );
        assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 8.0,
            "transformed shadow halo must dest-reset or PartialLoad stacks the fade\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
        assert!(
            intersect_rects(reset.rect, bar).is_none(),
            "dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
            reset.rect
        );
        let mut wrap_run = sample_text("wrapping", wrap_line[0] + 8.0);
        wrap_run.y = wrap_line[1];
        wrap_run.width = 80.0;
        wrap_run.height = wrap_line[3];
        let halo_dirty = PaintDirty {
            key: 2,
            bounds: dirty,
            kind: PaintDirtyKind::Node,
        };
        let underlay =
            node_present_layered_underlay_capture_text(&[halo_dirty], &[wrap_run], &hide_paints);
        assert!(
            !underlay.is_empty(),
            "halo dest-reset must restore wrapping glyphs under the overlay, got {} runs",
            underlay.len()
        );
        let last = last_present_visual_slots(&paints, &instances, &[], &[], &[]);
        let btn_last = last
            .iter()
            .find(|(k, _)| *k == 2)
            .map(|(_, b)| *b)
            .expect("button last-present");
        assert!(
            aabb_covers(btn_last, btn) || intersect_rects(btn_last, btn).is_some(),
            "layered last-present must keep the pill\n last={btn_last:?} btn={btn:?}"
        );
        assert!(
            aabb_covers(btn_last, halo) || btn_last[1] + btn_last[3] >= btn[1] + btn[3] + 8.0,
            "layered last-present must cover the SDF halo so hide-settle can erase it\n last={btn_last:?} halo={halo:?} btn={btn:?}"
        );
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            dirty,
            &[(2, halo)],
            &paints,
            8.0,
        );
        assert!(
            scissor[2] < 200.0,
            "scissor must stay on the pill+halo, not the 100% dock\n scissor={scissor:?} dirty={dirty:?}"
        );
        assert!(
            intersect_rects(scissor, bar).is_none(),
            "scissor must not include the overlay scrollbar\n scissor={scissor:?} bar={bar:?}"
        );
    }

    #[test]
    fn layered_stacking_context_wrapping_must_not_clip_jump_halo() {
        // MessageScroller `data-stacking-context` marks wrapping bubbles layered.
        // Occupancy shrink against those siblings left the SDF halo in retain
        // (stacked enter ring / hide ghost) and punched a pill-sized hole.
        let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
        card.color = [1.0, 1.0, 1.0, 1.0];
        let wrap = [16.0, 300.0, 280.0, 80.0];
        let mut wrap_fill = blank_instance(wrap[0], wrap[1], wrap[2], wrap[3]);
        wrap_fill.color = [0.96, 0.96, 0.97, 1.0];
        let btn = [120.0, 336.0, 80.0, 32.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 1.0];
        btn_fill.radii = [16.0; 4];
        btn_fill.shadow_color = [0.0, 0.0, 0.0, 0.05];
        btn_fill.shadow_blur = 2.0;
        btn_fill.shadow_offset = [0.0, 1.0];
        let bar = [308.0, 8.0, 8.0, 368.0];
        let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        bar_fill.color = [0.1, 0.1, 0.1, 0.4];
        let wrap_p = NodePaint {
            key: 1,
            bounds: wrap,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(wrap),
        };
        let btn_p = NodePaint {
            key: 2,
            bounds: btn,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (1, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(btn),
        };
        let bar_p = NodePaint {
            key: 3,
            bounds: bar,
            overlay: false,
            instance: (3, 4),
            textured: (0, 0),
            text: (1, 1),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(bar),
        };
        let instances = [card, wrap_fill, btn_fill, bar_fill];
        let halo = instance_paint_bounds(&instances[2]);
        let dirty = padded_xywh(btn, 16.0);
        let paints = [wrap_p, btn_p, bar_p];
        let reset =
            node_present_dest_reset_full(2, dirty, &paints, &instances, &[], &[], 1.0, Some(halo))
                .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 4.0,
            "stacking-context wrapping must not occupancy-clip the halo\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
        assert!(
            intersect_rects(reset.rect, bar).is_none(),
            "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
            reset.rect
        );
        let mut btn_hidden = btn_p;
        btn_hidden.instance = (3, 3);
        let hide_paints = [wrap_p, btn_hidden, bar_p];
        let reset = node_present_dest_reset_full(
            2,
            dirty,
            &hide_paints,
            &[card, wrap_fill, bar_fill],
            &[],
            &[],
            1.0,
            Some(halo),
        )
        .expect("hide dest-reset of Latest");
        assert!(
            aabb_covers(reset.rect, halo) || reset.rect[1] + reset.rect[3] >= btn[1] + btn[3] + 4.0,
            "stacking-context wrapping must not occupancy-clip the halo\n wipe={:?} halo={halo:?} btn={btn:?}",
            reset.rect
        );
        assert!(
            intersect_rects(reset.rect, bar).is_none(),
            "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
            reset.rect
        );
        let mut wrap_run = sample_text("wrapping", wrap[0] + 8.0);
        wrap_run.y = btn[1] + 8.0;
        wrap_run.width = 80.0;
        wrap_run.height = 16.0;
        let dirty_node = PaintDirty {
            key: 2,
            bounds: dirty,
            kind: PaintDirtyKind::Node,
        };
        let underlay =
            node_present_layered_underlay_capture_text(&[dirty_node], &[wrap_run], &hide_paints);
        assert!(
            !underlay.is_empty(),
            "halo dest-reset must recapture wrapping in the stacking-context band"
        );

        let bar_reset =
            node_present_dest_reset_full(3, bar, &paints, &instances, &[], &[], 1.0, Some(bar))
                .expect("overlay rail dest-reset");
        assert!(
            intersect_rects(bar_reset.rect, btn).is_none(),
            "scrollbar fade dest-reset must not punch Latest's pill\n wipe={:?} btn={btn:?}",
            bar_reset.rect
        );
    }

    #[test]
    fn layered_dest_reset_must_not_punch_overlay_scrollbar() {
        // Full-width dock last-present / scissor used to dest-reset across the
        // overlay auto rail. Layered clip must keep the wipe on the pill.
        let mut card = blank_instance(0.0, 0.0, 320.0, 384.0);
        card.color = [1.0, 1.0, 1.0, 1.0];
        let btn = [120.0, 340.0, 80.0, 32.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [0.1, 0.1, 0.12, 1.0];
        btn_fill.radii = [16.0; 4];
        let bar = [308.0, 8.0, 8.0, 368.0];
        let mut bar_fill = blank_instance(bar[0], bar[1], bar[2], bar[3]);
        bar_fill.color = [0.7, 0.7, 0.72, 0.4];
        let dock = [0.0, 336.0, 320.0, 40.0];
        let btn_p = NodePaint {
            key: 2,
            bounds: btn,
            overlay: false,
            instance: (1, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(btn),
        };
        let bar_p = NodePaint {
            key: 3,
            bounds: bar,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: Some([0.0, 0.0, 320.0, 384.0]),
            layered: true,
            rest: Some(bar),
        };
        let instances = [card, btn_fill, bar_fill];
        let visible = node_present_dest_reset_full(
            2,
            dock,
            &[btn_p, bar_p],
            &instances,
            &[],
            &[],
            1.0,
            Some(dock),
        )
        .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            aabb_covers(visible.rect, btn) || intersect_rects(visible.rect, btn).is_some(),
            "wipe must still cover the leftover pill\n wipe={:?} btn={btn:?}",
            visible.rect
        );
        assert!(
            visible.rect[2] <= btn[2] + 40.0,
            "full-width dock leftover must not expand Latest wipe into wrapping\n wipe={:?} btn={btn:?} dock={dock:?}",
            visible.rect
        );
        assert!(
            intersect_rects(visible.rect, bar).is_none(),
            "Latest dest-reset must not punch the overlay scrollbar\n wipe={:?} bar={bar:?}",
            visible.rect
        );
        let mut btn_hidden = btn_p;
        btn_hidden.instance = (3, 3);
        let wipe = node_present_dest_reset_full(
            2,
            dock,
            &[btn_hidden, bar_p],
            &[card, bar_fill],
            &[],
            &[],
            1.0,
            Some(dock),
        )
        .expect("hide dest-reset of Latest")
        .rect;
        assert!(
            aabb_covers(wipe, btn) || intersect_rects(wipe, btn).is_some(),
            "wipe must still cover the pill\n wipe={wipe:?} btn={btn:?}"
        );
        assert!(
            wipe[2] <= btn[2] + 40.0,
            "full-width dock dirty must not expand Latest wipe into wrapping\n wipe={wipe:?} btn={btn:?} dock={dock:?}"
        );
        assert!(
            intersect_rects(wipe, bar).is_none(),
            "Latest dest-reset must not punch the overlay scrollbar\n wipe={wipe:?} bar={bar:?}"
        );
        let scissor = node_present_scissor_logical(
            PaintDirtyKind::Node,
            2,
            dock,
            &[(2, dock)],
            &[btn_p, bar_p],
            8.0,
        );
        assert!(
            intersect_rects(scissor, bar).is_none(),
            "Latest node-present scissor must not include the overlay scrollbar\n scissor={scissor:?} bar={bar:?}"
        );
        let pins = named_scroll_blit_pins(
            &[PaintDirty {
                key: 2,
                bounds: dock,
                kind: PaintDirtyKind::Node,
            }],
            &[(2, dock)],
            &[btn_p, bar_p],
        );
        assert!(
            pins.iter().all(|p| intersect_rects(*p, bar).is_none()),
            "scroll-blit pin of Latest must stay off the overlay scrollbar, got {pins:?}"
        );
    }

    #[test]
    fn layered_dest_reset_replays_rounded_underlay_from_skip_blob() {
        // Viewport fingerprint-replay records one blob covering card+bubble.
        // Dest-reset of the jump pill must still replay the bubble SDF, not
        // fill the AABB with the card behind that blob.
        let mut card = blank_instance(0.0, 0.0, 400.0, 300.0);
        card.color = [0.96, 0.96, 0.97, 1.0];
        let bubble = [100.0, 200.0, 200.0, 60.0];
        let mut bubble_fill = blank_instance(bubble[0], bubble[1], bubble[2], bubble[3]);
        bubble_fill.color = [0.15, 0.15, 0.18, 1.0];
        bubble_fill.radii = [16.0; 4];
        let btn = [160.0, 240.0, 80.0, 36.0];
        let mut btn_fill = blank_instance(btn[0], btn[1], btn[2], btn[3]);
        btn_fill.color = [1.0, 1.0, 1.0, 1.0];
        btn_fill.radii = [18.0; 4];
        let viewport = NodePaint {
            key: 10,
            bounds: [0.0, 0.0, 400.0, 300.0],
            overlay: false,
            instance: (0, 2),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let btn_p = NodePaint {
            key: 2,
            bounds: btn,
            overlay: false,
            instance: (2, 3),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: true,
            rest: None,
        };
        let instances = [card, bubble_fill];
        let visible = node_present_dest_reset_full(
            2,
            btn,
            &[viewport, btn_p],
            &[card, bubble_fill, btn_fill],
            &[],
            &[],
            1.0,
            Some(btn),
        )
        .expect("leftover Latest dest-resets so the fade cannot ghost");
        assert!(
            visible.replay.0 <= 1 && visible.replay.1 > 1,
            "must replay the rounded bubble under leftover dest-reset, got {:?}",
            visible.replay
        );
        let mut btn_hidden = btn_p;
        btn_hidden.instance = (2, 2);
        let reset = node_present_dest_reset_full(
            2,
            btn,
            &[viewport, btn_hidden],
            &instances,
            &[],
            &[],
            1.0,
            Some(btn),
        )
        .expect("hide dest-reset of layered pill");
        assert!(
            reset.replay.0 <= 1 && reset.replay.1 > 1,
            "must replay the rounded bubble, not empty owner-chrome, got {:?}",
            reset.replay
        );
        assert!(
            reset.clear[0] > 0.9 && reset.clear[1] > 0.9,
            "wipe behind the bubble is the card, got {:?}",
            reset.clear
        );
    }

    #[test]
    fn in_flow_dest_reset_still_skips_neighbor_captions() {
        let mut nav = sample_text("NavLayout", 8.0);
        nav.width = 40.0;
        nav.height = 16.0;
        let mut typed = sample_text("asdasd", 80.0);
        typed.width = 40.0;
        typed.height = 16.0;
        let input_p = NodePaint {
            key: 2,
            bounds: [72.0, 0.0, 120.0, 32.0],
            overlay: false,
            instance: (0, 0),
            textured: (0, 0),
            text: (1, 2),
            chrome: (0, 0),
            paint_ops: (0, 0),
            overflow_clip: None,
            layered: false,
            rest: None,
        };
        let dirty = PaintDirty {
            key: 2,
            bounds: [72.0, 0.0, 120.0, 32.0],
            kind: PaintDirtyKind::Node,
        };
        let captured = node_present_glyphs_for_dirty(&dirty, &[nav, typed], &[input_p]);
        assert!(
            captured.contains(&1) && !captured.contains(&0),
            "in-flow Node capture stays membership-only, got {captured:?}"
        );
        assert!(!node_present_restore_underlay(2, dirty.bounds, &[input_p]));
    }
}
