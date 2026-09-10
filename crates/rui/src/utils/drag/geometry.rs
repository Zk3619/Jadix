//! Layout rectangles and pointer-space helpers (solid-dnd `layout.ts`).

use std::rc::Rc;

/// Axis lock for a drag (interact.js `lockAxis` / Sortable `direction`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Xy,
    X,
    Y,
    /// First movement past the activation threshold picks X or Y, then locks
    /// (interact.js `lockAxis: 'start'`). Sortable insert still uses
    /// [`Self::sort_axis`] (`Y`) — motion lock is not list direction.
    Start,
}

impl Axis {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "x" | "horizontal" => Self::X,
            "y" | "vertical" => Self::Y,
            "start" | "lock-start" | "first" => Self::Start,
            _ => Self::Xy,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Xy => "xy",
            Self::Start => "start",
        }
    }

    pub fn apply(self, dx: f32, dy: f32) -> (f32, f32) {
        match self {
            Self::X => (dx, 0.0),
            Self::Y => (0.0, dy),
            Self::Xy | Self::Start => (dx, dy),
        }
    }

    /// Dominant axis of a displacement (ties break to `X`, like interact
    /// `lockAxis: 'start'` taking `currentAxis[0]`).
    pub fn resolve_start(dx: f32, dy: f32) -> Self {
        if dx.abs() >= dy.abs() {
            Self::X
        } else {
            Self::Y
        }
    }

    /// interact.js `startAxis`: reject when movement has a **clear** dominant
    /// axis that is not `self`. Equal `|dx|==|dy|` (`currentAxis === 'xy'`)
    /// does not reject. `Xy` / `Start` accept any direction.
    pub fn allows_start_move(self, dx: f32, dy: f32) -> bool {
        match self {
            Self::Xy | Self::Start => true,
            Self::X | Self::Y => {
                if (dx.abs() - dy.abs()).abs() < 1e-4 {
                    true
                } else {
                    Self::resolve_start(dx, dy) == self
                }
            }
        }
    }

    /// Sortable insert / FLIP axis. `Xy` and `Start` behave as `Y` (vertical lists).
    pub fn sort_axis(self) -> Self {
        match self {
            Self::X => Self::X,
            Self::Y | Self::Xy | Self::Start => Self::Y,
        }
    }
}

/// solid-dnd pointer sensor: delay **or** distance vs RUI default **and**.
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

/// Ghost origin vs the source box (solid-dnd overlay measure / center).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayAlign {
    /// `origin = pointer - grab_offset` (item top-left follows the grab point).
    #[default]
    Grab,
    /// Source-box center minus ghost half-size, then pointer delta.
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
            targets: grids
                .into()
                .into_iter()
                .map(SnapTarget::Grid)
                .collect(),
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

/// Viewport-space rectangle (logical px). Matches solid-dnd `Layout`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn from_bounds(b: [f32; 4]) -> Self {
        Self {
            x: b[0],
            y: b[1],
            width: b[2],
            height: b[3],
        }
    }

    pub fn to_bounds(self) -> [f32; 4] {
        [self.x, self.y, self.width, self.height]
    }

    pub fn left(self) -> f32 {
        self.x
    }
    pub fn top(self) -> f32 {
        self.y
    }
    pub fn right(self) -> f32 {
        self.x + self.width
    }
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center(self) -> [f32; 2] {
        [self.x + self.width * 0.5, self.y + self.height * 0.5]
    }

    pub fn corners(self) -> [[f32; 2]; 4] {
        [
            [self.left(), self.top()],
            [self.right(), self.top()],
            [self.right(), self.bottom()],
            [self.left(), self.bottom()],
        ]
    }

    pub fn inflated(self, pad: f32) -> Self {
        if pad <= 0.0 {
            return self;
        }
        Self {
            x: self.x - pad,
            y: self.y - pad,
            width: self.width + pad * 2.0,
            height: self.height + pad * 2.0,
        }
    }

    pub fn translated(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    pub fn contains_point(self, p: [f32; 2]) -> bool {
        p[0] >= self.left() && p[0] <= self.right() && p[1] >= self.top() && p[1] <= self.bottom()
    }

    /// True when `inner` sits strictly inside this rect (nested droppable).
    pub fn contains_rect(self, inner: Self) -> bool {
        inner.left() >= self.left() - 1e-4
            && inner.right() <= self.right() + 1e-4
            && inner.top() >= self.top() - 1e-4
            && inner.bottom() <= self.bottom() + 1e-4
            && inner.area() + 1e-4 < self.area()
    }

    pub fn area(self) -> f32 {
        (self.width * self.height).max(0.0)
    }

    /// interact dropzone `overlap` ratio: `intersection_area / self.area`.
    pub fn overlap_fraction_in(self, zone: Self) -> f32 {
        let left = self.left().max(zone.left());
        let top = self.top().max(zone.top());
        let right = self.right().min(zone.right());
        let bottom = self.bottom().min(zone.bottom());
        let w = right - left;
        let h = bottom - top;
        if w <= 0.0 || h <= 0.0 {
            return 0.0;
        }
        let inter = w * h;
        let area = self.area();
        if area <= 1e-6 {
            0.0
        } else {
            inter / area
        }
    }

    /// Jaccard intersection ratio used by solid-dnd `mostIntersecting`.
    pub fn intersection_ratio(self, other: Self) -> f32 {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        let w = right - left;
        let h = bottom - top;
        if w <= 0.0 || h <= 0.0 {
            return 0.0;
        }
        let inter = w * h;
        let union = self.area() + other.area() - inter;
        if union <= 1e-6 {
            0.0
        } else {
            inter / union
        }
    }

    pub fn is_usable(self) -> bool {
        self.width >= 1.0 && self.height >= 1.0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_locks() {
        assert_eq!(Axis::X.apply(3.0, 4.0), (3.0, 0.0));
        assert_eq!(Axis::Y.apply(3.0, 4.0), (0.0, 4.0));
        assert_eq!(Axis::Xy.apply(3.0, 4.0), (3.0, 4.0));
        assert_eq!(Axis::Start.apply(3.0, 4.0), (3.0, 4.0));
        assert_eq!(Axis::resolve_start(10.0, 3.0), Axis::X);
        assert_eq!(Axis::resolve_start(3.0, 10.0), Axis::Y);
        assert_eq!(Axis::resolve_start(5.0, 5.0), Axis::X);
        assert!(Axis::X.allows_start_move(8.0, 1.0));
        assert!(!Axis::X.allows_start_move(1.0, 8.0));
        assert!(Axis::X.allows_start_move(5.0, 5.0));
        assert_eq!(Axis::Start.sort_axis(), Axis::Y);
        assert_eq!(Axis::parse("start"), Axis::Start);
    }

    #[test]
    fn drag_stack_parse_and_resolve() {
        assert_eq!(DragStack::parse("auto"), DragStack::Auto);
        assert_eq!(DragStack::parse("always-top"), DragStack::Top);
        assert_eq!(DragStack::parse("always_bottom"), DragStack::Bottom);
        assert_eq!(DragStack::parse("none"), DragStack::Keep);
        assert_eq!(DragStack::Keep.resolve(3, 9), 3);
        assert_eq!(
            DragStack::Top.resolve(2, 0),
            DragStack::TOP_BAND.saturating_add(2)
        );
        assert_eq!(
            DragStack::Bottom.resolve(1, 0),
            DragStack::BOTTOM_BAND.saturating_add(1)
        );
        assert_eq!(DragStack::Auto.resolve(4, 0), 4);
        assert_eq!(DragStack::Auto.resolve(4, 7), 7);
        assert!(DragStack::Top.resolve(0, 0) > DragStack::Auto.resolve(0, 99));
        assert!(DragStack::Bottom.resolve(0, 0) < DragStack::Auto.resolve(0, 1));
    }

    #[test]
    fn inflated_grows_box() {
        let r = Rect::from_bounds([10.0, 10.0, 20.0, 8.0]).inflated(4.0);
        assert_eq!(r.to_bounds(), [6.0, 6.0, 28.0, 16.0]);
    }

    #[test]
    fn snap_grid_rounds() {
        let g = SnapGrid::new(10.0, 10.0);
        assert_eq!(g.apply(14.0, 6.0), [10.0, 10.0]);
    }

    #[test]
    fn snap_grid_range_skips_when_far() {
        let g = SnapGrid::new(10.0, 10.0).range(2.0);
        assert_eq!(g.apply(14.0, 10.0), [14.0, 10.0], "4px from 10 > range 2");
        assert_eq!(g.apply(11.0, 10.0), [10.0, 10.0]);
    }

    #[test]
    fn snap_targets_picks_closest_in_range_grid() {
        let t = SnapTargets::new([
            SnapGrid::new(8.0, 8.0),
            SnapGrid::new(32.0, 32.0).offset([16.0, 16.0]),
        ])
        .range(20.0);
        assert_eq!(t.apply(10.0, 10.0), [8.0, 8.0]);
        assert_eq!(t.apply(30.0, 30.0), [32.0, 32.0]);
    }

    #[test]
    fn snap_targets_skips_when_all_out_of_range() {
        let t = SnapTargets::new([SnapGrid::new(10.0, 10.0).range(2.0)]).range(2.0);
        assert_eq!(t.apply(14.0, 10.0), [14.0, 10.0]);
    }

    #[test]
    fn snap_fn_target_snaps_to_line() {
        use std::rc::Rc;
        let t = SnapTargets::default().with_fn(Rc::new(|x, _y| Some([x, 64.0])));
        assert_eq!(t.apply(12.0, 70.0), [12.0, 64.0]);
    }

    #[test]
    fn restrict_anchor_center_lets_origin_leave_box() {
        let origin = clamp_origin_to_box_anchor(
            [-10.0, -10.0],
            [20.0, 20.0],
            [0.0, 0.0, 100.0, 80.0],
            RestrictAnchor::CENTER,
        );
        assert!((origin[0] + 10.0).abs() < 1e-3, "{origin:?}");
        assert!((origin[1] + 10.0).abs() < 1e-3, "{origin:?}");
    }

    #[test]
    fn intersection_none_and_full() {
        let a = Rect::from_bounds([0.0, 0.0, 10.0, 10.0]);
        let b = Rect::from_bounds([20.0, 20.0, 10.0, 10.0]);
        assert_eq!(a.intersection_ratio(b), 0.0);
        assert!((a.intersection_ratio(a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn clamp_keeps_ghost_inside() {
        let p = clamp_origin_to_box([90.0, 90.0], [20.0, 20.0], [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(p, [80.0, 80.0]);
    }

    #[test]
    fn restrict_parse_viewport_none_and_element_id() {
        assert_eq!(Restrict::parse(""), Restrict::None);
        assert_eq!(Restrict::parse("none"), Restrict::None);
        assert_eq!(Restrict::parse("viewport"), Restrict::Viewport);
        assert_eq!(Restrict::parse("window"), Restrict::Viewport);
        assert_eq!(Restrict::parse("parent"), Restrict::Parent);
        assert_eq!(Restrict::Parent.as_str(), "parent");
        assert_eq!(
            Restrict::parse("corners-arena"),
            Restrict::Element("corners-arena".into())
        );
        assert_eq!(
            Restrict::parse("#dock"),
            Restrict::Element("dock".into()),
            "leading # is optional CSS-id sugar"
        );
    }

    #[test]
    fn contains_rect_is_strict_nesting() {
        let outer = Rect::from_bounds([0.0, 0.0, 100.0, 100.0]);
        let inner = Rect::from_bounds([10.0, 10.0, 20.0, 20.0]);
        assert!(outer.contains_rect(inner));
        assert!(!inner.contains_rect(outer));
        assert!(!outer.contains_rect(outer));
        let overlap = Rect::from_bounds([80.0, 80.0, 40.0, 40.0]);
        assert!(!outer.contains_rect(overlap));
    }
}
