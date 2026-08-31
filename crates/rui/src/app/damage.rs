//! Dirty-region helpers for partial redraw (logical pixel coordinates).
//!
//! ## Default frame policy (WP-2.2)
//!
//! - **Logical PartialLoad**: structure unchanged + local `damage_rect` (Gallery counters).
//!   This logical strategy is the **shipped default** (not experimental).
//! - **GPU present (default)**: always swapchain `FullClear` — Solid-aligned coherent paint.
//!   Opt-in retain Load+scissor+blit via `RUI_PARTIAL_DAMAGE=1` (experimental; needs draw
//!   culling and/or fingerprint display-list before it wins). Tree-scoped collect prune
//!   (dirty subtree only) is B8 — it does **not** replace geometric damage.
//!
//! Chrome sync (`StyleChanged` / button hover) must push **local** rects via
//! [`push_frame_paint_damage`] — never expand to the full window on paint-only updates.
//! Scroll / layout moves pixels: PartialLoad+scissor is invalid for those frames.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    /// Per-frame paint dirty rects from chrome sync (button hover, etc.).
    static FRAME_PAINT_DAMAGE: RefCell<Option<[f32; 4]>> = RefCell::new(None);
}

/// Record a local paint dirty rect for this frame (logical px `[x,y,w,h]`).
pub fn push_frame_paint_damage(bounds: [f32; 4]) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    FRAME_PAINT_DAMAGE.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = Some(match *slot {
            Some(existing) => {
                rects_union(&[existing, bounds]).unwrap_or_else(|| union_rects(existing, bounds))
            }
            None => bounds,
        });
    });
}

/// Take and clear the frame-local paint damage accumulated by chrome sync.
pub fn take_frame_paint_damage() -> Option<[f32; 4]> {
    FRAME_PAINT_DAMAGE.with(|slot| slot.borrow_mut().take())
}

/// GPU color attachment load strategy for the main present pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameClearPolicy {
    /// Keep prior framebuffer; scissor to the damage rect.
    PartialLoad,
    /// Clear the whole attachment.
    FullClear,
}

static PARTIAL_LOAD_FRAMES: AtomicU64 = AtomicU64::new(0);
static FULL_CLEAR_FRAMES: AtomicU64 = AtomicU64::new(0);

/// Present-pass Load vs Clear counters (WP-2.2 / Gallery acceptance).
pub fn partial_load_frame_count() -> u64 {
    PARTIAL_LOAD_FRAMES.load(Ordering::Relaxed)
}

pub fn full_clear_frame_count() -> u64 {
    FULL_CLEAR_FRAMES.load(Ordering::Relaxed)
}

/// Reset counters (Gallery "Reset counters" / tests).
pub fn reset_frame_policy_counters() {
    PARTIAL_LOAD_FRAMES.store(0, Ordering::Relaxed);
    FULL_CLEAR_FRAMES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub fn reset_frame_policy_counters_for_test() {
    reset_frame_policy_counters();
}

/// Record which present policy was chosen (call once per submitted frame).
pub fn note_frame_clear_policy(policy: FrameClearPolicy) {
    match policy {
        FrameClearPolicy::PartialLoad => {
            PARTIAL_LOAD_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
        FrameClearPolicy::FullClear => {
            FULL_CLEAR_FRAMES.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Decide Clear vs Load+scissor from present flags and the accumulated damage rect.
pub fn frame_clear_policy(
    has_presented_frame: bool,
    force_full_redraw: bool,
    damage: Option<[f32; 4]>,
    viewport: [f32; 4],
) -> FrameClearPolicy {
    if !has_presented_frame || force_full_redraw {
        return FrameClearPolicy::FullClear;
    }
    match damage {
        Some(d) if !damage_covers_full(viewport, d) => {
            // Off-screen / non-intersecting damage cannot PartialLoad safely.
            if intersect_rects(d, viewport).is_none() {
                FrameClearPolicy::FullClear
            } else {
                FrameClearPolicy::PartialLoad
            }
        }
        _ => FrameClearPolicy::FullClear,
    }
}

/// Axis-aligned intersection of `[x,y,w,h]` rects. `None` when empty / no overlap.
pub fn intersect_rects(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let x2 = ax2.min(bx2);
    let y2 = ay2.min(by2);
    if x2 <= x || y2 <= y {
        return None;
    }
    Some([x, y, x2 - x, y2 - y])
}

/// Convert logical damage + pad to a physical scissor contained in the render target.
///
/// Returns `None` when the padded rect does not intersect the target (caller must FullClear
/// or skip scissor). Never returns a rect that fails wgpu's "contained in render target" check.
pub fn damage_to_scissor_physical(
    damage: [f32; 4],
    pad_logical: f32,
    scale: f32,
    target_w: u32,
    target_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if target_w == 0 || target_h == 0 || damage[2] <= 0.0 || damage[3] <= 0.0 {
        return None;
    }
    let scale = scale.max(0.0);
    let pad = pad_logical.max(0.0);
    let x0 = ((damage[0] - pad) * scale).floor();
    let y0 = ((damage[1] - pad) * scale).floor();
    let x1 = ((damage[0] + damage[2] + pad) * scale).ceil();
    let y1 = ((damage[1] + damage[3] + pad) * scale).ceil();

    let x0 = x0.clamp(0.0, target_w as f32) as u32;
    let y0 = y0.clamp(0.0, target_h as f32) as u32;
    let x1 = x1.clamp(0.0, target_w as f32) as u32;
    let y1 = y1.clamp(0.0, target_h as f32) as u32;
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// Union `bounds` into `damage_rect` for paint-only updates (same-width `bind_text`, etc.).
///
/// Does **not** set `force_full_redraw` — that would force a full Clear and defeat WP-2.2.
pub fn accumulate_paint_damage(damage_rect: &mut Option<[f32; 4]>, bounds: [f32; 4]) {
    if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return;
    }
    *damage_rect = Some(match *damage_rect {
        Some(existing) => {
            rects_union(&[existing, bounds]).unwrap_or_else(|| union_rects(existing, bounds))
        }
        None => bounds,
    });
}

/// Union two axis-aligned rects `[x, y, w, h]`.
pub fn union_rects(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let ax2 = a[0] + a[2];
    let ay2 = a[1] + a[3];
    let bx2 = b[0] + b[2];
    let by2 = b[1] + b[3];
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    let x2 = ax2.max(bx2);
    let y2 = ay2.max(by2);
    [x, y, x2 - x, y2 - y]
}

/// Union an arbitrary list of rects. Returns `None` when `rects` is empty.
pub fn rects_union(rects: &[[f32; 4]]) -> Option<[f32; 4]> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    Some(iter.fold(first, union_rects))
}

/// True when `damage` fully covers `viewport` (same coordinate space, `[x,y,w,h]`).
///
/// Empty / degenerate damage never covers. Floating slack of 0.5px accounts for layout rounding.
pub fn damage_covers_full(viewport: [f32; 4], damage: [f32; 4]) -> bool {
    if viewport[2] <= 0.0 || viewport[3] <= 0.0 || damage[2] <= 0.0 || damage[3] <= 0.0 {
        return false;
    }
    const EPS: f32 = 0.5;
    let (vx, vy, vw, vh) = (viewport[0], viewport[1], viewport[2], viewport[3]);
    let (dx, dy, dw, dh) = (damage[0], damage[1], damage[2], damage[3]);
    dx <= vx + EPS && dy <= vy + EPS && dx + dw + EPS >= vx + vw && dy + dh + EPS >= vy + vh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_disjoint_rects() {
        let u = union_rects([0.0, 0.0, 10.0, 10.0], [20.0, 5.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn union_overlapping_rects() {
        let u = union_rects([0.0, 0.0, 20.0, 20.0], [10.0, 10.0, 20.0, 20.0]);
        assert_eq!(u, [0.0, 0.0, 30.0, 30.0]);
    }

    #[test]
    fn union_contained_rect() {
        let u = union_rects([0.0, 0.0, 100.0, 100.0], [25.0, 25.0, 10.0, 10.0]);
        assert_eq!(u, [0.0, 0.0, 100.0, 100.0]);
    }

    #[test]
    fn rects_union_empty_and_many() {
        assert_eq!(rects_union(&[]), None);
        assert_eq!(
            rects_union(&[[0.0, 0.0, 10.0, 10.0]]),
            Some([0.0, 0.0, 10.0, 10.0])
        );
        let u = rects_union(&[
            [0.0, 0.0, 10.0, 10.0],
            [20.0, 5.0, 10.0, 10.0],
            [5.0, 0.0, 5.0, 5.0],
        ])
        .expect("non-empty");
        assert_eq!(u, [0.0, 0.0, 30.0, 15.0]);
    }

    #[test]
    fn damage_covers_full_viewport() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert!(damage_covers_full(vp, vp));
        assert!(damage_covers_full(vp, [-1.0, -1.0, 802.0, 602.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 400.0, 600.0]));
        assert!(!damage_covers_full(vp, [0.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn frame_clear_policy_partial_for_local_damage() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::PartialLoad
        );
    }

    #[test]
    fn frame_clear_policy_full_when_damage_outside_viewport() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, false, Some([100.0, 700.0, 200.0, 40.0]), vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn damage_to_scissor_clamps_inside_target() {
        // Regression: y=738 with h=1 on a 600-tall target panicked wgpu validation.
        assert_eq!(
            damage_to_scissor_physical([228.0, 730.0, 230.0, 20.0], 8.0, 1.0, 800, 600),
            None
        );
        let s = damage_to_scissor_physical([100.0, 500.0, 50.0, 80.0], 8.0, 1.0, 800, 600)
            .expect("intersects bottom");
        assert!(s.0 + s.2 <= 800);
        assert!(s.1 + s.3 <= 600);
        assert!(s.3 >= 1);
    }

    #[test]
    fn intersect_rects_basic() {
        assert_eq!(
            intersect_rects([0.0, 0.0, 100.0, 100.0], [50.0, 50.0, 100.0, 100.0]),
            Some([50.0, 50.0, 50.0, 50.0])
        );
        assert_eq!(
            intersect_rects([0.0, 0.0, 10.0, 10.0], [20.0, 20.0, 10.0, 10.0]),
            None
        );
    }

    #[test]
    fn frame_clear_policy_full_when_forced_or_covers() {
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, true, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(false, false, Some([10.0, 10.0, 40.0, 20.0]), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(true, false, Some(vp), vp),
            FrameClearPolicy::FullClear
        );
        assert_eq!(
            frame_clear_policy(true, false, None, vp),
            FrameClearPolicy::FullClear
        );
    }

    #[test]
    fn accumulate_paint_damage_does_not_require_full_clear() {
        let mut damage = None;
        let force = false;
        accumulate_paint_damage(&mut damage, [100.0, 200.0, 50.0, 16.0]);
        // Callers must not set force_full when using this helper (WP-2.2 bind_text).
        assert!(!force);
        assert_eq!(damage, Some([100.0, 200.0, 50.0, 16.0]));
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert_eq!(
            frame_clear_policy(true, force, damage, vp),
            FrameClearPolicy::PartialLoad
        );
    }

    #[test]
    fn push_frame_paint_damage_is_local_not_full_viewport() {
        let _ = take_frame_paint_damage();
        push_frame_paint_damage([10.0, 20.0, 40.0, 24.0]);
        push_frame_paint_damage([30.0, 20.0, 40.0, 24.0]);
        let d = take_frame_paint_damage().expect("local damage");
        let vp = [0.0, 0.0, 800.0, 600.0];
        assert!(!damage_covers_full(vp, d));
        assert_eq!(
            frame_clear_policy(true, false, Some(d), vp),
            FrameClearPolicy::PartialLoad
        );
        assert_eq!(take_frame_paint_damage(), None);
    }

    #[test]
    fn note_frame_clear_policy_counts() {
        reset_frame_policy_counters_for_test();
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        note_frame_clear_policy(FrameClearPolicy::PartialLoad);
        note_frame_clear_policy(FrameClearPolicy::FullClear);
        assert_eq!(partial_load_frame_count(), 2);
        assert_eq!(full_clear_frame_count(), 1);
    }
}
