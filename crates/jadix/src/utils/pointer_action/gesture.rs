//! Two-pointer pinch / rotate (interact.js `actions/gesture`).
//!
//! interact exposes `distance` / `angle` / `scale` / `ds` / `da` / `box`.
//! Jadix also reports **centroid** and **translation** (centroid delta) so a
//! caller can pan+zoom+rotate from one tick without a second drag session.

use super::BoxRect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureState {
    pub distance: f32,
    /// Radians, `atan2(dy, dx)` from pointer 0 → pointer 1.
    pub angle: f32,
    /// `distance / start_distance`.
    pub scale: f32,
    pub ds: f32,
    pub da: f32,
    /// Axis-aligned box of the current pointers (interact `box`).
    pub box_rect: BoxRect,
    /// Midpoint of the two pointers (Jadix beyond).
    pub centroid: [f32; 2],
    /// Centroid minus the start centroid (Jadix beyond).
    pub translation: [f32; 2],
}

impl Default for GestureState {
    fn default() -> Self {
        Self {
            distance: 0.0,
            angle: 0.0,
            scale: 1.0,
            ds: 0.0,
            da: 0.0,
            box_rect: BoxRect::from_xywh(0.0, 0.0, 0.0, 0.0),
            centroid: [0.0, 0.0],
            translation: [0.0, 0.0],
        }
    }
}

impl GestureState {
    /// One-shot: compare two current points to a start pair (Gallery / tests).
    pub fn from_pair(start: [[f32; 2]; 2], now: [[f32; 2]; 2]) -> Self {
        let mut sess = GestureSession::begin(start[0], start[1]);
        sess.tick(now[0], now[1])
    }

    /// Enclosing box of any number of points (Jadix beyond interact's two-touch box).
    pub fn box_of(points: &[[f32; 2]]) -> BoxRect {
        touch_bbox(points)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GestureSession {
    start_distance: f32,
    start_angle: f32,
    start_centroid: [f32; 2],
    prev: GestureState,
}

impl GestureSession {
    pub fn begin(a: [f32; 2], b: [f32; 2]) -> Self {
        let distance = touch_distance(a, b);
        let angle = touch_angle(a, b);
        let centroid = centroid(a, b);
        let box_rect = touch_bbox(&[a, b]);
        let prev = GestureState {
            distance,
            angle,
            scale: 1.0,
            ds: 0.0,
            da: 0.0,
            box_rect,
            centroid,
            translation: [0.0, 0.0],
        };
        Self {
            start_distance: distance.max(1e-6),
            start_angle: angle,
            start_centroid: centroid,
            prev,
        }
    }

    pub fn tick(&mut self, a: [f32; 2], b: [f32; 2]) -> GestureState {
        let distance = touch_distance(a, b);
        let angle = touch_angle(a, b);
        let scale = (distance / self.start_distance).clamp(0.01, 64.0);
        let centroid = centroid(a, b);
        let state = GestureState {
            distance,
            angle,
            scale,
            ds: scale - self.prev.scale,
            da: angle - self.prev.angle,
            box_rect: touch_bbox(&[a, b]),
            centroid,
            translation: [
                centroid[0] - self.start_centroid[0],
                centroid[1] - self.start_centroid[1],
            ],
        };
        self.prev = state;
        state
    }

    /// End / drop-to-one-finger: freeze last values, zero deltas (interact `ending`).
    pub fn end(&self) -> GestureState {
        let mut s = self.prev;
        s.ds = 0.0;
        s.da = 0.0;
        s
    }

    pub fn start_angle(&self) -> f32 {
        self.start_angle
    }
}

pub fn touch_distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

pub fn touch_angle(a: [f32; 2], b: [f32; 2]) -> f32 {
    (b[1] - a[1]).atan2(b[0] - a[0])
}

fn centroid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

pub fn touch_bbox(points: &[[f32; 2]]) -> BoxRect {
    if points.is_empty() {
        return BoxRect::from_xywh(0.0, 0.0, 0.0, 0.0);
    }
    let mut min_x = points[0][0];
    let mut min_y = points[0][1];
    let mut max_x = min_x;
    let mut max_y = min_y;
    for p in points.iter().skip(1) {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    BoxRect::from_ltrb(min_x, min_y, max_x, max_y)
}

#[cfg(test)]
#[path = "tests/gesture.rs"]
mod tests;
