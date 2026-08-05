//! # FocusTrap
//!
//! 激活时把 Tab 焦点环限制在指定 `focus_id` 列表内（模态对话框等）。
//! 通过全局信号 [`FOCUS_TRAP_RING_KEY`] / [`FOCUS_TRAP_OWNER_KEY`] 与运行时协作。
//!
//! ## 用法
//! ```ignore
//! <FocusTrap id="prim-trap" active={trap_on.get()} order="trap-a,trap-b">
//!   <Button id="trap-a">"A"</Button>
//!   <Button id="trap-b">"B"</Button>
//! </FocusTrap>
//! ```
//!
//! ## 注意
//! - `order` 为**逗号分隔的 focus_id**，须与子控件实际 `id` / `focus_id` 一致
//!   （Button 的 focus 通常就是其 `id`；Input 为 `input:{id}`）。
//! - 仅当前 `owner` 可清理陷阱；`template_id = ftrap:{id}`。

use crate::components::base::BaseComponentBuilder;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use rui_macros::component;

/// 逗号分隔的 `focus_id` 列表；激活时 Tab 仅在此集合内循环。
pub const FOCUS_TRAP_RING_KEY: &str = "__rui_focus_trap_ring";
/// 当前陷阱所有者（与 [`FocusTrap`] 的 `id` 一致），用于停用时的清理。
pub const FOCUS_TRAP_OWNER_KEY: &str = "__rui_focus_trap_owner";

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn FocusTrap(
    id: String,
    active: bool,
    order: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let ring = use_signal(FOCUS_TRAP_RING_KEY, String::new);
    let owner = use_signal(FOCUS_TRAP_OWNER_KEY, String::new);
    if active {
        ring.set(order.clone());
        owner.set(id.clone());
    } else if owner.peek() == id {
        ring.set(String::new());
        owner.set(String::new());
    }

    let mut el = children.unwrap_or_else(Element::new);
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    el.template_id = Some(format!("ftrap:{id}"));
    el
}
