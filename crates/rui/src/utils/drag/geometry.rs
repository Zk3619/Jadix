//! Layout rectangles and pointer-space helpers (solid-dnd `layout.ts`).

/// Axis lock for a drag (interact.js `lockAxis` / Sortable `direction`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Xy,
    X,
    Y,
}

impl Axis {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "x" | "horizontal" => Self::X,
            "y" | "vertical" => Self::Y,
            _ => Self::Xy,
        }
    }

    pub fn apply(self, dx: f32, dy: f32) -> (f32, f32) {
        match self {
            Self::X => (dx, 0.0),
            Self::Y => (0.0, dy),
            Self::Xy => (dx, dy),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGrid {
    pub x: f32,
    pub y: f32,
    pub offset: [f32; 2],
}

impl SnapGrid {
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
        [sx, sy]
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
        if union <= 1e-6 { 0.0 } else { inter / union }
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
    let max_x = (bbox[0] + bbox[2] - size[0]).max(bbox[0]);
    let max_y = (bbox[1] + bbox[3] - size[1]).max(bbox[1]);
    [
        origin[0].clamp(bbox[0], max_x),
        origin[1].clamp(bbox[1], max_y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_locks() {
        assert_eq!(Axis::X.apply(3.0, 4.0), (3.0, 0.0));
        assert_eq!(Axis::Y.apply(3.0, 4.0), (0.0, 4.0));
        assert_eq!(Axis::Xy.apply(3.0, 4.0), (3.0, 4.0));
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
        let g = SnapGrid {
            x: 10.0,
            y: 10.0,
            offset: [0.0, 0.0],
        };
        assert_eq!(g.apply(14.0, 6.0), [10.0, 10.0]);
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
