//! # Image
//!
//! GPU 纹理矩形：已上传的纹理键，或 `src`（本地路径 / `data:` URL）解码后上传。
//!
//! ## 用法
//! ```ignore
//! rui::queue_image_rgba8("demo", width, height, rgba_bytes);
//! <Image texture="demo" w="5.5rem" h="5.5rem" />
//!
//! <Image src="assets/photo.jpg" w="8rem" h="8rem" />
//! <Image src="data:image/svg+xml,..." w="2rem" h="2rem" />
//!
//! // CSS `aspect-ratio`（定宽、另一轴 auto 时生效；亦可用 utility `aspect-video`）
//! <Image src="assets/photo.jpg" w="12rem" aspect_ratio="16/9" rounded="0.5rem" />
//! <Image src="assets/photo.jpg" w="12rem" aspect-ratio="1 / 1" />
//!
//! // HTML / Solid 风格别名（宏与 prelude 均可用）
//! <Img src="assets/photo.jpg" w="8rem" h="8rem" />
//! <img src="assets/photo.jpg" w="8rem" h="8rem" />
//! ```
//!
//! JPEG/GIF/WebP 需 `images`（或分项 feature）；SVG 需 `svg`。PNG/ICO/BMP 默认可用。
//! 与 [`crate::components::AspectRatio`] 容器二选一或组合：容器负责裁剪铺满，属性直接钉在 Image 上。

use crate::app::pending_images::{queue_image_from_src, texture_key_for_src};
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
    src: String,
    base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
    children: Option<Element>,
) -> Element {
    let mut el: Element = r#box().into();
    el.template_id = Some(id);
    if !src.is_empty() {
        let key = if texture.is_empty() {
            texture_key_for_src(&src)
        } else {
            texture.clone()
        };
        queue_image_from_src(&key, &src);
        el.image_texture = Some(key);
    } else if !texture.is_empty() {
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

/// HTML / Solid 风格别名：与 [`Image`] 相同（`<Img>` / `Img(…)`）。
pub use Image as Img;
/// 与 [`ImageProps`] 相同。
pub use ImageProps as ImgProps;
