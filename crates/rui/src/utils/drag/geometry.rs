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

/// Where the transformed ghost may travel (interact.js `restrict`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Restrict {
    #[default]
    None,
    /// Clamp the ghost origin into the window `[0,0,vw,vh]`.
    Viewport,
    /// Explicit viewport-space box `[x, y, w, h]`.
    Rect([f32; 4]),
}

impl Restrict {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "viewport" | "parent" => Self::Viewport,
            _ => Self::None,
        }
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
}
