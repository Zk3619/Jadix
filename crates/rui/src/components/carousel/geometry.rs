//! Carousel product-layer submodule — shares parent symbols via `use super::*`.

use super::drag::is_carousel_dragging_id;
use super::*;

pub(super) fn step_px(ctx: &CarouselCtx) -> f32 {
    if ctx.variant == CarouselVariant::Stack {
        return stack_step_px(ctx.viewport_len.peek());
    }
    if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        return ctx.item_width_px;
    }
    let len = ctx.viewport_len.peek().max(1.0);
    len / ctx.per_view.max(0.25)
}

/// Stack coverflow sideways step (matches card-width formula: viewport × 0.46 × 0.52).
///
/// The upper limit is tightened with the viewport: ensure that the outer edge of the ad=1 side is still within the stage when the scale≈0.82 is stuck.
/// 避免窄屏下 `overflow:hidden` 裁掉圆角。勿再 `clamp(min=56)`——窄视口会被强制过大步长。
pub(super) fn stack_step_px(viewport_w: f32) -> f32 {
    track::stack_step_px(viewport_w)
}

pub(super) fn page_size(ctx: &CarouselCtx) -> usize {
    if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        let vp = ctx.viewport_len.peek().max(1.0);
        return (vp / ctx.item_width_px).floor().max(1.0) as usize;
    }
    // Seamless cloning, etc. still use integer group spacing; full page adsorption/page turning uses [`page_delta_px`] (can contain decimals perView).
    ctx.per_view.floor().max(1.0) as usize
}

/// 整页步进像素。
/// - [`CarouselPageSnap::Peek`]: `floor(perView)` grid, so that the last item of the current page becomes the first item of the next page.
/// - [`CarouselPageSnap::Viewport`]: One screen width (fraction lower ≈ sum of `perView` slots).
pub(super) fn page_delta_px(ctx: &CarouselCtx) -> f32 {
    let step = step_px(ctx);
    match ctx.page_snap {
        CarouselPageSnap::Peek => (page_size(ctx) as f32 * step).max(step),
        CarouselPageSnap::Viewport => {
            if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
                let n = page_size(ctx) as f32;
                return (n * ctx.item_width_px).max(ctx.item_width_px);
            }
            ctx.viewport_len.peek().max(step)
        }
    }
}

pub(super) fn scroll_delta(ctx: &CarouselCtx) -> usize {
    match ctx.scroll_by {
        CarouselScrollBy::Slide => 1,
        CarouselScrollBy::Page => page_size(ctx),
    }
}

/// 首尾是否允许绕回（所有 kind 均尊重 `loop`；Stack 默认 true 但可关）。
pub(super) fn wraps_nav(ctx: &CarouselCtx) -> bool {
    ctx.loop_slides
}

/// Orbital type and `loop && seamless`: clone the head and tail to make a seamless loop.
pub(super) fn track_loops(ctx: &CarouselCtx) -> bool {
    ctx.loop_slides && ctx.seamless && ctx.variant != CarouselVariant::Stack
}

pub(super) fn effective_snap(ctx: &CarouselCtx) -> bool {
    ctx.snap || ctx.autoplay_ms > 0
}

/// The most negative offset allowed when not looping (the last item can be cropped and still partially visible / flush and pasted to the right).
pub(super) fn max_scroll_offset(ctx: &CarouselCtx) -> f32 {
    let n = ctx.count.peek() as f32;
    let step = step_px(ctx);
    let vp = ctx.viewport_len.peek().max(1.0);
    if n <= 0.0 {
        return 0.0;
    }
    if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        return -((n * step) - vp).max(0.0);
    }
    if ctx.variant == CarouselVariant::Multi && ctx.end_align == CarouselEndAlign::Flush {
        return -((n - ctx.per_view).max(0.0) * step);
    }
    // Pad / Single：可滚到最后一格起点。
    let hi = max_index(ctx) as f32;
    -(hi * step)
}

pub(super) fn clamp_offset(ctx: &CarouselCtx, off: f32) -> f32 {
    track::clamp_linear_offset(off, max_scroll_offset(ctx), track_loops(ctx))
}

pub(super) fn loop_clone_count(ctx: &CarouselCtx) -> usize {
    if !track_loops(ctx) {
        return 0;
    }
    let n = ctx.count.peek();
    if n <= 1 {
        return 0;
    }
    let hi = max_index(ctx);
    // The previous one goes from 0→hi: the prefix needs to cover the real item hi; the next one goes from the last item: the suffix needs to be at least one screen.
    let need_prev = n.saturating_sub(hi).max(1);
    let need_view = if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        let vp = ctx.viewport_len.peek().max(1.0);
        (vp / ctx.item_width_px).ceil().max(1.0) as usize
    } else {
        ctx.per_view.ceil().max(1.0) as usize
    };
    need_view.max(scroll_delta(ctx)).max(need_prev).min(n)
}

/// The number of empty slots at the end of Pad mode (allowing the last item to stop on the left side of the viewport and expose the empty space).
pub(super) fn pad_spacer_count(ctx: &CarouselCtx) -> usize {
    // When looping seamlessly, the head and tail clones are used to connect, and blank slots are no longer added.
    if track_loops(ctx) {
        return 0;
    }
    if ctx.variant != CarouselVariant::Multi || ctx.end_align != CarouselEndAlign::Pad {
        return 0;
    }
    ctx.per_view.ceil().max(1.0) as usize - 1
}

pub(super) fn max_index(ctx: &CarouselCtx) -> usize {
    let n = ctx.count.peek();
    if n == 0 {
        return 0;
    }
    match ctx.variant {
        CarouselVariant::Stack | CarouselVariant::Single => n.saturating_sub(1),
        CarouselVariant::Multi => {
            // When making a seamless loop, each frame can be used as a starting point; flush/pad/fixed only constrains non-cyclic scrolling.
            if track_loops(ctx) {
                return n.saturating_sub(1);
            }
            if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
                let step = ctx.item_width_px;
                let vp = ctx.viewport_len.peek().max(1.0);
                let max_off = ((n as f32) * step - vp).max(0.0);
                return (max_off / step).ceil().min((n.saturating_sub(1)) as f32) as usize;
            }
            match ctx.end_align {
                // The last item can be used as the starting point of the viewport (right next to the empty slot/empty area).
                CarouselEndAlign::Pad => n.saturating_sub(1),
                // The last item snaps to the right edge of the viewport: scrolls up to (n - per_view).
                CarouselEndAlign::Flush => {
                    let room = (n as f32 - ctx.per_view).max(0.0);
                    (room.ceil() as usize).min(n.saturating_sub(1))
                }
            }
        }
    }
}

pub(super) fn ideal_offset(ctx: &CarouselCtx) -> f32 {
    let step = step_px(ctx);
    let cc = loop_clone_count(ctx);
    let hi = max_index(ctx);
    let min_off = max_scroll_offset(ctx);
    track::ideal_offset(track::IdealOffsetInput {
        index: ctx.index.peek(),
        max_index: hi,
        step,
        clone_count: cc,
        page_delta: page_delta_px(ctx),
        is_stack: ctx.variant == CarouselVariant::Stack,
        multi_page_linear: ctx.variant == CarouselVariant::Multi
            && ctx.scroll_by == CarouselScrollBy::Page
            && !track_loops(ctx),
        track_loops: track_loops(ctx),
        wraps_nav: wraps_nav(ctx),
        min_offset: min_off,
    })
}

pub(super) fn sync_offset_target(ctx: &CarouselCtx) {
    if is_carousel_dragging_id(&ctx.id) || loop_anim(&ctx.id).is_some() {
        return;
    }
    // 视口未测到真实宽度时 step≈1，会把目标拉到约 0（露出前缀克隆）。
    if track_loops(ctx) && ctx.viewport_len.peek() <= 1.0 {
        return;
    }
    let ideal = ideal_offset(ctx);
    let sig = offset_target(&ctx.id);
    if (sig.peek() - ideal).abs() < 0.5 {
        return;
    }
    sig.set(ideal);
}

/// Tracks are automatically aligned every frame: only `snap` mode pulls the offset back to the index grid point.
/// When `snap=false`, the offset is authoritative (it cannot be pulled back by the nearest index after being freely dragged and docked).
pub(super) fn auto_sync_offset_if_snapping(ctx: &CarouselCtx) {
    if !effective_snap(ctx) {
        return;
    }
    sync_offset_target(ctx);
}

/// The nearest index is derived from the current orbit offset (for dots/disabled wraparound button state).
pub(super) fn index_from_offset(ctx: &CarouselCtx, off: f32) -> usize {
    snap::index_from_offset(
        off,
        step_px(ctx),
        loop_clone_count(ctx),
        ctx.count.peek(),
        max_index(ctx),
        wraps_nav(ctx) && track_loops(ctx),
    )
}

/// `snap=false`: Step relative to the current offset (do not snap to the grid point first and then move).
pub(super) fn scroll_by_free_offset(ctx: &CarouselCtx, dir: i32) {
    let unit = if ctx.variant == CarouselVariant::Multi && ctx.scroll_by == CarouselScrollBy::Page {
        page_delta_px(ctx).max(step_px(ctx))
    } else {
        step_px(ctx) * scroll_delta(ctx).max(1) as f32
    };
    let cur = offset_target(&ctx.id).peek();
    // dir>0 → previous (offset increases); dir<0 → next (more negative).
    let raw = cur + dir as f32 * unit;
    let next = if track_loops(ctx) {
        let n = ctx.count.peek().max(1);
        wrap_loop_offset(raw, n, loop_clone_count(ctx), step_px(ctx))
    } else {
        clamp_offset(ctx, raw)
    };
    ctx.index.set(index_from_offset(ctx, next));
    animate_offset_to(&ctx.id, next);
}

/// Multi + `scrollBy=page`: Scroll and adsorb according to the entire page (viewport/`perView`).
pub(super) fn scroll_by_page(ctx: &CarouselCtx, dir: i32) {
    let page = page_delta_px(ctx).max(step_px(ctx));
    let cur = offset_target(&ctx.id).peek();
    let cur_page = (-cur / page).round().max(0.0);
    let target_page = if dir > 0 {
        (cur_page - 1.0).max(0.0)
    } else {
        cur_page + 1.0
    };
    let target = clamp_offset(ctx, -target_page * page);
    ctx.index.set(index_from_offset(ctx, target));
    animate_offset_to(&ctx.id, target);
}

/// Seamless track: fold the free offset back to the real band `(-(cc+n)*step, -cc*step]` to avoid stopping in the head and tail clone area.
pub(super) fn wrap_loop_offset(off: f32, n: usize, cc: usize, step: f32) -> f32 {
    if n == 0 || cc == 0 || step <= 1.0 {
        return off;
    }
    let period = n as f32 * step;
    let start = -(cc as f32) * step;
    let end = start - period;
    let mut o = off;
    while o > start {
        o -= period;
    }
    while o <= end {
        o += period;
    }
    o
}

/// Viewport width: only update the length used by step; target translate is followed by sync_offset_target / CSS.
/// Snap the first measurement immediately to avoid playing a false animation from 0.
pub(super) fn apply_viewport_len(ctx: &CarouselCtx, new_len: f32) -> bool {
    if new_len <= 1.0 {
        return false;
    }
    let prev = ctx.viewport_len.peek();
    let thresh = (prev.abs() * 0.01).max(2.0);
    if prev > 1.0 && (prev - new_len).abs() <= thresh {
        return false;
    }

    ctx.viewport_len.set(new_len);

    if is_carousel_dragging_id(&ctx.id) {
        return true;
    }
    // Seamless segment: only change the step, do not change the target (otherwise the clone animation will be interrupted).
    if loop_anim(&ctx.id).is_some() {
        return true;
    }
    // Free drag: retain the current offset (fraction is scaled according to the viewport ratio), only clamp/wrap, and do not pull back the index grid point.
    if !effective_snap(ctx) {
        if prev <= 1.0 {
            snap_offset_to(&ctx.id, ideal_offset(ctx));
        } else {
            let cur = offset_target(&ctx.id).peek();
            let scaled = if ctx.item_sizing == CarouselItemSizing::Fixed {
                cur
            } else {
                cur * (new_len / prev)
            };
            let parked = if track_loops(ctx) {
                let n = ctx.count.peek().max(1);
                wrap_loop_offset(scaled, n, loop_clone_count(ctx), step_px(ctx))
            } else {
                clamp_offset(ctx, scaled)
            };
            offset_target(&ctx.id).set(parked);
        }
        return true;
    }
    let ideal = ideal_offset(ctx);
    if prev <= 1.0 {
        snap_offset_to(&ctx.id, ideal);
    } else {
        // Idle/turning page: Change target, CSS transition retarget from current visual value (no hard snap).
        offset_target(&ctx.id).set(ideal);
    }
    true
}

pub(super) fn maybe_finish_loop_snap(ctx: &CarouselCtx, visual: f32) {
    let Some(anim) = loop_anim(&ctx.id) else {
        return;
    };
    if loop_reached(visual, anim.target_offset, 1.0) {
        finish_loop_anim(&ctx.id, ctx.index);
    }
}

/// Cross-border "next": From `from` and then forward `delta` space to enter the suffix clone, and then teleport to `(from+delta)%n`.
/// Switch to settle_index at the beginning to synchronize the dots with the target page; the track is still sliding towards the clone slot.
pub(super) fn begin_seamless_next(ctx: &CarouselCtx, from: usize, delta: usize) -> bool {
    if !track_loops(ctx) {
        return false;
    }
    let n = ctx.count.peek();
    let cc = loop_clone_count(ctx);
    let d = delta.max(1);
    if n <= 1 || cc == 0 || d > cc {
        return false;
    }
    let step = step_px(ctx);
    let settle_index = (from + d) % n;
    let target = -((from + cc + d) as f32) * step;
    let settle = -((settle_index + cc) as f32) * step;
    set_loop_anim(
        &ctx.id,
        LoopAnim {
            target_offset: target,
            settle_index,
            settle_offset: settle,
        },
    );
    // dots / index of external bind: aligned with the target page (don’t wait for the clone animation to end).
    ctx.index.set(settle_index);
    animate_offset_to(&ctx.id, target);
    true
}

/// Cross-border "previous": From `from` and then back `delta` space to enter the prefix clone, and then teleport to `(from+n-delta)%n`.
pub(super) fn begin_seamless_prev(ctx: &CarouselCtx, from: usize, delta: usize) -> bool {
    if !track_loops(ctx) {
        return false;
    }
    let n = ctx.count.peek();
    let cc = loop_clone_count(ctx);
    let d = delta.max(1);
    if n <= 1 || cc == 0 || d > cc || from + cc < d {
        return false;
    }
    let step = step_px(ctx);
    let settle_index = (from + n - d) % n;
    let target = -((from + cc - d) as f32) * step;
    let settle = -((settle_index + cc) as f32) * step;
    set_loop_anim(
        &ctx.id,
        LoopAnim {
            target_offset: target,
            settle_index,
            settle_offset: settle,
        },
    );
    ctx.index.set(settle_index);
    animate_offset_to(&ctx.id, target);
    true
}
