//! Carousel orbit offset pure function (product layer).
//!
//! CSS/variant shells stay in `carousel/mod.rs`; only ideal / clamp / index are counted here.

use super::snap;

/// Stack coverflow sideways step (matches card-width formula: viewport × 0.46 × 0.52).
pub fn stack_step_px(viewport_w: f32) -> f32 {
    const CARD_WIDTH_FRAC: f32 = 0.46;
    const SIDE_SCALE_AT_AD1: f32 = 0.82;
    let card_w = if viewport_w > 8.0 {
        viewport_w * CARD_WIDTH_FRAC
    } else {
        return 0.0;
    };
    if card_w <= 8.0 {
        return 0.0;
    }
    let ideal = card_w * 0.52;
    let max_fit = (viewport_w * 0.5 - card_w * SIDE_SCALE_AT_AD1 * 0.5 - 2.0).max(0.0);
    ideal.min(max_fit).min(200.0)
}

#[derive(Clone, Copy, Debug)]
pub struct IdealOffsetInput {
    pub index: usize,
    pub max_index: usize,
    pub step: f32,
    pub clone_count: usize,
    pub page_delta: f32,
    pub is_stack: bool,
    /// Multi + page + non-seamless loop.
    pub multi_page_linear: bool,
    pub track_loops: bool,
    pub wraps_nav: bool,
    /// Lower bound for clamp when not a loop (most negative offset).
    pub min_offset: f32,
}

/// Page grid plus the clamped final page, which can be shorter than a full page.
pub fn page_snap_offset(off: f32, page: f32, min_offset: f32) -> f32 {
    let page = page.max(1.0);
    let off = off.clamp(min_offset, 0.0);
    let grid = (-(-off / page).round() * page).clamp(min_offset, 0.0);
    if (off - min_offset).abs() < (off - grid).abs() {
        min_offset
    } else {
        grid
    }
}

pub fn ideal_offset(input: IdealOffsetInput) -> f32 {
    let IdealOffsetInput {
        index,
        max_index,
        step,
        clone_count,
        page_delta,
        is_stack,
        multi_page_linear,
        track_loops,
        wraps_nav,
        min_offset,
    } = input;

    if is_stack {
        let i = index.min(max_index);
        let raw = snap::snap_target_for_index(i, step, clone_count);
        return if wraps_nav {
            raw
        } else {
            raw.clamp(min_offset, 0.0)
        };
    }
    if multi_page_linear {
        let page = page_delta.max(step);
        let i = index.min(max_index) as f32;
        return page_snap_offset(-i * step, page, min_offset);
    }
    let i = index.min(max_index);
    let raw = snap::snap_target_for_index(i, step, clone_count);
    if !track_loops {
        return raw.clamp(min_offset, 0.0);
    }
    raw
}

pub fn clamp_linear_offset(off: f32, min_offset: f32, track_loops: bool) -> f32 {
    if track_loops {
        off
    } else {
        off.clamp(min_offset, 0.0)
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn ideal_single_page_snaps_to_index() {
        let off = ideal_offset(IdealOffsetInput {
            index: 2,
            max_index: 4,
            step: 100.0,
            clone_count: 0,
            page_delta: 100.0,
            is_stack: false,
            multi_page_linear: false,
            track_loops: false,
            wraps_nav: false,
            min_offset: -400.0,
        });
        assert!((off - (-200.0)).abs() < 0.01);
    }

    #[test]
    fn stack_step_scales_with_viewport() {
        let s = stack_step_px(400.0);
        assert!(s > 0.0 && s <= 200.0);
    }
}
