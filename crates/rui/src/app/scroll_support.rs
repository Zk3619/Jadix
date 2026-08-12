use crate::core::tree::Element;

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
pub(super) fn decay_scroll_velocity(
    vx: f32,
    vy: f32,
    dt: f32,
    friction_per_sec: f32,
) -> [f32; 2] {
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

fn collect_descendant_extent(
    el: &Element,
    max_right: &mut f32,
    max_bottom: &mut f32,
    current_rest: &str,
) {
    // When scroll containers are nested, outer scrollable content size must not be affected
    // by inner scroll containers' internal descendants (otherwise max_y/max_x would "breathe"
    // during smooth scrolling, causing rubber-banding and preventing reaching the true end).
    //
    // So: include each child box, but do not recurse into nested ScrollArea contents.
    for c in &el.children {
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

        collect_descendant_extent(c, max_right, max_bottom, current_rest);
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

/// Visit children topmost-first (z_index, then DOM order), matching [`super::tree_hit`].
fn for_each_hit_child<'a>(
    el: &'a Element,
    mut f: impl FnMut(&'a Element) -> bool,
) -> bool {
    if super::tree_hit::children_share_same_z_index(el) {
        for child in el.children.iter().rev() {
            if f(child) {
                return true;
            }
        }
    } else {
        let order = super::tree_hit::hit_test_child_indices(el);
        for i in order.into_iter().rev() {
            if f(&el.children[i]) {
                return true;
            }
        }
    }
    false
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
    if !crate::core::tree::pointer_events_targets_children(el) {
        return false;
    }
    let scroll_acc = super::tree_hit::escape_scroll_acc(el, scroll_acc);
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return false;
    }

    let mut next_acc = scroll_acc;
    if !is_body_portal {
        if el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
    }

    let mut child_hit = false;
    if for_each_hit_child(el, |child| {
        collect_scroll_chain_at(child, x, y, next_acc, out)
    }) {
        child_hit = true;
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

    if crate::layout::layout_tree::portal_hit_passthrough(el)
        || !crate::core::tree::pointer_events_targets_self(el)
    {
        return child_hit;
    }
    child_hit || inside
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
    if !crate::core::tree::pointer_events_targets_children(el) {
        return false;
    }
    let scroll_acc = super::tree_hit::escape_scroll_acc(el, scroll_acc);
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    let is_body_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    if !is_body_portal
        && (el.style.overflow.x != taffy::style::Overflow::Visible
            || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return false;
    }

    let mut next_acc = scroll_acc;
    if !is_body_portal {
        if el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
    }

    let mut child_hit = false;
    if for_each_hit_child(el, |child| {
        collect_scroll_rest_chain_at(child, x, y, next_acc, out)
    }) {
        child_hit = true;
    }

    if let Some(id) = el.focus_id.as_deref() {
        if let Some(rest) = id.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
        {
            out.push(rest.to_string());
        }
    }

    if crate::layout::layout_tree::portal_hit_passthrough(el)
        || !crate::core::tree::pointer_events_targets_self(el)
    {
        return child_hit;
    }
    child_hit || inside
}

#[allow(dead_code)]
pub(super) fn find_scroll_area_metrics_by_rest(
    el: &Element,
    rest: &str,
) -> Option<([f32; 4], [f32; 2], [f32; 2])> {
    if el.focus_id.as_deref()
        == Some(&format!(
            "{}{}",
            crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX,
            rest
        ))
    {
        let b = el.layout_bounds;
        let mut max_right = b[0] + b[2];
        let mut max_bottom = b[1] + b[3];
        collect_descendant_extent(el, &mut max_right, &mut max_bottom, rest);
        let content_w = (max_right - b[0]).max(b[2]);
        let content_h = (max_bottom - b[1]).max(b[3]);
        let max_x = (content_w - b[2]).max(0.0);
        let max_y = (content_h - b[3]).max(0.0);
        return Some((b, [max_x, max_y], [content_w, content_h]));
    }
    for child in &el.children {
        if let Some(m) = find_scroll_area_metrics_by_rest(child, rest) {
            return Some(m);
        }
    }
    None
}

/// Clamp offset + target to the current scroll max.
///
/// When the viewport grows enough that `max → 0`, offset is forced to 0 for paint,
/// but a stale **target** above max must also be written back. Otherwise smooth-scroll
/// (gallery `scroll_behavior="smooth"`) will re-seek the old target after a later
/// shrink — top content jumps out of view instead of keeping the top and letting the
/// bottom overflow.
pub(super) fn clamp_scroll_offset_and_target(offset: f32, target: f32, max: f32) -> (f32, f32) {
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
    let tx = crate::core::reactivity::use_signal(&tx_key, || sx.get());
    let ty = crate::core::reactivity::use_signal(&ty_key, || sy.get());
    let _behavior = crate::core::reactivity::use_signal(&behavior_key, || {
        crate::core::scroll_viewport::DEFAULT_SCROLL_BEHAVIOR.to_string()
    })
    .get();

    if immediate {
        sx.set_silent(target_x);
        sy.set_silent(target_y);
    }
    tx.set_silent(target_x);
    ty.set_silent(target_y);
}


pub fn get_system_scroll_lines() -> f32 {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        };
        let mut lines: u32 = 3;
        unsafe {
            if SystemParametersInfoW(
                SPI_GETWHEELSCROLLLINES,
                0,
                &mut lines as *mut u32 as *mut _,
                0 as SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            ) != 0
            {
                // On Windows, -1 (u32::MAX) means scroll a whole page.
                // We'll treat it as a large number of lines for now or just return a special value.
                if lines == u32::MAX {
                    return 20.0; // Approximation for page scroll
                }
                return lines as f32;
            }
        }
    }
    3.0 // Default for other OS or failure
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
