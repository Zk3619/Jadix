//! # Image
//!
//! GPU 纹理矩形：已上传的纹理键，或 `src`（本地路径 / `data:` URL / 可选 `http(s)`）解码后上传。
//!
//! ## Usage
//! ```ignore
//! jadix::queue_image_rgba8("demo", width, height, rgba_bytes);
//! <Image texture="demo" w="5.5rem" h="5.5rem" />
//!
//! <Image src="assets/photo.jpg" w="8rem" h="8rem" />
//! <Image src="data:image/svg+xml,..." w="2rem" h="2rem" />
//!
//! // `remote-asset` + `set_remote_asset_fetcher` (app owns HTTP; default builds leave the slot empty)
//! <Image src="https://example.com/photo.png" w="8rem" h="8rem" />
//!
//! // CSS `aspect-ratio` (effective when fixed width, another axis is auto; utility `aspect-video` can also be used)
//! <Image src="assets/photo.jpg" w="12rem" aspect_ratio="16/9" rounded="0.5rem" />
//! <Image src="assets/photo.jpg" w="12rem" aspect-ratio="1 / 1" />
//!
//! // HTML / Solid style alias (both macro and prelude available)
//! <Img src="assets/photo.jpg" w="8rem" h="8rem" />
//! <img src="assets/photo.jpg" w="8rem" h="8rem" />
//! ```
//!
//! JPEG/GIF/WebP/… decoded by built-in `image`; SVG requires `svg` feature.
//! `http(s)` `src` requires `remote-asset` plus [`crate::set_remote_asset_fetcher`]（手册 [`docs/ops/remote_asset.md`]）。
//! 与 [`crate::components::AspectRatio`] 容器二选一或组合：容器负责裁剪铺满，属性直接钉在 Image 上。

use crate::app::pending_images::{queue_image_from_src, texture_key_for_src};
use crate::components::base::{BaseComponentBuilder, SlotChrome};
use crate::core::tree::Element;
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box;

crate::__component_contract! {
ImageProps [base_builder];
/// GPU 纹理矩形。见模块文档。
#[allow(non_snake_case)]
pub fn Image(
identity [] id: String,
default [] texture: String,
default [] src: String,
base_builder [] base_builder: Option<Box<dyn FnOnce(BaseComponentBuilder) -> BaseComponentBuilder>>,
plain [] children: Option<Element>,
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
}

/// HTML / Solid style alias: Same as [`Image`] (`<Img>` / `Img(…)`).
pub use Image as Img;
/// Same as [`ImageProps`].
pub use ImageProps as ImgProps;

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct ImageBuilder {
    id: String,
    texture: String,
    src: String,
    chrome: SlotChrome,
    children: Vec<Element>,
}

pub fn image() -> ImageBuilder {
    ImageBuilder {
        id: String::new(),
        texture: String::new(),
        src: String::new(),
        chrome: SlotChrome::default(),
        children: Vec::new(),
    }
}

/// Prelude-safe alias of [`image`] (SVG/GPU primitive already owns `image()` on `jadix::prelude`).
pub fn ui_image() -> ImageBuilder {
    image()
}

impl ImageBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn texture(mut self, key: impl Into<String>) -> Self {
        self.texture = key.into();
        self
    }

    pub fn src(mut self, src: impl Into<String>) -> Self {
        self.src = src.into();
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl crate::core::tree::IntoElementList for ImageBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ImageBuilder> for Element {
    fn from(builder: ImageBuilder) -> Self {
        let children = if builder.children.is_empty() {
            None
        } else if builder.children.len() == 1 {
            builder.children.into_iter().next()
        } else {
            crate::components::base::packed_children(builder.children)
        };
        let el = Image(ImageProps {
            id: builder.id,
            texture: Some(builder.texture),
            src: Some(builder.src),
            children,
            base: None,
            base_builder: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn builder_src_and_style() {
        let el: Element = image()
            .src("assets/photo.jpg")
            .class("rounded-md")
            .style(|b| b.w("8rem").h("8rem"))
            .into();
        assert_eq!(el.image_texture.as_deref().unwrap().contains("photo"), true);
        assert_eq!(
            crate::layout::dim_length_px(el.style.size.width),
            crate::core::tree::IntoCssDim::into_css_dim("8rem")
                .as_immediate_dimension()
                .and_then(crate::layout::dim_length_px)
        );
    }
}
