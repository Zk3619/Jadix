use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::app::portal_overlay::ScrollFreeze;
use crate::core::tree::Element;
use crate::layout::layout_tree::ATTR_HOISTED_PORTAL;

/// 打开下拉后把某一项滚进视口（布局写出 content/viewport 后再应用，避免菜单 effect 订阅滚动信号）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollItemAlign {
    Start,
    Center,
    End,
}

/// When to apply a pending align.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollAlignWhen {
    /// Snap offset as soon as metrics exist (overlay measure / Expand pin).
    Before,
    /// Wait for a real clipped viewport, then honor the area's smooth/jump.
    After,
}

impl ScrollAlignWhen {
    pub(crate) fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "after" | "enter" | "animate" => Self::After,
            _ => Self::Before,
        }
    }
}

impl ScrollItemAlign {
    pub(crate) fn parse(s: &str) -> Option<Self> {
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

pub(crate) fn request_scroll_item_into_view(
    rest: impl Into<String>,
    index: usize,
    item_count: usize,
    align: ScrollItemAlign,
) {
    request_scroll_item_into_view_when(rest, index, item_count, align, ScrollAlignWhen::Before);
}

pub(crate) fn request_scroll_item_into_view_when(
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

pub(crate) fn request_scroll_descendant_into_view_when(
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
pub(crate) fn reset_scroll_item_into_view(rest: &str) {
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
    let Some(effective_vh) =
        effective_scroll_viewport_h_when(rest, viewport_h, content_h, pending.when)
    else {
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
        inside: &mut Option<[f32; 4]>,
        anywhere: &mut Option<[f32; 4]>,
    ) {
        let in_vp = in_vp || el.focus_id.as_deref() == Some(want);
        if node_matches_scroll_target(el, target) {
            let b = el.layout_bounds;
            if b[2] > 0.5 && b[3] > 0.5 {
                if in_vp {
                    *inside = Some(b);
                } else if anywhere.is_none() {
                    *anywhere = Some(b);
                }
            }
        }
        for c in &el.children {
            walk(c, target, want, in_vp, inside, anywhere);
        }
    }
    walk(root, target, &want, false, &mut inside, &mut anywhere);
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

#[cfg(test)]
mod wheel_delta_tests {
    use super::{wheel_delta_logical_px, WHEEL_LINE_HEIGHT_PX};
    use winit::dpi::PhysicalPosition;
    use winit::event::MouseScrollDelta;

    #[test]
    fn line_delta_is_lines_not_notches_times_spi() {
        // winit 0.31 Windows: one detent with SPI=3 arrives as LineDelta(0, 3).
        // Re-multiplying by SPI (old 0.30 path) would be 3 * 3 * 25 = 225.
        let (dx, dy) = wheel_delta_logical_px(MouseScrollDelta::LineDelta(0.0, 3.0), 1.5);
        assert!((dx).abs() < 1e-4);
        assert!((dy - 3.0 * WHEEL_LINE_HEIGHT_PX).abs() < 1e-4);
    }

    #[test]
    fn pixel_delta_is_physical_to_logical() {
        let (dx, dy) = wheel_delta_logical_px(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 150.0)),
            1.5,
        );
        assert!((dx).abs() < 1e-4);
        assert!((dy - 100.0).abs() < 1e-4);
    }
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
    use super::{snap_scroll_offset_to_device_px, wheel_scroll_target, SmoothScrollAxis};

    #[test]
    fn gentle_wheel_notch_accelerates_from_rest() {
        let mut motion = SmoothScrollAxis::new(0.0);
        let first = motion.step(0.0, 75.0, 0.016, 1.5).0;
        let second = motion.step(first, 75.0, 0.016, 1.5).0 - first;
        assert!(
            first > 0.0 && first <= 3.0,
            "gentle notch starts with a {first}px jump"
        );
        assert!(second > first, "start must ease in: {first} then {second}");
    }

    #[test]
    fn smooth_chase_is_independent_of_frame_rate() {
        let chase = |dt, frames| {
            let mut motion = SmoothScrollAxis::new(0.0);
            let mut y = 0.0;
            for _ in 0..frames {
                y = motion.step(y, 1000.0, dt, 1.5).0;
            }
            y
        };
        assert!((chase(0.016, 20) - chase(0.08, 4)).abs() <= 1.0);
        assert!((chase(1.0 / 240.0, 72) - chase(1.0 / 60.0, 18)).abs() <= 1.0);
    }

    #[test]
    fn rhythmic_notches_preserve_momentum_and_reverse_immediately() {
        let mut motion = SmoothScrollAxis::new(500.0);
        let mut y = 500.0;
        let mut target = y;
        for frame in 0..60 {
            if frame % 12 == 0 {
                target = wheel_scroll_target(y, target, -75.0, 10000.0);
            }
            let next = motion.step(y, target, 1.0 / 60.0, 1.5).0;
            assert!(next >= y && next <= target);
            assert!(next - y < 20.0, "rhythmic notch jumped {}px", next - y);
            y = next;
        }
        target = wheel_scroll_target(y, target, 75.0, 10000.0);
        let next = motion.step(y, target, 0.016, 1.5).0;
        assert!(next < y, "reverse input must cancel forward velocity");
    }

    #[test]
    fn quantized_chase_settles_without_overshoot_or_long_tail() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for distance in [0.4, 2.0, 16.0, 75.0, 240.0, -75.0] {
                let mut motion = SmoothScrollAxis::new(500.0);
                let target = snap_scroll_offset_to_device_px(500.0 + distance, scale);
                let mut y = 500.0;
                let mut done = false;
                for _ in 0..120 {
                    let (next, anim) = motion.step(y, target, 1.0 / 120.0, scale);
                    assert!((next * scale - (next * scale).round()).abs() < 0.001);
                    assert!((next - y) * distance >= -0.001);
                    assert!((target - next) * distance >= -0.001);
                    y = next;
                    if !anim {
                        done = true;
                        break;
                    }
                }
                assert!(done && y == target, "did not settle: {y} target={target}");
            }
        }
    }

    #[test]
    fn zero_elapsed_time_does_not_advance_and_external_jump_resets_motion() {
        let mut motion = SmoothScrollAxis::new(0.0);
        assert_eq!(motion.step(0.0, 75.0, 0.0, 1.5), (0.0, true));
        let y = motion.step(0.0, 75.0, 0.016, 1.5).0;
        assert!(y > 0.0);
        let jumped = motion.step(500.0, 575.0, 0.016, 1.5).0;
        assert!(jumped > 500.0 && jumped <= 503.0);
    }

    #[test]
    fn wheel_burst_during_stall_has_bounded_recovery_distance() {
        let current = 1000.0;
        let mut target = current;
        for _ in 0..15 {
            target = wheel_scroll_target(current, target, -75.0, 10000.0);
        }
        let mut motion = SmoothScrollAxis::new(current);
        let mut recovered = current;
        for _ in 0..4 {
            recovered = motion.step(recovered, target, 1.0 / 60.0, 1.5).0;
        }
        assert!(target - current <= 225.01);
        assert!(
            recovered - current <= 110.0,
            "recovery jumped {}px",
            recovered - current
        );
        assert_eq!(
            wheel_scroll_target(current, target, 75.0, 10000.0),
            current - 75.0
        );
    }

    #[test]
    fn wheel_target_preserves_normal_input_and_one_large_event() {
        assert_eq!(wheel_scroll_target(1000.0, 1000.0, -75.0, 10000.0), 1075.0);
        assert_eq!(wheel_scroll_target(1000.0, 1075.0, 0.0, 10000.0), 1075.0);
        assert_eq!(wheel_scroll_target(1000.0, 1000.0, -600.0, 10000.0), 1600.0);
        assert_eq!(wheel_scroll_target(50.0, 50.0, 75.0, 10000.0), 0.0);
        assert_eq!(wheel_scroll_target(9950.0, 9950.0, -75.0, 10000.0), 10000.0);
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
    fn snap_targets_cancel_leftover_smooth_chase() {
        use crate::core::reactivity::use_signal;
        use crate::core::scroll_viewport::{
            scroll_offset_y_signal_key, scroll_target_y_signal_key,
        };
        let rest = "snap-leftover-forms";
        use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).set_silent(80.0);
        use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32).set_silent(140.0);
        super::snap_scroll_targets_to_current_offsets([rest]);
        let ty = use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32).peek();
        let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
        assert!(
            (ty - sy).abs() < 0.01,
            "click must drop leftover target (ty={ty} sy={sy}) so unfreeze does not dump scroll"
        );
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
        has_pending_scroll_item_align, request_scroll_item_into_view, reset_scroll_item_into_view,
        scroll_offset_to_center_item, set_scroll_offset_with_behavior, ScrollItemAlign,
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

    #[test]
    fn pending_align_uses_placed_cap_immediately_while_expand_pins() {
        use super::clamp_max_y_for_pending_align;
        use crate::utils::popup::seed_floating_place_max_h_for_test;

        let _ = create_root(|| {
            let overlay_id = "cal-month";
            let rest = "cal-month:menu_scroll";
            cancel_scroll_item_into_view(rest);
            seed_floating_place_max_h_for_test(overlay_id, 200.0);
            request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
            let content_h = 12.0 * 32.0;
            let placed_vh = 200.0 - 8.0;
            // Expand pin: write against placed cap now (no post-open jump).
            assert!(apply_pending_scroll_item_into_view(
                rest, content_h, content_h
            ));
            assert!(!has_pending_scroll_item_align());
            let expected = scroll_offset_to_center_item(11, 32.0, placed_vh, 12);
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            assert!(
                (sy - expected).abs() < 0.5,
                "must center against placed cap while pinned, got {sy} expected {expected}"
            );
            assert!(
                (clamp_max_y_for_pending_align(rest, 0.0) - expected).abs() < 0.5,
                "pre-render clamp must not wipe the align while max_y is 0"
            );
            // Mid-expand layout max is only one row — still must keep the placed offset.
            request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
            assert!(apply_pending_scroll_item_into_view(
                rest,
                content_h - 32.0,
                content_h
            ));
            let sy2 =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            assert!(
                (sy2 - expected).abs() < 0.5,
                "mid-expand must not recenter against the tall viewport, got {sy2}"
            );
        });
    }

    #[test]
    fn pending_align_skips_expand_clip_when_content_fits_placed_cap() {
        use crate::utils::popup::seed_floating_place_max_h_for_test;

        let _ = create_root(|| {
            // ColorPicker format Select: 6 rows (~192px) fit in max_height 16rem.
            // DatePicker months overflow that cap, so they never hit this branch.
            let overlay_id = "cp-fmt";
            let rest = "cp-fmt:menu_scroll";
            cancel_scroll_item_into_view(rest);
            seed_floating_place_max_h_for_test(overlay_id, 256.0);
            request_scroll_item_into_view(rest, 5, 6, ScrollItemAlign::Center);
            let content_h = 6.0 * 32.0;
            let accordion_vh = 48.0;
            assert!(
                !apply_pending_scroll_item_into_view(rest, accordion_vh, content_h),
                "accordion clip is not a scroll viewport when the list fits the placed cap"
            );
            assert!(has_pending_scroll_item_align());
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            assert!(
                sy.abs() < 0.5,
                "must not recenter HSLA against the expanding clip, got {sy}"
            );
        });
    }

    #[test]
    fn pending_align_keeps_placed_cap_when_expand_viewport_is_a_sliver() {
        use crate::utils::popup::seed_floating_place_max_h_for_test;

        let _ = create_root(|| {
            let overlay_id = "cal-month-sliver";
            let rest = "cal-month-sliver:menu_scroll";
            cancel_scroll_item_into_view(rest);
            seed_floating_place_max_h_for_test(overlay_id, 200.0);
            request_scroll_item_into_view(rest, 11, 12, ScrollItemAlign::Center);
            let content_h = 12.0 * 32.0;
            let placed_vh = 200.0 - 8.0;
            // Expand pin-grandchildren: layout viewport is the accordion sliver.
            assert!(apply_pending_scroll_item_into_view(rest, 40.0, content_h));
            let expected = scroll_offset_to_center_item(11, 32.0, placed_vh, 12);
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            assert!(
                (sy - expected).abs() < 0.5,
                "must keep the placed-cap offset, not recenter in the sliver, got {sy} expected {expected}"
            );
        });
    }

    #[test]
    fn reset_scroll_item_into_view_keeps_offset() {
        let _ = create_root(|| {
            let rest = "cal-month:menu_scroll";
            cancel_scroll_item_into_view(rest);
            set_scroll_offset_with_behavior(rest, 0.0, 140.0, true);
            reset_scroll_item_into_view(rest);
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            assert!(
                (sy - 140.0).abs() < 0.5,
                "close must not wipe the menu offset, got {sy}"
            );
        });
    }

    #[test]
    fn pending_align_waits_while_viewport_matches_content() {
        let _ = create_root(|| {
            cancel_scroll_item_into_view("t-pin-align");
            request_scroll_item_into_view("t-pin-align", 8, 12, ScrollItemAlign::Center);
            let content_h = 12.0 * 32.0;
            assert!(!apply_pending_scroll_item_into_view(
                "t-pin-align",
                content_h,
                content_h
            ));
            assert!(has_pending_scroll_item_align());
            // Select rebuilds the open menu every frame; re-request must not
            // reset the clip wait or we never survive Expand pin.
            request_scroll_item_into_view("t-pin-align", 8, 12, ScrollItemAlign::Center);
            assert!(!apply_pending_scroll_item_into_view(
                "t-pin-align",
                content_h,
                content_h
            ));
            assert!(apply_pending_scroll_item_into_view(
                "t-pin-align",
                256.0,
                content_h
            ));
            assert!(!has_pending_scroll_item_align());
            let sy = crate::core::reactivity::use_signal(
                &scroll_offset_y_signal_key("t-pin-align"),
                || 0.0_f32,
            )
            .peek();
            let expected = scroll_offset_to_center_item(8, 32.0, 256.0, 12);
            assert!(
                (sy - expected).abs() < 0.5,
                "offset after unpin must center, got {sy} expected {expected}"
            );
        });
    }

    #[test]
    fn align_rect_centers_variable_height_child() {
        let viewport = [10.0, 20.0, 120.0, 200.0];
        let target = [10.0, 20.0 + 288.0, 120.0, 48.0];
        let max = [0.0, (12.0 * 32.0 + 48.0) - 200.0];
        let [_, sy] = super::scroll_offset_to_align_rect(
            viewport,
            target,
            [0.0, 0.0],
            max,
            ScrollItemAlign::Center,
        );
        let expected = (288.0_f32 + 24.0 - 100.0).clamp(0.0, max[1]);
        assert!(
            (sy - expected).abs() < 0.5,
            "variable-height child must center, got {sy} expected {expected}"
        );
    }

    #[test]
    fn descendant_align_uses_layout_bounds() {
        use crate::core::scroll_viewport::focus_id_for_scroll_viewport;
        use crate::core::tree::Element;
        let _ = create_root(|| {
            let rest = "custom-list";
            cancel_scroll_item_into_view(rest);
            super::request_scroll_descendant_into_view(rest, "row-9", ScrollItemAlign::Center);
            let mut row = Element::new().template_id("row-9");
            row.layout_bounds = [0.0, 288.0, 100.0, 32.0];
            let mut vp = Element::new().focus_id(focus_id_for_scroll_viewport(rest));
            vp.layout_bounds = [0.0, 0.0, 100.0, 200.0];
            vp.children.push(row);
            let mut root = Element::new();
            root.children.push(vp);
            let found = super::resolve_pending_descendant_bounds(&root);
            let content_h = 12.0 * 32.0;
            assert!(super::apply_pending_scroll_align(
                rest,
                [0.0, 0.0, 100.0, 200.0],
                100.0,
                content_h,
                found.get(rest).copied(),
            ));
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            let expected = super::scroll_offset_to_align_rect(
                [0.0, 0.0, 100.0, 200.0],
                [0.0, 288.0, 100.0, 32.0],
                [0.0, 0.0],
                [0.0, content_h - 200.0],
                ScrollItemAlign::Center,
            )[1];
            assert!(
                (sy - expected).abs() < 0.5,
                "descendant align must use layout box, got {sy} expected {expected}"
            );
        });
    }

    #[test]
    fn after_smooth_writes_target_not_offset() {
        use crate::core::scroll_viewport::{
            scroll_behavior_signal_key, scroll_target_y_signal_key, SCROLL_BEHAVIOR_SMOOTH,
        };
        let _ = create_root(|| {
            let rest = "after-smooth";
            cancel_scroll_item_into_view(rest);
            crate::core::reactivity::use_signal(&scroll_behavior_signal_key(rest), || {
                SCROLL_BEHAVIOR_SMOOTH.to_string()
            })
            .set_silent(SCROLL_BEHAVIOR_SMOOTH.to_string());
            super::request_scroll_item_into_view_when(
                rest,
                8,
                12,
                ScrollItemAlign::Center,
                super::ScrollAlignWhen::After,
            );
            let content_h = 12.0 * 32.0;
            assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            let ty =
                crate::core::reactivity::use_signal(&scroll_target_y_signal_key(rest), || 0.0_f32)
                    .peek();
            let expected = scroll_offset_to_center_item(8, 32.0, 200.0, 12);
            assert!(
                sy.abs() < 0.5,
                "after+smooth must not snap offset, got {sy}"
            );
            assert!(
                (ty - expected).abs() < 0.5,
                "after+smooth must chase target, got {ty} expected {expected}"
            );
        });
    }

    #[test]
    fn after_jump_snaps_offset() {
        use crate::core::scroll_viewport::{scroll_behavior_signal_key, SCROLL_BEHAVIOR_JUMP};
        let _ = create_root(|| {
            let rest = "after-jump";
            cancel_scroll_item_into_view(rest);
            crate::core::reactivity::use_signal(&scroll_behavior_signal_key(rest), || {
                SCROLL_BEHAVIOR_JUMP.to_string()
            })
            .set_silent(SCROLL_BEHAVIOR_JUMP.to_string());
            super::request_scroll_item_into_view_when(
                rest,
                8,
                12,
                ScrollItemAlign::Center,
                super::ScrollAlignWhen::After,
            );
            let content_h = 12.0 * 32.0;
            assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
            let sy =
                crate::core::reactivity::use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                    .peek();
            let expected = scroll_offset_to_center_item(8, 32.0, 200.0, 12);
            assert!(
                (sy - expected).abs() < 0.5,
                "after+jump must snap, got {sy} expected {expected}"
            );
        });
    }

    #[test]
    fn after_waits_for_real_clip_not_placed_cap() {
        use crate::utils::popup::seed_floating_place_max_h_for_test;
        let _ = create_root(|| {
            let overlay_id = "after-pin";
            let rest = "after-pin:menu_scroll";
            cancel_scroll_item_into_view(rest);
            seed_floating_place_max_h_for_test(overlay_id, 200.0);
            super::request_scroll_item_into_view_when(
                rest,
                11,
                12,
                ScrollItemAlign::Center,
                super::ScrollAlignWhen::After,
            );
            let content_h = 12.0 * 32.0;
            assert!(
                !apply_pending_scroll_item_into_view(rest, content_h, content_h),
                "after must wait until expand unpins"
            );
            assert!(has_pending_scroll_item_align());
            assert!(apply_pending_scroll_item_into_view(rest, 200.0, content_h));
        });
    }
}

#[cfg(test)]
mod hoisted_scroll_freeze_tests {
    use super::{collect_hoisted_scroll_rests, retain_unfrozen_scroll_rests};
    use crate::app::portal_overlay::ScrollFreeze;
    use crate::core::tree::Element;
    use crate::layout::layout_tree::ATTR_HOISTED_PORTAL;

    #[test]
    fn collect_hoisted_scroll_rests_skips_page_viewports() {
        let mut page_vp = Element::new();
        page_vp.focus_id = Some("scroll:page".into());
        let mut overlay = Element::new();
        overlay.set_attr(ATTR_HOISTED_PORTAL, "1");
        let mut menu_vp = Element::new();
        menu_vp.focus_id = Some("scroll:menu".into());
        overlay.children.push(menu_vp);
        page_vp.children.push(overlay);

        let hoisted = collect_hoisted_scroll_rests(&page_vp);
        assert!(hoisted.contains("menu"));
        assert!(!hoisted.contains("page"));

        let mut rests = vec!["page".into(), "menu".into()];
        retain_unfrozen_scroll_rests(&mut rests, ScrollFreeze::Page, &hoisted);
        assert_eq!(rests, vec!["menu".to_string()]);
    }
}
