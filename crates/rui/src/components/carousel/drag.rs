//! Carousel product-layer submodule — shares parent symbols via `use super::*`.

use std::cell::Cell;

use super::geometry::*;
use super::*;

// ── Drag ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct CarouselDrag {
    pub(super) id: String,
    pub(super) start_ptr: f32,
    pub(super) base_offset: f32,
    pub(super) index: Signal<usize>,
    pub(super) count: Signal<usize>,
    pub(super) per_view: f32,
    pub(super) wraps: bool,
    pub(super) loop_clones: usize,
    pub(super) end_align: CarouselEndAlign,
    pub(super) variant: CarouselVariant,
    pub(super) item_sizing: CarouselItemSizing,
    pub(super) item_width_px: f32,
    pub(super) scroll_by: CarouselScrollBy,
    pub(super) page_snap: CarouselPageSnap,
    pub(super) snap: bool,
    pub(super) viewport_len: Signal<f32>,
    pub(super) drag_px: Signal<f32>,
    pub(super) orientation: CarouselOrientation,
    pub(super) autoplay_ms: u64,
}

thread_local! {
    static CAROUSEL_DRAG: RefCell<Option<CarouselDrag>> = const { RefCell::new(None) };
    static AUTOPLAY_REG: RefCell<HashMap<String, CarouselCtx>> = RefCell::new(HashMap::new());
    /// Incremented once per `App::render` before reactive_sync. Offscreen skip means no stamp
    /// this frame → autoplay must not Advance (and must not busy-wake the event loop).
    static PAINT_FRAME: Cell<u64> = const { Cell::new(0) };
    static LAST_PAINT_FRAME: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
    static LAST_TICK_VISIBLE: RefCell<HashMap<String, bool>> = RefCell::new(HashMap::new());
}

pub(super) fn begin_carousel_drag(drag: CarouselDrag) {
    // When dragging to interrupt a seamless segment, you must settle, and you cannot just drop the flag (otherwise the spring will stop halfway and the target will remain).
    finish_loop_anim(&drag.id, drag.index);
    clear_autoplay_deadline(&drag.id, drag.autoplay_ms);
    crate::utils::pointer_drag::begin(
        crate::utils::pointer_drag::DragKind::Carousel,
        drag.id.clone(),
        0.0,
        0.0,
    );
    CAROUSEL_DRAG.with(|d| *d.borrow_mut() = Some(drag));
}

pub(super) fn drag_step_px(drag: &CarouselDrag) -> f32 {
    if drag.variant == CarouselVariant::Stack {
        return stack_step_px(drag.viewport_len.peek());
    }
    if drag.item_sizing == CarouselItemSizing::Fixed && drag.item_width_px > 1.0 {
        return drag.item_width_px;
    }
    let len = drag.viewport_len.peek().max(1.0);
    len / drag.per_view.max(0.25)
}

pub(super) fn drag_page_delta_px(drag: &CarouselDrag) -> f32 {
    let step = drag_step_px(drag);
    match drag.page_snap {
        CarouselPageSnap::Peek => {
            let n = if drag.item_sizing == CarouselItemSizing::Fixed && drag.item_width_px > 1.0 {
                let vp = drag.viewport_len.peek().max(1.0);
                (vp / drag.item_width_px).floor().max(1.0)
            } else {
                drag.per_view.floor().max(1.0)
            };
            (n * step).max(step)
        }
        CarouselPageSnap::Viewport => {
            if drag.item_sizing == CarouselItemSizing::Fixed && drag.item_width_px > 1.0 {
                let vp = drag.viewport_len.peek().max(1.0);
                let n = (vp / drag.item_width_px).floor().max(1.0);
                return (n * drag.item_width_px).max(drag.item_width_px);
            }
            drag.viewport_len.peek().max(step)
        }
    }
}

pub(super) fn drag_max_index(drag: &CarouselDrag) -> usize {
    let n = drag.count.peek();
    if n == 0 {
        return 0;
    }
    match drag.variant {
        CarouselVariant::Stack | CarouselVariant::Single => n.saturating_sub(1),
        CarouselVariant::Multi => {
            if drag.loop_clones > 0 {
                return n.saturating_sub(1);
            }
            if drag.item_sizing == CarouselItemSizing::Fixed && drag.item_width_px > 1.0 {
                let step = drag.item_width_px;
                let vp = drag.viewport_len.peek().max(1.0);
                let max_off = ((n as f32) * step - vp).max(0.0);
                return (max_off / step).ceil().min((n.saturating_sub(1)) as f32) as usize;
            }
            match drag.end_align {
                CarouselEndAlign::Pad => n.saturating_sub(1),
                CarouselEndAlign::Flush => {
                    let room = (n as f32 - drag.per_view).max(0.0);
                    (room.ceil() as usize).min(n.saturating_sub(1))
                }
            }
        }
    }
}

pub(super) fn drag_clamp_offset(drag: &CarouselDrag, off: f32) -> f32 {
    if drag.loop_clones > 0 || (drag.variant == CarouselVariant::Stack && drag.wraps) {
        return off;
    }
    let n = drag.count.peek() as f32;
    let step = drag_step_px(drag);
    let vp = drag.viewport_len.peek().max(1.0);
    let min_off = if drag.item_sizing == CarouselItemSizing::Fixed && drag.item_width_px > 1.0 {
        -((n * step) - vp).max(0.0)
    } else if drag.variant == CarouselVariant::Multi && drag.end_align == CarouselEndAlign::Flush {
        -((n - drag.per_view).max(0.0) * step)
    } else {
        let hi = drag_max_index(drag) as f32;
        -(hi * step)
    };
    off.clamp(min_off, 0.0)
}

/// 由视觉 offset 得到 nearest 逻辑 index（拖拽松手吸附用）。
pub(super) fn drag_nearest_index(
    drag: &CarouselDrag,
    visual: f32,
    step: f32,
    cc: usize,
    hi: usize,
) -> usize {
    let n = drag.count.peek().max(1);
    if step <= 1.0 {
        return drag.index.peek().min(hi);
    }
    let logical = ((-visual / step).round() as i32) - cc as i32;
    // Stack has no clone band, but loop should still be chosen nearest according to the loop shape.
    if drag.wraps && (cc > 0 || drag.variant == CarouselVariant::Stack) {
        let n_i = n as i32;
        let wrapped = ((logical % n_i) + n_i) % n_i;
        (wrapped as usize).min(hi)
    } else {
        logical.clamp(0, hi as i32) as usize
    }
}

/// 吸附目标：以视觉位置为准（过半格跟 nearest）；仅「几乎未动且仍在起点半格内」才钉回按下时的 index。
pub(super) fn resolve_snap_index(
    drag: &CarouselDrag,
    visual: f32,
    step: f32,
    cc: usize,
    hi: usize,
) -> usize {
    let start = drag.index.peek().min(hi);
    let nearest = drag_nearest_index(drag, visual, step, cc, hi);
    let start_off = -((start + cc) as f32) * step;
    let half = step.max(1.0) * 0.5;
    let in_home_half = (visual - start_off).abs() < half;
    let tiny_move = drag.drag_px.peek().abs() <= DRAG_THRESHOLD_PX;
    if tiny_move && in_home_half {
        start
    } else {
        nearest
    }
}

/// Seamless + adsorption and release: adsorb according to the nearest **track grid** (can include head and tail clones) visually.
/// 若目标落在克隆槽，走 [`LoopAnim`] 再瞬移回真实带，避免尾项→首项反向长滚。
pub(super) fn end_seamless_snap_drag(
    drag: &CarouselDrag,
    visual: f32,
    step: f32,
    n: usize,
    cc: usize,
    hi: usize,
) {
    let start = drag.index.peek().min(hi);
    let start_off = -((start + cc) as f32) * step;
    let half = step.max(1.0) * 0.5;
    let tiny_move = drag.drag_px.peek().abs() <= DRAG_THRESHOLD_PX;
    let home = tiny_move && (visual - start_off).abs() < half;

    let (track_i, idx) = if home || step <= 1.0 {
        ((start + cc) as i32, start)
    } else {
        let track_i = (-visual / step).round() as i32;
        let n_i = n as i32;
        let logical = track_i - cc as i32;
        let wrapped = ((logical % n_i) + n_i) % n_i;
        (track_i, (wrapped as usize).min(hi))
    };

    let target_off = -(track_i as f32) * step;
    let settle_off = -((idx + cc) as f32) * step;
    let in_clone = track_i < cc as i32 || track_i >= (cc + n) as i32;

    drag.index.set(idx);
    drag.drag_px.set(0.0);

    if in_clone && (target_off - settle_off).abs() > 0.5 {
        set_loop_anim(
            &drag.id,
            LoopAnim {
                target_offset: target_off,
                settle_index: idx,
                settle_offset: settle_off,
            },
        );
        animate_offset_from(&drag.id, visual, target_off);
    } else {
        animate_offset_from(&drag.id, visual, settle_off);
    }
    restart_autoplay_deadline(&drag.id, drag.autoplay_ms);
}

pub(super) fn end_carousel_drag() {
    crate::utils::pointer_drag::clear();
    CAROUSEL_DRAG.with(|d| {
        if let Some(drag) = d.borrow_mut().take() {
            let step = drag_step_px(&drag);
            let visual = drag.base_offset + drag.drag_px.peek();
            let n = drag.count.peek().max(1);
            let hi = drag_max_index(&drag);
            let cc = drag.loop_clones;

            // 不吸附：钉在当前视觉 offset（clamp/wrap），index 取 nearest 供 dots。
            if !drag.snap {
                let parked = if cc > 0 {
                    wrap_loop_offset(visual, n, cc, step)
                } else {
                    drag_clamp_offset(&drag, visual)
                };
                let track_i = if step > 1.0 {
                    (-parked / step).round() as i32
                } else {
                    cc as i32
                };
                let logical = track_i - cc as i32;
                let idx = if drag.wraps && (cc > 0 || drag.variant == CarouselVariant::Stack) {
                    let n_i = n as i32;
                    let wrapped = ((logical % n_i) + n_i) % n_i;
                    (wrapped as usize).min(hi)
                } else {
                    logical.clamp(0, hi as i32) as usize
                };
                drag.index.set(idx);
                drag.drag_px.set(0.0);
                snap_offset_to(&drag.id, parked);
                restart_autoplay_deadline(&drag.id, drag.autoplay_ms);
                return;
            }

            // Multi + page: Adsorb to the entire page; the target is based on the visual page (more than half of the page and nearest).
            if drag.variant == CarouselVariant::Multi
                && drag.scroll_by == CarouselScrollBy::Page
                && cc == 0
            {
                let page = drag_page_delta_px(&drag).max(step);
                let base_page = (-drag.base_offset / page).round().max(0.0);
                let nearest_page = (-visual / page).round().max(0.0);
                let home = -base_page * page;
                let in_home_half = (visual - home).abs() < page * 0.5;
                let tiny_move = drag.drag_px.peek().abs() <= DRAG_THRESHOLD_PX;
                let target_page = if tiny_move && in_home_half {
                    base_page
                } else {
                    nearest_page
                };
                let ideal = drag_clamp_offset(&drag, -target_page * page);
                let idx = if step > 1.0 {
                    ((-ideal / step).round() as i32).clamp(0, hi as i32) as usize
                } else {
                    0
                };
                drag.index.set(idx.min(hi));
                drag.drag_px.set(0.0);
                animate_offset_from(&drag.id, visual, ideal);
                restart_autoplay_deadline(&drag.id, drag.autoplay_ms);
                return;
            }

            // Seamless adsorption: You can stop at the clone grid and then loop_anim back to the real zone (do not animate directly to the opposite real slot).
            if cc > 0 {
                end_seamless_snap_drag(&drag, visual, step, n, cc, hi);
                return;
            }

            let idx = resolve_snap_index(&drag, visual, step, cc, hi);
            drag.index.set(idx);
            drag.drag_px.set(0.0);
            let ideal = {
                let raw_off = ((idx + cc) as f32) * step;
                if drag.variant == CarouselVariant::Stack && drag.wraps {
                    // Ring: ideal can cross the linear clamp (consistent with ideal_offset).
                    -raw_off
                } else {
                    drag_clamp_offset(&drag, -raw_off)
                }
            };
            animate_offset_from(&drag.id, visual, ideal);
            restart_autoplay_deadline(&drag.id, drag.autoplay_ms);
        }
    });
    crate::app::release_pointer_capture();
}

pub(super) fn is_carousel_dragging_id(id: &str) -> bool {
    CAROUSEL_DRAG.with(|d| d.borrow().as_ref().map(|x| x.id == id).unwrap_or(false))
}

/// CursorMoved：更新拖拽位移。
pub(crate) fn tick_carousel_drag(x: f32, y: f32) -> bool {
    CAROUSEL_DRAG.with(|d| {
        let mut slot = d.borrow_mut();
        let Some(drag) = slot.as_mut() else {
            return false;
        };
        let ptr = match drag.orientation {
            CarouselOrientation::Horizontal => x,
            CarouselOrientation::Vertical => y,
        };
        let delta = ptr - drag.start_ptr;
        if (drag.drag_px.peek() - delta).abs() > 0.25 {
            drag.drag_px.set(delta);
            let raw = drag.base_offset + delta;
            let next = if drag.loop_clones > 0 {
                // Dragging out of the clone cycle will result in a white screen: fold back to the real band. If you release the adsorption and still stop at the halfway point of cloning,
                // Use end_seamless_snap_drag to walk the LoopAnim instead of rolling directly to the opposite real slot.
                let n = drag.count.peek().max(1);
                let step = drag_step_px(drag);
                let wrapped = wrap_loop_offset(raw, n, drag.loop_clones, step);
                if (wrapped - raw).abs() > 0.5 {
                    drag.base_offset = wrapped - delta;
                }
                wrapped
            } else {
                drag_clamp_offset(drag, raw)
            };
            snap_offset_to(&drag.id, next);
        }
        true
    })
}

pub(crate) fn clear_carousel_drag_if_any() {
    let had = CAROUSEL_DRAG.with(|d| d.borrow().is_some());
    if had {
        end_carousel_drag();
    }
}

#[allow(dead_code)]
pub(crate) fn is_carousel_dragging() -> bool {
    CAROUSEL_DRAG.with(|d| d.borrow().is_some())
}

/// 自动轮播推进；有切换时返回 `true`（需重绘）。
pub(crate) fn tick_carousel_autoplay() -> bool {
    let now = Instant::now();
    let mut advanced = false;
    AUTOPLAY_REG.with(|reg| {
        for ctx in reg.borrow().values() {
            let visible = carousel_painted_this_frame(&ctx.id);
            let was = last_tick_visible(&ctx.id);
            set_last_tick_visible(&ctx.id, visible);
            let inner = decide_autoplay(
                ctx.autoplay_ms,
                ctx.paused.peek(),
                is_carousel_dragging_id(&ctx.id),
                loop_anim(&ctx.id).is_some(),
                now,
                next_autoplay_at(&ctx.id),
            );
            match gate_autoplay_visibility(visible, visible && !was, inner) {
                AutoplayAction::Skip => {}
                AutoplayAction::FinishLoop => {
                    maybe_finish_loop_snap(ctx, last_visual_offset(&ctx.id));
                    advanced = true;
                }
                AutoplayAction::ArmDeadline => bump_autoplay(ctx),
                AutoplayAction::Advance => {
                    scroll_next(ctx);
                    advanced = true;
                }
            }
        }
    });
    advanced
}

/// 下一次自动轮播唤醒时刻。离屏实例不参与，避免逾期 deadline 把事件循环空转。
pub(crate) fn carousel_next_wake_at() -> Option<Instant> {
    AUTOPLAY_REG.with(|reg| {
        reg.borrow()
            .values()
            .filter_map(|c| {
                if c.autoplay_ms == 0
                    || c.paused.peek()
                    || is_carousel_dragging_id(&c.id)
                    || !carousel_painted_this_frame(&c.id)
                {
                    None
                } else {
                    next_autoplay_at(&c.id)
                }
            })
            .min()
    })
}

pub(super) fn clear_carousel_drag_for_id(id: &str) {
    CAROUSEL_DRAG.with(|d| {
        let same = d.borrow().as_ref().map(|x| x.id == id).unwrap_or(false);
        if same {
            *d.borrow_mut() = None;
            crate::app::release_pointer_capture();
        }
    });
}

pub(super) fn register_autoplay(ctx: &CarouselCtx) {
    if ctx.autoplay_ms == 0 {
        AUTOPLAY_REG.with(|r| {
            r.borrow_mut().remove(&ctx.id);
        });
        return;
    }
    bump_autoplay(ctx);
    AUTOPLAY_REG.with(|r| {
        r.borrow_mut().insert(ctx.id.clone(), ctx.clone());
    });
}

pub(super) fn unregister_autoplay(id: &str) {
    AUTOPLAY_REG.with(|r| {
        r.borrow_mut().remove(id);
    });
    LAST_PAINT_FRAME.with(|m| {
        m.borrow_mut().remove(id);
    });
    LAST_TICK_VISIBLE.with(|m| {
        m.borrow_mut().remove(id);
    });
}

/// Call once at the start of each `App::render`, before carousel `reactive_sync`.
pub(crate) fn begin_carousel_paint_frame() {
    PAINT_FRAME.with(|c| c.set(c.get().wrapping_add(1)));
}

pub(super) fn stamp_carousel_painted(id: &str) {
    let frame = PAINT_FRAME.with(|c| c.get());
    LAST_PAINT_FRAME.with(|m| {
        m.borrow_mut().insert(id.to_string(), frame);
    });
}

fn carousel_painted_this_frame(id: &str) -> bool {
    let frame = PAINT_FRAME.with(|c| c.get());
    LAST_PAINT_FRAME.with(|m| m.borrow().get(id).copied() == Some(frame))
}

fn last_tick_visible(id: &str) -> bool {
    LAST_TICK_VISIBLE.with(|m| m.borrow().get(id).copied().unwrap_or(false))
}

fn set_last_tick_visible(id: &str, visible: bool) {
    LAST_TICK_VISIBLE.with(|m| {
        m.borrow_mut().insert(id.to_string(), visible);
    });
}
