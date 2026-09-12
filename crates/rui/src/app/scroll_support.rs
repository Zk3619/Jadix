use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::core::tree::Element;

/// 打开下拉后把某一项滚进视口（布局写出 content/viewport 后再应用，避免菜单 effect 订阅滚动信号）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollItemAlign {
    Start,
    Center,
}

#[derive(Clone, Debug)]
struct PendingScrollItemAlign {
    index: usize,
    item_count: usize,
    align: ScrollItemAlign,
    tries: u8,
    requested_at: Instant,
}

thread_local! {
    static PENDING_SCROLL_ITEM_ALIGN: RefCell<HashMap<String, PendingScrollItemAlign>> =
        RefCell::new(HashMap::new());
}

pub(crate) fn scroll_offset_to_center_item(
    index: usize,
    item_h: f32,
    viewport_h: f32,
    item_count: usize,
) -> f32 {
    let content_h = item_count as f32 * item_h;
    let max_sy = (content_h - viewport_h).max(0.0);
    let center = index as f32 * item_h + item_h * 0.5;
    (center - viewport_h * 0.5).clamp(0.0, max_sy)
}

pub(crate) fn request_scroll_item_into_view(
    rest: impl Into<String>,
    index: usize,
    item_count: usize,
    align: ScrollItemAlign,
) {
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        m.borrow_mut().insert(
            rest.into(),
            PendingScrollItemAlign {
                index,
                item_count: item_count.max(1),
                align,
                tries: 0,
                requested_at: Instant::now(),
            },
        );
    });
}

pub(crate) fn cancel_scroll_item_into_view(rest: &str) {
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        m.borrow_mut().remove(rest);
    });
}

pub(crate) fn has_pending_scroll_item_align() -> bool {
    const PENDING_ALIGN_MAX_AGE: Duration = Duration::from_secs(3);
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        let mut map = m.borrow_mut();
        let now = Instant::now();
        // If a rest key disappeared (page switch / window close), stale pending entries
        // would otherwise keep the redraw pump alive forever.
        map.retain(|_, v| now.duration_since(v.requested_at) <= PENDING_ALIGN_MAX_AGE);
        !map.is_empty()
    })
}

/// Aligned with real `content_h` / `viewport_h` after layout. The request is retained when the metric is not ready.
pub(crate) fn apply_pending_scroll_item_into_view(
    rest: &str,
    viewport_h: f32,
    content_h: f32,
) -> bool {
    let pending = PENDING_SCROLL_ITEM_ALIGN.with(|m| m.borrow_mut().remove(rest));
    let Some(mut pending) = pending else {
        return false;
    };
    if viewport_h <= 1.0 || content_h <= 1.0 {
        pending.tries = pending.tries.saturating_add(1);
        if pending.tries < 12 {
            PENDING_SCROLL_ITEM_ALIGN.with(|m| {
                m.borrow_mut().insert(rest.to_string(), pending);
            });
        }
        return false;
    }
    let item_h = content_h / pending.item_count as f32;
    let sy = match pending.align {
        ScrollItemAlign::Center => {
            scroll_offset_to_center_item(pending.index, item_h, viewport_h, pending.item_count)
        }
        ScrollItemAlign::Start => {
            let max_sy = (content_h - viewport_h).max(0.0);
            (pending.index as f32 * item_h).clamp(0.0, max_sy)
        }
    };
    set_scroll_offset_with_behavior(rest, 0.0, sy, true);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollAxis {
    X,
    Y,
}

#[derive(Clone, Debug)]
pub(super) struct ScrollbarDragState {
    pub(super) rest: String,
    pub(super) axis: ScrollAxis,
    pub(super) track_start: f32,
    pub(super) track_len: f32,
    pub(super) thumb_len: f32,
    pub(super) max_offset: f32,
    pub(super) grab_offset: f32,
}

/// Primary-pointer content pan over a scroll viewport (touch; optional mouse → fling).
///
/// Mouse arming is gated by [`crate::app::mouse_content_pan_enabled`] (default off).
#[derive(Clone, Debug)]
pub(super) struct ContentScrollDrag {
    pub(super) rest: String,
    pub(super) origin_x: f32,
    pub(super) origin_y: f32,
    pub(super) last_x: f32,
    pub(super) last_y: f32,
    pub(super) last_t: std::time::Instant,
    /// Offset-space velocity (px/s): positive increases scroll offset.
    pub(super) vel: [f32; 2],
    /// True once movement exceeds the arm threshold (avoids fighting taps).
    pub(super) armed: bool,
}

/// Exponential friction decay for fling inertia (`vx`/`vy` in px/s of scroll offset).
///
/// `friction_per_sec` ≈ how many e-folds of speed drop per second (higher = stops sooner).
pub(super) fn decay_scroll_velocity(vx: f32, vy: f32, dt: f32, friction_per_sec: f32) -> [f32; 2] {
    let dt = dt.max(0.0);
    let damp = (-friction_per_sec * dt).exp();
    let mut nx = vx * damp;
    let mut ny = vy * damp;
    const STOP_PX_PER_SEC: f32 = 12.0;
    if nx.abs() < STOP_PX_PER_SEC {
        nx = 0.0;
    }
    if ny.abs() < STOP_PX_PER_SEC {
        ny = 0.0;
    }
    [nx, ny]
}

/// Quantize a logical scroll offset to whole device pixels so glyphs, synthetic-bold
/// shadows, and page fills move on the same grid (avoids chase-tail jitter).
pub(super) fn snap_scroll_offset_to_device_px(offset: f32, ui_scale: f32) -> f32 {
    let s = ui_scale.max(0.01);
    (offset * s).round() / s
}

/// Ease `current` toward `target` on the device-pixel grid.
///
/// A hidden subpixel phase + `round()` for paint made the tail chatter against
/// glyphon / synthetic-bold (inertia jitter). The last 16 device pixels are a
/// regular 1px staircase — no leftover phase, and never dump 2px after a stall.
/// Returns `(paint_offset, still_animating)`.
pub(super) fn smooth_scroll_step(current: f32, target: f32, dt: f32, ui_scale: f32) -> (f32, bool) {
    const TAU_SEC: f32 = 0.14;
    const MAX_ALPHA: f32 = 0.18;
    const TAIL_DEVICE_PX: f32 = 16.0;
    let scale = ui_scale.max(0.01);
    let device_px = 1.0 / scale;
    let target = snap_scroll_offset_to_device_px(target, scale);
    let current = snap_scroll_offset_to_device_px(current, scale);
    let remaining = target - current;
    if !remaining.is_finite() {
        return (target, false);
    }
    if remaining.abs() < device_px * 0.5 {
        return (target, false);
    }

    let sign = remaining.signum();
    let dist_px = remaining.abs() / device_px;
    let step1 = snap_scroll_offset_to_device_px(current + device_px * sign, scale);

    if dist_px <= TAIL_DEVICE_PX {
        if (remaining > 0.0 && step1 >= target) || (remaining < 0.0 && step1 <= target) {
            return (target, false);
        }
        return (step1, true);
    }

    let raw_alpha = 1.0 - (-dt.max(0.0) / TAU_SEC).exp();
    let alpha = raw_alpha.min(MAX_ALPHA).clamp(0.0, 1.0);
    let mut next = snap_scroll_offset_to_device_px(current + remaining * alpha, scale);
    if remaining > 0.0 {
        next = next.min(target);
    } else {
        next = next.max(target);
    }
    if (next - current).abs() < device_px * 0.25 {
        next = step1;
        if (remaining > 0.0 && next >= target) || (remaining < 0.0 && next <= target) {
            return (target, false);
        }
    }
    (next, true)
}

fn collect_descendant_extent(
    el: &Element,
    max_right: &mut f32,
    max_bottom: &mut f32,
    current_rest: &str,
) {
    let mut stack: Vec<&Element> = el.children.iter().collect();
    while let Some(c) = stack.pop() {
        let cb = c.layout_bounds;
        *max_right = (*max_right).max(cb[0] + cb[2]);
        *max_bottom = (*max_bottom).max(cb[1] + cb[3]);

        let is_nested_scroll_area = c
            .focus_id
            .as_deref()
            .and_then(|fid| {
                fid.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
            })
            .is_some_and(|rest| rest != current_rest);

        if is_nested_scroll_area {
            continue;
        }
        stack.extend(c.children.iter());
    }
}

pub(super) fn parse_scrollbar_focus_id(focus_id: &str) -> Option<(String, ScrollAxis, bool)> {
    let (prefix, is_thumb) = if let Some(rest) =
        focus_id.strip_prefix(crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX)
    {
        (rest, true)
    } else if let Some(rest) =
        focus_id.strip_prefix(crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
    {
        (rest, false)
    } else {
        return None;
    };
    let (rest, axis_raw) = prefix.rsplit_once(':')?;
    let axis = match axis_raw {
        "x" => ScrollAxis::X,
        _ => ScrollAxis::Y,
    };
    Some((rest.to_string(), axis, is_thumb))
}

#[allow(dead_code)]
pub(super) fn collect_scroll_areas_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
) {
    if !crate::core::tree::pointer_events_targets_children(el) {
        return;
    }
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];

    // Clipping check: if we are outside current overflow container, do not search children.
    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && (x < vx || x > vx + b[2] || y < vy || y > vy + b[3])
    {
        return;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    for child in el.children.iter().rev() {
        collect_scroll_areas_at(child, x, y, next_acc, out);
    }

    if let Some(id) = el.focus_id.as_deref() {
        if let Some(rest) = id.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
        {
            let mut max_right = b[0] + b[2];
            let mut max_bottom = b[1] + b[3];
            collect_descendant_extent(el, &mut max_right, &mut max_bottom, rest);
            let content_w = (max_right - b[0]).max(b[2]);
            let content_h = (max_bottom - b[1]).max(b[3]);
            let max_x = (content_w - b[2]).max(0.0);
            let max_y = (content_h - b[3]).max(0.0);
            out.push((rest.to_string(), b, [max_x, max_y], [content_w, content_h]));
        }
    }
}

fn scroll_hit_self_occludes(el: &Element, child_hit: bool, inside: bool) -> bool {
    if crate::layout::layout_tree::portal_hit_passthrough(el)
        || !crate::core::tree::pointer_events_targets_self(el)
    {
        return child_hit;
    }
    child_hit || inside
}

fn collect_scroll_hit_walk(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    mut after: impl FnMut(&Element, [f32; 4], bool, bool) -> bool,
) -> bool {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        scroll_acc: [f32; 2],
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        scroll_acc,
        phase: Phase::Enter,
    }];
    let mut hit = false;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if hit {
                    continue;
                }
                if !crate::core::tree::pointer_events_targets_children(frame.el) {
                    continue;
                }
                let escaped = super::tree_hit::escape_scroll_acc(frame.el, frame.scroll_acc);
                let b = frame.el.layout_bounds;
                let vx = b[0] - escaped[0];
                let vy = b[1] - escaped[1];
                let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
                let is_body_portal = frame.el.template_id.as_deref()
                    == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
                if !is_body_portal
                    && (frame.el.style.overflow.x != taffy::style::Overflow::Visible
                        || frame.el.style.overflow.y != taffy::style::Overflow::Visible)
                    && !inside
                {
                    continue;
                }
                let mut next_acc = escaped;
                if !is_body_portal {
                    if frame.el.style.overflow.x != taffy::style::Overflow::Visible {
                        next_acc[0] += frame.el.scroll_offset[0];
                    }
                    if frame.el.style.overflow.y != taffy::style::Overflow::Visible {
                        next_acc[1] += frame.el.scroll_offset[1];
                    }
                }
                stack.push(Frame {
                    el: frame.el,
                    scroll_acc: escaped,
                    phase: Phase::After,
                });
                if super::tree_hit::children_share_same_z_index(frame.el) {
                    for child in &frame.el.children {
                        stack.push(Frame {
                            el: child,
                            scroll_acc: next_acc,
                            phase: Phase::Enter,
                        });
                    }
                } else {
                    for i in super::tree_hit::hit_test_child_indices(frame.el) {
                        stack.push(Frame {
                            el: &frame.el.children[i],
                            scroll_acc: next_acc,
                            phase: Phase::Enter,
                        });
                    }
                }
            }
            Phase::After => {
                let b = frame.el.layout_bounds;
                let vx = b[0] - frame.scroll_acc[0];
                let vy = b[1] - frame.scroll_acc[1];
                let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];
                let child_hit = hit;
                hit = after(frame.el, b, inside, child_hit);
            }
        }
    }
    hit
}

/// Collect only the topmost hit branch (deepest child path) under pointer.
/// Output order matches `collect_scroll_areas_at`: deepest scroll area first, then ancestors.
///
/// Occluders (modal underlay) that claim bounds stop sibling search even when they are
/// not themselves scroll areas — otherwise gallery scrollbars under a dialog keep receiving
/// hover / wheel.
pub(super) fn collect_scroll_chain_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
) -> bool {
    collect_scroll_hit_walk(el, x, y, scroll_acc, |el, b, inside, child_hit| {
        if let Some(id) = el.focus_id.as_deref() {
            if let Some(rest) =
                id.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
            {
                let mut max_right = b[0] + b[2];
                let mut max_bottom = b[1] + b[3];
                collect_descendant_extent(el, &mut max_right, &mut max_bottom, rest);
                let content_w = (max_right - b[0]).max(b[2]);
                let content_h = (max_bottom - b[1]).max(b[3]);
                let max_x = (content_w - b[2]).max(0.0);
                let max_y = (content_h - b[3]).max(0.0);
                out.push((rest.to_string(), b, [max_x, max_y], [content_w, content_h]));
            }
        }
        scroll_hit_self_occludes(el, child_hit, inside)
    })
}

/// Sticky wheel coalescing must not keep an ancestor rest once a nested viewport is
/// deepest under the pointer (content scrolled under a stationary cursor).
pub(crate) fn wheel_sticky_matches_deepest(sticky: &str, chain: &[String]) -> bool {
    chain.first().map(String::as_str) == Some(sticky)
}

pub(crate) type ScrollMetricsEntry = ([f32; 4], [f32; 2], [f32; 2]);

/// True when a freshly collected `max=0` is a clip/band-cull collapse, not a real
/// "content now fits" state.
///
/// Nested viewports scrolled out of the parent clip can report `content == viewport`
/// (flex-shrink / skipped children). Overwriting the cache with that sample would
/// clamp their offset to 0.
///
/// The opposite case is a **grown** viewport that still measures the previous
/// content and now covers it (short sidebar + window taller). That must be
/// accepted so offset/target clamp instead of staying scrolled into empty space.
#[inline]
pub(crate) fn is_stale_zero_max_sample(
    fresh_max: [f32; 2],
    fresh_bounds: [f32; 4],
    fresh_content: [f32; 2],
    prev: Option<&ScrollMetricsEntry>,
) -> bool {
    if fresh_max[0] >= 0.5 || fresh_max[1] >= 0.5 {
        return false;
    }
    let Some((prev_bounds, prev_max, prev_content)) = prev else {
        return false;
    };
    if prev_max[0] < 0.5 && prev_max[1] < 0.5 {
        return false;
    }
    const EPS: f32 = 0.5;
    let y_was_scrollable = prev_max[1] > EPS;
    let x_was_scrollable = prev_max[0] > EPS;
    let grew_h = fresh_bounds[3] > prev_bounds[3] + EPS;
    let grew_w = fresh_bounds[2] > prev_bounds[2] + EPS;
    // Content did not collapse to the new viewport (cull samples drop content_h
    // down to vh). Stretching with the viewport (`fresh >= prev`) is still a grow.
    let content_kept_y = fresh_content[1] + EPS >= prev_content[1];
    let content_kept_x = fresh_content[0] + EPS >= prev_content[0];
    // Maximize/`scroll_only` can also collapse content_h to the new vh. If last
    // frame's content already fits the grown viewport, accept max=0 so a nested
    // MessageScroller leftover cannot keep the sidebar overlay lit.
    let old_y_fits = prev_content[1] <= fresh_bounds[3] + EPS;
    let old_x_fits = prev_content[0] <= fresh_bounds[2] + EPS;
    if (y_was_scrollable && grew_h && (content_kept_y || old_y_fits))
        || (x_was_scrollable && grew_w && (content_kept_x || old_x_fits))
    {
        return false;
    }
    true
}

/// Replace cache from this frame's collect, but do not let a band-cull / clip-cull
/// sample publish `max=0` (or drop a nested viewport) over a known-scrollable rest.
/// That clamp-to-zero is what lets the parent steal the next wheel tick while inner
/// glyphs stay at the previous offset.
pub(crate) fn merge_scroll_metrics_cache(
    cache: &mut HashMap<String, ScrollMetricsEntry>,
    fresh: impl IntoIterator<Item = (String, [f32; 4], [f32; 2], [f32; 2])>,
) {
    let old = std::mem::take(cache);
    for (rest, bounds, max, content) in fresh {
        if is_stale_zero_max_sample(max, bounds, content, old.get(&rest)) {
            if let Some(prev) = old.get(&rest) {
                cache.insert(rest, *prev);
                continue;
            }
        }
        cache.insert(rest, (bounds, max, content));
    }
    for (rest, prev) in old {
        if prev.1[0] > 0.5 || prev.1[1] > 0.5 {
            cache.entry(rest).or_insert(prev);
        }
    }
}

/// Wheel hit-chain lookup: prefer a live cache entry, else reconstruct max from
/// content/viewport signals so a culled collect cannot make a nested scroller
/// look unscrollable.
pub(crate) fn wheel_metrics_for_rest(
    rest: &str,
    cache: &HashMap<String, ScrollMetricsEntry>,
) -> Option<ScrollMetricsEntry> {
    if let Some(&entry) = cache.get(rest) {
        if entry.1[0] > 0.5 || entry.1[1] > 0.5 {
            return Some(entry);
        }
    }
    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
    let vw = crate::core::reactivity::use_signal(keys.viewport_w.as_str(), || 0.0_f32).peek();
    let vh = crate::core::reactivity::use_signal(keys.viewport_h.as_str(), || 0.0_f32).peek();
    let cw = crate::core::reactivity::use_signal(keys.content_w.as_str(), || 0.0_f32).peek();
    let ch = crate::core::reactivity::use_signal(keys.content_h.as_str(), || 0.0_f32).peek();
    let max = [(cw - vw).max(0.0), (ch - vh).max(0.0)];
    if max[0] > 0.5 || max[1] > 0.5 {
        let bounds = cache.get(rest).map(|e| e.0).unwrap_or([0.0, 0.0, vw, vh]);
        return Some((bounds, max, [cw, ch]));
    }
    cache.get(rest).copied()
}

/// Collect only scroll-area ids on the topmost hit branch (deepest first).
/// This avoids computing extent/metrics during wheel hit-testing; callers can
/// resolve metrics lazily (preferably from cache) only for candidate ids.
pub(crate) fn collect_scroll_rest_chain_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<String>,
) -> bool {
    collect_scroll_hit_walk(el, x, y, scroll_acc, |el, _b, inside, child_hit| {
        if let Some(id) = el.focus_id.as_deref() {
            if let Some(rest) =
                id.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
            {
                out.push(rest.to_string());
            }
        }
        scroll_hit_self_occludes(el, child_hit, inside)
    })
}

#[allow(dead_code)]
pub(super) fn find_scroll_area_metrics_by_rest(
    el: &Element,
    rest: &str,
) -> Option<([f32; 4], [f32; 2], [f32; 2])> {
    let want = format!(
        "{}{}",
        crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX,
        rest
    );
    let el = crate::core::tree::find_in_tree(el, |n| n.focus_id.as_deref() == Some(want.as_str()))?;
    let b = el.layout_bounds;
    let mut max_right = b[0] + b[2];
    let mut max_bottom = b[1] + b[3];
    collect_descendant_extent(el, &mut max_right, &mut max_bottom, rest);
    let content_w = (max_right - b[0]).max(b[2]);
    let content_h = (max_bottom - b[1]).max(b[3]);
    let max_x = (content_w - b[2]).max(0.0);
    let max_y = (content_h - b[3]).max(0.0);
    Some((b, [max_x, max_y], [content_w, content_h]))
}

/// Keep only future deadlines. Returns true if any entry was dropped as expired.
pub(super) fn retain_future_instants(map: &mut HashMap<String, Instant>, now: Instant) -> bool {
    let mut expired = false;
    map.retain(|_, until| {
        if *until <= now {
            expired = true;
            false
        } else {
            true
        }
    });
    expired
}

/// Clamp offset + target to the current scroll max.
///
/// When the viewport grows enough that `max → 0`, offset is forced to 0 for paint,
/// but a stale **target** above max must also be written back. Otherwise smooth-scroll
/// (gallery `scroll_behavior="smooth"`) will re-seek the old target after a later
/// shrink — top content jumps out of view instead of keeping the top and letting the
/// bottom overflow.
pub(crate) fn clamp_scroll_offset_and_target(offset: f32, target: f32, max: f32) -> (f32, f32) {
    let max = max.max(0.0);
    (offset.clamp(0.0, max), target.clamp(0.0, max))
}

pub(crate) fn set_scroll_offset_with_behavior(
    rest: &str,
    target_x: f32,
    target_y: f32,
    immediate: bool,
) {
    let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(rest);
    let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(rest);
    let tx_key = crate::core::scroll_viewport::scroll_target_x_signal_key(rest);
    let ty_key = crate::core::scroll_viewport::scroll_target_y_signal_key(rest);
    let behavior_key = crate::core::scroll_viewport::scroll_behavior_signal_key(rest);

    let sx = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32);
    let sy = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32);
    // peek: The caller is always inside the tracked effect (Select/Combobox menu). `.get()` will order the entire menu to
    // Scroll offset/behavior, hundreds of Items are rebuilt as soon as the scroll wheel or layout is moved → Hover flashing.
    let tx = crate::core::reactivity::use_signal(&tx_key, || sx.peek());
    let ty = crate::core::reactivity::use_signal(&ty_key, || sy.peek());
    let _behavior = crate::core::reactivity::use_signal(&behavior_key, || {
        crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
    });

    if immediate {
        sx.set_silent(target_x);
        sy.set_silent(target_y);
    }
    tx.set_silent(target_x);
    ty.set_silent(target_y);
}

/// Apply keyboard / page scroll delta respecting viewport `scroll_behavior`.
/// Returns `true` when smooth mode wrote targets (caller should keep animating).
pub(crate) fn apply_scroll_navigation_delta(
    rest: &str,
    dx: f32,
    dy: f32,
    max_x: f32,
    max_y: f32,
) -> bool {
    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
    let behavior = crate::core::reactivity::use_signal(keys.behavior.as_str(), || {
        crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
    })
    .peek();
    let is_smooth = crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior);

    let sx = crate::core::reactivity::use_signal(keys.offset_x.as_str(), || 0.0_f32);
    let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32);
    let tx = crate::core::reactivity::use_signal(keys.target_x.as_str(), || sx.peek());
    let ty = crate::core::reactivity::use_signal(keys.target_y.as_str(), || sy.peek());

    if is_smooth {
        let next_x = (tx.peek() - dx).clamp(0.0, max_x);
        let next_y = (ty.peek() - dy).clamp(0.0, max_y);
        tx.set_silent(next_x);
        ty.set_silent(next_y);
        if dx.abs() > 0.1 || dy.abs() > 0.1 {
            crate::core::reactivity::use_signal(keys.activity.as_str(), || 0.0_f32).set_silent(1.0);
        }
        true
    } else {
        let next_x = (tx.peek() - dx).clamp(0.0, max_x);
        let next_y = (ty.peek() - dy).clamp(0.0, max_y);
        set_scroll_offset_with_behavior(rest, next_x, next_y, true);
        false
    }
}

/// Jump to absolute scroll offset (Home/End), honoring smooth vs jump behavior.
/// Returns `true` when smooth mode wrote targets.
pub(crate) fn apply_scroll_navigation_absolute(
    rest: &str,
    target_x: f32,
    target_y: f32,
    max_x: f32,
    max_y: f32,
) -> bool {
    let tx = target_x.clamp(0.0, max_x.max(0.0));
    let ty = target_y.clamp(0.0, max_y.max(0.0));
    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
    let behavior = crate::core::reactivity::use_signal(keys.behavior.as_str(), || {
        crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
    })
    .peek();
    if crate::core::scroll_viewport::scroll_behavior_is_smooth(&behavior) {
        crate::core::reactivity::use_signal(keys.target_x.as_str(), || 0.0_f32).set_silent(tx);
        crate::core::reactivity::use_signal(keys.target_y.as_str(), || 0.0_f32).set_silent(ty);
        crate::core::reactivity::use_signal(keys.activity.as_str(), || 0.0_f32).set_silent(1.0);
        true
    } else {
        set_scroll_offset_with_behavior(rest, tx, ty, true);
        false
    }
}

/// Nudge a scroll viewport along Y (positive `delta` scrolls content up / decreases offset).
/// Returns `Some(is_smooth)` when scrolled, `None` when unchanged.
pub(crate) fn nudge_scroll_viewport_y(rest: &str, delta: f32) -> Option<bool> {
    if delta.abs() < 0.5 {
        return None;
    }
    let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
    let sy = crate::core::reactivity::use_signal(keys.offset_y.as_str(), || 0.0_f32).peek();
    let vh = crate::core::reactivity::use_signal(keys.viewport_h.as_str(), || 0.0_f32).peek();
    let ch = crate::core::reactivity::use_signal(keys.content_h.as_str(), || 0.0_f32).peek();
    let max_y = (ch - vh).max(0.0);
    if max_y < 0.5 {
        return None;
    }
    let next = (sy - delta).clamp(0.0, max_y);
    if (next - sy).abs() < 0.5 {
        return None;
    }
    let vw = crate::core::reactivity::use_signal(keys.viewport_w.as_str(), || 0.0_f32).peek();
    let cw = crate::core::reactivity::use_signal(keys.content_w.as_str(), || 0.0_f32).peek();
    let max_x = (cw - vw).max(0.0);
    Some(apply_scroll_navigation_delta(
        rest,
        0.0,
        sy - next,
        max_x,
        max_y,
    ))
}

pub fn get_system_scroll_lines() -> f32 {
    crate::platform::system_scroll_lines()
}

#[cfg(test)]
mod inertia_tests {
    use super::decay_scroll_velocity;

    #[test]
    fn decay_scroll_velocity_reduces_and_stops() {
        let [vx, vy] = decay_scroll_velocity(400.0, -200.0, 0.016, 4.5);
        assert!(vx.abs() < 400.0 && vx > 0.0);
        assert!(vy.abs() < 200.0 && vy < 0.0);
        let mut v = [400.0_f32, 0.0];
        for _ in 0..120 {
            v = decay_scroll_velocity(v[0], v[1], 0.016, 4.5);
        }
        assert_eq!(v, [0.0, 0.0]);
    }
}

#[cfg(test)]
mod smooth_scroll_step_tests {
    use super::{smooth_scroll_step, snap_scroll_offset_to_device_px};

    #[test]
    fn snap_aligns_to_device_pixel() {
        assert!((snap_scroll_offset_to_device_px(10.4, 1.0) - 10.0).abs() < 1e-4);
        assert!((snap_scroll_offset_to_device_px(10.4, 1.25) - 10.4).abs() < 1e-4);
        assert!((snap_scroll_offset_to_device_px(10.2, 2.0) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn chase_tail_snaps_onto_device_grid() {
        let (next, anim) = smooth_scroll_step(10.2, 10.4, 0.016, 1.0);
        assert!(
            !anim,
            "sub-pixel remainder must snap, still_animating={anim}"
        );
        assert!((next - 10.0).abs() < 1e-4);
        let (done, anim2) = smooth_scroll_step(next, 10.4, 0.016, 1.0);
        assert!(!anim2);
        assert!((done - 10.0).abs() < 1e-4);
    }

    #[test]
    fn tail_does_not_overshoot_then_snap_back() {
        let target = 10.7_f32;
        let snapped_t = snap_scroll_offset_to_device_px(target, 1.0);
        let mut y = 8.0_f32;
        let mut prev = y;
        for _ in 0..80 {
            let (next, anim) = smooth_scroll_step(y, target, 0.016, 1.0);
            assert!(
                next + 1e-4 >= prev && next <= snapped_t + 1e-4,
                "non-monotonic or overshoot: {prev} -> {next} (target {snapped_t})"
            );
            assert!(
                (next - prev).abs() <= 1.01 + 1e-3,
                "tail must not dump >1px: {prev} -> {next}"
            );
            y = next;
            prev = next;
            if !anim {
                break;
            }
        }
        assert!((y - snapped_t).abs() < 1e-4);
    }

    #[test]
    fn long_chase_paint_stays_on_device_grid() {
        let mut y = 0.0_f32;
        let target = 240.0;
        let scale = 1.25_f32;
        let mut frames = 0u32;
        loop {
            let (next, anim) = smooth_scroll_step(y, target, 0.016, scale);
            let device = (next * scale).round() / scale;
            assert!(
                (next - device).abs() < 1e-4,
                "offset {next} not on device grid at scale {scale}"
            );
            y = next;
            frames += 1;
            if !anim {
                break;
            }
            assert!(frames < 400, "chase did not settle");
        }
        assert!((y - snap_scroll_offset_to_device_px(target, scale)).abs() < 1e-3);
        assert!(frames > 4, "should ease over several frames, got {frames}");
    }

    #[test]
    fn tail_paint_steps_are_single_device_pixels() {
        let scale = 1.0_f32;
        let device_px = 1.0 / scale;
        let target = 48.0;
        let mut y = 0.0_f32;
        let mut frames = 0u32;
        loop {
            let (next, anim) = smooth_scroll_step(y, target, 0.016, scale);
            let remaining = (target - y).abs();
            let delta = (next - y).abs();
            if remaining <= 16.0 * device_px + 1e-3 {
                assert!(
                    delta <= device_px * 1.01 + 1e-3,
                    "tail jumped {y} -> {next} (delta {delta}) with {remaining}px left"
                );
            }
            y = next;
            frames += 1;
            if !anim {
                break;
            }
            assert!(frames < 400, "chase did not settle");
        }
        assert!((y - target).abs() < 1e-3);
    }
}

#[cfg(test)]
mod clamp_scroll_extent_tests {
    use super::clamp_scroll_offset_and_target;

    #[test]
    fn expand_to_fit_clears_stale_target_so_later_shrink_keeps_top() {
        // Scrolled down, then viewport grows until max=0: both must become 0.
        let (o, t) = clamp_scroll_offset_and_target(280.0, 280.0, 0.0);
        assert_eq!((o, t), (0.0, 0.0));
        // Shrink again: with synced target, stay at top (bottom overflows).
        let (o2, t2) = clamp_scroll_offset_and_target(o, t, 200.0);
        assert_eq!((o2, t2), (0.0, 0.0));
    }

    #[test]
    fn shrink_while_scrolled_clamps_to_new_max_not_zero() {
        // Standard browser-like: stay near bottom when max shrinks.
        let (o, t) = clamp_scroll_offset_and_target(350.0, 350.0, 200.0);
        assert_eq!((o, t), (200.0, 200.0));
    }
}

#[cfg(test)]
mod wheel_sticky_tests {
    use super::wheel_sticky_matches_deepest;

    #[test]
    fn nested_deepest_breaks_outer_sticky() {
        let chain = vec!["inner".into(), "outer".into()];
        assert!(!wheel_sticky_matches_deepest("outer", &chain));
        assert!(wheel_sticky_matches_deepest("inner", &chain));
    }

    #[test]
    fn empty_chain_does_not_keep_sticky() {
        assert!(!wheel_sticky_matches_deepest("outer", &[]));
    }
}

#[cfg(test)]
mod scroll_metrics_merge_tests {
    use super::{is_stale_zero_max_sample, merge_scroll_metrics_cache, wheel_metrics_for_rest};
    use crate::core::reactivity::{create_root, use_signal};
    use crate::core::scroll_viewport::{scroll_content_h_signal_key, scroll_viewport_h_signal_key};
    use std::collections::HashMap;

    #[test]
    fn zero_max_cull_sample_does_not_overwrite_live_inner() {
        let mut cache = HashMap::new();
        cache.insert(
            "inner".into(),
            ([0.0, 100.0, 320.0, 384.0], [0.0, 240.0], [320.0, 624.0]),
        );
        cache.insert(
            "outer".into(),
            ([0.0, 0.0, 400.0, 800.0], [0.0, 200.0], [400.0, 1000.0]),
        );
        merge_scroll_metrics_cache(
            &mut cache,
            [
                (
                    "outer".into(),
                    [0.0, 0.0, 400.0, 800.0],
                    [0.0, 220.0],
                    [400.0, 1020.0],
                ),
                (
                    "inner".into(),
                    [0.0, 80.0, 320.0, 384.0],
                    [0.0, 0.0],
                    [320.0, 384.0],
                ),
            ],
        );
        let inner = cache.get("inner").expect("inner kept");
        assert!(
            inner.1[1] > 8.0,
            "cull sample max=0 must not replace live inner, got {:?}",
            inner.1
        );
        assert!((cache.get("outer").unwrap().1[1] - 220.0).abs() < 0.1);
    }

    #[test]
    fn omitted_inner_is_retained() {
        let mut cache = HashMap::new();
        cache.insert(
            "inner".into(),
            ([0.0, 100.0, 320.0, 384.0], [0.0, 240.0], [320.0, 624.0]),
        );
        merge_scroll_metrics_cache(
            &mut cache,
            [(
                "outer".into(),
                [0.0, 0.0, 400.0, 800.0],
                [0.0, 200.0],
                [400.0, 1000.0],
            )],
        );
        assert!(
            cache.get("inner").is_some_and(|e| e.1[1] > 8.0),
            "band-culled nested viewport must stay in the wheel cache"
        );
    }

    #[test]
    fn stale_zero_max_detects_cull_sample() {
        let prev = ([0.0, 100.0, 320.0, 384.0], [0.0, 100.0], [320.0, 484.0]);
        assert!(is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 80.0, 320.0, 384.0],
            [320.0, 384.0],
            Some(&prev),
        ));
        assert!(!is_stale_zero_max_sample(
            [0.0, 80.0],
            [0.0, 80.0, 320.0, 384.0],
            [320.0, 464.0],
            Some(&prev),
        ));
        assert!(!is_stale_zero_max_sample(
            [0.0, 0.0],
            [0.0, 0.0, 320.0, 384.0],
            [320.0, 384.0],
            None,
        ));
    }

    #[test]
    fn expand_viewport_until_fit_is_not_stale_zero() {
        // Scrolled sidebar: vh=300, content=500, max=200. Window grows to vh=600.
        let prev = ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]);
        assert!(
            !is_stale_zero_max_sample(
                [0.0, 0.0],
                [0.0, 48.0, 208.0, 600.0],
                [208.0, 500.0],
                Some(&prev),
            ),
            "grown viewport that covers previous content must clamp, not keep the short-screen max"
        );
    }

    #[test]
    fn expand_viewport_collapsed_content_is_not_stale_zero() {
        // Maximize + scroll_only can publish content_h == new vh (looks like a cull).
        let prev = ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]);
        assert!(
            !is_stale_zero_max_sample(
                [0.0, 0.0],
                [0.0, 48.0, 208.0, 600.0],
                [208.0, 600.0],
                Some(&prev),
            ),
            "grown viewport whose previous content now fits must not keep short-screen overflow"
        );
    }

    #[test]
    fn expand_viewport_until_fit_replaces_cache_and_clamps() {
        let mut cache = HashMap::new();
        cache.insert(
            "gal-clip__sidebar-content".into(),
            ([0.0, 48.0, 208.0, 300.0], [0.0, 200.0], [208.0, 500.0]),
        );
        merge_scroll_metrics_cache(
            &mut cache,
            [(
                "gal-clip__sidebar-content".into(),
                [0.0, 48.0, 208.0, 600.0],
                [0.0, 0.0],
                [208.0, 500.0],
            )],
        );
        let entry = cache
            .get("gal-clip__sidebar-content")
            .expect("sidebar viewport kept");
        assert!(
            entry.1[1] < 0.5,
            "expand-to-fit must publish max=0, got {:?}",
            entry.1
        );
        let (offset, target) = super::clamp_scroll_offset_and_target(180.0, 180.0, entry.1[1]);
        assert_eq!((offset, target), (0.0, 0.0));
    }

    #[test]
    fn wheel_metrics_fall_back_to_signals() {
        let _ = create_root(|| {
            use_signal(&scroll_viewport_h_signal_key("ms-sig"), || 0.0_f32).set_silent(384.0);
            use_signal(&scroll_content_h_signal_key("ms-sig"), || 0.0_f32).set_silent(800.0);
            let cache = HashMap::new();
            let m = wheel_metrics_for_rest("ms-sig", &cache).expect("signal fallback");
            assert!(m.1[1] > 8.0, "got max_y={}", m.1[1]);
        });
    }
}

#[cfg(test)]
mod visibility_deadline_tests {
    use super::retain_future_instants;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn expired_deadlines_are_removed_once() {
        let now = Instant::now();
        let mut map = HashMap::new();
        map.insert("past".into(), now - Duration::from_millis(1));
        map.insert("future".into(), now + Duration::from_secs(1));
        assert!(retain_future_instants(&mut map, now));
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("future"));
        assert!(!retain_future_instants(&mut map, now));
        assert_eq!(map.len(), 1);
    }
}

#[cfg(test)]
mod scroll_item_align_tests {
    use super::{
        apply_pending_scroll_item_into_view, cancel_scroll_item_into_view,
        has_pending_scroll_item_align, request_scroll_item_into_view, scroll_offset_to_center_item,
        ScrollItemAlign,
    };
    use crate::core::reactivity::create_root;
    use crate::core::scroll_viewport::scroll_offset_y_signal_key;

    #[test]
    fn year_center_uses_real_row_height_not_32px() {
        // 1900..=2100 → 201 items; 2026 is index 126. Real Aria row ≈ 26px, not 32.
        let sy_real = scroll_offset_to_center_item(126, 26.0, 248.0, 201);
        let sy_wrong = scroll_offset_to_center_item(126, 32.0, 248.0, 201);
        assert!(
            (sy_wrong - sy_real).abs() > 500.0,
            "32px guess is hundreds of px off, got real={sy_real} wrong={sy_wrong}"
        );
        let expected = 126.0 * 26.0 + 13.0 - 124.0;
        assert!((sy_real - expected).abs() < 0.5);
    }

    #[test]
    fn pending_align_waits_for_metrics_then_writes_offset() {
        let _ = create_root(|| {
            cancel_scroll_item_into_view("t-year-align");
            request_scroll_item_into_view("t-year-align", 126, 201, ScrollItemAlign::Center);
            assert!(has_pending_scroll_item_align());
            assert!(!apply_pending_scroll_item_into_view(
                "t-year-align",
                0.0,
                0.0
            ));
            assert!(has_pending_scroll_item_align());
            let content_h = 201.0 * 26.0;
            assert!(apply_pending_scroll_item_into_view(
                "t-year-align",
                248.0,
                content_h
            ));
            assert!(!has_pending_scroll_item_align());
            let sy = crate::core::reactivity::use_signal(
                &scroll_offset_y_signal_key("t-year-align"),
                || 0.0_f32,
            )
            .peek();
            let expected = scroll_offset_to_center_item(126, 26.0, 248.0, 201);
            assert!(
                (sy - expected).abs() < 0.5,
                "offset should match real row height, got {sy} expected {expected}"
            );
        });
    }
}
