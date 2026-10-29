//! Layout rectangles and pointer-space helpers (solid-dnd `layout.ts`).

use std::rc::Rc;

pub use crate::core::geometry::{Axis, Rect};

/// solid-dnd pointer sensor: delay **or** distance vs Jadix default **and**.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activation {
    /// Distance **and** delay (current `tick`). Default.
    #[default]
    And,
    /// Distance **or** delay (solid-dnd 250 ms / 10 px).
    Or,
}

impl Activation {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "or" => Self::Or,
            _ => Self::And,
        }
    }
}

/// Live list preview: insert-and-shift (default) vs swap two items (Sortable Swap plugin).
///
/// `Swap` turns off sibling FLIP and paints `jadix_drag_swap` on the peer. Submit
/// with [`super::sortable::apply_swap_event`] / [`super::sortable::apply_event`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Insert,
    Swap,
}

impl SortMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "swap" | "exchange" => Self::Swap,
            _ => Self::Insert,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Swap => "swap",
        }
    }
}

/// Ghost origin vs the pointer (solid-dnd overlay measure / center).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayAlign {
    /// `origin = pointer - grab_offset` (item top-left follows the grab point).
    #[default]
    Grab,
    /// Ghost center follows the (axis / restrict / snap) pointer:
    /// `origin = pointer - ghost_size / 2`.
    Center,
}

impl OverlayAlign {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" => Self::Center,
            _ => Self::Grab,
        }
    }
}

/// Stacking among overlapping draggables in one container (solid-dnd arbitrary move).
///
/// Author `z-index` still distinguishes items that share a policy (same or different).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DragStack {
    /// Raise this item above Auto / Keep siblings while dragging; keep the new
    /// order after drop (and after Esc cancel, which also leaves it on top).
    #[default]
    Auto,
    /// Always paint above Auto / Keep / Bottom, even at rest. Drag does not change it.
    Top,
    /// Always paint below Auto / Keep / Top, even at rest. Drag does not raise it.
    Bottom,
    /// Author `z-index` only; drag never writes stacking.
    Keep,
}

impl DragStack {
    /// Band for [`Self::Bottom`] so it stays under Auto ranks (`1…`).
    pub const BOTTOM_BAND: i32 = -1_000_000;
    /// Band for [`Self::Top`] so it stays above Auto ranks.
    pub const TOP_BAND: i32 = 1_000_000;

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "top" | "always-top" | "always_top" | "always-on-top" => Self::Top,
            "bottom" | "always-bottom" | "always_bottom" | "always-on-bottom" => Self::Bottom,
            "keep" | "none" | "unchanged" => Self::Keep,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Keep => "keep",
        }
    }

    /// Resting (or already-bumped Auto) z. Callers bump Auto rank before this
    /// when the item is the active drag.
    pub fn resolve(self, author_z: i32, auto_rank: i32) -> i32 {
        match self {
            Self::Keep => author_z,
            Self::Top => Self::TOP_BAND.saturating_add(author_z),
            Self::Bottom => Self::BOTTOM_BAND.saturating_add(author_z),
            Self::Auto => {
                if auto_rank != 0 {
                    auto_rank
                } else {
                    author_z
                }
            }
        }
    }
}

impl From<&str> for DragStack {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for DragStack {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// Where the transformed ghost may travel (interact.js `restrict`).
///
/// Parse: empty / `none` → [`Self::None`]; `viewport` / `window` →
/// [`Self::Viewport`]; `parent` → [`Self::Parent`]; any other token is a live
/// element id ([`Self::Element`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Restrict {
    #[default]
    None,
    /// Clamp the ghost origin into the window `[0,0,vw,vh]`.
    Viewport,
    /// Clamp to the draggable's author parent (skips the kit item shell).
    /// interact.js `restriction: 'parent'` — no id required on the container.
    Parent,
    /// Explicit viewport-space box `[x, y, w, h]`.
    Rect([f32; 4]),
    /// Clamp to the named node's pointer-space layout box (Droppable id,
    /// `template_id`, or `id`). Resolved each tick so nested scroll stays live.
    Element(String),
}

impl Restrict {
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() {
            return Self::None;
        }
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "none" | "off" | "false" => Self::None,
            "viewport" | "window" => Self::Viewport,
            "parent" => Self::Parent,
            _ => {
                let id = s.strip_prefix('#').unwrap_or(s);
                if id.is_empty() {
                    Self::None
                } else {
                    Self::Element(id.to_string())
                }
            }
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Viewport => "viewport",
            Self::Parent => "parent",
            Self::Rect(_) => "rect",
            Self::Element(id) => id.as_str(),
        }
    }
}

impl From<&str> for Restrict {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for Restrict {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// Optional snap-to-grid after axis lock (interact.js `snap`).
///
/// `range <= 0` always snaps (interact default `Infinity`). `end_only` skips
/// the builtin during `tick` and applies once in [`super::session::end`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGrid {
    pub x: f32,
    pub y: f32,
    pub offset: [f32; 2],
    /// Magnetic radius in pointer px. `0` = always snap.
    pub range: f32,
    /// Snap only when the session ends (interact `endOnly`).
    pub end_only: bool,
}

impl Default for SnapGrid {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            offset: [0.0, 0.0],
            range: 0.0,
            end_only: false,
        }
    }
}

impl SnapGrid {
    pub fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            ..Self::default()
        }
    }

    pub fn offset(mut self, offset: [f32; 2]) -> Self {
        self.offset = offset;
        self
    }

    pub fn range(mut self, range: f32) -> Self {
        self.range = range.max(0.0);
        self
    }

    pub fn end_only(mut self, yes: bool) -> Self {
        self.end_only = yes;
        self
    }

    pub fn apply(self, x: f32, y: f32) -> [f32; 2] {
        let sx = if self.x.abs() < 1e-4 {
            x
        } else {
            ((x - self.offset[0]) / self.x).round() * self.x + self.offset[0]
        };
        let sy = if self.y.abs() < 1e-4 {
            y
        } else {
            ((y - self.offset[1]) / self.y).round() * self.y + self.offset[1]
        };
        if self.range > 0.0 {
            let dx = sx - x;
            let dy = sy - y;
            if (dx * dx + dy * dy).sqrt() > self.range {
                return [x, y];
            }
        }
        [sx, sy]
    }
}

/// interact.js `SnapFunction` — return a snap point or `None` to skip.
pub type SnapPointFn = Rc<dyn Fn(f32, f32) -> Option<[f32; 2]>>;

/// One entry in interact `snap.targets` (grid or function).
#[derive(Clone)]
pub enum SnapTarget {
    Grid(SnapGrid),
    Fn(SnapPointFn),
}

/// interact.js `snap.targets` — grids and/or functions; closest in-range snap wins.
#[derive(Clone, Default)]
pub struct SnapTargets {
    pub targets: Vec<SnapTarget>,
    /// Global magnetic radius when a grid's own `range` is `0` (`0` = always snap).
    pub range: f32,
    /// Skip builtin snap during `tick`; apply once in [`super::session::end`].
    pub end_only: bool,
}

impl SnapTargets {
    pub fn new(grids: impl Into<Vec<SnapGrid>>) -> Self {
        Self {
            targets: grids.into().into_iter().map(SnapTarget::Grid).collect(),
            range: 0.0,
            end_only: false,
        }
    }

    pub fn with_fn(mut self, f: SnapPointFn) -> Self {
        self.targets.push(SnapTarget::Fn(f));
        self
    }

    pub fn range(mut self, range: f32) -> Self {
        self.range = range.max(0.0);
        self
    }

    pub fn end_only(mut self, yes: bool) -> Self {
        self.end_only = yes;
        self
    }

    fn candidate_point(&self, target: &SnapTarget, x: f32, y: f32) -> Option<[f32; 2]> {
        match target {
            SnapTarget::Grid(grid) => {
                let snapped = grid.apply(x, y);
                if snapped == [x, y] && grid.range > 0.0 {
                    None
                } else {
                    Some(snapped)
                }
            }
            SnapTarget::Fn(f) => f(x, y),
        }
    }

    fn target_range(&self, target: &SnapTarget) -> f32 {
        match target {
            SnapTarget::Grid(g) if g.range > 0.0 => g.range,
            _ => self.range,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> [f32; 2] {
        if self.targets.is_empty() {
            return [x, y];
        }
        let mut best_dist = f32::INFINITY;
        let mut best = [x, y];
        let mut found = false;
        for target in &self.targets {
            let Some(snapped) = self.candidate_point(target, x, y) else {
                continue;
            };
            let dx = snapped[0] - x;
            let dy = snapped[1] - y;
            let dist = (dx * dx + dy * dy).sqrt();
            let range = self.target_range(target);
            if range > 0.0 && dist > range {
                continue;
            }
            found = true;
            if dist < best_dist {
                best_dist = dist;
                best = snapped;
            }
        }
        if found {
            best
        } else {
            [x, y]
        }
    }
}

/// Single grid or interact-style multi-target snap.
#[derive(Clone)]
pub enum Snap {
    Grid(SnapGrid),
    Targets(SnapTargets),
}

impl Snap {
    pub fn grid(grid: SnapGrid) -> Self {
        Self::Grid(grid)
    }

    pub fn targets(targets: SnapTargets) -> Self {
        Self::Targets(targets)
    }

    pub fn end_only(&self) -> bool {
        match self {
            Snap::Grid(g) => g.end_only,
            Snap::Targets(t) => t.end_only,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> [f32; 2] {
        match self {
            Snap::Grid(g) => g.apply(x, y),
            Snap::Targets(t) => t.apply(x, y),
        }
    }
}

impl From<SnapGrid> for Snap {
    fn from(grid: SnapGrid) -> Self {
        Snap::Grid(grid)
    }
}

/// Which part of the ghost must stay inside the restrict box (interact.js
/// `elementRect`, 0–1 fractions of item size). Default is the full box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestrictAnchor {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl RestrictAnchor {
    /// Entire item inside the box (current clamp).
    pub const FULL: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
    };
    /// Only the center point must stay inside (interact `{0.5,0.5,0.5,0.5}`).
    pub const CENTER: Self = Self {
        left: 0.5,
        top: 0.5,
        right: 0.5,
        bottom: 0.5,
    };

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" | "centre" | "mid" => Self::CENTER,
            _ => Self::FULL,
        }
    }
}

impl Default for RestrictAnchor {
    fn default() -> Self {
        Self::FULL
    }
}

/// Paint overflow of the ghost vs the restrict box.
///
/// Position is still clamped by [`Restrict`] / [`RestrictAnchor`]. This only
/// decides whether ink that extends past the box (rotation, a larger custom
/// overlay, [`RestrictAnchor::CENTER`] corners) is **shown** or **clipped**.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RestrictOverflow {
    /// Ghost may paint outside the restrict box (Pinned overlay default).
    #[default]
    Visible,
    /// Clip the ghost to the restrict rectangle (viewport-space overflow hidden).
    Hidden,
}

impl RestrictOverflow {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "hidden" | "clip" | "hide" => Self::Hidden,
            _ => Self::Visible,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Hidden => "hidden",
        }
    }

    pub fn is_hidden(self) -> bool {
        matches!(self, Self::Hidden)
    }
}

impl From<&str> for RestrictOverflow {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for RestrictOverflow {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

pub fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

/// Clamp a ghost origin so `[origin, origin+size]` stays inside `box` (`[x,y,w,h]`).
pub fn clamp_origin_to_box(origin: [f32; 2], size: [f32; 2], bbox: [f32; 4]) -> [f32; 2] {
    clamp_origin_to_box_anchor(origin, size, bbox, RestrictAnchor::FULL)
}

/// Like [`clamp_origin_to_box`], but only the [`RestrictAnchor`] fraction of the
/// item must remain inside `bbox` (interact `elementRect`).
pub fn clamp_origin_to_box_anchor(
    origin: [f32; 2],
    size: [f32; 2],
    bbox: [f32; 4],
    anchor: RestrictAnchor,
) -> [f32; 2] {
    let w = size[0].max(0.0);
    let h = size[1].max(0.0);
    let left = w * anchor.left.clamp(0.0, 1.0);
    let top = h * anchor.top.clamp(0.0, 1.0);
    let right = w * anchor.right.clamp(0.0, 1.0);
    let bottom = h * anchor.bottom.clamp(0.0, 1.0);
    let min_x = bbox[0] - left;
    let max_x = (bbox[0] + bbox[2] - right).max(min_x);
    let min_y = bbox[1] - top;
    let max_y = (bbox[1] + bbox[3] - bottom).max(min_y);
    [origin[0].clamp(min_x, max_x), origin[1].clamp(min_y, max_y)]
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/geometry.rs"]
mod tests;
