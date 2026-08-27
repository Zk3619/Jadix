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
        color: [0.0, 0.0, 0.0, 0.0],
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

    /// Backdrop 模糊半径（逻辑 px）；`0` 关闭。
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
                    // stack — dismiss that first; only close the dialog when nothing else
                    // is stacked on top.
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
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, Signal};
    use crate::layout::layout_tree::ATTR_PORTAL_KEY;

    #[test]
    fn overlay_prefers_nested_menu_dismiss_before_closing_modal() {
        use crate::app::portal_overlay::{
            clear_overlay_stack, portal_sync, OverlayKind, OverlayMode,
        };
        use crate::utils::popup::arm_underlay_close;

        clear_overlay_stack();
        let dialog = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("dlg", dialog, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        arm_underlay_close();

        let underlay: Element = overlay(1000).close_on_click(dialog).into();
        if let Some(cb) = &underlay.on_click {
            cb.borrow_mut()();
        }
        assert!(
            !menu.peek(),
            "underlay click must dismiss nested menu first"
        );
        assert!(dialog.peek(), "dialog must stay open while menu was on top");
        clear_overlay_stack();
    }

    #[test]
    fn overlay_sets_underlay_portal_key_and_blur() {
        let (_owner, el) = create_root(|| {
            let open = Signal::new(true);
            Element::from(
                overlay(1200)
                    .color([0.0, 0.0, 0.0, 0.5])
                    .backdrop_blur(12.0)
                    .close_on_click(open),
            )
        });
        assert_eq!(
            el.attrs.get(ATTR_PORTAL_KEY).map(String::as_str),
            Some("underlay:1200")
        );
        assert!((el.style.backdrop_blur - 12.0).abs() < 0.01);
        assert!((el.style.bg_color[3] - 0.5).abs() < 0.01);
        assert!(el.on_click.is_some());
    }
}
