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
//! ## Usage
//! ```ignore
//! <Separator />
//! { Element::from(separator().vertical(true)) }
//! ```
//!
//! ## Notes
//! 垂直分割线时，父级宜 `items="stretch"` 且有明确高度，否则可能看不见。

use crate::components::base::SlotChrome;
use crate::core::tree::Element;
use crate::primitives::r#box::r#box;
use crate::tokens;

pub struct SeparatorBuilder {
    vertical: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn separator() -> SeparatorBuilder {
    SeparatorBuilder {
        vertical: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

crate::__component_contract! {
SeparatorProps [];
/// Separator component.
#[allow(non_snake_case)]
pub fn Separator(
default [] vertical: bool,
plain [] children: Option<Element>,
) -> Element {
    let mut builder = separator().vertical(vertical);
    if let Some(c) = children {
        builder = builder.child(c);
    }
    builder.into()
}
}

impl SeparatorBuilder {
    crate::impl_slot_chrome_methods!();

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
        builder.chrome.apply(el)
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;

    #[test]
    fn builder_style_reaches_separator() {
        let el: Element = separator().style(|b| b.p(8.0)).into();
        let pad = crate::layout::lp_length_px(el.style.padding.left).unwrap_or(0.0);
        assert!(
            (pad - 8.0).abs() < 0.01,
            "separator .style(|b| b.p(8)) must land on the line"
        );
    }
}
