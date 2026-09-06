//! CSS `clip-path` geometry: `none` / `inset()` / `polygon()` / `circle()` / `ellipse()` /
//! `path()` / `url(#id)`.
//!
//! Interpolation, hit-testing, AABB collect, and GPU even-odd packing live here so
//! collect / pointer / SDF do not grow ad-hoc inset fields. `path()` tessellates to
//! [`ClipPath::Polygon`]; `url(#id)` looks up the document clip definition table.

use std::cell::RefCell;
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};

thread_local! {
    static NAMED_CLIPS: RefCell<HashMap<String, ClipPath>> = RefCell::new(HashMap::new());
}

/// Even-odd vs nonzero winding (CSS `fill-rule` on `polygon()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClipFillRule {
    #[default]
    EvenOdd,
    NonZero,
}

impl ClipFillRule {
    fn from_css(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "evenodd" => Some(Self::EvenOdd),
            "nonzero" => Some(Self::NonZero),
            _ => None,
        }
    }
}

/// Length used by clip-path (`px` or `%` of the reference box axis).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipCoord {
    Px(f32),
    /// 0–1 fraction of the corresponding box edge.
    Percent(f32),
}

impl ClipCoord {
    pub fn resolve(self, axis_px: f32) -> f32 {
        match self {
            Self::Px(v) => v,
            Self::Percent(p) => p * axis_px,
        }
    }

    fn lerp(a: Self, b: Self, t: f32) -> Option<Self> {
        let t = t.clamp(0.0, 1.0);
        match (a, b) {
            (Self::Px(x), Self::Px(y)) => Some(Self::Px(x + (y - x) * t)),
            (Self::Percent(x), Self::Percent(y)) => Some(Self::Percent(x + (y - x) * t)),
            _ => None,
        }
    }

    fn parse(tok: &str) -> Option<Self> {
        let s = tok.trim();
        if let Some(rest) = s.strip_suffix('%') {
            let v: f32 = rest.trim().parse().ok()?;
            return Some(Self::Percent(v / 100.0));
        }
        if let Some(rest) = s.strip_suffix("px") {
            return rest.trim().parse().ok().map(Self::Px);
        }
        s.parse().ok().map(Self::Px)
    }
}

/// Author `clip-path` (reference box = border box).
#[derive(Clone, Debug, PartialEq)]
pub enum ClipPath {
    None,
    /// CSS `inset(t r b l)` (1–4 value shorthand already expanded).
    Inset {
        trbl: [ClipCoord; 4],
    },
    Polygon {
        fill: ClipFillRule,
        points: Vec<[ClipCoord; 2]>,
    },
    Circle {
        r: ClipCoord,
        cx: ClipCoord,
        cy: ClipCoord,
    },
    Ellipse {
        rx: ClipCoord,
        ry: ClipCoord,
        cx: ClipCoord,
        cy: ClipCoord,
    },
    /// `url(#id)` — resolved via [`lookup_clip_path`] (document definition table).
    Url(String),
}

impl Default for ClipPath {
    fn default() -> Self {
        Self::None
    }
}

impl ClipPath {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Resolve `url(#id)` through the named clip table (depth-capped).
    pub fn resolved(&self) -> ClipPath {
        self.resolved_at(0)
    }

    fn resolved_at(&self, depth: u8) -> ClipPath {
        if depth > 8 {
            return Self::None;
        }
        match self {
            Self::Url(id) => lookup_clip_path(id)
                .map(|c| c.resolved_at(depth + 1))
                .unwrap_or(Self::None),
            other => other.clone(),
        }
    }

    /// Resolved inset TRBL in px for the given border box (transition pack path).
    pub fn inset_trbl_px(&self, box_wh: [f32; 2]) -> Option<[f32; 4]> {
        match self {
            Self::Inset { trbl } => Some([
                trbl[0].resolve(box_wh[1]).max(0.0),
                trbl[1].resolve(box_wh[0]).max(0.0),
                trbl[2].resolve(box_wh[1]).max(0.0),
                trbl[3].resolve(box_wh[0]).max(0.0),
            ]),
            _ => None,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        parse_clip_path(raw)
    }

    /// Equal-kind, equal-vertex lerp. Mixed kinds or unequal polygon lists are discrete.
    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_resolved(&self.resolved(), &other.resolved(), t)
    }

    /// Axis-aligned bounds of the clip in the given border box `[x, y, w, h]`.
    pub fn aabb(&self, box_xywh: [f32; 4]) -> Option<[f32; 4]> {
        if matches!(self, Self::Url(_)) {
            return self.resolved().aabb(box_xywh);
        }
        if self.is_none() {
            return None;
        }
        let pts = self.to_world_polygon(box_xywh);
        if pts.len() < 3 {
            return Some(self.inset_aabb(box_xywh).unwrap_or(box_xywh));
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for [x, y] in &pts {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        Some([
            min_x,
            min_y,
            (max_x - min_x).max(0.0),
            (max_y - min_y).max(0.0),
        ])
    }

    fn inset_aabb(&self, box_xywh: [f32; 4]) -> Option<[f32; 4]> {
        let [x, y, w, h] = box_xywh;
        let trbl = self.inset_trbl_px([w, h])?;
        let t = trbl[0];
        let r = trbl[1];
        let b = trbl[2];
        let l = trbl[3];
        Some([x + l, y + t, (w - l - r).max(0.0), (h - t - b).max(0.0)])
    }

    /// World-space polygon (border-box origin). Circle / ellipse tessellate to ≤16 verts.
    pub fn to_world_polygon(&self, box_xywh: [f32; 4]) -> Vec<[f32; 2]> {
        if matches!(self, Self::Url(_)) {
            return self.resolved().to_world_polygon(box_xywh);
        }
        let [x, y, w, h] = box_xywh;
        match self {
            Self::None => Vec::new(),
            Self::Inset { .. } => {
                let aabb = self.inset_aabb(box_xywh).unwrap_or(box_xywh);
                let x0 = aabb[0];
                let y0 = aabb[1];
                let x1 = x0 + aabb[2];
                let y1 = y0 + aabb[3];
                vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
            }
            Self::Polygon { points, .. } => points
                .iter()
                .map(|p| [x + p[0].resolve(w), y + p[1].resolve(h)])
                .collect(),
            Self::Circle { r, cx, cy } => {
                let cx = x + cx.resolve(w);
                let cy = y + cy.resolve(h);
                let radius = r.resolve(w.min(h));
                tessellate_ellipse(cx, cy, radius, radius, GPU_CLIP_MAX_PTS)
            }
            Self::Ellipse { rx, ry, cx, cy } => {
                let cx = x + cx.resolve(w);
                let cy = y + cy.resolve(h);
                tessellate_ellipse(cx, cy, rx.resolve(w), ry.resolve(h), GPU_CLIP_MAX_PTS)
            }
            Self::Url(_) => Vec::new(),
        }
    }

    /// Point in border-box space (`local` relative to box origin).
    pub fn contains_local(&self, box_wh: [f32; 2], local: [f32; 2]) -> bool {
        if matches!(self, Self::Url(_)) {
            return self.resolved().contains_local(box_wh, local);
        }
        if self.is_none() {
            return true;
        }
        let box_xywh = [0.0, 0.0, box_wh[0], box_wh[1]];
        match self {
            Self::None => true,
            Self::Inset { .. } => self
                .inset_aabb([0.0, 0.0, box_wh[0], box_wh[1]])
                .map(|r| {
                    local[0] >= r[0]
                        && local[1] >= r[1]
                        && local[0] < r[0] + r[2]
                        && local[1] < r[1] + r[3]
                })
                .unwrap_or(true),
            Self::Circle { r, cx, cy } => {
                let cx = cx.resolve(box_wh[0]);
                let cy = cy.resolve(box_wh[1]);
                let radius = r.resolve(box_wh[0].min(box_wh[1]));
                let dx = local[0] - cx;
                let dy = local[1] - cy;
                dx * dx + dy * dy <= radius * radius
            }
            Self::Ellipse { rx, ry, cx, cy } => {
                let cx = cx.resolve(box_wh[0]);
                let cy = cy.resolve(box_wh[1]);
                let rx = rx.resolve(box_wh[0]).max(1e-6);
                let ry = ry.resolve(box_wh[1]).max(1e-6);
                let dx = (local[0] - cx) / rx;
                let dy = (local[1] - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            Self::Polygon { fill, .. } => {
                let pts = self.to_world_polygon(box_xywh);
                point_in_polygon(local, &pts, *fill)
            }
            Self::Url(_) => true,
        }
    }

    /// Visual hit: `origin` is the painted box top-left (`vx`, `vy`).
    pub fn contains_point(&self, box_xywh: [f32; 4], world: [f32; 2]) -> bool {
        if self.is_none() {
            return true;
        }
        let local = [world[0] - box_xywh[0], world[1] - box_xywh[1]];
        self.contains_local([box_xywh[2], box_xywh[3]], local)
    }

    pub fn to_gpu(&self, box_xywh: [f32; 4]) -> GpuClipPoly {
        if matches!(self, Self::Url(_)) {
            return self.resolved().to_gpu(box_xywh);
        }
        if self.is_none() {
            return GpuClipPoly::NONE;
        }
        if matches!(self, Self::Inset { .. }) {
            // Inset is an AABB; the instance `clip_pos/size` already discards.
            return GpuClipPoly::NONE;
        }
        let pts = self.to_world_polygon(box_xywh);
        GpuClipPoly::from_world_points(
            &pts,
            match self {
                Self::Polygon { fill, .. } => *fill,
                _ => ClipFillRule::EvenOdd,
            },
        )
    }
}

fn interpolate_resolved(this: &ClipPath, other: &ClipPath, t: f32) -> ClipPath {
    let t = t.clamp(0.0, 1.0);
    if t >= 1.0 - 1e-6 {
        return other.clone();
    }
    if t <= 1e-6 {
        return this.clone();
    }
    match (this, other) {
        (ClipPath::None, ClipPath::None) => ClipPath::None,
        (ClipPath::Inset { trbl: a }, ClipPath::Inset { trbl: b }) => {
            let mut trbl = [ClipCoord::Px(0.0); 4];
            for i in 0..4 {
                match ClipCoord::lerp(a[i], b[i], t) {
                    Some(c) => trbl[i] = c,
                    None => return discrete(this, other, t),
                }
            }
            ClipPath::Inset { trbl }
        }
        (
            ClipPath::Polygon {
                fill: fa,
                points: pa,
            },
            ClipPath::Polygon {
                fill: fb,
                points: pb,
            },
        ) if fa == fb && pa.len() == pb.len() && pa.len() >= 3 => {
            let mut points = Vec::with_capacity(pa.len());
            for (a, b) in pa.iter().zip(pb.iter()) {
                let Some(x) = ClipCoord::lerp(a[0], b[0], t) else {
                    return discrete(this, other, t);
                };
                let Some(y) = ClipCoord::lerp(a[1], b[1], t) else {
                    return discrete(this, other, t);
                };
                points.push([x, y]);
            }
            ClipPath::Polygon { fill: *fa, points }
        }
        (
            ClipPath::Circle {
                r: ra,
                cx: cxa,
                cy: cya,
            },
            ClipPath::Circle {
                r: rb,
                cx: cxb,
                cy: cyb,
            },
        ) => {
            let Some(r) = ClipCoord::lerp(*ra, *rb, t) else {
                return discrete(this, other, t);
            };
            let Some(cx) = ClipCoord::lerp(*cxa, *cxb, t) else {
                return discrete(this, other, t);
            };
            let Some(cy) = ClipCoord::lerp(*cya, *cyb, t) else {
                return discrete(this, other, t);
            };
            ClipPath::Circle { r, cx, cy }
        }
        (
            ClipPath::Ellipse {
                rx: rxa,
                ry: rya,
                cx: cxa,
                cy: cya,
            },
            ClipPath::Ellipse {
                rx: rxb,
                ry: ryb,
                cx: cxb,
                cy: cyb,
            },
        ) => {
            let Some(rx) = ClipCoord::lerp(*rxa, *rxb, t) else {
                return discrete(this, other, t);
            };
            let Some(ry) = ClipCoord::lerp(*rya, *ryb, t) else {
                return discrete(this, other, t);
            };
            let Some(cx) = ClipCoord::lerp(*cxa, *cxb, t) else {
                return discrete(this, other, t);
            };
            let Some(cy) = ClipCoord::lerp(*cya, *cyb, t) else {
                return discrete(this, other, t);
            };
            ClipPath::Ellipse { rx, ry, cx, cy }
        }
        _ => discrete(this, other, t),
    }
}

fn discrete(a: &ClipPath, b: &ClipPath, t: f32) -> ClipPath {
    if t >= 0.5 {
        b.clone()
    } else {
        a.clone()
    }
}

fn tessellate_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<[f32; 2]> {
    let n = n.max(8);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = (i as f32) * std::f32::consts::TAU / n as f32;
        out.push([cx + rx * a.cos(), cy + ry * a.sin()]);
    }
    out
}

pub fn point_in_polygon(p: [f32; 2], pts: &[[f32; 2]], fill: ClipFillRule) -> bool {
    if pts.len() < 3 {
        return false;
    }
    match fill {
        ClipFillRule::EvenOdd => evenodd(p, pts),
        ClipFillRule::NonZero => nonzero(p, pts),
    }
}

fn evenodd(p: [f32; 2], pts: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let [xi, yi] = pts[i];
        let [xj, yj] = pts[j];
        let intersect = ((yi > p[1]) != (yj > p[1]))
            && (p[0] < (xj - xi) * (p[1] - yi) / (yj - yi + 1e-12) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn nonzero(p: [f32; 2], pts: &[[f32; 2]]) -> bool {
    let mut wn = 0_i32;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let [xi, yi] = pts[i];
        let [xj, yj] = pts[j];
        if yj <= p[1] {
            if yi > p[1] && is_left([xj, yj], [xi, yi], p) > 0.0 {
                wn += 1;
            }
        } else if yi <= p[1] && is_left([xj, yj], [xi, yi], p) < 0.0 {
            wn -= 1;
        }
        j = i;
    }
    wn != 0
}

fn is_left(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1])
}

/// CSS `border-radius` overlap reduction: scale all corners so adjacent pairs fit the box.
pub fn clamp_border_radii(w: f32, h: f32, radii: [f32; 4]) -> [f32; 4] {
    let r = radii.map(|x| x.max(0.0));
    if w <= 0.0 || h <= 0.0 {
        return [0.0; 4];
    }
    let f = (w / (r[0] + r[1]).max(1e-6))
        .min(w / (r[3] + r[2]).max(1e-6))
        .min(h / (r[0] + r[3]).max(1e-6))
        .min(h / (r[1] + r[2]).max(1e-6))
        .min(1.0);
    r.map(|x| x * f)
}

/// True when any corner radius is large enough to change the clip silhouette.
pub fn radii_affect_clip(radii: [f32; 4]) -> bool {
    radii.iter().any(|r| *r > 0.5)
}

/// Point-in-rounded-rect (border box, CSS radii order TL TR BR BL).
pub fn rounded_rect_contains(box_xywh: [f32; 4], radii: [f32; 4], world: [f32; 2]) -> bool {
    let [x, y, w, h] = box_xywh;
    let px = world[0] - x;
    let py = world[1] - y;
    if px < 0.0 || py < 0.0 || px > w || py > h {
        return false;
    }
    if !radii_affect_clip(radii) {
        return true;
    }
    let r = clamp_border_radii(w, h, radii);
    let tl = r[0];
    let tr = r[1];
    let br = r[2];
    let bl = r[3];
    let (cx, cy, rad) = if px < tl && py < tl {
        (tl, tl, tl)
    } else if px > w - tr && py < tr {
        (w - tr, tr, tr)
    } else if px > w - br && py > h - br {
        (w - br, h - br, br)
    } else if px < bl && py > h - bl {
        (bl, h - bl, bl)
    } else {
        return true;
    };
    if rad <= 0.0 {
        return true;
    }
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= rad * rad
}

/// Tessellate a rounded rect to ≤ [`GPU_CLIP_MAX_PTS`] vertices (clockwise from top-left).
/// Used by `clip-path` circle/ellipse packing; overflow radii use SDF round slots instead.
pub fn tessellate_rounded_rect(box_xywh: [f32; 4], radii: [f32; 4]) -> Vec<[f32; 2]> {
    let [x, y, w, h] = box_xywh;
    let r = clamp_border_radii(w, h, radii);
    if w <= 0.0 || h <= 0.0 || !radii_affect_clip(r) {
        return Vec::new();
    }
    // 4 corners × 4 samples (3 segments) = 16 = GPU_CLIP_MAX_PTS.
    const SEGS: usize = 3;
    let mut pts = Vec::with_capacity(SEGS * 4 + 4);
    let mut arc = |cx: f32, cy: f32, rad: f32, a0: f32| {
        if rad < 0.5 {
            pts.push([cx, cy]);
            return;
        }
        for i in 0..=SEGS {
            let t = i as f32 / SEGS as f32;
            let a = a0 + std::f32::consts::FRAC_PI_2 * t;
            pts.push([cx + rad * a.cos(), cy + rad * a.sin()]);
        }
    };
    let tl = r[0];
    let tr = r[1];
    let br = r[2];
    let bl = r[3];
    arc(x + tl, y + tl, tl, std::f32::consts::PI);
    arc(x + w - tr, y + tr, tr, -std::f32::consts::FRAC_PI_2);
    arc(x + w - br, y + h - br, br, 0.0);
    arc(x + bl, y + h - bl, bl, std::f32::consts::FRAC_PI_2);
    pts
}

/// Max vertices packed into the SDF storage-buffer clip poly (circle/ellipse tessellation).
pub const GPU_CLIP_MAX_PTS: usize = 16;

/// Per-instance clip for the SDF shader: two polygon AND layers + two rounded-rect SDF slots.
///
/// Overflow `border-radius` uses the round-rect slots (same `sd_round_box` as fills) so
/// corners stay circular. `clip-path` polygons still use `pts` / `pts2`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuClipPoly {
    pub count: u32,
    /// 0 = even-odd, 1 = nonzero.
    pub fill: u32,
    pub count2: u32,
    pub fill2: u32,
    pub pts: [[f32; 2]; GPU_CLIP_MAX_PTS],
    pub pts2: [[f32; 2]; GPU_CLIP_MAX_PTS],
    /// World xywh; `z`/`w` (width/height) ≤ 0.5 means inactive.
    pub round_rect: [f32; 4],
    /// CSS radii TL, TR, BR, BL.
    pub round_radii: [f32; 4],
    pub round_rect2: [f32; 4],
    pub round_radii2: [f32; 4],
}

impl GpuClipPoly {
    pub const NONE: Self = Self {
        count: 0,
        fill: 0,
        count2: 0,
        fill2: 0,
        pts: [[0.0; 2]; GPU_CLIP_MAX_PTS],
        pts2: [[0.0; 2]; GPU_CLIP_MAX_PTS],
        round_rect: [0.0; 4],
        round_radii: [0.0; 4],
        round_rect2: [0.0; 4],
        round_radii2: [0.0; 4],
    };

    fn round_slot_active(xywh: [f32; 4]) -> bool {
        xywh[2] > 0.5 && xywh[3] > 0.5
    }

    pub fn from_world_points(pts: &[[f32; 2]], fill: ClipFillRule) -> Self {
        if pts.len() < 3 {
            return Self::NONE;
        }
        let mut packed = Self::NONE;
        packed.fill = match fill {
            ClipFillRule::EvenOdd => 0,
            ClipFillRule::NonZero => 1,
        };
        let n = pts.len().min(GPU_CLIP_MAX_PTS);
        packed.count = n as u32;
        if pts.len() <= GPU_CLIP_MAX_PTS {
            for (i, p) in pts.iter().take(n).enumerate() {
                packed.pts[i] = *p;
            }
        } else {
            for i in 0..n {
                let src = i * pts.len() / n;
                packed.pts[i] = pts[src];
            }
        }
        packed
    }

    pub fn is_active(self) -> bool {
        self.count >= 3
            || self.count2 >= 3
            || Self::round_slot_active(self.round_rect)
            || Self::round_slot_active(self.round_rect2)
    }

    /// Overflow `border-radius` clip via SDF round-box (not a chordal polygon).
    /// Empty when radii are ~0.
    pub fn from_rounded_rect(box_xywh: [f32; 4], radii: [f32; 4]) -> Self {
        if box_xywh[2] <= 0.0 || box_xywh[3] <= 0.0 || !radii_affect_clip(radii) {
            return Self::NONE;
        }
        let r = clamp_border_radii(box_xywh[2], box_xywh[3], radii);
        if !radii_affect_clip(r) {
            return Self::NONE;
        }
        let mut packed = Self::NONE;
        packed.round_rect = box_xywh;
        packed.round_radii = r;
        packed
    }

    fn nearer_poly(src: Self) -> (u32, u32, [[f32; 2]; GPU_CLIP_MAX_PTS]) {
        if src.count2 >= 3 {
            (src.count2, src.fill2, src.pts2)
        } else {
            (src.count, src.fill, src.pts)
        }
    }

    fn nearer_round(src: Self) -> ([f32; 4], [f32; 4]) {
        if Self::round_slot_active(src.round_rect2) {
            (src.round_rect2, src.round_radii2)
        } else {
            (src.round_rect, src.round_radii)
        }
    }

    /// GPU / CPU AND of ancestor clip with a local clip.
    ///
    /// If the ancestor already has two layers, keep the nearer ancestor layer + local
    /// (drop the farther one). Inactive layers pass through. Polygon and round-rect
    /// slots merge independently so `clip-path` can AND with overflow radii.
    pub fn and_layers(ancestor: Self, local: Self) -> Self {
        if !local.is_active() {
            return ancestor;
        }
        if !ancestor.is_active() {
            return local;
        }
        let mut out = Self::NONE;
        let (c, f, pts) = Self::nearer_poly(ancestor);
        out.count = c;
        out.fill = f;
        out.pts = pts;
        let (c, f, pts) = Self::nearer_poly(local);
        out.count2 = c;
        out.fill2 = f;
        out.pts2 = pts;
        let (rect, radii) = Self::nearer_round(ancestor);
        out.round_rect = rect;
        out.round_radii = radii;
        let (rect, radii) = Self::nearer_round(local);
        out.round_rect2 = rect;
        out.round_radii2 = radii;
        out
    }

    fn layer_contains(
        p: [f32; 2],
        count: u32,
        fill: u32,
        pts: &[[f32; 2]; GPU_CLIP_MAX_PTS],
    ) -> bool {
        if count < 3 {
            return true;
        }
        let n = (count as usize).min(GPU_CLIP_MAX_PTS);
        let slice = &pts[..n];
        let rule = if fill == 1 {
            ClipFillRule::NonZero
        } else {
            ClipFillRule::EvenOdd
        };
        point_in_polygon(p, slice, rule)
    }

    fn round_contains(xywh: [f32; 4], radii: [f32; 4], world: [f32; 2]) -> bool {
        if !Self::round_slot_active(xywh) {
            return true;
        }
        rounded_rect_contains(xywh, radii, world)
    }

    /// CPU mirror of the SDF two-layer AND (world pixels).
    pub fn contains_point(self, world: [f32; 2]) -> bool {
        Self::layer_contains(world, self.count, self.fill, &self.pts)
            && Self::layer_contains(world, self.count2, self.fill2, &self.pts2)
            && Self::round_contains(self.round_rect, self.round_radii, world)
            && Self::round_contains(self.round_rect2, self.round_radii2, world)
    }

    fn scale_round_slot(xywh: &mut [f32; 4], radii: &mut [f32; 4], scale: f32) {
        if !Self::round_slot_active(*xywh) {
            return;
        }
        xywh[0] *= scale;
        xywh[1] *= scale;
        xywh[2] *= scale;
        xywh[3] *= scale;
        radii[0] *= scale;
        radii[1] *= scale;
        radii[2] *= scale;
        radii[3] *= scale;
    }

    /// Map logical-pixel clip geometry into physical pixels (same factor as instances).
    pub fn scale_physical(&mut self, scale: f32) {
        if (scale - 1.0).abs() <= f32::EPSILON {
            return;
        }
        let n1 = (self.count as usize).min(GPU_CLIP_MAX_PTS);
        if self.count >= 3 {
            for i in 0..n1 {
                self.pts[i][0] *= scale;
                self.pts[i][1] *= scale;
            }
        }
        let n2 = (self.count2 as usize).min(GPU_CLIP_MAX_PTS);
        if self.count2 >= 3 {
            for i in 0..n2 {
                self.pts2[i][0] *= scale;
                self.pts2[i][1] *= scale;
            }
        }
        Self::scale_round_slot(&mut self.round_rect, &mut self.round_radii, scale);
        Self::scale_round_slot(&mut self.round_rect2, &mut self.round_radii2, scale);
    }

    /// Scale clip geometry about a world-space pivot (Dialog zoom inner chrome).
    pub fn scale_about(&mut self, sx: f32, sy: f32, cx: f32, cy: f32) {
        if (sx - 1.0).abs() <= f32::EPSILON && (sy - 1.0).abs() <= f32::EPSILON {
            return;
        }
        let n1 = (self.count as usize).min(GPU_CLIP_MAX_PTS);
        if self.count >= 3 {
            for i in 0..n1 {
                self.pts[i][0] = cx + (self.pts[i][0] - cx) * sx;
                self.pts[i][1] = cy + (self.pts[i][1] - cy) * sy;
            }
        }
        let n2 = (self.count2 as usize).min(GPU_CLIP_MAX_PTS);
        if self.count2 >= 3 {
            for i in 0..n2 {
                self.pts2[i][0] = cx + (self.pts2[i][0] - cx) * sx;
                self.pts2[i][1] = cy + (self.pts2[i][1] - cy) * sy;
            }
        }
        Self::scale_round_slot_about(&mut self.round_rect, &mut self.round_radii, sx, sy, cx, cy);
        Self::scale_round_slot_about(
            &mut self.round_rect2,
            &mut self.round_radii2,
            sx,
            sy,
            cx,
            cy,
        );
    }

    fn scale_round_slot_about(
        xywh: &mut [f32; 4],
        radii: &mut [f32; 4],
        sx: f32,
        sy: f32,
        cx: f32,
        cy: f32,
    ) {
        if !Self::round_slot_active(*xywh) {
            return;
        }
        xywh[0] = cx + (xywh[0] - cx) * sx;
        xywh[1] = cy + (xywh[1] - cy) * sy;
        xywh[2] *= sx;
        xywh[3] *= sy;
        let s = sx.abs().min(sy.abs());
        radii[0] *= s;
        radii[1] *= s;
        radii[2] *= s;
        radii[3] *= s;
    }
}

/// Clear the document clip definition table (start of a collect pass).
pub fn begin_named_clip_pass() {
    NAMED_CLIPS.with(|m| m.borrow_mut().clear());
}

/// Publish `clip_path` under `url(#id)` (later insert = nearer / last-write-wins).
pub fn register_clip_path(name: &str, clip: ClipPath) {
    let key = super::scroll_timeline::normalize_timeline_name(name);
    if key.is_empty() || matches!(clip, ClipPath::None | ClipPath::Url(_)) {
        return;
    }
    NAMED_CLIPS.with(|m| {
        m.borrow_mut().insert(key, clip);
    });
}

/// Lookup a named clip (tests / `url(#id)` resolve).
pub fn lookup_clip_path(name: &str) -> Option<ClipPath> {
    let key = super::scroll_timeline::normalize_timeline_name(name);
    NAMED_CLIPS.with(|m| m.borrow().get(&key).cloned())
}

/// Extract `#id` from a `url()` argument.
pub fn parse_url_hash(inner: &str) -> Option<String> {
    let s = inner.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    let s = s.strip_prefix('#').unwrap_or(s).trim();
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

/// CSS `clip-path` parser. `none` → [`ClipPath::None`]. Unknown functions → `None` (no change).
pub fn parse_clip_path(raw: &str) -> Option<ClipPath> {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(ClipPath::None);
    }
    let lower = s.to_ascii_lowercase();
    if let Some(inner) = strip_fn(&lower, "inset") {
        return parse_inset(inner).map(|trbl| ClipPath::Inset { trbl });
    }
    if let Some(inner) = strip_fn(&lower, "polygon") {
        return parse_polygon(inner);
    }
    if let Some(inner) = strip_fn(&lower, "circle") {
        return parse_circle(inner);
    }
    if let Some(inner) = strip_fn(&lower, "ellipse") {
        return parse_ellipse(inner);
    }
    if lower.starts_with("path(") {
        return parse_path_fn(s);
    }
    if lower.starts_with("url(") {
        return parse_url_fn(s);
    }
    None
}

fn parse_path_fn(raw: &str) -> Option<ClipPath> {
    let inner = strip_fn_preserve(raw, "path")?;
    let (fill, d_src) = if let Some((rule, rest)) = inner.split_once(',') {
        if let Some(f) = ClipFillRule::from_css(rule) {
            (f, rest.trim())
        } else {
            (ClipFillRule::EvenOdd, inner)
        }
    } else {
        (ClipFillRule::EvenOdd, inner)
    };
    let d = unquote_path(d_src)?;
    tessellate_path_d(d, fill)
}

fn parse_url_fn(raw: &str) -> Option<ClipPath> {
    let inner = strip_fn_preserve(raw, "url")?;
    parse_url_hash(inner).map(ClipPath::Url)
}

fn strip_fn_preserve<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let s = s.trim();
    if s.len() < name.len() + 2 {
        return None;
    }
    if !s.get(..name.len())?.eq_ignore_ascii_case(name) {
        return None;
    }
    s[name.len()..]
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')
        .map(str::trim)
}

fn unquote_path(s: &str) -> Option<&str> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Some(inner);
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return Some(inner);
    }
    Some(s)
}

fn tessellate_path_d(d: &str, fill: ClipFillRule) -> Option<ClipPath> {
    let mut points = Vec::new();
    for cmd in crate::primitives::path::decode_path_d(d) {
        match cmd {
            crate::primitives::path::PathCommand::MoveTo((x, y))
            | crate::primitives::path::PathCommand::LineTo((x, y)) => {
                points.push([ClipCoord::Px(x), ClipCoord::Px(y)]);
            }
            crate::primitives::path::PathCommand::Close => {}
        }
    }
    if points.len() < 3 {
        return None;
    }
    Some(ClipPath::Polygon { fill, points })
}

fn strip_fn<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let s = s.trim();
    let rest = s.strip_prefix(name)?;
    let rest = rest.trim_start();
    rest.strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .map(str::trim)
}

fn parse_inset(inner: &str) -> Option<[ClipCoord; 4]> {
    // Drop optional `round …` suffix (radii not used for AABB clip).
    let geom = inner
        .split_once("round")
        .map(|(g, _)| g)
        .unwrap_or(inner)
        .trim();
    let parts: Vec<ClipCoord> = geom
        .split_whitespace()
        .filter_map(ClipCoord::parse)
        .collect();
    Some(match parts.as_slice() {
        [a] => [*a, *a, *a, *a],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return None,
    })
}

fn parse_polygon(inner: &str) -> Option<ClipPath> {
    let mut s = inner.trim();
    let mut fill = ClipFillRule::EvenOdd;
    if let Some((rule, rest)) = s.split_once(',') {
        if let Some(r) = ClipFillRule::from_css(rule) {
            fill = r;
            s = rest.trim();
        }
    }
    let mut points = Vec::new();
    for pair in s.split(',') {
        let mut it = pair.split_whitespace();
        let x = ClipCoord::parse(it.next()?)?;
        let y = ClipCoord::parse(it.next()?)?;
        points.push([x, y]);
    }
    if points.len() < 3 {
        return None;
    }
    Some(ClipPath::Polygon { fill, points })
}

fn parse_position_pair(rest: &str) -> Option<(ClipCoord, ClipCoord)> {
    let t = rest.trim();
    let t = t.strip_prefix("at").map(str::trim).unwrap_or(t);
    let mut it = t.split_whitespace();
    let a = it.next().unwrap_or("50%");
    let b = it.next().unwrap_or(a);
    let cx = parse_pos_keyword_or_coord(a, true)?;
    let cy = parse_pos_keyword_or_coord(b, false)?;
    Some((cx, cy))
}

fn parse_pos_keyword_or_coord(tok: &str, x_axis: bool) -> Option<ClipCoord> {
    match tok {
        "center" => Some(ClipCoord::Percent(0.5)),
        "left" if x_axis => Some(ClipCoord::Percent(0.0)),
        "right" if x_axis => Some(ClipCoord::Percent(1.0)),
        "top" if !x_axis => Some(ClipCoord::Percent(0.0)),
        "bottom" if !x_axis => Some(ClipCoord::Percent(1.0)),
        _ => ClipCoord::parse(tok),
    }
}

fn parse_circle(inner: &str) -> Option<ClipPath> {
    let s = inner.trim();
    if s.is_empty() {
        return Some(ClipPath::Circle {
            r: ClipCoord::Percent(0.5),
            cx: ClipCoord::Percent(0.5),
            cy: ClipCoord::Percent(0.5),
        });
    }
    let (r_part, pos) = split_at_keyword(s);
    let r = parse_radius_token(r_part.trim())?;
    let (cx, cy) = match pos {
        Some(p) => parse_position_pair(p)?,
        None => (ClipCoord::Percent(0.5), ClipCoord::Percent(0.5)),
    };
    Some(ClipPath::Circle { r, cx, cy })
}

fn parse_ellipse(inner: &str) -> Option<ClipPath> {
    let s = inner.trim();
    if s.is_empty() {
        return Some(ClipPath::Ellipse {
            rx: ClipCoord::Percent(0.5),
            ry: ClipCoord::Percent(0.5),
            cx: ClipCoord::Percent(0.5),
            cy: ClipCoord::Percent(0.5),
        });
    }
    let (r_part, pos) = split_at_keyword(s);
    let mut it = r_part.split_whitespace();
    let rx = parse_radius_token(it.next().unwrap_or("50%"))?;
    let ry = parse_radius_token(it.next().unwrap_or("50%"))?;
    let (cx, cy) = match pos {
        Some(p) => parse_position_pair(p)?,
        None => (ClipCoord::Percent(0.5), ClipCoord::Percent(0.5)),
    };
    Some(ClipPath::Ellipse { rx, ry, cx, cy })
}

fn split_at_keyword(s: &str) -> (&str, Option<&str>) {
    if let Some(i) = s.find(" at ") {
        (&s[..i], Some(s[i + 1..].trim()))
    } else if s.starts_with("at ") {
        ("50%", Some(s))
    } else {
        (s, None)
    }
}

fn parse_radius_token(tok: &str) -> Option<ClipCoord> {
    match tok.trim() {
        "" | "closest-side" => Some(ClipCoord::Percent(0.5)),
        "farthest-side" => Some(ClipCoord::Percent(0.5)),
        t => ClipCoord::parse(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_evenodd_hit_and_miss() {
        let clip = parse_clip_path("polygon(50% 0%, 100% 100%, 0% 100%)").unwrap();
        assert!(clip.contains_local([100.0, 100.0], [50.0, 60.0]));
        assert!(!clip.contains_local([100.0, 100.0], [10.0, 10.0]));
    }

    #[test]
    fn polygon_equal_vertex_lerp() {
        let a = parse_clip_path("polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)").unwrap();
        let b = parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap();
        let mid = a.interpolate(&b, 0.5);
        match mid {
            ClipPath::Polygon { points, .. } => {
                assert_eq!(points.len(), 4);
                match points[0][0] {
                    ClipCoord::Percent(p) => assert!((p - 0.125).abs() < 1e-4),
                    other => panic!("{other:?}"),
                }
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn inset_aabb_and_contains() {
        let clip = parse_clip_path("inset(10px 20px 10px 20px)").unwrap();
        let aabb = clip.aabb([0.0, 0.0, 100.0, 80.0]).unwrap();
        assert!((aabb[0] - 20.0).abs() < 1e-3);
        assert!((aabb[2] - 60.0).abs() < 1e-3);
        assert!(clip.contains_point([0.0, 0.0, 100.0, 80.0], [50.0, 40.0]));
        assert!(!clip.contains_point([0.0, 0.0, 100.0, 80.0], [5.0, 5.0]));
    }

    #[test]
    fn circle_contains() {
        let clip = parse_clip_path("circle(50% at 50% 50%)").unwrap();
        assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
        assert!(!clip.contains_local([100.0, 100.0], [99.0, 99.0]));
    }

    #[test]
    fn none_parses() {
        assert!(matches!(parse_clip_path("none"), Some(ClipPath::None)));
    }

    #[test]
    fn rounded_rect_contains_cuts_corners() {
        let box_xywh = [0.0, 0.0, 80.0, 80.0];
        let r = [40.0; 4];
        assert!(rounded_rect_contains(box_xywh, r, [40.0, 40.0]));
        assert!(!rounded_rect_contains(box_xywh, r, [1.0, 1.0]));
        assert!(rounded_rect_contains(box_xywh, r, [40.0, 1.0]));
        assert!(rounded_rect_contains(box_xywh, [0.0; 4], [1.0, 1.0]));
    }

    #[test]
    fn rounded_rect_gpu_poly_is_active() {
        let poly = GpuClipPoly::from_rounded_rect([10.0, 20.0, 80.0, 80.0], [24.0; 4]);
        assert!(poly.is_active());
        assert!(poly.round_rect[2] > 0.5 && poly.round_rect[3] > 0.5);
        assert_eq!(poly.count, 0, "overflow radii must not tessellate into pts");
        let none = GpuClipPoly::from_rounded_rect([0.0, 0.0, 80.0, 80.0], [0.0; 4]);
        assert!(!none.is_active());
    }

    #[test]
    fn rounded_rect_gpu_poly_matches_sdf_contains() {
        let box_xywh = [0.0, 0.0, 80.0, 80.0];
        let r = [32.0; 4];
        let poly = GpuClipPoly::from_rounded_rect(box_xywh, r);
        assert!(poly.contains_point([40.0, 40.0]));
        assert!(poly.contains_point([40.0, 1.0]));
        assert!(!poly.contains_point([1.0, 1.0]));
        assert_eq!(
            poly.contains_point([8.0, 8.0]),
            rounded_rect_contains(box_xywh, r, [8.0, 8.0])
        );
        assert_eq!(std::mem::size_of::<GpuClipPoly>(), 336);
    }

    #[test]
    fn scale_physical_maps_round_rect_into_instance_space() {
        let mut poly = GpuClipPoly::from_rounded_rect([10.0, 20.0, 80.0, 60.0], [12.0; 4]);
        assert!(poly.contains_point([50.0, 50.0]));
        poly.scale_physical(2.0);
        assert!((poly.round_rect[0] - 20.0).abs() < 1e-4);
        assert!((poly.round_rect[2] - 160.0).abs() < 1e-4);
        assert!((poly.round_radii[0] - 24.0).abs() < 1e-4);
        assert!(
            poly.contains_point([100.0, 100.0]),
            "center in physical px must stay inside"
        );
        assert!(
            !poly.contains_point([15.0, 50.0]),
            "a logical-space interior point left of the scaled origin must miss"
        );
        assert!(!poly.contains_point([22.0, 42.0]));
    }

    #[test]
    fn two_round_rects_and() {
        let outer = GpuClipPoly::from_rounded_rect([0.0, 0.0, 100.0, 100.0], [20.0; 4]);
        let inner = GpuClipPoly::from_rounded_rect([10.0, 10.0, 40.0, 40.0], [8.0; 4]);
        let both = GpuClipPoly::and_layers(outer, inner);
        assert!(both.contains_point([30.0, 30.0]));
        assert!(!both.contains_point([50.0, 50.0]));
        assert!(!both.contains_point([11.0, 11.0]));
    }

    #[test]
    fn path_fn_tessellates_to_polygon() {
        let clip = parse_clip_path(r#"path("M 0 0 L 100 0 L 100 100 L 0 100 Z")"#).unwrap();
        assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
        assert!(!clip.contains_local([100.0, 100.0], [150.0, 50.0]));
    }

    #[test]
    fn url_clip_lookup() {
        begin_named_clip_pass();
        register_clip_path(
            "hole",
            parse_clip_path("polygon(25% 25%, 75% 25%, 75% 75%, 25% 75%)").unwrap(),
        );
        let clip = parse_clip_path("url(#hole)").unwrap();
        assert!(clip.contains_local([100.0, 100.0], [50.0, 50.0]));
        assert!(!clip.contains_local([100.0, 100.0], [5.0, 5.0]));
    }

    #[test]
    fn two_layer_gpu_and() {
        let a = GpuClipPoly::from_world_points(
            &[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
            ClipFillRule::EvenOdd,
        );
        let b = GpuClipPoly::from_world_points(
            &[[40.0, 40.0], [60.0, 40.0], [60.0, 60.0], [40.0, 60.0]],
            ClipFillRule::EvenOdd,
        );
        let both = GpuClipPoly::and_layers(a, b);
        assert!(both.contains_point([50.0, 50.0]));
        assert!(!both.contains_point([10.0, 10.0]));
    }
}
