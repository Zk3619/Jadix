//! 浮层几何与 Portal 辅助（**能力层 / 行为工具**，非样式组件）。
//!
//! 按 [`docs/COMPONENT_LIBRARY.md`](../../../../docs/COMPONENT_LIBRARY.md)：本模块只做
//! `mark_floating`、锚点 placement、viewport 钳位、underlay/portal 标记。
//! **禁止**写入某一组件的外观常量（如 Select `8rem`、Combobox `max-h-96`）；
//! 那些由 `components/*` 通过 attr / style 传入。

use std::cell::Cell;

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::overlay::overlay;

thread_local! {
    /// After opening an overlay, ignore underlay closes until the pointer is released.
    /// Prevents the opening press (or a same-cycle re-hit on the new underlay) from
    /// immediately dismissing the popover.
    static UNDERLAY_CLOSE_ARMED: Cell<bool> = const { Cell::new(true) };
}

/// Call when an overlay transitions to open (before/with `open.set(true)`).
pub fn disarm_underlay_close() {
    UNDERLAY_CLOSE_ARMED.set(false);
}

/// Call on primary-button release so the next outside click can dismiss.
pub fn arm_underlay_close() {
    UNDERLAY_CLOSE_ARMED.set(true);
}

/// Whether outside-press / underlay dismiss is currently allowed.
#[inline]
pub fn underlay_close_is_armed() -> bool {
    UNDERLAY_CLOSE_ARMED.get()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
}

impl PopupPlacement {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::BottomStart => "bottom-start",
            Self::BottomEnd => "bottom-end",
            Self::TopStart => "top-start",
            Self::TopEnd => "top-end",
        }
    }

    pub fn from_attr(s: &str) -> Self {
        match s {
            "bottom-end" => Self::BottomEnd,
            "top-start" => Self::TopStart,
            "top-end" => Self::TopEnd,
            _ => Self::BottomStart,
        }
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

/// 根据触发器矩形与视口，计算浮层左上角与可用高度（优先 BottomStart，空间不足则翻转）。
///
/// Always keeps [`FLOATING_VIEWPORT_MARGIN`] from the viewport edges and `gap` from the
/// trigger so the panel never sits flush against either.
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
    let gap = gap.max(0.0);
    let w = if menu_w > 0.0 { menu_w } else { tw };
    let prefer_bottom = matches!(
        preferred,
        PopupPlacement::BottomStart | PopupPlacement::BottomEnd
    );
    let prefer_end = matches!(
        preferred,
        PopupPlacement::BottomEnd | PopupPlacement::TopEnd
    );

    let space_below = (vh - margin - (ty + th) - gap).max(0.0);
    let space_above = (ty - margin - gap).max(0.0);
    let place_below = if prefer_bottom {
        space_below >= menu_h_estimate.min(80.0) || space_below >= space_above
    } else {
        space_above < menu_h_estimate.min(80.0) && space_below > space_above
    };

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

    let mut x = if prefer_end { tx + tw - w } else { tx };
    let x_max = (vw - margin - w).max(margin);
    if x + w > vw - margin {
        x = x_max;
    }
    if x < margin {
        x = margin;
    }

    (x, y, max_h, !place_below)
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
    match v {
        taffy::style::LengthPercentageAuto::Length(n) => Some(n),
        _ => None,
    }
}

fn dimension_length(v: taffy::style::Dimension) -> Option<f32> {
    match v {
        taffy::style::Dimension::Length(n) => Some(n),
        _ => None,
    }
}

/// Write absolute geometry onto a floating panel. Returns whether any field changed.
///
/// Capability only: applies placement numbers + attr-driven size constraints.
/// Does **not** invent product style defaults (those belong in `components/*`).
fn write_floating_place(el: &mut Element, place: FloatingPlace) -> bool {
    let mut changed = false;
    if inset_length(el.style.inset.left).is_none_or(|v| (v - place.x).abs() > 0.5) {
        el.style.inset.left = Element::into_margin_auto(place.x);
        changed = true;
    }
    if inset_length(el.style.inset.top).is_none_or(|v| (v - place.y).abs() > 0.5) {
        el.style.inset.top = Element::into_margin_auto(place.y);
        changed = true;
    }

    if place.width_auto {
        if !matches!(el.style.size.width, taffy::style::Dimension::Auto) {
            el.style.size.width = taffy::style::Dimension::Auto;
            changed = true;
        }
        let min_w = place.min_w.max(place.w).max(0.0);
        if min_w > 0.0
            && dimension_length(el.style.min_size.width).is_none_or(|v| (v - min_w).abs() > 0.5)
        {
            el.style.min_size.width = taffy::style::Dimension::Length(min_w);
            changed = true;
        }
    } else {
        if dimension_length(el.style.size.width).is_none_or(|v| (v - place.w).abs() > 0.5) {
            el.style.size.width = taffy::style::Dimension::Length(place.w);
            changed = true;
        }
        // Allow a larger min-width than the matched trigger (e.g. component sets
        // `rui_floating_min_w=trigger+1.75rem`).
        if place.min_w > place.w + 0.5
            && dimension_length(el.style.min_size.width)
                .is_none_or(|v| (v - place.min_w).abs() > 0.5)
        {
            el.style.min_size.width = taffy::style::Dimension::Length(place.min_w);
            changed = true;
        }
    }

    if let Some(raw) = el.attrs.get(ATTR_FLOATING_H) {
        let h = parse_css_px(raw, place.max_h).max(0.0);
        if dimension_length(el.style.size.height).is_none_or(|v| (v - h).abs() > 0.5) {
            el.style.size.height = taffy::style::Dimension::Length(h);
            changed = true;
        }
    } else {
        // Absolute floaters must not keep `h: 100%` of the portal/viewport (layout footgun).
        match el.style.size.height {
            taffy::style::Dimension::Percent(_) => {
                el.style.size.height = taffy::style::Dimension::Auto;
                changed = true;
            }
            taffy::style::Dimension::Length(_) | taffy::style::Dimension::Auto => {}
        }
    }

    // Viewport-available max, never raise above a smaller max already on the style/attr.
    let mut next_max = place.max_h;
    if let Some(existing) = dimension_length(el.style.max_size.height) {
        if existing > 0.0 {
            next_max = next_max.min(existing);
        }
    }
    if dimension_length(el.style.max_size.height).is_none_or(|v| (v - next_max).abs() > 0.5) {
        el.style.max_size.height = taffy::style::Dimension::Length(next_max);
        changed = true;
    }

    if let Some(raw) = el.attrs.get(ATTR_FLOATING_MIN_H) {
        let min_h = parse_css_px(raw, 0.0);
        if min_h > 0.0
            && dimension_length(el.style.min_size.height).is_none_or(|v| (v - min_h).abs() > 0.5)
        {
            el.style.min_size.height = taffy::style::Dimension::Length(min_h);
            changed = true;
        }
    }

    // Cap without overflow → children paint past the panel chrome.
    if el.style.overflow.y == taffy::style::Overflow::Visible {
        el.style.overflow.y = taffy::style::Overflow::Scroll;
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
    let place = use_signal(floating_place_signal_key(id).as_str(), FloatingPlace::default);
    let p = place.peek();
    // Default (0,0) before first sync is fine — sync will correct. Always write so absolute
    // panels never stay on `auto` insets.
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
fn collect_focus_bounds(el: &Element, scroll_acc: [f32; 2], out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = &el.focus_id {
        let b = el.layout_bounds;
        out.push((
            fid.clone(),
            [
                b[0] - scroll_acc[0],
                b[1] - scroll_acc[1],
                b[2],
                b[3],
            ],
        ));
    }
    let mut next_acc = scroll_acc;
    if el.style.overflow.x != taffy::style::Overflow::Visible {
        next_acc[0] += el.scroll_offset[0];
    }
    if el.style.overflow.y != taffy::style::Overflow::Visible {
        next_acc[1] += el.scroll_offset[1];
    }
    for c in &el.children {
        collect_focus_bounds(c, next_acc, out);
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
    if let (Some(id), Some(trigger_fid)) = (floating_id_of(el), floating_trigger_of(el)) {
        if let Some((_, tb)) = triggers.iter().find(|(f, _)| *f == trigger_fid) {
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
            } else if match_trigger {
                let w = tb[2].max(0.0);
                (w, false, attr_min_w.unwrap_or(w).max(w))
            } else {
                let floor = attr_min_w.unwrap_or(tb[2]).max(tb[2]).max(0.0);
                (floor, true, floor)
            };
            let preferred = PopupPlacement::from_attr(
                el.attrs
                    .get(ATTR_FLOATING_PLACEMENT)
                    .or_else(|| el.attrs.get("rui_select_placement"))
                    .map(|s| s.as_str())
                    .unwrap_or("bottom-start"),
            );
            let estimate_h = el.layout_bounds[3].max(32.0);
            let estimate_w = if width_auto {
                el.layout_bounds[2].max(menu_w).max(32.0)
            } else {
                menu_w.max(min_w).max(32.0)
            };
            let (x, y, available_max_h, _) = compute_anchored_placement(
                *tb,
                viewport,
                estimate_w,
                estimate_h,
                preferred,
                FLOATING_TRIGGER_GAP,
            );
            let max_h = el
                .attrs
                .get(ATTR_FLOATING_MAX_H)
                .or_else(|| el.attrs.get("rui_select_max_h"))
                .map(|s| available_max_h.min(parse_css_px(s, available_max_h)))
                .unwrap_or(available_max_h)
                .max(1.0);
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
pub fn sync_floating_placement(root: &mut Element, viewport_w: f32, viewport_h: f32) -> bool {
    let mut triggers = Vec::new();
    collect_focus_bounds(root, [0.0, 0.0], &mut triggers);
    let mut changed = false;
    apply_floating_menus(root, &triggers, [viewport_w, viewport_h], &mut changed);
    changed
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
    if triggers.is_empty() {
        return false;
    }
    path.iter().any(|e| {
        e.focus_id
            .as_ref()
            .is_some_and(|id| triggers.contains(id.as_str()))
    })
}

fn collect_floating_triggers(el: &Element, out: &mut std::collections::HashSet<String>) {
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
    for c in &el.children {
        collect_floating_triggers(c, out);
    }
}

/// 吞掉点击，避免穿透到 underlay（不 portal；用于已在 portal 舞台内的面板）。
pub fn swallow_clicks(inner: Element) -> Element {
    inner.on_click(|| {})
}

/// 面板包装：吞掉点击，避免穿透到 underlay；并 portal 提升。
///
/// 勿再包一层 [`portal_panel`]：嵌套 `rui_portal` 会把内层面板 hoist 出居中舞台。
pub fn panel_block_clicks(mut inner: Element, z: i32) -> Element {
    if inner
        .attrs
        .get(crate::layout::layout_tree::ATTR_PORTAL_KEY)
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        let key = floating_id_of(&inner)
            .map(|id| format!("panel:{id}"))
            .unwrap_or_else(|| format!("panel:{z}"));
        inner.set_attr(crate::layout::layout_tree::ATTR_PORTAL_KEY, key);
    }
    portal_panel(swallow_clicks(inner), z)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(el.style.overflow.y, taffy::style::Overflow::Scroll);
        match el.style.max_size.height {
            taffy::style::Dimension::Length(h) => assert!((h - 120.0).abs() < 0.1),
            other => panic!("expected max height length, got {other:?}"),
        }
        // Capability layer: no default min-height unless ATTR_FLOATING_MIN_H is set.
        assert!(
            dimension_length(el.style.min_size.height).unwrap_or(0.0) == 0.0,
            "popup must not invent a style min-height"
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
            matches!(el.style.size.height, taffy::style::Dimension::Auto),
            "ScrollArea h:100% must not stretch floating menus"
        );
        match el.style.max_size.height {
            taffy::style::Dimension::Length(h) => assert!((h - 192.0).abs() < 0.1),
            other => panic!("expected capped max height, got {other:?}"),
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
        match el.style.size.height {
            taffy::style::Dimension::Length(h) => assert!((h - 200.0).abs() < 0.1),
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
        match el.style.min_size.height {
            taffy::style::Dimension::Length(h) => assert!((h - 64.0).abs() < 0.1),
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
            matches!(el.style.size.width, taffy::style::Dimension::Auto),
            "independent panel must not force trigger width"
        );
        match el.style.min_size.width {
            taffy::style::Dimension::Length(w) => assert!((w - 120.0).abs() < 0.1),
            other => panic!("expected min-width = trigger, got {other:?}"),
        }
    }

    #[test]
    fn sync_floating_independent_width_is_content_auto() {
        let mut trigger = Element::new().focus_id("t-wide");
        trigger.layout_bounds = [20.0, 40.0, 80.0, 28.0];

        let mut panel = Element::new().absolute().w(300.0);
        mark_floating(&mut panel, "indep-menu", "t-wide", PopupPlacement::BottomStart);
        // match_trigger=0 and no rui_floating_w → content auto, min ≥ trigger
        panel.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "0");
        panel.layout_bounds = [0.0, 0.0, 220.0, 60.0];

        let mut root = Element::new().child(trigger).child(panel);
        assert!(sync_floating_placement(&mut root, 800.0, 600.0));
        let menu = &root.children[1];
        assert!(
            matches!(menu.style.size.width, taffy::style::Dimension::Auto),
            "panel width must stay independent of trigger"
        );
        match menu.style.min_size.width {
            taffy::style::Dimension::Length(w) => assert!((w - 80.0).abs() < 0.5),
            other => panic!("min-width should be trigger width, got {other:?}"),
        }
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
        let left = match menu.style.inset.left {
            taffy::style::LengthPercentageAuto::Length(v) => v,
            _ => panic!("expected Length left inset"),
        };
        let top = match menu.style.inset.top {
            taffy::style::LengthPercentageAuto::Length(v) => v,
            _ => panic!("expected Length top inset"),
        };
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
    fn sync_floating_reapplies_place_onto_fresh_panel() {
        // First open writes place into the signal + element.
        let mut trigger = Element::new().focus_id("t-reopen");
        trigger.layout_bounds = [80.0, 120.0, 100.0, 32.0];

        let mut panel = Element::new().absolute();
        mark_floating(&mut panel, "reopen-menu", "t-reopen", PopupPlacement::BottomStart);
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
}
