//! # Portal
//!
//! 将子树标记为 Portal：布局树每帧把带 `rui_portal=1` 的节点提升到根末尾，
//! 以便浮层绘制在上层。
//!
//! Select / Dialog / Popover / Dropdown 等组件浮层已自动 portal，并由
//! [`crate::app::portal_overlay`] 栈分配 z 与 Esc 顺序。本组件供应用手写自定义浮层。
//!
//! ## 用法
//! ```ignore
//! <Portal id="overlay-root">
//!   <Show when={open.get()}>
//!     <Rect absolute=true inset="0" z_index="900">…</Rect>
//!   </Show>
//! </Portal>
//! ```
//!
//! ## 注意
//! 只做提升标记；遮罩、居中、点击关闭、Esc 栈需自行实现或走 `portal_sync`。

use crate::core::tree::Element;
use rui_macros::component;

/// Portal 根：标记 `rui_portal`。见模块文档。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Portal(id: String, children: Option<Element>) -> Element {
    let mut inner = children.unwrap_or_else(Element::new);
    inner.template_id = Some(id);
    inner.set_attr("rui_portal", "1");
    inner
}
