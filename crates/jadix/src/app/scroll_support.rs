use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use web_time::{Duration, Instant};

use crate::app::portal_overlay::ScrollFreeze;
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_HOISTED_PORTAL;

/// 打开下拉后把某一项滚进视口（布局写出 content/viewport 后再应用，避免菜单 effect 订阅滚动信号）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollItemAlign {
    Start,
    Center,
    End,
}

/// When to apply a pending align.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAlignWhen {
    /// Snap offset as soon as metrics exist (overlay measure / Expand pin).
    Before,
    /// Wait for a real clipped viewport, then honor the area's smooth/jump.
    After,
}

impl ScrollAlignWhen {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "after" | "enter" | "animate" => Self::After,
            _ => Self::Before,
        }
    }
}

impl ScrollItemAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "false" | "0" => None,
            "start" | "top" | "left" => Some(Self::Start),
            "end" | "bottom" | "right" => Some(Self::End),
            _ => Some(Self::Center),
        }
    }
}

#[derive(Clone, Debug)]
enum PendingAlignTarget {
    UniformItems { index: usize, item_count: usize },
    Descendant { id: String },
}

#[derive(Clone, Debug)]
struct PendingScrollItemAlign {
    target: PendingAlignTarget,
    align: ScrollItemAlign,
    when: ScrollAlignWhen,
    tries: u8,
    /// Frames where viewport still matches content (Expand pin). Not reset by
    /// Select rebuilding the menu each frame — see [`request_scroll_item_into_view`].
    clip_waits: u8,
    requested_at: Instant,
}

thread_local! {
    static PENDING_SCROLL_ITEM_ALIGN: RefCell<HashMap<String, PendingScrollItemAlign>> =
        RefCell::new(HashMap::new());
    /// Offset written against a placed `max_h` while Expand still pins viewport≈content.
    /// Layout `max_y` is 0 in that window; clamp must not wipe the align.
    static HELD_SCROLL_ALIGN_Y: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
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

/// Menu chrome: Select/Combobox content `p-1` (0.25rem × 2).
const MENU_CHROME_Y: f32 = 8.0;

/// Viewport height implied by last synced floating `max_h` (viewport-available).
fn placed_scroll_viewport_h(rest: &str) -> Option<f32> {
    let overlay_id = rest.strip_suffix(":menu_scroll")?;
    let cap = crate::utils::popup::peek_floating_place_max_h(overlay_id).filter(|h| *h > 1.0)?;
    Some((cap - MENU_CHROME_Y).max(1.0))
}

fn effective_scroll_viewport_h_when(
    rest: &str,
    viewport_h: f32,
    content_h: f32,
    when: ScrollAlignWhen,
) -> Option<f32> {
    match when {
        ScrollAlignWhen::After => {
            // After enter: do not use the Expand placed-cap shortcut — wait until the
            // layout viewport actually clips, then chase with smooth/jump.
            if viewport_h + 0.5 < content_h {
                let placed = placed_scroll_viewport_h(rest);
                return Some(placed.map(|p| viewport_h.min(p)).unwrap_or(viewport_h));
            }
            None
        }
        ScrollAlignWhen::Before => {
            let placed = placed_scroll_viewport_h(rest);
            // DatePicker months overflow 16rem: align once against the idle cap.
            // Never chase the Expand accordion sliver (that recenters every frame).
            if let Some(p) = placed.filter(|p| *p + 0.5 < content_h) {
                return Some(p);
            }
            // ColorPicker 6 formats fit in 16rem. The accordion clip is not a
            // scroll viewport — treating it as one scrolls HSLA while chrome grows.
            if placed.is_some() {
                return None;
            }
            if viewport_h + 0.5 < content_h {
                return Some(viewport_h);
            }
            None
        }
    }
}

fn upsert_pending_scroll_align(
    rest: impl Into<String>,
    target: PendingAlignTarget,
    align: ScrollItemAlign,
    when: ScrollAlignWhen,
) {
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        let mut map = m.borrow_mut();
        let key = rest.into();
        match map.get_mut(&key) {
            Some(pending) => {
                let same_kind =
                    std::mem::discriminant(&pending.target) == std::mem::discriminant(&target);
                pending.target = target;
                pending.align = align;
                pending.when = when;
                if !same_kind {
                    pending.tries = 0;
                    pending.clip_waits = 0;
                }
            }
            None => {
                map.insert(
                    key,
                    PendingScrollItemAlign {
                        target,
                        align,
                        when,
                        tries: 0,
                        clip_waits: 0,
                        requested_at: Instant::now(),
                    },
                );
            }
        }
    });
}

pub fn request_scroll_item_into_view(
    rest: impl Into<String>,
    index: usize,
    item_count: usize,
    align: ScrollItemAlign,
) {
    request_scroll_item_into_view_when(rest, index, item_count, align, ScrollAlignWhen::Before);
}

pub fn request_scroll_item_into_view_when(
    rest: impl Into<String>,
    index: usize,
    item_count: usize,
    align: ScrollItemAlign,
    when: ScrollAlignWhen,
) {
    upsert_pending_scroll_align(
        rest,
        PendingAlignTarget::UniformItems {
            index,
            item_count: item_count.max(1),
        },
        align,
        when,
    );
}

#[cfg(test)]
pub(crate) fn request_scroll_descendant_into_view(
    rest: impl Into<String>,
    target: impl Into<String>,
    align: ScrollItemAlign,
) {
    request_scroll_descendant_into_view_when(rest, target, align, ScrollAlignWhen::Before);
}

pub fn request_scroll_descendant_into_view_when(
    rest: impl Into<String>,
    target: impl Into<String>,
    align: ScrollItemAlign,
    when: ScrollAlignWhen,
) {
    let id = target.into();
    if id.is_empty() {
        return;
    }
    upsert_pending_scroll_align(rest, PendingAlignTarget::Descendant { id }, align, when);
}

pub(crate) fn cancel_scroll_item_into_view(rest: &str) {
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        m.borrow_mut().remove(rest);
    });
    HELD_SCROLL_ALIGN_Y.with(|m| {
        m.borrow_mut().remove(rest);
    });
}

/// On close: drop a pending align so exit does not keep rewriting the viewport.
/// Do **not** snap offset to 0 — keep-alive Expand still paints the menu.
pub fn reset_scroll_item_into_view(rest: &str) {
    cancel_scroll_item_into_view(rest);
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
#[cfg(test)]
pub(crate) fn apply_pending_scroll_item_into_view(
    rest: &str,
    viewport_h: f32,
    content_h: f32,
) -> bool {
    apply_pending_scroll_align(rest, [0.0, 0.0, 0.0, viewport_h], 0.0, content_h, None)
}

pub(crate) fn apply_pending_scroll_align(
    rest: &str,
    viewport_bounds: [f32; 4],
    content_w: f32,
    content_h: f32,
    descendant_bounds: Option<[f32; 4]>,
) -> bool {
    let pending = PENDING_SCROLL_ITEM_ALIGN.with(|m| m.borrow_mut().remove(rest));
    let Some(mut pending) = pending else {
        return false;
    };
    let viewport_h = viewport_bounds[3];
    let viewport_w = viewport_bounds[2];
    if viewport_h <= 1.0 || content_h <= 1.0 {
        pending.tries = pending.tries.saturating_add(1);
        if pending.tries < 12 {
            PENDING_SCROLL_ITEM_ALIGN.with(|m| {
                m.borrow_mut().insert(rest.to_string(), pending);
            });
        }
        return false;
    }
    let effective_vh = effective_scroll_viewport_h_when(rest, viewport_h, content_h, pending.when)
        .or_else(|| {
            (matches!(pending.target, PendingAlignTarget::Descendant { .. })
                && content_w > viewport_w + 0.5)
                .then_some(viewport_h)
        });
    let Some(effective_vh) = effective_vh else {
        pending.clip_waits = pending.clip_waits.saturating_add(1);
        if pending.clip_waits < 36 {
            PENDING_SCROLL_ITEM_ALIGN.with(|m| {
                m.borrow_mut().insert(rest.to_string(), pending);
            });
        }
        return false;
    };
    let (sx, sy) = match &pending.target {
        PendingAlignTarget::UniformItems { index, item_count } => {
            let item_h = content_h / *item_count as f32;
            let max_sy = (content_h - effective_vh).max(0.0);
            let sy = match pending.align {
                ScrollItemAlign::Center => {
                    scroll_offset_to_center_item(*index, item_h, effective_vh, *item_count)
                }
                ScrollItemAlign::Start => (*index as f32 * item_h).clamp(0.0, max_sy),
                ScrollItemAlign::End => {
                    (*index as f32 * item_h + item_h - effective_vh).clamp(0.0, max_sy)
                }
            };
            (0.0, sy)
        }
        PendingAlignTarget::Descendant { .. } => {
            let Some(target) = descendant_bounds.filter(|b| b[2] > 0.5 && b[3] > 0.5) else {
                pending.clip_waits = pending.clip_waits.saturating_add(1);
                if pending.clip_waits < 36 {
                    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
                        m.borrow_mut().insert(rest.to_string(), pending);
                    });
                }
                return false;
            };
            let cur_x = crate::core::reactivity::use_signal(
                &crate::core::scroll_viewport::scroll_offset_x_signal_key(rest),
                || 0.0_f32,
            )
            .peek();
            let cur_y = crate::core::reactivity::use_signal(
                &crate::core::scroll_viewport::scroll_offset_y_signal_key(rest),
                || 0.0_f32,
            )
            .peek();
            let mut vb = viewport_bounds;
            vb[3] = effective_vh;
            let max_x = (content_w - viewport_w).max(0.0);
            let max_y = (content_h - effective_vh).max(0.0);
            let [nx, ny] = scroll_offset_to_align_rect(
                vb,
                target,
                [cur_x, cur_y],
                [max_x, max_y],
                pending.align,
            );
            (nx, ny)
        }
    };
    let immediate = match pending.when {
        ScrollAlignWhen::Before => true,
        ScrollAlignWhen::After => {
            let behavior = crate::core::reactivity::use_signal(
                &crate::core::scroll_viewport::scroll_behavior_signal_key(rest),
                || crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string(),
            )
            .peek();
            crate::core::scroll_viewport::scroll_behavior_is_jump(&behavior)
        }
    };
    set_scroll_offset_with_behavior(rest, sx, sy, immediate);
    let layout_max = (content_h - viewport_h).max(0.0);
    if pending.when == ScrollAlignWhen::Before && sy > layout_max + 0.5 {
        HELD_SCROLL_ALIGN_Y.with(|m| {
            m.borrow_mut().insert(rest.to_string(), sy);
        });
    } else {
        HELD_SCROLL_ALIGN_Y.with(|m| {
            m.borrow_mut().remove(rest);
        });
    }
    true
}

pub(crate) fn scroll_offset_to_align_rect(
    viewport: [f32; 4],
    target: [f32; 4],
    current: [f32; 2],
    max_scroll: [f32; 2],
    align: ScrollItemAlign,
) -> [f32; 2] {
    let content_x = target[0] - viewport[0] + current[0];
    let content_y = target[1] - viewport[1] + current[1];
    let (dx, dy) = match align {
        ScrollItemAlign::Start => (content_x, content_y),
        ScrollItemAlign::Center => (
            content_x + target[2] * 0.5 - viewport[2] * 0.5,
            content_y + target[3] * 0.5 - viewport[3] * 0.5,
        ),
        ScrollItemAlign::End => (
            content_x + target[2] - viewport[2],
            content_y + target[3] - viewport[3],
        ),
    };
    [
        dx.clamp(0.0, max_scroll[0].max(0.0)),
        dy.clamp(0.0, max_scroll[1].max(0.0)),
    ]
}

fn node_matches_scroll_target(el: &Element, target: &str) -> bool {
    el.focus_id.as_deref() == Some(target) || el.template_id.as_deref() == Some(target)
}

fn find_scroll_target_bounds(
    root: &Element,
    viewport_rest: &str,
    target: &str,
) -> Option<[f32; 4]> {
    let want = crate::core::scroll_viewport::focus_id_for_scroll_viewport(viewport_rest);
    let mut inside = None;
    let mut anywhere = None;
    fn walk(
        el: &Element,
        target: &str,
        want: &str,
        in_vp: bool,
        scroll_acc: [f32; 2],
        inside: &mut Option<[f32; 4]>,
        anywhere: &mut Option<[f32; 4]>,
    ) {
        let in_vp = in_vp || el.focus_id.as_deref() == Some(want);
        if node_matches_scroll_target(el, target) {
            let b = el.layout_bounds;
            if b[2] > 0.5 && b[3] > 0.5 {
                if in_vp {
                    *inside = Some([b[0] - scroll_acc[0], b[1] - scroll_acc[1], b[2], b[3]]);
                } else if anywhere.is_none() {
                    *anywhere = Some(b);
                }
            }
        }
        let child_acc = if in_vp {
            crate::core::tree::child_scroll_accumulation(el, scroll_acc)
        } else {
            scroll_acc
        };
        for c in &el.children {
            walk(c, target, want, in_vp, child_acc, inside, anywhere);
        }
    }
    walk(
        root,
        target,
        &want,
        false,
        [0.0; 2],
        &mut inside,
        &mut anywhere,
    );
    inside.or(anywhere)
}

/// Snapshot descendant target boxes before the post-render signal pass (split-borrow).
pub(crate) fn resolve_pending_descendant_bounds(root: &Element) -> HashMap<String, [f32; 4]> {
    PENDING_SCROLL_ITEM_ALIGN.with(|m| {
        let map = m.borrow();
        map.iter()
            .filter_map(|(rest, pending)| match &pending.target {
                PendingAlignTarget::Descendant { id } => {
                    find_scroll_target_bounds(root, rest, id).map(|b| (rest.clone(), b))
                }
                PendingAlignTarget::UniformItems { .. } => None,
            })
            .collect()
    })
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

pub(super) fn scrollbar_drag_bounds(
    root: &Element,
    rest: &str,
    axis: ScrollAxis,
) -> Option<([f32; 4], [f32; 4])> {
    let axis = if axis == ScrollAxis::X { "x" } else { "y" };
    let track_id = format!(
        "{}{rest}:{axis}",
        crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX
    );
    let thumb_id = format!(
        "{}{rest}:{axis}",
        crate::core::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX
    );
    let (track, _) = super::tree_hit::find_paint_aabb_by_focus_id(root, &track_id, [0.0; 2])?;
    let (thumb, _) = super::tree_hit::find_paint_aabb_by_focus_id(root, &thumb_id, [0.0; 2])?;
    Some((track, thumb))
}

#[cfg(test)]
#[path = "tests/scrollbar_drag.rs"]
mod scrollbar_drag_tests;
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
    pub(super) last_t: web_time::Instant,
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
/// shadows, and page fills move on the same grid. The chase keeps a continuous internal position while painting on this grid.
pub(super) fn snap_scroll_offset_to_device_px(offset: f32, ui_scale: f32) -> f32 {
    let s = ui_scale.max(0.01);
    (offset * s).round() / s
}

/// Critically damped chase: gentle acceleration, continuous retargeting, no overshoot.
/// Keep the continuous position between frames; rounding the integrator itself
/// makes a small wheel delta depend on frame rate and crawl at high refresh rates.
#[derive(Clone, Copy, Debug)]
pub(super) struct SmoothScrollAxis {
    position: f32,
    velocity: f32,
}

impl SmoothScrollAxis {
    pub(super) fn new(current: f32) -> Self {
        Self {
            position: current,
            velocity: 0.0,
        }
    }

    pub(super) fn step(
        &mut self,
        current: f32,
        target: f32,
        dt: f32,
        ui_scale: f32,
    ) -> (f32, bool) {
        const OMEGA: f32 = 18.0;
        let scale = ui_scale.max(0.01);
        let target = snap_scroll_offset_to_device_px(target, scale);
        let current = snap_scroll_offset_to_device_px(current, scale);
        if (snap_scroll_offset_to_device_px(self.position, scale) - current).abs() > 0.51 / scale {
            *self = Self::new(current);
        }
        let remaining = target - self.position;
        if !remaining.is_finite() {
            *self = Self::new(current);
            return (current, false);
        }
        // Reversing the wheel must cancel velocity toward the old destination.
        if self.velocity * remaining < 0.0 {
            self.velocity = 0.0;
        }
        let dt = dt.clamp(0.0, 0.08);
        if dt == 0.0 {
            return (current, (target - current).abs() > 0.5 / scale);
        }
        let displacement = self.position - target;
        let j = self.velocity + OMEGA * displacement;
        let decay = (-OMEGA * dt).exp();
        self.position = target + (displacement + j * dt) * decay;
        self.velocity = (self.velocity - OMEGA * j * dt) * decay;
        if (target - self.position) * remaining < 0.0
            || ((target - self.position).abs() * scale < 0.5 && self.velocity.abs() * scale < 12.0)
        {
            *self = Self::new(target);
            return (target, false);
        }
        (snap_scroll_offset_to_device_px(self.position, scale), true)
    }
}

/// Keep at most three ordinary wheel notches ahead of the painted position.
/// A single larger pixel-wheel event retains its distance; repeated events
/// while rendering is blocked cannot queue several screens of catch-up.
/// Keyboard/page navigation uses its own unbounded destination.
pub(super) fn wheel_scroll_target(current: f32, target: f32, delta: f32, max: f32) -> f32 {
    if delta == 0.0 {
        return target;
    }
    let pending = target - current;
    let base = if pending * delta > 0.0 {
        current
    } else {
        target
    };
    let limit = 225.0_f32.max(delta.abs());
    (base - delta)
        .clamp(current - limit, current + limit)
        .clamp(0.0, max)
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
        focus_id.strip_prefix(crate::core::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX)
    {
        (rest, true)
    } else if let Some(rest) =
        focus_id.strip_prefix(crate::core::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
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
    let scale = crate::core::tree::paint_scale_with_ancestor(el, el.paint_ancestor_scale);
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0] * scale[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1] * scale[1];
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

#[cfg(test)]
thread_local! {
    static SCROLL_HIT_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
                #[cfg(test)]
                SCROLL_HIT_VISITS.with(|count| count.set(count.get() + 1));
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
                // Within a self-occluding viewport, descendant hit geometry cannot
                // change its scroll owner when the stamped subtree has no other viewport.
                // Keep walking nested viewports and invalidated/unmounted summaries.
                if inside
                    && frame.el.collect_paint_hash != 0
                    && frame.el.children.iter().all(|child| {
                        child.collect_paint_hash != 0 && !child.collect_skip_has_scroll
                    })
                    && frame.el.focus_id.as_deref().is_some_and(|id| {
                        id.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
                    })
                    && scroll_hit_self_occludes(frame.el, false, true)
                {
                    hit = after(frame.el, b, true, false);
                    continue;
                }
                let mut next_acc = escaped;
                if !is_body_portal {
                    let scale = crate::core::tree::paint_scale_with_ancestor(
                        frame.el,
                        frame.el.paint_ancestor_scale,
                    );
                    if frame.el.style.overflow.x != taffy::style::Overflow::Visible {
                        next_acc[0] += frame.el.scroll_offset[0] * scale[0];
                    }
                    if frame.el.style.overflow.y != taffy::style::Overflow::Visible {
                        next_acc[1] += frame.el.scroll_offset[1] * scale[1];
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
#[cfg(test)]
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

pub(crate) type ScrollMetricsEntry = crate::core::scroll_viewport::ScrollMetricsSample;

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
    crate::core::scroll_viewport::is_stale_zero_max_sample(
        fresh_max,
        fresh_bounds,
        fresh_content,
        prev,
    )
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

/// Expand pin reports `max_y=0`. Keep a just-applied align offset until the
/// ScrollArea actually clips (otherwise the next pre-render clamp snaps to 0
/// and the menu appears at the top, then jumps after idle).
pub(crate) fn clamp_max_y_for_pending_align(rest: &str, raw_max_y: f32) -> f32 {
    HELD_SCROLL_ALIGN_Y.with(|m| {
        let mut map = m.borrow_mut();
        let Some(&held) = map.get(rest) else {
            return raw_max_y;
        };
        if raw_max_y + 0.5 >= held {
            map.remove(rest);
            return raw_max_y;
        }
        raw_max_y.max(held)
    })
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

/// Scroll viewport `rest` ids that live under a `jadix_hoisted_portal` subtree.
pub(crate) fn collect_hoisted_scroll_rests(root: &Element) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack: Vec<(&Element, bool)> = vec![(root, false)];
    while let Some((el, in_hoisted)) = stack.pop() {
        if el.collect_paint_hash != 0
            && (!el.collect_skip_has_scroll || (!in_hoisted && !el.collect_skip_side_channel))
        {
            continue;
        }
        let hoisted = in_hoisted
            || el
                .attrs
                .get(ATTR_HOISTED_PORTAL)
                .map(|s| s == "1")
                .unwrap_or(false);
        if hoisted {
            if let Some(fid) = el.focus_id.as_deref() {
                if let Some(rest) =
                    crate::core::scroll_viewport::strip_scroll_viewport_focus_id(fid)
                {
                    out.insert(rest.to_string());
                }
            }
        }
        for child in el.children.iter().rev() {
            stack.push((child, hoisted));
        }
    }
    out
}

/// Drop rests the active freeze scope must not move (page-only freeze keeps overlay rests).
pub(crate) fn retain_unfrozen_scroll_rests(
    rests: &mut Vec<String>,
    freeze: ScrollFreeze,
    overlay_rests: &HashSet<String>,
) {
    rests.retain(|r| !freeze.applies_to_rest(overlay_rests.contains(r.as_str())));
}

/// Drop leftover smooth-scroll chase: `target = offset` so the next frame does not
/// interpolate. Clicking a text field after a wheel-to-clip must not dump extra
/// page motion when the focus-chrome frame finally presents (user did not scroll).
pub(crate) fn snap_scroll_targets_to_current_offsets<'a>(rests: impl IntoIterator<Item = &'a str>) {
    for rest in rests {
        let sx_key = crate::core::scroll_viewport::scroll_offset_x_signal_key(rest);
        let sy_key = crate::core::scroll_viewport::scroll_offset_y_signal_key(rest);
        let tx_key = crate::core::scroll_viewport::scroll_target_x_signal_key(rest);
        let ty_key = crate::core::scroll_viewport::scroll_target_y_signal_key(rest);
        let sx = crate::core::reactivity::use_signal(&sx_key, || 0.0_f32).peek();
        let sy = crate::core::reactivity::use_signal(&sy_key, || 0.0_f32).peek();
        crate::core::reactivity::use_signal(&tx_key, || sx).set_silent(sx);
        crate::core::reactivity::use_signal(&ty_key, || sy).set_silent(sy);
    }
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

/// Logical px per `MouseScrollDelta::LineDelta` unit (one line).
pub(crate) const WHEEL_LINE_HEIGHT_PX: f32 = 25.0;

/// Convert a winit wheel event into layout-space (logical) pixels.
///
/// winit 0.31 `LineDelta` is already **lines**. On Windows the backend
/// multiplies `SPI_GETWHEELSCROLLLINES` (`use_system_wheel_speed` defaults
/// on). Multiplying by [`get_system_scroll_lines`] again triples a notch
/// (0.30 sent ±1 per detent). Keyboard arrows still use the SPI helper.
///
/// `PixelDelta` is a physical position; divide by `ui_scale`.
pub(crate) fn wheel_delta_logical_px(
    delta: winit::event::MouseScrollDelta,
    ui_scale: f32,
) -> (f32, f32) {
    match delta {
        winit::event::MouseScrollDelta::LineDelta(dx, dy) => {
            (dx * WHEEL_LINE_HEIGHT_PX, dy * WHEEL_LINE_HEIGHT_PX)
        }
        winit::event::MouseScrollDelta::PixelDelta(p) => {
            let s = ui_scale.max(1e-6);
            (p.x as f32 / s, p.y as f32 / s)
        }
        _ => (0.0, 0.0),
    }
}

pub(crate) fn wheel_delta_for_scroll_range(dx: f32, dy: f32, max_x: f32, max_y: f32) -> (f32, f32) {
    if max_x > 0.5 && max_y <= 0.5 && dx.abs() <= f32::EPSILON {
        (dy, 0.0)
    } else {
        (dx, dy)
    }
}

#[cfg(test)]
#[path = "tests/wheel_delta.rs"]
mod wheel_delta_tests;

#[cfg(test)]
#[path = "tests/inertia.rs"]
mod inertia_tests;

#[cfg(test)]
#[path = "tests/smooth_scroll_step.rs"]
mod smooth_scroll_step_tests;

#[cfg(test)]
#[path = "tests/clamp_scroll_extent.rs"]
mod clamp_scroll_extent_tests;

#[cfg(test)]
#[path = "tests/wheel_sticky.rs"]
mod wheel_sticky_tests;

#[cfg(test)]
#[path = "tests/scroll_metrics_merge.rs"]
mod scroll_metrics_merge_tests;

#[cfg(test)]
#[path = "tests/visibility_deadline.rs"]
mod visibility_deadline_tests;

#[cfg(test)]
#[path = "tests/scroll_item_align.rs"]
mod scroll_item_align_tests;

#[cfg(test)]
#[path = "tests/hoisted_scroll_freeze.rs"]
mod hoisted_scroll_freeze_tests;

#[cfg(test)]
#[path = "tests/scroll_owner_pruning.rs"]
mod scroll_owner_pruning_tests;
