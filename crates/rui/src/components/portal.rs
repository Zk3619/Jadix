use crate::core::tree::Element;
use rui_macros::component;

/// 将子树标记为 Portal：每帧提升到根节点末尾绘制（浮层/Modal）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Portal(id: String, children: Option<Element>) -> Element {
    let mut inner = children.unwrap_or_else(|| Element::new());
    inner.template_id = Some(id);
    inner.set_attr("rui_portal", "1");
    inner
}
