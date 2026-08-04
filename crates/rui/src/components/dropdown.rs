//! # Dropdown
//!
//! 触发器 + 菜单的垂直堆叠壳，负责 overlay 栈登记与 `open` 信号。
//! **不内置锚点定位**；打开时菜单经 portal 提升。
//!
//! ## 用法
//! ```ignore
//! Element::from(
//!     dropdown("dd", trigger_el, menu_el).open_signal(open)
//! )
//! // trigger 的 on_click 里 toggle open；菜单用 <Show when={open.get()}> 控制
//! ```
//!
//! ## 注意
//! - 未传 `open` 时用 `{id}__dropdown_open`。
//! - 打开时菜单 z 由 [`crate::app::portal_overlay`] 分配并 portal。更完整的「触发器 + 定位菜单」优先看 [`crate::components::Select`]。

use crate::app::portal_overlay::{portal_sync, OverlayKind, OverlayMode};
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::Element;
use crate::primitives::r#box::vstack;
use crate::utils::popup::portal_panel;

pub struct DropdownBuilder {
    id: String,
    open: Option<Signal<bool>>,
    trigger: Element,
    menu: Element,
}

/// 触发器 + 菜单壳。Esc 经 overlay 栈关闭。见模块文档。
pub fn dropdown(
    id: impl Into<String>,
    trigger: impl Into<Element>,
    menu: impl Into<Element>,
) -> DropdownBuilder {
    DropdownBuilder {
        id: id.into(),
        open: None,
        trigger: trigger.into(),
        menu: menu.into(),
    }
}

impl DropdownBuilder {
    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }
}

impl From<DropdownBuilder> for Element {
    fn from(builder: DropdownBuilder) -> Self {
        let scope = format!("{}__dropdown_open", builder.id);
        let open = builder
            .open
            .unwrap_or_else(|| use_signal(scope.as_str(), || false));
        let _ = open.get();
        let mut menu = builder.menu;
        if let Some(z) = portal_sync(&builder.id, open, OverlayKind::Menu, OverlayMode::Stacked) {
            menu = portal_panel(menu, z.panel);
        }
        vstack().child(builder.trigger).child(menu).into()
    }
}
