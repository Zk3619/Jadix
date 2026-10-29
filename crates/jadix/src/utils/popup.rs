//! 浮层几何与 Portal 辅助（**能力层 / 行为工具**，非样式组件）。
//!
//! 按 [`docs/COMPONENT_LIBRARY.md`](../../../../docs/COMPONENT_LIBRARY.md)：本模块只做
//! `mark_floating`、锚点 placement、viewport 钳位、underlay/portal 标记。
//! **产品挂载只走两条入口**：锚定浮层 [`mount_anchored_overlay`]；视口模态舞台
//! [`mount_viewport_portal_stage`]（+ 可选 [`mount_modal_layer`]）。禁止组件再手写
//! 第二套坐标 sync 或嵌套 `jadix_portal`。
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
    let _ = UNDERLAY_CLOSE_ARMED.try_with(|m| {
        m.borrow_mut().insert(scope, armed);
    });
}

fn armed_map_get() -> bool {
    let scope = window_scope::active();
    UNDERLAY_CLOSE_ARMED
        .try_with(|m| *m.borrow().get(&scope).unwrap_or(&true))
        .unwrap_or(true)
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
pub const ATTR_FLOATING: &str = "jadix_floating";
pub const ATTR_FLOATING_TRIGGER: &str = "jadix_floating_trigger";
pub const ATTR_FLOATING_PLACEMENT: &str = "jadix_floating_placement";
/// Definite panel width (`px` / `rem`). Takes precedence over match-trigger.
pub const ATTR_FLOATING_W: &str = "jadix_floating_w";
/// Optional max-height cap (`px` / `rem`). Intersected with viewport-available height.
pub const ATTR_FLOATING_MAX_H: &str = "jadix_floating_max_h";
/// `1` → panel width equals trigger; `0` → content width (`width: auto`) with min-width floor.
pub const ATTR_FLOATING_MATCH_TRIGGER: &str = "jadix_floating_match_trigger";
/// Optional fixed height (`px` / `rem`).
pub const ATTR_FLOATING_H: &str = "jadix_floating_h";
/// Optional min-height (`px` / `rem`). Applied only when set — no style default here.
pub const ATTR_FLOATING_MIN_H: &str = "jadix_floating_min_h";
/// Optional min-width: absolute (`8rem`) or relative to trigger (`trigger`, `trigger+1.75rem`).
pub const ATTR_FLOATING_MIN_W: &str = "jadix_floating_min_w";
/// Optional per-panel gap in px between trigger and floating panel (default [`FLOATING_TRIGGER_GAP`]).
pub const ATTR_FLOATING_GAP: &str = "jadix_floating_gap";
/// `flip`（默认）：视口内翻转并按剩余空间夹 `max_h`。`fixed`：钉在触发器首选侧，尺寸不随滚动挤压（可溢出视口）。
pub const ATTR_FLOATING_STRATEGY: &str = "jadix_floating_strategy";
/// Cursor / point anchor `"x,y"`（逻辑像素）。有值时忽略触发器几何，走 [`compute_cursor_menu_placement`]。
pub const ATTR_FLOATING_POINT: &str = "jadix_floating_point";
/// Intrinsic content height (logical px) for placement `estimate_h` while accordion clips.
pub(crate) const ATTR_FLOATING_CONTENT_H: &str = "jadix_floating_content_h";
/// Expand is taking an unconstrained measure frame — placement must not write viewport `max_h`.
pub(crate) const ATTR_FLOATING_EXPAND_MEASURE: &str = "jadix_floating_expand_measure";
/// Trigger-facing edge y when the panel is placed above; accordion `top` tracks visual height from this.
pub(crate) const ATTR_FLOATING_ANCHOR_EDGE_Y: &str = "jadix_floating_anchor_edge_y";
/// Presence is exiting, including the rebuild before exit keyframes are seeded.
pub(crate) const ATTR_FLOATING_EXITING: &str = "jadix_floating_exiting";
/// Expand enter/exit baseline height (logical px). When set, accordion clips from this height
/// instead of 0 so covered triggers (e.g. bubble reaction bar) continue downward/upward.
/// Exit lands on this height (the in-flow pill), then the portal hides — not accordion-up to 0.
pub const ATTR_FLOATING_EXPAND_FROM_H: &str = "jadix_floating_expand_from_h";
/// Expand enter/exit baseline width (logical px). When set, `max-width` interpolates with the
/// accordion so row-wise content (e.g. AvatarGroup +N) is clipped instead of popping in/out.
pub const ATTR_FLOATING_EXPAND_FROM_W: &str = "jadix_floating_expand_from_w";
/// When `"1"` with [`ATTR_FLOATING_EXPAND_FROM_W`], lerp `max-width` and `nowrap` (horizontal
/// clip). When unset, pin width to `from_w` so wrapped rows unveil via `max-height` (Bubble +N).
pub(crate) const ATTR_FLOATING_EXPAND_CLIP_X: &str = "jadix_floating_expand_clip_x";

/// Inset from the viewport edges so floating panels never sit flush against the window.
pub const FLOATING_VIEWPORT_MARGIN: f32 = 8.0;
/// Default gap between the trigger and the floating panel (placement geometry).
///
/// Matches ShadCN `sideOffset` (4px) used by Popover / Select / DatePicker / Tooltip.
pub const FLOATING_TRIGGER_GAP: f32 = 4.0;

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

/// Like [`mark_floating`], with an explicit trigger–panel gap in px.
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
    anchor_edge_y: Option<f32>,
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
            anchor_edge_y: None,
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
        ) || a.name.starts_with("jadix-expand-down-")
            || a.name.starts_with("jadix-expand-up-")
    })
}

fn expand_enter_animation_active(el: &Element) -> bool {
    el.style.animations.iter().any(|a| {
        matches!(a.name.as_str(), "accordion-down" | "collapsible-down")
            || a.name.starts_with("jadix-expand-down-")
    })
}

fn expand_measure_pending(el: &Element) -> bool {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_MEASURE)
        .is_some_and(|s| s == "1")
}

/// Placement must not own `max-size.height` while accordion keyframes run, or while
/// Expand is measuring unconstrained content height.
fn expand_from_h_set(el: &Element) -> bool {
    el.attrs
        .get(ATTR_FLOATING_EXPAND_FROM_H)
        .and_then(|s| s.parse::<f32>().ok())
        .is_some_and(|h| h > 0.5)
}

fn reverse_expand_edge_y(el: &Element) -> Option<f32> {
    el.attrs
        .get(ATTR_FLOATING_ANCHOR_EDGE_Y)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|e| *e > 0.0)
}

fn floating_content_h(el: &Element) -> Option<f32> {
    el.attrs
        .get(ATTR_FLOATING_CONTENT_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 0.5)
}

/// Auto-height scroll panels must hug the in-flow list, not the viewport cap.
/// ScrollArea's flex shells can fill max-height even when their rows are shorter.
/// Follow the single-child shell chain to the viewport; ignore absolute scrollbars
/// and portal placeholders so submenus cannot inflate the parent's measurement.
pub(crate) fn floating_scroll_content_h(el: &Element) -> Option<f32> {
    if !el.style.size.height.is_auto() || el.attrs.contains_key(ATTR_FLOATING_H) {
        return None;
    }
    let mut children = el.children.iter().filter(|c| {
        c.style.position != taffy::style::Position::Absolute
            && c.style.display != taffy::style::Display::None
    });
    let child = children.next()?;
    let bottom = match crate::layout::lp_kind(el.style.padding.bottom) {
        crate::layout::DimKind::Length(px) => px,
        crate::layout::DimKind::Percent(pct) => pct * el.layout_bounds[2],
        crate::layout::DimKind::Auto => 0.0,
    } + el.style.resolved_border_widths()[2];
    let content = if el
        .focus_id
        .as_deref()
        .is_some_and(|id| id.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX))
    {
        std::iter::once(child)
            .chain(children)
            .map(|c| c.layout_bounds[1] - el.layout_bounds[1] + c.layout_bounds[3])
            .fold(0.0_f32, f32::max)
    } else {
        if children.next().is_some() {
            return None;
        }
        child.layout_bounds[1] - el.layout_bounds[1] + floating_scroll_content_h(child)?
    };
    (content > 0.5).then_some(content + bottom)
}

fn skip_placement_max_h(el: &Element) -> bool {
    // Expand panels own max-height from the first rebuild (`from_h` is on the fresh
    // tree before MEASURE / keyframes). Writing viewport cap here is a 1-frame
    // fullscreen flash of the muted chrome. Raising a max-height that is still
    // shorter than CONTENT_H unclips that silhouette (enter from 0 / mid-exit).
    el.attrs.contains_key(ATTR_FLOATING_EXITING)
        || height_keyframe_animation_active(el)
        || expand_measure_pending(el)
        || expand_from_h_set(el)
        || accordion_clip_shorter_than_content(el)
}

fn accordion_clip_shorter_than_content(el: &Element) -> bool {
    let Some(cur) = dimension_length(el.style.max_size.height) else {
        return false;
    };
    let Some(content) = floating_content_h(el) else {
        return false;
    };
    cur + 0.5 < content
}

/// True when `clip_h` is the author/place max-h CAP (e.g. `16rem` / viewport
/// remainder), not an accordion-compressed silhouette.
fn style_max_h_is_place_cap(el: &Element, clip_h: f32) -> bool {
    floating_attr_max_h(el).is_some_and(|cap| (clip_h - cap).abs() < 1.0)
}

/// Painted height for reverse-expand glue: accordion clip cannot exceed intrinsic
/// content. Author / place `max-h` (e.g. `16rem`) is a cap, not the silhouette —
/// using it as `visual_h` floats an open menu too high. A leftover clip shorter
/// than content is the collapsing silhouette — using CONTENT_H jumps `top` up.
///
/// Critical: never glue with unconstrained content / layout height / place CAP
/// while Expand is measuring or has not yet written an accordion clip. Placement
/// and `prepare_expand_measure` both write the viewport CAP onto style max-height
/// before keyframes own it — treating that as painted height parks an upward
/// menu a full remainder above the trigger until enter seeds max-height≈0 and
/// the seam snaps back (inline ColorPicker format Select).
fn reverse_glue_visual_h(el: &Element, estimate_h: f32, animating_h: bool) -> f32 {
    let content = floating_content_h(el);
    let clip = dimension_length(el.style.max_size.height).filter(|h| *h >= 0.0);
    let laid_out = el.layout_bounds[3];

    // Measure intentionally sets max-h to the place CAP so Taffy can resolve
    // content. That CAP must not drive reverse-expand `top`.
    if expand_measure_pending(el) {
        if laid_out > 0.5 {
            return content.map(|c| laid_out.min(c)).unwrap_or(laid_out);
        }
        return 0.0;
    }

    if animating_h || expand_enter_animation_active(el) {
        // Prefer the accordion silhouette. A pre-keyframe place CAP (≥ content)
        // still sitting on style is not paint — grow from the trigger edge (0).
        let h = match clip {
            Some(h)
                if expand_enter_animation_active(el)
                    && !accordion_clip_shorter_than_content(el)
                    && content.is_some_and(|c| h + 0.5 >= c)
                    && style_max_h_is_place_cap(el, h) =>
            {
                0.0
            }
            Some(h) => h,
            None => 0.0,
        };
        return content.map(|c| h.min(c)).unwrap_or(h);
    }

    // Pre-enter Expand: CONTENT_H is known but accordion has not seeded yet.
    // Gluing with full content / place CAP here is the same gap→snap as above.
    // Plain menus (no author max-h) still use CONTENT_H for flip-above estimate.
    if clip.is_none() && content.is_some() && floating_attr_max_h(el).is_some() {
        return 0.0;
    }
    if let (Some(h), Some(c)) = (clip, content) {
        if !accordion_clip_shorter_than_content(el)
            && (style_max_h_is_place_cap(el, h) || h + 0.5 >= c)
        {
            // CAP-only (or fully open under CAP): never invent height from the
            // remainder. Prefer the box that was actually laid out.
            if laid_out > 0.5 {
                return laid_out.min(c);
            }
            if style_max_h_is_place_cap(el, h) || h > c + 0.5 {
                return 0.0;
            }
        }
    }
    if let Some(c) = content {
        let estimated = clip.map(|h| h.min(c)).unwrap_or(c);
        // Idle ScrollArea can shrink after its animation constraints are
        // removed, while CONTENT_H retains the larger intrinsic measurement.
        // Anchor the box that was actually laid out, not that historical cap.
        return if laid_out > 0.5 && inset_length(el.style.inset.top).is_some() {
            estimated.min(laid_out)
        } else {
            estimated
        };
    }
    laid_out.max(estimate_h)
}

/// Author `max-height` cap (`jadix_floating_max_h` / legacy `jadix_select_max_h`).
fn floating_attr_max_h(el: &Element) -> Option<f32> {
    el.attrs
        .get(ATTR_FLOATING_MAX_H)
        .or_else(|| el.attrs.get("jadix_select_max_h"))
        .map(|s| parse_css_px(s, 0.0))
        .filter(|h| *h > 0.0)
}

fn floating_estimate_h(el: &Element) -> f32 {
    let raw = if let Some(h) = el
        .attrs
        .get(ATTR_FLOATING_CONTENT_H)
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|h| *h > 0.5)
    {
        h
    } else if let Some(h) = floating_scroll_content_h(el) {
        h
    } else if el.layout_bounds[3] > 0.5 {
        el.layout_bounds[3]
    } else {
        32.0
    };
    // Flip / available-height must use the painted cap, not unconstrained content
    // (calendar year list is thousands of px; Select `max-h-16rem` still fits below).
    match floating_attr_max_h(el) {
        Some(cap) => raw.min(cap),
        None => raw,
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
        let mut visual_h = reverse_glue_visual_h(el, 0.0, true);
        if !expand_enter_animation_active(el) {
            if let Some(top) = inset_length(el.style.inset.top) {
                let glued = (edge - top).max(0.0);
                if glued > 0.5 {
                    visual_h = visual_h.min(glued);
                }
            }
        }
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
        // `jadix_floating_min_w=trigger+1.75rem`).
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
        //
        // Place CAP is an overflow limit only. Writing it as style max-height when the
        // list is shorter stretches ScrollArea `h:100%` into a wheel-scrollable empty
        // band (inline ColorPicker format Select opening upward with room above).
        // `apply_idle_expand_max_h` hugs CONTENT_H, but App present runs sync *after*
        // flush — without this clamp the next write undoes the hug every frame.
        if place.max_h <= 0.0 {
            if !el.style.max_size.height.is_auto() {
                el.style.max_size.height = taffy::style::Dimension::auto();
                changed = true;
            }
        } else {
            let mut mh = place.max_h.max(1.0);
            if let Some(content) = floating_content_h(el).or_else(|| floating_scroll_content_h(el))
            {
                mh = mh.min(content);
            }
            if dimension_length(el.style.max_size.height).is_none_or(|v| (v - mh).abs() > 0.5) {
                el.style.max_size.height = taffy::style::Dimension::length(mh);
                changed = true;
            }
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
            .or_else(|| el.attrs.get("jadix_select_match_trigger"))
            .map(|s| s.as_str() != "0")
            .unwrap_or(true);
        if let Some(w) = el
            .attrs
            .get(ATTR_FLOATING_W)
            .or_else(|| el.attrs.get("jadix_select_menu_w"))
            .and_then(|w| parse_explicit_floating_w(w, 0.0))
        {
            p.w = w;
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
        // No author cap → `max_h = 0` (style auto). A sentinel `4096` used to be
        // written as `max-height: 4096px` before the first sync; percent-height
        // children (or a one-frame flex stretch) then filled that band.
        p.max_h = el
            .attrs
            .get(ATTR_FLOATING_MAX_H)
            .or_else(|| el.attrs.get("jadix_select_max_h"))
            .map(|s| parse_css_px(s, 0.0))
            .unwrap_or(0.0);
        place.set_silent(p);
    }
    // Always write size so absolute panels are not 160×256 defaults. Drop only the
    // reverse-expand edge so a fresh open reseeds against the live trigger — but
    // still seed XY from the last place so the panel is not a zero box at the
    // portal origin before the first sync (HoverCard / Tooltip reopen flash).
    if el.attrs.contains_key(ATTR_FLOATING_EXITING) {
        if let Some(edge) = p.anchor_edge_y {
            el.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, format!("{edge}"));
        }
        let _ = write_floating_place(el, p);
    } else {
        p.anchor_edge_y = None;
        el.attrs.remove(ATTR_FLOATING_ANCHOR_EDGE_Y);
        // A previous viewport cap is not the new panel's intrinsic height.
        // Remeasure fresh content under the author's cap before deciding whether
        // to flip; otherwise reopening after a small viewport ratchets the size.
        p.max_h = floating_attr_max_h(el).unwrap_or(0.0);
        place.set_silent(p);
        let _ = write_floating_place(el, p);
    }
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

/// Explicit panel width. `"auto"` / empty is content-sized — not a length (would
/// otherwise `parse` as the fallback, usually trigger width).
fn parse_explicit_floating_w(raw: &str, fallback: f32) -> Option<f32> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(parse_css_px(s, fallback))
    }
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
        let scroll_acc = if crate::layout::layout_tree::escapes_ancestor_overflow(el) {
            [0.0_f32, 0.0_f32]
        } else {
            scroll_acc
        };
        if let Some(fid) = &el.focus_id {
            let b = crate::core::tree::visual_paint_aabb(el, scroll_acc);
            out.push((fid.clone(), b));
        }
        let mut next_acc = scroll_acc;
        let scale = crate::core::tree::paint_scale_with_ancestor(el, el.paint_ancestor_scale);
        let is_scroll_viewport = el
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
        if is_scroll_viewport || el.style.overflow.x != taffy::style::Overflow::Visible {
            next_acc[0] += el.scroll_offset[0] * scale[0];
        }
        if is_scroll_viewport || el.style.overflow.y != taffy::style::Overflow::Visible {
            next_acc[1] += el.scroll_offset[1] * scale[1];
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
        .or_else(|| el.attrs.get("jadix_select_menu").cloned())
}

fn floating_trigger_of(el: &Element) -> Option<String> {
    el.attrs
        .get(ATTR_FLOATING_TRIGGER)
        .cloned()
        .or_else(|| el.attrs.get("jadix_select_trigger").cloned())
}

/// Last non-zero visual box for `fid`.
///
/// `collect_focus_bounds` is document-order: an in-page 0×0 slot / stale ghost is
/// first, the hoisted copy inside the portal layer is last. `.find` anchored
/// nested Select / inline calendar dropdowns at the ghost (top of the gallery).
fn floating_trigger_bounds(triggers: &[(String, [f32; 4])], fid: &str) -> Option<[f32; 4]> {
    let mut best = None;
    for (f, b) in triggers {
        if f == fid && b[2] > 0.0 && b[3] > 0.0 {
            best = Some(*b);
        }
    }
    best
}

fn apply_floating_menus(
    el: &mut Element,
    triggers: &[(String, [f32; 4])],
    viewport: [f32; 2],
    point_space: Option<[f32; 4]>,
    scroll_acc: [f32; 2],
    changed: &mut bool,
) {
    let scroll_acc = if crate::layout::layout_tree::escapes_ancestor_overflow(el) {
        [0.0; 2]
    } else {
        scroll_acc
    };
    let next_scroll = crate::core::tree::child_scroll_accumulation(el, scroll_acc);
    let local_mount = el.template_id.as_deref().is_some_and(|mount| {
        el.children.iter().any(|child| {
            child
                .attrs
                .get(crate::layout::layout_tree::ATTR_HOISTED_MOUNT)
                .is_some_and(|target| target == mount)
        })
    });
    if local_mount
        || el
            .attrs
            .contains_key(crate::core::scaled_viewport::ATTR_PORTALS)
    {
        let bounds = crate::core::tree::visual_paint_aabb(el, scroll_acc);
        let size = [el.layout_bounds[2].max(1.0), el.layout_bounds[3].max(1.0)];
        let scale = [
            (bounds[2] / size[0]).max(0.01),
            (bounds[3] / size[1]).max(0.01),
        ];
        let source = point_space.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        let local: Vec<_> = triggers
            .iter()
            .map(|(id, rect)| {
                (
                    id.clone(),
                    [
                        (rect[0] * source[2] + source[0] - bounds[0]) / scale[0],
                        (rect[1] * source[3] + source[1] - bounds[1]) / scale[1],
                        rect[2] * source[2] / scale[0],
                        rect[3] * source[3] / scale[1],
                    ],
                )
            })
            .collect();
        for child in &mut el.children {
            apply_floating_menus(
                child,
                &local,
                size,
                Some([bounds[0], bounds[1], scale[0], scale[1]]),
                next_scroll,
                changed,
            );
        }
        return;
    }
    let Some(id) = floating_id_of(el) else {
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, point_space, next_scroll, changed);
        }
        return;
    };
    let preferred = PopupPlacement::from_attr(
        el.attrs
            .get(ATTR_FLOATING_PLACEMENT)
            .or_else(|| el.attrs.get("jadix_select_placement"))
            .map(String::as_str)
            .unwrap_or("bottom-start"),
    );

    // Exit owns its side and height cap, but its trigger can still move when an
    // ancestor changes size (e.g. ColorPicker adds/removes its alpha row).
    if el.attrs.contains_key(ATTR_FLOATING_EXITING) {
        if let Some(tb) =
            floating_trigger_of(el).and_then(|fid| floating_trigger_bounds(triggers, &fid))
        {
            let gap = el
                .attrs
                .get(ATTR_FLOATING_GAP)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(FLOATING_TRIGGER_GAP)
                .clamp(-64.0, 64.0);
            let place = use_signal(
                floating_place_signal_key(&id).as_str(),
                FloatingPlace::default,
            );
            if preferred.is_horizontal() {
                let (x, y, _, _) = compute_anchored_placement(
                    tb,
                    viewport,
                    el.layout_bounds[2],
                    el.layout_bounds[3],
                    preferred,
                    gap,
                );
                let mut next = place.peek();
                next.x = x;
                next.y = y;
                next.anchor_edge_y = None;
                el.attrs.remove(ATTR_FLOATING_ANCHOR_EDGE_Y);
                place.set_silent(next);
                *changed |= write_floating_place(el, next);
            } else if let Some(old_edge) = reverse_expand_edge_y(el) {
                // Above: keep the trigger-facing bottom edge glued.
                let edge = tb[1] - gap;
                let delta = edge - old_edge;
                if delta.abs() > 0.5 {
                    let top =
                        inset_length(el.style.inset.top).unwrap_or(el.layout_bounds[1]) + delta;
                    el.style.inset.top = Element::into_margin_auto(top);
                    el.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, format!("{edge}"));
                    let mut next = place.peek();
                    next.y = top;
                    next.anchor_edge_y = Some(edge);
                    place.set_silent(next);
                    *changed = true;
                }
            } else {
                // Below: keep the top glued to the trigger bottom (+ gap).
                let edge = tb[1] + tb[3] + gap;
                let old_top = inset_length(el.style.inset.top).unwrap_or(el.layout_bounds[1]);
                let delta = edge - old_top;
                if delta.abs() > 0.5 {
                    el.style.inset.top = Element::into_margin_auto(edge);
                    let mut next = place.peek();
                    next.y = edge;
                    next.anchor_edge_y = None;
                    place.set_silent(next);
                    *changed = true;
                }
            }
        }
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, point_space, next_scroll, changed);
        }
        return;
    }

    // Title-bar drag writes left/top as paint chrome. Re-running cursor placement
    // would remount-fight and flip/clamp the grab point away from the pointer.
    if crate::utils::anchored_overlay::floating_panel_drag_id().as_deref() == Some(id.as_str()) {
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, point_space, next_scroll, changed);
        }
        return;
    }

    // Unanchored overlays set ATTR_FLOATING (+ optional TRIGGER) so outside-press
    // treats the panel / toggle as inside — without PLACEMENT or POINT, do not
    // rewrite left/top (author `fixed` / inset is the geometry).
    let has_point = floating_point_of(el).is_some();
    let has_placement_attr = el.attrs.get(ATTR_FLOATING_PLACEMENT).is_some()
        || el.attrs.get("jadix_select_placement").is_some();
    let is_select = el.attrs.contains_key("jadix_select_menu");
    if !has_point && !has_placement_attr && !is_select {
        for c in &mut el.children {
            apply_floating_menus(c, triggers, viewport, point_space, next_scroll, changed);
        }
        return;
    }

    let point = floating_point_of(el).map(|point| {
        if let Some([x, y, sx, sy]) = point_space {
            [(point[0] - x) / sx, (point[1] - y) / sy]
        } else {
            point
        }
    });
    let trigger_fid = floating_trigger_of(el);
    let tb = if let Some(pt) = point {
        Some([pt[0], pt[1], 0.0, 0.0])
    } else if let Some(ref fid) = trigger_fid {
        floating_trigger_bounds(triggers, fid)
    } else {
        None
    };

    if let Some(tb) = tb {
        let match_trigger = el
            .attrs
            .get(ATTR_FLOATING_MATCH_TRIGGER)
            .or_else(|| el.attrs.get("jadix_select_match_trigger"))
            .map(|s| s.as_str() != "0")
            .unwrap_or(true);
        let explicit_w = el
            .attrs
            .get(ATTR_FLOATING_W)
            .or_else(|| el.attrs.get("jadix_select_menu_w"))
            .and_then(|s| parse_explicit_floating_w(s, tb[2]));
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
            // explicit `jadix_floating_min_w` wins (e.g. `0` → true w-fit for Tooltip).
            let floor = attr_min_w.unwrap_or(tb[2]).max(0.0);
            (floor, true, floor)
        };
        let attr_max_h = floating_attr_max_h(el);
        let estimate_h = floating_estimate_h(el);
        // Content-auto before measure: use the trigger width (not a 32px stub) so
        // start/end land on the first frame. Center still defers XY until measured —
        // a trigger-wide center guess then jumps when the real width arrives
        // (HoverCard / Tooltip jitter). Skipping XY for *all* unmeasured panels left
        // `apply_stored` (0,0) under Fade enter (Select / DatePicker corner flash).
        let size_unknown = width_auto && el.layout_bounds[2] <= 0.5 && el.layout_bounds[3] <= 0.5;
        let estimate_w = if width_auto {
            if el.layout_bounds[2] > 0.5 {
                el.layout_bounds[2].max(menu_w)
            } else {
                menu_w.max(tb[2]).max(1.0)
            }
        } else {
            menu_w.max(min_w).max(32.0)
        };
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
        let center_align = matches!(
            preferred,
            PopupPlacement::BottomCenter | PopupPlacement::TopCenter
        );
        if size_unknown && center_align {
            let next = FloatingPlace {
                x: 0.0,
                y: 0.0,
                w: menu_w.max(0.0),
                max_h: 0.0,
                width_auto,
                min_w: min_w.max(0.0),
                anchor_edge_y: None,
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
                let max_h = attr_max_h
                    .map(|cap| available_max_h.min(cap))
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
                let max_h = attr_max_h.unwrap_or(0.0);
                (x, y, max_h)
            } else {
                let (x, y, available_max_h, _) = compute_anchored_placement(
                    tb, viewport, estimate_w, estimate_h, preferred, gap,
                );
                let max_h = attr_max_h
                    .map(|cap| available_max_h.min(cap))
                    .unwrap_or(available_max_h)
                    .max(1.0);
                (x, y, max_h)
            };
            // Reverse expand: keep the trigger-facing edge glued while accordion clips.
            // Never drop a committed top placement mid-animation — a clip-sized estimate
            // would re-place the shrinking panel below the trigger (flat sliver flash).
            // Fixed cover already glued the seam above; skip so negative gap is not
            // misread as `ty - gap`.
            // Cursor placement already performs flip + viewport translation. A translated
            // box above the cursor is not necessarily an above-edge accordion; nor is
            // a horizontally anchored submenu shifted upward to fit the viewport.
            let skip_reverse_glue = preferred.is_horizontal()
                || (point.is_some()
                    && !height_keyframe_animation_active(el)
                    && !expand_measure_pending(el)
                    && floating_content_h(el).is_none())
                || (strategy_fixed && gap < 0.0);
            if !skip_reverse_glue {
                let animating_h = height_keyframe_animation_active(el);
                let entering = expand_enter_animation_active(el);
                // A committed above-edge must survive the whole open — including enter.
                // Dropping it while `entering` lets `compute_anchored_placement` flip to
                // the preferred below side as soon as the mid-enter clip/estimate fits
                // there (accordion grows upward → briefly downward → upward again).
                // Enter still re-seeds the edge *Y* from the live trigger below.
                let committed_above = reverse_expand_edge_y(el);
                let placed_above = committed_above.is_some() || y + 0.5 < tb[1];
                if placed_above {
                    // Keep the chosen side, but follow the live trigger. Exit
                    // has its own branch above and preserves the current height.
                    let edge = (tb[1] - gap).max(FLOATING_VIEWPORT_MARGIN);
                    el.set_attr(ATTR_FLOATING_ANCHOR_EDGE_Y, format!("{edge}"));
                    // Glue the trigger-facing edge to the *painted* height. `compute_anchored_placement`
                    // uses estimate.min(space_above); a stretched wrapper / viewport remainder
                    // slams `top` to the window margin (calendar month list floating over forms).
                    let mut visual_h = reverse_glue_visual_h(el, estimate_h, animating_h);
                    // During exit, preserve the committed top edge and shrink toward
                    // the trigger. Recomputing from the full content estimate causes
                    // a downward jump followed by an upward collapse.
                    if !entering && height_keyframe_animation_active(el) {
                        if let Some(top) = inset_length(el.style.inset.top) {
                            visual_h = (committed_above.unwrap_or(edge) - top).max(0.0);
                        }
                    }
                    // Idle Expand may write the place cap (16rem) onto max-height / CONTENT_H
                    // while the list is shorter. Exit must not grow past the already-glued
                    // silhouette — that slams `top` upward before accordion-up shrinks it.
                    if !entering {
                        if let Some(top) = inset_length(el.style.inset.top) {
                            let glued = (committed_above.unwrap_or(edge) - top).max(0.0);
                            if glued > 0.5 {
                                visual_h = visual_h.min(glued);
                            }
                        }
                    }
                    y = (edge - visual_h.min(max_h.max(0.0))).max(FLOATING_VIEWPORT_MARGIN);
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
                anchor_edge_y: reverse_expand_edge_y(el),
            };
            let place_key = floating_place_signal_key(&id);
            let place = use_signal(place_key.as_str(), FloatingPlace::default);
            let cur = place.peek();
            let signal_dirty = (cur.x - next.x).abs() > 0.5
                || (cur.y - next.y).abs() > 0.5
                || (cur.w - next.w).abs() > 0.5
                || (cur.max_h - next.max_h).abs() > 0.5
                || (cur.min_w - next.min_w).abs() > 0.5
                || cur.width_auto != next.width_auto
                || cur.anchor_edge_y != next.anchor_edge_y;
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
        apply_floating_menus(c, triggers, viewport, point_space, next_scroll, changed);
    }
}

/// 布局后根据触发器位置更新带 `jadix_floating`（或兼容 `jadix_select_*`）的浮层坐标。
/// 几何有变时返回 `true`（调用方应再跑一轮 layout）。
///
/// 滚动跟随时通常只改 `left/top`：调用方应 [`visit_floating_panels`] + `mark_element_layout_dirty`
/// 后走 **fast** relayout，禁止每帧 `build_and_layout` 全量重建 Taffy 节点。
pub fn sync_floating_placement(root: &mut Element, viewport_w: f32, viewport_h: f32) -> bool {
    let mut triggers = Vec::new();
    collect_focus_bounds(root, [0.0, 0.0], &mut triggers);
    let mut changed = false;
    apply_floating_menus(
        root,
        &triggers,
        [viewport_w, viewport_h],
        None,
        [0.0; 2],
        &mut changed,
    );
    changed
}

/// 遍历已标记的浮层面板（含兼容 `jadix_select_menu`）。
pub fn visit_floating_panels(root: &Element, mut f: impl FnMut(&Element)) {
    crate::core::tree::walk_elements(root, |el| {
        if floating_id_of(el).is_some() {
            f(el);
        }
    });
}

/// 标记 `jadix_portal=1`，布局时提升到根 portal 叠层（Solid `<Portal>` / `document.body`）。
pub fn mark_portal(mut el: Element) -> Element {
    el.set_attr("jadix_portal", "1");
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
                .or_else(|| el.attrs.get("jadix_select_trigger"))
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
/// 勿再包一层 [`portal_panel`]：嵌套 `jadix_portal` 会把内层面板 hoist 出居中舞台。
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

/// Keep a leftover portal/island host out of the parent's flex wrap.
///
/// A 0×0 `static` item still participates in `gap` (and can wrap onto a second
/// line, turning `gap` into row-gap). `position: absolute` drops it from flow.
/// Do not apply this to a `jadix_portal` panel itself — wrap a host around it so
/// hoist still lifts the panel, not a 0×0 leftover.
pub fn flow_hidden_portal_host(el: Element) -> Element {
    el.absolute()
        .w(0.0)
        .h(0.0)
        .min_w(0.0)
        .min_h(0.0)
        .pointer_events_box_none()
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
    flow_hidden_portal_host(Element::new().child(panel))
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
/// `jadix_portal`，把panel hoist 出舞台）。
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

/// Test / scroll-align helper: seed synced placement cap without a full layout pass.
#[cfg(test)]
pub(crate) fn seed_floating_place_max_h_for_test(id: &str, max_h: f32) {
    let place = FloatingPlace {
        x: 0.0,
        y: 0.0,
        w: 120.0,
        max_h,
        width_auto: false,
        min_w: 0.0,
        anchor_edge_y: None,
    };
    use_signal(
        floating_place_signal_key(id).as_str(),
        FloatingPlace::default,
    )
    .set_silent(place);
}

#[cfg(test)]
#[path = "tests/popup.rs"]
mod tests;
