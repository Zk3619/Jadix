//! 全屏模态遮罩原语（无头）：颜色 / 透明度 / 可选 backdrop 模糊 / 点击关闭。
//!
//! 不写入设计令牌默认色——由 Dialog 等产品层填入。Portal underlay 标记与
//! [`crate::utils::popup`] 的 arm/disarm 约定一致。

use crate::core::reactivity::Signal;
use crate::core::tree::Element;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::r#box;
use crate::utils::popup::{portal_panel, underlay_close_is_armed};

/// 构建全屏 underlay 遮罩。
pub struct OverlayBuilder {
    z: i32,
    color: [f32; 4],
    opacity: Option<f32>,
    backdrop_blur: f32,
    close_on_click: Option<Signal<bool>>,
}

/// 全屏遮罩；`z` 为 portal underlay 层。
pub fn overlay(z: i32) -> OverlayBuilder {
    OverlayBuilder {
        z,
        color: crate::style::TRANSPARENT,
        opacity: None,
        backdrop_blur: 0.0,
        close_on_click: None,
    }
}

impl OverlayBuilder {
    /// 遮罩 RGBA。若随后调用 [`Self::opacity`]，将覆盖 alpha。
    pub fn color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    /// 遮罩不透明度 0…1（写入 `color[3]`）。
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity.clamp(0.0, 1.0));
        self
    }

    /// Backdrop 模糊半径；`0` 关闭。RSX 无单位为 rem，Builder 数字为 px。
    pub fn backdrop_blur(mut self, blur: impl crate::core::tree::IntoF32) -> Self {
        self.backdrop_blur = blur.into_f32().max(0.0);
        self
    }

    /// 点击遮罩时将 `open` 设为 `false`（尊重 underlay arm）。
    pub fn close_on_click(mut self, open: Signal<bool>) -> Self {
        self.close_on_click = Some(open);
        self
    }
}

impl From<OverlayBuilder> for Element {
    fn from(builder: OverlayBuilder) -> Self {
        let OverlayBuilder {
            z,
            mut color,
            opacity,
            backdrop_blur,
            close_on_click,
        } = builder;
        if let Some(a) = opacity {
            color[3] = a;
        }

        // Underlay must claim its fullscreen silhouette (hoist keeps `auto` by default).
        // Always attach a click handler: either dismiss `open`, or a no-op swallow so
        // the dimmed area still eats presses instead of leaking to page content.
        let mut el: Element = if let Some(open) = close_on_click {
            interactive(r#box().into())
                .on_click(move || {
                    if !underlay_close_is_armed() {
                        return;
                    }
                    // Nested Select / Popover / Menu sits above this modal on the overlay
                    // stack — dismiss that first (including an already-exiting keep-alive
                    // layer started on pointer-down). Only close the dialog when nothing
                    // else is stacked on top.
                    if crate::app::portal_overlay::try_dismiss_outside_press() {
                        return;
                    }
                    open.set(false);
                })
                .into()
        } else {
            crate::utils::popup::swallow_clicks(r#box().into())
        };

        el = el
            .absolute()
            .inset(0.0)
            .w("100%")
            .h("100%")
            .bg(color)
            .backdrop_blur(backdrop_blur);
        el.set_attr(
            crate::layout::layout_tree::ATTR_PORTAL_KEY,
            format!("underlay:{z}"),
        );
        portal_panel(el, z)
    }
}

#[cfg(test)]
#[path = "tests/overlay.rs"]
mod tests;
