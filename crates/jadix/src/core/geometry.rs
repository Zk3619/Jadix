//! Shared logical geometry for pointer motion, scrolling, and hit testing.

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

/// Compatibility attribute for chrome that grows into a joined visual edge.
pub const ATTR_PAINT_NUDGE: &str = "jadix_button_group_nudge";

/// Offset of the visual origin; layout and hit geometry remain unchanged.
pub fn paint_nudge(element: &crate::core::tree::Element) -> [f32; 2] {
    match element.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str) {
        Some("x") => [-1.0, 0.0],
        Some("y") => [0.0, -1.0],
        _ => [0.0, 0.0],
    }
}

/// Shared ownership bounds for retained painting and damage invalidation.
pub fn paint_bounds(element: &crate::core::tree::Element, bounds: [f32; 4]) -> [f32; 4] {
    let [offset_x, offset_y] = paint_nudge(element);
    if offset_x == 0.0 && offset_y == 0.0 {
        return bounds;
    }
    [
        bounds[0] + offset_x,
        bounds[1] + offset_y,
        (bounds[2] - offset_x).max(0.0),
        (bounds[3] - offset_y).max(0.0),
    ]
}
