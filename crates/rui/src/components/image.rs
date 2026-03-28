use crate::components::base::BaseComponentBuilder;
use crate::core::tree::Element;
use crate::primitives::r#box;
use rui_macros::component;

/// GPU 纹理矩形；需先 [`crate::queue_image_rgba8`] 上传 `texture` 同名键。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Image(
    id: String,
    texture: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let mut el: Element = r#box().into();
    el.template_id = Some(id);
    if !texture.is_empty() {
        el.image_texture = Some(texture);
    }
    if let Some(b) = base_builder {
        let base = BaseComponentBuilder::new(el);
        el = b(base).el;
    }
    if let Some(c) = children {
        el = el.child(c);
    }
    el
}
