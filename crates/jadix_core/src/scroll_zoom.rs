//! Viewport magnification is independent of layout/CSS zoom. Offsets and visible
//! extents are always in unscaled document coordinates; anchors are viewport pixels.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScrollZoom {
    pub min: f64,
    pub max: f64,
    pub initial: f64,
    pub gestures: bool,
}
impl Default for ScrollZoom {
    fn default() -> Self {
        Self { min: 0.25, max: 4.0, initial: 1.0, gestures: true }
    }
}
impl ScrollZoom {
    pub fn is_valid(self) -> bool {
        [self.min, self.max, self.initial].into_iter().all(f64::is_finite)
            && self.min >= 0.01 && self.max <= 100.0
            && self.min <= self.initial && self.initial <= self.max
    }

    /// Preserve the document point below the anchor, clamping at content edges.
    /// Invalid input is rejected without mutating caller state.
    pub fn zoom_to(self, state: ZoomViewport, scale: f64, anchor: [f64; 2]) -> Option<ZoomViewport> {
        if !self.is_valid() || !state.is_valid() || !scale.is_finite() || scale <= 0.0
            || anchor.into_iter().any(|v| !v.is_finite()) { return None; }
        let scale = scale.clamp(self.min, self.max);
        let mut offset = [0.0; 2];
        for axis in 0..2 {
            let anchor = anchor[axis].clamp(0.0, state.viewport[axis]);
            offset[axis] = (state.offset[axis] + anchor / state.scale - anchor / scale)
                .clamp(0.0, (state.content[axis] - state.viewport[axis] / scale).max(0.0));
        }
        Some(ZoomViewport { scale, offset, ..state })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoomViewport {
    pub scale: f64,
    pub offset: [f64; 2],
    pub viewport: [f64; 2],
    pub content: [f64; 2],
}
impl ZoomViewport {
    pub fn is_valid(self) -> bool {
        self.scale.is_finite() && self.scale > 0.0
            && self.offset.into_iter().chain(self.viewport).chain(self.content)
                .all(|v| v.is_finite() && v >= 0.0)
    }
    pub fn visible_size(self) -> [f64; 2] { self.viewport.map(|v| v / self.scale) }
    pub fn document_point(self, point: [f64; 2]) -> [f64; 2] {
        [self.offset[0] + point[0] / self.scale, self.offset[1] + point[1] / self.scale]
    }
}
