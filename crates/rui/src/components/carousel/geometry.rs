//! Carousel 产品层子模块 — 经 `use super::*` 共享父模块符号。

use super::*;
use super::drag::is_carousel_dragging_id;


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

/// Stack coverflow 侧移步长（与卡宽公式一致：视口 × 0.46 × 0.52）。
///
/// 上限随视口收紧：保证 ad=1 侧卡在 scale≈0.82 时外缘仍落在舞台内，
/// 避免窄屏下 `overflow:hidden` 裁掉圆角。勿再 `clamp(min=56)`——窄视口会被强制过大步长。
pub(super) fn stack_step_px(viewport_w: f32) -> f32 {
    track::stack_step_px(viewport_w)
}

pub(super) fn page_size(ctx: &CarouselCtx) -> usize {
    if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        let vp = ctx.viewport_len.peek().max(1.0);
        return (vp / ctx.item_width_px).floor().max(1.0) as usize;
    }
    // 无缝克隆等仍用整数组距；整页吸附/翻页用 [`page_delta_px`]（可含小数 perView）。
    ctx.per_view.floor().max(1.0) as usize
}

/// 整页步进像素。
/// - [`CarouselPageSnap::Peek`]：`floor(perView)` 格，使当页裁切尾项成为下页首项。
/// - [`CarouselPageSnap::Viewport`]：一屏宽（fraction 下 ≈ `perView` 槽之和）。
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

/// 轨道型且 `loop && seamless`：首尾克隆做无缝循环。
pub(super) fn track_loops(ctx: &CarouselCtx) -> bool {
    ctx.loop_slides && ctx.seamless && ctx.variant != CarouselVariant::Stack
}

pub(super) fn effective_snap(ctx: &CarouselCtx) -> bool {
    ctx.snap || ctx.autoplay_ms > 0
}

/// 非循环时允许的最负 offset（末项可裁切仍部分可见 / flush 贴右）。
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
    // 上一从 0→hi：前缀需覆盖真实项 hi；下一从末项越过：后缀至少一屏。
    let need_prev = n.saturating_sub(hi).max(1);
    let need_view = if ctx.item_sizing == CarouselItemSizing::Fixed && ctx.item_width_px > 1.0 {
        let vp = ctx.viewport_len.peek().max(1.0);
        (vp / ctx.item_width_px).ceil().max(1.0) as usize
    } else {
        ctx.per_view.ceil().max(1.0) as usize
    };
    need_view.max(scroll_delta(ctx)).max(need_prev).min(n)
}

/// Pad 模式尾部空白槽数量（让末项可停在视口左侧并露出空区）。
pub(super) fn pad_spacer_count(ctx: &CarouselCtx) -> usize {
    // 无缝循环时用首尾克隆衔接，不再追加空白槽。
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
            // 无缝环形时每张都可作为起点；flush/pad/fixed 只约束非循环滚动。
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
                // 末项可作为视口起点（右侧靠空白槽 / 空区）。
                CarouselEndAlign::Pad => n.saturating_sub(1),
                // 末项贴齐视口右缘：最多滚到 (n - per_view)。
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

/// 轨道每帧自动对齐：仅 `snap` 模式把 offset 拉回 index 格点。
/// `snap=false` 时 offset 为权威（自由拖拽停靠后不得被 nearest index 拉回）。
pub(super) fn auto_sync_offset_if_snapping(ctx: &CarouselCtx) {
    if !effective_snap(ctx) {
        return;
    }
    sync_offset_target(ctx);
}

/// 由当前轨道 offset 推导 nearest index（供 dots / 禁绕回按钮态）。
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

/// `snap=false`：相对当前 offset 步进（不先吸附到格点再走）。
pub(super) fn scroll_by_free_offset(ctx: &CarouselCtx, dir: i32) {
    let unit = if ctx.variant == CarouselVariant::Multi && ctx.scroll_by == CarouselScrollBy::Page {
        page_delta_px(ctx).max(step_px(ctx))
    } else {
        step_px(ctx) * scroll_delta(ctx).max(1) as f32
    };
    let cur = offset_target(&ctx.id).peek();
    // dir>0 → 上一（offset 增大）；dir<0 → 下一（更负）。
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

/// Multi + `scrollBy=page`：按整页（视口/`perView`）翻页并吸附。
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

/// 无缝轨道：把自由 offset 折回真实带 `(-(cc+n)*step, -cc*step]`，避免停在首尾克隆区。
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

/// 视口宽度：只更新 step 用的长度；目标 translate 由 sync_offset_target / CSS 跟。
/// 首次测量立刻 snap，避免从 0 播一段假动画。
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
    // 无缝段：只改 step，勿改目标（否则克隆动画被打断）。
    if loop_anim(&ctx.id).is_some() {
        return true;
    }
    // 自由拖拽：保留当前 offset（fraction 按视口比缩放），仅 clamp/wrap，不拉回 index 格点。
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
        // 空闲/翻页中：改目标，CSS transition 从当前视觉值 retarget（无硬 snap）。
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

/// 跨界「下一」：从 `from` 再前进 `delta` 格进入后缀克隆，再瞬移到 `(from+delta)%n`。
/// 一开始就切到 settle_index，让 dots 与目标页同步；轨道仍滑向克隆槽。
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
    // dots / 外部 bind 的 index：与目标页对齐（勿等克隆动画结束）。
    ctx.index.set(settle_index);
    animate_offset_to(&ctx.id, target);
    true
}

/// 跨界「上一」：从 `from` 再后退 `delta` 格进入前缀克隆，再瞬移到 `(from+n-delta)%n`。
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

