//! # Image
//!
//! GPU 纹理矩形：把已上传的纹理键画成一块可布局的矩形（可圆角等）。
//!
//! ## 用法
//! ```ignore
//! rui::queue_image_rgba8("demo", width, height, rgba_bytes);
//! // …
//! <Image id="demo-img" texture="demo" w="5.5rem" h="5.5rem" rounded="0.375rem" />
//! ```
//!
//! ## 注意
//! - 须先 [`crate::queue_image_rgba8`]（或等价上传），且 `texture` 与上传键一致。
//! - 尺寸 / 圆角等走 `base`；设置 `el.image_texture` 供渲染器取样。

use crate::components::base::BaseComponentBuilder;
use crate::core::tree::Element;
use crate::primitives::r#box;
use rui_macros::component;

/// GPU 纹理矩形。见模块文档。
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
