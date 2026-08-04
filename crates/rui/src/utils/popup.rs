//! 浮层几何与 Portal 辅助（非组件）：锚点放置、遮罩、portal 标记。

use crate::core::reactivity::Signal;
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
    let prefer_bottom = matches!(preferred, PopupPlacement::BottomStart | PopupPlacement::BottomEnd);
    let prefer_end = matches!(preferred, PopupPlacement::BottomEnd | PopupPlacement::TopEnd);

    let space_below = (vh - (ty + th) - gap).max(0.0);
    let space_above = (ty - gap).max(0.0);
    let place_below = if prefer_bottom {
        space_below >= menu_h_estimate.min(80.0) || space_below >= space_above
    } else {
        space_above < menu_h_estimate.min(80.0) && space_below > space_above
    };

    let max_h = if place_below { space_below } else { space_above }.max(48.0);
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

/// 标记 `rui_portal=1`，布局时提升到根末尾。
pub fn mark_portal(mut el: Element) -> Element {
    el.set_attr("rui_portal", "1");
    el
}

/// 设置 z-index 并 portal 提升（浮层面板）。
pub fn portal_panel(mut inner: Element, z: i32) -> Element {
    inner.style.z_index = z;
    mark_portal(inner)
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
