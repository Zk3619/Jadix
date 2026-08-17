//! # Sheet
//!
//! 侧滑面板（产品 Sheet，与 Navigator `Presentation::Sheet` 转场栈无关）。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::{r#box, vstack};
use crate::tokens;
use crate::utils::popup::{
    mount_modal_layer, mount_viewport_portal_stage, swallow_clicks, underlay_close_on_click,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SheetSide {
    #[default]
    Right,
    Left,
}

pub struct SheetBuilder {
    id: String,
    open: Option<Signal<bool>>,
    side: SheetSide,
    width: String,
    body: Option<Box<dyn Fn() -> Element>>,
}

/// 侧栏 Sheet；Esc / 遮罩关闭。
pub fn sheet(id: impl Into<String>) -> SheetBuilder {
    SheetBuilder {
        id: id.into(),
        open: None,
        side: SheetSide::Right,
        width: "20rem".into(),
        body: None,
    }
}

impl SheetBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
    pub fn side(mut self, side: SheetSide) -> Self {
        self.side = side;
        self
    }
    pub fn w(mut self, w: impl Into<String>) -> Self {
        self.width = w.into();
        self
    }
    pub fn child(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.body = Some(Box::new(build));
        self
    }
}

impl From<SheetBuilder> for Element {
    fn from(builder: SheetBuilder) -> Self {
        let SheetBuilder {
            id,
            open,
            side,
            width,
            body,
        } = builder;
        let open =
            open.unwrap_or_else(|| use_signal(format!("{id}__sheet_open").as_str(), || false));
        let view_id = format!("{id}__sheet_view");

        reactive_view(view_id, move || {
            let _ = open.get();
            let Some(oz) = portal_sync(&id, open, OverlayKind::Modal, OverlayMode::Stacked) else {
                return Element::new();
            };
            if !open.peek() {
                return Element::new();
            }

            let mut panel: Element = vstack()
                .w(width.as_str())
                .h("100%")
                .p("1rem")
                .gap("0.75rem")
                .into();
            panel = panel.bg(tokens::background());
            panel.style.border_width = 1.0;
            panel.style.border_color = tokens::border();
            if let Some(build) = &body {
                panel = panel.child(build());
            }
            panel = panel.absolute().top(0.0);
            match side {
                SheetSide::Right => {
                    panel = panel.right(0.0);
                }
                SheetSide::Left => {
                    panel = panel.left(0.0);
                }
            }

            panel = swallow_clicks(panel);

            let stage: Element = r#box().inset(0.0).w("100%").h("100%").child(panel).into();
            mount_modal_layer(
                underlay_close_on_click(open, oz.underlay),
                mount_viewport_portal_stage(stage, oz.panel),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::popup::{panel_block_clicks, portal_panel};

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
    fn nested_portal_panel_is_the_old_sheet_bug() {
        let panel = panel_block_clicks(Element::new().text("sheet"), 1001);
        let stage = portal_panel(Element::new().child(panel), 1001);
        assert!(
            portal_nests_portal(&stage),
            "portal stage wrapping a panel_block_clicks child nests rui_portal"
        );
    }
}
