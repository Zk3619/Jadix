//! # Separator
//!
//! 1px 分割线：默认水平满宽；`.vertical(true)` 时垂直满高。颜色取 [`crate::tokens::border`]。
//!
//! 对齐 solid-ui / ShadCN：`shrink-0 bg-border` + 水平 `h-px w-full` / 垂直 `w-px h-full`。
//!
//! 菜单 / popover（父级 `p-1`）要贴边出血时，用框架级 [`Element::menu_bleed`] /
//! [`Element::bleed_x`]（任意节点），或本模块 [`SeparatorBuilder::menu_bleed`]：
//! ```ignore
//! Element::from(separator()).menu_bleed()
//! separator().menu_bleed()
//! some_row.bleed_x("0.25rem") // 任意组件取消父级水平 padding
//! ```
//! 不要写 `w("100%").mx("-…")` — 百分比宽 + 负 `mx` 在 flex 下会左右不对称。
//!
//! ## 用法
//! ```ignore
//! <Separator />
//! { Element::from(separator().vertical(true)) }
//! ```
//!
//! ## 注意
//! 垂直分割线时，父级宜 `items="stretch"` 且有明确高度，否则可能看不见。

use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::tokens;
use rui_macros::component;

pub struct SeparatorBuilder {
    vertical: bool,
    children: Vec<Element>,
}

pub fn separator() -> SeparatorBuilder {
    SeparatorBuilder {
        vertical: false,
        children: Vec::new(),
    }
}

/// Separator component.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Separator(vertical: bool, children: Option<Element>) -> Element {
    let mut builder = separator().vertical(vertical);
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}

impl SeparatorBuilder {
    pub fn vertical(mut self, v: bool) -> Self {
        self.vertical = v;
        self
    }

    pub fn child(mut self, c: Element) -> Self {
        self.children.push(c);
        self
    }

    pub fn children(mut self, c: Vec<Element>) -> Self {
        self.children.extend(c);
        self
    }

    /// ShadCN 菜单分割线：默认满宽线 + [`Element::menu_bleed`]（`-mx-1 my-1`）。
    pub fn menu_bleed(self) -> Element {
        Element::from(self).menu_bleed()
    }
}

impl crate::core::tree::IntoElementList for SeparatorBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<SeparatorBuilder> for Element {
    fn from(builder: SeparatorBuilder) -> Self {
        // solid-ui / ShadCN: shrink-0 bg-border; horizontal h-px w-full; vertical w-px h-full
        let mut el: Element = if builder.vertical {
            r#box().w("1px").h("100%").min_h("1rem").into()
        } else {
            r#box().h("1px").w("100%").min_w("1rem").into()
        };
        el = el.bg(tokens::border()).flex_shrink(0.0);
        let _ = builder.children;
        el
    }
}
