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

/// Collect only the topmost hit branch (deepest child path) under pointer.
/// Output order matches `collect_scroll_areas_at`: deepest scroll area first, then ancestors.
pub(super) fn collect_scroll_chain_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
) -> bool {
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return false;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    let mut child_hit = false;
    for child in el.children.iter().rev() {
        if collect_scroll_chain_at(child, x, y, next_acc, out) {
            child_hit = true;
            break;
        }
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

    child_hit || inside
}

/// Collect only scroll-area ids on the topmost hit branch (deepest first).
/// This avoids computing extent/metrics during wheel hit-testing; callers can
/// resolve metrics lazily (preferably from cache) only for candidate ids.
pub(super) fn collect_scroll_rest_chain_at(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
    out: &mut Vec<String>,
) -> bool {
    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0];
    let vy = b[1] - scroll_acc[1];
    let inside = x >= vx && x <= vx + b[2] && y >= vy && y <= vy + b[3];

    if (el.style.overflow.x != taffy::style::Overflow::Visible
        || el.style.overflow.y != taffy::style::Overflow::Visible)
        && !inside
    {
        return false;
    }

    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }

    let mut child_hit = false;
    for child in el.children.iter().rev() {
        if collect_scroll_rest_chain_at(child, x, y, next_acc, out) {
            child_hit = true;
            break;
        }
    }

    if let Some(id) = el.focus_id.as_deref() {
        if let Some(rest) = id.strip_prefix(crate::core::scroll_viewport::SCROLL_AREA_FOCUS_PREFIX)
        {
            out.push(rest.to_string());
        }
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

pub(super) fn set_scroll_offset_with_behavior(
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
    let _behavior = crate::core::reactivity::use_signal(&behavior_key, || "auto".to_string()).get();

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
