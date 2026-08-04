//! Headless popup helpers: Escape 关闭栈、`open` 信号注册、全屏点击 underlay。
use std::cell::RefCell;

use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::primitives::interactive::interactive;

thread_local! {
    /// Last-registered popup `open` signal (Escape closes the most recently registered).
    static LAST_POPUP_OPEN: RefCell<Option<Signal<bool>>> = RefCell::new(None);
}

/// Call when building a dialog/dropdown so [`try_close_top_popup`] can clear `open`.
pub fn register_last_popup_for_escape(open: Signal<bool>) {
    LAST_POPUP_OPEN.with(|s| {
        *s.borrow_mut() = Some(open);
    });
}

/// Sets `open` to false if it was true. Returns whether a popup was closed.
pub fn try_close_top_popup() -> bool {
    LAST_POPUP_OPEN.with(|s| {
        let borrow = s.borrow();
        let Some(sig) = borrow.as_ref() else {
            return false;
        };
        if sig.get() {
            sig.set(false);
            true
        } else {
            false
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PopupPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
}

/// 根据触发器矩形与视口，计算浮层左上角与可用高度（优先右下 / BottomStart，空间不足则翻转）。
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

/// Full-viewport underlay: click closes popup. Absolute + portal so it does not affect layout.
pub fn underlay_close_on_click(open: Signal<bool>) -> Element {
    let mut el: Element = interactive(r#box().into())
        .on_click(move || open.set(false))
        .into();
    el = el.absolute().inset(0.0).w("100%").h("100%");
    el.style.z_index = 900;
    el.set_attr("rui_portal", "1");
    el
}

/// Panel wrapper that stops hit-test from reaching underlay (`on_click` noop).
pub fn panel_block_clicks(mut inner: Element, z: i32) -> Element {
    inner.style.z_index = z;
    inner.on_click(|| {})
}
