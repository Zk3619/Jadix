//! Layout-pass cache of used sizes for CSS animations (`max-height: auto` → px).
//!
//! Tick reads [`UsedLayout::anim_height`] / [`UsedLayout::anim_width`] as the cascaded
//! used value when specified is `auto`. Components must not re-measure.

/// Used box after Taffy + collect. Written by layout; read by animation tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsedLayout {
    pub content_width: f32,
    pub content_height: f32,
    pub border_box: [f32; 4],
    /// Last unconstrained (or child-extent) height; survives clip `max-height` frames.
    pub intrinsic_width: f32,
    pub intrinsic_height: f32,
}

impl UsedLayout {
    /// Height keyframes interpolate toward this when specified `max-height`/`height` is `auto`.
    pub fn anim_height(&self) -> f32 {
        self.intrinsic_height
            .max(self.content_height)
            .max(self.border_box[3])
    }

    /// Width counterpart of [`Self::anim_height`].
    pub fn anim_width(&self) -> f32 {
        self.intrinsic_width
            .max(self.content_width)
            .max(self.border_box[2])
    }

    /// Record the laid-out border box. `clips_max_height` is true when present max-height
    /// is a length smaller than content — then intrinsic is **not** overwritten with 0.
    pub fn stamp_border_box(&mut self, border_box: [f32; 4], clips_max_height: bool) {
        self.border_box = border_box;
        let w = border_box[2].max(0.0);
        let h = border_box[3].max(0.0);
        self.content_width = w;
        self.content_height = h;
        if !clips_max_height {
            if w > 1.0 {
                self.intrinsic_width = w;
            }
            if h > 1.0 {
                self.intrinsic_height = h;
            }
        }
    }

    /// Raise intrinsic height from children when the clip shell itself is 0-sized.
    pub fn absorb_child_height(&mut self, child_h: f32) {
        if child_h > 1.0 {
            self.intrinsic_height = self.intrinsic_height.max(child_h);
        }
    }

    pub fn absorb_child_width(&mut self, child_w: f32) {
        if child_w > 1.0 {
            self.intrinsic_width = self.intrinsic_width.max(child_w);
        }
    }
}

/// After collect stamps `layout_bounds`, refresh used-layout on `el`.
pub fn stamp_element_used_layout(el: &mut crate::core::tree::Element) {
    use crate::layout::{dim_kind, DimKind};
    let clips = match dim_kind(el.style.max_size.height) {
        DimKind::Length(px) => px + 0.5 < el.layout_bounds[3].max(el.used_layout.intrinsic_height),
        _ => false,
    };
    // If we are currently clipped to ~0, still treat as clipping so we keep last intrinsic.
    let clips = clips
        || matches!(dim_kind(el.style.max_size.height), DimKind::Length(px) if px < 1.0)
            && el.used_layout.intrinsic_height > 1.0;
    let bounds = el.layout_bounds;
    el.used_layout.stamp_border_box(bounds, clips);

    let py = el.layout_bounds[1];
    let px = el.layout_bounds[0];
    let mut child_h = 0.0f32;
    let mut child_w = 0.0f32;
    for c in &el.children {
        child_h = child_h
            .max((c.layout_bounds[1] - py) + c.layout_bounds[3])
            .max(c.used_layout.anim_height());
        child_w = child_w
            .max((c.layout_bounds[0] - px) + c.layout_bounds[2])
            .max(c.used_layout.anim_width());
    }
    el.used_layout.absorb_child_height(child_h);
    el.used_layout.absorb_child_width(child_w);
}

#[cfg(test)]
#[path = "tests/used_layout.rs"]
mod tests;

/// If collect has not yet stamped used sizes, fall back to `layout_bounds`.
pub fn ensure_used_layout_from_bounds(el: &mut crate::core::tree::Element) {
    if el.used_layout.anim_height() >= 1.0 && el.used_layout.anim_width() >= 1.0 {
        return;
    }
    if el.layout_bounds[2] > 1.0 || el.layout_bounds[3] > 1.0 {
        let bounds = el.layout_bounds;
        el.used_layout.stamp_border_box(bounds, false);
    }
}
