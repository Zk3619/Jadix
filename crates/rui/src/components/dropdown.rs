//! # Dropdown
//!
//! 触发器 + 菜单的垂直堆叠壳，负责 overlay 栈登记与 `open` 信号。
//! 支持可选锚点定位（`anchor_focus_id` + `placement` → 统一 floating sync）。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!     dropdown("dd", move || trigger_el, move || menu_el)
//!         .open_signal(open)
//!         .anchor_focus_id("dd-trigger")
//!         .placement(PopupPlacement::BottomStart)
//! )
//! ```

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::app::reactive::reactive_view;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::utils::popup::{mark_floating, portal_panel, PopupPlacement};

pub struct DropdownBuilder {
    id: String,
    open: Option<Signal<bool>>,
    trigger: Box<dyn Fn() -> Element>,
    menu: Box<dyn Fn() -> Element>,
    anchor_focus_id: Option<String>,
    placement: PopupPlacement,
}

/// 触发器 + 菜单壳。Esc 经 overlay 栈关闭。见模块文档。
pub fn dropdown(
    id: impl Into<String>,
    trigger: impl Fn() -> Element + 'static,
    menu: impl Fn() -> Element + 'static,
) -> DropdownBuilder {
    DropdownBuilder {
        id: id.into(),
        open: None,
        trigger: Box::new(trigger),
        menu: Box::new(menu),
        anchor_focus_id: None,
        placement: PopupPlacement::BottomStart,
    }
}

impl DropdownBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    pub fn anchor_focus_id(mut self, fid: impl Into<String>) -> Self {
        self.anchor_focus_id = Some(fid.into());
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }
}

impl From<DropdownBuilder> for Element {
    fn from(builder: DropdownBuilder) -> Self {
        let DropdownBuilder {
            id,
            open,
            trigger,
            menu,
            anchor_focus_id,
            placement,
        } = builder;
        let scope = format!("{id}__dropdown_open");
        let open = open.unwrap_or_else(|| use_signal(scope.as_str(), || false));
        let view_id = format!("{id}__dropdown_view");
        let anchor = anchor_focus_id.unwrap_or_else(|| format!("{id}__trigger"));

        reactive_view(view_id, move || {
            let _ = open.get();
            let trigger_el = trigger();
            let mut menu_el = menu();
            if let Some(z) = portal_sync(&id, open, OverlayKind::Menu, OverlayMode::Stacked) {
                mark_floating(&mut menu_el, &id, &anchor, placement);
                menu_el = menu_el.absolute();
                menu_el = portal_panel(menu_el, z.panel);
            }
            vstack().child(trigger_el).child(menu_el).into()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropdown_builder_sets_anchor() {
        let open = use_signal("dd-test-open", || false);
        let el: Element = dropdown(
            "dd-test",
            || Element::new().focus_id("dd-test__trigger").text("t"),
            || Element::new().text("menu"),
        )
        .open_signal(open)
        .anchor_focus_id("dd-test__trigger")
        .into();
        assert_eq!(
            el.template_id.as_deref(),
            Some("dd-test__dropdown_view"),
            "dropdown must expose a stable reactive-view anchor"
        );
    }
}
