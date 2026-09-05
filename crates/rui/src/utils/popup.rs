//! 浮层几何与 Portal 辅助（**能力层 / 行为工具**，非样式组件）。
//!
//! 按 [`docs/COMPONENT_LIBRARY.md`](../../../../docs/COMPONENT_LIBRARY.md)：本模块只做
//! `mark_floating`、锚点 placement、viewport 钳位、underlay/portal 标记。
//! **产品挂载只走两条入口**：锚定浮层 [`mount_anchored_overlay`]；视口模态舞台
//! [`mount_viewport_portal_stage`]（+ 可选 [`mount_modal_layer`]）。禁止组件再手写
//! 第二套坐标 sync 或嵌套 `rui_portal`。
//! **禁止**写入某一组件的外观常量（如 Select `8rem`、Combobox `max-h-96`）；
//! 那些由 `components/*` 通过 attr / style 传入。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::app::window_scope::{self, WindowScope};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX;
use crate::core::tree::Element;
use crate::layout::layout_tree::{ATTR_ESCAPE_OVERFLOW_CLIP, ATTR_PORTAL_KEY, PORTAL_LAYER_ID};
use crate::primitives::overlay::overlay;

thread_local! {
    /// After opening an overlay, ignore underlay closes until the pointer is released.
    /// Prevents the opening press (or a same-cycle re-hit on the new underlay) from
    /// immediately dismissing the popover.
    ///
    /// Keyed by [`WindowScope`] so primary Dialog arm state cannot be toggled by
    /// secondary-window clicks (and vice versa).
    static UNDERLAY_CLOSE_ARMED: RefCell<HashMap<WindowScope, bool>> = RefCell::new(HashMap::new());
}

fn armed_map_set(armed: bool) {
    let scope = window_scope::active();
    UNDERLAY_CLOSE_ARMED.with(|m| {
        m.borrow_mut().insert(scope, armed);
    });
}

fn armed_map_get() -> bool {
    let scope = window_scope::active();
    UNDERLAY_CLOSE_ARMED.with(|m| *m.borrow().get(&scope).unwrap_or(&true))
}

/// Call when an overlay transitions to open (before/with `open.set(true)`).
pub fn disarm_underlay_close() {
    armed_map_set(false);
}

/// Call on primary-button release so the next outside click can dismiss.
pub fn arm_underlay_close() {
    armed_map_set(true);
}

/// Whether outside-press / underlay dismiss is currently allowed.
#[inline]
pub fn underlay_close_is_armed() -> bool {
    armed_map_get()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    /// Prefer below; align panel center with trigger center (edge-clamped).
    BottomCenter,
    TopStart,
    TopEnd,
    /// Prefer above; align panel center with trigger center (edge-clamped).
    TopCenter,
    /// Prefer to the right of the trigger; top-aligned (submenu).
    RightStart,
    /// Prefer to the left of the trigger; top-aligned.
    LeftStart,
}

impl PopupPlacement {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::BottomCenter => "bottom",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
            Self::TopCenter => "top",
            Self::RightStart => "right-start",
            Self::LeftStart => "left-start",
        }
    }

    pub fn from_attr(s: &str) -> Self {
        match s {
            "bottom-end" => Self::BottomEnd,
            "bottom" | "bottom-center" => Self::BottomCenter,
            "top-start" => Self::TopStart,
            "top-end" => Self::TopEnd,
            "top" | "top-center" => Self::TopCenter,
            "right" | "right-start" => Self::RightStart,
            "left" | "left-start" => Self::LeftStart,
            _ => Self::BottomStart,
        }
    }

    /// ShadCN `side` + `align` → placement（`bottom-start` 等组合写在 `side` 时忽略 `align`）。
    pub fn from_side_align(side: &str, align: &str) -> Self {
        let side = side.trim().to_ascii_lowercase();
        let align = align.trim().to_ascii_lowercase();
        if side.contains('-') {
            return Self::from_attr(&side);
        }
        match (side.as_str(), align.as_str()) {
            ("bottom", "start") => Self::BottomStart,
            ("bottom", "end") => Self::BottomEnd,
            ("bottom", _) => Self::BottomCenter,
            ("top", "start") => Self::TopStart,
            ("top", "end") => Self::TopEnd,
            ("top", _) => Self::TopCenter,
            ("right", _) => Self::RightStart,
            ("left", _) => Self::LeftStart,
            _ => Self::BottomStart,
        }
    }

    fn is_horizontal(self) -> bool {
        matches!(self, Self::RightStart | Self::LeftStart)
    }
}

/// Attr keys for the unified floating placement engine (geometry contract only).
pub const ATTR_FLOATING: &str = "rui_floating";
pub const ATTR_FLOATING_TRIGGER: &str = "rui_floating_trigger";
pub const ATTR_FLOATING_PLACEMENT: &str = "rui_floating_placement";
/// Definite panel width (`px` / `rem`). Takes precedence over match-trigger.
pub const ATTR_FLOATING_W: &str = "rui_floating_w";
/// Optional max-height cap (`px` / `rem`). Intersected with viewport-available height.
pub const ATTR_FLOATING_MAX_H: &str = "rui_floating_max_h";
/// `1` → panel width equals trigger; `0` → content width (`width: auto`) with min-width floor.
pub const ATTR_FLOATING_MATCH_TRIGGER: &str = "rui_floating_match_trigger";
/// Optional fixed height (`px` / `rem`).
pub const ATTR_FLOATING_H: &str = "rui_floating_h";
/// Optional min-height (`px` / `rem`). Applied only when set — no style default here.
pub const ATTR_FLOATING_MIN_H: &str = "rui_floating_min_h";
/// Optional min-width: absolute (`8rem`) or relative to trigger (`trigger`, `trigger+1.75rem`).
pub const ATTR_FLOATING_MIN_W: &str = "rui_floating_min_w";
/// Optional per-panel gap in px between trigger and floating panel (default [`FLOATING_TRIGGER_GAP`]).
pub const ATTR_FLOATING_GAP: &str = "rui_floating_gap";
/// `flip`（默认）：视口内翻转并按剩余空间夹 `max_h`。`fixed`：钉在触发器首选侧，尺寸不随滚动挤压（可溢出视口）。
pub const ATTR_FLOATING_STRATEGY: &str = "rui_floating_strategy";
/// Cursor / point anchor `"x,y"`（逻辑像素）。有值时忽略触发器几何，走 [`compute_cursor_menu_placement`]。
pub const ATTR_FLOATING_POINT: &str = "rui_floating_point";
/// Intrinsic content height (logical px) for placement `estimate_h` while accordion clips.
pub(crate) const ATTR_FLOATING_CONTENT_H: &str = "rui_floating_content_h";
/// Expand is taking an unconstrained measure frame — placement must not write viewport `max_h`.
pub(crate) const ATTR_FLOATING_EXPAND_MEASURE: &str = "rui_floating_expand_measure";
/// Trigger-facing edge y when the panel is placed above; accordion `top` tracks visual height from this.
pub(crate) const ATTR_FLOATING_ANCHOR_EDGE_Y: &str = "rui_floating_anchor_edge_y";
/// Expand enter/exit baseline height (logical px). When set, accordion clips from this height
/// instead of 0 so covered triggers (e.g. bubble reaction bar) continue downward/upward.
pub(crate) const ATTR_FLOATING_EXPAND_FROM_H: &str = "rui_floating_expand_from_h";
/// Expand enter/exit baseline width (logical px). When set, `max-width` interpolates with the
/// accordion so row-wise content (e.g. AvatarGroup +N) is clipped instead of popping in/out.
pub(crate) const ATTR_FLOATING_EXPAND_FROM_W: &str = "rui_floating_expand_from_w";
/// When `"1"` with [`ATTR_FLOATING_EXPAND_FROM_W`], lerp `max-width` and `nowrap` (horizontal
/// clip). When unset, pin width to `from_w` so wrapped rows unveil via `max-height` (Bubble +N).
pub(crate) const ATTR_FLOATING_EXPAND_CLIP_X: &str = "rui_floating_expand_clip_x";

/// Inset from the viewport edges so floating panels never sit flush against the window.
pub const FLOATING_VIEWPORT_MARGIN: f32 = 8.0;
/// Default gap between the trigger and the floating panel (placement geometry).
pub const FLOATING_TRIGGER_GAP: f32 = 8.0;

/// Tag a portal panel for post-layout anchored placement.
pub fn mark_floating(
    el: &mut Element,
    id: &str,
    trigger_focus_id: &str,
    placement: PopupPlacement,
) {
    el.set_attr(ATTR_FLOATING, id);
    el.set_attr(ATTR_FLOATING_TRIGGER, trigger_focus_id);
    el.set_attr(ATTR_FLOATING_PLACEMENT, placement.as_attr());
}

/// Like [`mark_floating`], with an explicit trigger–panel gap in px (e.g. tooltip `sideOffset`/`gutter` ≈ 4).
///
/// Negative `gap_px` overlaps the panel onto the trigger. Submenus use a positive gap for the
/// visual seam while keeping the floating silhouette (shadow host) itself unpadded.
pub fn mark_floating_with_gap(
    el: &mut Element,
    id: &str,
    trigger_focus_id: &str,
    placement: PopupPlacement,
    gap_px: f32,
) {
    mark_floating(el, id, trigger_focus_id, placement);
    let g = gap_px.clamp(-64.0, 64.0);
    el.set_attr(ATTR_FLOATING_GAP, format!("{g}"));
}

/// Context-menu style: expand from a viewport point (typically the cursor).
///
/// Still records `trigger_focus_id` for outside-press / path hit helpers.
pub fn mark_floating_at_point(
    el: &mut Element,
    id: &str,
    trigger_focus_id: &str,
    x: f32,
    y: f32,
    gap_px: f32,
) {
    mark_floating(el, id, trigger_focus_id, PopupPlacement::BottomStart);
    el.set_attr(ATTR_FLOATING_POINT, format!("{x},{y}"));
    el.set_attr(ATTR_FLOATING_GAP, format!("{}", gap_px.max(0.0)));
}

fn floating_point_of(el: &Element) -> Option<[f32; 2]> {
    let raw = el.attrs.get(ATTR_FLOATING_POINT)?;
    let (xs, ys) = raw.split_once(',')?;
    let x: f32 = xs.trim().parse().ok()?;
    let y: f32 = ys.trim().parse().ok()?;
    if x.is_finite() && y.is_finite() {
        Some([x, y])
    } else {
        None
    }
}

/// 以光标点为原点向右下展开。策略顺序：首选位置 → **换向** → **视口内平移** → 夹 `max_h`（内部滚动）。
///
/// 返回 `(x, y, max_h)`；`max_h` 为该位置下可用高度，供面板内 ScrollArea 使用。
pub fn compute_cursor_menu_placement(
    cursor: [f32; 2],
    viewport: [f32; 2],
    menu_w: f32,
    menu_h: f32,
    gap: f32,
) -> (f32, f32, f32) {
    let [px, py] = cursor;
    let [vw, vh] = viewport;
    let margin = FLOATING_VIEWPORT_MARGIN;
    let gap = gap.max(0.0);
    let w = menu_w.max(1.0);
    let h = menu_h.max(1.0);
    let avail_h = (vh - 2.0 * margin).max(1.0);

    let space_right = (vw - margin - (px + gap)).max(0.0);
    let space_left = (px - gap - margin).max(0.0);
    let space_below = (vh - margin - (py + gap)).max(0.0);
    let space_above = (py - gap - margin).max(0.0);

    // Preferred: grow bottom-right from the cursor.
    let mut x = px + gap;
    let mut y = py + gap;

    // 1) Flip when the preferred side of the cursor cannot hold the menu and the
    //    opposite side has more room (keep the menu attached to the cursor).
    if space_right + 0.5 < w && space_left > space_right {
        x = px - gap - w;
    }
    if space_below + 0.5 < h && space_above > space_below {
        y = py - gap - h;
    }

    // 2) Translate into the viewport (full size when possible).
    if x + w > vw - margin {
        x = (vw - margin - w).max(margin);
    }
    if x < margin {
        x = margin;
    }
    if y + h > vh - margin {
        y = (vh - margin - h).max(margin);
    }
    if y < margin {
        y = margin;
    }

    // 3) Clamp height for internal scrolling.
    let max_h = (vh - margin - y).max(48.0).min(avail_h).max(1.0);
    (x, y, max_h)
}

/// 根据触发器矩形与视口，计算浮层左上角与可用高度（优先下方/上方，空间不足则翻转）。
///
/// Always keeps [`FLOATING_VIEWPORT_MARGIN`] from the viewport edges and `gap` from the
/// trigger so the panel never sits flush against either.
/// Center placements keep the panel mid-x aligned with the trigger mid-x when possible,
/// otherwise clamp to the viewport edge while staying as close as possible.
pub fn compute_anchored_placement(
    trigger: [f32; 4],
    viewport: [f32; 2],
    menu_w: f32,
    menu_h_estimate: f32,
    preferred: PopupPlacement,
    gap: f32,
) -> (f32, f32, f32, bool) {
    let [tx, ty, tw, th] = trigger;
    let [vw, vh] = viewport;
    let margin = FLOATING_VIEWPORT_MARGIN;
    // Negative gap = overlap (submenu seam); keep a sane clamp.
    let gap = gap.clamp(-32.0, 64.0);
    let w = if menu_w > 0.0 { menu_w } else { tw };
    let h = menu_h_estimate.max(1.0);

    // Horizontal primary axis (submenu): flip → translate → clamp max_h.
    if preferred.is_horizontal() {
        let space_right = (vw - margin - (tx + tw) - gap).max(0.0);
        let space_left = (tx - margin - gap).max(0.0);
        let prefer_right = matches!(preferred, PopupPlacement::RightStart);
        let mut place_right = if prefer_right {
            space_right + 0.5 >= w.min(80.0) || space_right >= space_left
        } else {
            !(space_left + 0.5 >= w.min(80.0) || space_left >= space_right)
        };
        // Flip when preferred side cannot hold the panel and the opposite has more room.
        if prefer_right && space_right + 0.5 < w && space_left > space_right {
            place_right = false;
        } else if !prefer_right && space_left + 0.5 < w && space_right > space_left {
            place_right = true;
        }
        let mut x = if place_right {
            tx + tw + gap
        } else {
            tx - gap - w
        };
        let mut y = ty;
        if x + w > vw - margin {
            x = (vw - margin - w).max(margin);
        }
        if x < margin {
            x = margin;
        }
        if y + h > vh - margin {
            y = (vh - margin - h).max(margin);
        }
        if y < margin {
            y = margin;
        }
        let max_h = (vh - margin - y).max(48.0);
        return (x, y, max_h, !place_right);
    }

    let prefer_bottom = matches!(
        preferred,
        PopupPlacement::BottomStart | PopupPlacement::BottomEnd | PopupPlacement::BottomCenter
    );
    let prefer_end = matches!(
        preferred,
        PopupPlacement::BottomEnd | PopupPlacement::TopEnd
    );
    let prefer_center = matches!(
        preferred,
        PopupPlacement::BottomCenter | PopupPlacement::TopCenter
    );

    let space_below = (vh - margin - (ty + th) - gap).max(0.0);
    let space_above = (ty - margin - gap).max(0.0);
    // Prefer the requested side when it can hold the full content. Otherwise flip
    // to the opposite side when it fits — or, when neither fits, always pick the
    // roomier side so overflow scroll shows as much content as possible.
    let need = h;
    let mut place_below = prefer_bottom;
    if prefer_bottom {
        if space_below + 0.5 < need && space_above > space_below {
            place_below = false;
        }
    } else if space_above + 0.5 < need && space_below > space_above {
        place_below = true;
    }

    let max_h = if place_below {
        space_below
    } else {
        space_above
    }
    .max(48.0);
    let y = if place_below {
        ty + th + gap
    } else {
        (ty - gap - menu_h_estimate.min(max_h)).max(margin)
    };

    let mut x = if prefer_center {
        tx + (tw - w) * 0.5
    } else if prefer_end {
        tx + tw - w
    } else {
        tx
    };
    let x_max = (vw - margin - w).max(margin);
    if x + w > vw - margin {
        x = x_max;
    }
    if x < margin {
        x = margin;
    }
    (x, y, max_h, !place_below)
}

/// `strategy=fixed`：钉在触发器首选侧，不翻转、不按剩余视口夹高度（滚动时跟触发器走，可溢出窗口）。
///
/// 负 `gap` 让面板叠在触发器上（例如 reaction 展开就地盖住折叠条）；多余高度仍只朝首选侧生长。
fn compute_fixed_anchor(
    trigger: [f32; 4],
    viewport: [f32; 2],
    menu_w: f32,
    menu_h_estimate: f32,
    preferred: PopupPlacement,
    gap: f32,
) -> (f32, f32) {
    let [tx, ty, tw, th] = trigger;
    let [vw, _] = viewport;
    let margin = FLOATING_VIEWPORT_MARGIN;
    let cover = gap < 0.0;
    let gap = if cover { -th } else { gap.max(0.0) };
    let w = if menu_w > 0.0 { menu_w } else { tw };
    let prefer_bottom = matches!(
        preferred,
        PopupPlacement::BottomStart | PopupPlacement::BottomEnd | PopupPlacement::BottomCenter
    );
    let prefer_end = matches!(
        preferred,
        PopupPlacement::BottomEnd | PopupPlacement::TopEnd
    );
    let prefer_center = matches!(
        preferred,
        PopupPlacement::BottomCenter | PopupPlacement::TopCenter
    );
    let y = if prefer_bottom {
        ty + th + gap
    } else {
        ty - gap - menu_h_estimate.max(0.0)
    };
    let mut x = if prefer_center {
        tx + (tw - w) * 0.5
    } else if prefer_end {
        tx + tw - w
    } else {
        tx
    };
    let x_max = (vw - margin - w).max(margin);
    if !cover {
        if x + w > vw - margin {
            x = x_max;
        }
        if x < margin {
            x = margin;
        }
    }
    (x, y)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FloatingPlace {
    x: f32,
    y: f32,
    /// Definite width when [`Self::width_auto`] is false; otherwise the min-width floor.
    w: f32,
    max_h: f32,
    /// Content-sized width (`width: auto`); `w` / `min_w` supply the floor.
    width_auto: bool,
    /// Extra min-width (≥ `w` when matching trigger with a larger floor).
    min_w: f32,
}

impl Default for FloatingPlace {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 160.0,
            max_h: 256.0,
            width_auto: false,
            min_w: 0.0,
        }
    }
}

fn floating_place_signal_key(id: &str) -> String {
    format!("{id}__floating_place")
}

fn inset_length(v: taffy::style::LengthPercentageAuto) -> Option<f32> {
    crate::layout::lpa_length_px(v)
}

fn dimension_length(v: taffy::style::Dimension) -> Option<f32> {
    crate::layout::dim_length_px(v)
}

fn height_keyframe_animation_active(el: &Element) -> bool {
    el.style.animations.iter().any(|a| {
        matches!(
            a.name.as_str(),
            "accordion-down" | "accordion-up" | "collapsible-down" | "collapsible-up"
        ) || a.name.starts_with("rui-expand-down-")
            || a.name.starts_with("rui-expand-up-")
    })
}

fn expand_measure_pending(el: &Element) -> bool {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_MEASURE)
        .is_some_and(|s| s == "1")
}

/// Placement must not own `max-size.height` while accordion keyframes run, or while
/// Expand is measuring unconstrained content height.
fn skip_placement_max_h(el: &Element) -> bool {
    height_keyframe_animation_active(el) || expand_measure_pending(el)
}

fn floating_estimate_h(el: &Element) -> f32 {
    if let Some(h) = el
        .attrs
        .get(ATTR_FLOATING_CONTENT_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 0.5)
    {
        return h;
    }
    if el.layout_bounds[3] > 0.5 {
        el.layout_bounds[3]
    } else {
        32.0
    }
}

/// Last synced viewport-available `max_h` for this overlay, if placement has run.
pub(crate) fn peek_floating_place_max_h(id: &str) -> Option<f32> {
    let key = floating_place_signal_key(id);
    let place = use_signal(key.as_str(), FloatingPlace::default);
    let p = place.peek();
    if p == FloatingPlace::default() {
        return None;
    }
    Some(p.max_h)
}

/// After accordion tick: clip interpolated max-height to the current viewport cap
/// without retargeting the keyframe baseline (keep `snap.base`, sync `snap.written`).
/// When placed above, also glue `top` to the trigger-facing edge so expand/collapse
/// reverse instead of leaving a sliver below the trigger.
pub(crate) fn clamp_animated_max_h_to_place(el: &mut Element, overlay_id: &str) -> bool {
    if !height_keyframe_animation_active(el) {
        return false;
    }
    let mut changed = false;
    if let Some(cap) = peek_floating_place_max_h(overlay_id).filter(|h| *h > 0.0) {
        if let Some(cur) = dimension_length(el.style.max_size.height) {
            if cur > cap + 0.5 {
                let capped = taffy::style::Dimension::length(cap);
                el.style.max_size.height = capped;
                el.animation_snapshot.sync_written_max_height(capped);
                changed = true;
            }
        }
    }
    let edge = el
        .attrs
        .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
        .and_then(|s| s.parse::<f32>().ok());
    if let Some(edge) = edge {
        let visual_h = dimension_length(el.style.max_size.height).unwrap_or(0.0);
        let y = (edge - visual_h).max(FLOATING_VIEWPORT_MARGIN);
        if inset_length(el.style.inset.top).is_none_or(|v| (v - y).abs() > 0.5) {
            el.style.inset.top = Element::into_margin_auto(y);
            changed = true;
        }
    }
    changed
}

/// Write absolute geometry onto a floating panel. Returns whether any field changed.
///
/// Capability only: applies placement numbers + attr-driven size constraints.
/// Does **not** invent product style defaults (those belong in `components/*`).
fn write_floating_place(el: &mut Element, place: FloatingPlace) -> bool {
    write_floating_place_ex(el, place, true)
}

/// Like [`write_floating_place`], but `write_xy=false` only applies size constraints
/// (used while content-auto panels are still unmeasured — avoid a 32px-wide center guess).
fn write_floating_place_ex(el: &mut Element, place: FloatingPlace, write_xy: bool) -> bool {
    let mut changed = false;
    if write_xy {
        if inset_length(el.style.inset.left).is_none_or(|v| (v - place.x).abs() > 0.5) {
            el.style.inset.left = Element::into_margin_auto(place.x);
            changed = true;
        }
        if inset_length(el.style.inset.top).is_none_or(|v| (v - place.y).abs() > 0.5) {
            el.style.inset.top = Element::into_margin_auto(place.y);
            changed = true;
        }
    }

    if place.width_auto {
        if !el.style.size.width.is_auto() {
            el.style.size.width = taffy::style::Dimension::auto();
            changed = true;
        }
        let min_w = place.min_w.max(0.0);
        if min_w > 0.0 {
            if dimension_length(el.style.min_size.width).is_none_or(|v| (v - min_w).abs() > 0.5) {
                el.style.min_size.width = taffy::style::Dimension::length(min_w);
                changed = true;
            }
        } else if dimension_length(el.style.min_size.width).unwrap_or(0.0) > 0.5 {
            el.style.min_size.width = taffy::style::Dimension::auto();
            changed = true;
        }
    } else {
        if dimension_length(el.style.size.width).is_none_or(|v| (v - place.w).abs() > 0.5) {
            el.style.size.width = taffy::style::Dimension::length(place.w);
            changed = true;
        }
        // Allow a larger min-width than the matched trigger (e.g. component sets
        // `rui_floating_min_w=trigger+1.75rem`).
        if place.min_w > place.w + 0.5
            && dimension_length(el.style.min_size.width)
                .is_none_or(|v| (v - place.min_w).abs() > 0.5)
        {
            el.style.min_size.width = taffy::style::Dimension::length(place.min_w);
            changed = true;
        }
    }

    if let Some(raw) = el.attrs.get(ATTR_FLOATING_H) {
        let h = parse_css_px(raw, place.max_h).max(0.0);
        if dimension_length(el.style.size.height).is_none_or(|v| (v - h).abs() > 0.5) {
            el.style.size.height = taffy::style::Dimension::length(h);
            changed = true;
        }
    } else {
        // Absolute floaters must not keep `h: 100%` of the portal/viewport (layout footgun).
        match crate::layout::dim_kind(el.style.size.height) {
            crate::layout::DimKind::Percent(_) => {
                el.style.size.height = taffy::style::Dimension::auto();
                changed = true;
            }
            crate::layout::DimKind::Length(_) | crate::layout::DimKind::Auto => {}
        }
    }

    // Accordion max-height keyframes own `max-size.height`. Placement must not rewrite it
    // to the viewport cap — that retargets the interpolation base to ~full height:
    // enter looks instant, exit jumps tall then shrinks (flicker). Measure frames also
    // skip so we capture unconstrained content height instead of the overflow clip.
    if !skip_placement_max_h(el) {
        // `max_h <= 0` → unbounded (`strategy=fixed` without ATTR_FLOATING_MAX_H).
        // Viewport-available max must be allowed to **grow** after scroll (no ratchet).
        if place.max_h <= 0.0 {
            if !el.style.max_size.height.is_auto() {
                el.style.max_size.height = taffy::style::Dimension::auto();
                changed = true;
            }
        } else if dimension_length(el.style.max_size.height)
            .is_none_or(|v| (v - place.max_h).abs() > 0.5)
        {
            el.style.max_size.height = taffy::style::Dimension::length(place.max_h.max(1.0));
            changed = true;
        }
    }

    if !skip_placement_max_h(el) {
        if let Some(raw) = el.attrs.get(ATTR_FLOATING_MIN_H) {
            let min_h = parse_css_px(raw, 0.0);
            if min_h > 0.0
                && dimension_length(el.style.min_size.height)
                    .is_none_or(|v| (v - min_h).abs() > 0.5)
            {
                el.style.min_size.height = taffy::style::Dimension::length(min_h);
                changed = true;
            }
        }
    }

    // Cap without overflow → children paint past the panel chrome.
    // Use Hidden (not Scroll): product panels put ScrollArea inside; Scroll on the
    // floating root steals wheel and leaves the inner viewport thinking it has no overflow.
    // Own box-shadow is not clipped by own overflow — keep shadow on the floating silhouette.
    if el.style.overflow.y == taffy::style::Overflow::Visible {
        el.style.overflow.y = taffy::style::Overflow::Hidden;
        changed = true;
    }
    changed
}

/// Seed a freshly built portal panel from the last synced place (survives reopen).
///
/// Without this, a new absolute panel has `left/top: auto` and can lay out as a zero-size
/// box at the portal origin (children still paint → “lost container” at top-left) whenever
/// [`sync_floating_placement`] would otherwise skip writing because the place signal is unchanged.
pub fn apply_stored_floating_place(el: &mut Element, id: &str) {
    let place = use_signal(
        floating_place_signal_key(id).as_str(),
        FloatingPlace::default,
    );
    let mut p = place.peek();
    // Unsynced default is 160×256 definite — DatePicker calendar cannot fit. Seed from
    // attrs so the first layout is content-sized (`match_trigger=0` / explicit w).
    if p == FloatingPlace::default() {
        let match_trigger = el
            .attrs
            .get(ATTR_FLOATING_MATCH_TRIGGER)
            .or_else(|| el.attrs.get("rui_select_match_trigger"))
            .map(|s| s.as_str() != "0")
            .unwrap_or(true);
        if let Some(w) = el
            .attrs
            .get(ATTR_FLOATING_W)
            .or_else(|| el.attrs.get("rui_select_menu_w"))
        {
            p.w = parse_css_px(w, 0.0);
            p.width_auto = false;
        } else if !match_trigger {
            p.width_auto = true;
            p.w = 0.0;
            p.min_w = el
                .attrs
                .get(ATTR_FLOATING_MIN_W)
                .map(|s| parse_len_vs_trigger(s, 0.0))
                .unwrap_or(0.0);
        }
        let strategy_fixed = el
            .attrs
            .get(ATTR_FLOATING_STRATEGY)
            .map(|s| s.eq_ignore_ascii_case("fixed"))
            .unwrap_or(false);
        p.max_h = el
            .attrs
            .get(ATTR_FLOATING_MAX_H)
            .or_else(|| el.attrs.get("rui_select_max_h"))
            .map(|s| parse_css_px(s, 4096.0))
            .unwrap_or(if strategy_fixed { 0.0 } else { 4096.0 });
        place.set_silent(p);
    }
    // Always write so absolute panels never stay on `auto` insets.
    let _ = write_floating_place(el, p);
}

fn parse_css_px(raw: &str, fallback: f32) -> f32 {
    let s = raw.trim();
    if let Some(n) = s.strip_suffix("rem") {
        return n.trim().parse::<f32>().unwrap_or(fallback / 16.0) * 16.0;
    }
    if let Some(n) = s.strip_suffix("px") {
        return n.trim().parse::<f32>().unwrap_or(fallback);
    }
    s.parse::<f32>().unwrap_or(fallback)
}

/// Parse min-width: absolute length, `trigger`, or `trigger+1.75rem` (anchor-relative geometry).
fn parse_len_vs_trigger(raw: &str, trigger_w: f32) -> f32 {
    let s = raw.trim();
    if s.eq_ignore_ascii_case("trigger") {
        return trigger_w;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("trigger+") {
        return trigger_w + parse_css_px(rest.trim(), 0.0);
    }
    if let Some(rest) = lower.strip_prefix("trigger +") {
        return trigger_w + parse_css_px(rest.trim(), 0.0);
    }
    parse_css_px(s, trigger_w)
}

/// Collect focusable bounds in **visual** (scrolled) space — same convention as hit-testing.
///
/// `layout_bounds` is unscrolled absolute layout space; subtracting ancestor `scroll_offset`
/// yields viewport coordinates. Floating panels live on the portal layer (viewport space), so
/// anchoring must use these visual rects. Using raw `layout_bounds` places menus far below the
/// trigger once the page is scrolled (gallery pickers looked unbound and “vanished”).
///
/// Must match paint / hit-test: `scroll:` viewports still contribute offset when overflow was
/// cleared to `Visible`, and the portal layer / `position: fixed` resets accumulation.
fn collect_focus_bounds(el: &Element, scroll_acc: [f32; 2], out: &mut Vec<(String, [f32; 4])>) {
    let mut stack: Vec<(&Element, [f32; 2])> = vec![(el, scroll_acc)];
    while let Some((el, scroll_acc)) = stack.pop() {
        let scroll_acc =
            if el.template_id.as_deref() == Some(PORTAL_LAYER_ID) || el.style.position_fixed {
                [0.0_f32, 0.0_f32]
            } else {
                scroll_acc
            };
        if let Some(fid) = &el.focus_id {
            let b = crate::core::tree::visual_paint_aabb(el, scroll_acc);
            out.push((fid.clone(), b));
        }
        let mut next_acc = scroll_acc;
        let is_scroll_viewport = el
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
        if is_scroll_viewport || el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0];
        }
        if is_scroll_viewport || el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1];
        }
        for c in el.children.iter().rev() {
            stack.push((c, next_acc));
        }
    }
}

fn floating_id_of(el: &Element) -> Option<String> {
    el.attrs
        .get(ATTR_FLOATING)
        .cloned()
        .or_else(|| el.attrs.get("rui_select_menu").cloned())
}

fn floating_trigger_of(el: &Element) -> Option<String> {
    el.attrs
        .get(ATTR_FLOATING_TRIGGER)
        .cloned()
        .or_else(|| el.attrs.get("rui_select_trigger").cloned())
}

fn apply_floating_menus(
    el: &mut Element,
    triggers: &[(String, [f32; 4])],
    viewport: [f32; 2],
    changed: &mut bool,
) {
    let Some(id) = floating_id_of(el) else {
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, changed);
        }
        return;
    };

    // Unanchored overlays set ATTR_FLOATING (+ optional TRIGGER) so outside-press
    // treats the panel / toggle as inside — without PLACEMENT or POINT, do not
    // rewrite left/top (author `fixed` / inset is the geometry).
    let has_point = floating_point_of(el).is_some();
    let has_placement_attr = el.attrs.get(ATTR_FLOATING_PLACEMENT).is_some()
        || el.attrs.get("rui_select_placement").is_some();
    let is_select = el.attrs.contains_key("rui_select_menu");
    if !has_point && !has_placement_attr && !is_select {
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, changed);
        }
        return;
    }

    let point = floating_point_of(el);
    let trigger_fid = floating_trigger_of(el);
    let tb = if let Some(pt) = point {
        Some([pt[0], pt[1], 0.0, 0.0])
    } else if let Some(ref fid) = trigger_fid {
        triggers.iter().find(|(f, _)| f == fid).map(|(_, b)| *b)
    } else {
        None
    };

    if let Some(tb) = tb {
        let match_trigger = el
            .attrs
            .get(ATTR_FLOATING_MATCH_TRIGGER)
            .or_else(|| el.attrs.get("rui_select_match_trigger"))
            .map(|s| s.as_str() != "0")
            .unwrap_or(true);
        let explicit_w = el
            .attrs
            .get(ATTR_FLOATING_W)
            .or_else(|| el.attrs.get("rui_select_menu_w"))
            .map(|s| parse_css_px(s, tb[2]));
        let attr_min_w = el
            .attrs
            .get(ATTR_FLOATING_MIN_W)
            .map(|s| parse_len_vs_trigger(s, tb[2]));
        // Geometry knobs only — product defaults (8rem / max-h-96 / …) live on components.
        let (menu_w, width_auto, min_w) = if let Some(w) = explicit_w {
            (w.max(0.0), false, attr_min_w.unwrap_or(0.0))
        } else if point.is_some() {
            // Cursor menus are content-sized; min-width from attr (e.g. 8rem).
            let floor = attr_min_w.unwrap_or(0.0).max(0.0);
            (floor, true, floor)
        } else if match_trigger {
            let w = tb[2].max(0.0);
            (w, false, attr_min_w.unwrap_or(w).max(w))
        } else {
            // Content-sized (`width: auto`). Default min-width = trigger (Select menus);
            // explicit `rui_floating_min_w` wins (e.g. `0` → true w-fit for Tooltip).
            let floor = attr_min_w.unwrap_or(tb[2]).max(0.0);
            (floor, true, floor)
        };
        let preferred = PopupPlacement::from_attr(
            el.attrs
                .get(ATTR_FLOATING_PLACEMENT)
                .or_else(|| el.attrs.get("rui_select_placement"))
                .map(|s| s.as_str())
                .unwrap_or("bottom-start"),
        );
        let estimate_h = floating_estimate_h(el);
        let estimate_w = if width_auto {
            if el.layout_bounds[2] > 0.5 {
                el.layout_bounds[2].max(menu_w)
            } else {
                menu_w.max(32.0)
            }
        } else {
            menu_w.max(min_w).max(32.0)
        };
        // Content-auto + no layout size yet: applying a 32×32 center guess then correcting
        // after measure is the classic HoverCard / Tooltip open jitter. Size-mode only
        // until bounds exist; XY lands in one shot on the next sync.
        let size_unknown = width_auto && el.layout_bounds[2] <= 0.5 && el.layout_bounds[3] <= 0.5;
        let gap = el
            .attrs
            .get(ATTR_FLOATING_GAP)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(if point.is_some() {
                0.0
            } else {
                FLOATING_TRIGGER_GAP
            })
            .clamp(-64.0, 64.0);
        let strategy_fixed = el
            .attrs
            .get(ATTR_FLOATING_STRATEGY)
            .map(|s| s.eq_ignore_ascii_case("fixed"))
            .unwrap_or(false);
        if size_unknown {
            let next = FloatingPlace {
                x: 0.0,
                y: 0.0,
                w: menu_w.max(0.0),
                max_h: 0.0,
                width_auto,
                min_w: min_w.max(0.0),
            };
            if write_floating_place_ex(el, next, false) {
                *changed = true;
            }
        } else {
            let (x, mut y, max_h) = if point.is_some() && !strategy_fixed {
                let (x, y, available_max_h) = compute_cursor_menu_placement(
                    [tb[0], tb[1]],
                    viewport,
                    estimate_w,
                    estimate_h,
                    gap,
                );
                let max_h = el
                    .attrs
                    .get(ATTR_FLOATING_MAX_H)
                    .or_else(|| el.attrs.get("rui_select_max_h"))
                    .map(|s| available_max_h.min(parse_css_px(s, available_max_h)))
                    .unwrap_or(available_max_h)
                    .max(1.0);
                (x, y, max_h)
            } else if strategy_fixed {
                let (x, mut y) =
                    compute_fixed_anchor(tb, viewport, estimate_w, estimate_h, preferred, gap);
                // Cover + top: glue the trigger bottom so growing wrap height expands upward
                // without jumping the seam (estimate→measured churn).
                let cover = gap < 0.0;
                let prefer_bottom = matches!(
                    preferred,
                    PopupPlacement::BottomStart
                        | PopupPlacement::BottomEnd
                        | PopupPlacement::BottomCenter
                );
                if cover && !prefer_bottom {
                    let edge = tb[1] + tb[3];
                    el.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, format!("{edge}"));
                    // Prefer animated max-height while accordion clips — layout_bounds can
                    // still be the full content height and would slam `top` to the final y.
                    let visual_h = dimension_length(el.style.max_size.height)
                        .filter(|h| *h >= 0.0)
                        .filter(|_| height_keyframe_animation_active(el))
                        .or_else(|| (el.layout_bounds[3] > 0.5).then_some(el.layout_bounds[3]))
                        .unwrap_or_else(|| estimate_h.max(tb[3]).max(1.0));
                    y = edge - visual_h;
                } else if cover {
                    el.attrs.remove(ATTR_FLOATING_ANCHOR_EDGE_Y);
                }
                let max_h = el
                    .attrs
                    .get(ATTR_FLOATING_MAX_H)
                    .or_else(|| el.attrs.get("rui_select_max_h"))
                    .map(|s| parse_css_px(s, 0.0))
                    .filter(|h| *h > 0.0)
                    .unwrap_or(0.0);
                (x, y, max_h)
            } else {
                let (x, y, available_max_h, _) = compute_anchored_placement(
                    tb, viewport, estimate_w, estimate_h, preferred, gap,
                );
                let max_h = el
                    .attrs
                    .get(ATTR_FLOATING_MAX_H)
                    .or_else(|| el.attrs.get("rui_select_max_h"))
                    .map(|s| available_max_h.min(parse_css_px(s, available_max_h)))
                    .unwrap_or(available_max_h)
                    .max(1.0);
                (x, y, max_h)
            };
            // Reverse expand: keep the trigger-facing edge glued while accordion clips.
            // Never drop a committed top placement mid-animation — a clip-sized estimate
            // would re-place the shrinking panel below the trigger (flat sliver flash).
            // Fixed cover already glued the seam above; skip so negative gap is not
            // misread as `ty - gap`.
            let skip_reverse_glue = strategy_fixed && gap < 0.0;
            if !skip_reverse_glue {
                let existing_edge = el
                    .attrs
                    .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
                    .and_then(|s| s.parse::<f32>().ok())
                    .filter(|e| *e > 0.0);
                let animating_h = height_keyframe_animation_active(el);
                let placed_above = existing_edge.is_some() || y + 0.5 < tb[1];
                if placed_above {
                    let edge = existing_edge
                        .unwrap_or_else(|| (tb[1] - gap).max(FLOATING_VIEWPORT_MARGIN));
                    el.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, format!("{edge}"));
                    if animating_h {
                        let visual_h = dimension_length(el.style.max_size.height)
                            .filter(|h| *h >= 0.0)
                            .unwrap_or(el.layout_bounds[3].max(0.0));
                        y = (edge - visual_h.min(max_h.max(0.0))).max(FLOATING_VIEWPORT_MARGIN);
                    }
                } else if !animating_h {
                    el.attrs.remove(ATTR_FLOATING_ANCHOR_EDGE_Y);
                }
            }
            let next = FloatingPlace {
                x,
                y,
                w: menu_w.max(0.0),
                max_h,
                width_auto,
                min_w: min_w.max(0.0),
            };
            let place_key = floating_place_signal_key(&id);
            let place = use_signal(place_key.as_str(), FloatingPlace::default);
            let cur = place.peek();
            let signal_dirty = (cur.x - next.x).abs() > 0.5
                || (cur.y - next.y).abs() > 0.5
                || (cur.w - next.w).abs() > 0.5
                || (cur.max_h - next.max_h).abs() > 0.5
                || (cur.min_w - next.min_w).abs() > 0.5
                || cur.width_auto != next.width_auto;
            if signal_dirty {
                place.set_silent(next);
            }
            // Always write onto the element: a reopened portal panel is a new node with
            // `left/top: auto` even when the place signal already holds the right coords.
            if write_floating_place(el, next) {
                *changed = true;
            }
        }
    }

    for c in &mut el.children {
        apply_floating_menus(c, triggers, viewport, changed);
    }
}

/// 布局后根据触发器位置更新带 `rui_floating`（或兼容 `rui_select_*`）的浮层坐标。
/// 几何有变时返回 `true`（调用方应再跑一轮 layout）。
///
/// 滚动跟随时通常只改 `left/top`：调用方应 [`visit_floating_panels`] + `mark_element_layout_dirty`
/// 后走 **fast** relayout，禁止每帧 `build_and_layout` 全量重建 Taffy 节点。
pub fn sync_floating_placement(root: &mut Element, viewport_w: f32, viewport_h: f32) -> bool {
    let mut triggers = Vec::new();
    collect_focus_bounds(root, [0.0, 0.0], &mut triggers);
    let mut changed = false;
    apply_floating_menus(root, &triggers, [viewport_w, viewport_h], &mut changed);
    changed
}

/// 遍历已标记的浮层面板（含兼容 `rui_select_menu`）。
pub fn visit_floating_panels(root: &Element, mut f: impl FnMut(&Element)) {
    crate::core::tree::walk_elements(root, |el| {
        if floating_id_of(el).is_some() {
            f(el);
        }
    });
}

/// 标记 `rui_portal=1`，布局时提升到根 portal 叠层（Solid `<Portal>` / `document.body`）。
pub fn mark_portal(mut el: Element) -> Element {
    el.set_attr("rui_portal", "1");
    el
}

/// 标记 portal，并挂到宿主 `template_id`（Solid `<Portal mount={…}>`）。
///
/// 空 / 找不到宿主时，[`crate::layout::layout_tree::hoist_portal_subtrees`] 回退到根叠层。
pub fn mark_portal_mount(mut el: Element, mount: impl AsRef<str>) -> Element {
    let mount = mount.as_ref().trim();
    el = mark_portal(el);
    if !mount.is_empty() {
        el.set_attr(crate::layout::layout_tree::ATTR_PORTAL_MOUNT, mount);
    }
    el
}

/// 设置 z-index 并 portal 提升（浮层面板）。
pub fn portal_panel(mut inner: Element, z: i32) -> Element {
    inner.style.z_index = z;
    mark_portal(inner)
}

/// 设置 z-index，并 portal 提升到指定宿主。
pub fn portal_panel_mount(mut inner: Element, z: i32, mount: impl AsRef<str>) -> Element {
    inner.style.z_index = z;
    mark_portal_mount(inner, mount)
}

/// 全屏 underlay：点击将 `open` 设为 false；绝对定位 + portal。
///
/// Closes are ignored until [`arm_underlay_close`] (primary button release) after
/// [`disarm_underlay_close`] on open — so the opening gesture cannot instantly dismiss.
///
/// Use for **modal** overlays (Dialog / Sheet / Alert). Non-modal Popover / Menu / Select
/// should omit this and rely on [`crate::app::portal_overlay::try_dismiss_outside_press`]
/// so only the panel blocks pointer events (SolidJS / Radix dismissable layer).
///
/// 实现委托 [`crate::primitives::overlay`]（无默认着色；调用方可再 `.bg(...)`）。
pub fn underlay_close_on_click(open: Signal<bool>, z: i32) -> Element {
    overlay(z).close_on_click(open).into()
}

/// True when `path` hits the trigger focus id of any hoisted floating panel.
pub fn path_hits_floating_trigger(
    root: &Element,
    path: &[crate::app::pointer_events::HitPathEntry],
) -> bool {
    let mut triggers = std::collections::HashSet::new();
    collect_floating_triggers(root, &mut triggers);
    path.iter().any(|e| {
        e.focus_id.as_ref().is_some_and(|id| {
            triggers.contains(id.as_str())
                // Nested Select inside Dialog: the menu portal is not hoisted on the
                // opening click, so hoisted panels have not registered their trigger yet.
                // Without this, the same release runs `try_dismiss_outside_press` and
                // immediately closes the menu that `SelectTrigger` just opened.
                || id.ends_with(":trigger")
        })
    })
}

fn collect_floating_triggers(el: &Element, out: &mut std::collections::HashSet<String>) {
    crate::core::tree::walk_elements(el, |el| {
        if el
            .attrs
            .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
            .map(|s| s.as_str())
            == Some("1")
        {
            if let Some(t) = el
                .attrs
                .get(ATTR_FLOATING_TRIGGER)
                .or_else(|| el.attrs.get("rui_select_trigger"))
            {
                let t = t.trim();
                if !t.is_empty() {
                    out.insert(t.to_string());
                }
            }
        }
    });
}

/// 吞掉点击，避免事件落到 underlay / 下层页面（不 portal；用于已在 portal 舞台内的面板）。
pub fn swallow_clicks(inner: Element) -> Element {
    inner.on_click(|| {})
}

/// 面板包装：吞掉点击 + portal 提升（锚定浮层默认路径）。
///
/// Hoist 后保持 `pointer-events: auto`，面板轮廓挡住点击穿透；勿依赖「无 handler →
/// box-none」的旧默认。特殊透传（Toast / 空锚 / 视口舞台）请显式 `.pointer_events_*`。
///
/// 勿再包一层 [`portal_panel`]：嵌套 `rui_portal` 会把内层面板 hoist 出居中舞台。
pub fn panel_block_clicks(mut inner: Element, z: i32) -> Element {
    if inner
        .attrs
        .get(ATTR_PORTAL_KEY)
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        let key = floating_id_of(&inner)
            .map(|id| format!("panel:{id}"))
            .unwrap_or_else(|| format!("panel:{z}"));
        inner.set_attr(ATTR_PORTAL_KEY, key);
    }
    // Ensure Auto even if a caller accidentally set box-none on the panel chrome.
    if inner.pointer_events_mode() != crate::core::tree::PointerEvents::Auto {
        inner.set_attr(
            crate::core::tree::ATTR_POINTER_EVENTS,
            crate::core::tree::PointerEvents::Auto.as_str(),
        );
    }
    portal_panel(swallow_clicks(inner), z)
}

/// Zero-size mount for a portal-hoisted floating panel (Select / DatePicker / ColorPicker).
///
/// The panel stays in the tree until [`hoist_portal_subtrees`] lifts it; the anchor must **not**
/// sit in a `relative` trigger wrapper inside a `ScrollArea`, or pre-hoist `left/top` (viewport
/// space) is applied against a scrolled containing block → placement jumps and children paint
/// at mixed coordinates.
///
/// Prefer [`mount_anchored_overlay`] (this + [`panel_block_clicks`]) at call sites.
pub fn floating_portal_anchor(panel: Element) -> Element {
    Element::new()
        .w(0.0)
        .h(0.0)
        .child(panel)
        .pointer_events_box_none()
}

/// 锚定浮层单一路径：已 [`mark_floating`] 的panel → 吞click + portal + 0×0 layout锚。
///
/// Select / Combobox / DatePicker / ColorPicker / Tooltip / 有anchor的 Popover /
/// Dropdown / ContextMenu / Menubar must走这里，must not `portal_panel` 当 flex 兄弟。
pub fn mount_anchored_overlay(panel: Element, panel_z: i32) -> Element {
    floating_portal_anchor(panel_block_clicks(panel, panel_z))
}

/// viewport固定 portal 舞台（Dialog / Sheet / CommandOverlay）。
///
/// 舞台本身是全屏居中壳：显式 `box-none`，空白区click落到 underlay（close）。
/// 舞台内panel只用 [`swallow_clicks`]，**must not**再包 [`panel_block_clicks`]（会嵌套
/// `rui_portal`，把panel hoist 出舞台）。
pub fn mount_viewport_portal_stage(mut stage: Element, panel_z: i32) -> Element {
    stage = stage.fixed();
    stage.set_attr(ATTR_ESCAPE_OVERFLOW_CLIP, "1");
    if stage
        .attrs
        .get(ATTR_PORTAL_KEY)
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        stage.set_attr(ATTR_PORTAL_KEY, format!("panel:{panel_z}"));
    }
    // Special-purpose pass-through: empty stage chrome must not occlude the underlay.
    portal_panel(stage, panel_z).pointer_events_box_none()
}

/// Modal layer: underlay + stage already from [`mount_viewport_portal_stage`].
pub fn mount_modal_layer(underlay: Element, stage: Element) -> Element {
    Element::new()
        .child(underlay)
        .child(stage)
        .pointer_events_box_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visit_floating_panels_finds_marked() {
        let mut panel = Element::new();
        mark_floating(&mut panel, "m1", "t1", PopupPlacement::BottomStart);
        let nested = Element::new().child(panel);
        let root = Element::new().child(Element::new()).child(nested);
        let mut n = 0usize;
        visit_floating_panels(&root, |_| n += 1);
        assert_eq!(n, 1);
    }

    #[test]
    fn identity_floating_skips_xy_even_with_trigger() {
        let mut btn = Element::new().w(40.0).h(20.0);
        btn = btn.focus_id("hud-btn");
        btn.layout_bounds = [8.0, 8.0, 40.0, 20.0];
        let mut panel = Element::new()
            .fixed()
            .top("1rem")
            .right("1rem")
            .w(80.0)
            .h(40.0);
        panel.set_attr(ATTR_FLOATING, "hud");
        panel.set_attr(ATTR_FLOATING_TRIGGER, "hud-btn");
        let left_before = panel.style.inset.left;
        let mut root = Element::new().child(btn).child(panel);
        assert!(
            !sync_floating_placement(&mut root, 800.0, 600.0),
            "identity-only ATTR_FLOATING must not rewrite author inset"
        );
        assert_eq!(root.children[1].style.inset.left, left_before);
    }

    #[test]
    fn write_floating_place_pairs_max_h_with_overflow_scroll() {
        let mut el = Element::new().absolute().w(160.0);
        assert_eq!(el.style.overflow.y, taffy::style::Overflow::Visible);
        let place = FloatingPlace {
            x: 10.0,
            y: 20.0,
            w: 160.0,
            max_h: 120.0,
            width_auto: false,
            min_w: 0.0,
        };
        assert!(write_floating_place(&mut el, place));
        assert_eq!(el.style.overflow.y, taffy::style::Overflow::Hidden);
        match crate::layout::dim_kind(el.style.max_size.height) {
            crate::layout::DimKind::Length(h) => assert!((h - 120.0).abs() < 0.1),
            other => panic!("expected max height length, got {other:?}"),
        }
        // Capability layer: no default min-height unless ATTR_FLOATING_MIN_H is set.
        assert!(
            dimension_length(el.style.min_size.height).unwrap_or(0.0) == 0.0,
            "popup must not invent a style min-height"
        );
    }

    #[test]
    fn write_floating_place_skips_max_h_during_accordion_animation() {
        use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
        use taffy::style::Dimension;

        let mut el = Element::new().absolute().w(160.0);
        el.style.max_size.height = Dimension::length(24.0);
        el.style.animations = vec![keyframes_anim_entry(
            "accordion-down",
            200.0,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        )];
        let place = FloatingPlace {
            x: 10.0,
            y: 20.0,
            w: 160.0,
            max_h: 400.0,
            width_auto: false,
            min_w: 0.0,
        };
        let _ = write_floating_place(&mut el, place);
        match crate::layout::dim_kind(el.style.max_size.height) {
            crate::layout::DimKind::Length(h) => assert!(
                (h - 24.0).abs() < 0.1,
                "accordion mid max-height must not be replaced by viewport cap, got {h}"
            ),
            other => panic!("expected preserved Length, got {other:?}"),
        }
    }

    #[test]
    fn write_floating_place_skips_max_h_during_expand_measure() {
        use taffy::style::Dimension;

        let mut el = Element::new().absolute().w(160.0);
        el.style.max_size.height = Dimension::auto();
        el.set_attr(ATTR_FLOATING_EXPAND_MEASURE, "1");
        let place = FloatingPlace {
            x: 10.0,
            y: 20.0,
            w: 160.0,
            max_h: 80.0,
            width_auto: false,
            min_w: 0.0,
        };
        let _ = write_floating_place(&mut el, place);
        assert!(
            el.style.max_size.height.is_auto(),
            "measure frame must keep unconstrained max-height, not viewport cap"
        );
    }

    #[test]
    fn sync_floating_estimate_h_uses_content_h_not_clip() {
        let mut trigger = Element::new().focus_id("t-clip");
        // Near the bottom: clipped height would stay below; content height must flip above.
        trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating(
            &mut panel,
            "clip-menu",
            "t-clip",
            PopupPlacement::BottomStart,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
        panel.set_attr(ATTR_FLOATING_CONTENT_H, "200");

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let menu = &root.children[1];
        match inset_length(menu.style.inset.top) {
            Some(y) => assert!(
                (y - 292.0).abs() < 0.5,
                "top must use content height 200 (y=292), not clip 20 (y=536), got {y}"
            ),
            None => panic!("expected top inset"),
        }
    }

    #[test]
    fn sync_floating_top_placed_accordion_tracks_visual_height() {
        use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
        use taffy::style::Dimension;

        let mut trigger = Element::new().focus_id("t-up");
        trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating(&mut panel, "up-menu", "t-up", PopupPlacement::BottomStart);
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
        panel.set_attr(ATTR_FLOATING_CONTENT_H, "200");
        panel.style.max_size.height = Dimension::length(20.0);
        panel.style.animations = vec![keyframes_anim_entry(
            "accordion-down",
            200.0,
            TimingFunction::EaseOut,
            AnimationFillMode::Both,
        )];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let menu = &root.children[1];
        // Trigger-facing edge = 500 - 8; visual clip 20 → y = 472 (grows from trigger).
        // Final content y would be 292 — must not jump there while clipped.
        match inset_length(menu.style.inset.top) {
            Some(y) => assert!(
                (y - 472.0).abs() < 0.5,
                "upward accordion must track visual height from trigger, got {y}"
            ),
            None => panic!("expected top inset"),
        }
    }

    #[test]
    fn sync_floating_exit_keeps_top_placement_when_clip_shrinks() {
        use crate::style::{keyframes_anim_entry, AnimationFillMode, TimingFunction};
        use taffy::style::Dimension;

        let mut trigger = Element::new().focus_id("t-exit");
        trigger.layout_bounds = [20.0, 500.0, 80.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating(
            &mut panel,
            "exit-menu",
            "t-exit",
            PopupPlacement::BottomStart,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        panel.layout_bounds = [0.0, 0.0, 80.0, 20.0];
        // Rebuild often drops CONTENT_H; clip 20 would flip below (y=536) without a frozen edge.
        panel.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, "492");
        panel.style.max_size.height = Dimension::length(20.0);
        panel.style.animations = vec![keyframes_anim_entry(
            "accordion-up",
            200.0,
            TimingFunction::EaseIn,
            AnimationFillMode::Both,
        )];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let menu = &root.children[1];
        match inset_length(menu.style.inset.top) {
            Some(y) => assert!(
                (y - 472.0).abs() < 0.5,
                "collapsing top menu must not flip below the trigger, got {y}"
            ),
            None => panic!("expected top inset"),
        }
        assert_eq!(
            menu.attrs
                .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
                .map(String::as_str),
            Some("492")
        );
    }

    #[test]
    fn write_floating_place_coerces_percent_height_to_auto() {
        let mut el = Element::new().absolute().w(160.0).h("100%").max_h(192.0);
        let place = FloatingPlace {
            x: 0.0,
            y: 40.0,
            w: 160.0,
            max_h: 400.0,
            width_auto: false,
            min_w: 0.0,
        };
        assert!(write_floating_place(&mut el, place));
        assert!(
            el.style.size.height.is_auto(),
            "ScrollArea h:100% must not stretch floating menus"
        );
        match crate::layout::dim_kind(el.style.max_size.height) {
            crate::layout::DimKind::Length(h) => assert!(
                (h - 400.0).abs() < 0.1,
                "viewport max_h must not ratchet against prior style.max_h, got {h}"
            ),
            other => panic!("expected max height length, got {other:?}"),
        }
    }

    #[test]
    fn write_floating_place_max_h_can_grow_after_viewport_opens() {
        let mut el = Element::new().absolute().w(160.0);
        let small = FloatingPlace {
            x: 10.0,
            y: 20.0,
            w: 160.0,
            max_h: 80.0,
            width_auto: true,
            min_w: 0.0,
        };
        assert!(write_floating_place(&mut el, small));
        let big = FloatingPlace {
            max_h: 320.0,
            ..small
        };
        assert!(write_floating_place(&mut el, big));
        match crate::layout::dim_kind(el.style.max_size.height) {
            crate::layout::DimKind::Length(h) => assert!(
                (h - 320.0).abs() < 0.1,
                "max_h must grow back after scroll, got {h}"
            ),
            other => panic!("expected Length max height, got {other:?}"),
        }
    }

    #[test]
    fn write_floating_place_respects_fixed_height_attr() {
        let mut el = Element::new().absolute().w(160.0);
        el.set_attr(ATTR_FLOATING_H, "200px");
        let place = FloatingPlace {
            x: 0.0,
            y: 0.0,
            w: 160.0,
            max_h: 400.0,
            width_auto: false,
            min_w: 0.0,
        };
        assert!(write_floating_place(&mut el, place));
        match crate::layout::dim_kind(el.style.size.height) {
            crate::layout::DimKind::Length(h) => assert!((h - 200.0).abs() < 0.1),
            other => panic!("expected fixed height, got {other:?}"),
        }
    }

    #[test]
    fn write_floating_place_respects_min_h_override() {
        let mut el = Element::new().absolute().w(160.0).min_h(64.0);
        let place = FloatingPlace {
            x: 0.0,
            y: 0.0,
            w: 160.0,
            max_h: 200.0,
            width_auto: false,
            min_w: 0.0,
        };
        assert!(write_floating_place(&mut el, place));
        match crate::layout::dim_kind(el.style.min_size.height) {
            crate::layout::DimKind::Length(h) => assert!((h - 64.0).abs() < 0.1),
            other => panic!("expected preserved min height, got {other:?}"),
        }
    }

    #[test]
    fn write_floating_place_auto_width_uses_min_not_forced_w() {
        let mut el = Element::new().absolute().w(400.0);
        let place = FloatingPlace {
            x: 10.0,
            y: 20.0,
            w: 120.0,
            max_h: 200.0,
            width_auto: true,
            min_w: 120.0,
        };
        assert!(write_floating_place(&mut el, place));
        assert!(
            el.style.size.width.is_auto(),
            "independent panel must not force trigger width"
        );
        match crate::layout::dim_kind(el.style.min_size.width) {
            crate::layout::DimKind::Length(w) => assert!((w - 120.0).abs() < 0.1),
            other => panic!("expected min-width = trigger, got {other:?}"),
        }
    }

    #[test]
    fn sync_floating_independent_width_is_content_auto() {
        let mut trigger = Element::new().focus_id("t-wide");
        trigger.layout_bounds = [20.0, 40.0, 80.0, 28.0];

        let mut panel = Element::new().absolute().w(300.0);
        mark_floating(
            &mut panel,
            "indep-menu",
            "t-wide",
            PopupPlacement::BottomStart,
        );
        // match_trigger=0 and no rui_floating_w → content auto, min ≥ trigger
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.layout_bounds = [0.0, 0.0, 220.0, 60.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let menu = &root.children[1];
        assert!(
            menu.style.size.width.is_auto(),
            "panel width must stay independent of trigger"
        );
        match crate::layout::dim_kind(menu.style.min_size.width) {
            crate::layout::DimKind::Length(w) => assert!((w - 80.0).abs() < 0.5),
            other => panic!("min-width should be trigger width, got {other:?}"),
        }
    }

    #[test]
    fn sync_floating_w_fit_min_w_zero() {
        let mut trigger = Element::new().focus_id("t-fit");
        trigger.layout_bounds = [20.0, 40.0, 160.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating_with_gap(
            &mut panel,
            "tip-fit",
            "t-fit",
            PopupPlacement::BottomCenter,
            4.0,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        panel.layout_bounds = [0.0, 0.0, 72.0, 24.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let tip = &root.children[1];
        assert!(
            tip.style.size.width.is_auto(),
            "tooltip must be w-fit (width: auto)"
        );
        match crate::layout::dim_kind(tip.style.min_size.width) {
            crate::layout::DimKind::Auto => {}
            crate::layout::DimKind::Length(w) if w < 0.5 => {}
            other => panic!("w-fit must not floor min-width to trigger, got {other:?}"),
        }
        // Prefer below with gap 4; center on trigger mid-x (20+80=100 → x=100-36=64).
        match inset_length(tip.style.inset.top) {
            Some(y) => assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}"),
            None => panic!("expected top inset"),
        }
        match inset_length(tip.style.inset.left) {
            Some(x) => assert!((x - 64.0).abs() < 0.5, "x={x}"),
            None => panic!("expected left inset"),
        }
    }

    #[test]
    fn sync_floating_content_auto_defers_xy_until_measured() {
        let mut trigger = Element::new().focus_id("t-defer");
        trigger.layout_bounds = [100.0, 40.0, 80.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating_with_gap(
            &mut panel,
            "hc-defer",
            "t-defer",
            PopupPlacement::BottomCenter,
            4.0,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        // Unmeasured first paint — must not commit a 32px-wide center guess.
        panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];

        let mut root = Element::new().child(trigger).child(panel);
        let _ = sync_floating_placement(&mut root, 800.0, 600.0);
        assert!(
            inset_length(root.children[1].style.inset.left).is_none(),
            "unmeasured content-auto must not write left"
        );
        assert!(
            inset_length(root.children[1].style.inset.top).is_none(),
            "unmeasured content-auto must not write top"
        );

        root.children[1].layout_bounds = [0.0, 0.0, 256.0, 80.0];
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        // Center: mid trigger 140 − half panel 128 → x=12.
        match inset_length(root.children[1].style.inset.left) {
            Some(x) => assert!((x - 12.0).abs() < 0.5, "x={x}"),
            None => panic!("expected left after measure"),
        }
        match inset_length(root.children[1].style.inset.top) {
            Some(y) => assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}"),
            None => panic!("expected top after measure"),
        }
    }

    #[test]
    fn sync_floating_explicit_w_stable_before_and_after_layout() {
        let mut trigger = Element::new().focus_id("t-stable");
        trigger.layout_bounds = [100.0, 40.0, 80.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating_with_gap(
            &mut panel,
            "hc-stable",
            "t-stable",
            PopupPlacement::BottomCenter,
            4.0,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        panel.set_attr(ATTR_FLOATING_W, "256px");
        panel.layout_bounds = [0.0, 0.0, 0.0, 0.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let x0 = inset_length(root.children[1].style.inset.left).expect("left before layout");
        assert!((x0 - 12.0).abs() < 0.5, "x0={x0}");

        root.children[1].layout_bounds = [0.0, 0.0, 256.0, 120.0];
        let _ = sync_floating_placement(&mut root, 800.0, 600.0);
        let x1 = inset_length(root.children[1].style.inset.left).expect("left after layout");
        assert!(
            (x1 - x0).abs() < 0.5,
            "explicit w must not re-center after layout: {x0} → {x1}"
        );
    }

    #[test]
    fn compute_flips_above_when_below_is_tight() {
        // Trigger near bottom; small panel must flip above.
        let trigger = [100.0, 560.0, 80.0, 28.0];
        let (_x, y, _max_h, flipped) = compute_anchored_placement(
            trigger,
            [800.0, 600.0],
            72.0,
            24.0,
            PopupPlacement::BottomCenter,
            4.0,
        );
        assert!(flipped, "must flip above when below has no room");
        // ty - gap - h = 560 - 4 - 24 = 532
        assert!((y - 532.0).abs() < 0.5, "y={y}");
    }

    #[test]
    fn compute_picks_roomier_side_when_neither_fits() {
        // Trigger mid-low: below ≈ 100px, above ≈ 400px; content 300 fits neither.
        // Old min(80) threshold kept below; must open above to maximize scroll viewport.
        let trigger = [100.0, 420.0, 80.0, 28.0];
        let (_x, y, max_h, flipped) = compute_anchored_placement(
            trigger,
            [800.0, 600.0],
            160.0,
            300.0,
            PopupPlacement::BottomStart,
            8.0,
        );
        assert!(
            flipped,
            "neither side fits 300px; must pick the larger (above) side"
        );
        // space_above = 420 - 8 - 8 = 404; y = 420 - 8 - min(300, max(404,48))
        assert!(
            (max_h - 404.0).abs() < 0.5,
            "max_h must be the larger side, got {max_h}"
        );
        assert!(
            y + 0.5 < trigger[1],
            "panel must sit above the trigger, got y={y}"
        );
    }

    #[test]
    fn compute_center_aligns_mid_x() {
        // trigger mid = 100+40 = 140; panel w=72 → x = 140 - 36 = 104
        let (x, y, _, flipped) = compute_anchored_placement(
            [100.0, 40.0, 80.0, 28.0],
            [800.0, 600.0],
            72.0,
            24.0,
            PopupPlacement::BottomCenter,
            4.0,
        );
        assert!(!flipped);
        assert!((x - 104.0).abs() < 0.5, "x={x}");
        assert!((y - (40.0 + 28.0 + 4.0)).abs() < 0.5, "y={y}");
    }

    #[test]
    fn compute_center_clamps_to_viewport_edge() {
        // Near left edge: ideal center would go past margin → clamp to margin.
        let (x, _, _, _) = compute_anchored_placement(
            [10.0, 40.0, 40.0, 28.0],
            [800.0, 600.0],
            120.0,
            24.0,
            PopupPlacement::BottomCenter,
            4.0,
        );
        assert!((x - FLOATING_VIEWPORT_MARGIN).abs() < 0.5, "x={x}");

        // Near right edge: clamp so panel right stays at vw - margin.
        let (x2, _, _, _) = compute_anchored_placement(
            [760.0, 40.0, 30.0, 28.0],
            [800.0, 600.0],
            100.0,
            24.0,
            PopupPlacement::BottomCenter,
            4.0,
        );
        let expect = 800.0 - FLOATING_VIEWPORT_MARGIN - 100.0;
        assert!((x2 - expect).abs() < 0.5, "x2={x2}");
    }

    #[test]
    fn compute_bottom_start_basic() {
        let (x, y, max_h, flipped) = compute_anchored_placement(
            [10.0, 10.0, 100.0, 24.0],
            [800.0, 600.0],
            100.0,
            120.0,
            PopupPlacement::BottomStart,
            FLOATING_TRIGGER_GAP,
        );
        assert!(!flipped);
        assert_eq!(x, 10.0);
        // trigger bottom 34 + gap 8 → 42; not flush to trigger
        assert_eq!(y, 42.0);
        assert!(max_h > 48.0);
    }

    #[test]
    fn compute_clamps_away_from_viewport_edges() {
        let (x, y, _, _) = compute_anchored_placement(
            [0.0, 0.0, 40.0, 20.0],
            [200.0, 200.0],
            80.0,
            40.0,
            PopupPlacement::BottomStart,
            FLOATING_TRIGGER_GAP,
        );
        assert!(
            x >= FLOATING_VIEWPORT_MARGIN - 0.01,
            "x must keep viewport margin, got {x}"
        );
        assert!(
            (y - (20.0 + FLOATING_TRIGGER_GAP)).abs() < 0.01,
            "y must keep trigger gap, got {y}"
        );

        let (x2, _, _, _) = compute_anchored_placement(
            [180.0, 10.0, 40.0, 20.0],
            [200.0, 200.0],
            80.0,
            40.0,
            PopupPlacement::BottomEnd,
            FLOATING_TRIGGER_GAP,
        );
        assert!(
            x2 + 80.0 <= 200.0 - FLOATING_VIEWPORT_MARGIN + 0.01,
            "right edge must keep viewport margin, x={x2}"
        );
    }

    #[test]
    fn sync_floating_uses_scrolled_visual_trigger_bounds() {
        // Scroll viewport at (0,0) 800×600; content scrolled down by 400px.
        // Trigger layout_bounds y=500 (unscrolled) → visual y=100.
        let mut trigger = Element::new().focus_id("t1");
        trigger.layout_bounds = [40.0, 500.0, 120.0, 36.0];

        let mut scroller = Element::new().child(trigger);
        scroller.style.overflow.y = taffy::style::Overflow::Scroll;
        scroller.scroll_offset = [0.0, 400.0];
        scroller.layout_bounds = [0.0, 0.0, 800.0, 600.0];

        let mut panel = Element::new().absolute();
        mark_floating(&mut panel, "menu1", "t1", PopupPlacement::BottomStart);
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_W, "120px");
        panel.layout_bounds = [0.0, 0.0, 120.0, 80.0];

        let mut root = Element::new().child(scroller).child(panel);
        root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

        assert!(sync_floating_placement(&mut root, 800.0, 600.0));

        let menu = &root.children[1];
        let left = inset_length(menu.style.inset.left).expect("expected Length left inset");
        let top = inset_length(menu.style.inset.top).expect("expected Length top inset");
        assert!(
            (left - 40.0).abs() < 0.5,
            "x should follow visual trigger, got {left}"
        );
        // visual trigger bottom = 100+36=136; gap 8 → y=144 (not unscrolled 500+36+8=544)
        assert!(
            (top - 144.0).abs() < 0.5,
            "y must use scrolled visual bounds (expect ~144), got {top}"
        );
    }

    #[test]
    fn sync_floating_uses_scroll_viewport_prefix_when_overflow_visible() {
        // ScrollArea viewport may keep scroll_offset after overflow is cleared to Visible.
        // Paint/hit-test still accumulate via `scroll:` focus id; floating must too.
        let mut trigger = Element::new().focus_id("t-sv");
        trigger.layout_bounds = [20.0, 480.0, 100.0, 32.0];

        let mut scroller = Element::new().child(trigger);
        scroller.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}gallery"));
        scroller.style.overflow.y = taffy::style::Overflow::Visible;
        scroller.scroll_offset = [0.0, 360.0];
        scroller.layout_bounds = [0.0, 0.0, 800.0, 600.0];

        let mut panel = Element::new().absolute();
        mark_floating(&mut panel, "menu-sv", "t-sv", PopupPlacement::BottomStart);
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_W, "100px");
        panel.layout_bounds = [0.0, 0.0, 100.0, 80.0];

        let mut root = Element::new().child(scroller).child(panel);
        root.layout_bounds = [0.0, 0.0, 800.0, 600.0];

        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let top =
            inset_length(root.children[1].style.inset.top).expect("expected Length top inset");
        // visual trigger y=120, h=32, gap 8 → y=160 (not unscrolled 480+32+8=520)
        assert!(
            (top - 160.0).abs() < 0.5,
            "y must follow scroll: viewport offset, got {top}"
        );
    }

    #[test]
    fn sync_floating_reapplies_place_onto_fresh_panel() {
        // First open writes place into the signal + element.
        let mut trigger = Element::new().focus_id("t-reopen");
        trigger.layout_bounds = [80.0, 120.0, 100.0, 32.0];

        let mut panel = Element::new().absolute();
        mark_floating(
            &mut panel,
            "reopen-menu",
            "t-reopen",
            PopupPlacement::BottomStart,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_W, "100px");
        panel.layout_bounds = [0.0, 0.0, 100.0, 60.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let top1 = inset_length(root.children[1].style.inset.top).unwrap();
        // trigger bottom 152 + gap 8 → 160
        assert!((top1 - 160.0).abs() < 0.5, "first place y={top1}");

        // Second open: brand-new panel with auto insets; signal already holds the place.
        // Old bug skipped writing when signal was unchanged → stuck at (0,0) / zero-size.
        let mut trigger2 = Element::new().focus_id("t-reopen");
        trigger2.layout_bounds = [80.0, 120.0, 100.0, 32.0];
        let mut panel2 = Element::new().absolute();
        mark_floating(
            &mut panel2,
            "reopen-menu",
            "t-reopen",
            PopupPlacement::BottomStart,
        );
        panel2.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel2.set_attr(ATTR_FLOATING_W, "100px");
        panel2.layout_bounds = [0.0, 0.0, 100.0, 60.0];
        apply_stored_floating_place(&mut panel2, "reopen-menu");
        assert!(
            inset_length(panel2.style.inset.left).is_some(),
            "seed must set left before layout"
        );

        let mut root2 = Element::new().child(trigger2).child(panel2);
        // Even if sync considers the signal clean, it must still write onto the new node.
        let _ = sync_floating_placement(&mut root2, 800.0, 600.0);
        let left = inset_length(root2.children[1].style.inset.left).unwrap();
        let top = inset_length(root2.children[1].style.inset.top).unwrap();
        let width = dimension_length(root2.children[1].style.size.width).unwrap();
        assert!((left - 80.0).abs() < 0.5, "reopen left={left}");
        assert!((top - 160.0).abs() < 0.5, "reopen top={top}");
        assert!((width - 100.0).abs() < 0.5, "reopen width={width}");
    }

    #[test]
    fn sync_floating_fixed_strategy_keeps_height_near_viewport_edge() {
        let mut trigger = Element::new().focus_id("t-fixed");
        trigger.layout_bounds = [40.0, 520.0, 120.0, 36.0];

        let mut panel = Element::new().absolute();
        mark_floating(
            &mut panel,
            "dp-fixed",
            "t-fixed",
            PopupPlacement::BottomStart,
        );
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.set_attr(ATTR_FLOATING_MIN_W, "0");
        panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
        panel.layout_bounds = [0.0, 0.0, 280.0, 320.0];

        let mut root = Element::new().child(trigger).child(panel);
        root.layout_bounds = [0.0, 0.0, 800.0, 600.0];
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));

        let menu = &root.children[1];
        let top = inset_length(menu.style.inset.top).unwrap();
        // trigger bottom 556 + gap 8 → 564; not flipped above
        assert!(
            (top - 564.0).abs() < 0.5,
            "fixed y must stay below trigger, got {top}"
        );
        assert!(
            menu.style.max_size.height.is_auto(),
            "fixed strategy must not clamp max_h to remaining viewport"
        );
    }

    #[test]
    fn underlay_close_respects_disarm_until_arm() {
        disarm_underlay_close();
        let open = Signal::new(true);
        let underlay = underlay_close_on_click(open, 1000);
        if let Some(cb) = &underlay.on_click {
            cb.borrow_mut()();
        }
        assert!(open.peek(), "disarmed underlay must not close");
        arm_underlay_close();
        if let Some(cb) = &underlay.on_click {
            cb.borrow_mut()();
        }
        assert!(!open.peek(), "armed underlay closes");
    }

    #[test]
    fn underlay_close_arm_is_per_window_scope() {
        use crate::app::window_scope::{enter, WindowScope};
        let _p = enter(WindowScope::Primary);
        disarm_underlay_close();
        assert!(!underlay_close_is_armed());
        drop(_p);

        let _s = enter(WindowScope::Secondary(7));
        assert!(
            underlay_close_is_armed(),
            "secondary scope must not inherit primary disarm"
        );
        disarm_underlay_close();
        drop(_s);

        let _p2 = enter(WindowScope::Primary);
        assert!(
            !underlay_close_is_armed(),
            "primary disarm must survive secondary session"
        );
        arm_underlay_close();
    }

    fn is_zero_len(d: taffy::style::Dimension) -> bool {
        crate::layout::dim_length_px(d).is_some_and(|n| n.abs() < f32::EPSILON)
    }

    fn find_anchored_overlay(el: &Element) -> bool {
        let child_is_portal_floating = el.children.iter().any(|c| {
            c.attrs.get("rui_portal").map(String::as_str) == Some("1")
                && floating_id_of(c).is_some()
        });
        (is_zero_len(el.style.size.width)
            && is_zero_len(el.style.size.height)
            && child_is_portal_floating)
            || el.children.iter().any(find_anchored_overlay)
    }

    fn has_portal_mark(el: &Element) -> bool {
        el.attrs.get("rui_portal").map(String::as_str) == Some("1")
            || el.children.iter().any(has_portal_mark)
    }

    fn portal_nests_portal(el: &Element) -> bool {
        let self_p = el.attrs.get("rui_portal").map(String::as_str) == Some("1");
        if self_p && el.children.iter().any(has_portal_mark) {
            return true;
        }
        el.children.iter().any(portal_nests_portal)
    }

    #[test]
    fn compute_cursor_menu_prefers_bottom_right() {
        let (x, y, max_h) =
            compute_cursor_menu_placement([100.0, 80.0], [800.0, 600.0], 160.0, 120.0, 2.0);
        assert!((x - 102.0).abs() < 0.5, "x={x}");
        assert!((y - 82.0).abs() < 0.5, "y={y}");
        assert!(max_h >= 120.0, "max_h={max_h}");
    }

    #[test]
    fn compute_cursor_menu_flips_before_translate() {
        // Near bottom-right: preferred side can't hold the menu; opposite can → flip
        // (stay attached to the cursor) instead of sliding to the viewport edge.
        let (x, y, _) =
            compute_cursor_menu_placement([740.0, 520.0], [800.0, 600.0], 160.0, 120.0, 0.0);
        assert!(
            (x - (740.0 - 160.0)).abs() < 0.5,
            "must flip left of cursor, x={x}"
        );
        assert!(
            (y - (520.0 - 120.0)).abs() < 0.5,
            "must flip above cursor, y={y}"
        );
    }

    #[test]
    fn compute_cursor_menu_translates_when_flip_still_overflows() {
        // Menu wider than space on either side of the cursor → flip left, then translate.
        let (x, y, _) =
            compute_cursor_menu_placement([100.0, 40.0], [800.0, 600.0], 400.0, 80.0, 0.0);
        // Right of cursor has 692px (>400) so no horizontal flip; stays at x=100.
        assert!((x - 100.0).abs() < 0.5, "x={x}");
        assert!((y - 40.0).abs() < 0.5, "y={y}");

        // Cursor near left with a menu wider than remaining right space → translate.
        let (x2, _, _) =
            compute_cursor_menu_placement([50.0, 40.0], [800.0, 600.0], 760.0, 80.0, 0.0);
        assert!(
            (x2 - (800.0 - 8.0 - 760.0)).abs() < 0.5,
            "must translate after flip fails, x={x2}"
        );
    }

    #[test]
    fn compute_cursor_menu_caps_height_when_taller_than_viewport() {
        let (x, y, max_h) =
            compute_cursor_menu_placement([40.0, 500.0], [800.0, 600.0], 160.0, 900.0, 0.0);
        assert!((x - 40.0).abs() < 0.5, "x={x}");
        // Flip above first, then translate up to the margin, then clamp max_h.
        assert!(
            y <= 8.0 + 0.5,
            "should end near top after flip+translate, y={y}"
        );
        assert!(
            max_h <= 600.0 - 16.0 + 0.5,
            "max_h must fit viewport, max_h={max_h}"
        );
        assert!(max_h >= 48.0);
    }

    #[test]
    fn mount_anchored_overlay_is_zero_anchor_with_portal_panel() {
        let mut panel = Element::new().text("menu");
        mark_floating(&mut panel, "sel", "trig", PopupPlacement::BottomStart);
        let root = mount_anchored_overlay(panel, 1001);
        assert!(
            find_anchored_overlay(&root),
            "anchored overlay must be 0×0 host + rui_portal + rui_floating"
        );
        assert!(!portal_nests_portal(&root), "must not nest rui_portal");
    }

    #[test]
    fn mount_viewport_stage_does_not_nest_portal() {
        let inner = swallow_clicks(Element::new().text("dialog"));
        let stage = Element::new().w("100%").h("100%").child(inner);
        let root = mount_modal_layer(
            underlay_close_on_click(Signal::new(true), 1000),
            mount_viewport_portal_stage(stage, 1001),
        );
        assert!(!portal_nests_portal(&root));
        let mut n = 0usize;
        fn count(el: &Element, n: &mut usize) {
            if el.attrs.get("rui_portal").map(String::as_str) == Some("1") {
                *n += 1;
            }
            for c in &el.children {
                count(c, n);
            }
        }
        count(&root, &mut n);
        assert_eq!(n, 2, "underlay + stage each portal once");
    }

    #[test]
    fn floating_panel_after_hoist_occludes_page_hits() {
        use crate::app::pointer_events::hit_path;
        use crate::app::portal_overlay::{
            clear_overlay_stack, portal_sync, OverlayKind, OverlayMode,
        };
        use crate::core::reactivity::{create_root, Signal};
        use crate::core::tree::PointerEvents;
        use crate::layout::layout_tree::{
            hoist_portal_subtrees, ATTR_HOISTED_PORTAL, PORTAL_LAYER_ID,
        };

        clear_overlay_stack();
        let (_owner, mut root) = create_root(|| {
            let open = Signal::new(true);
            let oz = portal_sync("cp-hit", open, OverlayKind::Popover, OverlayMode::Stacked)
                .expect("stacked popover");

            // Full-page button under the future panel silhouette.
            let under = Element::new()
                .w(400.0)
                .h(400.0)
                .bg([1.0, 0.0, 0.0, 1.0])
                .focus_id("under-btn")
                .on_click(|| {});

            // Minimal floating panel (ColorPicker / Select chrome). Gaps between
            // children must still hit the panel — not the page underneath.
            let mut panel = Element::new()
                .w(200.0)
                .h(200.0)
                .bg([0.0, 1.0, 0.0, 1.0])
                .absolute()
                .left(100.0)
                .top(100.0)
                .child(
                    Element::new()
                        .w(200.0)
                        .h(40.0)
                        .bg([0.0, 0.0, 1.0, 1.0])
                        .pointer_events_none(),
                )
                .child(Element::new().w(200.0).h(40.0).bg([0.0, 0.5, 1.0, 1.0]));
            mark_floating(
                &mut panel,
                "cp-hit",
                "cp-hit:trigger",
                PopupPlacement::BottomStart,
            );
            let overlay = mount_anchored_overlay(panel, oz.panel);
            Element::new().w(400.0).h(400.0).child(under).child(overlay)
        });

        assert!(hoist_portal_subtrees(&mut root) >= 1);
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 400.0, 400.0, [0.0, 0.0]);

        let layer = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some(PORTAL_LAYER_ID))
            .expect("portal layer");
        let hoisted = layer
            .children
            .iter()
            .find(|c| c.attrs.get(ATTR_HOISTED_PORTAL).map(|s| s.as_str()) == Some("1"))
            .expect("hoisted panel");
        assert_eq!(
            hoisted.pointer_events_mode(),
            PointerEvents::Auto,
            "floating panel must claim its box after hoist"
        );
        assert!(
            hoisted.on_click.is_some(),
            "swallow_clicks must survive hoist"
        );

        // Mid-panel, between the two children (gap / chrome) — previously box-none
        // would miss here and hit `under-btn`.
        let path = hit_path(&root, 200.0, 180.0);
        assert!(
            path.iter().any(|e| {
                e.floating_id.as_deref() == Some("cp-hit")
                    || e.portal_key.as_deref() == Some("panel:cp-hit")
            }),
            "hit must stay on floating panel; path={:?}",
            path.iter()
                .map(|e| (
                    e.focus_id.clone(),
                    e.floating_id.clone(),
                    e.portal_key.clone()
                ))
                .collect::<Vec<_>>()
        );
        assert!(
            path.iter()
                .all(|e| e.focus_id.as_deref() != Some("under-btn")),
            "page control under the panel must not receive the hit"
        );
        clear_overlay_stack();
    }

    #[test]
    fn path_hits_floating_trigger_before_menu_portal_hoists() {
        use crate::app::pointer_events::HitPathEntry;

        let root = Element::new();
        let path = [HitPathEntry {
            focus_id: Some("gal-dialog-nested-select:trigger".into()),
            ..Default::default()
        }];
        assert!(
            path_hits_floating_trigger(&root, &path),
            "opening click on SelectTrigger must count as in-bounds before menu portal mounts"
        );
    }

    #[test]
    fn fixed_anchor_negative_gap_covers_trigger_without_flipping() {
        let trigger = [10.0, 100.0, 80.0, 32.0];
        let viewport = [800.0, 600.0];
        let (_x, y) = compute_fixed_anchor(
            trigger,
            viewport,
            80.0,
            64.0,
            PopupPlacement::BottomStart,
            -32.0,
        );
        assert!(
            (y - 100.0).abs() < 0.1,
            "bottom + negative gap: panel top sits on the trigger top (got {y})"
        );
        let (_x, y) = compute_fixed_anchor(
            trigger,
            viewport,
            80.0,
            64.0,
            PopupPlacement::TopStart,
            -32.0,
        );
        assert!(
            (y - 68.0).abs() < 0.1,
            "top + negative gap: panel bottom sits on the trigger bottom (got {y})"
        );
        let (_x, y) = compute_fixed_anchor(
            trigger,
            viewport,
            80.0,
            64.0,
            PopupPlacement::BottomStart,
            -8.0,
        );
        assert!(
            (y - 100.0).abs() < 0.1,
            "any negative gap covers using -trigger_height, not the raw gap (got {y})"
        );
        let edge = [790.0, 100.0, 80.0, 32.0];
        let (x, _y) = compute_fixed_anchor(
            edge,
            viewport,
            80.0,
            32.0,
            PopupPlacement::BottomStart,
            -32.0,
        );
        assert!(
            (x - 790.0).abs() < 0.1,
            "cover placement must not clamp away from the trigger (got {x})"
        );
    }

    #[test]
    fn fixed_cover_top_glues_bottom_when_height_grows() {
        let mut trigger = Element::new().focus_id("rx-t");
        trigger.layout_bounds = [20.0, 200.0, 96.0, 28.0];

        let mut panel = Element::new().absolute();
        mark_floating_with_gap(
            &mut panel,
            "rx-expand",
            "rx-t",
            PopupPlacement::TopStart,
            -28.0,
        );
        panel.set_attr(ATTR_FLOATING_STRATEGY, "fixed");
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        // First layout: short estimate (collapsed).
        panel.layout_bounds = [0.0, 0.0, 96.0, 28.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let y0 = inset_length(root.children[1].style.inset.top).expect("top");
        assert!(
            (y0 - 200.0).abs() < 0.5,
            "cover top with h=trigger: panel top aligns with trigger top, got {y0}"
        );

        // Wrap grew the panel — bottom seam must stay on trigger bottom (228).
        root.children[1].layout_bounds = [20.0, y0, 96.0, 88.0];
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let y1 = inset_length(root.children[1].style.inset.top).expect("top after grow");
        assert!(
            (y1 - (228.0 - 88.0)).abs() < 0.5,
            "growing upward must keep bottom on trigger bottom, got {y1}"
        );
        assert_eq!(
            root.children[1]
                .attrs
                .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
                .map(String::as_str),
            Some("228")
        );
    }
}
