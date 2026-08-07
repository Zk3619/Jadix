//! 浮层几何与 Portal 辅助（非组件）：锚点放置、遮罩、portal 标记、布局后 sync。

use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::r#box;

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

/// Attr keys for the unified floating placement engine.
pub const ATTR_FLOATING: &str = "rui_floating";
pub const ATTR_FLOATING_TRIGGER: &str = "rui_floating_trigger";
pub const ATTR_FLOATING_PLACEMENT: &str = "rui_floating_placement";
pub const ATTR_FLOATING_W: &str = "rui_floating_w";
pub const ATTR_FLOATING_MAX_H: &str = "rui_floating_max_h";
pub const ATTR_FLOATING_MATCH_TRIGGER: &str = "rui_floating_match_trigger";

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
    let w = if menu_w > 0.0 { menu_w } else { tw };
    let prefer_bottom = matches!(
        preferred,
        PopupPlacement::BottomStart | PopupPlacement::BottomEnd
    );
    let prefer_end = matches!(
        preferred,
        PopupPlacement::BottomEnd | PopupPlacement::TopEnd
    );

    let space_below = (vh - (ty + th) - gap).max(0.0);
    let space_above = (ty - gap).max(0.0);
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
        (ty - gap - menu_h_estimate.min(max_h)).max(0.0)
    };

    let mut x = if prefer_end { tx + tw - w } else { tx };
    if x + w > vw {
        x = (vw - w).max(0.0);
    }
    if x < 0.0 {
        x = 0.0;
    }

    (x, y, max_h, !place_below)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FloatingPlace {
    x: f32,
    y: f32,
    w: f32,
    max_h: f32,
}

impl Default for FloatingPlace {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 160.0,
            max_h: 256.0,
        }
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

fn collect_focus_bounds(el: &Element, out: &mut Vec<(String, [f32; 4])>) {
    if let Some(fid) = &el.focus_id {
        out.push((fid.clone(), el.layout_bounds));
    }
    for c in &el.children {
        collect_focus_bounds(c, out);
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
            let menu_w = if match_trigger {
                tb[2]
            } else if let Some(raw) = el
                .attrs
                .get(ATTR_FLOATING_W)
                .or_else(|| el.attrs.get("rui_select_menu_w"))
            {
                parse_css_px(raw, tb[2])
            } else {
                tb[2]
            };
            let max_h_cap = el
                .attrs
                .get(ATTR_FLOATING_MAX_H)
                .or_else(|| el.attrs.get("rui_select_max_h"))
                .map(|s| parse_css_px(s, 256.0))
                .unwrap_or(256.0);
            let preferred = PopupPlacement::from_attr(
                el.attrs
                    .get(ATTR_FLOATING_PLACEMENT)
                    .or_else(|| el.attrs.get("rui_select_placement"))
                    .map(|s| s.as_str())
                    .unwrap_or("bottom-start"),
            );
            let estimate_h = el.layout_bounds[3].max(32.0);
            let (x, y, max_h, _) =
                compute_anchored_placement(*tb, viewport, menu_w, estimate_h, preferred, 4.0);
            let max_h = max_h.min(max_h_cap).max(48.0);
            let w = menu_w.max(32.0);
            let next = FloatingPlace { x, y, w, max_h };
            let place_key = format!("{id}__floating_place");
            let place = use_signal(place_key.as_str(), FloatingPlace::default);
            let cur = place.peek();
            let dirty = (cur.x - next.x).abs() > 0.5
                || (cur.y - next.y).abs() > 0.5
                || (cur.w - next.w).abs() > 0.5
                || (cur.max_h - next.max_h).abs() > 0.5;
            if dirty {
                place.set_silent(next);
                el.style.inset.left = crate::core::tree::Element::into_margin_auto(x);
                el.style.inset.top = crate::core::tree::Element::into_margin_auto(y);
                el.style.size.width = taffy::style::Dimension::Length(w);
                el.style.max_size.height = taffy::style::Dimension::Length(max_h);
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
    collect_focus_bounds(root, &mut triggers);
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
pub fn underlay_close_on_click(open: Signal<bool>, z: i32) -> Element {
    let mut el: Element = interactive(r#box().into())
        .on_click(move || open.set(false))
        .into();
    el = el.absolute().inset(0.0).w("100%").h("100%");
    portal_panel(el, z)
}

/// 面板包装：吞掉点击，避免穿透到 underlay；并 portal 提升。
pub fn panel_block_clicks(mut inner: Element, z: i32) -> Element {
    inner = inner.on_click(|| {});
    portal_panel(inner, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_attr_roundtrip() {
        assert_eq!(
            PopupPlacement::from_attr(PopupPlacement::TopEnd.as_attr()),
            PopupPlacement::TopEnd
        );
    }

    #[test]
    fn compute_bottom_start_basic() {
        let (x, y, max_h, flipped) = compute_anchored_placement(
            [10.0, 10.0, 100.0, 24.0],
            [800.0, 600.0],
            100.0,
            120.0,
            PopupPlacement::BottomStart,
            4.0,
        );
        assert!(!flipped);
        assert_eq!(x, 10.0);
        assert_eq!(y, 38.0);
        assert!(max_h > 48.0);
    }
}
