//! Edge auto-scroll (SortableJS `scroll` / `bubbleScroll`).
//!
//! While an item is dragged, viewports under the pointer scroll when the
//! cursor sits in an edge band. Held still still pumps via
//! [`crate::app::lifecycle`] so long lists keep moving — a gap vs Sortable's
//! mouse-move-only fallback on some platforms.

use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_target_x_signal_key, scroll_target_y_signal_key,
    scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
};
use crate::core::tree::Element;

use super::geometry::Rect;

/// SortableJS defaults: 30px band, ~10px/frame. We use a slightly wider band.
#[derive(Clone, Copy, Debug)]
pub struct AutoScroll {
    pub enabled: bool,
    pub sensitivity: f32,
    /// Pixels per 16.67ms (one display frame).
    pub speed: f32,
}

impl Default for AutoScroll {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: 40.0,
            speed: 14.0,
        }
    }
}

/// A laid-out `scroll:` viewport (rest id + box).
#[derive(Clone, Debug)]
pub struct ScrollHit {
    pub rest: String,
    pub rect: Rect,
}

/// Signed scroll delta when `pointer` sits in the edge band of `[start, start+size]`.
pub fn edge_delta(pointer: f32, start: f32, size: f32, sensitivity: f32, step: f32) -> f32 {
    if size <= sensitivity * 2.0 {
        return 0.0;
    }
    if pointer < start + sensitivity {
        -step
    } else if pointer > start + size - sensitivity {
        step
    } else {
        0.0
    }
}

pub fn collect_scroll_hits(root: &Element, out: &mut Vec<ScrollHit>) {
    if let Some(fid) = &root.focus_id {
        if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
            let rect = Rect::from_bounds(root.layout_bounds);
            if rect.is_usable() {
                out.push(ScrollHit {
                    rest: rest.to_string(),
                    rect,
                });
            }
        }
    }
    for c in &root.children {
        collect_scroll_hits(c, out);
    }
}

fn nudge_axis(
    offset_key: &str,
    target_key: &str,
    max_key_vp: &str,
    max_key_content: &str,
    delta: f32,
) -> bool {
    if delta.abs() < 0.05 {
        return false;
    }
    let cur = use_signal(offset_key, || 0.0_f32).peek();
    let vp = use_signal(max_key_vp, || 0.0_f32).peek();
    let content = use_signal(max_key_content, || 0.0_f32).peek();
    let max = (content - vp).max(0.0);
    let next = (cur + delta).clamp(0.0, max);
    if (next - cur).abs() < 0.5 {
        return false;
    }
    use_signal(offset_key, || 0.0_f32).set(next);
    use_signal(target_key, || 0.0_f32).set(next);
    true
}

/// Scroll every overflowing viewport whose edge band contains `pointer`.
pub fn apply(pointer: [f32; 2], views: &[ScrollHit], cfg: AutoScroll, dt: f32) -> bool {
    if !cfg.enabled {
        return false;
    }
    let step = (cfg.speed * (dt * 60.0).clamp(0.25, 2.5)).max(1.0);
    let mut moved = false;
    for v in views {
        let dx = edge_delta(pointer[0], v.rect.x, v.rect.width, cfg.sensitivity, step);
        let dy = edge_delta(pointer[1], v.rect.y, v.rect.height, cfg.sensitivity, step);
        if dx.abs() > 0.05 {
            moved |= nudge_axis(
                &scroll_offset_x_signal_key(&v.rest),
                &scroll_target_x_signal_key(&v.rest),
                &scroll_viewport_w_signal_key(&v.rest),
                &scroll_content_w_signal_key(&v.rest),
                dx,
            );
        }
        if dy.abs() > 0.05 {
            moved |= nudge_axis(
                &scroll_offset_y_signal_key(&v.rest),
                &scroll_target_y_signal_key(&v.rest),
                &scroll_viewport_h_signal_key(&v.rest),
                &scroll_content_h_signal_key(&v.rest),
                dy,
            );
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_delta_top_and_bottom() {
        assert!(edge_delta(5.0, 0.0, 200.0, 40.0, 10.0) < 0.0);
        assert!(edge_delta(190.0, 0.0, 200.0, 40.0, 10.0) > 0.0);
        assert_eq!(edge_delta(100.0, 0.0, 200.0, 40.0, 10.0), 0.0);
    }
}
