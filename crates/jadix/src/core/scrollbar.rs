//! Scrollbar runtime contracts shared by layout, paint and input.
//!
//! Signal keys, focus identifiers, geometry and live visibility live here.
//! The themed widget, variants, builder and RSX tag belong to the component
//! library, not these runtime contracts.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
    scroll_offset_y_signal_key, scroll_viewport_h_signal_key, scroll_viewport_w_signal_key,
};
use crate::core::tree::Element;

/// Bind caller-owned track and thumb elements to the scrolling runtime.
///
/// Initializes logical geometry, focus identifiers and visibility signals only;
/// colors, borders, radii, track placement and composition remain with the caller.
/// `axis` is `x` for horizontal scrolling and `y` otherwise. An unmeasured
/// viewport keeps real parts so the runtime can update them after first layout.
pub fn configure_scrollbar_parts(
    track: &mut Element,
    thumb: &mut Element,
    target: Option<&str>,
    axis: &str,
    visibility: &str,
    thickness: f32,
) {
    let horizontal = axis == "x";
    let axis = if horizontal { "x" } else { "y" };
    let visibility = if visibility.eq_ignore_ascii_case("always") {
        "always"
    } else if visibility.eq_ignore_ascii_case("never") {
        "never"
    } else {
        "auto"
    };
    let (viewport, content, offset) = target.map_or((0.0, 0.0, 0.0), |target| {
        let (viewport_key, content_key, offset_key) = if horizontal {
            (
                scroll_viewport_w_signal_key(target),
                scroll_content_w_signal_key(target),
                scroll_offset_x_signal_key(target),
            )
        } else {
            (
                scroll_viewport_h_signal_key(target),
                scroll_content_h_signal_key(target),
                scroll_offset_y_signal_key(target),
            )
        };
        use_signal(&scrollbar_visibility_signal_key(target, axis), || {
            visibility.to_string()
        })
        .set(visibility.to_string());
        track.focus_id = Some(format!("{SCROLLBAR_TRACK_FOCUS_PREFIX}{target}:{axis}"));
        thumb.focus_id = Some(format!("{SCROLLBAR_THUMB_FOCUS_PREFIX}{target}:{axis}"));
        (
            use_signal(&viewport_key, || 0.0_f32).peek(),
            use_signal(&content_key, || 0.0_f32).peek(),
            use_signal(&offset_key, || 0.0_f32).peek(),
        )
    });
    let cross = thickness.max(2.0);
    let thumb_cross = (cross - 2.0).max(1.0);
    let main = if viewport > 1.0 { viewport } else { 120.0 };
    let minimum = scrollbar_min_thumb_along_track(main);
    let total = content.max(viewport);
    let ratio = if total > 0.0 { viewport / total } else { 0.0 };
    let thumb_main = (main * ratio.clamp(0.0, 1.0)).clamp(minimum, main.max(minimum));
    let max_offset = (content - viewport).max(0.0);
    let progress = if max_offset > 0.0 {
        offset / max_offset
    } else {
        0.0
    };
    let position = (main - thumb_main).max(0.0) * progress.clamp(0.0, 1.0);
    let (track_w, track_h, thumb_w, thumb_h, left, top) = if horizontal {
        (main, cross, thumb_main, thumb_cross, position, 1.0)
    } else {
        (cross, main, thumb_cross, thumb_main, 1.0, position)
    };
    let geometry = Element::new()
        .w(track_w)
        .h(track_h)
        .min_w(track_w)
        .min_h(track_h);
    track.style.size = geometry.style.size;
    track.style.min_size = geometry.style.min_size;
    if horizontal {
        track.style.max_size.height = taffy::style::Dimension::length(cross);
    } else {
        track.style.max_size.width = taffy::style::Dimension::length(cross);
    }
    let geometry = Element::new()
        .absolute()
        .w(thumb_w)
        .h(thumb_h)
        .min_w(thumb_cross)
        .min_h(thumb_cross)
        .ml(left)
        .mt(top);
    thumb.style.position = geometry.style.position;
    thumb.style.size = geometry.style.size;
    thumb.style.min_size = geometry.style.min_size;
    thumb.style.margin = geometry.style.margin;
}

pub fn scrollbar_visibility_signal_key(target_id: &str, axis: &str) -> String {
    format!("__jadix_scrollbar_visibility_{}:{}", target_id, axis)
}

pub const SCROLLBAR_THUMB_FOCUS_PREFIX: &str = "scrollbar-thumb:";
pub const SCROLLBAR_TRACK_FOCUS_PREFIX: &str = "scrollbar-track:";
/// When `"0"`, `auto` fade skips track chrome (overlay thumb-only).
pub const ATTR_SCROLLBAR_TRACK_FADES: &str = "jadix_scrollbar_track_fades";
/// Height reveal clips content temporarily; it must not expose scrollbar chrome or hit slabs.
pub(crate) const ATTR_SCROLLBAR_REVEAL_HIDDEN: &str = "jadix_scrollbar_reveal_hidden";
pub(crate) const ATTR_SCROLLBAR_FLEX_TRACK: &str = "jadix_scrollbar_flex_track";
pub(crate) const ATTR_SCROLLBAR_GROUP: &str = "jadix_scrollbar_group";

/// Register a flex group containing a track and caller-owned end controls.
/// The runtime updates nested track geometry and collapses the entire group
/// when the target axis disables its scrollbar.
pub fn configure_scrollbar_group(
    group: &mut Element,
    track: &mut Element,
    target: &str,
    axis: &str,
) {
    group.set_attr(ATTR_SCROLLBAR_GROUP, format!("{target}:{axis}"));
    track.set_attr(ATTR_SCROLLBAR_FLEX_TRACK, "1");
}

pub const CHROME_ALPHA_EPS: f32 = 0.01;

/// Restore base alpha after build-time `auto` zeroing; skip intentionally transparent track chrome.
#[inline]
pub(crate) fn normalize_scrollbar_base_for_auto(
    vis: &str,
    track_fades: bool,
    track_border_width: f32,
    thumb_border_width: f32,
    track_bg: &mut [f32; 4],
    track_border: &mut [f32; 4],
    thumb_bg: &mut [f32; 4],
    thumb_border: &mut [f32; 4],
) {
    if vis != "auto" {
        return;
    }
    if track_fades && track_bg[3] <= CHROME_ALPHA_EPS {
        track_bg[3] = 1.0;
    }
    if thumb_bg[3] <= CHROME_ALPHA_EPS {
        thumb_bg[3] = 1.0;
    }
    if track_fades && track_border_width > 0.0 && track_border[3] <= CHROME_ALPHA_EPS {
        track_border[3] = 1.0;
    }
    if thumb_border_width > 0.0 && thumb_border[3] <= CHROME_ALPHA_EPS {
        thumb_border[3] = 1.0;
    }
}

/// Live overlay thumb geometry for paint-hash.
///
/// Overlay collect draws the rail from scroll signals, not Element styles / Taffy
/// margin. Fingerprint skip would otherwise replay the last thumb while the list
/// keeps scrolling — MessageScroller uses jump, so collect-skip stays allowed.
pub(crate) fn scrollbar_live_geometry_bits(el: &Element) -> Option<[u32; 6]> {
    let fid = el.focus_id.as_deref()?;
    let prefix = fid
        .strip_prefix(SCROLLBAR_THUMB_FOCUS_PREFIX)
        .or_else(|| fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX))?;
    let (rest, _axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
    let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
    let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
    let vw = use_signal(&scroll_viewport_w_signal_key(rest), || 0.0_f32).peek();
    let vh = use_signal(&scroll_viewport_h_signal_key(rest), || 0.0_f32).peek();
    let cw = use_signal(&scroll_content_w_signal_key(rest), || 0.0_f32).peek();
    let ch = use_signal(&scroll_content_h_signal_key(rest), || 0.0_f32).peek();
    Some([
        sx.to_bits(),
        sy.to_bits(),
        vw.to_bits(),
        vh.to_bits(),
        cw.to_bits(),
        ch.to_bits(),
    ])
}

/// Live `auto` visibility cubic for paint / paint-hash.
///
/// Overlay collect and fingerprint skip must sample this instead of baking alpha into
/// `style.bg_color`. Full prepass writes base colors; ViewportsOnly frames then fade
/// without Taffy thumb resize or hit-slab rewrite.
pub(crate) fn auto_scrollbar_live_paint_alpha(el: &Element) -> Option<f32> {
    let fid = el.focus_id.as_deref()?;
    let (rest, axis, is_thumb) =
        if let Some(prefix) = fid.strip_prefix(SCROLLBAR_THUMB_FOCUS_PREFIX) {
            let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
            (rest, axis, true)
        } else if let Some(prefix) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
            let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
            (rest, axis, false)
        } else {
            return None;
        };
    if el.attrs.contains_key(ATTR_SCROLLBAR_REVEAL_HIDDEN) {
        return Some(0.0);
    }
    let vis = use_signal(&scrollbar_visibility_signal_key(rest, axis), || {
        "auto".to_string()
    })
    .peek();
    if vis.trim().eq_ignore_ascii_case("never") {
        return Some(0.0);
    }
    if !vis.trim().eq_ignore_ascii_case("auto") {
        return None;
    }
    if !is_thumb {
        let fades = el.attrs.get(ATTR_SCROLLBAR_TRACK_FADES).map(String::as_str) != Some("0");
        if !fades {
            return None;
        }
    }
    Some(
        crate::style::use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:{axis}"), 0.0)
            .peek()
            .clamp(0.0, 1.0),
    )
}

thread_local! {
    /// Last node-present alpha per overlay auto-bar. Idle PartialLoad only redraws dirties;
    /// without this, a fade that omits alpha-0 quads leaves the last gray pixels on retain.
    static LAST_AUTO_BAR_PRESENT_ALPHA: RefCell<HashMap<String, f32>> =
        RefCell::new(HashMap::new());
}

/// Dest-reset / node-present while the cubic is moving, including the settle-to-zero frame.
/// Settled hidden and settled opaque do not punch the rail every idle tick.
pub(crate) fn auto_scrollbar_needs_node_present(el: &Element) -> bool {
    let Some(alpha) = auto_scrollbar_live_paint_alpha(el) else {
        return false;
    };
    let Some(fid) = el.focus_id.as_deref() else {
        return false;
    };
    let (rest, axis) = if let Some(prefix) = fid.strip_prefix(SCROLLBAR_THUMB_FOCUS_PREFIX) {
        prefix.rsplit_once(':').unwrap_or((prefix, "y"))
    } else if let Some(prefix) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
        prefix.rsplit_once(':').unwrap_or((prefix, "y"))
    } else {
        return false;
    };
    let fading =
        crate::style::use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:{axis}"), 0.0)
            .is_active();
    LAST_AUTO_BAR_PRESENT_ALPHA.with(|slot| {
        let mut last = slot.borrow_mut();
        let prev = last.get(fid).copied();
        let changed = match prev {
            None => true,
            Some(p) => (p - alpha).abs() > 1e-4,
        };
        if fading || changed {
            last.insert(fid.to_string(), alpha);
            true
        } else {
            false
        }
    })
}

/// Restore build-time `auto` zeroing so collect can paint live cubic without a Full
/// prepass writing `style.bg`. Transparent overlay tracks (`live_paint_alpha` = None)
/// stay at 0.
#[inline]
pub(crate) fn auto_scrollbar_restore_base_alpha(el: &Element, mut color: [f32; 4]) -> [f32; 4] {
    if auto_scrollbar_live_paint_alpha(el).is_none() {
        return color;
    }
    if color[3] <= CHROME_ALPHA_EPS {
        color[3] = 1.0;
    }
    color
}

/// Apply `auto` / `always` visibility alpha; track and thumb fade independently.
#[inline]
pub(crate) fn apply_scrollbar_visible_alpha(
    visible_alpha: f32,
    track_fades: bool,
    thumb_border_fades: bool,
    track_bg: &mut [f32; 4],
    thumb_bg: &mut [f32; 4],
    track_border: &mut [f32; 4],
    thumb_border: &mut [f32; 4],
) {
    let v = visible_alpha.clamp(0.0, 1.0);
    if track_fades {
        track_bg[3] *= v;
        track_border[3] *= v;
    }
    thumb_bg[3] *= v;
    if thumb_border_fades {
        thumb_border[3] *= v;
    }
}

/// 沿轨道方向的最小滑块长度。
///
/// 旧实现用 `track_inner.min(24)`：当嵌套在小盒子里 `track_inner < 24` 时，`min_thumb == track_inner`，
/// `clamp` 后滑块恒为满轨，比例严重失真。
#[inline]
pub(crate) fn scrollbar_min_thumb_along_track(track_inner: f32) -> f32 {
    let t = track_inner.max(1.0);
    let cap = (t * 0.18).min(20.0);
    cap.max(3.0).min(t * 0.9)
}

/// Track border-box → content-box origin (where Taffy lays out the thumb).
///
/// The track uses a 1px layout border and **no padding**: `thumb_cross = thickness - 2`
/// then sits in the remaining 8px inside a 10px BorderBox. Adding padding on top of the
/// border double-counts and shifts the thumb ~1px down-right.
#[inline]
pub(crate) fn scrollbar_track_content_inset_px(
    padding: f32,
    border_width: f32,
    has_border: bool,
) -> f32 {
    let pad = padding.max(0.0);
    let border = if has_border {
        border_width.max(0.0)
    } else {
        0.0
    };
    pad + border
}

#[inline]
pub(crate) fn scrollbar_track_content_inset(track: &Element) -> f32 {
    let pad = crate::layout::lp_length_px(track.style.padding.left)
        .filter(|px| px.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    let has_border = !track.style.border_style.is_invisible() && track.style.border_width > 0.0;
    scrollbar_track_content_inset_px(pad, track.style.border_width, has_border)
}

/// Baseline cross-axis size used by scrollbar runtime fallbacks.
pub const SCROLLBAR_DEFAULT_CROSS_PX: f32 = 10.0;

/// Gap between an overlay track and the scroll-area edge (and the other bar).
pub const SCROLLBAR_EDGE_PAD: f32 = 2.0;

/// Edge-only lane-side inset (no vertical thickness). Runtime adds `v_cross`
/// when [`vertical_scrollbar_occupies_lane`] is true.
pub const ATTR_H_LANE_INSET: &str = "jadix_h_scrollbar_lane_inset";

/// Vertical track cross size used when expanding the H bar's lane-side inset.
pub const ATTR_H_LANE_V_CROSS: &str = "jadix_h_scrollbar_v_cross";

/// Whether the vertical bar is actually on screen (L-join owns the corner).
///
/// Overflow *allowing* a bar is not enough: `auto` hides it when the content
/// fits. An unmeasured viewport must **not** reserve the lane — baking a gutter
/// for a bar that never appears (wrap=off textarea) left the H track short
/// forever under ViewportsOnly. Prefer a one-frame H overshoot into the corner
/// when Y later proves necessary over a permanent dead gutter.
pub(crate) fn vertical_scrollbar_occupies_lane(
    allow_y: bool,
    visibility: &str,
    viewport_h: f32,
    content_h: f32,
) -> bool {
    if !allow_y {
        return false;
    }
    let vis = visibility.trim();
    if vis.eq_ignore_ascii_case("never") {
        return false;
    }
    if vis.eq_ignore_ascii_case("always") {
        return true;
    }
    if viewport_h <= 1.0 {
        return false;
    }
    content_h > viewport_h + 0.5
}

/// Lane-side inset for an inside horizontal track.
///
/// `edge` is the non-lane inset from build time (`SCROLLBAR_EDGE_PAD`, or a
/// caller's pad such as textarea `right(2)`). When the vertical bar occupies
/// the corner, add `vertical_cross`. Only this side changes; the other side
/// stays as written. The painted box stays the Taffy box — stretching it to
/// `viewport_w` in collect put ink outside the fade dest-reset.
pub(crate) fn horizontal_lane_side_inset(edge: f32, vertical_cross: f32, lane_shown: bool) -> f32 {
    let edge = edge.max(0.0);
    if lane_shown {
        edge + vertical_cross.max(0.0)
    } else {
        edge
    }
}

/// Main-axis length when Taffy has not laid that axis yet.
///
/// The cross axis must already be the inset-anchored thickness. A zero cross
/// size means `bottom` / `right` sits on the far edge; inventing a bar there
/// paints outside the field, and the fade dest-reset then Region-clears
/// whatever that rect overlaps (sibling text, a neighbor focus ring).
pub(crate) fn overlay_main_length_when_unlaid(
    viewport_main: f32,
    inset_start: f32,
    inset_end: f32,
) -> Option<f32> {
    if viewport_main <= 1.0 {
        return None;
    }
    Some((viewport_main - inset_start.max(0.0) - inset_end.max(0.0)).max(1.0))
}

/// L-join corner policy (Cursor / modern VS Code): the **vertical** bar owns the
/// corner square while it is actually shown. The horizontal overlay stops before
/// that lane only then — a hidden vertical bar must not shorten the horizontal one.
///
/// Returns main-axis padding for an **inside** (absolute) track:
/// - vertical → `(top, bottom)` — full height, only container edge pad
/// - horizontal → `(left, right)` — edge pad plus `v_cross` on the side the vertical bar occupies
pub fn scrollbar_overlay_main_insets(
    axis: &str,
    this_outside: bool,
    vertical_shown: bool,
    vertical_on_left: bool,
    edge_pad: f32,
    vertical_cross: f32,
) -> (f32, f32) {
    if this_outside {
        return (0.0, 0.0);
    }
    match axis {
        "y" => (edge_pad, edge_pad),
        "x" if vertical_shown => {
            if vertical_on_left {
                (edge_pad + vertical_cross, edge_pad)
            } else {
                (edge_pad, edge_pad + vertical_cross)
            }
        }
        _ => (edge_pad, edge_pad),
    }
}

#[cfg(test)]
#[path = "tests/chrome.rs"]
mod chrome_tests;

#[cfg(test)]
#[path = "tests/content_inset.rs"]
mod content_inset_tests;

#[cfg(test)]
#[path = "tests/overlay_main_inset.rs"]
mod overlay_main_inset_tests;

#[cfg(test)]
#[path = "tests/part_configuration.rs"]
mod part_configuration_tests;
